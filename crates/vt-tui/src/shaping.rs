//! The shape panel: fill a selected column with a ramp, random values, a walk, a wave, two values
//! in turn or a short list of them, or rework the values there, heard while it's adjusted.

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::generator::{Span, applied};
use vt_model::lfo::{Shape as Wave, note_rates};
use vt_model::scale::Scale;
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::shape::{
    Acid, DEFAULT_RATE, EUCLID_HITS, Edge, FillKind, MAX_ACID_NOTES, MAX_BEND, MAX_DEPTH,
    MAX_VALUES, MIN_DEPTH, Pink, Shape, ShapeColumn, Values, WAVES, Walk, WaveRate, Where,
    default_ends, default_step, open_rows, track_seed,
};
use vt_model::song::{Song, Track};
use vt_model::step::{FxCommand, MAX_NOTE, Note, Step};

use crate::app::{App, Selection, TrackSpan};
use crate::input::bindings::{Cmd, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker};

/// How far a small and a large step move a curve.
const BEND_NUDGE: i16 = 5;
const BEND_JUMP: i16 = 25;
/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i16 = 0x10;
/// How far a large step moves an `Every` or a `Chance`.
const EVERY_JUMP: i16 = 4;
const CHANCE_JUMP: i16 = 10;
/// How far a small and a large step move a phase, in degrees, as on an LFO.
const PHASE_NUDGE: i16 = 15;
const PHASE_JUMP: i16 = 90;
/// The longest `Run`, and how far a large step moves it.
const MAX_RUN: i16 = 256;
const RUN_JUMP: i16 = 4;
/// How far a large step moves a Euclidean `Where`'s hits or rotation.
const EUCLID_JUMP: i16 = 4;
/// The longest rate that can be typed, like `64 bars`.
const RATE_TEXT: usize = 8;
/// The counts of cycles across the selection that `K` on `Rate` lists after the times.
const LISTED_CYCLES: [u16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// The longest `Step` a walk of notes takes: two octaves of semitones, or of degrees.
const MAX_NOTE_STEP: u8 = 24;

/// A row of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRow {
    From,
    To,
    Curve,
    S,
    Fill,
    Wave,
    Rate,
    Phase,
    Values,
    Run,
    Where,
    Amount,
    Hits,
    Rotate,
    /// An `Acid` fill's own: how many notes its riff has, the chances of an octave, a row playing,
    /// an accent and a slide, and whether a roll keeps the pitches.
    Notes,
    Octave,
    Density,
    Accent,
    Slide,
    Pitches,
    /// A `Walk`'s largest move and what it does at the ends, and how many values a `1/f` sums.
    Step,
    Edge,
    Depth,
}

impl ShapeRow {
    /// The row's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::From => "From",
            Self::To => "To",
            Self::Curve => "Curve",
            Self::S => "S",
            Self::Fill => "Fill",
            Self::Wave => "Shape",
            Self::Rate => "Rate",
            Self::Phase => "Phase",
            Self::Values => "Values",
            Self::Run => "Run",
            Self::Where => "Where",
            Self::Amount => "Amount",
            Self::Hits => "Hits",
            Self::Rotate => "Rotate",
            Self::Notes => "Notes",
            Self::Octave => "Octave",
            Self::Density => "Density",
            Self::Accent => "Accent",
            Self::Slide => "Slide",
            Self::Pitches => "Pitches",
            Self::Step => "Step",
            Self::Edge => "Edge",
            Self::Depth => "Depth",
        }
    }
}

/// One track's part of what the panel shapes: the phrase it's on, its rows, and its column, whose
/// FX command is the first on its own rows, or the one chosen for a column with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub track: usize,
    pub phrase: u32,
    pub range: StepRange,
    pub column: ShapeColumn,
    /// Whether its column was empty, so it's generated rather than reshaped: notes of its own on
    /// the Note column, or the chosen command on an FX column.
    pub generate: bool,
}

/// Whether a column has nothing on these rows to reshape: no note, or no FX command in its slot.
fn empty_column(steps: &[Step], range: StepRange, column: ShapeColumn) -> bool {
    let mut rows = (range.first..=range.last).filter_map(|row| steps.get(row));
    match column {
        ShapeColumn::Note { .. } => !rows.any(|step| matches!(step.note, Note::On(_))),
        ShapeColumn::Fx { slot, .. } => {
            !rows.any(|step| step.fx.get(slot).is_some_and(|fx| fx.command.is_some()))
        }
        _ => false,
    }
}

/// The rows a shape fills on a column it generates on. Where it would choose the notes or the
/// commands already there, which an empty column hasn't, it takes every row.
fn generating(place: Where, column: ShapeColumn) -> Where {
    match (place, column) {
        (Where::Notes, ShapeColumn::Note { .. }) | (Where::Commands, _) => Where::AllRows,
        (place, _) => place,
    }
}

/// The open panel. The song underneath it is a preview until `Enter` keeps it.
#[derive(Debug, Clone)]
pub struct ShapePanel {
    pub selection: Selection,
    /// A part for each track shaped, the first track's first. Each track takes the same settings
    /// with a seed of its own.
    pub parts: Vec<Part>,
    pub column: ShapeColumn,
    pub shape: Shape,
    pub row: ShapeRow,
    /// What's being typed into the current row.
    pub typing: Option<String>,
    /// The live generator it was opened on with `gl`, by its place in its phrase's list, which
    /// it reshapes rather than shaping the rows once.
    pub live: Option<usize>,
    /// The song from before the panel opened.
    base: Arc<Song>,
    /// On a live generator, its phrase's steps as they'd be without it, which it shapes from.
    under: Option<Vec<Step>>,
}

impl ShapePanel {
    /// The rows on show. `Fill` comes first, as it decides the rest: a wave and `Alternate` have
    /// their own settings in place of a curve, a `List` has its values in place of the two ends,
    /// a transform but `Scale` works from the values there with no settings of its own,
    /// and `Amount` only means something for some choices of rows, as `Hits` and `Rotate` do for
    /// a Euclidean one. `Acid` has its riff's settings, its `Density` only where it writes notes
    /// of its own, and keeping the pitches there leaves it no ends or notes to choose. `Walk` has
    /// its `Step` and `Edge`, and `1/f` its `Depth`.
    pub fn rows(&self) -> Vec<ShapeRow> {
        let mut rows = vec![ShapeRow::Fill];
        if self.shape.kind == FillKind::Acid {
            let generates = self.parts.iter().any(|part| part.generate);
            if generates || !self.shape.acid.keep {
                rows.extend_from_slice(&[ShapeRow::From, ShapeRow::To, ShapeRow::Notes]);
            }
            rows.push(ShapeRow::Octave);
            if generates {
                rows.push(ShapeRow::Density);
            }
            rows.extend_from_slice(&[ShapeRow::Accent, ShapeRow::Slide, ShapeRow::Pitches]);
        }
        rows.extend_from_slice(match self.shape.kind {
            FillKind::Acid => &[],
            FillKind::Ramp | FillKind::Random => &[
                ShapeRow::From,
                ShapeRow::To,
                ShapeRow::Curve,
                ShapeRow::S,
                ShapeRow::Run,
            ],
            FillKind::Walk => &[
                ShapeRow::From,
                ShapeRow::To,
                ShapeRow::Step,
                ShapeRow::Edge,
                ShapeRow::Run,
            ],
            FillKind::Pink => &[ShapeRow::From, ShapeRow::To, ShapeRow::Depth, ShapeRow::Run],
            FillKind::Scale => &[ShapeRow::From, ShapeRow::To, ShapeRow::Curve, ShapeRow::S],
            FillKind::Wave => &[
                ShapeRow::From,
                ShapeRow::To,
                ShapeRow::Wave,
                ShapeRow::Rate,
                ShapeRow::Phase,
                ShapeRow::Run,
            ],
            FillKind::Alternate => &[ShapeRow::From, ShapeRow::To, ShapeRow::Run],
            FillKind::List => &[ShapeRow::Values, ShapeRow::Run],
            FillKind::Invert | FillKind::Shuffle | FillKind::Reverse | FillKind::Snap => &[],
        });
        rows.push(ShapeRow::Where);
        match self.shape.place {
            Where::Every(_) | Where::Chance(_) => rows.push(ShapeRow::Amount),
            Where::Euclid { .. } => rows.extend_from_slice(&[ShapeRow::Hits, ShapeRow::Rotate]),
            _ => {}
        }
        rows
    }

    /// How many rows the column could change, which a Euclidean `Where`'s hits are spread over:
    /// the most of any track's.
    pub fn open_rows(&self) -> usize {
        self.parts
            .iter()
            .map(|part| open_rows(self.base_steps(part), part.range, part.column))
            .max()
            .unwrap_or(0)
    }

    /// A part's steps, as they were when the panel opened, without the live generator it's on.
    pub fn base_steps(&self, part: &Part) -> &[vt_model::step::Step] {
        if let Some(under) = &self.under {
            return under;
        }
        self.base
            .tracks
            .get(part.track)
            .and_then(|track| track.phrases.get(&part.phrase))
            .map_or(&[], |phrase| phrase.steps())
    }

    /// The shape each part takes: the panel's, with the part's own seed, and on a column it
    /// generates on, every row where the panel chooses the notes or commands there.
    pub fn shape_of(&self, nth: usize) -> Shape {
        let part = &self.parts[nth];
        Shape {
            seed: track_seed(self.shape.seed, nth),
            place: if part.generate {
                generating(self.shape.place, part.column)
            } else {
                self.shape.place
            },
            ..self.shape
        }
    }

    /// Whether every part is generated, on a column that was empty.
    pub fn generates(&self) -> bool {
        self.parts.iter().all(|part| part.generate)
    }

    /// What it does, for its heading: generate, reshape, or across tracks, both.
    pub fn verb(&self) -> &'static str {
        match (
            self.generates(),
            self.parts.iter().any(|part| part.generate),
        ) {
            (true, _) => "Generate",
            (false, true) => "Generate and reshape",
            (false, false) => "Reshape",
        }
    }

    /// The fills it offers. A transform reworks what's there, so it's left out when nothing is.
    pub fn fills(&self) -> Vec<FillKind> {
        let generates = self.generates();
        self.column
            .fills()
            .into_iter()
            .filter(|kind| !(generates && kind.reworks()))
            .collect()
    }

    /// The choices of rows. Where nothing is there, the rows already holding a command aren't one.
    pub fn wheres(&self) -> Vec<Where> {
        let generates = self.generates();
        self.column
            .wheres()
            .into_iter()
            .filter(|place| !(generates && *place == Where::Commands))
            .collect()
    }

    /// The rows it chooses unless told otherwise: every row where nothing is there.
    pub fn default_where(&self) -> Where {
        if self.generates() {
            Where::AllRows
        } else {
            self.column.default_where()
        }
    }

    /// The edit that gives every part its shape: once, or the live generator a new shape.
    fn action(&self) -> Action {
        if let Some(generator) = self.live {
            let part = self.parts[0];
            return Action::Reshape {
                track: part.track,
                phrase: part.phrase,
                generator,
                shape: self.shape,
            };
        }
        let mut actions: Vec<Action> = self
            .parts
            .iter()
            .enumerate()
            .map(|(nth, part)| Action::Shape {
                track: part.track,
                phrase: part.phrase,
                range: part.range,
                column: part.column,
                shape: self.shape_of(nth),
            })
            .collect();
        if actions.len() == 1 {
            actions.remove(0)
        } else {
            Action::Batch(actions)
        }
    }

    /// The edit that keeps every part live, one generator to a track: on its column's lane, if
    /// it has one, and on its rows otherwise.
    fn live_action(&self) -> Action {
        let mut actions: Vec<Action> = self
            .parts
            .iter()
            .enumerate()
            .map(|(nth, part)| Action::Generate {
                track: part.track,
                phrase: part.phrase,
                column: part.column,
                span: if self.on_lane(part) {
                    Span::Lane
                } else {
                    Span::rows(part.range)
                },
                shape: self.shape_of(nth),
            })
            .collect();
        if actions.len() == 1 {
            actions.remove(0)
        } else {
            Action::Batch(actions)
        }
    }

    /// Whether a part is on an FX column with a lane, which a live generator fills.
    fn on_lane(&self, part: &Part) -> bool {
        let ShapeColumn::Fx { slot, .. } = part.column else {
            return false;
        };
        self.base
            .tracks
            .get(part.track)
            .and_then(|track| track.phrases.get(&part.phrase))
            .is_some_and(|phrase| phrase.lane(slot).is_some())
    }
}

/// A column's short name, as the phrase's heading names its live ones.
pub fn column_label(column: ShapeColumn) -> String {
    match column {
        ShapeColumn::Note { .. } => "Note".to_owned(),
        ShapeColumn::Velocity => "Vel".to_owned(),
        ShapeColumn::Nudge => "Nudge".to_owned(),
        ShapeColumn::Length => "Len".to_owned(),
        ShapeColumn::Fx { slot, .. } => format!("FX{}", slot + 1),
    }
}

/// Whether the cursor on a field is on a live generator's column.
fn on_column(field: Field, column: ShapeColumn) -> bool {
    match (field, column) {
        (Field::Note, ShapeColumn::Note { .. })
        | (Field::Velocity, ShapeColumn::Velocity)
        | (Field::Nudge, ShapeColumn::Nudge)
        | (Field::Gate | Field::GateLength, ShapeColumn::Length) => true,
        (
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot),
            ShapeColumn::Fx { slot: owned, .. },
        ) => slot == owned,
        _ => false,
    }
}

/// The chance in percent an `Acid` row sets: of an octave, a row playing, an accent or a slide.
fn acid_chance(acid: &mut Acid, row: ShapeRow) -> Option<&mut u8> {
    match row {
        ShapeRow::Octave => Some(&mut acid.octave),
        ShapeRow::Density => Some(&mut acid.density),
        ShapeRow::Accent => Some(&mut acid.accent),
        ShapeRow::Slide => Some(&mut acid.slide),
        _ => None,
    }
}

/// The next or previous choice, going round.
fn turned<T: Copy + PartialEq>(choices: &[T], current: T, sign: i16) -> T {
    let at = choices.iter().position(|c| *c == current).unwrap_or(0);
    let step = if sign > 0 { 1 } else { choices.len() - 1 };
    choices[(at + step) % choices.len()]
}

/// A character typed into a list of values. Two hex digits make a value, and the space after it
/// comes by itself, so `704055` types `70 40 55`; a space ends a value of one digit sooner.
fn type_list(text: &mut String, c: char) {
    let count = text.split_whitespace().count();
    let last = text.rsplit(' ').next().unwrap_or("");
    if c.is_ascii_hexdigit() {
        if last.len() == 2 {
            if count == MAX_VALUES {
                return;
            }
            text.push(' ');
        }
        text.push(c.to_ascii_uppercase());
    } else if c == ' ' && !last.is_empty() && count < MAX_VALUES {
        text.push(' ');
    }
}

/// A value moved and kept in `0..=top`.
fn moved(value: i16, by: i16, low: i16, high: i16) -> i16 {
    (value + by).clamp(low, high)
}

/// The longest list of notes or nudges that can be typed: eight notes and the spaces between.
const NAMED_LIST_TEXT: usize = 40;

/// An end moved a small or a large step: a note by a semitone or an octave, or by a degree or an
/// octave of them while the scale is on; a nudge by one or four 24ths; anything else by one or
/// `10` in hex.
fn stepped(column: ShapeColumn, value: u8, sign: i16, large: bool) -> u8 {
    let jump = match column {
        ShapeColumn::Note { scale, .. } if scale.on => {
            let degrees = if large {
                scale.kind.intervals().len()
            } else {
                1
            };
            return scale.moved(value, degrees as i32 * i32::from(sign));
        }
        ShapeColumn::Note { .. } => 12,
        ShapeColumn::Nudge => 4,
        _ => HEX_JUMP,
    };
    let (low, top) = (i16::from(column.lowest()), i16::from(column.highest()));
    moved(
        i16::from(value),
        sign * if large { jump } else { 1 },
        low,
        top,
    ) as u8
}

/// A walk's `Step` moved a small or a large step: on notes by one or an octave, of degrees while
/// the scale is on; on a nudge by one or four 24ths; anything else by one or `10` in hex. It
/// runs from 1 to two octaves of notes, or to the column's whole range.
fn walk_step(column: ShapeColumn, step: u8, sign: i16, large: bool) -> u8 {
    let jump = match column {
        ShapeColumn::Note { scale, .. } if scale.on => scale.kind.intervals().len() as i16,
        ShapeColumn::Note { .. } => 12,
        ShapeColumn::Nudge => 4,
        _ => HEX_JUMP,
    };
    let by = sign * if large { jump } else { 1 };
    moved(i16::from(step), by, 1, i16::from(longest_step(column))) as u8
}

/// The longest `Step` a walk takes on a column.
fn longest_step(column: ShapeColumn) -> u8 {
    match column {
        ShapeColumn::Note { .. } => MAX_NOTE_STEP,
        column => column.highest() - column.lowest(),
    }
}

/// Whether a column's values are typed as two hex digits, rather than as notes or nudges.
fn typed_in_hex(column: ShapeColumn) -> bool {
    !matches!(column, ShapeColumn::Note { .. } | ShapeColumn::Nudge)
}

/// Why a selection shapes nothing yet.
enum Refusal {
    Reason(&'static str),
    /// An FX column on this track is empty, so the command to write on it is chosen first.
    Command(usize),
}

impl App {
    /// The column a track's rows can fill, and whether it's empty so it's generated, or why they
    /// can't. An empty FX column takes the command chosen for it, if one is.
    fn shape_column(
        &self,
        (index, track): (usize, &Track),
        steps: &[Step],
        range: StepRange,
        field: Field,
        command: Option<FxCommand>,
    ) -> Result<(ShapeColumn, bool), Refusal> {
        if range.len() < 2 {
            return Err(Refusal::Reason("Select at least 2 rows to shape"));
        }
        let rows = || (range.first..=range.last).filter_map(|r| steps.get(r));
        let notes = rows().any(|step| matches!(step.note, Note::On(_)));
        // A note's velocity, nudge and length are only shaped where there's a note.
        let on_notes = |column| {
            if notes {
                Ok((column, false))
            } else {
                Err(Refusal::Reason("There are no notes in the selection"))
            }
        };
        let (column, generate) = match field {
            // An empty Note column takes notes of its own, at the velocity notes are typed with.
            Field::Note => (
                ShapeColumn::Note {
                    scale: self.scale,
                    writes: (!notes).then_some(self.note_velocity),
                },
                !notes,
            ),
            Field::Velocity => on_notes(ShapeColumn::Velocity)?,
            Field::Nudge => on_notes(ShapeColumn::Nudge)?,
            Field::GateLength => on_notes(ShapeColumn::Length)?,
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                match rows().find_map(|step| step.fx.get(slot).and_then(|fx| fx.command)) {
                    Some(command) => (ShapeColumn::Fx { slot, command }, false),
                    None => match command {
                        None => return Err(Refusal::Command(index)),
                        Some(command) if FxCommand::on_track(track).contains(&command) => {
                            (ShapeColumn::Fx { slot, command }, true)
                        }
                        Some(_) => {
                            return Err(Refusal::Reason("This track has no such FX command"));
                        }
                    },
                }
            }
            Field::Gate => {
                return Err(Refusal::Reason(
                    "Shape works on a gate's length, not its type and pulses",
                ));
            }
        };
        if generate && open_rows(steps, range, column) == 0 {
            return Err(Refusal::Reason("There's nothing to shape here"));
        }
        Ok((column, generate))
    }

    /// What a selection shapes on each track it covers, or why it shapes nothing. A track whose
    /// rows have nothing to shape is passed over. An empty FX column on any of them waits for
    /// its command.
    fn shape_parts(
        &self,
        selection: Selection,
        field: Field,
        command: Option<FxCommand>,
    ) -> Result<Vec<Part>, Refusal> {
        if selection.range.len() < 2 {
            return Err(Refusal::Reason("Select at least 2 rows to shape"));
        }
        if selection.scope.first != selection.scope.last {
            return Err(Refusal::Reason(
                "Shape works on one column at a time. Select with v",
            ));
        }
        let song = self.song();
        let mut parts = Vec::new();
        let mut refused = None;
        let covered = song
            .tracks
            .iter()
            .enumerate()
            .take(selection.tracks.last + 1)
            .skip(selection.tracks.first);
        for (track, sound) in covered {
            let Some((phrase, range)) = self.place_on(track, selection.range) else {
                continue;
            };
            let steps = sound.phrases.get(&phrase).map_or(&[][..], |p| p.steps());
            match self.shape_column((track, sound), steps, range, field, command) {
                Ok((column, generate)) => parts.push(Part {
                    track,
                    phrase,
                    range,
                    column,
                    generate,
                }),
                Err(Refusal::Command(track)) => return Err(Refusal::Command(track)),
                Err(Refusal::Reason(reason)) => {
                    refused.get_or_insert(reason);
                }
            }
        }
        if parts.is_empty() {
            Err(Refusal::Reason(
                refused.unwrap_or("There's nothing to shape here"),
            ))
        } else {
            Ok(parts)
        }
    }

    /// Open the panel on the selection, and show its first result. An empty FX column lists the
    /// commands first, to choose the one it writes.
    pub(crate) fn open_shape(&mut self) {
        let selection = self.current_selection();
        let field = self.cursor.field;
        self.open_shape_on(selection, field, None);
    }

    /// Open the panel on the selection, now its empty FX column has its command.
    pub(crate) fn pick_shape_command(&mut self, command: FxCommand) {
        if let Some((selection, field)) = self.shape_command.take() {
            self.open_shape_on(selection, field, Some(command));
        }
    }

    fn open_shape_on(&mut self, selection: Selection, field: Field, command: Option<FxCommand>) {
        let parts = match self.shape_parts(selection, field, command) {
            Ok(parts) => parts,
            Err(Refusal::Reason(reason)) => {
                self.status = reason.to_owned();
                return;
            }
            Err(Refusal::Command(track)) => {
                self.leave_selection(selection);
                self.cursor.field = field;
                self.shape_command = Some((selection, field));
                let (_, items) = self.fx_command_items(track, Pick::ShapeCommand);
                self.show_picker(Picker::new("FX commands", items).searching(""));
                self.status = "Choose the command to write on the empty FX column".to_owned();
                return;
            }
        };
        let base = Arc::clone(self.song());
        let first = parts[0];
        let steps = base.tracks[first.track]
            .phrases
            .get(&first.phrase)
            .map_or(&[][..], |p| p.steps());
        let generates = parts.iter().all(|part| part.generate);
        // Across tracks, a Note column with notes on some offers what re-pitching them does.
        let column = match first.column {
            ShapeColumn::Note { scale, .. } if !generates => ShapeColumn::Note {
                scale,
                writes: None,
            },
            column => column,
        };
        let (from, to) = if first.generate {
            self.generated_ends(first.column)
        } else {
            default_ends(steps, first.range, first.column)
        };
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let place = if generates {
            Where::AllRows
        } else {
            column.default_where()
        };
        let panel = ShapePanel {
            selection,
            parts,
            column,
            shape: Shape {
                from,
                to,
                curve: 0,
                s: 0,
                kind: FillKind::Ramp,
                wave: Wave::Sine,
                rate: DEFAULT_RATE,
                phase: 0,
                run: 1,
                values: Values::default(),
                place,
                seed: base.seed.wrapping_add(self.shape_seed),
                acid: Acid::default(),
                walk: Walk {
                    step: default_step(column),
                    ..Walk::default()
                },
                pink: Pink::default(),
            },
            row: ShapeRow::Fill,
            typing: None,
            live: None,
            base,
            under: None,
        };
        self.leave_selection(selection);
        self.cursor.field = field;
        self.shape = Some(panel);
        self.mode = Mode::Shape;
        self.preview_shape();
    }

    /// Where a shape on an empty column starts and ends: for notes, an octave up from the scale's
    /// root, or C with it off, in the octave notes are typed in; and the command's whole range.
    fn generated_ends(&self, column: ShapeColumn) -> (u8, u8) {
        match column {
            ShapeColumn::Note { scale, .. } => {
                let root = Scale {
                    root: if scale.on { scale.root } else { 0 },
                    ..scale
                };
                let low = root.degree(0, self.octave).min(MAX_NOTE - 12);
                (low, low + 12)
            }
            column => (column.lowest(), column.highest()),
        }
    }

    /// Show the panel's result, without adding an undo step.
    fn preview_shape(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        if let Ok(next) = edit(&panel.base, &panel.action()) {
            self.history.replace_current(next);
            self.publish();
        }
    }

    /// Keep what's showing, as one undo step: as plain values, or live. A live generator reopened
    /// and kept as plain values is applied.
    fn keep_shape(&mut self, live: bool) {
        let Some(panel) = self.shape.take() else {
            return;
        };
        self.mode = Mode::Normal;
        let part = panel.parts[0];
        let action = match (panel.live, live) {
            (None, false) | (Some(_), true) => None,
            (None, true) => Some(panel.live_action()),
            (Some(generator), false) => Some(Action::Batch(vec![
                panel.action(),
                Action::ApplyGenerator {
                    track: part.track,
                    phrase: part.phrase,
                    generator,
                },
            ])),
        };
        let shown = Arc::clone(self.song());
        let result = match action.map(|action| edit(&panel.base, &action)) {
            None => shown,
            Some(Ok(result)) => result,
            Some(Err(error)) => {
                self.status = error.to_string();
                shown
            }
        };
        self.history.replace_current(Arc::clone(&panel.base));
        if *result != *panel.base {
            if panel.live.is_none() {
                self.say_applied(&result);
            }
            self.history.commit(result);
            self.dirty = true;
        }
        self.publish();
        self.remember_shape(&panel);
        let name = column_label(part.column);
        match (panel.live, live) {
            (None, true) => {
                let lane = panel.on_lane(&part);
                self.status = match panel.parts.len() {
                    1 if lane => format!(
                        "{name} is live on its lane: gl opens it again, Space g a applies it"
                    ),
                    1 => format!("{name} is live: gl opens it again, Space g a applies it"),
                    tracks => format!(
                        "{name} is live on {tracks} tracks, each on its own: gl opens one again"
                    ),
                };
            }
            (Some(_), true) => self.status = format!("{name} stays live"),
            (Some(_), false) => {
                self.status = format!("{name} is applied, as plain values");
            }
            (None, false) => {}
        }
    }

    /// Say so if an edit applied a live generator, as it changed one of its cells by hand.
    pub(crate) fn say_applied(&mut self, next: &Arc<Song>) {
        if let Some(column) = applied(self.song(), next).first() {
            self.status = format!(
                "{} was live: it's plain values now. u makes it live again",
                column_label(*column)
            );
        }
    }

    /// The live generator under the cursor: its phrase, and its place in the phrase's list. On the
    /// cursor's column, it's the one covering the cursor's row, or else the nearest.
    pub(crate) fn generator_here(&self) -> Option<(u32, usize)> {
        let phrase = self.phrase_id()?;
        let step = self.cursor.step;
        self.phrase_here()?
            .generators()
            .iter()
            .enumerate()
            .filter(|(_, generator)| on_column(self.cursor.field, generator.column))
            .filter_map(|(index, generator)| {
                let rows = generator.rows()?;
                let away = if step < rows.first {
                    rows.first - step
                } else {
                    step.saturating_sub(rows.last)
                };
                Some((away, index))
            })
            .min()
            .map(|(_, index)| (phrase, index))
    }

    /// Open the panel again on the live generator under the cursor, on its saved shape.
    pub(crate) fn reopen_generator(&mut self) {
        let Some((phrase, index)) = self.generator_here() else {
            self.status =
                "No live column here. Select rows, press S, then L to keep it live".to_owned();
            return;
        };
        let base = Arc::clone(self.song());
        let track = self.cursor.track;
        let found = &base.tracks[track].phrases[&phrase];
        let generator = &found.generators()[index];
        let Some(range) = generator.rows() else {
            return;
        };
        // It generates if its column is empty without it.
        let under = generator.under(found.steps());
        let part = Part {
            track,
            phrase,
            range,
            column: generator.column,
            generate: empty_column(&under, range, generator.column),
        };
        let shape = generator.shape;
        self.shape = Some(ShapePanel {
            selection: Selection {
                range,
                scope: Scope::column(self.cursor.field.group()),
                tracks: TrackSpan::between(track, track),
            },
            parts: vec![part],
            column: part.column,
            shape,
            row: ShapeRow::Fill,
            typing: None,
            live: Some(index),
            base,
            under: Some(under),
        });
        self.mode = Mode::Shape;
        self.preview_shape();
    }

    /// Apply the live generator under the cursor, leaving its values as plain ones.
    pub(crate) fn apply_generator_here(&mut self) {
        let Some((phrase, generator)) = self.generator_here() else {
            self.status = "No live column here to apply".to_owned();
            return;
        };
        let track = self.cursor.track;
        let column = self.song().tracks[track].phrases[&phrase].generators()[generator].column;
        self.apply(&Action::ApplyGenerator {
            track,
            phrase,
            generator,
        });
        self.status = format!("{} is applied, as plain values", column_label(column));
    }

    /// Put back what was there before the panel opened.
    fn cancel_shape(&mut self) {
        let Some(panel) = self.shape.take() else {
            return;
        };
        self.mode = Mode::Normal;
        self.history.replace_current(panel.base);
        self.publish();
    }

    /// The mode a picker goes back to.
    pub(crate) fn mode_after_picker(&self) -> Mode {
        if self.shape.is_some() {
            Mode::Shape
        } else {
            Mode::Normal
        }
    }

    /// Apply a shape to a range, on each of these tracks. A ramp's, random fill's, walk's or 1/f
    /// fill's ends are chosen again from what's there on the first, a wave or `Alternate` keeps the two values it
    /// moves between, a `List` keeps its list, and `Scale` the two it scales onto. Each track has
    /// a seed of its own. Each track's column is judged as `S` judges it: an empty one is
    /// generated on, notes of its own or the same command.
    pub(crate) fn shape_again(
        &mut self,
        range: StepRange,
        tracks: TrackSpan,
        column: ShapeColumn,
        shape: Shape,
    ) {
        let song = Arc::clone(self.song());
        let steps_on = |track: usize, phrase: u32| -> &[Step] {
            song.tracks[track]
                .phrases
                .get(&phrase)
                .map_or(&[][..], |p| p.steps())
        };
        let places: Vec<(usize, u32, StepRange, ShapeColumn, bool)> = (tracks.first..=tracks.last)
            .filter_map(|track| {
                let (phrase, range) = self.place_on(track, range)?;
                let generate = empty_column(steps_on(track, phrase), range, column);
                // Notes go through the scale as it is now.
                let column = match column {
                    ShapeColumn::Note { .. } => ShapeColumn::Note {
                        scale: self.scale,
                        writes: generate.then_some(self.note_velocity),
                    },
                    column => column,
                };
                Some((track, phrase, range, column, generate))
            })
            .collect();
        let Some(&(track, phrase, first, column, generate)) = places.first() else {
            return;
        };
        let fresh = matches!(
            shape.kind,
            FillKind::Ramp | FillKind::Random | FillKind::Walk | FillKind::Pink
        );
        let (from, to) = if !fresh {
            (shape.from, shape.to)
        } else if generate {
            self.generated_ends(column)
        } else {
            default_ends(steps_on(track, phrase), first, column)
        };
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.shape_seed);
        let mut actions: Vec<Action> = places
            .into_iter()
            .enumerate()
            .map(
                |(nth, (track, phrase, range, column, generate))| Action::Shape {
                    track,
                    phrase,
                    range,
                    column,
                    shape: Shape {
                        from,
                        to,
                        seed: track_seed(seed, nth),
                        place: if generate {
                            generating(shape.place, column)
                        } else {
                            shape.place
                        },
                        ..shape
                    },
                },
            )
            .collect();
        let action = if actions.len() == 1 {
            actions.remove(0)
        } else {
            Action::Batch(actions)
        };
        self.apply(&action);
    }

    /// Handle a key while the panel is open.
    pub(crate) fn shape_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart),
            _ => {}
        }
        let typing = self.shape.as_ref().is_some_and(|p| p.typing.is_some());
        // Cmd with a movement key is the `=` `-` `+` or `_` it stands for, as everywhere, but
        // types nothing.
        let key = match (key.command, adjusting_key(key)) {
            (false, _) => key,
            (true, Some(plain)) if !typing => plain,
            (true, _) => return,
        };
        if typing {
            self.shape_typing_key(key.code);
            return;
        }
        match key.code {
            KeyCode::Esc => self.cancel_shape(),
            KeyCode::Enter => self.keep_shape(false),
            KeyCode::Char('L') => self.keep_shape(true),
            KeyCode::Char('j') | KeyCode::Down => self.move_shape_row(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_shape_row(-1),
            KeyCode::Char('=') => self.adjust_shape(1, false),
            KeyCode::Char('-') => self.adjust_shape(-1, false),
            KeyCode::Char('+') => self.adjust_shape(1, true),
            KeyCode::Char('_') => self.adjust_shape(-1, true),
            KeyCode::Char('x') => self.reset_shape_row(),
            KeyCode::Char('~') => self.change_shape(|panel| {
                let shape = &mut panel.shape;
                if shape.kind == FillKind::List {
                    shape.values = shape.values.reversed();
                } else {
                    std::mem::swap(&mut shape.from, &mut shape.to);
                }
            }),
            KeyCode::Char('r') => {
                self.shape_seed = self.shape_seed.wrapping_add(1);
                let seed = self.shape_seed;
                self.change_shape(|panel| {
                    let shape = &mut panel.shape;
                    // With the pitches kept, only the second die rolls: accents, slides, octaves.
                    if shape.kind == FillKind::Acid && shape.acid.keep {
                        shape.acid.roll = shape.acid.roll.wrapping_add(seed).wrapping_add(1);
                    } else {
                        shape.seed = panel.base.seed.wrapping_add(seed);
                    }
                });
            }
            KeyCode::Char('K') => self.open_shape_picker(),
            KeyCode::Char('i') => self.start_shape_typing(),
            _ => {}
        }
    }

    fn change_shape(&mut self, change: impl FnOnce(&mut ShapePanel)) {
        if let Some(panel) = &mut self.shape {
            change(panel);
            // A list starts as the two ends, so it sounds as `Alternate` did until it's typed.
            let shape = &mut panel.shape;
            if shape.kind == FillKind::List && shape.values.is_empty() {
                shape.values = Values::new(&[shape.from, shape.to]);
            }
        }
        self.preview_shape();
    }

    fn move_shape_row(&mut self, by: isize) {
        if let Some(panel) = &mut self.shape {
            let rows = panel.rows();
            let at = rows.iter().position(|row| *row == panel.row).unwrap_or(0);
            let to = at.saturating_add_signed(by).min(rows.len() - 1);
            panel.row = rows[to];
        }
    }

    fn adjust_shape(&mut self, sign: i16, large: bool) {
        if let Some(panel) = self.shape.as_ref().filter(|p| p.row == ShapeRow::Values) {
            self.status = match panel.column {
                ShapeColumn::Note { .. } => "Type the notes with i, like C-3 E-3 G-3",
                ShapeColumn::Nudge => "Type the nudges with i, like -3 0 +3",
                _ => "Type the values with i, as hex values like 70 40 55",
            }
            .to_owned();
            return;
        }
        self.change_shape(|panel| {
            let column = panel.column;
            let open = panel.open_rows() as i16;
            let fills = panel.fills();
            let wheres = panel.wheres();
            let shape = &mut panel.shape;
            let bend = sign * if large { BEND_JUMP } else { BEND_NUDGE };
            let most = i16::from(MAX_BEND);
            match panel.row {
                ShapeRow::From => shape.from = stepped(column, shape.from, sign, large),
                ShapeRow::To => shape.to = stepped(column, shape.to, sign, large),
                ShapeRow::Curve => {
                    shape.curve = moved(i16::from(shape.curve), bend, -most, most) as i8;
                }
                ShapeRow::S => shape.s = moved(i16::from(shape.s), bend, -most, most) as i8,
                ShapeRow::Fill => shape.kind = turned(&fills, shape.kind, sign),
                ShapeRow::Wave => shape.wave = turned(&WAVES, shape.wave, sign),
                ShapeRow::Rate if large => shape.rate = shape.rate.jumped(sign > 0),
                ShapeRow::Rate => shape.rate = shape.rate.nudged(sign > 0),
                ShapeRow::Phase => {
                    let by = sign * if large { PHASE_JUMP } else { PHASE_NUDGE };
                    shape.phase = (shape.phase as i16 + by).rem_euclid(360) as u16;
                }
                ShapeRow::Values => {}
                ShapeRow::Run => {
                    let by = sign * if large { RUN_JUMP } else { 1 };
                    shape.run = moved(shape.run as i16, by, 1, MAX_RUN) as u16;
                }
                ShapeRow::Where => {
                    let same = |choice: &Where| {
                        std::mem::discriminant(choice) == std::mem::discriminant(&shape.place)
                    };
                    let at = wheres.iter().position(same).unwrap_or(0);
                    let step = if sign > 0 { 1 } else { wheres.len() - 1 };
                    shape.place = wheres[(at + step) % wheres.len()];
                }
                ShapeRow::Amount => match shape.place {
                    Where::Every(every) => {
                        let by = sign * if large { EVERY_JUMP } else { 1 };
                        shape.place = Where::Every(moved(every as i16, by, 1, 256) as u16);
                    }
                    Where::Chance(percent) => {
                        let by = sign * if large { CHANCE_JUMP } else { 1 };
                        shape.place = Where::Chance(moved(i16::from(percent), by, 0, 100) as u8);
                    }
                    _ => {}
                },
                ShapeRow::Notes => {
                    let by = sign * if large { 4 } else { 1 };
                    let most = i16::from(MAX_ACID_NOTES);
                    shape.acid.notes = moved(i16::from(shape.acid.notes), by, 1, most) as u8;
                }
                ShapeRow::Octave | ShapeRow::Density | ShapeRow::Accent | ShapeRow::Slide => {
                    let by = sign * if large { CHANCE_JUMP } else { 1 };
                    if let Some(percent) = acid_chance(&mut shape.acid, panel.row) {
                        *percent = moved(i16::from(*percent), by, 0, 100) as u8;
                    }
                }
                ShapeRow::Pitches => shape.acid.keep = !shape.acid.keep,
                ShapeRow::Step => {
                    shape.walk.step = walk_step(column, shape.walk.step, sign, large);
                }
                ShapeRow::Edge => shape.walk.edge = shape.walk.edge.other(),
                ShapeRow::Depth => {
                    let by = sign * if large { 2 } else { 1 };
                    let (low, high) = (i16::from(MIN_DEPTH), i16::from(MAX_DEPTH));
                    shape.pink.depth = moved(i16::from(shape.pink.depth), by, low, high) as u8;
                }
                ShapeRow::Hits | ShapeRow::Rotate => {
                    if let Where::Euclid { hits, rotate } = shape.place {
                        let by = sign * if large { EUCLID_JUMP } else { 1 };
                        shape.place = if panel.row == ShapeRow::Hits {
                            let hits = moved(hits.min(open as u16) as i16, by, 0, open) as u16;
                            Where::Euclid { hits, rotate }
                        } else {
                            let rotate = (rotate as i16 + by).rem_euclid(open.max(1)) as u16;
                            Where::Euclid { hits, rotate }
                        };
                    }
                }
            }
        });
    }

    /// Put the current row back to what it started as.
    fn reset_shape_row(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let ends = self.generated_ends(panel.parts[0].column);
        self.change_shape(|panel| {
            let first = panel.parts[0];
            let (from, to) = if first.generate {
                ends
            } else {
                default_ends(panel.base_steps(&first), first.range, first.column)
            };
            let default_where = panel.default_where();
            let shape = &mut panel.shape;
            match panel.row {
                ShapeRow::From => shape.from = from,
                ShapeRow::To => shape.to = to,
                ShapeRow::Curve => shape.curve = 0,
                ShapeRow::S => shape.s = 0,
                ShapeRow::Fill => shape.kind = FillKind::Ramp,
                ShapeRow::Wave => shape.wave = Wave::Sine,
                ShapeRow::Rate => shape.rate = DEFAULT_RATE,
                ShapeRow::Phase => shape.phase = 0,
                ShapeRow::Values => shape.values = Values::new(&[shape.from, shape.to]),
                ShapeRow::Run => shape.run = 1,
                ShapeRow::Where => shape.place = default_where,
                ShapeRow::Amount => {
                    shape.place = match shape.place {
                        Where::Every(_) => Where::Every(4),
                        Where::Chance(_) => Where::Chance(50),
                        place => place,
                    };
                }
                ShapeRow::Notes => shape.acid.notes = Acid::default().notes,
                ShapeRow::Octave | ShapeRow::Density | ShapeRow::Accent | ShapeRow::Slide => {
                    let mut start = Acid::default();
                    let from = acid_chance(&mut start, panel.row).map(|percent| *percent);
                    if let (Some(percent), Some(from)) =
                        (acid_chance(&mut shape.acid, panel.row), from)
                    {
                        *percent = from;
                    }
                }
                ShapeRow::Pitches => shape.acid.keep = false,
                ShapeRow::Step => shape.walk.step = default_step(panel.column),
                ShapeRow::Edge => shape.walk.edge = Edge::default(),
                ShapeRow::Depth => shape.pink.depth = Pink::default().depth,
                ShapeRow::Hits | ShapeRow::Rotate => {
                    if let Where::Euclid { hits, rotate } = shape.place {
                        shape.place = if panel.row == ShapeRow::Hits {
                            Where::Euclid {
                                hits: EUCLID_HITS,
                                rotate,
                            }
                        } else {
                            Where::Euclid { hits, rotate: 0 }
                        };
                    }
                }
            }
            if !panel.rows().contains(&panel.row) {
                panel.row = ShapeRow::Where;
            }
        });
    }

    fn open_shape_picker(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let (title, items, position): (&'static str, Vec<Item>, usize) = match panel.row {
            ShapeRow::Fill => {
                let fills = panel.fills();
                let position = fills
                    .iter()
                    .position(|kind| *kind == panel.shape.kind)
                    .unwrap_or(0);
                let items = fills
                    .into_iter()
                    .map(|kind| {
                        Item::new(kind.name(), Pick::ShapeFill(kind)).meaning(kind.description())
                    })
                    .collect();
                ("Fill", items, position)
            }
            ShapeRow::Wave => (
                "Shape",
                WAVES
                    .map(|wave| Item::new(wave.name(), Pick::ShapeWave(wave)))
                    .to_vec(),
                WAVES
                    .iter()
                    .position(|wave| *wave == panel.shape.wave)
                    .unwrap_or(0),
            ),
            ShapeRow::Rate => {
                let rates: Vec<WaveRate> = note_rates()
                    .into_iter()
                    .map(WaveRate::Time)
                    .chain(LISTED_CYCLES.map(WaveRate::Cycles))
                    .collect();
                let position = rates
                    .iter()
                    .position(|rate| *rate == panel.shape.rate)
                    .unwrap_or(0);
                let items = rates
                    .into_iter()
                    .map(|rate| Item::new(rate.to_string(), Pick::ShapeRate(rate)))
                    .collect();
                ("Rate", items, position)
            }
            ShapeRow::Where => {
                let choices = panel.wheres();
                let same = |choice: &Where| {
                    std::mem::discriminant(choice) == std::mem::discriminant(&panel.shape.place)
                };
                let position = choices.iter().position(same).unwrap_or(0);
                let items = choices
                    .into_iter()
                    .map(|place| Item::new(place.name(), Pick::ShapeWhere(place)))
                    .collect();
                ("Where", items, position)
            }
            _ => {
                self.status = "K chooses from a list on Fill, Shape, Rate and Where".to_owned();
                return;
            }
        };
        self.show_picker(Picker::new(title, items).opened_on(Some(position)));
    }

    pub(crate) fn pick_shape_fill(&mut self, kind: FillKind) {
        self.change_shape(|panel| panel.shape.kind = kind);
    }

    pub(crate) fn pick_shape_wave(&mut self, wave: Wave) {
        self.change_shape(|panel| panel.shape.wave = wave);
    }

    pub(crate) fn pick_shape_rate(&mut self, rate: WaveRate) {
        self.change_shape(|panel| panel.shape.rate = rate);
    }

    pub(crate) fn pick_shape_where(&mut self, place: Where) {
        self.change_shape(|panel| panel.shape.place = place);
    }

    fn start_shape_typing(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        if matches!(panel.row, ShapeRow::Fill | ShapeRow::Wave | ShapeRow::Where) {
            self.open_shape_picker();
        } else if matches!(panel.row, ShapeRow::Pitches | ShapeRow::Edge) {
            self.adjust_shape(1, false);
        } else if let Some(panel) = &mut self.shape {
            panel.typing = Some(String::new());
        }
    }

    fn shape_typing_key(&mut self, code: KeyCode) {
        let Some(panel) = &mut self.shape else {
            return;
        };
        let in_hex = typed_in_hex(panel.column);
        let end = matches!(panel.row, ShapeRow::From | ShapeRow::To);
        // A walk's step is typed as its column's values are, in hex or as a number.
        let hex = (end || panel.row == ShapeRow::Step) && in_hex;
        // A note's name or a nudge, alone or in a list, ended with Enter.
        let named = (end || panel.row == ShapeRow::Values) && !in_hex;
        let signed = matches!(panel.row, ShapeRow::Curve | ShapeRow::S);
        let rate = panel.row == ShapeRow::Rate;
        let list = panel.row == ShapeRow::Values;
        let Some(text) = &mut panel.typing else {
            return;
        };
        let longest = if list { NAMED_LIST_TEXT } else { 4 };
        match code {
            KeyCode::Esc => panel.typing = None,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Enter => self.commit_shape_typing(),
            KeyCode::Char(c)
                if named
                    && text.len() < longest
                    && (c.is_ascii_alphanumeric()
                        || matches!(c, '#' | '-' | '+' | '_')
                        || (list && c == ' ')) =>
            {
                text.push(c);
            }
            KeyCode::Char(_) if named => {}
            KeyCode::Char(c) if list => type_list(text, c),
            KeyCode::Char(c) if hex && c.is_ascii_hexdigit() => {
                text.push(c.to_ascii_uppercase());
                if text.len() == 2 {
                    self.commit_shape_typing();
                }
            }
            KeyCode::Char(c)
                if rate
                    && text.len() < RATE_TEXT
                    && (c.is_ascii_alphanumeric() || matches!(c, '/' | ' ')) =>
            {
                text.push(c);
            }
            KeyCode::Char(c)
                if !hex
                    && !rate
                    && text.len() < 4
                    && (c.is_ascii_digit()
                        || (signed && text.is_empty() && matches!(c, '-' | '+'))) =>
            {
                text.push(c);
            }
            _ => {}
        }
    }

    fn commit_shape_typing(&mut self) {
        let Some(text) = self.shape.as_mut().and_then(|panel| panel.typing.take()) else {
            return;
        };
        let mut refused = None;
        self.change_shape(|panel| {
            let column = panel.column;
            let open = panel.open_rows() as u16;
            let shape = &mut panel.shape;
            let unknown = match column {
                ShapeColumn::Note { .. } => "A note is like C-3, C#3 or c3",
                ShapeColumn::Nudge => "A nudge is like -3, 0 or +B, up to B either way",
                _ => "A value is two hex digits",
            };
            match panel.row {
                ShapeRow::From | ShapeRow::To => match column.parse(&text) {
                    Some(value) if panel.row == ShapeRow::From => shape.from = value,
                    Some(value) => shape.to = value,
                    None => refused = Some(unknown),
                },
                ShapeRow::Curve | ShapeRow::S => match text.parse::<i16>() {
                    Ok(value) => {
                        let most = i16::from(MAX_BEND);
                        let value = value.clamp(-most, most) as i8;
                        if panel.row == ShapeRow::Curve {
                            shape.curve = value;
                        } else {
                            shape.s = value;
                        }
                    }
                    Err(_) => refused = Some("Curve and S run from -100 to 100"),
                },
                ShapeRow::Amount => match (shape.place, text.parse::<u16>()) {
                    (Where::Every(_), Ok(every)) if every > 0 => {
                        shape.place = Where::Every(every);
                    }
                    (Where::Chance(_), Ok(percent)) => {
                        shape.place = Where::Chance(percent.min(100) as u8);
                    }
                    _ => refused = Some("That amount isn't a whole number above 0"),
                },
                ShapeRow::Hits | ShapeRow::Rotate => match (shape.place, text.parse::<u16>()) {
                    (Where::Euclid { rotate, .. }, Ok(hits)) if panel.row == ShapeRow::Hits => {
                        shape.place = Where::Euclid {
                            hits: hits.min(open),
                            rotate,
                        };
                    }
                    (Where::Euclid { hits, .. }, Ok(rotate)) => {
                        shape.place = Where::Euclid {
                            hits,
                            rotate: rotate % open.max(1),
                        };
                    }
                    _ => refused = Some("Hits and Rotate are whole numbers"),
                },
                // A wave here is laid out by rows, so a rate has to be in time with them.
                ShapeRow::Rate => match text.parse::<WaveRate>() {
                    Ok(rate) => shape.rate = rate,
                    Err(_) => {
                        refused = Some("A rate here is like 1/4, 1/8d, 2 bars, 7st, or x3 cycles");
                    }
                },
                ShapeRow::Phase => match text.parse::<u16>() {
                    Ok(phase) if phase < 360 => shape.phase = phase,
                    _ => refused = Some("Phase runs from 0 to 359"),
                },
                ShapeRow::Notes => match text.parse::<u8>() {
                    Ok(notes) if notes > 0 => shape.acid.notes = notes.min(MAX_ACID_NOTES),
                    _ => refused = Some("Notes runs from 1 to 8"),
                },
                ShapeRow::Octave | ShapeRow::Density | ShapeRow::Accent | ShapeRow::Slide => {
                    match (acid_chance(&mut shape.acid, panel.row), text.parse::<u16>()) {
                        (Some(percent), Ok(typed)) => *percent = typed.min(100) as u8,
                        _ => refused = Some("A chance runs from 0 to 100"),
                    }
                }
                ShapeRow::Step => {
                    let typed = if typed_in_hex(column) {
                        u8::from_str_radix(&text, 16)
                    } else {
                        text.parse::<u8>()
                    };
                    let longest = longest_step(column);
                    match typed {
                        Ok(step) if step > 0 => shape.walk.step = step.min(longest),
                        _ if typed_in_hex(column) => {
                            refused = Some("Step is a hex value above 00");
                        }
                        _ => refused = Some("Step is a whole number above 0"),
                    }
                }
                ShapeRow::Depth => match text.parse::<u8>() {
                    Ok(depth) if (MIN_DEPTH..=MAX_DEPTH).contains(&depth) => {
                        shape.pink.depth = depth;
                    }
                    _ => refused = Some("Depth runs from 2 to 8"),
                },
                ShapeRow::Run => match text.parse::<i16>() {
                    Ok(run) if run > 0 => shape.run = run.min(MAX_RUN) as u16,
                    _ => refused = Some("Run is a whole number above 0"),
                },
                ShapeRow::Values => {
                    let typed: Option<Vec<u8>> = text
                        .split_whitespace()
                        .map(|value| column.parse(value))
                        .collect();
                    match typed {
                        Some(list) if !list.is_empty() => shape.values = Values::new(&list),
                        _ if !typed_in_hex(column) => refused = Some(unknown),
                        _ => refused = Some("Values are hex values like 70 40 55, up to 8 of them"),
                    }
                }
                ShapeRow::Fill
                | ShapeRow::Wave
                | ShapeRow::Where
                | ShapeRow::Pitches
                | ShapeRow::Edge => {}
            }
        });
        if let Some(reason) = refused {
            self.status = reason.to_owned();
        }
    }
}
