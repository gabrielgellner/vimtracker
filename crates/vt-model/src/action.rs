//! Recorded edits. `edit` is the only path that changes a song.

use std::sync::Arc;

use thiserror::Error;

use crate::accumulator::{Accumulation, MOST_DEGREES, MOST_LOOPS};
use crate::arpeggio::{Arpeggio, MOST_OCTAVES};
use crate::bank::{BankEntry, Points};
use crate::constants::{
    DEFAULT_PHRASE_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_LEVEL, MAX_MILLIBPM,
    MAX_PROJECT_NAME_CHARS, MAX_SOUND_TRACKS, MAX_TRACK_RESET, MAX_TRANSPOSE, MIN_MILLIBPM,
    MIN_TRANSPOSE,
};
use crate::generator::{Generator, Span, reads};
use crate::groove::{CUSTOM_GROOVES, Groove, GrooveId};
use crate::insert::Insert;
use crate::kit::CHOKE_GROUPS;
use crate::modulation::ModSlot;
use crate::names::is_project_name;
use crate::order::Order;
use crate::scale::Scale;
use crate::selection::{
    Block, Scope, Shift, StepRange, clear_step, mark_step, merge_step, paste_step, shift_step,
};
use crate::settings::SettingId;
use crate::shape::{FillKind, Shape, ShapeColumn, Source, written_from};
use crate::song::{
    Chain, ChainEntry, FilterMode, FilterType, Lane, Launch, Machine, Phrase, Repeat, Section,
    SectionStart, Song, Track, lowest_unused,
};
use crate::speed::Speed;
use crate::step::{FxCommand, FxSlot, Gate, MAX_NUDGE, Mark, Note, Step};

/// A new value for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue {
    Velocity(Option<u8>),
    /// A row's nudge, in 24ths of a step.
    Nudge(i8),
    Gate(Option<Gate>),
    Slot(usize, FxSlot),
}

/// A value written across a selection. A velocity means nothing without a note, and
/// an FX value nothing without a command, so those only go on steps that have one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// A note, with a velocity for steps that don't have one.
    Note {
        note: Note,
        velocity: u8,
    },
    Velocity(Option<u8>),
    /// A gate cell, which goes on every row, as a row without a note still takes its time.
    Gate(Option<Gate>),
    /// A gate cell's length, which only goes on rows with a note, as it's how long the note lasts.
    /// A row without a gate cell gets one pulse of `SGL` to hold it.
    GateLength(Option<u8>),
    /// A nudge, which only goes on rows with a note, as velocity does. 0 takes it away anywhere.
    Nudge(i8),
    Slot(usize, FxSlot),
    /// An FX value's coarse digits, keeping its fine part.
    SlotValue(usize, u8),
    /// An FX value's fine digits, where it has a fine part.
    SlotFine(usize, u8),
}

/// A sound from a kit going on a track: the bank entry it plays, and the name and choke group its
/// track takes. Its gain and tune are the entry's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitSound {
    pub sample: u8,
    pub name: String,
    pub choke: Option<u8>,
}

/// One edit, as plain data so it can be replayed and tested without a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write a note.
    SetNote {
        track: usize,
        phrase: u32,
        step: usize,
        note: Note,
    },
    /// Replace a whole step.
    SetStep {
        track: usize,
        phrase: u32,
        step: usize,
        value: Step,
    },
    /// Several edits as one change, or none if any is refused.
    Batch(Vec<Action>),
    /// Set the mark on every value in a selection.
    SetMark {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
        mark: Mark,
    },
    /// Set one field of a step.
    SetField {
        track: usize,
        phrase: u32,
        step: usize,
        value: FieldValue,
    },
    /// Fill a column across steps with a ramp or random values.
    Shape {
        track: usize,
        phrase: u32,
        range: StepRange,
        column: ShapeColumn,
        shape: Shape,
    },
    /// Keep a shape live on a column of a phrase, on rows or on the column's lane, starting from
    /// the values there.
    Generate {
        track: usize,
        phrase: u32,
        column: ShapeColumn,
        span: Span,
        shape: Shape,
    },
    /// Give one of a phrase's live generators, by its place in the phrase's list, a new shape.
    Reshape {
        track: usize,
        phrase: u32,
        generator: usize,
        shape: Shape,
    },
    /// Apply one of a phrase's live generators, leaving its values as plain ones.
    ApplyGenerator {
        track: usize,
        phrase: u32,
        generator: usize,
    },
    /// Write one value across steps.
    Fill {
        track: usize,
        phrase: u32,
        range: StepRange,
        fill: Fill,
    },
    /// Clear fields across steps.
    Clear {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
    },
    /// Move values across steps.
    Shift {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
        by: i32,
        kind: Shift,
        /// The scale notes move through, if it's on.
        scale: Option<Scale>,
    },
    /// Move the notes in a phrase to the nearest in a scale.
    FitToScale {
        track: usize,
        phrase: u32,
        scale: Scale,
    },
    /// Write a copied block some number of times end to end. Past the last stored step this stores
    /// more without changing the loop. Merging leaves steps alone where the copy is empty, and
    /// otherwise the copy overwrites them.
    Paste {
        track: usize,
        phrase: u32,
        at: usize,
        group: usize,
        block: Block,
        times: usize,
        merge: bool,
    },
    /// Double a phrase's loop by repeating it.
    Double {
        track: usize,
        phrase: u32,
    },
    SetSkipSize(u32),
    SetLimiter(bool),
    /// Name the project.
    SetSongName(String),
    /// Set the project's random seed.
    SetSeed(u64),
    /// When launched chains start.
    SetLaunch(Launch),
    /// What a chain is called on screen.
    SetChainLabel {
        track: usize,
        chain: u32,
        label: String,
    },
    /// Set the groove every track plays with unless it has its own.
    SetGroove(GrooveId),
    /// Give a track its own groove, or `None` to follow the project.
    SetTrackGroove {
        track: usize,
        groove: Option<GrooveId>,
    },
    /// Make or change one of the project's own grooves, counted from 0.
    SetCustomGroove {
        custom: usize,
        groove: Groove,
    },
    /// Set where one of a track's held values starts.
    SetTrackBase {
        track: usize,
        control: TrackControl,
        value: u8,
    },
    /// Replace one of a track's mod slots: its type, where it goes, its switch, or any type's own
    /// settings.
    SetMod {
        track: usize,
        slot: usize,
        modulator: ModSlot,
    },
    /// Put an effect, or nothing, in one of a track's slots.
    SetInsert {
        track: usize,
        slot: usize,
        insert: Insert,
    },
    /// Switch one of a track's slots out, or back in.
    SetBypass {
        track: usize,
        slot: usize,
        bypassed: bool,
    },
    /// Send every track back to its start after this many steps, or `None` to let them run.
    SetReset {
        section: usize,
        reset: Option<usize>,
    },
    /// Add a section after a position, with no chains.
    AddSection {
        at: usize,
    },
    /// Copy a section after itself, using the same chains.
    CloneSection {
        at: usize,
    },
    /// Remove a section, keeping at least one. Its chains stay for reuse.
    RemoveSection {
        at: usize,
    },
    /// Set how long a section plays, or follow its longest chain.
    SetSectionLength {
        section: usize,
        length: Option<usize>,
    },
    /// Set how many times a section plays before moving on.
    SetSectionRepeat {
        section: usize,
        repeat: Repeat,
    },
    /// Set what a section does to the tracks when it starts.
    SetSectionStart {
        section: usize,
        start: SectionStart,
    },
    /// Choose a section's chain for a track, or none.
    SetSectionChain {
        section: usize,
        track: usize,
        chain: Option<u32>,
    },
    /// Set a track's level.
    SetTrackLevel {
        track: usize,
        level: u8,
    },
    /// Set a track's trim, the gain at its input.
    SetTrackTrim {
        track: usize,
        trim: u8,
    },
    /// Put a track in a choke group, from 1 to `CHOKE_GROUPS`, or in none.
    SetTrackChoke {
        track: usize,
        choke: Option<u8>,
    },
    /// Put the sound tracks from `first` to `last` in a group, from 1 to `MAX_GROUPS`, or take
    /// them out of any, as one change.
    SetTrackGroup {
        first: usize,
        last: usize,
        group: Option<u8>,
    },
    /// Give a track its own `Gate`, on the `00` to `FF` scale, or `None` for its machine's.
    SetTrackGateLength {
        track: usize,
        gate_length: Option<u8>,
    },
    /// Set how long each of a track's steps is.
    SetTrackSpeed {
        track: usize,
        speed: Speed,
    },
    /// Set how many of the song's steps a track's chain plays before starting again, from 1 to
    /// `MAX_TRACK_RESET`, or `None` to play on.
    SetTrackReset {
        track: usize,
        reset: Option<u16>,
    },
    /// Set how long a track's ties glide into the next note, on the `00` to `FF` scale of
    /// [`glide_ms`](crate::settings::glide_ms), where `00` is off.
    SetTrackGlide {
        track: usize,
        glide: u8,
    },
    /// Switch a track filter's `Safe` on or off, which keeps `Res` short of ringing by itself.
    SetTrackSafe {
        track: usize,
        on: bool,
    },
    /// Set how a track's `ACU`s move its notes.
    SetAccumulation {
        track: usize,
        accumulation: Accumulation,
    },
    /// Set how a track's `ARP`s play.
    SetArpeggio {
        track: usize,
        arpeggio: Arpeggio,
    },
    /// Change the length of a phrase's loop, keeping every step.
    SetLength {
        track: usize,
        phrase: u32,
        length: usize,
    },
    /// Play a phrase's rows in another order.
    SetOrder {
        track: usize,
        phrase: u32,
        order: Order,
    },
    /// Give an FX column of a phrase a lane of its own, or none to follow the phrase's loop.
    SetLane {
        track: usize,
        phrase: u32,
        slot: usize,
        lane: Option<Lane>,
    },
    /// Move a phrase's loop by some steps, keeping its length.
    SlideLoop {
        track: usize,
        phrase: u32,
        by: isize,
    },
    /// Move a phrase's loop steps later or earlier, wrapping round within the loop.
    RotateLoop {
        track: usize,
        phrase: u32,
        by: isize,
    },
    /// Change the tempo, in thousandths of a BPM.
    SetMilliBpm(u32),
    /// Add tracks at the end, each with a new chain and phrase in the first section.
    AddTracks(usize),
    /// Remove the last track and its chains and phrases, keeping at least one.
    RemoveLastTrack,
    /// Choose the bank entry a track's sampler plays, or `00` for none.
    SetTrackSample {
        track: usize,
        sample: u8,
    },
    /// Put an entry in the bank at a number, replacing any there, or empty the number.
    SetBankEntry {
        number: u8,
        entry: Option<BankEntry>,
    },
    /// Cut a bank entry into slices starting at these frames of its file, or into none, as
    /// [`Points::reslice`](crate::bank::Points::reslice) does, and have the steps that chose a
    /// slice of it by number choose the one holding the sound they played, as one change.
    SetSlices {
        number: u8,
        starts: Vec<u64>,
    },
    /// Split the slice of a bank entry holding a frame of its file in two there, as
    /// [`Points::split_starts`](crate::bank::Points::split_starts) does. The part before keeps the
    /// slice's id and marks, and the steps' `SLI` locks follow their sound as a re-slice's do.
    SplitSlice {
        number: u8,
        at: u64,
    },
    /// Join a bank entry's slice, by its number, to the next, which it takes in with its marks, as
    /// [`Points::join_starts`](crate::bank::Points::join_starts) does, the steps' `SLI` locks
    /// following their sound.
    JoinSlices {
        number: u8,
        slice: u8,
    },
    /// Name a track, or leave it with no name.
    SetTrackName {
        track: usize,
        name: String,
    },
    /// Put a kit's sounds on tracks from the first, as one change, adding tracks if it needs more.
    LoadKit(Vec<KitSound>),
    /// Choose what makes a track's sound.
    SetTrackMachine {
        track: usize,
        machine: Machine,
    },
    /// Set one of a track's machine's settings.
    SetTrackSetting {
        track: usize,
        setting: SettingId,
        value: u8,
    },
    /// Set the fine part of one of a track's own settings or its machine's.
    SetTrackFine {
        track: usize,
        setting: SettingId,
        fine: u8,
    },
    /// Choose a track's filter.
    SetFilterType {
        track: usize,
        filter: FilterType,
    },
    /// Choose how a track's filter responds.
    SetFilterMode {
        track: usize,
        mode: FilterMode,
    },
    /// Change a chain row. A phrase that doesn't exist yet is created.
    SetChainEntry {
        track: usize,
        chain: u32,
        slot: usize,
        entry: ChainEntry,
    },
    /// Add a chain row. A phrase that doesn't exist yet is created.
    InsertChainEntry {
        track: usize,
        chain: u32,
        at: usize,
        entry: ChainEntry,
    },
    /// Remove a chain row, keeping at least one. Its phrase stays for reuse.
    RemoveChainEntry {
        track: usize,
        chain: u32,
        at: usize,
    },
    /// Copy the phrase in a chain row to a new ID, and point the row at the copy.
    ClonePhrase {
        track: usize,
        chain: u32,
        slot: usize,
        new: u32,
    },
    /// Copy a track's chain and its phrases to new IDs, and point the track at the copy.
    CloneChain {
        section: usize,
        track: usize,
    },
    /// Point a track at a chain. A chain that doesn't exist yet gets a new phrase.
    SetTrackChain {
        section: usize,
        track: usize,
        chain: u32,
    },
}

/// One of the values a track holds, set from its page rather than a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackControl {
    Cutoff,
    Resonance,
    Pan,
    SendA,
    SendB,
    Drive,
    Punch,
    LowCut,
    HighCut,
    Pitch,
    VelocityDelta,
    Quantise,
}

impl TrackControl {
    /// The setting this is the base of, if steps can set it too.
    pub fn setting(self) -> Option<SettingId> {
        match self {
            Self::Cutoff => Some(SettingId::CUT),
            Self::Resonance => Some(SettingId::RES),
            Self::Pan => Some(SettingId::PAN),
            Self::SendA => Some(SettingId::SEND_A),
            Self::SendB => Some(SettingId::SEND_B),
            Self::Pitch => Some(SettingId::PITCH),
            Self::VelocityDelta => Some(SettingId::VELOCITY_DELTA),
            Self::Quantise => Some(SettingId::QUANTISE),
            Self::Drive | Self::Punch | Self::LowCut | Self::HighCut => None,
        }
    }
}

/// Cut a bank entry into slices starting where `starts` says from its points, moving its marks
/// and the steps' `SLI` locks with their sound, as one change.
fn reslice(
    song: &mut Song,
    number: u8,
    starts: impl FnOnce(&Points) -> Result<Vec<u64>, EditError>,
) -> Result<(), EditError> {
    let mut entry = song
        .bank
        .get(number)
        .cloned()
        .ok_or(EditError::NoSample(number))?;
    let starts = starts(&entry.edit.points)?;
    let renumbered = entry.edit.points.reslice(&starts);
    song.bank.set(number, Some(entry));
    if let Some(renumbered) = renumbered {
        song.renumber_slices(number, &renumbered);
    }
    Ok(())
}

/// An FX slot with its value within what its command takes, and a fine part only on what has
/// one, or on an `SLI` locking a slice by its letter.
fn snapped(slot: FxSlot) -> FxSlot {
    if let Some(letter) = slot.slice_mark() {
        return FxSlot::slice_by_mark(letter, slot.mark).with_valid_mark();
    }
    let highest = slot.command.map_or(0xFF00, FxCommand::highest_value);
    let value = if slot.command.is_some_and(FxCommand::has_fine) {
        slot.value
    } else {
        slot.value & 0xFF00
    };
    FxSlot {
        value: value.min(highest),
        ..slot
    }
    .with_valid_mark()
}

fn section_mut(song: &mut Song, section: usize) -> Result<&mut Section, EditError> {
    song.sections
        .get_mut(section)
        .ok_or(EditError::NoSection(section))
}

/// A choke group is 1 to `CHOKE_GROUPS`, or none.
fn check_choke(choke: Option<u8>) -> Result<(), EditError> {
    match choke {
        Some(group) if !(1..=CHOKE_GROUPS).contains(&group) => Err(EditError::Choke(group)),
        _ => Ok(()),
    }
}

fn track_mut(song: &mut Song, track: usize) -> Result<&mut Track, EditError> {
    song.tracks.get_mut(track).ok_or(EditError::NoTrack(track))
}

/// Why an edit was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("no section {0}")]
    NoSection(usize),
    #[error("a song needs at least one section")]
    LastSection,
    #[error("no phrase {0:02X}")]
    NoPhrase(u32),
    #[error("phrase {phrase:02X} has no step {step}")]
    NoStep { phrase: u32, step: usize },
    #[error("phrase {phrase:02X} has no live generator {generator}")]
    NoGenerator { phrase: u32, generator: usize },
    #[error("a live generator on a lane needs an FX column with a lane")]
    NoLane,
    #[error("there's no phrase {0} to follow")]
    NoSource(Source),
    #[error("a phrase can't follow itself")]
    FollowsItself,
    #[error("{0} follows this phrase already, directly or through others, so this can't follow it")]
    FollowCycle(Source),
    #[error("tempo must be from 20 to 400 BPM")]
    Tempo,
    #[error("a project can have at most {MAX_SOUND_TRACKS} tracks")]
    TooManyTracks,
    #[error("a project needs at least one track")]
    LastTrack,
    #[error("those columns don't match here")]
    ColumnMismatch,
    #[error(
        "a mod slot's depth goes to FF either way, an envelope's times to {MAX_ENVELOPE_MS} ms and its sustain to 100%, and an LFO's rate and phase are in range"
    )]
    Mod,
    #[error("no chain {0:02X}")]
    NoChain(u32),
    #[error("chain {chain:02X} has no row {slot:02X}")]
    NoRow { chain: u32, slot: usize },
    #[error("a chain needs at least one phrase")]
    LastRow,
    #[error("transpose goes from {MIN_TRANSPOSE} to {MAX_TRANSPOSE}")]
    Transpose,
    #[error("a project has {CUSTOM_GROOVES} grooves of its own")]
    CustomGroove,
    #[error("no track {0}")]
    NoTrack(usize),
    #[error("the bank has no sample {0:02X}")]
    NoSample(u8),
    #[error("a slice already starts there, or it's outside the sample")]
    Split,
    #[error("slice {0:02X} has no slice after it to join")]
    Join(u8),
    #[error("a reset needs at least 1 step")]
    Reset,
    #[error("a track's reset is from 1 to {MAX_TRACK_RESET} steps, or off")]
    TrackReset,
    #[error("choke group {0} isn't one of 1 to {CHOKE_GROUPS}")]
    Choke(u8),
    #[error("group {0} isn't one of 1 to {MAX_GROUPS}")]
    Group(u8),
    #[error("only tracks that play sounds go in a group")]
    GroupMember,
    #[error(
        "an accumulator's limits go from 1 to {MOST_DEGREES}, and its loops from 1 to {MOST_LOOPS}"
    )]
    Accumulation,
    #[error("an arpeggio climbs from 1 to {MOST_OCTAVES} octaves")]
    Arpeggio,
    #[error(
        "a project's name is up to {MAX_PROJECT_NAME_CHARS} characters, with no / and not starting with a dot"
    )]
    SongName,
}

/// Apply an action, returning a new song that shares everything unchanged with the old one.
pub fn edit(song: &Arc<Song>, action: &Action) -> Result<Arc<Song>, EditError> {
    if let Action::Batch(actions) = action {
        let mut current = Arc::clone(song);
        for action in actions {
            current = edit(&current, action)?;
        }
        return Ok(current);
    }
    let mut next = Song::clone(song);
    match action {
        Action::Batch(_) => {}
        Action::SetNote {
            track,
            phrase,
            step,
            note,
        } => {
            step_mut(&mut next, *track, *phrase, *step)?.note = *note;
        }
        Action::SetStep {
            track,
            phrase,
            step,
            value,
        } => {
            *step_mut(&mut next, *track, *phrase, *step)? = value.clone();
        }
        Action::SetField {
            track,
            phrase,
            step,
            value,
        } => {
            let target = step_mut(&mut next, *track, *phrase, *step)?;
            match value {
                FieldValue::Velocity(velocity) => target.velocity = *velocity,
                FieldValue::Nudge(nudge) => target.nudge = (*nudge).clamp(-MAX_NUDGE, MAX_NUDGE),
                FieldValue::Gate(gate) => target.gate = *gate,
                FieldValue::Slot(slot, fx) => {
                    *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? = snapped(*fx);
                }
            }
        }
        Action::Shape {
            track,
            phrase,
            range,
            column,
            shape,
        } => {
            let found = track_mut(&mut next, *track)?
                .phrases
                .get(phrase)
                .cloned()
                .ok_or(EditError::NoPhrase(*phrase))?;
            // Once, as plain notes, a phrase may copy its own loop.
            let source = followed(&next, shape)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            for (row, step) in written_from(found.steps(), *range, *column, shape, source) {
                *step_mut(&mut next, *track, *phrase, row)? = step;
            }
        }
        Action::Generate {
            track,
            phrase,
            column,
            span,
            shape,
        } => {
            let source = followed_live(&next, shape, *track, *phrase)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            let generator = Generator::new(*column, *span, *shape);
            if !phrase_mut(&mut next, *track, *phrase)?.add_generator(generator, source) {
                return Err(EditError::NoLane);
            }
        }
        Action::Reshape {
            track,
            phrase,
            generator,
            shape,
        } => {
            let source = followed_live(&next, shape, *track, *phrase)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            if !phrase_mut(&mut next, *track, *phrase)?
                .reshape_generator(*generator, *shape, source)
            {
                return Err(EditError::NoGenerator {
                    phrase: *phrase,
                    generator: *generator,
                });
            }
        }
        Action::ApplyGenerator {
            track,
            phrase,
            generator,
        } => {
            phrase_mut(&mut next, *track, *phrase)?
                .apply_generator(*generator)
                .ok_or(EditError::NoGenerator {
                    phrase: *phrase,
                    generator: *generator,
                })?;
        }
        Action::Fill {
            track,
            phrase,
            range,
            fill,
        } => {
            for index in range.first..=range.last {
                let target = step_mut(&mut next, *track, *phrase, index)?;
                let has_note = matches!(target.note, Note::On(_));
                match fill {
                    Fill::Note { note, velocity } => {
                        target.note = *note;
                        if matches!(note, Note::On(_)) {
                            target.velocity.get_or_insert(*velocity);
                        }
                    }
                    Fill::Velocity(velocity) if has_note => target.velocity = *velocity,
                    Fill::Velocity(_) => {}
                    Fill::Gate(gate) => target.gate = *gate,
                    Fill::GateLength(length) if has_note => {
                        if length.is_some() || target.gate.is_some() {
                            target.gate = Some(Gate {
                                length: *length,
                                ..target.gate.unwrap_or_default()
                            });
                        }
                    }
                    Fill::GateLength(_) => {}
                    Fill::Nudge(nudge) if has_note || *nudge == 0 => {
                        target.nudge = (*nudge).clamp(-MAX_NUDGE, MAX_NUDGE);
                    }
                    Fill::Nudge(_) => {}
                    Fill::Slot(slot, value) => {
                        *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? =
                            snapped(*value);
                    }
                    Fill::SlotFine(slot, fine) => {
                        let slot = target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)?;
                        if slot.command.is_some_and(FxCommand::has_fine) {
                            *slot = snapped(FxSlot {
                                value: slot.value & 0xFF00 | u16::from(*fine),
                                ..*slot
                            });
                        }
                    }
                    // Typing coarse digits keeps the fine part, and over an `SLI` by a letter
                    // locks the slice by its number instead.
                    Fill::SlotValue(slot, value) => {
                        let slot = target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)?;
                        if let Some(command) = slot.command {
                            let fine = if command.has_fine() { slot.fine() } else { 0 };
                            *slot = snapped(FxSlot {
                                value: u16::from(*value) << 8 | u16::from(fine),
                                ..*slot
                            });
                        }
                    }
                }
            }
        }
        Action::SetMark {
            track,
            phrase,
            range,
            scope,
            mark,
        } => {
            for index in range.first..=range.last {
                mark_step(step_mut(&mut next, *track, *phrase, index)?, *scope, *mark);
            }
        }
        Action::Clear {
            track,
            phrase,
            range,
            scope,
        } => {
            for index in range.first..=range.last {
                clear_step(step_mut(&mut next, *track, *phrase, index)?, *scope);
            }
        }
        Action::Shift {
            track,
            phrase,
            range,
            scope,
            by,
            kind,
            scale,
        } => {
            // An empty length moves from the track's `Gate`, as that's what it plays at.
            let gate = next
                .tracks
                .get(*track)
                .ok_or(EditError::NoTrack(*track))?
                .gate_length();
            for index in range.first..=range.last {
                shift_step(
                    step_mut(&mut next, *track, *phrase, index)?,
                    *scope,
                    *by,
                    *kind,
                    *scale,
                    gate,
                );
            }
        }
        Action::FitToScale {
            track,
            phrase,
            scale,
        } => {
            let stored = next
                .tracks
                .get(*track)
                .and_then(|state| state.phrases.get(phrase))
                .map_or(0, |found| found.steps().len());
            for index in 0..stored {
                let step = step_mut(&mut next, *track, *phrase, index)?;
                if let Note::On(midi) = step.note {
                    step.note = Note::On(scale.fitted(midi));
                }
            }
        }
        Action::Paste {
            track,
            phrase,
            at,
            group,
            block,
            times,
            merge,
        } => {
            let target = phrase_mut(&mut next, *track, *phrase)?;
            let write = if *merge { merge_step } else { paste_step };
            let total = block.steps.len().saturating_mul(*times);
            target.grow_to(at.saturating_add(total));
            let sources = block.steps.iter().cycle().take(total);
            for (offset, source) in sources.enumerate() {
                let Some(step) = target.step_mut(at + offset) else {
                    break;
                };
                write(step, source, block.scope, *group).map_err(|_| EditError::ColumnMismatch)?;
            }
        }
        Action::Double { track, phrase } => {
            phrase_mut(&mut next, *track, *phrase)?.double();
        }
        Action::SetSkipSize(size) => next.skip = *size,
        Action::SetLimiter(on) => next.limiter = *on,
        Action::SetSongName(name) => {
            if !is_project_name(name) {
                return Err(EditError::SongName);
            }
            next.name.clone_from(name);
        }
        Action::SetSeed(seed) => next.seed = *seed,
        Action::SetLaunch(launch) => next.launch = *launch,
        Action::SetChainLabel {
            track,
            chain,
            label,
        } => {
            let owner = track_mut(&mut next, *track)?;
            let old = owner.chains.get(chain).ok_or(EditError::NoChain(*chain))?;
            let mut labelled = Chain::clone(old);
            labelled.label.clone_from(label);
            owner.chains.insert(*chain, Arc::new(labelled));
        }
        Action::SetGroove(groove) => next.groove = *groove,
        Action::SetTrackGroove { track, groove } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .groove = *groove;
        }
        Action::SetCustomGroove { custom, groove } => {
            if *custom >= CUSTOM_GROOVES {
                return Err(EditError::CustomGroove);
            }
            // The ones before it are made too, on the grid, as they'd have played.
            if next.grooves.len() <= *custom {
                next.grooves.resize(*custom + 1, Groove::default());
            }
            next.grooves[*custom] = *groove;
        }
        Action::SetTrackBase {
            track,
            control,
            value,
        } => {
            let track = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            match control {
                TrackControl::Cutoff => track.cutoff = *value,
                TrackControl::Resonance => track.resonance = *value,
                TrackControl::Pan => track.pan = *value,
                TrackControl::SendA => track.send_a = (*value).min(MAX_LEVEL),
                TrackControl::SendB => track.send_b = (*value).min(MAX_LEVEL),
                TrackControl::Drive => track.drive = *value,
                TrackControl::Punch => track.punch = *value,
                TrackControl::LowCut => track.low_cut = *value,
                TrackControl::HighCut => track.high_cut = *value,
                TrackControl::Pitch => track.pitch = *value,
                TrackControl::VelocityDelta => track.velocity_delta = *value,
                TrackControl::Quantise => track.quantise = *value,
            }
            // At the top, a fine part goes.
            if let Some(id) = control.setting() {
                track.set_fine(id, track.fine(id));
            }
        }
        Action::SetInsert {
            track,
            slot,
            insert,
        } => {
            if let Some(held) = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .inserts
                .get_mut(*slot)
            {
                *held = *insert;
            }
            // An empty slot has nothing to switch out, so what goes in next starts in.
            if *insert == Insert::Empty
                && let Some(bypassed) = next.tracks[*track].bypassed.get_mut(*slot)
            {
                *bypassed = false;
            }
        }
        Action::SetBypass {
            track,
            slot,
            bypassed,
        } => {
            if let Some(held) = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .bypassed
                .get_mut(*slot)
            {
                *held = *bypassed;
            }
        }
        Action::SetMod {
            track,
            slot,
            modulator,
        } => {
            if !modulator.in_range() {
                return Err(EditError::Mod);
            }
            *next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .mods
                .get_mut(*slot)
                .ok_or(EditError::Mod)? = *modulator;
        }
        Action::SetReset { section, reset } => {
            if *reset == Some(0) {
                return Err(EditError::Reset);
            }
            section_mut(&mut next, *section)?.reset = *reset;
        }
        Action::AddSection { at } => next.insert_section(*at),
        Action::CloneSection { at } => {
            let copy = section_mut(&mut next, *at)?.clone();
            next.sections.insert(*at + 1, copy);
        }
        Action::RemoveSection { at } => {
            if next.sections.len() <= 1 {
                return Err(EditError::LastSection);
            }
            section_mut(&mut next, *at)?;
            next.sections.remove(*at);
        }
        Action::SetSectionLength { section, length } => {
            if *length == Some(0) {
                return Err(EditError::Reset);
            }
            section_mut(&mut next, *section)?.length = *length;
        }
        Action::SetSectionRepeat { section, repeat } => {
            section_mut(&mut next, *section)?.repeat = *repeat;
        }
        Action::SetSectionStart { section, start } => {
            section_mut(&mut next, *section)?.start = *start;
        }
        Action::SetSectionChain {
            section,
            track,
            chain,
        } => {
            let owner = track_mut(&mut next, *track)?;
            if let Some(chain) = chain
                && !owner.chains.contains_key(chain)
            {
                owner.add_chain(*chain);
            }
            *section_mut(&mut next, *section)?
                .chains
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))? = *chain;
        }
        Action::SetTrackLevel { track, level } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .level = (*level).min(MAX_LEVEL);
        }
        Action::SetTrackTrim { track, trim } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .trim = *trim;
        }
        Action::SetTrackChoke { track, choke } => {
            check_choke(*choke)?;
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .choke = *choke;
        }
        Action::SetTrackGroup { first, last, group } => {
            if let Some(group) = *group
                && !(1..=MAX_GROUPS).contains(&usize::from(group))
            {
                return Err(EditError::Group(group));
            }
            if first > last || *last >= next.sound_tracks() {
                return Err(EditError::GroupMember);
            }
            for track in &mut next.tracks[*first..=*last] {
                track.group = *group;
            }
        }
        Action::SetTrackGateLength { track, gate_length } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .gate_length = *gate_length;
        }
        Action::SetTrackSpeed { track, speed } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .speed = *speed;
        }
        Action::SetTrackReset { track, reset } => {
            if reset.is_some_and(|steps| !(1..=MAX_TRACK_RESET).contains(&steps)) {
                return Err(EditError::TrackReset);
            }
            track_mut(&mut next, *track)?.reset = *reset;
        }
        Action::SetTrackGlide { track, glide } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .glide = *glide;
        }
        Action::SetTrackSafe { track, on } => track_mut(&mut next, *track)?.safe = *on,
        Action::SetAccumulation {
            track,
            accumulation,
        } => {
            if !accumulation.in_range() {
                return Err(EditError::Accumulation);
            }
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .accumulation = *accumulation;
        }
        Action::SetArpeggio { track, arpeggio } => {
            if !arpeggio.in_range() {
                return Err(EditError::Arpeggio);
            }
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .arpeggio = *arpeggio;
        }
        Action::SetLength {
            track,
            phrase,
            length,
        } => {
            phrase_mut(&mut next, *track, *phrase)?.set_len(*length);
        }
        Action::SlideLoop { track, phrase, by } => {
            phrase_mut(&mut next, *track, *phrase)?.slide(*by);
        }
        Action::SetOrder {
            track,
            phrase,
            order,
        } => {
            phrase_mut(&mut next, *track, *phrase)?.set_order(*order);
        }
        Action::SetLane {
            track,
            phrase,
            slot,
            lane,
        } => {
            phrase_mut(&mut next, *track, *phrase)?.set_lane(*slot, *lane);
        }
        Action::RotateLoop { track, phrase, by } => {
            phrase_mut(&mut next, *track, *phrase)?.rotate(*by);
        }
        Action::SetMilliBpm(millibpm) => {
            if !(MIN_MILLIBPM..=MAX_MILLIBPM).contains(millibpm) {
                return Err(EditError::Tempo);
            }
            next.millibpm = *millibpm;
        }
        Action::AddTracks(count) => {
            if next.sound_tracks() + count > MAX_SOUND_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            for _ in 0..*count {
                next.add_track();
            }
        }
        Action::RemoveLastTrack => {
            if next.sound_tracks() <= 1 {
                return Err(EditError::LastTrack);
            }
            next.remove_last_sound_track();
        }
        Action::SetTrackSample { track, sample } => {
            let owner = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            owner.set_setting(SettingId::SAMPLE, *sample);
            // Choosing a sample means wanting to hear it.
            if *sample != 0 {
                owner.machine = Machine::Sampler;
            }
        }
        Action::SetBankEntry { number, entry } => {
            next.bank.set(*number, entry.clone());
        }
        Action::SetSlices { number, starts } => {
            reslice(&mut next, *number, |_| Ok(starts.clone()))?;
        }
        Action::SplitSlice { number, at } => reslice(&mut next, *number, |points| {
            points.split_starts(*at).ok_or(EditError::Split)
        })?,
        Action::JoinSlices { number, slice } => reslice(&mut next, *number, |points| {
            points
                .join_starts(usize::from(*slice))
                .ok_or(EditError::Join(*slice))
        })?,
        Action::SetTrackName { track, name } => {
            let owner = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            // A fixed track without a name goes back to its letter.
            owner.name = if name.is_empty() {
                owner.kind.letter().to_owned()
            } else {
                name.clone()
            };
        }
        Action::LoadKit(sounds) => {
            if sounds.len() > MAX_SOUND_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            while next.sound_tracks() < sounds.len() {
                next.add_track();
            }
            for (track, sound) in next.tracks.iter_mut().zip(sounds) {
                check_choke(sound.choke)?;
                track.set_setting(SettingId::SAMPLE, sound.sample);
                track.machine = Machine::Sampler;
                track.name = sound.name.clone();
                track.choke = sound.choke;
            }
        }
        Action::SetChainEntry {
            track,
            chain,
            slot,
            entry,
        } => {
            check_transpose(entry)?;
            ensure_phrase(track_mut(&mut next, *track)?, entry.phrase);
            let target = chain_mut(&mut next, *track, *chain)?
                .entries
                .get_mut(*slot)
                .ok_or(EditError::NoRow {
                    chain: *chain,
                    slot: *slot,
                })?;
            *target = *entry;
        }
        Action::InsertChainEntry {
            track,
            chain,
            at,
            entry,
        } => {
            check_transpose(entry)?;
            ensure_phrase(track_mut(&mut next, *track)?, entry.phrase);
            let entries = &mut chain_mut(&mut next, *track, *chain)?.entries;
            let at = (*at).min(entries.len());
            entries.insert(at, *entry);
        }
        Action::RemoveChainEntry { track, chain, at } => {
            let entries = &mut chain_mut(&mut next, *track, *chain)?.entries;
            if entries.len() <= 1 {
                return Err(EditError::LastRow);
            }
            if *at >= entries.len() {
                return Err(EditError::NoRow {
                    chain: *chain,
                    slot: *at,
                });
            }
            entries.remove(*at);
        }
        Action::ClonePhrase {
            track,
            chain,
            slot,
            new,
        } => {
            let entry = chain_mut(&mut next, *track, *chain)?
                .entries
                .get_mut(*slot)
                .ok_or(EditError::NoRow {
                    chain: *chain,
                    slot: *slot,
                })?;
            let old = std::mem::replace(&mut entry.phrase, *new);
            let owner = track_mut(&mut next, *track)?;
            let copy = owner
                .phrases
                .get(&old)
                .cloned()
                .ok_or(EditError::NoPhrase(old))?;
            owner.phrases.insert(*new, copy);
        }
        Action::CloneChain { section, track } => {
            let old = section_mut(&mut next, *section)?
                .chains
                .get(*track)
                .copied()
                .flatten()
                .ok_or(EditError::NoTrack(*track))?;
            let owner = track_mut(&mut next, *track)?;
            let mut copy = Chain::clone(owner.chains.get(&old).ok_or(EditError::NoChain(old))?);
            // A phrase used twice in the chain stays one phrase in the copy.
            let mut made: Vec<(u32, u32)> = Vec::new();
            for entry in &mut copy.entries {
                if let Some((_, new)) = made.iter().find(|(from, _)| *from == entry.phrase) {
                    entry.phrase = *new;
                    continue;
                }
                let phrase = owner
                    .phrases
                    .get(&entry.phrase)
                    .cloned()
                    .ok_or(EditError::NoPhrase(entry.phrase))?;
                let new = lowest_unused(&owner.phrases);
                owner.phrases.insert(new, phrase);
                made.push((entry.phrase, new));
                entry.phrase = new;
            }
            let new = lowest_unused(&owner.chains);
            owner.chains.insert(new, Arc::new(copy));
            next.sections[*section].chains[*track] = Some(new);
        }
        Action::SetTrackChain {
            section,
            track,
            chain,
        } => {
            let owner = track_mut(&mut next, *track)?;
            if !owner.chains.contains_key(chain) {
                owner.add_chain(*chain);
            }
            *section_mut(&mut next, *section)?
                .chains
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))? = Some(*chain);
        }
        Action::SetTrackMachine { track, machine } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .machine = *machine;
        }
        Action::SetTrackSetting {
            track,
            setting,
            value,
        } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .set_setting(*setting, *value);
            let track = &mut next.tracks[*track];
            track.set_fine(*setting, track.fine(*setting));
        }
        Action::SetTrackFine {
            track,
            setting,
            fine,
        } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .set_fine(*setting, *fine);
        }
        Action::SetFilterType { track, filter } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .filter = *filter;
        }
        Action::SetFilterMode { track, mode } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .filter_mode = *mode;
        }
    }
    follow_generators(song, &mut next);
    Ok(Arc::new(next))
}

/// The phrase a `Follow` shape reads, to copy it once, or none for any other shape.
fn followed(song: &Song, shape: &Shape) -> Result<Option<Arc<Phrase>>, EditError> {
    if shape.kind != FillKind::Follow {
        return Ok(None);
    }
    let source = shape.follow.source;
    let found = song
        .source_phrase(source)
        .ok_or(EditError::NoSource(source))?;
    Ok(Some(Arc::clone(found)))
}

/// The phrase a live `Follow` on a phrase reads, refused if it's that phrase or reads it, itself
/// or through what it follows, so no edit goes round a cycle.
fn followed_live(
    song: &Song,
    shape: &Shape,
    track: usize,
    phrase: u32,
) -> Result<Option<Arc<Phrase>>, EditError> {
    let found = followed(song, shape)?;
    let here = Source { track, phrase };
    let source = shape.follow.source;
    if found.is_some() && source == here {
        return Err(EditError::FollowsItself);
    }
    if found.is_some() && reads(song, source, here) {
        return Err(EditError::FollowCycle(source));
    }
    Ok(found)
}

/// Let the live generators of every phrase an edit changed follow it: one whose cells it changed
/// by hand is applied, and one whose notes it changed rerenders, as part of the same edit. Then
/// every `Follow` of a phrase it changed, on whichever track, writes it again, and so on along
/// phrases that follow those, so a source and all its followers are one edit and one undo; a
/// `Follow` whose source has gone, with its track, is applied.
///
/// Every phrase is passed over by pointer to find the ones the edit changed, so the phrases that
/// follow another are gathered in the same pass rather than kept in an index, which every way of
/// changing a song would have to keep in step.
fn follow_generators(before: &Song, after: &mut Song) {
    let mut changed: Vec<Source> = Vec::new();
    let mut followers: Vec<Source> = Vec::new();
    for (index, track) in after.tracks.iter().enumerate() {
        let old = before.tracks.get(index);
        for (id, phrase) in &track.phrases {
            let here = Source {
                track: index,
                phrase: *id,
            };
            let kept = old
                .and_then(|old| old.phrases.get(id))
                .is_some_and(|old| Arc::ptr_eq(old, phrase));
            if !kept {
                changed.push(here);
            }
            if phrase.generators().iter().any(|g| g.source().is_some()) {
                followers.push(here);
            }
        }
    }
    // A track gone takes its phrases with it, which counts as changing them.
    let sound = before.sound_tracks();
    for (index, track) in before.tracks.iter().enumerate().take(sound) {
        for id in track.phrases.keys() {
            let here = Source {
                track: index,
                phrase: *id,
            };
            if after.source_phrase(here).is_none() && !changed.contains(&here) {
                changed.push(here);
            }
        }
    }
    for here in changed.clone() {
        let Some(phrase) = after
            .tracks
            .get(here.track)
            .and_then(|track| track.phrases.get(&here.phrase))
        else {
            continue;
        };
        if phrase.generators().is_empty() {
            continue;
        }
        let sources = sources_of(after, phrase);
        let phrase = after.tracks[here.track]
            .phrases
            .get_mut(&here.phrase)
            .expect("found above");
        Arc::make_mut(phrase).follow_generators(|source| looked_up(&sources, source));
    }
    if followers.is_empty() {
        return;
    }
    // Each follower after the followers it follows, so a change goes all the way along.
    let mut done = vec![false; followers.len()];
    loop {
        let mut moved = false;
        for nth in 0..followers.len() {
            if done[nth] {
                continue;
            }
            let here = followers[nth];
            let reading: Vec<Source> = after.tracks[here.track]
                .phrases
                .get(&here.phrase)
                .map(|phrase| {
                    phrase
                        .generators()
                        .iter()
                        .filter_map(Generator::source)
                        .collect()
                })
                .unwrap_or_default();
            let waiting = followers
                .iter()
                .zip(&done)
                .any(|(other, finished)| !finished && *other != here && reading.contains(other));
            if waiting {
                continue;
            }
            done[nth] = true;
            moved = true;
            if refollow(after, here, &changed) && !changed.contains(&here) {
                changed.push(here);
            }
        }
        if !moved {
            break;
        }
    }
}

/// The phrases a phrase's live `Follow`s read, as they are now.
fn sources_of(song: &Song, phrase: &Phrase) -> Vec<(Source, Arc<Phrase>)> {
    phrase
        .generators()
        .iter()
        .filter_map(Generator::source)
        .filter_map(|source| Some((source, Arc::clone(song.source_phrase(source)?))))
        .collect()
}

fn looked_up(sources: &[(Source, Arc<Phrase>)], source: Source) -> Option<&[Step]> {
    sources
        .iter()
        .find(|(found, _)| *found == source)
        .map(|(_, phrase)| phrase.loop_steps())
}

/// Write a phrase's `Follow`s again where the phrase they follow changed, and apply those whose
/// phrase has gone: true if that changed the phrase.
fn refollow(song: &mut Song, here: Source, changed: &[Source]) -> bool {
    let Some(phrase) = song.tracks[here.track].phrases.get(&here.phrase) else {
        return false;
    };
    let sources = sources_of(song, phrase);
    let due: Vec<(usize, Option<Source>)> = phrase
        .generators()
        .iter()
        .enumerate()
        .filter_map(|(index, generator)| {
            let source = generator.source()?;
            let gone = looked_up(&sources, source).is_none();
            (gone || changed.contains(&source)).then_some((index, (!gone).then_some(source)))
        })
        .collect();
    if due.is_empty() {
        return false;
    }
    let phrase = Arc::make_mut(
        song.tracks[here.track]
            .phrases
            .get_mut(&here.phrase)
            .expect("found above"),
    );
    let mut wrote = false;
    // From the last, so applying one leaves the places of those before it as they were. Applying
    // leaves the steps as they are.
    for (index, source) in due.into_iter().rev() {
        match source.and_then(|source| looked_up(&sources, source)) {
            Some(steps) => wrote |= phrase.refollow(index, steps),
            None => {
                phrase.apply_generator(index);
            }
        }
    }
    wrote
}

fn check_transpose(entry: &ChainEntry) -> Result<(), EditError> {
    if (MIN_TRANSPOSE..=MAX_TRANSPOSE).contains(&entry.transpose) {
        Ok(())
    } else {
        Err(EditError::Transpose)
    }
}

fn ensure_phrase(track: &mut Track, phrase: u32) {
    track
        .phrases
        .entry(phrase)
        .or_insert_with(|| Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
}

fn chain_mut(song: &mut Song, track: usize, chain: u32) -> Result<&mut Chain, EditError> {
    let target = track_mut(song, track)?
        .chains
        .get_mut(&chain)
        .ok_or(EditError::NoChain(chain))?;
    Ok(Arc::make_mut(target))
}

fn phrase_mut(song: &mut Song, track: usize, phrase: u32) -> Result<&mut Phrase, EditError> {
    let target = track_mut(song, track)?
        .phrases
        .get_mut(&phrase)
        .ok_or(EditError::NoPhrase(phrase))?;
    Ok(Arc::make_mut(target))
}

fn step_mut(
    song: &mut Song,
    track: usize,
    phrase: u32,
    step: usize,
) -> Result<&mut Step, EditError> {
    phrase_mut(song, track, phrase)?
        .step_mut(step)
        .ok_or(EditError::NoStep { phrase, step })
}
