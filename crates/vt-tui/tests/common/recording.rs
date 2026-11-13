//! The recording tests' own clock: an app of two tracks to record on, and keys and frames given
//! at times counted in steps from the start.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::keys::Key;

use super::press;

/// Two tracks: 16 empty rows on the first, and 4 on the second, where the cursor is.
pub fn recording_app(rows: Vec<Step>) -> (App, Realtime) {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(rows)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 1);
    (app, realtime)
}

/// Frames to a step at the default 120 BPM.
pub const FRAMES_PER_STEP: f64 = 6_000.0;

/// Seconds to a step at the default 120 BPM.
pub const SECONDS_PER_STEP: f64 = 0.125;

/// The time on the recording tests' own clock this many steps from the start. The app is given
/// these times for its frames and its keys, so where a note lands and how long it lasts come
/// from the frames played, never from how fast the test happens to run (#292).
pub fn time_at(at: f64) -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now) + Duration::from_secs_f64(at.max(0.0) * SECONDS_PER_STEP)
}

/// Keys pressed, tapped or held at a time on the recording tests' clock.
pub fn press_at(app: &mut App, at: f64, keys: &str) {
    for c in keys.chars() {
        app.key(Key::char(c), time_at(at));
    }
}

pub fn tap_at(app: &mut App, at: f64, keys: &str) {
    for c in keys.chars() {
        app.key(Key::char(c), time_at(at));
        app.key_released(Key::char(c), time_at(at));
    }
}

pub fn special_at(app: &mut App, at: f64, code: KeyCode) {
    app.key(Key::new(code), time_at(at));
}

/// Play from the start, as `Shift-Tab` does, and come back with the clock at 0.
pub fn play_from_the_top(app: &mut App, realtime: &mut Realtime) -> f64 {
    special_at(app, 0.0, KeyCode::BackTab);
    let mut out = vec![0.0; 2];
    realtime.process(&mut out);
    let at = 1.0 / FRAMES_PER_STEP;
    app.tick_at(time_at(at));
    at
}

/// Run the audio side on to this many steps from the start, and give the app the frame at the
/// time it's got to.
pub fn play_to(app: &mut App, realtime: &mut Realtime, at: &mut f64, to: f64) {
    let frames = ((to - *at) * FRAMES_PER_STEP).round() as usize;
    let mut out = vec![0.0; frames * 2];
    realtime.process(&mut out);
    *at += frames as f64 / FRAMES_PER_STEP;
    app.tick_at(time_at(*at));
}

pub fn rows_of(app: &App, track: usize) -> Vec<Step> {
    app.song().tracks[track].phrases[&0].steps().to_vec()
}
