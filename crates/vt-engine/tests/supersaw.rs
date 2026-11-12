//! `Supersaw` on a track: how it sounds, as a golden, and how wide it plays through the track.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

use support::golden;

const RATE: u32 = 48_000;

/// The place of one of its settings, by key.
fn place(key: &str) -> usize {
    (0..9)
        .find(|at| {
            SettingId::voice(SynthVoice::Supersaw, *at).is_some_and(|id| id.entry().key == key)
        })
        .unwrap()
}

/// A track of it playing these steps.
fn song(steps: &[String]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::Supersaw);
    let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn render(song: Song) -> Vec<f32> {
    let published = Arc::new(Published::new(
        Arc::new(song),
        Arc::new(SamplePool::default()),
    ));
    render_loop(published, RATE, RenderOptions::default())
}

fn lock(key: &str, value: u8) -> String {
    let id = SettingId::voice(SynthVoice::Supersaw, place(key)).unwrap();
    format!("{}:{value:02X}", id.key())
}

#[test]
fn it_renders_as_it_did() {
    // A low note, then the count, the stack and the shape moved while notes play.
    let steps = [
        "C-2 7F --- --- ---".to_owned(),
        format!("--- -- {} --- ---", lock("saws", 2)),
        format!("G-2 60 {} --- ---", lock("stack", 2)),
        format!("C-3 7F {} --- ---", lock("shape", 0xC0)),
    ];
    let rendered = render(song(&steps));
    assert!(rendered.iter().any(|sample| *sample != 0.0));
    golden("supersaw", &rendered);
}

#[test]
fn at_no_width_it_plays_mono_and_wide_it_plays_stereo() {
    let steps = ["C-2 7F --- --- ---".to_owned()];
    let sides = |width: u8| {
        let mut song = song(&steps);
        song.tracks[0].set_machine_value(place("width"), width);
        let mut engine = Engine::new(
            RATE,
            Arc::new(Published::new(
                Arc::new(song),
                Arc::new(SamplePool::default()),
            )),
        );
        engine.play_from(0);
        let mut out = vec![0.0; 9_600 * 2];
        engine.process(&mut out);
        let apart: f32 = out.chunks(2).map(|frame| (frame[0] - frame[1]).abs()).sum();
        let loud: f32 = out.iter().map(|sample| sample.abs()).sum();
        (apart, loud)
    };
    let (apart, loud) = sides(0x00);
    assert!(loud > 0.0);
    assert_eq!(apart, 0.0);
    let (apart, loud) = sides(0xFF);
    assert!(apart > 0.1 * loud, "{apart} {loud}");
}
