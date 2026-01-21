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
    channels: usize,
    sample_rate: u32,
    debug: bool,
}

pub fn print_input_devices() -> Result<(), AudioError> {
    let host = cpal::default_host();
    let mut devices = host.input_devices().map_err(AudioError::Devices)?;
    let mut index = 0;
    while let Some(device) = devices.next() {
        let name = device
            .name()
            .unwrap_or_else(|_| format!("Input Device {}", index));
        println!("Input: {}", name);

        match device.default_input_config() {
            Ok(default_cfg) => {
                let cfg = default_cfg.config();
                println!(
                    "  Default: {} ch, {:?}, {} Hz",
                    cfg.channels, default_cfg.sample_format(), cfg.sample_rate.0
                );
            }
            Err(err) => {
                eprintln!("  Default config error: {}", err);
            }
        }

        match device.supported_input_configs() {
            Ok(configs) => {
                for cfg in configs {
                    println!(
                        "  Supported: {} ch, {:?}, {}-{} Hz",
                        cfg.channels(),
                        cfg.sample_format(),
                        cfg.min_sample_rate().0,
                        cfg.max_sample_rate().0
                    );
                }
            }
            Err(err) => {
                eprintln!("  Supported config error: {}", err);
            }
        }
        index += 1;
    }
    Ok(())
}

impl AudioCapturer {
    pub fn new(cfg: &AudioConfig, debug: bool) -> Result<Self, AudioError> {
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
        let requested_rate = cfg.sample_rate;
        let selected_rate = select_sample_rate(&device, sample_format, stream_config.channels, requested_rate)
            .unwrap_or(stream_config.sample_rate.0);
        stream_config.sample_rate = SampleRate(selected_rate);
        let channels = stream_config.channels as usize;
        if debug {
            let device_name = device
                .name()
                .unwrap_or_else(|_| "Unknown input device".to_string());
            if selected_rate != requested_rate {
                println!(
                    "Requested {} Hz not supported; using {} Hz",
                    requested_rate, selected_rate
                );
            }
            println!(
                "Using input device: {} ({} ch @ {} Hz, {:?})",
                device_name,
                channels,
                stream_config.sample_rate.0,
                supported.sample_format()
            );
        }

        let actual_rate = stream_config.sample_rate.0;
        Ok(Self {
            device,
            config: stream_config,
            sample_format,
            channels,
            sample_rate: actual_rate,
            debug,
        })
    }

    pub fn capture(&self, duration: Duration) -> Result<Vec<i16>, AudioError> {
        let mut data = match self.sample_format {
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
        }?;

        if self.debug && !data.is_empty() {
            let target_peak = (i16::MAX as f32 * 0.8) as f32;
            let (peak, _rms) = peak_rms(&data);
            let mut scaled = false;
            if peak as f32 > target_peak {
                let scale = target_peak / peak as f32;
                for sample in data.iter_mut() {
                    *sample = (*sample as f32 * scale) as i16;
                }
                scaled = true;
            }
            let (peak_after, rms_after) = peak_rms(&data);
            let peak_pct = (peak_after as f64 / i16::MAX as f64) * 100.0;
            let rms_pct = (rms_after / i16::MAX as f64) * 100.0;
            if scaled {
                println!("Audio level: peak {:.1}%, rms {:.1}% (scaled)", peak_pct, rms_pct);
            } else {
                println!("Audio level: peak {:.1}%, rms {:.1}%", peak_pct, rms_pct);
            }
        }

        if self.sample_rate != 16_000 && data.len() > 1 {
            data = resample_linear(&data, self.sample_rate, 16_000);
        }

        Ok(data)
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

        let channels = self.channels.max(1);
        let stream = self
            .device
            .build_input_stream(
                &self.config,
                {
                    let convert = Arc::clone(&convert);
                    move |data: &[T], _| {
                        if let Ok(mut buf) = writer.lock() {
                            if channels == 1 {
                                buf.extend(data.iter().map(|sample| convert(*sample)));
                            } else {
                                for frame in data.chunks_exact(channels) {
                                    let mut sum: i32 = 0;
                                    for sample in frame {
                                        sum += convert(*sample) as i32;
                                    }
                                    let avg = (sum / channels as i32) as i16;
                                    buf.push(avg);
                                }
                            }
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

fn select_sample_rate(
    device: &Device,
    sample_format: SampleFormat,
    channels: u16,
    requested: u32,
) -> Option<u32> {
    let configs = device.supported_input_configs().ok()?;
    for cfg in configs {
        if cfg.sample_format() != sample_format || cfg.channels() != channels {
            continue;
        }
        if requested >= cfg.min_sample_rate().0 && requested <= cfg.max_sample_rate().0 {
            return Some(requested);
        }
    }
    None
}

fn peak_rms(samples: &[i16]) -> (i16, f64) {
    let (peak, sum_sq) = samples
        .iter()
        .fold((0i16, 0u64), |(peak, sum_sq), &sample| {
            let abs = sample.abs();
            let peak = peak.max(abs);
            let sum_sq = sum_sq + (sample as i32).saturating_mul(sample as i32) as u64;
            (peak, sum_sq)
        });
    let rms = (sum_sq as f64 / samples.len() as f64).sqrt();
    (peak, rms)
}

fn resample_linear(samples: &[i16], src_rate: u32, dst_rate: u32) -> Vec<i16> {
    if src_rate == dst_rate || samples.len() < 2 {
        return samples.to_vec();
    }
    if src_rate % dst_rate == 0 {
        let factor = (src_rate / dst_rate) as usize;
        if factor > 1 {
            let mut out = Vec::with_capacity(samples.len() / factor);
            for chunk in samples.chunks_exact(factor) {
                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                out.push((sum / factor as i32) as i16);
            }
            return out;
        }
    }
    let ratio = dst_rate as f64 / src_rate as f64;
    let out_len = ((samples.len() as f64) * ratio).max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 / ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = samples[idx] as f32;
        let s1 = samples.get(idx + 1).copied().unwrap_or(samples[idx]) as f32;
        let sample = s0 + (s1 - s0) * frac;
        out.push(sample as i16);
    }
    out
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
