//! What's drawn over a page and takes its keys while it's open: the picker and the explorer, the `:` line, the which-key popup and the guides, the shape panel, a macro's mapping view and `/`'s field jump.

pub mod command;
pub(crate) mod field_jump;
pub(crate) mod guides;
pub mod help;
pub mod mapping;
pub mod picker;
pub mod shape;

use vt_model::selection::Field;

use crate::app::Selection;
use command::completion::CommandMenu;
use command::history::CommandHistory;
use guides::Guide;
use mapping::MappingView;
use picker::Picker;
use shape::ShapePanel;

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
