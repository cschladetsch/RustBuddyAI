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
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".into());
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

    let capturer = Arc::new(AudioCapturer::new(&config.audio)?);
    let transcriber = Arc::new(Transcriber::new(&config.transcription)?);
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

        let intent = intent_client.infer_intent(&transcript, &config).await?;
        handle_intent(&executor, intent, &mut feedback);
    }
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
