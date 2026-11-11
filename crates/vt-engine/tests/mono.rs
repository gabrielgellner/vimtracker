//! What each effect claims about a sound that's the same on both sides (decision 147): an effect
//! that says it works one side out for both, or that its sides are linked, must come out exactly
//! as working out both does, and the same on both sides, as a sound goes from the same on both to
//! not and back. Every effect that claims either is checked, so a new one that opts in is too.

use vt_engine::dsp::distort::{InsertState, Mono};
use vt_engine::track_state::Keys;
use vt_model::insert::{
    Compressor, CompressorType, Distort, DistortType, Equaliser, GateMode, Insert, LowPassGate,
    Vowel,
};

const RATE: u32 = 48_000;

/// Every effect as it starts, and each of the types and modes of those that have them, set so
/// they do something.
fn every_effect() -> Vec<Insert> {
    let mut effects = Insert::ALL.to_vec();
    effects.extend(DistortType::ALL.map(|kind| {
        Insert::Distort(Distort {
            kind,
            amount: 0xA0,
            tone: 0x60,
            bits: 5,
            rate: 0x40,
            mix: 0xC0,
            ..Distort::DEFAULT
        })
    }));
    effects.push(Insert::Eq(Equaliser {
        low: 0xE0,
        mid: 0x20,
        mid_q: 0xC0,
        high: 0xD0,
        ..Equaliser::DEFAULT
    }));
    effects.extend(CompressorType::ALL.map(|kind| {
        Insert::Compressor(Compressor {
            kind,
            threshold: 0x30,
            ratio: 0xC0,
            peak_clip: 1,
            ..Compressor::DEFAULT
        })
    }));
    effects.extend(GateMode::ALL.map(|mode| {
        Insert::Lpg(LowPassGate {
            mode,
            amount: 0xC0,
            resonance: 0x80,
            ..LowPassGate::DEFAULT
        })
    }));
    effects.push(Insert::Vowel(Vowel {
        vowel: 0x90,
        resonance: 0xC0,
        mix: 0xE0,
        ..Vowel::DEFAULT
    }));
    effects
}

fn slot(insert: Insert) -> InsertState {
    let mut state = InsertState::default();
    state.set(insert);
    state.settle();
    state
}

/// Noise from a fixed seed, loud and quiet in turns.
fn noise(frames: usize, seed: u32) -> Vec<f32> {
    let mut state = seed.max(1);
    (0..frames)
        .map(|at| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let level = if (at / 2400) % 2 == 0 { 0.9 } else { 0.05 };
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
        })
        .collect()
}

/// Where the sides first differ.
const MONO_UNTIL: usize = 9600;

/// Noise the same on both sides, then different, then the same again, then a single frame apart,
/// then nothing for long enough for anything to rest, and the same on both sides again.
fn turns() -> Vec<[f32; 2]> {
    let left = noise(RATE as usize, 7);
    let right = noise(RATE as usize, 11);
    let mut frames: Vec<[f32; 2]> = left[..MONO_UNTIL].iter().map(|&s| [s; 2]).collect();
    frames.extend((MONO_UNTIL..14_400).map(|at| [left[at], right[at]]));
    frames.extend(left[14_400..24_000].iter().map(|&s| [s; 2]));
    frames[20_000][1] += 0.25;
    frames.extend(std::iter::repeat_n([0.0; 2], 2 * RATE as usize));
    frames.extend(left[24_000..].iter().map(|&s| [s; 2]));
    frames
}

fn same(frame: [f32; 2]) -> bool {
    frame[0].to_bits() == frame[1].to_bits()
}

#[test]
fn every_claim_comes_out_as_working_out_both_does() {
    let keys = Keys::default();
    let input = turns();
    let mut claims = 0;
    for insert in every_effect() {
        let mut shortcut = slot(insert);
        if shortcut.mono() == Mono::Stereo {
            continue;
        }
        claims += 1;
        let mut both = slot(insert);
        for (at, &frame) in input.iter().enumerate() {
            let (quick, kept) = shortcut.process_sides(frame, same(frame), 1.0, RATE, &keys);
            let (full, _) = both.process_sides(frame, false, 1.0, RATE, &keys);
            assert_eq!(
                quick.map(f32::to_bits),
                full.map(f32::to_bits),
                "{insert:?} at frame {at}"
            );
            assert!(
                !kept || same(quick),
                "{insert:?} at frame {at} says its sides are the same when they aren't"
            );
            // Until the sides first differ, it keeps them the same. After, its sides' states can
            // stay apart for a while, and it works out both, as the frame's checks above show.
            assert!(
                kept || at >= MONO_UNTIL,
                "{insert:?} at frame {at} doesn't keep a sound the same on both sides"
            );
        }
    }
    // An empty slot, and Distort, EQ3, the compressor, the gate and Vowel as they start, then
    // Distort's eight types, EQ3, the three compressors, the three gates and Vowel as set above.
    assert_eq!(claims, 22);
}
