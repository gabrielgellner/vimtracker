//! The faders as the steps automate them (`docs/designs/mixer-automation.md`): a track's `Level`,
//! `Pan` and sends show the steps' values while `LEV`, `PAN`, `SNA` and `SNB` drive them, a hand
//! moving one takes it over from the steps, and `Space e` and `Space E` give them back. The engine
//! keeps which are taken, as it keeps the mutes, and says so in each report; what the app has asked
//! for and the engine hasn't yet said is kept here for a moment, so nothing flickers back while the
//! request is on its way.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use vt_engine::link::Command;
use vt_engine::track_state::Faders;
use vt_model::action::Action;
use vt_model::settings::{FADER_COUNT, FADERS, SettingId};
use vt_model::song::Track;

use crate::app::App;
use crate::pages::track::Control;
use crate::services::Services;

/// How long a takeover or a giving back asked for is shown before the engine says so, after which
/// the engine's word is taken, as it can't have heard.
const WISH_KEPT: Duration = Duration::from_millis(500);

/// A takeover, or a giving back, asked of the engine and not yet in its reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wish {
    track: usize,
    fader: usize,
    taken: bool,
    at: Instant,
}

/// Which of a track's faders a control is, if it's one.
pub(crate) fn fader_of(control: Control) -> Option<usize> {
    match control {
        Control::Level => SettingId::LEVEL.fader(),
        Control::Base(which) => which.setting().and_then(SettingId::fader),
        _ => None,
    }
}

/// The track's faders a hand moved in an edit: a level, a pan or a send set, or a pan's fine part.
fn moved_by_hand(action: &Action, into: &mut Vec<(usize, usize)>) {
    match action {
        Action::SetTrackLevel { track, .. } => {
            into.extend(SettingId::LEVEL.fader().map(|f| (*track, f)))
        }
        Action::SetTrackBase { track, control, .. } => {
            into.extend(
                control
                    .setting()
                    .and_then(SettingId::fader)
                    .map(|f| (*track, f)),
            );
        }
        Action::SetTrackFine { track, setting, .. } => {
            into.extend(setting.fader().map(|f| (*track, f)));
        }
        Action::Batch(actions) => {
            for action in actions {
                moved_by_hand(action, into);
            }
        }
        _ => {}
    }
}

/// A track with its faders where the steps have them, for those the steps drive and no hand has
/// taken over, so what's drawn from it shows what's heard. One with none is the track itself.
pub(crate) fn as_heard<'a>(track: &'a Track, faders: &Faders) -> Cow<'a, Track> {
    steps_onto(Cow::Borrowed(track), faders)
}

/// A track as the track page and the mixer draw it: each setting a macro holds where the macro
/// puts it, then its faders where the steps have them while they drive them. One with neither is
/// the track itself, so only a track that needs one is copied, once.
pub(crate) fn as_shown<'a>(track: &'a Track, faders: &Faders) -> Cow<'a, Track> {
    steps_onto(track.with_macros(), faders)
}

/// A track with the faders the steps drive put where they have them.
fn steps_onto<'a>(mut track: Cow<'a, Track>, faders: &Faders) -> Cow<'a, Track> {
    if !(0..FADER_COUNT).any(|fader| faders.follows_steps(fader)) {
        return track;
    }
    let heard = track.to_mut();
    for (fader, id) in FADERS.into_iter().enumerate() {
        if !faders.follows_steps(fader) {
            continue;
        }
        let [coarse, fine] = faders.at[fader].to_be_bytes();
        match id {
            SettingId::LEVEL => heard.level = coarse,
            SettingId::PAN => heard.pan = coarse,
            SettingId::SEND_A => heard.send_a = coarse,
            _ => heard.send_b = coarse,
        }
        heard.set_fine(id, fine);
    }
    track
}

impl Services {
    /// The engine's report of a track's faders, with what's been asked of it since on top.
    pub fn faders(&self, track: usize) -> Faders {
        self.reported.faders.get(track).copied().unwrap_or_default()
    }

    /// A track as it's heard, its faders where the steps have them while they drive them.
    pub(crate) fn track_as_heard(&self, track: usize) -> Option<Cow<'_, Track>> {
        let found = self.song().tracks.get(track)?;
        Some(as_heard(found, &self.faders(track)))
    }

    /// A track as the pages draw it: where its macros put what they hold, then its faders where
    /// the steps have them while they drive them.
    pub(crate) fn track_shown(&self, track: usize) -> Option<Cow<'_, Track>> {
        let found = self.song().tracks.get(track)?;
        Some(as_shown(found, &self.faders(track)))
    }

    /// Whether a control on a track is a fader the steps drive now, so it shows their value.
    pub fn follows_steps(&self, track: usize, control: Control) -> bool {
        fader_of(control).is_some_and(|fader| self.faders(track).follows_steps(fader))
    }

    /// Whether a control on a track is a fader a hand has taken over from the steps.
    pub fn taken_over(&self, track: usize, control: Control) -> bool {
        fader_of(control).is_some_and(|fader| self.faders(track).taken[fader])
    }

    /// Take the engine's report of every track's faders, keeping what's been asked since that it
    /// doesn't say yet.
    pub(crate) fn follow_faders(&mut self, report: &[Faders], now: Instant) {
        for (kept, reported) in self.reported.faders.iter_mut().zip(report) {
            *kept = *reported;
        }
        let faders = &mut self.reported.faders;
        self.fader_wishes.retain(|wish| {
            let Some(faders) = faders.get_mut(wish.track) else {
                return false;
            };
            if faders.taken[wish.fader] == wish.taken
                || now.saturating_duration_since(wish.at) > WISH_KEPT
            {
                return false;
            }
            faders.taken[wish.fader] = wish.taken;
            true
        });
    }

    /// Before an edit: a hand moving a fader the steps drive takes it over from them, from now
    /// until it's given back, so the edit is what's heard.
    pub(crate) fn take_over_moved(&mut self, action: &Action) {
        let mut moved = Vec::new();
        moved_by_hand(action, &mut moved);
        for (track, fader) in moved {
            if !self.faders(track).follows_steps(fader) {
                continue;
            }
            if self.handle.send(Command::TakeOver { track, fader }) {
                self.wish(track, fader, true);
            }
        }
    }

    pub(crate) fn wish(&mut self, track: usize, fader: usize, taken: bool) {
        if let Some(faders) = self.reported.faders.get_mut(track) {
            faders.taken[fader] = taken;
        }
        self.fader_wishes
            .retain(|wish| (wish.track, wish.fader) != (track, fader));
        self.fader_wishes.push(Wish {
            track,
            fader,
            taken,
            at: self.now,
        });
    }
}

impl App {
    /// `Space e` and `Space E`: give the cursor's track's faders, or every track's, back to the
    /// steps, as Live's Re-Enable Automation does.
    pub(crate) fn give_back_faders(&mut self, every: bool) {
        let tracks = self.song().tracks.len();
        let which: Vec<usize> = if every {
            (0..tracks).collect()
        } else {
            vec![self.services.cursor.track]
        };
        let taken: Vec<(usize, usize)> = which
            .iter()
            .flat_map(|track| (0..FADER_COUNT).map(move |fader| (*track, fader)))
            .filter(|(track, fader)| self.services.faders(*track).taken[*fader])
            .collect();
        let name = if every {
            "Every track".to_owned()
        } else {
            let track = self.services.cursor.track;
            self.song().tracks[track].title(track)
        };
        if taken.is_empty() {
            self.services.status = format!("{name} has no fader taken over from the steps");
            return;
        }
        let track = (!every).then_some(self.services.cursor.track);
        if !self.services.handle.send(Command::GiveBack { track }) {
            return;
        }
        for (track, fader) in taken {
            self.services.wish(track, fader, false);
        }
        self.services.status = if every {
            "Every fader follows the steps again".to_owned()
        } else {
            format!("{name}'s faders follow the steps again")
        };
    }

    /// Opening a song gives every fader back to its steps, as a takeover belongs to playing the
    /// song it was made in.
    pub(crate) fn give_back_on_opening(&mut self) {
        self.services.handle.send(Command::GiveBack { track: None });
        self.services.fader_wishes.clear();
        self.services.reported.faders = [Faders::default(); vt_model::constants::MAX_TRACKS];
    }

    /// The status line's word on a fader's value, after it in real units: from the steps, or
    /// taken over from them, with how to give it back.
    pub(crate) fn fader_note(&self, track: usize, control: Control) -> Option<&'static str> {
        if self.services.taken_over(track, control) {
            Some(", taken over from the steps: Space e gives it back")
        } else if self.services.follows_steps(track, control) {
            Some(", from the steps")
        } else {
            None
        }
    }
}
