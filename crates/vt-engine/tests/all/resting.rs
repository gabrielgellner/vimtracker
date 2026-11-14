//! A track at rest, with nothing moving and nothing to hear, isn't worked out again until
//! something changes (#547), and a group, return or master with nothing coming in that would give
//! the same and change nothing is left out of the mix (#558): the song sounds exactly as it does
//! when everything is worked out every time, stopped or playing, through edits, previews,
//! auditions and tails ringing out.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::constants::MAX_GROUPS;
use vt_model::insert::{
    Compressor, CompressorType, Delay, Distort, DistortType, Equaliser, Exciter, Insert, Limiter,
    LowPassGate, Plate, Reverb, Space, Spring, Stutter,
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
/// The master's place among the fixed tracks, for `Engine::bus_rested`.
const MASTER_BUS: usize = MAX_GROUPS + 2;

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
    // Groups count from 1, as `G1` does.
    song.tracks[3].group = Some(1);
    song.tracks[4].group = Some(1);
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

/// The same with more in the mix: a Glue compressor with its clip and `Auto`, an EQ, a soft clip
/// and a limiter on the master, a low-pass gate and an exciter on a group no track is in, and a
/// delay on return B.
fn busy_mix() -> Song {
    let mut song = song();
    song.tracks[MASTER].inserts = [
        Insert::Compressor(Compressor {
            kind: CompressorType::Glue,
            auto: 1,
            peak_clip: 1,
            ..Compressor::DEFAULT
        }),
        Insert::Eq(Equaliser::DEFAULT),
        Insert::Distort(Distort {
            kind: DistortType::Soft,
            ..Distort::DEFAULT
        }),
        Insert::Limit(Limiter::DEFAULT),
    ];
    song.tracks[GROUP + 1].inserts[0] = Insert::Lpg(LowPassGate::DEFAULT);
    song.tracks[GROUP + 1].inserts[1] = Insert::Exciter(Exciter::DEFAULT);
    song.tracks[RETURN_A + 1].inserts[0] = Insert::Delay(Delay::DEFAULT);
    song.tracks[1].send_b = 0x80;
    song
}

/// The same with a master whose slots settle soon after a sound, a limiter and a bit crusher,
/// and no long reverb on return A, so the whole mix rests again once the tracks' tails die away.
fn settling_mix() -> Song {
    let mut song = song();
    song.tracks[RETURN_A].inserts[0] = Insert::Empty;
    song.tracks[MASTER].inserts[0] = Insert::Limit(Limiter::DEFAULT);
    song.tracks[MASTER].inserts[1] = Insert::Distort(Distort {
        kind: DistortType::Crush,
        ..Distort::DEFAULT
    });
    song
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), pool()))
}

/// Play through stopped and playing, with edits, previews, an audition and tails, letting tracks
/// rest or not, and say how many blocks the master was left out of.
fn play(song: fn() -> Song, rest: bool) -> (Vec<u32>, usize) {
    let mut engine = Engine::new(RATE, version(song()));
    engine.let_rest(rest);
    let mut heard = Vec::new();
    let mut rested = 0;
    let mut out = vec![0.0; BLOCK * 2];
    let mut run = |engine: &mut Engine, blocks: usize| {
        for _ in 0..blocks {
            engine.process(&mut out);
            heard.extend(out.iter().map(|sample| sample.to_bits()));
            rested += usize::from(engine.bus_rested(MASTER_BUS));
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
    edited.tracks[MASTER].level = 0xC0;
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
    (heard, rested)
}

/// Plays `song` with resting and without, and checks they sound the same and the master rested
/// for at least `least` blocks.
fn rests_without_a_change(song: fn() -> Song, least: usize) {
    let ((resting, rested), (worked, none)) = (play(song, true), play(song, false));
    assert!(
        resting
            .iter()
            .any(|&bits| f32::from_bits(bits).abs() > 0.01),
        "it plays"
    );
    assert!(resting == worked, "resting changed the sound");
    assert!(
        rested >= least && none == 0,
        "the master rested for {rested} blocks"
    );
}

#[test]
fn resting_tracks_change_nothing_heard() {
    // A compressor's `Auto` follows the level over a second whether it's on or not, so a master
    // with one rests only until it has heard something, for the length of this.
    rests_without_a_change(song, 20);
}

#[test]
fn resting_tracks_change_nothing_heard_in_a_mix_that_settles() {
    // A master whose slots settle soon after a sound rests again once what it heard has died
    // away.
    rests_without_a_change(settling_mix, 100);
}

#[test]
fn the_mix_rests_while_silent_and_wakes_for_what_comes_in() {
    let mut engine = Engine::new(RATE, version(busy_mix()));
    let mut out = vec![0.0; BLOCK * 2];
    engine.process(&mut out);
    assert!(
        (0..=MASTER_BUS).all(|bus| engine.bus_rested(bus)),
        "stopped and silent, the whole mix rests"
    );
    engine.preview(3, 60, 100);
    engine.process(&mut out);
    assert!(!engine.bus_rested(0), "a preview wakes its group");
    assert!(
        engine.bus_rested(1),
        "a group none of whose tracks plays rests"
    );
    assert!(
        !engine.bus_rested(MAX_GROUPS) && !engine.bus_rested(MASTER_BUS),
        "and while any track plays, the returns and master don't"
    );
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
