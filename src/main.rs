use student_mcp::{run_stdio, run_web, run_web_background, VERSION};

fn main() {
    student_mcp::paths::init();
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
        Some("logs") => print_logs(args.collect()),
        Some("settings-url") => match student_mcp::settings_url() {
            Ok(url) => println!("{url}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        Some("--app-icon") => {
            use std::io::Write;
            if std::io::stdout()
                .write_all(include_bytes!("resources/assets/app-icon.png"))
                .is_err()
            {
                std::process::exit(1);
            }
        }
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
        "student-mcp: your local MCP for Moodle, Omnivox, Brightspace and Canvas.

Commands:
  student-mcp (no command)        stdio MCP server for local AI clients
  student-mcp up [--port=8420]    Local browser configuration + MCP
                                  (automatic stable public link through srv.us)
  student-mcp up --background    Start quietly at login (tray only)
  student-mcp install             Opens the local configuration page
  student-mcp settings-url        Print the running local settings URL
  student-mcp logs [--lines=50] [--follow] [--mcp]
                                  Show the local activity log (tools, Moodle calls,
                                  file deliveries). --mcp shows inbound requests.
  student-mcp version
  student-mcp --licenses          Bundled licence texts and notices

Connector URL: /mcp/<token>; the token is in the page and in ~/.student-mcp.json."
    );
}

fn print_logs(args: Vec<String>) {
    use std::{
        io::{Read, Seek, SeekFrom, Write},
        time::Duration,
    };
    let mut lines = 50usize;
    let mut follow = false;
    let mut mcp = false;
    for arg in args {
        if arg == "--follow" {
            follow = true;
        } else if arg == "--mcp" {
            mcp = true;
        } else if let Some(value) = arg.strip_prefix("--lines=") {
            lines = value.parse().unwrap_or(50).clamp(1, 10_000);
        }
    }
    let path = if mcp {
        student_mcp::mcp_log_path()
    } else {
        student_mcp::activity_log_path()
    };
    eprintln!("{}", path.display());
    let mut printed = false;
    if let Ok(text) = std::fs::read_to_string(&path) {
        let all = text.lines().collect::<Vec<_>>();
        for line in all.iter().skip(all.len().saturating_sub(lines)) {
            println!("{line}");
        }
        printed = true;
    }
    if !printed {
        println!("No activity recorded yet. Use the app, then run this command again.");
    }
    if !follow {
        return;
    }
    let mut offset = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.len() <= offset {
            if meta.len() < offset {
                offset = 0;
            } else {
                continue;
            }
        }
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            continue;
        }
        let mut buffer = String::new();
        if file.read_to_string(&mut buffer).is_ok() && !buffer.is_empty() {
            print!("{buffer}");
            let _ = std::io::stdout().flush();
            offset += buffer.len() as u64;
        }
    }
}
