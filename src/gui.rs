use crate::config::{mcp_endpoint, Gateway, Settings};
use crate::funnel::{self, TailscaleInfo};
use crate::logos::Logos;
use eframe::egui::{
    self, pos2, Align2, Button, CentralPanel, Color32, Context, CursorIcon, FontId, Grid, Layout,
    Rect, RichText, Sense, Stroke, TextEdit, TextureHandle, Ui, Vec2,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

// A calm, almost-monochrome white palette.
const INK: Color32 = Color32::from_rgb(26, 26, 26);
const SUBTLE: Color32 = Color32::from_rgb(155, 155, 155);
const BORDER: Color32 = Color32::from_rgb(232, 232, 232);
const HOVER: Color32 = Color32::from_rgb(247, 247, 247);
const FIELD: Color32 = Color32::from_rgb(250, 250, 250);
const GREEN: Color32 = Color32::from_rgb(52, 168, 83);
const RED: Color32 = Color32::from_rgb(224, 67, 67);

const MOODLE_DEFAULT_URL: &str = "https://moodle.polymtl.ca";

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Err(String),
    Info,
}

enum Message {
    Done(Notice, bool), // bool: reload settings afterwards
    Funnel(TailscaleInfo, Notice),
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
}

pub struct App {
    settings: Settings,
    gateway: Gateway,
    port: u16,
    logos: Option<Logos>,
    bind_error: Option<String>,
    screen: &'static str, // "home" | "moodle" | "proxmox"
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
            .with_inner_size([440.0, 620.0])
            .with_min_inner_size([400.0, 520.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "student-mcp",
        options,
        Box::new(move |cc| {
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
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(20.0))
            .show(ctx, |ui| self.body(ui, ctx));
    }
}

impl App {
    fn body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match self.screen {
            "moodle" => self.moodle_screen(ui, ctx),
            "proxmox" => self.proxmox_screen(ui, ctx),
            _ => self.home_screen(ui, ctx),
        }
    }

    // ---------- home: the gallery ----------

    fn home_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal(|row| {
            row.label(RichText::new("student-mcp").size(15.0).strong().color(INK));
            row.with_layout(Layout::right_to_left(egui::Align::Center), |row| {
                let color = if self.bind_error.is_none() { GREEN } else { RED };
                let (rect, _) = row.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                row.painter().circle_filled(rect.center(), 3.5, color);
                if let Some(error) = &self.bind_error {
                    row.label(RichText::new(error).size(11.0).color(RED));
                }
            });
        });
        ui.add_space(18.0);

        let Some(owned) = self.logos.clone() else { return };
        let logos = &owned;
        let moodle_on = self.settings.moodle().is_some();
        let proxmox_on = self.settings.proxmox().is_some();

        let width = ui.available_width();
        let card_size = Vec2::new((width - 16.0) / 2.0, 148.0);

        Grid::new("gallery").spacing([16.0, 16.0]).show(ui, |grid| {
            if self
                .tile(grid, &logos.moodle, "Moodle", card_size, moodle_on, false)
            {
                self.draft = Draft::default();
                self.screen = "moodle";
                self.notice = None;
            }
            if self.tile(grid, &logos.proxmox, "Proxmox", card_size, proxmox_on, false) {
                self.draft = Draft::default();
                self.screen = "proxmox";
                self.notice = None;
            }
            grid.end_row();
            self.tile(grid, &logos.poly, "Dossier", card_size, false, true);
            self.tile(grid, &logos.outlook, "Courriel", card_size, false, true);
            grid.end_row();
        });

        ui.add_space(20.0);
        self.links_section(ui, ctx);
    }

    /// One gallery tile: white card, logo, name under it, green dot when active.
    fn tile(&mut self, ui: &mut Ui, texture: &TextureHandle, name: &str, size: Vec2, active: bool, soon: bool) -> bool {
        let clickable = !soon;
        let (rect, raw) = ui.allocate_exact_size(size, if clickable { Sense::click() } else { Sense::hover() });
        let response = if clickable { raw.on_hover_cursor(CursorIcon::PointingHand) } else { raw };
        let hovered = clickable && response.hovered();

        // card background + border
        ui.painter()
            .rect_filled(rect, 14.0, if hovered { HOVER } else { Color32::WHITE });
        if clickable {
            ui.painter().rect_stroke(
                rect,
                14.0,
                Stroke::new(1.0_f32, if hovered { Color32::from_rgb(206, 206, 206) } else { BORDER }),
                egui::StrokeKind::Middle,
            );
        }        // active dot, top-right
        if active {
            ui.painter()
                .circle_filled(rect.right_top() - Vec2::new(14.0, 14.0), 4.0, GREEN);
        }

        // logo fitted in the upper area of the card
        let area = Vec2::new(size.x - 36.0, size.y - 52.0);
        let tex_size = texture.size_vec2();
        let scale = (area.x / tex_size.x).min(area.y / tex_size.y).min(1.6);
        let logo = tex_size * scale * (if soon { 0.72 } else { 1.0 });
        let logo_min = rect.min + Vec2::new((size.x - logo.x) / 2.0, (size.y - 44.0 - logo.y) / 2.0 + 6.0);
        let tint = if soon { Color32::from_rgba_unmultiplied(215, 215, 215, 170) } else { Color32::WHITE };
        ui.painter()
            .image(texture.id(), Rect::from_min_size(logo_min, logo), uv_full(), tint);

        // name under the logo
        let name_color = if soon { SUBTLE } else { INK };
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 16.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(13.5),
            name_color,
        );

        response.clicked()
    }

    // ---------- links below the gallery ----------

    fn links_section(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let any = self.settings.moodle().is_some() || self.settings.proxmox().is_some();
        if !any {
            ui.label(RichText::new("Connecte un service pour obtenir ton lien MCP.").size(12.0).color(SUBTLE));
            return;
        }

        let local = mcp_endpoint(&format!("http://127.0.0.1:{}", self.port), &self.gateway.token);
        ui.label(RichText::new("Lien MCP").size(11.5).strong().color(SUBTLE));
        self.copy_row(ui, ctx, &local);
        ui.add_space(10.0);
        self.funnel_links(ui, ctx);
        self.notice_line(ui);
    }

    fn funnel_links(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match self.funnel_info.clone().filter(|info| info.present && info.logged_in) {
            None => {
                ui.horizontal(|row| {
                    if row.add(Button::new(RichText::new("Publier avec Tailscale").size(12.5).color(INK)).small()).clicked() && !self.busy {
                        self.spawn(ctx.clone(), || Message::Funnel(funnel::info(), Notice::Info));
                    }
                    row.label(RichText::new("pour Claude web").size(11.5).color(SUBTLE));
                });
            }
            Some(info) => {
                if info.funnel_on {
                    let url = info
                        .host
                        .as_deref()
                        .map(|host| funnel::public_connector_url(host, &self.gateway.token))
                        .unwrap_or_default();
                    ui.label(RichText::new("Lien public (Claude web — connecteur personnalisé)").size(11.5).strong().color(SUBTLE));
                    self.copy_row(ui, ctx, &url);
                    ui.add_space(4.0);
                    if !self.busy && ui.add(Button::new(RichText::new("Retirer le lien public").size(11.5).color(SUBTLE)).small()).clicked() {
                        self.spawn(ctx.clone(), || {
                            let note = funnel::funnel_disable();
                            Message::Funnel(funnel::info(), Notice::Ok(note))
                        });
                    }
                } else {
                    ui.horizontal(|row| {
                        if row.add(Button::new(RichText::new("Publier via Tailscale").size(12.5).color(INK)).small()).clicked() && !self.busy {
                            let port = self.port;
                            self.spawn(ctx.clone(), move || match funnel::funnel_enable(port) {
                                Ok(note) => Message::Funnel(funnel::info(), Notice::Ok(note)),
                                Err(message) => Message::Funnel(funnel::info(), Notice::Err(message)),
                            });
                        }
                        if row.add(Button::new(RichText::new("Actualiser").size(11.5).color(SUBTLE)).small()).clicked() && !self.busy {
                            self.spawn(ctx.clone(), || Message::Funnel(funnel::info(), Notice::Info));
                        }
                    });
                    if let Some(message) = &info.message {
                        ui.label(RichText::new(message).size(11.0).color(SUBTLE));
                    }
                }
            }
        }
        if self.busy {
            ui.label(RichText::new("…").size(11.5).color(SUBTLE));
        }
    }

    fn copy_row(&mut self, ui: &mut Ui, ctx: &egui::Context, text: &str) {
        ui.horizontal(|row| {
            row.label(
                RichText::new(if text.len() > 52 { format!("{}…", &text[..46]) } else { text.to_string() })
                    .monospace()
                    .size(11.5)
                    .color(INK),
            );
            let copied = self.copied_flash.map(|at| at.elapsed().as_secs_f32() < 1.2).unwrap_or(false);
            if row.add(Button::new(RichText::new(if copied { "Copié ✓" } else { "Copier" }).size(11.0).color(INK)).small()).clicked() {
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
                Notice::Info => return,
            };
            ui.add_space(6.0);
            ui.add_sized(
                [ui.available_width(), 0.0],
                egui::Label::new(RichText::new(text).size(11.5).color(color)).wrap(),
            );
        }
    }

    // ---------- service screens (white, minimal) ----------

    fn top_bar(&mut self, ui: &mut Ui, title: &str) {
        if ui.add(Button::new(RichText::new("←").size(15.0).color(INK)).small()).clicked() {
            self.screen = "home";
            self.notice = None;
        }
        ui.add_space(10.0);
        ui.label(RichText::new(title).size(20.0).strong().color(INK));
        ui.add_space(12.0);
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
                    .password(secret),
            );
        });
        ui.add_space(8.0);
    }

    fn toggle(ui: &mut Ui, label: &str, value: &mut bool) {
        ui.add(egui::Checkbox::new(value, RichText::new(label).size(11.5).color(SUBTLE)));
        ui.add_space(4.0);
    }

    fn action(ui: &mut Ui, label: &str) -> bool {
        let spacing = ui.spacing_mut();
        spacing.button_padding = Vec2::new(14.0, 8.0);
        let button = Button::new(RichText::new(label).color(Color32::WHITE).size(13.5)).fill(INK);
        let response = ui.add_sized([ui.available_width(), 34.0], button);
        ui.add_space(4.0);
        response.clicked()
    }

    fn busy_text(&self, ui: &mut Ui) {
        if self.busy {
            ui.label(RichText::new("…").size(11.5).color(SUBTLE));
        }
    }

    fn moodle_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.top_bar(ui, "Moodle");
        let configured = self.settings.moodle.is_some();

        if Self::toggle_alt(ui, "C'est une autre école", &mut self.draft.other_school) {
            Self::field(ui, "Adresse Moodle", &mut self.draft.url, "https://moodle.mon-ecole.ca", false);
        }
        Self::field(ui, "Nom d'utilisateur", &mut self.draft.username, "prenom.nom", false);
        Self::field(ui, "Mot de passe", &mut self.draft.password, "", true);
        if Self::toggle_alt(ui, "Coller un token à la place", &mut self.draft.use_token) {
            Self::field(ui, "Token", &mut self.draft.token, "token du service mobile", true);
        }
        ui.add_space(2.0);
        Self::toggle(ui, "Autoriser aussi les modifications (dépôt de devoirs…)", &mut self.draft.allow_write);
        ui.add_space(4.0);
        self.notice_line(ui);
        self.busy_text(ui);

        let site = if self.draft.other_school && !self.draft.url.trim().is_empty() {
            self.draft.url.clone()
        } else {
            MOODLE_DEFAULT_URL.to_string()
        };
        let draft = self.draft.clone();
        if Self::action(ui, "Enregistrer") && !self.busy {
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
                    Ok(message) => Message::Done(Notice::Ok(message), true),
                    Err(message) => Message::Done(Notice::Err(message), false),
                }
            });
        }
        if configured && self.button_plain(ui, "Se déconnecter") && !self.busy {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "moodle");
                Message::Done(Notice::Ok("Moodle déconnecté.".into()), true)
            });
        }
    }

    fn proxmox_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.top_bar(ui, "Proxmox");
        let configured = self.settings.proxmox.is_some();

        Self::field(ui, "Adresse du serveur", &mut self.draft.url, "https://mon-pve:8006", false);
        Self::field(ui, "ID du token API", &mut self.draft.token_id, "prenom.nom@pam!student-mcp", false);
        Self::field(ui, "Secret du token", &mut self.draft.secret, "généré dans Datacenter → API Tokens", true);
        ui.add_space(2.0);
        Self::toggle(ui, "Autoriser aussi start/stop/reboot des machines", &mut self.draft.allow_write);
        ui.add_space(4.0);
        self.notice_line(ui);
        self.busy_text(ui);

        let draft = self.draft.clone();
        if Self::action(ui, "Enregistrer") && !self.busy {
            self.spawn(ctx.clone(), move || {
                let mut fresh = Settings::load();
                match crate::mcp::activate_proxmox(&mut fresh, &draft.url, &draft.token_id, &draft.secret, draft.allow_write) {
                    Ok(message) => Message::Done(Notice::Ok(message), true),
                    Err(message) => Message::Done(Notice::Err(message), false),
                }
            });
        }
        if configured && self.button_plain(ui, "Se déconnecter") && !self.busy {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "proxmox");
                Message::Done(Notice::Ok("Proxmox déconnecté.".into()), true)
            });
        }
    }

    fn toggle_alt(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
        let changed = ui.add(egui::Checkbox::new(value, RichText::new(label).size(11.5).color(SUBTLE))).clicked();
        ui.add_space(4.0);
        changed
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
        let Some(receiver) = self.rx.as_ref() else { return };
        let Ok(message) = receiver.try_recv() else { return };
        self.rx = None;
        self.busy = false;
        match message {
            Message::Done(notice, reload) => {
                self.notice = Some(notice);
                if reload {
                    self.settings = Settings::load();
                    self.screen = "home";
                }
            }
            Message::Funnel(info, notice) => {
                self.funnel_info = Some(info);
                match notice {
                    Notice::Ok(text) => self.notice = Some(Notice::Ok(text)),
                    Notice::Err(text) => self.notice = Some(Notice::Err(text)),
                    Notice::Info => {}
                }
            }
        }
    }
}

fn uv_full() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}
