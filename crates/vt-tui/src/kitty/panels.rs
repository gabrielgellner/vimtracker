//! The pictures of the graphic panels, as data: what a page wants on screen ([`PanelWant`]), the
//! ids it's given back for them ([`PanelIds`]), and the images the terminal has
//! ([`PanelImages`]): each panel's picture in each of its shades, with what it was drawn from, so
//! moving the cursor or switching a device out and back sends nothing once each has been sent,
//! and a picture is sent again only when what shapes it changes.

use std::collections::BTreeMap;

use vt_model::envelope::Envelope;
use vt_model::modulation::Polarity;
use vt_model::song::{FilterMode, FilterType};

use super::generators::GenDrawing;
use super::plot::{self, LfoDrawing, MappingDrawing, Shade, TrackDrawing};
use super::rgb;
use super::slots::SlotDrawing;
use crate::sample_wave::Thumbnail;
use crate::theme;

/// The settings a filter curve is drawn from: its type, the mode it plays, `Cut` and `Res` as
/// they're playing, its drive, `Safe`, and the utility filter's low and high cuts.
pub(crate) type FilterSettings = (FilterType, FilterMode, u8, u8, u8, bool, u8, u8);

/// A panel that draws an image: the Sound page's filter curve and sampler's thumbnail, a mod
/// slot's picture on the Mod page, an envelope's shape, an LFO's cycle or a Track slot's map, an
/// FX slot's curve, or the zoomed one's, a live generator's preview, or the mapping view's
/// response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelKey {
    Filter,
    Waveform,
    Mod(usize),
    Slot(usize),
    /// The FX page's zoomed slot, whichever it is (decision 329).
    Zoom,
    /// A live generator's preview on the Gen page, by its place in its phrase's list.
    Gen(usize),
    /// The response of the mapping under the mapping view's cursor (decision 314).
    Mapping,
}

/// What a panel's picture was drawn from, which is all that's compared to tell if it's changed.
#[derive(Debug, Clone)]
pub(crate) enum Shape {
    Filter(FilterSettings),
    /// A sampler's slice, with where it starts and stops, drawn again only when one of those
    /// changes or the file's frames come in.
    Waveform(Thumbnail),
    /// An envelope's settings and its polarity.
    Envelope(Envelope, Polarity),
    /// An LFO's wave, where its cycle starts and its depth's sign, drawn again only when one of
    /// those changes, never for its rate or the size of its depth.
    Lfo(LfoDrawing),
    /// A Track slot's map, drawn again only when its source, its map or its depth's sign or
    /// polarity changes.
    Track(TrackDrawing),
    Slot(SlotDrawing),
    /// What a live generator writes across its span.
    Gen(GenDrawing),
    /// A macro mapping's response.
    Mapping(MappingDrawing),
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Filter(a), Self::Filter(b)) => a == b,
            (Self::Waveform(a), Self::Waveform(b)) => a == b,
            (Self::Envelope(a, one), Self::Envelope(b, other)) => a == b && one == other,
            (Self::Lfo(a), Self::Lfo(b)) => a == b,
            (Self::Track(a), Self::Track(b)) => a == b,
            (Self::Slot(a), Self::Slot(b)) => a == b,
            (Self::Gen(a), Self::Gen(b)) => a == b,
            (Self::Mapping(a), Self::Mapping(b)) => a == b,
            _ => false,
        }
    }
}

impl Shape {
    /// Its picture in a shade, as a PNG, for a panel this many characters across and down at
    /// characters of `cell` pixels, at the program's sample rate.
    pub fn png(
        &self,
        cells: (usize, usize),
        cell: (usize, usize),
        shade: Shade,
        rate: u32,
    ) -> Vec<u8> {
        let size = (cells.0 * cell.0, cells.1 * cell.1);
        let color = shade.color();
        match self {
            Self::Filter(settings) => plot::filter_from(*settings, rate, size).png(color),
            Self::Waveform(thumbnail) => {
                let (sound, outside) = thumbnail_colors(shade);
                thumbnail.picture(cells, cell, sound, outside).png()
            }
            Self::Envelope(envelope, polarity) => {
                plot::envelope_in(envelope, *polarity, size).png(color)
            }
            Self::Lfo(drawing) => plot::lfo_cycle(*drawing, size).png(color),
            Self::Track(drawing) => plot::track_map(*drawing, size).png(color),
            Self::Slot(drawing) => drawing.plot(rate, size).png(color),
            Self::Gen(drawing) => drawing.plot(size).png(color),
            Self::Mapping(drawing) => plot::mapping(*drawing, size).png(color),
        }
    }
}

/// A thumbnail's colours in a shade: the sound that plays and the rest. With the cursor in the
/// Machine, what plays is the text's colour, as on the sample page, rather than the active
/// green, which would be hard to tell from the start's teal.
fn thumbnail_colors(shade: Shade) -> ([u8; 3], [u8; 3]) {
    match shade {
        Shade::Focused => (rgb(theme::TEXT), rgb(theme::OVERLAY0)),
        Shade::Dim => (shade.color(), rgb(theme::SURFACE1)),
        Shade::Faint => (shade.color(), rgb(theme::SURFACE0)),
    }
}

/// A picture a page wants on screen this frame: which panel it's for, in which shade, what it's
/// drawn from, which is compared to tell if it's new, and its size in characters.
#[derive(Debug, Clone)]
pub(crate) struct PanelWant {
    pub key: PanelKey,
    pub shade: Shade,
    pub shape: Shape,
    pub cells: (usize, usize),
}

/// The ids of a page's pictures, by panel, for its drawing to put placeholders for. A panel with
/// none has nothing drawn yet, and its box is left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelIds(BTreeMap<PanelKey, u32>);

impl PanelIds {
    /// The id of a panel's picture, if it has one.
    pub fn get(&self, key: PanelKey) -> Option<u32> {
        self.0.get(&key).copied()
    }

    /// Give a panel's picture an id, as [`Graphics::panels`](super::Graphics::panels) does, or a
    /// test that draws a page with ids of its own.
    pub fn insert(&mut self, key: PanelKey, id: u32) {
        self.0.insert(key, id);
    }
}

impl FromIterator<(PanelKey, u32)> for PanelIds {
    fn from_iter<T: IntoIterator<Item = (PanelKey, u32)>>(ids: T) -> Self {
        Self(ids.into_iter().collect())
    }
}

/// One image the terminal has: what it shows, its id, and the frame it was sent in if it was sent
/// for a change, in place of another.
#[derive(Debug, Clone)]
struct Held {
    shape: Shape,
    id: u32,
    changed: Option<usize>,
}

/// What [`PanelImages::find`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// The image shows what's asked for.
    Current(u32),
    /// It shows something older, but it was sent too lately to send another yet.
    Waiting(u32),
    /// There's none, or it's old and another can be sent.
    Missing,
}

/// Every panel's images, in each of their shades.
#[derive(Debug, Default)]
pub struct PanelImages {
    held: BTreeMap<(PanelKey, Shade), Held>,
}

impl PanelImages {
    /// The image a panel has in a shade, if it shows `shape`. One that shows something else, and
    /// was itself sent for a change fewer than `every` frames before `frame`, is kept a little
    /// longer, so a picture that follows a modulated setting is sent at most once in that many
    /// frames, while one change after a rest is sent at once.
    pub fn find(
        &self,
        key: PanelKey,
        shade: Shade,
        shape: &Shape,
        frame: usize,
        every: usize,
    ) -> Found {
        match self.held.get(&(key, shade)) {
            Some(held) if held.shape == *shape => Found::Current(held.id),
            Some(Held {
                id,
                changed: Some(sent),
                ..
            }) if frame.wrapping_sub(*sent) < every => Found::Waiting(*id),
            _ => Found::Missing,
        }
    }

    /// Hold a panel's new image in a shade, sent in `frame`, giving back the id of the one it
    /// replaces.
    pub fn hold(
        &mut self,
        key: PanelKey,
        shade: Shade,
        shape: Shape,
        id: u32,
        frame: usize,
    ) -> Option<u32> {
        let replaced = self.held.get(&(key, shade)).map(|old| old.id);
        let held = Held {
            shape,
            id,
            changed: replaced.map(|_| frame),
        };
        self.held.insert((key, shade), held);
        replaced
    }

    /// Every image's id.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.held.values().map(|held| held.id)
    }

    /// Let go of every image, as after the screen is cleared or the characters change size, giving
    /// back their ids to forget, so each panel is drawn and sent again.
    pub fn clear(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.held)
            .into_values()
            .map(|held| held.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::envelope::Adsr;

    #[test]
    fn an_image_is_found_until_its_shape_changes_and_waits_while_it_just_changed() {
        let mut images = PanelImages::default();
        let shape = Shape::Envelope(Envelope::Adsr(Adsr::default()), Polarity::Uni);
        let key = PanelKey::Mod(0);
        assert_eq!(images.find(key, Shade::Dim, &shape, 0, 2), Found::Missing);
        assert_eq!(images.hold(key, Shade::Dim, shape.clone(), 5, 0), None);
        assert_eq!(
            images.find(key, Shade::Dim, &shape, 1, 2),
            Found::Current(5)
        );
        // Each shade is its own.
        assert_eq!(images.find(key, Shade::Faint, &shape, 1, 2), Found::Missing);
        let changed = Shape::Envelope(
            Envelope::Adsr(Adsr {
                attack_ms: 9,
                ..Adsr::default()
            }),
            Polarity::Uni,
        );
        // The first change is drawn at once.
        assert_eq!(images.find(key, Shade::Dim, &changed, 1, 2), Found::Missing);
        assert_eq!(images.hold(key, Shade::Dim, changed, 6, 1), Some(5));
        // Another the next frame waits for the one after.
        let again = Shape::Envelope(
            Envelope::Adsr(Adsr {
                attack_ms: 10,
                ..Adsr::default()
            }),
            Polarity::Uni,
        );
        assert_eq!(
            images.find(key, Shade::Dim, &again, 2, 2),
            Found::Waiting(6)
        );
        assert_eq!(images.find(key, Shade::Dim, &again, 3, 2), Found::Missing);
        assert_eq!(images.ids().collect::<Vec<_>>(), [6]);
        assert_eq!(images.clear(), [6]);
        assert_eq!(images.ids().count(), 0);
    }
}
