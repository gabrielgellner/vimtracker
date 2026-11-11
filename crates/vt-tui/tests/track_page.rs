//! The track page, `Space 4`: its Sound and Mod pages' settings and what each key does to
//! them, the cursor's moves through their rows, `/`'s jumps, and their drawings: the bars, the
//! filter's curve, the envelopes' shapes and the sample's waveform. The FX page is
//! `fx_page.rs`.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::constants::MAX_GROUPS;
use vt_model::groove::GrooveId;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::samples::SamplePool;
use vt_model::settings::{SettingId, gate_value};
use vt_model::song::{FilterType, Machine, Song, SynthVoice, TrackKind};
use vt_tui::app::App;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

/// Go to a track that's shown with `W` or `B`, as many times as there are shown tracks between,
/// as they step over the folded ones.
fn to_track(app: &mut App, track: usize) {
    let shown: Vec<usize> = app.folds.shown(app.song()).collect();
    let place = |track: usize| shown.iter().position(|&shown| shown == track).unwrap();
    let (from, to) = (place(app.cursor.track), place(track));
    let key = if to > from { "W" } else { "B" };
    press(app, &key.repeat(from.abs_diff(to)));
    assert_eq!(app.cursor.track, track);
}

#[test]
fn a_fixed_tracks_pages_only_show_what_it_has() {
    let (mut app, _rt) = app();
    to_track(&mut app, 2 + MAX_GROUPS);
    press(&mut app, " 4");
    let lines = track_page_text(&app);
    let fixed = format!("{} Return A", vt_tui::glyphs::RETURN);
    assert!(lines[0].starts_with(&fixed), "{}", lines[0]);
    assert!(lines[2].starts_with("Trim"), "{}", lines[2]);
    assert!(lines[3].starts_with("Level"), "{}", lines[3]);
    assert!(lines[4].starts_with("Pan"), "{}", lines[4]);
    assert!(!lines.iter().any(|line| line.contains("Machine")));
    press(&mut app, "WW");
    let lines = track_page_text(&app);
    let master = format!("{} Master", vt_tui::glyphs::MASTER);
    assert!(lines[0].starts_with(&master), "{}", lines[0]);
    assert!(lines[2].starts_with("Trim"), "{}", lines[2]);
    assert!(lines[3].starts_with("Level"), "{}", lines[3]);
    assert!(!lines.iter().any(|line| line.contains("Pan")));
}

/// The track page as lines of text, as big as it is on the author's screen: 150 by 40, beside the
/// main meter and above the status line.
fn track_text(app: &App) -> Vec<String> {
    let (width, height) = (150, 40);
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
        .collect()
}

/// The rest of the track page's line from a setting's name on, as a Sound device's cell starts:
/// its name, its value and its bar, then the cells after it.
fn from_name(app: &App, name: &str) -> String {
    let lines = track_text(app);
    let cell = format!("  {name} ");
    lines
        .iter()
        .find_map(|line| Some(line[line.find(&cell)? + 2..].to_owned()))
        .unwrap_or_else(|| panic!("{name} in {lines:#?}"))
}

/// The track page's line with a device's title on it.
fn title_line(app: &App, identity: &str) -> String {
    let lines = track_text(app);
    let title = format!("╭─ {identity} ");
    lines
        .iter()
        .find(|line| line.starts_with(&title))
        .cloned()
        .unwrap_or_else(|| panic!("{identity} in {lines:#?}"))
}

/// The value of the track's machine's setting at this place.
fn machine_value(app: &App, setting: usize) -> u8 {
    app.song().tracks[0].machine_value(setting)
}

#[test]
fn k_on_machine_chooses_a_voice_and_shows_its_settings() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    assert_eq!(column_name(&app), "Machine");
    assert!(
        title_line(&app, "Machine").starts_with("╭─ Machine  Sampler  Off ─"),
        "{}",
        title_line(&app, "Machine")
    );
    let depth = app.history.undo_depth();
    press(&mut app, "K");
    assert_eq!(
        app.picker.as_ref().unwrap().shown().count(),
        Machine::ALL.len()
    );
    press(&mut app, "VA");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::VirtualAnalog)
    );
    assert_eq!(app.history.undo_depth(), depth + 1);
    let text = track_text(&app);
    // The machine in its title, with no type after it, and its three settings down a column.
    assert!(text[2].starts_with("╭─ Machine  VA ─"), "{text:?}");
    assert!(
        text[4].starts_with("│  Detune    8000 ▕    │    ▏ "),
        "{text:?}"
    );
    assert!(
        text[6].starts_with("│  Square    0000 ▕         ▏ "),
        "{text:?}"
    );
    assert!(
        text[8].starts_with("│  Saw       8000 ▕████▌    ▏ "),
        "{text:?}"
    );
    for row in [4, 6, 8] {
        assert_eq!(text[row].matches('▕').count(), 1, "one column: {text:?}");
    }
    assert!(
        !text
            .iter()
            .any(|line| line.contains("Sample") || line.contains("Voice"))
    );

    press(&mut app, "j");
    assert_eq!(column_name(&app), "Detune");
    press(&mut app, "=");
    assert_eq!(machine_value(&app, 0), 0x81);
    press(&mut app, "j");
    press(&mut app, "i60");
    assert_eq!(machine_value(&app, 1), 0x60);
    press(&mut app, "x");
    assert_eq!(machine_value(&app, 1), 0);
    // Onto Square's fine digits, and no further: it's the row's last.
    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Square");
    // `}` goes to the same row in the devices below.
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Res");
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Level");
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Glide");
    press(&mut app, "ggh-");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Sampler,
        "VA comes after Sample"
    );
    assert!(title_line(&app, "Machine").contains("Sampler"));
    press(&mut app, "=x");
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
}

#[test]
fn each_voice_keeps_its_own_settings() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let names = |app: &App| -> Vec<String> {
        // The Machine device's first column, its rows apart by a blank line.
        let text = track_text(app);
        [4, 6, 8]
            .map(|row| text[row].split_whitespace().nth(1).unwrap().to_owned())
            .to_vec()
    };
    choose_machine(&mut app, "FM");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Fm)
    );
    assert_eq!(names(&app), ["Ratio", "Index", "Feedback"]);
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Index");
    press(&mut app, "i40");
    assert_eq!(machine_value(&app, 1), 0x40);

    // Another voice starts from its own defaults. Fold is two along, past PD.
    press(&mut app, "gg==");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Waveshaping),
        "the next"
    );
    assert_eq!(names(&app), ["Shape", "Fold", "Slope"]);
    assert_eq!(machine_value(&app, 1), 0, "Fold's own");
    press(&mut app, "jji20");
    assert_eq!(machine_value(&app, 1), 0x20);

    // Back to FM finds it as it was, and the same through samples.
    press(&mut app, "gg--");
    assert_eq!(machine_value(&app, 1), 0x40);
    choose_machine(&mut app, "Sample");
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
    choose_machine(&mut app, "Fold");
    assert_eq!(machine_value(&app, 1), 0x20);
}

#[test]
fn six_op_patch_counts_patches_and_k_lists_them_by_name() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let patch = |app: &App| machine_value(app, 0);
    choose_machine(&mut app, "6-op");
    // The patch is the machine's type, in its title after it.
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Patch");
    assert_eq!(patch(&app), 0x20, "E.PIANO 1 to start");
    assert!(
        title_line(&app, "Machine").starts_with("╭─ Machine  6-op  E.PIANO 1 ─"),
        "{}",
        title_line(&app, "Machine")
    );
    assert!(
        from_name(&app, "Bright").starts_with("Bright    8000"),
        "as the patch was made"
    );
    press(&mut app, "-");
    assert_eq!(patch(&app), 0x1F);
    press(&mut app, "i00=");
    assert_eq!(patch(&app), 1, "one patch a step");
    assert!(title_line(&app, "Machine").contains("  Mooger Low "));
    press(&mut app, "i5F=");
    assert_eq!(patch(&app), 0x5F, "it stops at the last");
    press(&mut app, "i90");
    assert_eq!(patch(&app), 0x5F);

    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 96);
    press(&mut app, "MARIMBA");
    special(&mut app, KeyCode::Enter);
    assert!(
        title_line(&app, "Machine").contains("MARIMBA"),
        "{}",
        title_line(&app, "Machine")
    );
    assert_eq!(usize::from(patch(&app)), 49);
}

#[test]
fn the_sound_page_sets_a_synths_accent() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let accent = |app: &App| machine_value(app, 3);
    choose_machine(&mut app, "Kick");
    // At the top of the second column, beside Punch.
    press(&mut app, "j$");
    assert_eq!(column_name(&app), "Accent");
    assert_eq!(accent(&app), 0xCC, "Plaits' own strength to start");
    assert!(
        from_name(&app, "Punch").contains("   Accent    CC00 ▕"),
        "{}",
        from_name(&app, "Punch")
    );
    press(&mut app, "=");
    assert_eq!(accent(&app), 0xCD);
    press(&mut app, "i40");
    assert_eq!(accent(&app), 0x40);
    press(&mut app, "x");
    assert_eq!(accent(&app), 0xCC, "back to its default");
}

#[test]
fn choosing_a_sample_goes_back_to_playing_samples() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {}", kit.display()));
    choose_machine(&mut app, "VA");
    press(&mut app, " 5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
}

#[test]
fn bars_show_where_a_value_is() {
    use vt_tui::views::widgets::bar::{centre_bar, level_bar};
    assert_eq!(level_bar(0x00, 9), "         ");
    assert_eq!(level_bar(0x01, 9), "▏        ");
    assert_eq!(level_bar(0x40, 9), "██▎      ");
    assert_eq!(level_bar(0x80, 9), "████▌    ");
    assert_eq!(level_bar(0xFE, 9), "████████▉");
    assert_eq!(level_bar(0xFF, 9), "█████████");
    assert_eq!(centre_bar(0x00, 9), "│        ");
    assert_eq!(centre_bar(0x80, 9), "    │    ");
    assert_eq!(centre_bar(0xFF, 9), "        │");
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let text = track_text(&app).join("\n");
    assert!(text.contains("8000 ▕    │    ▏"), "{text}");
}

#[test]
fn a_moving_bar_follows_what_plays_and_marks_what_is_set() {
    use ratatui::style::Style;
    use vt_tui::theme;
    use vt_tui::views::widgets::bar::ValueBar;
    let bar_spans =
        |set, live, centred| ValueBar::new(set, 9).playing(live).centred(centred).spans();
    let text = |spans: &[ratatui::text::Span]| {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    let style_at = |spans: &[ratatui::text::Span], cell: usize| spans[cell].style;
    // Nothing moving: only the bar, in one piece.
    let still = bar_spans(0x80, 0x80, false);
    assert_eq!((still.len(), text(&still).as_str()), (1, "████▌    "));
    assert_eq!(text(&bar_spans(0x80, 0x80, true)), "    │    ");

    // Moved below the setting: the fill follows, and a dim mark stays where it's set.
    let below = bar_spans(0xC0, 0x40, false);
    assert_eq!(text(&below), "██▎   │  ");
    assert_eq!(style_at(&below, 6), theme::faint());
    assert_eq!(style_at(&below, 0).fg, Some(theme::MAUVE));

    // Moved above it: the mark is cut out of the fill, so it still shows.
    let above = bar_spans(0x40, 0xC0, false);
    assert_eq!(text(&above), "██│███▊  ");
    assert_eq!(
        style_at(&above, 2),
        Style::new().fg(theme::BASE).bg(theme::MAUVE)
    );

    // Around a centre: a bright mark for what plays and a dim one for what's set.
    let centred = bar_spans(0x80, 0xC0, true);
    assert_eq!(text(&centred), "    │ │  ");
    assert_eq!(style_at(&centred, 4), theme::faint());
    assert_eq!(style_at(&centred, 6).fg, Some(theme::MAUVE));
    // A small move that stays in the set value's cell keeps the bright mark.
    let near = bar_spans(0x80, 0x82, true);
    assert_eq!(text(&near), "    │    ");
    assert_eq!(style_at(&near, 4).fg, Some(theme::MAUVE));
}

#[test]
fn a_bar_moves_at_the_same_pace_through_the_mark_as_either_side_of_it() {
    use vt_tui::views::widgets::bar::{ValueBar, level_bar};
    let text = |set, live| -> String {
        ValueBar::new(set, 9)
            .playing(live)
            .spans()
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    for set in 0..=u8::MAX {
        for live in 0..u8::MAX {
            // Each step the plain bar takes shows, wherever the mark is.
            if level_bar(live, 9) != level_bar(live + 1, 9) {
                assert_ne!(
                    text(set, live),
                    text(set, live + 1),
                    "set {set:02X}, from {live:02X}"
                );
            }
        }
    }
    // A slow rise across a mark at 80: the edge goes through its cell, with no mark to hide it.
    let frames =
        [0x70, 0x74, 0x78, 0x7B, 0x7E, 0x82, 0x85, 0x89, 0x8D, 0x92].map(|live| text(0x80, live));
    assert_eq!(
        frames,
        [
            "███▉│    ",
            "████│    ",
            "████▏    ",
            "████▎    ",
            "████▍    ",
            "████▌    ",
            "████▋    ",
            "████▊    ",
            "████▉    ",
            "████│▏   ",
        ]
    );
}

#[test]
fn the_page_draws_bars_from_live_values_and_leaves_the_numbers_alone() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let cut = |app: &App| from_name(app, "Cut");
    assert!(cut(&app).contains("FF00 ▕█████████▏"), "{}", cut(&app));
    app.live = Some(vt_engine::track_state::LiveValues {
        level: 0xE0,
        pan: 0x80,
        cutoff: 0x40,
        resonance: 0,
        send_a: 0,
        send_b: 0,
        machine: [0x80, 0, 0x80, 0xCC, 0, 0, 0, 0],
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
    });
    assert!(cut(&app).contains("FF00 ▕██▎     │▏"), "{}", cut(&app));
}

#[test]
fn the_ends_of_bars_line_up_down_each_column() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    for mark in ['▕', '▏'] {
        let mut columns: Vec<usize> = track_text(&app)
            .iter()
            .flat_map(|line| {
                line.chars()
                    .enumerate()
                    .filter(|(_, c)| *c == mark)
                    .map(|(x, _)| x)
                    .collect::<Vec<_>>()
            })
            .collect();
        columns.sort_unstable();
        columns.dedup();
        assert_eq!(columns.len(), 2, "{mark}: {columns:?}");
    }
}

/// An app on the track page.
fn on_track_page() -> (App, Realtime) {
    let (mut app, realtime) = tracks_of_different_lengths();
    // On Cut, the Filter's first setting, as the Machine's aren't all values.
    press(&mut app, " 4}j");
    assert_eq!(app.view, View::Track);
    (app, realtime)
}

#[test]
fn the_track_page_sets_base_values_and_undoes() {
    let (mut app, _realtime) = on_track_page();
    assert_eq!(app.song().tracks[0].cutoff, 0xFF);
    press(&mut app, "i40");
    assert_eq!(app.song().tracks[0].cutoff, 0x40);
    assert_eq!(app.mode, Mode::Normal);
    press(&mut app, "3-");
    assert_eq!(app.song().tracks[0].cutoff, 0x3D);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].cutoff, 0x40);

    // The Output's Pan, on the next track.
    press(&mut app, "W");
    press(&mut app, "gg2}jj");
    press(&mut app, "i20");
    assert_eq!(app.song().tracks[1].pan, 0x20);
    assert_eq!(app.song().tracks[0].pan, 0x80);
    press(&mut app, "ki7F");
    assert_eq!(app.song().tracks[1].level, 0x7F);
}

#[test]
fn levels_snap_to_the_top_of_the_scale_and_filter_values_do_not() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    // Cut isn't a level, and Res isn't either.
    press(&mut app, "i10iFF");
    assert_eq!(track(&app).cutoff, 0xFF);
    // Level, by typing and by nudging.
    press(&mut app, "gg2}ji");
    press(&mut app, "FF");
    assert_eq!(track(&app).level, 0xF8);
    press(&mut app, "20-");
    assert_eq!(track(&app).level, 0xE4);
    press(&mut app, "99=");
    assert_eq!(track(&app).level, 0xF8);
    command(&mut app, "tvol FF");
    assert_eq!(track(&app).level, 0xF8);
    // Pan runs to FF, and the sends stop at the top.
    press(&mut app, "jiFF");
    assert_eq!(track(&app).pan, 0xFF);
    press(&mut app, "kk$iFF");
    assert_eq!(track(&app).send_a, 0xF8);
    press(&mut app, "jiFF");
    assert_eq!(track(&app).send_b, 0xF8);
}

#[test]
fn the_track_page_types_lfo_settings() {
    let (mut app, _realtime) = on_track_page();
    let lfo = |app: &App| app.song().tracks[0].lfos[0];
    // Down to LFO 1, on Dest.
    press(&mut app, &format!("L{}0", to_lfo(0)));
    press(&mut app, "=");
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));
    press(&mut app, "-");
    assert_eq!(lfo(&app).destination, None);
    press(&mut app, "-");
    assert_eq!(
        lfo(&app).destination,
        Some(SettingId::lfo(vt_model::lfo::LFOS_PER_TRACK - 1, 1).unwrap()),
        "the last for LFO 1 on a sample track: the last LFO's phase"
    );

    press(&mut app, "l=");
    assert_eq!(lfo(&app).shape, Shape::Saw);

    press(&mut app, "l");
    typed(&mut app, "1/8d");
    assert_eq!(lfo(&app).rate, Rate::Note(720));
    typed(&mut app, "7st");
    assert_eq!(lfo(&app).rate, Rate::Steps(7));
    typed(&mut app, "2.5hz");
    assert_eq!(lfo(&app).rate, Rate::Hertz(250));
    typed(&mut app, "fast");
    assert_eq!(lfo(&app).rate, Rate::Hertz(250));
    assert!(app.status.contains("1/8d"), "{}", app.status);

    press(&mut app, "l");
    typed(&mut app, "-40");
    assert_eq!(lfo(&app).depth, -0x40);
    typed(&mut app, "+7F");
    assert_eq!(lfo(&app).depth, 0x7F);
    typed(&mut app, "100");
    assert_eq!(lfo(&app).depth, 0x7F);

    press(&mut app, "l");
    typed(&mut app, "90");
    assert_eq!(lfo(&app).phase, 90);
    typed(&mut app, "360");
    assert_eq!(lfo(&app).phase, 90);
    press(&mut app, "3-");
    assert_eq!(lfo(&app).phase, 45);
    press(&mut app, "4-");
    assert_eq!(lfo(&app).phase, 345);

    press(&mut app, "l=");
    assert!(lfo(&app).retrig);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists a setting that is a list");
    special(&mut app, KeyCode::Esc);

    // Nothing else moved.
    assert_eq!(app.song().tracks[0].lfos[1], Lfo::default());
}

#[test]
fn k_chooses_an_lfos_destination_from_a_list() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Normal, "not on a Dest");
    assert!(app.status.contains("Dest"), "{}", app.status);

    // The second LFO's Dest, on the next track.
    press(&mut app, "W");
    press(&mut app, &format!("L{}0K/", to_lfo(1)));
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.items().len(),
        16,
        "what a sample track has, its Sample, Slice, Start and Length, none, and LFO 3's two"
    );
    assert_eq!(picker.selected, 0, "what it moves now");
    press(&mut app, "res");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Track);
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
    assert_eq!(app.song().tracks[1].lfos[0], Lfo::default());
    assert_eq!(app.song().tracks[0].lfos[1], Lfo::default());

    // It opens on what's set, and leaves the other settings alone. Esc changes nothing.
    typed_rate_on_second_lfo(&mut app);
    press(&mut app, "0K");
    assert_eq!(
        listed(app.picker.as_ref().unwrap().current().unwrap()),
        "Res  Filter resonance"
    );
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
    press(&mut app, "K/pan");
    special(&mut app, KeyCode::Enter);
    let lfo = app.song().tracks[1].lfos[1];
    assert_eq!(lfo.destination, Some(SettingId::PAN));
    assert_eq!(lfo.rate, Rate::Steps(7));

    press(&mut app, "u");
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
}

fn typed_rate_on_second_lfo(app: &mut App) {
    press(app, "ll");
    typed(app, "7st");
}

#[test]
fn the_track_page_sets_the_filters_type_and_drive() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    // Down to Drive, then up to Type.
    press(&mut app, "jj");
    press(&mut app, "i40");
    assert_eq!(track(&app).drive, 0x40);
    press(&mut app, "+");
    assert_eq!(track(&app).drive, 0x50);
    press(&mut app, "iFF");
    assert_eq!(track(&app).drive, 0xFF, "drive isn't on the level scale");
    press(&mut app, "x");
    assert_eq!(track(&app).drive, 0);

    press(&mut app, "kkk");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "=");
    assert_eq!(track(&app).filter, FilterType::HighPass);
    press(&mut app, "-");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 9);
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Ladder);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists it too");
    assert_eq!(
        app.picker.as_ref().unwrap().selected,
        4,
        "opens on what's set"
    );
    special(&mut app, KeyCode::Esc);
    assert!(title_line(&app, "Filter").contains("Ladder"));
    press(&mut app, "u");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "K");
    press(&mut app, "ms20 hp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Ms20HighPass);
    assert!(title_line(&app, "Filter").contains("MS20 HP"));
    press(&mut app, "K");
    press(&mut app, "dirty");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Dirty);
    assert!(title_line(&app, "Filter").contains("Dirty"));
}

#[test]
fn the_track_page_sets_the_filters_mode_for_its_type() {
    use vt_model::song::FilterMode;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    // The mode is in the Filter's title, after its type.
    press(&mut app, "kl");
    assert_eq!(column_name(&app), "Mode");
    assert!(title_line(&app, "Filter").contains("12 dB"));
    press(&mut app, "=");
    assert_eq!(track(&app).filter_mode, FilterMode::Slope24);
    press(&mut app, "=");
    assert_eq!(
        track(&app).filter_mode,
        FilterMode::Slope12,
        "round the two"
    );

    // The ladder lists its six, and starts on its own first.
    press(&mut app, "hK");
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "l");
    assert!(title_line(&app, "Filter").contains("LP24"));
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 6);
    press(&mut app, "/hp24");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter_mode, FilterMode::HighPass24);

    // Another type that offers it keeps it, and one that doesn't plays its first, until it's back.
    press(&mut app, "hK");
    press(&mut app, "dirty");
    special(&mut app, KeyCode::Enter);
    assert!(title_line(&app, "Filter").contains("HP24"));
    // Round from the last type to `LP`.
    press(&mut app, "=");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    assert!(title_line(&app, "Filter").contains("12 dB"));
    assert_eq!(track(&app).filter_mode, FilterMode::HighPass24);
    press(&mut app, "K");
    press(&mut app, "ms20 lp");
    special(&mut app, KeyCode::Enter);
    assert!(title_line(&app, "Filter").contains("--"));
    press(&mut app, "l=");
    assert!(app.status.contains("no modes"), "{}", app.status);
    press(&mut app, "K");
    assert_ne!(app.mode, Mode::Picker);
    press(&mut app, "hK");
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    assert!(title_line(&app, "Filter").contains("HP24"));

    // `x` puts it back to the first.
    press(&mut app, "lx");
    assert_eq!(track(&app).filter_mode, FilterMode::default());
    assert!(title_line(&app, "Filter").contains("LP24"));
}

#[test]
fn the_track_page_sets_pitch_and_a_choke_group() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    let pitch = from_name(&app, "Pitch");
    assert!(pitch.starts_with("Pitch     8000 ▕    │    ▏"), "{pitch}");
    let choke = from_name(&app, "Choke");
    assert!(choke.starts_with("Choke     Off "), "{choke}");

    // In Play, two devices down from Cut.
    press(&mut app, "2}");
    assert_eq!(app.track_control().label(), "Pitch");
    press(&mut app, "i8C");
    assert_eq!(track(&app).pitch, 0x8C);
    press(&mut app, "x");
    assert_eq!(track(&app).pitch, 0x80);

    // Along the row past Speed.
    press(&mut app, "$");
    assert_eq!(app.track_control().label(), "Choke");
    press(&mut app, "=");
    assert_eq!(track(&app).choke, Some(1));
    press(&mut app, "3=");
    assert_eq!(track(&app).choke, Some(4));
    typed(&mut app, "16");
    assert_eq!(track(&app).choke, Some(16));
    press(&mut app, "=");
    assert_eq!(track(&app).choke, Some(16), "16 is the last");
    typed(&mut app, "17");
    assert_eq!(track(&app).choke, Some(16));
    assert!(app.status.contains("1 to 16"), "{}", app.status);
    assert!(from_name(&app, "Choke").starts_with("Choke     16 "));
    typed(&mut app, "off");
    assert_eq!(track(&app).choke, None);
    press(&mut app, "u");
    assert_eq!(track(&app).choke, Some(16));
    press(&mut app, "x");
    assert_eq!(track(&app).choke, None);
    press(&mut app, "=-");
    assert_eq!(track(&app).choke, None, "down from 1 is none");
}

#[test]
fn the_track_page_sets_how_long_notes_last() {
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let gate = |app: &App| app.song().tracks[0].gate_length;
    let line = |app: &App| from_name(app, "Gate");
    assert!(
        line(&app).contains("Gate") && line(&app).contains("100%"),
        "{}",
        line(&app)
    );

    // At the bottom of Play's first column.
    press(&mut app, "2}jj");
    assert_eq!(app.track_control().label(), "Gate");
    assert_eq!(value_shown(&app).as_deref(), Some("100 %"));
    press(&mut app, "-");
    assert_eq!(gate(&app), Some(252), "a percent down");
    press(&mut app, "_");
    assert_eq!(gate(&app), Some(227), "ten more");
    assert!(line(&app).contains("89%"), "{}", line(&app));
    typed(&mut app, "25");
    assert_eq!(gate(&app), Some(0x40));
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
    typed(&mut app, "50%");
    assert_eq!(gate(&app), Some(0x80));
    typed(&mut app, "0");
    assert_eq!(gate(&app), Some(0x80));
    assert!(app.status.contains("1 to 100"), "{}", app.status);
    press(&mut app, "200-");
    assert_eq!(gate(&app), Some(gate_value(1)), "1% is the least");
    press(&mut app, "x");
    assert_eq!(gate(&app), None, "back to the machine's");
    assert!(line(&app).contains("100%"), "{}", line(&app));
    press(&mut app, "u");
    assert_eq!(gate(&app), Some(gate_value(1)));
    press(&mut app, "x");

    // `Acid`'s own is 50%, as a 303's is.
    press(&mut app, "gghK");
    press(&mut app, "/acid");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Acid)
    );
    assert!(
        line(&app).contains("Gate") && line(&app).contains("50%"),
        "{}",
        line(&app)
    );
}

#[test]
fn the_sample_row_lists_the_bank_steps_through_it_and_clears() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    // The sample is the sampler's type, in the Machine's title after it.
    press(&mut app, "gg");
    press(&mut app, "l");
    assert_eq!(app.track_control(), Control::Sample);
    let title = title_line(&app, "Machine");
    assert!(
        title.starts_with("╭─ Machine  Sampler  01 snare ─"),
        "{title}"
    );
    assert_eq!(value_shown(&app).as_deref(), Some("01 snare: snare.wav"));

    press(&mut app, "=");
    assert_eq!(track(&app).sample(), 2);
    press(&mut app, "2=");
    assert_eq!(track(&app).sample(), 3, "it stops at the last");
    press(&mut app, "x");
    assert_eq!(track(&app).sample(), 0);
    assert_eq!(value_shown(&app).as_deref(), Some("Off: no sample"));
    press(&mut app, "-");
    assert_eq!(track(&app).sample(), 0);

    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(
        labels,
        ["00  Off", "01  snare", "02  kick", "03  hat", "Add a file"]
    );
    press(&mut app, "/hat");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).sample(), 3);
}

#[test]
fn the_track_page_sets_the_utility_filter() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    assert_eq!((track(&app).low_cut, track(&app).high_cut), (0, 0xFF));
    // Beside the filter's own, in the Filter.
    assert!(from_name(&app, "Cut").contains("   Low cut   00   ▕"));
    assert!(from_name(&app, "Res").contains("   High cut  FF   ▕"));

    press(&mut app, "$");
    press(&mut app, "i40");
    assert_eq!(track(&app).low_cut, 0x40);
    press(&mut app, "x");
    assert_eq!(track(&app).low_cut, 0);
    press(&mut app, "j");
    press(&mut app, "i80");
    assert_eq!(track(&app).high_cut, 0x80);
    press(&mut app, "=");
    assert_eq!(track(&app).high_cut, 0x81);
    press(&mut app, "+");
    assert_eq!(track(&app).high_cut, 0x91);
    press(&mut app, "x");
    assert_eq!(track(&app).high_cut, 0xFF);
    press(&mut app, "u");
    assert_eq!(track(&app).high_cut, 0x91);
    assert_eq!(app.track_control().label(), "High cut");
}

#[test]
fn cmd_with_the_movement_keys_changes_the_value_without_moving() {
    let (mut app, _realtime) = on_track_page();
    let attack = |app: &App| app.song().tracks[0].envelopes[0].attack_ms;
    press(&mut app, "L0l");
    typed(&mut app, "20");
    let label = app.track_control().label();
    with_cmd(&mut app, "D-l");
    assert_eq!(attack(&app), 25, "as =");
    with_cmd(&mut app, "D-k");
    assert_eq!(attack(&app), 75, "as +");
    with_cmd(&mut app, "D-h");
    assert_eq!(attack(&app), 70, "as -");
    with_cmd(&mut app, "D-j");
    assert_eq!(attack(&app), 20, "as _");
    assert_eq!(app.track_control().label(), label, "the cursor stays");
    // Option no longer adjusts.
    alt(&mut app, "M-l");
    assert_eq!(attack(&app), 20, "Option is unbound");
}

#[test]
fn a_held_cmd_l_keeps_adjusting() {
    use ratatui::crossterm::event::KeyEventKind;
    let (mut app, _realtime) = on_track_page();
    let attack = |app: &App| app.song().tracks[0].envelopes[0].attack_ms;
    press(&mut app, "L0l");
    typed(&mut app, "20");
    let key = parse_sequence("D-l").unwrap()[0];
    let now = Instant::now();
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Repeat, now);
    // kitty on macOS sends a held key's repeats as presses.
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Release, now);
    assert_eq!(attack(&app), 35, "three steps of 5");
}

#[test]
fn the_track_page_sets_envelopes() {
    let (mut app, _realtime) = on_track_page();
    let env = |app: &App, slot: usize| app.song().tracks[0].envelopes[slot];
    // Env 1, on its Attack.
    press(&mut app, "L0l");
    typed(&mut app, "20");
    assert_eq!(env(&app, 0).attack_ms, 20);
    press(&mut app, "=");
    assert_eq!(env(&app, 0).attack_ms, 25);
    press(&mut app, "+");
    assert_eq!(env(&app, 0).attack_ms, 75);
    typed(&mut app, "9000");
    assert_eq!(env(&app, 0).attack_ms, 75);
    assert!(app.status.contains("8000"), "{}", app.status);
    typed(&mut app, "8000");
    assert_eq!(env(&app, 0).attack_ms, 8000);
    press(&mut app, "=");
    assert_eq!(env(&app, 0).attack_ms, 8000);
    press(&mut app, "x");
    assert_eq!(env(&app, 0).attack_ms, 1);

    press(&mut app, "l");
    typed(&mut app, "180");
    assert_eq!(env(&app, 0).decay_ms, 180);
    press(&mut app, "l");
    typed(&mut app, "40");
    assert_eq!(env(&app, 0).sustain, 40);
    typed(&mut app, "101");
    assert_eq!(env(&app, 0).sustain, 40);
    assert!(app.status.contains("percent"), "{}", app.status);
    press(&mut app, "+");
    assert_eq!(env(&app, 0).sustain, 50);
    press(&mut app, "l");
    typed(&mut app, "300");
    assert_eq!(env(&app, 0).release_ms, 300);

    // Env 2, its depth in hex and its destination from a list.
    press(&mut app, "j0K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        listed(app.picker.as_ref().unwrap().current().unwrap()),
        "Cut  Filter cutoff",
        "opens on what's chosen"
    );
    press(&mut app, "/pitch");
    special(&mut app, KeyCode::Enter);
    assert_eq!(env(&app, 1).destination, Some(SettingId::PITCH));
    press(&mut app, "lllll");
    typed(&mut app, "+18");
    assert_eq!(env(&app, 1).depth, 0x18);
    press(&mut app, "lK");
    press(&mut app, "off");
    special(&mut app, KeyCode::Enter);
    assert!(!env(&app, 1).reset);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists a setting that is a list");
    special(&mut app, KeyCode::Esc);

    // Each is one undo, and the first envelope was left alone.
    press(&mut app, "u");
    assert!(env(&app, 1).reset);
    assert_eq!(env(&app, 0).release_ms, 300);
}

#[test]
fn the_track_page_draws_each_envelopes_shape() {
    use vt_tui::kitty::{Graphics, id_color};
    let (mut app, _realtime) = on_track_page();
    let mut graphics = Graphics::new();
    let page = |app: &mut App, graphics: &mut Graphics| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let text = |buffer: &ratatui::buffer::Buffer| {
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
    };
    press(&mut app, "L");
    let drawn = page(&mut app, &mut graphics);
    let lines = text(&drawn);
    let heading = lines.iter().position(|l| l.contains("Env 1  Vol")).unwrap();
    assert!(lines[heading].contains("Env 2  Cut"));
    let first = app.envelope_images.unwrap()[0];
    assert_eq!(drawn[(0, heading as u16 + 1)].fg, id_color(first));

    // Changing a value sends a new image, which the placeholders point to.
    press(&mut app, "gg0lll");
    typed(&mut app, "0");
    let changed = page(&mut app, &mut graphics);
    assert_ne!(changed[(0, heading as u16 + 1)].fg, id_color(first));
    assert_eq!(app.song().tracks[0].envelopes[0].sustain, 0);

    // The first envelope on Vol has no depth to show, and off it, it does.
    assert!(text(&page(&mut app, &mut graphics))[3].contains("--"));
    press(&mut app, "0K/cut");
    special(&mut app, KeyCode::Enter);
    assert!(text(&page(&mut app, &mut graphics))[3].contains("00"));
}

#[test]
fn every_list_setting_on_the_track_page_has_a_picker() {
    let (mut app, _realtime) = on_track_page();
    let lfo = |app: &App| app.song().tracks[0].lfos[0];
    let choose = |app: &mut App, keys: &str| {
        press(app, "K");
        assert_eq!(app.mode, Mode::Picker);
        press(app, keys);
        special(app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
    };
    press(&mut app, &format!("L{}0", to_lfo(0)));
    choose(&mut app, "/cut");
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));

    press(&mut app, "l");
    choose(&mut app, "square");
    assert_eq!(lfo(&app).shape, Shape::Square);

    press(&mut app, "l");
    assert_eq!(app.mode, Mode::Normal);
    choose(&mut app, "1/8d");
    assert_eq!(lfo(&app).rate, Rate::Note(720));

    // Retrig is three along, past Depth and Phase, which aren't lists.
    press(&mut app, "l");
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Dest"), "{}", app.status);
    press(&mut app, "ll");
    choose(&mut app, "on");
    assert!(lfo(&app).retrig);
    assert_eq!(lfo(&app).shape, Shape::Square, "the others are left alone");

    // The picker opens on what's set.
    press(&mut app, "0lK");
    assert_eq!(app.picker.as_ref().unwrap().selected, 4);
    special(&mut app, KeyCode::Esc);

    // On a list setting, i opens it too. On Rate, i types.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "li");
    assert_eq!(app.mode, Mode::Insert);
    special(&mut app, KeyCode::Esc);
}

#[test]
fn large_steps_on_the_track_page_suit_each_setting() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    press(&mut app, "_");
    assert_eq!(track(&app).cutoff, 0xEF);
    press(&mut app, "gg2}j");
    press(&mut app, "+");
    assert_eq!(track(&app).level, 0xF0);
    press(&mut app, "}l+");
    assert_eq!(
        track(&app).groove,
        GrooveId::from_index(1),
        "a groove moves one at a time"
    );
    press(&mut app, &format!("L{}0ll", to_lfo(0)));
    press(&mut app, "+");
    assert_eq!(track(&app).lfos[0].rate, Rate::Note(480));
    press(&mut app, "_");
    assert_eq!(track(&app).lfos[0].rate, Rate::Note(960));
    press(&mut app, "l+");
    assert_eq!(track(&app).lfos[0].depth, 0x10);
    press(&mut app, "l+");
    assert_eq!(track(&app).lfos[0].phase, 90);
    press(&mut app, "_");
    assert_eq!(track(&app).lfos[0].phase, 0);
}

#[test]
fn x_puts_track_page_settings_back_to_their_defaults() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    press(&mut app, "i40");
    press(&mut app, "x");
    assert_eq!(track(&app).cutoff, 0xFF);
    press(&mut app, "u");
    assert_eq!(track(&app).cutoff, 0x40, "one undo");
    press(&mut app, "x");

    press(&mut app, "gg2}");
    press(&mut app, "i40x");
    assert_eq!(track(&app).trim, 0x80);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).level, 0xE0);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).pan, 0x80);
    press(&mut app, "kk$i40x");
    assert_eq!(track(&app).send_a, 0);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).send_b, 0);

    // Only the setting under the cursor goes back.
    press(&mut app, &format!("L{}0=", to_lfo(0)));
    press(&mut app, "l=");
    press(&mut app, "l");
    typed(&mut app, "7st");
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "l");
    typed(&mut app, "90");
    press(&mut app, "l=");
    let lfo = |app: &App| track(app).lfos[0];
    press(&mut app, "x");
    assert!(!lfo(&app).retrig);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).phase, 0);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).depth, 0);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).rate, Lfo::default().rate);
    assert_eq!(lfo(&app).shape, Shape::Saw);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).shape, Shape::Tri);
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));
    press(&mut app, "hx");
    assert_eq!(lfo(&app), Lfo::default());

    // Already there, it leaves no undo step.
    let depth = app.history.undo_depth();
    press(&mut app, "x");
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn a_track_following_the_project_shows_its_groove_dimmed_and_x_returns_to_that() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "swing 60");
    let groove_cell = |app: &App| {
        let (width, height) = (150, 40);
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let name = |x: u16, y: u16| {
            (x..x + 6)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let (x, y) = (0..height)
            .flat_map(|y| (0..width - 6).map(move |x| (x, y)))
            .find(|(x, y)| name(*x, *y) == "Groove")
            .unwrap();
        let at = x + 10;
        let text: String = (at..at + 8).map(|x| buffer[(x, y)].symbol()).collect();
        (text, buffer[(at, y)].fg)
    };
    // The cursor is elsewhere, so the colour is the setting's own.
    assert_eq!(app.song().tracks[0].groove, None);
    let dim = vt_tui::theme::OVERLAY2;
    assert_eq!(groove_cell(&app), ("Swing 60".to_owned(), dim));

    // It's the track's own once set, and a nudge starts from what it was playing.
    press(&mut app, "2}j$=");
    assert_eq!(app.song().tracks[0].groove, GrooveId::swing(62));
    press(&mut app, "k");
    assert_eq!(groove_cell(&app), ("Swing 62".to_owned(), Color::Reset));

    press(&mut app, "j$x");
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "k");
    assert_eq!(groove_cell(&app), ("Swing 60".to_owned(), dim));
}

#[test]
fn plus_makes_a_rate_faster_of_any_kind() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, &format!("L{}0ll", to_lfo(0)));
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Note(720));
    typed(&mut app, "7st");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Steps(6));
    typed(&mut app, "2hz");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Hertz(210));
}

#[test]
fn typing_a_value_and_long_words_keep_the_sound_pages_columns_in_line() {
    let (mut app, _realtime) = app();
    let line = from_name;
    // The bar stays where it is while a value is typed, from its first key.
    press(&mut app, " 4jli");
    assert!(
        line(&app, "Start").starts_with("Start     _ 00 ▕         ▏"),
        "{}",
        line(&app, "Start")
    );
    special(&mut app, KeyCode::Esc);
    // A choice's name starts where the bars do, a word where the values do, and the column beside
    // each keeps to its place.
    let slice = line(&app, "Slice");
    assert!(
        slice.starts_with("Slice     00   whole         Start     0000 ▕"),
        "{slice}"
    );
    let play = line(&app, "Play");
    assert!(
        play.starts_with("Play      One-shot           Length    FF00 ▕"),
        "{play}"
    );
}

#[test]
fn the_track_page_shows_its_settings_in_columns() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, &format!("L{}0=ll", to_lfo(0)));
    typed(&mut app, "1/8d");
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "l");
    typed(&mut app, "90");
    let lines = track_page_text(&app);
    assert!(lines[0].contains(" Track 01 "), "{}", lines[0]);
    assert!(lines[2].contains("Attack") && lines[2].contains("Reset"));
    assert!(lines[3].starts_with("Env 1") && lines[3].contains("Vol"));
    assert!(lines[4].starts_with("Env 2") && lines[4].contains("Cut"));
    assert!(lines[5].starts_with("Env 3") && lines[5].contains("---"));
    assert!(lines[7].contains("Dest") && lines[7].contains("Retrig"));
    let lfo: Vec<&str> = lines[8].split_whitespace().collect();
    let triangle = vt_tui::glyphs::TRIANGLE_WAVE.to_string();
    // Its retrig is a toggle, off.
    let off = vt_tui::glyphs::SWITCH_OFF.to_string();
    assert_eq!(
        lfo,
        [
            "LFO", "1", "Cut", "Tri", &triangle, "1/8d", "+40", "90°", &off
        ]
    );
    assert!(lines[9].starts_with("LFO 2"));
    assert!(lines[10].starts_with("LFO 3"));
    press(&mut app, "H");
    let lines = track_text(&app);
    // Four devices, each with its settings down columns and its title on its frame's top.
    let rows = [
        (2, "╭─ Machine  Sampler  Off ─"),
        (
            4,
            "│  Slice     00   whole         Start     0000 ▕         ▏   ",
        ),
        (
            6,
            "│  Play      One-shot           Length    FF00 ▕█████████▏   ",
        ),
        (11, "╭─ Filter  LP  12 dB ─"),
        (
            13,
            "│  Cut       FF00 ▕█████████▏   Low cut   00   ▕         ▏   ",
        ),
        (
            15,
            "│  Res       0000 ▕         ▏   High cut  FF   ▕█████████▏   ",
        ),
        (17, "│  Drive     00   ▕         ▏   "),
        (20, "╭─ Output ─"),
        // The trim is at the top of the output, at 0 dB in the middle of its bar.
        (
            22,
            "│  Trim      80   ▕    │    ▏   Send A    0000 ▕         ▏   ",
        ),
        (
            24,
            "│  Level     E0   ▕███████▉ ▏   Send B    0000 ▕         ▏   ",
        ),
        (
            26,
            "│  Pan       8000 ▕    │    ▏   Group     Off               ",
        ),
        (29, "╭─ Play ─"),
        (
            31,
            "│  Pitch     8000 ▕    │    ▏   Speed     1/16               Choke     Off ",
        ),
        (
            33,
            "│  Glide     Off                Groove    Straight           ",
        ),
        (
            35,
            "│  Gate      100%               Reset     Off                ",
        ),
    ];
    for (row, text) in rows {
        assert!(lines[row].starts_with(text), "{row}: {}", lines[row]);
    }
}

#[test]
fn a_tracks_groove_on_the_page_is_chosen_from_a_list() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_track_page();
    let groove = |name: &str| GrooveId::named(name);
    press(&mut app, "2}j$");
    assert_eq!(app.track_control(), Control::Groove);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "/pull");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].groove, groove("Pull"));
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].groove, groove("Custom 1"));
    press(&mut app, "x");
    assert_eq!(app.song().tracks[0].groove, None, "back to the project's");
    // `i` chooses from the list too, which starts with following the project's.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].groove, None);
}

#[test]
fn h_and_l_change_pages_and_every_setting_is_on_one_page() {
    use vt_tui::track_page::{MOD_ROWS, Page};
    let (mut app, _realtime) = on_track_page();
    assert_eq!(app.track_page.page, Page::Sound);
    press(&mut app, "H");
    assert_eq!(app.track_page.page, Page::Sound, "it stops at the first");
    press(&mut app, "j$");
    press(&mut app, "L");
    assert_eq!(app.track_page.page, Page::Mod);
    assert_eq!(app.track_page.row, 0);
    let row = MOD_ROWS[0];
    assert!(row.contains(&app.track_control()), "the cursor stays valid");
    press(&mut app, "GH");
    assert_eq!(app.track_page.page, Page::Sound);
    assert_eq!(app.track_page.row, 0);
    let sound: Vec<_> = Page::Sound
        .devices(&app.song().tracks[0])
        .iter()
        .flat_map(|device| device.rows().flatten().flatten().collect::<Vec<_>>())
        .collect();
    let all: Vec<_> = sound
        .iter()
        .chain(MOD_ROWS.iter().flat_map(|row| row.iter()))
        .collect();
    for (at, control) in all.iter().enumerate() {
        assert!(!all[at + 1..].contains(control), "{control:?} is on twice");
    }
    // Only here: in the phrase view they keep their meaning.
    press(&mut app, " 3L");
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn the_sample_setting_chooses_from_the_bank_and_x_clears_it() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "ggl");
    assert_eq!(app.track_control(), vt_tui::track_page::Control::Sample);
    // With nothing in the bank, K opens the explorer to add a file.
    press(&mut app, "K");
    assert_ne!(app.mode, Mode::Normal);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].sample(), 0, "nothing to step to");
    command(&mut app, "name bd");
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let tone = dir.join("tone.wav");
    wav(&tone);
    command(&mut app, &format!("sample {}", tone.display()));
    let drawn = page_text(&app);
    assert!(drawn.contains("01 tone"), "{drawn}");
    assert!(drawn.contains("BD"), "{drawn}");
    assert!(!drawn.contains(dir.to_str().unwrap()), "{drawn}");
    press(&mut app, "x");
    assert_eq!(sample_of(&app, 0), None);
    press(&mut app, "u");
    assert!(sample_of(&app, 0).is_some());
}

fn page_text(app: &App) -> String {
    track_text(app).join("\n")
}

#[test]
fn envelopes_drawn_as_images_leave_placeholders_with_their_positions() {
    let (mut app, _realtime) = app();
    app.envelope_images = Some([0x01_02_03, 0x04_05_06, 0x07_08_09]);
    press(&mut app, " 4L");
    let buffer = {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let at = |text: &str| {
        (0..24).find(|y| {
            (0..80)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains(text)
        })
    };
    let label = at("Env 1  Vol").unwrap();
    let first = &buffer[(0, label + 1)];
    assert!(
        first.symbol().starts_with('\u{10EEEE}'),
        "{:?}",
        first.symbol()
    );
    assert_eq!(first.fg, ratatui::style::Color::Rgb(1, 2, 3));
    // The second envelope is one image width and a gap along, and holds its row and column.
    let second = &buffer[(30, label + 2)];
    assert_eq!(second.symbol(), "\u{10EEEE}\u{30D}\u{305}");
    assert_eq!(second.fg, ratatui::style::Color::Rgb(4, 5, 6));
    assert_eq!(buffer[(27, label + 1)].symbol(), "\u{10EEEE}\u{305}\u{36D}");
}

#[test]
fn a_changed_envelope_image_gets_a_new_id_and_moving_the_cursor_sends_nothing() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        (out.matches(",c=28,").count(), out)
    };
    // Nothing is sent until the page that shows them.
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    press(&mut app, " 4L");
    assert_eq!(
        sync(&mut app, &mut graphics).0,
        vt_model::constants::TRACK_ENVELOPES
    );
    let first = app.envelope_images.unwrap();
    assert_ne!(first[0], first[1]);
    assert_eq!(sync(&mut app, &mut graphics).0, 0, "nothing has changed");

    // Each envelope's other look is sent once, and after that the cursor only swaps between them.
    press(&mut app, "j");
    assert_eq!(sync(&mut app, &mut graphics).0, 2);
    let moved = app.envelope_images.unwrap();
    assert!(moved[0] != first[0] && moved[1] != first[1]);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    assert_eq!(app.envelope_images.unwrap(), first);
    press(&mut app, "j");
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    assert_eq!(app.envelope_images.unwrap(), moved);

    // A change makes a new image, and the old one is forgotten only after the screen moves on.
    press(&mut app, "=");
    let (sent, out) = sync(&mut app, &mut graphics);
    assert_eq!(sent, 1);
    let changed = app.envelope_images.unwrap();
    assert_ne!(changed[1], moved[1]);
    assert_eq!(changed[0], moved[0]);
    let forget = format!("a=d,d=I,i={},q=2", moved[1]);
    assert!(!out.contains(&forget));
    assert!(sync(&mut app, &mut graphics).1.contains(&forget));
}

#[test]
fn an_lfos_shape_has_its_glyph_after_its_name_and_keeps_the_columns_in_line() {
    use vt_tui::glyphs;
    use vt_tui::theme;
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    assert_eq!(app.song().tracks[0].lfos[0].shape, Shape::Tri);
    let plain = draw(&app);
    let row = (0..24)
        .find(|y| {
            (0..80)
                .map(|x| plain[(x, *y)].symbol())
                .collect::<String>()
                .starts_with("LFO 1")
        })
        .unwrap();
    let text = |buffer: &ratatui::buffer::Buffer, x: u16| buffer[(x, row)].symbol().to_owned();
    // After the name's column, one cell, dim while the cursor is on another LFO.
    assert_eq!(text(&plain, 30), glyphs::TRIANGLE_WAVE.to_string());
    assert_eq!(plain[(30, row)].fg, theme::dim().fg.unwrap());
    assert_eq!(text(&plain, 31), " ");

    // In the active colour while the cursor is on its LFO.
    press(&mut app, &to_lfo(0));
    assert_eq!(draw(&app)[(30, row)].fg, theme::active().fg.unwrap());

    // A falling saw has no glyph, and the columns after it stay where they were.
    press(&mut app, "l=");
    assert_eq!(app.song().tracks[0].lfos[0].shape, Shape::Saw);
    let saw = draw(&app);
    assert_eq!(text(&saw, 30), " ");
    for x in 31..80 {
        assert_eq!(text(&saw, x), text(&plain, x), "at {x}");
    }
    assert_eq!(glyphs::lfo_shape(Shape::Ramp), Some(glyphs::SAWTOOTH_WAVE));
    for shape in [Shape::Saw, Shape::Random, Shape::Smooth] {
        assert_eq!(glyphs::lfo_shape(shape), None);
    }
}

#[test]
fn the_filter_curve_follows_its_settings_and_the_cursor() {
    use vt_tui::kitty::Graphics;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out).unwrap().matches(",c=24,").count()
    };
    press(&mut app, " 4");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let plain = app.filter_image.unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0);

    // In the Filter it's picked out, once.
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Filter");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.filter_image.unwrap();
    assert_ne!(picked, plain);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.filter_image.unwrap(), plain);

    // A new setting draws it again.
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Cut");
    press(&mut app, "-");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.filter_image.unwrap(), picked);
    assert_eq!(sync(&mut app, &mut graphics), 0);

    // And so does a new mode.
    press(&mut app, "kl");
    assert_eq!(column_name(&app), "Mode");
    press(&mut app, "=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_eq!(sync(&mut app, &mut graphics), 0);
}

#[test]
fn the_filter_curve_moves_with_what_plays() {
    use vt_engine::track_state::LiveValues;
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        (out.matches(",c=24,").count(), out.matches("a=d,").count())
    };
    press(&mut app, " 4");
    sync(&mut app, &mut graphics);
    let still = app.filter_image.unwrap();
    let track = app.song().tracks[0].clone();
    let live = |cutoff, resonance| LiveValues {
        level: track.level,
        pan: track.pan,
        cutoff,
        resonance,
        send_a: track.send_a,
        send_b: track.send_b,
        machine: std::array::from_fn(|setting| track.machine_value(setting)),
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
    };

    // Each new value draws it once, and it holds while the value does.
    app.live = Some(live(0x40, track.resonance));
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
    let moved = app.filter_image.unwrap();
    assert_ne!(moved, still);
    // Holding, it sends nothing new, and the image it replaced is forgotten.
    assert_eq!(sync(&mut app, &mut graphics), (0, 1));
    assert_eq!(sync(&mut app, &mut graphics), (0, 0));
    app.live = Some(live(0x40, 0xC0));
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
    assert_ne!(app.filter_image.unwrap(), moved);

    // Back on the setting the next frame, as an LFO moves it, it keeps the last for a frame, so
    // it's sent at most every other frame, and then it's drawn from the setting again.
    let resonant = app.filter_image.unwrap();
    app.live = Some(live(track.cutoff, track.resonance));
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    assert_eq!(app.filter_image.unwrap(), resonant);
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
    assert_ne!(app.filter_image.unwrap(), resonant);
}

fn jump_labels(app: &App) -> Vec<String> {
    let track = app.song().tracks[app.cursor.track].clone();
    let jump = app.field_jump.as_ref().unwrap();
    let mut labels: Vec<String> = app
        .track_page
        .rows(Some(&track))
        .iter()
        .flat_map(|row| row.iter().flatten())
        .filter_map(|control| {
            let name = control.label_on(&track);
            jump.label(*control).map(|label| format!("{label} {name}"))
        })
        .collect();
    labels.sort();
    labels
}

#[test]
fn a_jump_labels_matching_settings_nearest_first_and_lands_on_the_value() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4/");
    assert_eq!(app.mode, Mode::Jump);
    assert!(jump_labels(&app).is_empty(), "nothing until you type");
    // Machine and Slice take `a` and `s` for the `c`, and go when the search narrows, leaving the
    // rest as they were.
    press(&mut app, "cu");
    assert_eq!(jump_labels(&app), ["d Cut", "f Low cut", "g High cut"]);

    press(&mut app, "f");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.field_jump.is_none());
    assert_eq!(column_name(&app), "Low cut");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].low_cut, 1, "ready to change");

    // The nearest is now Low cut itself.
    press(&mut app, "/c");
    assert_eq!(jump_labels(&app)[0], "a Low cut");
}

#[test]
fn a_jumps_label_is_never_the_next_letter_and_stays_as_you_type() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/c");
    let first = jump_labels(&app);
    assert!(first.iter().any(|label| label.ends_with("Machine")));
    assert!(
        !first.iter().any(|label| label.starts_with(['h', 'u'])),
        "Machine goes on to h and Cut to u: {first:?}"
    );
    let label_of = |labels: &[String], name: &str| {
        labels
            .iter()
            .find(|label| label.ends_with(name))
            .cloned()
            .unwrap()
    };
    let cut = label_of(&first, " Cut");
    press(&mut app, "u");
    assert_eq!(
        label_of(&jump_labels(&app), " Cut"),
        cut,
        "it keeps its label"
    );
    press(&mut app, "t");
    assert_eq!(app.field_jump.as_ref().unwrap().query, "cut");
    assert_eq!(jump_labels(&app).len(), 3);
    assert_eq!(label_of(&jump_labels(&app), " Cut"), cut);
}

#[test]
fn a_jump_finds_each_row_of_a_repeated_name_and_can_be_left() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    // Every envelope's and every LFO's.
    press(&mut app, " 4L/dep");
    assert_eq!(
        jump_labels(&app).len(),
        vt_model::constants::TRACK_ENVELOPES + vt_model::lfo::LFOS_PER_TRACK,
        "{:?}",
        jump_labels(&app)
    );

    // A slip keeps the search.
    press(&mut app, "Q");
    assert_eq!(app.field_jump.as_ref().unwrap().query, "dep");
    special(&mut app, KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.field_jump.as_ref().unwrap().query, "d");
    special(&mut app, KeyCode::Backspace);
    assert!(jump_labels(&app).is_empty());
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.mode, Mode::Normal);

    press(&mut app, "/dep");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(column_name(&app), "Dest", "it hasn't moved");

    // Enter goes to the nearest.
    press(&mut app, "/dep");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Depth");
    assert_eq!(app.track_page.row, 0);
}

#[test]
fn ctrl_o_comes_back_from_a_jump_to_the_setting_it_left() {
    use vt_tui::track_page::Page;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4jj");
    let start = (app.track_page.row, app.track_page.field);
    press(&mut app, "/res");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Res");
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!((app.track_page.row, app.track_page.field), start);
    assert_eq!(app.view, View::Track);

    // It goes back to the page it left, too.
    press(&mut app, "/res");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "L");
    assert_eq!(app.track_page.page, Page::Mod);
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.track_page.page, Page::Sound);
    assert_eq!((app.track_page.row, app.track_page.field), start);
}

#[test]
fn a_jumps_labels_cover_the_values_and_the_search_shows_below() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/res");
    let backend = ratatui::backend::TestBackend::new(150, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let label = &jump_labels(&app)[0][..1];
    let text = track_text(&app).join("\n");
    assert!(
        text.contains(&format!("Res{label}      {label}000 ")),
        "{text}"
    );
    assert!(text.contains("Cut       FF00 "), "{text}");
    let buffer = terminal.backend().buffer().clone();
    // In the Filter, in from its frame.
    let y = (0..40)
        .find(|y| (3..6).map(|x| buffer[(x, *y)].symbol()).collect::<String>() == "Res")
        .unwrap();
    assert_eq!(buffer[(13, y)].bg, vt_tui::theme::PEACH);

    let backend = ratatui::backend::TestBackend::new(80, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), &app))
        .unwrap();
    let last: String = (0..4)
        .map(|x| terminal.backend().buffer()[(x, 1)].symbol().to_owned())
        .collect();
    assert_eq!(last, "/res");
}

#[test]
fn a_jumps_labels_also_follow_the_names_being_searched() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/cu");
    let line = from_name;
    // Over the letter after what's typed, in the same columns as without a label.
    assert!(line(&app, "Cud").starts_with("Cud       dF00 ▕"));
    assert!(line(&app, "Low cuf").starts_with("Low cuf   f0   ▕"));
    assert!(line(&app, "High cug").starts_with("High cug  gF   ▕"));
    assert!(line(&app, "Res").starts_with("Res       0000 ▕"));
    // It moves along as you type, and past the end of a name it sits just after it.
    press(&mut app, "t");
    assert!(line(&app, "Cutd").starts_with("Cutd      dF00 ▕"));
    special(&mut app, KeyCode::Esc);

    // A name over two rows has both their labels, and the columns stay put.
    press(&mut app, "L/dep");
    let labels = jump_labels(&app);
    let text = track_text(&app);
    let heading = text.iter().find(|line| line.contains("Attack")).unwrap();
    assert!(
        labels.iter().all(|label| label.ends_with("Depth")),
        "{labels:?}"
    );
    let after = &heading[heading.find("Dep").unwrap() + 3..];
    assert!(
        !after.starts_with("th"),
        "two labels over the rest: {heading}"
    );
    assert!(heading.contains("Reset"), "{heading}");
    let at = |line: &str, word: &str| line.find(word).unwrap();
    special(&mut app, KeyCode::Esc);
    let plain = track_text(&app);
    let before = plain.iter().find(|line| line.contains("Attack")).unwrap();
    assert_eq!(at(heading, "Reset"), at(before, "Reset"));
}

#[test]
fn a_jump_works_on_the_mixer_and_stays_on_the_cursors_track() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 6l");
    assert_eq!(app.view, View::Mixer);
    assert_eq!(app.cursor.track, 1);
    press(&mut app, "/s");
    assert_eq!(app.mode, Mode::Jump);
    let jump = app.field_jump.as_ref().unwrap();
    let solo = jump.label(Control::Solo).unwrap();
    assert!(jump.label(Control::Level).is_none());

    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let line = |y: u16| {
        (0..80)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect::<String>()
    };
    // The name keeps its place with the label in it, and only this track's value has one.
    let y = (0..24)
        .find(|y| line(*y).starts_with(&format!("S{solo}lo")))
        .unwrap();
    assert_eq!(buffer[(1, y)].bg, vt_tui::theme::PEACH);
    assert_eq!(buffer[(7, y)].symbol(), "-", "track 1");
    assert_eq!(buffer[(14, y)].symbol(), solo.to_string(), "track 2");
    assert_eq!(buffer[(14, y)].bg, vt_tui::theme::PEACH);

    press(&mut app, &solo.to_string());
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(column_name(&app), "Solo");
    assert_eq!(app.cursor.track, 1);
    assert_eq!(app.view, View::Mixer);

    // Ctrl-O goes back to the column it left.
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(column_name(&app), "Level");
    assert_eq!(app.view, View::Mixer);
}

#[test]
fn slash_only_jumps_on_the_parameter_pages() {
    let (mut app, _realtime) = app();
    press(&mut app, " 3/");
    assert_ne!(app.mode, Mode::Jump);
    assert!(app.field_jump.is_none());
}

#[test]
fn the_sound_page_stops_on_a_values_fine_digits() {
    use vt_model::settings::SettingId;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = on_track_page();
    assert_eq!(column_name(&app), "Cut");
    let cut = |app: &App| {
        let track = &app.song().tracks[0];
        (track.cutoff, track.fine(SettingId::CUT))
    };
    // Cut starts open, at the top, where there's no fine part, so come down a step first.
    press(&mut app, "-");
    assert_eq!(cut(&app), (0xFE, 0));
    press(&mut app, "l");
    assert!(app.track_page.fine);
    assert_eq!(column_name(&app), "Cut", "still Cut, on its fine digits");
    press(&mut app, "i80");
    assert_eq!(cut(&app), (0xFE, 0x80));
    assert!(from_name(&app, "Cut").starts_with("Cut       FE80"));
    // The value's own digits keep it, and the top has none.
    press(&mut app, "h-");
    assert_eq!(cut(&app), (0xFD, 0x80));
    press(&mut app, "l");
    for _ in 0..0x80 {
        press(&mut app, "=");
    }
    assert_eq!(cut(&app), (0xFE, 0), "a fine nudge carries");
    press(&mut app, "-");
    assert_eq!(cut(&app), (0xFD, 0xFF));
    press(&mut app, "x");
    assert_eq!(cut(&app), (0xFD, 0), "x sets it back to 00");
    press(&mut app, "16-");
    assert_eq!(cut(&app), (0xFC, 0xF0));
    assert!(value_shown(&app).is_some());
    press(&mut app, "h==");
    assert_eq!(cut(&app), (0xFE, 0xF0));
    press(&mut app, "=");
    assert_eq!(cut(&app), (0xFF, 0), "none at the top");
    // Drive is whole, so it has no fine digits to stop on.
    press(&mut app, "/dri");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Drive");
    press(&mut app, "l");
    assert!(!app.track_page.fine);
}

#[test]
fn semicolon_on_the_track_page_plays_the_sound_from_the_keyboard() {
    use vt_tui::views::status::column_name;
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    let before = app.song().clone();
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Keys);
    assert_eq!(loudest(&mut realtime, 480), 0.0, "nothing until a key");
    press(&mut app, "a");
    assert!(loudest(&mut realtime, 4800) > 0.01, "a plays a note");
    press(&mut app, ">");
    assert_eq!(app.octave, 4, "> goes up an octave");
    assert!(Arc::ptr_eq(&before, app.song()), "nothing is written");
    // The page's own keys still work, so a setting changes while it plays.
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Detune");
    press(&mut app, "=");
    assert_eq!(machine_value(&app, 0), 0x81);
    assert_eq!(app.mode, Mode::Keys);
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let corner = &terminal.backend().buffer()[(0, 0)];
    assert_eq!((corner.symbol(), corner.fg), ("╭", vt_tui::theme::YELLOW));
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Normal);
    // Leaving the track page leaves the keyboard too.
    press(&mut app, "; 3");
    assert_eq!((app.view, app.mode), (View::Phrase, Mode::Normal));
}

#[test]
fn j_and_k_come_back_to_the_column_a_shorter_row_moved_them_from() {
    use vt_tui::track_page::Control;
    let (mut app, _rt) = app();
    press(&mut app, " 4LL");
    press(&mut app, "K");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    // Gain is the last on EQ3's top row, and the row under it has a hole there.
    press(&mut app, "j$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 7));
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::InsertParam(0, 5));
    press(&mut app, "k");
    assert_eq!(app.track_control(), Control::InsertParam(0, 7));
    // A move along the row forgets the column.
    press(&mut app, "jhk");
    assert_eq!(app.track_control(), Control::InsertParam(0, 2));

    // On the Sound page, through the Filter's last row, which has only its first column, and on
    // into the next device's second.
    press(&mut app, "HHgg}j$");
    assert_eq!(app.track_control().label(), "Low cut");
    press(&mut app, "jj");
    assert_eq!(app.track_control().label(), "Drive");
    press(&mut app, "j");
    assert_eq!(app.track_control().label(), "Send A");
    press(&mut app, "kk");
    assert_eq!(app.track_control().label(), "High cut");
    press(&mut app, "G");
    assert_eq!(app.track_control().label(), "Reset");
    press(&mut app, "gg");
    assert_eq!(app.track_control(), Control::Sample, "G and gg keep it too");
}

/// An app whose first track plays one sample and whose second plays another, and a third with none.
fn app_with_samples() -> (App, Realtime) {
    use vt_model::sample::SampleData;
    let mut song = Song::new(3);
    let mut pool = SamplePool::default();
    for (track, (path, level)) in [("a.wav", 0.5), ("b.wav", -0.25)].iter().enumerate() {
        pool.insert(
            path,
            SampleData {
                frames: vec![[*level; 2]; 100],
            },
        );
        song.sample_on(track, path);
    }
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

#[test]
fn a_samples_waveform_is_sent_once_and_follows_the_cursor_and_the_track() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app_with_samples();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=24,r=3,")
            .count()
    };
    press(&mut app, " 4");
    // The filter's curve and the waveform.
    assert_eq!(sync(&mut app, &mut graphics), 2);
    let plain = app.waveform_image.unwrap();
    assert_ne!(Some(plain), app.filter_image);
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // On the sample, in the Machine's title, it's picked out, once.
    press(&mut app, "l");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.waveform_image.unwrap();
    assert_ne!(picked, plain);
    press(&mut app, "h");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.waveform_image, Some(plain));

    // Another track's sample is drawn for it, and one with no sample has none.
    app.cursor.track = 1;
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.waveform_image, Some(plain));
    app.cursor.track = 2;
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.waveform_image, None);
    // A synth has none either, whatever file it still names.
    app.cursor.track = 0;
    press(&mut app, "=");
    assert_ne!(app.song().tracks[0].machine, Machine::Sampler);
    sync(&mut app, &mut graphics);
    assert_eq!(app.waveform_image, None);
}

#[test]
fn the_filter_curve_and_the_envelopes_are_smooth_at_their_sizes() {
    let (mut app, _realtime) = app();
    let mut graphics = vt_tui::kitty::Graphics::new();
    press(&mut app, " 4");
    let mut out = Vec::new();
    graphics.sync(&mut app, &mut out).unwrap();
    let sent = vt_tui::screenshot::sent_images(&out);
    // The filter's curve keeps its 24 by 3 characters, now at their pixels, in a palette with an
    // alpha for each of its entries: the line at eighths, and the gridlines.
    assert!(String::from_utf8(out).unwrap().contains(",c=24,r=3,"));
    let filter = &sent[&app.filter_image.unwrap()];
    assert_eq!(png_size(filter), (360, 117));
    assert_eq!(png_chunk(filter, b"tRNS").len(), 11);
    // The envelopes keep their 28 by 4.
    press(&mut app, "L");
    let mut out = Vec::new();
    graphics.sync(&mut app, &mut out).unwrap();
    assert_eq!(
        String::from_utf8(out.clone())
            .unwrap()
            .matches(",c=28,r=4,")
            .count(),
        3
    );
    let sent = vt_tui::screenshot::sent_images(&out);
    for id in app.envelope_images.unwrap() {
        assert_eq!(png_size(&sent[&id]), (420, 156));
        assert!(sent[&id].len() < 6000, "{}", sent[&id].len());
    }
}

#[test]
fn a_tracks_speed_on_the_page_is_chosen_from_a_list() {
    use vt_model::speed::Speed;
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let speed = |app: &App| app.song().tracks[0].speed;
    let line = |app: &App| from_name(app, "Speed");
    assert!(line(&app).contains("1/16"), "{}", line(&app));
    // In Play, past Pitch's fine digits.
    press(&mut app, "2}ll");
    assert_eq!(app.track_control(), Control::Speed);
    assert_eq!(value_shown(&app).as_deref(), Some("1/16"));
    press(&mut app, "=");
    assert_eq!(speed(&app), Speed::EighthTriplet);
    press(&mut app, "==");
    assert_eq!(speed(&app), Speed::QuarterTriplet);
    press(&mut app, "====");
    assert_eq!(speed(&app), Speed::Quarter, "it stops at the end");
    assert!(line(&app).contains("1/4"), "{}", line(&app));
    press(&mut app, "x");
    assert_eq!(speed(&app), Speed::Sixteenth);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "/1/8t");
    special(&mut app, KeyCode::Enter);
    assert_eq!(speed(&app), Speed::EighthTriplet);
    press(&mut app, "u");
    assert_eq!(speed(&app), Speed::Sixteenth);
}

#[test]
fn a_tracks_reset_on_the_page_is_typed_in_steps_or_bars() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let reset = |app: &App| app.song().tracks[0].reset;
    let line = |app: &App| from_name(app, "Reset");
    assert!(line(&app).contains("Off"), "{}", line(&app));
    // At the bottom of Play's second column.
    press(&mut app, "2}jj$");
    assert_eq!(app.track_control(), Control::Reset);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: its chain plays on")
    );
    press(&mut app, "=");
    assert_eq!(reset(&app), Some(1));
    press(&mut app, "4=");
    assert_eq!(reset(&app), Some(5));
    assert!(line(&app).contains("5-step"), "{}", line(&app));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every 5 steps of the song")
    );
    // A large step goes to the next whole bar, and back to the one before.
    press(&mut app, "+");
    assert_eq!(reset(&app), Some(16));
    assert!(line(&app).contains("1bar"), "{}", line(&app));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every bar of the song")
    );
    press(&mut app, "+");
    assert_eq!(reset(&app), Some(32));
    press(&mut app, "-");
    assert_eq!(reset(&app), Some(31));
    press(&mut app, "_");
    assert_eq!(reset(&app), Some(16));
    press(&mut app, "_");
    assert_eq!(reset(&app), None, "below a bar is off");

    typed(&mut app, "12");
    assert_eq!(reset(&app), Some(12));
    typed(&mut app, "2bar");
    assert_eq!(reset(&app), Some(32));
    typed(&mut app, "0x40");
    assert_eq!(reset(&app), Some(64));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every 4 bars of the song")
    );
    typed(&mut app, "0");
    assert_eq!(reset(&app), Some(64), "0 isn't a reset");
    assert!(app.status.contains("1 to 1024"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(reset(&app), None);
    press(&mut app, "u");
    assert_eq!(reset(&app), Some(64));
    press(&mut app, "x");
    assert_eq!(reset(&app), None);
    press(&mut app, "-");
    assert_eq!(reset(&app), None, "down from off stays off");
}

#[test]
fn the_mod_page_sets_the_accumulator_and_says_what_each_setting_does() {
    use vt_model::accumulator::{Accumulation, AtLimit, Moves, Reset};
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 4LGk");
    let text = track_text(&app).join("\n");
    assert!(
        text.contains("Up     Down   Moves  Limit  Adds   Reset"),
        "{text}"
    );
    assert!(
        text.contains("Acc       7      7      Row    Wrap   Row    Chain"),
        "{text}"
    );
    assert_eq!(column_name(&app), "Accumulator up");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("7 semitones above the note as written")
    );
    let accumulation = |app: &App| app.song().tracks[0].accumulation;

    // The limits move by one, or seven with `+`, and stop at 1 and 28.
    press(&mut app, "=");
    assert_eq!(accumulation(&app).up, 8);
    press(&mut app, "+++++");
    assert_eq!(accumulation(&app).up, 28);
    press(&mut app, "x");
    assert_eq!(accumulation(&app), Accumulation::default());
    // Typed, it's a number of degrees.
    press(&mut app, "li3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).down, 3);
    press(&mut app, "i40");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).down, 3);
    assert!(app.status.contains("from 1 to 28"), "{}", app.status);

    // The others are lists, which `K` shows with what each does.
    press(&mut app, "l");
    press(&mut app, "K");
    let picker = app.picker.as_ref().expect("the choices are listed");
    assert_eq!(picker.title, "Accumulator moves");
    let items: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(
        items,
        [
            "Row  Only the row with the ACU moves",
            "Track  Every row on the track moves with the ACUs"
        ]
    );
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).moves, Moves::Track);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("every row on the track moves with the ACUs")
    );
    press(&mut app, "l=");
    assert_eq!(accumulation(&app).at_limit, AtLimit::Bounce);
    press(&mut app, "ll==");
    assert_eq!(accumulation(&app).reset, Reset::Loops(1));
    let text = track_text(&app).join("\n");
    assert!(text.contains("Track  Bounce Row    1 loop"), "{text}");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("each time round the chain")
    );
}

#[test]
fn a_return_has_no_accumulator() {
    let (mut app, _realtime) = app();
    let returns = app
        .song()
        .tracks
        .iter()
        .position(|track| !track.is_sound())
        .unwrap();
    for _ in 0..returns {
        press(&mut app, "W");
    }
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(!text.contains("Acc "), "{text}");
}

#[test]
fn the_mod_page_sets_the_arpeggio_and_says_what_each_setting_does() {
    use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio};
    use vt_tui::views::status::{column_name, value_shown};
    use vt_tui::views::widgets::toggle;
    let (mut app, _realtime) = app();
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(text.contains("Rate   Mode        Oct    Retrig"), "{text}");
    assert!(
        text.contains(&format!(
            "Arp       Tick   Up          1      {}",
            toggle::text(false)
        )),
        "{text}"
    );
    assert_eq!(column_name(&app), "Arp rate");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("six notes to a step, a tick, as DEL counts")
    );
    let arpeggio = |app: &App| app.song().tracks[0].arpeggio;

    // The rates go from the shortest note to the longest, and stop at either end.
    press(&mut app, "=");
    assert_eq!(arpeggio(&app).rate, ArpRate::SixtyFourth);
    press(&mut app, "---");
    assert_eq!(arpeggio(&app).rate, ArpRate::HundredTwentyEighth);
    press(&mut app, "x");
    assert_eq!(arpeggio(&app), Arpeggio::default());
    // `K` lists them with how many notes each plays to a step.
    press(&mut app, "K");
    let picker = app.picker.as_ref().expect("the rates are listed");
    assert_eq!(picker.title, "Arp rate");
    assert_eq!(picker.current().unwrap().label, "Tick");
    let items: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(items[4], "1/32  Two notes to a step");
    assert_eq!(items.len(), 9);
    press(&mut app, "jjj");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).rate, ArpRate::ThirtySecond);

    // The modes go round, and `K` lists them.
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Arp mode");
    press(&mut app, "-");
    assert_eq!(arpeggio(&app).mode, ArpMode::Random);
    press(&mut app, "K");
    let items: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(items[2], "Up-down  Up and back down, the ends played once");
    assert_eq!(
        items[3],
        "Up-down-inc  Up and back down, the ends played twice"
    );
    press(&mut app, "kk");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).mode, ArpMode::UpDown);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("up and back down, the ends played once")
    );

    // The octaves are a number from 1 to 4.
    press(&mut app, "l====");
    assert_eq!(arpeggio(&app).octaves, 4);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("4 octaves: the notes climb that many before it starts again")
    );
    press(&mut app, "i2");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).octaves, 2);
    press(&mut app, "i5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).octaves, 2);
    assert!(app.status.contains("from 1 to 4"), "{}", app.status);

    // Retrig is on or off.
    press(&mut app, "l=");
    assert!(arpeggio(&app).retrig);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("each note starts again, with its envelopes")
    );
    let text = track_text(&app).join("\n");
    assert!(
        text.contains(&format!(
            "Arp       1/32   Up-down     2      {}",
            toggle::text(true)
        )),
        "{text}"
    );
    press(&mut app, "x");
    assert!(!arpeggio(&app).retrig);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("legato: each note only changes the pitch, and Acid slides")
    );

    // The longest mode has room, and the columns after it stay in line.
    press(&mut app, "hh+");
    assert_eq!(arpeggio(&app).mode, ArpMode::UpDownInc);
    let text = track_text(&app).join("\n");
    assert!(
        text.contains(&format!(
            "Arp       1/32   Up-down-inc 2      {}",
            toggle::text(false)
        )),
        "{text}"
    );
}

#[test]
fn a_return_has_no_arpeggio() {
    let (mut app, _realtime) = app();
    let returns = app
        .song()
        .tracks
        .iter()
        .position(|track| !track.is_sound())
        .unwrap();
    for _ in 0..returns {
        press(&mut app, "W");
    }
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(!text.contains("Arp "), "{text}");
}

#[test]
fn two_state_settings_are_toggles_on_the_fx_and_mod_pages_and_nothing_moves() {
    use vt_model::insert::Insert;
    use vt_tui::glyphs::{SWITCH_OFF, SWITCH_ON};
    use vt_tui::theme;
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_fx_page();
    four_devices(&mut app);
    let text = fx_page_text(&app).join("\n");
    for word in [" Off ", " On "] {
        assert!(!text.contains(word), "{word} in\n{text}");
    }
    // Where a setting's name is, by its row and the text it starts with.
    let find = |app: &App, name: &str| {
        let lines = fx_page_text(app);
        lines
            .iter()
            .enumerate()
            .find_map(|(y, line)| {
                let at = line.find(name)?;
                Some((line[..at].chars().count() as u16, y as u16))
            })
            .unwrap_or_else(|| panic!("{name}"))
    };
    let value_of = |app: &App, name: &str| {
        let (x, y) = find(app, name);
        // The value is after the name's 10 columns.
        glyph_at(&track_page_buffer(app), (x + 10, y))
    };
    let off = (SWITCH_OFF, theme::OVERLAY0, Color::Reset);
    let on = (SWITCH_ON, theme::GREEN, Color::Reset);
    // Glue's Auto and Peak Clip start off, and the Delay's Ping-pong on.
    for name in ["Auto", "Peak Clip"] {
        assert_eq!(value_of(&app, name), off, "{name}");
    }
    assert_eq!(value_of(&app, "Ping-pong"), on);
    // The Delay's Ping-pong and the Drive beside it are a column apart, as every column is.
    let (ping, row) = find(&app, "Ping-pong");
    assert_eq!(find(&app, "Drive"), (ping + 27, row));

    // `=` on Peak Clip flips it, in the cursor's colours while it's there and green after.
    press(&mut app, "ggj$j");
    assert_eq!(app.track_control(), Control::InsertParam(0, 10));
    press(&mut app, "=");
    assert!(matches!(
        app.song().tracks[0].inserts[0],
        Insert::Compressor(compressor) if compressor.peak_clip == 1
    ));
    let (glyph, _, background) = value_of(&app, "Peak Clip");
    assert_eq!((glyph, background), (SWITCH_ON, theme::ROSEWATER));
    assert!(status_line(&app).contains("On"), "{}", status_line(&app));
    press(&mut app, "k");
    assert_eq!(value_of(&app, "Peak Clip"), on);
    // Bypassed, it fades as the rest of the slot does.
    press(&mut app, "b");
    assert_eq!(value_of(&app, "Peak Clip").1, theme::OVERLAY0);

    // On the Mod page, an envelope's Reset and the LFOs' and the arpeggio's Retrig, under their
    // names.
    press(&mut app, "H");
    let buffer = track_page_buffer(&app);
    let lines: Vec<String> = (0..40)
        .map(|y| (0..150).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let column = |heading: &str, y: usize| {
        let at = lines[y].find(heading).unwrap();
        lines[y][..at].chars().count() as u16
    };
    let envelopes = lines
        .iter()
        .position(|line| line.contains("Reset"))
        .unwrap();
    let reset = column("Reset", envelopes);
    for row in 1..=3 {
        let y = (envelopes + row) as u16;
        assert_eq!(
            glyph_at(&buffer, (reset, y)).0,
            SWITCH_ON,
            "{}",
            lines[envelopes + row]
        );
    }
    let lfos = envelopes + 5;
    let retrig = column("Retrig", lfos);
    assert_eq!(glyph_at(&buffer, (retrig, lfos as u16 + 1)).0, SWITCH_OFF);
    let arp = lines
        .iter()
        .position(|line| line.starts_with("Arp"))
        .unwrap();
    let arp_retrig = column("Retrig", arp - 1);
    assert_eq!(glyph_at(&buffer, (arp_retrig, arp as u16)), off);
    // `=` on the first envelope's Reset flips it, and the next is still under its heading.
    press(&mut app, "gg$");
    assert_eq!(app.track_control().label(), "Reset");
    press(&mut app, "=");
    assert!(!app.song().tracks[0].envelopes[0].reset);
    let buffer = track_page_buffer(&app);
    assert_eq!(
        glyph_at(&buffer, (reset, envelopes as u16 + 1)).0,
        SWITCH_OFF
    );
    assert_eq!(buffer[(reset, envelopes as u16)].symbol(), "R");
}

#[test]
fn the_track_page_sets_how_long_a_tie_glides() {
    use vt_model::settings::glide_value;
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let glide = |app: &App| app.song().tracks[0].glide;
    let line = |app: &App| from_name(app, "Glide");
    assert!(line(&app).contains("Off"), "{}", line(&app));

    // Under Pitch, in Play.
    press(&mut app, "2}j");
    assert_eq!(app.track_control(), Control::Glide);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: a tie jumps to the next note")
    );
    press(&mut app, "=");
    assert_eq!(glide(&app), 1);
    assert_eq!(value_shown(&app).as_deref(), Some("2 ms"));
    press(&mut app, "+");
    assert_eq!(glide(&app), 0x11, "sixteen more");
    typed(&mut app, "60");
    assert_eq!(glide(&app), glide_value(60.0));
    assert!(line(&app).contains("60 ms"), "{}", line(&app));
    typed(&mut app, "5000");
    assert_eq!(glide(&app), glide_value(60.0));
    assert!(app.status.contains("up to 2000"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(glide(&app), 0);
    press(&mut app, "300=");
    assert_eq!(glide(&app), 0xFF, "it stops at the top");
    assert!(line(&app).contains("2.00 s"), "{}", line(&app));
    press(&mut app, "x");
    assert_eq!(glide(&app), 0, "back to off");
    press(&mut app, "u");
    assert_eq!(glide(&app), 0xFF);
    press(&mut app, "x");

    // Off on `Acid`, a tie slides as a 303's does, by the voice's own `Slide`.
    press(&mut app, "ggh");
    press(&mut app, "K");
    press(&mut app, "/acid");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Acid)
    );
    assert!(line(&app).contains("Slide"), "{}", line(&app));
    press(&mut app, "gg3}j");
    assert_eq!(app.track_control(), Control::Glide);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: a tie slides by Acid's own Slide")
    );
}

#[test]
fn the_sound_page_sets_a_samplers_start_length_and_play() {
    use vt_model::settings::PLAY_CHOICES;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let setting = |app: &App, id| app.song().tracks[0].setting(id);
    // In the Machine above, beside the slice.
    press(&mut app, "{l");
    assert_eq!(app.track_control().label_on(&app.song().tracks[0]), "Start");
    typed(&mut app, "40");
    assert_eq!(setting(&app, SettingId::SAMPLE_START), 0x40);
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
    press(&mut app, "j_");
    assert_eq!(setting(&app, SettingId::SAMPLE_LENGTH), 0xEF);
    press(&mut app, "h=");
    assert_eq!(setting(&app, SettingId::PLAY), 1);
    assert!(from_name(&app, "Play").contains(PLAY_CHOICES[1]));
    press(&mut app, "===");
    assert_eq!(setting(&app, SettingId::PLAY), 3, "Thru is the last");
    assert!(from_name(&app, "Play").starts_with("Play      Thru "));
    // Above Play, the slice, which plays the sample whole until it's sliced.
    press(&mut app, "k");
    assert_eq!(app.track_control().label_on(&app.song().tracks[0]), "Slice");
    assert!(from_name(&app, "Slice").starts_with("Slice     00   whole "));
    press(&mut app, "==");
    assert_eq!(setting(&app, SettingId::SLICE), 2);
    press(&mut app, "j");
    press(&mut app, "x");
    assert_eq!(setting(&app, SettingId::PLAY), 0);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker, "K lists the ways to play");
    special(&mut app, KeyCode::Esc);
}

#[test]
fn the_track_page_puts_a_track_in_a_group_and_a_group_has_its_own_page() {
    let (mut app, _realtime) = on_track_page();
    let group = |app: &App| app.song().tracks[0].group;
    // Under Send B, in the Output.
    press(&mut app, "}jj$");
    assert_eq!(app.track_control().label(), "Group");
    assert!(from_name(&app, "Group").starts_with("Group     Off "));
    press(&mut app, "=");
    assert_eq!(group(&app), Some(1));
    assert!(from_name(&app, "Group").starts_with("Group     G1 "));
    press(&mut app, "9=");
    assert_eq!(group(&app), Some(4), "4 is the last");
    typed(&mut app, "g2");
    assert_eq!(group(&app), Some(2));
    typed(&mut app, "5");
    assert_eq!(group(&app), Some(2));
    assert!(app.status.contains("1 to 4"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(group(&app), None);
    press(&mut app, "u");
    assert_eq!(group(&app), Some(2));
    press(&mut app, "x");
    assert_eq!(group(&app), None);

    // A group's own page has its trim, level, pan and sends, on the right where a track's output
    // is.
    // The empty groups are folded away, so this is with them shown.
    let at = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    app.folds.set_all(false);
    to_track(&mut app, at);
    let lines = track_page_text(&app);
    assert!(lines[0].contains("Group 2"), "{}", lines[0]);
    let output = lines[2].find("Trim").unwrap();
    assert!(output > 30, "{}", lines[2]);
    let labels = [
        (2, "Trim"),
        (3, "Level"),
        (4, "Pan"),
        (5, "Send A"),
        (6, "Send B"),
    ];
    for (line, label) in labels {
        assert_eq!(lines[line].find(label), Some(output), "{}", lines[line]);
    }
    press(&mut app, "ggjjjj");
    assert_eq!(app.track_control().label(), "Send B");
    typed(&mut app, "60");
    assert_eq!(app.song().tracks[at].send_b, 0x60);
}

#[test]
fn a_groups_page_lists_its_tracks_with_their_mutes_and_solos() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "track add 6");
    command(&mut app, "group 1 2-4");
    let at = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, &"W".repeat(at));
    let lines = track_page_text(&app);
    assert!(lines[2].starts_with("Tracks    02"), "{}", lines[2]);
    assert!(lines[3].starts_with("          03"), "{}", lines[3]);
    assert!(lines[4].starts_with("          04"), "{}", lines[4]);
    assert!(!lines[5].contains("05"), "{}", lines[5]);
    // The output stays beside the list.
    assert!(lines[2].contains("Trim"), "{}", lines[2]);
    assert!(lines[3].contains("Level"), "{}", lines[3]);
    app.levels[2].mute = true;
    assert!(
        track_page_text(&app)[3].contains(vt_tui::glyphs::MUTE),
        "{}",
        track_page_text(&app)[3]
    );
    // An empty group says so. It's folded away, so `W` steps over it, and the picker reaches it.
    let empty = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    press(&mut app, "W");
    assert_eq!(app.cursor.track, empty + MAX_GROUPS - 1, "on A");
    press(&mut app, " fbG2");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, empty);
    assert!(track_page_text(&app)[2].starts_with("Tracks    None"));
}

#[test]
fn a_groups_icon_and_its_tracks_group_are_in_its_colour() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "group 2");
    let color = vt_tui::theme::group(2);
    let cell_color = |app: &App, text: &str| {
        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let lines: Vec<String> = (0..24)
            .map(|y| (0..120).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let (y, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains(text))
            .unwrap();
        let column = line[..line.find(text).unwrap()].chars().count();
        buffer[(column as u16, y as u16)].fg
    };
    assert_eq!(cell_color(&app, "G2"), color, "the track's Group");
    let at = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    to_track(&mut app, at);
    let icon = vt_tui::glyphs::GROUP.to_string();
    assert_eq!(cell_color(&app, &icon), color, "the group's icon");
}

#[test]
fn the_trim_sits_above_the_level_and_reads_in_db() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let trim = |app: &App| app.song().tracks[0].trim;
    // At the top of the Output, under the Filter.
    press(&mut app, "}");
    assert_eq!(app.track_control(), Control::Trim);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.0 dB"));
    let depth = app.history.undo_depth();
    press(&mut app, "=");
    assert_eq!(trim(&app), 0x81);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.25 dB"));
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "+");
    assert_eq!(trim(&app), 0x91, "a large step is 4 dB");
    // Unlike a level, it runs to FF.
    press(&mut app, "iFF");
    assert_eq!(trim(&app), 0xFF);
    assert_eq!(value_shown(&app).as_deref(), Some("+31.75 dB"));
    press(&mut app, "i00");
    assert_eq!(value_shown(&app).as_deref(), Some("-inf dB"));
    press(&mut app, "x");
    assert_eq!(trim(&app), 0x80, "back to 0 dB");
    press(&mut app, "u");
    assert_eq!(trim(&app), 0x00);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Level);
    assert_eq!(app.song().tracks[0].level, 0xE0, "the fader is left alone");
}

/// The whole screen at the author's size, kitty full screen on a MacBook, a line at a time.
fn full_screen_text(app: &mut App) -> Vec<String> {
    let (width, height) = (163, 42);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn every_slot_envelope_and_lfo_fits_on_the_authors_screen() {
    use vt_model::constants::TRACK_ENVELOPES;
    use vt_model::insert::{INSERTS, Insert};
    use vt_model::lfo::LFOS_PER_TRACK;
    let mut song = Song::new(1);
    // A different effect in each slot, with a compressor's curve beside the last.
    song.tracks[0].inserts = [
        Insert::ALL[1],
        Insert::ALL[2],
        Insert::ALL[3],
        Insert::ALL[6],
    ];
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, " 4LL");
    let fx = full_screen_text(&mut app);
    let names = ["Distort", "Delay", "Reverb", "Comp"];
    assert_eq!(names.len(), INSERTS);
    for (slot, name) in names.iter().enumerate() {
        // Each slot's device, with its settings in its frame.
        let top = slot_title(slot, Some(true), name);
        let at = fx.iter().position(|line| line.starts_with(&top));
        let at = at.unwrap_or_else(|| panic!("{top} in\n{}", fx.join("\n")));
        assert!(fx[at + 2].starts_with("│  "), "{}", fx[at + 2]);
        let settings = fx[at + 2].trim_start_matches('│').trim();
        assert!(!settings.is_empty(), "{name}'s first row");
        assert!(fx[at + 8].starts_with('╰'), "{}", fx[at + 8]);
    }
    press(&mut app, "H");
    let modulation = full_screen_text(&mut app);
    let count = |text: &str| -> usize {
        modulation
            .iter()
            .map(|line| line.matches(text).count())
            .sum()
    };
    for envelope in 1..=TRACK_ENVELOPES {
        let name = format!("Env {envelope}");
        assert_eq!(
            count(&name),
            2,
            "{name}'s row and its shape\n{}",
            modulation.join("\n")
        );
    }
    for lfo in 1..=LFOS_PER_TRACK {
        assert_eq!(count(&format!("LFO {lfo}")), 1, "{}", modulation.join("\n"));
    }
}

#[test]
fn the_last_slot_lfo_and_envelope_are_set_locked_and_saved() {
    use vt_model::constants::TRACK_ENVELOPES;
    use vt_model::insert::{INSERTS, Insert};
    use vt_model::lfo::LFOS_PER_TRACK;
    use vt_model::settings::Effect;
    use vt_model::step::FxCommand;
    use vt_tui::track_page::{Control, EnvelopeField, LfoField};
    let (mut app, _realtime) = app();
    // A delay in the last slot.
    press(&mut app, " 4LL");
    press(&mut app, &"}".repeat(INSERTS - 1));
    assert_eq!(app.track_control(), Control::Insert(INSERTS - 1));
    press(&mut app, "K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    assert!(matches!(
        app.song().tracks[0].inserts[INSERTS - 1],
        Insert::Delay(_)
    ));

    // A step switches it out by its digit.
    press(&mut app, " 3lllllK");
    press(&mut app, &format!("{INSERTS}byp"));
    let item = app.picker.as_ref().unwrap().current().unwrap();
    assert_eq!(listed(item), format!("{INSERTS}BYP  {INSERTS} Bypass"));
    special(&mut app, KeyCode::Enter);
    press(&mut app, "01");
    let bypass = SettingId::bypass(INSERTS - 1).unwrap();
    let step = &app.phrase_here().unwrap().steps()[0];
    assert_eq!(step.fx[0].command, Some(FxCommand::Set(bypass)));

    // The last LFO on the delay's feedback, and the last envelope on the last LFO's depth.
    press(&mut app, " 4H");
    press(&mut app, &"j".repeat(TRACK_ENVELOPES + LFOS_PER_TRACK - 1));
    press(&mut app, "0");
    assert_eq!(
        app.track_control(),
        Control::Lfo(LFOS_PER_TRACK - 1, LfoField::Destination)
    );
    press(&mut app, "K/");
    press(&mut app, "feedback");
    special(&mut app, KeyCode::Enter);
    let feedback = SettingId::slot(INSERTS - 1, Effect::Delay, 1).unwrap();
    assert_eq!(
        app.song().tracks[0].lfos[LFOS_PER_TRACK - 1].destination,
        Some(feedback)
    );
    press(&mut app, &"k".repeat(LFOS_PER_TRACK));
    assert_eq!(
        app.track_control(),
        Control::Envelope(TRACK_ENVELOPES - 1, EnvelopeField::Destination)
    );
    press(&mut app, "K/");
    press(&mut app, &format!("l{LFOS_PER_TRACK} depth"));
    special(&mut app, KeyCode::Enter);
    let depth = SettingId::lfo(LFOS_PER_TRACK - 1, 0).unwrap();
    assert_eq!(
        app.song().tracks[0].envelopes[TRACK_ENVELOPES - 1].destination,
        Some(depth)
    );

    // All of it comes back from the saved song.
    let folder = folder();
    command(&mut app, &format!("w {}", folder.display()));
    let saved = vt_model::project::load_song(&folder).unwrap();
    let track = &saved.tracks[0];
    assert_eq!(track.inserts, app.song().tracks[0].inserts);
    assert_eq!(track.lfos[LFOS_PER_TRACK - 1].destination, Some(feedback));
    assert_eq!(
        track.envelopes[TRACK_ENVELOPES - 1].destination,
        Some(depth)
    );
    let step = track.phrases[&0].steps()[0].to_string();
    assert!(step.contains(&format!("{INSERTS}.bypass:01")), "{step}");
    std::fs::remove_dir_all(&folder).unwrap();
}
