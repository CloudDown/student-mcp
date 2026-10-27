use crate::config::{random_hex, token_matches, Settings};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_HEADER: usize = 32 * 1024;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 64;
const MAX_SESSIONS: usize = 256;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// A slot is released on every exit path, including a failed thread spawn.
struct ConnectionSlot(Arc<AtomicUsize>);

impl ConnectionSlot {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < MAX_CONNECTIONS).then_some(count + 1)
            })
            .ok()?;
        Some(Self(Arc::clone(active)))
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Bind the local MCP endpoint (the managed tunnel proxies here).
pub fn bind(port: u16) -> Result<TcpListener, std::io::Error> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Accept loop: one thread per connection, keep-alive honoured.
pub fn serve_controlled(listener: TcpListener, enabled: Arc<AtomicBool>) -> Result<(), String> {
    let sessions = Arc::new(Mutex::new(Sessions::default()));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Drop excess connections without blocking the accept loop.
                let Some(slot) = ConnectionSlot::acquire(&active) else {
                    continue;
                };
                let sessions = Arc::clone(&sessions);
                let enabled = Arc::clone(&enabled);
                let _ = std::thread::Builder::new().spawn(move || {
                    let _slot = slot;
                    let _ = connection(stream, sessions, enabled);
                });
            }
            Err(_) => continue, // transient accept failure: keep serving
        }
    }
    Err("Local server stopped.".to_string())
}

struct Request {
    method: String,
    path: String,
    accept_sse: bool,
    session: Option<String>,
    authorization: Option<String>,
    protocol_version: Option<String>,
    body: Vec<u8>,
    close: bool,
}

#[derive(Default)]
struct Sessions {
    order: VecDeque<String>,
    live: HashMap<String, ()>,
}

impl Sessions {
    fn insert(&mut self, id: String) {
        if self.live.contains_key(&id) {
            return;
        }
        while self.live.len() >= MAX_SESSIONS {
            match self.order.pop_front() {
                Some(old) => {
                    self.live.remove(&old);
                }
                None => {
                    self.live.clear();
                    break;
                }
            }
        }
        self.order.push_back(id.clone());
        self.live.insert(id, ());
    }

    fn contains(&self, id: &str) -> bool {
        self.live.contains_key(id)
    }

    fn remove(&mut self, id: &str) {
        self.live.remove(id);
        self.order.retain(|saved| saved != id);
    }
}

fn connection(
    mut stream: TcpStream,
    sessions: Arc<Mutex<Sessions>>,
    enabled: Arc<AtomicBool>,
) -> std::io::Result<()> {
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut pending = Vec::new();
    loop {
        let request = match read_request(&mut stream, &mut pending, REQUEST_TIMEOUT) {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()), // client closed cleanly
            Err(error) => {
                let (status, body) = error;
                respond(
                    &stream,
                    status,
                    &[("Content-Type", "application/json; charset=utf-8")],
                    body.as_bytes(),
                )?;
                return Ok(());
            }
        };
        if !enabled.load(Ordering::SeqCst) {
            respond(
                &stream,
                503,
                &[("Content-Type", "application/json")],
                b"{\"error\":\"Local link disabled.\"}",
            )?;
            return Ok(());
        }
        let (status, headers, body) = route(&request, &sessions);
        let terminate = request.close || status == 204;
        write_response(&stream, status, headers, &body, !terminate)?;
        if terminate {
            return Ok(());
        }
    }
}

type HttpError = (u16, String);

fn read_request(
    stream: &mut TcpStream,
    pending: &mut Vec<u8>,
    timeout: Duration,
) -> Result<Option<Request>, HttpError> {
    let deadline = Instant::now() + timeout;
    let mut buffer = std::mem::take(pending);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = find_double_crlf(&buffer) {
            if position > MAX_HEADER {
                return Err((431, "{\"error\":\"Headers too large.\"}".to_string()));
            }
            break position;
        }
        if buffer.len() > MAX_HEADER {
            return Err((431, "{\"error\":\"Headers too large.\"}".to_string()));
        }
        let read = read_before(stream, &mut chunk, deadline)?;
        if read == 0 {
            if buffer.is_empty() {
                return Ok(None); // clean EOF between keep-alive requests
            }
            return Err(broken(()));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default().to_uppercase();
    let target = first.next().unwrap_or_default().to_string();
    if method.is_empty() || target.is_empty() {
        return Err((400, "{\"error\":\"Malformed HTTP request.\"}".to_string()));
    }
    let mut content_length = None;
    let mut session = None;
    let mut authorization = None;
    let mut protocol_version = None;
    let mut close = false;
    let mut accept_sse = false;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            match key.as_str() {
                "content-length" => {
                    if content_length.is_some()
                        || value.is_empty()
                        || !value.bytes().all(|c| c.is_ascii_digit())
                    {
                        return Err((400, "{\"error\":\"Invalid Content-Length.\"}".into()));
                    }
                    content_length =
                        Some(value.parse::<usize>().map_err(|_| {
                            (400, "{\"error\":\"Invalid Content-Length.\"}".into())
                        })?);
                }
                "transfer-encoding" => {
                    return Err((
                        400,
                        "{\"error\":\"Transfer-Encoding is not supported; send Content-Length.\"}"
                            .into(),
                    ))
                }
                "mcp-session-id" => session = Some(value.to_string()),
                "authorization" => authorization = Some(value.to_string()),
                "mcp-protocol-version" if protocol_version.is_none() => {
                    protocol_version = Some(value.to_string());
                }
                "connection" => close = value.eq_ignore_ascii_case("close"),
                "accept" => accept_sse = value.to_ascii_lowercase().contains("text/event-stream"),
                _ => {}
            }
        }
    }
    let content_length = content_length.unwrap_or(0);
    if content_length > MAX_BODY {
        return Err((413, "{\"error\":\"Request body too large.\"}".to_string()));
    }
    let mut body: Vec<u8> = Vec::new();
    let body_start = header_end + 4;
    if body_start <= buffer.len() {
        body.extend_from_slice(&buffer[body_start..]);
    }
    while body.len() < content_length {
        let read = read_before(stream, &mut chunk, deadline)?;
        if read == 0 {
            return Err(broken(()));
        }
        body.extend_from_slice(&chunk[..read]);
    }
    *pending = body.split_off(content_length);
    let path = target.split('?').next().unwrap_or("").to_string();
    Ok(Some(Request {
        method,
        path,
        accept_sse,
        session,
        authorization,
        protocol_version,
        body,
        close,
    }))
}

fn read_before(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<usize, HttpError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err((408, "{\"error\":\"Request timed out.\"}".into()));
    }
    stream.set_read_timeout(Some(remaining)).map_err(broken)?;
    stream.read(buffer).map_err(|error| {
        if matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ) {
            (408, "{\"error\":\"Request timed out.\"}".into())
        } else {
            broken(error)
        }
    })
}

fn broken(_: impl Sized) -> HttpError {
    (400, "{\"error\":\"Connection interrupted.\"}".to_string())
}

fn find_double_crlf(buffer: &[u8]) -> Option<usize> {
    if buffer.len() < 4 {
        return None;
    }
    (0..=buffer.len() - 4).find(|index| {
        buffer[*index] == b'\r'
            && buffer[index + 1] == b'\n'
            && buffer[index + 2] == b'\r'
            && buffer[index + 3] == b'\n'
    })
}

fn bearer(authorization: &Option<String>) -> Option<String> {
    let value = authorization.as_deref()?.trim();
    let mut parts = value.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    let token = parts.next()?.trim();
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token.to_string())
    } else {
        None
    }
}

fn presented_token_matches(expected: &str, header: &Option<String>, path_token: &str) -> bool {
    // A stale Authorization header must not hide a correct secret in the URL,
    // and a correct bearer must not be rejected because the path token differs.
    let header_ok = token_matches(expected, bearer(header).as_deref());
    let path_ok = !path_token.is_empty() && token_matches(expected, Some(path_token));
    header_ok || path_ok
}

fn route(
    request: &Request,
    sessions: &Arc<Mutex<Sessions>>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let settings = Settings::load();
    let gateway = settings.gateway();
    route_with(request, sessions, &settings, &gateway.token)
}

fn route_with(
    request: &Request,
    sessions: &Arc<Mutex<Sessions>>,
    settings: &Settings,
    gateway_token: &str,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let path = request.path.as_str();
    if path == "/mcp" || path.starts_with("/mcp/") {
        let path_token = path["/mcp".len()..]
            .trim_start_matches('/')
            .trim()
            .to_string();
        if !presented_token_matches(gateway_token, &request.authorization, &path_token) {
            return unauthorized();
        }
        match request.method.as_str() {
            "OPTIONS" => cors_preflight(),
            "GET" => (
                405,
                vec![("Allow".to_string(), "POST".to_string())],
                json!({ "jsonrpc": "2.0", "error": { "code": -32601, "message": "GET not supported: this server answers with plain JSON (POST only)." } })
                    .to_string()
                    .into_bytes(),
            ),
            "DELETE" => {
                if let Some(session) = &request.session {
                    if let Ok(mut map) = sessions.lock() {
                        map.remove(session);
                    }
                }
                (204, Vec::new(), Vec::new())
            }
            "POST" => post_mcp(request, sessions, settings),
            _ => jsonrpc_error(405, "Unsupported HTTP method."),
        }
    } else if let Some(response) = crate::share::route(&request.method, path) {
        // Capability links for files returned by moodle_file. The random id in
        // the path is the secret; no Moodle token is exposed.
        response
    } else if is_oauth_discovery(path) {
        // This server authenticates with a secret link, not OAuth. Answering the
        // discovery probes with a plain 404 keeps Claude (and other MCP clients)
        // from starting a dynamic-client-registration flow that can never work.
        (
            404,
            vec![("Cache-Control".to_string(), "no-store".to_string())],
            json!({
                "error": "oauth_not_supported",
                "detail": "student-mcp uses a secret connector link (/mcp/<token>), not OAuth."
            })
            .to_string()
            .into_bytes(),
        )
    } else if path == "/" || path.is_empty() {
        match request.method.as_str() {
            // Some clients POST to the origin root after discovery. Accept it when
            // the token travels in the Authorization header, so a connector saved
            // without the /mcp/<token> suffix still has a chance to work.
            "POST" => {
                let provided = bearer(&request.authorization);
                if !token_matches(gateway_token, provided.as_deref()) {
                    // Never advertise OAuth here: a WWW-Authenticate challenge makes
                    // Claude attempt registration with a sign-in service that does
                    // not exist ("Unable to register with the sign-in service").
                    return unauthorized();
                }
                post_mcp(request, sessions, settings)
            }
            "OPTIONS" => cors_preflight(),
            _ => {
                let message = json!({
                    "app": "student-mcp",
                    "oauth": false,
                    "hint": "Point your AI assistant at /mcp/<token>; the token is in the student-mcp window.",
                    "status": "ok"
                });
                (200, Vec::new(), message.to_string().into_bytes())
            }
        }
    } else {
        jsonrpc_error(404, "Not found.")
    }
}

fn is_oauth_discovery(path: &str) -> bool {
    path.starts_with("/.well-known/oauth-")
        || path.starts_with("/.well-known/openid-configuration")
        || path == "/register"
        || path == "/authorize"
        || path == "/token"
}

/// A bare 401 without a `WWW-Authenticate` challenge. Advertising `Bearer`
/// here (even without `resource_metadata`) sends clients into OAuth discovery,
/// which then fails at dynamic client registration.
fn unauthorized() -> (u16, Vec<(String, String)>, Vec<u8>) {
    (
        401,
        vec![("Cache-Control".to_string(), "no-store".to_string())],
        json!({
            "error": "invalid_token",
            "hint": "Use the full link from the student-mcp window: /mcp/<token>."
        })
        .to_string()
        .into_bytes(),
    )
}

fn cors_preflight() -> (u16, Vec<(String, String)>, Vec<u8>) {
    (
        204,
        vec![
            ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
            (
                "Access-Control-Allow-Methods".to_string(),
                "POST, GET, DELETE, OPTIONS".to_string(),
            ),
            (
                "Access-Control-Allow-Headers".to_string(),
                "Content-Type, Authorization, Mcp-Session-Id, Accept, MCP-Protocol-Version"
                    .to_string(),
            ),
        ],
        Vec::new(),
    )
}

fn jsonrpc_error(status: u16, message: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    (
        status,
        Vec::new(),
        json!({ "jsonrpc": "2.0", "error": { "code": -32601, "message": message } })
            .to_string()
            .into_bytes(),
    )
}

fn post_mcp(
    request: &Request,
    sessions: &Arc<Mutex<Sessions>>,
    settings: &Settings,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let message: Value = match serde_json::from_slice(&request.body) {
        Ok(message) => message,
        Err(_) => {
            return (
                400,
                Vec::new(),
                json!({ "jsonrpc": "2.0", "error": { "code": -32700, "message": "Invalid JSON." } }).to_string().into_bytes(),
            );
        }
    };
    let is_notification = message.get("id").is_none() || message.get("id") == Some(&Value::Null);
    if is_notification {
        return (202, Vec::new(), Vec::new());
    }
    let body_version = message
        .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    if let (Some(header), Some(body)) = (request.protocol_version.as_deref(), body_version) {
        if header != body {
            let id = message.get("id").cloned().unwrap_or(Value::Null);
            let reply = crate::mcp::header_mismatch(
                &id,
                "MCP-Protocol-Version does not match the protocol version in the request body.",
            );
            return (
                400,
                Vec::new(),
                serde_json::to_vec(&reply).unwrap_or_default(),
            );
        }
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    let stateless = crate::mcp::allows_stateless(&message, request.protocol_version.as_deref());
    if method == "initialize" {
        let fresh = random_hex(16);
        if let Ok(mut map) = sessions.lock() {
            map.insert(fresh.clone());
        }
        headers.push(("Mcp-Session-Id".to_string(), fresh));
    } else if !stateless {
        let known = request
            .session
            .as_ref()
            .map(|session| {
                sessions
                    .lock()
                    .map(|map| map.contains(session))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !known {
            return jsonrpc_error(400, "Unknown MCP session: initialize first.");
        }
    }
    match crate::mcp::handle_message_versioned(settings, &message, request.protocol_version.as_deref())
    {
        Some(reply) => {
            let status = match reply.pointer("/error/code").and_then(Value::as_i64) {
                Some(-32020 | -32022) => 400,
                _ => 200,
            };
            let body = serde_json::to_vec(&reply).unwrap_or_default();
            let _ = request.accept_sse; // JSON direct mode: one JSON body per request
            (status, headers, body)
        }
        None => (202, Vec::new(), Vec::new()),
    }
}

fn write_response(
    stream: &TcpStream,
    status: u16,
    headers: Vec<(String, String)>,
    body: &[u8],
    keep_alive: bool,
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    for (key, value) in &headers {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    if status != 204 {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        let has = |name: &str| {
            headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name))
        };
        if !has("Content-Type") {
            head.push_str("Content-Type: application/json; charset=utf-8\r\n");
        }
        if !has("Cache-Control") {
            head.push_str("Cache-Control: no-store\r\n");
        }
        if !has("Access-Control-Allow-Origin") {
            head.push_str("Access-Control-Allow-Origin: *\r\n");
        }
    }
    head.push_str(if keep_alive {
        "Connection: keep-alive\r\n"
    } else {
        "Connection: close\r\n"
    });
    head.push_str("\r\n");
    let mut response = head.into_bytes();
    response.extend_from_slice(body);
    let mut output = stream.try_clone()?;
    output.write_all(&response)?;
    output.flush()?;
    Ok(())
}

fn respond(
    stream: &TcpStream,
    status: u16,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    let headers: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    write_response(stream, status, headers, body, false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_switch_blocks_requests_and_can_be_reenabled() {
        use super::*;
        crate::config::tests::with_temp_config(|| {
            let enabled = Arc::new(AtomicBool::new(true));
            for active in [true, false, true] {
                enabled.store(active, Ordering::SeqCst);
                let listener = bind(0).unwrap();
                let address = listener.local_addr().unwrap();
                let control = Arc::clone(&enabled);
                let worker = std::thread::spawn(move || {
                    let (stream, _) = listener.accept().unwrap();
                    connection(stream, Arc::new(Mutex::new(Sessions::default())), control).unwrap();
                });
                let mut client = TcpStream::connect(address).unwrap();
                client
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                client.write_all(b"GET /.well-known/oauth-authorization-server HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
                let mut response = String::new();
                client.read_to_string(&mut response).unwrap();
                assert!(
                    response.starts_with(if active {
                        "HTTP/1.1 404"
                    } else {
                        "HTTP/1.1 503"
                    }),
                    "{response}"
                );
                worker.join().unwrap();
            }
        });
    }

    use super::*;

    const TOKEN: &str = "test-token";

    fn socket_pair() -> (TcpStream, TcpStream) {
        let listener = bind(0).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn capability_file_urls_are_served_by_the_http_router() {
        let id = crate::share::publish(
            "notes.pdf",
            "application/pdf",
            b"%PDF-1.4 fake",
            "page one\u{0C}page two",
            2,
        )
        .unwrap();
        let (status, headers, body) = call(&request("GET", &format!("/s/{id}/text")));
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        assert!(
            headers
                .iter()
                .any(|(key, value)| key == "Content-Type" && value.starts_with("text/plain")),
            "{headers:?}"
        );
        assert_eq!(body, b"page one\x0Cpage two");
    }

    #[test]
    fn shared_file_responses_keep_their_own_content_type() {
        let (mut client, server) = socket_pair();
        write_response(
            &server,
            200,
            vec![
                ("Content-Type".to_string(), "application/pdf".to_string()),
                (
                    "Cache-Control".to_string(),
                    "private, max-age=60".to_string(),
                ),
            ],
            b"%PDF-1.4",
            false,
        )
        .unwrap();
        drop(server);
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let response = String::from_utf8_lossy(&response);
        assert!(
            response.contains("Content-Type: application/pdf\r\n"),
            "{response}"
        );
        assert!(!response.contains("application/json"), "{response}");
        assert!(!response.contains("no-store"), "{response}");
    }

    #[test]
    fn connection_capacity_is_bounded_and_released() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut slots: Vec<_> = (0..MAX_CONNECTIONS)
            .map(|_| ConnectionSlot::acquire(&active).unwrap())
            .collect();
        assert!(ConnectionSlot::acquire(&active).is_none());
        slots.pop();
        let recovered = ConnectionSlot::acquire(&active).expect("closed connection frees capacity");
        drop(slots);
        drop(recovered);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn idle_connections_and_incomplete_bodies_time_out() {
        for prefix in [
            "",
            "POST /mcp/test-token HTTP/1.1\r\nContent-Length: 10\r\n\r\n{}",
        ] {
            let (mut client, mut server) = socket_pair();
            client.write_all(prefix.as_bytes()).unwrap();
            let result = read_request(&mut server, &mut Vec::new(), Duration::from_millis(40));
            assert!(matches!(result, Err((408, _))));
        }
    }

    #[test]
    fn slow_trickle_does_not_extend_request_deadline() {
        let (mut client, mut server) = socket_pair();
        let writer = std::thread::spawn(move || {
            for _ in 0..100 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let started = Instant::now();
        let result = read_request(&mut server, &mut Vec::new(), Duration::from_millis(60));
        assert!(matches!(result, Err((408, _))));
        assert!(started.elapsed() < Duration::from_millis(800));
        drop(server);
        writer.join().unwrap();
    }

    #[test]
    fn keep_alive_preserves_pipelined_requests() {
        let (mut client, mut server) = socket_pair();
        client.write_all(b"POST /first HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}GET /second HTTP/1.1\r\nConnection: close\r\n\r\n").unwrap();
        let mut pending = Vec::new();
        let first = read_request(&mut server, &mut pending, Duration::from_secs(1))
            .unwrap()
            .unwrap();
        assert_eq!(first.path, "/first");
        assert_eq!(first.body, b"{}");
        let second = read_request(&mut server, &mut pending, Duration::from_secs(1))
            .unwrap()
            .unwrap();
        assert_eq!(second.path, "/second");
        assert!(second.close);
    }

    #[test]
    fn malformed_framing_and_oversized_requests_are_rejected() {
        for (headers, status) in [
            ("Content-Length: invalid".to_string(), 400),
            ("Content-Length: 0\r\nContent-Length: 1".to_string(), 400),
            ("Transfer-Encoding: chunked".to_string(), 400),
            (format!("Content-Length: {}", MAX_BODY + 1), 413),
            (format!("X-Fill: {}", "x".repeat(MAX_HEADER)), 431),
        ] {
            let (_client, mut server) = socket_pair();
            let mut pending =
                format!("POST /mcp/test-token HTTP/1.1\r\n{headers}\r\n\r\n").into_bytes();
            let result = read_request(&mut server, &mut pending, Duration::from_secs(1));
            assert!(matches!(result, Err((code, _)) if code == status));
        }
    }

    fn request(method: &str, path: &str) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            accept_sse: true,
            session: None,
            authorization: None,
            protocol_version: None,
            body: br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_vec(),
            close: false,
        }
    }

    fn sessions() -> Arc<Mutex<Sessions>> {
        Arc::new(Mutex::new(Sessions::default()))
    }

    fn call(request: &Request) -> (u16, Vec<(String, String)>, Vec<u8>) {
        route_with(request, &sessions(), &Settings::default(), TOKEN)
    }

    #[test]
    fn bad_token_gets_a_bare_401_without_an_oauth_challenge() {
        let (status, headers, _) = call(&request("POST", "/mcp/wrong"));
        assert_eq!(status, 401);
        assert!(
            headers
                .iter()
                .all(|(key, _)| !key.eq_ignore_ascii_case("WWW-Authenticate")),
            "an OAuth challenge would send Claude into client registration: {headers:?}"
        );
    }

    #[test]
    fn oauth_discovery_is_a_plain_404() {
        for path in [
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-authorization-server",
            "/.well-known/openid-configuration",
            "/register",
        ] {
            let (status, headers, body) = call(&request("GET", path));
            assert_eq!(status, 404, "{path}");
            assert!(headers.iter().all(|(key, _)| key != "WWW-Authenticate"));
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains("oauth_not_supported"), "{path}: {text}");
        }
    }

    #[test]
    fn secret_path_token_initializes_the_session() {
        let (status, headers, body) = call(&request("POST", "/mcp/test-token"));
        assert_eq!(status, 200);
        assert!(headers.iter().any(|(key, _)| key == "Mcp-Session-Id"));
        let reply: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(reply["jsonrpc"], "2.0");
    }

    #[test]
    fn bearer_token_is_accepted_at_the_origin_root() {
        for path in ["/", "/mcp", "/mcp/"] {
            let mut message = request("POST", path);
            message.authorization = Some(format!("Bearer {TOKEN}"));
            let (status, _, body) = call(&message);
            assert_eq!(status, 200, "{path}");
            let reply: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(reply["result"]["serverInfo"]["name"], "student-mcp");
        }
    }

    #[test]
    fn origin_root_without_a_token_is_401_and_the_get_is_a_status_page() {
        let (status, _, _) = call(&request("POST", "/"));
        assert_eq!(status, 401);
        let (status, _, body) = call(&request("GET", "/"));
        assert_eq!(status, 200);
        let status_page: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status_page["oauth"], false);
        assert_eq!(status_page["status"], "ok");
    }

    #[test]
    fn either_the_path_token_or_the_bearer_token_is_enough() {
        let mut message = request("POST", "/mcp/test-token");
        message.authorization = Some("Bearer wrong".into());
        assert_eq!(call(&message).0, 200);
        let mut message = request("POST", "/mcp/wrong");
        message.authorization = Some("bearer test-token".into());
        assert_eq!(call(&message).0, 200);
        let mut message = request("POST", "/mcp/wrong");
        message.authorization = Some("Bearer also-wrong".into());
        assert_eq!(call(&message).0, 401);
    }

    #[test]
    fn sessions_drop_the_oldest_without_clearing_the_rest() {
        let sessions = sessions();
        let settings = Settings::default();
        let mut first = None;
        let mut last = None;
        for index in 0..=MAX_SESSIONS {
            let (status, headers, _) =
                route_with(&request("POST", "/mcp/test-token"), &sessions, &settings, TOKEN);
            assert_eq!(status, 200, "{index}");
            let id = headers
                .into_iter()
                .find(|(key, _)| key == "Mcp-Session-Id")
                .unwrap()
                .1;
            if index == 0 {
                first = Some(id.clone());
            }
            last = Some(id);
        }
        let mut ping = request("POST", "/mcp/test-token");
        ping.body = br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#.to_vec();
        ping.session = first;
        assert_eq!(route_with(&ping, &sessions, &settings, TOKEN).0, 400);
        ping.session = last;
        assert_eq!(route_with(&ping, &sessions, &settings, TOKEN).0, 200);
    }

    #[test]
    fn modern_requests_do_not_need_a_session() {
        let mut message = request("POST", "/mcp/test-token");
        message.protocol_version = Some("2026-07-28".into());
        message.body = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#.to_vec();
        let (status, headers, body) = call(&message);
        assert_eq!(status, 200);
        assert!(headers.iter().all(|(key, _)| key != "Mcp-Session-Id"));
        let reply: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(reply["result"]["resultType"], "complete");
        assert!(reply["result"]["tools"].is_array());

        message.protocol_version = Some("2026-08-01".into());
        message.body = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#.to_vec();
        let (status, _, body) = call(&message);
        assert_eq!(status, 400);
        let reply: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(reply["error"]["code"], -32020);
    }

    #[test]
    fn preflight_advertises_the_mcp_headers() {
        let (status, headers, _) = call(&request("OPTIONS", "/mcp/test-token"));
        assert_eq!(status, 204);
        assert!(headers
            .iter()
            .any(|(key, value)| key == "Access-Control-Allow-Origin" && value == "*"));
        assert!(headers
            .iter()
            .any(|(key, value)| key == "Access-Control-Allow-Headers"
                && value.contains("Mcp-Session-Id")));
    }
}
