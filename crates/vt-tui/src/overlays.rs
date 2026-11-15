//! What's drawn over a page and takes its keys while it's open: the picker and the explorer, the `:` line, the which-key popup and the guides, the shape panel, a macro's mapping view and `/`'s field jump.

pub mod command;
pub(crate) mod field_jump;
pub(crate) mod guides;
pub mod help;
pub mod mapping;
pub mod picker;
pub mod shape;
