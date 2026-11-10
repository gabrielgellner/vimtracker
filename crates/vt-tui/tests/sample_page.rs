//! The sample page, `Space 0`: the cursor's track's sample as a big waveform, with Vim's motions
//! over it at every zoom, its pictures sent only when what they show changes, and its layout on
//! the author's screen and in a smaller window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::terminal::WindowSize;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash, Loop, Points};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::View;
use vt_tui::kitty::Graphics;
use vt_tui::run::draw_screen;

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
    keys(&mut app, "Space 4 j g d");
    assert_eq!(app.view, View::Sample);
    assert_eq!(cursor(&app), 1000);
    keys(&mut app, "C-o");
    assert_eq!(app.view, View::Track);
    // Elsewhere on the Sound page, it says where it works.
    keys(&mut app, "j g d");
    assert_eq!(app.view, View::Track);
    assert!(app.status.contains("Sample row"), "{}", app.status);
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

#[test]
fn j_k_and_a_count_g_go_by_slice_which_is_the_whole_entry_until_slices_come() {
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
/// after it, as the program does each frame. What's sent is said.
fn draw(
    app: &mut App,
    graphics: &mut Graphics,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, String) {
    let mut sent = Vec::new();
    graphics.use_window(WindowSize {
        columns: width,
        rows: height,
        width: width * 3,
        height: height * 6,
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
    // The ruler: the start's mark over its column, the second's time, and the end's mark.
    let ruler = row(&terminal, 2);
    assert_eq!(ruler.chars().nth(2), Some('▾'), "{ruler}");
    assert!(ruler.contains("╵1"), "{ruler}");
    assert_eq!(ruler.chars().nth(1 + (90_000 / FIT) as usize), Some('▾'));
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
    assert!(row(&terminal, 34).starts_with(" Cursor 0.021 s  frame 1000  slice 00"));
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
    for keys in ["z i", "z o", "z O", "z v", "z c", "g _", "] ]", "[ ["] {
        assert!(
            rows.iter().any(|row| row.trim_start().starts_with(keys)),
            "{keys}"
        );
    }
}
