//! A synth track's `Phase`: whether its oscillators run on from note to note, as a carried voice's
//! do, or start again on each note, at the bottom of a short dip to nothing (decision 370).

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phase, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Frames in a step at 120 BPM.
const STEP: usize = 6000;
/// How much of each note is compared: 10 ms from its step's start, past its dip and attack.
const HEARD: usize = 480;

/// One synth track playing these rows, with this `Phase`.
fn song(voice: SynthVoice, phase: Phase, rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.machine = Machine::Plaits(voice);
    track.phase = phase;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The left channel, for so many steps.
fn render(song: Song, steps: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(
        Arc::new(song),
        Arc::new(SamplePool::default()),
    ));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// The start of the note at a step, past the dip the note before fades out in.
fn start(out: &[f32], step: usize) -> &[f32] {
    &out[step * STEP + DIP..step * STEP + HEARD]
}

/// How long a carried note's dip from full takes, with a frame each side.
const DIP: usize = 66;

/// The most two notes' starts differ by, frame for frame.
fn apart(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .fold(0.0_f32, |most, (a, b)| most.max((a - b).abs()))
}

const AGAIN: [&str; 4] = [
    "C-3 7F --- --- ---",
    "C-3 7F --- --- ---",
    "C-3 7F --- --- ---",
    "C-3 7F --- --- ---",
];

#[test]
fn reset_starts_each_repeated_note_the_same_to_the_bit() {
    // Each note after the first is carried on the voice: `Free` runs its oscillators on, so the
    // same note starts wherever they've got to; `Reset` starts them again at the bottom of the
    // dip, so it starts as the note before did, frame for frame. (Unless it rolls where they
    // start, as `VA`'s `Drift` and the `Supersaw`'s `Phase` do, each note its own roll.)
    for voice in [
        SynthVoice::VirtualAnalog,
        SynthVoice::Fm,
        SynthVoice::FourOp,
        SynthVoice::CSaw,
    ] {
        let reset = render(song(voice, Phase::Reset, &AGAIN), 4);
        let between = apart(start(&reset, 2), start(&reset, 3));
        // A Braids model's DC blocker isn't started again, as it's settled to the wave the model
        // starts again on, and what's left of it moves a hundred-thousandth.
        let most = if voice.is_macro() { 1e-4 } else { 0.0 };
        assert!(between <= most, "{voice:?} with Reset: {between}");
        assert!(
            start(&reset, 2).iter().any(|sample| *sample != 0.0),
            "{voice:?} sounds"
        );
        let free = render(song(voice, Phase::Free, &AGAIN), 4);
        let between = apart(start(&free, 2), start(&free, 3));
        assert!(between > 0.01, "{voice:?} with Free: {between}");
    }
}

#[test]
fn free_is_the_carried_voice_as_it_was() {
    // `Free` is what a track had before it could choose: a song that doesn't set it plays as one
    // set to `Free` does.
    let mut unset = song(SynthVoice::VirtualAnalog, Phase::Free, &AGAIN);
    unset.tracks[0].phase = Phase::default();
    let free = song(SynthVoice::VirtualAnalog, Phase::Free, &AGAIN);
    assert_eq!(render(unset, 4), render(free, 4));
    assert_eq!(Phase::default(), Phase::Free);
}

#[test]
fn reset_dips_to_silence_whatever_the_note_shape_says() {
    // With the note shape's own `Reset` off, `Free` attacks from where the level is, but `Phase`
    // at `Reset` still takes the dip, as it can only start the oscillators again at silence.
    let mut keep = song(SynthVoice::VirtualAnalog, Phase::Reset, &AGAIN);
    keep.tracks[0].mods[0].adsr = Adsr {
        reset: false,
        ..Adsr::note_shape()
    };
    let out = render(keep, 4);
    let quietest = out[2 * STEP..2 * STEP + 200]
        .iter()
        .fold(f32::MAX, |least, sample| least.min(sample.abs()));
    assert!(quietest < 1e-4, "dips to {quietest}");
    let mut free = song(SynthVoice::VirtualAnalog, Phase::Free, &AGAIN);
    free.tracks[0].mods[0].adsr = keep_shape();
    let out = render(free, 4);
    let level = out[2 * STEP..2 * STEP + 200]
        .iter()
        .fold(0.0_f32, |most, sample| most.max(sample.abs()));
    assert!(level > 0.1, "Free carries on at {level}");
}

fn keep_shape() -> Adsr {
    Adsr {
        reset: false,
        ..Adsr::note_shape()
    }
}

#[test]
fn a_step_can_lock_it_for_its_note() {
    // `OSC01` on a step starts that note's oscillators again on a track left `Free`, as `Reset`
    // would; the note after it, with no lock, runs on again.
    let locked = [
        "C-3 7F --- --- ---",
        "C-3 7F --- --- ---",
        "C-3 7F phase:01 --- ---",
        "C-3 7F --- --- ---",
    ];
    let free = render(song(SynthVoice::VirtualAnalog, Phase::Free, &locked), 4);
    let reset = render(song(SynthVoice::VirtualAnalog, Phase::Reset, &AGAIN), 4);
    let locked = apart(start(&free, 2), start(&reset, 2));
    assert!(locked == 0.0, "the locked note starts again: {locked}");
    let after = apart(start(&free, 3), start(&reset, 3));
    assert!(after > 0.01, "the note after runs on: {after}");
}

#[test]
fn a_voice_without_a_phase_ignores_it() {
    // A noise has no cycle to start again, so `Reset` leaves it as `Free` has it.
    let free = render(song(SynthVoice::Noise, Phase::Free, &AGAIN), 4);
    let reset = render(song(SynthVoice::Noise, Phase::Reset, &AGAIN), 4);
    assert_eq!(free, reset);
}
