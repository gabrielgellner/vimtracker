//! The mixer, `Space 4`, a strip for each track. `MixerPage` is the page as a component (#320): the
//! row its cursor is on, which goes up and down a strip's settings, with the moves that need only
//! the cursor's track, and its drawing, in `draw`, given a [`Ctx`]. Across the strips, `h`,
//! `l`, `0` and `$` move the program's own cursor to another track, through the [`CtxMut`] it's
//! given, and the edits are the track page's, as the mixer turns the same controls. It's a
//! [`Page`], with its rows of the binding table, [`Page::KEYS`], here beside it.

pub mod draw;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_model::action::TrackControl;
use vt_model::selection::Motion;
use vt_model::song::Track;

use crate::input::bindings::Cmd;
use crate::pages::track::Control;
use crate::pages::{Ctx, CtxMut, Outcome, Page};

/// The mixer's columns.
pub(crate) const MIXER: [Control; 7] = [
    Control::Trim,
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::SendA),
    Control::Base(TrackControl::SendB),
    Control::Mute,
    Control::Solo,
];

/// The mixer's row it opens on, its `Level`, as it's for the mix.
pub(crate) const MIXER_START: usize = 1;

/// The mixer's columns, as rows like a page's.
pub(crate) const MIXER_ROWS: [&[Control]; 1] = [&MIXER];

/// The mixer as a component: the row its cursor is on, kept while other views are shown. Which
/// track it's on is the program's cursor, and what rows there are depends on that track, so each
/// move is given it, or none, and the page holds no song of its own. It's drawn with
/// [`Page::draw`], given the rest of the program as a [`Ctx`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixerPage {
    /// The row the cursor was put on, kept on a track that hasn't it, as [`MixerPage::row_here`]
    /// says.
    pub row: usize,
}

impl Default for MixerPage {
    fn default() -> Self {
        Self { row: MIXER_START }
    }
}

impl MixerPage {
    /// The mixer's rows a track has: a fixed track has no sends, and the master no pan, mute or
    /// solo.
    fn rows(track: Option<&Track>) -> Vec<usize> {
        (0..MIXER.len())
            .filter(|row| track.is_none_or(|track| MIXER[*row].exists_on(track)))
            .collect()
    }

    /// The row the cursor is on. On a track without the row it was on, it's the nearest above it,
    /// and the row it was on is kept, so going back to a track that has it finds it again.
    pub(crate) fn row_here(&self, track: Option<&Track>) -> usize {
        let rows = Self::rows(track);
        let wanted = self.row.min(MIXER.len() - 1);
        rows.iter()
            .rev()
            .find(|row| **row <= wanted)
            .or(rows.first())
            .copied()
            .unwrap_or(0)
    }

    /// What the cursor is on, on a track.
    pub fn control(&self, track: Option<&Track>) -> Control {
        MIXER[self.row_here(track)]
    }

    /// What the cursor is on, on the cursor's track.
    pub(crate) fn control_here(&self, ctx: &Ctx) -> Control {
        self.control(ctx.track_here())
    }

    /// Move up or down the mixer's rows, going only through those this track has.
    fn move_row(&mut self, by: isize, track: Option<&Track>) {
        let rows = Self::rows(track);
        let here = self.row_here(track);
        let at = rows.iter().position(|row| *row == here).unwrap_or(0);
        let to = (at as isize)
            .saturating_add(by)
            .clamp(0, rows.len() as isize - 1);
        self.row = rows.get(to as usize).copied().unwrap_or(0);
    }

    /// Run one of the page's own keys on a track, `j` and `k` up and down a strip and `gg` and
    /// `G` to its ends. `times` is its count, at least 1.
    pub fn key_on(&mut self, cmd: Cmd, times: usize, track: Option<&Track>) -> Outcome {
        let times = times as isize;
        match cmd {
            Cmd::Move(Motion::Down) => self.move_row(times, track),
            Cmd::Move(Motion::Up) => self.move_row(-times, track),
            Cmd::Move(Motion::First) => self.move_row(isize::MIN, track),
            Cmd::Move(Motion::Last) => self.move_row(isize::MAX, track),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// `0` and `$`: the first or last strip shown that has the row the cursor is on, for the
    /// program's cursor to go to, with that row kept for it. A folded strip is stepped over.
    pub(crate) fn end_track(&mut self, last: bool, ctx: &CtxMut) -> Option<usize> {
        let row = self.row_here(ctx.track_here());
        let tracks = &ctx.song().tracks;
        let mut shown = ctx
            .services
            .folds
            .shown(ctx.song())
            .filter(|&track| MIXER[row].exists_on(&tracks[track]));
        let found = if last { shown.last() } else { shown.next() };
        if found.is_some() {
            self.row = row;
        }
        found
    }
}

impl Page for MixerPage {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("h", Cmd::Move(Motion::Left), "Previous track"),
        ("l", Cmd::Move(Motion::Right), "Next track"),
        ("0", Cmd::Move(Motion::RowStart), "First track"),
        ("$", Cmd::Move(Motion::RowEnd), "Last track"),
        ("j", Cmd::Move(Motion::Down), "Next setting"),
        ("k", Cmd::Move(Motion::Up), "Previous setting"),
        ("g g", Cmd::Move(Motion::First), "First setting"),
        ("G", Cmd::Move(Motion::Last), "Last setting"),
        ("=", Cmd::Increase, "Increase the value, or switch on"),
        ("-", Cmd::Decrease, "Decrease the value, or switch off"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        ("C-a", Cmd::Increase, "Increase the value"),
        ("C-x", Cmd::Decrease, "Decrease the value"),
        ("x", Cmd::ClearField, "Back to its default"),
        ("K", Cmd::PickValue, "Switch mute or solo"),
        ("/", Cmd::FieldJump, "Jump to a setting by its name"),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the page, on the cursor's track: one of the page's
    /// own keys, [`MixerPage::key_on`]; `h` and `l` to the strips either side and `0` and `$` to
    /// the ends, moving the program's cursor; or passed back, for an edit, which is the track
    /// page's, or what it means across pages.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        let outcome = self.key_on(cmd, times, ctx.track_here());
        if !matches!(outcome, Outcome::Passed) {
            return outcome;
        }
        let view = ctx.modes.view;
        match cmd {
            Cmd::Move(Motion::Right) => ctx.services.move_track(times as isize, view),
            Cmd::Move(Motion::Left) => ctx.services.move_track(-(times as isize), view),
            Cmd::Move(Motion::RowStart | Motion::RowEnd) => {
                if let Some(to) = self.end_track(cmd == Cmd::Move(Motion::RowEnd), ctx) {
                    ctx.services.go_to_track_at(to, view);
                }
            }
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Draw the mixer.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::pages::mixer::draw::draw(frame, area, self, ctx);
    }

    fn column(&self, ctx: &Ctx) -> String {
        let control = self.control_here(ctx);
        ctx.track_here().map_or_else(
            || control.label().to_owned(),
            |track| control.name_on(track).to_owned(),
        )
    }
}
