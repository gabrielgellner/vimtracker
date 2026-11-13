//! The Mod page: a rack of half-width devices, the track's six mod slots down two columns, `Mod 1`
//! to `Mod 3` on the left and `Mod 4` to `Mod 6` on the right, and the accumulator beside the
//! arpeggio under them (the mod slots study, `docs/designs/mod-slots.md`, §7). A slot's title has
//! its type, its type's mode if it has one, and its destination, and a switch, which `b` flips to
//! hear the track without it. Each type states its grid here, by what its settings mean.

use std::sync::Arc;

use vt_model::action::Action;
use vt_model::grid::Grid;
use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot};
use vt_model::settings::SettingId;
use vt_model::song::Track;
use vt_model::step::{MAX_NOTE, MIN_NOTE, Note};
use vt_model::steps::STEPS;

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

/// A low-pass gate's settings: its rise and fall and how far it closes the filter down the left,
/// and how far it goes and its reset down the right, with a hole. It has no mode, as its curve is
/// the point of it.
const LPG_SETTINGS: [ModField; 5] = [
    ModField::Attack,
    ModField::Decay,
    ModField::Colour,
    ModField::Depth,
    ModField::Reset,
];
const LPG_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]];

/// An LFO's settings: what it plays down the left, and where it starts, how it fades after each
/// note and how its wave bends down the right. How notes start it is its mode, in the title.
const LFO_SETTINGS: [ModField; 6] = [
    ModField::Shape,
    ModField::Rate,
    ModField::Depth,
    ModField::Phase,
    ModField::Fade,
    ModField::Deform,
];
const LFO_GRID: [[Option<usize>; 3]; 2] =
    [[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]];

/// Steps' settings: how fast, how many and how far down the left, and where each note starts it,
/// how it drifts and how it goes from step to step down the right; then its values, a cursor stop
/// of its own in the panel beside them, on every row, so `l` from any of the grid goes in. What it
/// does at its end is in its title, with its mode.
const STEPS_SETTINGS: [ModField; 7] = [
    ModField::Rate,
    ModField::Length,
    ModField::Depth,
    ModField::Start,
    ModField::Mutate,
    ModField::Smooth,
    ModField::Step(0),
];
const STEPS_GRID: [[Option<usize>; 3]; 3] = [
    [Some(0), Some(1), Some(2)],
    [Some(3), Some(4), Some(5)],
    [Some(6), Some(6), Some(6)],
];

/// Track's settings: where its map starts and ends, and how far it goes, down the left, with the
/// right left empty. What it follows is its mode, in the title, and its map is drawn beside them.
const TRACK_SETTINGS: [ModField; 3] = [ModField::Low, ModField::High, ModField::Depth];
const TRACK_GRID: [[Option<usize>; 3]; 2] = [[Some(0), Some(1), Some(2)], [None, None, None]];

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
        ModKind::Lpg => (&LPG_SETTINGS, Grid::from_columns(&LPG_GRID)),
        ModKind::Lfo => (&LFO_SETTINGS, Grid::from_columns(&LFO_GRID)),
        ModKind::Steps => (&STEPS_SETTINGS, Grid::from_columns(&STEPS_GRID)),
        ModKind::Track => (&TRACK_SETTINGS, Grid::from_columns(&TRACK_GRID)),
    }
}

/// A mod slot as a half-width device, `Mod 1` to `Mod 6`, with its switch, then its type, its
/// type's mode if it has one, Steps' loop, its polarity if it has one to choose and its destination
/// in its title, in that order for `h` and `l`.
fn mod_device(slot: usize, modulator: &ModSlot) -> DeviceSpec {
    let kind = modulator.kind;
    let mut title = [None; DEVICE_COLUMNS];
    let fields = [
        ModField::Type,
        ModField::Mode,
        ModField::Loop,
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

/// The macros' places in their band: two rows of four, in order along each.
const MACROS_GRID: [[Option<usize>; 2]; 4] = [
    [Some(0), Some(4)],
    [Some(1), Some(5)],
    [Some(2), Some(6)],
    [Some(3), Some(7)],
];

/// The track's eight macros as a full-width band, two rows of four, with nothing in its title
/// (decision 285). Its own device, so it can sit on whichever page it belongs on.
pub(crate) fn macros_device() -> DeviceSpec {
    DeviceSpec::new(
        "Macros",
        [None; DEVICE_COLUMNS],
        Grid::from_columns(&MACROS_GRID),
        Control::Macro,
    )
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

    /// `Space g a` on a Steps slot: keep what its `Mutate` has drawn as its values, as one undo
    /// step, so the loop heard stays when `Mutate` goes back to none (#530). The engine reports the
    /// draws, as it hears them, and this makes the edit, so the song says what was heard and a
    /// render plays it again. Only steps drawn change, so a value written since keeps its own.
    pub(crate) fn keep_drift(&mut self) {
        let Control::Mod(slot, _) = self.track_control() else {
            self.status = "Space g a keeps a Steps slot's drift: this isn't a mod slot".to_owned();
            return;
        };
        let name = format!("Mod {}", slot + 1);
        let mut modulator = self.page_track().mods[slot];
        if modulator.kind != ModKind::Steps {
            self.status = format!(
                "Space g a keeps a Steps slot's drift: {name} is {}",
                modulator.kind.name()
            );
            return;
        }
        // The draws were worked out from the version the latest report is of, a frame or so
        // behind the song while it's being changed, and are kept only from a Steps slot there.
        let reported = self
            .reported_track()
            .is_some_and(|track| track.mods[slot].kind == ModKind::Steps);
        let drawn = self
            .live
            .filter(|_| reported)
            .map(|live| live.drawn[slot])
            .unwrap_or_default();
        let steps = &mut modulator.steps;
        let mut kept = 0;
        for step in 0..usize::from(steps.length).min(STEPS) {
            if let Some(value) = drawn.value(step) {
                kept += 1;
                steps.values[step] = value;
            }
        }
        if kept == 0 {
            self.status = format!("{name} has drawn nothing to keep: Mutate draws as it plays");
            return;
        }
        self.set_mod(slot, modulator);
        let values = if kept == 1 { "value" } else { "values" };
        self.status = format!("Kept the {kept} {values} {name} drew: Mutate 0% now plays them");
    }

    /// `Space g A`: write into the notes of the phrase here, or of the selection, the pitch the
    /// track's Steps and LFO slots on `Pitch` moved each to the last time it was heard, and switch
    /// those slots out, so it sounds the same, as one undo step (#569). The engine reports each
    /// slot's move as each note starts (`NotePitch`), and this makes the edit. A move of part of a
    /// semitone goes to the nearest note, as the Note column has no finer step.
    pub(crate) fn bake_pitch_mods(&mut self) {
        let selected = self.mode.is_visual().then(|| {
            let selection = self.current_selection();
            self.leave_selection(selection);
            selection.range
        });
        let track = self.cursor.track;
        let Some(phrase) = self.phrase_id() else {
            self.status = "No phrase here to write the pitch into".to_owned();
            return;
        };
        let song = Arc::clone(self.song());
        let owner = &song.tracks[track];
        let Some(steps) = owner.phrases.get(&phrase).map(|phrase| phrase.steps()) else {
            return;
        };
        // Envelopes and Track slots on `Pitch` shape each note rather than choose it, so they
        // stay; a slot switched out moves nothing.
        let baked = |slot: &ModSlot| {
            matches!(slot.kind, ModKind::Steps | ModKind::Lfo)
                && slot.destination == Some(SettingId::PITCH)
                && !slot.bypassed
        };
        if !owner.mods.iter().any(baked) {
            self.status = "No Steps or LFO slot on Pitch to write into the notes".to_owned();
            return;
        }
        let (first, last) = selected.map_or((0, steps.len().saturating_sub(1)), |range| {
            (range.first, range.last.min(steps.len().saturating_sub(1)))
        });
        let mut actions = Vec::new();
        let mut moved = [false; MOD_SLOTS];
        let (mut heard, mut unheard) = (0, 0);
        for (step, row) in steps.iter().enumerate().take(last + 1).skip(first) {
            let Note::On(midi) = row.note else {
                continue;
            };
            let Some(moves) = self.heard_pitches.get(&(track, phrase, step)) else {
                unheard += 1;
                continue;
            };
            heard += 1;
            let mut by = 0.0;
            for (at, slot) in owner.mods.iter().enumerate() {
                if baked(slot) {
                    by += moves[at];
                    moved[at] |= moves[at].abs() >= 0.5;
                }
            }
            let note = (f32::from(midi) + by)
                .round()
                .clamp(f32::from(MIN_NOTE), f32::from(MAX_NOTE)) as u8;
            if note != midi {
                actions.push(Action::SetNote {
                    track,
                    phrase,
                    step,
                    note: Note::On(note),
                });
            }
        }
        if heard == 0 {
            self.status = "None of these notes has been heard yet: play them first".to_owned();
            return;
        }
        let slots: Vec<usize> = (0..MOD_SLOTS).filter(|at| moved[*at]).collect();
        if slots.is_empty() {
            self.status = "No Steps or LFO slot on Pitch moved these notes".to_owned();
            return;
        }
        for &slot in &slots {
            actions.push(Action::SetMod {
                track,
                slot,
                modulator: ModSlot {
                    bypassed: true,
                    ..owner.mods[slot]
                },
            });
        }
        self.apply(&Action::Batch(actions));
        let names: Vec<String> = slots.iter().map(|at| format!("Mod {}", at + 1)).collect();
        let (names, them) = match names.as_slice() {
            [one] => (one.clone(), "it"),
            [rest @ .., last] => (format!("{} and {last}", rest.join(", ")), "them"),
            [] => unreachable!(),
        };
        let notes = if heard == 1 { "note" } else { "notes" };
        let mut status =
            format!("Wrote what {names} played into {heard} {notes}, and switched {them} out");
        if unheard > 0 {
            status.push_str(&format!("; {unheard} not heard yet stay as written"));
        }
        self.status = status;
    }
}
