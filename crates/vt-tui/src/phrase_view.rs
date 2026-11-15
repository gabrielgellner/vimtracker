//! What the phrase view keeps of its own, and the keys that need nothing else: one track's steps,
//! with the grid's cursor moving over them.
//!
//! `PhraseView` is the view as a component (#320): where it's scrolled to and how many lines fit,
//! whether it's laid out in time and shows the row inspector, and where a selection started and
//! how far it reaches. It's a [`Page`], with its rows of the binding table here beside it. Its own
//! keys are the moves, the keys over the view (`H` `M` `L`, `C-d` `C-u`, `C-e` `C-y`), reaching
//! another track from a selection, `zi` and `Space i`, run through the `Services`, and it's drawn
//! by `views::phrase`, given a [`Ctx`]. The grammar's operators, registers, Visual modes and `.`,
//! the edits and Insert's typing, the tree's keys, the loop's, recording and the pickers stay
//! `App`'s, as they change the modes or reach what spans the pages, and read the selection here.

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_model::constants::BEAT_STEPS;
use vt_model::selection::{
    Extent, FIRST_FX_ORDER, FX_GROUP, Field, Motion, Scope, StepRange, Target,
};
use vt_model::step::FxCommand;

use crate::app::{Cursor, Selection, TrackSpan};
use crate::input::bindings::{Cmd, Op};
use crate::input::mode::{Mode, View};
use crate::page::{Ctx, CtxMut, Outcome, Page};
use crate::services::Services;
use crate::timeline;

/// One beat, as a slide.
const BEAT: isize = BEAT_STEPS as isize;

/// The phrase view as a component: how it's scrolled and laid out, and the selection on it. Which
/// step and field the cursor is on is the program's cursor, which every page shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhraseView {
    /// The first step on screen, and how many lines fit.
    pub top: usize,
    pub height: usize,
    /// It's collapsed to a line a row, rather than laid out in time (decision 122).
    pub rows_collapsed: bool,
    /// It shows the row inspector right of the overview, for this session (decision 255).
    pub inspector: bool,
    /// Where a selection started: its step, and the columns the cursor was on.
    pub(crate) anchor: Option<(usize, Scope)>,
    /// How far a selection reaches across tracks from the cursor's.
    pub(crate) anchor_track: Option<usize>,
    /// The columns a selection keeps wherever the cursor goes, as a text object or `gv` sets them.
    pub(crate) scope: Option<Scope>,
    /// The selection `gv` brings back.
    pub(crate) last_selection: Option<Selection>,
}

impl Default for PhraseView {
    fn default() -> Self {
        Self {
            top: 0,
            height: 16,
            rows_collapsed: false,
            inspector: false,
            anchor: None,
            anchor_track: None,
            scope: None,
            last_selection: None,
        }
    }
}

impl PhraseView {
    /// Whether the view is laid out in time rather than collapsed to a line a row.
    pub fn timeline_on(&self) -> bool {
        !self.rows_collapsed
    }

    /// Collapse the view to a line a row, or open it out into its timeline again.
    fn toggle_timeline(&mut self, services: &mut Services) {
        self.rows_collapsed = !self.rows_collapsed;
        services.status = if self.rows_collapsed {
            "Rows: a line each, beside the other tracks' rows. z i lays them out in time"
        } else {
            "Timeline: a line for each pulse, beside the other tracks in time. z i collapses it"
        }
        .to_owned();
    }

    /// Show the row inspector, or hide it.
    fn toggle_inspector(&mut self, services: &mut Services) {
        self.inspector = !self.inspector;
        services.status = if self.inspector {
            "Row inspector: the cursor's row in words, while there's room. Space i hides it"
        } else {
            "Row inspector hidden. Space i shows it"
        }
        .to_owned();
    }

    /// How many lines each stored row of the cursor's phrase takes on screen.
    pub(crate) fn row_line_counts(&self, services: &Services) -> Vec<usize> {
        let timeline = self.timeline_on();
        services.phrase_here().map_or_else(Vec::new, |phrase| {
            (0..phrase.steps().len())
                .map(|index| timeline::row_lines(phrase, index, timeline))
                .collect()
        })
    }

    /// The furthest the view can scroll: to the end of what is stored, or with the loop's start at
    /// the top, so a slide can keep row `00` still on a screen with room to spare. Rows taking
    /// several lines in the timeline count each of them.
    fn last_top(&self, services: &Services) -> usize {
        let counts = self.row_line_counts(services);
        timeline::row_lines_before(&counts, counts.len(), self.height).max(services.extent().start)
    }

    /// Keep the cursor's row on screen, moving the view rather than the cursor.
    pub(crate) fn follow_cursor(&mut self, height: usize, services: &Services) {
        self.height = height.max(1);
        let last_top = self.last_top(services);
        let counts = self.row_line_counts(services);
        let step = services.cursor.step;
        if step < self.top {
            self.top = step;
        } else {
            let end = step + 1;
            self.top = self
                .top
                .max(timeline::row_lines_before(&counts, end, self.height))
                .min(step);
        }
        self.top = self.top.min(last_top);
    }

    /// `H`, `M` and `L`: the cursor to the top, the middle or the bottom of the view.
    fn view_cursor(&self, cmd: Cmd, services: &mut Services, view: View) {
        let counts = self.row_line_counts(services);
        let all = timeline::lines_over(&counts, 0..services.extent().stored.max(1));
        let height = self.height.min(all);
        // The bottom is the last row whose lines all fit, so the view doesn't move to show it.
        services.cursor.step = match cmd {
            Cmd::ViewTop => self.top,
            Cmd::ViewMiddle => timeline::row_at_line(&counts, self.top, height / 2),
            _ => timeline::row_at_line(&counts, self.top, height)
                .saturating_sub(1)
                .max(self.top),
        };
        services.clamp_cursor(view);
    }

    /// Half a screen down or up, with the cursor's row in the middle.
    fn half_page(&mut self, sign: isize, services: &mut Services) {
        let counts = self.row_line_counts(services);
        let half = (self.height / 2).max(1);
        let rows = timeline::rows_over(&counts, services.cursor.step, half, sign > 0).max(1);
        services.move_step(sign * rows as isize);
        let counts = self.row_line_counts(services);
        self.top = timeline::row_lines_before(&counts, services.cursor.step, self.height / 2);
    }

    /// Scroll the view. The cursor only moves if it would leave the screen.
    fn scroll(&mut self, by: isize, services: &mut Services, view: View) {
        self.top = self
            .top
            .saturating_add_signed(by)
            .min(self.last_top(services));
        let counts = self.row_line_counts(services);
        // The last row whose lines all fit, or the top's own if it doesn't.
        let bottom = timeline::row_at_line(&counts, self.top, self.height)
            .saturating_sub(1)
            .max(self.top);
        services.cursor.step = services.cursor.step.clamp(self.top, bottom);
        services.clamp_cursor(view);
    }

    /// The selection in a Visual mode, from where it started to the cursor.
    pub fn selection(&self, mode: Mode, cursor: Cursor) -> Selection {
        let (start, start_scope) = self
            .anchor
            .unwrap_or((cursor.step, Scope::column(cursor.field.group())));
        let scope = match (mode, self.scope) {
            (_, Some(fixed)) => fixed,
            (Mode::VisualStep, _) => Scope::WHOLE,
            _ => {
                let here = cursor.field.group();
                Scope {
                    first: start_scope.first.min(here),
                    last: start_scope.last.max(here),
                }
            }
        };
        let anchor_track = self.anchor_track.unwrap_or(cursor.track);
        Selection {
            range: StepRange::between(start, cursor.step),
            scope,
            tracks: TrackSpan::between(anchor_track, cursor.track),
        }
    }

    /// Reach another track from a selection. The cursor stays where it is, so moving between
    /// tracks of different lengths doesn't change which rows are selected.
    fn reach_track(&mut self, by: isize, services: &Services) {
        let last = services.song().tracks.len() as isize - 1;
        let from = self.anchor_track.unwrap_or(services.cursor.track) as isize;
        self.anchor_track = Some((from + by).clamp(0, last) as usize);
    }

    /// What `gg` and `G` go by: the cursor's FX column's lane if it has one, and the phrase's loop
    /// otherwise. Beats always go by the phrase's loop.
    pub(crate) fn motion_extent(motion: Motion, services: &Services) -> Extent {
        let extent = services.extent();
        let lane = match (motion, services.cursor.field) {
            (
                Motion::First | Motion::Last,
                Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot),
            ) => services.phrase_here().and_then(|phrase| phrase.lane(slot)),
            _ => None,
        };
        lane.map_or(extent, |lane| Extent {
            start: lane.start,
            length: lane.length,
            ..extent
        })
    }

    /// Move the cursor by a motion: along the row as many times as the count, or down the steps
    /// with the count as the motion takes it, as `5j` moves 5 steps and `5G` goes to step 5.
    pub(crate) fn move_cursor(motion: Motion, count: Option<usize>, services: &mut Services) {
        if motion.is_horizontal() {
            for _ in 0..count.unwrap_or(1) {
                Self::move_horizontal(motion, services);
            }
        } else {
            services.cursor.step = motion.step(
                services.cursor.step,
                count,
                Self::motion_extent(motion, services),
            );
        }
    }

    /// Move between fields, or between the step's parts: the note with its velocity and gate,
    /// then each FX slot, as the headings over the grid name them.
    fn move_horizontal(motion: Motion, services: &mut Services) {
        let here = services
            .phrase_here()
            .and_then(|phrase| phrase.steps().get(services.cursor.step));
        let slots = here.map_or(0, |step| step.fx.len());
        let last = FIRST_FX_ORDER - 1 + slots * 3;
        let order = services.cursor.field.order();
        let group = services.cursor.field.group();
        // A value with no fine part has no fine digits to land on, so they're passed over.
        let landable = |field: Field| match field {
            Field::FxFine(slot) => here
                .and_then(|step| step.fx.get(slot))
                .and_then(|cell| cell.command)
                .is_some_and(FxCommand::has_fine),
            _ => true,
        };
        let step = |from: usize, forward: bool| {
            let next = if forward {
                (from + 1).min(last)
            } else {
                from.saturating_sub(1)
            };
            let field = Field::from_order(next);
            if landable(field) {
                field
            } else if forward && next < last {
                Field::from_order(next + 1)
            } else {
                Field::from_order(next.saturating_sub(1))
            }
        };
        services.cursor.field = match motion {
            Motion::Right => step(order, true),
            Motion::Left => step(order, false),
            Motion::NextGroup if slots == 0 => services.cursor.field,
            Motion::NextGroup => {
                Field::of_group(group.max(FX_GROUP - 1).saturating_add(1).min(2 + slots))
            }
            Motion::RowStart => Field::Note,
            Motion::RowEnd if landable(Field::from_order(last)) => Field::from_order(last),
            Motion::RowEnd => Field::from_order(last - 1),
            _ if group > FX_GROUP => Field::of_group(group - 1),
            _ => Field::Note,
        };
    }

    /// Whether one of the view's own keys takes the count typed before it: a move does, as `5j`
    /// and `5G` do, and `zi` and `Space i` drop it. The keys over the view and reaching another
    /// track leave it for the key after them.
    pub(crate) fn takes_count(cmd: Cmd) -> bool {
        matches!(
            cmd,
            Cmd::Move(_) | Cmd::ToggleTimeline | Cmd::ToggleInspector
        )
    }

    /// Motions work in Normal mode, after an operator, and in Visual modes.
    pub(crate) const MOTIONS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("h", Cmd::Move(Motion::Left), "Previous field"),
        ("l", Cmd::Move(Motion::Right), "Next field"),
        ("j", Cmd::Move(Motion::Down), "Next step"),
        ("k", Cmd::Move(Motion::Up), "Previous step"),
        (
            "w",
            Cmd::Move(Motion::NextGroup),
            "Next part: the note, then each FX",
        ),
        (
            "b",
            Cmd::Move(Motion::PreviousGroup),
            "Previous part: an FX, or the note",
        ),
        ("0", Cmd::Move(Motion::RowStart), "Start of the row"),
        ("$", Cmd::Move(Motion::RowEnd), "End of the row"),
        ("}", Cmd::Move(Motion::NextBeat), "Next beat"),
        ("{", Cmd::Move(Motion::PreviousBeat), "Previous beat"),
        ("g g", Cmd::Move(Motion::First), "First step, or step n"),
        ("G", Cmd::Move(Motion::Last), "Last step, or step n"),
    ];

    /// Targets work after an operator and in Visual modes.
    pub(crate) const TARGETS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("i b", Cmd::On(Target::Beat), "This beat, whole steps"),
        ("i c", Cmd::On(Target::Column), "This column, whole phrase"),
        ("i p", Cmd::On(Target::Phrase), "This phrase, whole steps"),
    ];

    /// Viewport keys work wherever the cursor can move on screen.
    pub(crate) const VIEWPORT: &'static [(&'static str, Cmd, &'static str)] = &[
        ("H", Cmd::ViewTop, "Cursor to the top of the view"),
        ("M", Cmd::ViewMiddle, "Cursor to the middle of the view"),
        ("L", Cmd::ViewBottom, "Cursor to the bottom of the view"),
        ("C-d", Cmd::HalfPageDown, "Half a screen down, centred"),
        ("C-u", Cmd::HalfPageUp, "Half a screen up, centred"),
        ("C-e", Cmd::ScrollDown, "Scroll the view down"),
        ("C-y", Cmd::ScrollUp, "Scroll the view up"),
    ];

    /// Keys that act on a selection.
    pub(crate) const VISUAL: &'static [(&'static str, Cmd, &'static str)] = &[
        ("W", Cmd::NextTrack, "Reach the next track"),
        ("B", Cmd::PreviousTrack, "Reach the previous track"),
        ("] b", Cmd::NextTrack, "Reach the next track"),
        ("[ b", Cmd::PreviousTrack, "Reach the previous track"),
        ("d", Cmd::Operator(Op::Clear), "Clear the selection"),
        ("y", Cmd::Operator(Op::Copy), "Copy the selection"),
        (">", Cmd::Operator(Op::ShiftUp), "Shift the selection up"),
        (
            "<",
            Cmd::Operator(Op::ShiftDown),
            "Shift the selection down",
        ),
        ("x", Cmd::Operator(Op::Clear), "Clear the selection"),
        (
            "r",
            Cmd::Replace,
            "Replace this column across the selection",
        ),
        (
            "S",
            Cmd::Shape,
            "Fill this column with a ramp or random values",
        ),
        ("=", Cmd::Increase, "Increase the selection's values"),
        ("-", Cmd::Decrease, "Decrease the selection's values"),
        (
            "+",
            Cmd::IncreaseLarge,
            "Increase the selection's values, a large step",
        ),
        (
            "_",
            Cmd::DecreaseLarge,
            "Decrease the selection's values, a large step",
        ),
        (
            "C-a",
            Cmd::IncreaseChromatic,
            "Increase the selection's values, by a semitone in a scale",
        ),
        (
            "C-x",
            Cmd::DecreaseChromatic,
            "Decrease the selection's values, by a semitone in a scale",
        ),
        ("~", Cmd::CycleMark, "Cycle the selected values' marks"),
        (
            "Space g A",
            Cmd::BakeMods,
            "Write the pitch the mod slots played into the selected notes",
        ),
        (
            "Space o",
            Cmd::RotateSpeeds(false),
            "Rotate the speeds round the tracks selected",
        ),
        (".", Cmd::Repeat, "Repeat the last change on the selection"),
        (
            "u",
            Cmd::Undo,
            "Undo the last change, keeping the selection",
        ),
        ("C-r", Cmd::Redo, "Redo, keeping the selection"),
    ];

    /// Keys for Insert on the Note column, beside the note keys, which are its own and aren't
    /// listed here. As a 303 has its ACCENT and SLIDE keys past the top of its keyboard, these are
    /// past ours: `'` is a stress mark, and `/` a slide in guitar tab. `k` and `,` are its UP and
    /// DOWN, one above the other, and `o` above `l` widens the grid, as `Option-2` above
    /// `Option-1` does.
    pub(crate) const INSERT_NOTE: &'static [(&'static str, Cmd, &'static str)] = &[
        (
            "'",
            Cmd::Accent,
            "Note column: accent the note just written; held, the notes played",
        ),
        (
            "/",
            Cmd::Slide,
            "Note column: slide from the note just written; held, the notes played",
        ),
        (
            "k",
            Cmd::OctaveUp,
            "Note column: the note just written an octave up; held, the notes played",
        ),
        (
            ",",
            Cmd::OctaveDown,
            "Note column: the note just written an octave down; held, the notes played",
        ),
        ("o", Cmd::WidenGrid, "Note column: wider grid, as M-2"),
        ("l", Cmd::NarrowGrid, "Note column: narrower grid, as M-1"),
    ];

    /// A doubled operator key acts on the current step.
    pub(crate) const WHOLE_STEP: &'static [(&'static str, Cmd, &'static str)] = &[
        ("d", Cmd::WholeStep, "This step"),
        ("y", Cmd::WholeStep, "This step"),
        (">", Cmd::WholeStep, "This step"),
        ("<", Cmd::WholeStep, "This step"),
    ];
}

impl Page for PhraseView {
    /// Keys for Normal mode only.
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("d", Cmd::Operator(Op::Clear), "Clear"),
        ("y", Cmd::Operator(Op::Copy), "Copy"),
        (">", Cmd::Operator(Op::ShiftUp), "Shift up"),
        ("<", Cmd::Operator(Op::ShiftDown), "Shift down"),
        ("x", Cmd::ClearField, "Clear this field"),
        ("p", Cmd::Paste, "Paste over from the cursor"),
        ("g p", Cmd::PasteAfter, "Paste over, and land just after it"),
        (
            "g m",
            Cmd::PasteMerge,
            "Paste, keeping what the copy leaves empty",
        ),
        (";", Cmd::Audition, "Audition this row on every track"),
        ("r", Cmd::Replace, "Replace this field, and stay here"),
        ("s", Cmd::Substitute, "Clear this field and start Insert"),
        ("=", Cmd::Increase, "Increase the value"),
        ("-", Cmd::Decrease, "Decrease the value"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        (
            "C-a",
            Cmd::IncreaseChromatic,
            "Increase the value, by a semitone in a scale",
        ),
        (
            "C-x",
            Cmd::DecreaseChromatic,
            "Decrease the value, by a semitone in a scale",
        ),
        (
            "~",
            Cmd::CycleMark,
            "Cycle a value's mark: lock, hold, glide",
        ),
        (".", Cmd::Repeat, "Repeat the last change"),
        ("v", Cmd::VisualField, "Select fields"),
        ("V", Cmd::VisualStep, "Select whole steps"),
        ("g v", Cmd::Reselect, "Reselect the last selection"),
        (
            "M-1",
            Cmd::NarrowGrid,
            "Narrower grid for the arrows in Insert",
        ),
        ("M-2", Cmd::WidenGrid, "Wider grid for the arrows in Insert"),
        ("M-s", Cmd::ToggleScale, "Scale on or off"),
        (
            "z i",
            Cmd::ToggleTimeline,
            "Collapse the timeline to a line a row, or open it again",
        ),
        (
            "Space i",
            Cmd::ToggleInspector,
            "Inspect the cursor's row beside the overview, or stop",
        ),
        ("Space p d", Cmd::Double, "Double this phrase"),
        (
            "g l",
            Cmd::ReopenGenerator,
            "Open the shape panel again on the live column here",
        ),
        (
            "Space g a",
            Cmd::ApplyGenerator,
            "Apply the live column here, leaving plain values",
        ),
        (
            "Space g A",
            Cmd::BakeMods,
            "Write the pitch the mod slots played into the notes",
        ),
        (
            "Space r",
            Cmd::ToggleRecord,
            "Record notes played in Insert, while playing",
        ),
        ("K", Cmd::PickValue, "Choose this field from a list"),
        (
            "g d",
            Cmd::OpenItem,
            "On an SLI's value, open the sample page on its slice",
        ),
        ("g r r", Cmd::ListUses, "List where this phrase plays"),
        ("g c", Cmd::ClonePhrase, "Copy this phrase here"),
        ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
        ("] p", Cmd::NextSlot, "Next phrase in the chain"),
        ("[ p", Cmd::PreviousSlot, "Previous phrase in the chain"),
        ("] P", Cmd::LastSlot, "Last phrase in the chain"),
        ("[ P", Cmd::FirstSlot, "First phrase in the chain"),
        ("] l", Cmd::SlideLoop(1), "Slide the loop a step later"),
        ("[ l", Cmd::SlideLoop(-1), "Slide the loop a step earlier"),
        ("] L", Cmd::SlideLoop(BEAT), "Slide the loop a beat later"),
        (
            "[ L",
            Cmd::SlideLoop(-BEAT),
            "Slide the loop a beat earlier",
        ),
        ("] r", Cmd::RotateLoop(1), "Rotate the loop's steps later"),
        (
            "[ r",
            Cmd::RotateLoop(-1),
            "Rotate the loop's steps earlier",
        ),
        ("g s", Cmd::StartLoopHere, "Start the loop on this step"),
        ("g e", Cmd::EndLoopHere, "End the loop on this step"),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the view: a move, a key over the view, reaching
    /// another track from a selection, or `zi` and `Space i`, or pass it back. A move after an
    /// operator is what the operator covers, so it's the grammar's. The count is read as the
    /// motion takes it, as `5G` goes to step 5 where `5j` moves 5.
    fn key(&mut self, cmd: Cmd, _times: usize, ctx: &mut CtxMut) -> Outcome {
        let mode = ctx.modes.mode;
        let view = ctx.modes.view;
        let services = &mut *ctx.services;
        match cmd {
            Cmd::Move(_) if mode == Mode::OperatorPending => return Outcome::Passed,
            Cmd::Move(motion) => Self::move_cursor(motion, ctx.modes.count, services),
            Cmd::ViewTop | Cmd::ViewMiddle | Cmd::ViewBottom => {
                self.view_cursor(cmd, services, view);
            }
            Cmd::HalfPageDown => self.half_page(1, services),
            Cmd::HalfPageUp => self.half_page(-1, services),
            Cmd::ScrollDown => self.scroll(3, services, view),
            Cmd::ScrollUp => self.scroll(-3, services, view),
            Cmd::NextTrack if mode.is_visual() => self.reach_track(1, services),
            Cmd::PreviousTrack if mode.is_visual() => self.reach_track(-1, services),
            Cmd::ToggleTimeline => self.toggle_timeline(services),
            Cmd::ToggleInspector => self.toggle_inspector(services),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Draw the view.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::views::phrase::draw(frame, area, self, ctx);
    }
}
