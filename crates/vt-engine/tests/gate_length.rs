//! A track's `Gate`: how much of its last pulse a note lasts before its gate closes.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{ACID_GATE, Machine, Phrase, Song, SynthVoice};

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
    song.tracks[0].sample = Some("flat".to_owned());
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
    song.tracks[0].sample = None;
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
    // Find where it has fallen 40 dB from where it was, in windows of a millisecond.
    let ms = RATE as usize / 1000;
    let fallen = (HALF..STEP)
        .step_by(ms)
        .find(|at| peak(&out, *at, at + ms) < held / 100.0)
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
