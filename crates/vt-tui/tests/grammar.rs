//! Key sequences, starting and ending as step text.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::constants::MAX_PHRASE_STEPS;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::{App, Field};
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

const E: &str = "--- -- ---00 ---00 ---00";

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
                    shift: false,
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
    vec![format!("{note} 40 CUT20 ---00 ---00"); count]
}

fn refs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

const A: &str = "C-4 40 CUT20 ---00 ---00";
const B: &str = "D-4 50 VOL30 ---00 ---00";
const C: &str = "E-4 60 ---00 PAN80 ---00";
const NOTE_GONE_A: &str = "--- 40 CUT20 ---00 ---00";
const NOTE_GONE_B: &str = "--- 50 VOL30 ---00 ---00";
const NOTE_GONE_C: &str = "--- 60 ---00 PAN80 ---00";

#[test]
fn x_clears_one_field() {
    check(&[A], "x", &[NOTE_GONE_A]);
    check(&[A], "lx", &["C-4 -- CUT20 ---00 ---00"]);
}

#[test]
fn clearing_either_part_of_a_slot_clears_the_slot() {
    check(&[A], "llx", &["C-4 40 ---00 ---00 ---00"]);
    check(&[A], "lllx", &["C-4 40 ---00 ---00 ---00"]);
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
    check(&[A, B], "j>>", &[A, "D#4 50 VOL30 ---00 ---00"]);
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
    check(&start, "llldic", &["C-4 40 ---00 ---00 ---00"; 8]);
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
    let off = "OFF -- ---00 ---00 ---00";
    check(&[A, off, E], "2>j", &["C#4 40 CUT20 ---00 ---00", off, E]);
    // Whole steps move notes only.
    check(&[A], ">>", &["C#4 40 CUT20 ---00 ---00"]);
    // A volume column moves by 1, and an empty one is left alone.
    check(&[A, E], "l>j", &["C-4 41 CUT20 ---00 ---00", E]);
    // An FX value moves, and an empty slot is left alone.
    check(&[A], "ll>>", &["C#4 40 CUT20 ---00 ---00"]);
    check(&[A], "ll>j", &["C-4 40 CUT21 ---00 ---00"]);
    // Shifts below the range are clamped.
    check(
        &["C-0 00 ---00 ---00 ---00"],
        "<<",
        &["C-0 00 ---00 ---00 ---00"],
    );
}

#[test]
fn increment_takes_a_count_as_its_amount() {
    check(&[A], "3=", &["D#4 40 CUT20 ---00 ---00"]);
    check(&[A], "12-", &["C-3 40 CUT20 ---00 ---00"]);
    check(&[A], "l3=", &["C-4 43 CUT20 ---00 ---00"]);
    check(&[A], "ll=", &["C-4 40 CUT21 ---00 ---00"]);
    check(&[A], "^a", &["C#4 40 CUT20 ---00 ---00"]);
    check(&[A], "^x", &["B-3 40 CUT20 ---00 ---00"]);
    // Values clamp to their ranges.
    check(
        &["C-4 7F VOLFF ---00 ---00"],
        "l9+",
        &["C-4 7F VOLFF ---00 ---00"],
    );
    check(
        &["G-9 -- ---00 ---00 ---00"],
        "+",
        &["G-9 -- ---00 ---00 ---00"],
    );
}

#[test]
fn paste_overwrites_from_the_cursor() {
    check(&[A, B, E], "yyjjp", &[A, B, A]);
    // A copied column pastes as a column, including its empty fields.
    check(&[E, A], "ykjp", &[E, NOTE_GONE_A]);
    check(&[A, B], "yyjp", &[A, A]);
}

#[test]
fn a_count_stamps_the_copy_end_to_end() {
    check(&[A, E, E, E, E], "yyj3p", &[A, A, A, A, E]);
    check(&[A, B, E, E, E, E, E], "Vjyggjj2p", &[A, B, A, B, A, B, E]);
}

#[test]
fn paste_past_the_end_stores_more_and_leaves_the_loop_alone() {
    let (mut app, _rt) = app_with(&[A, E, E]);
    keys(&mut app, "yyjj9p");
    assert_eq!(steps(&app).len(), 11);
    assert_eq!(app.song().phrases[&0].len(), 3, "the loop hasn't changed");
    keys(&mut app, "u");
    assert_eq!(steps(&app).len(), 3);
    // A huge count is no trouble.
    keys(&mut app, "99999999p");
    assert_eq!(steps(&app).len(), MAX_PHRASE_STEPS);
    assert_eq!(app.song().phrases[&0].len(), 3);
}

#[test]
fn a_counted_paste_is_one_undo_and_dot_repeats_the_count() {
    check(&[A, E, E, E], "yyj3pu", &[A, E, E, E]);
    check(&[A, E, E, E, E, E], "yyj2pjjj.", &[A, A, A, E, A, A]);
}

#[test]
fn gp_lands_just_after_what_it_pasted() {
    let (mut app, _rt) = app_with(&[A, B, E, E, E, E, E, E]);
    keys(&mut app, "Vjyggjjgp");
    assert_eq!(app.cursor.step, 4);
    keys(&mut app, "gp");
    assert_eq!(app.cursor.step, 6);
    assert_eq!(steps(&app), [A, B, A, B, A, B, E, E]);
    keys(&mut app, ".");
    assert_eq!(app.cursor.step, 7, "stays on the phrase");
    assert_eq!(steps(&app), [A, B, A, B, A, B, A, B]);

    let (mut app, _rt) = app_with(&[A, E, E, E, E]);
    keys(&mut app, "yyj2gp");
    assert_eq!(app.cursor.step, 3, "a count moves past all of it");
}

#[test]
fn gp_doesnt_move_when_the_paste_is_refused() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "y1kjlgp");
    assert_eq!(app.cursor.step, 1);
    assert_eq!(steps(&app), [A, B]);
}

#[test]
fn gm_merges_and_leaves_what_the_copy_leaves_empty() {
    const KICK: &str = "C-4 40 ---00 ---00 ---00";
    const HAT: &str = "--- -- ---00 PAN80 ---00";
    const BOTH: &str = "C-4 40 ---00 PAN80 ---00";
    // Whole steps.
    check(&[HAT, KICK], "yyjgm", &[HAT, BOTH]);
    // A plain paste of the same thing wipes the kick.
    check(&[HAT, KICK], "yyjp", &[HAT, HAT]);
    // A full field wins, and OFF counts as full.
    check(
        &["D-4 -- ---00 ---00 ---00", KICK],
        "yyjgm",
        &["D-4 -- ---00 ---00 ---00", "D-4 40 ---00 ---00 ---00"],
    );
    check(
        &["OFF -- ---00 ---00 ---00", KICK],
        "yyjgm",
        &["OFF -- ---00 ---00 ---00", "OFF 40 ---00 ---00 ---00"],
    );
    // A column, and a count across steps with gaps in the copy.
    check(&[A, E, B, B, B, B], "Vjyjj2gm", &[A, E, A, B, A, B]);
    check(&[E, A], "yykgm", &[E, A]);
    check(&[E, A], "yjjgm", &[E, A, "C-4 -- ---00 ---00 ---00"]);
}

#[test]
fn gm_refuses_a_column_type_mismatch() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "y1kjlgm");
    assert_eq!(steps(&app), [A, B]);
    assert!(app.status.contains("don't match"), "{}", app.status);
}

#[test]
fn register_zero_keeps_the_last_copy_through_cuts() {
    // The cut takes over plain p, as in Vim, and "0 still has the copy.
    check(&[A, B, E], "yyjddjp", &[A, E, B]);
    check(&[A, B, E], "yyjddj\"0p", &[A, E, A]);
    // A cut naming it doesn't get in either.
    check(&[A, B, E], "yyj\"0ddj\"0p", &[A, E, A]);
    // A newer copy replaces it.
    check(&[A, B, E], "yyjyyj\"0p", &[A, B, B]);
}

#[test]
fn a_cut_alone_leaves_register_zero_empty() {
    let (mut app, _rt) = app_with(&[A, E]);
    keys(&mut app, "ddj\"0p");
    assert!(
        app.status.contains("Nothing in register 0"),
        "{}",
        app.status
    );
}

#[test]
fn other_digits_arent_registers() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "\"1");
    assert!(app.status.contains("a to z"), "{}", app.status);
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
        &["C#4 40 CUT20 ---00 ---00", "D#4 50 VOL30 ---00 ---00"],
    );
    check(
        &[A, B],
        "2=j.",
        &["D-4 40 CUT20 ---00 ---00", "E-4 50 VOL30 ---00 ---00"],
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
        &["C#4 40 CUT20 ---00 ---00", "D#4 50 VOL30 ---00 ---00"],
    );
}

#[test]
fn field_visual_selects_within_columns() {
    check(&[A, B], "vjd", &[NOTE_GONE_A, NOTE_GONE_B]);
    check(
        &[A, B],
        "vjlx",
        &["--- -- CUT20 ---00 ---00", "--- -- VOL30 ---00 ---00"],
    );
    check(
        &[A, B],
        "lvj3=",
        &["C-4 43 CUT20 ---00 ---00", "D-4 53 VOL30 ---00 ---00"],
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
    let start = vec!["C-4 -- ---00 ---00 ---00"; 4]
        .into_iter()
        .chain(vec!["D-4 -- ---00 ---00 ---00"; 4])
        .collect::<Vec<_>>();
    let up_c = "C#4 -- ---00 ---00 ---00";
    let up_d = "D#4 -- ---00 ---00 ---00";
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
    assert_eq!(app.cursor.field, Field::Velocity);
    keys(&mut app, "w");
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
    check(&[E, E], "li7f", &["--- 7F ---00 ---00 ---00", E]);
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "li2B<Esc>");
    assert_eq!(steps(&app)[0], "--- 2B ---00 ---00 ---00");
    assert_eq!(app.cursor.step, 1);
}

#[test]
fn a_velocity_above_the_top_writes_the_top() {
    check(&[E, E], "liff", &["--- 7F ---00 ---00 ---00", E]);
    check(&[FULL], "lrff", &["C-4 7F VOL10 PAN20 CUT30"]);
    // Other FX values have their own tops: the level scale's, or their whole range.
    check(&[FULL], "lllrff", &["C-4 70 VOLF8 PAN20 CUT30"]);
    check(&[FULL], "lllllrff", &["C-4 70 VOL10 PANFF CUT30"]);
}

#[test]
fn partly_typed_values_are_dropped_on_esc() {
    check(&[E], "li7<Esc>", &[E]);
}

#[test]
fn a_typed_command_takes_effect_only_with_its_value() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lliCUT");
    assert_eq!(
        steps(&app)[0],
        E,
        "CUT must not be written before its value"
    );
    assert_eq!(app.cursor.field, Field::FxValue(0));
    keys(&mut app, "40<Esc>");
    assert_eq!(steps(&app)[0], "--- -- CUT40 ---00 ---00");
}

#[test]
fn lowercase_commands_are_uppercased() {
    check(&[E], "lliret14<Esc>", &["--- -- RET14 ---00 ---00"]);
}

#[test]
fn an_invalid_command_is_refused() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "lliXYZ");
    assert_eq!(steps(&app), [A]);
    assert!(app.status.contains("isn't an FX command"));
    assert_eq!(app.cursor.field, Field::FxCommand(0));
}

#[test]
fn esc_after_a_command_writes_nothing() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "lliVOL<Esc>");
    assert_eq!(steps(&app), [A]);
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert!(app.status.contains("needs a value"));
}

#[test]
fn a_value_needs_a_command() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "llli40");
    assert_eq!(steps(&app), [E]);
    assert!(app.status.contains("FX command first"));
}

#[test]
fn a_value_on_an_existing_command_keeps_the_command() {
    check(&[A], "llli7F<Esc>", &["C-4 40 CUT7F ---00 ---00"]);
}

#[test]
fn the_step_setting_changes_how_far_entry_moves() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":set step=2<Enter>ia");
    assert_eq!(app.cursor.step, 2);
}

#[test]
fn slides_keep_the_row_and_scroll_the_notes_under_it() {
    let (mut app, _rt) = app_with(&[A, B, C, E, E, E, E, E]);
    keys(&mut app, ":len 4<Enter>j");
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "]l");
    assert_eq!(app.song().phrases[&0].start(), 1);
    assert_eq!(app.cursor.step, 2, "still row 01");
    assert_eq!(app.top, 1, "and the screen moved with it");
    keys(&mut app, "[l");
    assert_eq!(app.song().phrases[&0].start(), 0);
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "]L");
    assert_eq!(app.song().phrases[&0].start(), 4);
    keys(&mut app, "[L");
    assert_eq!(app.song().phrases[&0].start(), 0);
    keys(&mut app, "2]l");
    assert_eq!(app.song().phrases[&0].start(), 2);
    assert_eq!(app.song().phrases[&0].len(), 4);
}

#[test]
fn a_slide_keeps_row_zero_still_when_the_phrase_fits_on_screen() {
    let (mut app, _rt) = app_with(&[E; 8]);
    keys(&mut app, ":len 4<Enter>");
    app.follow_cursor(24);
    assert_eq!(app.top, 0);
    keys(&mut app, "2]l");
    app.follow_cursor(24);
    assert_eq!(app.top, 2, "row 00 is still the top row");
    assert_eq!(app.cursor.step, 2);
    // Scrolling up still shows what is above the loop, and it stays there.
    keys(&mut app, "^y");
    app.follow_cursor(24);
    assert_eq!(app.top, 0);
    keys(&mut app, "[l");
    app.follow_cursor(24);
    assert_eq!(app.top, 0, "nothing is above the top to keep row 00 still");
}

#[test]
fn undoing_a_slide_keeps_the_row() {
    let (mut app, _rt) = app_with(&[A, B, C, E, E, E, E, E]);
    keys(&mut app, ":len 4<Enter>j2]l");
    assert_eq!(app.cursor.step, 3);
    keys(&mut app, "u");
    assert_eq!(app.song().phrases[&0].start(), 0);
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "^r");
    assert_eq!(app.song().phrases[&0].start(), 2);
    assert_eq!(app.cursor.step, 3);
}

#[test]
fn sliding_above_the_first_step_adds_room_and_keeps_the_notes() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "2[l");
    let phrase = &app.song().phrases[&0];
    assert_eq!((phrase.start(), phrase.len()), (0, 2));
    assert_eq!(steps(&app), [E, E, A, B]);
    assert_eq!(app.cursor.step, 0, "still row 00");
    keys(&mut app, "u");
    assert_eq!(steps(&app), [A, B]);
}

#[test]
fn gs_starts_the_loop_on_the_cursor_and_leaves_the_notes_on_screen() {
    let (mut app, _rt) = app_with(&[A, B, C, E, E, E, E, E]);
    keys(&mut app, ":len 4<Enter>jjgs");
    assert_eq!(app.song().phrases[&0].start(), 2);
    assert_eq!(app.cursor.step, 2, "the cursor stays on its note");
    assert_eq!(app.song().phrases[&0].len(), 4);
}

#[test]
fn motions_go_by_the_loop_and_can_leave_it() {
    let (mut app, _rt) = app_with(&[E; 32]);
    keys(&mut app, ":len 8<Enter>6]l");
    keys(&mut app, "gg");
    assert_eq!(app.cursor.step, 6, "row 00");
    keys(&mut app, "G");
    assert_eq!(app.cursor.step, 13, "the loop's last row");
    keys(&mut app, "j");
    assert_eq!(app.cursor.step, 14, "out of the loop");
    keys(&mut app, "ggk");
    assert_eq!(app.cursor.step, 5, "and above it");
    keys(&mut app, "gg}");
    assert_eq!(app.cursor.step, 10, "a beat from the loop start");
    keys(&mut app, "{{");
    assert_eq!(app.cursor.step, 2, "a beat above it");
    keys(&mut app, "3G");
    assert_eq!(app.cursor.step, 8, "a count counts from the loop start");
}

#[test]
fn whole_phrase_targets_cover_only_what_plays() {
    let (mut app, _rt) = app_with(&[A, A, A, A, A, A]);
    keys(&mut app, ":len 2<Enter>2]ldip");
    assert_eq!(steps(&app), [A, A, E, E, A, A]);
}

#[test]
fn notes_can_be_written_outside_the_loop() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":len 2<Enter>Gjj");
    assert_eq!(app.cursor.step, 3);
    keys(&mut app, "ia<Esc>");
    assert_ne!(steps(&app)[3], E);
    assert_eq!(app.song().phrases[&0].len(), 2);
}

#[test]
fn going_to_a_phrase_lands_on_its_loop_start() {
    let (mut app, _rt) = app_with(&[E; 8]);
    keys(&mut app, ":len 4<Enter>]Lj");
    assert_eq!(app.cursor.step, 5);
    keys(&mut app, "]P[P");
    assert_eq!(app.cursor.step, 4);
    assert_eq!(app.top, 4);
}

#[test]
fn playing_from_outside_the_loop_starts_at_its_first_step() {
    let (mut app, _rt) = app_with(&[E; 8]);
    keys(&mut app, ":len 4<Enter>2]l");
    keys(&mut app, "j");
    assert_eq!(app.section_step(), 1);
    keys(&mut app, "gg");
    keys(&mut app, "k");
    assert_eq!(app.section_step(), 0);
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
    assert!(rows.iter().any(|(k, _)| k == "c"));
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
            .any(|r| r.contains("S-Tab") && r.contains("from the start"))
    );
    assert!(rows.iter().any(|r| r.contains("i b")));
    keys(&mut app, "j");
    assert_eq!(app.help_scroll, 1);
    keys(&mut app, "q");
    assert_eq!(app.mode, Mode::Normal);
}

const FULL: &str = "C-4 70 VOL10 PAN20 CUT30";

#[test]
fn zero_and_dollar_go_to_the_ends_of_the_row() {
    let (mut app, _rt) = app_with(&[FULL, FULL]);
    keys(&mut app, "ll0");
    assert_eq!(app.cursor.field, Field::Note);
    keys(&mut app, "$");
    assert_eq!(app.cursor.field, Field::FxValue(2));
    keys(&mut app, "$");
    assert_eq!(app.cursor.field, Field::FxValue(2));
    keys(&mut app, "0");
    assert_eq!(app.cursor.field, Field::Note);
    assert_eq!(app.cursor.step, 0);
}

#[test]
fn zero_is_still_part_of_a_count() {
    let (mut app, _rt) = app_with(&[E; 16]);
    keys(&mut app, "10j");
    assert_eq!(app.cursor.step, 10);
    assert_eq!(app.cursor.field, Field::Note);
}

#[test]
fn operators_cover_from_the_cursor_to_the_end_of_the_row() {
    check(&[FULL], "lld$", &["C-4 70 ---00 ---00 ---00"]);
    check(&[FULL], "ld0", &["--- -- VOL10 PAN20 CUT30"]);
    check(&[FULL], "d0", &["--- 70 VOL10 PAN20 CUT30"]);
    check(
        &[FULL, FULL],
        "lld$j.",
        &["C-4 70 ---00 ---00 ---00", "C-4 70 ---00 ---00 ---00"],
    );
}

#[test]
fn a_selection_can_grow_to_the_ends_of_the_row() {
    check(&[FULL], "lv$d", &["C-4 -- ---00 ---00 ---00"]);
    check(&[FULL], "lv0d", &["--- -- VOL10 PAN20 CUT30"]);
}

#[test]
fn r_replaces_a_note_and_keeps_everything_else() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "rk");
    assert_eq!(steps(&app)[0], "C-5 70 VOL10 PAN20 CUT30");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0);
    assert_eq!(app.cursor.field, Field::Note);
    keys(&mut app, "r1");
    assert_eq!(steps(&app)[0], "OFF 70 VOL10 PAN20 CUT30");
    keys(&mut app, "uu");
    assert_eq!(steps(&app)[0], FULL);
}

#[test]
fn r_ignores_the_note_velocity_setting() {
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "ic<Esc>");
    assert_eq!(app.note_velocity, 0x54);
    keys(&mut app, "rk");
    assert_eq!(steps(&app)[0], "C-5 70 VOL10 PAN20 CUT30");
    assert_eq!(app.note_velocity, 0x54);
}

#[test]
fn r_takes_a_whole_value_in_the_other_columns() {
    check(&[FULL], "lr40", &["C-4 40 VOL10 PAN20 CUT30"]);
    check(&[FULL], "llr04", &["C-4 70 VOL10 PAN20 CUT30"]);
    check(&[FULL], "lllr7f", &["C-4 70 VOL7F PAN20 CUT30"]);
    check(&[FULL], "llrres7f", &["C-4 70 RES7F PAN20 CUT30"]);
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "llrres7f");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert_eq!(app.cursor.step, 0);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn r_needs_a_command_before_an_fx_value() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "lllr");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("FX command"), "{}", app.status);
}

#[test]
fn escape_or_a_bad_value_leaves_r_unchanged() {
    check(&[FULL], "r<Esc>", &[FULL]);
    check(&[FULL], "lr4<Esc>", &[FULL]);
    check(&[FULL], "llrre<Esc>", &[FULL]);
    check(&[FULL], "llrres<Esc>", &[FULL]);
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "llrres<Esc>");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.entry.text.is_empty());

    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "rb");
    assert_eq!(
        app.mode,
        Mode::Replace,
        "b is not a note, so it keeps waiting"
    );
    keys(&mut app, "a");
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn backspace_edits_a_replace_and_never_moves() {
    check(&[FULL], "lr4<BS>5<BS><BS>70", &["C-4 70 VOL10 PAN20 CUT30"]);
    let (mut app, _rt) = app_with(&[FULL, FULL]);
    keys(&mut app, "jlr<BS>");
    assert_eq!(app.cursor.step, 1);
    assert_eq!(app.mode, Mode::Replace);
}

#[test]
fn dot_repeats_a_replace_at_the_cursor() {
    check(
        &[FULL, FULL, "--- -- ---00 ---00 ---00"],
        "rkj.",
        &[
            "C-5 70 VOL10 PAN20 CUT30",
            "C-5 70 VOL10 PAN20 CUT30",
            "--- -- ---00 ---00 ---00",
        ],
    );
    check(
        &[FULL, FULL],
        "lr40j.",
        &["C-4 40 VOL10 PAN20 CUT30", "C-4 40 VOL10 PAN20 CUT30"],
    );
    check(
        &[FULL, FULL],
        "llrres7fj.",
        &["C-4 70 RES7F PAN20 CUT30", "C-4 70 RES7F PAN20 CUT30"],
    );
}

#[test]
fn a_count_on_r_does_not_change_more_than_this_field() {
    check(&[FULL, FULL], "3rk", &["C-5 70 VOL10 PAN20 CUT30", FULL]);
}

#[test]
fn s_clears_the_field_and_starts_insert_there() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "s");
    assert_eq!(steps(&app)[0], "--- 70 VOL10 PAN20 CUT30");
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.cursor.step, 0);
    // A note typed in Insert takes the current velocity, as it always does.
    keys(&mut app, "k");
    assert_eq!(steps(&app)[0], "C-5 64 VOL10 PAN20 CUT30");
    assert_eq!(app.cursor.step, 1, "Insert moves down as usual");
    keys(&mut app, "<Esc>uu");
    assert_eq!(steps(&app)[0], FULL);
}

#[test]
fn s_works_on_the_other_columns() {
    check(&[FULL, E], "ls40<Esc>", &["C-4 40 VOL10 PAN20 CUT30", E]);
    check(&[FULL, E], "ls<Esc>", &["C-4 -- VOL10 PAN20 CUT30", E]);
    check(
        &[FULL, E],
        "llsres7f<Esc>",
        &["C-4 70 RES7F PAN20 CUT30", E],
    );
}

#[test]
fn s_on_an_fx_value_keeps_its_command_for_the_new_value() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "llls");
    assert_eq!(steps(&app)[0], "C-4 70 ---00 PAN20 CUT30");
    keys(&mut app, "60<Esc>");
    assert_eq!(steps(&app)[0], "C-4 70 VOL60 PAN20 CUT30");

    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "llls<Esc>");
    assert_eq!(steps(&app)[0], "C-4 70 ---00 PAN20 CUT30");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert!(app.status.contains("VOL needs a value"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(steps(&app)[0], FULL);
}

#[test]
fn a_count_on_s_clears_only_this_field() {
    check(
        &[FULL, FULL],
        "3s<Esc>",
        &["--- 70 VOL10 PAN20 CUT30", FULL],
    );
}

#[test]
fn dot_does_not_repeat_s() {
    check(
        &[FULL, FULL],
        "rks<Esc>j.",
        &["--- 70 VOL10 PAN20 CUT30", "C-5 70 VOL10 PAN20 CUT30"],
    );
}

#[test]
fn r_on_a_note_with_no_velocity_fills_it_in() {
    check(&[E], "ra", &["C-4 64 ---00 ---00 ---00"]);
}

const C4: &str = "C-4 64 ---00 ---00 ---00";
const LOUD: &str = "E-4 7F ---00 ---00 ---00";

#[test]
fn visual_r_fills_notes_on_every_selected_step() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, "vGra");
    assert_eq!(steps(&app), [C4, C4, C4, C4]);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0, "back at the top of the selection");
}

#[test]
fn visual_r_is_one_undo_however_many_steps() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, "vGra");
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, E, E, E]);
    keys(&mut app, "^r");
    assert_eq!(steps(&app), [C4, C4, C4, C4]);
}

#[test]
fn visual_r_only_covers_the_selection_and_keeps_what_notes_have() {
    let (mut app, _rt) = app_with(&[E, LOUD, E, E]);
    keys(&mut app, "vjra");
    assert_eq!(steps(&app), [C4, "C-4 7F ---00 ---00 ---00", E, E]);
}

#[test]
fn visual_r_works_from_the_bottom_up_and_with_whole_steps() {
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "Gvkkra");
    assert_eq!(steps(&app), [C4, C4, C4]);
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "jVjra");
    assert_eq!(steps(&app), [E, C4, C4]);
}

#[test]
fn visual_r_writes_note_offs() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "vjr1");
    assert_eq!(
        steps(&app),
        ["OFF -- ---00 ---00 ---00", "OFF -- ---00 ---00 ---00",]
    );
}

#[test]
fn escape_leaves_a_selection_replace_with_nothing_written() {
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "vG");
    assert_eq!(app.cursor.step, 2);
    keys(&mut app, "r<Esc>");
    assert_eq!(steps(&app), [E, E, E]);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0);
    // A later single replace isn't mistaken for the selection's.
    keys(&mut app, "ra");
    assert_eq!(steps(&app), [C4, E, E]);
}

#[test]
fn visual_r_puts_a_velocity_only_on_notes() {
    let (mut app, _rt) = app_with(&[E, LOUD, E, C4]);
    keys(&mut app, "lvGr40");
    assert_eq!(
        steps(&app),
        [E, "E-4 40 ---00 ---00 ---00", E, "C-4 40 ---00 ---00 ---00"]
    );
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, LOUD, E, C4]);

    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lvjr");
    assert_eq!(app.mode, Mode::VisualField, "nothing to replace");
    assert!(app.status.contains("no notes"), "{}", app.status);
}

#[test]
fn visual_r_fills_fx_commands_and_keeps_commands_for_values() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llvjrCUT40");
    assert_eq!(
        steps(&app),
        ["--- -- CUT40 ---00 ---00", "--- -- CUT40 ---00 ---00",]
    );
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, E]);

    let (mut app, _rt) = app_with(&[E, "--- -- RES10 ---00 ---00", E, "--- -- CUT20 ---00 ---00"]);
    keys(&mut app, "lllvGr7F");
    assert_eq!(
        steps(&app),
        [E, "--- -- RES7F ---00 ---00", E, "--- -- CUT7F ---00 ---00",]
    );
    assert_eq!(app.cursor.step, 0);
}

#[test]
fn dot_repeats_a_selection_replace() {
    let (mut app, _rt) = app_with(&[E, E, E, E, E, E]);
    keys(&mut app, "vjra");
    // In Normal it covers as many steps again, from the cursor.
    keys(&mut app, "jj.");
    assert_eq!(steps(&app), [C4, C4, C4, C4, E, E]);
    keys(&mut app, "u");
    assert_eq!(steps(&app), [C4, C4, E, E, E, E]);
    // On a selection it covers that selection, and it doesn't run off the end.
    keys(&mut app, "vjjjj.");
    assert_eq!(steps(&app), [C4, C4, C4, C4, C4, C4]);
    keys(&mut app, "u");
    keys(&mut app, "G.");
    assert_eq!(steps(&app), [C4, C4, E, E, E, C4]);
    assert_eq!(steps(&app).len(), 6);
}

#[test]
fn rotating_wraps_within_the_loop_and_counts() {
    let (mut app, _rt) = app_with(&[A, B, E, E]);
    keys(&mut app, "]r");
    assert_eq!(steps(&app), [E, A, B, E]);
    assert_eq!(app.cursor.step, 0, "the cursor stays on its row");
    keys(&mut app, "2]r");
    assert_eq!(steps(&app), [B, E, E, A]);
    keys(&mut app, "[r");
    assert_eq!(steps(&app), [E, E, A, B]);
    keys(&mut app, "u");
    assert_eq!(steps(&app), [B, E, E, A]);
}

#[test]
fn rotating_leaves_steps_outside_the_loop_alone() {
    let (mut app, _rt) = app_with(&[A, B, E, E, A, B]);
    keys(&mut app, ":len 3<Enter>]r");
    assert_eq!(steps(&app), [E, A, B, E, A, B]);
    keys(&mut app, "]lj]r");
    assert_eq!(steps(&app), [E, E, A, B, A, B]);
    assert_eq!(app.song().phrases[&0].start(), 1);
}

#[test]
fn rotating_all_the_way_round_leaves_no_undo_step() {
    let (mut app, _rt) = app_with(&[A, E]);
    let depth = app.history.undo_depth();
    keys(&mut app, "2]r");
    assert_eq!(steps(&app), [A, E]);
    assert_eq!(app.history.undo_depth(), depth);
    keys(&mut app, "3[r");
    assert_eq!(steps(&app), [E, A]);
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn level_fx_values_snap_to_the_top_of_the_scale() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llrVOLFF");
    assert_eq!(steps(&app)[0], "--- -- VOLF8 ---00 ---00");
    keys(&mut app, "lrFE");
    assert_eq!(steps(&app)[0], "--- -- VOLF8 ---00 ---00");
    keys(&mut app, "20=");
    assert_eq!(steps(&app)[0], "--- -- VOLF8 ---00 ---00");

    // Filling a selection snaps too, and other commands keep their whole range.
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llvjrDLYFF");
    assert_eq!(steps(&app)[1], "--- -- DLYF8 ---00 ---00");
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "llrCUTFF");
    assert_eq!(steps(&app)[0], "--- -- CUTFF ---00 ---00");
}

#[test]
fn ge_ends_the_loop_on_the_cursor_and_keeps_what_is_stored() {
    let (mut app, _rt) = app_with(&[A, B, E, E, E, E]);
    let length = |app: &App| app.song().phrases[&0].len();
    keys(&mut app, "jjge");
    assert_eq!(length(&app), 3);
    assert_eq!(steps(&app).len(), 6, "nothing is lost");
    assert_eq!(app.cursor.step, 2);

    // Below the loop, it brings the steps in.
    keys(&mut app, "jjge");
    assert_eq!(length(&app), 5);
    keys(&mut app, "u");
    assert_eq!(length(&app), 3);
}

#[test]
fn ge_counts_from_the_loop_start() {
    let (mut app, _rt) = app_with(&[A, B, E, E, E, E]);
    keys(&mut app, ":len 2<Enter>]ljjge");
    let phrase = &app.song().phrases[&0];
    assert_eq!((phrase.start(), phrase.len()), (1, 3));
}

#[test]
fn ge_refuses_above_the_start_and_leaves_no_undo_when_nothing_changes() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":len 2<Enter>]l");
    let depth = app.history.undo_depth();
    app.cursor.step = 0;
    keys(&mut app, "ge");
    assert_eq!(app.song().phrases[&0].len(), 2);
    assert!(app.status.contains("above its start"), "{}", app.status);
    app.cursor.step = 2;
    keys(&mut app, "ge");
    assert_eq!(app.song().phrases[&0].len(), 2);
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn len_takes_hex_after_0x() {
    let (mut app, _rt) = app_with(&[E; 32]);
    keys(&mut app, ":len 0x10<Enter>");
    assert_eq!(app.song().phrases[&0].len(), 16);
    keys(&mut app, ":len 10<Enter>");
    assert_eq!(app.song().phrases[&0].len(), 10);
}

#[test]
fn plus_and_underscore_are_large_steps() {
    // A note moves by octaves, and hex values by their top digit.
    check(&[A], "+", &["C-5 40 CUT20 ---00 ---00"]);
    check(&[A], "_", &["C-3 40 CUT20 ---00 ---00"]);
    check(&[A], "l+", &["C-4 50 CUT20 ---00 ---00"]);
    check(&[A], "l_", &["C-4 30 CUT20 ---00 ---00"]);
    check(&[A], "ll+", &["C-4 40 CUT30 ---00 ---00"]);
    check(&[A], "ll2_", &["C-4 40 CUT00 ---00 ---00"]);
    // They stop at each field's top.
    check(&[A], "l9+", &["C-4 7F CUT20 ---00 ---00"]);
    check(&[A], "ll99+", &["C-4 40 CUTFF ---00 ---00"]);
    check(&[B], "ll99+", &["D-4 50 VOLF8 ---00 ---00"]);
}

#[test]
fn equals_and_minus_are_small_steps_and_carry() {
    check(&[A], "=", &["C#4 40 CUT20 ---00 ---00"]);
    check(&[A], "-", &["B-3 40 CUT20 ---00 ---00"]);
    check(
        &["--- -- CUT7F ---00 ---00"],
        "ll=",
        &["--- -- CUT80 ---00 ---00"],
    );
}

#[test]
fn ret_moves_each_digit_alone_and_never_carries() {
    let ret = "--- -- RET1F ---00 ---00";
    check(&[ret], "ll=", &[ret]);
    check(&[ret], "ll-", &["--- -- RET1E ---00 ---00"]);
    check(&[ret], "ll+", &["--- -- RET2F ---00 ---00"]);
    check(&[ret], "ll_", &["--- -- RET0F ---00 ---00"]);
    check(&[ret], "ll_ll_", &["--- -- RET0F ---00 ---00"]);
    let top = "--- -- RETF4 ---00 ---00";
    check(&[top], "ll+", &[top]);
}

#[test]
fn large_steps_work_on_a_selection_and_repeat() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "vj+");
    assert_eq!(
        steps(&app),
        ["C-5 40 CUT20 ---00 ---00", "D-5 50 VOL30 ---00 ---00"]
    );
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "ll+.");
    assert_eq!(steps(&app), ["C-4 40 CUT40 ---00 ---00"]);
}

#[test]
fn k_chooses_an_fx_command_and_an_empty_slot_waits_for_its_value() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llK");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 12);
    keys(&mut app, "cut<Enter>");
    assert_eq!(app.mode, Mode::Replace);
    assert_eq!(steps(&app)[0], E, "nothing until the value is typed");
    keys(&mut app, "40");
    assert_eq!(steps(&app)[0], "--- -- CUT40 ---00 ---00");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0, "it stays on this step");
    assert_eq!(app.cursor.field, Field::FxValue(0));
}

#[test]
fn k_keeps_a_value_when_changing_the_command() {
    let (mut app, _rt) = app_with(&[A]);
    // On the command or the value, and it opens on what is there.
    keys(&mut app, "lllK");
    assert_eq!(app.picker.as_ref().unwrap().selected, 3);
    keys(&mut app, "res<Enter>");
    assert_eq!(steps(&app)[0], "C-4 40 RES20 ---00 ---00");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "u");
    assert_eq!(steps(&app)[0], A);

    // A level command snaps what it keeps.
    let (mut app, _rt) = app_with(&["--- -- CUTFF ---00 ---00"]);
    keys(&mut app, "llKvol<Enter>");
    assert_eq!(steps(&app)[0], "--- -- VOLF8 ---00 ---00");
}

#[test]
fn k_can_empty_a_slot_and_esc_leaves_it() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "llK<Esc>");
    assert_eq!(steps(&app)[0], A);
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "K---<Enter>");
    assert_eq!(steps(&app)[0], "C-4 40 ---00 ---00 ---00");

    // Leaving before the value writes nothing.
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "llKcut<Enter><Esc>");
    assert_eq!(steps(&app)[0], E);
    assert_eq!(app.mode, Mode::Normal);
}

/// Notes on rows 0, 1 and 3 of 4, at the velocities given, with the second row's slot holding `CUT`.
fn velocities(first: &str, second: &str, last: &str) -> Vec<String> {
    vec![
        format!("C-4 {first} ---00 ---00 ---00"),
        format!("C-4 {second} ---00 ---00 ---00"),
        E.to_owned(),
        format!("C-4 {last} ---00 ---00 ---00"),
    ]
}

fn app_of(rows: &[String]) -> (App, Realtime) {
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    app_with(&rows)
}

/// The velocity on each row.
fn vel(app: &App) -> Vec<String> {
    steps(app).iter().map(|s| s[4..6].to_owned()).collect()
}

#[test]
fn s_opens_on_the_first_and_last_values_and_previews_a_ramp() {
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    let depth = app.history.undo_depth();
    keys(&mut app, "lvGS");
    assert_eq!(app.mode, Mode::Shape);
    let shape = app.shape.as_ref().unwrap().shape;
    assert_eq!((shape.from, shape.to), (0x10, 0x40));
    // Heard straight away, only on notes, and spaced by row. Nothing is in the undo list yet.
    assert_eq!(vel(&app), ["10", "20", "--", "40"]);
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn enter_keeps_a_shape_as_one_undo_and_esc_puts_it_back() {
    let start = velocities("10", "7F", "40");
    let (mut app, _rt) = app_of(&start);
    keys(&mut app, "lvGS+=<Enter>");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.shape.is_none());
    assert_eq!(vel(&app), ["21", "2B", "--", "40"]);
    assert!(app.dirty);
    assert_eq!(app.cursor.step, 0);
    keys(&mut app, "u");
    assert_eq!(steps(&app), start);
    keys(&mut app, "^r");
    assert_eq!(vel(&app), ["21", "2B", "--", "40"]);

    let (mut app, _rt) = app_of(&start);
    let depth = app.history.undo_depth();
    keys(&mut app, "lvGS+++<Esc>");
    assert_eq!(steps(&app), start);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.history.undo_depth(), depth);
    assert!(!app.dirty);
}

#[test]
fn panel_keys_move_rows_change_values_and_swap_the_ends() {
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    keys(&mut app, "lvGS");
    let shape = |app: &App| app.shape.as_ref().unwrap().shape;
    keys(&mut app, "~");
    assert_eq!((shape(&app).from, shape(&app).to), (0x40, 0x10));
    assert_eq!(vel(&app), ["40", "30", "--", "10"]);
    keys(&mut app, "~j");
    keys(&mut app, "i7F");
    assert_eq!(shape(&app).to, 0x7F);
    keys(&mut app, "=");
    assert_eq!(shape(&app).to, 0x7F, "stops at the column's top");
    keys(&mut app, "x");
    assert_eq!(shape(&app).to, 0x40);
    keys(&mut app, "j=_");
    assert_eq!(shape(&app).curve, -20);
    keys(&mut app, "i60<Enter>");
    assert_eq!(shape(&app).curve, 60);
    keys(&mut app, "i-500<Enter>");
    assert_eq!(shape(&app).curve, -100, "it stops at the limit");
    keys(&mut app, "i<Esc>");
    assert_eq!(shape(&app).curve, -100);
    keys(&mut app, "x");
    assert_eq!(shape(&app).curve, 0);
    keys(&mut app, "j+");
    assert_eq!(shape(&app).s, 25);
    // Past the last row it stays.
    keys(&mut app, "jjjjj");
    assert_eq!(
        app.shape.as_ref().unwrap().row,
        vt_tui::shaping::ShapeRow::Where
    );
}

#[test]
fn k_chooses_fill_and_where_and_returns_to_the_panel() {
    let (mut app, _rt) = app_with(&[
        "C-4 -- CUT10 ---00 ---00",
        E,
        "C-4 -- ---00 ---00 ---00",
        "--- -- CUT30 ---00 ---00",
    ]);
    keys(&mut app, "llvGS");
    let shape = |app: &App| app.shape.as_ref().unwrap().shape;
    let fx =
        |app: &App| -> Vec<String> { steps(app).iter().map(|s| s[7..12].to_owned()).collect() };
    assert_eq!(fx(&app), ["CUT10", "---00", "---00", "CUT30"]);

    keys(&mut app, "K");
    assert_eq!(app.mode, Mode::Shape, "K isn't a list on From");
    assert!(app.status.contains("Fill"), "{}", app.status);

    // Where, from Commands to All rows, writes the command on the empty rows.
    keys(&mut app, "jjjjjKall<Enter>");
    assert_eq!(app.mode, Mode::Shape);
    assert_eq!(fx(&app), ["CUT10", "CUT1B", "CUT25", "CUT30"]);

    keys(&mut app, "Knotes<Enter>");
    assert_eq!(fx(&app)[0], "CUT10");
    assert_eq!(fx(&app)[1], "---00");
    assert_eq!(
        fx(&app)[3],
        "CUT30",
        "no note on this row, so it's left as it was"
    );
    keys(&mut app, "Kevery<Enter>");
    assert!(matches!(
        shape(&app).place,
        vt_model::shape::Where::Every(4)
    ));
    keys(&mut app, "j");
    assert_eq!(
        app.shape.as_ref().unwrap().row,
        vt_tui::shaping::ShapeRow::Amount
    );
    keys(&mut app, "-");
    assert!(matches!(
        shape(&app).place,
        vt_model::shape::Where::Every(3)
    ));
    keys(&mut app, "<Esc>");
    assert_eq!(steps(&app)[1], E);

    // An Esc from a picker goes back to the panel.
    keys(&mut app, "gvS");
    assert_eq!(app.mode, Mode::Shape);
    keys(&mut app, "jjjjjK<Esc>");
    assert_eq!(app.mode, Mode::Shape);
    keys(&mut app, "<Esc>");
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn random_fill_stays_in_range_and_rerolls() {
    let rows = vec!["C-4 40 ---00 ---00 ---00".to_owned(); 64];
    let (mut app, _rt) = app_of(&rows);
    keys(&mut app, "lvGS");
    keys(&mut app, "ji20jjjKrandom<Enter>");
    assert_eq!(app.mode, Mode::Shape);
    let before = vel(&app);
    assert!(
        before
            .iter()
            .all(|v| (0..=0x20).contains(&u8::from_str_radix(v, 16).unwrap()))
    );
    keys(&mut app, "r");
    assert_ne!(vel(&app), before);
    keys(&mut app, "<Enter>");
    keys(&mut app, "u");
    assert_eq!(steps(&app).len(), 64);
    assert!(vel(&app).iter().all(|v| v == "40"));
}

#[test]
fn shape_refuses_what_it_cannot_fill() {
    let reason = |rows: &[&str], typed: &str| {
        let (mut app, _rt) = app_with(rows);
        keys(&mut app, typed);
        assert_ne!(app.mode, Mode::Shape, "{typed}");
        app.status.clone()
    };
    let notes = ["C-4 -- ---00 ---00 ---00"; 3];
    assert!(reason(&notes, "vGS").contains("Velocity and FX"));
    assert!(reason(&notes, "VGS").contains("one column"));
    assert!(reason(&notes, "lvlGS").contains("one column"));
    assert!(reason(&notes, "lvS").contains("2 rows"));
    assert!(reason(&notes, "lllvGS").contains("FX command"));
    assert!(reason(&[E, E], "lvGS").contains("no notes"));
}

#[test]
fn dot_applies_the_same_shape_with_ends_from_what_is_there() {
    let (mut app, _rt) = app_with(&[
        "C-4 00 ---00 ---00 ---00",
        "C-4 00 ---00 ---00 ---00",
        "C-4 40 ---00 ---00 ---00",
        "C-4 10 ---00 ---00 ---00",
        "C-4 00 ---00 ---00 ---00",
        "C-4 30 ---00 ---00 ---00",
    ]);
    keys(&mut app, "lvjjS<Enter>");
    assert_eq!(vel(&app)[..3], ["00", "20", "40"]);
    // On a new selection, its own ends.
    keys(&mut app, "3jvjj.");
    assert_eq!(vel(&app)[3..], ["10", "20", "30"]);
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "u");
    assert_eq!(vel(&app)[3..], ["10", "00", "30"]);
    // In Normal, as many rows as before, from the cursor, stopping at the end.
    keys(&mut app, "G.");
    assert_eq!(steps(&app).len(), 6);
}

#[test]
fn the_transport_keys_work_with_the_panel_open() {
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    keys(&mut app, "lvGS");
    app.key(Key::new(KeyCode::Tab), Instant::now());
    assert_eq!(app.mode, Mode::Shape);
}

#[test]
fn a_shape_on_an_fx_column_snaps_to_its_commands_top() {
    let (mut app, _rt) = app_with(&["--- -- VOL10 ---00 ---00", "--- -- VOL20 ---00 ---00"]);
    keys(&mut app, "llvGS");
    keys(&mut app, "jiFF");
    assert_eq!(app.shape.as_ref().unwrap().shape.to, 0xF8);
    keys(&mut app, "<Enter>");
    assert_eq!(steps(&app)[1], "--- -- VOLF8 ---00 ---00");
}

const MARKED: &str = "C-4 40 CUT20h ---00 ---00";
const GLIDING: &str = "C-4 40 CUT20g ---00 ---00";

#[test]
fn tilde_cycles_a_values_mark_from_either_of_its_fields() {
    check(&[A], "ll~", &[MARKED]);
    check(&[A], "lll~", &[MARKED]);
    check(&[A], "ll~~", &[GLIDING]);
    check(&[A], "ll~~~", &[A]);
}

#[test]
fn tilde_is_one_undo_step_and_dot_repeats_it() {
    check(&[A], "ll~u", &[A]);
    check(&[A, A], "ll~j.", &[MARKED, MARKED]);
}

#[test]
fn tilde_refuses_anything_but_a_value_and_says_why() {
    for (start, typed) in [
        ("C-4 40 DEL10 ---00 ---00", "lll~"),
        ("C-4 40 ---00 ---00 ---00", "lll~"),
        ("C-4 40 CUT20 ---00 ---00", "~"),
    ] {
        let (mut app, _rt) = app_with(&[start]);
        keys(&mut app, typed);
        assert_eq!(steps(&app), [start], "after {typed:?}");
        assert!(app.status.contains("No value to mark"), "{}", app.status);
    }
}

#[test]
fn visual_tilde_makes_a_mixed_selection_the_same() {
    let start = [
        "C-4 40 CUT20 ---00 ---00",
        "C-4 40 RES30g ---00 ---00",
        "C-4 40 DEL10 ---00 ---00",
        "C-4 40 PAN40h ---00 ---00",
    ];
    check(
        &start,
        "llV3j~",
        &[
            "C-4 40 CUT20h ---00 ---00",
            "C-4 40 RES30h ---00 ---00",
            "C-4 40 DEL10 ---00 ---00",
            "C-4 40 PAN40h ---00 ---00",
        ],
    );
}

#[test]
fn visual_tilde_stays_in_the_selected_column_and_repeats() {
    let start = ["C-4 40 CUT20 RES30 ---00", "C-4 40 CUT20 RES30 ---00"];
    let one = "C-4 40 CUT20h RES30 ---00";
    check(&start, "llvj~", &[one, one]);
    let (mut app, _rt) = app_with(&[start[0], start[1], start[0]]);
    keys(&mut app, "llvj~jj.");
    assert_eq!(steps(&app), [one, one, "C-4 40 CUT20h RES30 ---00"]);
}

#[test]
fn a_typed_value_copies_the_mark_above_it_without_wrapping() {
    let empty = "--- -- ---00 ---00 ---00";
    let above = "C-4 40 CUT20h ---00 ---00";
    let typed = "--- -- CUT40h ---00 ---00";
    let plain = "--- -- CUT40 ---00 ---00";
    check(&[above, empty], "jlliCUT40<Esc>", &[above, typed]);
    check(&[empty, above], "lliCUT40<Esc>", &[plain, above]);
    let event_between = "C-4 40 DEL10 ---00 ---00";
    check(
        &[above, event_between, empty],
        "2jlliCUT40<Esc>",
        &[above, event_between, typed],
    );
}

#[test]
fn a_value_typed_over_another_keeps_its_mark() {
    let start = "--- -- CUT20 ---00 ---00";
    let above = "C-4 40 CUT20g ---00 ---00";
    check(
        &[above, "--- -- CUT20h ---00 ---00"],
        "jlllr55",
        &[above, "--- -- CUT55h ---00 ---00"],
    );
    check(&[start], "llrRES40", &["--- -- RES40 ---00 ---00"]);
}

#[test]
fn changing_a_value_to_an_event_drops_its_mark() {
    check(&[MARKED], "llrDEL10", &["C-4 40 DEL10 ---00 ---00"]);
}

#[test]
fn moving_a_slot_carries_its_mark_and_clearing_removes_it() {
    let empty = "--- -- ---00 ---00 ---00";
    check(&[MARKED, empty], "yyjp", &[MARKED, MARKED]);
    check(&[MARKED, empty], "]r", &[empty, MARKED]);
    let cleared = "C-4 40 ---00 ---00 ---00";
    check(&[MARKED], "llx", &[cleared]);
    check(&[MARKED], "ll=", &["C-4 40 CUT21h ---00 ---00"]);
}

#[test]
fn w_stops_at_the_last_slot_and_a_row_end_operator_covers_it() {
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "wwwwwwww");
    assert_eq!(app.cursor.field, Field::FxCommand(2));
    check(&[FULL], "d$", &["--- -- ---00 ---00 ---00"]);
}
