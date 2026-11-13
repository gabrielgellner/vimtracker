//! What a hand changes as it plays: fill, the scale, the faders it takes over and gives
//! back, and the mutes and solos, with what they leave heard.

use vt_model::constants::{MAX_GROUPS, MAX_TRACKS};
use vt_model::scale::Scale;

use crate::track_state::{Faders, TrackState};

use super::{Engine, Transport};

impl Engine {
    /// Turn fill on or off, which `CND`'s `FILL` and `!FILL` read as each row is decided.
    pub fn set_fill(&mut self, on: bool) {
        self.fill = on;
    }

    /// Whether fill is on.
    pub fn fill(&self) -> bool {
        self.fill
    }

    /// Set the scale an `ACU` moves notes through, from the next note it moves.
    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    /// A track's faders as they're heard, for the mixer, the Output device and the Roto.
    pub fn faders(&self, index: usize) -> Faders {
        self.tracks
            .get(index)
            .map(TrackState::faders)
            .unwrap_or_default()
    }

    /// A hand moved one of a track's faders that the steps drive: it's the hand's from now, until
    /// it's given back (decision 174).
    pub fn take_over(&mut self, track: usize, fader: usize) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.take_over(fader);
        }
    }

    /// Give a track's faders back to the steps, or every track's, as `Space e` and `Space E` do.
    pub fn give_back(&mut self, track: Option<usize>) {
        match track {
            Some(track) => {
                if let Some(state) = self.tracks.get_mut(track) {
                    state.give_back_all();
                }
            }
            None => {
                for state in &mut self.tracks {
                    state.give_back_all();
                }
            }
        }
    }

    /// Set whether a track is muted, taking back a switch of its mute waiting for the grid. A
    /// muted track's sounding note fades.
    pub fn set_mute(&mut self, track: usize, mute: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute = mute;
            state.mute_waiting = false;
        }
        self.apply_audibility();
    }

    /// Switch a track's mute at the next line of the launch grid, or take back a switch waiting
    /// for it. Only while playing, as there's no grid coming round otherwise.
    pub fn toggle_mute_on_grid(&mut self, track: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute_waiting = playing && !state.mute_waiting;
        }
    }

    /// Switch a track's solo at the next line of the launch grid, or take back a switch waiting
    /// for it, as [`Engine::toggle_mute_on_grid`] does a mute.
    pub fn toggle_solo_on_grid(&mut self, track: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo_waiting = playing && !state.solo_waiting;
        }
    }

    /// Whether a track's mute switches at the next line of the launch grid.
    pub fn mute_waiting(&self, track: usize) -> bool {
        self.tracks.get(track).is_some_and(|t| t.mute_waiting)
    }

    /// Whether a track's solo switches at the next line of the launch grid.
    pub fn solo_waiting(&self, track: usize) -> bool {
        self.tracks.get(track).is_some_and(|t| t.solo_waiting)
    }

    /// Switch every mute and solo waiting for this line of the grid, together, so they're heard
    /// from the same sample.
    pub(super) fn switch_waiting(&mut self) {
        for track in self.tracks.iter_mut() {
            if !(track.mute_waiting || track.solo_waiting) {
                continue;
            }
            track.mute ^= track.mute_waiting;
            track.solo ^= track.solo_waiting;
            track.mute_waiting = false;
            track.solo_waiting = false;
            track.switches_landed = track.switches_landed.wrapping_add(1);
        }
        self.apply_audibility();
    }

    /// Set whether a track is soloed. While a track that plays sounds or a group is soloed, only
    /// soloed ones play, with the groups of soloed tracks and the tracks of soloed groups, and the
    /// returns stay. While a return is soloed, only soloed returns are heard, and what sends to
    /// them keeps playing without being heard itself.
    pub fn set_solo(&mut self, track: usize, solo: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo = solo;
            state.solo_waiting = false;
        }
        self.apply_audibility();
    }

    /// Whether a sound track or a group is soloed.
    pub(super) fn any_solo(&self) -> bool {
        self.tracks[..self.returns()].iter().any(|t| t.solo)
    }

    /// Whether a return is soloed.
    pub(super) fn return_soloed(&self) -> bool {
        self.tracks[self.returns()..self.returns() + 2]
            .iter()
            .any(|t| t.solo)
    }

    /// Where the returns start, after the sound tracks and the groups.
    pub(super) fn returns(&self) -> usize {
        self.sounds() + MAX_GROUPS
    }

    /// The group track a sound track plays through, if it's in one.
    pub(super) fn group_of(&self, index: usize) -> Option<usize> {
        self.current
            .song
            .group_track_of(index)
            .filter(|group| *group < MAX_TRACKS)
    }

    /// Whether a sound track is heard, given whether anything's soloed: not muted, nor in a muted
    /// group, and, while something is soloed, soloed itself or in a soloed group.
    pub(super) fn heard_with(&self, index: usize, any_solo: bool) -> bool {
        let state = &self.tracks[index];
        let group = self.group_of(index).map(|group| &self.tracks[group]);
        !state.mute
            && !group.is_some_and(|group| group.mute)
            && (!any_solo || state.solo || group.is_some_and(|group| group.solo))
    }

    /// Whether a sound track plays into its group, as it's heard but leaving out what the group
    /// decides for itself: its own mute, and solos outside it. Only a track soloed in the same
    /// group, and the group not soloed, keeps its other tracks out. With the group's own fade
    /// after, what's heard is the same as by `heard_with`.
    fn feeds_with(&self, index: usize, group: usize, any_solo: bool) -> bool {
        let state = &self.tracks[index];
        let lit =
            || (0..self.sounds()).any(|t| self.tracks[t].solo && self.group_of(t) == Some(group));
        !state.mute && (!any_solo || state.solo || self.tracks[group].solo || !lit())
    }

    /// Whether a group is heard, given whether anything's soloed: not muted, and, while something
    /// is soloed, soloed itself or holding a soloed track.
    pub(super) fn group_heard_with(&self, group: usize, any_solo: bool) -> bool {
        let state = &self.tracks[group];
        let lit =
            || (0..self.sounds()).any(|t| self.tracks[t].solo && self.group_of(t) == Some(group));
        !state.mute && (!any_solo || state.solo || lit())
    }

    pub(super) fn apply_audibility(&mut self) {
        let any_solo = self.any_solo();
        let return_soloed = self.return_soloed();
        let (sounds, returns) = (self.sounds(), self.returns());
        for index in 0..sounds {
            let heard = self.heard_with(index, any_solo);
            // A group a compressor or Warps hears decides for itself whether it's heard, and its
            // tracks play into it all the same.
            let feeds = match self.group_of(index) {
                Some(group) if self.listened[group] => self.feeds_with(index, group, any_solo),
                _ => heard,
            };
            let track = &mut self.tracks[index];
            // One a compressor or Warps hears keeps playing, faded out of the mix and the sends.
            track.heard.target = if heard { 1.0 } else { 0.0 };
            track.feeds.target = if feeds { 1.0 } else { 0.0 };
            if !heard && !self.listened[index] {
                track.cut_to_tail();
            }
        }
        for group in sounds..returns {
            let heard = self.group_heard_with(group, any_solo);
            self.tracks[group].heard.target = if heard { 1.0 } else { 0.0 };
        }
        for track in self.tracks[returns..returns + 2].iter_mut() {
            let heard = !track.mute && (!return_soloed || track.solo);
            track.heard.target = if heard { 1.0 } else { 0.0 };
        }
    }
}
