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
use vt_model::settings::{FOUR_OP_SHARED, SettingId};
use vt_model::song::{Machine, Song, SynthVoice, Track, TrackKind};
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
        // 4-op's operators' settings are on devices of their own, which `track_page`'s
        // `four_op_has_a_device_for_each_operator_and_the_page_goes_down_to_them` reads.
        let own = if machine.operators().is_empty() {
            names.len()
        } else {
            FOUR_OP_SHARED
        };
        for (at, name) in names.iter().enumerate().take(own) {
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
    // The filter's second column has `Safe` on that row, beside `Res`.
    press(&mut app, "{");
    assert_eq!(app.track_control().label(), "Safe");
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
    // A fixed track's Sound page is its Output alone, with holes where it lacks a setting.
    let master = Track {
        kind: vt_model::song::TrackKind::Master,
        ..Track::default()
    };
    let devices = page.devices(Some(&master));
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].identity, "Output");
    assert_eq!(
        devices[0].grid,
        [
            [Some(Control::Trim), Some(Control::Limiter), None, None],
            [Some(Control::Level), None, None, None],
            [None; 4],
        ]
    );
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
        // Without an image, there's no box in the Filter.
        let plain = lines(&full_screen(&mut app));
        let (top, _) = titles(&plain)[1];
        assert!(!plain[top..top + 9].iter().any(|line| line.contains('┌')));

        app.filter_image = Some(0x01_02_03);
        let drawn = full_screen(&mut app);
        let text = lines(&drawn);
        let cells = image_cells(&drawn, Color::Rgb(1, 2, 3));
        // The large panel's size: 49 by 5.
        assert_eq!(cells.len(), 49 * 5);
        let rows: Vec<u16> = cells.iter().map(|(_, y)| *y).collect();
        let columns: Vec<u16> = cells.iter().map(|(x, _)| *x).collect();
        let (left, right) = (
            *columns.iter().min().unwrap(),
            *columns.iter().max().unwrap(),
        );
        let (first, last) = (*rows.iter().min().unwrap(), *rows.iter().max().unwrap());
        // All of the Filter's inside, with its box a row above and below.
        assert_eq!(usize::from(first), top + 2, "{text:#?}");
        assert_eq!(usize::from(last), top + 6);
        assert_eq!(drawn[(left - 1, first - 1)].symbol(), "┌");
        assert_eq!(drawn[(right + 1, last + 1)].symbol(), "┘");
        // Against the device's right side, two columns in from its frame.
        let frame = (right + 2..WIDTH)
            .find(|x| drawn[(*x, first)].symbol() == "│")
            .unwrap();
        assert_eq!(frame, right + 4, "{}", text[usize::from(first)]);
        // Clear of the settings beside it.
        let settings = text[usize::from(first)]
            .chars()
            .position(|c| c == 'L')
            .unwrap();
        assert!(
            usize::from(left) > settings + 30,
            "{}",
            text[usize::from(first)]
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
    assert_eq!(waveform.len(), 49 * 5);
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

/// The inside of the box in a device's panel, row by row, as text, for the device whose title is
/// at `top`.
fn panel_rows(text: &[String], top: usize) -> Vec<String> {
    let line: Vec<char> = text[top + 1].chars().collect();
    let left = line.iter().position(|c| *c == '┌').unwrap();
    let right = line.iter().position(|c| *c == '┐').unwrap();
    (top + 2..top + 7)
        .map(|row| {
            let chars: Vec<char> = text[row].chars().collect();
            chars[left + 1..right].iter().collect()
        })
        .collect()
}

#[test]
fn the_output_meter_shows_each_side_after_the_trim_with_its_held_peak_and_scale() {
    use vt_tui::meter::{FLOOR_DB, Held};
    for mode in [Mode::Normal, Mode::Keys] {
        let (mut app, _realtime) = playing(Machine::Plaits(SynthVoice::Kick));
        app.mode = mode;
        // Silent, the bars are empty and only the scale shows.
        let text = lines(&full_screen(&mut app));
        let (top, title) = titles(&text)[2].clone();
        assert_eq!(title, "Output");
        let rows = panel_rows(&text, top);
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|row| row.chars().count() == 49));
        assert_eq!(rows[1], format!(" L {}", " ".repeat(46)));
        assert_eq!(rows[3], "         -48     -36     -24     -12       0     ");
        assert!(rows[0].trim().is_empty() && rows[4].trim().is_empty());

        // At -12 dB now, after -6 dB a moment ago: 32 cells of 1.5 dB from -60, and the held
        // mark in the cell -6 dB is in.
        let mut held = Held::new(FLOOR_DB);
        held.update(-6.0, 0.0);
        held.update(-12.0, 1.0);
        assert_eq!((held.now, held.top), (-12.0, -6.0));
        let mut quieter = Held::new(FLOOR_DB);
        quieter.update(-6.0, 0.0);
        quieter.update(-13.5, 1.0);
        app.after_trim = [held, quieter];
        let text = lines(&full_screen(&mut app));
        let rows = panel_rows(&text, top);
        let left = format!(" L {}   ▏{}", "█".repeat(32), " ".repeat(10));
        assert_eq!(rows[1], left);
        // A dB and a half less is a cell less, with the same mark.
        let right = format!(" R {}    ▏{}", "█".repeat(31), " ".repeat(10));
        assert_eq!(rows[2], right);
        // Clear of the settings beside it, in the device, against its right side.
        let line = &text[top + 2];
        let group = line.find("Send A").unwrap();
        let box_left = text[top + 1].chars().position(|c| c == '┌').unwrap();
        assert!(box_left > line[..group].chars().count() + 29, "{line}");
    }
}

#[test]
fn the_output_meter_reads_the_cursors_track_after_its_trim_as_it_plays() {
    use vt_tui::meter::FLOOR_DB;
    let mut song = Song::new(2);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::Kick);
    *Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap())
        .step_mut(0)
        .unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    let heard = |trim: u8, level: u8| {
        let mut song = song.clone();
        song.tracks[0].trim = trim;
        song.tracks[0].level = level;
        let samples = Arc::new(SamplePool::default());
        let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
        let (handle, mut realtime) = connect(Engine::new(48_000, version));
        let mut app = App::new(song, None, samples, handle, 48_000);
        press(&mut app, " 4");
        app.handle
            .send(vt_engine::link::Command::PlayFrom { step: 0 });
        let mut out = vec![0.0; 2048 * 2];
        realtime.process(&mut out);
        app.tick();
        app.after_trim.map(|side| side.now)
    };
    let plain = heard(0x80, 0xE0);
    assert!(plain[0] > -30.0 && plain[0] < 6.0, "{plain:?}");
    // 12 dB more trim reads 12 dB more, and the fader, after it, changes nothing.
    let hot = heard(0x80 + 48, 0x00);
    assert!((hot[0] - plain[0] - 12.0).abs() < 0.01, "{plain:?} {hot:?}");
    assert!(plain[0] > FLOOR_DB);
}

/// What the moving filter curve costs: an LFO sweeping `Cut` over a bar, with a note on every
/// step, run as the program runs it, a frame of audio, a tick and a sync every 33 ms. It prints
/// what it measures with `--nocapture`, for decision 153.
#[test]
fn the_filter_curve_under_an_lfo_on_cut_is_sent_at_most_every_other_frame() {
    use vt_model::constants::{FRAME_MS, TICKS_PER_QUARTER};
    use vt_model::lfo::{Lfo, Rate, Shape};
    use vt_model::modulation::ModSlot;
    use vt_tui::kitty::Graphics;
    let mut song = Song::new(2);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
    song.tracks[0].cutoff = 0x80;
    song.tracks[0].resonance = 0xA0;
    song.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::CUT),
        Lfo {
            shape: Shape::Tri,
            rate: Rate::Note(TICKS_PER_QUARTER as u32 * 4),
            ..Lfo::default()
        },
    )
    .with_depth(0x60);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for step in 0..16 {
        *phrase.step_mut(step).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, " 4");
    app.handle
        .send(vt_engine::link::Command::PlayFrom { step: 0 });
    let mut graphics = Graphics::new();
    let frames = 4 * 1000 / FRAME_MS as usize;
    let audio = 48_000 * FRAME_MS as usize / 1000;
    let mut out = vec![0.0; audio * 2];
    let (mut sends, mut bytes, mut written, mut pngs) = (0, 0, 0, Vec::new());
    let mut syncing = std::time::Duration::ZERO;
    for _ in 0..frames {
        realtime.process(&mut out);
        app.tick();
        let mut sent = Vec::new();
        let started = Instant::now();
        graphics.sync(&mut app, &mut sent).unwrap();
        syncing += started.elapsed();
        let text = String::from_utf8_lossy(&sent);
        sends += text.matches(",c=49,r=5,").count();
        // Only the curve's commands, its first and the rest of its pieces, as the scope over the
        // main meter is sent in the same frames.
        let mut curve = false;
        for command in text.split("\x1b_G").skip(1) {
            if command.starts_with("a=T") {
                curve = command.contains(",c=49,r=5,");
            } else if command.starts_with("a=") {
                continue;
            }
            if curve {
                bytes += command.len() + "\x1b_G".len();
            }
        }
        written += sent.len();
        let images = vt_tui::screenshot::sent_images(&sent);
        pngs.extend(
            app.filter_image
                .and_then(|id| images.get(&id))
                .map(Vec::len),
        );
    }
    let seconds = frames as f32 * FRAME_MS as f32 / 1000.0;
    let average = pngs.iter().sum::<usize>() / pngs.len().max(1);
    eprintln!(
        "the filter curve under an LFO on Cut: {sends} sends in {seconds:.1} s, {:.1} a second; \
         PNGs of {} to {} bytes, {average} on average; {:.1} KB a second sent for it, of \
         {:.1} KB with the scope; {:.2} ms a frame to draw and send",
        sends as f32 / seconds,
        pngs.iter().min().unwrap_or(&0),
        pngs.iter().max().unwrap_or(&0),
        bytes as f32 / seconds / 1024.0,
        written as f32 / seconds / 1024.0,
        syncing.as_secs_f32() * 1000.0 / frames as f32,
    );
    // It moves, so it's sent, but never two frames running: at most 15 times a second.
    assert!(sends > frames / 4, "{sends} of {frames}");
    assert!(sends <= frames / 2 + 1, "{sends} of {frames}");
    assert!(average < 8000, "{average}");
}

/// An app on the Sound page of a fixed track, on its `Level`, in a song of six named tracks,
/// three of them in group 1, with tracks 03 to 05 and the group sending to return B.
fn on_fixed(kind: TrackKind) -> (App, Realtime) {
    let mut song = Song::new(6);
    for (at, name) in ["Kick", "Snare", "Hats", "Perc", "Pad", "Bass"]
        .into_iter()
        .enumerate()
    {
        song.tracks[at].name = name.to_owned();
    }
    for track in [0, 2, 3] {
        song.tracks[track].group = Some(1);
    }
    for (track, send) in [(2, 0x0C), (3, 0x30), (4, 0x60)] {
        song.tracks[track].send_b = send;
    }
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    song.tracks[group].send_b = 0x18;
    let at = song.fixed_track(kind).unwrap();
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    app.cursor.track = at;
    press(&mut app, " 4gg0j");
    (app, realtime)
}

#[test]
fn a_fixed_tracks_output_and_what_feeds_it_fit_the_page_and_the_keys_box_at_163_by_42() {
    let fixed = [
        (TrackKind::Group(1), Some("Tracks")),
        (TrackKind::ReturnB, Some("Sends in")),
        (TrackKind::Master, None),
    ];
    for (kind, feed) in fixed {
        let (mut app, _realtime) = on_fixed(kind);
        assert_eq!(app.track_control(), Control::Level);
        for (mode, top, frame) in [(Mode::Normal, 2, ""), (Mode::Keys, 3, "│")] {
            app.mode = mode;
            let screen = full_screen(&mut app);
            let lines = lines(&screen);
            let found = titles(&lines);
            // The Output at the top, as a sound track's Machine is, and what feeds it under.
            let expected: Vec<(usize, String)> = std::iter::once((top, "Output".to_owned()))
                .chain(feed.map(|feed| (top + 9, feed.to_owned())))
                .collect();
            assert_eq!(found, expected, "{kind:?} {mode:?}: {lines:#?}");
            // The Output's frame is the brighter, with the cursor in it; the other's isn't.
            let corner = |y: usize| {
                let x = lines[y].chars().position(|c| c == '╭').unwrap();
                screen[(x as u16, y as u16)].fg
            };
            assert_eq!(corner(top), vt_tui::theme::OVERLAY2);
            if feed.is_some() {
                assert_eq!(corner(top + 9), vt_tui::theme::SURFACE1);
            }
            // Every row ends at the frame, as the Sound page's devices do.
            let edge = (0..WIDTH)
                .find(|x| screen[(*x, top as u16)].symbol() == "╮")
                .unwrap();
            let bottom = top + if feed.is_some() { 17 } else { 8 };
            for y in top..=bottom {
                let end = screen[(edge, y as u16)].symbol();
                assert!(["╮", "│", "╯"].contains(&end), "{}", lines[y]);
            }
            assert!(lines[bottom].trim_start_matches(frame).starts_with('╰'));
            // A group's and a return's Output has the meter after its trim; the master's has
            // none, as the main meter is beside it.
            let boxed = lines[top + 1].contains('┌');
            assert_eq!(
                boxed,
                kind != TrackKind::Master,
                "{kind:?}: {}",
                lines[top + 1]
            );
            if kind != TrackKind::Master {
                assert!(lines[top + 5].contains("-48"), "{}", lines[top + 5]);
            }
        }
    }
}

#[test]
fn the_masters_limiter_is_a_toggle_after_its_level_and_switches_the_songs_with_undo() {
    use vt_tui::views::status::value_shown;
    use vt_tui::views::widgets::toggle;
    let (mut app, _realtime) = on_fixed(TrackKind::Master);
    // The toggle where the value would be, after the name's room, in its own colour.
    let limiter = |app: &mut App| {
        let screen = full_screen(app);
        let lines = lines(&screen);
        let line = &lines[4];
        let at = line.find("Limiter").expect("beside Trim");
        let x = line[..at].chars().count() + 10;
        let cell = &screen[(x as u16, 4)];
        (cell.symbol().chars().next().unwrap(), cell.fg)
    };
    assert!(app.song().limiter);
    assert_eq!(
        limiter(&mut app),
        (toggle::glyph(true), vt_tui::theme::GREEN)
    );
    // Beside `Trim`, after `Level` down the columns, as the master plays them.
    press(&mut app, "kl");
    assert_eq!(app.track_control(), Control::Limiter);
    assert_eq!(column_name(&app), "Limiter");
    assert_eq!(value_shown(&app).as_deref(), Some("on"));
    // `K` switches it, as one undo step, and the status line says what off means.
    let depth = app.history.undo_depth();
    press(&mut app, "K");
    assert!(!app.song().limiter);
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert!(app.status.starts_with("Limiter off"), "{}", app.status);
    assert_eq!(value_shown(&app).as_deref(), Some("off"));
    press(&mut app, "h");
    assert_eq!(
        limiter(&mut app),
        (toggle::glyph(false), vt_tui::theme::OVERLAY0)
    );
    press(&mut app, "l");
    press(&mut app, "u");
    assert!(app.song().limiter, "undone");
    press(&mut app, "-");
    assert!(!app.song().limiter, "- takes it off");
    let depth = app.history.undo_depth();
    press(&mut app, "-");
    assert_eq!(app.history.undo_depth(), depth, "already off");
    press(&mut app, "=");
    assert!(app.song().limiter, "= puts it on");
    press(&mut app, "i");
    assert!(!app.song().limiter, "i switches it too");
    assert_eq!(app.mode, Mode::Normal);
    press(&mut app, "x");
    assert!(app.song().limiter, "x puts it back as a new song has it");
    press(&mut app, "u");
    assert!(!app.song().limiter);
    press(&mut app, "u");
    assert!(app.song().limiter);
    // Only the master has it.
    for kind in [TrackKind::Group(1), TrackKind::ReturnB] {
        let (app, _realtime) = on_fixed(kind);
        let devices = app.track_page.devices(app.track_here());
        let mut places = devices[0].rows().flatten();
        assert!(!places.any(|place| place == Some(Control::Limiter)));
    }
}
