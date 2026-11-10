//! Opening the picker and acting on what it chooses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vt_model::accumulator::{MOST_DEGREES, amount, amount_text, amount_value};
use vt_model::action::{Action, FieldValue};
use vt_model::bank::{BankEntry, PlayParams};
use vt_model::condition::Condition;
use vt_model::groove::GrooveId;
use vt_model::kit::{KitEntry, load_kit};
use vt_model::order::Order;
use vt_model::sample::load_sample;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::settings::SettingId;
use vt_model::song::Machine;
use vt_model::step::{FxCommand, FxSlot, Gate, GateKind, Mark};

use crate::app::{App, Entry, Field};
use crate::banking::Seed;
use crate::explorer::Purpose;
use crate::guides::GUIDES;
use crate::input::command::Line;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Outcome, Pick, Picker, Preview, PreviewLine, Shown};
use crate::kitty;
use crate::song_view::COLUMNS_BEFORE_TRACKS;
use crate::views::picker::{PREVIEW_WAVE_HEIGHT, PREVIEW_WAVE_WIDTH};

/// A row for a kit's sound: its label, and its name beside it, or its name alone if it has no
/// label. Moving onto it plays it and shows its waveform, as a file's row does.
pub(crate) fn kit_item(entry: &KitEntry) -> Item {
    let (label, meaning) = if entry.label.is_empty() {
        (entry.name.clone(), String::new())
    } else {
        (entry.label.clone(), entry.name.clone())
    };
    Item::new(
        label,
        Pick::KitSound {
            path: entry.path.clone(),
            label: entry.label.clone(),
            name: entry.name.clone(),
            play: PlayParams {
                gain_db: entry.gain,
                tune: entry.tune,
                ..PlayParams::default()
            },
            choke: entry.choke,
        },
    )
    .meaning(meaning)
    .preview(Preview::Sample(entry.path.clone()))
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
            let heading = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Shown::new(item, heading, error.to_string());
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
        lines,
        ..Shown::new(item, kit.name, note)
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
    /// shown. A sample's is read in the background, and shows it's loading until it's in.
    pub(crate) fn refresh_preview(&mut self) {
        let rate = self.rate;
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let wanted = picker
            .current_index()
            .filter(|item| picker.has_previews() && picker.item(*item).preview.is_some());
        if picker.preview.as_ref().map(|shown| shown.item) == wanted {
            return;
        }
        picker.preview_images.clear();
        picker.preview_wave = None;
        let Some(item) = wanted else {
            picker.preview = None;
            return;
        };
        let Some(preview) = picker.item(item).preview.clone() else {
            return;
        };
        let shown = match &preview {
            Preview::Kit(path) => Some(kit_preview(item, path, rate, &mut picker.waves)),
            _ => self.explorer_preview(item, &preview),
        };
        if let Some(picker) = self.picker.as_mut() {
            picker.preview = shown.map(Arc::new);
        }
    }

    /// Open the list of grooves for the project, or for the cursor's track, which can also
    /// follow the project's, with the one playing highlighted.
    pub(crate) fn open_groove_picker(&mut self, track: bool) {
        self.count = None;
        let song = self.song();
        let own = song.tracks.get(self.cursor.track).and_then(|t| t.groove);
        let mut items = Vec::new();
        if track {
            let follow = format!("Project's, {}", song.groove);
            items.push(
                Item::new(follow, Pick::Line(Line::TrackGroove(None)))
                    .meaning("The groove the project plays with"),
            );
        }
        for id in GrooveId::all() {
            let line = if track {
                Line::TrackGroove(Some(id))
            } else {
                Line::Groove(id)
            };
            items.push(Item::new(id.name(), Pick::Line(line)).meaning(id.meaning()));
        }
        let (title, at) = if track {
            let at = own.map_or(0, |id| id.index() + 1);
            ("This track's groove", at)
        } else {
            ("The project's groove", song.groove.index())
        };
        self.show_picker(Picker::new(title, items).opened_on(Some(at)));
    }

    /// Open the list of orders the cursor's phrase's rows can play in, on the one they play in.
    pub(crate) fn open_order_picker(&mut self) {
        self.count = None;
        let Some(phrase) = self.phrase_here() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        let at = Order::ALL.iter().position(|order| *order == phrase.order());
        let items = Order::ALL
            .map(|order| {
                Item::new(order.name(), Pick::Line(Line::Order(Some(order))))
                    .meaning(order.meaning())
            })
            .to_vec();
        self.show_picker(Picker::new("This phrase's order", items).opened_on(at));
    }

    /// Play the cursor's phrase's rows in an order.
    pub(crate) fn set_order(&mut self, order: Order) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        self.apply(&Action::SetOrder {
            track: self.cursor.track,
            phrase,
            order,
        });
        self.status = format!("This phrase plays {}: {}", order, order.meaning());
    }

    /// Give the cursor's track its own groove, or have it follow the project's again.
    pub(crate) fn set_track_groove(&mut self, groove: Option<GrooveId>) {
        let track = self.cursor.track;
        if !self.song().tracks.get(track).is_some_and(|t| t.is_sound()) {
            self.status = "Only a track with notes has a groove of its own".to_owned();
            return;
        }
        self.apply(&Action::SetTrackGroove { track, groove });
        self.status = match groove {
            Some(groove) => format!("This track plays with {groove}"),
            None => format!("This track follows the project's {}", self.song().groove),
        };
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
                    _ if !state.is_sound() => state.kind.title().to_owned(),
                    Machine::Sampler => self
                        .song()
                        .bank
                        .get(state.sample())
                        .map_or_else(String::new, BankEntry::title),
                    Machine::Plaits(voice) => voice.name().to_owned(),
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

    /// Choose a kit in the explorer, from the kit folders, with each kit's sounds beside the list.
    /// `Enter` makes it the kit `Space 5` lists first, and `L` puts its sounds on tracks.
    pub(crate) fn open_kit_picker(&mut self) {
        self.open_kit_explorer();
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

    /// Choose this field from a list, in the columns that are one.
    pub(crate) fn open_field_picker(&mut self) {
        match self.cursor.field {
            // A condition's value is one of a list, so `K` on it lists them.
            Field::FxValue(slot)
                if self.current_slot(slot).and_then(|s| s.command) == Some(FxCommand::Cnd) =>
            {
                self.open_condition_picker(slot);
            }
            // So is an amount, as a signed byte is hard to type.
            Field::FxValue(slot)
                if self.current_slot(slot).and_then(|s| s.command) == Some(FxCommand::Acu) =>
            {
                self.open_amount_picker(slot);
            }
            // And a sample lock's is one of the bank's.
            Field::FxValue(slot)
                if self.current_slot(slot).and_then(|s| s.command)
                    == Some(FxCommand::Set(SettingId::SAMPLE)) =>
            {
                self.open_sample_lock_picker(slot);
            }
            // And a slice lock's is one of the slices of the sample the step plays.
            Field::FxValue(slot)
                if self.current_slot(slot).and_then(|s| s.command)
                    == Some(FxCommand::Set(SettingId::SLICE)) =>
            {
                self.open_slice_lock_picker(slot);
            }
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                self.open_fx_picker(slot, "");
            }
            Field::Gate | Field::GateLength => self.open_gate_picker(),
            Field::Note | Field::Velocity | Field::Nudge => {
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

    /// Put a chosen gate type in the cell, keeping its pulses and length, or empty it.
    fn pick_gate(&mut self, kind: Option<GateKind>) {
        let current = self.current_gate().unwrap_or_default();
        let gate = kind.map(|kind| Gate { kind, ..current });
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
                    (FxCommand::Set(id), Some(track)) => (id.name_on(track), id.group_on(track)),
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
    /// to be typed, as a command with no value yet doesn't take effect, and a `CND` lists its
    /// conditions to choose from and an `ACU` its amounts, unless the slot's value already is one.
    fn pick_fx_command(&mut self, slot: usize, command: Option<FxCommand>) {
        let Some(command) = command else {
            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
            self.cursor.field = Field::FxCommand(slot);
            return;
        };
        self.cursor.field = Field::FxValue(slot);
        let current = self.current_slot(slot);
        let has_condition = current
            .filter(|s| s.command.is_some())
            .is_some_and(|s| Condition::from_value(s.coarse()).is_some());
        if command == FxCommand::Cnd && !has_condition {
            self.open_condition_picker(slot);
            return;
        }
        let has_amount = current
            .filter(|s| s.command.is_some())
            .is_some_and(|s| amount(s.coarse()).is_some_and(|amount| amount != 0));
        if command == FxCommand::Acu && !has_amount {
            self.open_amount_picker(slot);
            return;
        }
        let has_value = current.is_some_and(|s| s.command.is_some());
        if command == FxCommand::Set(SettingId::SAMPLE) && !has_value {
            self.open_sample_lock_picker(slot);
            return;
        }
        if command == FxCommand::Set(SettingId::SLICE) && !has_value {
            self.open_slice_lock_picker(slot);
            return;
        }
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

    /// List the conditions a `CND` can hold, in Elektron's order and by Elektron's names, in
    /// groups, opening on the slot's own and on its search, as the FX commands' list does.
    pub(crate) fn open_condition_picker(&mut self, slot: usize) {
        let current = self
            .current_slot(slot)
            .filter(|s| s.command == Some(FxCommand::Cnd))
            .and_then(|s| Condition::from_value(s.coarse()));
        let items = Condition::all()
            .map(|condition| {
                Item::new(
                    condition.label(),
                    Pick::Condition {
                        slot,
                        value: condition.value(),
                    },
                )
                .meaning(format!(
                    "{:02X}  {}",
                    condition.value(),
                    condition.meaning()
                ))
                .category(condition.group())
            })
            .collect();
        let position = Condition::all().position(|condition| Some(condition) == current);
        self.show_picker(
            Picker::new("Conditions", items)
                .opened_on(position)
                .searching(""),
        );
    }

    /// List how far an `ACU` can move its note each time, up and then down, in degrees or semitones
    /// as the scale is on or off, opening on the slot's own, or on one up.
    pub(crate) fn open_amount_picker(&mut self, slot: usize) {
        let current = self
            .current_slot(slot)
            .filter(|s| s.command == Some(FxCommand::Acu))
            .and_then(|s| amount(s.coarse()))
            .filter(|amount| *amount != 0)
            .unwrap_or(1);
        let most = MOST_DEGREES as i8;
        let amounts: Vec<i8> = (1..=most).chain((1..=most).map(|down| -down)).collect();
        let items = amounts
            .iter()
            .map(|&amount| {
                let value = amount_value(amount);
                Item::new(format!("{amount:+}"), Pick::Amount { slot, value })
                    .meaning(format!(
                        "{value:02X}  {}",
                        capitalised(&amount_text(amount, self.scale.on))
                    ))
                    .category(if amount > 0 { "Up" } else { "Down" })
            })
            .collect();
        let position = amounts.iter().position(|amount| *amount == current);
        self.show_picker(
            Picker::new("Accumulate", items)
                .opened_on(position)
                .searching(""),
        );
    }

    /// Put a chosen amount in a slot as an `ACU`.
    fn pick_amount(&mut self, slot: usize, value: u8) {
        self.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(
            slot,
            FxSlot {
                command: Some(FxCommand::Acu),
                value: u16::from(value) << 8,
                mark: Mark::default(),
            },
        ));
    }

    /// Put a chosen condition in a slot as a `CND`.
    fn pick_condition(&mut self, slot: usize, value: u8) {
        self.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(
            slot,
            FxSlot {
                command: Some(FxCommand::Cnd),
                value: u16::from(value) << 8,
                mark: Mark::default(),
            },
        ));
    }

    /// Choose this track's sample in the explorer, with the kit's sounds first.
    pub(crate) fn open_default_sample_picker(&mut self) {
        let kit_name = self
            .kit_file
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| format!("Kit {}", stem.to_string_lossy()))
            .unwrap_or_else(|| "Kit".to_owned());
        let kit = self.kit.iter().map(kit_item).collect();
        self.open_sample_explorer(kit, kit_name);
    }

    pub(crate) fn picker_key(&mut self, key: Key) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match picker.key(key) {
            Outcome::Stay => {
                self.refresh_preview();
                self.follow_file();
            }
            Outcome::Close => {
                self.picker = None;
                self.mode = self.mode_after_picker();
                self.leave_explorer();
            }
            Outcome::Choose(pick) => {
                let naming = self.naming.take();
                self.picker = None;
                self.mode = self.mode_after_picker();
                self.leave_explorer();
                self.naming = naming;
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
            Pick::Sample(path) => {
                self.note_place_of(Purpose::Sample, &path);
                self.sample(&path);
            }
            Pick::Kit(path) => {
                self.note_place_of(Purpose::Kit, &path);
                self.use_kit(&path);
            }
            Pick::Folder(folder) => self.name_in(&folder),
            Pick::Project(folder) => {
                self.note_place_of(Purpose::Project, &folder);
                self.line(crate::input::command::Line::Edit(Some(folder)));
            }
            Pick::KitSound {
                path,
                label,
                name,
                play,
                choke,
            } => {
                let track = self.cursor.track;
                let mut more = vec![Action::SetTrackChoke { track, choke }];
                if !label.is_empty() {
                    more.push(Action::SetTrackName {
                        track,
                        name: label.clone(),
                    });
                }
                let seed = Seed { label, name, play };
                self.sample_with(&path, Some(&seed), more);
            }
            Pick::BankSample { track, number } => {
                self.apply(&Action::SetTrackSample {
                    track,
                    sample: number,
                });
            }
            Pick::SampleLock { slot, number } => self.pick_sample_lock(slot, number),
            Pick::SliceLock { slot, number } => self.pick_slice_lock(slot, number),
            Pick::SliceMarkLock { slot, letter } => self.pick_slice_mark_lock(slot, letter),
            Pick::Mark(letter) => self.jump_to_mark(letter),
            Pick::SampleSetting(setting) => self.pick_sample_setting(setting),
            Pick::SampleValue { setting, value } => self.pick_sample_value(setting, value),
            Pick::Track(track) => self.go_to_track(track),
            Pick::Audio(choice) => self.pick_audio(choice),
            // The list above the command line takes its own choices, so none comes here.
            Pick::Completion(_) => {}
            Pick::FxCommand { slot, command } => self.pick_fx_command(slot, command),
            Pick::Gate(kind) => self.pick_gate(kind),
            Pick::Condition { slot, value } => self.pick_condition(slot, value),
            Pick::Amount { slot, value } => self.pick_amount(slot, value),
            Pick::Accumulation {
                track,
                accumulation,
            } => {
                self.apply(&Action::SetAccumulation {
                    track,
                    accumulation,
                });
            }
            Pick::Arpeggio { track, arpeggio } => {
                self.apply(&Action::SetArpeggio { track, arpeggio });
            }
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
            Pick::Speed { track, speed } => {
                self.apply(&Action::SetTrackSpeed { track, speed });
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
            Pick::Line(line) => self.line(line),
            Pick::ScaleTo(scale) => self.set_scale(scale),
            Pick::ShowGroove(groove) => self.groove_shown = Some(groove),
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
            Pick::KitTracks(path) => {
                self.note_place_of(Purpose::Kit, &path);
                self.load_kit(&path);
            }
        }
    }

    /// Remember the folder a file chosen in the explorer is in.
    fn note_place_of(&mut self, purpose: Purpose, path: &Path) {
        if let Some(folder) = path.parent() {
            self.note_place(purpose, folder);
        }
    }

    /// Record a view as most recently used.
    pub(crate) fn note_view(&mut self, view: View) {
        self.recent_views.retain(|other| *other != view);
        self.recent_views.insert(0, view);
    }
}

/// Text with its first letter a capital, as a list's meanings start.
fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}
