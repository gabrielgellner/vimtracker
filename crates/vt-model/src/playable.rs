//! What the audio thread plays for one of the bank's entries: its sound at the program's rate and
//! the spans it can play, one for each slice in `SLI`'s order, each with its points, loop, gain,
//! pan, tune, reverse and fades worked out, a slice's own settings in place of the entry's. Its
//! gain takes in what normalising it needs, once its file's analysis is in.
//!
//! It's built from an entry's [`SampleEdit`](crate::bank::SampleEdit) on the edit thread, so the
//! engine never reads an edit: a new field there changes how a span is worked out here, and the
//! engine plays the span as it always has. A slice's settings so cost the voice nothing, and its
//! marks are each letter's slice number, so an `SLI 'a` is looked up rather than searched for.

use std::sync::Arc;

use crate::bank::{BankEntry, MARK_COUNT, MARK_LETTERS, PlayParams, SliceParams, Warp, mark_index};
use crate::sample::SampleData;
use crate::samples::{SampleId, SamplePool};

/// One entry, ready to play.
#[derive(Debug, Clone, PartialEq)]
pub struct Playable {
    /// Its place in the pool, which a voice keeps reading until its note ends.
    pub sample: SampleId,
    /// Its sound at the program's rate.
    pub frames: Arc<SampleData>,
    /// What it can play, by slice number: the whole entry alone, if it has no slices.
    pub spans: Box<[Span]>,
    /// The whole entry from its start to its end, as it plays with no slice's own settings, which
    /// the sample page plays from its cursor.
    pub whole: Span,
    /// Where the whole entry starts and ends, which a slice played on through, `Thru`, plays to.
    pub start: u64,
    pub end: u64,
    /// The number of the slice each letter names, `a` first, or none for a letter naming none.
    pub marks: [Option<u8>; MARK_COUNT],
    /// Where its sounds start, between its start and its end, for `Play`'s `Tempo` to start each
    /// at its own time when its warp is `Beats`: its slices' starts, or the sounds its `Sens`
    /// finds once its analysis is in if it has no slices. Empty for any other warp.
    pub beats: Box<[u64]>,
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
    /// What its pan multiplies the left and the right by: 1 for both in the middle.
    pub pan: [f32; 2],
    /// How many of its frames each of the song's steps plays, for `Play`'s `Tempo`, so the entry
    /// lasts as many steps as it says from its start to its end: none if it doesn't say.
    pub step_frames: Option<f64>,
    /// The tempo it was made at, in thousandths of a BPM, for `Tempo` to play it at the song's
    /// against, when it says that rather than how many steps it lasts, as an Octatrack file does.
    pub own_millibpm: Option<u32>,
    /// Whether `Tempo` starts each of its sounds at its own time, rather than changing its speed.
    pub beats: bool,
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
            pan: [1.0, 1.0],
            step_frames: None,
            own_millibpm: None,
            beats: false,
        }
    }

    /// Whether `Play`'s `Tempo` can keep it to the tempo: it says how long it is, or its tempo.
    pub fn keeps_tempo(&self) -> bool {
        self.step_frames.is_some() || self.own_millibpm.is_some()
    }

    /// How many of its frames each of the song's steps plays under `Tempo`, at the program's
    /// rate: from how many steps it lasts, or else a step at its own tempo.
    pub fn step_frames_at(&self, rate: u32) -> Option<f64> {
        self.step_frames.or_else(|| {
            let millibpm = f64::from(self.own_millibpm?.max(1));
            // A step is a sixteenth, a quarter of a beat.
            Some(f64::from(rate) * 60.0 * 1000.0 / millibpm / 4.0)
        })
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
        let gain = play.normalize_gain(normalize_peak(entry, samples));
        let step_frames = entry
            .edit
            .hints
            .length_steps()
            .filter(|_| end > start)
            .map(|steps| (end - start) as f64 / f64::from(steps.max(1)));
        let own_millibpm = entry.edit.hints.millibpm.filter(|_| step_frames.is_none());
        let beats = play.warp == Warp::Beats;
        let span = |from: u64, to: u64, over: &SliceParams, sliced: bool| {
            let play: PlayParams = over.over(play);
            Span {
                frames: length,
                start: from,
                end: to,
                // A slice keeps the entry's loop only if the loop is inside it, and otherwise
                // loops all of itself that plays, as an entry with no loop does.
                loop_: loop_.filter(|found| !sliced || (from <= found.start && found.end <= to)),
                reverse: play.reverse,
                fade_in: at(u64::from(play.fade_in)) as u32,
                fade_out: at(u64::from(play.fade_out)) as u32,
                gain: play.gain() * gain,
                speed: play.speed(),
                pan: over.pan_gains(),
                step_frames,
                own_millibpm,
                beats,
            }
        };
        let spans: Box<[Span]> = if points.slices.is_empty() {
            Box::new([span(start, end, &SliceParams::default(), false)])
        } else {
            // The pool's frames bring the end back to the file's.
            (0..points.slices.len())
                .map(|number| {
                    let frames = points.slice_frames(number, u64::MAX);
                    let from = at(frames.start).clamp(start, end);
                    let to = at(frames.end).clamp(from, end);
                    span(from, to, &points.slices[number].over, true)
                })
                .collect()
        };
        let mut marks = [None; MARK_COUNT];
        for (letter, slot) in MARK_LETTERS.zip(marks.iter_mut()) {
            *slot = points.marked(letter).map(|number| number as u8);
        }
        let beats: Box<[u64]> = match beats {
            false => Box::new([]),
            true => {
                let starts: Vec<u64> = match points.slices.is_empty() {
                    false => points.slices.iter().map(|slice| slice.start).collect(),
                    true => onset_analysis(entry, samples)
                        .map(|analysis| analysis.onsets(entry.edit.hints.sens))
                        .unwrap_or_default(),
                };
                let mut beats: Vec<u64> = starts
                    .into_iter()
                    .map(at)
                    .filter(|frame| (start + 1..end).contains(frame))
                    .collect();
                beats.dedup();
                beats.into()
            }
        };
        Some(Self {
            sample,
            frames,
            spans,
            whole: span(start, end, &SliceParams::default(), false),
            start,
            end,
            marks,
            beats,
        })
    }

    /// Where a place in play order is among the sounds `Beats` starts in a span: the place the
    /// sound it's in starts, and the place the next one does, or the span's end. A reversed span
    /// plays each from its end back, so its sounds start where the forward ones end.
    pub fn beat_around(&self, span: &Span, place: f64) -> (f64, f64) {
        let frames = span.frames as f64;
        let (first, last) = if span.reverse {
            (frames - span.end as f64, frames - span.start as f64)
        } else {
            (span.start as f64, span.end as f64)
        };
        let mut from = first;
        let mut until = last;
        let cuts = self.beats.iter().map(|frame| match span.reverse {
            false => *frame as f64,
            true => frames - *frame as f64,
        });
        for cut in cuts.filter(|cut| (first..last).contains(cut)) {
            if cut <= place {
                from = from.max(cut);
            } else {
                until = until.min(cut);
            }
        }
        (from, until)
    }

    /// The number of the slice a letter names, if it names one.
    pub fn marked(&self, letter: char) -> Option<u8> {
        self.marks[mark_index(letter)?]
    }

    /// The span a note plays: its slice's, by number, clamped to the last, or with `Thru`, on
    /// past the slice's end to the entry's, the way it plays, so a reversed one carries on back
    /// to the entry's start. It's only looked up, as the callback asks for it.
    pub fn span(&self, slice: u8, thru: bool) -> Span {
        let span = self.spans[usize::from(slice).min(self.spans.len() - 1)];
        match (thru, span.reverse) {
            (false, _) => span,
            (true, false) => Span {
                end: self.end,
                loop_: None,
                ..span
            },
            (true, true) => Span {
                start: self.start,
                loop_: None,
                ..span
            },
        }
    }
}

/// The analysis an entry starts its sounds by under `Beats`, when it has no slices to start them,
/// if it's in. A [`Playable`] worked out before it came in is out of date once it has.
pub fn onset_analysis<'a>(
    entry: &BankEntry,
    samples: &'a SamplePool,
) -> Option<&'a Arc<crate::analysis::Analysis>> {
    if entry.edit.play.warp != Warp::Beats || !entry.edit.points.slices.is_empty() {
        return None;
    }
    samples.analysis(entry.source()?.hash)
}

/// The peak normalising an entry works from, if it asks to be normalised and its file's analysis
/// is in. A [`Playable`] worked out before its analysis came in is out of date once it has.
pub fn normalize_peak(entry: &BankEntry, samples: &SamplePool) -> Option<f32> {
    entry.edit.play.normalize?;
    let source = entry.source()?;
    samples.analysis(source.hash).map(|analysis| analysis.peak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{ContentHash, Loop, MOST_PAN};
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
        assert_eq!(playable.span(0, false), Span::whole(48_000));
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
        let span = Playable::of(&entry, &pool(rates)).unwrap().span(0, false);
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
        let span = Playable::of(&entry, &pool(Rates::SAME))
            .unwrap()
            .span(0, false);
        assert_eq!((span.start, span.end), (48_000, 48_000));
        assert_eq!(span.loop_.map(|found| found.crossfade), Some(100));
    }

    /// An entry from 0.1 s to 0.9 s of the second, in four slices.
    fn sliced() -> BankEntry {
        let mut entry = BankEntry::of_file("file", ContentHash::UNKNOWN);
        entry.edit.points.start = 4800;
        entry.edit.points.end = Some(43_200);
        let starts = entry.edit.points.even_starts(4, 48_000);
        assert_eq!(starts, [4800, 14_400, 24_000, 33_600]);
        assert_eq!(entry.edit.points.reslice(&starts), None, "none before");
        entry
    }

    #[test]
    fn each_slice_plays_from_its_start_to_the_next_and_past_the_last_is_the_last() {
        let playable = Playable::of(&sliced(), &pool(Rates::SAME)).unwrap();
        assert_eq!(playable.spans.len(), 4);
        let ends = |slice| {
            let span = playable.span(slice, false);
            (span.start, span.end)
        };
        assert_eq!(ends(0), (4800, 14_400));
        assert_eq!(
            ends(3),
            (33_600, 43_200),
            "the last ends at the entry's end"
        );
        assert_eq!(ends(9), ends(3), "one past the last plays the last");
        let thru = playable.span(1, true);
        assert_eq!((thru.start, thru.end), (14_400, 43_200));
    }

    #[test]
    fn a_slices_own_settings_are_in_its_span_and_the_rest_play_as_the_entry() {
        let mut entry = sliced();
        entry.edit.play = PlayParams {
            gain_db: -6.0,
            tune: 2,
            cents: 10,
            fade_in: 48,
            ..PlayParams::default()
        };
        entry.edit.points.slices[2].over = SliceParams {
            gain_db: Some(0.0),
            pan: Some(-MOST_PAN),
            tune: Some(-12),
            reverse: Some(true),
            fade_in: Some(0),
            fade_out: Some(480),
        };
        let playable = Playable::of(&entry, &pool(Rates::SAME)).unwrap();
        let own = playable.span(2, false);
        assert_eq!(own.gain, 1.0);
        assert_eq!(
            own.speed,
            PlayParams {
                tune: -12,
                cents: 10,
                ..PlayParams::default()
            }
            .speed()
        );
        assert!(own.reverse);
        assert_eq!((own.fade_in, own.fade_out), (0, 480));
        assert!((own.pan[0] - std::f32::consts::SQRT_2).abs() < 1e-6 && own.pan[1].abs() < 1e-6);
        let other = playable.span(1, false);
        assert!((other.gain - entry.edit.play.gain()).abs() < 1e-6);
        assert_eq!(other.speed, entry.edit.play.speed());
        assert!(!other.reverse);
        assert_eq!((other.fade_in, other.fade_out), (48, 0));
        assert_eq!(other.pan, [1.0, 1.0]);
        // Reversed, on through plays back to the entry's start.
        let thru = playable.span(2, true);
        assert_eq!((thru.start, thru.end), (4800, 33_600));
    }

    #[test]
    fn a_slice_keeps_the_entrys_loop_only_if_the_loop_is_inside_it() {
        let mut entry = sliced();
        entry.edit.points.loop_ = Some(Loop {
            start: 16_000,
            end: 20_000,
            crossfade: 0,
        });
        let playable = Playable::of(&entry, &pool(Rates::SAME)).unwrap();
        assert!(playable.span(1, false).loop_.is_some());
        assert!(playable.span(0, false).loop_.is_none());
        assert!(
            playable.span(1, true).loop_.is_none(),
            "on through never loops"
        );
    }

    #[test]
    fn a_length_in_steps_gives_each_span_the_frames_a_step_plays_at_the_programs_rate() {
        let mut entry = sliced();
        assert_eq!(
            Playable::of(&entry, &pool(Rates::SAME))
                .unwrap()
                .whole
                .step_frames,
            None
        );
        entry.edit.hints.set_length_steps(Some(16));
        let rates = Rates {
            file: 44_100,
            loaded: 48_000,
        };
        let playable = Playable::of(&entry, &pool(rates)).unwrap();
        // From 0.1 s to 0.9 s of the second, at the program's rate.
        let step = (43_200.0 * 48.0 / 44.1 - 4800.0 * 48.0 / 44.1) / 16.0;
        let close = |found: Option<f64>| found.is_some_and(|found| (found - step).abs() < 1.0);
        assert!(close(playable.whole.step_frames));
        assert!(
            close(playable.span(2, false).step_frames),
            "every slice keeps the entry's"
        );
    }

    #[test]
    fn beats_start_at_the_slices_and_a_reversed_span_starts_them_from_their_ends() {
        let mut entry = sliced();
        assert!(
            Playable::of(&entry, &pool(Rates::SAME))
                .unwrap()
                .beats
                .is_empty()
        );
        entry.edit.play.warp = Warp::Beats;
        let playable = Playable::of(&entry, &pool(Rates::SAME)).unwrap();
        assert_eq!(&*playable.beats, &[14_400, 24_000, 33_600]);
        let whole = playable.whole;
        assert!(whole.beats);
        assert_eq!(playable.beat_around(&whole, 20_000.0), (14_400.0, 24_000.0));
        assert_eq!(playable.beat_around(&whole, 5000.0), (4800.0, 14_400.0));
        assert_eq!(playable.beat_around(&whole, 40_000.0), (33_600.0, 43_200.0));
        // In play order, reversed: the last slice's end, 43 200, is place 4800.
        let reversed = Span {
            reverse: true,
            ..whole
        };
        assert_eq!(playable.beat_around(&reversed, 5000.0), (4800.0, 14_400.0));
        assert_eq!(
            playable.beat_around(&reversed, 20_000.0),
            (14_400.0, 24_000.0)
        );
        // Within a slice's span, only its own.
        let slice = playable.span(1, false);
        assert_eq!(playable.beat_around(&slice, 20_000.0), (14_400.0, 24_000.0));
    }
}
