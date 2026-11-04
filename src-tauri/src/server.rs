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
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct ServerState {
    pub token: String,
    pub port: u16,
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

#[derive(Serialize)]
struct AgentsResponse {
    agents: Vec<String>,
}

async fn agents_handler() -> Json<AgentsResponse> {
    Json(AgentsResponse { agents: vec![] })
}

#[derive(Deserialize)]
struct SayPayload {
    agent_kind: Option<String>,
    text: String,
}

async fn say_handler(Json(payload): Json<SayPayload>) -> impl IntoResponse {
    tracing::info!(
        "Received say request: agent={:?}, text={}",
        payload.agent_kind,
        payload.text
    );
    (StatusCode::OK, Json(serde_json::json!({ "status": "queued" })))
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
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = Arc::new(ServerState {
            token: "testtoken123".to_string(),
            port: 4747,
        });
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
        let state = Arc::new(ServerState {
            token: "secrettoken456".to_string(),
            port: 4747,
        });
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
        let state = Arc::new(ServerState {
            token: "secrettoken456".to_string(),
            port: 4747,
        });
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
