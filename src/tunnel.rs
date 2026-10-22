//! Own a temporary Quick Tunnel. No accounts, system daemon or shell commands.
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Starting,
    Ready(String),
    Failed(String),
    Stopped,
}

#[derive(Default)]
pub struct Tunnel {
    pub state: State,
    worker: Option<Worker>,
}

struct Worker {
    stop: Sender<()>,
    events: Receiver<State>,
    join: JoinHandle<()>,
}

impl Tunnel {
    pub fn start(&mut self, port: u16) {
        self.stop();
        self.state = State::Starting;
        let (stop, stops) = channel();
        let (events, receiver) = channel();
        let join = thread::spawn(move || {
            let result = binary_path().and_then(|path| supervise(path, port, stops, &events));
            if let Err(message) = result {
                let _ = events.send(State::Failed(message));
            }
        });
        self.worker = Some(Worker {
            stop,
            events: receiver,
            join,
        });
    }

    pub fn poll(&mut self) {
        if let Some(worker) = &self.worker {
            while let Ok(state) = worker.events.try_recv() {
                self.state = state;
            }
            if worker.join.is_finished() {
                let worker = self.worker.take().unwrap();
                if worker.join.join().is_err() {
                    self.state =
                        State::Failed("The public link was interrupted. Try again.".into());
                }
            }
        }
    }

    pub fn origin(&self) -> Option<&str> {
        match &self.state {
            State::Ready(url) => Some(url),
            _ => None,
        }
    }

    pub fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.stop.send(());
            let _ = worker.join.join();
        }
        self.state = State::Stopped;
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

fn binary_path() -> Result<PathBuf, String> {
    // Explicit override for development only; never search an untrusted PATH.
    let path = if let Some(path) = std::env::var_os("STUDENT_MCP_CLOUDFLARED") {
        PathBuf::from(path)
    } else {
        std::env::current_exe()
            .map_err(|_| "Cannot locate the installed app.".to_string())?
            .with_file_name(if cfg!(windows) {
                "cloudflared.exe"
            } else {
                "cloudflared"
            })
    };
    if !path.is_file() {
        return Err("The bundled cloudflared program is missing. Reinstall student-mcp to create a public link.".into());
    }
    Ok(path)
}

fn command(path: PathBuf, port: u16) -> Command {
    let mut command = Command::new(path);
    command
        .args([
            "tunnel",
            "--no-autoupdate",
            "--config",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
            "--url",
            &format!("http://127.0.0.1:{port}"),
            // HTTP/2 works on networks that block the QUIC UDP port.
            "--protocol",
            "http2",
            "--metrics",
            "127.0.0.1:0",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

fn supervise(
    path: PathBuf,
    port: u16,
    stop: Receiver<()>,
    events: &Sender<State>,
) -> Result<(), String> {
    let mut child = command(path, port).spawn().map_err(|_| {
        "Cannot start cloudflared. Reinstall the app or check your security software.".to_string()
    })?;
    let stderr = child.stderr.take().unwrap();
    let (urls, receiver) = channel();
    // Drain logs for the whole lifetime so the child's stderr can never fill up.
    // Never retain or display logs, which may contain network details.
    let reader = thread::spawn(move || {
        let mut registered = false;
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if let Some(url) = quick_url(&line) {
                let _ = urls.send((Some(url), false));
            }
            if !registered && line.contains("Registered tunnel connection") {
                registered = true;
                let _ = urls.send((None, true));
            }
        }
    });
    let started = Instant::now();
    let mut announced = false;
    let mut origin = None;
    let mut connected = false;
    let result = loop {
        if stop.try_recv() != Err(std::sync::mpsc::TryRecvError::Empty) {
            break Ok(());
        }
        match child.try_wait() {
            Ok(Some(_)) => break Err("The public tunnel stopped. Check your Internet connection and retry; the new link may change.".into()),
            Err(_) => break Err("Cannot check the public tunnel. Try again.".into()),
            Ok(None) => {}
        }
        if !announced {
            while let Ok((url, registered)) = receiver.try_recv() {
                if url.is_some() {
                    origin = url;
                }
                connected |= registered;
            }
            if connected && origin.is_some() {
                announced = true;
                let _ = events.send(State::Ready(origin.take().unwrap()));
            } else if started.elapsed() > Duration::from_secs(60) {
                break Err("Cloudflare did not connect within 60 seconds. Check your Internet connection and retry.".into());
            }
        }
        thread::sleep(Duration::from_millis(100));
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    result
}

fn quick_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|word| {
        let url = url::Url::parse(word).ok()?;
        let host = url.host_str()?;
        let name = host.strip_suffix(".trycloudflare.com")?;
        (url.scheme() == "https"
            && !name.is_empty()
            && !name.contains('.')
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none())
        .then(|| format!("https://{host}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_a_quick_tunnel_origin() {
        assert_eq!(
            quick_url("INF | https://test-link.trycloudflare.com |"),
            Some("https://test-link.trycloudflare.com".into())
        );
        for value in [
            "https://trycloudflare.com",
            "http://test.trycloudflare.com",
            "https://test.trycloudflare.com.evil.org",
            "https://test.trycloudflare.com/mcp/secret",
            "https://user@test.trycloudflare.com",
            "https://test.trycloudflare.com?token=secret",
            "https://a.b.trycloudflare.com",
        ] {
            assert_eq!(quick_url(value), None, "{value}");
        }
    }

    #[test]
    fn command_ignores_user_config_and_exposes_only_loopback_port() {
        let cmd = command("cloudflared".into(), 8423);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args
            .windows(2)
            .any(|a| a == ["--url", "http://127.0.0.1:8423"]));
        assert!(args
            .windows(2)
            .any(|a| a == ["--config", if cfg!(windows) { "NUL" } else { "/dev/null" }]));
    }

    #[test]
    fn stop_clears_the_copyable_url() {
        let mut tunnel = Tunnel::default();
        tunnel.state = State::Ready("https://test.trycloudflare.com".into());
        tunnel.stop();
        assert_eq!(tunnel.origin(), None);
        assert_eq!(tunnel.state, State::Stopped);
    }

    #[cfg(unix)]
    #[test]
    fn supervisor_reports_child_exit_and_cancels_a_running_child() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "student-tunnel-{}-{}",
            std::process::id(),
            crate::config::random_hex(4)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-cloudflared");
        std::fs::write(&script, "#!/bin/sh\necho 'INF | https://fixture.trycloudflare.com |' >&2\necho 'INF Registered tunnel connection' >&2\nwhile :; do :; done\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (stop, stops) = channel();
        let (events, receiver) = channel();
        let path = script.clone();
        let worker = thread::spawn(move || supervise(path, 8420, stops, &events));
        let ready = receiver.recv_timeout(Duration::from_secs(5));
        let _ = stop.send(());
        assert!(worker.join().unwrap().is_ok()); // Child has been killed and reaped.
        assert_eq!(
            ready.unwrap(),
            State::Ready("https://fixture.trycloudflare.com".into())
        );

        std::fs::write(&script, "#!/bin/sh\necho 'INF | https://fixture.trycloudflare.com |' >&2\nwhile :; do :; done\n").unwrap();
        let (stop, stops) = channel();
        let (events, receiver) = channel();
        let path = script.clone();
        let worker = thread::spawn(move || supervise(path, 8420, stops, &events));
        let premature = receiver.recv_timeout(Duration::from_millis(300));
        let _ = stop.send(());
        assert!(worker.join().unwrap().is_ok());
        assert!(premature.is_err()); // A generated hostname alone is not a connection.

        std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        let (_stop, stops) = channel();
        let (events, _) = channel();
        assert!(supervise(script.clone(), 8420, stops, &events)
            .unwrap_err()
            .contains("stopped"));
        std::fs::remove_file(script).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    #[ignore = "Creates a real public Quick Tunnel with an isolated, unconfigured MCP server"]
    fn public_mcp_smoke() {
        crate::config::tests::with_temp_config(|| {
            let gateway = crate::config::Settings::default().gateway();
            let listener = crate::http::bind(0).unwrap();
            let port = listener.local_addr().unwrap().port();
            thread::spawn(move || {
                let _ = crate::http::serve(listener);
            });
            let mut tunnel = Tunnel::default();
            tunnel.start(port);
            let start = Instant::now();
            while tunnel.state == State::Starting && start.elapsed() < Duration::from_secs(65) {
                thread::sleep(Duration::from_millis(100));
                tunnel.poll();
            }
            let origin = tunnel
                .origin()
                .expect("public tunnel should start")
                .to_string();
            let endpoint = crate::config::mcp_endpoint(&origin, &gateway.token);
            let agent = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(8))
                .build();
            let response = loop {
                let response = agent.post(&endpoint)
                    .set("Accept", "application/json, text/event-stream")
                    .send_json(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}));
                match response {
                    Ok(response) => break response,
                    Err(error) if start.elapsed() < Duration::from_secs(90) => {
                        eprintln!("Waiting for Quick Tunnel connectivity: {error}");
                        thread::sleep(Duration::from_secs(2));
                    }
                    Err(error) => panic!("Public initialize failed: {error}"),
                }
            };
            let session = response.header("Mcp-Session-Id").unwrap().to_string();
            let initialized: serde_json::Value = response.into_json().unwrap();
            assert!(initialized["result"]["serverInfo"].is_object());
            let tools: serde_json::Value = agent
                .post(&endpoint)
                .set("Mcp-Session-Id", &session)
                .send_json(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
                .unwrap()
                .into_json()
                .unwrap();
            assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 1);
            assert!(matches!(
                agent
                    .post(&format!("{origin}/mcp/wrong-token"))
                    .send_json(serde_json::json!({})),
                Err(ureq::Error::Status(401, _))
            ));
            tunnel.stop();
            assert!(tunnel.origin().is_none());
            eprintln!(
                "Public MCP initialize, tools/list, authentication and tunnel shutdown passed."
            );
        });
    }
}
