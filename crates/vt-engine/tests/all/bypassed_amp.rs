//! A track's notes with the slot that shapes them switched out: the sound passes through at full,
//! so a machine with an envelope of its own is shaped by it alone and rings out its own release,
//! and the note ends as the machine settles (decision 371).

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Frames in a step at 120 BPM.
const STEP: usize = 6000;
/// Where a gate at `40`, a quarter of a step, closes.
const CLOSES: usize = STEP * 0x40 / 0xFF;
/// Frames in a millisecond.
const MS: usize = 48;

/// One note on a synth with a gate of a quarter, its notes' shape in or switched out.
fn song(voice: SynthVoice, bypassed: bool, set: &[(&str, u8)]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.machine = Machine::Plaits(voice);
    track.gate_length = Some(0x40);
    track.mods[0].bypassed = bypassed;
    for (key, value) in set {
        track.set_setting(SettingId::from_key(key).unwrap(), *value);
    }
    let mut rows = vec!["C-3 7F --- --- ---".to_owned()];
    rows.extend((1..16).map(|_| "--- -- --- --- ---".to_owned()));
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn render(song: Song, frames: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(
        Arc::new(song),
        Arc::new(SamplePool::default()),
    ));
    render_frames(version, RATE, frames)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

fn peak(part: &[f32]) -> f32 {
    part.iter().fold(0.0, |most, sample| most.max(sample.abs()))
}

/// Where the sound stops for good, and the last frame before it, against its peak.
fn end(out: &[f32]) -> (usize, f32) {
    let last = out.iter().rposition(|sample| *sample != 0.0).unwrap_or(0);
    (last, out[last].abs() / peak(out).max(1e-9))
}

/// 4-op's first operator heard alone with a long release of its own, and Acid's own decay.
const LONG_FOUR_OP: [(&str, u8); 1] = [("4op.release1", 0xD0)];

#[test]
fn a_switched_out_shape_lets_a_machine_ring_out_its_own_release() {
    // The notes' shape releases over 50 ms as the gate closes and cuts the operator's long
    // release off; switched out, the release is the operator's own, heard to its end.
    let shaped = render(song(SynthVoice::FourOp, false, &LONG_FOUR_OP), 2 * STEP);
    let open = render(song(SynthVoice::FourOp, true, &LONG_FOUR_OP), 2 * STEP);
    let after = |out: &[f32]| peak(&out[CLOSES + 80 * MS..CLOSES + 100 * MS]);
    assert!(
        after(&shaped) < 1e-6,
        "shaped, cut by 80 ms: {}",
        after(&shaped)
    );
    assert!(
        after(&open) > 0.05 * peak(&open),
        "switched out, still ringing at 80 ms: {}",
        after(&open) / peak(&open)
    );
    // Before the gate, both are the same sound but for the shape's 1 ms attack.
    assert!((peak(&open[..CLOSES]) - peak(&shaped[..CLOSES])).abs() < 0.05 * peak(&open));
}

#[test]
fn a_machine_with_its_own_envelope_ends_where_its_sound_is_gone() {
    for (voice, set) in [
        (SynthVoice::FourOp, &[("4op.release1", 0x70)][..]),
        (SynthVoice::SixOp, &[][..]),
        (SynthVoice::Acid, &[][..]),
    ] {
        let out = render(song(voice, true, set), 15 * STEP);
        let (last, step) = end(&out);
        println!(
            "{voice:?} ends {:.0} ms after its gate, from {:.0} dB of its peak",
            (last - CLOSES) as f32 / MS as f32,
            20.0 * step.max(1e-12).log10()
        );
        assert!(last > CLOSES, "{voice:?} sounds past its gate");
        assert!(last < 15 * STEP - 1, "{voice:?} ends: at {last}");
        // Its last frame is no step: under -80 dB of its peak.
        assert!(step < 1e-4, "{voice:?} ends from {step} of its peak");
    }
}

#[test]
fn a_machine_with_no_envelope_of_its_own_stops_as_its_gate_closes() {
    // A raw oscillator with nothing to shape it: switched out, the note stops as its gate does,
    // as one with no amp would, which is what the guide says to switch the slot out for.
    let out = render(song(SynthVoice::CSaw, true, &[]), 2 * STEP);
    let (last, _) = end(&out);
    assert!(
        last.abs_diff(CLOSES) < MS,
        "stops at {last}, the gate at {CLOSES}"
    );
    // And it starts at full, with no attack.
    assert!(out[..MS / 2].iter().any(|sample| sample.abs() > 0.05));
}
