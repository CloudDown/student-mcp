//! Native screenshots of the real UI with isolated, synthetic state.
//! cargo run --example ui-preview -- /tmp/opencode/student-mcp-screens
//! No HTTP listener, saved credentials, or Tailscale commands are used.
#![allow(dead_code)]

#[path = "../src/catalog.rs"]
mod catalog;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/funnel.rs"]
mod funnel;
#[path = "../src/http.rs"]
mod http;
#[path = "../src/logos.rs"]
mod logos;
#[path = "../src/mcp.rs"]
mod mcp;
#[path = "../src/moodle.rs"]
mod moodle;
#[path = "../src/proxmox.rs"]
mod proxmox;

mod gui {
    include!("../src/gui.rs");

    const SCENES: &[&str] = &[
        "gallery",
        "gallery-connected",
        "moodle",
        "moodle-filled",
        "moodle-advanced",
        "proxmox",
        "links",
        "links-public",
        "error",
        "busy",
    ];

    fn fixture(ctx: &Context, scene: &str) -> App {
        let mut settings = Settings::default();
        if matches!(scene, "gallery-connected" | "links" | "links-public") {
            settings.moodle = Some(crate::config::MoodleSettings {
                url: Some(MOODLE_DEFAULT_URL.into()),
                token: Some("demo-moodle-token".into()),
                allow_write: Some(false),
            });
        }
        let mut app = App {
            settings,
            gateway: Gateway {
                port: 8420,
                token: "demo-connector-key-not-a-real-secret".into(),
            },
            port: 8420,
            logos: Some(Logos::load(ctx)),
            bind_error: None,
            screen: match scene {
                "gallery" | "gallery-connected" => "home",
                "proxmox" => "proxmox",
                "links" | "links-public" => "links",
                _ => "moodle",
            },
            draft: Draft::default(),
            busy: scene == "busy",
            notice: None,
            rx: None,
            funnel_info: Some(TailscaleInfo {
                present: true,
                logged_in: true,
                host: Some("mon-ordinateur.exemple.ts.net".into()),
                funnel_on: false,
                message: None,
            }),
            copied_flash: None,
            scale: crate::config::DEFAULT_SCALE,
        };
        if matches!(scene, "moodle-filled" | "busy") {
            app.draft.username = "prenom.nom".into();
            app.draft.password = "demo-password".into();
        }
        if scene == "moodle-advanced" {
            app.draft.advanced = true;
            app.draft.other_school = true;
        }
        if scene == "error" {
            app.notice = Some(Notice::Err("Moodle refuse la connexion. Vérifie ton identifiant et ton mot de passe. Si ton école utilise la double authentification, utilise un token mobile dans les options avancées.".into()));
        }
        if scene == "links-public" {
            app.funnel_info = Some(TailscaleInfo {
                present: true,
                logged_in: true,
                host: Some("mon-ordinateur.exemple.ts.net".into()),
                funnel_on: true,
                message: None,
            });
        }
        app
    }

    struct Preview {
        app: App,
        directory: std::path::PathBuf,
        index: usize,
        frames: usize,
        ready_at: Instant,
    }

    impl eframe::App for Preview {
        fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
            let shot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = shot {
                let path = self.directory.join(format!("{}.png", SCENES[self.index]));
                let bytes: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                image::save_buffer(
                    &path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                println!(
                    "Captured {} ({}×{})",
                    path.display(),
                    image.width(),
                    image.height()
                );
                self.index += 1;
                if self.index == SCENES.len() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.app = fixture(ctx, SCENES[self.index]);
                self.frames = 0;
            }
            self.app.update(ctx, frame);
            if Instant::now() < self.ready_at {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            if self.frames == 2 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            } else if self.frames < 2 {
                ctx.request_repaint();
            }
            self.frames += 1;
        }
    }

    pub fn screenshots(directory: std::path::PathBuf) {
        std::fs::create_dir_all(&directory).unwrap();
        let mut options = native_options();
        options.viewport = options
            .viewport
            .with_title("student-mcp · UI preview")
            .with_app_id("student-mcp-preview");
        eframe::run_native(
            "student-mcp-preview",
            options,
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(egui::Visuals::light());
                Ok(Box::new(Preview {
                    app: fixture(&cc.egui_ctx, SCENES[0]),
                    directory,
                    index: 0,
                    frames: 0,
                    ready_at: Instant::now() + std::time::Duration::from_secs(3),
                }))
            }),
        )
        .unwrap();
    }
}

fn main() {
    let directory = std::env::args_os()
        .nth(1)
        .expect("Pass an output directory for screenshots");
    gui::screenshots(directory.into());
}
