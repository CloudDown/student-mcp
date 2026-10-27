use super::identity::{verify_host, Identity};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[path = "tests/fixture.rs"]
mod fixture;

#[test]
fn identity_persists_and_does_not_depend_on_the_local_port_or_mcp_token() {
    crate::config::tests::with_temp_config(|| {
        let path = crate::config::config_path();
        let first = Identity::load(&path).unwrap();
        let mut settings = crate::config::Settings::default();
        settings.gateway.token = Some(crate::config::random_hex(24));
        settings.gateway.port = Some(8421);
        settings.save().unwrap();
        let second = Identity::load(&path).unwrap();
        assert_eq!(first.origin, second.origin);
        assert_eq!(first.key.public_key(), second.key.public_key());
        assert!(first.origin.starts_with("https://"));
        assert!(first.origin.ends_with(".srv.us"));
        let other = Identity::load(&path.with_file_name("other.json")).unwrap();
        assert_ne!(first.origin, other.origin);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path.with_extension("srvus-key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    });
}

#[test]
fn corrupted_identity_is_never_silently_replaced() {
    crate::config::tests::with_temp_config(|| {
        let path = crate::config::config_path();
        let key = path.with_extension("srvus-key");
        std::fs::write(&key, b"invalid").unwrap();
        assert!(Identity::load(&path).is_err());
        assert_eq!(std::fs::read(key).unwrap(), b"invalid");
    });
}

#[cfg(unix)]
#[test]
fn identity_refuses_symlinks_without_touching_their_targets() {
    crate::config::tests::with_temp_config(|| {
        use std::os::unix::fs::symlink;
        let path = crate::config::config_path();
        let target = path.with_file_name("unrelated");
        std::fs::write(&target, b"leave unchanged").unwrap();
        symlink(&target, path.with_extension("srvus-key")).unwrap();
        assert!(Identity::load(&path).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"leave unchanged");
    });
}

#[test]
fn host_key_is_pinned_and_changes_are_refused_without_overwriting() {
    crate::config::tests::with_temp_config(|| {
        let path = crate::config::config_path().with_extension("srvus-host");
        verify_host(&path, b"original").unwrap();
        verify_host(&path, b"original").unwrap();
        assert!(verify_host(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"original");
    });
}

#[test]
fn reconnect_delay_is_bounded_and_stop_clears_the_copyable_url() {
    assert_eq!(retry_delay(0), Duration::from_secs(1));
    assert_eq!(retry_delay(2), Duration::from_secs(4));
    assert_eq!(retry_delay(u32::MAX), Duration::from_secs(30));
    let mut tunnel = Tunnel::default();
    tunnel.state = State::Ready("https://fixture.srv.us".into());
    tunnel.stop();
    assert_eq!(tunnel.origin(), None);
    assert_eq!(tunnel.state, State::Stopped);
    tunnel.state = State::Reconnecting("offline".into());
    assert_eq!(tunnel.origin(), None);
}

// Prevent the detached test listener from routing after config isolation ends.
struct LocalServer(Arc<AtomicBool>);
impl Drop for LocalServer {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn await_ready(tunnel: &mut Tunnel) -> String {
    let started = Instant::now();
    loop {
        tunnel.poll();
        if let Some(origin) = tunnel.origin() {
            return origin.into();
        }
        assert!(
            !matches!(tunnel.state, State::Failed(_)),
            "tunnel failed: {:?}",
            tunnel.state
        );
        assert!(
            started.elapsed() < Duration::from_secs(55),
            "public tunnel timed out"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn check_mcp(origin: &str, token: &str) {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(12))
        .build();
    for path in ["/mcp", "/mcp/wrong-token"] {
        assert!(matches!(
            agent
                .post(&format!("{origin}{path}"))
                .send_json(serde_json::json!({})),
            Err(ureq::Error::Status(401, _))
        ));
    }
    let endpoint = crate::config::mcp_endpoint(origin, token);
    let response = agent.post(&endpoint).set("Accept", "application/json, text/event-stream")
        .send_json(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"synthetic-srvus-probe","version":"1"}
        }})).unwrap_or_else(|_| panic!("public initialize failed"));
    let session = response.header("Mcp-Session-Id").unwrap().to_string();
    let initialized: serde_json::Value = response.into_json().unwrap();
    assert!(initialized["result"]["serverInfo"].is_object());
    let tools: serde_json::Value = agent
        .post(&endpoint)
        .set("Mcp-Session-Id", &session)
        .send_json(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap_or_else(|_| panic!("public tools/list failed"))
        .into_json()
        .unwrap();
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1, "no school accounts may be exposed");
    assert_eq!(tools[0]["name"], "student_status");
}

#[test]
#[ignore = "Opens real srv.us tunnels with an isolated, account-free MCP server"]
fn public_mcp_smoke() {
    crate::config::tests::with_temp_config(|| {
        crate::config::Settings::default().save().unwrap();
        let gateway = crate::config::Settings::load().gateway();
        let listener = crate::http::bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let enabled = Arc::new(AtomicBool::new(true));
        let _server = LocalServer(Arc::clone(&enabled));
        thread::spawn(move || {
            let _ = crate::http::serve_controlled(listener, enabled);
        });
        let mut tunnel = Tunnel::default();
        tunnel.start(port);
        let first = await_ready(&mut tunnel);
        check_mcp(&first, &gateway.token);
        let stopped_at = Instant::now();
        tunnel.stop();
        assert!(stopped_at.elapsed() < Duration::from_secs(2));
        assert!(tunnel.origin().is_none());
        tunnel.start(port);
        let second = await_ready(&mut tunnel);
        assert_eq!(first, second, "restarting must preserve the full link");
        check_mcp(&second, &gateway.token);
        tunnel.stop();
        eprintln!("Embedded srv.us tunnel: initialize, tools/list, auth denial, stable restart and stop passed; no school accounts used.");
    });
}
