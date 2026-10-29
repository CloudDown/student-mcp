use student_mcp::{run_stdio, run_web, run_web_background, VERSION};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => run_stdio(),
        Some("up") => {
            let args: Vec<String> = args.collect();
            let port = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--port=")?.parse().ok());
            if args.iter().any(|arg| arg == "--background") {
                run_web_background(port);
            } else {
                run_web(port, false);
            }
        }
        Some("install") => run_web(None, true),
        Some("version" | "--version" | "-V") => println!("student-mcp {VERSION}"),
        Some("--licenses") => print!(
            "{}\n{}\n{}",
            include_str!("../LICENSE"),
            include_str!("services/moodle/catalog/LICENSE.md"),
            include_str!("services/moodle/catalog/COPYING")
        ),
        Some("--help" | "-h" | "help") => usage(),
        Some(other) => {
            eprintln!("Unknown command: {other}");
            usage();
            std::process::exit(2);
        }
    }
}

fn usage() {
    eprintln!(
        "student-mcp: your local MCP for Moodle, Omnivox and Brightspace.

Commands:
  student-mcp (no command)        stdio MCP server for local AI clients
  student-mcp up [--port=8420]    Local browser configuration + MCP
                                  (automatic stable public link through srv.us)
  student-mcp up --background    Start quietly at login (tray only)
  student-mcp install             Opens the local configuration page
  student-mcp version
  student-mcp --licenses          Bundled licence texts and notices

Connector URL: /mcp/<token>; the token is in the page and in ~/.student-mcp.json."
    );
}
