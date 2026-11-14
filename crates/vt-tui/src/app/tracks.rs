//! Tracks: moving between them, levels, mutes and solos, at once or on the launch grid,
//! realigning them, and groups.

use std::sync::Arc;

use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::constants::FIXED_TRACKS;
use vt_model::label::is_name;
use vt_model::song::{Launch, TrackKind};

use super::{App, TrackLive};
use crate::input::command::LineError;

impl App {
    /// Keep one set of live controls per track as tracks come and go.
    pub(crate) fn fit_levels(&mut self) {
        // Those of the fixed tracks stay with them, and the engine clears any no longer used.
        let tracks = self.song().tracks.len();
        let was = self.levels.len();
        let (from, to) = (
            was.saturating_sub(FIXED_TRACKS),
            tracks.saturating_sub(FIXED_TRACKS),
        );
        if tracks > was {
            let added = std::iter::repeat_n(TrackLive::default(), tracks - was);
            self.levels.splice(from..from, added);
        } else {
            self.levels.drain(to..from);
        }
    }

    /// Move to the track as many along as `by`, stepping over those folded away, a folded group
    /// one stop (decisions 128 and 305).
    pub(crate) fn move_track(&mut self, by: isize) {
        let order = self.track_order();
        let track = self.folds.step(self.song(), self.cursor.track, by, order);
        self.go_to_track_at(track);
    }

    /// Go to another track, staying at the same position in the section.
    pub(crate) fn go_to_track_at(&mut self, track: usize) {
        let track = track.min(self.song().tracks.len() - 1);
        if track == self.cursor.track {
            return;
        }
        let section_step = self.section_step();
        let extent = self.extent();
        // A row outside the loop has no place in the section, so it keeps its number instead.
        let outside = (!extent.plays(self.cursor.step)).then(|| extent.row(self.cursor.step));
        let from = self.cursor.track;
        self.cursor.track = track;
        let song = Arc::clone(self.song());
        let position = song
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.chains.get(track).copied().flatten())
            .and_then(|chain| {
                let place = self.step_on(track, from, section_step);
                song.tracks[track].chain_position(chain, place, self.dealer(track))
            });
        match position {
            Some(position) => {
                self.cursor.slot = position.slot;
                let row = outside.unwrap_or(position.step as isize);
                self.cursor.step = self.extent().step_of(row);
            }
            None => self.cursor.slot = 0,
        }
    }

    /// Whether this track can be muted and soloed, saying so if not.
    fn can_silence(&mut self) -> bool {
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
        if !self.can_silence() {
            return;
        }
        let track = self.cursor.track;
        let Some(live) = self.levels.get(track).copied() else {
            return;
        };
        let name = format!("Track {:02}", track + 1);
        let (on, verb) = if solo {
            (live.solo, if live.solo { "unsolos" } else { "solos" })
        } else {
            (live.mute, if live.mute { "unmutes" } else { "mutes" })
        };
        if self.transport != Transport::Playing {
            if solo {
                self.switch_solo(track);
            } else {
                self.switch_mute(track);
            }
            let done = match (solo, on) {
                (false, false) => "muted",
                (false, true) => "unmuted",
                (true, false) => "soloed",
                (true, true) => "unsoloed",
            };
            self.status = format!("{name} {done} at once, as nothing's playing");
            return;
        }
        let command = if solo {
            Command::SoloOnGrid { track }
        } else {
            Command::MuteOnGrid { track }
        };
        if !self.handle.send(command) {
            return;
        }
        let jam = self.jam.get(track).copied().unwrap_or_default();
        let (waiting, what, key) = if solo {
            (jam.solo_waiting, "solo", "s")
        } else {
            (jam.mute_waiting, "mute", "m")
        };
        // The engine decides, and the status line shows it waiting from its next report.
        self.status = if waiting {
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
        if self.transport != Transport::Playing {
            self.status = "Realign works while playing".to_owned();
            return;
        }
        if !self.handle.send(Command::Realign) {
            return;
        }
        // The engine decides, and the transport shows it from its next report.
        self.status = if self.realigning {
            "Realign taken back".to_owned()
        } else {
            let when = self.grid_when();
            format!("Every track realigns {when}; Space A again takes it back")
        };
    }

    /// Start the cursor's track's chain again at the next line of the launch grid, or take back
    /// one waiting for it, as `Space A` does for every track.
    pub(crate) fn realign_track(&mut self) {
        let track = self.cursor.track;
        let Some(owner) = self.song().tracks.get(track) else {
            return;
        };
        if !owner.is_sound() {
            self.status = format!("{} plays no chain to realign", owner.kind.title());
            return;
        }
        if self.transport != Transport::Playing {
            self.status = "Realign works while playing".to_owned();
            return;
        }
        if !self.handle.send(Command::RealignTrack { track }) {
            return;
        }
        let name = format!("Track {:02}", track + 1);
        // The engine decides, and the transport shows it from its next report.
        self.status = if self.realigning_tracks[track] {
            format!("{name}'s realign taken back")
        } else {
            let when = self.grid_when();
            format!("{name} realigns {when}; Space a again takes it back")
        };
    }

    /// Turn fill on or off, saying which.
    pub(crate) fn set_fill(&mut self, on: bool) {
        self.fill = on;
        self.handle.send(Command::SetFill { on });
        self.status = if on { "Fill on" } else { "Fill off" }.to_owned();
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

    pub(super) fn clear_live(&mut self) {
        self.levels.fill(TrackLive::default());
        self.send_live();
        self.dirty = true;
    }

    /// Put tracks in a group, or take them out of any, as one undo step: those numbered from the
    /// first to the last, counted from 1, or the cursor's.
    pub(super) fn set_group(&mut self, group: Option<u8>, tracks: Option<(usize, usize)>) {
        let sounds = self.song().sound_tracks();
        let (first, last) = match tracks {
            Some((first, last)) => (first - 1, last - 1),
            None => (self.cursor.track, self.cursor.track),
        };
        if last >= sounds {
            self.status = match tracks {
                Some(_) => format!("There are {sounds} tracks that play sounds"),
                None => "Only tracks that play sounds go in a group".to_owned(),
            };
            return;
        }
        self.apply(&Action::SetTrackGroup { first, last, group });
        let which = if first == last {
            format!("Track {:02}", first + 1)
        } else {
            format!("Tracks {:02}-{:02}", first + 1, last + 1)
        };
        self.status = match group {
            Some(group) => format!("{which} in group {group}"),
            None => format!("{which} in no group"),
        };
    }

    /// `:name`: name the cursor's track, in two characters, or on a group's own track up to
    /// five, as `:group name` does (decision 308).
    pub(super) fn name_track(&mut self, name: Option<String>) {
        if self.song().tracks[self.cursor.track].is_group() {
            self.name_group(name);
            return;
        }
        if name.as_deref().is_some_and(|name| !is_name(name)) {
            self.status = LineError::Name.to_string();
            return;
        }
        self.apply(&Action::SetTrackName {
            track: self.cursor.track,
            name: name.unwrap_or_default(),
        });
    }

    /// Name the cursor's group, or the group the cursor's track is in, as one undo step, or with
    /// none give it back its letters, as `G1`.
    pub(super) fn name_group(&mut self, name: Option<String>) {
        let song = self.song();
        let here = &song.tracks[self.cursor.track];
        let number = match here.kind {
            TrackKind::Group(number) => Some(number),
            _ => here.group,
        };
        let Some((number, track)) =
            number.and_then(|number| Some((number, song.fixed_track(TrackKind::Group(number))?)))
        else {
            self.status = "This track is in no group".to_owned();
            return;
        };
        let name = name.unwrap_or_default();
        self.apply(&Action::SetTrackName {
            track,
            name: name.clone(),
        });
        self.status = if name.is_empty() {
            format!("Group {number} unnamed")
        } else {
            format!("Group {number} named {name}")
        };
    }
}
