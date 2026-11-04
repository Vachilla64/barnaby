pub mod config;
pub mod logging;
pub mod overlay;
pub mod server;

use overlay::{
    clickthrough::ClickthroughManager,
    monitors,
    OverlayState,
};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager, WebviewUrl, WebviewWindowBuilder,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging first
    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }

    // Load or create config
    let app_config = match config::load_or_create_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load/create config: {}", e);
            config::AppConfig::default()
        }
    };

    let server_token = app_config.token.clone();
    let preferred_port = app_config.port;

    let clickthrough = Arc::new(ClickthroughManager::new());
    let clickthrough_for_state = clickthrough.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(OverlayState {
            clickthrough: clickthrough_for_state,
        })
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_regions,
            overlay::set_drag_state,
            overlay::get_config,
            overlay::save_pet_position,
            overlay::open_settings,
            overlay::exit_app
        ])
        .setup(move |app| {
            // Build tray icon menu
            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;

            let menu = Menu::with_items(app, &[&settings_i, &separator, &quit_i])?;

            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "settings" => {
                            tracing::info!("Tray: Settings clicked");
                            if let Some(window) = app.get_webview_window("settings") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            tracing::info!("Tray: Quit clicked");
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("settings") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }

            let _tray = builder.build(app)?;

            // 1. Create multi-monitor transparent overlay window
            let bounds = monitors::get_virtual_screen_bounds();
            tracing::info!(
                "Virtual screen bounds: x={}, y={}, w={}, h={}",
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height
            );

            // Clean up old "main" window if present from default config
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.close();
            }

            let overlay_window = WebviewWindowBuilder::new(
                app,
                "overlay",
                WebviewUrl::App("index.html".into()),
            )
            .title("Devy Overlay")
            .position(bounds.x as f64, bounds.y as f64)
            .inner_size(bounds.width as f64, bounds.height as f64)
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .build()?;

            // Start cursor watcher for click-through
            clickthrough.start_watcher(overlay_window);

            // 2. Create settings window (hidden by default)
            let _settings_window = WebviewWindowBuilder::new(
                app,
                "settings",
                WebviewUrl::App("index.html".into()),
            )
            .title("Devy Settings")
            .inner_size(640.0, 560.0)
            .center()
            .visible(false)
            .build()?;

            // Start axum server in tokio runtime
            let token_clone = server_token.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match server::start_server(token_clone, preferred_port, Some(app_handle)).await {
                    Ok((actual_port, _handle)) => {
                        tracing::info!("Devy axum server running on port {}", actual_port);
                        if actual_port != preferred_port {
                            tracing::warn!(
                                "Port fallback used: {} (configured was {})",
                                actual_port,
                                preferred_port
                            );
                            if let Ok(mut cfg) = config::load_or_create_config() {
                                cfg.port = actual_port;
                                let _ = config::save_config(&cfg);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to start axum server: {}", e);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
