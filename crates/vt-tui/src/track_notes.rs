//! `vimtracker notes`: one track's notes from a saved project, once through its chain, with the
//! song's tempo and the track's machine settings, written as TOML for `tools/compare` to play on
//! every side. A song's line is read from the song itself, so no line has to be copied into the
//! repository to be compared (decision 143).
//!
//! The notes are timed as the sequencer plays them: each row's pulses at the track's speed, a
//! `Single` held its gate's share of the way through its last pulse, a `Multi` sounding on each
//! pulse, a `Hold` tied into a note on the next row as a slide, and a `Rest` or an `OFF` ending
//! what sounds. A track whose rows do more than that, with FX, nudges or a groove other than
//! `Straight`, is refused rather than played differently, as is a missing project or track.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;
use vt_model::constants::{FX_SLOTS, MAX_MILLIBPM, MIN_MILLIBPM, TICKS_PER_QUARTER};
use vt_model::groove::GrooveId;
use vt_model::order::Dealer;
use vt_model::project::{self, ProjectError};
use vt_model::settings::{gate_share, voice_key};
use vt_model::song::{Machine, Song, Track};
use vt_model::step::{ACCENT_VELOCITY, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note};

/// Why a track's notes couldn't be written.
#[derive(Debug, Error)]
pub enum NotesError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("{0}")]
    Track(String),
    #[error("couldn't write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("couldn't write the notes as TOML: {0}")]
    Toml(#[from] toml::ser::Error),
}

/// Which track to read: the first sound track with this name, this machine, or both, or the first
/// sound track if neither is given.
#[derive(Debug, Clone, Default)]
pub struct Choice {
    /// The track's name, as the track page shows it.
    pub track: Option<String>,
    /// Its machine, `sampler` or a synth voice by its key, such as `acid`.
    pub machine: Option<String>,
}

impl Choice {
    fn fits(&self, track: &Track) -> bool {
        let named = self
            .track
            .as_deref()
            .is_none_or(|name| track.name.eq_ignore_ascii_case(name));
        let playing = self
            .machine
            .as_deref()
            .is_none_or(|machine| machine_key(track.machine).eq_ignore_ascii_case(machine));
        named && playing
    }

    /// The choice in words, for when no track fits it.
    fn describe(&self) -> String {
        match (&self.track, &self.machine) {
            (Some(name), Some(machine)) => format!("sound track named {name} on {machine}"),
            (Some(name), None) => format!("sound track named {name}"),
            (None, Some(machine)) => format!("sound track on {machine}"),
            (None, None) => "sound tracks".to_owned(),
        }
    }
}

/// A machine by the key a probe spec's `machine` takes.
fn machine_key(machine: Machine) -> &'static str {
    match machine {
        Machine::Sampler => "sampler",
        Machine::Plaits(voice) => voice_key(voice),
    }
}

/// One track's line, once through its chain.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// The song's name.
    pub song: String,
    /// The track's name.
    pub track: String,
    /// Its machine, as a probe spec's `machine` takes it.
    pub machine: String,
    /// The song's tempo.
    pub bpm: f64,
    /// How long the chain lasts once through, in seconds.
    pub seconds: f64,
    /// Every one of the machine's settings on the track, by the keys songs store, defaults
    /// included.
    pub settings: BTreeMap<String, u8>,
    /// Its notes, in order.
    pub notes: Vec<LineNote>,
}

/// A note of the line, as a probe spec's notes are written.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LineNote {
    /// When it starts, in seconds from the chain's start.
    pub time: f64,
    /// How long it's held, in seconds.
    pub length: f64,
    /// A MIDI note number, where 60 is the program's `C-3`, with the chain's transpose.
    pub note: u8,
    /// Its velocity, from 1 to 127.
    pub velocity: u8,
    /// Whether its velocity accents it, as the program has it.
    pub accent: bool,
    /// Whether it slides into the next note, from a `Hold` before a note on the next row.
    pub slide: bool,
}

/// What `vimtracker notes` was asked for.
#[derive(Debug, Clone)]
pub struct NotesRequest {
    pub project: PathBuf,
    pub out: PathBuf,
    pub choice: Choice,
}

/// `vimtracker notes`: read a project, never writing to it, and write its track's line as TOML.
pub fn run(request: &NotesRequest) -> Result<(), NotesError> {
    let song = project::load_song(&request.project)?;
    let line = line(&song, &request.choice)?;
    write(&line, &request.out)?;
    println!(
        "Wrote {} notes of track {} to {}",
        line.notes.len(),
        line.track,
        request.out.display()
    );
    Ok(())
}

/// Write a line as TOML.
pub fn write(line: &Line, out: &Path) -> Result<(), NotesError> {
    let text = toml::to_string(line)?;
    std::fs::write(out, text).map_err(|source| NotesError::Write {
        path: out.display().to_string(),
        source,
    })
}

/// A note being built: where it ends, or `None` while it's held until something ends it.
struct Sounding {
    note: LineNote,
    end: Option<f64>,
}

impl Sounding {
    /// End it here if it's still sounding, as a new note or an `OFF` does.
    fn end_by(&mut self, time: f64) {
        let end = self.end.map_or(time, |end| end.min(time));
        self.end = Some(end);
    }
}

/// The track the choice names, its chain in the first section that gives it one, played once
/// through.
pub fn line(song: &Song, choice: &Choice) -> Result<Line, NotesError> {
    let refuse = |message: String| NotesError::Track(message);
    let index = (0..song.sound_tracks())
        .find(|index| choice.fits(&song.tracks[*index]))
        .ok_or_else(|| refuse(format!("the song has no {}", choice.describe())))?;
    let track = &song.tracks[index];
    let name = if track.name.is_empty() {
        format!("{}", index + 1)
    } else {
        track.name.clone()
    };
    let chain = song
        .sections
        .iter()
        .find_map(|section| section.chains.get(index).copied().flatten())
        .ok_or_else(|| refuse(format!("track {name} has no chain in any section")))?;
    if song.track_groove(index) != GrooveId::STRAIGHT {
        return Err(refuse(format!(
            "track {name} plays with a groove, and only Straight's timing is read"
        )));
    }
    let bpm = f64::from(song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM)) / 1000.0;
    let step_seconds = track.speed.ticks() as f64 * 60.0 / (TICKS_PER_QUARTER as f64 * bpm);
    let held_share = gate_share(f32::from(track.gate_length()));
    let dealer = Dealer::new(song.seed, index);
    let steps = track.chain_steps(chain);

    let mut notes: Vec<Sounding> = Vec::new();
    // Whether the last row's note was a `Hold`, which ties into a note on the next row.
    let mut tied = false;
    for at in 0..steps {
        let Some(position) = track.chain_position(chain, at, dealer) else {
            continue;
        };
        let Some(step) = track.step_at(position) else {
            continue;
        };
        let first = position.pulse == 0;
        let gate = step.gate_kind();
        let row = position.step;
        if first {
            let phrase = track.phrases.get(&position.phrase);
            if step.fx.iter().any(|slot| slot.command.is_some())
                || phrase.is_some_and(|phrase| (0..FX_SLOTS).any(|n| phrase.lane(n).is_some()))
            {
                return Err(refuse(format!(
                    "track {name} has FX on phrase {:02X}'s row {row:02X}, which aren't read",
                    position.phrase
                )));
            }
            if step.nudge != 0 {
                return Err(refuse(format!(
                    "track {name} nudges phrase {:02X}'s row {row:02X}, which isn't read",
                    position.phrase
                )));
            }
        }
        // What the row's note does on this pulse, as the sequencer has it.
        let event = match (step.note, gate) {
            (Note::On(_), GateKind::Rest) if first => Note::Off,
            (Note::On(_), GateKind::Rest) => Note::Empty,
            (Note::On(midi), GateKind::Multi) => Note::On(midi),
            (note, _) if first => note,
            _ => Note::Empty,
        };
        let time = at as f64 * step_seconds;
        let tie = first && tied && matches!(event, Note::On(_));
        if first {
            tied = gate == GateKind::Hold && matches!(event, Note::On(_));
        }
        match event {
            Note::Empty => {}
            Note::Off => {
                if let Some(last) = notes.last_mut() {
                    last.end_by(time);
                }
            }
            Note::On(midi) => {
                if let Some(last) = notes.last_mut() {
                    last.note.slide = tie;
                    last.end_by(time);
                }
                let note = (i16::from(midi) + i16::from(position.transpose))
                    .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE))
                    as u8;
                let velocity = step.velocity.unwrap_or(MAX_VELOCITY).max(1);
                let share = step
                    .length()
                    .map_or(held_share, |own| gate_share(f32::from(own)));
                // A `Single` closes its share of the way through its row's last pulse, a `Multi`
                // through each pulse, and a `Hold`, or a gate of 100%, holds on to the next note.
                let pulses = match gate {
                    GateKind::Multi => 1,
                    _ => step.pulses(),
                };
                let end = (gate != GateKind::Hold && share < 1.0).then(|| {
                    time + (pulses.saturating_sub(1) as f64 + f64::from(share)) * step_seconds
                });
                notes.push(Sounding {
                    note: LineNote {
                        time,
                        length: 0.0,
                        note,
                        velocity,
                        accent: velocity >= ACCENT_VELOCITY,
                        slide: false,
                    },
                    end,
                });
            }
        }
    }
    let seconds = steps as f64 * step_seconds;
    let notes = notes
        .into_iter()
        .map(|mut sounding| {
            sounding.end_by(seconds);
            let end = sounding.end.unwrap_or(seconds);
            LineNote {
                length: end - sounding.note.time,
                ..sounding.note
            }
        })
        .collect();
    let settings = (0..)
        .map_while(|setting| track.machine_setting(setting))
        .map(|id| (id.key(), track.setting(id)))
        .collect();
    Ok(Line {
        song: song.name.clone(),
        track: name,
        machine: machine_key(track.machine).to_owned(),
        bpm,
        seconds,
        settings,
        notes,
    })
}
