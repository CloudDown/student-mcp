//! Notification-area icon so closing the browser tab still shows the app is up.
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Open,
    Quit,
}

/// Handle kept alive for the process lifetime of `student-mcp up`.
pub struct Tray {
    receiver: Receiver<Command>,
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "windows", target_os = "macos")),
        allow(dead_code)
    )]
    _native: Option<NativeTray>,
}

impl Tray {
    /// Best-effort: if the desktop has no tray host, the app still runs.
    pub fn start() -> Self {
        let (sender, receiver) = channel();
        let native = match NativeTray::new(sender) {
            Ok(native) => Some(native),
            Err(message) => {
                eprintln!("System tray unavailable: {message}");
                None
            }
        };
        Self {
            receiver,
            _native: native,
        }
    }

    pub fn poll(&self) -> Option<Command> {
        self.receiver.try_recv().ok()
    }
}

/// Graduation smiley (flat emoji style), 64×64 RGBA.
const ICON_SIZE: u32 = 64;
const ICON_RGBA: &[u8] = include_bytes!("resources/assets/app-icon-64.rgba");

fn icon_rgba() -> Vec<u8> {
    debug_assert_eq!(ICON_RGBA.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    ICON_RGBA.to_vec()
}

struct Events(Sender<Command>);

impl Events {
    fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }
}

#[cfg(target_os = "linux")]
use linux::NativeTray;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use ksni::blocking::TrayMethods;

    pub(super) struct NativeTray(ksni::blocking::Handle<LinuxTray>);

    impl NativeTray {
        pub(super) fn new(sender: Sender<Command>) -> Result<Self, String> {
            LinuxTray(Events(sender))
                .spawn()
                .map(Self)
                .map_err(|error| error.to_string())
        }
    }

    impl Drop for NativeTray {
        fn drop(&mut self) {
            self.0.shutdown().wait();
        }
    }

    struct LinuxTray(Events);

    impl ksni::Tray for LinuxTray {
        const MENU_ON_ACTIVATE: bool = true;

        fn id(&self) -> String {
            "student-mcp".into()
        }
        fn title(&self) -> String {
            "student-mcp".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let mut argb = icon_rgba();
            for pixel in argb.as_chunks_mut::<4>().0 {
                pixel.rotate_right(1);
            }
            vec![ksni::Icon {
                width: ICON_SIZE as i32,
                height: ICON_SIZE as i32,
                data: argb,
            }]
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "student-mcp".into(),
                description: "Running — open configuration or quit".into(),
                ..Default::default()
            }
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: "Open".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Command::Open)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Close".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Command::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ksni::Tray as _;

        #[test]
        fn menu_opens_configuration_and_quits() {
            assert!(<LinuxTray as ksni::Tray>::MENU_ON_ACTIVATE);
            let (sender, receiver) = channel();
            let mut tray = LinuxTray(Events(sender));
            let menu = tray.menu();
            for (item, label, command) in [
                (&menu[0], "Open", Command::Open),
                (&menu[2], "Close", Command::Quit),
            ] {
                let ksni::MenuItem::Standard(item) = item else {
                    panic!("expected standard menu item");
                };
                assert_eq!(item.label, label);
                (item.activate)(&mut tray);
                assert_eq!(receiver.try_recv().unwrap(), command);
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
use desktop::NativeTray;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod desktop {
    use super::*;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        Icon, TrayIcon, TrayIconBuilder,
    };

    pub(super) struct NativeTray {
        _icon: TrayIcon,
    }

    impl NativeTray {
        pub(super) fn new(sender: Sender<Command>) -> Result<Self, String> {
            let menu = Menu::new();
            let open = MenuItem::new("Open", true, None);
            let quit = MenuItem::new("Close", true, None);
            menu.append_items(&[&open, &quit])
                .map_err(|error| error.to_string())?;
            let icon = Icon::from_rgba(icon_rgba(), ICON_SIZE, ICON_SIZE)
                .map_err(|error| error.to_string())?;
            let mut builder = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("student-mcp — running")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(true);
            #[cfg(target_os = "macos")]
            {
                builder = builder.with_icon_as_template(true);
            }
            let icon = builder.build().map_err(|error| error.to_string())?;
            let open_id = open.id().clone();
            let quit_id = quit.id().clone();
            let menu_events = Events(sender.clone());
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if event.id == open_id {
                    menu_events.send(Command::Open);
                }
                if event.id == quit_id {
                    menu_events.send(Command::Quit);
                }
            }));
            Ok(Self { _icon: icon })
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
struct NativeTray;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
impl NativeTray {
    fn new(_: Sender<Command>) -> Result<Self, String> {
        Err("System tray is not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_icon_is_nonempty_rgba_with_transparent_background() {
        let pixels = icon_rgba();
        assert_eq!(pixels.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
        // Yellow face present (graduation smiley).
        assert!(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 200 && p[1] > 150 && p[2] < 40 && p[3] == 255));
    }
}
