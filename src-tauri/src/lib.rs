pub mod agents;
pub mod brain;
pub mod config;
pub mod connect;
pub mod logging;
pub mod mcp;
pub mod overlay;
pub mod server;
pub mod tours;
pub mod voice_in;
pub mod vscode;
pub mod voice;

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
use voice::{
    phonon::PhononManager,
    playback::AudioPlayer,
    queue::SpeechQueue,
    VoiceState,
};

static OVERLAY_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const OVERLAY_READY_TIMEOUT_SECS: u64 = 10;

/// Called by the overlay page once it has mounted. Only then is the overlay window shown.
#[tauri::command]
fn overlay_ready(app: tauri::AppHandle) {
    OVERLAY_READY.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.show();
    }
}

/// A finished push-to-talk recording (base64 WAV) from the overlay page.
#[tauri::command]
async fn ptt_audio(wav_base64: String, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    use base64::Engine;
    let wav = base64::engine::general_purpose::STANDARD
        .decode(wav_base64)
        .map_err(|e| format!("bad audio: {e}"))?;
    let st = state.inner().clone();
    tokio::spawn(async move {
        voice_in::handle_recording(st, wav).await;
    });
    Ok(())
}

static PTT_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Hold the shortcut to talk. Pressing it also cuts any speech (barge-in).
fn register_push_to_talk(app: &tauri::AppHandle, core: Arc<server::ServerState>, shortcut: &str) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let result = app.global_shortcut().on_shortcut(shortcut, move |app, _shortcut, event| match event.state() {
        ShortcutState::Pressed => {
            if !PTT_HELD.swap(true, Ordering::SeqCst) {
                let core = core.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(q) = &core.speech_queue {
                        q.stop_speech().await;
                    }
                });
                let _ = app.emit("ptt:start", ());
            }
        }
        ShortcutState::Released => {
            if PTT_HELD.swap(false, Ordering::SeqCst) {
                let _ = app.emit("ptt:stop", ());
            }
        }
    });
    match result {
        Ok(()) => tracing::info!("Push-to-talk registered on {shortcut}"),
        Err(e) => tracing::warn!("Could not register push-to-talk {shortcut}: {e}"),
    }
}

#[tauri::command]
async fn ask_question(question: String, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    let st = state.inner().clone();
    tokio::spawn(async move {
        brain::ask_and_present(st, question).await;
    });
    Ok(())
}

#[tauri::command]
async fn accept_tour(state: tauri::State<'_, Arc<server::ServerState>>) -> Result<usize, String> {
    Ok(state.tours.start_pending("barnaby", server::pet_colour("barnaby")).await)
}

#[tauri::command]
async fn tour_control(action: String, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    if let Some(c) = tours::Control::parse(&action) {
        state.tours.control(c).await;
    }
    Ok(())
}

#[tauri::command]
fn save_api_key(key: String) -> Result<(), String> {
    brain::save_api_key(&key)
}

#[tauri::command]
fn has_api_key() -> bool {
    brain::load_api_key().is_some()
}

#[tauri::command]
async fn test_api_key() -> Result<String, String> {
    brain::test_connection().await
}

#[tauri::command]
fn connect_preview_claude() -> Result<String, String> {
    let cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    Ok(connect::describe_claude_connect(cfg.port))
}

#[tauri::command]
async fn connect_claude() -> Result<String, String> {
    let cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || connect::connect_claude_code(cfg.port, &cfg.token))
        .await
        .map_err(|e| e.to_string())?
}

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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(OverlayState {
            clickthrough: clickthrough_for_state,
        })
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_regions,
            overlay::set_drag_state,
            overlay::get_config,
            overlay::save_pet_position,
            overlay::open_settings,
            overlay::exit_app,
            overlay_ready,
            ptt_audio,
            ask_question,
            accept_tour,
            tour_control,
            save_api_key,
            test_api_key,
            has_api_key,
            connect_preview_claude,
            connect_claude,
            voice::get_phonon_status,
            voice::start_phonon,
            voice::install_uv,
            voice::install_ptts,
            voice::stop_speech,
            voice::set_voice_volume,
            voice::set_voice_enabled,
            voice::set_agent_voice
        ])
        .setup(move |app| {
            // Initialize voice subsystem
            let phonon = Arc::new(PhononManager::new());
            let player = match AudioPlayer::new() {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("AudioPlayer initialization failed: {}", e);
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e,
                    )));
                }
            };

            if !app_config.voice.enabled {
                player.set_volume(0.0);
            } else {
                player.set_volume(app_config.voice.volume);
            }

            let queue = SpeechQueue::new(phonon.clone(), player.clone(), Some(app.handle().clone()));

            app.manage(VoiceState {
                phonon: phonon.clone(),
                player: player.clone(),
                queue: queue.clone(),
            });

            // Automatically check and start Phonon if configured and uv/ptts available
            let phonon_auto = phonon.clone();
            tauri::async_runtime::spawn(async move {
                let status = phonon_auto.refresh_status().await;
                if status.state == voice::phonon::PhononState::Ready {
                    tracing::info!("Phonon server is already running and ready");
                } else if status.uv_installed && status.ptts_installed {
                    tracing::info!("Starting Phonon server automatically");
                    if let Err(e) = phonon_auto.start_server(None).await {
                        tracing::warn!("Auto-start of Phonon server failed: {}", e);
                    }
                }
            });
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
            .title("Barnaby Overlay")
            .position(bounds.x as f64, bounds.y as f64)
            .inner_size(bounds.width as f64, bounds.height as f64)
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            // Hidden until the page says it loaded. A failed load (for example the dev server
            // being down) would otherwise cover the whole screen with a browser error page.
            .visible(false)
            .build()?;

            let watchdog_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(OVERLAY_READY_TIMEOUT_SECS)).await;
                if !OVERLAY_READY.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::error!("Overlay page did not load in {OVERLAY_READY_TIMEOUT_SECS}s; closing the overlay");
                    if let Some(w) = watchdog_app.get_webview_window("overlay") {
                        let _ = w.close();
                    }
                }
            });

            // Ensure physical pixel matching across multi-monitor & high-DPI scaling
            let _ = overlay_window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(bounds.x, bounds.y)));
            let _ = overlay_window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(bounds.width as u32, bounds.height as u32)));

            // Start cursor watcher for click-through
            clickthrough.start_watcher(overlay_window);

            // 2. Create settings window (hidden by default)
            let _settings_window = WebviewWindowBuilder::new(
                app,
                "settings",
                WebviewUrl::App("index.html".into()),
            )
            .title("Barnaby Settings")
            .inner_size(640.0, 560.0)
            .center()
            .visible(false)
            .build()?;

            // Shared state for the HTTP server, MCP, VS Code link, tours and the Tauri commands
            let hub = Arc::new(vscode::VsCodeHub::new());
            let tours = tours::TourEngine::new(hub.clone(), Some(queue.clone()), Some(app.handle().clone()));
            let core = Arc::new(server::ServerState {
                token: server_token.clone(),
                port: preferred_port,
                app_handle: Some(app.handle().clone()),
                speech_queue: Some(queue.clone()),
                registry: Arc::new(agents::AgentRegistry::default()),
                hub,
                tours,
            });
            app.manage(core.clone());
            register_push_to_talk(app.handle(), core.clone(), &app_config.push_to_talk);

            // Start axum server in tokio runtime
            tauri::async_runtime::spawn(async move {
                match server::start_server(core, preferred_port).await {
                    Ok((actual_port, _handle)) => {
                        tracing::info!("Barnaby axum server running on port {}", actual_port);
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
        .on_window_event(|window, event| {
            // Closing Settings hides it. Destroying it would leave the tray with nothing to reopen.
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
