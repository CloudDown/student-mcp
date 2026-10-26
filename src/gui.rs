use crate::background::Background;
use crate::config::{mcp_endpoint, Gateway, Settings};
use crate::logos::Logos;
use crate::tunnel::{State as TunnelState, Tunnel};
use eframe::egui::{
    self, pos2, Align2, Button, CentralPanel, Color32, Context, CursorIcon, FontId, Layout, Rect,
    RichText, Sense, Stroke, TextEdit, TextureHandle, Ui, Vec2,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

// Quiet macOS-style surfaces. Blue means action; green is reserved for status.
const CANVAS: Color32 = Color32::from_rgb(245, 245, 247);
const INK: Color32 = Color32::from_rgb(29, 29, 31);
const SUBTLE: Color32 = Color32::from_rgb(105, 105, 112);
const MUTED: Color32 = Color32::from_rgb(134, 134, 139);
const BORDER: Color32 = Color32::from_rgb(222, 222, 227);
const HOVER: Color32 = Color32::from_rgb(239, 239, 242);
const PRESSED: Color32 = Color32::from_rgb(230, 230, 235);
const FIELD: Color32 = Color32::WHITE;
const BLUE: Color32 = Color32::from_rgb(0, 102, 204);
const GREEN: Color32 = Color32::from_rgb(36, 125, 61);
const GREEN_SOFT: Color32 = Color32::from_rgb(232, 245, 235);
const RED: Color32 = Color32::from_rgb(193, 42, 50);
const RED_SOFT: Color32 = Color32::from_rgb(253, 237, 238);
const AMBER: Color32 = Color32::from_rgb(137, 95, 10);

fn surface_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 3],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(12),
    }
}

fn focus_ring(ui: &Ui, rect: Rect, radius: u8) {
    ui.painter().rect_stroke(
        rect.expand(3.0),
        radius + 3,
        Stroke::new(2.0_f32, BLUE.gamma_multiply(0.45)),
        egui::StrokeKind::Outside,
    );
}

const MOODLE_DEFAULT_URL: &str = "https://moodle.polymtl.ca";

fn form_bounds(bounds: Rect) -> Rect {
    let size = Vec2::new(bounds.width().min(372.0), bounds.height());
    Rect::from_center_size(bounds.center(), size)
}

/// Centered green status line: a small dot followed by “Connected”.
fn connected_badge(ui: &mut Ui) {
    let galley =
        ui.painter()
            .layout_no_wrap("Connected".to_owned(), FontId::proportional(12.0), GREEN);
    let (dot, gap) = (9.0, 7.0);
    let size = Vec2::new(dot + gap + galley.size().x, 16.0);
    let rect = ui.allocate_exact_size(size, Sense::hover()).0;
    let center = rect.center();
    ui.painter()
        .circle_filled(pos2(rect.left() + dot / 2.0, center.y), 4.0, GREEN);
    ui.painter().galley(
        pos2(rect.left() + dot + gap, center.y - galley.size().y / 2.0),
        galley,
        GREEN,
    );
}

// ---------- drawn icons (no icon font needed) ----------

#[derive(Clone, Copy)]
enum Chevron {
    Down,
    Up,
}

fn chevron(painter: &egui::Painter, rect: Rect, dir: Chevron, color: Color32) {
    // A shallow, compact disclosure mark, rather than a tall letter-like V.
    let half_width = rect.width().min(rect.height()) * 0.22;
    let half_height = half_width * 0.5;
    let c = rect.center();
    let direction = match dir {
        Chevron::Down => 1.0,
        Chevron::Up => -1.0,
    };
    let from = c + Vec2::new(-half_width, -half_height * direction);
    let tip = c + Vec2::new(0.0, half_height * direction);
    let to = c + Vec2::new(half_width, -half_height * direction);
    let stroke = Stroke::new(1.6_f32, color);
    painter.line_segment([from, tip], stroke);
    painter.line_segment([tip, to], stroke);
    for point in [from, tip, to] {
        painter.circle_filled(point, stroke.width / 2.0, color);
    }
}

/// Compact return arrow: rounded ends and a shaft, not a text-like “<”.
fn back_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let tip = c + Vec2::new(-6.5, 0.0);
    let tail = c + Vec2::new(6.5, 0.0);
    let upper = c + Vec2::new(-1.0, -5.5);
    let lower = c + Vec2::new(-1.0, 5.5);
    let stroke = Stroke::new(1.8_f32, color);
    for end in [tail, upper, lower] {
        painter.line_segment([tip, end], stroke);
        painter.circle_filled(end, stroke.width / 2.0, color);
    }
    painter.circle_filled(tip, stroke.width / 2.0, color);
}

fn back_button(ui: &mut Ui, enabled: bool) -> bool {
    let response = ui
        .add_enabled(
            enabled,
            Button::new("")
                .frame(false)
                .min_size(Vec2::splat(32.0))
                .corner_radius(16.0),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Back to services");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Back to services")
    });
    let rect = response.rect;
    let fill = if enabled && response.is_pointer_button_down_on() {
        PRESSED
    } else if enabled && response.hovered() {
        HOVER
    } else {
        Color32::WHITE
    };
    ui.painter().circle_filled(rect.center(), 16.0, fill);
    ui.painter()
        .circle_stroke(rect.center(), 16.0, Stroke::new(0.5_f32, BORDER));
    back_icon(ui.painter(), rect, if enabled { INK } else { MUTED });
    if enabled && response.has_focus() {
        focus_ring(ui, rect, 16);
    }
    response.clicked()
}

/// Eye outline with a pupil; a slash through it while the secret is revealed.
fn eye_icon(painter: &egui::Painter, rect: Rect, visible: bool, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.26;
    let left = c - Vec2::new(r * 1.3, 0.0);
    let right = c + Vec2::new(r * 1.3, 0.0);
    for direction in [-1.0, 1.0] {
        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [left, c + Vec2::new(0.0, r * 1.7 * direction), right],
            false,
            Color32::TRANSPARENT,
            Stroke::new(1.5_f32, color),
        ));
    }
    painter.circle_filled(c, r * 0.36, color);
    if visible {
        let d = r * 1.3;
        painter.line_segment(
            [pos2(c.x - d, c.y + d), pos2(c.x + d, c.y - d)],
            Stroke::new(1.5_f32, color),
        );
    }
}

/// Two overlapping squares: the front one complete, only the top/left of the back one.
fn copy_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.29;
    let front = Rect::from_center_size(c + Vec2::new(s * 0.32, s * 0.32), Vec2::splat(s * 1.45));
    let back = Rect::from_center_size(c + Vec2::new(-s * 0.32, -s * 0.32), Vec2::splat(s * 1.45));
    let stroke = Stroke::new(1.6_f32, color);
    painter.line_segment([back.left_top(), back.right_top()], stroke);
    painter.line_segment([back.left_top(), back.left_bottom()], stroke);
    painter.rect_stroke(front, 2.0, stroke, egui::StrokeKind::Inside);
}

fn check_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.26;
    let stroke = Stroke::new(1.6_f32, color);
    painter.line_segment(
        [
            pos2(c.x - s, c.y - s * 0.05),
            pos2(c.x - s * 0.15, c.y + s * 0.75),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(c.x - s * 0.15, c.y + s * 0.75),
            pos2(c.x + s, c.y - s * 0.8),
        ],
        stroke,
    );
}

fn close_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.24;
    let stroke = Stroke::new(1.6_f32, color);
    painter.line_segment([pos2(c.x - s, c.y - s), pos2(c.x + s, c.y + s)], stroke);
    painter.line_segment([pos2(c.x + s, c.y - s), pos2(c.x - s, c.y + s)], stroke);
}

/// Square, borderless button holding a drawn icon.
fn icon_button(
    ui: &mut Ui,
    size: f32,
    tooltip: &str,
    enabled: bool,
    draw: impl FnOnce(&egui::Painter, Rect, bool),
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    if enabled && (response.hovered() || response.is_pointer_button_down_on()) {
        ui.painter().rect_filled(
            rect,
            rect.height() / 2.0,
            if response.is_pointer_button_down_on() {
                PRESSED
            } else {
                HOVER
            },
        );
    }
    if enabled && response.has_focus() {
        focus_ring(ui, rect, 8);
    }
    draw(ui.painter(), rect, enabled && response.hovered());
    enabled && response.clicked()
}

/// Green status line: a small dot followed by a short label.
fn status_line(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 9.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, GREEN);
        ui.label(RichText::new(text).size(12.5).strong().color(GREEN));
    });
}

/// Titled card: the two halves of the link screen, side by side vertically.
fn card(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(0.5_f32, BORDER))
        .corner_radius(16.0)
        .shadow(surface_shadow())
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

/// Small uppercase section title with an optional right-aligned control.
fn section_row(ui: &mut Ui, text: &str, trailing: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(text)
                .size(10.5)
                .strong()
                .extra_letter_spacing(0.7)
                .color(SUBTLE),
        );
        ui.with_layout(Layout::right_to_left(egui::Align::Center), trailing);
    });
    ui.add_space(7.0);
}

/// Hairline separator with breathing room.
fn hairline(ui: &mut Ui) {
    ui.add_space(10.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, BORDER));
    ui.add_space(10.0);
}

/// Soft, bordered explanatory block.
fn hint(ui: &mut Ui, text: &str, tone: (Color32, Color32)) {
    let (fg, bg) = tone;
    egui::Frame::new()
        .fill(bg)
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).size(11.5).color(fg)).wrap());
        });
}

/// Compact on/off switch; returns true when the value changed.
fn switch(ui: &mut Ui, value: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let on = *value;
    if response.has_focus() {
        focus_ring(ui, rect, 10);
    }
    ui.painter().rect_filled(
        rect,
        10.0,
        if on {
            Color32::from_rgb(48, 168, 78)
        } else {
            Color32::from_rgb(201, 201, 207)
        },
    );
    let knob_x = if on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    let knob = pos2(knob_x, rect.center().y);
    ui.painter().circle_filled(
        knob + Vec2::new(0.0, 1.0),
        8.5,
        Color32::from_black_alpha(18),
    );
    ui.painter().circle_filled(knob, 8.0, Color32::WHITE);
    if ui.is_enabled() && response.clicked() {
        *value = !on;
        true
    } else {
        false
    }
}

/// Two-option segmented control; returns true when the selection changed.
/// Prefer an installed platform sans; retain egui's bundled fonts as fallbacks.
fn configure_font(ctx: &Context) {
    #[cfg(target_os = "macos")]
    let paths = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ];
    #[cfg(target_os = "windows")]
    let paths = ["C:\\Windows\\Fonts\\segoeui.ttf"];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let paths = [
        "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
        "/usr/share/fonts/truetype/inter/Inter-Regular.otf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    ];
    if let Some(bytes) = paths.iter().find_map(|path| std::fs::read(path).ok()) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "platform-sans".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "platform-sans".into());
        ctx.set_fonts(fonts);
    }
}

fn configure_context(ctx: &Context) {
    configure_font(ctx);
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(INK);
    visuals.weak_text_color = Some(SUBTLE);
    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    visuals.widgets.active.bg_fill = PRESSED;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, BLUE);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, BLUE);
    visuals.selection.bg_fill = Color32::from_rgb(211, 231, 252);
    visuals.selection.stroke = Stroke::new(1.0_f32, INK);
    visuals.hyperlink_color = BLUE;
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, BLUE);
    visuals.text_edit_bg_color = Some(FIELD);
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;
    visuals.window_fill = Color32::WHITE;
    visuals.panel_fill = CANVAS;
    visuals.window_corner_radius = 16.into();
    visuals.menu_corner_radius = 12.into();
    visuals.window_shadow = surface_shadow();
    visuals.popup_shadow = surface_shadow();
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size.y = 22.0;
        style.visuals.widgets.inactive.corner_radius = 9.0.into();
        style.visuals.widgets.hovered.corner_radius = 9.0.into();
        style.visuals.widgets.active.corner_radius = 9.0.into();
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.5));
    });
}

/// Three course platforms in one row.
struct GalleryLayout {
    tiles: [Rect; 3],
    footer: Rect,
}

impl GalleryLayout {
    fn new(bounds: Rect, footer_height: f32) -> Self {
        let gap = 16.0;
        let side = ((bounds.width() - 2.0 * gap) / 3.0).min(136.0).floor();
        let width = side * 3.0 + gap * 2.0;
        let grid_height = side;
        let footer_gap = 24.0;
        let height = grid_height + footer_gap + footer_height;
        let origin = pos2(
            bounds.center().x - width / 2.0,
            bounds.top() + ((bounds.height() - height) / 2.0).max(0.0),
        );
        let service_bottom = origin.y + grid_height;
        let balanced_gap = ((bounds.bottom() - service_bottom - footer_height) / 2.0).max(0.0);
        Self {
            tiles: [
                Rect::from_min_size(origin, Vec2::splat(side)),
                Rect::from_min_size(origin + Vec2::new(side + gap, 0.0), Vec2::splat(side)),
                Rect::from_min_size(
                    origin + Vec2::new((side + gap) * 2.0, 0.0),
                    Vec2::splat(side),
                ),
            ],
            footer: Rect::from_min_size(
                pos2(
                    bounds.center().x - (width + 32.0).min(bounds.width()) / 2.0,
                    service_bottom + balanced_gap,
                ),
                Vec2::new((width + 32.0).min(bounds.width()), footer_height),
            ),
        }
    }
}

fn public_waiting_copy(state: &TunnelState, public_on: bool, has_services: bool) -> &'static str {
    if !public_on || matches!(state, TunnelState::Stopped) {
        return "Public link is off. Turn the switch on to create an HTTPS link through Cloudflare. No account needed.";
    }
    match state {
        TunnelState::Starting => "Opening a free HTTPS link through Cloudflare.",
        TunnelState::Failed(_) => "The public link was not created. Turn the switch on to try again.",
        TunnelState::Ready(_) => "",
        _ if !has_services => {
            "No service is connected yet. Turn the switch on to create the HTTPS link anyway."
        }
        _ => "Turn the switch on to create a free HTTPS link through Cloudflare.",
    }
}

fn connector_url(port: u16, token: &str, public_origin: Option<&str>) -> String {
    mcp_endpoint(
        public_origin.unwrap_or(&format!("http://127.0.0.1:{port}")),
        token,
    )
}

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Err(String),
}

enum Message {
    Done(Notice, NextScreen),
}

enum NextScreen {
    Stay,
    Home,
}

enum ButtonKind {
    Primary,
    Secondary,
    Danger,
}

#[derive(Default, Clone)]
struct Draft {
    url: String,
    username: String,
    password: String,
    token: String,
    allow_write: bool,
    other_school: bool,
    use_token: bool,
    advanced: bool,
    password_visible: bool,
    /// True only when the user explicitly asks to change an existing connection.
    editing: bool,
}

pub struct App {
    settings: Settings,
    gateway: Gateway,
    port: u16,
    logos: Option<Logos>,
    bind_error: Option<String>,
    screen: &'static str, // "home" | "moodle" | "omnivox" | "brightspace" | "links"
    draft: Draft,
    /// Saved accounts for the current service (id, url). Refreshed on open,
    /// not every frame — avoids racing disk I/O while drafting a new account.
    account_choices: Vec<(String, String)>,
    busy: bool,
    notice: Option<Notice>,
    rx: Option<Receiver<Message>>,
    check_rx: Option<Receiver<Result<String, String>>>,
    check_status: Option<Notice>,
    tunnel: Tunnel,
    local_enabled: Arc<AtomicBool>,
    background: Background,
    copied_flash: Option<(String, Instant)>,
    /// Interface zoom factor, persisted in the config file.
    scale: f32,
    /// Zoom −/+ hit targets in the same point space as the last pointer move.
    /// Kept across zoom changes so spam-clicks still land without moving the mouse
    /// (egui-winit keeps a stale pointer pos until the next CursorMoved).
    zoom_hit: Option<(Rect, Rect)>,
    /// While the primary button stays down after pressing a zoom control.
    zoom_held: Option<f32>,
    zoom_repeat_at: Option<Instant>,
}

fn native_options() -> eframe::NativeOptions {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("student-mcp")
            .with_app_id("student-mcp")
            .with_inner_size([440.0, 820.0])
            .with_min_inner_size([400.0, 560.0]),
        // Mesa's Wayland EGL vsync can block eglSwapBuffers when a window is
        // occluded or moved to another workspace. This stalls the UI thread,
        // including compositor ping/pong handling. Keep Linux swaps nonblocking;
        // eframe still redraws only on demand (no continuous repaint loop).
        vsync: !cfg!(target_os = "linux"),
        ..Default::default()
    };
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut options = options;
        if crate::background::supports_background_window() {
            options.event_loop_builder = Some(Box::new(|builder| {
                builder.with_x11();
            }));
        }
        options
    }
    #[cfg(not(target_os = "linux"))]
    {
        options
    }
}

pub fn run(port_override: Option<u16>, first_run: bool) {
    let settings = Settings::load();
    if first_run {
        let _ = settings.save(); // materialize the config file
    }
    let mut gateway = settings.gateway();
    let base_port = port_override.unwrap_or(gateway.port);
    // Bind now (before the window opens); try sibling ports if taken.
    let mut bind_error = None;
    let mut chosen = None;
    let local_enabled = Arc::new(AtomicBool::new(settings.gateway.local_enabled()));
    for attempt in 0..10 {
        let port = base_port + attempt;
        match crate::http::bind(port) {
            Ok(listener) => {
                chosen = Some(port);
                let enabled = Arc::clone(&local_enabled);
                std::thread::spawn(move || {
                    if let Err(message) = crate::http::serve_controlled(listener, enabled) {
                        eprintln!("{message}");
                    }
                });
                break;
            }
            Err(error) if attempt == 9 => bind_error = Some(error.to_string()),
            Err(_) => continue,
        }
    }
    let port = chosen.unwrap_or(base_port);
    gateway.port = port;
    #[cfg(not(test))]
    crate::share::set_port(if bind_error.is_none() { port } else { 0 });

    let result = eframe::run_native(
        "student-mcp",
        native_options(),
        Box::new(move |cc| {
            configure_context(&cc.egui_ctx);
            let mut app = App {
                settings: Settings::load(),
                gateway,
                port,
                logos: None,
                bind_error,
                screen: "home",
                draft: Draft::default(),
                account_choices: Vec::new(),
                busy: false,
                notice: None,
                rx: None,
                check_rx: None,
                check_status: None,
                tunnel: Tunnel::default(),
                local_enabled,
                background: Background::enabled(),
                copied_flash: None,
                scale: crate::config::DEFAULT_SCALE,
                zoom_hit: None,
                zoom_held: None,
                zoom_repeat_at: None,
            };
            app.scale = app.settings.scale();
            app.apply_scale(&cc.egui_ctx);
            app.logos = Some(Logos::load(&cc.egui_ctx));
            Ok(Box::new(app))
        }),
    );
    if let Err(error) = result {
        eprintln!("Window unavailable: {error:?} — the stdio MCP server is still available (student-mcp with no command).");
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Also runs for native application termination (e.g. Cmd-Q on macOS).
        self.tunnel.stop();
        self.background.shutdown();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.background.update(ctx) {
            self.tunnel.stop();
            return;
        }
        self.apply_scale(ctx);
        self.poll_jobs();
        self.update_tunnel(ctx);
        CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(24.0))
            .show(ctx, |ui| {
                if self.screen == "home" {
                    self.home_screen(ui);
                } else {
                    let bounds = form_bounds(ui.available_rect_before_wrap());
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(bounds)
                            .layout(Layout::top_down(egui::Align::Min)),
                        |ui| {
                            ui.set_width(bounds.width());
                            egui::ScrollArea::vertical()
                                .id_salt(("screen", self.screen))
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add_enabled_ui(!self.busy, |ui| self.body(ui, ctx));
                                });
                        },
                    );
                }
            });
        self.scale_control(ctx);
    }
}

impl App {
    // ---------- scale ----------

    fn apply_scale(&mut self, ctx: &Context) {
        if (ctx.zoom_factor() - self.scale).abs() > f32::EPSILON {
            ctx.set_zoom_factor(self.scale);
            ctx.request_repaint();
        }
    }

    fn change_scale(&mut self, delta: f32) {
        self.scale = ((self.scale + delta) * 10.0).round() / 10.0;
        self.scale = self
            .scale
            .clamp(crate::config::SCALE_MIN, crate::config::SCALE_MAX);
        self.settings.scale = Some(self.scale);
        let _ = self.settings.save();
    }

    /// Two independent zoom buttons pinned to the bottom-right corner.
    fn scale_control(&mut self, ctx: &Context) {
        let step = crate::config::SCALE_STEP;
        let min_hit = self.scale <= crate::config::SCALE_MIN;
        let max_hit = self.scale >= crate::config::SCALE_MAX;

        // Pointer events stay in the point space of the last CursorMoved. After a
        // zoom change that space no longer matches the painted layout, so keep the
        // last hit rects until the pointer moves again — spam-clicks still land.
        let pointer_moved = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        });
        let primary_down = ctx.input(|input| input.pointer.primary_down());
        let press_pos = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        let pointer_pos = ctx.input(|input| input.pointer.latest_pos());

        if !primary_down {
            self.zoom_held = None;
            self.zoom_repeat_at = None;
        }

        let mut paint_minus = Rect::NOTHING;
        let mut paint_plus = Rect::NOTHING;
        egui::Area::new(egui::Id::new("scale-control"))
            .anchor(Align2::RIGHT_BOTTOM, [-10.0, -10.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|row| {
                    row.spacing_mut().item_spacing.x = 9.0;
                    paint_minus = row.allocate_exact_size(Vec2::splat(28.0), Sense::hover()).0;
                    paint_plus = row.allocate_exact_size(Vec2::splat(28.0), Sense::hover()).0;
                });

                // Fresh moves share paint space; after zoom without a move, keep the
                // previous event-space targets so the stale pointer still hits.
                if pointer_moved || self.zoom_hit.is_none() {
                    self.zoom_hit = Some((paint_minus, paint_plus));
                }
                let (hit_minus, hit_plus) = self.zoom_hit.unwrap_or((paint_minus, paint_plus));
                let hover_minus =
                    !min_hit && pointer_pos.is_some_and(|pos| hit_minus.contains(pos));
                let hover_plus = !max_hit && pointer_pos.is_some_and(|pos| hit_plus.contains(pos));
                let press_minus = self.zoom_held == Some(-step)
                    || (primary_down && pointer_pos.is_some_and(|pos| hit_minus.contains(pos)));
                let press_plus = self.zoom_held == Some(step)
                    || (primary_down && pointer_pos.is_some_and(|pos| hit_plus.contains(pos)));

                Self::zoom_button_draw(ui, paint_minus, "−", min_hit, hover_minus, press_minus);
                Self::zoom_button_draw(ui, paint_plus, "+", max_hit, hover_plus, press_plus);
            });

        let (hit_minus, hit_plus) = self.zoom_hit.unwrap_or((paint_minus, paint_plus));
        let mut delta = None;
        if let Some(pos) = press_pos {
            if !min_hit && hit_minus.contains(pos) {
                delta = Some(-step);
                self.zoom_held = Some(-step);
                self.zoom_repeat_at = Some(Instant::now() + std::time::Duration::from_millis(400));
            } else if !max_hit && hit_plus.contains(pos) {
                delta = Some(step);
                self.zoom_held = Some(step);
                self.zoom_repeat_at = Some(Instant::now() + std::time::Duration::from_millis(400));
            }
        } else if let Some(held) = self.zoom_held {
            let due = self.zoom_repeat_at.is_some_and(|at| Instant::now() >= at);
            let blocked = (held < 0.0 && min_hit) || (held > 0.0 && max_hit);
            if due && !blocked {
                delta = Some(held);
                self.zoom_repeat_at = Some(Instant::now() + std::time::Duration::from_millis(70));
                ctx.request_repaint_after(std::time::Duration::from_millis(70));
            } else if let Some(at) = self.zoom_repeat_at {
                if !due {
                    ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
                }
            }
        }

        if let Some(delta) = delta {
            self.change_scale(delta);
        }
    }

    fn zoom_button_draw(
        ui: &mut Ui,
        rect: Rect,
        label: &str,
        disabled: bool,
        hovered: bool,
        pressed: bool,
    ) {
        let response = ui.interact(rect, ui.id().with(label), Sense::hover());
        let response = response.on_hover_text(if disabled {
            "Size limit"
        } else if label == "+" {
            "Larger"
        } else {
            "Smaller"
        });
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !disabled, label));
        ui.painter().add(
            egui::epaint::Shadow {
                offset: [0, if hovered { 5 } else { 3 }],
                blur: if hovered { 14 } else { 10 },
                spread: 0,
                color: Color32::from_black_alpha(if hovered { 45 } else { 22 }),
            }
            .as_shape(rect, 14),
        );
        ui.painter().rect_filled(
            rect,
            14.0,
            if pressed && !disabled {
                PRESSED
            } else {
                Color32::WHITE
            },
        );
        ui.painter().rect_stroke(
            rect,
            14.0,
            Stroke::new(0.5_f32, if hovered { MUTED } else { BORDER }),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(14.0),
            if disabled {
                MUTED
            } else if hovered {
                INK
            } else {
                SUBTLE
            },
        );
    }
    /// True when the current screen should show the connected summary instead
    /// of the credential form.
    fn viewing_connected(&self) -> bool {
        match self.screen {
            "omnivox" => self.settings.omnivox().is_some() && !self.draft.editing,
            "brightspace" => self.settings.brightspace().is_some() && !self.draft.editing,
            "moodle" => self.settings.moodle().is_some() && !self.draft.editing,
            _ => false,
        }
    }

    /// A configured service: check it, change access or credentials, or disconnect.
    fn connected_view(&mut self, ui: &mut Ui, ctx: &egui::Context, service: &str) {
        let allow_write = self.settings.moodle().is_some_and(|c| c.allow_write);
        let mut retry = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (label, color) = if self.check_rx.is_some() {
                    ("Checking…", SUBTLE)
                } else {
                    match &self.check_status {
                        Some(Notice::Ok(_)) => ("Online", GREEN),
                        Some(Notice::Err(_)) => ("Offline", RED),
                        None => ("Not checked", SUBTLE),
                    }
                };
                ui.label(RichText::new(label).size(11.5).strong().color(color));
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    retry = ui
                        .add_enabled(
                            self.check_rx.is_none(),
                            Button::new(RichText::new("Retry").size(11.0).color(SUBTLE))
                                .frame(false),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked();
                });
            });
            if let Some(Notice::Err(message)) = self.check_status.as_ref() {
                ui.add(egui::Label::new(RichText::new(message).size(10.5).color(RED)).wrap());
            }
            if let Some(Notice::Ok(message)) = self.check_status.as_ref() {
                ui.add(egui::Label::new(RichText::new(message).size(10.5).color(SUBTLE)).wrap());
            }
            if !matches!(service, "omnivox" | "brightspace") {
                hairline(ui);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Allow changes").size(12.0).color(INK));
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut on = allow_write;
                        if switch(ui, &mut on) {
                            self.set_allow_write(service, on);
                        }
                    });
                });
            }
        });
        if retry {
            self.check_service(ctx);
        }

        ui.add_space(8.0);
        if Self::button_soft(ui, "Change login", !self.busy) {
            self.draft.editing = true;
            self.notice = None;
        }
        ui.add_space(8.0);
        if Self::button_danger(ui, "Disconnect", !self.busy) {
            let service = service.to_string();
            self.spawn(
                ctx.clone(),
                move |mut fresh| match crate::mcp::disable_service(&mut fresh, &service) {
                    Ok(()) => Message::Done(
                        Notice::Ok(format!("{} disconnected.", service)),
                        NextScreen::Home,
                    ),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                },
            );
        }
        self.notice_line(ui);
    }

    fn set_allow_write(&mut self, service: &str, allow: bool) {
        let mut fresh = self.settings.reload();
        if service == "moodle" {
            if let Err(message) = crate::mcp::set_moodle_write(&mut fresh, allow) {
                self.notice = Some(Notice::Err(message));
            }
        }
        self.settings = fresh;
    }

    /// Test a saved connection in the background, without blocking navigation.
    fn check_service(&mut self, ctx: &Context) {
        if !self.viewing_connected() {
            return;
        }
        let service = self.screen;
        let settings = self.settings.clone();
        let (sender, receiver) = channel();
        self.check_rx = Some(receiver);
        self.check_status = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(crate::mcp::test_service(&settings, service));
            ctx.request_repaint();
        });
    }

    fn poll_check(&mut self) {
        let Some(receiver) = self.check_rx.as_ref() else {
            return;
        };
        let status = match receiver.try_recv() {
            Ok(Ok(message)) => Notice::Ok(message),
            Ok(Err(message)) => Notice::Err(message),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Notice::Err("The check was interrupted. Try again.".into())
            }
        };
        self.check_rx = None;
        self.check_status = Some(status);
    }

    fn body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match self.screen {
            "moodle" => self.moodle_screen(ui, ctx),
            "omnivox" => self.omnivox_screen(ui, ctx),
            "brightspace" => self.brightspace_screen(ui, ctx),
            "links" => self.links_screen(ui, ctx),
            _ => self.home_screen(ui),
        }
    }

    // ---------- home: the gallery ----------

    fn home_screen(&mut self, ui: &mut Ui) {
        if let Some(message) = &self.settings.config_error {
            ui.label(RichText::new(message).size(12.0).color(RED));
            ui.add_space(8.0);
        }
        let layout = self.gallery_layout(ui);
        let Some(logos) = self.logos.as_ref() else {
            return;
        };
        let moodle_clicked = Self::tile(
            ui,
            &logos.moodle,
            "Moodle",
            layout.tiles[0],
            !self.settings.account_list("moodle").is_empty(),
        );
        let omnivox_clicked = Self::tile(
            ui,
            &logos.omnivox,
            "Omnivox",
            layout.tiles[1],
            !self.settings.account_list("omnivox").is_empty(),
        );
        let brightspace_clicked = Self::tile(
            ui,
            &logos.brightspace,
            "Brightspace / myCourses",
            layout.tiles[2],
            !self.settings.account_list("brightspace").is_empty(),
        );
        if moodle_clicked || omnivox_clicked || brightspace_clicked {
            let service = if moodle_clicked {
                "moodle"
            } else if omnivox_clicked {
                "omnivox"
            } else {
                "brightspace"
            };
            self.open_service(service, ui.ctx());
            self.notice = None;
        }

        self.home_link(ui, layout.footer);
    }

    fn gallery_layout(&self, ui: &Ui) -> GalleryLayout {
        let bounds = ui.available_rect_before_wrap();
        let initial = GalleryLayout::new(bounds, 110.0);
        let url = connector_url(self.port, &self.gateway.token, self.tunnel.origin());
        let (text, font, color) = self.home_link_text(&url);
        let title_height = ui
            .painter()
            .layout_no_wrap("AI connection".into(), FontId::proportional(11.5), INK)
            .size()
            .y;
        let subtitle_height = ui
            .painter()
            .layout(text.into(), font, color, initial.footer.width() - 54.0)
            .size()
            .y;
        let footer_height = (title_height + 4.0 + subtitle_height + 32.0).max(110.0);
        GalleryLayout::new(bounds, footer_height)
    }

    fn home_link(&mut self, ui: &mut Ui, rect: Rect) {
        let url = connector_url(self.port, &self.gateway.token, self.tunnel.origin());
        let copied = self.is_copied(&url);
        ui.painter().add(surface_shadow().as_shape(rect, 14));
        ui.painter().rect_filled(rect, 14.0, Color32::WHITE);
        ui.painter().rect_stroke(
            rect,
            14.0,
            Stroke::new(0.5_f32, BORDER),
            egui::StrokeKind::Inside,
        );
        let title =
            ui.painter()
                .layout_no_wrap("AI connection".into(), FontId::proportional(11.5), INK);
        let (text, font, color) = self.home_link_text(&url);
        let subtitle = ui
            .painter()
            .layout(text.into(), font, color, rect.width() - 54.0);
        // Center the whole text group, including wrapped URLs, on the copy button.
        let text_top = rect.center().y - (title.size().y + 4.0 + subtitle.size().y) / 2.0;
        ui.painter()
            .galley(pos2(rect.left() + 12.0, text_top), title.clone(), INK);
        let text_rect = Rect::from_min_size(
            pos2(rect.left() + 12.0, text_top + title.size().y + 4.0),
            Vec2::new(rect.width() - 54.0, subtitle.size().y),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_width(text_rect.width());
                let link = ui
                    .add(egui::Label::new(subtitle).wrap().sense(Sense::click()))
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(if self.bind_error.is_some() {
                        self.bind_error.as_deref().unwrap_or_default()
                    } else {
                        "Open link settings"
                    });
                if link.clicked() {
                    self.screen = "links";
                    self.notice = None;
                }
            },
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_center_size(
                pos2(rect.right() - 24.0, rect.top() + 24.0),
                Vec2::splat(24.0),
            )),
            |ui| {
                if icon_button(
                    ui,
                    24.0,
                    if copied { "Copied" } else { "Copy link" },
                    self.bind_error.is_none() && self.tunnel.origin().is_some(),
                    |painter, rect, hovered| {
                        if copied {
                            check_icon(painter, rect, GREEN);
                        } else {
                            copy_icon(painter, rect, if hovered { INK } else { SUBTLE });
                        }
                    },
                ) {
                    ui.ctx().copy_text(url.clone());
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs(2));
                    self.copied_flash = Some((url, Instant::now()));
                }
            },
        );
    }

    fn home_link_text<'a>(&self, url: &'a str) -> (&'a str, FontId, Color32) {
        if self.bind_error.is_some() {
            ("Server offline", FontId::proportional(11.0), RED)
        } else if self.tunnel.origin().is_some() {
            (url, FontId::monospace(10.5), SUBTLE)
        } else {
            (
                match &self.tunnel.state {
                    TunnelState::Starting => "Creating your public link…",
                    TunnelState::Failed(_) => "Public link unavailable — click to retry",
                    TunnelState::Stopped => "Public link off — click to enable",
                    _ => "Connect a service to create your link",
                },
                FontId::proportional(11.0),
                SUBTLE,
            )
        }
    }

    /// App-like service tiles with quiet labels and a saved-connection indicator.
    fn tile(ui: &mut Ui, texture: &TextureHandle, name: &str, rect: Rect, active: bool) -> bool {
        let raw = ui.interact(
            rect,
            ui.make_persistent_id(("service", name)),
            Sense::click(),
        );
        let response = raw.on_hover_cursor(CursorIcon::PointingHand);
        let hovered = response.hovered();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));

        let pressed = response.is_pointer_button_down_on();
        let surface = if pressed { rect.shrink(2.0) } else { rect };
        if !pressed {
            ui.painter().add(surface_shadow().as_shape(surface, 22));
        }
        ui.painter().rect_filled(
            surface,
            22.0,
            if pressed {
                PRESSED
            } else if hovered {
                HOVER
            } else {
                Color32::WHITE
            },
        );
        ui.painter().rect_stroke(
            surface,
            22.0,
            Stroke::new(
                if active { 1.0_f32 } else { 0.5_f32 },
                if active {
                    GREEN
                } else if hovered {
                    MUTED
                } else {
                    BORDER
                },
            ),
            egui::StrokeKind::Inside,
        );
        if response.has_focus() {
            focus_ring(ui, surface, 22);
        }

        // Fit the complete app symbol without cropping or stretching it.
        let logo_bounds = Vec2::splat(surface.width() * 0.46);
        let texture_size = texture.size_vec2();
        let scale = (logo_bounds.x / texture_size.x).min(logo_bounds.y / texture_size.y);
        let logo_rect = Rect::from_center_size(
            surface.center() - Vec2::new(0.0, 10.0),
            texture_size * scale,
        );
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter()
            .image(texture.id(), logo_rect, uv, Color32::WHITE);
        let label = if name == "Brightspace / myCourses" {
            "Brightspace"
        } else {
            name
        };
        ui.painter().text(
            pos2(surface.center().x, surface.bottom() - 22.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.0),
            INK,
        );
        if active {
            ui.painter()
                .circle_filled(surface.right_top() + Vec2::new(-15.0, 15.0), 3.0, GREEN);
        }

        response
            .on_hover_text(if active {
                format!("{name} · Manage connection")
            } else {
                format!("{name} · Connect")
            })
            .clicked()
    }

    fn open_service(&mut self, service: &'static str, ctx: &Context) {
        let root = Settings::load();
        let id = root
            .account_list(service)
            .first()
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| service.into());
        self.open_account(service, &id, ctx);
    }

    /// Switch or add accounts. Hidden on first connect (no saved accounts yet).
    /// Placed under the service header so back / logo / title stay on top.
    fn account_picker(&mut self, ui: &mut Ui, ctx: &Context) {
        if self.account_choices.is_empty() {
            return;
        }
        let mut selected = self
            .settings
            .account_scope
            .as_ref()
            .map(|(_, id)| id.clone())
            .unwrap_or_else(|| self.screen.into());
        let is_new = !self.account_choices.iter().any(|(id, _)| id == &selected);
        let previous = selected.clone();
        let label = if is_new {
            "New account".into()
        } else {
            self.account_choices
                .iter()
                .find(|(id, _)| id == &selected)
                .map(|(id, url)| format!("{url} · {}", id.chars().take(8).collect::<String>()))
                .unwrap_or_else(|| "Account".into())
        };
        ui.label(RichText::new("Account").size(12.0).strong().color(INK));
        egui::ComboBox::from_id_salt("account-picker")
            .width(ui.available_width())
            .selected_text(RichText::new(label).color(INK))
            .show_ui(ui, |ui| {
                ui.set_min_width(220.0);
                for (id, url) in &self.account_choices {
                    ui.selectable_value(
                        &mut selected,
                        id.clone(),
                        format!("{url} · {}", id.chars().take(8).collect::<String>()),
                    );
                }
                if is_new {
                    ui.separator();
                    ui.add_enabled(
                        false,
                        egui::Label::new(RichText::new("New account").color(SUBTLE)),
                    );
                }
            });
        if selected != previous {
            self.open_account(self.screen, &selected, ctx);
        }
        // Offer "add another" only while viewing a saved account, not while drafting a new one.
        if !is_new {
            ui.add_space(6.0);
            if Self::button_soft(ui, "+ Add account", !self.busy) {
                let id = crate::config::random_hex(8);
                self.open_account(self.screen, &id, ctx);
                self.draft.editing = true;
            }
        }
        ui.add_space(10.0);
    }

    fn open_account(&mut self, service: &'static str, id: &str, ctx: &Context) {
        let root = Settings::load();
        self.account_choices = root.account_list(service);
        self.settings = root.for_account(service, id);
        self.draft = Draft::default();
        self.draft.editing = false;
        self.screen = service;
        match service {
            "omnivox" => {
                if let Some(config) = self.settings.omnivox() {
                    self.draft.url = config.base_url;
                }
            }
            "brightspace" => {
                if let Some(config) = self.settings.brightspace() {
                    self.draft.url = config.base_url;
                }
            }
            "moodle" => {
                if let Some(config) = self.settings.moodle() {
                    self.draft.allow_write = config.allow_write;
                    self.draft.other_school = config.base_url != MOODLE_DEFAULT_URL;
                    self.draft.url = config.base_url;
                }
            }
            _ => {}
        }
        if self.viewing_connected() {
            self.check_service(ctx);
        } else {
            self.check_rx = None;
            self.check_status = None;
        }
    }

    // ---------- the link screen: local and public ----------

    fn links_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "", "", false) {
            return;
        }
        // ---- local: this machine ----
        let local = mcp_endpoint(
            &format!("http://127.0.0.1:{}", self.port),
            &self.gateway.token,
        );
        let mut local_on = self.local_enabled.load(Ordering::SeqCst);
        section_row(ui, "LOCAL LINK", |row| {
            if row
                .add_enabled_ui(self.bind_error.is_none(), |ui| switch(ui, &mut local_on))
                .inner
            {
                if self.set_link_preferences(local_on, self.settings.gateway.public_enabled()) {
                    self.local_enabled.store(local_on, Ordering::SeqCst);
                    #[cfg(not(test))]
                    crate::share::set_port(if local_on { self.port } else { 0 });
                    if !local_on {
                        self.tunnel.stop();
                    } else {
                        self.tunnel.state = TunnelState::Idle;
                    }
                } else {
                    local_on = self.local_enabled.load(Ordering::SeqCst);
                }
            }
        });
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if self.bind_error.is_some() {
                    ui.label(RichText::new("Local link unavailable").color(RED));
                } else if local_on {
                    status_line(ui, "Local link active");
                } else {
                    ui.label(RichText::new("Local link off").color(SUBTLE));
                }
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    self.copy_link_button(ui, &local, local_on && self.bind_error.is_none());
                });
            });
            if !local_on || self.bind_error.is_some() {
                return;
            }
            ui.add_space(9.0);
            ui.add(
                egui::Label::new(
                    RichText::new(local.as_str())
                        .monospace()
                        .size(12.0)
                        .color(INK),
                )
                .wrap()
                .selectable(true),
            );
        });

        ui.add_space(16.0);

        // ---- public: over the internet ----
        self.public_section(ui, ctx);

        self.notice_line(ui);
    }

    fn public_section(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut enabled = matches!(
            self.tunnel.state,
            TunnelState::Starting | TunnelState::Ready(_)
        );
        // The switch is the way to ask for a link. A missing service must not
        // swallow the click; the local server still has to be up to receive it.
        let can_toggle = !self.busy && self.bind_error.is_none();
        section_row(ui, "PUBLIC LINK", |row| {
            let changed = row
                .add_enabled_ui(can_toggle, |ui| switch(ui, &mut enabled))
                .inner;
            if changed {
                let local_on = self.local_enabled.load(Ordering::SeqCst) || enabled;
                if self.set_link_preferences(local_on, enabled) {
                    self.local_enabled.store(local_on, Ordering::SeqCst);
                    if enabled {
                        self.tunnel.start(self.port);
                    } else {
                        self.tunnel.stop();
                    }
                    ctx.request_repaint();
                }
            }
        });
        if let Some(origin) = self.tunnel.origin() {
            let url = mcp_endpoint(origin, &self.gateway.token);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    status_line(ui, "Public link active");
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        self.copy_link_button(ui, &url, true);
                    });
                });
                ui.add_space(9.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(url.as_str())
                            .monospace()
                            .size(11.5)
                            .color(INK),
                    )
                    .wrap()
                    .selectable(true),
                );
            });
            return;
        }

        let starting = self.tunnel.state == TunnelState::Starting;
        card(ui, |ui| {
            let message = if self.bind_error.is_some() {
                "The local server is not running, so the public link cannot be created."
            } else {
                public_waiting_copy(
                    &self.tunnel.state,
                    self.settings.gateway.public_enabled(),
                    self.has_services(),
                )
            };
            ui.label(RichText::new(message).size(11.5).color(SUBTLE));
            if let TunnelState::Failed(message) = &self.tunnel.state {
                hint(ui, message, (RED, RED_SOFT));
            }
            if starting {
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(11.0));
                    ui.label(
                        RichText::new("This can take a few seconds.")
                            .size(10.5)
                            .color(SUBTLE),
                    );
                });
            }
        });
    }

    fn is_copied(&self, text: &str) -> bool {
        self.copied_flash
            .as_ref()
            .is_some_and(|(value, at)| value == text && at.elapsed().as_secs_f32() < 1.8)
    }

    fn copy_link_button(&mut self, ui: &mut Ui, url: &str, enabled: bool) {
        let copied = self.is_copied(url);
        if icon_button(
            ui,
            24.0,
            if copied { "Copied" } else { "Copy link" },
            enabled,
            |painter, rect, hovered| {
                if copied {
                    check_icon(painter, rect, GREEN);
                } else {
                    copy_icon(painter, rect, if hovered { INK } else { SUBTLE });
                }
            },
        ) {
            ui.ctx().copy_text(url.to_owned());
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(2));
            self.copied_flash = Some((url.to_owned(), Instant::now()));
        }
    }

    fn has_services(&self) -> bool {
        if self.settings.account_scope.is_some() {
            !Settings::load().services_active().is_empty()
        } else {
            !self.settings.services_active().is_empty()
        }
    }

    fn set_link_preferences(&mut self, local: bool, public: bool) -> bool {
        let mut fresh = Settings::load();
        fresh.gateway.local_enabled = Some(local);
        fresh.gateway.public_enabled = Some(public);
        if let Err(error) = fresh.save() {
            self.notice = Some(Notice::Err(format!(
                "Cannot save link settings. The previous settings are still active. {error}"
            )));
            return false;
        }
        self.settings = fresh;
        true
    }

    fn update_tunnel(&mut self, ctx: &Context) {
        self.tunnel.poll();
        // GUI tests use synthetic tunnel origins; do not leak them into the
        // process-wide file-link base used by other tests.
        #[cfg(not(test))]
        crate::share::set_origin(self.tunnel.origin().map(str::to_string));
        if !self.local_enabled.load(Ordering::SeqCst) || !self.settings.gateway.public_enabled() {
            if matches!(
                self.tunnel.state,
                TunnelState::Starting | TunnelState::Ready(_) | TunnelState::Idle
            ) {
                self.tunnel.stop();
                #[cfg(not(test))]
                crate::share::set_origin(None);
            }
            return;
        }
        // A link the user turned on stays up with or without a service.
        // Idle only auto-starts once a service exists, so a fresh install
        // does not open a tunnel before anyone asks for one.
        if self.has_services()
            && self.tunnel.state == TunnelState::Idle
            && self.bind_error.is_none()
        {
            self.tunnel.start(self.port);
        }
        if matches!(
            self.tunnel.state,
            TunnelState::Starting | TunnelState::Ready(_)
        ) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn notice_line(&mut self, ui: &mut Ui) {
        if let Some(notice) = self.notice.clone() {
            let (text, color, bg, ok) = match notice {
                Notice::Ok(text) => (text, GREEN, GREEN_SOFT, true),
                Notice::Err(text) => (text, RED, RED_SOFT, false),
            };
            ui.add_space(4.0);
            let mut dismiss = false;
            egui::Frame::new()
                .fill(bg)
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(12, 11))
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                        if ok {
                            check_icon(ui.painter(), rect, color);
                        } else {
                            ui.painter().circle_stroke(
                                rect.center(),
                                7.0,
                                Stroke::new(1.4_f32, color),
                            );
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "!",
                                FontId::proportional(9.0),
                                color,
                            );
                        }
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(RichText::new(text).size(11.5).color(color)).wrap(),
                        );
                        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                            if icon_button(ui, 18.0, "Dismiss", true, |painter, rect, hovered| {
                                close_icon(painter, rect, if hovered { color } else { MUTED })
                            }) {
                                dismiss = true;
                            }
                        });
                    });
                });
            if dismiss {
                self.notice = None;
            }
            ui.add_space(8.0);
        }
    }

    // ---------- service screens ----------

    fn top_bar(&mut self, ui: &mut Ui, title: &str, subtitle: &str, connected: bool) -> bool {
        let back = back_button(ui, !self.busy);
        if back {
            self.screen = "home";
            self.settings = Settings::load();
            self.account_choices.clear();
            self.notice = None;
            self.draft = Draft::default();
            return true;
        }
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            if let Some(logos) = &self.logos {
                let texture = match title {
                    "Moodle" => Some(&logos.moodle),
                    "Omnivox" => Some(&logos.omnivox),
                    "Brightspace" => Some(&logos.brightspace),
                    _ => None,
                };
                if let Some(texture) = texture {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(66.0), Sense::hover());
                    ui.painter().add(surface_shadow().as_shape(rect, 18));
                    ui.painter().rect_filled(rect, 18.0, Color32::WHITE);
                    ui.painter().rect_stroke(
                        rect,
                        18.0,
                        Stroke::new(0.5_f32, BORDER),
                        egui::StrokeKind::Inside,
                    );
                    let size = texture.size_vec2();
                    let scale = 38.0 / size.x.max(size.y);
                    ui.painter().image(
                        texture.id(),
                        Rect::from_center_size(rect.center(), size * scale),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.add_space(12.0);
                }
            }
            ui.label(
                RichText::new(title)
                    .size(24.0)
                    .strong()
                    .extra_letter_spacing(-0.5)
                    .color(INK),
            );
            if !subtitle.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(subtitle).size(12.0).color(SUBTLE));
            }
            if connected {
                ui.add_space(9.0);
                connected_badge(ui);
            }
        });
        ui.add_space(26.0);
        false
    }

    fn field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, secret: bool) {
        ui.label(RichText::new(label).size(12.0).strong().color(INK));
        Self::input_field(ui, label, buffer, hint, secret);
    }

    fn secret_field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, visible: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).strong().color(INK));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let shown = *visible;
                if icon_button(
                    ui,
                    24.0,
                    if shown { "Hide" } else { "Show" },
                    true,
                    |painter, rect, hovered| {
                        eye_icon(painter, rect, shown, if hovered { INK } else { SUBTLE })
                    },
                ) {
                    *visible = !*visible;
                }
            });
        });
        Self::input_field(ui, label, buffer, hint, !*visible);
    }

    /// Disclosure row for advanced settings: label plus a chevron, whole row clickable.
    fn advanced_row(ui: &mut Ui, expanded: &mut bool) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
        let response = response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if *expanded {
                "Hide advanced options"
            } else {
                "Show advanced options"
            });
        if response.hovered() {
            ui.painter().rect_filled(rect, 6.0, HOVER);
        }
        if response.has_focus() {
            focus_ring(ui, rect, 6);
        }
        ui.painter().text(
            rect.left_center() + Vec2::new(6.0, 0.0),
            Align2::LEFT_CENTER,
            "Advanced options",
            FontId::proportional(11.5),
            if response.hovered() { INK } else { SUBTLE },
        );
        chevron(
            ui.painter(),
            Rect::from_center_size(
                pos2(rect.right() - 16.0, rect.center().y),
                Vec2::splat(22.0),
            ),
            if *expanded {
                Chevron::Up
            } else {
                Chevron::Down
            },
            if response.hovered() { INK } else { SUBTLE },
        );
        if response.clicked() {
            *expanded = !*expanded;
        }
        ui.add_space(6.0);
    }

    fn input_field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, secret: bool) {
        let frame = egui::Frame::new()
            .fill(FIELD)
            .corner_radius(10.0)
            .stroke(Stroke::new(0.75_f32, BORDER))
            .inner_margin(egui::Margin::symmetric(12, 11));
        let field = frame.show(ui, |inner| {
            inner.add(
                TextEdit::singleline(buffer)
                    .id_salt(label)
                    .hint_text(RichText::new(hint).color(SUBTLE))
                    .text_color(INK)
                    .font(egui::TextStyle::Body)
                    .desired_width(inner.available_width())
                    .frame(false)
                    .password(secret),
            )
        });
        if field.inner.has_focus() {
            focus_ring(ui, field.response.rect, 10);
        }
        ui.add_space(12.0);
    }

    fn permission_row(ui: &mut Ui, label: &str, value: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(INK));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                switch(ui, value);
            });
        });
        ui.add_space(4.0);
    }

    fn toggle(ui: &mut Ui, label: &str, value: &mut bool) {
        ui.add(egui::Checkbox::new(
            value,
            RichText::new(label).size(11.5).color(SUBTLE),
        ));
        ui.add_space(4.0);
    }

    /// One native egui button for the three actions, with a consistent shape.
    fn full_button(ui: &mut Ui, label: &str, enabled: bool, kind: ButtonKind) -> bool {
        let (fill, hover, pressed, text, border) = match kind {
            ButtonKind::Primary => (
                BLUE,
                Color32::from_rgb(0, 92, 190),
                Color32::from_rgb(0, 76, 163),
                Color32::WHITE,
                Color32::TRANSPARENT,
            ),
            ButtonKind::Secondary => (Color32::WHITE, HOVER, PRESSED, BLUE, BORDER),
            ButtonKind::Danger => (
                RED_SOFT,
                Color32::from_rgb(250, 225, 227),
                Color32::from_rgb(245, 211, 214),
                RED,
                Color32::TRANSPARENT,
            ),
        };
        let response = ui
            .scope(|ui| {
                let widgets = &mut ui.visuals_mut().widgets;
                for (visual, background) in [
                    (&mut widgets.inactive, fill),
                    (&mut widgets.hovered, hover),
                    (&mut widgets.active, pressed),
                ] {
                    visual.weak_bg_fill = if enabled { background } else { PRESSED };
                    visual.bg_fill = visual.weak_bg_fill;
                    visual.bg_stroke = Stroke::new(0.5_f32, border);
                    visual.expansion = 0.0;
                }
                ui.add_enabled_ui(enabled, |ui| {
                    ui.add_sized(
                        [ui.available_width(), 42.0],
                        Button::new(RichText::new(label).size(13.0).strong().color(if enabled {
                            text
                        } else {
                            SUBTLE
                        }))
                        .corner_radius(11.0),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                })
                .inner
            })
            .inner;
        if response.has_focus() {
            focus_ring(ui, response.rect, 11);
        }
        ui.add_space(4.0);
        response.clicked()
    }

    fn action(ui: &mut Ui, label: &str, enabled: bool) -> bool {
        Self::full_button(ui, label, enabled, ButtonKind::Primary)
    }

    fn button_soft(ui: &mut Ui, label: &str, enabled: bool) -> bool {
        Self::full_button(ui, label, enabled, ButtonKind::Secondary)
    }

    fn button_danger(ui: &mut Ui, label: &str, enabled: bool) -> bool {
        Self::full_button(ui, label, enabled, ButtonKind::Danger)
    }

    fn busy_text(&self, ui: &mut Ui) {
        if self.busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(11.0));
                ui.label(
                    RichText::new("Checking the connection…")
                        .size(11.0)
                        .color(SUBTLE),
                );
            });
            ui.add_space(8.0);
        }
    }

    fn moodle_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let subtitle = if self.draft.other_school {
            "Your Moodle account"
        } else {
            "Polytechnique Montréal"
        };
        let online = self.viewing_connected() && matches!(self.check_status, Some(Notice::Ok(_)));
        if self.top_bar(ui, "Moodle", subtitle, online) {
            return;
        }
        self.account_picker(ui, ctx);
        // Re-read after the picker: switching account mutates settings/draft this frame.
        if self.viewing_connected() {
            self.connected_view(ui, ctx, "moodle");
            return;
        }
        let configured = self.settings.moodle().is_some();

        ui.add_space(4.0);

        if !self.draft.use_token {
            Self::field(
                ui,
                "Username",
                &mut self.draft.username,
                "firstname.lastname",
                false,
            );
            Self::secret_field(
                ui,
                "Password",
                &mut self.draft.password,
                "Moodle password",
                &mut self.draft.password_visible,
            );
        } else {
            Self::secret_field(
                ui,
                "Token",
                &mut self.draft.token,
                "Moodle mobile token",
                &mut self.draft.password_visible,
            );
        }
        Self::advanced_row(ui, &mut self.draft.advanced);
        if self.draft.advanced {
            ui.add_space(2.0);
            Self::toggle(
                ui,
                "Use an existing mobile token",
                &mut self.draft.use_token,
            );
            Self::toggle(ui, "Another school", &mut self.draft.other_school);
            if self.draft.other_school {
                Self::field(
                    ui,
                    "Moodle address",
                    &mut self.draft.url,
                    "https://moodle.my-school.edu",
                    false,
                );
            }
            Self::permission_row(ui, "Allow changes", &mut self.draft.allow_write);
        }
        ui.add_space(14.0);
        self.notice_line(ui);
        self.busy_text(ui);

        let site = if self.draft.other_school {
            self.draft.url.clone()
        } else {
            self.settings
                .moodle
                .as_ref()
                .and_then(|m| m.url.clone())
                .unwrap_or_else(|| MOODLE_DEFAULT_URL.to_string())
        };
        let draft = self.draft.clone();
        let ready = !self.busy
            && (draft.use_token && !draft.token.trim().is_empty()
                || !draft.use_token
                    && !draft.username.trim().is_empty()
                    && !draft.password.is_empty());
        let submit = Self::action(
            ui,
            if self.busy {
                "Connecting…"
            } else if configured {
                "Save changes"
            } else {
                "Connect to Moodle"
            },
            ready,
        ) || ready && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if submit {
            self.spawn(ctx.clone(), move |mut fresh| {
                let result = if draft.use_token && !draft.token.trim().is_empty() {
                    crate::mcp::activate_moodle(
                        &mut fresh,
                        &site,
                        "",
                        "",
                        &draft.token,
                        draft.allow_write,
                    )
                } else {
                    crate::mcp::activate_moodle(
                        &mut fresh,
                        &site,
                        &draft.username,
                        &draft.password,
                        "",
                        draft.allow_write,
                    )
                };
                match result {
                    Ok(message) => Message::Done(Notice::Ok(message), NextScreen::Home),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                }
            });
        }
    }

    fn omnivox_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "Omnivox", "Léa / Mio · Read only", self.viewing_connected()) {
            return;
        }
        self.account_picker(ui, ctx);
        if self.viewing_connected() {
            self.connected_view(ui, ctx, "omnivox");
            return;
        }
        let configured = self.settings.omnivox().is_some();
        if self.draft.use_token {
            Self::field(
                ui,
                "Omnivox MCP address",
                &mut self.draft.url,
                "http://127.0.0.1:3000",
                false,
            );
            Self::secret_field(
                ui,
                "Access key",
                &mut self.draft.token,
                "accessKey.txt",
                &mut self.draft.password_visible,
            );
        } else {
            Self::field(
                ui,
                "College address",
                &mut self.draft.url,
                "https://cegepxxx.omnivox.ca",
                false,
            );
            Self::field(
                ui,
                "Student number",
                &mut self.draft.username,
                "0000000",
                false,
            );
            Self::secret_field(
                ui,
                "Password",
                &mut self.draft.password,
                "Omnivox password",
                &mut self.draft.password_visible,
            );
        }
        Self::advanced_row(ui, &mut self.draft.advanced);
        if self.draft.advanced {
            ui.add_space(2.0);
            Self::toggle(
                ui,
                "Use an Omnivox MCP gateway",
                &mut self.draft.use_token,
            );
        }
        if self.draft.use_token {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Run Beat-YT/omnivox-mcp, log in there, then paste its address and access key.",
                    )
                    .size(10.5)
                    .color(SUBTLE),
                )
                .wrap(),
            );
        }
        ui.add_space(10.0);
        self.notice_line(ui);
        self.busy_text(ui);
        let draft = self.draft.clone();
        let ready = !self.busy
            && (draft.use_token && !draft.url.trim().is_empty() && !draft.token.trim().is_empty()
                || !draft.use_token
                    && !draft.url.trim().is_empty()
                    && !draft.username.trim().is_empty()
                    && !draft.password.is_empty());
        let submit = Self::action(
            ui,
            if self.busy {
                "Connecting…"
            } else if configured {
                "Save changes"
            } else {
                "Connect to Omnivox"
            },
            ready,
        ) || ready && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if submit {
            self.spawn(ctx.clone(), move |mut settings| {
                let result = if draft.use_token {
                    crate::omnivox::activate(&mut settings, &draft.url, &draft.token)
                } else {
                    crate::omnivox::activate_login(
                        &mut settings,
                        &draft.url,
                        &draft.username,
                        &draft.password,
                    )
                };
                match result {
                    Ok(message) => Message::Done(Notice::Ok(message), NextScreen::Home),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                }
            });
        }
    }

    fn brightspace_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(
            ui,
            "Brightspace",
            "myCourses · Read only",
            self.viewing_connected(),
        ) {
            return;
        }
        self.account_picker(ui, ctx);
        if self.viewing_connected() {
            self.connected_view(ui, ctx, "brightspace");
            return;
        }
        let configured = self.settings.brightspace().is_some();
        Self::field(
            ui,
            "Site address",
            &mut self.draft.url,
            "https://mycourses.mcgill.ca",
            false,
        );
        if self.draft.use_token {
            Self::secret_field(
                ui,
                "Token or session cookies",
                &mut self.draft.token,
                "Access token or d2lSessionVal=…",
                &mut self.draft.password_visible,
            );
        } else {
            Self::field(
                ui,
                "Username",
                &mut self.draft.username,
                "School username",
                false,
            );
            Self::secret_field(
                ui,
                "Password",
                &mut self.draft.password,
                "Brightspace password",
                &mut self.draft.password_visible,
            );
        }
        Self::advanced_row(ui, &mut self.draft.advanced);
        if self.draft.advanced {
            ui.add_space(2.0);
            Self::toggle(
                ui,
                "Use a token or browser session cookies",
                &mut self.draft.use_token,
            );
        }
        if self.draft.use_token {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Paste an access token, or d2lSessionVal and d2lSecureSessionVal from your browser. Not your school password.",
                    )
                    .size(10.5)
                    .color(SUBTLE),
                )
                .wrap(),
            );
        }
        ui.add_space(10.0);
        self.notice_line(ui);
        self.busy_text(ui);
        let draft = self.draft.clone();
        let ready = !self.busy
            && !draft.url.trim().is_empty()
            && (draft.use_token && !draft.token.trim().is_empty()
                || !draft.use_token
                    && !draft.username.trim().is_empty()
                    && !draft.password.is_empty());
        let submit = Self::action(
            ui,
            if self.busy {
                "Connecting…"
            } else if configured {
                "Save changes"
            } else {
                "Connect to Brightspace"
            },
            ready,
        ) || ready && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if submit {
            self.spawn(ctx.clone(), move |mut settings| {
                let result = if draft.use_token {
                    crate::brightspace::activate(&mut settings, &draft.url, &draft.token)
                } else {
                    crate::brightspace::activate_login(
                        &mut settings,
                        &draft.url,
                        &draft.username,
                        &draft.password,
                    )
                };
                match result {
                    Ok(message) => Message::Done(Notice::Ok(message), NextScreen::Home),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                }
            });
        }
    }

    fn spawn(&mut self, ctx: Context, job: impl FnOnce(Settings) -> Message + Send + 'static) {
        if self.busy {
            return;
        }
        let (sender, receiver): (Sender<Message>, Receiver<Message>) = channel();
        self.rx = Some(receiver);
        self.busy = true;
        self.notice = None;
        let settings = self.settings.reload();
        std::thread::spawn(move || {
            let message = job(settings);
            let _ = sender.send(message);
            ctx.request_repaint();
        });
    }

    fn poll_jobs(&mut self) {
        self.poll_check();
        let Some(receiver) = self.rx.as_ref() else {
            return;
        };
        let message = match receiver.try_recv() {
            Ok(message) => message,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                self.busy = false;
                self.notice = Some(Notice::Err(
                    "The connection was interrupted. Try again.".into(),
                ));
                return;
            }
        };
        self.rx = None;
        self.busy = false;
        match message {
            Message::Done(notice, next) => {
                self.notice = Some(notice);
                match next {
                    NextScreen::Stay => {}
                    NextScreen::Home => {
                        self.draft = Draft::default();
                        self.settings = Settings::load();
                        self.screen = "home";
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod gallery_tests {
    use super::*;
    use crate::config::MoodleSettings;

    #[test]
    fn disclosure_chevrons_are_compact_centered_and_mirrored() {
        let rect = Rect::from_center_size(pos2(40.0, 40.0), Vec2::splat(22.0));
        let mut paths = Vec::new();
        for direction in [Chevron::Down, Chevron::Up] {
            let ctx = Context::default();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                chevron(&ctx.debug_painter(), rect, direction, SUBTLE);
            });
            let lines: Vec<_> = output
                .shapes
                .into_iter()
                .filter_map(|shape| match shape.shape {
                    egui::Shape::LineSegment { points, .. } => Some(points),
                    _ => None,
                })
                .collect();
            assert_eq!(lines.len(), 2);
            let points: Vec<_> = lines.iter().flatten().copied().collect();
            let bounds = Rect::from_points(&points);
            assert_eq!(bounds.center(), rect.center());
            assert!(bounds.width() < 10.0);
            assert!(bounds.height() < 5.0);
            paths.push(points);
        }
        for (down, up) in paths[0].iter().zip(&paths[1]) {
            assert_eq!(down.x, up.x);
            assert!((down.y + up.y - 2.0 * rect.center().y).abs() < 0.001);
        }
    }

    #[test]
    fn visual_theme_uses_neutral_surfaces_and_distinct_action_and_status_colors() {
        let ctx = Context::default();
        configure_context(&ctx);
        let style = ctx.style();
        assert_eq!(style.visuals.panel_fill, CANVAS);
        assert_eq!(style.visuals.override_text_color, Some(INK));
        assert_eq!(style.visuals.hyperlink_color, BLUE);
        assert_ne!(BLUE, GREEN);
        assert_eq!(style.visuals.text_edit_bg_color, Some(Color32::WHITE));
    }

    #[test]
    fn public_link_copy_follows_the_tunnel_state() {
        assert!(public_waiting_copy(&TunnelState::Stopped, true, false).contains("Turn the switch"));
        assert!(public_waiting_copy(&TunnelState::Idle, false, true).contains("Turn the switch"));
        assert!(public_waiting_copy(&TunnelState::Idle, true, false).contains("switch"));
        assert!(
            public_waiting_copy(&TunnelState::Failed("offline".into()), true, true)
                .contains("try again")
        );
        assert!(public_waiting_copy(&TunnelState::Starting, true, true).contains("Opening"));
    }

    #[test]
    fn home_link_uses_the_current_quick_tunnel_origin() {
        let local = "http://127.0.0.1:8420/mcp/test-token";
        assert_eq!(connector_url(8420, "test-token", None), local);
        assert_eq!(
            connector_url(
                8420,
                "test-token",
                Some("https://student.trycloudflare.com")
            ),
            "https://student.trycloudflare.com/mcp/test-token"
        );
    }

    #[test]
    fn tunnel_does_not_start_without_a_service_or_restart_after_explicit_stop() {
        let ctx = Context::default();
        let mut app = App {
            settings: Settings::default(),
            gateway: Gateway {
                port: 8420,
                token: "test-gateway".into(),
            },
            port: 8420,
            logos: None,
            bind_error: None,
            screen: "home",
            draft: Draft::default(),
            account_choices: Vec::new(),
            busy: false,
            notice: None,
            rx: None,
            check_rx: None,
            check_status: None,
            tunnel: Tunnel::default(),
            local_enabled: Arc::new(AtomicBool::new(true)),
            background: Background::default(),
            copied_flash: None,
            scale: 1.0,
            zoom_hit: None,
            zoom_held: None,
            zoom_repeat_at: None,
        };
        app.update_tunnel(&ctx);
        assert_eq!(app.tunnel.state, TunnelState::Idle);
        app.tunnel.state = TunnelState::Ready("https://demo.trycloudflare.com".into());
        app.update_tunnel(&ctx); // No service does not tear down a link that is already up.
        assert_eq!(
            app.tunnel.state,
            TunnelState::Ready("https://demo.trycloudflare.com".into())
        );
        app.settings.moodle = Some(MoodleSettings {
            url: Some(MOODLE_DEFAULT_URL.into()),
            token: Some("demo".into()),
            ..Default::default()
        });
        app.tunnel.stop();
        app.update_tunnel(&ctx);
        assert_eq!(app.tunnel.state, TunnelState::Stopped);
        app.tunnel.state = TunnelState::Failed("offline".into());
        app.update_tunnel(&ctx);
        assert_eq!(app.tunnel.state, TunnelState::Failed("offline".into()));
        app.tunnel.state = TunnelState::Idle;
        app.bind_error = Some("port busy".into());
        app.update_tunnel(&ctx);
        assert_eq!(app.tunnel.state, TunnelState::Idle);

        crate::config::tests::with_temp_config(|| {
            app.bind_error = None;
            app.settings.save().unwrap();
            assert!(app.set_link_preferences(true, false));
            // Restore persisted settings and the startup tunnel state, as after a restart.
            app.settings = Settings::load();
            app.tunnel.state = TunnelState::Idle;
            app.update_tunnel(&ctx);
            assert_eq!(app.tunnel.state, TunnelState::Stopped);
            assert!(app.settings.gateway.local_enabled());
            assert!(!app.settings.gateway.public_enabled());

            assert!(app.set_link_preferences(false, false));
            assert!(!Settings::load().gateway.local_enabled());
            std::fs::remove_file(crate::config::config_path()).unwrap();
            std::fs::create_dir(crate::config::config_path()).unwrap();
            assert!(!app.set_link_preferences(true, true));
            assert!(!app.settings.gateway.public_enabled());
            assert!(!app.settings.gateway.local_enabled());
            assert!(matches!(app.notice, Some(Notice::Err(_))));
        });
    }

    #[test]
    fn saved_service_remains_manageable_when_the_live_check_fails() {
        let (sender, receiver) = channel();
        let mut app = App {
            settings: Settings {
                moodle: Some(MoodleSettings {
                    url: Some(MOODLE_DEFAULT_URL.into()),
                    token: Some("demo-token".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            gateway: Gateway {
                port: 8420,
                token: "demo-gateway".into(),
            },
            port: 8420,
            logos: None,
            bind_error: None,
            screen: "moodle",
            draft: Draft::default(),
            account_choices: Vec::new(),
            busy: false,
            notice: None,
            rx: None,
            check_rx: Some(receiver),
            check_status: None,
            tunnel: Tunnel::default(),
            local_enabled: Arc::new(AtomicBool::new(true)),
            background: Background::default(),
            copied_flash: None,
            scale: 1.0,
            zoom_hit: None,
            zoom_held: None,
            zoom_repeat_at: None,
        };
        assert!(app.viewing_connected());
        sender.send(Err("Moodle did not respond".into())).unwrap();
        app.poll_check();
        assert!(app.check_rx.is_none());
        assert!(matches!(app.check_status, Some(Notice::Err(_))));
        assert!(app.viewing_connected()); // Can still retry, edit or disconnect.

        let (sender, receiver) = channel();
        app.check_rx = Some(receiver);
        app.check_status = None;
        sender.send(Ok("Moodle · student".into())).unwrap();
        app.poll_check();
        assert!(matches!(app.check_status, Some(Notice::Ok(_))));
    }

    #[test]
    fn linux_window_avoids_blocking_vsync_and_identifies_itself() {
        let options = native_options();
        assert_eq!(options.vsync, !cfg!(target_os = "linux"));
        assert_eq!(options.viewport.app_id.as_deref(), Some("student-mcp"));
    }

    #[test]
    fn gallery_is_square_and_centered_at_different_window_sizes() {
        for size in [
            Vec2::new(342.0, 442.0),
            Vec2::new(382.0, 492.0),
            Vec2::new(833.0, 480.0),
        ] {
            let bounds = Rect::from_min_size(pos2(24.0, 24.0), size);
            let layout = GalleryLayout::new(bounds, 110.0);
            let grid = layout.tiles[0]
                .union(layout.tiles[1])
                .union(layout.tiles[2]);
            let content = grid.union(layout.footer);
            let side = ((bounds.width() - 32.0) / 3.0).min(136.0).floor();
            assert!((grid.center().x - bounds.center().x).abs() < 0.02);
            assert!((content.center().x - bounds.center().x).abs() < 0.02);
            assert!(bounds.contains_rect(content) || bounds.height() < content.height());
            assert!((layout.footer.center().x - bounds.center().x).abs() < 0.02);
            assert!(
                (layout.footer.top()
                    - layout.tiles[0].bottom()
                    - (bounds.bottom() - layout.footer.bottom()))
                .abs()
                    < 0.01
            );
            assert!(layout.footer.width() > grid.width());
            assert_eq!(layout.footer.height(), 110.0);
            for tile in layout.tiles {
                assert_eq!(tile.size(), Vec2::splat(side));
            }
            assert_eq!(layout.tiles[1].left() - layout.tiles[0].right(), 16.0);
            assert_eq!(layout.tiles[2].left() - layout.tiles[1].right(), 16.0);
            assert_eq!(layout.tiles[0].top(), layout.tiles[2].top());
        }
    }

    #[test]
    fn zoom_buttons_accept_repeated_clicks_without_pointer_move() {
        let ctx = Context::default();
        configure_context(&ctx);
        let mut app = App {
            settings: Settings::default(),
            gateway: Gateway {
                port: 8420,
                token: "demo".into(),
            },
            port: 8420,
            logos: None,
            bind_error: None,
            screen: "home",
            draft: Draft::default(),
            account_choices: Vec::new(),
            busy: false,
            notice: None,
            rx: None,
            check_rx: None,
            check_status: None,
            tunnel: Tunnel::default(),
            local_enabled: Arc::new(AtomicBool::new(true)),
            background: Background::default(),
            copied_flash: None,
            scale: 1.0,
            zoom_hit: None,
            zoom_held: None,
            zoom_repeat_at: None,
        };
        let screen = Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(400.0, 560.0));
        // Warm layout and capture the plus-button hit target.
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| app.scale_control(ctx),
            );
        }
        let plus = app.zoom_hit.expect("zoom hit rects").1;
        let start = app.scale;
        // Spam-click the same point with no PointerMoved between clicks, while the
        // zoom factor changes under the cursor (the original failure mode).
        for _ in 0..5 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerButton {
                            pos: plus.center(),
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::default(),
                        },
                        egui::Event::PointerButton {
                            pos: plus.center(),
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    app.apply_scale(ctx);
                    app.scale_control(ctx);
                },
            );
        }
        assert!(
            app.scale > start + 0.35,
            "expected repeated zoom-in without moving the pointer, got {start} → {}",
            app.scale
        );
    }

    fn collect_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, f32)> {
        let mut texts = Vec::new();
        fn walk(shape: &egui::Shape, texts: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(t) => {
                    texts.push((t.galley.text().to_string(), t.pos.y));
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, texts);
                    }
                }
                _ => {}
            }
        }
        for shape in shapes {
            walk(&shape.shape, &mut texts);
        }
        texts
    }

    fn render_body(app: &mut App, ctx: &Context, size: Vec2) -> Vec<(String, f32)> {
        let mut texts = Vec::new();
        for _ in 0..2 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default()
                        .frame(egui::Frame::new().fill(CANVAS).inner_margin(24.0))
                        .show(ctx, |ui| {
                            let bounds = form_bounds(ui.available_rect_before_wrap());
                            ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(bounds)
                                    .layout(Layout::top_down(egui::Align::Min)),
                                |ui| {
                                    ui.set_width(bounds.width());
                                    app.body(ui, ctx);
                                },
                            );
                        });
                },
            );
            texts = collect_texts(&output.shapes);
        }
        texts
    }

    #[test]
    fn first_connect_hides_account_picker_and_shows_the_form() {
        crate::config::tests::with_temp_config(|| {
            let ctx = Context::default();
            configure_context(&ctx);
            let mut app = App {
                settings: Settings::default().for_account("moodle", "moodle"),
                gateway: Gateway {
                    port: 8420,
                    token: "demo".into(),
                },
                port: 8420,
                logos: Some(crate::logos::Logos::load(&ctx)),
                bind_error: None,
                screen: "moodle",
                draft: Draft::default(),
                account_choices: Vec::new(),
                busy: false,
                notice: None,
                rx: None,
                check_rx: None,
                check_status: None,
                tunnel: Tunnel::default(),
                local_enabled: Arc::new(AtomicBool::new(true)),
                background: Background::default(),
                copied_flash: None,
                scale: 1.0,
                zoom_hit: None,
                zoom_held: None,
                zoom_repeat_at: None,
            };
            let texts = render_body(&mut app, &ctx, Vec2::new(400.0, 560.0));
            let labels: Vec<_> = texts.iter().map(|(t, _)| t.as_str()).collect();
            assert!(labels.contains(&"Moodle"), "{labels:?}");
            assert!(labels.contains(&"Username"), "{labels:?}");
            assert!(labels.contains(&"Password"), "{labels:?}");
            assert!(labels.contains(&"Connect to Moodle"), "{labels:?}");
            assert!(
                !labels.iter().any(|t| t.contains("New account")),
                "picker must stay hidden with no saved accounts: {labels:?}"
            );
            // Header (title) must appear above the credential fields.
            let title_y = texts.iter().find(|(t, _)| t == "Moodle").unwrap().1;
            let user_y = texts.iter().find(|(t, _)| t == "Username").unwrap().1;
            assert!(
                title_y < user_y,
                "title {title_y} should be above Username {user_y}"
            );
        });
    }

    #[test]
    fn new_account_draft_shows_form_under_the_service_header() {
        crate::config::tests::with_temp_config(|| {
            let mut root = Settings::default();
            root.moodle = Some(crate::config::MoodleSettings {
                url: Some(MOODLE_DEFAULT_URL.into()),
                token: Some("saved-token".into()),
                allow_write: Some(false),
            });
            root.save().unwrap();
            let listed = Settings::load().account_list("moodle");
            assert_eq!(
                listed.len(),
                1,
                "saved primary account must list before render: {listed:?} path={:?}",
                crate::config::config_path()
            );

            let ctx = Context::default();
            configure_context(&ctx);
            let mut app = App {
                settings: Settings::load().for_account("moodle", "brand-new-id"),
                gateway: Gateway {
                    port: 8420,
                    token: "demo".into(),
                },
                port: 8420,
                logos: Some(crate::logos::Logos::load(&ctx)),
                bind_error: None,
                screen: "moodle",
                draft: Draft {
                    editing: true,
                    ..Draft::default()
                },
                account_choices: listed.clone(),
                busy: false,
                notice: None,
                rx: None,
                check_rx: None,
                check_status: None,
                tunnel: Tunnel::default(),
                local_enabled: Arc::new(AtomicBool::new(true)),
                background: Background::default(),
                copied_flash: None,
                scale: 1.0,
                zoom_hit: None,
                zoom_held: None,
                zoom_repeat_at: None,
            };
            let texts = render_body(&mut app, &ctx, Vec2::new(400.0, 700.0));
            let labels: Vec<_> = texts.iter().map(|(t, _)| t.as_str()).collect();
            assert!(labels.contains(&"Moodle"), "{labels:?}");
            assert!(labels.contains(&"New account"), "{labels:?}");
            assert!(labels.contains(&"Username"), "{labels:?}");
            assert!(labels.contains(&"Connect to Moodle"), "{labels:?}");
            assert!(
                !labels.iter().any(|t| *t == "+ Add account"),
                "must not offer another add while drafting: {labels:?}"
            );
            let title_y = texts.iter().find(|(t, _)| t == "Moodle").unwrap().1;
            let picker_y = texts.iter().find(|(t, _)| t == "New account").unwrap().1;
            let user_y = texts.iter().find(|(t, _)| t == "Username").unwrap().1;
            assert!(
                title_y < picker_y && picker_y < user_y,
                "expected header → picker → form, got title={title_y} picker={picker_y} user={user_y}"
            );
        });
    }

    #[test]
    fn gallery_renders_all_icons_without_a_display() {
        let ctx = Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let logos = Logos::load(ctx);
            CentralPanel::default().show(ctx, |ui| {
                let layout = GalleryLayout::new(
                    Rect::from_min_size(pos2(24.0, 24.0), Vec2::new(382.0, 492.0)),
                    110.0,
                );
                for (index, (texture, name)) in [
                    (&logos.moodle, "Moodle"),
                    (&logos.omnivox, "Omnivox"),
                    (&logos.brightspace, "Brightspace"),
                ]
                .into_iter()
                .enumerate()
                {
                    assert!(!App::tile(ui, texture, name, layout.tiles[index], false));
                }
            });
        });
        let images: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                    Some(mesh)
                }
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 3);
        for image in images {
            assert!(image.vertices.iter().any(|v| v.uv == pos2(0.0, 0.0)));
            assert!(image.vertices.iter().any(|v| v.uv == pos2(1.0, 1.0)));
        }
    }
}
