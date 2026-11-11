//! Minimal MCP server over stdio.
//!
//! Newline-delimited JSON-RPC, four tools, no dependencies. Tool failures come
//! back as `isError` content rather than JSON-RPC errors: the model is meant to
//! read "stale ref" and call `browser_snapshot`, not to treat it as a crash.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use crate::browser::Browser;
use crate::engine::Engine;
use crate::types::{ActRequest, SnapshotRequest};

pub fn serve<E: Engine>(browser: &mut Browser<E>) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let message: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                write_message(&mut stdout, &error(None, -32700, "parse error"))?;
                continue;
            }
        };

        let id = message.get("id").cloned();
        // Notifications (no id) expect no reply, including initialized.
        let Some(id) = id else { continue };

        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let response = match method {
            "initialize" => ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "halo-browser",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools() })),
            "tools/call" => call_tool(browser, id, &params),
            other => error(Some(id), -32601, &format!("unknown method: {other}")),
        };

        write_message(&mut stdout, &response)?;
    }

    Ok(())
}

fn call_tool<E: Engine>(browser: &mut Browser<E>, id: Value, params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let outcome = match name {
        "browser_open" => open(browser, &args),
        "browser_snapshot" => snapshot(browser, &args),
        "browser_act" => act(browser, &args),
        "browser_read" => read(browser, &args),
        other => Err(format!("unknown tool: {other}")),
    };

    match outcome {
        Ok(text) => ok(
            id,
            json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
        ),
        Err(message) => ok(
            id,
            json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
        ),
    }
}

fn open<E: Engine>(browser: &mut Browser<E>, args: &Value) -> Result<String, String> {
    let url = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or("browser_open needs a url")?;
    let outcome = browser.open(url)?;
    Ok(format!(
        "opened {}\ntitle: {}\ngeneration: {}",
        outcome.url, outcome.title, outcome.generation
    ))
}

fn snapshot<E: Engine>(browser: &mut Browser<E>, args: &Value) -> Result<String, String> {
    let request: SnapshotRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("bad browser_snapshot arguments: {error}"))?;
    let snapshot = browser.snapshot(&request)?;

    let mut out = format!("generation: {}\n{}", snapshot.generation, snapshot.text);
    if let Some(truncation) = snapshot.truncated {
        out.push_str(&format!("\n[{}]", truncation.hint));
    }
    Ok(out)
}

fn act<E: Engine>(browser: &mut Browser<E>, args: &Value) -> Result<String, String> {
    let request: ActRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("bad browser_act arguments: {error}"))?;
    let outcome = browser.act(&request)?;
    Ok(format!(
        "{} -> {}\nurl: {}\ntitle: {}\ngeneration: {}\nevidence: {}\nnote: {}",
        request.action.as_str(),
        if outcome.changed { "page changed" } else { "page unchanged" },
        outcome.url,
        outcome.title,
        outcome.generation,
        outcome.evidence,
        outcome.note
    ))
}

fn read<E: Engine>(browser: &mut Browser<E>, args: &Value) -> Result<String, String> {
    let selector = args.get("selector").and_then(Value::as_str);
    let max_chars = args
        .get("max_chars")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    browser.read(selector, max_chars)
}

fn tools() -> Value {
    json!([
        {
            "name": "browser_open",
            "description": "Navigate the browser to a URL. Returns the page title and the snapshot generation to use for the next action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL or bare host, e.g. example.com" }
                },
                "required": ["url"]
            }
        },
        {
            "name": "browser_snapshot",
            "description": "Accessibility tree of the page with element refs (@eN). Call this before acting, and again whenever an action reports the page changed. Output is truncated to max_chars, so use scope to narrow it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "description": "CSS selector to scope the tree, e.g. #main or form" },
                    "interactive": { "type": "boolean", "description": "Only interactive elements. Defaults to true." },
                    "max_chars": { "type": "integer", "description": "Output budget in characters (default 4000, hard cap 12000)" }
                }
            }
        },
        {
            "name": "browser_act",
            "description": "Click, fill, or press a key. Requires the generation from the most recent snapshot; a ref from an older generation is rejected rather than guessed. Reports whether the page actually changed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "description": "Element ref from the snapshot, e.g. @e12" },
                    "generation": { "type": "integer", "description": "Generation returned by the last snapshot or action" },
                    "action": { "type": "string", "enum": ["click", "fill", "press"] },
                    "value": { "type": "string", "description": "Text for fill, key name for press" }
                },
                "required": ["action", "generation"]
            }
        },
        {
            "name": "browser_read",
            "description": "Visible text of an element, or of the page body when no selector is given. Cheaper than a snapshot when you only need content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector; omit for the whole body" },
                    "max_chars": { "type": "integer" }
                }
            }
        }
    ])
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

fn write_message(stdout: &mut impl Write, message: &Value) -> io::Result<()> {
    writeln!(stdout, "{message}")?;
    stdout.flush()
}
