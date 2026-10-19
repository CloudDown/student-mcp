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
            eprintln!("Unknown command: {other}");
            usage();
            std::process::exit(2);
        }
    }
}

fn port_from(args: &[String]) -> Option<u16> {
    args.iter().find_map(|value| {
        value
            .strip_prefix("--port=")
            .and_then(|rest| rest.parse::<u16>().ok())
    })
}

fn usage() {
    eprintln!(
        "student-mcp: your local MCP for Moodle, Proxmox and other student services.

Commands:
  student-mcp (no command)        stdio MCP server for local AI clients
  student-mcp up [--port=8420]    Small configuration window + local MCP
                                  (public link through Tailscale Funnel, button in the window)
  student-mcp install             Opens the configuration window
  student-mcp version

Connector URL: /mcp/<token>; the token is in the window and in ~/.student-mcp.json."
    );
}
