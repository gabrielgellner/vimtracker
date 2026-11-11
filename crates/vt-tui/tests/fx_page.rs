//! The track page's FX page: a device for each slot, the cursor through their titles and
//! grids, `dd`, `p` and `b`, and their graphic panels, the compressor's and EQ's curves and the
//! gain reduction meters, sent only when they change.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::engine::Transport;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

/// The FX page's text for a slot: its device's lines, from its title on its frame's top to its
/// frame's bottom.
fn fx_line(app: &App, slot: usize) -> String {
    let lines = fx_page_text(app);
    let at = lines
        .iter()
        .position(|line| line.starts_with(&format!("╭─ {} ", slot + 1)))
        .unwrap();
    lines[at..at + 9].join("\n")
}

#[test]
fn l_reaches_the_fx_page_and_stops_there() {
    use vt_tui::track_page::Page;
    let (mut app, _realtime) = on_fx_page();
    assert_eq!(app.track_page.page, Page::Fx);
    press(&mut app, "L");
    assert_eq!(app.track_page.page, Page::Fx, "it stops at the last");
    press(&mut app, "H");
    assert_eq!(app.track_page.page, Page::Mod);
    press(&mut app, "L");
    for slot in [0, 2] {
        let device = fx_line(&app, slot);
        let title = slot_title(slot, None, "Empty ─");
        assert!(device.starts_with(&title), "{device}");
        assert!(device.contains("│  K  pick an effect"), "{device}");
    }
}

#[test]
fn a_slot_takes_an_effect_and_shows_its_settings_from_its_list() {
    use vt_model::insert::Insert;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    let depth = app.history.undo_depth();
    assert_eq!(column_name(&app), "Effect");

    // Nothing but the effect, until there's something in the slot.
    press(&mut app, "$");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::ALL[1]);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    // The effect and its type are in the title, and its settings in its grid.
    let line = fx_line(&app, 0);
    assert!(
        line.starts_with(&slot_title(0, Some(true), "Distort  Soft ─")),
        "{line}"
    );
    assert!(line.contains("│  Amount    40 ▕"), "{line}");
    assert!(line.contains("Mix       FF ▕█████████▏"), "{line}");
    assert!(line.contains("Gain      E0 ▕"), "{line}");
    assert!(!line.contains("Type"), "{line}");

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Type");
    press(&mut app, "=");
    assert!(fx_line(&app, 0).contains("Distort  Hard"));
    press(&mut app, "-");
    press(&mut app, "-");
    let line = fx_line(&app, 0);
    assert!(
        line.contains("Distort  Distortion"),
        "it goes round: {line}"
    );
    // A pedal has a tone control as well.
    assert!(line.contains("Tone      80 ▕"), "{line}");
    press(&mut app, "-");
    assert!(fx_line(&app, 0).contains("Distort  Overdrive"));
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "fold");
    special(&mut app, KeyCode::Enter);
    assert!(fx_line(&app, 0).contains("Distort  Fold"));
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "typing a choice is the list too");
    special(&mut app, KeyCode::Esc);

    // Under the type is the grid's second column, what comes out.
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "h");
    assert_eq!(column_name(&app), "Amount");
    press(&mut app, "+");
    assert!(fx_line(&app, 0).contains("Amount    50"));
    press(&mut app, "ia7");
    assert!(fx_line(&app, 0).contains("Amount    A7"));
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Amount    40"));
    let depth = app.history.undo_depth();
    press(&mut app, "x");
    assert_eq!(
        app.history.undo_depth(),
        depth,
        "already there, so no undo step"
    );

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Gain");
    press(&mut app, "_");
    assert!(fx_line(&app, 0).contains("Gain      D0"));
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Gain      E0"));
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gain", "it stops at the last it uses");

    // Only this slot, and only this track.
    assert_eq!(app.song().tracks[0].inserts[1], Insert::Empty);
    assert_eq!(app.song().tracks[1].inserts[0], Insert::Empty);
}

#[test]
fn emptying_a_slot_keeps_the_cursor_valid() {
    use vt_model::insert::Insert;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=j$");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "k0x");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    // Undo puts the settings back, and taking them away again leaves the cursor on the effect.
    press(&mut app, "uj$");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    assert_eq!(column_name(&app), "Effect");
    // Another track's slot being empty does the same.
    press(&mut app, "=j$W");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "B");
    assert_eq!(column_name(&app), "Mix");
}

#[test]
fn the_effect_list_chooses_one_and_keeps_settings_if_it_is_already_there() {
    use vt_model::insert::Insert;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "dist");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::ALL[1]);
    press(&mut app, "j");
    press(&mut app, "i10");
    press(&mut app, "kK");
    press(&mut app, "dist");
    special(&mut app, KeyCode::Enter);
    assert!(
        fx_line(&app, 0).contains("Amount    10"),
        "{}",
        fx_line(&app, 0)
    );
}

#[test]
fn a_jump_finds_a_slots_settings_only_where_they_exist() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "/amo");
    assert_eq!(
        app.field_jump.as_ref().unwrap().query,
        "",
        "nothing to find yet"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, "=}=");
    press(&mut app, "/amo");
    let jump = app.field_jump.as_ref().unwrap();
    assert!(jump.label(Control::InsertParam(0, 1)).is_some());
    let second = jump.label(Control::InsertParam(1, 1)).unwrap();
    assert!(jump.label(Control::InsertParam(2, 1)).is_none());
    // The name and the value both carry it, in the columns they had.
    let line = fx_line(&app, 1);
    assert!(
        line.contains(&format!("Amo{second}nt    {second}0 ▕")),
        "{line}"
    );
    press(&mut app, &second.to_string());
    assert_eq!(column_name(&app), "Amount");
    assert_eq!(
        app.track_page.row, 5,
        "the second slot's first row of settings"
    );
}

#[test]
fn an_insert_set_on_the_page_is_heard() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 3k;");
    let plain = loudest(&mut realtime, 2000);
    assert!(plain > 0.0);
    press(&mut app, " 4LL=lj");
    press(&mut app, "iff");
    block(&mut realtime, &mut app);
    press(&mut app, " 3;");
    let driven = loudest(&mut realtime, 2000);
    assert!(driven > 0.0);
    assert_ne!(plain, driven, "{plain} and {driven}");
}

#[test]
fn the_cursors_device_has_a_brighter_frame_and_a_bypassed_one_goes_faint() {
    use ratatui::style::Modifier;
    use vt_tui::theme;
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(150, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = on_fx_page();
    // A Distort in the first slot and a Delay in the second, with the cursor on the Distort's
    // Amount.
    press(&mut app, "=}K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "{j");
    let (first, second) = (2, 11);
    let buffer = draw(&app);
    // The frames, and the effects' names in the titles, after the slot's digit and the room for
    // its switch.
    assert_eq!(buffer[(0, first)].fg, theme::OVERLAY2);
    assert_eq!(buffer[(0, second)].fg, theme::SURFACE1);
    assert_eq!(buffer[(149, first + 4)].fg, theme::OVERLAY2);
    assert_eq!(buffer[(9, first)].symbol(), "D");
    assert!(buffer[(9, first)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(9, second)].symbol(), "D");
    assert!(!buffer[(9, second)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(3, first)].fg, theme::OVERLAY2, "the digit is dim");
    // Beyond the title, the top border keeps its frame's colour.
    assert_eq!(buffer[(40, first)].symbol(), "─");
    assert_eq!(buffer[(40, first)].fg, theme::OVERLAY2);

    // Bypassed, everything in a slot is faint, its title's effect too, but for the cursor.
    press(&mut app, "b");
    assert!(app.song().tracks[0].bypassed[0]);
    let buffer = draw(&app);
    let amount_value = (13, first + 2);
    assert_eq!(buffer[(3, first + 2)].symbol(), "A");
    assert_eq!(buffer[(3, first + 2)].fg, theme::OVERLAY0);
    assert_eq!(buffer[(9, first)].fg, theme::OVERLAY0);
    assert_eq!(buffer[amount_value].bg, theme::ROSEWATER, "the cursor");
    assert_eq!(buffer[(30, first + 2)].symbol(), "M");
    assert_eq!(buffer[(30, first + 2)].fg, theme::OVERLAY0);
    // The other slot isn't.
    assert_eq!(buffer[(3, second + 2)].fg, theme::OVERLAY2);
}

#[test]
fn the_cursor_goes_through_each_slots_title_and_grid_over_its_holes() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    // Glue, an EQ, an empty slot, and a Distort.
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "l==h}K");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "}}=gg");
    let name = |app: &App| column_name(app);
    let at = |app: &App| (name(app), app.track_page.row);
    assert_eq!(name(&app), "Effect");
    // The title has the effect and its type, and the cursor stays in it along the row.
    press(&mut app, "l");
    assert_eq!(name(&app), "Type");
    press(&mut app, "l");
    assert_eq!(name(&app), "Type", "it stops at the last");
    press(&mut app, "h");
    assert_eq!(name(&app), "Effect");

    // Down the grid, keeping the column, and a hole takes the nearest setting in its row.
    press(&mut app, "j");
    assert_eq!(at(&app), ("Threshold".to_owned(), 1));
    press(&mut app, "lll");
    assert_eq!(name(&app), "Mix");
    press(&mut app, "j");
    assert_eq!(name(&app), "Peak Clip");
    press(&mut app, "j");
    assert_eq!(at(&app), ("Source".to_owned(), 3));
    press(&mut app, "j");
    assert_eq!(at(&app), ("Effect".to_owned(), 4), "the next slot's title");
    press(&mut app, "j");
    assert_eq!(name(&app), "Gain", "the column it was in");
    press(&mut app, "j");
    assert_eq!(name(&app), "High");
    press(&mut app, "j");
    assert_eq!(at(&app), ("Mid Q".to_owned(), 7));
    // An empty slot has only its effect, and the rows after it are passed by.
    press(&mut app, "j");
    assert_eq!(at(&app), ("Effect".to_owned(), 8));
    press(&mut app, "j");
    assert_eq!(
        at(&app),
        ("Type".to_owned(), 12),
        "the nearest to the column"
    );
    press(&mut app, "j");
    assert_eq!(name(&app), "Mix");
    press(&mut app, "jj");
    assert_eq!(at(&app), ("Gain".to_owned(), 14), "it stops at the last");

    // Up again, and from the first row k reaches the type in the title.
    press(&mut app, "kk");
    assert_eq!(at(&app), ("Type".to_owned(), 12));
    press(&mut app, "kk");
    assert_eq!(at(&app), ("Mid Q".to_owned(), 7));
    // `0` and `$` go to a row's ends, over its holes.
    press(&mut app, "k");
    assert_eq!(name(&app), "High", "still aiming for the last column");
    press(&mut app, "0");
    assert_eq!(name(&app), "Low");
    press(&mut app, "l");
    assert_eq!(name(&app), "Mid");
    press(&mut app, "$");
    assert_eq!(name(&app), "High");
    press(&mut app, "l");
    assert_eq!(name(&app), "High", "nothing after it");

    // `gg` and `G` are the first and last rows, and a count goes as far as it says.
    press(&mut app, "G");
    assert_eq!(at(&app), ("Gain".to_owned(), 14));
    press(&mut app, "gg");
    assert_eq!(at(&app), ("Type".to_owned(), 0));
    press(&mut app, "h2j");
    assert_eq!(at(&app), ("Ratio".to_owned(), 2));
}

#[test]
fn braces_go_to_the_same_setting_in_the_next_slot() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=}={"); // Slots 1 and 2 hold an effect.
    press(&mut app, "j");
    assert_eq!(
        (column_name(&app).as_str(), app.track_page.row),
        ("Amount", 1)
    );
    press(&mut app, "}");
    assert_eq!(
        (column_name(&app).as_str(), app.track_page.row),
        ("Amount", 5)
    );
    // An empty slot has only its effect, and the setting comes back after it.
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "{");
    assert_eq!(
        (column_name(&app).as_str(), app.track_page.row),
        ("Amount", 5)
    );
    press(&mut app, "{{{");
    assert_eq!(app.track_page.row, 1, "it stops at the first");
    // From the title, it's the next slot's title.
    press(&mut app, "kl}");
    assert_eq!(
        (column_name(&app).as_str(), app.track_page.row),
        ("Type", 4)
    );
    // A count.
    press(&mut app, "{j2}");
    assert_eq!(app.track_page.row, 9);
    press(&mut app, "9}");
    assert_eq!(app.track_page.row, 13, "it stops at the last");
}

#[test]
fn a_jump_labels_a_slots_effect_once() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "/effect");
    let jump = app.field_jump.as_ref().unwrap();
    let labels: Vec<char> = (0..3)
        .map(|slot| jump.label(Control::Insert(slot)).unwrap())
        .collect();
    assert_eq!(
        labels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    press(&mut app, &labels[2].to_string());
    assert_eq!(app.track_page.row, 8);
}

#[test]
fn crush_shows_whole_bits_that_step_and_stop_at_each_end() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=lK");
    press(&mut app, "crush");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "hj");
    assert_eq!(column_name(&app), "Bits");
    let line = fx_line(&app, 0);
    assert!(line.contains("Bits      9"), "{line}");
    assert!(!line.contains("Amount"), "{line}");
    press(&mut app, "=");
    assert!(fx_line(&app, 0).contains("Bits      10"));
    press(&mut app, "=====");
    assert!(
        fx_line(&app, 0).contains("Bits      12"),
        "it stops at the top"
    );
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Bits      9"));
    for _ in 0..12 {
        press(&mut app, "-");
    }
    assert!(
        fx_line(&app, 0).contains("Bits      2"),
        "it stops at the bottom"
    );
    // It's a list to choose from, and not typed.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "6");
    special(&mut app, KeyCode::Enter);
    assert!(
        fx_line(&app, 0).contains("Bits      6"),
        "{}",
        fx_line(&app, 0)
    );
    // Another mode has its own setting, as it was.
    press(&mut app, "kl-");
    assert!(
        fx_line(&app, 0).contains("Distort  Sine"),
        "{}",
        fx_line(&app, 0)
    );
    assert!(
        fx_line(&app, 0).contains("Amount    40"),
        "{}",
        fx_line(&app, 0)
    );
}

#[test]
fn the_fx_pages_bars_follow_what_plays() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=");
    let mut live = vt_engine::track_state::LiveValues {
        level: 0xE0,
        pan: 0x80,
        cutoff: 0xFF,
        resonance: 0,
        send_a: 0,
        send_b: 0,
        machine: [0x80, 0, 0x80, 0xCC, 0, 0, 0, 0],
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
        after_trim: Default::default(),
        lfos: Default::default(),
    };
    live.inserts[0][..4].copy_from_slice(&[0, 0xFF, 0xFF, 0xE0]);
    app.live = Some(live);
    let line = fx_line(&app, 0);
    // The number stays what's set, and the fill goes where it's playing, with a mark on the setting.
    assert!(line.contains("Amount    40 ▕██│██████▏"), "{line}");
    assert!(line.contains("Mix       FF ▕█████████▏"), "{line}");
}

#[test]
fn dd_and_p_move_an_effect_and_swap_two_as_one_undo_step() {
    use vt_model::insert::{DistortType, Insert};
    let (mut app, _realtime) = on_fx_page();
    let inserts = |app: &App| app.song().tracks[0].inserts;
    let soft = Insert::ALL[1];
    let fold = soft.with_value(0, DistortType::Fold as u8);
    // Soft in slot 1, and Fold in slot 2.
    press(&mut app, "=}=lK");
    press(&mut app, "fold");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "gg0");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty, Insert::Empty]);
    let depth = app.history.undo_depth();

    // Slot 1 into the last slot, which is empty.
    press(&mut app, "dd");
    assert_eq!(
        inserts(&app),
        [Insert::Empty, fold, Insert::Empty, Insert::Empty]
    );
    assert!(app.status.contains("Distort"), "{}", app.status);
    press(&mut app, "}}}p");
    assert_eq!(inserts(&app), [Insert::Empty, fold, Insert::Empty, soft]);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    press(&mut app, "u");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty, Insert::Empty]);

    // Slot 1 onto slot 2: they swap.
    press(&mut app, "gg");
    press(&mut app, "dd}p");
    assert_eq!(inserts(&app), [fold, soft, Insert::Empty, Insert::Empty]);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty, Insert::Empty]);

    // Back where it came from changes nothing.
    press(&mut app, "ggddp");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty, Insert::Empty]);
}

#[test]
fn dd_and_p_say_why_when_there_is_nothing_to_move() {
    use vt_model::insert::Insert;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "dd");
    assert!(app.status.contains("nothing"), "{}", app.status);
    press(&mut app, "p");
    assert!(app.status.contains("dd"), "{}", app.status);
    // On the other pages they aren't for steps or anything else.
    press(&mut app, "=dd");
    press(&mut app, "H");
    press(&mut app, "p");
    assert!(app.status.contains("FX page"), "{}", app.status);
    assert_eq!(app.view, View::Track);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    // It's still there to put, across tracks too.
    press(&mut app, "LW");
    press(&mut app, "p");
    assert_eq!(app.song().tracks[1].inserts[0], Insert::ALL[1]);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
}

#[test]
fn the_sound_follows_the_new_order() {
    use vt_engine::track_state::LiveValues;
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 4LL=dd}p");
    block(&mut realtime, &mut app);
    press(&mut app, " 3k;");
    let _ = loudest(&mut realtime, 4000);
    app.tick();
    let live: LiveValues = app.live.unwrap();
    assert_eq!(live.inserts[1][1], 0x40, "the second slot has it");
    assert_eq!(live.inserts[0][..4], [0; 4], "the first doesn't");
}

#[test]
fn b_bypasses_a_slot_and_brings_it_back_with_undo() {
    use vt_model::insert::INSERTS;
    let (mut app, _realtime) = on_fx_page();
    let bypassed = |app: &App| app.song().tracks[0].bypassed;
    press(&mut app, "b");
    assert!(app.status.contains("nothing"), "{}", app.status);
    assert_eq!(bypassed(&app), [false; INSERTS]);
    press(&mut app, "=b");
    assert_eq!(bypassed(&app), [true, false, false, false]);
    assert!(app.status.contains("Bypassed"), "{}", app.status);
    press(&mut app, "b");
    assert_eq!(bypassed(&app), [false; INSERTS]);
    press(&mut app, "bu");
    assert_eq!(bypassed(&app), [false; INSERTS]);
    press(&mut app, "b");
    // Emptying the slot brings it back in, for whatever goes there next.
    press(&mut app, "x");
    assert_eq!(bypassed(&app), [false; INSERTS]);
}

#[test]
fn zero_and_dollar_go_to_the_ends_of_a_slots_row() {
    use vt_tui::track_page::Control;
    let (mut app, _rt) = app();
    press(&mut app, " 4LL");
    press(&mut app, "K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    // Delay's first row is Time, Low cut and Width.
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::InsertParam(0, 0));
    press(&mut app, "$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 3));
    press(&mut app, "0");
    assert_eq!(app.track_control(), Control::InsertParam(0, 0));
    // Its last is Ping-pong, Drive and Gain.
    press(&mut app, "jj$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 8));
    press(&mut app, "0");
    assert_eq!(app.track_control(), Control::InsertParam(0, 2));
    press(&mut app, "h");
    assert_eq!(
        app.track_control(),
        Control::InsertParam(0, 2),
        "the effect is above, in the title, not to the left"
    );
    // The title has only the effect, as a delay has no type.
    press(&mut app, "kkk$");
    assert_eq!(app.track_control(), Control::Insert(0));
    press(&mut app, "0");
    assert_eq!(app.track_control(), Control::Insert(0));
}

#[test]
fn a_compressors_curve_is_sent_once_and_again_only_for_its_threshold_or_ratio() {
    use vt_model::insert::{Compressor, Insert};
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=13,r=5,")
            .count()
    };
    press(&mut app, " 4LL");
    assert_eq!(sync(&mut app, &mut graphics), 0, "no slot has a drawing");
    assert_eq!(app.slot_images, [None; vt_model::insert::INSERTS]);
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    assert!(matches!(
        app.song().tracks[0].inserts[0],
        Insert::Compressor(_)
    ));
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let first = app.slot_images[0].unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // The attack isn't on the curve, so changing it sends nothing.
    press(&mut app, "jl=");
    assert!(matches!(
        app.song().tracks[0].inserts[0],
        Insert::Compressor(Compressor { attack: 0x56, .. })
    ));
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.slot_images[0], Some(first));
    // The threshold is.
    press(&mut app, "h=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.slot_images[0], Some(first));
}

#[test]
fn a_compressors_type_shows_its_own_settings_and_console_and_glue_pick_their_switches_with_k() {
    use vt_model::insert::{CONSOLE_AUTO_RELEASE, Compressor, CompressorType, Insert, range_db};
    let screen = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(163, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..24)
            .map(|y| {
                (0..163)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let compressor = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => compressor,
        other => panic!("{other:?}"),
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).kind, CompressorType::Clean);
    let clean = screen(&app);
    assert!(
        clean.contains("Comp  Clean"),
        "its type is in its title: {clean}"
    );
    assert!(clean.contains("Auto"), "{clean}");

    // `=` on Type steps it to Console, which shows its switches, and Auto and Makeup as Clean
    // does.
    press(&mut app, "l=");
    assert_eq!(compressor(&app).kind, CompressorType::Console);
    let console = screen(&app);
    for shown in [
        "Console", "Ratio", "4:1", "Attack", "10 ms", "Release", "Auto", "Mix", "Makeup",
    ] {
        assert!(console.contains(shown), "{shown}: {console}");
    }
    // Auto is off, as a toggle.
    let off = vt_tui::views::widgets::toggle::text(false);
    assert!(console.contains(&off), "{console}");
    assert!(!console.contains("Gain"), "{console}");
    assert_eq!(compressor(&app).console_release, CONSOLE_AUTO_RELEASE);

    // The ratio is picked from its switch's three with `K`, and the release from its five. Under
    // the type is the attack, and the ratio under the threshold.
    press(&mut app, "jhj");
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 3);
    press(&mut app, "10:1");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).console_ratio, 2);
    press(&mut app, "lK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 5);
    press(&mut app, "0.3 s");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).console_release, 1);

    // Back to Clean, its own settings are as they were, and the console's are kept.
    press(&mut app, "kk-");
    let back = compressor(&app);
    assert_eq!(back.kind, CompressorType::Clean);
    assert_eq!(back.ratio, Compressor::DEFAULT.ratio);
    assert_eq!((back.console_ratio, back.console_release), (2, 1));

    // Two steps on is Glue, with Live's switches where Live loads them, its Range over the
    // makeup, and its clip after them.
    press(&mut app, "==");
    assert_eq!(compressor(&app).kind, CompressorType::Glue);
    let glue = screen(&app);
    for shown in [
        "Glue",
        "Ratio",
        "4:1",
        "Attack",
        "1 ms",
        "Release",
        "0.6 s",
        "Auto",
        "Mix",
        "Range",
        "Makeup",
        "Peak Clip",
    ] {
        assert!(glue.contains(shown), "{shown}: {glue}");
    }
    // Its attack has seven places, from 0.01 ms, and its release seven, Auto last.
    press(&mut app, "jK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 7);
    press(&mut app, "0.01 ms");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).glue_attack, 0);
    press(&mut app, "jK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 7);
    press(&mut app, "Auto");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).glue_release, 6);
    // The range reads in dB, all 70 of them as it starts. It's under the ratio.
    press(&mut app, "jh");
    let range = compressor(&app).range;
    assert_eq!(range, 0xFF);
    let shown = format!("{:.1} dB", range_db(f32::from(range)));
    assert_eq!(shown, "70.0 dB");
    assert!(status_line(&app).contains(&shown), "{}", status_line(&app));
}

#[test]
fn a_compressors_makeup_is_its_last_setting_and_reads_in_db() {
    use vt_model::insert::{CompressorType, Insert};
    let makeup = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => (compressor.kind, compressor.makeup),
        other => panic!("{other:?}"),
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    // On Console, Makeup heads the third column, after the attack under the type. It starts at
    // none and goes up in dB.
    press(&mut app, "l=");
    press(&mut app, "jl");
    assert!(
        status_line(&app).contains("+0.0 dB"),
        "{}",
        status_line(&app)
    );
    press(&mut app, "+");
    let (kind, value) = makeup(&app);
    assert_eq!(kind, CompressorType::Console);
    assert!(value > 0, "{value}");
    let shown = format!("{:+.1} dB", vt_model::insert::makeup_db(f32::from(value)));
    assert!(status_line(&app).contains(&shown), "{}", status_line(&app));
}

#[test]
fn a_compressor_and_a_limiter_show_how_far_theyre_turning_down() {
    use vt_engine::track_state::LiveValues;
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(150, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    // A limiter in the third slot.
    press(&mut app, "}}K");
    press(&mut app, "Limit");
    special(&mut app, KeyCode::Enter);
    app.slot_images = [Some(0x01_02_03), None, None, None];
    // 18 dB is nine tenths of the meter's 20: all of its top four rows and half of the last.
    app.live = Some(LiveValues {
        reduction: [18.0, 0.0, 3.0, 0.0],
        ..LiveValues::default()
    });
    let buffer = draw(&app);
    let row_of = |text: &str| {
        (0..40)
            .find(|y| {
                (0..150)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .starts_with(text)
            })
            .unwrap()
    };
    // The curve's placeholders in their box, 13 by 5, then the meter in its own, captioned, and
    // its scale, against the right of the slot's frame, with two columns between it and the
    // frame, filling the seven rows inside it.
    let comp = row_of(&slot_title(0, Some(true), "Comp"));
    let text = |buffer: &ratatui::buffer::Buffer, y: u16| {
        (0..150)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let rule = "─".repeat(13);
    assert!(
        text(&buffer, comp + 1).ends_with(&format!("┌{rule}┐  ┌──┐      │")),
        "{}",
        text(&buffer, comp + 1)
    );
    assert!(
        text(&buffer, comp + 7).ends_with(&format!("└{rule}┘  └GR┘      │")),
        "{}",
        text(&buffer, comp + 7)
    );
    assert_eq!(buffer[(123, comp + 2)].symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(buffer[(123, comp + 2)].fg, Color::Rgb(1, 2, 3));
    assert_eq!(buffer[(136, comp + 4)].symbol(), "│");
    // The scale beside it, 0 at the top and 4 dB a row.
    for (offset, scale) in [
        (2, "   0"),
        (3, "    "),
        (4, "  -8"),
        (5, "    "),
        (6, " -16"),
    ] {
        let line = text(&buffer, comp + offset);
        assert!(line.ends_with(&format!("│{scale}  │")), "{line}");
    }
    for row in 2..6 {
        assert_eq!(buffer[(140, comp + row)].symbol(), "█");
        assert_eq!(buffer[(141, comp + row)].symbol(), "█");
    }
    // Half a cell filled from the top: the lower half drawn in the background's colour.
    assert_eq!(buffer[(140, comp + 6)].symbol(), "▄");
    assert_eq!(buffer[(140, comp + 6)].fg, vt_tui::theme::BASE);
    // A limiter has its meter alone, with its scale, where the compressor's is: 3 dB is the top
    // three quarters of its top row, six eighths at half a dB each.
    let limit = row_of(&slot_title(2, Some(true), "Limit"));
    assert!(
        text(&buffer, limit + 7).ends_with("└GR┘      │"),
        "{}",
        text(&buffer, limit + 7)
    );
    assert!(text(&buffer, limit + 6).ends_with("│  │ -16  │"));
    assert_eq!(buffer[(140, limit + 2)].symbol(), "▂");
    assert_eq!(buffer[(140, limit + 3)].symbol(), " ");

    // The deepest it went lately is held as a faint mark below what it's turning down now: 12 dB
    // is the bottom of the third row.
    app.slot_reduction[2].update(12.0, 0.0);
    let held = draw(&app);
    assert_eq!(held[(140, limit + 2)].symbol(), "▂");
    assert_eq!(held[(140, limit + 3)].symbol(), " ");
    assert_eq!(held[(140, limit + 4)].symbol(), "▁");
    assert_eq!(held[(141, limit + 4)].fg, vt_tui::theme::OVERLAY0);
    assert_eq!(held[(140, limit + 5)].symbol(), " ");
    // 9 dB is just past the top of the third row.
    let mut lately = vt_tui::meter::Held::new(0.0);
    lately.update(9.0, 0.0);
    app.slot_reduction[2] = lately;
    let held = draw(&app);
    assert_eq!(held[(140, limit + 4)].symbol(), "▔");
    assert_eq!(held[(140, limit + 3)].symbol(), " ");

    // Bypassed, it turns nothing down and holds nothing, and its meter is empty.
    press(&mut app, "b");
    assert_eq!(app.song().tracks[0].bypassed, [false, false, true, false]);
    let bypassed = draw(&app);
    assert_eq!(bypassed[(140, limit + 2)].symbol(), "▔");
    assert_eq!(bypassed[(140, limit + 4)].symbol(), " ");
}

#[test]
fn an_eqs_curve_follows_its_settings_and_the_cursor() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=33,r=5,")
            .count()
    };
    press(&mut app, " 4LLK");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.slot_images[0].unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // Away from the slot it's dim, once, and coming back sends nothing.
    press(&mut app, "}");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let plain = app.slot_images[0].unwrap();
    assert_ne!(plain, picked);
    press(&mut app, "{");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.slot_images[0], Some(picked));

    // Any of its settings draws it again.
    press(&mut app, "j=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.slot_images[0], Some(picked));

    let buffer = {
        let backend = ratatui::backend::TestBackend::new(150, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let row = (0..24)
        .find(|y| {
            (0..150)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("EQ3")
        })
        .unwrap();
    // In its box against the right of the slot's frame, two columns in, 33 by 5 inside, filling
    // the seven rows inside the frame.
    let corner = |row: usize, column: usize| {
        let line: Vec<char> = vt_tui::kitty::placeholder_lines(33, 5)[row]
            .chars()
            .collect();
        line[column * 3..column * 3 + 3].iter().collect::<String>()
    };
    assert_eq!(buffer[(112, row + 1)].symbol(), "┌");
    assert_eq!(buffer[(113, row + 2)].symbol(), corner(0, 0));
    assert_eq!(buffer[(145, row + 6)].symbol(), corner(4, 32));
    assert_eq!(buffer[(146, row + 7)].symbol(), "┘");
    assert_eq!(buffer[(149, row + 4)].symbol(), "│");
}

/// The colour a panel's PNG draws its line in, the first after the see-through one.
fn line_color(png: &[u8]) -> Color {
    let palette = png_chunk(png, b"PLTE");
    Color::Rgb(palette[3], palette[4], palette[5])
}

/// What one sync sends: the images, by id.
fn synced(
    app: &mut App,
    graphics: &mut vt_tui::kitty::Graphics,
) -> std::collections::HashMap<u32, Vec<u8>> {
    let mut out = Vec::new();
    graphics.sync(app, &mut out).unwrap();
    vt_tui::screenshot::sent_images(&out)
}

#[test]
fn a_bypassed_slots_curve_is_faint_and_each_shade_is_sent_once() {
    use vt_tui::theme;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    let mut graphics = vt_tui::kitty::Graphics::new();
    let sent = synced(&mut app, &mut graphics);
    let focused = app.slot_images[0].unwrap();
    assert_eq!(line_color(&sent[&focused]), theme::active().fg.unwrap());

    // Switched out, it's faint, a picture of its own sent once.
    press(&mut app, "b");
    assert!(app.song().tracks[0].bypassed[0]);
    let sent = synced(&mut app, &mut graphics);
    let faint = app.slot_images[0].unwrap();
    assert_ne!(faint, focused);
    assert_eq!(line_color(&sent[&faint]), theme::faint().fg.unwrap());
    assert!(synced(&mut app, &mut graphics).is_empty());
    // Away from it, it stays faint.
    press(&mut app, "}");
    assert!(synced(&mut app, &mut graphics).is_empty());
    assert_eq!(app.slot_images[0], Some(faint));

    // Back in, away from it, it's dim, and with the cursor on it again, the bright one comes back
    // with nothing sent.
    press(&mut app, "{b}");
    assert!(!app.song().tracks[0].bypassed[0]);
    let sent = synced(&mut app, &mut graphics);
    let dim = app.slot_images[0].unwrap();
    assert_eq!(line_color(&sent[&dim]), theme::dim().fg.unwrap());
    press(&mut app, "{");
    assert!(synced(&mut app, &mut graphics).is_empty());
    assert_eq!(app.slot_images[0], Some(focused));
    press(&mut app, "b");
    assert!(synced(&mut app, &mut graphics).is_empty());
    assert_eq!(app.slot_images[0], Some(faint));
}

#[test]
fn the_panels_are_drawn_at_the_screens_pixels_and_sent_again_after_a_clear() {
    use ratatui::crossterm::terminal::WindowSize;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "}K");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    let mut graphics = vt_tui::kitty::Graphics::new();
    let sizes = |app: &App, sent: &std::collections::HashMap<u32, Vec<u8>>| {
        [0, 1].map(|slot| png_size(&sent[&app.slot_images[slot].unwrap()]))
    };
    // At the author's 15 by 39 pixels a character: the compressor's 13 by 5 is square.
    let sent = synced(&mut app, &mut graphics);
    assert_eq!(sizes(&app, &sent), [(195, 195), (495, 195)]);
    let first = app.slot_images;

    // A font of another size draws each again at its pixels, with new ids.
    let window = |columns: u16, rows: u16, cell: (u16, u16)| WindowSize {
        columns,
        rows,
        width: columns * cell.0,
        height: rows * cell.1,
    };
    graphics.use_window(window(163, 42, (10, 22)));
    let sent = synced(&mut app, &mut graphics);
    assert_eq!(sizes(&app, &sent), [(130, 110), (330, 110)]);
    let smaller = app.slot_images;
    assert!(smaller[..2].iter().zip(first).all(|(now, was)| *now != was));
    assert!(synced(&mut app, &mut graphics).is_empty());

    // The window changing size clears the screen, so each is stored again under a new id, and
    // the old ones are forgotten, as the clear has taken them off the screen.
    graphics.use_window(window(150, 40, (10, 22)));
    let mut out = Vec::new();
    graphics.sync(&mut app, &mut out).unwrap();
    assert_eq!(vt_tui::screenshot::sent_images(&out).len(), 2);
    assert!(
        app.slot_images[..2]
            .iter()
            .zip(smaller)
            .all(|(now, was)| *now != was)
    );
    let out = String::from_utf8(out).unwrap();
    for id in smaller.iter().flatten() {
        assert!(out.contains(&format!("a=d,d=I,i={id},q=2")), "{id}");
    }
    assert!(synced(&mut app, &mut graphics).is_empty());
}

#[test]
fn a_curve_under_an_lfo_is_sent_every_other_frame_and_costs_a_few_kilobytes() {
    use vt_engine::track_state::LiveValues;
    use vt_model::insert::MAX_PARAMS;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "}K");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    let mut graphics = vt_tui::kitty::Graphics::new();
    synced(&mut app, &mut graphics);
    let track = app.song().tracks[0].clone();
    // A second of frames, 30 of them, with the compressor's threshold and the EQ's low band
    // moving at every one, as an LFO moves them.
    let moved = |slot: usize, name: &str, by: u8| {
        let insert = track.inserts[slot];
        let mut values = [0; MAX_PARAMS];
        for (param, value) in values.iter_mut().enumerate().take(insert.params().len()) {
            *value = insert.value(param);
        }
        let param = insert
            .params()
            .iter()
            .position(|param| param.name == name)
            .unwrap();
        values[param] = values[param].wrapping_add(by);
        values
    };
    let (mut sends, mut bytes) = (0, 0);
    for frame in 0..30_u8 {
        let mut inserts = [[0; MAX_PARAMS]; vt_model::insert::INSERTS];
        inserts[0] = moved(0, "Threshold", 4 * frame + 1);
        inserts[1] = moved(1, "Low", 2 * frame + 1);
        app.live = Some(LiveValues {
            inserts,
            ..LiveValues::default()
        });
        let mut out = Vec::new();
        graphics.sync(&mut app, &mut out).unwrap();
        let commands = String::from_utf8(out.clone()).unwrap();
        sends += commands.matches("a=T,").count();
        bytes += out.len();
    }
    // 15 times a second for each, as the scope is.
    assert_eq!(sends, 30);
    let each = bytes / sends;
    assert!(each < 8000, "{each} bytes a picture");
}

#[test]
fn four_devices_fit_beside_the_main_meter_and_inside_the_keys_box_at_163x42() {
    use vt_model::insert::{Compressor, CompressorType, Insert};
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_fx_page();
    four_devices(&mut app);
    let compressor = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => compressor,
        other => panic!("{other:?}"),
    };
    assert_eq!(compressor(&app).kind, CompressorType::Glue);
    app.slot_images = [Some(0x01_02_03), Some(0x04_05_06), None, None];
    // Each row's text, with an image's placeholder, which is several characters, as `#`.
    let rows = |app: &mut App, keys: bool| {
        app.mode = if keys { Mode::Keys } else { Mode::Normal };
        let backend = ratatui::backend::TestBackend::new(163, 42);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..42)
            .map(|y| {
                (0..163)
                    .map(|x| match buffer[(x, y)].symbol() {
                        image if image.starts_with('\u{10EEEE}') => "#".to_owned(),
                        symbol => symbol.to_owned(),
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
    };
    let at = |line: &str, x: usize| line.chars().nth(x).unwrap_or(' ');
    // Four devices of nine rows fill rows 2 to 37 of the page, which is 150 wide beside the
    // main meter, with the frames against each other.
    let screen = rows(&mut app, false);
    for slot in 0..4 {
        let top = 2 + 9 * slot;
        let title = slot_title(slot, Some(true), "");
        assert!(screen[top].starts_with(&title), "{}", screen[top]);
        assert_eq!(at(&screen[top], 149), '╮', "{}", screen[top]);
        for row in &screen[top + 1..top + 8] {
            assert_eq!((at(row, 0), at(row, 149)), ('│', '│'), "{row}");
        }
        assert!(screen[top + 8].starts_with('╰'), "{}", screen[top + 8]);
        assert_eq!(at(&screen[top + 8], 149), '╯');
    }
    assert!(screen[38].trim().is_empty() || !screen[38].starts_with('╭'));
    // Glue's panel: its curve's box, 13 by 5 inside, the meter's, captioned, and its scale,
    // against the frame with two columns between, filling the seven rows inside it; and EQ3's
    // curve, 33 by 5 inside.
    let span = |line: &str, from: usize| line.chars().take(150).skip(from).collect::<String>();
    let rule = |width: usize| "─".repeat(width);
    assert_eq!(
        span(&screen[3], 122),
        format!("┌{}┐  ┌──┐      │", rule(13))
    );
    assert_eq!(
        span(&screen[4], 122),
        format!("│{}│  │▔▔│   0  │", "#".repeat(13))
    );
    assert_eq!(
        span(&screen[8], 122),
        format!("│{}│  │  │ -16  │", "#".repeat(13))
    );
    assert_eq!(
        span(&screen[9], 122),
        format!("└{}┘  └GR┘      │", rule(13))
    );
    assert_eq!(span(&screen[12], 112), format!("┌{}┐  │", rule(33)));
    assert_eq!(span(&screen[13], 112), format!("│{}│  │", "#".repeat(33)));
    assert_eq!(span(&screen[18], 112), format!("└{}┘  │", rule(33)));
    // Glue's settings fill all four columns, the last ending well before its graphics.
    let glue = &screen[4];
    assert!(glue.contains("│  Threshold"), "{glue}");
    assert!(glue.contains("Mix       FF ▕█████████▏"), "{glue}");
    let off = vt_tui::views::widgets::toggle::text(false);
    assert!(
        screen[6].contains(&format!("Peak Clip {off}")),
        "{}",
        screen[6]
    );
    // In the keyboard's box, one column and row in on each side, it all still fits: the devices
    // fill rows 3 to 38, with the box's bottom under them.
    let boxed = rows(&mut app, true);
    for slot in 0..4 {
        let top = 3 + 9 * slot;
        let title = format!("│{}", slot_title(slot, Some(true), ""));
        assert!(boxed[top].starts_with(&title), "{}", boxed[top]);
        assert_eq!(
            (at(&boxed[top], 148), at(&boxed[top], 149)),
            ('╮', '│'),
            "{}",
            boxed[top]
        );
        assert!(boxed[top + 8].starts_with("│╰"), "{}", boxed[top + 8]);
    }
    assert!(boxed[39].starts_with('╰'), "{}", boxed[39]);
    // Down Glue's last column the cursor reaches Peak Clip, picked from its two with `K`, which
    // switches the clip in.
    app.mode = Mode::Normal;
    press(&mut app, "ggj$j");
    assert_eq!(app.track_control(), Control::InsertParam(0, 10));
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 2);
    press(&mut app, "On");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).peak_clip, 1);
    assert_eq!(Compressor::DEFAULT.peak_clip, 0, "off, as Live loads it");
}

#[test]
fn the_fx_page_fades_a_slot_a_lock_bypasses_while_the_song_plays() {
    use vt_engine::track_state::LiveValues;
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    // Off the slot, so the cursor doesn't keep its colour.
    press(&mut app, "jj");
    // Whether the slot's effect is drawn faint.
    let faint = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height).any(|y| {
            let row: Vec<&str> = (0..60).map(|x| buffer[(x, y)].symbol()).collect();
            let at = row.windows(5).position(|cells| cells.concat() == "Delay");
            at.is_some_and(|x| buffer[(x as u16, y)].fg == vt_tui::theme::faint().fg.unwrap())
        })
    };
    let heard = |bypassed: bool| LiveValues {
        bypassed: [bypassed, false, false, false],
        ..LiveValues::default()
    };
    assert!(!faint(&app));
    app.transport = Transport::Playing;
    app.live = Some(heard(true));
    assert!(faint(&app), "a lock switches it out as it plays");
    app.live = Some(heard(false));
    assert!(!faint(&app));
    // Stopped, it shows the slot's own, whatever was last heard.
    app.transport = Transport::Stopped;
    app.live = Some(heard(true));
    assert!(!faint(&app));
    press(&mut app, "kkb");
    press(&mut app, "jj");
    assert!(faint(&app));
}

#[test]
fn a_slots_toggle_follows_b_and_a_lock_while_the_song_plays() {
    use vt_engine::track_state::LiveValues;
    use vt_tui::glyphs::{SWITCH_OFF, SWITCH_ON};
    use vt_tui::theme;
    let (mut app, _realtime) = on_fx_page();
    // A Delay in the first slot, the second left empty, and the cursor off the slot's title.
    press(&mut app, "K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "j");
    let (first, second) = (2, 11);
    // The switch is after the slot's digit, where the room for it was, and the effect is where it
    // was before it came.
    let switch = |app: &App| {
        let buffer = track_page_buffer(app);
        assert_eq!(buffer[(3, first)].symbol(), "1");
        assert_eq!(
            buffer[(7, first)].symbol(),
            "\u{a0}",
            "it keeps to its cell"
        );
        assert_eq!(buffer[(9, first)].symbol(), "D");
        let (glyph, color, _) = glyph_at(&buffer, (6, first));
        (glyph, color)
    };
    let on = (SWITCH_ON, theme::GREEN);
    let off = (SWITCH_OFF, theme::OVERLAY0);
    assert_eq!(switch(&app), on);
    // An empty slot has nothing to switch, and keeps the room.
    let buffer = track_page_buffer(&app);
    assert_eq!(buffer[(6, second)].symbol(), " ");
    assert_eq!(buffer[(7, second)].symbol(), " ");
    assert_eq!(buffer[(9, second)].symbol(), "E");

    // `b` flips it from anywhere in the slot, and back.
    press(&mut app, "b");
    assert!(app.song().tracks[0].bypassed[0]);
    assert_eq!(switch(&app), off);
    press(&mut app, "b");
    assert_eq!(switch(&app), on);

    // Playing, it shows what's heard, so a lock switches it out for its note and back in.
    let heard = |bypassed: bool| LiveValues {
        bypassed: [bypassed, false, false, false],
        ..LiveValues::default()
    };
    app.transport = Transport::Playing;
    app.live = Some(heard(true));
    assert_eq!(switch(&app), off, "a lock bypasses it");
    app.live = Some(heard(false));
    assert_eq!(switch(&app), on);
    press(&mut app, "b");
    assert_eq!(switch(&app), on, "a lock brings it in under b's bypass");
    // Stopped, it's the slot's own, whatever was last heard.
    app.transport = Transport::Stopped;
    assert_eq!(switch(&app), off);
}

/// The track page's cursor is a component of its own: given a track, `TrackPage` goes through a
/// rack's devices with no `App`, as the Sound and Mod pages' will once they give devices too.
#[test]
fn the_track_pages_cursor_goes_through_a_rack_on_its_own() {
    use vt_model::insert::Insert;
    use vt_model::selection::Motion;
    use vt_model::song::Track;
    use vt_tui::input::bindings::Cmd;
    use vt_tui::track_page::{Control, DeviceCursor, Outcome, Page, TrackPage};
    let effect = |name| {
        *Insert::ALL
            .iter()
            .find(|insert| insert.name() == name)
            .unwrap()
    };
    let mut track = Track::default();
    // An EQ, an empty slot, a Distort and another empty slot.
    track.inserts[0] = effect("EQ3");
    track.inserts[2] = effect("Distort");
    let track = Some(&track);
    let at = |device, row, column| {
        Some(DeviceCursor {
            device,
            row,
            column,
        })
    };
    // It opens on the Sound page, a rack of its own four, and `L` twice goes to FX.
    let mut page = TrackPage::default();
    assert_eq!(page.page, Page::Sound);
    assert_eq!(page.devices(track).len(), 4);
    page.key(Cmd::NextPage, 1, track);
    page.key(Cmd::NextPage, 1, track);
    assert_eq!(page.page, Page::Fx);
    assert_eq!(page.devices(track).len(), 4);
    assert_eq!(page.control(track), Control::Insert(0));
    assert_eq!(page.key(Cmd::Move(Motion::Down), 1, track), Outcome::Done);
    assert_eq!(page.device_cursor(track), at(0, 1, 0));
    // `}` keeps the row and column: an empty slot shows the cursor on its title, and the next
    // device has them again.
    page.key(Cmd::Move(Motion::NextBeat), 1, track);
    assert_eq!(page.device_cursor(track), at(1, 0, 0));
    page.key(Cmd::Move(Motion::NextBeat), 1, track);
    assert_eq!(page.device_cursor(track), at(2, 1, 0));
    // `j` and `k` pass an empty slot's grid by, from its title.
    page.key(Cmd::Move(Motion::Up), 2, track);
    assert_eq!(page.device_cursor(track), at(1, 0, 0));
    // The EQ's last row has a hole in the first column, so the cursor lands beside it.
    page.key(Cmd::Move(Motion::Up), 1, track);
    assert_eq!(page.device_cursor(track), at(0, 3, 1));
    // An edit isn't the page's own, and a return's Sound page isn't a rack.
    assert_eq!(page.key(Cmd::Increase, 1, track), Outcome::Passed);
    let fixed = Track {
        kind: vt_model::song::TrackKind::ReturnA,
        ..Track::default()
    };
    let fixed = Some(&fixed);
    page.key(Cmd::PreviousPage, 1, fixed);
    page.key(Cmd::PreviousPage, 1, fixed);
    assert_eq!(page.page, Page::Sound);
    assert_eq!(page.device_cursor(fixed), None);
    assert_eq!(
        page.key(Cmd::Move(Motion::NextBeat), 1, fixed),
        Outcome::Refused("{ and } go between devices, and this page has none")
    );
}
