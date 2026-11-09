//! What's worked out from a sample file once and kept: its waveform as a pyramid of peaks, its peak
//! level, and where its sound starts and ends. It's kept by the file's content hash in the cache
//! folder, as Ableton keeps an `.asd` beside a sample, but never beside the user's files and never
//! in the song, so it can always be built again.
//!
//! Everything here is at the file's own rate and in its own channels, as an entry's points are.

use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::bank::ContentHash;
use crate::sample::{Decoded, SampleError, decode_sample};

/// The cache file's format. A file of another version is built again rather than misread, so this
/// goes up whenever what's written changes.
pub const ANALYSIS_VERSION: u32 = 1;

/// The finest buckets of the pyramid are `2^FINEST` frames, 256, as Audacity's finest summary is.
/// A view closer than that reads the frames themselves.
pub const FINEST: u32 = 8;

/// The level, in dBFS, below which a frame is silence, for `:trim`.
pub const SILENCE_DB: f32 = -60.0;

/// What a cache file starts with.
const MAGIC: &[u8; 4] = b"VTAN";

/// The lowest and highest sample in a stretch of one channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinMax {
    pub min: f32,
    pub max: f32,
}

impl MinMax {
    /// Nothing yet: joining it to anything gives that.
    pub const EMPTY: Self = Self {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
    };

    /// One sample.
    pub fn of(sample: f32) -> Self {
        Self {
            min: sample,
            max: sample,
        }
    }

    /// Both stretches together.
    pub fn join(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Whether it holds no samples.
    pub fn is_empty(self) -> bool {
        self.min > self.max
    }

    /// The furthest from silence either way, or 0 for nothing.
    pub fn peak(self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.min.abs().max(self.max.abs())
        }
    }
}

/// A min/max pyramid of one file's channels: level `k` holds the lowest and highest sample of each
/// run of `2^(FINEST + k)` frames, per channel, up to one bucket over the whole file. So a view at
/// any zoom reads a bucket or two for each column, and the peak over any stretch reads a few
/// buckets at each level.
#[derive(Debug, Clone, PartialEq)]
pub struct Pyramid {
    channels: usize,
    frames: u64,
    /// Each level's buckets, with each bucket's channels side by side.
    levels: Vec<Vec<MinMax>>,
}

impl Pyramid {
    /// The pyramid of interleaved samples, in this many channels.
    pub fn build(samples: &[f32], channels: usize) -> Self {
        let channels = channels.max(1);
        let frames = samples.len() / channels;
        let size = 1 << FINEST;
        let mut levels = Vec::new();
        if frames > 0 {
            let mut finest = Vec::with_capacity(frames.div_ceil(size) * channels);
            let mut folded = vec![MinMax::EMPTY; channels];
            for bucket in samples[..frames * channels].chunks(size * channels) {
                folded.fill(MinMax::EMPTY);
                for frame in bucket.chunks_exact(channels) {
                    for (fold, sample) in folded.iter_mut().zip(frame) {
                        *fold = fold.join(MinMax::of(*sample));
                    }
                }
                finest.extend_from_slice(&folded);
            }
            levels.push(finest);
            while levels.last().is_some_and(|level| level.len() > channels) {
                let below = levels.last().expect("a level below");
                let mut level = Vec::with_capacity((below.len() / channels).div_ceil(2) * channels);
                for pair in below.chunks(channels * 2) {
                    let (first, second) = pair.split_at(channels);
                    for (channel, fold) in first.iter().enumerate() {
                        level.push(second.get(channel).map_or(*fold, |other| fold.join(*other)));
                    }
                }
                levels.push(level);
            }
        }
        Self {
            channels,
            frames: frames as u64,
            levels,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The file's length in frames.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// How many levels it has: none for an empty file.
    pub fn levels(&self) -> usize {
        self.levels.len()
    }

    /// How many frames each bucket of a level covers.
    pub fn bucket_frames(level: usize) -> u64 {
        1 << (FINEST as usize + level)
    }

    /// How many buckets a level has.
    pub fn buckets(&self, level: usize) -> usize {
        self.levels.get(level).map_or(0, |buckets| buckets.len()) / self.channels
    }

    /// One bucket of one channel.
    pub fn bucket(&self, level: usize, bucket: usize, channel: usize) -> MinMax {
        self.levels[level][bucket * self.channels + channel]
    }

    /// The finest buckets covering a stretch of frames, as a range of them.
    fn covering(&self, frames: Range<u64>) -> Range<usize> {
        let end = frames.end.min(self.frames);
        if frames.start >= end {
            return 0..0;
        }
        let size = Self::bucket_frames(0);
        (frames.start / size) as usize..end.div_ceil(size) as usize
    }

    /// The lowest and highest sample of one channel over a stretch of frames, read a few buckets
    /// at each level, so it's quick however long the stretch. It's exact for a stretch that starts
    /// and ends on the finest buckets' edges, and otherwise covers the buckets either end is in.
    pub fn fold(&self, channel: usize, frames: Range<u64>) -> MinMax {
        let Range {
            start: mut low,
            end: mut high,
        } = self.covering(frames);
        let mut fold = MinMax::EMPTY;
        let mut level = 0;
        // Take the odd bucket at either end, then go up a level, where each bucket is two of these.
        while low < high {
            if low % 2 == 1 {
                fold = fold.join(self.bucket(level, low, channel));
                low += 1;
            }
            if high % 2 == 1 {
                high -= 1;
                fold = fold.join(self.bucket(level, high, channel));
            }
            low /= 2;
            high /= 2;
            level += 1;
        }
        fold
    }

    /// The peak over a stretch of frames, of whichever channel is louder, as [`Pyramid::fold`]
    /// reads it.
    pub fn peak(&self, frames: Range<u64>) -> f32 {
        (0..self.channels)
            .map(|channel| self.fold(channel, frames.clone()).peak())
            .fold(0.0, f32::max)
    }

    /// One channel over a stretch of frames, as this many columns, each its lowest and highest
    /// sample. Each column reads a bucket or two of the level whose buckets are nearest its width
    /// without being wider, so it's as quick at any zoom. Columns narrower than the finest buckets
    /// each show the bucket they're in.
    pub fn view(&self, channel: usize, frames: Range<u64>, columns: usize) -> Vec<MinMax> {
        let end = frames.end.min(self.frames);
        let start = frames.start.min(end);
        let length = end - start;
        if columns == 0 || self.levels.is_empty() {
            return vec![MinMax::EMPTY; columns];
        }
        let per_column = (length / columns as u64).max(1);
        let level = (per_column.ilog2().saturating_sub(FINEST) as usize).min(self.levels() - 1);
        let shift = FINEST as usize + level;
        let buckets = self.buckets(level);
        (0..columns as u64)
            .map(|column| {
                let from = start + column * length / columns as u64;
                let to = (start + (column + 1) * length / columns as u64).max(from + 1);
                let first = (from >> shift) as usize;
                let last = (to.div_ceil(1 << shift) as usize).min(buckets);
                (first..last).fold(MinMax::EMPTY, |fold, bucket| {
                    fold.join(self.bucket(level, bucket, channel))
                })
            })
            .collect()
    }
}

/// What's worked out from one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The content it's of.
    pub hash: ContentHash,
    /// The file's own frames a second.
    pub rate: u32,
    pub pyramid: Pyramid,
    /// The highest level in the file, of whichever channel is louder, so normalising scales every
    /// channel the same.
    pub peak: f32,
    /// From the first frame above [`SILENCE_DB`] in any channel to just past the last, or none if
    /// it's all silence.
    pub sound: Option<Range<u64>>,
}

impl Analysis {
    /// Work it out from a file's sound, as it holds it.
    pub fn of(hash: ContentHash, decoded: &Decoded) -> Self {
        let channels = usize::from(decoded.channels.max(1));
        let samples = &decoded.samples;
        let pyramid = Pyramid::build(samples, channels);
        let peak = pyramid.peak(0..pyramid.frames());
        let threshold = 10_f32.powf(SILENCE_DB / 20.0);
        let loud = |frame: &[f32]| frame.iter().any(|sample| sample.abs() > threshold);
        let first = samples.chunks_exact(channels).position(loud);
        let sound = first.map(|first| {
            let last = samples
                .chunks_exact(channels)
                .rposition(loud)
                .unwrap_or(first);
            first as u64..last as u64 + 1
        });
        Self {
            hash,
            rate: decoded.rate,
            pyramid,
            peak,
            sound,
        }
    }

    pub fn channels(&self) -> usize {
        self.pyramid.channels()
    }

    /// The file's length in frames.
    pub fn frames(&self) -> u64 {
        self.pyramid.frames()
    }

    /// The bytes of its cache file.
    pub fn encode(&self) -> Vec<u8> {
        let pyramid = &self.pyramid;
        let buckets: usize = pyramid.levels.iter().map(Vec::len).sum();
        let mut bytes = Vec::with_capacity(64 + buckets * 8);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&ANALYSIS_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.hash.0.to_le_bytes());
        bytes.extend_from_slice(&self.rate.to_le_bytes());
        bytes.extend_from_slice(&(pyramid.channels as u32).to_le_bytes());
        bytes.extend_from_slice(&pyramid.frames.to_le_bytes());
        bytes.extend_from_slice(&self.peak.to_le_bytes());
        let sound = self.sound.clone().unwrap_or(u64::MAX..u64::MAX);
        bytes.extend_from_slice(&sound.start.to_le_bytes());
        bytes.extend_from_slice(&sound.end.to_le_bytes());
        bytes.extend_from_slice(&(pyramid.levels.len() as u32).to_le_bytes());
        for level in &pyramid.levels {
            bytes.extend_from_slice(&(level.len() as u64).to_le_bytes());
            for fold in level {
                bytes.extend_from_slice(&fold.min.to_le_bytes());
                bytes.extend_from_slice(&fold.max.to_le_bytes());
            }
        }
        bytes
    }

    /// Read a cache file's bytes, if they're whole, of this version, and of this content.
    pub fn decode(bytes: &[u8], hash: ContentHash) -> Option<Self> {
        let mut reader = Reader(bytes);
        if reader.take(4)? != MAGIC || reader.u32()? != ANALYSIS_VERSION {
            return None;
        }
        if ContentHash(u128::from_le_bytes(reader.array()?)) != hash {
            return None;
        }
        let rate = reader.u32()?;
        let channels = reader.u32()? as usize;
        let frames = reader.u64()?;
        let peak = f32::from_le_bytes(reader.array()?);
        let sound = reader.u64()?..reader.u64()?;
        let count = reader.u32()? as usize;
        if channels == 0 {
            return None;
        }
        let mut levels = Vec::with_capacity(count.min(64));
        let mut expected = (frames.div_ceil(Pyramid::bucket_frames(0)) as usize) * channels;
        for _ in 0..count {
            let length = reader.u64()? as usize;
            if length != expected {
                return None;
            }
            let (numbers, _) = reader.take(length.checked_mul(8)?)?.as_chunks::<4>();
            let level = numbers
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[min, max]| MinMax {
                    min: f32::from_le_bytes(*min),
                    max: f32::from_le_bytes(*max),
                })
                .collect();
            levels.push(level);
            expected = (expected / channels).div_ceil(2) * channels;
        }
        // A pyramid goes up to one bucket over the whole file, or has no levels if it's empty.
        let top = levels.last().map_or(0, Vec::len);
        if !reader.0.is_empty() || top != if frames == 0 { 0 } else { channels } {
            return None;
        }
        Some(Self {
            hash,
            rate,
            pyramid: Pyramid {
                channels,
                frames,
                levels,
            },
            peak,
            sound: (sound.start != u64::MAX).then_some(sound),
        })
    }
}

/// Reads a cache file's numbers in turn, giving none past its end.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(taken)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_le_bytes)
    }
}

/// Why a file couldn't be analysed.
#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("{path}: {source}")]
    Read { path: String, source: io::Error },
    #[error(transparent)]
    Decode(#[from] SampleError),
}

/// Where an analysis came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Read from the cache folder.
    Cached,
    /// Worked out from the file, and kept in the cache folder if there is one.
    Built,
}

/// The folder analyses are kept in: `vimtracker/analysis` in `$XDG_CACHE_HOME` if it's set, or else
/// in `~/Library/Caches` on macOS and `~/.cache` elsewhere.
pub fn cache_folder() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|base| !base.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let home = PathBuf::from(std::env::var_os("HOME")?);
            Some(if cfg!(target_os = "macos") {
                home.join("Library/Caches")
            } else {
                home.join(".cache")
            })
        })?;
    Some(base.join("vimtracker").join("analysis"))
}

/// Where a content's analysis is kept in a cache folder.
pub fn cache_file(folder: &Path, hash: ContentHash) -> PathBuf {
    folder.join(hash.to_string())
}

/// A file's analysis: read from the cache folder if it's there, or else worked out and kept there.
/// It's kept by the content hash of what was read, which is `hash` unless the file has changed
/// since, so a changed file is never given another's analysis. With no folder, it's only worked
/// out.
pub fn analyse(
    path: &Path,
    hash: ContentHash,
    cache: Option<&Path>,
) -> Result<(Analysis, Found), AnalysisError> {
    if let Some(folder) = cache
        && hash.is_known()
        && let Ok(bytes) = fs::read(cache_file(folder, hash))
        && let Some(analysis) = Analysis::decode(&bytes, hash)
    {
        return Ok((analysis, Found::Cached));
    }
    let read = |source| AnalysisError::Read {
        path: path.display().to_string(),
        source,
    };
    let actual = ContentHash::of(&fs::read(path).map_err(read)?);
    let analysis = Analysis::of(actual, &decode_sample(path)?);
    if let Some(folder) = cache {
        // A cache that can't be written is only slower next time.
        let _ = keep(folder, &analysis);
    }
    Ok((analysis, Found::Built))
}

/// Write an analysis into a cache folder, whole or not at all, so a reader never sees half of one.
fn keep(folder: &Path, analysis: &Analysis) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    let file = cache_file(folder, analysis.hash);
    let partial = folder.join(format!("{}.{}.part", analysis.hash, std::process::id()));
    fs::write(&partial, analysis.encode())?;
    fs::rename(&partial, &file).inspect_err(|_| {
        let _ = fs::remove_file(&partial);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wobbling stereo signal whose channels differ, long enough for several levels, with an
    /// odd length so the last buckets are partly full.
    fn signal(frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|at| {
                let t = at as f32;
                [
                    (t * 0.013).sin() * (t * 0.0007).cos(),
                    0.4 * (t * 0.031).sin() - 0.1,
                ]
            })
            .collect()
    }

    fn brute(samples: &[f32], channels: usize, channel: usize, frames: Range<u64>) -> MinMax {
        samples
            .chunks_exact(channels)
            .skip(frames.start as usize)
            .take((frames.end - frames.start) as usize)
            .fold(MinMax::EMPTY, |fold, frame| {
                fold.join(MinMax::of(frame[channel]))
            })
    }

    #[test]
    fn each_levels_buckets_match_a_fold_over_their_frames() {
        let frames = 100_003;
        let samples = signal(frames);
        let pyramid = Pyramid::build(&samples, 2);
        assert_eq!(pyramid.frames(), frames as u64);
        assert!(pyramid.levels() > 5);
        for level in 0..pyramid.levels() {
            let size = Pyramid::bucket_frames(level);
            assert_eq!(
                pyramid.buckets(level),
                (frames as u64).div_ceil(size) as usize
            );
            for bucket in 0..pyramid.buckets(level) {
                let start = bucket as u64 * size;
                let end = (start + size).min(frames as u64);
                for channel in 0..2 {
                    assert_eq!(
                        pyramid.bucket(level, bucket, channel),
                        brute(&samples, 2, channel, start..end),
                        "level {level}, bucket {bucket}, channel {channel}"
                    );
                }
            }
        }
        assert_eq!(
            pyramid.buckets(pyramid.levels() - 1),
            1,
            "the top is one bucket"
        );
    }

    #[test]
    fn a_stretchs_peak_from_the_pyramid_matches_a_fold_over_its_frames() {
        let frames = 70_001_u64;
        let samples = signal(frames as usize);
        let pyramid = Pyramid::build(&samples, 2);
        let size = Pyramid::bucket_frames(0);
        let mut seed = 12_345_u64;
        let mut next = |below: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) % below
        };
        for _ in 0..500 {
            let (a, b) = (next(frames + 1), next(frames + 1));
            let (start, end) = (a.min(b), a.max(b));
            // On the finest buckets' edges, it's exact.
            let (on_start, on_end) = (start / size * size, end.div_ceil(size) * size);
            let on_end = on_end.min(frames);
            for channel in 0..2 {
                assert_eq!(
                    pyramid.fold(channel, on_start..on_end),
                    brute(&samples, 2, channel, on_start..on_end)
                );
                // Off them, it covers the buckets the ends are in.
                assert_eq!(
                    pyramid.fold(channel, start..end),
                    brute(&samples, 2, channel, on_start..on_end)
                );
            }
            let linked = (0..2)
                .map(|channel| brute(&samples, 2, channel, on_start..on_end).peak())
                .fold(0.0, f32::max);
            assert_eq!(pyramid.peak(on_start..on_end), linked);
        }
        assert_eq!(pyramid.fold(0, 10..10), MinMax::EMPTY);
        assert_eq!(pyramid.peak(frames..frames + 100), 0.0);
    }

    #[test]
    fn a_view_reads_each_column_from_the_nearest_level() {
        let frames = 1 << 20;
        let samples = signal(frames);
        let pyramid = Pyramid::build(&samples, 2);
        for columns in [1, 7, 160, 2400] {
            let view = pyramid.view(1, 0..frames as u64, columns);
            assert_eq!(view.len(), columns);
            let whole = view
                .iter()
                .fold(MinMax::EMPTY, |fold, column| fold.join(*column));
            assert_eq!(whole, brute(&samples, 2, 1, 0..frames as u64));
            // Each column holds at least the frames it stands for.
            for (column, shown) in view.iter().enumerate() {
                let from = (column * frames / columns) as u64;
                let to = ((column + 1) * frames / columns) as u64;
                let exact = brute(&samples, 2, 1, from..to);
                assert!(shown.min <= exact.min && shown.max >= exact.max);
            }
        }
        // Columns on bucket edges show exactly their frames.
        let view = pyramid.view(0, 0..1 << 16, 16);
        for (column, shown) in view.iter().enumerate() {
            let from = column as u64 * 4096;
            assert_eq!(*shown, brute(&samples, 2, 0, from..from + 4096));
        }
    }

    #[test]
    fn an_analysis_finds_the_linked_peak_and_where_the_sound_is() {
        let mut samples = vec![0.0_f32; 2 * 1000];
        // Quiet noise below the threshold, then a hit in the right channel only.
        samples[2 * 10] = 0.0005;
        samples[2 * 300 + 1] = -0.8;
        samples[2 * 302] = 0.3;
        samples[2 * 700 + 1] = 0.01;
        let decoded = Decoded {
            samples,
            channels: 2,
            rate: 44_100,
        };
        let analysis = Analysis::of(ContentHash(7), &decoded);
        assert_eq!(analysis.peak, 0.8);
        assert_eq!(analysis.sound, Some(300..701));
        assert_eq!(analysis.frames(), 1000);
        assert_eq!(analysis.rate, 44_100);
        let silent = Decoded {
            samples: vec![0.0; 20],
            channels: 1,
            rate: 48_000,
        };
        let silent = Analysis::of(ContentHash(8), &silent);
        assert_eq!((silent.peak, silent.sound), (0.0, None));
        let empty = Decoded {
            samples: Vec::new(),
            channels: 2,
            rate: 48_000,
        };
        let empty = Analysis::of(ContentHash(9), &empty);
        assert_eq!(empty.pyramid.levels(), 0);
        assert_eq!(empty.pyramid.view(0, 0..10, 4), vec![MinMax::EMPTY; 4]);
    }

    #[test]
    fn a_cache_file_reads_back_only_whole_of_its_version_and_content() {
        let decoded = Decoded {
            samples: signal(5000),
            channels: 2,
            rate: 48_000,
        };
        let analysis = Analysis::of(ContentHash(99), &decoded);
        let bytes = analysis.encode();
        assert_eq!(Analysis::decode(&bytes, ContentHash(99)), Some(analysis));
        assert_eq!(Analysis::decode(&bytes, ContentHash(98)), None);
        assert_eq!(
            Analysis::decode(&bytes[..bytes.len() - 1], ContentHash(99)),
            None
        );
        let mut other = bytes.clone();
        other[4..8].copy_from_slice(&(ANALYSIS_VERSION + 1).to_le_bytes());
        assert_eq!(Analysis::decode(&other, ContentHash(99)), None);
        let mut longer = bytes;
        longer.push(0);
        assert_eq!(Analysis::decode(&longer, ContentHash(99)), None);
    }
}
