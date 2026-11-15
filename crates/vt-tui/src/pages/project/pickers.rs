//! The lists of the project's grooves and scales, which `:groove` and `:scale` open, and `K` on
//! the Song sub-page's Groove row and the track page's Groove. Each is built from the
//! [`Services`] and given back to be shown.

use vt_model::groove::GrooveId;
use vt_model::scale::ScaleKind;

use crate::overlays::command::parse::Line;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::services::Services;

impl Services {
    /// The list of grooves for the project, or for the cursor's track, which can also follow the
    /// project's, with the one playing highlighted.
    pub(crate) fn groove_picker(&self, track: bool) -> Picker {
        let song = self.song();
        let own = song.tracks.get(self.cursor.track).and_then(|t| t.groove);
        let mut items = Vec::new();
        if track {
            let follow = format!("Project's, {}", song.groove);
            items.push(
                Item::new(follow, Pick::Line(Line::TrackGroove(None)))
                    .meaning("The groove the project plays with"),
            );
        }
        for id in GrooveId::all() {
            let line = if track {
                Line::TrackGroove(Some(id))
            } else {
                Line::Groove(id)
            };
            items.push(Item::new(id.name(), Pick::Line(line)).meaning(id.meaning()));
        }
        let (title, at) = if track {
            let at = own.map_or(0, |id| id.index() + 1);
            ("This track's groove", at)
        } else {
            ("The project's groove", song.groove.index())
        };
        Picker::new(title, items).opened_on(Some(at))
    }

    /// The list of scales, with this one highlighted.
    pub(crate) fn scale_picker(&self) -> Picker {
        let items = ScaleKind::ALL
            .map(|kind| Item::new(kind.name(), Pick::Scale(kind)))
            .to_vec();
        let at = ScaleKind::ALL
            .iter()
            .position(|kind| *kind == self.live.scale.kind);
        Picker::new("Scales", items).opened_on(at)
    }
}
