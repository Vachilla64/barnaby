//! Look mode: the user clicks Barnaby's monocle, then selects code in VS Code, and he looks at it.
//!
//! The page draws a monocle ring on the cursor while this is on (the cursor watcher in
//! `overlay/clickthrough.rs` sends the cursor position). When the selection in VS Code has stopped
//! changing for a moment, look mode ends and the page is told what was selected.

use crate::server::ServerState;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;

/// The selection must stay put this long before he reacts, so a selection still being dragged
/// does not set him off early. (The VS Code extension already waits 300 ms.)
const SETTLE: Duration = Duration::from_millis(500);
/// Look mode switches itself off after this long without a selection.
const TIMEOUT: Duration = Duration::from_secs(90);

static ON: AtomicBool = AtomicBool::new(false);
/// Changes whenever look mode is switched, so an old timeout cannot end a newer look.
static MODE_EPOCH: AtomicU64 = AtomicU64::new(0);
/// Changes whenever VS Code reports a new selection, so only the last one is acted on.
static SELECTION_EPOCH: AtomicU64 = AtomicU64::new(0);

pub fn is_on() -> bool {
    ON.load(Ordering::SeqCst)
}

fn emit(state: &ServerState, event: &str, payload: Value) {
    if let Some(app) = &state.app_handle {
        let _ = app.emit(event, payload);
    }
}

/// Switch look mode on or off and tell the page. `reason` says why, for the page's wording.
pub fn set(state: &Arc<ServerState>, on: bool, reason: &str) {
    if ON.swap(on, Ordering::SeqCst) == on {
        return;
    }
    let epoch = MODE_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    emit(state, "look:state", json!({"on": on, "reason": reason}));
    if on {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(TIMEOUT).await;
            if is_on() && MODE_EPOCH.load(Ordering::SeqCst) == epoch {
                set(&state, false, "timeout");
            }
        });
    }
}

/// What VS Code has selected, from an `editor.context` message. None when nothing is selected.
#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    pub path: String,
    pub name: String,
    pub start_line: u64,
    pub end_line: u64,
}

pub fn parse_selection(msg: &Value) -> Option<Selection> {
    let text = msg.get("selection_text").and_then(Value::as_str)?;
    if text.trim().is_empty() {
        return None;
    }
    let path = msg.get("path").and_then(Value::as_str).filter(|p| !p.is_empty())?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_string();
    let line = |k: &str| msg.get(k).and_then(Value::as_u64);
    let start_line = line("start_line").unwrap_or(1);
    Some(Selection {
        path: path.to_string(),
        name,
        start_line,
        end_line: line("end_line").unwrap_or(start_line).max(start_line),
    })
}

/// Called for every `editor.context` message from VS Code.
pub fn on_context(state: &Arc<ServerState>, msg: &Value) {
    if !is_on() {
        return;
    }
    let seen = SELECTION_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(selection) = parse_selection(msg) else {
        return;
    };
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        if !is_on() || SELECTION_EPOCH.load(Ordering::SeqCst) != seen {
            return;
        }
        // One look per click on the monocle.
        set(&state, false, "selected");
        let after = crate::config::load_or_create_config().map(|c| c.look.after_select).unwrap_or(crate::config::AfterSelect::Choices);
        tracing::info!("Look mode: {} lines {}-{}", selection.name, selection.start_line, selection.end_line);
        emit(
            &state,
            "look:selected",
            json!({
                "path": selection.path, "name": selection.name,
                "start_line": selection.start_line, "end_line": selection.end_line,
                "after": after,
            }),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_needs_text_and_a_path() {
        let msg = json!({"path": "C:\\p\\src\\validate.ts", "selection_text": "if (x) {}", "start_line": 12, "end_line": 14});
        assert_eq!(
            parse_selection(&msg),
            Some(Selection { path: "C:\\p\\src\\validate.ts".into(), name: "validate.ts".into(), start_line: 12, end_line: 14 })
        );
        assert_eq!(parse_selection(&json!({"path": "a.ts", "selection_text": "  \n"})), None);
        assert_eq!(parse_selection(&json!({"selection_text": "x"})), None);
    }

    #[test]
    fn a_backwards_range_is_fixed() {
        let msg = json!({"path": "/p/a.ts", "selection_text": "x", "start_line": 9, "end_line": 3});
        assert_eq!(parse_selection(&msg).unwrap().end_line, 9);
    }
}
