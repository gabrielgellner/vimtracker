//! What keys do in the song view: a row per section, and a column for each track's chain.

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::constants::BAR_STEPS;
use vt_model::selection::Motion;
use vt_model::song::{Repeat, SectionStart, lowest_unused};

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// The columns before the tracks': Length, Rpt and Start.
pub const LENGTH_COLUMN: usize = 0;
pub const REPEAT_COLUMN: usize = 1;
pub const START_COLUMN: usize = 2;
pub const COLUMNS_BEFORE_TRACKS: usize = 3;
/// How far a large step moves a repeat count.
const LARGE_REPEAT: i64 = 4;
/// How far a large step moves a chain ID: its top digit.
const LARGE_CHAIN: i64 = 0x10;

impl App {
    /// The columns shown: Length, Rpt and Start, then each track's that isn't folded away.
    fn song_columns(&self) -> Vec<usize> {
        let tracks = self.folds.shown(self.song());
        (0..COLUMNS_BEFORE_TRACKS)
            .chain(tracks.map(|track| COLUMNS_BEFORE_TRACKS + track))
            .collect()
    }

    /// The column as many shown along from the cursor's as `by`, stepping over folded tracks.
    fn step_song_column(&self, by: isize) -> usize {
        let columns = self.song_columns();
        let at = columns.partition_point(|&column| column < self.song_column);
        let to = at.saturating_add_signed(by).min(columns.len() - 1);
        columns[to]
    }

    fn last_song_column(&self) -> usize {
        self.song_columns()
            .last()
            .copied()
            .unwrap_or(COLUMNS_BEFORE_TRACKS)
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
            Cmd::Move(Motion::Left) => self.song_column = self.step_song_column(-(times as isize)),
            Cmd::Move(Motion::Right) => self.song_column = self.step_song_column(times as isize),
            Cmd::Move(Motion::RowStart) => self.song_column = 0,
            Cmd::Move(Motion::RowEnd) => self.song_column = self.last_song_column(),
            Cmd::Increase => self.adjust_song(times as i64, false),
            Cmd::Decrease => self.adjust_song(-(times as i64), false),
            Cmd::IncreaseLarge => self.adjust_song(times as i64, true),
            Cmd::DecreaseLarge => self.adjust_song(-(times as i64), true),
            Cmd::ClearField => self.clear_song_field(),
            Cmd::PickValue if self.song_column == START_COLUMN => self.switch_start(),
            Cmd::PickValue => self.open_chain_picker(),
            Cmd::EnterInsert if self.song_column == START_COLUMN => self.switch_start(),
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
            Cmd::CloneChain => self.clone_section_chain(),
            Cmd::Launch => self.launch(),
            Cmd::Capture => self.capture(),
            Cmd::RemoveRow => {
                self.apply(&Action::RemoveSection { at: section });
                self.go_to_section(section);
            }
            Cmd::Audition => self.audition_section(section, 0),
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
            None if self.song_column == LENGTH_COLUMN => {
                let size = if large { BAR_STEPS as i64 } else { 1 };
                let length = self.song().section_length(&here) as i64 + by * size;
                self.apply(&Action::SetSectionLength {
                    section,
                    length: Some(length.max(1) as usize),
                });
            }
            None if self.song_column == REPEAT_COLUMN => {
                let size = if large { LARGE_REPEAT } else { 1 };
                self.apply(&Action::SetSectionRepeat {
                    section,
                    repeat: Repeat::new(i64::from(here.repeat.get()) + by * size),
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
            SectionStart::New => SectionStart::Reset,
            SectionStart::Reset => SectionStart::New,
        };
        self.apply(&Action::SetSectionStart {
            section: self.cursor.section,
            start,
        });
    }

    /// Back to following the longest chain, playing once, new, or no chain. Nothing changes if it's
    /// there.
    fn clear_song_field(&mut self) {
        let section = self.cursor.section;
        let Some(here) = self.section_here().cloned() else {
            return;
        };
        match self.song_track() {
            None if self.song_column == LENGTH_COLUMN => {
                if here.length.is_some() {
                    self.apply(&Action::SetSectionLength {
                        section,
                        length: None,
                    });
                }
            }
            None if self.song_column == REPEAT_COLUMN => {
                if here.repeat != Repeat::default() {
                    self.apply(&Action::SetSectionRepeat {
                        section,
                        repeat: Repeat::default(),
                    });
                }
            }
            None => {
                if here.start != SectionStart::New {
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

    /// Type a length or a repeat count in decimal, ending with `Enter`, or a chain ID's two hex
    /// digits.
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
                if self.song_column == LENGTH_COLUMN {
                    match text.parse() {
                        Ok(length) if length > 0 => self.apply(&Action::SetSectionLength {
                            section: self.cursor.section,
                            length: Some(length),
                        }),
                        _ => self.status = "A length is a number of steps, 1 or more".to_owned(),
                    }
                } else if self.song_column == REPEAT_COLUMN {
                    match text.parse::<u16>() {
                        Ok(times) if times > 0 => self.apply(&Action::SetSectionRepeat {
                            section: self.cursor.section,
                            repeat: Repeat::new(i64::from(times)),
                        }),
                        _ => self.status = "A repeat is a number of times, 1 or more".to_owned(),
                    }
                }
            }
            KeyCode::Char(c) if self.song_column < START_COLUMN && c.is_ascii_digit() => {
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

    /// The chain under the cursor, on a track's column.
    fn cell_chain(&self, track: usize) -> Option<u32> {
        self.section_here()?.chains.get(track).copied().flatten()
    }

    /// The label of the chain under the cursor, or nothing.
    pub fn song_label(&self) -> &str {
        self.song_track()
            .and_then(|track| {
                let chain = self.cell_chain(track)?;
                self.song().tracks.get(track)?.chains.get(&chain)
            })
            .map_or("", |chain| chain.label.as_str())
    }

    /// `K`: list this track's chains to choose from, with the one in this cell highlighted.
    fn open_chain_picker(&mut self) {
        let Some(track) = self.song_track() else {
            self.status = "K chooses a chain from a track's column".to_owned();
            return;
        };
        let current = self.cell_chain(track);
        let mut chains = vec![None];
        chains.extend(self.song().tracks[track].chains.keys().copied().map(Some));
        let position = chains.iter().position(|chain| *chain == current);
        let items = chains
            .iter()
            .map(|chain| {
                let (label, meaning) = chain.map_or_else(
                    || ("--".to_owned(), String::new()),
                    |id| {
                        let label = &self.song().tracks[track].chains[&id].label;
                        (format!("{id:02X}"), label.clone())
                    },
                );
                Item::new(label, Pick::SectionChain(*chain)).meaning(meaning)
            })
            .collect();
        self.show_picker(Picker::new("Chains", items).opened_on(position));
    }

    /// Put a chosen chain in the cell under the cursor.
    pub(crate) fn pick_section_chain(&mut self, chain: Option<u32>) {
        let Some(track) = self.song_track() else {
            return;
        };
        if self.cell_chain(track) != chain {
            self.apply(&Action::SetSectionChain {
                section: self.cursor.section,
                track,
                chain,
            });
        }
    }

    /// `gC`: copy the chain under the cursor and its phrases, for this cell alone.
    fn clone_section_chain(&mut self) {
        let Some(track) = self.song_track() else {
            self.status = "gC copies a chain from a track's column".to_owned();
            return;
        };
        self.cursor.track = track;
        self.clone_chain();
    }

    /// `:label`: name the chain under the cursor, in the song and chain views.
    pub(crate) fn label_chain(&mut self, label: Option<String>) {
        let track = match self.view {
            View::Song => self.song_track(),
            View::Chain => Some(self.cursor.track),
            _ => {
                self.status = ":label works in the song and chain views".to_owned();
                return;
            }
        };
        let Some(track) = track else {
            self.status = ":label names a chain from a track's column".to_owned();
            return;
        };
        let Some(chain) = self.cell_chain(track) else {
            self.status = "This track has no chain".to_owned();
            return;
        };
        self.apply(&Action::SetChainLabel {
            track,
            chain,
            label: label.unwrap_or_default(),
        });
    }

    /// `gd`: open the phrase of a chain with just one, or the rows of one with several. A cell with
    /// no chain gets a new one, and a chain with no phrase gets a new one.
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
        if self.chain_entries().len() > 1 {
            self.view = View::Chain;
            self.note_view(View::Chain);
        } else if let Some(chain) = self.chain_id() {
            self.enter_row(chain, 0);
        }
    }

    /// `Enter`: play the chain under the cursor on its track when its launch timing next allows, or
    /// stop the track then on a `--`. The song stays on this section, so the jam doesn't move on.
    fn launch(&mut self) {
        if self.transport != Transport::Playing {
            self.status = "Enter launches a chain while playing".to_owned();
            return;
        }
        let Some(track) = self.song_track() else {
            self.status = "Enter launches from a track's column".to_owned();
            return;
        };
        let chain = self
            .section_here()
            .and_then(|here| here.chains.get(track).copied().flatten());
        self.handle.send(Command::Launch { track, chain });
        if self.looping.is_none() {
            self.looping = Some(self.playing_section);
            self.handle.send(Command::LoopSection(self.looping));
        }
        let when = self.song().launch.when();
        self.status = match chain {
            Some(chain) => format!(
                "Track {:02} plays chain {chain:02X} {when}. Staying on section {:02}",
                track + 1,
                self.playing_section + 1
            ),
            None => format!(
                "Track {:02} stops {when}. Staying on section {:02}",
                track + 1,
                self.playing_section + 1
            ),
        };
    }

    /// `C`: add a section below the one playing, holding what each track is playing, and carry on
    /// in it without a break.
    fn capture(&mut self) {
        if self.transport != Transport::Playing {
            self.status = "C captures what's playing, so start the song first".to_owned();
            return;
        }
        let song = self.song();
        let Some(playing) = song.sections.get(self.playing_section) else {
            return;
        };
        let chains: Vec<Option<u32>> = (0..song.tracks.len())
            .map(|track| {
                let row = playing.chains.get(track).copied().flatten();
                self.jam
                    .get(track)
                    .and_then(|jam| jam.playing)
                    .unwrap_or(row)
            })
            .collect();
        if chains.iter().all(Option::is_none) {
            self.status = "Nothing is playing to capture".to_owned();
            return;
        }
        let at = self.playing_section + 1;
        // Playing on with the source's settings keeps its timing and realigning as they were.
        let length = song.section_length(playing);
        let (start, reset) = (playing.start, playing.reset);
        let mut actions = vec![
            Action::AddSection { at },
            Action::SetSectionStart { section: at, start },
            Action::SetReset { section: at, reset },
        ];
        if length > 0 {
            actions.push(Action::SetSectionLength {
                section: at,
                length: Some(length),
            });
        }
        for (track, chain) in chains.into_iter().enumerate() {
            actions.push(Action::SetSectionChain {
                section: at,
                track,
                chain,
            });
        }
        self.apply(&Action::Batch(actions));
        self.handle.send(Command::ContinueIn {
            section: at,
            length,
        });
        self.looping = Some(at);
        self.go_to_section(at);
        self.status = format!("Captured to section {:02}, and staying on it", at + 1);
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
