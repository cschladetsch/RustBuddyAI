use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub feedback: FeedbackConfig,
    #[serde(default)]
    pub deepseek: DeepSeekConfig,
    #[serde(default)]
    pub transcription: TranscriptionConfig,
    #[serde(default)]
    pub files: HashMap<String, PathBuf>,
    #[serde(default)]
    pub applications: HashMap<String, String>,
    #[serde(default)]
    pub system: SystemConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioConfig {
    #[allow(dead_code)]
    pub device_name: Option<String>,
    #[serde(default = "AudioConfig::default_capture_duration_secs")]
    pub capture_duration_secs: u64,
    #[allow(dead_code)]
    #[serde(default = "AudioConfig::default_sample_rate")]
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "HotkeyConfig::default_key")]
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackConfig {
    #[serde(default = "FeedbackMode::default")]
    pub mode: FeedbackMode,
    pub success_sound: Option<PathBuf>,
    pub error_sound: Option<PathBuf>,
    #[serde(default = "FeedbackConfig::default_voice")]
    #[cfg_attr(not(windows), allow(dead_code))]
    pub tts_voice: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackMode {
    Sound,
    Tts,
    Both,
}

impl FeedbackMode {
    fn default() -> Self {
        Self::Tts
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSeekConfig {
    #[serde(default = "DeepSeekConfig::default_endpoint")]
    pub endpoint: String,
    #[serde(default = "DeepSeekConfig::default_model")]
    pub model: String,
    #[serde(default = "DeepSeekConfig::default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptionConfig {
    #[serde(default = "TranscriptionConfig::default_model_path")]
    pub model_path: PathBuf,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub threads: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemConfig {
    #[serde(default)]
    pub volume_mute: bool,
    #[serde(default)]
    pub volume_up: bool,
    #[serde(default)]
    pub volume_down: bool,
    #[serde(default)]
    pub volume_set: bool,
    #[serde(default)]
    pub sleep: bool,
    #[serde(default)]
    pub shutdown: bool,
    #[serde(default)]
    pub restart: bool,
    #[serde(default)]
    pub lock: bool,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let data = fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&data).map_err(ConfigError::Toml)
    }

    pub fn deepseek_timeout(&self) -> Duration {
        Duration::from_secs(self.deepseek.timeout_secs)
    }

    pub fn file_keys(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    pub fn app_keys(&self) -> Vec<String> {
        self.applications.keys().cloned().collect()
    }

    pub fn system_actions(&self) -> Vec<&'static str> {
        self.system.enabled_actions()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            hotkey: HotkeyConfig::default(),
            feedback: FeedbackConfig::default(),
            deepseek: DeepSeekConfig::default(),
            transcription: TranscriptionConfig::default(),
            files: HashMap::new(),
            applications: HashMap::new(),
            system: SystemConfig::default(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            capture_duration_secs: Self::default_capture_duration_secs(),
            sample_rate: Self::default_sample_rate(),
        }
    }
}

impl AudioConfig {
    const fn default_capture_duration_secs() -> u64 {
        3
    }

    const fn default_sample_rate() -> u32 {
        16_000
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            key: Self::default_key(),
        }
    }
}

impl HotkeyConfig {
    fn default_key() -> String {
        "ctrl+alt+b".to_string()
    }
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            mode: FeedbackMode::default(),
            success_sound: None,
            error_sound: None,
            tts_voice: Self::default_voice(),
        }
    }
}

impl FeedbackConfig {
    fn default_voice() -> String {
        "default".to_string()
    }
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        Self {
            endpoint: Self::default_endpoint(),
            model: Self::default_model(),
            timeout_secs: Self::default_timeout_secs(),
        }
    }
}

impl DeepSeekConfig {
    fn default_endpoint() -> String {
        "http://localhost:11434/api/chat".to_string()
    }

    fn default_model() -> String {
        "deepseek-r1:latest".to_string()
    }

    const fn default_timeout_secs() -> u64 {
        5
    }
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            model_path: Self::default_model_path(),
            language: None,
            threads: None,
        }
    }
}

impl TranscriptionConfig {
    fn default_model_path() -> PathBuf {
        PathBuf::from("buddy/models/ggml-base.en.bin")
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            volume_mute: true,
            volume_up: true,
            volume_down: true,
            volume_set: true,
            sleep: true,
            shutdown: true,
            restart: true,
            lock: true,
        }
    }
}

impl SystemConfig {
    pub fn enabled_actions(&self) -> Vec<&'static str> {
        let mut actions = Vec::new();
        if self.volume_mute {
            actions.push("volume_mute");
        }
        if self.volume_up {
            actions.push("volume_up");
        }
        if self.volume_down {
            actions.push("volume_down");
        }
        if self.volume_set {
            actions.push("volume_set");
        }
        if self.sleep {
            actions.push("sleep");
        }
        if self.shutdown {
            actions.push("shutdown");
        }
        if self.restart {
            actions.push("restart");
        }
        if self.lock {
            actions.push("lock");
        }
        actions
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Toml(toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read config: {}", err),
            Self::Toml(err) => write!(f, "failed to parse config: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Toml(err) => Some(err),
        }
    }
}
