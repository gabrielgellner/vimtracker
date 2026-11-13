//! Key sequences, starting and ending as step text.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::constants::MAX_PHRASE_STEPS;
use vt_model::samples::SamplePool;
use vt_model::shape::FillKind;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::{App, Field};
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

const E: &str = "--- -- --- --- ---";

fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
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
        } else if let Some(tail) = rest.strip_prefix("<Down>") {
            app.key(Key::new(KeyCode::Down), now);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('^') {
            let c = tail.chars().next().unwrap();
            app.key(
                Key {
                    ctrl: true,
                    ..Key::char(c)
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
    app.song().tracks[0].phrases[&id]
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
    vec![format!("{note} 40 cut:20 --- ---"); count]
}

fn refs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

const A: &str = "C-3 40 cut:20 --- ---";
const B: &str = "D-3 50 vol:30 --- ---";
const C: &str = "E-3 60 --- pan:80 ---";
const NOTE_GONE_A: &str = "--- 40 cut:20 --- ---";
const NOTE_GONE_B: &str = "--- 50 vol:30 --- ---";
const NOTE_GONE_C: &str = "--- 60 --- pan:80 ---";

#[test]
fn x_clears_one_field() {
    check(&[A], "x", &[NOTE_GONE_A]);
    check(&[A], "lx", &["C-3 -- cut:20 --- ---"]);
}

#[test]
fn clearing_either_part_of_a_slot_clears_the_slot() {
    check(&[A], "lllllx", &["C-3 40 --- --- ---"]);
    check(&[A], "llllllx", &["C-3 40 --- --- ---"]);
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
    check(&[A, B], "j>>", &[A, "D#3 50 vol:30 --- ---"]);
}

#[test]
fn targets_cover_a_beat_a_column_or_the_phrase() {
    let start = notes("C-3", 8);
    let start = refs(&start);
    let mut beat = vec![E; 4];
    beat.extend(vec![A; 4]);
    check(&start, "dib", &beat);
    check(&start, "jjdib", &beat);
    check(&start, "2dib", &[E; 8]);
    check(&start, "d2ib", &[E; 8]);
    check(&start, "dip", &[E; 8]);
    check(&start, "dic", &[NOTE_GONE_A; 8]);
    check(&start, "llllldic", &["C-3 40 --- --- ---"; 8]);
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
    let off = "OFF -- --- --- ---";
    check(&[A, off, E], "2>j", &["C#3 40 cut:20 --- ---", off, E]);
    // Whole steps move notes only.
    check(&[A], ">>", &["C#3 40 cut:20 --- ---"]);
    // A volume column moves by 1, and an empty one is left alone.
    check(&[A, E], "l>j", &["C-3 41 cut:20 --- ---", E]);
    // An FX value moves, and an empty slot is left alone.
    check(&[A], "lllll>>", &["C#3 40 cut:20 --- ---"]);
    check(&[A], "lllll>j", &["C-3 40 cut:21 --- ---"]);
    // Shifts below the range are clamped.
    check(&["c_1 00 --- --- ---"], "<<", &["c_1 00 --- --- ---"]);
}

#[test]
fn increment_takes_a_count_as_its_amount() {
    check(&[A], "3=", &["D#3 40 cut:20 --- ---"]);
    check(&[A], "12-", &["C-2 40 cut:20 --- ---"]);
    check(&[A], "l3=", &["C-3 43 cut:20 --- ---"]);
    check(&[A], "lllll=", &["C-3 40 cut:21 --- ---"]);
    check(&[A], "^a", &["C#3 40 cut:20 --- ---"]);
    check(&[A], "^x", &["B-2 40 cut:20 --- ---"]);
    // Values clamp to their ranges.
    check(
        &["C-3 7F vol:F8 --- ---"],
        "l9+",
        &["C-3 7F vol:F8 --- ---"],
    );
    check(&["G-8 -- --- --- ---"], "+", &["G-8 -- --- --- ---"]);
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
    assert_eq!(
        app.song().tracks[0].phrases[&0].len(),
        3,
        "the loop hasn't changed"
    );
    keys(&mut app, "u");
    assert_eq!(steps(&app).len(), 3);
    // A huge count is no trouble.
    keys(&mut app, "99999999p");
    assert_eq!(steps(&app).len(), MAX_PHRASE_STEPS);
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 3);
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
    const KICK: &str = "C-3 40 --- --- ---";
    const HAT: &str = "--- -- --- pan:80 ---";
    const BOTH: &str = "C-3 40 --- pan:80 ---";
    // Whole steps.
    check(&[HAT, KICK], "yyjgm", &[HAT, BOTH]);
    // A plain paste of the same thing wipes the kick.
    check(&[HAT, KICK], "yyjp", &[HAT, HAT]);
    // A full field wins, and OFF counts as full.
    check(
        &["D-3 -- --- --- ---", KICK],
        "yyjgm",
        &["D-3 -- --- --- ---", "D-3 40 --- --- ---"],
    );
    check(
        &["OFF -- --- --- ---", KICK],
        "yyjgm",
        &["OFF -- --- --- ---", "OFF 40 --- --- ---"],
    );
    // A column, and a count across steps with gaps in the copy.
    check(&[A, E, B, B, B, B], "Vjyjj2gm", &[A, E, A, B, A, B]);
    check(&[E, A], "yykgm", &[E, A]);
    check(&[E, A], "yjjgm", &[E, A, "C-3 -- --- --- ---"]);
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
        &["C#3 40 cut:20 --- ---", "D#3 50 vol:30 --- ---"],
    );
    check(
        &[A, B],
        "2=j.",
        &["D-3 40 cut:20 --- ---", "E-3 50 vol:30 --- ---"],
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
        &["C#3 40 cut:20 --- ---", "D#3 50 vol:30 --- ---"],
    );
}

#[test]
fn field_visual_selects_within_columns() {
    check(&[A, B], "vjd", &[NOTE_GONE_A, NOTE_GONE_B]);
    check(
        &[A, B],
        "vjlx",
        &["--- -- cut:20 --- ---", "--- -- vol:30 --- ---"],
    );
    check(
        &[A, B],
        "lvj3=",
        &["C-3 43 cut:20 --- ---", "D-3 53 vol:30 --- ---"],
    );
}

#[test]
fn a_target_in_visual_mode_selects_it() {
    let start = notes("C-3", 8);
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
    let start = vec!["C-3 -- --- --- ---"; 4]
        .into_iter()
        .chain(vec!["D-3 -- --- --- ---"; 4])
        .collect::<Vec<_>>();
    let up_c = "C#3 -- --- --- ---";
    let up_d = "D#3 -- --- --- ---";
    let mut expected = vec![up_c; 4];
    expected.extend(vec![up_d; 4]);
    let (mut app, _rt) = app_with(&start);
    keys(&mut app, "Vib>");
    assert_eq!(app.mode, Mode::VisualStep, "the selection stays until Esc");
    keys(&mut app, "<Esc>}Vib.");
    assert_eq!(steps(&app), expected);
    keys(&mut app, "u");
    assert_eq!(steps(&app)[4], start[4], "undo takes back one change");
    assert_eq!(steps(&app)[0], up_c);
}

#[test]
fn reselect_restores_the_last_selection() {
    let (mut app, _rt) = app_with(&[A, B, C]);
    keys(&mut app, "Vj><Esc>");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "gv");
    assert_eq!(app.mode, Mode::VisualStep);
    assert_eq!(app.current_selection().range.last, 1);
    keys(&mut app, "<Esc>");
}

#[test]
fn motions_move_the_cursor_and_stop_at_edges() {
    let start = notes("C-3", 16);
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
    // `w` and `b` go between the note part and each FX slot, as the headings name them.
    keys(&mut app, "w");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    keys(&mut app, "l");
    assert_eq!(app.cursor.field, Field::FxValue(0));
    keys(&mut app, "w");
    assert_eq!(app.cursor.field, Field::FxCommand(1));
    keys(&mut app, "ww");
    assert_eq!(
        app.cursor.field,
        Field::FxCommand(2),
        "it stops at the last slot"
    );
    keys(&mut app, "b");
    assert_eq!(app.cursor.field, Field::FxCommand(1));
    keys(&mut app, "bb");
    assert_eq!(app.cursor.field, Field::Note);
    keys(&mut app, "llb");
    assert_eq!(
        app.cursor.field,
        Field::Note,
        "from the gate, back to the note"
    );
    keys(&mut app, "l2w");
    assert_eq!(
        app.cursor.field,
        Field::FxCommand(1),
        "a count goes further"
    );
    keys(&mut app, "20l");
    assert_eq!(app.cursor.field, Field::FxValue(2));
}

#[test]
fn viewport_keys_move_within_the_screen() {
    let start = notes("C-3", 64);
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
fn value_entry_commits_two_digits_and_stays() {
    check(&[E, E], "li7f", &["--- 7F --- --- ---", E]);
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "li2B<Esc>");
    assert_eq!(steps(&app)[0], "--- 2B --- --- ---");
    assert_eq!(app.cursor.step, 0);
}

#[test]
fn a_velocity_above_the_top_writes_the_top() {
    check(&[E, E], "liff", &["--- 7F --- --- ---", E]);
    check(&[FULL], "lrff", &["C-3 7F vol:10 pan:20 cut:30"]);
    // Other FX values have their own tops: the level scale's, or their whole range.
    check(&[FULL], "llllllrff", &["C-3 70 vol:F8 pan:20 cut:30"]);
    // `VOL` has no fine digits to pass, so `PAN` is a field nearer.
    check(&[FULL], "llllllllrff", &["C-3 70 vol:10 pan:FF cut:30"]);
}

#[test]
fn partly_typed_values_are_dropped_on_esc() {
    check(&[E], "li7<Esc>", &[E]);
}

#[test]
fn a_typed_command_takes_effect_only_with_its_value() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "llllliCUT<Enter>");
    assert_eq!(
        steps(&app)[0],
        E,
        "CUT must not be written before its value"
    );
    assert_eq!(app.cursor.field, Field::FxValue(0));
    keys(&mut app, "40<Esc>");
    assert_eq!(steps(&app)[0], "--- -- cut:40 --- ---");
}

#[test]
fn lowercase_commands_are_uppercased() {
    check(&[E], "llllliret<Enter>14<Esc>", &["--- -- ret:14 --- ---"]);
}

#[test]
fn an_invalid_command_is_refused() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "llllliXYZ<Enter>");
    assert_eq!(steps(&app), [A]);
    assert!(app.status.contains("isn't a label"), "{}", app.status);
    assert_eq!(app.picker.as_ref().unwrap().query, "xyz");
    keys(&mut app, "<Esc>");
    assert_eq!(steps(&app), [A]);
    assert_eq!(app.cursor.field, Field::FxCommand(0));
}

#[test]
fn esc_after_a_command_writes_nothing() {
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "llllliVOL<Enter><Esc>");
    assert_eq!(steps(&app), [A]);
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert!(app.status.contains("needs a value"));
}

#[test]
fn a_value_needs_a_command() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "lllllli40");
    assert_eq!(steps(&app), [E]);
    assert!(app.status.contains("FX command first"));
}

#[test]
fn a_value_on_an_existing_command_keeps_the_command() {
    check(&[A], "lllllli7F<Esc>", &["C-3 40 cut:7F --- ---"]);
}

#[test]
fn auto_jump_moves_entry_on_by_the_grid() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":set skip=2<Enter>i;a");
    assert_eq!(app.cursor.step, 2);
    keys(&mut app, "<Esc>li40");
    assert_eq!(steps(&app)[2], "--- 40 --- --- ---", "values move on too");
    assert_eq!(app.cursor.step, 3, "clamped at the last row");
}

#[test]
fn slides_keep_the_row_and_scroll_the_notes_under_it() {
    let (mut app, _rt) = app_with(&[A, B, C, E, E, E, E, E]);
    keys(&mut app, ":len 4<Enter>j");
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "]l");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 1);
    assert_eq!(app.cursor.step, 2, "still row 01");
    assert_eq!(app.top, 1, "and the screen moved with it");
    keys(&mut app, "[l");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 0);
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "]L");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 4);
    keys(&mut app, "[L");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 0);
    keys(&mut app, "2]l");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 2);
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 4);
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
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 0);
    assert_eq!(app.cursor.step, 1);
    keys(&mut app, "^r");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 2);
    assert_eq!(app.cursor.step, 3);
}

#[test]
fn sliding_above_the_first_step_adds_room_and_keeps_the_notes() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "2[l");
    let phrase = &app.song().tracks[0].phrases[&0];
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
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 2);
    assert_eq!(app.cursor.step, 2, "the cursor stays on its note");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 4);
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
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 2);
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

const FULL: &str = "C-3 70 vol:10 pan:20 cut:30";

#[test]
fn zero_and_dollar_go_to_the_ends_of_the_row() {
    let (mut app, _rt) = app_with(&[FULL, FULL]);
    keys(&mut app, "ll0");
    assert_eq!(app.cursor.field, Field::Note);
    // The last field is a setting's fine digits.
    keys(&mut app, "$");
    assert_eq!(app.cursor.field, Field::FxFine(2));
    keys(&mut app, "$");
    assert_eq!(app.cursor.field, Field::FxFine(2));
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
    check(&[FULL], "llld$", &["C-3 70 --- --- ---"]);
    check(&[FULL], "ld0", &["--- -- vol:10 pan:20 cut:30"]);
    check(&[FULL], "d0", &["--- 70 vol:10 pan:20 cut:30"]);
    check(
        &[FULL, FULL],
        "llld$j.",
        &["C-3 70 --- --- ---", "C-3 70 --- --- ---"],
    );
}

#[test]
fn a_selection_can_grow_to_the_ends_of_the_row() {
    check(&[FULL], "lv$d", &["C-3 -- --- --- ---"]);
    check(&[FULL], "lv0d", &["--- -- vol:10 pan:20 cut:30"]);
}

#[test]
fn r_replaces_a_note_and_keeps_everything_else() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "rk");
    assert_eq!(steps(&app)[0], "C-4 70 vol:10 pan:20 cut:30");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0);
    assert_eq!(app.cursor.field, Field::Note);
    keys(&mut app, "r1");
    assert_eq!(steps(&app)[0], "OFF 70 vol:10 pan:20 cut:30");
    keys(&mut app, "uu");
    assert_eq!(steps(&app)[0], FULL);
}

#[test]
fn r_ignores_the_note_velocity_setting() {
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "ic<Esc>");
    assert_eq!(app.note_velocity, 0x54);
    keys(&mut app, "rk");
    assert_eq!(steps(&app)[0], "C-4 70 vol:10 pan:20 cut:30");
    assert_eq!(app.note_velocity, 0x54);
}

#[test]
fn r_takes_a_whole_value_in_the_other_columns() {
    check(&[FULL], "lr40", &["C-3 40 vol:10 pan:20 cut:30"]);
    check(&[FULL], "lllllr04", &["C-3 70 vol:10 pan:20 cut:30"]);
    check(&[FULL], "llllllr7f", &["C-3 70 vol:7F pan:20 cut:30"]);
    check(
        &[FULL],
        "lllllrres<Enter>7f",
        &["C-3 70 res:7F pan:20 cut:30"],
    );
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "lllllrres<Enter>7f");
    assert_eq!(app.cursor.field, Field::FxCommand(0));
    assert_eq!(app.cursor.step, 0);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn r_needs_a_command_before_an_fx_value() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "llllllr");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("FX command"), "{}", app.status);
}

#[test]
fn escape_or_a_bad_value_leaves_r_unchanged() {
    check(&[FULL], "r<Esc>", &[FULL]);
    check(&[FULL], "lr4<Esc>", &[FULL]);
    check(&[FULL], "lllllrre<Esc>", &[FULL]);
    check(&[FULL], "lllllrres<Esc>", &[FULL]);
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "lllllrres<Esc>");
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
    check(
        &[FULL],
        "lr4<BS>5<BS><BS>70",
        &["C-3 70 vol:10 pan:20 cut:30"],
    );
    let (mut app, _rt) = app_with(&[FULL, FULL]);
    keys(&mut app, "jlr<BS>");
    assert_eq!(app.cursor.step, 1);
    assert_eq!(app.mode, Mode::Replace);
}

#[test]
fn dot_repeats_a_replace_at_the_cursor() {
    check(
        &[FULL, FULL, "--- -- --- --- ---"],
        "rkj.",
        &[
            "C-4 70 vol:10 pan:20 cut:30",
            "C-4 70 vol:10 pan:20 cut:30",
            "--- -- --- --- ---",
        ],
    );
    check(
        &[FULL, FULL],
        "lr40j.",
        &["C-3 40 vol:10 pan:20 cut:30", "C-3 40 vol:10 pan:20 cut:30"],
    );
    check(
        &[FULL, FULL],
        "lllllrres<Enter>7fj.",
        &["C-3 70 res:7F pan:20 cut:30", "C-3 70 res:7F pan:20 cut:30"],
    );
}

#[test]
fn a_count_on_r_does_not_change_more_than_this_field() {
    check(&[FULL, FULL], "3rk", &["C-4 70 vol:10 pan:20 cut:30", FULL]);
}

#[test]
fn s_clears_the_field_and_starts_insert_there() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "s");
    assert_eq!(steps(&app)[0], "--- 70 vol:10 pan:20 cut:30");
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.cursor.step, 0);
    // A note typed in Insert takes the current velocity, as it always does.
    keys(&mut app, "k");
    assert_eq!(steps(&app)[0], "C-4 64 vol:10 pan:20 cut:30");
    assert_eq!(app.cursor.step, 0, "Insert stays put as usual");
    keys(&mut app, "<Esc>uu");
    assert_eq!(steps(&app)[0], FULL);
}

#[test]
fn s_works_on_the_other_columns() {
    check(&[FULL, E], "ls40<Esc>", &["C-3 40 vol:10 pan:20 cut:30", E]);
    check(&[FULL, E], "ls<Esc>", &["C-3 -- vol:10 pan:20 cut:30", E]);
    check(
        &[FULL, E],
        "lllllsres<Enter>7f<Esc>",
        &["C-3 70 res:7F pan:20 cut:30", E],
    );
}

#[test]
fn s_on_an_fx_value_keeps_its_command_for_the_new_value() {
    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "lllllls");
    assert_eq!(steps(&app)[0], "C-3 70 --- pan:20 cut:30");
    keys(&mut app, "60<Esc>");
    assert_eq!(steps(&app)[0], "C-3 70 vol:60 pan:20 cut:30");

    let (mut app, _rt) = app_with(&[FULL, E]);
    keys(&mut app, "lllllls<Esc>");
    assert_eq!(steps(&app)[0], "C-3 70 --- pan:20 cut:30");
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
        &["--- 70 vol:10 pan:20 cut:30", FULL],
    );
}

#[test]
fn dot_does_not_repeat_s() {
    check(
        &[FULL, FULL],
        "rks<Esc>j.",
        &["--- 70 vol:10 pan:20 cut:30", "C-4 70 vol:10 pan:20 cut:30"],
    );
}

#[test]
fn r_on_a_note_with_no_velocity_fills_it_in() {
    check(&[E], "ra", &["C-3 64 --- --- ---"]);
}

const C3: &str = "C-3 64 --- --- ---";
const LOUD: &str = "E-3 7F --- --- ---";

#[test]
fn visual_r_fills_notes_on_every_selected_step() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, "vGra");
    assert_eq!(steps(&app), [C3, C3, C3, C3]);
    assert_eq!(
        app.mode,
        Mode::VisualField,
        "the selection stays, for another"
    );
    assert_eq!(app.cursor.step, 3, "the cursor's back where it was");
    assert_eq!(app.current_selection().range.len(), 4);
}

#[test]
fn visual_r_is_one_undo_however_many_steps() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, "vGra");
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, E, E, E]);
    keys(&mut app, "^r");
    assert_eq!(steps(&app), [C3, C3, C3, C3]);
}

#[test]
fn visual_r_only_covers_the_selection_and_keeps_what_notes_have() {
    let (mut app, _rt) = app_with(&[E, LOUD, E, E]);
    keys(&mut app, "vjra");
    assert_eq!(steps(&app), [C3, "C-3 7F --- --- ---", E, E]);
}

#[test]
fn visual_r_works_from_the_bottom_up_and_with_whole_steps() {
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "Gvkkra");
    assert_eq!(steps(&app), [C3, C3, C3]);
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "jVjra");
    assert_eq!(steps(&app), [E, C3, C3]);
}

#[test]
fn visual_r_writes_note_offs() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "vjr1");
    assert_eq!(steps(&app), ["OFF -- --- --- ---", "OFF -- --- --- ---",]);
}

#[test]
fn escape_leaves_a_selection_replace_with_nothing_written() {
    let (mut app, _rt) = app_with(&[E, E, E]);
    keys(&mut app, "vG");
    assert_eq!(app.cursor.step, 2);
    keys(&mut app, "r<Esc>");
    assert_eq!(steps(&app), [E, E, E]);
    assert_eq!(app.mode, Mode::VisualField, "back to the selection");
    assert_eq!(app.cursor.step, 2);
    // A later single replace isn't mistaken for the selection's.
    keys(&mut app, "<Esc>ra");
    assert_eq!(steps(&app), [E, E, C3]);
}

#[test]
fn visual_r_puts_a_velocity_only_on_notes() {
    let (mut app, _rt) = app_with(&[E, LOUD, E, C3]);
    keys(&mut app, "lvGr40");
    assert_eq!(
        steps(&app),
        [E, "E-3 40 --- --- ---", E, "C-3 40 --- --- ---"]
    );
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, LOUD, E, C3]);

    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lvjr");
    assert_eq!(app.mode, Mode::VisualField, "nothing to replace");
    assert!(app.status.contains("no notes"), "{}", app.status);
}

#[test]
fn visual_r_fills_fx_commands_and_keeps_commands_for_values() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lllllvjrCUT<Enter>40");
    assert_eq!(
        steps(&app),
        ["--- -- cut:40 --- ---", "--- -- cut:40 --- ---",]
    );
    keys(&mut app, "u");
    assert_eq!(steps(&app), [E, E]);

    let (mut app, _rt) = app_with(&[E, "--- -- res:10 --- ---", E, "--- -- cut:20 --- ---"]);
    keys(&mut app, "llllllvGr7F");
    assert_eq!(
        steps(&app),
        [E, "--- -- res:7F --- ---", E, "--- -- cut:7F --- ---",]
    );
    assert_eq!(app.cursor.step, 3);
}

#[test]
fn dot_repeats_a_selection_replace() {
    let (mut app, _rt) = app_with(&[E, E, E, E, E, E]);
    keys(&mut app, "vjra<Esc>");
    // In Normal it covers as many steps again, from the cursor.
    keys(&mut app, "j.");
    assert_eq!(steps(&app), [C3, C3, C3, C3, E, E]);
    keys(&mut app, "u");
    assert_eq!(steps(&app), [C3, C3, E, E, E, E]);
    // On a selection it covers that selection, and it doesn't run off the end.
    keys(&mut app, "vjjjj.");
    assert_eq!(steps(&app), [C3, C3, C3, C3, C3, C3]);
    keys(&mut app, "u<Esc>");
    keys(&mut app, "G.");
    assert_eq!(steps(&app), [C3, C3, E, E, E, C3]);
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
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 1);
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
    keys(&mut app, "lllllrVOL<Enter>FF");
    assert_eq!(steps(&app)[0], "--- -- vol:F8 --- ---");
    keys(&mut app, "lrFE");
    assert_eq!(steps(&app)[0], "--- -- vol:F8 --- ---");
    keys(&mut app, "20=");
    assert_eq!(steps(&app)[0], "--- -- vol:F8 --- ---");

    // Filling a selection snaps too, and other commands keep their whole range.
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lllllvjrSNA<Enter>FF");
    assert_eq!(steps(&app)[1], "--- -- sna:F8 --- ---");
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "lllllrCUT<Enter>FF");
    assert_eq!(steps(&app)[0], "--- -- cut:FF --- ---");
}

#[test]
fn ge_ends_the_loop_on_the_cursor_and_keeps_what_is_stored() {
    let (mut app, _rt) = app_with(&[A, B, E, E, E, E]);
    let length = |app: &App| app.song().tracks[0].phrases[&0].len();
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
    let phrase = &app.song().tracks[0].phrases[&0];
    assert_eq!((phrase.start(), phrase.len()), (1, 3));
}

#[test]
fn ge_refuses_above_the_start_and_leaves_no_undo_when_nothing_changes() {
    let (mut app, _rt) = app_with(&[E, E, E, E]);
    keys(&mut app, ":len 2<Enter>]l");
    let depth = app.history.undo_depth();
    app.cursor.step = 0;
    keys(&mut app, "ge");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 2);
    assert!(app.status.contains("above its start"), "{}", app.status);
    app.cursor.step = 2;
    keys(&mut app, "ge");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 2);
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn len_takes_hex_after_0x() {
    let (mut app, _rt) = app_with(&[E; 32]);
    keys(&mut app, ":len 0x10<Enter>");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 16);
    keys(&mut app, ":len 10<Enter>");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 10);
}

#[test]
fn plus_and_underscore_are_large_steps() {
    // A note moves by octaves, and hex values by their top digit.
    check(&[A], "+", &["C-4 40 cut:20 --- ---"]);
    check(&[A], "_", &["C-2 40 cut:20 --- ---"]);
    check(&[A], "l+", &["C-3 50 cut:20 --- ---"]);
    check(&[A], "l_", &["C-3 30 cut:20 --- ---"]);
    check(&[A], "lllll+", &["C-3 40 cut:30 --- ---"]);
    check(&[A], "lllll2_", &["C-3 40 cut:00 --- ---"]);
    // They stop at each field's top.
    check(&[A], "l9+", &["C-3 7F cut:20 --- ---"]);
    check(&[A], "lllll99+", &["C-3 40 cut:FF --- ---"]);
    check(&[B], "lllll99+", &["D-3 50 vol:F8 --- ---"]);
}

#[test]
fn equals_and_minus_are_small_steps_and_carry() {
    check(&[A], "=", &["C#3 40 cut:20 --- ---"]);
    check(&[A], "-", &["B-2 40 cut:20 --- ---"]);
    check(
        &["--- -- cut:7F --- ---"],
        "lllll=",
        &["--- -- cut:80 --- ---"],
    );
}

#[test]
fn ret_moves_each_digit_alone_and_never_carries() {
    let ret = "--- -- ret:1F --- ---";
    check(&[ret], "lllll=", &[ret]);
    check(&[ret], "lllll-", &["--- -- ret:1E --- ---"]);
    check(&[ret], "lllll+", &["--- -- ret:2F --- ---"]);
    check(&[ret], "lllll_", &["--- -- ret:0F --- ---"]);
    check(&[ret], "lllll_lll_", &["--- -- ret:0F --- ---"]);
    let top = "--- -- ret:F4 --- ---";
    check(&[top], "lllll+", &[top]);
}

#[test]
fn large_steps_work_on_a_selection_and_repeat() {
    let (mut app, _rt) = app_with(&[A, B]);
    keys(&mut app, "vj+");
    assert_eq!(
        steps(&app),
        ["C-4 40 cut:20 --- ---", "D-4 50 vol:30 --- ---"]
    );
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "lllll+.");
    assert_eq!(steps(&app), ["C-3 40 cut:40 --- ---"]);
}

#[test]
fn k_chooses_an_fx_command_and_an_empty_slot_waits_for_its_value() {
    let (mut app, _rt) = app_with(&[E, E]);
    keys(&mut app, "lllllK");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        app.picker.as_ref().unwrap().items().len(),
        46,
        "on a sample track with nothing in its slots: empty, its own 16, SMP, SLI, STA, SLEN and PLY, the mod slots' 16 and 8 events"
    );
    keys(&mut app, "cut<Enter>");
    assert_eq!(app.mode, Mode::Replace);
    assert_eq!(steps(&app)[0], E, "nothing until the value is typed");
    keys(&mut app, "40");
    assert_eq!(steps(&app)[0], "--- -- cut:40 --- ---");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.cursor.step, 0, "it stays on this step");
    assert_eq!(app.cursor.field, Field::FxValue(0));
}

#[test]
fn k_keeps_a_value_when_changing_the_command() {
    let (mut app, _rt) = app_with(&[A]);
    // On the command or the value, and it opens on what is there.
    keys(&mut app, "llllllK");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.as_str()),
        Some("CUT"),
        "inside Filter, which opens"
    );
    keys(&mut app, "res<Enter>");
    assert_eq!(steps(&app)[0], "C-3 40 res:20 --- ---");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "u");
    assert_eq!(steps(&app)[0], A);

    // A level command snaps what it keeps.
    let (mut app, _rt) = app_with(&["--- -- cut:FF --- ---"]);
    keys(&mut app, "lllllKvol<Enter>");
    assert_eq!(steps(&app)[0], "--- -- vol:F8 --- ---");
}

#[test]
fn k_can_empty_a_slot_and_esc_leaves_it() {
    let (mut app, _rt) = app_with(&[A]);
    // The list opens on its search: the first Esc goes to the list, the second leaves it.
    keys(&mut app, "lllllK<Esc>");
    assert_eq!(app.mode, Mode::Picker);
    keys(&mut app, "<Esc>");
    assert_eq!(steps(&app)[0], A);
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "K---<Enter>");
    assert_eq!(steps(&app)[0], "C-3 40 --- --- ---");

    // Leaving before the value writes nothing.
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "lllllKcut<Enter><Esc>");
    assert_eq!(steps(&app)[0], E);
    assert_eq!(app.mode, Mode::Normal);
}

/// Notes on rows 0, 1 and 3 of 4, at the velocities given, with the second row's slot holding `CUT`.
fn velocities(first: &str, second: &str, last: &str) -> Vec<String> {
    vec![
        format!("C-3 {first} --- --- ---"),
        format!("C-3 {second} --- --- ---"),
        E.to_owned(),
        format!("C-3 {last} --- --- ---"),
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
    keys(&mut app, "lvGSj+=<Enter>");
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
    keys(&mut app, "j~");
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
        "C-3 -- cut:10 --- ---",
        E,
        "C-3 -- --- --- ---",
        "--- -- cut:30 --- ---",
    ]);
    keys(&mut app, "lllllvGS");
    let shape = |app: &App| app.shape.as_ref().unwrap().shape;
    let fx = |app: &App| -> Vec<String> {
        steps(app)
            .iter()
            .map(|s| s.split(' ').nth(2).unwrap().to_owned())
            .collect()
    };
    assert_eq!(fx(&app), ["cut:10", "---", "---", "cut:30"]);

    keys(&mut app, "jK");
    assert_eq!(app.mode, Mode::Shape, "K isn't a list on From");
    assert!(app.status.contains("Fill"), "{}", app.status);

    // Where, from Commands to All rows, writes the command on the empty rows.
    keys(&mut app, "jjjjjKall<Enter>");
    assert_eq!(app.mode, Mode::Shape);
    // A ramp on a setting fills its fine part, so it moves evenly.
    assert_eq!(fx(&app), ["cut:10", "cut:1AAB", "cut:2555", "cut:30"]);

    keys(&mut app, "Knotes<Enter>");
    assert_eq!(fx(&app)[0], "cut:10");
    assert_eq!(fx(&app)[1], "---");
    assert_eq!(
        fx(&app)[3],
        "cut:30",
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
    keys(&mut app, "jjjjjjK<Esc>");
    assert_eq!(app.mode, Mode::Shape);
    keys(&mut app, "<Esc>");
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn a_euclidean_where_spreads_its_hits_among_the_notes_and_rotates() {
    use vt_model::shape::Where;
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_with(&["C-3 40 --- --- ---"; 8]);
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    keys(&mut app, "Kalt<Enter>ji7Fji00jjKeuc<Enter>");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 3, rotate: 0 }
    );
    // The tresillo, then Alternate counts among the rows it changes.
    assert_eq!(vel(&app), ["7F", "40", "40", "00", "40", "40", "7F", "40"]);
    keys(&mut app, "j");
    assert_eq!(panel(&app).row, ShapeRow::Hits);
    keys(&mut app, "==");
    assert_eq!(vel(&app), ["7F", "40", "00", "7F", "40", "00", "7F", "40"]);
    keys(&mut app, "+");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 8, rotate: 0 },
        "no more than 8"
    );
    keys(&mut app, "i5<Enter>j");
    assert_eq!(panel(&app).row, ShapeRow::Rotate);
    keys(&mut app, "-");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 5, rotate: 7 },
        "it goes round"
    );
    keys(&mut app, "i9<Enter>");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 5, rotate: 1 }
    );
    assert_eq!(vel(&app), ["40", "7F", "40", "00", "7F", "40", "00", "7F"]);
    keys(&mut app, "x");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 5, rotate: 0 }
    );
    keys(&mut app, "kx");
    assert_eq!(
        panel(&app).shape.place,
        Where::Euclid { hits: 3, rotate: 0 }
    );
    // Another Where hides Hits and Rotate.
    keys(&mut app, "kKnotes<Enter>");
    assert!(!panel(&app).rows().contains(&ShapeRow::Hits));
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), 1);
}

#[test]
fn the_transforms_rework_the_values_there_as_one_undo() {
    use vt_tui::shaping::ShapeRow;
    let start = [
        "C-3 10 --- --- ---",
        E,
        "C-3 30 --- --- ---",
        "C-3 70 --- --- ---",
    ];
    let (mut app, _rt) = app_with(&start);
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    keys(&mut app, "Kinvert<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Invert);
    assert_eq!(panel(&app).rows(), [ShapeRow::Fill, ShapeRow::Where]);
    assert_eq!(vel(&app), ["70", "--", "50", "10"]);
    keys(&mut app, "=");
    assert_eq!(panel(&app).shape.kind, FillKind::Shuffle);
    let dealt = vel(&app);
    let mut sorted = dealt.clone();
    sorted.sort();
    assert_eq!(sorted, ["--", "10", "30", "70"]);
    for _ in 0..8 {
        keys(&mut app, "r");
        if vel(&app) != dealt {
            break;
        }
    }
    assert_ne!(vel(&app), dealt, "r deals again");
    keys(&mut app, "=");
    assert_eq!(panel(&app).shape.kind, FillKind::Reverse);
    assert_eq!(vel(&app), ["70", "--", "30", "10"]);
    keys(&mut app, "-");
    keys(&mut app, "-");
    keys(&mut app, "-");
    assert_eq!(panel(&app).shape.kind, FillKind::Scale);
    assert!(panel(&app).rows().contains(&ShapeRow::Curve));
    keys(&mut app, "ji40ji60");
    assert_eq!(vel(&app), ["40", "--", "4B", "60"]);
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), 1);
    keys(&mut app, "u");
    assert_eq!(steps(&app), start);
}

#[test]
fn a_ramp_holds_for_its_run_and_a_wave_takes_a_rate_in_cycles() {
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_with(&["C-3 40 --- --- ---"; 8]);
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    keys(&mut app, "ji00ji30jjj");
    assert_eq!(panel(&app).row, ShapeRow::Run);
    keys(&mut app, "=");
    assert_eq!(vel(&app), ["00", "00", "10", "10", "20", "20", "30", "30"]);
    keys(&mut app, "kkkkkKwave<Enter>");
    assert_eq!(panel(&app).shape.run, 2, "a run is kept across fills");
    keys(&mut app, "jjjKsmooth<Enter>");
    assert_eq!(panel(&app).shape.wave.name(), "Smooth");
    keys(&mut app, "ji0<Enter>");
    assert!(app.status.contains("x3"), "{}", app.status);
    keys(&mut app, "ix2<Enter>");
    assert_eq!(panel(&app).shape.rate.to_string(), "x2");
    keys(&mut app, "=");
    assert_eq!(panel(&app).shape.rate.to_string(), "x3");
    keys(&mut app, "Kx5<Enter>");
    assert_eq!(panel(&app).shape.rate.to_string(), "x5");
    keys(&mut app, "x");
    assert_eq!(panel(&app).shape.rate.to_string(), "1/2");
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), 1);
}

#[test]
fn random_fill_stays_in_range_and_rerolls() {
    let rows = vec!["C-3 40 --- --- ---".to_owned(); 64];
    let (mut app, _rt) = app_of(&rows);
    keys(&mut app, "lvGS");
    keys(&mut app, "Krandom<Enter>jji20");
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
fn the_panel_fills_with_a_wave_and_shows_only_its_settings() {
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    assert!(panel(&app).rows().contains(&ShapeRow::Curve));
    // It opens on Fill, and `=` goes on past Random, Walk and 1/f.
    assert_eq!(panel(&app).row, ShapeRow::Fill);
    keys(&mut app, "====");
    assert_eq!(panel(&app).shape.kind, FillKind::Wave);
    let rows = panel(&app).rows();
    assert!(!rows.contains(&ShapeRow::Curve));
    assert!(rows.contains(&ShapeRow::Rate));
    assert_eq!(panel(&app).row, ShapeRow::Fill, "the cursor stays on Fill");

    keys(&mut app, "jjj=");
    assert_eq!(panel(&app).row, ShapeRow::Wave);
    assert_eq!(panel(&app).shape.wave.name(), "Tri");
    keys(&mut app, "j");
    assert_eq!(panel(&app).shape.rate.to_string(), "1/2");
    keys(&mut app, "i1/4<Enter>");
    assert_eq!(panel(&app).shape.rate.to_string(), "1/4");
    keys(&mut app, "i2hz<Enter>");
    assert_eq!(
        panel(&app).shape.rate.to_string(),
        "1/4",
        "not free of the grid"
    );
    keys(&mut app, "i7st<Enter>");
    assert_eq!(panel(&app).shape.rate.to_string(), "7st");
    keys(&mut app, "x+");
    assert_eq!(
        panel(&app).shape.rate.to_string(),
        "1/4",
        "faster, as on an LFO"
    );
    keys(&mut app, "j=_");
    assert_eq!(panel(&app).shape.phase, 285, "it goes round");
    keys(&mut app, "i90<Enter>");
    assert_eq!(panel(&app).shape.phase, 90);
    keys(&mut app, "i360<Enter>");
    assert_eq!(panel(&app).shape.phase, 90, "that's refused");
    keys(&mut app, "x");
    assert_eq!(panel(&app).shape.phase, 0);

    keys(&mut app, "<Enter>");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.history.undo_depth(), 1);
}

#[test]
fn the_panel_alternates_and_a_list_chooses_a_wave() {
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    keys(&mut app, "K");
    keys(&mut app, "alt<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Alternate);
    assert_eq!(vel(&app), ["10", "40", "--", "10"]);
    keys(&mut app, "jjj");
    assert_eq!(panel(&app).row, ShapeRow::Run);
    keys(&mut app, "=");
    assert_eq!(panel(&app).shape.run, 2);
    assert_eq!(vel(&app), ["10", "10", "--", "40"]);
    keys(&mut app, "i0<Enter>");
    assert_eq!(panel(&app).shape.run, 2, "that's refused");
    keys(&mut app, "x");
    assert_eq!(panel(&app).shape.run, 1);

    // Fill's list also goes to Wave, and Shape and Rate have lists of their own.
    keys(&mut app, "kkk");
    keys(&mut app, "Kwav<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Wave);
    keys(&mut app, "jjjKsquare<Enter>");
    assert_eq!(panel(&app).shape.wave.name(), "Square");
    keys(&mut app, "jK");
    assert!(app.picker.is_some());
    keys(&mut app, "<Esc><Esc>");
}

#[test]
fn dot_keeps_the_two_values_a_wave_or_alternate_moves_between() {
    let (mut app, _rt) = app_with(&[
        "C-3 10 --- --- ---",
        "C-3 10 --- --- ---",
        "C-3 70 --- --- ---",
        "C-3 7F --- --- ---",
        "C-3 00 --- --- ---",
        "C-3 30 --- --- ---",
    ]);
    keys(&mut app, "lvjjS");
    keys(&mut app, "Kalt<Enter>");
    keys(&mut app, "ji20ji60<Enter>");
    assert_eq!(vel(&app)[..3], ["20", "60", "20"]);
    keys(&mut app, "3jvjj.");
    assert_eq!(vel(&app)[3..], ["20", "60", "20"]);
}

#[test]
fn a_list_fill_types_its_values_and_goes_round_them() {
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_with(&[
        "C-3 10 --- --- ---",
        "C-3 7F --- --- ---",
        E,
        "C-3 7F --- --- ---",
        "C-3 7F --- --- ---",
        "C-3 7F --- --- ---",
        "C-3 40 --- --- ---",
    ]);
    keys(&mut app, "lvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    let list = |app: &App| panel(app).shape.values.to_string();

    // It starts as the two ends, so it sounds as Alternate did, and they're hidden.
    keys(&mut app, "K/list<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::List);
    assert_eq!(list(&app), "10 40");
    let rows = panel(&app).rows();
    assert!(!rows.contains(&ShapeRow::From) && !rows.contains(&ShapeRow::To));
    assert_eq!(vel(&app), ["10", "40", "--", "10", "40", "10", "40"]);

    // The values are typed: two digits make a value, a space ends a shorter one.
    keys(&mut app, "j");
    assert_eq!(panel(&app).row, ShapeRow::Values);
    keys(&mut app, "=");
    assert!(app.status.contains("Type the values"), "{}", app.status);
    assert_eq!(list(&app), "10 40");
    keys(&mut app, "i704055");
    assert_eq!(panel(&app).typing.as_deref(), Some("70 40 55"));
    keys(&mut app, "<Enter>");
    assert_eq!(list(&app), "70 40 55");
    assert_eq!(vel(&app), ["70", "40", "--", "55", "70", "40", "55"]);
    keys(&mut app, "i7 4<BS>40<Enter>");
    assert_eq!(list(&app), "07 40");
    keys(&mut app, "i<Enter>");
    assert!(app.status.contains("hex values"), "{}", app.status);
    assert_eq!(list(&app), "07 40", "nothing typed keeps the list");
    keys(&mut app, "i1020304050607071727<Enter>");
    assert_eq!(list(&app), "10 20 30 40 50 60 70 71", "eight at most");
    keys(&mut app, "iFF 20<Enter>");
    assert_eq!(list(&app), "7F 20", "a value stops at the column's top");
    keys(&mut app, "i30<Esc>");
    assert_eq!(list(&app), "7F 20");

    // Run gives each value that many rows, and ~ turns the list round.
    keys(&mut app, "i704055<Enter>j=");
    assert_eq!(vel(&app), ["70", "70", "--", "40", "40", "55", "55"]);
    keys(&mut app, "~");
    assert_eq!(list(&app), "55 40 70");
    keys(&mut app, "kx");
    assert_eq!(list(&app), "10 40", "the default is the two ends");
    keys(&mut app, "i704055<Enter><Enter>");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.history.undo_depth(), 1);

    // `.` keeps the list, as Alternate keeps its values.
    keys(&mut app, "u");
    keys(&mut app, "jjjvjjj.");
    assert_eq!(vel(&app), ["10", "7F", "--", "70", "70", "40", "40"]);
}

/// The note on each row.
fn notes_of(app: &App) -> Vec<String> {
    steps(app).iter().map(|s| s[..3].to_owned()).collect()
}

#[test]
fn shape_on_the_note_column_re_pitches_in_degrees_and_snaps() {
    use vt_tui::shaping::ShapeRow;
    let start = [
        "C-3 40 --- --- ---",
        "C-3 20 --- --- ---",
        E,
        "OFF -- --- --- ---",
        "C-3 70 --- --- ---",
        "C#3 30 --- --- ---",
    ];
    let (mut app, _rt) = app_with(&start);
    app.scale.on = true;
    keys(&mut app, "vGS");
    assert_eq!(app.mode, Mode::Shape);
    let panel = |app: &App| app.shape.clone().unwrap();
    // It opens on the first and last notes, and keeps the rhythm, velocities, offs and rests.
    assert_eq!((panel(&app).shape.from, panel(&app).shape.to), (60, 61));
    keys(&mut app, "jj");
    assert_eq!(panel(&app).row, ShapeRow::To);
    keys(&mut app, "ic4<Enter>");
    assert_eq!(panel(&app).shape.to, 72);
    assert_eq!(notes_of(&app), ["C-3", "D-3", "---", "OFF", "B-3", "C-4"]);
    assert_eq!(vel(&app), ["40", "20", "--", "--", "70", "30"]);
    // `=` moves an end a degree, `+` an octave.
    keys(&mut app, "-");
    assert_eq!(panel(&app).shape.to, 71);
    keys(&mut app, "_");
    assert_eq!(panel(&app).shape.to, 59);
    keys(&mut app, "ih3<Enter>");
    assert!(app.status.contains("C#3"), "{}", app.status);
    keys(&mut app, "x");
    assert_eq!(panel(&app).shape.to, 61);

    // Snap, only on notes, moves each to the nearest degree: C# down to C.
    keys(&mut app, "kkKsnap<Enter>");
    assert_eq!(panel(&app).shape.kind, vt_model::shape::FillKind::Snap);
    assert_eq!(notes_of(&app), ["C-3", "C-3", "---", "OFF", "C-3", "C-3"]);
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), 1);

    // A motif is typed as notes.
    keys(&mut app, "u");
    keys(&mut app, "vGSK/list<Enter>j");
    assert_eq!(panel(&app).row, ShapeRow::Values);
    keys(&mut app, "ic3 d#3 g3<Enter>");
    assert_eq!(notes_of(&app), ["C-3", "D#3", "---", "OFF", "G-3", "C-3"]);
    keys(&mut app, "ic3 q3<Enter>");
    assert!(app.status.contains("C-3"), "{}", app.status);
    keys(&mut app, "<Esc>");
    assert_eq!(steps(&app), start);
}

#[test]
fn shape_fills_a_nudge_and_a_length() {
    let (mut app, _rt) = app_with(&[
        "C-3 40 --- --- ---",
        E,
        "C-3 40 --- --- ---",
        "C-3 40 SGL1 80 --- --- ---",
    ]);
    keys(&mut app, "llvGS");
    let panel = |app: &App| app.shape.clone().unwrap();
    assert_eq!(panel(&app).column, vt_model::shape::ShapeColumn::Nudge);
    keys(&mut app, "ji-3<Enter>ji+3<Enter>");
    assert_eq!(
        steps(&app),
        [
            "C-3 40 -3 --- --- ---",
            E,
            "C-3 40 +1 --- --- ---",
            "C-3 40 +3 SGL1 80 --- --- ---",
        ]
    );
    keys(&mut app, "<Enter>");
    keys(&mut app, "ggllvGS");
    assert_eq!(panel(&app).column, vt_model::shape::ShapeColumn::Length);
    keys(&mut app, "ji20ji80");
    assert_eq!(
        steps(&app),
        [
            "C-3 40 -3 SGL1 20 --- --- ---",
            E,
            "C-3 40 +1 SGL1 60 --- --- ---",
            "C-3 40 +3 SGL1 80 --- --- ---",
        ]
    );
}

#[test]
fn shape_refuses_what_it_cannot_fill() {
    let reason = |rows: &[&str], typed: &str| {
        let (mut app, _rt) = app_with(rows);
        keys(&mut app, typed);
        assert_ne!(app.mode, Mode::Shape, "{typed}");
        app.status.clone()
    };
    let notes = ["C-3 -- --- --- ---"; 3];
    assert!(reason(&notes, "lllvGS").contains("length"));
    assert!(reason(&notes, "VGS").contains("one column"));
    assert!(reason(&notes, "lvllGS").contains("one column"));
    assert!(reason(&notes, "lvS").contains("2 rows"));
    // A velocity, nudge or length still needs notes; only the Note and FX columns generate.
    assert!(reason(&[E, E], "lvGS").contains("no notes"));
    assert!(reason(&[E, E], "llvGS").contains("no notes"));
}

/// Each row's note and velocity, like `C-3 50`.
fn notes_and_velocities(app: &App) -> Vec<String> {
    steps(app).iter().map(|s| s[..6].to_owned()).collect()
}

#[test]
fn s_on_an_empty_note_column_writes_notes_in_key_with_the_rhythm_where_picks() {
    use vt_model::shape::{ShapeColumn, Where};
    let start = [E; 8];
    let (mut app, _rt) = app_with(&start);
    app.scale.on = true;
    app.note_velocity = 0x50;
    let depth = app.history.undo_depth();
    keys(&mut app, "vGS");
    assert_eq!(app.mode, Mode::Shape);
    let panel = app.shape.clone().unwrap();
    assert!(panel.generates());
    assert_eq!(panel.verb(), "Generate");
    assert!(matches!(
        panel.column,
        ShapeColumn::Note {
            writes: Some(0x50),
            ..
        }
    ));
    // Every row, a run up the scale an octave from the root, at the typed velocity.
    assert_eq!(panel.shape.place, Where::AllRows);
    assert_eq!(panel.wheres()[0], Where::AllRows);
    assert!(!panel.wheres().contains(&Where::Notes));
    assert!(panel.fills().iter().all(|kind| !kind.reworks()));
    assert_eq!(
        notes_and_velocities(&app),
        [
            "C-3 50", "D-3 50", "E-3 50", "F-3 50", "G-3 50", "A-3 50", "B-3 50", "C-4 50"
        ]
    );
    // Every 4 rows.
    keys(&mut app, "jjjjjj=");
    assert!(matches!(
        app.shape.as_ref().unwrap().shape.place,
        Where::Every(4)
    ));
    assert_eq!(
        notes_of(&app),
        ["C-3", "---", "---", "---", "C-4", "---", "---", "---"]
    );
    // A tresillo, from Euclid.
    keys(&mut app, "==");
    assert!(matches!(
        app.shape.as_ref().unwrap().shape.place,
        Where::Euclid { .. }
    ));
    assert_eq!(
        notes_of(&app),
        ["C-3", "---", "---", "G-3", "---", "---", "C-4", "---"]
    );
    // A random fill stays in key, and the same seed picks the same.
    keys(&mut app, "kkkkkkKrandom<Enter>");
    let picked = notes_of(&app);
    let scale = app.scale;
    for note in picked.iter().filter(|note| *note != "---") {
        let Ok(vt_model::step::Note::On(midi)) = note.parse() else {
            panic!("{note}");
        };
        assert!(scale.contains(midi), "{note} is out of key");
    }
    keys(&mut app, "=-");
    assert_eq!(notes_of(&app), picked);
    // One undo for Enter.
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(steps(&app), start);
    // `.` generates again on an empty column: three notes, from the tresillo.
    keys(&mut app, ".");
    let written = notes_of(&app).iter().filter(|note| *note != "---").count();
    assert_eq!(written, 3);
    keys(&mut app, "u");
    // Esc puts everything back.
    keys(&mut app, "vGS<Esc>");
    assert_eq!(steps(&app), start);
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn s_on_a_note_column_with_notes_still_re_pitches_them_and_writes_none() {
    let start = ["C-3 40 --- --- ---", E, E, "D-3 20 --- --- ---"];
    let (mut app, _rt) = app_with(&start);
    keys(&mut app, "vGS");
    let panel = app.shape.clone().unwrap();
    assert!(!panel.generates());
    assert_eq!(panel.verb(), "Reshape");
    assert_eq!(panel.shape.place, vt_model::shape::Where::Notes);
    assert!(panel.fills().contains(&FillKind::Reverse));
    keys(&mut app, "jic3<Enter>jie3<Enter>");
    assert_eq!(
        notes_and_velocities(&app),
        ["C-3 40", "--- --", "--- --", "E-3 20"]
    );
}

#[test]
fn s_on_an_empty_fx_column_chooses_a_command_then_writes_it() {
    let start = [E; 4];
    let (mut app, _rt) = app_with(&start);
    let depth = app.history.undo_depth();
    keys(&mut app, "lllllvjjjS");
    // The command comes first, from the K list.
    assert_eq!(app.mode, Mode::Picker);
    assert!(app.shape.is_none());
    // Esc gives up, with nothing changed.
    keys(&mut app, "<Esc><Esc>");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(steps(&app), start);
    keys(&mut app, "vjjjScut<Enter>");
    assert_eq!(app.mode, Mode::Shape, "{}", app.status);
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert_eq!(panel.shape.place, vt_model::shape::Where::AllRows);
    assert!(!panel.wheres().contains(&vt_model::shape::Where::Commands));
    keys(&mut app, "ji00ji30");
    assert_eq!(
        steps(&app),
        [
            "--- -- cut:00 --- ---",
            "--- -- cut:10 --- ---",
            "--- -- cut:20 --- ---",
            "--- -- cut:30 --- ---",
        ]
    );
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(steps(&app), start);
    // With a command already there, it reshapes that one, as ever.
    let (mut app, _rt) = app_with(&["C-3 40 cut:00 --- ---", E, "C-3 40 cut:40 --- ---"]);
    keys(&mut app, "lllllvjjS");
    assert_eq!(app.mode, Mode::Shape);
    assert_eq!(app.shape.as_ref().unwrap().verb(), "Reshape");
}

#[test]
fn s_across_tracks_generates_on_each_empty_column_and_reshapes_the_rest() {
    let rows = [N; 4];
    let (mut app, _rt) = tracks_with(&[&rows, &[GONE; 4], &rows]);
    keys(&mut app, "vjjjWWS");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.parts.len(), 3);
    assert_eq!(
        panel.parts.iter().map(|p| p.generate).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(panel.verb(), "Generate and reshape");
    // Notes, as the first track has them: the empty track takes every row.
    assert_eq!(panel.shape.place, vt_model::shape::Where::Notes);
    keys(&mut app, "jic3<Enter>jic4<Enter>");
    let pitches = |app: &App, track: usize| -> Vec<String> {
        track_steps(app, track)
            .iter()
            .map(|s| s[..3].to_owned())
            .collect()
    };
    assert_eq!(pitches(&app, 0), ["C-3", "E-3", "G#3", "C-4"]);
    assert_eq!(pitches(&app, 1), ["C-3", "E-3", "G#3", "C-4"]);
    assert_eq!(track_steps(&app, 0)[1], "E-3 40 cut:20 --- ---");
    let depth = app.history.undo_depth();
    keys(&mut app, "<Enter>");
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "one undo for all three"
    );

    // An FX column: the empty track takes the command chosen, the other keeps its own.
    let (mut app, _rt) = tracks_with(&[&[N, N], &[GONE, GONE]]);
    keys(&mut app, "lllllvjWSres<Enter>");
    assert_eq!(app.mode, Mode::Shape, "{}", app.status);
    keys(&mut app, "ji00ji40<Enter>");
    assert!(
        track_steps(&app, 0)[1].contains("cut:40"),
        "{:?}",
        track_steps(&app, 0)
    );
    assert!(
        track_steps(&app, 1)[1].contains("res:40"),
        "{:?}",
        track_steps(&app, 1)
    );
}

#[test]
fn l_keeps_generated_notes_live_and_gl_reopens_them() {
    use vt_model::shape::Where;
    let (mut app, _rt) = app_with(&[E; 8]);
    keys(&mut app, "vGSjjjjjj===L");
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(
        notes_of(&app),
        ["C-3", "---", "---", "F#3", "---", "---", "C-4", "---"]
    );
    let depth = app.history.undo_depth();
    // `gl` reopens it generating, and turning its Rotate moves the notes, velocities and all.
    keys(&mut app, "jjgl");
    assert_eq!(app.mode, Mode::Shape);
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert_eq!(panel.shape.place, Where::Euclid { hits: 3, rotate: 0 });
    keys(&mut app, "jjjjjjjj=L");
    assert_eq!(
        notes_and_velocities(&app),
        [
            "--- --", "C-3 64", "--- --", "--- --", "F#3 64", "--- --", "--- --", "C-4 64"
        ]
    );
    assert_eq!(app.history.undo_depth(), depth + 1);
    // It owns the velocities it wrote too, so a hand edit to one applies it.
    keys(&mut app, "ggjlr10");
    assert!(generators(&app, 0).is_empty());
    assert_eq!(notes_and_velocities(&app)[1], "C-3 10");
    assert!(app.status.contains("plain values"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(generators(&app, 0).len(), 1);
}

/// Each row's note name without its octave, `--` for none, and whether it plays.
fn pitch_classes(app: &App) -> Vec<String> {
    steps(app).iter().map(|s| s[..2].to_owned()).collect()
}

#[test]
fn acid_writes_a_riff_with_accents_and_slides_and_keeps_its_pitches_on_a_roll() {
    use vt_tui::shaping::ShapeRow;
    let (mut app, _rt) = app_with(&[E; 16]);
    app.note_velocity = 0x50;
    let depth = app.history.undo_depth();
    keys(&mut app, "vGSKacid<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Acid);
    assert_eq!(
        panel.rows(),
        [
            ShapeRow::Fill,
            ShapeRow::From,
            ShapeRow::To,
            ShapeRow::Notes,
            ShapeRow::Octave,
            ShapeRow::Density,
            ShapeRow::Accent,
            ShapeRow::Slide,
            ShapeRow::Pitches,
            ShapeRow::Where,
        ]
    );
    // A riff over the octave from C-3, an octave either way at most, plain notes at the typed
    // velocity and accents at full.
    let written = |app: &App| -> Vec<String> {
        steps(app)
            .into_iter()
            .filter(|s| !s.starts_with("---"))
            .collect()
    };
    let riff = written(&app);
    assert!(riff.len() > 3, "{riff:?}");
    for step in &riff {
        let Ok(vt_model::step::Note::On(note)) = step[..3].parse() else {
            panic!("{step}");
        };
        assert!((48..=84).contains(&note), "{step}");
        assert!(step[4..6] == *"50" || step[4..6] == *"7F", "{step}");
    }
    // `r` rolls a new riff.
    keys(&mut app, "r");
    assert_ne!(written(&app), riff);
    // Turn the slides up to every note, then keep the pitches: `r` rolls only the accents,
    // slides and octaves, so each note keeps its name and row.
    keys(&mut app, "jjjjjjj");
    assert_eq!(app.shape.as_ref().unwrap().row, ShapeRow::Slide);
    keys(&mut app, "i100<Enter>");
    assert!(written(&app).iter().all(|step| step.contains("HLD")));
    keys(&mut app, "j=");
    assert!(app.shape.as_ref().unwrap().shape.acid.keep);
    let kept = pitch_classes(&app);
    let accents =
        |app: &App| -> Vec<String> { written(app).iter().map(|s| s[4..6].to_owned()).collect() };
    let mut rolled = false;
    for _ in 0..6 {
        let before = accents(&app);
        keys(&mut app, "r");
        assert_eq!(pitch_classes(&app), kept);
        rolled |= accents(&app) != before;
    }
    assert!(rolled, "a roll changes the accents");
    // Kept as one undo, and the panel's own steps added none.
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert!(steps(&app).iter().all(|s| s == E));
}

#[test]
fn acid_kept_live_rolls_again_through_gl() {
    let (mut app, _rt) = app_with(&[E; 16]);
    keys(&mut app, "vGSKacid<Enter>L");
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(generators(&app, 0)[0].shape.kind, FillKind::Acid);
    let first = steps(&app);
    let depth = app.history.undo_depth();
    keys(&mut app, "gl");
    assert_eq!(app.mode, Mode::Shape);
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert_eq!(panel.shape.kind, FillKind::Acid);
    // It counts the rows as they'd be without it, so all 16 are open to it.
    assert_eq!(panel.open_rows(), 16);
    let notes = first.iter().filter(|s| !s.starts_with("---")).count();
    let part = panel.parts[0];
    let written = vt_model::shape::written(
        panel.base_steps(&part),
        part.range,
        part.column,
        &panel.shape_of(0),
    );
    assert_eq!(written.len(), notes);
    keys(&mut app, "rL");
    assert_ne!(steps(&app), first);
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(steps(&app), first);
}

#[test]
fn acid_on_notes_keeps_their_rhythm_and_with_keep_their_pitches() {
    use vt_tui::shaping::ShapeRow;
    let start = [
        "C-3 40 --- --- ---",
        E,
        "D#3 40 --- --- ---",
        "OFF -- --- --- ---",
        "G-3 40 --- --- ---",
        E,
        "A#3 40 --- --- ---",
        E,
    ];
    let (mut app, _rt) = app_with(&start);
    keys(&mut app, "vGSKacid<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Reshape");
    assert!(
        !panel.rows().contains(&ShapeRow::Density),
        "the rhythm is there"
    );
    let rhythm =
        |app: &App| -> Vec<bool> { steps(app).iter().map(|s| s.starts_with("---")).collect() };
    assert_eq!(
        rhythm(&app),
        [false, true, false, false, false, true, false, true]
    );
    assert_eq!(notes_of(&app)[3], "OFF");
    // Keep the pitches: the ends and notes go, and with no octaves every note is as it was.
    keys(&mut app, "jjjjjjj");
    assert_eq!(app.shape.as_ref().unwrap().row, ShapeRow::Pitches);
    keys(&mut app, "=");
    assert_eq!(
        app.shape.as_ref().unwrap().rows(),
        [
            ShapeRow::Fill,
            ShapeRow::Octave,
            ShapeRow::Accent,
            ShapeRow::Slide,
            ShapeRow::Pitches,
            ShapeRow::Where,
        ]
    );
    keys(&mut app, "kkki0<Enter>");
    assert_eq!(app.shape.as_ref().unwrap().shape.acid.octave, 0);
    let notes: Vec<String> = start.iter().map(|s| s[..3].to_owned()).collect();
    for _ in 0..4 {
        keys(&mut app, "r");
        assert_eq!(notes_of(&app), notes);
    }
}

#[test]
fn dot_applies_the_same_shape_with_ends_from_what_is_there() {
    let (mut app, _rt) = app_with(&[
        "C-3 00 --- --- ---",
        "C-3 00 --- --- ---",
        "C-3 40 --- --- ---",
        "C-3 10 --- --- ---",
        "C-3 00 --- --- ---",
        "C-3 30 --- --- ---",
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
    let (mut app, _rt) = app_with(&["--- -- vol:10 --- ---", "--- -- vol:20 --- ---"]);
    keys(&mut app, "lllllvGS");
    keys(&mut app, "jjiFF");
    assert_eq!(app.shape.as_ref().unwrap().shape.to, 0xF8);
    keys(&mut app, "<Enter>");
    assert_eq!(steps(&app)[1], "--- -- vol:F8 --- ---");
}

const MARKED: &str = "C-3 40 cut:20h --- ---";
const GLIDING: &str = "C-3 40 cut:20g --- ---";

#[test]
fn tilde_cycles_a_values_mark_from_either_of_its_fields() {
    check(&[A], "lllll~", &[MARKED]);
    check(&[A], "llllll~", &[MARKED]);
    check(&[A], "lllll~~", &[GLIDING]);
    check(&[A], "lllll~~~", &[A]);
}

#[test]
fn tilde_is_one_undo_step_and_dot_repeats_it() {
    check(&[A], "lllll~u", &[A]);
    check(&[A, A], "lllll~j.", &[MARKED, MARKED]);
}

#[test]
fn tilde_refuses_anything_but_a_value_and_says_why() {
    for (start, typed) in [
        ("C-3 40 del:10 --- ---", "llll~"),
        ("C-3 40 --- --- ---", "llll~"),
        ("C-3 40 cut:20 --- ---", "~"),
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
        "C-3 40 cut:20 --- ---",
        "C-3 40 res:30g --- ---",
        "C-3 40 del:10 --- ---",
        "C-3 40 pan:40h --- ---",
    ];
    check(
        &start,
        "llV3j~",
        &[
            "C-3 40 cut:20h --- ---",
            "C-3 40 res:30h --- ---",
            "C-3 40 del:10 --- ---",
            "C-3 40 pan:40h --- ---",
        ],
    );
}

#[test]
fn visual_tilde_stays_in_the_selected_column_and_repeats() {
    let start = ["C-3 40 cut:20 res:30 ---", "C-3 40 cut:20 res:30 ---"];
    let one = "C-3 40 cut:20h res:30 ---";
    check(&start, "lllllvj~", &[one, one]);
    let (mut app, _rt) = app_with(&[start[0], start[1], start[0]]);
    keys(&mut app, "lllllvj~jj.");
    assert_eq!(steps(&app), [one, one, "C-3 40 cut:20h res:30 ---"]);
}

#[test]
fn a_typed_value_copies_the_mark_above_it_without_wrapping() {
    let empty = "--- -- --- --- ---";
    let above = "C-3 40 cut:20h --- ---";
    let typed = "--- -- cut:40h --- ---";
    let plain = "--- -- cut:40 --- ---";
    check(&[above, empty], "jllllliCUT<Enter>40<Esc>", &[above, typed]);
    check(&[empty, above], "llllliCUT<Enter>40<Esc>", &[plain, above]);
    let event_between = "C-3 40 del:10 --- ---";
    check(
        &[above, event_between, empty],
        "2jllllliCUT<Enter>40<Esc>",
        &[above, event_between, typed],
    );
}

#[test]
fn a_value_typed_over_another_keeps_its_mark() {
    let start = "--- -- cut:20 --- ---";
    let above = "C-3 40 cut:20g --- ---";
    check(
        &[above, "--- -- cut:20h --- ---"],
        "jllllllr55",
        &[above, "--- -- cut:55h --- ---"],
    );
    check(&[start], "lllllrRES<Enter>40", &["--- -- res:40 --- ---"]);
}

#[test]
fn changing_a_value_to_an_event_drops_its_mark() {
    check(&[MARKED], "lllllrDEL<Enter>10", &["C-3 40 del:10 --- ---"]);
}

#[test]
fn moving_a_slot_carries_its_mark_and_clearing_removes_it() {
    let empty = "--- -- --- --- ---";
    check(&[MARKED, empty], "yyjp", &[MARKED, MARKED]);
    check(&[MARKED, empty], "]r", &[empty, MARKED]);
    let cleared = "C-3 40 --- --- ---";
    check(&[MARKED], "lllllx", &[cleared]);
    check(&[MARKED], "lllll=", &["C-3 40 cut:21h --- ---"]);
}

#[test]
fn w_stops_at_the_last_slot_and_a_row_end_operator_covers_it() {
    let (mut app, _rt) = app_with(&[FULL]);
    keys(&mut app, "wwwwwwww");
    assert_eq!(app.cursor.field, Field::FxCommand(2));
    check(&[FULL], "d$", &["--- -- --- --- ---"]);
}

/// An app with these steps on each of several tracks, each with its own phrase.
fn tracks_with(rows: &[&[&str]]) -> (App, Realtime) {
    let mut song = Song::new(rows.len());
    for (id, steps) in rows.iter().enumerate() {
        let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
        song.tracks[id]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

fn track_steps(app: &App, track: usize) -> Vec<String> {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

const N: &str = "C-3 40 cut:20 --- ---";
const GONE: &str = "--- -- --- --- ---";

#[test]
fn a_selection_reaches_other_tracks_and_clears_them_as_one_undo() {
    let (mut app, _rt) = tracks_with(&[&[N, N, N], &[N, N, N], &[N, N, N]]);
    let depth = app.history.undo_depth();
    keys(&mut app, "VjWd");
    assert_eq!(track_steps(&app, 0), [GONE, GONE, N]);
    assert_eq!(track_steps(&app, 1), [GONE, GONE, N]);
    assert_eq!(track_steps(&app, 2), [N, N, N], "the third wasn't reached");
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert_eq!(app.cursor.track, 0, "it lands on the first track");
    keys(&mut app, "u");
    assert_eq!(track_steps(&app, 1), [N, N, N]);
}

#[test]
fn b_reaches_backwards_and_operators_change_each_track() {
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "WlvB=");
    assert_eq!(
        app.cursor.track, 1,
        "the selection stays, and the cursor in it"
    );
    assert_eq!(track_steps(&app, 0), ["C-3 41 cut:20 --- ---"]);
    assert_eq!(track_steps(&app, 1), ["C-3 41 cut:20 --- ---"]);
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "VW>");
    assert_eq!(track_steps(&app, 1), ["C#3 40 cut:20 --- ---"]);
    assert_eq!(track_steps(&app, 0), ["C#3 40 cut:20 --- ---"]);
}

#[test]
fn replace_and_marks_work_across_tracks() {
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "lllllvW~");
    assert_eq!(track_steps(&app, 0), ["C-3 40 cut:20h --- ---"]);
    assert_eq!(track_steps(&app, 1), ["C-3 40 cut:20h --- ---"]);
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "lvWr11");
    assert_eq!(track_steps(&app, 0), ["C-3 11 cut:20 --- ---"]);
    assert_eq!(track_steps(&app, 1), ["C-3 11 cut:20 --- ---"]);
}

#[test]
fn a_shorter_phrase_keeps_what_it_has() {
    let (mut app, _rt) = tracks_with(&[&[N, N, N], &[N]]);
    keys(&mut app, "VjjW>");
    assert_eq!(track_steps(&app, 0)[2], "C#3 40 cut:20 --- ---");
    assert_eq!(track_steps(&app, 1), ["C#3 40 cut:20 --- ---"]);
}

#[test]
fn rows_count_from_each_phrases_own_loop_start() {
    let (mut app, _rt) = tracks_with(&[&[N, N, N], &[N, N, N]]);
    let mut slid = Song::clone(app.song());
    Arc::make_mut(slid.tracks[1].phrases.get_mut(&0).unwrap()).set_len(2);
    Arc::make_mut(slid.tracks[1].phrases.get_mut(&0).unwrap()).slide(1);
    let samples = Arc::clone(&app.samples);
    let version = Arc::new(Published::new(Arc::new(slid.clone()), Arc::clone(&samples)));
    let (handle, _rt2) = connect(Engine::new(48_000, version));
    app = App::new(slid, None, samples, handle, 48_000);
    keys(&mut app, "VW>");
    assert_eq!(track_steps(&app, 0)[0], "C#3 40 cut:20 --- ---");
    assert_eq!(
        track_steps(&app, 1),
        [N, "C#3 40 cut:20 --- ---", N],
        "row 00 of a slid loop is its second step"
    );
}

#[test]
fn yank_and_paste_keep_each_tracks_part() {
    let a = "C-3 40 --- --- ---";
    let b = "D-3 50 --- --- ---";
    let (mut app, _rt) = tracks_with(&[&[a, GONE], &[b, GONE], &[GONE, GONE]]);
    keys(&mut app, "VWy");
    assert_eq!(app.cursor.track, 0);
    keys(&mut app, "jp");
    assert_eq!(track_steps(&app, 0), [a, a]);
    assert_eq!(track_steps(&app, 1), [b, b]);
    assert_eq!(track_steps(&app, 2), [GONE, GONE]);
    keys(&mut app, "u");
    assert_eq!(track_steps(&app, 1), [b, GONE], "one undo");
    // Past the last track, the rest is dropped.
    keys(&mut app, "WWkp");
    assert_eq!(track_steps(&app, 2), [a, GONE]);
}

#[test]
fn dot_repeats_across_as_many_tracks_and_gv_comes_back() {
    let (mut app, _rt) = tracks_with(&[&[N, N], &[N, N], &[N, N]]);
    keys(&mut app, "VW><Esc>j.");
    let up = "C#3 40 cut:20 --- ---";
    assert_eq!(track_steps(&app, 0), [up, up]);
    assert_eq!(track_steps(&app, 1), [up, up]);
    assert_eq!(track_steps(&app, 2), [N, N]);
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "VW<Esc>");
    assert_eq!(app.cursor.track, 0, "Esc stays where it is");
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "VW><Esc>gv");
    assert!(app.mode.is_visual());
    assert_eq!(app.current_selection().tracks.count(), 2);
}

#[test]
fn shape_across_tracks_gives_each_the_same_settings_and_a_seed_of_its_own() {
    let rows = [N; 8];
    let (mut app, _rt) = tracks_with(&[&rows, &rows, &[GONE; 8]]);
    let velocities = |app: &App, track: usize| -> Vec<String> {
        track_steps(app, track)
            .iter()
            .map(|s| s[4..6].to_owned())
            .collect()
    };
    let depth = app.history.undo_depth();
    // A ramp from 00 to 70 on both tracks; the third has no notes, so it's passed over.
    keys(&mut app, "lvGWWSji00ji70");
    assert_eq!(app.mode, Mode::Shape);
    assert_eq!(app.shape.as_ref().unwrap().parts.len(), 2);
    let ramp = ["00", "10", "20", "30", "40", "50", "60", "70"];
    assert_eq!(velocities(&app, 0), ramp);
    assert_eq!(velocities(&app, 1), ramp);
    assert_eq!(track_steps(&app, 2), [GONE; 8]);
    // A random fill picks for each track on its own.
    keys(&mut app, "kkKrandom<Enter><Enter>");
    assert_ne!(velocities(&app, 0), velocities(&app, 1));
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo for both");
    keys(&mut app, "u");
    assert_eq!(track_steps(&app, 0), rows);
    assert_eq!(track_steps(&app, 1), rows);
    // `.` shapes both again.
    keys(&mut app, ".");
    assert_ne!(track_steps(&app, 0), rows);
    assert_ne!(track_steps(&app, 1), rows);
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn shape_across_tracks_takes_each_tracks_own_fx_command_and_refuses_when_none_has_one() {
    let (mut app, _rt) = tracks_with(&[
        &["C-3 40 cut:00 --- ---", "C-3 40 cut:40 --- ---"],
        &["C-3 40 res:00 --- ---", "C-3 40 res:40 --- ---"],
    ]);
    keys(&mut app, "lllllvjWS");
    assert_eq!(app.mode, Mode::Shape);
    keys(&mut app, "<Enter>");
    assert_eq!(track_steps(&app, 0)[0], "C-3 40 cut:00 --- ---");
    assert!(
        track_steps(&app, 1)[1].contains("res:40"),
        "{:?}",
        track_steps(&app, 1)
    );
    let (mut app, _rt) = tracks_with(&[&[GONE, GONE], &[GONE, GONE]]);
    keys(&mut app, "lvjWS");
    assert_ne!(app.mode, Mode::Shape);
    assert!(app.status.contains("no notes"), "{}", app.status);
}

#[test]
fn a_gate_is_typed_as_its_name_and_pulses() {
    check(&[E, E], "llliHLD3<Esc>", &["--- -- HLD3 --- --- ---", E]);
    check(&[E], "llli4<Esc>", &["--- -- SGL4 --- --- ---"]);
    let held = "C-3 40 HLD3 cut:20 --- ---";
    check(&[held], "llli6<Esc>", &["C-3 40 HLD6 cut:20 --- ---"]);
    check(&[held], "lllrMLT2", &["C-3 40 MLT2 cut:20 --- ---"]);
    check(&[held], "lllx", &["C-3 40 cut:20 --- ---"]);
}

#[test]
fn a_bad_gate_is_refused() {
    let (mut app, _rt) = app_with(&[E]);
    keys(&mut app, "llliXYZ");
    assert!(app.status.contains("isn't a gate"), "{}", app.status);
    keys(&mut app, "9<Esc>");
    assert_eq!(steps(&app), [E]);
    assert!(app.status.contains("0 to 8"), "{}", app.status);
}

#[test]
fn equals_and_minus_change_a_gates_pulses() {
    let held = "C-3 40 HLD3 --- --- ---";
    check(&[held], "lll=", &["C-3 40 HLD4 --- --- ---"]);
    check(&[held], "lll9=", &["C-3 40 HLD8 --- --- ---"]);
    check(&[held], "lll9-", &["C-3 40 HLD0 --- --- ---"]);
    check(&[E], "lll=", &["--- -- SGL2 --- --- ---"]);
}

#[test]
fn the_gate_column_is_selected_filled_and_pasted_as_a_column() {
    let held = "C-3 40 HLD3 cut:20 --- ---";
    let plain = "C-3 40 cut:20 --- ---";
    check(
        &[plain, plain],
        "lllvjrRST2",
        &["C-3 40 RST2 cut:20 --- ---", "C-3 40 RST2 cut:20 --- ---"],
    );
    check(&[held, plain, plain], "lllvyjjp", &[held, plain, held]);
    // The gate stays in its column: it doesn't land in an FX slot.
    check(&[held, plain], "lllvyjllp", &[held, plain]);
}

#[test]
fn a_gates_length_is_typed_replaced_and_cleared_on_its_own() {
    let held = "C-3 40 HLD3 cut:20 --- ---";
    check(&[held], "lllli40<Esc>", &["C-3 40 HLD3 40 cut:20 --- ---"]);
    // An empty cell gets one pulse of `SGL` to hold it, which is how it played.
    check(&[A], "lllli40<Esc>", &["C-3 40 SGL1 40 cut:20 --- ---"]);
    check(&[held], "llllr80", &["C-3 40 HLD3 80 cut:20 --- ---"]);
    let short = "C-3 40 SGL2 40 cut:20 --- ---";
    check(&[short], "llllx", &["C-3 40 SGL2 cut:20 --- ---"]);
    check(&[short], "lllli<BS><Esc>", &["C-3 40 SGL2 cut:20 --- ---"]);
    check(&[short], "llllsC0<Esc>", &["C-3 40 SGL2 C0 cut:20 --- ---"]);
    // The gate's own part keeps the length, and `x` there empties the whole cell.
    check(
        &[short],
        "llliMLT3<Esc>",
        &["C-3 40 MLT3 40 cut:20 --- ---"],
    );
    check(&[short], "lllrHLD2", &["C-3 40 HLD2 40 cut:20 --- ---"]);
    check(&[short], "lllx", &[A]);
    // `.` puts the length on another row's cell, keeping that row's gate.
    check(
        &[short, held],
        "llllr80j.",
        &[
            "C-3 40 SGL2 80 cut:20 --- ---",
            "C-3 40 HLD3 80 cut:20 --- ---",
        ],
    );
}

#[test]
fn equals_and_minus_move_a_gates_length_and_an_empty_one_starts_from_the_tracks_gate() {
    let short = "C-3 40 SGL2 40 cut:20 --- ---";
    check(&[short], "llll=", &["C-3 40 SGL2 41 cut:20 --- ---"]);
    check(&[short], "llll+", &["C-3 40 SGL2 50 cut:20 --- ---"]);
    check(&[short], "llll3-", &["C-3 40 SGL2 3D cut:20 --- ---"]);
    // A sample track's `Gate` is 100%.
    check(&[A], "llll-", &["C-3 40 SGL1 FE cut:20 --- ---"]);
    check(&[E], "llll-", &[E]);
    // On the gate's own part, they still move the pulses.
    check(&[short], "lll=", &["C-3 40 SGL3 40 cut:20 --- ---"]);
}

#[test]
fn a_gates_length_is_filled_on_notes_and_copied_with_its_cell() {
    let short = "C-3 40 SGL2 40 cut:20 --- ---";
    let filled = "C-3 40 SGL1 60 cut:20 --- ---";
    check(&[A, E, A], "llllvjjr60", &[filled, E, filled]);
    check(&[short, A], "llllvyjp", &[short, short]);
    check(&[short, A], "lllvyjp", &[short, short]);
    // `K` on the gate chooses the gate, which keeps the length.
    let (mut app, _rt) = app_with(&[short]);
    keys(&mut app, "lllK");
    assert_eq!(app.mode, Mode::Picker);
    keys(&mut app, "/hld<Enter>");
    assert_eq!(steps(&app), ["C-3 40 HLD2 40 cut:20 --- ---"]);
    // And on the length, a length, which keeps the gate.
    keys(&mut app, "lK/C0<Enter>");
    assert_eq!(steps(&app), ["C-3 40 HLD2 C0 cut:20 --- ---"]);
}

#[test]
fn the_fine_digits_nudge_and_carry_and_the_coarse_ones_keep_them() {
    check(
        &["--- -- cut:40FF --- ---"],
        "lllllll=",
        &["--- -- cut:41 --- ---"],
    );
    check(
        &["--- -- cut:4100 --- ---"],
        "lllllll-",
        &["--- -- cut:40FF --- ---"],
    );
    check(
        &["--- -- cut:4012 --- ---"],
        "lllllll+",
        &["--- -- cut:4022 --- ---"],
    );
    check(
        &["--- -- cut:4012 --- ---"],
        "llllll=",
        &["--- -- cut:4112 --- ---"],
    );
    check(
        &["--- -- cut:FE80 --- ---"],
        "llllll=",
        &["--- -- cut:FF --- ---"],
    );
}

#[test]
fn typing_coarse_digits_keeps_the_fine_part_up_to_the_top() {
    check(
        &["--- -- cut:3F80 --- ---"],
        "lllllli40<Esc>",
        &["--- -- cut:4080 --- ---"],
    );
    check(
        &["--- -- cut:3F80 --- ---"],
        "llllllrFF",
        &["--- -- cut:FF --- ---"],
    );
    // Another setting's value starts afresh.
    check(
        &["--- -- cut:3F80 --- ---"],
        "lllllirES<Enter>40<Esc>",
        &["--- -- res:40 --- ---"],
    );
}

#[test]
fn the_fine_digits_are_typed_replaced_and_cleared_on_their_own() {
    check(
        &["--- -- cut:40 --- ---", "--- -- cut:50 --- ---"],
        "lllllllia0<Down>b0<Esc>",
        &["--- -- cut:40A0 --- ---", "--- -- cut:50B0 --- ---"],
    );
    check(
        &["--- -- cut:40 --- ---"],
        "lllllllr12",
        &["--- -- cut:4012 --- ---"],
    );
    check(
        &["--- -- cut:4080 --- ---"],
        "lllllllx",
        &["--- -- cut:40 --- ---"],
    );
    check(
        &["--- -- cut:4080 --- ---"],
        "lllllllsa0<Esc>",
        &["--- -- cut:40A0 --- ---"],
    );
    // A new value below starts with none, as its mark is copied and its fine part isn't.
    check(
        &["--- -- cut:4080h --- ---", E],
        "jlllllicut<Enter>50<Esc>",
        &["--- -- cut:4080h --- ---", "--- -- cut:50h --- ---"],
    );
}

#[test]
fn a_list_or_an_event_has_no_fine_digits_and_a_list_doesnt_glide() {
    let (mut app, _rt) = app_with(&["--- -- 1.distort.bits:03 del:10 cut:40"]);
    keys(&mut app, "lllllll");
    assert_eq!(app.cursor.field, Field::FxCommand(1), "past the list's");
    keys(&mut app, "ll");
    assert_eq!(app.cursor.field, Field::FxCommand(2), "and the event's");
    keys(&mut app, "hh");
    assert_eq!(app.cursor.field, Field::FxCommand(1));
    check(
        &["--- -- 1.distort.bits:03 --- ---"],
        "llllll~",
        &["--- -- 1.distort.bits:03h --- ---"],
    );
    check(
        &["--- -- 1.distort.bits:03 --- ---"],
        "llllll~~",
        &["--- -- 1.distort.bits:03 --- ---"],
    );
}

#[test]
fn equals_and_minus_nudge_a_row_by_24ths_and_plus_and_underscore_by_sixths() {
    check(&[A], "ll=", &["C-3 40 +1 cut:20 --- ---"]);
    check(&[A], "ll3-", &["C-3 40 -3 cut:20 --- ---"]);
    check(&[A], "ll+", &["C-3 40 +4 cut:20 --- ---"]);
    check(&[A], "ll_", &["C-3 40 -4 cut:20 --- ---"]);
    // Just under half a step is as far as it goes.
    check(&[A], "ll+++", &["C-3 40 +B cut:20 --- ---"]);
    check(&[A], "ll=-", &[A]);
    // A row without a note gets none, but one it has moves.
    check(&[E], "ll=", &[E]);
    check(
        &["--- -- +2 --- --- ---"],
        "ll=",
        &["--- -- +3 --- --- ---"],
    );
    // On the velocity, they move the velocity and keep the nudge.
    let nudged = "C-3 40 +2 cut:20 --- ---";
    check(&[nudged], "l=", &["C-3 41 +2 cut:20 --- ---"]);
}

#[test]
fn a_nudge_is_typed_as_a_sign_and_a_digit_and_cleared_on_its_own() {
    check(&[A], "lli3<Esc>", &["C-3 40 +3 cut:20 --- ---"]);
    check(&[A], "lli-5<Esc>", &["C-3 40 -5 cut:20 --- ---"]);
    check(&[A], "lli-+B<Esc>", &["C-3 40 +B cut:20 --- ---"]);
    let nudged = "C-3 40 -2 cut:20 --- ---";
    check(&[nudged], "lli0<Esc>", &[A]);
    check(&[nudged], "lli<BS><Esc>", &[A]);
    check(&[nudged], "llx", &[A]);
    // Past `B` is past half a step, so nothing's written.
    let (mut app, _rt) = app_with(&[A]);
    keys(&mut app, "lliC");
    assert_eq!(steps(&app), [A]);
    assert!(app.status.contains("up to B"), "{}", app.status);
    // `x` on the velocity leaves the nudge, as each is its own.
    check(&[nudged], "lx", &["C-3 -- -2 cut:20 --- ---"]);
    // `r` takes one, and `.` puts it on another row.
    check(
        &[A, A],
        "llr-2j.",
        &["C-3 40 -2 cut:20 --- ---", "C-3 40 -2 cut:20 --- ---"],
    );
    // Auto-jump moves on once the digit's in.
    let (mut app, _rt) = app_with(&[A, A]);
    assert!(!app.auto_jump);
    keys(&mut app, "lli;4");
    assert_eq!(app.cursor.step, 1);
    assert_eq!(app.cursor.field, Field::Nudge);
}

#[test]
fn a_nudge_is_filled_on_notes_and_copied_and_cleared_with_the_velocity() {
    let filled = "C-3 40 -3 cut:20 --- ---";
    check(&[A, E, A], "llvjjr-3", &[filled, E, filled]);
    // It's in the velocity's column group, so it goes with it.
    let nudged = "C-3 50 +6 cut:20 --- ---";
    check(&[nudged, A], "lvyjp", &[nudged, nudged]);
    check(&[nudged, A], "llvyjp", &[nudged, nudged]);
    check(&[nudged, A], "lvjd", &["C-3 -- cut:20 --- ---"; 2]);
    // `=` on a selection moves only the nudges.
    check(
        &[nudged, A, E],
        "llvjj=",
        &["C-3 50 +7 cut:20 --- ---", "C-3 40 +1 cut:20 --- ---", E],
    );
}

/// A key with Cmd, pressed and let go as the terminal sends it.
fn with_cmd(app: &mut App, name: &str) {
    use ratatui::crossterm::event::KeyEventKind;
    let key = vt_tui::input::keys::parse_sequence(name).unwrap()[0];
    let now = Instant::now();
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Release, now);
}

#[test]
fn a_selection_stays_through_repeated_adjusts_across_two_tracks() {
    let (mut app, _rt) = tracks_with(&[&[N, N], &[N, N], &[N, N]]);
    let depth = app.history.undo_depth();
    keys(&mut app, "vjW");
    for key in ["+", "+", "-"] {
        keys(&mut app, key);
        assert_eq!(app.mode, Mode::VisualField, "still selected after {key}");
    }
    let up = "B-4 40 cut:20 --- ---";
    assert_eq!(track_steps(&app, 0), [up, up]);
    assert_eq!(track_steps(&app, 1), [up, up]);
    assert_eq!(track_steps(&app, 2), [N, N], "the third wasn't reached");
    assert_eq!(
        app.history.undo_depth(),
        depth + 3,
        "each press is its own undo step"
    );
    assert_eq!(app.status, "+23 semitones");
    // `>` and `<`, `C-a` and `C-x` keep it too, and count.
    keys(&mut app, "<<^a");
    assert_eq!(app.mode, Mode::VisualField);
    assert_eq!(app.status, "+22 semitones");
}

#[test]
fn the_running_total_counts_each_columns_own_steps() {
    let (mut app, _rt) = app_with(&[N, N]);
    keys(&mut app, "lvj--");
    assert_eq!(app.status, "-2 steps");
    keys(&mut app, "+");
    assert_eq!(app.status, "+1 large step, -2 steps");
    assert_eq!(steps(&app), ["C-3 4E cut:20 --- ---"; 2]);
    // It stays in sight as the selection moves, and goes with it.
    keys(&mut app, "k");
    assert_eq!(
        app.adjusted_total().as_deref(),
        Some("+1 large step, -2 steps")
    );
    keys(&mut app, "<Esc>");
    assert_eq!(app.adjusted_total(), None);
    assert!(app.status.is_empty(), "{}", app.status);
    // A new selection starts from nothing, and presses that come back to where they were say so.
    keys(&mut app, "v=-");
    assert_eq!(app.status, "0 steps");
    // On notes, with the scale on, `=` and `-` move through it in degrees, `+` in semitones.
    let (mut app, _rt) = app_with(&[N]);
    app.scale.on = true;
    keys(&mut app, "v==+");
    assert_eq!(app.status, "+12 semitones, +2 degrees");
}

#[test]
fn undo_while_selected_takes_back_one_press_and_keeps_the_selection() {
    let (mut app, _rt) = tracks_with(&[&[N], &[N]]);
    keys(&mut app, "vW++");
    let octave = "C-4 40 cut:20 --- ---";
    keys(&mut app, "u");
    assert_eq!(track_steps(&app, 0), [octave]);
    assert_eq!(track_steps(&app, 1), [octave]);
    assert_eq!(app.mode, Mode::VisualField, "still selected");
    assert_eq!(app.current_selection().tracks.count(), 2);
    assert_eq!(app.status, "+12 semitones");
    keys(&mut app, "u");
    assert_eq!(track_steps(&app, 1), [N]);
    assert_eq!(app.adjusted_total(), None, "nothing moved now");
    keys(&mut app, "^r");
    assert_eq!(track_steps(&app, 1), [octave]);
    assert_eq!(app.status, "+12 semitones");
    // Another press goes on from there, on the same selection.
    keys(&mut app, "-");
    assert_eq!(track_steps(&app, 0), ["B-3 40 cut:20 --- ---"]);
    assert_eq!(track_steps(&app, 1), ["B-3 40 cut:20 --- ---"]);
    assert_eq!(app.status, "+11 semitones");
}

#[test]
fn undo_past_the_selections_own_changes_keeps_it_and_redo_comes_back_in_step() {
    let (mut app, _rt) = app_with(&[N]);
    keys(&mut app, "ra");
    keys(&mut app, "v+u");
    assert_eq!(app.adjusted_total(), None);
    keys(&mut app, "u");
    assert_eq!(steps(&app), [N], "the change from before the selection");
    assert_eq!(app.mode, Mode::VisualField);
    keys(&mut app, "^r");
    assert_eq!(
        app.adjusted_total(),
        None,
        "that change wasn't the selection's"
    );
    keys(&mut app, "^r");
    assert_eq!(app.status, "+12 semitones");
}

#[test]
fn d_x_and_y_end_a_kept_selection_and_gv_brings_it_back() {
    let (mut app, _rt) = app_with(&[N, N, N]);
    keys(&mut app, "vj+d");
    assert_eq!(app.mode, Mode::Normal, "d ends it");
    let cleared = "--- 40 cut:20 --- ---";
    assert_eq!(steps(&app), [cleared, cleared, N], "the notes it covered");
    keys(&mut app, "gv");
    assert_eq!(app.mode, Mode::VisualField);
    assert_eq!(app.current_selection().range.len(), 2);
    assert_eq!(app.adjusted_total(), None, "gv starts a new total");
    keys(&mut app, "y");
    assert_eq!(app.mode, Mode::Normal, "y ends it");
    keys(&mut app, "Vx");
    assert_eq!(app.mode, Mode::Normal, "x ends it");
}

#[test]
fn dot_on_a_selection_repeats_a_press_and_keeps_it() {
    let (mut app, _rt) = app_with(&[N, N]);
    keys(&mut app, "vj+<Esc>");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "gv.");
    assert_eq!(app.mode, Mode::VisualField);
    assert_eq!(steps(&app), ["C-5 40 cut:20 --- ---"; 2]);
    assert_eq!(app.status, "+12 semitones");
}

#[test]
fn cmd_with_the_movement_keys_keeps_the_selection_as_the_keys_they_stand_for() {
    let (mut app, _rt) = app_with(&[N, N]);
    keys(&mut app, "vj");
    with_cmd(&mut app, "D-k");
    with_cmd(&mut app, "D-k");
    with_cmd(&mut app, "D-h");
    assert_eq!(app.mode, Mode::VisualField);
    assert_eq!(steps(&app), ["B-4 40 cut:20 --- ---"; 2], "as + + -");
    assert_eq!(app.status, "+23 semitones");
    with_cmd(&mut app, "D-l");
    with_cmd(&mut app, "D-j");
    assert_eq!(steps(&app), ["C-4 40 cut:20 --- ---"; 2], "as = and _");
    assert_eq!(app.status, "+12 semitones");
}

#[test]
fn cmd_with_the_movement_keys_changes_the_shape_panels_values_as_its_keys_do() {
    let (mut app, _rt) = app_of(&velocities("10", "7F", "40"));
    keys(&mut app, "lvGSj");
    let shape = |app: &App| app.shape.as_ref().unwrap().shape;
    let start = shape(&app);
    let row = app.shape.as_ref().unwrap().row;
    for (plain, cmd) in [("=", "D-l"), ("-", "D-h"), ("+", "D-k"), ("_", "D-j")] {
        keys(&mut app, plain);
        let by_plain = shape(&app);
        assert_ne!(by_plain, start, "{plain} changes the start");
        app.shape.as_mut().unwrap().shape = start;
        with_cmd(&mut app, cmd);
        assert_eq!(shape(&app), by_plain, "{cmd} as {plain}");
        assert_eq!(app.shape.as_ref().unwrap().row, row, "without moving");
        app.shape.as_mut().unwrap().shape = start;
    }
    // While a value's typed, Cmd keys type nothing.
    keys(&mut app, "i");
    with_cmd(&mut app, "D-l");
    assert_eq!(app.shape.as_ref().unwrap().typing.as_deref(), Some(""));
}

/// The live generators on a track's phrase 00.
fn generators(app: &App, track: usize) -> Vec<vt_model::generator::Generator> {
    app.song().tracks[track].phrases[&0].generators().to_vec()
}

/// A tresillo of `7F` among 8 notes at `40`, kept live with `L`.
fn live_tresillo() -> (App, Realtime) {
    let (mut app, rt) = app_with(&["C-3 40 --- --- ---"; 8]);
    keys(&mut app, "lvGSKalt<Enter>ji7Fji7FjjKeuc<Enter>");
    assert_eq!(vel(&app), ["7F", "40", "40", "7F", "40", "40", "7F", "40"]);
    keys(&mut app, "L");
    (app, rt)
}

#[test]
fn l_keeps_a_shape_live_as_one_undo_and_gl_turns_its_rotate() {
    use vt_model::shape::Where;
    let (mut app, _rt) = live_tresillo();
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(generators(&app, 0).len(), 1);
    assert!(app.status.contains("live"), "{}", app.status);
    let depth = app.history.undo_depth();
    // `gl` from another row of the column opens the panel on its saved shape.
    keys(&mut app, "jjjgl");
    assert_eq!(app.mode, Mode::Shape);
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.live, Some(0));
    assert_eq!(panel.shape.place, Where::Euclid { hits: 3, rotate: 0 });
    // Down to Rotate, and round by one, heard as it's turned.
    keys(&mut app, "jjjjjj=");
    assert_eq!(vel(&app), ["40", "7F", "40", "40", "7F", "40", "40", "7F"]);
    assert_eq!(app.history.undo_depth(), depth);
    keys(&mut app, "L");
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(
        generators(&app, 0)[0].shape.place,
        Where::Euclid { hits: 3, rotate: 1 }
    );
    // `Esc` leaves it as it was.
    keys(&mut app, "gljjjjjj=<Esc>");
    assert_eq!(vel(&app), ["40", "7F", "40", "40", "7F", "40", "40", "7F"]);
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(vel(&app), ["7F", "40", "40", "7F", "40", "40", "7F", "40"]);
    assert_eq!(generators(&app, 0).len(), 1);
    // `Enter` there applies it with the new settings.
    keys(&mut app, "gljjjjjj==<Enter>");
    assert!(generators(&app, 0).is_empty());
    assert_eq!(vel(&app), ["7F", "40", "7F", "40", "40", "7F", "40", "40"]);
    assert!(app.status.contains("applied"), "{}", app.status);
    keys(&mut app, "gl");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("No live column"), "{}", app.status);
}

#[test]
fn a_hand_edit_to_a_live_column_applies_it_says_so_and_u_brings_it_back() {
    let (mut app, _rt) = live_tresillo();
    keys(&mut app, "jr10");
    assert!(generators(&app, 0).is_empty());
    assert_eq!(vel(&app), ["7F", "10", "40", "7F", "40", "40", "7F", "40"]);
    assert!(app.status.contains("plain values"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(vel(&app), ["7F", "40", "40", "7F", "40", "40", "7F", "40"]);
    // A note's pitch is only read, so it leaves it live.
    keys(&mut app, "h=");
    assert_eq!(generators(&app, 0).len(), 1);
}

#[test]
fn a_note_added_under_a_live_velocity_column_gets_a_generated_velocity() {
    let (mut app, _rt) = app_with(&["C-3 40 --- --- ---", E, "C-3 40 --- --- ---", E]);
    keys(&mut app, "lvGSji00ji40L");
    assert_eq!(vel(&app), ["00", "--", "40", "--"]);
    let depth = app.history.undo_depth();
    keys(&mut app, "hjia<Esc>");
    assert_eq!(generators(&app, 0).len(), 1);
    assert_eq!(vel(&app), ["00", "20", "40", "--"]);
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "the note and its velocity"
    );
    keys(&mut app, "u");
    assert_eq!(vel(&app), ["00", "--", "40", "--"]);
    assert_eq!(generators(&app, 0).len(), 1);
}

#[test]
fn space_g_a_and_gen_apply_apply_the_live_column_here() {
    let (mut app, _rt) = live_tresillo();
    let depth = app.history.undo_depth();
    keys(&mut app, " ga");
    assert!(generators(&app, 0).is_empty());
    assert_eq!(vel(&app), ["7F", "40", "40", "7F", "40", "40", "7F", "40"]);
    assert_eq!(app.history.undo_depth(), depth + 1);
    keys(&mut app, "u:gen apply<Enter>");
    assert!(generators(&app, 0).is_empty());
    keys(&mut app, ":gen<Enter>");
    assert!(app.status.contains("apply"), "{}", app.status);
    // On another column there's nothing to apply.
    keys(&mut app, "uh ga");
    assert_eq!(generators(&app, 0).len(), 1);
    assert!(app.status.contains("No live column"), "{}", app.status);
}

#[test]
fn across_tracks_l_keeps_one_generator_per_track_each_with_its_own_seed() {
    let rows = [N; 8];
    let (mut app, _rt) = tracks_with(&[&rows, &rows]);
    keys(&mut app, "lvGWSKrandom<Enter>L");
    let (one, two) = (generators(&app, 0), generators(&app, 1));
    assert_eq!((one.len(), two.len()), (1, 1));
    assert_ne!(one[0].shape.seed, two[0].shape.seed);
    assert!(app.status.contains("2 tracks"), "{}", app.status);
    // Each is reopened and applied on its own.
    let first = track_steps(&app, 0);
    keys(&mut app, "Wglr");
    assert_eq!(app.shape.as_ref().unwrap().parts.len(), 1);
    keys(&mut app, "L");
    assert_eq!(track_steps(&app, 0), first);
    keys(&mut app, " ga");
    assert!(generators(&app, 1).is_empty());
    assert_eq!(generators(&app, 0).len(), 1);
}

#[test]
fn l_on_an_fx_column_with_a_lane_keeps_it_live_on_the_lane() {
    let (mut app, _rt) = app_with(&["C-3 40 cut:00 --- ---", N, N, N, N, N]);
    keys(&mut app, "lllll:lane 3<Enter>vjjSjji40jjjjKall<Enter>L");
    let live = generators(&app, 0);
    assert_eq!(live[0].span, vt_model::generator::Span::Lane);
    assert!(app.status.contains("lane"), "{}", app.status);
    keys(&mut app, ":lane 5<Enter>");
    assert_eq!(
        generators(&app, 0)[0].rows(),
        Some(vt_model::selection::StepRange::between(0, 4))
    );
}

#[test]
fn a_live_column_is_drawn_in_the_shape_panels_colour_and_named_over_the_grid() {
    let (app, _rt) = live_tresillo();
    let backend = ratatui::backend::TestBackend::new(120, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let title: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
    // The wand, `glyphs::LIVE`, before the live columns.
    assert!(title.contains("\u{f0068} Vel"), "{title}");
    // The velocity on the second step, clear of the cursor; the note isn't marked.
    let row = 3;
    let at = (0..120)
        .find(|&x| buffer[(x, row)].symbol() == "C")
        .unwrap();
    assert_eq!(buffer[(at + 4, row)].fg, vt_tui::theme::TEAL);
    assert_ne!(buffer[(at, row)].fg, vt_tui::theme::TEAL);
}
