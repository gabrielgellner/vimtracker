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
    /// A sampler playing a sine as long, whose own top of the spectrum is next to nothing, so a
    /// step in its level stands out.
    SampledSine,
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

fn sample_pool(wave: fn(f32) -> f32) -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 40)
        .map(|at| {
            let s = 0.5 * wave((at % PERIOD) as f32 / PERIOD as f32);
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
        Source::SampledSaw | Source::SampledSine => {
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
        Source::SampledSaw => sample_pool(|at| 2.0 * at - 1.0),
        // Half a frame in, so no frame is exactly nothing, which a drop to silence would be taken for.
        Source::SampledSine => sample_pool(|at| (std::f32::consts::TAU * (at + 0.0025)).sin()),
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
    /// The level against the tied one in the last 0.5 ms of the 3, where a note is back.
    back_db: f64,
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
        back_db: level.last().copied().unwrap_or(0.0),
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

/// Whether a change sounds as clean as a tie, near enough: no dip, no click, and, unless each
/// note drifts a level and a cutoff of its own, which a tie doesn't, no offset either.
fn clean(change: &Change, drifting: bool) -> bool {
    change.dip_db > -1.5 && change.top_db < 3.0 && (drifting || change.offset < 0.05)
}

/// No reset on the note shape: the attack starts from where the level is.
fn no_reset() -> Option<Adsr> {
    Some(Adsr {
        reset: false,
        ..Adsr::note_shape()
    })
}

#[test]
fn random_16ths_carried_without_reset_change_as_cleanly_as_a_tie() {
    // A synth that holds carries its voice on into the next note: its oscillator runs on, so
    // without `Reset` a change is a tie's, but for the attack starting again from where it is.
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, voice, drift) in [
        ("CSaw, no reset", SynthVoice::CSaw, false),
        ("Saw Sq, no reset", SynthVoice::SawSquare, false),
        ("Morph, no reset", SynthVoice::Morph, false),
        ("VA, no reset", SynthVoice::VirtualAnalog, false),
        ("VA drifting, no reset", SynthVoice::VirtualAnalog, true),
        ("FM, no reset", SynthVoice::Fm, false),
        ("4-op, no reset", SynthVoice::FourOp, false),
    ] {
        let case = Case {
            adsr: no_reset(),
            drift,
            ..Case::plain(Source::Synth(voice))
        };
        if !clean(&measure(name, case, &notes), drift) {
            clicks.push(name);
        }
    }
    assert!(clicks.is_empty(), "these click at each note: {clicks:?}");
}

#[test]
fn random_16ths_carried_with_reset_dip_to_silence_without_a_click() {
    // With `Reset`, as a note's shape starts, a carried note's level falls to nothing over the
    // short fade on the same voice, and then attacks: a gap at each change, on purpose, so each
    // note is struck, but no click, as the oscillator runs on through it.
    let csaw = Source::Synth(SynthVoice::CSaw);
    let va = Case::plain(Source::Synth(SynthVoice::VirtualAnalog));
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, case) in [
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
            "Saw Sq, nothing tweaked",
            Case::plain(Source::Synth(SynthVoice::SawSquare)),
        ),
        ("VA, nothing tweaked", va),
        ("VA drifting, nothing tweaked", Case { drift: true, ..va }),
    ] {
        let change = measure(name, case, &notes);
        // Down to silence in the dip, back within 3 ms, and nothing at the top of the spectrum
        // the tie hasn't.
        if change.dip_db > -15.0 || change.back_db < -3.0 || change.top_db > 3.0 {
            clicks.push(name);
        }
    }
    // These are saws: on a near-sine, such as `FM` at its defaults, the top of the spectrum also
    // shows the corner where the note shape's own 1 ms attack ends, which every new note has.
    assert!(clicks.is_empty(), "these click or don't dip: {clicks:?}");
}

#[test]
#[ignore = "diagnosis: a new voice starting under the last one's fade dips at each change"]
fn random_16ths_on_a_new_voice_change_as_cleanly_as_a_tie() {
    // A sampler starts each note again, and a synth that doesn't carry starts a voice of its own,
    // under the old one's fade.
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, case) in [
        (
            "Sampled saw, nothing tweaked",
            Case::plain(Source::SampledSaw),
        ),
        (
            "Sampled saw, no reset",
            Case {
                adsr: no_reset(),
                ..Case::plain(Source::SampledSaw)
            },
        ),
    ] {
        if !clean(&measure(name, case, &notes), false) {
            clicks.push(name);
        }
    }
    // The same note again: the sampled saw's step is whole cycles, so each note starts where the
    // tied one is, and the two line up frame for frame.
    let same = vec!["C-3".to_string(); 16];
    let name = "Sampled saw, the same note";
    if !clean(
        &measure(name, Case::plain(Source::SampledSaw), &same),
        false,
    ) {
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

/// The top of the spectrum in the millisecond around a frame, from a quarter of it before, as
/// the sum of the second difference squared: a step's click is large there, a band-limited
/// wave's small.
fn top_around(out: &[f32], at: usize) -> f64 {
    (at - 12..at + 36)
        .map(|i| {
            let d2 = f64::from(out[i]) - 2.0 * f64::from(out[i - 1]) + f64::from(out[i - 2]);
            d2 * d2
        })
        .sum()
}

/// How much a level's steps at these frames add to the top of the spectrum around them, against
/// the same frames of the same notes held at full, in dB, summed over every step.
fn added_top(out: &[f32], held: &[f32], steps: &[usize]) -> f64 {
    let (stepped, full) = steps.iter().fold((0.0, 0.0), |(s, f), at| {
        (s + top_around(out, *at), f + top_around(held, *at))
    });
    10.0 * (stepped / f64::max(full, 1e-30)).log10()
}

/// The fade a cut note has always had, 64 frames straight down (1.3 ms): what a step through the
/// amp is held to, as it doesn't click.
const CUT_FADE: usize = 64;

/// The same notes held at full, faded out at each of these frames as a cut fades them.
fn cut_at(held: &[f32], steps: &[usize]) -> Vec<f32> {
    let mut out = held.to_vec();
    for at in steps {
        for (i, sample) in out[*at..*at + 48].iter_mut().enumerate() {
            *sample *= (1.0 - i as f32 / CUT_FADE as f32).max(0.0);
        }
    }
    out
}

/// What a step adds to the top at each of these frames, and what the cut's fade would.
fn step_against_cut(out: &[f32], held: &[f32], steps: &[usize]) -> (f64, f64) {
    (
        added_top(out, held, steps),
        added_top(&cut_at(held, steps), held, steps),
    )
}

#[test]
fn a_blip_with_no_decay_ends_without_a_click() {
    // A 1 ms attack with a decay of 0 to a sustain of 0: a blip, as it's set, whose end is a step
    // from full to nothing in its envelope. The amp's smoothing of a fall takes the click out
    // of it.
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, source) in [
        ("CSaw", Source::Synth(SynthVoice::CSaw)),
        ("Sampled sine", Source::SampledSine),
    ] {
        let case = Case {
            adsr: Some(adsr(1, 0, 0, 0)),
            ..Case::plain(source)
        };
        let out = render(&case, &rows(&notes, false), 16);
        let full = Case {
            adsr: Some(adsr(1, 0, 100, 0)),
            ..case
        };
        let held = render(&full, &rows(&notes, false), 16);
        let step = case.step();
        // Where each blip's attack ends and its envelope steps to nothing.
        let ends: Vec<usize> = (1..16)
            .map(|row| (row as f64 * step) as usize + RATE as usize / 1000)
            .collect();
        let (added, cut) = step_against_cut(&out, &held, &ends);
        let mut worst = 0.0_f32;
        let mut longest = 0;
        for note in out.chunks(step as usize) {
            let peak = note.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
            let heard = note
                .iter()
                .rposition(|s| s.abs() > peak * 0.01)
                .unwrap_or(0);
            longest = longest.max(heard);
            worst = worst.max(drop_to_silence(note));
        }
        println!(
            "{name:<13} a blip's end adds {added:+.1} dB to the top, a cut {cut:+.1}; it's heard \
             for {:.2} ms, and drops to silence from {worst:.3} of its peak in one frame",
            longest as f32 * 1000.0 / RATE as f32,
        );
        // Heard for its attack and the amp's short fall after it, and no longer.
        if added > cut || worst > 0.02 || longest > RATE as usize * 4 / 1000 {
            clicks.push(name);
        }
    }
    assert!(
        clicks.is_empty(),
        "these blips click as they end: {clicks:?}"
    );
}

#[test]
fn a_gate_closing_with_no_release_fades_rather_than_drops() {
    let notes = random_notes();
    let mut clicks = Vec::new();
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
        (
            "Sampled sine, gate 50%",
            Case {
                gate: Some(0x80),
                ..Case::plain(Source::SampledSine)
            },
        ),
        (
            "Sampled sine, gate 50%, R 0",
            Case {
                gate: Some(0x80),
                adsr: Some(adsr(1, 0, 100, 0)),
                ..Case::plain(Source::SampledSine)
            },
        ),
    ] {
        let out = render(&case, &rows(&notes, false), ROWS);
        let held = render(&Case { gate: None, ..case }, &rows(&notes, false), ROWS);
        let step = case.step();
        let closes: Vec<usize> = (1..ROWS)
            .map(|row| (row as f64 * step) as usize + (step * f64::from(0x80_u8) / 255.0) as usize)
            .collect();
        let (added, cut) = step_against_cut(&out, &held, &closes);
        // Each frame's fall to an exact silence, against the note's level, at each gate's close.
        let mut worst = 0.0_f32;
        for (row, close) in (1..ROWS).zip(&closes) {
            let start = (row as f64 * step) as usize;
            let level = out[start + 480..close - 48]
                .iter()
                .fold(0.0_f32, |most, s| most.max(s.abs()));
            let jump = out[close - 48..close + 96]
                .windows(2)
                .filter(|pair| pair[1] == 0.0)
                .map(|pair| pair[0].abs())
                .fold(0.0_f32, f32::max);
            worst = worst.max(jump / level.max(1e-9));
        }
        println!(
            "{name:<28} adds {added:+.1} dB to the top as it closes, a cut {cut:+.1}, and drops to \
             silence from {worst:.2} of its peak in one frame"
        );
        if worst > 0.05 || added > cut.max(0.0) {
            clicks.push(name);
        }
    }
    assert!(clicks.is_empty(), "these click as they close: {clicks:?}");
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
