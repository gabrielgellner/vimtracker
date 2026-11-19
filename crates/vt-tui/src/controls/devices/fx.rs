//! The FX page's devices: the track's slots, each a device.

use vt_model::insert::Insert;
use vt_model::song::Track;

use super::{DeviceSpec, TITLE_PLACES};
use crate::controls::Control;

/// The FX page's devices: a slot each, top to bottom.
pub(super) fn devices(track: &Track) -> Vec<DeviceSpec> {
    track
        .inserts
        .iter()
        .enumerate()
        .map(|(slot, insert)| DeviceSpec::slot(slot, *insert))
        .collect()
}

impl DeviceSpec {
    /// A slot on the FX page: its effect over the grid's first column, its type over the second
    /// and, for Beads, its quality over the third, as Reel's oversampling is, then its effect's
    /// grid. An empty slot has only its effect.
    pub fn slot(slot: usize, insert: Insert) -> Self {
        let mut title = [None; TITLE_PLACES];
        title[0] = Some(Control::Insert(slot));
        title[1] = insert.type_param().map(|at| Control::InsertParam(slot, at));
        title[2] = insert
            .title_param()
            .map(|at| Control::InsertParam(slot, at));
        let device = Self::new((slot + 1).to_string(), title, insert.grid(), |at| {
            Control::InsertParam(slot, at)
        });
        Self {
            switchable: insert != Insert::Empty,
            ..device
        }
    }
}
