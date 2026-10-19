use crate::config::{mcp_endpoint, Gateway, Settings};
use crate::funnel::{self, TailscaleInfo};
use crate::logos::Logos;
use eframe::egui::{
    self, pos2, Align2, Button, CentralPanel, Color32, Context, CursorIcon, FontId, Grid, Rect,
    RichText, Sense, Stroke, TextEdit, TextureHandle, Ui, Vec2,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

// A calm, almost-monochrome white palette.
const INK: Color32 = Color32::from_rgb(26, 26, 26);
const SUBTLE: Color32 = Color32::from_rgb(112, 112, 112);
const BORDER: Color32 = Color32::from_rgb(225, 225, 225);
const HOVER: Color32 = Color32::from_rgb(246, 246, 246);
const FIELD: Color32 = Color32::from_rgb(252, 252, 252);
const GREEN: Color32 = Color32::from_rgb(52, 168, 83);
const RED: Color32 = Color32::from_rgb(224, 67, 67);

const MOODLE_DEFAULT_URL: &str = "https://moodle.polymtl.ca";

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Err(String),
}

enum Message {
    Done(Notice, NextScreen),
    Funnel(TailscaleInfo, Notice),
}

enum NextScreen {
    Stay,
    Home,
    Links,
}

#[derive(Default, Clone)]
struct Draft {
    url: String,
    username: String,
    password: String,
    token: String,
    token_id: String,
    secret: String,
    allow_write: bool,
    other_school: bool,
    use_token: bool,
    advanced: bool,
}

pub struct App {
    settings: Settings,
    gateway: Gateway,
    port: u16,
    logos: Option<Logos>,
    bind_error: Option<String>,
    screen: &'static str, // "home" | "moodle" | "proxmox" | "links"
    draft: Draft,
    busy: bool,
    notice: Option<Notice>,
    rx: Option<Receiver<Message>>,
    funnel_info: Option<TailscaleInfo>,
    copied_flash: Option<Instant>,
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
    for attempt in 0..10 {
        let port = base_port + attempt;
        match crate::http::bind(port) {
            Ok(listener) => {
                chosen = Some(port);
                std::thread::spawn(move || {
                    if let Err(message) = crate::http::serve(listener) {
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("student-mcp")
            .with_inner_size([430.0, 540.0])
            .with_min_inner_size([390.0, 490.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "student-mcp",
        options,
        Box::new(move |cc| {
            let mut visuals = egui::Visuals::light();
            visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
            visuals.widgets.inactive.bg_fill = Color32::WHITE;
            visuals.widgets.hovered.bg_fill = HOVER;
            visuals.widgets.active.bg_fill = HOVER;
            visuals.selection.bg_fill = Color32::from_rgb(225, 234, 244);
            cc.egui_ctx.set_visuals(visuals);
            let mut app = App {
                settings: Settings::load(),
                gateway,
                port,
                logos: None,
                bind_error,
                screen: "home",
                draft: Draft::default(),
                busy: false,
                notice: None,
                rx: None,
                funnel_info: None,
                copied_flash: None,
            };
            app.logos = Some(Logos::load(&cc.egui_ctx));
            Ok(Box::new(app))
        }),
    );
    if let Err(error) = result {
        eprintln!("Fenêtre impossible : {error:?} — le serveur MCP stdio reste disponible (student-mcp sans commande).");
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(24.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.body(ui, ctx));
            });
    }
}

impl App {
    fn body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match self.screen {
            "moodle" => self.moodle_screen(ui, ctx),
            "proxmox" => self.proxmox_screen(ui, ctx),
            "links" => self.links_screen(ui, ctx),
            _ => self.home_screen(ui),
        }
    }

    // ---------- home: the gallery ----------

    fn home_screen(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Tes services").size(18.0).strong().color(INK));
        });
        ui.add_space(20.0);

        let Some(logos) = self.logos.as_ref() else {
            return;
        };
        let moodle_on = self.settings.moodle().is_some();
        let proxmox_on = self.settings.proxmox().is_some();

        let tile_size = 136.0;
        let gallery_width = tile_size * 2.0 + 18.0;
        ui.add_space(((ui.available_width() - gallery_width) * 0.5).max(0.0));

        let (moodle_clicked, proxmox_clicked) = Grid::new("gallery")
            .spacing([18.0, 18.0])
            .show(ui, |grid| {
                let moodle_clicked =
                    Self::tile(grid, &logos.moodle, "Moodle", tile_size, moodle_on, false);
                let proxmox_clicked = Self::tile(
                    grid,
                    &logos.proxmox,
                    "Proxmox",
                    tile_size,
                    proxmox_on,
                    false,
                );
                grid.end_row();
                Self::tile(grid, &logos.poly, "Dossier", tile_size, false, true);
                Self::tile(grid, &logos.outlook, "Courriel", tile_size, false, true);
                grid.end_row();
                (moodle_clicked, proxmox_clicked)
            })
            .inner;

        if moodle_clicked || proxmox_clicked {
            self.draft = Draft::default();
            self.screen = if moodle_clicked { "moodle" } else { "proxmox" };
            self.notice = None;
        }

        ui.add_space(18.0);
        if let Some(error) = &self.bind_error {
            ui.colored_label(RED, format!("Serveur local indisponible : {error}"));
        }
        if self.settings.moodle().is_some() || self.settings.proxmox().is_some() {
            ui.separator();
            ui.add_space(10.0);
            if ui
                .add(
                    Button::new(RichText::new("Voir mon lien  →").size(13.0).color(INK))
                        .frame(false),
                )
                .clicked()
            {
                self.screen = "links";
            }
        }
    }

    /// Square service tile: square app logo, label beneath, and a fine outline.
    fn tile(
        ui: &mut Ui,
        texture: &TextureHandle,
        name: &str,
        size: f32,
        active: bool,
        soon: bool,
    ) -> bool {
        let clickable = !soon;
        let (rect, raw) = ui.allocate_exact_size(
            Vec2::splat(size),
            if clickable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let response = if clickable {
            raw.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            raw
        };
        let hovered = clickable && response.hovered();

        ui.painter()
            .rect_filled(rect, 12.0, if hovered { HOVER } else { Color32::WHITE });
        ui.painter().rect_stroke(
            rect,
            12.0,
            Stroke::new(
                1.0_f32,
                if hovered {
                    Color32::from_rgb(205, 205, 205)
                } else {
                    BORDER
                },
            ),
            egui::StrokeKind::Inside,
        );
        if active {
            ui.painter()
                .circle_filled(rect.right_top() - Vec2::new(13.0, 13.0), 3.5, GREEN);
        }

        // Crop to a square viewport so wide wordmarks and square icons align equally.
        let logo_side = size * 0.54;
        let logo_rect =
            Rect::from_center_size(rect.center() - Vec2::new(0.0, 9.0), Vec2::splat(logo_side));
        let uv = square_uv(texture.size_vec2());
        let tint = if soon {
            Color32::from_white_alpha(105)
        } else {
            Color32::WHITE
        };
        ui.painter().image(texture.id(), logo_rect, uv, tint);

        let name_color = if soon { SUBTLE } else { INK };
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 17.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(13.0),
            name_color,
        );

        response.clicked()
    }

    // ---------- links below the gallery ----------

    fn links_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "Ton connecteur") {
            return;
        }
        let local = mcp_endpoint(
            &format!("http://127.0.0.1:{}", self.port),
            &self.gateway.token,
        );
        ui.label(RichText::new("Sur cet ordinateur").size(12.0).color(SUBTLE));
        ui.add_space(5.0);
        self.copy_row(ui, ctx, &local);
        ui.add_space(24.0);
        self.funnel_links(ui, ctx);
        self.notice_line(ui);
    }

    fn funnel_links(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.label(
            RichText::new("Pour Claude sur le web")
                .size(11.5)
                .color(SUBTLE),
        );
        match self.funnel_info.clone() {
            Some(info) if info.funnel_on => {
                let url = info
                    .host
                    .as_deref()
                    .map(|host| funnel::public_connector_url(host, &self.gateway.token))
                    .unwrap_or_default();
                self.copy_row(ui, ctx, &url);
                if ui
                    .add_enabled(
                        !self.busy,
                        Button::new(
                            RichText::new("Retirer le lien public")
                                .size(11.5)
                                .color(SUBTLE),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    self.spawn(ctx.clone(), || {
                        let note = funnel::funnel_disable();
                        Message::Funnel(funnel::info(), Notice::Ok(note))
                    });
                }
            }
            _ => {
                if ui
                    .add_enabled(
                        !self.busy && self.bind_error.is_none(),
                        Button::new("Publier avec Tailscale ↗"),
                    )
                    .clicked()
                {
                    let port = self.port;
                    self.spawn(ctx.clone(), move || match funnel::funnel_enable(port) {
                        Ok(note) => Message::Funnel(funnel::info(), Notice::Ok(note)),
                        Err(message) => Message::Funnel(funnel::info(), Notice::Err(message)),
                    });
                }
            }
        }
        if self.busy {
            ui.label(
                RichText::new("Connexion en cours…")
                    .size(11.5)
                    .color(SUBTLE),
            );
        }
    }

    fn copy_row(&mut self, ui: &mut Ui, ctx: &egui::Context, text: &str) {
        ui.horizontal(|row| {
            let preview = if text.len() > 42 {
                format!("{}…", &text[..39])
            } else {
                text.to_string()
            };
            row.add_sized(
                [row.available_width() - 84.0, 28.0],
                egui::Label::new(RichText::new(preview).monospace().size(11.0).color(INK))
                    .truncate(),
            );
            let copied = self
                .copied_flash
                .map(|at| at.elapsed().as_secs_f32() < 1.2)
                .unwrap_or(false);
            if row
                .add(Button::new(
                    RichText::new(if copied { "Copié ✓" } else { "Copier" })
                        .size(11.0)
                        .color(INK),
                ))
                .clicked()
            {
                ctx.copy_text(text.to_string());
                self.copied_flash = Some(Instant::now());
                ctx.request_repaint_after(std::time::Duration::from_millis(1300));
            }
        });
    }

    fn notice_line(&mut self, ui: &mut Ui) {
        if let Some(notice) = self.notice.clone() {
            let (text, color) = match notice {
                Notice::Ok(text) => (text, GREEN),
                Notice::Err(text) => (text, RED),
            };
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(text).size(11.5).color(color)).wrap());
        }
    }

    // ---------- service screens (white, minimal) ----------

    fn top_bar(&mut self, ui: &mut Ui, title: &str) -> bool {
        let back = ui
            .add(Button::new(RichText::new("←  Services").size(12.0).color(SUBTLE)).frame(false))
            .clicked();
        if back {
            self.screen = "home";
            self.notice = None;
            return true;
        }
        ui.add_space(28.0);
        ui.label(RichText::new(title).size(22.0).strong().color(INK));
        ui.add_space(20.0);
        false
    }

    fn field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, secret: bool) {
        ui.label(RichText::new(label).size(11.5).color(SUBTLE));
        let frame = egui::Frame::new()
            .fill(FIELD)
            .corner_radius(8.0)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(6.0);
        frame.show(ui, |inner| {
            inner.add(
                TextEdit::singleline(buffer)
                    .hint_text(hint)
                    .desired_width(inner.available_width())
                    .frame(false)
                    .password(secret),
            );
        });
        ui.add_space(8.0);
    }

    fn toggle(ui: &mut Ui, label: &str, value: &mut bool) {
        ui.add(egui::Checkbox::new(
            value,
            RichText::new(label).size(11.5).color(SUBTLE),
        ));
        ui.add_space(4.0);
    }

    fn action(ui: &mut Ui, label: &str, enabled: bool) -> bool {
        let button = Button::new(
            RichText::new(label)
                .color(if enabled { Color32::WHITE } else { SUBTLE })
                .size(13.5),
        )
        .fill(if enabled { INK } else { FIELD })
        .corner_radius(8.0);
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized([ui.available_width(), 40.0], button)
            })
            .inner;
        ui.add_space(4.0);
        response.clicked()
    }

    fn busy_text(&self, ui: &mut Ui) {
        if self.busy {
            ui.label(RichText::new("…").size(11.5).color(SUBTLE));
        }
    }

    fn moodle_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "Moodle") {
            return;
        }
        let configured = self.settings.moodle.is_some();
        if !self.draft.use_token {
            Self::field(
                ui,
                "Nom d'utilisateur",
                &mut self.draft.username,
                "prenom.nom",
                false,
            );
            Self::field(
                ui,
                "Mot de passe",
                &mut self.draft.password,
                "Mot de passe Moodle",
                true,
            );
        }
        if ui
            .add(
                Button::new(
                    RichText::new(if self.draft.advanced {
                        "Options avancées  −"
                    } else {
                        "Options avancées  +"
                    })
                    .size(11.5)
                    .color(SUBTLE),
                )
                .frame(false),
            )
            .clicked()
        {
            self.draft.advanced = !self.draft.advanced;
        }
        if self.draft.advanced {
            ui.add_space(6.0);
            Self::toggle(ui, "Autre école", &mut self.draft.other_school);
            if self.draft.other_school {
                Self::field(
                    ui,
                    "Adresse Moodle",
                    &mut self.draft.url,
                    "https://moodle.mon-ecole.ca",
                    false,
                );
            }
            Self::toggle(
                ui,
                "Utiliser un token au lieu du mot de passe",
                &mut self.draft.use_token,
            );
            if self.draft.use_token {
                Self::field(ui, "Token", &mut self.draft.token, "Token Moodle", true);
            }
            Self::toggle(
                ui,
                "Autoriser les modifications",
                &mut self.draft.allow_write,
            );
        }
        ui.add_space(16.0);
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
        if ui
            .add_enabled(
                ready,
                Button::new(
                    RichText::new(if configured {
                        "Mettre à jour"
                    } else {
                        "Continuer"
                    })
                    .color(if ready { Color32::WHITE } else { SUBTLE }),
                )
                .fill(if ready { INK } else { FIELD })
                .corner_radius(8.0)
                .min_size(Vec2::new(ui.available_width(), 40.0)),
            )
            .clicked()
        {
            self.spawn(ctx.clone(), move || {
                let mut fresh = Settings::load();
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
                    Ok(message) => Message::Done(Notice::Ok(message), NextScreen::Links),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                }
            });
        }
        ui.add_space(10.0);
        if configured && self.button_plain(ui, "Se déconnecter") && !self.busy {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "moodle");
                Message::Done(Notice::Ok("Moodle déconnecté.".into()), NextScreen::Home)
            });
        }
    }

    fn proxmox_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "Proxmox") {
            return;
        }
        let configured = self.settings.proxmox.is_some();

        Self::field(
            ui,
            "Adresse du serveur",
            &mut self.draft.url,
            "https://mon-pve:8006",
            false,
        );
        Self::field(
            ui,
            "ID du token API",
            &mut self.draft.token_id,
            "prenom.nom@pam!student-mcp",
            false,
        );
        Self::field(
            ui,
            "Secret du token",
            &mut self.draft.secret,
            "généré dans Datacenter → API Tokens",
            true,
        );
        if ui
            .add(
                Button::new(
                    RichText::new(if self.draft.advanced {
                        "Options avancées  −"
                    } else {
                        "Options avancées  +"
                    })
                    .size(11.5)
                    .color(SUBTLE),
                )
                .frame(false),
            )
            .clicked()
        {
            self.draft.advanced = !self.draft.advanced;
        }
        if self.draft.advanced {
            Self::toggle(
                ui,
                "Autoriser start/stop/reboot",
                &mut self.draft.allow_write,
            );
        }
        ui.add_space(16.0);
        self.notice_line(ui);
        self.busy_text(ui);

        let draft = self.draft.clone();
        let ready = !self.busy
            && !draft.url.trim().is_empty()
            && !draft.token_id.trim().is_empty()
            && !draft.secret.trim().is_empty();
        if Self::action(
            ui,
            if configured {
                "Mettre à jour"
            } else {
                "Continuer"
            },
            ready,
        ) {
            self.spawn(ctx.clone(), move || {
                let mut fresh = Settings::load();
                match crate::mcp::activate_proxmox(
                    &mut fresh,
                    &draft.url,
                    &draft.token_id,
                    &draft.secret,
                    draft.allow_write,
                ) {
                    Ok(message) => Message::Done(Notice::Ok(message), NextScreen::Links),
                    Err(message) => Message::Done(Notice::Err(message), NextScreen::Stay),
                }
            });
        }
        if configured && self.button_plain(ui, "Se déconnecter") && !self.busy {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "proxmox");
                Message::Done(Notice::Ok("Proxmox déconnecté.".into()), NextScreen::Home)
            });
        }
    }

    fn button_plain(&mut self, ui: &mut Ui, label: &str) -> bool {
        ui.add_space(2.0);
        let response = ui.add(
            Button::new(RichText::new(label).size(12.0).color(SUBTLE))
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0_f32, BORDER)),
        );
        response.clicked()
    }

    // ---------- plumbing ----------

    fn spawn(&mut self, ctx: Context, job: impl FnOnce() -> Message + Send + 'static) {
        let (sender, receiver): (Sender<Message>, Receiver<Message>) = channel();
        self.rx = Some(receiver);
        self.busy = true;
        std::thread::spawn(move || {
            let message = job();
            let _ = sender.send(message);
            ctx.request_repaint();
        });
    }

    fn poll_jobs(&mut self) {
        let Some(receiver) = self.rx.as_ref() else {
            return;
        };
        let Ok(message) = receiver.try_recv() else {
            return;
        };
        self.rx = None;
        self.busy = false;
        match message {
            Message::Done(notice, next) => {
                self.notice = Some(notice);
                match next {
                    NextScreen::Stay => {}
                    NextScreen::Home => {
                        self.settings = Settings::load();
                        self.screen = "home";
                    }
                    NextScreen::Links => {
                        self.settings = Settings::load();
                        self.screen = "links";
                    }
                }
            }
            Message::Funnel(info, notice) => {
                self.funnel_info = Some(info);
                match notice {
                    Notice::Ok(text) => self.notice = Some(Notice::Ok(text)),
                    Notice::Err(text) => self.notice = Some(Notice::Err(text)),
                }
            }
        }
    }
}

fn square_uv(size: Vec2) -> Rect {
    let side = size.x.min(size.y);
    let u = side / size.x;
    let v = side / size.y;
    Rect::from_min_max(
        pos2((1.0 - u) * 0.5, (1.0 - v) * 0.5),
        pos2((1.0 + u) * 0.5, (1.0 + v) * 0.5),
    )
}
