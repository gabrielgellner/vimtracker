//! The voices from Braids, each rendered as a golden, so any change to how one sounds is caught.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::samples::SamplePool;
use vt_model::settings::{SettingId, voice_key};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

use support::golden;

const RATE: u32 = 48_000;

/// A short line on one voice: a low note, a high one with its first setting moved, and one with
/// its second moved, so each setting's path is in the golden.
fn song(voice: SynthVoice) -> Song {
    let key = |at: usize| SettingId::voice(voice, at).unwrap().key();
    let steps = [
        "C-1 7F --- --- ---".to_owned(),
        "--- -- --- --- ---".to_owned(),
        format!("G-2 60 {}:{:02X} --- ---", key(0), {
            let entry = SettingId::voice(voice, 0).unwrap().entry();
            entry.kind.top().div_ceil(2)
        }),
        format!("C-2 7F {}:E0 --- ---", key(1)),
    ];
    let mut song = Song::new(1);
    song.limiter = false;
    song.tracks[0].machine = Machine::Plaits(voice);
    let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

#[test]
fn each_macro_voice_renders_as_it_did() {
    for voice in SynthVoice::ALL.into_iter().filter(|voice| voice.is_macro()) {
        let published = Arc::new(Published::new(
            Arc::new(song(voice)),
            Arc::new(SamplePool::default()),
        ));
        let rendered = render_loop(published, RATE, RenderOptions::default());
        assert!(rendered.iter().any(|sample| *sample != 0.0), "{voice:?}");
        golden(&format!("macro-{}", voice_key(voice)), &rendered);
    }
}
