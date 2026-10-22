//! Native screenshots of the real UI with isolated, synthetic state.
//! cargo run --example ui-preview -- /tmp/opencode/student-mcp-screens
//! No HTTP listener, saved credentials, or real public tunnels are used.
#![allow(dead_code)]

#[path = "../src/background.rs"]
mod background;
#[path = "../src/brightspace.rs"]
mod brightspace;
#[path = "../src/catalog.rs"]
mod catalog;
#[path = "../src/config.rs"]
mod config;
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
#[path = "../src/tunnel.rs"]
mod tunnel;

mod gui {
    include!("../src/gui.rs");

    const SCENES: &[&str] = &[
        "gallery",
        "gallery-connected",
        "gallery-public",
        "moodle",
        "moodle-connected",
        "moodle-offline",
        "moodle-editing",
        "moodle-filled",
        "moodle-token",
        "moodle-advanced",
        "proxmox",
        "proxmox-connected",
        "brightspace",
        "brightspace-connected",
        "links",
        "links-public",
        "error",
        "busy",
    ];

    fn fixture(ctx: &Context, scene: &str) -> App {
        let mut settings = Settings::default();
        if matches!(scene, "brightspace-connected" | "gallery-connected") {
            settings.brightspace = Some(crate::config::BrightspaceSettings {
                url: Some("https://mycourses.mcgill.ca".into()),
                token: Some("demo-brightspace-token".into()),
            });
        }
        if matches!(
            scene,
            "gallery-connected"
                | "gallery-public"
                | "links"
                | "links-public"
                | "moodle-connected"
                | "moodle-offline"
                | "moodle-editing"
        ) {
            settings.moodle = Some(crate::config::MoodleSettings {
                url: Some(MOODLE_DEFAULT_URL.into()),
                token: Some("demo-moodle-token".into()),
                allow_write: Some(false),
            });
        }
        if scene == "proxmox-connected" {
            settings.proxmox = Some(crate::config::ProxmoxSettings {
                url: Some("https://pve.example:8006".into()),
                token_id: Some("student@pam!student-mcp".into()),
                secret: Some("demo-secret".into()),
                allow_write: Some(true),
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
                "gallery" | "gallery-connected" | "gallery-public" => "home",
                "proxmox" | "proxmox-connected" => "proxmox",
                "brightspace" | "brightspace-connected" => "brightspace",
                "links" | "links-public" => "links",
                _ => "moodle",
            },
            draft: Draft::default(),
            busy: scene == "busy",
            notice: None,
            rx: None,
            check_rx: None,
            check_status: None,
            tunnel: {
                // Synthetic tunnel: no real worker or public connection.
                let mut tunnel = Tunnel::default();
                tunnel.state = TunnelState::Stopped; // Never launch a real tunnel in a preview.
                tunnel
            },
            copied_flash: None,
            background: Background::default(),
            local_enabled: Arc::new(AtomicBool::new(true)),
            scale: crate::config::DEFAULT_SCALE,
        };
        if scene == "moodle-editing" {
            app.draft.editing = true;
        }
        if scene == "moodle-connected" {
            app.check_status = Some(Notice::Ok(
                "Polytechnique Montréal · signed in as student (Moodle 4.5)".into(),
            ));
        }
        if scene == "moodle-offline" {
            app.check_status = Some(Notice::Err(
                "Cannot reach Moodle. Check the site address and your Internet connection.".into(),
            ));
        }
        if scene == "proxmox-connected" {
            app.check_status = Some(Notice::Ok("Proxmox VE 8.3 · pve-node".into()));
        }
        if scene == "brightspace-connected" {
            app.check_status = Some(Notice::Ok("Brightspace connected (read only).".into()));
        }
        if matches!(scene, "moodle-filled" | "busy") {
            app.draft.username = "firstname.lastname".into();
            app.draft.password = "demo-password".into();
        }
        if scene == "moodle-token" {
            app.draft.use_token = true;
            app.draft.token = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4".into();
        }
        if scene == "moodle-advanced" {
            app.draft.advanced = true;
            app.draft.other_school = true;
        }
        if scene == "error" {
            app.notice = Some(Notice::Err("Moodle refused the connection. Check the credentials, or switch to a mobile token if your school uses SSO or two-factor authentication.".into()));
        }
        if scene == "links-public" || scene == "gallery-public" {
            app.tunnel.state = TunnelState::Ready("https://demo-student.trycloudflare.com".into());
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
                configure_context(&cc.egui_ctx);
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn zoom_controls_render_as_two_separate_buttons() {
            fn button_surfaces(shape: &egui::Shape, buttons: &mut Vec<Rect>) {
                match shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.width() >= 28.0
                            && rect.rect.width() <= 29.0
                            && rect.corner_radius.nw == 14
                            && rect.stroke.width > 0.0 =>
                    {
                        buttons.push(rect.rect);
                    }
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            button_surfaces(shape, buttons);
                        }
                    }
                    _ => {}
                }
            }
            let ctx = Context::default();
            configure_context(&ctx);
            let mut app = fixture(&ctx, "gallery");
            let mut output = None;
            for _ in 0..2 {
                output = Some(ctx.run(egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(400.0, 560.0))),
                    ..Default::default()
                }, |ctx| {
                    CentralPanel::default().show(ctx, |_ui| {});
                    app.scale_control(ctx);
                }));
            }
            let output = output.unwrap();
            let mut buttons = Vec::new();
            for shape in &output.shapes {
                button_surfaces(&shape.shape, &mut buttons);
            }
            buttons.sort_by(|left, right| left.left().total_cmp(&right.left()));
            assert_eq!(buttons.len(), 2, "Expected distinct minus and plus backgrounds");
            assert!(buttons[1].left() - buttons[0].right() >= 9.0);
            let hovered = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(400.0, 560.0))),
                events: vec![egui::Event::PointerMoved(buttons[0].center())],
                ..Default::default()
            }, |ctx| {
                CentralPanel::default().show(ctx, |_ui| {});
                app.scale_control(ctx);
            });
            let mut shadows: Vec<_> = hovered.shapes.iter().filter_map(|shape| {
                match &shape.shape {
                    egui::Shape::Rect(rect) if rect.blur_width > 0.0 =>
                        Some((rect.rect.center().x, rect.fill.a())),
                    _ => None,
                }
            }).collect();
            shadows.sort_by(|left, right| left.0.total_cmp(&right.0));
            assert_eq!(shadows.len(), 2);
            assert!(shadows[0].1 > shadows[1].1, "Hover should strengthen only its own shadow");
        }

        #[test]
        fn gallery_link_fits_a_full_length_public_token_at_compact_sizes() {
            for size in [[400.0, 560.0], [440.0, 820.0], [800.0, 600.0]] {
                let ctx = Context::default();
                configure_context(&ctx);
                let mut app = fixture(&ctx, "gallery-public");
                app.gateway.token = "a".repeat(64);
                app.tunnel.state = TunnelState::Ready(
                    "https://industrial-partition-findings-tables.trycloudflare.com".into(),
                );
                for _ in 0..2 {
                    let mut footer = Rect::NOTHING;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size.into())),
                            ..Default::default()
                        },
                        |ctx| {
                            CentralPanel::default()
                                .frame(egui::Frame::new().fill(CANVAS).inner_margin(24.0))
                                .show(ctx, |ui| {
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        footer = app.gallery_layout(ui).footer;
                                        app.home_screen(ui);
                                    });
                                });
                        },
                    );
                    let text: Vec<_> = output.shapes.iter().filter_map(|shape| {
                        match &shape.shape {
                            egui::Shape::Text(text) if footer.contains(text.pos) => {
                                Some(Rect::from_min_size(text.pos, text.galley.size()))
                            }
                            _ => None,
                        }
                    }).collect();
                    assert_eq!(text.len(), 2, "{size:?}");
                    for bounds in text {
                        assert!(footer.shrink(8.0).contains_rect(bounds),
                            "Link overflows at {size:?}: {bounds:?} outside {footer:?}");
                    }
                }
            }
        }

        #[test]
        fn local_link_card_stays_compact_in_tall_windows() {
            fn find_card(shape: &egui::Shape) -> Option<Rect> {
                match shape {
                    egui::Shape::Rect(shape)
                        if shape.fill == Color32::WHITE && shape.rect.width() > 200.0 =>
                    {
                        Some(shape.rect)
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(find_card),
                    _ => None,
                }
            }
            for size in [[400.0, 1500.0], [440.0, 820.0], [800.0, 600.0]] {
                for enabled in [true, false] {
                    let ctx = Context::default();
                    configure_context(&ctx);
                    let mut app = fixture(&ctx, "links-public");
                    app.local_enabled.store(enabled, Ordering::SeqCst);
                    for pass in 0..2 {
                        let output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size.into())),
                                ..Default::default()
                            },
                            |ctx| {
                                CentralPanel::default()
                                    .frame(egui::Frame::new().fill(CANVAS).inner_margin(24.0))
                                    .show(ctx, |ui| {
                                        egui::ScrollArea::vertical()
                                            .show(ui, |ui| app.links_screen(ui, ctx));
                                    });
                            },
                        );
                        if pass == 0 { continue; }
                        let local_card = output.shapes.iter()
                            .find_map(|shape| find_card(&shape.shape))
                            .expect("Local link card should be visible");
                        assert!(local_card.height() < 160.0,
                            "Local card stretched at {size:?}: {local_card:?}");
                    }
                }
            }
        }

        #[test]
        fn public_switch_stops_active_or_starting_links_and_blocks_unavailable_start() {
            for scenario in ["ready", "starting", "no-service", "offline"] {
                let ctx = Context::default();
                configure_context(&ctx);
                let mut app = fixture(&ctx, "links-public");
                match scenario {
                    "starting" => app.tunnel.state = TunnelState::Starting,
                    "no-service" => {
                        app.settings = Settings::default();
                        app.tunnel.state = TunnelState::Stopped;
                    }
                    "offline" => {
                        app.bind_error = Some("port busy".into());
                        app.tunnel.state = TunnelState::Stopped;
                    }
                    _ => {}
                }
                let mut switch_center = None;
                for events in [vec![], vec![]] {
                    let output = ctx.run(egui::RawInput { events, ..Default::default() }, |ctx| {
                        CentralPanel::default().show(ctx, |ui| app.public_section(ui, ctx));
                    });
                    switch_center = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape) if shape.rect.size() == Vec2::new(36.0, 20.0) => {
                            Some(shape.rect.center())
                        }
                        _ => None,
                    });
                }
                let pos = switch_center.expect("Public link switch must always be visible");
                for pressed in [true, false] {
                    let _ = ctx.run(egui::RawInput {
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::default(),
                            },
                        ],
                        ..Default::default()
                    }, |ctx| {
                        CentralPanel::default().show(ctx, |ui| app.public_section(ui, ctx));
                    });
                }
                assert_eq!(app.tunnel.state, TunnelState::Stopped, "{scenario}");
                assert!(app.tunnel.origin().is_none());
                app.update_tunnel(&ctx);
                assert_eq!(app.tunnel.state, TunnelState::Stopped, "must not restart: {scenario}");
            }
        }

        #[test]
        fn connection_card_centers_text_for_status_errors_and_wrapped_urls() {
            for width in [260.0, 288.0, 342.0] {
                for offline in [false, true] {
                    for state in [
                        TunnelState::Idle,
                        TunnelState::Starting,
                        TunnelState::Stopped,
                        TunnelState::Failed("offline".into()),
                        TunnelState::Ready("https://demo-student.trycloudflare.com".into()),
                    ] {
                        let ctx = Context::default();
                        configure_context(&ctx);
                        let mut app = fixture(&ctx, "gallery");
                        app.tunnel.state = state;
                        app.bind_error = offline.then(|| "port busy".into());
                        let card = Rect::from_min_size(pos2(24.0, 24.0), Vec2::new(width, 76.0));
                        let mut output = None;
                        for _ in 0..2 {
                            output = Some(ctx.run(egui::RawInput::default(), |ctx| {
                                CentralPanel::default().show(ctx, |ui| app.home_link(ui, card));
                            }));
                        }
                        let text: Vec<_> = output
                            .unwrap()
                            .shapes
                            .into_iter()
                            .filter_map(|shape| match shape.shape {
                                egui::Shape::Text(text) => {
                                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(text.len(), 2);
                        let group = text[0].union(text[1]);
                        assert!(
                            (group.center().y - card.center().y).abs() < 0.5,
                            "{:?}, offline={offline}, width={width}: {group:?}",
                            app.tunnel.state
                        );
                        assert!(
                            card.shrink(4.0).contains_rect(group),
                            "{:?}, offline={offline}, width={width}: {group:?} outside {card:?}",
                            app.tunnel.state
                        );
                        assert_eq!(text[0].left(), text[1].left());
                    }
                }
            }
        }

        #[test]
        fn all_scenes_render_at_compact_default_and_wide_sizes() {
            for size in [[400.0, 560.0], [440.0, 820.0], [800.0, 600.0]] {
                for scene in SCENES {
                    let ctx = Context::default();
                    configure_context(&ctx);
                    let mut app = fixture(&ctx, scene);
                    let initial_screen = app.screen;
                    // Two passes allow egui to settle font/layout measurements.
                    for _ in 0..2 {
                        let output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size.into())),
                                ..Default::default()
                            },
                            |ctx| {
                                CentralPanel::default()
                                    .frame(egui::Frame::new().fill(CANVAS).inner_margin(24.0))
                                    .show(ctx, |ui| {
                                        egui::ScrollArea::vertical()
                                            .show(ui, |ui| app.body(ui, ctx));
                                    });
                            },
                        );
                        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                        assert!(!primitives.is_empty(), "{scene} at {size:?}");
                        assert_eq!(app.screen, initial_screen);
                        assert!(app.rx.is_none(), "Rendering must not start a connection");
                    }
                }
            }
        }
    }
}

fn main() {
    let directory = std::env::args_os()
        .nth(1)
        .expect("Pass an output directory for screenshots");
    gui::screenshots(directory.into());
}
