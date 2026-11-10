//! The sample page's waveform as a picture, drawn at one pixel for each of the screen's, so it's as
//! sharp as kitty can show it. It's drawn from whichever is nearest the zoom: the analysis's peak
//! pyramid down to 256 frames a pixel, the frames themselves below that, and closer than a frame a
//! pixel, lines between frames with a dot on each, as Audacity draws them, so zero crossings and
//! transients can be found by eye. Each lane has a faint line at zero.
//!
//! Its colours each mean something: the sound that plays, the sound outside the start and end that
//! doesn't, zero, and the start, end and loop points, each a line two pixels wide at its frame,
//! and where each slice starts, a line one pixel wide, under the others where they meet. The
//! cursor, the selection and the playhead aren't in it: they're drawn over it or under it, so they
//! move without it being sent again.

use std::ops::Range;

use ratatui::style::Color;
use vt_model::analysis::{Analysis, MinMax, Pyramid};
use vt_model::bank::Points;
use vt_model::sample::SampleData;
use vt_model::samples::Rates;

use crate::kitty::{Picture, rgb};
use crate::theme;

/// The colours of the picture, by their numbers in it.
pub const WAVE: u8 = 1;
/// The sound before the start or after the end, which doesn't play.
pub const OUTSIDE: u8 = 2;
pub const ZERO: u8 = 3;
pub const START: u8 = 4;
pub const END: u8 = 5;
pub const LOOP: u8 = 6;
pub const SLICE: u8 = 7;

/// The theme's colours for the points, which the ruler and the settings row use too, so each
/// point's colour means the same wherever it's shown.
pub const START_COLOR: Color = theme::TEAL;
pub const END_COLOR: Color = theme::RED;
pub const LOOP_COLOR: Color = theme::BLUE;
pub const SLICE_COLOR: Color = theme::PEACH;

/// How wide a point's line is, in pixels.
const POINT_PIXELS: usize = 2;
/// How wide a slice's line is, thinner than the points', as there can be many.
const SLICE_PIXELS: usize = 1;
/// The fewest pixels a frame has to itself for it to be drawn as a dot as well as a line.
const DOT_SPACING: f64 = 6.0;
/// How far a dot reaches from its frame, in pixels, either way.
const DOT_REACH: i64 = 2;

/// The picture's colours, in the order of their numbers.
pub fn colors() -> Vec<[u8; 3]> {
    [
        theme::TEXT,
        theme::OVERLAY0,
        theme::SURFACE1,
        START_COLOR,
        END_COLOR,
        LOOP_COLOR,
        SLICE_COLOR,
    ]
    .map(rgb)
    .to_vec()
}

/// What to draw, and how big.
pub struct Drawing<'a> {
    pub analysis: &'a Analysis,
    /// The file's frames at the program's rate, for drawing closer than the pyramid goes.
    pub frames: Option<(&'a SampleData, Rates)>,
    pub points: &'a Points,
    /// The frame at the left edge.
    pub first: u64,
    /// How many frames each character across covers.
    pub zoom: u64,
    /// How many characters it covers, across and down.
    pub columns: usize,
    pub rows: usize,
    /// A character's size in pixels.
    pub cell: (usize, usize),
    /// Whether both channels are drawn together in one lane, as the overview is, rather than a
    /// lane each, left above right.
    pub merged: bool,
}

impl Drawing<'_> {
    /// The first frame a column of pixels covers.
    fn frame_at(&self, x: usize) -> u64 {
        let across = self.cell.0.max(1) as u128;
        self.first + (x as u128 * u128::from(self.zoom) / across) as u64
    }

    /// The column of pixels a frame is at, which may be off either side.
    fn x_of(&self, frame: u64) -> i64 {
        let across = self.cell.0.max(1) as i128;
        ((i128::from(frame) - i128::from(self.first)) * across).div_euclid(i128::from(self.zoom))
            as i64
    }

    /// Whether a frame plays, between the start and the end.
    fn plays(&self, frame: u64) -> bool {
        let end = self.points.end.unwrap_or(u64::MAX);
        (self.points.start..end).contains(&frame)
    }

    /// The lowest and highest sample of some channels over some frames: from the pyramid when a
    /// pixel covers its finest buckets, and from the frames below that.
    fn fold(&self, channels: &Range<usize>, frames: Range<u64>) -> MinMax {
        let per_pixel = self.zoom as f64 / self.cell.0.max(1) as f64;
        let pyramid = &self.analysis.pyramid;
        match &self.frames {
            Some((data, rates)) if per_pixel < Pyramid::bucket_frames(0) as f64 => {
                let from = rates.loaded_frame(frames.start) as usize;
                let to = (rates.loaded_frame(frames.end) as usize)
                    .max(from + 1)
                    .min(data.frames.len());
                let mut fold = MinMax::EMPTY;
                for frame in data.frames.get(from..to).unwrap_or_default() {
                    for channel in channels.clone() {
                        fold = fold.join(MinMax::of(frame[channel.min(1)]));
                    }
                }
                fold
            }
            _ => channels.clone().fold(MinMax::EMPTY, |fold, channel| {
                fold.join(pyramid.fold(channel, frames.clone()))
            }),
        }
    }

    /// One frame's sample in a channel, as near as there is one.
    fn value(&self, channel: usize, frame: u64) -> f32 {
        match &self.frames {
            Some((data, rates)) => data
                .frames
                .get(rates.loaded_frame(frame) as usize)
                .map_or(0.0, |both| both[channel.min(1)]),
            None => {
                let fold = self.analysis.pyramid.fold(channel, frame..frame + 1);
                if fold.is_empty() {
                    0.0
                } else {
                    (fold.min + fold.max) / 2.0
                }
            }
        }
    }

    /// The picture, at one pixel for each of the screen's.
    pub fn picture(&self) -> Picture {
        let (width, height) = (self.columns * self.cell.0, self.rows * self.cell.1);
        let mut picture = Picture::new(width, height, colors());
        if width == 0 || height == 0 || self.zoom == 0 {
            return picture;
        }
        let channels = self.analysis.channels().clamp(1, 2);
        let lanes = if self.merged { 1 } else { channels };
        let lane_height = height / lanes;
        for lane in 0..lanes {
            let shown = if self.merged {
                0..channels
            } else {
                lane..lane + 1
            };
            let top = lane * lane_height;
            let lane = Lane {
                top,
                height: lane_height,
            };
            if !self.merged {
                let zero = lane.row(0.0);
                picture.pixels[zero * width..(zero + 1) * width].fill(ZERO);
            }
            if self.zoom >= self.cell.0 as u64 {
                self.draw_columns(&mut picture, &lane, &shown);
            } else {
                self.draw_lines(&mut picture, &lane, shown.start);
            }
        }
        self.draw_points(&mut picture);
        picture
    }

    /// A pixel or more of frames to each column: each column from its lowest to its highest
    /// sample, taking in the last frame of the column before, so a wave that swings faster than
    /// the columns stays joined up.
    fn draw_columns(&self, picture: &mut Picture, lane: &Lane, channels: &Range<usize>) {
        let length = self.analysis.frames();
        let width = picture.width;
        for x in 0..width {
            let from = self.frame_at(x);
            if from >= length {
                break;
            }
            let to = self.frame_at(x + 1).max(from + 1).min(length);
            let fold = self.fold(channels, from.saturating_sub(1)..to);
            if fold.is_empty() {
                continue;
            }
            let color = if self.plays(from) { WAVE } else { OUTSIDE };
            for y in lane.row(fold.max)..=lane.row(fold.min) {
                picture.pixels[y * width + x] = color;
            }
        }
    }

    /// Fewer frames than columns of pixels: a line from each frame to the next, two pixels thick,
    /// and a dot on each frame once they're far enough apart to tell apart.
    fn draw_lines(&self, picture: &mut Picture, lane: &Lane, channel: usize) {
        let length = self.analysis.frames();
        let width = picture.width as i64;
        let spacing = self.cell.0 as f64 / self.zoom as f64;
        // From the frame before the view to the one after it, so the line runs off both edges.
        let first = self.first.saturating_sub(1);
        let last = (self.frame_at(picture.width) + 1).min(length);
        let mut previous: Option<(i64, f64, bool)> = None;
        for frame in first..last {
            let x = self.x_of(frame);
            let y = lane.exact_row(self.value(channel, frame));
            let plays = self.plays(frame);
            if let Some((x0, y0, played)) = previous {
                let color = if played { WAVE } else { OUTSIDE };
                for at in x0.max(0)..x.min(width) {
                    let along = |at: i64| y0 + (y - y0) * (at - x0) as f64 / (x - x0).max(1) as f64;
                    let (a, b) = (along(at), along(at + 1));
                    let low = lane.clamp(a.min(b).round());
                    let high = lane.clamp(a.max(b).round()).max(low + 1);
                    let high = high.min(lane.top + lane.height - 1);
                    for row in low..=high {
                        picture.pixels[row * picture.width + at as usize] = color;
                    }
                }
            }
            if spacing >= DOT_SPACING && (0..width).contains(&x) {
                let color = if plays { WAVE } else { OUTSIDE };
                let row = lane.clamp(y.round()) as i64;
                for dy in -DOT_REACH..=DOT_REACH {
                    for dx in -DOT_REACH..=DOT_REACH {
                        let (px, py) = (x + dx, row + dy);
                        let inside = (0..width).contains(&px)
                            && (lane.top as i64..(lane.top + lane.height) as i64).contains(&py);
                        if inside {
                            picture.pixels[py as usize * picture.width + px as usize] = color;
                        }
                    }
                }
            }
            previous = Some((x, y, plays));
        }
    }

    /// Each slice's start, then the loop, the end and the start, each a line at its frame from top
    /// to bottom. The start is drawn last, so it shows where points meet, and the slices first,
    /// under the rest.
    fn draw_points(&self, picture: &mut Picture) {
        let mut lines: Vec<(u64, u8, usize)> = self
            .points
            .slices
            .iter()
            .map(|slice| (slice.start, SLICE, SLICE_PIXELS))
            .collect();
        if let Some(looped) = self.points.loop_ {
            lines.extend([
                (looped.start, LOOP, POINT_PIXELS),
                (looped.end, LOOP, POINT_PIXELS),
            ]);
        }
        if let Some(end) = self.points.end {
            lines.push((end, END, POINT_PIXELS));
        }
        lines.push((self.points.start, START, POINT_PIXELS));
        let width = picture.width as i64;
        for (frame, color, pixels) in lines {
            let x = self.x_of(frame);
            for at in x..x + pixels as i64 {
                if (0..width).contains(&at) {
                    for row in 0..picture.height {
                        picture.pixels[row * picture.width + at as usize] = color;
                    }
                }
            }
        }
    }
}

/// One channel's band of the picture.
struct Lane {
    top: usize,
    height: usize,
}

impl Lane {
    /// How far full scale reaches from zero, in pixels, leaving a pixel at the top and bottom.
    fn reach(&self) -> f64 {
        (self.height as f64 / 2.0 - 1.0).max(0.0)
    }

    /// The row a sample is at, as a fraction, with full scale near the lane's edges.
    fn exact_row(&self, sample: f32) -> f64 {
        let middle = self.top as f64 + (self.height / 2) as f64;
        middle - f64::from(sample.clamp(-1.0, 1.0)) * self.reach()
    }

    fn clamp(&self, row: f64) -> usize {
        (row.max(self.top as f64) as usize).min(self.top + self.height - 1)
    }

    /// The row a sample is at.
    fn row(&self, sample: f32) -> usize {
        self.clamp(self.exact_row(sample).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::bank::{ContentHash, Loop};
    use vt_model::sample::Decoded;

    /// A second of a slow sine at 48 kHz, in stereo with the right channel at half the left.
    fn sine() -> (Analysis, SampleData) {
        let samples: Vec<f32> = (0..48_000)
            .flat_map(|at| {
                let value = (at as f32 * std::f32::consts::TAU / 4800.0).sin();
                [value, value / 2.0]
            })
            .collect();
        let decoded = Decoded {
            samples: samples.clone(),
            channels: 2,
            rate: 48_000,
        };
        let data = SampleData {
            frames: samples.as_chunks::<2>().0.to_vec(),
        };
        (Analysis::of(ContentHash(1), &decoded), data)
    }

    fn column(picture: &Picture, x: usize) -> Vec<u8> {
        (0..picture.height)
            .map(|y| picture.pixels[y * picture.width + x])
            .collect()
    }

    #[test]
    fn each_point_is_a_line_at_its_frame_in_its_colour() {
        let (analysis, data) = sine();
        let mut points = Points {
            start: 1000,
            end: Some(30_000),
            loop_: Some(Loop {
                start: 10_000,
                end: 20_000,
                crossfade: 0,
            }),
            ..Points::default()
        };
        points.reslice(&[1000, 5000, 20_000, 25_000]);
        // 100 frames a column of 10 pixels: 10 frames a pixel.
        let drawing = Drawing {
            analysis: &analysis,
            frames: Some((&data, Rates::SAME)),
            points: &points,
            first: 0,
            zoom: 100,
            columns: 400,
            rows: 2,
            cell: (10, 20),
            merged: false,
        };
        let picture = drawing.picture();
        assert_eq!((picture.width, picture.height), (4000, 40));
        for (frame, color) in [(1000, START), (10_000, LOOP), (20_000, LOOP), (30_000, END)] {
            let x = frame / 10;
            assert!(column(&picture, x).iter().all(|pixel| *pixel == color));
            assert!(column(&picture, x + 1).iter().all(|pixel| *pixel == color));
            assert!(!column(&picture, x + 2).contains(&color), "{frame}");
        }
        // A slice's line is a pixel wide, and under the points where they meet.
        assert!(column(&picture, 500).iter().all(|pixel| *pixel == SLICE));
        assert!(!column(&picture, 501).contains(&SLICE));
        assert!(column(&picture, 2500).iter().all(|pixel| *pixel == SLICE));
        assert!(
            !column(&picture, 2000).contains(&SLICE),
            "the loop's end is over it"
        );
        assert!(
            !column(&picture, 100).contains(&SLICE),
            "the start is over the first"
        );
        // Before the start, the sound doesn't play, and after it, it does.
        assert!(column(&picture, 50).contains(&OUTSIDE));
        assert!(!column(&picture, 50).contains(&WAVE));
        assert!(column(&picture, 600).contains(&WAVE));
        // Each lane has its zero line, seen where the wave is near its peak.
        assert_eq!(picture.pixels[10 * 4000 + 120], ZERO);
        assert_eq!(picture.pixels[30 * 4000 + 120], ZERO);
    }

    #[test]
    fn left_is_above_right_at_its_own_level() {
        let (analysis, data) = sine();
        let points = Points::default();
        let drawing = Drawing {
            analysis: &analysis,
            frames: Some((&data, Rates::SAME)),
            points: &points,
            first: 0,
            zoom: 48_000,
            columns: 1,
            rows: 4,
            cell: (10, 25),
            merged: false,
        };
        let picture = drawing.picture();
        // A column of pixels covering whole cycles reaches near both edges of the left's lane,
        // and half as far in the right's.
        let pixels = column(&picture, 5);
        let reach = |lane: Range<usize>| {
            let lit: Vec<usize> = lane.filter(|y| pixels[*y] == WAVE).collect();
            lit.last().unwrap() - lit.first().unwrap()
        };
        let (left, right) = (reach(0..50), reach(50..100));
        assert!(left >= 46, "{left}");
        assert!(
            (right as f64 - left as f64 / 2.0).abs() <= 2.0,
            "{left} {right}"
        );
    }

    #[test]
    fn closer_than_a_frame_a_pixel_it_is_lines_with_a_dot_on_each_frame() {
        let (analysis, data) = sine();
        let points = Points::default();
        // One frame a column of 15 pixels, as on the author's screen at the closest zoom.
        let drawing = Drawing {
            analysis: &analysis,
            frames: Some((&data, Rates::SAME)),
            points: &points,
            first: 2000,
            zoom: 1,
            columns: 20,
            rows: 2,
            cell: (15, 30),
            merged: false,
        };
        let picture = drawing.picture();
        // Every column of pixels has some of the line in it, with nothing missing between frames.
        for x in 0..picture.width {
            let lit = column(&picture, x)[..30].contains(&WAVE);
            assert!(lit, "column {x}");
        }
        // A dot is five pixels wide at each frame.
        let row = (0..30)
            .find(|y| picture.pixels[y * picture.width + 15] == WAVE)
            .unwrap();
        let dot = (13..18)
            .filter(|x| picture.pixels[row * picture.width + x] == WAVE)
            .count();
        assert_eq!(dot, 5);
    }

    #[test]
    fn past_the_files_end_is_left_clear() {
        let (analysis, data) = sine();
        let points = Points::default();
        let drawing = Drawing {
            analysis: &analysis,
            frames: Some((&data, Rates::SAME)),
            points: &points,
            first: 0,
            zoom: 1000,
            columns: 60,
            rows: 1,
            cell: (4, 10),
            merged: true,
        };
        let picture = drawing.picture();
        // 48 columns of the 60 hold the file, and the overview has no zero line.
        assert!(column(&picture, 48 * 4 - 1).contains(&WAVE));
        assert!(column(&picture, 48 * 4 + 2).iter().all(|pixel| *pixel == 0));
    }
}
