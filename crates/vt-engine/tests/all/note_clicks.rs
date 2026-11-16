//! Clicks where one note gives way to the next, as heard on a synth playing a note on every 16th
//! with nothing tweaked. These measure, through the whole engine, what each note change does to
//! the level, to the top of the spectrum and to the offset from zero, folded over the step and
//! averaged over many random notes, against the same notes tied with `HLD`, whose voice carries on
//! from note to note without a new attack: a mono synth's legato, the cleanest a change can be.
//!
//! Each prints what it measures, as the diagnosis of #931 did, and asserts what a clean change
//! does. Those still ignored assert what today's engine doesn't do yet. Run them all with
//! `cargo test --release -p vt-engine --test all note_clicks -- --include-ignored --nocapture`.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// The windows the level is measured over, half a millisecond.
const WINDOW: usize = 24;
/// How many rows of random notes, and how many times they play.
const ROWS: usize = 64;
const LOOPS: usize = 2;
/// The sampled saw's period: 240 Hz at `C-3`, so a step at 120 BPM is 30 whole cycles.
const PERIOD: usize = 200;

/// What a track plays.
#[derive(Clone, Copy)]
enum Source {
    Synth(SynthVoice),
    /// A sampler playing a plain saw, long enough to tie through every note.
    SampledSaw,
}

/// How a track's notes are shaped and timed.
#[derive(Clone, Copy)]
struct Case {
    source: Source,
    bpm: u32,
    /// The first slot's ADSR, if it's changed from the plain note shape.
    adsr: Option<Adsr>,
    bypassed: bool,
    gate: Option<u8>,
    /// `VA`'s `Drift` at its least, so each note starts anywhere in its cycle and what the
    /// waveform does as it starts averages out over the notes, leaving what the change does.
    drift: bool,
}

impl Case {
    fn plain(source: Source) -> Self {
        Self {
            source,
            bpm: 120,
            adsr: None,
            bypassed: false,
            gate: None,
            drift: false,
        }
    }

    /// Frames in a step, which at 140 BPM isn't whole.
    fn step(&self) -> f64 {
        f64::from(RATE) * 60.0 / f64::from(self.bpm) / 4.0
    }
}

fn saw_pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 40)
        .map(|at| {
            let s = 0.5 * (2.0 * (at % PERIOD) as f32 / PERIOD as f32 - 1.0);
            [s, s]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// A one-track song of these rows, looped.
fn song(case: &Case, rows: &[String]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.millibpm = case.bpm * 1000;
    match case.source {
        Source::Synth(voice) => song.tracks[0].machine = Machine::Plaits(voice),
        Source::SampledSaw => {
            song.sample_on(0, "s");
        }
    }
    let track = &mut song.tracks[0];
    if let Some(adsr) = case.adsr {
        track.mods[0].adsr = adsr;
    }
    track.mods[0].bypassed = case.bypassed;
    track.gate_length = case.gate.or(track.gate_length);
    if case.drift {
        track.set_machine_value(3, 0x01);
    }
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The left channel of a song, for so many steps.
fn render(case: &Case, rows: &[String], steps: usize) -> Vec<f32> {
    let song = song(case, rows);
    let pool = match case.source {
        Source::SampledSaw => saw_pool(),
        Source::Synth(_) => Arc::new(SamplePool::default()),
    };
    let version = Arc::new(Published::new(Arc::new(song), pool));
    let frames = (case.step() * steps as f64).ceil() as usize;
    render_frames(version, RATE, frames)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// A note's name, Ableton's way, `C-3` being MIDI 60.
fn name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
    ];
    format!("{}{}", NAMES[usize::from(midi % 12)], midi / 12 - 2)
}

/// Random notes over two octaves from `C-2`, the same each run.
fn random_notes() -> Vec<String> {
    let mut seed = 0x1234_5678_u32;
    (0..ROWS)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            name(48 + (seed >> 16) as u8 % 25)
        })
        .collect()
}

/// Rows playing these notes, each a new note, or each tied into the next.
fn rows(notes: &[String], tied: bool) -> Vec<String> {
    let fx = if tied { "HLD1" } else { "---" };
    notes
        .iter()
        .map(|note| format!("{note} 7F {fx} --- ---"))
        .collect()
}

/// What repeats at the step rate, in each window of a step, averaged over every step but the
/// first: the level, the top of the spectrum, and the mean, an offset from zero.
struct Folded {
    level: Vec<f64>,
    top: Vec<f64>,
    offset: Vec<f64>,
}

fn fold(out: &[f32], step: f64) -> Folded {
    let windows = (step / WINDOW as f64) as usize;
    let mut power = vec![0.0; windows];
    let mut top = vec![0.0; windows];
    let mut sum = vec![0.0; windows];
    let mut count = vec![0usize; windows];
    for at in (step.ceil() as usize).max(2)..out.len() {
        let into = at as f64 % step;
        let window = ((into / WINDOW as f64) as usize).min(windows - 1);
        let s = f64::from(out[at]);
        // The second difference: a band-limited wave's is small but at its edges, a click's
        // large.
        let d2 = s - 2.0 * f64::from(out[at - 1]) + f64::from(out[at - 2]);
        power[window] += s * s;
        top[window] += d2 * d2;
        sum[window] += s;
        count[window] += 1;
    }
    let per = |sums: &[f64], f: fn(f64) -> f64| -> Vec<f64> {
        sums.iter()
            .zip(&count)
            .map(|(sum, n)| f(sum / (*n).max(1) as f64))
            .collect()
    };
    Folded {
        level: per(&power, f64::sqrt),
        top: per(&top, f64::sqrt),
        offset: per(&sum, |mean| mean),
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2].max(1e-12)
}

fn decibels(share: f64) -> f64 {
    20.0 * share.max(1e-9).log10()
}

/// What a note change does against the same change tied: how far the level dips in the first
/// 3 ms of a step and for how long it's 3 dB down, how far the top of the spectrum rises there,
/// and how far the offset from zero moves in the first 40 ms, as a share of the level.
struct Change {
    dip_db: f64,
    dip_ms: f64,
    top_db: f64,
    offset: f64,
    /// The level against the tied one, each 0.5 ms of the first 3 ms, for the eye.
    levels: String,
    /// The offset against the tied one, every 4 ms of the first 40, in hundredths of the level.
    offsets: String,
}

fn change(new: &Folded, tied: &Folded) -> Change {
    let ms = |ms: f64| (ms * f64::from(RATE) / 1000.0 / WINDOW as f64).ceil() as usize;
    let near = ms(3.0);
    let level: Vec<f64> = (0..near)
        .map(|w| decibels(new.level[w] / tied.level[w]))
        .collect();
    let top = (0..near)
        .map(|w| decibels(new.top[w] / tied.top[w]))
        .fold(f64::MIN, f64::max);
    let middle = median(&tied.level);
    let moved: Vec<f64> = (0..ms(40.0))
        .map(|w| (new.offset[w] - tied.offset[w]) / middle)
        .collect();
    Change {
        dip_db: level.iter().copied().fold(f64::MAX, f64::min),
        dip_ms: level.iter().filter(|db| **db < -3.0).count() as f64 * WINDOW as f64 * 1000.0
            / f64::from(RATE),
        top_db: top,
        offset: moved.iter().fold(0.0_f64, |most, o| most.max(o.abs())),
        levels: level
            .iter()
            .map(|db| format!("{db:+.0}"))
            .collect::<Vec<_>>()
            .join(" "),
        offsets: moved
            .chunks(8)
            .map(|part| {
                format!(
                    "{:+.0}",
                    100.0 * part.iter().sum::<f64>() / part.len() as f64
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn measure(name: &str, case: Case, notes: &[String]) -> Change {
    let steps = notes.len() * LOOPS;
    let new = fold(&render(&case, &rows(notes, false), steps), case.step());
    let tied = fold(&render(&case, &rows(notes, true), steps), case.step());
    let change = change(&new, &tied);
    println!(
        "{name:<30} dip {:>6.1} dB, {:>3.1} ms 3 dB down, top {:>+5.1} dB, offset {:.2} | \
         level, 0.5 ms each: {} | offset %, 4 ms each: {}",
        change.dip_db, change.dip_ms, change.top_db, change.offset, change.levels, change.offsets
    );
    change
}

/// The note shape with its times and sustain changed.
fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Adsr {
    Adsr {
        attack_ms,
        decay_ms,
        sustain,
        release_ms,
        ..Adsr::note_shape()
    }
}

/// Whether a change sounds as clean as a tie, near enough.
fn clean(change: &Change) -> bool {
    change.dip_db > -1.5 && change.top_db < 3.0 && change.offset < 0.05
}

#[test]
#[ignore = "diagnosis: a note on every 16th clicks at each change"]
fn random_16ths_change_notes_as_cleanly_as_a_tie() {
    let csaw = Source::Synth(SynthVoice::CSaw);
    let va = Case::plain(Source::Synth(SynthVoice::VirtualAnalog));
    let notes = random_notes();
    let cases = [
        ("CSaw, nothing tweaked", Case::plain(csaw)),
        (
            "CSaw at 140 BPM",
            Case {
                bpm: 140,
                ..Case::plain(csaw)
            },
        ),
        (
            "CSaw, note shape switched out",
            Case {
                bypassed: true,
                ..Case::plain(csaw)
            },
        ),
        (
            "CSaw, A 1 D 0 S 100 R 0",
            Case {
                adsr: Some(adsr(1, 0, 100, 0)),
                ..Case::plain(csaw)
            },
        ),
        (
            "CSaw, A 5 ms",
            Case {
                adsr: Some(adsr(5, 0, 100, 50)),
                ..Case::plain(csaw)
            },
        ),
        (
            "CSaw, no reset",
            Case {
                adsr: Some(Adsr {
                    reset: false,
                    ..Adsr::note_shape()
                }),
                ..Case::plain(csaw)
            },
        ),
        (
            "Saw Sq, nothing tweaked",
            Case::plain(Source::Synth(SynthVoice::SawSquare)),
        ),
        (
            "VA, nothing tweaked",
            Case::plain(Source::Synth(SynthVoice::VirtualAnalog)),
        ),
        (
            "Sampled saw, nothing tweaked",
            Case::plain(Source::SampledSaw),
        ),
        ("VA drifting, nothing tweaked", Case { drift: true, ..va }),
        (
            "VA drifting, A 1 D 0 S 100 R 0",
            Case {
                drift: true,
                adsr: Some(adsr(1, 0, 100, 0)),
                ..va
            },
        ),
        (
            "VA drifting, A 0",
            Case {
                drift: true,
                adsr: Some(adsr(0, 0, 100, 50)),
                ..va
            },
        ),
        (
            "VA drifting, A 5 ms",
            Case {
                drift: true,
                adsr: Some(adsr(5, 0, 100, 50)),
                ..va
            },
        ),
        (
            "VA drifting, no reset",
            Case {
                drift: true,
                adsr: Some(Adsr {
                    reset: false,
                    ..Adsr::note_shape()
                }),
                ..va
            },
        ),
    ];
    let mut clicks = Vec::new();
    for (name, case) in cases {
        if !clean(&measure(name, case, &notes)) {
            clicks.push(name);
        }
    }
    // The same note again: the sampled saw's step is whole cycles, so each note starts where the
    // tied one is, and the two line up frame for frame.
    let same = vec!["C-3".to_string(); 16];
    let name = "Sampled saw, the same note";
    if !clean(&measure(name, Case::plain(Source::SampledSaw), &same)) {
        clicks.push(name);
    }
    assert!(clicks.is_empty(), "these click at each note: {clicks:?}");
}

/// The most a sound falls to an exact silence in one frame, against its peak.
fn drop_to_silence(part: &[f32]) -> f32 {
    let peak = part.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
    let drop = part
        .windows(2)
        .filter(|pair| pair[1] == 0.0)
        .map(|pair| pair[0].abs())
        .fold(0.0_f32, f32::max);
    drop / peak.max(1e-9)
}

#[test]
fn a_blip_with_no_decay_falls_to_silence_rather_than_drops() {
    // A 1 ms attack with a decay of 0 to a sustain of 0: a blip, as it's set, but one whose decay
    // takes the shortest fall, a millisecond, rather than stepping to nothing in a frame.
    let case = Case {
        adsr: Some(adsr(1, 0, 0, 0)),
        ..Case::plain(Source::Synth(SynthVoice::CSaw))
    };
    let out = render(&case, &rows(&random_notes(), false), 16);
    let step = case.step() as usize;
    let mut worst = 0.0_f32;
    for (index, note) in out.chunks(step).enumerate() {
        let peak = note.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
        let heard = note
            .iter()
            .rposition(|s| s.abs() > peak * 0.01)
            .unwrap_or(0);
        let drop = drop_to_silence(note);
        if index < 4 {
            println!(
                "step {index}: peak {peak:.3}, heard for {:.2} ms, then a drop of {drop:.3} of \
                 its peak to silence in one frame",
                heard as f32 * 1000.0 / RATE as f32,
            );
        }
        // Heard for its attack and its fall, about 2 ms, and no longer.
        assert!((80..=110).contains(&heard), "step {index} is heard for {heard} frames");
        worst = worst.max(drop);
    }
    assert!(worst < 0.02, "a blip drops to silence from {worst:.3} of its peak");
}

#[test]
fn a_gate_closing_with_no_release_fades_rather_than_drops() {
    let notes = random_notes();
    let mut drops = Vec::new();
    for (name, case) in [
        (
            "CSaw, gate 50%",
            Case {
                gate: Some(0x80),
                ..Case::plain(Source::Synth(SynthVoice::CSaw))
            },
        ),
        (
            "CSaw, gate 50%, R 0",
            Case {
                gate: Some(0x80),
                adsr: Some(adsr(1, 0, 100, 0)),
                ..Case::plain(Source::Synth(SynthVoice::CSaw))
            },
        ),
        (
            "VA, gate 50%, R 0",
            Case {
                gate: Some(0x80),
                adsr: Some(adsr(1, 0, 100, 0)),
                ..Case::plain(Source::Synth(SynthVoice::VirtualAnalog))
            },
        ),
    ] {
        let out = render(&case, &rows(&notes, false), ROWS);
        let step = case.step();
        // Each frame's fall from the last, against the note's level, at each gate's close.
        let mut worst = 0.0_f32;
        for row in 1..ROWS {
            let start = (row as f64 * step) as usize;
            let close = start + (step * f64::from(0x80_u8) / 255.0) as usize;
            let level = out[start + 480..close - 48]
                .iter()
                .fold(0.0_f32, |most, s| most.max(s.abs()));
            // A release of 0 takes the shortest fall, a millisecond, so it's over within 2.
            let jump = out[close - 48..close + 96]
                .windows(2)
                .filter(|pair| pair[1] == 0.0)
                .map(|pair| pair[0].abs())
                .fold(0.0_f32, f32::max);
            worst = worst.max(jump / level.max(1e-9));
        }
        println!("{name:<24} drops to silence from {worst:.2} of its peak in one frame");
        if worst > 0.05 {
            drops.push(name);
        }
    }
    assert!(
        drops.is_empty(),
        "these drop to silence in a frame: {drops:?}"
    );
}

/// One note held for a second, alone: its level in 20 ms windows, and its offset from zero over
/// whole cycles, which a voice's own DC blocker takes out over its first tens of ms.
fn held(voice: SynthVoice, note: u8) -> (f64, Vec<f64>) {
    let case = Case::plain(Source::Synth(voice));
    let mut rows = vec![format!("{} 7F --- --- ---", name(note))];
    rows.extend((1..16).map(|_| "--- -- --- --- ---".to_string()));
    let out = render(&case, &rows, 8);
    let hertz = 261.625_565 * 2_f64.powf((f64::from(note) - 60.0) / 12.0);
    let period = f64::from(RATE) / hertz;
    // Each window is as near to whole cycles as frames allow, and at least 2 ms.
    let cycles = (96.0 / period).ceil();
    let window = (period * cycles).round() as usize;
    let mean = |part: &[f32]| part.iter().map(|s| f64::from(*s)).sum::<f64>() / part.len() as f64;
    let rms = |part: &[f32]| {
        (part.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / part.len() as f64).sqrt()
    };
    let level = rms(&out[RATE as usize / 2..RATE as usize]);
    let offsets = out[..RATE as usize / 10]
        .chunks(window)
        .map(|part| mean(part) / level)
        .collect();
    let windows: Vec<f64> = out[RATE as usize / 10..RATE as usize]
        .chunks(RATE as usize / 50)
        .map(rms)
        .collect();
    let most = windows.iter().copied().fold(0.0, f64::max);
    let least = windows.iter().copied().fold(f64::MAX, f64::min);
    (decibels(most / least), offsets)
}

#[test]
#[ignore = "diagnosis: what a held note does on its own"]
fn a_held_note_keeps_its_level_and_starts_with_no_offset() {
    let mut offsets = Vec::new();
    for voice in [
        SynthVoice::CSaw,
        SynthVoice::SawSquare,
        SynthVoice::Morph,
        SynthVoice::VirtualAnalog,
    ] {
        for note in [48, 60, 72, 84] {
            let (swing, offset) = held(voice, note);
            // The first window has the attack in it, so the one after is what's left over.
            let first = offset.get(1).copied().unwrap_or(0.0);
            println!(
                "{:<8} {}: level swings {swing:.2} dB over a second | offset in the first ms, as \
                 a share of its level, each ~2 ms: {}",
                format!("{voice:?}"),
                name(note),
                offset
                    .iter()
                    .take(12)
                    .map(|o| format!("{o:+.2}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if first.abs() > 0.05 {
                offsets.push((voice, note));
            }
        }
    }
    assert!(offsets.is_empty(), "these start off zero: {offsets:?}");
}

#[test]
#[ignore = "diagnosis: whether a held note has edges on the engine's or a voice's blocks"]
fn a_held_note_has_no_edges_at_block_boundaries() {
    let mut edged = Vec::new();
    for voice in [SynthVoice::CSaw, SynthVoice::VirtualAnalog] {
        let case = Case::plain(Source::Synth(voice));
        let mut rows = vec!["E-3 7F --- --- ---".to_string()];
        rows.extend((1..16).map(|_| "--- -- --- --- ---".to_string()));
        let out = render(&case, &rows, 16);
        // The top of the spectrum at each frame of a block, from 0.1 s in: the waveform's own
        // edges land anywhere in a block, so a block's own edge stands out.
        for block in [16_usize, 32] {
            let mut top = vec![0.0_f64; block];
            for at in RATE as usize / 10..out.len() {
                let d2 = f64::from(out[at]) - 2.0 * f64::from(out[at - 1]) + f64::from(out[at - 2]);
                top[at % block] += d2 * d2;
            }
            let ratio = top.iter().copied().fold(0.0, f64::max) / median(&top);
            println!(
                "{voice:?}: the top's largest frame of a {block}-frame block is {:+.1} dB over \
                 its median",
                10.0 * ratio.log10()
            );
            if ratio > 2.0 {
                edged.push((voice, block));
            }
        }
    }
    assert!(edged.is_empty(), "{edged:?}");
}
