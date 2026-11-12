//! The Mod page, `Space 4` then `L`: a rack of half-width devices, the six mod slots down two
//! columns, `Mod 1` to `Mod 3` on the left beside `Mod 4` to `Mod 6` on the right, and the
//! accumulator beside the arpeggio under them. Each slot's type, mode and destination are in its
//! title. The cursor moves across the two columns with `h` and `l`, and down each with `j`, `k`,
//! `{` and `}`. The settings themselves are in `track_page.rs`.

use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Realtime;
use vt_engine::track_state::{LfoNow, LiveValues};
use vt_model::action::Action;
use vt_model::lfo::{Fade, Lfo, LfoMode, Rate, Shape, wave};
use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot, Polarity};
use vt_model::selection::Motion;
use vt_model::settings::SettingId;
use vt_model::song::{Track, TrackKind};
use vt_model::steps::{Loop, StepRate, Steps, StepsMode};
use vt_tui::app::App;
use vt_tui::input::bindings::Cmd;
use vt_tui::input::mode::Mode;
use vt_tui::kitty::{DEFAULT_CELL, Graphics, dot_pixels};
use vt_tui::screenshot::sent_placements;
use vt_tui::track_page::{
    AccumulatorField, ArpField, Control, DeviceCursor, ModField, Page, TrackPage,
};

/// How many LFOs a new track's slots have, its last three.
const LFOS: usize = 3;

mod common;
use common::*;

/// Every setting a new track's mod slots have under their types, the three ADSRs' and the three
/// LFOs', then the accumulator's and the arpeggio's.
fn every_mod_setting(sound: bool) -> Vec<Control> {
    use AccumulatorField as Acc;
    let mut all = Vec::new();
    for slot in 0..MOD_SLOTS - LFOS {
        for field in [
            ModField::Type,
            ModField::Mode,
            ModField::Polarity,
            ModField::Destination,
            ModField::Attack,
            ModField::Decay,
            ModField::Sustain,
            ModField::Release,
            ModField::Depth,
            ModField::Reset,
        ] {
            all.push(Control::Mod(slot, field));
        }
    }
    for slot in MOD_SLOTS - LFOS..MOD_SLOTS {
        for field in [
            ModField::Type,
            ModField::Mode,
            ModField::Polarity,
            ModField::Destination,
            ModField::Shape,
            ModField::Rate,
            ModField::Depth,
            ModField::Phase,
            ModField::Fade,
            ModField::Deform,
        ] {
            all.push(Control::Mod(slot, field));
        }
    }
    if sound {
        for field in [
            Acc::Up,
            Acc::Down,
            Acc::Moves,
            Acc::AtLimit,
            Acc::Adds,
            Acc::Reset,
        ] {
            all.push(Control::Accumulator(field));
        }
        for field in [
            ArpField::Rate,
            ArpField::Mode,
            ArpField::Octaves,
            ArpField::Retrig,
        ] {
            all.push(Control::Arpeggio(field));
        }
    }
    all
}

/// The cursor's moves on the Mod page, with no `App`: given a track, from `Mod 1`'s type.
fn mod_page() -> TrackPage {
    let mut page = TrackPage::default();
    page.key(Cmd::NextPage, 1, Some(&Track::default()));
    assert_eq!(page.page, Page::Mod);
    page
}

/// Press each of some keys on the page, as `TrackPage::key` has them.
fn keys(page: &mut TrackPage, track: &Track, keys: &[(Cmd, usize)]) {
    for (cmd, times) in keys {
        page.key(*cmd, *times, Some(track));
    }
}

fn at(device: usize, row: usize, column: usize) -> Option<DeviceCursor> {
    Some(DeviceCursor {
        device,
        row,
        column,
    })
}

const DOWN: Cmd = Cmd::Move(Motion::Down);
const UP: Cmd = Cmd::Move(Motion::Up);
const LEFT: Cmd = Cmd::Move(Motion::Left);
const RIGHT: Cmd = Cmd::Move(Motion::Right);
const NEXT: Cmd = Cmd::Move(Motion::NextBeat);
const PREVIOUS: Cmd = Cmd::Move(Motion::PreviousBeat);

#[test]
fn every_modulator_setting_is_on_the_mod_page_once_and_the_keys_reach_it() {
    for (track, sound) in [
        (Track::default(), true),
        (
            Track {
                kind: TrackKind::ReturnA,
                ..Track::default()
            },
            false,
        ),
    ] {
        let wanted = every_mod_setting(sound);
        let placed: Vec<Control> = Page::Mod
            .devices(&track)
            .iter()
            .flat_map(|device| device.rows().flatten().flatten().collect::<Vec<_>>())
            .collect();
        assert_eq!(placed.len(), wanted.len(), "{placed:?}");
        for control in &wanted {
            assert!(placed.contains(control), "{control:?} isn't on the page");
        }
        // Walk the page with nothing but the keys: down each column of devices with `j`, and
        // along every row with `l`, starting from each column's top.
        let mut page = mod_page();
        page.key(Cmd::Move(Motion::First), 1, Some(&track));
        let mut reached = Vec::new();
        for column in 0..2 {
            page.key(Cmd::Move(Motion::First), 1, Some(&track));
            if column == 1 {
                // Off the end of `Mod 1`'s title into `Mod 4`'s.
                page.key(Cmd::Move(Motion::RowEnd), 1, Some(&track));
                page.key(RIGHT, 1, Some(&track));
            }
            loop {
                let row = page.place(Some(&track)).0;
                page.key(Cmd::Move(Motion::RowStart), 1, Some(&track));
                for _ in 0..4 {
                    reached.push(page.control(Some(&track)));
                    let before = page.device_cursor(Some(&track)).unwrap();
                    page.key(RIGHT, 1, Some(&track));
                    if page.device_cursor(Some(&track)).unwrap().device != before.device {
                        // Off the row's end into the device beside: back to where it was.
                        page.key(LEFT, 1, Some(&track));
                        break;
                    }
                }
                page.key(DOWN, 1, Some(&track));
                if page.place(Some(&track)).0 == row {
                    break;
                }
            }
        }
        for control in &wanted {
            assert!(
                reached.contains(control),
                "the keys don't reach {control:?}"
            );
        }
    }
}

#[test]
fn h_and_l_cross_between_the_columns_on_the_same_row() {
    let track = Track::default();
    let mut page = mod_page();
    let cursor = |page: &TrackPage| page.device_cursor(Some(&track));
    // The page reads `Mod 1`, `Mod 4`, `Mod 2`, … so `Mod 4` is the second device.
    assert_eq!(cursor(&page), at(0, 0, 0));
    assert_eq!(page.control(Some(&track)), Control::Mod(0, ModField::Type));
    // An ADSR's title is its type, its curve, its polarity and its destination, and `l` goes
    // along it, then into `Mod 4`'s title, whose type is first; `h` comes back to the last of
    // `Mod 1`'s.
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Mod(0, ModField::Mode));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(
        page.control(Some(&track)),
        Control::Mod(0, ModField::Polarity)
    );
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(
        page.control(Some(&track)),
        Control::Mod(0, ModField::Destination)
    );
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(cursor(&page), at(1, 0, 0));
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Type));
    page.key(LEFT, 1, Some(&track));
    assert_eq!(cursor(&page), at(0, 0, 3));
    // On `Sustain`, the last of `Mod 1`'s first row, `l` goes to `Shape`, the first of `Mod 4`'s,
    // and `h` comes back to the last.
    keys(&mut page, &track, &[(DOWN, 1)]);
    assert_eq!(
        page.control(Some(&track)),
        Control::Mod(0, ModField::Sustain)
    );
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(cursor(&page), at(1, 1, 0));
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Shape));
    page.key(LEFT, 1, Some(&track));
    assert_eq!(
        page.control(Some(&track)),
        Control::Mod(0, ModField::Sustain)
    );
    // A count goes on across: `3l` from `Attack` is `Sustain`, `Shape`, then `Phase`.
    keys(&mut page, &track, &[(LEFT, 1), (RIGHT, 3)]);
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Phase));
    // Nothing is right of the LFOs, so `l` stays, and `$` and `0` keep to the device.
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Phase));
    page.key(Cmd::Move(Motion::RowStart), 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Shape));
    // An LFO's last row has `Depth` and `Deform`, so `l` from `Depth` goes to `Deform`, and
    // from `Mod 1`'s `Reset` it lands on `Depth`.
    keys(&mut page, &track, &[(DOWN, 2), (RIGHT, 1)]);
    assert_eq!(
        page.control(Some(&track)),
        Control::Mod(3, ModField::Deform)
    );
    keys(&mut page, &track, &[(LEFT, 2)]);
    assert_eq!(page.control(Some(&track)), Control::Mod(0, ModField::Reset));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Mod(3, ModField::Depth));
    // From the accumulator's last row, the arpeggio has nothing level with it, so it's its last.
    keys(&mut page, &track, &[(LEFT, 2), (NEXT, 3)]);
    assert_eq!(
        page.control(Some(&track)),
        Control::Accumulator(AccumulatorField::Moves)
    );
    keys(&mut page, &track, &[(RIGHT, 2)]);
    assert_eq!(
        page.control(Some(&track)),
        Control::Arpeggio(ArpField::Mode)
    );
}

#[test]
fn a_slots_title_is_its_type_its_mode_its_polarity_then_its_destination_and_h_and_l_go_along_it() {
    let track = Track::default();
    let mut page = mod_page();
    let control = |page: &TrackPage| page.control(Some(&track));
    // An LFO has a mode after its type, then its polarity and its destination.
    keys(&mut page, &track, &[(RIGHT, 4)]);
    assert_eq!(control(&page), Control::Mod(3, ModField::Type));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(3, ModField::Mode));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(3, ModField::Polarity));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(3, ModField::Destination));
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(
        control(&page),
        Control::Mod(3, ModField::Destination),
        "the end"
    );
    page.key(Cmd::Move(Motion::RowStart), 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(3, ModField::Type));
    // `k` from a grid's left column is its type, and from its right column the title's second,
    // its mode: an ADSR's curve, and an LFO's mode.
    keys(&mut page, &track, &[(DOWN, 1), (UP, 1)]);
    assert_eq!(control(&page), Control::Mod(3, ModField::Type));
    keys(&mut page, &track, &[(DOWN, 1), (RIGHT, 1), (UP, 1)]);
    assert_eq!(control(&page), Control::Mod(3, ModField::Mode));
    let mut page = mod_page();
    keys(&mut page, &track, &[(DOWN, 1), (RIGHT, 1), (UP, 1)]);
    assert_eq!(control(&page), Control::Mod(0, ModField::Mode));
}

#[test]
fn braces_j_and_k_go_down_and_up_each_column_of_devices() {
    let track = Track::default();
    let mut page = mod_page();
    let control = |page: &TrackPage| page.control(Some(&track));
    // `}` goes down the left column, keeping the row and column, and stops at the bottom.
    keys(&mut page, &track, &[(DOWN, 2), (RIGHT, 1)]);
    assert_eq!(control(&page), Control::Mod(0, ModField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(1, ModField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(2, ModField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Accumulator(AccumulatorField::Adds));
    page.key(NEXT, 5, Some(&track));
    assert_eq!(control(&page), Control::Accumulator(AccumulatorField::Adds));
    page.key(PREVIOUS, 9, Some(&track));
    assert_eq!(control(&page), Control::Mod(0, ModField::Depth));
    // `3}` goes from `Mod 4` to the arpeggio, down the right column.
    keys(
        &mut page,
        &track,
        &[(Cmd::Move(Motion::First), 1), (RIGHT, 3)],
    );
    assert_eq!(control(&page), Control::Mod(3, ModField::Type));
    page.key(NEXT, 3, Some(&track));
    // The arpeggio's title has nothing in it, so the cursor is on its first row.
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Rate));
    assert_eq!(page.device_cursor(Some(&track)), at(7, 1, 0));
    // `k` goes up into `Mod 6`'s last row, past the arpeggio's empty title, and `j` back down.
    page.key(UP, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(5, ModField::Depth));
    page.key(DOWN, 1, Some(&track));
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Rate));
    // `j` stops at the bottom of its column, rather than going on into the other.
    page.key(DOWN, 9, Some(&track));
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Mode));
    // `gg` and `G` are the top and the bottom of the cursor's column.
    page.key(Cmd::Move(Motion::First), 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(3, ModField::Type));
    page.key(Cmd::Move(Motion::Last), 1, Some(&track));
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Mode));
    keys(
        &mut page,
        &track,
        &[(Cmd::Move(Motion::First), 1), (LEFT, 1)],
    );
    page.key(Cmd::Move(Motion::Last), 1, Some(&track));
    assert_eq!(
        control(&page),
        Control::Accumulator(AccumulatorField::Reset)
    );
    // `j` from an envelope's last row is the title of the one below, and `k` back.
    keys(
        &mut page,
        &track,
        &[(Cmd::Move(Motion::First), 1), (DOWN, 4)],
    );
    assert_eq!(control(&page), Control::Mod(1, ModField::Destination));
    page.key(UP, 1, Some(&track));
    assert_eq!(control(&page), Control::Mod(0, ModField::Reset));

    // A return has no accumulator or arpeggio, so its columns end with `Mod 3` and `Mod 6`.
    let fixed = Track {
        kind: TrackKind::ReturnA,
        ..Track::default()
    };
    let mut page = mod_page();
    page.key(NEXT, 9, Some(&fixed));
    assert_eq!(page.control(Some(&fixed)), Control::Mod(2, ModField::Type));
    page.key(DOWN, 9, Some(&fixed));
    assert_eq!(
        page.control(Some(&fixed)),
        Control::Mod(2, ModField::Release)
    );
}

#[test]
fn the_keys_move_across_the_mod_page_in_the_app() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Type));
    press(&mut app, "$l}");
    assert_eq!(app.track_control(), Control::Mod(4, ModField::Type));
    press(&mut app, "jh");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Sustain));
    press(&mut app, "2{");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Sustain));
    // `{` and `}` aren't refused here, as they are off a rack.
    assert!(!app.status.contains("go between"), "{}", app.status);
}

#[test]
fn each_slots_type_mode_and_destination_are_in_its_title() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let title = |app: &App, slot: usize, on: bool, controls: &str| {
        let identity = format!("Mod {slot}");
        let found = device_title(app, &identity);
        let wanted = modulator_title(&identity, on, controls);
        assert!(found.starts_with(&wanted), "{found} against {wanted}");
    };
    title(&app, 1, true, "ADSR  Exp  --  → Vol ─");
    title(&app, 2, true, "ADSR  Exp  Uni  → Cut ─");
    title(&app, 3, true, "ADSR  Exp  Uni  → --- ─");
    for slot in 4..=6 {
        title(&app, slot, true, "LFO  Free  Bi  → --- ─");
    }
    // The accumulator and the arpeggio have no destination, and no room for a switch.
    assert!(device_title(&app, "Accumulator").starts_with("╭─ Accumulator ───"));
    assert!(device_title(&app, "Arpeggio").starts_with("╭─ Arpeggio ───"));

    // `K` on it picks it, and the title says what it is.
    press(&mut app, &format!("{}$K/pan", to_lfo(2)));
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[5].destination,
        Some(SettingId::PAN)
    );
    title(&app, 6, true, "LFO  Free  Bi  → Pan ─");
    // `k` from the grid's first row reaches it.
    press(&mut app, "jkx");
    assert_eq!(app.song().tracks[0].mods[5].destination, None);
    title(&app, 6, true, "LFO  Free  Bi  → --- ─");

    // Going nowhere, it's dim, as an empty slot's title is, its arrow too; set, it's the
    // value's own colour. `Mod 1`'s title is on the page's third row, its arrow 28 in after its
    // curve and its polarity's `--`, and `Mod 3`'s two bands down, its arrow 29 in after its `Uni`.
    let buffer = track_page_buffer(&app);
    let dim = vt_tui::theme::dim().fg.unwrap();
    assert_eq!(buffer[(28, 2)].symbol(), "→");
    assert_eq!(buffer[(30, 2)].symbol(), "V");
    assert_ne!(buffer[(30, 2)].fg, dim, "Mod 1's Vol");
    let below = 2 + 9 * 2;
    assert_eq!(buffer[(29, below)].symbol(), "→");
    assert_eq!(buffer[(31, below)].symbol(), "-");
    assert_eq!(buffer[(31, below)].fg, dim, "Mod 3's ---");
    assert_eq!(buffer[(29, below)].fg, dim, "and its arrow");
}

#[test]
fn the_mod_page_fills_the_keys_box_at_163_by_42() {
    let (mut app, _realtime) = app();
    app.mod_images = Some([
        0x01_02_03, 0x04_05_06, 0x07_08_09, 0x0A_0B_0C, 0x0D_0E_0F, 0x10_11_12,
    ]);
    press(&mut app, " 4L");
    for keys_mode in [false, true] {
        if keys_mode {
            app.mode = Mode::Keys;
        }
        let lines = screen_cells(&mut app);
        let text = lines.join("\n");
        // Four bands of two devices, each 9 rows, from the row under the page's title.
        let first = if keys_mode { 3 } else { 2 };
        let left = usize::from(keys_mode);
        let width = if keys_mode { 73 } else { 74 };
        let pairs = [
            ("Mod 1", "Mod 4"),
            ("Mod 2", "Mod 5"),
            ("Mod 3", "Mod 6"),
            ("Accumulator", "Arpeggio"),
        ];
        for (band, (on_left, on_right)) in pairs.into_iter().enumerate() {
            let top = first + 9 * band;
            assert_eq!(device_at(&lines, on_left), (top, left), "{text}");
            assert_eq!(
                device_at(&lines, on_right),
                (top, left + width + 2),
                "{text}"
            );
            for column in [left, left + width + 2] {
                let corner = |row: usize, at: usize| lines[row].chars().nth(at).unwrap();
                assert_eq!(corner(top + 8, column), '╰', "{text}");
                assert_eq!(corner(top + 8, column + width - 1), '╯', "{text}");
            }
        }
        // The last band ends above the status line, and inside the Keys box's bottom edge.
        let bottom = first + 9 * 4 - 1;
        assert!(bottom < 40, "{text}");
        if keys_mode {
            assert!(lines[bottom + 1].starts_with('╰'), "{text}");
        }
        // Each envelope's shape and each LFO's cycle is in its panel at the half size, 24 by 5,
        // against the device's right side, and the settings beside it stop before it, with room
        // between.
        for band in 0..3 {
            let top = first + 9 * band;
            for (column, setting) in [(left, "Sustain"), (left + width + 2, "Phase")] {
                let device: Vec<String> = lines[top..top + 9]
                    .iter()
                    .map(|line| from_column(line, column).chars().take(width).collect())
                    .collect();
                let row = &device[2];
                let image = row.find('▒').map(|at| row[..at].chars().count()).unwrap();
                // Its frame's last column, 2 of room, the box's edge, then the picture's 24.
                assert_eq!(image, width - 1 - 2 - 1 - 24, "{row}");
                assert!(row.contains(setting), "{row}");
                let before: String = row.chars().take(image - 1).collect();
                assert!(before.ends_with("  "), "{row}");
                // Five rows of it, in its box on the device's inside rows.
                let rows: Vec<usize> = (0..9)
                    .filter(|at| device[*at].chars().nth(image) == Some('▒'))
                    .collect();
                assert_eq!(rows, [2, 3, 4, 5, 6], "{}", device.join("\n"));
                assert_eq!(device[1].chars().nth(image - 1), Some('┌'), "{text}");
                assert_eq!(device[7].chars().nth(image + 24), Some('┘'), "{text}");
                let pictured = device[4].chars().filter(|cell| *cell == '▒').count();
                assert_eq!(pictured, 24, "{}", device[4]);
                // And the terminal is told where each LFO's is, for its dot, and of no
                // envelope's.
                if setting == "Phase" {
                    let at = ((column + image) as u16, (top + 2) as u16);
                    assert_eq!(app.lfo_cycles_at[band + LFOS], Some(at), "{text}");
                } else {
                    assert_eq!(app.lfo_cycles_at[band], None, "{text}");
                }
            }
        }
    }
}

/// The depth a slot's grid shows, by the slot's number, as the screen has it.
fn depth_shown(app: &mut App, slot: usize) -> String {
    let lines = screen_cells(app);
    let (top, left) = device_at(&lines, &format!("Mod {slot}"));
    // Under its title, which can name another slot's depth.
    lines[top + 1..top + 9]
        .iter()
        .map(|line| from_column(line, left).chars().take(44).collect::<String>())
        .find_map(|row| {
            let after = row.split("Depth").nth(1)?;
            Some(after.split_whitespace().next()?.to_owned())
        })
        .unwrap_or_else(|| panic!("Mod {slot}'s depth"))
}

#[test]
fn the_default_mod_page_shows_six_slots_as_three_envelopes_beside_three_lfos() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let lines = screen_cells(&mut app);
    let text = lines.join("\n");
    let switch = vt_tui::views::widgets::toggle::glyph(true);
    for (slot, controls) in [
        (1, "ADSR  Exp  --  → Vol"),
        (2, "ADSR  Exp  Uni  → Cut"),
        (3, "ADSR  Exp  Uni  → ---"),
        (4, "LFO  Free  Bi  → ---"),
        (5, "LFO  Free  Bi  → ---"),
        (6, "LFO  Free  Bi  → ---"),
    ] {
        let wanted = format!("╭─ Mod {slot}  {switch}  {controls} ─");
        assert_eq!(text.matches(&wanted).count(), 1, "{wanted} in\n{text}");
    }
    // The note's own shape has no depth to set, and the filter's envelope has one.
    assert_eq!(depth_shown(&mut app, 1), "--");
    assert_eq!(depth_shown(&mut app, 2), "00");
}

#[test]
fn the_mod_page_goes_by_whole_bands_where_fewer_fit() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let draw = |app: &App| {
        // Room for two bands of devices under the page's title.
        let (width, height) = (150, 2 + 9 * 2 + 4);
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
    };
    let titles = |lines: &[String]| {
        lines
            .iter()
            .filter(|line| line.starts_with("╭─ "))
            .map(|line| line.chars().skip(3).take(5).collect::<String>())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&draw(&app)), ["Mod 1", "Mod 2"]);
    // On the arpeggio, the page has gone down to keep its band in view, as low as it can be.
    press(&mut app, "$l3}");
    assert_eq!(app.track_control(), Control::Arpeggio(ArpField::Rate));
    let lines = draw(&app);
    assert_eq!(titles(&lines), ["Mod 3", "Accum"]);
    // Only whole devices: each top has its bottom under it.
    for (row, line) in lines.iter().enumerate() {
        if line.starts_with('╭') {
            assert!(lines[row + 8].starts_with('╰'), "{}", lines.join("\n"));
        }
    }
    // Back up, it follows the cursor.
    press(&mut app, "gg");
    assert_eq!(titles(&draw(&app)), ["Mod 1", "Mod 2"]);
}

#[test]
fn b_switches_a_modulator_out_and_back_with_its_toggle_in_the_title() {
    use vt_tui::glyphs::{SWITCH_OFF, SWITCH_ON};
    use vt_tui::theme;
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let lfo = |app: &App| app.song().tracks[0].mods[3];
    // Every slot is in, with its switch green.
    let buffer = track_page_buffer(&app);
    let (env_switch, lfo_switch) = ((10, 2), (86, 2));
    assert_eq!(glyph_at(&buffer, env_switch).0, SWITCH_ON);
    assert_eq!(buffer[env_switch].fg, theme::GREEN);
    assert_eq!(glyph_at(&buffer, lfo_switch).0, SWITCH_ON);
    let name_colour = buffer[(79, 4)].fg;

    // `b` from any row of Mod 4, here its Rate, switches it out, as one undo step.
    press(&mut app, &format!("{}jj", to_lfo(0)));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Rate));
    let depth = app.history.undo_depth();
    press(&mut app, "b");
    assert!(lfo(&app).bypassed);
    assert!(app.status.contains("Switched Mod 4 out"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert!(device_title(&app, "Mod 4").starts_with(&modulator_title(
        "Mod 4",
        false,
        "LFO  Free  Bi  → --- ─"
    )));
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "ADSR  Exp  --  → Vol ─"
    )));

    // Its switch is faint, and so is all in it but the cursor: its title and its settings.
    let buffer = track_page_buffer(&app);
    assert_eq!(glyph_at(&buffer, lfo_switch).0, SWITCH_OFF);
    assert_eq!(buffer[lfo_switch].fg, theme::OVERLAY0);
    assert_eq!(buffer[(89, 2)].symbol(), "L");
    assert_eq!(buffer[(89, 2)].fg, theme::OVERLAY0);
    assert_eq!(buffer[(106, 2)].symbol(), "-");
    assert_eq!(buffer[(106, 2)].fg, theme::OVERLAY0);
    assert_eq!(buffer[(79, 4)].symbol(), "S");
    assert_eq!(buffer[(79, 4)].fg, theme::OVERLAY0);
    assert_eq!(buffer[(89, 6)].bg, theme::ROSEWATER, "the cursor");
    // The envelope beside it isn't.
    assert_eq!(buffer[(3, 4)].symbol(), "A");
    assert_eq!(buffer[(3, 4)].fg, name_colour);
    assert_eq!(buffer[env_switch].fg, theme::GREEN);

    // Again brings it back, and undo takes it back out.
    press(&mut app, "b");
    assert!(!lfo(&app).bypassed);
    assert!(app.status.contains("Mod 4 is back in"), "{}", app.status);
    assert_eq!(track_page_buffer(&app)[(79, 4)].fg, name_colour);
    press(&mut app, "u");
    assert!(lfo(&app).bypassed);

    // An envelope's flips the same way, from its title too.
    press(&mut app, "gghb");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Destination));
    assert!(app.song().tracks[0].mods[0].bypassed);
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        false,
        "ADSR  Exp  --  → Vol ─"
    )));
    assert!(lfo(&app).bypassed, "and only it");
}

#[test]
fn b_refuses_what_has_no_switch() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let before = app.song().tracks[0].clone();
    let depth = app.history.undo_depth();
    // The arpeggio, at the bottom of the LFOs' column, has nothing to switch.
    press(&mut app, &format!("{}3}}b", to_lfo(0)));
    assert_eq!(app.track_control(), Control::Arpeggio(ArpField::Rate));
    assert!(app.status.contains("no switch"), "{}", app.status);
    // Nor has the Sound page.
    press(&mut app, "Hb");
    assert_eq!(app.track_page.page, Page::Sound);
    assert!(app.status.contains("FX and Mod pages"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth);
    assert_eq!(app.song().tracks[0], before);
}

#[test]
fn k_on_a_slots_type_changes_it_keeping_where_it_goes_and_each_types_settings() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let slot = |app: &App| app.song().tracks[0].mods[1];
    // Mod 2, the filter's envelope, given a longer decay and depth, and switched out.
    press(&mut app, "}jj=");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Decay));
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "b");
    let adsr = slot(&app).adsr;
    assert_eq!(adsr.decay_ms, 225);
    assert_eq!(slot(&app).depth, 0x40);
    // `K` on its type lists the types, each with its meaning, opening on the one it is.
    press(&mut app, "kkh");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Type));
    press(&mut app, "K");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Type));
    let picker = app.picker.as_ref().expect("the type picker");
    assert_eq!(picker.title, "Mod 2 type");
    let depth = app.history.undo_depth();
    press(&mut app, "/lfo");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Lfo);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    // It keeps where it goes, how far and its switch, and its LFO is a new one's.
    assert_eq!(
        (
            slot(&app).destination,
            slot(&app).depth,
            slot(&app).bypassed
        ),
        (Some(SettingId::CUT), 0x40, true)
    );
    assert_eq!(slot(&app).lfo, Lfo::default());
    assert!(app.status.contains("Mod 2 is LFO"), "{}", app.status);
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        false,
        "LFO  Free  Bi  → Cut ─"
    )));
    // Its LFO's phase set, and back to an ADSR, which finds its envelope as it was.
    press(&mut app, "jl=");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Phase));
    let phase = slot(&app).lfo.phase;
    assert_ne!(phase, 0);
    press(&mut app, "hk");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Type));
    let depth = app.history.undo_depth();
    press(&mut app, "K");
    press(&mut app, "/adsr");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert_eq!(slot(&app).kind, ModKind::Adsr);
    assert_eq!(slot(&app).adsr, adsr);
    assert_eq!(slot(&app).depth, 0x40);
    // And again an LFO, finding its phase.
    press(&mut app, "K/lfo");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Lfo);
    assert_eq!(slot(&app).lfo.phase, phase);
    // Undo goes back a type at a time.
    press(&mut app, "u");
    assert_eq!(slot(&app).kind, ModKind::Adsr);
}

#[test]
fn equals_on_a_type_refuses_and_x_puts_back_the_slots_own() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let before = app.song().tracks[0].clone();
    let depth = app.history.undo_depth();
    // `=` and `-` don't move a type, as it changes what the device is.
    press(&mut app, "=");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Type));
    assert!(
        app.status.contains("K picks Mod 1's type"),
        "{}",
        app.status
    );
    press(&mut app, "-");
    assert_eq!(app.song().tracks[0], before);
    assert_eq!(app.history.undo_depth(), depth);
    // Mod 4, made an ADSR, goes back to an LFO with `x`, keeping where it goes.
    press(&mut app, &format!("{}$K/cut", to_lfo(0)));
    special(&mut app, KeyCode::Enter);
    press(&mut app, "0K");
    press(&mut app, "/adsr");
    special(&mut app, KeyCode::Enter);
    let slot = |app: &App| app.song().tracks[0].mods[3];
    assert_eq!(slot(&app).kind, ModKind::Adsr);
    press(&mut app, "x");
    assert_eq!(slot(&app).kind, ModKind::Lfo);
    assert_eq!(slot(&app).destination, Some(SettingId::CUT));
    assert_eq!(slot(&app).lfo, Lfo::default());
}

#[test]
fn k_on_an_lfos_mode_sets_whether_notes_start_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    press(&mut app, &format!("{}l", to_lfo(0)));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Mode));
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().title, "Mod 4 mode");
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].mods[3].lfo.mode, LfoMode::Trig);
    assert!(device_title(&app, "Mod 4").starts_with(&modulator_title(
        "Mod 4",
        true,
        "LFO  Trig  Bi  → --- ─"
    )));
    // `=` and `-` go through the modes, as any list does: the Digitakt's five.
    press(&mut app, "-");
    assert_eq!(app.song().tracks[0].mods[3].lfo.mode, LfoMode::Free);
    for mode in [LfoMode::Trig, LfoMode::Hold, LfoMode::One, LfoMode::Half] {
        press(&mut app, "=");
        assert_eq!(app.song().tracks[0].mods[3].lfo.mode, mode);
    }
    assert!(device_title(&app, "Mod 4").starts_with(&modulator_title(
        "Mod 4",
        true,
        "LFO  Half  Bi  → --- ─"
    )));
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].mods[3].lfo.mode, LfoMode::Free);
}

#[test]
fn an_lfos_fade_is_under_its_phase_as_a_signed_length() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    press(&mut app, &format!("{}jlj", to_lfo(0)));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Fade));
    let fade = |app: &App| app.song().tracks[0].mods[3].lfo.fade;
    assert_eq!(fade(&app), Fade::Off);
    // `=` fades in over the shortest length, `-` out, and `+` goes by straight lengths.
    press(&mut app, "=");
    assert_eq!(fade(&app), Fade::In(60));
    press(&mut app, "--");
    assert_eq!(fade(&app), Fade::Out(60));
    press(&mut app, "x+++");
    assert_eq!(fade(&app), Fade::In(240));
    // `i` types one, signed, and `K` lists them all, out, none and in.
    press(&mut app, "i-1 bar");
    special(&mut app, KeyCode::Enter);
    assert_eq!(fade(&app), Fade::Out(3840));
    press(&mut app, "itoo long");
    special(&mut app, KeyCode::Enter);
    assert_eq!(fade(&app), Fade::Out(3840));
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "LFO fade");
    assert_eq!(picker.items().len(), Fade::ALL.len());
    press(&mut app, "/+2 bars");
    special(&mut app, KeyCode::Enter);
    assert_eq!(fade(&app), Fade::In(7680));
    press(&mut app, "x");
    assert_eq!(fade(&app), Fade::Off);
}

#[test]
fn an_envelopes_depth_is_a_destination_of_the_envelope_before_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    // Mod 1's destination list has Mod 2's depth, an envelope's, after the LFOs' settings.
    press(&mut app, "$K/M2 Depth");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[0].destination,
        SettingId::modulator(1, 0)
    );
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "ADSR  Exp  Uni  → M2 Depth ─"
    )));
    // Off `Vol`, it no longer shapes the notes, so its depth is its own.
    assert_eq!(depth_shown(&mut app, 1), "00");
    // Mod 4's list has no envelope's depth, as they come before it.
    press(&mut app, &format!("gg{}$K/M2 Depth", to_lfo(0)));
    special(&mut app, KeyCode::Enter);
    assert_ne!(
        app.song().tracks[0].mods[3].destination,
        SettingId::modulator(1, 0)
    );
}

#[test]
fn a_switched_out_envelopes_shape_is_sent_faint_and_its_own_comes_back() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    press(&mut app, " 4L");
    graphics.sync(&mut app, &mut Vec::new()).unwrap();
    let shown = app.mod_images.unwrap();
    press(&mut app, "b");
    graphics.sync(&mut app, &mut Vec::new()).unwrap();
    let faint = app.mod_images.unwrap();
    assert_ne!(faint[0], shown[0], "a faint picture of its shape");
    assert_eq!(faint[1..], shown[1..], "the others' stay");
    // Back in, the picture it had is still held, so it's placed again.
    press(&mut app, "b");
    graphics.sync(&mut app, &mut Vec::new()).unwrap();
    assert_eq!(app.mod_images.unwrap(), shown);
}

#[test]
fn a_slots_picture_follows_its_type() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    press(&mut app, " 4L");
    graphics.sync(&mut app, &mut Vec::new()).unwrap();
    let shown = app.mod_images.unwrap();
    // Mod 3, made an LFO, has a cycle where its envelope's shape was, and its cycle is placed.
    press(&mut app, "}}K");
    press(&mut app, "/lfo");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].mods[2].kind, ModKind::Lfo);
    graphics.sync(&mut app, &mut Vec::new()).unwrap();
    let now = app.mod_images.unwrap();
    assert_ne!(now[2], shown[2]);
    assert_eq!(now[5], shown[5]);
    screen_cells(&mut app);
    assert!(app.lfo_cycles_at[2].is_some());
}

/// An app on the Mod page of a song whose first track's last three slots are these LFOs, with its
/// audio side.
fn with_lfos(lfos: [ModSlot; LFOS]) -> (App, Realtime) {
    let (mut app, realtime) = app();
    for (at, modulator) in lfos.into_iter().enumerate() {
        app.apply(&Action::SetMod {
            track: 0,
            slot: at + LFOS,
            modulator,
        });
    }
    press(&mut app, " 4L");
    (app, realtime)
}

/// An LFO on a destination, with a shape, a phase and a depth.
fn lfo_on(destination: SettingId, shape: Shape, phase: u16, depth: i16) -> ModSlot {
    ModSlot::lfo(
        Some(destination),
        Lfo {
            shape,
            phase,
            ..Lfo::default()
        },
    )
    .with_depth(depth)
}

/// An LFO going nowhere.
fn unassigned() -> ModSlot {
    ModSlot::lfo(None, Lfo::default())
}

/// What the engine would report for the LFOs, the last three slots, at these places in their
/// waves, as it works them out.
fn reported(app: &App, turns: [Option<f64>; LFOS]) -> LiveValues {
    let mods = &app.song().tracks[0].mods;
    LiveValues {
        mods: std::array::from_fn(|slot| {
            let turns = turns[slot.checked_sub(LFOS)?]?;
            Some(LfoNow::of(turns, wave(mods[slot].lfo.shape, turns, 0)))
        }),
        ..LiveValues::default()
    }
}

/// Bring the terminal's images up to date, draw the screen, and place what the program places
/// itself, giving back what was sent for each.
fn frame(app: &mut App, graphics: &mut Graphics) -> (String, String) {
    let mut synced = Vec::new();
    graphics.sync(app, &mut synced).unwrap();
    screen_cells(app);
    let mut placed = Vec::new();
    graphics.place(app, &mut placed).unwrap();
    (
        String::from_utf8_lossy(&synced).into_owned(),
        String::from_utf8_lossy(&placed).into_owned(),
    )
}

/// Where each dot's middle is, as everything sent has left it, in the author's screen's pixels,
/// with its image's id, top to bottom, which is the LFOs' order.
fn dot_middles(sent: &str) -> Vec<(u32, (f32, f32))> {
    let placements = sent_placements(sent.as_bytes(), DEFAULT_CELL);
    let half = dot_pixels(DEFAULT_CELL) as f32 / 2.0;
    let mut middles: Vec<_> = placements
        .placed
        .iter()
        .map(|placed| {
            let x = usize::from(placed.column) * DEFAULT_CELL.0 + placed.across;
            let y = usize::from(placed.row) * DEFAULT_CELL.1 + placed.down;
            (placed.id, (x as f32 + half, y as f32 + half))
        })
        .collect();
    middles.sort_by(|a, b| a.1.1.total_cmp(&b.1.1));
    middles
}

/// Where a point on an LFO's cycle is in the author's screen's pixels: `x` across the cycle from 0
/// to 1 and `level` up it, in the picture at `corner`, the line's half width in from its edges.
fn on_cycle(corner: (u16, u16), x: f64, level: f64) -> (f32, f32) {
    let (width, height) = (24.0 * DEFAULT_CELL.0 as f64, 5.0 * DEFAULT_CELL.1 as f64);
    let left = f64::from(corner.0) * DEFAULT_CELL.0 as f64;
    let top = f64::from(corner.1) * DEFAULT_CELL.1 as f64;
    (
        (left + 1.0 + x * (width - 2.0)) as f32,
        (top + 1.0 + (1.0 - level) * (height - 2.0)) as f32,
    )
}

#[test]
fn an_lfos_dot_rides_its_cycle_where_the_engine_says_it_is_and_moves_only_by_placing() {
    let (mut app, _realtime) = with_lfos([
        lfo_on(SettingId::CUT, Shape::Sine, 0, 0x40),
        // A quarter turn in, and upside down.
        lfo_on(SettingId::PAN, Shape::Ramp, 90, -0x30),
        // Going nowhere, so it never has a dot, whatever's reported.
        unassigned(),
    ]);
    let mut graphics = Graphics::new();
    let (synced, placed) = frame(&mut app, &mut graphics);
    assert!(synced.contains("a=T"), "the cycles are sent");
    assert_eq!(placed, "", "nothing reported, so no dot");

    // Each running LFO's dot is where its wave is, on its cycle drawn from its phase.
    let turns = [Some(0.1), Some(0.6), Some(0.3)];
    app.live = Some(reported(&app, turns));
    let (synced, placed) = frame(&mut app, &mut graphics);
    assert!(!synced.contains("a=T"), "the cycles are as they were");
    assert_eq!(
        placed.matches("a=t,").count(),
        2,
        "a dot in each of two shades, once"
    );
    assert_eq!(placed.matches("a=p,").count(), 2, "{placed:?}");
    let expected = |app: &App, turns: [Option<f64>; LFOS]| {
        let mods = app.song().tracks[0].mods;
        (0..2)
            .map(|at| {
                let modulator = mods[at + LFOS];
                let turns = turns[at].unwrap();
                let x = (turns - f64::from(modulator.lfo.phase) / 360.0).rem_euclid(1.0);
                let sign = if modulator.depth < 0 { -1.0 } else { 1.0 };
                let level = 0.5 + 0.5 * sign * f64::from(wave(modulator.lfo.shape, turns, 0));
                on_cycle(app.lfo_cycles_at[at + LFOS].unwrap(), x, level)
            })
            .collect::<Vec<_>>()
    };
    let check = |placed: &str, want: Vec<(f32, f32)>| {
        let middles = dot_middles(placed);
        assert_eq!(middles.len(), 2, "{placed:?}");
        for ((_, found), want) in middles.iter().zip(want) {
            assert!(
                (found.0 - want.0).abs() <= 1.0 && (found.1 - want.1).abs() <= 1.0,
                "{found:?} against {want:?}"
            );
        }
    };
    let mut all = placed.clone();
    check(&all, expected(&app, turns));
    // The sine's dot, a tenth of the way round, is in the cycle's top half, and the flipped
    // ramp's, past its middle, in the bottom half.
    let middles = dot_middles(&all);
    let middle_row = |at: usize| on_cycle(app.lfo_cycles_at[at + LFOS].unwrap(), 0.0, 0.5).1;
    assert!(middles[0].1.1 < middle_row(0));
    assert!(middles[1].1.1 > middle_row(1));
    assert!(placed.contains(",Y="), "{placed:?}");

    // Moving on, each is placed again, and nothing is sent: no image, only the two placements.
    let ids: Vec<u32> = middles.iter().map(|(id, _)| *id).collect();
    let turns = [Some(0.35), Some(0.95), Some(0.5)];
    app.live = Some(reported(&app, turns));
    let (synced, placed) = frame(&mut app, &mut graphics);
    assert!(
        !synced.contains("a=T") && !placed.contains("a=t"),
        "{placed:?}"
    );
    assert_eq!(placed.matches("a=p,").count(), 2, "{placed:?}");
    assert!(
        placed.len() < 2 * 80,
        "about 60 bytes a move: {}",
        placed.len()
    );
    all.push_str(&placed);
    check(&all, expected(&app, turns));
    let now: Vec<u32> = dot_middles(&all).iter().map(|(id, _)| *id).collect();
    assert_eq!(now, ids);
    // Where nothing moved, nothing is sent.
    let (_, placed) = frame(&mut app, &mut graphics);
    assert_eq!(placed, "");

    // With the cursor in Mod 4, its dot is the focused shade's, which is the other image.
    press(&mut app, &to_lfo(0));
    let (_, placed) = frame(&mut app, &mut graphics);
    assert!(placed.contains("a=d,d=i,"), "{placed:?}");
    all.push_str(&placed);
    let now: Vec<u32> = dot_middles(&all).iter().map(|(id, _)| *id).collect();
    assert_ne!(now[0], ids[0]);
    assert_eq!(now[1], ids[1]);
    check(&all, expected(&app, turns));

    // Switched out, its dot goes, and back in, it comes back.
    press(&mut app, "b");
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert_eq!(dot_middles(&all).len(), 1, "{placed:?}");
    press(&mut app, "b");
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert_eq!(dot_middles(&all).len(), 2, "{placed:?}");

    // Under the help, no dot shows through.
    app.mode = Mode::Help;
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert!(dot_middles(&all).is_empty(), "{placed:?}");
    app.mode = Mode::Normal;
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);

    // Stopped, the engine reports none running, and they go.
    app.live = Some(reported(&app, [None; LFOS]));
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert!(dot_middles(&all).is_empty(), "{placed:?}");
    // And off the Mod page too.
    app.live = Some(reported(&app, turns));
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert_eq!(dot_middles(&all).len(), 2);
    press(&mut app, "H");
    let (_, placed) = frame(&mut app, &mut graphics);
    all.push_str(&placed);
    assert!(dot_middles(&all).is_empty(), "{placed:?}");
}

#[test]
fn an_lfos_cycle_is_sent_again_only_for_its_wave_phase_deform_or_depths_sign() {
    let (mut app, _realtime) = with_lfos([
        lfo_on(SettingId::CUT, Shape::Sine, 0, 0x40),
        unassigned(),
        unassigned(),
    ]);
    let mut graphics = Graphics::new();
    let mut sync = |app: &mut App| {
        let mut sent = Vec::new();
        graphics.sync(app, &mut sent).unwrap();
        (
            app.mod_images.unwrap()[LFOS],
            // The panels' pictures, and not the scope's.
            String::from_utf8_lossy(&sent).matches(",c=24,r=5,").count(),
        )
    };
    let (first, sent) = sync(&mut app);
    assert_eq!(sent, 6, "three envelopes and three cycles");
    let set = |app: &mut App, change: &dyn Fn(&mut ModSlot)| {
        let mut modulator = app.song().tracks[0].mods[LFOS];
        change(&mut modulator);
        app.apply(&Action::SetMod {
            track: 0,
            slot: LFOS,
            modulator,
        });
    };
    // Its rate, the size of its depth, its destination and its mode leave it alone.
    set(&mut app, &|slot| slot.lfo.rate = Rate::Note(240));
    set(&mut app, &|slot| slot.depth = 0x7F);
    set(&mut app, &|slot| slot.destination = Some(SettingId::PAN));
    set(&mut app, &|slot| slot.lfo.mode = LfoMode::Trig);
    assert_eq!(sync(&mut app), (first, 0));
    // Its wave, its phase, its deform and its depth's sign each send it again.
    let changes: [&dyn Fn(&mut ModSlot); 4] = [
        &|slot| slot.lfo.shape = Shape::Square,
        &|slot| slot.lfo.phase = 45,
        &|slot| slot.lfo.deform = 0x40,
        &|slot| slot.depth = -0x7F,
    ];
    let mut seen = vec![first];
    for change in changes {
        set(&mut app, change);
        // Two frames on, as one changed a frame after another waits a frame.
        sync(&mut app);
        let (id, _) = sync(&mut app);
        assert!(!seen.contains(&id), "{id} in {seen:?}");
        seen.push(id);
    }
    // Switched out, it's sent faint, and back in, its own is placed again with nothing sent.
    press(&mut app, &to_lfo(0));
    let (focused, _) = sync(&mut app);
    press(&mut app, "b");
    let (faint, sent) = sync(&mut app);
    assert_ne!(faint, focused);
    assert_eq!(sent, 1, "its faint cycle");
    press(&mut app, "b");
    assert_eq!(sync(&mut app), (focused, 0));
}

/// What the dots cost: three LFOs moving, at a quarter, an eighth and a bar, run as the program
/// runs them, a frame of audio, a tick, a sync, a drawing and the placing every 33 ms. It prints
/// what it measures with `--nocapture`.
#[test]
fn three_lfos_dots_move_by_placing_alone_at_about_60_bytes_each() {
    use vt_model::constants::{FRAME_MS, TICKS_PER_QUARTER};
    let quarter = TICKS_PER_QUARTER as u32;
    let lfo = |destination, shape, ticks| {
        let mut slot = lfo_on(destination, shape, 0, 0x40);
        slot.lfo.rate = Rate::Note(ticks);
        slot
    };
    let (mut app, mut realtime) = with_lfos([
        lfo(SettingId::CUT, Shape::Sine, quarter),
        lfo(SettingId::PAN, Shape::Tri, quarter / 2),
        lfo(SettingId::RES, Shape::Smooth, quarter * 4),
    ]);
    app.handle
        .send(vt_engine::link::Command::PlayFrom { step: 0 });
    let mut graphics = Graphics::new();
    let frames = 4 * 1000 / FRAME_MS as usize;
    let audio = 48_000 * FRAME_MS as usize / 1000;
    let mut out = vec![0.0; audio * 2];
    let (mut placements, mut bytes, mut cycles_sent, mut all_moved) = (0, 0, 0, 0);
    let mut placing = std::time::Duration::ZERO;
    for at in 0..frames {
        realtime.process(&mut out);
        app.tick();
        let mut synced = Vec::new();
        graphics.sync(&mut app, &mut synced).unwrap();
        screen_cells(&mut app);
        let mut placed = Vec::new();
        let started = std::time::Instant::now();
        graphics.place(&app, &mut placed).unwrap();
        placing += started.elapsed();
        let placed = String::from_utf8_lossy(&placed);
        // After the first frame, which sends the pictures and the dots' two images.
        if at > 0 {
            cycles_sent += String::from_utf8_lossy(&synced)
                .matches(",c=24,r=5,")
                .count();
            assert!(!placed.contains("a=t"), "{placed:?}");
            let moved = placed.matches("a=p,").count();
            placements += moved;
            bytes += placed.len();
            all_moved += usize::from(moved == 3);
        }
    }
    let seconds = (frames - 1) as f32 * FRAME_MS as f32 / 1000.0;
    eprintln!(
        "three LFOs' dots: {placements} placements in {seconds:.1} s, {:.1} a second; {bytes} \
         bytes, {:.0} a placement and {:.2} KB a second; all three moved in {all_moved} of {} \
         frames; the cycles sent again {cycles_sent} times; {:.3} ms a frame to place them",
        placements as f32 / seconds,
        bytes as f32 / placements.max(1) as f32,
        bytes as f32 / seconds / 1024.0,
        frames - 1,
        placing.as_secs_f32() * 1000.0 / frames as f32,
    );
    assert_eq!(
        cycles_sent, 0,
        "the cycles never change with the LFOs moving"
    );
    assert!(all_moved > (frames - 1) * 9 / 10, "{all_moved}");
    assert!(bytes <= placements * 80, "{bytes} for {placements}");
}

#[test]
fn a_modulators_toggles_take_two_cells_and_nothing_moves_when_one_flips() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    // On Mod 1's Reset, which is on, its colours over both the toggle's cells.
    press(&mut app, "jjjl");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Reset));
    let before = track_page_buffer(&app);
    // Reset off and Mod 1 switched out: only its title's switch and Reset change, and nothing
    // else moves.
    press(&mut app, "=b");
    let after = track_page_buffer(&app);
    let flipped = toggles_flipped(&before, &after);
    assert_eq!(flipped.len(), 2, "{flipped:?}");
    press(&mut app, "-b");
    assert!(toggles_flipped(&before, &track_page_buffer(&app)).is_empty());
}

#[test]
fn a_slots_polarity_is_in_its_title_and_equals_flips_it_as_one_undo_step() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let slot = |app: &App| app.song().tracks[0].mods[1];
    // Mod 2's polarity, after its type and its curve, is its type's own until one is chosen.
    press(&mut app, "}2l");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Polarity));
    assert_eq!(slot(&app).polarity, None);
    let depth = app.history.undo_depth();
    press(&mut app, "=");
    assert_eq!(slot(&app).polarity, Some(Polarity::Bi));
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        true,
        "ADSR  Exp  Bi  → Cut ─"
    )));
    press(&mut app, "-");
    assert_eq!(
        slot(&app).polarity,
        Some(Polarity::Uni),
        "chosen, even as the type's own"
    );
    press(&mut app, "u");
    assert_eq!(slot(&app).polarity, Some(Polarity::Bi));
    // `x` puts it back to following the type.
    press(&mut app, "x");
    assert_eq!(slot(&app).polarity, None);
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        true,
        "ADSR  Exp  Uni  → Cut ─"
    )));
    // The slot shaping the notes has none to show.
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "ADSR  Exp  --  → Vol ─"
    )));
}

#[test]
fn k_on_a_polarity_lists_one_way_and_both() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    // On Mod 4's, an LFO's, after its mode.
    press(&mut app, &format!("{}ll", to_lfo(0)));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Polarity));
    press(&mut app, "K");
    let picker = app.picker.as_ref().expect("the polarity picker");
    assert_eq!(picker.title, "Mod 4 polarity");
    let labels: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert!(
        labels[0].starts_with("Uni") && labels[1].starts_with("Bi"),
        "{labels:?}"
    );
    assert_eq!(
        listed(picker.current().unwrap()).split_whitespace().next(),
        Some("Bi")
    );
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].mods[3].polarity, Some(Polarity::Uni));
    assert!(device_title(&app, "Mod 4").starts_with(&modulator_title(
        "Mod 4",
        true,
        "LFO  Free  Uni  → --- ─"
    )));
}

#[test]
fn a_type_change_keeps_a_chosen_polarity_and_otherwise_follows_the_type() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    let slot = |app: &App| app.song().tracks[0].mods[2];
    let pick = |app: &mut App, up: bool| {
        press(app, "0K");
        press(app, if up { "/adsr" } else { "/lfo" });
        special(app, KeyCode::Enter);
    };
    // Mod 3, an ADSR going one way, made an LFO, swings both ways, as an LFO does.
    press(&mut app, "}}");
    pick(&mut app, false);
    assert_eq!(slot(&app).kind, ModKind::Lfo);
    assert_eq!(
        (slot(&app).polarity, slot(&app).polarity()),
        (None, Polarity::Bi)
    );
    assert!(device_title(&app, "Mod 3").starts_with(&modulator_title(
        "Mod 3",
        true,
        "LFO  Free  Bi  → --- ─"
    )));
    // Back to an ADSR, one way again.
    pick(&mut app, true);
    assert_eq!(slot(&app).polarity(), Polarity::Uni);
    // Chosen both ways, it stays so through a change of type.
    press(&mut app, "2l=");
    assert_eq!(slot(&app).polarity, Some(Polarity::Bi));
    pick(&mut app, false);
    assert_eq!(slot(&app).kind, ModKind::Lfo);
    assert_eq!(slot(&app).polarity, Some(Polarity::Bi));
    pick(&mut app, true);
    assert_eq!(
        (slot(&app).kind, slot(&app).polarity()),
        (ModKind::Adsr, Polarity::Bi)
    );
}

#[test]
fn a_slots_picture_is_sent_again_when_its_polarity_changes() {
    let (mut app, _realtime) = with_lfos([
        lfo_on(SettingId::CUT, Shape::Sine, 0, 0x40),
        unassigned(),
        unassigned(),
    ]);
    let mut graphics = Graphics::new();
    let mut ids = |app: &mut App| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        // Twice, as a picture changed a frame after another waits a frame.
        graphics.sync(app, &mut Vec::new()).unwrap();
        app.mod_images.unwrap()
    };
    let first = ids(&mut app);
    let set = |app: &mut App, slot: usize, polarity: Polarity| {
        let modulator = ModSlot {
            polarity: Some(polarity),
            ..app.song().tracks[0].mods[slot]
        };
        app.apply(&Action::SetMod {
            track: 0,
            slot,
            modulator,
        });
    };
    // Chosen as its type's own, nothing changes for the picture.
    set(&mut app, LFOS, Polarity::Bi);
    set(&mut app, 1, Polarity::Uni);
    assert_eq!(ids(&mut app), first);
    // The LFO's cycle and the envelope's shape each change with it.
    set(&mut app, LFOS, Polarity::Uni);
    set(&mut app, 1, Polarity::Bi);
    let now = ids(&mut app);
    assert_ne!(now[LFOS], first[LFOS]);
    assert_ne!(now[1], first[1]);
    assert_eq!(now[2], first[2], "the others' stay");
}

#[test]
fn an_envelopes_curve_is_its_mode_in_the_title_and_its_picture_follows_it() {
    use vt_model::envelope::Curve;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let mut ids = |app: &mut App| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        graphics.sync(app, &mut Vec::new()).unwrap();
        app.mod_images.unwrap()
    };
    press(&mut app, " 4L");
    let first = ids(&mut app);
    let curve = |app: &App| app.song().tracks[0].mods[0].adsr.curve;
    // `l` from Mod 1's type is its curve, `Exp` until chosen.
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Mode));
    assert!(status_line(&app).contains("Curve"), "{}", status_line(&app));
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Mod 1 curve");
    assert_eq!(
        picker
            .items()
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["Exp", "Lin", "Snap", "Analog"]
    );
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(curve(&app), Curve::Lin);
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "ADSR  Lin  --  → Vol ─"
    )));
    // The picture is drawn again with the new curve, and only Mod 1's.
    let now = ids(&mut app);
    assert_ne!(now[0], first[0]);
    assert_eq!(now[1..], first[1..]);
    // `=` and `-` go through the four, round, `x` puts back `Exp`, and each is one undo step.
    press(&mut app, "=");
    assert_eq!(curve(&app), Curve::Snap);
    press(&mut app, "=");
    assert_eq!(curve(&app), Curve::Analog);
    press(&mut app, "=");
    assert_eq!(curve(&app), Curve::Exp);
    press(&mut app, "-x");
    assert_eq!(curve(&app), Curve::Exp);
    press(&mut app, "u");
    assert_eq!(curve(&app), Curve::Analog);
    // An LFO's mode is still its own.
    assert_eq!(app.song().tracks[0].mods[3].lfo.mode, LfoMode::Free);
}

#[test]
fn an_ahd_slot_holds_for_a_time_or_the_note_with_its_own_settings_and_picture() {
    use vt_model::envelope::{Ahd, Curve, Hold};
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let mut ids = |app: &mut App| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        graphics.sync(app, &mut Vec::new()).unwrap();
        app.mod_images.unwrap()
    };
    press(&mut app, " 4L");
    let first = ids(&mut app);
    let slot = |app: &App| app.song().tracks[0].mods[1];
    // Mod 2, the filter's envelope, made an AHD: the second type listed, after ADSR.
    press(&mut app, "}K");
    let picker = app.picker.as_ref().unwrap();
    let listed: Vec<&str> = picker
        .items()
        .iter()
        .map(|item| item.label.as_str())
        .collect();
    assert_eq!(
        listed,
        ["ADSR", "AHD", "Drum", "LPG", "LFO", "Steps", "Track"]
    );
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Ahd);
    assert_eq!(slot(&app).ahd, Ahd::default());
    assert_eq!(slot(&app).destination, Some(SettingId::CUT));
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        true,
        "AHD  Exp  Uni  → Cut ─"
    )));
    // Its grid: its times down the left, its depth and reset down the right, and a hole.
    let shown = screen_cells(&mut app).join("\n");
    for cell in ["Hold      30 ms", "Decay     200 ms", "Depth     00"] {
        assert!(shown.contains(cell), "{cell} in\n{shown}");
    }
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Attack));
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Hold));
    // Its own shape is drawn, and the others' stay.
    let now = ids(&mut app);
    assert_ne!(now[1], first[1]);
    assert_eq!(now[2..], first[2..]);
    // `-` takes its hold down a time at a time, and below no time to the note.
    press(&mut app, "x");
    assert_eq!(slot(&app).ahd.hold, Hold::Ms(30));
    typed(&mut app, "0");
    assert_eq!(slot(&app).ahd.hold, Hold::Ms(0));
    press(&mut app, "-");
    assert_eq!(slot(&app).ahd.hold, Hold::Note);
    let shown = screen_cells(&mut app).join("\n");
    assert!(shown.contains("Hold      Note"), "{shown}");
    press(&mut app, "-");
    assert_eq!(slot(&app).ahd.hold, Hold::Note, "it stops there");
    press(&mut app, "=");
    assert_eq!(slot(&app).ahd.hold, Hold::Ms(0));
    typed(&mut app, "note");
    assert_eq!(slot(&app).ahd.hold, Hold::Note);
    typed(&mut app, "2500");
    assert_eq!(slot(&app).ahd.hold, Hold::Note, "too long");
    typed(&mut app, "2000");
    assert_eq!(slot(&app).ahd.hold, Hold::Ms(2000));
    // Its attack and decay are its own, not its ADSR's, and so are its curve and reset.
    let adsr = slot(&app).adsr;
    press(&mut app, "j=");
    assert_eq!(slot(&app).ahd.decay_ms, 225);
    press(&mut app, "kkkl");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Mode));
    press(&mut app, "=");
    assert_eq!(slot(&app).ahd.curve, Curve::Lin);
    assert_eq!(slot(&app).adsr, adsr);
    // Back to an ADSR, which is as it was, and an AHD again, which is as it was too.
    let ahd = slot(&app).ahd;
    press(&mut app, "hK");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "Kk");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Adsr);
    assert_eq!(slot(&app).adsr, adsr);
    press(&mut app, "Kj");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).ahd, ahd);
}

#[test]
fn a_drum_slot_strikes_onto_a_body_with_its_own_settings_ratchet_and_picture() {
    use vt_model::envelope::{Curve, Drum, MAX_RATCHET};
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let mut ids = |app: &mut App| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        graphics.sync(app, &mut Vec::new()).unwrap();
        app.mod_images.unwrap()
    };
    press(&mut app, " 4L");
    let first = ids(&mut app);
    let slot = |app: &App| app.song().tracks[0].mods[1];
    // Mod 2 made a drum envelope, the third type listed.
    press(&mut app, "}K/drum");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Drum);
    assert_eq!(slot(&app).drum, Drum::default());
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        true,
        "Drum  Exp  Uni  → Cut ─"
    )));
    // Its grid: its peak, body and decay down the left, its depth and ratchet down the right,
    // and no reset, as it always strikes from full.
    let shown = screen_cells(&mut app).join("\n");
    for cell in [
        "Peak      10 ms",
        "Body      40%",
        "Decay     300 ms",
        "Depth     00",
        "Ratchet   1",
    ] {
        assert!(shown.contains(cell), "{cell} in\n{shown}");
    }
    assert!(!ModField::Reset.on(ModKind::Drum) && !ModField::Attack.on(ModKind::Drum));
    // Its own shape is drawn, and the others' stay.
    let now = ids(&mut app);
    assert_ne!(now[1], first[1]);
    assert_eq!(now[2..], first[2..]);
    // Each of its settings is its own, and moves as its kind does.
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Peak));
    press(&mut app, "=");
    assert_eq!(slot(&app).drum.peak_ms, 15);
    press(&mut app, "j+");
    assert_eq!(slot(&app).drum.body, 50);
    typed(&mut app, "101");
    assert_eq!(slot(&app).drum.body, 50, "past full");
    let adsr = slot(&app).adsr;
    press(&mut app, "j=");
    assert_eq!(slot(&app).drum.decay_ms, 325);
    assert_eq!(slot(&app).adsr, adsr, "its ADSR's decay is left");
    // `Ratchet`, under `Depth`, strikes 1 to 8 times.
    press(&mut app, "kl");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Ratchet));
    press(&mut app, "-");
    assert_eq!(slot(&app).drum.ratchet, 1, "it stops at one");
    press(&mut app, "===");
    assert_eq!(slot(&app).drum.ratchet, 4);
    typed(&mut app, "9");
    assert_eq!(slot(&app).drum.ratchet, 4, "too many");
    typed(&mut app, &MAX_RATCHET.to_string());
    assert_eq!(slot(&app).drum.ratchet, MAX_RATCHET);
    press(&mut app, "x");
    assert_eq!(slot(&app).drum.ratchet, 1);
    // Its curve is its own too.
    press(&mut app, "hkkl");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Mode));
    press(&mut app, "=");
    assert_eq!(slot(&app).drum.curve, Curve::Lin);
    assert_eq!(slot(&app).adsr, adsr);
}

#[test]
fn an_lpg_slot_has_no_mode_and_its_own_attack_decay_colour_and_reset() {
    use vt_model::envelope::Lpg;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let mut ids = |app: &mut App| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        graphics.sync(app, &mut Vec::new()).unwrap();
        app.mod_images.unwrap()
    };
    press(&mut app, " 4L");
    let first = ids(&mut app);
    let slot = |app: &App| app.song().tracks[0].mods[1];
    // Mod 2 made a low-pass gate: no curve in its title, as its curve is the point of it.
    press(&mut app, "}K/lpg");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Lpg);
    assert_eq!(slot(&app).lpg, Lpg::default());
    assert!(device_title(&app, "Mod 2").starts_with(&modulator_title(
        "Mod 2",
        true,
        "LPG  Uni  → Cut ─"
    )));
    let shown = screen_cells(&mut app).join("\n");
    for cell in [
        "Attack    1 ms",
        "Decay     400 ms",
        "Colour    C0",
        "Depth     00",
    ] {
        assert!(shown.contains(cell), "{cell} in\n{shown}");
    }
    let now = ids(&mut app);
    assert_ne!(now[1], first[1]);
    assert_eq!(now[2..], first[2..]);
    // Its settings are its own, not its ADSR's.
    let adsr = slot(&app).adsr;
    press(&mut app, "jj=");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Decay));
    assert_eq!(slot(&app).lpg.decay_ms, 425);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Colour));
    press(&mut app, "+");
    assert_eq!(slot(&app).lpg.colour, 0xD0);
    typed(&mut app, "ff");
    assert_eq!(slot(&app).lpg.colour, 0xFF);
    press(&mut app, "=");
    assert_eq!(slot(&app).lpg.colour, 0xFF, "it stops at the top");
    typed(&mut app, "zz");
    assert_eq!(slot(&app).lpg.colour, 0xFF, "not hex");
    press(&mut app, "x");
    assert_eq!(slot(&app).lpg.colour, 0xC0);
    press(&mut app, "kl");
    assert_eq!(app.track_control(), Control::Mod(1, ModField::Reset));
    press(&mut app, "=");
    assert!(!slot(&app).lpg.reset);
    assert!(slot(&app).adsr.reset);
    assert_eq!(slot(&app).adsr, adsr);
}

#[test]
fn a_drum_on_vol_shapes_the_notes_with_no_depth() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LK/drum");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].mods[0].kind, ModKind::Drum);
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "Drum  Exp  --  → Vol ─"
    )));
}

#[test]
fn an_ahd_on_vol_shapes_the_notes_with_no_depth() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LK");
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].mods[0].kind, ModKind::Ahd);
    assert!(device_title(&app, "Mod 1").starts_with(&modulator_title(
        "Mod 1",
        true,
        "AHD  Exp  --  → Vol ─"
    )));
    assert_eq!(depth_shown(&mut app, 1), "--");
}

#[test]
fn a_slot_made_steps_with_no_depth_is_given_ff_and_its_title_has_its_mode_and_loop() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    press(&mut app, &format!("{}K", to_lfo(0)));
    press(&mut app, "/steps");
    special(&mut app, KeyCode::Enter);
    let slot = |app: &App| app.song().tracks[0].mods[3];
    assert_eq!(slot(&app).kind, ModKind::Steps);
    assert_eq!(slot(&app).depth, 0xFF, "heard as written");
    assert!(app.status.contains("+FF"), "{}", app.status);
    // One undo step takes back both.
    press(&mut app, "u");
    assert_eq!((slot(&app).kind, slot(&app).depth), (ModKind::Lfo, 0));
    press(&mut app, "K/steps");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).kind, ModKind::Steps);
    // Its title is its type, its mode and its loop, then where it goes: no polarity.
    assert!(
        device_title(&app, "Mod 4").starts_with(&modulator_title(
            "Mod 4",
            true,
            "Steps  Trig  from 1  → --- ─"
        )),
        "{}",
        device_title(&app, "Mod 4")
    );
    // `=` on the loop goes through them, and `K` lists them.
    press(&mut app, "gg0ll");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Loop));
    press(&mut app, "=");
    assert_eq!(slot(&app).steps.looping, Loop::From(1));
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().title, "Steps loop");
    press(&mut app, "/bounce");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).steps.looping, Loop::Bounce);
}

/// The Mod page as the author's screen draws it, a line at a time.
fn mod_page_text(app: &App) -> Vec<String> {
    let (width, height) = (163, 40);
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// A track whose `Mod 4` is Steps on `Cut`, with the cursor on its type.
fn with_steps() -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = app();
    app.apply(&Action::SetMod {
        track: 0,
        slot: 3,
        modulator: ModSlot::steps(Some(SettingId::CUT), Steps::default()),
    });
    press(&mut app, " 4L");
    press(&mut app, &to_lfo(0));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Type));
    (app, realtime)
}

#[test]
fn the_steps_panel_is_a_cursor_stop_that_h_and_l_go_across() {
    let (mut app, _realtime) = with_steps();
    // Its grid: Rate, Length and Depth down the left, Start down the right, then its values.
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Rate));
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Start));
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(0)));
    press(&mut app, "3l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(3)));
    press(&mut app, "$");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(7)));
    // Past its last value there's nothing on the right, so it stays.
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(7)));
    press(&mut app, "0");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(0)));
    // `0` again, and `h` from the first, leave it for the grid.
    press(&mut app, "h");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Start));
    // From the right column's lower rows, `l` goes into it too.
    press(&mut app, "hjl");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Mutate));
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Step(0)));
    // From Mod 1 on its right, `h` comes in on the last value.
    let (mut app, _realtime) = with_steps();
    app.apply(&Action::SetMod {
        track: 0,
        slot: 0,
        modulator: ModSlot::steps(Some(SettingId::CUT), Steps::default()),
    });
    press(&mut app, "jh");
    assert_eq!(app.track_control(), Control::Mod(0, ModField::Step(7)));
}

#[test]
fn a_steps_values_are_edited_in_its_panel_and_drawn_as_bars_with_the_cursor_under_one() {
    let (mut app, _realtime) = with_steps();
    let values = |app: &App| app.song().tracks[0].mods[3].steps.values;
    press(&mut app, "jll");
    press(&mut app, "=");
    assert_eq!(values(&app)[0], 1);
    // `+` and `_` move an octave.
    press(&mut app, "l+");
    assert_eq!(values(&app)[1], 0x0C);
    press(&mut app, "__");
    assert_eq!(values(&app)[1], -0x0C);
    press(&mut app, "l");
    typed(&mut app, "-7f");
    assert_eq!(values(&app)[2], -0x7F);
    press(&mut app, "l");
    typed(&mut app, "ff");
    assert_eq!(values(&app)[3], 0xFF);
    press(&mut app, "hhhx");
    assert_eq!(values(&app)[..4], [0, -0x0C, -0x7F, 0xFF]);
    // The panel: a bar for each of the eight played, up for above none and down for below, and a
    // tick across the middle for none, with the `▲` under the step being edited.
    // The eight played are in the middle of the box's 24 columns, from its ninth.
    let lines = mod_page_text(&app);
    let (top, _) = device_at(&lines, "Mod 4");
    let bottom = (top..top + 9)
        .find(|row| lines[*row].contains('▲'))
        .expect("the cursor's mark");
    let border: Vec<char> = lines[bottom].chars().collect();
    let marker = border.iter().position(|c| *c == '▲').unwrap();
    let edge = border[..marker].iter().rposition(|c| *c == '└').unwrap();
    let first = edge + 1 + 8;
    let cell = |row: usize, step: usize| lines[bottom - 5 + row].chars().nth(first + step).unwrap();
    let shown = lines[bottom - 5..=bottom].join("\n");
    // Step 4 at `+FF` reaches the top row and step 3 at `-7F` halfway down; step 1 is a tick
    // across the middle row, and step 2 at `-0C` a dot just under it.
    assert_ne!(cell(0, 3), '⠀', "{shown}");
    assert_eq!(cell(0, 2), '⠀', "{shown}");
    assert_ne!(cell(3, 2), '⠀', "{shown}");
    assert_eq!(cell(2, 0), '⠆', "{shown}");
    assert_eq!(cell(3, 0), '⠀', "{shown}");
    assert_eq!(cell(2, 1), '⠄', "{shown}");
    // Nothing past the eighth, and the mark under the first, where the cursor is.
    assert_eq!(cell(0, 8), ' ', "{shown}");
    assert_eq!(marker, first, "{shown}");
}

#[test]
fn a_steps_rate_length_and_start_step_and_type() {
    let (mut app, _realtime) = with_steps();
    let steps = |app: &App| app.song().tracks[0].mods[3].steps;
    press(&mut app, "j=");
    assert_eq!(steps(&app).rate, StepRate::Note(180), "= is faster");
    press(&mut app, "x");
    typed(&mut app, "tick");
    assert_eq!(steps(&app).rate, StepRate::Tick);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Length));
    press(&mut app, "-----");
    assert_eq!(steps(&app).length, 3);
    typed(&mut app, "16");
    assert_eq!(steps(&app).length, 16);
    press(&mut app, "kl");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Start));
    typed(&mut app, "5");
    assert_eq!(steps(&app).start, 4);
    typed(&mut app, "17");
    assert_eq!(steps(&app).start, 4);
    // In `Note` mode, a rate means nothing.
    let mut slot = app.song().tracks[0].mods[3];
    slot.steps.mode = StepsMode::Note;
    app.apply(&Action::SetMod {
        track: 0,
        slot: 3,
        modulator: slot,
    });
    let lines = mod_page_text(&app);
    let (top, left) = device_at(&lines, "Mod 4");
    assert!(
        from_column(&lines[top + 2], left).starts_with("│  Rate      --"),
        "{}",
        lines[top + 2]
    );
}

#[test]
fn the_step_playing_is_lit_in_the_playheads_colour() {
    use vt_tui::theme;
    let (mut app, _realtime) = with_steps();
    let mut live = LiveValues::default();
    live.steps[3] = Some(2);
    app.live = Some(live);
    let backend = ratatui::backend::TestBackend::new(163, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = mod_page_text(&app);
    let (top, left) = device_at(&lines, "Mod 4");
    let (bottom, edge) = (top..top + 9)
        .find_map(|row| {
            let line: Vec<char> = lines[row].chars().collect();
            let edge = line[left..].iter().position(|c| *c == '└')?;
            Some((row, left + edge))
        })
        .unwrap();
    // The middle row, where every value of `00` has its tick.
    let at = |step: usize| &buffer[((edge + 9 + step) as u16, (bottom - 3) as u16)];
    assert_eq!(at(2).fg, theme::GREEN);
    assert_eq!(at(1).fg, theme::OVERLAY2);
    assert_eq!(at(1).symbol(), "⠆");
}

#[test]
fn a_steps_mutate_and_smooth_are_under_its_start() {
    let (mut app, _realtime) = with_steps();
    let steps = |app: &App| app.song().tracks[0].mods[3].steps;
    press(&mut app, "jlj");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Mutate));
    press(&mut app, "=+");
    assert_eq!(steps(&app).mutate, 11);
    typed(&mut app, "50%");
    assert_eq!(steps(&app).mutate, 50);
    typed(&mut app, "101");
    assert_eq!(steps(&app).mutate, 50);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Smooth));
    typed(&mut app, "-40");
    assert_eq!(steps(&app).smooth, -0x40);
    press(&mut app, "x");
    assert_eq!(steps(&app).smooth, 0);
    let lines = mod_page_text(&app);
    let (top, left) = device_at(&lines, "Mod 4");
    let row = |at: usize| from_column(&lines[top + at], left);
    assert!(
        row(4).starts_with("│  Length    8          Mutate    50%"),
        "{}",
        row(4)
    );
    assert!(
        row(6).starts_with("│  Depth     +FF        Smooth    00"),
        "{}",
        row(6)
    );
}

#[test]
fn pitch_in_scale_is_a_destination_of_its_own_for_every_type() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    // Mod 4, an LFO: its destination's list has `Pitch` and then `Pitch in scale`.
    press(&mut app, &format!("{}$K", to_lfo(0)));
    let picker = app.picker.as_ref().unwrap();
    let labels: Vec<&str> = picker
        .items()
        .iter()
        .map(|item| item.label.as_str())
        .collect();
    let pitch = labels.iter().position(|label| *label == "Pitch").unwrap();
    assert_eq!(labels[pitch + 1], "Pitch in scale");
    press(&mut app, "/in scale");
    special(&mut app, KeyCode::Enter);
    let slot = |app: &App| app.song().tracks[0].mods[3];
    assert_eq!(slot(&app).destination, Some(SettingId::PITCH));
    assert!(slot(&app).in_scale && slot(&app).goes_in_scale());
    assert!(
        device_title(&app, "Mod 4").contains("→ Pitch in scale"),
        "{}",
        device_title(&app, "Mod 4")
    );
    // `-` goes back to it as it is, and `=` on again.
    press(&mut app, "-");
    assert_eq!(slot(&app).destination, Some(SettingId::PITCH));
    assert!(!slot(&app).in_scale);
    press(&mut app, "=");
    assert!(slot(&app).goes_in_scale());
    press(&mut app, "x");
    assert!(!slot(&app).in_scale);
}

/// A track whose `Mod 4` is a Track slot, with the cursor on its type.
fn with_tracking(tracking: ModSlot) -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = app();
    app.apply(&Action::SetMod {
        track: 0,
        slot: 3,
        modulator: tracking,
    });
    press(&mut app, " 4L");
    press(&mut app, &to_lfo(0));
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Type));
    (app, realtime)
}

#[test]
fn a_track_slot_follows_velocity_with_its_source_in_the_title_and_its_map_down_the_left() {
    use vt_model::tracking::{Source, Tracking};
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    press(&mut app, &format!("{}K", to_lfo(0)));
    press(&mut app, "/track");
    special(&mut app, KeyCode::Enter);
    let slot = |app: &App| app.song().tracks[0].mods[3];
    assert_eq!(slot(&app).kind, ModKind::Track);
    assert_eq!(slot(&app).track, Tracking::default());
    assert_eq!(slot(&app).depth, 0, "a new type keeps the slot's depth");
    assert!(
        device_title(&app, "Mod 4").starts_with(&modulator_title(
            "Mod 4",
            true,
            "Track  Velocity  Uni  → --- ─"
        )),
        "{}",
        device_title(&app, "Mod 4")
    );
    let shown = screen_cells(&mut app).join("\n");
    for cell in ["Low       00", "High      7F", "Depth     00"] {
        assert!(shown.contains(cell), "{cell} in\n{shown}");
    }
    // `Low`, `High` and `Depth` down the left, by ones and by `10`.
    press(&mut app, "j=");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Low));
    assert_eq!(slot(&app).track.velocity.low, 1);
    press(&mut app, "+");
    assert_eq!(slot(&app).track.velocity.low, 0x11);
    typed(&mut app, "40");
    assert_eq!(slot(&app).track.velocity.low, 0x40);
    typed(&mut app, "80");
    assert_eq!(
        slot(&app).track.velocity.low,
        0x40,
        "past the highest velocity"
    );
    assert!(app.status.contains("00 to 7F"), "{}", app.status);
    press(&mut app, "j=");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::High));
    assert_eq!(slot(&app).track.velocity.high, 0x7F, "it stops at the top");
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Depth));
    // The source is its mode: `=` goes through them, and `K` lists them.
    press(&mut app, "gg0l");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Mode));
    press(&mut app, "=");
    assert_eq!(slot(&app).track.source, Source::Random);
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().title, "Mod 4 source");
    press(&mut app, "/note");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).track.source, Source::Note);
    // Each source has its own map: the notes', in notes, by a semitone and an octave.
    let shown = screen_cells(&mut app).join("\n");
    for cell in ["Low       C-1", "High      C-5"] {
        assert!(shown.contains(cell), "{cell} in\n{shown}");
    }
    press(&mut app, "gg0j=+");
    assert_eq!(slot(&app).track.note.low, 36 + 13);
    typed(&mut app, "G-8");
    assert_eq!(slot(&app).track.note.low, 127);
    typed(&mut app, "40");
    assert_eq!(slot(&app).track.note.low, 127, "a note, not hex");
    assert!(app.status.contains("c_1 to G-8"), "{}", app.status);
    press(&mut app, "x");
    assert_eq!(slot(&app).track.note.low, 36);
    // Velocity's is as it was left.
    assert_eq!(slot(&app).track.velocity.low, 0x40);
    press(&mut app, "gg0lx");
    assert_eq!(
        slot(&app).track.source,
        Source::Velocity,
        "x puts back Velocity"
    );
}

#[test]
fn a_track_slots_map_is_sent_with_a_dot_at_the_last_note() {
    use vt_model::tracking::Tracking;
    let (mut app, _realtime) = with_tracking(
        ModSlot::tracking(Some(SettingId::CUT), Tracking::default()).with_depth(0x40),
    );
    let mut graphics = Graphics::new();
    let (synced, placed) = frame(&mut app, &mut graphics);
    assert!(synced.contains("a=T"), "its map is sent");
    assert_eq!(placed, "", "no note yet, so no dot");
    let corner = app.lfo_cycles_at[3].expect("its map is placed");
    // The engine says the last note was half way along its velocities, and so half way up.
    app.live = Some(LiveValues {
        mods: std::array::from_fn(|slot| (slot == 3).then(|| LfoNow::along(0.5, 0.5))),
        ..LiveValues::default()
    });
    let (synced, placed) = frame(&mut app, &mut graphics);
    assert!(!synced.contains("a=T"), "the map is as it was");
    let middles = dot_middles(&placed);
    assert_eq!(middles.len(), 1, "{placed:?}");
    let want = on_cycle(corner, 0.5, 0.5);
    let found = middles[0].1;
    assert!(
        (found.0 - want.0).abs() <= 1.0 && (found.1 - want.1).abs() <= 1.0,
        "{found:?} against {want:?}"
    );
    // A depth of another size leaves the map as it is, and a map of its own sends it again.
    press(&mut app, "jjj=");
    assert_eq!(app.track_control(), Control::Mod(3, ModField::Depth));
    let (synced, _) = frame(&mut app, &mut graphics);
    assert!(!synced.contains("a=T"));
    press(&mut app, "k-");
    assert_eq!(app.song().tracks[0].mods[3].track.velocity.high, 0x7E);
    let (synced, _) = frame(&mut app, &mut graphics);
    assert!(synced.contains("a=T"));
}
