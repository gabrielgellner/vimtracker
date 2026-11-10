//! Groups: tracks summed into a group track, which plays the sum through its own slots, level and
//! pan to the master and sends to the returns (decision 123).

use std::f32::consts::TAU;
use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop, render_stems};
use vt_model::constants::MAX_GROUPS;
use vt_model::insert::{Compressor, CompressorType, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Song, TrackKind};

const RATE: u32 = 48_000;
const STEP: usize = 6000;
/// When a hit on the first step comes back from the delay, 3/16 of a note later at 120 BPM.
const ECHO: usize = 18_100;
/// When a hit on the first step is sounding, and one on the third.
const FIRST: usize = 100;
const THIRD: usize = 2 * STEP + 100;

/// A tone a little shorter than a step, so each hit has stopped before the next.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..5000)
        .map(|frame| {
            let sample = 0.4 * (frame as f32 * 220.0 / RATE as f32 * TAU).sin();
            [sample, sample]
        })
        .collect();
    pool.insert("tone", SampleData { frames });
    Arc::new(pool)
}

/// Tracks playing the tone, each hitting on the step given, with nothing sent.
fn song(hits: &[usize]) -> Song {
    let mut song = Song::new(hits.len());
    for (track, &step) in hits.iter().enumerate() {
        song.sample_on(track, "tone");
        song.tracks[track].envelopes[0].attack_ms = 0;
        song.tracks[track].envelopes[0].release_ms = 0;
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        *phrase.step_mut(step).unwrap() = "C-4 -- --- --- ---".parse().unwrap();
    }
    song
}

/// Where a fixed track is in a song.
fn place(song: &Song, kind: TrackKind) -> usize {
    song.fixed_track(kind).unwrap()
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
fn render_with(song: Song, set: impl FnOnce(&mut Engine, &Song)) -> Vec<[f32; 2]> {
    let mut engine = Engine::new(RATE, published(song.clone()));
    set(&mut engine, &song);
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 8 * 2];
    engine.process(&mut out);
    out.as_chunks::<2>().0.to_vec()
}

/// The loudest sample around a place.
fn loudest(out: &[[f32; 2]], around: usize) -> f32 {
    out[around..around + 400]
        .iter()
        .fold(0.0, |most, frame| most.max(frame[0].abs()))
}

/// A bus compressor pulling hard on the tone, quick enough to hear within a hit.
fn bus() -> Insert {
    Insert::Compressor(Compressor {
        kind: CompressorType::Bus,
        threshold: 0x60,
        bus_ratio: 2,
        bus_attack: 0,
        bus_release: 0,
        ..Compressor::DEFAULT
    })
}

#[test]
fn a_group_plays_the_sum_of_its_tracks() {
    let apart = song(&[0, 0]);
    let mut grouped = apart.clone();
    grouped.tracks[0].group = Some(1);
    grouped.tracks[1].group = Some(1);
    let (apart, grouped) = (render(apart), render(grouped));
    assert!(loudest(&apart, FIRST) > 0.5, "both hits together");
    assert_eq!(
        grouped, apart,
        "with nothing in its slots, the group is the sum"
    );
}

#[test]
fn two_drum_tracks_in_a_group_with_a_bus_compressor_are_the_compressed_sum() {
    let mut grouped = song(&[0, 0]);
    grouped.tracks[0].group = Some(1);
    grouped.tracks[1].group = Some(1);
    let group = place(&grouped, TrackKind::Group(1));
    grouped.tracks[group].inserts[0] = bus();
    // The same compressor on the master hears the same sum, as nothing else plays.
    let mut on_master = song(&[0, 0]);
    let master = place(&on_master, TrackKind::Master);
    on_master.tracks[master].inserts[0] = bus();
    let plain = render(song(&[0, 0]));
    let (grouped, on_master) = (render(grouped), render(on_master));
    assert_eq!(grouped, on_master, "the compressor hears the sum");
    let squeezed = loudest(&grouped, STEP / 2);
    assert!(
        squeezed < loudest(&plain, STEP / 2) * 0.7,
        "and turns it down: {squeezed}"
    );
}

#[test]
fn a_groups_level_and_slots_change_only_its_tracks() {
    let mut song = song(&[0, 2]);
    song.tracks[0].group = Some(1);
    let group = place(&song, TrackKind::Group(1));
    let plain = render(song.clone());
    song.tracks[group].level = 0;
    let out = render(song);
    assert_eq!(loudest(&out, FIRST), 0.0, "the grouped track is silent");
    assert_eq!(
        loudest(&out, THIRD),
        loudest(&plain, THIRD),
        "the other plays"
    );
}

#[test]
fn a_tracks_sends_go_from_the_track_and_the_groups_from_the_group() {
    let mut song = song(&[0]);
    song.tracks[0].group = Some(1);
    song.tracks[0].send_a = 0xE0;
    let group = place(&song, TrackKind::Group(1));
    song.tracks[group].level = 0;
    let out = render(song.clone());
    assert_eq!(loudest(&out, FIRST), 0.0, "the group is silent");
    assert!(
        loudest(&out, ECHO) > 0.05,
        "but the track's send still echoes"
    );

    song.tracks[0].send_a = 0;
    song.tracks[group].level = 0xE0;
    song.tracks[group].send_a = 0xE0;
    let out = render(song);
    assert!(loudest(&out, FIRST) > 0.2, "the group plays");
    assert!(loudest(&out, ECHO) > 0.05, "and sends to the delay itself");
}

#[test]
fn muting_a_group_mutes_its_tracks_and_what_they_send() {
    let mut song = song(&[0, 2]);
    song.tracks[0].group = Some(1);
    song.tracks[0].send_a = 0xE0;
    let out = render_with(song, |engine, song| {
        engine.set_mute(place(song, TrackKind::Group(1)), true);
    });
    assert!(loudest(&out, FIRST) < 1e-6, "{}", loudest(&out, FIRST));
    assert!(loudest(&out, ECHO) < 1e-6, "nor its send");
    assert!(loudest(&out, THIRD) > 0.2, "the track in no group plays");
}

#[test]
fn soloing_a_track_in_a_group_keeps_the_group() {
    let mut song = song(&[0, 2, 4]);
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    let out = render_with(song, |engine, _| engine.set_solo(0, true));
    assert!(loudest(&out, FIRST) > 0.2, "heard through its group");
    assert_eq!(loudest(&out, THIRD), 0.0, "not the other in the group");
    assert_eq!(loudest(&out, 4 * STEP + 100), 0.0, "nor the one in none");
}

#[test]
fn soloing_a_group_plays_all_its_tracks() {
    let mut song = song(&[0, 2, 4]);
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    let out = render_with(song.clone(), |engine, song| {
        engine.set_solo(place(song, TrackKind::Group(1)), true);
    });
    assert!(loudest(&out, FIRST) > 0.2);
    assert!(loudest(&out, THIRD) > 0.2);
    assert_eq!(loudest(&out, 4 * STEP + 100), 0.0, "not the one outside it");

    let out = render_with(song, |engine, song| {
        engine.set_solo(place(song, TrackKind::Group(2)), true);
    });
    assert!(
        out.iter().all(|frame| *frame == [0.0, 0.0]),
        "an empty group soloed plays nothing"
    );
}

#[test]
fn joining_a_muted_group_while_playing_is_heard_at_once() {
    let plain = song(&[0, 0]);
    let group = place(&plain, TrackKind::Group(1));
    let mut grouped = plain.clone();
    grouped.tracks[0].group = Some(1);
    let mut engine = Engine::new(RATE, published(plain));
    engine.set_mute(group, true);
    engine.swap(published(grouped));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    let out = out.as_chunks::<2>().0.to_vec();
    let alone = render(song(&[0]));
    assert!(
        (loudest(&out, STEP / 2) - loudest(&alone, STEP / 2)).abs() < 1e-4,
        "only the track outside the group"
    );
}

#[test]
fn a_group_renders_as_one_stem_and_its_tracks_on_request() {
    let mut song = song(&[0, 2, 4]);
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    song.tracks[1].send_a = 0xE0;
    let group = place(&song, TrackKind::Group(1));
    let version = Published::new(Arc::new(song.clone()), pool());

    let stems = render_stems(&version, RATE, RenderOptions::default());
    let tracks: Vec<usize> = stems.iter().map(|stem| stem.track).collect();
    assert_eq!(tracks, [2, group], "the track in no group, then the group");
    let mut alone = song.clone();
    alone.limiter = false;
    alone.tracks[2].mute = true;
    let expected = render_loop(published(alone), RATE, RenderOptions::default());
    assert_eq!(
        stems[1].samples, expected,
        "the group's stem is its tracks, sends and all"
    );

    let split = RenderOptions {
        each_track: true,
        ..RenderOptions::default()
    };
    let stems = render_stems(&version, RATE, split);
    let tracks: Vec<usize> = stems.iter().map(|stem| stem.track).collect();
    assert_eq!(tracks, [0, 1, 2], "each track, those in the group too");
}

#[test]
fn a_muted_group_has_no_stem_and_a_muted_track_is_left_out_of_its_groups() {
    let mut song = song(&[0, 2, 4]);
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    song.tracks[1].mute = true;
    let group = place(&song, TrackKind::Group(1));
    let version = Published::new(Arc::new(song.clone()), pool());
    let stems = render_stems(&version, RATE, RenderOptions::default());
    let out: Vec<[f32; 2]> = stems[1].samples.as_chunks::<2>().0.to_vec();
    assert_eq!(stems[1].track, group);
    assert!(
        loudest(&out, FIRST) > 0.2,
        "the group's stem has the track that plays"
    );
    assert_eq!(loudest(&out, THIRD), 0.0, "and not the muted one");

    song.tracks[group].mute = true;
    let version = Published::new(Arc::new(song), pool());
    let stems = render_stems(&version, RATE, RenderOptions::default());
    let tracks: Vec<usize> = stems.iter().map(|stem| stem.track).collect();
    assert_eq!(tracks, [2]);
}

#[test]
fn the_returns_come_after_every_group() {
    let song = song(&[0]);
    assert_eq!(place(&song, TrackKind::Group(1)), 1);
    assert_eq!(place(&song, TrackKind::ReturnA), 1 + MAX_GROUPS);
}

/// A compressor that turns all the way down whatever's over nothing, at once, hearing a source.
fn keyed(source: u8) -> Insert {
    Insert::Compressor(Compressor {
        threshold: 0x00,
        ratio: 0xFF,
        attack: 0x00,
        source,
        ..Compressor::DEFAULT
    })
}

/// Where `Source` names a group, after the sound tracks and the returns.
fn group_source(group: u8) -> u8 {
    vt_model::insert::SOURCE_NAMES
        .iter()
        .position(|name| *name == format!("G{group}"))
        .unwrap() as u8
}

#[test]
fn a_compressor_hearing_a_group_ducks_under_it_while_the_group_is_muted_or_soloed_out() {
    // A kit of two in G1, and a bass on the third track, all on the first step.
    let mut kit = song(&[0, 0, 0]);
    kit.tracks[0].group = Some(1);
    kit.tracks[1].group = Some(1);
    let mut ducked = kit.clone();
    ducked.tracks[2].inserts[0] = keyed(group_source(1));
    let group = place(&kit, TrackKind::Group(1));
    let at = STEP / 2;
    for (how, set) in [
        (
            "muted",
            (|engine: &mut Engine, group| engine.set_mute(group, true)) as fn(&mut Engine, usize),
        ),
        ("soloed out", |engine: &mut Engine, _| {
            engine.set_solo(2, true)
        }),
    ] {
        let plain = render_with(kit.clone(), |engine, _| set(engine, group));
        let under = render_with(ducked.clone(), |engine, _| set(engine, group));
        assert!(loudest(&plain, at) > 0.3, "{how}: the bass alone");
        assert!(
            loudest(&under, at) < loudest(&plain, at) * 0.2,
            "{how}: the kit still ducks it, {} {}",
            loudest(&under, at),
            loudest(&plain, at)
        );
    }
}

#[test]
fn a_muted_group_a_compressor_hears_is_still_silent() {
    let mut kit = song(&[0, 0, 0]);
    kit.tracks[0].group = Some(1);
    kit.tracks[1].group = Some(1);
    kit.tracks[2].inserts[0] = keyed(group_source(1));
    // The bass plays nothing, so all that could be heard is the kit.
    let phrase = Arc::make_mut(kit.tracks[2].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = vt_model::step::Step::default();
    let group = place(&kit, TrackKind::Group(1));
    let heard = render(kit.clone());
    assert!(loudest(&heard, FIRST) > 0.5, "the kit, unmuted");
    // Muted while it's silent, as it is before playing, it's silent from the first hit.
    let muted = render_with(kit, |engine, _| {
        engine.set_mute(group, true);
        engine.process(&mut [0.0; 64]);
    });
    assert!(muted.iter().all(|frame| *frame == [0.0, 0.0]), "nothing");
}

#[test]
fn a_group_nobody_hears_plays_as_it_did() {
    // The same song with a compressor hearing an empty group, which can't change what's heard
    // of G1's muted tracks.
    let mut kit = song(&[0, 0, 0]);
    kit.tracks[0].group = Some(1);
    let mut listening = kit.clone();
    listening.tracks[2].inserts[0] = keyed(group_source(2));
    let group = place(&kit, TrackKind::Group(1));
    let mute = |engine: &mut Engine, _: &Song| engine.set_mute(group, true);
    assert_eq!(render_with(listening, mute), render_with(kit, mute));
}
