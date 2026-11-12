//! The phrase view: one track's steps, the cursor and the playhead.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::constants::{BAR_STEPS, BEAT_STEPS, FX_SLOTS, TICKS_PER_STEP};
use vt_model::selection::{Scope, StepRange};
use vt_model::shape::ShapeColumn;
use vt_model::song::{Phrase, Track};
use vt_model::step::{FxCommand, GateKind, Mark, Note, Step};

use crate::app::{App, Field, TrackSpan};
use crate::glyphs;
use crate::input::mode::Mode;
use crate::theme;
use crate::timeline::{self, Line as TimeLine};
use crate::views::widgets::group::{self, Switched};

/// The width of a read-only note column for another track: its playhead marker, a note and a gap,
/// so a marker has a cell of its own and the playing line reads as columns, not one string.
const NEIGHBOUR_WIDTH: usize = 5;
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
pub const ROWS_AROUND_STEPS: u16 = 3;

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
/// How a row on the grid shows its nudge: as nothing, so only a nudged row's catches the eye.
const NUDGE_EMPTY: &str = "  ";
/// Where the first FX slot starts: after the note, velocity, nudge and gate cell with its length.
const FX_START: usize = 18;

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
    // A sign and the 24ths in one hex digit, as the most is `B`.
    let nudge = match step.nudge {
        0 => NUDGE_EMPTY.to_owned(),
        late if late > 0 => format!("+{late:X}"),
        early => format!("-{:X}", early.unsigned_abs()),
    };
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
fn pending_step(app: &App, step: &Step, typing: bool) -> Option<Step> {
    let (Field::FxValue(slot), Some(command)) = (app.cursor.field, app.entry.command) else {
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
        Field::Nudge => (7, 9),
        Field::Gate => (10, 14),
        Field::GateLength => (15, 17),
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

/// Which step of this phrase the playhead is on, if it is on this phrase.
pub(crate) fn playhead_step(app: &App) -> Option<usize> {
    playhead_place(app).map(|(step, _)| step)
}

/// Which step of this phrase the playhead is on, and which of the step's pulses, if it is on this
/// phrase.
pub(crate) fn playhead_place(app: &App) -> Option<(usize, usize)> {
    let owner = app.track_here()?;
    let chain = (*app.section_here()?.chains.get(app.cursor.track)?)?;
    let playhead = app.playing_place(app.cursor.track, chain)?;
    let position = owner.chain_position(chain, playhead, app.dealer(app.cursor.track))?;
    let start = owner.phrases.get(&position.phrase)?.start();
    (position.slot == app.cursor.slot && Some(position.phrase) == app.phrase_id())
        .then_some((start + position.step, position.pulse))
}

/// The note the cursor's track's row playing was heard at, before its chain's transpose, where an
/// accumulator moved it.
pub(crate) fn heard_note(app: &App) -> Option<u8> {
    app.places.get(app.cursor.track)?.accumulated
}

/// Where each FX column with a lane of its own is reading, if this phrase is the one playing.
fn lane_playheads(app: &App, phrase: &Phrase) -> [Option<usize>; FX_SLOTS] {
    let mut playheads = [None; FX_SLOTS];
    let Some(jam) = playhead_step(app).and(app.jam.get(app.cursor.track)) else {
        return playheads;
    };
    let count = app.places[app.cursor.track].count - jam.started;
    for (slot, playhead) in playheads.iter_mut().enumerate() {
        *playhead = phrase
            .lane(slot)
            .map(|lane| lane.step_at(count + jam.lane_offsets[slot]));
    }
    playheads
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
        ShapeColumn::Nudge => field_range(Field::Nudge),
        ShapeColumn::Length => field_range(Field::GateLength),
        ShapeColumn::Fx { slot, .. } => field_range(Field::FxCommand(slot)),
    }
}

/// Whether a step has a value in a live generator's column to mark: a note, a velocity, a nudge,
/// a length of its own, or an FX command.
fn live_value(step: &Step, column: ShapeColumn) -> bool {
    match column {
        ShapeColumn::Note { .. } => step.note != Note::Empty,
        ShapeColumn::Velocity => step.velocity.is_some(),
        ShapeColumn::Nudge => step.nudge != 0,
        ShapeColumn::Length => step.length().is_some(),
        ShapeColumn::Fx { slot, .. } => step.fx.get(slot).is_some_and(|fx| fx.command.is_some()),
    }
}

/// Which tracks fit in the overview beside the grid, in order, with the focused one among them and
/// those near it in view. Groups, returns and the master have their notes elsewhere, so they're
/// left out unless one is the track being edited. A folded group's tracks are left out too, with
/// a column of the group's own, named and empty, where the first of them would be (decision 128).
fn neighbours(app: &App, width: usize) -> Vec<usize> {
    let song = app.song();
    let mut tracks: Vec<usize> = song
        .tracks
        .iter()
        .enumerate()
        .filter(|(index, track)| {
            *index == app.cursor.track || (track.is_sound() && !app.folds.hides(song, *index))
        })
        .map(|(index, _)| index)
        .collect();
    for group in (0..song.tracks.len()).filter(|&track| app.folds.holds(song, track)) {
        if group == app.cursor.track {
            continue;
        }
        if let Some(first) = song.group_members(group).next() {
            let at = tracks.partition_point(|&track| track < first);
            tracks.insert(at, group);
        }
    }
    let shown = (width / NEIGHBOUR_WIDTH).min(tracks.len());
    if shown == 0 {
        return Vec::new();
    }
    let centre = tracks
        .iter()
        .position(|&track| track == app.cursor.track)
        .unwrap_or_else(|| tracks.partition_point(|&track| track < app.cursor.track));
    let start = centre.saturating_sub(shown / 2).min(tracks.len() - shown);
    tracks[start..start + shown].to_vec()
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
fn column(app: &App, track: usize) -> Option<Column<'_>> {
    if track != app.cursor.track && app.folds.holds(app.song(), track) {
        return None;
    }
    let owner = app.song().tracks.get(track)?;
    let chain = (*app.section_here()?.chains.get(track)?)?;
    let moving = app.playing_place(track, chain);
    let place = moving.unwrap_or_else(|| app.step_on(track, app.cursor.track, app.section_step()));
    let position = owner.chain_position(chain, place, app.dealer(track))?;
    let steps = owner.phrases.get(&position.phrase)?.loop_steps();
    Some(Column {
        steps,
        playing: moving.map(|_| (position.step, position.pulse)),
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
    columns: &[Option<Column>],
    tracks: &[usize],
    at: &At,
    selected: Option<(TrackSpan, isize, isize)>,
) -> Vec<Span<'static>> {
    columns
        .iter()
        .zip(tracks)
        .flat_map(|(column, track)| {
            let Some(column) = column else {
                return [Span::raw(" "), Span::raw("   "), Span::raw(" ")];
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
            let written = shown.map(|row| column.steps[row].note);
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
            [marker, note, Span::raw(" ")]
        })
        .collect()
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

/// A track's name, or its number if it has none.
pub(crate) fn track_name(app: &App, track: usize) -> String {
    let name = app.song().tracks.get(track).map_or("", |t| t.name.as_str());
    if name.is_empty() {
        format!("{:02}", track + 1)
    } else {
        name.to_owned()
    }
}

/// The glyph for a track that's muted or soloed, if it is, and its style: the active colour for
/// its own, or its group's colour for its group's. A mute wins, as the track is silent, and one of
/// its own wins over its group's. A mute or solo waiting for the launch grid is the glyph for
/// waiting, dim, until it switches.
pub(crate) fn level_glyph(app: &App, track: usize) -> Option<(char, Style)> {
    let jam = app.jam.get(track).copied().unwrap_or_default();
    if jam.mute_waiting || jam.solo_waiting {
        return Some((glyphs::WAITING, theme::dim()));
    }
    let (muted, soloed) = (group::muted(app, track), group::soloed(app, track));
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

/// A track's label over its column in the overview, three cells wide: the start of its name, and
/// its mute or solo glyph in the last cell if it has one, or a group's mark that its tracks are
/// folded into it.
fn column_label(app: &App, track: usize) -> (String, Option<(char, Style)>) {
    let name = track_name(app, track);
    let folded = (track != app.cursor.track && app.folds.holds(app.song(), track)).then(|| {
        let style = group::style(app.song(), track).unwrap_or_default();
        (glyphs::FOLDED, style)
    });
    match level_glyph(app, track).or(folded) {
        Some(glyph) => (
            format!("{:<2}", name.chars().take(2).collect::<String>()),
            Some(glyph),
        ),
        None => (
            format!("{:<3}", name.chars().take(3).collect::<String>()),
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

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(phrase) = app.phrase_here() else {
        frame.render_widget(Paragraph::new("This track has no phrase here."), area);
        return;
    };
    let height = usize::from(area.height.saturating_sub(ROWS_AROUND_STEPS)).max(1);
    let top = app.top;
    let selection = app.mode.is_visual().then(|| app.current_selection());
    let selection = selection
        .or(app.replacing_selection.map(|(selection, _)| selection))
        .or(app.shape.as_ref().map(|panel| panel.selection));
    let across = selection.map(|s| {
        let start = app.extent().start as isize;
        (
            s.tracks,
            s.range.first as isize - start,
            s.range.last as isize - start,
        )
    });
    let timeline_on = app.timeline_on();
    let lane_playheads = lane_playheads(app, phrase);
    let extent = phrase.extent();
    let stored = phrase.steps().len();
    // The playhead's row, and the line it's on: the line of its pulse in the timeline.
    let playhead_place = playhead_place(app);
    let playhead = playhead_place.map(|(step, _)| step);
    let playhead_line = playhead_place.map(|(step, pulse)| {
        let last = timeline::row_lines(phrase, step, timeline_on) - 1;
        (step, pulse.min(last))
    });
    let body_width = phrase
        .steps()
        .iter()
        .map(|step| step_text(step, app.track_here()).len())
        .max()
        .unwrap_or(0);
    let room = usize::from(area.width).saturating_sub(GUTTER + body_width + 2);
    let others = neighbours(app, room);
    let columns: Vec<_> = others.iter().map(|track| column(app, *track)).collect();
    // How long a line is, in ticks: one of this track's steps.
    let ticks = app
        .track_here()
        .map_or(TICKS_PER_STEP, |track| track.speed.ticks());
    // A short phrase still has rows for the longer tracks beside it, a line each past its end.
    let rows = if timeline_on {
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
    };
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
    // Where a line falls in the bar: by when it starts while it plays, and by its number outside
    // the loop, where it doesn't play, or on from the loop's end in the timeline.
    let stress = |line: &TimeLine| Stress::at(line.time);
    // The rule under a line, from the stress of the line after it. The loop's last line is
    // followed by its first, which starts a bar, and the phrase's last line by nothing.
    let rule_under = |at: usize| {
        let line = time_lines[at];
        let next = time_lines.get(at + 1)?;
        if next.index >= stored {
            None
        } else if extent.plays(line.index) && !extent.plays(next.index) {
            Stress::Bar.rule()
        } else {
            stress(next).rule()
        }
    };
    // Where a line is, for the other tracks' columns.
    let columns_at = |line: &TimeLine| {
        if timeline_on {
            At::Ticks(timeline::ticks_of(line, ticks))
        } else {
            At::Row(extent.row(line.index))
        }
    };
    let id = app.phrase_id().unwrap_or(0);
    let shared = app.phrase_uses(id).len() > 1;
    // What's said about the phrase after its number, each a group of its own, dim as a device
    // title's second words are: where it is in the chain, an order other than forward, and its
    // lanes' lengths.
    let mut details = vec![format!(
        "{}/{}",
        app.cursor.slot + 1,
        app.chain_entries().len()
    )];
    if !phrase.order().is_forward() {
        details.push(phrase.order().to_string());
    }
    let lanes: Vec<String> = (0..FX_SLOTS)
        .filter_map(|slot| Some(format!("FX{}:{}", slot + 1, phrase.lane(slot)?.length)))
        .collect();
    if !lanes.is_empty() {
        details.push(lanes.join(" "));
    }
    // Its live columns are named, in the shape panel's colour, as their cells are drawn.
    let mut live_names: Vec<String> = phrase
        .generators()
        .iter()
        .map(|generator| crate::shaping::column_label(generator.column))
        .collect();
    live_names.dedup();
    let live: Vec<(StepRange, ShapeColumn)> = phrase
        .generators()
        .iter()
        .filter_map(|generator| Some((generator.rows()?, generator.column)))
        .collect();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    // The title in groups two spaces apart, as the track page's is: the track, with its machine
    // or kind as a glyph before it and whether it's muted or soloed after; the phrase, with a link
    // if it plays in more than one place; then what's said about it.
    let mut title = vec![];
    if let Some(track) = app.track_here() {
        title.push(Span::styled(
            format!("{} ", glyphs::track(track)),
            theme::dim(),
        ));
    }
    title.push(Span::styled(
        format!("Track {}", track_name(app, app.cursor.track)),
        bold,
    ));
    // A track in a group carries its mark, in the group's colour, as on the mixer.
    if let Some(mark) = group::mark(app.song(), app.cursor.track) {
        title.extend([Span::raw(" "), mark.patch_style(bold)]);
    }
    if let Some((glyph, style)) = level_glyph(app, app.cursor.track) {
        title.push(Span::styled(format!(" {glyph}"), style));
    }
    title.push(Span::styled(format!("  Phrase {id:02X}"), bold));
    // The space after a glyph is one it isn't drawn over, so it keeps to its cell.
    let gap = if shared {
        title.push(Span::styled(format!(" {}", glyphs::LINK), theme::dim()));
        format!("{} ", glyphs::ICON_SPACE)
    } else {
        "  ".to_owned()
    };
    title.push(Span::styled(
        format!("{gap}{}", details.join("  ")),
        theme::dim(),
    ));
    if !live_names.is_empty() {
        title.push(Span::styled(
            format!("  {} {}", glyphs::LIVE, live_names.join(",")),
            bold.fg(theme::TEAL),
        ));
    }
    let cursor_style = theme::cursor(app.mode);
    let boxed = app.mode == Mode::Insert;
    let edge = Style::new().fg(theme::GREEN);
    let rule = |left: &str, right: &str| {
        if boxed {
            let inside = "─".repeat(body_width);
            let indent = " ".repeat(GUTTER - 1);
            Line::styled(format!("{indent}{left}{inside}{right}"), edge)
        } else {
            Line::raw("")
        }
    };
    // The headings row: the grid's parts, set into the box's top edge in Insert, and each other
    // track's label over its column's note, as `Note` is over the notes.
    let mut top_rule = if boxed {
        let inside = headings_line(body_width, '─');
        vec![Span::styled(
            format!("{}╭{inside}╮ ", " ".repeat(GUTTER - 1)),
            edge,
        )]
    } else {
        let headings = headings_line(body_width, ' ');
        vec![Span::styled(
            format!("{}{headings}  ", " ".repeat(GUTTER)),
            theme::dim(),
        )]
    };
    for track in &others {
        // The track being edited is marked out, so the overview shows every track in its place,
        // and a group and its tracks are in its colour.
        let (label, glyph) = column_label(app, *track);
        top_rule.push(Span::raw(" "));
        let marked = (*track == app.cursor.track).then(|| bold.fg(theme::BASE).bg(theme::GREEN));
        let named = group::style(app.song(), *track).map_or(bold, |tint| bold.patch(tint));
        top_rule.push(Span::styled(label, marked.unwrap_or(named)));
        let gap = match glyph {
            Some((glyph, style)) => {
                top_rule.push(Span::styled(glyph.to_string(), marked.unwrap_or(style)));
                glyphs::ICON_SPACE
            }
            None => ' ',
        };
        top_rule.push(Span::raw(gap.to_string()));
    }
    let side = if boxed { "│" } else { " " };
    let mut lines = vec![Line::from(title), Line::from(top_rule)];
    for (at_line, line) in time_lines.iter().enumerate().skip(first_line).take(height) {
        let index = line.index;
        let row = extent.row(index);
        let marker = if playhead_line == Some((index, line.pulse)) {
            ">"
        } else {
            " "
        };
        let Some(step) = phrase.steps().get(index) else {
            // The box closes on the first line past the phrase, so rows don't move in Insert.
            let (left, fill, right) = if boxed && at_line == stored_lines {
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
                Span::styled(format!("{left}{}{right}", fill.repeat(body_width)), edge),
                Span::raw(" "),
            ];
            spans.extend(column_spans(&columns, &others, &columns_at(line), across));
            lines.push(Line::from(spans));
            continue;
        };
        if line.pulse > 0 {
            let selected = selection
                .filter(|s| (s.range.first..=s.range.last).contains(&index))
                .map(|s| s.scope);
            let mut spans = vec![
                Span::raw("      "),
                Span::styled(format!(" {marker}"), theme::active()),
                Span::styled(side, edge),
            ];
            let width = step_text(step, app.track_here()).len();
            let lit = playhead == Some(index);
            spans.extend(continuation(
                step,
                width,
                selected,
                lit,
                rule_under(at_line),
            ));
            spans.push(Span::styled(
                " ".repeat(body_width.saturating_sub(width)),
                rule_under(at_line).unwrap_or_default(),
            ));
            spans.push(Span::styled(side, edge));
            spans.push(Span::raw(" "));
            spans.extend(column_spans(&columns, &others, &columns_at(line), across));
            lines.push(Line::from(spans));
            continue;
        }
        let typing = index == app.cursor.step && matches!(app.mode, Mode::Insert | Mode::Replace);
        let pending = pending_step(app, step, typing);
        let step = pending.as_ref().unwrap_or(step);
        let mut text = step_text(step, app.track_here());
        // A label being typed shows in its command's place, and the first digit of a value in its.
        if typing
            && let Field::FxCommand(_) = app.cursor.field
            && !app.entry.text.is_empty()
        {
            let (start, end) = field_range(app.cursor.field);
            if end <= text.len() {
                let typed = format!("{}_", app.entry.text);
                text.replace_range(start..end, &format!("{typed:<width$}", width = end - start));
            }
        }
        // The first digit of a value being typed shows in its place.
        if typing
            && matches!(
                app.cursor.field,
                Field::FxValue(_)
                    | Field::FxFine(_)
                    | Field::Velocity
                    | Field::Nudge
                    | Field::GateLength
            )
            && app.entry.text.len() == 1
        {
            let (start, end) = field_range(app.cursor.field);
            if end <= text.len() {
                text.replace_range(start..end, &format!("{}_", app.entry.text));
            }
        }
        // The row playing shows the note heard where an accumulator moved it, as the note column
        // otherwise shows notes as written.
        if playhead == Some(index)
            && matches!(step.note, Note::On(_))
            && let Some(heard) = heard_note(app)
        {
            let (start, end) = field_range(Field::Note);
            if end <= text.len() {
                text.replace_range(start..end, &Note::On(heard).to_string());
            }
        }
        // A lane's playhead goes in the gap before its column.
        for (slot, at) in lane_playheads.iter().enumerate() {
            let before = slot_columns(slot).0 - 1;
            if *at == Some(index) && text.get(before..=before) == Some(" ") {
                text.replace_range(before..=before, ">");
            }
        }
        let plays = extent.plays(index);
        let on_beat = stress(line) != Stress::Step;
        let number = match (plays, on_beat) {
            (true, true) => Style::new().fg(theme::YELLOW),
            (true, false) => Style::new().fg(theme::SUBTEXT1),
            (false, true) => theme::dim(),
            (false, false) => theme::faint(),
        };
        let mut spans = vec![
            Span::styled(relative_number(index, app.cursor.step), theme::faint()),
            Span::styled(row_number(row), number),
            Span::styled(format!(" {marker}"), theme::active()),
            Span::styled(side, edge),
        ];
        // A row its `CHA` or `CND` skipped this time is faint under the playhead, so the logic
        // can be followed.
        let skipped = app
            .places
            .get(app.cursor.track)
            .is_some_and(|place| place.skipped);
        let body = if playhead == Some(index) && skipped {
            theme::faint()
        } else if playhead == Some(index) {
            theme::active()
        } else if plays {
            Style::new()
        } else {
            theme::faint()
        };
        let mut styles = vec![body; text.len()];
        // What's empty is faint, as a code editor dims whitespace, so what's written stands out.
        // On the row playing it's the playhead's green set back, so the row reads as one lit bar
        // that keeps its shape, and an empty row isn't the brightest thing on screen.
        let lit = playhead == Some(index) && !skipped;
        let empty_look = if lit { playing_empty() } else { theme::faint() };
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
        for &(start, end) in &empty {
            for style in &mut styles[start..end.min(text.len())] {
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
            && app.scale.on
            && !app.scale.contains(midi)
        {
            let (start, end) = field_range(Field::Note);
            styles[start..end].fill(body.patch(theme::out_of_scale()));
        }
        // A column with a lane plays its own rows: they're lit, and the rest of it is dim.
        for (slot, lane_playhead) in lane_playheads.iter().enumerate() {
            let Some(lane) = phrase.lane(slot) else {
                continue;
            };
            let (start, end) = slot_columns(slot);
            let end = end.min(text.len());
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
        for (rows, column) in &live {
            let (start, end) = live_cells(*column);
            if (rows.first..=rows.last).contains(&index) && live_value(step, *column) {
                let end = end.min(text.len());
                for style in &mut styles[start.min(end)..end] {
                    *style = style.fg(theme::TEAL);
                }
            }
        }
        // A lock for a voice or an effect this track doesn't have now stays, grey with no colour
        // and brighter than an empty cell, so it isn't typed over. It plays again when its owner is
        // back.
        if let Some(track) = app.track_here() {
            for (slot, fx) in step.fx.iter().enumerate() {
                if fx.command.is_some_and(|command| !command.exists_on(track)) {
                    let (start, end) = slot_columns(slot);
                    let end = end.min(text.len());
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
        for &(start, end) in &empty {
            for cell in shown.iter_mut().take(end).skip(start) {
                if *cell == BLANK {
                    *cell = EMPTY;
                }
            }
        }
        let selected = selection.filter(|s| (s.range.first..=s.range.last).contains(&index));
        if let Some(selected) = selected {
            let (start, end) = selected_columns(step, selected.scope, text.len());
            styles[start..end].fill(theme::selected());
        }
        // The cursor stays visible inside a selection, so the moving end can be seen.
        if index == app.cursor.step {
            let (start, end) = field_range(app.cursor.field);
            let end = end.min(text.len());
            let start = start.min(end);
            let (first, last) = selected.map_or((start, end), |selected| {
                let (from, to) = selected_columns(step, selected.scope, text.len());
                (start.clamp(from, to), end.clamp(from, to))
            });
            styles[first..last].fill(cursor_style);
        }
        // The rule runs under the whole row, over whatever else its cells show.
        let under = rule_under(at_line);
        if let Some(under) = under {
            for style in &mut styles {
                *style = style.patch(under);
            }
        }
        spans.extend(spans_of(&shown, &styles));
        let padding = body_width.saturating_sub(text.len());
        spans.push(Span::styled(" ".repeat(padding), under.unwrap_or_default()));
        spans.push(Span::styled(side, edge));
        spans.push(Span::raw(" "));
        spans.extend(column_spans(&columns, &others, &columns_at(line), across));
        lines.push(Line::from(spans));
    }
    if !(first_line..first_line + height).contains(&stored_lines)
        || stored_lines >= time_lines.len()
    {
        lines.push(rule("╰", "╯"));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
