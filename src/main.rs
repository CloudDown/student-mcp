mod catalog;
mod config;
mod funnel;
mod gui;
mod http;
mod logos;
mod mcp;
mod moodle;
mod proxmox;
mod stdio;

use config::VERSION;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => stdio::run(), // default for local AI clients (OpenCode, Claude Desktop, ...)
        Some("up") => gui::run(port_from(&args.collect::<Vec<String>>()), false),
        Some("install") => gui::run(None, true), // popup right away: configuration lives there
        Some("version" | "--version" | "-V") => println!("student-mcp {VERSION}"),
        Some("--help" | "-h" | "help") => usage(),
        Some(other) => {
            eprintln!("Commande inconnue : {other}");
            usage();
            std::process::exit(2);
        }
    }
}

fn port_from(args: &[String]) -> Option<u16> {
    args.iter()
        .find_map(|value| value.strip_prefix("--port=").and_then(|rest| rest.parse::<u16>().ok()))
}

fn usage() {
    eprintln!(
        "student-mcp : ton MCP local pour Moodle, Proxmox et autres services étudiants.

Commandes :
  student-mcp (sans commande)     Serveur MCP stdio pour clients IA locaux
  student-mcp up [--port=8420]    Petite fenêtre de configuration + MCP local
                                  (lien public via Tailscale Funnel, bouton dans la fenêtre)
  student-mcp install             Ouvre la fenêtre de configuration
  student-mcp version

L'URL du connecteur : /mcp/<token> ; token dans la fenêtre et ~/.student-mcp.json."
    );
}
