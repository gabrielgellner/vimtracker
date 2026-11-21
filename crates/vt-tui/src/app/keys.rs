//! Keys as they arrive: pressed, repeated and let go, the holds that last while a key is down, the
//! scope a key goes to, the views and popups keys open, and auditions.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEventKind};
use vt_engine::link::Command;
use vt_model::constants::MAX_PHRASE_STEPS;
use vt_model::step::{HIGHEST_OCTAVE, LOWEST_OCTAVE, MAX_NOTE, MIN_NOTE, octave_start};

use crate::controls::Page;
use crate::input::bindings::{Binding, Cmd, Resolved, adjusting_key, degree_key, note_key};
use crate::input::keys::{Key, shown};
use crate::input::mode::{Mode, View};
use crate::overlays;
use crate::overlays::guides;
use crate::overlays::help::Help;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::overlays::shape::ShapePanel;
use crate::pages::CtxMut;
use crate::pages::project;
use crate::pages::sample::SampleKey;
use crate::pages::sample::auditioning::{PADS, pad_key};
use crate::pages::sample::marking::MarkWait;
use std::sync::Arc;

use super::accent_slide::{HeldFlag, Moved, Written};
use super::{App, Entry, HOLD_TIME, KEYS_NOTES, which_key_due};
use crate::services::Momentary;

/// The keys that are down, and what they hold while they are: notes, Insert's accent, slide and
/// octave keys, and a momentary change.
#[derive(Debug, Default)]
pub struct HeldKeys {
    /// The key last pressed, as it is on the keyboard, until it's let go.
    pub(super) key_down: Option<KeyCode>,
    /// Whether the key that's down ignores its repeats, as a transport key and a held note do.
    pub(crate) repeats_ignored: bool,
    /// The keys holding notes, on the keyboard or in Insert, in the order they went down, so
    /// they play a chord.
    pub(super) notes: Vec<HeldNote>,
    /// The keys holding an accent, slide or octave in Insert, for the notes played while they're
    /// down.
    pub(crate) flags: Vec<HeldFlag>,
    /// Where the last note played in Insert went, for an accent, slide or octave tapped after it.
    pub(crate) last_written: Option<Written>,
    /// The note a tapped octave key last moved, so the same key tapped again puts it back.
    pub(crate) octave_moved: Option<Moved>,
    /// What the key that's down started that lasts while it's held.
    pub(super) momentary: Option<Held>,
    /// What the press being handled started, for [`App::press`] to tie to its key.
    pub(crate) started: Option<Momentary>,
}

/// A key holding a note, on the keyboard or in Insert, and the note it started.
#[derive(Debug, Clone, Copy)]
pub(super) struct HeldNote {
    key: KeyCode,
    track: usize,
    note: u8,
}

/// A momentary change, the key it's held by, and when that key went down.
#[derive(Debug, Clone, Copy)]
pub(super) struct Held {
    key: KeyCode,
    since: Instant,
    pub(super) what: Momentary,
}

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
        if matches!(self.modes.mode, Mode::Shape | Mode::Mapping) {
            return adjusting_key(key).is_some();
        }
        let mut keys = self.grammar.pending.clone();
        keys.push(key);
        self.table.resolve(self.key_scope(), &keys) != Resolved::Unbound
    }

    /// A key pressed.
    pub fn key(&mut self, key: Key, now: Instant) {
        self.services.now = now;
        // Any key ends a turn of the wheel, as its own undo step.
        self.end_wheel_gesture();
        // A real second press comes after a release, so a press of the key that's down is the
        // terminal repeating it. kitty on macOS has been seen sending a held `a`'s repeats as
        // presses. A key holding a note is down too, even after another key has gone down.
        if self.held.key_down == Some(key.physical())
            || self.holds_note(key)
            || self.holds_flag(key)
        {
            self.key_repeated(key, now);
            return;
        }
        self.held.key_down = Some(key.physical());
        self.held.repeats_ignored = false;
        self.press(key, now);
    }

    /// A key the terminal repeats while it's held. A transport key acts once however long it's
    /// held, and a note on the keyboard is already held; anything else repeats as a press.
    pub fn key_repeated(&mut self, key: Key, now: Instant) {
        self.services.now = now;
        self.end_wheel_gesture();
        if (self.held.repeats_ignored && self.held.key_down == Some(key.physical()))
            || self.holds_note(key)
            || self.holds_flag(key)
        {
            return;
        }
        self.services.repeating = true;
        self.press(key, now);
        self.services.repeating = false;
    }

    /// A key let go: its held note ends, leaving any others held, and so does what it started
    /// that lasts while it's held.
    pub fn key_released(&mut self, key: Key, now: Instant) {
        self.services.now = now;
        if self.held.key_down == Some(key.physical()) {
            self.held.key_down = None;
            self.held.repeats_ignored = false;
        }
        if let Some(at) = self
            .held
            .notes
            .iter()
            .position(|held| held.key == key.physical())
        {
            let held = self.held.notes.remove(at);
            self.let_go_of(held);
        }
        self.let_go_of_flag(key);
        if self
            .held
            .momentary
            .is_some_and(|held| held.key == key.physical())
        {
            self.end_momentary(now);
        }
        self.settle_sample_page();
    }

    /// End what a key started that lasts while it's held. Down for less than [`HOLD_TIME`], it was
    /// a tap, and stays as it is: a mute or solo stays switched, and an audition plays out. Held
    /// longer, a mute or solo goes back to how it was, and an audition stops.
    fn end_momentary(&mut self, now: Instant) {
        let Some(held) = self.held.momentary.take() else {
            return;
        };
        let long = now.saturating_duration_since(held.since) >= HOLD_TIME;
        match held.what {
            Momentary::Mute { track, was } | Momentary::Solo { track, was } => {
                let mute = matches!(held.what, Momentary::Mute { .. });
                match self.services.live.levels.get_mut(track) {
                    Some(live) if long => {
                        if mute {
                            live.mute = was;
                            self.services
                                .handle
                                .send(Command::SetMute { track, mute: was });
                        } else {
                            live.solo = was;
                            self.services
                                .handle
                                .send(Command::SetSolo { track, solo: was });
                        }
                    }
                    // A save while it was down kept how it was before.
                    Some(_) => self.services.dirty = true,
                    None => {}
                }
            }
            Momentary::Fill { was } if long => self.set_fill(was),
            Momentary::Fill { .. } => {}
            Momentary::Audition => {
                self.services
                    .handle
                    .send(Command::EndAudition { held: long });
            }
            Momentary::AuditionNote { track, note } if long => {
                self.services
                    .handle
                    .send(Command::ReleasePreview { track, note });
            }
            Momentary::AuditionNote { track, note } => {
                self.services
                    .handle
                    .send(Command::FinishPreview { track, note });
            }
            // Only what it played itself, so a later one carries on.
            Momentary::SampleAudition { hearing } => {
                if long && hearing == self.sample_page.hearings {
                    self.services.handle.send(Command::StopFile);
                }
            }
        }
    }

    /// Whether this key is holding a note.
    fn holds_note(&self, key: Key) -> bool {
        self.held
            .notes
            .iter()
            .any(|held| held.key == key.physical())
    }

    /// End a held key's note, unless another key still down is holding the same one, as a key an
    /// octave up and one an octave down can.
    fn let_go_of(&mut self, held: HeldNote) {
        self.end_take(held.key);
        let shared = self
            .held
            .notes
            .iter()
            .any(|other| other.track == held.track && other.note == held.note);
        if !shared {
            self.services.handle.send(Command::ReleasePreview {
                track: held.track,
                note: held.note,
            });
        }
    }

    /// End every held note.
    fn let_go_of_held_notes(&mut self) {
        for held in std::mem::take(&mut self.held.notes) {
            self.let_go_of(held);
        }
    }

    /// The terminal window lost focus, so a key that's down may never be reported let go. Its
    /// note ends now, what it started that lasts while it's held ends as if it was let go now,
    /// and the next press of it is a press.
    pub fn focus_lost(&mut self, now: Instant) {
        self.services.now = now;
        self.held.key_down = None;
        self.held.repeats_ignored = false;
        self.let_go_of_held_notes();
        self.drop_flags();
        self.end_momentary(now);
        self.settle_sample_page();
    }

    /// Sound a note for as long as this key is down, ignoring the key's repeats. Each key held
    /// sounds its own note, so several play a chord.
    pub(super) fn hold_note(&mut self, key: Key, note: u8, velocity: u8) {
        let track = self.services.cursor.track;
        self.held.notes.push(HeldNote {
            key: key.physical(),
            track,
            note,
        });
        self.held.repeats_ignored = true;
        self.services.handle.send(Command::HoldPreview {
            track,
            note,
            velocity,
        });
    }

    /// Act on a press, or a repeat that acts as one.
    pub(crate) fn press(&mut self, key: Key, now: Instant) {
        let was = self.modes.mode;
        self.held.started = None;
        // The Gen page shows the phrase under the cursor, wherever a key or the time between
        // keys has moved it.
        self.gen_page.phrase = self.services.phrase_id();
        let before = Arc::clone(self.services.song());
        let said = self.services.status.clone();
        self.handle_key(key, now);
        self.keep_cursor_shown();
        self.gen_page.phrase = self.services.phrase_id();
        // Beside the grid, the panel follows the grid's cursor, or notes the device the keys
        // came into (decision 468).
        self.follow_after_key(&before, &said);
        // Something that lasts while its key is down is tied to that key, and doesn't repeat.
        // One already held ends as if let go now, as only one is kept.
        if let Some(what) = self.held.started.take() {
            self.end_momentary(now);
            self.held.momentary = Some(Held {
                key: key.physical(),
                since: now,
                what,
            });
            self.held.repeats_ignored = true;
        }
        // Leaving the keyboard, or Insert, lets go of what it's holding.
        if matches!(was, Mode::Keys | Mode::Insert) && self.modes.mode != was {
            self.let_go_of_held_notes();
            self.drop_flags();
            self.held.last_written = None;
            self.held.octave_moved = None;
        }
        self.settle_sample_page();
        // A selection keeps the song from when it started, and lets it go when it ends.
        if !self.modes.mode.is_visual() {
            self.selected_from = None;
        } else if self.selected_from.is_none() {
            self.selected_from = Some(Arc::clone(self.services.song()));
        }
    }

    pub(crate) fn handle_key(&mut self, key: Key, now: Instant) {
        match self.modes.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::VisualField
            | Mode::VisualStep
            | Mode::Chain
            | Mode::Track
            | Mode::Gen
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
            Mode::Command => self.command_line_key(key, now),
            Mode::Help => {
                let outcome = overlays::give(
                    &mut self.overlays.help,
                    &self.table,
                    &mut self.grammar,
                    CtxMut {
                        services: &mut self.services,
                        modes: &self.modes,
                        wheel: &mut self.wheel,
                    },
                    key,
                    now,
                );
                self.settled(outcome, false);
            }
            Mode::Guide => {
                let outcome = overlays::give(
                    &mut self.overlays.guide,
                    &self.table,
                    &mut self.grammar,
                    CtxMut {
                        services: &mut self.services,
                        modes: &self.modes,
                        wheel: &mut self.wheel,
                    },
                    key,
                    now,
                );
                self.settled(outcome, false);
            }
            Mode::Picker => self.picker_key(key, now),
            // The token editor's keys are the panel's to give, while its cursor is in them, and
            // otherwise the Gen page's field's.
            Mode::Tokens if self.overlays.shape.is_none() => self.gen_tokens_key(key, now),
            Mode::Shape | Mode::Tokens => self.shape_key(key, now),
            Mode::Mapping => self.mapping_key(key, now),
            Mode::Macros => self.macro_panel_key(key, now),
            Mode::Jump => {
                let page = self.jump_page();
                let Some(jump) = self.overlays.field_jump.as_mut() else {
                    return;
                };
                jump.lend(page);
                let outcome = overlays::give(
                    jump,
                    &self.table,
                    &mut self.grammar,
                    CtxMut {
                        services: &mut self.services,
                        modes: &self.modes,
                        wheel: &mut self.wheel,
                    },
                    key,
                    now,
                );
                self.settled(outcome, false);
            }
            Mode::Restore => self.restore_key(key),
            Mode::Keys => self.keys_key(key, now),
        }
    }

    /// Play the track's sound from a few keys, `a` `w` `s` `e` `d` `f`, without writing anything,
    /// while every other key works on the page as it does, so settings change as it plays. A note
    /// lasts as long as its key is down.
    fn keys_key(&mut self, key: Key, now: Instant) {
        if self.modes.view == View::Sample {
            self.sample_keys_key(key, now);
            return;
        }
        let plain = self.grammar.pending.is_empty() && !key.ctrl && !key.alt;
        match key.code {
            KeyCode::Esc | KeyCode::Char(';') if self.grammar.pending.is_empty() => {
                self.modes.mode = Mode::Normal;
                self.services.status.clear();
            }
            KeyCode::Char('<') if plain => {
                self.services.live.octave = (self.services.live.octave - 1).max(LOWEST_OCTAVE)
            }
            KeyCode::Char('>') if plain => {
                self.services.live.octave = (self.services.live.octave + 1).min(HIGHEST_OCTAVE)
            }
            KeyCode::Char(c) if plain && KEYS_NOTES.contains(c) => {
                if let Some(note) = self.played_key(c) {
                    self.hold_note(key, note, self.services.live.note_velocity);
                }
            }
            _ => {
                self.command_key_normal(key, now);
                // The keys only play on the track page.
                if self.modes.mode == Mode::Keys && self.modes.view != View::Track {
                    self.modes.mode = Mode::Normal;
                }
            }
        }
    }

    /// The note a key plays in the current octave. With a scale, the home row goes up through it
    /// from the root, and the row above is silent, so every key is in key.
    pub(super) fn played_key(&self, c: char) -> Option<u8> {
        let scale = self.services.scale_here();
        if scale.on {
            degree_key(c).map(|degree| scale.degree(degree, self.services.live.octave))
        } else {
            note_key(c).map(|semitones| {
                let midi = octave_start(self.services.live.octave) + i16::from(semitones);
                midi.clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8
            })
        }
    }

    /// What the popup should list, once the delay has passed.
    pub fn popup_rows(&self, now: Instant, delay: Duration) -> Vec<(String, &'static str)> {
        if self.grammar.pending.is_empty() || !which_key_due(self.grammar.pending_since, now, delay)
        {
            return Vec::new();
        }
        self.table
            .continuations_in(&self.key_scopes(), &self.grammar.pending)
    }

    /// The modes whose rows of the binding table apply now, the first's before the rest's: the
    /// token editor's before the shape panel's while its cursor is in a field of tokens.
    pub fn key_scopes(&self) -> Vec<Mode> {
        let scope = self.key_scope();
        match &self.overlays.shape {
            Some(panel) if scope == Mode::Shape && panel.tokens.is_some() => {
                vec![Mode::Tokens, scope]
            }
            _ => vec![scope],
        }
    }

    /// Which rows of the binding table apply now. Normal mode's keys depend on the view.
    pub fn key_scope(&self) -> Mode {
        match (self.modes.mode, self.modes.view) {
            (Mode::Keys | Mode::Normal, View::Track) => Mode::Track,
            (Mode::Normal, View::Gen) => Mode::Gen,
            (Mode::Normal, View::Chain) => Mode::Chain,
            (Mode::Normal, View::Mixer) => Mode::Mixer,
            (Mode::Normal, View::Audio) => Mode::Audio,
            (Mode::Normal, View::Project) => Mode::Project,
            (Mode::Normal, View::Sample) if self.sample_page.anchor.is_some() => Mode::SampleVisual,
            (Mode::Keys | Mode::Normal, View::Sample) => Mode::Sample,
            (Mode::Normal, View::Song) => Mode::Song,
            (mode, _) => mode,
        }
    }

    pub(crate) fn show_view(&mut self, view: View) {
        self.modes.count = None;
        // The audio page lists the devices afresh each time it opens.
        self.audio_page.audio.refresh |= view == View::Audio && self.modes.view != View::Audio;
        // Sections selected in the song view are left there.
        if view != View::Song {
            self.song_view.anchor = None;
        }
        self.modes.view = view;
        self.note_view(view);
        self.services.clamp_cursor(self.modes.view);
        if view != View::Phrase {
            self.stop_recording();
        }
        if view == View::Sample
            && let Err(why) = self.sample_page_here()
        {
            self.services.status = why;
        }
        // Gen shows the phrase under the cursor, its cursor where it was left (decision 466), in
        // the form it was last in (decision 467), and beside the grid on the device of the grid's
        // cursor's column (decision 468).
        if view == View::Gen {
            self.gen_form_shown();
        }
    }

    /// `Space 5` to `Space 7`, and `L` on to Macros: the track page on one of its pages, where its cursor was left, as a
    /// jump `C-o` comes back from. It's `App`'s, as it records the jump and shows the view.
    pub(crate) fn show_track_page(&mut self, page: Page) {
        if self.modes.view != View::Track || self.track_page.page != page {
            self.record_jump();
        }
        self.track_page.show(page);
        let song = Arc::clone(self.song());
        self.track_page
            .settle(song.tracks.get(self.services.cursor.track));
        self.show_view(View::Track);
    }

    /// Show a view, as a jump that `Ctrl-O` can undo.
    pub(crate) fn jump_to_view(&mut self, view: View) {
        if view != self.modes.view {
            self.record_jump();
        }
        self.show_view(view);
    }

    /// Record a view as most recently used.
    pub(crate) fn note_view(&mut self, view: View) {
        self.recent_views.retain(|other| *other != view);
        self.recent_views.insert(0, view);
    }

    /// Open the view picker, with the view before this one highlighted so `Enter` flips back. The
    /// track page is listed by the page it shows, where it's in the order, and then its other
    /// pages after every view; each item has the key that shows it beside it, from the table.
    pub(crate) fn open_view_picker(&mut self) {
        self.modes.count = None;
        let key = |cmd: Cmd| {
            self.table
                .rows()
                .find(|binding| binding.mode == Mode::Normal && binding.cmd == cmd)
                .map_or_else(String::new, |binding| shown(binding.keys))
        };
        let shown = self.track_page.page;
        // The Macros page has no key of its own: it's `L` from FX (decision 466).
        let page_item = |page: Page| {
            let meaning = match page {
                Page::Macros => format!("{}, L", key(Cmd::ShowTrackPage(Page::Fx))),
                page => key(Cmd::ShowTrackPage(page)),
            };
            Item::new(page.name().to_lowercase(), Pick::TrackPage(page)).meaning(meaning)
        };
        let mut items: Vec<Item> = self
            .recent_views
            .iter()
            .map(|view| match view {
                View::Track => page_item(shown),
                // The project's sub-pages are `Space 9`, then `H` or `L`.
                View::Project | View::Audio => {
                    let side = if *view == View::Audio { "L" } else { "H" };
                    Item::new(view.name(), Pick::View(*view))
                        .meaning(format!("{}, {side}", key(Cmd::ShowProject)))
                }
                _ => Item::new(view.name(), Pick::View(*view)).meaning(key(Cmd::ShowView(*view))),
            })
            .collect();
        items.extend(
            Page::ALL
                .into_iter()
                .filter(|page| *page != shown)
                .map(page_item),
        );
        let mut picker = Picker::new("Views", items).opened_on(Some(0));
        picker.move_highlight(1);
        self.show_picker(picker);
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
            rows.push(format!(
                "  {:<12} {}",
                shown(binding.keys),
                binding.description
            ));
        }
        rows
    }

    /// Hear a row, unless playback is using the tracks, for as long as the key is down if it's
    /// held.
    pub(crate) fn audition(&mut self, section_step: usize) {
        if let Some(started) = self.services.audition(section_step) {
            self.held.started = Some(started);
        }
    }

    /// A key in the sample page's keys mode. The pads play their slices, `<` and `>` go a page of
    /// them back or on, as they change octave on the track page's keyboard, `Esc` leaves, and
    /// every other key works on the page as it does, so `;` plays the slice the cursor is on.
    pub(crate) fn sample_keys_key(&mut self, key: Key, now: std::time::Instant) {
        use ratatui::crossterm::event::KeyCode;
        let plain = self.grammar.pending.is_empty() && !key.ctrl && !key.alt;
        let shown = self.sample_page_here();
        match (key.code, shown) {
            (KeyCode::Esc, _) if self.grammar.pending.is_empty() => {
                self.modes.mode = Mode::Normal;
                self.services.status.clear();
            }
            (KeyCode::Char(c @ ('<' | '>')), Ok(shown)) if plain => {
                self.services.status.clear();
                let from = self.sample_page.pads_from;
                let last = shown.slice_count().saturating_sub(1) / PADS * PADS;
                self.sample_page.pads_from = if c == '>' {
                    (from + PADS).min(last)
                } else {
                    from.saturating_sub(PADS)
                };
            }
            (KeyCode::Char(c), Ok(shown)) if plain && pad_key(c).is_some() => {
                self.modes.count = None;
                let number = self.sample_page.pads_from + pad_key(c).unwrap_or(0);
                if let Some(started) = self
                    .sample_page
                    .play_pad(&shown, number, &mut self.services)
                {
                    self.held.started = Some(started);
                }
            }
            _ => {
                let view = self.modes.view;
                self.command_key_normal(key, now);
                // The keys only play on the page they started on.
                if self.modes.mode == Mode::Keys && self.modes.view != view {
                    self.modes.mode = Mode::Normal;
                }
            }
        }
    }

    /// The guide for the page or view showing, which `Space ?` opens the list of guides on.
    /// Open the guide at the shape panel's fill, coming back to the panel when it closes. A fill
    /// without a section of its own opens at **Shaping a column**, and the status line says so.
    fn show_fill_guide(&mut self, fill: vt_model::shape::FillKind) {
        let (guide, heading, own) = guides::for_fill(fill.name());
        self.overlays.guide.show_at(guide, heading);
        self.services.status = if own {
            String::new()
        } else {
            format!("{} has no section of its own: {heading}", fill.name())
        };
        self.modes.mode = Mode::Guide;
    }

    fn guide_here(&self) -> guides::Guide {
        guides::for_view(self.modes.view, self.track_page.page)
    }

    /// Run a command the binding table resolved. A key goes, in this order, to the first that
    /// takes it, each passing back what isn't its own:
    ///
    /// 1. the overlay for the mode, the picker, the shape panel, the mapping view, the command
    ///    line, Insert, a jump, a guide or help, which [`App::handle_key`] gives its keys to
    ///    before they reach the table;
    /// 2. the page on screen, [`App::run_page`], through `Page::key` on a page that's a component;
    /// 3. on the track page and the mixer, what a key they passed back means across them, such as
    ///    `i`, `/`, `M`, `gm`, the Keys mode and the pickers, and then the track page's edits
    ///    ([`TrackPage::across`](crate::pages::track::TrackPage::across));
    /// 4. the tree of chains and phrases, [`App::run_tree`];
    /// 5. the grammar's operators, the motion after one, and the Visual modes,
    ///    [`App::run_grammar`];
    /// 6. what a key means anywhere, the `match` here.
    pub(crate) fn run(&mut self, cmd: Cmd) {
        if self.run_page(cmd) || self.run_tree(cmd) || self.run_grammar(cmd) {
            return;
        }
        match cmd {
            Cmd::Double => self.double(),
            Cmd::SlideLoop(by) => {
                let times = self.modes.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.slide_loop(by * times as isize);
            }
            Cmd::RotateLoop(by) => {
                let times = self.modes.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
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
            Cmd::ShowTrackPage(page) => self.show_track_page(page),
            Cmd::ShowProject => self.jump_to_view(project::last_shown(&self.recent_views)),
            Cmd::Picker => self.open_view_picker(),
            Cmd::TrackPicker => self.open_picker(self.services.track_picker()),
            Cmd::Help => {
                self.overlays.help = Help::over(self.help_rows());
                self.modes.mode = Mode::Help;
            }
            Cmd::Guides => match &self.overlays.shape {
                Some(panel) => self.show_fill_guide(panel.shape.kind),
                None => self.show_picker(guides::picker(self.guide_here())),
            },
            Cmd::ToggleMacroPanel => self.toggle_macro_panel(),
            Cmd::FocusMacroPanel => self.focus_macro_panel(),
            Cmd::ToggleDescription => {
                let on = !self.services.settings.describe;
                self.services.set_describing(on);
            }
            Cmd::Window(key) => self.window(key),
            Cmd::Shape => self.open_shape(),
            Cmd::Pattern => self.open_pattern(),
            // The sample page's marks: `'` and `m` wait for a letter, as an operator waits for a
            // motion, and `]'` and `['` are jumps, the jump list's.
            Cmd::Sample(key @ (SampleKey::Mark | SampleKey::NameSlice)) => {
                self.modes.count = None;
                self.grammar.awaiting_mark = Some(match key {
                    SampleKey::Mark => MarkWait::Jump,
                    _ => MarkWait::Name,
                });
            }
            Cmd::Sample(key @ (SampleKey::NextMark | SampleKey::PreviousMark)) => {
                let times = self.modes.count.take().unwrap_or(1).max(1);
                self.jump_to_next_mark(key == SampleKey::NextMark, times);
            }
            // With Gen's panel beside the grid, the panel moves to the column's device instead.
            Cmd::ReopenGenerator if self.gen_to_here() => {}
            Cmd::OpenGenerator => self.open_gen_here(),
            Cmd::ReopenGenerator => {
                let outcome = ShapePanel::reopen(&mut self.services, self.modes.view);
                self.settled(outcome, false);
            }
            Cmd::ApplyGenerator => self.services.apply_generator_here(),
            Cmd::BakeMods => {
                let selected = self.modes.mode.is_visual().then(|| {
                    let selection = self.current_selection();
                    self.leave_selection(selection);
                    selection.range
                });
                self.services.bake_pitch_mods(selected);
            }
            Cmd::MapMacro | Cmd::MacroMapping => {
                self.services.status =
                    "M and gm map settings to macros, on the track page".to_owned();
            }
            Cmd::Replace => self.begin_replace(),
            Cmd::Substitute => self.substitute(),
            Cmd::EnterInsert => {
                self.modes.entry = Entry::default();
                self.held.last_written = None;
                self.held.octave_moved = None;
                self.modes.mode = Mode::Insert;
            }
            Cmd::EnterCommand => {
                self.overlays.command.line.clear();
                // From a selection on the sample page, the line starts with its range, as Vim's
                // does from Visual, and the selection ends.
                if self.modes.view == View::Sample && self.sample_page.anchor.is_some() {
                    self.sample_page.end_selection();
                    self.overlays.command.line.push_str("'<,'>");
                }
                self.modes.mode = Mode::Command;
            }
            Cmd::PlayFromCursor => {
                let step = self.services.section_step(self.modes.view);
                // The song view plays from the row, so tracks stand where the song has them.
                let track = (self.modes.view != View::Song).then_some(self.services.cursor.track);
                self.services.handle.send(Command::PlaySection {
                    section: self.services.cursor.section,
                    step,
                    track,
                });
                self.held.repeats_ignored = true;
            }
            Cmd::PlayFromStart => {
                self.services.handle.send(Command::PlayFrom { step: 0 });
                self.held.repeats_ignored = true;
            }
            Cmd::Audition => {
                if self.services.extent().plays(self.services.cursor.step) {
                    let section_step = self.services.section_step(self.modes.view);
                    self.audition(section_step);
                } else {
                    self.audition_outside_loop();
                }
            }
            Cmd::PauseOrResume => {
                self.services.handle.send(Command::PauseOrResume);
                self.held.repeats_ignored = true;
            }
            Cmd::Quit => self.quit(false),
            Cmd::ToggleScale => self.toggle_scale(),
            Cmd::ToggleFold => self.toggle_fold(),
            Cmd::FoldAll(closed) => self.fold_all(closed),
            Cmd::NarrowGrid => self.change_grid(false),
            Cmd::WidenGrid => self.change_grid(true),
            Cmd::NextTrack => self.services.move_track(1, self.modes.view),
            Cmd::PreviousTrack => self.services.move_track(-1, self.modes.view),
            Cmd::NextGroupTrack => self.move_to_group(1),
            Cmd::PreviousGroupTrack => self.move_to_group(-1),
            Cmd::ToggleMute => {
                let track = self.services.cursor.track;
                if let Some(was) = self.services.live.levels.get(track).map(|live| live.mute) {
                    self.services.toggle_mute();
                    if self.services.live.levels[track].mute != was {
                        self.held.started = Some(Momentary::Mute { track, was });
                    }
                }
            }
            Cmd::ToggleSolo => {
                let track = self.services.cursor.track;
                if let Some(was) = self.services.live.levels.get(track).map(|live| live.solo) {
                    self.services.toggle_solo();
                    if self.services.live.levels[track].solo != was {
                        self.held.started = Some(Momentary::Solo { track, was });
                    }
                }
            }
            Cmd::MuteOnGrid => {
                self.switch_on_grid(false);
                self.held.repeats_ignored = true;
            }
            Cmd::SoloOnGrid => {
                self.switch_on_grid(true);
                self.held.repeats_ignored = true;
            }
            Cmd::ToggleFill => {
                let was = self.services.live.fill;
                self.set_fill(!was);
                self.held.started = Some(Momentary::Fill { was });
            }
            Cmd::Realign => {
                self.realign();
                self.held.repeats_ignored = true;
            }
            Cmd::RealignTrack => {
                self.realign_track();
                self.held.repeats_ignored = true;
            }
            Cmd::RotateSpeeds(every) => {
                self.rotate_speeds(every);
                self.held.repeats_ignored = true;
            }
            Cmd::GiveBack => self.services.give_back_faders(false),
            Cmd::GiveBackAll => self.services.give_back_faders(true),
            Cmd::ClearMutes => {
                // A held mute or solo stays cleared when it's let go.
                if self.held.momentary.is_some_and(|held| {
                    matches!(held.what, Momentary::Mute { .. } | Momentary::Solo { .. })
                }) {
                    self.held.momentary = None;
                }
                self.clear_live();
            }
            Cmd::ToggleRecord => self.toggle_recording(),
            Cmd::ToggleSampling => self.toggle_sampling(),
            Cmd::Undo => {
                let before = self.loop_place();
                if self.services.history.undo().is_some() {
                    // The version brought back may have been left in another scale.
                    self.services.follow_scale();
                    self.keep_row(before);
                    self.after_history();
                    if self.modes.mode.is_visual() {
                        self.undid_while_selecting(true);
                    }
                } else {
                    self.services.status = "Nothing to undo".to_owned();
                }
            }
            Cmd::Redo => {
                let before = self.loop_place();
                if self.services.history.redo().is_some() {
                    self.services.follow_scale();
                    self.keep_row(before);
                    self.after_history();
                    if self.modes.mode.is_visual() {
                        self.undid_while_selecting(false);
                    }
                } else {
                    self.services.status = "Nothing to redo".to_owned();
                }
            }
            _ => {}
        }
    }
}
