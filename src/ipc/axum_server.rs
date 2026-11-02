use std::net::SocketAddr;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, watch};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

use super::{DesktopEvent, DesktopSnapshot, PetCommand};

#[derive(Clone)]
pub struct AppState {
    pub cmd_tx: mpsc::Sender<PetCommand>,
    pub state_rx: watch::Receiver<DesktopSnapshot>,
    pub event_tx: broadcast::Sender<DesktopEvent>,
}

/// Spawns the Axum REST & WebSocket agent server on `127.0.0.1:8765`.
pub fn start_axum_server(
    cmd_tx: mpsc::Sender<PetCommand>,
    state_rx: watch::Receiver<DesktopSnapshot>,
    event_tx: broadcast::Sender<DesktopEvent>,
) {
    let state = AppState {
        cmd_tx,
        state_rx,
        event_tx,
    };

    tokio::spawn(async move {
        let app = Router::new()
            .route("/state", get(get_state_handler))
            .route("/command", post(post_command_handler))
            .route("/ws", get(ws_upgrade_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], 8765));
        info!("Starting Desktop Pet Agent API on http://{}", addr);

        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    error!("Axum server runtime error: {e}");
                }
            }
            Err(e) => {
                error!("Failed to bind Axum server to {}: {e}", addr);
            }
        }
    });
}

/// GET /state — returns serialized snapshot of the desktop environment and pet
async fn get_state_handler(State(state): State<AppState>) -> Json<DesktopSnapshot> {
    let snapshot = state.state_rx.borrow().clone();
    Json(snapshot)
}

/// POST /command — accepts JSON PetCommand and dispatches it to the engine channel
async fn post_command_handler(
    State(state): State<AppState>,
    Json(cmd): Json<PetCommand>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .cmd_tx
        .send(cmd)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to dispatch command: {e}")))?;

    Ok(Json(json!({ "status": "ok", "message": "Command dispatched to pet engine" })))
}

/// GET /ws — upgrades connection to WebSocket for real-time events and bidirectional command streaming
async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, state))
}

async fn handle_ws_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.event_tx.subscribe();

    info!("New agent WebSocket connected");

    loop {
        tokio::select! {
            // Forward desktop events to connected agent
            event_res = event_rx.recv() => {
                match event_res {
                    Ok(event) => {
                        if let Ok(json_str) = serde_json::to_string(&event) {
                            if sender.send(Message::Text(json_str.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("Agent WebSocket client lagged, skipped {} events", skipped);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Receive commands from agent via WebSocket
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<PetCommand>(&text) {
                            Ok(cmd) => {
                                if let Err(e) = state.cmd_tx.send(cmd).await {
                                    error!("Failed to dispatch WebSocket command: {e}");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Invalid WebSocket command JSON: {e}");
                                let err_resp = json!({ "error": "Invalid command JSON", "details": e.to_string() });
                                let _ = sender.send(Message::Text(err_resp.to_string().into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("Agent WebSocket disconnected");
}
