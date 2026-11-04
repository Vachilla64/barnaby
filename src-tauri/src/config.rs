use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub model: String,
    /// OpenAI-compatible Chat Completions base URL. Meta's by default; can point at a local model.
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    "https://api.meta.ai/v1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub volume: f32,
    pub voices: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetConfig {
    pub colour: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourConfig {
    /// true: VS Code opens each file and highlights the lines. false: Barnaby only says where to look.
    #[serde(default = "default_true")]
    pub navigate: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TourConfig {
    fn default() -> Self {
        Self { navigate: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub token: String,
    pub brain: BrainConfig,
    pub voice: VoiceConfig,
    pub push_to_talk: String,
    pub pets: HashMap<String, PetConfig>,
    pub projects: Vec<String>,
    #[serde(default)]
    pub tour: TourConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut voices = HashMap::new();
        voices.insert("claude-code".to_string(), "Freya".to_string());
        voices.insert("gemini-cli".to_string(), "Easton".to_string());
        voices.insert("other".to_string(), "Freya".to_string());

        let mut pets = HashMap::new();
        pets.insert(
            "claude-code".to_string(),
            PetConfig {
                colour: "#E8875B".to_string(),
                x: None,
                y: None,
            },
        );
        pets.insert(
            "gemini-cli".to_string(),
            PetConfig {
                colour: "#5B8DEF".to_string(),
                x: None,
                y: None,
            },
        );

        let random_token = generate_random_token();

        Self {
            port: 4747,
            token: random_token,
            brain: BrainConfig {
                model: "muse-spark-1.3".to_string(),
                base_url: default_base_url(),
            },
            voice: VoiceConfig {
                enabled: true,
                volume: 0.9,
                voices,
            },
            push_to_talk: "Ctrl+Alt+Space".to_string(),
            pets,
            projects: Vec::new(),
            tour: TourConfig::default(),
        }
    }
}

pub fn generate_random_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn get_appdata_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Barnaby")
}

pub fn get_config_path() -> PathBuf {
    get_appdata_dir().join("config.json")
}

pub fn load_or_create_config() -> Result<AppConfig, Box<dyn std::error::Error + Send + Sync>> {
    let dir = get_appdata_dir();
    if !dir.exists() {
        // The app was called Devy, then Oscar, before Barnaby; carry the old config across.
        for old in ["Oscar", "Devy"] {
            let old_dir = dir.with_file_name(old);
            if old_dir.exists() {
                let _ = fs::rename(&old_dir, &dir);
                break;
            }
        }
    }
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let config_path = get_config_path();
    if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    } else {
        let config = AppConfig::default();
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(&config_path, content)?;
        Ok(config)
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = get_appdata_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    let config_path = get_config_path();
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_token_format() {
        let token = generate_random_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_default_config_structure() {
        let config = AppConfig::default();
        assert_eq!(config.port, 4747);
        assert_eq!(config.token.len(), 32);
        assert_eq!(config.brain.model, "muse-spark-1.3");
        assert!(config.voice.enabled);
        assert_eq!(
            config.voice.voices.get("claude-code"),
            Some(&"Freya".to_string())
        );
        assert_eq!(
            config.voice.voices.get("gemini-cli"),
            Some(&"Easton".to_string())
        );
        assert_eq!(config.push_to_talk, "Ctrl+Alt+Space");
        assert!(config.pets.contains_key("claude-code"));
        assert!(config.pets.contains_key("gemini-cli"));

        let json_str = serde_json::to_string_pretty(&config).expect("serialize");
        let deserialized: AppConfig = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(deserialized.port, 4747);
        assert_eq!(deserialized.token, config.token);
    }
}
