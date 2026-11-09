//! Opening the picker and acting on what it chooses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vt_model::action::{Action, FieldValue, KitSound, TrackControl};
use vt_model::kit::{KitEntry, load_kit};
use vt_model::project;
use vt_model::sample::{is_sample_file, load_sample};
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::song::Machine;
use vt_model::step::{FxCommand, FxSlot, Gate, GateKind};

use crate::app::{App, Entry, Field};
use crate::guides::GUIDES;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Outcome, Pick, Picker, Preview, PreviewLine, Shown};
use crate::kitty;
use crate::song_view::COLUMNS_BEFORE_TRACKS;
use crate::views::picker::{PREVIEW_WAVE_HEIGHT, PREVIEW_WAVE_WIDTH};

/// How many files the sample browser lists, and how many folders deep it looks.
const MAX_SAMPLES: usize = 2000;
const MAX_DEPTH: usize = 4;

/// Find WAV, AIFF and FLAC files under a folder, skipping hidden ones, in a stable order.
pub fn find_samples(folder: &Path) -> Vec<PathBuf> {
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
            // A Rust build folder holds no samples, and takes a noticeable time to walk.
            if depth < MAX_DEPTH && !path.ends_with("target") {
                walk(&path, depth + 1, found);
            }
        } else if is_sample_file(&path) {
            found.push(path);
        }
    }
}

/// The kits in a folder and the folders in it, as the importer writes them: its RON files, by
/// path, those in the folder itself first.
pub fn kits_in(root: &Path) -> Vec<PathBuf> {
    let mut kits = Vec::new();
    walk_kits(root, 0, &mut kits);
    kits
}

fn walk_kits(folder: &Path, depth: usize, kits: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_visible(path))
        .collect();
    paths.sort();
    let (folders, files): (Vec<PathBuf>, Vec<PathBuf>) =
        paths.into_iter().partition(|path| path.is_dir());
    kits.extend(files.into_iter().filter(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ron"))
    }));
    if depth < MAX_DEPTH {
        for folder in folders {
            walk_kits(&folder, depth + 1, kits);
        }
    }
}

fn is_visible(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
}

/// A kit as `:kit` takes it: its path in its kit folder, without the extension.
fn kit_name(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder)
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .into_owned()
}

/// The folder a file is in, under the folder it was found from, which is the category the picker
/// lists it under, or empty for the folder itself.
fn folder_in(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A row for a kit's sound: its label, and its name beside it, under the kit's name.
fn kit_item(entry: &KitEntry, kit: &str) -> Item {
    Item::new(
        entry.label.clone(),
        Pick::KitSound {
            path: entry.path.clone(),
            label: entry.label.clone(),
            level: entry.level(),
            pitch: entry.pitch(),
            choke: entry.choke,
        },
    )
    .meaning(entry.name.clone())
    .category(format!("Kit {kit}"))
}

/// What a kit's preview shows: its name, how many sounds it has and where it came from, and each
/// sound with its label, its name and its waveform. Waveforms already worked out are used again.
fn kit_preview(
    item: usize,
    path: &Path,
    rate: u32,
    waves: &mut HashMap<PathBuf, Option<Vec<(usize, usize)>>>,
) -> Shown {
    let kit = match load_kit(path) {
        Ok(kit) => kit,
        Err(error) => {
            return Shown {
                item,
                heading: path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                note: error.to_string(),
                lines: Vec::new(),
            };
        }
    };
    let count = match kit.sounds.len() {
        1 => "1 sound".to_owned(),
        count => format!("{count} sounds"),
    };
    let note = if kit.source.is_empty() {
        count
    } else {
        format!("{count} from {}", kit.source)
    };
    let lines = kit
        .sounds
        .iter()
        .map(|sound| PreviewLine {
            label: sound.label.clone(),
            name: sound.name.clone(),
            wave: waves
                .entry(sound.path.clone())
                .or_insert_with(|| {
                    load_sample(&sound.path, rate).ok().map(|sample| {
                        kitty::waveform_spans(
                            &sample.frames,
                            PREVIEW_WAVE_WIDTH,
                            PREVIEW_WAVE_HEIGHT,
                        )
                    })
                })
                .clone(),
        })
        .collect();
    Shown {
        item,
        heading: kit.name,
        note,
        lines,
    }
}

impl App {
    /// A kit at a path, or by name in the first of the settings' kit folders that has it. A
    /// name found nowhere is looked for in the first folder, so the error names it.
    pub(crate) fn kit_path(&self, name: &Path) -> PathBuf {
        if name.is_file() {
            return name.to_path_buf();
        }
        let candidates: Vec<PathBuf> = self
            .settings
            .kit_folders()
            .iter()
            .map(|folder| folder.join(name).with_extension("ron"))
            .collect();
        candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .or(candidates.first())
            .cloned()
            .unwrap_or_else(|| name.with_extension("ron"))
    }

    /// Open the view picker, with the view before this one highlighted so `Enter` flips back.
    pub(crate) fn open_view_picker(&mut self) {
        self.count = None;
        let items: Vec<Item> = self
            .recent_views
            .iter()
            .map(|view| Item::new(view.name(), Pick::View(*view)))
            .collect();
        let mut picker = Picker::new("Views", items).opened_on(Some(0));
        picker.move_highlight(1);
        self.show_picker(picker);
    }

    /// Show a picker, and its preview if it has one.
    pub(crate) fn show_picker(&mut self, picker: Picker) {
        self.picker = Some(picker);
        self.mode = Mode::Picker;
        self.refresh_preview();
    }

    /// Load the preview of the item under the picker's cursor, if it has one and it isn't already
    /// shown.
    pub(crate) fn refresh_preview(&mut self) {
        let rate = self.rate;
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let wanted = picker
            .current_index()
            .filter(|item| picker.item(*item).preview.is_some());
        if picker.preview.as_ref().map(|shown| shown.item) == wanted {
            return;
        }
        picker.preview_images.clear();
        picker.preview = wanted.and_then(|item| match picker.item(item).preview.clone()? {
            Preview::Kit(path) => Some(Arc::new(kit_preview(item, &path, rate, &mut picker.waves))),
        });
    }

    /// Open the list of scales, with this one highlighted.
    pub(crate) fn open_scale_picker(&mut self) {
        self.count = None;
        let items = ScaleKind::ALL
            .map(|kind| Item::new(kind.name(), Pick::Scale(kind)))
            .to_vec();
        let at = ScaleKind::ALL
            .iter()
            .position(|kind| *kind == self.scale.kind);
        self.show_picker(Picker::new("Scales", items).opened_on(at));
    }

    /// Open the list of tracks, like Vim's buffers, with this one highlighted.
    pub(crate) fn open_track_picker(&mut self) {
        self.count = None;
        let items = self
            .song()
            .tracks
            .iter()
            .enumerate()
            .map(|(track, state)| {
                let sound = match state.machine {
                    _ if !state.is_sound() => state.kind.title(),
                    Machine::Sample => state
                        .sample
                        .as_deref()
                        .map_or("", |sample| sample.rsplit('/').next().unwrap_or(sample)),
                    Machine::Plaits(voice) => voice.name(),
                };
                let number = if state.is_sound() {
                    format!("{:02}", track + 1)
                } else {
                    String::new()
                };
                Item::new(
                    format!("{number:<2}  {}", state.name).trim_end(),
                    Pick::Track(track),
                )
                .meaning(sound)
            })
            .collect();
        // Found by name or number, as Vim's buffers are, so it opens on the search.
        self.show_picker(
            Picker::new("Tracks", items)
                .opened_on(Some(self.cursor.track))
                .searching(""),
        );
    }

    /// Go to a track chosen from the list, which `C-o` comes back from.
    fn go_to_track(&mut self, track: usize) {
        self.record_jump();
        self.move_track(track as isize - self.cursor.track as isize);
        self.song_column = COLUMNS_BEFORE_TRACKS + track;
    }

    /// Open the list of guides, in the order to learn them.
    pub(crate) fn open_guide_picker(&mut self) {
        let items = GUIDES
            .iter()
            .map(|guide| Item::new(guide.title(), Pick::Guide(*guide)))
            .collect();
        self.show_picker(Picker::new("Guides", items));
    }

    /// Choose a kit from those in the settings' kit folders, under the folders they're in, such as
    /// `Ableton/Drum Machines`, with each kit's sounds beside the list. `Enter` makes it the kit
    /// `Space 5` lists first, and `L` puts its sounds on tracks.
    pub(crate) fn open_kit_picker(&mut self) {
        let items: Vec<Item> = self
            .settings
            .kit_folders()
            .iter()
            .flat_map(|folder| {
                kits_in(folder).into_iter().map(move |path| {
                    let name = kit_name(folder, &path);
                    let label = name.rsplit('/').next().unwrap_or(&name).to_owned();
                    Item::new(label, Pick::Kit(path.clone()))
                        .category(folder_in(folder, &path))
                        .wider(Pick::KitTracks(path.clone()))
                        .preview(Preview::Kit(path))
                })
            })
            .collect();
        if items.is_empty() {
            self.status = match self.settings.kits.first() {
                Some(folder) => format!("No kits yet: add a kit file to {folder}, such as 909.ron"),
                None => "No kit folders: add one to kits in settings.ron".to_owned(),
            };
            return;
        }
        let current = self.kit_file.as_ref().and_then(|file| {
            items
                .iter()
                .position(|item| item.pick == Pick::Kit(file.clone()))
        });
        self.show_picker(
            Picker::new("Kits", items)
                .opened_on(current)
                .hinted("Enter use   L on tracks"),
        );
    }

    /// Make a kit's sounds the ones to choose from for a track. Nothing is loaded until one is.
    pub(crate) fn use_kit(&mut self, path: &Path) {
        match load_kit(path) {
            Ok(kit) if kit.sounds.is_empty() => {
                self.status = format!("{} has no sounds", path.display());
            }
            Ok(kit) => {
                self.status = format!(
                    "Kit {}: {} sounds. Space 5 chooses one for this track",
                    kit.name,
                    kit.sounds.len()
                );
                self.kit = kit.sounds;
                self.kit_file = Some(path.to_path_buf());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Put a kit's sounds on tracks from the first, keeping their other settings and adding
    /// tracks if needed. It is one undo step, and if any sound won't load, nothing changes.
    /// Replacing a different sample is reported.
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
        let mut pool = SamplePool::clone(&self.samples);
        let mut sounds = Vec::with_capacity(kit.sounds.len());
        for entry in &kit.sounds {
            let stored = self.store_sample(&entry.path).map_err(|e| e.to_string());
            let loaded = stored.and_then(|stored| {
                let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
                let data = load_sample(&resolved, self.rate).map_err(|e| e.to_string())?;
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
            sounds.push(KitSound {
                sample: stored,
                name: entry.label.clone(),
                level: entry.level(),
                pitch: entry.pitch(),
                choke: entry.choke,
            });
        }
        let replaced: Vec<String> = sounds
            .iter()
            .enumerate()
            .filter(|(track, sound)| {
                let old = self.song().tracks.get(*track);
                old.and_then(|t| t.sample.as_ref())
                    .is_some_and(|old| *old != sound.sample)
            })
            .map(|(track, _)| format!("{:02}", track + 1))
            .collect();
        let count = sounds.len();
        self.samples = Arc::new(pool);
        self.apply(&Action::LoadKit(sounds));
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

    /// Choose this field from a list, in the columns that are one.
    pub(crate) fn open_field_picker(&mut self) {
        match self.cursor.field {
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                self.open_fx_picker(slot, "");
            }
            Field::Gate => self.open_gate_picker(),
            Field::Note | Field::Velocity => {
                self.status = "K chooses on the gate and FX columns".to_owned();
            }
        }
    }

    /// List the gate types, with what each does and the cell's current one highlighted.
    fn open_gate_picker(&mut self) {
        let current = self.current_gate().map(|gate| gate.kind);
        let empty = Item::new("---", Pick::Gate(None)).meaning("Empty, one pulse of SGL");
        let items = std::iter::once(empty)
            .chain(GateKind::ALL.iter().map(|kind| {
                Item::new(kind.name(), Pick::Gate(Some(*kind))).meaning(kind.meaning())
            }))
            .collect();
        let position = GateKind::ALL
            .iter()
            .position(|kind| Some(*kind) == current)
            .map_or(0, |position| position + 1);
        self.show_picker(Picker::new("Gates", items).opened_on(Some(position)));
    }

    /// Put a chosen gate type in the cell, keeping its pulses, or empty it.
    fn pick_gate(&mut self, kind: Option<GateKind>) {
        let gate = kind.map(|kind| Gate {
            kind,
            pulses: self.current_gate().map_or(1, |gate| gate.pulses),
        });
        self.set_field(FieldValue::Gate(gate));
    }

    /// List the FX commands this track has, in categories as `Dest` is: the filter, output, the
    /// voice, each slot's effect, then the events. It opens on its search, so typing finds a label or
    /// a name at once, and `Esc` goes to the list. A query starts it searching for that.
    pub(crate) fn open_fx_picker(&mut self, slot: usize, query: &str) {
        let current = self.current_slot(slot).and_then(|s| s.command);
        let track = self.song().tracks.get(self.cursor.track);
        // Only what this track has, from its machine and its slots.
        let commands: Vec<FxCommand> =
            track.map_or_else(|| FxCommand::EVENTS.to_vec(), FxCommand::on_track);
        let empty = Item::new(
            "---",
            Pick::FxCommand {
                slot,
                command: None,
            },
        )
        .meaning("Empty");
        let items = std::iter::once(empty)
            .chain(commands.iter().map(|command| {
                let (name, category) = match (command, track) {
                    (FxCommand::Set(id), Some(track)) => (id.name_on(track), id.group()),
                    (FxCommand::Set(id), None) => (command.meaning(), id.group()),
                    _ => (command.meaning(), "Events".to_owned()),
                };
                Item::new(
                    command.name(),
                    Pick::FxCommand {
                        slot,
                        command: Some(*command),
                    },
                )
                .meaning(name)
                .category(category)
            }))
            .collect();
        let position = commands
            .iter()
            .position(|command| Some(*command) == current)
            .map_or(0, |position| position + 1);
        self.show_picker(
            Picker::new("FX commands", items)
                .opened_on(Some(position))
                .searching(query),
        );
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
                    mark: self.mark_for_typed_value(slot),
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

    /// Choose this track's sample from the usual places.
    pub(crate) fn open_default_sample_picker(&mut self) {
        let roots = self.sample_roots();
        self.open_samples(&roots, true);
    }

    /// Open the sample browser on a folder. If nothing is there, offer a typed path instead.
    pub(crate) fn open_sample_picker(&mut self, roots: &[PathBuf]) {
        self.open_samples(roots, false);
    }

    /// Open the sample browser, with the kit's sounds first if asked.
    fn open_samples(&mut self, roots: &[PathBuf], with_kit: bool) {
        let kit_name = self
            .kit_file
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kit = self
            .kit
            .iter()
            .filter(|_| with_kit)
            .map(|entry| kit_item(entry, &kit_name));
        let files = roots.iter().flat_map(|root| {
            find_samples(root).into_iter().map(move |path| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let category = folder_in(root, &path);
                Item::new(name, Pick::Sample(path)).category(category)
            })
        });
        let items: Vec<Item> = kit.chain(files).collect();
        if items.is_empty() {
            self.status = "No samples found here: type a path, or a folder of them".to_owned();
            self.command_line = "sample ".to_owned();
            self.mode = Mode::Command;
            return;
        }
        // On the track's own sample, or else the kit's first sound. Only files of the same name
        // are looked at closely, as a path can be written more than one way.
        let sample = self
            .song()
            .tracks
            .get(self.cursor.track)
            .and_then(|track| track.sample.as_deref())
            .map(|sample| project::resolve_sample(self.folder.as_deref(), sample));
        let same = |path: &Path| {
            sample.as_deref().is_some_and(|sample| {
                path.file_name() == sample.file_name()
                    && (path == sample
                        || path
                            .canonicalize()
                            .is_ok_and(|path| sample.canonicalize().is_ok_and(|s| s == path)))
            })
        };
        let current = items.iter().position(|item| match &item.pick {
            Pick::Sample(path) | Pick::KitSound { path, .. } => same(path),
            _ => false,
        });
        let first_sound = (current.is_none() && !self.kit.is_empty() && with_kit).then_some(0);
        // Files are found by name, whatever letter it starts with, so it opens on the search.
        self.show_picker(
            Picker::new("Samples", items)
                .opened_on(current)
                .revealing(first_sound)
                .searching(""),
        );
    }

    /// The folders the sample browser starts from: the project's samples, the settings' sample
    /// folders, then where we ran.
    pub(crate) fn sample_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(folder) = &self.folder {
            roots.push(folder.join("samples"));
        }
        for folder in self.settings.sample_folders() {
            if !roots.contains(&folder) {
                roots.push(folder);
            }
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
        match picker.key(key) {
            Outcome::Stay => self.refresh_preview(),
            Outcome::Close => {
                self.picker = None;
                self.mode = self.mode_after_picker();
            }
            Outcome::Choose(pick) => {
                self.picker = None;
                self.mode = self.mode_after_picker();
                self.choose(pick);
            }
        }
    }

    /// Do what a picker's choice says.
    fn choose(&mut self, pick: Pick) {
        match pick {
            Pick::View(view) => self.jump_to_view(view),
            Pick::Use {
                section,
                track,
                slot,
            } => self.open_use(section, track, slot),
            Pick::Sample(path) => self.sample(&path),
            Pick::Kit(path) => self.use_kit(&path),
            Pick::KitSound {
                path,
                label,
                level,
                pitch,
                choke,
            } => {
                let track = self.cursor.track;
                let mut more = vec![
                    Action::SetTrackLevel { track, level },
                    Action::SetTrackBase {
                        track,
                        control: TrackControl::Pitch,
                        value: pitch,
                    },
                    Action::SetTrackChoke { track, choke },
                ];
                if !label.is_empty() {
                    more.push(Action::SetTrackName { track, name: label });
                }
                self.sample_with(&path, more);
            }
            Pick::Track(track) => self.go_to_track(track),
            Pick::Audio(choice) => self.pick_audio(choice),
            // The list above the command line takes its own choices, so none comes here.
            Pick::Completion(_) => {}
            Pick::FxCommand { slot, command } => self.pick_fx_command(slot, command),
            Pick::Gate(kind) => self.pick_gate(kind),
            Pick::Lfo { track, slot, lfo } => {
                self.apply(&Action::SetLfo { track, slot, lfo });
            }
            Pick::Machine { track, machine } => {
                self.apply(&Action::SetTrackMachine { track, machine });
            }
            Pick::Setting {
                track,
                setting,
                value,
            } => {
                self.apply(&Action::SetTrackSetting {
                    track,
                    setting,
                    value,
                });
            }
            Pick::FilterType { track, filter } => {
                self.apply(&Action::SetFilterType { track, filter });
            }
            Pick::FilterMode { track, mode } => {
                self.apply(&Action::SetFilterMode { track, mode });
            }
            Pick::Envelope {
                track,
                slot,
                envelope,
            } => {
                self.apply(&Action::SetEnvelope {
                    track,
                    slot,
                    envelope,
                });
            }
            Pick::RowPhrase(phrase) => self.pick_row_phrase(phrase),
            Pick::SectionChain(chain) => self.pick_section_chain(chain),
            Pick::Insert {
                track,
                slot,
                insert,
            } => self.apply(&Action::SetInsert {
                track,
                slot,
                insert,
            }),
            Pick::Scale(kind) => self.set_scale(Scale {
                on: true,
                kind,
                ..self.scale
            }),
            Pick::ShapeFill(kind) => self.pick_shape_fill(kind),
            Pick::ShapeWave(wave) => self.pick_shape_wave(wave),
            Pick::ShapeRate(rate) => self.pick_shape_rate(rate),
            Pick::ShapeWhere(place) => self.pick_shape_where(place),
            Pick::Guide(guide) => {
                self.guide = Some(guide);
                self.guide_scroll = 0;
                self.mode = Mode::Guide;
            }
            Pick::KitTracks(path) => self.load_kit(&path),
        }
    }

    /// Record a view as most recently used.
    pub(crate) fn note_view(&mut self, view: View) {
        self.recent_views.retain(|other| *other != view);
        self.recent_views.insert(0, view);
    }
}
