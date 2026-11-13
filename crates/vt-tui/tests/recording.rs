//! Recording, `Space r`: notes played while playing go on their rows, timed by the frames played.

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;

mod common;
use common::*;

/// A recorded note's gate cell, as its pulses and its length.
fn length_of(step: &Step) -> Option<(u8, Option<u8>)> {
    step.gate.map(|gate| (gate.pulses, gate.length))
}

#[test]
fn space_r_arms_recording_only_while_playing_and_stopping_or_leaving_disarms() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    press(&mut app, " r");
    assert!(!app.recording());
    assert_eq!(app.status, "Recording arms while playing");

    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    assert!(app.recording());
    let (play, record) = (vt_tui::glyphs::PLAY, vt_tui::glyphs::RECORD);
    assert!(
        status_line(&app).contains(&format!(" {play} {record}  ")),
        "{}",
        status_line(&app)
    );
    press_at(&mut app, at, " r");
    assert!(!app.recording(), "it toggles");
    assert!(!status_line(&app).contains(record));
    assert!(status_line(&app).contains(&format!(" {play}  ")));

    // Pausing stops it.
    let tab = Key::new(KeyCode::Tab);
    press_at(&mut app, at, " r");
    app.key(tab, time_at(at));
    app.key_released(tab, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 1.0);
    assert_eq!(app.transport, Transport::Paused);
    assert!(!app.recording());

    // So does leaving the phrase view.
    app.key(tab, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 2.0);
    press_at(&mut app, at, " r");
    assert!(app.recording());
    press_at(&mut app, at, " 2");
    assert!(!app.recording());
}

#[test]
fn a_recorded_note_goes_on_the_row_its_own_track_is_playing_as_long_as_its_held() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 9 of the song is row 1 of the 4-row phrase. A quarter into it goes on it, nudged late
    // by a quarter of a step, six 24ths.
    play_to(&mut app, &mut rt, &mut at, 9.25);
    let a = Key::char('a');
    app.key(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[1].velocity, Some(app.note_velocity));
    assert_eq!(rows[1].nudge, 6);
    assert_eq!(app.cursor.step, 0, "the cursor stays");
    // Until it's let go, it holds on the row's pulse until something ends it.
    assert_eq!(length_of(&rows[1]), Some((1, Some(0xFF))));

    // Held for two steps, past its row's end, so it rings on, and the row keeps its pulse.
    play_to(&mut app, &mut rt, &mut at, 11.25);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(length_of(&rows[1]), Some((1, Some(0xFF))), "{rows:?}");
    assert!(
        rows[1].to_string().starts_with("C-3 64 +6 SGL1 FF "),
        "{}",
        rows[1]
    );
    assert!(
        rows[1].fx.iter().all(|slot| slot.command.is_none()),
        "no OFF"
    );
    assert_eq!(
        rows.iter().map(Step::pulses).sum::<usize>(),
        4,
        "the loop keeps its length"
    );
    assert_eq!(
        rows.iter().filter(|step| step.note != Note::Empty).count(),
        1
    );
    assert!(
        rows_of(&app, 0).iter().all(|step| *step == Step::default()),
        "the longer track is left alone"
    );
}

#[test]
fn a_note_played_late_in_a_step_waits_for_the_next_row() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 9.75);
    let s = Key::char('s');
    app.key(s, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 9.9);
    app.key_released(s, time_at(at));
    assert!(
        rows_of(&app, 1).iter().all(|step| step.note == Note::Empty),
        "not written before its row, which would play it twice"
    );
    play_to(&mut app, &mut rt, &mut at, 10.1);
    let rows = rows_of(&app, 1);
    assert_eq!(rows[2].note, Note::On(62), "{rows:?}");
    assert_eq!(rows[2].nudge, -6, "a quarter of a step early");
    // Held for 0.15 of a step.
    let (pulses, length) = length_of(&rows[2]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x26));
}

#[test]
fn a_note_held_until_its_row_comes_round_ends_there() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 9.0);
    let a = Key::char('a');
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 13.5);
    // Letting go later changes nothing.
    play_to(&mut app, &mut rt, &mut at, 14.5);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60));
    assert_eq!(
        length_of(&rows[1]),
        Some((1, Some(0xFF))),
        "held past its row, it rings on"
    );
}

#[test]
fn a_recorded_note_on_a_long_row_keeps_its_pulses_and_lasts_into_the_last() {
    let rows: Vec<Step> = ["--- -- SGL2", "--- --", "--- --"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    let (mut app, mut rt) = recording_app(rows);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 8 starts the 4-step loop's row 0, which lasts two.
    let a = Key::char('a');
    play_to(&mut app, &mut rt, &mut at, 8.0);
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 9.5);
    app.key_released(a, time_at(at));
    let (pulses, length) = length_of(&rows_of(&app, 1)[0]).unwrap();
    assert_eq!(pulses, 2);
    assert_eq!(length, Some(0x80), "half of its second pulse");

    // A tap lasts into its last pulse as little as the cell says, and the row stays as long.
    play_to(&mut app, &mut rt, &mut at, 12.0);
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 12.25);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(length_of(&rows[0]), Some((2, Some(0x00))), "{rows:?}");
    assert_eq!(rows.iter().map(Step::pulses).sum::<usize>(), 4);
}

#[test]
fn recording_overwrites_only_the_note_velocity_and_gate_and_a_pass_is_one_undo() {
    let rows: Vec<Step> = ["E-3 7F", "C-2 10 RST1 cut:40", "--- --", "--- --"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    let (mut app, mut rt) = recording_app(rows.clone());
    let depth = app.history.undo_depth();
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    for (from, key) in [(9.0, 'a'), (11.0, 'd')] {
        play_to(&mut app, &mut rt, &mut at, from);
        app.key(Key::char(key), time_at(at));
        play_to(&mut app, &mut rt, &mut at, from + 0.5);
        app.key_released(Key::char(key), time_at(at));
    }
    let written = rows_of(&app, 1);
    assert_eq!(written[0], rows[0], "a row not played into stays");
    assert_eq!(written[1].note, Note::On(60));
    assert_eq!(written[1].velocity, Some(app.note_velocity));
    assert_eq!(
        written[1].gate_kind(),
        vt_model::step::GateKind::Single,
        "a rest would silence it"
    );
    assert_eq!(
        written[1].pulses(),
        1,
        "held for half a step, the row lasts as long as it did"
    );
    assert_eq!(written[1].fx, rows[1].fx, "its FX stay");
    assert_eq!(written[1].length(), Some(0x80));
    assert_eq!(written[3].note, Note::On(64));
    assert_eq!(app.history.undo_depth(), depth + 1, "one step for the pass");

    special_at(&mut app, at, KeyCode::Esc);
    press_at(&mut app, at, " r");
    press_at(&mut app, at, "u");
    assert_eq!(rows_of(&app, 1), rows);
}

#[test]
fn a_held_note_when_recording_stops_keeps_its_length_so_far() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 8.0);
    app.key(Key::char('a'), time_at(at));
    play_to(&mut app, &mut rt, &mut at, 8.5);
    special_at(&mut app, at, KeyCode::Esc);
    press_at(&mut app, at, " r");
    assert!(!app.recording());
    let (pulses, length) = length_of(&rows_of(&app, 1)[0]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x80), "half a step so far");
}

#[test]
fn a_recorded_note_goes_on_the_row_nearest_where_the_groove_lands_it() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    // Every other step lands 96 ticks late, 0.4 of a step.
    command(&mut app, "swing 70");
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 9 of the song is row 1, which lands at 9.4. At 9.65 it's nearer than row 2, at 10,
    // though on the grid row 2 would be.
    play_to(&mut app, &mut rt, &mut at, 9.65);
    let a = Key::char('a');
    app.key(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[2].note, Note::Empty);
    assert_eq!(
        rows[1].nudge, 6,
        "a quarter of a step after where the groove lands it"
    );
    // Let go at 9.8, which is a quarter of row 1's step as the groove makes it, 0.6 long.
    play_to(&mut app, &mut rt, &mut at, 9.8);
    app.key_released(a, time_at(at));
    let (pulses, length) = length_of(&rows_of(&app, 1)[1]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x40));

    // A note just before a late row's landing goes on it, and waits for it to land.
    play_to(&mut app, &mut rt, &mut at, 11.3);
    let s = Key::char('s');
    app.key(s, time_at(at));
    assert_eq!(rows_of(&app, 1)[3].note, Note::Empty, "not before it lands");
    play_to(&mut app, &mut rt, &mut at, 11.5);
    app.key_released(s, time_at(at));
    assert_eq!(rows_of(&app, 1)[3].note, Note::On(62));
    assert_eq!(
        rows_of(&app, 1)[3].nudge,
        -2,
        "a tenth of a step before it lands"
    );
}

#[test]
fn a_recorded_note_goes_on_the_row_of_its_tracks_own_speed() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![Step::default(); 4])));
    song.tracks[1].speed = vt_model::speed::Speed::Eighth;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut rt) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "W");
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Song step 9.5 is the eighth track's step 4.75, so row 1's start, at its step 5, is nearer
    // than row 0's, and the note is a quarter of an eighth early.
    play_to(&mut app, &mut rt, &mut at, 9.5);
    let a = Key::char('a');
    app.key(a, time_at(at));
    assert!(
        rows_of(&app, 1).iter().all(|step| step.note == Note::Empty),
        "not before its row lands"
    );
    // Let go half an eighth later, at its step 5.25, so it lasts half its row's step.
    play_to(&mut app, &mut rt, &mut at, 10.5);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[1].nudge, -6);
    let (pulses, length) = length_of(&rows[1]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x80));
}
