//! Switching an envelope or an LFO out with `b`, and back in: what it moves glides to where it
//! would be without it, and back, and it keeps running underneath, so it comes back in time.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A quarter note, as ticks.
const QUARTER: u32 = 960;
/// Frames in a millisecond.
const MS: usize = 48;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 20],
        },
    );
    Arc::new(pool)
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), flat()))
}

/// One long note on a track held at `CUT 80`.
fn song() -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    song.tracks[0].cutoff = 0x80;
    song
}

/// The same with LFO 1 on `Cut`, a square a quarter long and `+40` deep: `C0` for two steps,
/// then `40` for two.
fn with_square_on_cut() -> Song {
    let mut song = song();
    song.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::CUT),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Note(QUARTER),
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    song
}

/// Run an engine on for some frames, in blocks of this many, giving its live cutoff after each.
fn cutoffs(engine: &mut Engine, frames: usize, block: usize) -> Vec<u8> {
    let mut out = vec![0.0; block * 2];
    (0..frames / block)
        .map(|_| {
            engine.process(&mut out);
            engine.live(0).cutoff
        })
        .collect()
}

/// Whether some live values glide from one to the other, through several on the way, rather than
/// jumping.
fn glides(values: &[u8], from: u8, to: u8) {
    let after = values.iter().position(|value| *value == to).unwrap();
    assert!(
        after >= 4,
        "it took {after} blocks to get there: {values:?}"
    );
    for pair in values[..=after].windows(2) {
        let (a, b) = (i16::from(pair[0]), i16::from(pair[1]));
        assert!((b - a).abs() <= 0x10, "a jump: {values:?}");
        let towards = (i16::from(to) - a).signum();
        assert!((b - a).signum() != -towards, "it turned back: {values:?}");
    }
    assert_ne!(values[0], to, "{values:?}");
    assert!(
        (i16::from(values[0]) - i16::from(from)).abs() <= 0x10,
        "{values:?}"
    );
}

#[test]
fn switching_out_an_lfo_on_cut_glides_to_the_held_value_and_stops_its_movement() {
    let mut song = with_square_on_cut();
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    assert_eq!(*cutoffs(&mut engine, STEP, STEP).last().unwrap(), 0xC0);

    song.tracks[0].mods[3].bypassed = true;
    engine.swap(version(&song));
    // In blocks of 16 frames over 10 ms: it glides down over the 5 ms a slot's bypass takes, and
    // lands there within its next update after that.
    let fading = cutoffs(&mut engine, 10 * MS, 16);
    glides(&fading, 0xC0, 0x80);
    let landed = fading.iter().position(|value| *value == 0x80).unwrap();
    assert!(landed * 16 <= 5 * MS + 32, "{fading:?}");
    // Then nothing moves it, through the square's low half and on.
    let after = cutoffs(&mut engine, STEP * 4, 1000);
    assert!(after.iter().all(|value| *value == 0x80), "{after:?}");
}

#[test]
fn switched_back_in_an_lfo_is_in_phase_with_one_left_in() {
    let mut song = with_square_on_cut();
    // A triangle whose cycle isn't a whole number of blocks, so being out of phase would show.
    song.tracks[0].mods[3].lfo.shape = Shape::Tri;
    song.tracks[0].mods[3].lfo.rate = Rate::Steps(3);
    let mut left_in = Engine::new(RATE, version(&song));
    let mut switched = Engine::new(RATE, version(&song));
    left_in.play_from(0);
    switched.play_from(0);
    let block = 700;
    let first = cutoffs(&mut left_in, STEP, block);
    assert_eq!(first, cutoffs(&mut switched, STEP, block));

    // Out for a step and a bit, it sits at the held value while the other moves.
    song.tracks[0].mods[3].bypassed = true;
    switched.swap(version(&song));
    let moving = cutoffs(&mut left_in, STEP + 1300, block);
    let held = cutoffs(&mut switched, STEP + 1300, block);
    assert!(held[2..].iter().all(|value| *value == 0x80), "{held:?}");
    assert!(moving.iter().any(|value| *value != 0x80), "{moving:?}");

    // Back in, it fades up, and once it has it's where the one left in is, block by block.
    song.tracks[0].mods[3].bypassed = false;
    switched.swap(version(&song));
    let left = cutoffs(&mut left_in, 10 * MS, 16);
    let back = cutoffs(&mut switched, 10 * MS, 16);
    assert_ne!(
        back[0], left[0],
        "it fades back rather than jumping: {back:?}"
    );
    assert_eq!(back[20..], left[20..]);
    let left = cutoffs(&mut left_in, STEP * 3, block);
    let back = cutoffs(&mut switched, STEP * 3, block);
    assert_eq!(back, left);
}

#[test]
fn switching_an_envelope_on_cut_out_and_back_glides_both_ways() {
    let mut song = song();
    song.tracks[0].cutoff = 0x40;
    let held = Adsr {
        attack_ms: 0,
        decay_ms: 0,
        sustain: 100,
        ..Adsr::decay()
    };
    song.tracks[0].mods[1] = ModSlot::adsr(Some(SettingId::CUT), held).with_depth(0x40);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    assert_eq!(*cutoffs(&mut engine, STEP, STEP).last().unwrap(), 0x80);

    song.tracks[0].mods[1].bypassed = true;
    engine.swap(version(&song));
    let out = cutoffs(&mut engine, 10 * MS, 16);
    glides(&out, 0x80, 0x40);
    assert_eq!(*out.last().unwrap(), 0x40);

    // It kept running underneath, so back in it's the held note's sustain again.
    song.tracks[0].mods[1].bypassed = false;
    engine.swap(version(&song));
    let out = cutoffs(&mut engine, 10 * MS, 16);
    glides(&out, 0x40, 0x80);
    assert_eq!(*out.last().unwrap(), 0x80);
}

#[test]
fn a_song_that_starts_with_a_modulator_switched_out_plays_as_with_no_depth() {
    let mut switched = with_square_on_cut();
    switched.tracks[0].mods[3].bypassed = true;
    switched.tracks[0].mods[1].depth = 0x40;
    switched.tracks[0].mods[1].bypassed = true;
    let mut without = with_square_on_cut();
    without.tracks[0].mods[3].depth = 0;
    let render = |song: &Song| render_frames(version(song), RATE, STEP * 4);
    assert_eq!(render(&switched), render(&without));
}

#[test]
fn the_first_envelope_on_vol_switched_out_leaves_notes_their_plain_shape() {
    let mut song = song();
    // A short decay to nothing, which ends the note well inside its step while it's in.
    song.tracks[0].mods[0].adsr.decay_ms = 50;
    song.tracks[0].mods[0].adsr.sustain = 0;
    let shaped = render_frames(version(&song), RATE, STEP);
    assert_eq!(shaped[(STEP - 1) * 2], 0.0);
    song.tracks[0].mods[0].bypassed = true;
    let plain = render_frames(version(&song), RATE, STEP);
    assert!(plain[(STEP - 1) * 2] > 0.5, "{}", plain[(STEP - 1) * 2]);
}
