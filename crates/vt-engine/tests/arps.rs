//! Arpeggios, after Renoise's `-Axy` and the M8's `ARP`: an `ARP xy` goes round its row's note,
//! the note and `x` up, and the note and `y` up, at the track's arp rate, in its mode and over its
//! octaves, for as long as the note lasts.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::speed::Speed;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// Frames in a tick, a sixth of a step.
const TICK: usize = STEP / 6;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-3`, which plays the samples at their own speed.
const C3: i32 = 60;
/// The sine's period at `C-3`, in frames: 375 Hz.
const SINE_PERIOD: f64 = 128.0;

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let ramp = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("ramp", SampleData { frames: ramp });
    let sine = (0..RATE as usize * 4)
        .map(|at| {
            let level = (at as f64 * std::f64::consts::TAU / SINE_PERIOD).sin() as f32 * 0.5;
            [level, level]
        })
        .collect();
    pool.insert("sine", SampleData { frames: sine });
    Arc::new(pool)
}

/// One track playing these rows on the ramp, with notes that start and stop at once, the scale
/// off, and these arpeggio settings.
fn song(rows: &[&str], arpeggio: Arpeggio) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.scale.on = false;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    track.arpeggio = arpeggio;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn with(rate: ArpRate, mode: ArpMode, octaves: u8) -> Arpeggio {
    Arpeggio {
        rate,
        mode,
        octaves,
        ..Arpeggio::default()
    }
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// The left channel, for this many frames.
fn render(song: &Song, frames: usize) -> Vec<f32> {
    render_frames(version(song), RATE, frames)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// The note sounding between two frames, by how fast the ramp climbs, or none if it doesn't.
fn pitch(out: &[f32], from: usize, to: usize) -> Option<i32> {
    let climb = (out[to] - out[from]) / (to - from) as f32;
    let own = 1.0 / RAMP_FRAMES as f32;
    (climb > own * 0.1).then(|| C3 + (12.0 * (climb / own).log2()).round() as i32)
}

/// The note heard through the middle of each of so many stretches this long, from a frame.
fn heard(out: &[f32], from: usize, each: usize, count: usize) -> Vec<Option<i32>> {
    (0..count)
        .map(|at| {
            let start = from + at * each;
            pitch(out, start + each / 5, start + each * 4 / 5)
        })
        .collect()
}

/// The notes a song plays, each this long, for so many of them from the top.
fn notes(song: &Song, each: usize, count: usize) -> Vec<Option<i32>> {
    let out = render(song, each * count + STEP);
    heard(&out, 0, each, count)
}

/// Notes by name, `C-3` up, for the notes heard to be read against.
fn named(names: &str) -> Vec<Option<i32>> {
    names
        .split_whitespace()
        .map(|name| {
            let semitones = match name {
                "." => return None,
                "B2" => -1,
                "C" => 0,
                "C#" => 1,
                "D" => 2,
                "D#" => 3,
                "E" => 4,
                "F" => 5,
                "F#" => 6,
                "G" => 7,
                "G#" => 8,
                "A" => 9,
                "A#" => 10,
                "B" => 11,
                "C4" => 12,
                "E4" => 16,
                "G4" => 19,
                "C5" => 24,
                other => panic!("no note {other}"),
            };
            Some(C3 + semitones)
        })
        .collect()
}

/// The note the sine plays between two frames, by how far apart it rises through zero, which its
/// level doesn't change.
fn sine_note(out: &[f32], from: usize, to: usize) -> i32 {
    let crossings: Vec<f64> = (from..to)
        .filter(|at| out[*at] < 0.0 && out[at + 1] >= 0.0)
        .map(|at| at as f64 + f64::from(-out[at] / (out[at + 1] - out[at])))
        .collect();
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    let period = (last - first) / (crossings.len() - 1) as f64;
    C3 + (12.0 * (SINE_PERIOD / period).log2()).round() as i32
}

/// Whether a note starts near a frame: the ramp starting again from nothing.
fn starts_near(out: &[f32], frame: usize) -> bool {
    out[frame + 100] < 0.002
}

#[test]
fn at_tick_it_goes_round_six_notes_a_step() {
    let song = song(&["C-3 -- arp:47", "--- --"], Arpeggio::default());
    assert_eq!(notes(&song, TICK, 12), named("C E G C E G  C E G C E G"));
}

#[test]
fn a_division_is_its_share_of_the_step() {
    let rows = ["C-3 -- SGL4 arp:47"];
    let rates = [
        (ArpRate::HundredTwentyEighth, STEP / 8),
        (ArpRate::SixtyFourth, STEP / 4),
        (ArpRate::ThirtySecondTriplet, STEP / 3),
        (ArpRate::ThirtySecond, STEP / 2),
        (ArpRate::SixteenthTriplet, STEP * 2 / 3),
        (ArpRate::Sixteenth, STEP),
    ];
    for (rate, each) in rates {
        let song = song(&rows, with(rate, ArpMode::Up, 1));
        assert_eq!(notes(&song, each, 4), named("C E G C"), "{rate:?}");
    }
    let eighths = song(
        &["C-3 -- SGL8 arp:47"],
        with(ArpRate::Eighth, ArpMode::Up, 1),
    );
    assert_eq!(notes(&eighths, 2 * STEP, 4), named("C E G C"));
}

#[test]
fn zero_crossings_hear_the_same_cycle() {
    let mut song = song(
        &["C-3 -- SGL2 arp:47"],
        with(ArpRate::ThirtySecond, ArpMode::Up, 1),
    );
    song.sample_on(0, "sine");
    let out = render(&song, 2 * STEP);
    let heard: Vec<i32> = (0..4)
        .map(|note| {
            let start = note * STEP / 2;
            sine_note(&out, start + 500, start + 2500)
        })
        .collect();
    assert_eq!(heard, [60, 64, 67, 60]);
}

#[test]
fn at_another_speed_each_note_is_as_long_against_the_tracks_step() {
    let mut eighths = song(&["C-3 -- arp:47"], Arpeggio::default());
    eighths.tracks[0].speed = Speed::Eighth;
    assert_eq!(notes(&eighths, 2 * TICK, 6), named("C E G C E G"));
    let mut triplets = song(&["C-3 -- arp:47"], Arpeggio::default());
    triplets.tracks[0].speed = Speed::SixteenthTriplet;
    assert_eq!(notes(&triplets, TICK * 2 / 3, 6), named("C E G C E G"));
}

#[test]
fn with_the_scale_on_x_and_y_are_degrees() {
    let mut major = song(&["C-3 -- arp:24"], Arpeggio::default());
    major.scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    assert_eq!(notes(&major, TICK, 3), named("C E G"));
    let mut minor = major.clone();
    minor.scale.kind = ScaleKind::Minor;
    assert_eq!(notes(&minor, TICK, 3), named("C D# G"));
    // Off, the same value is semitones.
    let chromatic = song(&["C-3 -- arp:24"], Arpeggio::default());
    assert_eq!(notes(&chromatic, TICK, 3), named("C D E"));
}

#[test]
fn the_modes_go_round_in_their_orders() {
    let rows = ["C-3 -- SGL2 arp:47"];
    let down = song(&rows, with(ArpRate::Tick, ArpMode::Down, 1));
    assert_eq!(notes(&down, TICK, 6), named("G E C G E C"));
    let up_down = song(&rows, with(ArpRate::Tick, ArpMode::UpDown, 1));
    assert_eq!(notes(&up_down, TICK, 9), named("C E G E C E G E C"));
    let up_down_inc = song(&rows, with(ArpRate::Tick, ArpMode::UpDownInc, 1));
    assert_eq!(notes(&up_down_inc, TICK, 9), named("C E G G E C C E G"));
    let both_octaves = song(&rows, with(ArpRate::Tick, ArpMode::UpDown, 2));
    assert_eq!(
        notes(&both_octaves, TICK, 11),
        named("C E G C4 E4 G4 E4 C4 G E C")
    );
}

#[test]
fn random_plays_any_of_its_notes_and_a_render_repeats() {
    let song = song(
        &["C-3 -- SGL4 arp:47"],
        with(ArpRate::Tick, ArpMode::Random, 2),
    );
    let heard = notes(&song, TICK, 24);
    let notes: Vec<i32> = heard.iter().flatten().copied().collect();
    assert_eq!(notes.len(), 24);
    let all = named("C E G C4 E4 G4");
    assert!(
        notes.iter().all(|note| all.contains(&Some(*note))),
        "{notes:?}"
    );
    // It isn't going round in order, and over a long note it plays most of them.
    assert_ne!(
        heard,
        named("C E G C4 E4 G4 C E G C4 E4 G4 C E G C4 E4 G4 C E G C4 E4 G4")
    );
    let different = all.iter().filter(|note| heard.contains(note)).count();
    assert!(different >= 4, "{heard:?}");
    let frames = 4 * STEP;
    assert_eq!(
        render_frames(version(&song), RATE, frames),
        render_frames(version(&song), RATE, frames)
    );
}

#[test]
fn random_leaves_chas_rolls_as_they_were() {
    // Which rows a chance lets play is the same whatever order the arpeggio goes round in, and
    // with no arpeggio at all.
    let starts = |rows: &[&str], arpeggio| {
        let out = render(&song(rows, arpeggio), 17 * STEP);
        (0..16)
            .map(|step| starts_near(&out, step * STEP))
            .collect::<Vec<_>>()
    };
    let arpeggiated = ["C-3 -- cha:80 arp:47"; 16];
    let plain = ["C-3 -- cha:80"; 16];
    let without = starts(&plain, Arpeggio::default());
    assert!(without.contains(&true) && without.contains(&false));
    for mode in ArpMode::ALL {
        let arpeggio = with(ArpRate::Tick, mode, 2);
        assert_eq!(starts(&arpeggiated, arpeggio), without, "{mode:?}");
    }
}

#[test]
fn octaves_climb_before_it_starts_again() {
    let two = song(&["C-3 -- SGL2 arp:47"], with(ArpRate::Tick, ArpMode::Up, 2));
    assert_eq!(notes(&two, TICK, 7), named("C E G C4 E4 G4 C"));
    // With nothing to go round, only the octaves.
    let octaves = song(&["C-3 -- SGL2 arp:00"], with(ArpRate::Tick, ArpMode::Up, 3));
    assert_eq!(notes(&octaves, TICK, 4), named("C C4 C5 C"));
    // And with one octave, `ARP 00` plays the note alone.
    let none = song(&["C-3 -- arp:00"], Arpeggio::default());
    assert_eq!(notes(&none, TICK, 3), named("C C C"));
}

#[test]
fn a_zero_makes_a_two_note_arp() {
    for value in ["70", "07"] {
        let song = song(&[&format!("C-3 -- arp:{value}")], Arpeggio::default());
        assert_eq!(notes(&song, TICK, 4), named("C G C G"), "ARP {value}");
    }
}

#[test]
fn legato_only_changes_the_pitch_and_retrig_starts_each_note_again() {
    let rows = ["C-3 -- arp:47", "--- --"];
    let legato = render(&song(&rows, Arpeggio::default()), 2 * STEP);
    let retrig = render(
        &song(
            &rows,
            Arpeggio {
                retrig: true,
                ..Arpeggio::default()
            },
        ),
        2 * STEP,
    );
    for note in 1..6 {
        let at = note * TICK;
        assert!(!starts_near(&legato, at), "legato carries on at {at}");
        assert!(starts_near(&retrig, at), "retrig starts again at {at}");
    }
    assert_eq!(heard(&retrig, 0, TICK, 6), named("C E G C E G"));
}

#[test]
fn on_acid_legato_slides_into_each_note_and_retrig_jumps_there() {
    let acid = |retrig| {
        let mut song = song(
            &["C-3 -- SGL2 arp:07"],
            Arpeggio {
                rate: ArpRate::ThirtySecond,
                retrig,
                ..Arpeggio::default()
            },
        );
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        song.tracks[0].gate_length = Some(0xFF);
        render(&song, 2 * STEP)
    };
    // How many times it rises through zero in a stretch, which goes up with the pitch.
    let rises = |out: &[f32], from: usize, to: usize| {
        (from..to)
            .filter(|at| out[*at] < 0.0 && out[at + 1] >= 0.0)
            .count()
    };
    let (legato, retrig) = (acid(false), acid(true));
    // Just after the G starts, a slide is still on its way up, and a new note is there.
    let just_after = |out: &[f32]| rises(out, STEP / 2 + 50, STEP / 2 + 650);
    assert!(
        just_after(&legato) < just_after(&retrig),
        "{} {}",
        just_after(&legato),
        just_after(&retrig)
    );
    // By the end of the note, both are at the G.
    let late = |out: &[f32]| rises(out, STEP / 2 + 1800, STEP / 2 + 2900);
    assert!(late(&legato).abs_diff(late(&retrig)) <= 1);
}

#[test]
fn the_next_note_ends_it() {
    let song = song(&["C-3 -- arp:47", "D-3 --"], Arpeggio::default());
    assert_eq!(notes(&song, TICK, 12), named("C E G C E G  D D D D D D"));
}

#[test]
fn it_lasts_its_note_through_its_pulses_and_the_empty_rows_after() {
    let pulses = song(&["C-3 -- SGL2 arp:47", "D-3 --"], Arpeggio::default());
    assert_eq!(
        notes(&pulses, TICK, 18),
        named("C E G C E G  C E G C E G  D D D D D D")
    );
    let empty = song(&["C-3 -- arp:47", "--- --"], Arpeggio::default());
    assert_eq!(notes(&empty, TICK, 9), named("C E G C E G C E G"));
}

#[test]
fn hld_carries_it_on_into_the_note_it_ties_into() {
    // `ARP 40` goes round two notes, three to a step, so the D carries on with the cycle's second.
    let arp = with(ArpRate::ThirtySecondTriplet, ArpMode::Up, 1);
    let tied = song(&["C-3 -- HLD1 arp:40", "D-3 --", "--- --"], arp);
    assert_eq!(notes(&tied, STEP / 3, 6), named("C E C  F# D F#"));
    // A row tied into with an `ARP` of its own carries on with that.
    let own = song(&["C-3 -- HLD1 arp:40", "D-3 -- arp:30", "--- --"], arp);
    assert_eq!(notes(&own, STEP / 3, 6), named("C E C  F D F"));
    // Without the `HLD`, the D is a new note with no arpeggio.
    let new_note = song(&["C-3 -- arp:40", "D-3 --", "--- --"], arp);
    assert_eq!(notes(&new_note, STEP / 3, 6), named("C E C  D D D"));
}

#[test]
fn a_new_note_starts_its_own_from_the_top() {
    let song = song(
        &["C-3 -- arp:47", "D-3 -- SGL2 arp:47"],
        with(ArpRate::SixteenthTriplet, ArpMode::Up, 1),
    );
    // The C's second note is cut short, and the D starts from its own note.
    let out = render(&song, 3 * STEP);
    assert_eq!(pitch(&out, 4400, 5800), Some(64));
    assert_eq!(heard(&out, STEP, STEP * 2 / 3, 3), named("D F# A"));
}

#[test]
fn the_gate_closing_ends_it() {
    let mut song = song(&["C-3 -- arp:47", "--- --"], Arpeggio::default());
    song.sample_on(0, "sine");
    // `6B` closes the gate 42% of the way through the step, on the G.
    song.tracks[0].gate_length = Some(0x6B);
    song.tracks[0].envelopes[0].release_ms = 500;
    let out = render(&song, 2 * STEP);
    let notes: Vec<i32> = (0..6)
        .map(|note| sine_note(&out, note * TICK + 100, note * TICK + 900))
        .collect();
    // Its release stays on the G.
    assert_eq!(notes, [60, 64, 67, 67, 67, 67]);
}

#[test]
fn an_off_ends_it() {
    let mut off = song(&["C-3 -- arp:47 off:03"], Arpeggio::default());
    off.sample_on(0, "sine");
    off.tracks[0].envelopes[0].release_ms = 500;
    let out = render(&off, STEP);
    let notes: Vec<i32> = (0..6)
        .map(|note| sine_note(&out, note * TICK + 100, note * TICK + 900))
        .collect();
    assert_eq!(notes, [60, 64, 67, 67, 67, 67]);
}

#[test]
fn each_pulse_of_a_mlt_starts_it_again() {
    let arp = with(ArpRate::SixteenthTriplet, ArpMode::Up, 1);
    let each = STEP * 2 / 3;
    let single = song(&["C-3 -- SGL2 arp:40"], arp);
    let out = render(&single, 3 * STEP);
    assert_eq!(heard(&out, 0, each, 3), named("C E C"));
    let multi = song(&["C-3 -- MLT2 arp:40"], arp);
    let out = render(&multi, 3 * STEP);
    // The second pulse starts again from the C, and its E comes two thirds of a step in.
    assert_eq!(pitch(&out, 200, 3800), Some(60));
    assert_eq!(pitch(&out, 4200, 5800), Some(64));
    assert_eq!(pitch(&out, STEP + 200, STEP + 3800), Some(60));
    assert_eq!(pitch(&out, STEP + 4200, STEP + 5800), Some(64));
}

#[test]
fn it_follows_a_nudged_or_delayed_note() {
    // Six 24ths late is a quarter of a step.
    let nudged = song(&["C-3 -- +6 arp:47", "--- --"], Arpeggio::default());
    let out = render(&nudged, 2 * STEP);
    assert_eq!(heard(&out, STEP / 4, TICK, 4), named("C E G C"));
    // `DEL 03` is half a step.
    let delayed = song(&["C-3 -- del:03 arp:47", "--- --"], Arpeggio::default());
    let out = render(&delayed, 2 * STEP);
    assert_eq!(heard(&out, STEP / 2, TICK, 4), named("C E G C"));
}

#[test]
fn an_accumulator_moves_its_root() {
    let song = song(&["C-3 -- acu:01 arp:47"], Arpeggio::default());
    assert_eq!(
        notes(&song, TICK, 12),
        named("C E G C E G  C# F G# C# F G#")
    );
}

#[test]
fn a_chains_transpose_moves_it_too() {
    use vt_model::song::{Chain, ChainEntry};
    let mut song = song(&["C-3 -- arp:47"], Arpeggio::default());
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 0,
                transpose: 2,
            }],
            label: String::new(),
        }),
    );
    assert_eq!(notes(&song, TICK, 3), named("D F# A"));
}

#[test]
fn a_ratchet_strikes_the_note_its_arpeggio_is_on() {
    // `RET 03` strikes every half step, on the arpeggio's fourth note, the C again.
    let song = song(&["C-3 -- arp:47 ret:03", "--- --"], Arpeggio::default());
    let out = render(&song, 2 * STEP);
    assert!(starts_near(&out, STEP / 2));
    assert_eq!(heard(&out, 0, TICK, 6), named("C E G C E G"));
}

#[test]
fn renders_with_arpeggios_repeat() {
    let rows = [
        "C-3 -- cha:80 arp:47",
        "E-3 -- MLT2 arp:37",
        "G-3 -- HLD1 arp:30 acu:01",
        "C-3 -- ret:02 arp:47",
    ];
    for mode in ArpMode::ALL {
        for retrig in [false, true] {
            let arpeggio = Arpeggio {
                mode,
                retrig,
                octaves: 2,
                ..Arpeggio::default()
            };
            let song = song(&rows, arpeggio);
            let render = || render_frames(version(&song), RATE, 16 * STEP);
            assert_eq!(render(), render(), "{mode:?} {retrig}");
        }
    }
}
