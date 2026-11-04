pub mod config;
pub mod logging;
pub mod server;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager,
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
                            if let Some(window) = app.get_webview_window("main") {
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
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }

            let _tray = builder.build(app)?;

            // Start axum server in tokio runtime
            let token_clone = server_token.clone();
            tauri::async_runtime::spawn(async move {
                match server::start_server(token_clone, preferred_port).await {
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
