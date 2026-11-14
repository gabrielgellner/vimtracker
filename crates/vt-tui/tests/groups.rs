//! Groups: putting tracks in them, their colours, and folding them in the song view, the mixer
//! and the overview.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::link::Realtime;
use vt_model::song::TrackKind;
use vt_tui::app::App;
use vt_tui::input::keys::Key;

mod common;
use common::*;

#[test]
fn group_puts_tracks_in_a_group_or_none_as_one_undo_step() {
    let (mut app, _realtime) = app();
    let groups = |app: &App| {
        app.song().tracks[..4]
            .iter()
            .map(|track| track.group)
            .collect::<Vec<_>>()
    };
    command(&mut app, "track add 2");
    command(&mut app, "group 1 1-3");
    assert_eq!(groups(&app), [Some(1), Some(1), Some(1), None]);
    assert!(
        app.status.contains("Tracks 01-03 in group 1"),
        "{}",
        app.status
    );
    press(&mut app, "u");
    assert_eq!(groups(&app), [None; 4], "one undo takes them all out");
    app.key(
        Key {
            ctrl: true,
            ..Key::char('r')
        },
        Instant::now(),
    );
    assert_eq!(groups(&app), [Some(1), Some(1), Some(1), None]);

    command(&mut app, "group 2");
    assert_eq!(groups(&app)[0], Some(2), "the cursor's track");
    command(&mut app, "group off 2-3");
    assert_eq!(groups(&app), [Some(2), None, None, None]);
    command(&mut app, "group off");
    assert_eq!(groups(&app), [None; 4]);
    assert!(
        app.status.contains("Track 01 in no group"),
        "{}",
        app.status
    );

    let sounds = app.song().sound_tracks();
    command(&mut app, &format!("group 1 1-{}", sounds + 1));
    assert_eq!(
        groups(&app),
        [None; 4],
        "past the last track, nothing changes"
    );
    assert!(
        app.status.contains(&format!("{sounds} tracks")),
        "{}",
        app.status
    );
    command(&mut app, "group 5 1");
    assert!(app.status.contains(":group takes"), "{}", app.status);

    // A group can't join a group. The track picker reaches it, empty and folded away.
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, " fbG1");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, group);
    command(&mut app, "group 2");
    assert_eq!(app.song().tracks[group].group, None);
    assert!(app.status.contains("Only tracks"), "{}", app.status);
}

/// The colour of the first cell of some text on a line, from a column on.
fn color_from(buffer: &ratatui::buffer::Buffer, row: usize, from: usize, text: &str) -> Color {
    let line: String = drawn_lines(buffer)[row].chars().skip(from).collect();
    let column = from + line[..line.find(text).unwrap()].chars().count();
    buffer[(column as u16, row as u16)].fg
}

/// Eight tracks, 01 to 03 in group 1 and 05 in group 2.
fn grouped() -> (App, Realtime) {
    let (mut app, realtime) = app();
    command(&mut app, "track add 6");
    command(&mut app, "group 1 1-3");
    command(&mut app, "group 2 5");
    (app, realtime)
}

/// Where the phrase view's overview starts on its headings row: past the grid's last heading.
fn overview_start(headings: &str) -> usize {
    let last = headings.find("FX 3").unwrap();
    headings[..last].chars().count() + "FX 3".len()
}

#[test]
fn a_group_and_its_tracks_are_named_in_its_colour_where_tracks_are_listed() {
    let (mut app, _realtime) = grouped();
    let (blue, peach) = (vt_tui::theme::group(1), vt_tui::theme::group(2));
    // The song view's columns.
    press(&mut app, " 1");
    let song = drawn(&app, vt_tui::views::song::draw);
    assert_eq!(color_from(&song, 2, 0, "02"), blue);
    assert_eq!(color_from(&song, 2, 0, "05"), peach);
    assert_eq!(color_from(&song, 2, 0, "G1"), blue);
    assert_ne!(color_from(&song, 2, 0, "04"), blue);
    // The phrase view's overview, beside the grid.
    press(&mut app, " 3");
    let phrase = drawn(&app, vt_tui::views::phrase::draw);
    let title = drawn_lines(&phrase)[0].clone();
    let headings = drawn_lines(&phrase)[1].clone();
    let from = overview_start(&headings);
    assert_eq!(color_from(&phrase, 1, from, "02"), blue);
    assert_eq!(color_from(&phrase, 1, from, "05"), peach);
    assert_ne!(color_from(&phrase, 1, from, "04"), blue);
    // The titles of a track in a group carry its mark.
    assert!(title.contains("Track 01 G1 "), "{title}");
    let chain = drawn_lines(&drawn(&app, vt_tui::views::chain::draw));
    assert!(chain[0].contains("Track 01 G1  Chain 00"), "{}", chain[0]);
    let page = drawn_lines(&drawn(&app, vt_tui::views::track::draw));
    assert!(page[0].contains("Track 01 G1 "), "{}", page[0]);
    // The track picker.
    press(&mut app, " fb");
    let items = app.picker.as_ref().unwrap().items();
    assert_eq!(items[1].style.fg, Some(blue));
    assert_eq!(items[3].style.fg, None);
    assert_eq!(items[4].style.fg, Some(peach));
}

#[test]
fn z_a_folds_a_group_into_its_column_in_the_song_view_and_the_cursor_steps_over_it() {
    use vt_tui::glyphs::{FOLDED, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 1");
    let heading = |app: &App| song_text(app)[2].clone();
    // The empty groups are folded away, the others open.
    assert!(
        heading(&app).contains("08  G1     G2     A"),
        "{}",
        heading(&app)
    );
    // On a track in a group, `z a` folds the group, and the cursor stands on it.
    press(&mut app, "llll");
    assert_eq!(app.song_column, 3 + 1);
    press(&mut app, "za");
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    assert_eq!(app.song_column, 3 + g1);
    assert!(app.status.contains("G1 folded: 01-03"), "{}", app.status);
    let text = heading(&app);
    assert!(!text.contains("01") && !text.contains("03"), "{text}");
    assert!(
        text.contains(&format!("08  G1{FOLDED}{ICON_SPACE}   G2")),
        "{text}"
    );
    // The cursor steps over the folded tracks and the empty groups.
    press(&mut app, "0lll");
    assert_eq!(app.song_column, 3 + 3, "on 04");
    press(&mut app, "h");
    assert_eq!(app.song_column, 2, "on Start");
    press(&mut app, "$");
    assert_eq!(app.song_column, 3 + app.song().tracks.len() - 1);
    // `z a` on the group opens it again.
    press(&mut app, "hhhh");
    assert_eq!(app.song_column, 3 + g1);
    press(&mut app, "za");
    assert!(heading(&app).contains("02  03  04"), "{}", heading(&app));
    // `zR` opens every group, the empty ones too, and `zM` folds them all.
    press(&mut app, "zR");
    assert!(
        heading(&app).contains("G1     G2     G3     G4     A"),
        "{}",
        heading(&app)
    );
    press(&mut app, "lll");
    let g4 = g1 + 3;
    assert_eq!(app.song_column, 3 + g4);
    press(&mut app, "zM");
    assert_eq!(app.song_column, 3 + g4 + 1, "off the empty group, onto A");
    let text = heading(&app);
    assert!(text.contains(&format!("08  G1{FOLDED}")), "{text}");
    assert!(!text.contains("G3") && !text.contains("05"), "{text}");
    // `z a` on a track in no group says so.
    press(&mut app, "0lll");
    press(&mut app, "za");
    assert!(app.status.contains("isn't in a group"), "{}", app.status);
}

#[test]
fn the_mixer_folds_a_groups_strips_into_it() {
    use vt_tui::glyphs::{FOLDED, GROUP, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 4");
    let names = |app: &App| mixer_text_at(app, 160)[2].clone();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    // Folding from a track in it puts the cursor on its strip.
    press(&mut app, "lza");
    assert_eq!(app.cursor.track, g1);
    let text = names(&app);
    assert!(!text.contains("01 ") && !text.contains("03 "), "{text}");
    assert!(
        text.contains(&format!("{GROUP}{ICON_SPACE}G1{FOLDED}{ICON_SPACE}")),
        "{text}"
    );
    assert!(text.trim_start().starts_with("04"), "{text}");
    // `h`, `l`, `W`, `B`, `0` and `$` step over folded tracks.
    press(&mut app, "0");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "h");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "B");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "$");
    assert_eq!(app.cursor.track, app.song().tracks.len() - 1);
    press(&mut app, "0l");
    assert_eq!(app.cursor.track, 4);
    // A track put in a folded group goes into it, and the cursor stands on the group.
    command(&mut app, "group 1");
    assert_eq!(app.cursor.track, g1);
    assert!(!names(&app).contains("05 "), "{}", names(&app));
    // The track picker opens the fold of a track it goes to.
    press(&mut app, " fb02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, 1);
    assert!(names(&app).contains("02 "), "{}", names(&app));
}

#[test]
fn the_phrase_views_overview_shows_a_folded_group_as_a_column_of_its_own() {
    use vt_tui::glyphs::{FOLDED, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 4lza0");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, " 3");
    let headings = drawn_lines(&drawn(&app, vt_tui::views::phrase::draw))[1].clone();
    let overview: String = headings.chars().skip(overview_start(&headings)).collect();
    assert!(
        overview.contains(&format!(" G1{FOLDED}{ICON_SPACE}    04 ")),
        "{overview}"
    );
    assert!(!overview.contains("01"), "{overview}");
}

/// Each track `key` reaches from where the cursor is, one after another, until it stops.
fn stops(app: &mut App, key: &str) -> Vec<usize> {
    let mut reached = vec![app.cursor.track];
    loop {
        press(app, key);
        if reached.last() == Some(&app.cursor.track) {
            return reached;
        }
        reached.push(app.cursor.track);
    }
}

#[test]
fn w_and_b_make_a_folded_group_one_stop_in_its_tracks_place() {
    let (mut app, _realtime) = grouped();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    let (g2, a) = (g1 + 1, g1 + 4);
    // `z a` in the phrase view, from a track in the group, folds it and stands on it.
    press(&mut app, " 3W");
    assert_eq!(app.cursor.track, 1);
    press(&mut app, "za");
    assert_eq!(app.cursor.track, g1);
    assert!(app.folds.hides(app.song(), 1));
    assert!(app.status.contains("G1 folded: 01-03"), "{}", app.status);
    // The group comes where its tracks would, then the track after them, and not again later.
    let forward = [g1, 3, 4, 5, 6, 7, g2, a, a + 1, a + 2];
    assert_eq!(stops(&mut app, "W"), forward);
    let mut back = forward;
    back.reverse();
    assert_eq!(stops(&mut app, "B"), back);
    // The same on the track pages, where the group's own page is the stop.
    press(&mut app, " 5W");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "B");
    assert_eq!(app.cursor.track, g1);
    let title = track_page_text(&app)[0].clone();
    assert!(title.contains("Group 1"), "{title}");
    assert_eq!(stops(&mut app, "]b"), forward);
    press(&mut app, "[b");
    assert_eq!(app.cursor.track, a + 1);
    // `z a` on the group opens it, and every track is a stop again.
    press(&mut app, " fbG1");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "za");
    assert!(app.status.contains("G1 open"), "{}", app.status);
    assert_eq!(app.cursor.track, g1);
    press(&mut app, " fb01");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        stops(&mut app, "W"),
        [0, 1, 2, 3, 4, 5, 6, 7, g1, g2, a, a + 1, a + 2]
    );
    // `zM` and `zR` work on the track pages too.
    press(&mut app, "zM");
    assert!(app.folds.hides(app.song(), 0) && app.folds.hides(app.song(), 4));
    press(&mut app, "zR");
    assert!(
        !app.folds.hides(app.song(), g1 + 2),
        "G3 shown, though empty"
    );
}

#[test]
fn a_track_folded_in_another_view_stays_in_front_until_the_cursor_leaves_it() {
    let (mut app, _realtime) = grouped();
    press(&mut app, " 3W");
    assert_eq!(app.cursor.track, 1);
    // Folded in the song view, whose column is Length, the phrase view keeps the track.
    press(&mut app, " 10zM 3j");
    assert!(app.folds.hides(app.song(), 1));
    assert_eq!(app.cursor.track, 1);
    // `W` goes on past the group.
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 3);
}

#[test]
fn bracket_g_goes_to_the_next_or_previous_group_folded_or_open() {
    let (mut app, _realtime) = grouped();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    let g2 = g1 + 1;
    // Open, the groups are after the tracks.
    press(&mut app, " 3]g");
    assert_eq!(app.cursor.track, g1);
    press(&mut app, "]g");
    assert_eq!(app.cursor.track, g2);
    press(&mut app, "]g");
    assert_eq!(app.cursor.track, g2, "G3 and G4 are folded away");
    assert!(app.status.contains("No group after"), "{}", app.status);
    press(&mut app, "[g");
    assert_eq!(app.cursor.track, g1);
    // Folded, a group is where its tracks are.
    press(&mut app, "zaWW");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "[g");
    assert_eq!(app.cursor.track, g1);
    press(&mut app, "]g");
    assert_eq!(app.cursor.track, g2);
    press(&mut app, "2[g");
    assert_eq!(app.cursor.track, g2, "there's only one before it");
    // The mixer goes by its strips, where the group stays after the tracks.
    press(&mut app, " 40");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "]g");
    assert_eq!(app.cursor.track, g1);
    // The song view moves its column.
    press(&mut app, " 102]g");
    assert_eq!(app.song_column, 3 + g2);
}

#[test]
fn folds_are_saved_with_the_song_and_undo_leaves_them() {
    let (mut app, _realtime) = grouped();
    let path = folder();
    let saved = || std::fs::read_to_string(path.join("song.ron")).unwrap();
    command(&mut app, &format!("w {}", path.display()));
    assert!(!saved().contains("view:"), "no view until a fold is set");
    command(&mut app, "group 2 4");
    command(&mut app, "w");
    assert!(!app.dirty);
    press(&mut app, " 4lza");
    assert!(app.folds.hides(app.song(), 0));
    assert!(app.dirty, "a fold is kept with the song");
    // Undo takes back the change before it, and leaves the fold.
    press(&mut app, "u");
    assert_eq!(app.song().tracks[3].group, None, "{}", app.status);
    assert!(app.folds.hides(app.song(), 0));
    app.key(
        Key {
            ctrl: true,
            ..Key::char('r')
        },
        Instant::now(),
    );
    assert_eq!(app.song().tracks[3].group, Some(2));
    assert!(app.folds.hides(app.song(), 0));
    command(&mut app, "w");
    let text = saved();
    assert!(text.contains("view:") && text.contains("folded:"), "{text}");
    assert!(!text.contains("opened:"), "{text}");
    // Opened again, the project has its folds, and undo still can't open them.
    command(&mut app, "group off 1-3");
    command(&mut app, &format!("e! {}", path.display()));
    assert!(app.folds.hides(app.song(), 0) && !app.folds.hides(app.song(), 4));
    let g3 = app.song().fixed_track(TrackKind::Group(3)).unwrap();
    assert!(app.folds.hides(app.song(), g3), "an empty group still is");
    assert!(!app.dirty);
    press(&mut app, "u");
    assert!(app.folds.hides(app.song(), 0));
    // An opened group is kept too, so the empty groups `zR` showed come back with it.
    press(&mut app, " 1zR");
    command(&mut app, "w");
    command(&mut app, &format!("e {}", path.display()));
    assert!(!app.folds.hides(app.song(), g3));
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn group_name_names_the_cursors_group_with_up_to_five_characters() {
    let (mut app, _realtime) = grouped();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    command(&mut app, "group name drums");
    assert_eq!(app.song().tracks[g1].name, "DRUMS", "from a track in it");
    assert!(app.status.contains("Group 1 named DRUMS"), "{}", app.status);
    command(&mut app, "group name drumss");
    assert!(
        app.status
            .contains(":group name takes 1 to 5 letters or digits"),
        "{}",
        app.status
    );
    assert_eq!(app.song().tracks[g1].name, "DRUMS", "six are refused");
    app.cursor.track = g1;
    command(&mut app, "group name");
    assert_eq!(
        app.song().tracks[g1].name,
        "G1",
        "on the group, back to its letters"
    );
    press(&mut app, "u");
    assert_eq!(app.song().tracks[g1].name, "DRUMS", "each is an undo step");
    app.cursor.track = 3;
    command(&mut app, "group name perc");
    assert!(app.status.contains("in no group"), "{}", app.status);
}

#[test]
fn a_group_with_five_characters_shows_its_whole_name_in_every_view() {
    use vt_tui::glyphs::{FOLDED, GROUP, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    command(&mut app, "group name drums");
    let mixer = |app: &App| mixer_text_at(app, 160)[2].clone();
    let song = |app: &App| drawn_lines(&drawn(app, vt_tui::views::song::draw))[2].clone();

    // Open, the song view's column holds it with a gap, and the mixer's strip leaves out the
    // group's icon to fit it.
    assert!(song(&app).contains("08  DRUMS  G2     A"), "{}", song(&app));
    assert!(mixer(&app).contains(" DRUMS "), "{}", mixer(&app));
    assert!(
        !mixer(&app).contains(&format!("{GROUP}{ICON_SPACE}DRUMS")),
        "{}",
        mixer(&app)
    );

    // Folded, both have room for the mark beside it.
    app.fold_group(1, true);
    assert!(
        song(&app).contains(&format!("08  DRUMS{FOLDED}{ICON_SPACE}G2")),
        "{}",
        song(&app)
    );
    assert!(
        mixer(&app).contains(&format!("DRUMS{FOLDED}{ICON_SPACE}")),
        "{}",
        mixer(&app)
    );

    // The phrase view's overview gives the folded group a column wide enough.
    app.cursor.track = 3;
    press(&mut app, " 3");
    let headings = drawn_lines(&drawn(&app, vt_tui::views::phrase::draw))[1].clone();
    let overview: String = headings.chars().skip(overview_start(&headings)).collect();
    assert!(
        overview.contains(&format!(" DRUMS{FOLDED}{ICON_SPACE} 04 ")),
        "{overview}"
    );

    // The track page's title, the track picker and the Roto's track list.
    app.cursor.track = g1;
    let page = drawn_lines(&drawn(&app, vt_tui::views::track::draw))[0].clone();
    assert!(page.contains("Group 1 DRUMS"), "{page}");
    let roto = vt_tui::roto::devices::live_tracks(app.song());
    assert_eq!(roto[g1].name.as_str(), "G1 DRUMS");
    assert_eq!(roto[g1 + 1].name.as_str(), "Group 2", "an unnamed group");
    press(&mut app, " fb");
    let items = app.picker.as_ref().unwrap().items();
    assert!(
        items.iter().any(|item| item.label.ends_with("DRUMS")),
        "{:?}",
        items.iter().map(|item| &item.label).collect::<Vec<_>>()
    );
}
