//! A lane's division of its own, `:lane 4/2` and `:lane /3`, and the track's `Length` and `Rate`
//! for each FX column on the Gen page's `Lane`, which move every phrase's lane on the column
//! (decisions 346 to 348, the lanes study).

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_model::lane::{LANE_RATES, Lane};
use vt_model::macros::MacroTarget;
use vt_model::settings::SettingId;
use vt_tui::app::App;
use vt_tui::controls::{Control, LaneField, Page};
use vt_tui::input::mode::Mode;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

fn lane(app: &App) -> Option<Lane> {
    app.song().tracks[0].phrases[&0].lane(0)
}

/// Sixteen empty rows, the cursor on FX 1's command on the third.
fn on_fx1() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&[E; 16]);
    press(&mut app, "llllljj");
    (app, realtime)
}

/// The Gen page, on the device after the macros', with FX 1 given a lane of 4 if `with_lane`.
fn on_gen_page(with_lane: bool) -> (App, Realtime) {
    let (mut app, realtime) = on_fx1();
    if with_lane {
        command(&mut app, "lane 4");
    }
    keys(&mut app, " 5LLL");
    assert_eq!(app.track_page.page, Page::Gen);
    keys(&mut app, "}");
    (app, realtime)
}

#[test]
fn lane_takes_a_division_after_a_slash_and_a_slash_alone_changes_only_that() {
    let (mut app, _realtime) = on_fx1();
    command(&mut app, "lane 4/2");
    let two = Lane {
        division: 2,
        ..Lane::new(2, 4)
    };
    assert_eq!(lane(&app), Some(two));
    assert!(
        app.services.status.contains("2 steps each"),
        "{}",
        app.services.status
    );
    command(&mut app, "lane /3");
    assert_eq!(lane(&app), Some(Lane { division: 3, ..two }));
    // A new length keeps the lane's division, and `:len` its start and division.
    command(&mut app, "len 5");
    assert_eq!(
        lane(&app).map(|lane| (lane.length, lane.division)),
        Some((5, 3))
    );
    command(&mut app, "lane 17/17");
    assert!(
        app.services.status.contains("1 to 16"),
        "{}",
        app.services.status
    );
    command(&mut app, "lane");
    command(&mut app, "lane /2");
    assert_eq!(lane(&app), None);
    assert!(
        app.services.status.contains("lane first"),
        "{}",
        app.services.status
    );
}

#[test]
fn hold_sits_under_length_and_turns_as_the_tracks_own() {
    let (mut app, _realtime) = on_gen_page(true);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Hold      00"), "{text}");
    keys(&mut app, "j");
    assert_eq!(app.track_control(), Control::Lane(0, LaneField::Hold));
    keys(&mut app, "i80<Enter>");
    assert_eq!(app.song().tracks[0].lanes[0].hold, 0x80);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Hold      80"), "{text}");
}

#[test]
fn the_gen_page_shows_a_lane_device_only_for_a_column_with_a_lane() {
    let (app, _realtime) = on_gen_page(false);
    let text = fx_page_text(&app).join("\n");
    assert!(!text.contains("FX1 Lane"), "{text}");
    assert_eq!(
        app.track_control(),
        Control::Base(vt_model::action::TrackControl::Order)
    );
    let (app, _realtime) = on_gen_page(true);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("FX1 Lane"), "{text}");
    assert!(text.contains("· here 4 rows, as written"), "{text}");
    assert_eq!(app.track_control(), Control::Lane(0, LaneField::Length));
}

#[test]
fn length_and_rate_turn_reset_and_list_as_the_tracks_own() {
    let (mut app, _realtime) = on_gen_page(true);
    keys(&mut app, "==");
    assert_eq!(app.song().tracks[0].lanes[0].length, 0x82);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("· here 6 rows, its own 4 rows"), "{text}");
    assert!(text.contains("+2"), "{text}");
    press(&mut app, "u");
    assert_eq!(
        app.song().tracks[0].lanes[0].length,
        0x81,
        "one undo step a press"
    );
    keys(&mut app, "x");
    assert_eq!(app.song().tracks[0].lanes[0].length, 0x80);
    keys(&mut app, "i-3<Enter>");
    assert_eq!(app.song().tracks[0].lanes[0].length, 0x7D, "typed in rows");
    keys(&mut app, "x");
    // `Rate` is a list: `K` lists its 31 choices, opened on `×1`, `=` goes faster, and `+` to
    // the end, the fastest.
    keys(&mut app, "l");
    assert_eq!(app.track_control(), Control::Lane(0, LaneField::Rate));
    keys(&mut app, "K");
    assert_eq!(app.modes.mode, Mode::Picker);
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!(picker.items().len(), LANE_RATES.len());
    assert_eq!(picker.current().unwrap().label, "×1");
    special(&mut app, KeyCode::Esc);
    keys(&mut app, "=");
    assert_eq!(app.song().tracks[0].lanes[0].rate, 16, "×2");
    let line = status_line(&app);
    assert!(
        line.contains("×2: each row of a lane on FX1 lasts its own steps over 2, faster"),
        "{line}"
    );
    assert!(
        line.contains("a row a step is as fast as a lane goes"),
        "{line}"
    );
    keys(&mut app, "--");
    let line = status_line(&app);
    assert!(
        line.contains("÷2: each row of a lane on FX1 lasts 2 times its own steps, slower"),
        "{line}"
    );
    keys(&mut app, "+");
    assert_eq!(app.song().tracks[0].lanes[0].rate, 30, "×16");
}

#[test]
fn m_maps_a_lanes_length_and_gd_goes_to_its_macro() {
    let (mut app, _realtime) = on_gen_page(true);
    keys(&mut app, "M1");
    let length = MacroTarget::Setting(SettingId::lane(0, 0).unwrap());
    let track = &app.song().tracks[0];
    assert_eq!(track.macro_holding(length).map(|(at, _)| at), Some(0));
    assert_eq!(length.name_on(track), "FX1 Length");
    // A setting a macro holds is turned on the macro.
    keys(&mut app, "=");
    assert!(
        app.services.status.contains("on macro 1"),
        "{}",
        app.services.status
    );
    keys(&mut app, "gd");
    assert_eq!(app.track_control(), Control::Macro(0));
    assert_eq!(app.modes.mode, Mode::Normal);
}

#[test]
fn the_title_shows_what_a_lane_is_heard_as_and_its_own_and_the_playhead_the_row_copied() {
    let (mut app, _realtime) = on_fx1();
    keys(&mut app, "rSNA");
    special(&mut app, KeyCode::Enter);
    keys(&mut app, "20");
    command(&mut app, "lane 4/2");
    on_the_row_being_edited(&mut app);
    let title = |app: &App| -> String {
        let buffer = screen(app);
        (0..100).map(|x| buffer[(x, 0)].symbol()).collect()
    };
    assert!(title(&app).contains("FX1:4/2"), "{}", title(&app));
    // Two more rows at `÷2`, half as fast: what's heard first, its own after.
    let moved = vt_model::action::Action::SetLaneMove {
        track: 0,
        slot: 0,
        moved: vt_model::lane::LaneMove {
            length: 0x82,
            rate: 14,
            start: 0,
            jump: 0,
            hold: 0,
        },
    };
    app.services.apply(&moved);
    assert!(title(&app).contains("FX1:6/4 (4/2)"), "{}", title(&app));
    // Heard at 6 rows of 4 steps, step 18 is the lane's row 4, a copy of its first, row 2.
    app.services.reported.transport = Transport::Playing;
    app.services.reported.jam[0].chain = Some(0);
    app.set_playhead(18);
    app.phrase_view.top = 0;
    let buffer = screen(&app);
    let lines: Vec<String> = (0..24)
        .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let playing = lines
        .iter()
        .find(|line| line.contains(">SNA"))
        .unwrap_or_else(|| panic!("no lane playhead:\n{}", lines.join("\n")));
    assert!(playing.contains(" 02 "), "{playing}");
}

#[test]
fn rows_past_a_shortened_lane_are_dim() {
    let sends = "--- -- sna:20 --- ---";
    let (mut app, _realtime) = app_with(&[E, E, sends, sends, sends, sends, E, E]);
    press(&mut app, "llllljj");
    command(&mut app, "lane 4");
    // Off the lane, so no row of it is the cursor's.
    press(&mut app, "jjjjj");
    on_the_row_being_edited(&mut app);
    // The style of the send's command on each row it's on, in order.
    let styles = |app: &App| {
        let buffer = screen(app);
        (0..24)
            .filter_map(|y| {
                let line: String = (0..100).map(|x| buffer[(x, y)].symbol()).collect();
                let x = line.find("SNA")? as u16;
                Some(buffer[(x, y)].style())
            })
            .collect::<Vec<_>>()
    };
    let before = styles(&app);
    assert_eq!(before.len(), 4);
    assert!(
        before.iter().all(|style| style.fg == before[0].fg),
        "all four lit: {before:?}"
    );
    app.services.apply(&vt_model::action::Action::SetLaneMove {
        track: 0,
        slot: 0,
        moved: vt_model::lane::LaneMove {
            length: 0x7E,
            rate: 15,
            start: 0,
            jump: 0,
            hold: 0,
        },
    });
    // At -2, the lane of 4 plays its first two, and the two past them are dim.
    let after = styles(&app);
    assert_eq!(after[0].fg, before[0].fg, "its first two still play");
    assert_eq!(after[1].fg, before[1].fg);
    assert_ne!(after[2].fg, before[2].fg, "past the length heard, dim");
    assert_ne!(after[3].fg, before[3].fg);
}

#[test]
fn the_kria_patchs_loop_knobs_turn_its_lanes() {
    let (mut app, _realtime) = app_with(&[E; 16]);
    command(&mut app, "patch");
    press(&mut app, "/kria");
    special(&mut app, KeyCode::Enter);
    let knobs = [("Length loop", 4), ("Ratchet loop", 3), ("Chance loop", 7)];
    for (slot, (name, rows)) in knobs.into_iter().enumerate() {
        let track = &app.song().tracks[0];
        let at = track
            .macros
            .iter()
            .position(|held| held.name.as_deref() == Some(name))
            .unwrap();
        let length = MacroTarget::Setting(SettingId::lane(slot, 0).unwrap());
        assert_eq!(track.macro_holding(length).map(|(on, _)| on), Some(at));
        assert_eq!(
            track.phrases[&0].lane(slot).map(|lane| lane.length),
            Some(rows)
        );
        // At its bottom it's a lane of 1, at its top 16, and it starts where the patch was saved.
        let heard = |app: &App, value: u16| {
            let map = app.song().tracks[0].macros[at].maps[0];
            let held = map.at(f32::from(value) / 256.0);
            app.song().tracks[0].phrases[&0]
                .lane(slot)
                .unwrap()
                .heard_length(vt_model::lane::length_rows((held >> 8) as u8))
        };
        assert_eq!(heard(&app, 0), 1, "{name}");
        assert_eq!(heard(&app, 0xFF00), 16, "{name}");
        let value = app.song().tracks[0].macros[at].value;
        assert_eq!(heard(&app, value), rows, "{name}");
    }
}
