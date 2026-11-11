//! `vimtracker bench`: how much of a core a saved project takes to play, as the status line's CPU
//! figure works it out, and where that time goes.
//!
//! It plays three ways. Flat out, block after block with no deadline, is the engine's own cost.
//! Paced, each block waits for its time on an ordinary thread, so the system sees a light load
//! as the audio thread's. Through the device, the real output stream's callback plays the song
//! and sends the device silence, which is the figure the status line shows, apart from the
//! interface drawing alongside it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use vt_engine::engine::Engine;
use vt_engine::link::{Command, connect};
use vt_engine::published::Published;
use vt_engine::timing::{PartTimes, clock_cost};
use vt_model::constants::FRAME_MS;
use vt_model::insert::INSERTS;
use vt_model::project;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Song};

use crate::audio::{AudioChoice, CallbackTime, Output, thread_cpu_time};
use crate::run::{RunError, analyse_now};

/// How the blocks are played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// As fast as they go.
    FlatOut,
    /// Each when its time comes, on an ordinary thread.
    Paced,
    /// Through the output device's callback, silently.
    Device,
}

/// What `vimtracker bench` was asked for.
pub struct BenchRequest {
    pub project: PathBuf,
    /// The rate and block size, except through the device, which uses its own.
    pub rate: u32,
    pub block: usize,
    pub seconds: u32,
    pub pace: Pace,
    /// Say how long each track, slot, group, return and the master take while playing.
    pub tracks: bool,
}

/// A block's time on the clock, and on the CPU for its thread.
#[derive(Debug, Clone, Copy, Default)]
struct BlockTime {
    wall: Duration,
    cpu: Duration,
}

/// What a run of blocks came to.
struct Summary {
    /// The share of a core on average and in the worst block, as percentages.
    average: f64,
    worst: f64,
    /// Where the worst block fell, in blocks from the start.
    worst_at: usize,
    /// The share of the worst block its thread was on the CPU, which is less than all of it when
    /// the system put it aside for another thread.
    worst_on_cpu: f64,
    /// The blocks that took longer than they last.
    late: usize,
    count: usize,
}

impl Summary {
    fn of(blocks: &[BlockTime], budget: f64) -> Self {
        let count = blocks.len().max(1);
        let total: f64 = blocks.iter().map(|block| block.wall.as_secs_f64()).sum();
        let (worst_at, worst) = blocks
            .iter()
            .enumerate()
            .max_by_key(|(_, block)| block.wall)
            .map_or((0, BlockTime::default()), |(at, block)| (at, *block));
        Self {
            average: total / count as f64 / budget * 100.0,
            worst: worst.wall.as_secs_f64() / budget * 100.0,
            worst_at,
            worst_on_cpu: worst.cpu.as_secs_f64() / worst.wall.as_secs_f64().max(1e-9) * 100.0,
            late: blocks
                .iter()
                .filter(|block| block.wall.as_secs_f64() > budget)
                .count(),
            count: blocks.len(),
        }
    }

    /// One line of the report.
    fn line(&self, name: &str, block: usize, rate: u32) -> String {
        let at = self.worst_at as f64 * block as f64 / f64::from(rate);
        format!(
            "{name:<8} {:5.2}% of a core, worst block {:5.1}% at {at:.2} s (block {}, {:.0}% of it \
             on the CPU), {} of {} blocks late",
            self.average, self.worst, self.worst_at, self.worst_on_cpu, self.late, self.count
        )
    }
}

/// Load a project's song and samples at a rate, with the bank analysed.
fn load(folder: &Path, rate: u32) -> Result<(Song, Arc<Published>), RunError> {
    let song = project::load_song(folder)?;
    let mut pool = SamplePool::default();
    let errors = project::load_samples(Some(folder), &song, rate, &mut pool);
    if let Some(first) = errors.first() {
        return Err(RunError::MissingSample(first.to_string()));
    }
    analyse_now(Some(folder), &song, &mut pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::new(pool)));
    Ok((song, version))
}

/// Play and report as asked.
pub fn run(request: &BenchRequest) -> Result<(), RunError> {
    match request.pace {
        Pace::FlatOut | Pace::Paced => on_this_thread(request),
        Pace::Device => through_device(request),
    }
}

/// Play blocks on this thread, flat out or paced.
fn on_this_thread(request: &BenchRequest) -> Result<(), RunError> {
    let rate = request.rate;
    let (song, version) = load(&request.project, rate)?;
    let block = request.block.max(1);
    let blocks = (request.seconds as usize * rate as usize).div_ceil(block);
    let budget = block as f64 / f64::from(rate);
    let paced = request.pace == Pace::Paced;
    let mut out = vec![0.0_f32; block * 2];
    let mut times = vec![BlockTime::default(); blocks];
    let mut measure = |engine: &mut Engine, times: &mut [BlockTime]| {
        let began = Instant::now();
        for (index, time) in times.iter_mut().enumerate() {
            let started = Instant::now();
            let cpu = thread_cpu_time();
            engine.process(&mut out);
            *time = BlockTime {
                wall: started.elapsed(),
                cpu: thread_cpu_time().saturating_sub(cpu),
            };
            if paced {
                let due = began + Duration::from_secs_f64(budget * (index + 1) as f64);
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
            }
        }
    };
    let mut engine = Engine::new(rate, Arc::clone(&version));
    measure(&mut engine, &mut times);
    let stopped = Summary::of(&times, budget);
    // Starting to play happens in a callback too, ahead of the block it starts in.
    let starting = Instant::now();
    engine.play_from(0);
    let starting = starting.elapsed().as_secs_f64();
    measure(&mut engine, &mut times);
    let playing = Summary::of(&times, budget);
    let pace = if paced { "paced" } else { "flat out" };
    println!(
        "{} at {rate} Hz in blocks of {block}, {} s each, {pace}",
        request.project.display(),
        request.seconds
    );
    println!("{}", stopped.line("stopped", block, rate));
    println!("{}", playing.line("playing", block, rate));
    println!(
        "starting to play took {:.0} us, {:.1}% of a block, ahead of the engine's own",
        starting * 1e6,
        starting / budget * 100.0
    );
    if request.tracks {
        // Again from the start, timed, so the figures above aren't slowed by the timing.
        let mut engine = Engine::new(rate, version);
        engine.play_from(0);
        engine.time_parts();
        measure(&mut engine, &mut times);
        let timed = Summary::of(&times, budget);
        let parts = engine.part_times().unwrap_or_default();
        print_parts(&song, &parts, blocks as f64 * budget, timed.average);
    }
    Ok(())
}

/// Play through the output device, silently, and time its callbacks.
fn through_device(request: &BenchRequest) -> Result<(), RunError> {
    let choice = AudioChoice::load();
    let output = Output::open(&choice).or_else(|_| Output::open(&AudioChoice::default()))?;
    let rate = output.rate();
    let name = output.name.clone();
    let (song, version) = load(&request.project, rate)?;
    let mut engine = Engine::new(rate, version);
    // Timed, it plays from the start, as only playing is broken down.
    if request.tracks {
        engine.play_from(0);
        engine.time_parts();
    }
    let (mut handle, realtime) = connect(engine);
    // Room for every callback of both runs, at the smallest buffer offered.
    let room = (request.seconds as usize * 2 + 1) * rate as usize / 16;
    let (stream, mut measured) = output.start_measured(realtime, room)?;
    let mut scope = Vec::with_capacity(rate as usize);
    let mut run_for = |handle: &mut vt_engine::link::Handle, times: &mut Vec<CallbackTime>| {
        let until = Instant::now() + Duration::from_secs(u64::from(request.seconds));
        // As the interface does each frame: send what's waiting, and read the report and scope.
        while Instant::now() < until {
            handle.pump();
            let _ = handle.latest();
            scope.clear();
            handle.read_scope(&mut scope);
            while handle.jam_change().is_some() {}
            while let Ok(time) = measured.times.pop() {
                times.push(time);
            }
            std::thread::sleep(Duration::from_millis(FRAME_MS));
        }
    };
    let mut stopped = Vec::new();
    if !request.tracks {
        run_for(&mut handle, &mut stopped);
        handle.send(Command::PlayFrom { step: 0 });
    }
    let mut playing = Vec::new();
    run_for(&mut handle, &mut playing);
    measured.stop.store(true, Ordering::Release);
    let realtime = loop {
        if let Ok(realtime) = measured.realtime.pop() {
            break realtime;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    drop(stream);
    let thread = measured.thread.pop().unwrap_or_default();
    // The first callback after starting to play may still be stopped, which doesn't matter.
    let frames = playing.first().map_or(0, |time| time.frames);
    println!(
        "{} through {name} at {rate} Hz, {frames} frames a callback, silent, {} s each",
        request.project.display(),
        request.seconds
    );
    if thread.real_time {
        println!(
            "The callback's thread is real-time: {} us of computation in each {} us, by {} us",
            thread.computation_us, thread.period_us, thread.constraint_us
        );
    } else {
        println!("The callback's thread isn't real-time");
    }
    println!(
        "{:<8} {:>9} {:>9} {:>9} {:>9}   as shares of a core, average and worst",
        "", "callback", "process", "engine", "on CPU"
    );
    for (title, times) in [("stopped", &stopped), ("playing", &playing)] {
        if times.is_empty() {
            continue;
        }
        print_callbacks(title, times, rate);
    }
    println!(
        "{} underruns reported",
        measured.xruns.load(Ordering::Relaxed)
    );
    if request.tracks {
        let mut engine = realtime.into_engine();
        let parts = engine.part_times().unwrap_or_default();
        let audio: f64 = playing
            .iter()
            .map(|time| f64::from(time.frames) / f64::from(rate))
            .sum();
        let engine_share = share(playing.iter().map(|time| time.engine), audio);
        print_parts(&song, &parts, audio, engine_share);
    }
    Ok(())
}

/// The share of some audio's time some durations take, as a percentage.
fn share(times: impl Iterator<Item = Duration>, audio: f64) -> f64 {
    times.map(|time| time.as_secs_f64()).sum::<f64>() / audio.max(1e-9) * 100.0
}

/// Two lines of the device's report: the average share of each part, and the worst callback's.
fn print_callbacks(title: &str, times: &[CallbackTime], rate: u32) {
    let audio: f64 = times
        .iter()
        .map(|time| f64::from(time.frames) / f64::from(rate))
        .sum();
    let average = [
        share(times.iter().map(|time| time.callback), audio),
        share(times.iter().map(|time| time.process), audio),
        share(times.iter().map(|time| time.engine), audio),
        share(times.iter().map(|time| time.cpu), audio),
    ];
    let budget = |time: &CallbackTime| f64::from(time.frames) / f64::from(rate);
    let (worst_at, worst) = times
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            let a = a.callback.as_secs_f64() / budget(a);
            let b = b.callback.as_secs_f64() / budget(b);
            a.total_cmp(&b)
        })
        .map_or((0, CallbackTime::default()), |(at, time)| (at, *time));
    let of_worst = |time: Duration| time.as_secs_f64() / budget(&worst).max(1e-9) * 100.0;
    let late = times
        .iter()
        .filter(|time| time.callback.as_secs_f64() > budget(time))
        .count();
    println!(
        "{title:<8} {:>8.2}% {:>8.2}% {:>8.2}% {:>8.2}%",
        average[0], average[1], average[2], average[3]
    );
    println!(
        "{:<8} {:>8.1}% {:>8.1}% {:>8.1}% {:>8.1}%   callback {worst_at} of {}, {late} late",
        "  worst",
        of_worst(worst.callback),
        of_worst(worst.process),
        of_worst(worst.engine),
        of_worst(worst.cpu),
        times.len()
    );
}

/// The table of where the engine's time went while playing, as shares of a core, with what the
/// clock's own reads cost taken off.
fn print_parts(song: &Song, parts: &PartTimes, audio: f64, timed_average: f64) {
    let cost = clock_cost().as_secs_f64();
    let percent = |time: Duration, reads: u64| {
        (time.as_secs_f64() - reads as f64 * cost).max(0.0) / audio * 100.0
    };
    println!();
    println!(
        "Where the time goes while playing, as shares of a core, less {:.0} ns for each of the \
         timing's own clock reads:",
        cost * 1e9
    );
    println!(
        "{:<4} {:<6} {:<12} {:>7} {:>7} {:>7}   slots",
        "", "name", "machine", "voice", "rest", "total"
    );
    let mut sum = 0.0;
    for (index, track) in song.tracks.iter().enumerate() {
        let slot_reads: u64 = parts.slot_laps[index].iter().sum();
        let voice = percent(parts.voices[index], parts.voice_laps[index]);
        // The slots' own reads land in the track's time too.
        let rest = percent(
            parts.shaping[index],
            parts.shaping_laps[index] + 2 * slot_reads,
        );
        sum += voice + rest;
        let number = if track.is_sound() {
            format!("{:02}", index + 1)
        } else {
            track.kind.letter()
        };
        let machine = if !track.is_sound() {
            String::new()
        } else {
            match track.machine {
                Machine::Sampler => "Sampler".to_owned(),
                Machine::Plaits(voice) => voice.name().to_owned(),
            }
        };
        let slots: Vec<String> = (0..INSERTS)
            .filter(|&slot| parts.slot_laps[index][slot] > 0)
            .map(|slot| {
                format!(
                    "{} {:.2}%",
                    track.inserts[slot].name(),
                    percent(parts.slots[index][slot], parts.slot_laps[index][slot])
                )
            })
            .collect();
        let voice = if track.is_sound() {
            format!("{voice:6.2}%")
        } else {
            String::new()
        };
        println!(
            "{number:<4} {:<6} {machine:<12} {voice:>7} {rest:6.2}% {:6.2}%   {}",
            track.name,
            percent(parts.voices[index], parts.voice_laps[index]) + rest,
            slots.join(", ")
        );
    }
    let sequencing = percent(parts.sequencing, parts.sequencing_laps);
    let mixing = percent(parts.mixing, parts.mixing_laps);
    let limiting = percent(parts.limiting, parts.limiting_laps);
    sum += sequencing + mixing + limiting;
    println!("{:<24} {:>7} {sequencing:6.2}%", "sequencing", "");
    println!("{:<24} {:>7} {mixing:6.2}%", "mixing", "");
    println!("{:<24} {:>7} {limiting:6.2}%", "limiter", "");
    println!(
        "{:<24} {:>7} {sum:6.2}%   of {timed_average:.2}% timed, the rest being the clock",
        "all", ""
    );
}
