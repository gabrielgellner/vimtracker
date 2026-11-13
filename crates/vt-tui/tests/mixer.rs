//! The mixer: its strips, meters and faders, its cursor, and editing tracks' settings from it.

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::{FIXED_TRACKS, MAX_GROUPS};
use vt_model::song::TrackKind;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

#[test]
fn the_mixer_leaves_out_what_a_fixed_track_doesnt_have() {
    let (mut app, _rt) = app();
    let return_a = 2 + MAX_GROUPS;
    press(&mut app, " 6");
    // Past the groups, which are empty and so folded away.
    press(&mut app, "ll");
    press(&mut app, "jj");
    assert_eq!(app.cursor.track, return_a);
    press(&mut app, "i40");
    assert_eq!(
        app.song().tracks[return_a].send_a,
        0,
        "a return sends nowhere"
    );
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, return_a + 2);
    press(&mut app, "ki40");
    assert_eq!(
        app.song().tracks[return_a + 2].pan,
        0x80,
        "the master has no pan"
    );
}

#[test]
fn the_mixers_cursor_stays_on_rows_a_track_has_and_comes_back() {
    use vt_tui::views::status::column_name;
    let (mut app, _rt) = app();
    // With the empty groups shown.
    press(&mut app, " 6zRjj");
    assert_eq!(column_name(&app), "Send A");
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, 2);
    assert_eq!(column_name(&app), "Send A", "a group sends");
    press(&mut app, &"l".repeat(MAX_GROUPS));
    assert_eq!(app.cursor.track, 2 + MAX_GROUPS);
    assert_eq!(
        column_name(&app),
        "Pan",
        "a return has no sends, so it's on the row above"
    );
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Mute", "and going down skips them");
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Pan");
    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Level", "the master only has its level");
    press(&mut app, "jG");
    assert_eq!(column_name(&app), "Level");
    press(&mut app, &"h".repeat(4 + MAX_GROUPS));
    assert_eq!(app.cursor.track, 0);
    assert_eq!(column_name(&app), "Level", "where it went is where it is");
    press(&mut app, "jjl");
    assert_eq!(column_name(&app), "Send A");
    press(&mut app, "lh");
    assert_eq!(column_name(&app), "Send A", "back on a track that has it");

    // `$` and `0` go to the last and first tracks that have the row.
    press(&mut app, "$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (1 + MAX_GROUPS, "Send A".to_owned()),
        "the last group"
    );
    press(&mut app, "0");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (0, "Send A".to_owned())
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (3 + MAX_GROUPS, "Pan".to_owned()),
        "B, as M has none"
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (4 + MAX_GROUPS, "Level".to_owned())
    );
}

fn mixer_text(app: &App) -> Vec<String> {
    mixer_text_at(app, 80)
}

/// The line of the mixer starting with a setting's name.
fn mixer_line(app: &App, name: &str) -> String {
    mixer_text(app)
        .into_iter()
        .find(|line| line.starts_with(name))
        .unwrap()
}

#[test]
fn the_mixer_edits_each_tracks_settings_as_the_track_page_does() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 6");
    assert_eq!(app.view, View::Mixer);
    let depth = app.history.undo_depth();
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].level, 0xE1);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].level, 0xE0);

    press(&mut app, "lj");
    assert_eq!(app.cursor.track, 1);
    press(&mut app, "i40");
    assert_eq!(app.song().tracks[1].pan, 0x40);
    assert_eq!(app.song().tracks[0].pan, 0x80, "only this track");
    press(&mut app, "x");
    assert_eq!(app.song().tracks[1].pan, 0x80);
    press(&mut app, "j+");
    assert_eq!(app.song().tracks[1].send_a, 0x10);
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Send B");
    press(&mut app, "Gk0");
    assert_eq!(app.cursor.track, 0);
    assert_eq!(column_name(&app), "Mute");
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 3 + MAX_GROUPS, "B, the last with a mute");
    press(&mut app, "gg");
    assert_eq!(
        column_name(&app),
        "Trim",
        "above the level, as on a console"
    );
}

#[test]
fn the_mixers_mute_and_solo_reach_the_engine() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 6Gk=");
    assert!(app.levels[0].mute);
    assert!(
        mixer_line(&app, "Mute").contains('M'),
        "{}",
        mixer_line(&app, "Mute")
    );
    press(&mut app, "=");
    assert!(app.levels[0].mute, "= only switches on");
    press(&mut app, " 3k;");
    assert_eq!(loudest(&mut realtime, 2000), 0.0, "muted");
    press(&mut app, " 6-");
    assert!(!app.levels[0].mute);
    press(&mut app, "K");
    assert!(app.levels[0].mute);
    press(&mut app, "x");
    assert!(!app.levels[0].mute);
    press(&mut app, "j=");
    assert!(app.levels[0].solo);
    assert!(mixer_line(&app, "Solo").contains(vt_tui::glyphs::SOLO));
    press(&mut app, "i");
    assert!(!app.levels[0].solo);
    assert_eq!(app.mode, Mode::Normal, "it switches rather than typing");
}

#[test]
fn a_long_mixer_scrolls_to_keep_the_track_in_view() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 30");
    command(&mut app, "name bd");
    press(&mut app, " 6");
    assert!(
        mixer_text(&app)[2].contains("01 BD"),
        "{}",
        mixer_text(&app)[2]
    );
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 31 + FIXED_TRACKS);
    let names = &mixer_text(&app)[2];
    assert!(names.contains("32"), "{names}");
    assert!(
        names.contains("\u{a0}A ") && names.contains("\u{a0}M"),
        "{names}"
    );
    assert!(!names.contains("01"), "{names}");
}

/// The colour a mixer's text is drawn in where some text starts on a row.
fn mixer_color(app: &App, row: usize, text: &str) -> ratatui::style::Color {
    let backend = ratatui::backend::TestBackend::new(160, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let line: String = (0..160).map(|x| buffer[(x, row as u16)].symbol()).collect();
    let column = line[..line.find(text).unwrap()].chars().count();
    buffer[(column as u16, row as u16)].fg
}

#[test]
fn a_groups_strip_names_it_and_its_tracks_and_its_tracks_are_marked() {
    use vt_tui::glyphs::{GROUP, ICON_SPACE};
    let (mut app, _realtime) = app();
    command(&mut app, "track add 4");
    command(&mut app, "group 1 1-3");
    command(&mut app, "group 2 5");
    press(&mut app, " 6");
    let text = mixer_text_at(&app, 160);
    let (names, routing) = (&text[2], &text[3]);
    // G1's strip has its icon and name, and under it the tracks in it.
    let g1 = format!("{GROUP}{ICON_SPACE}G1");
    assert!(names.contains(&g1), "{names}");
    let column = |line: &str, text: &str| line[..line.find(text).unwrap()].chars().count();
    let under = routing.chars().skip(column(names, &g1)).collect::<String>();
    assert!(under.starts_with("01-03"), "{routing}");
    // Each track in a group is marked with it, under its name, and one in none isn't.
    let marks: Vec<String> = routing.split_whitespace().map(String::from).collect();
    assert_eq!(marks, ["G1", "G1", "G1", "G2", "01-03", "05"], "{routing}");
    // A gap parts the sound tracks from the groups, and the groups from the returns.
    let strip = |name: &str| column(names, name);
    assert_eq!(strip(&g1) - strip("06"), 7 + 2, "{names}");
    // The empty groups are folded away, so the returns come after G2.
    let g3 = format!("{GROUP}{ICON_SPACE}G3");
    assert!(!names.contains(&g3), "{names}");
    assert_eq!(
        strip("\u{a0}A") - 1 - strip(&format!("{GROUP}{ICON_SPACE}G2")),
        7 + 2
    );
    // A group and its tracks share its colour, and an empty group is faint, once it's shown.
    let blue = vt_tui::theme::group(1);
    assert_eq!(mixer_color(&app, 2, &g1), blue);
    assert_eq!(mixer_color(&app, 3, "G1"), blue);
    assert_eq!(mixer_color(&app, 3, "G2"), vt_tui::theme::group(2));
    press(&mut app, "zR");
    assert_eq!(mixer_color(&app, 2, &g3), vt_tui::theme::OVERLAY0);
    let names = &mixer_text_at(&app, 160)[2];
    assert_eq!(
        column(names, "\u{a0}A") - 1 - column(names, &format!("{GROUP}{ICON_SPACE}G4")),
        7 + 2
    );
}

#[test]
fn the_mixer_names_its_parts_over_their_strips_with_the_cursors_brighter() {
    use ratatui::buffer::Buffer;
    use vt_tui::glyphs::{GROUP, ICON_SPACE};
    use vt_tui::theme::{OVERLAY2, SURFACE1, TEXT};
    let (mut app, _realtime) = app();
    command(&mut app, "track add 4");
    command(&mut app, "group 1 1-3");
    command(&mut app, "group 2 4");
    press(&mut app, " 6");
    // Drawn at the fullscreen kitty's size.
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(163, 42);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let line =
        |buffer: &Buffer, y: u16| -> String { (0..163).map(|x| buffer[(x, y)].symbol()).collect() };
    let column = |line: &str, text: &str| line[..line.find(text).unwrap()].chars().count();
    let buffer = draw(&app);
    let (title, parts, names) = (line(&buffer, 0), line(&buffer, 1), line(&buffer, 2));
    assert!(title.starts_with("Mixer"), "{title}");
    // Each part's name sits on a rule that starts over its first strip.
    let g1 = format!("{GROUP}{ICON_SPACE}G1");
    assert_eq!(column(&parts, "─ Tracks "), column(&names, "01"), "{parts}");
    assert_eq!(column(&parts, "─ Groups "), column(&names, &g1), "{parts}");
    assert_eq!(
        column(&parts, "─ Returns "),
        column(&names, "\u{a0}A") - 1,
        "{parts}"
    );
    // The rule runs over its strips, and stops a column before the gap to the next part.
    let tracks = &parts[parts.find("─ Tracks").unwrap()..parts.find("─ Groups").unwrap()];
    assert_eq!(tracks.chars().count(), 6 * 7 + 2, "{parts}");
    assert!(tracks.ends_with("─   "), "{parts}");
    // Nothing else moves: the strips' names are under it, and the settings at the bottom, with
    // `Trim` over `Level`.
    assert!(names.trim_start().starts_with("01"), "{names}");
    assert!(line(&buffer, 3).contains("G1"), "the routing");
    let rows: Vec<String> = (0..42).map(|y| line(&buffer, y)).collect();
    let trim = rows.iter().position(|row| row.starts_with("Trim")).unwrap();
    assert!(rows[trim + 1].starts_with("Level"));
    assert!(rows[41].starts_with("Solo"), "{}", rows[41]);
    // The cursor's part is brighter, its rule and its name.
    let colour = |buffer: &Buffer, text: &str, offset: usize| {
        let parts = line(buffer, 1);
        buffer[((column(&parts, text) + offset) as u16, 1)].fg
    };
    assert_eq!(colour(&buffer, "─ Tracks", 0), OVERLAY2);
    assert_eq!(colour(&buffer, "─ Tracks", 2), TEXT);
    assert_eq!(colour(&buffer, "─ Groups", 0), SURFACE1);
    assert_eq!(colour(&buffer, "─ Groups", 2), OVERLAY2, "dim");
    assert_eq!(colour(&buffer, "─ Returns", 0), SURFACE1);
    press(&mut app, "$");
    let buffer = draw(&app);
    assert_eq!(colour(&buffer, "─ Tracks", 0), SURFACE1);
    assert_eq!(colour(&buffer, "─ Tracks", 2), OVERLAY2);
    assert_eq!(colour(&buffer, "─ Returns", 0), OVERLAY2);
    assert_eq!(colour(&buffer, "─ Returns", 2), TEXT);
    // A lone group's strip is too narrow for the rule, so it has its name alone.
    let (mut app, _realtime) = self::app();
    command(&mut app, "track add 4");
    command(&mut app, "group 1 1-3");
    press(&mut app, " 6");
    let parts = line(&draw(&app), 1);
    assert!(parts.contains("──   Groups   ─ Returns"), "{parts}");
}

#[test]
fn a_tracks_mute_and_solo_from_its_group_show_apart_from_its_own() {
    use vt_tui::glyphs::{MUTE, SOLO};
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    command(&mut app, "group 1 1-2");
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, " 6");
    app.levels[group].mute = true;
    app.levels[2].mute = true;
    let mutes = mixer_line(&app, "Mute");
    let shown: Vec<&str> = mutes.split_whitespace().skip(1).collect();
    let mute = MUTE.to_string();
    assert_eq!(
        shown[..5],
        [&mute, &mute, &mute, "-", &mute],
        "01 to 04, and G1: {mutes}"
    );
    // The track's own in the active colour, its group's in the group's.
    let (blue, green) = (vt_tui::theme::group(1), vt_tui::theme::GREEN);
    assert_eq!(mixer_colors(&app, "Mute", MUTE), [blue, blue, green, green]);
    app.levels[group].mute = false;
    app.levels[group].solo = true;
    assert_eq!(mixer_colors(&app, "Solo", SOLO), [blue, blue, green]);
    assert!(mixer_colors(&app, "Mute", MUTE) == [green], "only 03's own");
}

/// The colour of each of a glyph on the mixer's row of a setting, from the left.
fn mixer_colors(app: &App, name: &str, glyph: char) -> Vec<ratatui::style::Color> {
    let backend = ratatui::backend::TestBackend::new(160, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = (0..16)
        .find(|&y| buffer[(0, y)].symbol() == &name[..1] && buffer[(1, y)].symbol() == &name[1..2])
        .unwrap();
    (0..160)
        .filter(|&x| buffer[(x, row)].symbol() == glyph.to_string())
        .map(|x| buffer[(x, row)].fg)
        .collect()
}

#[test]
fn the_mixers_meters_show_only_the_track_that_is_playing() {
    use vt_tui::meter::FLOOR_DB;
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 3k;");
    let _ = loudest(&mut realtime, 2000);
    app.tick();
    assert!(
        app.track_meters[0].top() > FLOOR_DB,
        "{}",
        app.track_meters[0].top()
    );
    assert_eq!(app.track_meters[1].top(), FLOOR_DB);
}

#[test]
fn the_mixer_draws_a_strip_per_track_with_a_meter_and_a_fader() {
    use vt_engine::mixer::Reading;
    let (mut app, _realtime) = app();
    let reading = Reading {
        before: [1.0, 0.0],
        after: [1.0, 0.0],
        power: [100.0, 0.0],
        frames: 100,
    };
    for _ in 0..50 {
        app.track_meters[0].update(Some(reading), 0.033);
    }
    command(&mut app, "track add 6");
    press(&mut app, " 6");
    let text = mixer_text(&app);
    assert!(text[2].starts_with("       01"), "{}", text[2]);
    assert!(text[2].contains("08"), "eight strips fit: {}", text[2]);
    // Under the names and the groups, the meter's left channel is full, and the right empty.
    let bars = &text[4..10];
    let left = |line: &String| line.chars().nth(7).unwrap();
    let right = |line: &String| line.chars().nth(8).unwrap();
    assert_eq!(left(&bars[bars.len() - 1]), '█', "{bars:?}");
    assert_eq!(right(&bars[bars.len() - 1]), ' ', "{bars:?}");
    // Every track has a fader, for a level of E0 here. The next track has only that.
    let fader = |line: &String, strip: usize| line.chars().nth(7 + strip * 7 + 3).unwrap();
    assert_ne!(fader(&bars[bars.len() - 1], 0), ' ');
    assert_ne!(fader(&bars[bars.len() - 1], 1), ' ');
    // Turning a fader down lowers only that strip's.
    let filled = |text: &[String], strip: usize| {
        text[4..10]
            .iter()
            .filter(|line| fader(line, strip) != ' ')
            .count()
    };
    let before = filled(&text, 0);
    press(&mut app, "x");
    press(&mut app, "i10");
    let text = mixer_text(&app);
    assert!(filled(&text, 0) < before, "{text:?}");
    assert_eq!(filled(&text, 1), before);
    assert!(mixer_line(&app, "Level").contains("10"));
}

#[test]
fn the_mixer_has_a_trim_above_each_fader_and_opens_on_the_fader() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 6");
    assert_eq!(column_name(&app), "Level");
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Trim");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].trim, 0x81);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.25 dB"));
    let text = mixer_text(&app);
    let trim = text
        .iter()
        .position(|line| line.starts_with("Trim"))
        .unwrap();
    assert!(text[trim + 1].starts_with("Level"), "{text:?}");
    assert!(text[trim].contains("81") && text[trim].contains("80"));
    // Every strip has one, the groups', the returns' and the master's too.
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 4 + MAX_GROUPS, "the master");
    assert_eq!(column_name(&app), "Trim");
}
