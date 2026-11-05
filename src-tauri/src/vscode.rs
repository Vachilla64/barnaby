//! WebSocket link to the VS Code extension (`/ws`). See SPEC 5.9.

use crate::server::ServerState;
use crate::tours::Control;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, Notify};

pub struct VsCodeHub {
    tx: broadcast::Sender<String>,
    clients: AtomicUsize,
    shown: Notify,
    folders: Mutex<Vec<String>>,
    context: Mutex<Option<Value>>,
}

impl Default for VsCodeHub {
    fn default() -> Self {
        Self::new()
    }
}

impl VsCodeHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            clients: AtomicUsize::new(0),
            shown: Notify::new(),
            folders: Mutex::new(vec![]),
            context: Mutex::new(None),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.clients.load(Ordering::SeqCst) > 0
    }

    pub fn send(&self, msg: Value) {
        let _ = self.tx.send(msg.to_string());
    }

    pub async fn wait_shown(&self) {
        self.shown.notified().await;
    }

    pub fn workspace_folders(&self) -> Vec<String> {
        self.folders.lock().unwrap().clone()
    }

    pub fn editor_context(&self) -> Option<Value> {
        self.context.lock().unwrap().clone()
    }

    /// Store what a VS Code message tells us. Returns a follow-up for the caller to act on.
    pub fn handle_message(&self, msg: &Value) -> Inbound {
        match msg.get("type").and_then(Value::as_str).unwrap_or("") {
            "hello" => {
                let folders: Vec<String> = msg
                    .get("workspace_folders")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                tracing::info!("VS Code hello: {} workspace folders", folders.len());
                *self.folders.lock().unwrap() = folders;
                Inbound::None
            }
            "editor.context" => {
                *self.context.lock().unwrap() = Some(msg.clone());
                Inbound::None
            }
            "tour.step.shown" => {
                self.shown.notify_one();
                Inbound::None
            }
            "tour.control" => msg
                .get("action")
                .and_then(Value::as_str)
                .and_then(Control::parse)
                .map(Inbound::Control)
                .unwrap_or(Inbound::None),
            "ask" => Inbound::Ask(msg.get("question").and_then(Value::as_str).unwrap_or("").to_string()),
            _ => Inbound::None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Inbound {
    None,
    Control(Control),
    Ask(String),
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

pub async fn ws_handler(
    State(state): State<Arc<ServerState>>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if q.token.as_deref() != Some(state.token.as_str()) {
        tracing::warn!("WebSocket rejected: bad token");
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| client_loop(state, socket))
}

async fn client_loop(state: Arc<ServerState>, mut socket: WebSocket) {
    let hub = state.hub.clone();
    hub.clients.fetch_add(1, Ordering::SeqCst);
    tracing::info!("VS Code connected");
    let mut rx = hub.tx.subscribe();

    loop {
        tokio::select! {
            outgoing = rx.recv() => {
                match outgoing {
                    Ok(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(msg) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                        let inbound = hub.handle_message(&msg);
                        if msg.get("type").and_then(Value::as_str) == Some("editor.context") {
                            crate::look::on_context(&state, &msg);
                        }
                        match inbound {
                            Inbound::Control(c) => {
                                // "Stop" (Alt+Escape in VS Code) also ends look mode.
                                if c == Control::Stop {
                                    crate::look::set(&state, false, "cancelled");
                                }
                                state.tours.control(c).await
                            }
                            Inbound::Ask(q) if !q.trim().is_empty() => {
                                let st = state.clone();
                                tokio::spawn(async move { crate::brain::ask_and_present(st, q).await; });
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    if hub.clients.fetch_sub(1, Ordering::SeqCst) == 1 {
        // Nothing left to look at.
        crate::look::set(&state, false, "disconnected");
    }
    tracing::info!("VS Code disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hello_stores_folders_and_control_parses() {
        let hub = VsCodeHub::new();
        hub.handle_message(&json!({"type": "hello", "workspace_folders": ["C:\\proj"]}));
        assert_eq!(hub.workspace_folders(), vec!["C:\\proj"]);
        assert_eq!(
            hub.handle_message(&json!({"type": "tour.control", "action": "next"})),
            Inbound::Control(Control::Next)
        );
        assert_eq!(hub.handle_message(&json!({"type": "tour.control", "action": "dance"})), Inbound::None);
        assert_eq!(
            hub.handle_message(&json!({"type": "ask", "question": "where is login?"})),
            Inbound::Ask("where is login?".into())
        );
    }

    #[test]
    fn context_is_kept() {
        let hub = VsCodeHub::new();
        hub.handle_message(&json!({"type": "editor.context", "path": "a.ts", "cursor_line": 3}));
        assert_eq!(hub.editor_context().unwrap()["cursor_line"], 3);
        assert!(!hub.is_connected());
    }
}
