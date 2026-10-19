use crate::config::{mcp_endpoint, Gateway, Settings};
use crate::funnel::{self, TailscaleInfo};
use crate::logos::Logos;
use eframe::egui::{
    self, pos2, Align2, Button, CentralPanel, Color32, Context, CursorIcon, FontId, Layout, Rect,
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

fn form_bounds(bounds: Rect, expanded: bool) -> Rect {
    let size = Vec2::new(
        bounds.width().min(336.0),
        if expanded {
            bounds.height()
        } else {
            bounds.height().min(440.0)
        },
    );
    Rect::from_center_size(bounds.center(), size)
}

fn configure_context(ctx: &Context) {
    let mut visuals = egui::Visuals::light();
    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.active.bg_fill = HOVER;
    visuals.selection.bg_fill = Color32::from_rgb(225, 234, 244);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.0));
    });
}

/// Fixed, balanced proportions; position is derived from the panel, not its cursor.
struct GalleryLayout {
    title: Rect,
    tiles: [Rect; 4],
    footer: Rect,
}

impl GalleryLayout {
    fn new(bounds: Rect) -> Self {
        let gap = 16.0;
        let side = ((bounds.width() - gap) / 2.0).min(136.0);
        let width = side * 2.0 + gap;
        let height = width + 88.0;
        let origin = pos2(
            bounds.center().x - width / 2.0,
            bounds.top() + ((bounds.height() - height) / 2.0).max(0.0),
        );
        let grid_origin = origin + Vec2::new(0.0, 48.0);
        Self {
            title: Rect::from_min_size(origin, Vec2::new(width, 24.0)),
            tiles: std::array::from_fn(|index| {
                let offset = Vec2::new((index % 2) as f32, (index / 2) as f32) * (side + gap);
                Rect::from_min_size(grid_origin + offset, Vec2::splat(side))
            }),
            footer: Rect::from_min_size(
                grid_origin + Vec2::new(0.0, width + 12.0),
                Vec2::new(width, 28.0),
            ),
        }
    }
}

#[derive(Debug, Clone)]
enum Notice {
    Ok(String),
    Err(String),
}

enum Message {
    Done(Notice, NextScreen),
    Funnel(TailscaleInfo, Notice),
    FunnelStatus(TailscaleInfo),
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
    password_visible: bool,
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
    copied_flash: Option<(String, Instant)>,
    /// Interface zoom factor, persisted in the config file.
    scale: f32,
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("student-mcp")
            .with_app_id("student-mcp")
            .with_inner_size([430.0, 540.0])
            .with_min_inner_size([390.0, 490.0]),
        // Mesa's Wayland EGL vsync can block eglSwapBuffers when a window is
        // occluded or moved to another workspace. This stalls the UI thread,
        // including compositor ping/pong handling. Keep Linux swaps nonblocking;
        // eframe still redraws only on demand (no continuous repaint loop).
        vsync: !cfg!(target_os = "linux"),
        ..Default::default()
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
                busy: false,
                notice: None,
                rx: None,
                funnel_info: None,
                copied_flash: None,
                scale: crate::config::DEFAULT_SCALE,
            };
            app.scale = app.settings.scale();
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
        self.apply_scale(ctx);
        self.poll_jobs();
        CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(24.0))
            .show(ctx, |ui| {
                if self.screen == "home" {
                    self.home_screen(ui);
                } else {
                    let wide = self.draft.advanced
                        || self.notice.is_some()
                        || self.screen == "proxmox"
                        || self.screen == "links";
                    let bounds = form_bounds(ui.available_rect_before_wrap(), wide);
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

    /// Small bordered-less `− / +` control pinned to the bottom-right corner.
    fn scale_control(&mut self, ctx: &Context) {
        let step = crate::config::SCALE_STEP;
        egui::Area::new(egui::Id::new("scale-control"))
            .anchor(Align2::RIGHT_BOTTOM, [-10.0, -10.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let mut changed = None;
                ui.horizontal(|row| {
                    row.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
                    if Self::zoom_button(row, "−", self.scale <= crate::config::SCALE_MIN) {
                        changed = Some(-step);
                    }
                    if Self::zoom_button(row, "+", self.scale >= crate::config::SCALE_MAX) {
                        changed = Some(step);
                    }
                });
                if let Some(delta) = changed {
                    self.change_scale(delta);
                }
            });
    }

    fn zoom_button(ui: &mut Ui, label: &str, disabled: bool) -> bool {
        ui.add_enabled(
            !disabled,
            Button::new(RichText::new(label).size(15.0).color(SUBTLE)).frame(false),
        )
        .on_hover_text(if disabled {
            "Taille limite"
        } else if label == "+" {
            "Agrandir"
        } else {
            "Réduire"
        })
        .clicked()
    }
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
        let layout = GalleryLayout::new(ui.available_rect_before_wrap());
        ui.painter().text(
            layout.title.center(),
            Align2::CENTER_CENTER,
            "Tes services",
            FontId::proportional(18.0),
            INK,
        );
        let Some(logos) = self.logos.as_ref() else {
            return;
        };
        let moodle_on = self.settings.moodle().is_some();
        let proxmox_on = self.settings.proxmox().is_some();

        let moodle_clicked = Self::tile(
            ui,
            &logos.moodle,
            "Moodle",
            layout.tiles[0],
            moodle_on,
            false,
        );
        let proxmox_clicked = Self::tile(
            ui,
            &logos.proxmox,
            "Proxmox",
            layout.tiles[1],
            proxmox_on,
            false,
        );
        Self::tile(ui, &logos.poly, "Dossier", layout.tiles[2], false, true);
        Self::tile(ui, &logos.outlook, "Courriel", layout.tiles[3], false, true);

        if moodle_clicked || proxmox_clicked {
            self.open_service(if moodle_clicked { "moodle" } else { "proxmox" });
            self.notice = None;
        }

        if let Some(error) = &self.bind_error {
            ui.put(
                layout.footer,
                egui::Label::new(RichText::new("Serveur indisponible").color(RED)),
            )
            .on_hover_text(error);
        } else if moodle_on || proxmox_on {
            if ui
                .put(
                    layout.footer,
                    Button::new(RichText::new("Mon lien pour Claude").size(13.0).color(INK))
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
        rect: Rect,
        active: bool,
        soon: bool,
    ) -> bool {
        let clickable = !soon;
        let raw = ui.interact(
            rect,
            ui.make_persistent_id(("service", name)),
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
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, clickable, name));

        ui.painter()
            .rect_filled(rect, 14.0, if hovered { HOVER } else { Color32::WHITE });
        ui.painter().rect_stroke(
            rect,
            14.0,
            Stroke::new(
                1.0_f32,
                if response.has_focus() {
                    INK
                } else if hovered {
                    Color32::from_rgb(200, 200, 200)
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

        // Fit the complete app symbol without cropping or stretching it.
        let logo_bounds = Vec2::splat(rect.width() * 0.42);
        let texture_size = texture.size_vec2();
        let scale = (logo_bounds.x / texture_size.x).min(logo_bounds.y / texture_size.y);
        let logo_rect =
            Rect::from_center_size(rect.center() - Vec2::new(0.0, 12.0), texture_size * scale);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let tint = if soon {
            Color32::from_white_alpha(125)
        } else {
            Color32::WHITE
        };
        ui.painter().image(texture.id(), logo_rect, uv, tint);

        let name_color = if soon { SUBTLE } else { INK };
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 25.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(13.0),
            name_color,
        );

        if soon {
            response.on_hover_text("Bientôt disponible");
            false
        } else {
            response
                .on_hover_text(if active {
                    "Connecté — modifier la connexion"
                } else {
                    "Connecter ce service"
                })
                .clicked()
        }
    }

    fn open_service(&mut self, service: &'static str) {
        self.draft = Draft::default();
        self.screen = service;
        match service {
            "moodle" => {
                if let Some(config) = self.settings.moodle() {
                    self.draft.allow_write = config.allow_write;
                    self.draft.other_school = config.base_url != MOODLE_DEFAULT_URL;
                    self.draft.url = config.base_url;
                }
            }
            "proxmox" => {
                if let Some(config) = self.settings.proxmox() {
                    self.draft.url = config.base_url;
                    self.draft.token_id = config.token_id;
                    self.draft.allow_write = config.allow_write;
                }
            }
            _ => {}
        }
    }

    // ---------- links below the gallery ----------

    fn links_screen(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.top_bar(ui, "Ton lien pour Claude") {
            return;
        }
        let count = self.settings.services_active().len();
        ui.label(
            RichText::new(format!(
                "{} service{} connecté{} · {} outils",
                count,
                if count > 1 { "s" } else { "" },
                if count > 1 { "s" } else { "" },
                crate::mcp::tool_definitions(&self.settings).len()
            ))
            .size(12.0)
            .color(GREEN),
        );
        ui.add_space(16.0);
        self.funnel_links(ui, ctx);
        self.notice_line(ui);
        ui.add_space(16.0);
        egui::CollapsingHeader::new(RichText::new("Utilisation locale").size(12.0).color(SUBTLE))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Pour les clients MCP sur cet ordinateur.")
                        .size(12.0)
                        .color(SUBTLE),
                );
                let local = mcp_endpoint(
                    &format!("http://127.0.0.1:{}", self.port),
                    &self.gateway.token,
                );
                self.copy_row(ui, ctx, &local);
            });
        ui.add_space(20.0);
        ui.label(
            RichText::new("Garde cette fenêtre ouverte pendant l'utilisation.")
                .size(11.5)
                .color(SUBTLE),
        );
    }

    fn funnel_links(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        match self.funnel_info.clone() {
            None => {
                ui.label(
                    RichText::new("Vérification de Tailscale…")
                        .size(12.0)
                        .color(SUBTLE),
                );
                if !self.busy {
                    self.spawn(ctx.clone(), || Message::FunnelStatus(funnel::info()));
                }
            }
            Some(info) if info.funnel_on => {
                let url = info
                    .host
                    .as_deref()
                    .map(|host| funnel::public_connector_url(host, &self.gateway.token))
                    .unwrap_or_default();
                ui.label(
                    RichText::new("Lien public actif")
                        .size(13.0)
                        .strong()
                        .color(INK),
                );
                ui.add_space(6.0);
                self.copy_row(ui, ctx, &url);
                ui.add_space(12.0);
                for step in [
                    "1. Claude > Paramètres > Connecteurs",
                    "2. Ajouter un connecteur personnalisé",
                    "3. Colle le lien dans l'URL du serveur.",
                ] {
                    ui.add(egui::Label::new(RichText::new(step).size(12.0).color(SUBTLE)).wrap());
                    ui.add_space(4.0);
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Ce lien est privé : ne le partage pas.")
                        .size(11.5)
                        .color(SUBTLE),
                );
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
            Some(info) if !info.present || !info.logged_in => {
                ui.label(
                    RichText::new(if !info.present {
                        "Installe Tailscale pour obtenir ton lien public."
                    } else {
                        "Connecte-toi dans Tailscale, puis réessaie."
                    })
                    .size(13.0)
                    .color(INK),
                );
                ui.add_space(8.0);
                if !info.present {
                    ui.hyperlink_to("Télécharger Tailscale", "https://tailscale.com/download");
                    ui.add_space(8.0);
                }
                if Self::action(ui, "Vérifier à nouveau", !self.busy) {
                    self.spawn(ctx.clone(), || Message::FunnelStatus(funnel::info()));
                }
            }
            Some(_) => {
                ui.label(
                    RichText::new("Crée un lien sécurisé pour connecter Claude à tes services.")
                        .size(13.0)
                        .color(SUBTLE),
                );
                ui.add_space(14.0);
                if Self::action(
                    ui,
                    if self.busy {
                        "Création du lien…"
                    } else {
                        "Créer mon lien public"
                    },
                    !self.busy && self.bind_error.is_none(),
                ) {
                    let port = self.port;
                    self.spawn(ctx.clone(), move || match funnel::funnel_enable(port) {
                        Ok(note) => Message::Funnel(funnel::info(), Notice::Ok(note)),
                        Err(message) => Message::Funnel(funnel::info(), Notice::Err(message)),
                    });
                }
            }
        }
        if self.busy {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Cela peut prendre quelques secondes.")
                    .size(11.5)
                    .color(SUBTLE),
            );
        }
    }

    fn copy_row(&mut self, ui: &mut Ui, ctx: &egui::Context, text: &str) {
        egui::Frame::new()
            .fill(FIELD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                // Wrap the entire URL; never crop a secret or depend on byte offsets.
                ui.add(
                    egui::Label::new(RichText::new(text).monospace().size(12.0).color(INK))
                        .wrap()
                        .selectable(true),
                );
                ui.add_space(10.0);
                let copied = self
                    .copied_flash
                    .as_ref()
                    .is_some_and(|(value, at)| value == text && at.elapsed().as_secs_f32() < 1.8);
                if ui
                    .add(
                        Button::new(
                            RichText::new(if copied {
                                "Lien copié"
                            } else {
                                "Copier le lien"
                            })
                            .size(12.0)
                            .color(INK),
                        )
                        .corner_radius(6.0),
                    )
                    .clicked()
                {
                    ctx.copy_text(text.to_string());
                    self.copied_flash = Some((text.to_string(), Instant::now()));
                    ctx.request_repaint_after(std::time::Duration::from_secs(2));
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
            egui::Frame::new()
                .fill(if color == RED {
                    Color32::from_rgb(255, 246, 245)
                } else {
                    Color32::from_rgb(245, 251, 246)
                })
                .corner_radius(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).wrap());
                });
            ui.add_space(8.0);
        }
    }

    // ---------- service screens (white, minimal) ----------

    fn top_bar(&mut self, ui: &mut Ui, title: &str) -> bool {
        let back = ui
            .add_enabled(
                !self.busy,
                Button::new(
                    RichText::new("Retour aux services")
                        .size(12.0)
                        .color(SUBTLE),
                )
                .frame(false),
            )
            .clicked();
        if back {
            self.screen = "home";
            self.notice = None;
            self.draft = Draft::default();
            return true;
        }
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            if let Some(logos) = &self.logos {
                let texture = match title {
                    "Moodle" => Some(&logos.moodle),
                    "Proxmox" => Some(&logos.proxmox),
                    _ => None,
                };
                if let Some(texture) = texture {
                    ui.image((texture.id(), Vec2::splat(36.0)));
                    ui.add_space(8.0);
                }
            }
            ui.label(RichText::new(title).size(21.0).strong().color(INK));
            if title == "Moodle" {
                ui.label(
                    RichText::new(if self.draft.other_school {
                        "Ton compte Moodle"
                    } else {
                        "Polytechnique Montréal"
                    })
                    .size(12.0)
                    .color(SUBTLE),
                );
            }
        });
        ui.add_space(20.0);
        false
    }

    fn field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, secret: bool) {
        ui.label(RichText::new(label).size(12.0).color(SUBTLE));
        Self::input_field(ui, label, buffer, hint, secret);
    }

    fn secret_field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, visible: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(SUBTLE));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        Button::new(
                            RichText::new(if *visible { "Masquer" } else { "Afficher" })
                                .size(11.0)
                                .color(SUBTLE),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    *visible = !*visible;
                }
            });
        });
        Self::input_field(ui, label, buffer, hint, !*visible);
    }

    fn input_field(ui: &mut Ui, label: &str, buffer: &mut String, hint: &str, secret: bool) {
        let frame = egui::Frame::new()
            .fill(FIELD)
            .corner_radius(8.0)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(10.0);
        frame.show(ui, |inner| {
            inner.add(
                TextEdit::singleline(buffer)
                    .id_salt(label)
                    .hint_text(hint)
                    .font(egui::TextStyle::Body)
                    .desired_width(inner.available_width())
                    .frame(false)
                    .password(secret),
            );
        });
        ui.add_space(12.0);
    }

    fn toggle(ui: &mut Ui, label: &str, value: &mut bool) {
        ui.add(egui::Checkbox::new(
            value,
            RichText::new(label).size(11.5).color(SUBTLE),
        ));
        ui.add_space(4.0);
    }

    fn action(ui: &mut Ui, label: &str, enabled: bool) -> bool {
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label)
                });
                let fill = if !enabled {
                    Color32::from_rgb(239, 239, 239)
                } else if response.is_pointer_button_down_on() {
                    Color32::from_rgb(65, 65, 65)
                } else if response.hovered() {
                    Color32::from_rgb(45, 45, 45)
                } else {
                    INK
                };
                ui.painter().rect_filled(rect, 8.0, fill);
                if response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        10.0,
                        Stroke::new(1.0_f32, INK),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(13.0),
                    if enabled { Color32::WHITE } else { SUBTLE },
                );
                response
            })
            .inner;
        ui.add_space(4.0);
        response.clicked()
    }

    fn busy_text(&self, ui: &mut Ui) {
        if self.busy {
            ui.label(
                RichText::new("Vérification de la connexion…")
                    .size(12.0)
                    .color(SUBTLE),
            );
            ui.add_space(8.0);
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
            Self::secret_field(
                ui,
                "Mot de passe",
                &mut self.draft.password,
                "Mot de passe Moodle",
                &mut self.draft.password_visible,
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
        let submit = Self::action(
            ui,
            if self.busy {
                "Connexion…"
            } else if configured {
                "Mettre à jour"
            } else {
                "Connecter Moodle"
            },
            ready,
        ) || ready && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if submit {
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
        ui.label(
            RichText::new("Ton mot de passe n'est jamais enregistré.")
                .size(11.5)
                .color(SUBTLE),
        );
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
        Self::secret_field(
            ui,
            "Secret du token",
            &mut self.draft.secret,
            "Secret du token API",
            &mut self.draft.password_visible,
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
            if self.busy {
                "Connexion…"
            } else if configured {
                "Mettre à jour"
            } else {
                "Connecter Proxmox"
            },
            ready,
        ) || ready && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
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
        ui.add_space(8.0);
        ui.label(
            RichText::new("Crée le token dans Proxmox : Datacenter > Permissions > API Tokens.")
                .size(11.5)
                .color(SUBTLE),
        );
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
        let response = ui.add_enabled(
            !self.busy,
            Button::new(RichText::new(label).size(12.0).color(SUBTLE)).frame(false),
        );
        response.clicked()
    }

    // ---------- plumbing ----------

    fn spawn(&mut self, ctx: Context, job: impl FnOnce() -> Message + Send + 'static) {
        if self.busy {
            return;
        }
        let (sender, receiver): (Sender<Message>, Receiver<Message>) = channel();
        self.rx = Some(receiver);
        self.busy = true;
        self.notice = None;
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
        let message = match receiver.try_recv() {
            Ok(message) => message,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                self.busy = false;
                self.notice = Some(Notice::Err(
                    "La connexion a été interrompue. Tu peux réessayer.".into(),
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
                    NextScreen::Links => {
                        self.draft = Draft::default();
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
            Message::FunnelStatus(info) => {
                self.funnel_info = Some(info);
            }
        }
    }
}

#[cfg(test)]
mod gallery_tests {
    use super::*;

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
            let layout = GalleryLayout::new(bounds);
            let grid = layout.tiles[0].union(layout.tiles[3]);
            let content = layout.title.union(layout.footer);
            assert_eq!(grid.width(), grid.height());
            assert_eq!(grid.center().x, bounds.center().x);
            assert_eq!(content.center(), bounds.center());
            assert!(bounds.contains_rect(content));
            for tile in layout.tiles {
                assert_eq!(tile.size(), Vec2::splat(136.0));
            }
            assert_eq!(layout.tiles[1].left() - layout.tiles[0].right(), 16.0);
            assert_eq!(layout.tiles[2].top() - layout.tiles[0].bottom(), 16.0);
        }
    }

    #[test]
    fn gallery_renders_all_icons_without_a_display() {
        let ctx = Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let logos = Logos::load(ctx);
            CentralPanel::default().show(ctx, |ui| {
                let layout = GalleryLayout::new(Rect::from_min_size(
                    pos2(24.0, 24.0),
                    Vec2::new(382.0, 492.0),
                ));
                for (index, (texture, name)) in [
                    (&logos.moodle, "Moodle"),
                    (&logos.proxmox, "Proxmox"),
                    (&logos.poly, "Dossier"),
                    (&logos.outlook, "Courriel"),
                ]
                .into_iter()
                .enumerate()
                {
                    assert!(!App::tile(
                        ui,
                        texture,
                        name,
                        layout.tiles[index],
                        false,
                        index >= 2
                    ));
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
        assert_eq!(images.len(), 4);
        for image in images {
            assert!(image.vertices.iter().any(|v| v.uv == pos2(0.0, 0.0)));
            assert!(image.vertices.iter().any(|v| v.uv == pos2(1.0, 1.0)));
        }
    }
}
