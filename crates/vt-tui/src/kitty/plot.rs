//! Curves drawn at the screen's own pixels, for the track page's graphic panels: a smooth line,
//! from how much of each pixel it covers, over faint gridlines. Its PNG has a palette with an
//! alpha for each entry, so a pixel the line only grazes is the line's colour, part see-through,
//! and nothing is stretched, as the picture is the size of the characters it covers.
//!
//! Every curve is drawn here: the compressor's, the EQ's and, zoomed, a distortion's and the
//! filter effects' on the FX page, the filter's on the Sound page, and the envelopes and the LFOs'
//! cycles on the Mod page, with the dot that rides an LFO's.

use std::cmp::Ordering;

use vt_engine::dsp::dynamics::compressor::curve_db;
use vt_engine::dsp::dynamics::eq::response_db as eq_response_db;
use vt_engine::dsp::effects::distort::transfer;
use vt_engine::dsp::filters::filter::track_response_db;
use vt_model::attractor::{Attractor, System, view};
use vt_model::envelope::{Adsr, Envelope};
use vt_model::insert::{Compressor, Distort, Equaliser, threshold_db};
use vt_model::lfo::{self, Lfo, LfoMode, deformed_wave};
use vt_model::macros::{MacroMap, response};
use vt_model::modulation::{ModSlot, Polarity};
use vt_model::song::{FilterType, Track};
use vt_model::tracking::{Bounds, Source};

use super::encoding::indexed_png;
use super::panels::FilterSettings;
use super::rgb;
use crate::theme;
use crate::views::curve::{
    ahd_level_at, ahd_stage_ends, drum_level_at, drum_stage_ends, level_at, lpg_level_at,
    lpg_stage_ends, stage_ends,
};

/// How wide the line is, in pixels.
const LINE_PIXELS: f32 = 2.0;
/// How many points are worked out along each pixel across, so a narrow peak between two isn't
/// missed.
const POINTS_PER_PIXEL: usize = 2;
/// How finely a pixel's share of the line is kept: the line's colour at 1/8 to 8/8, which are
/// its palette's entries 1 to 8.
pub(super) const STEPS: u8 = 8;
/// The palette entries of a gridline and of a fainter one, after the line's.
const GRID: u8 = STEPS + 1;
const FAINTER: u8 = STEPS + 2;
/// How solid each is, out of 255: about 55% and 35%, so they're there without asking to be read.
const GRID_ALPHA: u8 = 140;
const FAINTER_ALPHA: u8 = 90;
/// A dotted line's pixels on and off, and a dashed one's.
const DOT: (usize, usize) = (2, 2);
const DASH: (usize, usize) = (5, 3);

/// What a compressor's curve covers, in dB, both in and out, from `0` at the top and right. It's
/// as far down as the threshold goes.
pub const DYNAMICS_RANGE_DB: f32 = 60.0;
/// The levels a compressor's gridlines are at, in dB, across and down.
const DYNAMICS_GRID_DB: [f32; 2] = [-20.0, -40.0];
/// How far up and down an EQ's curve goes, in dB.
pub const EQ_RANGE_DB: f32 = 18.0;
/// The frequencies a frequency plot marks, in Hz.
const DECADES_HZ: [f32; 3] = [100.0, 1000.0, 10_000.0];
/// What a filter curve's height covers, in dB, with the frequency across it from 20 Hz to 20 kHz.
/// It has more room below than above, as a filter takes away more than it adds.
pub const CURVE_BOTTOM_DB: f32 = -36.0;
pub const CURVE_TOP_DB: f32 = 24.0;

/// How a panel's picture is shown: dim, brighter while the cursor is in its device, or faint
/// while the device is switched out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shade {
    Dim,
    Focused,
    Faint,
}

impl Shade {
    /// The shade for a device with the cursor in it or not, switched out or not, which wins.
    pub fn of(focused: bool, faint: bool) -> Self {
        match (focused, faint) {
            (_, true) => Self::Faint,
            (true, false) => Self::Focused,
            (false, false) => Self::Dim,
        }
    }

    /// The line's colour.
    pub fn color(self) -> [u8; 3] {
        let style = match self {
            Self::Dim => theme::dim(),
            Self::Focused => theme::active(),
            Self::Faint => theme::faint(),
        };
        rgb(style.fg.unwrap_or(theme::TEXT))
    }
}

/// How strong a gridline is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Grid,
    Fainter,
}

impl Mark {
    fn entry(self) -> u8 {
        match self {
            Self::Grid => GRID,
            Self::Fainter => FAINTER,
        }
    }
}

/// A picture of a curve over gridlines, before it's a PNG.
#[derive(Debug, Clone)]
pub struct Plot {
    pub width: usize,
    pub height: usize,
    /// How much of each pixel the line covers, in eighths, row by row from the top left.
    pub cover: Vec<u8>,
    /// Each pixel's gridline, if it has one: its palette entry, or `0`.
    marks: Vec<u8>,
}

impl Plot {
    /// Nothing yet, this many pixels across and down.
    pub fn new(width: usize, height: usize) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        Self {
            width,
            height,
            cover: vec![0; width * height],
            marks: vec![0; width * height],
        }
    }

    /// Where a point falls in pixels, at `x` from 0 to 1 across and `level` from 0 at the bottom
    /// to 1 at the top, as [`point_at`] has it.
    pub fn at(&self, x: f32, level: f32) -> (f32, f32) {
        point_at((self.width, self.height), x, level)
    }

    /// The pixel a point in pixels is in, if it's in the picture.
    fn pixel(&self, (x, y): (f32, f32)) -> Option<usize> {
        let (x, y) = (x.floor(), y.floor());
        let inside =
            x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height;
        inside.then(|| y as usize * self.width + x as usize)
    }

    /// A line through points in pixels, `LINE_PIXELS` wide. Each pixel near it is covered by as
    /// much as the nearest segment is in it, which keeps its edges smooth.
    pub fn line(&mut self, points: &[(f32, f32)]) {
        let reach = LINE_PIXELS / 2.0 + 0.5;
        for pair in points.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let range = |a: f32, b: f32, most: usize| {
                let low = (a.min(b) - reach).floor().max(0.0) as usize;
                let high = ((a.max(b) + reach).ceil().max(0.0) as usize).min(most);
                low..high
            };
            for y in range(from.1, to.1, self.height) {
                for x in range(from.0, to.0, self.width) {
                    let centre = (x as f32 + 0.5, y as f32 + 0.5);
                    let share = (reach - distance(centre, from, to)).clamp(0.0, 1.0);
                    let steps = (share * f32::from(STEPS)).round() as u8;
                    let cover = &mut self.cover[y * self.width + x];
                    *cover = (*cover).max(steps);
                }
            }
        }
    }

    /// A curve of `level_at` across the picture, each `x` from 0 to 1, starting from `start` at
    /// the left edge, with a point at each of `corners` too, so a sharp one stays sharp.
    pub fn curve(&mut self, level_at: impl Fn(f32) -> f32, start: f32, corners: &[f32]) {
        let count = self.width * POINTS_PER_PIXEL;
        let mut across: Vec<f32> = (0..=count).map(|step| step as f32 / count as f32).collect();
        across.extend(corners.iter().filter(|x| (0.0..=1.0).contains(*x)));
        across.sort_by(f32::total_cmp);
        let mut points = vec![self.at(0.0, start)];
        points.extend(across.into_iter().map(|x| self.at(x, level_at(x))));
        self.line(&points);
    }

    /// A gridline from one point to another, each `(x, level)` from 0 to 1, a pixel wide, in
    /// pieces on and off.
    fn broken(&mut self, from: (f32, f32), to: (f32, f32), mark: Mark, (on, off): (usize, usize)) {
        let (start, end) = (self.at(from.0, from.1), self.at(to.0, to.1));
        let length = (end.0 - start.0).hypot(end.1 - start.1).round() as usize;
        for step in 0..=length {
            if step % (on + off) >= on {
                continue;
            }
            let share = step as f32 / length.max(1) as f32;
            let point = (
                start.0 + (end.0 - start.0) * share,
                start.1 + (end.1 - start.1) * share,
            );
            if let Some(pixel) = self.pixel(point) {
                self.marks[pixel] = mark.entry();
            }
        }
    }

    /// A dotted gridline across the whole picture at a level.
    pub fn across(&mut self, level: f32, mark: Mark) {
        self.broken((0.0, level), (1.0, level), mark, DOT);
    }

    /// A dotted gridline down the whole picture at `x`.
    pub fn down(&mut self, x: f32, mark: Mark) {
        self.broken((x, 1.0), (x, 0.0), mark, DOT);
    }

    /// A dotted gridline from one point to another, each `(x, level)`.
    pub fn dotted(&mut self, from: (f32, f32), to: (f32, f32), mark: Mark) {
        self.broken(from, to, mark, DOT);
    }

    /// A dotted gridline along a curve of `level_at` across the picture, each `x` from 0 to 1, its
    /// dots spaced along the curve, as a straight one's are.
    pub fn traced(&mut self, level_at: impl Fn(f32) -> f32, mark: Mark) {
        let (on, off) = DOT;
        let mut along = 0.0_f32;
        let mut last = self.at(0.0, level_at(0.0));
        for step in 1..=self.width * POINTS_PER_PIXEL {
            let x = step as f32 / (self.width * POINTS_PER_PIXEL) as f32;
            let next = self.at(x, level_at(x));
            let length = (next.0 - last.0).hypot(next.1 - last.1);
            let pieces = length.ceil().max(1.0) as usize;
            for piece in 0..pieces {
                let share = piece as f32 / pieces as f32;
                let point = (
                    last.0 + (next.0 - last.0) * share,
                    last.1 + (next.1 - last.1) * share,
                );
                let place = (along + length * share) as usize;
                if place % (on + off) < on
                    && let Some(pixel) = self.pixel(point)
                {
                    self.marks[pixel] = mark.entry();
                }
            }
            along += length;
            last = next;
        }
    }

    /// A dashed gridline up from the bottom at `x`, to a level.
    pub fn dashed_up(&mut self, x: f32, level: f32, mark: Mark) {
        self.broken((x, 0.0), (x, level), mark, DASH);
    }

    /// Each pixel's palette entry: the line where it covers any of the pixel, over a gridline.
    pub fn pixels(&self) -> Vec<u8> {
        self.cover
            .iter()
            .zip(&self.marks)
            .map(|(cover, mark)| if *cover > 0 { *cover } else { *mark })
            .collect()
    }

    /// As a PNG with the line in a colour and the gridlines faint. It's always compressed: a
    /// mostly empty picture comes to a few kilobytes.
    pub fn png(&self, color: [u8; 3]) -> Vec<u8> {
        let grid = rgb(theme::OVERLAY0);
        let mut colors = vec![color; usize::from(STEPS)];
        colors.extend([grid, grid]);
        let mut alphas = vec![0];
        alphas.extend((1..=u16::from(STEPS)).map(|step| (step * 255 / u16::from(STEPS)) as u8));
        alphas.extend([GRID_ALPHA, FAINTER_ALPHA]);
        indexed_png(self.width, &self.pixels(), &colors, &alphas, true)
    }
}

/// Where a point falls in pixels in a picture this many across and down, at `x` from 0 to 1 across
/// and `level` from 0 at the bottom to 1 at the top. The line's centre is kept half its width in
/// from the edges, so it's whole at the top and bottom.
pub fn point_at((width, height): (usize, usize), x: f32, level: f32) -> (f32, f32) {
    let margin = LINE_PIXELS / 2.0;
    let across = margin + x.clamp(0.0, 1.0) * (width.max(1) as f32 - 2.0 * margin);
    let down = margin + (1.0 - level.clamp(0.0, 1.0)) * (height.max(1) as f32 - 2.0 * margin);
    (across, down)
}

/// How far a point is from the nearest point of a segment.
fn distance(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let along = (to.0 - from.0, to.1 - from.1);
    let length = along.0 * along.0 + along.1 * along.1;
    let share = if length > 0.0 {
        (((point.0 - from.0) * along.0 + (point.1 - from.1) * along.1) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let nearest = (from.0 + along.0 * share, from.1 + along.1 * share);
    (point.0 - nearest.0).hypot(point.1 - nearest.1)
}

/// Where a frequency is across a frequency plot, from 20 Hz at 0 to 20 kHz at 1, evenly in pitch.
fn frequency_x(hz: f32) -> f32 {
    (hz / 20.0).log10() / 3.0
}

/// The frequency at `x` across a frequency plot.
pub fn frequency_at(x: f32) -> f32 {
    20.0 * 1000_f32.powf(x)
}

/// Where a level in dB is on a compressor's curve, in or out, from 0 at -60 dB to 1 at 0.
pub fn dynamics_level(db: f32) -> f32 {
    db / DYNAMICS_RANGE_DB + 1.0
}

/// A compressor's static curve, this many pixels across and down: how loud a steady level comes
/// out, against how loud it went in, each from -60 dB to 0, as its own gain computer works it out,
/// knee and all. Under it, faint, are gridlines at -20 and -40 dB both ways, the 1:1 diagonal, so
/// how far the curve bends from it is the compression, and a dashed line up to the curve at the
/// threshold.
pub fn compressor(settings: Compressor, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    for db in DYNAMICS_GRID_DB {
        plot.across(dynamics_level(db), Mark::Grid);
        plot.down(dynamics_level(db), Mark::Grid);
    }
    plot.dotted((0.0, 0.0), (1.0, 1.0), Mark::Grid);
    let threshold = threshold_db(f32::from(settings.threshold));
    let bent = dynamics_level(curve_db(threshold, settings));
    plot.dashed_up(dynamics_level(threshold), bent, Mark::Grid);
    let level = |x: f32| dynamics_level(curve_db(DYNAMICS_RANGE_DB * (x - 1.0), settings));
    plot.curve(level, level(0.0), &[]);
    plot
}

/// Dotted lines down a frequency plot at 100 Hz, 1 kHz and 10 kHz.
fn decades(plot: &mut Plot) {
    for hz in DECADES_HZ {
        plot.down(frequency_x(hz), Mark::Fainter);
    }
}

/// Where a level in dB is on an EQ's curve, from 0 at -18 dB to 1 at +18 dB.
pub fn eq_level(db: f32) -> f32 {
    db / (2.0 * EQ_RANGE_DB) + 0.5
}

/// An EQ's curve: what it does at each frequency, from 20 Hz to 20 kHz across it, as the filter's
/// curve is drawn, and from -18 dB to +18 dB up it, a little more than a band can do, over a
/// dotted line at 0 dB and fainter ones down it at 100 Hz, 1 kHz and 10 kHz.
pub fn eq(settings: Equaliser, rate: u32, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    decades(&mut plot);
    plot.across(eq_level(0.0), Mark::Grid);
    let level = |x: f32| eq_level(eq_response_db(settings, rate, frequency_at(x)));
    plot.curve(level, level(0.0), &[]);
    plot
}

/// A distortion's transfer curve, this many pixels across and down: what a steady level from -1
/// to 1 comes out as, across, from -1 to 1 up, as [`transfer`] has it, `Mix` and all, over dotted
/// lines through nothing both ways and a fainter 1:1 diagonal, so how far the curve leaves it is
/// what the distortion does. A type without a curve draws its gridlines alone.
pub fn distort(settings: Distort, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    plot.across(0.5, Mark::Grid);
    plot.down(0.5, Mark::Grid);
    plot.dotted((0.0, 0.0), (1.0, 1.0), Mark::Fainter);
    if transfer(settings, 0.0).is_none() {
        return plot;
    }
    let level = |x: f32| transfer(settings, 2.0 * x - 1.0).map_or(0.5, |out| 0.5 + 0.5 * out);
    plot.curve(level, level(0.0), &[]);
    plot
}

/// Where a level in dB is on a filter's curve, from 0 at its bottom to 1 at its top.
pub fn filter_level(db: f32) -> f32 {
    (db - CURVE_BOTTOM_DB) / (CURVE_TOP_DB - CURVE_BOTTOM_DB)
}

/// The filter curve a panel's settings say, as [`filter`] draws it for a track with them.
pub(crate) fn filter_from(settings: FilterSettings, rate: u32, size: (usize, usize)) -> Plot {
    let (filter, filter_mode, cutoff, resonance, drive, safe, low_cut, high_cut) = settings;
    let track = Track {
        filter,
        filter_mode,
        cutoff,
        resonance,
        drive,
        safe,
        low_cut,
        high_cut,
        ..Track::default()
    };
    self::filter(&track, rate, size)
}

/// A track's filter and utility filter: what they let through at each frequency, from 20 Hz to
/// 20 kHz across and -36 dB to +24 dB up, over a dotted line at 0 dB and fainter ones down it at
/// 100 Hz, 1 kHz and 10 kHz, as the EQ's is. A dashed line rises from the bottom to the curve at
/// `Cut`, as the compressor's does at its threshold, unless the filter is open and out of the way.
pub fn filter(track: &Track, rate: u32, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    decades(&mut plot);
    plot.across(filter_level(0.0), Mark::Grid);
    let level = |x: f32| filter_level(track_response_db(track, rate, frequency_at(x)));
    if !filter_open(track) {
        // `Cut` is evenly in pitch from 20 Hz to 20 kHz, as the plot is across.
        let cut = f32::from(track.cutoff) / 255.0;
        plot.dashed_up(cut, level(cut), Mark::Grid);
    }
    plot.curve(level, level(0.0), &[]);
    plot
}

/// Whether a track's filter has nothing to do, so it's out of the way, as it is when playing.
fn filter_open(track: &Track) -> bool {
    track.filter == FilterType::LowPass
        && track.cutoff == u8::MAX
        && track.resonance == 0
        && track.drive == 0
}

/// An envelope's shape, from silence, with a point at the end of each stage so its corners are
/// sharp, over a dotted baseline and fainter dotted lines down it between the stages, so each of
/// its times has a part of the picture to point at.
pub fn envelope(envelope: &Adsr, size: (usize, usize)) -> Plot {
    envelope_in(&Envelope::Adsr(*envelope), Polarity::Uni, size)
}

/// An envelope's shape in a polarity: over its baseline one way, or its centre line both ways, as
/// what it moves is then at rest half way up. An AHD's hold is a stage of its own, with its lines
/// either side.
pub fn envelope_in(
    envelope: &Envelope,
    polarity: Polarity,
    (width, height): (usize, usize),
) -> Plot {
    let mut plot = Plot::new(width, height);
    let rest = match polarity {
        Polarity::Uni => 0.0,
        Polarity::Bi => 0.5,
    };
    plot.across(rest, Mark::Grid);
    let ends: Vec<f32> = match envelope {
        Envelope::Adsr(adsr) => stage_ends(adsr).to_vec(),
        Envelope::Ahd(ahd) => ahd_stage_ends(ahd).to_vec(),
        Envelope::Drum(drum) => drum_stage_ends(drum),
        Envelope::Lpg(lpg) => lpg_stage_ends(lpg).to_vec(),
    };
    // Between each stage and the next: attack and decay, decay and sustain, and sustain and
    // release, or attack and hold, and hold and decay.
    for end in &ends[..ends.len() - 1] {
        plot.down(*end, Mark::Fainter);
    }
    match envelope {
        Envelope::Adsr(adsr) => plot.curve(|x| level_at(adsr, x), 0.0, &ends),
        Envelope::Ahd(ahd) => plot.curve(|x| ahd_level_at(ahd, x), 0.0, &ends),
        // It strikes from nothing to full at once, so its line goes straight up at the start.
        Envelope::Drum(drum) => plot.curve(|x| drum_level_at(drum, x), 0.0, &ends),
        Envelope::Lpg(lpg) => plot.curve(|x| lpg_level_at(lpg, x), 0.0, &ends),
    }
    plot
}

/// How many times a peak found among a curve's points is narrowed down between its neighbours,
/// each time to 0.618 of the way: to well under a thousandth of a pixel.
const PEAK_STEPS: usize = 24;

/// Where a curve across a picture `width` pixels wide peaks, each `x` from 0 to 1: every point of
/// it worked out, as [`Plot::curve`] works them out, that's higher than both of its neighbours,
/// found again more closely between them. A comb's or a resonator's peak can be narrower than the
/// two points a pixel the curve has, and so would be drawn short of its top, or not at all.
pub fn peaks(level_at: &impl Fn(f32) -> f32, width: usize) -> Vec<f32> {
    let count = width * POINTS_PER_PIXEL;
    let x = |step: usize| step as f32 / count as f32;
    let levels: Vec<f32> = (0..=count).map(|step| level_at(x(step))).collect();
    let golden = (5.0_f32.sqrt() - 1.0) / 2.0;
    (1..count)
        .filter(|&step| levels[step] > levels[step - 1] && levels[step] >= levels[step + 1])
        .map(|step| {
            // A golden-section search for the top between the neighbours.
            let (mut low, mut high) = (x(step - 1), x(step + 1));
            for _ in 0..PEAK_STEPS {
                let one = high - golden * (high - low);
                let other = low + golden * (high - low);
                if level_at(one) < level_at(other) {
                    low = one;
                } else {
                    high = other;
                }
            }
            (low + high) / 2.0
        })
        .collect()
}

/// A slot effect that filters, as its curve on the FX page's zoom is drawn from (decision 333):
/// what it does to a quiet sound at each frequency, in dB, and where its dashed line goes up, at
/// the note it's tuned to or the cutoff it's at. A low-pass gate's has how far it opens too.
pub struct Filtering<F, G> {
    pub db_at: F,
    pub marked: Option<f32>,
    pub opened: Option<G>,
}

/// A slot effect's frequency curve, from 20 Hz to 20 kHz across and -36 dB to +24 dB up, as the
/// Sound page's filter curve is, over a dotted line at 0 dB and fainter ones down it at 100 Hz,
/// 1 kHz and 10 kHz. Its peaks are drawn to their tops however narrow, a dashed line rises to it
/// at the frequency marked, and a low-pass gate's curve opened all the way is a fainter dotted
/// curve, so what lies between is how far it moves.
pub fn filtering<F: Fn(f32) -> f32, G: Fn(f32) -> f32>(
    filtering: Filtering<F, G>,
    (width, height): (usize, usize),
) -> Plot {
    let mut plot = Plot::new(width, height);
    decades(&mut plot);
    plot.across(filter_level(0.0), Mark::Grid);
    if let Some(opened) = &filtering.opened {
        plot.traced(|x| filter_level(opened(frequency_at(x))), Mark::Fainter);
    }
    let level = |x: f32| filter_level((filtering.db_at)(frequency_at(x)));
    if let Some(hertz) = filtering.marked.filter(|hz| (20.0..=20_000.0).contains(hz)) {
        let x = frequency_x(hertz);
        plot.dashed_up(x, level(x), Mark::Grid);
    }
    let corners = peaks(&level, width);
    plot.curve(level, level(0.0), &corners);
    plot
}

/// How far down a delay's repeats and a reverb's tail are drawn, in dB: as far as a reverb's
/// decay time measures.
pub const TIME_RANGE_DB: f32 = 60.0;

/// Where a level, as a gain, is up a time plot, from 0 at -60 dB to 1 at 0 dB and over.
fn time_level(gain: f32) -> f32 {
    (1.0 + 20.0 * gain.max(1e-9).log10() / TIME_RANGE_DB).clamp(0.0, 1.0)
}

/// A delay's repeats over `span` seconds across, each a line up from the middle, as loud as it is
/// on the left, and down from it, as loud as it is on the right, from nothing at -60 dB in the
/// middle to 0 dB at the top and bottom: a ping-pong's cross from side to side and a straight
/// one's stand both ways. The sound itself, with `Mix`, is the first at the left. Fainter dotted
/// lines down it mark the beats, `beat` seconds apart, so the repeats can be read against them.
pub fn repeats(sounds: &[(f32, [f32; 2])], beat: f32, span: f32, size: (usize, usize)) -> Plot {
    let mut plot = Plot::new(size.0, size.1);
    plot.across(0.5, Mark::Grid);
    let span = span.max(1e-3);
    if beat > 0.0 {
        let mut at = beat;
        while at < span {
            plot.down(at / span, Mark::Fainter);
            at += beat;
        }
    }
    for &(seconds, [left, right]) in sounds {
        let x = seconds / span;
        let up = 0.5 + 0.5 * time_level(left);
        let down = 0.5 - 0.5 * time_level(right);
        if up > 0.5 || down < 0.5 {
            let points = [plot.at(x, up), plot.at(x, down)];
            plot.line(&points);
        }
    }
    plot
}

/// A reverb's tail over `span` seconds across, from 0 dB at the top to -60 dB at the bottom:
/// nothing until its pre-delay, `pre`, then falling in a straight line, `seconds[0]` to fall all
/// the way at its lows, with a dotted line falling as its highs do, in `seconds[1]`. Dotted lines
/// across at -20 and -40 dB, as the compressor's, and fainter ones down it every `every` seconds.
pub fn tail(pre: f32, seconds: [f32; 2], span: f32, every: f32, size: (usize, usize)) -> Plot {
    let mut plot = Plot::new(size.0, size.1);
    let span = span.max(1e-3);
    for db in DYNAMICS_GRID_DB {
        plot.across(1.0 + db / TIME_RANGE_DB, Mark::Grid);
    }
    if every > 0.0 {
        let mut at = every;
        while at < span {
            plot.down(at / span, Mark::Fainter);
            at += every;
        }
    }
    let start = pre / span;
    // Where a fall `seconds` long reaches the bottom, or the right edge on its way.
    let fall = |seconds: f32| {
        let end = (pre + seconds) / span;
        if end <= 1.0 {
            (end, 0.0)
        } else {
            (1.0, 1.0 - (1.0 - start) / (end - start))
        }
    };
    let highs = fall(seconds[1]);
    plot.dotted((start, 1.0), highs, Mark::Grid);
    let lows = fall(seconds[0]);
    let points = [(0.0, 0.0), (start, 0.0), (start, 1.0), lows].map(|(x, level)| plot.at(x, level));
    plot.line(&points);
    plot
}

/// How far either side of a jump in a wave its two points are, as a share of the cycle, so the
/// jump is drawn straight up or down.
const JUMP: f32 = 1e-4;

/// What an LFO's cycle is drawn from: its wave, where in it the cycle starts, in degrees, and
/// whether a negative depth turns it upside down. Its rate and the size of its depth aren't in
/// it, so the picture never changes with them: the dot's speed shows the rate, and the depth's
/// number is beside it. The random waves draw only their centre line, so neither their phase nor
/// their sign is in it. One way only, its dotted line is where it's at rest, the bottom of the
/// wave, rather than its centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LfoDrawing {
    pub shape: lfo::Shape,
    pub phase: u16,
    pub flipped: bool,
    pub unipolar: bool,
    /// How far its wave is bent, as its `Deform` says.
    pub deform: i16,
    /// Whether it plays only half its cycle from each note, as `Half` does, then stays: the
    /// picture draws that half, then a flat line where it stays.
    pub half: bool,
}

impl LfoDrawing {
    /// What an LFO's settings draw, with the depth of its slot.
    pub fn of(lfo: &Lfo, depth: i16) -> Self {
        if lfo.shape.is_attractor() {
            // Its phase is the axis heard, and its deform its parameter, which both change it.
            Self {
                shape: lfo.shape,
                phase: lfo.phase % 360,
                flipped: depth < 0,
                unipolar: false,
                deform: lfo.deform,
                half: false,
            }
        } else if lfo_draws_its_wave(lfo.shape) {
            Self {
                shape: lfo.shape,
                phase: lfo.phase % 360,
                flipped: depth < 0,
                unipolar: false,
                deform: lfo.deform,
                half: lfo.mode == LfoMode::Half,
            }
        } else {
            Self {
                shape: lfo.shape,
                phase: 0,
                flipped: false,
                unipolar: false,
                deform: 0,
                half: false,
            }
        }
    }

    /// How much of the cycle drawn is its wave, before any flat line where it stays.
    fn span(self) -> f32 {
        if self.half { 0.5 } else { 1.0 }
    }

    /// The same, in a polarity, which moves its dotted line.
    #[must_use]
    pub fn in_polarity(self, polarity: Polarity) -> Self {
        Self {
            unipolar: polarity == Polarity::Uni,
            ..self
        }
    }

    /// How far round its wave the cycle starts, from 0 to 1.
    fn start(self) -> f64 {
        f64::from(self.phase) / 360.0
    }

    /// Where a wave's value, from -1 to 1, is up the picture, from 0 at the bottom to 1 at the top,
    /// upside down for a negative depth.
    pub fn level_of(self, value: f32) -> f32 {
        let value = if self.flipped { -value } else { value };
        0.5 + 0.5 * value.clamp(-1.0, 1.0)
    }

    /// The cycle's level at `x` from 0 to 1 across: the engine's own wave, from the cycle's start.
    /// The right edge has the end of the cycle, rather than the start of the next, so a saw's fall
    /// isn't drawn up the edge. A half cycle stays where its half ends, just short of it, as the
    /// engine does.
    pub fn level_at(self, x: f32) -> f32 {
        let turns = f64::from(x.min(self.span() - JUMP)) + self.start();
        self.level_of(deformed_wave(self.shape, turns, 0, self.deform_share()))
    }

    /// Its deform from -1 to 1, as the engine bends the wave.
    fn deform_share(self) -> f32 {
        f32::from(self.deform) / f32::from(lfo::MAX_DEFORM)
    }

    /// Where a point so far round the wave, from 0 to 1, is across the cycle drawn. An attractor's
    /// is already where it is across its picture, from 0 to 1.
    pub fn x_of(self, turns: f64) -> f32 {
        if self.shape.is_attractor() {
            return turns.clamp(0.0, 1.0) as f32;
        }
        (turns - self.start()).rem_euclid(1.0) as f32
    }

    /// Where the wave has a corner across the cycle: a point at each peak, and one either side of
    /// each jump, so the line goes straight up or down there.
    fn corners(self) -> Vec<f32> {
        // A deformed square falls where its width ends.
        let width = [0.0, 0.5 + 0.5 * f64::from(self.deform_share())];
        let (peaks, jumps): (&[f64], &[f64]) = match self.shape {
            lfo::Shape::Tri => (&[0.25, 0.75], &[]),
            lfo::Shape::Saw | lfo::Shape::Ramp => (&[], &[0.0]),
            lfo::Shape::Square => (&[], &width),
            _ => (&[], &[]),
        };
        let mut corners: Vec<f32> = peaks.iter().map(|turns| self.x_of(*turns)).collect();
        for jump in jumps.iter().map(|turns| self.x_of(*turns)) {
            if jump > 0.0 {
                corners.extend([jump - JUMP, jump]);
            }
        }
        // A half cycle has a corner where it stops, and none past it.
        if self.half {
            corners.retain(|x| *x < self.span() - JUMP);
            corners.push(self.span() - JUMP);
        }
        corners
    }
}

/// Whether an LFO's cycle draws its wave: every shape but the random ones, whose values aren't
/// known ahead, so drawing a made-up staircase would say something untrue.
pub fn lfo_draws_its_wave(shape: lfo::Shape) -> bool {
    !matches!(shape, lfo::Shape::Random | lfo::Shape::Smooth)
}

/// One cycle of an LFO, from its `Phase`, at full height, over a dotted centre line. A random one
/// has the centre line alone, and an attractor is traced side on, as [`attractor_trace`] has it.
pub fn lfo_cycle(drawing: LfoDrawing, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    // One way only, it's at rest where the wave is lowest, or highest flipped.
    let rest = if drawing.unipolar {
        drawing.level_of(-1.0)
    } else {
        0.5
    };
    plot.across(rest, Mark::Grid);
    if let Some(system) = System::of(drawing.shape) {
        let trace = attractor_trace(drawing, system);
        plot.line(&trace.map(|(x, level)| plot.at(x, level)));
    } else if lfo_draws_its_wave(drawing.shape) {
        let level = |x: f32| drawing.level_at(x);
        plot.curve(level, level(0.0), &drawing.corners());
    }
    plot
}

/// How many cycles of an attractor its picture traces, and how many points a cycle.
const TRACED_CYCLES: usize = 16;
const TRACED_POINTS: usize = 32;

/// An attractor as the Mod page draws it, each point `(x, level)` from 0 to 1: what's heard of it
/// up the picture, as a wave's value is, and the next axis round across it, so the picture is the
/// attractor itself seen side on, whose shape is the same wherever it started, and the dot rides
/// it where the LFO is. It traces 16 cycles from its seed, after settling, at its deform.
pub fn attractor_trace(
    drawing: LfoDrawing,
    system: System,
) -> [(f32, f32); TRACED_CYCLES * TRACED_POINTS] {
    let deform = drawing.deform_share();
    let turns = drawing.start();
    let mut attractor = Attractor::seeded(system, 0, deform, 0.0);
    std::array::from_fn(|at| {
        let cycles = at as f64 / TRACED_POINTS as f64;
        let reached = attractor.reach(cycles, deform);
        debug_assert!(reached, "a trace goes on a step at a time");
        let point = attractor.point(cycles, deform);
        (
            0.5 + 0.5 * view(point, turns + 1.0 / 3.0),
            drawing.level_of(view(point, turns)),
        )
    })
}

/// What a Track slot's map is drawn from: its source, where its map starts and ends, whether a
/// negative depth turns it upside down, and whether it's one way, which puts its dotted line where
/// it's at rest rather than half way. The size of its depth isn't in it, as the depth's number is
/// beside it, and the source's whole span is across the picture, so the map's place shows where in
/// the notes, the velocities or the rolls it rises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackDrawing {
    pub source: Source,
    pub bounds: Bounds,
    pub flipped: bool,
    pub unipolar: bool,
}

impl TrackDrawing {
    /// What a Track slot's settings draw.
    pub fn of(slot: &ModSlot) -> Self {
        Self {
            source: slot.track.source,
            bounds: slot.track.bounds(),
            flipped: slot.depth < 0,
            unipolar: slot.polarity() == Polarity::Uni,
        }
    }

    /// Where a value of its source is across the picture, from 0 to 1.
    pub fn x_of(self, value: f32) -> f32 {
        self.source.along(value)
    }

    /// Where a level of the map, from 0 to 1, is up the picture, upside down for a negative depth.
    pub fn level_of(self, level: f32) -> f32 {
        if self.flipped { 1.0 - level } else { level }
    }

    /// The map's level at `x` from 0 to 1 across the source's span.
    fn level_at(self, x: f32) -> f32 {
        let (lowest, highest) = self.source.span();
        let value = f32::from(lowest) + x * f32::from(highest - lowest);
        self.level_of(self.bounds.level(value))
    }
}

/// A Track slot's map across its source's span: flat to `Low`, rising to `High` and flat after,
/// with a faint line down at each and a dotted line where it's at rest, as an envelope's has.
pub fn track_map(drawing: TrackDrawing, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    let rest = if drawing.unipolar {
        drawing.level_of(0.0)
    } else {
        0.5
    };
    plot.across(rest, Mark::Grid);
    let low = drawing.x_of(f32::from(drawing.bounds.low));
    let high = drawing.x_of(f32::from(drawing.bounds.high));
    for x in [low, high] {
        plot.down(x, Mark::Fainter);
    }
    // The same twice is a switch, which goes straight up there.
    let level = |x: f32| drawing.level_at(x);
    plot.curve(level, level(0.0), &[low - JUMP, low, high - JUMP, high]);
    plot
}

/// What a macro mapping's response draws (decision 314): its `Curve` and `S`, which way it
/// turns its setting, and where the macro is turned, from 0 to 1, so it's drawn again only when
/// one of those changes, never for how far apart its ends are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MappingDrawing {
    pub curve: i8,
    pub s: i8,
    /// Which way its ends go: `Less` for a setting it turns up, `Greater` down.
    pub way: Ordering,
    pub turned: f32,
}

impl MappingDrawing {
    /// What a mapping with its macro at `value`, `0000` to `FF00`, draws.
    pub fn of(map: &MacroMap, value: u16) -> Self {
        Self {
            curve: map.curve,
            s: map.s,
            way: map.min.cmp(&map.max),
            turned: f32::from(value) / f32::from(0xFF00_u16),
        }
    }

    /// Where its setting is up the picture with its straight line's share `along` from its min
    /// to its max, from its lower end at the bottom to its higher at the top, or across the
    /// middle while its ends are the same.
    fn level_of(self, along: f32) -> f32 {
        match self.way {
            Ordering::Less => along,
            Ordering::Equal => 0.5,
            Ordering::Greater => 1.0 - along,
        }
    }

    /// Where its setting is up the picture with the macro `along` its turn.
    pub fn level_at(self, along: f32) -> f32 {
        self.level_of(response(along, self.curve, self.s))
    }
}

/// A macro mapping's response: the macro's turn across, from `00` at the left to `FF` at the
/// right, and its setting up, along its curve, over the straight line it bends, dotted, as the
/// compressor's 1:1 is, with a fainter line down where the macro is turned.
pub fn mapping(drawing: MappingDrawing, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    let line = |along: f32| drawing.level_of(along);
    plot.dotted((0.0, line(0.0)), (1.0, line(1.0)), Mark::Grid);
    plot.down(drawing.turned, Mark::Fainter);
    let level = |along: f32| drawing.level_at(along);
    plot.curve(level, level(0.0), &[]);
    plot
}

/// A round dot this many pixels across, smooth at its edge, as a PNG in a colour: what's placed on
/// an LFO's cycle where it is now.
pub fn dot(diameter: usize, color: [u8; 3]) -> Vec<u8> {
    let mut plot = Plot::new(diameter, diameter);
    let radius = diameter as f32 / 2.0;
    for y in 0..plot.height {
        for x in 0..plot.width {
            let from = (x as f32 + 0.5 - radius).hypot(y as f32 + 0.5 - radius);
            let share = (radius + 0.5 - from).clamp(0.0, 1.0);
            plot.cover[y * plot.width + x] = (share * f32::from(STEPS)).round() as u8;
        }
    }
    plot.png(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::envelope::Curve;

    /// Where the line's middle is down a column of pixels, weighting each by how much of it the
    /// line covers, if the line is in it.
    fn middle(plot: &Plot, x: usize) -> Option<f32> {
        let (mut weight, mut sum) = (0.0, 0.0);
        for y in 0..plot.height {
            let cover = f32::from(plot.cover[y * plot.width + x]);
            weight += cover;
            sum += cover * (y as f32 + 0.5);
        }
        (weight > 0.0).then(|| sum / weight)
    }

    #[test]
    fn a_line_is_two_pixels_wide_with_soft_edges() {
        let mut plot = Plot::new(40, 20);
        // On the line between two rows of pixels, it covers both, whole.
        plot.line(&[(0.0, 10.0), (40.0, 10.0)]);
        let column: Vec<u8> = (0..20).map(|y| plot.cover[y * 40 + 20]).collect();
        assert_eq!(&column[8..12], [0, 8, 8, 0]);
        assert_eq!(column.iter().filter(|cover| **cover > 0).count(), 2);
        // Through the middle of a row, it covers it and half of each either side.
        let mut plot = Plot::new(40, 20);
        plot.line(&[(0.0, 10.5), (40.0, 10.5)]);
        let column: Vec<u8> = (0..20).map(|y| plot.cover[y * 40 + 20]).collect();
        assert_eq!(&column[8..13], [0, 4, 8, 4, 0]);
        assert_eq!(
            column.iter().map(|cover| u32::from(*cover)).sum::<u32>(),
            16
        );
        // A slope has pixels part covered along its edges, which is what keeps it smooth.
        let mut slope = Plot::new(40, 40);
        slope.line(&[(0.0, 0.0), (40.0, 30.0)]);
        assert!(slope.cover.iter().any(|cover| (1..8).contains(cover)));
        for x in 2..38 {
            let expected = x as f32 * 0.75 + 0.375;
            let found = middle(&slope, x).unwrap();
            assert!(
                (found - expected).abs() < 0.5,
                "{x}: {found} against {expected}"
            );
        }
    }

    #[test]
    fn a_plots_png_has_an_alpha_for_each_entry_and_is_compressed() {
        let plot = compressor(Compressor::DEFAULT, (195, 195));
        let png = plot.png([10, 20, 30]);
        let chunk = |name: &[u8; 4]| {
            let at = png.windows(4).position(|window| window == name).unwrap();
            let length = u32::from_be_bytes(png[at - 4..at].try_into().unwrap()) as usize;
            png[at + 4..at + 4 + length].to_vec()
        };
        assert_eq!(&chunk(b"IHDR")[..10], [0, 0, 0, 195, 0, 0, 0, 195, 4, 3]);
        assert_eq!(
            chunk(b"tRNS"),
            [
                0,
                31,
                63,
                95,
                127,
                159,
                191,
                223,
                255,
                GRID_ALPHA,
                FAINTER_ALPHA
            ]
        );
        assert_eq!(&chunk(b"PLTE")[3..6], [10, 20, 30]);
        assert_eq!(chunk(b"PLTE")[3 * 9..3 * 10], rgb(theme::OVERLAY0));
        // Compressed, not stored: 195 rows of 98 bytes would be 19 KB.
        assert_eq!(chunk(b"IDAT")[..2], [0x78, 0x9C]);
        assert!(png.len() < 6000, "{}", png.len());
    }

    #[test]
    fn the_three_shades_are_dim_focused_and_faint() {
        assert_eq!(Shade::of(false, false), Shade::Dim);
        assert_eq!(Shade::of(true, false), Shade::Focused);
        assert_eq!(Shade::of(true, true), Shade::Faint);
        assert_eq!(Shade::of(false, true), Shade::Faint);
        assert_eq!(Shade::Dim.color(), rgb(theme::dim().fg.unwrap()));
        assert_eq!(Shade::Focused.color(), rgb(theme::active().fg.unwrap()));
        assert_eq!(Shade::Faint.color(), rgb(theme::faint().fg.unwrap()));
    }

    #[test]
    fn a_compressors_curve_is_its_gain_computers_with_gridlines_and_its_threshold() {
        let settings = Compressor {
            threshold: 0x80,
            ratio: 0xC0,
            ..Compressor::DEFAULT
        };
        let plot = compressor(settings, (195, 195));
        // At each column the line's middle is where the compressor's own curve puts it, within a
        // pixel, which is 0.3 dB here.
        for x in 1..194 {
            let (across, _) = plot.at(0.0, 0.0);
            let share = (x as f32 + 0.5 - across) / (195.0 - LINE_PIXELS);
            let out = curve_db(DYNAMICS_RANGE_DB * (share - 1.0), settings);
            let expected = plot.at(share, dynamics_level(out)).1;
            let found = middle(&plot, x).unwrap();
            assert!(
                (found - expected).abs() < 1.0,
                "{x}: {found} against {expected}"
            );
        }
        let pixels = plot.pixels();
        let entry = |x: f32, level: f32| {
            let (x, y) = plot.at(x, level);
            pixels[y as usize * 195 + x as usize]
        };
        // Gridlines at -20 and -40 dB, which the line doesn't cross there, and the diagonal under
        // it, well below the threshold.
        let lines: Vec<u8> = (0..195).map(|y| pixels[y * 195 + 2]).collect();
        let row = plot.at(0.0, dynamics_level(-20.0)).1 as usize;
        assert!((row - 1..=row + 1).any(|y| lines[y] == GRID));
        // The threshold's dashes, up to the curve.
        let threshold = threshold_db(f32::from(settings.threshold));
        let x = dynamics_level(threshold);
        let dashes = (0..10).filter(|step| entry(x, *step as f32 * 0.03) == GRID);
        assert!(dashes.count() >= 5);
        // Above it the curve is flatter than the diagonal, so it ends below the top right.
        let end = middle(&plot, 193).unwrap();
        assert!(end > plot.at(1.0, 1.0).1 + 20.0, "{end}");
    }

    #[test]
    fn a_combs_peaks_narrower_than_a_pixel_are_drawn_to_their_tops() {
        use vt_engine::dsp::filters::comb;
        use vt_model::insert::Comb;
        // Ringing for seconds at middle C, its peaks are a few cents wide, and a pixel of 2100 is
        // nearly six.
        let settings = Comb {
            feedback: 0xF8,
            mix: 0xC0,
            tone: 0xFF,
            ..Comb::DEFAULT
        };
        let db_at = comb::response(settings, 60.0, 48_000);
        let level = |x: f32| filter_level(db_at(frequency_at(x)));
        let (width, height) = (2100, 624);
        let found = peaks(&level, width);
        let drawn = Filtering {
            db_at: &db_at,
            marked: Some(261.6),
            opened: None::<fn(f32) -> f32>,
        };
        let plot = filtering(drawn, (width, height));
        for harmonic in 1..=12 {
            let hertz = 261.626 * harmonic as f32;
            // Its top, as high as any found every tenth of a cent within 20 cents of the harmonic.
            let top = (-200..=200)
                .map(|tenth| level(frequency_x(hertz * (tenth as f32 / 12_000.0).exp2())))
                .fold(f32::MIN, f32::max);
            let at = found
                .iter()
                .copied()
                .min_by(|a, b| {
                    let off = |x: f32| (x - frequency_x(hertz)).abs();
                    off(*a).total_cmp(&off(*b))
                })
                .unwrap();
            assert!(level(at) > top - 0.5 / height as f32, "{harmonic}");
            assert!(top < 1.0, "{harmonic} is in the picture: {top}");
            // And the line goes up to it, within a pixel, unless it's past the top.
            let (x, y) = plot.at(at, top);
            let highest = (0..height)
                .find(|row| plot.cover[row * width + x as usize] > 0)
                .unwrap();
            assert!(
                (highest as f32 - y).abs() < 2.0,
                "{harmonic}: {highest} against {y}"
            );
        }
    }

    #[test]
    fn a_delays_repeats_stand_up_for_the_left_and_down_for_the_right_as_loud_as_each_is() {
        let (width, height) = (600, 200);
        // The sound at full level both sides, a repeat on the left 20 dB down, and one on the
        // right 40 dB down.
        let sounds = [(0.0, [1.0, 1.0]), (0.5, [0.1, 0.0]), (1.0, [0.0, 0.01])];
        let plot = repeats(&sounds, 0.25, 1.5, (width, height));
        let column = |x: f32| plot.at(x, 0.5).0 as usize;
        let covered = |x: usize| -> Vec<usize> {
            (0..height)
                .filter(|y| plot.cover[y * width + x] > 0)
                .collect()
        };
        let reach = |x: f32| {
            let rows = covered(column(x));
            (*rows.first().unwrap() as f32, *rows.last().unwrap() as f32)
        };
        let (_, middle) = plot.at(0.0, 0.5);
        // The sound reaches the top and the bottom.
        let (top, bottom) = reach(0.0);
        assert!(
            top <= 1.0 && bottom >= height as f32 - 2.0,
            "{top} {bottom}"
        );
        // -20 dB is two thirds of the way up the top half, and -40 a third of the way down.
        let (top, bottom) = reach(0.5 / 1.5);
        let (_, expected) = plot.at(0.0, 0.5 + 0.5 * (2.0 / 3.0));
        assert!(
            (top - expected).abs() < 2.0 && (bottom - middle).abs() < 2.0,
            "{top} {bottom}"
        );
        let (top, bottom) = reach(1.0 / 1.5);
        let (_, expected) = plot.at(0.0, 0.5 - 0.5 / 3.0);
        assert!(
            (bottom - expected).abs() < 2.0 && (top - middle).abs() < 2.0,
            "{top} {bottom}"
        );
        // The beats are fainter lines down it.
        let (beat, _) = plot.at(0.25 / 1.5, 0.0);
        assert!((0..height).any(|y| plot.marks[y * width + beat as usize] == FAINTER));
    }

    #[test]
    fn a_reverbs_tail_falls_60_db_in_its_decay_time_after_its_pre_delay() {
        let (width, height) = (600, 200);
        let plot = tail(0.5, [2.0, 1.0], 3.0, 1.0, (width, height));
        // Before the pre-delay it's at the bottom, then it's at the top, and halfway through its
        // fall it's halfway down.
        let x = |seconds: f32| plot.at(seconds / 3.0, 0.0).0 as usize;
        let first = |column: usize| (0..height).find(|y| plot.cover[y * width + column] > 0);
        assert!(first(x(0.25)).unwrap() >= height - 3);
        let (_, halfway) = plot.at(0.0, 0.5);
        assert!((first(x(1.5)).unwrap() as f32 - halfway).abs() < 2.0);
        // Past its decay time there's nothing.
        assert_eq!(first(x(2.8)), None);
        // The highs' dotted line is half way down after half a second.
        let (_, half) = plot.at(0.0, 0.5);
        assert!(
            (0..height)
                .filter(|y| (*y as f32 - half).abs() < 3.0)
                .any(|y| plot.marks[y * width + x(1.0)] == GRID)
        );
    }

    #[test]
    fn a_gates_curve_opened_all_the_way_is_dotted_under_it() {
        let flat = |_: f32| -12.0;
        let drawn = Filtering {
            db_at: flat,
            marked: None,
            opened: Some(|_: f32| 0.0),
        };
        let plot = filtering(drawn, (600, 200));
        // The 0 dB line is a gridline too, so the opened curve is looked for a little above it.
        let (_, y) = plot.at(0.5, filter_level(0.0));
        let row: Vec<u8> = (0..600)
            .map(|x| plot.marks[(y as usize) * 600 + x])
            .collect();
        assert!(row.contains(&FAINTER));
        assert!(row.contains(&0), "dotted");
    }

    #[test]
    fn a_distortions_curve_is_its_transfer_and_a_type_without_one_has_its_gridlines_alone() {
        use vt_model::insert::DistortType;
        for kind in [DistortType::Soft, DistortType::Hard, DistortType::Sine] {
            let settings = Distort {
                kind,
                amount: 0x60,
                mix: 0xC0,
                ..Distort::DEFAULT
            };
            let plot = distort(settings, (630, 624));
            // Where it's not steep, the line's middle is within a pixel of the curve.
            for x in (10..620).step_by(20) {
                let share = (x as f32 + 0.5 - 1.0) / (630.0 - LINE_PIXELS);
                let out = transfer(settings, 2.0 * share - 1.0).unwrap();
                let expected = plot.at(share, 0.5 + 0.5 * out).1;
                let next = transfer(settings, 2.0 * share - 1.0 + 0.01).unwrap();
                if (next - out).abs() > 0.02 {
                    continue;
                }
                let found = middle(&plot, x).unwrap();
                assert!(
                    (found - expected).abs() < 1.0,
                    "{kind:?} {x}: {found} against {expected}"
                );
            }
        }
        let down = Distort {
            kind: DistortType::Down,
            ..Distort::DEFAULT
        };
        let plot = distort(down, (630, 624));
        assert!(plot.cover.iter().all(|cover| *cover == 0));
        assert!(plot.pixels().contains(&GRID));
    }

    #[test]
    fn every_compressor_type_is_drawn_by_its_own_curve() {
        use vt_model::insert::CompressorType;
        for kind in [
            CompressorType::Clean,
            CompressorType::Console,
            CompressorType::Glue,
        ] {
            let settings = Compressor {
                kind,
                threshold: 0x60,
                ..Compressor::DEFAULT
            };
            let plot = compressor(settings, (195, 195));
            for x in (5..190).step_by(10) {
                let share = (x as f32 + 0.5 - 1.0) / (195.0 - LINE_PIXELS);
                let out = curve_db(DYNAMICS_RANGE_DB * (share - 1.0), settings);
                let expected = plot.at(share, dynamics_level(out)).1;
                let found = middle(&plot, x).unwrap();
                assert!(
                    (found - expected).abs() < 1.0,
                    "{kind:?} {x}: {found} {expected}"
                );
            }
        }
    }

    #[test]
    fn an_eqs_curve_is_its_sum_over_a_0_db_line() {
        let flat = eq(Equaliser::DEFAULT, 48_000, (495, 195));
        let zero = flat.at(0.0, 0.5).1;
        for x in 0..495 {
            let found = middle(&flat, x).unwrap();
            assert!((found - zero).abs() < 0.5, "{x}: {found}");
        }
        let settings = Equaliser {
            low: 0xD0,
            mid: 0x50,
            ..Equaliser::DEFAULT
        };
        let plot = eq(settings, 48_000, (495, 195));
        for x in 1..494 {
            let share = (x as f32 + 0.5 - 1.0) / (495.0 - LINE_PIXELS);
            let db = eq_response_db(settings, 48_000, frequency_at(share));
            let expected = plot.at(share, eq_level(db)).1;
            let found = middle(&plot, x).unwrap();
            assert!(
                (found - expected).abs() < 1.0,
                "{x}: {found} against {expected}"
            );
        }
        // The 0 dB line shows where the curve is away from it, and the 1 kHz line down it.
        let pixels = plot.pixels();
        let row = zero as usize;
        assert!((0..495).filter(|x| pixels[row * 495 + x] == GRID).count() > 100);
        let kilohertz = plot.at(frequency_x(1000.0), 0.0).0 as usize;
        assert!(
            (0..195)
                .filter(|y| pixels[y * 495 + kilohertz] == FAINTER)
                .count()
                > 40
        );
    }

    #[test]
    fn a_filter_curve_is_flat_when_its_open_and_goes_where_the_filter_does() {
        use vt_model::song::FilterType;
        let open = Track {
            filter: FilterType::LowPass,
            cutoff: 0xFF,
            resonance: 0,
            drive: 0,
            low_cut: 0,
            high_cut: 0xFF,
            ..Track::default()
        };
        let plot = filter(&open, 48_000, (735, 195));
        let flat = plot.at(0.0, filter_level(0.0)).1;
        for x in 0..735 {
            assert!((middle(&plot, x).unwrap() - flat).abs() < 0.5, "{x}");
        }
        assert!(plot.png([255; 3]).len() < 6000);
        // Open, it has nothing to mark at its cut: only the 0 dB line under the curve and the
        // fainter lines down it.
        let pixels = plot.pixels();
        let zero = flat as usize;
        for y in (0..195_usize).filter(|y| y.abs_diff(zero) > 1) {
            assert!((0..735).all(|x| pixels[y * 735 + x] != GRID), "{y}");
        }
        let closed = Track {
            cutoff: 0x80,
            ..open
        };
        let plot = filter(&closed, 48_000, (735, 195));
        assert!(middle(&plot, 10).unwrap() < middle(&plot, 725).unwrap());
    }

    #[test]
    fn a_filter_curve_is_within_a_pixel_of_its_response_with_gridlines_and_a_mark_at_cut() {
        use vt_model::song::{FilterMode, FilterType};
        let size = (735, 195);
        for (filter_type, mode, cutoff, resonance) in [
            (FilterType::LowPass, FilterMode::Slope12, 0x80, 0x00),
            (FilterType::LowPass, FilterMode::Slope24, 0x60, 0xC0),
            (FilterType::HighPass, FilterMode::Slope12, 0x70, 0x40),
            (FilterType::BandPass, FilterMode::Slope12, 0x90, 0x80),
            (FilterType::Ladder, FilterMode::Slope24, 0xB4, 0x70),
        ] {
            let track = Track {
                filter: filter_type,
                filter_mode: mode,
                cutoff,
                resonance,
                drive: 0,
                low_cut: 0x28,
                high_cut: 0xFF,
                ..Track::default()
            };
            let plot = filter(&track, 48_000, size);
            // At each column the line's middle is where the filter's own response puts it,
            // within a pixel, where it's on the plot and not running steeply off its bottom.
            for x in 1..size.0 - 1 {
                let share = (x as f32 + 0.5 - 1.0) / (size.0 as f32 - LINE_PIXELS);
                let db = track_response_db(&track, 48_000, frequency_at(share));
                let next = track_response_db(&track, 48_000, frequency_at(share + 0.002));
                if db < CURVE_BOTTOM_DB + 2.0 || (next - db).abs() > 1.0 {
                    continue;
                }
                let expected = plot.at(share, filter_level(db)).1;
                let found = middle(&plot, x).unwrap();
                assert!(
                    (found - expected).abs() < 1.0,
                    "{filter_type:?} {x}: {found} against {expected}"
                );
            }
            let pixels = plot.pixels();
            // The 0 dB line, where the curve isn't, and the fainter line down it at 1 kHz.
            let zero = plot.at(0.0, filter_level(0.0)).1 as usize;
            assert!(
                (0..size.0)
                    .filter(|x| pixels[zero * size.0 + x] == GRID)
                    .count()
                    > 60,
                "{filter_type:?}"
            );
            let kilohertz = plot.at(frequency_x(1000.0), 0.0).0 as usize;
            assert!(
                (0..size.1)
                    .filter(|y| pixels[y * size.0 + kilohertz] == FAINTER)
                    .count()
                    > 40,
                "{filter_type:?}"
            );
            // The dashes up from the bottom at `Cut`'s frequency, which is its share across.
            let cut = f32::from(cutoff) / 255.0;
            let (x, bottom) = plot.at(cut, 0.0);
            let dashes = (0..20)
                .map(|step| bottom as usize - step)
                .filter(|y| pixels[y * size.0 + x as usize] == GRID)
                .count();
            assert!(dashes >= 10, "{filter_type:?}: {dashes}");
            assert!(plot.png([255; 3]).len() < 8000, "{filter_type:?}");
        }
    }

    #[test]
    fn an_envelope_reaches_full_and_ends_at_silence() {
        let shape = Adsr {
            attack_ms: 1,
            decay_ms: 200,
            sustain: 0x40,
            release_ms: 50,
            ..Adsr::default()
        };
        let plot = envelope(&shape, (360, 195));
        // The attack of a millisecond still reaches the top, whole.
        let top = plot.at(0.0, 1.0).1 as usize;
        assert!((0..20).any(|x| plot.cover[top * 360 + x] == STEPS));
        // It starts from the bottom and ends on it.
        let bottom = plot.at(0.0, 0.0).1;
        assert!(plot.cover[bottom as usize * 360] > 0);
        assert!((middle(&plot, 359).unwrap() - bottom).abs() < 0.5);
    }

    /// Whether a curve is steep around `x`, where a column's middle isn't its level: within two
    /// pixels either side, as far as the line reaches, it moves by more than a pixel and a half.
    fn steep(plot: &Plot, level: impl Fn(f32) -> f32, x: f32) -> bool {
        let pixel = 2.0 / plot.width as f32;
        let (before, after) = (level((x - pixel).max(0.0)), level((x + pixel).min(1.0)));
        (before - after).abs() * plot.height as f32 > 1.5
    }

    #[test]
    fn an_envelopes_shape_is_its_adsr_within_a_pixel_over_its_baseline_and_stages() {
        let size = (360, 195);
        let times = [(1, 200, 0x40, 50), (300, 80, 0xC0, 2000), (20, 20, 0, 400)];
        for ((attack_ms, decay_ms, sustain, release_ms), curve) in times
            .into_iter()
            .flat_map(|times| Curve::ALL.map(|curve| (times, curve)))
        {
            let shape = Adsr {
                attack_ms,
                decay_ms,
                sustain,
                release_ms,
                curve,
                ..Adsr::default()
            };
            let plot = envelope(&shape, size);
            let level = |x: f32| level_at(&shape, x);
            // At each column the line's middle is where the envelope's own stages put it, within
            // a pixel, where it isn't too steep for a column to have a middle.
            for x in 1..size.0 - 1 {
                let share = (x as f32 + 0.5 - 1.0) / (size.0 as f32 - LINE_PIXELS);
                if steep(&plot, level, share) {
                    continue;
                }
                let expected = plot.at(share, level(share)).1;
                let found = middle(&plot, x).unwrap();
                assert!(
                    (found - expected).abs() < 1.0,
                    "{shape:?} {x}: {found} against {expected}"
                );
            }
            // The sustain is flat at its level for a quarter of the way.
            let ends = stage_ends(&shape);
            let sustain = plot.at(0.0, shape.sustain_level()).1;
            let held = (ends[1] + ends[2]) / 2.0;
            let column = plot.at(held, 0.0).0 as usize;
            assert!((middle(&plot, column).unwrap() - sustain).abs() < 0.5);
            let pixels = plot.pixels();
            // A dotted baseline under the rest of it, after the release.
            let bottom = plot.at(0.0, 0.0).1 as usize;
            let after = plot.at(ends[3], 0.0).0 as usize;
            let row = |y: usize| &pixels[y * size.0..(y + 1) * size.0];
            assert!(
                (bottom - 1..=bottom + 1).any(|y| row(y).contains(&GRID)),
                "{shape:?}"
            );
            assert!(after < size.0, "{shape:?}");
            // A fainter dotted line down between each stage and the next, where the line isn't.
            for end in &ends[..3] {
                let x = plot.at(*end, 0.0).0 as usize;
                let marked = (0..size.1).filter(|y| pixels[y * size.0 + x] == FAINTER);
                assert!(marked.count() > 30, "{shape:?} at {end}");
            }
        }
    }

    #[test]
    fn an_ahds_shape_is_its_stages_within_a_pixel_with_a_line_either_side_of_its_hold() {
        use vt_model::envelope::{Ahd, Hold};
        let size = (360, 195);
        for (attack_ms, hold, decay_ms, curve) in [
            (1, Hold::Ms(30), 200, Curve::Exp),
            (100, Hold::Note, 400, Curve::Snap),
            (20, Hold::Ms(2000), 50, Curve::Lin),
        ] {
            let shape = Ahd {
                attack_ms,
                hold,
                decay_ms,
                curve,
                ..Ahd::default()
            };
            let plot = envelope_in(&Envelope::Ahd(shape), Polarity::Uni, size);
            let level = |x: f32| ahd_level_at(&shape, x);
            for x in 1..size.0 - 1 {
                let share = (x as f32 + 0.5 - 1.0) / (size.0 as f32 - LINE_PIXELS);
                if steep(&plot, level, share) {
                    continue;
                }
                let expected = plot.at(share, level(share)).1;
                let found = middle(&plot, x).unwrap();
                assert!(
                    (found - expected).abs() < 1.0,
                    "{shape:?} {x}: {found} against {expected}"
                );
            }
            // Full through its hold, and a fainter line at either end of it, but none after.
            let ends = ahd_stage_ends(&shape);
            let column = plot.at((ends[0] + ends[1]) / 2.0, 0.0).0 as usize;
            let top = plot.at(0.0, 1.0).1;
            assert!(
                (middle(&plot, column).unwrap() - top).abs() < 0.5,
                "{shape:?}"
            );
            let pixels = plot.pixels();
            let marked = |end: f32| {
                let x = plot.at(end, 0.0).0 as usize;
                (0..size.1)
                    .filter(|y| pixels[y * size.0 + x] == FAINTER)
                    .count()
            };
            for end in &ends[..2] {
                assert!(marked(*end) > 30, "{shape:?} at {end}");
            }
            assert!(marked(ends[2]) < 30, "{shape:?}");
        }
    }

    #[test]
    fn an_lfos_cycle_is_the_engines_wave_within_a_pixel_from_its_phase_and_flipped_by_its_sign() {
        use vt_model::lfo::Shape as Wave;
        let size = (360, 195);
        for shape in [Wave::Sine, Wave::Tri, Wave::Saw, Wave::Ramp, Wave::Square] {
            // A deform bends the drawing as it bends the engine's wave.
            for (phase, depth, deform) in [(0, 0x40, 0), (90, 0x40, 0x60), (200, -0x30, -0xA0)] {
                let lfo = Lfo {
                    shape,
                    phase,
                    deform,
                    ..Lfo::default()
                };
                let drawing = LfoDrawing::of(&lfo, depth);
                let plot = lfo_cycle(drawing, size);
                // What the engine plays, worked out here from its own wave, with the depth's sign.
                let sign = if depth < 0 { -1.0 } else { 1.0 };
                let level = |x: f32| {
                    let turns = f64::from(x.min(0.9999)) + f64::from(phase) / 360.0;
                    let bend = f32::from(deform) / f32::from(lfo::MAX_DEFORM);
                    0.5 + 0.5 * sign * deformed_wave(shape, turns, 0, bend)
                };
                for x in 1..size.0 - 1 {
                    let share = (x as f32 + 0.5 - 1.0) / (size.0 as f32 - LINE_PIXELS);
                    if steep(&plot, level, share) {
                        continue;
                    }
                    let expected = plot.at(share, level(share)).1;
                    let found = middle(&plot, x).unwrap();
                    assert!(
                        (found - expected).abs() < 1.0,
                        "{shape:?} {phase} {depth} {x}: {found} against {expected}"
                    );
                }
                // A jump inside the cycle is drawn straight up or down: a column of the line from
                // top to bottom. A saw from 0 has its jump at the cycle's ends.
                if shape == Wave::Square || matches!(shape, Wave::Saw | Wave::Ramp) && phase != 0 {
                    let jump = (0..size.0)
                        .any(|x| (2..size.1 - 2).all(|y| plot.cover[y * size.0 + x] > 0));
                    assert!(jump, "{shape:?} {phase}");
                }
                // Over its dotted centre line.
                let centre = plot.at(0.0, 0.5).1 as usize;
                let pixels = plot.pixels();
                let marked = (0..size.0)
                    .filter(|x| (centre - 1..=centre + 1).any(|y| pixels[y * size.0 + x] == GRID));
                assert!(marked.count() > 60, "{shape:?} {phase}");
                assert!(plot.png([255; 3]).len() < 6000, "{shape:?} {phase}");
            }
        }
    }

    #[test]
    fn an_attractor_is_drawn_side_on_with_the_axis_heard_up_it() {
        use vt_model::attractor::attractor_at;
        let size = (360, 195);
        for shape in [lfo::Shape::Lorenz, lfo::Shape::Rossler] {
            let system = System::of(shape).unwrap();
            let lfo = Lfo {
                shape,
                ..Lfo::default()
            };
            let drawing = LfoDrawing::of(&lfo, 0x40);
            let plot = lfo_cycle(drawing, size);
            let covered = plot.cover.iter().filter(|cover| **cover > 0).count();
            assert!(covered > size.0 * 8, "{shape:?} {covered}");
            // Up it is `x`, the axis heard at 0°, and across it `y`, the next one round, as the
            // engine has them from the same seed.
            let trace = attractor_trace(drawing, system);
            let point = attractor_at(system, 3.0, 0, 0.0);
            let (x, level) = trace[3 * TRACED_POINTS];
            assert!(
                (level - drawing.level_of(point[0])).abs() < 1e-6,
                "{shape:?}"
            );
            assert!((x - (0.5 + 0.5 * point[1])).abs() < 1e-6, "{shape:?}");
            // Its axis, its deform and its depth's sign each draw it differently; its mode doesn't.
            for other in [
                Lfo { phase: 240, ..lfo },
                Lfo {
                    deform: 0x80,
                    ..lfo
                },
            ] {
                let other = lfo_cycle(LfoDrawing::of(&other, 0x40), size);
                assert_ne!(other.cover, plot.cover, "{shape:?}");
            }
            assert_ne!(
                lfo_cycle(LfoDrawing::of(&lfo, -0x40), size).cover,
                plot.cover
            );
            let half = Lfo {
                mode: LfoMode::Half,
                ..lfo
            };
            assert_eq!(LfoDrawing::of(&half, 0x40), drawing);
            // Its dot is placed across by where the engine says it is.
            assert_eq!(drawing.x_of(0.25), 0.25);
        }
    }

    #[test]
    fn a_half_cycle_draws_its_half_then_stays_flat_where_it_ends() {
        let size = (360, 195);
        let lfo = |mode| Lfo {
            shape: lfo::Shape::Saw,
            mode,
            ..Lfo::default()
        };
        let half = LfoDrawing::of(&lfo(LfoMode::Half), 0x40);
        assert_ne!(half, LfoDrawing::of(&lfo(LfoMode::One), 0x40));
        let plot = lfo_cycle(half, size);
        // A saw falls from the top to the middle over the first half, then stays there.
        let top = plot.at(0.0, 1.0).1;
        let centre = plot.at(0.0, 0.5).1;
        let quarter = plot.at(0.0, 0.75).1;
        let found = |share: f32| middle(&plot, (share * size.0 as f32) as usize).unwrap();
        assert!((found(0.25) - quarter).abs() < 1.5, "{}", found(0.25));
        for share in [0.6, 0.75, 0.95] {
            assert!(
                (found(share) - centre).abs() < 1.5,
                "{share}: {}",
                found(share)
            );
        }
        assert!(found(0.01) - top < 3.0);
        // `One` plays the whole cycle, as the other modes draw it.
        let one = lfo_cycle(LfoDrawing::of(&lfo(LfoMode::One), 0x40), size);
        let low = plot.at(0.0, 0.25).1;
        assert!((middle(&one, size.0 * 3 / 4).unwrap() - low).abs() < 1.5);
    }

    #[test]
    fn a_random_lfo_draws_only_its_centre_line_whatever_its_phase_and_sign() {
        use vt_model::lfo::Shape as Wave;
        for shape in [Wave::Random, Wave::Smooth] {
            let plain = LfoDrawing::of(
                &Lfo {
                    shape,
                    ..Lfo::default()
                },
                0,
            );
            let moved = LfoDrawing::of(
                &Lfo {
                    shape,
                    phase: 120,
                    ..Lfo::default()
                },
                -0x40,
            );
            assert_eq!(plain, moved, "the same picture, so it isn't sent again");
            let plot = lfo_cycle(plain, (360, 195));
            assert!(plot.cover.iter().all(|cover| *cover == 0), "{shape:?}");
            assert!(plot.pixels().contains(&GRID));
        }
        // A wave that's drawn changes with both.
        let tri = |phase, depth| {
            LfoDrawing::of(
                &Lfo {
                    phase,
                    ..Lfo::default()
                },
                depth,
            )
        };
        assert_ne!(tri(0, 0x40), tri(90, 0x40));
        assert_ne!(tri(0, 0x40), tri(0, -0x40));
        assert_eq!(tri(0, 0x40), tri(0, 0x10), "but not with the depth's size");
    }

    #[test]
    fn where_a_wave_is_goes_across_the_cycle_from_its_phase() {
        let drawing = LfoDrawing::of(
            &Lfo {
                phase: 90,
                ..Lfo::default()
            },
            0,
        );
        // The cycle starts a quarter of the way round, so that's its left edge.
        assert!((drawing.x_of(0.25) - 0.0).abs() < 1e-6);
        assert!((drawing.x_of(0.75) - 0.5).abs() < 1e-6);
        assert!((drawing.x_of(0.0) - 0.75).abs() < 1e-6);
        // And the level there is the wave's.
        assert!(
            (drawing.level_at(0.5) - drawing.level_of(lfo::wave(lfo::Shape::Tri, 0.75, 0))).abs()
                < 1e-6
        );
    }

    #[test]
    fn a_dot_is_round_and_solid_in_the_middle() {
        let png = dot(7, [10, 20, 30]);
        let mut plot = Plot::new(7, 7);
        let radius = 3.5_f32;
        for y in 0..7 {
            for x in 0..7 {
                let from = (x as f32 + 0.5 - radius).hypot(y as f32 + 0.5 - radius);
                plot.cover[y * 7 + x] = ((radius + 0.5 - from).clamp(0.0, 1.0) * 8.0).round() as u8;
            }
        }
        assert_eq!(png, plot.png([10, 20, 30]));
        // Solid across its middle, nothing in its corners, and the same each way round.
        assert!(plot.cover[3 * 7..4 * 7].iter().all(|cover| *cover == STEPS));
        assert_eq!(plot.cover[0], 0);
        for y in 0..7 {
            for x in 0..7 {
                assert_eq!(plot.cover[y * 7 + x], plot.cover[x * 7 + (6 - y)]);
            }
        }
        assert!(png.len() < 200, "{}", png.len());
    }

    #[test]
    fn a_mappings_response_is_its_curve_within_a_pixel_and_turned_over_for_an_inverted_one() {
        use vt_model::macros::MacroTarget;
        use vt_model::settings::SettingId;
        let size = (195, 195);
        let whole = MacroMap::whole(MacroTarget::Setting(SettingId::CUT));
        for (curve, s) in [(0, 0), (60, 0), (-60, 40), (30, 100)] {
            for inverted in [false, true] {
                let map = if inverted {
                    MacroMap {
                        min: whole.max,
                        max: whole.min,
                        ..whole
                    }
                } else {
                    whole
                }
                .bent(curve, s);
                let drawing = MappingDrawing::of(&map, 0x4000);
                let plot = mapping(drawing, size);
                // Where the setting is between its lower and its higher end, as `at` puts it.
                let level = |x: f32| f32::from(map.at(x * 255.0)) / f32::from(whole.max);
                for x in 1..size.0 - 1 {
                    let share = (x as f32 + 0.5 - 1.0) / (size.0 as f32 - LINE_PIXELS);
                    if steep(&plot, level, share) {
                        continue;
                    }
                    let expected = plot.at(share, level(share)).1;
                    let found = middle(&plot, x).unwrap();
                    assert!(
                        (found - expected).abs() < 1.0,
                        "{curve} {s} {inverted} {x}: {found} against {expected}"
                    );
                }
                assert!(plot.png([255; 3]).len() < 3000);
            }
        }
        // Moving an end draws nothing new; bending it, or turning the other way, does.
        let drawn = MappingDrawing::of(&whole, 0x4000);
        assert_eq!(
            MappingDrawing::of(
                &MacroMap {
                    max: 0x8000,
                    ..whole
                },
                0x4000
            ),
            drawn
        );
        assert_ne!(MappingDrawing::of(&whole.bent(5, 0), 0x4000), drawn);
        let down = MacroMap {
            min: 0x9000,
            max: 0x1000,
            ..whole
        };
        assert_ne!(MappingDrawing::of(&down, 0x4000), drawn);
    }
}
