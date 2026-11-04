use super::phonon::PhononManager;
use super::playback::AudioPlayer;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeechKind {
    Status,
    Answer,
    Tour,
    Alert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub agent_id: String,
    pub text: String,
    pub priority: Priority,
    pub kind: SpeechKind,
    #[serde(default)]
    pub voice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechStartEvent {
    pub agent_id: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechEndEvent {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechBubbleEvent {
    pub agent_id: String,
    pub text: String,
}

/// Pure sentence splitter that respects abbreviations, numbers, and common code extensions.
pub struct SentenceSplitter;

impl SentenceSplitter {
    pub fn split(text: &str) -> Vec<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let mut sentences = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = trimmed.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let c = chars[i];
            current.push(c);

            // Potential sentence boundary: '.', '!', '?', or newline
            if c == '.' || c == '!' || c == '?' || c == '\n' {
                // If followed by newline or end of string, or space + uppercase/quote
                let is_end_of_string = i + 1 >= len;
                let next_is_whitespace = !is_end_of_string && chars[i + 1].is_whitespace();

                if is_end_of_string || next_is_whitespace {
                    // Check if this period is part of an abbreviation or number or code file
                    if c == '.' && Self::is_abbreviation_or_number(&current, &chars, i) {
                        i += 1;
                        continue;
                    }

                    let candidate = current.trim().to_string();
                    if !candidate.is_empty() {
                        sentences.push(candidate);
                    }
                    current.clear();

                    // Skip subsequent whitespace
                    while i + 1 < len && chars[i + 1].is_whitespace() {
                        i += 1;
                    }
                }
            }
            i += 1;
        }

        let remaining = current.trim().to_string();
        if !remaining.is_empty() {
            sentences.push(remaining);
        }

        if sentences.is_empty() && !trimmed.is_empty() {
            sentences.push(trimmed.to_string());
        }

        sentences
    }

    fn is_abbreviation_or_number(current: &str, chars: &[char], dot_idx: usize) -> bool {
        let words: Vec<&str> = current.split_whitespace().collect();
        let last_word = words.last().copied().unwrap_or("");
        let lower = last_word.to_lowercase();

        // Common abbreviations
        let abbrevs = [
            "e.g.", "eg.", "i.e.", "ie.", "mr.", "mrs.", "dr.", "ms.", "vs.", "etc.",
            "approx.", "dept.", "est.", "fig.", "inc.", "ltd.", "no.", "oz.", "sq.",
        ];
        for abbr in &abbrevs {
            if lower == *abbr || lower.ends_with(abbr) {
                return true;
            }
        }

        // Single letter abbreviation like "U.S." or initials "J."
        if last_word.len() <= 2 && last_word.chars().all(|c| c.is_alphabetic() || c == '.') {
            return true;
        }

        // Version numbers like "v1.0" or "v0.1"
        if lower.starts_with('v') && lower[1..].chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }

        // Decimal numbers like "3.14" (previous and next chars are digits)
        let prev_is_digit = dot_idx > 0 && chars[dot_idx - 1].is_ascii_digit();
        let next_is_digit = dot_idx + 1 < chars.len() && chars[dot_idx + 1].is_ascii_digit();
        if prev_is_digit && next_is_digit {
            return true;
        }

        // IP address or port like "127.0.0.1:8001"
        if prev_is_digit && dot_idx + 1 < chars.len() && (chars[dot_idx + 1].is_ascii_digit() || chars[dot_idx + 1] == '.') {
            return true;
        }

        // Code file extensions like "validate.ts" or "main.rs"
        let code_extensions = [
            ".ts", ".js", ".rs", ".py", ".json", ".html", ".css", ".go", ".c", ".cpp", ".h",
        ];
        if dot_idx + 1 < chars.len() {
            let rest: String = chars[dot_idx..].iter().take(6).collect();
            for ext in &code_extensions {
                if rest.starts_with(ext) {
                    return true;
                }
            }
        }

        false
    }
}

/// Pure throttle tracker: a pet speaks at most one low/normal status per 20 seconds.
pub struct ThrottleTracker {
    last_status_spoken: HashMap<String, Instant>,
    throttle_duration: Duration,
}

impl ThrottleTracker {
    pub fn new(throttle_duration: Duration) -> Self {
        Self {
            last_status_spoken: HashMap::new(),
            throttle_duration,
        }
    }

    /// Evaluates if an utterance should be voiced or throttled.
    /// Returns true if it should be spoken out loud, false if it should be throttled (bubble only).
    pub fn should_speak(&mut self, agent_id: &str, priority: Priority, kind: SpeechKind, now: Instant) -> bool {
        // High priority items (alerts, answers, tours) are never throttled
        if priority == Priority::High || kind != SpeechKind::Status {
            return true;
        }

        // Low or Normal status: enforce throttle duration
        if let Some(last) = self.last_status_spoken.get(agent_id) {
            if now.duration_since(*last) < self.throttle_duration {
                return false;
            }
        }

        self.last_status_spoken.insert(agent_id.to_string(), now);
        true
    }

    pub fn reset(&mut self, agent_id: &str) {
        self.last_status_spoken.remove(agent_id);
    }
}

struct QueuedItem {
    agent_id: String,
    sentence: String,
    priority: Priority,
    voice: String,
}

struct QueueState {
    items: VecDeque<QueuedItem>,
    current_priority: Option<Priority>,
    current_agent: Option<String>,
    is_playing: bool,
    cancel_token: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct SpeechQueue {
    state: Arc<Mutex<QueueState>>,
    throttle: Arc<std::sync::Mutex<ThrottleTracker>>,
    phonon: Arc<PhononManager>,
    player: AudioPlayer,
    app_handle: Option<AppHandle>,
}

impl SpeechQueue {
    pub fn new(
        phonon: Arc<PhononManager>,
        player: AudioPlayer,
        app_handle: Option<AppHandle>,
    ) -> Self {
        let initial_state = QueueState {
            items: VecDeque::new(),
            current_priority: None,
            current_agent: None,
            is_playing: false,
            cancel_token: Arc::new(AtomicBool::new(false)),
        };

        Self {
            state: Arc::new(Mutex::new(initial_state)),
            throttle: Arc::new(std::sync::Mutex::new(ThrottleTracker::new(Duration::from_secs(20)))),
            phonon,
            player,
            app_handle,
        }
    }

    /// Enqueue speech request. Evaluates throttle, splits sentences, and triggers playback pipeline.
    pub async fn enqueue(&self, req: SpeechRequest) {
        let now = Instant::now();
        let should_voice = {
            let mut th = self.throttle.lock().unwrap();
            th.should_speak(&req.agent_id, req.priority, req.kind, now)
        };

        if !should_voice {
            tracing::info!(
                "Throttling spoken audio for agent '{}' status; updating bubble only",
                req.agent_id
            );
            if let Some(app) = &self.app_handle {
                let _ = app.emit(
                    "speech:bubble",
                    SpeechBubbleEvent {
                        agent_id: req.agent_id.clone(),
                        text: req.text.clone(),
                    },
                );
            }
            return;
        }

        let sentences = SentenceSplitter::split(&req.text);
        if sentences.is_empty() {
            return;
        }

        let voice = req.voice.unwrap_or_else(|| "Toby".to_string());

        let mut st = self.state.lock().await;

        // If high priority and currently lower priority is playing, barge-in / interrupt!
        if req.priority == Priority::High {
            if let Some(curr_pri) = st.current_priority {
                if curr_pri < Priority::High {
                    tracing::info!("Barge-in: High priority interrupting active speech");
                    st.cancel_token.store(true, Ordering::SeqCst);
                    self.player.stop();
                    st.items.clear();
                }
            }
        }

        for sentence in sentences {
            st.items.push_back(QueuedItem {
                agent_id: req.agent_id.clone(),
                sentence,
                priority: req.priority,
                voice: voice.clone(),
            });
        }

        if !st.is_playing {
            st.is_playing = true;
            drop(st);
            self.start_pipeline();
        }
    }

    /// Stop speech immediately, clear queue, and cancel playback.
    pub async fn stop_speech(&self) {
        let mut st = self.state.lock().await;
        st.cancel_token.store(true, Ordering::SeqCst);
        self.player.stop();
        st.items.clear();
        st.is_playing = false;
        st.current_priority = None;

        if let Some(agent) = st.current_agent.take() {
            if let Some(app) = &self.app_handle {
                let _ = app.emit("speech:end", SpeechEndEvent { agent_id: agent });
            }
        }
    }

    /// Playback pipeline loop with prefetching of next sentence.
    fn start_pipeline(&self) {
        let queue_clone = self.clone();

        tokio::spawn(async move {
            let mut prefetched: Option<(QueuedItem, Result<Vec<u8>, String>)> = None;

            loop {
                // Fetch next item from queue if not already prefetched
                let next_item = {
                    let mut st = queue_clone.state.lock().await;
                    if let Some((item, audio_res)) = prefetched.take() {
                        st.current_priority = Some(item.priority);
                        st.current_agent = Some(item.agent_id.clone());
                        st.cancel_token = Arc::new(AtomicBool::new(false));
                        Some((item, audio_res))
                    } else if let Some(item) = st.items.pop_front() {
                        st.current_priority = Some(item.priority);
                        st.current_agent = Some(item.agent_id.clone());
                        st.cancel_token = Arc::new(AtomicBool::new(false));
                        drop(st);

                        // Synthesize current item
                        let audio = queue_clone.phonon.synthesize(&item.sentence, &item.voice).await;
                        Some((item, audio))
                    } else {
                        st.is_playing = false;
                        st.current_priority = None;
                        st.current_agent = None;
                        None
                    }
                };

                let Some((item, audio_res)) = next_item else {
                    break;
                };

                let cancel_token = {
                    let st = queue_clone.state.lock().await;
                    st.cancel_token.clone()
                };

                // Emit speech:start event to Tauri UI
                if let Some(app) = &queue_clone.app_handle {
                    let _ = app.emit(
                        "speech:start",
                        SpeechStartEvent {
                            agent_id: item.agent_id.clone(),
                            sentence: item.sentence.clone(),
                        },
                    );
                }

                // Lookahead to prefetch sentence N+1 while sentence N plays
                let next_in_queue = {
                    let st = queue_clone.state.lock().await;
                    st.items.front().map(|it| QueuedItem {
                        agent_id: it.agent_id.clone(),
                        sentence: it.sentence.clone(),
                        priority: it.priority,
                        voice: it.voice.clone(),
                    })
                };

                let prefetch_handle = if let Some(next_it) = next_in_queue {
                    let phonon = queue_clone.phonon.clone();
                    Some(tokio::spawn(async move {
                        let audio = phonon.synthesize(&next_it.sentence, &next_it.voice).await;
                        (next_it, audio)
                    }))
                } else {
                    None
                };

                // Play sentence N
                match audio_res {
                    Ok(wav_bytes) => {
                        let done_rx = queue_clone.player.play(wav_bytes, cancel_token.clone());
                        let _ = done_rx.await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Phonon synthesis failed for '{}' ({}); falling back to bubble-only timing",
                            item.sentence,
                            e
                        );
                        // Fallback: Show bubble without audio; never block
                        let char_delay = (item.sentence.len() as u64) * 50;
                        let duration_ms = char_delay.clamp(1800, 5000);
                        let mut elapsed = 0;
                        while elapsed < duration_ms {
                            if cancel_token.load(Ordering::Relaxed) {
                                break;
                            }
                            sleep(Duration::from_millis(50)).await;
                            elapsed += 50;
                        }
                    }
                }

                // Emit speech:end event to Tauri UI
                if let Some(app) = &queue_clone.app_handle {
                    let _ = app.emit(
                        "speech:end",
                        SpeechEndEvent {
                            agent_id: item.agent_id.clone(),
                        },
                    );
                }

                // If prefetch was running, await and store it
                if let Some(handle) = prefetch_handle {
                    if let Ok((pf_item, pf_audio)) = handle.await {
                        // Check if items queue was cleared during playback
                        let mut st = queue_clone.state.lock().await;
                        if !st.items.is_empty() && st.items.front().map(|it| &it.sentence) == Some(&pf_item.sentence) {
                            st.items.pop_front();
                            prefetched = Some((pf_item, pf_audio));
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence_splitter_basic() {
        let text = "Hello there! How are you doing today? I am doing great.";
        let sentences = SentenceSplitter::split(text);
        assert_eq!(
            sentences,
            vec![
                "Hello there!",
                "How are you doing today?",
                "I am doing great."
            ]
        );
    }

    #[test]
    fn test_sentence_splitter_abbreviations_and_numbers() {
        let text = "We loaded v1.0 e.g. for Mr. Smith at 127.0.0.1:8001. That cost $3.14! Please verify auth.ts now.";
        let sentences = SentenceSplitter::split(text);
        assert_eq!(
            sentences,
            vec![
                "We loaded v1.0 e.g. for Mr. Smith at 127.0.0.1:8001.",
                "That cost $3.14!",
                "Please verify auth.ts now."
            ]
        );
    }

    #[test]
    fn test_sentence_splitter_newlines_and_whitespace() {
        let text = "First line.\nSecond line.\n\nThird line without dot";
        let sentences = SentenceSplitter::split(text);
        assert_eq!(
            sentences,
            vec!["First line.", "Second line.", "Third line without dot"]
        );
    }

    #[test]
    fn test_throttle_tracker_normal_status_throttled_within_20s() {
        let mut tracker = ThrottleTracker::new(Duration::from_secs(20));
        let start = Instant::now();

        // First status should speak
        assert!(tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, start));

        // Status after 5s should be throttled
        let at_5s = start + Duration::from_secs(5);
        assert!(!tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, at_5s));

        // Status after 19s should still be throttled
        let at_19s = start + Duration::from_secs(19);
        assert!(!tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, at_19s));

        // Status after 21s should speak
        let at_21s = start + Duration::from_secs(21);
        assert!(tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, at_21s));
    }

    #[test]
    fn test_throttle_tracker_high_priority_never_throttled() {
        let mut tracker = ThrottleTracker::new(Duration::from_secs(20));
        let start = Instant::now();

        // First status speaks
        assert!(tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, start));

        // Alert at high priority immediately after should speak
        let at_1s = start + Duration::from_secs(1);
        assert!(tracker.should_speak("claude-code", Priority::High, SpeechKind::Alert, at_1s));

        // Answer at high priority should speak
        let at_2s = start + Duration::from_secs(2);
        assert!(tracker.should_speak("claude-code", Priority::High, SpeechKind::Answer, at_2s));
    }

    #[test]
    fn test_throttle_tracker_different_agents_independent() {
        let mut tracker = ThrottleTracker::new(Duration::from_secs(20));
        let start = Instant::now();

        // Claude speaks
        assert!(tracker.should_speak("claude-code", Priority::Normal, SpeechKind::Status, start));

        // Gemini speaks at 1s - independent of Claude!
        let at_1s = start + Duration::from_secs(1);
        assert!(tracker.should_speak("gemini-cli", Priority::Normal, SpeechKind::Status, at_1s));
    }
}
