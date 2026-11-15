//! The Mod page's devices: the track's six mod slots, half-width, down two columns, and the
//! accumulator beside the arpeggio under them. Each type's title and grid are its spec's, in the
//! model (`vt_model::modulation::MOD_SPECS`).

use vt_model::grid::Grid;
use vt_model::modulation::{MOD_SLOTS, ModSlot};
use vt_model::song::Track;

use super::{DEVICE_COLUMNS, DeviceSpec};
use crate::controls::{AccumulatorField, ArpField, Control};

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

/// A mod slot as a half-width device, `Mod 1` to `Mod 6`, with its switch, then its type's title
/// and grid as its spec has them ([`ModKind::spec`]): its type, its type's mode if it has one,
/// Steps' loop, its polarity if it has one to choose and its destination, in that order for `h`
/// and `l`.
fn mod_device(slot: usize, modulator: &ModSlot) -> DeviceSpec {
    let spec = modulator.kind.spec();
    let mut title = [None; DEVICE_COLUMNS];
    for (place, field) in title.iter_mut().zip(spec.title) {
        *place = Some(Control::Mod(slot, *field));
    }
    let device = DeviceSpec::new(format!("Mod {}", slot + 1), title, spec.grid, |at| {
        Control::Mod(slot, spec.fields[at])
    });
    DeviceSpec {
        switchable: true,
        ..device.half()
    }
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

/// How many bands of two the slots take, a column of them on either side.
const SLOT_BANDS: usize = MOD_SLOTS / 2;

/// The Mod page's devices in the order they're read: each slot of the left column beside the one
/// three after it on the right, `Mod 1`, `Mod 4`, `Mod 2`, `Mod 5`, `Mod 3`, `Mod 6`, then the
/// accumulator and the arpeggio beside it. A return, a group or the master has no notes for an
/// accumulator or an arpeggio to play, so its page ends with the slots.
pub(super) fn devices(track: &Track) -> Vec<DeviceSpec> {
    let mut devices = Vec::new();
    for band in 0..SLOT_BANDS {
        for slot in [band, band + SLOT_BANDS] {
            devices.push(mod_device(slot, &track.mods[slot]));
        }
    }
    if track.is_sound() {
        devices.extend([accumulator_device(), arpeggio_device()]);
    }
    devices
}

/// Whether the mod slot a control is in is switched out, if it's in one.
pub(crate) fn modulator_bypassed(control: Control, track: &Track) -> Option<bool> {
    match control {
        Control::Mod(slot, _) => Some(track.mods[slot].bypassed),
        _ => None,
    }
}
