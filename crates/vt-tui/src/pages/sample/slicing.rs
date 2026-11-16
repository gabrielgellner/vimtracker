//! Slicing the sample the current track plays, from `:slice`: into so many of the same length, or
//! back into one, and how one slice plays, its gain, pan, tune, reverse and fades. Each is one
//! undo step on the bank entry, which every track playing it hears. And the lists a slice is
//! chosen from, by its number or by a letter naming it, which the Sound page's `Slice` and a step's
//! `SLI` open.

use vt_model::action::Action;
use vt_model::analysis::{Analysis, Pyramid};
use vt_model::bank::{BankEntry, MAX_SLICES, SliceParams};
use vt_model::constants::BAR_STEPS;
use vt_model::onsets::{MIN_GAP_SECONDS, MIN_SILENCE_SECONDS, after_silence, quiet_level};
use vt_model::samples::Rates;
use vt_model::settings::SettingId;
use vt_model::song::Machine;
use vt_model::step::{FxCommand, FxSlot, Mark, Step};

use crate::input::mode::View;
use crate::overlays::command::parse::{Address, SliceChange, SliceRange};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::sample::draw::time;
use crate::pages::sample::{SamplePage, Shown, beat_frames};
use crate::services::Services;
use crate::services::banking::sample_text;
use crate::services::sampling::still_recording;

/// A time in microseconds as frames at a rate, to the nearest.
fn frames(micros: u64, rate: u32) -> u64 {
    ((u128::from(micros) * u128::from(rate) + 500_000) / 1_000_000) as u64
}

/// A slice's pan as it's said: `C` in the middle, or how far to the left or right, as `25 L`.
pub(crate) fn pan_text(pan: i8) -> String {
    match pan {
        0 => "C".to_owned(),
        pan if pan < 0 => format!("{} L", pan.unsigned_abs()),
        pan => format!("{pan} R"),
    }
}

/// The bank entry a step plays: the one its own `SMP` locks, or else its track's `Sample`.
pub(crate) fn entry_of_step(step: Option<&Step>, own: u8) -> u8 {
    step.and_then(|step| {
        step.fx
            .iter()
            .rev()
            .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)))
    })
    .map_or(own, |slot| slot.coarse())
}

/// A slice's own setting changed as `:slice` says, and what it plays now, as the status line says
/// it.
fn set_own(over: &mut SliceParams, change: SliceChange, rate: u32, length: u64) -> String {
    let as_entry = "as the sample".to_owned();
    match change {
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
        SliceChange::Even(_)
        | SliceChange::Clear
        | SliceChange::Transients
        | SliceChange::Beat(_)
        | SliceChange::Silence => String::new(),
    }
}

/// Where slices a step of frames apart start in a stretch, from its start. A last one shorter than
/// half a step joins the one before, so a loop a few frames longer than a bar still cuts in 16.
fn beat_starts(within: &std::ops::Range<u64>, step: f64) -> Vec<u64> {
    let step = step.max(1.0);
    let mut starts = Vec::new();
    for at in 0_u64.. {
        let start = within.start + (at as f64 * step).round() as u64;
        let short = ((within.end.saturating_sub(start)) as f64) < step / 2.0;
        if start >= within.end || (at > 0 && short) || starts.len() > MAX_SLICES {
            break;
        }
        starts.push(start);
    }
    starts
}

/// Where the sounds a `Sens` finds start in a stretch, to slice it at, none too near its end. The
/// stretch's own start starts a slice if there's sound before the first, or the first is next to
/// it, so silence before the first hit isn't a slice of its own.
fn transient_starts(analysis: &Analysis, within: &std::ops::Range<u64>, sens: u8) -> Vec<u64> {
    let gap = (MIN_GAP_SECONDS * f64::from(analysis.rate)).round() as u64;
    let onsets: Vec<u64> = analysis
        .onsets(sens)
        .into_iter()
        .filter(|onset| within.contains(onset) && within.end - onset >= gap)
        .collect();
    let Some(&first) = onsets.first() else {
        return Vec::new();
    };
    let mut starts = Vec::with_capacity(onsets.len() + 1);
    if first < within.start + gap {
        starts.push(within.start);
        starts.extend(&onsets[1..]);
        return starts;
    }
    // The whole buckets before the first sound, as it's in the bucket it starts in.
    let size = Pyramid::bucket_frames(0);
    let before = within.start..first / size * size;
    if before.start < before.end && analysis.pyramid.peak(before) > quiet_level(analysis.peak) {
        starts.push(within.start);
    }
    starts.extend(onsets);
    starts
}

impl SamplePage {
    /// The slice a change to one slice is to: the one the page's cursor is in, while it's the
    /// view and shows this entry, or else the one the track's `Slice` is on.
    pub(crate) fn slice_here(
        &self,
        view: View,
        number: u8,
        entry: &BankEntry,
        services: &Services,
    ) -> usize {
        let points = &entry.edit.points;
        let shown = self.shown.map(|(shown, _)| shown);
        let page = view == View::Sample && shown == Some(number);
        let slice = if page {
            points.slice_at(self.cursor)
        } else {
            let track = services.song().tracks.get(services.cursor.track);
            usize::from(track.map_or(0, |track| track.setting(SettingId::SLICE)))
        };
        slice.min(points.slices.len().saturating_sub(1))
    }

    /// `:slice`: slice the current track's sample, or change how its slices play, as one undo step, and say
    /// what it is now. A range re-slices only the slices inside it, or changes those starting in
    /// it, and with none it's the whole sample, or the slice here for a slice's own setting.
    pub(crate) fn change_slices(
        &self,
        change: SliceChange,
        range: Option<SliceRange>,
        view: View,
        recording: Option<(u8, Option<f64>)>,
        services: &mut Services,
    ) {
        let number = services.track_sample_number();
        let Some(entry) = services.song().bank.get(number).cloned() else {
            services.status = format!(
                "Track {:02} plays no sample to slice: K on Sample chooses one",
                services.cursor.track + 1
            );
            return;
        };
        let Some((rate, length)) = services.file_of(&entry) else {
            services.status = "Its file isn't loaded, so its length isn't known".to_owned();
            return;
        };
        let name = sample_text(services.song(), number);
        let points = &entry.edit.points;
        let end = points.end_in(length);
        let whole = points.start.min(end)..end;
        let within = match range {
            None => whole.clone(),
            Some(range) => match self.range_frames(&entry, range, length) {
                Ok(within) => within,
                Err(why) => {
                    services.status = why;
                    return;
                }
            },
        };
        let had = points.slices.len();
        // The status line's words for the range, if it's less than the whole.
        let place = if within == whole {
            String::new()
        } else {
            format!(
                " from {} to {}",
                time(within.start, rate),
                time(within.end, rate)
            )
        };
        let (inside, said) = match change {
            SliceChange::Clear if range.is_none() => {
                if had == 0 {
                    services.status = format!("{name} has no slices");
                } else {
                    services.apply(&Action::SetSlices {
                        number,
                        starts: Vec::new(),
                    });
                    services.status = format!("{name} plays as one again: u to undo");
                }
                return;
            }
            SliceChange::Clear => (vec![within.start], "one slice".to_owned()),
            SliceChange::Even(count) => {
                let count = count.clamp(1, MAX_SLICES) as u128;
                let span = u128::from(within.end - within.start);
                let mut starts: Vec<u64> = (0..count)
                    .map(|at| within.start + (span * at / count) as u64)
                    .collect();
                starts.dedup();
                let long = (within.end - within.start) / starts.len().max(1) as u64;
                let said = format!("{} slices of {}", starts.len(), time(long, rate));
                (starts, said)
            }
            SliceChange::Beat(note) => {
                let steps = entry.edit.hints.length_steps();
                let bar = match steps {
                    Some(steps) => {
                        (whole.end - whole.start) as f64 * BAR_STEPS as f64
                            / f64::from(steps.max(1))
                    }
                    None => 4.0 * beat_frames(rate, services.song().millibpm),
                };
                let starts = beat_starts(&within, bar / f64::from(note.max(1)));
                let by = match steps {
                    Some(steps) => format!(
                        "as it lasts {}",
                        crate::pages::sample::points::steps_text(steps)
                    ),
                    None => "at the song's tempo".to_owned(),
                };
                let said = format!("{} slices of 1/{note}, {by}", starts.len());
                (starts, said)
            }
            SliceChange::Transients | SliceChange::Silence => {
                let Some(analysis) = services.entry_analysis(&entry) else {
                    services.status =
                        format!("{name} can't be analysed: its file is missing or has changed");
                    return;
                };
                let sens = entry.edit.hints.sens;
                let transients = change == SliceChange::Transients;
                let starts = if transients {
                    transient_starts(&analysis, &within, sens)
                } else {
                    Self::silence_starts(&entry, &analysis, &within, recording, services)
                };
                if starts.is_empty() {
                    services.status = if transients {
                        format!(
                            "No sound found in {name}{place} at Sens {sens}%: a higher Sens, \
                             from K on the sample page, finds more"
                        )
                    } else {
                        format!("{name}{place} is all silence")
                    };
                    return;
                }
                let count = starts.len();
                let said = if transients {
                    format!("{count} slices, at its sounds at Sens {sens}%")
                } else {
                    format!("{count} slices, at its sounds after silence")
                };
                (starts, said)
            }
            _ => {
                let here = self.slice_here(view, number, &entry, services);
                let within = range.map(|_| within);
                Self::change_slice_settings(change, within, entry, (rate, length), here, services);
                return;
            }
        };
        let starts = entry.edit.points.restarted(within, &inside, length);
        if starts.len() > MAX_SLICES {
            services.status = format!(
                "That's {} slices, more than {MAX_SLICES}: a range, or a lower Sens, makes fewer",
                starts.len()
            );
            return;
        }
        let now: Vec<u64> = entry
            .edit
            .points
            .slices
            .iter()
            .map(|slice| slice.start)
            .collect();
        if now == starts {
            services.status = format!("{name} is sliced so already");
            return;
        }
        let total = starts.len();
        services.apply(&Action::SetSlices { number, starts });
        let followed = if had > 0 {
            ", and SLI locks follow their sound"
        } else {
            ""
        };
        let all = if place.is_empty() {
            String::new()
        } else {
            format!(", {total} in all")
        };
        services.status = format!("{name}{place} in {said}{all}{followed}: u to undo");
    }

    /// A slice's own setting, for the slice here, or for every slice starting in a stretch of
    /// frames, as one undo step.
    fn change_slice_settings(
        change: SliceChange,
        within: Option<std::ops::Range<u64>>,
        mut entry: BankEntry,
        (rate, length): (u32, u64),
        here: usize,
        services: &mut Services,
    ) {
        let number = services.track_sample_number();
        let name = sample_text(services.song(), number);
        if entry.edit.points.slices.is_empty() {
            services.status = format!("{name} has no slices: :slice 16 cuts it into 16");
            return;
        }
        let slices = &entry.edit.points.slices;
        let targets: Vec<usize> = match &within {
            Some(within) => (0..slices.len())
                .filter(|at| within.contains(&slices[*at].start))
                .collect(),
            None => vec![here],
        };
        let (Some(first), Some(last)) = (targets.first().copied(), targets.last().copied()) else {
            services.status = format!("No slice of {name} starts in that range");
            return;
        };
        let mut said = String::new();
        for at in targets {
            said = set_own(&mut entry.edit.points.slices[at].over, change, rate, length);
        }
        services.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        services.status = if first == last {
            format!("{name}: slice {first:02X} plays {said}")
        } else {
            format!("{name}: slices {first:02X} to {last:02X} play {said}")
        };
    }

    /// The frames a range covers of an entry: from the start of the first address's to the end
    /// of the last's, either way round, inside the entry's start and end, or why it covers none.
    fn range_frames(
        &self,
        entry: &BankEntry,
        range: SliceRange,
        length: u64,
    ) -> Result<std::ops::Range<u64>, String> {
        let points = &entry.edit.points;
        let end = points.end_in(length);
        let start = points.start.min(end);
        let (first, last) = match range {
            SliceRange::Whole => return Ok(start..end),
            SliceRange::Between(first, last) => (first, last),
        };
        let frames = |address: Address| match address {
            Address::Mark(letter) => points
                .marked(letter)
                .map(|slice| points.slice_frames(slice, length))
                .ok_or_else(|| format!("'{letter} names no slice: m{letter} names the slice here")),
            Address::SelectionStart | Address::SelectionEnd => {
                let (first, last) = self
                    .last_selection
                    .ok_or_else(|| "No selection yet: v starts one".to_owned())?;
                let frame = if address == Address::SelectionStart {
                    first
                } else {
                    last
                };
                Ok(frame..frame + 1)
            }
        };
        let (first, last) = (frames(first)?, frames(last)?);
        let from = first.start.min(last.start).clamp(start, end);
        let to = first.end.max(last.end).clamp(start, end);
        if from >= to {
            return Err("That range is outside the sample's start and end".to_owned());
        }
        Ok(from..to)
    }

    /// Where sounds start after silence in a stretch of an entry, each to the frame it rises
    /// above the silence between them.
    fn silence_starts(
        entry: &BankEntry,
        analysis: &Analysis,
        within: &std::ops::Range<u64>,
        recording: Option<(u8, Option<f64>)>,
        services: &Services,
    ) -> Vec<u64> {
        let quiet = quiet_level(analysis.peak);
        let gap = (MIN_SILENCE_SECONDS * f64::from(analysis.rate)).round() as u64;
        let number = services.track_sample_number();
        let shown = still_recording(recording, number)
            .is_none()
            .then(|| Shown::of(&services.samples, number, entry).ok())
            .flatten();
        let size = Pyramid::bucket_frames(0);
        after_silence(&analysis.pyramid, within.clone(), quiet, gap)
            .into_iter()
            .map(|from| {
                let to = ((from / size + 1) * size).min(within.end);
                shown
                    .as_ref()
                    .and_then(|shown| shown.first_above(from..to, quiet))
                    .unwrap_or(from)
            })
            .collect()
    }
}

impl Services {
    /// The slices of the current track's sample, to choose the one its sampler plays.
    pub(crate) fn slice_picker(&self) -> Picker {
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
        Picker::new("Slice", items).opened_on(Some(at))
    }

    /// The mark an `SLI` in a slot has, if it has one, to keep as its value changes.
    pub(crate) fn slice_lock_mark(&self, slot: usize) -> Mark {
        self.current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)))
            .map_or(Mark::default(), |cell| cell.mark)
    }

    /// An entry's slices as a list, each with its number, the letters naming it and where it
    /// starts, as `03  a  0.214 s`, or the entry as one, `00`, if it has none.
    pub(crate) fn slice_items(&self, number: u8, pick: impl Fn(u8) -> Pick) -> Vec<Item> {
        let Some(entry) = self.song().bank.get(number) else {
            return vec![Item::new("00", pick(0)).meaning("No sample to slice")];
        };
        let points = &entry.edit.points;
        let file = self.file_of(entry);
        let length = file.map_or(u64::MAX, |(_, length)| length);
        // The letters line up in a column as wide as the most any slice has, if any has one.
        let letters = |at: usize| points.marks_of(at).into_iter().collect::<String>();
        let widest = (0..points.slices.len())
            .map(|at| letters(at).len())
            .max()
            .unwrap_or(0);
        (0..points.slice_count())
            .map(|at| {
                let frames = points.slice_frames(at, length);
                let named = match widest {
                    0 => String::new(),
                    widest => format!("  {:<widest$}", letters(at)),
                };
                let label = match file {
                    Some((rate, _)) => format!("{at:02X}{named}  {}", time(frames.start, rate)),
                    None => format!("{at:02X}{named}"),
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

    /// Each letter naming a slice of an entry, in the order of their slices, as `'a  03  0.214 s`.
    pub(crate) fn mark_items(&self, number: u8, pick: impl Fn(char) -> Pick) -> Vec<Item> {
        let Some(entry) = self.song().bank.get(number) else {
            return Vec::new();
        };
        let points = &entry.edit.points;
        let file = self.file_of(entry);
        let length = file.map_or(u64::MAX, |(_, length)| length);
        points
            .marks_in_order()
            .into_iter()
            .map(|(letter, at)| {
                let frames = points.slice_frames(at, length);
                let label = match file {
                    Some((rate, _)) => format!("'{letter}  {at:02X}  {}", time(frames.start, rate)),
                    None => format!("'{letter}  {at:02X}"),
                };
                Item::new(label, pick(letter)).meaning(format!("The slice named {letter}"))
            })
            .collect()
    }

    /// The bank entry the current track's sampler plays, before any step chooses another.
    pub(crate) fn track_sample_number(&self) -> u8 {
        self.song()
            .tracks
            .get(self.cursor.track)
            .filter(|track| track.machine == Machine::Sampler)
            .map_or(0, |track| track.sample())
    }

    /// The bank entry the step under the cursor plays.
    pub(crate) fn step_entry(&self) -> u8 {
        let step = self
            .phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step));
        entry_of_step(step, self.track_sample_number())
    }

    /// What the status line says of a step's `SLI`: which slice of the sample the step plays,
    /// where it starts and how long it is, and for one by a letter, which slice the letter names,
    /// or that it names none, so the track's own plays.
    pub(crate) fn slice_shown(&self, cell: FxSlot) -> String {
        match (cell.slice_mark(), self.slice_of_lock(cell)) {
            (Some(letter), Ok(slice)) => format!("'{letter} names {}", self.slice_said(slice)),
            (Some(letter), Err(own)) => format!(
                "'{letter} names no slice of {}, so the track's own plays: {}",
                sample_text(self.song(), self.step_entry()),
                self.slice_said(own)
            ),
            (None, _) => self.slice_said(cell.coarse()),
        }
    }

    /// Which slice of the sample the step under the cursor plays this is, where it starts and how
    /// long it is.
    fn slice_said(&self, slice: u8) -> String {
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
    pub(crate) fn slice_choice(&self, track: &vt_model::song::Track) -> String {
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

    /// The slice an `SLI` plays of the sample the step under the cursor plays, by its number: the
    /// one its letter names, if it's by a letter. A letter naming none plays the track's own
    /// `Slice`, which comes back as an error with it.
    pub(crate) fn slice_of_lock(&self, cell: FxSlot) -> Result<u8, u8> {
        let Some(letter) = cell.slice_mark() else {
            return Ok(cell.coarse());
        };
        let own = self
            .song()
            .tracks
            .get(self.cursor.track)
            .map_or(0, |track| track.setting(SettingId::SLICE));
        self.song()
            .bank
            .get(self.step_entry())
            .and_then(|entry| entry.edit.points.marked(letter))
            .map(|number| number as u8)
            .ok_or(own)
    }

    /// The rate an entry's file was recorded at, and its length in the file's frames, if it's
    /// loaded.
    pub(crate) fn file_of(&self, entry: &BankEntry) -> Option<(u32, u64)> {
        let id = self.samples.id_of(&entry.source()?.path)?;
        let loaded = self.samples.get(id)?.frames.len() as u64;
        // One made at the program's rate, as a recording is, is at the program's rate.
        match self.samples.rates(id) {
            Rates::SAME => Some((self.rate, loaded)),
            rates => Some((rates.file.max(1), rates.file_frame(loaded))),
        }
    }
}
