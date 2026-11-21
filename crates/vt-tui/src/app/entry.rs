//! Typing into the grid: notes, values, FX commands, gates, lengths and nudges, in Insert and
//! Replace, and the scale and grid they go by.

use crate::app::accent_slide::Written;
use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Command;
use vt_model::accumulator::{MOST_DEGREES, amount};
use vt_model::action::{Action, FieldValue};
use vt_model::condition::Condition;
use vt_model::scale::Scale;
use vt_model::selection::Field;
use vt_model::speed::Speed;
use vt_model::step::slid;
use vt_model::step::{
    FxCommand, FxSlot, Gate, GateKind, HIGHEST_OCTAVE, LOWEST_OCTAVE, MAX_PULSES, MAX_VELOCITY,
    MOST_NUDGE, Mark, Note, Step, nudge_digits,
};

use crate::input::bindings::{Cmd, Resolved};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::typed::{HEX, Typed};
use crate::pages::phrase::PhraseView;
use crate::services::Services;

use super::{App, Entry, GRID_SIZES, LONGEST_LABEL, next_note_velocity, snap_by};

impl App {
    /// Typing here means what it does in Insert, for this one field, and `advance` ends it.
    pub(super) fn replace_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => {
                self.cancel_entry();
                if let Some(field) = self.grammar.replacing.take() {
                    self.services.cursor.field = field;
                }
                if let Some((selection, _)) = self.grammar.replacing_selection.take() {
                    self.services.cursor.step = selection.range.first;
                    self.cancel_selection_replace();
                }
            }
            KeyCode::Backspace => {
                self.modes.entry.text.pop();
            }
            KeyCode::Enter if self.modes.entry.text.is_empty() => {}
            KeyCode::Char('c' | 'v') if self.services.cursor.field == Field::Note => {}
            _ => match self.services.cursor.field {
                Field::Note => self.insert_note(key),
                Field::Velocity | Field::FxValue(_) => self.insert_value(key),
                Field::Nudge => self.insert_nudge(key),
                Field::Gate => self.insert_gate(key),
                Field::GateLength => self.insert_length(key),
                Field::FxCommand(slot) => self.insert_command(key, slot),
                Field::FxFine(slot) => self.insert_fine(key, slot),
            },
        }
    }

    pub(super) fn insert_key(&mut self, key: Key) {
        // You can start and pause while writing, as `Tab` and `Shift-Tab` are never typed.
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart),
            _ => {}
        }
        if self.modes.view == View::Phrase && key.alt {
            match key.code {
                KeyCode::Char('1') => self.change_grid(false),
                KeyCode::Char('2') => self.change_grid(true),
                KeyCode::Char('s') => self.toggle_scale(),
                _ => {}
            }
            return;
        }
        let skip = self.song().skip as usize;
        let arrow = match (key.code, key.shift) {
            (KeyCode::Up, _) => Some(-1),
            (KeyCode::Down, _) => Some(1),
            (KeyCode::Left, true) => Some(snap_by(self.current_row(), skip, false)),
            (KeyCode::Right, true) => Some(snap_by(self.current_row(), skip, true)),
            (KeyCode::Left, false) => Some(-(skip as isize)),
            (KeyCode::Right, false) => Some(skip as isize),
            _ => None,
        };
        if let Some(by) = arrow {
            if self.modes.view == View::Phrase {
                self.insert_move(by);
            }
            return;
        }
        if self.modes.view == View::Project {
            let entry = &mut self.modes.entry;
            let page = &mut self.project_page;
            if let Some(outcome) = page.insert(key, entry, &mut self.services) {
                self.modes.mode = Mode::Normal;
                self.settled(outcome, false);
            }
            return;
        }
        if self.modes.view != View::Phrase {
            if !key.ctrl {
                match self.modes.view {
                    View::Chain => {
                        let entry = &mut self.modes.entry;
                        if !self.chain_view.insert(key.code, entry, &mut self.services) {
                            self.modes.mode = Mode::Normal;
                        }
                    }
                    View::Song => {
                        let entry = &mut self.modes.entry;
                        if !self.song_view.insert(key.code, entry, &mut self.services) {
                            self.modes.mode = Mode::Normal;
                        }
                    }
                    _ => {
                        let control = self.track_control();
                        let view = self.modes.view;
                        let entry = &mut self.modes.entry;
                        let services = &mut self.services;
                        let rack = if view == View::Gen {
                            &mut self.gen_page
                        } else {
                            &mut self.track_page
                        };
                        if !rack.insert(key.code, control, view, entry, services) {
                            self.modes.mode = Mode::Normal;
                        }
                    }
                }
            }
            return;
        }
        if key.code == KeyCode::Char(';') && !key.ctrl && !key.alt {
            self.services.toggle_auto_jump();
            return;
        }
        match self.services.cursor.field {
            Field::Note => self.insert_note(key),
            Field::Velocity | Field::FxValue(_) => self.insert_value(key),
            Field::Nudge => self.insert_nudge(key),
            Field::Gate => self.insert_gate(key),
            Field::GateLength => self.insert_length(key),
            Field::FxCommand(slot) => self.insert_command(key, slot),
            Field::FxFine(slot) => self.insert_fine(key, slot),
        }
    }

    pub(super) fn toggle_scale(&mut self) {
        self.modes.count = None;
        self.services.set_scale(Scale {
            on: !self.services.live.scale.on,
            ..self.services.live.scale
        });
    }

    /// Move this phrase's notes to the nearest in the scale.
    pub(super) fn fit_to_scale(&mut self) {
        let scale = self.services.scale_here();
        if !scale.on {
            self.services.status = "The scale is off".to_owned();
            return;
        }
        if let Some(phrase) = self.services.phrase_id() {
            self.services.apply(&Action::FitToScale {
                track: self.services.cursor.track,
                phrase,
                scale,
            });
        }
    }

    /// Halve or double the skip setting, as a grid narrows or widens in Ableton. A size between
    /// these goes to the next one along, and one past them only comes back.
    pub(super) fn change_grid(&mut self, wider: bool) {
        self.modes.count = None;
        let skip = self.song().skip;
        let next = if wider {
            GRID_SIZES.iter().find(|size| **size > skip)
        } else {
            GRID_SIZES.iter().rfind(|size| **size < skip)
        };
        if let Some(next) = next {
            self.services.apply(&Action::SetSkipSize(*next));
        }
    }

    fn insert_note(&mut self, key: Key) {
        if self.modes.mode == Mode::Insert
            && let Some(flag) = self.flag_key(key)
        {
            self.hold_flag(key, flag);
            return;
        }
        if self.modes.mode == Mode::Insert {
            match self.table.resolve(Mode::Insert, &[key]) {
                Resolved::Command(Cmd::WidenGrid) => return self.change_grid(true),
                Resolved::Command(Cmd::NarrowGrid) => return self.change_grid(false),
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => self.modes.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.back_for_clearing();
                self.clear_note();
            }
            KeyCode::Char('z') => {
                self.services.live.octave = (self.services.live.octave - 1).max(LOWEST_OCTAVE)
            }
            KeyCode::Char('x') => {
                self.services.live.octave = (self.services.live.octave + 1).min(HIGHEST_OCTAVE)
            }
            KeyCode::Char('c') => {
                self.services.live.note_velocity =
                    next_note_velocity(self.services.live.note_velocity, false)
            }
            KeyCode::Char('v') => {
                self.services.live.note_velocity =
                    next_note_velocity(self.services.live.note_velocity, true)
            }
            // A note, and an OFF, is written once however long its key is held.
            KeyCode::Char('1') => {
                self.held.repeats_ignored = true;
                self.write_note(Note::Off);
                self.advance();
            }
            KeyCode::Char(c) if !key.ctrl => {
                // While recording, a note goes in where the track is playing, and the cursor
                // stays.
                let played = self.played_key(c).map(|midi| self.flagged_note(midi));
                if let Some(midi) = played
                    && self.recording.is_some()
                    && self.modes.mode == Mode::Insert
                {
                    self.record_note(key, midi);
                    let velocity = self.flagged_velocity(self.services.live.note_velocity);
                    self.hold_note(key, midi, velocity);
                } else if let Some(midi) = played {
                    let on_step =
                        self.services.phrase_here().cloned().and_then(|phrase| {
                            phrase.steps().get(self.services.cursor.step).cloned()
                        });
                    let velocity = on_step
                        .as_ref()
                        .map_or(self.services.live.note_velocity, |step| {
                            self.played_with(step)
                        });
                    let velocity = self.flagged_velocity(velocity);
                    let (track, step) = (self.services.cursor.track, self.services.cursor.step);
                    let phrase = self.write_played_note(midi);
                    self.hold_note(key, midi, velocity);
                    self.advance();
                    self.held.octave_moved = None;
                    self.held.last_written = phrase.map(|phrase| Written {
                        track,
                        phrase,
                        step,
                        cursor: self.services.cursor,
                    });
                }
            }
            _ => {}
        }
    }

    fn current_row(&self) -> isize {
        self.services.extent().row(self.services.cursor.step)
    }

    /// Move without writing, staying in Insert. Anything half typed is dropped.
    fn insert_move(&mut self, by: isize) {
        if !self.modes.entry.text.is_empty() || self.modes.entry.command.is_some() {
            self.cancel_entry();
            self.modes.mode = Mode::Insert;
        }
        self.services.move_step(by);
    }

    /// Write a played note with its velocity, and an accent or slide whose key is down, as one
    /// change, and say which phrase it went in.
    fn write_played_note(&mut self, midi: u8) -> Option<u32> {
        let Some(id) = self.services.phrase_id() else {
            self.services.status = "This track has no phrase here".to_owned();
            return None;
        };
        let current = self.services.phrase_here();
        let step = current.and_then(|p| p.steps().get(self.services.cursor.step).cloned())?;
        let velocity = self.flagged_velocity(self.played_with(&step));
        let gate = if self.flagged_slide() {
            slid(step.gate, true)
        } else {
            step.gate
        };
        self.services.apply(&Action::SetStep {
            track: self.services.cursor.track,
            phrase: id,
            step: self.services.cursor.step,
            value: Step {
                note: Note::On(midi),
                velocity: Some(velocity),
                gate,
                ..step
            },
        });
        self.use_flags();
        Some(id)
    }

    /// The velocity a played note takes. Replacing keeps what the step has.
    fn played_with(&self, step: &Step) -> u8 {
        if self.modes.mode == Mode::Replace {
            step.velocity.unwrap_or(self.services.live.note_velocity)
        } else {
            self.services.live.note_velocity
        }
    }

    /// Clear a note along with the velocity it was written with.
    fn clear_note(&mut self) {
        let Some(id) = self.services.phrase_id() else {
            return;
        };
        let current = self.services.phrase_here();
        let Some(step) = current.and_then(|p| p.steps().get(self.services.cursor.step).cloned())
        else {
            return;
        };
        self.services.apply(&Action::SetStep {
            track: self.services.cursor.track,
            phrase: id,
            step: self.services.cursor.step,
            value: Step {
                note: Note::Empty,
                velocity: None,
                ..step
            },
        });
    }

    /// Leave Insert, dropping anything typed but not written.
    fn cancel_entry(&mut self) {
        if let Some(command) = self.modes.entry.command
            && let Field::FxValue(slot) = self.services.cursor.field
        {
            self.services.status =
                format!("{} needs a value, so nothing was written", command.name());
            self.services.cursor.field = Field::FxCommand(slot);
        }
        self.modes.entry = Entry::default();
        self.modes.mode = Mode::Normal;
    }

    pub(crate) fn set_field(&mut self, value: FieldValue) {
        let Some(phrase) = self.services.phrase_id() else {
            return;
        };
        self.services.apply(&Action::SetField {
            track: self.services.cursor.track,
            phrase,
            step: self.services.cursor.step,
            value,
        });
    }

    /// Two hex digits commit, and auto-jump moves on by the grid. A command typed just before
    /// takes effect here, so typing `CUT` during playback doesn't close the filter.
    fn insert_value(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    match self.services.cursor.field {
                        Field::Velocity => self.set_field(FieldValue::Velocity(None)),
                        Field::FxValue(slot) if self.modes.entry.command.is_none() => {
                            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                if let Typed::Commit(text) = self.modes.entry.text.key(key.code, HEX) {
                    self.commit_value(&text);
                }
            }
            _ => {}
        }
    }

    fn commit_value(&mut self, text: &str) {
        let Ok(value) = u8::from_str_radix(text, 16) else {
            return;
        };
        match self.services.cursor.field {
            Field::Velocity => {
                self.set_field(FieldValue::Velocity(Some(value.min(MAX_VELOCITY))));
            }
            Field::FxValue(slot) => {
                let current = self.services.current_slot(slot).unwrap_or_default();
                // A condition's value is one of a list, and anything else is refused.
                let command = self.modes.entry.command.or(current.command);
                if command == Some(FxCommand::Cnd) && Condition::from_value(value).is_none() {
                    self.services.status =
                        format!("CND {value:02X} isn't a condition: K lists them");
                    return;
                }
                if command == Some(FxCommand::Acu) && amount(value).is_none() {
                    self.services.status = format!(
                        "ACU {value:02X} is past {MOST_DEGREES}: 01 to 1C go up, FF to E4 down, and K lists them"
                    );
                    return;
                }
                let Some(command) = self.modes.entry.command.take().or(current.command) else {
                    self.services.status = "Type an FX command first".to_owned();
                    return;
                };
                // Typed over the same setting, the coarse digits change and the fine part stays.
                // Typed over an `SLI` by a letter, it's by the number typed.
                let fine = if current.command == Some(command) && command.has_fine() {
                    current.fine()
                } else {
                    0
                };
                self.set_field(FieldValue::Slot(
                    slot,
                    FxSlot {
                        command: Some(command),
                        value: u16::from(value) << 8 | u16::from(fine),
                        mark: self.mark_for_typed_value(slot),
                    },
                ));
            }
            _ => {}
        }
        self.advance();
    }

    /// Two hex digits set a value's fine part, keeping its coarse digits, and auto-jump moves on
    /// by the grid.
    fn insert_fine(&mut self, key: Key, slot: usize) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_fine(slot, 0);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                let current = self.services.current_slot(slot).unwrap_or_default();
                if !current.command.is_some_and(FxCommand::has_fine) {
                    self.services.status = "Only a setting that sweeps has a fine part".to_owned();
                    return;
                }
                if let Typed::Commit(text) = self.modes.entry.text.key(key.code, HEX) {
                    if let Ok(fine) = u8::from_str_radix(&text, 16) {
                        self.set_fine(slot, fine);
                    }
                    self.advance();
                }
            }
            _ => {}
        }
    }

    /// Set a value's fine part, where it has one.
    pub(crate) fn set_fine(&mut self, slot: usize, fine: u8) {
        let current = self.services.current_slot(slot).unwrap_or_default();
        if current.command.is_some_and(FxCommand::has_fine) {
            self.set_field(FieldValue::Slot(
                slot,
                FxSlot {
                    value: current.value & 0xFF00 | u16::from(fine),
                    ..current
                },
            ));
        }
    }

    /// The mark a value typed here gets: the one it's typed over, or else the nearest value's
    /// above it in this column.
    pub(crate) fn mark_for_typed_value(&self, slot: usize) -> Mark {
        let phrase = self.services.phrase_here();
        let Some(steps) = phrase.map(|phrase| phrase.steps()) else {
            return Mark::default();
        };
        let upward = steps[..=self.services.cursor.step.min(steps.len() - 1)]
            .iter()
            .rev();
        upward
            .filter_map(|step| step.fx.get(slot))
            .find(|fx| fx.is_value())
            .map_or(Mark::default(), |fx| fx.mark)
    }

    /// A label, then `Enter`, moves to the value. One that isn't on the track opens the list.
    fn insert_command(&mut self, key: Key, slot: usize) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                }
            }
            // A label is 3 or 4 letters, after a slot's digit for an effect's setting.
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_alphanumeric() => {
                if self.modes.entry.text.len() < LONGEST_LABEL {
                    self.modes.entry.text.push(c.to_ascii_uppercase());
                }
            }
            // A label on this track goes to its value. Anything else opens the list, searching
            // for what was typed, so a name half remembered still gets there.
            KeyCode::Enter if !self.modes.entry.text.is_empty() => {
                let name = self.modes.entry.text.take();
                let track = self.song().tracks.get(self.services.cursor.track);
                match track.and_then(|track| FxCommand::typed(&name, track)) {
                    Some(command) => {
                        self.modes.entry.command = Some(command);
                        self.services.cursor.field = Field::FxValue(slot);
                    }
                    None => {
                        self.services.status = format!("{name} isn't a label on this track");
                        let query = name.to_lowercase();
                        self.show_picker(PhraseView::fx_picker(&self.services, slot, &query));
                    }
                }
            }
            _ => {}
        }
    }

    /// A gate's three letters then its pulses, like `HLD3`. A digit on its own keeps the gate type
    /// that's there, or `SGL` in an empty cell, and changes only the pulses. The note's length
    /// stays.
    fn insert_gate(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_field(FieldValue::Gate(None));
                }
            }
            KeyCode::Char(c @ '0'..='9') if !key.ctrl && self.modes.entry.text.len() != 1 => {
                let pulses = c as u8 - b'0';
                if pulses > MAX_PULSES {
                    self.services.status = format!("A row lasts 0 to {MAX_PULSES} pulses");
                    self.modes.entry.text.clear();
                    return;
                }
                let name = self.modes.entry.text.take();
                let kind = match name.as_str() {
                    "" => self
                        .services
                        .current_gate()
                        .map_or(GateKind::Single, |gate| gate.kind),
                    name => match GateKind::from_name(name) {
                        Some(kind) => kind,
                        None => {
                            self.services.status =
                                format!("{name} isn't a gate: SGL, MLT, HLD or RST");
                            return;
                        }
                    },
                };
                let length = self.services.current_gate().and_then(|gate| gate.length);
                self.set_field(FieldValue::Gate(Some(Gate {
                    kind,
                    pulses,
                    length,
                })));
                self.advance();
            }
            KeyCode::Char(c)
                if !key.ctrl && c.is_ascii_alphabetic() && self.modes.entry.text.len() < 3 =>
            {
                self.modes.entry.text.push(c.to_ascii_uppercase());
                if self.modes.entry.text.len() == 3
                    && GateKind::from_name(&self.modes.entry.text).is_none()
                {
                    self.services.status = format!(
                        "{} isn't a gate: SGL, MLT, HLD or RST",
                        self.modes.entry.text
                    );
                    self.modes.entry.text.clear();
                }
            }
            _ => {}
        }
    }

    /// Two hex digits set how much of its last pulse the row's note lasts, and auto-jump moves on
    /// by the grid.
    fn insert_length(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_length(None);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                if let Typed::Commit(text) = self.modes.entry.text.key(key.code, HEX) {
                    if let Ok(length) = u8::from_str_radix(&text, 16) {
                        self.set_length(Some(length));
                    }
                    self.advance();
                }
            }
            _ => {}
        }
    }

    /// Give the gate cell under the cursor a length of its own, keeping its gate and pulses, or
    /// take it away so the note plays at the track's `Gate`. An empty cell gets one pulse of `SGL`
    /// to hold a length, which is how it played.
    pub(crate) fn set_length(&mut self, length: Option<u8>) {
        let gate = match self.services.current_gate() {
            Some(gate) => Some(Gate { length, ..gate }),
            None if length.is_some() => Some(Gate {
                length,
                ..Gate::default()
            }),
            None => None,
        };
        self.set_field(FieldValue::Gate(gate));
    }

    /// A nudge's 24ths of a sixteenth in hex, late unless `-` comes first, and `+` or `=` first
    /// says late again: one digit, `0` to `B` at a sixteenth, on a track whose nudges go no
    /// further than `F`, and two, `00` to `2F` at a quarter and `7F` at a whole note, on one whose
    /// go further. All `0`s put the row back on the grid, and auto-jump moves on by the grid once
    /// the digits are in.
    fn insert_nudge(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.modes.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_nudge(0);
                }
            }
            KeyCode::Char(sign @ ('-' | '+' | '='))
                if !key.ctrl && self.modes.entry.text.len() <= 1 =>
            {
                self.modes.entry.text = if sign == '-' { "-" } else { "+" }.into();
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                let most = self.services.most_nudge_here();
                let early = self.modes.entry.text.starts_with('-');
                let mut digits: String = self
                    .modes
                    .entry
                    .text
                    .trim_start_matches(['-', '+'])
                    .to_owned();
                digits.push(c.to_ascii_uppercase());
                let amount = u8::from_str_radix(&digits, 16).unwrap_or(0);
                if amount > most as u8 {
                    let how = if most == MOST_NUDGE {
                        "as far as a nudge goes"
                    } else {
                        "just under half its step"
                    };
                    self.services.status =
                        format!("A nudge on this track goes up to {most:X}, {how}");
                    return;
                }
                let amount = amount as i8;
                if digits.len() < nudge_digits(most) {
                    self.modes.entry.text.push(c.to_ascii_uppercase());
                    return;
                }
                self.modes.entry.text.clear();
                self.set_nudge(if early { -amount } else { amount });
                self.advance();
            }
            _ => {}
        }
    }

    /// Nudge the row under the cursor, or put it back on the grid with 0.
    pub(crate) fn set_nudge(&mut self, nudge: i8) {
        self.set_field(FieldValue::Nudge(nudge));
    }

    fn advance(&mut self) {
        if self.modes.mode == Mode::Replace {
            self.finish_replace();
            return;
        }
        if self.services.live.auto_jump {
            self.services.move_step(self.song().skip as isize);
        }
    }

    /// Before `Backspace` clears: with auto-jump, back one grid cell to the last entry, and
    /// without, it clears where it is.
    fn back_for_clearing(&mut self) {
        if self.services.live.auto_jump {
            self.services.move_step(-(self.song().skip as isize));
        }
    }

    fn write_note(&mut self, note: Note) {
        let Some(phrase) = self.services.phrase_id() else {
            self.services.status = "This track has no phrase here".to_owned();
            return;
        };
        self.services.apply(&Action::SetNote {
            track: self.services.cursor.track,
            phrase,
            step: self.services.cursor.step,
            note,
        });
    }
}

impl Services {
    /// Set the scale, which isn't an edit to undo, and say what it now does.
    pub(crate) fn set_scale(&mut self, scale: Scale) {
        self.live.scale = scale;
        self.handle.send(Command::SetScale { scale });
        self.dirty = true;
        // The live generators' notes go through it too, written again and sent as a version.
        if self.follow_scale() {
            self.publish();
        }
        self.status = if scale.on {
            format!("Scale {} {}", scale.root_name(), scale.kind.name())
        } else {
            "Scale off".to_owned()
        };
    }

    /// `;` in Insert: whether entering moves on by the grid. Like the scale, it isn't an edit to
    /// undo.
    pub(crate) fn toggle_auto_jump(&mut self) {
        self.live.auto_jump = !self.live.auto_jump;
        self.dirty = true;
        self.status = if self.live.auto_jump {
            "Auto-jump on: each entry moves on by the grid".to_owned()
        } else {
            "Auto-jump off: entering stays put".to_owned()
        };
    }

    /// An FX slot of the row under the cursor.
    pub(crate) fn current_slot(&self, slot: usize) -> Option<FxSlot> {
        let phrase = self.phrase_here()?;
        phrase.steps().get(self.cursor.step)?.fx.get(slot).copied()
    }

    /// The furthest the cursor's track's nudges go at its speed.
    pub(crate) fn most_nudge_here(&self) -> i8 {
        self.track_here()
            .map_or(Speed::Sixteenth, |track| track.speed)
            .most_nudge()
    }

    /// How far the row under the cursor is nudged, in 24ths of a sixteenth.
    pub(crate) fn current_nudge(&self) -> i8 {
        self.phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step))
            .map_or(0, |step| step.nudge)
    }

    /// The gate cell under the cursor.
    pub(crate) fn current_gate(&self) -> Option<Gate> {
        let phrase = self.phrase_here()?;
        phrase.steps().get(self.cursor.step)?.gate
    }
}
