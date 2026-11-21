//! What's drawn over a page and takes its keys while it's open: the picker and the explorer, the `:` line, the which-key popup and the guides, the shape panel, a macro's mapping view, `/`'s field jump and the macro panel.

pub mod command;
pub(crate) mod field_jump;
pub(crate) mod guides;
pub mod help;
pub mod macro_panel;
pub mod mapping;
pub mod picker;
pub mod shape;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_model::selection::Field;

use std::time::Instant;

use crate::app::Selection;
use crate::app::grammar::Grammar;
use crate::input::bindings::{Cmd, Resolved, Table};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::pages::{Ctx, CtxMut, Outcome};
use command::CommandLine;
use field_jump::FieldJump;
use guides::GuideView;
use help::Help;
use macro_panel::MacroPanel;
use mapping::MappingView;
use picker::Picker;
use picker::exploring::Explorer;
use shape::ShapePanel;

/// One of the program's overlays as a component, as a [`crate::pages::Page`] is: its own state,
/// its keys and its drawing. It's given what a page is to run a key, and answers with the same
/// [`Outcome`], so what only `App` can do for it is asked for the way a page asks.
///
/// It differs from a page where overlays do. Every overlay has a line that's typed into, a
/// search, a value or a name, so it takes keys as text as well as through the table. And it
/// takes no count: an overlay that counts keeps its own digits, as the picker does.
pub(crate) trait Overlay {
    /// The mode the keys are in while it's open, which its rows of the binding table are under.
    const MODE: Mode;

    /// Its rows of the binding table, as `(keys, Cmd, description)`, in the order which-key and
    /// `Space k` list them.
    const KEYS: &'static [(&'static str, Cmd, &'static str)];

    /// Run one of its keys from the table, or pass it back where it means nothing as things
    /// are, to be taken as text.
    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome;

    /// A key as it reads one, where it takes no notice of what's held with it: the same key
    /// without that.
    fn reads(&self, key: Key) -> Key {
        key
    }

    /// Whether every key is text for now: a search, a value or a name is being typed.
    fn takes_text(&self) -> bool {
        false
    }

    /// A mode whose rows of the table come before its own for now, the token editor's while the
    /// cursor is in a field of tokens (decision 456): a key that mode doesn't bind is its own
    /// mode's.
    fn inner_mode(&self) -> Option<Mode> {
        None
    }

    /// Whether a key types itself now, before the table reads it: a hex digit among `Weights`'
    /// digits, where `a` to `f` are digits rather than keys.
    fn types_now(&self, _key: Key) -> bool {
        false
    }

    /// Take a key as text: any key while [`Overlay::takes_text`], and otherwise one the table
    /// doesn't bind in its mode or that [`Overlay::key`] passed back.
    fn text(&mut self, _key: Key, _ctx: &mut CtxMut) -> Outcome {
        Outcome::Passed
    }

    /// The first key of one of its sequences was typed, and the next is waited for.
    fn waiting(&mut self) {}

    /// Draw it over the page.
    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx);
}

/// What a key means to an overlay, by the binding table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Meant {
    /// One of its rows of the table.
    Cmd(Cmd),
    /// Nothing in the table: text.
    Text(Key),
    /// The start of one of its sequences, or the end of one given up: nothing yet.
    Wait,
}

/// What a key means in an overlay's mode, after the keys typed before it and still waiting,
/// which it joins or ends. A sequence that isn't bound gives up its earlier keys and its last
/// is read again alone, so a `g` not followed by `g` is dropped, and `Esc` after the start of
/// one only lets it go.
#[cfg(test)]
pub(crate) fn meant(table: &Table, mode: Mode, pending: &mut Vec<Key>, key: Key) -> Meant {
    meant_in(table, &[mode], pending, key)
}

/// What a key means in the first of some modes that binds it, as [`meant`] reads one.
pub(crate) fn meant_in(table: &Table, modes: &[Mode], pending: &mut Vec<Key>, key: Key) -> Meant {
    if key.code == KeyCode::Esc && !pending.is_empty() {
        pending.clear();
        return Meant::Wait;
    }
    pending.push(key);
    loop {
        match table.resolve_in(modes, pending) {
            Resolved::Command(cmd) => {
                pending.clear();
                return Meant::Cmd(cmd);
            }
            Resolved::Prefix => return Meant::Wait,
            Resolved::Unbound if pending.len() > 1 => {
                pending.clear();
                pending.push(key);
            }
            Resolved::Unbound => {
                pending.clear();
                return Meant::Text(key);
            }
        }
    }
}

/// Give an overlay a key: as text while it's typing, and otherwise as the table reads it, a key
/// it has no row for, or passes back, being text too. `pending` is the keys of a sequence typed
/// so far.
pub(crate) fn press<O: Overlay>(
    overlay: &mut O,
    table: &Table,
    pending: &mut Vec<Key>,
    key: Key,
    ctx: &mut CtxMut,
) -> Outcome {
    let key = overlay.reads(key);
    if overlay.takes_text() || (pending.is_empty() && overlay.types_now(key)) {
        return overlay.text(key, ctx);
    }
    let inner = overlay.inner_mode();
    let modes = match inner {
        Some(inner) => &[inner, O::MODE][..],
        None => &[O::MODE][..],
    };
    match meant_in(table, modes, pending, key) {
        Meant::Cmd(cmd) => match overlay.key(cmd, ctx) {
            Outcome::Passed => overlay.text(key, ctx),
            outcome => outcome,
        },
        Meant::Text(key) => overlay.text(key, ctx),
        Meant::Wait => {
            if !pending.is_empty() {
                overlay.waiting();
            }
            Outcome::Done
        }
    }
}

/// Give an overlay a key as the program does, [`press`], with what `App` lends it: the table,
/// the keys of a sequence typed so far, which which-key lists what follows from when the first
/// was typed, and what a page is given to run a key.
pub(crate) fn give<O: Overlay>(
    overlay: &mut O,
    table: &Table,
    grammar: &mut Grammar,
    mut ctx: CtxMut,
    key: Key,
    now: Instant,
) -> Outcome {
    let was_waiting = !grammar.pending.is_empty();
    let outcome = press(overlay, table, &mut grammar.pending, key, &mut ctx);
    match (was_waiting, grammar.pending.is_empty()) {
        (false, false) => grammar.pending_since = Some(now),
        (_, true) => grammar.pending_since = None,
        (true, false) => {}
    }
    outcome
}

/// The overlays that are open, and what they keep between their openings: the picker and the
/// explorer's files, places and tags, the `:` line and its history, how far the help and the
/// guide have scrolled, the shape panel and a macro's mapping view.
pub struct Overlays {
    /// The open picker or explorer.
    pub picker: Option<Picker>,
    /// The file explorer: the kit in use, the files it reads and plays, its places and tags, and
    /// what a folder chosen in it goes on to.
    pub explorer: Explorer,
    /// The `:` line as typed so far, its list of matches and the lines run before.
    pub command: CommandLine,
    /// `Space k`'s list of every key, and how far it has scrolled.
    pub help: Help,
    /// The open guide, how far it has scrolled, and how many of its rows fit.
    pub guide: GuideView,
    /// The open shape panel.
    pub shape: Option<ShapePanel>,
    /// The open mapping view of a macro.
    pub mapping: Option<MappingView>,
    /// A selection to shape once the command for its empty FX column is chosen, and its field.
    pub(crate) shape_command: Option<(Selection, Field)>,
    /// The search for a setting under way, `/` on the track page or the mixer.
    pub field_jump: Option<FieldJump>,
    /// The macro panel at the bottom of the screen, whether it shows, and where its cursor is.
    pub macro_panel: MacroPanel,
    /// Whether the line describing the setting under the cursor shows, `Space d`, for the session
    /// only, not in the song.
    pub describing: bool,
}

impl Overlays {
    /// None open, and an explorer that reads files at this rate.
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            picker: None,
            explorer: Explorer::new(rate),
            command: CommandLine::default(),
            help: Help::default(),
            guide: GuideView::default(),
            shape: None,
            mapping: None,
            shape_command: None,
            field_jump: None,
            macro_panel: MacroPanel::default(),
            describing: false,
        }
    }
}
