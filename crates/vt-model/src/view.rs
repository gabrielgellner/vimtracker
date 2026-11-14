//! How the views show a song, kept with it so a project opens as it was left: which groups are
//! folded (decision 305). None of it changes how the song plays, and undo doesn't go back over it.

use serde::{Deserialize, Serialize};

/// The `view` section of `song.ron`, left out while it holds nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewState {
    /// The groups folded, by number from 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folded: Vec<u8>,
    /// The groups opened, by number from 1. A group in neither list is folded while it's empty and
    /// open once a track joins it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opened: Vec<u8>,
}

impl ViewState {
    /// Whether there's nothing to keep, so the section is left out.
    pub fn is_empty(&self) -> bool {
        self.folded.is_empty() && self.opened.is_empty()
    }
}
