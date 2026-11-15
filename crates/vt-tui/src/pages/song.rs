//! What keys do in the song view: a row per section, and a column for each track's chain.
//!
//! `SongView` is the view as a component (#320): the column its cursor is on and the sections
//! `V` selects, its keys, and its drawing, in `draw`, given a [`Ctx`]. It's a [`Page`], with its
//! rows of the binding table, [`Page::KEYS`], here beside it. Its keys are all its own: the moves
//! along a row and to other sections, the edits, `a`, `gc`, `dd` and `C`, `Enter`'s launch, `V`,
//! `;`, `gC`, Insert's typing, and `K`'s picker. What only `App` can do they ask for in their
//! [`Outcome`]: the picker, Insert, an audition held while the key is down, and the phrase view
//! starting afresh on another section (#784). The tree's `gd`, and `:loop` and `:label`, which
//! read the song view's cursor, stay `App`'s.

pub mod draw;

use std::sync::Arc;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use vt_engine::engine::{SectionLoop, Transport};
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::constants::BAR_STEPS;
use vt_model::selection::Motion;
use vt_model::song::{Repeat, SectionStart, lowest_unused};

use crate::app::{App, Entry, Momentary};
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::input::typed::{HEX, Typed};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::{Ctx, CtxMut, Outcome, Page};
use crate::services::Services;

/// The columns before the tracks': Length, Rpt and Start.
pub(crate) const LENGTH_COLUMN: usize = 0;
pub(crate) const REPEAT_COLUMN: usize = 1;
pub(crate) const START_COLUMN: usize = 2;
pub(crate) const COLUMNS_BEFORE_TRACKS: usize = 3;
/// How far a large step moves a repeat count.
const LARGE_REPEAT: i64 = 4;
/// How far a large step moves a chain ID: its top digit.
const LARGE_CHAIN: i64 = 0x10;

/// The song view as a component: the column its cursor is on, kept while other views are shown,
/// and where a selection of sections started. Which section it's on is the program's cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SongView {
    /// Length, Rpt and Start, then a column for each track, by its index after them.
    pub column: usize,
    /// Where a selection of sections started, while `V` selects them.
    pub anchor: Option<usize>,
}

impl SongView {
    /// The columns shown: Length, Rpt and Start, then each track's that isn't folded away.
    fn columns(services: &Services) -> Vec<usize> {
        let tracks = services.folds.shown(services.song());
        (0..COLUMNS_BEFORE_TRACKS)
            .chain(tracks.map(|track| COLUMNS_BEFORE_TRACKS + track))
            .collect()
    }

    /// The column as many shown along from the cursor's as `by`, stepping over folded tracks.
    fn step_column(&self, by: isize, services: &Services) -> usize {
        let columns = Self::columns(services);
        let at = columns.partition_point(|&column| column < self.column);
        let to = at.saturating_add_signed(by).min(columns.len() - 1);
        columns[to]
    }

    fn last_column(services: &Services) -> usize {
        Self::columns(services)
            .last()
            .copied()
            .unwrap_or(COLUMNS_BEFORE_TRACKS)
    }

    /// The track the cursor's column belongs to, if it's a track's.
    pub fn track(&self) -> Option<usize> {
        self.column.checked_sub(COLUMNS_BEFORE_TRACKS)
    }

    /// The sections selected with `V`, from where it started to the cursor's `section`, if they
    /// are and the view on screen is this one.
    pub fn selected(&self, view: View, section: usize) -> Option<SectionLoop> {
        let anchor = self.anchor?;
        (view == View::Song).then(|| SectionLoop::new(anchor, section))
    }

    /// Run one of the view's own keys `times` times, or pass it back.
    fn key_on(&mut self, cmd: Cmd, times: usize, services: &mut Services) -> Outcome {
        let section = services.cursor.section;
        match cmd {
            Cmd::Move(Motion::Down) => {
                return go_to_section(section.saturating_add(times), services);
            }
            Cmd::Move(Motion::Up) => return go_to_section(section.saturating_sub(times), services),
            Cmd::Move(Motion::First) => return go_to_section(0, services),
            Cmd::Move(Motion::Last) => return go_to_section(usize::MAX, services),
            Cmd::Move(Motion::Left) => self.column = self.step_column(-(times as isize), services),
            Cmd::Move(Motion::Right) => self.column = self.step_column(times as isize, services),
            Cmd::Move(Motion::RowStart) => self.column = 0,
            Cmd::Move(Motion::RowEnd) => self.column = Self::last_column(services),
            Cmd::Increase => self.adjust(times as i64, false, services),
            Cmd::Decrease => self.adjust(-(times as i64), false, services),
            Cmd::IncreaseLarge => self.adjust(times as i64, true, services),
            Cmd::DecreaseLarge => self.adjust(-(times as i64), true, services),
            Cmd::ClearField => self.clear(services),
            Cmd::PickValue | Cmd::EnterInsert if self.column == START_COLUMN => {
                Self::switch_start(services);
            }
            Cmd::Launch => return self.launch(services),
            Cmd::VisualStep => {
                self.anchor = match self.anchor {
                    Some(_) => None,
                    None => Some(services.cursor.section),
                };
            }
            Cmd::PickValue => return self.chain_picker(services),
            Cmd::EnterInsert => return Outcome::Insert(Entry::default()),
            Cmd::AddSection => {
                services.apply(&Action::AddSection { at: section + 1 });
                return go_to_section(section + 1, services);
            }
            Cmd::CloneSection => {
                services.apply(&Action::CloneSection { at: section });
                return go_to_section(section + 1, services);
            }
            Cmd::CloneChain => self.clone_chain(services),
            Cmd::Capture => return capture(services),
            Cmd::RemoveRow => {
                services.apply(&Action::RemoveSection { at: section });
                return go_to_section(section, services);
            }
            Cmd::Audition => {
                services.handle.send(Command::Audition {
                    section,
                    section_step: 0,
                    track: None,
                });
                return Outcome::Hold(Momentary::Audition);
            }
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// The chain under the cursor, on a track's column.
    fn cell_chain(track: usize, services: &Services) -> Option<u32> {
        services
            .section_here()?
            .chains
            .get(track)
            .copied()
            .flatten()
    }

    /// `K`: list this track's chains to choose from, with the one in this cell highlighted.
    fn chain_picker(&self, services: &mut Services) -> Outcome {
        let Some(track) = self.track() else {
            services.status = "K chooses a chain from a track's column".to_owned();
            return Outcome::Done;
        };
        let current = Self::cell_chain(track, services);
        let chains_of = &services.song().tracks[track].chains;
        let mut chains = vec![None];
        chains.extend(chains_of.keys().copied().map(Some));
        let position = chains.iter().position(|chain| *chain == current);
        let items = chains
            .iter()
            .map(|chain| {
                let (label, meaning) = chain.map_or_else(
                    || ("--".to_owned(), String::new()),
                    |id| (format!("{id:02X}"), chains_of[&id].label.clone()),
                );
                Item::new(label, Pick::SectionChain(*chain)).meaning(meaning)
            })
            .collect();
        Outcome::Open(Box::new(Picker::new("Chains", items).opened_on(position)))
    }

    /// Put a chosen chain in the cell under the cursor.
    pub(crate) fn pick_chain(&self, chain: Option<u32>, services: &mut Services) {
        let Some(track) = self.track() else {
            return;
        };
        services.apply(&Action::SetSectionChain {
            section: services.cursor.section,
            track,
            chain,
        });
    }

    /// `gC`: copy the chain under the cursor and its phrases, for this cell alone.
    fn clone_chain(&self, services: &mut Services) {
        let Some(track) = self.track() else {
            services.status = "gC copies a chain from a track's column".to_owned();
            return;
        };
        services.cursor.track = track;
        services.clone_chain();
    }

    /// Type a length or a repeat count in decimal, ending with `Enter`, or a chain ID's two hex
    /// digits, into what Insert has typed. Returns whether Insert goes on.
    pub(crate) fn insert(&self, key: KeyCode, entry: &mut Entry, services: &mut Services) -> bool {
        let section = services.cursor.section;
        match key {
            KeyCode::Esc => {
                *entry = Entry::default();
                return false;
            }
            KeyCode::Backspace => {
                entry.text.pop();
            }
            KeyCode::Enter => {
                let text = entry.text.take();
                if self.column == LENGTH_COLUMN {
                    match text.parse() {
                        Ok(length) if length > 0 => services.apply(&Action::SetSectionLength {
                            section,
                            length: Some(length),
                        }),
                        _ => {
                            services.status = "A length is a number of steps, 1 or more".to_owned()
                        }
                    }
                } else if self.column == REPEAT_COLUMN {
                    match text.parse::<u16>() {
                        Ok(times) if times > 0 => services.apply(&Action::SetSectionRepeat {
                            section,
                            repeat: Repeat::new(i64::from(times)),
                        }),
                        _ => {
                            services.status = "A repeat is a number of times, 1 or more".to_owned()
                        }
                    }
                }
                return false;
            }
            KeyCode::Char(c) if self.column < START_COLUMN && c.is_ascii_digit() => {
                entry.text.push(c);
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                if let Typed::Commit(text) = entry.text.key(key, HEX) {
                    if let (Some(track), Ok(chain)) = (self.track(), u32::from_str_radix(&text, 16))
                    {
                        services.apply(&Action::SetSectionChain {
                            section,
                            track,
                            chain: Some(chain),
                        });
                    }
                    return false;
                }
            }
            _ => {}
        }
        true
    }

    fn adjust(&self, by: i64, large: bool, services: &mut Services) {
        let section = services.cursor.section;
        let Some(here) = services.section_here().cloned() else {
            return;
        };
        match self.track() {
            None if self.column == LENGTH_COLUMN => {
                let size = if large { BAR_STEPS as i64 } else { 1 };
                let length = services.song().section_length(&here) as i64 + by * size;
                services.apply(&Action::SetSectionLength {
                    section,
                    length: Some(length.max(1) as usize),
                });
            }
            None if self.column == REPEAT_COLUMN => {
                let size = if large { LARGE_REPEAT } else { 1 };
                services.apply(&Action::SetSectionRepeat {
                    section,
                    repeat: Repeat::new(i64::from(here.repeat.get()) + by * size),
                });
            }
            None => Self::switch_start(services),
            Some(track) => {
                let size = if large { LARGE_CHAIN } else { 1 };
                let chain = match here.chains.get(track).copied().flatten() {
                    Some(id) => (i64::from(id) + by * size).max(0) as u32,
                    None => lowest_unused(&services.song().tracks[track].chains),
                };
                services.apply(&Action::SetSectionChain {
                    section,
                    track,
                    chain: Some(chain),
                });
            }
        }
    }

    fn switch_start(services: &mut Services) {
        let Some(here) = services.section_here() else {
            return;
        };
        let start = match here.start {
            SectionStart::New => SectionStart::Reset,
            SectionStart::Reset => SectionStart::New,
        };
        services.apply(&Action::SetSectionStart {
            section: services.cursor.section,
            start,
        });
    }

    /// Back to following the longest chain, playing once, new, or no chain. Nothing changes if it's
    /// there.
    fn clear(&self, services: &mut Services) {
        let section = services.cursor.section;
        let Some(here) = services.section_here().cloned() else {
            return;
        };
        match self.track() {
            None if self.column == LENGTH_COLUMN => services.apply(&Action::SetSectionLength {
                section,
                length: None,
            }),
            None if self.column == REPEAT_COLUMN => services.apply(&Action::SetSectionRepeat {
                section,
                repeat: Repeat::default(),
            }),
            None => {
                if here.start != SectionStart::New {
                    Self::switch_start(services);
                }
            }
            Some(track) => services.apply(&Action::SetSectionChain {
                section,
                track,
                chain: None,
            }),
        }
    }

    /// `Enter`: play the chain under the cursor on its track when its launch timing next allows, or
    /// stop the track then on a `--`. The song stays on this section, so the jam doesn't move on.
    fn launch(&self, services: &mut Services) -> Outcome {
        if services.transport != Transport::Playing {
            return Outcome::Refused("Enter launches a chain while playing");
        }
        let Some(track) = self.track() else {
            return Outcome::Refused("Enter launches from a track's column");
        };
        let chain = services
            .section_here()
            .and_then(|here| here.chains.get(track).copied().flatten());
        services.handle.send(Command::Launch { track, chain });
        if services.looping.is_none() {
            services.looping = Some(SectionLoop::one(services.playing_section));
            services.handle.send(Command::Loop(services.looping));
        }
        let when = services.song().launch.when();
        services.status = match chain {
            Some(chain) => format!(
                "Track {:02} plays chain {chain:02X} {when}. Staying on section {:02}",
                track + 1,
                services.playing_section + 1
            ),
            None => format!(
                "Track {:02} stops {when}. Staying on section {:02}",
                track + 1,
                services.playing_section + 1
            ),
        };
        Outcome::Done
    }
}

impl Page for SongView {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::Move(Motion::Down), "Next section"),
        ("k", Cmd::Move(Motion::Up), "Previous section"),
        ("g g", Cmd::Move(Motion::First), "First section"),
        ("G", Cmd::Move(Motion::Last), "Last section"),
        ("h", Cmd::Move(Motion::Left), "Previous column"),
        ("l", Cmd::Move(Motion::Right), "Next column"),
        ("0", Cmd::Move(Motion::RowStart), "Length column"),
        ("$", Cmd::Move(Motion::RowEnd), "Last track's column"),
        ("=", Cmd::Increase, "Increase the value"),
        ("-", Cmd::Decrease, "Decrease the value"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        ("C-a", Cmd::Increase, "Increase the value"),
        ("C-x", Cmd::Decrease, "Decrease the value"),
        (
            "x",
            Cmd::ClearField,
            "Follow the longest chain, new, or no chain",
        ),
        (
            "K",
            Cmd::PickValue,
            "Switch new and reset, or choose this cell's chain",
        ),
        (
            "g d",
            Cmd::OpenItem,
            "Open this chain's phrase, its rows, or add one",
        ),
        ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
        ("a", Cmd::AddSection, "Add an empty section after this one"),
        ("g c", Cmd::CloneSection, "Copy this section after itself"),
        (
            "Enter",
            Cmd::Launch,
            "Play this chain on its track next, or stop on --",
        ),
        (
            "C",
            Cmd::Capture,
            "Add a section of what's playing, and stay on it",
        ),
        ("d d", Cmd::RemoveRow, "Remove this section"),
        (";", Cmd::Audition, "Hear the start of this section"),
        (
            "V",
            Cmd::VisualStep,
            "Select sections, for :loop; V or Esc ends it",
        ),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the view, [`SongView::key_on`], or pass it back, for
    /// the tree's `gd` and what means the same on every page.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        self.key_on(cmd, times, ctx.services)
    }

    /// Draw the view.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::pages::song::draw::draw(frame, area, self, ctx);
    }

    fn column(&self, ctx: &Ctx) -> String {
        match self.column {
            LENGTH_COLUMN => "Length".to_owned(),
            REPEAT_COLUMN => "Rpt".to_owned(),
            START_COLUMN => "Start".to_owned(),
            column => {
                let index = column - COLUMNS_BEFORE_TRACKS;
                let name = ctx.song().tracks.get(index).map_or_else(
                    || format!("Track {:02}", index + 1),
                    |track| track.title(index),
                );
                let label = self.label(ctx);
                if label.is_empty() {
                    format!("{name} chain")
                } else {
                    format!("{name} chain: {label}")
                }
            }
        }
    }
}

impl SongView {
    /// The label of the chain under the cursor, or nothing.
    fn label<'a>(&self, ctx: &Ctx<'a>) -> &'a str {
        self.track()
            .and_then(|track| {
                let chain = ctx.section_here()?.chains.get(track).copied().flatten()?;
                ctx.song().tracks.get(track)?.chains.get(&chain)
            })
            .map_or("", |chain| chain.label.as_str())
    }
}

/// Edit another section, at its start, so the phrase view starts afresh there.
fn go_to_section(section: usize, services: &mut Services) -> Outcome {
    let last = services.song().sections.len() - 1;
    services.cursor.section = section.min(last);
    services.cursor.slot = 0;
    services.go_to_loop_start();
    Outcome::Moved
}

/// `C`: add a section below the one playing, holding what each track is playing, and carry on
/// in it without a break.
fn capture(services: &mut Services) -> Outcome {
    if services.transport != Transport::Playing {
        return Outcome::Refused("C captures what's playing, so start the song first");
    }
    let song = Arc::clone(services.song());
    let Some(playing) = song.sections.get(services.playing_section) else {
        return Outcome::Done;
    };
    let chains: Vec<Option<u32>> = (0..song.tracks.len())
        .map(|track| {
            let row = playing.chains.get(track).copied().flatten();
            services
                .jam
                .get(track)
                .and_then(|jam| jam.playing)
                .unwrap_or(row)
        })
        .collect();
    if chains.iter().all(Option::is_none) {
        return Outcome::Refused("Nothing is playing to capture");
    }
    let at = services.playing_section + 1;
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
    services.apply(&Action::Batch(actions));
    services.handle.send(Command::ContinueIn {
        section: at,
        length,
    });
    services.looping = Some(SectionLoop::one(at));
    let moved = go_to_section(at, services);
    services.status = format!("Captured to section {:02}, and staying on it", at + 1);
    moved
}

impl App {
    /// `:label`: name the chain under the cursor, in the song and chain views.
    pub(crate) fn label_chain(&mut self, label: Option<String>) {
        let track = match self.modes.view {
            View::Song => self.song_view.track(),
            View::Chain => Some(self.services.cursor.track),
            _ => {
                self.services.status = ":label works in the song and chain views".to_owned();
                return;
            }
        };
        let Some(track) = track else {
            self.services.status = ":label names a chain from a track's column".to_owned();
            return;
        };
        let Some(chain) = SongView::cell_chain(track, &self.services) else {
            self.services.status = "This track has no chain".to_owned();
            return;
        };
        self.services.apply(&Action::SetChainLabel {
            track,
            chain,
            label: label.unwrap_or_default(),
        });
    }

    /// `gd`: open the phrase of a chain with just one, or the rows of one with several. A cell with
    /// no chain gets a new one, and a chain with no phrase gets a new one.
    pub(crate) fn open_section_chain(&mut self) {
        let Some(track) = self.song_view.track() else {
            self.services.status = "gd opens a chain from a track's column".to_owned();
            return;
        };
        let section = self.services.cursor.section;
        let chain = self
            .services
            .section_here()
            .and_then(|here| here.chains.get(track).copied().flatten());
        self.record_jump();
        if chain.is_none() {
            let chain = lowest_unused(&self.song().tracks[track].chains);
            self.services.apply(&Action::SetSectionChain {
                section,
                track,
                chain: Some(chain),
            });
        }
        self.services.cursor.track = track;
        self.services.cursor.slot = 0;
        if self.services.chain_entries().len() > 1 {
            self.modes.view = View::Chain;
            self.note_view(View::Chain);
        } else if let Some(chain) = self.services.chain_id() {
            self.enter_row(chain, 0);
        }
    }

    /// `:loop`: keep playing the sections selected with `V`, or the section being edited, or go
    /// back to playing the song if that one is already looping. While playing, a loop starts on
    /// the next line of the launch grid, with its first section if the one playing isn't in it.
    pub(crate) fn toggle_loop(&mut self) {
        let section = self.services.cursor.section;
        let selected = self
            .song_view
            .selected(self.modes.view, self.services.cursor.section);
        self.services.looping = match (selected, self.services.looping) {
            (Some(selected), _) => Some(selected),
            (None, Some(looping)) if looping.holds(section) => None,
            (None, _) => Some(SectionLoop::one(section)),
        };
        self.services
            .handle
            .send(Command::Loop(self.services.looping));
        let Some(looping) = self.services.looping else {
            self.services.status = "Playing the song".to_owned();
            return;
        };
        let sections = if looping.first == looping.last {
            format!("section {:02}", looping.first + 1)
        } else {
            format!("sections {:02}-{:02}", looping.first + 1, looping.last + 1)
        };
        self.services.status = if self.services.transport == Transport::Playing {
            let when = self.grid_when();
            format!("Looping {sections} {when}")
        } else {
            format!("Looping {sections}")
        };
    }
}
