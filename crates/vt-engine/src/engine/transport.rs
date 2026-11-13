//! Starting and stopping, realigning, launching, the sections played in turn and looping.

use std::sync::Arc;

use vt_model::constants::{BAR_STEPS, MAX_MILLIBPM, MAX_TRACKS, MIN_MILLIBPM, TICKS_PER_STEP};
use vt_model::order::Dealer;
use vt_model::song::{Launch, Section, SectionStart, Song, Track};

use crate::link::JamState;
use crate::track_state::Clock;

use super::{Engine, SectionLoop, Transport};

impl Engine {
    /// Send every track back to its start from the next step, leaving the beat where it is.
    pub fn realign(&mut self) {
        self.realign_at(self.next_step * TICKS_PER_STEP);
    }

    /// Send every track back to its start at the next line of the realign grid, or take back a
    /// realign waiting for it. Only while playing, as playing starts every track at its start.
    pub fn toggle_realign(&mut self) {
        self.realign_waiting = self.transport == Transport::Playing && !self.realign_waiting;
    }

    /// Whether a realign is waiting for its line of the grid.
    pub fn realign_waiting(&self) -> bool {
        self.realign_waiting
    }

    /// Start one track's chain again at the next line of the realign grid, or take back one
    /// waiting for it. Only while playing, as an every-track realign is.
    pub fn toggle_realign_track(&mut self, index: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(track) = self.tracks.get_mut(index) {
            track.realign_waiting = playing && !track.realign_waiting;
        }
    }

    /// Whether a track's own realign is waiting for its line of the grid.
    pub fn track_realign_waiting(&self, index: usize) -> bool {
        self.tracks
            .get(index)
            .is_some_and(|track| track.realign_waiting)
    }

    /// How far apart a track's own resets are on the grid, in ticks, if it has them.
    fn reset_ticks(&self, index: usize) -> Option<u64> {
        let steps = self.current.song.tracks.get(index)?.reset?;
        Some(u64::from(steps.max(1)) * TICKS_PER_STEP)
    }

    /// Whether a track's own reset falls on the song's step at this place on the grid: the first
    /// of the song's steps at or after its reset's length from where its chain last started.
    pub(super) fn reset_due(&self, index: usize, grid: u64) -> bool {
        let Some(every) = self.reset_ticks(index) else {
            return false;
        };
        let (every, step) = (every as i64, TICKS_PER_STEP as i64);
        let since = grid as i64 - self.tracks[index].clock.anchor;
        since >= every && since.div_euclid(every) > (since - step).div_euclid(every)
    }

    /// Start each track's chain again at the last of its own resets before this place on the
    /// grid, as playing up to it would have, for working out where playing from a place starts.
    /// Each track's clock is no more than a step of its own past the last place this was asked.
    fn reset_tracks_before(&mut self, at: u64) {
        for index in 0..MAX_TRACKS {
            let Some(every) = self.reset_ticks(index) else {
                continue;
            };
            let anchor = self.tracks[index].clock.anchor;
            let first = ((anchor + every as i64).max(0) as u64).next_multiple_of(TICKS_PER_STEP);
            if first >= at {
                continue;
            }
            let last = first + (at - 1 - first) / every * every;
            let ticks = self.speed_ticks(index);
            let track = &mut self.tracks[index];
            track.clock.skip_to(last, ticks);
            track.restart_at(last);
        }
    }

    /// How many steps apart the lines are that a realign waits for: the launch grid's, or a bar's
    /// when launches wait for a chain's end, as every track's chain ends at a different place.
    pub(super) fn realign_grid(&self) -> u64 {
        match self.current.song.launch {
            Launch::Every(steps) => u64::from(steps.get()),
            Launch::ChainEnd => BAR_STEPS as u64,
        }
    }

    /// Whether the song's step about to fire is a line of a grid this many steps apart. Each
    /// repeat of a section starts the grid again, so a section shorter than the grid has a line
    /// only at its start.
    pub(super) fn on_grid_line(&self, steps: u64) -> bool {
        let length = self
            .playing_section()
            .map_or(0, |s| self.current.song.section_length(s));
        (self.section_place % (length as u64).max(1)).is_multiple_of(steps.max(1))
    }

    /// Send every track back to its start at this place on the grid, counting its steps again
    /// from there.
    fn realign_at(&mut self, at: u64) {
        self.place = 0;
        for track in self.tracks.iter_mut() {
            track.clock.count = 0;
            track.restart_at(at);
        }
    }

    /// Bring a section in at this place on the grid: a track given a different chain starts it
    /// from its top there, lining its steps up with the section's start.
    fn start_row(&mut self, section: &Section, at: u64) {
        for (track, chain) in self.tracks.iter_mut().zip(&section.chains) {
            if track.playing != *chain {
                track.playing = *chain;
                track.restart_at(at);
            }
        }
    }

    /// How long a track's steps are at its own speed, in ticks.
    pub(super) fn speed_ticks(&self, index: usize) -> u64 {
        self.current
            .song
            .tracks
            .get(index)
            .map_or(TICKS_PER_STEP, |track| track.speed.ticks())
    }

    /// Count every track on to the first of its steps at or after this place on the grid, as if
    /// it had played at its own speed.
    fn skip_tracks_to(&mut self, at: u64) {
        for index in 0..MAX_TRACKS {
            let ticks = self.speed_ticks(index);
            self.tracks[index].clock.skip_to(at, ticks);
        }
    }

    /// Work out the running count and each track's start from the rows before this one, as if the
    /// song had played to this step of it, and give the step within the section.
    ///
    /// Each track's steps are walked at its own speed on a grid counted from the walk's start, and
    /// moved to where playing starts once that's known. A track partway through a longer step
    /// there comes in on its next.
    fn walk_to(&mut self, section: usize, step: usize) -> u64 {
        self.place = 0;
        for track in self.tracks.iter_mut() {
            track.playing = None;
            track.clock = Clock::default();
            track.restart(0);
            track.forget_worked(0);
            track.forget_conditions();
            track.forget_accumulators();
        }
        let song = Arc::clone(&self.current.song);
        let Some(rows) = song.sections.get(..=section) else {
            return 0;
        };
        // A row that resets makes what came before it not matter.
        let first = rows
            .iter()
            .rposition(|row| row.start == SectionStart::Reset)
            .unwrap_or(0);
        let mut within = 0;
        let mut tick = 0;
        for (at, row) in rows.iter().enumerate().skip(first) {
            let length = song.section_length(row) as u64;
            if length == 0 {
                continue;
            }
            self.reset_tracks_before(tick);
            self.skip_tracks_to(tick);
            self.start_row(row, tick);
            let plays = song.section_plays(row) as u64;
            let mut steps = if at == section {
                within = step as u64 % plays;
                within
            } else {
                plays
            };
            // A resetting section's earlier repeats leave nothing behind.
            if row.start == SectionStart::Reset && steps > length {
                steps = (steps - 1) % length + 1;
            }
            let (place, reset) = after_steps(self.place, steps, row.reset);
            if reset {
                // The last reset was as many steps back as the count has come since.
                let reset_at = tick + (steps - place) * TICKS_PER_STEP;
                self.skip_tracks_to(reset_at);
                self.realign_at(reset_at);
            }
            self.place = place;
            tick += steps * TICKS_PER_STEP;
        }
        self.reset_tracks_before(tick);
        self.skip_tracks_to(tick);
        let by = (within * TICKS_PER_STEP) as i64 - tick as i64;
        for track in self.tracks.iter_mut() {
            track.clock.shift(by);
        }
        within
    }

    /// Put a track's chain at this step of its own, starting it there at the next step, leaving
    /// the others where they are.
    pub fn place_track(&mut self, track: usize, step: usize) {
        let at = self.next_step * TICKS_PER_STEP;
        let ticks = self.speed_ticks(track) as i64;
        if let Some(state) = self.tracks.get_mut(track) {
            state.clock.line_up(at);
            state.restart(state.clock.count - step as i64);
            // Its own reset counts from where its chain would have started, on the same grid.
            state.clock.anchor -= step as i64 * ticks;
        }
    }

    /// Start at a step within a section, or at a step of a track's own with that track's chain
    /// there: the song starts at the step of its own that the track's falls in.
    pub fn play_section_at(&mut self, section: usize, step: usize, track: Option<usize>) {
        let Some(track) = track else {
            self.play_section(section, step);
            return;
        };
        let song_step = step as u64 * self.speed_ticks(track) / TICKS_PER_STEP;
        self.play_section(section, song_step as usize);
        self.place_track(track, step);
    }

    /// Play a chain on one track when its launch timing allows, or stop it then.
    pub fn launch(&mut self, track: usize, chain: Option<u32>) {
        if let Some(track) = self.tracks.get_mut(track) {
            track.cued = Some(chain);
        }
    }

    /// Carry on in another section as if nothing changed, with launched chains still running. It
    /// has this length, so the place in it is the place in the last section's current repeat.
    pub fn continue_in(&mut self, section: usize, length: usize) {
        self.section = section;
        self.section_place %= (length as u64).max(1);
        self.looping = Some(SectionLoop::one(section));
        self.loop_cued = None;
    }

    /// What a track is launched onto and cued to, while jamming.
    pub fn jam(&self, track: usize) -> JamState {
        self.tracks
            .get(track)
            .map_or_else(JamState::default, |track| JamState {
                playing: track.launched,
                cued: track.cued,
                chain: track.playing,
                started: track.started,
                lane_offsets: track.lane_offsets,
                mute: track.mute,
                solo: track.solo,
                mute_waiting: track.mute_waiting,
                solo_waiting: track.solo_waiting,
                switches_landed: track.switches_landed,
            })
    }

    /// Back to every track playing its section's chain, forgetting what's cued.
    fn clear_launches(&mut self) {
        self.end_launches();
        for track in self.tracks.iter_mut() {
            track.cued = None;
        }
    }

    /// Back to every track playing its section's chain, keeping what's cued.
    fn end_launches(&mut self) {
        for track in self.tracks.iter_mut() {
            track.launched = None;
        }
    }

    /// The chain a track is on: one launched, or its section's.
    pub(super) fn track_chain(&self, index: usize) -> Option<u32> {
        if let Some(launched) = self.tracks[index].launched {
            return launched;
        }
        let section = self.current.song.sections.get(self.section)?;
        *section.chains.get(index)?
    }

    /// The chain a track is on, and how far into it a step of its running count falls, counting
    /// every time round it since it started, as which time round deals a dealt order. A step
    /// before it started, as playing from a step can count, is in its first time round.
    pub(super) fn track_place(&self, index: usize, count: i64) -> Option<(u32, usize)> {
        let chain = self.track_chain(index)?;
        let length = (self.current.song.tracks.get(index)?.chain_steps(chain) as i64).max(1);
        let place = count - self.tracks[index].started;
        let laps = place.div_euclid(length).max(0);
        Some((chain, (laps * length + place.rem_euclid(length)) as usize))
    }

    /// What a track's dealt orders are dealt from.
    pub(super) fn dealer(&self, index: usize) -> Dealer {
        Dealer::new(self.current.song.seed, index)
    }

    /// Start what a track has cued, if this place on the grid is one its launch timing starts one
    /// on: one of the song's steps for a launch on the bar's grid, which a track partway through
    /// a longer step starts on there, or the track's own step at its chain's end.
    pub(super) fn start_cued(&mut self, index: usize, at: u64, song_step: bool) {
        if self.tracks[index].cued.is_none() {
            return;
        }
        let due = match self.current.song.launch {
            Launch::Every(steps) => song_step && self.on_grid_line(u64::from(steps.get())),
            Launch::ChainEnd => {
                let state = &self.tracks[index];
                state.clock.beat == at
                    && self
                        .track_place(index, state.clock.count)
                        .is_none_or(|(current, place)| {
                            let length = self.current.song.tracks[index].chain_steps(current);
                            length == 0 || place.is_multiple_of(length)
                        })
            }
        };
        if due {
            self.begin_cued(index, at);
        }
    }

    /// Start what a track has cued now, at this place on the grid, if it has anything cued.
    fn begin_cued(&mut self, index: usize, at: u64) {
        let Some(chain) = self.tracks[index].cued.take() else {
            return;
        };
        if chain.is_none() {
            self.tracks[index].stop();
        }
        self.tracks[index].launched = Some(chain);
        self.tracks[index].restart_at(at);
    }

    /// Whether any section has a length, so there's a next step to fire. One with a length and no
    /// chains is a rest, and one with neither is skipped.
    pub(super) fn has_steps(&self) -> bool {
        let song = &self.current.song;
        song.sections.iter().any(|s| song.section_length(s) > 0)
    }

    /// The section playing. Removing one while playing moves to whichever takes its place.
    pub(super) fn playing_section(&self) -> Option<&Section> {
        let sections = &self.current.song.sections;
        sections.get(self.section).or(sections.last())
    }

    /// The section playing, by position.
    pub fn section(&self) -> usize {
        self.section
            .min(self.current.song.sections.len().saturating_sub(1))
    }

    /// Keep playing this section rather than moving on, from now, or go back to playing the song.
    pub fn set_loop_section(&mut self, section: Option<usize>) {
        self.looping = section.map(SectionLoop::one);
        self.loop_cued = None;
    }

    /// Keep playing these sections in turn rather than moving on, or go back to playing the song.
    /// While playing a section that isn't one of them, a loop waits for the next line of the
    /// launch grid, where the first of them starts; a section ending is a line too. Otherwise it's
    /// at once, as nothing moves until the section playing ends, which is a line anyway, and so is
    /// going back to the song, as the section playing then plays to its end and moves on.
    pub fn loop_sections(&mut self, sections: Option<SectionLoop>) {
        self.loop_cued = None;
        let playing = self.transport == Transport::Playing;
        match sections {
            Some(sections) if playing && !sections.holds(self.section()) => {
                self.loop_cued = Some(sections);
            }
            _ => self.looping = sections,
        }
    }

    /// Whether a loop is waiting for its line of the grid.
    pub fn loop_waiting(&self) -> bool {
        self.loop_cued.is_some()
    }

    /// The sections being kept playing, if any.
    pub fn looping(&self) -> Option<SectionLoop> {
        self.looping
    }

    /// Go back to the first section's start without touching anything sounding, for a render. A
    /// step a groove's lead worked out early is worked out again from the top.
    pub fn rewind(&mut self) {
        if self.worked_out_early(true) {
            self.next_step -= 1;
            let from = self.next_step * TICKS_PER_STEP;
            for track in self.tracks.iter_mut() {
                track.forget_steps(from);
            }
        }
        self.section = 0;
        self.section_place = 0;
        self.realign();
        self.clear_launches();
    }

    /// Move to the next section if this one has played out. False if nothing has any length.
    pub(super) fn enter_section_if_due(&mut self) -> bool {
        let song = Arc::clone(&self.current.song);
        let count = song.sections.len();
        if count == 0 {
            return false;
        }
        self.section = self.section.min(count - 1);
        let here = &song.sections[self.section];
        if self.section_place < song.section_plays(here) as u64 {
            // A loop cued lands on the grid's next line, and its first section starts there if
            // the one playing isn't one of its.
            if self.loop_cued.is_some() && self.on_grid_line(self.realign_grid()) {
                let sections = self.loop_cued.take().unwrap_or_else(|| unreachable!());
                self.looping = Some(sections);
                if !sections.holds(self.section) {
                    let (from, first) = (self.section, sections.first.min(count - 1));
                    return self.enter_section(&song, first, from, false);
                }
            }
            // Each repeat of a resetting section starts it over.
            let length = song.section_length(here) as u64;
            if here.start == SectionStart::Reset
                && self.section_place > 0
                && self.section_place.is_multiple_of(length.max(1))
            {
                self.realign();
            }
            return true;
        }
        // A section's start is a line of the grid, so a loop cued lands here.
        if let Some(sections) = self.loop_cued.take() {
            self.looping = Some(sections);
        }
        let from = self.section;
        let (next, wrapped) = match self.looping {
            Some(sections) if sections.first < count => sections.next(&song, from),
            _ => next_row(&song, from),
        };
        self.enter_section(&song, next, from, wrapped)
    }

    /// Start a section, coming from another or the same one, and going back round to the first if
    /// `wrapped`: it runs its `Start`, ends launches if it's another, and starts what's cued. False
    /// if it has no length.
    fn enter_section(&mut self, song: &Song, next: usize, from: usize, wrapped: bool) -> bool {
        self.section = next;
        self.section_place = 0;
        let section = &song.sections[next];
        if song.section_length(section) == 0 {
            return false;
        }
        if next != from {
            self.end_launches();
        }
        let at = self.next_step * TICKS_PER_STEP;
        if section.start == SectionStart::Reset {
            self.realign();
        } else if next != from && wrapped {
            for track in self.tracks.iter_mut() {
                track.restart_at(at);
            }
        } else if next != from {
            self.start_row(section, at);
        }
        // A cue left waiting isn't lost to the song moving on.
        if next != from {
            for index in 0..self.tracks.len() {
                self.begin_cued(index, at);
            }
        }
        true
    }

    /// Start at a step within the section, whatever is happening now. Anything sounding fades, and
    /// starting from the top resets held values.
    pub fn play_from(&mut self, step: usize) {
        self.play_section(0, step);
    }

    /// Start at a step within a section. Only the song's very start resets held values.
    pub fn play_section(&mut self, section: usize, step: usize) {
        self.halt(Transport::Stopped);
        self.clear_launches();
        if section == 0 && step == 0 {
            self.reset_tracks();
        }
        self.section = section;
        let within = self.walk_to(section, step);
        self.section_place = within;
        self.next_step = within;
        self.origin_tick = within * TICKS_PER_STEP;
        self.frames_since_origin = 0;
        self.millibpm = self.current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        self.transport = Transport::Playing;
        for track in self.tracks.iter_mut() {
            track.count_cycles_from_the_top();
        }
        self.update_lfos();
    }

    /// Pause in place, or resume from the stored position.
    pub fn pause_or_resume(&mut self) {
        if self.transport == Transport::Playing {
            self.halt(Transport::Paused);
        } else {
            self.transport = Transport::Playing;
        }
    }

    /// Go back to the start, resetting held values and random generators. If playing, stop there.
    pub fn return_to_start(&mut self) {
        self.halt(Transport::Stopped);
        self.clear_launches();
        self.section = 0;
        self.section_place = 0;
        self.next_step = 0;
        self.realign();
        self.origin_tick = 0;
        self.frames_since_origin = 0;
        for track in self.tracks.iter_mut() {
            track.forget_worked(0);
        }
        self.reset_tracks();
    }

    /// Reset every track state's held values, random generators and LFOs, those past the song's
    /// tracks as an empty track's. Going through every setting is most of the time this takes, so
    /// a state past the tracks of every version since the last reset, and silent, has only the rest
    /// reset: its settings are still the empty track's that reset gave it, as only a track the song
    /// has changes them.
    pub(super) fn reset_tracks(&mut self) {
        let seed = self.current.song.seed;
        let fresh = Track::default();
        let tracks = &self.current.song.tracks;
        for (index, state) in self.tracks.iter_mut().enumerate() {
            let used = index < self.used || state.sounding() || state.ringing(self.rate);
            if used {
                let track = tracks.get(index).unwrap_or(&fresh);
                state.reset_held(seed, index, track, self.clock);
            } else {
                state.reset_all_but_settings(seed, index, self.clock);
            }
        }
        self.used = tracks.len().min(MAX_TRACKS);
    }

    /// Stop sequencing but let everything sounding finish on its own, for a render's tail.
    pub fn let_ring(&mut self) {
        self.transport = Transport::Stopped;
        for track in self.tracks.iter_mut() {
            track.voice.release_if_held();
        }
    }

    /// Stop moving. Voices fade so stopping doesn't click, and effect tails keep ringing.
    fn halt(&mut self, to: Transport) {
        for track in self.tracks.iter_mut() {
            track.stop();
        }
        self.transport = to;
        self.realign_waiting = false;
        // A loop waiting for the grid is the loop from now, as there's no line coming round.
        if let Some(sections) = self.loop_cued.take() {
            self.looping = Some(sections);
        }
        for track in self.tracks.iter_mut() {
            track.realign_waiting = false;
            track.mute_waiting = false;
            track.solo_waiting = false;
        }
    }
}

/// The next row with a length after this one, in order, and whether that went past the last. If no
/// other has one, it's this row.
fn next_row(song: &Song, from: usize) -> (usize, bool) {
    let count = song.sections.len();
    let mut wrapped = false;
    for ahead in 1..=count {
        let at = (from + ahead) % count;
        wrapped |= at == 0;
        if song.section_length(&song.sections[at]) > 0 {
            return (at, wrapped);
        }
    }
    (from, false)
}

/// Where the running count is after some steps from a place, under a section's `:reset`, and
/// whether a reset came within them. A count at or past the reset goes back on its next step.
fn after_steps(place: u64, steps: u64, reset: Option<usize>) -> (u64, bool) {
    let Some(reset) = reset.map(|reset| reset.max(1) as u64) else {
        return (place + steps, false);
    };
    let until = reset.saturating_sub(place);
    if until >= steps {
        return (place + steps, false);
    }
    (((steps - until - 1) % reset) + 1, true)
}
