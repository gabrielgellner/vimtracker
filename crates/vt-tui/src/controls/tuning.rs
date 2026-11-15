//! The Sampler device's `Tune` and `Cents`: the sample's own tuning, in its bank entry, which the
//! sample page's settings row and `:sample tune` set too, shown and changed from the track that
//! plays it, as an MPC's pad `Tune` or Simpler's `Transpose` and `Detune` are on the sound. The
//! note then plays relative to it, and the track's `Pitch` transposes on top.
//!
//! `Tune` is the slice's own where the slice the track's `Slice` is on has one, as `:slice tune`
//! gives it, and the sample's otherwise, so tuning a sample once retunes every slice that plays as
//! it. A slice has no cents of its own, so `Cents` is always the sample's. Neither is a track's
//! setting, so a step can't lock them: `PTCH` is the transpose a step locks. Each change is one
//! undo step on the bank entry.

use vt_model::bank::{BankEntry, MAX_CENTS};
use vt_model::kit::MAX_TUNE;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song, Track};

use super::Control;

/// Whose tune `Tune` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuneOf {
    /// The sample's own, which every slice without its own plays with.
    Sample,
    /// The slice's own, by its number, which it plays in place of the sample's.
    Slice(usize),
}

/// The tuning a sampler track's `Tune` and `Cents` show and change: the bank entry it plays, by
/// its number, and whose tune `Tune` is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tuning<'a> {
    pub number: u8,
    pub entry: &'a BankEntry,
    pub of: TuneOf,
}

/// The tuning a track's `Tune` and `Cents` are of, or none for a track that isn't a sampler or
/// plays no sample, whose two are dim.
pub(crate) fn tuning<'a>(song: &'a Song, track: &Track) -> Option<Tuning<'a>> {
    if !track.is_sound() || track.machine != Machine::Sampler {
        return None;
    }
    let number = track.sample();
    let entry = song.bank.get(number)?;
    let slices = &entry.edit.points.slices;
    // Past the last slice it plays the last, as the engine does.
    let slice = usize::from(track.setting(SettingId::SLICE)).min(slices.len().saturating_sub(1));
    let of = match slices.get(slice) {
        Some(own) if own.over.tune.is_some() => TuneOf::Slice(slice),
        _ => TuneOf::Sample,
    };
    Some(Tuning { number, entry, of })
}

/// The lowest and highest `Tune` or `Cents` goes, either side of none.
pub(crate) fn tuning_most(control: Control) -> i8 {
    if control == Control::Cents {
        MAX_CENTS
    } else {
        MAX_TUNE
    }
}

/// How far `=` and `+` move it: a semitone and an octave, or a cent and ten.
pub(crate) fn steps(control: Control) -> (i32, i32) {
    if control == Control::Cents {
        (1, 10)
    } else {
        (1, 12)
    }
}

/// A value of `Tune` or `Cents` as the device and the status line say it, as `+7 st` or `-20 ct`.
pub(crate) fn tuning_text(control: Control, value: i8) -> String {
    let units = if control == Control::Cents {
        "ct"
    } else {
        "st"
    };
    format!("{value:+} {units}")
}

impl Tuning<'_> {
    /// `Tune` in semitones, the slice's own or the sample's, or `Cents`.
    pub(crate) fn value(&self, control: Control) -> i8 {
        let play = self.entry.edit.play;
        match (control, self.of) {
            (Control::Tune, TuneOf::Slice(slice)) => self.entry.edit.points.slices[slice]
                .over
                .tune
                .unwrap_or(play.tune),
            (Control::Tune, TuneOf::Sample) => play.tune,
            _ => play.cents,
        }
    }

    /// Whether it's left as a new sample has it, so the device shows it dim: the sample's own at
    /// none. A slice's own is never left alone, as it has one.
    pub(crate) fn at_rest(&self, control: Control) -> bool {
        self.value(control) == 0 && (control == Control::Cents || self.of == TuneOf::Sample)
    }

    /// Whether the sample has slices, so the device says whose `Tune` is.
    pub(crate) fn sliced(&self) -> bool {
        !self.entry.edit.points.slices.is_empty()
    }

    /// The entry with `Tune` or `Cents` set, or with none, a slice's tune back to the sample's and
    /// the sample's to none.
    pub(crate) fn with(&self, control: Control, value: Option<i8>) -> BankEntry {
        let mut entry = self.entry.clone();
        let limit = tuning_most(control);
        let value = value.map(|value| value.clamp(-limit, limit));
        match (control, self.of) {
            (Control::Tune, TuneOf::Slice(slice)) => {
                entry.edit.points.slices[slice].over.tune = value;
            }
            (Control::Tune, TuneOf::Sample) => entry.edit.play.tune = value.unwrap_or(0),
            _ => entry.edit.play.cents = value.unwrap_or(0),
        }
        entry
    }

    /// Whose it is, for the status line: `Slice 03` or the sample's name.
    pub(crate) fn whose(&self, control: Control) -> String {
        match (control, self.of) {
            (Control::Tune, TuneOf::Slice(slice)) => format!("Slice {slice:02X}"),
            _ => self.entry.title(),
        }
    }
}

/// What the status line says of `Tune` or `Cents` on a track: its value and whose it is.
pub(crate) fn tuning_shown(song: &Song, track: &Track, control: Control) -> String {
    let Some(tuning) = tuning(song, track) else {
        return "no sample: K on Sample chooses one".to_owned();
    };
    let text = tuning_text(control, tuning.value(control));
    match (control, tuning.of) {
        (Control::Tune, TuneOf::Slice(slice)) => {
            format!("{text}, slice {slice:02X}'s own, in place of the sample's")
        }
        (Control::Tune, TuneOf::Sample) if tuning.sliced() => {
            format!("{text}, the sample's, for its slices without their own")
        }
        _ => format!("{text}, the sample's"),
    }
}
