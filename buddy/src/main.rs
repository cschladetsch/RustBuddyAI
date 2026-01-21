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
use executor::{CommandExecutor, ExecutionResult};
use feedback::FeedbackPlayer;
use hotkey::{HotkeyError, HotkeyListener};
use intent::{Intent, IntentClient, IntentError};
use std::{path::Path, path::PathBuf, sync::Arc, time::Duration, time::Instant};
#[cfg(windows)]
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
#[cfg(windows)]
use windows::Win32::Foundation::HINSTANCE;
use transcription::Transcriber;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Buddy exited with error: {}", err);
    }
}

async fn run() -> Result<(), BuddyError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut test_phrases: Vec<String> = Vec::new();
    if args.iter().any(|arg| arg == "--list-audio") {
        audio::print_input_devices()?;
        return Ok(());
    }
    let mut config_path = None;
    let mut debug_override: Option<bool> = None;
    let mut whisper_log_override: Option<bool> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--debug" => debug_override = Some(true),
            "--no-debug" => debug_override = Some(false),
            "--whisper-log" => whisper_log_override = Some(true),
            "--no-whisper-log" => whisper_log_override = Some(false),
            "--test-intent" => {
                let next = args.get(index + 1);
                if let Some(phrase) = next {
                    test_phrases.push(phrase.clone());
                    index += 1;
                } else {
                    eprintln!("Missing value for --test-intent");
                    return Ok(());
                }
            }
            _ if config_path.is_none() && !arg.starts_with("--") => config_path = Some(arg.clone()),
            _ => {}
        }
        index += 1;
    }
    let config_path = config_path.unwrap_or_else(|| "config.toml".into());
    let config = match Config::load(&config_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!(
                "Failed to load config '{}': {}. Trying default config.",
                config_path, err
            );
            let fallback_path = Path::new(&config_path)
                .parent()
                .map(|dir| dir.join("config.default.toml"))
                .unwrap_or_else(|| PathBuf::from("config.default.toml"));
            match Config::load(&fallback_path) {
                Ok(cfg) => {
                    println!("Loaded default config from '{}'", fallback_path.display());
                    cfg
                }
                Err(fallback_err) => return Err(BuddyError::Config(fallback_err)),
            }
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
        println!(
            "Whisper CUDA feature enabled: {}",
            colorize(
                if cfg!(feature = "cuda") { "yes" } else { "no" },
                if cfg!(feature = "cuda") {
                    Color::Green
                } else {
                    Color::Yellow
                }
            )
        );
        if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
            println!("CUDA_PATH: {}", cuda_path);
        }
        #[cfg(windows)]
        if cfg!(feature = "cuda") {
            if !check_cublas_loaded() {
                eprintln!("{}", colorize(
                    "CUDA enabled but cublas64_*.dll was not loadable. Check CUDA_PATH/bin on PATH.",
                    Color::Red
                ));
            }
        }
    }

    let intent_client = IntentClient::new(&config);
    wait_for_intent_ready(&intent_client).await?;
    if !test_phrases.is_empty() {
        for phrase in test_phrases {
            println!("Input: {}", phrase);
            match intent_client.infer_intent(&phrase, &config).await {
                Ok(intent) => {
                    println!(
                        "Output: action={:?} confidence={:.2}",
                        intent.action(),
                        intent.confidence()
                    );
                }
                Err(err) => eprintln!("Intent error: {}", err),
            }
        }
        return Ok(());
    }

    let capturer = Arc::new(AudioCapturer::new(&config.audio, debug)?);
    let initial_prompt = build_transcription_prompt(&config);
    let transcriber = Arc::new(Transcriber::new(
        &config.transcription,
        initial_prompt,
        debug,
        !whisper_log,
    )?);
    if debug {
        println!("Whisper system info: {}", whisper_rs::print_system_info());
    }
    let executor = CommandExecutor::new(&config);
    let mut feedback = FeedbackPlayer::new(&config.feedback);
    let mut hotkey = HotkeyListener::new(&config.hotkey)?;

    println!(
        "Buddy ready. Press '{}' to issue a voice command.",
        config.hotkey.key
    );

    loop {
        if debug {
            println!("Waiting for hotkey...");
        }
        hotkey.wait().await?;
        if debug {
            println!("Hotkey received");
        }
        let total_start = Instant::now();
        println!("Recording audio...");
        let capturer_clone = Arc::clone(&capturer);
        let max_duration = if config.audio.capture_duration_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(config.audio.capture_duration_secs))
        };
        let capture_start = Instant::now();
        let audio_buffer =
            tokio::task::spawn_blocking(move || capturer_clone.capture(max_duration)).await??;
        let capture_elapsed = capture_start.elapsed();

        println!("Transcribing...");
        let transcribe_start = Instant::now();
        let transcript = transcriber.transcribe(&audio_buffer)?;
        let transcribe_elapsed = transcribe_start.elapsed();
        if transcript.trim().is_empty() {
            eprintln!("No speech detected");
            feedback.error("I didn't hear anything");
            continue;
        }
        println!("Heard: {}", transcript);
        let normalized = transcript
            .trim()
            .trim_end_matches(|c: char| c == '.' || c == '!' || c == '?');
        if normalized.eq_ignore_ascii_case("help") {
            let help = "Say: open <file>, launch <app>, set volume, mute, lock, sleep, or ask a question.";
            println!("Help: {}", help);
            feedback.say(help);
            continue;
        }

        let intent_start = Instant::now();
        let intent = match intent_client.infer_intent(&transcript, &config).await {
            Ok(intent) => intent,
            Err(err) => {
                eprintln!("Intent error: {}", err);
                feedback.error("Intent failed");
                continue;
            }
        };
        let intent_elapsed = intent_start.elapsed();
        let execute_start = Instant::now();
        handle_intent(&executor, intent, &mut feedback);
        let execute_elapsed = execute_start.elapsed();
        if debug {
            let total_elapsed = total_start.elapsed();
            println!(
                "{}",
                colorize(
                    &format!(
                        "Timings: capture={:.2}s transcribe={:.2}s intent={:.2}s execute={:.2}s total={:.2}s",
                        capture_elapsed.as_secs_f64(),
                        transcribe_elapsed.as_secs_f64(),
                        intent_elapsed.as_secs_f64(),
                        execute_elapsed.as_secs_f64(),
                        total_elapsed.as_secs_f64()
                    ),
                    Color::Cyan
                )
            );
        }
        if debug {
            println!("Command complete");
        }
    }
}

#[derive(Clone, Copy)]
enum Color {
    Red,
    Green,
    Yellow,
    Cyan,
}

fn colorize(text: &str, color: Color) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        return text.to_string();
    }
    let code = match color {
        Color::Red => "31",
        Color::Green => "32",
        Color::Yellow => "33",
        Color::Cyan => "36",
    };
    format!("\x1b[{}m{}\x1b[0m", code, text)
}

#[cfg(windows)]
fn check_cublas_loaded() -> bool {
    let candidates = ["cublas64_13.dll", "cublas64_12.dll", "cublas64_11.dll"];
    for name in candidates {
        if load_library(name).is_some() {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn load_library(name: &str) -> Option<HINSTANCE> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if let Ok(handle) = GetModuleHandleW(windows::core::PCWSTR(wide.as_ptr())) {
            return Some(handle.into());
        }
        if let Ok(loaded) = LoadLibraryW(windows::core::PCWSTR(wide.as_ptr())) {
            return Some(loaded.into());
        }
    }
    None
}

async fn wait_for_intent_ready(intent_client: &IntentClient) -> Result<(), IntentError> {
    let attempts = 240;
    let delay = Duration::from_secs(1);
    for attempt in 1..=attempts {
        match intent_client.wait_for_ready().await {
            Ok(()) => return Ok(()),
            Err(err) => {
                if attempt == attempts {
                    return Err(err);
                }
                eprintln!(
                    "Intent service not ready (attempt {}/{}): {}",
                    attempt, attempts, err
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
    Ok(())
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
    intent: Intent,
    feedback: &mut FeedbackPlayer,
) {
    let confidence = intent.confidence();
    match executor.execute(&intent) {
        Ok(result) => match result {
            ExecutionResult::Action(message) => {
                println!("{} (confidence {:.2})", message, confidence);
                feedback.success();
            }
            ExecutionResult::Answer(response) => {
                println!("Speaking response...");
                println!("Answer: {} (confidence {:.2})", response, confidence);
                feedback.say(&response);
            }
        },
        Err(err) => {
            eprintln!("Action failed: {}", err);
            if matches!(err, executor::ExecutionError::UnknownIntent) {
                feedback.error("I don't know how to do that");
            } else {
                feedback.error("Command failed");
            }
        }
    }
}

#[derive(Debug)]
enum BuddyError {
    Config(config::ConfigError),
    Audio(audio::AudioError),
    Transcription(transcription::TranscriptionError),
    Intent(IntentError),
    Hotkey(HotkeyError),
    Join(tokio::task::JoinError),
}

impl std::fmt::Display for BuddyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {}", err),
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
            Self::Config(err) => Some(err),
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

impl From<config::ConfigError> for BuddyError {
    fn from(err: config::ConfigError) -> Self {
        Self::Config(err)
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
