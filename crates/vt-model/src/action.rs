//! Recorded edits. `edit` is the only path that changes a song.

use std::sync::Arc;

use thiserror::Error;

use crate::constants::{MAX_MILLIBPM, MAX_TRACKS, MIN_MILLIBPM};
use crate::song::{Instrument, Song};
use crate::step::{Note, Step};

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
    /// Change a phrase's length, keeping steps past it hidden.
    SetLength { phrase: u32, length: usize },
    /// Change the tempo, in thousandths of a BPM.
    SetMilliBpm(u32),
    /// Add tracks at the end, each with its own new chain and phrase.
    AddTracks(usize),
    /// Remove the last track, keeping at least one. Its chain and phrase stay for reuse.
    RemoveLastTrack,
    /// Set an instrument's sample, creating the instrument if needed.
    SetSample { instrument: u32, sample: String },
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
        Action::SetLength { phrase, length } => {
            let phrase = next
                .phrases
                .get_mut(phrase)
                .ok_or(EditError::NoPhrase(*phrase))?;
            Arc::make_mut(phrase).set_len(*length);
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
    }
    Ok(Arc::new(next))
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
