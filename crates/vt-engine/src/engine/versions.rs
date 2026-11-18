//! Taking a new version of the song: its memory, its bank, its tracks as they come and go,
//! and what each compressor or Warps hears.

use std::sync::Arc;

use vt_model::constants::{
    FIXED_TRACKS, FX_SLOTS, MAX_MILLIBPM, MAX_SOUND_TRACKS, MAX_TRACKS, MIN_MILLIBPM,
    TICKS_PER_QUARTER, TICKS_PER_STEP,
};
use vt_model::insert::Insert;
use vt_model::samples::SampleId;
use vt_model::settings::PlayMode;
use vt_model::song::{Song, Track};

use crate::dsp::effects::memory::{Memory, MemoryCount};
use crate::published::{Bank, Handover, Published, Span};
use crate::track_state::base_of;
use crate::voice::{NoteOn, Voice};

use super::{Engine, Transport};

impl Engine {
    /// Keep a long buffer the edit thread has sent, for a version it's about to send.
    pub(crate) fn receive_memory(&mut self, memory: Memory) {
        self.memory.put(memory);
    }

    /// The edit thread wants this many back, as they're more than its versions need.
    pub(crate) fn release_memory(&mut self, count: MemoryCount) {
        self.memory.owe(count);
    }

    /// The next long buffer to send back to be freed.
    pub(crate) fn next_freed_memory(&mut self) -> Option<Memory> {
        self.memory.next_owed()
    }

    /// How many long buffers are held that no slot is using.
    #[cfg(test)]
    pub(crate) fn spare_memory(&self) -> usize {
        self.memory.spare()
    }

    /// The id of the version playing, [`Published::id`].
    pub(crate) fn version(&self) -> u64 {
        self.current.id
    }

    /// Switch to a new version, as [`hand_over`](Self::hand_over) does, making the room for its
    /// generators here, so not for the callback.
    pub fn swap(&mut self, version: Arc<Published>) -> Arc<Published> {
        self.hand_over(Handover::new(version)).version
    }

    /// Switch to a new version, returning the old one so it can be freed elsewhere. The new
    /// version's scratch for generators is the engine's now, and the one it had goes back with the
    /// old version, to be dropped on the edit thread.
    ///
    /// A track whose chain changed length stays on the row it was going to play next. An ID that
    /// no longer exists is silent, and a removed track's voices fade.
    pub fn hand_over(&mut self, handover: Handover) -> Handover {
        let Handover {
            version,
            mut scratch,
        } = handover;
        let old = std::mem::replace(&mut self.current, version);
        std::mem::swap(&mut self.gens, &mut scratch);
        // What each generator was heard as carries on, for `Mutate` (decision 430).
        self.gens.keep_heard_from(&scratch);
        // What any track plays with may have changed.
        for track in self.tracks.iter_mut() {
            track.wake();
        }
        self.used = self
            .used
            .max(self.current.song.tracks.len().min(MAX_TRACKS));
        self.keep_fixed_tracks(old.song.sound_tracks());
        self.forget_rewritten_steps(&old.song);
        self.line_up_new_tracks(old.song.sound_tracks());
        self.keep_places(&old.song);
        self.follow_base(&old);
        for track in self.tracks.iter_mut().skip(self.current.song.tracks.len()) {
            track.stop();
            track.launched = None;
            track.cued = None;
            track.mute = false;
            track.solo = false;
            track.mute_waiting = false;
            track.solo_waiting = false;
            track.give_back_all();
            track.give_back_memory(&mut self.memory);
        }
        self.follow_levels();
        self.retune();
        self.find_listened();
        // A track that's joined or left a group is heard by that group's mute and solo now.
        let was = old.song.tracks.iter().map(|track| track.group);
        if was.ne(self.current.song.tracks.iter().map(|track| track.group)) {
            self.apply_audibility();
        }
        if !Arc::ptr_eq(&old.bank, &self.current.bank) {
            self.follow_bank();
        }
        Handover {
            version: old,
            scratch,
        }
    }

    /// Have every note playing one of the bank's entries follow its changes, heard at once, as
    /// moving a point is: notes sounding, and those waiting to sound or to sound again. One whose
    /// entry now plays another file, or nothing, keeps playing what it was.
    fn follow_bank(&mut self) {
        let bank = &self.current.bank;
        for track in self.tracks.iter_mut() {
            follow_voice(&mut track.voice, bank);
            follow_voice(&mut track.tail, bank);
            if let Some(note) = &mut track.last_note {
                follow_note(note, bank);
            }
            if let Some(delayed) = &mut track.delayed {
                follow_note(&mut delayed.note, bank);
            }
        }
        self.previews.follow(
            |voice| follow_voice(voice, bank),
            |note| follow_note(note, bank),
        );
    }

    /// Note which tracks a compressor, Warps or a low-pass gate hears. One that's muted keeps playing, silently, so
    /// a muted kick can still duck a bass, and one that no longer is heard stops. A group that's
    /// heard has its tracks heard too, so a muted kit can still duck a bass through its group.
    pub(super) fn find_listened(&mut self) {
        let song = &self.current.song;
        let sounds = song.sound_tracks();
        let mut listened = [false; MAX_TRACKS];
        for insert in song.tracks.iter().flat_map(|track| track.inserts) {
            let heard = match insert {
                Insert::Compressor(compressor) => compressor.source_track(sounds),
                Insert::Warps(warps) => warps.source_track(sounds),
                Insert::Lpg(gate) => gate.source_track(sounds),
                _ => None,
            };
            if let Some(track) = heard
                && let Some(listened) = listened.get_mut(track)
            {
                *listened = true;
            }
        }
        for track in 0..sounds.min(MAX_TRACKS) {
            if let Some(group) = song.group_track_of(track)
                && listened.get(group) == Some(&true)
            {
                listened[track] = true;
            }
        }
        if listened != self.listened {
            self.listened = listened;
            self.apply_audibility();
        }
    }

    /// Whether a track plays its notes: when it's heard, or when a compressor or Warps hears it.
    pub(super) fn plays(&self, index: usize) -> bool {
        self.audible(index) || self.listened[index]
    }

    /// A note has started on a track, so what's sounding on the others in its choke group stops,
    /// fading out as a new note fades the old one.
    pub(super) fn choke(&mut self, index: usize) {
        let sounds = self.sounds();
        let tracks = &self.current.song.tracks;
        let Some(group) = tracks.get(index).and_then(|track| track.choke) else {
            return;
        };
        for (other, track) in tracks.iter().enumerate().take(sounds) {
            if other != index && track.choke == Some(group) {
                self.tracks[other].cut_to_tail();
            }
        }
    }

    /// The fixed tracks come after the others, so as those come and go, what the fixed tracks keep
    /// moves with them.
    fn keep_fixed_tracks(&mut self, was: usize) {
        let now = self.sounds();
        let was = was.min(MAX_SOUND_TRACKS);
        let moved = &mut self.tracks[was.min(now)..was.max(now) + FIXED_TRACKS];
        if now > was {
            moved.rotate_right(now - was);
        } else {
            moved.rotate_left(was - now);
        }
    }

    /// A Steps step whose written value an edit changed is heard as written at once, not as
    /// `Mutate` drew it (decision 384). A track the version shares with the last is passed over by
    /// its pointer, and a new one has nothing drawn to compare.
    fn forget_rewritten_steps(&mut self, old: &Song) {
        let (now, was) = (self.sounds(), old.sound_tracks());
        for index in 0..self.current.song.tracks.len().min(MAX_TRACKS) {
            let then = match index {
                _ if index >= now => index - now + was,
                _ if index < was => index,
                _ => continue,
            };
            if self.current.song.tracks.same(index, &old.tracks, then) {
                continue;
            }
            if let (Some(before), Some(after)) =
                (old.tracks.get(then), self.current.song.tracks.get(index))
            {
                self.tracks[index].follow_written_steps(before, after);
            }
        }
    }

    /// A track that's new comes in on the song's next step, counting its steps as if it had played
    /// at its speed since the last reset.
    fn line_up_new_tracks(&mut self, was: usize) {
        let at = self.next_step * TICKS_PER_STEP;
        for index in was.min(MAX_SOUND_TRACKS)..self.sounds() {
            let ticks = self.speed_ticks(index);
            let clock = &mut self.tracks[index].clock;
            clock.line_up(at);
            clock.count = (self.place * TICKS_PER_STEP).div_ceil(ticks) as i64;
        }
    }

    /// Keep each playing track on the row it was going to play next, as its chain's length changes:
    /// the same slot of its chain and row of that phrase's loop, or its chain's first row if those
    /// have gone. Only `started` moves, and a track whose chain didn't change is left alone, so its
    /// count since its chain started carries on. A track the version shares with the last, which
    /// is every track an edit didn't touch, is passed over by its pointer, as finding its row
    /// again finds it where it is.
    fn keep_places(&mut self, old: &Song) {
        if self.transport == Transport::Stopped {
            return;
        }
        let (now, was) = (self.sounds(), old.sound_tracks());
        for index in 0..self.current.song.tracks.len().min(MAX_TRACKS) {
            // A fixed track is compared with what it was, wherever that was, and a new one is new.
            let then = match index {
                _ if index >= now => index - now + was,
                _ if index < was => index,
                _ => continue,
            };
            // The very track it was has the chains and phrases it had, so it's where it was.
            if self.current.song.tracks.same(index, &old.tracks, then) {
                continue;
            }
            let old_chain = match self.tracks[index].launched {
                Some(launched) => launched,
                None => old
                    .sections
                    .get(self.section)
                    .and_then(|s| s.chains.get(then).copied().flatten()),
            };
            let (Some(chain), Some(before)) = (self.track_chain(index), old.tracks.get(then))
            else {
                continue;
            };
            if old_chain != Some(chain) {
                continue;
            }
            let current = Arc::clone(&self.current);
            let track = &current.song.tracks[index];
            let (old_length, length) = (
                before.chain_steps(chain) as i64,
                track.chain_steps(chain) as i64,
            );
            if old_length == 0 || length == 0 {
                continue;
            }
            let dealer = self.dealer(index);
            let walk = self.walk(index);
            let state = &mut self.tracks[index];
            let place = state.clock.count;
            let count = place - state.started;
            let next = count.rem_euclid(old_length);
            let laps = count.div_euclid(old_length);
            let lapped = |length: i64, at: i64| (laps.max(0) * length + at) as usize;
            let Some(was_at) = before.chain_position(chain, lapped(old_length, next), dealer, walk)
            else {
                continue;
            };
            // The same pulse of the row too, as far as the row still lasts, where this pass of
            // its phrase's walk plays it nearest to where it was, or where it was if the pass
            // doesn't play it.
            let kept = track
                .chain_place(
                    chain,
                    was_at.row_place(),
                    laps.max(0) as usize,
                    dealer,
                    walk,
                )
                .unwrap_or(0) as i64;
            let moved = length != old_length || kept != next;
            if moved {
                state.started = place - (laps * length + kept);
            }
            let (Some(old_phrase), Some(at)) = (
                before.phrases.get(&was_at.phrase),
                track.chain_position(chain, lapped(length, kept), dealer, walk),
            ) else {
                continue;
            };
            let Some(phrase) = track.phrases.get(&at.phrase) else {
                continue;
            };
            let new_count = place - state.started;
            let (was_entry, entry) = ((was_at.phrase, was_at.slot), (at.phrase, at.slot));
            for slot in 0..FX_SLOTS {
                let (old_lane, lane) = (old_phrase.lane(slot), phrase.lane(slot));
                let same = was_at.phrase == at.phrase;
                if !moved && old_lane == lane && same {
                    continue;
                }
                let Some(lane) = lane else {
                    state.lane_offsets[slot] = 0;
                    continue;
                };
                // Where this column was going to read, which it keeps if its lane still has the
                // step, as the lane counts it now.
                let ahead =
                    old_lane.map(|old| (old, state.lane_ahead(slot, old, was_entry, count)));
                let was_reading = match ahead {
                    Some((old, ahead)) => old.stored_from(ahead.row, state.lane_start(slot, old)),
                    None => old_phrase.start() + was_at.step,
                };
                let kept = same && lane.contains(was_reading);
                let row = if kept { was_reading - lane.start } else { 0 };
                // Placed from the count after, as on a new phrase in the chain, it reads that row,
                // as far through it as it was.
                let heard = state.heard_lane(slot, lane);
                let division = heard.division.max(1) as i64;
                let within = match ahead {
                    Some((_, ahead)) if kept && ahead.left > 0 => {
                        division - (ahead.left as i64).min(division)
                    }
                    _ => 0,
                };
                let offset = row as i64 * division + within - new_count;
                let cycle = heard.length.max(1) as i64 * division;
                state.lane_offsets[slot] = offset.rem_euclid(cycle);
                // A lane that's as it was carries on where it is, in a copy of its rows too.
                match ahead {
                    Some((old, ahead)) if same && old == lane => {
                        state.put_lane(slot, entry, new_count, ahead);
                    }
                    _ => state.unplace_lane(slot),
                }
            }
        }
    }

    /// How many tracks play sounds, and so where the fixed tracks start.
    pub(super) fn sounds(&self) -> usize {
        self.current.song.sound_tracks().min(MAX_SOUND_TRACKS)
    }

    /// Hear a change to a track's page straight away. A held value a step has set since stays
    /// until something changes it, so this only acts on what actually changed. It's compared with
    /// the version the engine was playing, which may not be the one before this on the edit
    /// thread, as a version still waiting is replaced by the next.
    fn follow_base(&mut self, old: &Published) {
        let fresh = Track::default();
        let (now, was) = (self.sounds(), old.song.sound_tracks());
        let song = &self.current.song;
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&song.tracks) {
            // A fixed track is compared with what it was, wherever that was.
            let then = if index >= now {
                index - now + was
            } else {
                index
            };
            let was_there = index >= now || index < was;
            // The very track it was has nothing moved, and its macros hold what they held, so
            // it's compared by its pointer alone. Its macros still put what they hold where they
            // have it, as they do for every version, over a value a step has set since.
            if was_there && song.tracks.same(index, &old.song.tracks, then) {
                if track.macros.iter().any(|held| !held.maps.is_empty()) {
                    state.put_macros_again(track);
                }
                continue;
            }
            let before = if was_there {
                old.song.tracks.get(then).unwrap_or(&fresh)
            } else {
                &fresh
            };
            // Only what may have moved is looked up, which on a track the edit didn't touch is
            // nothing.
            for id in track.settings_moved_from(before).iter() {
                let (was, now) = (base_of(id, before), base_of(id, track));
                if was != now {
                    state.set_base(id, now);
                }
            }
            // A setting a macro no longer holds goes back to the song's, and the macros put what
            // they hold where they have it again, as their maps may have changed.
            for map in before.macros.iter().flat_map(|held| &held.maps) {
                if track.macro_holding(map.target).is_none()
                    && let Some(id) = map.target.setting()
                {
                    state.set_base(id, base_of(id, track));
                }
            }
            state.put_macros_again(track);
        }
        self.follow_group_macros(old);
    }

    /// Have each group's macros put what they hold on its tracks again, as their maps, the group's
    /// tracks or the tracks themselves may have changed: what a group's macro held of a track and
    /// holds no longer goes back to the track's own (decision 408).
    fn follow_group_macros(&mut self, old: &Published) {
        let (now, was) = (&self.current.group_macros, &old.group_macros);
        if !now.any() && !was.any() {
            return;
        }
        let song = &self.current.song;
        let sounds = song.sound_tracks().min(self.tracks.len());
        for (index, state) in self.tracks.iter_mut().enumerate().take(sounds) {
            let (member, was_member) = (now.members[index], was.members[index]);
            if let Some((group, place)) = was_member {
                let track = &song.tracks[index];
                let holds = |target| {
                    member.is_some_and(|(group, place)| {
                        let macros = &song.tracks[group].macros;
                        macros
                            .iter()
                            .any(|held| held.map_on(Some(place), target).is_some())
                    })
                };
                let before = old.song.tracks[group].macros.iter();
                let mut let_go = false;
                for map in before.flat_map(|held| &held.maps) {
                    if map.member != Some(place) || holds(map.target) {
                        continue;
                    }
                    let_go = true;
                    if let Some(id) = map.target.setting() {
                        state.set_base(id, base_of(id, track));
                    }
                }
                // A generator's setting goes back to its shape, and the track's own macros put
                // theirs again.
                if let_go {
                    state.put_macros_again(track);
                }
            }
            if member.is_some() {
                state.put_group_macros_again();
            }
        }
        self.put_group_macros();
    }

    /// Move the tempo origin here if the tempo changed, so the next step's time stays right.
    fn retune(&mut self) {
        let millibpm = self.current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        if millibpm == self.millibpm {
            return;
        }
        let numerator = u128::from(self.frames_since_origin)
            * u128::from(TICKS_PER_QUARTER)
            * u128::from(self.millibpm);
        let denominator = 60 * u128::from(self.rate) * 1000;
        self.origin_tick += (numerator / denominator) as u64;
        self.frames_since_origin = 0;
        self.millibpm = millibpm;
    }
}

/// The span a voice's slice of its entry plays now, if the entry still plays the voice's sample.
/// A slice gone since it started plays the last, as `SLI` past the last does. A voice with no
/// entry, or one emptied since, has none; the bank always has all 256 places.
fn span_now(
    bank: &Bank,
    entry: Option<u8>,
    (slice, thru, turned): (u8, bool, bool),
    sample: SampleId,
) -> Option<Span> {
    let entry = usize::from(entry?);
    let Some(place) = bank.get(entry) else {
        debug_assert!(false, "the bank has no place {entry}");
        return None;
    };
    let playable = place.as_deref()?;
    (playable.sample == sample).then(|| playable.span(slice, thru, turned))
}

/// Have a voice follow its entry's changes.
fn follow_voice(voice: &mut Voice, bank: &Bank) {
    if let Some(span) = span_now(bank, voice.entry(), voice.slice(), voice.sample()) {
        voice.respan(span);
    }
}

/// Have a note waiting to start follow its entry's changes.
fn follow_note(note: &mut NoteOn, bank: &Bank) {
    let slice = (note.slice, note.cut.mode == PlayMode::Thru, note.cut.turned);
    if let Some(span) = span_now(bank, note.entry, slice, note.sample) {
        note.span = Some(span);
    }
}

#[cfg(test)]
mod tests {
    use vt_model::order::Order;
    use vt_model::samples::SamplePool;
    use vt_model::settings::SettingId;
    use vt_model::song::{Chain, ChainEntry, Lane, Phrase};
    use vt_model::speed::Speed;
    use vt_model::step::Gate;

    use super::*;
    use crate::track_state::LOOKUPS;

    /// How many settings the engine looks up to take a new version of a song.
    fn lookups(engine: &mut Engine, song: &Song) -> usize {
        let samples = Arc::clone(&engine.current().samples);
        let next = Published::after(Arc::new(song.clone()), samples, Some(engine.current()));
        LOOKUPS.with(|count| count.set(0));
        engine.swap(Arc::new(next));
        LOOKUPS.with(std::cell::Cell::get)
    }

    #[test]
    fn a_track_an_edit_didnt_touch_looks_up_no_settings() {
        let mut song = Song::new(16);
        let first = Published::new(Arc::new(song.clone()), Arc::new(SamplePool::default()));
        let mut engine = Engine::new(48_000, Arc::new(first));
        engine.play_from(0);
        engine.process(&mut [0.0; 128]);
        assert_eq!(lookups(&mut engine, &song), 0, "the same song again");
        // A turn of one track's cutoff looks up that track's own settings, on both versions.
        song.tracks[3].cutoff = 0x40;
        let own = (0..).map_while(SettingId::track).count();
        assert_eq!(lookups(&mut engine, &song), 2 * own);
        assert_eq!(engine.live(3).cutoff, 0x40, "and hears it");
    }

    /// A song whose first track plays a chain of two phrases in an order, with rows of two pulses
    /// and of none and a lane on its first FX column, at a speed of its own.
    fn walking(order: Order) -> Song {
        let mut song = Song::new(3);
        song.millibpm = 300_000;
        song.seed = 7;
        let track = &mut song.tracks[0];
        track.speed = Speed::SixteenthTriplet;
        for (id, length) in [(0, 7), (1, 5)] {
            let mut phrase = Phrase::new(length);
            phrase.set_order(order);
            for row in 0..length {
                let step = phrase.step_mut(row).unwrap();
                *step = "C-3 --".parse().unwrap();
                step.gate = Some(Gate {
                    pulses: [1, 2, 0][row % 3],
                    ..Gate::default()
                });
            }
            phrase.set_lane(0, Some(Lane::new(0, 3)));
            track.phrases.insert(id, Arc::new(phrase));
        }
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        let entries = vec![entry(0), entry(1), entry(0)];
        let label = String::new();
        track.chains.insert(0, Arc::new(Chain { entries, label }));
        song
    }

    #[test]
    fn a_track_an_edit_didnt_touch_keeps_its_place_in_every_order() {
        for order in Order::ALL {
            let song = walking(order);
            let first = Published::new(Arc::new(song), Arc::new(SamplePool::default()));
            let mut engine = Engine::new(48_000, Arc::new(first));
            engine.play_from(0);
            let mut last = Arc::clone(engine.current());
            for block in 0..400 {
                engine.process(&mut [0.0; 512]);
                let place = |engine: &Engine| {
                    let state = &engine.tracks[0];
                    (state.started, state.lane_offsets, state.clock.count)
                };
                let before = place(&engine);
                // Another track's cutoff turns, as a knob held does, a version a block.
                let mut next = Song::clone(&last.song);
                next.tracks[1].cutoff = block as u8;
                assert!(next.tracks.same(0, &last.song.tracks, 0));
                let next = Arc::new(Published::after(
                    Arc::new(next),
                    Arc::clone(&last.samples),
                    Some(&last),
                ));
                engine.swap(Arc::clone(&next));
                last = next;
                assert_eq!(place(&engine), before, "{order:?}, block {block}");
            }
            assert!(engine.tracks[0].clock.count > 60, "{order:?} played on");
        }
    }
}
