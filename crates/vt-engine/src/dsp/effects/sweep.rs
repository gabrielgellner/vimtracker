//! Sines swept through a slot and measured, for the tests that hold the slot effects' curves to
//! what's heard (decision 333).

use std::f64::consts::TAU;

use vt_model::insert::Insert;
use vt_model::song::Song;

use crate::dsp::effects::memory::{MemoryPool, SongPlace};
use crate::dsp::effects::slot::{InsertState, Keys};

/// How loud the sines are: quiet enough that every clip and limiter on the way leaves them alone,
/// even on a peak 40 dB up.
const LEVEL: f64 = 1e-3;

/// A slot holding `insert`, with what it keeps from a pool of its own, following `note` and
/// settled on its settings, as it is once it's been playing a while.
pub(crate) fn slot(insert: Insert, note: f32, rate: u32) -> InsertState {
    let mut song = Song::new(1);
    song.tracks[0].inserts[0] = insert;
    let mut pool = MemoryPool::for_song(&song, rate);
    let mut state = InsertState::default();
    state.follow_note(note);
    let place = SongPlace {
        frames_per_tick: 25.0,
        ..SongPlace::STILL
    };
    state.hold(insert, &mut pool, place, 0);
    state.settle();
    state
}

/// How much a slot leaves a sine at `hertz`, in dB, by the middle of its two sides: played for
/// `settle` seconds so what rings has come to a steady level, then measured over the next tenth of
/// a second or more, a whole number of cycles, by its Fourier sum. `keys` is what it hears of the
/// other tracks.
pub(crate) fn measured_db(
    state: &mut InsertState,
    hertz: f32,
    settle: f32,
    rate: u32,
    keys: &Keys,
) -> f32 {
    let turn = f64::from(hertz) / f64::from(rate);
    let settle = (settle * rate as f32) as usize;
    let cycles = (0.1 * f64::from(hertz)).ceil().max(8.0);
    let frames = (cycles / turn).round() as usize;
    let (mut real, mut imaginary) = (0.0, 0.0);
    for frame in 0..settle + frames {
        let phase = TAU * ((turn * frame as f64) % 1.0);
        let input = (phase.sin() * LEVEL) as f32;
        let out = state.process([input; 2], 1.0, rate, keys);
        if frame >= settle {
            let middle = f64::from(out[0] + out[1]) * 0.5;
            real += middle * phase.cos();
            imaginary += middle * phase.sin();
        }
    }
    let size = 2.0 * real.hypot(imaginary) / frames as f64 / LEVEL;
    (20.0 * size.max(1e-12).log10()) as f32
}

/// How close a curve has to be to what's measured, in dB: a pixel of the zoom's 624 is 0.1 dB of
/// the 60 its frequency curves cover.
pub(crate) const WITHIN_DB: f32 = 0.1;
/// How far down a curve has to be before it's only held to being as far down, as a notch's depth
/// is lost in the measuring, and the curve's bottom is 36 dB down.
pub(crate) const DEEP_DB: f32 = -40.0;

/// Frequencies from 25 Hz to 16 kHz, evenly in pitch, `count` of them.
pub(crate) fn across(count: usize) -> impl Iterator<Item = f32> {
    (0..count).map(move |at| 25.0 * 640_f32.powf(at as f32 / (count - 1) as f32))
}

/// Whether a curve says what was measured: within [`WITHIN_DB`], or both deep in a notch.
pub(crate) fn agrees(curve: f32, measured: f32) -> bool {
    (curve - measured).abs() < WITHIN_DB || (curve < DEEP_DB && measured < DEEP_DB + 2.0)
}
