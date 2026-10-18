use crate::config::Settings;
use crate::mcp::handle_message;
use serde_json::Value;
use std::io::{BufRead, Write};

/// MCP over stdio for local AI clients: one JSON-RPC message per line.
pub fn run() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let Ok(line_text) = line else { break };
        if line_text.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line_text) {
            Ok(message) => message,
            Err(_) => continue, // a corrupted frame cannot be answered: ignore to stay DAG-safe
        };
        // Each call re-reads the settings so changes from the popup apply immediately.
        let settings = Settings::load();
        if let Some(reply) = handle_message(&settings, &message) {
            let Ok(text) = serde_json::to_string(&reply) else { break };
            if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                break; // client is gone: shut down with it
            }
        }
    }
}
