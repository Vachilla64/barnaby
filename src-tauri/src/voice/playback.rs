use super::phonon::PcmStream;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Audio to buffer before playing, so a small hiccup in synthesis does not stutter the first words.
const PREBUFFER: Duration = Duration::from_millis(150);
/// Give up waiting for the first audio after this long.
const FIRST_AUDIO_TIMEOUT: Duration = Duration::from_secs(20);
/// An output that has not asked for audio for this long is dead (device unplugged, driver reset).
const STALL_AFTER: Duration = Duration::from_millis(700);
/// A freshly opened output (a Bluetooth speaker waking up, say) may take longer than that to ask
/// for its first audio.
const START_GRACE: Duration = Duration::from_secs(3);
/// How often to check whether Windows' default device has changed while a sentence plays.
const DEVICE_CHECK_EVERY: Duration = Duration::from_millis(450);
/// After switching device, start this far back so a word cut off by the switch is heard whole.
const REWIND: Duration = Duration::from_millis(200);
const LOCAL_CHUNK: usize = 4096;

/// The audio of one sentence as it arrives. It is kept (not thrown away as it plays) so playback can
/// carry on from the same place on another output device.
struct Shared {
    samples: Mutex<Vec<i16>>,
    /// The synthesizer has sent everything.
    finished: AtomicBool,
    /// Index of the next sample the output will be given.
    handed: AtomicUsize,
    /// Counts every request the output makes (silence included). It stops when the output dies.
    pulls: AtomicU64,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            samples: Mutex::new(Vec::new()),
            finished: AtomicBool::new(false),
            handed: AtomicUsize::new(0),
            pulls: AtomicU64::new(0),
        })
    }

    fn len(&self) -> usize {
        self.samples.lock().unwrap().len()
    }

    /// Move everything the synthesizer has sent so far into the buffer.
    fn drain(&self, rx: &Receiver<Vec<i16>>) {
        loop {
            match rx.try_recv() {
                Ok(chunk) => self.samples.lock().unwrap().extend(chunk),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.finished.store(true, Ordering::Release);
                    return;
                }
            }
        }
    }
}

/// A rodio source that plays `Shared` from `start`. If playback catches up with synthesis it plays
/// silence instead of ending; it ends only when the synthesizer is done and every sample is played.
struct StreamSource {
    shared: Arc<Shared>,
    /// Next index of `shared` to copy into `local`.
    pos: usize,
    local: Vec<i16>,
    local_pos: usize,
    sample_rate: u32,
}

impl StreamSource {
    fn new(shared: Arc<Shared>, start: usize, sample_rate: u32) -> Self {
        shared.handed.store(start, Ordering::Relaxed);
        Self { shared, pos: start, local: Vec::new(), local_pos: 0, sample_rate }
    }
}

impl Iterator for StreamSource {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        self.shared.pulls.fetch_add(1, Ordering::Relaxed);
        if self.local_pos >= self.local.len() {
            let samples = self.shared.samples.lock().unwrap();
            // Read `finished` before the length: if it is set, everything is already in the buffer.
            let finished = self.shared.finished.load(Ordering::Acquire);
            if self.pos >= samples.len() {
                return if finished { None } else { Some(0) };
            }
            let end = (self.pos + LOCAL_CHUNK).min(samples.len());
            self.local.clear();
            self.local.extend_from_slice(&samples[self.pos..end]);
            self.local_pos = 0;
            self.pos = end;
        }
        let sample = self.local[self.local_pos];
        self.local_pos += 1;
        let handed = self.pos - (self.local.len() - self.local_pos);
        self.shared.handed.store(handed, Ordering::Relaxed);
        Some(sample)
    }
}

impl Source for StreamSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Notices an output that has stopped asking for audio.
struct Watchdog {
    pulls: u64,
    moved_at: Instant,
}

impl Watchdog {
    fn new(now: Instant) -> Self {
        Self { pulls: 0, moved_at: now }
    }

    /// True when the output has made no request for `STALL_AFTER` (`START_GRACE` before its first).
    fn stalled(&mut self, pulls: u64, now: Instant) -> bool {
        if pulls != self.pulls {
            self.pulls = pulls;
            self.moved_at = now;
            return false;
        }
        let limit = if pulls == 0 { START_GRACE } else { STALL_AFTER };
        now.duration_since(self.moved_at) > limit
    }
}

/// Collect the first ~150 ms of audio. Returns false if nothing ever arrived or it was cancelled.
fn prebuffer(stream: &PcmStream, shared: &Shared, cancel: &AtomicBool) -> bool {
    let wanted = (stream.sample_rate as u128 * PREBUFFER.as_millis() / 1000) as usize;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) || started.elapsed() > FIRST_AUDIO_TIMEOUT {
            return false;
        }
        match stream.rx.recv_timeout(Duration::from_millis(15)) {
            Ok(chunk) => {
                shared.samples.lock().unwrap().extend(chunk);
                if shared.len() >= wanted {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                shared.finished.store(true, Ordering::Release);
                break;
            }
        }
    }
    shared.len() > 0
}

enum AudioCommand {
    Play {
        stream: PcmStream,
        cancel_token: Arc<AtomicBool>,
        done_tx: tokio::sync::oneshot::Sender<bool>,
    },
    Stop,
    SetVolume(f32),
}

/// An open audio output, and the name of the device it was opened on.
struct Output {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    device: String,
}

fn default_device_name() -> Option<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    rodio::cpal::default_host().default_output_device().and_then(|d| d.name().ok())
}

fn open_output() -> Option<Output> {
    let device = default_device_name().unwrap_or_default();
    match OutputStream::try_default() {
        Ok((_stream, handle)) => {
            tracing::info!("Audio output opened on '{device}'");
            Some(Output { _stream, handle, device })
        }
        Err(e) => {
            tracing::error!("Failed to open the audio output: {e}");
            None
        }
    }
}

/// Close the output and open a fresh one on whatever Windows' default device is now. A device that
/// has just been plugged in can take a moment, so try a few times.
fn reopen_output(output: &mut Option<Output>) -> bool {
    drop(output.take());
    for attempt in 0..4 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(250));
        }
        *output = open_output();
        if output.is_some() {
            return true;
        }
    }
    false
}

/// True when there is no output yet, or Windows' default device is no longer the one it uses.
fn output_is_stale(output: &Option<Output>) -> bool {
    match output {
        None => true,
        Some(o) => default_device_name().is_some_and(|now| now != o.device),
    }
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
            .name("barnaby-audio-worker".into())
            .spawn(move || {
                // The output stream lives on this audio thread. It is reopened whenever Windows
                // switches the default device (earphones unplugged, a Bluetooth speaker connecting),
                // and also when it simply stops working: a headphone jack or a virtual device such as
                // FxSound keeps its name while the stream behind it dies, so the name alone is not
                // enough.
                let mut output = open_output();

                let mut current_sink: Option<Sink> = None;
                let mut current_volume: f32 = 0.9;

                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        AudioCommand::Play {
                            stream,
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

                            let shared = Shared::new();
                            if !prebuffer(&stream, &shared, &cancel_token) {
                                let _ = done_tx.send(false);
                                continue;
                            }

                            if output_is_stale(&output) && !reopen_output(&mut output) {
                                tracing::warn!("No audio output device available, skipping playback");
                                let _ = done_tx.send(false);
                                continue;
                            }
                            let Some(sink) = new_sink(&output, current_volume, &shared, 0, stream.sample_rate) else {
                                let _ = done_tx.send(false);
                                continue;
                            };
                            let mut sink = sink;

                            // Wait until the sentence has finished playing or cancellation is requested.
                            let rx = &stream.rx;
                            let mut completed = true;
                            let mut watchdog = Watchdog::new(Instant::now());
                            let mut last_device_check = Instant::now();
                            while !sink.empty() {
                                if cancel_token.load(Ordering::Relaxed) {
                                    sink.stop();
                                    completed = false;
                                    break;
                                }
                                shared.drain(rx);
                                thread::sleep(Duration::from_millis(15));

                                let now = Instant::now();
                                let stalled = watchdog.stalled(shared.pulls.load(Ordering::Relaxed), now);
                                let moved = now.duration_since(last_device_check) >= DEVICE_CHECK_EVERY && {
                                    last_device_check = now;
                                    output_is_stale(&output)
                                };
                                if !stalled && !moved {
                                    continue;
                                }

                                // The output died or Windows switched device. Carry on from where we were.
                                tracing::warn!(
                                    "Audio output {}; reopening it and carrying on",
                                    if stalled { "stopped responding" } else { "changed" }
                                );
                                sink.stop();
                                let rewind = (stream.sample_rate as u128 * REWIND.as_millis() / 1000) as usize;
                                let resume = shared.handed.load(Ordering::Relaxed).saturating_sub(rewind);
                                if !reopen_output(&mut output) {
                                    completed = false;
                                    break;
                                }
                                match new_sink(&output, current_volume, &shared, resume, stream.sample_rate) {
                                    Some(s) => sink = s,
                                    None => {
                                        completed = false;
                                        break;
                                    }
                                }
                                watchdog = Watchdog::new(Instant::now());
                                last_device_check = Instant::now();
                            }
                            let _ = done_tx.send(completed);

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
        stream: PcmStream,
        cancel_token: Arc<AtomicBool>,
    ) -> tokio::sync::oneshot::Receiver<bool> {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(AudioCommand::Play {
            stream,
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

/// A sink on the open output that plays the sentence from sample `start`.
fn new_sink(output: &Option<Output>, volume: f32, shared: &Arc<Shared>, start: usize, sample_rate: u32) -> Option<Sink> {
    let out = output.as_ref()?;
    match Sink::try_new(&out.handle) {
        Ok(sink) => {
            sink.set_volume(volume);
            sink.append(StreamSource::new(shared.clone(), start, sample_rate));
            sink.play();
            Some(sink)
        }
        Err(e) => {
            tracing::error!("Failed to create audio sink: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(shared: &Shared, samples: &[i16], finished: bool) {
        shared.samples.lock().unwrap().extend_from_slice(samples);
        if finished {
            shared.finished.store(true, Ordering::Release);
        }
    }

    fn take(source: &mut StreamSource, n: usize) -> Vec<i16> {
        (0..n).filter_map(|_| source.next()).collect()
    }

    #[test]
    fn plays_in_order_then_ends_when_finished() {
        let shared = Shared::new();
        feed(&shared, &[1, 2, 3], true);
        let mut src = StreamSource::new(shared, 0, 24000);
        assert_eq!(take(&mut src, 3), vec![1, 2, 3]);
        assert_eq!(src.next(), None);
    }

    #[test]
    fn plays_silence_instead_of_ending_while_audio_is_still_coming() {
        let shared = Shared::new();
        feed(&shared, &[7], false);
        let mut src = StreamSource::new(shared.clone(), 0, 24000);
        assert_eq!(take(&mut src, 3), vec![7, 0, 0]);
        feed(&shared, &[8, 9], true);
        assert_eq!(take(&mut src, 3), vec![8, 9]);
        assert_eq!(src.next(), None);
    }

    #[test]
    fn a_new_source_resumes_where_the_old_one_stopped() {
        let shared = Shared::new();
        let all: Vec<i16> = (1..=10).collect();
        feed(&shared, &all, true);
        let mut old = StreamSource::new(shared.clone(), 0, 24000);
        assert_eq!(take(&mut old, 4), vec![1, 2, 3, 4]);
        // The device died here. The next device starts from what had been handed out.
        let resume = shared.handed.load(Ordering::Relaxed);
        assert_eq!(resume, 4);
        let mut fresh = StreamSource::new(shared, resume, 24000);
        assert_eq!(take(&mut fresh, 10), vec![5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn handed_stays_exact_across_chunk_boundaries() {
        let shared = Shared::new();
        let all: Vec<i16> = (0..(LOCAL_CHUNK as i32 * 2 + 10)).map(|i| (i % 1000) as i16).collect();
        feed(&shared, &all, true);
        let mut src = StreamSource::new(shared.clone(), 0, 24000);
        take(&mut src, LOCAL_CHUNK + 5);
        assert_eq!(shared.handed.load(Ordering::Relaxed), LOCAL_CHUNK + 5);
    }

    #[test]
    fn watchdog_only_fires_when_the_output_stops_asking() {
        let t0 = Instant::now();
        let mut dog = Watchdog::new(t0);
        // Healthy: the pull count keeps growing, however long it runs.
        assert!(!dog.stalled(100, t0 + Duration::from_millis(500)));
        assert!(!dog.stalled(900, t0 + Duration::from_millis(1500)));
        // Dead: the count freezes.
        assert!(!dog.stalled(900, t0 + Duration::from_millis(2000)));
        assert!(dog.stalled(900, t0 + Duration::from_millis(2300)));
    }

    #[test]
    fn a_new_output_gets_time_to_wake_up() {
        let t0 = Instant::now();
        let mut dog = Watchdog::new(t0);
        assert!(!dog.stalled(0, t0 + Duration::from_millis(2000)));
        assert!(dog.stalled(0, t0 + Duration::from_millis(3100)));
    }
}
