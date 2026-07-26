//! MCP server over stdio (S101 P3).
//!
//! Exposes [`crate::agent::tools::ToolRegistry`] to any MCP client — Claude
//! Code, Claude Desktop, a script — so the same tools the in-editor agent loop
//! uses can drive the content tree from outside. One registry, two frontends;
//! defining the surface twice is how they drift.
//!
//! Hand-rolled JSON-RPC 2.0 rather than an MCP SDK: the surface needed here is
//! `initialize`, `notifications/initialized`, `tools/list`, and `tools/call`,
//! `serde_json` is already a dependency, and the alternative is a large async
//! dependency tree in a GUI crate for four methods.
//!
//! **Headless.** This mode never starts egui, so there are no live tabs —
//! [`Tool::needs_session`](crate::agent::tools::Tool::needs_session) tools are
//! not advertised. Serving the full surface from a running editor is P7.
//!
//! Framing is newline-delimited JSON: one request object per line, one
//! response object per line. Notifications (no `id`) get no response at all —
//! answering one is a protocol violation that some clients treat as fatal.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::agent::mode::Mode;
use crate::agent::tools::{ToolCtx, ToolRegistry};

/// Fallback protocol version when the client does not name one.
///
/// The handshake echoes the client's `protocolVersion` when it sends one: the
/// server's job is to agree on a version the client already speaks, and
/// answering a client's `2025-06-18` with a hardcoded older string makes
/// conforming clients disconnect.
const FALLBACK_PROTOCOL_VERSION: &str = "2024-11-05";

const SERVER_NAME: &str = "reachlock-content";

/// JSON-RPC error codes used here (spec-defined).
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Run the stdio server until EOF. Returns the process exit code.
pub fn serve_stdio() -> i32 {
    let registry = ToolRegistry::new();
    // Headless: no tabs, so session tools are not advertised.
    let ctx = ToolCtx::headless();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            // The client closed the pipe. Normal shutdown, not a failure.
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some(response) = handle_line(&registry, &line, &ctx) else {
            // A notification, or something unparseable with no id to answer
            // against. Either way there is nothing to send.
            continue;
        };
        if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
    0
}

/// Handle one line of stdio framing. `None` means "send nothing".
fn handle_line(registry: &ToolRegistry, line: &str, ctx: &ToolCtx) -> Option<String> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            // No id is recoverable from unparseable input, and the spec's
            // null-id parse error is more likely to confuse a client than
            // help. Log to stderr — stdout is the protocol channel and any
            // stray byte on it corrupts the stream.
            eprintln!("mcp: ignoring unparseable line: {e}");
            return None;
        }
    };

    handle_request(registry, &request, ctx).map(|v| v.to_string())
}

/// Handle one JSON-RPC request object. `None` means "send nothing" — a
/// notification. Transport-independent: stdio and HTTP both go through here,
/// so the two frontends cannot answer the same method differently.
pub fn handle_request(registry: &ToolRegistry, request: &Value, ctx: &ToolCtx) -> Option<Value> {
    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

    // A request without an `id` is a notification: handle it, answer nothing.
    let id = request.get("id").cloned()?;

    let result = match method {
        "initialize" => Ok(initialize_result(request)),
        "tools/list" => Ok(tools_list_result(registry, ctx)),
        "tools/call" => tools_call_result(registry, request, ctx),
        other => Err((
            METHOD_NOT_FOUND,
            format!("method `{other}` is not supported by this server"),
        )),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

fn initialize_result(request: &Value) -> Value {
    let version = request
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or(FALLBACK_PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn tools_list_result(registry: &ToolRegistry, ctx: &ToolCtx) -> Value {
    // Build mode: an external MCP client brings its own approval flow, and
    // the Plan/Build toggle is an in-editor affordance. Session tools are
    // advertised only when there is a session to run them — advertising and
    // then failing at call time wastes a turn and reads as a broken server.
    let tools: Vec<Value> = registry
        .available(Mode::Build, ctx.has_session())
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": (t.input_schema)(),
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tools_call_result(
    registry: &ToolRegistry,
    request: &Value,
    ctx: &ToolCtx,
) -> Result<Value, (i64, String)> {
    let params = request
        .get("params")
        .ok_or((INVALID_PARAMS, "tools/call requires params".to_string()))?;
    let name = params.get("name").and_then(|n| n.as_str()).ok_or((
        INVALID_PARAMS,
        "tools/call requires a tool name".to_string(),
    ))?;

    if registry.get(name).is_some_and(|t| t.needs_session) && !ctx.has_session() {
        return Err((
            INVALID_PARAMS,
            format!(
                "`{name}` needs a live editor session; this is the headless server. \
                 Run the editor and use its MCP endpoint instead."
            ),
        ));
    }

    // Absent arguments mean "no arguments" — zero-arg tools like check_tree
    // are commonly called with the field omitted entirely.
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome = registry.dispatch(name, &args, Mode::Build, ctx);

    // A tool failure is a successful JSON-RPC call carrying `isError`, not a
    // JSON-RPC error. The distinction matters: a protocol error is the
    // client's problem, a tool error is the model's to read and act on.
    Ok(json!({
        "content": [{ "type": "text", "text": outcome.content }],
        "isError": outcome.is_error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(line: &str) -> Option<Value> {
        let reg = ToolRegistry::new();
        handle_line(&reg, line, &ToolCtx::headless())
            .map(|s| serde_json::from_str(&s).expect("response is JSON"))
    }

    #[test]
    fn initialize_echoes_the_clients_protocol_version() {
        let resp = call(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .expect("initialize is answered");
        assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(resp["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_without_a_version_falls_back() {
        let resp = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], FALLBACK_PROTOCOL_VERSION);
    }

    /// Answering a notification is a protocol violation; some clients treat
    /// the unexpected response as fatal.
    #[test]
    fn notifications_get_no_response() {
        assert!(call(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn tools_list_advertises_the_content_tools_with_schemas() {
        let resp = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"query_content"), "got {names:?}");
        assert!(names.contains(&"check_tree"), "got {names:?}");
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "bad schema on {t}");
            // MCP spells it `inputSchema`; the provider trait spells the same
            // thing `input_schema`. Getting this wrong makes every tool
            // unusable with no error from the transport.
            assert!(t.get("input_schema").is_none());
        }
    }

    #[test]
    fn an_unknown_method_is_a_jsonrpc_error() {
        let resp = call(r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#).unwrap();
        assert_eq!(resp["error"]["code"], METHOD_NOT_FOUND);
    }

    /// A tool that fails must still be a successful call carrying `isError` —
    /// a JSON-RPC error means the *client* got the protocol wrong.
    #[test]
    fn a_failing_tool_is_a_result_with_is_error() {
        let resp = call(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_content","arguments":{"kind":"nonsense"}}}"#,
        )
        .unwrap();
        assert!(resp.get("error").is_none(), "{resp}");
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["content"][0]["type"], "text");
    }

    #[test]
    fn tools_call_without_arguments_is_allowed() {
        let resp = call(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"check_tree"}}"#,
        )
        .unwrap();
        assert!(resp.get("error").is_none(), "{resp}");
        assert_eq!(resp["result"]["isError"], false);
    }

    #[test]
    fn unparseable_input_does_not_answer_or_panic() {
        assert!(call("{ this is not json").is_none());
    }
}
