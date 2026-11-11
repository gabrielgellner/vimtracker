//! The images of the track page's graphic panels the terminal has: each panel's picture in each
//! of its shades, with what it was drawn from, so moving the cursor or switching a device out and
//! back sends nothing once each has been sent, and a picture is sent again only when what shapes
//! it changes.

use std::collections::BTreeMap;

use vt_model::envelope::Adsr;

use super::plot::{LfoDrawing, Shade};
use super::{FilterSettings, SlotDrawing};
use crate::sample_wave::Thumbnail;

/// A panel that draws an image: the Sound page's filter curve and sampler's thumbnail, a mod
/// slot's picture on the Mod page, an envelope's shape or an LFO's cycle, or an FX slot's curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelKey {
    Filter,
    Waveform,
    Mod(usize),
    Slot(usize),
}

/// What a panel's picture was drawn from, which is all that's compared to tell if it's changed.
#[derive(Debug, Clone)]
pub enum Shape {
    Filter(FilterSettings),
    /// A sampler's slice, with where it starts and stops, drawn again only when one of those
    /// changes or the file's frames come in.
    Waveform(Thumbnail),
    Envelope(Adsr),
    /// An LFO's wave, where its cycle starts and its depth's sign, drawn again only when one of
    /// those changes, never for its rate or the size of its depth.
    Lfo(LfoDrawing),
    Slot(SlotDrawing),
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Filter(a), Self::Filter(b)) => a == b,
            (Self::Waveform(a), Self::Waveform(b)) => a == b,
            (Self::Envelope(a), Self::Envelope(b)) => a == b,
            (Self::Lfo(a), Self::Lfo(b)) => a == b,
            (Self::Slot(a), Self::Slot(b)) => a == b,
            _ => false,
        }
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

    #[test]
    fn an_image_is_found_until_its_shape_changes_and_waits_while_it_just_changed() {
        let mut images = PanelImages::default();
        let shape = Shape::Envelope(Adsr::default());
        let key = PanelKey::Mod(0);
        assert_eq!(images.find(key, Shade::Dim, &shape, 0, 2), Found::Missing);
        assert_eq!(images.hold(key, Shade::Dim, shape.clone(), 5, 0), None);
        assert_eq!(
            images.find(key, Shade::Dim, &shape, 1, 2),
            Found::Current(5)
        );
        // Each shade is its own.
        assert_eq!(images.find(key, Shade::Faint, &shape, 1, 2), Found::Missing);
        let changed = Shape::Envelope(Adsr {
            attack_ms: 9,
            ..Adsr::default()
        });
        // The first change is drawn at once.
        assert_eq!(images.find(key, Shade::Dim, &changed, 1, 2), Found::Missing);
        assert_eq!(images.hold(key, Shade::Dim, changed, 6, 1), Some(5));
        // Another the next frame waits for the one after.
        let again = Shape::Envelope(Adsr {
            attack_ms: 10,
            ..Adsr::default()
        });
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
