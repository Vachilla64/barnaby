//! Agent registry and the pure mapping from agent hook events to pet reactions.

use crate::voice::queue::Priority;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FEED_LIMIT: usize = 20;
/// A `Stop` hook stays quiet if the agent already called `barnaby_done` this recently.
pub const DONE_GRACE: Duration = Duration::from_secs(30);
/// No message for this long: the pet goes to sleep.
pub const SLEEP_AFTER: Duration = Duration::from_secs(10 * 60);
/// No message for this long: the pet is removed.
pub const REMOVE_AFTER: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub kind: String,
    pub state: String,
    pub feed: Vec<String>,
    #[serde(skip)]
    pub last_seen: Instant,
    #[serde(skip)]
    pub last_done: Option<Instant>,
}

#[derive(Default)]
pub struct AgentRegistry {
    agents: Mutex<HashMap<String, AgentInfo>>,
    feeds: Mutex<HashMap<String, VecDeque<String>>>,
}

impl AgentRegistry {
    /// One entry per agent kind: several sessions of the same kind share one pet.
    pub fn touch(&self, kind: &str, state: Option<&str>, feed_line: Option<&str>) {
        let mut agents = self.agents.lock().unwrap();
        let entry = agents.entry(kind.to_string()).or_insert_with(|| AgentInfo {
            kind: kind.to_string(),
            state: "idle".into(),
            feed: vec![],
            last_seen: Instant::now(),
            last_done: None,
        });
        entry.last_seen = Instant::now();
        if let Some(s) = state {
            entry.state = s.to_string();
        }
        if let Some(line) = feed_line {
            let mut feeds = self.feeds.lock().unwrap();
            let feed = feeds.entry(kind.to_string()).or_default();
            feed.push_front(line.to_string());
            feed.truncate(FEED_LIMIT);
            entry.feed = feed.iter().cloned().collect();
        }
    }

    pub fn mark_done(&self, kind: &str) {
        self.touch(kind, Some("happy"), None);
        if let Some(a) = self.agents.lock().unwrap().get_mut(kind) {
            a.last_done = Some(Instant::now());
        }
    }

    pub fn done_recently(&self, kind: &str) -> bool {
        self.agents
            .lock()
            .unwrap()
            .get(kind)
            .and_then(|a| a.last_done)
            .map(|t| t.elapsed() < DONE_GRACE)
            .unwrap_or(false)
    }

    /// Put quiet agents to sleep and drop long-gone ones. Returns the kinds that just fell asleep
    /// and the kinds that were removed.
    pub fn sweep(&self, now: Instant) -> (Vec<String>, Vec<String>) {
        let mut agents = self.agents.lock().unwrap();
        let mut slept = Vec::new();
        let mut removed = Vec::new();
        for (kind, a) in agents.iter_mut() {
            let quiet = now.saturating_duration_since(a.last_seen);
            if quiet >= REMOVE_AFTER {
                removed.push(kind.clone());
            } else if quiet >= SLEEP_AFTER && a.state != "sleeping" {
                a.state = "sleeping".into();
                slept.push(kind.clone());
            }
        }
        for kind in &removed {
            agents.remove(kind);
            self.feeds.lock().unwrap().remove(kind);
        }
        (slept, removed)
    }

    pub fn list(&self) -> Vec<AgentInfo> {
        let mut v: Vec<_> = self.agents.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.kind.cmp(&b.kind));
        v
    }
}

/// What a hook event should make the pet do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookAction {
    pub state: Option<&'static str>,
    pub speech: Option<(String, Priority)>,
    pub feed: Option<String>,
    /// Speak `speech` only if the agent did not already announce it is done.
    pub skip_if_done_recently: bool,
}

pub fn display_name(kind: &str) -> &'static str {
    match kind {
        "claude-code" => "Claude",
        "gemini-cli" => "Gemini",
        "antigravity" => "Antigravity",
        _ => "Your agent",
    }
}

/// Maps a Claude Code hook payload to a pet reaction. Pure, so it is unit tested.
pub fn map_hook(kind: &str, payload: &Value) -> HookAction {
    let name = display_name(kind);
    let event = payload
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    match event {
        "SessionStart" => HookAction {
            state: Some("idle"),
            ..Default::default()
        },
        "UserPromptSubmit" => HookAction {
            state: Some("working"),
            ..Default::default()
        },
        "Notification" => {
            let msg = payload.get("message").and_then(Value::as_str).unwrap_or("");
            let lower = msg.to_lowercase();
            let text = if lower.contains("permission") {
                format!("{name} needs your approval in the terminal.")
            } else {
                format!("{name} is waiting for you.")
            };
            HookAction {
                state: Some("alert"),
                speech: Some((text, Priority::High)),
                feed: Some(if msg.is_empty() { "Needs attention".into() } else { msg.to_string() }),
                skip_if_done_recently: false,
            }
        }
        "PostToolUse" => {
            let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
            if matches!(tool, "Edit" | "Write" | "MultiEdit") {
                let file = payload
                    .pointer("/tool_input/file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("a file");
                let short = file.rsplit(['\\', '/']).next().unwrap_or(file);
                HookAction {
                    state: Some("working"),
                    feed: Some(format!("edited {short}")),
                    ..Default::default()
                }
            } else {
                HookAction::default()
            }
        }
        "Stop" => HookAction {
            state: Some("idle"),
            speech: Some((format!("{name} has finished."), Priority::High)),
            feed: Some("Finished".into()),
            skip_if_done_recently: true,
        },
        "SessionEnd" => HookAction {
            state: Some("sleeping"),
            ..Default::default()
        },
        _ => HookAction::default(),
    }
}

/// Pet state for a `barnaby_status` phase.
pub fn state_for_phase(phase: &str) -> &'static str {
    match phase {
        "planning" => "thinking",
        "blocked" => "alert",
        "done" => "happy",
        _ => "working",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn notification_permission_is_high_priority_alert() {
        let a = map_hook(
            "claude-code",
            &json!({"hook_event_name": "Notification", "message": "Claude needs your permission to use Bash"}),
        );
        assert_eq!(a.state, Some("alert"));
        let (text, prio) = a.speech.unwrap();
        assert_eq!(prio, Priority::High);
        assert!(text.contains("approval"));
    }

    #[test]
    fn notification_idle_says_waiting() {
        let a = map_hook(
            "claude-code",
            &json!({"hook_event_name": "Notification", "message": "Claude is waiting for your input"}),
        );
        assert!(a.speech.unwrap().0.contains("waiting"));
    }

    #[test]
    fn edit_tools_go_to_feed_without_speech() {
        let a = map_hook(
            "claude-code",
            &json!({"hook_event_name": "PostToolUse", "tool_name": "Edit", "tool_input": {"file_path": "C:\\p\\src\\login.ts"}}),
        );
        assert_eq!(a.feed.as_deref(), Some("edited login.ts"));
        assert!(a.speech.is_none());
    }

    #[test]
    fn other_tools_do_nothing() {
        let a = map_hook("claude-code", &json!({"hook_event_name": "PostToolUse", "tool_name": "Bash"}));
        assert_eq!(a, HookAction::default());
    }

    #[test]
    fn stop_announces_unless_done_was_called() {
        let a = map_hook("claude-code", &json!({"hook_event_name": "Stop"}));
        assert_eq!(a.speech.unwrap().0, "Claude has finished.");
        assert!(a.skip_if_done_recently);
    }

    #[test]
    fn session_events_set_state() {
        assert_eq!(map_hook("x", &json!({"hook_event_name": "UserPromptSubmit"})).state, Some("working"));
        assert_eq!(map_hook("x", &json!({"hook_event_name": "SessionEnd"})).state, Some("sleeping"));
        assert_eq!(map_hook("x", &json!({"hook_event_name": "Bogus"})), HookAction::default());
    }

    #[test]
    fn registry_shares_one_entry_per_kind_and_tracks_done() {
        let r = AgentRegistry::default();
        r.touch("claude-code", Some("working"), Some("a"));
        r.touch("claude-code", None, Some("b"));
        assert_eq!(r.list().len(), 1);
        assert_eq!(r.list()[0].feed, vec!["b", "a"]);
        assert!(!r.done_recently("claude-code"));
        r.mark_done("claude-code");
        assert!(r.done_recently("claude-code"));
    }

    #[test]
    fn sweep_sleeps_then_removes_quiet_agents() {
        let r = AgentRegistry::default();
        r.touch("claude-code", Some("working"), None);
        let t0 = Instant::now();
        assert_eq!(r.sweep(t0), (vec![], vec![]));
        let (slept, removed) = r.sweep(t0 + SLEEP_AFTER + Duration::from_secs(1));
        assert_eq!((slept, removed), (vec!["claude-code".to_string()], vec![]));
        assert_eq!(r.list()[0].state, "sleeping");
        // Already asleep: not reported again.
        assert_eq!(r.sweep(t0 + SLEEP_AFTER + Duration::from_secs(2)).0, Vec::<String>::new());
        let (_, removed) = r.sweep(t0 + REMOVE_AFTER + Duration::from_secs(1));
        assert_eq!(removed, vec!["claude-code".to_string()]);
        assert!(r.list().is_empty());
    }

    #[test]
    fn phases_map_to_states() {
        assert_eq!(state_for_phase("planning"), "thinking");
        assert_eq!(state_for_phase("blocked"), "alert");
        assert_eq!(state_for_phase("testing"), "working");
    }
}
