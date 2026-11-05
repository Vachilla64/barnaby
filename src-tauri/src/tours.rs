//! Tour engine: walks the user through code, one step at a time, in step with speech.

use crate::config::TourPace;
use crate::vscode::VsCodeHub;
use crate::voice::queue::{Priority, SpeechKind, SpeechQueue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

/// A walk with more stops than a person can hold in mind is a tour nobody follows.
pub const MAX_STEPS: usize = 6;
const SHOWN_TIMEOUT: Duration = Duration::from_secs(3);
/// In `TourPace::Auto`: a breath between one step and the next.
const PAUSE_BETWEEN_STEPS: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TourStep {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// Two to five plain words naming the step. Agents' tours may leave it out.
    #[serde(default)]
    pub title: String,
    pub narration: String,
}

impl TourStep {
    /// What the step is called on screen: its title, or the file name when it has none.
    pub fn label(&self) -> String {
        let title = self.title.trim();
        if !title.is_empty() {
            return title.to_string();
        }
        Path::new(&self.path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone())
    }
}

/// A whole tour: what it is called, its steps, and the one thing to remember at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tour {
    pub title: String,
    pub steps: Vec<TourStep>,
    /// One sentence he says after the last step. May be empty.
    pub recap: String,
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

/// The recap as a plain statement. He introduces it ("The thing to remember: ..."), so a recap that
/// begins with "Remember" would say it twice.
pub fn clean_recap(recap: &str) -> String {
    let r = recap.trim();
    let lower = r.to_lowercase();
    let rest = ["remember that ", "remember: ", "remember, ", "remember "]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map(|p| r[p.len()..].trim_start())
        .unwrap_or(r);
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// After a step has been spoken: does he stop and wait for the person? Never after the last step,
/// which simply ends.
pub fn waits_for_person(pace: TourPace, index: usize, total: usize) -> bool {
    pace == TourPace::Wait && index + 1 < total
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

/// How a file name is said aloud: "login.ts" becomes "login dot ts".
fn spoken_file(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    name.replace('.', " dot ").replace(['_', '-'], " ")
}

/// What he says first so you know where to look: "Look at lines 12 to 20 of login dot ts."
pub fn where_phrase(step: &TourStep) -> String {
    let lines = if step.start_line == step.end_line {
        format!("line {}", step.start_line)
    } else {
        format!("lines {} to {}", step.start_line, step.end_line)
    };
    format!("Look at {lines} of {}.", spoken_file(&step.path))
}

/// What he says for a step. When VS Code is following along, the highlight already shows where to
/// look, so he only talks about the code. Otherwise the location goes in front, unless the narration
/// already names the file (the full name, like "login.ts", or the way it is said, like "login dot ts").
pub fn spoken_step(step: &TourStep, following: bool) -> String {
    if following {
        return step.narration.clone();
    }
    let name = Path::new(&step.path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let narration = step.narration.to_lowercase();
    let names_file = !name.is_empty() && (narration.contains(&name) || narration.contains(&spoken_file(&step.path).to_lowercase()));
    if names_file {
        step.narration.clone()
    } else {
        format!("{} {}", where_phrase(step), step.narration)
    }
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
                title: s.title,
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
    pending: std::sync::Mutex<Option<Tour>>,
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
    pub async fn start(self: &Arc<Self>, agent_kind: &str, tour: Tour, colour: String) -> usize {
        let roots = self.hub.workspace_folders();
        let steps = validate_steps(tour.steps, &roots);
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
        let (title, recap) = (tour.title, tour.recap);
        tokio::spawn(async move {
            engine.run(&kind, &title, &recap, steps, colour, rx).await;
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
    pub fn set_pending(&self, tour: Tour) {
        *self.pending.lock().unwrap() = Some(tour);
    }

    pub async fn start_pending(self: &Arc<Self>, agent_kind: &str, colour: String) -> usize {
        let taken = self.pending.lock().unwrap().take();
        match taken {
            Some(tour) => self.start(agent_kind, tour, colour).await,
            None => 0,
        }
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, payload);
        }
    }

    async fn run(&self, kind: &str, title: &str, recap: &str, steps: Vec<TourStep>, colour: String, mut rx: mpsc::Receiver<Control>) {
        let tour_id = format!("tour-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
        let total = steps.len();
        let mut index = 0usize;
        // Settings: move VS Code for the user or only say where to look, and who sets the pace.
        let settings = crate::config::load_or_create_config().map(|c| c.tour).unwrap_or_default();
        let (navigate, pace) = (settings.navigate, settings.pace);
        let mut finished = false;
        tracing::info!(
            "Tour '{title}' started: {total} steps (vscode connected: {}, navigate: {navigate}, pace: {pace:?})",
            self.hub.is_connected()
        );
        // The agenda first: what the person is about to be shown, in order.
        self.emit("tour:start", json!({
            "agent_kind": kind, "tour_id": tour_id, "title": title, "total": total, "pace": pace,
            "labels": steps.iter().map(TourStep::label).collect::<Vec<_>>(),
            "vscode": navigate && self.hub.is_connected()
        }));

        'tour: loop {
            let step = &steps[index];
            let connected = navigate && self.hub.is_connected();
            if connected {
                self.hub.send(json!({
                    "type": "tour.step", "tour_id": tour_id, "index": index, "total": total,
                    "path": step.path, "start_line": step.start_line, "end_line": step.end_line, "colour": colour
                }));
            }
            self.emit("tour:step", json!({
                "agent_kind": kind, "tour_id": tour_id, "index": index, "total": total,
                "path": step.path, "start_line": step.start_line, "end_line": step.end_line,
                "narration": step.narration, "title": step.label(), "vscode": connected
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

            self.say(kind, &spoken_step(step, connected)).await;

            // Wait for the speech to finish, then either for the person (they set the pace) or for a
            // short breath. Controls are heard throughout.
            let mut paused = false;
            let mut waiting = false;
            let mut gap_left: Option<Duration> = None;
            loop {
                let tick = tokio::time::sleep(Duration::from_millis(100));
                tokio::select! {
                    c = rx.recv() => {
                        // "Go on" while he waits for you means the same as "next".
                        let c = match c {
                            Some(Control::Resume) if waiting => Some(Control::Next),
                            other => other,
                        };
                        match c {
                            Some(Control::Pause) => { paused = true; }
                            Some(Control::Resume) => { paused = false; }
                            other => {
                                let last_next = other == Some(Control::Next) && index + 1 >= total;
                                match self.apply(other, index, total).await {
                                    Some(Move::Goto(i)) => { index = i; continue 'tour; }
                                    // Next on the last step is "finish": he still sums up.
                                    Some(Move::End) | None => { finished = last_next; break 'tour; }
                                }
                            }
                        }
                    }
                    _ = tick => {
                        if paused { continue; }
                        match gap_left {
                            None => {
                                if !self.is_busy().await {
                                    if waits_for_person(pace, index, total) {
                                        if !waiting {
                                            waiting = true;
                                            self.emit("tour:waiting", json!({"tour_id": tour_id, "index": index}));
                                        }
                                    } else {
                                        gap_left = Some(PAUSE_BETWEEN_STEPS);
                                    }
                                }
                            }
                            Some(left) => {
                                if left <= Duration::from_millis(100) { break; }
                                gap_left = Some(left - Duration::from_millis(100));
                            }
                        }
                    }
                }
            }

            if index + 1 >= total {
                finished = true;
                break;
            }
            index += 1;
        }

        let recap = clean_recap(recap);
        if finished {
            let closing = if recap.is_empty() {
                "That's the tour.".to_string()
            } else {
                format!("That's the tour. The thing to remember: {recap}")
            };
            self.say(kind, &closing).await;
        }
        if navigate && self.hub.is_connected() {
            self.hub.send(json!({"type": "tour.end", "tour_id": tour_id}));
        }
        self.emit("tour:end", json!({
            "agent_kind": kind, "tour_id": tour_id, "finished": finished,
            "recap": if finished { recap.as_str() } else { "" }
        }));
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
    fn says_where_to_look_unless_the_narration_already_does() {
        let step = |n: &str, s, e| TourStep { path: "C:\\p\\src\\login_form.ts".into(), start_line: s, end_line: e, title: String::new(), narration: n.into() };
        assert_eq!(where_phrase(&step("x", 12, 20)), "Look at lines 12 to 20 of login form dot ts.");
        assert_eq!(where_phrase(&step("x", 7, 7)), "Look at line 7 of login form dot ts.");
        assert_eq!(spoken_step(&step("The check happens here.", 3, 4), false), "Look at lines 3 to 4 of login form dot ts. The check happens here.");
        assert_eq!(spoken_step(&step("In login_form.ts the password is hashed.", 3, 4), false), "In login_form.ts the password is hashed.");
        // A common word that is also the file's stem must not count as naming the file.
        assert!(spoken_step(&step("This is the login form.", 3, 4), false).starts_with("Look at lines 3 to 4"));
    }

    #[test]
    fn when_vs_code_follows_along_the_highlight_says_where_so_he_does_not() {
        let step = TourStep { path: "C:\\p\\src\\a.ts".into(), start_line: 3, end_line: 9, title: String::new(), narration: "The check happens here.".into() };
        assert_eq!(spoken_step(&step, true), "The check happens here.");
    }

    #[test]
    fn a_step_is_called_by_its_title_or_else_its_file() {
        let step = |title: &str| TourStep { path: "C:\\p\\src\\login.ts".into(), start_line: 1, end_line: 2, title: title.into(), narration: "n".into() };
        assert_eq!(step("Checking the password").label(), "Checking the password");
        assert_eq!(step("  ").label(), "login.ts");
        // Tours from agents have no titles at all.
        let from_agent: TourStep = serde_json::from_str(r#"{"path":"a.ts","start_line":1,"end_line":2,"narration":"n"}"#).unwrap();
        assert_eq!(from_agent.label(), "a.ts");
    }

    #[test]
    fn the_recap_does_not_say_remember_twice() {
        assert_eq!(clean_recap("Remember shape first, then secrets."), "Shape first, then secrets.");
        assert_eq!(clean_recap("  remember that the ticket lasts 30 minutes. "), "The ticket lasts 30 minutes.");
        assert_eq!(clean_recap("Remember: check twice."), "Check twice.");
        assert_eq!(clean_recap("Words become sound one sentence at a time."), "Words become sound one sentence at a time.");
        assert_eq!(clean_recap("   "), "");
    }

    #[test]
    fn he_waits_for_the_person_between_steps_but_not_after_the_last() {
        assert!(waits_for_person(TourPace::Wait, 0, 4));
        assert!(waits_for_person(TourPace::Wait, 2, 4));
        assert!(!waits_for_person(TourPace::Wait, 3, 4));
        assert!(!waits_for_person(TourPace::Auto, 0, 4));
        assert_eq!(TourPace::default(), TourPace::Wait);
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
        let step = |p: &str, s, e| TourStep { path: p.into(), start_line: s, end_line: e, title: "t".into(), narration: "n".into() };
        let out = validate_steps(vec![step("a.txt", 0, 99), step("missing.txt", 1, 2), step("a.txt", 4, 2)], &roots);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].start_line, out[0].end_line), (1, 5));
        assert_eq!((out[1].start_line, out[1].end_line), (4, 4));
        assert_eq!(out[0].title, "t");
        let many: Vec<_> = (0..20).map(|_| step("a.txt", 1, 1)).collect();
        assert_eq!(validate_steps(many, &roots).len(), MAX_STEPS);
    }
}
