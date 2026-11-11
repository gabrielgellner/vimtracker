//! The Sound page: the machine, its filter and the track's output, in rows that depend on what the
//! track is and what it plays.

use vt_model::action::TrackControl;

use super::Control;
use super::controls::base;

/// The Sound page of a track playing a sample, top to bottom, with output on the right: its
/// `Sample`, then the sampler's `Slice`, `Start`, `Length` and `Play`, as a synth's settings are,
/// with the filter below them as on a synth with more than four.
pub const SAMPLE_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Sample, Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::Setting(4), Control::Group],
    &[Control::FilterType, base(TrackControl::Pitch)],
    &[Control::FilterMode, Control::Groove],
    &[base(TrackControl::Cutoff), Control::Choke],
    &[base(TrackControl::Resonance), Control::GateLength],
    &[base(TrackControl::Drive), Control::Speed],
    &[base(TrackControl::LowCut), Control::Reset],
    &[base(TrackControl::HighCut), Control::Glide],
];

/// The Sound page of a track playing a synth with four settings: its settings down the left, and
/// output on the right, which is a row longer.
pub const SYNTH_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::FilterType, Control::Group],
    &[Control::FilterMode, base(TrackControl::Pitch)],
    &[base(TrackControl::Cutoff), Control::Groove],
    &[base(TrackControl::Resonance), Control::Choke],
    &[base(TrackControl::Drive), Control::GateLength],
    &[base(TrackControl::LowCut), Control::Speed],
    &[base(TrackControl::HighCut), Control::Reset],
    &[Control::Glide],
];

/// The same for a synth with more than four, whose later settings carry on down the left.
pub const SYNTH8_ROWS: [&[Control]; 16] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::Setting(4), Control::Group],
    &[Control::Setting(5), base(TrackControl::Pitch)],
    &[Control::Setting(6), Control::Groove],
    &[Control::Setting(7), Control::Choke],
    &[Control::FilterType, Control::GateLength],
    &[Control::FilterMode, Control::Speed],
    &[base(TrackControl::Cutoff), Control::Reset],
    &[base(TrackControl::Resonance), Control::Glide],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];

/// The same for a synth with three, whose fifth row has only its `Send B`, on the right.
pub const SYNTH3_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[base(TrackControl::SendB)],
    &[Control::FilterType, Control::Group],
    &[Control::FilterMode, base(TrackControl::Pitch)],
    &[base(TrackControl::Cutoff), Control::Groove],
    &[base(TrackControl::Resonance), Control::Choke],
    &[base(TrackControl::Drive), Control::GateLength],
    &[base(TrackControl::LowCut), Control::Speed],
    &[base(TrackControl::HighCut), Control::Reset],
    &[Control::Glide],
];

/// The Sound page of a return, which has no machine or filter.
pub const RETURN_ROWS: [&[Control]; 3] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
];

/// The Sound page of a group, which sends to the returns as well.
pub const GROUP_ROWS: [&[Control]; 5] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
    &[base(TrackControl::SendA)],
    &[base(TrackControl::SendB)],
];

/// The Sound page of the master.
pub const MASTER_ROWS: [&[Control]; 2] = [&[Control::Trim], &[Control::Level]];
