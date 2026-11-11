//! The Mod page, `Space 4` then `L`: a rack of half-width devices, the envelopes down the left
//! beside the LFOs down the right, and the accumulator beside the arpeggio under them. Each
//! modulator's destination is in its title. The cursor moves across the two columns with `h` and
//! `l`, and down each with `j`, `k`, `{` and `}`. The settings themselves are in `track_page.rs`.

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::lfo::LFOS_PER_TRACK;
use vt_model::selection::Motion;
use vt_model::settings::SettingId;
use vt_model::song::{Track, TrackKind};
use vt_tui::app::App;
use vt_tui::input::bindings::Cmd;
use vt_tui::input::mode::Mode;
use vt_tui::track_page::{
    AccumulatorField, ArpField, Control, DeviceCursor, EnvelopeField, LfoField, Page, TrackPage,
};

mod common;
use common::*;

/// Every setting a modulator has, an envelope's, an LFO's, the accumulator's and the arpeggio's.
fn every_mod_setting(sound: bool) -> Vec<Control> {
    use AccumulatorField as Acc;
    use EnvelopeField as Env;
    let mut all = Vec::new();
    for slot in 0..TRACK_ENVELOPES {
        for field in [
            Env::Destination,
            Env::Attack,
            Env::Decay,
            Env::Sustain,
            Env::Release,
            Env::Depth,
            Env::Reset,
        ] {
            all.push(Control::Envelope(slot, field));
        }
    }
    for slot in 0..LFOS_PER_TRACK {
        for field in [
            LfoField::Destination,
            LfoField::Shape,
            LfoField::Rate,
            LfoField::Depth,
            LfoField::Phase,
            LfoField::Retrig,
        ] {
            all.push(Control::Lfo(slot, field));
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

/// The cursor's moves on the Mod page, with no `App`: given a track, from `Env 1`'s destination.
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
    // The page reads `Env 1`, `LFO 1`, `Env 2`, … so `LFO 1` is the second device.
    assert_eq!(cursor(&page), at(0, 0, 0));
    // From the title's only control, `l` goes into `LFO 1`'s title, and `h` back.
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(cursor(&page), at(1, 0, 0));
    assert_eq!(
        page.control(Some(&track)),
        Control::Lfo(0, LfoField::Destination)
    );
    page.key(LEFT, 1, Some(&track));
    assert_eq!(cursor(&page), at(0, 0, 0));
    // On `Sustain`, the last of `Env 1`'s first row, `l` goes to `Shape`, the first of `LFO 1`'s,
    // and `h` comes back to the last.
    keys(&mut page, &track, &[(DOWN, 1), (RIGHT, 1)]);
    assert_eq!(
        page.control(Some(&track)),
        Control::Envelope(0, EnvelopeField::Sustain)
    );
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(cursor(&page), at(1, 1, 0));
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Shape));
    page.key(LEFT, 1, Some(&track));
    assert_eq!(
        page.control(Some(&track)),
        Control::Envelope(0, EnvelopeField::Sustain)
    );
    // A count goes on across: `3l` from `Attack` is `Sustain`, `Shape`, then `Phase`.
    keys(&mut page, &track, &[(LEFT, 1), (RIGHT, 3)]);
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Phase));
    // Nothing is right of the LFOs, so `l` stays, and `$` and `0` keep to the device.
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Phase));
    page.key(Cmd::Move(Motion::RowStart), 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Shape));
    // `LFO 1`'s last row has a hole under `Retrig`, so `l` from `Depth` goes nowhere in it, and
    // from `Env 1`'s `Reset` it lands on `Depth`.
    keys(&mut page, &track, &[(DOWN, 2), (RIGHT, 1)]);
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Depth));
    page.key(LEFT, 1, Some(&track));
    assert_eq!(
        page.control(Some(&track)),
        Control::Envelope(0, EnvelopeField::Reset)
    );
    page.key(RIGHT, 1, Some(&track));
    assert_eq!(page.control(Some(&track)), Control::Lfo(0, LfoField::Depth));
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
fn braces_j_and_k_go_down_and_up_each_column_of_devices() {
    let track = Track::default();
    let mut page = mod_page();
    let control = |page: &TrackPage| page.control(Some(&track));
    // `}` goes down the left column, keeping the row and column, and stops at the bottom.
    keys(&mut page, &track, &[(DOWN, 2), (RIGHT, 1)]);
    assert_eq!(control(&page), Control::Envelope(0, EnvelopeField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Envelope(1, EnvelopeField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Envelope(2, EnvelopeField::Depth));
    page.key(NEXT, 1, Some(&track));
    assert_eq!(control(&page), Control::Accumulator(AccumulatorField::Adds));
    page.key(NEXT, 5, Some(&track));
    assert_eq!(control(&page), Control::Accumulator(AccumulatorField::Adds));
    page.key(PREVIOUS, 9, Some(&track));
    assert_eq!(control(&page), Control::Envelope(0, EnvelopeField::Depth));
    // `3}` goes from `LFO 1` to the arpeggio, down the right column.
    keys(
        &mut page,
        &track,
        &[(Cmd::Move(Motion::First), 1), (RIGHT, 1)],
    );
    assert_eq!(control(&page), Control::Lfo(0, LfoField::Destination));
    page.key(NEXT, 3, Some(&track));
    // The arpeggio's title has nothing in it, so the cursor is on its first row.
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Rate));
    assert_eq!(page.device_cursor(Some(&track)), at(7, 1, 0));
    // `k` goes up into `LFO 3`'s last row, past the arpeggio's empty title, and `j` back down.
    page.key(UP, 1, Some(&track));
    assert_eq!(control(&page), Control::Lfo(2, LfoField::Depth));
    page.key(DOWN, 1, Some(&track));
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Rate));
    // `j` stops at the bottom of its column, rather than going on into the other.
    page.key(DOWN, 9, Some(&track));
    assert_eq!(control(&page), Control::Arpeggio(ArpField::Mode));
    // `gg` and `G` are the top and the bottom of the cursor's column.
    page.key(Cmd::Move(Motion::First), 1, Some(&track));
    assert_eq!(control(&page), Control::Lfo(0, LfoField::Destination));
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
        Control::Accumulator(AccumulatorField::Moves)
    );
    // `j` from an envelope's last row is the title of the one below, and `k` back.
    keys(
        &mut page,
        &track,
        &[(Cmd::Move(Motion::First), 1), (DOWN, 4)],
    );
    assert_eq!(
        control(&page),
        Control::Envelope(1, EnvelopeField::Destination)
    );
    page.key(UP, 1, Some(&track));
    assert_eq!(control(&page), Control::Envelope(0, EnvelopeField::Release));

    // A return has no accumulator or arpeggio, so its columns end with `Env 3` and `LFO 3`.
    let fixed = Track {
        kind: TrackKind::ReturnA,
        ..Track::default()
    };
    let mut page = mod_page();
    page.key(NEXT, 9, Some(&fixed));
    assert_eq!(
        page.control(Some(&fixed)),
        Control::Envelope(2, EnvelopeField::Destination)
    );
    page.key(DOWN, 9, Some(&fixed));
    assert_eq!(
        page.control(Some(&fixed)),
        Control::Envelope(2, EnvelopeField::Release)
    );
}

#[test]
fn the_keys_move_across_the_mod_page_in_the_app() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    assert_eq!(
        app.track_control(),
        Control::Envelope(0, EnvelopeField::Destination)
    );
    press(&mut app, "l}");
    assert_eq!(app.track_control(), Control::Lfo(1, LfoField::Destination));
    press(&mut app, "jh");
    assert_eq!(
        app.track_control(),
        Control::Envelope(1, EnvelopeField::Sustain)
    );
    press(&mut app, "2{");
    assert_eq!(
        app.track_control(),
        Control::Envelope(0, EnvelopeField::Sustain)
    );
    // `{` and `}` aren't refused here, as they are off a rack.
    assert!(!app.status.contains("go between"), "{}", app.status);
}

#[test]
fn each_modulators_destination_is_in_its_title() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    assert!(device_title(&app, "Env 1").starts_with("╭─ Env 1     Vol ─"));
    assert!(device_title(&app, "Env 2").starts_with("╭─ Env 2     Cut ─"));
    assert!(device_title(&app, "Env 3").starts_with("╭─ Env 3     --- ─"));
    // The accumulator and the arpeggio have no destination, and no room for a switch.
    assert!(device_title(&app, "Accumulator").starts_with("╭─ Accumulator ───"));
    assert!(device_title(&app, "Arpeggio").starts_with("╭─ Arpeggio ───"));

    // `K` on it picks it, and the title says what it is.
    press(&mut app, &format!("{}K/pan", to_lfo(2)));
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[2].destination,
        Some(SettingId::PAN)
    );
    assert!(device_title(&app, "LFO 3").starts_with("╭─ LFO 3     Pan ─"));
    // `k` from the grid's first row reaches it.
    press(&mut app, "jkx");
    assert_eq!(app.song().tracks[0].lfos[2].destination, None);
    assert!(device_title(&app, "LFO 3").starts_with("╭─ LFO 3     --- ─"));

    // Going nowhere, it's dim, as an empty slot's title is; set, it's the value's own colour.
    // `Env 1`'s title is on the page's third row and `Env 3`'s two bands down, each 13 in.
    let buffer = track_page_buffer(&app);
    let dim = vt_tui::theme::dim().fg.unwrap();
    assert_eq!(buffer[(13, 2)].symbol(), "V");
    assert_ne!(buffer[(13, 2)].fg, dim, "Env 1's Vol");
    assert_eq!(buffer[(13, 2 + 9 * 2)].symbol(), "-");
    assert_eq!(buffer[(13, 2 + 9 * 2)].fg, dim, "Env 3's ---");
}

#[test]
fn the_mod_page_fills_the_keys_box_at_163_by_42() {
    let (mut app, _realtime) = app();
    app.envelope_images = Some([0x01_02_03, 0x04_05_06, 0x07_08_09]);
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
            ("Env 1", "LFO 1"),
            ("Env 2", "LFO 2"),
            ("Env 3", "LFO 3"),
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
        // Each envelope's shape is in its panel, against the device's right side, and the
        // settings beside it stop before it, with room between.
        for band in 0..3 {
            let top = first + 9 * band;
            let row = from_column(&lines[top + 2], left);
            let image = row.find('▒').map(|at| row[..at].chars().count()).unwrap();
            // Its frame's last column, 2 of room, the box's edge, then the shape's 28.
            assert_eq!(image, width - 1 - 2 - 1 - 28, "{row}");
            assert!(row.contains("Sustain"), "{row}");
            let before: String = row.chars().take(image - 1).collect();
            assert!(before.ends_with("  "), "{row}");
        }
    }
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
    assert_eq!(titles(&draw(&app)), ["Env 1", "Env 2"]);
    // On the arpeggio, the page has gone down to keep its band in view, as low as it can be.
    press(&mut app, "l3}");
    assert_eq!(app.track_control(), Control::Arpeggio(ArpField::Rate));
    let lines = draw(&app);
    assert_eq!(titles(&lines), ["Env 3", "Accum"]);
    // Only whole devices: each top has its bottom under it.
    for (row, line) in lines.iter().enumerate() {
        if line.starts_with('╭') {
            assert!(lines[row + 8].starts_with('╰'), "{}", lines.join("\n"));
        }
    }
    // Back up, it follows the cursor.
    press(&mut app, "gg");
    assert_eq!(titles(&draw(&app)), ["Env 1", "Env 2"]);
}
