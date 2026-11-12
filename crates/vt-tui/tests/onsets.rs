//! Sounds on the sample page: `w` and `b` go from one sound's start to the next as the entry's
//! `Sens` finds them, `e` to where a sound decays, and `is` and `ip` select a sound or a slice.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::onsets::DEFAULT_SENS;
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::{Mode, View};
use vt_tui::sample_settings::SampleSetting;

const RATE: u32 = 48_000;
const LENGTH: usize = 96_000;

/// Where the loud hits start, a beat apart at 120 BPM, with a quiet one between each pair.
const HITS: [u64; 4] = [2000, 26_000, 50_000, 74_000];
const GHOSTS: [u64; 3] = [14_000, 38_000, 62_000];

/// Each hit lasts this long, decaying by 6 dB each tenth of it, into silence.
const HIT_FRAMES: u64 = 8000;

/// Silence with hits of noise at the frames given, each at its level, decaying.
fn hits() -> Decoded {
    let mut samples = vec![0.0_f32; LENGTH * 2];
    let mut seed = 7_u32;
    let all = HITS
        .iter()
        .map(|at| (*at, 0.9))
        .chain(GHOSTS.iter().map(|at| (*at, 0.02)));
    for (start, level) in all {
        for frame in start..start + HIT_FRAMES {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0;
            let gain = 0.5_f32.powf((frame - start) as f32 / (HIT_FRAMES / 10) as f32);
            let value = noise * level * gain;
            samples[frame as usize * 2] = value;
            samples[frame as usize * 2 + 1] = value;
        }
    }
    Decoded {
        samples,
        channels: 2,
        rate: RATE,
    }
}

fn app() -> (App, Realtime) {
    let mut song = Song::new(1);
    let mut pool = SamplePool::default();
    let decoded = hits();
    pool.insert(
        "hits.wav",
        SampleData {
            frames: decoded.samples.as_chunks::<2>().0.to_vec(),
        },
    );
    pool.insert_analysis(Arc::new(Analysis::of(ContentHash(0xC3), &decoded)));
    let number = song
        .bank
        .add(BankEntry::of_file("hits.wav", ContentHash(0xC3)))
        .unwrap();
    song.tracks[0].machine = Machine::Sampler;
    song.tracks[0].set_setting(SettingId::SAMPLE, number);
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(RATE, version));
    let mut app = App::new(song, None, samples, handle, RATE);
    keys(&mut app, "Space 0");
    assert_eq!(app.view, View::Sample);
    (app, realtime)
}

fn keys(app: &mut App, names: &str) {
    for key in parse_sequence(names).unwrap() {
        app.key(key, Instant::now());
        app.key_released(key, Instant::now());
    }
}

fn command(app: &mut App, line: &str) {
    keys(app, ":");
    for c in line.chars() {
        app.key(Key::new(KeyCode::Char(c)), Instant::now());
    }
    app.key(Key::new(KeyCode::Enter), Instant::now());
}

fn cursor(app: &App) -> u64 {
    app.sample_page.cursor
}

fn sens(app: &App) -> u8 {
    app.song().bank.get(1).unwrap().edit.hints.sens
}

/// The app with the sample's `Sens` at 30, where only the loud hits count, and the focus back on
/// the waveform.
fn loud_only() -> (App, Realtime) {
    let (mut app, realtime) = app();
    keys(&mut app, "K j Enter _ _ Esc");
    assert_eq!(sens(&app), 30);
    (app, realtime)
}

/// Whether a frame is within a few of one expected.
fn near(found: u64, expected: u64) -> bool {
    found.abs_diff(expected) <= 4
}

#[test]
fn w_and_b_step_through_the_hits_and_counts_skip_some() {
    let (mut app, _realtime) = loud_only();
    assert_eq!(cursor(&app), 0);
    for hit in HITS {
        keys(&mut app, "w");
        assert!(near(cursor(&app), hit), "{} for {hit}", cursor(&app));
    }
    keys(&mut app, "w");
    assert!(near(cursor(&app), HITS[3]));
    assert!(app.status.contains("No sound further"), "{}", app.status);
    keys(&mut app, "b");
    assert!(near(cursor(&app), HITS[2]));
    keys(&mut app, "2 b");
    assert!(near(cursor(&app), HITS[0]));
    keys(&mut app, "3 w");
    assert!(near(cursor(&app), HITS[3]));
}

#[test]
fn sens_finds_the_quiet_hits_too_as_it_rises() {
    let (mut app, _realtime) = loud_only();
    // K lists the settings, Snap then Sens, and + raises it by 10.
    keys(&mut app, "K j Enter");
    assert_eq!(app.sample_page.setting, Some(SampleSetting::Sens));
    keys(&mut app, "+");
    assert!(app.status.contains("finding 4 sounds"), "{}", app.status);
    keys(&mut app, "+ + + + + +");
    assert_eq!(sens(&app), 100);
    assert!(app.status.contains("finding 7 sounds"), "{}", app.status);
    keys(&mut app, "Esc g g");
    let mut all: Vec<u64> = HITS.iter().chain(&GHOSTS).copied().collect();
    all.sort_unstable();
    for hit in all {
        keys(&mut app, "w");
        assert!(near(cursor(&app), hit), "{} for {hit}", cursor(&app));
    }
    keys(&mut app, "u");
    assert_eq!(sens(&app), 90);
    // Sens is the whole entry's, so the row shows it whatever slice the cursor's in.
    command(&mut app, "slice 4");
    keys(&mut app, "j j K j Enter x");
    assert_eq!(sens(&app), DEFAULT_SENS, "x puts it back");
}

#[test]
fn e_goes_to_where_each_sound_decays() {
    let (mut app, _realtime) = loud_only();
    // Each hit falls 30 dB half way through, then into its noise's dips.
    keys(&mut app, "e");
    let first = cursor(&app);
    assert!(
        first > HITS[0] + HIT_FRAMES / 3 && first < HITS[0] + HIT_FRAMES,
        "{first}"
    );
    // From where it decays, the next one's.
    keys(&mut app, "e");
    assert!(cursor(&app) > HITS[1] && cursor(&app) < HITS[1] + HIT_FRAMES);
    keys(&mut app, "b e");
    assert!(cursor(&app) > HITS[1] && cursor(&app) < HITS[1] + HIT_FRAMES);
    keys(&mut app, "2 e");
    assert!(cursor(&app) > HITS[3] && cursor(&app) < HITS[3] + HIT_FRAMES);
}

#[test]
fn is_selects_the_sound_here_and_ip_the_slice() {
    let (mut app, _realtime) = loud_only();
    keys(&mut app, "w w l l l");
    keys(&mut app, "v");
    assert_eq!(app.key_scope(), Mode::SampleVisual);
    keys(&mut app, "i s");
    assert_eq!(app.mode, Mode::Normal, "i doesn't start the pads");
    let selected = app.sample_page.selection().unwrap();
    assert!(near(selected.start, HITS[1]), "{selected:?}");
    assert!(selected.end > HITS[1] + HIT_FRAMES / 3 && selected.end < HITS[1] + HIT_FRAMES);
    keys(&mut app, "Esc");
    command(&mut app, "slice 4");
    keys(&mut app, "v i p");
    let shown = app.sample_shown().unwrap();
    let (_, slice) = shown.slice_at(cursor(&app));
    assert_eq!(app.sample_page.selection(), Some(slice));
    // Off a selection, `i` plays pads as before.
    keys(&mut app, "Esc i");
    assert_eq!(app.mode, Mode::Keys);
}

/// Where the sample's slices start.
fn starts(app: &App) -> Vec<u64> {
    let points = &app.song().bank.get(1).unwrap().edit.points;
    points.slices.iter().map(|slice| slice.start).collect()
}

#[test]
fn slice_transients_puts_a_slice_on_each_hit_and_sens_changes_how_many() {
    let (mut app, _realtime) = loud_only();
    command(&mut app, "slice transients");
    let found = starts(&app);
    assert_eq!(found.len(), HITS.len(), "{found:?}: {}", app.status);
    for (start, hit) in found.iter().zip(HITS) {
        assert!(near(*start, hit), "{start} for {hit}");
    }
    assert!(app.status.contains("4 slices"), "{}", app.status);
    // Every hit, the quiet ones too, at a higher Sens; and `u` takes it back.
    keys(&mut app, "K j Enter + + + + + + + Esc");
    assert_eq!(sens(&app), 100);
    command(&mut app, "slice transients");
    assert_eq!(starts(&app).len(), HITS.len() + GHOSTS.len());
    keys(&mut app, "u");
    assert_eq!(starts(&app).len(), HITS.len());
}

#[test]
fn slice_beat_cuts_a_bar_into_sixteenths_at_the_tempo_or_by_its_length_in_bars() {
    // Two seconds is a bar at 120 BPM.
    let (mut app, _realtime) = app();
    command(&mut app, "slice beat 1/16");
    let expected: Vec<u64> = (0..16).map(|at| at * 6000).collect();
    assert_eq!(starts(&app), expected, "{}", app.status);
    // Said to be two bars long, eighths are as long as sixteenths.
    command(&mut app, "sample bars 2");
    assert_eq!(app.song().bank.get(1).unwrap().edit.hints.bars, Some(2));
    command(&mut app, "slice beat 1/8");
    assert_eq!(starts(&app), expected);
    command(&mut app, "slice beat 1/4");
    assert_eq!(starts(&app).len(), 8);
}

#[test]
fn slice_silence_gives_one_slice_per_hit() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice silence");
    let mut all: Vec<u64> = HITS.iter().chain(&GHOSTS).copied().collect();
    all.sort_unstable();
    let found = starts(&app);
    assert_eq!(found.len(), all.len(), "{found:?}: {}", app.status);
    for (start, hit) in found.iter().zip(all) {
        assert!(near(*start, hit), "{start} for {hit}");
    }
}

#[test]
fn a_range_limits_re_slicing_to_the_slices_inside_it() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    command(&mut app, "mark a 1");
    command(&mut app, "mark b 2");
    command(&mut app, "mark c 3");
    command(&mut app, "'a,'b slice 8");
    let mut expected = vec![0];
    expected.extend((0..8).map(|at| 24_000 + at * 6000));
    expected.push(72_000);
    assert_eq!(starts(&app), expected, "{}", app.status);
    // The marks follow: 'a and 'b both start where they did, and 'c is still on the last.
    let points = &app.song().bank.get(1).unwrap().edit.points;
    assert_eq!(points.marked('a'), Some(1));
    assert_eq!(points.marked('c'), Some(9));
    // `:` from a selection starts the line with its range.
    keys(&mut app, "G v 4 k");
    let selected = app.sample_page.selection().unwrap();
    keys(&mut app, ":");
    assert_eq!(app.command_line, "'<,'>");
    assert_eq!(app.sample_page.anchor, None);
    for c in "slice clear".chars() {
        app.key(Key::new(KeyCode::Char(c)), Instant::now());
    }
    app.key(Key::new(KeyCode::Enter), Instant::now());
    let now = starts(&app);
    assert!(now.contains(&selected.start), "{now:?} {selected:?}");
    assert!(
        now.iter()
            .all(|at| !(selected.start + 1..selected.end).contains(at)),
        "{now:?} {selected:?}"
    );
    assert!(now.starts_with(&expected[..now.len().min(6)]));
}
