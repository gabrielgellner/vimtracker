//! What keys do in the chain view, `Space 2`: one track's chain, a row for each phrase it plays in
//! order and its transpose, and the row past the end, which adds one.
//!
//! `ChainView` is the view as a component (#320): the column its cursor is on and the row last
//! copied, its moves between the columns, the edits of a row, `yy` and `p`, and its drawing, in
//! `draw`, given a [`Ctx`]. It's a [`Page`], with its rows of the binding table, [`Page::KEYS`],
//! here beside it. Going to another row moves the cursor every page shares and the phrase view's
//! first row, so it's `App`'s, in the `impl App` below, as are `dd`, which keeps the cursor on a
//! row, the phrase picker, Insert and `;`. The tree's own keys, `gd`, `grr`, `gc` and `gC`, are in
//! `app/tree.rs`.

pub mod draw;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_model::action::Action;
use vt_model::constants::{MAX_TRANSPOSE, MIN_TRANSPOSE};
use vt_model::selection::Motion;
use vt_model::song::ChainEntry;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::Mode;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::{Ctx, CtxMut, Outcome, Page};
use crate::services::Services;

/// A column of the chain view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChainColumn {
    #[default]
    Phrase,
    Transpose,
}

/// The chain view as a component: the column its cursor is on, kept while other views are shown,
/// and the row `yy` and `dd` last copied, for `p`. Which track, chain and row it's on is the
/// program's cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainView {
    pub column: ChainColumn,
    pub(crate) yanked: Option<ChainEntry>,
}

impl ChainView {
    /// Run one of the view's own keys `times` times, or pass it back.
    fn key_on(&mut self, cmd: Cmd, times: usize, services: &mut Services) -> Outcome {
        match cmd {
            Cmd::Move(Motion::Left | Motion::RowStart) => self.column = ChainColumn::Phrase,
            Cmd::Move(Motion::Right | Motion::RowEnd) => self.column = ChainColumn::Transpose,
            Cmd::Increase => self.adjust(times as i32, false, services),
            Cmd::Decrease => self.adjust(-(times as i32), false, services),
            Cmd::IncreaseLarge => self.adjust(times as i32, true, services),
            Cmd::DecreaseLarge => self.adjust(-(times as i32), true, services),
            Cmd::ClearField => return self.reset(services),
            Cmd::CopyRow => {
                if let Some(entry) = services.chain_entries().get(services.cursor.slot).copied() {
                    self.yanked = Some(entry);
                    services.status = format!("Copied phrase {:02X}", entry.phrase);
                }
            }
            Cmd::Paste => return self.paste(services),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Change a row. A large step is an octave of transpose, or `10` in hex of phrase numbers.
    fn adjust(&self, by: i32, large: bool, services: &mut Services) {
        let (Some(chain), Some(entry)) = (
            services.chain_id(),
            services.chain_entries().get(services.cursor.slot).copied(),
        ) else {
            return;
        };
        let entry = match self.column {
            ChainColumn::Transpose => ChainEntry {
                transpose: (i32::from(entry.transpose) + if large { by * 12 } else { by })
                    .clamp(i32::from(MIN_TRANSPOSE), i32::from(MAX_TRANSPOSE))
                    as i8,
                ..entry
            },
            ChainColumn::Phrase => {
                let Some(phrases) = services.track_here().map(|track| &track.phrases) else {
                    return;
                };
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
        services.apply(&Action::SetChainEntry {
            track: services.cursor.track,
            chain,
            slot: services.cursor.slot,
            entry,
        });
    }

    /// Put this row's transpose back to 0. A row always plays a phrase, so `dd` is for removing it.
    fn reset(&self, services: &mut Services) -> Outcome {
        if self.column != ChainColumn::Transpose {
            return Outcome::Refused("x resets Transpose. A row needs a phrase, so dd removes it");
        }
        if let (Some(chain), Some(entry)) = (
            services.chain_id(),
            services.chain_entries().get(services.cursor.slot).copied(),
        ) && entry.transpose != 0
        {
            services.apply(&Action::SetChainEntry {
                track: services.cursor.track,
                chain,
                slot: services.cursor.slot,
                entry: ChainEntry {
                    transpose: 0,
                    ..entry
                },
            });
        }
        Outcome::Done
    }

    /// `p`: add the row last copied below the cursor's, and go to it.
    fn paste(&self, services: &mut Services) -> Outcome {
        let (Some(chain), Some(entry)) = (services.chain_id(), self.yanked) else {
            return Outcome::Refused("No chain row copied yet");
        };
        let at = (services.cursor.slot + 1).min(services.chain_entries().len());
        services.apply(&Action::InsertChainEntry {
            track: services.cursor.track,
            chain,
            at,
            entry,
        });
        services.cursor.slot = at;
        Outcome::Done
    }
}

impl Page for ChainView {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::Move(Motion::Down), "Next row"),
        ("k", Cmd::Move(Motion::Up), "Previous row"),
        ("g g", Cmd::Move(Motion::First), "First row"),
        ("G", Cmd::Move(Motion::Last), "The empty row at the end"),
        ("h", Cmd::Move(Motion::Left), "Phrase column"),
        ("l", Cmd::Move(Motion::Right), "Transpose column"),
        ("0", Cmd::Move(Motion::RowStart), "Phrase column"),
        ("$", Cmd::Move(Motion::RowEnd), "Transpose column"),
        ("=", Cmd::Increase, "Next phrase, or transpose up"),
        ("-", Cmd::Decrease, "Previous phrase, or transpose down"),
        (
            "+",
            Cmd::IncreaseLarge,
            "Next phrase, or transpose up, a large step",
        ),
        (
            "_",
            Cmd::DecreaseLarge,
            "Previous phrase, or transpose down, a large step",
        ),
        ("C-a", Cmd::Increase, "Next phrase, or transpose up"),
        ("C-x", Cmd::Decrease, "Previous phrase, or transpose down"),
        ("x", Cmd::ClearField, "Transpose back to 0"),
        ("K", Cmd::PickValue, "Choose this row's phrase from a list"),
        ("d d", Cmd::RemoveRow, "Remove this row"),
        ("y y", Cmd::CopyRow, "Copy this row"),
        ("p", Cmd::Paste, "Add the copied row below"),
        (
            ";",
            Cmd::Audition,
            "Audition the start of this row, every track",
        ),
        ("g d", Cmd::OpenItem, "Open this phrase, or add one"),
        ("g r r", Cmd::ListUses, "List where this phrase plays"),
        ("g c", Cmd::ClonePhrase, "Copy this phrase here"),
        ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the view: a move between the columns, an edit of the
    /// row, `yy` or `p`, [`ChainView::key_on`], or passed back, for a move to another row, `dd`,
    /// the phrase picker, Insert, `;`, or the tree.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        self.key_on(cmd, times, ctx.services)
    }

    /// Draw the view.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::pages::chain::draw::draw(frame, area, self, ctx);
    }

    fn column(&self, _ctx: &Ctx) -> String {
        match self.column {
            ChainColumn::Phrase => "Phrase".to_owned(),
            ChainColumn::Transpose => "Transpose".to_owned(),
        }
    }
}

impl App {
    /// List the phrases to choose from for this row, with the one it plays highlighted.
    fn open_phrase_picker(&mut self) {
        if self.chain_view.column != ChainColumn::Phrase {
            self.services.status = "K chooses a phrase on the Phrase column".to_owned();
            return;
        }
        let Some(_) = self.services.chain_id() else {
            self.services.status = "This track has no chain".to_owned();
            return;
        };
        let current = self
            .services
            .chain_entries()
            .get(self.services.cursor.slot)
            .map(|e| e.phrase);
        let phrases: Vec<(u32, usize)> =
            self.services.track_here().map_or_else(Vec::new, |track| {
                track
                    .phrases
                    .iter()
                    .map(|(id, phrase)| (*id, phrase.len()))
                    .collect()
            });
        let position = phrases.iter().position(|(id, _)| Some(*id) == current);
        let items = phrases
            .iter()
            .map(|(id, length)| {
                let shared = if self.services.phrase_uses(*id).len() > 1 {
                    "*"
                } else {
                    ""
                };
                Item::new(format!("{id:02X}{shared}"), Pick::RowPhrase(*id))
                    .meaning(format!("{length} steps"))
            })
            .collect();
        self.show_picker(Picker::new("Phrases", items).opened_on(position));
    }

    /// Play a chosen phrase on this row, or on a new one if the cursor is past the last.
    pub(crate) fn pick_row_phrase(&mut self, phrase: u32) {
        let Some(chain) = self.services.chain_id() else {
            return;
        };
        let slot = self.services.cursor.slot;
        let action = match self.services.chain_entries().get(slot).copied() {
            Some(entry) => Action::SetChainEntry {
                track: self.services.cursor.track,
                chain,
                slot,
                entry: ChainEntry { phrase, ..entry },
            },
            None => Action::InsertChainEntry {
                track: self.services.cursor.track,
                chain,
                at: slot,
                entry: ChainEntry {
                    phrase,
                    transpose: 0,
                },
            },
        };
        self.services.apply(&action);
    }

    /// Run a command the chain view passed back: a move to another row, `dd`, which keeps the
    /// cursor on a row, the phrase picker, Insert or `;`. Returns false if it means the same as
    /// anywhere, or is the tree's.
    pub(crate) fn run_chain_passed(&mut self, cmd: Cmd) -> bool {
        let count = self.modes.count.take();
        let times = count.unwrap_or(1).max(1);
        let rows = self.services.chain_entries().len();
        let chain = self.services.chain_id();
        match cmd {
            Cmd::Move(Motion::Down) => {
                self.move_slot((self.services.cursor.slot + times).min(rows))
            }
            Cmd::Move(Motion::Up) => {
                self.move_slot(self.services.cursor.slot.saturating_sub(times))
            }
            Cmd::Move(Motion::First) => self.move_slot(0),
            Cmd::Move(Motion::Last) => self.move_slot(rows),
            Cmd::Audition if self.services.cursor.slot < rows => {
                let section_step = self.services.section_step(self.modes.view);
                self.audition(section_step);
            }
            Cmd::Audition => {}
            Cmd::PickValue => self.open_phrase_picker(),
            Cmd::RemoveRow => {
                if let (Some(chain), Some(entry)) = (
                    chain,
                    self.services
                        .chain_entries()
                        .get(self.services.cursor.slot)
                        .copied(),
                ) {
                    self.chain_view.yanked = Some(entry);
                    self.services.apply(&Action::RemoveChainEntry {
                        track: self.services.cursor.track,
                        chain,
                        at: self.services.cursor.slot,
                    });
                    self.services.clamp_cursor(self.modes.view);
                }
            }
            Cmd::EnterInsert => {
                if self.chain_view.column == ChainColumn::Transpose
                    && self.services.cursor.slot >= rows
                {
                    self.services.status = "Add a phrase to this row first".to_owned();
                } else {
                    self.modes.entry = Entry::default();
                    self.modes.mode = Mode::Insert;
                }
            }
            _ => {
                self.modes.count = count;
                return false;
            }
        }
        true
    }

    /// Type a phrase ID in hex, or a transpose in semitones, then `Enter`.
    pub(crate) fn insert_chain_row(&mut self, key: KeyCode) {
        let phrase = self.chain_view.column == ChainColumn::Phrase;
        match key {
            KeyCode::Esc => {
                self.modes.entry = Entry::default();
                self.modes.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.modes.entry.text.pop();
            }
            KeyCode::Enter => self.commit_chain_row(),
            KeyCode::Char(c)
                if phrase && c.is_ascii_hexdigit() && self.modes.entry.text.len() < 8 =>
            {
                self.modes.entry.text.push(c.to_ascii_uppercase());
            }
            KeyCode::Char(c)
                if !phrase
                    && self.modes.entry.text.len() < 4
                    && (c.is_ascii_digit()
                        || (self.modes.entry.text.is_empty() && matches!(c, '-' | '+'))) =>
            {
                self.modes.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_chain_row(&mut self) {
        let text = self.modes.entry.text.take();
        self.modes.mode = Mode::Normal;
        let Some(chain) = self.services.chain_id() else {
            return;
        };
        let slot = self.services.cursor.slot;
        let existing = self.services.chain_entries().get(slot).copied();
        let action = match self.chain_view.column {
            ChainColumn::Phrase => {
                let Ok(phrase) = u32::from_str_radix(&text, 16) else {
                    return;
                };
                match existing {
                    Some(entry) => Action::SetChainEntry {
                        track: self.services.cursor.track,
                        chain,
                        slot,
                        entry: ChainEntry { phrase, ..entry },
                    },
                    None => Action::InsertChainEntry {
                        track: self.services.cursor.track,
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
                    self.services.status =
                        format!("Transpose goes from {MIN_TRANSPOSE} to {MAX_TRANSPOSE}");
                    return;
                };
                let Some(entry) = existing else {
                    return;
                };
                Action::SetChainEntry {
                    track: self.services.cursor.track,
                    chain,
                    slot,
                    entry: ChainEntry { transpose, ..entry },
                }
            }
        };
        self.services.apply(&action);
    }
}
