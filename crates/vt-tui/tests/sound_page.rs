//! The Sound page as a rack of devices: Machine, Filter, Output and Play, each with its own grid,
//! the machine's type and the filter's in their titles, the cursor's moves through them, the
//! filter's curve in its panel, and how the four fit the author's screen.

use std::sync::Arc;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::insert::Insert;
use vt_model::samples::SamplePool;
use vt_model::selection::Motion;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song, SynthVoice, Track};
use vt_tui::app::App;
use vt_tui::input::bindings::Cmd;
use vt_tui::input::mode::Mode;
use vt_tui::track_page::{Control, DeviceCursor, Outcome, Page, TrackPage};
use vt_tui::views::status::column_name;

mod common;
use common::*;

/// The author's screen: kitty full screen on a MacBook.
const WIDTH: u16 = 163;
const HEIGHT: u16 = 42;

/// An app on the Sound page of a track playing this machine.
fn playing(machine: Machine) -> (App, Realtime) {
    let mut song = Song::new(2);
    song.tracks[0].machine = machine;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, " 4");
    (app, realtime)
}

/// The whole screen at the author's size.
fn full_screen(app: &mut App) -> Buffer {
    let backend = ratatui::backend::TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// A screen as lines of text.
fn lines(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (0..area.height)
        .map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// Where each device's title is, top to bottom, by the row it's on and how far in it starts.
fn titles(lines: &[String]) -> Vec<(usize, String)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            let at = line.find("╭─ ")?;
            let title: String = line[at + "╭─ ".len()..]
                .split(" ─")
                .next()?
                .trim()
                .to_owned();
            Some((row, title))
        })
        .filter(|(_, title)| title != "Keys")
        .collect()
}

#[test]
fn the_four_devices_fill_the_page_and_the_keys_box_at_163_by_42() {
    // A drum synth of four settings, the eight-setting `Acid`, the sampler, and 6-op, whose type
    // is a patch.
    let machines = [
        (Machine::Plaits(SynthVoice::Kick), "Machine  Kick"),
        (Machine::Plaits(SynthVoice::Acid), "Machine  Acid  Saw"),
        (Machine::Sampler, "Machine  Sampler  Off"),
        (
            Machine::Plaits(SynthVoice::SixOp),
            "Machine  6-op  E.PIANO 1",
        ),
    ];
    for (machine, title) in machines {
        let (mut app, _realtime) = playing(machine);
        app.filter_image = Some(0x01_02_03);
        for (mode, top, frame) in [(Mode::Normal, 2, ""), (Mode::Keys, 3, "│")] {
            app.mode = mode;
            let screen = full_screen(&mut app);
            let lines = lines(&screen);
            let found = titles(&lines);
            let rows: Vec<usize> = found.iter().map(|(row, _)| *row).collect();
            // Four of nine rows each fill the 36 rows from the first, the status line's two
            // under them, and the Keys box's frame under them in its mode.
            assert_eq!(rows, [top, top + 9, top + 18, top + 27], "{lines:#?}");
            assert_eq!(found[0].1, title, "{machine:?}");
            assert_eq!(found[1].1, "Filter  LP  12 dB");
            assert_eq!(found[2].1, "Output");
            assert_eq!(found[3].1, "Play");
            let bottom = top + 35;
            let line = lines[bottom].trim_start_matches(frame);
            assert!(line.starts_with('╰'), "{machine:?} {mode:?}: {lines:#?}");
            // Under it, the Keys box's frame, or nothing but the main meter beside.
            let under = &lines[bottom + 1];
            match mode {
                Mode::Keys => assert!(under.starts_with('╰'), "{under}"),
                _ => assert!(under[..100].trim().is_empty(), "{under}"),
            }
            // Every row of every device ends at its frame, so nothing runs past it.
            let row = |y: usize| y as u16;
            let edge = (0..WIDTH)
                .find(|x| screen[(*x, row(top))].symbol() == "╮")
                .unwrap();
            for y in top..=bottom {
                let end = screen[(edge, row(y))].symbol();
                assert!(["╮", "│", "╯"].contains(&end), "{}", lines[y]);
            }
        }
    }
}

#[test]
fn each_machines_device_shows_every_setting_once_and_its_type_in_its_title() {
    for machine in Machine::ALL {
        let (mut app, _realtime) = playing(machine);
        let lines = lines(&full_screen(&mut app));
        let (top, title) = titles(&lines)[0].clone();
        assert!(
            title.starts_with(&format!("Machine  {}", machine.name())),
            "{title}"
        );
        // The Machine's rows, inside its frame.
        let device = lines[top + 1..top + 8].join("\n");
        let names: Vec<&str> = machine.settings().map(SettingId::name).collect();
        for (at, name) in names.iter().enumerate() {
            let shown = device.matches(&format!("  {name} ")).count();
            if Some(at) == machine.type_param() {
                assert_eq!(shown, 0, "{machine:?}: {name} is in the title");
            } else {
                assert_eq!(shown, 1, "{machine:?}: {name} in\n{device}");
            }
        }
        // A type follows the machine in the title, as the name of what it's set to.
        let parts = title.split("  ").count();
        assert_eq!(
            parts,
            2 + usize::from(machine.type_param().is_some()),
            "{title}"
        );
    }
}

#[test]
fn j_and_k_go_through_each_devices_title_and_settings() {
    let (mut app, _realtime) = playing(Machine::Plaits(SynthVoice::Acid));
    assert_eq!(app.track_control(), Control::Machine);
    let cursor = |app: &App| {
        app.track_page
            .device_cursor(app.song().tracks.first())
            .map(|cursor| (cursor.device, cursor.row, cursor.column))
    };
    assert_eq!(cursor(&app), Some((0, 0, 0)));
    // The machine's type is beside it in the title, and `j` goes down from there.
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Wave");
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Env mod");
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Decay");
    // Acid has two rows, so the next is the Filter's title, its mode over the second column.
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::FilterMode);
    assert_eq!(cursor(&app), Some((1, 0, 1)));
    press(&mut app, "h");
    assert_eq!(app.track_control(), Control::FilterType);
    press(&mut app, "jjj");
    assert_eq!(column_name(&app), "Drive");
    // The Output has no title controls, so `j` passes its title by, and `k` back too.
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Trim);
    assert_eq!(cursor(&app), Some((2, 1, 0)));
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Drive");
    // `G` and `gg` go to the last and first rows.
    press(&mut app, "G");
    assert_eq!(app.track_control(), Control::GateLength);
    press(&mut app, "gg");
    assert_eq!(app.track_control(), Control::Machine);
}

#[test]
fn braces_go_between_devices_keeping_the_row_and_column() {
    let (mut app, _realtime) = playing(Machine::Plaits(SynthVoice::Acid));
    // From the machine's type, in the title's second place, to the filter's mode.
    press(&mut app, "l}");
    assert_eq!(app.track_control(), Control::FilterMode);
    // The Output's title is empty, so the cursor lands on its first settings.
    press(&mut app, "}");
    assert_eq!(app.track_control().label(), "Send A");
    press(&mut app, "}");
    assert_eq!(app.track_control().label(), "Speed");
    // It stops at the last, and comes back the way it went.
    press(&mut app, "}");
    assert_eq!(app.track_control().label(), "Speed");
    press(&mut app, "3{");
    assert_eq!(column_name(&app), "Wave");
    // A count and the same row: the second settings row of the machine, then the Output's.
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Decay");
    press(&mut app, "2}");
    assert_eq!(app.track_control().label(), "Send B");
    press(&mut app, "{");
    assert_eq!(app.track_control().label(), "High cut");
}

#[test]
fn the_sound_rack_moves_on_its_own_without_an_app() {
    let mut track = Track {
        machine: Machine::Plaits(SynthVoice::Kick),
        ..Track::default()
    };
    track.inserts[0] = Insert::ALL[1];
    let track = Some(&track);
    let at = |device, row, column| {
        Some(DeviceCursor {
            device,
            row,
            column,
        })
    };
    let mut page = TrackPage::default();
    assert_eq!(page.page, Page::Sound);
    let devices = page.devices(track);
    let identities: Vec<&str> = devices
        .iter()
        .map(|device| device.identity.as_str())
        .collect();
    assert_eq!(identities, ["Machine", "Filter", "Output", "Play"]);
    // Kick has no type, so its title has only the machine.
    assert_eq!(devices[0].title, [Some(Control::Machine), None, None, None]);
    assert_eq!(page.device_cursor(track), at(0, 0, 0));
    assert_eq!(page.key(Cmd::Move(Motion::Right), 1, track), Outcome::Done);
    assert_eq!(page.device_cursor(track), at(0, 0, 0), "nothing beside it");
    page.key(Cmd::Move(Motion::Down), 1, track);
    page.key(Cmd::Move(Motion::RowEnd), 1, track);
    assert_eq!(page.control(track), Control::Setting(3), "Kick's Accent");
    page.key(Cmd::Move(Motion::NextBeat), 2, track);
    assert_eq!(page.device_cursor(track), at(2, 1, 1));
    // A fixed track's Sound page is still rows of its output.
    let master = Track {
        kind: vt_model::song::TrackKind::Master,
        ..Track::default()
    };
    assert!(page.devices(Some(&master)).is_empty());
}

/// Where the filter's curve's placeholders are on a screen: each cell in its colour.
fn image_cells(buffer: &Buffer, color: Color) -> Vec<(u16, u16)> {
    let area = buffer.area;
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let cell = &buffer[(*x, *y)];
            cell.fg == color && cell.symbol().starts_with('\u{10EEEE}')
        })
        .collect()
}

#[test]
fn the_filter_curve_stays_in_its_panel_in_the_filter() {
    for mode in [Mode::Normal, Mode::Keys] {
        let (mut app, _realtime) = playing(Machine::Sampler);
        app.mode = mode;
        // Without an image, there's no box.
        let plain = full_screen(&mut app);
        assert!(!lines(&plain).iter().any(|line| line.contains('┌')));

        app.filter_image = Some(0x01_02_03);
        let drawn = full_screen(&mut app);
        let text = lines(&drawn);
        let (top, _) = titles(&text)[1];
        let cells = image_cells(&drawn, Color::Rgb(1, 2, 3));
        // Its size is as it was: 24 by 3.
        assert_eq!(cells.len(), 24 * 3);
        let rows: Vec<u16> = cells.iter().map(|(_, y)| *y).collect();
        let columns: Vec<u16> = cells.iter().map(|(x, _)| *x).collect();
        let (left, right) = (
            *columns.iter().min().unwrap(),
            *columns.iter().max().unwrap(),
        );
        let (first, last) = (*rows.iter().min().unwrap(), *rows.iter().max().unwrap());
        // In the middle of the Filter's inside, with its box a row above and below.
        assert_eq!(usize::from(first), top + 3, "{text:#?}");
        assert_eq!(usize::from(last), top + 5);
        assert_eq!(drawn[(left - 1, first - 1)].symbol(), "┌");
        assert_eq!(drawn[(right + 1, last + 1)].symbol(), "┘");
        // Against the device's right side, two columns in from its frame.
        let frame = (right + 2..WIDTH)
            .find(|x| drawn[(*x, first)].symbol() == "│")
            .unwrap();
        assert_eq!(frame, right + 4, "{}", text[usize::from(first)]);
        // Clear of the settings beside it.
        let settings = text[usize::from(first) - 1].find("Low cut").unwrap();
        assert!(
            usize::from(left) > settings + 30,
            "{}",
            text[usize::from(first) - 1]
        );
    }
}

#[test]
fn a_samplers_waveform_is_in_the_machines_panel() {
    let (mut app, _realtime) = playing(Machine::Sampler);
    app.filter_image = Some(0x01_02_03);
    app.waveform_image = Some(0x04_05_06);
    let drawn = full_screen(&mut app);
    let text = lines(&drawn);
    let found = titles(&text);
    let waveform = image_cells(&drawn, Color::Rgb(4, 5, 6));
    let filter = image_cells(&drawn, Color::Rgb(1, 2, 3));
    assert_eq!(waveform.len(), 24 * 3);
    let machine = found[0].0 as u16..found[1].0 as u16;
    let device = found[1].0 as u16..found[2].0 as u16;
    assert!(waveform.iter().all(|(_, y)| machine.contains(y)));
    assert!(filter.iter().all(|(_, y)| device.contains(y)));
    // One above the other, in the same place in each device.
    assert_eq!(waveform[0].0, filter[0].0);
    // A synth has none.
    let (mut app, _realtime) = playing(Machine::Plaits(SynthVoice::Kick));
    app.waveform_image = Some(0x04_05_06);
    assert!(image_cells(&full_screen(&mut app), Color::Rgb(4, 5, 6)).is_empty());
}

#[test]
fn acids_sound_page_at_163_by_42() {
    let (mut app, _realtime) = playing(Machine::Plaits(SynthVoice::Acid));
    app.filter_image = Some(0x01_02_03);
    press(&mut app, "}j");
    assert_eq!(column_name(&app), "Cut");
    let text = lines(&full_screen(&mut app));
    let expected = [
        (2, "╭─ Machine  Acid  Saw ─"),
        (
            4,
            "│  Cutoff    8000 ▕████▌    ▏   Env mod   8000 ▕████▌    ▏   Accent    8000 ▕████▌    ▏   Drive     0000 ▕         ▏   ",
        ),
        (
            6,
            "│  Resonance 8000 ▕████▌    ▏   Decay     8000 ▕████▌    ▏   Slide     0000 ▕         ▏   ",
        ),
        (11, "╭─ Filter  LP  12 dB ─"),
        (
            31,
            "│  Pitch     8000 ▕    │    ▏   Speed     1/16               Choke     Off ",
        ),
        (33, "│  Glide     Slide              Groove    Straight"),
        (35, "│  Gate      50%                Reset     Off"),
    ];
    for (row, line) in expected {
        assert!(text[row].starts_with(line), "{row}: {}", text[row]);
    }
}
