//! Mutes and solos on the launch grid: a key arms a track's mute or solo while playing, and it
//! switches at the next line of the grid, with every other waiting switch, on the same sample.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Launch, Phrase, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
const HALF: usize = STEP / 2;

/// A level that holds, so what's heard is only what the mutes and solos let through.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[0.25, 0.25]; RATE as usize * 8],
        },
    );
    Arc::new(pool)
}

/// Tracks each playing a note on every step, in a section long enough that the grid isn't cut
/// short by its repeats, with lines every 4 steps.
fn version(tracks: usize) -> Arc<Published> {
    let mut song = Song::new(tracks);
    song.limiter = false;
    song.sections[0].length = Some(64);
    song.launch = Launch::every(4).unwrap();
    for index in 0..tracks {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.mods[0].adsr.attack_ms = 0;
        track.mods[0].adsr.release_ms = 0;
        let steps = ["C-3 40"; 4]
            .iter()
            .map(|text| text.parse().unwrap())
            .collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    Arc::new(Published::new(Arc::new(song), pool()))
}

fn playing(tracks: usize) -> Engine {
    let mut engine = Engine::new(RATE, version(tracks));
    engine.play_from(0);
    engine
}

/// Play this many frames, adding them to what's been heard.
fn play(engine: &mut Engine, frames: usize, heard: &mut Vec<f32>) {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    heard.extend(out);
}

#[test]
fn a_mute_armed_mid_bar_switches_exactly_on_the_next_line() {
    // Armed in step 1, it switches on step 4, as muting there by hand does, to the sample.
    let mut armed = playing(1);
    let mut heard = Vec::new();
    play(&mut armed, STEP + HALF, &mut heard);
    armed.toggle_mute_on_grid(0);
    assert!(armed.mute_waiting(0));
    play(&mut armed, 3 * STEP - HALF, &mut heard);
    assert!(!armed.jam(0).mute, "not before the line");
    play(&mut armed, 4 * STEP, &mut heard);
    assert!(armed.jam(0).mute);
    assert!(!armed.mute_waiting(0), "it's done once landed");
    assert_eq!(armed.jam(0).switches_landed, 1);

    let mut by_hand = playing(1);
    let mut expected = Vec::new();
    play(&mut by_hand, 4 * STEP, &mut expected);
    by_hand.set_mute(0, true);
    play(&mut by_hand, 4 * STEP, &mut expected);
    assert_eq!(heard, expected);
    assert!(heard[(3 * STEP + HALF) * 2] > 0.1, "heard up to the line");
    assert!(heard[(6 * STEP) * 2].abs() < 1e-6, "silent after it");
}

#[test]
fn arming_again_before_the_line_takes_it_back() {
    let mut engine = playing(1);
    let mut heard = Vec::new();
    play(&mut engine, HALF, &mut heard);
    engine.toggle_mute_on_grid(0);
    engine.toggle_mute_on_grid(0);
    assert!(!engine.mute_waiting(0));
    play(&mut engine, 8 * STEP, &mut heard);
    assert!(!engine.jam(0).mute);
    assert_eq!(engine.jam(0).switches_landed, 0);
    assert!(heard[(5 * STEP) * 2] > 0.1);
}

#[test]
fn two_tracks_armed_in_the_same_bar_switch_on_the_same_sample() {
    // One mutes and the other solos, armed a step apart: both land on step 4, as switching
    // both there by hand does.
    let mut armed = playing(2);
    let mut heard = Vec::new();
    play(&mut armed, HALF, &mut heard);
    armed.toggle_mute_on_grid(0);
    play(&mut armed, 2 * STEP, &mut heard);
    armed.toggle_solo_on_grid(1);
    play(&mut armed, 5 * STEP + HALF, &mut heard);
    assert!(armed.jam(0).mute && armed.jam(1).solo);

    let mut by_hand = playing(2);
    let mut expected = Vec::new();
    play(&mut by_hand, 4 * STEP, &mut expected);
    by_hand.set_mute(0, true);
    by_hand.set_solo(1, true);
    play(&mut by_hand, 4 * STEP, &mut expected);
    assert_eq!(heard, expected);
}

#[test]
fn a_solo_switches_off_on_the_grid_too() {
    let mut engine = playing(2);
    engine.set_solo(1, true);
    let mut heard = Vec::new();
    play(&mut engine, HALF, &mut heard);
    engine.toggle_solo_on_grid(1);
    play(&mut engine, 3 * STEP, &mut heard);
    assert!(engine.jam(1).solo, "still soloed before the line");
    play(&mut engine, STEP, &mut heard);
    assert!(!engine.jam(1).solo);
}

#[test]
fn only_while_playing_and_stopping_or_switching_by_hand_forgets_it() {
    let mut engine = Engine::new(RATE, version(1));
    engine.toggle_mute_on_grid(0);
    assert!(!engine.mute_waiting(0), "no grid comes round while stopped");
    engine.play_from(0);
    engine.toggle_mute_on_grid(0);
    engine.pause_or_resume();
    assert!(!engine.mute_waiting(0), "pausing forgets it");
    engine.pause_or_resume();
    engine.toggle_solo_on_grid(0);
    engine.set_solo(0, false);
    assert!(!engine.solo_waiting(0), "a switch by hand takes it back");
    engine.toggle_mute_on_grid(0);
    engine.set_mute(0, true);
    assert!(!engine.mute_waiting(0));
    engine.process(&mut vec![0.0; 8 * STEP * 2]);
    assert!(engine.jam(0).mute, "the one by hand stays");
}
