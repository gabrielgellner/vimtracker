//! The mouse wheel and the trackpad as a knob on the cursor's setting.
//!
//! Each tick is `=` or `-` where the cursor is, wherever those keys adjust something, a locked
//! selection included. Up and down and sideways are one knob, as a trackpad's fingers drift from
//! one to the other, and alike: the value follows the fingers, rising as they go up or right, as a
//! level bar fills, whichever way macOS scrolls the screen, and an up tick counts for as much of
//! the fingers' way as the sideways ones the same swipe sends, about two, as the terminal's rows
//! are taller than its columns are wide. Speed alone sizes a tick, with no Shift: where the
//! cursor's setting has a finer step than `=`, its fine digits, the tempo's fine part or the
//! sample page's frame, a slow or steady turn's tick is a [`feel::PARTS`]th of `=`'s step, and a
//! faster one's more, in a straight line up to [`feel::MOST`] of `=`'s steps. A list or a toggle
//! goes by its places instead, one at a time with a detent between them, and stops at either end,
//! so up or right puts a toggle on and a fling's momentum only pushes on the same way. Where the
//! pointer is doesn't matter, and clicks and drags do nothing.
//!
//! One gesture is one undo step, as a Roto knob's touch to let-go is: it ends once the wheel pauses
//! for [`GESTURE_PAUSE`], or for [`feel::MOMENTUM`] after a fling, or at any key.
//!
//! It's off until turned on, on the audio page, as the terminal sends the program its mouse
//! only while it asks, and then keeps plain drags for itself no more: in kitty and Ghostty,
//! Shift and a drag still select text. The setting is kept for this machine in `wheel.ron`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};
use vt_model::selection::Field;

use crate::app::App;
use crate::controls::{self, ParamView};
use crate::input::bindings::Resolved;
use crate::input::keys::{Key, parse};
use crate::input::mode::{Mode, View};
use crate::pages::audio::AudioRow;
use crate::pages::project::{BPM_NUDGE, FINE_NUDGE, ProjectControl};
use crate::roto::GESTURE_PAUSE;
use crate::widgets::param::edit;

/// The setting's file, in the config folder beside `audio.ron`.
pub const SETTING_FILE: &str = "wheel.ron";

/// Asks the terminal for its mouse's buttons and wheel, as SGR reports: no motion, as nothing
/// uses it, and every button is let through to be ignored but the wheel.
pub(crate) const CAPTURE_ON: &str = "\x1b[?1000h\x1b[?1006h";

/// Gives the mouse back to the terminal, for selecting text.
pub(crate) const CAPTURE_OFF: &str = "\x1b[?1006l\x1b[?1000l";

/// The wheel's feel, all in one place for tuning by hand.
pub mod feel {
    use std::time::Duration;

    /// How quickly the wheel's speed forgets a tick: its speed is how many sideways ticks, or
    /// their worth, came in about the last this long, each counting less as it ages.
    pub(crate) const SPEED_TIME: Duration = Duration::from_millis(100);

    /// A speed at or below this, about 25 sideways ticks a second held, or 12 up, is a slow or
    /// steady turn, every tick the least step, so the change is in proportion to the swipe. On a
    /// MacBook's trackpad in kitty a careful turn comes at 4 to 10 up ticks a second.
    pub const SLOW: f64 = 3.0;

    /// A speed at or above this, about 75 sideways ticks a second, or 37 up, is a sweep, a tick
    /// each the most step; kitty sends a fast swipe's ticks at the screen's 60 a second, with now
    /// and then two at once. Between the two, a tick's size rises in a straight line, so a turn
    /// speeds up evenly as it quickens.
    pub const FAST: f64 = 8.0;

    /// The least step, at [`SLOW`] and below, as a share of `=`'s, where the setting has a finer
    /// step: a 16th, a step of a value's first fine digit, and a 16th of a pixel on the sample
    /// page, so even a slow tick is seen to move. A setting with no finer step takes its `=`.
    pub const PARTS: u64 = 16;

    /// The most step, at [`FAST`] and above, in `=`'s steps: one, so a sweep is `=` a tick. On a
    /// fine part itself, a step of its value.
    pub const MOST: u64 = 1;

    /// The least step on the tempo, in hundredths of a BPM: a tenth.
    pub(crate) const TEMPO_LEAST: u64 = 10;

    /// What an up or down tick counts for, in sideways ones, where the terminal's cell isn't
    /// known: kitty sends a tick for each column or row the scroll crosses, and a row is about
    /// twice as high as a column is wide, so the same swipe sends half the ticks up as sideways.
    /// An up tick counts for two in the speed, a list's detent and, where the setting has a finer
    /// step, its size, so both axes feel as sideways did when every tick counted whole; with no
    /// finer step, a tick either way is one `=`. Once the screen's drawn, its own cell's height
    /// over its width is used.
    pub(crate) const UPRIGHT: f64 = 2.0;

    /// How long a turn's ticks must stop before a tick the other way is taken at once. Sooner
    /// than that, it's a trackpad's bounce.
    pub const TURN_PAUSE: Duration = Duration::from_millis(100);

    /// How many ticks the other way in a row are taken as a turn even with no pause: a quick,
    /// deliberate change of direction.
    pub(crate) const TURN_TICKS: u32 = 3;

    /// A speed at or above this, about 50 sideways ticks a second, or 25 up, past a careful
    /// turn, is a fling: macOS goes on sending ticks after the fingers lift, slowing over about a
    /// second, and may bounce back a tick or two at the end. A list or a toggle moves a place
    /// more as a turn reaches it, if it's turned [`DETENT_TICKS`] since its last, and none in its
    /// momentum, so a flick moves it one or two places; a new fling, its speed rising by this
    /// much again from the slowest the momentum had come to, moves it a place on.
    pub const FLING: f64 = 5.5;

    /// How long the wheel must be still after a fling, or on a list or a toggle, before a tick
    /// the other way is taken at once, rather than as a bounce, and before its gesture ends:
    /// longer than the gaps of momentum's slowest ticks.
    pub const MOMENTUM: Duration = Duration::from_millis(600);

    /// How many sideways ticks, or their worth, a list or a toggle takes to move a place after
    /// the last, in a slow turn: six, or three up. The first tick of a turn moves it at once.
    pub const DETENT_TICKS: f64 = 6.0;

    /// The least time between two places of a list or a toggle, in a turn quicker than
    /// [`DETENT_TICKS`] in this long.
    pub const DETENT_TIME: Duration = Duration::from_millis(250);
}

/// Whether the wheel turns the cursor's setting, remembered for this machine rather than the
/// song.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelSetting {
    /// Off unless turned on, as the terminal's own drag to select text is lost while it's on
    /// (the author's decision of 2026-11-11).
    pub on: bool,
    /// The value goes against the fingers rather than with them.
    pub reversed: bool,
}

impl WheelSetting {
    /// Where it's kept: `wheel.ron` in the config folder.
    pub fn path() -> Option<PathBuf> {
        crate::services::config::config_folder().map(|folder| folder.join(SETTING_FILE))
    }

    /// The setting kept at `path`, or off if there's none or it won't read.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keep it at `path` for next time, on one line, whole or not at all.
    pub(crate) fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let text = ron::to_string(self).map_err(std::io::Error::other)?;
        Ok(vt_model::ron_file::write_atomic(path, text)?)
    }
}

/// Whether macOS scrolls naturally, as System Settings' "Natural scrolling" says. It's on unless
/// it's been turned off, when the key is written.
#[cfg(target_os = "macos")]
pub(crate) fn natural_scrolling() -> bool {
    use objc2_foundation::{NSUserDefaults, ns_string};
    objc2::rc::autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = ns_string!("com.apple.swipescrolldirection");
        defaults.objectForKey(key).is_none() || defaults.boolForKey(key)
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn natural_scrolling() -> bool {
    false
}

/// Asks the terminal for the mouse's reports, or gives the mouse back.
pub(crate) fn capture(out: &mut impl Write, on: bool) -> std::io::Result<()> {
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
    pub(crate) pause: Duration,
    /// Whether macOS scrolls naturally, the screen following the fingers, which turns the
    /// terminal's up into the fingers going down. Asked for as the program starts.
    pub natural: bool,
    /// The terminal's cell in pixels, across and down, once the screen's been drawn, for what an
    /// up or down tick counts for.
    pub cell: Option<(usize, usize)>,
    gesture: Option<Gesture>,
}

impl Default for Wheel {
    fn default() -> Self {
        Self {
            setting: WheelSetting::default(),
            setting_path: None,
            pause: GESTURE_PAUSE,
            natural: false,
            cell: None,
            gesture: None,
        }
    }
}

impl Wheel {
    /// What an up or down tick counts for, in sideways ones: the cell's height over its width, as
    /// the terminal sends a tick for each column or row the fingers cross.
    pub fn upright(&self) -> f64 {
        match self.cell {
            Some((across, down)) if across > 0 && down > 0 => {
                (down as f64 / across as f64).clamp(1.0, 4.0)
            }
            _ => feel::UPRIGHT,
        }
    }

    /// Turns the wheel on or off, keeping the choice for this machine and saying so on the status
    /// line, as the audio page's `Wheel` row does. A turn under way is left to end as it pauses:
    /// a key reaching the row has ended it already, and once the wheel is off no tick adds to it.
    pub(crate) fn switch(&mut self, on: bool, status: &mut String) {
        if self.setting.on == on {
            return;
        }
        self.setting.on = on;
        *status = if on {
            "Wheel on: scrolling turns the setting under the cursor, and Shift-drag selects text"
        } else {
            "Wheel off: the terminal has its mouse back"
        }
        .to_owned();
        self.save_setting(status);
    }

    /// Turns the value with the fingers or against them, keeping the choice for this machine.
    pub(crate) fn reverse(&mut self, reversed: bool, status: &mut String) {
        self.setting.reversed = reversed;
        *status = if reversed {
            "The wheel turns against the fingers"
        } else {
            "The wheel turns with the fingers"
        }
        .to_owned();
        self.save_setting(status);
    }

    /// Keeps the wheel's setting for next time, saying so if it can't be.
    fn save_setting(&self, status: &mut String) {
        if let Some(path) = &self.setting_path
            && let Err(error) = self.setting.save_to(path)
        {
            *status = format!("{}: couldn't be saved: {error}", path.display());
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
    /// How many sideways ticks came in about the last [`feel::SPEED_TIME`], each counting less as
    /// it ages, so it rises and falls smoothly rather than in whole ticks, and an up one for
    /// more, as [`Wheel::upright`] says; and its speed from that, less the half of its last tick
    /// that's over a sideways one's worth, as a count read as a tick comes is over by half the
    /// tick, so an up tick is as quick as the sideways ones the same swipe sends, and sideways
    /// ones count as they did when every tick counted whole.
    ticks: f64,
    speed: f64,
    /// Whether this way's ticks have reached [`feel::FLING`], so momentum follows them, and the
    /// quickest they've come since and the slowest since that.
    flung: bool,
    peak: f64,
    trough: f64,
    /// Whether its last tick was on a list or a toggle, which waits out momentum as after a
    /// fling.
    places: bool,
    /// When a list or a toggle last moved a place this way, and how many ticks have come since.
    placed: Option<Instant>,
    notches: f64,
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
            ticks: 0.0,
            speed: 0.0,
            flung: false,
            peak: 0.0,
            trough: 0.0,
            places: false,
            placed: None,
            notches: 0.0,
            owed: None,
        }
    }

    /// Whether momentum may still be coming: after a fling, and on a list or a toggle, where a
    /// slower swipe's momentum would move it too.
    fn coasting(&self) -> bool {
        self.flung || self.places
    }

    /// How long the wheel must be still for the gesture to end: `pause`, or while momentum may
    /// be coming, long enough for it and its bounce to be part of it.
    fn rest(&self, pause: Duration) -> Duration {
        if self.coasting() {
            pause.max(feel::MOMENTUM)
        } else {
            pause
        }
    }

    /// Takes a tick this way, worth `weight` sideways ones, or lets it go. One the way the
    /// turn is going is taken, and adds to its speed. One the other way is a trackpad's bounce,
    /// and is let go, unless the turn had paused for [`feel::TURN_PAUSE`], or for
    /// [`feel::MOMENTUM`] after a fling or on a list or a toggle, or it's the
    /// [`feel::TURN_TICKS`]th such tick in a row: then the turn goes the new way, from a
    /// standstill. For a tick taken, whether it's the one that makes the turn a fling.
    fn take(&mut self, up: bool, weight: f64, now: Instant) -> Option<bool> {
        self.last = now;
        if up != self.up {
            self.turning += 1;
            let still = now.saturating_duration_since(self.taken);
            let wait = if self.coasting() {
                feel::MOMENTUM
            } else {
                feel::TURN_PAUSE
            };
            if still < wait && self.turning < feel::TURN_TICKS {
                return None;
            }
            self.up = up;
            self.ticks = 0.0;
            self.start_over();
        }
        self.turning = 0;
        let age = now.saturating_duration_since(self.taken).as_secs_f64();
        self.ticks = self.ticks * (-age / feel::SPEED_TIME.as_secs_f64()).exp() + weight;
        self.speed = self.ticks - (weight - 1.0) / 2.0;
        self.taken = now;
        self.trough = self.trough.min(self.speed);
        if self.flung && self.speed >= feel::FLING.max(self.trough + feel::FLING) {
            // The fingers again, flinging as the momentum slows: a stroke of its own.
            self.start_over();
        } else if self.speed > self.peak {
            // Still quickening.
            (self.peak, self.trough) = (self.speed, self.speed);
        }
        let flinging = !self.flung && self.speed >= feel::FLING;
        if flinging {
            self.flung = true;
            (self.peak, self.trough) = (self.speed, self.speed);
        }
        Some(flinging)
    }

    /// As at a turn's first tick: no fling, and a list moves at once.
    fn start_over(&mut self) {
        self.flung = false;
        self.placed = None;
        self.notches = 0.0;
    }

    /// Whether a tick taken on a list or a toggle, worth `weight`, moves it a place: the turn's
    /// first does, and then one each [`feel::DETENT_TICKS`] and no sooner than
    /// [`feel::DETENT_TIME`] after the last, or as the tick that makes the turn a fling, with no
    /// wait; its momentum moves none.
    fn detent(&mut self, weight: f64, flinging: bool, now: Instant) -> bool {
        if self.flung && !flinging {
            return false;
        }
        self.notches += weight;
        let due = match self.placed {
            None => true,
            Some(placed) => {
                // A hair under, so up ticks of a cell's worth that add up to it aren't a rounding
                // error short.
                self.notches >= feel::DETENT_TICKS - 1e-9
                    && (flinging || now.saturating_duration_since(placed) >= feel::DETENT_TIME)
            }
        };
        if due {
            self.placed = Some(now);
            self.notches = 0.0;
        }
        due
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
    /// Places of a list or a toggle, `up` or down, made only while there's a place that way to
    /// go to.
    places: bool,
    up: bool,
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

/// A setting's finer step: how to take it, and the least and the most a tick takes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FinerStep {
    finer: Finer,
    least: u64,
    most: u64,
}

/// How many of a setting's finest step a sideways tick at this speed is worth: the least at or
/// below [`feel::SLOW`], the most at or above [`feel::FAST`], and in a straight line between, so
/// a turn speeds up evenly as it quickens.
pub fn tick_size(speed: f64, least: u64, most: u64) -> f64 {
    let least = least.max(1) as f64;
    let most = (most as f64).max(least);
    let rise = ((speed - feel::SLOW) / (feel::FAST - feel::SLOW)).clamp(0.0, 1.0);
    least + (most - least) * rise
}

/// A setting that's one of a few places, a list's choices or off and on, which the wheel moves a
/// place at a time, stopping at either end rather than going round: whether `=` has a place to
/// go to, and `-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Places {
    pub up: bool,
    pub down: bool,
}

impl Places {
    /// The `at`th of `count`, or none of them, which either key takes to one.
    pub(crate) fn of(at: Option<usize>, count: usize) -> Self {
        match at {
            Some(at) => Self {
                up: at + 1 < count,
                down: at > 0,
            },
            None => Self {
                up: count > 0,
                down: count > 0,
            },
        }
    }

    /// Off or on.
    pub(crate) fn switch(on: bool) -> Self {
        Self { up: !on, down: on }
    }

    /// A described setting's, if it's a toggle or a list: where `=` and `-` would take it if
    /// it didn't go round.
    pub(crate) fn of_view(view: &ParamView) -> Option<Self> {
        if !view.toggle && view.choices.is_empty() {
            return None;
        }
        let stopping = ParamView {
            wraps: false,
            ..view.clone()
        };
        let moves = |up| edit::stepped(&stopping, up, false, 1) != view.value;
        Some(Self {
            up: moves(true),
            down: moves(false),
        })
    }

    fn goes(self, up: bool) -> bool {
        if up { self.up } else { self.down }
    }
}

impl App {
    /// A mouse report from the terminal. Only the wheel acts: the fingers going up or right raise
    /// the setting under the cursor, and down or left lower it, or the other way if the setting
    /// says so, an up or down tick counting for as much of the fingers' way as the sideways ones
    /// the same swipe sends. Shift changes nothing, as speed sizes a tick. Nothing happens while the setting is off, which it is
    /// unless turned on, as the terminal then sends nothing anyway.
    pub fn mouse_event(&mut self, event: MouseEvent, now: Instant) {
        if !self.wheel.setting.on {
            return;
        }
        let (raised, sideways) = match event.kind {
            MouseEventKind::ScrollUp => (true, false),
            MouseEventKind::ScrollRight => (true, true),
            MouseEventKind::ScrollDown => (false, false),
            MouseEventKind::ScrollLeft => (false, true),
            _ => return,
        };
        // Natural scrolling sends the fingers' up as the terminal's down.
        let up = raised != (self.wheel.natural != self.wheel.setting.reversed);
        let weight = if sideways { 1.0 } else { self.wheel.upright() };
        self.turn_wheel(up, weight, now);
    }

    /// One tick of the wheel, worth `weight` sideways ones, owed to the cursor's setting
    /// as the key that adjusts it by its finest step and made at the next frame: a part of that
    /// step sized by the turn's speed, or a list's or a toggle's next place, if its detent says.
    fn turn_wheel(&mut self, up: bool, weight: f64, now: Instant) {
        self.services.now = now;
        let pause = self.wheel.pause;
        if self.wheel.gesture.as_ref().is_some_and(|gesture| {
            now.saturating_duration_since(gesture.last) >= gesture.rest(pause)
        }) {
            self.end_wheel_gesture();
        }
        let finer = self.finer_here();
        let key = match (finer.map(|step| step.finer), up) {
            (Some(Finer::SampleFrame), true) => "D-l",
            (Some(Finer::SampleFrame), false) => "D-h",
            (_, true) => "=",
            (_, false) => "-",
        };
        let Ok(key) = parse(key) else {
            return;
        };
        if !self.wheel_adjusts(key) {
            return;
        }
        // A list or a toggle has no finer step.
        let places = finer.is_none() && self.places_here().is_some();
        let (depth, adjusted) = (
            self.services.history.undo_depth(),
            self.grammar.adjustments.steps(),
        );
        let gesture = self
            .wheel
            .gesture
            .get_or_insert_with(|| Gesture::new(depth, adjusted, up, now));
        gesture.places = places;
        let Some(flinging) = gesture.take(up, weight, now) else {
            return;
        };
        let (steps, least) = match finer {
            _ if places => {
                if !gesture.detent(weight, flinging, now) {
                    return;
                }
                (1.0, 1)
            }
            Some(step) => (
                tick_size(gesture.speed, step.least, step.most) * weight,
                step.least,
            ),
            // With no finer step, a tick is a place whichever way it comes, so a slow one is
            // never two; its weight still counts in the speed that sizes it.
            None => (tick_size(gesture.speed, 1, feel::MOST), 1),
        };
        let finer = finer.map(|step| step.finer);
        match &mut gesture.owed {
            Some(owed) if owed.key == key && owed.finer == finer && owed.places == places => {
                owed.steps += steps;
            }
            owed => {
                // Another key, as the turn changed direction or the cursor's setting changed:
                // what the last one owed is made first, and its fraction let go.
                let before = owed.replace(Owed {
                    key,
                    finer,
                    steps,
                    least,
                    places,
                    up,
                });
                if let Some(mut before) = before {
                    let count = before.take_whole();
                    self.make_wheel_change(&before, count);
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
        let count = owed.take_whole();
        let owed = *owed;
        self.make_wheel_change(&owed, count);
    }

    /// Presses the owed key `count` times over, as a count before it, on the setting's fine part
    /// if it's taken there. A list or a toggle moves a place, if it has one that way, so it
    /// never goes round.
    fn make_wheel_change(&mut self, owed: &Owed, count: u64) {
        let (key, finer) = (owed.key, owed.finer);
        let count = match owed.places {
            true if self
                .places_here()
                .is_some_and(|places| places.goes(owed.up)) =>
            {
                count.min(1)
            }
            true => 0,
            false => count,
        };
        if count == 0 {
            return;
        }
        // The wheel isn't a key, so `.` still repeats the last key's change.
        let last_change = self.grammar.last_change.clone();
        if let Some(finer) = finer {
            self.step_onto_fine(finer, true);
        }
        self.modes.count = (count > 1).then_some(count as usize);
        self.press(key, self.services.now);
        self.modes.count = None;
        if let Some(finer) = finer {
            self.step_onto_fine(finer, false);
        }
        self.grammar.last_change = last_change;
    }

    /// Ends the wheel's gesture, if one is going, making what it owes and its ticks one undo
    /// step. A key ends it.
    pub(crate) fn end_wheel_gesture(&mut self) {
        self.make_owed_wheel_change();
        let Some(gesture) = self.wheel.gesture.take() else {
            return;
        };
        if self.services.history.undo_depth() > gesture.depth + 1 {
            self.services.history.squash_to(gesture.depth);
        }
        if self.modes.mode.is_visual() {
            self.grammar.adjustments.join_from(gesture.adjusted);
        }
    }

    /// Each frame: makes what the wheel's ticks since the last owe, all at once rather than a
    /// jump a tick, and ends its gesture once it has paused.
    pub(crate) fn wheel_frame(&mut self, now: Instant) {
        self.make_owed_wheel_change();
        let pause = self.wheel.pause;
        let paused = self.wheel.gesture.as_ref().is_some_and(|gesture| {
            now.saturating_duration_since(gesture.last) >= gesture.rest(pause)
        });
        if paused {
            self.end_wheel_gesture();
        }
    }

    /// Whether the terminal should be sending the mouse.
    pub fn wants_mouse(&self) -> bool {
        self.wheel.setting.on
    }

    /// Whether this key adjusts something here, so a tick may press it: with no keys typed before
    /// it, in a mode that binds it, or in the shape panel, which reads `=` and its kin itself.
    /// The wheel's own `Direction` is left to the keys, as turning it would turn the wheel's
    /// next tick round.
    fn wheel_adjusts(&self, key: Key) -> bool {
        if !self.grammar.pending.is_empty() {
            return false;
        }
        if self.key_scope() == Mode::Audio && self.audio_page.row_here() == AudioRow::WheelDirection
        {
            return false;
        }
        match self.modes.mode {
            Mode::Shape | Mode::Mapping => true,
            Mode::OperatorPending => false,
            _ => matches!(
                self.table.resolve(self.key_scope(), &[key]),
                Resolved::Command(_)
            ),
        }
    }

    /// Where the cursor's setting has a step finer than `=`'s, how to take it, and the least and
    /// the most a tick takes of it. The least is one a slow tick is seen to move: a 16th of `=`'s
    /// step on a value's fine digits, a tenth of a BPM on the tempo, and a 16th of a pixel on the
    /// sample page; with the cursor on the fine part itself, its own `=`. The most is
    /// [`feel::MOST`] of `=`'s steps, or on a fine part itself, of its value's. A selection moves
    /// by `=`'s step, as its values are whole.
    fn finer_here(&self) -> Option<FinerStep> {
        if self.modes.mode.is_visual() || self.modes.mode == Mode::Shape {
            return None;
        }
        // A fine part is a 256th of its setting's step. On the fine part itself a slow tick is
        // its own `=`, and a tick tops out at the value's.
        const FINE_STEPS: u64 = 256;
        const FINE_LEAST: u64 = FINE_STEPS / feel::PARTS;
        const FINE_MOST: u64 = FINE_STEPS * feel::MOST;
        let step = |finer, least, most| Some(FinerStep { finer, least, most });
        match (self.key_scope(), self.modes.view) {
            (Mode::Track, View::Track) | (Mode::Gen, View::Gen) => {
                let has_fine = self.rack().has_fine_here(self.services.track_here());
                match (has_fine, self.rack().fine) {
                    (true, true) => step(Finer::Already, 1, FINE_MOST),
                    (true, false) => step(Finer::TrackFine, FINE_LEAST, FINE_MOST),
                    _ => None,
                }
            }
            (Mode::Normal, View::Phrase) => {
                let (Field::FxValue(slot) | Field::FxFine(slot)) = self.services.cursor.field
                else {
                    return None;
                };
                let sweeps = self
                    .services
                    .current_slot(slot)
                    .and_then(|fx| fx.command)
                    .is_some_and(|command| command.has_fine());
                match self.services.cursor.field {
                    Field::FxFine(_) if sweeps => step(Finer::Already, 1, FINE_MOST),
                    Field::FxValue(_) if sweeps => {
                        step(Finer::PhraseFine(slot), FINE_LEAST, FINE_MOST)
                    }
                    _ => None,
                }
            }
            (Mode::Project, _) if self.project_page.control(self.song()) == ProjectControl::Bpm => {
                let most = (BPM_NUDGE / FINE_NUDGE) as u64 * feel::MOST;
                if self.project_page.fine {
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
                    (pixel / feel::PARTS).max(1),
                    pixel * feel::MOST,
                )
            }
            _ => None,
        }
    }

    /// Where the cursor's setting is a list or a toggle, which the wheel moves a place at a time:
    /// a described setting on the track page or the mixer, a phrase's FX value that chooses
    /// from a list, and the lists and switches of the audio and project pages and the sample
    /// page's settings rows. A selection moves its values as numbers.
    fn places_here(&self) -> Option<Places> {
        if self.modes.mode.is_visual() || matches!(self.modes.mode, Mode::Shape | Mode::Mapping) {
            return None;
        }
        match (self.key_scope(), self.modes.view) {
            (Mode::Track | Mode::Mixer | Mode::Gen, _) => {
                let control = self.track_control();
                let track = self.services.page_track_as_heard();
                if !control.exists_on(&track) {
                    return None;
                }
                Places::of_view(&controls::view(control, &track, self.song(), false)?)
            }
            (Mode::Normal, View::Phrase) => {
                let (Field::FxCommand(slot) | Field::FxValue(slot)) = self.services.cursor.field
                else {
                    return None;
                };
                let fx = self.services.current_slot(slot)?;
                let command = fx.command?;
                let listed = command
                    .setting()
                    .is_some_and(|id| !id.entry().choices().is_empty());
                listed.then(|| {
                    let count = usize::from(command.highest()) + 1;
                    Places::of(Some(usize::from(fx.coarse())), count)
                })
            }
            (Mode::Audio, _) => self
                .audio_page
                .wheel_places(self.wheel.setting, &self.services),
            (Mode::Project, _) => self.project_page.wheel_places(&self.services),
            (Mode::Sample | Mode::SampleVisual, _) => self.sample_page.wheel_places(&self.services),
            _ => None,
        }
    }

    /// Puts the cursor on its setting's fine part for a tick, or back off it after.
    fn step_onto_fine(&mut self, finer: Finer, onto: bool) {
        match finer {
            Finer::TrackFine => self.rack_mut().fine = onto,
            Finer::PhraseFine(slot) => {
                self.services.cursor.field = if onto {
                    Field::FxFine(slot)
                } else {
                    Field::FxValue(slot)
                };
            }
            Finer::TempoFine => self.project_page.fine = onto,
            Finer::Already | Finer::SampleFrame => {}
        }
    }
}
