//! The FX page: a rack of the track's slots, each a device, and what `b`, `dd` and `p` do to them.

use std::sync::Arc;

use vt_model::action::{Action, edit};
use vt_model::insert::Insert;
use vt_model::song::Track;

use crate::app::App;

use super::{Control, DEVICE_COLUMNS, DEVICE_ROWS, DeviceSpec};

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
    /// A slot on the FX page: its effect over the grid's first column and its type over the
    /// second, then its effect's grid. An empty slot has only its effect.
    pub fn slot(slot: usize, insert: Insert) -> Self {
        let mut title = [None; DEVICE_COLUMNS];
        title[0] = Some(Control::Insert(slot));
        title[1] = insert.type_param().map(|at| Control::InsertParam(slot, at));
        let device = Self::new((slot + 1).to_string(), title, insert.grid(), |at| {
            Control::InsertParam(slot, at)
        });
        Self {
            switchable: insert != Insert::Empty,
            ..device
        }
    }
}

/// An effect taken out of a slot, waiting to be put in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakenInsert {
    pub insert: Insert,
    pub track: usize,
    pub slot: usize,
    /// How far undo went when it was taken, to tell if `p` comes straight after.
    pub undo_depth: usize,
}

impl App {
    /// The slot the cursor is in, on the FX page.
    fn fx_slot(&self) -> usize {
        self.track_page
            .device_cursor(self.track_here())
            .map_or(0, |cursor| cursor.device)
    }

    /// `b`: switch this slot out to hear the track without it, or back in.
    pub(super) fn toggle_bypass(&mut self) {
        let slot = self.fx_slot();
        let track = self.page_track();
        let insert = track.inserts[slot];
        if insert == Insert::Empty {
            self.status = "There's nothing in this slot to bypass".to_owned();
            return;
        }
        let bypassed = !track.bypassed[slot];
        self.apply(&Action::SetBypass {
            track: self.cursor.track,
            slot,
            bypassed,
        });
        self.status = if bypassed {
            format!("Bypassed {}: b brings it back", insert.name())
        } else {
            format!("{} is back in", insert.name())
        };
    }

    /// `dd`: take the effect out of this slot, for `p` to put somewhere.
    pub(super) fn take_insert(&mut self) {
        let slot = self.fx_slot();
        let insert = self.page_track().inserts[slot];
        if insert == Insert::Empty {
            self.status = "There's nothing in this slot to take".to_owned();
            return;
        }
        self.set_insert(slot, Insert::Empty);
        self.taken_insert = Some(TakenInsert {
            insert,
            track: self.cursor.track,
            slot,
            undo_depth: self.history.undo_depth(),
        });
        self.move_control(slot * DEVICE_ROWS, 0);
        self.status = format!("Took {}: p puts it in a slot", insert.name());
    }

    /// `p`: put what was taken into this slot. What's there goes where it came from, if that's still
    /// empty, so two slots swap. Straight after `dd`, the whole move is one undo step.
    pub(super) fn put_insert(&mut self) {
        let Some(taken) = self.taken_insert.take() else {
            self.status = "dd takes an effect out of a slot first".to_owned();
            return;
        };
        let (track, slot) = (self.cursor.track, self.fx_slot());
        let displaced = self.page_track().inserts[slot];
        let mut actions = vec![Action::SetInsert {
            track,
            slot,
            insert: taken.insert,
        }];
        let source_empty = (taken.track, taken.slot) != (track, slot)
            && self
                .song()
                .tracks
                .get(taken.track)
                .is_some_and(|source| source.inserts[taken.slot] == Insert::Empty);
        if displaced != Insert::Empty {
            if source_empty {
                actions.push(Action::SetInsert {
                    track: taken.track,
                    slot: taken.slot,
                    insert: displaced,
                });
            } else {
                // There's nowhere for it to go, so it's the next one to put.
                self.taken_insert = Some(TakenInsert {
                    insert: displaced,
                    track,
                    slot,
                    undo_depth: 0,
                });
            }
        }
        let mut next = Arc::clone(self.song());
        for action in &actions {
            if let Ok(edited) = edit(&next, action) {
                next = edited;
            }
        }
        if taken.undo_depth == self.history.undo_depth() {
            self.history.replace_current(next);
        } else {
            self.history.commit(next);
        }
        self.dirty = true;
        self.publish();
        self.status = format!("Put {} in slot {}", taken.insert.name(), slot + 1);
    }

    pub(super) fn set_insert(&mut self, slot: usize, insert: Insert) {
        self.apply(&Action::SetInsert {
            track: self.cursor.track,
            slot,
            insert,
        });
    }
}
