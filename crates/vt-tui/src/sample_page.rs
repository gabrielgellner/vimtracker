//! The sample page, `Space 0`: the sample the cursor's track plays, as a big waveform to move over
//! with Vim's motions. A sample is a buffer and a cell column is a character, but the cursor is a
//! line placed to the pixel, so `h` and `l` move a pixel at the zoom shown, speeding up to a column
//! as they're held, `0` and `$` go to the slice's ends, and `zi` and `zo` zoom on the cursor.
//! `placing.rs` edits the entry's points from here, `sample_settings.rs` its settings, and `v`
//! selects frames to hear.
//!
//! Everything here is in frames at the file's own rate, as an entry's points are, so the cursor
//! stays on the same sound whatever rate the program plays at.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vt_model::analysis::{Analysis, Pyramid, SILENCE_DB};
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::sample::SampleData;
use vt_model::samples::Rates;
use vt_model::selection::Motion;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::marking::MarkWait;
use crate::placing::{Edge, Nudge};
use crate::sample_settings::SampleSetting;
use crate::views::sample::WaveOnScreen;

/// How many columns the waveform has until the page is first drawn: the author's screen, 163
/// characters across, less a column either side.
pub const DEFAULT_COLUMNS: usize = 161;

/// How soon after a step a press of the same key still counts as the terminal repeating it, as
/// kitty on macOS has sent a held key's repeats as presses.
pub const REPEAT_PRESS: Duration = Duration::from_millis(60);

/// How long a held `h` or `l` can go between repeats and keep its speed. A longer gap starts it
/// again from a pixel.
pub const REPEAT_GAP: Duration = Duration::from_millis(250);

/// How long a held `h` or `l` takes to build from a pixel a repeat to a column, counted from its
/// first repeat.
pub const SPEED_UP: Duration = Duration::from_millis(500);

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
    /// `'` and a letter: the slice the letter names, as a jump.
    Mark,
    /// `]'` and `['`: the next or previous slice a letter names, as a jump.
    NextMark,
    PreviousMark,
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
    /// `m` and a letter: name the slice under the cursor with it.
    NameSlice,
    /// `Enter`: split the slice at the cursor, or at the playhead while one plays.
    Split,
    /// `J`: join the slice under the cursor with the next.
    Join,
    /// Cmd-l and Cmd-h: nudge the point under the cursor a frame later or earlier.
    NudgeFrame {
        later: bool,
    },
    /// `g s`, `g e`, `g l` and `g L`: put the start, the end or a loop point at the cursor.
    SetPoint(Edge),
    /// `o`: the cursor to the selection's other end, as Vim's `o` in Visual mode.
    OtherEnd,
    /// `>` and `<`: the slice's gain up or down a dB.
    GainUp,
    GainDown,
    /// `~`: the slice played the other way.
    Reverse,
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
    /// A bank entry shown in place of a track's own, while that track is the cursor's, as `g d`
    /// on a step's `SLI` under an `SMP` of its own shows the sample the step plays.
    pub pinned: Option<(usize, u8)>,
    /// A character's width in pixels, as the waveform and the cursor's line are drawn for, so `h`
    /// and `l` can move the line a pixel. None until the pictures are drawn, as in a render, where
    /// they move a column.
    pub cell_width: Option<usize>,
    /// The `h` or `l` being held, if one is.
    held_step: Option<HeldStep>,
    /// The bank entry and content the page last played, while what it played is the one-off
    /// file the preview voices have, so its playhead is shown over this entry and no other.
    pub heard: Option<(u8, ContentHash)>,
    /// How many times the page has played something, so a key let go stops only what it started.
    pub hearings: u64,
    /// The slice the keys mode's first key plays, a multiple of [`PADS`](crate::auditioning::PADS).
    pub pads_from: usize,
    /// Where a selection of frames started, while `v` is selecting: it reaches from there to the
    /// cursor, both included.
    pub anchor: Option<u64>,
    /// The first and last frame of the last selection, for `'<`, `'>` and `gv`.
    pub last_selection: Option<(u64, u64)>,
    /// The setting the settings rows have the focus on, if they have it rather than the waveform.
    pub setting: Option<SampleSetting>,
}

/// A run of `h` or `l` as it's held, which speeds up from a pixel a repeat to a column, as an
/// encoder's steps grow the faster it's turned on the Polyend Tracker and the Deluge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldStep {
    /// Whether it's `l` rather than `h`.
    later: bool,
    /// When it last stepped.
    last: Instant,
    /// When its first repeat came, from which it speeds up.
    repeating_since: Option<Instant>,
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
            cell_width: None,
            held_step: None,
            pinned: None,
            heard: None,
            hearings: 0,
            pads_from: 0,
            anchor: None,
            last_selection: None,
            setting: None,
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
        self.entry.edit.points.end_in(self.length())
    }

    /// Where each slice starts, in order: the entry's start alone, as one slice, if it has none.
    pub fn slice_starts(&self) -> Vec<u64> {
        let points = &self.entry.edit.points;
        if points.slices.is_empty() {
            return vec![points.start.min(self.length())];
        }
        points
            .slices
            .iter()
            .map(|slice| slice.start.min(self.length()))
            .collect()
    }

    /// How many slices it has to choose from: the entry as one, if it has none.
    pub fn slice_count(&self) -> usize {
        self.entry.edit.points.slice_count()
    }

    /// The slice a frame is in, by its number, and the frames it covers. A frame before the first
    /// slice is in the first.
    pub fn slice_at(&self, frame: u64) -> (usize, std::ops::Range<u64>) {
        let number = self.entry.edit.points.slice_at(frame);
        (number, self.slice(number))
    }

    /// The frames of a slice by its number, clamped to the last: from its start to the next one's
    /// or the entry's end, as it plays.
    pub fn slice(&self, number: usize) -> std::ops::Range<u64> {
        let points = &self.entry.edit.points;
        if points.slices.is_empty() {
            let start = points.start.min(self.length());
            return start..self.end().max(start);
        }
        let number = number.min(points.slices.len() - 1);
        let start = points.slices[number].start.min(self.length());
        let end = points
            .slices
            .get(number + 1)
            .map_or(self.end(), |next| next.start.min(self.length()));
        start..end.max(start)
    }

    /// Every point, in order: the start, the end, the loop's, and where each slice starts.
    pub fn points(&self) -> Vec<u64> {
        let Points {
            start,
            end,
            loop_,
            slices,
            ..
        } = &self.entry.edit.points;
        let mut points = vec![*start];
        points.extend(end);
        if let Some(looped) = loop_ {
            points.extend([looped.start, looped.end]);
        }
        points.extend(slices.iter().map(|slice| slice.start));
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
        self.on_screen_column(self.cursor)
    }

    /// A frame's column on screen, counted from the view's left, if it's in view.
    pub fn on_screen_column(&self, frame: u64) -> Option<usize> {
        let column = self.column_of(frame).checked_sub(self.column)?;
        (column < self.columns as u64).then_some(column as usize)
    }

    /// The frames selected, the anchor's and the cursor's included, while `v` is selecting.
    pub fn selection(&self) -> Option<std::ops::Range<u64>> {
        let anchor = self.anchor?;
        Some(anchor.min(self.cursor)..anchor.max(self.cursor) + 1)
    }

    /// End a selection, keeping its ends for `'<`, `'>` and `gv`.
    pub fn end_selection(&mut self) {
        if let Some(selected) = self.selection() {
            self.last_selection = Some((selected.start, selected.end - 1));
        }
        self.anchor = None;
    }

    /// Start afresh on a sample: the whole of it in view, and the cursor on its start.
    fn start_on(&mut self, shown: &Shown) {
        self.shown = Some((shown.number, shown.analysis.hash));
        self.zoom = self.fit(shown.length());
        self.column = 0;
        self.pads_from = 0;
        self.anchor = None;
        self.last_selection = None;
        self.setting = None;
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

    /// Move the cursor for `h` or `l`: by a pixel at the zoom shown, or `times` pixels with a count,
    /// and as a key is held, by more each repeat, up to a column. Closer than a frame a pixel, a
    /// pixel is a frame. As its column's background, or with no pictures drawn, the cursor can only
    /// show columns, so it moves a column.
    fn step(
        &mut self,
        later: bool,
        count: Option<usize>,
        repeating: bool,
        now: Instant,
        length: u64,
    ) {
        let times = count.unwrap_or(1).max(1) as u64;
        let held = self.held_step.take();
        let zoom = self.zoom.max(1);
        let cell = match self.cell_width {
            Some(cell) if self.cursor_line && cell > 1 => cell as u64,
            _ => {
                let by = times.saturating_mul(zoom);
                let to = if later {
                    self.cursor.saturating_add(by)
                } else {
                    self.cursor.saturating_sub(by)
                };
                self.go(to, length);
                return;
            }
        };
        let pixels = if count.is_some() {
            1
        } else {
            // A repeat, or a press of the same key straight after, carries the run on.
            let carried = held.filter(|held| {
                let since = now.saturating_duration_since(held.last);
                held.later == later && since <= REPEAT_GAP && (repeating || since <= REPEAT_PRESS)
            });
            let repeating_since = carried.map(|held| held.repeating_since.unwrap_or(now));
            self.held_step = Some(HeldStep {
                later,
                last: now,
                repeating_since,
            });
            repeating_since.map_or(1, |since| {
                held_pixels(cell, now.saturating_duration_since(since))
            })
        };
        let to = if zoom < cell {
            // Several pixels to a frame: a frame a step, or a count of them, or as many as a
            // held key's pixels cover, so it still builds to a column.
            let frames =
                (u128::from(pixels) * u128::from(zoom) + u128::from(cell / 2)) / u128::from(cell);
            let by = times.saturating_mul((frames as u64).max(1));
            if later {
                self.cursor.saturating_add(by)
            } else {
                self.cursor.saturating_sub(by)
            }
        } else {
            // To the first frame of the pixel that many along, so each step moves the line by
            // exactly that many pixels, wherever in its pixel the cursor was.
            let pixel = u128::from(self.cursor) * u128::from(cell) / u128::from(zoom);
            let by = u128::from(times.saturating_mul(pixels));
            let pixel = if later {
                pixel.saturating_add(by)
            } else {
                pixel.saturating_sub(by)
            };
            (pixel * u128::from(zoom))
                .div_ceil(u128::from(cell))
                .min(u128::from(u64::MAX)) as u64
        };
        self.go(to, length);
    }

    /// Put the cursor on a frame with the view in the middle of it, as a jump to it lands.
    pub fn show_frame(&mut self, frame: u64, length: u64) {
        self.cursor = frame.min(length.saturating_sub(1));
        self.middle_on_cursor();
        self.clamp_view(length);
    }

    /// Put the cursor on a frame, inside the sample, and the view on it.
    pub(crate) fn go(&mut self, frame: u64, length: u64) {
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
    /// The bank entry the cursor's track plays, or one a jump chose for it, with its analysis, or
    /// why the page can't show it.
    pub fn sample_shown(&self) -> Result<Shown, String> {
        let track = self.cursor.track;
        let number = match self.sample_page.pinned {
            Some((pinned, number)) if pinned == track => number,
            _ => self
                .song()
                .tracks
                .get(track)
                .map_or(0, |track| track.sample()),
        };
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

    /// The sample page's own modes, as the status line's badge names them: `PADS` for the keys
    /// mode, `VISUAL` while selecting, and `SETTINGS` with the focus on the settings rows, where
    /// `h` and `l` go along the rows rather than the waveform. None on the waveform.
    pub fn sample_mode(&self) -> Option<&'static str> {
        if self.view != View::Sample {
            return None;
        }
        if self.mode == Mode::Keys {
            Some("PADS")
        } else if self.sample_page.anchor.is_some() {
            Some(Mode::VisualField.label())
        } else if self.sample_page.setting.is_some() {
            Some("SETTINGS")
        } else {
            None
        }
    }

    /// What the keys do in the sample page's mode, and how to leave it, for under the status
    /// line while nothing else is said there, so a mode is never on without a word.
    pub fn sample_mode_hint(&self) -> Option<String> {
        if self.view != View::Sample {
            return None;
        }
        if self.mode == Mode::Keys {
            let shown = self.sample_shown().ok()?;
            return Some(self.pads_hint(&shown));
        }
        if self.sample_page.anchor.is_some() {
            return Some(
                "; plays the selection, o goes to its other end; v or Esc ends it".to_owned(),
            );
        }
        let setting = self.sample_page.setting?;
        Some(format!(
            "{}: = - + _ change it, K lists its values, x puts it back, h l the setting before or \
             after; Esc goes back to the waveform",
            setting.name()
        ))
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
        let (repeating, now) = (self.repeating, self.now);
        // With the focus on the settings rows, `h` and `l` go along them and the keys that change
        // values change the setting rather than a point.
        if let Some(setting) = self.sample_page.setting {
            let large = matches!(cmd, Cmd::IncreaseLarge | Cmd::DecreaseLarge);
            let up = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
            match cmd {
                Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right) => {
                    let later = cmd == Cmd::Move(Motion::Right);
                    self.move_setting(&shown, later, times as usize);
                    return true;
                }
                Cmd::Move(Motion::RowStart) | Cmd::Move(Motion::RowEnd) => {
                    let later = cmd == Cmd::Move(Motion::RowEnd);
                    self.move_setting(&shown, later, usize::MAX);
                    return true;
                }
                Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                    self.change_setting(&shown, setting, up, large, times);
                    return true;
                }
                Cmd::Sample(SampleKey::NudgeFrame { later }) => {
                    self.change_setting(&shown, setting, later, false, times);
                    return true;
                }
                Cmd::ClearField => {
                    self.reset_setting(&shown, setting);
                    return true;
                }
                Cmd::PickValue => {
                    self.open_setting_values_picker(&shown, setting);
                    return true;
                }
                _ => {}
            }
        }
        let page = &mut self.sample_page;
        let zoom = page.zoom;
        let columns = page.columns as u64;
        // Any other key ends a held `h` or `l`'s run.
        if !matches!(cmd, Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right)) {
            page.held_step = None;
        }
        match cmd {
            Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right) => {
                let later = cmd == Cmd::Move(Motion::Right);
                page.step(later, count, repeating, now, length);
            }
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
            Cmd::Audition => match page.selection() {
                Some(selected) => self.audition_frames(&shown, selected),
                None => self.audition_slice(&shown),
            },
            Cmd::PlayFromCursor => self.audition_from_cursor(&shown),
            Cmd::EnterInsert => self.start_pads(),
            Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                let by = if matches!(cmd, Cmd::Increase | Cmd::Decrease) {
                    Nudge::Pixel
                } else {
                    Nudge::Column
                };
                let later = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
                self.nudge_point(&shown, by, later, times);
            }
            Cmd::ClearField => self.remove_point(&shown),
            Cmd::PickValue => self.open_settings_picker(&shown),
            Cmd::VisualField => {
                if page.anchor.is_some() {
                    page.end_selection();
                } else {
                    page.anchor = Some(page.cursor);
                }
            }
            Cmd::Reselect => match page.last_selection {
                Some((first, last)) => {
                    page.anchor = Some(first);
                    page.go(last, length);
                }
                None => self.status = "No selection yet: v starts one".to_owned(),
            },
            Cmd::VisualStep | Cmd::Operator(_) => {
                self.status = "Whole slices and the operators come later: v selects frames to hear"
                    .to_owned();
            }
            Cmd::Replace
            | Cmd::Substitute
            | Cmd::IncreaseChromatic
            | Cmd::DecreaseChromatic
            | Cmd::CycleMark
            | Cmd::Paste
            | Cmd::PasteAfter
            | Cmd::PasteMerge
            | Cmd::Repeat => {
                self.status = "That doesn't work on the sample page yet".to_owned();
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
            SampleKey::Mark => self.awaiting_mark = Some(MarkWait::Jump),
            SampleKey::NameSlice => self.awaiting_mark = Some(MarkWait::Name),
            SampleKey::Split => self.split_here(shown),
            SampleKey::Join => self.join_here(shown),
            SampleKey::NudgeFrame { later } => {
                self.nudge_point(shown, Nudge::Frame, later, times);
            }
            SampleKey::SetPoint(edge) => self.set_point(shown, edge),
            SampleKey::OtherEnd => match page.anchor {
                Some(anchor) => {
                    page.anchor = Some(page.cursor);
                    page.go(anchor, length);
                }
                None => self.status = "o swaps a selection's ends: v starts one".to_owned(),
            },
            SampleKey::GainUp | SampleKey::GainDown => {
                let up = key == SampleKey::GainUp;
                self.change_setting(shown, SampleSetting::Gain, up, false, times);
            }
            SampleKey::Reverse => self.reverse_here(shown),
            SampleKey::NextMark | SampleKey::PreviousMark => {
                self.jump_to_next_mark(key == SampleKey::NextMark, times as usize);
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

/// How many pixels a held `h` or `l` moves each repeat, this long after its first repeat: one at
/// first, building on a square's curve to a character's width, a column, after [`SPEED_UP`], so it
/// creeps for fine placing and then crosses the sample.
pub fn held_pixels(cell: u64, held: Duration) -> u64 {
    let along = (held.as_secs_f64() / SPEED_UP.as_secs_f64()).min(1.0);
    1 + (cell.saturating_sub(1) as f64 * along * along).round() as u64
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
            | Cmd::Audition
            | Cmd::PlayFromCursor
            | Cmd::EnterInsert
            | Cmd::Increase
            | Cmd::Decrease
            | Cmd::IncreaseLarge
            | Cmd::DecreaseLarge
            | Cmd::ClearField
            | Cmd::VisualField
            | Cmd::Reselect
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
