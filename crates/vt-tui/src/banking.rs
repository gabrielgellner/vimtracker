//! The project's bank of samples: adding files to it from the explorer, `:sample` and kits,
//! listing it for a track's `Sample` and a step's `SMP`, and removing what nothing plays.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use vt_model::action::{Action, FieldValue, KitSound};
use vt_model::bank::{Bank, BankEntry, ContentHash, PlayParams, Source};
use vt_model::constants::{MAX_GROUPS, MAX_SOUND_TRACKS};
use vt_model::kit::load_kit;
use vt_model::label::GROUP_NAME_CHARS;
use vt_model::project;
use vt_model::samples::SamplePool;
use vt_model::seeding;
use vt_model::settings::SettingId;
use vt_model::song::{Song, TrackKind};
use vt_model::step::{FxCommand, FxSlot, Mark};

use crate::app::{App, Field};
use crate::input::command::Line;
use crate::input::picker::{Item, KitChoice, Pick, Picker, Preview};

/// What a kit says about one of its sounds, for the entry it becomes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seed {
    pub label: String,
    pub name: String,
    pub play: PlayParams,
}

/// What's said when the bank has no room.
const FULL: &str = "The bank is full, at 255 samples: :bank clean removes those nothing plays";

/// Words with their first letter a capital, to start a sentence.
fn capitalised(words: &str) -> String {
    let mut letters = words.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A bank entry's number and name as a list or a row shows it: `03 BD Bass Drum`, or `Off`.
pub fn sample_text(song: &Song, number: u8) -> String {
    match (number, song.bank.get(number)) {
        (0, _) => "Off".to_owned(),
        (number, Some(entry)) => format!("{number:02X} {}", entry.title()),
        (number, None) => format!("{number:02X} ---"),
    }
}

/// What the status line says of a `Sample` value: its entry and the file it plays.
pub fn sample_shown(song: &Song, number: u8) -> String {
    match (number, song.bank.get(number)) {
        (0, _) => "Off: no sample".to_owned(),
        (number, Some(entry)) => {
            let file = entry
                .source()
                .map_or("", |source| source.path.rsplit('/').next().unwrap_or(""));
            format!("{number:02X} {}: {file}", entry.title())
        }
        (number, None) => format!("{number:02X}: nothing in the bank there, so it's silent"),
    }
}

/// A file put in the bank: its number, whether it's new, and, for a new one whose file says how it
/// plays, the entry seeded from what it says, with that in words, and what in the file couldn't
/// be read.
struct Banked {
    number: u8,
    new: bool,
    seeded: Option<(BankEntry, String)>,
    problems: Vec<String>,
}

/// A kit's sounds put in a copy of the bank: the entry each plays, in order, the edits that add
/// the new ones, the edits that give them the points their files say, what in the files couldn't
/// be read, and the pool with their files loaded.
struct KitBanked {
    numbers: Vec<u8>,
    adds: Vec<Action>,
    seeds: Vec<Action>,
    problems: Vec<String>,
    pool: SamplePool,
}

/// The first group with no tracks in it, by its number from 1, for a kit's new tracks.
fn free_group(song: &Song) -> Option<u8> {
    (1..=MAX_GROUPS as u8).find(|&number| {
        song.fixed_track(TrackKind::Group(number))
            .is_some_and(|group| song.group_members(group).next().is_none())
    })
}

/// Words that say a kit is a kit rather than which one, so they don't name its group.
const GENERIC_WORDS: [&str; 12] = [
    "kit", "kits", "drum", "drums", "core", "pack", "packs", "rack", "racks", "set", "the", "vol",
];

/// What a group whose kit's name says nothing better is called (decision 308).
pub const KIT_GROUP_NAME: &str = "DRUMS";

/// A name's words, split at spaces and punctuation, leaving out those that only say it's a kit.
fn telling_words(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .filter(|word| !GENERIC_WORDS.contains(&word.to_ascii_lowercase().as_str()))
}

/// A group's name from its kit's: the first word of its name, in capitals, up to the five
/// characters a group's name holds, so `909 Core Kit` names it `909` and `Rytm Analog` `RYTM`.
/// A first word that's generic, as `Kit-808`'s `Kit` is, gives way to the kit's category, by its
/// first word that isn't and fits whole, or else `DRUMS`, as `Drum Machines` gives (decision 308).
pub fn group_name(kit: &str, category: &str) -> String {
    let first = kit
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| !word.is_empty());
    let name = match first {
        Some(word) if telling_words(word).next().is_some() => word,
        _ => telling_words(category)
            .find(|word| word.len() <= GROUP_NAME_CHARS)
            .unwrap_or(KIT_GROUP_NAME),
    };
    name.chars()
        .take(GROUP_NAME_CHARS)
        .collect::<String>()
        .to_ascii_uppercase()
}

impl App {
    /// Put a file in a bank, loading it into a pool, unless the bank already plays it. A kit's
    /// sound comes with what the kit says of it, and is the same entry only if it plays the same
    /// file the same way, as a kit can tune one file twice for two toms. The new entry plays the
    /// file as it is; what the file's markers or an `.ot` beside it say comes back separately, so
    /// it can be taken as an undo step of its own.
    fn bank_file(
        &self,
        file: &Path,
        seed: Option<&Seed>,
        bank: &mut Bank,
        pool: &mut SamplePool,
    ) -> Result<Banked, String> {
        let stored = self.store_sample(file).map_err(|error| error.to_string())?;
        let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
        let bytes =
            fs::read(&resolved).map_err(|error| format!("{}: {error}", resolved.display()))?;
        let hash = ContentHash::of(&bytes);
        let source = Source {
            path: stored.clone(),
            hash,
        };
        let found = match seed {
            Some(seed) => bank
                .iter()
                .find(|(_, entry)| entry.plays(&source) && entry.edit.play == seed.play)
                .map(|(number, _)| number),
            None => bank.find(&source),
        };
        if let Some(number) = found {
            return Ok(Banked {
                number,
                new: false,
                seeded: None,
                problems: Vec::new(),
            });
        }
        let id = pool
            .load(&stored, &resolved, self.rate)
            .map_err(|error| error.to_string())?;
        let loaded = pool.get(id).map_or(0, |sample| sample.frames.len() as u64);
        let mut from_file = seeding::seed(&resolved, &bytes, pool.rates(id).file_frame(loaded));
        let mut entry = BankEntry::of_file(stored, hash);
        if let Some(seed) = seed {
            entry.label = seed.label.clone();
            if !seed.name.is_empty() {
                entry.name = seed.name.clone();
            }
            entry.edit.play = seed.play;
            // The kit's gain is the sound's.
            from_file.gain_db = None;
        }
        let number = bank.free().ok_or_else(|| FULL.to_owned())?;
        let seeded = (!from_file.is_empty()).then(|| {
            let mut seeded = entry.clone();
            from_file.apply(&mut seeded.edit);
            (seeded, from_file.describe())
        });
        bank.set(number, Some(entry));
        Ok(Banked {
            number,
            new: true,
            seeded,
            problems: from_file.problems,
        })
    }

    /// Put a file in the bank and have the current track play it, with other changes to the
    /// track, as one undo step. A folder opens in the explorer instead. What the file says of how
    /// it plays, its slices, loop and the like, follows as a second undo step, so `u` leaves them
    /// out and keeps the file.
    pub(crate) fn sample_with(&mut self, file: &Path, seed: Option<&Seed>, more: Vec<Action>) {
        if file.is_dir() {
            self.open_sample_folder(file);
            return;
        }
        let mut bank = self.song().bank.clone();
        let mut pool = SamplePool::clone(&self.samples);
        let banked = match self.bank_file(file, seed, &mut bank, &mut pool) {
            Ok(found) => found,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let Banked {
            number,
            new,
            seeded,
            problems,
        } = banked;
        self.samples = Arc::new(pool);
        let track = self.cursor.track;
        let mut actions = Vec::new();
        if new {
            actions.push(Action::SetBankEntry {
                number,
                entry: bank.get(number).cloned(),
            });
        }
        actions.push(Action::SetTrackSample {
            track,
            sample: number,
        });
        actions.extend(more);
        self.apply(&Action::Batch(actions));
        let banked = self.song().bank.get(number).is_some();
        let seeded = seeded.filter(|_| banked);
        if let Some((entry, _)) = &seeded {
            self.apply(&Action::SetBankEntry {
                number,
                entry: Some(entry.clone()),
            });
        }
        if !self.status.is_empty() {
            return;
        }
        let added = if new {
            "loaded into the bank"
        } else {
            "from the bank"
        };
        let mut said = format!(
            "Track {:02} plays {}, {added}",
            track + 1,
            sample_text(self.song(), number)
        );
        if let Some((_, what)) = seeded {
            said = format!("{said} with {what}: u leaves them out");
        }
        for problem in problems {
            said = format!("{said}. {}", capitalised(&problem));
        }
        self.status = said;
    }

    /// Put a kit's sounds, each as its file and what the kit says of it, in a copy of the bank,
    /// loading their files: the entry each plays, the edits that add the new ones, and the points
    /// their files say as edits of their own. Any that won't load stops it, with the file named.
    fn bank_kit_sounds<'a>(
        &self,
        sounds: impl IntoIterator<Item = (&'a Path, Seed)>,
    ) -> Result<KitBanked, String> {
        let mut bank = self.song().bank.clone();
        let mut pool = SamplePool::clone(&self.samples);
        let mut banked = KitBanked {
            numbers: Vec::new(),
            adds: Vec::new(),
            seeds: Vec::new(),
            problems: Vec::new(),
            pool: SamplePool::default(),
        };
        for (path, seed) in sounds {
            let Banked {
                number,
                new,
                seeded,
                problems,
            } = self
                .bank_file(path, Some(&seed), &mut bank, &mut pool)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if let Some((entry, _)) = seeded {
                banked.seeds.push(Action::SetBankEntry {
                    number,
                    entry: Some(entry),
                });
            }
            let file = path.file_name().unwrap_or_default().to_string_lossy();
            banked
                .problems
                .extend(problems.iter().map(|problem| format!("{file}: {problem}")));
            if new {
                banked.adds.push(Action::SetBankEntry {
                    number,
                    entry: bank.get(number).cloned(),
                });
            }
            banked.numbers.push(number);
        }
        banked.pool = pool;
        Ok(banked)
    }

    /// After a kit's sounds are in: the points their files say, as an undo step of their own, and
    /// the status line saying so and what couldn't be read, after what it already says.
    fn finish_kit(&mut self, seeds: Vec<Action>, problems: &[String]) {
        let seeded = seeds.len();
        if seeded > 0 {
            self.apply(&Action::Batch(seeds));
            let which = if seeded == 1 {
                "1 sound with points from its file".to_owned()
            } else {
                format!("{seeded} sounds with points from their files")
            };
            self.status = format!("{}, {which}: u leaves them out", self.status);
        }
        for problem in problems {
            self.status = format!("{}. {problem}", self.status);
        }
    }

    /// Put a kit's sounds in the bank and on tracks from the first, keeping the tracks' other
    /// settings and adding tracks if needed. It is one undo step, and if any sound won't load,
    /// nothing changes. Replacing a track's other sample is reported.
    pub fn load_kit(&mut self, path: &Path) {
        let kit = match load_kit(path) {
            Ok(kit) if kit.sounds.is_empty() => {
                self.status = format!("{} has no sounds", path.display());
                return;
            }
            Ok(kit) => kit,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let seeds = kit.sounds.iter().map(|sound| {
            let seed = Seed {
                label: sound.label.clone(),
                name: sound.name.clone(),
                play: PlayParams {
                    gain_db: sound.gain,
                    tune: sound.tune,
                    ..PlayParams::default()
                },
            };
            (sound.path.as_path(), seed)
        });
        let banked = match self.bank_kit_sounds(seeds) {
            Ok(banked) => banked,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let sounds: Vec<KitSound> = kit
            .sounds
            .iter()
            .zip(&banked.numbers)
            .map(|(sound, &sample)| KitSound {
                sample,
                name: sound.label.clone(),
                choke: sound.choke,
            })
            .collect();
        let replaced: Vec<String> = sounds
            .iter()
            .enumerate()
            .filter(|(track, sound)| {
                self.song()
                    .tracks
                    .get(*track)
                    .is_some_and(|old| old.sample() != 0 && old.sample() != sound.sample)
            })
            .map(|(track, _)| format!("{:02}", track + 1))
            .collect();
        let count = sounds.len();
        let KitBanked {
            mut adds,
            seeds,
            problems,
            pool,
            ..
        } = banked;
        let seeded = !seeds.is_empty();
        self.samples = Arc::new(pool);
        adds.push(Action::LoadKit(sounds));
        self.apply(&Action::Batch(adds));
        self.fit_levels();
        self.kit_file = Some(path.to_path_buf());
        self.status = format!("Kit {}: {count} tracks", kit.name);
        self.finish_kit(seeds, &[]);
        if !replaced.is_empty() {
            let undo = if seeded { "u again" } else { "u" };
            self.status = format!(
                "{}. Replaced {}: {undo} to undo",
                self.status,
                replaced.join(" ")
            );
        }
        self.finish_kit(Vec::new(), &problems);
    }

    /// The kit dialog's `Enter`: put these of a kit's sounds on new tracks after the last, never
    /// over one, each named by its label and in its choke group, and the tracks in the first
    /// group with none, named after the kit and folded. It is one undo step, and if any sound
    /// won't load, nothing changes. Where the tracks have a kick, a snare or a hat, a beat from
    /// Grids' map is offered after.
    pub fn load_kit_tracks(&mut self, path: &Path, chosen: &[KitChoice]) {
        if chosen.is_empty() {
            return;
        }
        let kit = match load_kit(path) {
            Ok(kit) => kit,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let first = self.song().sound_tracks();
        let count = chosen.len();
        if first + count > MAX_SOUND_TRACKS {
            self.status = format!(
                "{count} more tracks would be over the {MAX_SOUND_TRACKS} a project can have"
            );
            return;
        }
        let seeds = chosen.iter().map(|sound| {
            let seed = Seed {
                label: sound.label.clone(),
                name: sound.name.clone(),
                play: sound.play,
            };
            (sound.path.as_path(), seed)
        });
        let banked = match self.bank_kit_sounds(seeds) {
            Ok(banked) => banked,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let KitBanked {
            numbers,
            mut adds,
            seeds,
            problems,
            pool,
        } = banked;
        adds.push(Action::AddTracks(count));
        for (at, (sound, &sample)) in chosen.iter().zip(&numbers).enumerate() {
            let track = first + at;
            adds.push(Action::SetTrackSample { track, sample });
            adds.push(Action::SetTrackChoke {
                track,
                choke: sound.choke,
            });
            if !sound.label.is_empty() {
                adds.push(Action::SetTrackName {
                    track,
                    name: sound.label.clone(),
                });
            }
        }
        let group = free_group(self.song());
        if let Some(number) = group {
            adds.push(Action::SetTrackGroup {
                first,
                last: first + count - 1,
                group: Some(number),
            });
            // The group moves along with the fixed tracks, past the new ones.
            let name = group_name(&kit.name, &kit.category);
            if let Some(at) = self.song().fixed_track(TrackKind::Group(number))
                && !name.is_empty()
            {
                adds.push(Action::SetTrackName {
                    track: at + count,
                    name,
                });
            }
        }
        let depth = self.history.undo_depth();
        self.samples = Arc::new(pool);
        self.apply(&Action::Batch(adds));
        if self.history.undo_depth() == depth {
            return;
        }
        self.fit_levels();
        // A cursor on a fixed track stays on it.
        if self.cursor.track >= first {
            self.cursor.track += count;
        }
        if let Some(number) = group {
            self.fold_group(number, true);
        }
        self.kit_file = Some(path.to_path_buf());
        let tracks = match count {
            1 => format!("track {:02}", first + 1),
            _ => format!("tracks {:02}-{:02}", first + 1, first + count),
        };
        self.status = match group {
            Some(number) => format!("Kit {}: {tracks} in G{number}, folded", kit.name),
            None => format!(
                "Kit {}: {tracks} in no group, as all {MAX_GROUPS} have tracks",
                kit.name
            ),
        };
        self.finish_kit(seeds, &problems);
        let tracks: Vec<usize> = (first..first + count).collect();
        self.offer_kit_beat(&tracks);
    }

    /// Remove the bank's entries nothing names, as one undo step: no track's `Sample` and no
    /// step's `SMP`. The numbers of the rest stay as they are.
    pub(crate) fn clean_bank(&mut self) {
        let used = self.song().samples_in_use();
        let unused: Vec<u8> = self
            .song()
            .bank
            .iter()
            .map(|(number, _)| number)
            .filter(|number| !used.contains(number))
            .collect();
        if unused.is_empty() {
            self.status = "Every sample in the bank is played".to_owned();
            return;
        }
        let count = unused.len();
        let actions = unused
            .into_iter()
            .map(|number| Action::SetBankEntry {
                number,
                entry: None,
            })
            .collect();
        self.apply(&Action::Batch(actions));
        let noun = if count == 1 { "sample" } else { "samples" };
        self.status = format!("Removed {count} {noun} nothing plays: u to undo");
    }

    /// The bank as a list: `Off`, then each entry with its file beside it and its waveform shown,
    /// for choosing what the current track plays or what a step's `SMP` locks. The track's list
    /// ends with a way to add a file.
    fn bank_items(&self, pick: impl Fn(u8) -> Pick) -> Vec<Item> {
        let song = self.song();
        let off = Item::new("00  Off", pick(0)).meaning("No sample");
        let entries = song.bank.iter().map(|(number, entry)| {
            let source = entry.source();
            let file = source.map_or("", |source| {
                source.path.rsplit('/').next().unwrap_or(&source.path)
            });
            let item =
                Item::new(format!("{number:02X}  {}", entry.title()), pick(number)).meaning(file);
            match source {
                Some(source) => item.preview(Preview::Sample(project::resolve_sample(
                    self.folder.as_deref(),
                    &source.path,
                ))),
                None => item,
            }
        });
        std::iter::once(off).chain(entries).collect()
    }

    /// Where a number is in the bank's list, which has `Off` first.
    fn bank_place(&self, number: u8) -> usize {
        self.song()
            .bank
            .iter()
            .position(|(at, _)| at == number)
            .map_or(0, |at| at + 1)
    }

    /// Choose what the current track's sampler plays, from the bank. With nothing in the bank yet,
    /// the explorer opens to add a file.
    pub(crate) fn open_bank_picker(&mut self) {
        self.count = None;
        if self.song().bank.is_empty() {
            self.open_default_sample_picker();
            return;
        }
        let track = self.cursor.track;
        let current = self
            .song()
            .tracks
            .get(track)
            .map_or(0, |track| track.sample());
        let mut items = self.bank_items(|number| Pick::BankSample { track, number });
        items.push(
            Item::new("Add a file", Pick::Line(Line::Sample(None)))
                .meaning(":sample, from the kit and your folders"),
        );
        let at = self.bank_place(current);
        self.show_picker(Picker::new("Bank", items).opened_on(Some(at)));
    }

    /// Choose the sample a step's `SMP` locks, from the bank.
    pub(crate) fn open_sample_lock_picker(&mut self, slot: usize) {
        self.count = None;
        let current = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SAMPLE)))
            .map_or(0, |cell| cell.coarse());
        let items = self.bank_items(|number| Pick::SampleLock { slot, number });
        let at = self.bank_place(current);
        self.show_picker(Picker::new("Sample lock", items).opened_on(Some(at)));
    }

    /// Put a chosen bank entry in a slot as an `SMP`, keeping its mark.
    pub(crate) fn pick_sample_lock(&mut self, slot: usize, number: u8) {
        let mark = self
            .current_slot(slot)
            .filter(|cell| cell.command == Some(FxCommand::Set(SettingId::SAMPLE)))
            .map_or(Mark::default(), |cell| cell.mark);
        self.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(
            slot,
            FxSlot {
                command: Some(FxCommand::Set(SettingId::SAMPLE)),
                value: u16::from(number) << 8,
                mark,
            },
        ));
    }
}
