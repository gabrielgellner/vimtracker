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
use vt_model::envelope::{Adsr, Curve};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phase, Phrase, Song, SynthVoice};

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
    /// A sampler playing white noise, which no two notes share a phase of, as different sounds
    /// don't.
    SampledNoise,
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
    /// The track's `Phase`.
    phase: Phase,
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
            phase: Phase::Free,
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

/// White noise as long, the same each run.
fn noise_pool() -> Arc<SamplePool> {
    let mut seed = 0x9E37_79B9_u32;
    let frames = (0..RATE as usize * 40)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let s = 0.5 * (seed as f32 / u32::MAX as f32 * 2.0 - 1.0);
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
        Source::SampledSaw | Source::SampledSine | Source::SampledNoise => {
            song.sample_on(0, "s");
        }
    }
    let track = &mut song.tracks[0];
    if let Some(adsr) = case.adsr {
        track.mods[0].adsr = adsr;
    }
    track.mods[0].bypassed = case.bypassed;
    track.gate_length = case.gate.or(track.gate_length);
    track.phase = case.phase;
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
        Source::SampledNoise => noise_pool(),
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
    /// The lowest and the highest level against the tied one, of each 0.5 ms of the first 6 ms,
    /// where two voices overlap: a dip, and a bump where they add up.
    low_db: f64,
    high_db: f64,
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
    let overlap: Vec<f64> = (0..ms(6.0))
        .map(|w| decibels(new.level[w] / tied.level[w]))
        .collect();
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
        low_db: overlap.iter().copied().fold(f64::MAX, f64::min),
        high_db: overlap.iter().copied().fold(f64::MIN, f64::max),
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
        "{name:<30} dip {:>6.1} dB, {:>3.1} ms 3 dB down, top {:>+5.1} dB, offset {:.2}, \
         {:+.1} to {:+.1} dB in 6 ms | level, 0.5 ms each: {} | offset %, 4 ms each: {}",
        change.dip_db,
        change.dip_ms,
        change.top_db,
        change.offset,
        change.low_db,
        change.high_db,
        change.levels,
        change.offsets
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

/// No reset on the note shape: the attack starts from where the level is. It's the plain note
/// shape's, a new track's (decision 373).
fn no_reset() -> Option<Adsr> {
    assert!(!Adsr::note_shape().reset);
    None
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
    // With the notes' shape switched out there's none to start again: the machine's own, or
    // none, shapes the notes, which run into one another as tied (decision 371).
    let name = "CSaw, note shape switched out";
    let switched = Case {
        bypassed: true,
        ..Case::plain(Source::Synth(SynthVoice::CSaw))
    };
    if !clean(&measure(name, switched, &notes), false) {
        clicks.push(name);
    }
    assert!(clicks.is_empty(), "these click at each note: {clicks:?}");
}

#[test]
fn random_16ths_with_phase_reset_dip_to_silence_without_a_click() {
    // `Phase` at `Reset` starts the oscillators again at the bottom of a dip to nothing, which it
    // takes whatever the note shape's `Reset` says: with it off, the change dips as with it on,
    // and no more of the top of the spectrum than a tie (decision 370).
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, voice) in [
        ("CSaw, Phase Reset, no reset", SynthVoice::CSaw),
        ("VA, Phase Reset, no reset", SynthVoice::VirtualAnalog),
        ("Saw Sq, Phase Reset, no reset", SynthVoice::SawSquare),
    ] {
        let case = Case {
            adsr: no_reset(),
            phase: Phase::Reset,
            ..Case::plain(Source::Synth(voice))
        };
        let change = measure(name, case, &notes);
        if change.dip_db > -15.0 || change.back_db < -3.0 || change.top_db > 3.0 {
            clicks.push(name);
        }
    }
    assert!(clicks.is_empty(), "these click or don't dip: {clicks:?}");
}

#[test]
fn random_16ths_carried_with_reset_dip_to_silence_without_a_click() {
    // With `Reset`, as a note's shape starts, a carried note's level falls to nothing over the
    // short fade on the same voice, and then attacks: a gap at each change, on purpose, so each
    // note is struck, but no click, as the oscillator runs on through it.
    let reset = Some(Adsr {
        reset: true,
        ..Adsr::note_shape()
    });
    let csaw = Case {
        adsr: reset,
        ..Case::plain(Source::Synth(SynthVoice::CSaw))
    };
    let va = Case {
        adsr: reset,
        ..Case::plain(Source::Synth(SynthVoice::VirtualAnalog))
    };
    let notes = random_notes();
    let mut clicks = Vec::new();
    for (name, case) in [
        ("CSaw, Reset on", csaw),
        ("CSaw at 140 BPM", Case { bpm: 140, ..csaw }),
        (
            "CSaw, A 1 D 0 S 100 R 0",
            Case {
                adsr: Some(Adsr {
                    reset: true,
                    ..adsr(1, 0, 100, 0)
                }),
                ..csaw
            },
        ),
        (
            "Saw Sq, Reset on",
            Case {
                adsr: reset,
                ..Case::plain(Source::Synth(SynthVoice::SawSquare))
            },
        ),
        ("VA, Reset on", va),
        ("VA drifting, Reset on", Case { drift: true, ..va }),
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
fn a_new_voice_crossfades_at_equal_power() {
    // A sampler starts each note again, a voice of its own, rising on a quarter sine as the old
    // one falls on a quarter cosine, over 2.7 ms: their powers add to one, so two sounds that
    // share no phase, such as noise and different notes, hold their level (decision 368).
    let notes = random_notes();
    let same = vec!["C-3".to_string(); 16];
    let noise = measure(
        "(a) sampled noise, random notes",
        Case::plain(Source::SampledNoise),
        &notes,
    );
    // The same steady tone struck again in phase is where two voices add most: up to 3 dB, for
    // as long as the overlap, the cost of holding the level of different sounds.
    let again = measure(
        "(b) sampled saw, the same note",
        Case::plain(Source::SampledSaw),
        &same,
    );
    // Each new note starts its saw at the bottom, where the tie is anywhere in its cycle, which
    // is most of what's left of its dip.
    let saw = measure(
        "Sampled saw, random notes",
        Case::plain(Source::SampledSaw),
        &notes,
    );
    assert!(
        noise.low_db > -1.25 && noise.high_db < 1.25,
        "different sounds go from {:+.1} to {:+.1} dB",
        noise.low_db,
        noise.high_db
    );
    assert!(
        again.high_db < 3.5 && again.low_db > -1.0,
        "the same tone goes from {:+.1} to {:+.1} dB",
        again.low_db,
        again.high_db
    );
    assert!(
        saw.low_db > -2.5 && saw.top_db < 3.0,
        "random notes dip {:.1} dB, top {:+.1} dB",
        saw.low_db,
        saw.top_db
    );
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
        // A sampler's notes held would each rise over the last one's fade, so they're held for
        // half a step, and each starts after silence with its attack as set, as a blip does.
        let full = Case {
            adsr: Some(adsr(1, 0, 100, 0)),
            gate: matches!(source, Source::SampledSine).then_some(0x80),
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

/// One note held for a second, after silence, as the first note after a rest is: its level in
/// 20 ms windows, and its offset from zero over whole cycles, which a DC blocker starting from
/// rest would take out over the first tens of ms, a thump.
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
        .chunks_exact(window)
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
fn the_first_note_after_a_rest_starts_with_no_offset() {
    // Braids' DC blocker runs on from note to note; a new voice's is settled before its first
    // frame, so a note after a rest starts centred rather than thumping as the blocker catches up.
    let mut offsets = Vec::new();
    for voice in [
        SynthVoice::CSaw,
        SynthVoice::SawSquare,
        SynthVoice::Morph,
        SynthVoice::HardSync,
        SynthVoice::FeedbackFm,
        SynthVoice::Vosim,
        SynthVoice::Fof,
        SynthVoice::WaveMap,
        SynthVoice::WaveLine,
        SynthVoice::VirtualAnalog,
    ] {
        for note in [36, 48, 60, 72, 84] {
            let (swing, offset) = held(voice, note);
            // The first window has the attack in it, so the ones after are what's left over.
            let first = offset
                .iter()
                .skip(1)
                .take(6)
                .fold(0.0_f64, |most, o| most.max(o.abs()));
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
            if first > 0.05 {
                offsets.push((voice, note));
            }
        }
    }
    assert!(offsets.is_empty(), "these start off zero: {offsets:?}");
}

#[test]
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

// #942: the dip `Reset` makes on a carried note, and plucks with a sustain of 0. These print what
// they measure, for the issue, and assert nothing. With `VT_DIP=frames` and `VT_DIP_CURVE=cos` on
// the branch `measure/942-retrig-dip`, the dip is another candidate's.

/// What a change's notch is against the same change tied, over its first 10 ms.
struct Notch {
    depth_db: f64,
    /// For how long it's 3, 6 and 20 dB down, in ms.
    down_ms: [f64; 3],
    /// What it takes out, as so many ms of silence: the power lost against the tie.
    lost_ms: f64,
    /// When it's back within 1 dB of the tie after its deepest, in ms from the step.
    back_ms: f64,
    top_db: f64,
}

fn notch(new: &Folded, tied: &Folded) -> Notch {
    let window_ms = WINDOW as f64 * 1000.0 / f64::from(RATE);
    let windows = (10.0 / window_ms) as usize;
    let level: Vec<f64> = (0..windows)
        .map(|w| decibels(new.level[w] / tied.level[w]))
        .collect();
    let deepest = (0..windows)
        .min_by(|a, b| level[*a].total_cmp(&level[*b]))
        .unwrap_or(0);
    let down = |db: f64| level.iter().filter(|l| **l < -db).count() as f64 * window_ms;
    let lost = (0..windows)
        .map(|w| 1.0 - (new.level[w] / tied.level[w]).powi(2))
        .sum::<f64>()
        * window_ms;
    let back = (deepest..windows)
        .find(|w| level[*w] > -1.0)
        .map_or(10.0, |w| w as f64 * window_ms);
    Notch {
        depth_db: level[deepest],
        down_ms: [down(3.0), down(6.0), down(20.0)],
        lost_ms: lost,
        back_ms: back,
        top_db: (0..windows)
            .map(|w| decibels(new.top[w] / tied.top[w]))
            .fold(f64::MIN, f64::max),
    }
}

#[test]
#[ignore = "measures #942's dip, printing it"]
fn retrig_notch_table() {
    let notes = random_notes();
    let dip = std::env::var("VT_DIP").unwrap_or_else(|_| "64".into());
    let curve = std::env::var("VT_DIP_CURVE").unwrap_or_else(|_| "lin".into());
    println!("the dip: {dip} frames, {curve}");
    let reset = Some(Adsr {
        reset: true,
        ..Adsr::note_shape()
    });
    for (name, source) in [
        ("CSaw", Source::Synth(SynthVoice::CSaw)),
        ("VA", Source::Synth(SynthVoice::VirtualAnalog)),
        ("Saw Sq", Source::Synth(SynthVoice::SawSquare)),
        ("FM", Source::Synth(SynthVoice::Fm)),
        ("Sampled saw", Source::SampledSaw),
    ] {
        for (shape, adsr) in [
            ("Reset on", reset),
            (
                "Reset on, A 0",
                Some(Adsr {
                    reset: true,
                    ..adsr(0, 0, 100, 50)
                }),
            ),
            (
                "Reset on, Lin",
                Some(Adsr {
                    curve: Curve::Lin,
                    reset: true,
                    ..Adsr::note_shape()
                }),
            ),
            (
                "Reset on, Analog",
                Some(Adsr {
                    curve: Curve::Analog,
                    reset: true,
                    ..Adsr::note_shape()
                }),
            ),
            ("Reset off", no_reset()),
        ] {
            let case = Case {
                adsr,
                ..Case::plain(source)
            };
            let steps = notes.len() * LOOPS;
            let new = fold(&render(&case, &rows(&notes, false), steps), case.step());
            let tied = fold(&render(&case, &rows(&notes, true), steps), case.step());
            let n = notch(&new, &tied);
            println!(
                "{name:<12} {shape:<16} | depth {:>6.1} dB | 3/6/20 dB down {:.1}/{:.1}/{:.1} ms \
                 | lost {:.2} ms | back {:.1} ms | top {:+.1} dB",
                n.depth_db,
                n.down_ms[0],
                n.down_ms[1],
                n.down_ms[2],
                n.lost_ms,
                n.back_ms,
                n.top_db
            );
        }
    }
}

/// The top of the spectrum's energy over these frames: the second difference squared.
fn top_over(out: &[f32], from: usize, to: usize) -> f64 {
    (from.max(2)..to)
        .map(|i| {
            let d2 = f64::from(out[i]) - 2.0 * f64::from(out[i - 1]) + f64::from(out[i - 2]);
            d2 * d2
        })
        .sum()
}

/// What a pluck with a sustain of 0 does at its start and its end, against the same notes on an
/// oscillator running at full and shaped by the envelope as it's set: the onset's top of the
/// spectrum, in its first ms and the two after, against the running wave's own at full, and the
/// same of the model; the top around where the decay lands; and what's left of the note's
/// offset from zero.
struct Pluck {
    onset_db: f64,
    model_db: f64,
    after_db: f64,
    after_model_db: f64,
    end_db: f64,
    end_model_db: f64,
    drop: f32,
    /// How many frames into its step a note is first heard, on average.
    starts: f64,
    /// Each note's offset from zero over what's heard of it, as a share of its level, the root
    /// mean square over the notes, and the model's.
    offset: f64,
    model_offset: f64,
}

fn pluck(case: Case, adsr: Adsr, notes: &[String]) -> Pluck {
    let steps = notes.len() * LOOPS;
    let out = render(&case, &rows(notes, false), steps);
    let running = Case {
        adsr: Some(self::adsr(0, 0, 100, 0)),
        ..case
    };
    let tied = render(&running, &rows(notes, true), steps);
    let step = case.step() as usize;
    let attack = u64::from(adsr.attack_ms) * u64::from(RATE) / 1000;
    let decay = u64::from(adsr.decay_ms) * u64::from(RATE) / 1000;
    let length = (attack + decay) as usize;
    let mut model = vec![0.0_f32; out.len()];
    for k in 0..steps {
        let s = k * step;
        for i in 0..step.min(out.len() - s) {
            let level = if (i as u64) < attack + decay {
                adsr.held(i as u64, 0.0, RATE)
            } else {
                0.0
            };
            model[s + i] = tied[s + i] * level;
        }
    }
    // The running wave's top at full, a frame.
    let full = top_over(&tied, step, out.len()) / (out.len() - step) as f64;
    let ms = RATE as usize / 1000;
    let (mut onset, mut onset_model, mut after, mut after_model) = (0.0, 0.0, 0.0, 0.0);
    let (mut end, mut end_model, mut drop, mut starts) = (0.0, 0.0, 0.0_f32, 0.0);
    let (mut offset, mut model_offset) = (0.0, 0.0);
    let heard = steps - 1;
    for k in 1..steps {
        let s = k * step;
        onset += top_over(&out, s, s + ms);
        onset_model += top_over(&model, s, s + ms);
        after += top_over(&out, s + ms, s + 3 * ms);
        after_model += top_over(&model, s + ms, s + 3 * ms);
        let e = s + length;
        if e + ms < (k + 1) * step {
            end += top_over(&out, e - ms / 2, e + ms);
            end_model += top_over(&model, e - ms / 2, e + ms);
        }
        let note = &out[s..(s + step).min(out.len())];
        drop = drop.max(drop_to_silence(note));
        starts += note.iter().position(|v| *v != 0.0).unwrap_or(0) as f64;
        let share = |part: &[f32]| {
            let part = &part[..length.min(part.len()).max(1)];
            let mean = part.iter().map(|v| f64::from(*v)).sum::<f64>() / part.len() as f64;
            let rms = (part.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / part.len() as f64)
                .sqrt();
            (mean / rms.max(1e-12)).powi(2)
        };
        offset += share(note);
        model_offset += share(&model[s..(s + step).min(out.len())]);
    }
    let per = |energy: f64, frames: usize| {
        10.0 * (energy / (heard * frames) as f64 / full).max(1e-30).log10()
    };
    if std::env::var_os("VT_PROFILE").is_some() {
        // Each 0.25 ms of the first 4: the level and the top against the model's, in dB.
        let power = |out: &[f32], from: usize, to: usize| {
            (from..to).map(|i| f64::from(out[i]).powi(2)).sum::<f64>()
        };
        let mut line = String::new();
        for w in 0..16 {
            let (mut p, mut pm, mut t, mut tm) = (0.0, 0.0, 0.0, 0.0);
            for k in 1..steps {
                let at = k * step + w * 12;
                p += power(&out, at, at + 12);
                pm += power(&model, at, at + 12);
                t += top_over(&out, at, at + 12);
                tm += top_over(&model, at, at + 12);
            }
            line += &format!(
                " {:+.1}/{:+.1}",
                10.0 * (p / pm).log10(),
                10.0 * (t / tm).log10()
            );
        }
        println!("  level/top against the model, each 0.25 ms:{line}");
    }
    Pluck {
        onset_db: per(onset, ms),
        model_db: per(onset_model, ms),
        after_db: per(after, 2 * ms),
        after_model_db: per(after_model, 2 * ms),
        end_db: per(end, ms * 3 / 2),
        end_model_db: per(end_model, ms * 3 / 2),
        drop,
        starts: starts / heard as f64,
        offset: (offset / heard as f64).sqrt(),
        model_offset: (model_offset / heard as f64).sqrt(),
    }
}

#[test]
#[ignore = "measures #942's plucks' onsets, printing them"]
fn pluck_onset_table() {
    // How much brighter a note is than its wave running at full, for its top of the spectrum
    // against its level: what its envelope's own corners add. On a sine, whose own top is next
    // to nothing, it's the envelope's alone.
    let notes = random_notes();
    let steps = notes.len() * LOOPS;
    for (name, source) in [
        ("CSaw", Source::Synth(SynthVoice::CSaw)),
        ("VA", Source::Synth(SynthVoice::VirtualAnalog)),
        ("Sampled sine", Source::SampledSine),
    ] {
        let running = Case {
            adsr: Some(adsr(0, 0, 100, 0)),
            ..Case::plain(source)
        };
        let tied = render(&running, &rows(&notes, true), steps);
        let step = running.step() as usize;
        let power = |out: &[f32], from: usize, to: usize| {
            (from..to).map(|i| f64::from(out[i]).powi(2)).sum::<f64>()
        };
        let own = top_over(&tied, step, tied.len()) / power(&tied, step, tied.len());
        let mut shapes: Vec<(String, Adsr)> = Vec::new();
        for (attack, curve) in [
            (1, Curve::Exp),
            (1, Curve::Lin),
            (1, Curve::Analog),
            (2, Curve::Exp),
            (3, Curve::Exp),
            (5, Curve::Exp),
        ] {
            for (decay, sustain) in [(5, 0), (20, 0), (80, 0), (0, 100)] {
                shapes.push((
                    format!("A {attack} {} D {decay} S {sustain}", curve.name()),
                    Adsr {
                        curve,
                        ..adsr(attack, decay, sustain, 50)
                    },
                ));
            }
        }
        for (shape, adsr) in shapes {
            let case = Case {
                adsr: Some(adsr),
                ..Case::plain(source)
            };
            let out = render(&case, &rows(&notes, false), steps);
            let ms = RATE as usize / 1000;
            let bright = |from: usize, to: usize| {
                let (mut top, mut level) = (0.0, 0.0);
                for k in 1..steps {
                    let s = k * step;
                    top += top_over(&out, s + from, s + to);
                    level += power(&out, s + from, s + to);
                }
                10.0 * (top / level / own).log10()
            };
            println!(
                "{name:<13} {shape:<22} | brighter than its wave: first 3 ms {:+5.1} dB, first 10 \
                 ms {:+5.1} dB",
                bright(0, 3 * ms),
                bright(0, 10 * ms),
            );
        }
    }
}

#[test]
#[ignore = "measures #942's plucks, printing them"]
fn sustain_zero_pluck_table() {
    let notes = random_notes();
    println!(
        "onset: the top of the spectrum in a note's first ms, and in the 2 ms after, against the \
         running wave's own at full, as it is and as modelled (the same wave, running, times the \
         envelope as set). end: around where the decay lands. A voice whose envelope has ended \
         goes idle, and the next note starts a fresh one."
    );
    for (name, source) in [
        ("CSaw", Source::Synth(SynthVoice::CSaw)),
        ("VA", Source::Synth(SynthVoice::VirtualAnalog)),
        ("Sampled saw", Source::SampledSaw),
    ] {
        let mut shapes: Vec<(String, Adsr)> = Vec::new();
        for decay in [5, 20, 80, 120, 130] {
            for reset in [true, false] {
                shapes.push((
                    format!(
                        "A 1 D {decay} S 0, reset {}",
                        if reset { "on" } else { "off" }
                    ),
                    Adsr {
                        reset,
                        ..adsr(1, decay, 0, 50)
                    },
                ));
            }
        }
        for attack in [0, 2, 5] {
            shapes.push((
                format!("A {attack} D 20 S 0, reset on"),
                Adsr {
                    reset: true,
                    ..adsr(attack, 20, 0, 50)
                },
            ));
        }
        for (shape, adsr) in shapes {
            let case = Case {
                adsr: Some(adsr),
                ..Case::plain(source)
            };
            let p = pluck(case, adsr, &notes);
            let fresh = if adsr.attack_ms + adsr.decay_ms < 125 {
                "fresh"
            } else {
                "carried"
            };
            println!(
                "{name:<12} {shape:<26} {fresh:<7} | onset {:+6.1} dB (model {:+6.1}) | 1-3 ms \
                 {:+6.1} (model {:+6.1}) | end {:+6.1} (model {:+6.1}) | drop {:.3} | starts \
                 {:.1} frames in | offset {:.3} (model {:.3})",
                p.onset_db,
                p.model_db,
                p.after_db,
                p.after_model_db,
                p.end_db,
                p.end_model_db,
                p.drop,
                p.starts,
                p.offset,
                p.model_offset
            );
        }
    }
}
