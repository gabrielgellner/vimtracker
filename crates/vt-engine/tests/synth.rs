//! Tracks playing the synth machines.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems};
use vt_model::lfo::{Lfo, Rate};
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::target::Target;

const RATE: u32 = 48_000;
const STEP: usize = 6000;

/// A synth track playing these steps.
fn song(steps: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits;
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

const NOTE: &str = "C-2 7F ---00 ---00 ---00";
const REST: &str = "--- -- ---00 ---00 ---00";

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
    silent.tracks[0].machine = Machine::Sample;
    assert!(play(silent, 2).iter().all(|sample| *sample == 0.0));
}

#[test]
fn off_releases_it_and_kil_cuts_it() {
    let off = play(song(&[NOTE, "--- -- OFF00 ---00 ---00", REST, REST]), 4);
    assert!(peak(&off, 0) > 0.1);
    assert!(peak(&off, 3) < 0.001, "{}", peak(&off, 3));
    let kil = play(song(&[NOTE, "--- -- KIL00 ---00 ---00", REST, REST]), 4);
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
        |song| song.tracks[0].harmonics = 0xC0,
        |song| song.tracks[0].timbre = 0x60,
        |song| song.tracks[0].morph = 0x00,
    ];
    for change in change {
        let mut changed = song(&[NOTE, REST]);
        change(&mut changed);
        assert_ne!(play(changed, 2), plain);
    }
}

#[test]
fn step_commands_lock_its_settings() {
    for command in ["HAR", "TIM", "MOR"] {
        let locked = format!("C-2 7F {command}40 ---00 ---00");
        let out = play(song(&[&locked, REST]), 2);
        assert_ne!(out, play(song(&[NOTE, REST]), 2), "{command}");
    }
}

#[test]
fn an_lfo_moves_its_settings() {
    for destination in [Target::Har, Target::Tim, Target::Mor] {
        let mut moving = song(&[NOTE, REST, REST, REST]);
        moving.tracks[0].lfos[0] = Lfo {
            destination: Some(destination),
            rate: Rate::Hertz(4000),
            depth: 0x40,
            ..Lfo::default()
        };
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
    let high = play(song(&["C-4 7F ---00 ---00 ---00"]), 2);
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
    song.tracks[0].synth_voice = voice;
    song
}

#[test]
fn every_synth_sounds_with_no_sample_and_goes_through_its_settings() {
    for voice in SynthVoice::ALL {
        // With some of its second setting, as FM with none is a sine whatever else is set.
        let with_some = |steps: &[&str]| {
            let mut song = on(voice, steps);
            song.tracks[0].timbre = 0x40;
            song
        };
        let out = play(with_some(&[NOTE, REST]), 2);
        assert!(peak(&out, 0) > 0.05, "{voice:?}: {}", peak(&out, 0));
        assert!(peak(&out, 0) < 1.5, "{voice:?}: {}", peak(&out, 0));
        for command in ["HAR", "TIM", "MOR"] {
            // 6-op's Harm is a patch number, so past its last patch it plays the same one.
            let value = if voice == SynthVoice::SixOp && command == "HAR" {
                "20"
            } else {
                "F0"
            };
            let locked = format!("C-2 7F {command}{value} ---00 ---00");
            let changed = play(with_some(&[&locked, REST]), 2);
            assert!(changed != out, "{voice:?} {command}");
        }
    }
}

#[test]
fn accent_changes_how_a_struck_voice_sounds_and_nothing_else() {
    for voice in SynthVoice::ALL {
        let soft = play(on(voice, &["C-3 7F ACC20 ---00 ---00", REST]), 2);
        let hard = play(on(voice, &["C-3 7F ACCFF ---00 ---00", REST]), 2);
        assert_eq!(
            soft != hard,
            voice.uses_accent(),
            "{voice:?}: does accent change it?"
        );
    }
    // The track's own accent does the same as a step's.
    let mut set = on(SynthVoice::Modal, &[NOTE, REST]);
    set.tracks[0].accent = 0xFF;
    let locked = on(SynthVoice::Modal, &["C-2 7F ACCFF ---00 ---00", REST]);
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
        on(SynthVoice::String, &["C-3 7F ACC00 ---00 ---00", REST]),
        2,
    );
    let hard = play(
        on(SynthVoice::String, &["C-3 7F ACCFF ---00 ---00", REST]),
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
    kick.tracks[0].morph = 0x20;
    let out = play(kick, 8);
    assert!(peak(&out, 7) < peak(&out, 0) * 0.05, "{}", peak(&out, 7));
    let out = play(on(SynthVoice::Fm, &steps), 8);
    assert!(peak(&out, 7) > peak(&out, 0) * 0.5, "{}", peak(&out, 7));
}

#[test]
fn a_new_note_hits_a_kick_again() {
    let steps = [NOTE, REST, REST, REST, NOTE, REST, REST, REST];
    let mut kick = on(SynthVoice::Kick, &steps);
    kick.tracks[0].morph = 0x20;
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
        drum.tracks[0].morph = 0x20;
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
    snare.tracks[0].harmonics = 0xC0;
    let out = play(snare.clone(), 8);
    let first = &out[..STEP * 2];
    let second = &out[4 * STEP * 2..5 * STEP * 2];
    assert!(first != second, "two hits the same");
    assert!(play(snare.clone(), 8) == out, "the same seed differs");
    snare.seed = 7;
    assert!(play(snare, 8) != out, "another seed is the same");
}
