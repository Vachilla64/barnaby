use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;

enum AudioCommand {
    Play {
        wav_bytes: Vec<u8>,
        cancel_token: Arc<AtomicBool>,
        done_tx: tokio::sync::oneshot::Sender<bool>,
    },
    Stop,
    SetVolume(f32),
}

/// Thread-safe audio player wrapping rodio in a dedicated worker thread.
#[derive(Clone)]
pub struct AudioPlayer {
    cmd_tx: Sender<AudioCommand>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = channel::<AudioCommand>();

        thread::Builder::new()
            .name("devy-audio-worker".into())
            .spawn(move || {
                // Initialize default output stream on this audio thread
                let stream_res = OutputStream::try_default();
                let (_stream, stream_handle) = match stream_res {
                    Ok((s, h)) => (Some(s), Some(h)),
                    Err(e) => {
                        tracing::error!("Failed to initialize audio output stream: {}", e);
                        (None, None)
                    }
                };

                let mut current_sink: Option<Sink> = None;
                let mut current_volume: f32 = 0.9;

                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        AudioCommand::Play {
                            wav_bytes,
                            cancel_token,
                            done_tx,
                        } => {
                            // If user cancelled before play started
                            if cancel_token.load(Ordering::Relaxed) {
                                let _ = done_tx.send(false);
                                continue;
                            }

                            // Stop previous sink if any
                            if let Some(old_sink) = current_sink.take() {
                                old_sink.stop();
                            }

                            let Some(handle) = &stream_handle else {
                                tracing::warn!("No audio output handle available, skipping playback");
                                let _ = done_tx.send(false);
                                continue;
                            };

                            let sink_res = Sink::try_new(handle);
                            let sink = match sink_res {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::error!("Failed to create audio sink: {}", e);
                                    let _ = done_tx.send(false);
                                    continue;
                                }
                            };

                            sink.set_volume(current_volume);

                            let cursor = Cursor::new(wav_bytes);
                            match Decoder::new(cursor) {
                                Ok(source) => {
                                    sink.append(source);
                                    sink.play();

                                    // Wait until sound finishes or cancellation is requested
                                    let mut completed = true;
                                    while !sink.empty() {
                                        if cancel_token.load(Ordering::Relaxed) {
                                            sink.stop();
                                            completed = false;
                                            break;
                                        }
                                        thread::sleep(std::time::Duration::from_millis(15));
                                    }

                                    let _ = done_tx.send(completed);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to decode WAV audio: {}", e);
                                    let _ = done_tx.send(false);
                                }
                            }

                            current_sink = Some(sink);
                        }
                        AudioCommand::Stop => {
                            if let Some(sink) = current_sink.take() {
                                sink.stop();
                            }
                        }
                        AudioCommand::SetVolume(vol) => {
                            current_volume = vol.clamp(0.0, 1.0);
                            if let Some(sink) = &current_sink {
                                sink.set_volume(current_volume);
                            }
                        }
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn audio worker thread: {}", e))?;

        Ok(Self { cmd_tx })
    }

    pub fn play(
        &self,
        wav_bytes: Vec<u8>,
        cancel_token: Arc<AtomicBool>,
    ) -> tokio::sync::oneshot::Receiver<bool> {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(AudioCommand::Play {
            wav_bytes,
            cancel_token,
            done_tx,
        });
        done_rx
    }

    pub fn stop(&self) {
        let _ = self.cmd_tx.send(AudioCommand::Stop);
    }

    pub fn set_volume(&self, volume: f32) {
        let _ = self.cmd_tx.send(AudioCommand::SetVolume(volume));
    }
}
