//! The bank's files analysed on a thread of their own, as they join the bank or a project opens:
//! each one's peak pyramid, peak level and where its sound is, read from the cache folder if it's
//! there, or worked out and kept there if not. Unlike the explorer's loader, which reads only the
//! newest file asked for, every file asked for is analysed in turn.
//!
//! `:normalize` and `:trim` are built on them, and change only the bank entry's settings.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use vt_model::action::Action;
use vt_model::analysis::{Analysis, Found, analyse};
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::project;
use vt_model::samples::SamplePool;

use crate::app::App;

/// The longest the bank's analyses are waited for, by a render or `:trim`.
const WAIT: Duration = Duration::from_secs(30);

/// One file's analysis, or why there's none.
#[derive(Debug)]
pub struct Analysed {
    /// The content it was asked for by. The analysis is of another if the file has changed since.
    pub asked: ContentHash,
    pub result: Result<(Arc<Analysis>, Found), String>,
}

/// The thread that analyses files, started when the first is asked for.
pub struct Analyst {
    /// Where analyses are kept, or none to keep them only in memory, as tests do.
    cache: Option<PathBuf>,
    requests: Option<Sender<(PathBuf, ContentHash)>>,
    results: Option<Receiver<Analysed>>,
    /// Asked for and not in yet.
    waiting: HashSet<ContentHash>,
    /// Not to be asked for again: couldn't be read, or have changed since they joined the bank.
    failed: HashSet<ContentHash>,
    /// How many were worked out, and how many read from the cache folder.
    pub built: usize,
    pub cached: usize,
}

impl Analyst {
    /// Ready to analyse files, keeping them in a cache folder if there is one.
    pub fn new(cache: Option<PathBuf>) -> Self {
        Self {
            cache,
            requests: None,
            results: None,
            waiting: HashSet::new(),
            failed: HashSet::new(),
            built: 0,
            cached: 0,
        }
    }

    /// Where analyses are kept.
    pub fn cache(&self) -> Option<&Path> {
        self.cache.as_deref()
    }

    /// Ask for a file's analysis, by its content, unless it's on its way or couldn't be had.
    pub fn want(&mut self, path: &Path, hash: ContentHash) {
        if !hash.is_known() || self.waiting.contains(&hash) || self.failed.contains(&hash) {
            return;
        }
        let cache = self.cache.clone();
        let requests = self.requests.get_or_insert_with(|| {
            let (requests, asked) = mpsc::channel::<(PathBuf, ContentHash)>();
            let (send, results) = mpsc::channel();
            self.results = Some(results);
            std::thread::spawn(move || {
                while let Ok((path, hash)) = asked.recv() {
                    let result = analyse(&path, hash, cache.as_deref())
                        .map(|(analysis, found)| (Arc::new(analysis), found))
                        .map_err(|error| error.to_string());
                    let analysed = Analysed {
                        asked: hash,
                        result,
                    };
                    if send.send(analysed).is_err() {
                        return;
                    }
                }
            });
            requests
        });
        if requests.send((path.to_path_buf(), hash)).is_ok() {
            self.waiting.insert(hash);
        }
    }

    /// Whether any file asked for is still being analysed.
    pub fn is_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Take in the analyses done since last asked.
    pub fn poll(&mut self) -> Vec<Analysed> {
        let mut done = Vec::new();
        while let Some(Ok(analysed)) = self.results.as_ref().map(Receiver::try_recv) {
            self.count(&analysed);
            done.push(analysed);
        }
        done
    }

    /// Wait up to `limit` for every file asked for, as a render must and as tests do.
    pub fn wait(&mut self, limit: Duration) -> Vec<Analysed> {
        let until = Instant::now() + limit;
        let mut done = self.poll();
        while self.is_waiting() {
            let Some(results) = self.results.as_ref() else {
                break;
            };
            match results.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(analysed) => {
                    self.count(&analysed);
                    done.push(analysed);
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        done
    }

    fn count(&mut self, analysed: &Analysed) {
        self.waiting.remove(&analysed.asked);
        match &analysed.result {
            Ok((analysis, found)) => {
                match found {
                    Found::Built => self.built += 1,
                    Found::Cached => self.cached += 1,
                }
                if analysis.hash != analysed.asked {
                    self.failed.insert(analysed.asked);
                }
            }
            Err(_) => {
                self.failed.insert(analysed.asked);
            }
        }
    }
}

impl App {
    /// Keep the bank's analyses in a cache folder from now on, as the program does and tests
    /// needn't, and ask for those not in yet.
    pub fn use_analysis_cache(&mut self, folder: Option<PathBuf>) {
        self.analyst = Analyst::new(folder);
        self.analyse_bank();
    }

    /// Ask for the analysis of each of the bank's files that isn't in yet.
    pub(crate) fn analyse_bank(&mut self) {
        let song = Arc::clone(self.song());
        let sources = song.bank.iter().flat_map(|(_, entry)| entry.sources());
        for source in sources {
            if source.hash.is_known() && self.samples.analysis(source.hash).is_none() {
                let path = project::resolve_sample(self.folder.as_deref(), &source.path);
                self.analyst.want(&path, source.hash);
            }
        }
    }

    /// Take in the analyses done in the background, and play with them.
    pub(crate) fn take_analyses(&mut self) {
        let done = self.analyst.poll();
        self.keep_analyses(done);
    }

    /// Wait for every analysis asked for, then take them in, as a render must and as tests do.
    pub fn finish_analysis(&mut self) {
        let done = self.analyst.wait(WAIT);
        self.keep_analyses(done);
    }

    /// Put analyses in the pool and play with them. One of a file that's changed since it joined
    /// the bank isn't kept for it, as it's of other content than the bank's.
    fn keep_analyses(&mut self, done: Vec<Analysed>) {
        let mut pool = None;
        for analysed in done {
            if let Ok((analysis, _)) = analysed.result
                && analysis.hash == analysed.asked
            {
                pool.get_or_insert_with(|| SamplePool::clone(&self.samples))
                    .insert_analysis(analysis);
            }
        }
        if let Some(pool) = pool {
            self.samples = Arc::new(pool);
            self.publish();
        }
    }

    /// The bank entry the current track plays, and its number, or why there's none.
    fn track_entry(&self) -> Result<(u8, BankEntry), String> {
        let track = self.cursor.track;
        let number = self
            .song()
            .tracks
            .get(track)
            .map_or(0, |track| track.sample());
        match self.song().bank.get(number) {
            Some(entry) => Ok((number, entry.clone())),
            None => Err(format!("Track {:02} plays no sample", track + 1)),
        }
    }

    /// The analysis of an entry's file, waiting for it if it's on its way.
    pub(crate) fn entry_analysis(&mut self, entry: &BankEntry) -> Option<Arc<Analysis>> {
        let hash = entry.source()?.hash;
        if self.samples.analysis(hash).is_none() {
            self.analyse_bank();
            self.finish_analysis();
        }
        self.samples.analysis(hash).cloned()
    }

    /// Bring the current track's sample to a peak level, in hundredths of a dBFS, or stop
    /// normalising it, as one undo step. It's a gain worked out from the file's peak, of whichever
    /// channel is louder, so nothing is written.
    pub(crate) fn normalize_sample(&mut self, level: Option<i16>) {
        let (number, mut entry) = match self.track_entry() {
            Ok(found) => found,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let level = level.map(|level| f32::from(level) / 100.0);
        if entry.edit.play.normalize == level {
            self.status = match level {
                Some(level) => format!("Sample {number:02X} is normalised to {level} dBFS already"),
                None => format!("Sample {number:02X} isn't normalised"),
            };
            return;
        }
        entry.edit.play.normalize = level;
        let peak = self
            .entry_analysis(&entry)
            .map(|analysis| analysis.peak)
            .filter(|peak| *peak > 0.0);
        let gain = 20.0 * entry.edit.play.normalize_gain(peak).log10();
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = match (level, peak) {
            (None, _) => format!("Sample {number:02X} is no longer normalised: u to undo"),
            (Some(level), Some(peak)) => format!(
                "Sample {number:02X} normalised to {level} dBFS, {gain:+.1} dB from its peak of {:.1} dBFS: u to undo",
                20.0 * peak.log10()
            ),
            (Some(level), None) => format!(
                "Sample {number:02X} normalises to {level} dBFS, but it's silent or its file \
                 can't be analysed: u to undo"
            ),
        };
    }

    /// Start and end the current track's sample where its sound does, from its analysis, as one
    /// undo step. Nothing is written, and the file's start and end are still there to go back to.
    pub(crate) fn trim_sample(&mut self) {
        let (number, mut entry) = match self.track_entry() {
            Ok(found) => found,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let Some(analysis) = self.entry_analysis(&entry) else {
            self.status = format!(
                "Sample {number:02X} can't be analysed: its file is missing or has changed"
            );
            return;
        };
        let Some(sound) = analysis.sound.clone() else {
            self.status = format!("Sample {number:02X} is all silence, so there's nothing to keep");
            return;
        };
        // Its loop stays where it was, as only where it starts and ends is found here.
        let points = Points {
            start: sound.start,
            end: (sound.end < analysis.frames()).then_some(sound.end),
            ..entry.edit.points.clone()
        };
        let seconds = |frame: u64| frame as f64 / f64::from(analysis.rate.max(1));
        let kept = format!(
            "{:.3} s to {:.3} s of {:.3} s",
            seconds(sound.start),
            seconds(sound.end),
            seconds(analysis.frames())
        );
        if entry.edit.points == points {
            self.status = format!("Sample {number:02X} is trimmed already, to {kept}");
            return;
        }
        entry.edit.points = points;
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = format!("Sample {number:02X} trimmed to {kept}: u to undo");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, level: i16) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for at in 0..2000 {
            let value = if at % 2 == 0 { level } else { level / 2 };
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-analysing-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn files_are_analysed_in_turn_once_each_and_read_back_from_the_cache() {
        let folder = folder("turn");
        let cache = folder.join("cache");
        let files: Vec<(PathBuf, ContentHash)> = (1..=3)
            .map(|at| {
                let path = folder.join(format!("{at}.wav"));
                wav(&path, 1000 * at);
                let hash = ContentHash::of_file(&path).unwrap();
                (path, hash)
            })
            .collect();
        let mut analyst = Analyst::new(Some(cache.clone()));
        for (path, hash) in &files {
            analyst.want(path, *hash);
            analyst.want(path, *hash);
        }
        assert!(analyst.is_waiting());
        let done = analyst.wait(Duration::from_secs(10));
        assert_eq!(done.len(), 3, "each once, none passed over");
        assert_eq!((analyst.built, analyst.cached), (3, 0));
        let mut again = Analyst::new(Some(cache));
        for (path, hash) in &files {
            again.want(path, *hash);
        }
        let done = again.wait(Duration::from_secs(10));
        assert_eq!((again.built, again.cached), (0, 3));
        let (analysis, _) = done[2].result.as_ref().unwrap();
        assert_eq!(analysis.hash, files[2].1);
        assert!((analysis.peak - 3000.0 / 32_768.0).abs() < 1e-6);
    }
}
