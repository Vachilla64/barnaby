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
        let uv_ok = Self::detect_uv();
        let ptts_ok = if uv_ok { Self::detect_ptts() } else { false };

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

        let output = cmd.output().map_err(|e| format!("Failed to run powershell: {}", e))?;
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

        let output = cmd.output().map_err(|e| format!("Failed to run uv tool install ptts: {}", e))?;
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

    /// Synthesize speech via POST /tts (streamed WAV)
    pub async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>, String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err("Empty text for synthesis".into());
        }

        let voice_name = if voice.is_empty() { "Toby" } else { voice };

        let form_params = [
            ("text", trimmed),
            ("voice_url", voice_name),
        ];

        let resp = self
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

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed reading audio bytes from Phonon: {}", e))?;

        Ok(bytes.to_vec())
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

impl Drop for PhononManager {
    fn drop(&mut self) {
        self.stop();
    }
}
