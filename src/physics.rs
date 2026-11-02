#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::time::Instant;
use crate::win32_scanner::{ScannedWindow, WindowScanner};

pub const PET_WIDTH: f32 = 48.0;
pub const PET_HEIGHT: f32 = 48.0;
pub const GRAVITY: f32 = 980.0; // px / s^2
pub const WALK_SPEED: f32 = 120.0; // px / s
pub const JUMP_VELOCITY: f32 = -420.0; // px / s

/// A horizontal top-edge platform segment that the pet can walk or stand on.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformSegment {
    pub x1: f32,
    pub x2: f32,
    pub y: f32,
    pub hwnd: isize,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PetStateKind {
    Idle,
    Walking,
    Jumping,
    Falling,
    Sitting,
    Sleeping,
    Petted,
    Backflipping,
    Eating,
    DraggedByUser,
    PeekingBehindWindow,
    PushingWindow,
}

#[derive(Debug, Clone)]
pub enum PetAction {
    Idle { timer: f32 },
    Walking { dir: f32, timer: f32 },
    Jumping,
    Falling,
    Sitting { timer: f32 },
    Sleeping,
    Petted { timer: f32 },
    Backflipping { progress: f32 },
    Eating { timer: f32 },
    PeekingFromScreenEdge { timer: f32 },
    DraggedByUser { grab_offset_x: f32, grab_offset_y: f32 },
    PeekingBehindWindow { hwnd: isize, timer: f32 },
    PushingWindow { hwnd: isize, dir: f32, timer: f32 },
}

#[derive(Debug, Clone)]
pub struct SpeechBubble {
    pub text: String,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
pub enum ParticleType {
    Heart,
    Sparkle { r: u8, g: u8, b: u8 },
    Zzz,
    Treat,
}

#[derive(Debug, Clone)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub size: f32,
    pub particle_type: ParticleType,
}

/// The complete simulated world and pet entity state
pub struct WorldState {
    pub screen_width: f32,
    pub screen_height: f32,

    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub facing_right: bool,
    pub rotation_rad: f32,

    pub state_kind: PetStateKind,
    pub action: PetAction,
    pub anim_timer: f32,

    pub standing_on: Option<isize>,
    pub last_standing_window_pos: Option<(i32, i32)>,

    pub cursor_pos: Option<(f32, f32)>,
    pub petting_meter: f32,
    pub last_cursor_pos: Option<(f32, f32)>,

    pub drag_history: Vec<(f32, f32, Instant)>,
    pub particles: Vec<Particle>,
    pub active_treat: Option<(f32, f32)>,

    pub active_speech: Option<SpeechBubble>,
    pub debug_draw_platforms: bool,
    pub nav_target_x: Option<f32>,

    pub platforms: Vec<PlatformSegment>,
    pub scanned_windows: Vec<ScannedWindow>,
}

impl WorldState {
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            screen_width,
            screen_height,
            x: screen_width / 2.0 - PET_WIDTH / 2.0,
            y: 100.0,
            vx: 0.0,
            vy: 0.0,
            facing_right: true,
            rotation_rad: 0.0,
            state_kind: PetStateKind::Falling,
            action: PetAction::Falling,
            anim_timer: 0.0,
            standing_on: None,
            last_standing_window_pos: None,
            cursor_pos: None,
            petting_meter: 0.0,
            last_cursor_pos: None,
            drag_history: Vec::with_capacity(8),
            particles: Vec::with_capacity(64),
            active_treat: None,
            active_speech: Some(SpeechBubble {
                text: "Hello! Desktop pet ready.".to_string(),
                expires_at: Instant::now() + std::time::Duration::from_secs(4),
            }),
            debug_draw_platforms: false,
            nav_target_x: None,
            platforms: Vec::new(),
            scanned_windows: Vec::new(),
        }
    }

    /// Extract exposed horizontal top-edge platform segments from scanned windows.
    pub fn update_platforms(&mut self, windows: Vec<ScannedWindow>) {
        let mut platforms = Vec::new();

        for i in 0..windows.len() {
            let win = &windows[i];

            if win.is_fullscreen {
                continue;
            }

            let top_y = win.bounds.top as f32;
            let mut intervals: Vec<(f32, f32)> = vec![(win.bounds.left as f32, win.bounds.right as f32)];

            for j in 0..i {
                let higher = &windows[j];
                if higher.bounds.top <= win.bounds.top && higher.bounds.bottom >= win.bounds.top {
                    let occ_left = higher.bounds.left as f32;
                    let occ_right = higher.bounds.right as f32;

                    let mut next_intervals = Vec::with_capacity(intervals.len() * 2);
                    for (seg_left, seg_right) in intervals {
                        if occ_left <= seg_left && occ_right >= seg_right {
                            continue;
                        }
                        if occ_left > seg_left && occ_right < seg_right {
                            next_intervals.push((seg_left, occ_left));
                            next_intervals.push((occ_right, seg_right));
                            continue;
                        }
                        if occ_left <= seg_left && occ_right > seg_left && occ_right < seg_right {
                            next_intervals.push((occ_right, seg_right));
                            continue;
                        }
                        if occ_left > seg_left && occ_left < seg_right && occ_right >= seg_right {
                            next_intervals.push((seg_left, occ_left));
                            continue;
                        }
                        next_intervals.push((seg_left, seg_right));
                    }
                    intervals = next_intervals;
                }
            }

            for (seg_x1, seg_x2) in intervals {
                if seg_x2 - seg_x1 >= 24.0 {
                    platforms.push(PlatformSegment {
                        x1: seg_x1,
                        x2: seg_x2,
                        y: top_y,
                        hwnd: win.hwnd,
                        title: win.title.clone(),
                    });
                }
            }
        }

        // True screen floor
        platforms.push(PlatformSegment {
            x1: 0.0,
            x2: self.screen_width,
            y: self.screen_height,
            hwnd: 0,
            title: "Ground / Screen Floor".to_string(),
        });

        self.platforms = platforms;
        self.scanned_windows = windows;
    }

    /// Triggers the desktop traversal sequence: peeks from the left edge and leaps into the new desktop!
    pub fn trigger_desktop_traversal(&mut self) {
        self.wake_up();
        // Place pet poking out from the left screen bezel
        self.x = -PET_WIDTH * 0.65;
        self.y = (self.screen_height * 0.62).clamp(150.0, self.screen_height - 120.0);
        self.vx = 0.0;
        self.vy = 0.0;
        self.rotation_rad = 0.0;
        self.facing_right = true;
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.state_kind = PetStateKind::PeekingBehindWindow;
        self.action = PetAction::PeekingFromScreenEdge { timer: 0.85 };
        self.say("Wait for me! *pokes out* 🐾".to_string(), 2500);
        self.spawn_sparkle_burst(0.0, self.y + PET_HEIGHT / 2.0);
    }

    /// Advance physics, collision, particles, and animation state by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.anim_timer += dt;

        if let Some(speech) = &self.active_speech {
            if Instant::now() > speech.expires_at {
                self.active_speech = None;
            }
        }

        self.tick_particles(dt);
        self.petting_meter = (self.petting_meter - dt * 2.0).max(0.0);

        // 1. Handle Active Dragging by User
        if let PetAction::DraggedByUser { grab_offset_x, grab_offset_y } = self.action {
            if let Some((cur_x, cur_y)) = self.cursor_pos {
                let target_x = cur_x - grab_offset_x;
                let target_y = cur_y - grab_offset_y;

                let now = Instant::now();
                self.drag_history.push((target_x, target_y, now));
                self.drag_history.retain(|(_, _, t)| now.duration_since(*t).as_millis() < 150);

                self.x = target_x;
                self.y = target_y;
                self.vx = 0.0;
                self.vy = 0.0;
                self.rotation_rad = 0.0;
                self.standing_on = None;
                self.last_standing_window_pos = None;
                self.state_kind = PetStateKind::DraggedByUser;
            }
            return;
        }

        // 2. High-Frequency 60Hz Window Riding (smooth real-time tracking)
        if let Some(hwnd) = self.standing_on {
            if hwnd != 0 {
                if let Some(bounds) = WindowScanner::get_window_bounds_fast(hwnd) {
                    let cur_pos = (bounds.left, bounds.top);
                    if let Some(old_pos) = self.last_standing_window_pos {
                        let dx = (cur_pos.0 - old_pos.0) as f32;
                        let dy = (cur_pos.1 - old_pos.1) as f32;

                        if dx.abs() > 0.0 || dy.abs() > 0.0 {
                            self.x += dx;
                            self.y += dy;
                        }
                    }
                    self.last_standing_window_pos = Some(cur_pos);

                    // Check if pet walked off the platform
                    let feet_x = self.x + PET_WIDTH / 2.0;
                    let feet_y = self.y + PET_HEIGHT;
                    if feet_x < (bounds.left as f32 - 12.0)
                        || feet_x > (bounds.right as f32 + 12.0)
                        || (feet_y - bounds.top as f32).abs() > 16.0
                    {
                        self.standing_on = None;
                        self.last_standing_window_pos = None;
                        self.transition_to_falling();
                    }
                } else {
                    // Window was closed, minimized, or moved to another virtual desktop
                    self.standing_on = None;
                    self.last_standing_window_pos = None;
                    self.transition_to_falling();
                }
            }
        }

        // 3. State Machine Logic & AI Behaviors
        match self.action {
            PetAction::PeekingFromScreenEdge { mut timer } => {
                self.vx = 0.0;
                self.vy = 0.0;
                self.rotation_rad = 0.0;
                timer -= dt;

                if timer <= 0.0 {
                    // Leap from edge into the new desktop!
                    self.vx = 340.0;
                    self.vy = -440.0;
                    self.state_kind = PetStateKind::Backflipping;
                    self.action = PetAction::Backflipping { progress: 0.0 };
                    self.spawn_sparkle_burst(self.x + PET_WIDTH, self.y + PET_HEIGHT / 2.0);
                    self.say("Here I am! ✨".to_string(), 2500);
                } else {
                    self.action = PetAction::PeekingFromScreenEdge { timer };
                }
            }

            PetAction::Idle { mut timer } => {
                self.vx = 0.0;
                self.rotation_rad = 0.0;
                timer -= dt;

                if let Some((tx, _ty)) = self.active_treat {
                    let diff = tx - (self.x + PET_WIDTH / 2.0);
                    let dir = if diff > 0.0 { 1.0 } else { -1.0 };
                    self.action = PetAction::Walking { dir, timer: 5.0 };
                    self.state_kind = PetStateKind::Walking;
                    return;
                }

                if timer <= 0.0 {
                    let dice = (self.anim_timer * 17.0).fract();
                    if dice < 0.5 {
                        let dir = if (dice * 10.0).fract() > 0.5 { 1.0 } else { -1.0 };
                        self.facing_right = dir > 0.0;
                        self.action = PetAction::Walking { dir, timer: 3.0 + dice * 4.0 };
                        self.state_kind = PetStateKind::Walking;
                    } else if dice < 0.8 {
                        self.action = PetAction::Sitting { timer: 4.0 + dice * 3.0 };
                        self.state_kind = PetStateKind::Sitting;
                    } else {
                        self.action = PetAction::Idle { timer: 2.0 + dice * 2.0 };
                        self.state_kind = PetStateKind::Idle;
                    }
                } else {
                    self.action = PetAction::Idle { timer };
                }
            }

            PetAction::Sitting { mut timer } => {
                self.vx = 0.0;
                self.rotation_rad = 0.0;
                timer -= dt;
                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.5 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::Sitting { timer };
                }
            }

            PetAction::Sleeping => {
                self.vx = 0.0;
                self.rotation_rad = 0.0;
                if (self.anim_timer * 1.5).fract() < dt * 1.5 {
                    self.spawn_zzz();
                }
            }

            PetAction::Petted { mut timer } => {
                self.vx = 0.0;
                timer -= dt;
                if (self.anim_timer * 8.0).fract() < dt * 8.0 {
                    self.spawn_heart();
                }
                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.0 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::Petted { timer };
                }
            }

            PetAction::Backflipping { mut progress } => {
                progress += dt * 3.5;
                self.rotation_rad = progress * std::f32::consts::TAU;
                if progress >= 1.0 {
                    self.rotation_rad = 0.0;
                    self.action = PetAction::Falling;
                    self.state_kind = PetStateKind::Falling;
                } else {
                    self.action = PetAction::Backflipping { progress };
                }
            }

            PetAction::Eating { mut timer } => {
                self.vx = 0.0;
                timer -= dt;
                if timer <= 0.0 {
                    self.active_treat = None;
                    self.say("Yummy! Loved that! ❤️".to_string(), 2500);
                    self.action = PetAction::Idle { timer: 2.0 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::Eating { timer };
                }
            }

            PetAction::Walking { mut dir, mut timer } => {
                self.rotation_rad = 0.0;

                if let Some((tx, _ty)) = self.active_treat {
                    let diff = tx - (self.x + PET_WIDTH / 2.0);
                    if diff.abs() < 12.0 {
                        self.action = PetAction::Eating { timer: 1.5 };
                        self.state_kind = PetStateKind::Eating;
                        self.spawn_sparkle_burst(self.x + PET_WIDTH / 2.0, self.y + PET_HEIGHT / 2.0);
                        return;
                    }
                    dir = if diff > 0.0 { 1.0 } else { -1.0 };
                } else if let Some(target_x) = self.nav_target_x {
                    let diff = target_x - (self.x + PET_WIDTH / 2.0);
                    if diff.abs() < 8.0 {
                        self.nav_target_x = None;
                        self.action = PetAction::Idle { timer: 3.0 };
                        self.state_kind = PetStateKind::Idle;
                        dir = 0.0;
                    } else {
                        dir = if diff > 0.0 { 1.0 } else { -1.0 };
                    }
                }

                self.vx = dir * WALK_SPEED;
                self.facing_right = dir > 0.0;
                timer -= dt;

                let feet_x = self.x + PET_WIDTH / 2.0;
                let lookahead_x = feet_x + dir * (PET_WIDTH / 2.0 + 8.0);
                let on_edge = !self.platforms.iter().any(|p| {
                    p.hwnd == self.standing_on.unwrap_or(0)
                        && lookahead_x >= p.x1
                        && lookahead_x <= p.x2
                });

                if on_edge && self.nav_target_x.is_none() && self.active_treat.is_none() {
                    dir = -dir;
                    self.facing_right = dir > 0.0;
                }

                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.0 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::Walking { dir, timer };
                }
            }

            PetAction::Jumping => {
                self.vy += GRAVITY * dt;
                if self.vy > 0.0 {
                    self.action = PetAction::Falling;
                    self.state_kind = PetStateKind::Falling;
                }
            }

            PetAction::Falling => {
                self.vy += GRAVITY * dt;
                if self.vy > 900.0 {
                    self.vy = 900.0;
                }
            }

            PetAction::PeekingBehindWindow { hwnd, mut timer } => {
                timer -= dt;
                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.0 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::PeekingBehindWindow { hwnd, timer };
                }
            }

            PetAction::PushingWindow { hwnd, dir, mut timer } => {
                timer -= dt;
                if (timer * 20.0).fract() < 0.2 {
                    crate::win32_scanner::WindowScanner::nudge_window(hwnd, (dir * 2.0) as i32, 0);
                }
                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.0 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::PushingWindow { hwnd, dir, timer };
                }
            }

            PetAction::DraggedByUser { .. } => {}
        }

        // 4. Update Position
        let _prev_x = self.x;
        let prev_y = self.y;

        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Keep pet within screen bounds (unless peeking from left edge)
        if !matches!(self.action, PetAction::PeekingFromScreenEdge { .. }) {
            if self.x < 0.0 {
                self.x = 0.0;
                self.vx = 0.0;
            } else if self.x + PET_WIDTH > self.screen_width {
                self.x = self.screen_width - PET_WIDTH;
                self.vx = 0.0;
            }
        }

        // 5. Platform Landing & Ground Collision
        let prev_feet_y = prev_y + PET_HEIGHT;
        let new_feet_y = self.y + PET_HEIGHT;
        let feet_x = self.x + PET_WIDTH / 2.0;

        if self.vy >= 0.0 && !matches!(self.action, PetAction::PeekingFromScreenEdge { .. }) {
            let mut landed_platform = None;

            for platform in &self.platforms {
                if feet_x >= platform.x1 - 6.0 && feet_x <= platform.x2 + 6.0 {
                    if prev_feet_y <= platform.y + 4.0 && new_feet_y >= platform.y - 4.0 {
                        landed_platform = Some(platform.clone());
                        break;
                    }
                }
            }

            if let Some(p) = landed_platform {
                self.y = p.y - PET_HEIGHT;
                self.vy = 0.0;
                self.standing_on = Some(p.hwnd);

                if let Some(bounds) = WindowScanner::get_window_bounds_fast(p.hwnd) {
                    self.last_standing_window_pos = Some((bounds.left, bounds.top));
                }

                if self.state_kind == PetStateKind::Falling || self.state_kind == PetStateKind::Jumping {
                    self.spawn_sparkle_burst(feet_x, self.y + PET_HEIGHT);
                    self.action = PetAction::Idle { timer: 1.5 };
                    self.state_kind = PetStateKind::Idle;
                }
            } else if self.standing_on.is_none() && self.state_kind != PetStateKind::Jumping && self.state_kind != PetStateKind::Backflipping {
                self.transition_to_falling();
            }
        }
    }

    fn tick_particles(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.lifetime -= dt;

            if matches!(p.particle_type, ParticleType::Treat) {
                p.vy += 600.0 * dt;
                if p.y >= self.screen_height - 10.0 {
                    p.y = self.screen_height - 10.0;
                    p.vy = 0.0;
                    p.vx = 0.0;
                }
            }
        }
        self.particles.retain(|p| p.lifetime > 0.0);
    }

    pub fn transition_to_falling(&mut self) {
        if self.state_kind != PetStateKind::Falling
            && self.state_kind != PetStateKind::DraggedByUser
            && self.state_kind != PetStateKind::Backflipping
            && !matches!(self.action, PetAction::PeekingFromScreenEdge { .. })
        {
            self.state_kind = PetStateKind::Falling;
            self.action = PetAction::Falling;
            self.standing_on = None;
            self.last_standing_window_pos = None;
        }
    }

    pub fn pet_by_cursor(&mut self, cursor_x: f32, cursor_y: f32) {
        if let Some((last_x, last_y)) = self.last_cursor_pos {
            let dist = ((cursor_x - last_x).powi(2) + (cursor_y - last_y).powi(2)).sqrt();
            if dist > 3.0 && dist < 40.0 {
                self.petting_meter += 0.35;
                if self.petting_meter > 1.2 && self.state_kind != PetStateKind::DraggedByUser {
                    self.state_kind = PetStateKind::Petted;
                    self.action = PetAction::Petted { timer: 1.5 };
                    if self.active_speech.is_none() {
                        let replies = ["Purrrrrr~ ❤️", "Hehe, that tickles! ✨", "So cozy~ 🥰", "Warm fuzzies! 💕"];
                        let idx = (self.anim_timer * 10.0) as usize % replies.len();
                        self.say(replies[idx].to_string(), 2500);
                    }
                }
            }
        }
        self.last_cursor_pos = Some((cursor_x, cursor_y));
    }

    pub fn handle_single_click(&mut self) {
        self.wake_up();
        self.vy = -180.0;
        self.spawn_sparkle_burst(self.x + PET_WIDTH / 2.0, self.y + PET_HEIGHT / 2.0);
        let replies = ["Boop! ✨", "Hello friend! 😊", "Ready to explore!", "Peekaboo! 👀"];
        let idx = (self.anim_timer * 7.0) as usize % replies.len();
        self.say(replies[idx].to_string(), 2200);
    }

    pub fn handle_double_click(&mut self) {
        self.wake_up();
        self.vy = -450.0;
        self.state_kind = PetStateKind::Backflipping;
        self.action = PetAction::Backflipping { progress: 0.0 };
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.spawn_sparkle_burst(self.x + PET_WIDTH / 2.0, self.y + PET_HEIGHT);
        self.say("Ta-da! Acrobatics! 🤸✨".to_string(), 2500);
    }

    pub fn toss_treat(&mut self, cursor_x: f32, cursor_y: f32) {
        self.wake_up();
        self.active_treat = Some((cursor_x, cursor_y));
        self.particles.push(Particle {
            x: cursor_x,
            y: cursor_y,
            vx: 0.0,
            vy: 50.0,
            lifetime: 10.0,
            max_lifetime: 10.0,
            size: 14.0,
            particle_type: ParticleType::Treat,
        });
        self.say("Ooh, a treat! Mine mine! 🍓".to_string(), 2500);
    }

    pub fn handle_user_idle(&mut self, idle_ms: u64) {
        if idle_ms > 60000 && self.state_kind == PetStateKind::Idle {
            self.state_kind = PetStateKind::Sleeping;
            self.action = PetAction::Sleeping;
        }
    }

    pub fn wake_up(&mut self) {
        if self.state_kind == PetStateKind::Sleeping {
            self.state_kind = PetStateKind::Idle;
            self.action = PetAction::Idle { timer: 2.0 };
            self.say("Yawn... I'm awake! ☀️".to_string(), 2500);
        }
    }

    pub fn spawn_heart(&mut self) {
        let center_x = self.x + PET_WIDTH / 2.0;
        let offset_x = ((self.anim_timer * 23.0).sin()) * 16.0;
        self.particles.push(Particle {
            x: center_x + offset_x,
            y: self.y - 5.0,
            vx: ((self.anim_timer * 13.0).cos()) * 20.0,
            vy: -45.0,
            lifetime: 1.2,
            max_lifetime: 1.2,
            size: 12.0,
            particle_type: ParticleType::Heart,
        });
    }

    pub fn spawn_zzz(&mut self) {
        let center_x = self.x + PET_WIDTH / 2.0 + 8.0;
        self.particles.push(Particle {
            x: center_x,
            y: self.y,
            vx: 15.0,
            vy: -25.0,
            lifetime: 2.0,
            max_lifetime: 2.0,
            size: 12.0,
            particle_type: ParticleType::Zzz,
        });
    }

    pub fn spawn_sparkle_burst(&mut self, x: f32, y: f32) {
        let colors = [
            (255, 230, 0),
            (0, 240, 255),
            (255, 80, 180),
            (100, 255, 120),
        ];

        for i in 0..8 {
            let angle = (i as f32 / 8.0) * std::f32::consts::TAU;
            let speed = 90.0 + (i as f32 * 12.0);
            let color = colors[i % colors.len()];

            self.particles.push(Particle {
                x,
                y,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                lifetime: 0.6,
                max_lifetime: 0.6,
                size: 6.0,
                particle_type: ParticleType::Sparkle { r: color.0, g: color.1, b: color.2 },
            });
        }
    }

    pub fn start_drag(&mut self, cursor_x: f32, cursor_y: f32) {
        self.wake_up();
        let grab_offset_x = cursor_x - self.x;
        let grab_offset_y = cursor_y - self.y;
        self.action = PetAction::DraggedByUser { grab_offset_x, grab_offset_y };
        self.state_kind = PetStateKind::DraggedByUser;
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.drag_history.clear();
        self.drag_history.push((self.x, self.y, Instant::now()));
    }

    pub fn release_drag(&mut self) {
        if let PetAction::DraggedByUser { .. } = self.action {
            let mut throw_vx = 0.0;
            let mut throw_vy = 0.0;

            if self.drag_history.len() >= 2 {
                let first = self.drag_history.first().unwrap();
                let last = self.drag_history.last().unwrap();
                let dt = last.2.duration_since(first.2).as_secs_f32();
                if dt > 0.01 && dt < 0.25 {
                    throw_vx = (last.0 - first.0) / dt;
                    throw_vy = (last.1 - first.1) / dt;
                }
            }

            let max_speed: f32 = 1200.0;
            self.vx = throw_vx.clamp(-max_speed, max_speed);
            self.vy = throw_vy.clamp(-max_speed, max_speed);

            self.state_kind = PetStateKind::Falling;
            self.action = PetAction::Falling;
            self.drag_history.clear();
        }
    }

    pub fn jump_to_point(&mut self, target_x: f32, target_y: f32) {
        self.wake_up();
        let dx = target_x - (self.x + PET_WIDTH / 2.0);
        let dy = target_y - (self.y + PET_HEIGHT);

        self.vy = JUMP_VELOCITY.min(-150.0 + dy * 0.5);
        self.vx = (dx * 1.5).clamp(-500.0, 500.0);
        self.facing_right = dx > 0.0;
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.state_kind = PetStateKind::Jumping;
        self.action = PetAction::Jumping;
    }

    pub fn walk_to_window(&mut self, title_contains: &str) -> bool {
        self.wake_up();
        let title_lower = title_contains.to_lowercase();
        if let Some(target_win) = self.scanned_windows.iter().find(|w| w.title.to_lowercase().contains(&title_lower)) {
            let target_center_x = (target_win.bounds.left + target_win.bounds.right) as f32 / 2.0;
            let target_top_y = target_win.bounds.top as f32;

            if (self.y + PET_HEIGHT - target_top_y).abs() < 50.0 {
                self.nav_target_x = Some(target_center_x);
                let dir = if target_center_x > self.x { 1.0 } else { -1.0 };
                self.facing_right = dir > 0.0;
                self.action = PetAction::Walking { dir, timer: 10.0 };
                self.state_kind = PetStateKind::Walking;
            } else {
                self.jump_to_point(target_center_x, target_top_y);
            }
            true
        } else {
            false
        }
    }

    pub fn say(&mut self, text: String, duration_ms: u64) {
        self.active_speech = Some(SpeechBubble {
            text,
            expires_at: Instant::now() + std::time::Duration::from_millis(duration_ms),
        });
    }

    pub fn hit_test(&self, cursor_x: f32, cursor_y: f32) -> bool {
        let margin = 6.0;
        cursor_x >= self.x - margin
            && cursor_x <= self.x + PET_WIDTH + margin
            && cursor_y >= self.y - margin
            && cursor_y <= self.y + PET_HEIGHT + margin
    }
}
