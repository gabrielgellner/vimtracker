//! Gen beside the grid follows the grid's cursor (decision 468, the study
//! `docs/designs/gen-on-phrase.md` §5.3): on a column of the grid, the panel's cursor goes to that
//! column's device, the generator whose rows hold the cursor's row, or the column's first; on the
//! gate, Length's; on an FX slot, its generator and then its lane; and on a column with neither,
//! `Empty`, which then names the column. It doesn't move the grid's cursor, and while the keys are
//! in the panel nothing follows: the panel's device leads, lighting its rows in the grid.

use vt_model::selection::{Field, StepRange};
use vt_model::shape::ShapeColumn;
use vt_model::song::{Phrase, Track};
use vt_model::step::FxCommand;

use super::TrackPage;
use super::generators::{columns, on_field};
use crate::controls::{Control, DEVICE_ROWS, DeviceSpec};
use crate::services::generators::generator_label;

/// What the panel follows on the grid's cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Followed {
    /// A live generator, by its place in its phrase's list.
    Generator(usize),
    /// An FX column's lane, on a column with no generator.
    Lane(usize),
    /// `Empty`, for a column with neither, by the column's field.
    Empty(Field),
}

/// The grid's column a field is on, as Gen's devices have them: the gate's is Length's, as the
/// gate cell's last digits are its length, and an FX slot's command and fine digits are its
/// column's.
pub(crate) fn column_field(field: Field) -> Field {
    match field {
        Field::Gate => Field::GateLength,
        Field::FxCommand(slot) | Field::FxFine(slot) => Field::FxValue(slot),
        field => field,
    }
}

/// A column's short name, as `K` on `Empty` lists them: `Vel`, `FX2`.
pub(crate) fn column_name(field: Field) -> String {
    let field = column_field(field);
    columns()
        .into_iter()
        .find(|(column, _)| *column == field)
        .map_or_else(String::new, |(_, name)| name)
}

/// What the panel follows with the grid's cursor on a field and row of a phrase.
pub(crate) fn followed(phrase: &Phrase, field: Field, step: usize) -> Followed {
    let field = column_field(field);
    let generators = phrase.generators();
    let on: Vec<usize> = (0..generators.len())
        .filter(|at| on_field(&generators[*at], field))
        .collect();
    let holds = |at: &&usize| {
        generators[**at]
            .rows()
            .is_some_and(|rows| (rows.first..=rows.last).contains(&step))
    };
    // The top one of the column's, as the page orders them, where none holds the row.
    let first = on.iter().min_by_key(|at| {
        generators[**at]
            .rows()
            .map_or(usize::MAX, |rows| rows.first)
    });
    if let Some(at) = on.iter().find(holds).or(first) {
        return Followed::Generator(*at);
    }
    match field {
        Field::FxValue(slot) if phrase.lane(slot).is_some() => Followed::Lane(slot),
        field => Followed::Empty(field),
    }
}

/// The column `Empty` names beside the grid: the grid's cursor's, when it has no generator and no
/// lane.
pub(crate) fn empty_named(phrase: &Phrase, field: Field, step: usize) -> Option<Field> {
    match followed(phrase, field, step) {
        Followed::Empty(field) => Some(field),
        _ => None,
    }
}

/// What the status line says the panel follows: `Vel is live, rows 00–1F`, so going along a row
/// reads each column's generator out loud.
pub(crate) fn followed_said(phrase: &Phrase, followed: Followed) -> String {
    let rows = |range: StepRange| format!("rows {:02X}–{:02X}", range.first, range.last);
    match followed {
        Followed::Generator(at) => {
            let Some(generator) = phrase.generators().get(at) else {
                return String::new();
            };
            let name = match generator.column {
                ShapeColumn::Fx { slot, command } => format!("FX{} {}", slot + 1, command.name()),
                _ => generator_label(generator),
            };
            match generator.rows() {
                Some(range) => format!("{name} is live, {}: Ctrl-w l turns it", rows(range)),
                None => format!("{name} is live: Ctrl-w l turns it"),
            }
        }
        Followed::Lane(slot) => {
            let name = format!("FX{}", slot + 1);
            match lane_rows(phrase, slot) {
                Some(range) => format!("{name} has a lane, {}: Ctrl-w l turns it", rows(range)),
                None => format!("{name} has a lane: Ctrl-w l turns it"),
            }
        }
        Followed::Empty(field) => format!(
            "{} isn't live: Ctrl-w l and K make it live",
            column_name(field)
        ),
    }
}

/// The rows of an FX column's lane.
fn lane_rows(phrase: &Phrase, slot: usize) -> Option<StepRange> {
    let lane = phrase.lane(slot)?;
    (lane.length > 0).then(|| StepRange::between(lane.start, lane.start + lane.length - 1))
}

/// The live generator a device is for: a generator's own, its `Loops` or its `Weights`.
fn generator_of(device: &DeviceSpec) -> Option<usize> {
    device.controls().find_map(|control| match control {
        Control::Gen { generator, .. } => Some(generator),
        _ => None,
    })
}

impl TrackPage {
    /// Where a followed device is among Gen's: a generator's own device, a lane's, or `Empty`.
    fn device_of(&self, track: Option<&Track>, followed: Followed) -> Option<usize> {
        let devices = self.devices(track);
        devices.iter().position(|device| match followed {
            Followed::Generator(at) => {
                matches!(device.title[0], Some(Control::Gen { generator, .. }) if generator == at)
            }
            Followed::Lane(slot) => device.lane_slot() == Some(slot),
            Followed::Empty(_) => matches!(device.title[0], Some(Control::NewGenerator(_))),
        })
    }

    /// Put the panel's cursor on the device followed, at the row and setting it's at in the
    /// device it's in, as moving between devices keeps them; it stays put where it's in it
    /// already.
    pub(crate) fn follow(&mut self, track: Option<&Track>, followed: Followed) {
        let Some(device) = self.device_of(track, followed) else {
            return;
        };
        let here = self.device_cursor(track);
        if here.is_some_and(|here| here.device == device) {
            return;
        }
        let row = device * DEVICE_ROWS + here.map_or(0, |here| here.row);
        self.move_to(track, row, self.field);
    }

    /// The live generator the panel's cursor is in: its own device, its `Loops` or its
    /// `Weights`.
    pub(crate) fn device_generator(&self, track: Option<&Track>) -> Option<usize> {
        let cursor = self.device_cursor(track)?;
        generator_of(self.devices(track).get(cursor.device)?)
    }

    /// The rows the panel's device lights in the grid, and their column: a generator's span, or
    /// a lane's rows on its FX column.
    pub(crate) fn lit_rows(&self, track: Option<&Track>) -> Option<(ShapeColumn, StepRange)> {
        let phrase = track?.phrases.get(&self.phrase?)?;
        let cursor = self.device_cursor(track)?;
        let device = self.devices(track).into_iter().nth(cursor.device)?;
        if let Some(slot) = device.lane_slot() {
            // The command doesn't matter for which cells are lit, only the slot.
            let column = ShapeColumn::Fx {
                slot,
                command: FxCommand::Del,
            };
            return Some((column, lane_rows(phrase, slot)?));
        }
        let generator = phrase.generators().get(generator_of(&device)?)?;
        Some((generator.column, generator.rows()?))
    }
}
