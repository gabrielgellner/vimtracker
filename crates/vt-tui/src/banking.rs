//! The project's bank of samples: adding files to it from the explorer, `:sample` and kits,
//! listing it for a track's `Sample` and a step's `SMP`, and removing what nothing plays.

use std::path::Path;
use std::sync::Arc;

use vt_model::action::{Action, FieldValue, KitSound};
use vt_model::bank::{Bank, BankEntry, ContentHash, PlayParams, Source};
use vt_model::kit::load_kit;
use vt_model::project;
use vt_model::sample::load_sample;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_model::step::{FxCommand, FxSlot, Mark};

use crate::app::{App, Field};
use crate::input::command::Line;
use crate::input::picker::{Item, Pick, Picker, Preview};

/// What a kit says about one of its sounds, for the entry it becomes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seed {
    pub label: String,
    pub name: String,
    pub play: PlayParams,
}

/// What's said when the bank has no room.
const FULL: &str = "The bank is full, at 255 samples: :bank clean removes those nothing plays";

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

impl App {
    /// Put a file in a bank, loading it into a pool, unless the bank already plays it, and give
    /// its number and whether it's new. A kit's sound comes with what the kit says of it, and is
    /// the same entry only if it plays the same file the same way, as a kit can tune one file
    /// twice for two toms.
    fn bank_file(
        &self,
        file: &Path,
        seed: Option<&Seed>,
        bank: &mut Bank,
        pool: &mut SamplePool,
    ) -> Result<(u8, bool), String> {
        let stored = self.store_sample(file).map_err(|error| error.to_string())?;
        let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
        let hash = ContentHash::of_file(&resolved)
            .map_err(|error| format!("{}: {error}", resolved.display()))?;
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
            return Ok((number, false));
        }
        if pool.id_of(&stored).is_none() {
            let data = load_sample(&resolved, self.rate).map_err(|error| error.to_string())?;
            pool.insert(&stored, data);
        }
        let mut entry = BankEntry::of_file(stored, hash);
        if let Some(seed) = seed {
            entry.label = seed.label.clone();
            if !seed.name.is_empty() {
                entry.name = seed.name.clone();
            }
            entry.edit.play = seed.play;
        }
        let number = bank.free().ok_or_else(|| FULL.to_owned())?;
        bank.set(number, Some(entry));
        Ok((number, true))
    }

    /// Put a file in the bank and have the current track play it, with other changes to the
    /// track, as one undo step. A folder opens in the explorer instead.
    pub(crate) fn sample_with(&mut self, file: &Path, seed: Option<&Seed>, more: Vec<Action>) {
        if file.is_dir() {
            self.open_sample_folder(file);
            return;
        }
        let mut bank = self.song().bank.clone();
        let mut pool = SamplePool::clone(&self.samples);
        let (number, new) = match self.bank_file(file, seed, &mut bank, &mut pool) {
            Ok(found) => found,
            Err(error) => {
                self.status = error;
                return;
            }
        };
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
        if self.status.is_empty() {
            let added = if new {
                "loaded into the bank"
            } else {
                "from the bank"
            };
            self.status = format!(
                "Track {:02} plays {}, {added}",
                track + 1,
                sample_text(self.song(), number)
            );
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
        let mut bank = self.song().bank.clone();
        let mut pool = SamplePool::clone(&self.samples);
        let mut actions = Vec::new();
        let mut sounds = Vec::with_capacity(kit.sounds.len());
        for sound in &kit.sounds {
            let seed = Seed {
                label: sound.label.clone(),
                name: sound.name.clone(),
                play: PlayParams {
                    gain_db: sound.gain,
                    tune: sound.tune,
                },
            };
            let (number, new) = match self.bank_file(&sound.path, Some(&seed), &mut bank, &mut pool)
            {
                Ok(found) => found,
                Err(error) => {
                    self.status = format!("{}: {error}", sound.path.display());
                    return;
                }
            };
            if new {
                actions.push(Action::SetBankEntry {
                    number,
                    entry: bank.get(number).cloned(),
                });
            }
            sounds.push(KitSound {
                sample: number,
                name: sound.label.clone(),
                choke: sound.choke,
            });
        }
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
        self.samples = Arc::new(pool);
        actions.push(Action::LoadKit(sounds));
        self.apply(&Action::Batch(actions));
        self.fit_levels();
        self.kit_file = Some(path.to_path_buf());
        self.status = format!("Kit {}: {count} tracks", kit.name);
        if !replaced.is_empty() {
            self.status = format!(
                "{}. Replaced {}: u to undo",
                self.status,
                replaced.join(" ")
            );
        }
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
                .meaning("Space 5, from the kit and your folders"),
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
