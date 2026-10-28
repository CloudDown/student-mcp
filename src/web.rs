//! Local configuration UI. This listener is never forwarded through the MCP tunnel.
use crate::{
    config::{mcp_endpoint, random_hex, token_matches, Settings},
    omnivox::{LoginOutcome, PendingLogin},
    tunnel::Tunnel,
};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tiny_http::{Header, Method, Response, Server};

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    match result {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(_) => eprintln!("Open {url} in your browser."),
    }
}

pub fn run(port: Option<u16>, _first_run: bool) {
    if let Err(error) = serve(port) {
        eprintln!("student-mcp: {error}");
    }
}

fn serve(port: Option<u16>) -> Result<(), String> {
    let settings = Settings::load();
    if let Some(error) = settings.config_error {
        return Err(error);
    }
    let gateway = settings.gateway();
    let instance = crate::config::config_path().with_extension("ui.json");
    let listener = match crate::http::bind(port.unwrap_or(gateway.port)) {
        Ok(listener) => listener,
        Err(error) => {
            if let Ok(bytes) = std::fs::read(&instance) {
                if let Ok((admin_port, key)) = serde_json::from_slice::<(u16, String)>(&bytes) {
                    let origin = format!("http://127.0.0.1:{admin_port}");
                    if ureq::AgentBuilder::new()
                        .redirects(0)
                        .timeout(Duration::from_secs(2))
                        .build()
                        .get(&format!("{origin}/state"))
                        .set("Authorization", &format!("Bearer {key}"))
                        .call()
                        .is_ok()
                    {
                        open_browser(&format!("{origin}/#{key}"));
                        return Ok(());
                    }
                }
            }
            return Err(format!("Cannot start MCP: {error}"));
        }
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let enabled = Arc::new(AtomicBool::new(settings.gateway.local_enabled()));
    let control = Arc::clone(&enabled);
    std::thread::spawn(move || {
        let _ = crate::http::serve_controlled(listener, control);
    });
    // The public relay needs a private loopback target independent of whether
    // the user exposes the local MCP endpoint. Never tunnel into the local
    // listener, whose availability is controlled by the Local Link switch.
    let tunnel_listener = crate::http::bind(0).map_err(|e| e.to_string())?;
    let tunnel_port = tunnel_listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    std::thread::spawn(move || {
        let _ = crate::http::serve_controlled(tunnel_listener, Arc::new(AtomicBool::new(true)));
    });
    crate::share::set_port(if settings.gateway.public_enabled() {
        tunnel_port
    } else {
        port
    });
    let server = Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
    let origin = format!("http://{}", server.server_addr());
    let secret = random_hex(32);
    // The MCP bind owns this instance. Replace only stale discovery data.
    let _ = std::fs::remove_file(&instance);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&instance).map_err(|e| e.to_string())?;
    serde_json::to_writer(
        file,
        &(
            server
                .server_addr()
                .to_ip()
                .ok_or("Missing admin address")?
                .port(),
            &secret,
        ),
    )
    .map_err(|e| e.to_string())?;
    let config_url = format!("{origin}/#{secret}");
    open_browser(&config_url);
    let tray = crate::tray::Tray::start();
    let mut tunnel = Tunnel::default();
    let mut pending: Option<(Settings, Box<PendingLogin>)> = None;
    loop {
        match tray.poll() {
            Some(crate::tray::Command::Open) => open_browser(&config_url),
            Some(crate::tray::Command::Quit) => {
                tunnel.stop();
                let _ = std::fs::remove_file(&instance);
                return Ok(());
            }
            None => {}
        }
        tunnel.poll();
        crate::share::set_origin(tunnel.origin().map(str::to_owned));
        let current = Settings::load();
        crate::share::set_port(if current.gateway.public_enabled() {
            tunnel_port
        } else {
            port
        });
        if current.gateway.public_enabled()
            && !current.services_active().is_empty()
            && tunnel.state == crate::tunnel::State::Idle
        {
            tunnel.start(tunnel_port);
        }
        let Some(mut request) = server
            .recv_timeout(Duration::from_millis(250))
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let path = request.url().to_string();
        let authorization = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str());
        let request_origin = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Origin"))
            .map(|h| h.value.as_str());
        let authorized = token_matches(
            &secret,
            authorization.and_then(|v| v.strip_prefix("Bearer ")),
        ) && request_origin.is_none_or(|value| value == origin);
        let (status, mime, body, stop) = if request.method() == &Method::Get && path == "/" {
            (
                200,
                "text/html; charset=utf-8",
                include_str!("resources/web/index.html").as_bytes().to_vec(),
                false,
            )
        } else if request.method() == &Method::Get && path.starts_with("/assets/") {
            let image: Option<&'static [u8]> = match path.as_str() {
                "/assets/moodle.png" => Some(include_bytes!("resources/assets/moodle-icon.png")),
                "/assets/omnivox.png" => Some(include_bytes!("resources/assets/omnivox-icon.png")),
                "/assets/brightspace.png" => {
                    Some(include_bytes!("resources/assets/brightspace-icon.png"))
                }
                _ => None,
            };
            if let Some(image) = image {
                (200, "image/png", image.to_vec(), false)
            } else {
                (404, "text/plain", Vec::new(), false)
            }
        } else if !authorized {
            (
                401,
                "application/json",
                json!({"error":"Unauthorized"}).to_string().into_bytes(),
                false,
            )
        } else if request.method() == &Method::Get && path == "/state" {
            let mut accounts = Vec::new();
            for service in ["moodle", "omnivox", "brightspace"] {
                for (id, url) in current.account_list(service) {
                    let scoped = current.for_account(service, &id);
                    let allow_write = match service {
                        "moodle" => scoped
                            .moodle
                            .as_ref()
                            .is_some_and(|m| m.allow_write.unwrap_or(false)),
                        "brightspace" => scoped
                            .brightspace
                            .as_ref()
                            .is_some_and(|b| b.allow_write.unwrap_or(false)),
                        _ => false,
                    };
                    accounts.push(
                        json!({"service":service,"id":id,"url":url,"allow_write":allow_write}),
                    );
                }
            }
            (200, "application/json", json!({"accounts":accounts,"verification":pending.as_ref().map(|(_,p)| &p.prompt),"link":mcp_endpoint(tunnel.origin().unwrap_or(&format!("http://127.0.0.1:{port}")), &gateway.token),"local_link":mcp_endpoint(&format!("http://127.0.0.1:{port}"), &gateway.token),"tunnel":format!("{:?}",tunnel.state),"public":current.gateway.public_enabled(),"local":enabled.load(Ordering::SeqCst)}).to_string().into_bytes(),false)
        } else if request.method() == &Method::Post && path == "/action" {
            let mut bytes = Vec::new();
            let read = request.as_reader().take(16_385).read_to_end(&mut bytes);
            let result = if read.is_err() || bytes.len() > 16_384 {
                Err("Request too large or unreadable".into())
            } else {
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| "Invalid JSON".to_string())
                    .and_then(|args| {
                        action(&args, &mut pending, &enabled, &mut tunnel, tunnel_port)
                    })
            };
            match result {
                Ok(value) => {
                    let stop = value["stop"] == true;
                    (
                        200,
                        "application/json",
                        value.to_string().into_bytes(),
                        stop,
                    )
                }
                Err(error) => (
                    400,
                    "application/json",
                    json!({"error":error}).to_string().into_bytes(),
                    false,
                ),
            }
        } else {
            (404, "application/json", b"{}".to_vec(), false)
        };
        let mut response = Response::from_data(body).with_status_code(status);
        for (name,value) in [("Content-Type",mime),("Cache-Control","no-store"),("Referrer-Policy","no-referrer"),("X-Content-Type-Options","nosniff"),("Content-Security-Policy","default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")] {
            response.add_header(Header::from_bytes(name,value).unwrap());
        }
        let _ = request.respond(response);
        if stop {
            tunnel.stop();
            let _ = std::fs::remove_file(&instance);
            return Ok(());
        }
    }
}

fn action(
    args: &Value,
    pending: &mut Option<(Settings, Box<PendingLogin>)>,
    enabled: &AtomicBool,
    tunnel: &mut Tunnel,
    port: u16,
) -> Result<Value, String> {
    let get = |key| args.get(key).and_then(Value::as_str).unwrap_or("");
    match get("action") {
        "stop" => return Ok(json!({"stop":true})),
        "links" => {
            let local = args["local"] == true;
            let public = args["public"] == true;
            Settings::update(|s| {
                s.gateway.local_enabled = Some(local);
                s.gateway.public_enabled = Some(public);
            })
            .map_err(|e| e.to_string())?;
            enabled.store(local, Ordering::SeqCst);
            if public {
                tunnel.start(port);
            } else {
                tunnel.stop();
            }
            return Ok(json!({"message":"Connection settings saved"}));
        }
        "cancel" => {
            *pending = None;
            return Ok(json!({"message":"Cancelled"}));
        }
        "verify" | "resend" => {
            let (settings, challenge) = pending.as_mut().ok_or("No pending verification")?;
            if get("action") == "resend" {
                challenge.resend()?;
                return Ok(json!({"verification":challenge.prompt,"message":"Code sent"}));
            }
            let message = challenge.verify(settings, get("code"))?;
            *pending = None;
            return Ok(json!({"message":message}));
        }
        _ => {}
    }
    let service = get("service");
    if !["moodle", "omnivox", "brightspace"].contains(&service) {
        return Err("Unknown service".into());
    }
    let id = if get("id").is_empty() {
        random_hex(8)
    } else {
        get("id").to_owned()
    };
    let mut settings = Settings::load().for_account(service, &id);
    if get("action") == "permissions" {
        match service {
            "moodle" => crate::mcp::set_moodle_write(&mut settings, args["allow_write"] == true)?,
            "brightspace" => {
                crate::mcp::set_brightspace_write(&mut settings, args["allow_write"] == true)?
            }
            _ => return Err("This service has no write permissions toggle.".into()),
        }
        return Ok(json!({"message":"Permissions saved"}));
    }
    if get("action") == "disconnect" {
        crate::mcp::disable_service(&mut settings, service)?;
        *pending = None;
        return Ok(json!({"message":"Disconnected"}));
    }
    if get("action") == "check" {
        return crate::mcp::test_service(&settings, service)
            .map(|message| json!({"message":message}));
    }
    if get("action") != "connect" {
        return Err("Unknown action".into());
    }
    *pending = None;
    let message = match service {
        "moodle" => crate::mcp::activate_moodle(
            &mut settings,
            get("url"),
            get("username"),
            get("password"),
            get("token"),
            args["allow_write"] == true,
        )?,
        "brightspace" => crate::brightspace::activate(
            &mut settings,
            get("url"),
            get("token"),
            args["allow_write"] == true,
        )?,
        _ => {
            if get("token").is_empty() {
                match crate::omnivox::activate_login(
                    &mut settings,
                    get("url"),
                    get("username"),
                    get("password"),
                )? {
                    LoginOutcome::Connected(message) => message,
                    LoginOutcome::VerificationRequired(challenge) => {
                        let reply = json!({"verification":challenge.prompt});
                        *pending = Some((settings, challenge));
                        return Ok(reply);
                    }
                }
            } else {
                crate::omnivox::activate(&mut settings, get("url"), get("token"))?
            }
        }
    };
    Ok(json!({"message":message}))
}
