//! Recorded edits. `edit` is the only path that changes a song.

use std::sync::Arc;

use thiserror::Error;

use crate::constants::{
    DEFAULT_PHRASE_STEPS, MAX_ENVELOPE_MS, MAX_LEVEL, MAX_MILLIBPM, MAX_SWING, MAX_TRACKS,
    MAX_TRANSPOSE, MIN_MILLIBPM, MIN_SWING, MIN_TRANSPOSE,
};
use crate::lfo::Lfo;
use crate::selection::{
    Block, Scope, Shift, StepRange, clear_step, merge_step, paste_step, shift_step,
};
use crate::song::{Chain, ChainEntry, Instrument, Phrase, Song, lowest_unused};
use crate::step::{FxCommand, FxSlot, Note, Step};

/// A new value for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue {
    Velocity(Option<u8>),
    Instrument(Option<u32>),
    Slot(usize, FxSlot),
}

/// A value written across a selection. A velocity or instrument means nothing without a note, and
/// an FX value nothing without a command, so those only go on steps that have one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// A note, with an instrument and velocity for steps that don't have them.
    Note {
        note: Note,
        instrument: u32,
        velocity: u8,
    },
    Velocity(Option<u8>),
    Instrument(Option<u32>),
    Slot(usize, FxSlot),
    SlotValue(usize, u8),
}

/// A sample going into a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSample {
    pub id: u32,
    pub sample: String,
    pub labelled: bool,
}

/// One edit, as plain data so it can be replayed and tested without a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write a note, and an instrument if the note plays.
    SetNote {
        phrase: u32,
        step: usize,
        note: Note,
        instrument: Option<u32>,
    },
    /// Replace a whole step.
    SetStep {
        phrase: u32,
        step: usize,
        value: Step,
    },
    /// Set one field of a step.
    SetField {
        phrase: u32,
        step: usize,
        value: FieldValue,
    },
    /// Write one value across steps.
    Fill {
        phrase: u32,
        range: StepRange,
        fill: Fill,
    },
    /// Clear fields across steps.
    Clear {
        phrase: u32,
        range: StepRange,
        scope: Scope,
    },
    /// Move values across steps.
    Shift {
        phrase: u32,
        range: StepRange,
        scope: Scope,
        by: i32,
        kind: Shift,
    },
    /// Write a copied block some number of times end to end. Past the last stored step this stores
    /// more without changing the loop. Merging leaves steps alone where the copy is empty, and
    /// otherwise the copy overwrites them.
    Paste {
        phrase: u32,
        at: usize,
        group: usize,
        block: Block,
        times: usize,
        merge: bool,
    },
    /// Double a phrase's loop by repeating it.
    Double {
        phrase: u32,
    },
    /// Set how far Insert moves down after a note.
    SetStepSize(u32),
    SetSkipSize(u32),
    SetLimiter(bool),
    /// Set the project's random seed.
    SetSeed(u64),
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
    SetReset(Option<usize>),
    /// Set a track's level.
    SetTrackLevel {
        track: usize,
        level: u8,
    },
    /// Change the length of a phrase's loop, keeping every step.
    SetLength {
        phrase: u32,
        length: usize,
    },
    /// Move a phrase's loop by some steps, keeping its length.
    SlideLoop {
        phrase: u32,
        by: isize,
    },
    /// Move a phrase's loop steps later or earlier, wrapping round within the loop.
    RotateLoop {
        phrase: u32,
        by: isize,
    },
    /// Change the tempo, in thousandths of a BPM.
    SetMilliBpm(u32),
    /// Add tracks at the end, each with its own new chain and phrase.
    AddTracks(usize),
    /// Remove the last track, keeping at least one. Its chain and phrase stay for reuse.
    RemoveLastTrack,
    /// Set an instrument's sample, creating the instrument if needed.
    SetSample {
        instrument: u32,
        sample: String,
    },
    /// Set several instruments' samples as one change, creating them if needed. A slot marked
    /// as labelled becomes so, and others stay as they were.
    SetSamples(Vec<SlotSample>),
    /// Leave an instrument with no sample.
    ClearSample {
        instrument: u32,
    },
    /// Mark whether a slot's name is a label, creating the instrument if needed.
    SetLabelled {
        id: u32,
        labelled: bool,
    },
    /// Set an instrument's setting, creating the instrument if needed.
    SetInstrument {
        id: u32,
        field: InstrumentField,
        value: u16,
    },
    /// Set a step's instrument to a new one.
    AssignNewInstrument {
        phrase: u32,
        step: usize,
        id: u32,
    },
    /// Change a chain row. A phrase that doesn't exist yet is created.
    SetChainEntry {
        chain: u32,
        slot: usize,
        entry: ChainEntry,
    },
    /// Add a chain row. A phrase that doesn't exist yet is created.
    InsertChainEntry {
        chain: u32,
        at: usize,
        entry: ChainEntry,
    },
    /// Remove a chain row, keeping at least one. Its phrase stays for reuse.
    RemoveChainEntry {
        chain: u32,
        at: usize,
    },
    /// Copy the phrase in a chain row to a new ID, and point the row at the copy.
    ClonePhrase {
        chain: u32,
        slot: usize,
        new: u32,
    },
    /// Copy a track's chain and its phrases to new IDs, and point the track at the copy.
    CloneChain {
        track: usize,
    },
    /// Point a track at a chain. A chain that doesn't exist yet gets a new phrase.
    SetTrackChain {
        track: usize,
        chain: u32,
    },
}

/// An instrument setting with a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentField {
    Volume,
    Attack,
    Release,
}

/// One of the values a track holds, set from its page rather than a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackControl {
    Cutoff,
    Resonance,
    Pan,
    Delay,
    Reverb,
}

/// An FX slot with its value within what its command takes.
fn snapped(slot: FxSlot) -> FxSlot {
    let highest = slot.command.map_or(0xFF, FxCommand::highest);
    FxSlot {
        value: slot.value.min(highest),
        ..slot
    }
}

/// Why an edit was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
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
    #[error("volume goes up to FF")]
    Volume,
    #[error("attack and release go up to {MAX_ENVELOPE_MS} ms")]
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
    let mut next = Song::clone(song);
    match action {
        Action::SetNote {
            phrase,
            step,
            note,
            instrument,
        } => {
            let target = step_mut(&mut next, *phrase, *step)?;
            target.note = *note;
            if instrument.is_some() {
                target.instrument = *instrument;
            }
        }
        Action::SetStep {
            phrase,
            step,
            value,
        } => {
            *step_mut(&mut next, *phrase, *step)? = value.clone();
        }
        Action::SetField {
            phrase,
            step,
            value,
        } => {
            let target = step_mut(&mut next, *phrase, *step)?;
            match value {
                FieldValue::Velocity(velocity) => target.velocity = *velocity,
                FieldValue::Instrument(instrument) => target.instrument = *instrument,
                FieldValue::Slot(slot, fx) => {
                    *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? = snapped(*fx);
                }
            }
        }
        Action::Fill {
            phrase,
            range,
            fill,
        } => {
            for index in range.first..=range.last {
                let target = step_mut(&mut next, *phrase, index)?;
                let has_note = matches!(target.note, Note::On(_));
                match fill {
                    Fill::Note {
                        note,
                        instrument,
                        velocity,
                    } => {
                        target.note = *note;
                        if matches!(note, Note::On(_)) {
                            target.instrument.get_or_insert(*instrument);
                            target.velocity.get_or_insert(*velocity);
                        }
                    }
                    Fill::Velocity(velocity) if has_note => target.velocity = *velocity,
                    Fill::Instrument(instrument) if has_note => target.instrument = *instrument,
                    Fill::Velocity(_) | Fill::Instrument(_) => {}
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
        Action::Clear {
            phrase,
            range,
            scope,
        } => {
            for index in range.first..=range.last {
                clear_step(step_mut(&mut next, *phrase, index)?, *scope);
            }
        }
        Action::Shift {
            phrase,
            range,
            scope,
            by,
            kind,
        } => {
            for index in range.first..=range.last {
                shift_step(step_mut(&mut next, *phrase, index)?, *scope, *by, *kind);
            }
        }
        Action::Paste {
            phrase,
            at,
            group,
            block,
            times,
            merge,
        } => {
            let target = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            let target = Arc::make_mut(target);
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
        Action::Double { phrase } => {
            let target = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            Arc::make_mut(target).double();
        }
        Action::SetStepSize(size) => next.step = *size,
        Action::SetSkipSize(size) => next.skip = *size,
        Action::SetLimiter(on) => next.limiter = *on,
        Action::SetSeed(seed) => next.seed = *seed,
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
        Action::SetReset(reset) => {
            if *reset == Some(0) {
                return Err(EditError::Reset);
            }
            for section in &mut next.sections {
                section.reset = *reset;
            }
        }
        Action::SetTrackLevel { track, level } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .level = (*level).min(MAX_LEVEL);
        }
        Action::SetLength { phrase, length } => {
            let phrase = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            Arc::make_mut(phrase).set_len(*length);
        }
        Action::SlideLoop { phrase, by } => {
            let phrase = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            Arc::make_mut(phrase).slide(*by);
        }
        Action::RotateLoop { phrase, by } => {
            let phrase = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            Arc::make_mut(phrase).rotate(*by);
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
        Action::SetSample { instrument, sample } => {
            let entry = next
                .instruments
                .entry(*instrument)
                .or_insert_with(|| Arc::new(Instrument::default()));
            Arc::make_mut(entry).sample = Some(sample.clone());
        }
        Action::ClearSample { instrument } => {
            if let Some(entry) = next.instruments.get_mut(instrument) {
                Arc::make_mut(entry).sample = None;
            }
        }
        Action::SetSamples(samples) => {
            for slot in samples {
                let entry = next
                    .instruments
                    .entry(slot.id)
                    .or_insert_with(|| Arc::new(Instrument::default()));
                let instrument = Arc::make_mut(entry);
                instrument.sample = Some(slot.sample.clone());
                instrument.labelled |= slot.labelled;
            }
        }
        Action::SetLabelled { id, labelled } => {
            let entry = next
                .instruments
                .entry(*id)
                .or_insert_with(|| Arc::new(Instrument::default()));
            Arc::make_mut(entry).labelled = *labelled;
        }
        Action::AssignNewInstrument { phrase, step, id } => {
            step_mut(&mut next, *phrase, *step)?.instrument = Some(*id);
            next.instruments
                .entry(*id)
                .or_insert_with(|| Arc::new(Instrument::default()));
        }
        Action::SetChainEntry { chain, slot, entry } => {
            check_transpose(entry)?;
            ensure_phrase(&mut next, entry.phrase);
            let target =
                chain_mut(&mut next, *chain)?
                    .entries
                    .get_mut(*slot)
                    .ok_or(EditError::NoRow {
                        chain: *chain,
                        slot: *slot,
                    })?;
            *target = *entry;
        }
        Action::InsertChainEntry { chain, at, entry } => {
            check_transpose(entry)?;
            ensure_phrase(&mut next, entry.phrase);
            let entries = &mut chain_mut(&mut next, *chain)?.entries;
            let at = (*at).min(entries.len());
            entries.insert(at, *entry);
        }
        Action::RemoveChainEntry { chain, at } => {
            let entries = &mut chain_mut(&mut next, *chain)?.entries;
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
        Action::ClonePhrase { chain, slot, new } => {
            let entry =
                chain_mut(&mut next, *chain)?
                    .entries
                    .get_mut(*slot)
                    .ok_or(EditError::NoRow {
                        chain: *chain,
                        slot: *slot,
                    })?;
            let old = std::mem::replace(&mut entry.phrase, *new);
            let copy = next
                .phrases
                .get(&old)
                .cloned()
                .ok_or(EditError::NoPhrase(old))?;
            next.phrases.insert(*new, copy);
        }
        Action::CloneChain { track } => {
            let section = next.sections.first().ok_or(EditError::NoTrack(*track))?;
            let old = section
                .chains
                .get(*track)
                .copied()
                .flatten()
                .ok_or(EditError::NoTrack(*track))?;
            let mut copy = Chain::clone(next.chains.get(&old).ok_or(EditError::NoChain(old))?);
            // A phrase used twice in the chain stays one phrase in the copy.
            let mut made: Vec<(u32, u32)> = Vec::new();
            for entry in &mut copy.entries {
                if let Some((_, new)) = made.iter().find(|(from, _)| *from == entry.phrase) {
                    entry.phrase = *new;
                    continue;
                }
                let phrase = next
                    .phrases
                    .get(&entry.phrase)
                    .cloned()
                    .ok_or(EditError::NoPhrase(entry.phrase))?;
                let new = lowest_unused(&next.phrases);
                next.phrases.insert(new, phrase);
                made.push((entry.phrase, new));
                entry.phrase = new;
            }
            let new = lowest_unused(&next.chains);
            next.chains.insert(new, Arc::new(copy));
            next.sections[0].chains[*track] = Some(new);
        }
        Action::SetTrackChain { track, chain } => {
            if !next.chains.contains_key(chain) {
                let phrase = lowest_unused(&next.phrases);
                ensure_phrase(&mut next, phrase);
                next.chains.insert(
                    *chain,
                    Arc::new(Chain {
                        entries: vec![ChainEntry {
                            phrase,
                            transpose: 0,
                        }],
                    }),
                );
            }
            let slot = next
                .sections
                .first_mut()
                .and_then(|section| section.chains.get_mut(*track))
                .ok_or(EditError::NoTrack(*track))?;
            *slot = Some(*chain);
        }
        Action::SetInstrument { id, field, value } => {
            let entry = next
                .instruments
                .entry(*id)
                .or_insert_with(|| Arc::new(Instrument::default()));
            let instrument = Arc::make_mut(entry);
            match field {
                InstrumentField::Volume => {
                    let volume = u8::try_from(*value).map_err(|_| EditError::Volume)?;
                    instrument.volume = volume.min(MAX_LEVEL);
                }
                InstrumentField::Attack | InstrumentField::Release => {
                    if *value > MAX_ENVELOPE_MS {
                        return Err(EditError::Envelope);
                    }
                    if *field == InstrumentField::Attack {
                        instrument.attack_ms = *value;
                    } else {
                        instrument.release_ms = *value;
                    }
                }
            }
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

fn ensure_phrase(song: &mut Song, phrase: u32) {
    song.phrases
        .entry(phrase)
        .or_insert_with(|| Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
}

fn chain_mut(song: &mut Song, chain: u32) -> Result<&mut Chain, EditError> {
    let target = song
        .chains
        .get_mut(&chain)
        .ok_or(EditError::NoChain(chain))?;
    Ok(Arc::make_mut(target))
}

fn step_mut(song: &mut Song, phrase: u32, step: usize) -> Result<&mut Step, EditError> {
    let target = song
        .phrases
        .get_mut(&phrase)
        .ok_or(EditError::NoPhrase(phrase))?;
    Arc::make_mut(target)
        .step_mut(step)
        .ok_or(EditError::NoStep { phrase, step })
}
