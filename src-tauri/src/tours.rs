//! Tour engine: walks the user through code, one step at a time, in step with speech.

use crate::vscode::VsCodeHub;
use crate::voice::queue::{Priority, SpeechKind, SpeechQueue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

pub const MAX_STEPS: usize = 12;
const SHOWN_TIMEOUT: Duration = Duration::from_secs(3);
const PAUSE_BETWEEN_STEPS: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TourStep {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub narration: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Next,
    Back,
    Stop,
    Pause,
    Resume,
}

impl Control {
    pub fn parse(action: &str) -> Option<Control> {
        match action {
            "next" => Some(Control::Next),
            "back" | "previous" => Some(Control::Back),
            "stop" => Some(Control::Stop),
            "pause" => Some(Control::Pause),
            "resume" | "continue" => Some(Control::Resume),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Move {
    Goto(usize),
    End,
}

/// Where the tour goes after a jump control (`Next` or `Back`) at `index` of `total` steps.
pub fn jump(index: usize, total: usize, action: Control) -> Move {
    match action {
        Control::Next if index + 1 < total => Move::Goto(index + 1),
        Control::Next | Control::Stop => Move::End,
        Control::Back => Move::Goto(index.saturating_sub(1)),
        Control::Pause | Control::Resume => Move::Goto(index),
    }
}

/// Resolve a tour path against the known workspace folders. Absolute paths pass through.
pub fn resolve_path(path: &str, roots: &[String]) -> Option<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    // Models sometimes prefix the project folder's own name ("demo-project/src/a.ts").
    // Try the path as given, then with that prefix removed.
    let normalized = path.replace('\\', "/");
    roots.iter().find_map(|r| {
        let root = Path::new(r);
        let direct = root.join(&normalized);
        if direct.exists() {
            return Some(direct);
        }
        let name = root.file_name()?.to_string_lossy().to_string();
        let stripped = normalized.strip_prefix(&format!("{name}/"))?;
        let candidate = root.join(stripped);
        candidate.exists().then_some(candidate)
    })
}

fn line_count(path: &Path) -> u32 {
    std::fs::read_to_string(path)
        .map(|t| t.lines().count().max(1) as u32)
        .unwrap_or(1)
}

/// Keep at most `MAX_STEPS` steps whose files exist, with line numbers clamped into range.
/// Paths become absolute.
pub fn validate_steps(steps: Vec<TourStep>, roots: &[String]) -> Vec<TourStep> {
    steps
        .into_iter()
        .filter_map(|s| {
            let abs = resolve_path(&s.path, roots)?;
            let lines = line_count(&abs);
            let start = s.start_line.clamp(1, lines);
            let end = s.end_line.clamp(start, lines);
            Some(TourStep {
                path: abs.components().collect::<PathBuf>().to_string_lossy().to_string(),
                start_line: start,
                end_line: end,
                narration: s.narration,
            })
        })
        .take(MAX_STEPS)
        .collect()
}

pub struct TourEngine {
    hub: Arc<VsCodeHub>,
    queue: Option<SpeechQueue>,
    app: Option<AppHandle>,
    pending: std::sync::Mutex<Option<(String, Vec<TourStep>)>>,
    control_tx: Mutex<Option<mpsc::Sender<Control>>>,
}

impl TourEngine {
    pub fn new(hub: Arc<VsCodeHub>, queue: Option<SpeechQueue>, app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Self {
            hub,
            queue,
            app,
            pending: std::sync::Mutex::new(None),
            control_tx: Mutex::new(None),
        })
    }

    pub async fn control(&self, action: Control) {
        if let Some(tx) = self.control_tx.lock().await.as_ref() {
            let _ = tx.send(action).await;
        }
    }

    /// Start a tour, replacing any tour that is running. Returns the number of steps.
    pub async fn start(self: &Arc<Self>, agent_kind: &str, title: &str, steps: Vec<TourStep>, colour: String) -> usize {
        let roots = self.hub.workspace_folders();
        let steps = validate_steps(steps, &roots);
        if steps.is_empty() {
            return 0;
        }
        let total = steps.len();
        let (tx, rx) = mpsc::channel(8);
        {
            let mut slot = self.control_tx.lock().await;
            if let Some(old) = slot.take() {
                let _ = old.send(Control::Stop).await;
            }
            *slot = Some(tx);
        }
        let engine = self.clone();
        let kind = agent_kind.to_string();
        let title = title.to_string();
        tokio::spawn(async move {
            engine.run(&kind, &title, steps, colour, rx).await;
        });
        total
    }

    async fn say(&self, kind: &str, text: &str) {
        if let Some(q) = &self.queue {
            q.enqueue(crate::voice::queue::SpeechRequest {
                agent_id: kind.to_string(),
                text: text.to_string(),
                priority: Priority::Normal,
                kind: SpeechKind::Tour,
                voice: Some(crate::server::voice_for(kind)),
            })
            .await;
        }
    }

    async fn is_busy(&self) -> bool {
        match &self.queue {
            Some(q) => q.is_busy().await,
            None => false,
        }
    }

    /// Hold a tour the brain proposed until the user says "Show me".
    pub fn set_pending(&self, title: &str, steps: Vec<TourStep>) {
        *self.pending.lock().unwrap() = Some((title.to_string(), steps));
    }

    pub async fn start_pending(self: &Arc<Self>, agent_kind: &str, colour: String) -> usize {
        let taken = self.pending.lock().unwrap().take();
        match taken {
            Some((title, steps)) => self.start(agent_kind, &title, steps, colour).await,
            None => 0,
        }
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, payload);
        }
    }

    async fn run(&self, kind: &str, title: &str, steps: Vec<TourStep>, colour: String, mut rx: mpsc::Receiver<Control>) {
        let tour_id = format!("tour-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
        let total = steps.len();
        let mut index = 0usize;
        tracing::info!("Tour '{title}' started: {total} steps (vscode connected: {})", self.hub.is_connected());

        'tour: loop {
            let step = &steps[index];
            let connected = self.hub.is_connected();
            if connected {
                self.hub.send(json!({
                    "type": "tour.step", "tour_id": tour_id, "index": index, "total": total,
                    "path": step.path, "start_line": step.start_line, "end_line": step.end_line, "colour": colour
                }));
            }
            self.emit("tour:step", json!({
                "agent_kind": kind, "tour_id": tour_id, "index": index, "total": total,
                "path": step.path, "start_line": step.start_line, "end_line": step.end_line,
                "narration": step.narration, "title": title, "vscode": connected
            }));

            // Wait for VS Code to show the step (or a control, or the timeout).
            if connected {
                let shown = self.hub.wait_shown();
                tokio::select! {
                    _ = shown => {}
                    _ = tokio::time::sleep(SHOWN_TIMEOUT) => {}
                    c = rx.recv() => {
                        match self.apply(c, index, total).await { Some(Move::Goto(i)) => { index = i; continue 'tour; } Some(Move::End) | None => break 'tour, }
                    }
                }
            }

            self.say(kind, &step.narration).await;

            // Wait for the speech to finish, then the gap, listening for controls throughout.
            let mut paused = false;
            let mut gap_left: Option<Duration> = None;
            loop {
                let tick = tokio::time::sleep(Duration::from_millis(100));
                tokio::select! {
                    c = rx.recv() => {
                        match c {
                            Some(Control::Pause) => { paused = true; }
                            Some(Control::Resume) => { paused = false; }
                            other => match self.apply(other, index, total).await {
                                Some(Move::Goto(i)) => { index = i; continue 'tour; }
                                Some(Move::End) | None => break 'tour,
                            },
                        }
                    }
                    _ = tick => {
                        if paused { continue; }
                        match gap_left {
                            None => { if !self.is_busy().await { gap_left = Some(PAUSE_BETWEEN_STEPS); } }
                            Some(left) => {
                                if left <= Duration::from_millis(100) { break; }
                                gap_left = Some(left - Duration::from_millis(100));
                            }
                        }
                    }
                }
            }

            if index + 1 >= total {
                self.say(kind, "That's the tour.").await;
                break;
            }
            index += 1;
        }

        if self.hub.is_connected() {
            self.hub.send(json!({"type": "tour.end", "tour_id": tour_id}));
        }
        self.emit("tour:end", json!({"agent_kind": kind, "tour_id": tour_id}));
        *self.control_tx.lock().await = None;
        tracing::info!("Tour '{title}' ended");
    }

    /// Apply a jump control. Cuts any speech first. `None` means the channel closed.
    async fn apply(&self, c: Option<Control>, index: usize, total: usize) -> Option<Move> {
        let c = c?;
        if let Some(q) = &self.queue {
            q.stop_speech().await;
        }
        Some(jump(index, total, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_moves_within_bounds() {
        assert_eq!(jump(0, 3, Control::Next), Move::Goto(1));
        assert_eq!(jump(2, 3, Control::Next), Move::End);
        assert_eq!(jump(0, 3, Control::Back), Move::Goto(0));
        assert_eq!(jump(2, 3, Control::Back), Move::Goto(1));
        assert_eq!(jump(1, 3, Control::Stop), Move::End);
    }

    #[test]
    fn resolves_paths_prefixed_with_the_project_folder_name() {
        let dir = std::env::temp_dir().join("barnaby_prefix_test").join("demo-project");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.ts"), "x
").unwrap();
        let roots = vec![dir.to_string_lossy().to_string()];
        assert!(resolve_path("src/a.ts", &roots).is_some());
        assert!(resolve_path("demo-project/src/a.ts", &roots).is_some());
        assert!(resolve_path("demo-project\\src\\a.ts", &roots).is_some());
        assert!(resolve_path("other/src/a.ts", &roots).is_none());
    }

    #[test]
    fn control_words_parse() {
        assert_eq!(Control::parse("previous"), Some(Control::Back));
        assert_eq!(Control::parse("continue"), Some(Control::Resume));
        assert_eq!(Control::parse("dance"), None);
    }

    #[test]
    fn validate_clamps_lines_drops_missing_files_and_caps_steps() {
        let dir = std::env::temp_dir().join("barnaby_tour_test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5\n").unwrap();
        let roots = vec![dir.to_string_lossy().to_string()];
        let step = |p: &str, s, e| TourStep { path: p.into(), start_line: s, end_line: e, narration: "n".into() };
        let out = validate_steps(vec![step("a.txt", 0, 99), step("missing.txt", 1, 2), step("a.txt", 4, 2)], &roots);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].start_line, out[0].end_line), (1, 5));
        assert_eq!((out[1].start_line, out[1].end_line), (4, 4));
        let many: Vec<_> = (0..20).map(|_| step("a.txt", 1, 1)).collect();
        assert_eq!(validate_steps(many, &roots).len(), MAX_STEPS);
    }
}
