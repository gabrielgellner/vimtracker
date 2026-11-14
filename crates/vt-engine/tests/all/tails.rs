//! After a sound, everything falls to an exact silence and the mix rests again (#260). Left to
//! itself, a filter's state fell into the denormals and traded the smallest floats between its
//! states for good, so a delay or flanger after a resonant filter rang for ever once playing
//! stopped, and an EQ on the master never let it rest.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::constants::MAX_GROUPS;
use vt_model::insert::{Chorus, Delay, Distort, DistortType, Equaliser, Flanger, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{FilterType, Machine, Song};

const RATE: u32 = 48_000;
const BLOCK: usize = 500;
/// The master's place among the fixed tracks, for `Engine::bus_rested`.
const MASTER_BUS: usize = MAX_GROUPS + 2;

fn pool() -> Arc<SamplePool> {
    let frames = (0..9600)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let value = (t * 110.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 9600.0);
            [value * 0.9, value * 0.7]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("tone", SampleData { frames });
    Arc::new(pool)
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), pool()))
}

/// A note on each of the first `tracks`, on the first and ninth steps.
fn notes(song: &mut Song, tracks: usize) {
    for track in song.tracks.iter_mut().take(tracks) {
        let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
        for step in [0, 8] {
            *phrase.step_mut(step).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
        }
    }
}

/// Play for `seconds`, stop, and say how long after stopping the master rested for a whole
/// block, giving exact silence, within `within` seconds, or none. It can rest part way through a
/// block, once the last tail falls quiet.
fn rests_after_playing(song: Song, seconds: f32, within: f32) -> Option<f32> {
    let mut engine = Engine::new(RATE, version(song));
    let mut out = vec![0.0; BLOCK * 2];
    engine.play_from(0);
    for _ in 0..(seconds * RATE as f32) as usize / BLOCK {
        engine.process(&mut out);
    }
    engine.pause_or_resume();
    let mut rested = false;
    for block in 0..(within * RATE as f32) as usize / BLOCK {
        engine.process(&mut out);
        if rested && engine.bus_rested(MASTER_BUS) {
            assert!(
                out.iter().all(|&sample| sample == 0.0),
                "it rests on silence"
            );
            return Some((block * BLOCK) as f32 / RATE as f32);
        }
        rested = engine.bus_rested(MASTER_BUS);
    }
    None
}

#[test]
fn resonant_filters_into_a_delay_and_a_flanger_ring_out() {
    // As the author's `sweet-groove` has them: a resonant low-pass into a delay, an MS-20
    // low-pass into a flanger and a chorus, and an EQ on the master. Each rang for good.
    let mut song = Song::new(2);
    for track in 0..2 {
        song.sample_on(track, "tone");
        song.tracks[track].cutoff = 0x60;
        song.tracks[track].resonance = 0xC0;
    }
    song.tracks[0].filter = FilterType::LowPass;
    song.tracks[0].inserts[0] = Insert::Delay(Delay::DEFAULT);
    song.tracks[0].inserts[1] = Insert::Distort(Distort {
        kind: DistortType::Soft,
        ..Distort::DEFAULT
    });
    song.tracks[1].filter = FilterType::Ms20LowPass;
    song.tracks[1].drive = 0x40;
    song.tracks[1].inserts[0] = Insert::Flanger(Flanger::DEFAULT);
    song.tracks[1].inserts[1] = Insert::Chorus(Chorus::DEFAULT);
    let master = song.tracks.len() - 1;
    song.tracks[master].inserts[0] = Insert::Eq(Equaliser {
        low: 0xC0,
        mid: 0xC0,
        ..Equaliser::DEFAULT
    });
    notes(&mut song, 2);
    let rested = rests_after_playing(song, 2.0, 60.0);
    assert!(rested.is_some(), "the mix never rested");
}

#[test]
fn every_voice_rings_out() {
    // Each machine, through a resonant filter, played and stopped: it falls silent and the mix
    // rests again.
    for machine in Machine::ALL {
        let mut song = Song::new(1);
        song.sample_on(0, "tone");
        song.tracks[0].machine = machine;
        song.tracks[0].cutoff = 0x80;
        song.tracks[0].resonance = 0xA0;
        notes(&mut song, 1);
        let rested = rests_after_playing(song, 0.3, 20.0);
        assert!(rested.is_some(), "{machine:?} still sounds after 20 s");
    }
}
