use crate::config::AudioConfig;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SampleFormat, SampleRate, SizedSample, StreamConfig,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub struct AudioCapturer {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
}

impl AudioCapturer {
    pub fn new(cfg: &AudioConfig) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = if let Some(name) = &cfg.device_name {
            let mut devices = host.input_devices().map_err(AudioError::Devices)?;
            let mut selected = None;
            while let Some(dev) = devices.next() {
                if let Ok(dev_name) = dev.name() {
                    if &dev_name == name {
                        selected = Some(dev);
                        break;
                    }
                }
            }
            selected.ok_or_else(|| AudioError::DeviceNotFound(name.clone()))?
        } else {
            host.default_input_device()
                .ok_or(AudioError::NoDefaultDevice)?
        };

        let supported = device
            .default_input_config()
            .map_err(AudioError::DefaultConfig)?;
        let sample_format = supported.sample_format();
        let mut stream_config: StreamConfig = supported.config().clone();
        stream_config.channels = 1;
        stream_config.sample_rate = SampleRate(cfg.sample_rate);

        Ok(Self {
            device,
            config: stream_config,
            sample_format,
        })
    }

    pub fn capture(&self, duration: Duration) -> Result<Vec<i16>, AudioError> {
        match self.sample_format {
            SampleFormat::I16 => self.capture_with_type::<i16, _>(duration, |sample| sample),
            SampleFormat::U16 => self.capture_with_type::<u16, _>(duration, |sample| {
                let centered = sample as i32 - i16::MAX as i32 - 1;
                centered as i16
            }),
            SampleFormat::F32 => self.capture_with_type::<f32, _>(duration, |sample| {
                let clamped = sample.max(-1.0).min(1.0);
                (clamped * i16::MAX as f32) as i16
            }),
            _ => Err(AudioError::UnsupportedFormat(self.sample_format)),
        }
    }

    fn capture_with_type<T, F>(
        &self,
        duration: Duration,
        convert: F,
    ) -> Result<Vec<i16>, AudioError>
    where
        T: Sample + SizedSample + Send + 'static,
        F: Fn(T) -> i16 + Send + Sync + 'static,
    {
        let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let writer = buffer.clone();
        let convert = Arc::new(convert);
        let err_fn = |err| eprintln!("audio stream error: {}", err);

        let stream = self
            .device
            .build_input_stream(
                &self.config,
                {
                    let convert = Arc::clone(&convert);
                    move |data: &[T], _| {
                        if let Ok(mut buf) = writer.lock() {
                            buf.extend(data.iter().map(|sample| convert(*sample)));
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(AudioError::BuildStream)?;

        stream.play().map_err(AudioError::PlayStream)?;
        thread::sleep(duration);
        drop(stream);

        let mut data = buffer.lock().map_err(|_| AudioError::BufferAccess)?;
        Ok(std::mem::take(&mut *data))
    }
}

#[derive(Debug)]
pub enum AudioError {
    Devices(cpal::DevicesError),
    DeviceNotFound(String),
    NoDefaultDevice,
    DefaultConfig(cpal::DefaultStreamConfigError),
    UnsupportedFormat(SampleFormat),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    BufferAccess,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Devices(err) => write!(f, "device query error: {}", err),
            Self::DeviceNotFound(name) => write!(f, "input device '{}' not found", name),
            Self::NoDefaultDevice => write!(f, "no default input device available"),
            Self::DefaultConfig(err) => write!(f, "failed to read default config: {}", err),
            Self::UnsupportedFormat(fmt) => write!(f, "unsupported sample format: {:?}", fmt),
            Self::BuildStream(err) => write!(f, "failed building stream: {}", err),
            Self::PlayStream(err) => write!(f, "failed starting stream: {}", err),
            Self::BufferAccess => write!(f, "failed accessing buffer"),
        }
    }
}

impl std::error::Error for AudioError {}
