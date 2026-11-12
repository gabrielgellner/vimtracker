//! vimtracker's devices as the Roto shows them: each device of a track becomes a device of knobs,
//! and buttons if it's stored, each with its setting's name, the feel its kind gives it, where
//! it's set, and its family's colour. Pure: a track and its song in, the Roto's description out,
//! so it's tested with no Roto at all.
//!
//! **Where each setting goes** is its kind of device's layout ([`layouts`]), and nothing else:
//! not the track page's grid, which is laid out for the screen (issue #419). A device with off/on
//! settings is stored on the Roto, once vimtracker's setups are there, so they're on buttons; a
//! live device has them on two-step knobs.
//!
//! **Colours** are by family, from the one table [`FAMILY_COLOURS`].
//!
//! **Feel** follows the setting's kind, as issue #411's table has it: a list of up to 16 choices
//! clicks through them, each named; of 17 to 24, it clicks with names for the first 16; a longer
//! list, or one that changes with the track, such as an LFO's `Dest` or the song's samples, turns
//! smoothly; an off/on setting is a switch; a centred value has a notch at its middle; and any
//! other value turns smoothly.

use vt_model::accumulator::{Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::TrackControl;
use vt_model::arpeggio::{ArpMode, ArpRate, MOST_OCTAVES};
use vt_model::constants::{
    BAR_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET, TICKS_PER_STEP,
};
use vt_model::envelope::{Curve, Hold, MAX_HOLD_MS, MAX_RATCHET};
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{Fade, LfoMode, Rate, Shape};
use vt_model::modulation::{MAX_DEPTH, ModKind, Polarity};
use vt_model::settings::{Kind, SettingId, Units};
use vt_model::song::{FilterType, Machine, Song, Track, TrackKind};
use vt_model::speed::Speed;
use vt_roto::Label;
use vt_roto::bytes::Rgb;
use vt_roto::colour;
use vt_roto::live::{Feel, LiveControl, LiveDevice, LiveTrack, position_at, position_of};
use vt_roto::store::{NAMESPACE, OwnSetup};
use vt_roto::sysex::{CONTROLS, MAX_STEPS};

use ratatui::style::Color;

use super::layouts::{self, Setting, Turn};
use crate::theme;
use crate::track_page::{AccumulatorField, ArpField, Control, DeviceSpec, ModField, Page};
use crate::views::track::toggle_on;

/// What a setting is for, which gives its knob's colour, the same wherever the setting is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// Loudness and gain: levels, sends, trims and dynamics. Green, as the meters are at a safe
    /// level.
    Level,
    /// Tone: the filter and the EQ's bands. Blue.
    Tone,
    /// Where it sits: pan. Teal.
    Space,
    /// Time and pitch: rates, lengths, glides, pitches. Yellow.
    Time,
    /// Modulation: what a mod slot, the accumulator or the arpeggio does. Mauve.
    Modulation,
    /// Character: drive and a machine's or an effect's own colour of sound. Peach.
    Character,
    /// Which one: a machine, an effect, a sample, a type, a group. The text's colour.
    Choice,
}

/// One family's colours: on screen, on the Roto, and what goes in it, for the guide's legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FamilyColour {
    pub family: Family,
    /// Its colour in the app's palette.
    pub screen: Color,
    /// Its colour scheme on the Roto, an index into `vt_roto::colour::SCHEMES`: the vivid scheme
    /// of the screen colour's hue, as the nearest to the palette's pastels read too alike on the
    /// Roto's small screens.
    pub scheme: u8,
    /// The colour's name, and what has it.
    pub name: &'static str,
    pub holds: &'static str,
}

/// The one table of what colour each kind of setting is, on screen and on the Roto. A knob's or
/// a button's colour is its family's here, unless its layout gives it another family.
pub const FAMILY_COLOURS: [FamilyColour; 7] = [
    FamilyColour {
        family: Family::Level,
        screen: theme::GREEN,
        scheme: 19,
        name: "green",
        holds: "levels, sends, trims, mixes, gains, and a compressor's dynamics",
    },
    FamilyColour {
        family: Family::Tone,
        screen: theme::BLUE,
        scheme: 22,
        name: "blue",
        holds: "a filter's cutoff, resonance, type and cuts, and an EQ's bands",
    },
    FamilyColour {
        family: Family::Space,
        screen: theme::TEAL,
        scheme: 20,
        name: "teal",
        holds: "pan",
    },
    FamilyColour {
        family: Family::Time,
        screen: theme::YELLOW,
        scheme: 17,
        name: "yellow",
        holds: "times, rates, lengths, glides and pitches",
    },
    FamilyColour {
        family: Family::Modulation,
        screen: theme::MAUVE,
        scheme: 24,
        name: "violet",
        holds: "what a mod slot, the accumulator or the arpeggio does",
    },
    FamilyColour {
        family: Family::Character,
        screen: theme::PEACH,
        scheme: 15,
        name: "orange",
        holds: "drive, and a machine's or an effect's own character",
    },
    FamilyColour {
        family: Family::Choice,
        screen: theme::TEXT,
        scheme: 13,
        name: "white",
        holds: "choices: a machine, an effect, a sample, a type, off and on",
    },
];

impl Family {
    fn colours(self) -> &'static FamilyColour {
        FAMILY_COLOURS
            .iter()
            .find(|colours| colours.family == self)
            .expect("every family has a row")
    }

    /// The family's colour, from the app's palette.
    pub fn colour(self) -> Color {
        self.colours().screen
    }

    /// The family's colour scheme on the Roto.
    pub fn scheme(self) -> u8 {
        self.colours().scheme
    }

    /// A setting of the registry's family, by its units.
    fn of_units(units: Units) -> Self {
        match units {
            Units::Decibels
            | Units::Threshold
            | Units::Ratio
            | Units::Makeup
            | Units::Range
            | Units::Input
            | Units::Ceiling
            | Units::VelocityDelta => Self::Level,
            Units::Hertz
            | Units::Boost
            | Units::Q
            | Units::AcidCutoff
            | Units::Vowel
            | Units::ShiftHertz
            | Units::ExciterTone => Self::Tone,
            Units::Pan | Units::SpreadHertz => Self::Space,
            Units::Semitones
            | Units::Cents
            | Units::Attack
            | Units::Release
            | Units::LimitRelease
            | Units::FlangerDelay
            | Units::Grain
            | Units::GrainTime
            | Units::GrainLength
            | Units::Density
            | Units::AcidDecay
            | Units::Slide
            | Units::GateLength
            | Units::GateDelta
            | Units::Glide
            | Units::LoopTime
            | Units::ExciterAttack
            | Units::ExciterRelease => Self::Time,
            Units::Depth | Units::Degrees => Self::Modulation,
            _ => Self::Character,
        }
    }
}

/// The registry's setting a control is on this track, if it's one.
fn registry_setting(control: Control, track: &Track) -> Option<SettingId> {
    match control {
        Control::Setting(_) => control.setting_on(track),
        Control::Base(which) => which.setting(),
        Control::Level => Some(SettingId::LEVEL),
        Control::InsertParam(..) => control.value_on(track).map(|(id, _)| id),
        _ => None,
    }
}

/// A control's family.
pub fn family(control: Control, track: &Track) -> Family {
    if let Some(id) = registry_setting(control, track) {
        let entry = id.entry();
        // A resonance and an effect's cuts are the filter's kind, though each is a share like
        // any other; an effect's mix is how loud it is, as its gain is; and a rate or a time
        // chosen from note divisions is a time.
        return match (entry.kind, entry.key) {
            (_, "res" | "resonance" | "lowcut" | "highcut") => Family::Tone,
            (_, "mix") => Family::Level,
            // A compressor's ratio and times chosen from its own list are a ratio and times,
            // as the ones that sweep are.
            (Kind::Stepped { .. }, key) if key.ends_with("ratio") => Family::Level,
            (Kind::Stepped { .. }, key) if key.ends_with("attack") || key.ends_with("release") => {
                Family::Time
            }
            (Kind::Stepped { .. }, "time" | "rate") => Family::Time,
            (Kind::Continuous { units, .. }, _) => Family::of_units(units),
            (Kind::Stepped { .. }, _) => Family::Choice,
        };
    }
    match control {
        Control::Trim | Control::Base(TrackControl::SendA | TrackControl::SendB) => Family::Level,
        Control::Limiter => Family::Level,
        Control::Base(TrackControl::LowCut | TrackControl::HighCut)
        | Control::FilterType
        | Control::FilterMode => Family::Tone,
        Control::Base(TrackControl::Drive) => Family::Character,
        Control::GateLength
        | Control::Speed
        | Control::Groove
        | Control::Reset
        | Control::Glide
        | Control::Mod(
            _,
            ModField::Rate
            | ModField::Attack
            | ModField::Decay
            | ModField::Release
            | ModField::Hold,
        )
        | Control::Arpeggio(ArpField::Rate) => Family::Time,
        Control::Mod(_, ModField::Sustain) => Family::Level,
        Control::Mod(..) | Control::Accumulator(_) | Control::Arpeggio(_) => Family::Modulation,
        _ => Family::Choice,
    }
}

/// A knob's name: its setting's on the track page, an effect's setting by its own name.
pub fn name(control: Control, track: &Track) -> Label {
    let text = match control {
        Control::InsertParam(..) => control.name_on(track),
        _ => control.label_on(track),
    };
    Label::new(text)
}

/// A knob for one of a list of choices, the `at`th: stepped and named for up to 24, smooth past
/// that, as a list that long is turned through rather than clicked.
fn list(names: impl IntoIterator<Item = String>, at: usize) -> (Feel, u16) {
    let names: Vec<Label> = names.into_iter().map(|name| Label::new(&name)).collect();
    let count = names.len();
    let position = position_of(at, count);
    if (2..=usize::from(MAX_STEPS)).contains(&count) {
        (Feel::Steps(names), position)
    } else {
        (Feel::Smooth, position)
    }
}

/// A knob for one of a list that changes with the track or the song, such as a destination: it
/// turns smoothly, as its steps would move under it.
fn changing(at: usize, count: usize) -> (Feel, u16) {
    (Feel::Smooth, position_of(at, count))
}

/// A knob for a value from 0 to a top, centred or not.
fn value(value: u16, top: u16, centred: bool) -> (Feel, u16) {
    let feel = if centred {
        Feel::CentreNotch
    } else {
        Feel::Smooth
    };
    (feel, position_at(f64::from(value) / f64::from(top.max(1))))
}

/// A knob for a setting in the registry, with its coarse value and fine part together.
fn registry(id: SettingId, coarse: u8, fine: u8) -> (Feel, u16) {
    match id.entry().kind {
        Kind::Continuous { top, middle, .. } => {
            let whole = u16::from(coarse.min(top)) << 8 | u16::from(fine);
            value(whole, u16::from(top) << 8, middle)
        }
        Kind::Stepped { choices, .. } => list(
            choices.iter().map(|choice| (*choice).to_owned()),
            usize::from(coarse),
        ),
    }
}

/// Where an envelope's time is among the steps `=` takes: 1 ms to 10 ms, 5 ms to 100 ms, then
/// 25 ms.
fn time_index(ms: u16) -> u16 {
    match ms {
        0..=9 => ms,
        10..=99 => 10 + (ms - 10) / 5,
        _ => 28 + (ms - 100) / 25,
    }
}

/// Every time the steps `=` takes go through, from 0 ms to the longest.
pub(crate) fn time_steps() -> Vec<u16> {
    (0..10)
        .chain((10..100).step_by(5))
        .chain((100..=MAX_ENVELOPE_MS).step_by(25))
        .collect()
}

/// Every hold an AHD's knob goes through: `Note` at its bottom, below no time, then the times the
/// steps `=` takes, up to the longest hold.
pub(crate) fn hold_steps() -> Vec<Hold> {
    std::iter::once(Hold::Note)
        .chain(
            time_steps()
                .into_iter()
                .take_while(|ms| *ms <= MAX_HOLD_MS)
                .map(Hold::Ms),
        )
        .collect()
}

/// How far along an AHD's holds one is, as [`hold_steps`] has them.
fn hold_position(hold: Hold) -> u16 {
    let steps = hold_steps();
    let at = match hold {
        Hold::Note => 0,
        Hold::Ms(ms) => steps
            .iter()
            .position(|step| matches!(step, Hold::Ms(at) if *at >= ms))
            .unwrap_or(steps.len() - 1),
    };
    position_at(at as f64 / (steps.len() - 1) as f64)
}

/// How far along an envelope's times one is, by the steps `=` takes, so a short time is as easy
/// to set on the knob as with keys.
pub(crate) fn time_position(ms: u16) -> u16 {
    position_at(f64::from(time_index(ms)) / f64::from(time_index(MAX_ENVELOPE_MS)))
}

/// How fast an LFO is, from its slowest, 64 bars a cycle, to its fastest, 100 Hz, evenly in
/// octaves, at 120 BPM for the rates in hertz, so every kind of rate has a place on one knob.
pub(crate) fn rate_position(rate: Rate) -> u16 {
    const SLOWEST: f64 = -6.0;
    const FASTEST: f64 = 7.65;
    let bar_ticks = (BAR_STEPS as u64 * TICKS_PER_STEP) as f64;
    let cycles_a_bar = rate.cycles(bar_ticks, 2.0).max(f64::MIN_POSITIVE);
    position_at((cycles_a_bar.log2() - SLOWEST) / (FASTEST - SLOWEST))
}

/// A depth either side of none, as a mod slot's.
fn depth(depth: i16) -> (Feel, u16) {
    let top = MAX_DEPTH;
    value(
        (depth.clamp(-top, top) + top) as u16,
        (2 * top) as u16,
        true,
    )
}

/// Whether a mod slot's two-way choice is on, as its button shows it: any slot's polarity is `Bi`
/// (the button under its `Depth`).
fn mod_switch_on(control: Control, track: &Track) -> Option<bool> {
    match control {
        Control::Mod(slot, ModField::Polarity) => Some(track.mods[slot].polarity() == Polarity::Bi),
        _ => None,
    }
}

/// A knob's feel and where it's set, for a control on this track, or nothing if the track
/// hasn't the setting, as a fixed track hasn't `Pitch`, or it has nothing to choose, as a filter
/// type without modes has for `Mode`.
fn feel(control: Control, track: &Track, song: &Song) -> Option<(Feel, u16)> {
    if !control.exists_on(track) {
        return None;
    }
    if let Some(on) = toggle_on(control, track, song).or_else(|| mod_switch_on(control, track)) {
        return Some((Feel::Switch, if on { position_of(1, 2) } else { 0 }));
    }
    let names = |items: &[&str]| {
        items
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    let found = match control {
        Control::Machine => list(
            Machine::ALL.iter().map(|machine| machine.name().to_owned()),
            Machine::ALL
                .iter()
                .position(|machine| *machine == track.machine)
                .unwrap_or(0),
        ),
        Control::Sample => {
            let numbers: Vec<u8> = std::iter::once(0)
                .chain(song.bank.iter().map(|(number, _)| number))
                .collect();
            let at = numbers
                .iter()
                .position(|number| *number == track.sample())
                .unwrap_or(0);
            changing(at, numbers.len())
        }
        Control::Setting(_) => {
            let id = control.setting_on(track)?;
            registry(id, track.setting(id), track.fine(id))
        }
        Control::Base(which) => match which.setting() {
            Some(id) => {
                let coarse = match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::SendA => track.send_a,
                    TrackControl::SendB => track.send_b,
                    TrackControl::VelocityDelta => track.velocity_delta,
                    _ => track.pitch,
                };
                registry(id, coarse, track.fine(id))
            }
            None => {
                let coarse = match which {
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    _ => track.high_cut,
                };
                value(u16::from(coarse), 0xFF, false)
            }
        },
        Control::Level => registry(SettingId::LEVEL, track.level, 0),
        // `80` is 0 dB, so the notch is where it neither cuts nor boosts.
        Control::Trim => value(u16::from(track.trim), 0xFF, true),
        Control::Groove => {
            let grooves: Vec<_> = vt_model::groove::GrooveId::all().collect();
            let playing = track.groove.unwrap_or(song.groove);
            list(
                grooves.iter().map(|groove| groove.name().to_owned()),
                grooves
                    .iter()
                    .position(|groove| *groove == playing)
                    .unwrap_or(0),
            )
        }
        Control::Choke => list(
            std::iter::once("Off".to_owned()).chain((1..=CHOKE_GROUPS).map(|g| g.to_string())),
            usize::from(track.choke.unwrap_or(0)),
        ),
        Control::Group => list(
            std::iter::once("Off".to_owned()).chain((1..=MAX_GROUPS).map(|g| format!("G{g}"))),
            usize::from(track.group.unwrap_or(0)),
        ),
        Control::GateLength => value(
            u16::from(vt_model::settings::gate_percent(track.gate_length())) - 1,
            99,
            false,
        ),
        Control::Speed => list(
            Speed::ALL.iter().map(|speed| speed.name().to_owned()),
            usize::from(track.speed.index()),
        ),
        Control::Reset => value(track.reset.unwrap_or(0), MAX_TRACK_RESET, false),
        Control::Glide => value(u16::from(track.glide), 0xFF, false),
        Control::FilterType => list(
            FilterType::ALL
                .iter()
                .map(|filter| filter.name().to_owned()),
            FilterType::ALL
                .iter()
                .position(|filter| *filter == track.filter)
                .unwrap_or(0),
        ),
        Control::FilterMode => {
            let modes = track.filter.modes();
            if modes.is_empty() {
                return None;
            }
            let playing = track.filter.plays(track.filter_mode);
            list(
                modes.iter().map(|mode| mode.name().to_owned()),
                modes.iter().position(|mode| *mode == playing).unwrap_or(0),
            )
        }
        Control::Mod(slot, field) => {
            let modulator = &track.mods[slot];
            let (adsr, ahd, drum, lpg, lfo) = (
                &modulator.adsr,
                &modulator.ahd,
                &modulator.drum,
                &modulator.lpg,
                &modulator.lfo,
            );
            let ahd_kind = modulator.kind == ModKind::Ahd;
            let drum_kind = modulator.kind == ModKind::Drum;
            let lpg_kind = modulator.kind == ModKind::Lpg;
            match field {
                // An envelope's curve is a musical list, which leaves the layout as it is.
                ModField::Mode if modulator.kind.is_envelope() => {
                    let curve = match modulator.kind {
                        ModKind::Ahd => ahd.curve,
                        ModKind::Drum => drum.curve,
                        _ => adsr.curve,
                    };
                    list(
                        Curve::ALL.iter().map(|curve| curve.name().to_owned()),
                        Curve::ALL
                            .iter()
                            .position(|each| *each == curve)
                            .unwrap_or(0),
                    )
                }
                // How notes start an LFO, a musical list of five, which leaves the layout as it is.
                ModField::Mode => list(
                    LfoMode::ALL.iter().map(|mode| mode.name().to_owned()),
                    LfoMode::ALL
                        .iter()
                        .position(|mode| *mode == lfo.mode)
                        .unwrap_or(0),
                ),
                // What a slot is, the app's alone.
                ModField::Type | ModField::Polarity | ModField::Reset => return None,
                ModField::Destination => {
                    let choices = SettingId::for_mod(track, slot);
                    let at = modulator
                        .destination
                        .and_then(|to| choices.iter().position(|choice| *choice == to))
                        .map_or(0, |at| at + 1);
                    changing(at, choices.len() + 1)
                }
                ModField::Depth => depth(modulator.depth),
                ModField::Attack if ahd_kind => (Feel::Smooth, time_position(ahd.attack_ms)),
                ModField::Attack if lpg_kind => (Feel::Smooth, time_position(lpg.attack_ms)),
                ModField::Decay if lpg_kind => (Feel::Smooth, time_position(lpg.decay_ms)),
                ModField::Colour => value(u16::from(lpg.colour), 0xFF, false),
                ModField::Attack => (Feel::Smooth, time_position(adsr.attack_ms)),
                ModField::Decay if ahd_kind => (Feel::Smooth, time_position(ahd.decay_ms)),
                ModField::Decay if drum_kind => (Feel::Smooth, time_position(drum.decay_ms)),
                ModField::Peak => (Feel::Smooth, time_position(drum.peak_ms)),
                ModField::Body => value(u16::from(drum.body.min(100)), 100, false),
                ModField::Ratchet => list(
                    (1..=MAX_RATCHET).map(|strikes| strikes.to_string()),
                    usize::from(drum.ratchet.clamp(1, MAX_RATCHET) - 1),
                ),
                ModField::Decay => (Feel::Smooth, time_position(adsr.decay_ms)),
                // Smooth, its value shown as it turns, with `Note` at the bottom.
                ModField::Hold => (Feel::Smooth, hold_position(ahd.hold)),
                ModField::Release => (Feel::Smooth, time_position(adsr.release_ms)),
                ModField::Sustain => value(u16::from(adsr.sustain.min(100)), 100, false),
                ModField::Shape => list(
                    Shape::ALL.iter().map(|shape| shape.name().to_owned()),
                    Shape::ALL
                        .iter()
                        .position(|shape| *shape == lfo.shape)
                        .unwrap_or(0),
                ),
                // Smooth over every kind of rate, its value shown as it turns: a stored knob keeps
                // at most 16 steps, and a note value has 24 (the author's choice, 2026-11-11).
                ModField::Rate => (Feel::Smooth, rate_position(lfo.rate)),
                ModField::Phase => value(lfo.phase.min(359), 359, false),
                ModField::Deform => depth(lfo.deform),
                // Smooth through every fade, out, none and in, its value shown as it turns.
                ModField::Fade => list(
                    Fade::ALL.iter().map(ToString::to_string),
                    Fade::ALL
                        .iter()
                        .position(|fade| *fade == lfo.fade)
                        .unwrap_or(0),
                ),
            }
        }
        Control::Accumulator(field) => {
            let accumulation = track.accumulation;
            let degrees = |at: u8| {
                list(
                    (1..=MOST_DEGREES).map(|degrees| degrees.to_string()),
                    usize::from(at.saturating_sub(1)),
                )
            };
            match field {
                AccumulatorField::Up => degrees(accumulation.up),
                AccumulatorField::Down => degrees(accumulation.down),
                AccumulatorField::Moves => list(
                    Moves::ALL.iter().map(|moves| moves.name().to_owned()),
                    Moves::ALL
                        .iter()
                        .position(|moves| *moves == accumulation.moves)
                        .unwrap_or(0),
                ),
                AccumulatorField::AtLimit => list(
                    AtLimit::ALL.iter().map(|at| at.name().to_owned()),
                    AtLimit::ALL
                        .iter()
                        .position(|at| *at == accumulation.at_limit)
                        .unwrap_or(0),
                ),
                AccumulatorField::Adds => list(
                    Adds::ALL.iter().map(|adds| adds.name().to_owned()),
                    Adds::ALL
                        .iter()
                        .position(|adds| *adds == accumulation.adds)
                        .unwrap_or(0),
                ),
                AccumulatorField::Reset => {
                    let resets: Vec<Reset> = Reset::all().collect();
                    list(
                        resets.iter().map(|reset| reset.name()),
                        resets
                            .iter()
                            .position(|reset| *reset == accumulation.reset)
                            .unwrap_or(0),
                    )
                }
            }
        }
        Control::Arpeggio(field) => {
            let arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => list(
                    ArpRate::ALL.iter().map(|rate| rate.name().to_owned()),
                    ArpRate::ALL
                        .iter()
                        .position(|rate| *rate == arpeggio.rate)
                        .unwrap_or(0),
                ),
                ArpField::Mode => list(
                    ArpMode::ALL.iter().map(|mode| mode.name().to_owned()),
                    ArpMode::ALL
                        .iter()
                        .position(|mode| *mode == arpeggio.mode)
                        .unwrap_or(0),
                ),
                ArpField::Octaves => list(
                    (1..=MOST_OCTAVES).map(|octaves| octaves.to_string()),
                    usize::from(arpeggio.octaves.saturating_sub(1)),
                ),
                ArpField::Retrig => return None,
            }
        }
        Control::Insert(slot) => list(
            Insert::ALL.iter().map(|insert| insert.name().to_owned()),
            Insert::ALL
                .iter()
                .position(|insert| insert.name() == track.inserts[slot].name())
                .unwrap_or(0),
        ),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            let set = insert.value(param);
            if found.choices.is_empty() {
                let (id, _) = control.value_on(track)?;
                registry(id, set, 0)
            } else {
                list(names(found.choices), usize::from(set))
            }
        }
        // The mixer's, which no device has.
        Control::Mute | Control::Solo | Control::Limiter => return None,
    };
    Some(found)
}

/// One knob, for a control on this track, as its kind has it, or nothing for a hole.
pub fn knob(control: Control, track: &Track, song: &Song) -> Option<LiveControl> {
    let (feel, position) = feel(control, track, song)?;
    Some(LiveControl {
        name: name(control, track),
        feel,
        position,
        colour: family(control, track).scheme(),
    })
}

/// One knob or button as a layout places it, with its slot's colour and turn where it gives
/// them.
pub fn placed(
    control: Control,
    setting: &Setting,
    track: &Track,
    song: &Song,
) -> Option<LiveControl> {
    let mut knob = knob(control, track, song)?;
    if let Some(family) = setting.family {
        knob.colour = family.scheme();
    }
    match setting.turn {
        Some(Turn::Smooth) if knob.feel != Feel::Switch => knob.feel = Feel::Smooth,
        Some(Turn::CentreNotch) if knob.feel != Feel::Switch => knob.feel = Feel::CentreNotch,
        _ => {}
    }
    Some(knob)
}

/// Where a device's settings are on the Roto: its knobs and its buttons, each a page of eight
/// after another, holes kept, from its layout alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// Whether it's shown as one of vimtracker's stored setups, with its buttons.
    pub stored: bool,
    pub knobs: Vec<Option<(Control, Setting)>>,
    pub buttons: Vec<Option<(Control, Setting)>>,
}

/// A device's placement: stored, its knobs and buttons as its layout has them, if it's a kind
/// that's stored and vimtracker's setups are on the Roto (`stored_on_roto`); otherwise live, its
/// buttons moved onto knobs.
pub fn placement(spec: &DeviceSpec, track: &Track, stored_on_roto: bool) -> Placement {
    let layout = layouts::layout_of(spec, track);
    let resolve = |setting: &Setting| {
        layouts::resolve(setting, spec, track).map(|control| (control, *setting))
    };
    if layout.stored && stored_on_roto {
        Placement {
            stored: true,
            knobs: layout
                .layout
                .knobs()
                .into_iter()
                .map(|slot| slot.and_then(resolve))
                .collect(),
            buttons: layout
                .layout
                .buttons()
                .into_iter()
                .map(|slot| slot.and_then(resolve))
                .collect(),
        }
    } else {
        Placement {
            stored: false,
            knobs: layout
                .layout
                .live_knobs()
                .into_iter()
                .map(|slot| slot.and_then(|placed| resolve(placed.setting)))
                .collect(),
            buttons: Vec::new(),
        }
    }
}

/// What a device is called on the Roto, and the key its hash is made from: a machine by its name,
/// a slot by its number and effect, and the rest as the track page names them. A stored device's
/// key is its stored setup's, in vimtracker's namespace, the same for every device of its kind.
fn identity(
    page: Page,
    spec: &DeviceSpec,
    track: &Track,
    slot: usize,
    stored: bool,
) -> (String, String) {
    let lower = |text: &str| text.to_ascii_lowercase().replace(' ', "-");
    let (name, key) = match page {
        Page::Fx => {
            let insert = track.inserts[slot];
            let name = if insert == Insert::Empty {
                format!("{} Empty", slot + 1)
            } else {
                format!("{} {}", slot + 1, insert.name())
            };
            (
                name,
                format!("vimtracker/fx{}/{}", slot + 1, lower(insert.name())),
            )
        }
        _ if spec.identity == "Machine" => (
            track.machine.name().to_owned(),
            format!("vimtracker/machine/{}", lower(track.machine.name())),
        ),
        // A mod slot by its number and its type, as an FX slot is by its digit and its effect.
        _ if matches!(spec.title[0], Some(Control::Mod(..))) => {
            let Some(Control::Mod(at, _)) = spec.title[0] else {
                unreachable!()
            };
            let kind = track.mods[at].kind.name();
            (
                format!("M{} {kind}", at + 1),
                format!("vimtracker/mod{}/{}", at + 1, lower(kind)),
            )
        }
        _ => (
            spec.identity.clone(),
            format!("vimtracker/{}", lower(&spec.identity)),
        ),
    };
    if stored {
        let kind = layouts::layout_of(spec, track).kind;
        return (name, format!("{NAMESPACE}{kind}"));
    }
    (name, key)
}

/// Whether a device is switched in: an FX slot that's filled and not bypassed, a mod slot that's
/// in, and every other device.
fn enabled(control: Option<Control>, track: &Track) -> bool {
    match control {
        Some(Control::Insert(slot)) => {
            track.inserts[slot] != Insert::Empty && !track.bypassed[slot]
        }
        Some(control) => crate::track_page::modulator_bypassed(control, track) != Some(true),
        None => true,
    }
}

/// One device of a page as the Roto shows it, stored with its buttons if it's a kind that's
/// stored and vimtracker's setups are on the Roto, and live otherwise.
pub fn live_device(
    page: Page,
    slot: usize,
    spec: &DeviceSpec,
    track: &Track,
    song: &Song,
    stored_on_roto: bool,
) -> LiveDevice {
    let placement = placement(spec, track, stored_on_roto);
    let (name, key) = identity(page, spec, track, slot, placement.stored);
    let shown = |slots: Vec<Option<(Control, Setting)>>| {
        slots
            .into_iter()
            .map(|slot| slot.and_then(|(control, setting)| placed(control, &setting, track, song)))
            .collect()
    };
    LiveDevice {
        key,
        name: Label::new(&name),
        enabled: enabled(spec.title[0], track),
        stored: placement.stored,
        // An empty slot has nothing on the Roto, as its effect is chosen in the app: a page of
        // nothing.
        controls: if placement.knobs.is_empty() {
            vec![None; CONTROLS]
        } else {
            shown(placement.knobs)
        },
        switches: shown(placement.buttons),
    }
}

/// A track's devices as the Roto's plugin list shows them, in the order the design note's §3 has:
/// the Sound page's, then the FX page's slots, then the Mod page's.
pub fn live_devices(track: &Track, song: &Song, stored_on_roto: bool) -> Vec<LiveDevice> {
    track_devices(track)
        .into_iter()
        .map(|(page, slot, spec)| live_device(page, slot, &spec, track, song, stored_on_roto))
        .collect()
}

/// A track's devices in the order the Roto's list has them, each with its page and its place
/// on that page.
pub fn track_devices(track: &Track) -> Vec<(Page, usize, DeviceSpec)> {
    [Page::Sound, Page::Fx, Page::Mod]
        .into_iter()
        .flat_map(|page| {
            page.devices(track)
                .into_iter()
                .enumerate()
                .map(move |(slot, spec)| (page, slot, spec))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The setting on the knob the Roto numbers `index`, from 1 across the pages, of a track's
/// `device`th device, if there's one there.
pub fn control_at(
    track: &Track,
    device: usize,
    index: u16,
    stored_on_roto: bool,
) -> Option<Control> {
    let (_, _, spec) = track_devices(track).into_iter().nth(device)?;
    let at = usize::from(index.checked_sub(1)?);
    let placement = placement(&spec, track, stored_on_roto);
    placement
        .knobs
        .get(at)
        .copied()
        .flatten()
        .map(|(control, _)| control)
}

/// The setting on the button the Roto numbers `index`, from 1 across the pages, of a track's
/// `device`th device, if it's stored and has one there.
pub fn button_at(
    track: &Track,
    device: usize,
    index: u16,
    stored_on_roto: bool,
) -> Option<Control> {
    let (_, _, spec) = track_devices(track).into_iter().nth(device)?;
    let at = usize::from(index.checked_sub(1)?);
    let placement = placement(&spec, track, stored_on_roto);
    placement
        .buttons
        .get(at)
        .copied()
        .flatten()
        .map(|(control, _)| control)
}

/// One of each kind of device, each as a song, the track it's on, its page and its place there:
/// every machine, every effect in each of its types, the filter, each kind of track's output,
/// the Play device and each modulator. The stored setups are made from these, and the tests go
/// through them.
pub fn every_kind_of_device() -> Vec<(Song, usize, Page, usize)> {
    let mut base = Song::new(1);
    base.tracks[0].filter = FilterType::ALL
        .iter()
        .copied()
        .find(|filter| filter.has_modes())
        .unwrap_or_default();
    let mut found = Vec::new();
    for machine in Machine::ALL {
        let mut song = base.clone();
        song.tracks[0].machine = machine;
        found.push((song, 0, Page::Sound, 0));
    }
    for slot in 1..Page::Sound.devices(&base.tracks[0]).len() {
        found.push((base.clone(), 0, Page::Sound, slot));
    }
    for slot in 0..Page::Mod.devices(&base.tracks[0]).len() {
        found.push((base.clone(), 0, Page::Mod, slot));
    }
    // Each type of mod slot a new track hasn't, in `Mod 3`.
    let unused = ModKind::ALL
        .into_iter()
        .filter(|kind| base.tracks[0].mods.iter().all(|slot| slot.kind != *kind));
    for kind in unused {
        let mut song = base.clone();
        song.tracks[0].mods[2].kind = kind;
        let devices = Page::Mod.devices(&song.tracks[0]);
        let at = devices.iter().position(|device| device.identity == "Mod 3");
        found.push((song, 0, Page::Mod, at.unwrap_or_default()));
    }
    for insert in Insert::ALL {
        let types = insert
            .type_param()
            .map_or(1, |at| insert.params()[at].choices.len());
        for kind in 0..types {
            let mut song = base.clone();
            song.tracks[0].inserts[0] = match insert.type_param() {
                Some(at) => insert.with_value(at, kind as u8),
                None => insert,
            };
            found.push((song, 0, Page::Fx, 0));
        }
    }
    for (at, track) in base.tracks.iter().enumerate().skip(1) {
        if !track.is_sound() {
            found.push((base.clone(), at, Page::Sound, 0));
        }
    }
    found
}

/// vimtracker's stored setups: one for each kind of device that's stored, made from its layout
/// as a device of that kind is shown.
pub fn stored_setups() -> Vec<OwnSetup> {
    let mut setups: Vec<OwnSetup> = Vec::new();
    for (song, at, page, slot) in every_kind_of_device() {
        let track = &song.tracks[at];
        let spec = &page.devices(track)[slot];
        let layout = layouts::layout_of(spec, track);
        let key = format!("{NAMESPACE}{}", layout.kind);
        if !layout.stored || setups.iter().any(|setup| setup.key == key) {
            continue;
        }
        let device = live_device(page, slot, spec, track, &song, true);
        let name = layout.layout.pages.first().map_or("", |first| first.name);
        setups.push(OwnSetup::of(&device, name));
    }
    setups
}

/// What switches a track's `device`th device in and out, as its enabled state on the Roto: an
/// FX slot's bypass, or a mod slot's own switch. Other devices are always in.
pub fn device_switch(track: &Track, device: usize) -> Option<Control> {
    let (_, _, spec) = track_devices(track).into_iter().nth(device)?;
    match spec.title[0]? {
        control @ Control::Insert(_) => Some(control),
        control => crate::track_page::modulator_bypassed(control, track).map(|_| control),
    }
}

/// The song's tracks as the Roto's track list shows them, which PLUGIN mode's FUNC chooses from:
/// a sound track by its number and name, as the grid's header has it, a fixed track by its
/// title; each in its group's colour, and a group marked as one.
pub fn live_tracks(song: &Song) -> Vec<LiveTrack> {
    song.tracks
        .iter()
        .enumerate()
        .map(|(at, track)| {
            let name = match track.kind {
                TrackKind::Sound if track.name.is_empty() => format!("Track {:02}", at + 1),
                TrackKind::Sound => format!("{:02} {}", at + 1, track.name),
                kind => kind.title(),
            };
            let group = match track.kind {
                TrackKind::Group(group) => Some(group),
                _ => track.group,
            };
            let colour = group.map_or(theme::TEXT, theme::group);
            LiveTrack {
                name: Label::new(&name),
                colour: scheme_of(colour),
                grouped: matches!(track.kind, TrackKind::Group(_)),
            }
        })
        .collect()
}

/// The Roto's colour scheme nearest one of the app's colours.
fn scheme_of(colour: Color) -> u8 {
    match colour {
        Color::Rgb(red, green, blue) => colour::nearest(Rgb::new(red, green, blue)),
        _ => 0,
    }
}
