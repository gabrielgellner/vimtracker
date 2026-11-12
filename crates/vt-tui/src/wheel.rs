//! The mouse wheel and the trackpad as a knob on the cursor's setting (#425): each tick is `=` or
//! `-` where the cursor is, wherever those keys adjust something, a locked selection included,
//! and with Shift `+` or `_`. Up and down and sideways are one knob, as a trackpad's fingers drift
//! from one to the other: the value follows the fingers, rising as they go up or right, as a level
//! bar fills, whichever way macOS scrolls the screen. Where the cursor's setting has a finer step
//! than `=`, its fine digits, the tempo's fine part or the sample page's frame, a tick takes a
//! part of that, the same for every tick of a slow or steady turn, so twice the swipe is twice the
//! change, and only a fast swipe takes more at a time, up to one of `=`'s steps. Where the pointer
//! is doesn't matter, and clicks and drags do nothing.
//!
//! One gesture is one undo step, as a Roto knob's touch to let-go is (decision 160): it ends once
//! the wheel pauses for [`GESTURE_PAUSE`], or at any key.
//!
//! It's off until turned on, on the audio page, as the terminal sends the program its mouse
//! only while it asks, and then keeps plain drags for itself no more: in kitty and Ghostty,
//! Shift and a drag still select text. The setting is kept for this machine in `wheel.ron`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};
use vt_model::selection::Field;

use crate::app::App;
use crate::input::bindings::Resolved;
use crate::input::keys::{Key, parse};
use crate::input::mode::{Mode, View};
use crate::project_page::{BPM_NUDGE, FINE_NUDGE, ProjectControl};
pub use crate::roto::GESTURE_PAUSE;

/// The setting's file, in the config folder beside `audio.ron`.
pub const SETTING_FILE: &str = "wheel.ron";

/// Asks the terminal for its mouse's buttons and wheel, as SGR reports: no motion, as nothing
/// uses it, and every button is let through to be ignored but the wheel.
pub const CAPTURE_ON: &str = "\x1b[?1000h\x1b[?1006h";

/// Gives the mouse back to the terminal, for selecting text.
pub const CAPTURE_OFF: &str = "\x1b[?1006l\x1b[?1000l";

/// The wheel's feel, all in one place for tuning by hand (decision 163).
pub mod feel {
    use std::time::Duration;

    /// How quickly the wheel's speed forgets a tick: its speed is how many ticks came in about
    /// the last this long, each counting less as it ages.
    pub const SPEED_TIME: Duration = Duration::from_millis(100);

    /// A speed at or below this, about 25 ticks a second held, is a slow or steady turn, every
    /// tick the least step, so the change is in proportion to the swipe. On a MacBook's trackpad
    /// in kitty a careful turn comes at 4 to 10 ticks a second.
    pub const SLOW: f64 = 3.0;

    /// A speed at or above this, about 75 ticks a second, is a sweep, a tick each the most step,
    /// one of `=`'s; kitty sends a fast swipe's ticks at the screen's 60 a second, with now and
    /// then two at once. Between the two, a tick's size rises in a straight line.
    pub const FAST: f64 = 8.0;

    /// The least step on a setting's fine digits: a 16th of `=`'s, a step of their first digit,
    /// so even a slow tick is seen to move.
    pub const FINE_LEAST: u64 = 16;

    /// The least step on the tempo, in hundredths of a BPM: a tenth.
    pub const TEMPO_LEAST: u64 = 10;

    /// The least step on the sample page, as a part of `=`'s pixel: a 16th, or a frame.
    pub const PIXEL_PARTS: u64 = 16;

    /// How long a turn's ticks must stop before a tick the other way is taken at once. Sooner
    /// than that, it's a trackpad's bounce.
    pub const TURN_PAUSE: Duration = Duration::from_millis(100);

    /// How many ticks the other way in a row are taken as a turn even with no pause: a quick,
    /// deliberate change of direction.
    pub const TURN_TICKS: u32 = 3;
}

/// Whether the wheel turns the cursor's setting, remembered for this machine rather than the
/// song.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelSetting {
    /// Off unless turned on, as the terminal's own drag to select text is lost while it's on
    /// (the author's decision of 2026-11-11, #425).
    pub on: bool,
    /// The value goes against the fingers rather than with them.
    pub reversed: bool,
}

impl WheelSetting {
    /// Where it's kept: `wheel.ron` in the config folder.
    pub fn path() -> Option<PathBuf> {
        crate::config::config_folder().map(|folder| folder.join(SETTING_FILE))
    }

    /// The setting kept at `path`, or off if there's none or it won't read.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keep it at `path` for next time.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = ron::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Whether macOS scrolls naturally, as System Settings' "Natural scrolling" says. It's on unless
/// it's been turned off, when the key is written.
#[cfg(target_os = "macos")]
pub fn natural_scrolling() -> bool {
    use objc2_foundation::{NSUserDefaults, ns_string};
    objc2::rc::autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = ns_string!("com.apple.swipescrolldirection");
        defaults.objectForKey(key).is_none() || defaults.boolForKey(key)
    })
}

#[cfg(not(target_os = "macos"))]
pub fn natural_scrolling() -> bool {
    false
}

/// Asks the terminal for the mouse's reports, or gives the mouse back.
pub fn capture(out: &mut impl Write, on: bool) -> std::io::Result<()> {
    out.write_all(if on { CAPTURE_ON } else { CAPTURE_OFF }.as_bytes())?;
    out.flush()
}

/// The wheel as the app holds it.
#[derive(Debug, Clone)]
pub struct Wheel {
    pub setting: WheelSetting,
    /// Where the setting is saved when it changes, or nowhere, as in tests.
    pub setting_path: Option<PathBuf>,
    /// How long the wheel stays still before a gesture's undo step closes.
    pub pause: Duration,
    /// Whether macOS scrolls naturally, the screen following the fingers, which turns the
    /// terminal's up into the fingers going down. Asked for as the program starts.
    pub natural: bool,
    gesture: Option<Gesture>,
}

impl Default for Wheel {
    fn default() -> Self {
        Self {
            setting: WheelSetting::default(),
            setting_path: None,
            pause: GESTURE_PAUSE,
            natural: false,
            gesture: None,
        }
    }
}

/// One turn of the wheel, from its first tick until it pauses.
#[derive(Debug, Clone)]
struct Gesture {
    /// The undo depth before its first tick.
    depth: usize,
    /// How many undo steps the selection's running total had before its first tick.
    adjusted: usize,
    /// Its last tick, taken or not, which the pause that ends it is counted from.
    last: Instant,
    /// Whether it's going up, and its last tick taken that way.
    up: bool,
    taken: Instant,
    /// How many ticks in a row have come another way and not been taken.
    turning: u32,
    /// Its speed: how many ticks came in about the last [`feel::SPEED_TIME`], each counting
    /// less as it ages, so it rises and falls smoothly rather than in whole ticks.
    speed: f64,
    /// What its ticks since the last frame owe the setting, made once each frame.
    owed: Option<Owed>,
}

impl Gesture {
    fn new(depth: usize, adjusted: usize, up: bool, now: Instant) -> Self {
        Self {
            depth,
            adjusted,
            last: now,
            up,
            taken: now,
            turning: 0,
            speed: 0.0,
            owed: None,
        }
    }

    /// Whether a tick this way is taken. One the way the turn is going always is, and adds to
    /// its speed. One the other way is a trackpad's bounce at the end of a fling, and is let go,
    /// unless the turn had paused for [`feel::TURN_PAUSE`] or it's the [`feel::TURN_TICKS`]th
    /// such tick in a row: then the turn goes the new way, from a standstill.
    fn take(&mut self, up: bool, now: Instant) -> bool {
        self.last = now;
        if up != self.up {
            self.turning += 1;
            let paused = now.saturating_duration_since(self.taken) >= feel::TURN_PAUSE;
            if !paused && self.turning < feel::TURN_TICKS {
                return false;
            }
            self.up = up;
            self.speed = 0.0;
        }
        self.turning = 0;
        let age = now.saturating_duration_since(self.taken).as_secs_f64();
        self.speed = self.speed * (-age / feel::SPEED_TIME.as_secs_f64()).exp() + 1.0;
        self.taken = now;
        true
    }
}

/// What a turn's ticks owe the setting, in its finest step, by the key that takes it.
#[derive(Debug, Clone, Copy)]
struct Owed {
    key: Key,
    finer: Option<Finer>,
    /// How many of the finest step. A frame makes a whole number of the least step and keeps
    /// the rest, so a speed between two sizes comes out between them.
    steps: f64,
    least: u64,
}

impl Owed {
    /// Takes the whole number of least steps owed, keeping the rest.
    fn take_whole(&mut self) -> u64 {
        let least = self.least.max(1);
        // A hair over, so a sum of exact sizes isn't a rounding error short of the next step.
        let whole = (self.steps / least as f64 + 1e-9).floor().max(0.0) as u64 * least;
        self.steps = (self.steps - whole as f64).max(0.0);
        whole
    }
}

/// How a tick is turned into the finest step the cursor's setting has, and back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finer {
    /// `=` is already the finest step.
    Already,
    /// Onto the track page's fine digits for the tick.
    TrackFine,
    /// Onto an FX value's fine digits in the phrase for the tick.
    PhraseFine(usize),
    /// Onto the tempo's fine part for the tick.
    TempoFine,
    /// Cmd-l and Cmd-h, the sample page's nudge by a frame, in place of `=`'s pixel.
    SampleFrame,
}

/// A setting's finer step: how to take it, and the least and most a tick takes of it, the most
/// being one of `=`'s steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FinerStep {
    finer: Finer,
    least: u64,
    most: u64,
}

/// How many of a setting's finest step a tick at this speed is worth: the least at or below
/// [`feel::SLOW`], the most at or above [`feel::FAST`], and in a straight line between, so a
/// turn speeds up evenly as it quickens.
pub fn tick_size(speed: f64, least: u64, most: u64) -> f64 {
    let least = least.max(1) as f64;
    let most = (most as f64).max(least);
    let rise = ((speed - feel::SLOW) / (feel::FAST - feel::SLOW)).clamp(0.0, 1.0);
    least + (most - least) * rise
}

impl App {
    /// A mouse report from the terminal. Only the wheel acts: the fingers going up or right raise
    /// the setting under the cursor, and down or left lower it, or the other way if the setting
    /// says so, and Shift takes the large step. Nothing happens while the setting is off, which
    /// it is unless turned on, as the terminal then sends nothing anyway.
    pub fn mouse_event(&mut self, event: MouseEvent, now: Instant) {
        if !self.wheel.setting.on {
            return;
        }
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let raised = match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollRight => true,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollLeft => false,
            _ => return,
        };
        // Natural scrolling sends the fingers' up as the terminal's down.
        let up = raised != (self.wheel.natural != self.wheel.setting.reversed);
        self.turn_wheel(up, shift, now);
    }

    /// One tick of the wheel, owed to the cursor's setting as the key that adjusts it by the
    /// finest step, or by the large one, and made at the next frame.
    fn turn_wheel(&mut self, up: bool, large: bool, now: Instant) {
        self.now = now;
        if self
            .wheel
            .gesture
            .as_ref()
            .is_some_and(|gesture| now.saturating_duration_since(gesture.last) >= self.wheel.pause)
        {
            self.end_wheel_gesture();
        }
        let finer = if large { None } else { self.finer_here() };
        let key = match (finer.map(|step| step.finer), large, up) {
            (Some(Finer::SampleFrame), _, true) => "D-l",
            (Some(Finer::SampleFrame), _, false) => "D-h",
            (_, true, true) => "+",
            (_, true, false) => "_",
            (_, false, true) => "=",
            (_, false, false) => "-",
        };
        let Ok(key) = parse(key) else {
            return;
        };
        if !self.wheel_adjusts(key) {
            return;
        }
        let (depth, adjusted) = (self.history.undo_depth(), self.adjustments.steps());
        let gesture = self
            .wheel
            .gesture
            .get_or_insert_with(|| Gesture::new(depth, adjusted, up, now));
        if !gesture.take(up, now) {
            return;
        }
        let (steps, least) = finer.map_or((1.0, 1), |step| {
            (tick_size(gesture.speed, step.least, step.most), step.least)
        });
        let finer = finer.map(|step| step.finer);
        match &mut gesture.owed {
            Some(owed) if owed.key == key && owed.finer == finer => owed.steps += steps,
            owed => {
                // Another key, as Shift went down or the turn changed direction: what the
                // last one owed is made first, and its fraction let go.
                let before = owed.replace(Owed {
                    key,
                    finer,
                    steps,
                    least,
                });
                if let Some(mut before) = before {
                    self.make_wheel_change(before.key, before.finer, before.take_whole());
                }
            }
        }
    }

    /// Makes what the wheel's ticks since the last frame owe the setting, as one press.
    fn make_owed_wheel_change(&mut self) {
        let Some(owed) = self
            .wheel
            .gesture
            .as_mut()
            .and_then(|gesture| gesture.owed.as_mut())
        else {
            return;
        };
        let (key, finer, count) = (owed.key, owed.finer, owed.take_whole());
        self.make_wheel_change(key, finer, count);
    }

    /// Presses the key `count` times over, as a count before it, on the setting's fine part if
    /// it's taken there.
    fn make_wheel_change(&mut self, key: Key, finer: Option<Finer>, count: u64) {
        if count == 0 {
            return;
        }
        // The wheel isn't a key, so `.` still repeats the last key's change.
        let last_change = self.last_change.clone();
        if let Some(finer) = finer {
            self.step_onto_fine(finer, true);
        }
        self.count = (count > 1).then_some(count as usize);
        self.press(key, self.now);
        self.count = None;
        if let Some(finer) = finer {
            self.step_onto_fine(finer, false);
        }
        self.last_change = last_change;
    }

    /// Ends the wheel's gesture, if one is going, making what it owes and its ticks one undo
    /// step. A key ends it.
    pub fn end_wheel_gesture(&mut self) {
        self.make_owed_wheel_change();
        let Some(gesture) = self.wheel.gesture.take() else {
            return;
        };
        if self.history.undo_depth() > gesture.depth + 1 {
            self.history.squash_to(gesture.depth);
        }
        if self.mode.is_visual() {
            self.adjustments.join_from(gesture.adjusted);
        }
    }

    /// Each frame: makes what the wheel's ticks since the last owe, all at once rather than a
    /// jump a tick, and ends its gesture once it has paused.
    pub(crate) fn wheel_frame(&mut self, now: Instant) {
        self.make_owed_wheel_change();
        let paused =
            self.wheel.gesture.as_ref().is_some_and(|gesture| {
                now.saturating_duration_since(gesture.last) >= self.wheel.pause
            });
        if paused {
            self.end_wheel_gesture();
        }
    }

    /// Turns the value with the fingers or against them, keeping the choice for this machine.
    pub fn set_wheel_reversed(&mut self, reversed: bool) {
        self.wheel.setting.reversed = reversed;
        self.status = if reversed {
            "The wheel turns against the fingers"
        } else {
            "The wheel turns with the fingers"
        }
        .to_owned();
        self.save_wheel_setting();
    }

    /// Keeps the wheel's setting for next time, saying so if it can't be.
    fn save_wheel_setting(&mut self) {
        if let Some(path) = self.wheel.setting_path.clone()
            && let Err(error) = self.wheel.setting.save_to(&path)
        {
            self.status = format!("{}: couldn't be saved: {error}", path.display());
        }
    }

    /// Whether the terminal should be sending the mouse.
    pub fn wants_mouse(&self) -> bool {
        self.wheel.setting.on
    }

    /// Turns the wheel on or off, keeping the choice for this machine.
    pub fn set_wheel(&mut self, on: bool) {
        if self.wheel.setting.on == on {
            return;
        }
        self.end_wheel_gesture();
        self.wheel.setting.on = on;
        self.status = if on {
            "Wheel on: scrolling turns the setting under the cursor, and Shift-drag selects text"
        } else {
            "Wheel off: the terminal has its mouse back"
        }
        .to_owned();
        self.save_wheel_setting();
    }

    /// Whether this key adjusts something here, so a tick may press it: with no keys typed before
    /// it, in a mode that binds it, or in the shape panel, which reads `=` and its kin itself.
    fn wheel_adjusts(&self, key: Key) -> bool {
        if !self.pending.is_empty() {
            return false;
        }
        match self.mode {
            Mode::Shape => true,
            Mode::OperatorPending => false,
            _ => matches!(
                self.table.resolve(self.key_scope(), &[key]),
                Resolved::Command(_)
            ),
        }
    }

    /// Where the cursor's setting has a step finer than `=`'s, how to take it, and the least and
    /// most a tick takes of it. The least is one a slow tick is seen to move: a 16th of `=`'s
    /// step on a value's fine digits, a tenth of a BPM on the tempo, and a 16th of a pixel on the
    /// sample page; with the cursor on the fine part itself, its own `=`. A selection moves by
    /// `=`'s step, as its values are whole.
    fn finer_here(&self) -> Option<FinerStep> {
        if self.mode.is_visual() || self.mode == Mode::Shape {
            return None;
        }
        // A fine part is a 256th of its setting's step.
        const FINE_STEPS: u64 = 256;
        let step = |finer, least, most| Some(FinerStep { finer, least, most });
        match (self.key_scope(), self.view) {
            (Mode::Track, View::Track) => {
                let has_fine = self.track_page.has_fine_here(self.track_here());
                match (has_fine, self.track_page.fine) {
                    (true, true) => step(Finer::Already, 1, FINE_STEPS),
                    (true, false) => step(Finer::TrackFine, feel::FINE_LEAST, FINE_STEPS),
                    _ => None,
                }
            }
            (Mode::Normal, View::Phrase) => {
                let (Field::FxValue(slot) | Field::FxFine(slot)) = self.cursor.field else {
                    return None;
                };
                let sweeps = self
                    .current_slot(slot)
                    .and_then(|fx| fx.command)
                    .is_some_and(|command| command.has_fine());
                match self.cursor.field {
                    Field::FxFine(_) if sweeps => step(Finer::Already, 1, FINE_STEPS),
                    Field::FxValue(_) if sweeps => {
                        step(Finer::PhraseFine(slot), feel::FINE_LEAST, FINE_STEPS)
                    }
                    _ => None,
                }
            }
            (Mode::Project, _) if self.project_control() == ProjectControl::Bpm => {
                let most = (BPM_NUDGE / FINE_NUDGE) as u64;
                if self.project_fine {
                    step(Finer::Already, 1, most)
                } else {
                    step(Finer::TempoFine, feel::TEMPO_LEAST, most)
                }
            }
            (Mode::Sample | Mode::SampleVisual, _) if self.sample_page.setting.is_none() => {
                // A pixel's frames, as `=` nudges a point by a pixel.
                let zoom = self.sample_page.zoom.max(1);
                let pixel = match self.sample_page.cell_width {
                    Some(cell) if cell > 1 => zoom / cell as u64,
                    _ => zoom,
                }
                .max(1);
                step(
                    Finer::SampleFrame,
                    (pixel / feel::PIXEL_PARTS).max(1),
                    pixel,
                )
            }
            _ => None,
        }
    }

    /// Puts the cursor on its setting's fine part for a tick, or back off it after.
    fn step_onto_fine(&mut self, finer: Finer, onto: bool) {
        match finer {
            Finer::TrackFine => self.track_page.fine = onto,
            Finer::PhraseFine(slot) => {
                self.cursor.field = if onto {
                    Field::FxFine(slot)
                } else {
                    Field::FxValue(slot)
                };
            }
            Finer::TempoFine => self.project_fine = onto,
            Finer::Already | Finer::SampleFrame => {}
        }
    }
}
