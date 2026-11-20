//! MyMaxmakeLab MCP server: lets AI assistants (Claude, ChatGPT/Codex,
//! Gemini, Mistral, or any model behind an MCP client) generate designs
//! and G-code with the same code as the application.
//!
//! Transport: JSON-RPC 2.0 over stdio, one message per line, as the
//! Model Context Protocol specifies for local servers.
//!
//! It never talks to a machine: programs are written to files that the
//! operator opens in MyMaxmakeLab, previews, frames and starts with all
//! the safety checks.

mod tools;

use serde_json::{Value, json};
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2025-06-18";

fn response(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Answer to one request, `None` for notifications.
pub fn handle(message: &Value) -> Option<Value> {
    let id = message.get("id")?;
    let method = message["method"].as_str().unwrap_or_default();
    Some(match method {
        "initialize" => {
            let requested = message["params"]["protocolVersion"]
                .as_str()
                .unwrap_or(PROTOCOL_VERSION);
            response(
                id,
                json!({
                    "protocolVersion": requested,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "mymaxmakelab", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Laser and CNC design tools. Generated G-code is written to files \
                        for the operator to open in MyMaxmakeLab; this server never moves a machine."
                }),
            )
        }
        "ping" => response(id, json!({})),
        "tools/list" => response(id, json!({"tools": tools::definitions()})),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            let arguments = message["params"]
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));
            match tools::call(name, &arguments) {
                Ok(text) => response(
                    id,
                    json!({"content": [{"type": "text", "text": text}], "isError": false}),
                ),
                // Tool failures are results the model can read and fix.
                Err(problem) => response(
                    id,
                    json!({"content": [{"type": "text", "text": problem}], "isError": true}),
                ),
            }
        }
        _ => error(id, -32601, "method not found"),
    })
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&message),
            Err(_) => Some(error(&Value::Null, -32700, "parse error")),
        };
        if let Some(reply) = reply {
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_lists_tools_and_calls_one() {
        let init = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "mymaxmakelab");
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"generate_box") && names.contains(&"svg_to_gcode"));
        let call = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"generate_maze","arguments":{"columns":5,"rows":4}}}))
        .unwrap();
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("<svg"), "{text}");
        let unknown =
            handle(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}}))
                .unwrap();
        assert_eq!(unknown["result"]["isError"], true);
        let missing = handle(&json!({"jsonrpc":"2.0","id":5,"method":"resources/list"})).unwrap();
        assert_eq!(missing["error"]["code"], -32601);
    }
}
