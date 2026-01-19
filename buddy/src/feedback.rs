use crate::config::{FeedbackConfig, FeedbackMode};
use rodio::{Decoder, OutputStream, Sink};
use std::{fs::File, io::BufReader, path::Path};

#[cfg(windows)]
use tts::Tts;

pub struct FeedbackPlayer {
    mode: FeedbackMode,
    success_sound: Option<String>,
    error_sound: Option<String>,
    #[cfg(windows)]
    tts: Option<Tts>,
}

impl FeedbackPlayer {
    pub fn new(cfg: &FeedbackConfig) -> Self {
        Self {
            mode: cfg.mode.clone(),
            success_sound: cfg
                .success_sound
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            error_sound: cfg
                .error_sound
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            #[cfg(windows)]
            tts: init_tts(&cfg.tts_voice),
        }
    }

    pub fn success(&mut self) {
        match self.mode {
            FeedbackMode::Sound => {
                if let Some(path) = self.success_sound.clone() {
                    play_sound(Path::new(&path));
                }
            }
            FeedbackMode::Tts => {
                self.speak("Ok");
            }
            FeedbackMode::Both => {
                if let Some(path) = self.success_sound.clone() {
                    play_sound(Path::new(&path));
                }
                self.speak("Ok");
            }
        }
    }

    pub fn error(&mut self, message: &str) {
        match self.mode {
            FeedbackMode::Sound => {
                if let Some(path) = self.error_sound.clone() {
                    play_sound(Path::new(&path));
                }
            }
            FeedbackMode::Tts => self.speak(message),
            FeedbackMode::Both => {
                if let Some(path) = self.error_sound.clone() {
                    play_sound(Path::new(&path));
                }
                self.speak(message);
            }
        }
    }

    fn speak(&mut self, text: &str) {
        #[cfg(windows)]
        {
            if let Some(tts) = self.tts.as_mut() {
                let _ = tts.speak(text, false);
            }
        }

        #[cfg(not(windows))]
        {
            let _ = text;
        }
    }
}

#[cfg(windows)]
fn init_tts(preferred_voice: &str) -> Option<Tts> {
    let mut tts = Tts::default().ok()?;
    if !preferred_voice.eq_ignore_ascii_case("default") {
        if let Ok(voices) = tts.voices() {
            if let Some(voice) = voices
                .into_iter()
                .find(|voice| voice.name().eq_ignore_ascii_case(preferred_voice))
            {
                let _ = tts.set_voice(&voice);
            }
        }
    }
    Some(tts)
}

fn play_sound(path: &Path) {
    if let Err(err) = try_play_sound(path) {
        eprintln!("failed to play sound {}: {}", path.display(), err);
    }
}

fn try_play_sound(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let (_stream, stream_handle) = OutputStream::try_default().map_err(|e| e.to_string())?;
    let sink = Sink::try_new(&stream_handle).map_err(|e| e.to_string())?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    let source = Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}
