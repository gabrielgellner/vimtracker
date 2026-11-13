//! Each track's modulation: worked out as the song moves, started and released with its
//! notes, left resting while it would change nothing, and reported.

use vt_model::constants::{MAX_GROUPS, MAX_TRACKS};
use vt_model::envelope::Envelope;
use vt_model::modulation::{ModKind, ModSlot, note_shape_slot};

use crate::track_state::{Drawn, LiveValues, NotePitch, TrackState};

use super::{Engine, RestingBuses, Transport};

impl Engine {
    /// Work out again the modulation of each track a note has just started on, if one of its mod
    /// slots follows the note: a Track slot, or one in scale on `Pitch`, which moves from it. Its
    /// modulation was worked out as the note's step began, before the note's pitch and velocity
    /// were, so this has it right from the note's first frame rather than the next block's. A
    /// voice started afresh takes the pitch at once, as it has played nothing to slew from, so a
    /// note moved in scale is in tune from its start, as a note written there is.
    pub(super) fn track_new_notes(&mut self) {
        if !self.tracks.iter().any(|state| state.new_note) {
            return;
        }
        let now = self.song_time();
        let stopped = self.transport == Transport::Stopped;
        for (index, state) in self.tracks.iter_mut().enumerate() {
            let fresh = std::mem::take(&mut state.fresh_voice);
            if !std::mem::take(&mut state.new_note) {
                continue;
            }
            if let Some(track) = self.current.song.tracks.get(index)
                && track
                    .mods
                    .iter()
                    .any(|slot| slot.kind == ModKind::Track || slot.goes_in_scale())
            {
                state.modulate(track, index, now, stopped, &mut self.memory);
                if fresh {
                    state.voice.snap_pitch();
                }
            }
        }
    }

    /// Work out what each track's mod slots add now, and give its voice and slots their settings.
    /// A track resting, where that would change nothing, is left as it is while it's silent, with
    /// no tail ringing and no preview through it; the groups, returns and master, which hear the
    /// others, while every sound track is (#547).
    pub(super) fn update_lfos(&mut self) {
        let now = self.song_time();
        let stopped = self.transport == Transport::Stopped;
        let sounds = self.sounds();
        let previews = self.previews.sounding();
        let mut silent = [false; MAX_TRACKS];
        let mut all_silent = true;
        for (index, silent) in silent.iter_mut().enumerate().take(sounds) {
            let state = &self.tracks[index];
            *silent = !state.sounding()
                && !state.ringing(self.rate)
                && !self.previews.on_track(&previews, index);
            all_silent &= *silent;
        }
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            let silent = if index < sounds {
                silent[index]
            } else {
                all_silent && !state.ringing(self.rate)
            };
            if silent && self.resting && state.rests(now, stopped) {
                state.keep_resting(now);
            } else {
                state.modulate(track, index, now, stopped, &mut self.memory);
            }
        }
    }

    /// Whether a track at rest is left as it is rather than worked out again, as it is unless a
    /// test asks otherwise to hear that it changes nothing.
    #[doc(hidden)]
    pub fn let_rest(&mut self, on: bool) {
        self.resting = on;
    }

    /// Whether a track is at rest, for tests.
    #[doc(hidden)]
    pub fn rests(&self, index: usize) -> bool {
        self.tracks.get(index).is_some_and(TrackState::at_rest)
    }

    /// Whether the last run left a group, a return or the master out, as working it out would
    /// have given the same frame and changed nothing, by its place among the fixed tracks, for
    /// tests.
    #[doc(hidden)]
    pub fn bus_rested(&self, fixed: usize) -> bool {
        let RestingBuses {
            groups,
            returns,
            master,
        } = self.rested;
        match fixed.checked_sub(MAX_GROUPS) {
            None => groups[fixed],
            Some(place @ 0..2) => returns[place],
            Some(2) => master.is_some(),
            Some(_) => false,
        }
    }

    /// Start a note's modulation: the track's envelopes, and LFOs that follow its notes.
    pub(super) fn start_modulation(&mut self, index: usize) {
        let now = self.song_time();
        let Some(track) = self.current.song.tracks.get(index) else {
            return;
        };
        let state = &mut self.tracks[index];
        state.trigger_mods(track, now);
        state.modulate(track, index, now, false, &mut self.memory);
    }

    /// Begin the release of a track's envelopes, when its note ends.
    pub(super) fn release_modulation(&mut self, index: usize) {
        let now = self.song_time();
        if let Some(track) = self.current.song.tracks.get(index) {
            self.tracks[index].release_mods(track, now);
        }
    }

    /// What each mod slot moved the pitch of a track's last note started by, once, for the report.
    pub fn take_note_pitch(&mut self, index: usize) -> Option<NotePitch> {
        self.tracks.get_mut(index)?.take_note_pitch()
    }

    /// What a track is playing with right now. A sound track with nothing going through it isn't
    /// run, so its slots turn nothing down, whatever they last did.
    pub fn live(&self, index: usize) -> LiveValues {
        match (self.tracks.get(index), self.current.song.tracks.get(index)) {
            (Some(state), Some(track)) => {
                let mut live = state.live();
                let run = !track.is_sound()
                    || state.sounding()
                    || state.ringing(self.rate)
                    || self.previews.on_track(&self.previews.sounding(), index);
                if !run {
                    live.reduction = Default::default();
                }
                // A note heard while stopped works the LFOs out once, but they aren't moving.
                if self.transport == Transport::Stopped {
                    live.mods = Default::default();
                }
                // What `Mutate` drew is heard only from a Steps slot with it up, so only that's
                // reported, and kept.
                for (drawn, slot) in live.drawn.iter_mut().zip(&track.mods) {
                    if slot.kind != ModKind::Steps || slot.steps.mutate == 0 {
                        *drawn = Drawn::default();
                    }
                }
                live
            }
            _ => LiveValues::default(),
        }
    }

    /// The shape of a track's notes: its first envelope on `Vol`, whichever slot that is, if it's
    /// in. Switched out, or with none, its notes have the plain shape a new track's have, from
    /// the next one.
    pub(super) fn note_shape(&self, index: usize) -> Envelope {
        self.current
            .song
            .tracks
            .get(index)
            .and_then(|track| track.mods.get(note_shape_slot(&track.mods)?))
            .filter(|slot| !slot.bypassed)
            .map_or(Envelope::note_shape(), ModSlot::envelope)
    }
}
