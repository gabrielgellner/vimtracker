//! What's drawn over a page and takes its keys while it's open: the picker and the explorer, the `:` line, the which-key popup and the guides, the shape panel, a macro's mapping view and `/`'s field jump.

pub mod command;
pub(crate) mod field_jump;
pub(crate) mod guides;
pub mod help;
pub mod mapping;
pub mod picker;
pub mod shape;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_model::selection::Field;

use crate::app::Selection;
use crate::input::bindings::{Cmd, Resolved, Table};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::pages::{Ctx, CtxMut, Outcome};
use command::completion::CommandMenu;
use command::history::CommandHistory;
use guides::Guide;
use mapping::MappingView;
use picker::Picker;
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

    /// Whether every key is text for now: a search, a value or a name is being typed.
    fn takes_text(&self) -> bool {
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
pub(crate) fn meant(table: &Table, mode: Mode, pending: &mut Vec<Key>, key: Key) -> Meant {
    if key.code == KeyCode::Esc && !pending.is_empty() {
        pending.clear();
        return Meant::Wait;
    }
    pending.push(key);
    loop {
        match table.resolve(mode, pending) {
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
    if overlay.takes_text() {
        return overlay.text(key, ctx);
    }
    match meant(table, O::MODE, pending, key) {
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

/// The overlays that are open, and what they keep between their openings: the picker, the `:`
/// line and its history, how far the help and the guide have scrolled, the shape panel and a
/// macro's mapping view.
pub struct Overlays {
    /// The open picker or explorer.
    pub picker: Option<Picker>,
    /// The `:` line as typed so far.
    pub command_line: String,
    /// The list of matches above the command line, while there's more than one.
    pub command_menu: Option<CommandMenu>,
    /// The command lines run before, which `Up` and `Down` go through.
    pub command_history: CommandHistory,
    /// How far the which-key help has scrolled.
    pub help_scroll: usize,
    /// The open guide, how far it has scrolled, and how many of its rows fit.
    pub guide: Option<Guide>,
    pub guide_scroll: usize,
    pub guide_page: usize,
    pub guide_width: usize,
    /// The open shape panel.
    pub shape: Option<ShapePanel>,
    /// The open mapping view of a macro.
    pub mapping: Option<MappingView>,
    /// A selection to shape once the command for its empty FX column is chosen, and its field.
    pub(crate) shape_command: Option<(Selection, Field)>,
}

impl Default for Overlays {
    fn default() -> Self {
        Self {
            picker: None,
            command_line: String::new(),
            command_menu: None,
            command_history: CommandHistory::default(),
            help_scroll: 0,
            guide: None,
            guide_scroll: 0,
            guide_page: 16,
            guide_width: 76,
            shape: None,
            mapping: None,
            shape_command: None,
        }
    }
}
