use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;
pub const PHONON_HEALTH_URL: &str = "http://127.0.0.1:8001/health";
pub const PHONON_TTS_URL: &str = "http://127.0.0.1:8001/tts";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PhononState {
    Ready,
    Starting,
    Installing,
    Offline,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhononStatusInfo {
    pub state: PhononState,
    pub uv_installed: bool,
    pub ptts_installed: bool,
    pub message: Option<String>,
}

#[cfg(windows)]
struct JobObjectWrapper(HANDLE);

#[cfg(windows)]
unsafe impl Send for JobObjectWrapper {}
#[cfg(windows)]
unsafe impl Sync for JobObjectWrapper {}

#[cfg(windows)]
impl Drop for JobObjectWrapper {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

pub struct PhononManager {
    child: Arc<Mutex<Option<Child>>>,
    #[cfg(windows)]
    _job_object: Arc<Mutex<Option<JobObjectWrapper>>>,
    status: Arc<Mutex<PhononStatusInfo>>,
    client: reqwest::Client,
}

impl PhononManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        let initial_status = PhononStatusInfo {
            state: PhononState::Offline,
            uv_installed: false,
            ptts_installed: false,
            message: None,
        };

        Self {
            child: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            _job_object: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(initial_status)),
            client,
        }
    }

    /// Check if Phonon health endpoint answers ok
    pub async fn check_health(&self) -> bool {
        match self.client.get(PHONON_HEALTH_URL).timeout(Duration::from_millis(1500)).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        return text.trim() == "ok" || text.trim() == "phonon-tts";
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Refresh detection of uv, ptts, and health
    pub async fn refresh_status(&self) -> PhononStatusInfo {
        let is_healthy = self.check_health().await;
        // detect_* spawn child processes; keep them off the async worker threads.
        let (uv_ok, ptts_ok) = tokio::task::spawn_blocking(|| {
            let uv_ok = Self::detect_uv();
            (uv_ok, uv_ok && Self::detect_ptts())
        })
        .await
        .unwrap_or((false, false));

        let mut current = self.status.lock().unwrap();
        current.uv_installed = uv_ok;
        current.ptts_installed = ptts_ok;

        if is_healthy {
            current.state = PhononState::Ready;
            current.message = Some("Phonon TTS engine is ready.".into());
        } else if current.state == PhononState::Starting {
            // Keep starting state while polling
        } else if current.state == PhononState::Installing {
            // Keep installing state while working
        } else {
            current.state = PhononState::Offline;
            current.message = Some("Phonon TTS engine is offline.".into());
        }

        current.clone()
    }

    pub fn get_status(&self) -> PhononStatusInfo {
        self.status.lock().unwrap().clone()
    }

    pub fn detect_uv() -> bool {
        let mut cmd = Command::new("where.exe");
        cmd.arg("uv").creation_flags(CREATE_NO_WINDOW);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                return true;
            }
        }

        // Also check default uv install location on Windows
        if let Some(user_profile) = dirs::home_dir() {
            let uv_path = user_profile.join(".local").join("bin").join("uv.exe");
            if uv_path.exists() {
                return true;
            }
            let uv_cargo = user_profile.join(".cargo").join("bin").join("uv.exe");
            if uv_cargo.exists() {
                return true;
            }
        }
        false
    }

    pub fn detect_ptts() -> bool {
        let mut cmd = Command::new("uv");
        cmd.args(["tool", "list"]).creation_flags(CREATE_NO_WINDOW);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.contains("ptts");
            }
        }
        false
    }

    /// Install uv using the official Astral script (SPEC 5.3)
    pub async fn install_uv(&self) -> Result<(), String> {
        {
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Installing;
            st.message = Some("Installing uv package manager...".into());
        }

        tracing::info!("Starting uv installation via PowerShell");
        let script = "irm https://astral.sh/uv/install.ps1 | iex";
        let mut cmd = Command::new("powershell");
        cmd.args(["-ExecutionPolicy", "Bypass", "-c", script])
            .creation_flags(CREATE_NO_WINDOW);

        let output = tokio::task::spawn_blocking(move || cmd.output())
            .await
            .map_err(|e| format!("Install task failed: {}", e))?
            .map_err(|e| format!("Failed to run powershell: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Failed;
            st.message = Some(format!("Failed to install uv: {}", err));
            return Err(err);
        }

        let mut st = self.status.lock().unwrap();
        st.uv_installed = true;
        st.state = PhononState::Offline;
        st.message = Some("uv installed successfully.".into());
        Ok(())
    }

    /// Install ptts uv tool (SPEC 5.3)
    pub async fn install_ptts(&self) -> Result<(), String> {
        {
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Installing;
            st.message = Some("Installing Phonon TTS (ptts)...".into());
        }

        tracing::info!("Installing ptts via uv tool install ptts");
        let mut cmd = Command::new("uv");
        cmd.args(["tool", "install", "ptts"])
            .creation_flags(CREATE_NO_WINDOW);

        let output = tokio::task::spawn_blocking(move || cmd.output())
            .await
            .map_err(|e| format!("Install task failed: {}", e))?
            .map_err(|e| format!("Failed to run uv tool install ptts: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Failed;
            st.message = Some(format!("Failed to install ptts: {}", err));
            return Err(err);
        }

        let mut st = self.status.lock().unwrap();
        st.ptts_installed = true;
        st.state = PhononState::Offline;
        st.message = Some("ptts installed successfully.".into());
        Ok(())
    }

    /// Start the Phonon TTS server in a Windows Job Object with hidden window
    pub async fn start_server(&self, custom_script_path: Option<PathBuf>) -> Result<(), String> {
        if self.check_health().await {
            tracing::info!("Phonon server already answering health check on 8001");
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Ready;
            st.message = Some("Phonon server is already running.".into());
            return Ok(());
        }

        // Find phonon_server.py
        let server_script = match custom_script_path {
            Some(p) if p.exists() => p,
            _ => Self::find_server_script()?,
        };

        tracing::info!("Using phonon_server.py at {:?}", server_script);

        #[cfg(windows)]
        let job = unsafe {
            let job_handle = CreateJobObjectW(None, None)
                .map_err(|e| format!("Failed to create Windows Job Object: {}", e))?;

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
                BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                    LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    ..Default::default()
                },
                ..Default::default()
            };

            let set_res = SetInformationJobObject(
                job_handle,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if let Err(e) = set_res {
                let _ = windows::Win32::Foundation::CloseHandle(job_handle);
                return Err(format!("Failed to set Job Object limits: {}", e));
            }

            job_handle
        };

        let mut cmd = Command::new("uv");
        cmd.args([
            "tool",
            "run",
            "--from",
            "ptts",
            "python",
            server_script.to_str().ok_or("Invalid script path")?,
        ])
        .creation_flags(CREATE_NO_WINDOW);

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn phonon server process: {}", e))?;

        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            let process_handle = HANDLE(child.as_raw_handle());
            if let Err(e) = AssignProcessToJobObject(job, process_handle) {
                tracing::warn!("Failed to assign process to Job Object: {}", e);
            }
        }

        #[cfg(windows)]
        {
            let mut job_slot = self._job_object.lock().unwrap();
            *job_slot = Some(JobObjectWrapper(job));
        }

        {
            let mut child_slot = self.child.lock().unwrap();
            *child_slot = Some(child);
        }

        {
            let mut st = self.status.lock().unwrap();
            st.state = PhononState::Starting;
            st.message = Some("Phonon server starting (downloading/loading model)...".into());
        }

        // Spawn background health polling for up to 180 seconds (SPEC 5.3)
        let client_clone = self.client.clone();
        let status_arc = self.status.clone();
        tokio::spawn(async move {
            let mut attempts = 0;
            while attempts < 180 {
                sleep(Duration::from_secs(1)).await;
                attempts += 1;

                if let Ok(resp) = client_clone.get(PHONON_HEALTH_URL).send().await {
                    if resp.status().is_success() {
                        tracing::info!("Phonon server is healthy after {}s", attempts);
                        let mut st = status_arc.lock().unwrap();
                        st.state = PhononState::Ready;
                        st.message = Some("Phonon server ready".into());
                        return;
                    }
                }
            }

            tracing::error!("Phonon server failed to answer health check within 180s");
            let mut st = status_arc.lock().unwrap();
            st.state = PhononState::Failed;
            st.message = Some("Phonon server timed out during startup.".into());
        });

        Ok(())
    }

    /// Locate phonon_server.py in development or packaged build
    fn find_server_script() -> Result<PathBuf, String> {
        let candidates = [
            PathBuf::from("resources/phonon/phonon_server.py"),
            PathBuf::from("../resources/phonon/phonon_server.py"),
            PathBuf::from("resources/phonon_server.py"),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Ok(candidate.canonicalize().unwrap_or(candidate.clone()));
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let candidate = parent.join("resources").join("phonon").join("phonon_server.py");
                if candidate.exists() {
                    return Ok(candidate);
                }
                let candidate2 = parent.join("phonon_server.py");
                if candidate2.exists() {
                    return Ok(candidate2);
                }
            }
        }

        Err("Could not locate resources/phonon/phonon_server.py".into())
    }

    /// Start synthesis via POST /tts and return as soon as the WAV header has arrived. The audio
    /// keeps flowing into the returned stream while it is generated, so playback can begin on the
    /// first chunk instead of after the whole sentence.
    pub async fn synthesize_stream(&self, text: &str, voice: &str) -> Result<PcmStream, String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err("Empty text for synthesis".into());
        }

        let voice_name = if voice.is_empty() { crate::config::DEFAULT_VOICE } else { voice };

        let form_params = [
            ("text", trimmed),
            ("voice_url", voice_name),
        ];

        let mut resp = self
            .client
            .post(PHONON_TTS_URL)
            .form(&form_params)
            .send()
            .await
            .map_err(|e| format!("Failed to reach Phonon server at {}: {}", PHONON_TTS_URL, e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(format!("Phonon synthesis failed ({}): {}", status, err_body));
        }

        // The body starts with a WAV header, then raw 16-bit samples until the server is done.
        let mut head: Vec<u8> = Vec::new();
        let (sample_rate, first_bytes) = loop {
            match resp.chunk().await {
                Ok(Some(c)) => head.extend_from_slice(&c),
                Ok(None) => return Err("Phonon ended the audio before sending a WAV header".into()),
                Err(e) => return Err(format!("Failed reading audio from Phonon: {e}")),
            }
            if let Some((rate, data_offset)) = parse_wav_header(&head)? {
                break (rate, head[data_offset..].to_vec());
            }
        };

        let (tx, rx) = std::sync::mpsc::channel::<Vec<i16>>();
        tokio::spawn(async move {
            let mut bytes = first_bytes;
            loop {
                // Send every complete sample; keep a stray odd byte for the next chunk.
                let whole = bytes.len() / 2 * 2;
                if whole > 0 {
                    let samples: Vec<i16> = bytes[..whole]
                        .chunks_exact(2)
                        .map(|p| i16::from_le_bytes([p[0], p[1]]))
                        .collect();
                    bytes.drain(..whole);
                    // The player dropped the stream (cancelled): stop pulling audio.
                    if tx.send(samples).is_err() {
                        return;
                    }
                }
                match resp.chunk().await {
                    Ok(Some(c)) => bytes.extend_from_slice(&c),
                    Ok(None) => return,
                    Err(e) => {
                        tracing::warn!("Phonon audio stream broke mid-sentence: {e}");
                        return;
                    }
                }
            }
        });

        Ok(PcmStream { sample_rate, rx })
    }

    pub fn stop(&self) {
        let mut child_guard = self.child.lock().unwrap();
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill();
        }
        let mut st = self.status.lock().unwrap();
        st.state = PhononState::Offline;
    }
}

/// Audio on its way from Phonon: mono 16-bit samples at `sample_rate`. The channel closes when
/// synthesis ends, or breaks.
pub struct PcmStream {
    pub sample_rate: u32,
    pub rx: std::sync::mpsc::Receiver<Vec<i16>>,
}

/// Read the WAV header Phonon streams ahead of the samples. Its data length is "unknown" (all ones),
/// so we do not trust sizes: just find the `fmt ` chunk, then where the `data` chunk starts.
/// `Ok(None)` means more bytes are needed. Returns (sample rate, offset of the first sample).
fn parse_wav_header(buf: &[u8]) -> Result<Option<(u32, usize)>, String> {
    if buf.len() < 12 {
        return Ok(None);
    }
    if &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return Err("Phonon sent audio that is not a WAV file".into());
    }
    let mut pos = 12;
    let mut rate = None;
    while pos + 8 <= buf.len() {
        let id = &buf[pos..pos + 4];
        let size = u32::from_le_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as usize;
        if id == b"data" {
            return match rate {
                Some(r) => Ok(Some((r, pos + 8))),
                None => Err("WAV data came before its format chunk".into()),
            };
        }
        if id == b"fmt " {
            if buf.len() < pos + 8 + 16 {
                return Ok(None);
            }
            let f = &buf[pos + 8..];
            let channels = u16::from_le_bytes([f[2], f[3]]);
            let bits = u16::from_le_bytes([f[14], f[15]]);
            if channels != 1 || bits != 16 {
                return Err(format!("Unsupported Phonon audio: {channels} channels, {bits} bits"));
            }
            rate = Some(u32::from_le_bytes([f[4], f[5], f[6], f[7]]));
        }
        pos += 8 + size;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streamed_header() -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&u32::MAX.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0, 0xC0, 0x5D, 0, 0, 0x80, 0xBB, 0, 0, 2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&u32::MAX.to_le_bytes());
        wav
    }

    #[test]
    fn reads_rate_and_data_offset_from_unknown_length_header() {
        let mut wav = streamed_header();
        assert_eq!(parse_wav_header(&wav).unwrap(), Some((24000, 44)));
        wav.extend_from_slice(&[0u8; 100]);
        assert_eq!(parse_wav_header(&wav).unwrap(), Some((24000, 44)));
    }

    #[test]
    fn asks_for_more_bytes_when_header_is_incomplete() {
        let wav = streamed_header();
        assert_eq!(parse_wav_header(&wav[..10]).unwrap(), None);
        assert_eq!(parse_wav_header(&wav[..30]).unwrap(), None);
        assert_eq!(parse_wav_header(&wav[..43]).unwrap(), None);
    }

    #[test]
    fn rejects_non_wav_and_unsupported_formats() {
        assert!(parse_wav_header(b"{\"error\": \"nope\"}   ").is_err());
        let mut stereo = streamed_header();
        stereo[22] = 2;
        assert!(parse_wav_header(&stereo).is_err());
    }
}

impl Drop for PhononManager {
    fn drop(&mut self) {
        self.stop();
    }
}
