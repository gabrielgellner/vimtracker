//! Taking a new version of the song: its memory, its bank, its tracks as they come and go,
//! and what each compressor or Warps hears.

use std::sync::Arc;

use vt_model::constants::{
    FIXED_TRACKS, MAX_MILLIBPM, MAX_SOUND_TRACKS, MAX_TRACKS, MIN_MILLIBPM, TICKS_PER_QUARTER,
    TICKS_PER_STEP,
};
use vt_model::insert::Insert;
use vt_model::samples::SampleId;
use vt_model::settings::{PlayMode, SettingId};
use vt_model::song::{Song, Track};

use crate::dsp::memory::{Memory, MemoryCount};
use crate::gens::GenScratch;
use crate::published::{Bank, Published, Span};
use crate::track_state::base_of;
use crate::voice::{NoteOn, Voice};

use super::{Engine, Transport};

impl Engine {
    /// Keep a long buffer the edit thread has sent, for a version it's about to send.
    pub fn receive_memory(&mut self, memory: Memory) {
        self.memory.put(memory);
    }

    /// The edit thread wants this many back, as they're more than its versions need.
    pub fn release_memory(&mut self, count: MemoryCount) {
        self.memory.owe(count);
    }

    /// The next long buffer to send back to be freed.
    pub fn next_freed_memory(&mut self) -> Option<Memory> {
        self.memory.next_owed()
    }

    /// How many long buffers are held that no slot is using.
    pub fn spare_memory(&self) -> usize {
        self.memory.spare()
    }

    /// The id of the version playing, [`Published::id`].
    pub fn version(&self) -> u64 {
        self.current.id
    }

    /// Switch to a new version, returning the old one so it can be freed elsewhere.
    ///
    /// A track whose chain changed length stays on the row it was going to play next. An ID that
    /// no longer exists is silent, and a removed track's voices fade.
    pub fn swap(&mut self, version: Arc<Published>) -> Arc<Published> {
        let old = std::mem::replace(&mut self.current, version);
        // The new version's scratch for generators is the engine's now, and the one it had goes
        // back with the old version, to be dropped on the edit thread.
        let mut scratch = GenScratch::default();
        self.current.scratch.exchange(&mut scratch);
        std::mem::swap(&mut self.gens, &mut scratch);
        old.scratch.exchange(&mut scratch);
        // What any track plays with may have changed.
        for track in self.tracks.iter_mut() {
            track.wake();
        }
        self.used = self
            .used
            .max(self.current.song.tracks.len().min(MAX_TRACKS));
        self.keep_fixed_tracks(old.song.sound_tracks());
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
        old
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
    /// count since its chain started carries on.
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
            let state = &mut self.tracks[index];
            let place = state.clock.count;
            let count = place - state.started;
            let next = count.rem_euclid(old_length);
            let laps = count.div_euclid(old_length);
            let lapped = |length: i64, at: i64| (laps.max(0) * length + at) as usize;
            let Some(was_at) = before.chain_position(chain, lapped(old_length, next), dealer)
            else {
                continue;
            };
            // The same pulse of the row too, as far as the row still lasts, where this pass of
            // its phrase's walk plays it nearest to where it was, or where it was if the pass
            // doesn't play it.
            let kept = track
                .chain_place(chain, was_at.row_place(), laps.max(0) as usize, dealer)
                .unwrap_or(0) as i64;
            let moved = length != old_length || kept != next;
            if moved {
                state.started = place - (laps * length + kept);
            }
            let (Some(old_phrase), Some(at)) = (
                before.phrases.get(&was_at.phrase),
                track.chain_position(chain, lapped(length, kept), dealer),
            ) else {
                continue;
            };
            let Some(phrase) = track.phrases.get(&at.phrase) else {
                continue;
            };
            let new_count = place - state.started;
            for (slot, lane_offset) in state.lane_offsets.iter_mut().enumerate() {
                let (old_lane, lane) = (old_phrase.lane(slot), phrase.lane(slot));
                if !moved && old_lane == lane && was_at.phrase == at.phrase {
                    continue;
                }
                let Some(lane) = lane else {
                    *lane_offset = 0;
                    continue;
                };
                // The step this column was going to read, which it keeps if its lane still has it.
                let was_reading = match old_lane {
                    Some(old_lane) => old_lane.step_at(count + *lane_offset),
                    None => old_phrase.start() + was_at.step,
                };
                let target = if was_at.phrase == at.phrase && lane.contains(was_reading) {
                    was_reading
                } else {
                    lane.start
                };
                let offset = (target - lane.start) as i64 - new_count;
                *lane_offset = offset.rem_euclid(lane.length.max(1) as i64);
            }
        }
    }

    /// How many tracks play sounds, and so where the fixed tracks start.
    pub(super) fn sounds(&self) -> usize {
        self.current.song.sound_tracks().min(MAX_SOUND_TRACKS)
    }

    /// Hear a change to a track's page straight away. A held value a step has set since stays
    /// until something changes it, so this only acts on what actually changed.
    fn follow_base(&mut self, old: &Published) {
        let fresh = Track::default();
        let (now, was) = (self.sounds(), old.song.sound_tracks());
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            // A fixed track is compared with what it was, wherever that was.
            let then = if index >= now {
                index - now + was
            } else {
                index
            };
            let before = if index >= now || index < was {
                old.song.tracks.get(then).unwrap_or(&fresh)
            } else {
                &fresh
            };
            for id in SettingId::all() {
                let (was, now) = (base_of(id, before), base_of(id, track));
                if was != now {
                    state.set_base(id, now);
                }
            }
            // A setting a macro no longer holds goes back to the song's, and the macros put what
            // they hold where they have it again, as their maps may have changed (decision 284).
            for map in before.macros.iter().flat_map(|held| &held.maps) {
                if track.macro_holding(map.target).is_none() {
                    let id = map.target.setting();
                    state.set_base(id, base_of(id, track));
                }
            }
            state.put_macros_again(track);
        }
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
/// A slice gone since it started plays the last, as `SLI` past the last does.
fn span_now(
    bank: &Bank,
    entry: Option<u8>,
    (slice, thru): (u8, bool),
    sample: SampleId,
) -> Option<Span> {
    let playable = bank.get(usize::from(entry?))?.as_deref()?;
    (playable.sample == sample).then(|| playable.span(slice, thru))
}

/// Have a voice follow its entry's changes.
fn follow_voice(voice: &mut Voice, bank: &Bank) {
    if let Some(span) = span_now(bank, voice.entry(), voice.slice(), voice.sample()) {
        voice.respan(span);
    }
}

/// Have a note waiting to start follow its entry's changes.
fn follow_note(note: &mut NoteOn, bank: &Bank) {
    let slice = (note.slice, note.cut.mode == PlayMode::Thru);
    if let Some(span) = span_now(bank, note.entry, slice, note.sample) {
        note.span = Some(span);
    }
}
