//! The mouse wheel and the trackpad as a knob on the cursor's setting (#425), through the
//! terminal's mouse reports as crossterm reads them: a tick is `=` or `-` where the cursor is, in
//! the setting's finest step, Shift or sideways is `+` or `_`, a gesture is one undo step, and
//! nothing is captured or done until it's turned on.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::{App, Field};
use vt_tui::audio_page::AudioRow;
use vt_tui::input::mode::{Mode, View};
use vt_tui::wheel::{WheelSetting, tick_size};

mod common;
use common::*;

const NOTE: &str = "C-3 40 cut:20 --- ---";

/// An app on a phrase of these steps, with the wheel turned on.
fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    app.wheel.setting.on = true;
    (app, realtime)
}

fn steps(app: &App) -> Vec<String> {
    let id = app.phrase_id().unwrap();
    app.song().tracks[0].phrases[&id]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// A mouse report of this kind, with Shift or not, at the screen's corner, as where the pointer
/// is doesn't matter.
fn report(kind: MouseEventKind, shift: bool) -> MouseEvent {
    MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: if shift {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        },
    }
}

/// Scroll a tick up or down at `at`.
fn scroll(app: &mut App, up: bool, at: Instant) {
    let kind = if up {
        MouseEventKind::ScrollUp
    } else {
        MouseEventKind::ScrollDown
    };
    app.mouse_event(report(kind, false), at);
}

/// Ticks a second apart, each slow enough to be the finest step, all in one gesture.
fn slow_ticks(app: &mut App, up: bool, count: usize, from: Instant) -> Instant {
    let mut at = from;
    for _ in 0..count {
        scroll(app, up, at);
        at += Duration::from_millis(150);
    }
    at
}

#[test]
fn the_wheel_is_off_until_turned_on_on_the_audio_page_and_kept_for_the_machine() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    app.wheel.setting.on = false;
    let folder = folder();
    let path = folder.join("vimtracker").join(vt_tui::wheel::SETTING_FILE);
    app.wheel.setting_path = Some(path.clone());
    assert!(!app.wants_mouse(), "nothing is captured as it starts");
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "a report while it's off does nothing");
    // Its row is after the buffer, before the Roto's.
    press(&mut app, " 7jjj");
    assert_eq!(app.audio_row_here(), AudioRow::Wheel);
    press(&mut app, "=");
    assert!(app.wants_mouse());
    assert!(app.status.contains("Shift-drag"), "{}", app.status);
    assert_eq!(WheelSetting::load_from(&path), WheelSetting { on: true });
    press(&mut app, "-");
    assert!(!app.wants_mouse());
    assert_eq!(WheelSetting::load_from(&path), WheelSetting { on: false });
    press(&mut app, "K");
    assert!(app.wants_mouse(), "K switches it");
    press(&mut app, "x");
    assert!(!app.wants_mouse(), "x puts it back off");
    std::fs::write(&path, "(on: maybe)").unwrap();
    assert_eq!(
        WheelSetting::load_from(&path),
        WheelSetting { on: false },
        "a file that won't read leaves it off"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn up_raises_down_lowers_and_shift_or_sideways_takes_the_large_step() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let now = Instant::now();
    scroll(&mut app, true, now);
    assert_eq!(steps(&app), ["C#3 40 cut:20 --- ---"]);
    scroll(&mut app, false, now);
    scroll(&mut app, false, now);
    assert_eq!(steps(&app), ["B-2 40 cut:20 --- ---"]);
    app.mouse_event(report(MouseEventKind::ScrollUp, true), now);
    assert_eq!(steps(&app), ["B-3 40 cut:20 --- ---"], "Shift is +");
    app.mouse_event(report(MouseEventKind::ScrollLeft, false), now);
    assert_eq!(steps(&app), ["B-2 40 cut:20 --- ---"], "left is _");
    app.mouse_event(report(MouseEventKind::ScrollRight, false), now);
    assert_eq!(steps(&app), ["B-3 40 cut:20 --- ---"], "right is +");
    // The cutoff's value, a hex digit's large step.
    press(&mut app, "llllll");
    assert_eq!(app.cursor.field, Field::FxValue(0));
    app.mouse_event(report(MouseEventKind::ScrollDown, true), now);
    assert_eq!(steps(&app), ["B-3 40 cut:10 --- ---"]);
}

#[test]
fn clicks_and_drags_do_nothing() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let depth = app.history.undo_depth();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Moved,
    ] {
        app.mouse_event(report(kind, false), Instant::now());
    }
    assert_eq!(steps(&app), [NOTE]);
    assert_eq!(app.history.undo_depth(), depth);
    assert_eq!(app.cursor.field, Field::Note, "the cursor stays");
}

#[test]
fn a_gesture_is_one_undo_step_ended_by_a_pause_or_a_key() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let depth = app.history.undo_depth();
    let start = Instant::now();
    let end = slow_ticks(&mut app, true, 5, start);
    assert_eq!(steps(&app), ["F-3 40 cut:20 --- ---"]);
    // Still turning: a frame within the pause leaves it open.
    app.tick_at(end);
    app.tick_at(end + Duration::from_millis(400));
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    // A second turn after the pause is a step of its own.
    slow_ticks(&mut app, true, 2, end + Duration::from_secs(1));
    app.tick_at(end + Duration::from_secs(3));
    assert_eq!(app.history.undo_depth(), depth + 2);
    assert_eq!(steps(&app), ["G-3 40 cut:20 --- ---"]);
    press(&mut app, "u");
    assert_eq!(steps(&app), ["F-3 40 cut:20 --- ---"]);
    press(&mut app, "u");
    assert_eq!(steps(&app), [NOTE], "the first turn, all at once");
    // A key ends a turn at once, with no pause.
    let later = Instant::now() + Duration::from_secs(10);
    slow_ticks(&mut app, false, 3, later);
    press(&mut app, "j");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "ku");
    assert_eq!(steps(&app), [NOTE]);
}

#[test]
fn a_tick_on_an_fx_value_moves_its_fine_digits_and_quick_ticks_more_of_them() {
    let (mut app, _realtime) = app_with(&["--- -- cut:40 --- ---"]);
    press(&mut app, "llllll");
    assert_eq!(app.cursor.field, Field::FxValue(0));
    let start = Instant::now();
    let end = slow_ticks(&mut app, true, 2, start);
    assert_eq!(steps(&app), ["--- -- cut:4002 --- ---"]);
    assert_eq!(app.cursor.field, Field::FxValue(0), "the cursor stays");
    slow_ticks(&mut app, false, 3, end);
    assert_eq!(steps(&app), ["--- -- cut:3FFF --- ---"], "it carries");
    // Shift is `+` where the cursor is, a step of the value.
    app.mouse_event(
        report(MouseEventKind::ScrollUp, true),
        end + Duration::from_secs(1),
    );
    assert_eq!(steps(&app), ["--- -- cut:4FFF --- ---"]);
    // A burst grows by the square of its speed, up to one of `=`'s steps a tick.
    let (mut app, _realtime) = app_with(&["--- -- cut:40 --- ---"]);
    press(&mut app, "llllll");
    let burst = Instant::now();
    for _ in 0..20 {
        scroll(&mut app, true, burst);
    }
    let moved: u64 = (1..=20).map(|recent| tick_size(recent, 0x100)).sum();
    assert_eq!(moved, 0x9D8);
    assert_eq!(steps(&app), ["--- -- cut:49D8 --- ---"]);
}

#[test]
fn a_tick_on_a_sound_page_setting_moves_its_fine_digits() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    // Cut, the Filter's first setting, open at the top.
    press(&mut app, " 4}j");
    assert_eq!(app.view, View::Track);
    let cut = |app: &App| {
        let track = &app.song().tracks[0];
        (track.cutoff, track.fine(SettingId::CUT))
    };
    assert_eq!(cut(&app), (0xFF, 0));
    let end = slow_ticks(&mut app, false, 2, Instant::now());
    assert_eq!(cut(&app), (0xFE, 0xFE));
    assert!(!app.track_page.fine, "the cursor stays on the value");
    // Shift is `_` on the value's own digits, keeping the fine part.
    app.mouse_event(report(MouseEventKind::ScrollDown, true), end);
    assert_eq!(cut(&app), (0xEE, 0xFE));
    // On the fine digits already, it's the same.
    press(&mut app, "l");
    assert!(app.track_page.fine);
    slow_ticks(&mut app, true, 2, end + Duration::from_secs(1));
    assert_eq!(cut(&app), (0xEF, 0));
    assert!(app.track_page.fine);
}

#[test]
fn a_tick_on_the_tempo_moves_its_fine_part() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, " 9ggjj");
    assert_eq!(
        app.project_control(),
        vt_tui::project_page::ProjectControl::Bpm
    );
    let end = slow_ticks(&mut app, true, 3, Instant::now());
    assert_eq!(app.song().millibpm, 120_030, "hundredths of a BPM");
    assert!(!app.project_fine);
    app.mouse_event(report(MouseEventKind::ScrollUp, true), end);
    assert_eq!(app.song().millibpm, 130_030, "Shift is +, 10 BPM");
}

#[test]
fn a_tick_moves_a_locked_selection_and_a_gesture_is_one_undo_with_its_total() {
    let (mut app, _realtime) = app_with(&[NOTE, NOTE, NOTE, NOTE]);
    press(&mut app, "Vjj");
    let depth = app.history.undo_depth();
    let end = slow_ticks(&mut app, true, 3, Instant::now());
    let up = "D#3 40 cut:20 --- ---";
    assert_eq!(steps(&app), [up, up, up, NOTE]);
    assert_eq!(app.mode, Mode::VisualStep, "it stays selected");
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
    app.tick_at(end + Duration::from_secs(1));
    assert_eq!(app.history.undo_depth(), depth + 1);
    // A second turn, then undo takes back each turn whole, the total with it.
    slow_ticks(&mut app, false, 1, end + Duration::from_secs(2));
    app.tick_at(end + Duration::from_secs(3));
    assert_eq!(app.adjusted_total().as_deref(), Some("+2 semitones"));
    press(&mut app, "u");
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
    press(&mut app, "u");
    assert_eq!(steps(&app), [NOTE, NOTE, NOTE, NOTE]);
    assert_eq!(app.mode, Mode::VisualStep);
    let redo = vt_tui::input::keys::Key {
        ctrl: true,
        ..vt_tui::input::keys::Key::char('r')
    };
    app.key(redo, Instant::now());
    assert_eq!(
        steps(&app),
        [up, up, up, NOTE],
        "redo brings the turn back whole"
    );
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
}

#[test]
fn the_wheel_waits_for_keys_typed_before_it_and_types_nothing() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, "d");
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "an operator waiting for its motion");
    assert_eq!(app.mode, Mode::OperatorPending);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Insert);
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "Insert has no = to stand for");
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn dot_repeats_the_last_keys_change_not_the_wheels() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, "=");
    slow_ticks(&mut app, true, 2, Instant::now());
    assert_eq!(steps(&app), ["D#3 40 cut:20 --- ---"]);
    press(&mut app, ".");
    assert_eq!(steps(&app), ["E-3 40 cut:20 --- ---"]);
}
