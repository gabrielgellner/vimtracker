//! The shape panel: fill a selected column with a ramp, random values, a walk, a wave, two values
//! in turn or a short list of them, or rework the values there, heard while it's adjusted. On the
//! Note column it can also follow another track's phrase.
//!
//! What a live generator is read as, which the Gen page, the Roto and macros share with the
//! panel, is in [`crate::services::generators`]; making one live, reopening it and applying it
//! are in [`live`].

pub mod draw;
mod live;

use std::sync::Arc;
use std::time::Instant;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::notation::{
    Around, Field as TokenField, GrammarId, Kind, STACKED, Spot, Table, compile_rhythm, kit_start,
    mini_layer, mini_start, rhythm_preset, stacks, with_insert,
};
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{
    Channel, Context, DrumPart, FillKind, GenParam, Grids, KitLine, Own, OwnRhythm, Shape,
    ShapeColumn, Source, Starts, TonnetzVoice, Walk, Where, counted, default_ends, default_step,
    degree_count, degree_name, divisions_seed, dropped, drum_parts, is_empty, keeping_step,
    kept_list, line_steps, on_track, open_rows, track_seed, variation_notes, written_from,
};
use vt_model::song::{Machine, Song, SynthVoice, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Note, Step};

use crate::app::{App, Selection, TrackSpan};
use crate::input::bindings::{Cmd, DESCRIBE, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::tokens::{Did, Opening, TokenKey, Tokens};
use crate::input::typed::{Typed, TypedValue};
use crate::overlays::Overlay;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;
use crate::services::generators::{Part, column_label, settle, too_few};
use crate::widgets::tokens::{reached, said_after};
use crate::widgets::value_bars::{BarRow, ValueBars};

/// Why a kit line isn't made on these tracks, by their numbers: they have no names, which its
/// hits would be (decision 472).
fn unnamed_said(tracks: &[usize]) -> String {
    let numbers: Vec<String> = tracks.iter().map(|at| format!("{:02}", at + 1)).collect();
    match numbers.split_last() {
        Some((last, [])) => {
            format!(
                "Track {last} has no name, and a kit line plays its tracks by their names: :name it first"
            )
        }
        Some((last, rest)) => format!(
            "Tracks {} and {last} have no names, and a kit line plays its tracks by their names: :name them first",
            rest.join(", ")
        ),
        None => String::new(),
    }
}

/// The title of the list offering a beat after a kit's sounds are loaded as tracks.
pub const KIT_BEAT_PICKER: &str = "Start a beat";

/// The rows a shape fills on a column it generates on. Where it would choose the notes or the
/// commands already there, which an empty column hasn't, it takes every row.
fn generating(place: Where, column: ShapeColumn) -> Where {
    match (place, column) {
        (Where::Notes, ShapeColumn::Note { .. }) | (Where::Commands, _) => Where::AllRows,
        (place, _) => place,
    }
}

/// A `Where` of `Cells` on the `nth` of the tracks shaped at once, each its own cell of one
/// automaton, the row they start from worked out once for all of them, from `seed` or from
/// `hits`, whether each has a note on its first row. Any other is as it is.
fn cells_across(place: Where, seed: u64, nth: usize, hits: &[bool]) -> Where {
    match place {
        Where::Cells(cells) => Where::Cells(cells.for_track(seed, nth, hits)),
        place => place,
    }
}

/// Whether a phrase's steps have a note on a range's first row, which a `Cells` across tracks
/// can start from.
fn first_hit(steps: &[Step], range: StepRange) -> bool {
    steps
        .get(range.first)
        .is_some_and(|step| matches!(step.note, Note::On(_)))
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
    /// The values a `List` goes through, held beside the shape as a generator keeps them, as they
    /// don't fit in one. Kept while another fill is tried, and given only to a `List`.
    pub list: Vec<u8>,
    /// An `Own` rhythm's text, held beside the shape likewise, and given only to a `Where` of
    /// `Own` (decision 461).
    pub rhythm: String,
    /// A `Mini`'s line, held likewise and given only to a `Mini` (decision 462): across tracks,
    /// a stack, `,`, gives each track the next of its layers.
    pub mini: String,
    /// The kit line a `Mini` makes across a group's tracks, or is, where the panel spans two or
    /// more tracks of one group on their Note column or opened on one (decision 464): given to
    /// the shape while its fill is `Mini`.
    pub kit: Option<KitLine>,
    /// The names of the tracks that kit line spans, which its hits are: empty for no kit line.
    pub names: Vec<String>,
    /// The note each part's track plays a kit line's hits at: the first in its rows, or none,
    /// while `From` is still `kit_from`, as the panel opened (decision 464). Turning `From` gives
    /// every track the one note.
    kit_notes: Vec<Option<u8>>,
    kit_from: u8,
    /// The setting the cursor is on.
    pub row: GenParam,
    /// What's being typed into the current row.
    pub typing: Option<TypedValue>,
    /// The token the cursor is on in the row's buffer, while it's open (decision 463), by its
    /// place: on `Weights`, the digit, by its degree from the root (decisions 388 and 456);
    /// a `List`'s value, a `Tonnetz`'s move or a mask's bit (decision 459).
    pub weight: Option<usize>,
    /// The row's text while its buffer is open.
    pub tokens: Option<Tokens>,
    /// The live generator it was opened on with `gl`, by its place in its phrase's list, which
    /// it reshapes rather than shaping the rows once.
    pub live: Option<usize>,
    /// The song from before the panel opened.
    base: Arc<Song>,
    /// On a live generator, its phrase's steps as they'd be without it, which it shapes from.
    under: Option<Vec<Step>>,
    /// The phrases a `Follow` can read, by track and then ID: those on sound tracks but the
    /// parts' own, and any that read one of them, which would go round a cycle.
    pub sources: Vec<Source>,
    /// The one it starts on: the phrase beside it on the nearest track with one, before it first,
    /// or what that one follows.
    default_source: Option<Source>,
    /// Whether `Enter` or `L` kept it, as it closes.
    pub(crate) kept: bool,
    /// Whether it was `L` that kept it, live.
    pub(crate) kept_live: bool,
    /// What makes it a pattern entered with `!`, drawn as its buffer alone (decision 474).
    pub pattern: Option<PatternEntry>,
}

/// A pattern entered with `!` (decision 474): a `Mini` on the cursor's column whose line counts
/// from the phrase's row 00 and goes round down the whole phrase, written only on the rows
/// selected, or every row, and edited in its buffer alone.
#[derive(Debug, Clone)]
pub struct PatternEntry {
    /// The line as typed, `x` and all, which the buffer shows. The panel's `mini` is the same
    /// with each `x` written out as the last note typed.
    pub typed: String,
    /// The last note typed, by name, which `x` is.
    pub note: String,
    /// Whether rows were selected, which the grid shows while it's open.
    pub selected: bool,
    /// How many rows the phrase has, where the line is cut.
    pub rows: usize,
}

/// A `Step`'s rows, as the status line says them.
fn rows_word(step: u8) -> &'static str {
    if step == 1 { "row" } else { "rows" }
}

/// The `Step`s `[s` and `]s` go between.
const STEPS: [u8; 5] = [1, 2, 4, 8, 16];

/// A `Step` halved or doubled, to the next of [`STEPS`] below or above it.
fn stepped(step: u8, up: bool) -> u8 {
    if up {
        STEPS.into_iter().find(|to| *to > step).unwrap_or(16)
    } else {
        STEPS.into_iter().rfind(|to| *to < step).unwrap_or(1)
    }
}

impl ShapePanel {
    /// The rows on show: the settings its shape has, `Fill` first as it decides the rest
    /// ([`GenParam::exists_for`]). `Acid`'s `Density` shows where any track generates.
    pub fn rows(&self) -> Vec<GenParam> {
        let generates = self.parts.iter().any(|part| part.generate);
        GenParam::of(&self.shape, self.column, generates)
    }

    /// What its settings act on: its column, the rows it could change, whether every part is
    /// generated on, and what a `Follow` can read and reads now.
    pub fn context(&self) -> Context<'_> {
        Context {
            column: self.column,
            rows: self.counted_rows(),
            generates: self.generates(),
            sources: &self.sources,
            source: self.source_steps(),
            list: &self.list,
            rhythm: self.line(),
        }
    }

    /// The text its `Text` row shows: a `Mini`'s line, its stack and all, or an `Own` rhythm's.
    pub fn line(&self) -> &str {
        if self.shape.kind == FillKind::Mini {
            &self.mini
        } else {
            &self.rhythm
        }
    }

    /// The list a generator of its shape keeps: its own, on a `List`, and none on any other fill.
    pub fn kept_list(&self) -> Box<[u8]> {
        kept_list(&self.shape, &self.list)
    }

    /// The rhythm a generator of its shape keeps: its own, on `Own`, and none on any other
    /// `Where`, or a `Mini`'s line, with the beats it reads as, for its first part.
    pub fn own(&self) -> OwnRhythm {
        self.own_of(0)
    }

    /// The same for a part, by its place: a `Mini` across tracks takes the next of its line's
    /// layers, going round (decision 462).
    pub fn own_of(&self, nth: usize) -> OwnRhythm {
        let column = self.parts.get(nth).map_or(self.column, |part| part.column);
        // On a kit line, each track holds the whole line and plays its own name's hits.
        if self.shape.kit.is_some()
            && let Some(part) = self.parts.get(nth)
        {
            let name = self.base.tracks[part.track].label(part.track);
            return OwnRhythm::kept_as(&self.shape, column, self.mini.trim(), &name);
        }
        let text = match self.shape.kind {
            FillKind::Mini => mini_layer(&self.mini, nth),
            _ => &self.rhythm,
        };
        OwnRhythm::kept(&self.shape, column, text)
    }

    /// The text of that rhythm, or of the first part's layer.
    pub fn kept_rhythm(&self) -> Box<str> {
        self.kept_rhythm_of(0)
    }

    /// The text a part's generator keeps, by its place.
    pub fn kept_rhythm_of(&self, nth: usize) -> Box<str> {
        self.own_of(nth).text().into()
    }

    /// The text `.` shapes again with: a `Mini`'s whole line, its stack and all, so each track
    /// takes a layer again, or an `Own` rhythm's.
    pub fn kept_line(&self) -> Box<str> {
        match self.shape.kind {
            FillKind::Mini => self.mini.trim().into(),
            _ => self.kept_rhythm(),
        }
    }

    /// The bars of the column it shapes on the phrase in view, each row of its span with its value
    /// as the panel opened and as it's shaped now, or none where the phrase in view isn't one it
    /// shapes or the column's values aren't amounts.
    pub(crate) fn value_bars(&self, ctx: &Ctx) -> Option<ValueBars> {
        let (track, phrase) = (ctx.cursor().track, ctx.phrase_id()?);
        let nth = self
            .parts
            .iter()
            .position(|part| part.track == track && part.phrase == phrase)?;
        let part = &self.parts[nth];
        let steps = self.base_steps(part);
        let own = self.own_of(nth);
        let texts = own.texts(&self.list);
        let written = written_from(
            steps,
            part.range,
            part.column,
            &self.shape_of(nth),
            texts,
            self.source_steps(),
        );
        let rows = (part.range.first..=part.range.last)
            .filter_map(|step| {
                let before = steps.get(step)?;
                let now = written
                    .iter()
                    .find(|(row, _)| *row == step)
                    .map_or(before, |(_, step)| step);
                Some(BarRow {
                    step,
                    before: part.column.value(before),
                    now: part.column.value(now),
                })
            })
            .collect();
        ValueBars::new(ValueBars::label_of(self.column), self.column, rows)
    }

    /// The loop of the phrase a `Follow` reads, as the panel opened on it.
    pub(crate) fn source_steps(&self) -> Option<&[Step]> {
        if self.shape.kind != FillKind::Follow {
            return None;
        }
        self.base
            .source_phrase(self.shape.follow.source)
            .map(|phrase| phrase.loop_steps())
    }

    /// How many rows its `Where` counts over, which a Euclidean `Where`'s hits are spread over:
    /// the most of any track's.
    pub fn counted_rows(&self) -> usize {
        self.parts
            .iter()
            .map(|part| counted(self.base_steps(part), part.range, part.column, &self.shape))
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

    /// The shape each part takes: the panel's, with the part's own seed, on a column it
    /// generates on, every row where the panel chooses the notes or commands there, and across
    /// tracks, its own part of Grids' beat, its own channel of `Divisions`, each three tracks
    /// sharing a seed so the third plays the strikes of the other two, its side of
    /// `Complementary`'s coin, which each pair of tracks shares, its own cell of one `Cells`
    /// automaton, or its own voice of a `Tonnetz`' chords, every track walking from one seed.
    pub fn shape_of(&self, nth: usize) -> Shape {
        let part = &self.parts[nth];
        let channels =
            self.shape.kind == FillKind::Divisions && self.shape.divisions.channel.is_none();
        let placed = on_track(&self.shape, nth);
        let hits: Vec<bool> = self
            .parts
            .iter()
            .map(|part| first_hit(self.base_steps(part), part.range))
            .collect();
        let place = if part.generate {
            generating(placed.place, part.column)
        } else {
            placed.place
        };
        Shape {
            seed: if channels {
                divisions_seed(self.shape.seed, nth)
            } else {
                placed.seed
            },
            divisions: if channels {
                self.shape.divisions.for_track(nth)
            } else {
                self.shape.divisions
            },
            place: cells_across(place, self.shape.seed, nth, &hits),
            grids: Grids {
                part: self.shape.grids.part.or(Some(part.drum)),
                ..self.shape.grids
            },
            tonnetz: placed.tonnetz,
            // A kit line's track keeps its own note, until `From` is turned.
            from: match self.kit_notes.get(nth) {
                Some(Some(note))
                    if self.shape.kit.is_some() && self.shape.from == self.kit_from =>
                {
                    *note
                }
                _ => self.shape.from,
            },
            // A `Mini`'s layer, a track's, says whether it laps, ratchets and draws.
            mini: match self.shape.kind {
                FillKind::Mini => {
                    let mut shape = self.shape;
                    self.own_of(nth).settle(&mut shape);
                    shape.mini
                }
                _ => self.shape.mini,
            },
            ..self.shape
        }
    }

    /// The speed of the first track it shapes.
    pub fn speed(&self) -> Option<Speed> {
        let first = self.parts.first()?;
        Some(self.base.tracks.get(first.track)?.speed)
    }

    /// Why a `Variation` has nothing to learn from, if every part has too few notes.
    pub(crate) fn too_few_notes(&self) -> Option<String> {
        let most = self
            .parts
            .iter()
            .map(|part| variation_notes(self.base_steps(part), part.range))
            .max()?;
        too_few(&self.shape, most)
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

    /// The edit that gives every part its shape: once, or the live generator a new shape.
    fn action(&self) -> Action {
        if let Some(generator) = self.live {
            let part = self.parts[0];
            return Action::Reshape {
                track: part.track,
                phrase: part.phrase,
                generator,
                shape: self.shape,
                list: Some(self.kept_list()),
                rhythm: Some(self.kept_rhythm()),
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
                list: self.kept_list(),
                rhythm: self.kept_rhythm_of(nth),
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
                list: self.kept_list(),
                rhythm: self.kept_rhythm_of(nth),
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

/// The shape the panel opens with on a column, from and to these, on these rows: a ramp, with
/// every other fill's settings as they start.
fn starting_shape(column: ShapeColumn, (from, to): (u8, u8), place: Where) -> Shape {
    Shape {
        from,
        to,
        place,
        walk: Walk {
            step: default_step(column),
            ..Walk::default()
        },
        ..Shape::default()
    }
}

/// The list of a generator setting's choices, as the panel and the Gen page list them: `param`'s
/// in `context`, titled `title` and opened on the one `shape` has. Each item picks its choice
/// through `pick` and says what it means, and a phrase a `Follow` can read is described by
/// `follows`, where it's given. `None` if the setting isn't a list.
pub(crate) fn choice_picker(
    title: &'static str,
    param: GenParam,
    shape: &Shape,
    context: &Context,
    pick: impl Fn(Choice) -> Pick,
    follows: Option<&dyn Fn(Source) -> String>,
) -> Option<Picker> {
    let choices = param.choices(context)?;
    let position = param.chosen(shape, context).unwrap_or(0);
    let items = choices
        .into_iter()
        .map(|choice| {
            let item = Item::new(choice.name(), pick(choice));
            match (choice, follows, choice.description()) {
                (Choice::Source(source), Some(follows), _) => item.meaning(follows(source)),
                (_, _, Some(meaning)) => item.meaning(meaning),
                (_, _, None) => item,
            }
        })
        .collect();
    Some(Picker::new(title, items).opened_on(Some(position)))
}

/// What a list of FX commands opens once its command is chosen: the shape panel, or a
/// pattern's buffer, on rows selected or every row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opens {
    Shape,
    Pattern { selected: bool },
}

/// A selection waiting for its FX command: its rows, its column and what it opens.
pub(crate) type Waiting = (Selection, Field, Opens);

/// Why a selection shapes nothing yet.
enum Refusal {
    Reason(&'static str),
    /// An FX column on this track is empty, so the command to write on it is chosen first.
    Command(usize),
    /// Why is already on the status line.
    Said,
}

impl Services {
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
            // A row chosen without a note takes one of its own, at the velocity notes are typed
            // with.
            Field::Note => (
                ShapeColumn::Note {
                    scale: self.live.scale,
                    writes: Some(self.live.note_velocity),
                },
                !notes,
            ),
            Field::Velocity => on_notes(ShapeColumn::Velocity)?,
            // As far as the track's nudges go at its speed.
            Field::Nudge => on_notes(ShapeColumn::Nudge {
                most: track.speed.most_nudge(),
            })?,
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
        view: View,
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
            let Some((phrase, range)) = self.place_on(track, selection.range, view) else {
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
                    drum: DrumPart::default(),
                }),
                Err(Refusal::Command(track)) => return Err(Refusal::Command(track)),
                Err(Refusal::Reason(reason)) => {
                    refused.get_or_insert(reason);
                }
                Err(Refusal::Said) => {}
            }
        }
        if parts.is_empty() {
            return Err(Refusal::Reason(
                refused.unwrap_or("There's nothing to shape here"),
            ));
        }
        let tracks: Vec<usize> = parts.iter().map(|part| part.track).collect();
        for (part, drum) in parts.iter_mut().zip(self.drums_of(&tracks)) {
            part.drum = drum;
        }
        Ok(parts)
    }

    /// The part each of these tracks plays, in order: its sound's, or the first the others leave.
    fn drums_of(&self, tracks: &[usize]) -> Vec<DrumPart> {
        let known: Vec<Option<DrumPart>> =
            tracks.iter().map(|track| self.drum_of(*track)).collect();
        drum_parts(&known)
    }

    /// What's offered after a kit's sounds are loaded as tracks: a beat from Grids' map on the
    /// first kick, snare and hat among them, each playing its own part as `S` across them would,
    /// in a list whose `Enter` writes it and whose `Esc` leaves the tracks empty. None if there's
    /// no drum among them.
    pub(crate) fn kit_beat_picker(&self, tracks: &[usize]) -> Option<Picker> {
        let mut found = [false; DrumPart::ALL.len()];
        let drums: Vec<usize> = tracks
            .iter()
            .copied()
            .filter(|&track| {
                self.drum_of(track)
                    .is_some_and(|part| !std::mem::replace(&mut found[part.index()], true))
            })
            .collect();
        if drums.is_empty() {
            return None;
        }
        let names: Vec<&str> = drums
            .iter()
            .map(|&track| self.song().tracks[track].name.as_str())
            .collect();
        let items = vec![
            Item::new("Grids", Pick::KitBeat(drums.clone()))
                .meaning(format!("A beat on {} from Grids' map", names.join(" "))),
            Item::new("None", Pick::KitBeat(Vec::new())).meaning("Leave the tracks empty"),
        ];
        Some(Picker::new(KIT_BEAT_PICKER, items))
    }

    /// Write a beat from Grids' map on these tracks' first phrases, each its own part, from the
    /// middle of the map as `S` starts, as one undo step.
    pub(crate) fn write_kit_beat(&mut self, tracks: &[usize]) {
        let song = Arc::clone(self.song());
        let column = ShapeColumn::Note {
            scale: self.live.scale,
            writes: Some(self.live.note_velocity),
        };
        let ends = self.generated_ends(column);
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.shape_seed);
        let drums = self.drums_of(tracks);
        let actions: Vec<Action> = tracks
            .iter()
            .zip(drums)
            .enumerate()
            .filter_map(|(nth, (&track, part))| {
                let chain = song
                    .sections
                    .first()?
                    .chains
                    .get(track)
                    .copied()
                    .flatten()?;
                let phrase = song.tracks[track]
                    .chains
                    .get(&chain)?
                    .entries
                    .first()?
                    .phrase;
                let rows = song.tracks[track].phrases.get(&phrase)?.steps().len();
                let mut shape = starting_shape(column, ends, Where::AllRows);
                shape.kind = FillKind::Grids;
                shape.seed = track_seed(seed, nth);
                shape.grids.part = Some(part);
                Some(Action::Shape {
                    track,
                    phrase,
                    range: StepRange::between(0, rows.saturating_sub(1)),
                    column,
                    shape,
                    list: Box::default(),
                    rhythm: Box::default(),
                })
            })
            .collect();
        if actions.is_empty() {
            return;
        }
        self.apply(&Action::Batch(actions));
        self.status =
            "A beat from Grids' map: S on its notes changes it, u takes it off".to_owned();
    }

    /// Apply a shape to a range, on each of these tracks. A ramp's, random fill's, walk's or 1/f
    /// fill's ends are chosen again from what's there on the first, a wave or `Alternate` keeps the two values it
    /// moves between, a `List` keeps its list, `list`, an `Own` rhythm its text, `rhythm`, and
    /// `Scale` the two it scales onto. Each
    /// track has a seed of its own. Each track's column is judged as `S` judges it: an empty one
    /// is generated on, notes of its own or the same command.
    pub(crate) fn shape_again(
        &mut self,
        range: StepRange,
        tracks: TrackSpan,
        column: ShapeColumn,
        (shape, list, rhythm): (Shape, &[u8], &str),
        view: View,
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
                let (phrase, range) = self.place_on(track, range, view)?;
                let generate = is_empty(steps_on(track, phrase), range, column);
                // Notes go through the scale as it is now.
                let column = match column {
                    ShapeColumn::Note { .. } => ShapeColumn::Note {
                        scale: self.live.scale,
                        writes: Some(self.live.note_velocity),
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
        // Grids across tracks gives each its own part, as `S` does, and `Divisions` each its own
        // channel.
        let tracks: Vec<usize> = places.iter().map(|place| place.0).collect();
        let drums = self.drums_of(&tracks);
        let channels = shape.kind == FillKind::Divisions && tracks.len() > 1;
        // A `Tonnetz` across tracks gives each a voice of the triad, as `S` does.
        let mut shape = shape;
        if tracks.len() > 1 {
            shape.tonnetz.voice = None;
        }
        let hits: Vec<bool> = places
            .iter()
            .map(|(track, phrase, range, ..)| first_hit(steps_on(*track, *phrase), *range))
            .collect();
        let mut actions: Vec<Action> = places
            .into_iter()
            .enumerate()
            .map(|(nth, (track, phrase, range, column, generate))| {
                // A seed of its own, or for `Complementary`, its pair's, and its side.
                let placed = on_track(&Shape { seed, ..shape }, nth);
                Action::Shape {
                    track,
                    phrase,
                    range,
                    column,
                    shape: Shape {
                        from,
                        to,
                        seed: if channels {
                            divisions_seed(seed, nth)
                        } else {
                            placed.seed
                        },
                        divisions: if channels {
                            shape.divisions.for_track(nth)
                        } else {
                            shape.divisions
                        },
                        place: cells_across(
                            if generate {
                                generating(placed.place, column)
                            } else {
                                placed.place
                            },
                            seed,
                            nth,
                            &hits,
                        ),
                        grids: Grids {
                            part: match shape.grids.part {
                                Some(part) if tracks.len() == 1 => Some(part),
                                _ => Some(drums[nth]),
                            },
                            ..shape.grids
                        },
                        tonnetz: placed.tonnetz,
                        ..shape
                    },
                    list: list.into(),
                    // A `Mini`'s stack gives each track the next of its layers.
                    rhythm: match shape.kind {
                        FillKind::Mini if shape.kit.is_none() => mini_layer(rhythm, nth).into(),
                        _ => rhythm.into(),
                    },
                }
            })
            .collect();
        let action = if actions.len() == 1 {
            actions.remove(0)
        } else {
            Action::Batch(actions)
        };
        self.apply(&action);
    }
}

impl ShapePanel {
    /// The panel a selection opens, on its first result, or why it opens none.
    fn new(
        selection: Selection,
        field: Field,
        command: Option<FxCommand>,
        services: &mut Services,
        view: View,
    ) -> Result<Self, Refusal> {
        let parts = services.shape_parts(selection, field, command, view)?;
        let base = Arc::clone(services.song());
        let first = parts[0];
        let steps = base.tracks[first.track]
            .phrases
            .get(&first.phrase)
            .map_or(&[][..], |p| p.steps());
        let generates = parts.iter().all(|part| part.generate);
        let alone = parts.len() == 1;
        let column = first.column;
        let (from, to) = if first.generate {
            services.generated_ends(first.column)
        } else {
            default_ends(steps, first.range, first.column)
        };
        services.shape_seed = services.shape_seed.wrapping_add(1);
        let place = if generates {
            Where::AllRows
        } else {
            column.default_where()
        };
        let (sources, default_source) = match column {
            ShapeColumn::Note { .. } => services.follow_sources(&base, &parts, view),
            _ => (Vec::new(), None),
        };
        let mut shape = starting_shape(column, (from, to), place);
        shape.seed = base.seed.wrapping_add(services.shape_seed);
        // Across tracks, each takes its own part, and its own channel; a track at `1/32` reads
        // its rows as 32nds, and `Divisions` plays its rows as long as the track's step.
        shape.grids.part = alone.then_some(first.drum);
        shape.divisions.channel = alone.then_some(Channel::A);
        shape.divisions = shape
            .divisions
            .for_step(base.tracks[first.track].speed.ticks());
        // A `Tonnetz` on the `Chord` voice writes its chords for it, and on any other its roots;
        // across tracks, each takes a voice of the triad.
        shape.tonnetz.voice = alone.then(|| match base.tracks[first.track].machine {
            Machine::Plaits(SynthVoice::Chord) => TonnetzVoice::Chord,
            _ => TonnetzVoice::Root,
        });
        // Two or more tracks of one group, on their Note column, make a kit line with `Mini`.
        let groups: Vec<Option<u8>> = parts
            .iter()
            .map(|part| base.tracks[part.track].group)
            .collect();
        let kit = match groups.first() {
            Some(Some(group))
                if parts.len() > 1
                    && matches!(column, ShapeColumn::Note { .. })
                    && groups.iter().all(|other| *other == Some(*group)) =>
            {
                Some(base.next_kit_line(*group))
            }
            _ => None,
        };
        let names = match kit {
            Some(_) => parts
                .iter()
                .map(|part| base.tracks[part.track].label(part.track))
                .collect(),
            None => Vec::new(),
        };
        let kit_notes = parts
            .iter()
            .map(|part| {
                let steps = base.tracks[part.track]
                    .phrases
                    .get(&part.phrase)
                    .map_or(&[][..], |p| p.steps());
                (part.range.first..=part.range.last).find_map(|row| match steps.get(row)?.note {
                    Note::On(note) => Some(note),
                    _ => None,
                })
            })
            .filter(|_| kit.is_some())
            .collect();
        Ok(Self {
            selection,
            parts,
            column,
            shape,
            list: Vec::new(),
            rhythm: String::new(),
            mini: String::new(),
            kit,
            names,
            kit_notes,
            kit_from: from,
            row: GenParam::Fill,
            typing: None,
            weight: None,
            tokens: None,
            live: None,
            base,
            under: None,
            sources,
            default_source,
            kept: false,
            kept_live: false,
            pattern: None,
        })
    }

    /// The panel `!` opens (decision 474): a `Mini` on the selection's column, at the `Step` a
    /// pattern was last given, counted from the phrase's row 00, with its buffer open on `text`,
    /// typing at its end, or on nothing typed yet. Across a group's tracks, a kit line, refused
    /// as `S` refuses one.
    fn pattern(
        selection: Selection,
        (field, command): (Field, Option<FxCommand>),
        services: &mut Services,
        (view, selected): (View, bool),
        text: &str,
    ) -> Result<Self, Refusal> {
        let mut panel = Self::new(selection, field, command, services, view)?;
        let first = panel.parts[0];
        let rows = panel
            .base
            .tracks
            .get(first.track)
            .and_then(|track| track.phrases.get(&first.phrase))
            .map_or(0, |phrase| phrase.steps().len());
        let note = ShapeColumn::Note {
            scale: services.live.scale,
            writes: None,
        }
        .show(services.live.insert_note());
        panel.pattern = Some(PatternEntry {
            typed: String::new(),
            note,
            selected,
            rows,
        });
        panel.row = GenParam::MiniText;
        let step = services.live.pattern_step;
        let skip = first.range.first as u16;
        // A degree counts from the root in the last note typed's octave, as `x` is that note;
        // a kit line's tracks each keep their own.
        let from = (panel.kit.is_none() && matches!(panel.column, ShapeColumn::Note { .. }))
            .then(|| services.live.insert_note());
        panel.change(
            |panel| {
                panel.shape.kind = FillKind::Mini;
                panel.shape.mini.step = step;
                panel.shape.mini.skip = skip;
                if let Some(from) = from {
                    panel.shape.from = from;
                }
            },
            services,
        );
        // A kit line on a track with no name is refused, as `S` refuses it, and said.
        if panel.shape.kind != FillKind::Mini {
            services.history.replace_current(Arc::clone(&panel.base));
            services.publish();
            return Err(Refusal::Said);
        }
        if !text.is_empty() {
            panel.commit_typing(text, services);
        }
        let how = if text.is_empty() {
            Opening::Replace
        } else {
            Opening::Append
        };
        panel.open_buffer(how, services);
        Ok(panel)
    }
}

impl App {
    /// Open the panel on the selection, and show its first result. An empty FX column lists the
    /// commands first, to choose the one it writes.
    pub(crate) fn open_shape(&mut self) {
        let selection = self.current_selection();
        let field = self.services.cursor.field;
        self.open_shape_on(selection, field, None);
    }

    /// Open the panel, or a pattern's buffer, on the selection, now its empty FX column has its
    /// command.
    pub(crate) fn pick_shape_command(&mut self, command: FxCommand) {
        match self.overlays.shape_command.take() {
            Some((selection, field, Opens::Shape)) => {
                self.open_shape_on(selection, field, Some(command));
            }
            Some((selection, field, Opens::Pattern { selected })) => {
                let text = std::mem::take(&mut self.overlays.pattern_text);
                self.open_pattern_on(selection, (field, Some(command)), selected, &text);
            }
            None => {}
        }
    }

    fn open_shape_on(&mut self, selection: Selection, field: Field, command: Option<FxCommand>) {
        let panel = match ShapePanel::new(
            selection,
            field,
            command,
            &mut self.services,
            self.modes.view,
        ) {
            Ok(panel) => panel,
            Err(refusal) => {
                self.refused(refusal, (selection, field, Opens::Shape), "");
                return;
            }
        };
        self.leave_selection(selection);
        self.services.cursor.field = field;
        panel.preview(&mut self.services);
        self.overlays.shape = Some(panel);
        self.modes.mode = Mode::Shape;
    }

    /// Why a panel or a pattern's buffer didn't open, said; or on an empty FX column, the
    /// commands listed, to choose the one it writes, which opens it.
    fn refused(&mut self, refusal: Refusal, (selection, field, opens): Waiting, text: &str) {
        match refusal {
            Refusal::Reason(reason) => self.services.status = reason.to_owned(),
            Refusal::Said => {}
            Refusal::Command(track) => {
                if opens != (Opens::Pattern { selected: false }) {
                    self.leave_selection(selection);
                }
                self.services.cursor.field = field;
                self.overlays.shape_command = Some((selection, field, opens));
                text.clone_into(&mut self.overlays.pattern_text);
                let (_, items) = self.services.fx_command_items(track, Pick::ShapeCommand);
                self.show_picker(Picker::new("FX commands", items).searching(""));
                self.services.status =
                    "Choose the command to write on the empty FX column".to_owned();
            }
        }
    }

    /// `!` (decision 474): a pattern's buffer on the cursor's column, on the rows selected or
    /// on every row of the phrase, opened on nothing typed yet. On the tracks of a `W`
    /// selection, a pattern on each, or across a group's tracks a kit line.
    pub(crate) fn open_pattern(&mut self) {
        if let Some((selection, selected)) = self.pattern_rows() {
            let field = self.services.cursor.field;
            self.open_pattern_on(selection, (field, None), selected, "");
        }
    }

    /// The rows `!` writes on and whether they were selected: in a Visual mode the selection's,
    /// on the cursor's column, and otherwise every row of the cursor's phrase. None, said, off
    /// the phrase grid or a phrase.
    pub(crate) fn pattern_rows(&mut self) -> Option<(Selection, bool)> {
        if self.modes.view != View::Phrase {
            self.services.status = "! enters a pattern on the phrase grid".to_owned();
            return None;
        }
        let scope = Scope::column(self.services.cursor.field.group());
        if self.modes.mode.is_visual() {
            let selection = Selection {
                scope,
                ..self.current_selection()
            };
            return Some((selection, true));
        }
        let Some(rows) = self
            .services
            .phrase_here()
            .map(|phrase| phrase.steps().len())
        else {
            self.services.status = "This track has no phrase here".to_owned();
            return None;
        };
        let track = self.services.cursor.track;
        let selection = Selection {
            range: StepRange::between(0, rows.saturating_sub(1)),
            scope,
            tracks: TrackSpan {
                first: track,
                last: track,
            },
        };
        Some((selection, false))
    }

    /// `.` after a pattern (decision 474): the same line, at the same `Step`, on the cursor's
    /// column, on the rows selected or every row, kept as it was, plain or live; or where it
    /// doesn't read on this column, why. An empty FX column takes the command it wrote.
    pub(crate) fn repeat_pattern(
        &mut self,
        text: &str,
        step: u8,
        live: bool,
        command: Option<FxCommand>,
    ) {
        let Some((selection, selected)) = self.pattern_rows() else {
            return;
        };
        let field = self.services.cursor.field;
        let command = command.filter(|_| {
            matches!(
                field,
                Field::FxCommand(_) | Field::FxValue(_) | Field::FxFine(_)
            )
        });
        self.services.live.pattern_step = step;
        if !self.open_pattern_on(selection, (field, command), selected, text) {
            return;
        }
        let Some(mut panel) = self.overlays.shape.take() else {
            return;
        };
        self.modes.mode = Mode::Normal;
        let reads = panel
            .pattern
            .as_ref()
            .is_some_and(|pattern| pattern.typed == text.trim());
        if !reads {
            let why = std::mem::take(&mut self.services.status);
            self.services
                .history
                .replace_current(Arc::clone(&panel.base));
            self.services.publish();
            self.services.status = format!("The last pattern doesn't read here: {why}");
            return;
        }
        panel.leave_tokens();
        self.services.status.clear();
        panel.keep(live, &mut self.services);
    }

    /// Open a pattern's buffer on these rows, the line `text` in it; whether it opened.
    pub(crate) fn open_pattern_on(
        &mut self,
        selection: Selection,
        (field, command): (Field, Option<FxCommand>),
        selected: bool,
        text: &str,
    ) -> bool {
        let opened = ShapePanel::pattern(
            selection,
            (field, command),
            &mut self.services,
            (self.modes.view, selected),
            text,
        );
        let panel = match opened {
            Ok(panel) => panel,
            Err(refusal) => {
                self.refused(
                    refusal,
                    (selection, field, Opens::Pattern { selected }),
                    text,
                );
                return false;
            }
        };
        if selected && self.modes.mode.is_visual() {
            self.leave_selection(selection);
        }
        self.services.cursor.field = field;
        self.overlays.shape = Some(panel);
        self.modes.mode = Mode::Shape;
        true
    }

    /// The mode a picker goes back to.
    pub(crate) fn mode_after_picker(&self) -> Mode {
        if self.overlays.shape.is_some() {
            Mode::Shape
        } else {
            Mode::Normal
        }
    }
}

/// A key of the shape panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKey {
    /// `Esc`: put back what was there.
    Cancel,
    /// `Enter`: keep it as plain values.
    Keep,
    /// `L`: keep it live.
    KeepLive,
    Down,
    Up,
    Increase,
    Decrease,
    IncreaseLarge,
    DecreaseLarge,
    /// `x`: the row back to what it started as.
    Reset,
    /// `~`: swap the ends, or reverse a list.
    Swap,
    /// `r`: roll a new seed.
    Roll,
    /// `K`: choose from a list.
    Choose,
    /// `i`: type the row's value; on a text in a grammar, its buffer typing at the end.
    Type,
    /// `a`: as `i`.
    Append,
    /// `cc` and `S`: as `i`, but on a text in a grammar, its buffer typing it all again.
    Retype,
    /// `Tab` and `Shift-Tab` play as they do in Normal.
    Play,
    PlayFromStart,
    /// `Space d`: describe the setting under the cursor, or stop, as on the pages.
    Describe,
    /// `Space ?`: the guide, at the fill's section.
    Guide,
    /// `l`: the row's buffer, where it's a text in a grammar: `Weights`' digits, a `List`'s
    /// values, a `Tonnetz`'s moves, a mask's bits or a rhythm's `Text`, on its first token.
    Right,
    /// `[s`: a `Mini`'s `Step` halved, in the buffer of a pattern entered with `!` too.
    HalveStep,
    /// `]s`: a `Mini`'s `Step` doubled.
    DoubleStep,
}

impl Overlay for ShapePanel {
    const MODE: Mode = Mode::Shape;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        (
            "Esc",
            Cmd::InShape(ShapeKey::Cancel),
            "Close, putting back what was there",
        ),
        (
            "Enter",
            Cmd::InShape(ShapeKey::Keep),
            "Keep it, as plain values",
        ),
        (
            "L",
            Cmd::InShape(ShapeKey::KeepLive),
            "Keep it live, as a generator",
        ),
        ("j", Cmd::InShape(ShapeKey::Down), "The next setting"),
        ("Down", Cmd::InShape(ShapeKey::Down), "The next setting"),
        ("k", Cmd::InShape(ShapeKey::Up), "The setting before"),
        ("Up", Cmd::InShape(ShapeKey::Up), "The setting before"),
        (
            "=",
            Cmd::InShape(ShapeKey::Increase),
            "Increase the setting",
        ),
        (
            "-",
            Cmd::InShape(ShapeKey::Decrease),
            "Decrease the setting",
        ),
        (
            "+",
            Cmd::InShape(ShapeKey::IncreaseLarge),
            "Increase the setting a large step",
        ),
        (
            "_",
            Cmd::InShape(ShapeKey::DecreaseLarge),
            "Decrease the setting a large step",
        ),
        (
            "x",
            Cmd::InShape(ShapeKey::Reset),
            "The setting back to what it started as",
        ),
        (
            "~",
            Cmd::InShape(ShapeKey::Swap),
            "Swap the ends, or reverse a list",
        ),
        ("r", Cmd::InShape(ShapeKey::Roll), "Roll it again"),
        ("K", Cmd::InShape(ShapeKey::Choose), "Choose from a list"),
        (
            "i",
            Cmd::InShape(ShapeKey::Type),
            "Type the setting; on a text, its buffer typing at the end",
        ),
        (
            "a",
            Cmd::InShape(ShapeKey::Append),
            "Type the setting; on a text, its buffer typing at the end",
        ),
        (
            "S",
            Cmd::InShape(ShapeKey::Retype),
            "Type the setting; on a text, its buffer typing it all again",
        ),
        (
            "c c",
            Cmd::InShape(ShapeKey::Retype),
            "Type the setting; on a text, its buffer typing it all again",
        ),
        (
            "Tab",
            Cmd::InShape(ShapeKey::Play),
            "Play, or pause in place",
        ),
        (
            "S-Tab",
            Cmd::InShape(ShapeKey::PlayFromStart),
            "Play from the start",
        ),
        ("Space d", Cmd::InShape(ShapeKey::Describe), DESCRIBE),
        (
            "Space ?",
            Cmd::InShape(ShapeKey::Guide),
            "The guide, at the fill's section",
        ),
        (
            "l",
            Cmd::InShape(ShapeKey::Right),
            "A text's buffer, on its first token: weights, values, moves, bits or a rhythm",
        ),
        (
            "[ s",
            Cmd::InShape(ShapeKey::HalveStep),
            "Halve a Mini's Step, to 1, 2, 4, 8 or 16 rows",
        ),
        (
            "] s",
            Cmd::InShape(ShapeKey::DoubleStep),
            "Double a Mini's Step, to 1, 2, 4, 8 or 16 rows",
        ),
    ];

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let services = &mut *ctx.services;
        let key = match cmd {
            Cmd::InShape(key) => key,
            Cmd::InTokens(key) => return self.token_key(key, services),
            _ => return Outcome::Passed,
        };
        // The buffer is a dialog: of the panel's keys, only playing, what describes the token
        // and turning a `Mini`'s `Step` reach past it, and in a pattern's, `L` keeping it live.
        let pattern = self.pattern.is_some();
        if self.tokens.is_some()
            && !matches!(
                key,
                ShapeKey::Play
                    | ShapeKey::PlayFromStart
                    | ShapeKey::Describe
                    | ShapeKey::Guide
                    | ShapeKey::HalveStep
                    | ShapeKey::DoubleStep
            )
            && !(pattern && key == ShapeKey::KeepLive)
        {
            return Outcome::Done;
        }
        match key {
            ShapeKey::Play => return Outcome::Do(Cmd::PauseOrResume),
            ShapeKey::PlayFromStart => return Outcome::Do(Cmd::PlayFromStart),
            ShapeKey::Describe => return Outcome::Do(Cmd::ToggleDescription),
            ShapeKey::Guide => return Outcome::Do(Cmd::Guides),
            ShapeKey::Cancel => {
                services.history.replace_current(Arc::clone(&self.base));
                services.publish();
                return Outcome::Close;
            }
            ShapeKey::Keep => return self.keep(false, services),
            ShapeKey::KeepLive => return self.keep(true, services),
            ShapeKey::Down => self.move_row(1),
            ShapeKey::Up => self.move_row(-1),
            ShapeKey::Increase => self.adjust(1, false, services),
            ShapeKey::Decrease => self.adjust(-1, false, services),
            ShapeKey::IncreaseLarge => self.adjust(1, true, services),
            ShapeKey::DecreaseLarge => self.adjust(-1, true, services),
            ShapeKey::Reset => self.reset_row(services),
            ShapeKey::Swap => self.change(
                |panel| {
                    let shape = &mut panel.shape;
                    if shape.kind == FillKind::List {
                        panel.list.reverse();
                    } else {
                        std::mem::swap(&mut shape.from, &mut shape.to);
                    }
                },
                services,
            ),
            ShapeKey::Roll => {
                let mut shape = self.shape;
                services.roll(&mut shape);
                self.change(|panel| panel.shape = shape, services);
            }
            ShapeKey::Choose => return self.choices(services),
            ShapeKey::Type | ShapeKey::Append => {
                if !self.open_buffer(Opening::Append, services) {
                    return self.start_typing(services);
                }
            }
            ShapeKey::Retype => {
                if !self.open_buffer(Opening::Replace, services) {
                    return self.start_typing(services);
                }
            }
            ShapeKey::Right => {
                self.open_buffer(Opening::Tokens, services);
            }
            ShapeKey::HalveStep => self.turn_step(false, services),
            ShapeKey::DoubleStep => self.turn_step(true, services),
        }
        Outcome::Done
    }

    /// `Tab` and `Shift-Tab` play whatever's held with them. Cmd with `=`'s and its neighbours'
    /// Cmd keys is that key, but types nothing; Cmd with any other key does nothing; and nothing
    /// else held is read.
    fn reads(&self, key: Key) -> Key {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => Key::new(key.code),
            _ if key.command && self.typing.is_none() => adjusting_key(key).unwrap_or(key),
            _ if key.command => key,
            code => Key::new(code),
        }
    }

    fn takes_text(&self) -> bool {
        self.typing.is_some() || self.tokens.as_ref().is_some_and(Tokens::completes)
    }

    fn inner_mode(&self) -> Option<Mode> {
        self.tokens.as_ref().map(|_| Mode::Tokens)
    }

    fn types_now(&self, key: Key) -> bool {
        match (key.code, &self.tokens) {
            (KeyCode::Char(c), Some(tokens)) => !key.command && tokens.types(c),
            _ => false,
        }
    }

    /// What's typed into a row; `Tab` and `Shift-Tab` play while it is, but complete while a field
    /// of tokens is typed or its list is open, and any other key does nothing.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        let completes = self.tokens.as_ref().is_some_and(Tokens::completes);
        match key.code {
            KeyCode::Tab | KeyCode::BackTab if completes => {
                return self.token_text(key.code, ctx.services);
            }
            KeyCode::Tab => return Outcome::Do(Cmd::PauseOrResume),
            KeyCode::BackTab => return Outcome::Do(Cmd::PlayFromStart),
            _ if key.command => {}
            code if self.typing.is_some() => self.typed(code, ctx.services),
            code if self.tokens.is_some() => return self.token_text(code, ctx.services),
            _ => {}
        }
        Outcome::Done
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        draw::draw(frame, area, ctx, self);
    }
}

impl ShapePanel {
    /// Show the panel's result, without adding an undo step.
    pub(crate) fn preview(&self, services: &mut Services) {
        if let Ok(next) = edit(&self.base, &self.action()) {
            services.history.replace_current(next);
            services.publish();
        }
    }

    /// Keep what's showing, as one undo step: as plain values, or live. A live generator reopened
    /// and kept as plain values is applied. It closes once it's kept, which `App` remembers for
    /// `.`.
    fn keep(&mut self, live: bool, services: &mut Services) -> Outcome {
        if let Some(why) = self.too_few_notes() {
            services.status = why;
            return Outcome::Done;
        }
        let part = self.parts[0];
        let action = match (self.live, live) {
            (None, false) | (Some(_), true) => None,
            (None, true) => Some(self.live_action()),
            (Some(generator), false) => Some(Action::Batch(vec![
                self.action(),
                Action::ApplyGenerator {
                    track: part.track,
                    phrase: part.phrase,
                    generator,
                },
            ])),
        };
        let shown = Arc::clone(services.song());
        let result = match action.map(|action| edit(&self.base, &action)) {
            None => shown,
            Some(Ok(result)) => result,
            Some(Err(error)) => {
                services.status = error.to_string();
                shown
            }
        };
        services.history.replace_current(Arc::clone(&self.base));
        if self.live.is_none() {
            services.say_applied(&result);
        }
        if services.history.commit(result) {
            services.dirty = true;
        }
        services.publish();
        self.kept = true;
        self.kept_live = live;
        if let Some(pattern) = &self.pattern {
            pattern.typed.clone_into(&mut services.live.last_pattern);
            services.live.pattern_step = self.shape.mini.step;
        }
        let mut name = column_label(part.column);
        if self.shape.kind == FillKind::Follow {
            name = format!("{name}←{}", self.shape.follow.source);
        }
        match (self.live, live) {
            (None, true) => {
                let lane = self.on_lane(&part);
                services.status = match self.parts.len() {
                    1 if lane => format!(
                        "{name} is live on its lane: gl opens it again, Space g a applies it"
                    ),
                    1 => format!("{name} is live: gl opens it again, Space g a applies it"),
                    tracks => format!(
                        "{name} is live on {tracks} tracks, each on its own: gl opens one again"
                    ),
                };
            }
            (Some(_), true) => services.status = format!("{name} stays live"),
            (Some(_), false) => {
                services.status = format!("{name} is applied, as plain values");
            }
            (None, false) => {}
        }
        if let Some(cut) = self.cut_said() {
            services.status = if services.status.is_empty() {
                cut
            } else {
                format!("{}; {cut}", services.status)
            };
        }
        Outcome::Close
    }

    /// Change the shape, settle what follows from it, and show the result. A `List` with no
    /// values yet starts as the two ends, so it sounds as `Alternate` did until it's typed.
    /// A named rhythm made `Own` starts from its text, its part a token a pulse (decision 461).
    pub(crate) fn change(&mut self, change: impl FnOnce(&mut Self), services: &mut Services) {
        let (before, text) = (self.shape.place, self.rhythm.clone());
        let was = self.shape;
        change(self);
        settle(&mut self.shape, &self.sources, self.default_source);
        if self.shape.kind == FillKind::List && self.list.is_empty() {
            self.list = self.kept_list().into();
        }
        // A `Mini` across a group's tracks is a kit line, and only a `Mini` (decision 464), and
        // only while each of its tracks has a name, its token (decision 472).
        self.shape.kit = self.kit.filter(|_| self.shape.kind == FillKind::Mini);
        if self.shape.kit.is_some() && was.kit.is_none() {
            let unnamed: Vec<usize> = self
                .parts
                .iter()
                .map(|part| part.track)
                .filter(|&track| self.base.tracks[track].name.is_empty())
                .collect();
            if !unnamed.is_empty() {
                self.shape = was;
                services.status = unnamed_said(&unnamed);
                return;
            }
        }
        // A `Mini` with no line yet starts from what's in its rows (decision 462), a kit line
        // from which of its tracks has a note on each.
        if self.shape.kind == FillKind::Mini
            && self.mini.is_empty()
            && self.pattern.is_none()
            && let Some(part) = self.parts.first()
        {
            self.mini = if self.shape.kit.is_some() {
                let notes: Vec<Vec<bool>> = self
                    .parts
                    .iter()
                    .map(|part| {
                        let steps = self.base_steps(part);
                        (part.range.first..=part.range.last)
                            .map(|row| {
                                steps
                                    .get(row)
                                    .is_some_and(|step| matches!(step.note, Note::On(_)))
                            })
                            .collect()
                    })
                    .collect();
                kit_start(&self.names, &notes)
            } else {
                mini_start(self.base_steps(part), part.range, part.column)
            };
        }
        if let Some(from) = from_named(before, self.shape.place)
            && self.rhythm == text
        {
            self.rhythm = from;
        }
        self.own().settle(&mut self.shape);
        self.preview(services);
        if let Some(said) = self.dropped() {
            services.status = said;
        }
    }

    /// How many of a `Mini`'s hits share a row on its first part and are dropped, as the status
    /// line says it, where any are.
    fn dropped(&self) -> Option<String> {
        let part = self.parts.first()?;
        dropped_said(&self.shape_of(0), &self.own_of(0), part.range.len())
    }

    /// `[s` and `]s`: a `Mini`'s `Step` halved or doubled, which a pattern entered with `!`
    /// remembers for the next.
    fn turn_step(&mut self, up: bool, services: &mut Services) {
        if self.shape.kind != FillKind::Mini {
            services.status = "[s and ]s halve and double a Mini's Step".to_owned();
            return;
        }
        let step = stepped(self.shape.mini.step, up);
        self.change(|panel| panel.shape.mini.step = step, services);
        if self.pattern.is_some() {
            services.live.pattern_step = step;
        }
        self.say_token(services);
        if self.tokens.is_none() {
            services.status = format!("Step {step} {}", rows_word(step));
        }
    }

    /// How many rows of a pattern's line, one time through, are past the phrase's end and cut
    /// there (decision 474): none where it fits, or off a pattern.
    pub fn rows_cut(&self) -> usize {
        let Some(pattern) = &self.pattern else {
            return 0;
        };
        let lap = line_steps(self.own().beats()) as usize * usize::from(self.shape.mini.step);
        lap.saturating_sub(pattern.rows)
    }

    /// What the status line says of a pattern cut at the phrase's end, where it is.
    fn cut_said(&self) -> Option<String> {
        let cut = self.rows_cut();
        let rows = self.pattern.as_ref()?.rows;
        match cut {
            0 => None,
            1 => Some(format!("1 row didn't fit in the phrase's {rows}")),
            cut => Some(format!("{cut} rows didn't fit in the phrase's {rows}")),
        }
    }

    fn move_row(&mut self, by: isize) {
        let rows = self.rows();
        let at = rows.iter().position(|row| *row == self.row).unwrap_or(0);
        let to = at.saturating_add_signed(by).min(rows.len() - 1);
        self.row = rows[to];
    }

    fn adjust(&mut self, sign: i16, large: bool, services: &mut Services) {
        if self.row == GenParam::Weights {
            services.status = "l opens the weights, a digit a note, to turn them there".to_owned();
            return;
        }
        let mut shape = self.shape;
        if let Err(reason) = self.row.step(&mut shape, &self.context(), sign, large) {
            services.status = reason.to_owned();
            return;
        }
        self.change(|panel| panel.shape = shape, services);
    }

    /// Where `From` and `To` go back to: the ends of what was there on the first track, or on a
    /// column generated on, the ends it starts with.
    fn ends(&self, services: &Services) -> (u8, u8) {
        let first = self.parts[0];
        if first.generate {
            services.generated_ends(first.column)
        } else {
            default_ends(self.base_steps(&first), first.range, first.column)
        }
    }

    /// Put the current row back to what it started as.
    fn reset_row(&mut self, services: &mut Services) {
        let starts = Starts {
            ends: self.ends(services),
            source: self.default_source,
            part: Some(self.parts[0].drum),
        };
        self.change(
            |panel| {
                let mut shape = panel.shape;
                panel.row.reset(&mut shape, &panel.context(), starts);
                panel.shape = shape;
                // A list goes back to the two ends, as it starts.
                if panel.row == GenParam::Values {
                    panel.list = vec![shape.from, shape.to];
                }
                let rows = panel.rows();
                if !rows.contains(&panel.row) {
                    panel.row = if rows.contains(&GenParam::Where) {
                        GenParam::Where
                    } else {
                        GenParam::Fill
                    };
                }
            },
            services,
        );
    }

    /// `K`: the row's choices, as a picker over the panel, or why it has none.
    fn choices(&self, services: &mut Services) -> Outcome {
        let title = match self.row {
            GenParam::Source => "Follow",
            row => row.name(),
        };
        let follows = |source: Source| {
            let rows = self
                .base
                .source_phrase(source)
                .map_or(0, |phrase| phrase.loop_steps().len());
            let name = services.track_name(source.track);
            format!("Track {name}, {rows} rows")
        };
        let picker = choice_picker(
            title,
            self.row,
            &self.shape,
            &self.context(),
            Pick::ShapeChoice,
            Some(&follows),
        );
        match picker {
            Some(picker) => Outcome::Open(Box::new(picker)),
            None => {
                services.status =
                    "K chooses from a list on Fill, Shape, Rate, Where, Source and Part".to_owned();
                Outcome::Done
            }
        }
    }

    /// `i`: type the row, or choose from its list, or switch it.
    fn start_typing(&mut self, services: &mut Services) -> Outcome {
        match self.row.typing(self.column) {
            Typing::Listed => return self.choices(services),
            Typing::Toggled => self.adjust(1, false, services),
            _ => self.typing = Some(TypedValue::default()),
        }
        Outcome::Done
    }

    fn typed(&mut self, code: KeyCode, services: &mut Services) {
        let typing = self.row.typing(self.column);
        let Some(text) = &mut self.typing else {
            return;
        };
        // `Enter` keeps a single hex digit too, here.
        let typed = match code {
            KeyCode::Enter => Typed::Commit(text.take()),
            _ => text.key(code, typing.into()),
        };
        match typed {
            Typed::Editing => {}
            Typed::Cancel => self.typing = None,
            Typed::Commit(text) => {
                self.typing = None;
                self.commit_typing(&text, services);
            }
        }
    }

    fn commit_typing(&mut self, text: &str, services: &mut Services) {
        let mut refused = None;
        let mut said = None;
        self.change(
            |panel| {
                if panel.row == GenParam::Values {
                    match GenParam::typed_list(panel.column, text) {
                        Ok(list) => panel.list = list,
                        Err(why) => refused = Some(why),
                    }
                    return;
                }
                if panel.row == GenParam::OwnText {
                    match typed_rhythm(&mut panel.shape, text) {
                        Ok(()) => panel.rhythm = text.trim().to_owned(),
                        Err(why) => refused = Some(why),
                    }
                    return;
                }
                if panel.row == GenParam::MiniText {
                    // In a pattern, each `x` is written out as the last note typed.
                    let line = match &panel.pattern {
                        Some(pattern) if panel.shape.kit.is_none() => {
                            with_insert(text, &pattern.note)
                        }
                        _ => text.to_owned(),
                    };
                    match typed_mini(&line, panel.column, panel.parts.len(), &panel.names) {
                        Ok(()) => {
                            panel.mini = line.trim().to_owned();
                            if let Some(pattern) = &mut panel.pattern {
                                text.trim().clone_into(&mut pattern.typed);
                            }
                        }
                        Err(why) => said = Some(why),
                    }
                    return;
                }
                let mut shape = panel.shape;
                refused = panel.row.typed(&mut shape, &panel.context(), text).err();
                panel.shape = shape;
            },
            services,
        );
        if let Some(reason) = refused {
            services.status = reason.to_owned();
        }
        if let Some(reason) = said {
            services.status = reason;
        }
    }
}

impl ShapePanel {
    /// The notes `Weights`' digits are for, from the root, as the grid spells them: the scale's
    /// degrees while it's on, and the twelve semitones while it's off. None off the Note column.
    pub fn weight_notes(&self) -> Vec<&'static str> {
        let ShapeColumn::Note { scale, .. } = self.column else {
            return Vec::new();
        };
        (0..degree_count(scale))
            .map(|degree| degree_name(scale, degree))
            .collect()
    }

    /// The digit the cursor is on in `Weights`, if it's in them: its place, its note and its
    /// weight.
    pub fn weight_here(&self) -> Option<(usize, &'static str, u8)> {
        let at = self.weight?;
        let note = *self.weight_notes().get(at)?;
        Some((at, note, self.shape.random.weights[at]))
    }

    /// A row's text as a field of tokens, where it's in a grammar.
    pub fn field(&self, row: GenParam) -> Option<TokenField> {
        let grammar = row.grammar(self.column)?.grammar();
        let text = self.setting_text(row);
        let field = TokenField::new(grammar, &text, self.column);
        // A kit line's hits are its tracks' names, and in a pattern `x` is the last note typed.
        Some(match row {
            GenParam::MiniText if self.shape.kit.is_some() => field.in_kit(&self.names),
            GenParam::MiniText if self.pattern.is_some() => field.with_insert(),
            _ => field,
        })
    }

    /// A row's text as its field shows it: in a pattern, the line as typed, `x` and all.
    fn setting_text(&self, row: GenParam) -> String {
        match &self.pattern {
            Some(pattern) if row == GenParam::MiniText => pattern.typed.clone(),
            _ => row
                .grammar(self.column)
                .map_or_else(String::new, |grammar| {
                    (grammar.grammar().from_setting)(&row.shown(&self.shape, &self.context()))
                }),
        }
    }

    /// The part shaping a track's phrase, by its place, or the first where none does.
    fn part_on(&self, track: usize, phrase: Option<u32>) -> usize {
        self.parts
            .iter()
            .position(|part| part.track == track && Some(part.phrase) == phrase)
            .unwrap_or(0)
    }

    /// Which token wrote each row of the part shaping a track's phrase, or the first part, as
    /// it's shaped now, on a pass of it from 0 (decision 460).
    pub fn table(&self, track: usize, phrase: Option<u32>, pass: i64) -> Option<Table> {
        let nth = self.part_on(track, phrase);
        let part = self.parts.get(nth)?;
        Some(Table::render(
            self.base_steps(part),
            part.range,
            part.column,
            &self.shape_of(nth).on_pass(pass),
            self.own_of(nth).texts(&self.list),
            self.source_steps(),
        ))
    }

    /// The rows the token under the cursor writes on a track's phrase, while the cursor's in a
    /// field of tokens and its text reads, with the column they're on: for the grid to light.
    pub fn token_rows(&self, track: usize, phrase: u32) -> Option<(ShapeColumn, Vec<usize>)> {
        let (tokens, at) = (self.tokens.as_ref()?, self.weight?);
        let part = self.parts.get(self.part_on(track, Some(phrase)))?;
        if (part.track, part.phrase) != (track, phrase) {
            return None;
        }
        let table = self.table(track, Some(phrase), 0)?;
        Some((part.column, table.rows(&tokens.field, at)))
    }

    /// The token of a field that wrote the row playing on the cursor's phrase, while it plays:
    /// from the table of the pass it's on, for a live generator that varies.
    pub fn playing_token(&self, ctx: &Ctx, field: &TokenField) -> Option<usize> {
        let row = ctx.playhead_place()?.0;
        let (track, phrase) = (ctx.cursor().track, ctx.phrase_id());
        let pass = match (self.live, phrase) {
            (Some(generator), Some(phrase)) => ctx.heard_pass(track, phrase, generator),
            _ => 0,
        };
        self.table(track, phrase, pass)?.token_of(field, row)
    }

    /// The buffer of a row in a grammar, opened as `how` says (decision 463), the token it's on
    /// named or why it isn't typing said; false on a row that isn't one.
    fn open_buffer(&mut self, how: Opening, services: &mut Services) -> bool {
        let Some(field) = self.field(self.row) else {
            return false;
        };
        let (tokens, at, why) = Tokens::open(field, how);
        self.tokens = Some(tokens);
        self.weight = Some(at);
        // `cc` and `S` change the text as they open it.
        if how == Opening::Replace {
            self.write_tokens(services);
        }
        self.say_token(services);
        if let Some(why) = why {
            services.status = why;
        }
        true
    }

    fn leave_tokens(&mut self) {
        self.tokens = None;
        self.weight = None;
    }

    /// One of the token editor's keys.
    fn token_key(&mut self, key: TokenKey, services: &mut Services) -> Outcome {
        let tracks = self.track_names(services);
        let reached = self.reached();
        let around = self.around(&tracks, &reached);
        let (Some(tokens), Some(at)) = (self.tokens.as_mut(), self.weight.as_mut()) else {
            return Outcome::Passed;
        };
        let did = match key {
            TokenKey::Offer => tokens.offer(*at, &around),
            key => tokens.key(key, at),
        };
        self.tokens_did(did, services)
    }

    /// A key that isn't the table's, in the tokens: a token typed in place, a count, what's being
    /// typed, or the list `K` or `Tab` opened.
    fn token_text(&mut self, code: KeyCode, services: &mut Services) -> Outcome {
        // `!` typed first in a pattern's buffer, so `!!`, opens it on the last pattern.
        if code == KeyCode::Char('!')
            && self.pattern.is_some()
            && self
                .tokens
                .as_ref()
                .is_some_and(|tokens| tokens.field.text().trim().is_empty())
        {
            let last = services.live.last_pattern.clone();
            if last.is_empty() {
                services.status = "No pattern yet for !! to open".to_owned();
                return Outcome::Done;
            }
            self.commit_typing(&last, services);
            self.open_buffer(Opening::Append, services);
            return Outcome::Done;
        }
        let tracks = self.track_names(services);
        let reached = self.reached();
        let around = self.around(&tracks, &reached);
        let (Some(tokens), Some(at)) = (self.tokens.as_mut(), self.weight.as_mut()) else {
            return Outcome::Done;
        };
        let did = tokens.text(code, at, &around);
        self.tokens_did(did, services)
    }

    /// The names of the tracks the panel shapes, for a kit line's tokens: on one, the names of
    /// all the tracks it spans.
    fn track_names(&self, services: &Services) -> Vec<String> {
        if self.shape.kit.is_some() {
            return self.names.clone();
        }
        self.parts
            .iter()
            .map(|part| services.track_name(part.track))
            .collect()
    }

    /// What's around a field, for what `K` and `Tab` offer in it: the column, with its scale, and
    /// From and To.
    fn around<'a>(&self, tracks: &'a [String], reached: &'a [String]) -> Around<'a> {
        Around {
            column: self.column,
            from: self.shape.from,
            to: self.shape.to,
            tracks,
            reached,
        }
    }

    /// On a `Tonnetz`'s moves, the chord each move would reach in the cursor's place.
    fn reached(&self) -> Vec<String> {
        let at = self.weight.unwrap_or(0);
        reached(self.row, &self.shape_of(0), &self.context(), at)
    }

    /// After a key in the tokens: the text written into the setting where it reads, and heard,
    /// while text that doesn't leaves the last that did playing.
    fn tokens_did(&mut self, did: Did, services: &mut Services) -> Outcome {
        match did {
            // What the setting said of what's typed, if it refused it, stays on the status line.
            Did::Kept => {
                services.status.clear();
                self.write_tokens(services);
                self.leave_tokens();
                // A pattern's `Enter` writes it, as plain rows.
                if self.pattern.is_some() {
                    return self.keep(false, services);
                }
                return Outcome::Done;
            }
            // A pattern's `Esc` puts everything back.
            Did::Out if self.pattern.is_some() => {
                services.history.replace_current(Arc::clone(&self.base));
                services.publish();
                services.status.clear();
                return Outcome::Close;
            }
            Did::Out => {
                self.leave_tokens();
                services.status.clear();
                return Outcome::Done;
            }
            Did::Refused(why) => {
                services.status = why;
                return Outcome::Done;
            }
            Did::Moved => {}
            Did::Changed => self.write_tokens(services),
        }
        self.say_token(services);
        Outcome::Done
    }

    /// The field's text into the setting where it reads, and back as the setting shows it once
    /// it's no longer being typed.
    fn write_tokens(&mut self, services: &mut Services) {
        let Some(tokens) = &self.tokens else {
            return;
        };
        if tokens.field.bad().is_some() {
            return;
        }
        let grammar = tokens.field.grammar();
        let text = (grammar.to_setting)(tokens.field.text());
        self.commit_typing(&text, services);
        let shown = self.setting_text(self.row);
        if let Some(tokens) = self.tokens.as_mut()
            && tokens.field.inserting().is_none()
        {
            tokens.field.set(&shown);
        }
    }

    /// The status line names the token under the cursor, `Weight of G: F`, or says why the text
    /// doesn't read.
    fn say_token(&self, services: &mut Services) {
        let (Some(tokens), Some(at)) = (&self.tokens, self.weight) else {
            return;
        };
        let field = &tokens.field;
        services.status = match (field.bad(), field.inserting()) {
            // At the start of a word, what can go there, as Mockup 2 of the study has it, while
            // the text it's typed into is still short of it.
            (_, Some(_)) if field.typed_word() == Some("") => {
                let after = match field.spot(at).0 {
                    Spot::After(Some((_, before))) if field.fixed().is_none() => {
                        format!("Typing after {before}: ")
                    }
                    _ => String::new(),
                };
                format!("{after}K lists what can go here, Tab completes a word")
            }
            (Some(bad), _) => format!(
                "{}; the column plays the last text that read. Esc puts it back",
                bad.why
            ),
            (None, Some(_)) => String::new(),
            (None, None) => {
                let after = said_after(self.row, field, at, &self.shape_of(0), &self.context());
                format!("{}{after}", token_named(field, at).0)
            }
        };
        if field.bad().is_none()
            && let Some(cut) = self.cut_said()
        {
            services.status = if services.status.is_empty() {
                cut
            } else {
                format!("{}; {cut}", services.status)
            };
        }
    }
}

/// The text a named rhythm made `Own` starts from, its part a token a pulse, where a change made
/// one `Own` (decision 461).
pub(crate) fn from_named(before: Where, after: Where) -> Option<String> {
    match (before, after) {
        (
            Where::Rhythm {
                rhythm,
                part,
                child,
            },
            Where::Own(_),
        ) => Some(rhythm_preset(rhythm, part, child)),
        _ => None,
    }
}

/// An `Own` rhythm's text typed: a shape on a named rhythm is made `Own`, and text that doesn't
/// read is refused, saying why.
pub(crate) fn typed_rhythm(shape: &mut Shape, text: &str) -> Result<(), &'static str> {
    if compile_rhythm(text.trim()).is_err() {
        return Err("That rhythm doesn't read: x a hit, . a rest, [ ] a group, < > one each lap");
    }
    if let Where::Rhythm { .. } = shape.place {
        shape.place = Where::Own(Own::default());
    }
    Ok(())
}

/// A `Mini`'s line typed for so many tracks: refused, saying why, where it doesn't read, and
/// where it stacks layers on one track, which holds one (decision 462). On a kit line, `names`,
/// its hits are the names of its tracks, and its stack's layers are theirs (decision 464).
pub(crate) fn typed_mini(
    text: &str,
    column: ShapeColumn,
    tracks: usize,
    names: &[String],
) -> Result<(), String> {
    let text = text.trim();
    if !names.is_empty() {
        let grammar = GrammarId::Kit.grammar();
        return (grammar.read)(text, column, names).map_err(|bad| bad.why);
    }
    let grammar = GrammarId::Mini.grammar();
    if let Err(bad) = (grammar.read)(text, column, &[]) {
        return Err(bad.why);
    }
    if tracks < 2 && stacks(text) {
        return Err(STACKED.to_owned());
    }
    Ok(())
}

/// What the status line adds of a `Mini`'s line on a part: how many of its hits share a row and
/// are dropped, and the `Step` that keeps them, as the study's §5.2 says.
pub(crate) fn dropped_said(shape: &Shape, own: &OwnRhythm, rows: usize) -> Option<String> {
    if shape.kind != FillKind::Mini {
        return None;
    }
    let count = dropped(own.beats(), shape.mini, shape.seed, rows);
    if count == 0 {
        return None;
    }
    let hits = if count == 1 {
        "hit shares"
    } else {
        "hits share"
    };
    let keeps = match keeping_step(own.beats(), shape.mini) {
        Some(1) => "; Step 1 row keeps them".to_owned(),
        Some(step) => format!("; Step {step} rows keeps them"),
        None => String::new(),
    };
    Some(format!("{count} {hits} a row and dropped{keeps}"))
}

/// The token at a place as the status line says it, `Weight of G: F`, and what it means, for
/// `Space d`.
pub(crate) fn token_named(field: &TokenField, at: usize) -> (String, String) {
    let grammar = field.grammar();
    let (kind, text) = match (field.tokens().get(at), field.token(at)) {
        (Some(token), Some(text)) => (token.kind, text),
        _ => (Kind::Unread, ""),
    };
    let about = (grammar.about)(kind, at, field.column());
    let meaning = (grammar.meaning)(kind).replacen("{}", &about.subject, 1);
    // A bit is said as what it is, rather than as its mark.
    let text = match (kind, text) {
        (Kind::Bit, "." | "0") => "off",
        (Kind::Bit, _) => "on",
        _ => text,
    };
    (format!("{}: {text}", about.name), meaning)
}

impl App {
    /// Give the shape panel a key. A shape kept is remembered, for `.` to shape again, and one
    /// kept live with Gen's panel beside the grid lands there, in its device.
    pub(crate) fn shape_key(&mut self, key: Key, now: Instant) {
        let Some(panel) = self.overlays.shape.as_mut() else {
            return;
        };
        let outcome = crate::overlays::give(
            panel,
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
        let kept = matches!(outcome, Outcome::Close)
            .then(|| self.overlays.shape.take())
            .flatten()
            .filter(|panel| panel.kept);
        if let Some(panel) = &kept {
            self.remember_shape(panel);
        }
        self.settled(outcome, false);
        if let Some(panel) = &kept {
            self.land_kept(panel);
        }
    }
}
