//! When each step and each track's own events fall, in frames, and firing them as they do.

use vt_model::arpeggio::ArpMode;
use vt_model::constants::{GROOVE_UNIT_TICKS, MAX_TRACKS, TICKS_PER_QUARTER, TICKS_PER_STEP};
use vt_model::song::SynthVoice;
use vt_model::step::{MAX_NOTE, MIN_NOTE};

use crate::track_state::{Arpeggiating, PendingStep, Retrigger};
use crate::voice::NoteOn;

use super::{Engine, sample_of_tick};

impl Engine {
    /// Frames until the next step is worked out: its place on the grid, or as far before it as
    /// a groove can land a step early.
    pub(super) fn frames_until_next_step(&self) -> u64 {
        if !self.has_steps() {
            return u64::MAX;
        }
        self.frames_until_worked_out(self.next_step * TICKS_PER_STEP)
    }

    /// Frames until a step at this place on the grid is worked out.
    fn frames_until_worked_out(&self, grid: u64) -> u64 {
        let tick = grid
            .saturating_sub(self.current.lead)
            .saturating_sub(self.origin_tick);
        sample_of_tick(tick, self.rate, self.millibpm).saturating_sub(self.frames_since_origin)
    }

    /// The track whose own step falls soonest before the song's next step, and frames until it's
    /// worked out. One whose next step falls with the song's is worked out with it.
    pub(super) fn next_track_step(&self) -> Option<(usize, u64)> {
        let next = self.next_step * TICKS_PER_STEP;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        let (index, beat) = (0..tracks)
            .map(|index| (index, self.tracks[index].clock.beat))
            .filter(|(_, beat)| *beat < next)
            .min_by_key(|(index, beat)| (*beat, *index))?;
        if !self.has_steps() {
            return None;
        }
        Some((index, self.frames_until_worked_out(beat)))
    }

    /// Frames in so many ticks of a sixteenth, taken as shares of a step this many ticks long, so
    /// a push, a nudge or a `DEL` on a slower track is that much longer. At a sixteenth it's the
    /// frames in the ticks, exactly.
    pub(super) fn frames_of(&self, ticks: u64, step: u64) -> u64 {
        let numerator = u128::from(ticks) * u128::from(step) * 60 * u128::from(self.rate) * 1000;
        let denominator =
            u128::from(TICKS_PER_STEP) * u128::from(TICKS_PER_QUARTER) * u128::from(self.millibpm);
        (numerator / denominator) as u64
    }

    /// Where on the clock a step lands: its place on the grid, moved by its push in ticks of a
    /// step this many ticks long. Each part is worked out from the tempo's origin, so a step on
    /// the grid lands where it always did, and swing's delay is what it always was.
    pub(super) fn landing(&self, grid: u64, push: i64, step: u64) -> u64 {
        let on_grid = sample_of_tick(
            grid.saturating_sub(self.origin_tick),
            self.rate,
            self.millibpm,
        );
        let moved = self.frames_of(push.unsigned_abs(), step);
        let lands = if push >= 0 {
            on_grid + moved
        } else {
            on_grid.saturating_sub(moved)
        };
        (self.clock + lands).saturating_sub(self.frames_since_origin)
    }

    /// Frames until a track's delayed, cut, release or retrigger event.
    pub(super) fn frames_until_track_event(&self) -> u64 {
        let mut soonest = u64::MAX;
        for track in self.tracks.iter() {
            soonest = soonest.min(track.soonest().saturating_sub(self.clock));
            let events = [
                track.delayed.map(|d| d.at),
                track.kill_at,
                track.off_at,
                track.gate_at,
                track.retrigger.map(|r| r.next_at),
                track.arp.map(|arp| arp.next_at),
            ];
            for at in events.into_iter().flatten() {
                soonest = soonest.min(at.saturating_sub(self.clock));
            }
        }
        soonest
    }

    /// How long a note of so many pulses is held for, from its start, at its row's own length or
    /// else the track's `Gate`, or `None` at 100%, which holds it until something ends it, as
    /// before there was a `Gate`. Its pulses are steps as the groove lands them, so it runs to
    /// where its last pulse lands, and then its share of how long the groove makes that pulse,
    /// each as long as the track's speed makes a step.
    pub(super) fn gate_frames(
        &self,
        index: usize,
        pulses: usize,
        own: Option<u8>,
        step: &PendingStep,
    ) -> Option<u64> {
        let share = self.tracks[index].gate_share(own);
        if share >= 1.0 {
            return None;
        }
        let (groove, first) = (&step.groove, step.running);
        let last = first + pulses.saturating_sub(1);
        let ticks = TICKS_PER_STEP as f64;
        let moved = f64::from(groove.push(last)) - f64::from(groove.push(first));
        let length = groove.length(last) as f64 / ticks;
        let pulses = pulses.saturating_sub(1) as f64 + moved / ticks + f64::from(share) * length;
        let step = step.ticks as f64 * 60.0 * f64::from(self.rate) * 1000.0
            / (TICKS_PER_QUARTER as f64 * f64::from(self.millibpm));
        Some((pulses * step).round() as u64)
    }

    /// Frames in so many of `DEL`'s units, sixths of a step this many ticks long.
    pub(super) fn groove_units(&self, units: u8, step: u64) -> u64 {
        self.frames_of(u64::from(units) * GROOVE_UNIT_TICKS, step)
    }

    /// Whether a track is heard by the mutes and solos: a sound track by its group's too, and a
    /// group by its tracks'.
    pub(super) fn audible(&self, index: usize) -> bool {
        let any_solo = self.any_solo();
        match index {
            _ if index < self.sounds() => self.heard_with(index, any_solo),
            _ if index < self.returns() => self.group_heard_with(index, any_solo),
            _ => {
                let state = &self.tracks[index];
                !state.mute && (!any_solo || state.solo)
            }
        }
    }

    /// Run anything due now on each track's own schedule.
    pub(super) fn fire_due_events(&mut self) {
        let clock = self.clock;
        for index in 0..MAX_TRACKS {
            while let Some(part) = self.tracks[index].take_due(clock) {
                self.fire_track(index, part);
            }
            if let Some(delayed) = self.tracks[index].delayed
                && delayed.at <= clock
            {
                self.tracks[index].delayed = None;
                self.start_modulation(index);
                if self.plays(index) {
                    let (kill, off, gate, retrigger) = {
                        let t = &self.tracks[index];
                        (t.kill_at, t.off_at, t.gate_at, t.retrigger)
                    };
                    self.tracks[index].start(delayed.note, self.rate);
                    self.choke(index);
                    let t = &mut self.tracks[index];
                    t.kill_at = kill;
                    t.off_at = off;
                    t.gate_at = gate;
                    t.retrigger = retrigger;
                }
            }
            // Whatever ends the note ends its arpeggio.
            if self.tracks[index].kill_at.is_some_and(|at| at <= clock) {
                self.tracks[index].kill_at = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].cut_to_tail();
            }
            if self.tracks[index].off_at.is_some_and(|at| at <= clock) {
                self.tracks[index].off_at = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].voice.release();
            }
            // The gate closing lets the note go as `OFF` does, and ends its ratchets, which only
            // repeat while the gate's open.
            if self.tracks[index].gate_at.is_some_and(|at| at <= clock) {
                self.tracks[index].gate_at = None;
                self.tracks[index].retrigger = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].voice.release();
            }
            // An arpeggio's note goes before a ratchet due with it, so the ratchet strikes it.
            if let Some(arp) = self.tracks[index].arp
                && arp.next_at <= clock
            {
                self.fire_arp(index, arp);
            }
            if let Some(retrigger) = self.tracks[index].retrigger
                && retrigger.next_at <= clock
            {
                self.fire_retrigger(index, retrigger);
            }
        }
    }

    fn fire_retrigger(&mut self, index: usize, retrigger: Retrigger) {
        let rate = self.rate;
        let audible = self.plays(index);
        if self.tracks[index].last_note.is_some() {
            self.start_modulation(index);
        }
        // With the track's `ACU`s adding on each hit, each moves the row's note on.
        let moved = match (retrigger.accumulates, self.tracks[index].last_note) {
            (Some(row), Some(_)) => {
                let heard =
                    self.accumulate(index, row.epoch, row.row, row.written, Some(row.amount));
                Some(
                    (i16::from(heard) + i16::from(row.transpose))
                        .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8,
                )
            }
            _ => None,
        };
        let track = &mut self.tracks[index];
        let Some(note) = track.last_note else {
            track.retrigger = None;
            return;
        };
        // `Acid` has no level but its accent, as a 303 has none, so each hit plays at the note's
        // own level, plain or accented, and `RET`'s fade and swell leave it alone (decision 124).
        // The accent carries on to every hit with the rest of the note.
        let gain = if note.synth == Some(SynthVoice::Acid) {
            note.gain
        } else {
            (note.gain * retrigger.factor).clamp(0.0, 1.0)
        };
        let next = NoteOn {
            gain,
            note: moved.unwrap_or(note.note),
            ..note
        };
        let (kill, off, gate) = (track.kill_at, track.off_at, track.gate_at);
        if audible {
            track.start(next, rate);
        }
        track.kill_at = kill;
        track.off_at = off;
        track.gate_at = gate;
        track.last_note = Some(next);
        track.wake();
        track.retrigger = Some(Retrigger {
            next_at: retrigger.next_at + retrigger.interval,
            ..retrigger
        });
    }

    /// Play an arpeggio's next note: as a tie, which changes only the pitch, or with `Retrig` on,
    /// starting the note and its envelopes again. Each note is counted from the first, so a long
    /// note's don't drift.
    fn fire_arp(&mut self, index: usize, arp: Arpeggiating) {
        let rate = self.rate;
        let audible = self.plays(index);
        let Some(note) = self.tracks[index].last_note else {
            self.tracks[index].arp = None;
            return;
        };
        let roll = if arp.mode == ArpMode::Random {
            self.tracks[index].arp_rng.next_u32()
        } else {
            0
        };
        let place = arp.cycle.place(arp.mode, arp.played, roll);
        let next = NoteOn {
            note: arp.cycle.note(place),
            ..note
        };
        if arp.retrig {
            self.start_modulation(index);
            let track = &mut self.tracks[index];
            let (kill, off, gate) = (track.kill_at, track.off_at, track.gate_at);
            if audible {
                track.restrike(next, rate);
            }
            track.kill_at = kill;
            track.off_at = off;
            track.gate_at = gate;
            track.last_note = Some(next);
            track.wake();
        } else {
            self.tracks[index].tie(next);
        }
        let played = arp.played + 1;
        let next_at = arp.from + self.frames_of(played * arp.note_ticks, arp.step_ticks);
        self.tracks[index].arp = Some(Arpeggiating {
            played,
            place,
            next_at,
            ..arp
        });
    }

    /// Move the song on a step: into the next section if this one has played out, and back to the
    /// start at a reset. Then work out the step of every track whose own step falls here, and
    /// start what's cued on any partway through a longer step, which then comes in here.
    pub(super) fn fire_step(&mut self) {
        // The step last worked out, which is still the one heard until this one's place comes.
        self.heard_place = self.place.saturating_sub(1);
        if !self.enter_section_if_due() {
            return;
        }
        if self.past_reset(self.place) {
            self.realign();
        }
        // A realign asked for, of every track or of one, lands here as a reset would, after the
        // section has moved on, and so do mutes and solos waiting for the grid, before the line's
        // notes, which they're heard or silenced with.
        let switching = self.tracks.iter().any(|t| t.mute_waiting || t.solo_waiting);
        let waiting =
            self.realign_waiting || switching || self.tracks.iter().any(|t| t.realign_waiting);
        let line = waiting && self.on_grid_line(self.realign_grid());
        if switching && line {
            self.switch_waiting();
        }
        if self.realign_waiting && line {
            self.realign_waiting = false;
            self.realign();
            // Every track's own realign waiting for this line comes with it.
            for track in self.tracks.iter_mut() {
                track.realign_waiting = false;
            }
        }
        let grid = self.next_step * TICKS_PER_STEP;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        for index in 0..tracks {
            // A track's own realign, or its own reset, starts its chain again here, as a section
            // bringing it in would, a longer step cut short. A reset or realign of every track
            // just now has started it here already.
            let realigns = line && self.tracks[index].realign_waiting;
            if realigns {
                self.tracks[index].realign_waiting = false;
            }
            if realigns || self.reset_due(index, grid) {
                self.tracks[index].restart_at(grid);
            }
            if self.tracks[index].clock.beat != grid {
                self.start_cued(index, grid, true);
                if self.tracks[index].clock.beat != grid {
                    continue;
                }
            }
            self.track_step(index, grid, true);
        }
    }

    /// Work out a track's next step, which falls here on the grid, after playing what's waiting
    /// and starting what's cued, as the track's launch timing allows on the song's step or the
    /// track's own.
    pub(super) fn track_step(&mut self, index: usize, at: u64, song_step: bool) {
        // A big tempo jump can bring this step before the last one landed. With nudges, a row
        // nudged late can land after the next step is worked out, and with speeds a fast track's
        // steps can be worked out ahead of a slow one's lead, so only what's due goes now.
        let by = if self.current.nudges || self.current.speeds {
            self.clock
        } else {
            u64::MAX
        };
        while let Some(part) = self.tracks[index].take_due(by) {
            self.fire_track(index, part);
        }
        self.start_cued(index, at, song_step);
        self.work_out(index);
    }
}
