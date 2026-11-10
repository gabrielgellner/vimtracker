//! Placing points on the sample page. `Enter` splits the slice at the cursor, or at the playhead
//! while one plays, so a break can be sliced by tapping along to it; `J` joins a slice with the
//! next; `x` takes the point under the cursor away, never the audio; `=` `-`, `+` `_` and Cmd-h/l
//! nudge it by a pixel, a column or a frame, between its neighbours; and `g s`, `g e`, `g l` and
//! `g L` put the start, the end and the loop's ends at the cursor. With the entry's snap on, a
//! point placed goes to the nearest zero crossing.
//!
//! Each is one undo step on the bank entry, through the actions `:slice` and `:mark` use, so the
//! steps' `SLI` locks and the letters follow their slices as they renumber.

use vt_model::action::Action;
use vt_model::bank::{BankEntry, Loop};

use crate::app::App;
use crate::banking::sample_text;
use crate::sample_page::Shown;
use crate::views::sample::time;

/// A point of an entry other than a slice's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
    LoopStart,
    LoopEnd,
}

impl Edge {
    /// What it's called, at the start of a sentence.
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "The start",
            Self::End => "The end",
            Self::LoopStart => "The loop's start",
            Self::LoopEnd => "The loop's end",
        }
    }
}

/// A point on the sample page: where a slice starts, by its number, or one of the entry's edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    Slice(usize),
    Edge(Edge),
}

/// How far a nudge moves a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nudge {
    /// `=` and `-`: a pixel of the waveform at the zoom shown, as `h` and `l` move the cursor.
    Pixel,
    /// `+` and `_`: a column, a character's width.
    Column,
    /// Cmd-h and Cmd-l: a frame, whatever the zoom.
    Frame,
}

impl Shown {
    /// Where a point is, in the file's frames, if the entry has it.
    pub fn point_frame(&self, point: Point) -> Option<u64> {
        let points = &self.entry.edit.points;
        match point {
            Point::Slice(number) => points.slices.get(number).map(|slice| slice.start),
            Point::Edge(Edge::Start) => Some(points.start),
            Point::Edge(Edge::End) => Some(self.end()),
            Point::Edge(Edge::LoopStart) => points.loop_.map(|looped| looped.start),
            Point::Edge(Edge::LoopEnd) => points.loop_.map(|looped| looped.end),
        }
    }

    /// The point nearest a frame, no further than `reach` frames from it. Where two are as near, a
    /// slice's start comes first, then the start, the end and the loop's, as slicing is what the
    /// page is mostly for and `g s`, `g e`, `g l` and `g L` place the others.
    pub fn point_near(&self, frame: u64, reach: u64) -> Option<Point> {
        let slices = (0..self.entry.edit.points.slices.len()).map(Point::Slice);
        let edges = [Edge::Start, Edge::End, Edge::LoopStart, Edge::LoopEnd].map(Point::Edge);
        let mut nearest: Option<(u64, Point)> = None;
        for point in slices.chain(edges) {
            let Some(at) = self.point_frame(point) else {
                continue;
            };
            let distance = at.abs_diff(frame);
            if distance <= reach && nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, point));
            }
        }
        nearest.map(|(_, point)| point)
    }

    /// Where a point can go without passing its neighbours: a slice's start between the slices
    /// either side of it, inside the entry; the start before the end and the end after the start;
    /// and the loop's start before its end and its end after its start, inside the file.
    pub fn point_bounds(&self, point: Point) -> std::ops::RangeInclusive<u64> {
        let points = &self.entry.edit.points;
        let length = self.length();
        let end = self.end();
        match point {
            Point::Slice(number) => {
                let low = match number.checked_sub(1) {
                    Some(before) => points.slices[before].start + 1,
                    None => points.start.min(end),
                };
                let high = match points.slices.get(number + 1) {
                    Some(next) => next.start.saturating_sub(1),
                    None => end.saturating_sub(1),
                };
                low..=high.max(low)
            }
            Point::Edge(Edge::Start) => 0..=end.saturating_sub(1),
            Point::Edge(Edge::End) => (points.start + 1).min(length)..=length,
            Point::Edge(Edge::LoopStart) => {
                let high = points.loop_.map_or(length, |looped| looped.end);
                0..=high.saturating_sub(1)
            }
            Point::Edge(Edge::LoopEnd) => {
                let low = points.loop_.map_or(0, |looped| looped.start + 1);
                low.min(length)..=length
            }
        }
    }

    /// The entry with a point moved to a frame, which is inside its bounds.
    fn moved(&self, point: Point, frame: u64) -> BankEntry {
        let mut entry = self.entry.clone();
        let length = self.length();
        let points = &mut entry.edit.points;
        match point {
            Point::Slice(number) => points.slices[number].start = frame,
            Point::Edge(Edge::Start) => points.start = frame,
            Point::Edge(Edge::End) => points.end = (frame < length).then_some(frame),
            Point::Edge(Edge::LoopStart) => {
                if let Some(looped) = &mut points.loop_ {
                    looped.start = frame;
                }
            }
            Point::Edge(Edge::LoopEnd) => {
                if let Some(looped) = &mut points.loop_ {
                    looped.end = frame;
                }
            }
        }
        entry
    }

    /// The zero crossing nearest a frame, within `within` frames of it, and only at or after it,
    /// or at or before it, if `toward` says which way. A frame is one if it's silent, or if the
    /// sound changes sign from the frame before, the channels taken together. The earlier of two
    /// as near wins. None if there's none so near, or the frames aren't loaded.
    pub fn zero_crossing(&self, frame: u64, within: u64, toward: Option<bool>) -> Option<u64> {
        let (data, rates) = self.frames.as_ref()?;
        let channels = self.analysis.channels().clamp(1, 2);
        let value = |at: u64| -> Option<f32> {
            let both = data.frames.get(rates.loaded_frame(at) as usize)?;
            Some(both[..channels].iter().sum())
        };
        let crossing = |at: u64| -> bool {
            let Some(now) = value(at) else {
                return false;
            };
            if now == 0.0 {
                return true;
            }
            let Some(before) = at.checked_sub(1).and_then(value) else {
                return false;
            };
            (before > 0.0 && now < 0.0) || (before < 0.0 && now > 0.0)
        };
        let length = self.length();
        for distance in 0..=within {
            let earlier = frame.checked_sub(distance).filter(|_| toward != Some(true));
            let later = frame
                .checked_add(distance)
                .filter(|at| *at < length && toward != Some(false));
            if let Some(at) = earlier.into_iter().chain(later).find(|at| crossing(*at)) {
                return Some(at);
            }
        }
        None
    }
}

impl App {
    /// A frame a point is to be placed at, on the nearest zero crossing within a column if the
    /// entry snaps, and only that way from it if it's moving one way.
    fn snapped(&self, shown: &Shown, frame: u64, toward: Option<bool>) -> u64 {
        if !shown.entry.edit.play.snap_zero {
            return frame;
        }
        let within = self.sample_page.zoom.max(1);
        shown.zero_crossing(frame, within, toward).unwrap_or(frame)
    }

    /// The point under the cursor: the nearest within half a column of it.
    pub fn point_here(&self, shown: &Shown) -> Option<Point> {
        let reach = self.sample_page.zoom / 2;
        shown.point_near(self.sample_page.cursor, reach)
    }

    /// Where the newest sound of the entry shown is, in its file's frames, for `Enter` to split at
    /// while it plays: what the page itself played, if it's still playing, or else the track's
    /// note of it, which comes before its tail in the report.
    fn tapped_frame(&self, shown: &Shown) -> Option<u64> {
        let (_, rates) = shown.frames.as_ref()?;
        let heard = self.sample_page.heard == Some((shown.number, shown.analysis.hash));
        let heads: Vec<_> = self.playheads.iter().flatten().collect();
        let own = heads.iter().find(|head| head.entry.is_none() && heard);
        let note = heads.iter().find(|head| head.entry == Some(shown.number));
        own.or(note).map(|head| rates.file_frame(head.frame))
    }

    /// `Enter`: split the slice at the cursor, or at the playhead while the entry plays, so a break
    /// can be sliced by tapping along to it. The part before keeps the slice's id and letters.
    pub(crate) fn split_here(&mut self, shown: &Shown) {
        let (at, tapped) = match self.tapped_frame(shown) {
            Some(frame) => (frame, true),
            None => (self.sample_page.cursor, false),
        };
        let at = self.snapped(shown, at, None);
        let points = &shown.entry.edit.points;
        let rate = shown.rate();
        if points.split_starts(at).is_none() {
            self.status = if points.slices.iter().any(|slice| slice.start == at) {
                format!("A slice already starts at {}", time(at, rate))
            } else {
                "Slices split between the start and the end".to_owned()
            };
            return;
        }
        let number = shown.number;
        self.apply(&Action::SplitSlice { number, at });
        let Some(entry) = self.song().bank.get(number) else {
            return;
        };
        let slice = entry.edit.points.slice_at(at);
        let count = entry.edit.points.slices.len();
        let by = if tapped { "the playhead" } else { "the cursor" };
        self.status = format!(
            "Slice {slice:02X} of {count} from {} at {by}: u to undo",
            time(at, rate)
        );
    }

    /// `J`: join the slice under the cursor with the next, which it takes in with its letters.
    pub(crate) fn join_here(&mut self, shown: &Shown) {
        let points = &shown.entry.edit.points;
        if points.slices.is_empty() {
            self.status = format!("{} has no slices to join", shown.entry.title());
            return;
        }
        let (slice, _) = shown.slice_at(self.sample_page.cursor);
        if slice + 1 >= points.slices.len() {
            self.status = format!("Slice {slice:02X} is the last: there's none after to join");
            return;
        }
        self.apply(&Action::JoinSlices {
            number: shown.number,
            slice: slice as u8,
        });
        let count = self
            .song()
            .bank
            .get(shown.number)
            .map_or(0, |entry| entry.edit.points.slices.len());
        self.status = format!(
            "Joined slices {slice:02X} and {:02X}, so there are {count}: u to undo",
            slice + 1
        );
    }

    /// `x`: take away the point under the cursor, never the audio. A slice's start joins it to the
    /// slice before; the start goes back to the file's, the end to the file's, and either loop
    /// point takes the loop away.
    pub(crate) fn remove_point(&mut self, shown: &Shown) {
        let Some(point) = self.point_here(shown) else {
            self.status = "No point under the cursor: ]] and [[ go to one".to_owned();
            return;
        };
        let number = shown.number;
        let name = sample_text(self.song(), number);
        let mut entry = shown.entry.clone();
        let points = &mut entry.edit.points;
        let said = match point {
            Point::Slice(0) if points.slices.len() == 1 => {
                self.apply(&Action::SetSlices {
                    number,
                    starts: Vec::new(),
                });
                self.status = format!("{name} plays as one again: u to undo");
                return;
            }
            Point::Slice(0) => {
                self.status =
                    "Slice 00 is where the slices start: J joins it with 01, or :slice clear"
                        .to_owned();
                return;
            }
            Point::Slice(slice) => {
                self.apply(&Action::JoinSlices {
                    number,
                    slice: (slice - 1) as u8,
                });
                self.status = format!(
                    "Took slice {slice:02X}'s start away, so {:02X} plays on through it: u to undo",
                    slice - 1
                );
                return;
            }
            Point::Edge(Edge::Start) if points.start == 0 => {
                self.status = "The start is already the file's".to_owned();
                return;
            }
            Point::Edge(Edge::Start) => {
                points.start = 0;
                "starts with its file again"
            }
            Point::Edge(Edge::End) if points.end.is_none() => {
                self.status = "The end is already the file's".to_owned();
                return;
            }
            Point::Edge(Edge::End) => {
                points.end = None;
                "ends with its file again"
            }
            Point::Edge(Edge::LoopStart | Edge::LoopEnd) => {
                points.loop_ = None;
                "has no loop of its own"
            }
        };
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        self.status = format!("{name} {said}: u to undo");
    }

    /// `=` `-`, `+` `_` and Cmd-h/l: move the point under the cursor by a pixel, a column or a
    /// frame, `times` over, no further than its neighbours, with the cursor following it so the
    /// next nudge moves it again. With snap on, a pixel's or a column's nudge goes on to the next
    /// zero crossing that way; a frame's never snaps, as it's for placing by hand.
    pub(crate) fn nudge_point(&mut self, shown: &Shown, by: Nudge, later: bool, times: u64) {
        let Some(point) = self.point_here(shown) else {
            self.status = "No point under the cursor to nudge: ]] and [[ go to one".to_owned();
            return;
        };
        let Some(from) = shown.point_frame(point) else {
            return;
        };
        let page = &self.sample_page;
        let zoom = page.zoom.max(1);
        let to = match by {
            Nudge::Frame => step_frames(from, times, later),
            Nudge::Column => step_frames(from, times.saturating_mul(zoom), later),
            Nudge::Pixel => match page.cell_width {
                Some(cell) if cell > 1 => pixels_along(from, times, later, zoom, cell as u64),
                // With no pictures drawn there's no pixel, so a column, as `h` and `l` move.
                _ => step_frames(from, times.saturating_mul(zoom), later),
            },
        };
        let to = if by == Nudge::Frame {
            to
        } else {
            self.snapped(shown, to, Some(later))
        };
        let bounds = shown.point_bounds(point);
        let to = to.clamp(*bounds.start(), *bounds.end());
        let length = shown.length();
        if to == from {
            self.status = format!("{} can't go further that way", point_name(point));
            return;
        }
        self.apply(&Action::SetBankEntry {
            number: shown.number,
            entry: Some(shown.moved(point, to)),
        });
        self.sample_page.go(to, length);
        self.status = format!(
            "{} at {}, frame {to}: u to undo",
            point_name(point),
            time(to, shown.rate())
        );
    }

    /// `g s`, `g e`, `g l` and `g L`: put the start, the end, or the loop's start or end at the
    /// cursor. With no loop, `g l` loops from the cursor to the end and `g L` from the start to
    /// the cursor.
    pub(crate) fn set_point(&mut self, shown: &Shown, edge: Edge) {
        let at = self.snapped(shown, self.sample_page.cursor, None);
        let mut entry = shown.entry.clone();
        let length = shown.length();
        let end = shown.end();
        let points = &mut entry.edit.points;
        let refused = match edge {
            Edge::Start if at >= end => Some("The start goes before the end"),
            Edge::Start => {
                points.start = at;
                None
            }
            Edge::End if at <= points.start => Some("The end goes after the start"),
            Edge::End => {
                points.end = (at < length).then_some(at);
                None
            }
            Edge::LoopStart => {
                let looped = points.loop_.get_or_insert(Loop {
                    start: at,
                    end,
                    crossfade: 0,
                });
                looped.start = at;
                (at >= looped.end).then_some("The loop's start goes before its end")
            }
            Edge::LoopEnd => {
                let start = points.start;
                let looped = points.loop_.get_or_insert(Loop {
                    start,
                    end: at,
                    crossfade: 0,
                });
                looped.end = at;
                (at <= looped.start).then_some("The loop's end goes after its start")
            }
        };
        if let Some(why) = refused {
            self.status = why.to_owned();
            return;
        }
        self.apply(&Action::SetBankEntry {
            number: shown.number,
            entry: Some(entry),
        });
        self.status = format!(
            "{} at {}, frame {at}: u to undo",
            edge.name(),
            time(at, shown.rate())
        );
    }
}

/// What a point is called, at the start of a sentence.
fn point_name(point: Point) -> String {
    match point {
        Point::Slice(number) => format!("Slice {number:02X}"),
        Point::Edge(edge) => edge.name().to_owned(),
    }
}

/// A frame this many frames on, or back, not before the first.
fn step_frames(frame: u64, by: u64, later: bool) -> u64 {
    if later {
        frame.saturating_add(by)
    } else {
        frame.saturating_sub(by)
    }
}

/// A frame moved this many pixels of the waveform along at a zoom, for characters `cell` pixels
/// wide: to the first frame of the pixel that many along, so its line moves exactly that many
/// pixels, as `h` and `l` move the cursor's. Closer than a frame a pixel, a pixel is a frame.
pub fn pixels_along(frame: u64, pixels: u64, later: bool, zoom: u64, cell: u64) -> u64 {
    if zoom < cell {
        return step_frames(frame, pixels, later);
    }
    let pixel = u128::from(frame) * u128::from(cell) / u128::from(zoom);
    let pixel = if later {
        pixel.saturating_add(u128::from(pixels))
    } else {
        pixel.saturating_sub(u128::from(pixels))
    };
    (pixel * u128::from(zoom))
        .div_ceil(u128::from(cell))
        .min(u128::from(u64::MAX)) as u64
}
