//! The phrase view: one track's steps, the cursor and the playhead.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::constants::{BAR_STEPS, BEAT_STEPS, FX_SLOTS, TICKS_PER_STEP};
use vt_model::generator::Span as GenSpan;
use vt_model::label::GROUP_NAME_CHARS;
use vt_model::selection::{Extent, Field, Scope, StepRange};
use vt_model::shape::{ShapeColumn, Source};
use vt_model::song::{Phrase, Song, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, GateKind, Mark, Note, Step, nudge_text};

use crate::app::{Selection, TrackSpan};
use crate::input::mode::Mode;
use crate::pages::Ctx;
use crate::pages::phrase::PhraseView;
use crate::pages::phrase::inspector;
use crate::pages::phrase::timeline::{self, Line as TimeLine};
use crate::services::folding::Order;
use crate::widgets::glyphs;
use crate::widgets::group::{self, Switched};
use crate::widgets::role;
use crate::widgets::table::centred_run;
use crate::widgets::theme;

/// The width of a read-only note column for another track: its playhead marker, a note and a gap,
/// so a marker has a cell of its own and the playing line reads as columns, not one string.
const NEIGHBOUR_WIDTH: usize = 5;
/// A group's column is wider, for its whole name over it with the fold mark (decision 308).
const GROUP_NEIGHBOUR_WIDTH: usize = NEIGHBOUR_WIDTH + GROUP_NAME_CHARS - 2;
/// The gap between the overview and the row inspector.
const INSPECTOR_GAP: usize = 1;
/// Room for the distance from the cursor, the row number, the playhead marker and a gap.
const GUTTER: usize = 9;
/// In another track's column, in its playhead's place before a note: more than one of its rows
/// starts in this line's time, and the first shows, so nothing is hidden without a sign. A plain
/// character, as a Nerd Font icon could be drawn over the note after it.
pub const MORE_STARTS: char = '\u{22ee}';
/// On a continuation line, under the note of a row that holds it through its pulses.
const HELD: char = '\u{2502}';

/// The rows that aren't steps: the heading, and the box's top and bottom. The box always has its
/// room, so nothing moves when Insert starts.
pub(crate) const ROWS_AROUND_STEPS: u16 = 3;

/// Room for a slot's command on screen, a label of up to 5 characters like `2FDBK`, and a gap.
const COMMAND_WIDTH: usize = 6;

/// A slot on screen: its command, coarse and fine value, its mark, and the gap before the next.
const SLOT_WIDTH: usize = COMMAND_WIDTH + 2 + FINE_WIDTH + MARK_WIDTH + 1;

/// The fine part of a value, after its coarse digits.
const FINE_WIDTH: usize = 2;

/// The gap and mark after a slot's value.
const MARK_WIDTH: usize = 2;
/// An empty value cell's mark in a step's text, which is worked on a byte to a character, so it's
/// drawn as `EMPTY` once the text is laid out.
const BLANK: char = '.';
/// How an empty value cell is drawn: faint middle dots, as Renoise and OpenMPT draw theirs, as
/// dashes line up with the beat and bar rules into one line and dots can't (#313). The note keeps
/// `---`, the tracker's sign that nothing plays.
const EMPTY: char = '\u{b7}';
/// How an empty velocity cell shows, which plays at the track's velocity.
const VELOCITY_EMPTY: &str = "..";
/// How an empty gate cell shows, which plays as one pulse of `SGL`.
const GATE_EMPTY: &str = "....";
/// How a gate cell shows with no length of its own, which plays at the track's `Gate`.
const LENGTH_EMPTY: &str = "..";
/// How an empty FX slot shows its command and its value: no `00`, as it has none.
const COMMAND_EMPTY: &str = "...";
const VALUE_EMPTY: &str = "..";
/// How wide a row's nudge is on the grid: a sign and two hex digits, as far as a track at a quarter
/// goes. A track whose nudges need one digit leaves the third blank.
const NUDGE_WIDTH: usize = 3;
/// Where the first FX slot starts: after the note, velocity, nudge and gate cell with its length.
const FX_START: usize = 19;

/// Where a row falls in the bar: on the first step of a bar or of a beat, or between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stress {
    Bar,
    Beat,
    Step,
}

impl Stress {
    /// The stress of a row that starts this many steps from the loop's start.
    fn at(time: isize) -> Self {
        if time.rem_euclid(BAR_STEPS as isize) == 0 {
            Self::Bar
        } else if time.rem_euclid(BEAT_STEPS as isize) == 0 {
            Self::Beat
        } else {
            Self::Step
        }
    }

    /// The rule under the row before one with this stress, which marks where its bar or beat
    /// begins: brighter for a bar than a beat, as a DAW's grid draws them, and none between steps.
    /// It's an underline, so it costs no row and moves nothing.
    fn rule(self) -> Option<Style> {
        let colour = match self {
            Self::Bar => theme::OVERLAY0,
            Self::Beat => theme::SURFACE1,
            Self::Step => return None,
        };
        Some(
            Style::new()
                .add_modifier(Modifier::UNDERLINED)
                .underline_color(colour),
        )
    }
}

/// A step's text on screen, naming a slot's settings from what's on this track.
fn step_text(step: &Step, track: Option<&Track>) -> String {
    let velocity = step
        .velocity
        .map_or(VELOCITY_EMPTY.to_owned(), |v| format!("{v:02X}"));
    let gate = step.gate.map_or_else(
        || GATE_EMPTY.to_owned(),
        |gate| format!("{}{}", gate.kind.name(), gate.pulses),
    );
    let length = step
        .length()
        .map_or_else(|| LENGTH_EMPTY.to_owned(), |length| format!("{length:02X}"));
    // A sign and the 24ths in as many hex digits as the track's nudges need, and nothing on the
    // grid, so only a nudged row's catches the eye.
    let most = track
        .map_or(Speed::Sixteenth, |track| track.speed)
        .most_nudge();
    let nudge = format!("{:<NUDGE_WIDTH$}", nudge_text(step.nudge, most));
    let mut text = format!("{} {velocity} {nudge} {gate} {length}", step.note);
    for slot in &step.fx {
        let name = match (slot.command, track) {
            (Some(command), Some(_)) => command.name(),
            (Some(command), None) => command.name().to_owned(),
            (None, _) => COMMAND_EMPTY.to_owned(),
        };
        let name: String = name.chars().take(COMMAND_WIDTH - 1).collect();
        let mark = if slot.is_value() {
            slot.mark.glyph()
        } else {
            ' '
        };
        // Only a continuous setting's value has a fine part. An event's or a list's is whole, and
        // an empty cell has none.
        let fine = if slot.command.is_some_and(FxCommand::has_fine) {
            format!("{:02X}", slot.fine())
        } else {
            "  ".to_owned()
        };
        // An `SLI` by a letter shows the letter, as `'a`, in its two digits' place.
        let value = match slot.slice_mark() {
            _ if slot.command.is_none() => VALUE_EMPTY.to_owned(),
            Some(letter) => format!("'{letter}"),
            None => format!("{:02X}", slot.coarse()),
        };
        text.push_str(&format!(" {name:<COMMAND_WIDTH$}{value}{fine} {mark}"));
    }
    text
}

/// The step as it will be once the value being typed is in: a command chosen in Insert waits for
/// its value before it's written, but shows in its cell meanwhile.
fn pending_step(ctx: &Ctx, step: &Step, typing: bool) -> Option<Step> {
    let (Field::FxValue(slot), Some(command)) = (ctx.cursor().field, ctx.entry().command) else {
        return None;
    };
    if !typing {
        return None;
    }
    let mut step = step.clone();
    step.fx.get_mut(slot)?.command = Some(command);
    Some(step)
}

/// Split text into runs of the same style, given a style for each character.
fn spans_of(text: &[char], styles: &[Style]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for end in 1..=text.len() {
        if end == text.len() || styles[end] != styles[start] {
            let run: String = text[start..end].iter().collect();
            spans.push(Span::styled(run, styles[start]));
            start = end;
        }
    }
    spans
}

/// The character range of a field within a step's text.
fn field_range(field: Field) -> (usize, usize) {
    let fx_start = |slot: usize| FX_START + slot * SLOT_WIDTH;
    match field {
        Field::Note => (0, 3),
        Field::Velocity => (4, 6),
        Field::Nudge => (7, 7 + NUDGE_WIDTH),
        Field::Gate => (11, 15),
        Field::GateLength => (16, 18),
        Field::FxCommand(slot) => (fx_start(slot), fx_start(slot) + COMMAND_WIDTH),
        Field::FxValue(slot) => (
            fx_start(slot) + COMMAND_WIDTH,
            fx_start(slot) + COMMAND_WIDTH + 2,
        ),
        Field::FxFine(slot) => (
            fx_start(slot) + COMMAND_WIDTH + 2,
            fx_start(slot) + COMMAND_WIDTH + 2 + FINE_WIDTH,
        ),
    }
}

/// The character range a selection covers on one step.
fn selected_columns(step: &Step, scope: Scope, length: usize) -> (usize, usize) {
    if scope.is_whole() {
        return (0, length);
    }
    let slots = step.fx.len();
    let last = scope.last.min(2 + slots);
    let (start, _) = field_range(Field::of_group(scope.first.min(last)));
    let end = match Field::of_group(last) {
        Field::FxCommand(slot) => field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH,
        Field::Velocity => field_range(Field::Nudge).1,
        Field::Gate => field_range(Field::GateLength).1,
        field => field_range(field).1,
    };
    (start.min(length), end.min(length))
}

/// The characters an FX slot takes in a step's text, with its mark.
fn slot_columns(slot: usize) -> (usize, usize) {
    let (start, _) = field_range(Field::FxCommand(slot));
    (
        start,
        field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH,
    )
}

/// The characters a live generator's cells take in a step's text: an FX column's by its command,
/// so its values keep their colour.
fn live_cells(column: ShapeColumn) -> (usize, usize) {
    match column {
        ShapeColumn::Note { .. } => field_range(Field::Note),
        ShapeColumn::Velocity => field_range(Field::Velocity),
        ShapeColumn::Nudge { .. } => field_range(Field::Nudge),
        ShapeColumn::Length => field_range(Field::GateLength),
        ShapeColumn::Fx { slot, .. } => field_range(Field::FxCommand(slot)),
    }
}

/// A live generator's cells on a step as the grid writes them, for the status line to say: a
/// note, `7F`, `CUT40`; a note with its velocity and gate where `heard` differs from the step in
/// those too, as an `Acid` line's accents and slides do.
pub(crate) fn generator_cell_text(
    step: &Step,
    heard: &Step,
    column: ShapeColumn,
    track: Option<&Track>,
) -> String {
    let text = step_text(step, track);
    let (start, end) = match column {
        ShapeColumn::Note { .. } if step.velocity != heard.velocity || step.gate != heard.gate => {
            (0, field_range(Field::GateLength).1)
        }
        ShapeColumn::Fx { slot, .. } => {
            let (start, end) = slot_columns(slot);
            (start, end - MARK_WIDTH)
        }
        column => live_cells(column),
    };
    let text = text.get(start..end.min(text.len())).unwrap_or("");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lay over a row's text what each live generator playing on it was last heard as there, where
/// that isn't what's written: a generator on the phrase's rows while the playhead is on it, one
/// on a lane while its lane's playhead is. Only the characters it changed are laid, so another's
/// cells and the rest of the row stay.
fn heard_over(
    ctx: &Ctx,
    phrase: &Phrase,
    index: usize,
    playhead: Option<usize>,
    lane_playheads: &[Option<usize>; FX_SLOTS],
    text: &mut String,
) {
    let Some(id) = ctx.phrase_id() else {
        return;
    };
    let Some(written) = phrase.steps().get(index) else {
        return;
    };
    let as_written: Vec<char> = step_text(written, ctx.track_here()).chars().collect();
    for (at, generator) in phrase.generators().iter().enumerate() {
        let playing = match (generator.span, generator.column) {
            (GenSpan::Lane, ShapeColumn::Fx { slot, .. }) => lane_playheads[slot] == Some(index),
            _ => playhead == Some(index),
        };
        if !playing {
            continue;
        }
        let Some((heard, _)) = ctx.heard_cell(ctx.cursor().track, id, at, index) else {
            continue;
        };
        let heard: Vec<char> = step_text(&heard, ctx.track_here()).chars().collect();
        let mut chars: Vec<char> = text.chars().collect();
        if heard.len() != chars.len() || as_written.len() != chars.len() {
            continue;
        }
        for (place, char) in heard.iter().enumerate() {
            if *char != as_written[place] {
                chars[place] = *char;
            }
        }
        *text = chars.into_iter().collect();
    }
}

/// Whether a step has a value in a live generator's column to mark: a note, a velocity, a nudge,
/// a length of its own, or an FX command.
fn live_value(step: &Step, column: ShapeColumn) -> bool {
    match column {
        ShapeColumn::Note { .. } => step.note != Note::Empty,
        ShapeColumn::Velocity => step.velocity.is_some(),
        ShapeColumn::Nudge { .. } => step.nudge != 0,
        ShapeColumn::Length => step.length().is_some(),
        ShapeColumn::Fx { slot, .. } => step.fx.get(slot).is_some_and(|fx| fx.command.is_some()),
    }
}

/// Which tracks fit in the overview beside the grid, in order, with the focused one among them and
/// those near it in view. Groups, returns and the master have their notes elsewhere, so they're
/// left out unless one is the track being edited. A folded group's tracks are left out too, with
/// a column of the group's own, named and empty, where the first of them would be (decision 128).
fn neighbours(ctx: &Ctx, width: usize) -> Vec<usize> {
    let song = ctx.song();
    let mut tracks: Vec<usize> = (0..song.tracks.len())
        .filter(|&index| {
            index == ctx.cursor().track
                || (song.tracks[index].is_sound() && !ctx.folds().hides(song, index))
                || ctx.folds().holds(song, index)
        })
        .collect();
    // In the order `W` and `B` go through them (decision 305).
    tracks.sort_by_key(|&track| ctx.folds().place(song, track, Order::Stops));
    let centre = tracks
        .iter()
        .position(|&track| track == ctx.cursor().track)
        .unwrap_or_else(|| tracks.partition_point(|&track| track < ctx.cursor().track));
    let widths: Vec<usize> = tracks
        .iter()
        .map(|&track| neighbour_width(song, track))
        .collect();
    tracks[centred_run(&widths, centre, width)].to_vec()
}

/// How wide another track's column is: a group's has room for its whole name.
fn neighbour_width(song: &Song, track: usize) -> usize {
    if song.tracks[track].is_group() {
        GROUP_NEIGHBOUR_WIDTH
    } else {
        NEIGHBOUR_WIDTH
    }
}

/// How wide the other tracks' columns are together.
fn neighbours_width(song: &Song, tracks: &[usize]) -> usize {
    tracks
        .iter()
        .map(|&track| neighbour_width(song, track))
        .sum()
}

/// The headings over the grid's parts, the note and each FX slot, numbered as the title's lanes
/// and the FX page's devices are, at the columns they start at.
fn section_headings() -> Vec<(usize, String)> {
    let mut headings = vec![(0, "Note".to_owned())];
    for slot in 0..FX_SLOTS {
        headings.push((
            field_range(Field::FxCommand(slot)).0,
            format!("FX {}", slot + 1),
        ));
    }
    headings
}

/// A frame's edge with names set into it: the line in the frame's style and the names in theirs.
fn frame_edge(text: &str, edge: Style, named: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let styles: Vec<Style> = chars
        .iter()
        .map(|c| {
            if "─╭╮".contains(*c) {
                edge
            } else {
                named
            }
        })
        .collect();
    spans_of(&chars, &styles)
}

/// The headings in a line of a width, each at its column on a fill, with a space either side of
/// it as a device's name is set into its frame, and cut where the line ends.
fn headings_line(width: usize, fill: char) -> String {
    let mut line = vec![fill; width];
    for (start, heading) in section_headings() {
        let end = start + heading.len();
        if start > 0 && start <= width {
            line[start - 1] = ' ';
        }
        if end < width {
            line[end] = ' ';
        }
        for (at, character) in heading.chars().enumerate() {
            if let Some(cell) = line.get_mut(start + at) {
                *cell = character;
            }
        }
    }
    line.into_iter().collect()
}

/// What another track's column shows: the phrase it's on and, if it's moving, where in that phrase.
struct Column<'a> {
    steps: &'a [Step],
    /// The row playing, and which of its pulses.
    playing: Option<(usize, usize)>,
    /// The note the row playing was heard as, where a moving generator played another than
    /// what's written (decision 300).
    heard: Option<Note>,
    /// When each row starts, in the track's own steps, and how many ticks its step lasts.
    starts: Vec<usize>,
    ticks: u64,
}

impl Column<'_> {
    /// How many ticks its loop lasts.
    fn length(&self) -> u64 {
        self.steps.iter().map(Step::pulses).sum::<usize>() as u64 * self.ticks
    }

    /// When the row playing starts, and where the playhead is, in ticks from row `00`.
    fn playing_ticks(&self) -> Option<(u64, u64)> {
        let (row, pulse) = self.playing?;
        let start = *self.starts.get(row)? as u64;
        Some((start * self.ticks, (start + pulse as u64) * self.ticks))
    }
}

/// The phrase a track is on now, or the one that would play from the cursor while stopped. A
/// group its tracks are folded into has none to show.
fn column<'a>(ctx: &Ctx<'a>, track: usize) -> Option<Column<'a>> {
    if track != ctx.cursor().track && ctx.folds().holds(ctx.song(), track) {
        return None;
    }
    let owner = ctx.song().tracks.get(track)?;
    let chain = (*ctx.section_here()?.chains.get(track)?)?;
    let moving = ctx.playing_place(track, chain);
    let place =
        moving.unwrap_or_else(|| ctx.step_on(track, ctx.cursor().track, ctx.section_step()));
    let position = owner.chain_position(chain, place, ctx.dealer(track))?;
    let held = owner.phrases.get(&position.phrase)?;
    let steps = held.loop_steps();
    let heard = moving
        .and_then(|_| ctx.heard_step(track, position.phrase, held.start() + position.step))
        .map(|step| step.note);
    Some(Column {
        steps,
        playing: moving.map(|_| (position.step, position.pulse)),
        heard,
        starts: timeline::starts(steps),
        ticks: owner.speed.ticks(),
    })
}

/// Where a line of the grid is, for the other tracks' columns: a row, collapsed, or the ticks from
/// row `00` it covers in the timeline, if it covers any.
enum At {
    Row(isize),
    Ticks(Option<Range<u64>>),
}

/// The other tracks' columns on one line. Row `00` is each track's own loop start, and past its
/// end the column is blank, so it shows the track's real length. In the timeline, a line shows
/// the row each track starts in its time, with a mark before it when more than one does, and
/// nothing when none does, as a slower track leaves lines blank.
fn column_spans(
    song: &Song,
    columns: &[Option<Column>],
    tracks: &[usize],
    at: &At,
    selected: Option<(TrackSpan, isize, isize)>,
) -> Vec<Span<'static>> {
    columns
        .iter()
        .zip(tracks)
        .flat_map(|(column, track)| {
            // A group's wider column has the room after its gap.
            let gap = Span::raw(" ".repeat(neighbour_width(song, *track) - NEIGHBOUR_WIDTH + 1));
            let Some(column) = column else {
                return [Span::raw(" "), Span::raw("   "), gap];
            };
            // The row shown, whether the playhead is in this line, and whether more rows start.
            let (shown, playing, more) = match at {
                At::Row(row) => {
                    let shown = usize::try_from(*row)
                        .ok()
                        .filter(|row| *row < column.steps.len());
                    let playing = shown.is_some() && column.playing.map(|(row, _)| row) == shown;
                    (shown, playing, false)
                }
                At::Ticks(None) => (None, false, false),
                At::Ticks(Some(within)) => {
                    let found =
                        timeline::starting_in(column.steps, &column.starts, column.ticks, within);
                    let ticks = column.playing_ticks();
                    let playing = ticks.is_some_and(|(_, now)| within.contains(&now));
                    // The playhead's row shows where it starts, over another starting there.
                    let playing_row = column
                        .playing
                        .zip(ticks)
                        .filter(|(_, (start, _))| playing && within.contains(start))
                        .map(|((row, _), _)| row);
                    let shown = playing_row.or(found.map(|(first, _)| first));
                    (shown, playing, found.is_some_and(|(_, count)| count > 1))
                }
            };
            let in_selection = shown.is_some_and(|row| {
                selected.is_some_and(|(span, first, last)| {
                    span.contains(*track) && (first..=last).contains(&(row as isize))
                })
            });
            // The row playing shows the note heard, where a moving generator played another.
            let written = shown.map(|row| match column.heard {
                Some(heard) if playing && column.playing.map(|(at, _)| at) == Some(row) => heard,
                _ => column.steps[row].note,
            });
            let note = written.map(|note| note.to_string());
            // Empty is faint, as on the grid, so where the other tracks play stands out.
            let look = if written == Some(Note::Empty) {
                theme::faint()
            } else {
                theme::dim()
            };
            let mark = if more { MORE_STARTS } else { ' ' };
            // The marker's cell, the note's, then the gap.
            let (marker, note) = match note {
                // Playing, a note is lit and an empty row's `---` dimmed, as on the grid.
                Some(note) if playing => {
                    let lit = if written == Some(Note::Empty) {
                        playing_empty()
                    } else {
                        theme::active()
                    };
                    (Span::styled(">", theme::active()), Span::styled(note, lit))
                }
                // The playhead is on a row that started in an earlier line.
                None if playing => (Span::styled(">", theme::active()), Span::raw("   ")),
                Some(note) if in_selection => (
                    Span::styled(mark.to_string(), theme::selected()),
                    Span::styled(note, theme::selected()),
                ),
                Some(note) => (
                    Span::styled(mark.to_string(), look),
                    Span::styled(note, look),
                ),
                None => (Span::raw(" "), Span::raw("   ")),
            };
            [marker, note, gap]
        })
        .collect()
}

/// Which of a chain's phrases fit in the title's room, around the one being edited: as many as fit
/// either side of it, with two cells for `…` and its gap where the strip is cut. Each is its
/// width, with the gap after it.
fn strip_window(widths: &[usize], here: usize, room: usize) -> Range<usize> {
    let cut = |range: &Range<usize>| {
        let ends = usize::from(range.start > 0) + usize::from(range.end < widths.len());
        widths[range.clone()].iter().sum::<usize>() + 2 * ends
    };
    let here = here.min(widths.len().saturating_sub(1));
    // Out from it a phrase at a time, one side and then the other, so it stays in the middle
    // until an end of the chain is reached.
    let mut shown = here..(here + 1).min(widths.len());
    let mut grew = true;
    while grew {
        grew = false;
        let right = shown.start..shown.end + 1;
        if shown.end < widths.len() && cut(&right) <= room {
            shown = right;
            grew = true;
        }
        let left = shown.start.saturating_sub(1)..shown.end;
        if shown.start > 0 && cut(&left) <= room {
            shown = left;
            grew = true;
        }
    }
    shown
}

/// The chain in the title, in place of a count: its phrases in order, the one being edited in the
/// cursor's colour and the rest dim, a phrase played in more than one place with its link, and
/// `…` where it's cut to its room, so `]p` and `[p` are seen moving along it (B3 of the study).
fn chain_strip(ctx: &Ctx, room: usize) -> Vec<Span<'static>> {
    let entries = ctx.chain_entries();
    let items: Vec<(String, bool)> = entries
        .iter()
        .map(|entry| {
            let linked = ctx.phrase_uses(entry.phrase).len() > 1;
            let mark = if linked {
                glyphs::LINK.to_string()
            } else {
                String::new()
            };
            (format!("{:02X}{mark}", entry.phrase), linked)
        })
        .collect();
    let widths: Vec<usize> = items
        .iter()
        .map(|(text, _)| text.chars().count() + 1)
        .collect();
    let shown = strip_window(&widths, ctx.cursor().slot, room);
    let here = Style::new()
        .fg(theme::ROSEWATER)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![];
    if shown.start > 0 {
        spans.push(Span::styled("\u{2026} ", theme::dim()));
    }
    for at in shown.clone() {
        let (text, linked) = &items[at];
        let look = if at == ctx.cursor().slot {
            here
        } else {
            theme::dim()
        };
        spans.push(Span::styled(text.clone(), look));
        // The space after a link is one it isn't drawn over.
        let gap = if *linked { glyphs::ICON_SPACE } else { ' ' };
        if at + 1 < shown.end || shown.end < items.len() {
            spans.push(Span::raw(gap.to_string()));
        }
    }
    if shown.end < items.len() {
        spans.push(Span::styled("\u{2026}", theme::dim()));
    }
    spans
}

/// An empty cell on the row playing: the playhead's green pushed toward the screen, as the part of
/// a bar that isn't solid is.
fn playing_empty() -> Style {
    Style::new().fg(theme::dimmed(theme::GREEN))
}

/// How far a row is from the cursor's, as the count to type before `j` or `k`, so it is decimal.
/// The cursor's own row is blank.
fn relative_number(index: usize, cursor: usize) -> String {
    match index.abs_diff(cursor) {
        0 => "   ".to_owned(),
        // Long phrases can reach three digits, which then use up the gap.
        distance => format!("{:<3}", format!("{distance:>2}")),
    }
}

/// A row's number. Row `00` is the loop's first step, and rows above it are negative.
fn row_number(row: isize) -> String {
    let sign = if row < 0 { '-' } else { ' ' };
    format!("{sign}{:02X}", row.unsigned_abs())
}

/// The glyph for a track that's muted or soloed, if it is, and its style: the active colour for
/// its own, or its group's colour for its group's. A mute wins, as the track is silent, and one of
/// its own wins over its group's. A mute or solo waiting for the launch grid is the glyph for
/// waiting, dim, until it switches.
pub(crate) fn level_glyph(ctx: &Ctx, track: usize) -> Option<(char, Style)> {
    let jam = ctx.jam().get(track).copied().unwrap_or_default();
    if jam.mute_waiting || jam.solo_waiting {
        return Some((glyphs::WAITING, theme::dim()));
    }
    let (song, levels) = (ctx.song(), ctx.levels());
    let (muted, soloed) = (
        group::muted(song, levels, track),
        group::soloed(song, levels, track),
    );
    let order = [
        (muted == Switched::Own, glyphs::MUTE, muted),
        (muted.is_on(), glyphs::MUTE, muted),
        (soloed == Switched::Own, glyphs::SOLO, soloed),
        (soloed.is_on(), glyphs::SOLO, soloed),
    ];
    order
        .into_iter()
        .find(|(on, _, _)| *on)
        .map(|(_, glyph, why)| (glyph, why.style()))
}

/// A track's label over its column in the overview, three cells wide, or six for a group's whole
/// name: the start of its name, and its mute or solo glyph after it if it has one, or a group's
/// mark that its tracks are folded into it. Without a glyph, the name fills the cells.
fn column_label(ctx: &Ctx, track: usize) -> (String, Option<(char, Style)>) {
    let name = ctx.track_name(track);
    let cells = neighbour_width(ctx.song(), track) - 2;
    let folded = (track != ctx.cursor().track && ctx.folds().holds(ctx.song(), track)).then(|| {
        let style = group::style(ctx.song(), track).unwrap_or_default();
        (glyphs::FOLDED, style)
    });
    match level_glyph(ctx, track).or(folded) {
        Some(glyph) => (name.chars().take(cells - 1).collect(), Some(glyph)),
        None => (
            format!("{:<cells$}", name.chars().take(cells).collect::<String>()),
            None,
        ),
    }
}

/// A continuation line of a row, one of the lines of its pulses after its first, in the timeline:
/// dim, with a line under a note the row holds, the note again where a `MLT` plays it on each
/// pulse, and nothing for a rest or a row with no note. The FX band, a selection of the row and
/// the rule under it carry on through it, so the row reads as one block.
fn continuation(
    step: &Step,
    width: usize,
    selected: Option<Scope>,
    lit: bool,
    under: Option<Style>,
) -> Vec<Span<'static>> {
    let mut text = vec![' '; width];
    let mut styles = vec![Style::new(); width];
    let (start, end) = field_range(Field::Note);
    if let Note::On(_) = step.note
        && end <= width
    {
        let look = if lit { theme::active() } else { theme::dim() };
        match step.gate_kind() {
            GateKind::Multi => {
                for (cell, character) in step.note.to_string().chars().enumerate() {
                    text[start + cell] = character;
                }
                styles[start..end].fill(look);
            }
            GateKind::Rest => {}
            GateKind::Single | GateKind::Hold => {
                text[start + 1] = HELD;
                styles[start + 1] = if lit { look } else { theme::faint() };
            }
        }
    }
    for style in styles.iter_mut().skip(FX_START) {
        *style = style.bg(theme::FX_BAND);
    }
    if let Some(scope) = selected {
        let (from, to) = selected_columns(step, scope, width);
        styles[from..to].fill(theme::selected());
    }
    if let Some(under) = under {
        for style in &mut styles {
            *style = style.patch(under);
        }
    }
    spans_of(&text, &styles)
}

/// The rows there are lines for: a short phrase still has rows for the longer tracks beside it, a
/// line each past its end.
fn rows_shown(phrase: &Phrase, columns: &[Option<Column>], timeline_on: bool, ticks: u64) -> usize {
    let extent = phrase.extent();
    let stored = phrase.steps().len();
    if timeline_on {
        let stored_lines: usize = (0..stored)
            .map(|index| timeline::row_lines(phrase, index, true))
            .sum();
        let needed = columns
            .iter()
            .flatten()
            .map(|column| extent.start + column.length().div_ceil(ticks) as usize)
            .fold(stored_lines, usize::max);
        stored + (needed - stored_lines)
    } else {
        columns
            .iter()
            .flatten()
            .map(|column| extent.start + column.steps.len())
            .fold(stored, usize::max)
    }
}

/// The title in groups two spaces apart, as the track page's is: the track, with its machine or
/// kind as a glyph before it and whether it's muted or soloed after; its chain, with the phrases
/// along it; then what's said about this one.
fn title(ctx: &Ctx, phrase: &Phrase, width: u16) -> Line<'static> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut title = vec![];
    if let Some(track) = ctx.track_here() {
        title.push(Span::styled(
            format!("{} ", glyphs::track(track)),
            theme::dim(),
        ));
    }
    title.push(Span::styled("Track ", bold));
    title.extend(role::ruled(
        ctx.song(),
        ctx.cursor().track,
        &ctx.track_name(ctx.cursor().track),
        bold,
    ));
    // A track in a group carries its mark, in the group's colour, as on the mixer.
    if let Some(mark) = group::mark(ctx.song(), ctx.cursor().track) {
        title.extend([Span::raw(" "), mark.patch_style(bold)]);
    }
    if let Some((glyph, style)) = level_glyph(ctx, ctx.cursor().track) {
        title.push(Span::styled(format!(" {glyph}"), style));
    }
    let chain = ctx.chain_id().unwrap_or(0);
    title.push(Span::styled(format!("  Chain {chain:02X}  "), bold));
    let after = title_details(ctx, phrase);
    // The strip has what's left of the row once the rest of the title and the CPU figure at its
    // right are in.
    let cells = |spans: &[Span]| {
        spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum::<usize>()
    };
    let (system, _) = crate::screen::system::text(0, 0, 0);
    let room = usize::from(width)
        .saturating_sub(cells(&title) + cells(&after) + system.chars().count() + 2);
    title.extend(chain_strip(ctx, room));
    title.extend(after);
    Line::from(title)
}

/// What the title says about the phrase after the chain: an order other than forward and its
/// lanes' lengths, its live columns, and how many phrases follow it.
fn title_details(ctx: &Ctx, phrase: &Phrase) -> Vec<Span<'static>> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    // What's said about the phrase after the chain, each a group of its own, dim as a device
    // title's second words are: an order other than forward, and its lanes' lengths.
    let mut details = vec![];
    if !phrase.order().is_forward() {
        details.push(phrase.order().to_string());
    }
    let lanes: Vec<String> = (0..FX_SLOTS)
        .filter_map(|slot| Some(format!("FX{}:{}", slot + 1, phrase.lane(slot)?.length)))
        .collect();
    if !lanes.is_empty() {
        details.push(lanes.join(" "));
    }
    // Its live columns are named, in the shape panel's colour, as their cells are drawn, each with
    // `~` after it where what was heard of it isn't what's written (decision 300).
    let mut live_names: Vec<String> = phrase
        .generators()
        .iter()
        .enumerate()
        .map(|(at, generator)| {
            let moving = ctx
                .phrase_id()
                .is_some_and(|id| ctx.generator_moving(ctx.cursor().track, id, at));
            let name = crate::services::generators::generator_label(generator);
            if moving { format!("{name}~") } else { name }
        })
        .collect();
    live_names.dedup();
    // How many phrases follow this one, live, on any track.
    let followers = ctx.phrase_id().map_or(0, |id| {
        ctx.song()
            .followers(Source {
                track: ctx.cursor().track,
                phrase: id,
            })
            .len()
    });
    let mut after = vec![];
    if !details.is_empty() {
        after.push(Span::styled(
            format!("  {}", details.join("  ")),
            theme::dim(),
        ));
    }
    if !live_names.is_empty() {
        after.push(Span::styled(
            format!("  {} {}", glyphs::LIVE, live_names.join(",")),
            bold.fg(theme::TEAL),
        ));
    }
    if followers > 0 {
        after.push(Span::styled(
            format!("  Followed by {followers}"),
            Style::new().fg(theme::TEAL),
        ));
    }
    after
}

/// The grid's frame's side.
const SIDE: &str = "│";

/// The grid as it's laid out for a frame, worked out once before its lines are drawn: the phrase
/// and the lines of it in view, the selection and the playheads on it, its width, and the other
/// tracks' columns beside it.
struct Grid<'a> {
    phrase: &'a Phrase,
    extent: Extent,
    /// How many rows the phrase stores.
    stored: usize,
    /// How many lines of it fit.
    height: usize,
    selection: Option<Selection>,
    /// The selection's tracks and its rows from the loop's start, for the other tracks' columns.
    across: Option<(TrackSpan, isize, isize)>,
    timeline_on: bool,
    lane_playheads: [Option<usize>; FX_SLOTS],
    /// The playhead's row, and the line it's on: the line of its pulse in the timeline.
    playhead: Option<usize>,
    playhead_line: Option<(usize, usize)>,
    /// How wide the rows' text is, at its widest.
    body_width: usize,
    /// How wide the row inspector is, while it's shown.
    inspector_width: Option<usize>,
    /// The other tracks in the overview, and what each one's column shows.
    others: Vec<usize>,
    columns: Vec<Option<Column<'a>>>,
    /// How long a line is, in ticks: one of this track's steps.
    ticks: u64,
    /// Every line, each a row's own or one of its continuations, with when it starts, so a beat
    /// line is one that starts on a beat even when rows before it last several pulses.
    time_lines: Vec<TimeLine>,
    /// The first line on screen, and the first past the phrase's rows.
    first_line: usize,
    stored_lines: usize,
    /// The rows each live generator writes, and its column.
    live: Vec<(StepRange, ShapeColumn)>,
    /// The frame's edge, and the grid's parts named in it.
    edge: Style,
    named: Style,
}

impl<'a> Grid<'a> {
    /// Lay the grid out in an area.
    fn new(ctx: &Ctx<'a>, view: &PhraseView, phrase: &'a Phrase, area: Rect) -> Self {
        let height = usize::from(area.height.saturating_sub(ROWS_AROUND_STEPS)).max(1);
        let top = view.top;
        let selection = ctx
            .mode()
            .is_visual()
            .then(|| view.selection(ctx.mode(), ctx.cursor()));
        let selection = selection.or(ctx.held_selection());
        let across = selection.map(|s| {
            let start = ctx.extent().start as isize;
            (
                s.tracks,
                s.range.first as isize - start,
                s.range.last as isize - start,
            )
        });
        let timeline_on = view.timeline_on();
        let lane_playheads = ctx.lane_playheads(phrase);
        let extent = phrase.extent();
        let stored = phrase.steps().len();
        // The playhead's row, and the line it's on: the line of its pulse in the timeline.
        let playhead_place = ctx.playhead_place();
        let playhead = playhead_place.map(|(step, _)| step);
        let playhead_line = playhead_place.map(|(step, pulse)| {
            let last = timeline::row_lines(phrase, step, timeline_on) - 1;
            (step, pulse.min(last))
        });
        let body_width = phrase
            .steps()
            .iter()
            .map(|step| step_text(step, ctx.track_here()).len())
            .max()
            .unwrap_or(0);
        let room = usize::from(area.width).saturating_sub(GUTTER + body_width + 2);
        // The row inspector takes the room right of the overview while every track's column still
        // fits beside it, and gives way to the overview when they need the room.
        let every = neighbours_width(ctx.song(), &neighbours(ctx, usize::MAX));
        let inspector_width = view
            .inspector
            .then(|| room.saturating_sub(every + INSPECTOR_GAP))
            .filter(|width| *width >= usize::from(inspector::NARROWEST))
            .map(|width| width.min(usize::from(inspector::WIDEST)));
        let others = neighbours(
            ctx,
            room - inspector_width.map_or(0, |width| width + INSPECTOR_GAP),
        );
        let columns: Vec<_> = others.iter().map(|track| column(ctx, *track)).collect();
        // How long a line is, in ticks: one of this track's steps.
        let ticks = ctx
            .track_here()
            .map_or(TICKS_PER_STEP, |track| track.speed.ticks());
        let rows = rows_shown(phrase, &columns, timeline_on, ticks);
        // Every line, each a row's own or one of its continuations, with when it starts, so a beat
        // line is one that starts on a beat even when rows before it last several pulses.
        let time_lines = timeline::lines(phrase, timeline_on, rows);
        let first_line = time_lines
            .iter()
            .position(|line| line.index >= top)
            .unwrap_or(time_lines.len());
        let stored_lines = time_lines
            .iter()
            .position(|line| line.index >= stored)
            .unwrap_or(time_lines.len());
        let live: Vec<(StepRange, ShapeColumn)> = phrase
            .generators()
            .iter()
            .filter_map(|generator| Some((generator.rows()?, generator.column)))
            .collect();
        // The grid is a device, framed as the track page's are, its parts named in its top edge:
        // dim while it's looked at, as a device without the cursor is, and green in Insert, so
        // Insert lights the frame rather than drawing a box that wasn't there. It's always there,
        // so nothing moves when Insert starts.
        let insert = ctx.mode() == Mode::Insert;
        let edge = Style::new().fg(if insert {
            theme::GREEN
        } else {
            theme::SURFACE1
        });
        let named = if insert { edge } else { theme::dim() };
        Self {
            phrase,
            extent,
            stored,
            height,
            selection,
            across,
            timeline_on,
            lane_playheads,
            playhead,
            playhead_line,
            body_width,
            inspector_width,
            others,
            columns,
            ticks,
            time_lines,
            first_line,
            stored_lines,
            live,
            edge,
            named,
        }
    }

    /// The rule under a line, from the stress of the line after it, where a line falls in the bar:
    /// by when it starts while it plays, and by its number outside the loop, where it doesn't
    /// play, or on from the loop's end in the timeline. The loop's last line is followed by its
    /// first, which starts a bar, and the phrase's last line by nothing.
    fn rule_under(&self, at: usize) -> Option<Style> {
        let line = self.time_lines[at];
        let next = self.time_lines.get(at + 1)?;
        if next.index >= self.stored {
            None
        } else if self.extent.plays(line.index) && !self.extent.plays(next.index) {
            Stress::Bar.rule()
        } else {
            Stress::at(next.time).rule()
        }
    }

    /// The other tracks' columns on a line, which is a row of theirs collapsed, or the ticks it
    /// covers in the timeline.
    fn overview(&self, song: &Song, line: &TimeLine) -> Vec<Span<'static>> {
        let at = if self.timeline_on {
            At::Ticks(timeline::ticks_of(line, self.ticks))
        } else {
            At::Row(self.extent.row(line.index))
        };
        column_spans(song, &self.columns, &self.others, &at, self.across)
    }

    /// The playhead's marker before a line, if it's on it.
    fn marker(&self, line: &TimeLine) -> &'static str {
        if self.playhead_line == Some((line.index, line.pulse)) {
            ">"
        } else {
            " "
        }
    }

    /// The headings row: the frame's top edge with the grid's parts in it, and each other track's
    /// label over its column's note, as `Note` is over the notes.
    fn headings(&self, ctx: &Ctx) -> Line<'static> {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let mut top_rule = vec![Span::raw(" ".repeat(GUTTER - 1))];
        top_rule.extend(frame_edge(
            &format!("╭{}╮", headings_line(self.body_width, '─')),
            self.edge,
            self.named,
        ));
        top_rule.push(Span::raw(" "));
        for track in &self.others {
            // The track being edited is marked out, so the overview shows every track in its
            // place, and a group and its tracks are in its colour.
            let (label, glyph) = column_label(ctx, *track);
            top_rule.push(Span::raw(" "));
            let marked =
                (*track == ctx.cursor().track).then(|| bold.fg(theme::BASE).bg(theme::GREEN));
            let named = group::style(ctx.song(), *track).map_or(bold, |tint| bold.patch(tint));
            let used = label.chars().count() + usize::from(glyph.is_some());
            top_rule.extend(role::ruled(
                ctx.song(),
                *track,
                &label,
                marked.unwrap_or(named),
            ));
            let gap = match glyph {
                Some((glyph, style)) => {
                    top_rule.push(Span::styled(glyph.to_string(), marked.unwrap_or(style)));
                    glyphs::ICON_SPACE
                }
                None => ' ',
            };
            // The gap, and the rest of the column after a glyph beside a short name.
            let rest = neighbour_width(ctx.song(), *track) - 2 - used;
            top_rule.push(Span::raw(format!("{gap}{}", " ".repeat(rest))));
        }
        Line::from(top_rule)
    }

    /// The grid's lines on screen, and its frame's bottom edge where it's in view.
    fn lines(&self, ctx: &Ctx) -> Vec<Line<'static>> {
        let mut lines = vec![];
        let shown = self
            .time_lines
            .iter()
            .enumerate()
            .skip(self.first_line)
            .take(self.height);
        for (at_line, line) in shown {
            lines.push(match self.phrase.steps().get(line.index) {
                None => self.past_end(ctx, at_line, line),
                Some(step) if line.pulse > 0 => self.continued(ctx, at_line, line, step),
                Some(step) => self.row(ctx, at_line, line, step),
            });
        }
        if !(self.first_line..self.first_line + self.height).contains(&self.stored_lines)
            || self.stored_lines >= self.time_lines.len()
        {
            let inside = "─".repeat(self.body_width);
            let indent = " ".repeat(GUTTER - 1);
            lines.push(Line::styled(format!("{indent}╰{inside}╯"), self.edge));
        }
        lines
    }

    /// A line past the phrase's rows, for the longer tracks beside it. The frame closes on the
    /// first.
    fn past_end(&self, ctx: &Ctx, at_line: usize, line: &TimeLine) -> Line<'static> {
        let row = self.extent.row(line.index);
        let (left, fill, right) = if at_line == self.stored_lines {
            ("╰", "─", "╯")
        } else {
            (" ", " ", " ")
        };
        let number = if line.time.rem_euclid(BEAT_STEPS as isize) == 0 {
            theme::dim()
        } else {
            theme::faint()
        };
        let mut spans = vec![
            Span::raw("   "),
            Span::styled(row_number(row), number),
            Span::raw("  "),
            Span::styled(
                format!("{left}{}{right}", fill.repeat(self.body_width)),
                self.edge,
            ),
            Span::raw(" "),
        ];
        spans.extend(self.overview(ctx.song(), line));
        Line::from(spans)
    }

    /// A continuation line of a row, one of the lines of its pulses after its first.
    fn continued(&self, ctx: &Ctx, at_line: usize, line: &TimeLine, step: &Step) -> Line<'static> {
        let index = line.index;
        let selected = self
            .selection
            .filter(|s| (s.range.first..=s.range.last).contains(&index))
            .map(|s| s.scope);
        let mut spans = vec![
            Span::raw("      "),
            Span::styled(format!(" {}", self.marker(line)), theme::active()),
            Span::styled(SIDE, self.edge),
        ];
        let width = step_text(step, ctx.track_here()).len();
        let lit = self.playhead == Some(index);
        spans.extend(continuation(
            step,
            width,
            selected,
            lit,
            self.rule_under(at_line),
        ));
        spans.push(Span::styled(
            " ".repeat(self.body_width.saturating_sub(width)),
            self.rule_under(at_line).unwrap_or_default(),
        ));
        spans.push(Span::styled(SIDE, self.edge));
        spans.push(Span::raw(" "));
        spans.extend(self.overview(ctx.song(), line));
        Line::from(spans)
    }

    /// A row's own line, its first: its numbers, then its cells as written, as they're being
    /// typed or as they were heard, lit where it plays, with the selection and the cursor on it,
    /// then the other tracks' columns beside it.
    fn row(&self, ctx: &Ctx, at_line: usize, line: &TimeLine, step: &Step) -> Line<'static> {
        let index = line.index;
        let row = self.extent.row(index);
        let typing =
            index == ctx.cursor().step && matches!(ctx.mode(), Mode::Insert | Mode::Replace);
        let pending = pending_step(ctx, step, typing);
        let step = pending.as_ref().unwrap_or(step);
        let text = self.row_text(ctx, step, index, typing);
        let plays = self.extent.plays(index);
        let on_beat = Stress::at(line.time) != Stress::Step;
        let number = match (plays, on_beat) {
            (true, true) => Style::new().fg(theme::YELLOW),
            (true, false) => Style::new().fg(theme::SUBTEXT1),
            (false, true) => theme::dim(),
            (false, false) => theme::faint(),
        };
        let mut spans = vec![
            Span::styled(relative_number(index, ctx.cursor().step), theme::faint()),
            Span::styled(row_number(row), number),
            Span::styled(format!(" {}", self.marker(line)), theme::active()),
            Span::styled(SIDE, self.edge),
        ];
        // A row its `CHA` or `CND` skipped this time is faint under the playhead, so the logic
        // can be followed.
        let skipped = ctx.row_skipped();
        let body = if self.playhead == Some(index) && skipped {
            theme::faint()
        } else if self.playhead == Some(index) {
            theme::active()
        } else if plays {
            Style::new()
        } else {
            theme::faint()
        };
        // What's empty is faint, as a code editor dims whitespace, so what's written stands out.
        // On the row playing it's the playhead's green set back, so the row reads as one lit bar
        // that keeps its shape, and an empty row isn't the brightest thing on screen.
        let lit = self.playhead == Some(index) && !skipped;
        let empty = empty_cells(step);
        let mut styles = written_styles(ctx, step, text.len(), body, lit, plays, &empty);
        self.fx_styles(ctx, step, index, &mut styles);
        let shown = marked_text(step, &text, &empty, &mut styles);
        self.select(ctx, step, index, text.len(), &mut styles);
        // The rule runs under the whole row, over whatever else its cells show.
        let under = self.rule_under(at_line);
        if let Some(under) = under {
            for style in &mut styles {
                *style = style.patch(under);
            }
        }
        spans.extend(spans_of(&shown, &styles));
        let padding = self.body_width.saturating_sub(text.len());
        spans.push(Span::styled(" ".repeat(padding), under.unwrap_or_default()));
        spans.push(Span::styled(SIDE, self.edge));
        spans.push(Span::raw(" "));
        spans.extend(self.overview(ctx.song(), line));
        Line::from(spans)
    }

    /// A row's text: as written, with what's being typed in its place, the cells of a generator
    /// as they were heard, the note heard on the row playing, and a lane's playhead.
    fn row_text(&self, ctx: &Ctx, step: &Step, index: usize, typing: bool) -> String {
        let mut text = step_text(step, ctx.track_here());
        // A label being typed shows in its command's place, and the first digit of a value in its.
        if typing
            && let Field::FxCommand(_) = ctx.cursor().field
            && !ctx.entry().text.is_empty()
        {
            let (start, end) = field_range(ctx.cursor().field);
            if end <= text.len() {
                let typed = ctx.entry().text.shown();
                text.replace_range(start..end, &format!("{typed:<width$}", width = end - start));
            }
        }
        // The first digit of a value being typed shows in its place, and a nudge's sign and its
        // first digit where it takes two.
        let nudge = ctx.cursor().field == Field::Nudge && !ctx.entry().text.is_empty();
        if typing
            && matches!(
                ctx.cursor().field,
                Field::FxValue(_)
                    | Field::FxFine(_)
                    | Field::Velocity
                    | Field::Nudge
                    | Field::GateLength
            )
            && (ctx.entry().text.len() == 1 || nudge)
        {
            let (start, end) = field_range(ctx.cursor().field);
            if end <= text.len() {
                let typed = ctx.entry().text.shown();
                text.replace_range(start..end, &format!("{typed:<width$}", width = end - start));
            }
        }
        // The row playing shows each moving generator's cells on it as they were heard, and a
        // lane's generator the row its lane plays, as the grid otherwise shows what's written
        // (decision 300).
        if !typing {
            heard_over(
                ctx,
                self.phrase,
                index,
                self.playhead,
                &self.lane_playheads,
                &mut text,
            );
        }
        // The row playing shows the note heard where an accumulator moved it, as the note column
        // otherwise shows notes as written.
        if self.playhead == Some(index)
            && matches!(step.note, Note::On(_))
            && let Some(heard) = ctx.heard_note()
        {
            let (start, end) = field_range(Field::Note);
            if end <= text.len() {
                text.replace_range(start..end, &Note::On(heard).to_string());
            }
        }
        // A lane's playhead goes in the gap before its column.
        for (slot, at) in self.lane_playheads.iter().enumerate() {
            let before = slot_columns(slot).0 - 1;
            if *at == Some(index) && text.get(before..=before) == Some(" ") {
                text.replace_range(before..=before, ">");
            }
        }
        text
    }

    /// Colour a row's FX: a column with a lane by its own rows, a live generator's cells, a lock
    /// for what the track doesn't have, the values, and the band they sit on.
    fn fx_styles(&self, ctx: &Ctx, step: &Step, index: usize, styles: &mut [Style]) {
        let len = styles.len();
        // A column with a lane plays its own rows: they're lit, and the rest of it is dim.
        for (slot, lane_playhead) in self.lane_playheads.iter().enumerate() {
            let Some(lane) = self.phrase.lane(slot) else {
                continue;
            };
            let (start, end) = slot_columns(slot);
            let end = end.min(len);
            let style = if *lane_playhead == Some(index) {
                theme::active()
            } else if lane.contains(index) {
                Style::new()
            } else {
                theme::faint()
            };
            if start < end {
                styles[start - 1..end].fill(style);
            }
        }
        // A live generator's cells are in the shape panel's colour, an FX column by its command,
        // so its values keep theirs.
        for (rows, column) in &self.live {
            let (start, end) = live_cells(*column);
            if (rows.first..=rows.last).contains(&index) && live_value(step, *column) {
                let end = end.min(len);
                for style in &mut styles[start.min(end)..end] {
                    *style = style.fg(theme::TEAL);
                }
            }
        }
        // A lock for a voice or an effect this track doesn't have now stays, grey with no colour
        // and brighter than an empty cell, so it isn't typed over. It plays again when its owner is
        // back.
        if let Some(track) = ctx.track_here() {
            for (slot, fx) in step.fx.iter().enumerate() {
                if fx.command.is_some_and(|command| !command.exists_on(track)) {
                    let (start, end) = slot_columns(slot);
                    let end = end.min(len);
                    if start < end {
                        styles[start..end].fill(theme::dim());
                    }
                }
            }
        }
        // A setting's value is in the value colour, its coarse digits bright and bold and its fine
        // ones set back. An event's is in its own colour, as it isn't a setting. Rows already
        // coloured keep theirs.
        for (slot, fx) in step.fx.iter().enumerate() {
            if fx.command.is_none() {
                continue;
            }
            let hue = if fx.is_value() {
                theme::MAUVE
            } else {
                theme::PEACH
            };
            let (start, end) = field_range(Field::FxValue(slot));
            let coarse = Style::new().fg(hue).add_modifier(Modifier::BOLD);
            let fine = Style::new().fg(theme::fine(hue));
            for (range, look) in [(start..end, coarse), (end..end + FINE_WIDTH, fine)] {
                for style in &mut styles[range] {
                    if style.fg.is_none() {
                        *style = style.patch(look);
                    }
                }
            }
        }
        // The FX sit on a band a shade darker than the screen, so the two parts read apart.
        // A selection's or the cursor's colour goes over it.
        for style in styles.iter_mut().skip(FX_START) {
            if style.bg.is_none() {
                *style = style.bg(theme::FX_BAND);
            }
        }
    }

    /// Put the selection over a row, where it reaches it, and the cursor over that, so the
    /// cursor stays visible inside a selection and the moving end can be seen.
    fn select(&self, ctx: &Ctx, step: &Step, index: usize, len: usize, styles: &mut [Style]) {
        let selected = self
            .selection
            .filter(|s| (s.range.first..=s.range.last).contains(&index));
        if let Some(selected) = selected {
            let (start, end) = selected_columns(step, selected.scope, len);
            styles[start..end].fill(theme::selected());
        }
        // The cursor stays visible inside a selection, so the moving end can be seen.
        if index == ctx.cursor().step {
            let (start, end) = field_range(ctx.cursor().field);
            let end = end.min(len);
            let start = start.min(end);
            let (first, last) = selected.map_or((start, end), |selected| {
                let (from, to) = selected_columns(step, selected.scope, len);
                (start.clamp(from, to), end.clamp(from, to))
            });
            styles[first..last].fill(theme::cursor(ctx.mode()));
        }
    }
}

/// The cells of a row that are empty: its note, velocity, gate and length where it has none, and
/// each FX slot without a command.
fn empty_cells(step: &Step) -> Vec<(usize, usize)> {
    let mut empty = Vec::new();
    if step.note == Note::Empty {
        empty.push(field_range(Field::Note));
    }
    if step.velocity.is_none() {
        empty.push(field_range(Field::Velocity));
    }
    if step.gate.is_none() {
        empty.push(field_range(Field::Gate));
    }
    if step.length().is_none() {
        empty.push(field_range(Field::GateLength));
    }
    for (slot, fx) in step.fx.iter().enumerate() {
        if fx.command.is_none() {
            empty.push(slot_columns(slot));
        }
    }
    empty
}

/// The styles of a row's cells by what's written in them: its body, with what's empty faint, its
/// own length and nudge dim, and a note out of the scale marked.
fn written_styles(
    ctx: &Ctx,
    step: &Step,
    len: usize,
    body: Style,
    lit: bool,
    plays: bool,
    empty: &[(usize, usize)],
) -> Vec<Style> {
    let mut styles = vec![body; len];
    let empty_look = if lit { playing_empty() } else { theme::faint() };
    for &(start, end) in empty {
        for style in &mut styles[start..end.min(len)] {
            if style.fg.is_none() || lit {
                *style = style.patch(empty_look);
            }
        }
    }
    // A row's own length and its nudge are dim, so its note, gate and pulses stand out.
    let mut dimmed = Vec::new();
    if step.length().is_some() {
        dimmed.push(field_range(Field::GateLength));
    }
    if step.nudge != 0 {
        dimmed.push(field_range(Field::Nudge));
    }
    for (start, end) in dimmed {
        for style in &mut styles[start..end] {
            if style.fg.is_none() {
                *style = style.patch(theme::dim());
            }
        }
    }
    if let Note::On(midi) = step.note
        && plays
        && ctx.scale().on
        && !ctx.scale().contains(midi)
    {
        let (start, end) = field_range(Field::Note);
        styles[start..end].fill(body.patch(theme::out_of_scale()));
    }
    styles
}

/// A row's text as it's shown, a character to a cell: each value's mark as its glyph, styled, and
/// what's empty as the empty glyph.
fn marked_text(
    step: &Step,
    text: &str,
    empty: &[(usize, usize)],
    styles: &mut [Style],
) -> Vec<char> {
    // A value's mark is a glyph beside it: a lock, dim, as it's what a value does unless
    // marked otherwise; a pin for a hold and a rising slope for a glide in the value's fine
    // colour, as they change what the value does and belong to it. The text keeps the
    // letters, worked on a character to a cell, so the glyphs go in last.
    let mut shown: Vec<char> = text.chars().collect();
    for (slot, fx) in step.fx.iter().enumerate() {
        if !fx.is_value() {
            continue;
        }
        let at = field_range(Field::FxValue(slot)).1 + FINE_WIDTH + MARK_WIDTH - 1;
        let look = match fx.mark {
            Mark::Lock => theme::dim(),
            Mark::Hold | Mark::Glide => Style::new().fg(theme::fine(theme::MAUVE)),
        };
        if styles[at].fg.is_none() {
            styles[at] = styles[at].patch(look);
        }
        if shown.get(at) == Some(&fx.mark.glyph()) {
            shown[at] = glyphs::mark(fx.mark);
        }
    }
    for &(start, end) in empty {
        for cell in shown.iter_mut().take(end).skip(start) {
            if *cell == BLANK {
                *cell = EMPTY;
            }
        }
    }
    shown
}

/// Draw the view in an area, as it's laid out and scrolled.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &PhraseView, ctx: &Ctx) {
    let Some(phrase) = ctx.phrase_here() else {
        frame.render_widget(Paragraph::new("This track has no phrase here."), area);
        return;
    };
    let grid = Grid::new(ctx, view, phrase, area);
    let mut lines = vec![title(ctx, phrase, area.width), grid.headings(ctx)];
    lines.extend(grid.lines(ctx));
    frame.render_widget(Paragraph::new(lines), area);
    // Beside the overview, its top level with the grid's frame.
    if let Some(width) = grid.inspector_width {
        let x = GUTTER
            + grid.body_width
            + 2
            + neighbours_width(ctx.song(), &grid.others)
            + INSPECTOR_GAP;
        let at = Rect::new(
            area.x + x as u16,
            area.y + 1,
            width as u16,
            inspector::HEIGHT.min(area.height.saturating_sub(1)),
        );
        let row = row_number(grid.extent.row(ctx.cursor().step));
        inspector::draw(frame, at, ctx, row.trim_start());
    }
}
