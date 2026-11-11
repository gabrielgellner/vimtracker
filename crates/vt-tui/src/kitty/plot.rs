//! Curves drawn at the screen's own pixels, for the track page's graphic panels: a smooth line,
//! from how much of each pixel it covers, over faint gridlines. Its PNG has a palette with an
//! alpha for each entry, so a pixel the line only grazes is the line's colour, part see-through,
//! and nothing is stretched, as the picture is the size of the characters it covers.
//!
//! Every curve is drawn here: the compressor's and the EQ's on the FX page, the filter's on the
//! Sound page and the envelopes on the Mod page.

use vt_engine::dsp::compressor::curve_db;
use vt_engine::dsp::eq::response_db as eq_response_db;
use vt_engine::dsp::filter::track_response_db;
use vt_model::envelope::Envelope;
use vt_model::insert::{Compressor, Equaliser, threshold_db};
use vt_model::song::Track;

use super::{indexed_png, rgb};
use crate::theme;
use crate::views::curve::{level_at, stage_ends};

/// How wide the line is, in pixels.
const LINE_PIXELS: f32 = 2.0;
/// How many points are worked out along each pixel across, so a narrow peak between two isn't
/// missed.
const POINTS_PER_PIXEL: usize = 2;
/// How finely a pixel's share of the line is kept: the line's colour at 1/8 to 8/8, which are
/// its palette's entries 1 to 8.
const STEPS: u8 = 8;
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
    /// to 1 at the top. The line's centre is kept half its width in from the edges, so it's
    /// whole at the top and bottom.
    pub fn at(&self, x: f32, level: f32) -> (f32, f32) {
        let margin = LINE_PIXELS / 2.0;
        let across = margin + x.clamp(0.0, 1.0) * (self.width as f32 - 2.0 * margin);
        let down = margin + (1.0 - level.clamp(0.0, 1.0)) * (self.height as f32 - 2.0 * margin);
        (across, down)
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

/// Where a level in dB is on a filter's curve, from 0 at its bottom to 1 at its top.
pub fn filter_level(db: f32) -> f32 {
    (db - CURVE_BOTTOM_DB) / (CURVE_TOP_DB - CURVE_BOTTOM_DB)
}

/// A track's filter and utility filter: what they let through at each frequency.
pub fn filter(track: &Track, rate: u32, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    let level = |x: f32| filter_level(track_response_db(track, rate, frequency_at(x)));
    plot.curve(level, level(0.0), &[]);
    plot
}

/// An envelope's shape, from silence, with a point at the end of each stage so its corners are
/// sharp.
pub fn envelope(envelope: &Envelope, (width, height): (usize, usize)) -> Plot {
    let mut plot = Plot::new(width, height);
    plot.curve(|x| level_at(envelope, x), 0.0, &stage_ends(envelope));
    plot
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let plot = filter(&open, 48_000, (360, 117));
        let flat = plot.at(0.0, filter_level(0.0)).1;
        for x in 0..360 {
            assert!((middle(&plot, x).unwrap() - flat).abs() < 0.5, "{x}");
        }
        assert!(plot.png([255; 3]).len() < 3000);
        let closed = Track {
            cutoff: 0x80,
            ..open
        };
        let plot = filter(&closed, 48_000, (360, 117));
        assert!(middle(&plot, 10).unwrap() < middle(&plot, 350).unwrap());
    }

    #[test]
    fn an_envelope_reaches_full_and_ends_at_silence() {
        let shape = Envelope {
            attack_ms: 1,
            decay_ms: 200,
            sustain: 0x40,
            release_ms: 50,
            ..Envelope::default()
        };
        let plot = envelope(&shape, (420, 156));
        // The attack of a millisecond still reaches the top, whole.
        let top = plot.at(0.0, 1.0).1 as usize;
        assert!((0..20).any(|x| plot.cover[top * 420 + x] == STEPS));
        // It starts from the bottom and ends on it.
        let bottom = plot.at(0.0, 0.0).1;
        assert!(plot.cover[bottom as usize * 420] > 0);
        assert!((middle(&plot, 419).unwrap() - bottom).abs() < 0.5);
    }
}
