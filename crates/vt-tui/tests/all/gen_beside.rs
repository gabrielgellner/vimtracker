//! Gen beside the grid (decision 467): the side form, the grid's own columns at the left and a
//! 95-wide panel at the right at 163x42, with the meter, the overview, the row inspector and the
//! timeline hidden; `Ctrl-w` `h` `l` `w` `o` `v` `c` and `Esc` between them; the form kept for
//! the session. A text's buffer opens over the panel, under its cell, and the rows its token
//! writes are lit in the grid beside it (decision 470). With the panel open, `S` draws in its
//! place, `L` lands in the new device, `gl` moves the panel to the cursor's live column, and `gL`
//! opens it there (decision 469).

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::GenParam;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::{Mode, View};

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// Two tracks, the first's eight empty rows with a tresillo of notes kept live on them, then
/// Gen, which opens beside the grid.
fn beside_the_grid() -> (App, Realtime) {
    let mut song = Song::new(2);
    let empty: Step = E.parse().unwrap();
    for track in &mut song.tracks {
        track
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(vec![empty.clone(); 8])));
    }
    let (mut app, realtime) = app_for(song);
    keys(&mut app, "vGSjjjjjj===L");
    keys(&mut app, " 8");
    assert_eq!(app.modes.view, View::Gen);
    (app, realtime)
}

/// The characters of a line from one column to another, both counted in.
fn columns(line: &str, from: usize, to: usize) -> String {
    line.chars().skip(from).take(to + 1 - from).collect()
}

/// The meter's heading, which shows only while the meter does.
fn meter_shown(lines: &[String]) -> bool {
    lines.iter().any(|line| line.contains("     Main"))
}

#[test]
fn the_side_form_is_the_phrases_own_columns_at_0_to_66_and_the_panel_at_68_to_162() {
    let (mut app, _rt) = beside_the_grid();
    let lines = screen_cells(&mut app);
    let frames = &lines[1];
    assert_eq!(frames.chars().nth(8), Some('╭'), "{frames}");
    assert_eq!(
        frames.chars().nth(66),
        Some('╮'),
        "the grid ends at 66: {frames}"
    );
    assert_eq!(frames.chars().nth(67), Some(' '), "a gap at 67: {frames}");
    assert_eq!(
        frames.chars().nth(68),
        Some('╭'),
        "the panel starts at 68: {frames}"
    );
    assert_eq!(
        frames.chars().nth(162),
        Some('╮'),
        "and ends at 162: {frames}"
    );
    assert!(
        columns(frames, 68, 162).contains("Ramp  Euclid · Note 00–07"),
        "{frames}"
    );
    // The grid's rows end at its frame, with no other track's column beside them.
    let row = &lines[2];
    assert_eq!(row.chars().nth(66), Some('│'), "{row}");
    assert_eq!(row.chars().nth(67), Some(' '), "{row}");
    // The title names the grid on the left and Gen over the panel, the CPU figure at the right,
    // seen from the grid, as with the keys in a device its column's bars go over the title
    // (decision 468).
    keys(&mut app, "^wh");
    let lines = screen_cells(&mut app);
    let title = &lines[0];
    assert!(columns(title, 0, 66).contains("Track 01  Grid "), "{title}");
    assert!(!columns(title, 0, 66).contains("Gen"), "{title}");
    assert!(
        columns(title, 68, 100).starts_with(" Gen   Phrase 00"),
        "{title}"
    );
    assert!(title.contains("CPU"), "{title}");
    assert!(!meter_shown(&lines), "no meter beside the panel");
    // `Phrase`'s four columns take the four-column cells, so the fourth isn't cut.
    let text = lines.join("\n");
    assert!(text.contains("Order Y   Own"), "{text}");
    // The status line is the panel's while the keys are in it.
    keys(&mut app, "^wl");
    assert!(status_line(&app).contains("GEN"), "{}", status_line(&app));
}

/// Without the panel, the grid's title names the phrase view's two pages, as the track page's
/// names its four, and the overview and the meter are beside it.
#[test]
fn the_grid_alone_names_both_pages_and_has_its_overview_and_meter() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(!app.gen_beside_shown());
    let lines = screen_cells(&mut app);
    assert!(
        lines[0].contains("Track 01  Grid  Gen   Chain 00"),
        "{}",
        lines[0]
    );
    assert!(meter_shown(&lines));
    assert!(
        lines[1].chars().nth(68).is_some_and(|c| c != '╭'),
        "{}",
        lines[1]
    );
}

/// `Ctrl-w h` and `Esc` give the keys to the grid, the panel staying; grid keys work there as
/// they do; `Ctrl-w l` gives them back to the panel, and `Ctrl-w w` goes to the other one.
#[test]
fn ctrl_w_h_l_w_and_esc_move_the_keys_between_the_grid_and_the_panel() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wh");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(app.gen_beside_shown(), "the panel stays");
    assert_eq!(app.modes.mode, Mode::Normal);
    keys(&mut app, "jj");
    assert_eq!(app.services.cursor.step, 2, "the grid's keys");
    let lines = screen_cells(&mut app);
    assert_eq!(lines[1].chars().nth(68), Some('╭'), "{}", lines[1]);
    assert!(!meter_shown(&lines));
    // On another track, the panel shows that track's phrase, which has nothing live, on the
    // `Empty` for the cursor's column.
    keys(&mut app, "W");
    let other = screen_cells(&mut app).join("\n");
    assert!(other.contains("Empty · Note"), "{other}");
    keys(&mut app, "B^wl");
    assert_eq!(app.modes.view, View::Gen);
    assert_eq!(app.services.cursor.step, 2, "the grid's cursor stays put");
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.view, View::Phrase, "Esc goes back to the grid");
    assert!(app.gen_beside_shown());
    keys(&mut app, "^ww");
    assert_eq!(app.modes.view, View::Gen);
    keys(&mut app, "^ww");
    assert_eq!(app.modes.view, View::Phrase);
    // `Esc` in the grid stays in the grid.
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.view, View::Phrase);
    // `Esc` with a count waiting lets the count go, staying in the panel.
    keys(&mut app, "^wl3<Esc>");
    assert_eq!(app.modes.view, View::Gen);
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.view, View::Phrase);
}

/// The cursor's setting is lit only while the keys are in the panel.
#[test]
fn the_panels_cursor_shows_only_while_the_keys_are_in_it() {
    let (mut app, _rt) = beside_the_grid();
    let lit = |app: &mut App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
        terminal
            .draw(|frame| vt_tui::run::draw_screen(frame, app, None, std::time::Instant::now()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (68..163).any(|x| (1..40).any(|y| buffer[(x, y)].bg == vt_tui::widgets::theme::ROSEWATER))
    };
    assert!(lit(&mut app));
    keys(&mut app, "^wh");
    assert!(!lit(&mut app));
}

/// `Ctrl-w o` makes Gen the whole page, with its previews and the meter, and `Ctrl-w v` puts it
/// back beside the grid; the form is kept for the session, so `Space 8` opens it as it was.
#[test]
fn ctrl_w_o_and_v_change_the_form_and_space_8_keeps_the_last() {
    let (mut app, _rt) = beside_the_grid();
    assert!(app.gen_page.beside, "the side form is the default");
    keys(&mut app, "^wo");
    assert_eq!(app.modes.view, View::Gen);
    assert!(!app.gen_page.beside);
    assert!(!app.gen_beside_shown());
    let lines = screen_cells(&mut app);
    assert!(meter_shown(&lines));
    assert!(
        lines[0].contains("Track 01  Grid  Gen   Phrase 00"),
        "{}",
        lines[0]
    );
    assert_eq!(lines[2].chars().next(), Some('╭'), "{}", lines[2]);
    // To the grid and back: still the whole page.
    keys(&mut app, "H");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(!app.gen_beside_shown(), "no panel beside the grid");
    keys(&mut app, " 8");
    assert!(!app.gen_page.beside);
    assert!(!app.gen_beside_shown());
    // `Ctrl-w v` beside the grid again, the keys staying in Gen.
    keys(&mut app, "^wv");
    assert_eq!(app.modes.view, View::Gen);
    assert!(app.gen_page.beside);
    assert!(app.gen_beside_shown());
    keys(&mut app, " 2 8");
    assert!(
        app.gen_beside_shown(),
        "Space 8 opens it beside the grid again"
    );
    // `Space 3` goes to the grid with the panel still open.
    keys(&mut app, " 3");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(app.gen_beside_shown());
}

/// From the grid alone, `Ctrl-w v` opens the panel beside it, the keys staying in the grid, and
/// `Ctrl-w l` opens Gen in its form when it's closed.
#[test]
fn from_the_grid_ctrl_w_v_opens_the_panel_and_ctrl_w_l_opens_gen() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc");
    assert!(!app.gen_beside_shown());
    keys(&mut app, "^wv");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(app.gen_beside_shown());
    keys(&mut app, "^wc^wl");
    assert_eq!(app.modes.view, View::Gen);
    assert!(app.gen_beside_shown());
    // `Ctrl-w c` from the whole page closes it to the grid.
    keys(&mut app, "^wo^wc");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(!app.gen_beside_shown());
}

/// Beside the grid there are no previews to send, the grid's column being beside the panel;
/// the whole page has them.
#[test]
fn the_side_form_draws_no_previews() {
    use vt_tui::graphics::{Graphics, PanelKey};
    let (mut app, _rt) = beside_the_grid();
    let mut graphics = Graphics::new();
    app.send_pictures(&mut graphics, &mut Vec::new()).unwrap();
    assert_eq!(app.gen_page.images.get(PanelKey::Gen(0)), None);
    let lines = screen_cells(&mut app).join("\n");
    assert!(!lines.contains('▒'), "{lines}");
    keys(&mut app, "^wo");
    app.send_pictures(&mut graphics, &mut Vec::new()).unwrap();
    assert!(app.gen_page.images.get(PanelKey::Gen(0)).is_some());
    let lines = screen_cells(&mut app).join("\n");
    assert!(lines.contains('▒'), "{lines}");
}

/// The overview, the row inspector and the timeline hide while the panel is beside the grid, and
/// come back as they were when it closes.
#[test]
fn the_overview_inspector_and_timeline_come_back_when_the_panel_closes() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc i");
    assert!(app.phrase_view.inspector);
    assert!(app.phrase_view.timeline_on());
    let alone = screen_cells(&mut app);
    let overview = alone[2].chars().skip(67).collect::<String>();
    assert!(
        !overview.trim().is_empty(),
        "the other track's column: {}",
        alone[2]
    );
    keys(&mut app, "^wv");
    assert!(app.phrase_view.inspector, "kept");
    assert!(!app.phrase_view.inspector_on());
    assert!(!app.phrase_view.timeline_on());
    keys(&mut app, "zi");
    assert!(
        app.services.status.contains("once Gen's panel closes"),
        "{}",
        app.services.status
    );
    keys(&mut app, "zi^wc");
    assert!(app.phrase_view.inspector_on());
    assert!(app.phrase_view.timeline_on());
    // All but the message line.
    assert_eq!(screen_cells(&mut app)[..41], alone[..41]);
}

/// Sixteen empty rows with an `Own` rhythm of notes kept live on them, Gen beside the grid, and
/// the keys on the rhythm's `Text`, its buffer closed.
fn on_own_text() -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, realtime) = app_for(song);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>");
    down_to(&mut app, GenParam::Rhythm);
    keys(&mut app, "K/own<Enter>L 8");
    assert!(app.gen_beside_shown());
    'found: for _ in 0..12 {
        keys(&mut app, "0");
        for _ in 0..6 {
            if here(&app) == Some(GenParam::OwnText) {
                break 'found;
            }
            keys(&mut app, "l");
        }
        keys(&mut app, "j");
    }
    assert_eq!(here(&app), Some(GenParam::OwnText));
    (app, realtime)
}

/// The token buffer opens over the panel, under its cell, and never over the grid: its frame from
/// the panel's 68 to its 162 at most (decision 470 pins decision 467's placing).
#[test]
fn the_token_buffer_opens_over_the_panel() {
    let (mut app, _rt) = on_own_text();
    let cell = screen_cells(&mut app)
        .iter()
        .position(|line| columns(line, 68, 162).contains("Text      x . ."))
        .expect("the Text cell");
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.mode, Mode::Tokens);
    let lines = screen_cells(&mut app);
    let (y, x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| {
            line.find("╭ Text ")
                .map(|at| (y, line[..at].chars().count()))
        })
        .unwrap_or_else(|| panic!("no buffer: {}", lines.join("\n")));
    assert!(x >= 68, "at column {x}, over the grid: {}", lines[y]);
    assert_eq!(y, cell + 1, "under its cell: {}", lines.join("\n"));
    let right = x + lines[y].chars().skip(x).position(|c| c == '╮').unwrap();
    assert!(right <= 162, "to column {right}: {}", lines[y]);
    // The grid beside it is drawn whole, its frame and rows untouched.
    for line in &lines[1..18] {
        assert!(
            matches!(line.chars().nth(66), Some('╮' | '│' | '╯')),
            "{line}"
        );
    }
}

/// The rows of the grid beside the panel with a cell in the cursor's colour, bold, by their
/// numbers: those the buffer's token writes (decision 460).
fn lit_in_grid(app: &mut App) -> Vec<usize> {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, std::time::Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = drawn_lines(&buffer);
    (0..16)
        .filter(|row| {
            let number = format!("{row:02X}");
            let Some(y) = lines
                .iter()
                .position(|line| line.get(4..6) == Some(number.as_str()))
            else {
                return false;
            };
            (0..67).any(|x| {
                let cell = &buffer[(x, y as u16)];
                cell.fg == vt_tui::widgets::theme::ROSEWATER
                    && cell.modifier.contains(ratatui::style::Modifier::BOLD)
            })
        })
        .collect()
}

/// With the buffer open in the panel beside the grid, the rows its cursor's token writes are lit
/// in the grid, moving as the cursor moves along the text, and none once it closes (decision
/// 470).
#[test]
fn the_buffers_token_lights_its_rows_in_the_grid_beside_it() {
    let (mut app, _rt) = on_own_text();
    assert!(
        lit_in_grid(&mut app).is_empty(),
        "nothing lit while it's shut"
    );
    // `x . . x . . x . . . x . x . . .`: each `x` writes its own row.
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.mode, Mode::Tokens);
    keys(&mut app, "lll");
    assert_eq!(lit_in_grid(&mut app), vec![3]);
    keys(&mut app, "lll");
    assert_eq!(lit_in_grid(&mut app), vec![6]);
    // A rest writes nothing.
    keys(&mut app, "l");
    assert!(lit_in_grid(&mut app).is_empty());
    keys(&mut app, "lll");
    assert_eq!(lit_in_grid(&mut app), vec![10]);
    keys(&mut app, "<Enter>");
    assert_ne!(app.modes.mode, Mode::Tokens);
    assert!(lit_in_grid(&mut app).is_empty(), "none once it closes");
}

/// The live generator Gen's cursor is on, by its place in the phrase's list.
fn gen_device(app: &App) -> Option<usize> {
    match app.gen_page.control(app.services.track_here()) {
        Control::Gen { generator, .. } => Some(generator),
        _ => None,
    }
}

/// From the grid, beside the panel, `S` on four rows of Nudge, the column after Velocity.
fn shaping_nudge() -> (App, Realtime) {
    let (mut app, realtime) = beside_the_grid();
    keys(&mut app, "^whllggvjjjS");
    assert_eq!(app.modes.mode, Mode::Shape);
    (app, realtime)
}

/// `S` draws its box at the panel's top left, not over the grid, whose columns keep its bars.
#[test]
fn with_the_panel_open_s_draws_its_box_in_the_panels_place() {
    let (mut app, _rt) = shaping_nudge();
    let lines = screen_cells(&mut app);
    assert_eq!(lines[2].chars().nth(68), Some('╭'), "{}", lines[2]);
    let text = lines.join("\n");
    let hints = lines
        .iter()
        .find(|line| line.contains("Enter keep  L keep live"))
        .unwrap_or_else(|| panic!("no shape panel: {text}"));
    assert_eq!(hints.chars().nth(68), Some('╰'), "{hints}");
    // The grid's frame and the panel's both stand.
    assert_eq!(lines[1].chars().nth(66), Some('╮'), "{}", lines[1]);
    assert_eq!(lines[1].chars().nth(162), Some('╮'), "{}", lines[1]);
    assert!(lines[2].contains('█'), "the bars stay on the grid: {text}");
}

/// `L` keeps it live and lands in its device in the panel, the keys in it; `Enter` keeps the
/// values and leaves the keys in the grid.
#[test]
fn l_lands_in_the_new_device_and_enter_stays_in_the_grid() {
    let (mut app, _rt) = shaping_nudge();
    keys(&mut app, "L");
    assert_eq!(app.modes.view, View::Gen);
    assert!(app.gen_beside_shown());
    assert_eq!(gen_device(&app), Some(1), "{}", app.services.status);
    let (mut app, _rt) = shaping_nudge();
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(app.gen_beside_shown());
}

/// With the panel open, `gl` moves it to the cursor's live column's device, the keys staying in
/// the grid and no shape panel opening. Off a live column it says so.
#[test]
fn with_the_panel_open_gl_moves_it_to_the_columns_device() {
    let (mut app, _rt) = shaping_nudge();
    keys(&mut app, "L^wh0gl");
    assert_eq!(app.modes.view, View::Phrase);
    assert_eq!(app.modes.mode, Mode::Normal);
    assert!(app.overlays.shape.is_none());
    assert_eq!(gen_device(&app), Some(0));
    assert!(
        app.services.status.contains("Ctrl-w l"),
        "{}",
        app.services.status
    );
    keys(&mut app, "llgl");
    assert_eq!(gen_device(&app), Some(1));
    keys(&mut app, "lgl");
    assert!(
        app.services.status.starts_with("No live column here"),
        "{}",
        app.services.status
    );
    assert_eq!(gen_device(&app), Some(1));
    // Without the panel, `gl` opens the shape panel as it did.
    keys(&mut app, "^wc0gl");
    assert_eq!(app.modes.mode, Mode::Shape);
}

/// `gL` opens the side form on the live column's device, the keys in it, whichever form was last.
#[test]
fn gl_capital_opens_the_side_form_on_the_columns_device() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wo^wc");
    assert!(!app.gen_page.beside);
    keys(&mut app, "lgL");
    assert!(
        app.services.status.starts_with("No live column here"),
        "{}",
        app.services.status
    );
    assert_eq!(app.modes.view, View::Phrase);
    keys(&mut app, "0gL");
    assert_eq!(app.modes.view, View::Gen);
    assert!(app.gen_beside_shown());
    assert!(app.gen_page.beside);
    assert_eq!(gen_device(&app), Some(0));
}
