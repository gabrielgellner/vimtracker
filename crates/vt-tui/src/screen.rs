//! What's always on screen around the page: the status line, how the program is running, and the main output's meter.

pub mod describe;
pub mod meter;
pub mod status;
pub(crate) mod system;

use crate::services::audio::{LoadReadout, SlowdownReadout};

/// What the main loop measures for the screen around the page: how the program is running, and
/// the scope's image beside the meter.
#[derive(Debug)]
pub struct Readouts {
    /// The CPU figure on the status line.
    pub cpu: LoadReadout,
    /// How slowed the audio thread is, for the leaf beside it.
    pub slowdown: SlowdownReadout,
    /// The id of the scope's image, and whether the last screen drawn had room for it.
    pub scope_image: Option<u32>,
    pub scope_shown: bool,
}

impl Default for Readouts {
    fn default() -> Self {
        Self {
            cpu: LoadReadout::default(),
            slowdown: SlowdownReadout::default(),
            scope_image: None,
            scope_shown: true,
        }
    }
}
