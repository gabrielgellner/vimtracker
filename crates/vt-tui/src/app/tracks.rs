//! Tracks: moving between them, levels, mutes and solos, at once or on the launch grid,
//! realigning them, rotating their speeds, and groups.

use std::sync::Arc;

use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::bank::BankEntry;
use vt_model::constants::FIXED_TRACKS;
use vt_model::label::is_name;
use vt_model::song::{Launch, Machine, TrackKind};
use vt_model::speed::Speed;

use super::{App, TrackLive};
use crate::overlays::command::parse::LineError;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::services::Services;
use crate::widgets::{group, role};

impl Services {
    /// The list of tracks, like Vim's buffers, with this one highlighted. It opens on its search,
    /// as a track is found by name or number, as Vim's buffers are.
    pub(crate) fn track_picker(&self) -> Picker {
        let song = self.song();
        let items = song
            .tracks
            .iter()
            .enumerate()
            .map(|(track, state)| {
                let sound = match state.machine {
                    _ if !state.is_sound() => state.kind.title().to_owned(),
                    Machine::Sampler => song
                        .bank
                        .get(state.sample())
                        .map_or_else(String::new, BankEntry::title),
                    Machine::Plaits(voice) => voice.name().to_owned(),
                };
                let number = if state.is_sound() {
                    format!("{:02}", track + 1)
                } else {
                    String::new()
                };
                // A group and the tracks in it are in its colour, and what each is for is the rule
                // under it, as on the mixer.
                let style = group::style(song, track).unwrap_or_default();
                Item::new(
                    format!("{number:<2}  {}", state.name).trim_end(),
                    Pick::Track(track),
                )
                .meaning(sound)
                .styled(style.patch(role::rule(song, track)))
            })
            .collect();
        Picker::new("Tracks", items)
            .opened_on(Some(self.cursor.track))
            .searching("")
    }

    /// Whether this track can be muted and soloed, saying so if not.
    pub(crate) fn can_silence(&mut self) -> bool {
        let master = self
            .song()
            .tracks
            .get(self.cursor.track)
            .is_some_and(|track| track.kind == TrackKind::Master);
        if master {
            self.status = "The master can't be muted or soloed".to_owned();
        }
        !master
    }

    pub(crate) fn toggle_mute(&mut self) {
        if !self.can_silence() {
            return;
        }
        self.switch_mute(self.cursor.track);
    }

    /// Mutes a track, or unmutes it, as the keys do: not an undo step, as it's how the song
    /// plays that it changes, but saved with the song.
    pub(crate) fn switch_mute(&mut self, track: usize) {
        if let Some(live) = self.levels.get_mut(track) {
            live.mute = !live.mute;
            let mute = live.mute;
            self.handle.send(Command::SetMute { track, mute });
            self.dirty = true;
        }
    }

    pub(crate) fn toggle_solo(&mut self) {
        if !self.can_silence() {
            return;
        }
        self.switch_solo(self.cursor.track);
    }

    /// Solos a track, or takes its solo off, as the keys do.
    pub(crate) fn switch_solo(&mut self, track: usize) {
        if let Some(live) = self.levels.get_mut(track) {
            live.solo = !live.solo;
            let solo = live.solo;
            self.handle.send(Command::SetSolo { track, solo });
            self.dirty = true;
        }
    }
}

impl App {
    /// Keep one set of live controls per track as tracks come and go.
    pub(crate) fn fit_levels(&mut self) {
        // Those of the fixed tracks stay with them, and the engine clears any no longer used.
        let tracks = self.song().tracks.len();
        let was = self.services.levels.len();
        let (from, to) = (
            was.saturating_sub(FIXED_TRACKS),
            tracks.saturating_sub(FIXED_TRACKS),
        );
        if tracks > was {
            let added = std::iter::repeat_n(TrackLive::default(), tracks - was);
            self.services.levels.splice(from..from, added);
        } else {
            self.services.levels.drain(to..from);
        }
    }

    /// When a realign or a switch on the grid lands: on the launch grid's next line, or on the next
    /// bar when launches wait for a chain's end, as each track's chain ends somewhere else.
    pub(crate) fn grid_when(&self) -> String {
        match self.song().launch {
            Launch::ChainEnd => "on the next bar".to_owned(),
            every => every.when(),
        }
    }

    /// Switch the cursor's track's mute, or its solo, at the next line of the launch grid, or
    /// take back a switch waiting for it. The engine switches it, with every other waiting for the
    /// same line, and the live controls follow from its report. Stopped, there's no grid coming
    /// round, so it switches at once, as `Space m` does.
    pub(crate) fn switch_on_grid(&mut self, solo: bool) {
        if !self.services.can_silence() {
            return;
        }
        let track = self.services.cursor.track;
        let Some(live) = self.services.levels.get(track).copied() else {
            return;
        };
        let name = format!("Track {:02}", track + 1);
        let (on, verb) = if solo {
            (live.solo, if live.solo { "unsolos" } else { "solos" })
        } else {
            (live.mute, if live.mute { "unmutes" } else { "mutes" })
        };
        if self.services.transport != Transport::Playing {
            if solo {
                self.services.switch_solo(track);
            } else {
                self.services.switch_mute(track);
            }
            let done = match (solo, on) {
                (false, false) => "muted",
                (false, true) => "unmuted",
                (true, false) => "soloed",
                (true, true) => "unsoloed",
            };
            self.services.status = format!("{name} {done} at once, as nothing's playing");
            return;
        }
        let command = if solo {
            Command::SoloOnGrid { track }
        } else {
            Command::MuteOnGrid { track }
        };
        if !self.services.handle.send(command) {
            return;
        }
        let jam = self.services.jam.get(track).copied().unwrap_or_default();
        let (waiting, what, key) = if solo {
            (jam.solo_waiting, "solo", "s")
        } else {
            (jam.mute_waiting, "mute", "m")
        };
        // The engine decides, and the status line shows it waiting from its next report.
        self.services.status = if waiting {
            format!("{name}'s {what} switch taken back")
        } else {
            let when = self.grid_when();
            format!("{name} {verb} {when}; Space g {key} again takes it back")
        };
    }

    /// Send every track back to its start at the next line of the launch grid, or take back a
    /// realign waiting for it. It moves where playback is rather than the song, so it isn't an
    /// undo step.
    pub(crate) fn realign(&mut self) {
        if self.services.transport != Transport::Playing {
            self.services.status = "Realign works while playing".to_owned();
            return;
        }
        if !self.services.handle.send(Command::Realign) {
            return;
        }
        // The engine decides, and the transport shows it from its next report.
        self.services.status = if self.services.realigning {
            "Realign taken back".to_owned()
        } else {
            let when = self.grid_when();
            format!("Every track realigns {when}; Space A again takes it back")
        };
    }

    /// Start the cursor's track's chain again at the next line of the launch grid, or take back
    /// one waiting for it, as `Space A` does for every track.
    pub(crate) fn realign_track(&mut self) {
        let track = self.services.cursor.track;
        let Some(owner) = self.song().tracks.get(track) else {
            return;
        };
        if !owner.is_sound() {
            self.services.status = format!("{} plays no chain to realign", owner.kind.title());
            return;
        }
        if self.services.transport != Transport::Playing {
            self.services.status = "Realign works while playing".to_owned();
            return;
        }
        if !self.services.handle.send(Command::RealignTrack { track }) {
            return;
        }
        let name = format!("Track {:02}", track + 1);
        // The engine decides, and the transport shows it from its next report.
        self.services.status = if self.services.realigning_tracks[track] {
            format!("{name}'s realign taken back")
        } else {
            let when = self.grid_when();
            format!("{name} realigns {when}; Space a again takes it back")
        };
    }

    /// Rotate the speeds round the tracks selected, the cursor's group or, `every`, every track
    /// that plays sounds: each takes the next one's `Speed` and the last the first's, as the 4ms
    /// Shuffling Clock Multiplier's Rotate passes its outputs' multiplications along (decision
    /// 328). A count rotates that many times, so as many presses as tracks bring them back. It's
    /// one undo step. While recording, the speeds go in as held `RATE`s on the next row each track
    /// plays instead, so the pass plays the rotation back where it fell.
    pub(crate) fn rotate_speeds(&mut self, every: bool) {
        let times = self.modes.count.take().unwrap_or(1);
        let tracks = match self.tracks_to_rotate(every) {
            Ok(tracks) => tracks,
            Err(why) => {
                self.services.status = why;
                return;
            }
        };
        if self.recording() {
            self.record_rotation(&tracks, times);
            return;
        }
        let song = Arc::clone(self.song());
        let speeds: Vec<Speed> = tracks.iter().map(|&t| song.tracks[t].speed).collect();
        let turned = rotated(&speeds, times);
        let changes: Vec<Action> = tracks
            .iter()
            .zip(&turned)
            .filter(|&(&track, &speed)| song.tracks[track].speed != speed)
            .map(|(&track, &speed)| Action::SetTrackSpeed { track, speed })
            .collect();
        if changes.is_empty() {
            self.services.status = format!(
                "Rotating changes nothing: these tracks are all at {}",
                speeds[0]
            );
            return;
        }
        self.services.apply(&Action::Batch(changes));
        self.services.status = format!("Speeds rotated: {}", speeds_said(&tracks, &turned));
    }

    /// The tracks a rotation goes round, in order: those selected, every track that plays sounds,
    /// or those in the cursor's group, or why there aren't two of them.
    fn tracks_to_rotate(&mut self, every: bool) -> Result<Vec<usize>, String> {
        let song = Arc::clone(self.song());
        let sounds = |track: &usize| song.tracks.get(*track).is_some_and(|t| t.is_sound());
        let tracks: Vec<usize> = if self.modes.mode.is_visual() {
            let selection = self.current_selection();
            self.leave_selection(selection);
            (selection.tracks.first..=selection.tracks.last)
                .filter(sounds)
                .collect()
        } else if every {
            (0..song.tracks.len()).filter(sounds).collect()
        } else {
            let here = &song.tracks[self.services.cursor.track];
            let group = match here.kind {
                TrackKind::Group(number) => Some(number),
                _ => here.group,
            };
            let Some(group) = group else {
                return Err(format!(
                    "Track {:02} is in no group: select tracks with v and W, or Space O rotates \
                     every track",
                    self.services.cursor.track + 1
                ));
            };
            let tracks: Vec<usize> = (0..song.tracks.len())
                .filter(|track| sounds(track) && song.tracks[*track].group == Some(group))
                .collect();
            if tracks.len() < 2 {
                return Err(format!(
                    "Group {group} needs two tracks or more to rotate their speeds"
                ));
            }
            tracks
        };
        if tracks.len() < 2 {
            return Err("Rotating needs two tracks or more: reach them with W".to_owned());
        }
        Ok(tracks)
    }

    /// Turn fill on or off, saying which.
    pub(crate) fn set_fill(&mut self, on: bool) {
        self.services.fill = on;
        self.services.handle.send(Command::SetFill { on });
        self.services.status = if on { "Fill on" } else { "Fill off" }.to_owned();
    }

    pub(super) fn clear_live(&mut self) {
        self.services.levels.fill(TrackLive::default());
        self.send_live();
        self.services.dirty = true;
    }

    /// Put tracks in a group, or take them out of any, as one undo step: those numbered from the
    /// first to the last, counted from 1, or the cursor's.
    pub(super) fn set_group(&mut self, group: Option<u8>, tracks: Option<(usize, usize)>) {
        let sounds = self.song().sound_tracks();
        let (first, last) = match tracks {
            Some((first, last)) => (first - 1, last - 1),
            None => (self.services.cursor.track, self.services.cursor.track),
        };
        if last >= sounds {
            self.services.status = match tracks {
                Some(_) => format!("There are {sounds} tracks that play sounds"),
                None => "Only tracks that play sounds go in a group".to_owned(),
            };
            return;
        }
        self.services
            .apply(&Action::SetTrackGroup { first, last, group });
        let which = if first == last {
            format!("Track {:02}", first + 1)
        } else {
            format!("Tracks {:02}-{:02}", first + 1, last + 1)
        };
        self.services.status = match group {
            Some(group) => format!("{which} in group {group}"),
            None => format!("{which} in no group"),
        };
    }

    /// `:name`: name the cursor's track, in two characters, or on a group's own track up to
    /// five, as `:group name` does (decision 308).
    pub(super) fn name_track(&mut self, name: Option<String>) {
        if self.song().tracks[self.services.cursor.track].is_group() {
            self.name_group(name);
            return;
        }
        if name.as_deref().is_some_and(|name| !is_name(name)) {
            self.services.status = LineError::Name.to_string();
            return;
        }
        self.services.apply(&Action::SetTrackName {
            track: self.services.cursor.track,
            name: name.unwrap_or_default(),
        });
    }

    /// Name the cursor's group, or the group the cursor's track is in, as one undo step, or with
    /// none give it back its letters, as `G1`.
    pub(super) fn name_group(&mut self, name: Option<String>) {
        let song = self.song();
        let here = &song.tracks[self.services.cursor.track];
        let number = match here.kind {
            TrackKind::Group(number) => Some(number),
            _ => here.group,
        };
        let Some((number, track)) =
            number.and_then(|number| Some((number, song.fixed_track(TrackKind::Group(number))?)))
        else {
            self.services.status = "This track is in no group".to_owned();
            return;
        };
        let name = name.unwrap_or_default();
        self.services.apply(&Action::SetTrackName {
            track,
            name: name.clone(),
        });
        self.services.status = if name.is_empty() {
            format!("Group {number} unnamed")
        } else {
            format!("Group {number} named {name}")
        };
    }
}

/// Speeds rotated this many times: each takes the one after it, and the last the first's.
pub(crate) fn rotated(speeds: &[Speed], times: usize) -> Vec<Speed> {
    let mut turned = speeds.to_vec();
    if !speeds.is_empty() {
        turned.rotate_left(times % speeds.len());
    }
    turned
}

/// Tracks and their speeds as the status line says them, `01 1/8, 02 1/16`, or how many tracks
/// when there are more than fit.
pub(crate) fn speeds_said(tracks: &[usize], speeds: &[Speed]) -> String {
    const MOST_SAID: usize = 8;
    if tracks.len() > MOST_SAID {
        return format!("{} tracks, each at the next one's", tracks.len());
    }
    let said: Vec<String> = tracks
        .iter()
        .zip(speeds)
        .map(|(track, speed)| format!("{:02} {speed}", track + 1))
        .collect();
    said.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_many_rotations_as_speeds_bring_them_back() {
        let speeds = [Speed::Sixteenth, Speed::Eighth, Speed::EighthTriplet];
        assert_eq!(
            rotated(&speeds, 1),
            [Speed::Eighth, Speed::EighthTriplet, Speed::Sixteenth]
        );
        assert_eq!(rotated(&speeds, 2), rotated(&rotated(&speeds, 1), 1));
        assert_eq!(rotated(&speeds, 3), speeds);
        assert_eq!(rotated(&speeds, 4), rotated(&speeds, 1));
    }
}
