use crate::config::{random_hex, token_matches, Settings};
use crate::mcp::handle_message;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const MAX_HEADER: usize = 32 * 1024;
const MAX_BODY: usize = 2 * 1024 * 1024;

/// Bind the local MCP endpoint (funnel proxies here).
pub fn bind(port: u16) -> Result<TcpListener, std::io::Error> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Accept loop: one thread per connection, keep-alive honoured.
pub fn serve(listener: TcpListener) -> Result<(), String> {
    let sessions: Arc<Mutex<HashMap<String, ()>>> = Arc::new(Mutex::new(HashMap::new()));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let sessions = Arc::clone(&sessions);
                std::thread::spawn(move || {
                    let _ = connection(stream, sessions);
                });
            }
            Err(_) => continue, // transient accept failure: keep serving
        }
    }
    Err("Serveur local arrêté.".to_string())
}

struct Request {
    method: String,
    path: String,
    accept_sse: bool,
    session: Option<String>,
    authorization: Option<String>,
    body: Vec<u8>,
    close: bool,
}

fn connection(
    mut stream: TcpStream,
    sessions: Arc<Mutex<HashMap<String, ()>>>,
) -> std::io::Result<()> {
    loop {
        let request = match read_request(&mut stream) {
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
        let (status, headers, body) = route(&request, &sessions);
        let terminate = request.close || status == 204;
        write_response(&stream, status, headers, &body, !terminate)?;
        if terminate {
            return Ok(());
        }
    }
}

type HttpError = (u16, String);

fn read_request(stream: &mut TcpStream) -> Result<Option<Request>, HttpError> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = find_double_crlf(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HEADER {
            return Err((431, "{\"error\":\"En-têtes trop gros.\"}".to_string()));
        }
        let read = stream.read(&mut chunk).map_err(broken)?;
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
        return Err((400, "{\"error\":\"Requête HTTP mal formée.\"}".to_string()));
    }
    let mut content_length = 0usize;
    let mut session = None;
    let mut authorization = None;
    let mut close = false;
    let mut accept_sse = false;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            match key.as_str() {
                "content-length" => content_length = value.parse().unwrap_or(0),
                "mcp-session-id" => session = Some(value.to_string()),
                "authorization" => authorization = Some(value.to_string()),
                "connection" => close = value.eq_ignore_ascii_case("close"),
                "accept" => accept_sse = value.to_ascii_lowercase().contains("text/event-stream"),
                _ => {}
            }
        }
    }
    if content_length > MAX_BODY {
        return Err((
            413,
            "{\"error\":\"Corps de requête trop gros.\"}".to_string(),
        ));
    }
    let mut body: Vec<u8> = Vec::new();
    let body_start = header_end + 4;
    if body_start <= buffer.len() {
        body.extend_from_slice(&buffer[body_start..]);
    }
    while body.len() < content_length {
        let read = stream.read(&mut chunk).map_err(broken)?;
        if read == 0 {
            return Err(broken(()));
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    let path = target.split('?').next().unwrap_or("").to_string();
    Ok(Some(Request {
        method,
        path,
        accept_sse,
        session,
        authorization,
        body,
        close,
    }))
}

fn broken(_: impl Sized) -> HttpError {
    (400, "{\"error\":\"Connexion interrompue.\"}".to_string())
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
    let value = authorization.as_deref()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .map(str::to_string)
}

fn route(
    request: &Request,
    sessions: &Arc<Mutex<HashMap<String, ()>>>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    if request.path == "/mcp" || request.path.starts_with("/mcp/") {
        let settings = Settings::load();
        let gateway_token = settings.gateway().token;
        let path_token = request.path["/mcp".len()..]
            .trim_start_matches('/')
            .trim()
            .to_string();
        let provided = bearer(&request.authorization).or(if path_token.is_empty() {
            None
        } else {
            Some(path_token)
        });
        if !token_matches(&gateway_token, provided.as_deref()) {
            return (
                401,
                vec![("WWW-Authenticate".to_string(), "Bearer".to_string())],
                json!({ "error": "Token manquant ou invalide." })
                    .to_string()
                    .into_bytes(),
            );
        }
        match request.method.as_str() {
            "OPTIONS" => (204, vec![], Vec::new()),
            "GET" => (
                405,
                vec![("Allow".to_string(), "POST".to_string())],
                json!({ "jsonrpc": "2.0", "error": { "code": -32601, "message": "GET non supporté : ce serveur répond en JSON direct (POST seulement)." } })
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
            "POST" => post_mcp(request, sessions),
            _ => jsonrpc_error(405, "Méthode HTTP non supportée."),
        }
    } else if request.path == "/" || request.path.is_empty() {
        let message = json!({
            "app": "student-mcp",
            "hint": "Pointe ton assistant IA vers /mcp/<token> ; le token est dans la fenêtre student-mcp."
        });
        (200, Vec::new(), message.to_string().into_bytes())
    } else {
        jsonrpc_error(404, "Page introuvable.")
    }
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
    sessions: &Arc<Mutex<HashMap<String, ()>>>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let message: Value = match serde_json::from_slice(&request.body) {
        Ok(message) => message,
        Err(_) => {
            return (
                400,
                Vec::new(),
                json!({ "jsonrpc": "2.0", "error": { "code": -32700, "message": "JSON invalide." } }).to_string().into_bytes(),
            );
        }
    };
    let is_notification = message.get("id").is_none() || message.get("id") == Some(&Value::Null);
    if is_notification {
        return (202, Vec::new(), Vec::new());
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    if method == "initialize" {
        let fresh = random_hex(16);
        if let Ok(mut map) = sessions.lock() {
            if map.len() > 256 {
                map.clear(); // generous eviction; every connector re-initializes anyway
            }
            map.insert(fresh.clone(), ());
        }
        headers.push(("Mcp-Session-Id".to_string(), fresh));
    } else {
        let known = request
            .session
            .as_ref()
            .map(|session| {
                sessions
                    .lock()
                    .map(|map| map.contains_key(session))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !known {
            return jsonrpc_error(400, "Session MCP inconnue : réinitialise d'abord.");
        }
    }
    let settings = Settings::load();
    match handle_message(&settings, &message) {
        Some(reply) => {
            let body = serde_json::to_vec(&reply).unwrap_or_default();
            let _ = request.accept_sse; // JSON direct mode: one JSON body per request
            (200, headers, body)
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
        head.push_str("Content-Type: application/json; charset=utf-8\r\n");
        head.push_str("Cache-Control: no-store\r\n");
        head.push_str("Access-Control-Allow-Origin: *\r\n");
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
