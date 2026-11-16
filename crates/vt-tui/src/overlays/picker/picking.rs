//! Showing the picker, its keys, and acting on what it chooses. The lists it opens on are built
//! beside what they're for: the phrase cell's in `pages/phrase/pickers.rs`, the grooves and scales
//! in `pages/project/pickers.rs`, the views in `app/keys.rs`, the tracks in `app/tracks.rs`, the
//! guides in `overlays/guides.rs`, and the kits and the samples in the explorer's `exploring.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;

use vt_model::accumulator::amount;
use vt_model::action::{Action, FieldValue};
use vt_model::condition::Condition;
use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot};
use vt_model::scale::Scale;
use vt_model::selection::Field;
use vt_model::settings::SettingId;
use vt_model::song::Role;
use vt_model::step::{FxCommand, FxSlot, Gate, GateKind, Mark};

use crate::app::{App, Entry};
use crate::controls::{Control, set};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::overlays;
use crate::overlays::picker::explorer::Purpose;
use crate::overlays::picker::{KitChoice, Pick, Picker};
use crate::pages::chain::ChainView;
use crate::pages::phrase::PhraseView;
use crate::pages::song::COLUMNS_BEFORE_TRACKS;
use crate::pages::track::tuning::set_tuning;
use crate::pages::{CtxMut, Outcome};
use crate::services::banking::Seed;

impl App {
    /// Show a picker, and its preview if it has one.
    pub(crate) fn show_picker(&mut self, picker: Picker) {
        self.overlays.picker = Some(picker);
        self.modes.mode = Mode::Picker;
        self.overlays.refresh_preview(self.services.rate);
    }

    /// Show a picker a key or a command line opened, which takes the count.
    pub(crate) fn open_picker(&mut self, picker: Picker) {
        self.modes.count = None;
        self.show_picker(picker);
    }

    /// Go to a track chosen from the list, which `C-o` comes back from.
    fn go_to_track(&mut self, track: usize) {
        self.record_jump();
        // A track folded away is shown again, as Vim opens a fold a jump lands in, and the song
        // keeps the fold opened.
        let song = Arc::clone(self.song());
        let was = self.services.folds;
        self.services.folds.open_over(&song, track);
        self.folds_changed(was);
        self.services.go_to_track_at(track, self.modes.view);
        self.song_view.column = COLUMNS_BEFORE_TRACKS + track;
    }

    /// Put a chosen gate type in the cell, keeping its pulses and length, or empty it.
    fn pick_gate(&mut self, kind: Option<GateKind>) {
        let current = self.services.current_gate().unwrap_or_default();
        let gate = kind.map(|kind| Gate { kind, ..current });
        self.set_field(FieldValue::Gate(gate));
    }

    /// Put a chosen command in a slot. A slot with a value keeps it. An empty one waits for its value
    /// to be typed, as a command with no value yet doesn't take effect, and a `CND` lists its
    /// conditions to choose from and an `ACU` its amounts, unless the slot's value already is one.
    fn pick_fx_command(&mut self, slot: usize, command: Option<FxCommand>) {
        let Some(command) = command else {
            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
            self.services.cursor.field = Field::FxCommand(slot);
            return;
        };
        self.services.cursor.field = Field::FxValue(slot);
        let current = self.services.current_slot(slot);
        let has_condition = current
            .filter(|s| s.command.is_some())
            .is_some_and(|s| Condition::from_value(s.coarse()).is_some());
        if command == FxCommand::Cnd && !has_condition {
            self.show_picker(PhraseView::condition_picker(&self.services, slot));
            return;
        }
        let has_amount = current
            .filter(|s| s.command.is_some())
            .is_some_and(|s| amount(s.coarse()).is_some_and(|amount| amount != 0));
        if command == FxCommand::Acu && !has_amount {
            self.show_picker(PhraseView::amount_picker(&self.services, slot));
            return;
        }
        let has_value = current.is_some_and(|s| s.command.is_some());
        if command == FxCommand::Set(SettingId::SAMPLE) && !has_value {
            self.open_picker(PhraseView::sample_lock_picker(&self.services, slot));
            return;
        }
        if command == FxCommand::Set(SettingId::SLICE) && !has_value {
            self.open_picker(PhraseView::slice_lock_picker(&self.services, slot));
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
        self.modes.entry = Entry {
            command: Some(command),
            ..Entry::default()
        };
        self.grammar.replacing = Some(Field::FxValue(slot));
        self.modes.mode = Mode::Replace;
    }

    /// Set one of a track's settings to one of its choices, as its view writes it.
    fn pick_param(&mut self, track: usize, control: Control, value: i32) {
        let song = Arc::clone(self.song());
        let action = song
            .tracks
            .get(track)
            .and_then(|found| set(control, track, found, &song, value, None));
        if let Some(action) = action {
            self.services.apply(&action);
        }
    }

    /// Replace one of a track's mod slots with what was chosen. A new type keeps the slot's
    /// destination, depth and switch, and finds its own settings as the slot last had them.
    fn pick_mod(&mut self, track: usize, slot: usize, mut modulator: ModSlot) {
        let was = self
            .song()
            .tracks
            .get(track)
            .map(|found| found.mods[slot].kind);
        let changed = was.is_some_and(|kind| kind != modulator.kind);
        // Steps' values are heard as written at `+FF`, so one with no depth is given that, in the
        // same undo step, rather than playing nothing.
        let given = changed && modulator.kind == ModKind::Steps && modulator.depth == 0;
        if given {
            modulator.depth = MAX_DEPTH;
        }
        self.services.apply(&Action::SetMod {
            track,
            slot,
            modulator,
        });
        if given {
            self.services.status = format!(
                "Mod {} is Steps now, at a depth of +FF so its values are heard as written",
                slot + 1
            );
        } else if changed {
            self.services.status = format!(
                "Mod {} is {} now, with its destination, depth and switch kept",
                slot + 1,
                modulator.kind.name()
            );
        }
    }

    /// Put a chosen amount in a slot as an `ACU`.
    fn pick_amount(&mut self, slot: usize, value: u8) {
        self.services.cursor.field = Field::FxValue(slot);
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
        self.services.cursor.field = Field::FxValue(slot);
        self.set_field(FieldValue::Slot(
            slot,
            FxSlot {
                command: Some(FxCommand::Cnd),
                value: u16::from(value) << 8,
                mark: Mark::default(),
            },
        ));
    }

    /// Give the open picker a key, through the overlays' seam, and settle what it asks for.
    pub(crate) fn picker_key(&mut self, key: Key, now: Instant) {
        let Some(picker) = self.overlays.picker.as_mut() else {
            self.modes.mode = Mode::Normal;
            return;
        };
        // A digit chooses a macro at once, as `M5` does.
        if picker.title == crate::overlays::mapping::MACRO_PICKER
            && let KeyCode::Char(digit @ '1'..='8') = key.code
        {
            self.overlays.picker = None;
            self.grammar.pending.clear();
            self.grammar.pending_since = None;
            self.modes.mode = self.mode_after_picker();
            self.map_to_macro(usize::from(digit as u8 - b'1'));
            return;
        }
        // A key with Alt or Cmd means nothing to it, and leaves a `g` or a `z` waiting, and
        // Shift on an arrow is the arrow.
        if key.alt || key.command {
            self.overlays.refresh_preview(self.services.rate);
            self.follow_file();
            return;
        }
        let key = Key {
            shift: false,
            ..key
        };
        let was_waiting = !self.grammar.pending.is_empty();
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        let pending = &mut self.grammar.pending;
        let outcome = overlays::press(picker, &self.table, pending, key, &mut ctx);
        // Which-key lists what follows a `g` or a `z` from when it was typed.
        match (was_waiting, self.grammar.pending.is_empty()) {
            (false, false) => self.grammar.pending_since = Some(now),
            (_, true) => self.grammar.pending_since = None,
            (true, false) => {}
        }
        match outcome {
            Outcome::Done | Outcome::Passed => {
                self.overlays.refresh_preview(self.services.rate);
                self.follow_file();
            }
            outcome => {
                self.settled(outcome, false);
            }
        }
    }

    /// Do what a picker's choice says.
    pub(crate) fn choose(&mut self, pick: Pick) {
        match pick {
            Pick::View(view) => self.jump_to_view(view),
            Pick::TrackPage(page) => self.show_track_page(page),
            Pick::Macro(at) => self.map_to_macro(at),
            Pick::MacroHolding(at) => self.go_to_macro(at),
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
            Pick::Preset { kind, path } => {
                self.note_place_of(Purpose::Preset(kind), &path);
                self.load_preset(kind, &path);
            }
            Pick::FactoryPatch(name) => {
                let over = std::mem::take(&mut self.overlays.explorer.patch_over);
                self.add_factory_patch(&name, over);
            }
            Pick::Prompt(text) => {
                self.overlays.command.line = text;
                self.overlays.command.menu = None;
                self.modes.mode = Mode::Command;
                self.services.status =
                    "Type a name, with folders before it if you like, then Enter".to_owned();
            }
            Pick::Project(folder) => {
                self.note_place_of(Purpose::Project, &folder);
                let force = std::mem::take(&mut self.overlays.explorer.opening_discards);
                self.line(crate::overlays::command::parse::Line::Edit {
                    path: Some(folder),
                    force,
                });
            }
            Pick::KitSound(KitChoice {
                path,
                label,
                name,
                play,
                choke,
            }) => {
                let track = self.services.cursor.track;
                let mut more = vec![Action::SetTrackChoke { track, choke }];
                // A kit's sound is a drum, unless the track's been told it's for something else.
                if self
                    .song()
                    .tracks
                    .get(track)
                    .is_some_and(|t| t.role.is_auto())
                {
                    more.push(Action::SetTrackRole {
                        track,
                        role: Role::Drums,
                    });
                }
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
                self.services.apply(&Action::SetTrackSample {
                    track,
                    sample: number,
                });
            }
            Pick::SampleLock { slot, number } => self.pick_sample_lock(slot, number),
            Pick::SliceLock { slot, number } => self.pick_slice_lock(slot, number),
            Pick::SliceMarkLock { slot, letter } => self.pick_slice_mark_lock(slot, letter),
            Pick::Mark(letter) => self.jump_to_mark(letter),
            Pick::SampleSetting(setting) => {
                self.sample_page.pick_setting(setting, &mut self.services);
            }
            Pick::SampleValue { setting, value } => self.pick_sample_value(setting, value),
            Pick::Tuning { control, value } => set_tuning(&mut self.services, control, value),
            Pick::Track(track) => self.go_to_track(track),
            Pick::Audio(choice) => self.pick_audio(choice),
            // The list above the command line takes its own choices, so none comes here.
            Pick::Completion(_) => {}
            Pick::FxCommand { slot, command } => self.pick_fx_command(slot, command),
            Pick::Gate(kind) => self.pick_gate(kind),
            Pick::Length(length) => self.set_length(length),
            Pick::Condition { slot, value } => self.pick_condition(slot, value),
            Pick::Amount { slot, value } => self.pick_amount(slot, value),
            Pick::Mod {
                track,
                slot,
                modulator,
            } => self.pick_mod(track, slot, modulator),
            Pick::Setting {
                track,
                setting,
                value,
            } => {
                self.services.apply(&Action::SetTrackSetting {
                    track,
                    setting,
                    value,
                });
            }
            Pick::Param {
                track,
                control,
                value,
            } => self.pick_param(track, control, value),
            Pick::RowPhrase(phrase) => ChainView::pick_phrase(phrase, &mut self.services),
            Pick::SectionChain(chain) => self.song_view.pick_chain(chain, &mut self.services),
            Pick::Line(line) => self.line(line),
            Pick::ScaleTo(scale) => self.services.set_scale(scale),
            Pick::ShowGroove(groove) => self.project_page.groove_shown = Some(groove),
            Pick::Scale(kind) => self.services.set_scale(Scale {
                on: true,
                kind,
                ..self.services.live.scale
            }),
            Pick::ShapeChoice(choice) => {
                if let Some(panel) = self.overlays.shape.as_mut() {
                    panel.change(|panel| choice.apply(&mut panel.shape), &mut self.services);
                }
            }
            Pick::GenChoice {
                phrase,
                generator,
                choice,
            } => self.pick_gen_choice(phrase, generator, choice),
            Pick::GenColumn(field) => self.pick_gen_column(field, None),
            Pick::GenCommand { field, command } => self.pick_gen_column(field, Some(command)),
            Pick::ShapeCommand(command) => self.pick_shape_command(command),
            Pick::Guide(guide) => {
                self.overlays.guide.show(guide);
                self.modes.mode = Mode::Guide;
            }
            Pick::KitTracks(path) => {
                self.note_place_of(Purpose::Kit, &path);
                self.load_kit(&path);
            }
            Pick::KitLoad { kit, sounds } => {
                self.note_place_of(Purpose::Kit, &kit);
                self.load_kit_tracks(&kit, &sounds);
            }
            Pick::KitBeat(tracks) => self.write_kit_beat(&tracks),
        }
    }

    /// Remember the folder a file chosen in the explorer is in.
    fn note_place_of(&mut self, purpose: Purpose, path: &Path) {
        if let Some(folder) = path.parent() {
            self.note_place(purpose, folder);
        }
    }
}
