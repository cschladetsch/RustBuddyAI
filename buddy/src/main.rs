mod audio;
mod config;
mod executor;
mod feedback;
mod hotkey;
mod intent;
mod transcription;
mod windows_api;

use audio::AudioCapturer;
use config::Config;
use executor::CommandExecutor;
use feedback::FeedbackPlayer;
use hotkey::{HotkeyError, HotkeyListener};
use intent::{IntentAction, IntentClient, IntentError, IntentResponse};
use std::{sync::Arc, time::Duration};
use transcription::Transcriber;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Buddy exited with error: {}", err);
    }
}

async fn run() -> Result<(), BuddyError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list-audio") {
        audio::print_input_devices()?;
        return Ok(());
    }
    let mut config_path = None;
    let mut debug_override: Option<bool> = None;
    let mut whisper_log_override: Option<bool> = None;
    for arg in &args {
        match arg.as_str() {
            "--debug" => debug_override = Some(true),
            "--no-debug" => debug_override = Some(false),
            "--whisper-log" => whisper_log_override = Some(true),
            "--no-whisper-log" => whisper_log_override = Some(false),
            _ if config_path.is_none() => config_path = Some(arg.clone()),
            _ => {}
        }
    }
    let config_path = config_path.unwrap_or_else(|| "config.toml".into());
    let config = match Config::load(&config_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!(
                "Failed to load config '{}': {}. Falling back to defaults.",
                config_path, err
            );
            Config::default()
        }
    };
    let debug = debug_override.unwrap_or(config.logging.debug);
    let whisper_log = whisper_log_override.unwrap_or(config.logging.whisper_log);
    if !whisper_log {
        unsafe {
            whisper_rs::set_log_callback(Some(silent_whisper_log), std::ptr::null_mut());
        }
    }
    if debug {
        println!("Loaded config from '{}'", config_path);
        if let Some(path) = config.files.get("resume") {
            println!("Config mapping: resume -> {}", path.display());
            if !path.exists() {
                eprintln!("Warning: resume path does not exist");
            }
        }
    }

    let capturer = Arc::new(AudioCapturer::new(&config.audio, debug)?);
    let initial_prompt = build_transcription_prompt(&config);
    let transcriber = Arc::new(Transcriber::new(&config.transcription, initial_prompt)?);
    let intent_client = IntentClient::new(&config);
    let executor = CommandExecutor::new(&config);
    let mut feedback = FeedbackPlayer::new(&config.feedback);
    let mut hotkey = HotkeyListener::new(&config.hotkey)?;

    println!(
        "Buddy ready. Press '{}' to issue a voice command.",
        config.hotkey.key
    );

    loop {
        hotkey.wait().await?;
        println!("Recording audio...");
        let capturer_clone = Arc::clone(&capturer);
        let capture_duration = Duration::from_secs(config.audio.capture_duration_secs);
        let audio_buffer =
            tokio::task::spawn_blocking(move || capturer_clone.capture(capture_duration)).await??;

        println!("Transcribing...");
        let transcript = transcriber.transcribe(&audio_buffer)?;
        if transcript.trim().is_empty() {
            eprintln!("No speech detected");
            feedback.error("I didn't hear anything");
            continue;
        }
        println!("Heard: {}", transcript);

        let intent = match intent_client.infer_intent(&transcript, &config).await {
            Ok(intent) => intent,
            Err(err) => {
                eprintln!("Intent error: {}", err);
                feedback.error("Intent failed");
                continue;
            }
        };
        handle_intent(&executor, intent, &mut feedback);
    }
}

fn build_transcription_prompt(config: &Config) -> Option<String> {
    let mut phrases = Vec::new();
    if !config.files.is_empty() {
        let mut keys: Vec<_> = config.files.keys().cloned().collect();
        keys.sort();
        for key in keys {
            phrases.push(format!("Open {}.", key));
        }
    }
    if !config.applications.is_empty() {
        let mut keys: Vec<_> = config.applications.keys().cloned().collect();
        keys.sort();
        for key in keys {
            phrases.push(format!("Launch {}.", key));
        }
    }
    let system = &config.system;
    if system.volume_mute {
        phrases.push("Mute volume.".to_string());
    }
    if system.volume_up {
        phrases.push("Volume up.".to_string());
    }
    if system.volume_down {
        phrases.push("Volume down.".to_string());
    }
    if system.volume_set {
        phrases.push("Set volume to 50.".to_string());
    }
    if system.sleep {
        phrases.push("Go to sleep.".to_string());
    }
    if system.restart {
        phrases.push("Restart computer.".to_string());
    }
    if system.shutdown {
        phrases.push("Shut down computer.".to_string());
    }
    if system.lock {
        phrases.push("Lock computer.".to_string());
    }
    if phrases.is_empty() {
        None
    } else {
        Some(phrases.join(" "))
    }
}

unsafe extern "C" fn silent_whisper_log(
    _level: std::os::raw::c_int,
    _text: *const std::os::raw::c_char,
    _user_data: *mut std::ffi::c_void,
) {
}

fn handle_intent(
    executor: &CommandExecutor<'_>,
    intent: IntentResponse,
    feedback: &mut FeedbackPlayer,
) {
    let confidence = intent.confidence;
    match intent.action {
        IntentAction::Unknown => {
            eprintln!("Intent not recognized");
            feedback.error("I don't know how to do that");
        }
        _ => match executor.execute(&intent) {
            Ok(message) => {
                println!("{} (confidence {:.2})", message, confidence);
                feedback.success();
            }
            Err(err) => {
                eprintln!("Action failed: {}", err);
                feedback.error("Command failed");
            }
        },
    }
}

#[derive(Debug)]
enum BuddyError {
    Audio(audio::AudioError),
    Transcription(transcription::TranscriptionError),
    Intent(IntentError),
    Hotkey(HotkeyError),
    Join(tokio::task::JoinError),
}

impl std::fmt::Display for BuddyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Audio(err) => write!(f, "audio error: {}", err),
            Self::Transcription(err) => write!(f, "transcription error: {}", err),
            Self::Intent(err) => write!(f, "intent error: {}", err),
            Self::Hotkey(err) => write!(f, "hotkey error: {}", err),
            Self::Join(err) => write!(f, "task failed: {}", err),
        }
    }
}

impl std::error::Error for BuddyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Audio(err) => Some(err),
            Self::Transcription(err) => Some(err),
            Self::Intent(err) => Some(err),
            Self::Hotkey(err) => Some(err),
            Self::Join(err) => Some(err),
        }
    }
}

impl From<audio::AudioError> for BuddyError {
    fn from(err: audio::AudioError) -> Self {
        Self::Audio(err)
    }
}

impl From<transcription::TranscriptionError> for BuddyError {
    fn from(err: transcription::TranscriptionError) -> Self {
        Self::Transcription(err)
    }
}

impl From<IntentError> for BuddyError {
    fn from(err: IntentError) -> Self {
        Self::Intent(err)
    }
}

impl From<HotkeyError> for BuddyError {
    fn from(err: HotkeyError) -> Self {
        Self::Hotkey(err)
    }
}

impl From<tokio::task::JoinError> for BuddyError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Join(err)
    }
}
