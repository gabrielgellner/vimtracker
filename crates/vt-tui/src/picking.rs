//! Opening the picker and acting on what it chooses.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, FieldValue, KitSound};
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
use crate::input::picker::{Focus, Item, Pick, Picker};
use crate::song_view::COLUMNS_BEFORE_TRACKS;

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

/// A kit's name: its file name without the extension.
fn kit_name(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Where kits are looked for by name, from where the program ran.
const KITS_DIR: &str = "samples/kits";

/// A kit named in `samples/kits`, or at a path.
pub(crate) fn kit_path(name: &Path) -> PathBuf {
    if name.is_file() {
        name.to_path_buf()
    } else {
        Path::new(KITS_DIR).join(name).with_extension("ron")
    }
}

/// A row for a kit's sound: its name and its file.
fn kit_item(entry: &KitEntry) -> Item {
    let file = entry
        .path
        .file_name()
        .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
    Item {
        label: format!("{:<4}{file}", entry.name),
        pick: Pick::KitSound {
            path: entry.path.clone(),
            name: entry.name.clone(),
        },
    }
}

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

    /// Open the list of scales, with this one highlighted.
    pub(crate) fn open_scale_picker(&mut self) {
        self.count = None;
        let items = ScaleKind::ALL
            .map(|kind| Item {
                label: kind.name().to_owned(),
                pick: Pick::Scale(kind),
            })
            .to_vec();
        let at = ScaleKind::ALL
            .iter()
            .position(|kind| *kind == self.scale.kind)
            .unwrap_or(0);
        let mut picker = Picker::new("Scales", items);
        picker.move_highlight(at as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
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
                Item {
                    label: format!("{number:<2}  {:<4}{sound}", state.name),
                    pick: Pick::Track(track),
                }
            })
            .collect();
        let mut picker = Picker::new("Tracks", items);
        picker.move_highlight(self.cursor.track as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
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

    /// Make a kit's sounds the ones to choose from for a track. Nothing is loaded until one is.
    pub(crate) fn use_kit(&mut self, path: &Path) {
        match load_kit(path) {
            Ok(entries) if entries.is_empty() => {
                self.status = format!("{} has no sounds", path.display());
            }
            Ok(entries) => {
                self.status = format!(
                    "Kit {}: {} sounds. Space 5 chooses one for this track",
                    kit_name(path),
                    entries.len()
                );
                self.kit = entries;
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Put a kit's sounds on tracks from the first, keeping their other settings and adding
    /// tracks if needed. It is one undo step, and if any sound won't load, nothing changes.
    /// Replacing a different sample is reported.
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
        let mut sounds = Vec::with_capacity(entries.len());
        for entry in &entries {
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
                name: entry.name.clone(),
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
        self.status = format!("Kit {}: {count} tracks", kit_name(path));
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
        let empty = Item {
            label: "---  Empty, one pulse of SGL".to_owned(),
            pick: Pick::Gate(None),
        };
        let items = std::iter::once(empty)
            .chain(GateKind::ALL.iter().map(|kind| Item {
                label: format!("{}  {}", kind.name(), kind.meaning()),
                pick: Pick::Gate(Some(*kind)),
            }))
            .collect();
        let position = GateKind::ALL
            .iter()
            .position(|kind| Some(*kind) == current)
            .map_or(0, |position| position + 1);
        let mut picker = Picker::new("Gates", items);
        picker.move_highlight(position as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Put a chosen gate type in the cell, keeping its pulses, or empty it.
    fn pick_gate(&mut self, kind: Option<GateKind>) {
        let gate = kind.map(|kind| Gate {
            kind,
            pulses: self.current_gate().map_or(1, |gate| gate.pulses),
        });
        self.set_field(FieldValue::Gate(gate));
    }

    /// List the FX commands this track has, grouped as `Dest` is: the filter, output, the voice,
    /// each slot's effect, then the events. It opens on its search, so typing finds a label or a
    /// name at once, and `/` goes to the groups. A query starts it searching for that.
    pub(crate) fn open_fx_picker(&mut self, slot: usize, query: &str) {
        let current = self.current_slot(slot).and_then(|s| s.command);
        let track = self.song().tracks.get(self.cursor.track);
        // Only what this track has, from its machine and its slots.
        let commands: Vec<FxCommand> =
            track.map_or_else(|| FxCommand::EVENTS.to_vec(), FxCommand::on_track);
        let empty = Item {
            label: "---   Empty".to_owned(),
            pick: Pick::FxCommand {
                slot,
                command: None,
            },
        };
        let items = std::iter::once(empty)
            .chain(commands.iter().map(|command| {
                let name = match (command, track) {
                    (FxCommand::Set(id), Some(track)) => id.name_on(track),
                    _ => command.meaning(),
                };
                Item {
                    label: format!("{:<6}{name}", command.name()),
                    pick: Pick::FxCommand {
                        slot,
                        command: Some(*command),
                    },
                }
            }))
            .collect();
        let groups = std::iter::once("---".to_owned())
            .chain(commands.iter().map(|command| match command {
                FxCommand::Set(id) => id.group(),
                _ => "Events".to_owned(),
            }))
            .collect();
        let position = commands
            .iter()
            .position(|command| Some(*command) == current)
            .map_or(0, |position| position + 1);
        let mut picker = Picker::grouped("FX commands", items, groups, position);
        picker.toggle_search();
        if !query.is_empty() {
            picker.set_query(query.to_owned());
        }
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
        let kit = self.kit.iter().filter(|_| with_kit).map(kit_item);
        let files = roots.iter().flat_map(|root| {
            find_samples(root).into_iter().map(move |path| Item {
                label: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                pick: Pick::Sample(path),
            })
        });
        let items: Vec<Item> = kit.chain(files).collect();
        if items.is_empty() {
            self.status = "No samples found here: type a path, or a folder of them".to_owned();
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
        if picker.has_groups() {
            match (picker.focus, key.code) {
                (_, KeyCode::Char('/')) => return picker.toggle_search(),
                (Focus::List, KeyCode::Char('j')) => return picker.move_highlight(1),
                (Focus::List, KeyCode::Char('k')) => return picker.move_highlight(-1),
                (Focus::List, KeyCode::Char('h')) => {
                    picker.focus = Focus::Groups;
                    return;
                }
                (Focus::Groups, KeyCode::Char('j')) => return picker.move_group(1),
                (Focus::Groups, KeyCode::Char('k')) => return picker.move_group(-1),
                (Focus::Groups, KeyCode::Char('l')) => {
                    picker.focus = Focus::List;
                    return;
                }
                // Into a group of more than one, and a lone item is chosen below.
                (Focus::Groups, KeyCode::Enter) if picker.in_group() > 1 => {
                    picker.focus = Focus::List;
                    return;
                }
                // Browsing doesn't type.
                (Focus::List | Focus::Groups, KeyCode::Char(_)) => return,
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => {
                self.picker = None;
                self.mode = self.mode_after_picker();
            }
            KeyCode::Enter => {
                let pick = picker.current().map(|item| item.pick.clone());
                self.picker = None;
                self.mode = self.mode_after_picker();
                match pick {
                    Some(Pick::View(view)) => self.jump_to_view(view),
                    Some(Pick::Use {
                        section,
                        track,
                        slot,
                    }) => self.open_use(section, track, slot),
                    Some(Pick::Sample(path)) => self.sample(&path),
                    Some(Pick::Kit(path)) => self.use_kit(&path),
                    Some(Pick::KitSound { path, name }) => {
                        let name = (!name.is_empty()).then_some(name.as_str());
                        self.sample_named(&path, name);
                    }
                    Some(Pick::Track(track)) => self.go_to_track(track),
                    Some(Pick::Audio(choice)) => self.pick_audio(choice),
                    Some(Pick::FxCommand { slot, command }) => self.pick_fx_command(slot, command),
                    Some(Pick::Gate(kind)) => self.pick_gate(kind),
                    Some(Pick::Lfo { track, slot, lfo }) => {
                        self.apply(&Action::SetLfo { track, slot, lfo });
                    }
                    Some(Pick::Machine { track, machine }) => {
                        self.apply(&Action::SetTrackMachine { track, machine });
                    }
                    Some(Pick::Setting {
                        track,
                        setting,
                        value,
                    }) => {
                        self.apply(&Action::SetTrackSetting {
                            track,
                            setting,
                            value,
                        });
                    }
                    Some(Pick::FilterType { track, filter }) => {
                        self.apply(&Action::SetFilterType { track, filter });
                    }
                    Some(Pick::Envelope {
                        track,
                        slot,
                        envelope,
                    }) => {
                        self.apply(&Action::SetEnvelope {
                            track,
                            slot,
                            envelope,
                        });
                    }
                    Some(Pick::RowPhrase(phrase)) => self.pick_row_phrase(phrase),
                    Some(Pick::SectionChain(chain)) => self.pick_section_chain(chain),
                    Some(Pick::Insert {
                        track,
                        slot,
                        insert,
                    }) => self.apply(&Action::SetInsert {
                        track,
                        slot,
                        insert,
                    }),
                    Some(Pick::Scale(kind)) => self.set_scale(Scale {
                        on: true,
                        kind,
                        ..self.scale
                    }),
                    Some(Pick::ShapeFill(kind)) => self.pick_shape_fill(kind),
                    Some(Pick::ShapeWave(wave)) => self.pick_shape_wave(wave),
                    Some(Pick::ShapeRate(rate)) => self.pick_shape_rate(rate),
                    Some(Pick::ShapeWhere(place)) => self.pick_shape_where(place),
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
