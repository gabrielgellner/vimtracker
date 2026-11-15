//! The key a control goes by: what the Roto's layouts name it by and a setting's view carries.

use vt_model::action::TrackControl;
use vt_model::modulation::ModField;
use vt_model::song::Track;
use vt_model::steps::STEPS;

use super::{AccumulatorField, ArpField, Control, LaneField};

/// The macros' keys, by their place, as the registry has them.
const MACRO_KEYS: [&str; vt_model::macros::MACROS] = [
    "macro1", "macro2", "macro3", "macro4", "macro5", "macro6", "macro7", "macro8",
];

/// Steps' values' keys, by their place.
pub(crate) const STEP_KEYS: [&str; STEPS] = [
    "step1", "step2", "step3", "step4", "step5", "step6", "step7", "step8", "step9", "step10",
    "step11", "step12", "step13", "step14", "step15", "step16",
];

/// The key a layout names a control by on this track: a machine's setting and an effect's by the
/// key songs store them under, and the track's own settings by a name of their own.
pub fn key(control: Control, track: &Track) -> &'static str {
    match control {
        Control::Gen { param, .. } => param.key(),
        Control::NewGenerator(_) => "empty",
        Control::Machine => "machine",
        Control::Sample => "sample",
        Control::Tune => "tune",
        Control::Cents => "cents",
        Control::Setting(_) => control
            .setting_on(track)
            .map_or("setting", |id| id.entry().key),
        Control::InsertParam(slot, param) => track.inserts[slot]
            .params()
            .get(param)
            .map_or("setting", |found| found.key),
        Control::Insert(_) => "effect",
        Control::Macro(at) => MACRO_KEYS[at.min(MACRO_KEYS.len() - 1)],
        Control::Mute => "mute",
        Control::Solo => "solo",
        Control::Base(which) => match which {
            TrackControl::Cutoff => "cutoff",
            TrackControl::Resonance => "resonance",
            TrackControl::Drive => "drive",
            TrackControl::Punch => "punch",
            TrackControl::LowCut => "lowcut",
            TrackControl::HighCut => "highcut",
            TrackControl::Pan => "pan",
            TrackControl::SendA => "senda",
            TrackControl::SendB => "sendb",
            TrackControl::Pitch => "pitch",
            TrackControl::VelocityDelta => "velocity",
            TrackControl::Quantise => "quantise",
        },
        Control::Level => "level",
        Control::Trim => "trim",
        Control::Groove => "groove",
        Control::Choke => "choke",
        Control::Group => "group",
        Control::Role => "role",
        Control::GateLength => "gate",
        Control::Speed => "speed",
        Control::Reset => "reset",
        Control::Glide => "glide",
        Control::Limiter => "limiter",
        Control::Safe => "safe",
        Control::FilterType => "type",
        Control::FilterMode => "mode",
        Control::Mod(_, field) => match field {
            ModField::Type => "type",
            ModField::Mode => "mode",
            ModField::Polarity => "polarity",
            ModField::Destination => "dest",
            ModField::Depth => "depth",
            ModField::Attack => "attack",
            ModField::Decay => "decay",
            ModField::Sustain => "sustain",
            ModField::Release => "release",
            ModField::Reset => "reset",
            ModField::Hold => "hold",
            ModField::Peak => "peak",
            ModField::Body => "body",
            ModField::Ratchet => "ratchet",
            ModField::Colour => "colour",
            ModField::Shape => "shape",
            ModField::Rate => "rate",
            ModField::Phase => "phase",
            ModField::Deform => "deform",
            ModField::Fade => "fade",
            ModField::Length => "length",
            ModField::Start => "start",
            ModField::Loop => "loop",
            ModField::Mutate => "mutate",
            ModField::Smooth => "smooth",
            ModField::Step(at) => STEP_KEYS[usize::from(at).min(STEPS - 1)],
            ModField::Low => "low",
            ModField::High => "high",
        },
        Control::Accumulator(field) => match field {
            AccumulatorField::Up => "up",
            AccumulatorField::Down => "down",
            AccumulatorField::Moves => "moves",
            AccumulatorField::AtLimit => "atlimit",
            AccumulatorField::Adds => "adds",
            AccumulatorField::Reset => "reset",
        },
        Control::Arpeggio(field) => match field {
            ArpField::Rate => "rate",
            ArpField::Mode => "mode",
            ArpField::Octaves => "octaves",
            ArpField::Retrig => "retrig",
        },
        Control::Lane(_, LaneField::Length) => "length",
        Control::Lane(_, LaneField::Rate) => "rate",
    }
}
