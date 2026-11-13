//! Keys as they arrive: pressed, repeated and let go, the holds that last while a key is down, the
//! scope a key goes to, the views and popups keys open, and auditions.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEventKind};
use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::constants::MAX_PHRASE_STEPS;
use vt_model::step::{HIGHEST_OCTAVE, LOWEST_OCTAVE, MAX_NOTE, MIN_NOTE, octave_start};

use crate::input::bindings::{Binding, Cmd, Resolved, adjusting_key, degree_key, note_key};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};

use super::{App, Entry, HOLD_TIME, Held, HeldNote, KEYS_NOTES, Momentary, which_key_due};

impl App {
    /// A key from the terminal, which the kitty keyboard protocol says is a press, a repeat or a
    /// release. What a held key does, in every mode, is decided here and in the three below.
    pub fn key_event(&mut self, key: Key, kind: KeyEventKind, now: Instant) {
        if key.ignored() {
            return;
        }
        // Only a key bound with Cmd, such as `Cmd-l`, goes through, so `Cmd-a` isn't read as `a`.
        // Its release still counts, as `a` can be held before Cmd goes down.
        if key.command && kind != KeyEventKind::Release && !self.binds(key) {
            return;
        }
        match kind {
            KeyEventKind::Press => self.key(key, now),
            KeyEventKind::Repeat => self.key_repeated(key, now),
            KeyEventKind::Release => self.key_released(key, now),
        }
    }

    /// Whether a key, after any typed before it, means something here. The shape panel reads its
    /// own keys, `=` `-` `+` `_` among them, so their Cmd keys mean something there too.
    fn binds(&self, key: Key) -> bool {
        if self.mode == Mode::Shape {
            return adjusting_key(key).is_some();
        }
        let mut keys = self.pending.clone();
        keys.push(key);
        self.table.resolve(self.key_scope(), &keys) != Resolved::Unbound
    }

    /// A key pressed.
    pub fn key(&mut self, key: Key, now: Instant) {
        self.now = now;
        // Any key ends a turn of the wheel, as its own undo step.
        self.end_wheel_gesture();
        // A real second press comes after a release, so a press of the key that's down is the
        // terminal repeating it. kitty on macOS has been seen sending a held `a`'s repeats as
        // presses. A key holding a note is down too, even after another key has gone down.
        if self.key_down == Some(key.physical()) || self.holds_note(key) || self.holds_flag(key) {
            self.key_repeated(key, now);
            return;
        }
        self.key_down = Some(key.physical());
        self.repeats_ignored = false;
        self.press(key, now);
    }

    /// A key the terminal repeats while it's held. A transport key acts once however long it's
    /// held, and a note on the keyboard is already held; anything else repeats as a press.
    pub fn key_repeated(&mut self, key: Key, now: Instant) {
        self.now = now;
        self.end_wheel_gesture();
        if (self.repeats_ignored && self.key_down == Some(key.physical()))
            || self.holds_note(key)
            || self.holds_flag(key)
        {
            return;
        }
        self.repeating = true;
        self.press(key, now);
        self.repeating = false;
    }

    /// A key let go: its held note ends, leaving any others held, and so does what it started
    /// that lasts while it's held.
    pub fn key_released(&mut self, key: Key, now: Instant) {
        self.now = now;
        if self.key_down == Some(key.physical()) {
            self.key_down = None;
            self.repeats_ignored = false;
        }
        if let Some(at) = self
            .held_notes
            .iter()
            .position(|held| held.key == key.physical())
        {
            let held = self.held_notes.remove(at);
            self.let_go_of(held);
        }
        self.let_go_of_flag(key);
        if self
            .momentary
            .is_some_and(|held| held.key == key.physical())
        {
            self.end_momentary(now);
        }
    }

    /// End what a key started that lasts while it's held. Down for less than [`HOLD_TIME`], it was
    /// a tap, and stays as it is: a mute or solo stays switched, and an audition plays out. Held
    /// longer, a mute or solo goes back to how it was, and an audition stops.
    fn end_momentary(&mut self, now: Instant) {
        let Some(held) = self.momentary.take() else {
            return;
        };
        let long = now.saturating_duration_since(held.since) >= HOLD_TIME;
        match held.what {
            Momentary::Mute { track, was } | Momentary::Solo { track, was } => {
                let mute = matches!(held.what, Momentary::Mute { .. });
                match self.levels.get_mut(track) {
                    Some(live) if long => {
                        if mute {
                            live.mute = was;
                            self.handle.send(Command::SetMute { track, mute: was });
                        } else {
                            live.solo = was;
                            self.handle.send(Command::SetSolo { track, solo: was });
                        }
                    }
                    // A save while it was down kept how it was before.
                    Some(_) => self.dirty = true,
                    None => {}
                }
            }
            Momentary::Fill { was } if long => self.set_fill(was),
            Momentary::Fill { .. } => {}
            Momentary::Audition => {
                self.handle.send(Command::EndAudition { held: long });
            }
            Momentary::AuditionNote { track, note } if long => {
                self.handle.send(Command::ReleasePreview { track, note });
            }
            Momentary::AuditionNote { track, note } => {
                self.handle.send(Command::FinishPreview { track, note });
            }
            // Only what it played itself, so a later one carries on.
            Momentary::SampleAudition { hearing } => {
                if long && hearing == self.sample_page.hearings {
                    self.handle.send(Command::StopFile);
                }
            }
        }
    }

    /// Whether this key is holding a note.
    fn holds_note(&self, key: Key) -> bool {
        self.held_notes
            .iter()
            .any(|held| held.key == key.physical())
    }

    /// End a held key's note, unless another key still down is holding the same one, as a key an
    /// octave up and one an octave down can.
    fn let_go_of(&mut self, held: HeldNote) {
        self.end_take(held.key);
        let shared = self
            .held_notes
            .iter()
            .any(|other| other.track == held.track && other.note == held.note);
        if !shared {
            self.handle.send(Command::ReleasePreview {
                track: held.track,
                note: held.note,
            });
        }
    }

    /// End every held note.
    fn let_go_of_held_notes(&mut self) {
        for held in std::mem::take(&mut self.held_notes) {
            self.let_go_of(held);
        }
    }

    /// The terminal window lost focus, so a key that's down may never be reported let go. Its
    /// note ends now, what it started that lasts while it's held ends as if it was let go now,
    /// and the next press of it is a press.
    pub fn focus_lost(&mut self, now: Instant) {
        self.now = now;
        self.key_down = None;
        self.repeats_ignored = false;
        self.let_go_of_held_notes();
        self.drop_flags();
        self.end_momentary(now);
    }

    /// Sound a note for as long as this key is down, ignoring the key's repeats. Each key held
    /// sounds its own note, so several play a chord.
    pub(super) fn hold_note(&mut self, key: Key, note: u8, velocity: u8) {
        let track = self.cursor.track;
        self.held_notes.push(HeldNote {
            key: key.physical(),
            track,
            note,
        });
        self.repeats_ignored = true;
        self.handle.send(Command::HoldPreview {
            track,
            note,
            velocity,
        });
    }

    /// Act on a press, or a repeat that acts as one.
    pub(crate) fn press(&mut self, key: Key, now: Instant) {
        let was = self.mode;
        self.started = None;
        // The Gen page shows the phrase under the cursor, wherever a key or the time between
        // keys has moved it.
        self.track_page.phrase = self.phrase_id();
        self.handle_key(key, now);
        self.keep_cursor_shown();
        self.track_page.phrase = self.phrase_id();
        // Something that lasts while its key is down is tied to that key, and doesn't repeat.
        // One already held ends as if let go now, as only one is kept.
        if let Some(what) = self.started.take() {
            self.end_momentary(now);
            self.momentary = Some(Held {
                key: key.physical(),
                since: now,
                what,
            });
            self.repeats_ignored = true;
        }
        // Leaving the keyboard, or Insert, lets go of what it's holding.
        if matches!(was, Mode::Keys | Mode::Insert) && self.mode != was {
            self.let_go_of_held_notes();
            self.drop_flags();
            self.last_written = None;
        }
    }

    fn handle_key(&mut self, key: Key, now: Instant) {
        match self.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::VisualField
            | Mode::VisualStep
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song
            | Mode::Audio
            | Mode::Project
            | Mode::Sample
            | Mode::SampleVisual => {
                self.command_key_normal(key, now);
            }
            Mode::Insert => self.insert_key(key),
            Mode::Replace => self.replace_key(key),
            Mode::Command => self.command_key(key),
            Mode::Help => self.help_key(key),
            Mode::Guide => self.guide_key(key),
            Mode::Picker => self.picker_key(key),
            Mode::Shape => self.shape_key(key),
            Mode::Jump => self.jump_key(key),
            Mode::Restore => self.restore_key(key),
            Mode::Keys => self.keys_key(key, now),
        }
    }

    /// Play the track's sound from a few keys, `a` `w` `s` `e` `d` `f`, without writing anything,
    /// while every other key works on the page as it does, so settings change as it plays. A note
    /// lasts as long as its key is down.
    fn keys_key(&mut self, key: Key, now: Instant) {
        if self.view == View::Sample {
            self.sample_keys_key(key, now);
            return;
        }
        let plain = self.pending.is_empty() && !key.ctrl && !key.alt;
        match key.code {
            KeyCode::Esc | KeyCode::Char(';') if self.pending.is_empty() => {
                self.mode = Mode::Normal;
                self.status.clear();
            }
            KeyCode::Char('<') if plain => self.octave = (self.octave - 1).max(LOWEST_OCTAVE),
            KeyCode::Char('>') if plain => self.octave = (self.octave + 1).min(HIGHEST_OCTAVE),
            KeyCode::Char(c) if plain && KEYS_NOTES.contains(c) => {
                if let Some(note) = self.played_key(c) {
                    self.hold_note(key, note, self.note_velocity);
                }
            }
            _ => {
                self.command_key_normal(key, now);
                // The keys only play on the track page.
                if self.mode == Mode::Keys && self.view != View::Track {
                    self.mode = Mode::Normal;
                }
            }
        }
    }

    /// The note a key plays in the current octave. With a scale, the home row goes up through it
    /// from the root, and the row above is silent, so every key is in key.
    pub(super) fn played_key(&self, c: char) -> Option<u8> {
        if self.scale.on {
            degree_key(c).map(|degree| self.scale.degree(degree, self.octave))
        } else {
            note_key(c).map(|semitones| {
                let midi = octave_start(self.octave) + i16::from(semitones);
                midi.clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8
            })
        }
    }

    /// What the popup should list, once the delay has passed.
    pub fn popup_rows(&self, now: Instant, delay: Duration) -> Vec<(String, &'static str)> {
        if self.pending.is_empty() || !which_key_due(self.pending_since, now, delay) {
            return Vec::new();
        }
        self.table.continuations(self.key_scope(), &self.pending)
    }

    /// Which rows of the binding table apply now. Normal mode's keys depend on the view.
    pub fn key_scope(&self) -> Mode {
        match (self.mode, self.view) {
            (Mode::Keys, View::Track) => Mode::Track,
            (Mode::Keys, View::Sample) => Mode::Sample,
            (Mode::Normal, View::Chain) => Mode::Chain,
            (Mode::Normal, View::Track) => Mode::Track,
            (Mode::Normal, View::Mixer) => Mode::Mixer,
            (Mode::Normal, View::Audio) => Mode::Audio,
            (Mode::Normal, View::Project) => Mode::Project,
            (Mode::Normal, View::Sample) if self.sample_page.anchor.is_some() => Mode::SampleVisual,
            (Mode::Normal, View::Sample) => Mode::Sample,
            (Mode::Normal, View::Song) => Mode::Song,
            (mode, _) => mode,
        }
    }

    pub(crate) fn show_view(&mut self, view: View) {
        self.count = None;
        // The audio page lists the devices afresh each time it opens.
        self.audio.refresh |= view == View::Audio && self.view != View::Audio;
        // Sections selected in the song view are left there.
        if view != View::Song {
            self.section_anchor = None;
        }
        self.view = view;
        self.note_view(view);
        self.clamp_cursor();
        if view != View::Phrase {
            self.stop_recording();
        }
        if view == View::Sample
            && let Err(why) = self.sample_page_here()
        {
            self.status = why;
        }
    }

    /// Show a view, as a jump that `Ctrl-O` can undo.
    pub(crate) fn jump_to_view(&mut self, view: View) {
        if view != self.view {
            self.record_jump();
        }
        self.show_view(view);
    }

    /// Every key, for `Space k`.
    pub fn help_rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        let mut last = None;
        for binding in self.table.rows() {
            // The sample page's keys while it selects are its own, and only its text objects
            // differ.
            let as_sample =
                |other: &Binding| other.mode == Mode::Sample && other.keys == binding.keys;
            if binding.mode == Mode::SampleVisual && self.table.rows().any(as_sample) {
                continue;
            }
            if last != Some(binding.mode) {
                rows.push(String::new());
                rows.push(format!("{:?}", binding.mode));
                last = Some(binding.mode);
            }
            rows.push(format!("  {:<12} {}", binding.keys, binding.description));
        }
        rows
    }

    fn help_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            _ => {
                self.help_scroll = 0;
                self.mode = Mode::Normal;
            }
        }
    }

    /// Hear a row, unless playback is using the tracks, for as long as the key is down if it's
    /// held.
    pub(crate) fn audition(&mut self, section_step: usize) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
        } else {
            self.audition_track(self.cursor.section, section_step);
        }
    }

    /// Hear a row of a section, for as long as the key is down if it's held.
    pub(crate) fn audition_section(&mut self, section: usize, section_step: usize) {
        self.handle.send(Command::Audition {
            section,
            section_step,
            track: None,
        });
        self.started = Some(Momentary::Audition);
    }

    /// Hear the row the cursor's track is on at a step of its own, with the row each other track
    /// plays then, for as long as the key is down if it's held.
    pub(crate) fn audition_track(&mut self, section: usize, step: usize) {
        self.handle.send(Command::Audition {
            section,
            section_step: step,
            track: Some(self.cursor.track),
        });
        self.started = Some(Momentary::Audition);
    }

    /// Scroll an open guide, stopping when its last row reaches the bottom of the window.
    fn guide_key(&mut self, key: Key) {
        let Some(guide) = self.guide else {
            self.mode = Mode::Normal;
            return;
        };
        let last = guide
            .lines(self.guide_width)
            .len()
            .saturating_sub(self.guide_page);
        let half = (self.guide_page / 2).max(1);
        self.guide_scroll = match key.code {
            KeyCode::Char('d') if key.ctrl => self.guide_scroll + half,
            KeyCode::Char('u') if key.ctrl => self.guide_scroll.saturating_sub(half),
            KeyCode::Char('j') | KeyCode::Down => self.guide_scroll + 1,
            KeyCode::Char('k') | KeyCode::Up => self.guide_scroll.saturating_sub(1),
            KeyCode::Char('g') => 0,
            KeyCode::Char('G') => last,
            KeyCode::Char('?') => {
                self.guide = None;
                self.open_guide_picker();
                return;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.guide = None;
                self.mode = Mode::Normal;
                return;
            }
            _ => self.guide_scroll,
        }
        .min(last);
    }

    pub(crate) fn run(&mut self, cmd: Cmd) {
        let handled = match self.view {
            View::Chain => self.run_chain(cmd),
            View::Track => self.run_track_page(cmd),
            View::Mixer => self.run_mixer(cmd),
            View::Audio => self.run_audio_page(cmd),
            View::Project => self.run_project_page(cmd),
            View::Sample => self.run_sample_page(cmd),
            View::Song => self.run_song(cmd),
            View::Phrase => false,
        };
        if handled || self.run_tree(cmd) || self.run_grammar(cmd) {
            return;
        }
        match cmd {
            Cmd::ViewTop | Cmd::ViewMiddle | Cmd::ViewBottom => self.view_cursor(cmd),
            Cmd::HalfPageDown => self.half_page(1),
            Cmd::HalfPageUp => self.half_page(-1),
            Cmd::ScrollDown => self.scroll(3),
            Cmd::ScrollUp => self.scroll(-3),
            Cmd::Double => self.double(),
            Cmd::SlideLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.slide_loop(by * times as isize);
            }
            Cmd::RotateLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.rotate_loop(by * times as isize);
            }
            Cmd::StartLoopHere => self.start_loop_here(),
            Cmd::EndLoopHere => self.end_loop_here(),
            Cmd::ShowView(view) => {
                // `Space 0` shows the track's own sample, whatever a jump showed last.
                if view == View::Sample {
                    self.sample_page.pinned = None;
                }
                self.jump_to_view(view);
            }
            Cmd::Picker => self.open_view_picker(),
            Cmd::TrackPicker => self.open_track_picker(),
            Cmd::Help => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
            Cmd::Guides => self.open_guide_picker(),
            Cmd::PickValue => self.open_field_picker(),
            Cmd::Shape => self.open_shape(),
            Cmd::ReopenGenerator => self.reopen_generator(),
            Cmd::ApplyGenerator => self.apply_generator_here(),
            Cmd::BakeMods => self.bake_pitch_mods(),
            Cmd::PickSample => self.open_default_sample_picker(),
            Cmd::PickKit => self.open_kit_picker(),
            Cmd::Replace => self.begin_replace(),
            Cmd::Substitute => self.substitute(),
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.last_written = None;
                self.mode = Mode::Insert;
            }
            Cmd::EnterCommand => {
                self.command_line.clear();
                // From a selection on the sample page, the line starts with its range, as Vim's
                // does from Visual, and the selection ends.
                if self.view == View::Sample && self.sample_page.anchor.is_some() {
                    self.sample_page.end_selection();
                    self.command_line.push_str("'<,'>");
                }
                self.mode = Mode::Command;
            }
            Cmd::PlayFromCursor => {
                let step = self.section_step();
                // The song view plays from the row, so tracks stand where the song has them.
                let track = (self.view != View::Song).then_some(self.cursor.track);
                self.handle.send(Command::PlaySection {
                    section: self.cursor.section,
                    step,
                    track,
                });
                self.repeats_ignored = true;
            }
            Cmd::PlayFromStart => {
                self.handle.send(Command::PlayFrom { step: 0 });
                self.repeats_ignored = true;
            }
            Cmd::Audition => {
                if self.extent().plays(self.cursor.step) {
                    let section_step = self.section_step();
                    self.audition(section_step);
                } else {
                    self.audition_outside_loop();
                }
            }
            Cmd::PauseOrResume => {
                self.handle.send(Command::PauseOrResume);
                self.repeats_ignored = true;
            }
            Cmd::Quit => self.quit(false),
            Cmd::ToggleScale => self.toggle_scale(),
            Cmd::ToggleTimeline => self.toggle_timeline(),
            Cmd::ToggleInspector => self.toggle_inspector(),
            Cmd::ToggleFold => self.toggle_fold(),
            Cmd::FoldAll(closed) => self.fold_all(closed),
            Cmd::NarrowGrid => self.change_grid(false),
            Cmd::WidenGrid => self.change_grid(true),
            Cmd::NextTrack => self.move_track(1),
            Cmd::PreviousTrack => self.move_track(-1),
            Cmd::ToggleMute => {
                let track = self.cursor.track;
                if let Some(was) = self.levels.get(track).map(|live| live.mute) {
                    self.toggle_mute();
                    if self.levels[track].mute != was {
                        self.started = Some(Momentary::Mute { track, was });
                    }
                }
            }
            Cmd::ToggleSolo => {
                let track = self.cursor.track;
                if let Some(was) = self.levels.get(track).map(|live| live.solo) {
                    self.toggle_solo();
                    if self.levels[track].solo != was {
                        self.started = Some(Momentary::Solo { track, was });
                    }
                }
            }
            Cmd::MuteOnGrid => {
                self.switch_on_grid(false);
                self.repeats_ignored = true;
            }
            Cmd::SoloOnGrid => {
                self.switch_on_grid(true);
                self.repeats_ignored = true;
            }
            Cmd::ToggleFill => {
                let was = self.fill;
                self.set_fill(!was);
                self.started = Some(Momentary::Fill { was });
            }
            Cmd::Realign => {
                self.realign();
                self.repeats_ignored = true;
            }
            Cmd::RealignTrack => {
                self.realign_track();
                self.repeats_ignored = true;
            }
            Cmd::GiveBack => self.give_back_faders(false),
            Cmd::GiveBackAll => self.give_back_faders(true),
            Cmd::ClearMutes => {
                // A held mute or solo stays cleared when it's let go.
                if self.momentary.is_some_and(|held| {
                    matches!(held.what, Momentary::Mute { .. } | Momentary::Solo { .. })
                }) {
                    self.momentary = None;
                }
                self.clear_live();
            }
            Cmd::ToggleRecord => self.toggle_recording(),
            Cmd::Undo => {
                let before = self.loop_place();
                if self.history.undo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                    if self.mode.is_visual() {
                        self.undid_while_selecting(true);
                    }
                } else {
                    self.status = "Nothing to undo".to_owned();
                }
            }
            Cmd::Redo => {
                let before = self.loop_place();
                if self.history.redo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                    if self.mode.is_visual() {
                        self.undid_while_selecting(false);
                    }
                } else {
                    self.status = "Nothing to redo".to_owned();
                }
            }
            _ => {}
        }
    }
}
