//! The mouse wheel and the trackpad as a knob on the cursor's setting (#425, #943), through the
//! terminal's mouse reports as crossterm reads them: a tick is `=` or `-` where the cursor is, in
//! the setting's finest step sized by speed alone, up and sideways alike, with the fingers
//! whichever way the system scrolls; a list or a toggle goes by its places, with a detent, and
//! stops at its ends; a gesture is one undo step; and nothing is captured or done until it's
//! turned on. Recorded streams of ticks, as a MacBook's trackpad sends them in kitty, stand in for
//! the fingers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use vt_engine::link::Realtime;
use vt_model::arpeggio::ArpMode;
use vt_model::selection::Field;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};
use vt_tui::input::wheel::{WheelSetting, feel, tick_size};
use vt_tui::pages::audio::AudioRow;
use vt_tui::screen::status::column_name;

use crate::common::*;

const NOTE: &str = "C-3 40 cut:20 --- ---";

const UP: MouseEventKind = MouseEventKind::ScrollUp;
const DOWN: MouseEventKind = MouseEventKind::ScrollDown;
const RIGHT: MouseEventKind = MouseEventKind::ScrollRight;
const LEFT: MouseEventKind = MouseEventKind::ScrollLeft;

/// An app on a phrase of these steps, with the wheel turned on.
fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, realtime) = app_for(song);
    app.wheel.setting.on = true;
    (app, realtime)
}

/// A mouse report of this kind, with Shift or not, at the screen's corner, as where the pointer
/// is doesn't matter.
fn report(kind: MouseEventKind, shift: bool) -> MouseEvent {
    MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: if shift {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        },
    }
}

/// A report at `at`, and the frame after it, which makes what it owes.
fn report_at(app: &mut App, kind: MouseEventKind, shift: bool, at: Instant) {
    app.mouse_event(report(kind, shift), at);
    app.tick_at(at);
}

/// Scroll a tick up or down at `at`, and the frame after it.
fn scroll(app: &mut App, up: bool, at: Instant) {
    report_at(app, if up { UP } else { DOWN }, false, at);
}

/// Ticks 150 ms apart, each slow enough to be the finest step, all in one gesture.
fn slow_ticks(app: &mut App, up: bool, count: usize, from: Instant) -> Instant {
    let mut at = from;
    for _ in 0..count {
        scroll(app, up, at);
        at += Duration::from_millis(150);
    }
    at
}

/// A recorded stream of the terminal's wheel reports: each tick's time from the first, in
/// milliseconds, and which way.
type Stream = Vec<(f64, MouseEventKind)>;

/// A slow, careful turn: `count` ticks at `per_second`, which comes at 4 to 10 a second on a
/// MacBook's trackpad in kitty.
fn careful(kind: MouseEventKind, per_second: f64, count: usize) -> Stream {
    (0..count)
        .map(|at| (at as f64 * 1000.0 / per_second, kind))
        .collect()
}

/// A fast swipe: ticks at the screen's 60 a second for `ms`, every fifth with another at once.
fn swipe(kind: MouseEventKind, ms: f64) -> Stream {
    let frame = 1000.0 / 60.0;
    let mut stream = Stream::new();
    let mut at = 0.0;
    let mut count = 0;
    while at < ms {
        stream.push((at, kind));
        if count % 5 == 3 {
            stream.push((at, kind));
        }
        at += frame;
        count += 1;
    }
    stream
}

/// A fling: a swipe for `ms`, and then its momentum, the ticks macOS goes on sending after the
/// fingers lift, slowing from 60 a second by a factor of e each 300 ms and stopping under 1.5 a
/// second, about a second later, each on a frame of the screen's.
fn fling(kind: MouseEventKind, ms: f64) -> Stream {
    let mut stream = swipe(kind, ms);
    let frame = 1.0 / 60.0;
    let lifted = stream.last().map_or(0.0, |(at, _)| *at) + frame * 1000.0;
    let (mut owed, mut since) = (0.0, 0.0);
    loop {
        let rate = 60.0 * (-since / 0.3_f64).exp();
        if rate < 1.5 {
            break;
        }
        owed += rate * frame;
        while owed >= 1.0 {
            stream.push((lifted + since * 1000.0, kind));
            owed -= 1.0;
        }
        since += frame;
    }
    stream
}

/// A stream with a bounce at its end: `count` ticks the other way, 120 ms apart after its last.
fn bounced(mut stream: Stream, count: usize) -> Stream {
    let (last, kind) = *stream.last().unwrap();
    let back = match kind {
        UP => DOWN,
        DOWN => UP,
        RIGHT => LEFT,
        _ => RIGHT,
    };
    for at in 1..=count {
        stream.push((last + 120.0 * at as f64, back));
    }
    stream
}

/// The same stream on the other axis, the same way: the same swipe of the fingers crosses twice
/// as many columns as rows, so each tick is two, half its gap apart, or at once where it was.
fn sideways(stream: &Stream) -> Stream {
    let turned = |kind| match kind {
        UP => RIGHT,
        DOWN => LEFT,
        RIGHT => UP,
        _ => DOWN,
    };
    let mut out = Stream::new();
    for (at, (ms, kind)) in stream.iter().enumerate() {
        let next = stream.get(at + 1).map_or(*ms, |(next, _)| *next);
        out.push((*ms, turned(*kind)));
        out.push(((ms + next) / 2.0, turned(*kind)));
    }
    out
}

/// Plays a stream from `start`, a frame after each tick, reading `watch` after each, and then
/// lets its gesture end. Returns what was read and when it ended.
fn play_watching<T>(
    app: &mut App,
    start: Instant,
    stream: &Stream,
    watch: impl Fn(&App) -> T,
) -> (Vec<T>, Instant) {
    let mut seen = Vec::new();
    let mut last = start;
    for (ms, kind) in stream {
        last = start + Duration::from_secs_f64(ms / 1000.0);
        app.mouse_event(report(*kind, false), last);
        app.tick_at(last);
        seen.push(watch(app));
    }
    let end = last + Duration::from_secs(2);
    app.tick_at(end);
    (seen, end + Duration::from_secs(1))
}

/// Plays a stream from `start` and lets its gesture end. Returns when it ended.
fn play(app: &mut App, start: Instant, stream: &Stream) -> Instant {
    play_watching(app, start, stream, |_| ()).1
}

/// How many times what was read changed from one tick to the next.
fn changes<T: PartialEq>(seen: &[T]) -> usize {
    seen.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

/// An app on the Mod page's arpeggio, on its `Rate`.
fn on_the_arpeggio() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&[NOTE]);
    // Across to the LFOs and down past them, onto its first row, as its title has nothing.
    press(&mut app, " 5L$l3}");
    assert_eq!(column_name(&app), "Arp rate");
    (app, realtime)
}

fn retrig(app: &App) -> bool {
    app.song().tracks[0].arpeggio.retrig
}

fn arp_mode(app: &App) -> usize {
    let mode = app.song().tracks[0].arpeggio.mode;
    ArpMode::ALL.iter().position(|each| *each == mode).unwrap()
}

/// The FX value's coarse and fine digits as one number, `0x4020` for `cut:4020`.
fn cut(app: &App) -> u16 {
    app.song().tracks[0].phrases[&0].steps()[0].fx[0].value
}

#[test]
fn the_wheel_is_off_until_turned_on_on_the_audio_page_and_kept_for_the_machine() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    app.wheel.setting.on = false;
    let folder = folder();
    let path = folder
        .join("vimtracker")
        .join(vt_tui::input::wheel::SETTING_FILE);
    app.wheel.setting_path = Some(path.clone());
    assert!(!app.wants_mouse(), "nothing is captured as it starts");
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "a report while it's off does nothing");
    // Its row is after the buffer and the input, before the Roto's.
    press(&mut app, " 9Ljjjj");
    assert_eq!(app.audio_page.row_here(), AudioRow::Wheel);
    press(&mut app, "=");
    assert!(app.wants_mouse());
    assert!(
        app.services.status.contains("Shift-drag"),
        "{}",
        app.services.status
    );
    assert_eq!(
        WheelSetting::load_from(&path),
        WheelSetting {
            on: true,
            reversed: false
        }
    );
    press(&mut app, "-");
    assert!(!app.wants_mouse());
    assert_eq!(
        WheelSetting::load_from(&path),
        WheelSetting {
            on: false,
            reversed: false
        }
    );
    press(&mut app, "K");
    assert!(app.wants_mouse(), "K switches it");
    press(&mut app, "x");
    assert!(!app.wants_mouse(), "x puts it back off");
    std::fs::write(&path, "(on: maybe)").unwrap();
    assert_eq!(
        WheelSetting::load_from(&path),
        WheelSetting {
            on: false,
            reversed: false
        },
        "a file that won't read leaves it off"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn up_or_right_raises_down_or_left_lowers_sideways_counts_as_far_and_shift_is_nothing() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    // Each a moment after the last, as a turn the other way waits for the last to pause.
    let mut at = Instant::now();
    let mut next = || {
        at += Duration::from_millis(400);
        at
    };
    scroll(&mut app, true, next());
    assert_eq!(steps(&app), ["C#3 40 cut:20 --- ---"]);
    scroll(&mut app, false, next());
    scroll(&mut app, false, next());
    assert_eq!(steps(&app), ["B-2 40 cut:20 --- ---"]);
    report_at(&mut app, UP, true, next());
    assert_eq!(steps(&app), ["C-3 40 cut:20 --- ---"], "Shift is still =");
    // A column is half a row, so a sideways tick is half the fingers' way of one up: two make
    // a step, in one turn.
    report_at(&mut app, RIGHT, false, next());
    assert_eq!(steps(&app), ["C-3 40 cut:20 --- ---"], "half a semitone");
    let more = at + Duration::from_millis(150);
    report_at(&mut app, RIGHT, false, more);
    assert_eq!(steps(&app), ["C#3 40 cut:20 --- ---"], "right is =");
    let mut at = more;
    let mut next = || {
        at += Duration::from_millis(400);
        at
    };
    report_at(&mut app, LEFT, true, next());
    report_at(&mut app, LEFT, false, at + Duration::from_millis(150));
    assert_eq!(steps(&app), ["C-3 40 cut:20 --- ---"], "left is -");
    // The screen's own cell says what a column is, once it's drawn.
    app.wheel.cell = Some((10, 40));
    assert_eq!(app.wheel.sideways(), 0.25);
    app.wheel.cell = Some((12, 24));
    assert_eq!(app.wheel.sideways(), 0.5);
}

#[test]
fn the_value_follows_the_fingers_under_natural_scrolling_unless_reversed() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    // Natural scrolling sends the fingers going up as the terminal's down, and right as left.
    app.wheel.natural = true;
    // A square cell, so a sideways tick is a whole step.
    app.wheel.cell = Some((20, 20));
    let mut at = Instant::now();
    let mut next = || {
        at += Duration::from_millis(400);
        at
    };
    report_at(&mut app, DOWN, false, next());
    assert_eq!(
        steps(&app),
        ["C#3 40 cut:20 --- ---"],
        "the fingers went up"
    );
    report_at(&mut app, LEFT, false, next());
    assert_eq!(
        steps(&app),
        ["D-3 40 cut:20 --- ---"],
        "the fingers went right"
    );
    report_at(&mut app, UP, false, next());
    assert_eq!(
        steps(&app),
        ["C#3 40 cut:20 --- ---"],
        "the fingers went down"
    );
    // Reversed, on the audio page's row below the wheel's, and kept for the machine.
    let folder = folder();
    let path = folder
        .join("vimtracker")
        .join(vt_tui::input::wheel::SETTING_FILE);
    app.wheel.setting_path = Some(path.clone());
    press(&mut app, " 9Ljjjjj");
    assert_eq!(app.audio_page.row_here(), AudioRow::WheelDirection);
    press(&mut app, "=");
    assert!(app.wheel.setting.reversed);
    assert!(WheelSetting::load_from(&path).reversed);
    // The wheel leaves its own direction alone, as turning it would turn the next tick round.
    report_at(&mut app, UP, false, next());
    assert!(app.wheel.setting.reversed);
    press(&mut app, " 3");
    report_at(&mut app, DOWN, false, next());
    assert_eq!(
        steps(&app),
        ["C-3 40 cut:20 --- ---"],
        "against the fingers"
    );
    press(&mut app, " 9Lx");
    assert!(
        !app.wheel.setting.reversed,
        "x puts it back with the fingers"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn clicks_and_drags_do_nothing() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let depth = app.services.history.undo_depth();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Moved,
    ] {
        app.mouse_event(report(kind, false), Instant::now());
    }
    assert_eq!(steps(&app), [NOTE]);
    assert_eq!(app.services.history.undo_depth(), depth);
    assert_eq!(app.services.cursor.field, Field::Note, "the cursor stays");
}

#[test]
fn a_gesture_is_one_undo_step_ended_by_a_pause_or_a_key() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let depth = app.services.history.undo_depth();
    let start = Instant::now();
    let end = slow_ticks(&mut app, true, 5, start);
    assert_eq!(steps(&app), ["F-3 40 cut:20 --- ---"]);
    // Still turning: a frame within the pause leaves it open.
    app.tick_at(end);
    app.tick_at(end + Duration::from_millis(400));
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo step"
    );
    // A second turn after the pause is a step of its own.
    slow_ticks(&mut app, true, 2, end + Duration::from_secs(1));
    app.tick_at(end + Duration::from_secs(3));
    assert_eq!(app.services.history.undo_depth(), depth + 2);
    assert_eq!(steps(&app), ["G-3 40 cut:20 --- ---"]);
    press(&mut app, "u");
    assert_eq!(steps(&app), ["F-3 40 cut:20 --- ---"]);
    press(&mut app, "u");
    assert_eq!(steps(&app), [NOTE], "the first turn, all at once");
    // A key ends a turn at once, with no pause.
    let later = Instant::now() + Duration::from_secs(10);
    slow_ticks(&mut app, false, 3, later);
    press(&mut app, "j");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    press(&mut app, "ku");
    assert_eq!(steps(&app), [NOTE]);
    // A fling's momentum is part of it, though its last ticks come further apart than the pause.
    let start = Instant::now() + Duration::from_secs(20);
    let stream = fling(UP, 200.0);
    let gaps = stream.windows(2).map(|pair| pair[1].0 - pair[0].0);
    assert!(gaps.fold(0.0, f64::max) > 300.0);
    play(&mut app, start, &stream);
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(steps(&app), [NOTE]);
}

#[test]
fn a_tick_on_an_fx_value_moves_its_fine_digits_and_quick_ticks_more_of_them() {
    let (mut app, _realtime) = app_with(&["--- -- cut:40 --- ---"]);
    press(&mut app, "llllll");
    assert_eq!(app.services.cursor.field, Field::FxValue(0));
    let start = Instant::now();
    let end = slow_ticks(&mut app, true, 2, start);
    assert_eq!(
        steps(&app),
        ["--- -- cut:4020 --- ---"],
        "a slow tick is a 16th of a step, a fine digit that's seen to move"
    );
    assert_eq!(
        app.services.cursor.field,
        Field::FxValue(0),
        "the cursor stays"
    );
    slow_ticks(&mut app, false, 3, end + Duration::from_secs(1));
    assert_eq!(steps(&app), ["--- -- cut:3FF0 --- ---"], "it carries");
    // On the fine digits themselves, a slow tick is their own `=`.
    press(&mut app, "l");
    assert_eq!(app.services.cursor.field, Field::FxFine(0));
    slow_ticks(&mut app, true, 2, end + Duration::from_secs(3));
    assert_eq!(steps(&app), ["--- -- cut:3FF2 --- ---"]);
    // A burst grows with its speed, and comes in whole 16ths at the frame after it.
    let (mut app, _realtime) = app_with(&["--- -- cut:40 --- ---"]);
    press(&mut app, "llllll");
    let burst = Instant::now();
    for _ in 0..6 {
        app.mouse_event(report(UP, false), burst);
    }
    assert_eq!(
        steps(&app),
        ["--- -- cut:40 --- ---"],
        "nothing before the frame"
    );
    app.tick_at(burst);
    // Each tick at once, its speed the ticks so far, less half the last, as a count read as it
    // comes is over by that.
    let moved: f64 = (1..=6)
        .map(|ticks| tick_size(f64::from(ticks) - 0.5, 16, 0x100))
        .sum();
    let moved = (moved / 16.0).floor() as u16 * 16;
    assert_eq!(cut(&app), 0x4000 + moved);
    assert!(moved > 0x200, "more than two steps: {moved:X}");
}

#[test]
fn a_ticks_size_grows_by_the_same_share_for_each_bit_of_speed() {
    // A value's fine digits, a 256th of `=`'s step, and a 16th of it the least.
    assert_eq!(tick_size(0.0, 16, 256), 16.0, "still, the least");
    assert_eq!(tick_size(feel::SLOW, 16, 256), 16.0, "slow, the least");
    assert_eq!(
        tick_size(feel::FAST, 16, 256),
        4096.0,
        "a sweep, 16 of `=`'s"
    );
    assert_eq!(tick_size(feel::FAST * 4.0, 16, 256), 4096.0, "and no more");
    let middle = (feel::SLOW + feel::FAST) / 2.0;
    assert!(
        (tick_size(middle, 16, 256) - 256.0).abs() < 1e-9,
        "halfway, one"
    );
    // Each tenth of the way multiplies it by the same, with no jumps.
    let sizes: Vec<f64> = (0..=10)
        .map(|part| {
            let speed = feel::SLOW + (feel::FAST - feel::SLOW) * f64::from(part) / 10.0;
            tick_size(speed, 16, 256)
        })
        .collect();
    let growths: Vec<f64> = sizes.windows(2).map(|pair| pair[1] / pair[0]).collect();
    let tenth = 256_f64.powf(0.1);
    assert!(
        growths.iter().all(|growth| (growth - tenth).abs() < 1e-9),
        "{growths:?}"
    );
    // A setting with no finer step takes its `=` until the curve passes it, half way, and then
    // the same as one with.
    assert_eq!(tick_size(feel::SLOW, 1, 1), 1.0);
    assert_eq!(tick_size(middle, 1, 1), 1.0);
    let three_quarters = (feel::SLOW + 3.0 * feel::FAST) / 4.0;
    assert!((tick_size(three_quarters, 1, 1) - 4.0).abs() < 1e-9);
    assert_eq!(tick_size(feel::FAST, 1, 1), 16.0);
    // On a fine part itself, its own `=` slowly, and a step of the value at most.
    assert_eq!(tick_size(feel::SLOW, 1, 16), 1.0);
    assert_eq!(tick_size(feel::FAST, 1, 16), 256.0);
}

#[test]
fn a_slow_turn_takes_the_finest_step_and_a_fling_crosses_the_range() {
    let (mut app, _realtime) = app_with(&["--- -- cut:00 --- ---"]);
    press(&mut app, "llllll");
    // A careful turn at 4 and at 10 a second: every tick a 16th of a step.
    let start = Instant::now();
    let (seen, end) = play_watching(&mut app, start, &careful(UP, 4.0, 8), cut);
    let moves: Vec<u16> = seen.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(moves.iter().all(|moved| *moved == 0x10), "{moves:X?}");
    let (seen, end) = play_watching(&mut app, end, &careful(UP, 10.0, 8), cut);
    let moves: Vec<u16> = seen.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(moves.iter().all(|moved| *moved == 0x10), "{moves:X?}");
    assert_eq!(cut(&app), 0x0100, "16 ticks, one step");
    // A long fling, its momentum with it, crosses the range, which `=` alone took 255 of.
    play(&mut app, end, &fling(UP, 400.0));
    assert_eq!(cut(&app), 0xFF00);
    // A shorter one, or a swipe with no momentum, a good part of it.
    for stream in [fling(UP, 200.0), swipe(UP, 300.0)] {
        let swept = swept(&stream);
        assert!((0x4000..=0xC000).contains(&swept), "{swept:X}");
    }
}

/// Where a stream takes a phrase's `cut:00`, from its value's coarse digits.
fn swept(stream: &Stream) -> u16 {
    let (mut app, _realtime) = app_with(&["--- -- cut:00 --- ---"]);
    press(&mut app, "llllll");
    play(&mut app, Instant::now(), stream);
    cut(&app)
}

#[test]
fn sideways_turns_a_value_as_far_as_up_and_down_for_the_same_swipe() {
    for stream in [careful(UP, 6.0, 12), swipe(UP, 300.0), fling(UP, 200.0)] {
        let (up, across) = (swept(&stream), swept(&sideways(&stream)));
        let (low, high) = (f64::from(up) * 0.85, f64::from(up) * 1.15);
        assert!(
            (low..=high).contains(&f64::from(across)),
            "up {up:X}, sideways {across:X}"
        );
    }
}

#[test]
fn a_toggle_goes_the_fingers_way_and_a_fling_or_its_bounce_never_flips_it_back() {
    let (mut app, _realtime) = on_the_arpeggio();
    press(&mut app, "lj");
    assert_eq!(column_name(&app), "Arp retrig");
    assert!(!retrig(&app));
    // Its keys still turn it over either way.
    press(&mut app, "=");
    assert!(retrig(&app));
    press(&mut app, "=");
    assert!(!retrig(&app));
    let mut at = Instant::now();
    for (stream, on) in [
        (bounced(fling(UP, 200.0), 2), true),
        (bounced(fling(UP, 100.0), 1), true),
        (bounced(fling(DOWN, 200.0), 2), false),
        (bounced(fling(RIGHT, 200.0), 2), true),
        (bounced(fling(LEFT, 400.0), 2), false),
        (careful(UP, 6.0, 10), true),
        (bounced(swipe(DOWN, 300.0), 2), false),
    ] {
        let (seen, end) = play_watching(&mut app, at, &stream, retrig);
        assert_eq!(retrig(&app), on, "{seen:?}");
        assert!(changes(&seen) <= 1, "it flipped back: {seen:?}");
        at = end;
    }
}

#[test]
fn a_short_list_moves_a_place_a_detent_and_stops_at_its_ends() {
    let (mut app, _realtime) = on_the_arpeggio();
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Arp mode");
    assert_eq!(arp_mode(&app), 0);
    // A careful turn: a place at its first tick, and then each third.
    let (seen, end) = play_watching(&mut app, Instant::now(), &careful(UP, 5.0, 7), arp_mode);
    assert_eq!(seen, [1, 1, 1, 2, 2, 2, 3]);
    // On past the last, where its keys go round, it stops.
    let end = play(&mut app, end, &careful(UP, 5.0, 10));
    assert_eq!(arp_mode(&app), ArpMode::ALL.len() - 1);
    press(&mut app, "=");
    assert_eq!(arp_mode(&app), 0, "its keys go round");
    let end = play(&mut app, end, &careful(DOWN, 5.0, 4));
    assert_eq!(arp_mode(&app), 0, "and the wheel stops at the first");
    // A flick or a fling moves it one place or two, its momentum and bounce no more, either axis.
    let mut at = end;
    for stream in [
        fling(UP, 100.0),
        bounced(fling(UP, 200.0), 2),
        sideways(&fling(UP, 200.0)),
        bounced(fling(RIGHT, 400.0), 2),
    ] {
        press(&mut app, "x");
        let (seen, end) = play_watching(&mut app, at, &stream, arp_mode);
        assert!((1..=2).contains(&arp_mode(&app)), "{seen:?}");
        assert!(changes(&seen) <= 2, "{seen:?}");
        at = end;
    }
}

#[test]
fn a_long_list_moves_a_bounded_number_of_places_and_a_new_stroke_moves_it_on() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, " 5");
    assert_eq!(column_name(&app), "Machine");
    let place = |app: &App| {
        let track = &app.song().tracks[0];
        vt_tui::controls::view(vt_tui::controls::Control::Machine, track, app.song(), false)
            .unwrap()
            .value
    };
    assert_eq!(place(&app), 0, "the sampler, first");
    press(&mut app, "-");
    assert!(place(&app) > 30, "its keys go round, to the last of dozens");
    press(&mut app, "=");
    // Down from the first, the wheel stays.
    let end = play(&mut app, Instant::now(), &bounced(fling(DOWN, 300.0), 2));
    assert_eq!(place(&app), 0);
    // A fling up, its momentum and bounce: two places.
    let (seen, end) = play_watching(&mut app, end, &bounced(fling(UP, 400.0), 2), place);
    assert_eq!(place(&app), 2, "{seen:?}");
    // A flick, and another before its momentum ends: two places and one more.
    let mut flicks = fling(UP, 100.0);
    let second = flicks.last().unwrap().0 - 400.0;
    flicks.extend(
        fling(UP, 100.0)
            .iter()
            .map(|(ms, kind)| (ms + second, *kind)),
    );
    flicks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (seen, _) = play_watching(&mut app, end, &flicks, place);
    assert_eq!(place(&app), 5, "{seen:?}");
}

#[test]
fn a_bounce_is_let_go_and_a_turn_after_a_pause_or_three_ticks_is_taken() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    let start = Instant::now();
    let at = |ms: u64| start + Duration::from_millis(ms);
    // A slow turn, with no momentum after it.
    for ms in [0, 100, 200] {
        scroll(&mut app, true, at(ms));
    }
    assert_eq!(steps(&app), ["D#3 40 cut:20 --- ---"]);
    // A tick back sooner than the turn's pause is a bounce.
    scroll(&mut app, false, at(250));
    assert_eq!(steps(&app), ["D#3 40 cut:20 --- ---"], "a bounce is let go");
    scroll(&mut app, true, at(300));
    assert_eq!(steps(&app), ["E-3 40 cut:20 --- ---"], "the turn goes on");
    // After a pause, a tick the other way turns it at once.
    let paused = 300 + feel::TURN_PAUSE.as_millis() as u64;
    scroll(&mut app, false, at(paused));
    assert_eq!(steps(&app), ["D#3 40 cut:20 --- ---"]);
    // With no pause, three in a row are a deliberate turn: the third is taken.
    scroll(&mut app, true, at(paused + 10));
    scroll(&mut app, true, at(paused + 20));
    assert_eq!(
        steps(&app),
        ["D#3 40 cut:20 --- ---"],
        "two back are a bounce"
    );
    scroll(&mut app, true, at(paused + 30));
    assert_eq!(steps(&app), ["E-3 40 cut:20 --- ---"], "the third, a turn");
    // After a fling, a tick back is its bounce until the wheel has been still for its momentum.
    let (mut app, _realtime) = app_with(&["--- -- cut:00 --- ---"]);
    press(&mut app, "llllll");
    let stream = fling(UP, 200.0);
    let start = Instant::now();
    let last = start + Duration::from_secs_f64(stream.last().unwrap().0 / 1000.0);
    for (ms, kind) in &stream {
        report_at(
            &mut app,
            *kind,
            false,
            start + Duration::from_secs_f64(ms / 1000.0),
        );
    }
    let flung = cut(&app);
    scroll(&mut app, false, last + Duration::from_millis(250));
    scroll(&mut app, false, last + Duration::from_millis(500));
    assert_eq!(cut(&app), flung, "its bounce is let go");
    scroll(
        &mut app,
        false,
        last + Duration::from_millis(500) + feel::MOMENTUM,
    );
    assert_eq!(cut(&app), flung - 0x10, "still for its momentum, a turn");
}

#[test]
fn a_frame_makes_its_ticks_once_and_a_key_makes_them_before_it_acts() {
    let (mut app, _realtime) = app_with(&["--- -- cut:40 --- ---", NOTE]);
    press(&mut app, "llllll");
    let start = Instant::now();
    let depth = app.services.history.undo_depth();
    for ms in [0, 2, 4] {
        app.mouse_event(report(UP, false), start + Duration::from_millis(ms));
    }
    assert_eq!(
        steps(&app)[0],
        "--- -- cut:40 --- ---",
        "owed until the frame"
    );
    app.tick_at(start + Duration::from_millis(5));
    let made = steps(&app)[0].clone();
    assert_ne!(made, "--- -- cut:40 --- ---");
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one change for the frame"
    );
    // Ticks with no frame yet are made before a key moves the cursor away.
    app.mouse_event(report(UP, false), start + Duration::from_millis(400));
    press(&mut app, "j");
    assert_ne!(steps(&app)[0], made, "made on the first step, before j");
    assert_eq!(steps(&app)[1], NOTE);
}

#[test]
fn a_tick_on_a_sound_page_setting_moves_its_fine_digits() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    // Cut, the Filter's first setting, open at the top.
    press(&mut app, " 5}j");
    assert_eq!(app.modes.view, View::Track);
    let cut = |app: &App| {
        let track = &app.song().tracks[0];
        (track.cutoff, track.fine(SettingId::CUT))
    };
    assert_eq!(cut(&app), (0xFF, 0));
    let end = slow_ticks(&mut app, false, 2, Instant::now());
    assert_eq!(cut(&app), (0xFE, 0xE0));
    assert!(!app.track_page.fine, "the cursor stays on the value");
    // On the fine digits themselves, a slow tick is their own `=`.
    press(&mut app, "l");
    assert!(app.track_page.fine);
    slow_ticks(&mut app, true, 2, end + Duration::from_secs(1));
    assert_eq!(cut(&app), (0xFE, 0xE2));
    assert!(app.track_page.fine);
}

#[test]
fn a_tick_on_the_tempo_moves_its_fine_part() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, " 9ggjjj");
    assert_eq!(
        app.project_page.control(app.song()),
        vt_tui::pages::project::ProjectControl::Bpm
    );
    let end = slow_ticks(&mut app, true, 3, Instant::now());
    assert_eq!(app.song().millibpm, 120_300, "a tenth of a BPM a slow tick");
    assert!(!app.project_page.fine);
    // A fling sweeps it.
    play(&mut app, end, &fling(UP, 200.0));
    assert!(app.song().millibpm > 140_000, "{}", app.song().millibpm);
}

#[test]
fn a_tick_moves_a_locked_selection_and_a_gesture_is_one_undo_with_its_total() {
    let (mut app, _realtime) = app_with(&[NOTE, NOTE, NOTE, NOTE]);
    press(&mut app, "Vjj");
    let depth = app.services.history.undo_depth();
    let end = slow_ticks(&mut app, true, 3, Instant::now());
    let up = "D#3 40 cut:20 --- ---";
    assert_eq!(steps(&app), [up, up, up, NOTE]);
    assert_eq!(app.modes.mode, Mode::VisualStep, "it stays selected");
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
    app.tick_at(end + Duration::from_secs(1));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    // A second turn, then undo takes back each turn whole, the total with it.
    slow_ticks(&mut app, false, 1, end + Duration::from_secs(2));
    app.tick_at(end + Duration::from_secs(3));
    assert_eq!(app.adjusted_total().as_deref(), Some("+2 semitones"));
    press(&mut app, "u");
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
    press(&mut app, "u");
    assert_eq!(steps(&app), [NOTE, NOTE, NOTE, NOTE]);
    assert_eq!(app.modes.mode, Mode::VisualStep);
    let redo = vt_tui::input::keys::Key {
        ctrl: true,
        ..vt_tui::input::keys::Key::char('r')
    };
    app.key(redo, Instant::now());
    assert_eq!(
        steps(&app),
        [up, up, up, NOTE],
        "redo brings the turn back whole"
    );
    assert_eq!(app.adjusted_total().as_deref(), Some("+3 semitones"));
}

#[test]
fn the_wheel_waits_for_keys_typed_before_it_and_types_nothing() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, "d");
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "an operator waiting for its motion");
    assert_eq!(app.modes.mode, Mode::OperatorPending);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "i");
    assert_eq!(app.modes.mode, Mode::Insert);
    scroll(&mut app, true, Instant::now());
    assert_eq!(steps(&app), [NOTE], "Insert has no = to stand for");
    assert_eq!(app.modes.mode, Mode::Insert);
}

#[test]
fn dot_repeats_the_last_keys_change_not_the_wheels() {
    let (mut app, _realtime) = app_with(&[NOTE]);
    press(&mut app, "=");
    slow_ticks(&mut app, true, 2, Instant::now());
    assert_eq!(steps(&app), ["D#3 40 cut:20 --- ---"]);
    press(&mut app, ".");
    assert_eq!(steps(&app), ["E-3 40 cut:20 --- ---"]);
}
