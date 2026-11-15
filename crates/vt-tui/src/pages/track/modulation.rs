//! The Mod page: a rack of half-width devices, the track's six mod slots down two columns, `Mod 1`
//! to `Mod 3` on the left and `Mod 4` to `Mod 6` on the right, and the accumulator beside the
//! arpeggio under them (the mod slots study, `docs/designs/mod-slots.md`, §7). A slot's title has
//! its type, its type's mode if it has one, and its destination, and a switch, which `b` flips to
//! hear the track without it. Each type's title and grid are its spec's, in the model
//! (`vt_model::modulation::MOD_SPECS`), which say why each setting sits where it does.

use std::sync::Arc;

use vt_model::action::Action;
use vt_model::grid::Grid;
use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot};
use vt_model::settings::SettingId;
use vt_model::song::Track;
use vt_model::step::{FxCommand, FxSlot, MAX_NOTE, MIN_NOTE, Note, Step};
use vt_model::steps::STEPS;
use vt_model::variation::{Varies, varies_at};

use crate::app::App;
use crate::services::Services;

use super::editing::set_mod;

use super::{AccumulatorField, ArpField, Control, DEVICE_COLUMNS, DeviceSpec};

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

impl App {
    /// `Space g a` on a Steps slot: keep what its `Mutate` has drawn as its values, as one undo
    /// step, so the loop heard stays when `Mutate` goes back to none (#530). The engine reports the
    /// draws, as it hears them, and this makes the edit, so the song says what was heard and a
    /// render plays it again. Only steps drawn change, so a value written since keeps its own.
    pub(crate) fn keep_drift(&mut self) {
        let Control::Mod(slot, _) = self.track_control() else {
            self.services.status =
                "Space g a keeps a Steps slot's drift: this isn't a mod slot".to_owned();
            return;
        };
        let name = format!("Mod {}", slot + 1);
        let mut modulator = self.services.page_track().mods[slot];
        if modulator.kind != ModKind::Steps {
            self.services.status = format!(
                "Space g a keeps a Steps slot's drift: {name} is {}",
                modulator.kind.name()
            );
            return;
        }
        // The draws were worked out from the version the latest report is of, a frame or so
        // behind the song while it's being changed, and are kept only from a Steps slot there.
        let reported = self
            .services
            .reported_track()
            .is_some_and(|track| track.mods[slot].kind == ModKind::Steps);
        let drawn = self
            .services
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
            self.services.status =
                format!("{name} has drawn nothing to keep: Mutate draws as it plays");
            return;
        }
        set_mod(&mut self.services, slot, modulator);
        let values = if kept == 1 { "value" } else { "values" };
        self.services.status =
            format!("Kept the {kept} {values} {name} drew: Mutate 0% now plays them");
    }

    /// `Space g A`: make the phrase here what was last heard, as one undo step: write into its
    /// notes, or the selection's, the pitch the track's Steps and LFO slots on `Pitch` moved each
    /// to the last time it was heard, and switch those slots out (#569); write in what each row's
    /// own `RND`s on its note moved it to, and take those `RND`s out (decision 323); and apply each
    /// live generator a macro or `Vary` moved, each row as it was heard and the rest as written
    /// (decision 300). The engine reports each slot's and `RND`'s move as each note starts
    /// (`NotePitch`), and each generator's row as it plays. A move of part of a semitone goes to
    /// the nearest note, as the Note column has no finer step.
    pub(crate) fn bake_pitch_mods(&mut self) {
        let selected = self.modes.mode.is_visual().then(|| {
            let selection = self.current_selection();
            self.leave_selection(selection);
            selection.range
        });
        let track = self.services.cursor.track;
        let Some(phrase) = self.services.phrase_id() else {
            self.services.status = "No phrase here to write the pitch into".to_owned();
            return;
        };
        let song = Arc::clone(self.song());
        let owner = &song.tracks[track];
        let Some(held) = owner.phrases.get(&phrase) else {
            return;
        };
        let steps = held.steps();
        // Envelopes and Track slots on `Pitch` shape each note rather than choose it, so they
        // stay; a slot switched out moves nothing.
        let baked = |slot: &ModSlot| {
            matches!(slot.kind, ModKind::Steps | ModKind::Lfo)
                && slot.destination == Some(SettingId::PITCH)
                && !slot.bypassed
        };
        // The generators heard otherwise than written, applied whole, last first so each keeps
        // its place in the list until it goes.
        let moving: Vec<usize> = (0..held.generators().len())
            .rev()
            .filter(|at| self.heard_rows_of(track, phrase, *at).is_some())
            .collect();
        let (first, last) = selected.map_or((0, steps.len().saturating_sub(1)), |range| {
            (range.first, range.last.min(steps.len().saturating_sub(1)))
        });
        // A row's own `RND`s on its note, by column, not a lane's, which plays whatever row is
        // under it.
        let varying = |row: &Step| -> Vec<usize> {
            (0..row.fx.len())
                .filter(|&column| {
                    row.fx[column].command == Some(FxCommand::Rnd)
                        && held.lane(column).is_none()
                        && varies_at(&row.fx, column) == Varies::Note
                })
                .collect()
        };
        let any_varying =
            steps
                .iter()
                .enumerate()
                .take(last + 1)
                .skip(first)
                .any(|(row, written)| {
                    let heard = self.services.heard_step(track, phrase, row);
                    !varying(heard.as_ref().unwrap_or(written)).is_empty()
                });
        if moving.is_empty() && !owner.mods.iter().any(baked) && !any_varying {
            self.services.status =
                "No Steps or LFO slot on Pitch, RND on a note, or generator moved, to write in"
                    .to_owned();
            return;
        }
        let mut actions: Vec<Action> = moving
            .iter()
            .map(|&generator| Action::ApplyGenerator {
                track,
                phrase,
                generator,
            })
            .collect();
        // Each row as heard, with every moving generator on it laid over.
        let mut rows: std::collections::BTreeMap<usize, Step> = (0..steps.len())
            .filter_map(|row| Some((row, self.services.heard_step(track, phrase, row)?)))
            .collect();
        let mut moved = [false; MOD_SLOTS];
        let (mut heard, mut unheard, mut varied) = (0, 0, 0);
        for (step, written) in steps.iter().enumerate().take(last + 1).skip(first) {
            let row = rows.get(&step).unwrap_or(written);
            let Note::On(midi) = row.note else {
                continue;
            };
            let rnds = varying(row);
            if !owner.mods.iter().any(baked) && rnds.is_empty() {
                continue;
            }
            let Some(pitch) = self.services.heard_pitches.get(&(track, phrase, step)) else {
                unheard += 1;
                continue;
            };
            heard += 1;
            let mut by = 0.0;
            for (at, slot) in owner.mods.iter().enumerate() {
                if baked(slot) {
                    by += pitch.moves[at];
                    moved[at] |= pitch.moves[at].abs() >= 0.5;
                }
            }
            // The pass heard is what the row plays from now on, so its `RND`s on the note go,
            // whether or not they moved it this time.
            if !rnds.is_empty() {
                by += f32::from(pitch.varied);
                varied += 1;
            }
            let note = (f32::from(midi) + by)
                .round()
                .clamp(f32::from(MIN_NOTE), f32::from(MAX_NOTE)) as u8;
            // A row with nothing but its note to change changes only its note, so a live column
            // elsewhere on it stays live.
            if rnds.is_empty() && !rows.contains_key(&step) {
                if note != midi {
                    actions.push(Action::SetNote {
                        track,
                        phrase,
                        step,
                        note: Note::On(note),
                    });
                }
                continue;
            }
            let mut row = row.clone();
            row.note = Note::On(note);
            for column in rnds {
                row.fx[column] = FxSlot::default();
            }
            rows.insert(step, row);
        }
        let slots: Vec<usize> = (0..MOD_SLOTS).filter(|at| moved[*at]).collect();
        if moving.is_empty() {
            if heard == 0 {
                self.services.status =
                    "None of these notes has been heard yet: play them first".to_owned();
                return;
            }
            if slots.is_empty() && varied == 0 {
                self.services.status = "No Steps or LFO slot on Pitch moved these notes".to_owned();
                return;
            }
        }
        actions.extend(std::mem::take(&mut rows).into_iter().map(|(step, value)| {
            Action::SetStep {
                track,
                phrase,
                step,
                value,
            }
        }));
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
        self.services.apply(&Action::Batch(actions));
        let mut said = Vec::new();
        if !slots.is_empty() {
            let names: Vec<String> = slots.iter().map(|at| format!("Mod {}", at + 1)).collect();
            let (names, them) = match names.as_slice() {
                [one] => (one.clone(), "it"),
                [rest @ .., last] => (format!("{} and {last}", rest.join(", ")), "them"),
                [] => unreachable!(),
            };
            let notes = if heard == 1 { "note" } else { "notes" };
            said.push(format!(
                "Wrote what {names} played into {heard} {notes}, and switched {them} out"
            ));
        }
        if varied > 0 {
            let notes = if varied == 1 { "note" } else { "notes" };
            let verb = if said.is_empty() { "Wrote" } else { "wrote" };
            said.push(format!(
                "{verb} what RND played into {varied} {notes}, and took their RNDs out"
            ));
        }
        if !moving.is_empty() {
            let names: Vec<String> = moving
                .iter()
                .rev()
                .map(|at| crate::overlays::shape::generator_label(&held.generators()[*at]))
                .collect();
            let verb = if said.is_empty() {
                "Applied"
            } else {
                "applied"
            };
            said.push(format!("{verb} {} as heard", names.join(", ")));
        }
        let mut status = said.join("; ");
        if unheard > 0 {
            status.push_str(&format!("; {unheard} notes not heard yet stay as written"));
        }
        self.services.status = status;
    }
}

/// `b`: switch the cursor's mod slot out, to hear the track without it, or back in. Its
/// settings stay as they are, and it keeps running underneath.
pub(super) fn toggle_modulator(services: &mut Services, control: Control) {
    let Control::Mod(slot, _) = control else {
        services.status = "b switches a mod slot, and this has no switch".to_owned();
        return;
    };
    let mut modulator = services.page_track().mods[slot];
    modulator.bypassed = !modulator.bypassed;
    services.apply(&Action::SetMod {
        track: services.cursor.track,
        slot,
        modulator,
    });
    let name = format!("Mod {}", slot + 1);
    services.status = if modulator.bypassed {
        format!("Switched {name} out: b brings it back")
    } else {
        format!("{name} is back in")
    };
}
