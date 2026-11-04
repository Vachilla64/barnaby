use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::WebviewWindow;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClickRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ClickRegion {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < (self.x + self.width) && py >= self.y && py < (self.y + self.height)
    }
}

pub struct ClickthroughManager {
    regions: Arc<Mutex<Vec<ClickRegion>>>,
    generation: Arc<AtomicU64>,
    is_dragging: Arc<AtomicBool>,
    is_interactive: Arc<AtomicBool>,
}

impl Default for ClickthroughManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickthroughManager {
    pub fn new() -> Self {
        Self {
            regions: Arc::new(Mutex::new(Vec::new())),
            generation: Arc::new(AtomicU64::new(0)),
            is_dragging: Arc::new(AtomicBool::new(false)),
            is_interactive: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_regions(&self, regions: Vec<ClickRegion>) {
        if let Ok(mut lock) = self.regions.lock() {
            *lock = regions;
        }
    }

    pub fn set_dragging(&self, dragging: bool, window: Option<&WebviewWindow>) {
        self.is_dragging.store(dragging, Ordering::SeqCst);
        if dragging {
            if let Some(w) = window {
                let _ = w.set_ignore_cursor_events(false);
                self.is_interactive.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn start_watcher(&self, window: WebviewWindow) {
        let current_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_tracker = self.generation.clone();
        let regions_ref = self.regions.clone();
        let dragging_ref = self.is_dragging.clone();
        let interactive_ref = self.is_interactive.clone();

        // Start by ignoring cursor events (click-through by default)
        let _ = window.set_ignore_cursor_events(true);
        interactive_ref.store(false, Ordering::SeqCst);

        std::thread::spawn(move || {
            tracing::info!(
                "Cursor click-through watcher thread started (gen {})",
                current_gen
            );

            while gen_tracker.load(Ordering::SeqCst) == current_gen {
                let mut pt = POINT::default();
                let success = unsafe { GetCursorPos(&mut pt).is_ok() };

                if success {
                    let dragging = dragging_ref.load(Ordering::SeqCst);
                    let mut inside = dragging;

                    if !inside {
                        if let Ok(guard) = regions_ref.lock() {
                            inside = guard.iter().any(|r| r.contains(pt.x, pt.y));
                        }
                    }

                    let was_interactive = interactive_ref.load(Ordering::SeqCst);

                    if inside && !was_interactive {
                        let _ = window.set_ignore_cursor_events(false);
                        interactive_ref.store(true, Ordering::SeqCst);
                    } else if !inside && was_interactive {
                        let _ = window.set_ignore_cursor_events(true);
                        interactive_ref.store(false, Ordering::SeqCst);
                    }
                }

                std::thread::sleep(Duration::from_millis(8));
            }

            // PLAYBOOK lesson: always clear click-through when leaving
            let _ = window.set_ignore_cursor_events(false);
            tracing::info!(
                "Cursor watcher exiting (gen {}) - reset click-through",
                current_gen
            );
        });
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_region_contains() {
        let region = ClickRegion {
            x: 100,
            y: 200,
            width: 50,
            height: 50,
        };

        // Inside
        assert!(region.contains(100, 200)); // top-left
        assert!(region.contains(125, 225)); // center
        assert!(region.contains(149, 249)); // bottom-right inside

        // Outside
        assert!(!region.contains(99, 200)); // left
        assert!(!region.contains(100, 199)); // above
        assert!(!region.contains(150, 225)); // right boundary
        assert!(!region.contains(125, 250)); // bottom boundary
        assert!(!region.contains(0, 0));
    }
}
