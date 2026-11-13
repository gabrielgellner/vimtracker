//! A phrase's loop and an FX column's lane, set, slid and turned from the cursor.

use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::selection::Field;
use vt_model::song::Lane;
use vt_model::step::{MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note};

use super::{App, Momentary};

impl App {
    pub(super) fn double(&mut self) {
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::Double {
                track: self.cursor.track,
                phrase,
            });
        }
    }

    /// The phrase under the cursor and where its loop starts.
    pub(super) fn loop_place(&self) -> (Option<u32>, usize) {
        (self.phrase_id(), self.extent().start)
    }

    /// After the loop has moved, keep the cursor and the view on the same row numbers, so the
    /// notes scroll under them.
    pub(super) fn keep_row(&mut self, before: (Option<u32>, usize)) {
        let (phrase, old) = before;
        if phrase != self.phrase_id() {
            return;
        }
        let by = self.extent().start as isize - old as isize;
        self.cursor.step = self.cursor.step.saturating_add_signed(by);
        self.top = self.top.saturating_add_signed(by);
    }

    pub(super) fn slide_loop(&mut self, by: isize) {
        if let Some(phrase) = self.phrase_id() {
            let before = self.loop_place();
            self.apply(&Action::SlideLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
            self.keep_row(before);
            self.clamp_cursor();
        }
    }

    /// Rotate the loop's steps. The cursor stays on its row. Going all the way round changes
    /// nothing, so it doesn't leave an undo step.
    pub(super) fn rotate_loop(&mut self, by: isize) {
        let length = self.extent().length as isize;
        if length > 0
            && by.rem_euclid(length) != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::RotateLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
        }
    }

    /// The FX column the cursor is on.
    pub(super) fn fx_column(&self) -> Option<usize> {
        match self.cursor.field {
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => Some(slot),
            Field::Note | Field::Velocity | Field::Nudge | Field::Gate | Field::GateLength => None,
        }
    }

    /// The FX column the cursor is on, if it already has a lane, which the loop keys then set. A
    /// column without one follows the notes, and the loop keys set the phrase's loop as they do
    /// on the notes, so a column only comes apart from them with `:lane`.
    pub(super) fn lane_slot(&self) -> Option<usize> {
        let slot = self.fx_column()?;
        self.phrase_here()?.lane(slot).map(|_| slot)
    }

    /// The cursor's FX column's lane as it is, or the phrase's loop if it has none.
    pub(super) fn lane_here(&self, slot: usize) -> Lane {
        let extent = self.extent();
        self.phrase_here()
            .and_then(|phrase| phrase.lane(slot))
            .unwrap_or(Lane {
                start: extent.start,
                length: extent.length,
            })
    }

    /// Give the cursor's FX column a lane, or none to follow the phrase's loop again.
    pub(super) fn set_lane(&mut self, slot: usize, lane: Option<Lane>) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        if self.phrase_here().and_then(|p| p.lane(slot)) == lane {
            return;
        }
        self.apply(&Action::SetLane {
            track: self.cursor.track,
            phrase,
            slot,
            lane,
        });
        self.status = match lane {
            Some(lane) => format!("FX {} loops on its own, {} steps", slot + 1, lane.length),
            None => format!("FX {} follows the phrase's loop", slot + 1),
        };
    }

    /// Start the loop on the cursor's step. The notes stay where they are and the numbers change.
    /// On an FX column, its lane starts there instead, keeping its length.
    pub(super) fn start_loop_here(&mut self) {
        if let Some(slot) = self.lane_slot() {
            let length = self.lane_here(slot).length;
            let start = self.cursor.step;
            self.set_lane(slot, Some(Lane { start, length }));
            return;
        }
        let by = self.extent().row(self.cursor.step);
        if by != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::SlideLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
        }
    }

    /// End the loop on the cursor's step, so its row is the last that plays. Steps after it stay
    /// stored, and a dim step below the loop brings the steps before it in.
    pub(super) fn end_loop_here(&mut self) {
        if let Some(slot) = self.lane_slot() {
            let start = self.lane_here(slot).start;
            let Some(length) = (self.cursor.step + 1).checked_sub(start).filter(|l| *l > 0) else {
                self.status = "A lane can't end above its start".to_owned();
                return;
            };
            self.set_lane(slot, Some(Lane { start, length }));
            return;
        }
        let extent = self.extent();
        let Ok(row) = usize::try_from(extent.row(self.cursor.step)) else {
            self.status = "The loop can't end above its start".to_owned();
            return;
        };
        let length = row + 1;
        if let Some(phrase) = self.phrase_id()
            && length != extent.length
        {
            self.apply(&Action::SetLength {
                track: self.cursor.track,
                phrase,
                length,
            });
        }
    }

    /// Hear the note on a row outside the loop. Other tracks have nothing to play with it, since
    /// only the loop has a place in the section.
    pub(super) fn audition_outside_loop(&mut self) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
            return;
        }
        let Some(step) = self
            .phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step))
        else {
            return;
        };
        let Note::On(midi) = step.note else {
            return;
        };
        let velocity = step.velocity.unwrap_or(MAX_VELOCITY);
        let transpose = self
            .chain_entries()
            .get(self.cursor.slot)
            .map_or(0, |entry| entry.transpose);
        let note = (i16::from(midi) + i16::from(transpose))
            .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
        self.handle.send(Command::HoldPreview {
            track: self.cursor.track,
            note,
            velocity,
        });
        self.started = Some(Momentary::AuditionNote {
            track: self.cursor.track,
            note,
        });
    }
}
