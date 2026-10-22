use serde_json::Value;
use std::process::Command;

// Prefixes are restored on the upstream URL because Tailscale strips set-path
// before forwarding. Keep discovery probes away from unrelated root services.
const MOUNTS: [&str; 4] = [
    "/mcp",
    "/.well-known/oauth-authorization-server",
    "/.well-known/oauth-protected-resource",
    "/.well-known/openid-configuration",
];

#[derive(Debug, Clone)]
pub struct TailscaleInfo {
    pub present: bool,
    pub logged_in: bool,
    /// Device DNS name inside the tailnet, e.g. laptop.tailXXXX.ts.net
    pub host: Option<String>,
    /// Whether this MCP endpoint (not merely another app) is public on 443.
    pub funnel_on: bool,
    pub message: Option<String>,
}

fn run(program: &str, args: &[&str]) -> Option<std::process::Output> {
    Command::new(program).args(args).output().ok()
}

fn output_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Inspect the local tailscale CLI without ever panicking, for popup UX.
pub fn info(port: u16) -> TailscaleInfo {
    if run("tailscale", &["version"]).is_none() {
        return TailscaleInfo {
            present: false,
            logged_in: false,
            host: None,
            funnel_on: false,
            message: Some("Tailscale is not installed (tailscale.com/download).".to_string()),
        };
    }
    let status = match run("tailscale", &["status", "--json"]) {
        Some(output) if output.status.success() => output,
        _ => {
            return TailscaleInfo {
                present: true,
                logged_in: false,
                host: None,
                funnel_on: false,
                message: Some(
                    "Sign in to Tailscale: run `tailscale login` (or open the app).".to_string(),
                ),
            }
        }
    };
    let parsed: Option<Value> = serde_json::from_str(&output_text(&status)).ok();
    let backend_running = parsed
        .as_ref()
        .and_then(|v| v.get("BackendState"))
        .and_then(Value::as_str)
        == Some("Running");
    let host = parsed
        .as_ref()
        .and_then(|v| v.get("Self"))
        .and_then(|v| v.get("DNSName"))
        .and_then(Value::as_str)
        .map(|dns| dns.trim_end_matches('.').to_string());
    let behind = backend_running && host.is_some();
    if !behind {
        return TailscaleInfo {
            present: true,
            logged_in: false,
            host: None,
            funnel_on: false,
            message: Some(
                "Sign in to Tailscale: run `tailscale login` (or open the app).".to_string(),
            ),
        };
    }
    let serve_status = serve_config();
    let funnel_on = serve_status.as_ref().is_ok_and(|config| {
        host.as_deref()
            .is_some_and(|host| mcp_is_public(config, host, port))
    });
    TailscaleInfo {
        present: true,
        logged_in: true,
        host,
        funnel_on,
        message: serve_status.err(),
    }
}

fn serve_config() -> Result<Value, String> {
    let output = run("tailscale", &["funnel", "status", "--json"])
        .filter(|output| output.status.success())
        .ok_or("Cannot read Tailscale Funnel configuration.")?;
    serde_json::from_str(&output_text(&output))
        .map_err(|_| "Tailscale did not return a valid Funnel configuration.".into())
}

fn handlers<'a>(config: &'a Value, host: &str) -> Option<&'a serde_json::Map<String, Value>> {
    config
        .get("Web")?
        .get(format!("{host}:443"))?
        .get("Handlers")?
        .as_object()
}

fn proxy_matches(handler: &Value, port: u16, path: &str) -> bool {
    let Some(proxy) = handler.get("Proxy").and_then(Value::as_str) else {
        return false;
    };
    let Ok(url) = url::Url::parse(proxy) else {
        return false;
    };
    url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        && url.port_or_known_default() == Some(port)
        && url.path().trim_end_matches('/') == path
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn mcp_is_public(config: &Value, host: &str, port: u16) -> bool {
    if config
        .get("TCP")
        .and_then(|v| v.get("443"))
        .and_then(|v| v.get("HTTPS"))
        .and_then(Value::as_bool)
        != Some(true)
    {
        return false;
    }
    if config
        .get("AllowFunnel")
        .and_then(|v| v.get(format!("{host}:443")))
        .and_then(Value::as_bool)
        != Some(true)
    {
        return false;
    }
    let Some(handlers) = handlers(config, host) else {
        return false;
    };
    if let Some(handler) = handlers.get("/mcp").or_else(|| handlers.get("/mcp/")) {
        return proxy_matches(handler, port, "/mcp");
    }
    // Recognize a legacy standalone root proxy, but never an arbitrary hub.
    handlers
        .get("/")
        .is_some_and(|handler| proxy_matches(handler, port, ""))
}

fn enable_routes(config: &Value, host: &str, port: u16) -> Result<Vec<(String, String)>, String> {
    let handlers = handlers(config, host);
    let mut routes = Vec::new();
    for path in MOUNTS {
        if let Some(existing) =
            handlers.and_then(|h| h.get(path).or_else(|| h.get(&format!("{path}/"))))
        {
            if !proxy_matches(existing, port, path) {
                return Err(format!("Tailscale route {path} is already used by another service. It was not changed."));
            }
        }
        routes.push((path.into(), format!("http://127.0.0.1:{port}{path}")));
    }
    Ok(routes)
}

fn run_funnel(args: &[&str]) -> bool {
    if run("tailscale", args).is_some_and(|output| output.status.success()) {
        return true;
    }
    let mut sudo_args = vec!["-n", "tailscale"];
    sudo_args.extend_from_slice(args);
    run("sudo", &sudo_args).is_some_and(|output| output.status.success())
}

fn enable_args(path: &str, target: &str) -> Vec<String> {
    vec![
        "funnel".into(),
        "--bg".into(),
        "--https=443".into(),
        format!("--set-path={path}"),
        target.into(),
    ]
}

fn disable_args(path: &str) -> Vec<String> {
    vec![
        "funnel".into(),
        "--https=443".into(),
        format!("--set-path={path}"),
        "off".into(),
    ]
}

fn apply(args: &[String]) -> Result<(), String> {
    if run_funnel(&args.iter().map(String::as_str).collect::<Vec<_>>()) {
        Ok(())
    } else {
        Err(format!(
            "Tailscale could not update the MCP route. Run:\n  sudo tailscale {}\n then try again.",
            args.join(" ")
        ))
    }
}

/// Public HTTPS connector URL advertised through Tailscale Funnel.
pub fn public_connector_url(host: &str, token: &str) -> String {
    format!("https://{host}/mcp/{token}")
}

/// Turn the funnel on; on Linux this may require sudo, so the message has the exact command.
pub fn funnel_enable(port: u16) -> Result<String, String> {
    let info = info(port);
    if !info.present {
        return Err(
            "Install Tailscale first (tailscale.com/download), sign in, then try again.".into(),
        );
    }
    if !info.logged_in {
        return Err(info
            .message
            .clone()
            .unwrap_or_else(|| "Sign in to Tailscale first: `tailscale login`.".into()));
    }
    let host = info
        .host
        .as_deref()
        .ok_or("Tailscale did not return a hostname.")?;
    let config = serve_config()?;
    // Preflight every route before modifying anything: never replace another app.
    for (path, target) in enable_routes(&config, host, port)? {
        apply(&enable_args(&path, &target))?;
    }
    if !mcp_is_public(&serve_config()?, host, port) {
        return Err("Tailscale did not publish the MCP endpoint on this port.".into());
    }
    Ok(format!(
        "Public MCP link active on {host}. Other applications were left unchanged."
    ))
}

fn removable_routes(config: &Value, host: &str, port: u16) -> Vec<String> {
    let Some(handlers) = handlers(config, host) else {
        return Vec::new();
    };
    handlers
        .iter()
        .filter_map(|(path, handler)| {
            let normalized = path.trim_end_matches('/');
            let owned = (MOUNTS.contains(&normalized) || path == "/")
                && proxy_matches(handler, port, normalized);
            owned.then(|| path.clone())
        })
        .collect()
}

/// Remove only our routes, not all Funnel handlers on this shared machine.
pub fn funnel_disable(port: u16) -> Result<String, String> {
    let info = info(port);
    let host = info
        .host
        .as_deref()
        .ok_or("Cannot find the Tailscale hostname.")?;
    for path in removable_routes(&serve_config()?, host, port) {
        apply(&disable_args(&path))?;
    }
    if mcp_is_public(&serve_config()?, host, port) {
        return Err("The MCP endpoint is still public. Check the Tailscale routes.".into());
    }
    Ok("Public MCP link removed. Other applications were left unchanged.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HOST: &str = "apps.example.ts.net";

    fn shared_hub() -> Value {
        json!({
            "TCP": {"443": {"HTTPS": true}},
            "Web": {"apps.example.ts.net:443": {"Handlers": {
                "/": {"Proxy": "http://127.0.0.1:18080"},
                "/other": {"Proxy": "http://127.0.0.1:8000"}
            }}},
            "AllowFunnel": {"apps.example.ts.net:443": true}
        })
    }

    fn add_route(config: &mut Value, path: &str, target: &str) {
        config["Web"]["apps.example.ts.net:443"]["Handlers"][path] = json!({"Proxy": target});
    }

    #[test]
    fn another_apps_funnel_does_not_make_the_mcp_public() {
        let mut config = shared_hub();
        assert!(!mcp_is_public(&config, HOST, 8420));
        add_route(&mut config, "/mcp", "http://127.0.0.1:8420/mcp");
        assert!(mcp_is_public(&config, HOST, 8420));
        assert!(!mcp_is_public(&config, HOST, 8421));
        assert!(!mcp_is_public(&config, "another.example.ts.net", 8420));
        config["AllowFunnel"]["apps.example.ts.net:443"] = json!(false);
        assert!(!mcp_is_public(&config, HOST, 8420));
    }

    #[test]
    fn root_mcp_is_recognized_but_a_shadowing_route_is_not() {
        let mut config = shared_hub();
        add_route(&mut config, "/", "http://127.0.0.1:8420");
        assert!(mcp_is_public(&config, HOST, 8420));
        add_route(&mut config, "/mcp", "http://127.0.0.1:18080");
        assert!(!mcp_is_public(&config, HOST, 8420));
    }

    #[test]
    fn mcp_mount_must_restore_its_prefix_and_use_the_current_local_port() {
        let mut config = shared_hub();
        for target in [
            "http://127.0.0.1:8420", // Would strip /mcp and forward only the token.
            "http://127.0.0.1:8421/mcp",
            "http://remote.example:8420/mcp",
            "http://127.0.0.1:8420/mcp?query=1",
        ] {
            add_route(&mut config, "/mcp", target);
            assert!(!mcp_is_public(&config, HOST, 8420), "{target}");
        }
        add_route(&mut config, "/mcp", "http://localhost:8420/mcp");
        assert!(mcp_is_public(&config, HOST, 8420));
    }

    #[test]
    fn enable_scopes_every_command_and_preserves_unrelated_routes() {
        let mut config = shared_hub();
        let original = config.clone();
        for (path, target) in enable_routes(&config, HOST, 8420).unwrap() {
            assert_ne!(path, "/");
            let args = enable_args(&path, &target);
            assert!(args.contains(&format!("--set-path={path}")));
            assert_eq!(
                args.last().unwrap(),
                &format!("http://127.0.0.1:8420{path}")
            );
            add_route(&mut config, &path, &target);
        }
        assert_eq!(
            config["Web"]["apps.example.ts.net:443"]["Handlers"]["/"],
            original["Web"]["apps.example.ts.net:443"]["Handlers"]["/"]
        );
        assert_eq!(
            config["Web"]["apps.example.ts.net:443"]["Handlers"]["/other"],
            original["Web"]["apps.example.ts.net:443"]["Handlers"]["/other"]
        );
        assert!(mcp_is_public(&config, HOST, 8420));
        assert_eq!(
            enable_routes(&config, HOST, 8420).unwrap().len(),
            MOUNTS.len()
        );
    }

    #[test]
    fn conflicting_routes_are_rejected_before_any_updates() {
        for path in MOUNTS {
            let mut config = shared_hub();
            add_route(&mut config, path, "http://127.0.0.1:8000");
            assert!(enable_routes(&config, HOST, 8420).is_err());
        }
    }

    #[test]
    fn disable_removes_only_owned_routes_including_legacy_root() {
        let mut config = shared_hub();
        for (path, target) in enable_routes(&config, HOST, 8420).unwrap() {
            add_route(&mut config, &path, &target);
        }
        let owned = removable_routes(&config, HOST, 8420);
        assert_eq!(owned.len(), MOUNTS.len());
        assert!(!owned.contains(&"/".into()));
        assert!(!owned.contains(&"/other".into()));
        assert!(removable_routes(&config, HOST, 8421).is_empty());
        for path in owned {
            let args = disable_args(&path);
            assert!(args.contains(&format!("--set-path={path}")));
            assert_eq!(args.last().unwrap(), "off");
        }
        add_route(&mut config, "/", "http://127.0.0.1:8420");
        assert!(removable_routes(&config, HOST, 8420).contains(&"/".into()));
    }
}
