//! The `cpal` output stream, adapted from gmuse's `audio/device.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use std::path::PathBuf;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, SampleFormat, StreamConfig, SupportedBufferSize};
use serde::{Deserialize, Serialize};
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
    /// The device has changed or gone, so audio stops.
    pub lost: AtomicBool,
    /// How long from the callback to the sound leaving the device, in microseconds, as the device
    /// reports it: the buffer and the device's own delay, which on Bluetooth is most of it.
    pub latency: AtomicU64,
}

impl AudioStats {
    /// Start counting afresh, for a new stream.
    pub fn reset(&self) {
        for counter in [&self.late, &self.errors, &self.load, &self.latency] {
            counter.store(0, Ordering::Relaxed);
        }
        self.lost.store(false, Ordering::Release);
    }
}

/// The CPU figure for the status line. The callback's own number changes every block, so this
/// averages it and only changes the figure now and then.
#[derive(Debug, Default)]
pub struct LoadReadout {
    total: u64,
    samples: u64,
    since: Option<Instant>,
    shown: u64,
}

impl LoadReadout {
    /// How long each figure stays up.
    const EVERY: Duration = Duration::from_millis(500);

    /// Add one look at the callback's load, in thousandths of the buffer.
    pub fn sample(&mut self, load: u64, now: Instant) {
        let since = *self.since.get_or_insert(now);
        self.total += load;
        self.samples += 1;
        if now.saturating_duration_since(since) >= Self::EVERY {
            self.shown = (self.total + self.samples * 5) / (self.samples * 10);
            *self = Self {
                since: Some(now),
                shown: self.shown,
                ..Self::default()
            };
        }
    }

    /// The figure to show, as a whole percent.
    pub fn percent(&self) -> u64 {
        self.shown
    }
}

/// The sample rates a device is offered at, where it supports them.
const STANDARD_RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];
/// The buffer sizes offered, in frames, where a device supports them.
const BUFFER_SIZES: [u32; 8] = [32, 64, 128, 256, 512, 1024, 2048, 4096];

/// What the audio page has chosen, remembered for this machine rather than the song. Each is the
/// system's own choice when it's `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioChoice {
    /// The device's id, as `coreaudio:...`.
    pub device: Option<String>,
    pub rate: Option<u32>,
    /// In frames.
    pub buffer: Option<u32>,
}

impl AudioChoice {
    /// Where it's kept: `audio.ron` in the config folder, `$XDG_CONFIG_HOME/vimtracker` or
    /// `~/.config/vimtracker`.
    pub fn path() -> Option<PathBuf> {
        crate::config::config_folder().map(|folder| folder.join("audio.ron"))
    }

    /// The choice kept from last time, or the system's own if there's none or it won't read.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keep it for next time.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = ron::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// An output device and what it can do, for the audio page to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// The standard rates it plays at, lowest first.
    pub rates: Vec<u32>,
    /// The buffer sizes it takes, smallest first. Empty if it doesn't say.
    pub buffers: Vec<u32>,
    /// Whether it's the system's default output.
    pub default: bool,
}

/// A range of rates a device plays f32 in stereo at, with the range of buffer sizes it takes
/// there, if it says.
type ConfigRange = (u32, u32, Option<(u32, u32)>);

/// The rates and buffer sizes a device's ranges allow, of the ones offered.
fn offered(ranges: &[ConfigRange]) -> (Vec<u32>, Vec<u32>) {
    let rates = STANDARD_RATES
        .into_iter()
        .filter(|rate| {
            ranges
                .iter()
                .any(|(low, high, _)| (low..=high).contains(&rate))
        })
        .collect();
    let buffers = BUFFER_SIZES
        .into_iter()
        .filter(|size| {
            ranges
                .iter()
                .any(|(_, _, buffer)| buffer.is_some_and(|(low, high)| (low..=high).contains(size)))
        })
        .collect();
    (rates, buffers)
}

/// The ranges of a device's configurations that the engine can play through: f32, in stereo or
/// more, as rate ranges with their buffer size ranges.
fn usable_ranges(device: &cpal::Device) -> Vec<ConfigRange> {
    device
        .supported_output_configs()
        .map(|configs| {
            configs
                .filter(|config| {
                    config.sample_format() == SampleFormat::F32 && config.channels() >= 2
                })
                .map(|config| {
                    let buffer = match *config.buffer_size() {
                        SupportedBufferSize::Range { min, max } => Some((min, max)),
                        SupportedBufferSize::Unknown => None,
                    };
                    (config.min_sample_rate(), config.max_sample_rate(), buffer)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every output device the engine can play through.
pub fn devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default = host
        .default_output_device()
        .and_then(|device| device.id().ok());
    let Ok(found) = host.output_devices() else {
        return Vec::new();
    };
    found
        .filter_map(|device| {
            let id = device.id().ok()?;
            let ranges = usable_ranges(&device);
            if ranges.is_empty() {
                return None;
            }
            let (rates, buffers) = offered(&ranges);
            Some(DeviceInfo {
                name: device
                    .description()
                    .map_or_else(|_| id.to_string(), |about| about.name().to_owned()),
                default: default.as_ref() == Some(&id),
                id: id.to_string(),
                rates,
                buffers,
            })
        })
        .collect()
}

/// A chosen device, before its stream starts.
pub struct Output {
    device: cpal::Device,
    config: StreamConfig,
    /// What it's called, for the audio page.
    pub name: String,
    /// What it couldn't do of what was chosen, for the status line.
    pub note: Option<String>,
}

impl Output {
    /// Open what's chosen, falling back to the system's own choice for anything that isn't there
    /// now, and saying so in `note`.
    pub fn open(choice: &AudioChoice) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let mut note = None;
        let chosen = choice.device.as_deref().and_then(|id| {
            let found = id
                .parse()
                .ok()
                .and_then(|id: cpal::DeviceId| host.device_by_id(&id));
            if found.is_none() {
                note = Some("The chosen audio device isn't there, so the default plays".to_owned());
            }
            found
        });
        let device = chosen
            .or_else(|| host.default_output_device())
            .ok_or(AudioError::NoDevice)?;
        let default = device
            .default_output_config()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let ranges = usable_ranges(&device);
        // A chosen rate needs a range of the device's that plays it.
        let config = match choice.rate {
            Some(rate) if rate != default.sample_rate() => device
                .supported_output_configs()
                .ok()
                .and_then(|mut configs| {
                    configs.find_map(|config| {
                        (config.sample_format() == SampleFormat::F32 && config.channels() >= 2)
                            .then(|| config.try_with_sample_rate(rate))
                            .flatten()
                    })
                })
                .unwrap_or(default),
            _ => default,
        };
        if config.sample_format() != SampleFormat::F32 {
            return Err(AudioError::Format(config.sample_format()));
        }
        if config.channels() < 2 {
            return Err(AudioError::Channels(config.channels()));
        }
        let mut config: StreamConfig = config.into();
        let (_, buffers) = offered(&ranges);
        if let Some(buffer) = choice.buffer.filter(|size| buffers.contains(size)) {
            config.buffer_size = BufferSize::Fixed(buffer);
        }
        let name = device
            .description()
            .map_or_else(|_| "Audio".to_owned(), |about| about.name().to_owned());
        Ok(Self {
            device,
            config,
            name,
            note,
        })
    }

    /// The device's sample rate.
    pub fn rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Start playing. Audio stops when the returned stream is dropped. The buffer it plays with
    /// is the second part, if the device says.
    pub fn start(
        self,
        mut realtime: Realtime,
        stats: Arc<AudioStats>,
    ) -> Result<(cpal::Stream, Option<u32>), AudioError> {
        stats.reset();
        let channels = usize::from(self.config.channels);
        let rate = f64::from(self.config.sample_rate);
        let error_stats = Arc::clone(&stats);
        let mut scratch = vec![0.0_f32; MAX_BLOCK * 2];
        let stream = self
            .device
            .build_output_stream(
                self.config,
                move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    let started = Instant::now();
                    let when = info.timestamp();
                    let latency = when.playback.duration_since(when.callback);
                    stats
                        .latency
                        .store(latency.as_micros() as u64, Ordering::Relaxed);
                    for frames in out.chunks_mut(MAX_BLOCK * channels) {
                        let count = frames.len() / channels;
                        let block = &mut scratch[..count * 2];
                        realtime.process(block);
                        for (frame, stereo) in frames
                            .chunks_exact_mut(channels)
                            .zip(block.as_chunks::<2>().0)
                        {
                            // Whatever the limiter setting, nothing over full reaches the device.
                            frame[0] = stereo[0].clamp(-1.0, 1.0);
                            frame[1] = stereo[1].clamp(-1.0, 1.0);
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
                // Never print from here: the terminal is showing the interface. A device that
                // changes or goes away stops audio, rather than the program following it.
                move |error| match error.kind() {
                    ErrorKind::Xrun => {
                        error_stats.errors.fetch_add(1, Ordering::Relaxed);
                    }
                    ErrorKind::RealtimeDenied | ErrorKind::DeviceBusy => {}
                    _ => error_stats.lost.store(true, Ordering::Release),
                },
                None,
            )
            .map_err(|e| AudioError::Device(e.to_string()))?;
        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let buffer = stream.buffer_size().ok();
        Ok((stream, buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_offers_the_standard_rates_and_buffers_its_ranges_allow() {
        let (rates, buffers) =
            offered(&[(44_100, 48_000, Some((64, 600))), (96_000, 96_000, None)]);
        assert_eq!(rates, [44_100, 48_000, 96_000]);
        assert_eq!(buffers, [64, 128, 256, 512]);
        assert_eq!(offered(&[(44_100, 44_100, None)]).1, Vec::<u32>::new());
    }

    #[test]
    fn a_choice_reads_back_as_it_was_written() {
        let choice = AudioChoice {
            device: Some("coreaudio:BuiltIn".to_owned()),
            rate: Some(44_100),
            buffer: Some(128),
        };
        let text = ron::to_string(&choice).unwrap();
        assert_eq!(ron::from_str::<AudioChoice>(&text).unwrap(), choice);
        assert_eq!(
            ron::from_str::<AudioChoice>("()").unwrap(),
            AudioChoice::default()
        );
    }

    #[test]
    fn the_cpu_figure_is_a_whole_average_that_changes_only_now_and_then() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut readout = LoadReadout::default();
        assert_eq!(readout.percent(), 0);
        readout.sample(70, at(0));
        readout.sample(100, at(100));
        assert_eq!(readout.percent(), 0, "still gathering the first figure");
        readout.sample(40, at(500));
        assert_eq!(readout.percent(), 7);
        // The callback's number can jump around without the figure moving.
        readout.sample(900, at(600));
        assert_eq!(readout.percent(), 7);
        readout.sample(100, at(1000));
        assert_eq!(readout.percent(), 50);
    }
}
