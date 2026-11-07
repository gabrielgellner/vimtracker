//! Recorded edits. `edit` is the only path that changes a song.

use std::sync::Arc;

use thiserror::Error;

use crate::constants::{
    DEFAULT_PHRASE_STEPS, MAX_ENVELOPE_MS, MAX_LEVEL, MAX_MILLIBPM, MAX_SWING, MAX_TRACKS,
    MAX_TRANSPOSE, MIN_MILLIBPM, MIN_SWING, MIN_TRANSPOSE,
};
use crate::envelope::Envelope;
use crate::lfo::Lfo;
use crate::scale::Scale;
use crate::selection::{
    Block, Scope, Shift, StepRange, clear_step, mark_step, merge_step, paste_step, shift_step,
};
use crate::shape::{Shape, ShapeColumn, values};
use crate::song::{
    Chain, ChainEntry, FilterType, Launch, Machine, Phrase, Repeat, Section, SectionStart, Song,
    SynthVoice, Track, lowest_unused,
};
use crate::step::{FxCommand, FxSlot, Mark, Note, Step};

/// A new value for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue {
    Velocity(Option<u8>),
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
    Slot(usize, FxSlot),
    SlotValue(usize, u8),
}

/// A sound from a kit going on a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitSound {
    pub sample: String,
    pub name: String,
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
    /// Set how far Insert moves down after a note.
    SetStepSize(u32),
    SetSkipSize(u32),
    SetLimiter(bool),
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
    /// Set the project's swing, in tenths of a percent.
    SetSwing(u16),
    /// Give a track its own swing, or `None` to follow the project.
    SetTrackSwing {
        track: usize,
        swing: Option<u16>,
    },
    /// Set where one of a track's held values starts.
    SetTrackBase {
        track: usize,
        control: TrackControl,
        value: u8,
    },
    /// Replace one of a track's LFOs.
    SetLfo {
        track: usize,
        slot: usize,
        lfo: Lfo,
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
    /// Change the length of a phrase's loop, keeping every step.
    SetLength {
        track: usize,
        phrase: u32,
        length: usize,
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
    /// Set a track's sample, or leave it with none.
    SetTrackSample {
        track: usize,
        sample: Option<String>,
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
    /// Choose which synth a track plays.
    SetTrackSynthVoice {
        track: usize,
        voice: SynthVoice,
    },
    /// Choose a track's filter.
    SetFilterType {
        track: usize,
        filter: FilterType,
    },
    /// Replace one of a track's envelopes.
    SetEnvelope {
        track: usize,
        slot: usize,
        envelope: Envelope,
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
    Delay,
    Reverb,
    Drive,
    LowCut,
    HighCut,
    Harmonics,
    Timbre,
    Morph,
}

/// An FX slot with its value within what its command takes.
fn snapped(slot: FxSlot) -> FxSlot {
    let highest = slot.command.map_or(0xFF, FxCommand::highest);
    FxSlot {
        value: slot.value.min(highest),
        ..slot
    }
    .with_valid_mark()
}

fn section_mut(song: &mut Song, section: usize) -> Result<&mut Section, EditError> {
    song.sections
        .get_mut(section)
        .ok_or(EditError::NoSection(section))
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
    #[error("tempo must be from 20 to 400 BPM")]
    Tempo,
    #[error("a project can have at most {MAX_TRACKS} tracks")]
    TooManyTracks,
    #[error("a project needs at least one track")]
    LastTrack,
    #[error("those columns don't match here")]
    ColumnMismatch,
    #[error("an envelope's times go up to {MAX_ENVELOPE_MS} ms, and its sustain to 100%")]
    Envelope,
    #[error("no chain {0:02X}")]
    NoChain(u32),
    #[error("chain {chain:02X} has no row {slot:02X}")]
    NoRow { chain: u32, slot: usize },
    #[error("a chain needs at least one phrase")]
    LastRow,
    #[error("transpose goes from {MIN_TRANSPOSE} to {MAX_TRANSPOSE}")]
    Transpose,
    #[error("swing must be from 50 to 75")]
    Swing,
    #[error("no track {0}")]
    NoTrack(usize),
    #[error("a reset needs at least 1 step")]
    Reset,
    #[error("an LFO's rate, depth or phase is out of range")]
    Lfo,
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
            for (row, value) in values(found.steps(), *range, *column, shape) {
                column.write(step_mut(&mut next, *track, *phrase, row)?, value);
            }
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
                    Fill::Slot(slot, value) => {
                        *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? =
                            snapped(*value);
                    }
                    Fill::SlotValue(slot, value) => {
                        let slot = target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)?;
                        if slot.command.is_some() {
                            *slot = snapped(FxSlot {
                                value: *value,
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
            for index in range.first..=range.last {
                shift_step(
                    step_mut(&mut next, *track, *phrase, index)?,
                    *scope,
                    *by,
                    *kind,
                    *scale,
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
        Action::SetStepSize(size) => next.step = *size,
        Action::SetSkipSize(size) => next.skip = *size,
        Action::SetLimiter(on) => next.limiter = *on,
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
        Action::SetSwing(swing) => {
            if !(MIN_SWING..=MAX_SWING).contains(swing) {
                return Err(EditError::Swing);
            }
            next.swing = *swing;
        }
        Action::SetTrackSwing { track, swing } => {
            if swing.is_some_and(|swing| !(MIN_SWING..=MAX_SWING).contains(&swing)) {
                return Err(EditError::Swing);
            }
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .swing = *swing;
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
                TrackControl::Delay => track.delay = (*value).min(MAX_LEVEL),
                TrackControl::Reverb => track.reverb = (*value).min(MAX_LEVEL),
                TrackControl::Drive => track.drive = *value,
                TrackControl::LowCut => track.low_cut = *value,
                TrackControl::HighCut => track.high_cut = *value,
                TrackControl::Harmonics => track.harmonics = *value,
                TrackControl::Timbre => track.timbre = *value,
                TrackControl::Morph => track.morph = *value,
            }
        }
        Action::SetLfo { track, slot, lfo } => {
            if !lfo.in_range() {
                return Err(EditError::Lfo);
            }
            *next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .lfos
                .get_mut(*slot)
                .ok_or(EditError::Lfo)? = *lfo;
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
            if next.tracks.len() + count > MAX_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            for _ in 0..*count {
                next.add_track();
            }
        }
        Action::RemoveLastTrack => {
            if next.tracks.len() <= 1 {
                return Err(EditError::LastTrack);
            }
            next.tracks.pop();
            for section in &mut next.sections {
                section.chains.pop();
            }
        }
        Action::SetTrackSample { track, sample } => {
            let owner = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            owner.sample = sample.clone();
            // Choosing a sample means wanting to hear it.
            if sample.is_some() {
                owner.machine = Machine::Sample;
            }
        }
        Action::SetTrackName { track, name } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .name = name.clone();
        }
        Action::LoadKit(sounds) => {
            if sounds.len() > MAX_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            while next.tracks.len() < sounds.len() {
                next.add_track();
            }
            for (track, sound) in next.tracks.iter_mut().zip(sounds) {
                track.sample = Some(sound.sample.clone());
                track.machine = Machine::Sample;
                track.name = sound.name.clone();
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
        Action::SetTrackSynthVoice { track, voice } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .synth_voice = *voice;
        }
        Action::SetFilterType { track, filter } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .filter = *filter;
        }
        Action::SetEnvelope {
            track,
            slot,
            envelope,
        } => {
            if !envelope.in_range() {
                return Err(EditError::Envelope);
            }
            *next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .envelopes
                .get_mut(*slot)
                .ok_or(EditError::Envelope)? = *envelope;
        }
    }
    Ok(Arc::new(next))
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
