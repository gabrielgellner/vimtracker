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
use vt_engine::slowdown::{Look, SlowdownCheck};

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
    /// How many times slower than full speed the system is running the callback, in thousandths,
    /// as it last looked, or 0 before it has.
    pub slowdown: AtomicU64,
}

impl AudioStats {
    /// Start counting afresh, for a new stream.
    pub fn reset(&self) {
        for counter in [
            &self.late,
            &self.errors,
            &self.load,
            &self.latency,
            &self.slowdown,
        ] {
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

/// How many times slower than full speed the audio thread runs, for the leaf beside the CPU
/// figure. The callback looks four times a second and each look varies, so this smooths them, and
/// shows the leaf above ×1.6 and hides it below ×1.3, so it doesn't flicker about one figure.
#[derive(Debug, Clone)]
pub struct SlowdownReadout {
    /// The smoothed factor, which starts at full speed.
    factor: f64,
    /// When the last look was taken in.
    last: Option<Instant>,
    /// Whether the leaf is showing.
    slowed: bool,
    /// What the system says it's doing to save power, as last asked, for the explanation.
    pub power: PowerState,
}

impl Default for SlowdownReadout {
    fn default() -> Self {
        Self {
            factor: 1.0,
            last: None,
            slowed: false,
            power: PowerState::default(),
        }
    }
}

impl SlowdownReadout {
    /// The leaf shows once the smoothed factor is over this.
    pub const SHOW_ABOVE: f64 = 1.6;
    /// And goes once it's under this.
    pub const HIDE_BELOW: f64 = 1.3;
    /// How quickly the factor follows the looks: it goes nearly two thirds of the way to a new
    /// one in this long.
    const SETTLING: Duration = Duration::from_secs(2);

    /// Take in the callback's last look, in thousandths of full speed, or 0 before it has looked.
    pub fn sample(&mut self, thousandths: u64, now: Instant) {
        if thousandths == 0 {
            return;
        }
        let look = thousandths as f64 / 1000.0;
        if let Some(last) = self.last {
            let elapsed = now.saturating_duration_since(last).as_secs_f64();
            let share = 1.0 - (-elapsed / Self::SETTLING.as_secs_f64()).exp();
            self.factor += (look - self.factor) * share;
        }
        self.last = Some(now);
        if self.factor > Self::SHOW_ABOVE {
            self.slowed = true;
        } else if self.factor < Self::HIDE_BELOW {
            self.slowed = false;
        }
    }

    /// Whether the leaf shows: the system is running the audio thread slowly enough to matter.
    pub fn slowed(&self) -> bool {
        self.slowed
    }

    /// How many times slower than full speed, smoothed.
    pub fn factor(&self) -> f64 {
        self.factor
    }

    /// What the leaf means, for a CPU figure, while it shows: how slowed, and the figure at full
    /// speed, as `slowed ×3.6 by power saving, about 8% at full speed`.
    pub fn explanation(&self, cpu: u64) -> Option<String> {
        if !self.slowed {
            return None;
        }
        let full = (cpu as f64 / self.factor).round();
        let full = if full < 1.0 {
            "under 1%".to_owned()
        } else {
            format!("about {full:.0}%")
        };
        // Heat slows every core hardest, and Low Power Mode every core, where otherwise it's the
        // system's own choice of core and clock for a light load.
        let cause = if self.power.hot {
            "heat"
        } else if self.power.low_power {
            "Low Power Mode"
        } else {
            "power saving"
        };
        Some(format!(
            "slowed ×{:.1} by {cause}, {full} at full speed",
            self.factor
        ))
    }
}

/// Start working out the slowdown check's time at full speed on this machine, on a thread of its
/// own that asks for the performance cores, for 100 ms while other work goes on. Join it before
/// audio starts.
pub fn measure_full_speed() -> std::thread::JoinHandle<Duration> {
    std::thread::spawn(|| {
        ask_for_performance_cores();
        full_speed_here()
    })
}

/// The slowdown check's time at full speed on this machine: the faster of what it takes now and
/// the fastest it has taken before, which is kept in the cache folder. Starting in Low Power Mode,
/// which slows every core, or while the machine is busy, then still has full speed to go by.
fn full_speed_here() -> Duration {
    let now = vt_engine::slowdown::full_speed();
    let Some(path) = full_speed_path() else {
        return now;
    };
    let kept = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| read_full_speed(&text));
    match kept {
        Some(kept) if kept <= now => kept,
        _ => {
            // Nothing is lost if it can't be kept but the next start's comparison.
            if let Some(folder) = path.parent() {
                let _ = std::fs::create_dir_all(folder);
            }
            let _ = std::fs::write(&path, write_full_speed(now));
            now
        }
    }
}

/// Ask the system to run this thread as work the user is waiting on, which it puts on the
/// performance cores at their full clock.
#[cfg(target_os = "macos")]
fn ask_for_performance_cores() {
    // SAFETY: it changes only this thread's class, and takes no pointers.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn ask_for_performance_cores() {}

/// Where the fastest kernel time is kept: `full-speed.txt` in the program's cache folder.
fn full_speed_path() -> Option<PathBuf> {
    Some(vt_model::analysis::program_cache_folder()?.join("full-speed.txt"))
}

/// What a kept time is for: this kernel, in this kind of build, as a debug build runs it slower.
fn full_speed_key() -> String {
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    format!("kernel {} {build}", vt_engine::slowdown::KERNEL_VERSION)
}

/// A kept time, as `kernel 1 release 2125` in nanoseconds.
fn write_full_speed(time: Duration) -> String {
    format!("{} {}\n", full_speed_key(), time.as_nanos())
}

/// A kept time, if it's for this kernel and build.
fn read_full_speed(text: &str) -> Option<Duration> {
    let (key, nanos) = text.trim().rsplit_once(' ')?;
    let nanos: u64 = nanos.parse().ok().filter(|nanos| *nanos > 0)?;
    (key == full_speed_key()).then(|| Duration::from_nanos(nanos))
}

/// What the system says it's doing that slows every core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PowerState {
    /// Low Power Mode is on.
    pub low_power: bool,
    /// The Mac is hot enough that the system is slowing it to cool down: a thermal state of
    /// serious or critical.
    pub hot: bool,
}

impl PowerState {
    /// Ask the system. It's for the interface's thread, never the audio thread.
    #[cfg(target_os = "macos")]
    pub fn now() -> Self {
        use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
        objc2::rc::autoreleasepool(|_| {
            let process = NSProcessInfo::processInfo();
            Self {
                low_power: process.isLowPowerModeEnabled(),
                hot: process.thermalState().0 >= NSProcessInfoThermalState::Serious.0,
            }
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn now() -> Self {
        Self::default()
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
    /// is the second part, if the device says. `full_speed` is the slowdown check's time at full
    /// speed, from [`vt_engine::slowdown::full_speed`].
    pub fn start(
        self,
        mut realtime: Realtime,
        stats: Arc<AudioStats>,
        full_speed: Duration,
    ) -> Result<(cpal::Stream, Option<u32>), AudioError> {
        stats.reset();
        let channels = usize::from(self.config.channels);
        let rate = f64::from(self.config.sample_rate);
        let error_stats = Arc::clone(&stats);
        let mut scratch = vec![0.0_f32; MAX_BLOCK * 2];
        let mut slowdown = SlowdownCheck::new(full_speed, self.config.sample_rate);
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
                    let frames = out.len() / channels;
                    let budget = frames as f64 / rate;
                    let spent = started.elapsed().as_secs_f64();
                    if spent > budget {
                        stats.late.fetch_add(1, Ordering::Relaxed);
                    }
                    if budget > 0.0 {
                        let load = (spent / budget * 1000.0) as u64;
                        stats.load.store(load, Ordering::Relaxed);
                    }
                    // After the figure is taken, so the look isn't counted in it.
                    if let Some(look) = slowdown.after(frames) {
                        let factor = (look.factor * 1000.0) as u64;
                        stats.slowdown.store(factor, Ordering::Relaxed);
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

/// What one callback of a measured stream took, for `vimtracker bench --device`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CallbackTime {
    /// The frames the device asked for.
    pub frames: u32,
    /// The whole callback, as the status line times it.
    pub callback: Duration,
    /// The callback's end of the queues, the engine and the report.
    pub process: Duration,
    /// The engine alone.
    pub engine: Duration,
    /// The callback's thread's own CPU time, which leaves out any time it was put aside for
    /// another thread.
    pub cpu: Duration,
    /// The slowdown check's look, if it took one after this callback.
    pub look: Option<Look>,
    /// How long the check took, looking or not.
    pub looking: Duration,
}

/// How the system runs the callback's thread, as it says from inside the first callback.
#[derive(Debug, Clone, Copy, Default)]
pub struct CallbackThread {
    /// Whether it has a real-time policy: so much computation in each period, before a deadline.
    pub real_time: bool,
    /// That policy's period, computation and constraint, in microseconds.
    pub period_us: u32,
    pub computation_us: u32,
    pub constraint_us: u32,
}

/// The CPU time the calling thread has had.
pub fn thread_cpu_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid timespec for the call to fill.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut time) };
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

/// How the system runs the calling thread.
#[cfg(target_os = "macos")]
// libc would have the mach2 crate used for the timebase, which isn't worth a dependency here.
#[allow(deprecated)]
fn this_thread() -> CallbackThread {
    let mut policy = libc::thread_time_constraint_policy {
        period: 0,
        computation: 0,
        constraint: 0,
        preemptible: 0,
    };
    let mut count = (size_of::<libc::thread_time_constraint_policy>() / size_of::<u32>()) as u32;
    let mut default = 0;
    // SAFETY: the policy and its count are valid for the call to fill, and the thread is this one.
    let status = unsafe {
        libc::thread_policy_get(
            libc::pthread_mach_thread_np(libc::pthread_self()),
            libc::THREAD_TIME_CONSTRAINT_POLICY as u32,
            (&raw mut policy).cast(),
            &raw mut count,
            &raw mut default,
        )
    };
    let mut timebase = libc::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: `timebase` is valid for the call to fill.
    unsafe { libc::mach_timebase_info(&raw mut timebase) };
    let us = |ticks: u32| {
        (u64::from(ticks) * u64::from(timebase.numer) / u64::from(timebase.denom.max(1)) / 1000)
            as u32
    };
    CallbackThread {
        real_time: status == 0 && default == 0,
        period_us: us(policy.period),
        computation_us: us(policy.computation),
        constraint_us: us(policy.constraint),
    }
}

#[cfg(not(target_os = "macos"))]
fn this_thread() -> CallbackThread {
    CallbackThread::default()
}

/// What a measured stream sends back.
pub struct Measured {
    /// Each callback's times, as they come.
    pub times: rtrb::Consumer<CallbackTime>,
    /// The thread the callbacks run on, once the first has.
    pub thread: rtrb::Consumer<CallbackThread>,
    /// The callback's end, once `stop` is set and the next callback has given it up.
    pub realtime: rtrb::Consumer<Realtime>,
    /// Set to have the callback stop playing and give its end back.
    pub stop: Arc<AtomicBool>,
    /// Underruns the system reported.
    pub xruns: Arc<AtomicU64>,
}

impl Output {
    /// Play through the device as [`Output::start`] does, timing each callback, but send the
    /// device only silence. For `vimtracker bench --device`, which hears nothing. The times of
    /// up to `blocks` callbacks wait to be read.
    pub fn start_measured(
        self,
        realtime: Realtime,
        blocks: usize,
        full_speed: Duration,
    ) -> Result<(cpal::Stream, Measured), AudioError> {
        let channels = usize::from(self.config.channels);
        let mut slowdown = SlowdownCheck::new(full_speed, self.config.sample_rate);
        let mut scratch = vec![0.0_f32; MAX_BLOCK * 2];
        let (mut times_in, times) = rtrb::RingBuffer::new(blocks.max(1));
        let (mut thread_in, thread) = rtrb::RingBuffer::new(1);
        let (mut realtime_in, realtime_out) = rtrb::RingBuffer::new(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let mut realtime = Some(realtime);
        let mut first = true;
        let xruns = Arc::new(AtomicU64::new(0));
        let counting = Arc::clone(&xruns);
        let stream = self
            .device
            .build_output_stream(
                self.config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let started = Instant::now();
                    let cpu = thread_cpu_time();
                    if first {
                        first = false;
                        let _ = thread_in.push(this_thread());
                    }
                    if stopping.load(Ordering::Acquire)
                        && let Some(done) = realtime.take()
                    {
                        let _ = realtime_in.push(done);
                    }
                    let Some(realtime) = &mut realtime else {
                        out.fill(0.0);
                        return;
                    };
                    let (mut process, mut engine) = (Duration::ZERO, Duration::ZERO);
                    for frames in out.chunks_mut(MAX_BLOCK * channels) {
                        let count = frames.len() / channels;
                        let block = &mut scratch[..count * 2];
                        let processing = Instant::now();
                        realtime.process(block);
                        process += processing.elapsed();
                        engine += realtime.engine_time();
                        // The same as the stream that plays, so it costs the same.
                        for (frame, stereo) in frames
                            .chunks_exact_mut(channels)
                            .zip(block.as_chunks::<2>().0)
                        {
                            frame[0] = stereo[0].clamp(-1.0, 1.0);
                            frame[1] = stereo[1].clamp(-1.0, 1.0);
                            frame[2..].fill(0.0);
                        }
                    }
                    // Then nothing is heard.
                    std::hint::black_box(&mut *out).fill(0.0);
                    let frames = out.len() / channels;
                    let callback = started.elapsed();
                    let cpu = thread_cpu_time().saturating_sub(cpu);
                    // After the times are taken, as the playing stream's is.
                    let looking = Instant::now();
                    let look = slowdown.after(frames);
                    let looking = looking.elapsed();
                    let _ = times_in.push(CallbackTime {
                        frames: frames as u32,
                        callback,
                        process,
                        engine,
                        cpu,
                        look,
                        looking,
                    });
                },
                move |error| {
                    if error.kind() == ErrorKind::Xrun {
                        counting.fetch_add(1, Ordering::Relaxed);
                    }
                },
                None,
            )
            .map_err(|e| AudioError::Device(e.to_string()))?;
        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        Ok((
            stream,
            Measured {
                times,
                thread,
                realtime: realtime_out,
                stop,
                xruns,
            },
        ))
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

    /// A readout given the same look every frame, 30 a second, for so long.
    fn held(readout: &mut SlowdownReadout, thousandths: u64, from: Instant, ms: u64) -> Instant {
        let mut now = from;
        for _ in 0..ms / 33 {
            now += Duration::from_millis(33);
            readout.sample(thousandths, now);
        }
        now
    }

    #[test]
    fn the_slowdown_is_smoothed_and_only_counts_once_looked_at() {
        let start = Instant::now();
        let mut readout = SlowdownReadout::default();
        // Before the callback has looked, it's at full speed.
        readout.sample(0, start);
        assert_eq!(readout.factor(), 1.0);
        readout.sample(3600, start);
        assert_eq!(readout.factor(), 1.0, "the first look starts the clock");
        // Nearly two thirds of the way in two seconds.
        let now = held(&mut readout, 3600, start, 2000);
        assert!((readout.factor() - 2.6).abs() < 0.1, "{}", readout.factor());
        held(&mut readout, 3600, now, 10_000);
        assert!((readout.factor() - 3.6).abs() < 0.01);
    }

    #[test]
    fn the_leaf_shows_over_one_point_six_and_goes_under_one_point_three() {
        let start = Instant::now();
        let mut readout = SlowdownReadout::default();
        readout.sample(1000, start);
        let now = held(&mut readout, 1500, start, 20_000);
        assert!(!readout.slowed(), "x1.5 isn't enough to show it");
        let now = held(&mut readout, 1700, now, 20_000);
        assert!(readout.slowed());
        // Between the two, it stays as it was, either way.
        let now = held(&mut readout, 1400, now, 20_000);
        assert!(readout.slowed(), "x1.4 doesn't hide it");
        let now = held(&mut readout, 1200, now, 20_000);
        assert!(!readout.slowed());
        held(&mut readout, 1400, now, 20_000);
        assert!(!readout.slowed(), "x1.4 doesn't show it again");
    }

    #[test]
    fn a_short_spike_doesnt_show_the_leaf() {
        let start = Instant::now();
        let mut readout = SlowdownReadout::default();
        readout.sample(1000, start);
        let now = held(&mut readout, 1000, start, 5000);
        // A look stays up a quarter of a second until the next: one at four times.
        let now = held(&mut readout, 4000, now, 250);
        assert!(!readout.slowed(), "{}", readout.factor());
        held(&mut readout, 1000, now, 5000);
        assert!(!readout.slowed());
    }

    #[test]
    fn the_explanation_says_how_slowed_and_the_figure_at_full_speed() {
        let start = Instant::now();
        let mut readout = SlowdownReadout::default();
        readout.sample(3600, start);
        assert_eq!(
            readout.explanation(29),
            None,
            "not while it's at full speed"
        );
        held(&mut readout, 3600, start, 30_000);
        assert_eq!(
            readout.explanation(29).as_deref(),
            Some("slowed ×3.6 by power saving, about 8% at full speed")
        );
        assert_eq!(
            readout.explanation(1).as_deref(),
            Some("slowed ×3.6 by power saving, under 1% at full speed")
        );
        readout.power.low_power = true;
        assert_eq!(
            readout.explanation(29).as_deref(),
            Some("slowed ×3.6 by Low Power Mode, about 8% at full speed")
        );
        readout.power.hot = true;
        assert_eq!(
            readout.explanation(29).as_deref(),
            Some("slowed ×3.6 by heat, about 8% at full speed")
        );
    }

    #[test]
    fn a_kept_full_speed_is_read_back_only_for_this_kernel_and_build() {
        let time = Duration::from_nanos(1458);
        let text = write_full_speed(time);
        assert_eq!(read_full_speed(&text), Some(time));
        let other = text.replacen(&full_speed_key(), "kernel 0 release", 1);
        let other = if other == text {
            text.replacen(&full_speed_key(), "kernel 0 debug", 1)
        } else {
            other
        };
        assert_eq!(read_full_speed(&other), None);
        assert_eq!(read_full_speed(""), None);
        assert_eq!(read_full_speed(&format!("{} 0", full_speed_key())), None);
        assert_eq!(read_full_speed(&format!("{} x", full_speed_key())), None);
    }
}
