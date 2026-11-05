//! Voice in: push-to-talk audio -> Muse Voice Transcribe -> command words or a question.

use crate::server::ServerState;
use crate::tours::Control;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Tour(Control),
    Repeat,
    Mute,
}

/// Words that may follow a command word without making the sentence a question.
const FILLER: [&str; 12] = ["please", "step", "one", "now", "it", "that", "the", "tour", "thanks", "thank", "you", "again"];

const PHRASES: [(&str, Command); 22] = [
    ("next", Command::Tour(Control::Next)),
    ("next step", Command::Tour(Control::Next)),
    ("go next", Command::Tour(Control::Next)),
    ("skip", Command::Tour(Control::Next)),
    ("back", Command::Tour(Control::Back)),
    ("go back", Command::Tour(Control::Back)),
    ("previous", Command::Tour(Control::Back)),
    ("previous step", Command::Tour(Control::Back)),
    ("stop", Command::Tour(Control::Stop)),
    ("cancel", Command::Tour(Control::Stop)),
    ("enough", Command::Tour(Control::Stop)),
    ("end the tour", Command::Tour(Control::Stop)),
    ("pause", Command::Tour(Control::Pause)),
    ("hold on", Command::Tour(Control::Pause)),
    ("wait", Command::Tour(Control::Pause)),
    ("continue", Command::Tour(Control::Resume)),
    ("resume", Command::Tour(Control::Resume)),
    ("go on", Command::Tour(Control::Resume)),
    ("repeat", Command::Repeat),
    ("say that again", Command::Repeat),
    ("mute", Command::Mute),
    ("be quiet", Command::Mute),
];

fn normalise(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Claims a sentence only when it really is a command: the whole utterance is a command phrase,
/// or it starts with one and the rest is filler ("next please", "stop the tour").
/// Anything else is a question for the brain.
pub fn match_command(text: &str) -> Option<Command> {
    let words = normalise(text);
    if words.is_empty() || words.len() > 5 {
        return None;
    }
    // Longest phrase first so "next step" wins over "next".
    let mut phrases: Vec<&(&str, Command)> = PHRASES.iter().collect();
    phrases.sort_by_key(|(p, _)| std::cmp::Reverse(p.split(' ').count()));
    for (phrase, command) in phrases {
        let pw: Vec<&str> = phrase.split(' ').collect();
        if words.len() >= pw.len() && words.iter().zip(&pw).all(|(a, b)| a == b) {
            let rest = &words[pw.len()..];
            if rest.iter().all(|w| FILLER.contains(&w.as_str())) {
                return Some(*command);
            }
        }
    }
    None
}

// ---------- WAV ----------

pub fn is_wav(bytes: &[u8]) -> bool {
    bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

// ---------- Transcription ----------

/// `POST {base}/asr/transcribe`, multipart: JSON part `request`, file part `audio` (WAV, mono, 16-bit, 16 or 24 kHz).
pub async fn transcribe(base_url: &str, key: &str, wav: Vec<u8>) -> Result<String, String> {
    if !is_wav(&wav) {
        return Err("The recording was not a WAV file.".into());
    }
    let request = json!({"mode": "PUSH_TO_TALK", "model": "muse-voice-transcribe-1.0", "audioEncoding": "WAV"});
    let form = reqwest::multipart::Form::new()
        .part(
            "request",
            reqwest::multipart::Part::text(request.to_string())
                .mime_str("application/json")
                .map_err(|e| e.to_string())?,
        )
        .part(
            "audio",
            reqwest::multipart::Part::bytes(wav)
                .file_name("speech.wav")
                .mime_str("audio/wav")
                .map_err(|e| e.to_string())?,
        );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/asr/transcribe", base_url.trim_end_matches('/')))
        .bearer_auth(key)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("transcription request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("transcription error {status}: {}", text.chars().take(200).collect::<String>()));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("bad transcription reply: {e}"))?;
    Ok(v.get("transcript").and_then(Value::as_str).unwrap_or("").trim().to_string())
}

// ---------- Acting on an utterance ----------

fn emit(state: &ServerState, event: &str, payload: Value) {
    if let Some(app) = &state.app_handle {
        let _ = app.emit(event, payload);
    }
}

/// Run a recognised command word.
pub async fn run_command(state: &Arc<ServerState>, command: Command) {
    match command {
        Command::Tour(Control::Stop) => {
            crate::look::set(state, false, "cancelled");
            state.tours.control(Control::Stop).await;
            if let Some(q) = &state.speech_queue {
                q.stop_speech().await;
            }
        }
        Command::Tour(c) => state.tours.control(c).await,
        Command::Repeat => emit(state, "command:repeat", json!({})),
        Command::Mute => emit(state, "command:mute", json!({})),
    }
}

/// A finished recording arrives here: transcribe it, then run it as a command or ask the brain.
pub async fn handle_recording(state: Arc<ServerState>, wav: Vec<u8>) {
    let cfg = match crate::config::load_or_create_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("config: {e}");
            return;
        }
    };
    let Some(key) = crate::brain::load_api_key().or_else(|| (!cfg.brain.base_url.contains("meta.ai")).then(|| "local".to_string())) else {
        crate::brain::open_settings_window(&state);
        crate::brain::speak_error(&state, "I need a Meta API key first. I opened settings.").await;
        return;
    };
    emit(&state, "agent:state", json!({"agent_kind": "barnaby", "state": "thinking", "text": null}));
    match transcribe(&cfg.brain.base_url, &key, wav).await {
        Ok(text) if !text.is_empty() => {
            tracing::info!("Heard: {text}");
            match match_command(&text) {
                Some(command) => run_command(&state, command).await,
                None => {
                    crate::brain::ask_and_present(state, text).await;
                }
            }
        }
        other => {
            if let Err(e) = other {
                tracing::warn!("Transcription failed: {e}");
            }
            crate::brain::speak_error(&state, "I didn't catch that. You can type your question instead.").await;
            emit(&state, "ptt:fallback", json!({}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_utterance_commands() {
        assert_eq!(match_command("Next."), Some(Command::Tour(Control::Next)));
        assert_eq!(match_command("go back"), Some(Command::Tour(Control::Back)));
        assert_eq!(match_command("Stop!"), Some(Command::Tour(Control::Stop)));
        assert_eq!(match_command("pause"), Some(Command::Tour(Control::Pause)));
        assert_eq!(match_command("Continue"), Some(Command::Tour(Control::Resume)));
        assert_eq!(match_command("repeat that"), Some(Command::Repeat));
        assert_eq!(match_command("be quiet"), Some(Command::Mute));
    }

    #[test]
    fn command_with_filler_words() {
        assert_eq!(match_command("next step please"), Some(Command::Tour(Control::Next)));
        assert_eq!(match_command("stop the tour"), Some(Command::Tour(Control::Stop)));
        assert_eq!(match_command("say that again"), Some(Command::Repeat));
    }

    #[test]
    fn questions_are_not_claimed() {
        assert_eq!(match_command("Where is the login handled?"), None);
        assert_eq!(match_command("back button handler"), None);
        assert_eq!(match_command("stop words in the tokenizer, where are they"), None);
        assert_eq!(match_command("what does next do"), None);
        assert_eq!(match_command(""), None);
    }

    #[tokio::test]
    async fn transcribe_sends_multipart_and_reads_transcript() {
        use axum::{extract::Multipart, routing::post, Json, Router};
        async fn handler(mut mp: Multipart) -> Json<Value> {
            let mut parts = vec![];
            while let Some(field) = mp.next_field().await.unwrap() {
                let name = field.name().unwrap_or("").to_string();
                let bytes = field.bytes().await.unwrap();
                parts.push((name, bytes.len()));
            }
            let ok = parts.iter().any(|(n, l)| n == "request" && *l > 10) && parts.iter().any(|(n, l)| n == "audio" && *l > 40);
            Json(json!({"transcript": if ok { " where is the login handled " } else { "bad request" }}))
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/v1/asr/transcribe", post(handler))).await.unwrap();
        });

        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        wav.extend_from_slice(&[0u8; 60]);
        let text = transcribe(&format!("http://{addr}/v1"), "key", wav).await.unwrap();
        assert_eq!(text, "where is the login handled");
        assert!(transcribe(&format!("http://{addr}/v1"), "key", vec![1, 2, 3]).await.is_err());
    }

    #[test]
    fn wav_detection() {
        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        wav.extend_from_slice(&[0u8; 40]);
        assert!(is_wav(&wav));
        assert!(!is_wav(b"not audio"));
    }
}
