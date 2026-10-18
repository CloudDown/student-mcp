use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct TailscaleInfo {
    pub present: bool,
    pub logged_in: bool,
    /// Device DNS name inside the tailnet, e.g. laptop.tailXXXX.ts.net
    pub host: Option<String>,
    /// Whether a funnel already proxies 443 to something.
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
pub fn info() -> TailscaleInfo {
    if run("tailscale", &["version"]).is_none() {
        return TailscaleInfo {
            present: false,
            logged_in: false,
            host: None,
            funnel_on: false,
            message: Some("Tailscale n'est pas installé (tailscale.com/download).".to_string()),
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
                message: Some("Connecte-toi à Tailscale : exécute `tailscale login` (ou ouvre l'app).".to_string()),
            }
        }
    };
    let parsed: Option<Value> = serde_json::from_str(&output_text(&status)).ok();
    let backend_running = parsed.as_ref().and_then(|v| v.get("BackendState")).and_then(Value::as_str) == Some("Running");
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
            message: Some("Connecte-toi à Tailscale : exécute `tailscale login` (ou ouvre l'app).".to_string()),
        };
    }
    let serve_status = run("tailscale", &["serve", "status"]).map(|output| output_text(&output)).unwrap_or_default();
    let funnel_on = serve_status.to_lowercase().contains("funnel");
    TailscaleInfo { present: true, logged_in: true, host, funnel_on, message: None }
}

/// Public HTTPS connector URL advertised through Tailscale Funnel.
pub fn public_connector_url(host: &str, token: &str) -> String {
    format!("https://{host}/mcp/{token}")
}

/// Turn the funnel on; on Linux this may require sudo, so the message has the exact command.
pub fn funnel_enable(port: u16) -> Result<String, String> {
    let info = info();
    if !info.present {
        return Err("Installe d'abord Tailscale (tailscale.com/download), connecte-toi, puis réessaie.".into());
    }
    if !info.logged_in {
        return Err(info.message.clone().unwrap_or_else(|| "Connecte-toi à Tailscale d'abord : `tailscale login`.".into()));
    }
    let port_text = port.to_string();
    let direct_ok = run("tailscale", &["funnel", "--bg", &port_text])
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !direct_ok {
        let sudo_ok = run("sudo", &["-n", "tailscale", "funnel", "--bg", &port_text])
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !sudo_ok {
            return Err(format!(
                "Tailscale n'a pas pu activer le funnel automatiquement. Exécute une fois :\n  sudo tailscale funnel --bg {port}\n puis réessaie."
            ));
        }
    }
    Ok(format!(
        "Funnel actif : certificat HTTPS de {} émis par Tailscale.",
        info.host.as_deref().unwrap_or("?")
    ))
}

/// Remove the published URL once the popup is closed.
pub fn funnel_disable() -> String {
    let _ = run("tailscale", &["funnel", "off"]);
    let _ = run("sudo", &["-n", "tailscale", "funnel", "off"]);
    "Funnel retiré : l'URL publique ne répond plus.".to_string()
}
