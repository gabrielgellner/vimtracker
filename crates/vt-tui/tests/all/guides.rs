//! The guides, `Space ?`, and the list of keys, `Space k`.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_model::step::Note;
use vt_tui::input::mode::Mode;
use vt_tui::overlays::help::{POPUP_DELAY, draw_popup};

use crate::common::*;

#[test]
fn space_question_opens_guides_and_space_k_lists_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    assert_eq!(app.modes.mode, Mode::Picker);
    press(&mut app, "note");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.mode, Mode::Guide);
    assert_eq!(
        app.overlays.guide.open.map(|g| g.title()),
        Some("Note entry")
    );
    press(&mut app, "q");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(app.overlays.guide.open, None);
    press(&mut app, " k");
    assert_eq!(app.modes.mode, Mode::Help);
}

/// The guide under the list's cursor as `Space ?` opens it, from where the keys before it went.
fn guide_opened_from(keys: &str) -> String {
    let (mut app, _realtime) = app();
    press(&mut app, keys);
    press(&mut app, " ?");
    assert_eq!(app.modes.mode, Mode::Picker, "from {keys:?}");
    let picker = app.overlays.picker.as_ref().unwrap();
    picker.current().unwrap().label.clone()
}

#[test]
fn space_question_opens_on_the_guide_for_the_page_showing() {
    assert_eq!(guide_opened_from(""), "Navigation");
    assert_eq!(guide_opened_from(" 2"), "Phrases and chains");
    assert_eq!(guide_opened_from(" 1"), "Phrases and chains");
    assert_eq!(
        guide_opened_from(" 4"),
        "The Sound page, output and groups",
        "the mixer is in the Sound page's guide"
    );
    assert_eq!(guide_opened_from(" 5"), "The Sound page, output and groups");
    assert_eq!(guide_opened_from(" 6"), "The Mod page, envelopes and LFOs");
    assert_eq!(guide_opened_from(" 7"), "The FX page and its effects");
    assert_eq!(guide_opened_from(" 8"), "Gen, the Macros page and presets");
    assert_eq!(
        guide_opened_from(" 7L"),
        "Gen, the Macros page and presets",
        "the Macros page is in Gen's guide"
    );
    assert_eq!(guide_opened_from(" 9"), "Saving and rendering");
    assert_eq!(
        guide_opened_from(" 9L"),
        "The meter",
        "the audio page is in its guide"
    );
    assert_eq!(guide_opened_from(" 0"), "The sample editor");
}

#[test]
fn question_in_a_guide_goes_back_to_the_list_on_that_guide() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?gg");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.overlays.guide.open.map(|g| g.title()),
        Some("Your first loop")
    );
    press(&mut app, "?");
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "Your first loop");
}

#[test]
fn a_guide_stops_scrolling_at_its_end() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    special(&mut app, KeyCode::Enter);
    app.overlays.guide.page = 10;
    let last = app
        .overlays
        .guide
        .open
        .unwrap()
        .lines(app.overlays.guide.width)
        .len()
        - 10;
    press(&mut app, "G");
    assert_eq!(app.overlays.guide.scroll, last);
    press(&mut app, "j");
    assert_eq!(app.overlays.guide.scroll, last);
    press(&mut app, "g");
    assert_eq!(app.overlays.guide.scroll, 0);
    press(&mut app, "?");
    assert_eq!(app.modes.mode, Mode::Picker);
    assert_eq!(
        app.overlays.picker.as_ref().map(|p| p.query.as_str()),
        Some("")
    );
}

/// What the note entry and navigation guides promise.
#[test]
fn the_guides_moves_do_what_they_say() {
    let (mut app, _realtime) = app();
    press(&mut app, "4j");
    assert_eq!(app.services.cursor.step, 4);
    press(&mut app, "}");
    assert_eq!(app.services.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "2}");
    assert_eq!(app.services.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "8G");
    assert_eq!(app.services.cursor.step, 7);
    press(&mut app, "gg");

    press(&mut app, "i");
    press(&mut app, "a");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ji");
    assert_eq!(app.modes.mode, Mode::Insert);
    assert_eq!(
        app.services.cursor.step, 1,
        "the note stayed put, and j went on one"
    );
    special(&mut app, KeyCode::Esc);

    press(&mut app, "gg");
    press(&mut app, "i;");
    tap(&mut app, "aaaa");
    special(&mut app, KeyCode::Esc);
    let written: Vec<usize> = notes(&app)
        .iter()
        .enumerate()
        .filter(|(_, note)| matches!(note, Note::On(_)))
        .map(|(step, _)| step)
        .collect();
    assert_eq!(written, [0, 4, 8, 12]);

    press(&mut app, "i;");
    assert!(!app.services.live.auto_jump);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    let before = app.services.cursor.step;
    press(&mut app, "ia");
    assert_eq!(app.services.cursor.step, before);
}

#[test]
fn the_space_popup_is_wide_enough_for_every_description() {
    let (mut app, _realtime) = app();
    press(&mut app, " ");
    let later = Instant::now() + Duration::from_secs(1);
    let rows = app.popup_rows(later, POPUP_DELAY);
    assert!(rows.iter().any(|(keys, _)| keys == "i"), "{rows:?}");
    let (width, height) = (163, 42);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| draw_popup(frame, frame.area(), &app, later))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let screen: Vec<String> = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    for (keys, description) in rows {
        assert!(
            screen.iter().any(|line| line.contains(description)),
            "{keys} {description} is cut off:\n{}",
            screen.join("\n")
        );
    }
}

#[test]
fn the_keys_of_help_and_of_a_guide_are_in_the_binding_table() {
    let (app, _realtime) = app();
    let help = app.help_rows();
    for (mode, row) in [
        ("Help", "Scroll down"),
        ("Guide", "The list of guides"),
        ("Shape", "The guide, at the fill's section"),
    ] {
        let at = help.iter().position(|line| line == mode).unwrap();
        let rows = help[at + 1..].iter().take_while(|line| !line.is_empty());
        assert!(rows.clone().any(|line| line.contains(row)), "{mode}");
    }
}

/// The heading at the top of the open guide.
fn guide_top(app: &vt_tui::app::App) -> String {
    let view = &app.overlays.guide;
    let line = &view.open.unwrap().lines(view.width)[view.scroll];
    line.spans.iter().map(|span| &*span.content).collect()
}

#[test]
fn space_question_in_the_shape_panel_opens_at_the_fills_section() {
    for fill in ["Random", "Grids", "Acid", "Tonnetz"] {
        let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 8]);
        keys(&mut app, &format!("vGSK/{}<Enter>", fill.to_lowercase()));
        let kind = app.overlays.shape.as_ref().unwrap().shape.kind;
        assert_eq!(kind.name(), fill);
        keys(&mut app, " ?");
        assert_eq!(app.modes.mode, Mode::Guide);
        assert_eq!(
            app.overlays.guide.open.map(|g| g.title()),
            Some("Generators in depth")
        );
        assert_eq!(guide_top(&app), fill);
        assert_eq!(app.services.status, "");
    }
}

#[test]
fn a_fill_without_a_section_opens_at_shaping_a_column() {
    let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 8]);
    keys(&mut app, "vGS");
    let kind = app.overlays.shape.as_ref().unwrap().shape.kind;
    assert_eq!(kind.name(), "Ramp");
    keys(&mut app, " ?");
    assert_eq!(
        app.overlays.guide.open.map(|g| g.title()),
        Some("Selecting and editing")
    );
    assert_eq!(guide_top(&app), "Shaping a column");
    assert_eq!(
        app.services.status,
        "Ramp has no section of its own: Shaping a column"
    );
}

#[test]
fn closing_the_fills_guide_comes_back_to_the_shape_panel_as_it_was() {
    let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 8]);
    keys(&mut app, "vGSK/random<Enter>jj=r");
    let panel = format!("{:?}", app.overlays.shape);
    let drawn = shape_panel(&app, 52, 16);
    let song = app.services.history.current().clone();
    keys(&mut app, " ?jj<Esc>");
    assert_eq!(app.modes.mode, Mode::Shape);
    assert_eq!(format!("{:?}", app.overlays.shape), panel);
    assert_eq!(shape_panel(&app, 52, 16), drawn);
    assert_eq!(app.services.history.current(), &song, "the preview stays");
    // `?` in the guide, then `Esc` from the list, comes back to it too.
    keys(&mut app, " ??<Esc>");
    assert_eq!(app.modes.mode, Mode::Shape);
    assert_eq!(format!("{:?}", app.overlays.shape), panel);
}

/// Where `text` writes a key with a modifier as Vim does, `D-l` for `Cmd-l` or `C-o` for
/// `Ctrl-o`. A note such as `C-3` or `D-1` has a digit after its dash, so it isn't one.
fn vim_modifiers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len().saturating_sub(2))
        .filter(|&at| {
            matches!(chars[at], 'D' | 'C' | 'M' | 'S')
                && chars[at + 1] == '-'
                && chars[at + 2].is_ascii_alphabetic()
                && (at == 0 || !chars[at - 1].is_alphabetic())
        })
        .map(|at| {
            chars[at.saturating_sub(10)..(at + 12).min(chars.len())]
                .iter()
                .collect()
        })
        .collect()
}

#[test]
fn keys_are_shown_as_cmd_ctrl_option_and_shift_never_as_vim_writes_them() {
    let (app, _realtime) = app();
    let help = app.help_rows().join("\n");
    assert_eq!(vim_modifiers(&help), Vec::<String>::new(), "Space k");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/guides");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let guide = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            vim_modifiers(&guide),
            Vec::<String>::new(),
            "{}",
            path.display()
        );
    }
    // The popup, the status line and the view picker show keys as a `Key` writes itself.
    for binding in vt_tui::input::bindings::bindings() {
        let shown = vt_tui::input::keys::shown(binding.keys);
        assert_eq!(
            vim_modifiers(&shown),
            Vec::<String>::new(),
            "{}",
            binding.keys
        );
    }
}

#[test]
fn space_slash_searches_every_guides_text_and_opens_where_it_found_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " /");
    assert_eq!(app.modes.mode, Mode::Picker);
    // Each word exactly, in any order, whatever its case: a sentence of Editing's.
    press(&mut app, "PATTERN buffer");
    let picker = app.overlays.picker.as_ref().unwrap();
    assert!(!picker.rows().is_empty());
    assert!(
        picker.shown().all(|item| {
            let text = item.label.to_lowercase();
            // A word may be in its heading or its guide's title, which a search looks through too.
            let around = format!("{} {}", item.meaning, item.category).to_lowercase();
            (text.contains("pattern") || around.contains("pattern")) && text.contains("buffer")
        }),
        "only what has both words"
    );
    let first = picker.current().unwrap().label.clone();
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.mode, Mode::Guide);
    let view = &app.overlays.guide;
    let guide = view.open.expect("a guide");
    assert_eq!(view.found, ["pattern", "buffer"]);
    // The passage found is on the guide's top line, or as near as the guide's end lets it be.
    let lines: Vec<String> = guide
        .lines(view.width)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    let first_word = first.split_whitespace().next().unwrap();
    assert!(
        lines[view.scroll..(view.scroll + view.page).min(lines.len())]
            .iter()
            .any(|line| line.contains(first_word)),
        "{first} not in view"
    );
    // `n` goes on to the next it found in this guide, and round.
    let at = app.overlays.guide.scroll;
    press(&mut app, "n");
    assert_ne!(app.overlays.guide.scroll, at);
    press(&mut app, "N");
    assert_eq!(app.overlays.guide.scroll, at);
}

#[test]
fn a_loose_spelling_finds_nothing_in_the_guides_as_grep_would_not() {
    let (mut app, _realtime) = app();
    press(&mut app, " /");
    press(&mut app, "ptrnbfr");
    assert_eq!(app.overlays.picker.as_ref().unwrap().rows().len(), 0);
}

#[test]
fn n_with_no_search_says_how_to_search() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    press(&mut app, "note");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "n");
    assert!(
        app.services.status.contains("/ searches"),
        "{}",
        app.services.status
    );
}
