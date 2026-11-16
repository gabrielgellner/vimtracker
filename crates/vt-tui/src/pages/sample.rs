//! The sample page, `Space 0`: the sample the cursor's track plays, as a big waveform to move over
//! with Vim's motions. A sample is a buffer and a cell column is a character, but the cursor is a
//! line placed to the pixel, so `h` and `l` move a pixel at the zoom shown, speeding up to a column
//! as they're held, `0` and `$` go to the slice's ends, `w` `b` and `e` go by sound as Vim's go by
//! word, and `zi` and `zo` zoom on the cursor.
//! `placing` edits the entry's points from here, `settings` its settings, and `v`
//! selects frames to hear.
//!
//! `SamplePage` is the page as a component: its cursor, zoom, view, selection and the settings'
//! focus, the pictures it shows, its moves and its edits of the entry, through the [`Services`]'
//! actions, what it plays, in `auditioning`, and its drawing, in `draw`, given a [`Ctx`]. What only
//! `App` can do its keys ask for in their [`Outcome`]: the picker, the pads' keys mode, and what
//! plays held while the key is down. The marks' letters and jumps are `App`'s, as they wait for a
//! letter and go on the jump list, so the page passes them back. It's a [`Page`], with its rows of
//! the binding table, [`Page::KEYS`], here beside it.
//!
//! Everything here is in frames at the file's own rate, as an entry's points are, so the cursor
//! stays on the same sound whatever rate the program plays at.

pub(crate) mod auditioning;
pub mod draw;
pub(crate) mod marking;
pub(crate) mod placing;
pub(crate) mod points;
pub mod settings;
pub(crate) mod slicing;
pub mod wave;
pub(crate) mod waveform;

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_engine::engine::Playhead;
use vt_model::analysis::{Analysis, Pyramid, SILENCE_DB};
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::onsets::decay;
use vt_model::sample::SampleData;
use vt_model::samples::{Rates, SamplePool};
use vt_model::selection::Motion;
use vt_model::song::Song;

use crate::graphics::plot::Shade;
use crate::graphics::{PanelIds, PanelKey, PanelWant, Shape};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::pages::sample::draw::{WAVE_ROWS, WaveOnScreen, page_size, wave_columns};
use crate::pages::sample::placing::{Edge, Nudge};
use crate::pages::sample::settings::SampleSetting;
use crate::pages::sample::wave::{Wave, drawn_points};
use crate::pages::{Ctx, CtxMut, Outcome, Page};
use crate::services::Services;
use crate::services::sampling::still_recording;

/// How many columns the waveform has until the page is first drawn: the author's screen, 163
/// characters across, less a column either side.
pub(crate) const DEFAULT_COLUMNS: usize = 161;

/// How soon after a step a press of the same key still counts as the terminal repeating it, as
/// kitty on macOS has sent a held key's repeats as presses.
pub(crate) const REPEAT_PRESS: Duration = Duration::from_millis(60);

/// How long a held `h` or `l` can go between repeats and keep its speed. A longer gap starts it
/// again from a pixel.
pub(crate) const REPEAT_GAP: Duration = Duration::from_millis(250);

/// How long a held `h` or `l` takes to build from a pixel a repeat to a column, counted from its
/// first repeat.
pub(crate) const SPEED_UP: Duration = Duration::from_millis(500);

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
    /// `w` and `b`: the next or previous sound's start, its onset, as `Sens` finds them, as Vim's
    /// go by word.
    NextSound,
    PreviousSound,
    /// `e`: where the sound decays, or the next one's, as Vim's goes to a word's end.
    SoundEnd,
    /// `is` and `ip` while selecting: this sound, from its onset to where it decays, and this
    /// slice, as Vim's text objects are a word and a paragraph.
    ThisSound,
    ThisSlice,
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

/// The sample page as a component: where it is, kept while other views are shown, and started
/// afresh for another sample, and the pictures it shows. What it shows is the bank entry the
/// cursor's track plays, which it reads from the song each time, holding none of its own. It's
/// drawn with [`Page::draw`], given the rest of the program as a [`Ctx`].
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
    /// How many frames long what the page last settled on is, so it settles again when it's laid
    /// out for other room, or none while it shows nothing.
    settled: Option<u64>,
    /// Whether the cursor is a thin line at its frame rather than its column's background.
    pub(crate) cursor_line: bool,
    /// Where the waveform was last drawn on screen, if it was.
    pub(crate) on_screen: Option<WaveOnScreen>,
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
    pub(crate) hearings: u64,
    /// The slice the keys mode's first key plays, a multiple of [`PADS`](auditioning::PADS).
    pub pads_from: usize,
    /// Where a selection of frames started, while `v` is selecting: it reaches from there to the
    /// cursor, both included.
    pub anchor: Option<u64>,
    /// The first and last frame of the last selection, for `'<`, `'>` and `gv`.
    pub(crate) last_selection: Option<(u64, u64)>,
    /// The setting the settings rows have the focus on, if they have it rather than the waveform.
    pub setting: Option<SampleSetting>,
    /// The ids of the pictures of the waveform and its overview, for the drawing to put
    /// placeholders for, or none while they aren't drawn, as in a render.
    pub images: PanelIds,
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
            settled: None,
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
            images: PanelIds::default(),
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
    /// A bank entry by its number, with its analysis and frames as the pool holds them, or why it
    /// can't be shown: it has no file, or its file is still being analysed or can't be found.
    pub fn of(samples: &SamplePool, number: u8, entry: &BankEntry) -> Result<Self, String> {
        let Some(source) = entry.source() else {
            return Err(format!("Sample {number:02X} has no file"));
        };
        let Some(analysis) = samples.analysis(source.hash) else {
            return Err(if source.hash.is_known() {
                format!("Analysing {}…", entry.title())
            } else {
                format!("{} can't be found, so it can't be shown", source.path)
            });
        };
        let frames = samples.id_of(&source.path).and_then(|id| {
            let data = samples.shared(id)?;
            Some((Arc::clone(data), samples.rates(id)))
        });
        Ok(Self {
            number,
            entry: entry.clone(),
            analysis: Arc::clone(analysis),
            frames,
        })
    }

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

    /// Where the sounds start that the entry's `Sens` finds, in order.
    pub fn onsets(&self) -> Vec<u64> {
        self.analysis.onsets(self.entry.edit.hints.sens)
    }

    /// The frames of the sound starting at one of these onsets, by its place among them: from it
    /// to where it decays, before the next one starts.
    pub fn sound(&self, onsets: &[u64], at: usize) -> std::ops::Range<u64> {
        let start = onsets[at];
        let next = onsets.get(at + 1).copied().unwrap_or(self.length());
        start..decay(&self.analysis.pyramid, start, next).max(start + 1)
    }

    /// The place among these onsets of the sound a frame is in: the last to start at or before
    /// it, or the first if none has.
    pub(crate) fn sound_at(onsets: &[u64], frame: u64) -> usize {
        onsets
            .iter()
            .rposition(|onset| *onset <= frame)
            .unwrap_or(0)
    }

    /// The first frame of a stretch louder than a level in any channel, as the pool holds the
    /// file's frames, if any is.
    pub(crate) fn first_above(&self, frames: std::ops::Range<u64>, level: f32) -> Option<u64> {
        let (data, rates) = self.frames.as_ref()?;
        let channels = self.analysis.channels().min(2);
        frames.into_iter().find(|frame| {
            let at = rates.loaded_frame(*frame) as usize;
            data.frames
                .get(at)
                .is_some_and(|both| both[..channels].iter().any(|s| s.abs() > level))
        })
    }

    /// The first or last frame of a stretch whose sound is above [`SILENCE_DB`], in any channel.
    /// The pyramid finds the first bucket with sound in it, and the frames find where in it.
    pub(crate) fn sound_edge(&self, frames: std::ops::Range<u64>, last: bool) -> Option<u64> {
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
    /// The page's text objects, while it selects: a sound is a word, and a slice a paragraph.
    /// They're its rows of the binding table beside [`Page::KEYS`], under its Visual mode.
    pub(crate) const OBJECTS: &'static [(&'static str, Cmd, &'static str)] = &[
        (
            "i s",
            Cmd::Sample(SampleKey::ThisSound),
            "This sound, from its start to where it decays",
        ),
        ("i p", Cmd::Sample(SampleKey::ThisSlice), "This slice"),
    ];

    /// The widest zoom: as many frames to a column as fit the whole sample across the view.
    pub fn fit(&self, length: u64) -> u64 {
        length.div_ceil(self.columns.max(1) as u64).max(1)
    }

    /// The column a frame is in, at the zoom shown.
    pub(crate) fn column_of(&self, frame: u64) -> u64 {
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
    pub(crate) fn on_screen_column(&self, frame: u64) -> Option<usize> {
        let column = self.column_of(frame).checked_sub(self.column)?;
        (column < self.columns as u64).then_some(column as usize)
    }

    /// The frames selected, the anchor's and the cursor's included, while `v` is selecting.
    pub fn selection(&self) -> Option<std::ops::Range<u64>> {
        let anchor = self.anchor?;
        Some(anchor.min(self.cursor)..anchor.max(self.cursor) + 1)
    }

    /// End a selection, keeping its ends for `'<`, `'>` and `gv`.
    pub(crate) fn end_selection(&mut self) {
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
    pub(crate) fn zoomed_in(zoom: u64) -> u64 {
        if zoom <= 1 {
            1
        } else if zoom.is_power_of_two() {
            zoom / 2
        } else {
            1 << zoom.ilog2()
        }
    }

    /// The next zoom out: twice as many frames a column, up to the widest.
    pub(crate) fn zoomed_out(zoom: u64, fit: u64) -> u64 {
        zoom.saturating_mul(2).min(fit)
    }
}

impl SamplePage {
    /// The bank entry the page shows while a track is the cursor's: one a jump chose for it, or
    /// the one it plays, by its number, or why there's none.
    pub(crate) fn entry_here<'a>(
        &self,
        song: &'a Song,
        track: usize,
    ) -> Result<(u8, &'a BankEntry), String> {
        let number = match self.pinned {
            Some((pinned, number)) if pinned == track => number,
            _ => song.tracks.get(track).map_or(0, |track| track.sample()),
        };
        match song.bank.get(number) {
            Some(entry) => Ok((number, entry)),
            None => Err(format!(
                "Track {:02} plays no sample. :sample chooses one",
                track + 1
            )),
        }
    }

    /// What the page shows for the cursor's track, with its analysis, or why it can't show it.
    /// An entry still recording isn't shown either, which the program, whose recording it is,
    /// says first, in [`App::sample_shown`].
    pub fn shown(&self, services: &Services) -> Result<Shown, String> {
        let (number, entry) = self.entry_here(services.song(), services.cursor.track)?;
        Shown::of(&services.samples, number, entry)
    }

    /// What the page shows for the cursor's track, or why it can't show it, with the recording
    /// as `App` lends it: the entry the page is on, which a jump may have chosen, unless it's
    /// still recording.
    pub(crate) fn shown_while(
        &self,
        services: &Services,
        recording: Option<(u8, Option<f64>)>,
    ) -> Result<Shown, String> {
        let (number, entry) = self.entry_here(services.song(), services.cursor.track)?;
        if let Some(why) = still_recording(recording, number) {
            return Err(why);
        }
        Shown::of(&services.samples, number, entry)
    }

    /// The page's own modes, as the status line's badge names them: `PADS` for the keys mode,
    /// `VISUAL` while selecting, and `SETTINGS` with the focus on the settings rows, where `h` and
    /// `l` go along the rows rather than the waveform. None on the waveform, or off the page.
    pub(crate) fn badge(&self, view: View, mode: Mode) -> Option<&'static str> {
        if view != View::Sample {
            return None;
        }
        if mode == Mode::Keys {
            Some("PADS")
        } else if self.anchor.is_some() {
            Some(Mode::VisualField.label())
        } else if self.setting.is_some() {
            Some("SETTINGS")
        } else {
            None
        }
    }

    /// Bring the page up to date with what it shows, starting afresh if it's another sample.
    pub(crate) fn settle_on(&mut self, shown: &Shown) {
        if self.shown != Some((shown.number, shown.analysis.hash)) {
            self.start_on(shown);
        }
        self.settle(shown.length());
        self.settled = Some(shown.length());
    }

    /// Bring the page up to date with what it shows, or with there being nothing to show.
    pub(crate) fn settle_here(&mut self, shown: &Result<Shown, String>) {
        match shown {
            Ok(shown) => self.settle_on(shown),
            Err(_) => self.settled = None,
        }
    }

    /// Lay the page out for this much room, so the waveform has as many columns as it's drawn
    /// with, and what it shows settles inside them.
    pub(crate) fn lay_out(&mut self, area: Rect) {
        let columns = wave_columns(page_size(area).0);
        if columns == self.columns {
            return;
        }
        self.columns = columns;
        if let Some(length) = self.settled {
            self.settle(length);
        }
    }

    /// The ids of the waveform's picture and the overview's, while the terminal has both.
    pub(crate) fn wave_images(&self) -> Option<(u32, u32)> {
        let images = &self.images;
        images
            .get(PanelKey::SampleWave)
            .zip(images.get(PanelKey::SampleOverview))
    }

    /// Where the voices playing the entry shown are, in its file's frames, for the playheads: the
    /// track's own notes of it, any of its notes on the preview voices, and what the page itself
    /// played, of the playheads the engine last reported.
    pub fn playheads(&self, shown: &Shown, heads: &[Option<Playhead>]) -> Vec<u64> {
        let Some((_, rates)) = &shown.frames else {
            return Vec::new();
        };
        let heard = self.heard == Some((shown.number, shown.analysis.hash));
        heads
            .iter()
            .flatten()
            .filter(|head| match head.entry {
                Some(number) => number == shown.number,
                None => heard,
            })
            .map(|head| rates.file_frame(head.frame))
            .collect()
    }

    /// Run one of the page's own keys on what it shows, `times` over, at least once, with `count`
    /// the count as typed, as `gg` and `G` go to the slice it names: a move, a jump by sound or
    /// point, a zoom, a selection, or an edit of a point or a setting, through the services'
    /// actions. What plays, the pads, the marks' jumps and the picker are passed back, as the
    /// program's. Every motion is handled here, so none reaches the phrase.
    pub fn key_on(
        &mut self,
        cmd: Cmd,
        times: u64,
        count: Option<usize>,
        shown: &Shown,
        services: &mut Services,
    ) -> Outcome {
        let length = shown.length();
        let beat = beat_frames(shown.rate(), services.song().millibpm);
        let (repeating, now) = (services.repeating, services.now);
        // With the focus on the settings rows, `h` and `l` go along them and the keys that change
        // values change the setting rather than a point.
        if let Some(setting) = self.setting {
            let large = matches!(cmd, Cmd::IncreaseLarge | Cmd::DecreaseLarge);
            let up = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
            match cmd {
                Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right) => {
                    let later = cmd == Cmd::Move(Motion::Right);
                    self.move_setting(shown, later, times as usize, services);
                    return Outcome::Done;
                }
                Cmd::Move(Motion::RowStart) | Cmd::Move(Motion::RowEnd) => {
                    let later = cmd == Cmd::Move(Motion::RowEnd);
                    self.move_setting(shown, later, usize::MAX, services);
                    return Outcome::Done;
                }
                Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                    self.change_setting(shown, setting, up, large, times, services);
                    return Outcome::Done;
                }
                Cmd::Sample(SampleKey::NudgeFrame { later }) => {
                    self.change_setting(shown, setting, later, false, times, services);
                    return Outcome::Done;
                }
                Cmd::ClearField => {
                    self.reset_setting(shown, setting, services);
                    return Outcome::Done;
                }
                // Its values are listed in the program's picker.
                Cmd::PickValue => return Outcome::Passed,
                _ => {}
            }
        }
        let zoom = self.zoom;
        let columns = self.columns as u64;
        // Any other key ends a held `h` or `l`'s run.
        if !matches!(cmd, Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right)) {
            self.held_step = None;
        }
        match cmd {
            Cmd::Move(Motion::Left) | Cmd::Move(Motion::Right) => {
                let later = cmd == Cmd::Move(Motion::Right);
                self.step(later, count, repeating, now, length);
            }
            Cmd::Move(Motion::RowStart) => self.go(shown.slice_at(self.cursor).1.start, length),
            Cmd::Move(Motion::RowEnd) => {
                let slice = shown.slice_at(self.cursor).1;
                self.go(slice.end.saturating_sub(1).max(slice.start), length);
            }
            Cmd::Move(Motion::First) | Cmd::Move(Motion::Last) if count.is_some() => {
                let number = count.unwrap_or(0);
                self.go(shown.slice(number).start, length);
            }
            Cmd::Move(Motion::First) => self.go(0, length),
            Cmd::Move(Motion::Last) => self.go(length, length),
            Cmd::Move(Motion::Down) | Cmd::Move(Motion::Up) => {
                let (number, _) = shown.slice_at(self.cursor);
                let number = if cmd == Cmd::Move(Motion::Down) {
                    number.saturating_add(times as usize)
                } else {
                    number.saturating_sub(times as usize)
                };
                self.go(shown.slice(number).start, length);
            }
            Cmd::Move(Motion::NextBeat) | Cmd::Move(Motion::PreviousBeat) => {
                let later = cmd == Cmd::Move(Motion::NextBeat);
                let to = beat_from(
                    self.cursor,
                    shown.entry.edit.points.start,
                    beat,
                    times,
                    later,
                );
                self.go(to, length);
            }
            Cmd::ViewTop => {
                self.go((self.column + times - 1) * zoom, length);
            }
            Cmd::ViewMiddle => self.go((self.column + columns / 2) * zoom, length),
            Cmd::ViewBottom => {
                let back = (times - 1).min(columns - 1);
                self.go((self.column + columns - 1 - back) * zoom, length);
            }
            Cmd::HalfPageDown | Cmd::HalfPageUp => {
                let by = (columns / 2).max(1) * times;
                let later = cmd == Cmd::HalfPageDown;
                if later {
                    self.column += by;
                    self.cursor = self.cursor.saturating_add(by * zoom);
                } else {
                    self.column = self.column.saturating_sub(by);
                    self.cursor = self.cursor.saturating_sub(by * zoom);
                }
                self.clamp_view(length);
                self.go(self.cursor, length);
            }
            Cmd::ScrollDown | Cmd::ScrollUp => {
                if cmd == Cmd::ScrollDown {
                    self.column += times;
                } else {
                    self.column = self.column.saturating_sub(times);
                }
                self.clamp_view(length);
                // A cursor scrolled off the view goes to the nearest column still in it, keeping
                // its place in its column, as Vim keeps the cursor on screen.
                let within = self.cursor % zoom;
                let cursor_column = self.column_of(self.cursor);
                let column = cursor_column.clamp(self.column, self.column + columns - 1);
                if column != cursor_column {
                    self.cursor = (column * zoom + within).min(length.saturating_sub(1));
                }
            }
            Cmd::NarrowGrid => zoom_by(self, times, true, length),
            Cmd::WidenGrid => zoom_by(self, times, false, length),
            Cmd::Sample(key) => return self.sample_key(key, shown, times, services),
            Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                let by = if matches!(cmd, Cmd::Increase | Cmd::Decrease) {
                    Nudge::Pixel
                } else {
                    Nudge::Column
                };
                let later = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
                self.nudge_point(shown, by, later, times, services);
            }
            Cmd::ClearField => self.remove_point(shown, services),
            Cmd::VisualField => {
                if self.anchor.is_some() {
                    self.end_selection();
                } else {
                    self.anchor = Some(self.cursor);
                }
            }
            Cmd::Reselect => match self.last_selection {
                Some((first, last)) => {
                    self.anchor = Some(first);
                    self.go(last, length);
                }
                None => return Outcome::Refused("No selection yet: v starts one"),
            },
            Cmd::VisualStep | Cmd::Operator(_) => {
                return Outcome::Refused(
                    "Whole slices and the operators come later: v selects frames to hear",
                );
            }
            Cmd::Replace
            | Cmd::Substitute
            | Cmd::IncreaseChromatic
            | Cmd::DecreaseChromatic
            | Cmd::CycleMark
            | Cmd::Paste
            | Cmd::PasteAfter
            | Cmd::PasteMerge
            | Cmd::Repeat => return Outcome::Refused("That doesn't work on the sample page yet"),
            // `;`, `Space c`, `i` and `K` are the program's, as they play, start the pads or open
            // the picker, and the rest mean what they do on every page.
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// One of the page's own keys beside the motions it shares with the other views. `'`, `m`,
    /// `]'` and `['` are passed back, as the program waits for their letter or keeps the jump.
    fn sample_key(
        &mut self,
        key: SampleKey,
        shown: &Shown,
        times: u64,
        services: &mut Services,
    ) -> Outcome {
        let length = shown.length();
        match key {
            SampleKey::FirstSound | SampleKey::LastSound => {
                let last = key == SampleKey::LastSound;
                let (number, slice) = shown.slice_at(self.cursor);
                match shown.sound_edge(slice, last) {
                    Some(frame) => self.go(frame, length),
                    None => services.status = format!("Slice {number:02X} is all silence"),
                }
            }
            SampleKey::NextPoint | SampleKey::PreviousPoint => {
                let points = shown.points();
                let cursor = self.cursor;
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
                    Some(frame) => self.go(frame, length),
                    None => return Outcome::Refused("No point further that way"),
                }
            }
            SampleKey::NextSound | SampleKey::PreviousSound => {
                let later = key == SampleKey::NextSound;
                let onsets = shown.onsets();
                let cursor = self.cursor;
                // As far as there are sounds, as a count past the last stops on it.
                let found = if later {
                    let first = onsets.partition_point(|onset| *onset <= cursor);
                    let last = onsets.len().saturating_sub(1);
                    onsets.get((first + times as usize - 1).min(last).max(first))
                } else {
                    let past = onsets.partition_point(|onset| *onset < cursor);
                    past.checked_sub(1)
                        .map(|last| &onsets[last.saturating_sub(times as usize - 1)])
                };
                match found {
                    Some(frame) => self.go(*frame, length),
                    None if onsets.is_empty() => services.status = no_sounds(shown),
                    None => {
                        return Outcome::Refused(
                            "No sound further that way: a higher Sens finds more",
                        );
                    }
                }
            }
            SampleKey::SoundEnd => {
                let onsets = shown.onsets();
                if onsets.is_empty() {
                    services.status = no_sounds(shown);
                    return Outcome::Done;
                }
                let mut at = Shown::sound_at(&onsets, self.cursor);
                let mut to = self.cursor;
                for _ in 0..times {
                    // Its last frame, as Vim's `e` lands on a word's last character, or the next
                    // sound's from there.
                    let mut end = shown.sound(&onsets, at).end - 1;
                    if end <= to && at + 1 < onsets.len() {
                        at += 1;
                        end = shown.sound(&onsets, at).end - 1;
                    }
                    if end <= to {
                        break;
                    }
                    to = end;
                }
                if to == self.cursor {
                    return Outcome::Refused("No sound ends further that way");
                }
                self.go(to, length);
            }
            SampleKey::ThisSound | SampleKey::ThisSlice => {
                if self.anchor.is_none() {
                    return Outcome::Refused("is and ip select while selecting: v starts it");
                }
                let cursor = self.cursor;
                let selected = if key == SampleKey::ThisSlice {
                    let (number, here) = shown.slice_at(cursor);
                    let last = number + times as usize - 1;
                    here.start..shown.slice(last).end.max(here.start + 1)
                } else {
                    let onsets = shown.onsets();
                    if onsets.is_empty() {
                        services.status = no_sounds(shown);
                        return Outcome::Done;
                    }
                    let first = Shown::sound_at(&onsets, cursor);
                    let last = (first + times as usize - 1).min(onsets.len() - 1);
                    shown.sound(&onsets, first).start..shown.sound(&onsets, last).end
                };
                self.anchor = Some(selected.start);
                self.go(selected.end - 1, length);
            }
            SampleKey::Mark
            | SampleKey::NameSlice
            | SampleKey::NextMark
            | SampleKey::PreviousMark => return Outcome::Passed,
            SampleKey::Split => self.split_here(shown, services),
            SampleKey::Join => self.join_here(shown, services),
            SampleKey::NudgeFrame { later } => {
                self.nudge_point(shown, Nudge::Frame, later, times, services);
            }
            SampleKey::SetPoint(edge) => self.set_point(shown, edge, services),
            SampleKey::OtherEnd => match self.anchor {
                Some(anchor) => {
                    self.anchor = Some(self.cursor);
                    self.go(anchor, length);
                }
                None => return Outcome::Refused("o swaps a selection's ends: v starts one"),
            },
            SampleKey::GainUp | SampleKey::GainDown => {
                let up = key == SampleKey::GainUp;
                self.change_setting(shown, SampleSetting::Gain, up, false, times, services);
            }
            SampleKey::Reverse => self.reverse_here(shown, services),
            SampleKey::CursorMiddle => {
                self.middle_on_cursor();
                self.clamp_view(length);
            }
            SampleKey::CursorLeft => {
                self.column = self.column_of(self.cursor);
                self.clamp_view(length);
            }
            SampleKey::CursorRight => {
                let back = self.columns.saturating_sub(1) as u64;
                self.column = self.column_of(self.cursor).saturating_sub(back);
                self.clamp_view(length);
            }
            SampleKey::ZoomIn => zoom_by(self, times, true, length),
            SampleKey::ZoomOut => zoom_by(self, times, false, length),
            SampleKey::ZoomAll => {
                self.zoom = self.fit(length);
                self.column = 0;
            }
            SampleKey::ZoomSlice => {
                let (_, slice) = shown.slice_at(self.cursor);
                let wanted = (slice.end - slice.start).div_ceil(self.columns.max(1) as u64);
                let fit = self.fit(length);
                let mut zoom = fit;
                while SamplePage::zoomed_in(zoom) >= wanted.max(1) && zoom > 1 {
                    zoom = SamplePage::zoomed_in(zoom);
                }
                self.zoom = zoom;
                // The slice's middle in the view's.
                let middle = slice.start.midpoint(slice.end);
                self.column = self
                    .column_of(middle)
                    .saturating_sub((self.columns / 2) as u64);
                self.clamp_view(length);
                if self.cursor_on_screen().is_none() {
                    self.cursor = slice.start.min(length.saturating_sub(1));
                }
            }
            SampleKey::CursorStyle => {
                self.cursor_line = !self.cursor_line;
                services.status = if self.cursor_line {
                    "The cursor is a line at its frame: zc for its column".to_owned()
                } else {
                    "The cursor is its column: zc for a line at its frame".to_owned()
                };
            }
        }
        Outcome::Done
    }
}

impl Page for SamplePage {
    /// A sample is a buffer and a cell column a character, and the slice is the line, but the
    /// cursor is a line placed to the pixel, so `h` and `l` move a pixel at the zoom shown,
    /// speeding up as they're held.
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("h", Cmd::Move(Motion::Left), "A pixel earlier, faster held"),
        ("l", Cmd::Move(Motion::Right), "A pixel later, faster held"),
        ("0", Cmd::Move(Motion::RowStart), "Start of the slice"),
        ("$", Cmd::Move(Motion::RowEnd), "End of the slice"),
        (
            "^",
            Cmd::Sample(SampleKey::FirstSound),
            "First sound in the slice",
        ),
        (
            "g _",
            Cmd::Sample(SampleKey::LastSound),
            "Last sound in the slice",
        ),
        ("j", Cmd::Move(Motion::Down), "Next slice"),
        ("k", Cmd::Move(Motion::Up), "Previous slice"),
        (
            "g g",
            Cmd::Move(Motion::First),
            "Start of the sample, or slice n",
        ),
        (
            "G",
            Cmd::Move(Motion::Last),
            "End of the sample, or slice n",
        ),
        (
            "}",
            Cmd::Move(Motion::NextBeat),
            "Next beat, from the start, at the tempo",
        ),
        (
            "{",
            Cmd::Move(Motion::PreviousBeat),
            "Previous beat, from the start, at the tempo",
        ),
        (
            "w",
            Cmd::Sample(SampleKey::NextSound),
            "Next sound's start, as Sens finds them",
        ),
        (
            "b",
            Cmd::Sample(SampleKey::PreviousSound),
            "Previous sound's start, as Sens finds them",
        ),
        (
            "e",
            Cmd::Sample(SampleKey::SoundEnd),
            "Where the sound decays, or the next one",
        ),
        (
            "] ]",
            Cmd::Sample(SampleKey::NextPoint),
            "Next point: a slice, the start, end or loop",
        ),
        (
            "[ [",
            Cmd::Sample(SampleKey::PreviousPoint),
            "Previous point: a slice, the start, end or loop",
        ),
        (
            "'",
            Cmd::Sample(SampleKey::Mark),
            "Then a letter: the slice it names, as a jump",
        ),
        (
            "] '",
            Cmd::Sample(SampleKey::NextMark),
            "Next slice a letter names, as a jump",
        ),
        (
            "[ '",
            Cmd::Sample(SampleKey::PreviousMark),
            "Previous slice a letter names, as a jump",
        ),
        ("H", Cmd::ViewTop, "Cursor to the left of the view"),
        ("M", Cmd::ViewMiddle, "Cursor to the middle of the view"),
        ("L", Cmd::ViewBottom, "Cursor to the right of the view"),
        ("C-d", Cmd::HalfPageDown, "Half a screen later"),
        ("C-u", Cmd::HalfPageUp, "Half a screen earlier"),
        ("C-e", Cmd::ScrollDown, "Scroll the view a column later"),
        ("C-y", Cmd::ScrollUp, "Scroll the view a column earlier"),
        (
            "z z",
            Cmd::Sample(SampleKey::CursorMiddle),
            "View with the cursor in its middle",
        ),
        (
            "z s",
            Cmd::Sample(SampleKey::CursorLeft),
            "View with the cursor at its left",
        ),
        (
            "z e",
            Cmd::Sample(SampleKey::CursorRight),
            "View with the cursor at its right",
        ),
        (
            "z i",
            Cmd::Sample(SampleKey::ZoomIn),
            "Zoom in by 2, on the cursor",
        ),
        (
            "z o",
            Cmd::Sample(SampleKey::ZoomOut),
            "Zoom out by 2, on the cursor",
        ),
        ("z O", Cmd::Sample(SampleKey::ZoomAll), "The whole sample"),
        (
            "z v",
            Cmd::Sample(SampleKey::ZoomSlice),
            "The slice, filling the view",
        ),
        ("M-1", Cmd::NarrowGrid, "Zoom in by 2, as z i"),
        ("M-2", Cmd::WidenGrid, "Zoom out by 2, as z o"),
        (
            "z c",
            Cmd::Sample(SampleKey::CursorStyle),
            "The cursor as a line, or as its column",
        ),
        (
            ";",
            Cmd::Audition,
            "Hear the slice, or the selection; held, until let go",
        ),
        (
            "Enter",
            Cmd::Sample(SampleKey::Split),
            "Split the slice here, or at the playhead as it plays",
        ),
        (
            "J",
            Cmd::Sample(SampleKey::Join),
            "Join the slice with the next",
        ),
        (
            "x",
            Cmd::ClearField,
            "Take the point here away, or a setting back",
        ),
        (
            "=",
            Cmd::Increase,
            "Nudge the point here a pixel later, or a setting up",
        ),
        (
            "-",
            Cmd::Decrease,
            "Nudge the point here a pixel earlier, or a setting down",
        ),
        (
            "+",
            Cmd::IncreaseLarge,
            "Nudge the point here a column later, or a setting up more",
        ),
        (
            "_",
            Cmd::DecreaseLarge,
            "Nudge the point here a column earlier, or a setting down more",
        ),
        (
            "D-l",
            Cmd::Sample(SampleKey::NudgeFrame { later: true }),
            "Nudge the point here a frame later",
        ),
        (
            "D-h",
            Cmd::Sample(SampleKey::NudgeFrame { later: false }),
            "Nudge the point here a frame earlier",
        ),
        (
            "g s",
            Cmd::Sample(SampleKey::SetPoint(Edge::Start)),
            "Start the sample here",
        ),
        (
            "g e",
            Cmd::Sample(SampleKey::SetPoint(Edge::End)),
            "End the sample here",
        ),
        (
            "g l",
            Cmd::Sample(SampleKey::SetPoint(Edge::LoopStart)),
            "Start the loop here",
        ),
        (
            "g L",
            Cmd::Sample(SampleKey::SetPoint(Edge::LoopEnd)),
            "End the loop here",
        ),
        (
            "m",
            Cmd::Sample(SampleKey::NameSlice),
            "Then a letter: name the slice here with it",
        ),
        ("v", Cmd::VisualField, "Select frames, or end the selection"),
        ("g v", Cmd::Reselect, "Select the last selection again"),
        (
            "o",
            Cmd::Sample(SampleKey::OtherEnd),
            "To the selection's other end",
        ),
        (
            ">",
            Cmd::Sample(SampleKey::GainUp),
            "The slice's gain up a dB",
        ),
        (
            "<",
            Cmd::Sample(SampleKey::GainDown),
            "The slice's gain down a dB",
        ),
        ("~", Cmd::Sample(SampleKey::Reverse), "Reverse the slice"),
        (
            "R",
            Cmd::ToggleSampling,
            "Record into a new sample, as Space R, and show it once done",
        ),
        (
            "K",
            Cmd::PickValue,
            "Choose a setting to change, or list its values",
        ),
    ];

    type Drawn = Option<WaveOnScreen>;

    /// Run a key as the program gives it to the page, on the sample the cursor's track plays: one
    /// of the page's own keys, [`SamplePage::key_on`], what plays, the pads, or `K`'s picker, or
    /// passed back, for the marks and what means the same on every page.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        let Ok(shown) = self.shown(ctx.services) else {
            return Outcome::Passed;
        };
        let count = ctx.modes.count;
        let services = &mut *ctx.services;
        let started = match self.key_on(cmd, times as u64, count, &shown, services) {
            Outcome::Passed => match cmd {
                Cmd::PickValue => {
                    let picker = match self.setting {
                        Some(setting) => {
                            self.values_picker(&shown, setting, services.song().millibpm)
                        }
                        None => self.settings_picker(&shown),
                    };
                    return Outcome::Open(Box::new(picker));
                }
                Cmd::Audition => match self.selection() {
                    Some(selected) => self.audition_frames(&shown, selected, services),
                    None => self.audition_slice(&shown, services),
                },
                Cmd::PlayFromCursor => self.audition_from_cursor(&shown, services),
                // The keys mode, where two rows of keys play the slices. The status line says
                // `PADS` while it's on, and what the keys play under it.
                Cmd::EnterInsert => {
                    services.status.clear();
                    return Outcome::Enter(Mode::Keys);
                }
                _ => return Outcome::Passed,
            },
            outcome => return outcome,
        };
        started.map_or(Outcome::Done, Outcome::Hold)
    }

    /// Draw the page, giving back where its waveform is on screen, if it's drawn, for the lines
    /// and marks the terminal places over it.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Option<WaveOnScreen> {
        crate::pages::sample::draw::draw(frame, area, self, ctx)
    }

    fn column(&self, _ctx: &Ctx) -> String {
        match self.setting {
            Some(setting) => format!("Sample: {}", setting.name()),
            None => "Sample".to_owned(),
        }
    }

    /// What the keys do in the page's mode, and how to leave it, so a mode is never on without a
    /// word.
    fn hint(&self, ctx: &Ctx) -> Option<String> {
        if ctx.mode() == Mode::Keys {
            let shown = ctx.sample_shown(self).ok()?;
            return Some(self.pads_hint(&shown));
        }
        if self.anchor.is_some() {
            return Some(
                "; plays the selection, o goes to its other end; v or Esc ends it".to_owned(),
            );
        }
        let setting = self.setting?;
        Some(format!(
            "{}: = - + _ change it, K lists its values, x puts it back, h l the setting before or \
             after; Esc goes back to the waveform",
            setting.name()
        ))
    }

    /// The waveform and the overview of the whole sample under it, as the view and the points
    /// have them, or none with no sample to show.
    fn panels(&self, ctx: &Ctx) -> Vec<PanelWant> {
        let Ok(shown) = ctx.sample_shown(self) else {
            return Vec::new();
        };
        let wave = Wave {
            analysis: Arc::clone(&shown.analysis),
            frames: shown.frames.clone(),
            points: drawn_points(&shown.entry.edit.points),
            first: self.first_frame(),
            zoom: self.zoom,
            columns: self.columns,
            rows: usize::from(WAVE_ROWS),
            merged: false,
        };
        let overview = Wave {
            first: 0,
            zoom: self.fit(shown.length()),
            rows: 1,
            merged: true,
            ..wave.clone()
        };
        [
            (PanelKey::SampleWave, wave),
            (PanelKey::SampleOverview, overview),
        ]
        .into_iter()
        .map(|(key, wave)| PanelWant {
            key,
            shade: Shade::Focused,
            cells: (wave.columns, wave.rows),
            shape: Shape::Wave(wave),
        })
        .collect()
    }

    fn take_panels(&mut self, ids: PanelIds, _wants: &[PanelWant], _rate: u32) {
        self.images = ids;
    }
}

/// What the status line says when the entry's `Sens` finds no sound in it.
fn no_sounds(shown: &Shown) -> String {
    format!(
        "No sound found at Sens {}%: a higher Sens, from K, finds more",
        shown.entry.edit.hints.sens
    )
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
pub(crate) fn held_pixels(cell: u64, held: Duration) -> u64 {
    let along = (held.as_secs_f64() / SPEED_UP.as_secs_f64()).min(1.0);
    1 + (cell.saturating_sub(1) as f64 * along * along).round() as u64
}

/// How many of a file's frames a beat lasts at a tempo, in thousandths of a beat a minute.
pub(crate) fn beat_frames(rate: u32, millibpm: u32) -> f64 {
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
pub(crate) fn is_sample_motion(cmd: Cmd) -> bool {
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
