//! The shape panel: fill a selected column with a ramp, random values, a walk, a wave, two values
//! in turn or a short list of them, or rework the values there, heard while it's adjusted. On the
//! Note column it can also follow another track's phrase.

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::generator::{Generator, Span, applied, reads, unfollowed};
use vt_model::lfo::Shape as Wave;
use vt_model::scale::Scale;
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{
    Acid, Bits, Context, DEFAULT_RATE, FillKind, Follow, GenParam, MAX_VALUES, Pink, Shape,
    ShapeColumn, Source, Starts, Values, Walk, Where, default_ends, default_step, is_empty,
    open_rows, track_seed,
};
use vt_model::song::{Song, Track};
use vt_model::step::{FxCommand, MAX_NOTE, Note, Step};

use crate::app::{App, Selection, TrackSpan};
use crate::input::bindings::{Cmd, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker};

/// The longest rate that can be typed, like `64 bars`.
const RATE_TEXT: usize = 8;

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
    /// The setting the cursor is on.
    pub row: GenParam,
    /// What's being typed into the current row.
    pub typing: Option<String>,
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
}

impl ShapePanel {
    /// The rows on show: the settings its shape has, `Fill` first as it decides the rest
    /// ([`GenParam::exists_for`]). `Acid`'s `Density` shows where any track generates.
    pub fn rows(&self) -> Vec<GenParam> {
        GenParam::of(&self.shape, self.parts.iter().any(|part| part.generate))
    }

    /// What its settings act on: its column, the rows it could change, whether every part is
    /// generated on, and what a `Follow` can read and reads now.
    pub fn context(&self) -> Context<'_> {
        Context {
            column: self.column,
            open: self.open_rows(),
            generates: self.generates(),
            sources: &self.sources,
            source: self.source_steps(),
        }
    }

    /// The loop of the phrase a `Follow` reads, as the panel opened on it.
    pub fn source_steps(&self) -> Option<&[Step]> {
        if self.shape.kind != FillKind::Follow {
            return None;
        }
        self.base
            .source_phrase(self.shape.follow.source)
            .map(|phrase| phrase.loop_steps())
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

/// A live generator as the Gen page edits it: a copy of it, with its phrase's steps as they'd be
/// without it, the phrases a `Follow` can read and the loop it reads, and what `x` puts back.
#[derive(Debug, Clone)]
pub struct LiveGen {
    pub generator: Generator,
    pub under: Vec<Step>,
    sources: Vec<Source>,
    source: Option<Vec<Step>>,
    pub starts: Starts,
}

impl LiveGen {
    /// What its settings act on. It always covers rows, as [`App::live_generator`] gives one
    /// only then.
    pub fn context(&self) -> Context<'_> {
        let rows = self.generator.rows().unwrap_or(StepRange::between(0, 0));
        let source = (self.generator.shape.kind == FillKind::Follow)
            .then_some(self.source.as_deref())
            .flatten();
        self.generator
            .context(&self.under, &self.sources, source)
            .unwrap_or(Context {
                column: self.generator.column,
                open: open_rows(&self.under, rows, self.generator.column),
                generates: false,
                sources: &self.sources,
                source,
            })
    }

    /// A shape it's given, made whole as the panel makes it: a list starts as the two ends, and
    /// a `Follow` reads a phrase it can.
    pub fn settle(&self, shape: &mut Shape) {
        settle(shape, &self.sources, self.starts.source);
    }
}

/// A shape made whole after a change: a list starts as the two ends, so it sounds as `Alternate`
/// did until it's typed, and a `Follow` follows a phrase it can, the one it starts on if what it
/// holds isn't one.
fn settle(shape: &mut Shape, sources: &[Source], start: Option<Source>) {
    if shape.kind == FillKind::List && shape.values.is_empty() {
        shape.values = Values::new(&[shape.from, shape.to]);
    }
    if shape.kind == FillKind::Follow
        && !sources.contains(&shape.follow.source)
        && let Some(source) = start.or(sources.first().copied())
    {
        shape.follow.source = source;
    }
}

/// Read a song's live generator, one of a track's phrase's by its place in its list, with what
/// its settings act on, from the song alone, as the Roto and the page's text do: `None` if
/// there's no such generator. A `Follow`'s sources aren't listed here, so its `Source` has no
/// choices; the app's own edits list them ([`App::live_generator`]).
pub fn with_live<R>(
    song: &Song,
    track: &Track,
    phrase: u32,
    index: usize,
    read: impl FnOnce(&Generator, &Context) -> R,
) -> Option<R> {
    let found = track.phrases.get(&phrase)?;
    let generator = found.generators().get(index)?;
    let under = generator.under(found.steps());
    let source = generator
        .source()
        .and_then(|source| song.source_phrase(source))
        .map(|followed| followed.loop_steps());
    let context = generator.context(&under, &[], source)?;
    Some(read(generator, &context))
}

/// A live generator's setting as the Gen page shows it, from the song alone, or nothing if
/// there's no such generator.
pub fn live_shown(
    song: &Song,
    track: &Track,
    phrase: u32,
    index: usize,
    param: GenParam,
) -> String {
    with_live(song, track, phrase, index, |generator, context| {
        param.shown(&generator.shape, context)
    })
    .unwrap_or_default()
}

/// A live generator's name in the phrase's heading: its column, and for a `Follow` what it
/// follows, like `Note←T02:05`.
pub fn generator_label(generator: &Generator) -> String {
    let column = column_label(generator.column);
    match generator.source() {
        Some(source) => format!("{column}←{source}"),
        None => column,
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

/// The longest list of notes or nudges that can be typed: eight notes and the spaces between.
const NAMED_LIST_TEXT: usize = 40;

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
        let panel = match self.new_panel(selection, field, command) {
            Ok(panel) => panel,
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
        self.leave_selection(selection);
        self.cursor.field = field;
        self.shape = Some(panel);
        self.mode = Mode::Shape;
        self.preview_shape();
    }

    /// The panel a selection opens, on its first result, or why it opens none.
    fn new_panel(
        &mut self,
        selection: Selection,
        field: Field,
        command: Option<FxCommand>,
    ) -> Result<ShapePanel, Refusal> {
        let parts = self.shape_parts(selection, field, command)?;
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
        let (sources, default_source) = match column {
            ShapeColumn::Note { .. } => self.follow_sources(&base, &parts),
            _ => (Vec::new(), None),
        };
        Ok(ShapePanel {
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
                follow: Follow::default(),
                walk: Walk {
                    step: default_step(column),
                    ..Walk::default()
                },
                pink: Pink::default(),
                bits: Bits::default(),
            },
            row: GenParam::Fill,
            typing: None,
            live: None,
            base,
            under: None,
            sources,
            default_source,
        })
    }

    /// Make a column of the phrase under the cursor live over its loop, or its lane, with the
    /// shape the panel opens with, as `vG`, `S` and `L` would, for the Gen page's `Empty`. An
    /// empty FX column lists the commands first. Says whether it did.
    pub(crate) fn make_live(&mut self, field: Field, command: Option<FxCommand>) -> bool {
        let extent = self.extent();
        let track = self.cursor.track;
        let selection = Selection {
            range: StepRange::between(extent.start, extent.start + extent.length.max(1) - 1),
            scope: Scope::column(field.group()),
            tracks: TrackSpan::between(track, track),
        };
        let panel = match self.new_panel(selection, field, command) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => {
                self.status = reason.to_owned();
                return false;
            }
            Err(Refusal::Command(track)) => {
                let (_, items) =
                    self.fx_command_items(track, |command| Pick::GenCommand { field, command });
                self.show_picker(Picker::new("FX commands", items).searching(""));
                self.status = "Choose the command to write on the empty FX column".to_owned();
                return false;
            }
        };
        self.apply(&panel.live_action());
        let name = column_label(panel.parts[0].column);
        self.status = format!("{name} is live: turn it here, Enter opens the panel on it");
        true
    }

    /// The phrases these parts can follow, and the one to start on: the phrase beside them on the
    /// nearest track that has one, looking before the first part's track and then after it, as a
    /// canon's theme is usually above its answers, or what that phrase follows, if it does.
    fn follow_sources(&self, song: &Song, parts: &[Part]) -> (Vec<Source>, Option<Source>) {
        let own: Vec<Source> = parts
            .iter()
            .map(|part| Source {
                track: part.track,
                phrase: part.phrase,
            })
            .collect();
        let sources: Vec<Source> = song
            .tracks
            .iter()
            .enumerate()
            .take(song.sound_tracks())
            .flat_map(|(track, owner)| {
                owner.phrases.keys().map(move |phrase| Source {
                    track,
                    phrase: *phrase,
                })
            })
            .filter(|source| !own.iter().any(|here| reads(song, *source, *here)))
            .collect();
        let first = parts.first().map_or(0, |part| part.track);
        let before = (0..first).rev();
        let after = first + 1..song.sound_tracks();
        let mut beside = before.chain(after).find_map(|track| {
            let (phrase, _) = self.phrase_beside(track)?;
            let source = Source { track, phrase };
            sources.contains(&source).then_some(source)
        });
        // One that follows another itself gives way to what it follows, so each voice of a canon
        // starts on the theme. Cycles are refused, so this ends.
        while let Some(followed) = beside
            .and_then(|source| song.source_phrase(source))
            .and_then(|phrase| phrase.generators().iter().find_map(Generator::source))
            .filter(|source| sources.contains(source))
        {
            beside = Some(followed);
        }
        (sources, beside)
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
        let mut name = column_label(part.column);
        if panel.shape.kind == FillKind::Follow {
            name = format!("{name}←{}", panel.shape.follow.source);
        }
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
        if let Some(source) = unfollowed(self.song(), next).first() {
            self.status = format!(
                "{source} is gone, so what followed it is plain notes now. u brings it back"
            );
        } else if let Some(column) = applied(self.song(), next).first() {
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
            generate: is_empty(&under, range, generator.column),
        };
        let shape = generator.shape;
        let (sources, default_source) = match part.column {
            ShapeColumn::Note { .. } => self.follow_sources(&base, &[part]),
            _ => (Vec::new(), None),
        };
        self.shape = Some(ShapePanel {
            selection: Selection {
                range,
                scope: Scope::column(self.cursor.field.group()),
                tracks: TrackSpan::between(track, track),
            },
            parts: vec![part],
            column: part.column,
            shape,
            row: GenParam::Fill,
            typing: None,
            live: Some(index),
            base,
            under: Some(under),
            sources,
            default_source,
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
        let name =
            generator_label(&self.song().tracks[track].phrases[&phrase].generators()[generator]);
        self.apply(&Action::ApplyGenerator {
            track,
            phrase,
            generator,
        });
        self.status = format!("{name} is applied, as plain values");
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
                let generate = is_empty(steps_on(track, phrase), range, column);
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
                let Some(mut shape) = self.shape.as_ref().map(|panel| panel.shape) else {
                    return;
                };
                self.roll(&mut shape);
                self.change_shape(|panel| panel.shape = shape);
            }
            KeyCode::Char('K') => self.open_shape_picker(),
            KeyCode::Char('i') => self.start_shape_typing(),
            _ => {}
        }
    }

    /// Roll a shape's seed again, as `r` does: with `Acid`'s pitches kept, only its second die,
    /// the accents, slides and octaves.
    pub(crate) fn roll(&mut self, shape: &mut Shape) {
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = self.shape_seed;
        if shape.kind == FillKind::Acid && shape.acid.keep {
            shape.acid.roll = shape.acid.roll.wrapping_add(seed).wrapping_add(1);
        } else {
            shape.seed = self.song().seed.wrapping_add(seed);
        }
    }

    /// One of the track's live generators, with what its settings act on, if there's one there.
    pub(crate) fn live_generator(
        &self,
        track: usize,
        phrase: u32,
        index: usize,
    ) -> Option<LiveGen> {
        let song = self.song();
        let found = song.tracks.get(track)?.phrases.get(&phrase)?;
        let generator = found.generators().get(index)?.clone();
        let rows = generator.rows()?;
        let under = generator.under(found.steps());
        let generates = is_empty(&under, rows, generator.column);
        let part = Part {
            track,
            phrase,
            range: rows,
            column: generator.column,
            generate: generates,
        };
        let (sources, default_source) = match generator.column {
            ShapeColumn::Note { .. } => self.follow_sources(song, &[part]),
            _ => (Vec::new(), None),
        };
        let source = song
            .source_phrase(generator.shape.follow.source)
            .map(|phrase| phrase.loop_steps().to_vec());
        let ends = if generates {
            self.generated_ends(generator.column)
        } else {
            default_ends(&under, rows, generator.column)
        };
        Some(LiveGen {
            generator,
            under,
            sources,
            source,
            starts: Starts {
                ends,
                source: default_source,
            },
        })
    }

    fn change_shape(&mut self, change: impl FnOnce(&mut ShapePanel)) {
        if let Some(panel) = &mut self.shape {
            change(panel);
            settle(&mut panel.shape, &panel.sources, panel.default_source);
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
        let Some(panel) = &self.shape else {
            return;
        };
        let mut shape = panel.shape;
        if let Err(reason) = panel.row.step(&mut shape, &panel.context(), sign, large) {
            self.status = reason.to_owned();
            return;
        }
        self.change_shape(|panel| panel.shape = shape);
    }

    /// Where `From` and `To` go back to: the ends of what was there on the first track, or on a
    /// column generated on, the ends it starts with.
    fn shape_ends(&self, panel: &ShapePanel) -> (u8, u8) {
        let first = panel.parts[0];
        if first.generate {
            self.generated_ends(first.column)
        } else {
            default_ends(panel.base_steps(&first), first.range, first.column)
        }
    }

    /// Put the current row back to what it started as.
    fn reset_shape_row(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let starts = Starts {
            ends: self.shape_ends(panel),
            source: panel.default_source,
        };
        self.change_shape(|panel| {
            let mut shape = panel.shape;
            panel.row.reset(&mut shape, &panel.context(), starts);
            panel.shape = shape;
            let rows = panel.rows();
            if !rows.contains(&panel.row) {
                panel.row = if rows.contains(&GenParam::Where) {
                    GenParam::Where
                } else {
                    GenParam::Fill
                };
            }
        });
    }

    fn open_shape_picker(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let context = panel.context();
        let Some(choices) = panel.row.choices(&context) else {
            self.status = "K chooses from a list on Fill, Shape, Rate, Where and Source".to_owned();
            return;
        };
        let position = panel.row.chosen(&panel.shape, &context).unwrap_or(0);
        let items = choices
            .into_iter()
            .map(|choice| {
                let item = Item::new(choice.name(), Pick::ShapeChoice(choice));
                match (choice, choice.description()) {
                    (Choice::Source(source), _) => {
                        let rows = panel
                            .base
                            .source_phrase(source)
                            .map_or(0, |phrase| phrase.loop_steps().len());
                        let name = crate::views::phrase::track_name(self, source.track);
                        item.meaning(format!("Track {name}, {rows} rows"))
                    }
                    (_, Some(meaning)) => item.meaning(meaning),
                    (_, None) => item,
                }
            })
            .collect();
        let title = match panel.row {
            GenParam::Source => "Follow",
            row => row.name(),
        };
        self.show_picker(Picker::new(title, items).opened_on(Some(position)));
    }

    pub(crate) fn pick_shape_choice(&mut self, choice: Choice) {
        self.change_shape(|panel| choice.apply(&mut panel.shape));
    }

    fn start_shape_typing(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        match panel.row.typing(panel.column) {
            Typing::Listed => self.open_shape_picker(),
            Typing::Toggled => self.adjust_shape(1, false),
            _ => {
                if let Some(panel) = &mut self.shape {
                    panel.typing = Some(String::new());
                }
            }
        }
    }

    fn shape_typing_key(&mut self, code: KeyCode) {
        let Some(panel) = &mut self.shape else {
            return;
        };
        let typing = panel.row.typing(panel.column);
        let Some(text) = &mut panel.typing else {
            return;
        };
        match (code, typing) {
            (KeyCode::Esc, _) => panel.typing = None,
            (KeyCode::Backspace, _) => {
                text.pop();
            }
            (KeyCode::Enter, _) => self.commit_shape_typing(),
            // A note's name or a nudge, alone or in a list, ended with Enter.
            (KeyCode::Char(c), Typing::Named { list }) => {
                let longest = if list { NAMED_LIST_TEXT } else { 4 };
                if text.len() < longest
                    && (c.is_ascii_alphanumeric()
                        || matches!(c, '#' | '-' | '+' | '_')
                        || (list && c == ' '))
                {
                    text.push(c);
                }
            }
            (KeyCode::Char(c), Typing::HexList) => type_list(text, c),
            (KeyCode::Char(c), Typing::Hex) if c.is_ascii_hexdigit() => {
                text.push(c.to_ascii_uppercase());
                if text.len() == 2 {
                    self.commit_shape_typing();
                }
            }
            (KeyCode::Char(c), Typing::Rate)
                if text.len() < RATE_TEXT
                    && (c.is_ascii_alphanumeric() || matches!(c, '/' | ' ')) =>
            {
                text.push(c);
            }
            (KeyCode::Char(c), Typing::Whole { signed })
                if text.len() < 4
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
            let mut shape = panel.shape;
            refused = panel.row.typed(&mut shape, &panel.context(), &text).err();
            panel.shape = shape;
        });
        if let Some(reason) = refused {
            self.status = reason.to_owned();
        }
    }
}
