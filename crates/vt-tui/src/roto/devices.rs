//! vimtracker's devices as the Roto shows them live: each device of a track, as the track page
//! lays it out (`DeviceSpec`), becomes a device of knobs, each with its setting's name, the feel
//! its kind gives it, where it's set, and its family's colour. Pure: a track and its song in, the
//! Roto's description out, so it's tested with no Roto at all.
//!
//! **Knob order** is row by row through the device's grid, holes kept, then its title's controls
//! (the author's decision of 2026-11-11, the design note's §3): knobs 1 to 4 are the grid's top
//! row and 5 to 8 its middle, and the second page is its bottom row and then the title, so a
//! column's settings are always four knobs apart. A page with nothing on it at the end is left
//! off.
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
use vt_model::grid::GRID_ROWS;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{MAX_LFO_DEPTH, Rate, Shape};
use vt_model::settings::{Kind, SettingId, Units};
use vt_model::song::{FilterType, Machine, Song, Track, TrackKind};
use vt_model::speed::Speed;
use vt_roto::Label;
use vt_roto::bytes::Rgb;
use vt_roto::colour;
use vt_roto::live::{Feel, LiveControl, LiveDevice, LiveTrack, position_at, position_of};
use vt_roto::sysex::{CONTROLS, MAX_STEPS};

use ratatui::style::Color;

use crate::theme;
use crate::track_page::{
    AccumulatorField, ArpField, Control, DEVICE_COLUMNS, DeviceSpec, EnvelopeField, LfoField, Page,
};
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
    /// Modulation: what an LFO, an envelope, the accumulator or the arpeggio does. Mauve.
    Modulation,
    /// Character: drive and a machine's or an effect's own colour of sound. Peach.
    Character,
    /// Which one: a machine, an effect, a sample, a type, a group. The text's colour.
    Choice,
}

impl Family {
    /// The family's colour, from the app's palette.
    pub fn colour(self) -> Color {
        match self {
            Self::Level => theme::GREEN,
            Self::Tone => theme::BLUE,
            Self::Space => theme::TEAL,
            Self::Time => theme::YELLOW,
            Self::Modulation => theme::MAUVE,
            Self::Character => theme::PEACH,
            Self::Choice => theme::TEXT,
        }
    }

    /// The Roto's colour scheme nearest the family's colour.
    pub fn scheme(self) -> u8 {
        scheme_of(self.colour())
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
            | Units::Ceiling => Self::Level,
            Units::Hertz | Units::Boost | Units::Q | Units::AcidCutoff | Units::Vowel => Self::Tone,
            Units::Pan => Self::Space,
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
            | Units::Glide => Self::Time,
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
        Control::Level => Some(SettingId::VOL),
        Control::InsertParam(..) => control.value_on(track).map(|(id, _)| id),
        _ => None,
    }
}

/// A control's family.
pub fn family(control: Control, track: &Track) -> Family {
    if let Some(id) = registry_setting(control, track) {
        let entry = id.entry();
        // A resonance and an effect's cuts are the filter's kind, though each is a share like
        // any other, and a rate or a time chosen from note divisions is a time.
        return match (entry.kind, entry.key) {
            (_, "res" | "resonance" | "lowcut" | "highcut") => Family::Tone,
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
        | Control::Lfo(_, LfoField::Rate)
        | Control::Envelope(
            _,
            EnvelopeField::Attack | EnvelopeField::Decay | EnvelopeField::Release,
        )
        | Control::Arpeggio(ArpField::Rate) => Family::Time,
        Control::Envelope(_, EnvelopeField::Sustain) => Family::Level,
        Control::Lfo(..)
        | Control::Envelope(..)
        | Control::Accumulator(_)
        | Control::Arpeggio(_) => Family::Modulation,
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

/// A depth either side of none, as an LFO's or an envelope's.
fn depth(depth: i16) -> (Feel, u16) {
    let top = MAX_LFO_DEPTH;
    value(
        (depth.clamp(-top, top) + top) as u16,
        (2 * top) as u16,
        true,
    )
}

/// A knob's feel and where it's set, for a control on this track, or nothing if the track
/// hasn't the setting, as a fixed track hasn't `Pitch`, or it has nothing to choose, as a filter
/// type without modes has for `Mode`.
fn feel(control: Control, track: &Track, song: &Song) -> Option<(Feel, u16)> {
    if !control.exists_on(track) {
        return None;
    }
    if let Some(on) = toggle_on(control, track, song) {
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
        Control::Level => registry(SettingId::VOL, track.level, track.fine(SettingId::VOL)),
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
        Control::Lfo(slot, field) => {
            let lfo = &track.lfos[slot];
            match field {
                LfoField::Destination => {
                    let choices = SettingId::for_lfo(track, slot);
                    let at = lfo
                        .destination
                        .and_then(|to| choices.iter().position(|choice| *choice == to))
                        .map_or(0, |at| at + 1);
                    changing(at, choices.len() + 1)
                }
                LfoField::Shape => list(
                    Shape::ALL.iter().map(|shape| shape.name().to_owned()),
                    Shape::ALL
                        .iter()
                        .position(|shape| *shape == lfo.shape)
                        .unwrap_or(0),
                ),
                LfoField::Rate => (Feel::Smooth, rate_position(lfo.rate)),
                LfoField::Depth => depth(lfo.depth),
                LfoField::Phase => value(lfo.phase.min(359), 359, false),
                // A switch, which `toggle_on` has already given.
                LfoField::Retrig => return None,
            }
        }
        Control::Envelope(slot, field) => {
            let envelope = &track.envelopes[slot];
            match field {
                EnvelopeField::Destination => {
                    let choices = SettingId::for_envelope(track);
                    let at = envelope
                        .destination
                        .and_then(|to| choices.iter().position(|choice| *choice == to))
                        .map_or(0, |at| at + 1);
                    changing(at, choices.len() + 1)
                }
                EnvelopeField::Attack => (Feel::Smooth, time_position(envelope.attack_ms)),
                EnvelopeField::Decay => (Feel::Smooth, time_position(envelope.decay_ms)),
                EnvelopeField::Release => (Feel::Smooth, time_position(envelope.release_ms)),
                EnvelopeField::Sustain => value(u16::from(envelope.sustain.min(100)), 100, false),
                EnvelopeField::Depth => depth(envelope.depth),
                EnvelopeField::Reset => return None,
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

/// One knob, for a control on this track, or nothing for a hole.
pub fn knob(control: Control, track: &Track, song: &Song) -> Option<LiveControl> {
    let (feel, position) = feel(control, track, song)?;
    Some(LiveControl {
        name: name(control, track),
        feel,
        position,
        colour: family(control, track).scheme(),
    })
}

/// A device's controls in knob order: its grid row by row, then its title, holes kept, with an
/// empty last page left off.
pub fn knob_order(spec: &DeviceSpec) -> Vec<Option<Control>> {
    let mut order: Vec<Option<Control>> = (0..GRID_ROWS)
        .flat_map(|row| spec.grid[row])
        .chain(spec.title)
        .collect();
    debug_assert_eq!(order.len(), (GRID_ROWS + 1) * DEVICE_COLUMNS);
    while order.len() > CONTROLS && order[order.len() - CONTROLS..].iter().all(Option::is_none) {
        order.truncate(order.len() - CONTROLS);
    }
    order
}

/// What a device is called on the Roto, and the key its hash is made from: a machine by its name,
/// a slot by its number and effect, and the rest as the track page names them.
fn identity(page: Page, spec: &DeviceSpec, track: &Track, slot: usize) -> (String, String) {
    let lower = |text: &str| text.to_ascii_lowercase().replace(' ', "-");
    match page {
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
        _ => (
            spec.identity.clone(),
            format!("vimtracker/{}", lower(&spec.identity)),
        ),
    }
}

/// Whether a device is switched in: an FX slot that's filled and not bypassed, an envelope or an
/// LFO that's in, and every other device.
fn enabled(control: Option<Control>, track: &Track) -> bool {
    match control {
        Some(Control::Insert(slot)) => {
            track.inserts[slot] != Insert::Empty && !track.bypassed[slot]
        }
        Some(control) => crate::track_page::modulator_bypassed(control, track) != Some(true),
        None => true,
    }
}

/// One device of a page as the Roto shows it.
pub fn live_device(
    page: Page,
    slot: usize,
    spec: &DeviceSpec,
    track: &Track,
    song: &Song,
) -> LiveDevice {
    let (name, key) = identity(page, spec, track, slot);
    LiveDevice {
        key,
        name: Label::new(&name),
        enabled: enabled(spec.title[0], track),
        controls: knob_order(spec)
            .into_iter()
            .map(|control| control.and_then(|control| knob(control, track, song)))
            .collect(),
    }
}

/// A track's devices as the Roto's plugin list shows them, in the order the design note's §3 has:
/// the Sound page's, then the FX page's slots, then the Mod page's.
pub fn live_devices(track: &Track, song: &Song) -> Vec<LiveDevice> {
    track_devices(track)
        .into_iter()
        .map(|(page, slot, spec)| live_device(page, slot, &spec, track, song))
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
pub fn control_at(track: &Track, device: usize, index: u16) -> Option<Control> {
    let (_, _, spec) = track_devices(track).into_iter().nth(device)?;
    let at = usize::from(index.checked_sub(1)?);
    knob_order(&spec).get(at).copied().flatten()
}

/// What switches a track's `device`th device in and out, as its enabled state on the Roto: an
/// FX slot's bypass, or an envelope's or an LFO's own switch. Other devices are always in.
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
