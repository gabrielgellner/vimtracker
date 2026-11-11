//! The Mod page: a rack of half-width devices, each envelope beside the LFO of its number, and the
//! accumulator beside the arpeggio under them (the track pages study,
//! `docs/designs/track-pages.md`, §2.2). An envelope or an LFO has its destination in its title,
//! as what it's for, and each states its grid here, by what its settings mean.

use vt_model::constants::TRACK_ENVELOPES;
use vt_model::grid::Grid;
use vt_model::lfo::LFOS_PER_TRACK;
use vt_model::song::Track;

use super::{
    AccumulatorField, ArpField, Control, DEVICE_COLUMNS, DeviceSpec, EnvelopeField, LfoField,
};

/// An envelope's settings, as its grid places them: its times down the left, and its levels down
/// the right.
const ENVELOPE_SETTINGS: [EnvelopeField; 6] = [
    EnvelopeField::Attack,
    EnvelopeField::Decay,
    EnvelopeField::Release,
    EnvelopeField::Sustain,
    EnvelopeField::Depth,
    EnvelopeField::Reset,
];
const ENVELOPE_GRID: [[Option<usize>; 3]; 2] =
    [[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]];

/// An LFO's settings: what it plays down the left, and where it starts down the right, with a hole
/// under `Retrig` for the `Fade` that #86 brings.
const LFO_SETTINGS: [LfoField; 5] = [
    LfoField::Shape,
    LfoField::Rate,
    LfoField::Depth,
    LfoField::Phase,
    LfoField::Retrig,
];
const LFO_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]];

/// The accumulator's settings: how far it goes down the left, and what it does there and when
/// down the right.
const ACCUMULATOR_SETTINGS: [AccumulatorField; 6] = [
    AccumulatorField::Up,
    AccumulatorField::Down,
    AccumulatorField::Moves,
    AccumulatorField::AtLimit,
    AccumulatorField::Adds,
    AccumulatorField::Reset,
];
const ACCUMULATOR_GRID: [[Option<usize>; 3]; 2] =
    [[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]];

/// The arpeggio's settings: how it steps down the left, and how far and how it starts each note
/// down the right.
const ARPEGGIO_SETTINGS: [ArpField; 4] = [
    ArpField::Rate,
    ArpField::Mode,
    ArpField::Octaves,
    ArpField::Retrig,
];
const ARPEGGIO_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), None], [Some(2), Some(3), None]];

/// A title with one control, over the grid's first column.
fn titled(control: Control) -> [Option<Control>; DEVICE_COLUMNS] {
    let mut title = [None; DEVICE_COLUMNS];
    title[0] = Some(control);
    title
}

/// An envelope as a half-width device, `Env 1` to `Env 3`, with its destination in its title.
fn envelope_device(slot: usize) -> DeviceSpec {
    let title = titled(Control::Envelope(slot, EnvelopeField::Destination));
    let grid = Grid::from_columns(&ENVELOPE_GRID);
    DeviceSpec::new(format!("Env {}", slot + 1), title, grid, |at| {
        Control::Envelope(slot, ENVELOPE_SETTINGS[at])
    })
    .half()
}

/// An LFO as a half-width device, `LFO 1` to `LFO 3`, with its destination in its title.
fn lfo_device(slot: usize) -> DeviceSpec {
    let title = titled(Control::Lfo(slot, LfoField::Destination));
    let grid = Grid::from_columns(&LFO_GRID);
    DeviceSpec::new(format!("LFO {}", slot + 1), title, grid, |at| {
        Control::Lfo(slot, LFO_SETTINGS[at])
    })
    .half()
}

/// The accumulator as a half-width device, with nothing in its title.
fn accumulator_device() -> DeviceSpec {
    let grid = Grid::from_columns(&ACCUMULATOR_GRID);
    DeviceSpec::new("Accumulator", [None; DEVICE_COLUMNS], grid, |at| {
        Control::Accumulator(ACCUMULATOR_SETTINGS[at])
    })
    .half()
}

/// The arpeggio as a half-width device, with nothing in its title.
fn arpeggio_device() -> DeviceSpec {
    let grid = Grid::from_columns(&ARPEGGIO_GRID);
    DeviceSpec::new("Arpeggio", [None; DEVICE_COLUMNS], grid, |at| {
        Control::Arpeggio(ARPEGGIO_SETTINGS[at])
    })
    .half()
}

/// The Mod page's devices in the order they're read: each envelope, then the LFO of its number
/// beside it, then the accumulator and the arpeggio beside it. A return, a group or the master
/// has no notes for an accumulator or an arpeggio to play, so its page ends with the LFOs.
pub(super) fn devices(track: &Track) -> Vec<DeviceSpec> {
    let mut devices = Vec::new();
    for slot in 0..TRACK_ENVELOPES.max(LFOS_PER_TRACK) {
        if slot < TRACK_ENVELOPES {
            devices.push(envelope_device(slot));
        }
        if slot < LFOS_PER_TRACK {
            devices.push(lfo_device(slot));
        }
    }
    if track.is_sound() {
        devices.extend([accumulator_device(), arpeggio_device()]);
    }
    devices
}
