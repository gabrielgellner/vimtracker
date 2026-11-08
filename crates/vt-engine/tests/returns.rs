//! The returns and the master: what's routed through them, and what each can change.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_stems};
use vt_model::insert::{Distort, DistortType, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

const RATE: u32 = 48_000;
const STEP: usize = 6000;
/// When a hit on the first step comes back from the delay, 3/16 of a note later at 120 BPM.
const ECHO: usize = 18_100;
/// When the hit itself is sounding.
const HIT: usize = 100;
/// Where the fixed tracks are, after the one track that plays.
const RETURN_A: usize = 1;
const RETURN_B: usize = 2;
const MASTER: usize = 3;

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[0.5, 0.5]; 400],
        },
    );
    Arc::new(pool)
}

/// One short hit on the first step, sending this to both returns.
fn song(send: &str) -> Song {
    let mut song = Song::new(1);
    song.tracks[0].sample = Some("s".to_owned());
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    song.tracks[0].send_a = 0xE0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = format!("C-4 -- {send} --- ---").parse().unwrap();
    song
}

fn published(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), pool()))
}

fn render(song: Song) -> Vec<[f32; 2]> {
    render_frames(published(song), RATE, STEP * 8)
        .as_chunks::<2>()
        .0
        .to_vec()
}

/// Play with some live controls set first.
fn render_with(song: Song, set: impl FnOnce(&mut Engine)) -> Vec<[f32; 2]> {
    let mut engine = Engine::new(RATE, published(song));
    set(&mut engine);
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 8 * 2];
    engine.process(&mut out);
    out.as_chunks::<2>().0.to_vec()
}

/// Whether anything is sounding around a place.
fn loudest(out: &[[f32; 2]], around: usize) -> f32 {
    out[around..around + 200]
        .iter()
        .fold(0.0, |most, frame| most.max(frame[0].abs()))
}

fn hard() -> Insert {
    Insert::Distort(Distort {
        kind: DistortType::Hard,
        amount: 0xFF,
        mix: 0xFF,
        gain: 0xE0,
        ..Distort::DEFAULT
    })
}

#[test]
fn a_send_goes_through_its_return() {
    let out = render(song("---"));
    assert!(loudest(&out, HIT) > 0.1);
    assert!(loudest(&out, ECHO) > 0.05, "A is a delay");
    let mut dry = song("---");
    dry.tracks[0].send_a = 0;
    assert_eq!(loudest(&render(dry), ECHO), 0.0, "nothing sent");
    let mut reverbed = song("---");
    reverbed.tracks[0].send_a = 0;
    reverbed.tracks[0].send_b = 0xE0;
    assert!(
        loudest(&render(reverbed), STEP * 2) > 0.001,
        "B is a reverb"
    );
}

#[test]
fn an_echo_comes_back_exactly_on_time() {
    let out = render(song("---"));
    let first = (HIT + 1000..out.len())
        .find(|at| out[*at][0] != 0.0)
        .unwrap();
    assert!((18_000..18_010).contains(&first), "{first}");
}

#[test]
fn a_returns_level_changes_only_what_comes_back() {
    let plain = render(song("---"));
    let mut quiet = song("---");
    quiet.tracks[RETURN_A].level = 0;
    let out = render(quiet);
    assert_eq!(loudest(&out, ECHO), 0.0);
    assert_eq!(loudest(&out, HIT), loudest(&plain, HIT));
}

#[test]
fn a_returns_pan_moves_only_what_comes_back() {
    let mut panned = song("---");
    panned.tracks[RETURN_A].pan = 0;
    let out = render(panned);
    assert_eq!(out[ECHO][1], 0.0, "all to the left");
    assert!(out[ECHO][0].abs() > 0.05);
    assert_eq!(out[HIT][0], out[HIT][1], "the hit stays in the middle");
}

#[test]
fn a_returns_slot_shapes_only_what_comes_back() {
    let plain = render(song("---"));
    let mut shaped = song("---");
    shaped.tracks[RETURN_A].inserts[1] = hard();
    let out = render(shaped);
    assert_eq!(out[HIT], plain[HIT]);
    assert_ne!(out[ECHO], plain[ECHO]);
}

#[test]
fn the_masters_level_and_slots_change_everything() {
    let plain = render(song("---"));
    let mut silent = song("---");
    silent.tracks[MASTER].level = 0;
    assert!(render(silent).iter().all(|frame| *frame == [0.0, 0.0]));
    let mut shaped = song("---");
    shaped.tracks[MASTER].inserts[0] = hard();
    let out = render(shaped);
    assert_ne!(out[HIT], plain[HIT]);
    assert_ne!(out[ECHO], plain[ECHO]);
}

#[test]
fn a_returns_own_steps_reach_it_and_a_tracks_dont() {
    let plain = render(song("---"));
    let mut sequenced = song("---");
    sequenced.tracks[RETURN_A].add_chain(0);
    sequenced.sections[0].chains[RETURN_A] = Some(0);
    let phrase = Arc::make_mut(sequenced.tracks[RETURN_A].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "--- -- vol:00h --- ---".parse().unwrap();
    let out = render(sequenced);
    assert_eq!(loudest(&out, ECHO), 0.0, "its own step turned it down");
    assert_eq!(loudest(&out, HIT), loudest(&plain, HIT), "and only it");
}

#[test]
fn a_note_on_a_fixed_track_plays_nothing() {
    let mut song = song("---");
    song.tracks[0].send_a = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "--- -- --- --- ---".parse().unwrap();
    for track in [RETURN_A, RETURN_B, MASTER] {
        song.tracks[track].sample = Some("s".to_owned());
        song.tracks[track].add_chain(0);
        song.sections[0].chains[track] = Some(0);
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        *phrase.step_mut(0).unwrap() = "C-4 -- --- --- ---".parse().unwrap();
    }
    assert!(render(song).iter().all(|frame| *frame == [0.0, 0.0]));
}

#[test]
fn soloing_a_track_keeps_the_returns() {
    let mut two = song("---");
    two.add_track();
    let out = render_with(two.clone(), |engine| engine.set_solo(0, true));
    assert!(loudest(&out, ECHO) > 0.05);
    let out = render_with(two, |engine| engine.set_solo(1, true));
    assert_eq!(loudest(&out, HIT), 0.0, "the other one's silent");
    assert_eq!(loudest(&out, ECHO), 0.0, "and so sends nothing");
}

#[test]
fn muting_a_return_silences_it_and_leaves_the_rest() {
    let plain = render(song("---"));
    let out = render_with(song("---"), |engine| engine.set_mute(RETURN_A, true));
    assert_eq!(loudest(&out, HIT), loudest(&plain, HIT));
    assert!(loudest(&out, ECHO) < 1e-6, "{}", loudest(&out, ECHO));
}

#[test]
fn soloing_a_return_hears_what_sends_to_it_without_the_rest() {
    let out = render_with(song("---"), |engine| engine.set_solo(RETURN_A, true));
    assert_eq!(loudest(&out, HIT), 0.0, "not the hit itself");
    assert!(loudest(&out, ECHO) > 0.05, "but what it sent");
    let out = render_with(song("---"), |engine| engine.set_solo(RETURN_B, true));
    assert!(loudest(&out, ECHO) < 1e-6, "not the other return");
}

#[test]
fn a_fixed_tracks_live_controls_stay_with_it_as_tracks_come_and_go() {
    let start = song("---");
    let mut more = start.clone();
    more.add_track();
    let mut engine = Engine::new(RATE, published(start.clone()));
    engine.set_mute(RETURN_A, true);
    engine.swap(published(more));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 8 * 2];
    engine.process(&mut out);
    let out = out.as_chunks::<2>().0.to_vec();
    assert!(loudest(&out, HIT) > 0.1, "track 1 still plays");
    assert!(loudest(&out, ECHO) < 1e-6, "and A is still muted");

    engine.swap(published(start));
    engine.set_mute(RETURN_A, false);
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 8 * 2];
    engine.process(&mut out);
    let out = out.as_chunks::<2>().0.to_vec();
    assert!(loudest(&out, ECHO) > 0.05);
}

#[test]
fn a_delay_in_a_slot_keeps_ringing_after_its_note_ends() {
    let mut song = song("---");
    song.tracks[0].send_a = 0;
    song.tracks[0].inserts[2] = Insert::RETURN_DELAY;
    let out = render(song);
    assert_eq!(loudest(&out, HIT), 0.0, "all wet, so not the hit");
    assert!(loudest(&out, ECHO) > 0.05, "but its echo, after the note");
    let first = (0..out.len()).find(|at| out[*at][0] != 0.0).unwrap();
    assert!((18_000..18_010).contains(&first), "{first}");
}

#[test]
fn a_return_with_nothing_in_its_slots_passes_on_what_it_is_sent() {
    let mut song = song("---");
    song.tracks[RETURN_A].inserts[0] = Insert::Empty;
    let out = render(song);
    assert_eq!(loudest(&out, ECHO), 0.0, "no echo");
    let plain = render(self::song("---"));
    assert!(
        loudest(&out, HIT) > loudest(&plain, HIT) * 1.9,
        "the hit twice over"
    );
}

#[test]
fn stems_are_of_the_tracks_that_play_and_carry_what_they_send() {
    let mut two = song("---");
    two.add_track();
    let version = Published::new(Arc::new(two), pool());
    let stems = render_stems(&version, RATE, RenderOptions::default());
    let tracks: Vec<usize> = stems.iter().map(|stem| stem.track).collect();
    assert_eq!(tracks, [0, 1]);
}
