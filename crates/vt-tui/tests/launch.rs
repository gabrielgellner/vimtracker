//! The launch grid: realigning tracks and arming mutes with `Space a` and `Space g m` while
//! playing.

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_model::constants::MAX_GROUPS;

mod common;
use common::*;

/// The status line's transport while playing with a realign waiting.
fn realign_shown() -> String {
    use vt_tui::glyphs::{PLAY, WAITING};
    format!(" {PLAY} {WAITING} REALIGN  ")
}

#[test]
fn space_a_realigns_the_cursors_track_on_the_launch_grid_while_playing() {
    let (mut app, mut rt) = app();
    tap(&mut app, " a");
    assert_eq!(app.status, "Realign works while playing");

    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " a");
    assert_eq!(
        app.status,
        "Track 01 realigns on the 1bar grid; Space a again takes it back"
    );
    block(&mut rt, &mut app);
    assert!(rt.engine().track_realign_waiting(0));
    assert!(!rt.engine().realign_waiting(), "only this track");
    assert!(app.realigning_tracks[0]);
    assert!(status_line(&app).contains(&realign_shown()));
    assert!(!app.dirty, "it moves playback, not the song");

    // Again, it's taken back.
    tap(&mut app, " a");
    assert_eq!(app.status, "Track 01's realign taken back");
    block(&mut rt, &mut app);
    assert!(!rt.engine().track_realign_waiting(0));
    assert!(!status_line(&app).contains("REALIGN"));

    // Asked for again, it lands on the bar and is done.
    tap(&mut app, " a");
    block(&mut rt, &mut app);
    assert!(app.realigning_tracks[0]);
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(!app.realigning_tracks[0], "landed on the bar");
    press(&mut app, "u");
    assert!(!app.dirty);
}

#[test]
fn space_g_m_mutes_the_cursors_track_on_the_launch_grid_while_playing() {
    use vt_tui::glyphs::{PLAY, WAITING};
    let (mut app, mut rt) = app();
    // Stopped, there's no grid to wait for, so it switches at once.
    tap(&mut app, " gm");
    assert_eq!(app.status, "Track 01 muted at once, as nothing's playing");
    assert!(app.levels[0].mute);
    tap(&mut app, " gm");
    assert!(!app.levels[0].mute);

    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " gm");
    assert_eq!(
        app.status,
        "Track 01 mutes on the 1bar grid; Space g m again takes it back"
    );
    block(&mut rt, &mut app);
    assert!(rt.engine().mute_waiting(0));
    assert!(app.jam[0].mute_waiting);
    assert!(!app.levels[0].mute, "not until the line");
    assert!(status_line(&app).contains(&format!(" {PLAY} {WAITING} MUTE  ")));
    press(&mut app, " 6");
    assert!(
        screen_rows(&mut app)
            .iter()
            .any(|row| row.trim_start().starts_with("Mute") && row.contains(WAITING)),
        "the mixer's Mute row waits"
    );

    // Again, it's taken back.
    tap(&mut app, " gm");
    assert_eq!(app.status, "Track 01's mute switch taken back");
    block(&mut rt, &mut app);
    assert!(!rt.engine().mute_waiting(0));
    assert!(!status_line(&app).contains("MUTE"));

    // Armed again, it lands on the bar, and the live mute follows it, saved with the song.
    app.dirty = false;
    tap(&mut app, " gm");
    block(&mut rt, &mut app);
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(!app.jam[0].mute_waiting, "landed on the bar");
    assert!(app.levels[0].mute);
    assert!(app.dirty);

    // `Space g s` solos the same way, and unmuting by hand meanwhile leaves it waiting.
    tap(&mut app, " gs");
    assert_eq!(
        app.status,
        "Track 01 solos on the 1bar grid; Space g s again takes it back"
    );
    tap(&mut app, " m");
    block(&mut rt, &mut app);
    assert!(app.jam[0].solo_waiting);
    assert!(status_line(&app).contains(&format!(" {PLAY} {WAITING} SOLO  ")));
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(app.levels[0].solo);
    assert!(!app.levels[0].mute);
}

#[test]
fn mutes_armed_on_two_tracks_land_together_and_space_capital_m_takes_them_back() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " gm");
    app.cursor.track = 1;
    tap(&mut app, " gm");
    block(&mut rt, &mut app);
    assert!(app.jam[0].mute_waiting && app.jam[1].mute_waiting);
    tap(&mut app, " M");
    block(&mut rt, &mut app);
    assert!(!rt.engine().mute_waiting(0) && !rt.engine().mute_waiting(1));

    tap(&mut app, " gm");
    app.cursor.track = 0;
    tap(&mut app, " gm");
    let mut landed = None;
    for blocks in 0..400 {
        block(&mut rt, &mut app);
        let both = (app.levels[0].mute, app.levels[1].mute);
        if both != (false, false) {
            landed = Some((blocks, both));
            break;
        }
    }
    let (_, both) = landed.expect("they land on the bar");
    assert_eq!(both, (true, true), "in the same block");
}

#[test]
fn space_a_on_a_return_says_it_has_no_chain() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    let returns = app.song().sound_tracks() + MAX_GROUPS;
    app.cursor.track = returns;
    tap(&mut app, " a");
    assert_eq!(app.status, "Return A plays no chain to realign");
}

#[test]
fn space_a_works_in_every_view_and_space_k_lists_it_beside_space_capital_a() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    for view in ["1", "2", "3", "4", "6", "7", "9", "0"] {
        press(&mut app, &format!(" {view}"));
        tap(&mut app, " a");
        assert!(
            app.status.starts_with("Track 01 realigns"),
            "{view}: {}",
            app.status
        );
        block(&mut rt, &mut app);
        tap(&mut app, " a");
        assert_eq!(app.status, "Track 01's realign taken back", "{view}");
        block(&mut rt, &mut app);
    }
    let rows = app.help_rows();
    let listed = rows
        .iter()
        .filter(|row| row.trim_start().starts_with("Space a "))
        .count();
    assert_eq!(listed, 8, "{rows:?}");
}

#[test]
fn space_a_realigns_every_track_on_the_launch_grid_while_playing() {
    let (mut app, mut rt) = app();
    tap(&mut app, " A");
    assert_eq!(app.status, "Realign works while playing");

    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " A");
    assert_eq!(
        app.status,
        "Every track realigns on the 1bar grid; Space A again takes it back"
    );
    block(&mut rt, &mut app);
    assert!(rt.engine().realign_waiting());
    assert!(app.realigning);
    assert!(status_line(&app).contains(&realign_shown()));
    assert!(!app.dirty, "it moves playback, not the song");

    // Again, it's taken back.
    tap(&mut app, " A");
    assert_eq!(app.status, "Realign taken back");
    block(&mut rt, &mut app);
    assert!(!rt.engine().realign_waiting());
    assert!(!status_line(&app).contains("REALIGN"));

    // Asked for again, it lands on the bar and is done.
    tap(&mut app, " A");
    block(&mut rt, &mut app);
    assert!(app.realigning);
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(!app.realigning, "landed on the bar");
    assert_eq!(app.transport, Transport::Playing);

    // Nothing to undo.
    press(&mut app, "u");
    assert!(!app.dirty);
}

#[test]
fn a_realign_says_it_waits_for_the_bar_when_launches_wait_for_a_chain() {
    let (mut app, mut rt) = app();
    command(&mut app, "launch chain");
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " A");
    assert_eq!(
        app.status,
        "Every track realigns on the next bar; Space A again takes it back"
    );
}

#[test]
fn space_a_works_in_every_view_and_space_k_lists_it() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    for view in ["1", "2", "3", "4", "6", "7", "9", "0"] {
        press(&mut app, &format!(" {view}"));
        tap(&mut app, " A");
        assert!(
            app.status.starts_with("Every track realigns"),
            "{view}: {}",
            app.status
        );
        // The engine says it's waiting in its next report, which the second press reads.
        block(&mut rt, &mut app);
        tap(&mut app, " A");
        assert_eq!(app.status, "Realign taken back", "{view}");
        block(&mut rt, &mut app);
    }
    let rows = app.help_rows();
    let listed = rows
        .iter()
        .filter(|row| row.trim_start().starts_with("Space A "))
        .count();
    assert_eq!(listed, 8, "{rows:?}");
}
