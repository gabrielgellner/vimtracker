//! A track at rest, with nothing moving and nothing to hear, isn't worked out again until
//! something changes (#547): the song sounds exactly as it does when every track is worked out
//! every time, stopped or playing, through edits, previews, auditions and tails ringing out.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::constants::MAX_GROUPS;
use vt_model::insert::{
    Compressor, Delay, Distort, DistortType, Insert, Plate, Reverb, Space, Spring, Stutter,
};
use vt_model::lfo::Lfo;
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Not a multiple of the 32 frames between workings out, so things happen between them too.
const BLOCK: usize = 500;
const SOUNDS: usize = 5;
/// Where the fixed tracks are, after the tracks that play and the groups.
const GROUP: usize = SOUNDS;
const RETURN_A: usize = SOUNDS + MAX_GROUPS;
const MASTER: usize = RETURN_A + 2;

fn pool() -> Arc<SamplePool> {
    let frames = (0..9600)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let value = (t * 220.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 9600.0);
            [value * 0.7, value * 0.6]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("tone", SampleData { frames });
    Arc::new(pool)
}

/// Five tracks through effects that ring, keep time or follow the note, one with an LFO on its
/// filter, two in a group with a compressor, sending to a return with a reverb, into a master
/// with a compressor.
fn song() -> Song {
    let mut song = Song::new(SOUNDS);
    for track in 0..SOUNDS {
        song.sample_on(track, "tone");
        song.tracks[track].send_a = 0x60;
    }
    song.tracks[0].inserts[0] = Insert::Reverb(Reverb::DEFAULT);
    song.tracks[0].inserts[1] = Insert::Compressor(Compressor::DEFAULT);
    song.tracks[1].inserts[0] = Insert::Plate(Plate::DEFAULT);
    song.tracks[1].inserts[1] = Insert::Delay(Delay::DEFAULT);
    song.tracks[1].inserts[2] = Insert::Stutter(Stutter::DEFAULT);
    song.tracks[2].machine = Machine::Plaits(SynthVoice::Acid);
    song.tracks[2].mods[3] = ModSlot::lfo(Some(SettingId::CUT), Lfo::default()).with_depth(0x40);
    song.tracks[3].inserts[0] = Insert::Spring(Spring::DEFAULT);
    song.tracks[3].inserts[1] = Insert::Distort(Distort {
        kind: DistortType::Overdrive,
        ..Distort::DEFAULT
    });
    song.tracks[3].group = Some(0);
    song.tracks[4].group = Some(0);
    song.tracks[GROUP].inserts[0] = Insert::Compressor(Compressor::DEFAULT);
    song.tracks[RETURN_A].inserts[0] = Insert::Space(Space::DEFAULT);
    song.tracks[MASTER].inserts[0] = Insert::Compressor(Compressor::DEFAULT);
    for (track, steps) in [
        (0, [0, 8]),
        (1, [2, 12]),
        (2, [4, 6]),
        (3, [1, 9]),
        (4, [5, 13]),
    ] {
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        for step in steps {
            *phrase.step_mut(step).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
        }
    }
    song
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), pool()))
}

/// Play through stopped and playing, with edits, previews, an audition and tails, letting tracks
/// rest or not.
fn play(rest: bool) -> Vec<u32> {
    let mut engine = Engine::new(RATE, version(song()));
    engine.let_rest(rest);
    let mut heard = Vec::new();
    let mut out = vec![0.0; BLOCK * 2];
    let mut run = |engine: &mut Engine, blocks: usize| {
        for _ in 0..blocks {
            engine.process(&mut out);
            heard.extend(out.iter().map(|sample| sample.to_bits()));
        }
    };
    run(&mut engine, 20);
    for track in 0..SOUNDS {
        engine.preview(track, 60, 100);
        run(&mut engine, 7);
    }
    run(&mut engine, 200);
    // Edited while stopped: the next note hears it at once.
    let mut edited = song();
    for track in edited.tracks.iter_mut().take(SOUNDS) {
        track.set_setting(SettingId::CUT, 0x50);
        track.set_setting(SettingId::PITCH, 0x84);
    }
    edited.tracks[2].mods[3].bypassed = true;
    edited.tracks[0].inserts[0] = Insert::Reverb(Reverb {
        decay: 0x30,
        ..Reverb::DEFAULT
    });
    engine.swap(version(edited.clone()));
    run(&mut engine, 1);
    for track in 0..SOUNDS {
        engine.preview(track, 67, 90);
        run(&mut engine, 3);
    }
    run(&mut engine, 150);
    engine.audition(0);
    run(&mut engine, 20);
    engine.end_audition(false);
    run(&mut engine, 150);
    engine.play_from(0);
    run(&mut engine, 300);
    engine.set_mute(1, true);
    edited.tracks[2].mods[3].bypassed = false;
    engine.swap(version(edited));
    run(&mut engine, 200);
    // Stopped, the tails ring out.
    engine.pause_or_resume();
    run(&mut engine, 400);
    engine.return_to_start();
    run(&mut engine, 50);
    engine.play_from(0);
    run(&mut engine, 200);
    engine.let_ring();
    run(&mut engine, 400);
    // Rung out to below what's heard, though not to nothing, a reverb is woken by a preview
    // between workings out, and goes on from where its wobbles would be.
    for track in 0..SOUNDS {
        engine.preview(track, 64, 100);
        run(&mut engine, 5);
    }
    run(&mut engine, 200);
    heard
}

#[test]
fn resting_tracks_change_nothing_heard() {
    let (resting, worked) = (play(true), play(false));
    assert!(
        resting
            .iter()
            .any(|&bits| f32::from_bits(bits).abs() > 0.01),
        "it plays"
    );
    assert!(resting == worked, "resting changed the sound");
}

#[test]
fn a_stopped_track_rests_until_an_edit_and_one_with_an_lfo_playing_does_not() {
    let mut engine = Engine::new(RATE, version(song()));
    let mut out = vec![0.0; BLOCK * 2];
    engine.process(&mut out);
    assert!(
        (0..=MASTER).all(|track| engine.rests(track)),
        "stopped, all rest"
    );
    engine.swap(version(song()));
    assert!(!engine.rests(0), "an edit wakes it");
    engine.process(&mut out);
    assert!(engine.rests(0), "and it rests again");
    engine.play_from(0);
    engine.process(&mut out);
    assert!(!engine.rests(2), "an LFO moves its filter");
    assert!(engine.rests(4), "nothing moves this one");
}
