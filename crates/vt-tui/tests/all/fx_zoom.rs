//! The FX page zoomed in on one slot, `z i` (decision 329): the strip of every slot, the zoomed
//! slot's big graphics over its settings, the gain reduction history, and the images it sends.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::track_state::LiveValues;
use vt_tui::app::App;
use vt_tui::input::mode::Mode;
use vt_tui::track_page::Page;

use crate::common::*;
use vt_tui::input::mode::View;
use vt_tui::kitty::{PanelIds, PanelKey};

/// The screen at the author's 163 by 42, with the keyboard's mode or not.
fn zoomed_screen(app: &mut App, keys: bool) -> Vec<String> {
    app.modes.mode = if keys { Mode::Keys } else { Mode::Normal };
    let screen = screen_cells(app);
    app.modes.mode = Mode::Normal;
    screen
}

/// The characters of a line from one column to another, not counting the last.
fn cut(line: &str, from: usize, to: usize) -> String {
    line.chars().skip(from).take(to - from).collect()
}

#[test]
fn z_i_zooms_in_on_the_cursors_slot_under_a_strip_of_them_all_and_z_o_goes_back() {
    let (mut app, _realtime) = on_fx_page();
    four_devices(&mut app);
    app.track_page.images.insert(PanelKey::Zoom, 0x01_02_03);
    press(&mut app, "zi");
    assert!(app.track_page.zoomed);
    assert!(app.fx_zoomed());
    let screen = zoomed_screen(&mut app, false);
    // The strip: a tile for each slot, 36 wide with 2 between, filling the page's 150 columns,
    // each with its switch, its effect and its type, and the first row of its settings by their
    // short labels, a compressor's with its gain reduction at the end.
    for (slot, x) in [0, 38, 76, 114].into_iter().enumerate() {
        let tile = cut(&screen[2], x, x + 36);
        assert!(
            tile.starts_with(&slot_title(slot, Some(true), "")),
            "{tile}"
        );
        assert!(cut(&screen[2], x, x + 36).ends_with('╮'), "{tile}");
        assert!(cut(&screen[4], x, x + 36).starts_with('╰'));
    }
    assert!(screen[2].contains("Comp  Glue"), "{}", screen[2]);
    assert!(screen[2].contains("Distort  Overdrive"), "{}", screen[2]);
    let glue = cut(&screen[3], 0, 36);
    assert!(glue.starts_with("│ THRS C0  "), "{glue}");
    assert!(glue.ends_with("GR       │"), "{glue}");
    // Then the slot, its frame the page's width, its title as the rack's.
    assert!(screen[6].starts_with(&slot_title(0, Some(true), "Comp  Glue ─")));
    assert_eq!(screen[6].chars().nth(149), Some('╮'));
    // Its curve, 42 by 16, its meter with its scale, and the last two bars in their own box.
    let rule = |width: usize| "─".repeat(width);
    assert_eq!(
        cut(&screen[8], 0, 150),
        format!("│  ┌{}┐  ┌──┐      ┌{}┐    │", rule(42), rule(84))
    );
    assert_eq!(
        cut(&screen[9], 0, 60),
        format!("│  │{}│  │▔▔│   0  │", "▒".repeat(42))
    );
    assert_eq!(cut(&screen[13], 49, 59), "│  │  -6  ");
    assert_eq!(cut(&screen[21], 49, 59), "│  │ -18  ");
    assert_eq!(
        cut(&screen[25], 0, 150),
        format!("│  └{}┘  └GR┘      └2 bars{}┘    │", rule(42), rule(78))
    );
    // The settings below, with bars of 17 cells.
    assert!(screen[27].starts_with("│  Threshold"), "{}", screen[27]);
    assert!(
        screen[27].contains("Mix       FF ▕█████████████████▏"),
        "{}",
        screen[27]
    );
    assert!(screen[33].starts_with('╰'), "{}", screen[33]);
    assert!(
        screen[34..40]
            .iter()
            .all(|line| cut(line, 0, 150).trim().is_empty())
    );
    // The badge says so, and the line under it how to go on.
    let status = status_line(&app);
    assert!(status.starts_with(" ZOOM "), "{status}");
    let hint = screen[41].trim();
    assert_eq!(hint, "Slot 1 zoomed in: { } another slot, z o every slot");

    // In the keyboard's box, a column and a row in, it all still fits.
    let boxed = zoomed_screen(&mut app, true);
    assert!(boxed[3].starts_with(&format!("│{}", slot_title(0, Some(true), ""))));
    assert!(boxed[7].starts_with(&format!("│{}", slot_title(0, Some(true), ""))));
    assert!(cut(&boxed[9], 0, 150).ends_with("┐  ││"), "{}", boxed[9]);
    assert!(boxed[34].starts_with("│╰"), "{}", boxed[34]);
    assert!(boxed[39].starts_with('╰'));

    // `}` zooms in on the next slot, keeping the setting: an EQ's curve across the whole width.
    press(&mut app, "}");
    assert!(app.track_page.zoomed);
    let screen = zoomed_screen(&mut app, false);
    assert!(screen[6].starts_with(&slot_title(1, Some(true), "EQ3 ─")));
    assert_eq!(cut(&screen[8], 0, 150), format!("│  ┌{}┐    │", rule(140)));
    assert_eq!(
        cut(&screen[9], 0, 150),
        format!("│  │{}│    │", "▒".repeat(140))
    );
    assert_eq!(
        app.fx_zoom_hint().unwrap(),
        "Slot 2 zoomed in: { } another slot, z o every slot"
    );
    // A pedal has nothing to draw: its settings alone, as tall as a device in the rack.
    app.track_page.images = PanelIds::default();
    press(&mut app, "}");
    let screen = zoomed_screen(&mut app, false);
    assert!(screen[6].contains("Distort  Overdrive"), "{}", screen[6]);
    assert!(screen[8].starts_with("│  Amount"), "{}", screen[8]);
    assert!(screen[14].starts_with('╰'), "{}", screen[14]);
    // `j` past its last row goes on into the next slot's title, which is zoomed in on then.
    press(&mut app, "jjj");
    assert!(zoomed_screen(&mut app, false)[6].contains("Delay"));

    // `z o` goes back to the rack, as does `z i` again.
    press(&mut app, "zo");
    assert!(!app.track_page.zoomed);
    let rack = zoomed_screen(&mut app, false);
    for slot in 0..4 {
        assert!(rack[2 + 9 * slot].starts_with(&slot_title(slot, Some(true), "")));
    }
    assert!(!status_line(&app).starts_with(" ZOOM "));
    press(&mut app, "zizi");
    assert!(!app.track_page.zoomed);

    // Only the FX page zooms, and it's kept zoomed while another page shows.
    press(&mut app, "zi");
    press(&mut app, "H");
    assert_eq!(app.track_page.page, Page::Mod);
    assert!(!app.fx_zoomed());
    assert!(!status_line(&app).starts_with(" ZOOM "));
    press(&mut app, "zi");
    assert_eq!(
        app.services.status,
        "z i zooms in on an effect, on the FX page"
    );
    press(&mut app, "L");
    assert!(app.fx_zoomed());
}

#[test]
fn an_empty_slots_tile_in_the_strip_says_empty_dim() {
    let (mut app, _realtime) = on_fx_page();
    // A Glue in the first slot, and the others empty.
    press(&mut app, "KComp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "l==h");
    press(&mut app, "zi");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row: String = (0..163)
        .map(|x| buffer[(x, 2)].symbol())
        .collect::<String>();
    let at = row.find("Empty").expect("an empty tile") as u16;
    let x = row[..at as usize].chars().count() as u16;
    assert_eq!(buffer[(x, 2)].fg, vt_tui::theme::OVERLAY2, "{row}");
    let glue = row[..row.find("Glue").unwrap()].chars().count() as u16;
    assert_ne!(buffer[(glue, 2)].fg, vt_tui::theme::OVERLAY2, "{row}");
}

#[test]
fn the_zoomed_meter_and_its_two_bars_show_how_far_it_turned_down() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "zi");
    // 12 dB is half the meter's 24: its top eight rows.
    app.services.live = Some(LiveValues {
        reduction: [12.0, 0.0, 0.0, 0.0],
        ..LiveValues::default()
    });
    let screen = zoomed_screen(&mut app, false);
    // With no curve sent yet its box is left out, so the meter starts the panel.
    assert_eq!(cut(&screen[8], 3, 7), "┌──┐");
    for (row, line) in screen.iter().enumerate().take(17).skip(9) {
        assert_eq!(cut(line, 4, 6), "██", "{row}");
    }
    assert_eq!(cut(&screen[17], 4, 6), "  ");
    // The tile's bar: 12 dB of its 20 is three of its five cells.
    let tile = cut(&screen[3], 0, 36);
    assert!(tile.ends_with("GR ███   │"), "{tile}");

    // The history fills as the frames come in: a second at 120 BPM is a quarter of its two bars.
    // With no audio running there are no reports, so the last values stay.
    app.services.live = Some(LiveValues {
        reduction: [6.0, 0.0, 0.0, 0.0],
        ..LiveValues::default()
    });
    let start = Instant::now();
    app.tick_at(start);
    for frame in 1..=30 {
        app.tick_at(start + Duration::from_millis(33 * frame));
    }
    let width = 84;
    let head = app.track_page.reduction_history.head(width);
    assert!((20..23).contains(&head), "{head}");
    assert_eq!(app.track_page.reduction_history.column(0, 5, width), 6.0);
    let screen = zoomed_screen(&mut app, false);
    let history = |row: usize| cut(&screen[row], 14, 14 + width);
    // 6 dB is a quarter of 24: the top four rows, under the sweep's columns so far, then the gap
    // after the newest.
    for row in 9..13 {
        let line = history(row);
        assert!(line.starts_with(&"█".repeat(head)), "{row}: {line}");
        assert_eq!(line.chars().nth(head + 1), Some(' '));
    }
    assert!(history(13).trim().is_empty(), "{}", history(13));

    // Bypassed, it's faint.
    press(&mut app, "b");
    let buffer = {
        let backend = ratatui::backend::TestBackend::new(150, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::run::draw_view(frame, frame.area(), &app, View::Track))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    assert_eq!(buffer[(14, 9)].symbol(), "█");
    assert_eq!(buffer[(14, 9)].fg, vt_tui::theme::OVERLAY0);
}

#[test]
fn zoomed_in_only_its_slot_is_drawn_bigger_and_a_shaping_distortion_has_its_curve() {
    use std::collections::HashMap;
    let synced = |app: &mut App, graphics: &mut vt_tui::kitty::Graphics| -> HashMap<u32, Vec<u8>> {
        let mut out = Vec::new();
        app.send_pictures(graphics, &mut out).unwrap();
        vt_tui::screenshot::sent_images(&out)
    };
    let (mut app, _realtime) = on_fx_page();
    four_devices(&mut app);
    let mut graphics = vt_tui::kitty::Graphics::new();
    synced(&mut app, &mut graphics);
    assert!(app.track_page.images.get(PanelKey::Slot(0)).is_some());
    assert_eq!(app.track_page.images.get(PanelKey::Zoom), None);
    press(&mut app, "zi");
    // At the author's 15 by 39 pixels a character, the compressor's curve is nearly square, and
    // it's the only one sent.
    let sent = synced(&mut app, &mut graphics);
    assert_eq!(sent.len(), 1);
    let comp = app.track_page.images.get(PanelKey::Zoom).unwrap();
    assert_eq!(png_size(&sent[&comp]), (630, 624));
    assert_eq!(slot_images(&app), [None; 4]);
    assert!(synced(&mut app, &mut graphics).is_empty(), "sent once");
    // The EQ's is the device's width.
    press(&mut app, "}");
    let sent = synced(&mut app, &mut graphics);
    assert_eq!(
        png_size(&sent[&app.track_page.images.get(PanelKey::Zoom).unwrap()]),
        (2100, 624)
    );
    // An Overdrive has none, and Soft, which shapes each sample alone, has its transfer curve.
    press(&mut app, "}");
    assert!(synced(&mut app, &mut graphics).is_empty());
    assert_eq!(app.track_page.images.get(PanelKey::Zoom), None);
    press(&mut app, "l");
    press(&mut app, "K");
    press(&mut app, "Soft");
    special(&mut app, KeyCode::Enter);
    let sent = synced(&mut app, &mut graphics);
    assert_eq!(
        png_size(&sent[&app.track_page.images.get(PanelKey::Zoom).unwrap()]),
        (630, 624)
    );
    let screen = zoomed_screen(&mut app, false);
    assert_eq!(cut(&screen[9], 0, 48), format!("│  │{}│ ", "▒".repeat(42)));
    // Back in the rack, the rack's own come back.
    press(&mut app, "zo");
    synced(&mut app, &mut graphics);
    assert!(
        app.track_page.images.get(PanelKey::Slot(0)).is_some()
            && app.track_page.images.get(PanelKey::Slot(1)).is_some()
    );
    assert_eq!(app.track_page.images.get(PanelKey::Zoom), None);
}

#[test]
fn the_filter_effects_have_their_curves_at_the_note_played_and_as_far_as_a_gate_is_open() {
    use std::collections::HashMap;
    let synced = |app: &mut App, graphics: &mut vt_tui::kitty::Graphics| -> HashMap<u32, Vec<u8>> {
        let mut out = Vec::new();
        app.send_pictures(graphics, &mut out).unwrap();
        vt_tui::screenshot::sent_images(&out)
    };
    let (mut app, _realtime) = on_fx_page();
    for effect in ["LPG", "Reso", "Comb", "Vowel"] {
        press(&mut app, "K");
        press(&mut app, effect);
        special(&mut app, KeyCode::Enter);
        press(&mut app, "}");
    }
    press(&mut app, "gg");
    let mut graphics = vt_tui::kitty::Graphics::new();
    synced(&mut app, &mut graphics);
    press(&mut app, "zi");
    // Each is a frequency curve the device's width, as the EQ's is, sent once.
    for slot in 0..4 {
        let sent = synced(&mut app, &mut graphics);
        assert_eq!(sent.len(), 1, "slot {slot}");
        assert_eq!(
            png_size(&sent[&app.track_page.images.get(PanelKey::Zoom).unwrap()]),
            (2100, 624)
        );
        assert!(synced(&mut app, &mut graphics).is_empty(), "slot {slot}");
        press(&mut app, "}");
    }
    // What the slots play with, as the engine reports it.
    let inserts = app.song().tracks[0].inserts;
    let live = |note: f32, opening: (f32, f32)| LiveValues {
        inserts: inserts.map(|insert| std::array::from_fn(|param| insert.value(param))),
        openings: [Some(opening), None, None, None],
        note,
        ..LiveValues::default()
    };
    // The resonator's follows the note the track plays. A change so soon after the last is sent a
    // frame later, so each is given two.
    let mut twice = |app: &mut App| {
        let mut sent = synced(app, &mut graphics);
        sent.extend(synced(app, &mut graphics));
        sent
    };
    press(&mut app, "gg}");
    app.services.live = Some(live(60.0, (0.0, 0.1)));
    twice(&mut app);
    let at_c = app.track_page.images.get(PanelKey::Zoom).unwrap();
    app.services.live = Some(live(67.0, (0.0, 0.1)));
    assert_eq!(twice(&mut app).len(), 1);
    assert_ne!(app.track_page.images.get(PanelKey::Zoom).unwrap(), at_c);
    // The gate's follows how far it's open, but not a step finer than it's drawn.
    press(&mut app, "{");
    app.services.live = Some(live(67.0, (0.0, 0.25)));
    twice(&mut app);
    let closed = app.track_page.images.get(PanelKey::Zoom).unwrap();
    app.services.live = Some(live(67.0, (0.0, 0.25 + 1e-4)));
    assert!(twice(&mut app).is_empty());
    assert_eq!(app.track_page.images.get(PanelKey::Zoom), Some(closed));
    app.services.live = Some(live(67.0, (1.0, 0.6)));
    assert_eq!(twice(&mut app).len(), 1);
    assert_ne!(app.track_page.images.get(PanelKey::Zoom), Some(closed));
}

#[test]
fn a_delay_has_its_repeats_and_each_reverb_its_tail_with_their_time_scales() {
    use std::collections::HashMap;
    let synced = |app: &mut App, graphics: &mut vt_tui::kitty::Graphics| -> HashMap<u32, Vec<u8>> {
        let mut out = Vec::new();
        app.send_pictures(graphics, &mut out).unwrap();
        vt_tui::screenshot::sent_images(&out)
    };
    let (mut app, _realtime) = on_fx_page();
    for effect in ["Delay", "Reverb", "Plate", "Spring"] {
        press(&mut app, "K");
        press(&mut app, effect);
        special(&mut app, KeyCode::Enter);
        press(&mut app, "}");
    }
    press(&mut app, "gg");
    let mut graphics = vt_tui::kitty::Graphics::new();
    synced(&mut app, &mut graphics);
    press(&mut app, "zi");
    for (slot, said) in [
        "s across, a line a beat",
        "falls 60 dB in",
        "falls 60 dB in",
        "falls 60 dB in",
    ]
    .into_iter()
    .enumerate()
    {
        let sent = synced(&mut app, &mut graphics);
        assert_eq!(sent.len(), 1, "slot {slot}");
        assert_eq!(
            png_size(&sent[&app.track_page.images.get(PanelKey::Zoom).unwrap()]),
            (2100, 624)
        );
        assert!(synced(&mut app, &mut graphics).is_empty(), "slot {slot}");
        // Its time scale is under its box.
        let screen = zoomed_screen(&mut app, false);
        assert!(
            screen.iter().any(|line| line.contains(said)),
            "slot {slot}: {screen:#?}"
        );
        press(&mut app, "}");
    }
}

#[test]
fn a_space_echo_has_its_repeats_drawn_as_a_delays_are() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Space Echo");
    special(&mut app, KeyCode::Enter);
    let mut graphics = vt_tui::kitty::Graphics::new();
    let mut out = Vec::new();
    app.send_pictures(&mut graphics, &mut out).unwrap();
    press(&mut app, "zi");
    let mut out = Vec::new();
    app.send_pictures(&mut graphics, &mut out).unwrap();
    let sent = vt_tui::screenshot::sent_images(&out);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        png_size(&sent[&app.track_page.images.get(PanelKey::Zoom).unwrap()]),
        (2100, 624)
    );
    let screen = zoomed_screen(&mut app, false);
    assert!(
        screen
            .iter()
            .any(|line| line.contains("s across, a line a beat")),
        "{screen:#?}"
    );
}
