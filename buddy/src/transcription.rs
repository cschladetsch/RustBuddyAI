use crate::config::TranscriptionConfig;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
    language: Option<String>,
    threads: i32,
}

impl Transcriber {
    pub fn new(cfg: &TranscriptionConfig) -> Result<Self, TranscriptionError> {
        let model_path = resolve_path(&cfg.model_path);
        let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            .map_err(|err| TranscriptionError::Model(err.to_string()))?;
        let threads = cfg
            .threads
            .unwrap_or_else(|| num_cpus::get().max(1))
            .clamp(1, 16) as i32;
        Ok(Self {
            ctx,
            language: cfg.language.clone(),
            threads,
        })
    }

    pub fn transcribe(&self, audio: &[i16]) -> Result<String, TranscriptionError> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut state = self
            .ctx
            .create_state()
            .map_err(|err| TranscriptionError::State(err.to_string()))?;
        let mut params = FullParams::new(SamplingStrategy::default());
        params.set_n_threads(self.threads);
        if let Some(lang) = &self.language {
            params.set_language(Some(lang));
        }

        let audio_f32: Vec<f32> = audio
            .iter()
            .map(|sample| *sample as f32 / i16::MAX as f32)
            .collect();
        state
            .full(params, &audio_f32)
            .map_err(|err| TranscriptionError::Inference(err.to_string()))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|err| TranscriptionError::State(err.to_string()))?;
        let mut transcript = String::new();
        for idx in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(idx) {
                let text = segment.trim();
                if !text.is_empty() {
                    if !transcript.is_empty() {
                        transcript.push(' ');
                    }
                    transcript.push_str(text);
                }
            }
        }
        Ok(transcript)
    }
}

fn resolve_path(path: &Path) -> String {
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(path)
            .to_string_lossy()
            .to_string()
    }
}

#[derive(Debug)]
pub enum TranscriptionError {
    Model(String),
    State(String),
    Inference(String),
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(err) => write!(f, "failed to load Whisper model: {}", err),
            Self::State(err) => write!(f, "failed to initialize Whisper state: {}", err),
            Self::Inference(err) => write!(f, "transcription error: {}", err),
        }
    }
}

impl std::error::Error for TranscriptionError {}
