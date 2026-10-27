use crate::config::Settings;
use crate::mcp::handle_message;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// One stdio frame. Invalid JSON gets a parse error so the client is not left waiting.
pub fn response_for_line(line: &str, settings: &Settings) -> Option<Value> {
    if line.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(message) => handle_message(settings, &message),
        Err(_) => Some(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32700, "message": "Parse error" }
        })),
    }
}

/// MCP over stdio for local AI clients: one JSON-RPC message per line.
pub fn run() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let Ok(line_text) = line else { break };
        // Each call re-reads the settings so changes from the popup apply immediately.
        let settings = Settings::load();
        if let Some(reply) = response_for_line(&line_text, &settings) {
            let Ok(text) = serde_json::to_string(&reply) else {
                break;
            };
            if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                break; // client is gone: shut down with it
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_json_is_a_parse_error_and_blank_lines_are_ignored() {
        let settings = Settings::default();
        assert!(response_for_line("   ", &settings).is_none());
        let reply = response_for_line("{", &settings).unwrap();
        assert_eq!(reply["error"]["code"], -32700);
        let reply =
            response_for_line(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, &settings).unwrap();
        assert_eq!(reply["result"], serde_json::json!({}));
    }
}
