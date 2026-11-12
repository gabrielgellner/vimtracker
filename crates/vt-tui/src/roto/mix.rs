//! The song as the Roto's MIX mode shows it (`docs/designs/roto-control.md` §4.4): the sound
//! tracks then the groups in one list, the returns then the master in the other, each a
//! [`Strip`] with its name, its group's colour, its faders' positions and its switches, and each
//! fader the same setting the track page and the mixer edit, so a knob's move is the edit its key
//! would make (`roto/edits.rs`).

use vt_model::action::{Action, TrackControl};
use vt_model::constants::{MAX_GROUPS, MAX_LEVEL, UNITY_LEVEL};
use vt_model::song::{Song, Track, TrackKind};
use vt_roto::Label;
use vt_roto::live::KNOB_TOP;
use vt_roto::mix::{Fader, Place, Strip};
use vt_roto::sysex::AllTracks;

use super::{devices, edits};
use crate::app::TrackLive;
use crate::automating::as_heard;
use crate::meter::{Meter, height_of};
use crate::track_page::Control;
use vt_engine::track_state::Faders;

/// The sends, in order: what each send's knob moves.
pub const SENDS: [TrackControl; 2] = [TrackControl::SendA, TrackControl::SendB];

/// Where the Roto's meters turn yellow and red, in dB: where the app's own meters turn yellow
/// and peach (`theme::level`).
pub const METER_YELLOW_DB: f32 = -6.0;
pub const METER_RED_DB: f32 = 0.0;

/// The highest a Roto meter goes.
const METER_TOP: f32 = 127.0;

/// Where the notch of MIX mode's Level knob is, of its 0 to 16383: the firmware's own, where
/// Live's faders have 0 dB, measured on the author's Roto (firmware 3.3.0), where a knob let go in
/// it read 13004 every time. vimtracker's 0 dB, `E0`, is put there, so the notch is 0 dB here too.
pub const LEVEL_NOTCH: u16 = 13004;

/// A level, `00` to `F8` with its fine digits as a fraction, as MIX mode's Level knob has it:
/// `00` at the bottom, 0 dB in the notch and +6 dB at the top, evenly in dB between.
pub fn level_position(level: f64) -> u16 {
    let unity = f64::from(UNITY_LEVEL);
    let top = f64::from(MAX_LEVEL);
    let notch = f64::from(LEVEL_NOTCH);
    let position = if level <= unity {
        level / unity * notch
    } else {
        notch + (level - unity) / (top - unity) * (f64::from(KNOB_TOP) - notch)
    };
    position.round().clamp(0.0, f64::from(KNOB_TOP)) as u16
}

/// The level MIX mode's Level knob is at, the reverse of [`level_position`], to the nearest of
/// the level's steps.
pub fn level_at(position: u16) -> u8 {
    let unity = f64::from(UNITY_LEVEL);
    let top = f64::from(MAX_LEVEL);
    let notch = f64::from(LEVEL_NOTCH);
    let position = f64::from(position.min(KNOB_TOP));
    let level = if position <= notch {
        position / notch * unity
    } else {
        unity + (position - notch) / (f64::from(KNOB_TOP) - notch) * (top - unity)
    };
    level.round().clamp(0.0, top) as u8
}

/// A track's level as MIX mode's Level knob has it: whole steps, as the level scale has no fine
/// part (decision 175).
fn track_level(track: &Track) -> f64 {
    f64::from(track.level)
}

/// The edit a MIX knob turned to a position makes, as [`edits::turned`] does, with the Level
/// knob on its own curve.
pub fn turned(control: Control, at: usize, song: &Song, position: u16) -> Option<Action> {
    if control != Control::Level {
        return edits::turned(control, at, song, position);
    }
    let track = song.tracks.get(at)?;
    // A knob left where its level puts it changes nothing, fine digits and all.
    if level_position(track_level(track)) == position {
        return None;
    }
    let level = level_at(position);
    (level != track.level).then_some(Action::SetTrackLevel { track: at, level })
}

/// The setting a fader is on a track.
pub fn control_of(fader: Fader) -> Option<Control> {
    match fader {
        Fader::Level => Some(Control::Level),
        Fader::Pan => Some(Control::Base(TrackControl::Pan)),
        Fader::Send(send) => SENDS
            .get(usize::from(send))
            .map(|send| Control::Base(*send)),
    }
}

/// How many tracks come before the returns: the sound tracks and the groups.
fn tracks_before_returns(song: &Song) -> usize {
    song.sound_tracks() + MAX_GROUPS
}

/// The song's track at a place in MIX mode's lists.
pub fn track_of(song: &Song, (list, index): Place) -> Option<usize> {
    let at = match list {
        AllTracks::Audio => usize::from(index),
        AllTracks::MasterReturn => tracks_before_returns(song) + usize::from(index),
    };
    let in_list = match list {
        AllTracks::Audio => at < tracks_before_returns(song),
        AllTracks::MasterReturn => at < song.tracks.len(),
    };
    in_list.then_some(at)
}

/// Where a song's track is in MIX mode's lists.
pub fn place_of(song: &Song, track: usize) -> Place {
    let returns = tracks_before_returns(song);
    if track < returns {
        (AllTracks::Audio, track as u16)
    } else {
        (AllTracks::MasterReturn, (track - returns) as u16)
    }
}

/// The sends' names, as the returns they go to are named.
pub fn send_names() -> Vec<Label> {
    [TrackKind::ReturnA, TrackKind::ReturnB]
        .iter()
        .map(|kind| Label::new(&kind.title()))
        .collect()
}

/// Where a fader is on a track, if the track has it.
fn position(control: Control, track: &Track, song: &Song) -> Option<u16> {
    control
        .exists_on(track)
        .then(|| devices::knob(control, track, song).map(|knob| knob.position))
        .flatten()
}

/// Every track as MIX mode shows it, in its two lists, with the mutes and solos the keys and the
/// Roto set while playing, and each fader where the steps have it while they drive it, so its
/// motor follows them (decision 174).
pub fn strips(song: &Song, levels: &[TrackLive], faders: &[Faders]) -> [Vec<Strip>; 2] {
    let live = |at: usize| levels.get(at).copied().unwrap_or_default();
    let silenced = |at: usize| (live(at).mute, live(at).solo);
    let mut lists = [Vec::new(), Vec::new()];
    for ((at, track), shown) in song
        .tracks
        .iter()
        .enumerate()
        .zip(devices::live_tracks(song))
    {
        let can_silence = Control::Mute.exists_on(track);
        let track = as_heard(track, &faders.get(at).copied().unwrap_or_default());
        let track = track.as_ref();
        let strip = Strip {
            name: shown.name,
            colour: shown.colour,
            grouped: shown.grouped,
            level: Some(level_position(track_level(track))),
            pan: position(Control::Base(TrackControl::Pan), track, song),
            sends: SENDS
                .iter()
                .map(|send| position(Control::Base(*send), track, song))
                .collect(),
            mute: can_silence.then_some(live(at).mute),
            solo: can_silence.then_some(live(at).solo),
            heard: song.heard_by(at, silenced),
        };
        let list = usize::from(at >= tracks_before_returns(song));
        lists[list].push(strip);
    }
    lists
}

/// A meter's place on the Roto's, from 0 to 127, by the scale the app's meters are drawn on.
pub fn meter_level(db: f32) -> u8 {
    (height_of(db) * METER_TOP).round().clamp(0.0, METER_TOP) as u8
}

/// A track's meter for the Roto, left and right: each channel's peak as it falls, as the
/// mixer page draws it.
pub fn meter_levels(meter: &Meter) -> [u8; 2] {
    meter.channels.map(|channel| meter_level(channel.peak.now))
}

/// Where a fader's knob goes when the Roto asks for its default: where a new track has it, with
/// no fine digits.
pub fn default_position(control: Control, track: &Track, song: &Song) -> Option<u16> {
    let new = Track::default();
    let mut track = track.clone();
    match control {
        Control::Level => return Some(level_position(f64::from(new.level))),
        Control::Base(TrackControl::Pan) => track.pan = new.pan,
        Control::Base(TrackControl::SendA) => track.send_a = new.send_a,
        Control::Base(TrackControl::SendB) => track.send_b = new.send_b,
        _ => return None,
    }
    track.fine.clear();
    position(control, &track, song)
}
