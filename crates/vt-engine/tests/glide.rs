//! A track's `Glide`: how long a `HLD` tie glides its pitch into the next note, on every voice,
//! Renoise's `-Gxx` done as a time rather than a rate. Off, a tie jumps as it always did, and
//! `Acid` slides by its own `Slide`.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::glide_value;
use vt_model::song::{FULL_GATE, Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// Frames in a ms.
const MS: usize = RATE as usize / 1000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// A glide's time constant is a third of its time, so it's halfway there in this share of it.
const HALFWAY: f64 = std::f64::consts::LN_2 / 3.0;
/// A tie from `C-3` to `C-4`, an octave up, with two and a half seconds after it to hear the
/// glide land before the phrase comes round.
const TIE: [&str; 21] = [
    "C-3 -- HLD1",
    "C-4 --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
    "--- --",
];

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let ramp = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("ramp", SampleData { frames: ramp });
    Arc::new(pool)
}

/// One track playing these rows, gliding its ties for so many ms, or with `Glide` off at `None`.
/// Notes start and stop at once and hold to the next, so only the pitch moves.
fn song(rows: &[&str], machine: Machine, glide_ms: Option<f32>) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.machine = machine;
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    track.gate_length = Some(FULL_GATE);
    track.glide = glide_ms.map_or(0, glide_value);
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
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

/// How far above `C-3` the ramp plays around a frame, in semitones, by how fast it climbs.
fn semitones_at(out: &[f32], at: usize) -> f64 {
    let (from, to) = (at - 12, at + 12);
    let climb = f64::from(out[to] - out[from]) / (to - from) as f64;
    12.0 * (climb * RAMP_FRAMES as f64).log2()
}

/// The frequency a synth plays between two frames, by how far apart it rises through zero.
fn hertz_between(out: &[f32], from: usize, to: usize) -> f64 {
    let crossings: Vec<f64> = (from..to)
        .filter(|at| out[*at] < 0.0 && out[at + 1] >= 0.0)
        .map(|at| at as f64 + f64::from(-out[at] / (out[at + 1] - out[at])))
        .collect();
    assert!(crossings.len() > 2, "too few crossings to hear a pitch");
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    RATE as f64 * (crossings.len() - 1) as f64 / (last - first)
}

#[test]
fn a_tie_glides_a_sample_an_octave_up_in_its_time() {
    let out = render(&song(&TIE, Machine::Sampler, Some(100.0)), 4);
    // It starts from the note it ties from, is halfway in pitch at the time constant's share,
    // nearly there when its time is up, and lands on the note.
    assert!(semitones_at(&out, 12).abs() < 0.01, "C-3 before it");
    assert!(semitones_at(&out, STEP + 12) < 0.5);
    let halfway = STEP + (100.0 * HALFWAY * MS as f64) as usize;
    let at_halfway = semitones_at(&out, halfway);
    assert!((at_halfway - 6.0).abs() < 0.3, "{at_halfway}");
    let at_its_time = semitones_at(&out, STEP + 100 * MS);
    assert!((11.2..12.0).contains(&at_its_time), "{at_its_time}");
    let landed = semitones_at(&out, STEP + 300 * MS);
    assert!((landed - 12.0).abs() < 0.01, "{landed}");
}

#[test]
fn a_longer_glide_takes_longer_in_proportion() {
    for ms in [20.0, 400.0] {
        let out = render(&song(&TIE, Machine::Sampler, Some(ms)), 4);
        let halfway = STEP + (f64::from(ms) * HALFWAY * MS as f64) as usize;
        let at_halfway = semitones_at(&out, halfway);
        assert!((at_halfway - 6.0).abs() < 0.3, "{ms} ms: {at_halfway}");
    }
}

#[test]
fn with_glide_off_a_tie_jumps_as_before() {
    let out = render(&song(&TIE, Machine::Sampler, None), 2);
    let next = semitones_at(&out, STEP + 12);
    assert!((next - 12.0).abs() < 0.01, "{next}");
    // A note that isn't tied doesn't glide whatever the setting.
    let struck = render(
        &song(&["C-3 --", "C-4 --"], Machine::Sampler, Some(400.0)),
        2,
    );
    // Past the note before's fade, which a new note's start cuts it with.
    assert!((semitones_at(&struck, STEP + 200) - 12.0).abs() < 0.01);
}

#[test]
fn a_lock_on_either_row_of_a_tie_sets_its_glide() {
    let halfway = STEP + (100.0 * HALFWAY * MS as f64) as usize;
    let glide = format!("glide:{:02X}", glide_value(100.0));
    let on_the_tie = [
        &format!("C-3 -- HLD1 {glide}"),
        "C-4 --",
        "--- --",
        "--- --",
    ];
    let on_the_next = [
        "C-3 -- HLD1",
        &format!("C-4 -- {glide}"),
        "--- --",
        "--- --",
    ];
    for rows in [on_the_tie, on_the_next] {
        let rows: Vec<&str> = rows.iter().map(|row| row.as_ref()).collect();
        let out = render(&song(&rows, Machine::Sampler, None), 3);
        let at_halfway = semitones_at(&out, halfway);
        assert!((at_halfway - 6.0).abs() < 0.3, "{rows:?}: {at_halfway}");
    }
}

#[test]
fn a_synth_glides_by_its_pitch() {
    let voice = Machine::Plaits(SynthVoice::VirtualAnalog);
    let (low, high) = (261.63, 523.25);
    let glided = render(&song(&TIE, voice, Some(100.0)), 3);
    let jumped = render(&song(&TIE, voice, None), 3);
    // Early on it's still between the notes, and by twice its time it's on the new one.
    let early = hertz_between(&glided, STEP + 5 * MS, STEP + 20 * MS);
    assert!(early > low * 1.05 && early < high * 0.8, "{early} Hz");
    let landed = hertz_between(&glided, STEP + 200 * MS, STEP + 240 * MS);
    assert!((landed / high - 1.0).abs() < 0.01, "{landed} Hz");
    let at_once = hertz_between(&jumped, STEP + 5 * MS, STEP + 20 * MS);
    assert!((at_once / high - 1.0).abs() < 0.01, "{at_once} Hz");
}

#[test]
fn every_pitched_synth_glides() {
    // The noises and drums have no single pitch to follow through zero, and `Formant` rises
    // through zero at its formants rather than its pitch.
    let unpitched = [
        SynthVoice::Formant,
        SynthVoice::Noise,
        SynthVoice::Dust,
        SynthVoice::Snare,
        SynthVoice::Hat,
        SynthVoice::Kick,
    ];
    for voice in SynthVoice::ALL {
        if unpitched.contains(&voice) {
            continue;
        }
        let machine = Machine::Plaits(voice);
        // Where it lands is heard on the sample and `VA` above: a voice of several partials,
        // as a chord or an electric piano, rises through zero by their phases too.
        let glided = render(&song(&TIE, machine, Some(200.0)), 2);
        let jumped = render(&song(&TIE, machine, None), 2);
        let early = |out: &[f32]| hertz_between(out, STEP + 5 * MS, STEP + 40 * MS);
        assert!(
            early(&glided) < early(&jumped) * 0.85,
            "{voice:?}: {} Hz gliding, {} Hz jumping",
            early(&glided),
            early(&jumped)
        );
    }
}

#[test]
fn acid_slides_by_its_own_slide_with_glide_off_and_by_glide_when_its_set() {
    let acid = Machine::Plaits(SynthVoice::Acid);
    let (low, high) = (261.63, 523.25);
    let own = render(&song(&TIE, acid, None), 4);
    let slow = render(&song(&TIE, acid, Some(1000.0)), 18);
    // Its own 60 ms slide is most of the way there 40 ms in, where a second's is barely moving.
    let window = |out: &[f32]| hertz_between(out, STEP + 40 * MS, STEP + 60 * MS);
    assert!(window(&own) > high * 0.85, "{} Hz", window(&own));
    assert!(window(&slow) < low * 1.3, "{} Hz", window(&slow));
    // Both land on the note.
    let late = |out: &[f32], at: usize| hertz_between(out, STEP + at * MS, STEP + (at + 40) * MS);
    assert!((late(&own, 300) / high - 1.0).abs() < 0.01);
    assert!((late(&slow, 2000) / high - 1.0).abs() < 0.01);
}
