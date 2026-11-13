//! The sample page, `Space 0`: the cursor's track's sample as a big waveform, with Vim's motions
//! over it at every zoom, its pictures sent only when what they show changes, and its layout on
//! the author's screen and in a smaller window.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::terminal::WindowSize;
use ratatui::style::Color;
use vt_engine::engine::{Engine, PLAYHEADS, Playhead, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::action::Action;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash, Loop, Points};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_model::step::FxCommand;
use vt_tui::app::{App, Field};
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::{Mode, View};
use vt_tui::kitty::Graphics;
use vt_tui::run::draw_screen;
use vt_tui::sample_settings::SampleSetting;
use vt_tui::sample_wave::{END_COLOR, LOOP_COLOR, SLICE_COLOR, START_COLOR};
use vt_tui::views::status::value_shown;

const RATE: u32 = 48_000;
/// Two seconds.
const LENGTH: u64 = 96_000;
/// The sound is a square wave at half of full scale from here to here, with silence either side.
const SOUND: std::ops::Range<u64> = 4800..86_400;
/// How many frames a column the whole sample takes across the 161 columns of the author's screen.
const FIT: u64 = 597;

/// The test sample: a square wave between silences, the right channel at half the left.
fn signal(level: f32) -> Decoded {
    let samples = (0..LENGTH)
        .flat_map(|at| {
            let value = if SOUND.contains(&at) {
                if (at / 100) % 2 == 0 { level } else { -level }
            } else {
                0.0
            };
            [value, value / 2.0]
        })
        .collect();
    Decoded {
        samples,
        channels: 2,
        rate: RATE,
    }
}

/// The points the first sample has: a start, an end, and a loop in the middle.
fn points() -> Points {
    Points {
        start: 1000,
        end: Some(90_000),
        loop_: Some(Loop {
            start: 48_000,
            end: 72_000,
            crossfade: 0,
        }),
        ..Points::default()
    }
}

/// An app whose first two tracks each play a sample of the bank: the first with points, and the
/// second, quieter, as its file is.
fn app() -> (App, Realtime) {
    let mut song = Song::new(2);
    let mut pool = SamplePool::default();
    for (track, (name, hash, level)) in [("one.wav", 0xA1, 0.5), ("two.wav", 0xB2, 0.25)]
        .into_iter()
        .enumerate()
    {
        let decoded = signal(level);
        pool.insert(
            name,
            SampleData {
                frames: decoded.samples.as_chunks::<2>().0.to_vec(),
            },
        );
        pool.insert_analysis(Arc::new(Analysis::of(ContentHash(hash), &decoded)));
        let mut entry = BankEntry::of_file(name, ContentHash(hash));
        if track == 0 {
            entry.edit.points = points();
        }
        let number = song.bank.add(entry).unwrap();
        song.tracks[track].machine = Machine::Sampler;
        song.tracks[track].set_setting(SettingId::SAMPLE, number);
    }
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(RATE, version));
    (App::new(song, None, samples, handle, RATE), realtime)
}

/// Press keys by their names, as the binding table writes them.
fn keys(app: &mut App, names: &str) {
    for key in parse_sequence(names).unwrap() {
        app.key(key, Instant::now());
        app.key_released(key, Instant::now());
    }
}

/// The app on the sample page.
fn on_page() -> (App, Realtime) {
    let (mut app, realtime) = app();
    keys(&mut app, "Space 0");
    assert_eq!(app.view, View::Sample);
    (app, realtime)
}

fn cursor(app: &App) -> u64 {
    app.sample_page.cursor
}

fn view(app: &App) -> (u64, u64) {
    (app.sample_page.zoom, app.sample_page.column)
}

#[test]
fn space_0_opens_the_page_on_the_tracks_sample_whole_with_the_cursor_on_its_start() {
    let (mut app, _realtime) = on_page();
    assert_eq!(view(&app), (FIT, 0));
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "C-o");
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn g_d_on_the_sound_pages_sample_row_opens_it_and_c_o_comes_back() {
    let (mut app, _realtime) = app();
    keys(&mut app, "Space 4 l g d");
    assert_eq!(app.view, View::Sample);
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "C-o");
    assert_eq!(app.view, View::Track);
    // Elsewhere on the Sound page, it says where it works.
    keys(&mut app, "j g d");
    assert_eq!(app.view, View::Track);
    assert!(app.status.contains("Machine's title"), "{}", app.status);
}

#[test]
fn w_and_b_show_the_next_and_previous_tracks_sample_afresh() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "z i l l");
    keys(&mut app, "W");
    assert_eq!(app.cursor.track, 1);
    assert_eq!(app.sample_shown().unwrap().number, 2);
    // Another sample starts whole, with the cursor on its start.
    let _ = app.sample_page_here();
    assert_eq!((view(&app), cursor(&app)), ((FIT, 0), 0));
    keys(&mut app, "B");
    let _ = app.sample_page_here();
    assert_eq!(cursor(&app), 1000);
}

#[test]
fn with_no_pictures_drawn_h_and_l_move_a_column_at_the_zoom_shown_with_counts() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "l");
    assert_eq!(cursor(&app), 1000 + FIT);
    keys(&mut app, "3 l");
    assert_eq!(cursor(&app), 1000 + 4 * FIT);
    keys(&mut app, "h");
    assert_eq!(cursor(&app), 1000 + 3 * FIT);
    // Closer in, a column is fewer frames.
    keys(&mut app, "z i z i");
    assert_eq!(app.sample_page.zoom, 256);
    let here = cursor(&app);
    keys(&mut app, "2 l");
    assert_eq!(cursor(&app), here + 512);
    // And at a frame a column, a frame.
    keys(&mut app, "8 z i");
    assert_eq!(app.sample_page.zoom, 1);
    let here = cursor(&app);
    keys(&mut app, "h");
    assert_eq!(cursor(&app), here - 1);
    // Never past either end.
    keys(&mut app, "g g h");
    assert_eq!(cursor(&app), 0);
}

/// A character's width in pixels, as on the author's screen.
const CELL: u64 = 15;

/// The pixel of the waveform, from the file's start, that the cursor's line is in.
fn pixel(app: &App) -> u64 {
    cursor(app) * CELL / app.sample_page.zoom
}

/// Tap a key at a moment: pressed, and let go straight after.
fn tap(app: &mut App, name: &str, at: Instant) {
    let key = parse_sequence(name).unwrap()[0];
    app.key(key, at);
    app.key_released(key, at);
}

/// The app on the sample page, with its pictures drawn for the author's characters.
fn on_page_in_pixels() -> (App, Realtime) {
    let (mut app, realtime) = on_page();
    app.sample_page.cell_width = Some(CELL as usize);
    (app, realtime)
}

#[test]
fn h_and_l_move_the_line_a_pixel_at_every_zoom() {
    let (mut app, _realtime) = on_page_in_pixels();
    let mut at = Instant::now();
    for zoom in [FIT, 512, 64, 16] {
        while app.sample_page.zoom > zoom {
            keys(&mut app, "z i");
        }
        assert_eq!(app.sample_page.zoom, zoom);
        keys(&mut app, "M");
        // Taps apart, so none is taken for a repeat.
        for key in ["l", "h"] {
            for _ in 0..20 {
                let (was, frame) = (pixel(&app), cursor(&app));
                at += Duration::from_millis(200);
                tap(&mut app, key, at);
                let wanted = if key == "l" { was + 1 } else { was - 1 };
                assert_eq!(pixel(&app), wanted, "{key} at {zoom} frames a column");
                // A pixel's frames, give or take the one the cursor was part way through.
                let moved = cursor(&app).abs_diff(frame);
                assert!(moved <= zoom.div_ceil(CELL), "{moved} at {zoom}");
            }
        }
    }
}

#[test]
fn closer_than_a_frame_a_pixel_h_and_l_move_a_frame() {
    let (mut app, _realtime) = on_page_in_pixels();
    keys(&mut app, "M 7 z i");
    assert_eq!(app.sample_page.zoom, 8);
    let mut at = Instant::now();
    let here = cursor(&app);
    for _ in 0..5 {
        at += Duration::from_millis(200);
        tap(&mut app, "l", at);
    }
    assert_eq!(cursor(&app), here + 5);
    at += Duration::from_millis(200);
    tap(&mut app, "h", at);
    assert_eq!(cursor(&app), here + 4);
    keys(&mut app, "3 z i");
    assert_eq!(app.sample_page.zoom, 1);
    let here = cursor(&app);
    at += Duration::from_millis(200);
    tap(&mut app, "h", at);
    assert_eq!(cursor(&app), here - 1);
}

#[test]
fn a_count_moves_that_many_pixels_or_close_in_frames() {
    let (mut app, _realtime) = on_page_in_pixels();
    let was = pixel(&app);
    keys(&mut app, "3 l");
    assert_eq!(pixel(&app), was + 3);
    keys(&mut app, "1 0 h");
    assert_eq!(pixel(&app), was - 7);
    keys(&mut app, "7 z i");
    assert_eq!(app.sample_page.zoom, 8);
    let here = cursor(&app);
    keys(&mut app, "4 l");
    assert_eq!(cursor(&app), here + 4);
}

/// Hold a key: pressed at a moment, then repeated after a delay, at a rate, as kitty does. Says
/// how many pixels each press and repeat moved the line.
fn hold(app: &mut App, name: &str, at: Instant, repeats: usize, every: Duration) -> Vec<u64> {
    let key = parse_sequence(name).unwrap()[0];
    let mut moved = Vec::new();
    let mut was = pixel(app);
    app.key(key, at);
    moved.push(pixel(app).abs_diff(was));
    // macOS's shortest delay before repeating.
    let mut at = at + Duration::from_millis(225);
    for _ in 0..repeats {
        was = pixel(app);
        app.key_repeated(key, at);
        moved.push(pixel(app).abs_diff(was));
        at += every;
    }
    app.key_released(key, at);
    moved
}

#[test]
fn a_held_l_builds_from_a_pixel_a_repeat_to_a_column() {
    let (mut app, _realtime) = on_page_in_pixels();
    keys(&mut app, "z i z i g g");
    // kitty's own repeat, every 30 ms, for 0.9 s.
    let moved = hold(&mut app, "l", Instant::now(), 30, Duration::from_millis(30));
    assert_eq!(&moved[..3], &[1, 1, 1], "{moved:?}");
    assert!(moved.windows(2).all(|pair| pair[0] <= pair[1]), "{moved:?}");
    // A column a repeat from half a second after the first repeat.
    let full = moved.iter().position(|&pixels| pixels == CELL).unwrap();
    assert!((15..=19).contains(&full), "{full} {moved:?}");
    assert!(moved[full..].iter().all(|&pixels| pixels == CELL));
    // Building smoothly, never by more than a few pixels a repeat.
    assert!(
        moved.windows(2).all(|pair| pair[1] - pair[0] <= 3),
        "{moved:?}"
    );
    // Back the other way it starts again from a pixel.
    let moved = hold(&mut app, "h", Instant::now(), 2, Duration::from_millis(30));
    assert_eq!(moved, [1, 1, 1]);
}

#[test]
fn presses_straight_after_each_other_count_as_a_held_key() {
    // kitty on macOS has sent a held key's repeats as presses, which may come with releases.
    let (mut app, _realtime) = on_page_in_pixels();
    keys(&mut app, "z i z i g g");
    let mut at = Instant::now();
    let mut moved = Vec::new();
    for _ in 0..40 {
        let was = pixel(&app);
        tap(&mut app, "l", at);
        moved.push(pixel(&app) - was);
        at += Duration::from_millis(30);
    }
    assert_eq!(moved[..2], [1, 1]);
    assert_eq!(*moved.last().unwrap(), CELL, "{moved:?}");
}

#[test]
fn another_key_a_gap_or_a_count_starts_a_held_key_again_from_a_pixel() {
    let (mut app, _realtime) = on_page_in_pixels();
    keys(&mut app, "z i z i g g");
    let start = Instant::now();
    let every = Duration::from_millis(30);
    let moved = hold(&mut app, "l", start, 30, every);
    assert_eq!(*moved.last().unwrap(), CELL);
    let l = parse_sequence("l").unwrap()[0];
    let mut at = start + Duration::from_secs(2);
    // Held for a while, and up to speed.
    app.key(l, at);
    for _ in 0..30 {
        at += every;
        app.key_repeated(l, at);
    }
    let was = pixel(&app);
    at += every;
    app.key_repeated(l, at);
    assert_eq!(pixel(&app) - was, CELL);
    // Another key between repeats.
    keys(&mut app, "z z");
    let was = pixel(&app);
    at += every;
    app.key_repeated(l, at);
    assert_eq!(pixel(&app) - was, 1, "after another key");
    // A gap in the repeats.
    for _ in 0..30 {
        at += every;
        app.key_repeated(l, at);
    }
    let was = pixel(&app);
    at += Duration::from_millis(400);
    app.key_repeated(l, at);
    assert_eq!(pixel(&app) - was, 1, "after a gap");
    // A count steps that many pixels, however long the key's held, and the run starts again.
    for _ in 0..30 {
        at += every;
        app.key_repeated(l, at);
    }
    keys(&mut app, "2");
    let was = pixel(&app);
    at += every;
    app.key_repeated(l, at);
    assert_eq!(pixel(&app) - was, 2, "with a count");
    let was = pixel(&app);
    at += every;
    app.key_repeated(l, at);
    assert_eq!(pixel(&app) - was, 1, "after a count");
}

#[test]
fn as_its_column_the_cursor_moves_a_column() {
    let (mut app, _realtime) = on_page_in_pixels();
    keys(&mut app, "z c z i M");
    let zoom = app.sample_page.zoom;
    let here = cursor(&app);
    keys(&mut app, "l");
    assert_eq!(cursor(&app), here + zoom);
    keys(&mut app, "3 h");
    assert_eq!(cursor(&app), here - 2 * zoom);
    let moved = hold(&mut app, "l", Instant::now(), 30, Duration::from_millis(30));
    assert!(moved.iter().all(|&pixels| pixels == CELL), "{moved:?}");
}

#[test]
fn ends_of_the_slice_and_of_the_sample() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "l l $");
    assert_eq!(cursor(&app), 89_999, "the slice's last frame");
    keys(&mut app, "0");
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "G");
    assert_eq!(cursor(&app), LENGTH - 1);
    keys(&mut app, "g g");
    assert_eq!(cursor(&app), 0);
    // Before the start, the slice is still the first, from the start.
    keys(&mut app, "$");
    assert_eq!(cursor(&app), 89_999);
}

#[test]
fn first_and_last_sound_are_where_it_rises_above_silence() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "^");
    assert_eq!(cursor(&app), SOUND.start);
    keys(&mut app, "g _");
    assert_eq!(cursor(&app), SOUND.end - 1);
    // The same at the closest zoom.
    keys(&mut app, "z O 1 0 z i g g ^");
    assert_eq!(app.sample_page.zoom, 1);
    assert_eq!(cursor(&app), SOUND.start);
}

/// Type a command line and run it.
fn command(app: &mut App, line: &str) {
    keys(app, ":");
    for c in line.chars() {
        app.key(Key::new(KeyCode::Char(c)), Instant::now());
    }
    app.key(Key::new(KeyCode::Enter), Instant::now());
}

/// Where the first sample's four slices start, from its start at 1000 to its end at 90 000.
const QUARTERS: [u64; 4] = [1000, 23_250, 45_500, 67_750];

#[test]
fn j_k_and_a_count_g_go_by_real_slices_and_brackets_stop_at_each() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    assert!(app.status.contains("4 slices of 0.464 s"), "{}", app.status);
    let starts: Vec<u64> = app
        .song()
        .bank
        .get(1)
        .unwrap()
        .edit
        .points
        .slices
        .iter()
        .map(|s| s.start)
        .collect();
    assert_eq!(starts, QUARTERS);
    keys(&mut app, "j");
    assert_eq!(cursor(&app), QUARTERS[1]);
    keys(&mut app, "2 j");
    assert_eq!(cursor(&app), QUARTERS[3]);
    keys(&mut app, "k");
    assert_eq!(cursor(&app), QUARTERS[2]);
    keys(&mut app, "l $");
    assert_eq!(cursor(&app), QUARTERS[3] - 1, "the slice's last frame");
    keys(&mut app, "0");
    assert_eq!(cursor(&app), QUARTERS[2]);
    keys(&mut app, "1 G");
    assert_eq!(cursor(&app), QUARTERS[1]);
    keys(&mut app, "9 g g");
    assert_eq!(cursor(&app), QUARTERS[3], "past the last is the last");
    // The slices are points too, between the start, the loop and the end.
    keys(&mut app, "g g ] ] ] ] ] ] ] ]");
    assert_eq!(cursor(&app), 48_000);
    keys(&mut app, "[ [");
    assert_eq!(cursor(&app), QUARTERS[2]);
    // `:slice clear` makes it one again, and `u` brings the slices back.
    command(&mut app, "slice clear");
    assert_eq!(app.sample_shown().unwrap().slice_count(), 1);
    keys(&mut app, "u");
    assert_eq!(app.sample_shown().unwrap().slice_count(), 4);
}

#[test]
fn the_ruler_numbers_each_slice_and_the_settings_row_counts_them() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    let ruler = row(&terminal, 2);
    // At 597 frames a column, slice 01 is in the waveform's column 38, the screen's 39, with its
    // number in the two before it. The terminal places the mark, a pixel either side of its
    // line at 3 pixels a character, so the columns it covers are left for it, and the number is
    // before the first: slice 02's line is its column's first pixel, so it reaches the one before.
    let column = |frame: u64| 1 + (frame / FIT) as usize;
    for (number, start) in QUARTERS.iter().enumerate().skip(1) {
        let line = (start * 3 / FIT) as usize;
        let (first, last) = (1 + (line - 1) / 3, 1 + (line + 1) / 3);
        assert_eq!(first, column(*start) - usize::from(number == 2));
        let shown: String = ruler.chars().skip(first - 2).take(2).collect();
        assert_eq!(shown, format!("{number:02X}"), "{ruler}");
        let left: String = ruler.chars().skip(first).take(last - first + 1).collect();
        assert!(left.trim().is_empty(), "{ruler}");
    }
    // Slice 00 is under the start, with its number after it at the left edge. Its mark reaches
    // into the column after its line's, so the number is after that.
    let first: String = ruler.chars().skip(column(1000)).take(4).collect();
    assert_eq!(first, "  00", "{ruler}");
    // With no pictures, as in a render, the ruler draws the marks itself.
    let mut text = Terminal::new(TestBackend::new(163, 42)).unwrap();
    app.sample_images = None;
    app.sample_page.cell_width = None;
    text.draw(|frame| draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let ruler = row(&text, 2);
    for (number, start) in QUARTERS.iter().enumerate().skip(1) {
        let shown: String = ruler.chars().skip(column(*start) - 2).take(3).collect();
        assert_eq!(shown, format!("{number:02X}▾"), "{ruler}");
    }
    let first: String = ruler.chars().skip(column(1000)).take(3).collect();
    assert_eq!(first, "▾00", "{ruler}");
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    let settings = (0..42)
        .map(|y| row(&terminal, y))
        .find(|line| line.contains("Slices"));
    assert!(settings.unwrap().contains("Slices 4"));
    // The slice's row names the slice the cursor is in, with its settings, its own in the text's
    // colour and the rest as the sample plays.
    keys(&mut app, "j");
    command(&mut app, "slice gain -3");
    command(&mut app, "slice pan -25");
    assert!(
        app.status.contains("slice 01 plays panned 25 L"),
        "{}",
        app.status
    );
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    let slice_row = (0..42)
        .map(|y| row(&terminal, y))
        .find(|line| line.starts_with(" Slice 01"))
        .unwrap();
    assert!(
        slice_row.contains("Gain -3.0 dB   Pan 25 L   Tune +0 st"),
        "{slice_row}"
    );
}

#[test]
fn j_k_and_a_count_g_go_by_slice_which_is_the_whole_entry_with_none() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "G k");
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "g g j");
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "G 5 G");
    assert_eq!(cursor(&app), 1000, "slice 5 is clamped to the last");
    keys(&mut app, "G 1 g g");
    assert_eq!(cursor(&app), 1000);
}

#[test]
fn braces_go_by_beats_from_the_start_at_the_songs_tempo() {
    // 120 BPM at 48 kHz: a beat is 24 000 frames, counted from the start at 1000.
    let (mut app, _realtime) = on_page();
    keys(&mut app, "}");
    assert_eq!(cursor(&app), 25_000);
    keys(&mut app, "2 }");
    assert_eq!(cursor(&app), 73_000);
    keys(&mut app, "{");
    assert_eq!(cursor(&app), 49_000);
    keys(&mut app, "l {");
    assert_eq!(
        cursor(&app),
        49_000,
        "off the grid, back to the beat before"
    );
}

#[test]
fn brackets_go_from_point_to_point() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "g g ] ]");
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "] ]");
    assert_eq!(cursor(&app), 48_000);
    keys(&mut app, "2 ] ]");
    assert_eq!(cursor(&app), 90_000);
    keys(&mut app, "] ]");
    assert_eq!(cursor(&app), 90_000);
    assert!(app.status.contains("No point"), "{}", app.status);
    keys(&mut app, "[ [");
    assert_eq!(cursor(&app), 72_000);
    keys(&mut app, "3 [ [");
    assert_eq!(cursor(&app), 1000);
}

#[test]
fn h_m_and_l_go_to_the_left_middle_and_right_of_the_view() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "L");
    assert_eq!(cursor(&app), 160 * FIT);
    keys(&mut app, "M");
    assert_eq!(cursor(&app), 80 * FIT);
    keys(&mut app, "H");
    assert_eq!(cursor(&app), 0);
    keys(&mut app, "3 H");
    assert_eq!(cursor(&app), 2 * FIT);
    keys(&mut app, "3 L");
    assert_eq!(cursor(&app), 158 * FIT);
}

#[test]
fn zooming_in_and_out_comes_back_to_the_same_view() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "M");
    let whole = view(&app);
    keys(&mut app, "z i");
    // The cursor stays in its column on screen.
    assert_eq!(app.sample_page.zoom, 512);
    assert_eq!(app.sample_page.cursor_on_screen(), Some(80));
    keys(&mut app, "z o");
    assert_eq!(view(&app), whole);
    // Deeper, with counts, and with the other keys.
    keys(&mut app, "3 z i");
    let close = view(&app);
    assert_eq!(close.0, 128);
    keys(&mut app, "2 z i 2 z o");
    assert_eq!(view(&app), close);
    keys(&mut app, "M-1 M-2");
    assert_eq!(view(&app), close);
    keys(&mut app, "3 z o");
    assert_eq!(view(&app), whole);
    keys(&mut app, "6 z i z O");
    assert_eq!(view(&app), (FIT, 0));
}

#[test]
fn zv_fits_the_slice_to_the_view() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "z i z i z i z v");
    let page = &app.sample_page;
    // The slice is 89 000 frames: 553 a column fits it, so the widest zoom does.
    assert_eq!(page.zoom, FIT);
    assert_eq!(page.column, 0);
}

#[test]
fn the_view_scrolls_by_half_a_screen_or_a_column() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "M 3 z i");
    let (zoom, column) = view(&app);
    let here = cursor(&app);
    keys(&mut app, "C-d");
    assert_eq!(view(&app), (zoom, column + 80));
    assert_eq!(cursor(&app), here + 80 * zoom);
    keys(&mut app, "C-u");
    assert_eq!((view(&app), cursor(&app)), ((zoom, column), here));
    keys(&mut app, "C-e");
    assert_eq!(view(&app), (zoom, column + 1));
    assert_eq!(cursor(&app), here, "the cursor stays while it's in view");
    keys(&mut app, "2 C-y");
    assert_eq!(view(&app), (zoom, column - 1));
    // The cursor at the left edge is pushed along by the view.
    keys(&mut app, "z s C-e");
    assert_eq!(cursor(&app), here + zoom);
    keys(&mut app, "z s");
    assert_eq!(app.sample_page.cursor_on_screen(), Some(0));
    keys(&mut app, "z e");
    assert_eq!(app.sample_page.cursor_on_screen(), Some(160));
    keys(&mut app, "z z");
    assert_eq!(app.sample_page.cursor_on_screen(), Some(80));
}

#[test]
fn a_cursor_leaving_the_view_brings_it_half_a_screen_along() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "6 z i z s");
    let (zoom, column) = view(&app);
    keys(&mut app, "1 6 1 l");
    assert_eq!(view(&app), (zoom, column + 161 - 80));
    assert_eq!(app.sample_page.cursor_on_screen(), Some(80));
}

/// Draw the screen at a size, after sending the pictures it needs, and place the cursor's line
/// after it, as the program does each frame, with characters 3 pixels by 6. What's sent is said.
fn draw(
    app: &mut App,
    graphics: &mut Graphics,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, String) {
    draw_in(app, graphics, width, height, (3, 6))
}

/// Draw the screen as [`draw`] does, with characters of this many pixels across and down.
fn draw_in(
    app: &mut App,
    graphics: &mut Graphics,
    width: u16,
    height: u16,
    cell: (u16, u16),
) -> (Terminal<TestBackend>, String) {
    let mut sent = Vec::new();
    graphics.use_window(WindowSize {
        columns: width,
        rows: height,
        width: width * cell.0,
        height: height * cell.1,
    });
    graphics.sync(app, &mut sent).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    graphics.place(app, &mut sent).unwrap();
    (terminal, String::from_utf8_lossy(&sent).into_owned())
}

/// A row of the screen as text, without the placeholders' marks.
fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
        .collect()
}

#[test]
fn the_pictures_are_sent_for_the_zoom_and_view_but_never_for_the_cursor_alone() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches(",c=161,r=28,").count(), 1, "the waveform");
    assert_eq!(sent.matches(",c=161,r=1,").count(), 1, "and the overview");
    // The cursor's line is sent once, to be placed, and placed after the screen is drawn.
    let stored = sent.find("a=t,f=100").expect("the line is stored");
    assert!(sent[stored..].contains("a=p,"), "and placed");
    // Placed at its frame: frame 1000 at 597 a column of 3 pixels is 5 pixels in, so the
    // waveform's second column, two pixels into it, on the screen's fourth row.
    assert!(sent.contains("\x1b[4;3H\x1b_Ga=p,"), "{sent:?}");
    assert!(sent.contains(",X=2,h=168,"), "{sent:?}");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.is_empty(), "nothing changed: {sent:?}");
    for moves in ["l", "3 h", "$", "M", "] ]"] {
        keys(&mut app, moves);
        let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
        assert!(!sent.contains("a=T"), "{moves} sent a picture");
        assert!(!sent.contains("a=t"), "{moves} sent the line again");
        assert!(sent.contains("a=p,"), "{moves} didn't move the line");
    }
    // As a column, the cursor is the cell's background, and the line is taken away.
    keys(&mut app, "z c");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.contains("a=d,d=i"));
    assert!(!sent.contains("a=T"));
    keys(&mut app, "l");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.is_empty(), "{sent:?}");
    // Zooming sends the waveform again, and not the overview, which is of the whole sample.
    keys(&mut app, "z i");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches("a=T").count(), 1);
    // As does scrolling.
    keys(&mut app, "C-e");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches("a=T").count(), 1);
}

/// The pixel, counted from the waveform's left, where the cursor's line was last placed, in the
/// 3-pixel characters `draw` gives.
fn placed(sent: &str) -> Option<i64> {
    let place = sent.rfind("\x1b_Ga=p,")?;
    let jump = sent[..place].rfind("\x1b[")?;
    let column: i64 = sent[jump..place]
        .split(';')
        .nth(1)?
        .trim_end_matches('H')
        .parse()
        .ok()?;
    let offset = sent[place..]
        .split("X=")
        .nth(1)?
        .split(',')
        .next()?
        .parse::<i64>()
        .ok()?;
    // The waveform's first column is the screen's second, which is 2 counted from 1.
    Some((column - 2) * 3 + offset)
}

#[test]
fn each_l_places_the_line_a_pixel_along_wherever_in_its_pixel_the_cursor_is() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(app.sample_page.cell_width, Some(3));
    let mut was = placed(&sent).expect("the line is placed");
    assert_eq!(was, 5, "frame 1000 at 597 a column of 3 pixels");
    let mut at = Instant::now();
    for (key, by) in [("l", 1), ("h", -1)] {
        for _ in 0..12 {
            at += Duration::from_millis(200);
            tap(&mut app, key, at);
            let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
            let now = placed(&sent).expect("the line moved");
            assert_eq!(now, was + by, "{key}");
            was = now;
        }
    }
    // Part way into a pixel, the line is in that pixel.
    keys(&mut app, "^");
    let page = &app.sample_page;
    assert_ne!((page.cursor * 3) % page.zoom, 0, "between pixels");
    let wanted = ((page.cursor - page.first_frame()) * 3 / page.zoom) as i64;
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(placed(&sent), Some(wanted));
}

#[test]
fn the_line_is_taken_away_while_something_is_drawn_over_the_page() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    draw(&mut app, &mut graphics, 163, 42);
    keys(&mut app, "Space Space");
    assert!(app.picker.is_some());
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.contains("a=d,d=i"), "{sent:?}");
    app.key(Key::new(KeyCode::Esc), Instant::now());
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.contains("a=p,"));
    // Leaving the page takes it away too.
    keys(&mut app, "Space 3");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.contains("a=d,d=i"));
}

#[test]
fn a_window_that_changes_size_is_sent_the_line_again() {
    // ratatui clears the screen as the window changes size, and the clear frees the line in
    // kitty and Ghostty, so it's sent again rather than placed by an id the terminal has lost.
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (_, sent) = draw(&mut app, &mut graphics, 170, 42);
    let stored = sent.find("a=t,f=100").expect("the line is sent again");
    assert!(sent[stored..].contains("a=p,"));
    // The waveform is wider, so it's drawn again too, once the page has been laid out at the
    // new size, and the line isn't sent again.
    let (_, sent) = draw(&mut app, &mut graphics, 170, 42);
    assert!(sent.contains(",c=168,r=28,"));
    assert!(!sent.contains("a=t,f=100"));
}

#[test]
fn the_page_on_the_authors_screen() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    assert!(row(&terminal, 0).starts_with(" Sample 01  one"));
    assert!(row(&terminal, 0).contains("2.000 s  48 kHz  stereo  zoom whole sample"));
    // The ruler: the start's column left for its mark, which the terminal places, and the
    // seconds' times.
    let ruler = row(&terminal, 2);
    assert_eq!(ruler.chars().nth(2), Some(' '), "{ruler}");
    assert!(ruler.contains("╵1"), "{ruler}");
    // The waveform: 28 rows of 161 placeholders, then the overview.
    let buffer = terminal.backend().buffer();
    for y in 3..32 {
        assert_eq!(buffer[(0, y)].symbol(), " ");
        assert!(buffer[(1, y)].symbol().starts_with('\u{10EEEE}'));
        assert!(buffer[(161, y)].symbol().starts_with('\u{10EEEE}'));
        assert_eq!(buffer[(162, y)].symbol(), " ");
    }
    assert!(
        row(&terminal, 33).starts_with(" Start 0.021 s   End 1.875 s   Loop 1.000 s to 1.500 s")
    );
    let off = vt_tui::glyphs::SWITCH_OFF;
    assert!(
        row(&terminal, 33).contains(&format!("Snap {off}"))
            && row(&terminal, 33).contains("Sens 50%"),
        "{}",
        row(&terminal, 33)
    );
    assert!(row(&terminal, 34).starts_with(&format!(
        " Whole sample   Gain +0.0 dB   Tune +0 st   Reverse {off}"
    )));
    assert!(
        row(&terminal, 35).starts_with(" Cursor 0.021 s  frame 1000  slice 00  on the start"),
        "{}",
        row(&terminal, 35)
    );
    // The whole width is the page's, with no meter.
    assert!(!row(&terminal, 10).contains("L  R  G"));
}

#[test]
fn a_smaller_window_shows_the_part_of_the_page_with_the_cursor() {
    let (mut app, _realtime) = on_page();
    keys(&mut app, "z c");
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 100, 24);
    let (terminal, _) = draw(&mut app, &mut graphics, 100, 24);
    assert!(row(&terminal, 0).starts_with(" Sample 01  one"));
    // The page isn't squeezed: its waveform keeps its 161 columns.
    assert_eq!(app.sample_page.columns, 161);
    let cursor_shown = |terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (0..100).find(|x| buffer[(*x, 10)].bg == vt_tui::views::sample::cursor_column())
    };
    assert_eq!(cursor_shown(&terminal), Some(2));
    // At the right of the view, the box has scrolled to keep the cursor in it.
    keys(&mut app, "L");
    let (terminal, _) = draw(&mut app, &mut graphics, 100, 24);
    let shown = cursor_shown(&terminal).expect("the cursor is in the window");
    assert!((40..100).contains(&shown), "{shown}");
    assert!(!row(&terminal, 0).starts_with(" Sample 01"));
}

#[test]
fn a_track_with_no_sample_says_so() {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(RATE, version));
    let mut app = App::new(song, None, samples, handle, RATE);
    keys(&mut app, "Space 0 l");
    assert_eq!(app.view, View::Sample);
    assert!(app.status.contains("plays no sample"), "{}", app.status);
    let mut graphics = Graphics::new();
    // The first frame has the scope's picture, until the page is drawn with no room for it.
    draw(&mut app, &mut graphics, 163, 42);
    let (terminal, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.is_empty(), "{sent:?}");
    assert!(row(&terminal, 2).contains("Track 01 plays no sample. Space 5 chooses one"));
}

#[test]
fn every_key_on_the_page_is_listed() {
    let (app, _realtime) = on_page();
    let rows = app.help_rows();
    for keys in [
        "z i", "z o", "z O", "z v", "z c", "g _", "] ]", "[ [", "'", "] '", "[ '",
    ] {
        assert!(
            rows.iter().any(|row| row.trim_start().starts_with(keys)),
            "{keys}"
        );
    }
}

/// Put a step's text on the first track's first phrase, with the cursor on its second FX value.
fn step_with(app: &mut App, step: usize, text: &str) {
    app.apply(&Action::SetStep {
        track: 0,
        phrase: 0,
        step,
        value: text.parse().unwrap(),
    });
    app.cursor.step = step;
    app.cursor.field = Field::FxValue(1);
}

#[test]
fn g_d_on_a_steps_sli_opens_the_page_on_its_slice_and_c_o_comes_back() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    step_with(&mut app, 2, "C-3 -- --- sampler.slice:02 ---");
    let shown = value_shown(&app).unwrap_or_default();
    assert!(shown.contains("02 of 4: from 0.948 s"), "{shown}");
    keys(&mut app, "g d");
    assert_eq!(app.view, View::Sample);
    assert_eq!(cursor(&app), QUARTERS[2]);
    assert_eq!(app.sample_shown().unwrap().number, 1);
    keys(&mut app, "C-o");
    assert_eq!(app.view, View::Phrase);
    // Under an `SMP` of its own, it's that sample's slice, and `Space 0` is the track's again.
    let mut sliced = app.song().bank.get(2).unwrap().clone();
    sliced.edit.points.reslice(&[0, 48_000]);
    app.apply(&Action::SetBankEntry {
        number: 2,
        entry: Some(sliced),
    });
    step_with(&mut app, 3, "C-3 -- sampler.sample:02 sampler.slice:01 ---");
    keys(&mut app, "g d");
    assert_eq!(app.sample_shown().unwrap().number, 2);
    assert_eq!(cursor(&app), 48_000);
    keys(&mut app, "C-o Space 0");
    assert_eq!(app.sample_shown().unwrap().number, 1);
    // Elsewhere in the phrase, it says where it works.
    keys(&mut app, "C-o");
    app.cursor.field = Field::Note;
    keys(&mut app, "g d");
    assert_eq!(app.view, View::Phrase);
    assert!(app.status.contains("SLI"), "{}", app.status);
}

#[test]
fn k_on_sli_lists_the_slices_with_their_starts_and_puts_the_one_chosen() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    step_with(&mut app, 0, "C-3 -- --- sampler.slice:01 ---");
    keys(&mut app, "K");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .expect("the slices are listed")
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(
        labels,
        ["00  0.021 s", "01  0.484 s", "02  0.948 s", "03  1.411 s"]
    );
    keys(&mut app, "j Enter");
    let slot = app.phrase_here().unwrap().steps()[0].fx[1];
    assert_eq!(slot.command, Some(FxCommand::Set(SettingId::SLICE)));
    assert_eq!(slot.coarse(), 2);
    // The Sound page's `Slice` lists them too, for the track's own.
    keys(&mut app, "Space 4 g g j K j j j Enter");
    assert_eq!(app.song().tracks[0].setting(SettingId::SLICE), 3);
}

#[test]
fn a_slices_own_settings_need_slices_and_a_count_has_a_limit() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice gain -3");
    assert!(app.status.contains("has no slices"), "{}", app.status);
    command(&mut app, "slice 1000");
    assert!(app.status.contains(":slice takes"), "{}", app.status);
    command(&mut app, "slice 4");
    // Off the sample page, it's the slice the track's `Slice` is on.
    app.apply(&Action::SetTrackSetting {
        track: 0,
        setting: SettingId::SLICE,
        value: 3,
    });
    command(&mut app, "slice reverse on");
    command(&mut app, "slice tune -12");
    let over = app.song().bank.get(1).unwrap().edit.points.slices[3].over;
    assert_eq!((over.reverse, over.tune), (Some(true), Some(-12)));
    command(&mut app, "slice tune");
    let over = app.song().bank.get(1).unwrap().edit.points.slices[3].over;
    assert_eq!(over.tune, None, "alone, it plays as the sample");
}

#[test]
fn slicing_again_moves_the_steps_locks_with_their_sound_and_one_u_takes_both_back() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    step_with(&mut app, 0, "C-3 -- --- sampler.slice:03 ---");
    let depth = app.history.undo_depth();
    command(&mut app, "slice 8");
    assert!(app.status.contains("SLI locks follow"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    let lock = |app: &App| app.phrase_here().unwrap().steps()[0].fx[1].coarse();
    assert_eq!(lock(&app), 6);
    keys(&mut app, "u");
    assert_eq!(lock(&app), 3);
    assert_eq!(app.sample_shown().unwrap().slice_count(), 4);
}

/// The ruler's marks as the terminal has them, from what's been sent to it.
#[derive(Default)]
struct Marks {
    /// Each mark's image, by its id, and the colour it's drawn in.
    images: BTreeMap<u32, Color>,
    /// Where each mark is, by its image and its placement: its screen row and column, counted
    /// from 1, and how many pixels into the column it starts.
    placed: BTreeMap<(u32, u32), (u16, u16, usize)>,
}

/// A key's value among a command's controls, as `i` in `a=p,i=7`.
fn control<'a>(controls: &'a str, name: &str) -> Option<&'a str> {
    controls
        .split(',')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
}

impl Marks {
    /// Take in what was sent, and say how many bytes of it were placing or taking off marks.
    fn take(&mut self, sent: &str) -> usize {
        let pieces: Vec<&str> = sent.split("\x1b_G").collect();
        let mut bytes = 0;
        for index in 1..pieces.len() {
            let (controls, data) = pieces[index].split_once(';').unwrap();
            let data = data.split("\x1b\\").next().unwrap();
            let id = || control(controls, "i").unwrap().parse::<u32>().unwrap();
            let placement = || control(controls, "p").map(|p| p.parse::<u32>().unwrap());
            let command = "\x1b_G".len() + controls.len() + 1 + data.len() + "\x1b\\".len();
            match control(controls, "a") {
                Some("t") => {
                    let png = STANDARD.decode(data).unwrap();
                    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
                    // The cursor's line is 2 pixels wide, and every mark wider. Its colour is
                    // the palette's second, after the see-through one.
                    if width > 2 {
                        self.images
                            .insert(id(), Color::Rgb(png[44], png[45], png[46]));
                    }
                }
                Some("p") if self.images.contains_key(&id()) => {
                    let before = pieces[index - 1];
                    let jump = &before[before.rfind("\x1b[").unwrap()..];
                    let (row, column) = jump[2..].trim_end_matches('H').split_once(';').unwrap();
                    let offset = control(controls, "X").unwrap().parse().unwrap();
                    self.placed.insert(
                        (id(), placement().unwrap()),
                        (row.parse().unwrap(), column.parse().unwrap(), offset),
                    );
                    // With the cursor saved, moved to the mark's character and put back.
                    bytes += command + "\x1b7".len() + jump.len() + "\x1b8".len();
                }
                Some("d") if self.images.contains_key(&id()) => {
                    let id = id();
                    match (control(controls, "d"), placement()) {
                        (Some("i"), Some(placement)) => {
                            self.placed.remove(&(id, placement));
                        }
                        (Some("I"), _) => {
                            self.images.remove(&id);
                            self.placed.retain(|(image, _), _| *image != id);
                        }
                        _ => self.placed.retain(|(image, _), _| *image != id),
                    }
                    bytes += command;
                }
                _ => {}
            }
        }
        bytes
    }

    /// Where the marks of this colour are, in order.
    fn of(&self, color: Color) -> Vec<(u16, u16, usize)> {
        let mut at: Vec<_> = self
            .placed
            .iter()
            .filter(|((image, _), _)| self.images[image] == color)
            .map(|(_, at)| *at)
            .collect();
        at.sort_unstable();
        at
    }
}

/// Where the marks of these points should be, those in view, worked out from the page: each
/// point's line at the pixel its frame is in, as the waveform draws it, and its mark a third of a
/// character either side of the line, so its tip is on the line, on the ruler's row, the third.
fn marks_wanted(app: &App, frames: &[u64], cell: usize) -> Vec<(u16, u16, usize)> {
    let page = &app.sample_page;
    let first = page.first_frame();
    let mut at: Vec<_> = frames
        .iter()
        .filter(|frame| **frame >= first && (**frame - first) / page.zoom < 161)
        .map(|frame| {
            let line = ((frame - first) as u128 * cell as u128 / u128::from(page.zoom)) as usize;
            // The waveform starts at the screen's second column.
            let left = cell + line - cell / 3;
            (3, (left / cell + 1) as u16, left % cell)
        })
        .collect();
    at.sort_unstable();
    at
}

/// The first sample's points once it's cut in four, by colour: the start, the end, the loop's
/// ends and the slices' starts.
fn points_by_color() -> [(Color, Vec<u64>); 4] {
    [
        (START_COLOR, vec![1000]),
        (END_COLOR, vec![90_000]),
        (LOOP_COLOR, vec![48_000, 72_000]),
        (SLICE_COLOR, QUARTERS.to_vec()),
    ]
}

#[test]
fn each_mark_is_placed_with_its_tip_on_its_lines_pixel_at_every_zoom() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    // Slice 02, near the loop's start, so both stay in view as it zooms in.
    keys(&mut app, "2 G");
    let mut graphics = Graphics::new();
    let mut marks = Marks::default();
    // The author's screen, at 15 pixels by 39 a character, and a narrower character.
    for cell in [15, 7] {
        let mut zooms = Vec::new();
        for step in 0..12 {
            let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (cell, 39));
            marks.take(&sent);
            for (color, frames) in points_by_color() {
                let wanted = marks_wanted(&app, &frames, usize::from(cell));
                let zoom = app.sample_page.zoom;
                assert_eq!(
                    marks.of(color),
                    wanted,
                    "{color:?} at {zoom} frames a column"
                );
            }
            // Each is a character tall.
            assert!(!sent.contains("a=p,") || sent.contains(",h=39,"));
            zooms.push(app.sample_page.zoom);
            if step < 11 {
                keys(&mut app, "z i");
            }
        }
        assert_eq!(zooms[..3], [FIT, 512, 256]);
        assert_eq!(zooms[11], 1);
        // Nothing's sent again when nothing's changed.
        let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (cell, 39));
        assert!(sent.is_empty(), "{sent:?}");
        keys(&mut app, "z O");
    }
    // The start, the end, the loop and the slices are a colour each, as their lines are.
    assert_eq!(marks.images.len(), 4);
}

#[test]
fn moving_the_cursor_sends_nothing_for_the_marks() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    let mut graphics = Graphics::new();
    let mut marks = Marks::default();
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    let placed = marks.placed.clone();
    assert_eq!(
        placed.len(),
        8,
        "the start, the end, the loop's two and four slices"
    );
    for moves in ["l", "3 h", "$", "M", "] ]", "j", "z c", "l", "z c"] {
        keys(&mut app, moves);
        let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
        assert_eq!(marks.take(&sent), 0, "{moves} placed marks: {sent:?}");
        assert_eq!(marks.placed, placed);
    }
}

#[test]
fn a_window_that_changes_size_is_sent_the_marks_again() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    let mut marks = Marks::default();
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    let first: Vec<u32> = marks.images.keys().copied().collect();
    assert_eq!(first.len(), 4);
    // The clear as the window changes size frees them, so they're stored again with new ids and
    // placed again.
    let (_, sent) = draw_in(&mut app, &mut graphics, 170, 42, (15, 39));
    marks.placed.clear();
    marks.take(&sent);
    let again: Vec<u32> = marks.images.keys().copied().collect();
    assert_eq!(again.len(), 4, "{sent:?}");
    assert!(again.iter().all(|id| !first.contains(id)));
    for (color, frames) in points_by_color().into_iter().take(3) {
        assert_eq!(marks.of(color), marks_wanted(&app, &frames, 15));
    }
    // Once the page is laid out at the new size, they're placed for it, and not sent again.
    let (_, sent) = draw_in(&mut app, &mut graphics, 170, 42, (15, 39));
    assert!(!sent.contains("a=t,f=100"), "{sent:?}");
}

#[test]
fn the_marks_are_taken_off_while_something_is_drawn_over_the_page() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    let mut graphics = Graphics::new();
    let mut marks = Marks::default();
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    let placed = marks.placed.clone();
    keys(&mut app, "Space Space");
    assert!(app.picker.is_some());
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    assert!(marks.placed.is_empty(), "{sent:?}");
    // A command for each kind of mark, rather than for each mark, and one for the cursor's line.
    assert_eq!(sent.matches("a=d,d=i,i=").count(), 4 + 1, "{sent:?}");
    app.key(Key::new(KeyCode::Esc), Instant::now());
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    assert_eq!(marks.placed, placed);
    assert!(!sent.contains("a=t,f=100"), "placed again, not sent again");
    // Leaving the page takes them off too.
    keys(&mut app, "Space 3");
    let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
    marks.take(&sent);
    assert!(marks.placed.is_empty());
}

#[test]
fn a_scroll_places_only_the_marks_in_view() {
    let mut measured = Vec::new();
    for (slices, zoom_ins) in [(16, 2), (256, 4)] {
        let (mut app, _realtime) = on_page();
        command(&mut app, &format!("slice {slices}"));
        keys(&mut app, &"z i ".repeat(zoom_ins));
        let mut graphics = Graphics::new();
        let mut marks = Marks::default();
        let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
        marks.take(&sent);
        let mut steps = Vec::new();
        for _ in 0..8 {
            keys(&mut app, "C-e");
            let (_, sent) = draw_in(&mut app, &mut graphics, 163, 42, (15, 39));
            assert!(!sent.contains("a=t,f=100"), "the marks aren't sent again");
            steps.push((marks.take(&sent), marks.placed.len()));
            let (color, frames) = &points_by_color()[2];
            assert_eq!(marks.of(*color), marks_wanted(&app, frames, 15));
        }
        // Each in view moves a column, at about 60 bytes a mark.
        for (bytes, shown) in &steps {
            assert!(*bytes <= (shown + 1) * 64, "{steps:?}");
        }
        measured.push((slices, app.sample_page.zoom, steps));
    }
    eprintln!("bytes placing marks and marks in view, a scroll at a time: {measured:?}");
}

/// Each letter naming a slice of the first sample, with the slice's number.
fn marked(app: &App) -> Vec<(char, usize)> {
    app.song().bank.get(1).unwrap().edit.points.marks_in_order()
}

#[test]
fn mark_names_a_slice_and_quote_goes_to_it_as_a_jump() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "mark a");
    assert!(app.status.contains("has no slices"), "{}", app.status);
    command(&mut app, "slice 4");
    let depth = app.history.undo_depth();
    command(&mut app, "mark a 2");
    assert!(app.status.contains("'a names slice 02"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    // With no number, the slice the cursor's in.
    keys(&mut app, "G");
    command(&mut app, "mark b");
    command(&mut app, "mark c 9");
    assert!(app.status.contains("slices 00 to 03"), "{}", app.status);
    assert_eq!(marked(&app), [('a', 2), ('b', 3)]);
    // `'a` goes to its slice's start, and `C-o` back to where the cursor was.
    keys(&mut app, "g g");
    keys(&mut app, "' a");
    assert_eq!(cursor(&app), QUARTERS[2]);
    keys(&mut app, "C-o");
    assert_eq!((app.view, cursor(&app)), (View::Sample, 0));
    keys(&mut app, "' z");
    assert!(app.status.contains("is named z"), "{}", app.status);
    assert_eq!(cursor(&app), 0, "a letter naming nothing goes nowhere");
    // `]'` and `['` go by the named slices, with counts, as jumps.
    keys(&mut app, "] '");
    assert_eq!(cursor(&app), QUARTERS[2]);
    keys(&mut app, "] '");
    assert_eq!(cursor(&app), QUARTERS[3]);
    keys(&mut app, "] '");
    assert!(app.status.contains("further"), "{}", app.status);
    keys(&mut app, "[ '");
    assert_eq!(cursor(&app), QUARTERS[2]);
    keys(&mut app, "g g 2 ] '");
    assert_eq!(cursor(&app), QUARTERS[3]);
    keys(&mut app, "C-o");
    assert_eq!(cursor(&app), 0);
    // A letter moves to the slice it's put on, and `u` puts it back.
    command(&mut app, "mark a 1");
    assert!(app.status.contains("not 02"), "{}", app.status);
    keys(&mut app, "u");
    assert_eq!(marked(&app), [('a', 2), ('b', 3)]);
    // From elsewhere, `'a` from `:marks` opens the page on it.
    keys(&mut app, "Space 3");
    command(&mut app, "marks");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .expect("the marks are listed")
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(labels, ["'a  02  0.948 s", "'b  03  1.411 s"]);
    keys(&mut app, "j Enter");
    assert_eq!((app.view, cursor(&app)), (View::Sample, QUARTERS[3]));
    keys(&mut app, "C-o");
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn delmarks_takes_letters_off_and_slicing_moves_them() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    for (letter, slice) in [('a', 0), ('b', 1), ('c', 2), ('x', 3)] {
        command(&mut app, &format!("mark {letter} {slice}"));
    }
    command(&mut app, "delmarks a-b x");
    assert!(app.status.contains("Took 'a 'b 'x off"), "{}", app.status);
    assert_eq!(marked(&app), [('c', 2)]);
    keys(&mut app, "u");
    assert_eq!(marked(&app).len(), 4);
    command(&mut app, "delmarks!");
    assert!(marked(&app).is_empty());
    command(&mut app, "delmarks!");
    assert!(app.status.contains("No such marks"), "{}", app.status);
    keys(&mut app, "u");
    // Slicing into 8 keeps each old start, so each mark follows its sound.
    command(&mut app, "slice 8");
    assert_eq!(marked(&app), [('a', 0), ('b', 2), ('c', 4), ('x', 6)]);
    command(&mut app, "slice clear");
    assert!(marked(&app).is_empty(), "clearing drops them");
    command(&mut app, "marks");
    assert!(app.status.contains("No slice"), "{}", app.status);
}

#[test]
fn k_on_sli_locks_by_a_letter_which_the_grid_shows_and_follows_its_slice() {
    let (mut app, _realtime) = app();
    command(&mut app, "slice 4");
    command(&mut app, "mark b 2");
    command(&mut app, "mark a 2");
    command(&mut app, "mark d 3");
    step_with(&mut app, 0, "C-3 -- --- sampler.slice:01 ---");
    keys(&mut app, "K");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .expect("the slices are listed")
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(
        labels,
        [
            "'a  02  0.948 s",
            "'b  02  0.948 s",
            "'d  03  1.411 s",
            "00      0.021 s",
            "01      0.484 s",
            "02  ab  0.948 s",
            "03  d   1.411 s",
        ]
    );
    // It opens on the slice locked now, and `g g` goes to the first letter.
    keys(&mut app, "g g Enter");
    let slot = app.phrase_here().unwrap().steps()[0].fx[1];
    assert_eq!(slot.slice_mark(), Some('a'));
    let shown = value_shown(&app).unwrap_or_default();
    assert!(shown.contains("'a names 02 of 4"), "{shown}");
    // The grid shows the letter.
    let mut graphics = Graphics::new();
    keys(&mut app, "Space 3");
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    let grid = (0..42).map(|y| row(&terminal, y)).collect::<Vec<_>>();
    assert!(
        grid.iter().any(|line| line.contains("SLI   'a")),
        "{}",
        grid.join("\n")
    );
    // Slices added before it leave the step as it is, and the letter names the same sound.
    app.apply(&Action::SplitSlice {
        number: 1,
        at: 10_000,
    });
    let shown = value_shown(&app).unwrap_or_default();
    assert!(shown.contains("'a names 03 of 5"), "{shown}");
    // With the letter gone, the step keeps it, and the track's own `Slice` plays.
    command(&mut app, "delmarks a");
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[1].slice_mark(),
        Some('a')
    );
    let shown = value_shown(&app).unwrap_or_default();
    assert!(
        shown.contains("'a names no slice of 01 one, so the track's own plays: 00 of 5"),
        "{shown}"
    );
    // Two digits typed over it lock by number again.
    keys(&mut app, "r 0 3");
    let slot = app.phrase_here().unwrap().steps()[0].fx[1];
    assert_eq!((slot.slice_mark(), slot.coarse()), (None, 3));
}

#[test]
fn the_ruler_puts_the_letters_after_their_slices_mark() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    command(&mut app, "mark a 2");
    command(&mut app, "mark b 2");
    command(&mut app, "mark z 1");
    let column = |frame: u64| 1 + (frame / FIT) as usize;
    let mut text = Terminal::new(TestBackend::new(163, 42)).unwrap();
    app.sample_images = None;
    app.sample_page.cell_width = None;
    text.draw(|frame| draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let ruler = row(&text, 2);
    let shown: String = ruler
        .chars()
        .skip(column(QUARTERS[2]) - 2)
        .take(5)
        .collect();
    assert_eq!(shown, "02▾ab", "{ruler}");
    let shown: String = ruler
        .chars()
        .skip(column(QUARTERS[1]) - 2)
        .take(4)
        .collect();
    assert_eq!(shown, "01▾z", "{ruler}");
    // The settings row lists them in the order of their slices, and the cursor's row names its
    // slice's.
    keys(&mut app, "2 G");
    text.draw(|frame| draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let rows: Vec<String> = (0..42).map(|y| row(&text, y)).collect();
    let settings = rows.iter().find(|line| line.contains("Slices")).unwrap();
    assert!(settings.contains("Marks zab"), "{settings}");
    let slice_row = rows.iter().find(|line| line.starts_with(" Slice")).unwrap();
    assert!(slice_row.starts_with(" Slice 02 'a 'b"), "{slice_row}");
    // With the pictures drawn, the letters keep clear of the columns the placed mark covers.
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    let ruler = row(&terminal, 2);
    let line = (QUARTERS[2] * 3 / FIT) as usize;
    let last = 1 + (line + 1) / 3;
    let letters: String = ruler.chars().skip(last + 1).take(2).collect();
    assert_eq!(letters, "ab", "{ruler}");
}

/// Play this many frames of audio, and take in the callback's report.
fn play(realtime: &mut Realtime, app: &mut App, frames: usize) {
    let mut out = vec![0.0; frames * 2];
    realtime.process(&mut out);
    app.tick();
}

/// Where the first track's voices are in their samples, as the engine has them.
fn heads(realtime: &Realtime) -> Vec<Playhead> {
    realtime
        .engine()
        .playheads(0)
        .into_iter()
        .flatten()
        .collect()
}

#[test]
fn semicolon_plays_the_slice_under_the_cursor_and_its_playhead_follows() {
    let (mut app, mut realtime) = on_page();
    command(&mut app, "slice 4");
    keys(&mut app, "2 G l l l");
    assert!(cursor(&app) > QUARTERS[2]);
    keys(&mut app, ";");
    play(&mut realtime, &mut app, 1000);
    assert_eq!(
        heads(&realtime),
        [Playhead {
            entry: None,
            frame: QUARTERS[2] + 1000
        }],
        "from the slice's start, not the cursor"
    );
    let shown = app.sample_shown().unwrap();
    assert_eq!(app.sample_playheads(&shown), [QUARTERS[2] + 1000]);
    // A tap plays to the slice's end, and no further.
    play(
        &mut realtime,
        &mut app,
        (QUARTERS[3] - QUARTERS[2]) as usize - 1500,
    );
    assert_eq!(realtime.engine().previews_sounding(), 1, "a tap plays on");
    play(&mut realtime, &mut app, 1000);
    assert!(heads(&realtime).is_empty(), "it ends with the slice");
    assert!(app.sample_playheads(&shown).is_empty());
}

#[test]
fn semicolon_held_plays_until_let_go_and_tapped_plays_out() {
    let (mut app, mut realtime) = on_page();
    let key = parse_sequence(";").unwrap()[0];
    let at = Instant::now();
    app.key(key, at);
    play(&mut realtime, &mut app, 1000);
    assert_eq!(realtime.engine().previews_sounding(), 1);
    // Its repeats while it's held don't start it again.
    app.key_repeated(key, at + Duration::from_millis(200));
    play(&mut realtime, &mut app, 1000);
    assert_eq!(heads(&realtime)[0].frame, 1000 + 2000);
    app.key_released(key, at + Duration::from_millis(400));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(realtime.engine().previews_sounding(), 0, "held, it stops");
    // Tapped, it plays on.
    app.key(key, at + Duration::from_secs(1));
    app.key_released(key, at + Duration::from_millis(1100));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(
        realtime.engine().previews_sounding(),
        1,
        "tapped, it plays on"
    );
    // Esc lets go of it.
    app.key(Key::new(KeyCode::Esc), at + Duration::from_secs(2));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(realtime.engine().previews_sounding(), 0);
}

#[test]
fn space_c_plays_from_the_cursor_to_the_end_and_tab_stays_the_songs() {
    let (mut app, mut realtime) = on_page();
    keys(&mut app, "} l");
    let from = cursor(&app);
    keys(&mut app, "Space c");
    play(&mut realtime, &mut app, 500);
    assert_eq!(
        heads(&realtime),
        [Playhead {
            entry: None,
            frame: from + 500
        }]
    );
    // It plays to the entry's end at 90 000, not on to the file's.
    play(&mut realtime, &mut app, (90_000 - from) as usize - 1000);
    assert_eq!(realtime.engine().previews_sounding(), 1);
    play(&mut realtime, &mut app, 1200);
    assert_eq!(realtime.engine().previews_sounding(), 0);
    keys(&mut app, "Tab");
    play(&mut realtime, &mut app, 100);
    assert_eq!(app.transport, Transport::Playing, "Tab plays the song");
}

#[test]
fn the_keys_mode_plays_slices_in_order_from_the_home_row_and_moves_the_cursor() {
    let (mut app, mut realtime) = on_page();
    command(&mut app, "slice 20");
    keys(&mut app, "i");
    assert_eq!(app.mode, Mode::Keys);
    assert_eq!(app.key_scope(), Mode::Sample);
    let shown = app.sample_shown().unwrap();
    for (name, number) in [("a", 0), ("s", 1), ("k", 7), ("q", 8), ("i", 15), ("d", 2)] {
        keys(&mut app, name);
        assert_eq!(cursor(&app), shown.slice(number).start, "{name}");
        play(&mut realtime, &mut app, 100);
        assert_eq!(
            heads(&realtime)[0].frame,
            shown.slice(number).start + 100,
            "{name} plays slice {number}"
        );
    }
    // `>` and `<` go a page of sixteen on and back, and no further than the last slice.
    keys(&mut app, "> a");
    assert_eq!(cursor(&app), shown.slice(16).start);
    keys(&mut app, "> f");
    assert_eq!(cursor(&app), shown.slice(19).start);
    keys(&mut app, "g");
    assert!(app.status.contains("No slice 14"), "{}", app.status);
    keys(&mut app, "< <");
    assert_eq!(app.sample_page.pads_from, 0);
    // Other keys work on the page as they do, and Esc leaves.
    keys(&mut app, "0");
    assert_eq!(cursor(&app), shown.slice(19).start);
    keys(&mut app, "Esc");
    assert_eq!(app.mode, Mode::Normal);
    keys(&mut app, "a");
    assert_eq!(cursor(&app), shown.slice(19).start, "a plays nothing now");
}

#[test]
fn a_pad_held_stops_when_let_go_and_the_next_pad_takes_its_place() {
    let (mut app, mut realtime) = on_page();
    command(&mut app, "slice 4");
    keys(&mut app, "i");
    let at = Instant::now();
    let [a, s] = ["a", "s"].map(|name| parse_sequence(name).unwrap()[0]);
    app.key(a, at);
    play(&mut realtime, &mut app, 1000);
    app.key(s, at + Duration::from_millis(400));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(
        heads(&realtime)[0].frame,
        QUARTERS[1] + 4800,
        "s took a's place"
    );
    // Letting go of a, held long, doesn't stop s.
    app.key_released(a, at + Duration::from_millis(600));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(realtime.engine().previews_sounding(), 1);
    app.key_released(s, at + Duration::from_millis(900));
    play(&mut realtime, &mut app, 4800);
    assert_eq!(realtime.engine().previews_sounding(), 0, "s held, let go");
}

/// Where each placement of an image was placed, by placement id, from the waveform's left in the
/// 3-pixel characters `draw` gives.
fn placements(sent: &str, id: &str) -> BTreeMap<u32, i64> {
    let mut placed = BTreeMap::new();
    let mut from = 0;
    while let Some(at) = sent[from..].find("\x1b_Ga=p,") {
        let start = from + at;
        let end = start + sent[start..].find(';').unwrap();
        let controls = &sent[start + 3..end];
        from = end;
        if control(controls, "i") != Some(id) {
            continue;
        }
        let placement: u32 = control(controls, "p").unwrap().parse().unwrap();
        let offset: i64 = control(controls, "X").unwrap().parse().unwrap();
        // The cursor jump just before the command says the character.
        let jump = &sent[sent[..start].rfind("\x1b[").unwrap()..start];
        let column: i64 = jump
            .split(';')
            .nth(1)
            .unwrap()
            .trim_end_matches('H')
            .parse()
            .unwrap();
        placed.insert(placement, (column - 2) * 3 + offset);
    }
    placed
}

/// The id of the image stored in the playhead's green, its line.
fn playhead_id(sent: &str) -> Option<String> {
    sent.split("\x1b_G")
        .filter_map(|command| command.split_once(';'))
        .find(|(controls, data)| {
            controls.starts_with("a=t,")
                && STANDARD
                    .decode(data.split("\x1b\\").next().unwrap_or_default())
                    .is_ok_and(|png| png.windows(3).any(|rgb| rgb == [0xa6, 0xd1, 0x89]))
        })
        .and_then(|(controls, _)| control(controls, "i").map(str::to_owned))
}

#[test]
fn each_playhead_is_a_green_line_placed_at_its_pixel_and_nothing_is_sent_while_nothing_plays() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(
        sent.is_empty(),
        "nothing plays, so nothing's sent: {sent:?}"
    );
    // The track's note of the entry, and what the page played, each have a line. A note of
    // another entry doesn't.
    app.sample_page.heard = app.sample_page.shown;
    app.playheads[0] = Some(Playhead {
        entry: Some(1),
        frame: FIT * 10,
    });
    app.playheads[1] = Some(Playhead {
        entry: Some(2),
        frame: FIT * 20,
    });
    app.playheads[2] = Some(Playhead {
        entry: None,
        frame: FIT * 30 + 400,
    });
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    let id = playhead_id(&sent).expect("the line is stored once");
    assert_eq!(
        placements(&sent, &id),
        BTreeMap::from([(1, 30), (2, 92)]),
        "{sent:?}"
    );
    // Moving one places only it again, a pixel along.
    app.playheads[0] = Some(Playhead {
        entry: Some(1),
        frame: FIT * 10 + 200,
    });
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(playhead_id(&sent).is_none(), "not stored again");
    assert_eq!(placements(&sent, &id), BTreeMap::from([(1, 31)]));
    assert!(sent.len() < 100, "{} bytes: {sent:?}", sent.len());
    // When it stops, its lines are taken off, and then nothing more is sent.
    app.playheads = [None; PLAYHEADS];
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches(&format!("a=d,d=i,i={id},")).count(), 2);
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.is_empty(), "{sent:?}");
}

#[test]
fn as_its_column_a_playhead_is_its_columns_background() {
    let (mut app, _realtime) = on_page();
    app.playheads[0] = Some(Playhead {
        entry: Some(1),
        frame: FIT * 10 + 5,
    });
    let green = vt_tui::views::sample::playhead_column();
    let green_columns = |terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (0..163)
            .filter(|x| buffer[(*x, 10)].bg == green)
            .collect::<Vec<u16>>()
    };
    // With the pictures drawn and the cursor a line, it's a line, so no column is green.
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (terminal, _) = draw(&mut app, &mut graphics, 163, 42);
    assert!(green_columns(&terminal).is_empty());
    // As the cursor's column, `zc`, it's its column too: the waveform's 11th, the screen's 12th.
    keys(&mut app, "z c");
    let (terminal, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(sent.contains("a=d,d=i"), "the line is taken off");
    assert_eq!(green_columns(&terminal), [11]);
}

/// The first sample's slices, as where each starts and its id.
fn slices(app: &App) -> Vec<(u64, u32)> {
    app.song()
        .bank
        .get(1)
        .unwrap()
        .edit
        .points
        .slices
        .iter()
        .map(|slice| (slice.start, slice.id))
        .collect()
}

/// Put the sample page's cursor on a frame.
fn put_cursor(app: &mut App, frame: u64) {
    let length = app.sample_shown().unwrap().length();
    app.sample_page.show_frame(frame, length);
}

#[test]
fn enter_j_and_x_split_join_and_unsplit_with_ids_and_letters_following_and_each_undoes() {
    let (mut app, _realtime) = on_page();
    // At the start there's nothing to split.
    keys(&mut app, "Enter");
    assert!(slices(&app).is_empty());
    assert!(
        app.status.contains("between the start and the end"),
        "{}",
        app.status
    );
    command(&mut app, "slice 4");
    keys(&mut app, "j m a");
    assert_eq!(marked(&app), [('a', 1)]);
    // Enter splits slice 01 at the cursor: the part before keeps its id and its letter.
    put_cursor(&mut app, 30_000);
    keys(&mut app, "Enter");
    let split = vec![
        (1000, 0),
        (23_250, 1),
        (30_000, 4),
        (45_500, 2),
        (67_750, 3),
    ];
    assert_eq!(slices(&app), split);
    assert_eq!(marked(&app), [('a', 1)]);
    assert!(app.status.contains("Slice 02 of 5"), "{}", app.status);
    // Again on a slice's start, there's one there already.
    keys(&mut app, "Enter");
    assert_eq!(slices(&app).len(), 5);
    assert!(app.status.contains("already starts"), "{}", app.status);
    // J joins the slice under the cursor with the next, which takes its letters with it.
    put_cursor(&mut app, 50_000);
    keys(&mut app, "m b");
    assert_eq!(marked(&app), [('a', 1), ('b', 3)]);
    put_cursor(&mut app, 31_000);
    keys(&mut app, "J");
    assert_eq!(
        slices(&app),
        [(1000, 0), (23_250, 1), (30_000, 4), (67_750, 3)]
    );
    assert_eq!(marked(&app), [('a', 1), ('b', 2)]);
    // x on a slice's start takes it away, joining it to the slice before, never cutting audio.
    put_cursor(&mut app, 30_000);
    keys(&mut app, "x");
    assert_eq!(slices(&app), [(1000, 0), (23_250, 1), (67_750, 3)]);
    assert_eq!(marked(&app), [('a', 1), ('b', 1)]);
    assert_eq!(
        app.sample_shown().unwrap().end(),
        90_000,
        "the audio is all there"
    );
    // Slice 00 is where the slices start, so x leaves it.
    put_cursor(&mut app, 1000);
    keys(&mut app, "x");
    assert_eq!(slices(&app).len(), 3);
    // Each step is one undo.
    keys(&mut app, "u");
    assert_eq!(slices(&app).len(), 4);
    keys(&mut app, "u");
    assert_eq!(slices(&app).len(), 5);
    keys(&mut app, "u u");
    assert_eq!(slices(&app).len(), 4);
    assert_eq!(marked(&app), [('a', 1)]);
}

#[test]
fn nudges_move_a_point_by_a_pixel_a_column_and_a_frame_at_every_zoom() {
    let (mut app, _realtime) = on_page_in_pixels();
    command(&mut app, "slice 4");
    keys(&mut app, "j");
    let start = |app: &App| slices(app)[1].0;
    for zoom in [FIT, 512, 64, 16, 4] {
        while app.sample_page.zoom > zoom {
            keys(&mut app, "z i");
        }
        assert_eq!(app.sample_page.zoom, zoom);
        let from = start(&app);
        let pixel = |frame: u64| frame * CELL / zoom;
        keys(&mut app, "=");
        let moved = start(&app);
        if zoom >= CELL {
            assert_eq!(pixel(moved), pixel(from) + 1, "a pixel at {zoom}");
            assert!(moved - from <= zoom.div_ceil(CELL), "{moved} at {zoom}");
        } else {
            assert_eq!(
                moved,
                from + 1,
                "a frame a step closer than a frame a pixel"
            );
        }
        assert_eq!(cursor(&app), moved, "the cursor follows the point");
        keys(&mut app, "-");
        assert!(start(&app) < moved);
        let here = start(&app);
        keys(&mut app, "+");
        assert_eq!(start(&app), here + zoom, "a column at {zoom}");
        keys(&mut app, "_ D-l");
        assert_eq!(start(&app), here + 1, "a frame at {zoom}");
        keys(&mut app, "D-h 3 D-l");
        assert_eq!(start(&app), here + 3, "a count of frames at {zoom}");
        keys(&mut app, "3 D-h");
        assert_eq!(start(&app), here);
    }
    // No further than the slices either side.
    keys(&mut app, "z O 1 0 0 0 _");
    assert_eq!(start(&app), 1001);
    keys(&mut app, "1 0 0 0 +");
    assert_eq!(start(&app), 45_499);
    // Every nudge is one undo.
    keys(&mut app, "u");
    assert_eq!(start(&app), 1001);
    // Nothing renumbers, so the ids stay in order.
    assert_eq!(
        slices(&app).iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
}

/// What a key does to the song from where the cursor is, undone after, with the cursor put back.
fn change_by(app: &mut App, key: &str) -> Song {
    let (before, at) = (Song::clone(app.song()), cursor(app));
    keys(app, key);
    let after = Song::clone(app.song());
    if after != before {
        keys(app, "u");
    }
    put_cursor(app, at);
    after
}

#[test]
fn cmd_keys_nudge_as_their_keys_but_cmd_h_and_l_by_a_frame_and_on_a_setting_all_four_as_theirs() {
    let (mut app, _realtime) = on_page_in_pixels();
    command(&mut app, "slice 4");
    keys(&mut app, "j");
    let start = Song::clone(app.song());
    // Up and down a column, as `+` and `_`.
    for (plain, cmd) in [("+", "D-k"), ("_", "D-j")] {
        let by_plain = change_by(&mut app, plain);
        assert_ne!(by_plain, start, "{plain} nudges");
        assert_eq!(change_by(&mut app, cmd), by_plain, "{cmd} as {plain}");
    }
    // Left and right a frame, where `=` and `-` go a pixel: the sample page's own, kept.
    let from = slices(&app)[1].0;
    keys(&mut app, "D-l");
    assert_eq!(slices(&app)[1].0, from + 1);
    keys(&mut app, "u");
    // On a setting, all four are the keys they stand for.
    keys(&mut app, "K j j j j Enter l");
    assert_eq!(app.sample_page.setting, Some(SampleSetting::Pan));
    let start = Song::clone(app.song());
    for (plain, cmd) in [("=", "D-l"), ("-", "D-h"), ("+", "D-k"), ("_", "D-j")] {
        let by_plain = change_by(&mut app, plain);
        assert_ne!(by_plain, start, "{plain} changes the pan");
        assert_eq!(change_by(&mut app, cmd), by_plain, "{cmd} as {plain}");
    }
}

#[test]
fn g_s_g_e_g_l_and_g_upper_l_put_the_points_at_the_cursor_and_x_takes_them_back() {
    let (mut app, _realtime) = on_page();
    let points = |app: &App| app.song().bank.get(1).unwrap().edit.points.clone();
    put_cursor(&mut app, 2000);
    keys(&mut app, "g s");
    assert_eq!(points(&app).start, 2000);
    put_cursor(&mut app, 80_000);
    keys(&mut app, "g e");
    assert_eq!(points(&app).end, Some(80_000));
    put_cursor(&mut app, 50_000);
    keys(&mut app, "g l g L");
    assert!(app.status.contains("after its start"), "{}", app.status);
    put_cursor(&mut app, 60_000);
    keys(&mut app, "g L");
    let looped = points(&app).loop_.unwrap();
    assert_eq!((looped.start, looped.end), (50_000, 60_000));
    // The start can't pass the end.
    put_cursor(&mut app, 85_000);
    keys(&mut app, "g s");
    assert_eq!(points(&app).start, 2000);
    // x on a loop point takes the loop away, and on the end puts the file's back.
    put_cursor(&mut app, 50_000);
    keys(&mut app, "x");
    assert_eq!(points(&app).loop_, None);
    put_cursor(&mut app, 80_000);
    keys(&mut app, "x");
    assert_eq!(points(&app).end, None);
    // With no loop, g l loops from the cursor to the end.
    put_cursor(&mut app, 30_000);
    keys(&mut app, "g l");
    let looped = points(&app).loop_.unwrap();
    assert_eq!((looped.start, looped.end), (30_000, LENGTH));
    keys(&mut app, "u u");
    assert_eq!(points(&app).end, Some(80_000));
}

#[test]
fn snap_and_reverse_are_toggles_that_flip_with_their_settings() {
    use vt_tui::glyphs::{SWITCH_OFF, SWITCH_ON};
    use vt_tui::theme;
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    // The glyph after a setting's name on a row of the settings, its colours, and where the next
    // setting starts.
    let toggle = |app: &mut App, graphics: &mut Graphics, y: u16, name: &str| {
        let (terminal, _) = draw(app, graphics, 163, 42);
        let line = row(&terminal, y);
        let at = line.find(name).unwrap();
        let x = (line[..at].chars().count() + name.chars().count()) as u16;
        let buffer = terminal.backend().buffer();
        // The two plain spaces after it, in its colours, are the rest of what its picture covers.
        for after in [1, 2] {
            assert_eq!(buffer[(x + after, y)].symbol(), " ", "{line}");
            assert_eq!(buffer[(x + after, y)].fg, buffer[(x, y)].fg, "{line}");
            assert_eq!(buffer[(x + after, y)].bg, buffer[(x, y)].bg, "{line}");
        }
        let cell = &buffer[(x, y)];
        (cell.symbol().chars().next().unwrap(), cell.fg, cell.bg)
    };
    let off = (SWITCH_OFF, theme::OVERLAY0, theme::BASE);
    let on = (SWITCH_ON, theme::GREEN, theme::BASE);
    draw(&mut app, &mut graphics, 163, 42);
    let fades = |app: &mut App, graphics: &mut Graphics| {
        let (terminal, _) = draw(app, graphics, 163, 42);
        row(&terminal, 34).find("Fade in")
    };
    let fade_at = fades(&mut app, &mut graphics);
    assert!(fade_at.is_some());
    assert_eq!(toggle(&mut app, &mut graphics, 33, "Snap "), off);
    assert_eq!(toggle(&mut app, &mut graphics, 34, "Reverse "), off);
    // `~` turns it round, and what's after it stays where it was.
    keys(&mut app, "~");
    assert_eq!(toggle(&mut app, &mut graphics, 34, "Reverse "), on);
    assert_eq!(fades(&mut app, &mut graphics), fade_at);
    // `K` and `=` turn Snap on, in the cursor's colours while the focus is on it, and the status
    // line still says the word.
    keys(&mut app, "K Enter =");
    assert!(app.song().bank.get(1).unwrap().edit.play.snap_zero);
    let (glyph, _, background) = toggle(&mut app, &mut graphics, 33, "Snap ");
    assert_eq!((glyph, background), (SWITCH_ON, theme::ROSEWATER));
    assert!(app.status.contains("on"), "{}", app.status);
    keys(&mut app, "Esc");
    assert_eq!(toggle(&mut app, &mut graphics, 33, "Snap "), on);
}

#[test]
fn with_snap_on_a_point_placed_lands_on_a_zero_crossing_within_a_column() {
    let (mut app, _realtime) = on_page();
    // K on the waveform lists the settings, Snap first, and = turns it on.
    keys(&mut app, "K Enter =");
    assert_eq!(app.sample_page.setting, Some(SampleSetting::Snap));
    assert!(app.song().bank.get(1).unwrap().edit.play.snap_zero);
    keys(&mut app, "Esc");
    assert_eq!(app.sample_page.setting, None);
    let shown = app.sample_shown().unwrap();
    // The square wave changes sign every 100 frames.
    for (target, zoom) in [(30_030, FIT), (40_071, 256), (44_449, 64)] {
        while app.sample_page.zoom > zoom {
            keys(&mut app, "z i");
        }
        put_cursor(&mut app, target);
        keys(&mut app, "Enter");
        let points = &app.song().bank.get(1).unwrap().edit.points;
        let placed = points.slices[points.slice_at(target + zoom)].start;
        assert_eq!(placed % 100, 0, "{placed} from {target}");
        assert!(
            placed.abs_diff(target) <= zoom,
            "{placed} from {target} at {zoom}"
        );
        assert_eq!(shown.zero_crossing(target, zoom, None), Some(placed));
    }
    // A nudge goes on to the next crossing that way.
    let start = slices(&app)[1].0;
    put_cursor(&mut app, start);
    keys(&mut app, "=");
    assert_eq!(slices(&app)[1].0, start + 100);
    // A frame's nudge never snaps.
    keys(&mut app, "D-l");
    assert_eq!(slices(&app)[1].0, start + 101);
}

#[test]
fn enter_while_it_plays_splits_at_the_playhead() {
    let (mut app, mut realtime) = on_page();
    command(&mut app, "slice 4");
    keys(&mut app, "2 G ;");
    play(&mut realtime, &mut app, 1000);
    put_cursor(&mut app, 10_000);
    keys(&mut app, "Enter");
    assert_eq!(slices(&app)[3].0, QUARTERS[2] + 1000, "{:?}", slices(&app));
    assert!(app.status.contains("playhead"), "{}", app.status);
    assert_eq!(cursor(&app), 10_000, "the cursor stays where it was");
    // Tapped again further on.
    play(&mut realtime, &mut app, 2000);
    keys(&mut app, "Enter");
    assert_eq!(slices(&app)[4].0, QUARTERS[2] + 3000);
    // Once it's stopped, Enter splits at the cursor again.
    keys(&mut app, "Esc");
    play(&mut realtime, &mut app, 4000);
    play(&mut realtime, &mut app, 4000);
    keys(&mut app, "Enter");
    assert_eq!(slices(&app)[1].0, 10_000);
}

#[test]
fn the_settings_rows_change_a_slices_own_and_the_gain_and_reverse_keys_work_from_the_waveform() {
    let (mut app, _realtime) = on_page();
    let over =
        |app: &App, slice: usize| app.song().bank.get(1).unwrap().edit.points.slices[slice].over;
    // With no slices, they're the entry's own.
    keys(&mut app, ">");
    assert_eq!(app.song().bank.get(1).unwrap().edit.play.gain_db, 1.0);
    keys(&mut app, "~");
    assert!(app.song().bank.get(1).unwrap().edit.play.reverse);
    keys(&mut app, "u u");
    command(&mut app, "slice 4");
    keys(&mut app, "j 3 <");
    assert_eq!(over(&app, 1).gain_db, Some(-3.0));
    assert!(
        app.status.contains("Slice 01 gain -3.0 dB"),
        "{}",
        app.status
    );
    keys(&mut app, "~");
    assert_eq!(over(&app, 1).reverse, Some(true));
    // K, and the fifth setting, after Snap, Sens, Steps and Warp, is Gain, which x puts back as
    // the sample's.
    keys(&mut app, "K j j j j Enter");
    assert_eq!(app.sample_page.setting, Some(SampleSetting::Gain));
    keys(&mut app, "x");
    assert_eq!(over(&app, 1).gain_db, None);
    // l goes along the rows to Pan, where + moves ten.
    keys(&mut app, "l + -");
    assert_eq!(over(&app, 1).pan, Some(9));
    // j goes on to the next slice with the focus where it was.
    keys(&mut app, "j =");
    assert_eq!(over(&app, 2).pan, Some(1));
    // K lists the values, the sample's own first.
    keys(&mut app, "K Enter");
    assert_eq!(over(&app, 2).pan, None);
    // Fades in milliseconds.
    keys(&mut app, "$ 1 0 =");
    assert_eq!(over(&app, 2).fade_out, Some(480));
    keys(&mut app, "Esc =");
    assert_eq!(
        over(&app, 2).fade_out,
        Some(480),
        "= on the waveform nudges again"
    );
    keys(&mut app, "u u");
    assert_eq!(over(&app, 2).fade_out, None);
}

#[test]
fn v_selects_frames_to_hear_and_quote_lt_gt_and_quote_quote_go_back() {
    let (mut app, mut realtime) = on_page();
    put_cursor(&mut app, 10_000);
    keys(&mut app, "v");
    put_cursor(&mut app, 20_000);
    assert_eq!(app.sample_page.selection(), Some(10_000..20_001));
    keys(&mut app, ";");
    play(&mut realtime, &mut app, 1000);
    assert_eq!(
        heads(&realtime),
        [Playhead {
            entry: None,
            frame: 11_000
        }]
    );
    keys(&mut app, "o");
    assert_eq!(cursor(&app), 10_000);
    keys(&mut app, "Esc");
    assert_eq!(app.sample_page.selection(), None);
    // '< and '> are the last selection's ends, as jumps, and '' goes back from the latest.
    put_cursor(&mut app, 40_000);
    keys(&mut app, "' >");
    assert_eq!(cursor(&app), 20_000);
    keys(&mut app, "' <");
    assert_eq!(cursor(&app), 10_000);
    keys(&mut app, "' '");
    assert_eq!(cursor(&app), 20_000);
    keys(&mut app, "' '");
    assert_eq!(cursor(&app), 10_000);
    // g v selects it again.
    keys(&mut app, "g v");
    assert_eq!(app.sample_page.selection(), Some(10_000..20_001));
}

#[test]
fn every_point_editing_key_is_listed() {
    let (app, _realtime) = on_page();
    let rows = app.help_rows();
    for keys in [
        "Enter", "J", "x", "=", "+", "D-l", "g s", "g e", "g l", "g L", "m", "v", "g v", ">", "~",
        "K",
    ] {
        assert!(
            rows.iter()
                .any(|row| row.trim_start().starts_with(&format!("{keys} "))),
            "{keys}"
        );
    }
}

/// The loudness of this many frames of audio, as the root of their mean square, after taking in
/// the callback's report.
fn loudness(realtime: &mut Realtime, app: &mut App, frames: usize) -> f32 {
    let mut out = vec![0.0; frames * 2];
    realtime.process(&mut out);
    app.tick();
    (out.iter().map(|sample| sample * sample).sum::<f32>() / out.len() as f32).sqrt()
}

/// A stereo file at 44.1 kHz, a 441 Hz sine at half of full scale for a second, so it's loaded
/// at the engine's 48 kHz as a file from a library is.
fn sine_file(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for at in 0..44_100 {
        let phase = at as f32 * 441.0 / 44_100.0 * std::f32::consts::TAU;
        let value = (16_384.0 * phase.sin()) as i16;
        writer.write_sample(value).unwrap();
        writer.write_sample(value).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn semicolon_and_space_c_are_heard_from_a_file_loaded_at_another_rate() {
    let folder = std::env::temp_dir().join(format!("vt-sample-heard-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("sine.wav");
    sine_file(&file);
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(RATE, version));
    let mut app = App::new(song, None, samples, handle, RATE);
    command(&mut app, &format!("sample {}", file.display()));
    // The page shows it once it's analysed, on a thread of its own.
    let started = Instant::now();
    while app.sample_shown().is_err() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
        app.tick();
    }
    keys(&mut app, "Space 0");
    assert_eq!(app.view, View::Sample);
    assert!(
        loudness(&mut realtime, &mut app, 4800) < 1e-6,
        "silent at first"
    );
    for audition in [";", "Space c"] {
        keys(&mut app, audition);
        let heard = loudness(&mut realtime, &mut app, 4800);
        // A sine at half of full scale is about 0.35, less the track's level and the note's.
        assert!(heard > 0.05, "{audition} is heard: {heard} {}", app.status);
        let shown = app.sample_shown().unwrap();
        assert!(
            !app.sample_playheads(&shown).is_empty(),
            "{audition}'s playhead"
        );
        assert!(app.status.is_empty(), "{audition}: {}", app.status);
        keys(&mut app, "Esc");
        loudness(&mut realtime, &mut app, RATE as usize / 2);
    }
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn an_audition_a_mute_or_a_solo_silences_says_so() {
    let (mut app, mut realtime) = on_page();
    keys(&mut app, ";");
    assert!(loudness(&mut realtime, &mut app, 4800) > 0.05);
    assert!(app.status.is_empty(), "{}", app.status);
    keys(&mut app, "Esc");
    // It plays through the track, so the track's mute silences it, and the page says so.
    keys(&mut app, "Space m");
    loudness(&mut realtime, &mut app, RATE as usize / 2);
    keys(&mut app, ";");
    assert!(loudness(&mut realtime, &mut app, 4800) < 1e-6, "muted");
    assert_eq!(
        app.status,
        "Track 01 is muted, so it plays silently. Space m unmutes it"
    );
    // Another track soloed silences it too.
    keys(&mut app, "Esc Space m");
    app.cursor.track = 1;
    keys(&mut app, "Space s");
    app.cursor.track = 0;
    loudness(&mut realtime, &mut app, RATE as usize / 2);
    keys(&mut app, "Space c");
    assert!(loudness(&mut realtime, &mut app, 4800) < 1e-6, "soloed out");
    assert_eq!(
        app.status,
        "Track 02 is soloed, so Track 01 plays silently. Space M clears the solos"
    );
    keys(&mut app, "Esc Space M");
    loudness(&mut realtime, &mut app, RATE as usize / 2);
    keys(&mut app, ";");
    assert!(
        loudness(&mut realtime, &mut app, 4800) > 0.05,
        "heard again"
    );
    assert!(app.status.is_empty(), "{}", app.status);
}

/// The status line's badge and the line under it.
fn status_lines(app: &mut App) -> (String, String) {
    let mut terminal = Terminal::new(TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    (row(&terminal, 40), row(&terminal, 41))
}

#[test]
fn each_mode_of_the_page_names_itself_on_the_badge_and_esc_leaves_it() {
    let (mut app, _realtime) = on_page();
    command(&mut app, "slice 4");
    let (badge, _) = status_lines(&mut app);
    assert!(badge.starts_with(" SAMPLE "), "{badge}");
    // The keys mode: h, j and k are pads there, so it says so, and that Esc gives them back.
    keys(&mut app, "i");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" PADS "), "{badge}");
    assert!(
        hint.contains("Esc leaves, for h j k l to move again"),
        "{hint}"
    );
    // It stays named after a pad plays.
    keys(&mut app, "s");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" PADS "), "{badge}");
    assert!(hint.contains("play slices 00 to 07"), "{hint}");
    keys(&mut app, "Esc");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" SAMPLE "), "{badge}");
    assert!(hint.trim().is_empty(), "{hint}");
    // The settings rows: h and l go along them, so it says so, and h at the first says that Esc
    // goes back to the waveform.
    keys(&mut app, "K Enter");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" SETTINGS "), "{badge}");
    assert!(hint.contains("Esc goes back to the waveform"), "{hint}");
    let before = cursor(&app);
    keys(&mut app, "h");
    assert_eq!(cursor(&app), before, "h goes along the settings");
    assert!(
        app.status.contains("is the first setting. Esc goes back"),
        "{}",
        app.status
    );
    keys(&mut app, "Esc");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" SAMPLE "), "{badge}");
    assert!(hint.trim().is_empty(), "{hint}");
    // Visual says how it ends.
    keys(&mut app, "v");
    let (badge, hint) = status_lines(&mut app);
    assert!(badge.starts_with(" VISUAL "), "{badge}");
    assert!(hint.contains("v or Esc ends it"), "{hint}");
    keys(&mut app, "Esc");
    let (badge, _) = status_lines(&mut app);
    assert!(badge.starts_with(" SAMPLE "), "{badge}");
}

#[test]
fn with_the_focus_on_the_settings_the_cursors_line_is_set_back() {
    let (mut app, _realtime) = on_page();
    let mut graphics = Graphics::new();
    draw(&mut app, &mut graphics, 163, 42);
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert!(!sent.contains("a=t,"), "nothing new to send");
    keys(&mut app, "K Enter");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches("a=t,").count(), 1, "a dimmer line");
    keys(&mut app, "Esc");
    let (_, sent) = draw(&mut app, &mut graphics, 163, 42);
    assert_eq!(sent.matches("a=t,").count(), 1, "the line as it was");
}
