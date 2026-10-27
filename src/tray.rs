//! Notification-area icon so closing the browser tab still shows the app is up.
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

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
        match self.receiver.try_recv() {
            Ok(command) => Some(command),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
}

/// Small transparent S monogram, independent of theme fonts.
fn icon_rgba() -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for (y, row) in [
        "01111", "10000", "10000", "01110", "00001", "00001", "11110",
    ]
    .iter()
    .enumerate()
    {
        for (x, value) in row.bytes().enumerate() {
            if value == b'1' {
                for dy in 0..3 {
                    for dx in 0..3 {
                        let offset = ((5 + y * 3 + dy) * 32 + 8 + x * 3 + dx) * 4;
                        pixels[offset..offset + 4].copy_from_slice(&[0, 102, 204, 255]);
                    }
                }
            }
        }
    }
    pixels
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
        fn id(&self) -> String {
            "student-mcp".into()
        }
        fn title(&self) -> String {
            "student-mcp".into()
        }
        fn activate(&mut self, _: i32, _: i32) {
            self.0.send(Command::Open);
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let mut argb = icon_rgba();
            for pixel in argb.chunks_exact_mut(4) {
                pixel.rotate_right(1);
            }
            vec![ksni::Icon {
                width: 32,
                height: 32,
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
                    label: "Open student-mcp".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Command::Open)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Quit student-mcp".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Command::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
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
            let open = MenuItem::new("Open student-mcp", true, None);
            let quit = MenuItem::new("Quit student-mcp", true, None);
            menu.append_items(&[&open, &quit])
                .map_err(|error| error.to_string())?;
            let icon = Icon::from_rgba(icon_rgba(), 32, 32).map_err(|error| error.to_string())?;
            let mut builder = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("student-mcp — running")
                .with_menu(Box::new(menu));
            #[cfg(target_os = "macos")]
            {
                builder = builder.with_icon_as_template(true);
            }
            #[cfg(target_os = "windows")]
            {
                builder = builder.with_menu_on_left_click(false);
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
            #[cfg(target_os = "windows")]
            {
                let click = Events(sender);
                tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
                    if matches!(
                        event,
                        tray_icon::TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            button_state: tray_icon::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        click.send(Command::Open);
                    }
                }));
            }
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
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
    }
}
