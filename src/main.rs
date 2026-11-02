mod ipc;
mod physics;
mod renderer;
mod win32_scanner;

use std::sync::Arc;
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tokio::sync::{broadcast, mpsc, watch};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::ipc::axum_server::start_axum_server;
use crate::ipc::{DesktopEvent, DesktopSnapshot, PetCommand, WindowInfoSnapshot};
use crate::physics::WorldState;
use crate::renderer::softbuffer_skia::SoftbufferSkiaRenderer;
use crate::renderer::RenderBackend;
use crate::win32_scanner::{timeBeginPeriod, timeEndPeriod, WindowScanner};

struct DesktopPetApp {
    // Spatial bounds
    virtual_x: i32,
    virtual_y: i32,
    virtual_w: u32,
    virtual_h: u32,

    // Window & renderer
    window: Option<Arc<Window>>,
    renderer: Option<SoftbufferSkiaRenderer>,
    own_hwnd: Option<isize>,

    // Subsystems
    scanner: WindowScanner,
    world: WorldState,

    // IPC channels
    cmd_rx: mpsc::Receiver<PetCommand>,
    state_tx: watch::Sender<DesktopSnapshot>,
    event_tx: broadcast::Sender<DesktopEvent>,

    // Timing, clicks & hit-testing state
    last_tick: Instant,
    last_scan: Instant,
    last_foreground_title: String,
    last_click_time: Instant,
    is_interactive_hovered: bool,
}

impl DesktopPetApp {
    pub fn new(
        cmd_rx: mpsc::Receiver<PetCommand>,
        state_tx: watch::Sender<DesktopSnapshot>,
        event_tx: broadcast::Sender<DesktopEvent>,
    ) -> Self {
        let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(800) as u32;
        let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(600) as u32;

        info!(
            "Detected Windows Virtual Desktop: origin=({}, {}), dimensions={}x{}",
            vx, vy, vw, vh
        );

        let world = WorldState::new(vw as f32, vh as f32);
        let scanner = WindowScanner::new(vx, vy, vw as i32, vh as i32);

        Self {
            virtual_x: vx,
            virtual_y: vy,
            virtual_w: vw,
            virtual_h: vh,
            window: None,
            renderer: None,
            own_hwnd: None,
            scanner,
            world,
            cmd_rx,
            state_tx,
            event_tx,
            last_tick: Instant::now(),
            last_scan: Instant::now(),
            last_foreground_title: String::new(),
            last_click_time: Instant::now() - Duration::from_secs(10),
            is_interactive_hovered: false,
        }
    }

    /// Configures Win32 borderless transparent click-through layering and Alt+Tab exclusion.
    fn setup_win32_window_styles(&mut self, window: &Window) {
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                let hwnd = win32_handle.hwnd.get() as isize;
                self.own_hwnd = Some(hwnd);
                self.scanner.set_own_hwnd(hwnd);

                unsafe {
                    let win_hwnd = HWND(hwnd as *mut _);
                    let cur_ex_style = GetWindowLongW(win_hwnd, GWL_EXSTYLE) as u32;

                    let target_ex_style = cur_ex_style
                        | WS_EX_TOOLWINDOW.0
                        | WS_EX_LAYERED.0
                        | WS_EX_TOPMOST.0
                        | WS_EX_TRANSPARENT.0;

                    SetWindowLongW(win_hwnd, GWL_EXSTYLE, target_ex_style as i32);
                    let _ = SetWindowPos(
                        win_hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
                info!("Applied native Win32 layered click-through styles to HWND 0x{:X}", hwnd);
            }
        }
    }

    /// Evaluates if global cursor hovers over the pet or speech bubble.
    /// Toggles `WS_EX_TRANSPARENT` dynamically so mouse clicks pass through everywhere else.
    fn update_dynamic_hit_testing(&mut self) {
        let cursor_pos = WindowScanner::get_cursor_pos();

        let is_over_pet = if let Some((cx, cy)) = cursor_pos {
            let rel_x = (cx - self.virtual_x) as f32;
            let rel_y = (cy - self.virtual_y) as f32;
            self.world.cursor_pos = Some((rel_x, rel_y));

            let hits = self.world.hit_test(rel_x, rel_y);
            if hits {
                self.world.pet_by_cursor(rel_x, rel_y);
            }
            hits
        } else {
            self.world.cursor_pos = None;
            false
        };

        if is_over_pet != self.is_interactive_hovered {
            self.is_interactive_hovered = is_over_pet;

            if let Some(window) = &self.window {
                let _ = window.set_cursor_hittest(is_over_pet);
            }

            if let Some(hwnd) = self.own_hwnd {
                unsafe {
                    let win_hwnd = HWND(hwnd as *mut _);
                    let cur_ex = GetWindowLongW(win_hwnd, GWL_EXSTYLE) as u32;
                    let target_ex = if is_over_pet {
                        cur_ex & !WS_EX_TRANSPARENT.0
                    } else {
                        cur_ex | WS_EX_TRANSPARENT.0
                    };

                    SetWindowLongW(win_hwnd, GWL_EXSTYLE, target_ex as i32);
                    let _ = SetWindowPos(
                        win_hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
            }
        }
    }

    /// Dispatches external IPC commands received from REST, WebSocket, or gRPC
    fn process_ipc_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            info!("Processing agent command: {:?}", cmd);
            match cmd {
                PetCommand::WalkToWindow { title_contains } => {
                    let found = self.world.walk_to_window(&title_contains);
                    if found {
                        self.world.say(format!("Walking to window: {}", title_contains), 3000);
                    } else {
                        self.world.say(format!("Window not found: {}", title_contains), 3000);
                    }
                }
                PetCommand::JumpToPoint { x, y } => {
                    self.world.jump_to_point(x, y);
                    self.world.say("Wheee! Jumping!".to_string(), 2500);
                }
                PetCommand::ShakeActiveWindow => {
                    let foreground = self.scanner.scan_windows().into_iter().find(|w| w.is_active);
                    if let Some(active) = foreground {
                        WindowScanner::shake_window(active.hwnd);
                        self.world.say(format!("Shook window: {}", active.title), 3000);
                    }
                }
                PetCommand::MoveToCaret => {
                    if let Some((cx, cy)) = WindowScanner::track_text_caret() {
                        let rel_x = (cx - self.virtual_x) as f32;
                        let rel_y = (cy - self.virtual_y) as f32;
                        self.world.jump_to_point(rel_x, rel_y - 30.0);
                        self.world.say("Visiting your cursor!".to_string(), 3000);
                    } else {
                        self.world.say("No text caret detected.".to_string(), 2500);
                    }
                }
                PetCommand::Say { text, duration_ms } => {
                    self.world.say(text, duration_ms.unwrap_or(4000));
                }
                PetCommand::ToggleDebugPlatforms { enabled } => {
                    self.world.debug_draw_platforms = enabled;
                    let msg = if enabled { "Debug platforms ON" } else { "Debug platforms OFF" };
                    self.world.say(msg.to_string(), 2000);
                }
            }
        }
    }

    /// Broadcasts the current world and window snapshot to agent channels
    fn broadcast_state_snapshot(&self) {
        let standing_title = self.world.standing_on.and_then(|hwnd| {
            self.world.scanned_windows.iter().find(|w| w.hwnd == hwnd).map(|w| w.title.clone())
        });

        let foreground_title = WindowScanner::get_foreground_window_title();
        let idle_ms = WindowScanner::check_user_idle_ms();

        let windows_snapshot = self.world.scanned_windows.iter().map(|w| WindowInfoSnapshot {
            hwnd: w.hwnd,
            title: w.title.clone(),
            x: w.bounds.left,
            y: w.bounds.top,
            width: w.bounds.width(),
            height: w.bounds.height(),
            is_active: w.is_active,
        }).collect();

        let snapshot = DesktopSnapshot {
            pet_x: self.world.x,
            pet_y: self.world.y,
            pet_vx: self.world.vx,
            pet_vy: self.world.vy,
            state: format!("{:?}", self.world.state_kind),
            standing_on_title: standing_title,
            foreground_window_title: foreground_title,
            user_idle_ms: idle_ms,
            windows: windows_snapshot,
            active_speech: self.world.active_speech.as_ref().map(|s| s.text.clone()),
            debug_platforms_enabled: self.world.debug_draw_platforms,
        };

        let _ = self.state_tx.send_replace(snapshot);
    }
}

impl ApplicationHandler for DesktopPetApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        info!("Creating fullscreen transparent overlay window on Windows 11...");

        let window_attrs = WindowAttributes::default()
            .with_title("Desktop Pet Overlay")
            .with_transparent(true)
            .with_decorations(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(self.virtual_x, self.virtual_y))
            .with_inner_size(PhysicalSize::new(self.virtual_w, self.virtual_h));

        let window = match event_loop.create_window(window_attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!("Failed to create transparent overlay window: {e}");
                return;
            }
        };

        self.setup_win32_window_styles(&window);

        match SoftbufferSkiaRenderer::new(window.clone(), self.virtual_w, self.virtual_h) {
            Ok(renderer) => {
                info!("Softbuffer 2D CPU renderer initialized successfully");
                self.renderer = Some(renderer);
            }
            Err(e) => {
                error!("Failed to initialize Softbuffer renderer: {e}");
            }
        }

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                    self.world.screen_width = size.width as f32;
                    self.world.screen_height = size.height as f32;
                    self.scanner.set_screen_bounds(
                        self.virtual_x,
                        self.virtual_y,
                        size.width as i32,
                        size.height as i32,
                    );
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if let Some((cx, cy)) = WindowScanner::get_cursor_pos() {
                    let rel_x = (cx - self.virtual_x) as f32;
                    let rel_y = (cy - self.virtual_y) as f32;

                    match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            if self.world.hit_test(rel_x, rel_y) {
                                let now = Instant::now();
                                if now.duration_since(self.last_click_time).as_millis() < 350 {
                                    self.world.handle_double_click();
                                    self.last_click_time = now - Duration::from_secs(1);
                                } else {
                                    self.world.handle_single_click();
                                    self.world.start_drag(rel_x, rel_y);
                                    self.last_click_time = now;
                                }
                                let _ = self.event_tx.send(DesktopEvent::PetClicked { x: rel_x, y: rel_y });
                            }
                        }

                        (MouseButton::Left, ElementState::Released) => {
                            if self.world.state_kind == crate::physics::PetStateKind::DraggedByUser {
                                self.world.release_drag();
                                let _ = self.event_tx.send(DesktopEvent::PetThrown {
                                    vx: self.world.vx,
                                    vy: self.world.vy,
                                });
                            }
                        }

                        (MouseButton::Right, ElementState::Pressed) => {
                            self.world.toss_treat(rel_x, rel_y);
                        }

                        _ => {}
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.render_frame(&self.world, self.world.debug_draw_platforms);
                }
            }

            WindowEvent::CloseRequested => {
                info!("Desktop Pet engine exiting...");
                unsafe { timeEndPeriod(1) };
                std::process::exit(0);
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);

        // Run buttery-smooth 60Hz physics, animation, and dynamic hit-testing loop
        if dt >= 1.0 / 60.0 {
            self.last_tick = now;

            // 1. Process agent IPC commands
            self.process_ipc_commands();

            // 2. Window scanner & Virtual Desktop Traversal check
            if now.duration_since(self.last_scan).as_millis() >= 100 {
                self.last_scan = now;
                let scanned = self.scanner.scan_windows();

                // Check if user swiped to a different Windows 11 Virtual Desktop!
                if self.scanner.check_virtual_desktop_switch(&scanned) {
                    info!("Windows 11 Virtual Desktop switch detected! Pet traversing to new desktop...");
                    self.world.trigger_desktop_traversal();
                }

                let fg_title = WindowScanner::get_foreground_window_title();
                if fg_title != self.last_foreground_title && !fg_title.is_empty() {
                    self.last_foreground_title = fg_title.clone();
                    let active_hwnd = scanned.iter().find(|w| w.is_active).map(|w| w.hwnd).unwrap_or(0);
                    let _ = self.event_tx.send(DesktopEvent::WindowFocused {
                        hwnd: active_hwnd,
                        title: fg_title,
                    });
                }

                let idle_ms = WindowScanner::check_user_idle_ms();
                self.world.handle_user_idle(idle_ms);

                self.world.update_platforms(scanned);
            }

            // 3. Advance physics (includes instant 60Hz window-riding!)
            let prev_standing = self.world.standing_on;
            self.world.tick(dt);

            if self.world.standing_on.is_some() && self.world.standing_on != prev_standing {
                let hwnd = self.world.standing_on.unwrap();
                let title = self.world.scanned_windows.iter().find(|w| w.hwnd == hwnd)
                    .map(|w| w.title.clone())
                    .unwrap_or_else(|| "Desktop Floor".to_string());
                let _ = self.event_tx.send(DesktopEvent::PetLandedOnWindow { hwnd, title });
            }

            // 4. Update dynamic hit-testing & petting detection
            self.update_dynamic_hit_testing();

            // 5. Broadcast snapshot to REST/WebSocket
            self.broadcast_state_snapshot();

            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        // Sub-millisecond timer resolution enabled by timeBeginPeriod(1)
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));
    }
}

fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Windows 11 Desktop Pet Interactivity Engine...");

    // Bump Windows scheduler resolution from default 15.6ms to 1ms for rock-solid 60 FPS
    unsafe { timeBeginPeriod(1) };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");

    let (cmd_tx, cmd_rx) = mpsc::channel::<PetCommand>(64);
    let (state_tx, state_rx) = watch::channel::<DesktopSnapshot>(DesktopSnapshot {
        pet_x: 500.0,
        pet_y: 200.0,
        pet_vx: 0.0,
        pet_vy: 0.0,
        state: "Falling".to_string(),
        standing_on_title: None,
        foreground_window_title: String::new(),
        user_idle_ms: 0,
        windows: Vec::new(),
        active_speech: None,
        debug_platforms_enabled: false,
    });
    let (event_tx, _event_rx) = broadcast::channel::<DesktopEvent>(128);

    let event_tx_clone = event_tx.clone();
    rt.spawn(async move {
        start_axum_server(cmd_tx, state_rx, event_tx_clone);
    });

    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            error!("Failed to create Winit event loop: {e}");
            unsafe { timeEndPeriod(1) };
            return;
        }
    };

    let mut app = DesktopPetApp::new(cmd_rx, state_tx, event_tx);
    if let Err(e) = event_loop.run_app(&mut app) {
        error!("Event loop error: {e}");
    }

    unsafe { timeEndPeriod(1) };
}
