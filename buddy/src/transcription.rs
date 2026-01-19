use crate::config::{AudioConfig, TranscriptionConfig};

#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use windows::{
    core::HSTRING,
    Foundation::TimeSpan,
    Globalization::Language,
    Media::SpeechRecognition::{
        SpeechRecognitionResultStatus, SpeechRecognitionScenario, SpeechRecognitionTopicConstraint,
        SpeechRecognizer,
    },
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

#[cfg(target_os = "windows")]
pub struct Transcriber {
    _guard: RoGuard,
    recognizer: SpeechRecognizer,
}

#[cfg(not(target_os = "windows"))]
pub struct Transcriber;

impl Transcriber {
    #[cfg(target_os = "windows")]
    pub fn new(
        cfg: &TranscriptionConfig,
        audio_cfg: &AudioConfig,
    ) -> Result<Self, TranscriptionError> {
        let guard = RoGuard::new()?;
        let recognizer = create_recognizer(cfg)?;
        configure_topic(&recognizer, cfg)?;
        configure_timeouts(&recognizer, cfg, audio_cfg)?;
        compile_constraints(&recognizer)?;
        Ok(Self {
            _guard: guard,
            recognizer,
        })
    }

    #[cfg(not(target_os = "windows"))]
    pub fn new(
        _cfg: &TranscriptionConfig,
        _audio_cfg: &AudioConfig,
    ) -> Result<Self, TranscriptionError> {
        Err(TranscriptionError::Unsupported(
            "Windows speech recognition is only available on Windows",
        ))
    }

    #[cfg(target_os = "windows")]
    pub fn transcribe(&self) -> Result<String, TranscriptionError> {
        let result = self.recognizer.RecognizeAsync()?.get()?;
        match result.Status()? {
            SpeechRecognitionResultStatus::Success => Ok(result.Text()?.to_string()),
            SpeechRecognitionResultStatus::TimeoutExceeded
            | SpeechRecognitionResultStatus::PauseLimitExceeded => Ok(String::new()),
            other => Err(TranscriptionError::RecognitionStatus(other)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn transcribe(&self) -> Result<String, TranscriptionError> {
        Err(TranscriptionError::Unsupported(
            "Windows speech recognition is only available on Windows",
        ))
    }
}

#[cfg(target_os = "windows")]
fn create_recognizer(cfg: &TranscriptionConfig) -> Result<SpeechRecognizer, TranscriptionError> {
    if let Some(tag) = cfg
        .language_tag
        .as_deref()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
    {
        let language = Language::CreateLanguage(&HSTRING::from(tag))?;
        SpeechRecognizer::Create(&language).map_err(TranscriptionError::from)
    } else {
        SpeechRecognizer::new().map_err(TranscriptionError::from)
    }
}

#[cfg(target_os = "windows")]
fn configure_topic(
    recognizer: &SpeechRecognizer,
    cfg: &TranscriptionConfig,
) -> Result<(), TranscriptionError> {
    let constraints = recognizer.Constraints()?;
    constraints.Clear()?;
    let hint = cfg
        .topic_hint
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let topic = if hint.is_empty() { "dictation" } else { &hint };
    let constraint = SpeechRecognitionTopicConstraint::Create(
        SpeechRecognitionScenario::Dictation,
        &HSTRING::from(topic),
    )?;
    constraints.Append(&constraint)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_timeouts(
    recognizer: &SpeechRecognizer,
    cfg: &TranscriptionConfig,
    audio_cfg: &AudioConfig,
) -> Result<(), TranscriptionError> {
    let timeouts = recognizer.Timeouts()?;
    let initial = cfg
        .initial_silence_timeout_ms
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(audio_cfg.capture_duration_secs.max(1)));
    let end_silence = cfg
        .end_silence_timeout_ms
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(1200));
    timeouts.SetInitialSilenceTimeout(duration_to_timespan(initial))?;
    timeouts.SetEndSilenceTimeout(duration_to_timespan(end_silence))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn compile_constraints(recognizer: &SpeechRecognizer) -> Result<(), TranscriptionError> {
    let compilation = recognizer.CompileConstraintsAsync()?.get()?;
    match compilation.Status()? {
        SpeechRecognitionResultStatus::Success => Ok(()),
        other => Err(TranscriptionError::CompilationStatus(other)),
    }
}

#[cfg(target_os = "windows")]
fn duration_to_timespan(duration: Duration) -> TimeSpan {
    const HUNDRED_NANOS_PER_SEC: i64 = 10_000_000;
    let secs = (duration.as_secs() as i64).saturating_mul(HUNDRED_NANOS_PER_SEC);
    let nanos = (duration.subsec_nanos() as i64) / 100;
    TimeSpan {
        Duration: secs.saturating_add(nanos),
    }
}

#[cfg(target_os = "windows")]
struct RoGuard;

#[cfg(target_os = "windows")]
impl RoGuard {
    fn new() -> Result<Self, TranscriptionError> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        Ok(Self)
    }
}

#[cfg(target_os = "windows")]
impl Drop for RoGuard {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

#[derive(Debug)]
pub enum TranscriptionError {
    #[cfg(target_os = "windows")]
    Windows(windows::core::Error),
    #[cfg(target_os = "windows")]
    CompilationStatus(SpeechRecognitionResultStatus),
    #[cfg(target_os = "windows")]
    RecognitionStatus(SpeechRecognitionResultStatus),
    #[cfg(not(target_os = "windows"))]
    Unsupported(&'static str),
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "windows")]
            Self::Windows(err) => write!(f, "windows speech recognition error: {}", err),
            #[cfg(target_os = "windows")]
            Self::CompilationStatus(status) => {
                write!(f, "failed to compile recognition constraints: {:?}", status)
            }
            #[cfg(target_os = "windows")]
            Self::RecognitionStatus(status) => {
                write!(f, "speech recognition failed: {:?}", status)
            }
            #[cfg(not(target_os = "windows"))]
            Self::Unsupported(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for TranscriptionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        #[allow(clippy::match_single_binding)]
        match self {
            #[cfg(target_os = "windows")]
            Self::Windows(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(target_os = "windows")]
impl From<windows::core::Error> for TranscriptionError {
    fn from(err: windows::core::Error) -> Self {
        Self::Windows(err)
    }
}
