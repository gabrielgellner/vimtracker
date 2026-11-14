//! What a track is for, as a thin rule under its name in the role's colour wherever the track is
//! named (decision 316): the song view's columns, the mixer's strips, the phrase view's labels and
//! titles, the track list and the track page's title. It's an underline, as the phrase grid's beat
//! rules are, so it costs no cell and leaves the name's own colour, a group's, as it was.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use vt_model::song::{Song, TrackKind};

use crate::theme;

/// The colour of the rule under a track's name: a sound track's role's; a group's, the role its
/// tracks all share, so a folded group of drums still reads as drums, or the bus's while they're
/// for different things; a return's and the master's the bus's. A sound track with no role, and a
/// group with no tracks, have none.
pub fn colour(song: &Song, track: usize) -> Option<Color> {
    let this = song.tracks.get(track)?;
    match this.kind {
        TrackKind::Sound => theme::role(this.role()),
        TrackKind::Group(_) => {
            let mut members = song.group_members(track).map(|at| song.tracks[at].role());
            let first = members.next()?;
            if members.all(|role| role == first) {
                theme::role(first).or(Some(theme::BUS))
            } else {
                Some(theme::BUS)
            }
        }
        TrackKind::ReturnA | TrackKind::ReturnB | TrackKind::Master => Some(theme::BUS),
    }
}

/// The style that draws the rule, to patch over a name's, or none.
pub fn rule(song: &Song, track: usize) -> Style {
    colour(song, track).map_or_else(Style::new, |colour| {
        Style::new()
            .add_modifier(Modifier::UNDERLINED)
            .underline_color(colour)
    })
}

/// A name in a style, with the track's rule under it: under its text, not the spaces either side,
/// so the rule is as long as the name.
pub fn ruled(song: &Song, track: usize, name: &str, style: Style) -> Vec<Span<'static>> {
    let body = name.trim();
    let before = name.len() - name.trim_start().len();
    let after = name.len() - before - body.len();
    let mut spans = Vec::new();
    if before > 0 {
        spans.push(Span::styled(name[..before].to_owned(), style));
    }
    spans.push(Span::styled(
        body.to_owned(),
        style.patch(rule(song, track)),
    ));
    if after > 0 {
        spans.push(Span::styled(name[before + body.len()..].to_owned(), style));
    }
    spans
}

#[cfg(test)]
mod tests {
    use vt_model::song::{Machine, Role, SynthVoice};

    use super::*;

    #[test]
    fn a_track_is_ruled_by_its_role_and_a_group_by_what_its_tracks_share() {
        let mut song = Song::new(8);
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Kick);
        song.tracks[1].machine = Machine::Plaits(SynthVoice::Acid);
        song.tracks[2].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
        song.tracks[2].role = Role::None;
        assert_eq!(colour(&song, 0), Some(theme::MAROON));
        assert_eq!(colour(&song, 1), Some(theme::YELLOW));
        assert_eq!(colour(&song, 2), None, "told it's for nothing");
        assert_eq!(colour(&song, 4), None, "a sampler could be anything");
        song.tracks[4].role = Role::Pad;
        assert_eq!(colour(&song, 4), Some(theme::LAVENDER));

        let group = song.fixed_track(TrackKind::Group(1)).unwrap();
        assert_eq!(colour(&song, group), None, "an empty group has none");
        song.tracks[0].group = Some(1);
        song.tracks[3].group = Some(1);
        song.tracks[3].role = Role::Drums;
        assert_eq!(colour(&song, group), Some(theme::MAROON));
        song.tracks[1].group = Some(1);
        assert_eq!(colour(&song, group), Some(theme::BUS));

        let master = song.fixed_track(TrackKind::Master).unwrap();
        assert_eq!(colour(&song, master), Some(theme::BUS));
    }

    #[test]
    fn the_rule_is_as_long_as_the_name() {
        let mut song = Song::new(8);
        song.tracks[0].role = Role::Drums;
        let spans = ruled(&song, 0, " BD  ", Style::new());
        let text: Vec<&str> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, [" ", "BD", "  "]);
        assert!(spans[1].style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(!spans[2].style.add_modifier.contains(Modifier::UNDERLINED));
    }
}
