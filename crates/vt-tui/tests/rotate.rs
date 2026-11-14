//! Rotating the tracks' speeds, `Space o` and `Space O`, after the 4ms Shuffling Clock
//! Multiplier's Rotate: each track takes the next one's speed (decision 328).

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::action::Action;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Mark, Step};
use vt_tui::app::App;

mod common;
use common::*;

use Speed::{Eighth, EighthTriplet, Quarter, Sixteenth};

/// Four tracks at these speeds.
fn at_speeds(speeds: [Speed; 4]) -> (App, Realtime) {
    let mut song = Song::new(4);
    for (track, speed) in speeds.into_iter().enumerate() {
        song.tracks[track].speed = speed;
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

fn speeds(app: &App) -> Vec<Speed> {
    (0..4).map(|track| app.song().tracks[track].speed).collect()
}

#[test]
fn space_o_rotates_the_groups_speeds_and_as_many_presses_as_tracks_bring_them_back() {
    let (mut app, _rt) = at_speeds([Sixteenth, Eighth, Quarter, EighthTriplet]);
    press(&mut app, " o");
    assert!(
        app.services.status.contains("in no group"),
        "{}",
        app.services.status
    );
    assert_eq!(speeds(&app), [Sixteenth, Eighth, Quarter, EighthTriplet]);

    command(&mut app, "group 1 1-3");
    press(&mut app, " o");
    assert_eq!(
        speeds(&app),
        [Eighth, Quarter, Sixteenth, EighthTriplet],
        "each takes the next one's, the last the first's, and the fourth isn't in the group"
    );
    assert_eq!(
        app.services.status,
        "Speeds rotated: 01 1/8, 02 1/4, 03 1/16"
    );
    press(&mut app, " o o");
    assert_eq!(speeds(&app), [Sixteenth, Eighth, Quarter, EighthTriplet]);

    // One press is one undo step.
    press(&mut app, "u");
    assert_eq!(speeds(&app), [Quarter, Sixteenth, Eighth, EighthTriplet]);

    // A count rotates that many times, as one step.
    press(&mut app, "2 o");
    assert_eq!(speeds(&app), [Eighth, Quarter, Sixteenth, EighthTriplet]);
    press(&mut app, "u");
    assert_eq!(speeds(&app), [Quarter, Sixteenth, Eighth, EighthTriplet]);

    // From a group's own track, its tracks.
    let group = app
        .song()
        .fixed_track(vt_model::song::TrackKind::Group(1))
        .unwrap();
    while app.cursor.track != group {
        press(&mut app, "W");
    }
    press(&mut app, " o");
    assert_eq!(speeds(&app), [Sixteenth, Eighth, Quarter, EighthTriplet]);
}

#[test]
fn space_o_rotates_the_tracks_selected_and_space_capital_o_every_track() {
    let (mut app, _rt) = at_speeds([Sixteenth, Eighth, Quarter, EighthTriplet]);
    press(&mut app, "vWW o");
    assert_eq!(speeds(&app), [Eighth, Quarter, Sixteenth, EighthTriplet]);
    assert!(!app.mode.is_visual(), "the selection is left");

    press(&mut app, " O");
    assert_eq!(speeds(&app), [Quarter, Sixteenth, EighthTriplet, Eighth]);
    press(&mut app, " O O O");
    assert_eq!(speeds(&app), [Eighth, Quarter, Sixteenth, EighthTriplet]);

    // All at one speed, there's nothing to change.
    let (mut app, _rt) = at_speeds([Eighth; 4]);
    press(&mut app, " O");
    assert!(
        app.services.status.contains("changes nothing"),
        "{}",
        app.services.status
    );
    press(&mut app, "u");
    assert!(
        app.services.status.contains("Nothing to undo"),
        "{}",
        app.services.status
    );
}

#[test]
fn rotating_while_recording_writes_held_rates_on_each_tracks_next_row() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    app.services.apply(&Action::SetTrackSpeed {
        track: 1,
        speed: Eighth,
    });
    let rate = |app: &App, track: usize, row: usize| {
        let step = &rows_of(app, track)[row];
        step.fx
            .iter()
            .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SPEED)))
            .map(|slot| (Speed::from_index(usize::from(slot.coarse())), slot.mark))
    };
    let mut at = play_from_the_top(&mut app, &mut rt);
    press(&mut app, " r");
    assert!(app.recording());
    // Song step 2.5 is the first track's step 2.5 and the eighth track's 1.25.
    play_to(&mut app, &mut rt, &mut at, 2.5);
    press(&mut app, " O");
    assert_eq!(
        rate(&app, 0, 3),
        Some((Eighth, Mark::Hold)),
        "{}",
        app.services.status
    );
    assert_eq!(rate(&app, 1, 2), Some((Sixteenth, Mark::Hold)));
    assert_eq!(
        (app.song().tracks[0].speed, app.song().tracks[1].speed),
        (Sixteenth, Eighth),
        "the tracks' own speeds stay for the pass's start"
    );
    assert!(
        app.services.status.starts_with("RATE recorded"),
        "{}",
        app.services.status
    );

    // Heard from there: the first track plays eighths from its row 3.
    play_to(&mut app, &mut rt, &mut at, 4.5);
    assert_eq!(app.services.places[0].speed, Eighth);
    assert_eq!(app.services.places[1].speed, Sixteenth);

    // A second press turns them on from what the rows now hold, on the rows after.
    press(&mut app, " O");
    let next_first = (0..16).find(|row| *row > 3 && rate(&app, 0, *row).is_some());
    assert!(next_first.is_some(), "{:?}", rows_of(&app, 0));
    assert_eq!(
        rate(&app, 0, next_first.unwrap()),
        Some((Sixteenth, Mark::Hold))
    );

    // The pass is one undo step, its rates and all.
    press(&mut app, " r");
    press(&mut app, "u");
    assert!((0..16).all(|row| rate(&app, 0, row).is_none()));
    assert!((0..4).all(|row| rate(&app, 1, row).is_none()));
}

#[test]
fn rotating_back_before_the_row_lands_takes_the_rate_round_again() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1].speed = Quarter;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut rt) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press(&mut app, " r");
    play_to(&mut app, &mut rt, &mut at, 1.5);
    press(&mut app, " O");
    let held = |app: &App, track: usize, row: usize| rows_of(app, track)[row].fx[0].coarse();
    assert_eq!(held(&app, 0, 2), Quarter.index());
    assert_eq!(held(&app, 1, 1), Sixteenth.index());
    // Pressed again before those rows play, the rows' own rates are what turns.
    press(&mut app, " O");
    assert_eq!(held(&app, 0, 2), Sixteenth.index());
    assert_eq!(held(&app, 1, 1), Quarter.index());
}
