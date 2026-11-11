//! A track's trim: the gain at its input, before anything else it does, for gain staging, from
//! silence at `00` through 0 dB at `80` to +31.75 dB at `FF` (decision 136).

use std::f32::consts::TAU;
use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::constants::UNITY_TRIM;
use vt_model::insert::{Distort, DistortType, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Song, TrackKind, trim_db, trim_gain};

const RATE: u32 = 48_000;
/// A step at 120 BPM.
const STEP: usize = 6000;
/// How long each render is: long enough for an echo.
const FRAMES: usize = 4 * STEP;
/// When the note is well under way.
const SOUNDING: usize = 1000;
/// When a hit on the first step comes back from the delay, 3/16 of a note later at 120 BPM.
const ECHO: usize = 18_100;

/// A steady level, so a gain shows as a ratio, the same a little shorter than a step, to hear an
/// echo of, and a tone, for the filter's drive to bend.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "steady",
        SampleData {
            frames: vec![[0.25, 0.25]; RATE as usize],
        },
    );
    pool.insert(
        "short",
        SampleData {
            frames: vec![[0.25, 0.25]; 5000],
        },
    );
    let tone = (0..RATE as usize)
        .map(|frame| {
            let sample = 0.4 * (frame as f32 * 220.0 / RATE as f32 * TAU).sin();
            [sample, sample]
        })
        .collect();
    pool.insert("tone", SampleData { frames: tone });
    Arc::new(pool)
}

/// One track playing a sample from the first step, starting and stopping at once, with the
/// limiter off so a boost comes through whole.
fn song(sample: &str) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, sample);
    let track = &mut song.tracks[0];
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    song
}

fn published(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), pool()))
}

/// The left channel.
fn render(song: Song) -> Vec<f32> {
    render_frames(published(song), RATE, FRAMES)
        .iter()
        .step_by(2)
        .copied()
        .collect()
}

/// The largest step from one frame to the next.
fn largest_step(out: &[f32]) -> f32 {
    out.windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

/// The loudest sample over a stretch.
fn loudest(out: &[f32]) -> f32 {
    out.iter().fold(0.0, |most, sample| most.max(sample.abs()))
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.log10()
}

/// The same song with one track's trim set.
fn trimmed(mut song: Song, track: usize, trim: u8) -> Song {
    song.tracks[track].trim = trim;
    song
}

#[test]
fn the_scale_is_quarter_db_steps_either_side_of_80_with_00_off() {
    assert_eq!(UNITY_TRIM, 0x80);
    assert_eq!(trim_db(0x80), Some(0.0));
    // Exactly, so 0 dB changes nothing, and the golden renders, at 0 dB, are as they were.
    assert_eq!(trim_gain(0x80), 1.0);
    assert_eq!(trim_db(0xFF), Some(31.75));
    assert_eq!(trim_db(0xC0), Some(16.0));
    assert_eq!(trim_db(0x40), Some(-16.0));
    assert_eq!(trim_db(0x01), Some(-31.75));
    assert_eq!(trim_db(0x00), None);
    assert_eq!(trim_gain(0x00), 0.0);
    assert!((db(trim_gain(0xFF)) - 31.75).abs() < 1e-4);
    assert!((db(trim_gain(0x01)) + 31.75).abs() < 1e-4);
}

#[test]
fn the_top_boosts_by_exactly_its_db() {
    let plain = render(song("steady"));
    let boosted = render(trimmed(song("steady"), 0, 0xFF));
    assert!(plain[SOUNDING] > 0.2, "{}", plain[SOUNDING]);
    for frame in [1, SOUNDING, STEP] {
        let gained = db(boosted[frame] / plain[frame]);
        assert!((gained - 31.75).abs() < 1e-3, "{frame}: {gained} dB");
    }
}

#[test]
fn the_bottom_is_silence() {
    let silent = render(trimmed(song("tone"), 0, 0x00));
    assert!(silent.iter().all(|sample| *sample == 0.0));
}

#[test]
fn it_comes_before_the_filters_drive_so_a_hot_trim_drives_it_harder() {
    // +12 dB in and 12 dB off the fader is the same level, but the drive hears the 12 dB.
    let hot = |mut song: Song| {
        song.tracks[0].trim = UNITY_TRIM + 48;
        song.tracks[0].level -= 48;
        song
    };
    let clean = song("tone");
    let (plain, matched) = (render(clean.clone()), render(hot(clean)));
    let apart = largest_difference(&plain, &matched);
    assert!(
        apart < 1e-4,
        "with nothing that bends, the two cancel: {apart}"
    );

    let mut driven = song("tone");
    driven.tracks[0].drive = 0xFF;
    let (plain, hotter) = (render(driven.clone()), render(hot(driven)));
    let squeezed = db(loudest(&hotter[SOUNDING..STEP]) / loudest(&plain[SOUNDING..STEP]));
    assert!(
        squeezed < -1.0,
        "the drive squeezes the hotter input more: {squeezed} dB"
    );
}

fn largest_difference(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max)
}

#[test]
fn moving_it_glides_without_a_click() {
    let mut engine = Engine::new(RATE, published(song("steady")));
    engine.play_from(0);
    let mut before = vec![0.0; STEP * 2];
    engine.process(&mut before);
    let level = before[before.len() - 2];
    assert!(level > 0.2, "{level}");
    for (trim, end) in [(0x00, 0.0), (UNITY_TRIM, level)] {
        engine.swap(published(trimmed(song("steady"), 0, trim)));
        let mut out = vec![0.0; STEP * 2];
        engine.process(&mut out);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let step = largest_step(&left);
        assert!(step < level / 100.0, "to {trim:02X}, a step of {step}");
        let last = left[left.len() - 1];
        assert!(
            (last - end).abs() < 1e-4,
            "to {trim:02X}, it ends at {last}"
        );
    }
}

#[test]
fn a_groups_trim_turns_up_or_silences_what_its_tracks_give_it() {
    let mut grouped = song("steady");
    grouped.tracks[0].group = Some(1);
    let group = grouped.fixed_track(TrackKind::Group(1)).unwrap();
    let plain = render(grouped.clone());
    let silent = render(trimmed(grouped.clone(), group, 0x00));
    assert!(silent.iter().all(|sample| *sample == 0.0));
    let boosted = render(trimmed(grouped, group, 0xFF));
    let gained = db(boosted[SOUNDING] / plain[SOUNDING]);
    assert!((gained - 31.75).abs() < 1e-3, "{gained} dB");
}

#[test]
fn a_returns_trim_sets_how_hard_what_is_sent_hits_its_slots() {
    let mut hit = song("short");
    hit.tracks[0].send_a = 0xE0;
    let return_a = hit.fixed_track(TrackKind::ReturnA).unwrap();
    let echo = |out: &[f32]| loudest(&out[ECHO..ECHO + 400]);
    let plain = render(hit.clone());
    assert!(echo(&plain) > 0.05, "the delay echoes: {}", echo(&plain));
    let silent = render(trimmed(hit, return_a, 0x00));
    assert_eq!(echo(&silent), 0.0, "nothing reaches the delay");
    // With nothing in its slots, what comes back is what's sent, turned up.
    let mut sending = song("steady");
    sending.tracks[0].send_a = 0xE0;
    sending.tracks[return_a].inserts = Default::default();
    let with_return = render(sending.clone());
    let only_dry = render(trimmed(sending.clone(), return_a, 0x00));
    let boosted = render(trimmed(sending, return_a, 0xFF));
    let returned = with_return[SOUNDING] - only_dry[SOUNDING];
    let gained = db((boosted[SOUNDING] - only_dry[SOUNDING]) / returned);
    assert!((gained - 31.75).abs() < 1e-2, "{gained} dB");
}

#[test]
fn the_masters_trim_turns_up_the_whole_mix() {
    let song = song("steady");
    let master = song.fixed_track(TrackKind::Master).unwrap();
    let plain = render(song.clone());
    let silent = render(trimmed(song.clone(), master, 0x00));
    assert!(silent.iter().all(|sample| *sample == 0.0));
    let boosted = render(trimmed(song, master, 0xFF));
    let gained = db(boosted[SOUNDING] / plain[SOUNDING]);
    assert!((gained - 31.75).abs() < 1e-3, "{gained} dB");
}

/// A distortion in a slot, which squeezes a louder input more.
fn distortion() -> Insert {
    Insert::Distort(Distort {
        kind: DistortType::Hard,
        amount: 0x80,
        mix: 0xFF,
        ..Distort::DEFAULT
    })
}

/// How much louder a fixed track's output gets, in dB, for 12 dB more trim and 12 dB less level,
/// with a distortion in its first slot: nothing if the trim came after the slot.
fn squeezed_by_its_slots(mut song: Song, track: usize) -> f32 {
    song.tracks[track].inserts[0] = distortion();
    let plain = render(song.clone());
    song.tracks[track].trim = UNITY_TRIM + 48;
    song.tracks[track].level -= 48;
    let hot = render(song);
    db(loudest(&hot[SOUNDING..STEP]) / loudest(&plain[SOUNDING..STEP]))
}

#[test]
fn a_fixed_tracks_trim_comes_before_its_slots() {
    let mut grouped = song("tone");
    grouped.tracks[0].group = Some(1);
    let group = grouped.fixed_track(TrackKind::Group(1)).unwrap();
    let squeezed = squeezed_by_its_slots(grouped, group);
    assert!(squeezed < -1.0, "the group's slot hears it: {squeezed} dB");
    let plain = song("tone");
    let master = plain.fixed_track(TrackKind::Master).unwrap();
    let squeezed = squeezed_by_its_slots(plain, master);
    assert!(squeezed < -1.0, "and the master's: {squeezed} dB");
}
