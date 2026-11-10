//! A track's `Gate`: how much of its last pulse a note lasts before its gate closes.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{SHORTEST_GATE, SettingId};
use vt_model::song::{ACID_GATE, Lane, Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// Where a gate at `80`, 50.2% of a pulse, closes, in frames from the note's start.
const HALF: usize = STEP * 0x80 / 0xFF;
/// How far either side of a moment a test looks, which is well within a step.
const NEAR: usize = 100;
/// Below this, nothing's sounding.
const SILENT: f32 = 1e-4;

/// A sample that holds one level, so what's heard is the note's envelope alone.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = vec![[0.5, 0.5]; RATE as usize * 8];
    pool.insert("flat", SampleData { frames });
    Arc::new(pool)
}

/// One sample track playing these rows, with notes that start and stop at once, so the moment
/// one ends can be seen.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "flat");
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The same with its `Gate` set.
fn gated(rows: &[&str], gate: u8) -> Song {
    let mut song = song(rows);
    song.tracks[0].gate_length = Some(gate);
    song
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song.clone()), pool()));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

fn sounding(out: &[f32], at: usize) -> bool {
    out[at].abs() > SILENT
}

/// How long the note starting at this step sounds for, in frames.
fn heard_for(out: &[f32], step: usize) -> usize {
    let start = step * STEP;
    (start..out.len())
        .find(|at| !sounding(out, *at))
        .unwrap_or(out.len())
        - start
}

/// Where a gate at this value closes in its first pulse, in frames.
fn closes(value: usize) -> usize {
    STEP * value / 0xFF
}

/// Whether a note sounded for about as long as a gate at this value.
fn lasted(out: &[f32], step: usize, value: usize) -> bool {
    heard_for(out, step).abs_diff(closes(value)) < NEAR
}

#[test]
fn multi_at_half_has_a_gap_in_each_pulse() {
    let rows = ["C-4 -- MLT2", "--- --", "--- --"];
    let full = render(&song(&rows), 3);
    let half = render(&gated(&rows, 0x80), 3);
    for pulse in 0..2 {
        let start = pulse * STEP;
        assert!(
            sounding(&full, start + HALF + NEAR),
            "pulse {pulse} at 100%"
        );
        assert!(sounding(&half, start + NEAR), "pulse {pulse} starts");
        assert!(sounding(&half, start + HALF - NEAR), "pulse {pulse} before");
        assert!(!sounding(&half, start + HALF + NEAR), "pulse {pulse} after");
        assert!(!sounding(&half, start + STEP - NEAR), "pulse {pulse} gap");
    }
}

#[test]
fn single_closes_its_gate_in_its_rows_last_pulse() {
    let out = render(
        &gated(&["C-4 -- SGL3", "--- --", "--- --", "--- --"], 0x80),
        4,
    );
    for at in [NEAR, STEP + HALF, 2 * STEP + HALF - NEAR] {
        assert!(sounding(&out, at), "held at {at}");
    }
    assert!(
        !sounding(&out, 2 * STEP + HALF + NEAR),
        "closed halfway through the third pulse"
    );
    assert!(!sounding(&out, 3 * STEP + NEAR));
}

#[test]
fn a_gate_is_measured_from_a_delayed_notes_start() {
    // `DEL 03` starts the note half a step late, so it ends half a step later too.
    let out = render(&gated(&["C-4 -- del:03", "--- --"], 0x80), 2);
    assert!(!sounding(&out, NEAR));
    assert!(sounding(&out, STEP / 2 + HALF - NEAR));
    assert!(!sounding(&out, STEP / 2 + HALF + NEAR));
}

#[test]
fn hold_ignores_the_gate_and_the_note_it_ties_into_closes_its_own() {
    let out = render(&gated(&["C-4 -- HLD2", "D-4 --", "--- --"], 0x80), 3);
    for at in [HALF + NEAR, STEP + HALF + NEAR, 2 * STEP + HALF - NEAR] {
        assert!(sounding(&out, at), "tied at {at}");
    }
    assert!(
        !sounding(&out, 2 * STEP + HALF + NEAR),
        "the tied note's own gate"
    );
}

#[test]
fn a_full_gate_plays_exactly_as_none() {
    let rows = [
        "C-4 -- MLT3",
        "D-4 -- SGL2",
        "--- -- ret:02",
        "C-4 -- HLD1",
        "E-4 --",
    ];
    let mut none = song(&rows);
    none.tracks[0].envelopes[0] = Default::default();
    let mut full = none.clone();
    full.tracks[0].gate_length = Some(0xFF);
    assert_eq!(render(&none, 8), render(&full, 8));
}

#[test]
fn a_len_lock_sets_its_rows_gate_and_ends_with_its_note() {
    let out = render(&song(&["C-4 -- gate:40", "C-4 --", "--- --"]), 3);
    // `40` is a quarter of the pulse.
    let quarter = STEP * 0x40 / 0xFF;
    assert!(sounding(&out, quarter - NEAR));
    assert!(!sounding(&out, quarter + NEAR));
    // The next note plays at the track's `Gate` again, which holds it.
    assert!(sounding(&out, STEP + HALF + NEAR));
    assert!(sounding(&out, 2 * STEP + HALF));
}

#[test]
fn an_lfo_moves_the_gate() {
    // A square at full depth takes `Gate` from `FF` right down for its second half.
    let mut song = song(&["C-4 --", "C-4 --", "C-4 --", "C-4 --"]);
    song.tracks[0].lfos[0] = Lfo {
        destination: Some(SettingId::GATE_LENGTH),
        shape: Shape::Square,
        rate: Rate::Steps(4),
        depth: 0xFF,
        ..Lfo::default()
    };
    let out = render(&song, 4);
    assert!(sounding(&out, STEP - NEAR), "held through the first step");
    assert!(
        !sounding(&out, 3 * STEP + STEP / 4),
        "short in the LFO's low half"
    );
}

#[test]
fn closing_the_gate_releases_the_note_without_a_click() {
    // At the default envelope, which releases over 50 ms, as `OFF` does.
    let mut song = gated(&["C-4 -- MLT4", "--- --"], 0x40);
    song.tracks[0].envelopes[0] = Default::default();
    let out = render(&song, 5);
    let closes = STEP * 0x40 / 0xFF;
    let release = RATE as usize * 50 / 1000;
    for pulse in 0..4 {
        let from = pulse * STEP + closes - NEAR;
        let jump = out[from..from + release + 2 * NEAR]
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        // Falling from 0.5 over 50 ms is about 0.0002 a frame.
        assert!(
            jump < 0.001,
            "pulse {pulse} jumps {jump} as its gate closes"
        );
        assert!(
            !sounding(&out, from + release + 2 * NEAR),
            "and it goes quiet"
        );
    }
}

/// An `Acid` track playing these rows, at its own `Gate`.
fn acid(rows: &[&str]) -> Song {
    let mut song = song(rows);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
    song.tracks[0].envelopes[0] = Default::default();
    song
}

/// The loudest the left channel is over a stretch.
fn peak(out: &[f32], from: usize, to: usize) -> f32 {
    out[from..to]
        .iter()
        .fold(0.0, |peak, sample| sample.abs().max(peak))
}

#[test]
fn acid_starts_at_half_and_ends_its_note_about_16_ms_after_the_gate() {
    assert_eq!(acid(&[]).tracks[0].gate_length(), ACID_GATE);
    let out = render(&acid(&["C-2 64", "--- --"]), 2);
    let held = peak(&out, HALF - 1000, HALF - 100);
    // Find where it has fallen 40 dB from where it was, a millisecond at a time, over a cycle of
    // the note, so the quiet stretch a resonant wave has between its peaks, as long as a
    // millisecond on a C-2 with a 303's resonance, isn't taken for its end (decision 129).
    let ms = RATE as usize / 1000;
    let cycle = (RATE as f32 / 65.406).round() as usize;
    let fallen = (HALF..STEP - cycle)
        .step_by(ms)
        .find(|at| peak(&out, *at, at + cycle) < held / 100.0)
        .expect("it falls silent");
    let after = (fallen - HALF) as f32 / ms as f32;
    assert!(
        (8.0..=30.0).contains(&after),
        "{after} ms after the gate closed"
    );
    // Held at 100%, it's still sounding.
    let mut full = acid(&["C-2 64", "--- --"]);
    full.tracks[0].gate_length = Some(0xFF);
    let full = render(&full, 2);
    assert!(peak(&full, STEP - 1000, STEP) > held / 4.0);
}

#[test]
fn an_acid_slide_holds_its_gate_into_the_next_note() {
    let out = render(&acid(&["C-2 64 HLD1", "C-3 64", "--- --"]), 3);
    let held = peak(&out, HALF - 1000, HALF - 100);
    // Through where a plain note's gate would close, and on into the note slid to.
    assert!(peak(&out, HALF + 1000, STEP) > held / 4.0);
    assert!(peak(&out, STEP, STEP + 1000) > held / 4.0);
    // The note slid to closes its own gate halfway through its step.
    assert!(peak(&out, STEP + HALF + 2000, 2 * STEP) < held / 100.0);
}

#[test]
fn a_rows_own_length_overrides_the_tracks_gate() {
    let out = render(
        &gated(
            &["C-4 -- SGL1 40", "C-4 --", "C-4 -- SGL2 C0", "--- --"],
            0x80,
        ),
        4,
    );
    assert!(lasted(&out, 0, 0x40), "{}", heard_for(&out, 0));
    assert!(lasted(&out, 1, 0x80), "the next row plays at the track's");
    // A `SGL2`'s length is of its last pulse, as `Gate`'s is.
    assert!(lasted(&out, 2, 0xFF + 0xC0), "{}", heard_for(&out, 2));
}

#[test]
fn a_len_lock_only_moves_rows_without_a_length() {
    let out = render(
        &song(&[
            "C-4 -- SGL1 C0 gate:40h",
            "C-4 --",
            "C-4 -- SGL1 C0 gate:20",
            "C-4 -- gate:20",
            "--- --",
        ]),
        5,
    );
    assert!(lasted(&out, 0, 0xC0), "its own length, not the lock's");
    assert!(lasted(&out, 1, 0x40), "the held lock, on a row with none");
    assert!(lasted(&out, 2, 0xC0));
    assert!(lasted(&out, 3, 0x20));
}

#[test]
fn ldlt_moves_both_kinds_of_row_up_or_down() {
    let out = render(
        &gated(
            &[
                "C-4 -- SGL1 80 gatedelta:40",
                "C-4 -- SGL1 40 gatedelta:C0",
                "C-4 -- gatedelta:40",
                "C-4 -- gatedelta:C0",
                "C-4 --",
                "--- --",
            ],
            0x80,
        ),
        6,
    );
    // `40` takes a quarter of a pulse off, and `C0` adds a quarter.
    assert!(lasted(&out, 0, 0x40), "{}", heard_for(&out, 0));
    assert!(lasted(&out, 1, 0x80), "{}", heard_for(&out, 1));
    assert!(lasted(&out, 2, 0x40), "{}", heard_for(&out, 2));
    assert!(lasted(&out, 3, 0xC0), "{}", heard_for(&out, 3));
    assert!(lasted(&out, 4, 0x80), "a lock ends with its note");
}

#[test]
fn ldlt_in_a_drifting_lane_gives_a_row_a_different_length_each_pass() {
    let rows = [
        "C-4 -- gatedelta:40",
        "C-4 -- gatedelta:80",
        "C-4 -- gatedelta:C0",
        "C-4 --",
    ];
    let mut song = gated(&rows, 0x80);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 3,
        }),
    );
    let out = render(&song, 12);
    // The 4-row phrase's first row reads the 3-row lane's rows 0, 1 and 2 in turn.
    for (pass, value) in [0x40, 0x80, 0xC0].into_iter().enumerate() {
        assert!(
            lasted(&out, pass * 4, value),
            "pass {pass}: {}",
            heard_for(&out, pass * 4)
        );
    }
}

#[test]
fn an_lfo_on_gate_moves_a_rows_own_length() {
    let mut song = song(&[
        "C-4 -- SGL1 80",
        "C-4 -- SGL1 80",
        "C-4 -- SGL1 80",
        "C-4 -- SGL1 80",
    ]);
    song.tracks[0].lfos[0] = Lfo {
        destination: Some(SettingId::GATE_LENGTH),
        shape: Shape::Square,
        rate: Rate::Steps(4),
        depth: 0x40,
        ..Lfo::default()
    };
    let out = render(&song, 4);
    assert!(lasted(&out, 1, 0xC0), "{}", heard_for(&out, 1));
    assert!(lasted(&out, 3, 0x40), "{}", heard_for(&out, 3));
}

#[test]
fn a_moved_length_stays_between_1_and_100_percent() {
    let out = render(
        &song(&[
            "C-4 -- SGL1 10 gatedelta:00",
            "C-4 -- SGL1 C0 gatedelta:FF",
            "--- --",
            "C-4 -- SGL1 01 gatedelta:00",
            "--- --",
        ]),
        5,
    );
    // The shortest, 1%, still sounds.
    let shortest = (STEP as f32 * SHORTEST_GATE) as usize;
    assert!(
        heard_for(&out, 0).abs_diff(shortest) < NEAR / 2,
        "{}",
        heard_for(&out, 0)
    );
    assert!(
        heard_for(&out, 3).abs_diff(shortest) < NEAR / 2,
        "{}",
        heard_for(&out, 3)
    );
    // At 100% nothing closes it, so it holds through the empty row to the next note.
    assert!(sounding(&out, 2 * STEP + STEP / 2));
}

#[test]
fn hold_ignores_its_own_length_and_ldlt() {
    let out = render(
        &gated(&["C-4 -- HLD2 20 gatedelta:00", "D-4 --", "--- --"], 0xFF),
        4,
    );
    for at in [STEP / 2, STEP + STEP / 2, 2 * STEP + NEAR] {
        assert!(sounding(&out, at), "tied at {at}");
    }
    // The note tied into closes its own gate, and the lock is still on, as it's the same note.
    let closes = 2 * STEP + closes(0xFF - 0x80);
    assert!(sounding(&out, closes - NEAR));
    assert!(!sounding(&out, closes + NEAR));
}

#[test]
fn lengths_of_100_percent_and_an_ldlt_of_80_play_exactly_as_none() {
    let plain = [
        "C-4 -- MLT3",
        "D-4 -- SGL2",
        "--- -- ret:02",
        "C-4 -- HLD1",
        "E-4 --",
    ];
    let full = [
        "C-4 -- MLT3 FF gatedelta:80",
        "D-4 -- SGL2 FF",
        "--- -- ret:02",
        "C-4 -- HLD1 FF gatedelta:80h",
        "E-4 -- SGL1 FF",
    ];
    let mut none = song(&plain);
    none.tracks[0].envelopes[0] = Default::default();
    let mut set = song(&full);
    set.tracks[0].envelopes[0] = Default::default();
    assert_eq!(render(&none, 8), render(&set, 8));
}
