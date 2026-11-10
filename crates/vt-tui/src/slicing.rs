//! Slicing the sample the current track plays, from `:slice`: into so many of the same length, or
//! back into one, and how one slice plays, its gain, pan, tune, reverse and fades. Each is one
//! undo step on the bank entry, which every track playing it hears. And choosing a slice from a
//! list, `K` on a step's `SLI` or the Sound page's `Slice`, and `g d` on a step's `SLI` to see it
//! on the sample page.

use vt_model::action::{Action, FieldValue};
use vt_model::bank::BankEntry;
use vt_model::settings::SettingId;
use vt_model::song::Machine;
use vt_model::step::{FxCommand, FxSlot, Mark, Step};

use crate::app::{App, Field};
use crate::banking::sample_text;
use crate::input::command::SliceChange;
use crate::input::mode::View;
use crate::input::picker::{Item, Pick, Picker};
use crate::views::sample::time;

/// A time in microseconds as frames at a rate, to the nearest.
fn frames(micros: u64, rate: u32) -> u64 {
    ((u128::from(micros) * u128::from(rate) + 500_000) / 1_000_000) as u64
}

/// A slice's pan as it's said: `C` in the middle, or how far to the left or right, as `25 L`.
pub fn pan_text(pan: i8) -> String {
    match pan {
        0 => "C".to_owned(),
        pan if pan < 0 => format!("{} L", pan.unsigned_abs()),
        pan => format!("{pan} R"),
    }
}

/// The bank entry a step plays: the one its own `SMP` locks, or else its track's `Sample`.
pub fn entry_of_step(step: Option<&Step>, own: u8) -> u8 {
    step.and_then(|step| {
        step.fx
            .iter()
            .rev()
            .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)))
    })
    .map_or(own, |slot| slot.coarse())
}

impl App {
    /// The bank entry the current track's sampler plays, before any step chooses another.
    fn track_sample_number(&self) -> u8 {
        self.song()
            .tracks
            .get(self.cursor.track)
            .filter(|track| track.machine == Machine::Sampler)
            .map_or(0, |track| track.sample())
    }

    /// The slice a change to one slice is to: the one the sample page's cursor is in, while it
    /// shows this entry, or else the one the track's `Slice` is on.
    fn slice_here(&self, number: u8, entry: &BankEntry) -> usize {
        let points = &entry.edit.points;
        let shown = self.sample_page.shown.map(|(shown, _)| shown);
        let page = self.view == View::Sample && shown == Some(number);
        let slice = if page {
            points.slice_at(self.sample_page.cursor)
        } else {
            let track = self.song().tracks.get(self.cursor.track);
            usize::from(track.map_or(0, |track| track.setting(SettingId::SLICE)))
        };
        slice.min(points.slices.len().saturating_sub(1))
    }

    /// Slice the current track's sample, or change how one of its slices plays, as one undo step,
    /// and say what it is now.
    pub(crate) fn change_slices(&mut self, change: SliceChange) {
        let number = self.track_sample_number();
        let Some(mut entry) = self.song().bank.get(number).cloned() else {
            self.status = format!(
                "Track {:02} plays no sample to slice: K on Sample chooses one",
                self.cursor.track + 1
            );
            return;
        };
        let Some((rate, length)) = self.file_of(&entry) else {
            self.status = "Its file isn't loaded, so its length isn't known".to_owned();
            return;
        };
        let name = sample_text(self.song(), number);
        match change {
            SliceChange::Even(count) => {
                let points = &entry.edit.points;
                let starts = points.even_starts(count, length);
                let had = points.slices.len();
                let made = starts.len();
                let end = points.end_in(length);
                let long = (end - points.start.min(end)) / made.max(1) as u64;
                self.apply(&Action::SetSlices { number, starts });
                let followed = if had > 0 {
                    ", and SLI locks follow their sound"
                } else {
                    ""
                };
                self.status = format!(
                    "{name} in {made} slices of {}{followed}: u to undo",
                    time(long, rate)
                );
                return;
            }
            SliceChange::Clear if entry.edit.points.slices.is_empty() => {
                self.status = format!("{name} has no slices");
                return;
            }
            SliceChange::Clear => {
                self.apply(&Action::SetSlices {
                    number,
                    starts: Vec::new(),
                });
                self.status = format!("{name} plays as one again: u to undo");
                return;
            }
            _ => {}
        }
        if entry.edit.points.slices.is_empty() {
            self.status = format!("{name} has no slices: :slice 16 cuts it into 16");
            return;
        }
        let at = self.slice_here(number, &entry);
        let over = &mut entry.edit.points.slices[at].over;
        let as_entry = "as the sample".to_owned();
        let said = match change {
            SliceChange::Gain(hundredths) => {
                over.gain_db = hundredths.map(|hundredths| hundredths as f32 / 100.0);
                over.gain_db
                    .map_or(as_entry, |db| format!("at {db:+.1} dB"))
            }
            SliceChange::Pan(pan) => {
                over.pan = pan;
                pan.map_or(as_entry, |pan| format!("panned {}", pan_text(pan)))
            }
            SliceChange::Tune(semitones) => {
                over.tune = semitones;
                semitones.map_or(as_entry, |semitones| format!("tuned {semitones:+} st"))
            }
            SliceChange::Reverse(reverse) => {
                over.reverse = reverse;
                match reverse {
                    None => as_entry,
                    Some(true) => "reversed".to_owned(),
                    Some(false) => "forward".to_owned(),
                }
            }
            SliceChange::Fade(fades) => {
                let fade = |micros| frames(micros, rate).min(length) as u32;
                over.fade_in = fades.map(|(fade_in, _)| fade(fade_in));
                over.fade_out = fades.map(|(_, fade_out)| fade(fade_out));
                match (over.fade_in, over.fade_out) {
                    (Some(fade_in), Some(fade_out)) => format!(
                        "fading in over {} and out over {}",
                        time(u64::from(fade_in), rate),
                        time(u64::from(fade_out), rate)
                    ),
                    _ => as_entry,
                }
            }
            SliceChange::Even(_) | SliceChange::Clear => return,
        };
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = format!("{name}: slice {at:02X} plays {said}");
    }

    /// An entry's slices as a list, each with its number and where it starts, as `03  0.214 s`,
    /// or the entry as one, `00`, if it has none.
    fn slice_items(&self, number: u8, pick: impl Fn(u8) -> Pick) -> Vec<Item> {
        let Some(entry) = self.song().bank.get(number) else {
            return vec![Item::new("00", pick(0)).meaning("No sample to slice")];
        };
        let points = &entry.edit.points;
        let file = self.file_of(entry);
        let length = file.map_or(u64::MAX, |(_, length)| length);
        (0..points.slice_count())
            .map(|at| {
                let frames = points.slice_frames(at, length);
                let label = match file {
                    Some((rate, _)) => format!("{at:02X}  {}", time(frames.start, rate)),
                    None => format!("{at:02X}"),
                };
                let item = Item::new(label, pick(at as u8));
                match file {
                    Some((rate, _)) => {
                        item.meaning(format!("{} long", time(frames.end - frames.start, rate)))
                    }
                    None => item,
                }
            })
            .collect()
    }

    /// Choose the slice the current track's sampler plays, from its sample's slices.
    pub(crate) fn open_slice_picker(&mut self) {
        self.count = None;
        let track = self.cursor.track;
        let number = self.track_sample_number();
        let current = self
            .song()
            .tracks
            .get(track)
            .map_or(0, |track| track.setting(SettingId::SLICE));
        let items = self.slice_items(number, |value| Pick::Setting {
            track,
            setting: SettingId::SLICE,
            value,
        });
        let at = usize::from(current).min(items.len() - 1);
        self.show_picker(Picker::new("Slice", items).opened_on(Some(at)));
    }

    /// The bank entry the step under the cursor plays.
    fn step_entry(&self) -> u8 {
        let step = self
            .phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step));
        entry_of_step(step, self.track_sample_number())
    }

    /// What the status line says of a step's `SLI`: which slice of the sample the step plays,
    /// where it starts and how long it is.
    pub fn slice_shown(&self, slice: u8) -> String {
        let number = self.step_entry();
        let Some(entry) = self.song().bank.get(number) else {
            return format!("{slice:02X}: no sample to slice");
        };
        let points = &entry.edit.points;
        if points.slices.is_empty() {
            return format!(
                "{slice:02X}: {} has no slices, so it plays whole",
                sample_text(self.song(), number)
            );
        }
        let count = points.slices.len();
        let played = usize::from(slice).min(count - 1);
        let past = if played != usize::from(slice) {
            format!(" is past the last, so it plays {played:02X}")
        } else {
            String::new()
        };
        let Some((rate, length)) = self.file_of(entry) else {
            return format!("{slice:02X} of {count}{past}");
        };
        let frames = points.slice_frames(played, length);
        format!(
            "{slice:02X} of {count}{past}: from {}, {} long",
            time(frames.start, rate),
            time(frames.end - frames.start, rate)
        )
    }

    /// What the Sound page says beside a track's `Slice`: how many slices its sample has, and
    /// where the one it plays starts, as `of 16 at 0.484 s`, or `whole` with none.
    pub fn slice_choice(&self, track: &vt_model::song::Track) -> String {
        let Some(entry) = self.song().bank.get(track.sample()) else {
            return "whole".to_owned();
        };
        let points = &entry.edit.points;
        if points.slices.is_empty() {
            return "whole".to_owned();
        }
        let count = points.slices.len();
        let slice = usize::from(track.setting(SettingId::SLICE));
        match self.file_of(entry) {
            Some((rate, length)) => {
                let frames = points.slice_frames(slice, length);
                format!("of {count} at {}", time(frames.start, rate))
            }
            None => format!("of {count}"),
        }
    }

    /// Choose the slice a step's `SLI` locks, from the slices of the sample the step plays.
    pub(crate) fn open_slice_lock_picker(&mut self, slot: usize) {
        self.count = None;
        let current = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)))
            .map_or(0, |cell| cell.coarse());
        let items = self.slice_items(self.step_entry(), |number| Pick::SliceLock { slot, number });
        let at = usize::from(current).min(items.len() - 1);
        self.show_picker(Picker::new("Slice lock", items).opened_on(Some(at)));
    }

    /// Put a chosen slice in a slot as an `SLI`, keeping its mark.
    pub(crate) fn pick_slice_lock(&mut self, slot: usize, number: u8) {
        let mark = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)))
            .map_or(Mark::default(), |cell| cell.mark);
        self.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(
            slot,
            FxSlot {
                command: Some(FxCommand::Set(SettingId::SLICE)),
                value: u16::from(number) << 8,
                mark,
            },
        ));
    }

    /// `g d` on a step's `SLI` value: the sample page on the slice it locks, of the sample the step
    /// plays, as a jump `C-o` comes back from. Whether the cursor was on one.
    pub(crate) fn open_slice_of_step(&mut self) -> bool {
        let Field::FxValue(slot) = self.cursor.field else {
            return false;
        };
        let Some(cell) = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)))
        else {
            return false;
        };
        let number = self.step_entry();
        let track = self.cursor.track;
        self.jump_to_view(View::Sample);
        self.sample_page.pinned = (number != self.track_sample_number()).then_some((track, number));
        match self.sample_page_here() {
            Ok(shown) => {
                let slice = usize::from(cell.coarse());
                let points = &shown.entry.edit.points;
                let frames = points.slice_frames(slice, shown.length());
                self.sample_page.show_frame(frames.start, shown.length());
                let count = points.slice_count();
                self.status = if slice >= count {
                    format!(
                        "Slice {slice:02X} is past the last, {:02X}, so it plays that",
                        count - 1
                    )
                } else {
                    format!("{}: slice {slice:02X}", sample_text(self.song(), number))
                };
            }
            Err(why) => self.status = why,
        }
        true
    }
}
