#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::time::Instant;
use crate::win32_scanner::ScannedWindow;

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
    DraggedByUser { grab_offset_x: f32, grab_offset_y: f32 },
    PeekingBehindWindow { hwnd: isize, timer: f32 },
    PushingWindow { hwnd: isize, dir: f32, timer: f32 },
}

#[derive(Debug, Clone)]
pub struct SpeechBubble {
    pub text: String,
    pub expires_at: Instant,
}

/// The complete simulated world and pet entity state
pub struct WorldState {
    // Spatial dimensions
    pub screen_width: f32,
    pub screen_height: f32,

    // Pet position & physics
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub facing_right: bool,

    // State machine
    pub state_kind: PetStateKind,
    pub action: PetAction,
    pub anim_timer: f32,

    // Window-riding state
    pub standing_on: Option<isize>,
    pub last_standing_window_pos: Option<(i32, i32)>,

    // Mouse drag velocity estimation
    pub drag_history: Vec<(f32, f32, Instant)>,

    // Speech and debug state
    pub active_speech: Option<SpeechBubble>,
    pub debug_draw_platforms: bool,

    // Current navigation target
    pub nav_target_x: Option<f32>,

    // Platforms and windows cached for collision & rendering
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
            state_kind: PetStateKind::Falling,
            action: PetAction::Falling,
            anim_timer: 0.0,
            standing_on: None,
            last_standing_window_pos: None,
            drag_history: Vec::with_capacity(8),
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
    /// Uses 1D interval subtraction across Z-order to eliminate regions occluded by higher windows.
    pub fn update_platforms(&mut self, windows: Vec<ScannedWindow>) {
        let mut platforms = Vec::new();

        // 1. Process application windows in top-to-bottom Z-order
        for i in 0..windows.len() {
            let win = &windows[i];
            let top_y = win.bounds.top as f32;

            // Start with initial top edge span [left, right]
            let mut intervals: Vec<(f32, f32)> = vec![(win.bounds.left as f32, win.bounds.right as f32)];

            // Subtract occlusion from every window j higher in the Z-order (j < i)
            for j in 0..i {
                let higher = &windows[j];
                // Check if the higher window vertically covers the top edge of window i
                if higher.bounds.top <= win.bounds.top && higher.bounds.bottom >= win.bounds.top {
                    let occ_left = higher.bounds.left as f32;
                    let occ_right = higher.bounds.right as f32;

                    let mut next_intervals = Vec::with_capacity(intervals.len() * 2);
                    for (seg_left, seg_right) in intervals {
                        // Case A: Higher window completely covers segment -> eliminate
                        if occ_left <= seg_left && occ_right >= seg_right {
                            continue;
                        }
                        // Case B: Higher window punches hole in middle -> split into two
                        if occ_left > seg_left && occ_right < seg_right {
                            next_intervals.push((seg_left, occ_left));
                            next_intervals.push((occ_right, seg_right));
                            continue;
                        }
                        // Case C: Overlaps left portion
                        if occ_left <= seg_left && occ_right > seg_left && occ_right < seg_right {
                            next_intervals.push((occ_right, seg_right));
                            continue;
                        }
                        // Case D: Overlaps right portion
                        if occ_left > seg_left && occ_left < seg_right && occ_right >= seg_right {
                            next_intervals.push((seg_left, occ_left));
                            continue;
                        }
                        // No overlap
                        next_intervals.push((seg_left, seg_right));
                    }
                    intervals = next_intervals;
                }
            }

            // Keep segments with walkable width >= 24px
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

        // 2. Add fallback ground platform at bottom of primary display (or taskbar top edge)
        let ground_y = self.screen_height - 40.0;
        platforms.push(PlatformSegment {
            x1: 0.0,
            x2: self.screen_width,
            y: ground_y,
            hwnd: 0,
            title: "Ground / Desktop Bottom".to_string(),
        });

        self.platforms = platforms;
        self.scanned_windows = windows;
    }

    /// Advance physics, collision, and animation state by `dt` seconds (e.g. 1/60s).
    pub fn tick(&mut self, dt: f32) {
        self.anim_timer += dt;

        // Clean up expired speech bubbles
        if let Some(speech) = &self.active_speech {
            if Instant::now() > speech.expires_at {
                self.active_speech = None;
            }
        }

        // 1. Handle Active Dragging by User
        if let PetAction::DraggedByUser { grab_offset_x, grab_offset_y } = self.action {
            if let Some((cur_x, cur_y)) = crate::win32_scanner::WindowScanner::get_cursor_pos() {
                let target_x = cur_x as f32 - grab_offset_x;
                let target_y = cur_y as f32 - grab_offset_y;

                let now = Instant::now();
                self.drag_history.push((target_x, target_y, now));
                // Keep only last 8 samples within 150ms
                self.drag_history.retain(|(_, _, t)| now.duration_since(*t).as_millis() < 150);

                self.x = target_x;
                self.y = target_y;
                self.vx = 0.0;
                self.vy = 0.0;
                self.standing_on = None;
                self.last_standing_window_pos = None;
                self.state_kind = PetStateKind::DraggedByUser;
            }
            return;
        }

        // 2. Handle Riding Moving Windows
        if let Some(hwnd) = self.standing_on {
            if hwnd != 0 {
                // Find standing window in scanned windows
                if let Some(win) = self.scanned_windows.iter().find(|w| w.hwnd == hwnd) {
                    let cur_pos = (win.bounds.left, win.bounds.top);
                    if let Some(old_pos) = self.last_standing_window_pos {
                        let dx = (cur_pos.0 - old_pos.0) as f32;
                        let dy = (cur_pos.1 - old_pos.1) as f32;

                        if dx.abs() > 0.0 || dy.abs() > 0.0 {
                            // Ride the moving window smoothly
                            self.x += dx;
                            self.y += dy;
                        }
                    }
                    self.last_standing_window_pos = Some(cur_pos);

                    // Check if pet walked off or was shifted off the platform
                    let feet_x = self.x + PET_WIDTH / 2.0;
                    let is_still_on_platform = self.platforms.iter().any(|p| {
                        p.hwnd == hwnd && feet_x >= p.x1 - 4.0 && feet_x <= p.x2 + 4.0 && (self.y + PET_HEIGHT - p.y).abs() < 12.0
                    });

                    if !is_still_on_platform {
                        self.standing_on = None;
                        self.last_standing_window_pos = None;
                        self.transition_to_falling();
                    }
                } else {
                    // Window was closed or minimized! Transition to falling
                    self.standing_on = None;
                    self.last_standing_window_pos = None;
                    self.transition_to_falling();
                }
            }
        }

        // 3. State Machine Logic & AI Behaviors
        match self.action {
            PetAction::Idle { mut timer } => {
                self.vx = 0.0;
                timer -= dt;
                if timer <= 0.0 {
                    // Decide next action: 50% Walk, 30% Sit, 20% Stay Idle
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
                timer -= dt;
                if timer <= 0.0 {
                    self.action = PetAction::Idle { timer: 2.5 };
                    self.state_kind = PetStateKind::Idle;
                } else {
                    self.action = PetAction::Sitting { timer };
                }
            }

            PetAction::Walking { mut dir, mut timer } => {
                // If navigating to a specific target, steer towards it
                if let Some(target_x) = self.nav_target_x {
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

                // Edge patrol: check if pet is nearing the edge of the current platform
                let feet_x = self.x + PET_WIDTH / 2.0;
                let lookahead_x = feet_x + dir * (PET_WIDTH / 2.0 + 8.0);
                let on_edge = !self.platforms.iter().any(|p| {
                    p.hwnd == self.standing_on.unwrap_or(0)
                        && lookahead_x >= p.x1
                        && lookahead_x <= p.x2
                });

                if on_edge && self.nav_target_x.is_none() {
                    // Turn around so pet doesn't fall accidentally
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
                    self.vy = 900.0; // Terminal velocity
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
                // Micro nudge the target window
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

        // Screen boundary clamp horizontally
        if self.x < 0.0 {
            self.x = 0.0;
            self.vx = 0.0;
        } else if self.x + PET_WIDTH > self.screen_width {
            self.x = self.screen_width - PET_WIDTH;
            self.vx = 0.0;
        }

        // 5. Platform Landing & Ground Collision
        // When moving downward (vy >= 0), check if feet crossed any exposed platform segment
        let prev_feet_y = prev_y + PET_HEIGHT;
        let new_feet_y = self.y + PET_HEIGHT;
        let feet_x = self.x + PET_WIDTH / 2.0;

        if self.vy >= 0.0 {
            let mut landed_platform = None;

            for platform in &self.platforms {
                // Must be horizontally within the platform span (with 6px grace margin)
                if feet_x >= platform.x1 - 6.0 && feet_x <= platform.x2 + 6.0 {
                    // Check if feet crossed the platform plane from above
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

                if let Some(win) = self.scanned_windows.iter().find(|w| w.hwnd == p.hwnd) {
                    self.last_standing_window_pos = Some((win.bounds.left, win.bounds.top));
                }

                if self.state_kind == PetStateKind::Falling || self.state_kind == PetStateKind::Jumping {
                    self.action = PetAction::Idle { timer: 1.5 };
                    self.state_kind = PetStateKind::Idle;
                }
            } else if self.standing_on.is_none() && self.state_kind != PetStateKind::Jumping {
                self.transition_to_falling();
            }
        }
    }

    pub fn transition_to_falling(&mut self) {
        if self.state_kind != PetStateKind::Falling && self.state_kind != PetStateKind::DraggedByUser {
            self.state_kind = PetStateKind::Falling;
            self.action = PetAction::Falling;
            self.standing_on = None;
            self.last_standing_window_pos = None;
        }
    }

    /// User starts dragging the pet
    pub fn start_drag(&mut self, cursor_x: f32, cursor_y: f32) {
        let grab_offset_x = cursor_x - self.x;
        let grab_offset_y = cursor_y - self.y;
        self.action = PetAction::DraggedByUser { grab_offset_x, grab_offset_y };
        self.state_kind = PetStateKind::DraggedByUser;
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.drag_history.clear();
        self.drag_history.push((self.x, self.y, Instant::now()));
    }

    /// User releases the pet; calculate release velocity and throw!
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

            // Clamp maximum throw speed for controllable physics
            let max_speed: f32 = 1200.0;
            self.vx = throw_vx.clamp(-max_speed, max_speed);
            self.vy = throw_vy.clamp(-max_speed, max_speed);

            self.state_kind = PetStateKind::Falling;
            self.action = PetAction::Falling;
            self.drag_history.clear();
        }
    }

    /// Jump directly towards a point (x, y)
    pub fn jump_to_point(&mut self, target_x: f32, target_y: f32) {
        let dx = target_x - (self.x + PET_WIDTH / 2.0);
        let dy = target_y - (self.y + PET_HEIGHT);

        // Approximate ballistic arc
        self.vy = JUMP_VELOCITY.min(-150.0 + dy * 0.5);
        self.vx = (dx * 1.5).clamp(-500.0, 500.0);
        self.facing_right = dx > 0.0;
        self.standing_on = None;
        self.last_standing_window_pos = None;
        self.state_kind = PetStateKind::Jumping;
        self.action = PetAction::Jumping;
    }

    /// Walk to a specific window matching title substring
    pub fn walk_to_window(&mut self, title_contains: &str) -> bool {
        let title_lower = title_contains.to_lowercase();
        if let Some(target_win) = self.scanned_windows.iter().find(|w| w.title.to_lowercase().contains(&title_lower)) {
            let target_center_x = (target_win.bounds.left + target_win.bounds.right) as f32 / 2.0;
            let target_top_y = target_win.bounds.top as f32;

            if (self.y + PET_HEIGHT - target_top_y).abs() < 50.0 {
                // Same vertical layer: walk
                self.nav_target_x = Some(target_center_x);
                let dir = if target_center_x > self.x { 1.0 } else { -1.0 };
                self.facing_right = dir > 0.0;
                self.action = PetAction::Walking { dir, timer: 10.0 };
                self.state_kind = PetStateKind::Walking;
            } else {
                // Different height: jump towards window top edge!
                self.jump_to_point(target_center_x, target_top_y);
            }
            true
        } else {
            false
        }
    }

    /// Speak a message in a speech bubble
    pub fn say(&mut self, text: String, duration_ms: u64) {
        self.active_speech = Some(SpeechBubble {
            text,
            expires_at: Instant::now() + std::time::Duration::from_millis(duration_ms),
        });
    }

    /// Check if cursor intersects the pet's interactive bounding box
    pub fn hit_test(&self, cursor_x: f32, cursor_y: f32) -> bool {
        // Generous clickable bounding box around pet
        let margin = 6.0;
        cursor_x >= self.x - margin
            && cursor_x <= self.x + PET_WIDTH + margin
            && cursor_y >= self.y - margin
            && cursor_y <= self.y + PET_HEIGHT + margin
    }
}
