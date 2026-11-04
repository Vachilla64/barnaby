//! Brain: answers questions about a codebase with Muse Spark (OpenAI-compatible Chat Completions).

pub mod index;

use crate::config::BrainConfig;
use crate::server::{emit_agent_state, speak, ServerState};
use crate::tours::TourStep;
use crate::voice::queue::{Priority, SpeechKind};
use index::RepoIndex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const MAX_TOOL_ROUNDS: usize = 8;
const MAX_READ_LINES: usize = 400;
const MAX_SEARCH_HITS: usize = 50;
const KEYRING_SERVICE: &str = "Barnaby";
const KEYRING_USER: &str = "meta-api-key";
/// The voice and pet name used when the brain speaks.
const BRAIN_KIND: &str = "barnaby";

const SYSTEM_PROMPT: &str = "You are Barnaby, a friendly guide who explains a codebase to the person who owns it. \
They may not have written most of it. Answer in plain spoken English, as if talking next to them: short sentences, \
no markdown, no code read aloud, file names said simply (\"the validate file in auth\"). Only state what the code shows; \
read files when unsure. When the answer lives in specific places, return a tour: an ordered walk through the files, \
each step with a narration of one to three sentences that says what this piece does and how it connects to the next.\n\n\
Reply with ONLY a JSON object, no other text, in this shape:\n\
{\"speech\": \"2-4 spoken sentences, no code, no markdown\", \"tour\": [{\"path\": \"relative/path.ext\", \"start_line\": 1, \"end_line\": 10, \"narration\": \"one to three sentences\"}]}\n\
\"tour\" may be an empty list. Use at most 12 steps and real line numbers from the files.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Answer {
    pub speech: String,
    #[serde(default)]
    pub tour: Vec<TourStep>,
}

// ---------- API key ----------

pub fn save_api_key(key: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .and_then(|e| e.set_password(key.trim()))
        .map_err(|e| format!("Could not save the key: {e}"))
}

pub fn load_api_key() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|k| !k.trim().is_empty())
        .or_else(|| std::env::var("MODEL_API_KEY").ok().filter(|k| !k.trim().is_empty()))
}

/// A local model (for example Ollama) needs no key; Meta's API does.
fn key_for(cfg: &BrainConfig) -> Result<String, String> {
    match load_api_key() {
        Some(k) => Ok(k),
        None if !cfg.base_url.contains("meta.ai") => Ok("local".into()),
        None => Err("no key".into()),
    }
}

// ---------- Tools ----------

fn tool_defs() -> Value {
    json!([
        {"type": "function", "function": {
            "name": "read_file", "description": "Read lines of a file in the project (max 400 lines per call).",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string"}, "start_line": {"type": "integer"}, "end_line": {"type": "integer"}
            }, "required": ["path"]}}},
        {"type": "function", "function": {
            "name": "search", "description": "Regex search across project files. Returns up to 50 hits as path:line: text.",
            "parameters": {"type": "object", "properties": {
                "pattern": {"type": "string"}, "glob": {"type": "string", "description": "Optional file extension filter such as ts"}
            }, "required": ["pattern"]}}},
        {"type": "function", "function": {
            "name": "list_dir", "description": "List a directory in the project.",
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}}
    ])
}

/// Join `rel` onto `root` and refuse anything that escapes the project.
pub fn sandbox_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let joined = root.join(rel.trim_start_matches(['/', '\\']));
    let canon = joined.canonicalize().map_err(|_| format!("No such path: {rel}"))?;
    if canon.starts_with(&root) {
        Ok(canon)
    } else {
        Err("That path is outside the project.".into())
    }
}

pub fn run_tool(root: &Path, index: &RepoIndex, name: &str, args: &Value) -> String {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("");
    let n = |k: &str| args.get(k).and_then(Value::as_u64).map(|v| v as usize);
    match name {
        "read_file" => {
            let path = match sandbox_path(root, s("path")) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                return "Could not read that file as text.".into();
            };
            let start = n("start_line").unwrap_or(1).max(1);
            let end = n("end_line").unwrap_or(start + MAX_READ_LINES - 1).min(start + MAX_READ_LINES - 1);
            text.lines()
                .enumerate()
                .skip(start - 1)
                .take(end.saturating_sub(start) + 1)
                .map(|(i, l)| format!("{:>4}| {}", i + 1, l))
                .collect::<Vec<_>>()
                .join("\n")
        }
        "search" => {
            let Ok(re) = regex::Regex::new(s("pattern")) else {
                return "Invalid regex.".into();
            };
            let ext = s("glob").trim_start_matches("*.").trim_start_matches('.');
            let mut hits = Vec::new();
            'files: for f in &index.files {
                if !ext.is_empty() && !f.rel_path.ends_with(&format!(".{ext}")) {
                    continue;
                }
                for (i, line) in f.text.lines().enumerate() {
                    if re.is_match(line) {
                        hits.push(format!("{}:{}: {}", f.rel_path, i + 1, line.trim()));
                        if hits.len() >= MAX_SEARCH_HITS {
                            break 'files;
                        }
                    }
                }
            }
            if hits.is_empty() {
                "No matches.".into()
            } else {
                hits.join("\n")
            }
        }
        "list_dir" => {
            let path = match sandbox_path(root, s("path")) {
                Ok(p) => p,
                Err(e) => return e,
            };
            match std::fs::read_dir(&path) {
                Ok(rd) => {
                    let mut names: Vec<String> = rd
                        .flatten()
                        .map(|e| {
                            let n = e.file_name().to_string_lossy().to_string();
                            if e.path().is_dir() {
                                format!("{n}/")
                            } else {
                                n
                            }
                        })
                        .collect();
                    names.sort();
                    names.join("\n")
                }
                Err(e) => e.to_string(),
            }
        }
        other => format!("Unknown tool: {other}"),
    }
}

// ---------- Answer parsing ----------

/// Pull the JSON object out of a model reply, tolerating code fences and chatter around it.
pub fn parse_answer(text: &str) -> Result<Answer, String> {
    let start = text.find('{').ok_or("no JSON object in reply")?;
    let end = text.rfind('}').ok_or("no JSON object in reply")?;
    if end < start {
        return Err("no JSON object in reply".into());
    }
    let answer: Answer = serde_json::from_str(&text[start..=end]).map_err(|e| e.to_string())?;
    if answer.speech.trim().is_empty() {
        return Err("empty speech".into());
    }
    Ok(answer)
}

// ---------- Chat Completions ----------

async fn chat(cfg: &BrainConfig, key: &str, messages: &[Value], with_tools: bool) -> Result<Value, String> {
    let mut body = json!({"model": cfg.model, "messages": messages});
    if with_tools {
        body["tools"] = tool_defs();
        body["tool_choice"] = json!("auto");
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/chat/completions", cfg.base_url.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("API error {status}: {}", text.chars().take(300).collect::<String>()));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("bad API reply: {e}"))?;
    v.pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| "API reply had no message".into())
}

pub async fn ask(
    cfg: &BrainConfig,
    key: &str,
    root: &Path,
    question: &str,
    editor: Option<&Value>,
) -> Result<Answer, String> {
    let root_buf = root.to_path_buf();
    let index = tokio::task::spawn_blocking(move || RepoIndex::build(&root_buf))
        .await
        .map_err(|e| e.to_string())?;

    let mut context = format!("Project repo map:\n{}", index.repo_map());
    if let Some(full) = index.full_text() {
        context.push_str(&format!("\nFull source:\n{full}"));
    }
    let mut user = format!("{context}\n\nQuestion: {question}");
    if let Some(ed) = editor {
        let path = ed.get("path").and_then(Value::as_str).unwrap_or("");
        if !path.is_empty() {
            let selected = ed
                .get("selection_text")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(|t| format!(" Selected text:\n{t}"))
                .unwrap_or_default();
            user.push_str(&format!(
                "\n\nThe user has {path} open at line {}.{selected}",
                ed.get("cursor_line").and_then(Value::as_u64).unwrap_or(1),
            ));
        }
    }

    let mut messages = vec![
        json!({"role": "system", "content": SYSTEM_PROMPT}),
        json!({"role": "user", "content": user}),
    ];
    let mut retried = false;

    for round in 0..=MAX_TOOL_ROUNDS {
        let with_tools = round < MAX_TOOL_ROUNDS;
        let msg = chat(cfg, key, &messages, with_tools).await?;
        let calls = msg.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default();
        if with_tools && !calls.is_empty() {
            messages.push(msg.clone());
            for call in calls {
                let id = call.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                let name = call.pointer("/function/name").and_then(Value::as_str).unwrap_or("");
                let args: Value = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .and_then(|a| serde_json::from_str(a).ok())
                    .unwrap_or(json!({}));
                let result = run_tool(root, &index, name, &args);
                messages.push(json!({"role": "tool", "tool_call_id": id, "content": result}));
            }
            continue;
        }
        let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
        match parse_answer(content) {
            Ok(a) => return Ok(a),
            Err(e) if !retried => {
                tracing::warn!("Bad answer JSON ({e}); retrying once");
                retried = true;
                messages.push(json!({"role": "assistant", "content": content}));
                messages.push(json!({"role": "user", "content": "Reply with ONLY the JSON object described in the instructions."}));
            }
            Err(e) => return Err(format!("The model did not return a usable answer: {e}")),
        }
    }
    Err("The model kept calling tools without answering.".into())
}

// ---------- Presenting an answer ----------

const SHOW_ME_WORDS: [&str; 4] = ["show me", "walk me through", "take me through", "give me a tour"];

pub fn wants_tour(question: &str) -> bool {
    let q = question.to_lowercase();
    SHOW_ME_WORDS.iter().any(|w| q.contains(w))
}

fn pick_project(state: &ServerState) -> Option<PathBuf> {
    let folders = state.hub.workspace_folders();
    let from_editor = state.hub.editor_context().and_then(|c| {
        let p = c.get("path")?.as_str()?.to_lowercase().replace('/', "\\");
        folders
            .iter()
            .find(|f| p.starts_with(&f.to_lowercase().replace('/', "\\")))
            .cloned()
    });
    from_editor
        .or_else(|| folders.first().cloned())
        .or_else(|| crate::config::load_or_create_config().ok().and_then(|c| c.projects.first().cloned()))
        .map(PathBuf::from)
}

async fn say_error(state: &ServerState, text: &str) {
    emit_agent_state(state, BRAIN_KIND, "alert", Some(text));
    speak(state, BRAIN_KIND, text, Priority::High, SpeechKind::Alert).await;
}

/// Answer `question` out loud, then offer or start a tour. Used by the Ask box, VS Code and REST.
pub async fn ask_and_present(state: Arc<ServerState>, question: String) -> Option<Answer> {
    let cfg = crate::config::load_or_create_config().ok()?;
    let key = match key_for(&cfg.brain) {
        Ok(k) => k,
        Err(_) => {
            if let Some(app) = &state.app_handle {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            say_error(&state, "I need a Meta API key first. I opened settings.").await;
            return None;
        }
    };
    let Some(root) = pick_project(&state) else {
        say_error(&state, "Which project? Open it in VS Code and ask again.").await;
        return None;
    };

    emit_agent_state(&state, BRAIN_KIND, "thinking", Some(&format!("Asked: {question}")));
    let editor = state.hub.editor_context();
    match ask(&cfg.brain, &key, &root, &question, editor.as_ref()).await {
        Ok(answer) => {
            emit_agent_state(&state, BRAIN_KIND, "talking", None);
            speak(&state, BRAIN_KIND, &answer.speech, Priority::High, SpeechKind::Answer).await;
            if !answer.tour.is_empty() {
                let colour = crate::server::pet_colour(BRAIN_KIND);
                if wants_tour(&question) {
                    let mut roots = state.hub.workspace_folders();
                    roots.push(root.to_string_lossy().to_string());
                    let steps = crate::tours::validate_steps(answer.tour.clone(), &roots);
                    state.tours.start(BRAIN_KIND, "Tour", steps, colour).await;
                } else {
                    let mut roots = state.hub.workspace_folders();
                    roots.push(root.to_string_lossy().to_string());
                    let steps = crate::tours::validate_steps(answer.tour.clone(), &roots);
                    if !steps.is_empty() {
                        let n = steps.len();
                        state.tours.set_pending("Tour", steps);
                        if let Some(app) = &state.app_handle {
                            use tauri::Emitter;
                            let _ = app.emit("tour:offer", json!({"steps": n}));
                        }
                    }
                }
            }
            Some(answer)
        }
        Err(e) => {
            tracing::error!("Brain error: {e}");
            say_error(&state, "I couldn't reach my brain just now. Try again in a moment.").await;
            None
        }
    }
}

/// Settings "Test" button: one tiny request.
pub async fn test_connection() -> Result<String, String> {
    let cfg = crate::config::load_or_create_config().map_err(|e| e.to_string())?;
    let key = key_for(&cfg.brain).map_err(|_| "No key saved yet.".to_string())?;
    let msg = chat(&cfg.brain, &key, &[json!({"role": "user", "content": "Reply with the word ok."})], false).await?;
    Ok(format!(
        "Connected to {} ({}).",
        cfg.brain.base_url,
        msg.get("content").and_then(Value::as_str).unwrap_or("").trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_fenced_and_chatty_json() {
        let plain = r#"{"speech":"Login is in auth.","tour":[]}"#;
        assert_eq!(parse_answer(plain).unwrap().speech, "Login is in auth.");
        let fenced = "```json\n{\"speech\":\"Hi.\",\"tour\":[{\"path\":\"a.ts\",\"start_line\":1,\"end_line\":2,\"narration\":\"n\"}]}\n```";
        assert_eq!(parse_answer(fenced).unwrap().tour.len(), 1);
        assert!(parse_answer("Sure! {\"speech\":\"x\"} done").is_ok());
        assert!(parse_answer("no json here").is_err());
        assert!(parse_answer("{\"speech\":\"\"}").is_err());
    }

    #[test]
    fn show_me_questions_start_tours() {
        assert!(wants_tour("Show me where login is handled"));
        assert!(wants_tour("walk me through the server"));
        assert!(!wants_tour("where is the server started?"));
    }

    #[test]
    fn sandbox_refuses_escapes_and_tools_work() {
        let dir = std::env::temp_dir().join("barnaby_sandbox_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.ts"), "line one\nline two\nline three\n").unwrap();
        std::fs::write(dir.parent().unwrap().join("barnaby_secret.txt"), "secret").unwrap();
        assert!(sandbox_path(&dir, "src/a.ts").is_ok());
        assert!(sandbox_path(&dir, "../barnaby_secret.txt").is_err());
        assert!(sandbox_path(&dir, "nope.txt").is_err());
        let idx = RepoIndex::build(&dir);
        let read = run_tool(&dir, &idx, "read_file", &json!({"path": "src/a.ts", "start_line": 2, "end_line": 3}));
        assert_eq!(read, "   2| line two\n   3| line three");
        assert!(run_tool(&dir, &idx, "search", &json!({"pattern": "two"})).contains("src/a.ts:2: line two"));
        assert!(run_tool(&dir, &idx, "list_dir", &json!({"path": "src"})).contains("a.ts"));
        assert!(run_tool(&dir, &idx, "read_file", &json!({"path": "../barnaby_secret.txt"})).contains("outside"));
    }
}
