//! Connect buttons: register Barnaby's MCP server and hooks with a coding agent.
//! Always merges into existing settings and keeps a backup; never overwrites.

use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const HOOK_MARKER: &str = "/api/v1/hooks/claude-code";
const EVENTS: [(&str, Option<&str>); 6] = [
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("Notification", None),
    ("PostToolUse", Some("Edit|Write|MultiEdit")),
    ("Stop", None),
    ("SessionEnd", None),
];

fn hook_command(port: u16, token: &str) -> String {
    format!(
        "curl -s -m 2 -X POST http://127.0.0.1:{port}{HOOK_MARKER} -H \"Authorization: Bearer {token}\" -H \"Content-Type: application/json\" --data-binary @-"
    )
}

/// Adds Barnaby's hook entries to a Claude Code settings object. Returns how many were added.
/// Existing hooks are kept. Entries already pointing at Barnaby are replaced, so a changed
/// port or token is picked up.
pub fn merge_hooks(settings: &mut Value, port: u16, token: &str) -> usize {
    if !settings.is_object() {
        *settings = json!({});
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let command = hook_command(port, token);
    let mut added = 0;
    for (event, matcher) in EVENTS {
        let list = hooks
            .as_object_mut()
            .unwrap()
            .entry(event)
            .or_insert_with(|| json!([]));
        if !list.is_array() {
            *list = json!([]);
        }
        let arr = list.as_array_mut().unwrap();
        arr.retain(|entry| !entry.to_string().contains(HOOK_MARKER));
        let mut entry = json!({"hooks": [{"type": "command", "command": command}]});
        if let Some(m) = matcher {
            entry["matcher"] = json!(m);
        }
        arr.push(entry);
        added += 1;
    }
    added
}

fn claude_settings_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude").join("settings.json")
}

pub fn describe_claude_connect(port: u16) -> String {
    format!(
        "Barnaby will change two things:\n\n\
         1. Run: claude mcp add --transport http --scope user barnaby http://127.0.0.1:{port}/mcp (with your local token)\n\n\
         2. Add one hook for each of: {}\n   to {}\n   Your existing settings stay. A backup is saved as settings.json.barnaby.bak.\n\nContinue?",
        EVENTS.map(|(e, _)| e).join(", "),
        claude_settings_path().display()
    )
}

pub fn connect_claude_code(port: u16, token: &str) -> Result<String, String> {
    // 1. Hooks
    let path = claude_settings_path();
    let mut settings: Value = if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        std::fs::write(path.with_extension("json.barnaby.bak"), &text).map_err(|e| e.to_string())?;
        serde_json::from_str(&text)
            .map_err(|e| format!("{} is not valid JSON, left untouched: {e}", path.display()))?
    } else {
        json!({})
    };
    let added = merge_hooks(&mut settings, port, token);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).map_err(|e| e.to_string())?;

    // 2. MCP server (remove first so a re-connect updates the token)
    let _ = Command::new("claude")
        .args(["mcp", "remove", "--scope", "user", "barnaby"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let out = Command::new("claude")
        .args([
            "mcp",
            "add",
            "--transport",
            "http",
            "--scope",
            "user",
            "barnaby",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--header",
            &format!("Authorization: Bearer {token}"),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Hooks added, but running `claude` failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Hooks added, but `claude mcp add` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(format!("Connected. Added {added} hooks and the barnaby MCP server. Restart Claude Code sessions."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_existing_hooks_and_is_idempotent() {
        let mut s = json!({"theme": "dark", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "echo mine"}]}]}});
        assert_eq!(merge_hooks(&mut s, 4747, "tok"), 6);
        merge_hooks(&mut s, 4747, "tok2");
        let stop = s["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "user's hook plus exactly one Barnaby hook");
        assert!(stop[0].to_string().contains("echo mine"));
        assert!(stop[1].to_string().contains("tok2"));
        assert_eq!(s["theme"], "dark");
        assert_eq!(s["hooks"]["PostToolUse"][0]["matcher"], "Edit|Write|MultiEdit");
    }

    #[test]
    fn merge_into_empty_settings() {
        let mut s = json!({});
        merge_hooks(&mut s, 4748, "t");
        assert_eq!(s["hooks"].as_object().unwrap().len(), 6);
        assert!(s["hooks"]["Notification"][0]["hooks"][0]["command"].as_str().unwrap().contains(":4748/"));
    }
}
