//! The cells each track page's `CellSource` gives its grid, and the titles of its devices, pinned as
//! they're drawn today, styles and all, in the states the screenshots don't show: the cursor on a
//! value or its fine digits or off it, dim at a default, typing, a jump's labels, what's playing
//! moving the bar off what's set, a macro's range, a fader the steps drive and one taken over from
//! them, a slot a step's lock switches out, words, choices and toggles. They're drawn through the
//! whole page, so one way of building the cells can be swapped for another (the UI plan's U4) and
//! checked to draw the same.

use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModField;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Phrase, Role, Song, SynthVoice, TrackKind};
use vt_model::step::Step;

use crate::app::App;
use crate::controls::{Control, Page};
use crate::input::mode::View;
use crate::widgets::pin::{app, app_with, assert_drawn, cut, press, special};

const CUT: Control = Control::Base(TrackControl::Cutoff);
const RES: Control = Control::Base(TrackControl::Resonance);

/// The track page as it's drawn beside the main meter at 163x42, or Gen while it shows.
fn page(app: &App) -> Buffer {
    let view = if app.modes.view == View::Gen {
        View::Gen
    } else {
        View::Track
    };
    let mut terminal = Terminal::new(TestBackend::new(150, 40)).unwrap();
    terminal
        .draw(|frame| crate::run::draw_view(frame, frame.area(), app, view))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Where a setting's name first starts on the page, two spaces after something, as its cell does.
fn name_at(buffer: &Buffer, name: &str) -> (u16, u16) {
    let area = buffer.area;
    let wanted = format!("  {name} ");
    for y in 0..area.height {
        let row: String = (0..area.width)
            .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect();
        if let Some(at) = row.find(&wanted) {
            let column = row[..at].chars().count() as u16 + 2;
            return (column, y);
        }
    }
    panic!("no {name:?} on the page");
}

/// A setting's cell on the page, this wide from its name.
fn cell(app: &App, name: &str, width: u16) -> Buffer {
    let buffer = page(app);
    let (x, y) = name_at(&buffer, name);
    cut(&buffer, Rect::new(x, y, width, 1))
}

/// A row of the page from a column on, this wide.
fn row(app: &App, y: u16, x: u16, width: u16) -> Buffer {
    cut(&page(app), Rect::new(x, y, width, 1))
}

/// The setting of the page showing with this name, on the cursor's track.
fn named(app: &App, name: &str) -> Control {
    let track = app.ctx().track_here().unwrap();
    let rows = app.track_page.rows(Some(track));
    let found = rows.iter().flatten().flatten();
    *found
        .into_iter()
        .find(|control| control.name_on(track) == name)
        .unwrap_or_else(|| panic!("no {name:?} on the page"))
}

/// The cursor on a setting of the page showing.
fn put(app: &mut App, control: Control) {
    let rows = app.track_page.rows(app.ctx().track_here());
    let (row, field) = rows
        .iter()
        .enumerate()
        .find_map(|(row, places)| {
            let field = places.iter().position(|place| *place == Some(control))?;
            Some((row, field))
        })
        .unwrap_or_else(|| panic!("no {control:?} on the page"));
    app.track_page.row = row;
    app.track_page.field = field;
    app.track_page.fine = false;
    assert_eq!(app.track_page.control_here(&app.ctx()), control);
}

/// A block of audio and the frame after it, so the latest report is of the song as it is, and
/// live values put in after are drawn against it.
fn catch_up(app: &mut App, realtime: &mut Realtime) {
    let mut out = vec![0.0; 256 * 2];
    realtime.process(&mut out);
    app.tick();
}

/// Live values that are the song's but for what a test changes.
fn live(app: &App) -> LiveValues {
    app.services.reported.live.unwrap_or_default()
}

/// The Sound page of the first track, a sampler.
fn on_sound() -> (App, Realtime) {
    let (mut app, realtime) = app();
    press(&mut app, " 5");
    assert_eq!(app.track_page.page, Page::Sound);
    (app, realtime)
}

#[test]
fn a_value_and_its_fine_digits_take_the_cursor_in_turn() {
    let (mut app, _realtime) = on_sound();
    put(&mut app, CUT);
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       FF00 ▕█████████▏   ",
            "aaaaaaaaaabbaa.cdddddddddc...",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
    app.track_page.fine = true;
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       FF00 ▕█████████▏   ",
            "aaaaaaaaaa..bb.cdddddddddc...",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
    put(&mut app, RES);
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       FF00 ▕█████████▏   ",
            "aaaaaaaaaa..aa.bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
}

#[test]
fn a_centred_bar_keeps_its_middle_and_a_value_with_no_fine_part_its_room() {
    let (app, _realtime) = on_sound();
    assert_drawn(
        &cell(&app, "Pan", 29),
        &[
            "Pan       8000 ▕    │    ▏   ",
            "aaaaaaaaaa..aa.bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
    assert_drawn(
        &cell(&app, "Trim", 29),
        &[
            "Trim      80   ▕    │    ▏   ",
            "aaaaaaaaaa.....bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
    assert_drawn(
        &cell(&app, "Drive", 29),
        &[
            "Drive     0000 ▕         ▏   ",
            "aaaaaaaaaa..aa.bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
}

#[test]
fn typing_takes_the_values_place_and_keeps_its_fine_digits_where_they_are() {
    let (mut app, _realtime) = on_sound();
    put(&mut app, CUT);
    press(&mut app, "i4");
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       4_00 ▕█████████▏   ",
            "aaaaaaaaaabbaa.cdddddddddc...",
            "a fg OVERLAY2",
            "b fg BASE bg GREEN",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
}

#[test]
fn a_jump_labels_the_names_it_finds_and_their_values() {
    let (mut app, _realtime) = on_sound();
    press(&mut app, "/cu");
    assert_drawn(
        &row(&app, 13, 1, 90),
        &[
            "  Cuf       fF00 ▕█████████▏   Punch     0000 ▕         ▏   Low cuj   j000 ▕         ▏    ",
            "..aab.......b.aa.cdddddddddc...aaaaaaaaaa..aa.cdddddddddc...aaaaaab...b.aa.cdddddddddc....",
            "a fg OVERLAY2",
            "b fg BASE bg PEACH +BOLD",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
}

#[test]
fn the_bar_follows_whats_playing_and_marks_whats_set() {
    let (mut app, mut realtime) = on_sound();
    catch_up(&mut app, &mut realtime);
    app.services.reported.live = Some(LiveValues {
        cutoff: 0x40,
        resonance: 0x30,
        ..live(&app)
    });
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       FF00 ▕██▎     │▏   ",
            "aaaaaaaaaa..aa.bccccccccbb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
    assert_drawn(
        &cell(&app, "Res", 29),
        &[
            "Res       0000 ▕│▋       ▏   ",
            "aaaaaaaaaa..aa.bcddddddddb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg BASE bg MAUVE",
            "d fg MAUVE",
        ],
    );
}

#[test]
fn a_fader_the_steps_drive_is_in_the_playing_colour_and_one_taken_over_is_marked() {
    let (mut app, mut realtime) = on_sound();
    catch_up(&mut app, &mut realtime);
    app.services.reported.live = Some(LiveValues {
        level: 0x60,
        ..live(&app)
    });
    // A fader's bar is filled to the fader, with a thin mark at what's heard.
    assert_drawn(
        &cell(&app, "Level", 29),
        &[
            "Level     E0   ▕███│███▉ ▏   ",
            "aaaaaaaaaa.....bcccdcccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
            "d fg BASE bg MAUVE",
        ],
    );
    app.services.reported.faders[0].automated[0] = true;
    assert_drawn(
        &cell(&app, "Level", 29),
        &[
            "Level     E0   ▕███│███▉ ▏   ",
            "aaaaaaaaaabb...cbbbdbbbbbc...",
            "a fg OVERLAY2",
            "b fg MAUVE",
            "c fg OVERLAY0",
            "d fg BASE bg MAUVE",
        ],
    );
    app.services.reported.faders[0].taken[0] = true;
    assert_drawn(
        &cell(&app, "Level", 29),
        &[
            "Level \u{f0741}   E0   ▕███│███▉ ▏   ",
            "aaaaaabaaa.....cdddedddddc...",
            "a fg OVERLAY2",
            "b fg PEACH",
            "c fg OVERLAY0",
            "d fg MAUVE",
            "e fg BASE bg MAUVE",
        ],
    );
}

#[test]
fn a_macro_holding_a_setting_draws_its_range_behind_the_fill() {
    let mut song = Song::new(2);
    song.tracks[0].macros[0].maps.push(MacroMap {
        target: MacroTarget::Setting(SettingId::CUT),
        min: 0x2000,
        max: 0xC000,
        curve: 0,
        s: 0,
        zone: Default::default(),
        member: None,
    });
    let (mut app, _realtime) = app_with(song);
    press(&mut app, " 5");
    assert_drawn(
        &cell(&app, "Cut", 29),
        &[
            "Cut       2000 ▕█▏░░░░░  ▏   ",
            "aaaaaaaaaa..aa.bccbbbbbccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
}

#[test]
fn a_macro_holding_a_setting_with_no_bar_marks_it_after_its_value() {
    let mut song = Song::new(2);
    for id in [SettingId::SPEED, SettingId::FILTER_MODE] {
        song.tracks[0].macros[0]
            .maps
            .push(MacroMap::whole(MacroTarget::Setting(id)));
    }
    let (mut app, _realtime) = app_with(song);
    press(&mut app, " 5");
    // The macro at 00 takes it to its first choice.
    assert_drawn(
        &cell(&app, "Speed", 29),
        &[
            "Speed     1/32 •             ",
            "aaaaaaaaaa.....b.............",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
        ],
    );
    // The mark stays faint after the cursor's colour.
    let speed = named(&app, "Speed");
    put(&mut app, speed);
    assert_drawn(
        &cell(&app, "Speed", 29),
        &[
            "Speed     1/32 •             ",
            "aaaaaaaaaabbbb.c.............",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
        ],
    );
    // A title's control has it too.
    assert_drawn(
        &row(&app, 11, 0, 26),
        &[
            "╭─ Filter  LP  12 dB • ───",
            "aabccccccbbbbbbbbbbbbdbaaa",
            "a fg SURFACE1",
            "b fg TEXT",
            "c fg OVERLAY2",
            "d fg OVERLAY0",
        ],
    );
}

#[test]
fn a_setting_left_alone_or_doing_nothing_here_is_dim() {
    let (app, _realtime) = on_sound();
    assert_drawn(
        &cell(&app, "Speed", 29),
        &[
            "Speed     1/16               ",
            "aaaaaaaaaaaaaa...............",
            "a fg OVERLAY2",
        ],
    );
    assert_drawn(
        &cell(&app, "Glide", 29),
        &[
            "Glide     Off                ",
            "aaaaaaaaaaaaa................",
            "a fg OVERLAY2",
        ],
    );
    assert_drawn(
        &cell(&app, "Punch", 29),
        &[
            "Punch     0000 ▕         ▏   ",
            "aaaaaaaaaa..aa.bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
    assert_drawn(
        &cell(&app, "Role", 29),
        &[
            "Role      None               ",
            "aaaaaaaaaaaaaa...............",
            "a fg OVERLAY2",
        ],
    );
}

#[test]
fn words_choices_and_toggles_take_the_bars_room() {
    let (mut app, _realtime) = on_sound();
    assert_drawn(
        &cell(&app, "Slice", 29),
        &[
            "Slice     00   whole         ",
            "aaaaaaaaaa.....bbbbb.........",
            "a fg OVERLAY2",
            "b fg MAUVE",
        ],
    );
    assert_drawn(
        &cell(&app, "Play", 29),
        &[
            "Play      One-shot           ",
            "aaaaaaaaaa...................",
            "a fg OVERLAY2",
        ],
    );
    assert_drawn(
        &cell(&app, "Safe", 29),
        &[
            "Safe      \u{f204}                  ",
            "aaaaaaaaaaaaa................",
            "a fg OVERLAY2",
        ],
    );
    put(&mut app, Control::Safe);
    assert_drawn(
        &cell(&app, "Safe", 29),
        &[
            "Safe      \u{f204}                  ",
            "aaaaaaaaaabbb................",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
        ],
    );
    press(&mut app, "=");
    put(&mut app, CUT);
    assert_drawn(
        &cell(&app, "Safe", 29),
        &[
            "Safe      \u{f205}                  ",
            "aaaaaaaaaaaaa................",
            "a fg OVERLAY2",
        ],
    );
}

#[test]
fn a_role_is_in_its_colour_set_back_while_its_auto_and_a_group_in_its_own() {
    let mut song = Song::new(2);
    song.tracks[0].group = Some(1);
    song.tracks[0].role = Role::Drums;
    song.tracks[1].machine = Machine::Plaits(SynthVoice::Kick);
    let (mut app, _realtime) = app_with(song);
    press(&mut app, " 5");
    assert_drawn(
        &cell(&app, "Role", 29),
        &[
            "Role      Drums              ",
            "aaaaaaaaaabbbbb..............",
            "a fg OVERLAY2",
            "b fg MAROON",
        ],
    );
    assert_drawn(
        &cell(&app, "Group", 29),
        &[
            "Group     G1                 ",
            "aaaaaaaaaabb.................",
            "a fg OVERLAY2",
            "b fg BLUE",
        ],
    );
    app.services.cursor.track = 1;
    assert_drawn(
        &cell(&app, "Role", 29),
        &[
            "Role      Drums              ",
            "aaaaaaaaaabbbbb..............",
            "a fg OVERLAY2",
            "b fg #83616c",
        ],
    );
    assert_drawn(
        &cell(&app, "Group", 29),
        &[
            "Group     Off                ",
            "aaaaaaaaaa...................",
            "a fg OVERLAY2",
        ],
    );
}

#[test]
fn a_devices_title_has_its_controls_bold_while_the_cursor_is_in_it() {
    let (mut app, _realtime) = on_sound();
    put(&mut app, Control::FilterType);
    assert_drawn(
        &row(&app, 11, 0, 24),
        &[
            "╭─ Filter  LP  12 dB ───",
            "aabaaaaaabbccbbdddddbaaa",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg BASE bg ROSEWATER +BOLD",
            "d fg TEXT +BOLD",
        ],
    );
    put(&mut app, Control::Level);
    assert_drawn(
        &row(&app, 11, 0, 24),
        &[
            "╭─ Filter  LP  12 dB ───",
            "aabccccccbbbbbbbbbbbbaaa",
            "a fg SURFACE1",
            "b fg TEXT",
            "c fg OVERLAY2",
        ],
    );
}

#[test]
fn a_returns_sends_in_and_a_groups_tracks_are_rows_the_cursor_doesnt_stop_in() {
    let mut song = Song::new(2);
    song.tracks[0].group = Some(1);
    song.tracks[0].send_a = 0x0C;
    song.tracks[1].send_a = 0xC0;
    let (mut app, _realtime) = app_with(song);
    press(&mut app, " 5");
    let ret = app.song().fixed_track(TrackKind::ReturnA).unwrap();
    app.services.cursor.track = ret;
    assert_drawn(
        &row(&app, 11, 0, 70),
        &[
            "╭─ Sends in ──────────────────────────────────────────────────────────",
            "aabccccccccbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "a fg SURFACE1",
            "b fg TEXT",
            "c fg OVERLAY2",
        ],
    );
    assert_drawn(
        &row(&app, 13, 1, 70),
        &[
            "  01        0C00 ▕▍        ▏                                          ",
            "..aa..........bb.cdddddddddc..........................................",
            "a fg BLUE",
            "b fg OVERLAY2",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    app.services.cursor.track = group;
    assert_drawn(
        &row(&app, 13, 1, 70),
        &[
            "  01                                                                  ",
            "..aa..................................................................",
            "a fg BLUE",
        ],
    );
}

#[test]
fn a_mod_slots_settings_are_words_and_its_reset_a_toggle() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5L");
    put(&mut app, Control::Mod(0, ModField::Attack));
    assert_drawn(
        &row(&app, 4, 1, 72),
        &[
            "  Attack    1 ms       Sustain   100%                                   ",
            "..aaaaaaaaaabbbb.......aaaaaaaaaa.......................................",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
        ],
    );
    assert_drawn(
        &row(&app, 8, 1, 72),
        &[
            "  Release   50 ms      Reset     \u{f205}                                      ",
            "..aaaaaaaaaa...........aaaaaaaaaabbb....................................",
            "a fg OVERLAY2",
            "b fg GREEN",
        ],
    );
    // Its title, with the cursor in the slot, and the next slot's, without.
    assert_drawn(
        &row(&app, 2, 0, 40),
        &[
            "╭─ Mod 1  \u{f205}   ADSR  Exp  --  → Vol ─────",
            "aabaaaaabbcccbddddbbdddbbddbbdddddbaaaaa",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg GREEN",
            "d fg TEXT +BOLD",
        ],
    );
    assert_drawn(
        &row(&app, 11, 0, 40),
        &[
            "╭─ Mod 2  \u{f205}   ADSR  Exp  Uni  → Cut ────",
            "aabcccccbbdddbbbbbbbbbbbbbbbbbbbbbbbaaaa",
            "a fg SURFACE1",
            "b fg TEXT",
            "c fg OVERLAY2",
            "d fg GREEN",
        ],
    );
}

/// The FX page with a compressor in the first slot, the cursor on it.
fn on_fx() -> (App, Realtime) {
    let (mut app, realtime) = app();
    press(&mut app, " 5LLKComp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.track_page.page, Page::Fx);
    assert_eq!(app.track_page.control_here(&app.ctx()), Control::Insert(0));
    (app, realtime)
}

#[test]
fn a_slots_title_and_settings_with_the_cursor_on_its_effect_then_a_setting() {
    let (mut app, _realtime) = on_fx();
    assert_drawn(
        &row(&app, 2, 0, 40),
        &[
            "╭─ 1  \u{f205}   Comp  Clean ──────────────────",
            "aababbcccbddddbbeeeeebaaaaaaaaaaaaaaaaaa",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg GREEN",
            "d fg BASE bg ROSEWATER +BOLD",
            "e fg TEXT +BOLD",
        ],
    );
    assert_drawn(
        &row(&app, 4, 1, 110),
        &[
            "  Threshold C0 ▕██████▊  ▏   Attack    55 ▕███      ▏   Makeup    00 ▕         ▏   Mix       FF ▕█████████▏   ",
            "..aaaaaaaaaa...bcccccccccb...aaaaaaaaaa...bcccccccccb...aaaaaaaaaa...bcccccccccb...aaaaaaaaaa...bcccccccccb...",
            "a fg OVERLAY2",
            "b fg OVERLAY0",
            "c fg MAUVE",
        ],
    );
    let threshold = named(&app, "Threshold");
    put(&mut app, threshold);
    assert_drawn(
        &row(&app, 2, 0, 40),
        &[
            "╭─ 1  \u{f205}   Comp  Clean ──────────────────",
            "aababbcccbddddbbdddddbaaaaaaaaaaaaaaaaaa",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg GREEN",
            "d fg TEXT +BOLD",
        ],
    );
    assert_drawn(
        &row(&app, 4, 1, 110),
        &[
            "  Threshold C0 ▕██████▊  ▏   Attack    55 ▕███      ▏   Makeup    00 ▕         ▏   Mix       FF ▕█████████▏   ",
            "..aaaaaaaaaabb.cdddddddddc...aaaaaaaaaa...cdddddddddc...aaaaaaaaaa...cdddddddddc...aaaaaaaaaa...cdddddddddc...",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
            "d fg MAUVE",
        ],
    );
}

#[test]
fn a_slots_bar_follows_whats_playing_and_a_macros_range() {
    let (mut app, mut realtime) = on_fx();
    let (threshold, attack) = (named(&app, "Threshold"), named(&app, "Attack"));
    put(&mut app, attack);
    press(&mut app, "M1");
    assert!(
        app.services.status.contains("macro 1"),
        "{}",
        app.services.status
    );
    put(&mut app, threshold);
    catch_up(&mut app, &mut realtime);
    let mut live = live(&app);
    for (control, heard) in [(threshold, 0x10), (attack, 0xF0)] {
        let Control::InsertParam(slot, param) = control else {
            unreachable!()
        };
        live.inserts[slot][param] = heard;
    }
    app.services.reported.live = Some(live);
    assert_drawn(
        &row(&app, 4, 1, 110),
        &[
            "  Threshold C0 ▕▌     │  ▏   Attack    55 ▕██│█████▍▏   Makeup    00 ▕         ▏   Mix       FF ▕█████████▏   ",
            "..aaaaaaaaaabb.cddddddcddc...aaaaaaaaaa...cddeddddddc...aaaaaaaaaa...cdddddddddc...aaaaaaaaaa...cdddddddddc...",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
            "d fg MAUVE",
            "e fg BASE bg MAUVE",
        ],
    );
}

#[test]
fn a_slot_a_steps_lock_switches_out_fades_but_for_the_cursor() {
    let (mut app, mut realtime) = on_fx();
    let attack = named(&app, "Attack");
    put(&mut app, attack);
    catch_up(&mut app, &mut realtime);
    let mut live = live(&app);
    live.bypassed[0] = true;
    app.services.reported.live = Some(live);
    app.services.reported.transport = Transport::Playing;
    assert_drawn(
        &row(&app, 2, 0, 40),
        &[
            "╭─ 1  \u{f204}   Comp  Clean ──────────────────",
            "aababbcccbddddbbdddddbaaaaaaaaaaaaaaaaaa",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg OVERLAY0",
            "d fg OVERLAY0 +BOLD",
        ],
    );
    assert_drawn(
        &row(&app, 4, 1, 110),
        &[
            "  Threshold C0 ▕██████▊  ▏   Attack    55 ▕███      ▏   Makeup    00 ▕         ▏   Mix       FF ▕█████████▏   ",
            "..aaaaaaaaa.aa.aaaaaaaaaaa...aaaaaa....bb.aaaaaaaaaaa...aaaaaa....aa.aaaaaaaaaaa...aaa.......aa.aaaaaaaaaaa...",
            "a fg OVERLAY0",
            "b fg BASE bg ROSEWATER",
        ],
    );
}

#[test]
fn the_macros_band_shows_where_each_is_heard() {
    let (mut app, mut realtime) = app();
    press(&mut app, " 5LLL");
    assert_eq!(app.track_page.page, Page::Macros);
    catch_up(&mut app, &mut realtime);
    let mut live = live(&app);
    live.macros[1] = 0x8000;
    app.services.reported.live = Some(live);
    assert_drawn(
        &row(&app, 4, 1, 70),
        &[
            "  Macro 1      0000 ▕         ▏   Macro 2      8000 ▕│███▌    ▏   Macr",
            "..aaaaaaaaaaaaabbbb.cdddddddddc...aaaaaaaaaaaaaaaaa.ceddddddddc...aaaa",
            "a fg OVERLAY2",
            "b fg BASE bg ROSEWATER",
            "c fg OVERLAY0",
            "d fg MAUVE",
            "e fg BASE bg MAUVE",
        ],
    );
}

#[test]
fn a_generators_settings() {
    let mut song = Song::new(1);
    let empty: Step = "--- -- --- --- ---".parse().unwrap();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![empty; 8])));
    let (mut app, _realtime) = app_with(song);
    press(&mut app, "vGSjjjjjj===L");
    press(&mut app, " 8j");
    assert!(matches!(
        app.gen_page.control_here(&app.ctx()),
        Control::Gen { .. }
    ));
    assert_drawn(
        &cut(&page(&app), Rect::new(0, 2, 110, 8)),
        &[
            "╭─ Ramp  Euclid · Note 00–07 ─────────────────────────────────────────────────────────────────────────────────",
            "aabccccbbccccccbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "│                                                                                                             ",
            "a.............................................................................................................",
            "│  From      C-3  ▕███▋     ▏   Curve     0    ▕    │    ▏   Over      All rows                               ",
            "a..aaaaaaaaaaddd..efffffffffe...aaaaaaaaaa.....efffffffffe...aaaaaaaaaaaaaaaaaa...............................",
            "│                                                                                                             ",
            "a.............................................................................................................",
            "│  To        C-4  ▕████▋    ▏   S         0    ▕    │    ▏   Hits      3    ▕███▍     ▏                       ",
            "a..aaaaaaaaaa.....efffffffffe...aaaaaaaaaa.....efffffffffe...aaaaaaaaaa.....efffffffffe.......................",
            "│                                                                                                             ",
            "a.............................................................................................................",
            "│  Run       1    ▕         ▏                                Rotate    0    ▕         ▏                       ",
            "a..aaaaaaaaaa.....efffffffffe................................aaaaaaaaaa.....efffffffffe.......................",
            "│                                                                                                             ",
            "a.............................................................................................................",
            "a fg OVERLAY2",
            "b fg TEXT",
            "c fg TEXT +BOLD",
            "d fg BASE bg ROSEWATER",
            "e fg OVERLAY0",
            "f fg MAUVE",
        ],
    );
}

#[test]
fn the_filter_curve_is_drawn_from_what_plays_a_lock_included() {
    use vt_engine::track_state::LiveFilter;
    use vt_model::song::{FilterMode, FilterType};

    use crate::graphics::{PanelKey, Shape};
    let (mut app, mut realtime) = on_sound();
    catch_up(&mut app, &mut realtime);
    let drawn = |app: &App| {
        app.track_page
            .pictures(&app.ctx())
            .into_iter()
            .find_map(|want| match (want.key, want.shape) {
                (PanelKey::Filter, Shape::Filter(settings)) => Some(settings),
                _ => None,
            })
            .unwrap()
    };
    let track = app.song().tracks[0].clone();
    assert_eq!(drawn(&app).0, track.filter, "as set while nothing moves it");
    // A step's lock of the type, its drive, Safe and cuts, as the engine says it hears them.
    let filter = LiveFilter {
        kind: FilterType::Ladder,
        mode: FilterMode::LowPass12,
        drive: 0x80,
        punch: 0,
        safe: true,
        low_cut: 0x30,
        high_cut: 0xC0,
    };
    app.services.reported.live = Some(LiveValues {
        cutoff: 0x40,
        resonance: 0x50,
        filter,
        ..live(&app)
    });
    assert_eq!(
        drawn(&app),
        (
            FilterType::Ladder,
            FilterMode::LowPass12,
            0x40,
            0x50,
            0x80,
            true,
            0x30,
            0xC0
        )
    );
}
