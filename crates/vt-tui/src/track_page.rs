//! What keys do on the track page.

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, TrackControl};
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_SWING, MIN_SWING, TRACK_ENVELOPES};
use vt_model::envelope::Envelope;
use vt_model::lfo::{Destination, LFOS_PER_TRACK, Lfo, MAX_LFO_DEPTH, Rate, Shape, note_rates};
use vt_model::selection::Motion;
use vt_model::song::{FilterType, Machine, Track};

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::command::swing;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How far a small and a large step move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
/// How far a small and a large step move a swing, in tenths of a percent.
const SWING_NUDGE: i32 = 10;
const SWING_JUMP: i32 = 50;
/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i64 = 0x10;
/// How many small steps a large one is for an envelope time.
const TIME_JUMP: u16 = 10;
/// How far a large step moves a sustain, in percent.
const SUSTAIN_JUMP: i16 = 10;

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

/// One setting of an envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeField {
    Destination,
    Attack,
    Decay,
    Sustain,
    Release,
    Depth,
    Reset,
}

/// One setting on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// What makes the sound.
    Machine,
    /// The file it plays.
    Sample,
    Mute,
    Solo,
    Base(TrackControl),
    Level,
    Swing,
    FilterType,
    Lfo(usize, LfoField),
    Envelope(usize, EnvelopeField),
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

const fn envelope_row(slot: usize) -> [Control; 7] {
    [
        Control::Envelope(slot, EnvelopeField::Destination),
        Control::Envelope(slot, EnvelopeField::Attack),
        Control::Envelope(slot, EnvelopeField::Decay),
        Control::Envelope(slot, EnvelopeField::Sustain),
        Control::Envelope(slot, EnvelopeField::Release),
        Control::Envelope(slot, EnvelopeField::Depth),
        Control::Envelope(slot, EnvelopeField::Reset),
    ]
}

/// Each envelope's row.
pub const ENVELOPES: [[Control; 7]; TRACK_ENVELOPES] = [envelope_row(0), envelope_row(1)];
/// Each LFO's row.
pub const LFOS: [[Control; 6]; LFOS_PER_TRACK] = [lfo_row(0), lfo_row(1)];

/// The mixer's columns.
pub const MIXER: [Control; 6] = [
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::Delay),
    Control::Base(TrackControl::Reverb),
    Control::Mute,
    Control::Solo,
];

const fn base(control: TrackControl) -> Control {
    Control::Base(control)
}

/// The Sound page of a track playing a sample, top to bottom, with output on the right.
pub const SAMPLE_ROWS: [&[Control]; 8] = [
    &[Control::Machine, Control::Level],
    &[Control::Sample, base(TrackControl::Pan)],
    &[Control::FilterType, base(TrackControl::Delay)],
    &[base(TrackControl::Cutoff), base(TrackControl::Reverb)],
    &[base(TrackControl::Resonance), Control::Swing],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];
/// The Sound page of a track playing a synth.
pub const SYNTH_ROWS: [&[Control]; 10] = [
    &[Control::Machine, Control::Level],
    &[base(TrackControl::Harmonics), base(TrackControl::Pan)],
    &[base(TrackControl::Timbre), base(TrackControl::Delay)],
    &[base(TrackControl::Morph), base(TrackControl::Reverb)],
    &[Control::FilterType, Control::Swing],
    &[base(TrackControl::Cutoff)],
    &[base(TrackControl::Resonance)],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];
/// The Mod page's rows, top to bottom.
pub const MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] =
    [&ENVELOPES[0], &ENVELOPES[1], &LFOS[0], &LFOS[1]];

/// A part of the track page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Sound,
    Mod,
}

impl Page {
    /// The page's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Mod => "Mod",
        }
    }

    /// The page's rows, top to bottom, for a track with this machine.
    pub fn rows(self, machine: Machine) -> &'static [&'static [Control]] {
        match (self, machine) {
            (Self::Sound, Machine::Sample) => &SAMPLE_ROWS,
            (Self::Sound, Machine::VirtualAnalog) => &SYNTH_ROWS,
            (Self::Mod, _) => &MOD_ROWS,
        }
    }
}

impl Control {
    /// The setting's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Base(TrackControl::Cutoff) => "Cut",
            Self::Base(TrackControl::Resonance) => "Res",
            Self::Base(TrackControl::Pan) => "Pan",
            Self::Base(TrackControl::Delay) => "Delay",
            Self::Base(TrackControl::Reverb) => "Reverb",
            Self::Base(TrackControl::Drive) => "Drive",
            Self::Base(TrackControl::LowCut) => "Low cut",
            Self::Base(TrackControl::HighCut) => "High cut",
            Self::Machine => "Machine",
            Self::Base(TrackControl::Harmonics) => "Harm",
            Self::Base(TrackControl::Timbre) => "Timbre",
            Self::Base(TrackControl::Morph) => "Morph",
            Self::Sample => "File",
            Self::Mute => "Mute",
            Self::Solo => "Solo",
            Self::FilterType => "Type",
            Self::Level => "Level",
            Self::Swing => "Swing",
            Self::Lfo(_, LfoField::Destination) => "Dest",
            Self::Lfo(_, LfoField::Shape) => "Shape",
            Self::Lfo(_, LfoField::Rate) => "Rate",
            Self::Lfo(_, LfoField::Depth) => "Depth",
            Self::Lfo(_, LfoField::Phase) => "Phase",
            Self::Lfo(_, LfoField::Retrig) => "Retrig",
            Self::Envelope(_, EnvelopeField::Destination) => "Dest",
            Self::Envelope(_, EnvelopeField::Attack) => "Attack",
            Self::Envelope(_, EnvelopeField::Decay) => "Decay",
            Self::Envelope(_, EnvelopeField::Sustain) => "Sustain",
            Self::Envelope(_, EnvelopeField::Release) => "Release",
            Self::Envelope(_, EnvelopeField::Depth) => "Depth",
            Self::Envelope(_, EnvelopeField::Reset) => "Reset",
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
            Self::Machine
                | Self::Sample
                | Self::Mute
                | Self::Solo
                | Self::FilterType
                | Self::Lfo(
                    _,
                    LfoField::Destination | LfoField::Shape | LfoField::Retrig
                )
                | Self::Envelope(_, EnvelopeField::Destination | EnvelopeField::Reset)
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
        if self.view == View::Mixer {
            return MIXER[self.mixer_column.min(MIXER.len() - 1)];
        }
        let rows = self.page_rows();
        let row = rows[self.track_row.min(rows.len() - 1)];
        row[self.track_field.min(row.len() - 1)]
    }

    /// The rows of the page showing, which depend on the track's machine.
    pub(crate) fn page_rows(&self) -> &'static [&'static [Control]] {
        let machine = self
            .song()
            .tracks
            .get(self.cursor.track)
            .map_or(Machine::Sample, |track| track.machine);
        self.track_page.rows(machine)
    }

    fn page_track(&self) -> Track {
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

    fn adjust_control(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        let track = self.page_track();
        let change = |value: u8| {
            let size = if large { HEX_JUMP } else { 1 };
            let moved = i64::from(value) + if up { size } else { -size } * count as i64;
            moved.clamp(0, 0xFF) as u8
        };
        match control {
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                let on = if control == Control::Mute {
                    live.mute
                } else {
                    live.solo
                };
                if on != up {
                    self.toggle_control(control);
                }
            }
            Control::Machine => {
                let machine = next_in(&Machine::ALL, track.machine, up);
                self.apply(&Action::SetTrackMachine {
                    track: self.cursor.track,
                    machine,
                });
            }
            Control::Sample => {
                self.status = "K chooses a sample".to_owned();
            }
            Control::Base(which) => {
                let value = match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::Delay => track.delay,
                    TrackControl::Reverb => track.reverb,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Harmonics => track.harmonics,
                    TrackControl::Timbre => track.timbre,
                    TrackControl::Morph => track.morph,
                };
                self.set_hex(control, change(value));
            }
            Control::Level => self.set_hex(control, change(track.level)),
            Control::FilterType => {
                // One way at a time, as a large step would skip through a short list.
                let filter = next_in(&FilterType::ALL, track.filter, up);
                self.apply(&Action::SetFilterType {
                    track: self.cursor.track,
                    filter,
                });
            }
            Control::Swing => {
                let from = track.swing.unwrap_or(self.song().swing);
                let size = if large { SWING_JUMP } else { SWING_NUDGE };
                let by = size * count.min(usize::from(MAX_SWING)) as i32;
                let moved = i32::from(from) + if up { by } else { -by };
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: Some(moved.clamp(i32::from(MIN_SWING), i32::from(MAX_SWING)) as u16),
                });
            }
            Control::Lfo(slot, field) => {
                let mut lfo = track.lfos[slot];
                for _ in 0..count.min(0x200) {
                    adjust_lfo(&mut lfo, field, up, large);
                }
                self.set_lfo(slot, lfo);
            }
            Control::Envelope(slot, field) => {
                let mut envelope = track.envelopes[slot];
                for _ in 0..count.min(0x200) {
                    adjust_envelope(&mut envelope, field, up, large);
                }
                self.set_envelope(slot, envelope);
            }
        }
    }

    fn set_envelope(&mut self, slot: usize, envelope: Envelope) {
        self.apply(&Action::SetEnvelope {
            track: self.cursor.track,
            slot,
            envelope,
        });
    }

    fn move_control(&mut self, row: usize, field: usize) {
        let rows = self.page_rows();
        self.track_row = row.min(rows.len() - 1);
        self.track_field = field.min(rows[self.track_row].len() - 1);
    }

    /// Run a command on the mixer. Moving goes between tracks, and everything else means what it
    /// does on the track page.
    pub(crate) fn run_mixer(&mut self, cmd: Cmd) -> bool {
        let times = self.count.unwrap_or(1).max(1) as isize;
        let track = self.cursor.track as isize;
        match cmd {
            Cmd::Move(Motion::Down) => self.move_track(times),
            Cmd::Move(Motion::Up) => self.move_track(-times),
            Cmd::Move(Motion::First) => self.move_track(-track),
            Cmd::Move(Motion::Last) => {
                self.move_track(self.song().tracks.len() as isize - 1 - track)
            }
            Cmd::Move(Motion::Right) => {
                let moved = self.mixer_column.saturating_add(times as usize);
                self.mixer_column = moved.min(MIXER.len() - 1);
            }
            Cmd::Move(Motion::Left) => {
                self.mixer_column = self.mixer_column.saturating_sub(times as usize);
            }
            Cmd::Move(Motion::RowStart) => self.mixer_column = 0,
            Cmd::Move(Motion::RowEnd) => self.mixer_column = MIXER.len() - 1,
            _ => return self.run_track_page(cmd),
        }
        self.count = None;
        true
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
            Cmd::Move(Motion::Last) => self.move_control(usize::MAX, field),
            Cmd::Move(Motion::Right) => self.move_control(row, field.saturating_add(times)),
            Cmd::Move(Motion::Left) => self.move_control(row, field.saturating_sub(times)),
            Cmd::Move(Motion::RowStart) => self.move_control(row, 0),
            Cmd::Move(Motion::RowEnd) => self.move_control(row, usize::MAX),
            Cmd::ShowPage(page) => {
                self.track_page = page;
                self.move_control(0, field);
            }
            Cmd::Increase => self.adjust_control(true, times, false),
            Cmd::Decrease => self.adjust_control(false, times, false),
            Cmd::IncreaseLarge => self.adjust_control(true, times, true),
            Cmd::DecreaseLarge => self.adjust_control(false, times, true),
            Cmd::ClearField => self.reset_control(),
            Cmd::PickValue => self.open_control_picker(),
            Cmd::EnterInsert if self.track_control().is_choice() => self.open_control_picker(),
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

    /// Put the setting under the cursor back to its default. If it's there already, nothing changes
    /// and no undo step is left.
    fn reset_control(&mut self) {
        let control = self.track_control();
        let track = self.page_track();
        let fresh = Track::default();
        match control {
            Control::Base(which) => {
                let value = |track: &Track| match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::Delay => track.delay,
                    TrackControl::Reverb => track.reverb,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Harmonics => track.harmonics,
                    TrackControl::Timbre => track.timbre,
                    TrackControl::Morph => track.morph,
                };
                if value(&track) != value(&fresh) {
                    self.set_hex(control, value(&fresh));
                }
            }
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                if (control == Control::Mute && live.mute)
                    || (control == Control::Solo && live.solo)
                {
                    self.toggle_control(control);
                }
            }
            Control::Machine => {
                if track.machine != fresh.machine {
                    self.apply(&Action::SetTrackMachine {
                        track: self.cursor.track,
                        machine: fresh.machine,
                    });
                }
            }
            Control::Sample if track.sample.is_some() => {
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample: None,
                });
            }
            Control::Sample => {}
            Control::Level if track.level != fresh.level => self.set_hex(control, fresh.level),
            Control::Level => {}
            Control::Swing if track.swing.is_some() => {
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: None,
                });
            }
            Control::Swing => {}
            Control::FilterType => {
                if track.filter != fresh.filter {
                    self.apply(&Action::SetFilterType {
                        track: self.cursor.track,
                        filter: fresh.filter,
                    });
                }
            }
            Control::Lfo(slot, field) => {
                let (lfo, default) = (track.lfos[slot], Lfo::default());
                let reset = match field {
                    LfoField::Destination => Lfo {
                        destination: default.destination,
                        ..lfo
                    },
                    LfoField::Shape => Lfo {
                        shape: default.shape,
                        ..lfo
                    },
                    LfoField::Rate => Lfo {
                        rate: default.rate,
                        ..lfo
                    },
                    LfoField::Depth => Lfo {
                        depth: default.depth,
                        ..lfo
                    },
                    LfoField::Phase => Lfo {
                        phase: default.phase,
                        ..lfo
                    },
                    LfoField::Retrig => Lfo {
                        retrig: default.retrig,
                        ..lfo
                    },
                };
                if reset != lfo {
                    self.set_lfo(slot, reset);
                }
            }
            Control::Envelope(slot, field) => {
                let (envelope, default) = (track.envelopes[slot], fresh.envelopes[slot]);
                let reset = match field {
                    EnvelopeField::Destination => Envelope {
                        destination: default.destination,
                        ..envelope
                    },
                    EnvelopeField::Attack => Envelope {
                        attack_ms: default.attack_ms,
                        ..envelope
                    },
                    EnvelopeField::Decay => Envelope {
                        decay_ms: default.decay_ms,
                        ..envelope
                    },
                    EnvelopeField::Sustain => Envelope {
                        sustain: default.sustain,
                        ..envelope
                    },
                    EnvelopeField::Release => Envelope {
                        release_ms: default.release_ms,
                        ..envelope
                    },
                    EnvelopeField::Depth => Envelope {
                        depth: default.depth,
                        ..envelope
                    },
                    EnvelopeField::Reset => Envelope {
                        reset: default.reset,
                        ..envelope
                    },
                };
                if reset != envelope {
                    self.set_envelope(slot, reset);
                }
            }
        }
    }

    /// List what an LFO's setting can be, with the current one highlighted.
    /// Switch this track's mute or solo.
    fn toggle_control(&mut self, control: Control) {
        if control == Control::Mute {
            self.toggle_mute();
        } else {
            self.toggle_solo();
        }
    }

    fn open_control_picker(&mut self) {
        let control = self.track_control();
        if matches!(control, Control::Mute | Control::Solo) {
            self.toggle_control(control);
            return;
        }
        if self.track_control() == Control::Sample {
            self.open_default_sample_picker();
            return;
        }
        if self.track_control() == Control::FilterType {
            self.open_filter_picker();
            return;
        }
        if self.track_control() == Control::Machine {
            self.open_machine_picker();
            return;
        }
        if let Control::Envelope(slot, field) = self.track_control() {
            self.open_envelope_picker(slot, field);
            return;
        }
        let Control::Lfo(slot, field) = self.track_control() else {
            self.status =
                "K chooses from a list on Machine, File, Type, Dest, Shape, Rate, Retrig and Reset"
                    .to_owned();
            return;
        };
        let track = self.cursor.track;
        let lfo = self.page_track().lfos[slot];
        let (title, choices): (&'static str, Vec<(String, Lfo)>) = match field {
            LfoField::Destination => {
                let off = (
                    "---  Off".to_owned(),
                    Lfo {
                        destination: None,
                        ..lfo
                    },
                );
                let on = Destination::ALL.map(|destination| {
                    (
                        format!("{}  {}", destination.name(), meaning(destination)),
                        Lfo {
                            destination: Some(destination),
                            ..lfo
                        },
                    )
                });
                ("LFO destination", std::iter::once(off).chain(on).collect())
            }
            LfoField::Shape => {
                let shapes =
                    Shape::ALL.map(|shape| (shape.name().to_owned(), Lfo { shape, ..lfo }));
                ("LFO shape", shapes.to_vec())
            }
            LfoField::Rate => {
                let rates = note_rates()
                    .into_iter()
                    .map(|rate| (rate.to_string(), Lfo { rate, ..lfo }))
                    .collect();
                ("LFO rate. i types any other", rates)
            }
            LfoField::Retrig => {
                let choices = [false, true].map(|retrig| {
                    let name = if retrig { "on" } else { "off" };
                    (name.to_owned(), Lfo { retrig, ..lfo })
                });
                ("LFO retrig", choices.to_vec())
            }
            LfoField::Depth | LfoField::Phase => {
                self.status =
                    "K chooses from a list on Type, Dest, Shape, Rate, Retrig and Reset".to_owned();
                return;
            }
        };
        let position = choices.iter().position(|(_, choice)| *choice == lfo);
        let items = choices
            .into_iter()
            .map(|(label, lfo)| Item {
                label,
                pick: Pick::Lfo { track, slot, lfo },
            })
            .collect();
        let mut picker = Picker::new(title, items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the filter types, with what each does and the current one highlighted.
    fn open_filter_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().filter;
        let items = FilterType::ALL
            .map(|filter| Item {
                label: format!("{}  {}", filter.name(), filter.meaning()),
                pick: Pick::FilterType { track, filter },
            })
            .to_vec();
        let position = FilterType::ALL.iter().position(|filter| *filter == current);
        let mut picker = Picker::new("Filter type", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the machines, with what each does and the current one highlighted.
    fn open_machine_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().machine;
        let items = Machine::ALL
            .map(|machine| Item {
                label: format!("{:<8}{}", machine.name(), machine.meaning()),
                pick: Pick::Machine { track, machine },
            })
            .to_vec();
        let position = Machine::ALL.iter().position(|machine| *machine == current);
        let mut picker = Picker::new("Machine", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List what an envelope's setting can be, with the current one highlighted.
    fn open_envelope_picker(&mut self, slot: usize, field: EnvelopeField) {
        let track = self.cursor.track;
        let envelope = self.page_track().envelopes[slot];
        let (title, choices): (&'static str, Vec<(String, Envelope)>) = match field {
            EnvelopeField::Destination => {
                let off = (
                    "---  Off".to_owned(),
                    Envelope {
                        destination: None,
                        ..envelope
                    },
                );
                let on = Destination::ALL.map(|destination| {
                    (
                        format!("{}  {}", destination.name(), meaning(destination)),
                        Envelope {
                            destination: Some(destination),
                            ..envelope
                        },
                    )
                });
                (
                    "Envelope destination",
                    std::iter::once(off).chain(on).collect(),
                )
            }
            EnvelopeField::Reset => {
                let choices = [true, false].map(|reset| {
                    let name = if reset {
                        "on  Each note starts it from zero"
                    } else {
                        "off  Each note goes on from where it is"
                    };
                    (name.to_owned(), Envelope { reset, ..envelope })
                });
                ("Envelope reset", choices.to_vec())
            }
            _ => {
                self.status = "K chooses from a list on Dest and Reset".to_owned();
                return;
            }
        };
        let position = choices.iter().position(|(_, choice)| *choice == envelope);
        let items = choices
            .into_iter()
            .map(|(label, envelope)| Item {
                label,
                pick: Pick::Envelope {
                    track,
                    slot,
                    envelope,
                },
            })
            .collect();
        let mut picker = Picker::new(title, items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
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
            Control::Mute | Control::Solo => {}
            Control::Sample => {}
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
            Control::Lfo(..) | Control::FilterType | Control::Machine => {}
            Control::Envelope(slot, field) => {
                let current = track.envelopes[slot];
                let ms = text.parse::<u16>().ok().filter(|ms| *ms <= MAX_ENVELOPE_MS);
                let changed = match field {
                    EnvelopeField::Attack => ms.map(|attack_ms| Envelope {
                        attack_ms,
                        ..current
                    }),
                    EnvelopeField::Decay => ms.map(|decay_ms| Envelope {
                        decay_ms,
                        ..current
                    }),
                    EnvelopeField::Release => ms.map(|release_ms| Envelope {
                        release_ms,
                        ..current
                    }),
                    EnvelopeField::Sustain => text
                        .trim_end_matches('%')
                        .parse::<u8>()
                        .ok()
                        .filter(|sustain| *sustain <= 100)
                        .map(|sustain| Envelope { sustain, ..current }),
                    EnvelopeField::Depth => {
                        parse_depth(text).map(|depth| Envelope { depth, ..current })
                    }
                    EnvelopeField::Destination | EnvelopeField::Reset => None,
                };
                match changed {
                    Some(envelope) => self.set_envelope(slot, envelope),
                    None => {
                        self.status = match field {
                            EnvelopeField::Sustain => "Sustain is a percent from 0 to 100",
                            EnvelopeField::Depth => "Depth is hex from -FF to +FF",
                            _ => "Times are milliseconds from 0 to 8000",
                        }
                        .to_owned();
                    }
                }
            }
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
        Destination::Pitch => "Pitch, with depth in semitones",
        Destination::Har => "Synth detune",
        Destination::Tim => "Synth square",
        Destination::Mor => "Synth saw",
    }
}

/// How far a small step moves an envelope time. Short times need finer steps than long ones.
fn time_step(value: u16) -> u16 {
    match value {
        0..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}

/// Move one setting of an envelope one notch.
fn adjust_envelope(envelope: &mut Envelope, field: EnvelopeField, up: bool, large: bool) {
    let time = |value: u16| {
        let size = if large { TIME_JUMP } else { 1 };
        if up {
            (value + time_step(value) * size).min(MAX_ENVELOPE_MS)
        } else {
            value.saturating_sub(time_step(value.saturating_sub(1)) * size)
        }
    };
    match field {
        EnvelopeField::Destination => {
            let mut choices = vec![None];
            choices.extend(Destination::ALL.map(Some));
            envelope.destination = next_in(&choices, envelope.destination, up);
        }
        EnvelopeField::Attack => envelope.attack_ms = time(envelope.attack_ms),
        EnvelopeField::Decay => envelope.decay_ms = time(envelope.decay_ms),
        EnvelopeField::Release => envelope.release_ms = time(envelope.release_ms),
        EnvelopeField::Sustain => {
            let size = if large { SUSTAIN_JUMP } else { 1 };
            let by = if up { size } else { -size };
            envelope.sustain = (i16::from(envelope.sustain) + by).clamp(0, 100) as u8;
        }
        EnvelopeField::Depth => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = envelope.depth + if up { size } else { -size };
            envelope.depth = moved.clamp(-MAX_LFO_DEPTH, MAX_LFO_DEPTH);
        }
        EnvelopeField::Reset => envelope.reset = !envelope.reset,
    }
}

/// Move one setting of an LFO one notch.
fn adjust_lfo(lfo: &mut Lfo, field: LfoField, up: bool, large: bool) {
    match field {
        LfoField::Destination => {
            let mut choices = vec![None];
            choices.extend(Destination::ALL.map(Some));
            lfo.destination = next_in(&choices, lfo.destination, up);
        }
        LfoField::Shape => lfo.shape = next_in(&Shape::ALL, lfo.shape, up),
        LfoField::Rate if large => lfo.rate = lfo.rate.jumped(up),
        LfoField::Rate => lfo.rate = lfo.rate.nudged(up),
        LfoField::Depth => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = lfo.depth + if up { size } else { -size };
            lfo.depth = moved.clamp(-MAX_LFO_DEPTH, MAX_LFO_DEPTH);
        }
        LfoField::Phase => {
            let size = if large { PHASE_JUMP } else { PHASE_NUDGE };
            let by = if up { size } else { -size };
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
