pub mod clickthrough;
pub mod monitors;
pub mod platforms;

use crate::config;
pub use clickthrough::{ClickRegion, ClickthroughManager};
pub use platforms::{get_window_bounds, scan_window_platforms, PlatformRect, PlatformSegment};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

pub struct OverlayState {
    pub clickthrough: Arc<ClickthroughManager>,
}

#[tauri::command]
pub fn set_click_regions(regions: Vec<ClickRegion>, state: State<'_, OverlayState>) {
    tracing::debug!("Setting {} click regions: {:?}", regions.len(), regions);
    state.clickthrough.set_regions(regions);
}

#[tauri::command]
pub fn set_drag_state(is_dragging: bool, app: AppHandle, state: State<'_, OverlayState>) {
    let window = app.get_webview_window("overlay");
    state.clickthrough.set_dragging(is_dragging, window.as_ref());
}

#[tauri::command]
pub fn get_config() -> Result<config::AppConfig, String> {
    config::load_or_create_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_pet_position(agent_kind: String, x: i32, y: i32) -> Result<(), String> {
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    let pet = cfg.pets.entry(agent_kind).or_insert_with(|| config::PetConfig {
        colour: "#E8875B".to_string(),
        x: None,
        y: None,
    });
    pet.x = Some(x);
    pet.y = Some(y);
    config::save_config(&cfg).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}
