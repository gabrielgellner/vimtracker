//! Tracks playing the synth machines.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems};
use vt_model::lfo::{Lfo, Rate};
use vt_model::modulation::ModSlot;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{FULL_GATE, Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
const STEP: usize = 6000;

/// A synth track playing these steps.
fn song(steps: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::default());
    let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(
        Arc::new(song),
        Arc::new(SamplePool::default()),
    ))
}

fn play(song: Song, steps: usize) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; steps * STEP * 2];
    engine.process(&mut out);
    out
}

/// The loudest part of a step, on the left channel.
fn peak(out: &[f32], step: usize) -> f32 {
    out[step * STEP * 2..(step + 1) * STEP * 2]
        .iter()
        .step_by(2)
        .fold(0.0, |peak: f32, sample| peak.max(sample.abs()))
}

/// How loud a step is on average, on the left channel.
fn rms(out: &[f32], step: usize) -> f32 {
    let left: Vec<f32> = out[step * STEP * 2..(step + 1) * STEP * 2]
        .iter()
        .step_by(2)
        .copied()
        .collect();
    (left.iter().map(|sample| sample * sample).sum::<f32>() / left.len() as f32).sqrt()
}

const NOTE: &str = "C-1 7F --- --- ---";
const REST: &str = "--- -- --- --- ---";

#[test]
fn it_sounds_with_no_sample_and_holds_until_the_next_note() {
    let out = play(song(&[NOTE, REST, REST, REST]), 4);
    for step in 0..4 {
        assert!(peak(&out, step) > 0.1, "step {step}: {}", peak(&out, step));
    }
}

#[test]
fn a_sample_track_with_no_sample_is_still_silent() {
    let mut silent = song(&[NOTE, REST]);
    silent.tracks[0].machine = Machine::Sampler;
    assert!(play(silent, 2).iter().all(|sample| *sample == 0.0));
}

#[test]
fn off_releases_it_and_kil_cuts_it() {
    let off = play(song(&[NOTE, "--- -- off:00 --- ---", REST, REST]), 4);
    assert!(peak(&off, 0) > 0.1);
    assert!(peak(&off, 3) < 0.001, "{}", peak(&off, 3));
    let kil = play(song(&[NOTE, "--- -- kil:00 --- ---", REST, REST]), 4);
    assert!(peak(&kil, 3) < 0.001, "{}", peak(&kil, 3));
}

#[test]
fn it_goes_through_the_track_filter() {
    let open = play(song(&[NOTE, REST]), 2);
    let mut closed = song(&[NOTE, REST]);
    closed.tracks[0].cutoff = 0;
    let closed = play(closed, 2);
    assert!(peak(&closed, 1) < peak(&open, 1) * 0.5);
}

#[test]
fn the_page_values_change_the_sound() {
    let plain = play(song(&[NOTE, REST]), 2);
    let change: [fn(&mut Song); 3] = [
        |song| song.tracks[0].set_machine_value(0, 0xC0),
        |song| song.tracks[0].set_machine_value(1, 0x60),
        |song| song.tracks[0].set_machine_value(2, 0x00),
    ];
    for change in change {
        let mut changed = song(&[NOTE, REST]);
        change(&mut changed);
        assert_ne!(play(changed, 2), plain);
    }
}

/// A step locking one of a voice's settings, by its place among them.
fn lock(voice: SynthVoice, setting: usize, value: &str) -> String {
    let key = SettingId::voice(voice, setting).unwrap().key();
    format!("C-1 7F {key}:{value} --- ---")
}

#[test]
fn step_commands_lock_its_settings() {
    for setting in 0..3 {
        let locked = lock(SynthVoice::VirtualAnalog, setting, "40");
        let out = play(song(&[&locked, REST]), 2);
        assert_ne!(out, play(song(&[NOTE, REST]), 2), "{locked}");
    }
}

#[test]
fn a_lock_of_another_voices_setting_does_nothing() {
    let plain = play(song(&[NOTE, REST]), 2);
    let kick = lock(SynthVoice::Kick, 0, "F0");
    assert_eq!(play(song(&[&kick, REST]), 2), plain, "the track plays VA");
}

#[test]
fn an_lfo_moves_its_settings() {
    for destination in (0..3).filter_map(|at| SettingId::voice(SynthVoice::VirtualAnalog, at)) {
        let mut moving = song(&[NOTE, REST, REST, REST]);
        moving.tracks[0].mods[3] = ModSlot::lfo(
            Some(destination),
            Lfo {
                rate: Rate::Hertz(4000),
                ..Lfo::default()
            },
        )
        .with_depth(0x40);
        assert_ne!(
            play(moving, 4),
            play(song(&[NOTE, REST, REST, REST]), 4),
            "{destination:?}"
        );
    }
}

#[test]
fn notes_change_its_pitch() {
    let low = play(song(&[NOTE]), 2);
    let high = play(song(&["C-3 7F --- --- ---"]), 2);
    let crossings = |out: &[f32]| {
        out.chunks(2)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0][0] < 0.0 && pair[1][0] >= 0.0)
            .count()
    };
    let (low, high) = (crossings(&low), crossings(&high));
    assert!(high > low * 3, "{low} against {high}");
}

#[test]
fn renders_and_stems_are_the_same_each_time() {
    let whole = song(&[NOTE, REST, REST, REST]);
    let render = || render_loop(version(whole.clone()), RATE, RenderOptions::default());
    assert_eq!(render(), render());
    assert!(render().iter().any(|sample| *sample != 0.0));
    let stems = render_stems(&version(whole.clone()), RATE, RenderOptions::default());
    assert_eq!(stems.len(), 1);
    assert!(stems[0].samples.iter().any(|sample| *sample != 0.0));
}

#[test]
fn a_preview_plays_with_the_tracks_settings() {
    let mut engine = Engine::new(RATE, version(song(&[REST])));
    engine.preview(0, 48, 0x7F);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert!(out.iter().any(|sample| *sample != 0.0));
}

/// A song of one voice.
fn on(voice: SynthVoice, steps: &[&str]) -> Song {
    let mut song = song(steps);
    song.tracks[0].machine = Machine::Plaits(voice);
    song
}

#[test]
fn every_synth_sounds_with_no_sample_and_goes_through_its_settings() {
    for voice in SynthVoice::ALL {
        // With some of its second setting, as FM with none is a sine whatever else is set.
        let with_some = |steps: &[&str]| {
            let mut song = on(voice, steps);
            song.tracks[0].set_machine_value(1, 0x40);
            song
        };
        let out = play(with_some(&[NOTE, REST]), 2);
        assert!(peak(&out, 0) > 0.05, "{voice:?}: {}", peak(&out, 0));
        assert!(peak(&out, 0) < 1.5, "{voice:?}: {}", peak(&out, 0));
        for setting in 0..3 {
            // A list, like 6-op's patches or the chords, takes a place half way along it.
            let id = SettingId::voice(voice, setting).unwrap();
            let value = match id.entry().kind {
                vt_model::settings::Kind::Stepped { .. } => {
                    format!("{:02X}", id.entry().kind.top().div_ceil(2))
                }
                _ => "F0".to_owned(),
            };
            let locked = lock(voice, setting, &value);
            let changed = play(with_some(&[&locked, REST]), 2);
            assert!(changed != out, "{voice:?} {setting}");
        }
    }
}

#[test]
fn accent_changes_how_a_struck_voice_sounds_and_nothing_else() {
    for voice in SynthVoice::ALL {
        if SettingId::voice(voice, 3).is_none() {
            continue;
        }
        let soft = play(on(voice, &[&lock(voice, 3, "20"), REST]), 2);
        let hard = play(on(voice, &[&lock(voice, 3, "FF"), REST]), 2);
        assert_ne!(soft, hard, "{voice:?}");
    }
    // The track's own accent does the same as a step's.
    let mut set = on(SynthVoice::Modal, &[NOTE, REST]);
    set.tracks[0].set_machine_value(3, 0xFF);
    let locked = on(
        SynthVoice::Modal,
        &[&lock(SynthVoice::Modal, 3, "FF"), REST],
    );
    assert_eq!(play(set, 2), play(locked, 2));
}

#[test]
fn a_harder_strike_is_brighter_on_a_string() {
    let roughness = |out: &[f32]| {
        let total: f32 = out
            .chunks(2)
            .collect::<Vec<_>>()
            .windows(2)
            .map(|pair| (pair[1][0] - pair[0][0]).powi(2))
            .sum();
        let energy: f32 = out.iter().step_by(2).map(|s| s * s).sum();
        total / energy
    };
    let soft = play(
        on(
            SynthVoice::String,
            &[&lock(SynthVoice::String, 3, "00"), REST],
        ),
        2,
    );
    let hard = play(
        on(
            SynthVoice::String,
            &[&lock(SynthVoice::String, 3, "FF"), REST],
        ),
        2,
    );
    assert!(
        roughness(&hard) > roughness(&soft),
        "{} {}",
        roughness(&hard),
        roughness(&soft)
    );
}

#[test]
fn a_kick_dies_away_while_fm_holds() {
    let steps = [NOTE, REST, REST, REST, REST, REST, REST, REST];
    let mut kick = on(SynthVoice::Kick, &steps);
    kick.tracks[0].set_machine_value(2, 0x20);
    let out = play(kick, 8);
    assert!(peak(&out, 7) < peak(&out, 0) * 0.05, "{}", peak(&out, 7));
    let out = play(on(SynthVoice::Fm, &steps), 8);
    assert!(peak(&out, 7) > peak(&out, 0) * 0.5, "{}", peak(&out, 7));
}

#[test]
fn a_new_note_hits_a_kick_again() {
    let steps = [NOTE, REST, REST, REST, NOTE, REST, REST, REST];
    let mut kick = on(SynthVoice::Kick, &steps);
    kick.tracks[0].set_machine_value(2, 0x20);
    let out = play(kick, 8);
    assert!(peak(&out, 4) > peak(&out, 3) * 2.0);
}

#[test]
fn a_tailed_render_releases_a_held_note_rather_than_running_on() {
    for voice in SynthVoice::ALL.into_iter().filter(|voice| voice.holds()) {
        let held = on(voice, &[NOTE, REST]);
        let options = RenderOptions {
            tail: true,
            ..RenderOptions::default()
        };
        let frames = render_loop(version(held), RATE, options).len() / 2;
        assert!(
            frames < RATE as usize * 2,
            "{voice:?}: {} seconds",
            frames as f32 / RATE as f32
        );
    }
}

#[test]
fn the_drums_die_away_by_themselves() {
    let steps = [NOTE, REST, REST, REST, REST, REST, REST, REST];
    for voice in SynthVoice::ALL.into_iter().filter(|voice| !voice.holds()) {
        let mut drum = on(voice, &steps);
        drum.tracks[0].set_machine_value(2, 0x20);
        let out = play(drum, 8);
        assert!(
            peak(&out, 7) < peak(&out, 0) * 0.05,
            "{voice:?}: {} against {}",
            peak(&out, 7),
            peak(&out, 0)
        );
    }
}

#[test]
fn each_snare_hit_has_its_own_noise_and_a_render_repeats() {
    let mut snare = on(
        SynthVoice::Snare,
        &[NOTE, REST, REST, REST, NOTE, REST, REST, REST],
    );
    snare.tracks[0].set_machine_value(0, 0xC0);
    let out = play(snare.clone(), 8);
    let first = &out[..STEP * 2];
    let second = &out[4 * STEP * 2..5 * STEP * 2];
    assert!(first != second, "two hits the same");
    assert!(play(snare.clone(), 8) == out, "the same seed differs");
    snare.seed = 7;
    assert!(play(snare, 8) != out, "another seed is the same");
}

/// How many times a stretch of the left channel rises through zero, a second's worth.
fn pitch_of(out: &[f32], from: usize, to: usize) -> f32 {
    let left: Vec<f32> = out[from * STEP * 2..to * STEP * 2]
        .iter()
        .step_by(2)
        .copied()
        .collect();
    let rising = left
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    rising as f32 * RATE as f32 / left.len() as f32
}

#[test]
fn acid_ignores_velocity_but_for_the_accent() {
    // As on a 303, every plain note sounds the same whatever its velocity, and so does every
    // accent (decision 121). The voice's `Accent` starts at noon, where an accent is about 4 dB
    // louder over its first step, as Acid V's is (decision 124).
    let at = |velocity: &str| {
        let note = format!("C-1 {velocity}");
        play(on(SynthVoice::Acid, &[&note, REST]), 2)
    };
    let plain = at("10");
    for velocity in ["40", "64", "6F"] {
        assert!(at(velocity) == plain, "a plain note at {velocity} differs");
    }
    let accented = at("70");
    assert!(
        at("7F") == accented,
        "an accent at 7F differs from one at 70"
    );
    // Measured on average, as a resonant wave's peaks come and go with its filter.
    let louder = 20.0 * (rms(&accented, 0) / rms(&plain, 0)).log10();
    assert!(louder > 3.0 && louder < 6.0, "{louder} dB");
}

#[test]
fn every_other_voice_is_as_loud_as_its_velocity() {
    // A note's level is its velocity's share of the most, on every voice but `Acid`.
    for voice in SynthVoice::ALL {
        if voice == SynthVoice::Acid {
            continue;
        }
        let at = |velocity: &str| {
            let note = format!("C-1 {velocity}");
            play(on(voice, &[&note, REST]), 2)
        };
        let (soft, full) = (at("40"), at("7F"));
        let ratio = rms(&soft, 0) / rms(&full, 0);
        let expected = f32::from(0x40_u8) / f32::from(0x7F_u8);
        assert!(
            (ratio / expected - 1.0).abs() < 0.01,
            "{voice:?}: {ratio} against {expected}"
        );
    }
}

/// How loud each of a step's six units is on average, on the left channel.
fn unit_levels(out: &[f32]) -> Vec<f32> {
    let unit = STEP / 6;
    out[..STEP * 2]
        .chunks(unit * 2)
        .map(|part| {
            let left = part.iter().step_by(2);
            (left.map(|sample| sample * sample).sum::<f32>() / unit as f32).sqrt()
        })
        .collect()
}

#[test]
fn acid_retriggers_every_hit_at_the_notes_level_while_other_voices_fade() {
    // `RET` every two units, steady, fading by half and swelling by 60% a hit. As on a 303, an
    // `Acid` note has no level but its accent, so each hit plays at the note's own, plain or
    // accented, and the fade and swell change nothing (decision 124).
    let with = |voice, velocity: &str, retrigger: &str| {
        let note = format!("C-1 {velocity} ret:{retrigger}");
        play(on(voice, &[&note, REST]), 2)
    };
    for velocity in ["64", "7F"] {
        let steady = with(SynthVoice::Acid, velocity, "02");
        for retrigger in ["52", "D2"] {
            assert!(
                with(SynthVoice::Acid, velocity, retrigger) == steady,
                "ret:{retrigger} changed an Acid note at {velocity}"
            );
        }
    }
    // Every hit is still a new note: the accent carries on to each, so an accented note's last
    // hit is as much louder than a plain one's as its first.
    let plain = unit_levels(&with(SynthVoice::Acid, "64", "02"));
    let accented = unit_levels(&with(SynthVoice::Acid, "7F", "02"));
    for unit in [0, 4] {
        assert!(
            accented[unit] > plain[unit] * 1.3,
            "unit {unit}: {} against {}",
            accented[unit],
            plain[unit]
        );
    }
    // Another voice still fades and swells.
    let steady = unit_levels(&with(SynthVoice::VirtualAnalog, "64", "02"));
    let fading = unit_levels(&with(SynthVoice::VirtualAnalog, "64", "52"));
    let swelling = unit_levels(&with(SynthVoice::VirtualAnalog, "40", "D2"));
    let swelling_steady = unit_levels(&with(SynthVoice::VirtualAnalog, "40", "02"));
    assert!(
        fading[4] < steady[4] * 0.5,
        "{} against {}",
        fading[4],
        steady[4]
    );
    assert!(
        swelling[4] > swelling_steady[4] * 1.5,
        "{} against {}",
        swelling[4],
        swelling_steady[4]
    );
}

#[test]
fn acid_slides_into_the_note_after_a_hold() {
    // Held to the next note, so the second note's pitch can be heard over the rows after it.
    let held = |rows: &[&str]| {
        let mut song = on(SynthVoice::Acid, rows);
        song.tracks[0].gate_length = Some(FULL_GATE);
        play(song, 4)
    };
    let slid = held(&["C-1 64 HLD1", "C-2 64", REST, REST]);
    let struck = held(&["C-1 64", "C-2 64", REST, REST]);
    // Both end on the second note's pitch.
    for out in [&slid, &struck] {
        let hertz = pitch_of(out, 2, 4);
        assert!((hertz - 130.8).abs() < 6.0, "{hertz} Hz");
    }
    // Struck again, its envelopes start over, so it's louder and brighter at the second note than
    // when it slides there.
    assert!(
        rms(&struck, 1) > rms(&slid, 1) * 1.1,
        "{} {}",
        rms(&struck, 1),
        rms(&slid, 1)
    );
}

#[test]
fn acid_carries_its_accent_sweep_from_note_to_note() {
    // Accents in a row, at full resonance, swell more than a lone one does. Played with the
    // track's resonance, and the voice's own up.
    let mut steps = vec!["C-1 7F"; 4];
    steps.push(REST);
    let mut song = on(SynthVoice::Acid, &steps);
    song.tracks[0].set_machine_value(2, 0xFF);
    let out = play(song, 5);
    assert!(
        peak(&out, 3) > peak(&out, 0),
        "{} {}",
        peak(&out, 3),
        peak(&out, 0)
    );
}

#[test]
fn acid_plays_one_held_note_at_a_time_and_slides_between_overlapping_keys() {
    // With no resonance, so a low note's resonant peak at its second harmonic doesn't add zero
    // crossings to count.
    let mut song = on(SynthVoice::Acid, &[REST]);
    song.tracks[0].set_machine_value(2, 0x00);
    let mut engine = Engine::new(RATE, version(song));
    let listen = |engine: &mut Engine, steps: usize| {
        let mut out = vec![0.0; steps * STEP * 2];
        engine.process(&mut out);
        out
    };
    engine.hold_preview(0, 36, 0x64);
    let low = pitch_of(&listen(&mut engine, 4), 1, 4);
    // A key down while the first is held slides there on the same voice.
    engine.hold_preview(0, 48, 0x64);
    assert_eq!(engine.previews_sounding(), 1);
    let high = pitch_of(&listen(&mut engine, 4), 1, 4);
    assert!(
        (high / low - 2.0).abs() < 0.1,
        "an octave up: {high} Hz from {low} Hz"
    );
    // Letting go of the newest slides back to the one still down.
    engine.release_preview(0, 48);
    let back = pitch_of(&listen(&mut engine, 4), 1, 4);
    assert!((back / low - 1.0).abs() < 0.05, "{back} Hz, not {low} Hz");
    // A key let go under the one playing is forgotten, so letting go of it ends the note.
    engine.hold_preview(0, 48, 0x64);
    engine.release_preview(0, 36);
    let still = listen(&mut engine, 2);
    assert!(peak(&still, 1) > 0.01, "the newest still sounds");
    engine.release_preview(0, 48);
    let ended = listen(&mut engine, 8);
    assert!(peak(&ended, 7) < 1e-3, "{}", peak(&ended, 7));
}

#[test]
fn every_other_synth_plays_a_voice_for_each_held_key() {
    for voice in SynthVoice::ALL
        .into_iter()
        .filter(|voice| *voice != SynthVoice::Acid)
    {
        let mut engine = Engine::new(RATE, version(on(voice, &[REST])));
        engine.hold_preview(0, 48, 0x64);
        engine.hold_preview(0, 52, 0x64);
        engine.hold_preview(0, 55, 0x64);
        let mut out = vec![0.0; 256];
        engine.process(&mut out);
        assert_eq!(engine.previews_sounding(), 3, "{voice:?}");
    }
}

/// The voices with a `Drift`, and its place among each one's settings.
fn drifting() -> Vec<(SynthVoice, usize)> {
    SynthVoice::ALL
        .into_iter()
        .filter_map(|voice| {
            let place = (0..9).find(|at| {
                SettingId::voice(voice, *at).is_some_and(|id| id.entry().key == "drift")
            })?;
            Some((voice, place))
        })
        .collect()
}

#[test]
fn the_analog_voices_have_a_drift() {
    let voices: Vec<SynthVoice> = drifting().into_iter().map(|(voice, _)| voice).collect();
    assert_eq!(
        voices,
        [
            SynthVoice::VirtualAnalog,
            SynthVoice::VirtualAnalogVcf,
            SynthVoice::Acid,
            SynthVoice::Chiptune,
        ]
    );
}

#[test]
fn drift_makes_each_note_differ_and_a_render_repeats() {
    // Acid's notes carry on from each other, so they're long enough apart to start afresh.
    let steps = [NOTE, REST, REST, REST, REST, REST, REST, REST, NOTE, REST];
    for (voice, place) in drifting() {
        let mut drifts = on(voice, &steps);
        drifts.tracks[0].set_machine_value(place, 0xFF);
        let out = play(drifts.clone(), 10);
        let first = &out[..STEP * 2];
        let second = &out[8 * STEP * 2..9 * STEP * 2];
        assert!(first != second, "{voice:?}: two notes the same");
        assert!(
            play(drifts.clone(), 10) == out,
            "{voice:?}: the same seed differs"
        );
        drifts.seed = 7;
        assert!(
            play(drifts, 10) != out,
            "{voice:?}: another seed is the same"
        );
    }
}
