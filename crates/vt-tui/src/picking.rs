//! Opening the picker and acting on what it chooses.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, FieldValue, SlotSample};
use vt_model::kit::load_kit;
use vt_model::label::name_of;
use vt_model::project;
use vt_model::sample::load_wav;
use vt_model::samples::SamplePool;
use vt_model::step::{FxCommand, FxSlot};

use crate::app::{App, Entry, Field};
use crate::guides::GUIDES;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How many files the sample browser lists, and how many folders deep it looks.
const MAX_SAMPLES: usize = 2000;
const MAX_DEPTH: usize = 4;

/// Find WAV files under a folder, skipping hidden ones, in a stable order.
pub fn find_wavs(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(folder, 0, &mut found);
    found
}

fn walk(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MAX_SAMPLES {
            return;
        }
        if !is_visible(&path) {
            continue;
        }
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, found);
            }
        } else if is_wav(&path) {
            found.push(path);
        }
    }
}

/// The kits in a folder: its RON files, by name.
pub fn kits_in(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut kits: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            is_visible(path)
                && path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ron"))
        })
        .collect();
    kits.sort();
    kits
}

fn is_visible(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

/// A kit's name: its file name without the extension.
fn kit_name(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Where kits are looked for by name, from where the program ran.
const KITS_DIR: &str = "samples/kits";

impl App {
    /// Open the view picker, with the view before this one highlighted so `Enter` flips back.
    pub(crate) fn open_view_picker(&mut self) {
        self.count = None;
        let items = self
            .recent_views
            .iter()
            .map(|view| Item {
                label: view.name().to_owned(),
                pick: Pick::View(*view),
            })
            .collect();
        let mut picker = Picker::new("Views", items);
        picker.move_highlight(1);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Open the list of guides, in the order to learn them.
    pub(crate) fn open_guide_picker(&mut self) {
        let items = GUIDES
            .iter()
            .map(|guide| Item {
                label: guide.title().to_owned(),
                pick: Pick::Guide(*guide),
            })
            .collect();
        self.picker = Some(Picker::new("Guides", items));
        self.mode = Mode::Picker;
    }

    /// Choose a kit from those in `samples/kits`.
    pub(crate) fn open_kit_picker(&mut self) {
        let items: Vec<Item> = kits_in(Path::new(KITS_DIR))
            .into_iter()
            .map(|path| Item {
                label: kit_name(&path),
                pick: Pick::Kit(path),
            })
            .collect();
        if items.is_empty() {
            self.status = format!("No kits yet: add a kit file to {KITS_DIR}, such as 909.ron");
            return;
        }
        self.picker = Some(Picker::new("Kits", items));
        self.mode = Mode::Picker;
    }

    /// Load a kit named in `samples/kits`, or at a path.
    pub(crate) fn load_named_kit(&mut self, name: &Path) {
        if name.is_file() {
            self.load_kit(name);
        } else {
            self.load_kit(&Path::new(KITS_DIR).join(name).with_extension("ron"));
        }
    }

    /// Fill slots with a kit's sounds, keeping their other settings: a labelled sound in its
    /// label's slot, and the others from `00`. It is one undo step, and if any sound won't load,
    /// nothing changes. Replacing a different sample is reported.
    pub fn load_kit(&mut self, path: &Path) {
        let entries = match load_kit(path) {
            Ok(entries) if entries.is_empty() => {
                self.status = format!("{} has no sounds", path.display());
                return;
            }
            Ok(entries) => entries,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let mut pool = SamplePool::clone(&self.samples);
        let mut samples = Vec::with_capacity(entries.len());
        let mut next_number = 0;
        for entry in &entries {
            let stored = self.store_sample(&entry.path).map_err(|e| e.to_string());
            let loaded = stored.and_then(|stored| {
                let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
                let data = load_wav(&resolved, self.rate).map_err(|e| e.to_string())?;
                Ok((stored, data))
            });
            let (stored, data) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    self.status = format!("{}: {error}", entry.path.display());
                    return;
                }
            };
            pool.insert(&stored, data);
            let id = entry.slot.unwrap_or_else(|| {
                next_number += 1;
                next_number - 1
            });
            samples.push(SlotSample {
                id,
                sample: stored,
                labelled: entry.slot.is_some(),
            });
        }
        let replaced: Vec<String> = samples
            .iter()
            .filter(|slot| {
                let old = self.song().instruments.get(&slot.id);
                old.and_then(|i| i.sample.as_ref())
                    .is_some_and(|old| *old != slot.sample)
            })
            .map(|slot| name_of(slot.id))
            .collect();
        let names: Vec<String> = samples.iter().map(|slot| name_of(slot.id)).collect();
        self.samples = Arc::new(pool);
        self.instrument = samples[0].id;
        self.apply(&Action::SetSamples(samples));
        self.status = format!("Kit {}: {}", kit_name(path), names.join(" "));
        if !replaced.is_empty() {
            self.status = format!(
                "{}. Replaced {}: u to undo",
                self.status,
                replaced.join(" ")
            );
        }
    }

    /// Choose this step's instrument from those that exist, labels first, with the current one
    /// highlighted. It only means something on the Instrument column.
    pub(crate) fn open_instrument_picker(&mut self) {
        self.open_instruments(Pick::Instrument);
    }

    /// Choose this field from a list, in the columns that are one.
    pub(crate) fn open_field_picker(&mut self) {
        match self.cursor.field {
            Field::Instrument => self.open_instrument_picker(),
            Field::FxCommand(slot) | Field::FxValue(slot) => self.open_fx_picker(slot),
            Field::Note | Field::Velocity => {
                self.status = "K chooses on the Instrument and FX columns".to_owned();
            }
        }
    }

    /// List the FX commands, with what each does and this slot's current one highlighted.
    fn open_fx_picker(&mut self, slot: usize) {
        let current = self.current_slot(slot).and_then(|s| s.command);
        let empty = Item {
            label: "---  Empty".to_owned(),
            pick: Pick::FxCommand {
                slot,
                command: None,
            },
        };
        let items = std::iter::once(empty)
            .chain(FxCommand::ALL.map(|command| Item {
                label: format!("{}  {}", command.name(), command.meaning()),
                pick: Pick::FxCommand {
                    slot,
                    command: Some(command),
                },
            }))
            .collect();
        let position = FxCommand::ALL
            .iter()
            .position(|command| Some(*command) == current)
            .map_or(0, |position| position + 1);
        let mut picker = Picker::new("FX commands", items);
        picker.move_highlight(position as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Put a chosen command in a slot. A slot with a value keeps it. An empty one waits for its value
    /// to be typed, as a command with no value yet doesn't take effect.
    fn pick_fx_command(&mut self, slot: usize, command: Option<FxCommand>) {
        let Some(command) = command else {
            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
            self.cursor.field = Field::FxCommand(slot);
            return;
        };
        self.cursor.field = Field::FxValue(slot);
        let current = self.current_slot(slot);
        if let Some(value) = current.filter(|s| s.command.is_some()).map(|s| s.value) {
            self.set_field(FieldValue::Slot(
                slot,
                FxSlot {
                    command: Some(command),
                    value,
                },
            ));
            return;
        }
        self.entry = Entry {
            command: Some(command),
            ..Entry::default()
        };
        self.replacing = Some(Field::FxValue(slot));
        self.mode = Mode::Replace;
    }

    /// Choose the current instrument from anywhere, writing nothing to the song.
    pub(crate) fn open_current_instrument_picker(&mut self) {
        self.open_instruments(Pick::CurrentInstrument);
    }

    /// List the instruments that exist, labels first, with the current one highlighted.
    fn open_instruments(&mut self, pick: fn(u32) -> Pick) {
        let song = self.song();
        let mut slots: Vec<u32> = song.instruments.keys().copied().collect();
        slots.sort_by_key(|id| !song.is_labelled(*id));
        let current = slots.iter().position(|id| *id == self.instrument);
        let items: Vec<Item> = slots
            .iter()
            .map(|id| {
                let sample = song.instruments[id]
                    .sample
                    .as_deref()
                    .unwrap_or("(no sample)");
                let file = sample.rsplit('/').next().unwrap_or(sample);
                Item {
                    label: format!("{}  {file}", name_of(*id)),
                    pick: pick(*id),
                }
            })
            .collect();
        let mut picker = Picker::new("Instruments", items);
        picker.move_highlight(current.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    fn pick_instrument(&mut self, id: u32) {
        self.instrument = id;
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::SetField {
                phrase,
                step: self.cursor.step,
                value: FieldValue::Instrument(Some(id)),
            });
        }
    }

    /// Open the sample browser on a folder. If nothing is there, offer a typed path instead.
    pub(crate) fn open_sample_picker(&mut self, roots: &[PathBuf]) {
        let items: Vec<Item> = roots
            .iter()
            .flat_map(|root| {
                find_wavs(root).into_iter().map(move |path| Item {
                    label: path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                    pick: Pick::Sample(path),
                })
            })
            .collect();
        if items.is_empty() {
            self.status = "No WAV files found here: type a path, or a folder of them".to_owned();
            self.command_line = "sample ".to_owned();
            self.mode = Mode::Command;
            return;
        }
        self.picker = Some(Picker::new("Samples", items));
        self.mode = Mode::Picker;
    }

    /// The folders the sample browser starts from: the project's samples, then where we ran.
    pub(crate) fn sample_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(folder) = &self.folder {
            roots.push(folder.join("samples"));
        }
        if let Ok(current) = std::env::current_dir()
            && !self
                .folder
                .as_deref()
                .is_some_and(|f| current.starts_with(f))
        {
            roots.push(current);
        }
        roots
    }

    pub(crate) fn picker_key(&mut self, key: Key) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.picker = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let pick = picker.current().map(|item| item.pick.clone());
                self.picker = None;
                self.mode = Mode::Normal;
                match pick {
                    Some(Pick::View(view)) => self.jump_to_view(view),
                    Some(Pick::Use { track, slot }) => self.open_use(track, slot),
                    Some(Pick::Sample(path)) => self.sample(&path),
                    Some(Pick::Kit(path)) => self.load_kit(&path),
                    Some(Pick::Instrument(id)) => self.pick_instrument(id),
                    Some(Pick::CurrentInstrument(id)) => self.instrument = id,
                    Some(Pick::FxCommand { slot, command }) => self.pick_fx_command(slot, command),
                    Some(Pick::Lfo { track, slot, lfo }) => {
                        self.apply(&Action::SetLfo { track, slot, lfo });
                    }
                    Some(Pick::RowPhrase(phrase)) => self.pick_row_phrase(phrase),
                    Some(Pick::Guide(guide)) => {
                        self.guide = Some(guide);
                        self.guide_scroll = 0;
                        self.mode = Mode::Guide;
                    }
                    None => {}
                }
            }
            KeyCode::Down => picker.move_highlight(1),
            KeyCode::Up => picker.move_highlight(-1),
            KeyCode::Char('n') if key.ctrl => picker.move_highlight(1),
            KeyCode::Char('p') if key.ctrl => picker.move_highlight(-1),
            KeyCode::Backspace => {
                let mut query = picker.query.clone();
                query.pop();
                picker.set_query(query);
            }
            KeyCode::Char(c) if !key.ctrl => {
                let query = format!("{}{c}", picker.query);
                picker.set_query(query);
            }
            _ => {}
        }
    }

    /// Record a view as most recently used.
    pub(crate) fn note_view(&mut self, view: View) {
        self.recent_views.retain(|other| *other != view);
        self.recent_views.insert(0, view);
    }
}
