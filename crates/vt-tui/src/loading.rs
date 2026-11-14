//! Samples read on a thread of their own, for the explorer's preview and for playing the file its
//! cursor is on, so a long file doesn't hold up the screen. The newest file asked for is the one
//! read, so moving quickly past a folder of them doesn't queue each one, and the last few read are
//! kept, so moving back is instant.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use vt_model::sample::{SampleData, SampleInfo, load_sample_with_info};

use crate::kitty;

/// How many files read are kept. A long file can be large, so it's only a few.
pub const KEPT: usize = 4;

/// The longest the tests and the screenshots wait for a file to be read. One is read in well under
/// a second, but a heavily loaded machine has taken over ten, and a wait ends as soon as the file
/// is in, so a long limit costs nothing.
pub const WAIT: Duration = Duration::from_secs(60);

/// A file read: its sound at the program's rate, what the file held, and its waveform folded for
/// the preview.
#[derive(Debug)]
pub struct Loaded {
    pub sample: Arc<SampleData>,
    pub info: SampleInfo,
    pub wave: Vec<(usize, usize)>,
    /// A loop or a one-shot by how its sound ends, if that says (decision 315).
    pub heard: Option<&'static str>,
}

/// A file read, or why it couldn't be.
pub type Read = Result<Arc<Loaded>, String>;

/// The thread that reads files, started when the first is asked for.
pub struct Loader {
    rate: u32,
    /// The waveform's size, in characters.
    wave: (usize, usize),
    requests: Option<Sender<PathBuf>>,
    results: Option<Receiver<(PathBuf, Read)>>,
    /// The file last asked for, until it's read.
    waiting: Option<PathBuf>,
    /// The files read, the newest last.
    kept: Vec<(PathBuf, Read)>,
}

impl Loader {
    /// Ready to read files at this rate, with waveforms this many characters across and down.
    pub fn new(rate: u32, wave: (usize, usize)) -> Self {
        Self {
            rate,
            wave,
            requests: None,
            results: None,
            waiting: None,
            kept: Vec::new(),
        }
    }

    /// A file already read.
    pub fn get(&self, path: &Path) -> Option<&Read> {
        self.kept
            .iter()
            .find(|(kept, _)| kept == path)
            .map(|(_, read)| read)
    }

    /// A file, if it's been read. If not, it's asked for, and [`Loader::poll`] says when it's in.
    pub fn want(&mut self, path: &Path) -> Option<Read> {
        if let Some(read) = self.get(path) {
            return Some(read.clone());
        }
        if self.waiting.as_deref() == Some(path) {
            return None;
        }
        let (rate, wave) = (self.rate, self.wave);
        let requests = self.requests.get_or_insert_with(|| {
            let (requests, asked) = mpsc::channel::<PathBuf>();
            let (send, results) = mpsc::channel();
            self.results = Some(results);
            std::thread::spawn(move || {
                while let Ok(mut path) = asked.recv() {
                    // Only the newest matters.
                    while let Ok(newer) = asked.try_recv() {
                        path = newer;
                    }
                    let read = read(&path, rate, wave);
                    if send.send((path, read)).is_err() {
                        return;
                    }
                }
            });
            requests
        });
        if requests.send(path.to_path_buf()).is_ok() {
            self.waiting = Some(path.to_path_buf());
        }
        None
    }

    /// Whether a file asked for is still being read.
    pub fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// Take in the files read since last asked, and say which they were.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        let mut done = Vec::new();
        while let Some(Ok((path, read))) = self.results.as_ref().map(Receiver::try_recv) {
            self.keep(path.clone(), read);
            done.push(path);
        }
        done
    }

    /// Wait up to `limit` for the file asked for, taking in any others on the way, for tests and
    /// the screenshots.
    pub fn wait(&mut self, limit: Duration) -> Vec<PathBuf> {
        let until = Instant::now() + limit;
        let mut done = self.poll();
        while self.waiting.is_some() {
            let Some(results) = self.results.as_ref() else {
                break;
            };
            match results.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok((path, read)) => {
                    self.keep(path.clone(), read);
                    done.push(path);
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        done
    }

    fn keep(&mut self, path: PathBuf, read: Read) {
        if self.waiting.as_ref() == Some(&path) {
            self.waiting = None;
        }
        self.kept.retain(|(kept, _)| *kept != path);
        self.kept.push((path, read));
        let over = self.kept.len().saturating_sub(KEPT);
        self.kept.drain(..over);
    }
}

/// Read a file and fold its waveform.
fn read(path: &Path, rate: u32, (columns, rows): (usize, usize)) -> Read {
    let (sample, info) = load_sample_with_info(path, rate).map_err(|error| error.to_string())?;
    let wave = kitty::waveform_spans(&sample.frames, columns, rows);
    let end = vt_model::tag::sound_end(&sample.frames);
    let heard = vt_model::tag::guess_heard(sample.frames.len() as u64, rate, end);
    Ok(Arc::new(Loaded {
        sample: Arc::new(sample),
        info,
        wave,
        heard,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, frames: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..frames {
            writer.write_sample(1000_i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-loading-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_file_is_read_in_the_background_and_kept() {
        let folder = folder("kept");
        let files: Vec<PathBuf> = (0..=KEPT)
            .map(|at| {
                let path = folder.join(format!("{at}.wav"));
                wav(&path, 44_100);
                path
            })
            .collect();
        let mut loader = Loader::new(48_000, (8, 1));
        assert!(loader.want(&files[0]).is_none(), "not read yet");
        assert!(loader.is_waiting());
        assert!(loader.want(&files[0]).is_none(), "and not asked for twice");
        assert_eq!(loader.wait(WAIT), [files[0].clone()]);
        let read = loader.want(&files[0]).unwrap().unwrap();
        assert_eq!(
            read.info,
            SampleInfo {
                rate: 44_100,
                channels: 1,
                frames: 44_100
            }
        );
        assert!(
            read.sample.frames.len().abs_diff(48_000) <= 2,
            "at the program's rate"
        );
        assert_eq!(read.wave.len(), 8 * 6, "six columns of dots to a character");
        // Only a few are kept.
        for file in &files[1..] {
            loader.want(file);
            assert_eq!(loader.wait(WAIT), std::slice::from_ref(file));
        }
        assert!(loader.get(&files[0]).is_none(), "the oldest has gone");
        assert!(loader.get(&files[KEPT]).is_some());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_file_that_wont_read_says_why() {
        let folder = folder("bad");
        let path = folder.join("bad.wav");
        std::fs::write(&path, "not audio").unwrap();
        let mut loader = Loader::new(48_000, (8, 1));
        loader.want(&path);
        assert_eq!(loader.wait(WAIT), std::slice::from_ref(&path));
        let error = loader.want(&path).unwrap().unwrap_err();
        assert!(error.contains("bad.wav"), "{error}");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
