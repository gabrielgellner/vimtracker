//! The `cpal` output stream, adapted from gmuse's `audio/device.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, SampleFormat, StreamConfig};
use thiserror::Error;
use vt_engine::link::Realtime;

/// The most frames rendered at once, so the scratch buffer can be made ahead of time.
const MAX_BLOCK: usize = 4096;

/// Why audio could not start.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no audio output device")]
    NoDevice,
    #[error("the output device uses {0:?}, and only f32 is supported")]
    Format(SampleFormat),
    #[error("the output device has {0} channel, and two are needed")]
    Channels(u16),
    #[error("audio device error: {0}")]
    Device(String),
}

/// Counters the callback updates and the status line reads.
#[derive(Debug, Default)]
pub struct AudioStats {
    /// Blocks where the callback took longer than the buffer's duration.
    pub late: AtomicU64,
    /// Underruns the system reported.
    pub errors: AtomicU64,
    /// Callback time as a share of buffer duration, in thousandths.
    pub load: AtomicU64,
    /// The device has gone, so the program exits.
    pub lost: AtomicBool,
}

/// A chosen device, before its stream starts.
pub struct Output {
    device: cpal::Device,
    config: StreamConfig,
}

impl Output {
    /// Use the default output device and its default configuration.
    pub fn open() -> Result<Self, AudioError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(AudioError::NoDevice)?;
        let default = device
            .default_output_config()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        if default.sample_format() != SampleFormat::F32 {
            return Err(AudioError::Format(default.sample_format()));
        }
        if default.channels() < 2 {
            return Err(AudioError::Channels(default.channels()));
        }
        Ok(Self {
            device,
            config: default.into(),
        })
    }

    /// The device's sample rate.
    pub fn rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Start playing. Audio stops when the returned stream is dropped.
    pub fn start(
        self,
        mut realtime: Realtime,
        stats: Arc<AudioStats>,
    ) -> Result<cpal::Stream, AudioError> {
        let channels = usize::from(self.config.channels);
        let rate = f64::from(self.config.sample_rate);
        let error_stats = Arc::clone(&stats);
        let mut scratch = vec![0.0_f32; MAX_BLOCK * 2];
        let stream = self
            .device
            .build_output_stream(
                self.config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let started = Instant::now();
                    for frames in out.chunks_mut(MAX_BLOCK * channels) {
                        let count = frames.len() / channels;
                        let block = &mut scratch[..count * 2];
                        realtime.process(block);
                        for (frame, stereo) in frames
                            .chunks_exact_mut(channels)
                            .zip(block.as_chunks::<2>().0)
                        {
                            frame[0] = stereo[0];
                            frame[1] = stereo[1];
                            frame[2..].fill(0.0);
                        }
                    }
                    let budget = (out.len() / channels) as f64 / rate;
                    let spent = started.elapsed().as_secs_f64();
                    if spent > budget {
                        stats.late.fetch_add(1, Ordering::Relaxed);
                    }
                    if budget > 0.0 {
                        let load = (spent / budget * 1000.0) as u64;
                        stats.load.store(load, Ordering::Relaxed);
                    }
                },
                // Never print from here: the terminal is showing the interface.
                move |error| match error.kind() {
                    ErrorKind::Xrun => {
                        error_stats.errors.fetch_add(1, Ordering::Relaxed);
                    }
                    ErrorKind::DeviceChanged
                    | ErrorKind::RealtimeDenied
                    | ErrorKind::DeviceBusy => {}
                    _ => error_stats.lost.store(true, Ordering::Release),
                },
                None,
            )
            .map_err(|e| AudioError::Device(e.to_string()))?;
        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(stream)
    }
}
