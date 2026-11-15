//! The FX page: a rack of the track's slots, each a device, and what `b`, `dd` and `p` do to them.

use std::sync::Arc;

use vt_model::action::{Action, edit};
use vt_model::insert::Insert;
use vt_model::song::Track;

use crate::app::App;

use super::{Control, DEVICE_COLUMNS, DEVICE_ROWS, DeviceSpec, Page, TrackPage};
use crate::input::mode::View;
use crate::pages::Ctx;
use crate::services::Services;

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
    /// and, for Beads, its quality over the third, then its effect's grid. An empty slot has only
    /// its effect.
    pub fn slot(slot: usize, insert: Insert) -> Self {
        let mut title = [None; DEVICE_COLUMNS];
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

/// An effect taken out of a slot, waiting to be put in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TakenInsert {
    pub insert: Insert,
    pub track: usize,
    pub slot: usize,
    /// How far undo went when it was taken, to tell if `p` comes straight after.
    pub undo_depth: usize,
}

impl App {
    /// Whether the FX page shows, zoomed in on the cursor's slot (decision 329).
    pub fn fx_zoomed(&self) -> bool {
        self.modes.view == View::Track && self.track_page.page == Page::Fx && self.track_page.zoomed
    }

    /// What the status line says under it while the FX page is zoomed in, as the track page
    /// tells it.
    pub fn fx_zoom_hint(&self) -> Option<String> {
        self.track_page.zoom_hint(&self.ctx())
    }
}

impl TrackPage {
    /// What the keys do while the FX page is zoomed in, and how to leave it, for under the status
    /// line while nothing else is said there, as the sample page's modes have.
    pub(super) fn zoom_hint(&self, ctx: &Ctx) -> Option<String> {
        (ctx.view() == View::Track && self.page == Page::Fx && self.zoomed).then(|| {
            format!(
                "Slot {} zoomed in: {{ }} another slot, z o every slot",
                self.fx_slot(ctx.track_here()) + 1
            )
        })
    }

    /// The slot the cursor is in, on the FX page.
    fn fx_slot(&self, track: Option<&Track>) -> usize {
        self.device_cursor(track).map_or(0, |cursor| cursor.device)
    }

    /// `b`: switch this slot out to hear the track without it, or back in.
    pub(super) fn toggle_bypass(&self, services: &mut Services) {
        let slot = self.fx_slot(services.track_here());
        let track = services.page_track();
        let insert = track.inserts[slot];
        if insert == Insert::Empty {
            services.status = "There's nothing in this slot to bypass".to_owned();
            return;
        }
        let bypassed = !track.bypassed[slot];
        services.apply(&Action::SetBypass {
            track: services.cursor.track,
            slot,
            bypassed,
        });
        services.status = if bypassed {
            format!("Bypassed {}: b brings it back", insert.name())
        } else {
            format!("{} is back in", insert.name())
        };
    }

    /// `dd`: take the effect out of this slot, for `p` to put somewhere.
    pub(super) fn take_insert(&mut self, services: &mut Services) {
        let slot = self.fx_slot(services.track_here());
        let insert = services.page_track().inserts[slot];
        if insert == Insert::Empty {
            services.status = "There's nothing in this slot to take".to_owned();
            return;
        }
        set_insert(services, slot, Insert::Empty);
        self.taken_insert = Some(TakenInsert {
            insert,
            track: services.cursor.track,
            slot,
            undo_depth: services.history.undo_depth(),
        });
        let song = Arc::clone(services.song());
        self.move_to(
            song.tracks.get(services.cursor.track),
            slot * DEVICE_ROWS,
            0,
        );
        services.status = format!("Took {}: p puts it in a slot", insert.name());
    }

    /// `p`: put what was taken into this slot. What's there goes where it came from, if that's still
    /// empty, so two slots swap. Straight after `dd`, the whole move is one undo step.
    pub(super) fn put_insert(&mut self, services: &mut Services) {
        let Some(taken) = self.taken_insert.take() else {
            services.status = "dd takes an effect out of a slot first".to_owned();
            return;
        };
        let (track, slot) = (services.cursor.track, self.fx_slot(services.track_here()));
        let displaced = services.page_track().inserts[slot];
        let mut actions = vec![Action::SetInsert {
            track,
            slot,
            insert: taken.insert,
        }];
        let source_empty = (taken.track, taken.slot) != (track, slot)
            && services
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
        let mut next = Arc::clone(services.song());
        for action in &actions {
            if let Ok(edited) = edit(&next, action) {
                next = edited;
            }
        }
        if taken.undo_depth == services.history.undo_depth() {
            services.history.replace_current(next);
        } else {
            services.history.commit(next);
        }
        services.dirty = true;
        services.publish();
        services.status = format!("Put {} in slot {}", taken.insert.name(), slot + 1);
    }
}

pub(super) fn set_insert(services: &mut Services, slot: usize, insert: Insert) {
    services.apply(&Action::SetInsert {
        track: services.cursor.track,
        slot,
        insert,
    });
}
