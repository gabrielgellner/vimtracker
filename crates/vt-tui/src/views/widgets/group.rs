//! How a group shows (decision 126): its colour, which it shares with the mark on each of its
//! tracks, the tracks it sums, and a mute or solo a track has from its group rather than its own.

use ratatui::style::Style;
use ratatui::text::Span;
use vt_model::song::{Song, TrackKind};

use crate::app::App;
use crate::theme;

/// The number of the group a track is, or plays through.
pub fn number(song: &Song, track: usize) -> Option<u8> {
    let group = match song.tracks.get(track)?.kind {
        TrackKind::Group(number) => return Some(number),
        TrackKind::Sound => song.group_track_of(track)?,
        _ => return None,
    };
    match song.tracks[group].kind {
        TrackKind::Group(number) => Some(number),
        _ => None,
    }
}

/// The colour of the group a track is, or plays through.
pub fn style(song: &Song, track: usize) -> Option<Style> {
    number(song, track).map(|number| Style::new().fg(theme::group(number)))
}

/// The mark of a sound track in a group, `G1` in the group's colour, as under its strip on the
/// mixer and after its name in a view's title (decision 128).
pub fn mark(song: &Song, track: usize) -> Option<Span<'static>> {
    let number = number(song, track).filter(|_| song.tracks[track].is_sound())?;
    Some(Span::styled(format!("G{number}"), style(song, track)?))
}

/// Whether a track is a group with no tracks in it, which is shown dim.
pub fn is_empty(song: &Song, track: usize) -> bool {
    song.tracks.get(track).is_some_and(|this| this.is_group())
        && song.group_members(track).next().is_none()
}

/// The tracks a group sums, numbered as the screen numbers them, with a run of three or more as its
/// first and last: `01-04 07 09`.
pub fn runs(members: &[usize]) -> String {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for &track in members {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == track => *last = track,
            _ => runs.push((track, track)),
        }
    }
    let mut words = Vec::new();
    for (first, last) in runs {
        match last - first {
            0 => words.push(format!("{:02}", first + 1)),
            1 => words.extend([format!("{:02}", first + 1), format!("{:02}", last + 1)]),
            _ => words.push(format!("{:02}-{:02}", first + 1, last + 1)),
        }
    }
    words.join(" ")
}

/// Why a track's mute or solo is on: its own, or its group's, by the group's number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switched {
    Off,
    Own,
    Group(u8),
}

impl Switched {
    pub fn is_on(self) -> bool {
        self != Self::Off
    }

    /// Its glyph's style: the active colour for the track's own, and the group's colour for its
    /// group's, so the two read apart and the second says which group it's from.
    pub fn style(self) -> Style {
        match self {
            Self::Group(number) => Style::new().fg(theme::group(number)),
            _ => theme::active(),
        }
    }
}

/// Whether a track's own switch is on, or its group's, as each was last set live.
fn switched(app: &App, track: usize, on: impl Fn(usize) -> bool) -> Switched {
    if on(track) {
        return Switched::Own;
    }
    let song = app.song();
    match (song.group_track_of(track), number(song, track)) {
        (Some(group), Some(number)) if on(group) => Switched::Group(number),
        _ => Switched::Off,
    }
}

/// Whether a track is muted, by itself or by its group.
pub fn muted(app: &App, track: usize) -> Switched {
    switched(app, track, |index| {
        app.levels.get(index).is_some_and(|live| live.mute)
    })
}

/// Whether a track is soloed, by itself or by its group.
pub fn soloed(app: &App, track: usize) -> Switched {
    switched(app, track, |index| {
        app.levels.get(index).is_some_and(|live| live.solo)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_of_three_or_more_are_their_ends() {
        assert_eq!(runs(&[]), "");
        assert_eq!(runs(&[0]), "01");
        assert_eq!(runs(&[0, 1]), "01 02");
        assert_eq!(runs(&[0, 1, 2, 3, 6, 8, 9]), "01-04 07 09 10");
    }
}
