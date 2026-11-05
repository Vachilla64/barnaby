//! Minimal MCP server (Streamable HTTP, JSON-RPC 2.0): initialize, tools/list, tools/call.

use crate::agents::state_for_phase;
use crate::server::{emit_agent_state, speak, ServerState};
use crate::voice::queue::{Priority, SpeechKind};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-06-18";

fn tools() -> Value {
    json!([
        {
            "name": "barnaby_status",
            "description": "Call when you start a task, at each milestone, and when you are blocked. Keep the summary short and spoken-style (25 words or fewer). Do not call it more than once a minute.",
            "inputSchema": {"type": "object", "properties": {
                "summary": {"type": "string", "description": "What you are doing, in plain spoken words."},
                "phase": {"type": "string", "enum": ["planning", "working", "testing", "blocked", "done"]},
                "project_root": {"type": "string"}
            }, "required": ["summary", "phase"]}
        },
        {
            "name": "barnaby_say",
            "description": "Say something short to the user out loud (60 words or fewer). Set urgent only when the user must act now.",
            "inputSchema": {"type": "object", "properties": {
                "text": {"type": "string"}, "urgent": {"type": "boolean"}
            }, "required": ["text"]}
        },
        {
            "name": "barnaby_done",
            "description": "Call when the task is finished: say what changed in plain words (40 words or fewer).",
            "inputSchema": {"type": "object", "properties": {
                "summary": {"type": "string"},
                "files_changed": {"type": "array", "items": {"type": "string"}}
            }, "required": ["summary"]}
        },
        {
            "name": "barnaby_ask_user",
            "description": "Ask the user a quick question out loud instead of stopping. Offer at most 4 short options.",
            "inputSchema": {"type": "object", "properties": {
                "question": {"type": "string"},
                "options": {"type": "array", "items": {"type": "string"}, "maxItems": 4}
            }, "required": ["question"]}
        },
        {
            "name": "barnaby_tour",
            "description": "Walk the user through code you wrote: 3 to 5 steps (6 at most), one idea each, in plain everyday English. Each step has a file path, a line range, a short title (2 to 5 words) and one or two short spoken sentences. The user moves from step to step at their own pace. End with a recap: the one thing to remember.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string"},
                "steps": {"type": "array", "maxItems": 6, "items": {"type": "object", "properties": {
                    "path": {"type": "string"}, "start_line": {"type": "integer"},
                    "end_line": {"type": "integer"}, "title": {"type": "string"}, "narration": {"type": "string"}
                }, "required": ["path", "start_line", "end_line", "narration"]}},
                "recap": {"type": "string"}
            }, "required": ["title", "steps"]}
        }
    ])
}

fn str_arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Kind of agent from the MCP `clientInfo.name`, which the client sends on `initialize`.
/// Tool calls carry no client info, so the kind comes from the `?kind=` query on the URL,
/// defaulting to Claude Code.
async fn call_tool(state: &ServerState, kind: &str, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "barnaby_status" => {
            let summary = str_arg(args, "summary");
            let phase = str_arg(args, "phase");
            let s = state_for_phase(phase);
            state.registry.touch(kind, Some(s), Some(summary));
            emit_agent_state(state, kind, s, Some(summary));
            speak(state, kind, summary, Priority::Normal, SpeechKind::Status).await;
            Ok("ok".into())
        }
        "barnaby_say" => {
            let urgent = args.get("urgent").and_then(Value::as_bool).unwrap_or(false);
            let text = str_arg(args, "text");
            state.registry.touch(kind, None, Some(text));
            let prio = if urgent { Priority::High } else { Priority::Normal };
            speak(state, kind, text, prio, SpeechKind::Status).await;
            Ok("ok".into())
        }
        "barnaby_done" => {
            let summary = str_arg(args, "summary");
            state.registry.mark_done(kind);
            state.registry.touch(kind, None, Some(summary));
            emit_agent_state(state, kind, "happy", Some(summary));
            speak(state, kind, summary, Priority::High, SpeechKind::Status).await;
            Ok("ok".into())
        }
        "barnaby_ask_user" => {
            // Bubble buttons and spoken answers arrive with P4/P6; for now the question is spoken
            // and the agent is told to carry on or ask in the terminal.
            let q = str_arg(args, "question");
            state.registry.touch(kind, Some("alert"), Some(q));
            emit_agent_state(state, kind, "alert", Some(q));
            speak(state, kind, q, Priority::High, SpeechKind::Alert).await;
            Ok("no answer".into())
        }
        "barnaby_tour" => {
            let title = str_arg(args, "title");
            let steps: Vec<crate::tours::TourStep> = args
                .get("steps")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            state.registry.touch(kind, None, Some(&format!("tour: {title}")));
            let tour = crate::tours::Tour { title: title.to_string(), steps, recap: str_arg(args, "recap").to_string() };
            let n = state.tours.start(kind, tour, crate::server::pet_colour(kind)).await;
            if n == 0 {
                Err("No tour steps pointed at files I can find. Use absolute paths or paths inside an open workspace.".into())
            } else {
                Ok("started".into())
            }
        }
        other => Err(format!("Unknown tool: {other}")),
    }
}

/// Handles one JSON-RPC message. Returns `None` for notifications, which get no body.
pub async fn handle(state: &ServerState, kind: &str, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let ok = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result});
    let err = |code: i64, message: String| {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
    };

    if id.is_none() {
        return None; // notifications/initialized and friends
    }

    Some(match method {
        "initialize" => {
            let client = msg
                .pointer("/params/clientInfo/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            tracing::info!("MCP initialize from client {client}");
            state.registry.touch(kind, Some("idle"), None);
            ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "barnaby", "version": env!("CARGO_PKG_VERSION")}
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("");
            let args = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
            match call_tool(state, kind, name, &args).await {
                Ok(text) => ok(json!({"content": [{"type": "text", "text": text}], "isError": false})),
                Err(e) => ok(json!({"content": [{"type": "text", "text": e}], "isError": true})),
            }
        }
        other => err(-32601, format!("Method not found: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::test_state;

    #[tokio::test]
    async fn initialize_and_list_tools() {
        let st = test_state();
        let r = handle(&st, "claude-code", &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"claude-code"}}}))
            .await
            .unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
        let r = handle(&st, "claude-code", &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await.unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        let st = test_state();
        assert!(handle(&st, "claude-code", &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await.is_none());
    }

    #[tokio::test]
    async fn status_call_updates_registry() {
        let st = test_state();
        let r = handle(&st, "claude-code", &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"barnaby_status","arguments":{"summary":"Reading the login code.","phase":"working"}}}))
            .await
            .unwrap();
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(st.registry.list()[0].feed[0], "Reading the login code.");
    }

    #[tokio::test]
    async fn unknown_tool_and_method_are_errors() {
        let st = test_state();
        let r = handle(&st, "x", &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}})).await.unwrap();
        assert_eq!(r["result"]["isError"], true);
        let r = handle(&st, "x", &json!({"jsonrpc":"2.0","id":5,"method":"bogus"})).await.unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }
}
