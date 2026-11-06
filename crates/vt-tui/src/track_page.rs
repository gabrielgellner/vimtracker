//! What keys do on the track page.

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, TrackControl};
use vt_model::constants::{MAX_SWING, MIN_SWING};
use vt_model::lfo::{Destination, LFOS_PER_TRACK, Lfo, MAX_LFO_DEPTH, Rate, Shape};
use vt_model::selection::Motion;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::command::swing;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker};

/// How far `+` and `-` move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;

/// One setting of an LFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoField {
    Destination,
    Shape,
    Rate,
    Depth,
    Phase,
    Retrig,
}

/// One setting on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Base(TrackControl),
    Level,
    Swing,
    Lfo(usize, LfoField),
}

const fn lfo_row(slot: usize) -> [Control; 6] {
    [
        Control::Lfo(slot, LfoField::Destination),
        Control::Lfo(slot, LfoField::Shape),
        Control::Lfo(slot, LfoField::Rate),
        Control::Lfo(slot, LfoField::Depth),
        Control::Lfo(slot, LfoField::Phase),
        Control::Lfo(slot, LfoField::Retrig),
    ]
}

/// The filter's row.
pub const FILTER: [Control; 2] = [
    Control::Base(TrackControl::Cutoff),
    Control::Base(TrackControl::Resonance),
];
/// The output's row.
pub const OUTPUT: [Control; 5] = [
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::Delay),
    Control::Base(TrackControl::Reverb),
    Control::Swing,
];
/// Each LFO's row.
pub const LFOS: [[Control; 6]; LFOS_PER_TRACK] = [lfo_row(0), lfo_row(1)];

/// Every row, top to bottom.
pub const ROWS: [&[Control]; 2 + LFOS_PER_TRACK] = [&FILTER, &OUTPUT, &LFOS[0], &LFOS[1]];

impl Control {
    /// The setting's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Base(TrackControl::Cutoff) => "Cut",
            Self::Base(TrackControl::Resonance) => "Res",
            Self::Base(TrackControl::Pan) => "Pan",
            Self::Base(TrackControl::Delay) => "Delay",
            Self::Base(TrackControl::Reverb) => "Reverb",
            Self::Level => "Level",
            Self::Swing => "Swing",
            Self::Lfo(_, LfoField::Destination) => "Dest",
            Self::Lfo(_, LfoField::Shape) => "Shape",
            Self::Lfo(_, LfoField::Rate) => "Rate",
            Self::Lfo(_, LfoField::Depth) => "Depth",
            Self::Lfo(_, LfoField::Phase) => "Phase",
            Self::Lfo(_, LfoField::Retrig) => "Retrig",
        }
    }

    /// Whether this is two hex digits, entered as soon as the second is typed.
    fn is_hex(self) -> bool {
        matches!(self, Self::Base(_) | Self::Level)
    }

    /// Whether this is picked from a list rather than typed.
    fn is_choice(self) -> bool {
        matches!(
            self,
            Self::Lfo(
                _,
                LfoField::Destination | LfoField::Shape | LfoField::Retrig
            )
        )
    }
}

/// The next item in a list, going round.
fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
    let position = items.iter().position(|item| *item == current).unwrap_or(0);
    let step = if up { 1 } else { items.len() - 1 };
    items[(position + step) % items.len()]
}

impl App {
    /// What the cursor is on.
    pub fn track_control(&self) -> Control {
        let row = ROWS[self.track_row.min(ROWS.len() - 1)];
        row[self.track_field.min(row.len() - 1)]
    }

    fn page_track(&self) -> vt_model::song::Track {
        self.song()
            .tracks
            .get(self.cursor.track)
            .cloned()
            .unwrap_or_default()
    }

    fn set_lfo(&mut self, slot: usize, lfo: Lfo) {
        self.apply(&Action::SetLfo {
            track: self.cursor.track,
            slot,
            lfo,
        });
    }

    fn set_hex(&mut self, control: Control, value: u8) {
        let track = self.cursor.track;
        match control {
            Control::Base(control) => self.apply(&Action::SetTrackBase {
                track,
                control,
                value,
            }),
            Control::Level => self.apply(&Action::SetTrackLevel {
                track,
                level: value,
            }),
            _ => {}
        }
    }

    fn adjust_control(&mut self, up: bool, count: usize) {
        let control = self.track_control();
        let track = self.page_track();
        let change = |value: u8| {
            let moved = i64::from(value) + if up { 1 } else { -1 } * count as i64;
            moved.clamp(0, 0xFF) as u8
        };
        match control {
            Control::Base(which) => {
                let value = match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::Delay => track.delay,
                    TrackControl::Reverb => track.reverb,
                };
                self.set_hex(control, change(value));
            }
            Control::Level => self.set_hex(control, change(track.level)),
            Control::Swing => {
                let from = track.swing.unwrap_or(self.song().swing);
                let by = 10 * count.min(usize::from(MAX_SWING)) as i32;
                let moved = i32::from(from) + if up { by } else { -by };
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: Some(moved.clamp(i32::from(MIN_SWING), i32::from(MAX_SWING)) as u16),
                });
            }
            Control::Lfo(slot, field) => {
                let mut lfo = track.lfos[slot];
                for _ in 0..count.min(0x200) {
                    adjust_lfo(&mut lfo, field, up);
                }
                self.set_lfo(slot, lfo);
            }
        }
    }

    fn move_control(&mut self, row: usize, field: usize) {
        self.track_row = row.min(ROWS.len() - 1);
        self.track_field = field.min(ROWS[self.track_row].len() - 1);
    }

    /// Run a command on the track page. Returns false if it means the same as anywhere.
    pub(crate) fn run_track_page(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let (row, field) = (self.track_row, self.track_field);
        match cmd {
            Cmd::Move(Motion::Down) => self.move_control(row.saturating_add(times), field),
            Cmd::Move(Motion::Up) => self.move_control(row.saturating_sub(times), field),
            Cmd::Move(Motion::First) => self.move_control(0, field),
            Cmd::Move(Motion::Last) => self.move_control(ROWS.len() - 1, field),
            Cmd::Move(Motion::Right) => self.move_control(row, field.saturating_add(times)),
            Cmd::Move(Motion::Left) => self.move_control(row, field.saturating_sub(times)),
            Cmd::Move(Motion::RowStart) => self.move_control(row, 0),
            Cmd::Move(Motion::RowEnd) => self.move_control(row, usize::MAX),
            Cmd::Increase => self.adjust_control(true, times),
            Cmd::Decrease => self.adjust_control(false, times),
            Cmd::PickDestination => self.open_destination_picker(),
            Cmd::EnterInsert if self.track_control().is_choice() => {
                self.status = "Press + or - to change this".to_owned();
            }
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// List what an LFO can move, with what it moves now highlighted.
    fn open_destination_picker(&mut self) {
        let Control::Lfo(slot, LfoField::Destination) = self.track_control() else {
            self.status = "K chooses from a list on an LFO's Dest".to_owned();
            return;
        };
        let track = self.cursor.track;
        let current = self.page_track().lfos[slot].destination;
        let mut choices = vec![None];
        choices.extend(Destination::ALL.map(Some));
        let items = choices
            .iter()
            .map(|destination| Item {
                label: match destination {
                    Some(destination) => {
                        format!("{}  {}", destination.name(), meaning(*destination))
                    }
                    None => "---  Off".to_owned(),
                },
                pick: Pick::Destination {
                    track,
                    slot,
                    destination: *destination,
                },
            })
            .collect();
        let position = choices.iter().position(|choice| *choice == current);
        let mut picker = Picker::new("LFO destination", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    pub(crate) fn pick_destination(
        &mut self,
        track: usize,
        slot: usize,
        destination: Option<Destination>,
    ) {
        let Some(found) = self.song().tracks.get(track) else {
            return;
        };
        let lfo = Lfo {
            destination,
            ..found.lfos[slot]
        };
        self.apply(&Action::SetLfo { track, slot, lfo });
    }

    /// Type a value: two hex digits for levels, and anything else then `Enter`.
    pub(crate) fn insert_track_page(&mut self, key: KeyCode) {
        let control = self.track_control();
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if !control.is_hex() => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                if !text.is_empty() {
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if control.is_hex() && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    self.mode = Mode::Normal;
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if !control.is_hex() && self.entry.text.len() < 12 => {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_control(&mut self, control: Control, text: &str) {
        let track = self.page_track();
        match control {
            Control::Base(_) | Control::Level => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    self.set_hex(control, value);
                }
            }
            Control::Swing if text.eq_ignore_ascii_case("off") => {
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: None,
                });
            }
            Control::Swing => match swing(text) {
                Some(value) => self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: Some(value),
                }),
                None => self.status = "Swing is a percent from 50 to 75, or off".to_owned(),
            },
            Control::Lfo(slot, LfoField::Rate) => match text.parse::<Rate>() {
                Ok(rate) => {
                    let lfo = Lfo {
                        rate,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                Err(error) => self.status = error.to_string(),
            },
            Control::Lfo(slot, LfoField::Depth) => match parse_depth(text) {
                Some(depth) => {
                    let lfo = Lfo {
                        depth,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                None => self.status = "Depth is hex from -FF to +FF".to_owned(),
            },
            Control::Lfo(slot, LfoField::Phase) => match text.parse::<u16>() {
                Ok(phase) if phase < 360 => {
                    let lfo = Lfo {
                        phase,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                _ => self.status = "Phase is degrees from 0 to 359".to_owned(),
            },
            Control::Lfo(..) => {}
        }
    }
}

/// What a destination moves, for the picker.
fn meaning(destination: Destination) -> &'static str {
    match destination {
        Destination::Cut => "Filter cutoff",
        Destination::Res => "Filter resonance",
        Destination::Pan => "Pan",
        Destination::Vol => "Volume",
        Destination::Dly => "Delay send",
        Destination::Rev => "Reverb send",
    }
}

/// Move one setting of an LFO one notch.
fn adjust_lfo(lfo: &mut Lfo, field: LfoField, up: bool) {
    match field {
        LfoField::Destination => {
            let mut choices = vec![None];
            choices.extend(Destination::ALL.map(Some));
            lfo.destination = next_in(&choices, lfo.destination, up);
        }
        LfoField::Shape => lfo.shape = next_in(&Shape::ALL, lfo.shape, up),
        LfoField::Rate => lfo.rate = lfo.rate.nudged(up),
        LfoField::Depth => {
            let moved = lfo.depth + if up { 1 } else { -1 };
            lfo.depth = moved.clamp(-MAX_LFO_DEPTH, MAX_LFO_DEPTH);
        }
        LfoField::Phase => {
            let by = if up { PHASE_NUDGE } else { -PHASE_NUDGE };
            lfo.phase = (i32::from(lfo.phase) + by).rem_euclid(360) as u16;
        }
        LfoField::Retrig => lfo.retrig = !lfo.retrig,
    }
}

/// Read a depth, with an optional sign, in hex.
fn parse_depth(text: &str) -> Option<i16> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let depth = i16::from_str_radix(digits, 16).ok()?;
    (0..=MAX_LFO_DEPTH).contains(&depth).then_some(sign * depth)
}
