//! The Mod page: a rack of half-width devices, the track's six mod slots down two columns, `Mod 1`
//! to `Mod 3` on the left and `Mod 4` to `Mod 6` on the right, and the accumulator beside the
//! arpeggio under them (the mod slots study, `docs/designs/mod-slots.md`, §7). A slot's title has
//! its type, its type's mode if it has one, and its destination, and a switch, which `b` flips to
//! hear the track without it. Each type states its grid here, by what its settings mean.

use vt_model::action::Action;
use vt_model::grid::Grid;
use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot};
use vt_model::song::Track;

use crate::app::App;

use super::{AccumulatorField, ArpField, Control, DEVICE_COLUMNS, DeviceSpec, ModField};

/// An ADSR's settings, as its grid places them: its times down the left, and its levels down the
/// right.
const ADSR_SETTINGS: [ModField; 6] = [
    ModField::Attack,
    ModField::Decay,
    ModField::Release,
    ModField::Sustain,
    ModField::Depth,
    ModField::Reset,
];
const ADSR_GRID: [[Option<usize>; 3]; 2] =
    [[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]];

/// An AHD's settings: its times down the left, and how far it goes and its reset down the right,
/// with a hole left for a `Delay` before the attack, Vital's DAHD, if one is missed.
const AHD_SETTINGS: [ModField; 5] = [
    ModField::Attack,
    ModField::Hold,
    ModField::Decay,
    ModField::Depth,
    ModField::Reset,
];
const AHD_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]];

/// A drum envelope's settings: its peak, body and decay down the left, and how far it goes and how
/// many times its peak strikes down the right, with a hole. It has no reset, as it always strikes
/// from full.
const DRUM_SETTINGS: [ModField; 5] = [
    ModField::Peak,
    ModField::Body,
    ModField::Decay,
    ModField::Depth,
    ModField::Ratchet,
];
const DRUM_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]];

/// An LFO's settings: what it plays down the left, and where it starts down the right, with a hole
/// under `Phase` for the `Fade` that #86 brings. Whether notes start it is its mode, in the title.
const LFO_SETTINGS: [ModField; 4] = [
    ModField::Shape,
    ModField::Rate,
    ModField::Depth,
    ModField::Phase,
];
const LFO_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [Some(3), None, None]];

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

/// A slot's grid of settings under its type.
fn type_grid(kind: ModKind) -> (&'static [ModField], Grid) {
    match kind {
        ModKind::Adsr => (&ADSR_SETTINGS, Grid::from_columns(&ADSR_GRID)),
        ModKind::Ahd => (&AHD_SETTINGS, Grid::from_columns(&AHD_GRID)),
        ModKind::Drum => (&DRUM_SETTINGS, Grid::from_columns(&DRUM_GRID)),
        ModKind::Lfo => (&LFO_SETTINGS, Grid::from_columns(&LFO_GRID)),
    }
}

/// A mod slot as a half-width device, `Mod 1` to `Mod 6`, with its switch, then its type, its
/// type's mode if it has one, its polarity and its destination in its title, in that order for `h`
/// and `l`.
fn mod_device(slot: usize, modulator: &ModSlot) -> DeviceSpec {
    let kind = modulator.kind;
    let mut title = [None; DEVICE_COLUMNS];
    let fields = [
        ModField::Type,
        ModField::Mode,
        ModField::Polarity,
        ModField::Destination,
    ];
    for (place, field) in title
        .iter_mut()
        .zip(fields.into_iter().filter(|field| field.on(kind)))
    {
        *place = Some(Control::Mod(slot, field));
    }
    let (settings, grid) = type_grid(kind);
    let device = DeviceSpec::new(format!("Mod {}", slot + 1), title, grid, |at| {
        Control::Mod(slot, settings[at])
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

impl App {
    /// `b`: switch the cursor's mod slot out, to hear the track without it, or back in. Its
    /// settings stay as they are, and it keeps running underneath.
    pub(super) fn toggle_modulator(&mut self) {
        let Control::Mod(slot, _) = self.track_control() else {
            self.status = "b switches a mod slot, and this has no switch".to_owned();
            return;
        };
        let mut modulator = self.page_track().mods[slot];
        modulator.bypassed = !modulator.bypassed;
        self.apply(&Action::SetMod {
            track: self.cursor.track,
            slot,
            modulator,
        });
        let name = format!("Mod {}", slot + 1);
        self.status = if modulator.bypassed {
            format!("Switched {name} out: b brings it back")
        } else {
            format!("{name} is back in")
        };
    }
}
