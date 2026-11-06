//! What keys do in the song view: a row per section, and a column for each track's chain.

use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::selection::Motion;
use vt_model::song::{SectionStart, lowest_unused};

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};

/// The columns before the tracks': Length and Start.
pub const COLUMNS_BEFORE_TRACKS: usize = 2;
/// How far a large step moves a length: a bar.
const LARGE_LENGTH: i64 = 16;
/// How far a large step moves a chain ID: its top digit.
const LARGE_CHAIN: i64 = 0x10;

impl App {
    fn last_song_column(&self) -> usize {
        COLUMNS_BEFORE_TRACKS + self.song().tracks.len() - 1
    }

    /// The track the cursor's column belongs to, if it's a track's.
    fn song_track(&self) -> Option<usize> {
        self.song_column.checked_sub(COLUMNS_BEFORE_TRACKS)
    }

    /// Edit another section, at its start.
    fn go_to_section(&mut self, section: usize) {
        let last = self.song().sections.len() - 1;
        self.cursor.section = section.min(last);
        self.cursor.slot = 0;
        self.go_to_loop_start();
    }

    /// Run a command in the song view.
    pub(crate) fn run_song(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let section = self.cursor.section;
        match cmd {
            Cmd::Move(Motion::Down) => self.go_to_section(section.saturating_add(times)),
            Cmd::Move(Motion::Up) => self.go_to_section(section.saturating_sub(times)),
            Cmd::Move(Motion::First) => self.go_to_section(0),
            Cmd::Move(Motion::Last) => self.go_to_section(usize::MAX),
            Cmd::Move(Motion::Left) => self.song_column = self.song_column.saturating_sub(times),
            Cmd::Move(Motion::Right) => {
                self.song_column = (self.song_column + times).min(self.last_song_column());
            }
            Cmd::Move(Motion::RowStart) => self.song_column = 0,
            Cmd::Move(Motion::RowEnd) => self.song_column = self.last_song_column(),
            Cmd::Increase => self.adjust_song(times as i64, false),
            Cmd::Decrease => self.adjust_song(-(times as i64), false),
            Cmd::IncreaseLarge => self.adjust_song(times as i64, true),
            Cmd::DecreaseLarge => self.adjust_song(-(times as i64), true),
            Cmd::ClearField => self.clear_song_field(),
            Cmd::PickValue if self.song_column == 1 => self.switch_start(),
            Cmd::PickValue => self.status = "K switches carry and reset on Start".to_owned(),
            Cmd::EnterInsert if self.song_column == 1 => self.switch_start(),
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
            Cmd::AddSection => {
                self.apply(&Action::AddSection { at: section + 1 });
                self.go_to_section(section + 1);
            }
            Cmd::CloneSection => {
                self.apply(&Action::CloneSection { at: section });
                self.go_to_section(section + 1);
            }
            Cmd::RemoveRow => {
                self.apply(&Action::RemoveSection { at: section });
                self.go_to_section(section);
            }
            Cmd::Audition => {
                self.handle.send(Command::Audition {
                    section,
                    section_step: 0,
                });
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    fn adjust_song(&mut self, by: i64, large: bool) {
        let section = self.cursor.section;
        let Some(here) = self.section_here().cloned() else {
            return;
        };
        match self.song_track() {
            None if self.song_column == 0 => {
                let size = if large { LARGE_LENGTH } else { 1 };
                let length = self.song().section_length(&here) as i64 + by * size;
                self.apply(&Action::SetSectionLength {
                    section,
                    length: Some(length.max(1) as usize),
                });
            }
            None => self.switch_start(),
            Some(track) => {
                let size = if large { LARGE_CHAIN } else { 1 };
                let chain = match here.chains.get(track).copied().flatten() {
                    Some(id) => (i64::from(id) + by * size).max(0) as u32,
                    None => lowest_unused(&self.song().tracks[track].chains),
                };
                self.apply(&Action::SetSectionChain {
                    section,
                    track,
                    chain: Some(chain),
                });
            }
        }
    }

    fn switch_start(&mut self) {
        let Some(here) = self.section_here() else {
            return;
        };
        let start = match here.start {
            SectionStart::Carry => SectionStart::Reset,
            SectionStart::Reset => SectionStart::Carry,
        };
        self.apply(&Action::SetSectionStart {
            section: self.cursor.section,
            start,
        });
    }

    /// Back to following the longest chain, carrying, or no chain. Nothing changes if it's there.
    fn clear_song_field(&mut self) {
        let section = self.cursor.section;
        let Some(here) = self.section_here().cloned() else {
            return;
        };
        match self.song_track() {
            None if self.song_column == 0 => {
                if here.length.is_some() {
                    self.apply(&Action::SetSectionLength {
                        section,
                        length: None,
                    });
                }
            }
            None => {
                if here.start != SectionStart::Carry {
                    self.switch_start();
                }
            }
            Some(track) => {
                if here.chains.get(track).copied().flatten().is_some() {
                    self.apply(&Action::SetSectionChain {
                        section,
                        track,
                        chain: None,
                    });
                }
            }
        }
    }

    /// Type a length in decimal, ending with `Enter`, or a chain ID's two hex digits.
    pub(crate) fn insert_song(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                if self.song_column == 0 {
                    match text.parse() {
                        Ok(length) if length > 0 => self.apply(&Action::SetSectionLength {
                            section: self.cursor.section,
                            length: Some(length),
                        }),
                        _ => self.status = "A length is a number of steps, 1 or more".to_owned(),
                    }
                }
            }
            KeyCode::Char(c) if self.song_column == 0 && c.is_ascii_digit() => {
                self.entry.text.push(c);
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    self.mode = Mode::Normal;
                    if let (Some(track), Ok(chain)) =
                        (self.song_track(), u32::from_str_radix(&text, 16))
                    {
                        self.apply(&Action::SetSectionChain {
                            section: self.cursor.section,
                            track,
                            chain: Some(chain),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// `gd`: open the chain under the cursor, adding one if it has none.
    pub(crate) fn open_section_chain(&mut self) {
        let Some(track) = self.song_track() else {
            self.status = "gd opens a chain from a track's column".to_owned();
            return;
        };
        let section = self.cursor.section;
        let chain = self
            .section_here()
            .and_then(|here| here.chains.get(track).copied().flatten());
        self.record_jump();
        if chain.is_none() {
            let chain = lowest_unused(&self.song().tracks[track].chains);
            self.apply(&Action::SetSectionChain {
                section,
                track,
                chain: Some(chain),
            });
        }
        self.cursor.track = track;
        self.cursor.slot = 0;
        self.view = View::Chain;
        self.note_view(View::Chain);
    }

    /// `:loop`: keep playing the section being edited, or go back to playing the song.
    pub(crate) fn toggle_loop(&mut self) {
        let section = self.cursor.section;
        self.looping = if self.looping == Some(section) {
            None
        } else {
            Some(section)
        };
        self.handle.send(Command::LoopSection(self.looping));
        self.status = match self.looping {
            Some(section) => format!("Looping section {:02}", section + 1),
            None => "Playing the song".to_owned(),
        };
    }
}
