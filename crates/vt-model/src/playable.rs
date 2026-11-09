//! What the audio thread plays for one of the bank's entries: its sound at the program's rate and
//! the spans it can play, each with its points, loop, gain, tune, reverse and fades worked out.
//!
//! It's built from an entry's [`SampleEdit`](crate::bank::SampleEdit) on the edit thread, so the
//! engine never reads an edit: a new field there changes how a span is worked out here, and the
//! engine plays the span as it always has.

use std::sync::Arc;

use crate::bank::BankEntry;
use crate::sample::SampleData;
use crate::samples::{SampleId, SamplePool};

/// One entry, ready to play.
#[derive(Debug, Clone, PartialEq)]
pub struct Playable {
    /// Its place in the pool, which a voice keeps reading until its note ends.
    pub sample: SampleId,
    /// Its sound at the program's rate.
    pub frames: Arc<SampleData>,
    /// What it can play. For now there's one, the whole entry, and slices add theirs (#270).
    pub spans: Box<[Span]>,
}

/// A stretch of a sample and how it plays, in the pool's frames, at the program's rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// The whole sample's length, so a place counted back from its end is a frame.
    pub frames: u64,
    /// Its first frame, and one past its last, as the file has them, whichever way it plays.
    pub start: u64,
    pub end: u64,
    /// Its loop, if it has one of its own, which a note loops round when its `Play` says to.
    pub loop_: Option<LoopSpan>,
    /// Whether it plays from its end back to its start.
    pub reverse: bool,
    /// How many frames it fades in from its start and out to its end.
    pub fade_in: u32,
    pub fade_out: u32,
    /// What its gain multiplies the sound by, and how much faster than its note it plays.
    pub gain: f32,
    pub speed: f64,
}

/// A loop in a span: its first frame, one past its last, and how many frames its crossfade lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopSpan {
    pub start: u64,
    pub end: u64,
    pub crossfade: u32,
}

impl Span {
    /// All of a sample of this many frames, forward and as it is.
    pub fn whole(frames: u64) -> Self {
        Self {
            frames,
            start: 0,
            end: frames,
            loop_: None,
            reverse: false,
            fade_in: 0,
            fade_out: 0,
            gain: 1.0,
            speed: 1.0,
        }
    }
}

impl Playable {
    /// Work out how an entry plays, if its file is in the pool. Points in the file's own frames
    /// become the pool's, and any past the end are brought back to it.
    pub fn of(entry: &BankEntry, samples: &SamplePool) -> Option<Self> {
        let source = entry.source()?;
        let sample = samples.id_of(&source.path)?;
        let frames = Arc::clone(samples.shared(sample)?);
        let rates = samples.rates(sample);
        let length = frames.frames.len() as u64;
        let at = |frame: u64| rates.loaded_frame(frame).min(length);
        let points = &entry.edit.points;
        let play = entry.edit.play;
        let start = at(points.start);
        let end = points.end.map_or(length, at).max(start);
        // The crossfade runs into the loop from just before it, so it can't start before the
        // sample does, or be more than half the loop.
        let loop_ = points
            .loop_
            .map(|found| {
                (
                    at(found.start),
                    at(found.end),
                    at(u64::from(found.crossfade)),
                )
            })
            .filter(|(start, end, _)| start < end)
            .map(|(start, end, crossfade)| LoopSpan {
                start,
                end,
                crossfade: crossfade.min(start).min((end - start) / 2) as u32,
            });
        let span = Span {
            frames: length,
            start,
            end,
            loop_,
            reverse: play.reverse,
            fade_in: at(u64::from(play.fade_in)) as u32,
            fade_out: at(u64::from(play.fade_out)) as u32,
            gain: play.gain(),
            speed: play.speed(),
        };
        Some(Self {
            sample,
            frames,
            spans: Box::new([span]),
        })
    }

    /// The span a note plays: the whole entry, until slices come.
    pub fn span(&self) -> Span {
        self.spans[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{ContentHash, Loop, PlayParams};
    use crate::samples::Rates;

    fn pool(rates: Rates) -> SamplePool {
        let mut pool = SamplePool::default();
        let frames = vec![[0.0; 2]; 48_000];
        pool.insert_converted("file", SampleData { frames }, rates);
        pool
    }

    #[test]
    fn a_file_alone_plays_whole() {
        let entry = BankEntry::of_file("file", ContentHash::UNKNOWN);
        let playable = Playable::of(&entry, &pool(Rates::SAME)).unwrap();
        assert_eq!(playable.span(), Span::whole(48_000));
        let elsewhere = BankEntry::of_file("missing", ContentHash::UNKNOWN);
        assert!(Playable::of(&elsewhere, &pool(Rates::SAME)).is_none());
    }

    #[test]
    fn points_at_the_files_rate_are_found_at_the_programs() {
        let mut entry = BankEntry::of_file("file", ContentHash::UNKNOWN);
        entry.edit.points.start = 4410;
        entry.edit.points.end = Some(22_050);
        entry.edit.points.loop_ = Some(Loop {
            start: 11_025,
            end: 22_050,
            crossfade: 441,
        });
        entry.edit.play = PlayParams {
            reverse: true,
            fade_in: 441,
            ..PlayParams::default()
        };
        let rates = Rates {
            file: 44_100,
            loaded: 48_000,
        };
        let span = Playable::of(&entry, &pool(rates)).unwrap().span();
        assert_eq!((span.start, span.end), (4800, 24_000));
        assert_eq!(
            span.loop_,
            Some(LoopSpan {
                start: 12_000,
                end: 24_000,
                crossfade: 480,
            })
        );
        assert!(span.reverse);
        assert_eq!(span.fade_in, 480);
    }

    #[test]
    fn points_past_the_end_come_back_to_it_and_a_crossfade_fits_its_loop() {
        let mut entry = BankEntry::of_file("file", ContentHash::UNKNOWN);
        entry.edit.points.start = 60_000;
        entry.edit.points.loop_ = Some(Loop {
            start: 100,
            end: 1100,
            crossfade: 5000,
        });
        let span = Playable::of(&entry, &pool(Rates::SAME)).unwrap().span();
        assert_eq!((span.start, span.end), (48_000, 48_000));
        assert_eq!(span.loop_.map(|found| found.crossfade), Some(100));
    }
}
