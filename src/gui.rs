use crate::config::{mcp_endpoint, Gateway, Settings, VERSION};
use crate::funnel::{self, TailscaleInfo};
use eframe::egui::{self, Button, Color32, Label, RichText, Sense, TextEdit, Ui};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

const ACCENT: [Color32; 4] = [
    Color32::from_rgb(0xf2, 0x99, 0x4a), // Moodle orange
    Color32::from_rgb(0xb1, 0x6a, 0xe3), // Proxmox violet
    Color32::from_rgb(0x54, 0x6e, 0x7a), // placeholder Dossier
    Color32::from_rgb(0x51, 0x7e, 0x5e), // placeholder Courriel
];

fn dim() -> Color32 {
    Color32::from_rgb(150, 150, 150)
}

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
    ack: bool,
}

pub struct App {
    settings: Settings,
    gateway: Gateway,
    port: u16,
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
    // Bind now (before the window opens) and try a few sibling ports if taken.
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
            .with_inner_size([430.0, 640.0])
            .with_min_inner_size([380.0, 520.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "student-mcp",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(App {
                settings: Settings::load(),
                gateway,
                port,
                bind_error,
                screen: "home",
                draft: Draft::default(),
                busy: false,
                notice: None,
                rx: None,
                funnel_info: None,
                copied_flash: None,
            }))
        }),
    );
    if let Err(error) = result {
        eprintln!("Fenêtre impossible : {error:?} — le serveur MCP stdio reste disponible (student-mcp sans commande).");
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        egui::CentralPanel::default().show(ctx, |ui| self.body(ui, ctx));
    }
}

impl App {
    fn body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|row| {
                row.add(Label::new(RichText::new("student-mcp").size(21.0).strong()).selectable(false));
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
                    row.label(RichText::new(format!("v{VERSION}")).small().color(dim()));
                });
            });
            match self.screen {
                "home" => self.home_screen(ui, ctx),
                "moodle" => self.moodle_screen(ui, ctx),
                "proxmox" => self.proxmox_screen(ui, ctx),
                _ => self.home_screen(ui, ctx),
            }
            self.footer(ui);
        });
    }

    fn home_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.add_space(10.0);
        if let Some(error) = &self.bind_error {
            ui.colored_label(
                Color32::from_rgb(0xe9, 0x4f, 0x4f),
                format!("Serveur MCP local impossible : port {} occupé. Ferme l'autre instance, puis rouvre.", self.port),
            );
            let _ = error;
        } else {
            ui.horizontal(|row| {
                dot(row, Color32::from_rgb(0x43, 0xa0, 0x5f));
                row.label(format!("Serveur MCP local : 127.0.0.1:{}", self.port));
            });
        }

        // Local connector URL + copy
        ui.add_space(6.0);
        let local = mcp_endpoint(&format!("http://127.0.0.1:{}", self.port), &self.gateway.token);
        ui.label(RichText::new("Connecteur local (Claude Desktop, OpenCode…). Monospace :").small());
        self.row_copy(ui, ctx, &local, true);

        ui.add_space(14.0);
        ui.heading(RichText::new("Services").size(15.0));
        ui.add_space(4.0);
        let moodle_on = self.settings.moodle().is_some();
        let proxmox_on = self.settings.proxmox().is_some();
        if self.card(ui, "M", "Moodle", if moodle_on { "Actif — cliquer pour voir ou modifier" } else { "Cours, devoirs, notes, horaire" }, ACCENT[0], true) {
            self.draft = Draft::default();
            self.screen = "moodle";
            self.notice = None;
        }
        if self.card(ui, "P", "Proxmox", if proxmox_on { "Actif — cliquer pour voir ou modifier" } else { "VMs et containers de ton labo" }, ACCENT[1], true) {
            self.draft = Draft::default();
            self.screen = "proxmox";
            self.notice = None;
        }
        self.card(ui, "D", "Dossier étudiant", "Bientôt — pas d'API publique", ACCENT[2], false);
        self.card(ui, "@", "Courriel Poly", "Bientôt — pas d'API publique", ACCENT[3], false);

        ui.add_space(16.0);
        ui.heading(RichText::new("Lien public (optionnel)").size(15.0));
        ui.add_space(2.0);
        ui.label(
            RichText::new("Publie une URL HTTPS que tu peux coller dans Claude sur le web comme connecteur personnalisé.")
                .small()
                .color(dim()),
        );
        self.funnel_section(ui, ctx);
    }

    fn card(&mut self, ui: &mut Ui, initial: &str, title: &str, tagline: &str, accent: Color32, enabled: bool) -> bool {
        let (fill, text) = if enabled { (accent, Color32::WHITE) } else { (Color32::from_rgb(38, 38, 38), dim()) };
        let button = Button::new(
            RichText::new(format!("{initial}   {title}\n        {tagline}"))
                .strong()
                .color(text),
        )
        .fill(fill);
        let response = ui
            .add_sized([ui.available_width(), 52.0], button)
            .interact(Sense::click());
        let clicked = enabled && response.clicked();
        if enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        ui.add_space(6.0);
        clicked
    }

    fn funnel_section(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match &self.funnel_info {
            None => {
                if self.button_enabled(ui, "Vérifier Tailscale", true) {
                    self.spawn(ctx.clone(), || {
                        let info = funnel::info();
                        let notice = if info.present && info.logged_in {
                            Notice::Ok("Tailscale connecté.".into())
                        } else {
                            Notice::Err(info.message.clone().unwrap_or_default())
                        };
                        Message::Funnel(info, notice)
                    });
                }
            }
            Some(info) if !info.present => {
                ui.colored_label(Color32::from_rgb(0xe9, 0x4f, 0x4f), "Tailscale n'est pas installé sur cette machine.");
                if self.button_enabled(ui, "Re-vérifier", true) {
                    self.spawn(ctx.clone(), || Message::Funnel(funnel::info(), Notice::Info));
                }
            }
            Some(info) if !info.logged_in => {
                ui.colored_label(Color32::from_rgb(0xe9, 0x4f, 0x4f), info.message.clone().unwrap_or_default());
                if self.button_enabled(ui, "Re-vérifier", true) {
                    self.spawn(ctx.clone(), || Message::Funnel(funnel::info(), Notice::Info));
                }
            }
            Some(info) => {
                if info.funnel_on {
                    let url = info
                        .host
                        .as_deref()
                        .map(|host| funnel::public_connector_url(host, &self.gateway.token))
                        .unwrap_or_default();
                    ui.colored_label(Color32::from_rgb(0x43, 0xa0, 0x5f), "URL publique active :");
                    self.row_copy(ui, ctx, &url, true);
                    if self.button_enabled(ui, "Retirer l'URL publique", true) {
                        self.spawn(ctx.clone(), move || {
                            let note = funnel::funnel_disable();
                            Message::Funnel(funnel::info(), Notice::Ok(note))
                        });
                    }
                } else {
                    if self.button_enabled(ui, "Publier via Tailscale Funnel", true) {
                        let port = self.port;
                        self.spawn(ctx.clone(), move || match funnel::funnel_enable(port) {
                            Ok(note) => Message::Funnel(funnel::info(), Notice::Ok(note)),
                            Err(message) => Message::Funnel(funnel::info(), Notice::Err(message)),
                        });
                    }
                    ui.label(
                        RichText::new("Cela crée une URL du type https://ton-machine.ts.net/mcp/<token>. Seule la clé compte : garde-la pour toi.")
                            .small()
                            .color(dim()),
                    );
                }
            }
        }
    }

    fn moodle_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.back_button(ui) {
            return;
        }
        ui.heading("Moodle");
        ui.add_space(4.0);
        let shown = self.settings.moodle.as_ref().and_then(|current| current.url.clone());
        if let Some(url) = shown {
            self.row_copy(ui, ctx, &url, false);
        }
        ui.add_space(6.0);
        ui.label("Adresse du site Moodle");
        ui.add(
            TextEdit::singleline(&mut self.draft.url)
                .hint_text("moodle.polymtl.ca")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        ui.label("Méthode A — identifiant Poly (le plus simple)");
        ui.add(
            TextEdit::singleline(&mut self.draft.username)
                .hint_text("prenom.nom")
                .desired_width(f32::INFINITY),
        );
        ui.add(
            TextEdit::singleline(&mut self.draft.password)
                .password(true)
                .hint_text("mot de passe (pas gardé sur disque)")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        ui.label("Méthode B — coller un token mobile existant");
        ui.add(
            TextEdit::singleline(&mut self.draft.token)
                .password(true)
                .hint_text("optionnel : remplit A et B pour tester")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);
        ui.checkbox(
            &mut self.draft.allow_write,
            "Autoriser les modifications (dépôts, messages…)",
        );
        ui.checkbox(&mut self.draft.ack, "J'utilise mes propres identifiants selon les règles de l'école.");
        ui.add_space(8.0);
        self.notice_line(ui);
        self.busy_line(ui);
        let ready = self.draft.ack && !self.busy;
        let draft = self.draft.clone();
        if self.button_enabled(ui, "Connecter Moodle", ready) {
            self.spawn(ctx.clone(), move || {
                let mut fresh = Settings::load();
                match crate::mcp::activate_moodle(
                    &mut fresh,
                    &draft.url,
                    &draft.username,
                    &draft.password,
                    &draft.token,
                    draft.allow_write,
                ) {
                    Ok(message) => Message::Done(Notice::Ok(message), true),
                    Err(message) => Message::Done(Notice::Err(message), false),
                }
            });
        }
        if self.settings.moodle.is_some() && self.button_enabled(ui, "Désactiver Moodle", !self.busy) {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "moodle");
                Message::Done(Notice::Ok("Moodle déconnecté.".into()), true)
            });
        }
    }

    fn proxmox_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.back_button(ui) {
            return;
        }
        ui.heading("Proxmox VE");
        ui.add_space(4.0);
        let shown = self.settings.proxmox.as_ref().and_then(|current| {
            let token_id = current.token_id.clone()?;
            let url = current.url.clone()?;
            Some(format!("{url} — {token_id}"))
        });
        if let Some(line) = shown {
            self.row_copy(ui, ctx, &line, false);
        }
        ui.add_space(6.0);
        ui.label("Adresse du serveur (racine)");
        ui.add(
            TextEdit::singleline(&mut self.draft.url)
                .hint_text("https://mon-pve:8006")
                .desired_width(f32::INFINITY),
        );
        ui.label("ID du token API");
        ui.add(
            TextEdit::singleline(&mut self.draft.token_id)
                .hint_text("utilisateur@pam!etudiant-mcp")
                .desired_width(f32::INFINITY),
        );
        ui.label("Secret du token");
        ui.add(
            TextEdit::singleline(&mut self.draft.secret)
                .password(true)
                .hint_text("clé secrète générée par le datacenter Proxmox")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);
        ui.checkbox(&mut self.draft.allow_write, "Autoriser start/stop/reboot des machines");
        ui.checkbox(&mut self.draft.ack, "Ce serveur Proxmox est le mien ; j'assume les actions.");
        ui.add_space(8.0);
        self.notice_line(ui);
        self.busy_line(ui);
        let ready = self.draft.ack && !self.busy;
        let draft = self.draft.clone();
        if self.button_enabled(ui, "Connecter Proxmox", ready) {
            self.spawn(ctx.clone(), move || {
                let mut fresh = Settings::load();
                match crate::mcp::activate_proxmox(&mut fresh, &draft.url, &draft.token_id, &draft.secret, draft.allow_write) {
                    Ok(message) => Message::Done(Notice::Ok(message), true),
                    Err(message) => Message::Done(Notice::Err(message), false),
                }
            });
        }
        if self.settings.proxmox.is_some() && self.button_enabled(ui, "Désactiver Proxmox", !self.busy) {
            self.spawn(ctx.clone(), || {
                let mut fresh = Settings::load();
                crate::mcp::disable_service(&mut fresh, "proxmox");
                Message::Done(Notice::Ok("Proxmox déconnecté.".into()), true)
            });
        }
    }

    // ---------- shared pieces ----------

    fn back_button(&mut self, ui: &mut Ui) -> bool {
        let go_back = ui.add(Button::new("← Accueil")).clicked();
        ui.add_space(6.0);
        go_back
    }

    fn button_enabled(&mut self, ui: &mut Ui, label: &str, enabled: bool) -> bool {
        let response = ui.add_enabled(enabled, Button::new(label).min_size(egui::vec2(ui.available_width(), 28.0)));
        let clicked = response.clicked();
        ui.add_space(4.0);
        clicked
    }

    fn row_copy(&mut self, ui: &mut Ui, ctx: &egui::Context, text: &str, monospace: bool) {
        ui.horizontal(|row| {
            let label = if monospace {
                RichText::new(text).monospace().small()
            } else {
                RichText::new(text).small()
            };
            row.add(Label::new(label).sense(Sense::click()));
            let copy_label = if self.copied_flash.map(|at| at.elapsed().as_secs_f32() < 1.2).unwrap_or(false) {
                "Copié !"
            } else {
                "Copier"
            };
            if row.add(Button::new(RichText::new(copy_label).small())).clicked() {
                ctx.copy_text(text.to_string());
                self.copied_flash = Some(Instant::now());
                ctx.request_repaint_after(std::time::Duration::from_millis(1300));
            }
        });
    }

    fn notice_line(&mut self, ui: &mut Ui) {
        if let Some(notice) = &self.notice {
            let (text, color) = match notice {
                Notice::Ok(text) => (text, Color32::from_rgb(0x43, 0xa0, 0x5f)),
                Notice::Err(text) => (text, Color32::from_rgb(0xe9, 0x4f, 0x4f)),
                Notice::Info => return,
            };
            ui.colored_label(color, text);
            ui.add_space(4.0);
        }
    }

    fn busy_line(&mut self, ui: &mut Ui) {
        if self.busy {
            ui.label(RichText::new("Travail en cours…").color(dim()));
            ui.add_space(4.0);
        }
    }

    fn footer(&self, ui: &mut Ui) {
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!(
                "Config : {} — MCP stdio aussi dispo (student-mcp sans commande).",
                crate::config::config_path().display()
            ))
            .small()
            .color(dim()),
        );
    }

    fn spawn(&mut self, ctx: egui::Context, job: impl FnOnce() -> Message + Send + 'static) {
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

fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, color);
}
