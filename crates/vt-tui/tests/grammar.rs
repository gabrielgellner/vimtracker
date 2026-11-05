//! Key sequences, starting and ending as step text.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::{App, Field};
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

const E: &str = "--- -- -- ---00 ---00 ---00";

fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
    song.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Type keys. `<Esc>` and `<Enter>` are written out, and `^x` means control.
fn keys(app: &mut App, text: &str) {
    let now = Instant::now();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("<Esc>") {
            app.key(Key::new(KeyCode::Esc), now);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<Enter>") {
            app.key(Key::new(KeyCode::Enter), now);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<BS>") {
            app.key(Key::new(KeyCode::Backspace), now);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('^') {
            let c = tail.chars().next().unwrap();
            app.key(
                Key {
                    code: KeyCode::Char(c),
                    ctrl: true,
                    alt: false,
                },
                now,
            );
            rest = &tail[c.len_utf8()..];
        } else {
            app.key(Key::char(c), now);
            rest = &rest[c.len_utf8()..];
        }
    }
}

fn steps(app: &App) -> Vec<String> {
    let id = app.phrase_id().unwrap();
    app.song().phrases[&id]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Run keys on steps and check the result.
#[track_caller]
fn check(start: &[&str], typed: &str, expected: &[&str]) {
    let (mut app, _rt) = app_with(start);
    keys(&mut app, typed);
    assert_eq!(steps(&app), expected, "after {typed:?}");
}

/// A phrase of `count` steps, with the given note on each.
fn notes(note: &str, count: usize) -> Vec<String> {
    vec![format!("{note} 40 01 CUT20 ---00 ---00"); count]
}

fn refs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

const A: &str = "C-4 40 01 CUT20 ---00 ---00";
const B: &str = "D-4 50 02 VOL30 ---00 ---00";
const C: &str = "E-4 60 03 ---00 PAN80 ---00";
const NOTE_GONE_A: &str = "--- 40 01 CUT20 ---00 ---00";
const NOTE_GONE_B: &str = "--- 50 02 VOL30 ---00 ---00";
const NOTE_GONE_C: &str = "--- 60 03 ---00 PAN80 ---00";

#[test]
fn x_clears_one_field() {
    check(&[A], "x", &[NOTE_GONE_A]);
    check(&[A], "lx", &["C-4 -- 01 CUT20 ---00 ---00"]);
    check(&[A], "llx", &["C-4 40 -- CUT20 ---00 ---00"]);
}

#[test]
fn clearing_either_part_of_a_slot_clears_the_slot() {
    check(&[A], "lllx", &["C-4 40 01 ---00 ---00 ---00"]);
    check(&[A], "llllx", &["C-4 40 01 ---00 ---00 ---00"]);
}

#[test]
fn dj_clears_a_column_not_whole_steps() {
    check(&[A, B, C], "dj", &[NOTE_GONE_A, NOTE_GONE_B, C]);
}

#[test]
fn a_count_extends_the_range() {
    check(
        &[A, B, C, A, B, C],
        "d4j",
        &[
            NOTE_GONE_A,
            NOTE_GONE_B,
            NOTE_GONE_C,
            NOTE_GONE_A,
            NOTE_GONE_B,
            C,
        ],
    );
    check(&[A, B, C], "jd2k", &[NOTE_GONE_A, NOTE_GONE_B, C]);
    // A count before the operator counts too.
    check(&[A, B, C], "2dj", &[NOTE_GONE_A, NOTE_GONE_B, NOTE_GONE_C]);
}

#[test]
fn doubled_operators_act_on_whole_steps() {
    check(&[A, B], "dd", &[E, B]);
    check(&[A, B, C], "2dd", &[E, E, C]);
    check(&[A, B], "j>>", &[A, "D#4 50 02 VOL30 ---00 ---00"]);
}

#[test]
fn targets_cover_a_beat_a_column_or_the_phrase() {
    let start = notes("C-4", 8);
    let start = refs(&start);
    let mut beat = vec![E; 4];
    beat.extend(vec![A; 4]);
    check(&start, "dib", &beat);
    check(&start, "jjdib", &beat);
    check(&start, "2dib", &[E; 8]);
    check(&start, "d2ib", &[E; 8]);
    check(&start, "dip", &[E; 8]);
    check(&start, "dic", &[NOTE_GONE_A; 8]);
    check(&start, "lllldic", &["C-4 40 01 ---00 ---00 ---00"; 8]);
}

#[test]
fn it_isnt_a_target_yet() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "dit");
    assert_eq!(steps(&app), [A]);
    assert!(app.status.contains("isn't bound"), "{}", app.status);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn shifting_moves_notes_and_values_and_skips_the_rest() {
    let off = "OFF -- -- ---00 ---00 ---00";
    check(
        &[A, off, E],
        "2>j",
        &["C#4 40 01 CUT20 ---00 ---00", off, E],
    );
    // Whole steps move notes only.
    check(&[A], ">>", &["C#4 40 01 CUT20 ---00 ---00"]);
    // A volume column moves by 1, and an empty one is left alone.
    check(&[A, E], "l>j", &["C-4 41 01 CUT20 ---00 ---00", E]);
    // An FX value moves, and an empty slot is left alone.
    check(&[A], "lll>>", &["C#4 40 01 CUT20 ---00 ---00"]);
    check(&[A], "lll>j", &["C-4 40 01 CUT21 ---00 ---00"]);
    // Instrument IDs and shifts below the range are left alone or clamped.
    check(&[A], "ll>j", &[A]);
    check(
        &["C-0 00 -- ---00 ---00 ---00"],
        "<<",
        &["C-0 00 -- ---00 ---00 ---00"],
    );
}

#[test]
fn increment_takes_a_count_as_its_amount() {
    check(&[A], "3+", &["D#4 40 01 CUT20 ---00 ---00"]);
    check(&[A], "12-", &["C-3 40 01 CUT20 ---00 ---00"]);
    check(&[A], "l3+", &["C-4 43 01 CUT20 ---00 ---00"]);
    check(&[A], "ll+", &["C-4 40 02 CUT20 ---00 ---00"]);
    check(&[A], "lll+", &["C-4 40 01 CUT21 ---00 ---00"]);
    check(&[A], "^a", &["C#4 40 01 CUT20 ---00 ---00"]);
    check(&[A], "^x", &["B-3 40 01 CUT20 ---00 ---00"]);
    // Values clamp to their ranges.
    check(
        &["C-4 FF -- VOLFF ---00 ---00"],
        "l9+",
        &["C-4 FF -- VOLFF ---00 ---00"],
    );
    check(
        &["G-9 -- -- ---00 ---00 ---00"],
        "+",
        &["G-9 -- -- ---00 ---00 ---00"],
    );
}

#[test]
fn paste_overwrites_from_the_cursor_and_clips_at_the_end() {
    check(&[A, B, E], "yyjjp", &[A, B, A]);
    check(&[A, B, E], "y2jjjp", &[A, B, "C-4 -- -- ---00 ---00 ---00"]);
    // A copied column pastes as a column, including its empty fields.
    check(&[E, A], "ykjp", &[E, NOTE_GONE_A]);
    check(&[A, E], "yjjp", &[A, "C-4 -- -- ---00 ---00 ---00"]);
    check(&[A, B], "yyjp", &[A, A]);
}

#[test]
fn paste_refuses_a_column_type_mismatch() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "y1kjlp");
    assert_eq!(steps(&app), [A, B]);
    assert!(app.status.contains("don't match"), "{}", app.status);
}

#[test]
fn deleting_fills_the_register() {
    check(&[A, E], "ddjp", &[E, A]);
}

#[test]
fn named_registers_keep_separate_contents() {
    check(&[A, B, E, E], "\"ayyj\"byyjj\"ap", &[A, B, E, A]);
    check(&[A, B, E, E], "\"ayyjyyjj\"ap", &[A, B, E, A]);
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "\"zp");
    assert!(app.status.contains("Nothing in register z"));
}

#[test]
fn dot_repeats_each_kind_of_change_at_the_cursor() {
    check(&[A, B, C], "ddj.", &[E, E, C]);
    check(&[A, B, C], "xj.", &[NOTE_GONE_A, NOTE_GONE_B, C]);
    check(
        &[A, B],
        ">>j.",
        &["C#4 40 01 CUT20 ---00 ---00", "D#4 50 02 VOL30 ---00 ---00"],
    );
    check(
        &[A, B],
        "2+j.",
        &["D-4 40 01 CUT20 ---00 ---00", "E-4 50 02 VOL30 ---00 ---00"],
    );
    check(&[A, B, E], "yyjjp.", &[A, B, A]);
    check(&[A, B, C], "dj", &[NOTE_GONE_A, NOTE_GONE_B, C]);
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, ".");
    assert!(app.status.contains("Nothing to repeat"));
}

#[test]
fn copying_isnt_repeated_by_dot() {
    check(&[A, B], "dd", &[E, B]);
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "xjyy.");
    assert_eq!(steps(&app), [NOTE_GONE_A, NOTE_GONE_B]);
}

#[test]
fn step_visual_selects_whole_steps() {
    check(&[A, B, C], "Vjd", &[E, E, C]);
    check(&[A, B, C], "jVkd", &[E, E, C]);
    check(
        &[A, B],
        "Vj>",
        &["C#4 40 01 CUT20 ---00 ---00", "D#4 50 02 VOL30 ---00 ---00"],
    );
}

#[test]
fn field_visual_selects_within_columns() {
    check(&[A, B], "vjd", &[NOTE_GONE_A, NOTE_GONE_B]);
    check(
        &[A, B],
        "vjlx",
        &["--- -- 01 CUT20 ---00 ---00", "--- -- 02 VOL30 ---00 ---00"],
    );
    check(
        &[A, B],
        "lvj3+",
        &["C-4 43 01 CUT20 ---00 ---00", "D-4 53 02 VOL30 ---00 ---00"],
    );
}

#[test]
fn a_target_in_visual_mode_selects_it() {
    let start = notes("C-4", 8);
    let start = refs(&start);
    let mut beat = vec![E; 4];
    beat.extend(vec![A; 4]);
    check(&start, "vibd", &beat);
    check(&start, "Vipd", &[E; 8]);
    check(&start, "vicd", &[NOTE_GONE_A; 8]);
}

#[test]
fn esc_leaves_visual_and_operator_pending_with_no_change() {
    check(&[A, B], "Vj<Esc>d<Esc>", &[A, B]);
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "V<Esc>");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "d<Esc>");
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn transposing_a_beat_then_repeating_on_the_next_beat() {
    // CUJ 2: select a beat, transpose it, go to the next beat, select it, repeat.
    let start = vec!["C-4 -- 01 ---00 ---00 ---00"; 4]
        .into_iter()
        .chain(vec!["D-4 -- 01 ---00 ---00 ---00"; 4])
        .collect::<Vec<_>>();
    let up_c = "C#4 -- 01 ---00 ---00 ---00";
    let up_d = "D#4 -- 01 ---00 ---00 ---00";
    let mut expected = vec![up_c; 4];
    expected.extend(vec![up_d; 4]);
    let (mut app, _rt) = app_with(&start);
    keys(&mut app, "Vib>");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "}Vib.");
    assert_eq!(steps(&app), expected);
    keys(&mut app, "u");
    assert_eq!(steps(&app)[4], start[4], "undo takes back one change");
    assert_eq!(steps(&app)[0], up_c);
}

#[test]
fn reselect_restores_the_last_selection() {
    let (mut app, _rt) = app_with(&[A, B, C]);
    keys(&mut app, "Vj>");
    keys(&mut app, "gv");
    assert_eq!(app.mode, Mode::VisualStep);
    assert_eq!(app.current_selection().range.last, 1);
    keys(&mut app, "<Esc>");
}

#[test]
fn motions_move_the_cursor_and_stop_at_edges() {
    let start = notes("C-4", 16);
    let (mut app, _rt) = app_with(&refs(&start));
    keys(&mut app, "}");
    assert_eq!(app.cursor.step, 4);
    keys(&mut app, "2}");
    assert_eq!(app.cursor.step, 12);
    keys(&mut app, "{");
    assert_eq!(app.cursor.step, 8);
    keys(&mut app, "G");
    assert_eq!(app.cursor.step, 15);
    keys(&mut app, "gg");
    assert_eq!(app.cursor.step, 0);
    keys(&mut app, "5G");
    assert_eq!(app.cursor.step, 4);
    keys(&mut app, "99j");
    assert_eq!(app.cursor.step, 15);
    keys(&mut app, "h");
    assert_eq!(app.cursor.field, Field::Note);
    keys(&mut app, "w");
    assert_eq!(app.cursor.field, Field::Volume);
    keys(&mut app, "ww");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    keys(&mut app, "l");
    assert_eq!(app.cursor.field, Field::FxValue(0));
    keys(&mut app, "w");
    assert_eq!(app.cursor.field, Field::FxCommand(1));
    keys(&mut app, "b");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    keys(&mut app, "20l");
    assert_eq!(app.cursor.field, Field::FxValue(2));
}

#[test]
fn viewport_keys_move_within_the_screen() {
    let start = notes("C-4", 64);
    let (mut app, _rt) = app_with(&refs(&start));
    app.follow_cursor(20);
    keys(&mut app, "^d");
    assert_eq!(app.cursor.step, 10);
    keys(&mut app, "^u");
    assert_eq!(app.cursor.step, 0);
    keys(&mut app, "L");
    assert_eq!(app.cursor.step, 19);
    keys(&mut app, "H");
    assert_eq!(app.cursor.step, 0);
    keys(&mut app, "M");
    assert_eq!(app.cursor.step, 10);
    keys(&mut app, "^e");
    assert_eq!(app.top, 3);
    assert_eq!(app.cursor.step, 10, "the cursor stays while it's on screen");
    keys(&mut app, "H^y");
    assert_eq!(app.top, 0);
    assert_eq!(app.cursor.step, 3);
}

#[test]
fn value_entry_commits_two_digits_and_moves_down() {
    check(&[E, E], "li7f", &["--- 7F -- ---00 ---00 ---00", E]);
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "liaB<Esc>");
    assert_eq!(steps(&app)[0], "--- AB -- ---00 ---00 ---00");
    assert_eq!(app.cursor.step, 1);
}

#[test]
fn partly_typed_values_are_dropped_on_esc() {
    check(&[E], "li7<Esc>", &[E]);
}

#[test]
fn instrument_entry_commits_on_enter_and_becomes_current() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lli1a2<Enter><Esc>");
    assert_eq!(steps(&app)[0], "--- -- 1A2 ---00 ---00 ---00");
    assert_eq!(app.instrument, 0x1A2);
}

#[test]
fn a_typed_command_takes_effect_only_with_its_value() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llliCUT");
    assert_eq!(
        steps(&app)[0],
        E,
        "CUT must not be written before its value"
    );
    assert_eq!(app.cursor.field, Field::FxValue(0));
    keys(&mut app, "40<Esc>");
    assert_eq!(steps(&app)[0], "--- -- -- CUT40 ---00 ---00");
}

#[test]
fn lowercase_commands_are_uppercased() {
    check(&[E], "llliret14<Esc>", &["--- -- -- RET14 ---00 ---00"]);
}

#[test]
fn an_invalid_command_is_refused() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "llliXYZ");
    assert_eq!(steps(&app), [A]);
    assert!(app.status.contains("isn't an FX command"));
    assert_eq!(app.cursor.field, Field::FxCommand(0));
}

#[test]
fn esc_after_a_command_writes_nothing() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "llliVOL<Esc>");
    assert_eq!(steps(&app), [A]);
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert!(app.status.contains("needs a value"));
}

#[test]
fn a_value_needs_a_command() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "lllli40");
    assert_eq!(steps(&app), [E]);
    assert!(app.status.contains("FX command first"));
}

#[test]
fn a_value_on_an_existing_command_keeps_the_command() {
    check(&[A], "lllli7F<Esc>", &["C-4 40 01 CUT7F ---00 ---00"]);
}

#[test]
fn the_step_setting_changes_how_far_entry_moves() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":set step=2<Enter>ia");
    assert_eq!(app.cursor.step, 2);
}

#[test]
fn doubling_repeats_visible_steps_as_one_undo_step() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, " pd");
    assert_eq!(steps(&app), [A, B, A, B]);
    keys(&mut app, ":double<Enter>");
    assert_eq!(steps(&app), [A, B, A, B, A, B, A, B]);
    keys(&mut app, "uu");
    assert_eq!(steps(&app), [A, B]);
}

#[test]
fn undo_takes_back_a_whole_operator() {
    check(&[A, B, C], "2dju", &[A, B, C]);
}

#[test]
fn the_popup_waits_and_lists_next_keys() {
    let (mut app, _rt) = app_with(&[A]);
    let start = Instant::now();
    app.key(Key::char(' '), start);
    let delay = Duration::from_millis(300);
    assert!(app.popup_rows(start, delay).is_empty());
    let rows = app.popup_rows(start + delay, delay);
    assert!(rows.iter().any(|(k, _)| k == "x"));
    assert!(rows.iter().any(|(k, _)| k == "p d"));
}

#[test]
fn help_lists_keys_from_the_table_and_any_key_closes_it() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, " k");
    assert_eq!(app.mode, Mode::Help);
    let rows = app.help_rows();
    assert!(
        rows.iter()
            .any(|r| r.contains("Space x") && r.contains("Pause"))
    );
    assert!(rows.iter().any(|r| r.contains("i b")));
    keys(&mut app, "j");
    assert_eq!(app.help_scroll, 1);
    keys(&mut app, "q");
    assert_eq!(app.mode, Mode::Normal);
}
