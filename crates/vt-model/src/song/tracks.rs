//! A song's tracks, each shared with the versions an edit didn't change it in.

use std::ops::{
    Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::Track;

/// A song's tracks in order, the sound tracks and then the fixed ones.
///
/// Each is behind an `Arc`, as a track's phrases and chains are, so a copy of the song shares
/// every track with the song it's a copy of, and two versions' tracks can be told apart by
/// pointer with [`same`](Self::same). Reading is as from a `Vec<Track>`. Anything that gives a
/// `&mut Track` (`tracks[index]` written to, `get_mut`, `iter_mut`, `for track in &mut tracks`)
/// first makes that track this song's own, copying it if another version shares it, whether or
/// not it's then changed: so ask for one only to change it, and never go through all of them
/// mutably to change a few.
///
/// It saves as the list of tracks it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tracks(Vec<Arc<Track>>);

/// Reading the tracks in order.
#[derive(Debug, Clone)]
pub struct Iter<'a>(std::slice::Iter<'a, Arc<Track>>);

/// Going through the tracks to change every one.
#[derive(Debug)]
pub struct IterMut<'a>(std::slice::IterMut<'a, Arc<Track>>);

fn read(track: &Arc<Track>) -> &Track {
    track
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Track;

    #[inline]
    fn next(&mut self) -> Option<&'a Track> {
        self.0.next().map(read)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<&'a Track> {
        self.0.nth(n).map(read)
    }

    #[inline]
    fn fold<B, F: FnMut(B, &'a Track) -> B>(self, init: B, mut f: F) -> B {
        self.0.fold(init, |folded, track| f(folded, track))
    }
}

impl DoubleEndedIterator for Iter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(read)
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl std::iter::FusedIterator for Iter<'_> {}

impl<'a> Iterator for IterMut<'a> {
    type Item = &'a mut Track;

    fn next(&mut self) -> Option<&'a mut Track> {
        self.0.next().map(Arc::make_mut)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    // Passing over tracks leaves them shared, so `skip` copies only those it then gives.
    fn nth(&mut self, n: usize) -> Option<&'a mut Track> {
        self.0.nth(n).map(Arc::make_mut)
    }
}

impl DoubleEndedIterator for IterMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(Arc::make_mut)
    }
}

impl ExactSizeIterator for IterMut<'_> {}

impl std::iter::FusedIterator for IterMut<'_> {}

impl Tracks {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Track> {
        self.0.get(index).map(read)
    }

    /// A track to change, made this song's own first.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Track> {
        self.0.get_mut(index).map(Arc::make_mut)
    }

    pub fn first(&self) -> Option<&Track> {
        self.0.first().map(read)
    }

    #[cfg(test)]
    pub(crate) fn last(&self) -> Option<&Track> {
        self.0.last().map(read)
    }

    /// The first track to change, made this song's own first.
    pub fn first_mut(&mut self) -> Option<&mut Track> {
        self.0.first_mut().map(Arc::make_mut)
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.0.iter())
    }

    /// Every track to change, each made this song's own first: for a change to all of them.
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut(self.0.iter_mut())
    }

    /// The tracks as they're shared, to compare or keep one by pointer.
    #[cfg(test)]
    pub(crate) fn shared(&self) -> &[Arc<Track>] {
        &self.0
    }

    /// Whether the track at a place here is the very one at a place in another version's tracks,
    /// so nothing of it can differ. Tracks that aren't may still be equal.
    pub fn same(&self, index: usize, other: &Tracks, other_index: usize) -> bool {
        match (self.0.get(index), other.0.get(other_index)) {
            (Some(this), Some(that)) => Arc::ptr_eq(this, that),
            _ => false,
        }
    }

    pub(crate) fn insert(&mut self, index: usize, track: Track) {
        self.0.insert(index, Arc::new(track));
    }

    /// Take a track out, as its own if no other version shares it and as a copy if one does.
    pub(crate) fn remove(&mut self, index: usize) -> Track {
        Arc::unwrap_or_clone(self.0.remove(index))
    }

    pub fn truncate(&mut self, length: usize) {
        self.0.truncate(length);
    }

    /// Take a range of tracks out.
    #[cfg(test)]
    pub(crate) fn drain(&mut self, range: impl std::ops::RangeBounds<usize>) {
        self.0.drain(range);
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        self.0.swap(a, b);
    }
}

impl Index<usize> for Tracks {
    type Output = Track;

    fn index(&self, index: usize) -> &Track {
        &self.0[index]
    }
}

impl IndexMut<usize> for Tracks {
    /// The track to change, made this song's own first.
    fn index_mut(&mut self, index: usize) -> &mut Track {
        Arc::make_mut(&mut self.0[index])
    }
}

/// A range of the tracks is read as they're shared, each an `Arc` that reads as its track.
macro_rules! index_range {
    ($($range:ty),*) => {$(
        impl Index<$range> for Tracks {
            type Output = [Arc<Track>];

            fn index(&self, range: $range) -> &[Arc<Track>] {
                &self.0[range]
            }
        }
    )*};
}

index_range!(
    Range<usize>,
    RangeFrom<usize>,
    RangeTo<usize>,
    RangeFull,
    RangeInclusive<usize>,
    RangeToInclusive<usize>
);

impl<'a> IntoIterator for &'a Tracks {
    type Item = &'a Track;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Tracks {
    type Item = &'a mut Track;
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

impl FromIterator<Track> for Tracks {
    fn from_iter<I: IntoIterator<Item = Track>>(tracks: I) -> Self {
        Self(tracks.into_iter().map(Arc::new).collect())
    }
}

impl From<Vec<Track>> for Tracks {
    fn from(tracks: Vec<Track>) -> Self {
        tracks.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(names: &[&str]) -> Tracks {
        let track = |name: &&str| Track {
            name: (*name).to_owned(),
            ..Track::default()
        };
        names.iter().map(track).collect()
    }

    #[test]
    fn a_copy_shares_every_track_until_one_is_written_to() {
        let tracks = named(&["a", "b", "c"]);
        let mut copy = tracks.clone();
        assert!((0..3).all(|at| copy.same(at, &tracks, at)));
        // Reading through a mutable copy copies nothing.
        assert_eq!(copy[1].name, "b");
        assert_eq!(
            copy.iter().map(|track| track.name.as_str()).next_back(),
            Some("c")
        );
        assert!((0..3).all(|at| copy.same(at, &tracks, at)));
        copy[1].name = "B".to_owned();
        assert_eq!(tracks[1].name, "b", "the one copied from is as it was");
        assert_eq!(copy[1].name, "B");
        let shared: Vec<bool> = (0..3).map(|at| copy.same(at, &tracks, at)).collect();
        assert_eq!(shared, [true, false, true]);
        // A track is the same only at the places asked, and none past the end is.
        assert!(!copy.same(0, &tracks, 2));
        assert!(!copy.same(3, &tracks, 3));
    }

    #[test]
    fn asking_to_change_a_shared_track_copies_it_changed_or_not() {
        let tracks = named(&["a", "b"]);
        let mut copy = tracks.clone();
        let _ = copy.get_mut(0);
        assert!(!copy.same(0, &tracks, 0));
        assert!(copy.same(1, &tracks, 1));
        assert_eq!(copy, tracks, "though they're still equal");
        for _ in &mut copy {}
        assert!(!copy.same(1, &tracks, 1));
        // One that's its song's own already stays where it is.
        let own = Arc::as_ptr(&copy.shared()[0]);
        copy[0].saved_mute = true;
        assert_eq!(Arc::as_ptr(&copy.shared()[0]), own);
    }

    #[test]
    fn tracks_come_and_go_in_place() {
        let mut tracks = named(&["a", "c"]);
        let before = tracks.clone();
        tracks.insert(1, named(&["b"]).remove(0));
        let names =
            |tracks: &Tracks| -> String { tracks.iter().map(|t| t.name.as_str()).collect() };
        assert_eq!(names(&tracks), "abc");
        assert!(
            tracks.same(2, &before, 1),
            "one moved along is still shared"
        );
        assert_eq!(tracks.remove(0).name, "a");
        assert_eq!(
            before[0].name, "a",
            "and one taken out is still in the other"
        );
        assert_eq!(names(&tracks), "bc");
        assert_eq!(tracks[1..].len(), 1);
        assert_eq!(tracks[1..][0].name, "c");
    }

    #[test]
    fn they_save_as_a_plain_list_of_tracks() {
        let tracks = named(&["a", "b"]);
        let plain: Vec<Track> = tracks.iter().cloned().collect();
        let text = ron::to_string(&tracks).unwrap();
        assert_eq!(text, ron::to_string(&plain).unwrap());
        assert_eq!(ron::from_str::<Tracks>(&text).unwrap(), tracks);
    }
}
