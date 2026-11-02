pub mod axum_server;
pub mod grpc_stub;

use serde::{Deserialize, Serialize};

/// High-level serialized snapshot of the desktop environment and pet state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub state: String,
    pub standing_on_title: Option<String>,
    pub foreground_window_title: String,
    pub user_idle_ms: u64,
    pub windows: Vec<WindowInfoSnapshot>,
    pub active_speech: Option<String>,
    pub debug_platforms_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfoSnapshot {
    pub hwnd: isize,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_active: bool,
}

/// Commands accepted by the Desktop Pet Engine via REST, WebSocket, or gRPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PetCommand {
    WalkToWindow { title_contains: String },
    JumpToPoint { x: f32, y: f32 },
    ShakeActiveWindow,
    MoveToCaret,
    Say { text: String, duration_ms: Option<u64> },
    ToggleDebugPlatforms { enabled: bool },
}

/// Real-time desktop and pet events streamed to agents via WebSocket / gRPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DesktopEvent {
    WindowFocused { hwnd: isize, title: String },
    PetClicked { x: f32, y: f32 },
    PetLandedOnWindow { hwnd: isize, title: String },
    PetThrown { vx: f32, vy: f32 },
    UserIdle { idle_ms: u64 },
}
