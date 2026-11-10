//! The Nerd Font symbols the program draws, each named after its Nerd Font name and codepoint.
//! kitty and Ghostty both ship the symbols built in (decision 101), so they need no font set up.
//! Each one means something, as a switch's state does, and none is there for decoration.

/// A switch that's on: `nf-fa-toggle_on`, U+F205.
pub const SWITCH_ON: char = '\u{f205}';
/// A switch that's off: `nf-fa-toggle_off`, U+F204.
pub const SWITCH_OFF: char = '\u{f204}';
