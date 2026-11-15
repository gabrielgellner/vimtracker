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

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::selection::{Field, StepRange};
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{
    Channel, Context, DrumPart, FillKind, GenParam, Grids, Shape, ShapeColumn, Source, Starts,
    TonnetzVoice, Walk, Where, default_ends, default_step, divisions_seed, drum_parts, is_empty,
    on_track, open_rows, track_seed, variation_notes,
};
use vt_model::song::{Machine, Song, SynthVoice, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Note, Step};

use crate::app::{App, Selection, TrackSpan};
use crate::input::bindings::{Cmd, adjusting_key};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::typed::{Typed, TypedValue};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::services::generators::{Part, column_label, settle, too_few};

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
/// `hits`, whether each has a note on its first row (decision 326). Any other is as it is.
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
    /// The setting the cursor is on.
    pub row: GenParam,
    /// What's being typed into the current row.
    pub typing: Option<TypedValue>,
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
        let generates = self.parts.iter().any(|part| part.generate);
        GenParam::of(&self.shape, self.column, generates)
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
    pub(crate) fn source_steps(&self) -> Option<&[Step]> {
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
                    scale: self.services.scale,
                    writes: (!notes).then_some(self.services.note_velocity),
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
                    drum: DrumPart::default(),
                }),
                Err(Refusal::Command(track)) => return Err(Refusal::Command(track)),
                Err(Refusal::Reason(reason)) => {
                    refused.get_or_insert(reason);
                }
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
        let known: Vec<Option<DrumPart>> = tracks
            .iter()
            .map(|track| self.services.drum_of(*track))
            .collect();
        drum_parts(&known)
    }

    /// After a kit's sounds are loaded as tracks, offer a beat from Grids' map on the first kick,
    /// snare and hat among them, each playing its own part as `S` across them would (decision
    /// 296), in a list whose `Enter` writes it and whose `Esc` leaves the tracks empty.
    pub(crate) fn offer_kit_beat(&mut self, tracks: &[usize]) {
        let mut found = [false; DrumPart::ALL.len()];
        let drums: Vec<usize> = tracks
            .iter()
            .copied()
            .filter(|&track| {
                self.services
                    .drum_of(track)
                    .is_some_and(|part| !std::mem::replace(&mut found[part.index()], true))
            })
            .collect();
        if drums.is_empty() {
            return;
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
        self.show_picker(Picker::new(KIT_BEAT_PICKER, items));
    }

    /// Write a beat from Grids' map on these tracks' first phrases, each its own part, from the
    /// middle of the map as `S` starts, as one undo step.
    pub(crate) fn write_kit_beat(&mut self, tracks: &[usize]) {
        let song = Arc::clone(self.song());
        let column = ShapeColumn::Note {
            scale: self.services.scale,
            writes: Some(self.services.note_velocity),
        };
        let ends = self.services.generated_ends(column);
        self.services.shape_seed = self.services.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.services.shape_seed);
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
                })
            })
            .collect();
        if actions.is_empty() {
            return;
        }
        self.services.apply(&Action::Batch(actions));
        self.services.status =
            "A beat from Grids' map: S on its notes changes it, u takes it off".to_owned();
    }

    /// Open the panel on the selection, and show its first result. An empty FX column lists the
    /// commands first, to choose the one it writes.
    pub(crate) fn open_shape(&mut self) {
        let selection = self.current_selection();
        let field = self.services.cursor.field;
        self.open_shape_on(selection, field, None);
    }

    /// Open the panel on the selection, now its empty FX column has its command.
    pub(crate) fn pick_shape_command(&mut self, command: FxCommand) {
        if let Some((selection, field)) = self.overlays.shape_command.take() {
            self.open_shape_on(selection, field, Some(command));
        }
    }

    fn open_shape_on(&mut self, selection: Selection, field: Field, command: Option<FxCommand>) {
        let panel = match self.new_panel(selection, field, command) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => {
                self.services.status = reason.to_owned();
                return;
            }
            Err(Refusal::Command(track)) => {
                self.leave_selection(selection);
                self.services.cursor.field = field;
                self.overlays.shape_command = Some((selection, field));
                let (_, items) = self.services.fx_command_items(track, Pick::ShapeCommand);
                self.show_picker(Picker::new("FX commands", items).searching(""));
                self.services.status =
                    "Choose the command to write on the empty FX column".to_owned();
                return;
            }
        };
        self.leave_selection(selection);
        self.services.cursor.field = field;
        self.overlays.shape = Some(panel);
        self.modes.mode = Mode::Shape;
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
        let alone = parts.len() == 1;
        // Across tracks, a Note column with notes on some offers what re-pitching them does.
        let column = match first.column {
            ShapeColumn::Note { scale, .. } if !generates => ShapeColumn::Note {
                scale,
                writes: None,
            },
            column => column,
        };
        let (from, to) = if first.generate {
            self.services.generated_ends(first.column)
        } else {
            default_ends(steps, first.range, first.column)
        };
        self.services.shape_seed = self.services.shape_seed.wrapping_add(1);
        let place = if generates {
            Where::AllRows
        } else {
            column.default_where()
        };
        let (sources, default_source) = match column {
            ShapeColumn::Note { .. } => {
                self.services.follow_sources(&base, &parts, self.modes.view)
            }
            _ => (Vec::new(), None),
        };
        let mut shape = starting_shape(column, (from, to), place);
        shape.seed = base.seed.wrapping_add(self.services.shape_seed);
        // Across tracks, each takes its own part, and its own channel; a track at `1/32` reads
        // its rows as 32nds, and `Divisions` plays its rows as long as the track's step.
        shape.grids.part = alone.then_some(first.drum);
        shape.divisions.channel = alone.then_some(Channel::A);
        shape.divisions = shape
            .divisions
            .for_step(base.tracks[first.track].speed.ticks());
        // A `Tonnetz` on the `Chord` voice writes its chords for it, and on any other its roots;
        // across tracks, each takes a voice of the triad (decision 332).
        shape.tonnetz.voice = alone.then(|| match base.tracks[first.track].machine {
            Machine::Plaits(SynthVoice::Chord) => TonnetzVoice::Chord,
            _ => TonnetzVoice::Root,
        });
        Ok(ShapePanel {
            selection,
            parts,
            column,
            shape,
            row: GenParam::Fill,
            typing: None,
            live: None,
            base,
            under: None,
            sources,
            default_source,
        })
    }

    /// Show the panel's result, without adding an undo step.
    fn preview_shape(&mut self) {
        let Some(panel) = &self.overlays.shape else {
            return;
        };
        if let Ok(next) = edit(&panel.base, &panel.action()) {
            self.services.history.replace_current(next);
            self.services.publish();
        }
    }

    /// Keep what's showing, as one undo step: as plain values, or live. A live generator reopened
    /// and kept as plain values is applied.
    fn keep_shape(&mut self, live: bool) {
        if let Some(why) = self
            .overlays
            .shape
            .as_ref()
            .and_then(ShapePanel::too_few_notes)
        {
            self.services.status = why;
            return;
        }
        let Some(panel) = self.overlays.shape.take() else {
            return;
        };
        self.modes.mode = Mode::Normal;
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
                self.services.status = error.to_string();
                shown
            }
        };
        self.services
            .history
            .replace_current(Arc::clone(&panel.base));
        if panel.live.is_none() {
            self.services.say_applied(&result);
        }
        if self.services.history.commit(result) {
            self.services.dirty = true;
        }
        self.services.publish();
        self.remember_shape(&panel);
        let mut name = column_label(part.column);
        if panel.shape.kind == FillKind::Follow {
            name = format!("{name}←{}", panel.shape.follow.source);
        }
        match (panel.live, live) {
            (None, true) => {
                let lane = panel.on_lane(&part);
                self.services.status = match panel.parts.len() {
                    1 if lane => format!(
                        "{name} is live on its lane: gl opens it again, Space g a applies it"
                    ),
                    1 => format!("{name} is live: gl opens it again, Space g a applies it"),
                    tracks => format!(
                        "{name} is live on {tracks} tracks, each on its own: gl opens one again"
                    ),
                };
            }
            (Some(_), true) => self.services.status = format!("{name} stays live"),
            (Some(_), false) => {
                self.services.status = format!("{name} is applied, as plain values");
            }
            (None, false) => {}
        }
    }

    /// Put back what was there before the panel opened.
    fn cancel_shape(&mut self) {
        let Some(panel) = self.overlays.shape.take() else {
            return;
        };
        self.modes.mode = Mode::Normal;
        self.services.history.replace_current(panel.base);
        self.services.publish();
    }

    /// The mode a picker goes back to.
    pub(crate) fn mode_after_picker(&self) -> Mode {
        if self.overlays.shape.is_some() {
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
                        scale: self.services.scale,
                        writes: generate.then_some(self.services.note_velocity),
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
            self.services.generated_ends(column)
        } else {
            default_ends(steps_on(track, phrase), first, column)
        };
        self.services.shape_seed = self.services.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.services.shape_seed);
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
                }
            })
            .collect();
        let action = if actions.len() == 1 {
            actions.remove(0)
        } else {
            Action::Batch(actions)
        };
        self.services.apply(&action);
    }

    /// Handle a key while the panel is open.
    pub(crate) fn shape_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart),
            _ => {}
        }
        let typing = self
            .overlays
            .shape
            .as_ref()
            .is_some_and(|p| p.typing.is_some());
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
                let Some(mut shape) = self.overlays.shape.as_ref().map(|panel| panel.shape) else {
                    return;
                };
                self.services.roll(&mut shape);
                self.change_shape(|panel| panel.shape = shape);
            }
            KeyCode::Char('K') => self.open_shape_picker(),
            KeyCode::Char('i') => self.start_shape_typing(),
            _ => {}
        }
    }

    fn change_shape(&mut self, change: impl FnOnce(&mut ShapePanel)) {
        if let Some(panel) = &mut self.overlays.shape {
            change(panel);
            settle(&mut panel.shape, &panel.sources, panel.default_source);
        }
        self.preview_shape();
    }

    fn move_shape_row(&mut self, by: isize) {
        if let Some(panel) = &mut self.overlays.shape {
            let rows = panel.rows();
            let at = rows.iter().position(|row| *row == panel.row).unwrap_or(0);
            let to = at.saturating_add_signed(by).min(rows.len() - 1);
            panel.row = rows[to];
        }
    }

    fn adjust_shape(&mut self, sign: i16, large: bool) {
        let Some(panel) = &self.overlays.shape else {
            return;
        };
        let mut shape = panel.shape;
        if let Err(reason) = panel.row.step(&mut shape, &panel.context(), sign, large) {
            self.services.status = reason.to_owned();
            return;
        }
        self.change_shape(|panel| panel.shape = shape);
    }

    /// Where `From` and `To` go back to: the ends of what was there on the first track, or on a
    /// column generated on, the ends it starts with.
    fn shape_ends(&self, panel: &ShapePanel) -> (u8, u8) {
        let first = panel.parts[0];
        if first.generate {
            self.services.generated_ends(first.column)
        } else {
            default_ends(panel.base_steps(&first), first.range, first.column)
        }
    }

    /// Put the current row back to what it started as.
    fn reset_shape_row(&mut self) {
        let Some(panel) = &self.overlays.shape else {
            return;
        };
        let starts = Starts {
            ends: self.shape_ends(panel),
            source: panel.default_source,
            part: Some(panel.parts[0].drum),
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
        let Some(panel) = &self.overlays.shape else {
            return;
        };
        let title = match panel.row {
            GenParam::Source => "Follow",
            row => row.name(),
        };
        let follows = |source: Source| {
            let rows = panel
                .base
                .source_phrase(source)
                .map_or(0, |phrase| phrase.loop_steps().len());
            let name = self.services.track_name(source.track);
            format!("Track {name}, {rows} rows")
        };
        let picker = choice_picker(
            title,
            panel.row,
            &panel.shape,
            &panel.context(),
            Pick::ShapeChoice,
            Some(&follows),
        );
        match picker {
            Some(picker) => self.show_picker(picker),
            None => {
                self.services.status =
                    "K chooses from a list on Fill, Shape, Rate, Where, Source and Part".to_owned();
            }
        }
    }

    pub(crate) fn pick_shape_choice(&mut self, choice: Choice) {
        self.change_shape(|panel| choice.apply(&mut panel.shape));
    }

    fn start_shape_typing(&mut self) {
        let Some(panel) = &self.overlays.shape else {
            return;
        };
        match panel.row.typing(panel.column) {
            Typing::Listed => self.open_shape_picker(),
            Typing::Toggled => self.adjust_shape(1, false),
            _ => {
                if let Some(panel) = &mut self.overlays.shape {
                    panel.typing = Some(TypedValue::default());
                }
            }
        }
    }

    fn shape_typing_key(&mut self, code: KeyCode) {
        let Some(panel) = &mut self.overlays.shape else {
            return;
        };
        let typing = panel.row.typing(panel.column);
        let Some(text) = &mut panel.typing else {
            return;
        };
        // `Enter` keeps a single hex digit too, here.
        let typed = match code {
            KeyCode::Enter => Typed::Commit(text.take()),
            _ => text.key(code, typing.into()),
        };
        match typed {
            Typed::Editing => {}
            Typed::Cancel => panel.typing = None,
            Typed::Commit(text) => {
                panel.typing = None;
                self.commit_shape_typing(&text);
            }
        }
    }

    fn commit_shape_typing(&mut self, text: &str) {
        let mut refused = None;
        self.change_shape(|panel| {
            let mut shape = panel.shape;
            refused = panel.row.typed(&mut shape, &panel.context(), text).err();
            panel.shape = shape;
        });
        if let Some(reason) = refused {
            self.services.status = reason.to_owned();
        }
    }
}
