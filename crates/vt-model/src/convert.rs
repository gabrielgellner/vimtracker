//! A one-off conversion of a project saved in format 10, whose nudges were 24ths of their track's
//! step, to format 11, where a nudge is a 24th of a sixteenth on every track (decision 299). It's
//! never part of loading a song: `vimtracker convert-nudges` runs it on a project folder, keeping
//! the old file beside the new one.
//!
//! On a track at a sixteenth nothing changes. On any other, each nudge is multiplied by the
//! track's step in sixteenths, half at `1/32`, two at `1/8` and four at `1/4`, rounded to the
//! nearest and kept to the track's new range, so it lands where it did. That's the rows' nudges,
//! what live generators keep under their rows, and a live Nudge generator's ends, list and step.
//! A row under a `RATE` lock is converted at its track's own speed, not the lock's.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

use crate::constants::TICKS_PER_STEP;
use crate::project::{ProjectError, save_song};
use crate::shape::{Shape, ShapeColumn, Values};
use crate::song::Song;
use crate::speed::Speed;
use crate::step::Step;

/// The format this converts from.
pub const FROM_FORMAT: u32 = 10;
/// What a Nudge column held at the old format: `B` either way of its middle.
const OLD_MOST: i8 = 11;

/// Why a project could not be converted.
#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} uses format {found}, and only format {FROM_FORMAT} songs are converted")]
    Version { path: String, found: u32 },
    #[error("{path}: {detail}")]
    Parse { path: String, detail: String },
    #[error("{0} is already there, from an earlier conversion")]
    Backup(String),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

/// What a conversion changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Converted {
    /// Nudges written on rows, or kept under generators, given new values.
    pub nudges: usize,
    /// Of those, how many went past their track's new range and were kept to it: only a `B` on a
    /// `1/32` track, which was 5.5 of the new unit.
    pub clamped: usize,
    /// Live Nudge generators whose settings were converted.
    pub generators: usize,
}

#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

/// Convert a project folder's `song.ron`, and its autosave if it has one, writing each old file
/// beside it first as `song.format-10.ron` and `song.autosave.format-10.ron`.
pub fn convert_project(folder: &Path) -> Result<Converted, ConvertError> {
    let mut total = convert_file(folder, "song.ron", "song.format-10.ron", true)?;
    let autosave = convert_file(
        folder,
        "song.autosave.ron",
        "song.autosave.format-10.ron",
        false,
    )?;
    total.nudges += autosave.nudges;
    total.clamped += autosave.clamped;
    total.generators += autosave.generators;
    Ok(total)
}

fn convert_file(
    folder: &Path,
    name: &str,
    backup: &str,
    needed: bool,
) -> Result<Converted, ConvertError> {
    let path = folder.join(name);
    if !needed && !path.is_file() {
        return Ok(Converted::default());
    }
    let shown = path.display().to_string();
    let io = |source| ConvertError::Io {
        path: shown.clone(),
        source,
    };
    let text = fs::read_to_string(&path).map_err(io)?;
    let (song, converted) = match convert_text(&text, &shown) {
        // An autosave from after the change is left as it is.
        Err(ConvertError::Version { .. }) if !needed => return Ok(Converted::default()),
        result => result?,
    };
    let kept = folder.join(backup);
    if kept.exists() {
        return Err(ConvertError::Backup(kept.display().to_string()));
    }
    fs::write(&kept, &text).map_err(io)?;
    if name == "song.ron" {
        save_song(folder, &song)?;
    } else {
        crate::project::write_autosave(folder, &song)?;
    }
    Ok(converted)
}

/// A format 10 song's text, read as format 11 with its nudges converted.
pub fn convert_text(text: &str, path: &str) -> Result<(Song, Converted), ConvertError> {
    let parse = |error: &dyn std::fmt::Display| ConvertError::Parse {
        path: path.to_owned(),
        detail: error.to_string(),
    };
    let probe: VersionProbe = ron::from_str(text).map_err(|e| parse(&e))?;
    if probe.format_version != FROM_FORMAT {
        return Err(ConvertError::Version {
            path: path.to_owned(),
            found: probe.format_version,
        });
    }
    // The format's only other change: a Nudge column says how far its nudges go.
    let text = text
        .replace(
            "column: Nudge,",
            &format!("column: Nudge(most: {OLD_MOST}),"),
        )
        .replace("column:Nudge,", &format!("column:Nudge(most:{OLD_MOST}),"));
    let mut song: Song = ron::from_str(&text).map_err(|e| parse(&e))?;
    song.format_version = crate::constants::FORMAT_VERSION;
    song.ensure_fixed_tracks();
    let converted = convert_song(&mut song);
    Ok((song, converted))
}

/// Give every nudge on a track not at a sixteenth its value in 24ths of a sixteenth, from what
/// it was in 24ths of the track's step.
pub fn convert_song(song: &mut Song) -> Converted {
    let mut converted = Converted::default();
    for track in &mut song.tracks {
        let speed = track.speed;
        if speed == Speed::Sixteenth {
            continue;
        }
        for phrase in track.phrases.values_mut() {
            let phrase = Arc::make_mut(phrase);
            for index in 0..phrase.steps().len() {
                if let Some(step) = phrase.step_mut(index) {
                    convert_step(step, speed, &mut converted);
                }
            }
            for generator in phrase.generators_mut() {
                for step in generator.kept_steps_mut() {
                    convert_step(step, speed, &mut Converted::default());
                }
                if let ShapeColumn::Nudge { most } = generator.column {
                    convert_shape(&mut generator.shape, most, speed);
                    generator.column = ShapeColumn::Nudge {
                        most: speed.most_nudge(),
                    };
                    converted.generators += 1;
                }
            }
        }
    }
    converted
}

/// A nudge in 24ths of a step at this speed, in 24ths of a sixteenth, to the nearest, and whether
/// it had to be kept to the speed's range.
pub fn converted_nudge(nudge: i8, speed: Speed) -> (i8, bool) {
    let exact = f64::from(nudge) * speed.ticks() as f64 / TICKS_PER_STEP as f64;
    let most = speed.most_nudge();
    let near = exact.round() as i64;
    let kept = near.clamp(-i64::from(most), i64::from(most));
    (kept as i8, kept != near)
}

fn convert_step(step: &mut Step, speed: Speed, converted: &mut Converted) {
    if step.nudge == 0 {
        return;
    }
    let (nudge, clamped) = converted_nudge(step.nudge, speed);
    step.nudge = nudge;
    converted.nudges += 1;
    converted.clamped += usize::from(clamped);
}

/// A Nudge generator's ends and list, held from its old middle, held from its new one, and its
/// walk's step in the new unit.
fn convert_shape(shape: &mut Shape, old_most: i8, speed: Speed) {
    let most = speed.most_nudge();
    let held = |value: u8| {
        let old = (i16::from(value) - i16::from(old_most)) as i8;
        (converted_nudge(old, speed).0 + most) as u8
    };
    shape.from = held(shape.from);
    shape.to = held(shape.to);
    let values: Vec<u8> = shape.values.as_slice().iter().map(|v| held(*v)).collect();
    shape.values = Values::new(&values);
    let step = f64::from(shape.walk.step) * speed.ticks() as f64 / TICKS_PER_STEP as f64;
    shape.walk.step = (step.round() as u8).clamp(1, 2 * most as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nudge_keeps_its_time_at_every_speed() {
        let converted: Vec<i8> = Speed::ALL
            .iter()
            .map(|speed| converted_nudge(-6, *speed).0)
            .collect();
        assert_eq!(converted, [-3, -4, -6, -8, -12, -16, -24]);
        assert_eq!(converted_nudge(11, Speed::Quarter), (44, false));
        assert_eq!(converted_nudge(11, Speed::ThirtySecond), (5, true));
        assert_eq!(converted_nudge(-11, Speed::EighthTriplet), (-15, false));
    }
}
