//! The sample page, `Space 0`: the sample the cursor's track plays, as a big waveform to move over
//! with Vim's motions. A sample is a buffer and a cell column is a character, so `h` and `l` move a
//! column at the zoom shown, `0` and `$` go to the slice's ends, and `zi` and `zo` zoom on the
//! cursor. It only shows the bank entry for now: editing its points comes with #276.
//!
//! Everything here is in frames at the file's own rate, as an entry's points are, so the cursor
//! stays on the same sound whatever rate the program plays at.

use std::sync::Arc;

use vt_model::analysis::{Analysis, Pyramid, SILENCE_DB};
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::sample::SampleData;
use vt_model::samples::Rates;
use vt_model::selection::Motion;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::views::sample::WaveOnScreen;

/// How many columns the waveform has until the page is first drawn: the author's screen, 163
/// characters across, less a column either side.
pub const DEFAULT_COLUMNS: usize = 161;

/// A key of the sample page's own, beside the motions it shares with the other views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKey {
    /// `^` and `g_`: the first or last frame of the slice that isn't silence, as Vim's first and
    /// last character that isn't blank.
    FirstSound,
    LastSound,
    /// `]]` and `[[`: the next or previous point of any kind.
    NextPoint,
    PreviousPoint,
    /// `zz`, `zs` and `ze`: the view placed with the cursor in its middle, at its left or at its
    /// right.
    CursorMiddle,
    CursorLeft,
    CursorRight,
    /// `zi` and `zo`: half or twice as many frames to a column, on the cursor.
    ZoomIn,
    ZoomOut,
    /// `zO`: the whole sample. A capital is the wider of a pair.
    ZoomAll,
    /// `zv`: the slice the cursor is in, filling the view.
    ZoomSlice,
    /// `zc`: the cursor as a thin line at its frame, or as its column's background.
    CursorStyle,
}

/// Where the sample page is, kept while other views are shown, and started afresh for another
/// sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplePage {
    /// The bank entry and the content it was set up for.
    pub shown: Option<(u8, ContentHash)>,
    /// The frame the cursor is on.
    pub cursor: u64,
    /// How many frames each column of the waveform covers: a power of two, or as many as fit the
    /// whole sample across the view, which is the widest zoom.
    pub zoom: u64,
    /// The first column shown, counted in columns of the zoom from the file's start, so the view
    /// always starts on a column's edge.
    pub column: u64,
    /// How many columns the waveform has, as the page was last drawn.
    pub columns: usize,
    /// Whether the cursor is a thin line at its frame rather than its column's background.
    pub cursor_line: bool,
    /// Where the waveform was last drawn on screen, if it was.
    pub on_screen: Option<WaveOnScreen>,
}

impl Default for SamplePage {
    fn default() -> Self {
        Self {
            shown: None,
            cursor: 0,
            zoom: 1,
            column: 0,
            columns: DEFAULT_COLUMNS,
            cursor_line: true,
            on_screen: None,
        }
    }
}

/// What the page shows: a bank entry, its file's analysis, and its frames as the pool holds them.
#[derive(Clone)]
pub struct Shown {
    pub number: u8,
    pub entry: BankEntry,
    pub analysis: Arc<Analysis>,
    /// At the program's rate, for drawing and searching closer than the analysis goes. None if
    /// the file didn't load.
    pub frames: Option<(Arc<SampleData>, Rates)>,
}

impl Shown {
    /// The file's length in frames.
    pub fn length(&self) -> u64 {
        self.analysis.frames()
    }

    /// The file's own frames a second.
    pub fn rate(&self) -> u32 {
        self.analysis.rate
    }

    /// One past the last frame that plays.
    pub fn end(&self) -> u64 {
        let points = &self.entry.edit.points;
        points.end.unwrap_or(self.length()).min(self.length())
    }

    /// Where each slice starts, in order. Until slices come (#270), the entry is one slice, from
    /// its start to its end.
    pub fn slice_starts(&self) -> Vec<u64> {
        vec![self.entry.edit.points.start.min(self.length())]
    }

    /// The slice a frame is in, by its number, and the frames it covers. A frame before the first
    /// slice is in the first.
    pub fn slice_at(&self, frame: u64) -> (usize, std::ops::Range<u64>) {
        let starts = self.slice_starts();
        let number = starts
            .iter()
            .rposition(|start| *start <= frame)
            .unwrap_or(0);
        (number, self.slice(number))
    }

    /// The frames of a slice by its number, clamped to the last.
    pub fn slice(&self, number: usize) -> std::ops::Range<u64> {
        let starts = self.slice_starts();
        let number = number.min(starts.len() - 1);
        let end = starts.get(number + 1).copied().unwrap_or(self.end());
        starts[number]..end.max(starts[number])
    }

    /// Every point, in order: the start, the end and the loop's, and the slices once they come.
    pub fn points(&self) -> Vec<u64> {
        let Points { start, end, loop_ } = &self.entry.edit.points;
        let mut points = vec![*start];
        points.extend(end);
        if let Some(looped) = loop_ {
            points.extend([looped.start, looped.end]);
        }
        points.retain(|point| *point <= self.length());
        points.sort_unstable();
        points.dedup();
        points
    }

    /// The first or last frame of a stretch whose sound is above [`SILENCE_DB`], in any channel.
    /// The pyramid finds the first bucket with sound in it, and the frames find where in it.
    pub fn sound_edge(&self, frames: std::ops::Range<u64>, last: bool) -> Option<u64> {
        let threshold = 10_f32.powf(SILENCE_DB / 20.0);
        let pyramid = &self.analysis.pyramid;
        let size = Pyramid::bucket_frames(0);
        let loud = |bucket: u64| {
            let from = (bucket * size).max(frames.start);
            let to = ((bucket + 1) * size).min(frames.end);
            pyramid.peak(from..to) > threshold
        };
        let buckets = frames.start / size..frames.end.div_ceil(size);
        let bucket = if last {
            buckets.rev().find(|bucket| loud(*bucket))?
        } else {
            buckets.into_iter().find(|bucket| loud(*bucket))?
        };
        let from = (bucket * size).max(frames.start);
        let to = ((bucket + 1) * size).min(frames.end);
        let Some((data, rates)) = &self.frames else {
            return Some(if last { to - 1 } else { from });
        };
        let channels = self.analysis.channels().min(2);
        let sounds = |frame: &u64| {
            let at = rates.loaded_frame(*frame) as usize;
            data.frames
                .get(at)
                .is_some_and(|both| both[..channels].iter().any(|s| s.abs() > threshold))
        };
        let found = if last {
            (from..to).rev().find(sounds)
        } else {
            (from..to).find(sounds)
        };
        // Converting the rate can soften an edge below the threshold, so the bucket still counts.
        Some(found.unwrap_or(if last { to - 1 } else { from }))
    }
}

impl SamplePage {
    /// The widest zoom: as many frames to a column as fit the whole sample across the view.
    pub fn fit(&self, length: u64) -> u64 {
        length.div_ceil(self.columns.max(1) as u64).max(1)
    }

    /// The column a frame is in, at the zoom shown.
    pub fn column_of(&self, frame: u64) -> u64 {
        frame / self.zoom.max(1)
    }

    /// The first frame shown.
    pub fn first_frame(&self) -> u64 {
        self.column * self.zoom
    }

    /// The cursor's column on screen, counted from the view's left, if it's in view.
    pub fn cursor_on_screen(&self) -> Option<usize> {
        let column = self.column_of(self.cursor).checked_sub(self.column)?;
        (column < self.columns as u64).then_some(column as usize)
    }

    /// Start afresh on a sample: the whole of it in view, and the cursor on its start.
    fn start_on(&mut self, shown: &Shown) {
        self.shown = Some((shown.number, shown.analysis.hash));
        self.zoom = self.fit(shown.length());
        self.column = 0;
        self.cursor = shown
            .entry
            .edit
            .points
            .start
            .min(shown.length().saturating_sub(1));
    }

    /// Keep the zoom, the view and the cursor inside the sample, as the window or the sample
    /// changes.
    fn settle(&mut self, length: u64) {
        let last = length.saturating_sub(1);
        self.cursor = self.cursor.min(last);
        let fit = self.fit(length);
        if self.zoom > fit || self.zoom == 0 {
            self.zoom = fit;
            self.column = 0;
        }
        self.clamp_view(length);
    }

    /// Keep the view from going past the sample's end, as far as the zoom lets it.
    fn clamp_view(&mut self, length: u64) {
        let last_column = self.column_of(length.saturating_sub(1));
        let most = last_column.saturating_sub(self.columns.saturating_sub(1) as u64);
        self.column = self.column.min(most);
    }

    /// Move the view so the cursor is in it. One that's left it is put in the middle, as Vim does
    /// with `sidescroll` at its default of 0, so holding `l` sends the waveform again every half
    /// screen rather than every column.
    fn follow(&mut self, length: u64) {
        if self.cursor_on_screen().is_none() {
            self.middle_on_cursor();
        }
        self.clamp_view(length);
    }

    fn middle_on_cursor(&mut self) {
        let half = (self.columns / 2) as u64;
        self.column = self.column_of(self.cursor).saturating_sub(half);
    }

    /// Put the cursor on a frame, inside the sample, and the view on it.
    fn go(&mut self, frame: u64, length: u64) {
        self.cursor = frame.min(length.saturating_sub(1));
        self.follow(length);
    }

    /// Zoom to this many frames a column, keeping the cursor in the same column on screen, so
    /// zooming in and back out comes back to the same view.
    fn zoom_to(&mut self, zoom: u64, length: u64) {
        if self.cursor_on_screen().is_none() {
            self.middle_on_cursor();
        }
        let on_screen = self.column_of(self.cursor) - self.column;
        self.zoom = zoom.clamp(1, self.fit(length));
        self.column = self.column_of(self.cursor).saturating_sub(on_screen);
        self.clamp_view(length);
    }

    /// The next zoom in: half as many frames a column, down to one. From the widest zoom, which
    /// needn't be a power of two, it's the power of two below it.
    pub fn zoomed_in(zoom: u64) -> u64 {
        if zoom <= 1 {
            1
        } else if zoom.is_power_of_two() {
            zoom / 2
        } else {
            1 << zoom.ilog2()
        }
    }

    /// The next zoom out: twice as many frames a column, up to the widest.
    pub fn zoomed_out(zoom: u64, fit: u64) -> u64 {
        zoom.saturating_mul(2).min(fit)
    }
}

impl App {
    /// The bank entry the cursor's track plays, with its analysis, or why the page can't show it.
    pub fn sample_shown(&self) -> Result<Shown, String> {
        let track = self.cursor.track;
        let number = self
            .song()
            .tracks
            .get(track)
            .map_or(0, |track| track.sample());
        let Some(entry) = self.song().bank.get(number) else {
            return Err(format!(
                "Track {:02} plays no sample. Space 5 chooses one",
                track + 1
            ));
        };
        let Some(source) = entry.source() else {
            return Err(format!("Sample {number:02X} has no file"));
        };
        let Some(analysis) = self.samples.analysis(source.hash) else {
            return Err(if source.hash.is_known() {
                format!("Analysing {}…", entry.title())
            } else {
                format!("{} can't be found, so it can't be shown", source.path)
            });
        };
        let frames = self.samples.id_of(&source.path).and_then(|id| {
            let data = self.samples.shared(id)?;
            Some((Arc::clone(data), self.samples.rates(id)))
        });
        Ok(Shown {
            number,
            entry: entry.clone(),
            analysis: Arc::clone(analysis),
            frames,
        })
    }

    /// Bring the page up to date with the sample the cursor's track plays, starting afresh if
    /// it's another, and say what it shows.
    pub fn sample_page_here(&mut self) -> Result<Shown, String> {
        let shown = self.sample_shown()?;
        let page = &mut self.sample_page;
        if page.shown != Some((shown.number, shown.analysis.hash)) {
            page.start_on(&shown);
        }
        page.settle(shown.length());
        Ok(shown)
    }

    /// Run a key on the sample page. Every motion is handled here, so none reaches the phrase.
    pub(crate) fn run_sample_page(&mut self, cmd: Cmd) -> bool {
        let shown = match self.sample_page_here() {
            Ok(shown) => Some(shown),
            Err(why) => {
                if is_sample_motion(cmd) {
                    self.status = why;
                    self.count = None;
                    return true;
                }
                None
            }
        };
        let Some(shown) = shown else {
            return false;
        };
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1) as u64;
        let length = shown.length();
        let beat = beat_frames(shown.rate(), self.song().millibpm);
        let page = &mut self.sample_page;
        let zoom = page.zoom;
        let columns = page.columns as u64;
        match cmd {
            Cmd::Move(Motion::Left) => page.go(page.cursor.saturating_sub(times * zoom), length),
            Cmd::Move(Motion::Right) => page.go(page.cursor.saturating_add(times * zoom), length),
            Cmd::Move(Motion::RowStart) => page.go(shown.slice_at(page.cursor).1.start, length),
            Cmd::Move(Motion::RowEnd) => {
                let slice = shown.slice_at(page.cursor).1;
                page.go(slice.end.saturating_sub(1).max(slice.start), length);
            }
            Cmd::Move(Motion::First) | Cmd::Move(Motion::Last) if count.is_some() => {
                let number = count.unwrap_or(0);
                page.go(shown.slice(number).start, length);
            }
            Cmd::Move(Motion::First) => page.go(0, length),
            Cmd::Move(Motion::Last) => page.go(length, length),
            Cmd::Move(Motion::Down) | Cmd::Move(Motion::Up) => {
                let (number, _) = shown.slice_at(page.cursor);
                let number = if cmd == Cmd::Move(Motion::Down) {
                    number.saturating_add(times as usize)
                } else {
                    number.saturating_sub(times as usize)
                };
                page.go(shown.slice(number).start, length);
            }
            Cmd::Move(Motion::NextBeat) | Cmd::Move(Motion::PreviousBeat) => {
                let later = cmd == Cmd::Move(Motion::NextBeat);
                let to = beat_from(
                    page.cursor,
                    shown.entry.edit.points.start,
                    beat,
                    times,
                    later,
                );
                page.go(to, length);
            }
            Cmd::ViewTop => {
                page.go((page.column + times - 1) * zoom, length);
            }
            Cmd::ViewMiddle => page.go((page.column + columns / 2) * zoom, length),
            Cmd::ViewBottom => {
                let back = (times - 1).min(columns - 1);
                page.go((page.column + columns - 1 - back) * zoom, length);
            }
            Cmd::HalfPageDown | Cmd::HalfPageUp => {
                let by = (columns / 2).max(1) * times;
                let later = cmd == Cmd::HalfPageDown;
                if later {
                    page.column += by;
                    page.cursor = page.cursor.saturating_add(by * zoom);
                } else {
                    page.column = page.column.saturating_sub(by);
                    page.cursor = page.cursor.saturating_sub(by * zoom);
                }
                page.clamp_view(length);
                page.go(page.cursor, length);
            }
            Cmd::ScrollDown | Cmd::ScrollUp => {
                if cmd == Cmd::ScrollDown {
                    page.column += times;
                } else {
                    page.column = page.column.saturating_sub(times);
                }
                page.clamp_view(length);
                // A cursor scrolled off the view goes to the nearest column still in it, keeping
                // its place in its column, as Vim keeps the cursor on screen.
                let within = page.cursor % zoom;
                let cursor_column = page.column_of(page.cursor);
                let column = cursor_column.clamp(page.column, page.column + columns - 1);
                if column != cursor_column {
                    page.cursor = (column * zoom + within).min(length.saturating_sub(1));
                }
            }
            Cmd::NarrowGrid => zoom_by(page, times, true, length),
            Cmd::WidenGrid => zoom_by(page, times, false, length),
            Cmd::Sample(key) => self.run_sample_key(key, &shown, times),
            Cmd::EnterInsert | Cmd::Replace | Cmd::Substitute => {
                self.status = "The sample page only shows the sample for now".to_owned();
            }
            Cmd::Operator(_) | Cmd::VisualField | Cmd::VisualStep | Cmd::Reselect => {
                self.status = "Selecting and editing the sample come later".to_owned();
            }
            Cmd::Increase
            | Cmd::Decrease
            | Cmd::IncreaseLarge
            | Cmd::DecreaseLarge
            | Cmd::IncreaseChromatic
            | Cmd::DecreaseChromatic
            | Cmd::ClearField
            | Cmd::CycleMark
            | Cmd::Paste
            | Cmd::PasteAfter
            | Cmd::PasteMerge
            | Cmd::Repeat => {
                self.status = "The sample page only shows the sample for now".to_owned();
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    fn run_sample_key(&mut self, key: SampleKey, shown: &Shown, times: u64) {
        let length = shown.length();
        let page = &mut self.sample_page;
        match key {
            SampleKey::FirstSound | SampleKey::LastSound => {
                let last = key == SampleKey::LastSound;
                let (number, slice) = shown.slice_at(page.cursor);
                match shown.sound_edge(slice, last) {
                    Some(frame) => page.go(frame, length),
                    None => self.status = format!("Slice {number:02X} is all silence"),
                }
            }
            SampleKey::NextPoint | SampleKey::PreviousPoint => {
                let points = shown.points();
                let cursor = page.cursor;
                let mut to = None;
                for _ in 0..times {
                    let from = to.unwrap_or(cursor);
                    let next = if key == SampleKey::NextPoint {
                        points.iter().find(|point| **point > from)
                    } else {
                        points.iter().rev().find(|point| **point < from)
                    };
                    match next {
                        Some(point) => to = Some(*point),
                        None => break,
                    }
                }
                match to {
                    Some(frame) => page.go(frame, length),
                    None => self.status = "No point further that way".to_owned(),
                }
            }
            SampleKey::CursorMiddle => {
                page.middle_on_cursor();
                page.clamp_view(length);
            }
            SampleKey::CursorLeft => {
                page.column = page.column_of(page.cursor);
                page.clamp_view(length);
            }
            SampleKey::CursorRight => {
                let back = page.columns.saturating_sub(1) as u64;
                page.column = page.column_of(page.cursor).saturating_sub(back);
                page.clamp_view(length);
            }
            SampleKey::ZoomIn => zoom_by(page, times, true, length),
            SampleKey::ZoomOut => zoom_by(page, times, false, length),
            SampleKey::ZoomAll => {
                page.zoom = page.fit(length);
                page.column = 0;
            }
            SampleKey::ZoomSlice => {
                let (_, slice) = shown.slice_at(page.cursor);
                let wanted = (slice.end - slice.start).div_ceil(page.columns.max(1) as u64);
                let fit = page.fit(length);
                let mut zoom = fit;
                while SamplePage::zoomed_in(zoom) >= wanted.max(1) && zoom > 1 {
                    zoom = SamplePage::zoomed_in(zoom);
                }
                page.zoom = zoom;
                // The slice's middle in the view's.
                let middle = (slice.start + slice.end) / 2;
                page.column = page
                    .column_of(middle)
                    .saturating_sub((page.columns / 2) as u64);
                page.clamp_view(length);
                if page.cursor_on_screen().is_none() {
                    page.cursor = slice.start.min(length.saturating_sub(1));
                }
            }
            SampleKey::CursorStyle => {
                page.cursor_line = !page.cursor_line;
                self.status = if page.cursor_line {
                    "The cursor is a line at its frame: zc for its column".to_owned()
                } else {
                    "The cursor is its column: zc for a line at its frame".to_owned()
                };
            }
        }
    }
}

/// Zoom in or out by a factor of two this many times.
fn zoom_by(page: &mut SamplePage, times: u64, closer: bool, length: u64) {
    let fit = page.fit(length);
    let mut zoom = page.zoom;
    for _ in 0..times.min(64) {
        zoom = if closer {
            SamplePage::zoomed_in(zoom)
        } else {
            SamplePage::zoomed_out(zoom, fit)
        };
    }
    page.zoom_to(zoom, length);
}

/// How many of a file's frames a beat lasts at a tempo, in thousandths of a beat a minute.
pub fn beat_frames(rate: u32, millibpm: u32) -> f64 {
    f64::from(rate) * 60_000.0 / f64::from(millibpm.max(1))
}

/// The beat `times` beats on from a frame, or back, on a grid of beats from the entry's start.
fn beat_from(frame: u64, start: u64, beat: f64, times: u64, later: bool) -> u64 {
    let beats = (frame as f64 - start as f64) / beat;
    // Off the grid, the first beat either way is the nearest one that way.
    let on = if later {
        beats.floor() + times as f64
    } else {
        beats.ceil() - times as f64
    };
    (start as f64 + on * beat).round().max(0.0) as u64
}

/// Whether a key moves over the sample, so it means nothing while there's no sample to show.
fn is_sample_motion(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Move(_)
            | Cmd::ViewTop
            | Cmd::ViewMiddle
            | Cmd::ViewBottom
            | Cmd::HalfPageDown
            | Cmd::HalfPageUp
            | Cmd::ScrollDown
            | Cmd::ScrollUp
            | Cmd::NarrowGrid
            | Cmd::WidenGrid
            | Cmd::Sample(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooming_in_halves_to_one_and_out_doubles_to_the_whole_sample() {
        assert_eq!(SamplePage::zoomed_in(1024), 512);
        assert_eq!(SamplePage::zoomed_in(1), 1);
        // From the widest, which needn't be a power of two, to the power of two below it.
        assert_eq!(SamplePage::zoomed_in(89_410), 65_536);
        assert_eq!(SamplePage::zoomed_out(65_536, 89_410), 89_410);
        assert_eq!(SamplePage::zoomed_out(512, 89_410), 1024);
    }

    #[test]
    fn a_beat_on_lands_on_the_grid_from_the_start() {
        // 120 BPM at 48 kHz: a beat is 24 000 frames.
        let beat = beat_frames(48_000, 120_000);
        assert_eq!(beat, 24_000.0);
        assert_eq!(beat_from(1000, 1000, beat, 1, true), 25_000);
        assert_eq!(beat_from(1001, 1000, beat, 1, true), 25_000);
        assert_eq!(beat_from(1001, 1000, beat, 1, false), 1000);
        assert_eq!(beat_from(25_000, 1000, beat, 2, false), 0);
        assert_eq!(beat_from(30_000, 1000, beat, 3, true), 97_000);
    }
}
