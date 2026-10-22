//! Window lifetime is separate from the MCP/tunnel lifetime. Native callbacks
//! only wake egui and enqueue commands; quitting follows the normal Drop path.
use eframe::egui::{Context, ViewportCommand};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Open,
    Quit,
    Unavailable,
}

#[derive(Clone)]
struct Events {
    sender: Sender<Event>,
    ctx: Context,
}

impl Events {
    fn send(&self, event: Event) {
        let _ = self.sender.send(event);
        self.ctx.request_repaint(); // Also wakes a hidden, otherwise idle window.
    }
}

pub struct Background {
    enabled: bool,
    attempted: bool,
    native: Option<NativeTray>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    hidden: bool,
    quitting: bool,
}

// Disabled by default so headless tests and UI previews cannot create native UI.
impl Default for Background {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Self {
            enabled: false,
            attempted: false,
            native: None,
            sender,
            receiver,
            hidden: false,
            quitting: false,
        }
    }
}

impl Background {
    pub fn enabled() -> Self {
        Self {
            enabled: supports_background_window(),
            ..Self::default()
        }
    }

    pub fn available(&self) -> bool {
        self.native.as_ref().is_some_and(NativeTray::available)
    }

    pub fn shutdown(&mut self) {
        self.quitting = true;
        self.native = None;
    }

    /// Called from the first UI update, not the app constructor: on macOS the
    /// AppKit event loop must already be running when the status item is created.
    /// Returns true when the caller should stop the tunnel and finish quitting.
    pub fn update(&mut self, ctx: &Context) -> bool {
        if self.enabled && !self.attempted {
            self.attempted = true;
            match NativeTray::new(Events {
                sender: self.sender.clone(),
                ctx: ctx.clone(),
            }) {
                Ok(native) => self.native = Some(native),
                Err(message) => eprintln!(
                    "System tray unavailable: {message}. Closing the window will quit the app."
                ),
            }
        }
        let lost = self
            .native
            .as_ref()
            .is_some_and(|native| !native.available());
        if lost {
            self.apply(Event::Unavailable, ctx);
        }
        while let Ok(event) = self.receiver.try_recv() {
            self.apply(event, ctx);
        }
        if ctx.input(|input| input.viewport().close_requested())
            && should_hide_on_close(self.available(), self.quitting)
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.hidden = true;
        }
        // Notice a disconnected Linux tray even while there are no MCP jobs.
        if self.hidden {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        self.quitting
    }

    fn apply(&mut self, event: Event, ctx: &Context) {
        match event {
            Event::Open if !self.quitting => self.show(ctx),
            Event::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Event::Unavailable => {
                self.native = None;
                if self.hidden && !self.quitting {
                    self.show(ctx);
                }
            }
            Event::Open => {}
        }
    }

    fn show(&mut self, ctx: &Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }
}

pub fn supports_background_window() -> bool {
    // Winit cannot hide or restore Wayland windows. The native GUI selects X11
    // (including XWayland) when available, so tray restoration remains reliable.
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty())
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

fn should_hide_on_close(tray_available: bool, quitting: bool) -> bool {
    tray_available && !quitting
}

// A small transparent S monogram, bundled in code, independent of theme/fonts.
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

#[cfg(target_os = "linux")]
use linux::NativeTray;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use ksni::blocking::TrayMethods;

    pub(super) struct NativeTray(ksni::blocking::Handle<LinuxTray>);

    impl NativeTray {
        pub(super) fn new(events: Events) -> Result<Self, String> {
            // Unlike assuming SNI support, spawn fails when the desktop has no
            // tray host. Never hide the user's only window in that situation.
            LinuxTray(events)
                .spawn()
                .map(Self)
                .map_err(|error| error.to_string())
        }
        pub(super) fn available(&self) -> bool {
            !self.0.is_closed()
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
            self.0.send(Event::Open);
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
                description: "Running in the background".into(),
                ..Default::default()
            }
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: "Open student-mcp".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Event::Open)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Quit student-mcp".into(),
                    activate: Box::new(|tray: &mut Self| tray.0.send(Event::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
        fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
            self.0.send(Event::Unavailable);
            false // Restore the window instead of running invisibly without a tray.
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
        pub(super) fn new(events: Events) -> Result<Self, String> {
            let menu = Menu::new();
            let open = MenuItem::new("Open student-mcp", true, None);
            let quit = MenuItem::new("Quit student-mcp", true, None);
            menu.append_items(&[&open, &quit])
                .map_err(|error| error.to_string())?;
            let icon = Icon::from_rgba(icon_rgba(), 32, 32).map_err(|error| error.to_string())?;
            let mut builder = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("student-mcp — running in the background")
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
            let menu_events = events.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if event.id == open_id {
                    menu_events.send(Event::Open);
                }
                if event.id == quit_id {
                    menu_events.send(Event::Quit);
                }
            }));
            #[cfg(target_os = "windows")]
            tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
                if matches!(
                    event,
                    tray_icon::TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    }
                ) {
                    events.send(Event::Open);
                }
            }));
            Ok(Self { _icon: icon })
        }
        pub(super) fn available(&self) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hides_only_with_a_tray_and_never_when_quitting() {
        assert!(should_hide_on_close(true, false));
        assert!(!should_hide_on_close(false, false));
        assert!(!should_hide_on_close(true, true));
        assert!(!should_hide_on_close(false, true));
    }

    #[test]
    fn reopen_and_tray_loss_restore_the_window_and_quit_is_final() {
        let ctx = Context::default();
        let mut background = Background::default();
        background.hidden = true;
        background.apply(Event::Open, &ctx);
        assert!(!background.hidden);
        background.hidden = true;
        background.apply(Event::Unavailable, &ctx);
        assert!(!background.hidden);
        background.apply(Event::Quit, &ctx);
        assert!(background.quitting);
        background.hidden = true;
        background.apply(Event::Open, &ctx);
        assert!(background.hidden); // A queued click cannot cancel quitting.
    }

    #[test]
    fn native_callbacks_queue_commands_without_exiting_the_process() {
        let mut background = Background::default();
        let events = Events {
            sender: background.sender.clone(),
            ctx: Context::default(),
        };
        events.send(Event::Quit);
        assert!(!background.quitting);
        assert!(background.update(&events.ctx));
    }

    #[test]
    fn bundled_icon_is_nonempty_rgba_with_transparent_background() {
        let pixels = icon_rgba();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
    }
}
