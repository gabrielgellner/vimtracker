//! Slicing the sample the current track plays, from `:slice`: into so many of the same length, or
//! back into one, and how one slice plays, its gain, pan, tune, reverse and fades. Each is one
//! undo step on the bank entry, which every track playing it hears. And choosing a slice from a
//! list, `K` on a step's `SLI` or the Sound page's `Slice`, by its number or by a letter naming it,
//! and `g d` on a step's `SLI` to see it on the sample page.

use vt_model::action::{Action, FieldValue};
use vt_model::analysis::{Analysis, Pyramid};
use vt_model::bank::{BankEntry, MAX_SLICES, SliceParams};
use vt_model::onsets::{MIN_GAP_SECONDS, MIN_SILENCE_SECONDS, after_silence, quiet_level};
use vt_model::settings::SettingId;
use vt_model::song::Machine;
use vt_model::step::{FxCommand, FxSlot, Mark, Step};

use crate::app::{App, Field};
use crate::banking::sample_text;
use crate::input::command::{Address, SliceChange, SliceRange};
use crate::input::mode::View;
use crate::input::picker::{Item, Pick, Picker};
use crate::sample_page::beat_frames;
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

impl App {
    /// The bank entry the current track's sampler plays, before any step chooses another.
    pub(crate) fn track_sample_number(&self) -> u8 {
        self.song()
            .tracks
            .get(self.cursor.track)
            .filter(|track| track.machine == Machine::Sampler)
            .map_or(0, |track| track.sample())
    }

    /// The slice a change to one slice is to: the one the sample page's cursor is in, while it
    /// shows this entry, or else the one the track's `Slice` is on.
    pub(crate) fn slice_here(&self, number: u8, entry: &BankEntry) -> usize {
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

    /// Slice the current track's sample, or change how its slices play, as one undo step, and say
    /// what it is now. A range re-slices only the slices inside it, or changes those starting in
    /// it, and with none it's the whole sample, or the slice here for a slice's own setting.
    pub(crate) fn change_slices(&mut self, change: SliceChange, range: Option<SliceRange>) {
        let number = self.track_sample_number();
        let Some(entry) = self.song().bank.get(number).cloned() else {
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
        let points = &entry.edit.points;
        let end = points.end_in(length);
        let whole = points.start.min(end)..end;
        let within = match range {
            None => whole.clone(),
            Some(range) => match self.range_frames(&entry, range, length) {
                Ok(within) => within,
                Err(why) => {
                    self.status = why;
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
                    self.status = format!("{name} has no slices");
                } else {
                    self.apply(&Action::SetSlices {
                        number,
                        starts: Vec::new(),
                    });
                    self.status = format!("{name} plays as one again: u to undo");
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
                let bar = match entry.edit.hints.bars {
                    Some(bars) => (whole.end - whole.start) as f64 / f64::from(bars.max(1)),
                    None => 4.0 * beat_frames(rate, self.song().millibpm),
                };
                let starts = beat_starts(&within, bar / f64::from(note.max(1)));
                let by = match entry.edit.hints.bars {
                    Some(1) => "as it's a bar long".to_owned(),
                    Some(bars) => format!("as it's {bars} bars long"),
                    None => "at the song's tempo".to_owned(),
                };
                let said = format!("{} slices of 1/{note}, {by}", starts.len());
                (starts, said)
            }
            SliceChange::Transients | SliceChange::Silence => {
                let Some(analysis) = self.entry_analysis(&entry) else {
                    self.status =
                        format!("{name} can't be analysed: its file is missing or has changed");
                    return;
                };
                let sens = entry.edit.hints.sens;
                let transients = change == SliceChange::Transients;
                let starts = if transients {
                    transient_starts(&analysis, &within, sens)
                } else {
                    self.silence_starts(&entry, &analysis, &within)
                };
                if starts.is_empty() {
                    self.status = if transients {
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
                self.change_slice_settings(change, range.map(|_| within), entry, rate, length);
                return;
            }
        };
        let starts = entry.edit.points.restarted(within, &inside, length);
        if starts.len() > MAX_SLICES {
            self.status = format!(
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
            self.status = format!("{name} is sliced so already");
            return;
        }
        let total = starts.len();
        self.apply(&Action::SetSlices { number, starts });
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
        self.status = format!("{name}{place} in {said}{all}{followed}: u to undo");
    }

    /// A slice's own setting, for the slice here, or for every slice starting in a stretch of
    /// frames, as one undo step.
    fn change_slice_settings(
        &mut self,
        change: SliceChange,
        within: Option<std::ops::Range<u64>>,
        mut entry: BankEntry,
        rate: u32,
        length: u64,
    ) {
        let number = self.track_sample_number();
        let name = sample_text(self.song(), number);
        if entry.edit.points.slices.is_empty() {
            self.status = format!("{name} has no slices: :slice 16 cuts it into 16");
            return;
        }
        let slices = &entry.edit.points.slices;
        let targets: Vec<usize> = match &within {
            Some(within) => (0..slices.len())
                .filter(|at| within.contains(&slices[*at].start))
                .collect(),
            None => vec![self.slice_here(number, &entry)],
        };
        let (Some(first), Some(last)) = (targets.first().copied(), targets.last().copied()) else {
            self.status = format!("No slice of {name} starts in that range");
            return;
        };
        let mut said = String::new();
        for at in targets {
            said = set_own(&mut entry.edit.points.slices[at].over, change, rate, length);
        }
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = if first == last {
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
                    .sample_page
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
        &self,
        entry: &BankEntry,
        analysis: &Analysis,
        within: &std::ops::Range<u64>,
    ) -> Vec<u64> {
        let quiet = quiet_level(analysis.peak);
        let gap = (MIN_SILENCE_SECONDS * f64::from(analysis.rate)).round() as u64;
        let shown = self.shown_of(self.track_sample_number(), entry).ok();
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

    /// An entry's slices as a list, each with its number, the letters naming it and where it
    /// starts, as `03  a  0.214 s`, or the entry as one, `00`, if it has none.
    fn slice_items(&self, number: u8, pick: impl Fn(u8) -> Pick) -> Vec<Item> {
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
    /// where it starts and how long it is, and for one by a letter, which slice the letter names,
    /// or that it names none, so the track's own plays.
    pub fn slice_shown(&self, cell: FxSlot) -> String {
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

    /// Choose the slice a step's `SLI` locks, from the slices of the sample the step plays: first
    /// by each letter naming one, as `'a  03  0.214 s`, which keeps to its slice as slices before
    /// it come and go, and then by number.
    pub(crate) fn open_slice_lock_picker(&mut self, slot: usize) {
        self.count = None;
        let current = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)));
        let number = self.step_entry();
        let mut items = self.mark_items(number, |letter| Pick::SliceMarkLock { slot, letter });
        let named = items.len();
        items.extend(self.slice_items(number, |number| Pick::SliceLock { slot, number }));
        let at = match current.map(|cell| (cell.slice_mark(), cell.coarse())) {
            Some((Some(letter), _)) => items[..named]
                .iter()
                .position(|item| item.pick == Pick::SliceMarkLock { slot, letter }),
            Some((None, number)) => Some(named + usize::from(number).min(items.len() - named - 1)),
            None => Some(named),
        };
        self.show_picker(Picker::new("Slice lock", items).opened_on(at));
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

    /// The mark an `SLI` in a slot has, if it has one, to keep as its value changes.
    fn slice_lock_mark(&self, slot: usize) -> Mark {
        self.current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SLICE)))
            .map_or(Mark::default(), |cell| cell.mark)
    }

    /// Put a chosen slice in a slot as an `SLI`, keeping its mark.
    pub(crate) fn pick_slice_lock(&mut self, slot: usize, number: u8) {
        let mark = self.slice_lock_mark(slot);
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

    /// Put a slice chosen by a letter in a slot as an `SLI 'a`, keeping its mark.
    pub(crate) fn pick_slice_mark_lock(&mut self, slot: usize, letter: char) {
        let mark = self.slice_lock_mark(slot);
        self.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(slot, FxSlot::slice_by_mark(letter, mark)));
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
        let slice = self.slice_of_lock(cell).unwrap_or_else(|own| own);
        match self.sample_page_here() {
            Ok(shown) => {
                let slice = usize::from(slice);
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
