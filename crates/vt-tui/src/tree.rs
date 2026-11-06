//! Moving through tracks, chains and phrases: the chain view's keys, `gd`, cloning and the jump list.

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::constants::{MAX_TRANSPOSE, MIN_TRANSPOSE};
use vt_model::selection::Motion;
use vt_model::song::{ChainEntry, lowest_unused};

use crate::app::{App, Cursor, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How many places the jump list remembers.
const JUMP_LIMIT: usize = 100;

/// A column of the chain view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainColumn {
    Phrase,
    Transpose,
}

/// Somewhere `Ctrl-O` can go back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    view: View,
    cursor: Cursor,
}

impl App {
    /// The chain on the cursor's track.
    pub fn chain_id(&self) -> Option<u32> {
        let section = self.section_here()?;
        section.chains.get(self.cursor.track).copied().flatten()
    }

    /// The rows in the cursor's chain.
    pub fn chain_entries(&self) -> &[ChainEntry] {
        self.chain_id()
            .and_then(|id| self.song().chains.get(&id))
            .map_or(&[], |chain| chain.entries.as_slice())
    }

    fn here(&self) -> Place {
        Place {
            view: self.view,
            cursor: self.cursor,
        }
    }

    /// Remember this place before a jump away from it.
    pub(crate) fn record_jump(&mut self) {
        self.jumps.truncate(self.jump_index);
        self.jumps.push(self.here());
        if self.jumps.len() > JUMP_LIMIT {
            self.jumps.remove(0);
        }
        self.jump_index = self.jumps.len();
    }

    fn restore(&mut self, place: Place) {
        self.view = place.view;
        self.note_view(place.view);
        self.cursor = place.cursor;
        self.clamp_cursor();
    }

    fn jump_back(&mut self) {
        if self.jump_index == 0 {
            self.status = "Nothing to jump back to".to_owned();
            return;
        }
        if self.jump_index == self.jumps.len() {
            // Keep where we are, so `Tab` can come back.
            self.jumps.push(self.here());
        }
        self.jump_index -= 1;
        self.restore(self.jumps[self.jump_index]);
    }

    fn jump_forward(&mut self) {
        if self.jump_index + 1 >= self.jumps.len() {
            self.status = "Nothing to jump forward to".to_owned();
            return;
        }
        self.jump_index += 1;
        self.restore(self.jumps[self.jump_index]);
    }

    /// `gd`: open the item under the cursor, creating it on an empty reference.
    fn open_item(&mut self) {
        match self.view {
            View::Chain => self.open_row(),
            View::Song => self.open_section_chain(),
            View::Phrase | View::Track | View::Mixer => {}
        }
    }

    fn open_row(&mut self) {
        let Some(chain) = self.chain_id() else {
            self.status = "This track has no chain".to_owned();
            return;
        };
        let slot = self.cursor.slot;
        self.record_jump();
        if slot >= self.chain_entries().len() {
            let phrase = lowest_unused(&self.song().phrases);
            self.apply(&Action::InsertChainEntry {
                chain,
                at: slot,
                entry: ChainEntry {
                    phrase,
                    transpose: 0,
                },
            });
            self.status = format!("New phrase {phrase:02X}");
        }
        self.view = View::Phrase;
        self.note_view(View::Phrase);
        self.go_to_loop_start();
        self.say_if_shared();
    }

    /// Which sections, tracks and slots play a phrase.
    pub fn phrase_uses(&self, phrase: u32) -> Vec<(usize, usize, usize)> {
        let song = self.song();
        let mut uses = Vec::new();
        for (number, section) in song.sections.iter().enumerate() {
            for (track, chain) in section.chains.iter().enumerate() {
                let Some(chain) = chain.and_then(|id| song.chains.get(&id)) else {
                    continue;
                };
                for (slot, entry) in chain.entries.iter().enumerate() {
                    if entry.phrase == phrase {
                        uses.push((number, track, slot));
                    }
                }
            }
        }
        uses
    }

    /// How many places in the song play the cursor's chain.
    pub fn chain_uses(&self) -> usize {
        let Some(chain) = self.chain_id() else {
            return 0;
        };
        let sections = &self.song().sections;
        let chains = sections.iter().flat_map(|section| &section.chains);
        chains.filter(|c| **c == Some(chain)).count()
    }

    fn say_if_shared(&mut self) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        let uses = self.phrase_uses(phrase).len();
        if uses > 1 {
            self.status = format!("Phrase {phrase:02X} is used {uses} times: gc makes a copy here");
        }
    }

    /// `gc`: copy the phrase at the cursor's slot to a new ID, and point this slot at the copy.
    fn clone_phrase(&mut self) {
        let (Some(chain), Some(old)) = (self.chain_id(), self.phrase_id()) else {
            self.status = "There is no phrase here to clone".to_owned();
            return;
        };
        let new = lowest_unused(&self.song().phrases);
        self.apply(&Action::ClonePhrase {
            chain,
            slot: self.cursor.slot,
            new,
        });
        self.status = if self.chain_uses() > 1 {
            format!(
                "Cloned {old:02X} to {new:02X}. This chain is shared, so all its uses change: gC copies it"
            )
        } else {
            format!("Cloned {old:02X} to {new:02X} here. Other uses keep {old:02X}")
        };
    }

    /// `gC`: copy this track's chain and all its phrases, and point this track at the copy.
    fn clone_chain(&mut self) {
        let Some(old) = self.chain_id() else {
            self.status = "This track has no chain".to_owned();
            return;
        };
        self.apply(&Action::CloneChain {
            section: self.cursor.section,
            track: self.cursor.track,
        });
        if let Some(new) = self.chain_id() {
            self.status = format!("Copied chain {old:02X} and its phrases to chain {new:02X}");
        }
    }

    /// `grr`: list where the phrase at the cursor plays.
    fn list_uses(&mut self) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "There is no phrase here".to_owned();
            return;
        };
        let items = self
            .phrase_uses(phrase)
            .into_iter()
            .map(|(section, track, slot)| Item {
                label: format!(
                    "Section {:02} track {:02} slot {slot:02X}",
                    section + 1,
                    track + 1
                ),
                pick: Pick::Use {
                    section,
                    track,
                    slot,
                },
            })
            .collect();
        self.picker = Some(Picker::new("Uses", items));
        self.mode = Mode::Picker;
    }

    /// Go to a slot from the picker.
    pub(crate) fn open_use(&mut self, section: usize, track: usize, slot: usize) {
        self.record_jump();
        self.cursor.section = section;
        self.cursor.track = track;
        self.cursor.slot = slot;
        self.view = View::Phrase;
        self.note_view(View::Phrase);
        self.go_to_loop_start();
        self.clamp_cursor();
    }

    fn move_slot(&mut self, to: usize) {
        self.cursor.slot = to;
        self.go_to_loop_start();
        self.clamp_cursor();
    }

    /// Run a command about the tree. These mean the same in each view they are bound in.
    pub(crate) fn run_tree(&mut self, cmd: Cmd) -> bool {
        let last = self.chain_entries().len().saturating_sub(1);
        match cmd {
            Cmd::OpenItem => self.open_item(),
            Cmd::JumpBack => self.jump_back(),
            Cmd::JumpForward => self.jump_forward(),
            Cmd::ListUses => self.list_uses(),
            Cmd::ClonePhrase => self.clone_phrase(),
            Cmd::CloneChain => self.clone_chain(),
            Cmd::NextSlot => self.move_slot((self.cursor.slot + 1).min(last)),
            Cmd::PreviousSlot => self.move_slot(self.cursor.slot.saturating_sub(1)),
            Cmd::LastSlot => self.move_slot(last),
            Cmd::FirstSlot => self.move_slot(0),
            _ => return false,
        }
        self.count = None;
        true
    }

    /// Change a row. A large step is an octave of transpose, or `10` in hex of phrase numbers.
    fn adjust_row(&mut self, by: i32, large: bool) {
        let (Some(chain), Some(entry)) = (
            self.chain_id(),
            self.chain_entries().get(self.cursor.slot).copied(),
        ) else {
            return;
        };
        let entry = match self.chain_column {
            ChainColumn::Transpose => ChainEntry {
                transpose: (i32::from(entry.transpose) + if large { by * 12 } else { by })
                    .clamp(i32::from(MIN_TRANSPOSE), i32::from(MAX_TRANSPOSE))
                    as i8,
                ..entry
            },
            ChainColumn::Phrase => {
                let phrases = &self.song().phrases;
                let distance = if large { by.unsigned_abs() * 0x10 } else { 1 };
                let next = if by > 0 {
                    phrases
                        .range(entry.phrase.saturating_add(distance)..)
                        .next()
                } else {
                    phrases
                        .range(..=entry.phrase.saturating_sub(distance))
                        .next_back()
                        .filter(|(phrase, _)| **phrase != entry.phrase)
                };
                let Some((&phrase, _)) = next else {
                    return;
                };
                ChainEntry { phrase, ..entry }
            }
        };
        self.apply(&Action::SetChainEntry {
            chain,
            slot: self.cursor.slot,
            entry,
        });
    }

    /// Put this row's transpose back to 0. A row always plays a phrase, so `dd` is for removing it.
    fn reset_row(&mut self) {
        if self.chain_column != ChainColumn::Transpose {
            self.status = "x resets Transpose. A row needs a phrase, so dd removes it".to_owned();
            return;
        }
        if let (Some(chain), Some(entry)) = (
            self.chain_id(),
            self.chain_entries().get(self.cursor.slot).copied(),
        ) && entry.transpose != 0
        {
            self.apply(&Action::SetChainEntry {
                chain,
                slot: self.cursor.slot,
                entry: ChainEntry {
                    transpose: 0,
                    ..entry
                },
            });
        }
    }

    /// List the phrases to choose from for this row, with the one it plays highlighted.
    fn open_phrase_picker(&mut self) {
        if self.chain_column != ChainColumn::Phrase {
            self.status = "K chooses a phrase on the Phrase column".to_owned();
            return;
        }
        let Some(_) = self.chain_id() else {
            self.status = "This track has no chain".to_owned();
            return;
        };
        let current = self.chain_entries().get(self.cursor.slot).map(|e| e.phrase);
        let phrases: Vec<(u32, usize)> = self
            .song()
            .phrases
            .iter()
            .map(|(id, phrase)| (*id, phrase.len()))
            .collect();
        let position = phrases.iter().position(|(id, _)| Some(*id) == current);
        let items = phrases
            .iter()
            .map(|(id, length)| {
                let shared = if self.phrase_uses(*id).len() > 1 {
                    "*"
                } else {
                    ""
                };
                Item {
                    label: format!("{id:02X}{shared}  {length} steps"),
                    pick: Pick::RowPhrase(*id),
                }
            })
            .collect();
        let mut picker = Picker::new("Phrases", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Play a chosen phrase on this row, or on a new one if the cursor is past the last.
    pub(crate) fn pick_row_phrase(&mut self, phrase: u32) {
        let Some(chain) = self.chain_id() else {
            return;
        };
        let slot = self.cursor.slot;
        let action = match self.chain_entries().get(slot).copied() {
            Some(entry) => Action::SetChainEntry {
                chain,
                slot,
                entry: ChainEntry { phrase, ..entry },
            },
            None => Action::InsertChainEntry {
                chain,
                at: slot,
                entry: ChainEntry {
                    phrase,
                    transpose: 0,
                },
            },
        };
        self.apply(&action);
    }

    /// Run a command in the chain view. Returns false if it means the same as anywhere.
    pub(crate) fn run_chain(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let rows = self.chain_entries().len();
        let chain = self.chain_id();
        match cmd {
            Cmd::Move(Motion::Down) => self.move_slot((self.cursor.slot + times).min(rows)),
            Cmd::Move(Motion::Up) => self.move_slot(self.cursor.slot.saturating_sub(times)),
            Cmd::Move(Motion::First) => self.move_slot(0),
            Cmd::Move(Motion::Last) => self.move_slot(rows),
            Cmd::Audition if self.cursor.slot < rows => {
                let section_step = self.section_step();
                self.audition(section_step);
            }
            Cmd::Audition => {}
            Cmd::Move(Motion::Left | Motion::RowStart) => self.chain_column = ChainColumn::Phrase,
            Cmd::Move(Motion::Right | Motion::RowEnd) => {
                self.chain_column = ChainColumn::Transpose;
            }
            Cmd::Increase => self.adjust_row(times as i32, false),
            Cmd::Decrease => self.adjust_row(-(times as i32), false),
            Cmd::IncreaseLarge => self.adjust_row(times as i32, true),
            Cmd::DecreaseLarge => self.adjust_row(-(times as i32), true),
            Cmd::ClearField => self.reset_row(),
            Cmd::PickValue => self.open_phrase_picker(),
            Cmd::RemoveRow => {
                if let (Some(chain), Some(entry)) =
                    (chain, self.chain_entries().get(self.cursor.slot).copied())
                {
                    self.yanked_row = Some(entry);
                    self.apply(&Action::RemoveChainEntry {
                        chain,
                        at: self.cursor.slot,
                    });
                    self.clamp_cursor();
                }
            }
            Cmd::CopyRow => {
                if let Some(entry) = self.chain_entries().get(self.cursor.slot).copied() {
                    self.yanked_row = Some(entry);
                    self.status = format!("Copied phrase {:02X}", entry.phrase);
                }
            }
            Cmd::Paste => match (chain, self.yanked_row) {
                (Some(chain), Some(entry)) => {
                    let at = (self.cursor.slot + 1).min(rows);
                    self.apply(&Action::InsertChainEntry { chain, at, entry });
                    self.cursor.slot = at;
                }
                _ => self.status = "No chain row copied yet".to_owned(),
            },
            Cmd::EnterInsert => {
                if self.chain_column == ChainColumn::Transpose && self.cursor.slot >= rows {
                    self.status = "Add a phrase to this row first".to_owned();
                } else {
                    self.entry = Entry::default();
                    self.mode = Mode::Insert;
                }
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// Type a phrase ID in hex, or a transpose in semitones, then `Enter`.
    pub(crate) fn insert_chain_row(&mut self, key: KeyCode) {
        let phrase = self.chain_column == ChainColumn::Phrase;
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter => self.commit_chain_row(),
            KeyCode::Char(c) if phrase && c.is_ascii_hexdigit() && self.entry.text.len() < 8 => {
                self.entry.text.push(c.to_ascii_uppercase());
            }
            KeyCode::Char(c)
                if !phrase
                    && self.entry.text.len() < 4
                    && (c.is_ascii_digit()
                        || (self.entry.text.is_empty() && matches!(c, '-' | '+'))) =>
            {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_chain_row(&mut self) {
        let text = std::mem::take(&mut self.entry.text);
        self.mode = Mode::Normal;
        let Some(chain) = self.chain_id() else {
            return;
        };
        let slot = self.cursor.slot;
        let existing = self.chain_entries().get(slot).copied();
        let action = match self.chain_column {
            ChainColumn::Phrase => {
                let Ok(phrase) = u32::from_str_radix(&text, 16) else {
                    return;
                };
                match existing {
                    Some(entry) => Action::SetChainEntry {
                        chain,
                        slot,
                        entry: ChainEntry { phrase, ..entry },
                    },
                    None => Action::InsertChainEntry {
                        chain,
                        at: slot,
                        entry: ChainEntry {
                            phrase,
                            transpose: 0,
                        },
                    },
                }
            }
            ChainColumn::Transpose => {
                let Ok(transpose) = text.parse::<i8>() else {
                    self.status = format!("Transpose goes from {MIN_TRANSPOSE} to {MAX_TRANSPOSE}");
                    return;
                };
                let Some(entry) = existing else {
                    return;
                };
                Action::SetChainEntry {
                    chain,
                    slot,
                    entry: ChainEntry { transpose, ..entry },
                }
            }
        };
        self.apply(&action);
    }

    /// `:chain n`: point this track at a chain, creating it if needed.
    pub(crate) fn set_track_chain(&mut self, chain: u32) {
        self.apply(&Action::SetTrackChain {
            section: self.cursor.section,
            track: self.cursor.track,
            chain,
        });
        self.cursor.slot = 0;
        self.cursor.step = 0;
        self.clamp_cursor();
    }
}
