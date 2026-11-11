pub mod agents;
pub mod brain;
pub mod config;
pub mod connect;
pub mod logging;
pub mod look;
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
/// False when another program already owns the talk key, so Barnaby can say so.
static PTT_REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Hold the shortcut to talk. Pressing it also cuts any speech (barge-in). Returns false when
/// Windows refuses the key (usually because another program has it).
fn register_push_to_talk(app: &tauri::AppHandle, core: Arc<server::ServerState>, shortcut: &str) -> bool {
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
    let ok = match result {
        Ok(()) => {
            tracing::info!("Push-to-talk registered on {shortcut}");
            true
        }
        Err(e) => {
            tracing::warn!("Could not register push-to-talk {shortcut}: {e}");
            false
        }
    };
    PTT_REGISTERED.store(ok, std::sync::atomic::Ordering::SeqCst);
    ok
}

/// Pick another talk key. Keeps the old one if Windows refuses the new one.
#[tauri::command]
fn set_push_to_talk(
    shortcut: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<server::ServerState>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    let shortcut = shortcut.trim().to_string();
    if shortcut.is_empty() {
        return Err("Type a key combination, for example Ctrl+Alt+K.".into());
    }
    let _ = app.global_shortcut().unregister_all();
    if register_push_to_talk(&app, state.inner().clone(), &shortcut) {
        cfg.push_to_talk = shortcut;
        return config::save_config(&cfg).map_err(|e| e.to_string());
    }
    // Put the old key back.
    register_push_to_talk(&app, state.inner().clone(), &cfg.push_to_talk);
    Err(format!("Windows would not give me {shortcut}: another program is using it, or it is not a valid key combination."))
}

/// `shown` is what the chat list displays as the user's message (a button's label, for example).
#[tauri::command]
async fn ask_question(
    question: String,
    shown: Option<String>,
    state: tauri::State<'_, Arc<server::ServerState>>,
) -> Result<(), String> {
    let st = state.inner().clone();
    tokio::spawn(async move {
        brain::ask_and_present_as(st, question, shown).await;
    });
    Ok(())
}

/// The monocle was clicked: wait for a selection in VS Code (on), or stop waiting (off).
/// Fails with "vscode" when VS Code is not linked, because there is nothing to look at then.
#[tauri::command]
fn set_look_mode(on: bool, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    if on && !state.hub.is_connected() {
        return Err("vscode".into());
    }
    look::set(state.inner(), on, if on { "monocle" } else { "cancelled" });
    Ok(())
}

#[tauri::command]
fn set_look_after_select(after: config::AfterSelect) -> Result<(), String> {
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    cfg.look.after_select = after;
    config::save_config(&cfg).map_err(|e| e.to_string())
}

/// Forget the conversation so far.
#[tauri::command]
fn reset_conversation(state: tauri::State<'_, Arc<server::ServerState>>) {
    brain::reset_conversation(state.inner());
}

#[derive(serde::Serialize)]
struct SetupStatus {
    has_key: bool,
    /// VS Code is talking to Barnaby right now.
    vscode: bool,
    /// The extension is on disk, whether or not VS Code has loaded it yet.
    extension_installed: bool,
    folders: Vec<String>,
    onboarding_done: bool,
    talk_key: String,
    talk_key_ok: bool,
}

/// True when Barnaby's extension sits in the extensions folder of VS Code or a close relative.
fn extension_installed() -> bool {
    let Some(home) = dirs::home_dir() else { return false };
    [".vscode", ".vscode-insiders", ".cursor", ".antigravity"].iter().any(|d| {
        std::fs::read_dir(home.join(d).join("extensions"))
            .is_ok_and(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("barnaby.barnaby-vscode")))
    })
}

/// What the first-run guide ticks off.
#[tauri::command]
fn setup_status(state: tauri::State<'_, Arc<server::ServerState>>) -> SetupStatus {
    let cfg = config::load_or_create_config().ok();
    SetupStatus {
        has_key: brain::load_api_key().is_some(),
        vscode: state.hub.is_connected(),
        extension_installed: extension_installed(),
        folders: state.hub.workspace_folders(),
        onboarding_done: cfg.as_ref().map(|c| c.onboarding_done).unwrap_or(false),
        talk_key: cfg.map(|c| c.push_to_talk).unwrap_or_default(),
        talk_key_ok: PTT_REGISTERED.load(std::sync::atomic::Ordering::SeqCst),
    }
}

/// The guide was finished or skipped: do not show it on every start.
#[tauri::command]
fn finish_onboarding() -> Result<(), String> {
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    cfg.onboarding_done = true;
    config::save_config(&cfg).map_err(|e| e.to_string())
}

/// The newest `barnaby-vscode-*.vsix` that ships beside the app (or in the repo while developing).
fn find_vsix() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find_map(|dir| {
        ["vscode-extension", "resources/vscode"].iter().find_map(|sub| {
            let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir.join(sub))
                .ok()?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|x| x == "vsix")
                        && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("barnaby-vscode"))
                })
                .collect();
            found.sort();
            found.pop()
        })
    })
}

/// VS Code's command line tool: on PATH, or in the default per-user install.
fn find_code_cli() -> Option<std::path::PathBuf> {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("code.cmd"))
        .find(|p| p.exists());
    on_path.or_else(|| {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let p = std::path::PathBuf::from(local).join("Programs/Microsoft VS Code/bin/code.cmd");
        p.exists().then_some(p)
    })
}

/// Install Barnaby's extension into VS Code. The user pressed the button, so this is their go-ahead.
#[tauri::command]
async fn install_vscode_extension() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        use std::os::windows::process::CommandExt;
        let vsix = find_vsix().ok_or("I could not find my VS Code extension file (barnaby-vscode-*.vsix).")?;
        let code = find_code_cli().ok_or_else(|| {
            format!(
                "I could not find VS Code's command line. In VS Code open Extensions, choose Install from VSIX, and pick {}.",
                vsix.display()
            )
        })?;
        let out = std::process::Command::new(code)
            .arg("--install-extension")
            .arg(&vsix)
            .arg("--force")
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| format!("Could not run VS Code: {e}"))?;
        if out.status.success() {
            Ok("Installed. In VS Code press Ctrl+Shift+P and run \"Reload Window\", and I will notice it.".to_string())
        } else {
            Err(format!("VS Code said: {}", String::from_utf8_lossy(&out.stderr).trim()))
        }
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Tour setting: true moves VS Code for the user, false only says where to look.
#[tauri::command]
fn set_tour_navigate(navigate: bool) -> Result<(), String> {
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    cfg.tour.navigate = navigate;
    config::save_config(&cfg).map_err(|e| e.to_string())
}

/// Tour setting: wait for the person at each step, or move on by himself.
#[tauri::command]
fn set_tour_pace(pace: config::TourPace) -> Result<(), String> {
    let mut cfg = config::load_or_create_config().map_err(|e| e.to_string())?;
    cfg.tour.pace = pace;
    config::save_config(&cfg).map_err(|e| e.to_string())
}

/// Speak a line in Barnaby's own voice (greetings, "clearing my throat").
#[tauri::command]
async fn say_text(text: String, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    server::speak(&state, "barnaby", &text, voice::queue::Priority::High, voice::queue::SpeechKind::Status).await;
    Ok(())
}

/// Speak a sample in any voice, for the voice picker in Settings.
#[tauri::command]
async fn preview_voice(voice: String, state: tauri::State<'_, Arc<server::ServerState>>) -> Result<(), String> {
    let queue = state.speech_queue.as_ref().ok_or("The voice engine is not ready yet.")?;
    queue
        .enqueue(voice::queue::SpeechRequest {
            agent_id: "preview".into(),
            text: format!("Good evening. I am {voice}, at your service."),
            priority: voice::queue::Priority::High,
            kind: voice::queue::SpeechKind::Status,
            voice: Some(voice),
        })
        .await;
    Ok(())
}

#[derive(serde::Serialize)]
struct AppStatus {
    vscode: bool,
    folders: Vec<String>,
    agents: Vec<agents::AgentInfo>,
}

/// What Settings shows at the top: is VS Code linked, which agents are talking to Barnaby.
#[tauri::command]
fn app_status(state: tauri::State<'_, Arc<server::ServerState>>) -> AppStatus {
    AppStatus {
        vscode: state.hub.is_connected(),
        folders: state.hub.workspace_folders(),
        agents: state.registry.list(),
    }
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
            overlay::platforms::scan_window_platforms,
            overlay::platforms::get_window_bounds,
            overlay::get_config,
            overlay::save_pet_position,
            overlay::open_settings,
            overlay::exit_app,
            overlay_ready,
            ptt_audio,
            ask_question,
            set_look_mode,
            set_look_after_select,
            reset_conversation,
            setup_status,
            finish_onboarding,
            install_vscode_extension,
            set_push_to_talk,
            accept_tour,
            tour_control,
            save_api_key,
            test_api_key,
            has_api_key,
            connect_preview_claude,
            connect_claude,
            set_tour_navigate,
            set_tour_pace,
            say_text,
            preview_voice,
            app_status,
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
                conversation: Default::default(),
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
