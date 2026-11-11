//! The mouse wheel and the trackpad as a knob on the cursor's setting (#425): each tick up or down
//! is `=` or `-` where the cursor is, wherever those keys adjust something, a locked selection
//! included, and Shift, or scrolling sideways, is `+` or `_`. Where the cursor's setting has a
//! finer step than `=`, its fine digits, the tempo's fine part or the sample page's frame, a tick
//! takes that, and quick ticks take more of it at a time, so a slow turn lands on any value and a
//! flick sweeps. Where the pointer is doesn't matter, and clicks and drags do nothing.
//!
//! One gesture is one undo step, as a Roto knob's touch to let-go is (decision 160): it ends once
//! the wheel pauses for [`GESTURE_PAUSE`], or at any key.
//!
//! It's off until turned on, on the audio page, as the terminal sends the program its mouse
//! only while it asks, and then keeps plain drags for itself no more: in kitty and Ghostty,
//! Shift and a drag still select text. The setting is kept for this machine in `wheel.ron`.

use std::collections::VecDeque;
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

/// How far back ticks count toward a tick's speed.
const SPEED_WINDOW: Duration = Duration::from_millis(100);

/// The most ticks kept for the speed, more than come in [`SPEED_WINDOW`].
const TICKS_KEPT: usize = 64;

/// Whether the wheel turns the cursor's setting, remembered for this machine rather than the
/// song.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelSetting {
    /// Off unless turned on, as the terminal's own drag to select text is lost while it's on
    /// (the author's decision of 2026-11-11, #425).
    pub on: bool,
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
    gesture: Option<Gesture>,
}

impl Default for Wheel {
    fn default() -> Self {
        Self {
            setting: WheelSetting::default(),
            setting_path: None,
            pause: GESTURE_PAUSE,
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
    /// Its last ticks, the latest last, for its speed.
    ticks: VecDeque<Instant>,
}

impl Gesture {
    fn last(&self) -> Option<Instant> {
        self.ticks.back().copied()
    }

    /// Counts a tick, and says how many ticks have come in the last [`SPEED_WINDOW`], it too.
    fn tick(&mut self, now: Instant) -> u64 {
        if self.ticks.len() == TICKS_KEPT {
            self.ticks.pop_front();
        }
        self.ticks.push_back(now);
        self.ticks
            .iter()
            .filter(|at| now.saturating_duration_since(**at) < SPEED_WINDOW)
            .count() as u64
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

/// How many ticks in [`SPEED_WINDOW`], this one included, make a tick worth how many of the
/// finest step: one alone is one, and the square of how many beyond that, up to `most`, the
/// number of them in one of `=`'s steps.
pub fn tick_size(recent: u64, most: u64) -> u64 {
    recent
        .max(1)
        .saturating_mul(recent.max(1))
        .clamp(1, most.max(1))
}

impl App {
    /// A mouse report from the terminal. Only the wheel acts: up raises the setting under the
    /// cursor and down lowers it, a step each tick, and Shift, or scrolling sideways, takes the
    /// large step, right raising it and left lowering it. Nothing happens while the setting is
    /// off, which it is unless turned on, as the terminal then sends nothing anyway.
    pub fn mouse_event(&mut self, event: MouseEvent, now: Instant) {
        if !self.wheel.setting.on {
            return;
        }
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let (up, large) = match event.kind {
            MouseEventKind::ScrollUp => (true, shift),
            MouseEventKind::ScrollDown => (false, shift),
            MouseEventKind::ScrollRight => (true, true),
            MouseEventKind::ScrollLeft => (false, true),
            _ => return,
        };
        self.turn_wheel(up, large, now);
    }

    /// One tick of the wheel, as the key that adjusts the cursor's setting by the finest step,
    /// or by the large one.
    pub fn turn_wheel(&mut self, up: bool, large: bool, now: Instant) {
        self.now = now;
        if let Some(gesture) = &self.wheel.gesture
            && gesture
                .last()
                .is_some_and(|last| now.saturating_duration_since(last) >= self.wheel.pause)
        {
            self.end_wheel_gesture();
        }
        let finer = if large { None } else { self.finer_here() };
        let key = match (finer, large, up) {
            (Some((Finer::SampleFrame, _)), _, true) => "D-l",
            (Some((Finer::SampleFrame, _)), _, false) => "D-h",
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
        let gesture = self.wheel.gesture.get_or_insert_with(|| Gesture {
            depth,
            adjusted,
            ticks: VecDeque::with_capacity(TICKS_KEPT),
        });
        let recent = gesture.tick(now);
        let size = finer.map_or(1, |(_, most)| tick_size(recent, most));
        // The wheel isn't a key, so `.` still repeats the last key's change.
        let last_change = self.last_change.clone();
        if let Some((finer, _)) = finer {
            self.step_onto_fine(finer, true);
        }
        self.count = (size > 1).then_some(size as usize);
        self.press(key, now);
        self.count = None;
        if let Some((finer, _)) = finer {
            self.step_onto_fine(finer, false);
        }
        self.last_change = last_change;
    }

    /// Ends the wheel's gesture, if one is going, making its ticks one undo step. A key ends it.
    pub fn end_wheel_gesture(&mut self) {
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

    /// Ends the wheel's gesture once it has paused, each frame.
    pub(crate) fn end_paused_wheel(&mut self, now: Instant) {
        let paused = self.wheel.gesture.as_ref().is_some_and(|gesture| {
            gesture
                .last()
                .is_none_or(|last| now.saturating_duration_since(last) >= self.wheel.pause)
        });
        if paused {
            self.end_wheel_gesture();
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
        if let Some(path) = self.wheel.setting_path.clone()
            && let Err(error) = self.wheel.setting.save_to(&path)
        {
            self.status = format!("{}: couldn't be saved: {error}", path.display());
        }
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

    /// Where the cursor's setting has a step finer than `=`'s, how to take it, and how many of
    /// it make one of `=`'s. A selection moves by `=`'s step, as its values are whole.
    fn finer_here(&self) -> Option<(Finer, u64)> {
        if self.mode.is_visual() || self.mode == Mode::Shape {
            return None;
        }
        // A fine part is a 256th of its setting's step.
        const FINE_STEPS: u64 = 256;
        match (self.key_scope(), self.view) {
            (Mode::Track, View::Track) => {
                let has_fine = self.track_page.has_fine_here(self.track_here());
                match (has_fine, self.track_page.fine) {
                    (true, true) => Some((Finer::Already, FINE_STEPS)),
                    (true, false) => Some((Finer::TrackFine, FINE_STEPS)),
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
                    Field::FxFine(_) if sweeps => Some((Finer::Already, FINE_STEPS)),
                    Field::FxValue(_) if sweeps => Some((Finer::PhraseFine(slot), FINE_STEPS)),
                    _ => None,
                }
            }
            (Mode::Project, _) if self.project_control() == ProjectControl::Bpm => {
                let finer = if self.project_fine {
                    Finer::Already
                } else {
                    Finer::TempoFine
                };
                Some((finer, (BPM_NUDGE / FINE_NUDGE) as u64))
            }
            (Mode::Sample, _) if self.sample_page.setting.is_none() => {
                // A pixel's frames, as `=` nudges a point by a pixel.
                let zoom = self.sample_page.zoom.max(1);
                let pixel = match self.sample_page.cell_width {
                    Some(cell) if cell > 1 => zoom / cell as u64,
                    _ => zoom,
                };
                Some((Finer::SampleFrame, pixel.max(1)))
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
