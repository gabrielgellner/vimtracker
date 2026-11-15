//! Moving through tracks, chains and phrases: `gd`, cloning, what uses a phrase or a chain, and
//! the moves between a chain's rows every view shares.

use std::sync::Arc;

use vt_model::action::Action;
use vt_model::bloom::{Bloom, Path};
use vt_model::song::{ChainEntry, lowest_unused};

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::overlays::command::parse::Line;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::track::Control;
use crate::services::Services;

impl App {
    /// `gd`: open the item under the cursor, creating it on an empty reference.
    fn open_item(&mut self) {
        match self.modes.view {
            View::Chain => self.open_row(),
            View::Song => self.open_section_chain(),
            View::Track if self.track_control() == Control::Sample => {
                self.jump_to_view(View::Sample);
            }
            View::Track => {
                if !self.open_macro_here() {
                    self.services.status =
                        "g d opens the sample page from the sample, in the Machine's title, and a macro from a setting it holds"
                            .to_owned();
                }
            }
            View::Phrase => {
                if !self.open_slice_of_step() {
                    self.services.status =
                        "g d opens the sample page from an SLI's value".to_owned();
                }
            }
            View::Mixer | View::Audio | View::Project | View::Sample => {}
        }
    }

    fn open_row(&mut self) {
        let Some(chain) = self.services.chain_id() else {
            self.services.status = "This track has no chain".to_owned();
            return;
        };
        let slot = self.services.cursor.slot;
        self.record_jump();
        self.enter_row(chain, slot);
    }

    /// Open the phrase on a row of a chain, adding one if the row is past the last.
    pub(crate) fn enter_row(&mut self, chain: u32, slot: usize) {
        if slot >= self.services.chain_entries().len() {
            let phrase = self
                .services
                .track_here()
                .map_or(0, |track| lowest_unused(&track.phrases));
            self.services.apply(&Action::InsertChainEntry {
                track: self.services.cursor.track,
                chain,
                at: slot,
                entry: ChainEntry {
                    phrase,
                    transpose: 0,
                },
            });
            self.services.status = format!("New phrase {phrase:02X}");
        }
        self.modes.view = View::Phrase;
        self.note_view(View::Phrase);
        self.go_to_loop_start();
        self.say_if_shared();
    }

    fn say_if_shared(&mut self) {
        let Some(phrase) = self.services.phrase_id() else {
            return;
        };
        let uses = self.services.phrase_uses(phrase).len();
        if uses > 1 {
            self.services.status =
                format!("Phrase {phrase:02X} is used {uses} times: gc makes a copy here");
        }
    }

    /// `gc`: copy the phrase at the cursor's slot to a new ID, and point this slot at the copy.
    fn clone_phrase(&mut self) {
        let (Some(chain), Some(old)) = (self.services.chain_id(), self.services.phrase_id()) else {
            self.services.status = "There is no phrase here to clone".to_owned();
            return;
        };
        let new = self
            .services
            .track_here()
            .map_or(0, |track| lowest_unused(&track.phrases));
        self.services.apply(&Action::ClonePhrase {
            track: self.services.cursor.track,
            chain,
            slot: self.services.cursor.slot,
            new,
        });
        self.services.status = if self.services.chain_uses() > 1 {
            format!(
                "Cloned {old:02X} to {new:02X}. This chain is shared, so all its uses change: gC copies it"
            )
        } else {
            format!("Cloned {old:02X} to {new:02X} here. Other uses keep {old:02X}")
        };
    }

    /// `:bloom`: grow branches from the phrase at the cursor's slot, the trunk, each a new phrase
    /// following the one before it and mutating as it copies, and put the path through them in
    /// the slot's place (decision 307). Each bloom draws a seed of its own, as `S` does.
    pub(crate) fn grow_bloom(&mut self, branches: Option<u8>, mutation: Option<u8>, path: Path) {
        let (Some(chain), Some(trunk)) = (self.services.chain_id(), self.services.phrase_id())
        else {
            self.services.status = "There is no phrase here to grow branches from".to_owned();
            return;
        };
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let first = Bloom::default();
        let bloom = Bloom {
            branches: branches.unwrap_or(first.branches),
            mutation: mutation.unwrap_or(first.mutation),
            path,
            seed: self.song().seed.wrapping_add(self.shape_seed),
            ..first
        };
        let before = Arc::clone(self.song());
        self.services.apply(&Action::Bloom {
            track: self.services.cursor.track,
            chain,
            slot: self.services.cursor.slot,
            scale: self.services.scale,
            bloom,
        });
        if Arc::ptr_eq(&before, self.song()) {
            return;
        }
        let grown: Vec<String> = self
            .services
            .track_here()
            .into_iter()
            .flat_map(|track| track.phrases.keys())
            .filter(|id| {
                !before.tracks[self.services.cursor.track]
                    .phrases
                    .contains_key(id)
            })
            .map(|id| format!("{id:02X}"))
            .collect();
        let shared = if self.services.chain_uses() > 1 {
            ". This chain is shared, so all its uses play it"
        } else {
            ""
        };
        self.services.status = format!(
            "{trunk:02X} grew {} {}, {}, each a mutation of the one before, played {path}{shared}",
            grown.len(),
            if grown.len() == 1 {
                "branch"
            } else {
                "branches"
            },
            grown.join(" "),
        );
    }

    /// `:bloom` alone: choose the path, each listed with what it plays for three branches.
    pub(crate) fn open_bloom_picker(&mut self) {
        self.modes.count = None;
        if self.services.phrase_id().is_none() {
            self.services.status = "There is no phrase here to grow branches from".to_owned();
            return;
        }
        let branches = Bloom::default().branches;
        let items = Path::ALL
            .map(|path| {
                let plays: Vec<String> = path
                    .places(usize::from(branches), 0)
                    .into_iter()
                    .map(|place| match place {
                        0 => "T".to_owned(),
                        branch => branch.to_string(),
                    })
                    .collect();
                let line = Line::Bloom {
                    branches: None,
                    mutation: None,
                    path: Some(path),
                };
                let meaning = match path {
                    Path::Shuffle => path.meaning().to_owned(),
                    _ => format!("{}: {}", path.meaning(), plays.join(" ")),
                };
                Item::new(path.word(), Pick::Line(line)).meaning(meaning)
            })
            .to_vec();
        self.show_picker(Picker::new("Grow three branches, played", items));
    }

    /// `gC`: copy this track's chain and all its phrases, and point this track at the copy.
    pub(crate) fn clone_chain(&mut self) {
        let Some(old) = self.services.chain_id() else {
            self.services.status = "This track has no chain".to_owned();
            return;
        };
        self.services.apply(&Action::CloneChain {
            section: self.services.cursor.section,
            track: self.services.cursor.track,
        });
        if let Some(new) = self.services.chain_id() {
            self.services.status =
                format!("Copied chain {old:02X} and its phrases to chain {new:02X}");
        }
    }

    /// `grr`: list where the phrase at the cursor plays.
    fn list_uses(&mut self) {
        let Some(phrase) = self.services.phrase_id() else {
            self.services.status = "There is no phrase here".to_owned();
            return;
        };
        let track = self.services.cursor.track;
        let items = self
            .services
            .phrase_uses(phrase)
            .into_iter()
            .map(|(section, slot)| {
                Item::new(
                    format!("Section {:02} slot {slot:02X}", section + 1),
                    Pick::Use {
                        section,
                        track,
                        slot,
                    },
                )
            })
            .collect();
        self.show_picker(Picker::new("Uses", items));
    }

    /// Go to a slot from the picker.
    pub(crate) fn open_use(&mut self, section: usize, track: usize, slot: usize) {
        self.record_jump();
        self.services.cursor.section = section;
        self.services.cursor.track = track;
        self.services.cursor.slot = slot;
        self.modes.view = View::Phrase;
        self.note_view(View::Phrase);
        self.go_to_loop_start();
        self.services.clamp_cursor(self.modes.view);
    }

    pub(crate) fn move_slot(&mut self, to: usize) {
        self.services.cursor.slot = to;
        self.go_to_loop_start();
        self.services.clamp_cursor(self.modes.view);
    }

    /// Run a command about the tree. These mean the same in each view they are bound in.
    pub(crate) fn run_tree(&mut self, cmd: Cmd) -> bool {
        let last = self.services.chain_entries().len().saturating_sub(1);
        match cmd {
            Cmd::OpenItem => self.open_item(),
            Cmd::JumpBack => self.jump_back(),
            Cmd::JumpForward => self.jump_forward(),
            Cmd::ListUses => self.list_uses(),
            Cmd::ClonePhrase => self.clone_phrase(),
            Cmd::CloneChain => self.clone_chain(),
            Cmd::NextSlot => self.move_slot((self.services.cursor.slot + 1).min(last)),
            Cmd::PreviousSlot => self.move_slot(self.services.cursor.slot.saturating_sub(1)),
            Cmd::LastSlot => self.move_slot(last),
            Cmd::FirstSlot => self.move_slot(0),
            _ => return false,
        }
        self.modes.count = None;
        true
    }

    /// `:chain n`: point this track at a chain, creating it if needed.
    pub(crate) fn set_track_chain(&mut self, chain: u32) {
        self.services.apply(&Action::SetTrackChain {
            section: self.services.cursor.section,
            track: self.services.cursor.track,
            chain,
        });
        self.services.cursor.slot = 0;
        self.services.cursor.step = 0;
        self.services.clamp_cursor(self.modes.view);
    }
}

impl Services {
    /// Which sections and slots play a phrase of the cursor's track.
    pub fn phrase_uses(&self, phrase: u32) -> Vec<(usize, usize)> {
        let Some(track) = self.track_here() else {
            return Vec::new();
        };
        let mut uses = Vec::new();
        for (number, section) in self.song().sections.iter().enumerate() {
            let chain = section.chains.get(self.cursor.track).copied().flatten();
            let Some(chain) = chain.and_then(|id| track.chains.get(&id)) else {
                continue;
            };
            for (slot, entry) in chain.entries.iter().enumerate() {
                if entry.phrase == phrase {
                    uses.push((number, slot));
                }
            }
        }
        uses
    }

    /// How many sections play the cursor's chain.
    pub fn chain_uses(&self) -> usize {
        let Some(chain) = self.chain_id() else {
            return 0;
        };
        let sections = &self.song().sections;
        let chains = sections
            .iter()
            .filter_map(|section| section.chains.get(self.cursor.track));
        chains.filter(|c| **c == Some(chain)).count()
    }
}
