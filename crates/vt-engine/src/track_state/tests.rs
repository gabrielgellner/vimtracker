use vt_model::constants::CUT_FADE_FRAMES;
use vt_model::modulation::ModKind;
use vt_model::song::{Machine, level_gain};
use vt_model::step::Mark;

use crate::dsp::effects::slot::SidechainFrames;

use super::mods::in_scale;
use super::shaping::{pan_gains, pan_position};
use super::*;

#[test]
fn a_value_for_a_voice_not_playing_waits_for_it() {
    let mut state = TrackState::new(0, 0);
    let on = |voice| Track {
        machine: Machine::Plaits(voice),
        ..Track::default()
    };
    state.load(&on(SynthVoice::Kick));
    let kick_decay = state.synth_settings().morph;
    let string_decay = SettingId::voice(SynthVoice::String, 2).unwrap();
    state.set_value(string_decay, 0x1000, Mark::Hold);
    assert_eq!(
        state.synth_settings().morph,
        kick_decay,
        "the Kick doesn't hear it"
    );
    state.load(&on(SynthVoice::String));
    assert_eq!(state.synth_settings().morph, 0x10 as f32 / 255.0);
}

#[test]
fn a_tracks_own_fine_part_is_in_its_base() {
    let mut track = Track {
        cutoff: 0x40,
        ..Track::default()
    };
    track.set_fine(SettingId::CUT, 0x80);
    assert_eq!(base_of(SettingId::CUT, &track), 0x4080);
    track.cutoff = 0xFF;
    track.set_fine(SettingId::CUT, 0x80);
    assert_eq!(base_of(SettingId::CUT, &track), 0xFF00, "none at the top");
}

#[test]
fn a_values_fine_part_is_heard_between_its_coarse_steps() {
    let mut state = TrackState::new(0, 0);
    state.load(&Track::default());
    state.set_value(SettingId::CUT, 0x4080, Mark::Lock);
    assert_eq!(state.cutoff.target, 64.5 / 255.0);
    state.set_value(SettingId::CUT, 0x4000, Mark::Lock);
    assert_eq!(
        state.cutoff.target,
        0x40 as f32 / 255.0,
        "the same as before fine parts"
    );
}

#[test]
fn a_value_for_an_effect_not_in_its_slot_waits_for_it() {
    use vt_model::insert::Insert;
    let mut state = TrackState::new(0, 0);
    let mut track = Track::default();
    track.inserts[1] = Insert::ALL[3];
    state.load(&track);
    let feedback = SettingId::slot(1, EffectKind::Delay, 1).unwrap();
    state.set_value(feedback, 0x3000, Mark::Hold);
    assert_ne!(
        state.slot_values[1][1], 0x30 as f32,
        "the Reverb doesn't hear it"
    );
    track.inserts[1] = Insert::ALL[2];
    state.load(&track);
    assert_eq!(state.slot_values[1][1], 0x30 as f32);
}

/// A track's state with its values from a track, as playing from the top leaves them.
fn from_the_top(track: &Track) -> TrackState {
    let mut state = TrackState::new(0, 0);
    state.reset_held(0, 0, track, 0);
    state
}

#[test]
fn a_macro_reaches_a_slots_own_settings_the_accumulators_and_the_arpeggios_through_a_copy() {
    use vt_model::macros::{MacroMap, MacroTarget};
    let mut track = Track::default();
    track.mods[4].kind = ModKind::Steps;
    track.mods[4].steps.mutate = 10;
    let whole = |id| MacroMap::whole(MacroTarget::Setting(id));
    track.macros[0].maps = vec![
        whole(SettingId::mod_own(1, 1).unwrap()),
        whole(SettingId::mod_own(4, 7).unwrap()),
        whole(SettingId::accumulation(1).unwrap()),
        whole(SettingId::arpeggio(1).unwrap()),
    ];
    let written = track.clone();
    let state = from_the_top(&track);
    assert_eq!(state.heard_mod(&track, 1).adsr.decay_ms, 0, "at 00, none");
    assert_eq!(state.heard_mod(&track, 4).steps.mutate, 0);
    assert_eq!(state.heard_accumulation(&track).down, 1);
    assert_eq!(state.heard_arpeggio(&track).octaves, 1);
    track.macros[0].value = 0xFF00;
    let state = from_the_top(&track);
    assert_eq!(
        state.heard_mod(&track, 1).adsr.decay_ms,
        vt_model::constants::MAX_ENVELOPE_MS
    );
    assert_eq!(state.heard_mod(&track, 4).steps.mutate, 100);
    assert_eq!(state.heard_accumulation(&track).down, 28);
    assert_eq!(state.heard_arpeggio(&track).octaves, 4);
    assert_eq!(track.mods[4].steps.mutate, 10, "the song keeps its own");
    // With nothing on them, they're the song's, and a slot is read as it is.
    let mut free = written;
    free.macros[0].maps.clear();
    let state = from_the_top(&free);
    assert_eq!(state.heard_mod(&free, 4), free.mods[4]);
    assert_eq!(state.heard_accumulation(&free), free.accumulation);
    assert_eq!(state.heard_arpeggio(&free), free.arpeggio);
}

#[test]
fn a_lev_lock_sets_the_fader_and_a_note_puts_it_back() {
    let track = Track {
        level: 0xC0,
        ..Track::default()
    };
    let mut state = from_the_top(&track);
    assert_eq!(state.level.target, level_gain(f32::from(0xC0_u8)));
    assert_eq!(state.faders().at[0], 0xC000);
    assert!(!state.faders().automated[0]);
    state.set_value(SettingId::LEVEL, 0xE000, Mark::Lock);
    assert_eq!(state.level.target, 1.0, "E0 is 0 dB");
    assert_eq!(state.faders().at[0], 0xE000);
    assert!(state.faders().follows_steps(0));
    state.end_locks();
    assert_eq!(state.level.target, level_gain(f32::from(0xC0_u8)));
    assert!(
        state.faders().automated[0],
        "still the steps' until playing from the top"
    );
    state.reset_held(0, 0, &track, 0);
    assert!(!state.faders().automated[0]);
}

#[test]
fn a_lev_hold_lasts_through_a_note_until_playing_from_the_top() {
    let mut state = from_the_top(&Track::default());
    state.set_value(SettingId::LEVEL, 0x8000, Mark::Hold);
    state.end_locks();
    assert_eq!(state.faders().at[0], 0x8000, "the hold stays");
    state.reset_held(0, 0, &Track::default(), 0);
    assert_eq!(state.faders().at[0], 0xE000, "back to the set value");
}

#[test]
fn a_takeover_keeps_the_steps_hold_whichever_comes_first_of_it_and_the_hands_edit() {
    for edit_first in [false, true] {
        let mut state = from_the_top(&Track::default());
        state.set_value(SettingId::LEVEL, 0x8000, Mark::Hold);
        if edit_first {
            state.set_base(SettingId::LEVEL, 0xD000);
            state.take_over(0);
        } else {
            state.take_over(0);
            state.set_base(SettingId::LEVEL, 0xD000);
        }
        assert_eq!(state.faders().at[0], 0xD000, "the hand's");
        state.give_back(0);
        assert_eq!(state.faders().at[0], 0x8000, "the steps' hold");
    }
}

#[test]
fn vol_moves_what_is_heard_around_the_fader_and_not_the_fader() {
    let mut state = from_the_top(&Track::default());
    state.set_value(SettingId::LEVEL, 0xC000, Mark::Hold);
    state.set_value(SettingId::VOL, 0xD000, Mark::Hold);
    let faders = state.faders();
    assert_eq!(faders.at[0], 0xC000);
    assert_eq!(faders.moved[0], 0xB0, "16 steps under the fader");
    assert_eq!(state.live().level, 0xB0);
}

#[test]
fn a_hand_takes_a_fader_over_until_it_is_given_back() {
    let mut state = from_the_top(&Track::default());
    state.set_value(SettingId::PAN, 0x2000, Mark::Hold);
    assert_eq!(state.faders().at[1], 0x2000);
    state.take_over(1);
    assert_eq!(state.faders().at[1], 0x8000, "the track page's value");
    assert!(state.faders().taken[1] && !state.faders().follows_steps(1));
    state.set_base(SettingId::PAN, 0x9000);
    assert_eq!(state.faders().at[1], 0x9000, "the hand's move is heard");
    state.set_value(SettingId::PAN, 0x3000, Mark::Lock);
    assert_eq!(state.faders().at[1], 0x9000, "the steps' aren't");
    state.set_value(SettingId::SEND_A, 0x4000, Mark::Lock);
    assert_eq!(
        state.faders().at[2],
        0x4000,
        "the other faders still follow"
    );
    state.end_locks();
    state.give_back(1);
    assert_eq!(state.faders().at[1], 0x2000, "back to the hold underneath");
    assert!(state.faders().follows_steps(1));
}

#[test]
fn a_fader_given_back_after_a_lock_ends_hears_the_hands_value() {
    let mut state = from_the_top(&Track::default());
    state.set_value(SettingId::LEVEL, 0x8000, Mark::Lock);
    state.take_over(0);
    state.set_base(SettingId::LEVEL, 0xD000);
    state.end_locks();
    state.give_back(0);
    assert_eq!(state.faders().at[0], 0xD000);
    state.set_value(SettingId::LEVEL, 0x9000, Mark::Lock);
    assert_eq!(state.faders().at[0], 0x9000, "the steps drive it again");
}

#[test]
fn pan_is_exactly_centred_at_80_and_keeps_power() {
    assert_eq!(pan_position(0x80 as f32), 0.5);
    assert_eq!(pan_position(0x00 as f32), 0.0);
    assert_eq!(pan_position(0xFF as f32), 1.0);
    let (l, r) = pan_gains(0.5);
    assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
    let (l, r) = pan_gains(0.2);
    assert!((l * l + r * r - 2.0).abs() < 1e-5);
}

/// A track's state as it plays it: its values from the top, then its filter, drive, punch
/// and utility filter, which modulating takes from the track, all settled.
fn playing(track: &Track, rate: u32) -> TrackState {
    let mut state = from_the_top(track);
    let mut memory = MemoryPool::for_song(&vt_model::song::Song::new(1), rate);
    let now = SongTime {
        tick: 0.0,
        clock: 0,
        rate,
        seed: 0,
        frames_per_tick: 100.0,
        rendered: 0,
        scale: Default::default(),
    };
    state.modulate(track, 0, now, true, &mut memory);
    state.settle(rate);
    state
}

#[test]
fn a_slot_in_scale_moves_the_note_to_the_scales_nearest() {
    use vt_model::scale::{Scale, ScaleKind};
    let major = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    let semitones = |added: f32| (added * 255.0).round();
    // From C, three up is D sharp, between D and E, so the lower, D.
    assert_eq!(semitones(in_scale(3.0 / 255.0, Some(60.0), major)), 2.0);
    assert_eq!(semitones(in_scale(4.0 / 255.0, Some(60.0), major)), 4.0);
    assert_eq!(semitones(in_scale(-1.0 / 255.0, Some(60.0), major)), -1.0);
    // From E, one up is F, in the scale.
    assert_eq!(semitones(in_scale(1.0 / 255.0, Some(64.0), major)), 1.0);
    // No whole move leaves a note off the scale as written; and off or with no note it's as
    // it was.
    assert_eq!(in_scale(0.3 / 255.0, Some(61.0), major), 0.0);
    let off = Scale { on: false, ..major };
    assert_eq!(in_scale(3.0 / 255.0, Some(60.0), off), 3.0 / 255.0);
    assert_eq!(in_scale(3.0 / 255.0, None, major), 3.0 / 255.0);
}

#[test]
fn a_broken_sample_from_a_voice_is_a_frame_of_silence_to_the_filter() {
    let rate = 48_000;
    let run = RunContext::new(rate);
    let sidechains = SidechainFrames::default();
    for filter in FilterType::ALL {
        for broken in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            // Resonant and driven, with the utility filter cutting both ends, so every stage
            // with state hears it.
            let track = Track {
                filter,
                cutoff: 0x60,
                resonance: 0xC0,
                drive: 0x40,
                low_cut: 0x20,
                high_cut: 0xE0,
                ..Track::default()
            };
            let (mut hit, mut silent) = (playing(&track, rate), playing(&track, rate));
            for frame in 0..4_000 {
                let sound = (frame as f32 * 0.05).sin() * 0.5;
                let (to_hit, to_silent) = if frame == 1_000 {
                    ([broken, sound], [0.0, sound])
                } else {
                    ([sound; 2], [sound; 2])
                };
                let out = hit.shape(to_hit, run, &sidechains);
                let heard = silent.shape(to_silent, run, &sidechains);
                // Within the same frame it's as if the voice had gone silent for one.
                assert_eq!(out.dry, heard.dry, "{filter:?} {broken} at {frame}");
                assert_eq!(out.send_a, heard.send_a);
                assert!(out.dry.iter().all(|s| s.is_finite()));
            }
            assert!(
                hit.after_trim.iter().all(|s| s.is_finite()),
                "the meter too"
            );
        }
    }
}

#[test]
fn punch_is_heard_only_on_the_clean_filter_types() {
    let rate = 48_000;
    let run = RunContext::new(rate);
    let sidechains = SidechainFrames::default();
    let render = |filter, punch| {
        let track = Track {
            filter,
            cutoff: 0x20,
            resonance: 0xFF,
            punch,
            ..Track::default()
        };
        let mut state = playing(&track, rate);
        (0..4_800)
            .map(|frame| {
                let strike = if frame < 48 { 1.0 } else { 0.0 };
                state.shape([strike; 2], run, &sidechains).dry[0]
            })
            .collect::<Vec<f32>>()
    };
    for filter in FilterType::ALL {
        let (plain, punched) = (render(filter, 0), render(filter, 0xFF));
        assert_eq!(plain != punched, filter.has_punch(), "{filter:?}");
    }
}

#[test]
fn safe_is_heard_only_on_the_types_that_ring_by_themselves() {
    let rate = 48_000;
    let run = RunContext::new(rate);
    let sidechains = SidechainFrames::default();
    let render = |filter, safe| {
        let track = Track {
            filter,
            cutoff: 0x80,
            resonance: 0xFF,
            safe,
            ..Track::default()
        };
        let mut state = playing(&track, rate);
        (0..4_800)
            .map(|frame| {
                let strike = if frame < 48 { 1.0 } else { 0.0 };
                state.shape([strike; 2], run, &sidechains).dry[0]
            })
            .collect::<Vec<f32>>()
    };
    for filter in FilterType::ALL {
        let (plain, safe) = (render(filter, false), render(filter, true));
        assert_eq!(plain != safe, filter.has_safe(), "{filter:?}");
    }
}

/// A voice has room for the biggest synth, as the audio thread can't allocate, and a note
/// starting writes a whole one and may copy the last to the tail, so its size is watched. It's
/// 17,360 bytes, nearly all of it `ParticleNoise` (16,720), with `SawComb` (16,536) and `Speech`
/// (15,128) close behind and every other synth under 6 KB. In a release build a start took about
/// 1.2 µs and the copy to the tail about 1.1 µs, a twentieth of a percent of a block of 256
/// frames, so it's left; past this, a synth's long buffer wants to come out of the voice.
#[test]
fn a_voice_stays_under_twenty_kilobytes() {
    let size = size_of::<Voice>();
    assert!(size <= 20 * 1024, "{size} bytes");
}

/// A track's state is in the engine once for each track there can be. It's 131,408 bytes,
/// 34,720 of them its voice and its tail, and 2,496 its four slots, each holding the one small
/// effect it plays rather than room for all seven.
#[test]
fn a_tracks_state_stays_under_160_kilobytes() {
    let size = size_of::<TrackState>();
    assert!(size <= 160 * 1024, "{size} bytes");
}

/// A sampler's note at full, shaped by an ADSR with this attack.
fn note_with_attack(attack_ms: u16) -> NoteOn {
    use vt_model::envelope::{Adsr, Envelope};
    NoteOn {
        sample: 0,
        synth: None,
        note: 60,
        velocity: 0x7F,
        gain: 1.0,
        entry: None,
        slice: 0,
        span: None,
        cut: Cut::default(),
        accent: false,
        shape: Envelope::Adsr(Adsr {
            attack_ms,
            ..Adsr::note_shape()
        }),
        release_after: None,
    }
}

/// A note with this attack on a sample held at half, after a note that's sounded for 1,000
/// frames or after silence: its track's voice and the tail it cut, frame by frame.
fn cutting(attack_ms: u16, after_a_note: bool) -> Vec<(f32, f32)> {
    use vt_model::sample::SampleData;
    use vt_model::samples::Frames;
    let rate = 48_000;
    let data = SampleData {
        frames: vec![[0.5, 0.5]; rate as usize],
    };
    let mut state = from_the_top(&Track::default());
    if after_a_note {
        state.start(note_with_attack(1), rate);
        for _ in 0..1_000 {
            state.voice.next(Some(Frames::from(&data)));
        }
    }
    state.start(note_with_attack(attack_ms), rate);
    assert_eq!(state.tail.active(), after_a_note);
    (0..2_000)
        .map(|_| {
            let voice = state.voice.next(Some(Frames::from(&data)))[0];
            (voice, state.tail.next(Some(Frames::from(&data)))[0])
        })
        .collect()
}

#[test]
fn a_note_cutting_another_keeps_an_attack_longer_than_the_fade_as_set() {
    // 3 ms is longer than the 2.7 ms fade: the note plays its attack as set, to the bit, while
    // the note it cut falls under it (decision 368).
    let cut = cutting(3, true);
    let alone = cutting(3, false);
    for (at, ((voice, tail), (after_silence, _))) in cut.iter().zip(&alone).enumerate() {
        assert_eq!(voice.to_bits(), after_silence.to_bits(), "at {at}");
        if at >= CUT_FADE_FRAMES as usize {
            assert_eq!(*tail, 0.0, "the tail is gone by {at}");
        }
    }
    assert!(
        cut[CUT_FADE_FRAMES as usize / 2].1 > 0.3,
        "the tail falls under it"
    );
}

#[test]
fn a_note_cutting_another_rises_at_equal_power_over_a_shorter_attack() {
    // A 1 ms attack sits inside the fade: the note rises over the whole fade instead, as the one
    // it cut falls, their powers adding to the note's own (decision 368).
    let cut = cutting(1, true);
    let alone = cutting(1, false);
    for (at, (voice, tail)) in cut.iter().enumerate() {
        let power = voice * voice + tail * tail;
        assert!((power - 0.25).abs() < 1e-4, "{power} at {at}");
        if at >= CUT_FADE_FRAMES as usize {
            assert_eq!(*voice, 0.5, "at full from {at}");
        }
    }
    // Slower than the attack as set, which is at full by 48 frames.
    assert!(cut[48].0 < 0.45, "{}", cut[48].0);
    assert_eq!(alone[48].0, 0.5);
}
