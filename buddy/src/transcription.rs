use crate::config::TranscriptionConfig;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
    language: Option<String>,
    threads: i32,
    initial_prompt: Option<String>,
    suppress_native_logs: bool,
}

impl Transcriber {
    pub fn new(
        cfg: &TranscriptionConfig,
        initial_prompt: Option<String>,
        debug: bool,
        suppress_native_logs: bool,
    ) -> Result<Self, TranscriptionError> {
        let model_path = resolve_path(&cfg.model_path);
        let mut ctx_params = WhisperContextParameters::new();
        let use_gpu = cfg!(feature = "cuda");
        ctx_params.use_gpu(use_gpu);
        if debug {
            println!("Whisper context use_gpu: {}", use_gpu);
        }
        let ctx = WhisperContext::new_with_params(&model_path, ctx_params)
            .map_err(|err| TranscriptionError::Model(err.to_string()))?;
        let threads = cfg
            .threads
            .unwrap_or_else(|| num_cpus::get().max(1))
            .clamp(1, 16) as i32;
        Ok(Self {
            ctx,
            language: cfg.language.clone(),
            threads,
            initial_prompt,
            suppress_native_logs,
        })
    }

    pub fn transcribe(&self, audio: &[i16]) -> Result<String, TranscriptionError> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let _silencer = if self.suppress_native_logs {
            StderrSilencer::new()
        } else {
            None
        };
        let mut state = self
            .ctx
            .create_state()
            .map_err(|err| TranscriptionError::State(err.to_string()))?;
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: 0.0,
        });
        params.set_n_threads(self.threads);
        if let Some(lang) = &self.language {
            params.set_language(Some(lang));
        }
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_max_tokens(32);
        params.set_suppress_blank(true);
        params.set_suppress_non_speech_tokens(true);
        if let Some(prompt) = &self.initial_prompt {
            params.set_initial_prompt(prompt);
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

struct StderrSilencer {
    saved_fd: i32,
}

impl StderrSilencer {
    fn new() -> Option<Self> {
        unsafe {
            let saved_fd = _dup(2);
            if saved_fd == -1 {
                return None;
            }
            let nul = std::fs::OpenOptions::new().write(true).open("NUL").ok()?;
            let handle = std::os::windows::io::IntoRawHandle::into_raw_handle(nul);
            let nul_fd = _open_osfhandle(handle as isize, 0);
            if nul_fd == -1 {
                let _ = _close(saved_fd);
                return None;
            }
            if _dup2(nul_fd, 2) == -1 {
                let _ = _close(nul_fd);
                let _ = _close(saved_fd);
                return None;
            }
            let _ = _close(nul_fd);
            Some(Self { saved_fd })
        }
    }
}

impl Drop for StderrSilencer {
    fn drop(&mut self) {
        unsafe {
            let _ = _dup2(self.saved_fd, 2);
            let _ = _close(self.saved_fd);
        }
    }
}

extern "C" {
    fn _dup(fd: i32) -> i32;
    fn _dup2(fd: i32, fd2: i32) -> i32;
    fn _close(fd: i32) -> i32;
    fn _open_osfhandle(osfhandle: isize, flags: i32) -> i32;
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
