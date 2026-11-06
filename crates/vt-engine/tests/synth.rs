//! A track playing the virtual analog machine.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems};
use vt_model::lfo::{Destination, Lfo, Rate};
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song};

const RATE: u32 = 48_000;
const STEP: usize = 6000;

/// A synth track playing these steps.
fn song(steps: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::VirtualAnalog;
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
    for destination in [Destination::Har, Destination::Tim, Destination::Mor] {
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
