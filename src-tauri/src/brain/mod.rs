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

/// How he talks. It is built on what is known about teaching: give the answer and a map first, then
/// small steps the learner can hold in mind (about four), one idea each, in plain words; do not
/// read the screen aloud; finish with the one thing to remember.
const SYSTEM_PROMPT: &str = "You are Sir Barnaby, a kind old English gentleman who helps the owner of a codebase understand it. \
If you ever introduce yourself, say you are Sir Barnaby.\n\n\
WHO YOU ARE TALKING TO\n\
Assume the owner is clever but has not read this code and may be new to programming. Your words are SPOKEN aloud, so write for the ear.\n\n\
PLAIN ENGLISH\n\
- Short sentences, about 15 words, one idea each. Everyday words. Active voice.\n\
- Say what a thing is FOR before how it works. Purpose first, detail second.\n\
- Never leave jargon unexplained. If a technical word is needed, say what it means in the same breath (\"a route, which is a web address the server answers\").\n\
- Do not read code aloud: no symbols, no line numbers, no long names. Call things by their job (\"the part that checks the password\"). You may say a file or function name once, simply, so they can find it.\n\
- Only state what the code shows. Read files when unsure. If you are not sure, say so.\n\n\
THE ANSWER\n\
- speech: the answer comes first, in the very first sentence. If you return a tour, end speech with the map: say how many stops there are and name them in order, in the same words as the step titles but written in lower case inside the sentence (\"I will show you three things: where the form is built, where the password is checked, and where the result goes.\"). In all, at most 4 sentences and about 60 words. Without a tour, 2 to 4 short sentences.\n\
- tour: only when the answer lives in two or more places worth seeing. Otherwise leave it empty. Aim for 3 or 4 steps, never more than 5. Fewer, clearer steps beat many.\n\
  - One idea per step. Order the steps as a story (what starts it, what happens, what comes out), not by file.\n\
  - title: 2 to 5 plain words naming the step (\"Where the password is checked\").\n\
  - narration: one or two short sentences, about 30 words at most. Say what this piece does and why it matters or what it hands on to the next piece. When the file changes, say so in plain words (\"Over in the login file, ...\"). Never repeat an earlier step. Never describe code line by line.\n\
  - Highlight only the lines you talk about: 15 lines at most per step, with real line numbers from the files.\n\
- recap: one plain sentence stating the single idea to take away (do not begin it with \"Remember\"). Empty when there is no tour.\n\
- If the user has selected code, explain that selection first and say which file it is in; then say briefly how it connects to the rest of the project.\n\
- Earlier questions and answers come before the new question. For follow-ups such as \"why?\" or \"and where is that called?\", answer just that, briefly, and do not give the overview again.\n\n\
Reply with ONLY a JSON object, no other text, in this shape:\n\
{\"speech\": \"...\", \"tour\": [{\"path\": \"relative/path.ext\", \"start_line\": 1, \"end_line\": 10, \"title\": \"...\", \"narration\": \"...\"}], \"recap\": \"...\"}\n\
\"tour\" may be an empty list.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Answer {
    pub speech: String,
    #[serde(default)]
    pub tour: Vec<TourStep>,
    #[serde(default)]
    pub recap: String,
}

/// One question and the answer he gave (what he said and the stops he offered), kept so a follow-up
/// such as "tell me more about the second one" can refer back to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub question: String,
    pub speech: String,
    pub tour: Vec<TourStep>,
}

/// The running conversation. It belongs to one project: asking about another starts afresh.
#[derive(Debug, Default)]
pub struct Conversation {
    pub root: Option<PathBuf>,
    pub turns: Vec<Turn>,
}

/// Older turns are dropped past this many, to keep requests small.
const MAX_TURNS: usize = 8;

impl Conversation {
    /// The turns to send with a question about `root`. Switching project forgets the old ones.
    pub fn history_for(&mut self, root: &Path) -> Vec<Turn> {
        if self.root.as_deref() != Some(root) {
            self.root = Some(root.to_path_buf());
            self.turns.clear();
        }
        self.turns.clone()
    }

    pub fn record(&mut self, turn: Turn) {
        self.turns.push(turn);
        if self.turns.len() > MAX_TURNS {
            self.turns.remove(0);
        }
    }

    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

fn norm_path(p: &str) -> String {
    p.to_lowercase().replace('/', "\\")
}

/// A note about what the user has open in the editor, or None when it is not part of the project.
/// Files outside the project folder are never described, so their code is never sent anywhere.
pub fn editor_note(ed: &Value, root: &Path) -> Option<String> {
    let path = ed.get("path")?.as_str().filter(|p| !p.is_empty())?;
    let root_s = norm_path(&root.to_string_lossy());
    let path_n = norm_path(path);
    if !path_n.starts_with(&root_s) {
        return None;
    }
    let rel = path.get(root_s.len()..).unwrap_or(path).trim_start_matches(['\\', '/']).replace('\\', "/");
    let line = |k: &str| ed.get(k).and_then(Value::as_u64).unwrap_or(1);
    let selected = ed.get("selection_text").and_then(Value::as_str).unwrap_or("").trim_end();
    if selected.trim().is_empty() {
        Some(format!("The user has {rel} open, with the cursor on line {}.", line("cursor_line")))
    } else {
        Some(format!(
            "The user has selected lines {} to {} of {rel}:\n```\n{selected}\n```\n\
When they say \"this\", \"it\", \"here\" or \"that code\", they mean this selection.",
            line("start_line"),
            line("end_line"),
        ))
    }
}

/// System prompt (with the project in it), then the earlier turns, then the new question.
pub fn build_messages(system: String, history: &[Turn], user: String) -> Vec<Value> {
    let mut messages = vec![json!({"role": "system", "content": system})];
    for t in history {
        messages.push(json!({"role": "user", "content": t.question}));
        messages.push(json!({"role": "assistant", "content": json!({"speech": t.speech, "tour": t.tour}).to_string()}));
    }
    messages.push(json!({"role": "user", "content": user}));
    messages
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
    if cfg.base_url.contains("meta.ai") {
        // Muse Spark always reasons; "low" keeps spoken answers quick.
        body["reasoning_effort"] = json!("low");
    }
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
    history: &[Turn],
) -> Result<Answer, String> {
    let root_buf = root.to_path_buf();
    let index = tokio::task::spawn_blocking(move || RepoIndex::build(&root_buf))
        .await
        .map_err(|e| e.to_string())?;

    // The project goes in the system message so it is the same for every turn of a conversation.
    let mut system = format!("{SYSTEM_PROMPT}\n\nProject repo map:\n{}", index.repo_map());
    if let Some(full) = index.full_text() {
        system.push_str(&format!("\n\nFull source:\n{full}"));
    }
    let mut user = format!("Question: {question}");
    if let Some(note) = editor.and_then(|ed| editor_note(ed, root)) {
        user.push_str(&format!("\n\n{note}"));
    }

    let mut messages = build_messages(system, history, user);
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

pub fn open_settings_window(state: &ServerState) {
    if let Some(app) = &state.app_handle {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("settings") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

/// Tell the chat list something (a question, an answer, an error).
fn chat_emit(state: &ServerState, event: &str, payload: Value) {
    if let Some(app) = &state.app_handle {
        use tauri::Emitter;
        let _ = app.emit(event, payload);
    }
}

/// Forget the conversation so far; the next question starts afresh.
pub fn reset_conversation(state: &ServerState) {
    state.conversation.lock().unwrap().clear();
    chat_emit(state, "chat:reset", json!({}));
}

pub async fn speak_error(state: &ServerState, text: &str) {
    chat_emit(state, "chat:error", json!({"text": text}));
    emit_agent_state(state, BRAIN_KIND, "alert", Some(text));
    speak(state, BRAIN_KIND, text, Priority::High, SpeechKind::Alert).await;
}

/// Answer `question` out loud, then offer or start a tour. Used by the chat box, VS Code and REST.
pub async fn ask_and_present(state: Arc<ServerState>, question: String) -> Option<Answer> {
    ask_and_present_as(state, question, None).await
}

/// Like `ask_and_present`, but the chat list shows `shown` as the user's message (for example the
/// label of a button) while the model gets the full `question`.
pub async fn ask_and_present_as(state: Arc<ServerState>, question: String, shown: Option<String>) -> Option<Answer> {
    chat_emit(&state, "chat:user", json!({"text": shown.unwrap_or_else(|| question.clone())}));
    let cfg = crate::config::load_or_create_config().ok()?;
    let key = match key_for(&cfg.brain) {
        Ok(k) => k,
        Err(_) => {
            open_settings_window(&state);
            speak_error(&state, "I need a Meta API key first. I opened settings.").await;
            return None;
        }
    };
    let Some(root) = pick_project(&state) else {
        speak_error(&state, "Which project? Open it in VS Code and ask again.").await;
        return None;
    };

    emit_agent_state(&state, BRAIN_KIND, "thinking", Some(&format!("Asked: {question}")));
    let editor = state.hub.editor_context();
    let history = state.conversation.lock().unwrap().history_for(&root);
    match ask(&cfg.brain, &key, &root, &question, editor.as_ref(), &history).await {
        Ok(answer) => {
            state
                .conversation
                .lock()
                .unwrap()
                .record(Turn { question: question.clone(), speech: answer.speech.clone(), tour: answer.tour.clone() });

            let mut roots = state.hub.workspace_folders();
            roots.push(root.to_string_lossy().to_string());
            let steps = crate::tours::validate_steps(answer.tour.clone(), &roots);
            let tour_now = !steps.is_empty() && wants_tour(&question);
            let tour_offered = !steps.is_empty() && !tour_now;
            let agenda: Vec<String> = steps.iter().map(TourStep::label).collect();

            // The chat list shows the full answer and the map of what comes next; the voice reads
            // the answer out.
            chat_emit(
                &state,
                "chat:answer",
                json!({"speech": answer.speech, "agenda": agenda, "tour_started": tour_now}),
            );
            emit_agent_state(&state, BRAIN_KIND, "talking", None);
            speak(&state, BRAIN_KIND, &answer.speech, Priority::High, SpeechKind::Answer).await;

            let tour = crate::tours::Tour { title: "Tour".into(), steps, recap: answer.recap.clone() };
            if tour_now {
                state.tours.start(BRAIN_KIND, tour, crate::server::pet_colour(BRAIN_KIND)).await;
            } else if tour_offered {
                state.tours.set_pending(tour);
            }
            Some(answer)
        }
        Err(e) => {
            tracing::error!("Brain error: {e}");
            speak_error(&state, "I couldn't reach my brain just now. Try again in a moment.").await;
            None
        }
    }
}

/// Settings "Test" button: one tiny request, with an error a person can act on.
pub async fn test_connection() -> Result<String, String> {
    let cfg = crate::config::load_or_create_config().map_err(|e| e.to_string())?;
    let key = key_for(&cfg.brain).map_err(|_| "No key saved yet. Paste one above and press Save.".to_string())?;
    match chat(&cfg.brain, &key, &[json!({"role": "user", "content": "Reply with the word ok."})], false).await {
        Ok(msg) => {
            let reply = msg.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string();
            Ok(if reply.is_empty() {
                format!("Key works. {} answered.", cfg.brain.model)
            } else {
                format!("Key works. {} answered: {}", cfg.brain.model, reply.chars().take(40).collect::<String>())
            })
        }
        Err(e) if e.contains("401") || e.contains("403") => Err(
            "The API rejected this key. Copy the whole key again from dev.meta.ai, press Save, then Test.".into(),
        ),
        Err(e) if e.starts_with("request failed") => Err(format!(
            "Could not reach {}. Check your internet connection. ({e})",
            cfg.brain.base_url
        )),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_fenced_and_chatty_json() {
        let plain = r#"{"speech":"Login is in auth.","tour":[]}"#;
        assert_eq!(parse_answer(plain).unwrap().speech, "Login is in auth.");
        let fenced = "```json\n{\"speech\":\"Hi.\",\"tour\":[{\"path\":\"a.ts\",\"start_line\":1,\"end_line\":2,\"title\":\"The start\",\"narration\":\"n\"}],\"recap\":\"It starts here.\"}\n```";
        let parsed = parse_answer(fenced).unwrap();
        assert_eq!(parsed.tour.len(), 1);
        assert_eq!(parsed.tour[0].title, "The start");
        assert_eq!(parsed.recap, "It starts here.");
        // The title and the recap are optional: an older reply shape still works.
        assert_eq!(parse_answer(plain).unwrap().recap, "");
        assert!(parse_answer("Sure! {\"speech\":\"x\"} done").is_ok());
        assert!(parse_answer("no json here").is_err());
        assert!(parse_answer("{\"speech\":\"\"}").is_err());
    }

    #[test]
    fn editor_note_describes_a_selection_inside_the_project_only() {
        let root = Path::new("C:\\code\\app");
        let selected = json!({
            "path": "C:\\code\\app\\src\\auth\\validate.ts", "selection_text": "if (!ok) throw e;\n",
            "start_line": 12, "end_line": 13, "cursor_line": 13
        });
        let note = editor_note(&selected, root).unwrap();
        assert!(note.contains("lines 12 to 13 of src/auth/validate.ts"));
        assert!(note.contains("if (!ok) throw e;"));
        assert!(note.contains("they mean this selection"));

        let cursor_only = json!({"path": "c:/code/app/README.md", "selection_text": "", "cursor_line": 4});
        assert_eq!(editor_note(&cursor_only, root).unwrap(), "The user has README.md open, with the cursor on line 4.");

        // A file outside the project is never described, so its code is never sent.
        let elsewhere = json!({"path": "C:\\other\\secret.ts", "selection_text": "password = 1"});
        assert!(editor_note(&elsewhere, root).is_none());
        assert!(editor_note(&json!({}), root).is_none());
    }

    #[test]
    fn conversation_keeps_recent_turns_and_forgets_on_a_new_project() {
        let mut conv = Conversation::default();
        let a = Path::new("C:\\a");
        assert!(conv.history_for(a).is_empty());
        for i in 0..10 {
            conv.record(Turn { question: format!("q{i}"), speech: format!("a{i}"), tour: vec![] });
        }
        let kept = conv.history_for(a);
        assert_eq!(kept.len(), MAX_TURNS);
        assert_eq!(kept[0].question, "q2");
        assert!(conv.history_for(Path::new("C:\\b")).is_empty());
    }

    #[test]
    fn messages_put_history_between_the_system_prompt_and_the_new_question() {
        let stop = TourStep { path: "src/auth.ts".into(), start_line: 1, end_line: 5, title: "The check".into(), narration: "It checks.".into() };
        let history = vec![Turn { question: "where is login?".into(), speech: "In auth.".into(), tour: vec![stop] }];
        let msgs = build_messages("sys".into(), &history, "why?".into());
        let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert!(msgs[2]["content"].as_str().unwrap().contains("In auth."));
        // The stops he showed are remembered, so "tell me more about the second one" has something to mean.
        assert!(msgs[2]["content"].as_str().unwrap().contains("The check"));
        assert_eq!(msgs[3]["content"], "why?");
    }

    #[test]
    fn the_prompt_asks_for_the_teaching_shape() {
        // Plain English, answer and map first, few steps with titles, one thing to remember.
        for needle in ["PLAIN ENGLISH", "never more than 5", "\"title\"", "recap", "Do not read code aloud", "do not give the overview again"] {
            assert!(SYSTEM_PROMPT.contains(needle), "the prompt lost: {needle}");
        }
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
