//! Loop to tempo on the sample page and from the command line (#286): how many steps a sample
//! lasts, its warp, `Play`'s `Tempo`, and grab tempo, `:bpm sample`.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash, Hints, Warp};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::{PLAY_CHOICES, PlayMode, SettingId};
use vt_model::song::{Machine, Song};
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::View;
use vt_tui::sample_settings::SampleSetting;

const RATE: u32 = 48_000;

/// A sampler playing a file of so many frames, on the sample page.
fn app(frames: usize) -> (App, Realtime) {
    app_with(frames, |_| {})
}

/// As `app`, its entry changed first.
fn app_with(frames: usize, edit: impl FnOnce(&mut BankEntry)) -> (App, Realtime) {
    let mut song = Song::new(1);
    let mut pool = SamplePool::default();
    let decoded = Decoded {
        samples: vec![0.1; frames * 2],
        channels: 2,
        rate: RATE,
    };
    pool.insert(
        "loop.wav",
        SampleData {
            frames: decoded.samples.as_chunks::<2>().0.to_vec(),
        },
    );
    pool.insert_analysis(Arc::new(Analysis::of(ContentHash(0xA1), &decoded)));
    let mut entry = BankEntry::of_file("loop.wav", ContentHash(0xA1));
    edit(&mut entry);
    let number = song.bank.add(entry).unwrap();
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

fn hints(app: &App) -> Hints {
    app.song().bank.get(1).unwrap().edit.hints
}

#[test]
fn play_has_a_tempo_choice() {
    assert_eq!(PLAY_CHOICES[PlayMode::Tempo as usize], "Tempo");
    assert_eq!(PlayMode::from_index(4), PlayMode::Tempo);
}

#[test]
fn a_length_in_steps_is_kept_in_bars_where_its_whole_bars() {
    let (mut app, _realtime) = app(96_000);
    command(&mut app, "sample steps 8");
    assert_eq!((hints(&app).bars, hints(&app).steps), (None, Some(8)));
    assert!(app.status.contains("lasts 8 steps"), "{}", app.status);
    command(&mut app, "sample steps 32");
    assert_eq!((hints(&app).bars, hints(&app).steps), (Some(2), None));
    command(&mut app, "sample bars 1");
    assert_eq!(hints(&app).length_steps(), Some(16));
    assert!(app.status.contains("lasts a bar"), "{}", app.status);
    command(&mut app, "sample steps off");
    assert_eq!(hints(&app), Hints::default());
}

#[test]
fn grab_tempo_plays_a_sample_of_known_length_at_its_own_speed() {
    // A second and a half that lasts a bar is 160 BPM.
    let (mut app, _realtime) = app(72_000);
    command(&mut app, "sample bars 1");
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 160_000, "{}", app.status);
    assert!(app.status.contains("Tempo 160 BPM"), "{}", app.status);
    // Eight steps of it is half that.
    command(&mut app, "sample steps 8");
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 80_000);
    // A start and an end change it.
    command(&mut app, "sample end 1");
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 120_000);
}

#[test]
fn grab_tempo_on_an_unknown_loop_takes_the_nearest_power_of_two_and_u_takes_both_back() {
    // At 120 BPM, a second and a half is nearest a bar, at 160, rather than half one, at 80.
    let (mut app, _realtime) = app(72_000);
    assert_eq!(app.song().millibpm, 120_000);
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 160_000, "{}", app.status);
    assert_eq!(hints(&app).length_steps(), Some(16));
    assert!(app.status.contains("nearest"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(app.song().millibpm, 120_000);
    assert_eq!(hints(&app).length_steps(), None);
    // A tempo no song can have says so.
    let (mut short, _short) = crate::app(10);
    command(&mut short, "sample steps 4096");
    command(&mut short, "bpm sample");
    assert_eq!(short.song().millibpm, 120_000);
    assert!(
        short.status.contains("can't set a tempo"),
        "{}",
        short.status
    );
}

#[test]
fn grab_tempo_on_one_that_says_its_tempo_takes_that_and_u_takes_it_back() {
    // Seeded at 98.5 BPM, as an Octatrack file says, and no length: the song takes its tempo,
    // and its length stays unsaid.
    let (mut app, _realtime) = app_with(72_000, |entry| entry.edit.hints.millibpm = Some(98_500));
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 98_500, "{}", app.status);
    assert!(app.status.contains("Tempo 98.5 BPM"), "{}", app.status);
    assert_eq!(hints(&app).length_steps(), None);
    keys(&mut app, "u");
    assert_eq!(app.song().millibpm, 120_000);
    // One that says its length too goes by that.
    command(&mut app, "sample bars 1");
    command(&mut app, "bpm sample");
    assert_eq!(app.song().millibpm, 160_000);
}

#[test]
fn steps_and_warp_are_the_entrys_on_the_settings_row() {
    let (mut app, _realtime) = app(96_000);
    // K, then Steps, after Snap and Sens; its values say how fast each would play.
    keys(&mut app, "K j j Enter");
    assert_eq!(
        app.sample_page.setting,
        Some(SampleSetting::Steps),
        "{}",
        app.status
    );
    keys(&mut app, "+");
    assert_eq!(hints(&app).length_steps(), Some(16));
    assert!(
        app.status
            .contains("playing at 1.000x on Play's Tempo at 120 BPM"),
        "{}",
        app.status
    );
    keys(&mut app, "=");
    assert_eq!(hints(&app).steps, Some(17));
    keys(&mut app, "x");
    assert_eq!(hints(&app).length_steps(), None);
    // Warp is next, and goes between its speed and its sounds.
    keys(&mut app, "l =");
    assert_eq!(app.sample_page.setting, Some(SampleSetting::Warp));
    assert_eq!(app.song().bank.get(1).unwrap().edit.play.warp, Warp::Beats);
    assert!(app.status.contains("Warp Beats"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(
        app.song().bank.get(1).unwrap().edit.play.warp,
        Warp::Repitch
    );
}
