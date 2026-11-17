//! Working out a track's step and playing it: the row it lands on, its nudge, its
//! conditions, its variations, its accumulator and its arpeggio.

use std::sync::Arc;

use vt_model::accumulator::{Adds, MOST_DEGREES, Reset, amount, moved_note};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::condition::{Condition, Situation};
use vt_model::constants::{FX_SLOTS, NUDGE_TICKS, TICKS_PER_STEP};
use vt_model::dice::{DiceStep, looped_chance};
use vt_model::groove::{FULL_VELOCITY, Groove, GrooveId, MAX_PUSH};
use vt_model::order::{WalkMove, jumped};
use vt_model::phrase::Phrase;
use vt_model::settings::{SettingId, moved_chance};
use vt_model::shape::RowView;
use vt_model::song::{ChainPosition, Launch, Song, Track};
use vt_model::speed::Speed;
use vt_model::step::{
    FxCommand, FxSlot, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Mark, Note, Step, most_nudge,
};
use vt_model::variation::{
    Roll, Variation, Varies, kept, thin_keeps, varied_lock, varied_ratchet, varies_at,
};

use crate::gens::{Passes, Played, RowAt};
use crate::track_state::{
    Accumulating, Arpeggiating, Decided, DelayedNote, Epoch, JumpAt, LaneRead, Part, PendingStep,
    Retrigger, RowKey, Worked,
};
use crate::voice::NoteOn;

/// The settings that move a track's phrase's walk, read before a row plays (decisions 400 and
/// 401).
const WALK_SETTINGS: [SettingId; 6] = [
    SettingId::ORDER,
    SettingId::REVERSE,
    SettingId::ORDER_X,
    SettingId::ORDER_Y,
    SettingId::PHRASE_LENGTH,
    SettingId::PHRASE_START,
];

/// The settings of a track's dice loop, read before a row plays (decisions 410 and 411).
const DICE_SETTINGS: [SettingId; 3] = [
    SettingId::DICE_LOOP,
    SettingId::DICE_MUTATE,
    SettingId::DICE_JUMP,
];

/// Where each FX column's lane reads at a step, for those with one.
pub(super) type LaneReads = [Option<LaneRead>; FX_SLOTS];

use super::Engine;
use super::transport::next_row;

impl Engine {
    /// Work out a track's next step, and play it now or when its groove lands it. A row with a
    /// nudge lands its note part that much off where the groove lands its lanes. The step lasts as
    /// long as its speed says, until the next of that speed's steps.
    pub(super) fn work_out(&mut self, index: usize) {
        let (lead, groove_lead) = (self.current.lead, self.current.groove_lead as i64);
        let nudges = self.current.nudges;
        let clock = self.tracks[index].clock;
        let (grid, running) = (clock.beat, clock.count);
        self.tracks[index].playing = self.track_chain(index);
        let place = self.track_place(index, running);
        let count = running - self.tracks[index].started;
        // Each lane is read once a step, here, in order, so it counts along.
        let lanes = self.read_lanes(index, place, count);
        // How its settings move its phrase's walk is read first, as it says which row the rest
        // look at (decision 400).
        let walk = self.walk_ahead(index, place, running, &lanes);
        // Whether it jumps to another row is decided before anything looks at the row, so its
        // groove, speed and nudge are the row's it plays.
        let jumped = self.jump_at(index, place, running, &lanes);
        // Which step's dice it rolls, read before the row plays too, so a lock on the row counts
        // (decision 410).
        let dice = self.dice_at(index, place, running, count, &lanes);
        // The groove counts the track's own steps from the start or the last reset, not its
        // loop, so an odd length stays on the beat, and a track at an eighth swings its eighths.
        let (groove, speed) = self.step_choices(index, running, &lanes);
        let ticks = speed.ticks();
        let humanise = self.tracks[index].humanise(groove.humanise());
        let at = running.max(0) as usize;
        let push = (i64::from(groove.push(at)) + humanise).clamp(-groove_lead, i64::from(MAX_PUSH));
        let step = PendingStep {
            at: self.landing(grid, push, ticks),
            running: at,
            groove,
            ticks,
            part: Part::Whole,
            step: grid,
            leads: true,
            place,
            count,
            lanes,
            jumped,
            walk,
            dice,
            plays: None,
            start: self.tracks[index].chain_starts,
        };
        // Grid, then groove, then nudge. A track's notes play in order, so a row's note part
        // never lands before the last row's. The push is a share of the step and the nudge the
        // same time on every track, so they're added as ticks times a sixteenth's.
        let row_at = if nudges {
            let nudge = self.row_nudge(index, running, ticks, &lanes);
            let sixteenth = TICKS_PER_STEP as i64;
            let moved = push * ticks as i64 + nudge * sixteenth;
            let at = self.landed(grid, moved.max(-(lead as i64) * sixteenth));
            self.tracks[index]
                .last_row_at()
                .map_or(at, |last| at.max(last))
        } else {
            step.at
        };
        let parts = if row_at == step.at {
            [Some(step), None]
        } else {
            let lanes = PendingStep {
                part: Part::Lanes,
                leads: step.at < row_at,
                ..step
            };
            let row = PendingStep {
                at: row_at,
                part: Part::Row,
                leads: row_at < step.at,
                ..step
            };
            [Some(lanes), Some(row)]
        };
        let state = &mut self.tracks[index];
        let next = state.clock.after(ticks);
        state.note_worked(Worked {
            grid,
            count: running,
            ticks: next - grid,
            speed,
            lanes,
            jumped,
            walk,
        });
        state.clock.advance(ticks);
        for part in parts.into_iter().flatten() {
            if let Some(earliest) = self.tracks[index].wait_for(part) {
                self.fire_track(index, earliest);
            }
        }
        while let Some(part) = self.tracks[index].take_due(self.clock) {
            self.fire_track(index, part);
        }
    }

    /// Whether the step a track is working out plays another row of its phrase than the one its
    /// order walks to, and which, as its `Jump` decides: once for each visit of a row, on the
    /// step it starts, or the first worked out of it, which its later pulses keep. The chance is
    /// read before the row plays, as `QNT` is: a `JMP` on the row walked to, or in a lane at it,
    /// or what the track holds, with what modulation adds. With none of those it rolls nothing,
    /// and the walk plays as it did before there was a `Jump` (decision 391).
    fn jump_at(
        &mut self,
        index: usize,
        place: Option<(u32, usize)>,
        running: i64,
        lanes: &LaneReads,
    ) -> Option<usize> {
        let (chain, place) = place?;
        let current = Arc::clone(&self.current);
        let owner = current.song.tracks.get(index)?;
        let position = owner.chain_position(chain, place, self.dealer(index), self.walk(index))?;
        let start = place - position.pulse;
        let kept = self.tracks[index].jump;
        if let Some(kept) = kept.filter(|kept| kept.chain == chain && kept.start == start) {
            return kept.row;
        }
        // The chance is the walked row's, so nothing is jumped while it's read.
        self.tracks[index].jump = None;
        let state = &self.tracks[index];
        let moved = state.jump_moved();
        let written = owner.step_at(position)?;
        let locked = written
            .fx
            .iter()
            .any(|slot| slot.command == Some(FxCommand::Set(SettingId::JUMP)));
        let held = state.held_choice(SettingId::JUMP, false);
        let row = if held == 0 && moved <= 0.0 && !locked && lanes.iter().all(Option::is_none) {
            None
        } else {
            let walked = self.row_ahead(index, running, lanes);
            let chosen = self.chosen_ahead(index, running, walked, SettingId::JUMP, lanes);
            let chance = ((f32::from(chosen) + moved) / 255.0).clamp(0.0, 1.0);
            let phrase = owner.phrases.get(&position.phrase)?;
            jumped(phrase.loop_steps(), position.deal, position.visit, chance)
        };
        self.tracks[index].jump = Some(JumpAt { chain, start, row });
        row
    }

    /// Which step's dice the step a track is working out rolls, its `CHA`s and `RND`s, as its dice
    /// loop decides, or `None` with the loop off, when they roll as they always have (decision
    /// 410). Its settings are read before the row plays, as `JMP`'s are: a `DLP`, `DMU` or `DJMP`
    /// on the row it plays, or in a lane at it, or what the track holds, with what modulation
    /// adds. With the loop off, no lock on the row and no lane, nothing more is read.
    fn dice_at(
        &mut self,
        index: usize,
        place: Option<(u32, usize)>,
        running: i64,
        count: i64,
        lanes: &LaneReads,
    ) -> Option<DiceStep> {
        let held = self.tracks[index].held_dice();
        let locked =
            lanes.iter().any(Option::is_some) || self.plays_lock(index, place, &DICE_SETTINGS);
        let settings = if locked {
            let row = self.row_ahead(index, running, lanes);
            let chosen = DICE_SETTINGS.map(|id| self.chosen_ahead(index, running, row, id, lanes));
            self.tracks[index].dice_of(|id| {
                let at = DICE_SETTINGS.iter().position(|dice| *dice == id);
                at.map_or(0, |at| chosen[at])
            })
        } else {
            held
        };
        if settings.length == 0 {
            return None;
        }
        let seed = self.current.song.seed;
        self.tracks[index]
            .dice
            .roll(settings, seed, index, DiceStep { running, count })
    }

    /// Whether the row a track plays at this place in its chain, as written, locks any of these
    /// settings: the row its `Jump` played in place of the one walked to, if it jumped.
    fn plays_lock(&self, index: usize, place: Option<(u32, usize)>, ids: &[SettingId]) -> bool {
        let Some((chain, place)) = place else {
            return false;
        };
        let Some(owner) = self.current.song.tracks.get(index) else {
            return false;
        };
        let Some(mut position) =
            owner.chain_position(chain, place, self.dealer(index), self.walk(index))
        else {
            return false;
        };
        if let Some(row) = self.tracks[index]
            .jump
            .filter(|jump| jump.chain == chain && jump.start == place - position.pulse)
            .and_then(|jump| jump.row)
        {
            position.step = row;
        }
        owner.step_at(position).is_some_and(|step| {
            step.fx.iter().any(|slot| {
                ids.iter()
                    .any(|id| slot.command == Some(FxCommand::Set(*id)))
            })
        })
    }

    /// How a track's settings move its phrase's walk at the step it's working out, read before the
    /// row plays, as `JMP` is (decision 400): an `ORD`, `REV`, `ORDX` or `ORDY` on the row the
    /// walk held goes to, or in a lane at it, or what the track holds, each with what modulation
    /// adds. With no lock on that row, none held and no lane, it's what's held, and at rest every
    /// phrase walks as it's written. With `Cue` on, the window's `length` and `start` are those its
    /// pass took at its first step (decision 402). It's kept for the step's lookups, and rides
    /// with the step.
    fn walk_ahead(
        &mut self,
        index: usize,
        place: Option<(u32, usize)>,
        running: i64,
        lanes: &LaneReads,
    ) -> WalkMove {
        let held = self.tracks[index].held_walk();
        if !self.tracks[index].cues() {
            self.tracks[index].window = None;
            return self.walk_chosen(index, place, running, lanes, held);
        }
        // A pass takes the window at its first step, or at the first it plays since it started.
        let first = place.is_some_and(|(chain, place)| {
            let current = Arc::clone(&self.current);
            current.song.tracks.get(index).is_some_and(|owner| {
                owner
                    .chain_position(chain, place, self.dealer(index), held)
                    .is_some_and(|position| position.time == 0)
            })
        });
        let window = self.tracks[index].window.filter(|_| !first);
        let cued = |walk: WalkMove| match window {
            Some((length, start)) => WalkMove {
                length,
                start,
                ..walk
            },
            None => walk,
        };
        let walk = cued(self.walk_chosen(index, place, running, lanes, cued(held)));
        self.tracks[index].window = Some((walk.length, walk.start));
        self.tracks[index].walk = walk;
        walk
    }

    /// How a track's settings move its phrase's walk at the step it's working out, from `held`,
    /// what it holds with what modulation adds, and any lock read before the row plays.
    fn walk_chosen(
        &mut self,
        index: usize,
        place: Option<(u32, usize)>,
        running: i64,
        lanes: &LaneReads,
        held: WalkMove,
    ) -> WalkMove {
        self.tracks[index].walk = held;
        let Some((chain, place)) = place else {
            return held;
        };
        let current = Arc::clone(&self.current);
        let Some(owner) = current.song.tracks.get(index) else {
            return held;
        };
        let locks = |slot: &FxSlot| {
            WALK_SETTINGS
                .iter()
                .any(|id| slot.command == Some(FxCommand::Set(*id)))
        };
        let locked = owner
            .chain_position(chain, place, self.dealer(index), held)
            .and_then(|position| owner.step_at(position))
            .is_some_and(|step| step.fx.iter().any(locks));
        let held_lock = WALK_SETTINGS
            .iter()
            .any(|id| self.tracks[index].locked(*id));
        if !locked && !held_lock && lanes.iter().all(Option::is_none) {
            return held;
        }
        let row = self.row_ahead(index, running, lanes);
        let chosen = WALK_SETTINGS.map(|id| self.chosen_ahead(index, running, row, id, lanes));
        let walk = self.tracks[index].walk_of(|id| {
            let at = WALK_SETTINGS.iter().position(|walks| *walks == id);
            at.map_or(0, |at| chosen[at])
        });
        self.tracks[index].walk = walk;
        walk
    }

    /// How a track's settings move its phrase's walk at the step it's working out, or last worked
    /// out (decision 400).
    pub(super) fn walk(&self, index: usize) -> WalkMove {
        self.tracks[index].walk
    }

    /// How far a row's nudge moves its note part, in ticks: as far as it's written, kept to just
    /// under half the step it plays at, so a `RATE` to a faster step keeps its time unless that's
    /// too far, and brought toward the grid by the track's `QNT`, which is read before the row
    /// plays, as `GRV` is.
    fn row_nudge(&mut self, index: usize, running: i64, ticks: u64, lanes: &LaneReads) -> i64 {
        let Some(row @ (_, view)) = self.row_ahead(index, running, lanes) else {
            return 0;
        };
        if view.nudge == 0 {
            return 0;
        }
        let most = most_nudge(ticks);
        let written = i64::from(view.nudge.clamp(-most, most)) * NUDGE_TICKS as i64;
        let chosen = self.chosen_ahead(index, running, Some(row), SettingId::QUANTISE, lanes);
        self.tracks[index].quantised(written, chosen)
    }

    /// Where each FX column's lane reads at the step a track is working out, at this place in its
    /// chain and count since it started, read once, as that step is worked out
    /// ([`TrackState::lane_row`]).
    fn read_lanes(&mut self, index: usize, place: Option<(u32, usize)>, count: i64) -> LaneReads {
        let mut lanes = [None; FX_SLOTS];
        let Some((chain, place)) = place else {
            return lanes;
        };
        let current = Arc::clone(&self.current);
        let Some(owner) = current.song.tracks.get(index) else {
            return lanes;
        };
        let Some(position) =
            owner.chain_position(chain, place, self.dealer(index), self.walk(index))
        else {
            return lanes;
        };
        let Some(phrase) = owner.phrases.get(&position.phrase) else {
            return lanes;
        };
        let entry = (position.phrase, position.slot);
        for (slot, read) in lanes.iter_mut().enumerate() {
            if let Some(lane) = phrase.lane(slot) {
                *read = Some(self.tracks[index].lane_row(slot, lane, entry, count));
            }
        }
        lanes
    }

    /// The row a track plays at this count of its steps, looked at before it plays: where it is
    /// in its chain, and the row as it's heard, with its lanes where they read.
    fn row_ahead(
        &mut self,
        index: usize,
        running: i64,
        lanes: &LaneReads,
    ) -> Option<(ChainPosition, RowView)> {
        let (chain, place) = self.track_place(index, running)?;
        let current = Arc::clone(&self.current);
        let owner = current.song.tracks.get(index)?;
        let mut position =
            owner.chain_position(chain, place, self.dealer(index), self.walk(index))?;
        if let Some(row) = self.tracks[index]
            .jump
            .filter(|jump| jump.chain == chain && jump.start == place - position.pulse)
            .and_then(|jump| jump.row)
        {
            position.step = row;
        }
        let phrase = owner.phrases.get(&position.phrase)?;
        let step = owner.step_at(position)?;
        let count = running - self.tracks[index].started;
        let row = phrase.start() + position.step;
        let read = Read {
            chain,
            count,
            position,
            lanes: *lanes,
        };
        Some((
            position,
            self.view_row(index, read, row, step, Played::Ahead),
        ))
    }

    /// A stored row of the phrase a track is reading as it's heard: as written, with the cells of
    /// any of its generators the engine is moving laid over, the row of a step or of a lane's. A
    /// row `played`, rather than looked at before it plays, reports what was heard on it.
    pub(super) fn view_row(
        &mut self,
        index: usize,
        read: Read,
        row: usize,
        step: &Step,
        played: Played,
    ) -> RowView {
        let at = RowAt {
            track: index,
            phrase: read.position.phrase,
            row,
        };
        let (current, state) = (&self.current, &self.tracks[index]);
        let passes = || passes(&current.song, index, read);
        self.gens.view(
            &current.song,
            &current.unders,
            &state.gen_holds,
            at,
            step,
            passes,
            played,
        )
    }

    /// What a row chooses of a setting read before it plays, as `GRV`, `RATE` and `QNT` are: one
    /// on its row or in a lane at it, the last column's winning as they run left to right, or else
    /// what the track holds, which a note on the row's first pulse ends any lock of, as playing it
    /// will. A lane's row lasting several steps chooses on each, as its lock holds through it.
    fn chosen_ahead(
        &mut self,
        index: usize,
        running: i64,
        row: Option<(ChainPosition, RowView)>,
        id: SettingId,
        lanes: &LaneReads,
    ) -> u8 {
        let Some((position, view)) = row else {
            return self.tracks[index].held_choice(id, false);
        };
        let first = position.pulse == 0;
        let count = running - self.tracks[index].started;
        let current = Arc::clone(&self.current);
        let chain = self
            .track_place(index, running)
            .map_or(0, |(chain, _)| chain);
        let phrase = current
            .song
            .tracks
            .get(index)
            .and_then(|owner| owner.phrases.get(&position.phrase));
        let read = Read {
            chain,
            count,
            position,
            lanes: *lanes,
        };
        let mut chosen = None;
        for number in (0..FX_SLOTS).rev() {
            let slot = match (phrase, lanes[number]) {
                (Some(phrase), Some(lane)) => phrase.steps().get(lane.row).and_then(|step| {
                    self.view_row(index, read, lane.row, step, Played::Ahead)
                        .fx(number)
                        .copied()
                        .filter(|slot| lane.first || holds_through(slot))
                }),
                (_, None) if first => view.fx(number).copied(),
                (None, Some(_)) | (_, None) => None,
            };
            if let Some(slot) = slot.filter(|slot| slot.command == Some(FxCommand::Set(id))) {
                chosen = Some(slot.coarse());
                break;
            }
        }
        let note = first && matches!(view.note, Note::On(_)) && view.gate_kind() != GateKind::Rest;
        chosen.unwrap_or_else(|| self.tracks[index].held_choice(id, note))
    }

    /// The groove a track's step plays with and its speed, worked out before it's played, as each
    /// says when the step lands: a `GRV` or `RATE` on its row or in a lane at it, or else what the
    /// track holds (`chosen_ahead`). A `GRV` of `00` is the project's.
    fn step_choices(&mut self, index: usize, running: i64, lanes: &LaneReads) -> (Groove, Speed) {
        // With no `GRV` or `RATE` in the song, there's no row to look on, and the track holds its
        // own.
        let row = if self.current.groove_steps || self.current.speed_steps {
            self.row_ahead(index, running, lanes)
        } else {
            None
        };
        let groove = self.chosen_ahead(index, running, row, SettingId::GROOVE, lanes);
        let speed = self.chosen_ahead(index, running, row, SettingId::SPEED, lanes);
        let song = &self.current.song;
        let groove = match usize::from(groove).checked_sub(1) {
            Some(at) => GrooveId::from_index(at).unwrap_or(GrooveId::STRAIGHT),
            None => song.groove,
        };
        (song.groove(groove), Speed::from_index(usize::from(speed)))
    }

    /// Start or stop a track's note for a step, and run its FX left to right: all of it, or the
    /// part a nudge has it land apart, its lanes' values or its row's note part.
    pub(super) fn fire_track(&mut self, index: usize, pending: PendingStep) {
        let row = pending.part.plays_row();
        if row {
            self.tracks[index].retrigger = None;
        }
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some((chain, place)) = pending.place else {
            return;
        };
        let Some(owner) = song.tracks.get(index) else {
            return;
        };
        // Which time round its chain the track is, for a Steps slot that moves on a pass.
        if pending.leads {
            self.tracks[index].chain_pass = place as i64 / owner.chain_steps(chain).max(1) as i64;
            self.tracks[index].wake();
        }
        let Some(mut position) =
            owner.chain_position(chain, place, self.dealer(index), pending.walk)
        else {
            return;
        };
        if let Some(row) = pending.jumped {
            position.step = row;
        }
        let Some(written) = owner.step_at(position) else {
            return;
        };
        let phrase = owner.phrases.get(&position.phrase).map(Arc::as_ref);
        let count = pending.count;
        let read = Read {
            chain,
            count,
            position,
            lanes: pending.lanes,
        };
        let row_at = phrase.map_or(0, |phrase| phrase.start()) + position.step;
        let played = if position.pulse == 0 {
            Played::Row
        } else {
            Played::Ahead
        };
        let step = self.view_row(index, read, row_at, written, played);
        // A row's later pulses repeat its note only on a `Multi`, and run only the note's own timing.
        let first = position.pulse == 0;
        let gate = step.gate_kind();
        let mut fx = self.read_row_fx(index, phrase, read, &step);
        // `CHA` and `CND` go first, as whether the note plays decides whether locks end, with
        // the chances moved by the `CDLT` the row reads before its locks run.
        let moved = self.row_chance_delta(index, &fx.fx, &step, first);
        // With `Chance target` on `Pulse`, the share each of the row's pulses and ratchet hits
        // plays with (decision 406).
        let share = self
            .chance_by_pulse(index, &fx.fx, &step, first)
            .then(|| pulse_share(&fx.read, moved));
        let decided = match pending.plays {
            Some(decided) => decided,
            None => {
                let strikes = matches!(step.note, Note::On(_));
                self.decide_row(index, &fx.fx, &pending, position, strikes, moved, share)
            }
        };
        let plays = decided.note;
        let variation = if row && decided.fx {
            // An `RND` rolls the dice of the step its track's dice loop holds, if it's on.
            let rolls = pending.dice.map_or(count, |dice| dice.count);
            self.vary(index, &mut fx.fx, &fx.read, &fx.lane_steps, rolls, moved)
        } else {
            NoteVariation::default()
        };
        // What the row's note does on this pulse.
        let event = event_of(step.note, gate, first);
        let tying = first && self.tracks[index].tied && self.tracks[index].voice.active();
        // The last note's locks end where the step's first part lands, so a lane's lock on a step
        // whose note is nudged late lasts that note.
        if pending.leads && plays && first && matches!(event, Note::On(_)) && !tying {
            self.tracks[index].end_locks();
        }
        // A lane's values stay where the groove lands the step. What times the note goes with it,
        // wherever it's written. A failed `CND` skips the row's own FX, as Elektron's skips a
        // trig's locks, while a lane keeps to its own loop.
        let lane_steps = fx.lane_steps;
        let runs = |number: usize, slot: &FxSlot| match slot.command {
            Some(
                FxCommand::Del
                | FxCommand::Kil
                | FxCommand::Off
                | FxCommand::Ret
                | FxCommand::Cha
                | FxCommand::Cnd
                | FxCommand::Acu
                | FxCommand::Arp
                | FxCommand::Rnd
                | FxCommand::Sld,
            ) => row && decided.fx,
            _ if lane_steps[number].is_some() => pending.part.runs_lanes(),
            _ => row && decided.fx,
        };
        // An `SLD` slides the row's locks of continuous settings, and the next note's, rather
        // than having them jump (decision 417). A thinned row's locks run, so it slides too.
        let slides = first
            && fx.fx.iter().enumerate().any(|(number, slot)| {
                slot.command == Some(FxCommand::Sld) && slot.coarse() > 0 && runs(number, slot)
            });
        for (number, slot) in fx.fx.into_iter().enumerate() {
            let Some(command) = slot.command.filter(|_| runs(number, &slot)) else {
                continue;
            };
            match command {
                FxCommand::Del | FxCommand::Kil | FxCommand::Off | FxCommand::Ret => {
                    fx.time(command, slot.coarse());
                }
                FxCommand::Acu | FxCommand::Arp => fx.pitch(command, slot.coarse()),
                // Decided before any of them run, or read above.
                FxCommand::Cha | FxCommand::Cnd | FxCommand::Rnd | FxCommand::Sld => {}
                FxCommand::Set(_) if slides && slot.mark == Mark::Lock && command.glides() => {
                    self.slide(index, slot, phrase, read, pending.ticks);
                }
                FxCommand::Set(_) => self.lock(index, number, slot, phrase, read, pending.ticks),
            }
        }
        if slides {
            self.slide_into_note(index, &fx.fx, phrase, read, pending.ticks);
        }
        // A `MAC` lock on the row reaches what its macro holds of the generators on the row itself,
        // as it reaches any other setting: the macros are put now, and the row read again if that
        // moved a generator, so its note is heard with it.
        // A group's go onto its tracks at once too, which read their rows after it (decision 408).
        let reread = row && first && self.tracks[index].put_macros_now(owner);
        if row && first && self.current.group_macros.leads(index) {
            self.put_group_macros();
        }
        let (step, gate, event) = if reread {
            let step = self.view_row(index, read, row_at, written, played);
            let gate = step.gate_kind();
            (step, gate, event_of(step.note, gate, first))
        } else {
            (step, gate, event)
        };
        if !row {
            if !self.tracks[index].sounding() {
                self.tracks[index].settle(self.rate);
            }
            return;
        }
        let tie = tying && plays && fx.delay.is_none() && matches!(event, Note::On(_));
        // In `Legato`, a row's note arriving while the one sounding still has its gate open ties
        // into it as a `HLD` does, after this row's locks, so a lock of `Mode` here counts. A
        // `Multi`'s later pulses strike, as a ratchet's hits do (decision 380).
        let legato = !tie
            && first
            && plays
            && fx.delay.is_none()
            && matches!(event, Note::On(_))
            && self.ties_legato(index);
        if plays && fx.delay.is_none() && matches!(event, Note::On(_)) && !tie && !legato {
            self.start_modulation(index);
        }
        if !self.tracks[index].sounding() {
            self.tracks[index].settle(self.rate);
        }
        let start_frames = fx
            .delay
            .map_or(0, |units| self.groove_units(units, pending.ticks));
        let start_at = self.clock + start_frames;
        if first {
            self.tracks[index].tied =
                gate == GateKind::Hold && plays && matches!(event, Note::On(_));
            self.tracks[index].accumulated = None;
        }
        // A row's `ACU` adds once as it plays, or on each pulse of a `MLT` and each hit of a `RET`
        // as the track says. A note that's skipped, or rests, adds nothing.
        let accumulation = self.tracks[index].heard_accumulation(owner);
        let adds = first || accumulation.adds != Adds::Row;
        let key = RowKey {
            phrase: position.phrase,
            row: position.step,
        };
        let epoch = self.epoch(index, &pending, chain);
        let mut accumulating = None;
        match event {
            Note::Off if plays => {
                self.release_modulation(index);
                self.tracks[index].voice.release();
                self.tracks[index].retrigger = None;
                self.tracks[index].arp = None;
            }
            Note::Empty | Note::Off => {}
            Note::On(_) if !plays => {}
            Note::On(midi) => {
                let start = NoteStart {
                    key,
                    epoch,
                    transpose: position.transpose,
                    adds,
                    retriggers: accumulation.adds == Adds::Retrig,
                    tie,
                    at: start_at,
                };
                let (note, varied, moves) =
                    self.row_pitch(index, midi, &fx, variation, &pending, start);
                accumulating = moves;
                let velocity = self.row_velocity(index, step.velocity, &pending);
                let on = self.note_on(index, note, velocity, self.note_shape(index));
                // A row's note on a track with a slot on `Pitch`, or with an `RND` of its own on
                // its note, has what the slots and the `RND`s move it by reported, from the
                // phrase's first step, for `Space g A` to write in.
                if first
                    && on.is_some()
                    && (variation.rolled
                        || owner
                            .mods
                            .iter()
                            .any(|slot| slot.destination == Some(SettingId::PITCH)))
                {
                    let start = phrase.map_or(0, |phrase| phrase.start());
                    self.tracks[index].pitch_row = Some(RowKey {
                        row: start + key.row,
                        ..key
                    });
                    self.tracks[index].pitch_varied = varied;
                }
                self.start_row_note(index, on, tie, legato, start_at, start_frames > 0);
            }
        }
        // A note's gate closes its length of the way through its last pulse: a `Single`'s is its
        // row's last, a `Multi`'s note is one pulse long, and a `Hold` ties past it into the next
        // row. It's worked out after the FX have run, so a `LEN` or `LDLT` on this step counts,
        // and so does a lane's.
        if plays && matches!(event, Note::On(_)) && gate != GateKind::Hold {
            let pulses = if gate == GateKind::Multi {
                1
            } else {
                step.pulses()
            };
            self.tracks[index].gate_at = self
                .gate_frames(index, pulses, step.length(), &pending)
                .map(|frames| start_at + frames);
        }
        // Each hit rolls the dice of the step the dice loop holds, if it's on.
        let rolls = pending.dice.map_or(count, |dice| dice.count);
        let keep = share.map(|share| (rolls, share));
        self.time_note(index, &fx, start_at, pending.ticks, accumulating, keep);
    }

    /// The note a row starts, from the note as written: moved on by the track's accumulators, its
    /// own `ACU` with them if it adds now, then by its `RND`s, with its chain's transpose, or an
    /// arpeggio's note from there. With it, what the row's own `RND`s moved it by, in semitones,
    /// as it's heard, and what a `RET` moves it by on each hit, as the track says.
    fn row_pitch(
        &mut self,
        index: usize,
        midi: u8,
        fx: &RowFx,
        variation: NoteVariation,
        pending: &PendingStep,
        start: NoteStart,
    ) -> (u8, i8, Option<Accumulating>) {
        let NoteStart {
            key,
            epoch,
            transpose,
            ..
        } = start;
        let accumulates = fx.accumulates;
        let heard = self.accumulate(index, epoch, key, midi, accumulates.filter(|_| start.adds));
        // The row's `RND`s move the note from where the accumulators have it, so they don't add to
        // the walk.
        let moved = moved_note(heard, variation.by, self.scale);
        self.tracks[index].accumulated = (moved != midi).then_some(moved);
        let accumulating = accumulates
            .filter(|_| start.retriggers)
            .map(|amount| Accumulating {
                row: key,
                amount,
                written: midi,
                varied: variation.by,
                transpose,
                epoch,
            });
        let own = moved_note(heard, variation.own, self.scale);
        let varied = (i16::from(own) - i16::from(heard)) as i8;
        let heard = moved;
        let note = (i16::from(heard) + i16::from(transpose))
            .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
        let played = (heard, transpose);
        let arp = self.arpeggiate(index, pending, fx.arpeggiates, played, start.tie, start.at);
        self.tracks[index].arp = arp;
        let note = arp.map_or(note, |arp| arp.cycle.note(arp.place));
        (note, varied, accumulating)
    }

    /// A row's note's velocity: its own, as `VDLT` moves it after the FX have run so a lock on
    /// this step counts, scaled by the groove's velocity for the step.
    fn row_velocity(&self, index: usize, written: Option<u8>, pending: &PendingStep) -> u8 {
        let own = self.tracks[index].velocity(written.unwrap_or(MAX_VELOCITY));
        let own = u32::from(own);
        let percent = u32::from(pending.groove.velocity(pending.running));
        let velocity = if percent == u32::from(FULL_VELOCITY) {
            own
        } else {
            (own * percent + 50) / 100
        };
        velocity.min(u32::from(MAX_VELOCITY)) as u8
    }

    /// Whether a note starting now on a track ties into the one sounding by `Legato`: the note it
    /// would play, after this row's locks, is on the synth or sample sounding, whose gate is
    /// still open (decision 380).
    fn ties_legato(&self, index: usize) -> bool {
        self.tracks[index].legato()
            && self
                .note_on(index, 0, 0, self.note_shape(index))
                .is_some_and(|on| self.tracks[index].ties_legato(&on))
    }

    /// Start a row's note: tied into the one before, by a `Hold` or by `Legato`, held until its
    /// `DEL` has gone by, `delayed`, started now, or left to its tail while the track isn't heard.
    fn start_row_note(
        &mut self,
        index: usize,
        on: Option<NoteOn>,
        tie: bool,
        legato: bool,
        start_at: u64,
        delayed: bool,
    ) {
        match on {
            // A `Hold` before it ties into this note, with no new attack.
            Some(on) if tie => self.tracks[index].tie(on),
            // So does `Legato`, as a new row's note.
            Some(on) if legato => self.tracks[index].tie_legato(on),
            Some(on) if delayed => {
                self.tracks[index].last_note = Some(on);
                self.tracks[index].wake();
                self.tracks[index].delayed = Some(DelayedNote {
                    at: start_at,
                    note: on,
                });
            }
            Some(on) if self.plays(index) => {
                self.tracks[index].start(on, self.rate);
                self.choke(index);
            }
            Some(on) => {
                self.tracks[index].last_note = Some(on);
                self.tracks[index].wake();
                self.tracks[index].cut_to_tail();
            }
            None => self.tracks[index].cut_to_tail(),
        }
    }

    /// Read a row's FX columns at a pulse. Each reads its own lane's step if it has one, and the
    /// note's row, `step`, if not. What each reads is kept whether it runs on this pulse or not,
    /// for an `RND` to see what's to its left. A lane's row lasting several steps runs its FX on
    /// its first, as a row's own run on its first pulse, so a `RET` or a `CHA` acts once; a lock or
    /// a hold of it is put again on the rest, so it holds through the row though a note there ends
    /// the last's. A row's later pulses run only the note's own timing, and only on a `Multi`.
    fn read_row_fx(
        &mut self,
        index: usize,
        phrase: Option<&Phrase>,
        read: Read,
        step: &RowView,
    ) -> RowFx {
        let first = read.position.pulse == 0;
        let gate = step.gate_kind();
        let row_fx = |slot: &FxSlot| {
            first
                || gate == GateKind::Multi
                    && matches!(
                        slot.command,
                        Some(
                            FxCommand::Del
                                | FxCommand::Kil
                                | FxCommand::Off
                                | FxCommand::Ret
                                | FxCommand::Acu
                                | FxCommand::Arp
                                | FxCommand::Rnd
                        )
                    )
        };
        let mut fx = RowFx::default();
        for number in 0..FX_SLOTS {
            let found = match phrase.zip(read.lanes[number]) {
                Some((phrase, lane)) => {
                    fx.lane_steps[number] = Some(lane.row);
                    let played = match lane.first {
                        true => Played::Lane(number),
                        false => Played::Ahead,
                    };
                    let found = phrase.steps().get(lane.row).and_then(|step| {
                        self.view_row(index, read, lane.row, step, played)
                            .fx(number)
                            .copied()
                    });
                    fx.read[number] = found.unwrap_or_default();
                    found.filter(|slot| lane.first || holds_through(slot))
                }
                None => {
                    fx.read[number] = step.fx(number).copied().unwrap_or_default();
                    step.fx(number).copied().filter(|slot| row_fx(slot))
                }
            };
            fx.fx[number] = found.unwrap_or_default();
        }
        fx
    }

    /// The `CDLT` a row's chances are moved by, on the `00` to `FF` scale: a lock of it among the
    /// row's FX, or in a lane at it, the last column's winning as it would when the locks run,
    /// read before they do, or what the track holds, under any lock a note on the row's first
    /// pulse would end, with what modulation adds (decision 405).
    fn row_chance_delta(
        &self,
        index: usize,
        fx: &[FxSlot; FX_SLOTS],
        step: &RowView,
        first: bool,
    ) -> f32 {
        let own = fx
            .iter()
            .rev()
            .find(|slot| slot.command == Some(FxCommand::Set(SettingId::CHANCE_DELTA)))
            .map(|slot| f32::from(slot.value) / 256.0);
        let note = first && matches!(step.note, Note::On(_)) && step.gate_kind() != GateKind::Rest;
        self.tracks[index].chance_delta(own, note)
    }

    /// Whether a row's chance decides each of its pulses and ratchet hits rather than the whole
    /// row: `Chance target` on `Pulse`, a lock of it among the row's FX, or what the track holds,
    /// under any lock a note on the row's first pulse would end, with what modulation adds
    /// (decision 406).
    fn chance_by_pulse(
        &self,
        index: usize,
        fx: &[FxSlot; FX_SLOTS],
        step: &RowView,
        first: bool,
    ) -> bool {
        let own = fx
            .iter()
            .rev()
            .find(|slot| slot.command == Some(FxCommand::Set(SettingId::CHANCE_TARGET)))
            .map(|slot| f32::from(slot.coarse()));
        let note = first && matches!(step.note, Note::On(_)) && step.gate_kind() != GateKind::Rest;
        self.tracks[index].chance_target(own, note) >= 0.5
    }

    /// Decide a row's `CHA`s and `CND`s. A row's own are decided once, on its first pulse, by the
    /// step's first part to land, and later pulses keep what that decided. Every `CHA` is rolled
    /// whatever the conditions say, so its rolls are the same with or without them. Each `CHA`'s
    /// chance is moved by `moved`, the row's `CDLT`, and below `80` a row that `strikes` a note
    /// with no `CHA` and no condition counts as a `CHA FF` and thins too, rolled apart from the
    /// `CHA`s so theirs stay as they were; what that decides isn't a condition, so `PRE` and `NEI`
    /// don't read it (decision 405). With a `share`, `Chance target` on `Pulse`, the row's first
    /// pulse is decided so, and each later one that strikes is rolled with that share of its own,
    /// so a pulse dropped leaves the rest of the row; only its conditions decide the whole row
    /// (decision 406).
    #[allow(
        clippy::too_many_arguments,
        reason = "the row as it's read, and what moves its chances"
    )]
    fn decide_row(
        &mut self,
        index: usize,
        fx: &[FxSlot; FX_SLOTS],
        pending: &PendingStep,
        position: ChainPosition,
        strikes: bool,
        moved: f32,
        share: Option<f32>,
    ) -> Decided {
        let first = position.pulse == 0;
        let mut decided = if first {
            Decided::PLAYS
        } else {
            self.tracks[index].row
        };
        let mut chance = None;
        let seed = self.current.song.seed;
        for (column, slot) in fx.iter().enumerate() {
            if slot.command != Some(FxCommand::Cha) {
                continue;
            }
            // With the dice loop on, the dice of the step it holds; otherwise the track's stream.
            let rolled = match pending.dice {
                Some(dice) => {
                    let chance = moved_chance(f32::from(slot.coarse()), 255.0, moved);
                    looped_chance(seed, index, dice, column, chance)
                }
                None => self.tracks[index].rng.chance_moved(slot.coarse(), moved),
            };
            decided.note &= rolled;
            chance = Some(chance.unwrap_or(true) && rolled);
        }
        let conditioned = || {
            fx.iter().any(|slot| {
                slot.command == Some(FxCommand::Cnd)
                    && Condition::from_value(slot.coarse()).is_some()
            })
        };
        if first && strikes && moved < 128.0 && chance.is_none() && !conditioned() {
            // Thinned by the dice of the step the dice loop holds, if it's on.
            let rolls = pending.dice.map_or(pending.count, |dice| dice.count);
            decided.note &= thin_keeps(seed, index, rolls, moved);
        }
        let holds = self.conditions_hold(index, fx, pending, position, chance);
        if !holds {
            decided = Decided {
                note: false,
                fx: false,
            };
        }
        if first {
            // On pulses, the row's later pulses roll their own, so only its conditions carry on.
            self.tracks[index].row = match share {
                Some(_) => Decided {
                    note: holds,
                    fx: holds,
                },
                None => decided,
            };
        } else if let Some(share) = share
            && strikes
        {
            let seed = self.current.song.seed;
            let rolls = pending.dice.map_or(pending.count, |dice| dice.count);
            decided.note &= kept(seed, index, rolls, 0, share);
        }
        if pending.part != Part::Whole {
            self.tracks[index].decide(pending.step, decided);
        }
        decided
    }

    /// Put a column's lock, or start its glide to the next value of the same setting in the
    /// column, the row's or its lane's, which a lane's rows reach at its division heard.
    fn lock(
        &mut self,
        index: usize,
        number: usize,
        slot: FxSlot,
        phrase: Option<&Phrase>,
        read: Read,
        ticks: u64,
    ) {
        let Some(command @ FxCommand::Set(id)) = slot.command else {
            return;
        };
        if slot.mark != Mark::Glide {
            self.tracks[index].set_value(id, slot.value, slot.mark);
            return;
        }
        let lane = read.lanes[number];
        let rows = lane.map_or(1, |lane| lane.heard.division);
        let position = read.position;
        let to = phrase.and_then(|phrase| {
            match lane {
                Some(lane) => phrase.next_in_lane(number, lane.row, command),
                None => phrase.glide_to(
                    position.visit,
                    position.deal,
                    position.walk,
                    number,
                    command,
                ),
            }
            .map(|(steps, to)| (to, (steps * rows) as f64 * ticks as f64))
        });
        let now = self.song_time();
        self.tracks[index].start_glide(id, slot.value, to, now);
    }

    /// Slide a row's lock, under an `SLD`, from its value to where its setting next changes: the
    /// next lock of it on, or the track's own value at the next note, which would end the lock
    /// (decision 417). With nowhere to go it's a lock.
    fn slide(
        &mut self,
        index: usize,
        slot: FxSlot,
        phrase: Option<&Phrase>,
        read: Read,
        ticks: u64,
    ) {
        let Some(command @ FxCommand::Set(id)) = slot.command else {
            return;
        };
        let position = read.position;
        let to = phrase.and_then(|phrase| {
            phrase.slide_to(position.visit, position.deal, position.walk, command)
        });
        let Some((steps, to)) = to else {
            self.tracks[index].set_value(id, slot.value, Mark::Lock);
            return;
        };
        let now = self.song_time();
        let ticks = steps as f64 * ticks as f64;
        self.tracks[index].start_slide(id, Some(slot.value), to, ticks, now);
    }

    /// Slide what the next note locks and an `SLD` row doesn't, from where it's heard, so it
    /// arrives as that note starts, as the Analog Four slides a value locked on either trig
    /// (decision 417). A setting another row changes first, or in a column with a lane of its own,
    /// is left to jump.
    fn slide_into_note(
        &mut self,
        index: usize,
        own: &[FxSlot],
        phrase: Option<&Phrase>,
        read: Read,
        ticks: u64,
    ) {
        let Some(phrase) = phrase else {
            return;
        };
        let position = read.position;
        let (visit, deal, walk) = (position.visit, position.deal, position.walk);
        let Some((steps, next)) = phrase.next_note(visit, deal, walk) else {
            return;
        };
        let now = self.song_time();
        for (column, slot) in next.fx.iter().enumerate() {
            let Some(command @ FxCommand::Set(id)) = slot.command else {
                continue;
            };
            if !command.glides()
                || phrase.lane(column).is_some()
                || own.iter().any(|own| own.command == Some(command))
                || phrase
                    .slide_to(visit, deal, walk, command)
                    .map(|(at, _)| at)
                    != Some(steps)
            {
                continue;
            }
            let ticks = steps as f64 * ticks as f64;
            self.tracks[index].start_slide(id, None, Some(*slot), ticks, now);
        }
    }

    /// Time a row's note as its `KIL`, `OFF` and `RET` say, from where it starts, `start_at`. A
    /// `RET` with no units doesn't retrigger.
    fn time_note(
        &mut self,
        index: usize,
        fx: &RowFx,
        start_at: u64,
        ticks: u64,
        accumulates: Option<Accumulating>,
        keep: Option<(i64, f32)>,
    ) {
        if let Some(units) = fx.kill {
            self.tracks[index].kill_at = Some(start_at + self.groove_units(units, ticks));
        }
        if let Some(units) = fx.off {
            self.tracks[index].off_at = Some(start_at + self.groove_units(units, ticks));
        }
        if let Some(value) = fx.retrigger {
            let units = value & 0x0F;
            if units > 0 {
                let interval = self.groove_units(units, ticks).max(1);
                self.tracks[index].retrigger = Some(Retrigger {
                    next_at: start_at + interval,
                    interval,
                    factor: retrigger_factor(value >> 4),
                    accumulates,
                    keep,
                    hits: 0,
                });
            }
        }
    }

    /// Roll a row's `RND`s, left to right as they run, and vary what each has to its left: a
    /// lock's value, or whether a `RET` ratchets and its units, in `fx` before they run, and the
    /// note by what comes back. Each one's chance is moved by `moved`, the row's `CDLT`. A lock or a `RET` that doesn't run on this pulse isn't varied.
    fn vary(
        &self,
        index: usize,
        fx: &mut [FxSlot; FX_SLOTS],
        read: &[FxSlot; FX_SLOTS],
        lane_steps: &[Option<usize>; FX_SLOTS],
        count: i64,
        moved: f32,
    ) -> NoteVariation {
        let seed = self.current.song.seed;
        let mut variation = NoteVariation::default();
        for column in 0..FX_SLOTS {
            if fx[column].command != Some(FxCommand::Rnd) {
                continue;
            }
            let chance = Variation::of(fx[column].coarse());
            let roll = Roll::new(seed, index, count, column);
            let hits = roll.hits_moved(chance, moved);
            let left = column.saturating_sub(1);
            match varies_at(read, column) {
                Varies::Note => {
                    let own = lane_steps[column].is_none();
                    variation.rolled |= own;
                    if hits {
                        let by = roll.moved_by(u16::from(chance.range));
                        variation.by += by;
                        if own {
                            variation.own += by;
                        }
                    }
                }
                Varies::Nothing => {}
                _ if fx[left].command != read[left].command => {}
                Varies::Lock(id) if hits => {
                    fx[left].value = varied_lock(id, fx[left].value, chance.range, roll);
                }
                Varies::Lock(_) => {}
                Varies::Ratchet if hits => {
                    let units = varied_ratchet(fx[left].coarse(), chance.range, roll);
                    fx[left].value = u16::from(units) << 8;
                }
                // The `RET` doesn't ratchet this time.
                Varies::Ratchet => fx[left] = FxSlot::default(),
            }
        }
        variation
    }

    /// The arpeggio a note starting now plays, from the note as it's heard before its chain's
    /// transpose, which goes on after, an accumulator's move included, so the arpeggio moves with
    /// it. A `HLD` carries the one sounding into the note it ties into, keeping its place and its
    /// time, with that row's own `ARP` if it has one. Any other note starts one of its own, or has
    /// none.
    fn arpeggiate(
        &mut self,
        index: usize,
        pending: &PendingStep,
        value: Option<u8>,
        (heard, transpose): (u8, i8),
        tie: bool,
        start_at: u64,
    ) -> Option<Arpeggiating> {
        let carried = self.tracks[index].arp.filter(|_| tie);
        let value = value.or(carried.map(|arp| arp.value))?;
        let settings = self.tracks[index].heard_arpeggio(self.current.song.tracks.get(index)?);
        let cycle = Cycle::new(heard, value, settings.octaves, self.scale, transpose);
        if !cycle.moves() {
            return None;
        }
        if let Some(carried) = carried {
            return Some(Arpeggiating {
                cycle,
                value,
                ..carried
            });
        }
        let roll = if settings.mode == ArpMode::Random {
            self.tracks[index].arp_rng.next_u32()
        } else {
            0
        };
        let note_ticks = settings.rate.ticks();
        Some(Arpeggiating {
            cycle,
            value,
            mode: settings.mode,
            retrig: settings.retrig,
            from: start_at,
            played: 1,
            place: cycle.place(settings.mode, 0, roll),
            note_ticks,
            step_ticks: pending.ticks,
            next_at: start_at + self.frames_of(note_ticks, pending.ticks),
        })
    }

    /// Which epoch a step's accumulators count in: the start of its chain it was worked out after,
    /// and with a reset every so many loops, which group of them its count is in. With a reset only
    /// when playing starts, it's always the same one.
    fn epoch(&self, index: usize, pending: &PendingStep, chain: u32) -> Epoch {
        let Some(owner) = self.current.song.tracks.get(index) else {
            return (0, 0);
        };
        match self.tracks[index].heard_accumulation(owner).reset {
            Reset::Play => (0, 0),
            Reset::Chain => (pending.start, 0),
            Reset::Loops(loops) => {
                let length = owner.chain_steps(chain).max(1) as i64 * i64::from(loops.max(1));
                (pending.start, pending.count.div_euclid(length))
            }
        }
    }

    /// The note a row plays at, before its chain's transpose: the note as written, moved on by the
    /// track's accumulators. Its own `ACU`, if it adds now, adds first, unless this is its first
    /// play since they started again.
    pub(super) fn accumulate(
        &mut self,
        index: usize,
        epoch: Epoch,
        key: RowKey,
        written: u8,
        amount: Option<i8>,
    ) -> u8 {
        let Some(owner) = self.current.song.tracks.get(index) else {
            return written;
        };
        let state = &mut self.tracks[index];
        let settings = state.heard_accumulation(owner);
        state.accumulators.enter(epoch);
        if let Some(amount) = amount.filter(|amount| *amount != 0) {
            state
                .accumulators
                .add(key, amount, settings, &mut state.accumulator_rng);
        }
        let offset = state.accumulators.offset(key, settings.moves);
        let heard = moved_note(written, offset, self.scale);
        state.accumulated = (heard != written).then_some(heard);
        heard
    }

    /// Decide a row's conditions, left to right, as its first pulse lands, and keep what they and
    /// its `CHA` came to for `PRE` here and `NEI` on the track to the right to read next. The row
    /// plays only if every one holds. A value that isn't a condition is left out, as if it wasn't
    /// there.
    fn conditions_hold(
        &mut self,
        index: usize,
        fx: &[FxSlot; FX_SLOTS],
        pending: &PendingStep,
        position: ChainPosition,
        chance: Option<bool>,
    ) -> bool {
        let previous = self.tracks[index].passed;
        let neighbour = index
            .checked_sub(1)
            .is_some_and(|left| self.tracks[left].passed);
        let mut holds = true;
        let mut decided = chance;
        for (number, slot) in fx.iter().enumerate() {
            if slot.command != Some(FxCommand::Cnd) {
                continue;
            }
            let Some(condition) = Condition::from_value(slot.coarse()) else {
                continue;
            };
            let pass = match pending.lanes[number] {
                Some(lane) => lane.laps,
                None => self.row_pass(index, pending, position),
            };
            let last =
                matches!(condition, Condition::Last(_)) && self.last_pass(index, pending, position);
            let result = condition.holds(Situation {
                pass,
                fill: self.fill,
                previous,
                neighbour,
                last,
            });
            holds &= result;
            if !condition.reads_last() {
                decided = Some(decided.unwrap_or(true) && result);
            }
        }
        if let Some(passed) = decided {
            self.tracks[index].passed = passed;
        }
        holds
    }

    /// How many times the track has played this row before, since its chain started: once each
    /// time round the chain for each time its phrase is in it, so a phrase played four times in
    /// a row counts four passes, and the first is 0.
    fn row_pass(&self, index: usize, pending: &PendingStep, position: ChainPosition) -> i64 {
        let Some((chain, _)) = pending.place else {
            return 0;
        };
        let Some(owner) = self.current.song.tracks.get(index) else {
            return 0;
        };
        phrase_pass(owner, chain, pending.count, position)
    }

    /// Whether this pass of a row's phrase is the last before the track moves on, for `LST`
    /// (decision 419): before it plays another phrase, another chain or none, at the end of the
    /// pass or partway through it. What's queued counts, as on Elektron's machines a queued
    /// pattern change makes the pattern's pass its last: a chain launched on the track that lands
    /// by the pass's end, and a loop of sections waiting for its line. Then the section's end, if
    /// it comes by then, and the chain's next phrase. A chain started again, by a reset or a
    /// realign, isn't moving on, and nor is a window waiting for the pass under `Cue`.
    fn last_pass(&self, index: usize, pending: &PendingStep, position: ChainPosition) -> bool {
        let Some((chain, place)) = pending.place else {
            return false;
        };
        let song = Arc::clone(&self.current.song);
        let Some(owner) = song.tracks.get(index) else {
            return false;
        };
        let Some(phrase) = owner.phrases.get(&position.phrase) else {
            return false;
        };
        // The pass ends where its last step does, on the track's grid.
        let left = phrase.time().saturating_sub(position.time) as u64;
        let end = pending.step + left * pending.ticks;
        let state = &self.tracks[index];
        // A launch of the chain playing starts it again, which isn't moving on.
        let launch = state.cued.filter(|cued| *cued != Some(chain));
        if launch.is_some() {
            let lands = match song.launch {
                Launch::Every(steps) => self.next_line(u64::from(steps.get())),
                Launch::ChainEnd => {
                    let length = owner.chain_steps(chain).max(1);
                    pending.step + (length - place % length) as u64 * pending.ticks
                }
            };
            if lands <= end {
                return true;
            }
        }
        let count = song.sections.len();
        if count == 0 {
            return false;
        }
        let from = self.section.min(count - 1);
        let ends = self.section_end();
        // A loop of sections waiting lands at its line, or at the section's end.
        let (next, at) = match self.loop_cued {
            Some(sections) if !sections.holds(from) => {
                let line = self.next_line(self.realign_grid()).min(ends);
                (sections.first.min(count - 1), line)
            }
            looping => {
                let next = match looping.or(self.looping) {
                    Some(sections) if sections.first < count => sections.next(&song, from).0,
                    _ => next_row(&song, from).0,
                };
                (next, ends)
            }
        };
        if next != from && at <= end {
            let chain_next = match launch {
                Some(launched) => launched,
                None => song.sections[next].chains.get(index).copied().flatten(),
            };
            if chain_next != Some(chain) {
                return true;
            }
        }
        // The chain carries on, to the phrase in its next place.
        let Some(entries) = owner.chains.get(&chain).map(|chain| &chain.entries) else {
            return false;
        };
        let after = (1..=entries.len())
            .map(|ahead| entries[(position.slot + ahead) % entries.len()])
            .find(|entry| owner.phrases.contains_key(&entry.phrase));
        after.is_some_and(|entry| entry.phrase != position.phrase)
    }
}

/// A row's FX columns at a pulse, as `fire_track` reads them, and what its timing and pitch
/// commands leave for its note as they run left to right.
#[derive(Debug, Clone, Copy, Default)]
struct RowFx {
    /// What each column runs on this pulse.
    fx: [FxSlot; FX_SLOTS],
    /// What each column reads, whether it runs on this pulse or not.
    read: [FxSlot; FX_SLOTS],
    /// The row each column's lane reads, for those with one.
    lane_steps: [Option<usize>; FX_SLOTS],
    /// The units of a `DEL`, a `KIL` and an `OFF`, and a `RET`'s value.
    delay: Option<u8>,
    kill: Option<u8>,
    off: Option<u8>,
    retrigger: Option<u8>,
    /// What the row's `ACU`s add up to, and its `ARP`'s value.
    accumulates: Option<i8>,
    arpeggiates: Option<u8>,
}

impl RowFx {
    /// Take a `DEL`, `KIL`, `OFF` or `RET` that runs, the last column's winning.
    fn time(&mut self, command: FxCommand, value: u8) {
        match command {
            FxCommand::Del => self.delay = Some(value),
            FxCommand::Kil => self.kill = Some(value),
            FxCommand::Off => self.off = Some(value),
            FxCommand::Ret => self.retrigger = Some(value),
            _ => {}
        }
    }

    /// Take an `ACU` or `ARP` that runs. Two `ACU`s on a row add up, as far as one goes at once,
    /// and a value past that is left out, as if it wasn't there. The last column's `ARP` wins, as
    /// they run left to right.
    fn pitch(&mut self, command: FxCommand, value: u8) {
        match command {
            FxCommand::Acu => {
                if let Some(amount) = amount(value) {
                    let most = MOST_DEGREES as i8;
                    let sum = self.accumulates.unwrap_or(0).saturating_add(amount);
                    self.accumulates = Some(sum.clamp(-most, most));
                }
            }
            FxCommand::Arp => self.arpeggiates = Some(value),
            _ => {}
        }
    }
}

/// Where a row's note starts, for its pitch: its row, the epoch its accumulators count in and its
/// chain's transpose, whether its `ACU` adds now and on each hit of a `RET`, whether it ties into
/// the note before, and when it starts.
#[derive(Debug, Clone, Copy)]
struct NoteStart {
    key: RowKey,
    epoch: Epoch,
    transpose: i8,
    adds: bool,
    retriggers: bool,
    tie: bool,
    at: u64,
}

/// What a row's `RND`s do to its note, in scale degrees with the scale on and semitones with it
/// off.
#[derive(Debug, Clone, Copy, Default)]
struct NoteVariation {
    /// How far they move it, all of them.
    by: i16,
    /// How far those in the row's own columns move it, rather than a lane's, which `Space g A`
    /// writes in.
    own: i16,
    /// Whether one in the row's own columns rolled for its note, so what it did is reported.
    rolled: bool,
}

/// The share of the time each pulse and ratchet hit of a row plays when `Chance target` is on
/// `Pulse` (decision 406): its `CHA`s' chances, each moved by `moved`, the row's `CDLT`, together;
/// or with none, under `CDLT` below `80`, the share a row with no `CHA` keeps, unless a condition
/// decides it; or every time.
fn pulse_share(read: &[FxSlot; FX_SLOTS], moved: f32) -> f32 {
    let mut chances = read
        .iter()
        .filter(|slot| slot.command == Some(FxCommand::Cha))
        .peekable();
    if chances.peek().is_none() {
        let conditioned = read.iter().any(|slot| {
            slot.command == Some(FxCommand::Cnd) && Condition::from_value(slot.coarse()).is_some()
        });
        return match conditioned || moved >= 128.0 {
            true => 1.0,
            false => moved_chance(1.0, 1.0, moved),
        };
    }
    chances
        .map(|slot| moved_chance(f32::from(slot.coarse()), 255.0, moved) / 255.0)
        .product()
}

/// How much `RET`'s first digit changes the level on each retrigger. `0` is no change, 1-7 are
/// 10% to 70% quieter, and 8-F are 10% to 80% louder, compounding. `Acid` ignores it.
fn retrigger_factor(digit: u8) -> f32 {
    match digit {
        0 => 1.0,
        1..=7 => 1.0 - f32::from(digit) * 0.1,
        _ => 1.0 + f32::from(digit - 7) * 0.1,
    }
}

/// Where a track is reading a row: its chain, the count of its steps since that started, the
/// place in it, and where its lanes read at that step.
#[derive(Debug, Clone, Copy)]
pub(super) struct Read {
    pub chain: u32,
    pub count: i64,
    pub position: ChainPosition,
    pub lanes: LaneReads,
}

/// How many times a track has played its phrase before, since its chain started, at a count of its
/// steps: once each time round the chain for each time the phrase is in it. It's worked out from
/// where the track is, never counted along, so playing from a step hears the pass playing there
/// from the top would.
fn phrase_pass(owner: &Track, chain: u32, count: i64, position: ChainPosition) -> i64 {
    let Some(entries) = owner.chains.get(&chain).map(|chain| &chain.entries) else {
        return 0;
    };
    let length = owner.chain_steps(chain).max(1) as i64;
    let times = |entries: &[vt_model::song::ChainEntry]| {
        entries
            .iter()
            .filter(|entry| entry.phrase == position.phrase)
            .count() as i64
    };
    let before = times(entries.get(..position.slot).unwrap_or_default());
    count.div_euclid(length) * times(entries) + before
}

/// The pass a row is read on, for its phrase's generators: of the phrase, and of each FX column's
/// lane, as many times round the length it's heard at as the lane has been.
fn passes(song: &Song, index: usize, read: Read) -> Passes {
    let Some(owner) = song.tracks.get(index) else {
        return Passes::default();
    };
    Passes {
        rows: phrase_pass(owner, read.chain, read.count, read.position),
        lanes: read.lanes.map(|lane| lane.map_or(0, |lane| lane.laps)),
    }
}

/// Whether a lane's FX is put again on its row's later steps: a lock or a hold, which holds through
/// the row, and not an event or a glide, which act once, as its row starts.
fn holds_through(slot: &FxSlot) -> bool {
    matches!(slot.command, Some(FxCommand::Set(_))) && slot.mark != Mark::Glide
}

/// What a row's note does on a pulse: a rest's ends the note on its first, a `Multi` plays on
/// every one, and any other plays on its first alone.
fn event_of(note: Note, gate: GateKind, first: bool) -> Note {
    match (note, gate) {
        (Note::On(_), GateKind::Rest) if first => Note::Off,
        (Note::On(_), GateKind::Rest) => Note::Empty,
        (Note::On(midi), GateKind::Multi) => Note::On(midi),
        (note, _) if first => note,
        _ => Note::Empty,
    }
}
