use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct ServerState {
    pub token: String,
    pub port: u16,
    pub app_handle: Option<AppHandle>,
    pub speech_queue: Option<crate::voice::queue::SpeechQueue>,
    pub registry: Arc<crate::agents::AgentRegistry>,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    version: &'static str,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn agents_handler(State(state): State<Arc<ServerState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "agents": state.registry.list() }))
}

/// Tell the overlay an agent's pet changed state.
pub fn emit_agent_state(state: &ServerState, kind: &str, pet_state: &str, text: Option<&str>) {
    if let Some(app) = &state.app_handle {
        let _ = app.emit(
            "agent:state",
            serde_json::json!({ "agent_kind": kind, "state": pet_state, "text": text }),
        );
    }
}

/// Speak through the queue with the voice configured for this agent kind.
pub async fn speak(
    state: &ServerState,
    kind: &str,
    text: &str,
    priority: crate::voice::queue::Priority,
    speech_kind: crate::voice::queue::SpeechKind,
) {
    let Some(queue) = &state.speech_queue else { return };
    let voice = crate::config::load_or_create_config()
        .ok()
        .and_then(|cfg| cfg.voice.voices.get(kind).cloned())
        .unwrap_or_else(|| "Toby".into());
    queue
        .enqueue(crate::voice::queue::SpeechRequest {
            agent_id: kind.to_string(),
            text: text.to_string(),
            priority,
            kind: speech_kind,
            voice: Some(voice),
        })
        .await;
}

#[derive(Deserialize)]
struct KindQuery {
    kind: Option<String>,
}

async fn mcp_handler(
    State(state): State<Arc<ServerState>>,
    axum::extract::Query(q): axum::extract::Query<KindQuery>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let kind = q.kind.unwrap_or_else(|| "claude-code".into());
    // JSON-RPC batches are legal; answer each message.
    let messages = match body {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    let mut replies = Vec::new();
    for m in &messages {
        if let Some(r) = crate::mcp::handle(&state, &kind, m).await {
            replies.push(r);
        }
    }
    match replies.len() {
        0 => StatusCode::ACCEPTED.into_response(),
        1 => Json(replies.remove(0)).into_response(),
        _ => Json(replies).into_response(),
    }
}

async fn hook_handler(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(kind): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let action = crate::agents::map_hook(&kind, &payload);
    state.registry.touch(&kind, action.state, action.feed.as_deref());
    if let Some(pet_state) = action.state {
        emit_agent_state(&state, &kind, pet_state, action.feed.as_deref());
    }
    if let Some((text, priority)) = &action.speech {
        if !(action.skip_if_done_recently && state.registry.done_recently(&kind)) {
            speak(&state, &kind, text, *priority, crate::voice::queue::SpeechKind::Alert).await;
        }
    }
    StatusCode::OK
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SayPayload {
    pub agent_kind: Option<String>,
    pub text: String,
}

async fn say_handler(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<SayPayload>,
) -> impl IntoResponse {
    tracing::info!(
        "Received say request: agent={:?}, text={}",
        payload.agent_kind,
        payload.text
    );

    if let Some(queue) = &state.speech_queue {
        let agent_id = payload
            .agent_kind
            .clone()
            .unwrap_or_else(|| "claude-code".into());

        let voice = if let Ok(cfg) = crate::config::load_or_create_config() {
            cfg.voice
                .voices
                .get(&agent_id)
                .cloned()
                .unwrap_or_else(|| "Toby".into())
        } else {
            "Toby".into()
        };

        queue
            .enqueue(crate::voice::queue::SpeechRequest {
                agent_id,
                text: payload.text.clone(),
                priority: crate::voice::queue::Priority::Normal,
                kind: crate::voice::queue::SpeechKind::Status,
                voice: Some(voice),
            })
            .await;
    } else if let Some(app) = &state.app_handle {
        if let Err(e) = app.emit("pet:say", &payload) {
            tracing::error!("Failed to emit pet:say event: {}", e);
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "queued" })),
    )
}

async fn auth_middleware(
    State(state): State<Arc<ServerState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers().get(header::AUTHORIZATION);
    if let Some(auth_val) = auth_header {
        if let Ok(auth_str) = auth_val.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str["Bearer ".len()..];
                if token == state.token {
                    return Ok(next.run(req).await);
                }
            }
        }
    }

    tracing::warn!("Unauthorized request: invalid or missing token");
    Err(StatusCode::UNAUTHORIZED)
}

pub fn build_router(state: Arc<ServerState>) -> Router {
    let protected_routes = Router::new()
        .route("/api/v1/agents", get(agents_handler))
        .route("/api/v1/say", post(say_handler))
        .route("/api/v1/hooks/{kind}", post(hook_handler))
        .route("/mcp", post(mcp_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/health", get(health_handler))
        .merge(protected_routes)
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

pub async fn start_server(
    token: String,
    preferred_port: u16,
    app_handle: Option<AppHandle>,
    speech_queue: Option<crate::voice::queue::SpeechQueue>,
    registry: Arc<crate::agents::AgentRegistry>,
) -> Result<(u16, tokio::task::JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    let mut bound_listener = None;
    let mut bound_port = preferred_port;

    let ports_to_try: Vec<u16> = std::iter::once(preferred_port)
        .chain(4748..=4757)
        .collect();

    for port in ports_to_try {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!("Bound server to 127.0.0.1:{}", port);
                bound_port = port;
                bound_listener = Some(listener);
                break;
            }
            Err(e) => {
                tracing::warn!("Port {} unavailable: {}", port, e);
            }
        }
    }

    let listener = bound_listener.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "No ports available between 4747 and 4757",
        )
    })?;

    let state = Arc::new(ServerState {
        token,
        port: bound_port,
        app_handle,
        speech_queue,
        registry,
    });

    let app = build_router(state);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("Server error: {}", e);
        }
    });

    Ok((bound_port, handle))
}

#[cfg(test)]
pub fn test_state_with_token(token: &str) -> ServerState {
    ServerState {
        token: token.to_string(),
        port: 4747,
        app_handle: None,
        speech_queue: None,
        registry: Arc::new(crate::agents::AgentRegistry::default()),
    }
}

#[cfg(test)]
pub fn test_state() -> ServerState {
    test_state_with_token("test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = Arc::new(test_state_with_token("testtoken123"));
        let app = build_router(state);

        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn test_auth_unauthorized_without_token() {
        let state = Arc::new(test_state_with_token("secrettoken456"));
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/v1/agents")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_authorized_with_bearer_token() {
        let state = Arc::new(test_state_with_token("secrettoken456"));
        let app = build_router(state);

        let req = Request::builder()
            .uri("/api/v1/agents")
            .header(header::AUTHORIZATION, "Bearer secrettoken456")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
