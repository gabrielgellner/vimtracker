//! Working out a track's step and playing it: the row it lands on, its nudge, its
//! conditions, its accumulator and its arpeggio.

use std::sync::Arc;

use vt_model::accumulator::{Adds, MOST_DEGREES, Reset, amount, moved_note};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::condition::{Condition, Situation};
use vt_model::constants::{FX_SLOTS, NUDGE_TICKS, TICKS_PER_STEP};
use vt_model::groove::{FULL_VELOCITY, Groove, GrooveId, MAX_PUSH};
use vt_model::settings::SettingId;
use vt_model::shape::RowView;
use vt_model::song::{ChainPosition, Song, Track};
use vt_model::speed::Speed;
use vt_model::step::{
    FxCommand, FxSlot, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Mark, Note, Step, most_nudge,
};

use crate::gens::{Passes, Played, RowAt};
use crate::track_state::{
    Accumulating, Arpeggiating, Decided, DelayedNote, Epoch, Part, PendingStep, Retrigger, RowKey,
    TrackState, Worked,
};

use super::Engine;

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
        // The groove counts the track's own steps from the start or the last reset, not its
        // loop, so an odd length stays on the beat, and a track at an eighth swings its eighths.
        let (groove, ticks) = self.step_choices(index, running);
        let humanise = self.tracks[index].humanise(groove.humanise());
        let at = running.max(0) as usize;
        let push = (i64::from(groove.push(at)) + humanise).clamp(-groove_lead, i64::from(MAX_PUSH));
        let place = self.track_place(index, running);
        let step = PendingStep {
            at: self.landing(grid, push, ticks),
            running: at,
            groove,
            ticks,
            part: Part::Whole,
            step: grid,
            leads: true,
            place,
            count: running - self.tracks[index].started,
            plays: None,
            start: self.tracks[index].chain_starts,
        };
        // Grid, then groove, then nudge. A track's notes play in order, so a row's note part
        // never lands before the last row's. The push is a share of the step and the nudge the
        // same time on every track, so they're added as ticks times a sixteenth's.
        let row_at = if nudges {
            let nudge = self.row_nudge(index, running, ticks);
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

    /// How far a row's nudge moves its note part, in ticks: as far as it's written, kept to just
    /// under half the step it plays at, so a `RATE` to a faster step keeps its time unless that's
    /// too far, and brought toward the grid by the track's `QNT`, which is read before the row
    /// plays, as `GRV` is.
    fn row_nudge(&mut self, index: usize, running: i64, ticks: u64) -> i64 {
        let Some(row @ (_, view)) = self.row_ahead(index, running) else {
            return 0;
        };
        if view.nudge == 0 {
            return 0;
        }
        let most = most_nudge(ticks);
        let written = i64::from(view.nudge.clamp(-most, most)) * NUDGE_TICKS as i64;
        let chosen = self.chosen_ahead(index, running, Some(row), SettingId::QUANTISE);
        self.tracks[index].quantised(written, chosen)
    }

    /// The row a track plays at this count of its steps, looked at before it plays: where it is
    /// in its chain, and the row as it's heard.
    fn row_ahead(&mut self, index: usize, running: i64) -> Option<(ChainPosition, RowView)> {
        let (chain, place) = self.track_place(index, running)?;
        let current = Arc::clone(&self.current);
        let owner = current.song.tracks.get(index)?;
        let position = owner.chain_position(chain, place, self.dealer(index))?;
        let phrase = owner.phrases.get(&position.phrase)?;
        let step = owner.step_at(position)?;
        let count = running - self.tracks[index].started;
        let row = phrase.start() + position.step;
        let read = Read {
            chain,
            count,
            position,
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
        let passes = || passes(&current.song, state, index, read);
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
    /// will.
    fn chosen_ahead(
        &mut self,
        index: usize,
        running: i64,
        row: Option<(ChainPosition, RowView)>,
        id: SettingId,
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
        };
        let mut chosen = None;
        for number in (0..FX_SLOTS).rev() {
            let slot = match phrase.and_then(|phrase| Some((phrase, phrase.lane(number)?))) {
                Some((phrase, lane)) => {
                    let at = lane.step_at(count + self.tracks[index].lane_offsets[number]);
                    phrase.steps().get(at).and_then(|step| {
                        self.view_row(index, read, at, step, Played::Ahead)
                            .fx(number)
                            .copied()
                    })
                }
                None if first => view.fx(number).copied(),
                None => None,
            };
            if let Some(slot) = slot.filter(|slot| slot.command == Some(FxCommand::Set(id))) {
                chosen = Some(slot.coarse());
                break;
            }
        }
        let note = first && matches!(view.note, Note::On(_)) && view.gate_kind() != GateKind::Rest;
        chosen.unwrap_or_else(|| self.tracks[index].held_choice(id, note))
    }

    /// The groove a track's step plays with and how long it is, in ticks, worked out before it's
    /// played, as each says when the step lands: a `GRV` or `RATE` on its row or in a lane at it,
    /// or else what the track holds (`chosen_ahead`). A `GRV` of `00` is the project's.
    fn step_choices(&mut self, index: usize, running: i64) -> (Groove, u64) {
        // With no `GRV` or `RATE` in the song, there's no row to look on, and the track holds its
        // own.
        let row = if self.current.groove_steps || self.current.speed_steps {
            self.row_ahead(index, running)
        } else {
            None
        };
        let groove = self.chosen_ahead(index, running, row, SettingId::GROOVE);
        let speed = self.chosen_ahead(index, running, row, SettingId::SPEED);
        let song = &self.current.song;
        let groove = match usize::from(groove).checked_sub(1) {
            Some(at) => GrooveId::from_index(at).unwrap_or(GrooveId::STRAIGHT),
            None => song.groove,
        };
        let speed = Speed::from_index(usize::from(speed));
        (song.groove(groove), speed.ticks())
    }

    /// Start or stop a track's note for a step, and run its FX left to right: all of it, or the
    /// part a nudge has it land apart, its lanes' values or its row's note part.
    pub(super) fn fire_track(&mut self, index: usize, pending: PendingStep) {
        let running = pending.running;
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
        let Some(position) = owner.chain_position(chain, place, self.dealer(index)) else {
            return;
        };
        let Some(written) = owner.step_at(position) else {
            return;
        };
        let phrase = owner.phrases.get(&position.phrase);
        let count = pending.count;
        let read = Read {
            chain,
            count,
            position,
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
                        )
                    )
        };
        // Each FX column reads its own lane's step if it has one, and the note's row if not.
        let mut lane_steps = [None; FX_SLOTS];
        let mut fx = [FxSlot::default(); FX_SLOTS];
        for (number, slot) in fx.iter_mut().enumerate() {
            let lane = phrase.and_then(|phrase| Some((phrase, phrase.lane(number)?)));
            let found = match lane {
                Some((phrase, lane)) => {
                    let at = lane.step_at(count + self.tracks[index].lane_offsets[number]);
                    lane_steps[number] = Some(at);
                    phrase.steps().get(at).and_then(|step| {
                        self.view_row(index, read, at, step, Played::Lane(number))
                            .fx(number)
                            .copied()
                    })
                }
                None => step.fx(number).copied().filter(|slot| row_fx(slot)),
            };
            *slot = found.unwrap_or_default();
        }
        let mut delay_units = None;
        let mut kill_units = None;
        let mut off_units = None;
        let mut retrigger = None;
        let mut accumulates: Option<i8> = None;
        let mut arpeggiates = None;
        // `CHA` and `CND` go first, as whether the note plays decides whether locks end. A row's
        // own are decided once, on its first pulse, by the step's first part to land. Every `CHA`
        // is rolled whatever the conditions say, so its rolls are the same with or without them.
        let decided = match pending.plays {
            Some(decided) => decided,
            None => {
                let mut decided = if first {
                    Decided::PLAYS
                } else {
                    self.tracks[index].row
                };
                let mut chance = None;
                for slot in fx.iter().filter(|s| s.command == Some(FxCommand::Cha)) {
                    let rolled = self.tracks[index].rng.chance(slot.coarse());
                    decided.note &= rolled;
                    chance = Some(chance.unwrap_or(true) && rolled);
                }
                if !self.conditions_hold(index, &fx, &lane_steps, &pending, position, chance) {
                    decided = Decided {
                        note: false,
                        fx: false,
                    };
                }
                if first {
                    self.tracks[index].row = decided;
                }
                if pending.part != Part::Whole {
                    self.tracks[index].decide(pending.step, decided);
                }
                decided
            }
        };
        let plays = decided.note;
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
        let runs = |number: usize, slot: &FxSlot| match slot.command {
            Some(
                FxCommand::Del
                | FxCommand::Kil
                | FxCommand::Off
                | FxCommand::Ret
                | FxCommand::Cha
                | FxCommand::Cnd
                | FxCommand::Acu
                | FxCommand::Arp,
            ) => row && decided.fx,
            _ if lane_steps[number].is_some() => pending.part.runs_lanes(),
            _ => row && decided.fx,
        };
        for (number, slot) in fx.iter().enumerate() {
            let Some(command) = slot.command.filter(|_| runs(number, slot)) else {
                continue;
            };
            match command {
                FxCommand::Del => delay_units = Some(slot.coarse()),
                FxCommand::Kil => kill_units = Some(slot.coarse()),
                FxCommand::Off => off_units = Some(slot.coarse()),
                FxCommand::Ret => retrigger = Some(slot.coarse()),
                FxCommand::Cha | FxCommand::Cnd => {}
                // Two on a row add up, as far as an `ACU` goes at once. A value past that is left
                // out, as if it wasn't there.
                FxCommand::Acu => {
                    if let Some(amount) = amount(slot.coarse()) {
                        let most = MOST_DEGREES as i8;
                        let sum = accumulates.unwrap_or(0).saturating_add(amount);
                        accumulates = Some(sum.clamp(-most, most));
                    }
                }
                // The last column's wins, as they run left to right.
                FxCommand::Arp => arpeggiates = Some(slot.coarse()),
                FxCommand::Set(id) if slot.mark == Mark::Glide => {
                    let value = command;
                    let to = phrase.and_then(|phrase| {
                        match lane_steps[number] {
                            Some(at) => phrase.next_in_lane(number, at, value),
                            None => phrase.glide_to(position.visit, position.deal, number, value),
                        }
                        .map(|(steps, to)| (to, steps as f64 * pending.ticks as f64))
                    });
                    let now = self.song_time();
                    self.tracks[index].start_glide(id, slot.value, to, now);
                }
                FxCommand::Set(id) => self.tracks[index].set_value(id, slot.value, slot.mark),
            }
        }
        // A `MAC` lock on the row reaches what its macro holds of the generators on the row itself,
        // as it reaches any other setting: the macros are put now, and the row read again if that
        // moved a generator, so its note is heard with it (decision 300).
        let (step, gate, event) = if row && first && self.tracks[index].put_macros_now(owner) {
            let step = self.view_row(index, read, row_at, written, played);
            let gate = step.gate_kind();
            (step, gate, event_of(step.note, gate, first))
        } else {
            (step, gate, event)
        };
        if !row {
            if !self.tracks[index].sounding() {
                self.tracks[index].settle();
            }
            return;
        }
        let tie = tying && plays && delay_units.is_none() && matches!(event, Note::On(_));
        if plays && delay_units.is_none() && matches!(event, Note::On(_)) && !tie {
            self.start_modulation(index);
        }
        if !self.tracks[index].sounding() {
            self.tracks[index].settle();
        }
        let start_frames = delay_units.map_or(0, |units| self.groove_units(units, pending.ticks));
        let start_at = self.clock + start_frames;
        if first {
            self.tracks[index].tied =
                gate == GateKind::Hold && plays && matches!(event, Note::On(_));
            self.tracks[index].accumulated = None;
        }
        // A row's `ACU` adds once as it plays, or on each pulse of a `MLT` and each hit of a `RET`
        // as the track says. A note that's skipped, or rests, adds nothing.
        let accumulation = owner.accumulation;
        let adds = first || accumulation.adds != Adds::Row;
        let key = RowKey {
            phrase: position.phrase,
            row: position.step,
        };
        let epoch = self.epoch(index, &pending, chain);
        let mut accumulating = None;
        match event {
            Note::Empty => {}
            Note::Off if plays => {
                self.release_modulation(index);
                self.tracks[index].voice.release();
                self.tracks[index].retrigger = None;
                self.tracks[index].arp = None;
            }
            Note::Off => {}
            Note::On(_) if !plays => {}
            Note::On(midi) => {
                let heard = self.accumulate(index, epoch, key, midi, accumulates.filter(|_| adds));
                if accumulation.adds == Adds::Retrig {
                    accumulating = accumulates.map(|amount| Accumulating {
                        row: key,
                        amount,
                        written: midi,
                        transpose: position.transpose,
                        epoch,
                    });
                }
                let note = (i16::from(heard) + i16::from(position.transpose))
                    .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE))
                    as u8;
                let played = (heard, position.transpose);
                let arp = self.arpeggiate(index, &pending, arpeggiates, played, tie, start_at);
                self.tracks[index].arp = arp;
                let note = arp.map_or(note, |arp| arp.cycle.note(arp.place));
                // `VDLT` moves the note's own, after the FX have run so a lock on this step
                // counts, and the groove's velocity for the step scales that.
                let own = self.tracks[index].velocity(step.velocity.unwrap_or(MAX_VELOCITY));
                let own = u32::from(own);
                let percent = u32::from(pending.groove.velocity(running));
                let velocity = if percent == u32::from(FULL_VELOCITY) {
                    own
                } else {
                    (own * percent + 50) / 100
                };
                let velocity = velocity.min(u32::from(MAX_VELOCITY)) as u8;
                let on = self.note_on(index, note, velocity, self.note_shape(index));
                // A row's note on a track with a slot on `Pitch` has what the slots move it by
                // reported, from the phrase's first step, for `Space g A` to write in.
                if first
                    && on.is_some()
                    && owner
                        .mods
                        .iter()
                        .any(|slot| slot.destination == Some(SettingId::PITCH))
                {
                    let start = phrase.map_or(0, |phrase| phrase.start());
                    self.tracks[index].pitch_row = Some(RowKey {
                        row: start + key.row,
                        ..key
                    });
                }
                match on {
                    // A `Hold` before it ties into this note, with no new attack.
                    Some(on) if tie => self.tracks[index].tie(on),
                    Some(on) if start_frames > 0 => {
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
        if let Some(units) = kill_units {
            self.tracks[index].kill_at = Some(start_at + self.groove_units(units, pending.ticks));
        }
        if let Some(units) = off_units {
            self.tracks[index].off_at = Some(start_at + self.groove_units(units, pending.ticks));
        }
        if let Some(value) = retrigger {
            let units = value & 0x0F;
            if units > 0 {
                let interval = self.groove_units(units, pending.ticks).max(1);
                self.tracks[index].retrigger = Some(Retrigger {
                    next_at: start_at + interval,
                    interval,
                    factor: retrigger_factor(value >> 4),
                    accumulates: accumulating,
                });
            }
        }
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
        match owner.accumulation.reset {
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
        lane_steps: &[Option<usize>; FX_SLOTS],
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
            let pass = match lane_steps[number] {
                Some(_) => self.lane_pass(index, number, pending, position),
                None => self.row_pass(index, pending, position),
            };
            let result = condition.holds(Situation {
                pass,
                fill: self.fill,
                previous,
                neighbour,
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

    /// How many times round its own loop a lane has been since the track's chain started.
    fn lane_pass(
        &self,
        index: usize,
        number: usize,
        pending: &PendingStep,
        position: ChainPosition,
    ) -> i64 {
        let lane = self
            .current
            .song
            .tracks
            .get(index)
            .and_then(|owner| owner.phrases.get(&position.phrase))
            .and_then(|phrase| phrase.lane(number));
        let Some(lane) = lane else {
            return 0;
        };
        let count = pending.count + self.tracks[index].lane_offsets[number];
        count.div_euclid(lane.length.max(1) as i64)
    }
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

/// Where a track is reading a row: its chain, the count of its steps since that started, and the
/// place in it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Read {
    pub chain: u32,
    pub count: i64,
    pub position: ChainPosition,
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
/// lane, as many times round its own loop as the lane has been.
fn passes(song: &Song, state: &TrackState, index: usize, read: Read) -> Passes {
    let Some(owner) = song.tracks.get(index) else {
        return Passes::default();
    };
    let phrase = owner.phrases.get(&read.position.phrase);
    Passes {
        rows: phrase_pass(owner, read.chain, read.count, read.position),
        lanes: std::array::from_fn(|number| {
            let lane = phrase.and_then(|phrase| phrase.lane(number));
            lane.map_or(0, |lane| {
                (read.count + state.lane_offsets[number]).div_euclid(lane.length.max(1) as i64)
            })
        }),
    }
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
