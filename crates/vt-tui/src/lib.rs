//! The vimtracker terminal program.
//!
//! - [`app`]: `App`, the program's state, and what spans its pages: key dispatch, the modes, held
//!   keys, the jump list and the `:` runner.
//! - [`pages`]: each page as a component, with its keys, its edits and its drawing.
//! - [`overlays`]: what's drawn over a page and takes its keys: the picker, the `:` line, the
//!   guides and keys, the shape panel, a macro's mapping and `/`'s field jump.
//! - [`services`]: what every page shares, from the song and its history to the helper threads.
//! - [`screen`]: what's always on screen: the status line, the system readout and the main meter.
//! - [`widgets`]: the pieces pages are drawn from, the colours and the glyphs.
//! - [`graphics`]: pictures through kitty's graphics protocol.
//! - [`input`]: keys, the binding table, the modes and views, typed values and the mouse wheel.
//! - [`roto`]: the Roto-Control.
//! - [`offline`]: the subcommands that run without a screen.

#![warn(unreachable_pub)]

pub mod app;
pub mod graphics;
pub mod input;
pub mod offline;
pub mod overlays;
pub mod pages;
pub mod roto;
pub mod run;
pub mod screen;
pub mod screenshot;
pub mod services;
pub mod widgets;
