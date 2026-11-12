//! `4-op` on a track: how it sounds, as a golden.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::samples::SamplePool;
use vt_model::settings::{MAX_MACHINE_SETTINGS, SettingId};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

use support::golden;

const RATE: u32 = 48_000;

/// A lock of one of its settings, by key.
fn lock(key: &str, value: u8) -> String {
    let id = (0..MAX_MACHINE_SETTINGS)
        .filter_map(|at| SettingId::voice(SynthVoice::FourOp, at))
        .find(|id| id.entry().key == key)
        .unwrap();
    format!("{}:{value:02X}", id.key())
}

fn render(steps: &[String]) -> Vec<f32> {
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::FourOp);
    let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let published = Arc::new(Published::new(
        Arc::new(song),
        Arc::new(SamplePool::default()),
    ));
    render_loop(published, RATE, RenderOptions::default())
}

#[test]
fn it_renders_as_it_did() {
    // A low note, then the algorithm, a modulator's level and wave, the feedback and a warp
    // moved while notes play.
    let steps = [
        format!("C-2 7F {} --- ---", lock("level2", 0xC0)),
        format!(
            "--- -- {} {} ---",
            lock("algorithm", 7),
            lock("level3", 0x90)
        ),
        format!("G-2 60 {} {} ---", lock("wave2", 4), lock("feedback", 0xE0)),
        format!("C-3 7F {} {} ---", lock("warp1", 1), lock("amount1", 0xC0)),
    ];
    let rendered = render(&steps);
    assert!(rendered.iter().any(|sample| *sample != 0.0));
    golden("four-op", &rendered);
}
