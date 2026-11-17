//! The macro panel (decision 385): a strip at the bottom of the screen with the cursor's track's
//! eight macros, as the Gen page's band has them, so they can be turned from any page. `Space t`
//! shows it or hides it, and the pages give up its rows while it shows. `Space T` puts the keys in
//! it, showing it if it's hidden: `h` and `l` go between the macros, `=` `-` `+` `_`, `x`, `i` and
//! the wheel turn the one the keys are on, as the band's keys do, and `Esc` gives the keys back to
//! the page, its cursor where it was.
//!
//! It follows the cursor's track, which the project page's Song and Audio sub-pages leave where
//! it was, so it shows the track last on. Whether it shows is kept for the session only, not in
//! the song.

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use std::time::Instant;
use vt_model::macros::MACROS;

use crate::app::App;
use crate::controls::Control;
use crate::input::bindings::{Cmd, DESCRIBE, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::typed::{HEX, Typed, TypedValue};
use crate::overlays::Overlay;
use crate::pages::track::draw::draw_macro_strip;
use crate::pages::track::editing::{adjust_control, reset_control};
use crate::pages::track::typing::commit_control;
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;

pub(crate) use crate::pages::track::draw::STRIP_HEIGHT;

/// The macro panel: whether it shows, the macro the keys go to in it, and what's typed there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroPanel {
    /// Whether it shows at the bottom of the screen.
    pub open: bool,
    /// The macro its cursor is on, kept while the keys are elsewhere.
    pub at: usize,
    /// The mode the keys were in before they came to the panel, which `Esc` gives them back to.
    pub back: Mode,
    /// What's typed so far after `i`, two hex digits, if it's typing.
    pub typing: Option<TypedValue>,
}

impl Default for MacroPanel {
    fn default() -> Self {
        Self {
            open: false,
            at: 0,
            back: Mode::Normal,
            typing: None,
        }
    }
}

/// A key of the macro panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroKey {
    /// Back to the page, the panel left showing.
    Leave,
    /// Back to the page, the panel hidden.
    Hide,
    Left,
    Right,
    Increase,
    Decrease,
    IncreaseLarge,
    DecreaseLarge,
    /// `x`: back to where it starts.
    Reset,
    /// `i`: type it, two hex digits.
    Type,
    Undo,
    Redo,
    /// Play or pause, and the next or the previous track, as on every page.
    Play,
    NextTrack,
    PreviousTrack,
    /// `Space d`: describe the macro under the cursor, or stop, as on the pages.
    Describe,
}

impl MacroPanel {
    /// The setting the panel's cursor is on.
    pub fn control(&self) -> Control {
        Control::Macro(self.at.min(MACROS - 1))
    }

    /// Say on the status line what's typed so far, or why nothing is.
    fn typed(&mut self, code: KeyCode, services: &mut Services) {
        let Some(typed) = self.typing.as_mut() else {
            return;
        };
        match typed.key(code, HEX) {
            Typed::Editing => services.status = typed.shown(),
            Typed::Cancel => {
                self.typing = None;
                services.status.clear();
            }
            Typed::Commit(text) => {
                self.typing = None;
                services.status.clear();
                commit_control(services, self.control(), &text);
            }
        }
    }
}

impl Overlay for MacroPanel {
    const MODE: Mode = Mode::Macros;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        (
            "Esc",
            Cmd::InMacros(MacroKey::Leave),
            "Back to the page, the panel showing",
        ),
        (
            "q",
            Cmd::InMacros(MacroKey::Leave),
            "Back to the page, the panel showing",
        ),
        (
            "Space t",
            Cmd::InMacros(MacroKey::Hide),
            "Hide the panel, and back to the page",
        ),
        (
            "Space T",
            Cmd::InMacros(MacroKey::Leave),
            "Back to the page, the panel showing",
        ),
        ("h", Cmd::InMacros(MacroKey::Left), "The macro before"),
        ("Left", Cmd::InMacros(MacroKey::Left), "The macro before"),
        ("l", Cmd::InMacros(MacroKey::Right), "The next macro"),
        ("Right", Cmd::InMacros(MacroKey::Right), "The next macro"),
        ("=", Cmd::InMacros(MacroKey::Increase), "Turn it up"),
        ("-", Cmd::InMacros(MacroKey::Decrease), "Turn it down"),
        (
            "+",
            Cmd::InMacros(MacroKey::IncreaseLarge),
            "Turn it up, a large step",
        ),
        (
            "_",
            Cmd::InMacros(MacroKey::DecreaseLarge),
            "Turn it down, a large step",
        ),
        (
            "x",
            Cmd::InMacros(MacroKey::Reset),
            "Back to where it starts",
        ),
        (
            "i",
            Cmd::InMacros(MacroKey::Type),
            "Type it, two hex digits",
        ),
        ("u", Cmd::InMacros(MacroKey::Undo), "Undo"),
        ("C-r", Cmd::InMacros(MacroKey::Redo), "Redo"),
        (
            "Tab",
            Cmd::InMacros(MacroKey::Play),
            "Play, or pause in place",
        ),
        ("W", Cmd::InMacros(MacroKey::NextTrack), "Next track"),
        (
            "B",
            Cmd::InMacros(MacroKey::PreviousTrack),
            "Previous track",
        ),
        ("Space d", Cmd::InMacros(MacroKey::Describe), DESCRIBE),
    ];

    /// A turn is the Gen page band's own edit of the macro, one undo step a press. The wheel's
    /// ticks of a frame come as one press with a count, which is taken.
    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Cmd::InMacros(key) = cmd else {
            return Outcome::Passed;
        };
        let times = ctx.modes.count.unwrap_or(1).max(1);
        let services = &mut *ctx.services;
        let control = self.control();
        match key {
            MacroKey::Leave => return Outcome::Close,
            MacroKey::Hide => {
                self.open = false;
                return Outcome::Close;
            }
            MacroKey::Left => self.at = self.at.saturating_sub(1),
            MacroKey::Right => self.at = (self.at + 1).min(MACROS - 1),
            MacroKey::Increase => adjust_control(services, control, true, times, false),
            MacroKey::Decrease => adjust_control(services, control, false, times, false),
            MacroKey::IncreaseLarge => adjust_control(services, control, true, times, true),
            MacroKey::DecreaseLarge => adjust_control(services, control, false, times, true),
            MacroKey::Reset => reset_control(services, control),
            MacroKey::Type => {
                self.typing = Some(TypedValue::default());
                services.status = "Two hex digits for this macro".to_owned();
            }
            MacroKey::Undo => return Outcome::Do(Cmd::Undo),
            MacroKey::Redo => return Outcome::Do(Cmd::Redo),
            MacroKey::Play => return Outcome::Do(Cmd::PauseOrResume),
            MacroKey::NextTrack => return Outcome::Do(Cmd::NextTrack),
            MacroKey::PreviousTrack => return Outcome::Do(Cmd::PreviousTrack),
            MacroKey::Describe => return Outcome::Do(Cmd::ToggleDescription),
        }
        Outcome::Done
    }

    /// Cmd with `=`'s and its neighbours' Cmd keys is that key.
    fn reads(&self, key: Key) -> Key {
        if key.command {
            adjusting_key(key).unwrap_or(key)
        } else {
            key
        }
    }

    fn takes_text(&self) -> bool {
        self.typing.is_some()
    }

    /// What's typed after `i`; any other key does nothing.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        self.typed(key.code, ctx.services);
        Outcome::Done
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        let cursor = (ctx.mode() == Mode::Macros).then_some(self.at);
        draw_macro_strip(frame, area, ctx, cursor);
    }
}

/// What the status line names while the keys are in the panel: the macro its cursor is on, as
/// the Gen page's band names it.
pub(crate) fn column(panel: &MacroPanel, services: &Services) -> String {
    let at = panel.control();
    match services.track_here() {
        Some(track) => at.name_on(track).to_owned(),
        None => at.label().to_owned(),
    }
}

impl App {
    /// `Space t`: show the macro panel, or hide it.
    pub(crate) fn toggle_macro_panel(&mut self) {
        let panel = &mut self.overlays.macro_panel;
        panel.open = !panel.open;
    }

    /// `Space T`: the keys into the macro panel, showing it, on the macro its cursor was left on.
    pub(crate) fn focus_macro_panel(&mut self) {
        self.modes.count = None;
        let panel = &mut self.overlays.macro_panel;
        panel.open = true;
        panel.typing = None;
        panel.back = self.modes.mode;
        self.modes.mode = Mode::Macros;
    }

    /// Give the macro panel a key.
    pub(crate) fn macro_panel_key(&mut self, key: Key, now: Instant) {
        let mut panel = std::mem::take(&mut self.overlays.macro_panel);
        let outcome = crate::overlays::give(
            &mut panel,
            &self.table,
            &mut self.grammar,
            CtxMut {
                services: &mut self.services,
                modes: &self.modes,
                wheel: &mut self.wheel,
            },
            key,
            now,
        );
        self.overlays.macro_panel = panel;
        self.settled(outcome, false);
    }
}
