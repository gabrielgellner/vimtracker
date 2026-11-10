//! A three-band EQ: a low shelf, a bell in the middle and a high shelf, each a state variable
//! filter with its outputs mixed into that response, after Andrew Simper's "Linear Trapezoidal
//! Integrated State Variable Filter With Low Noise Optimisation" (Cytomic, 2016). A filter like
//! this stays smooth as its settings move, so a band can be swept by an LFO without zippering.

use std::f32::consts::PI;

use vt_model::insert::Equaliser as Settings;
use vt_model::settings::{band_db, band_q};

use crate::dsp::blocks::smoother::Smoothed;

/// The shelves' width: a gentle slope with no bump.
const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// The highest a band sits, as a share of the rate, so its tangent stays finite.
const HIGHEST: f32 = 0.49;

/// What each band's filter mixes from its outputs, and its coefficients.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Coefficients {
    /// The filter's tuning and damping, kept for working out its response.
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    /// How much of the input, the band-pass and the low-pass go out.
    m0: f32,
    m1: f32,
    m2: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    LowShelf,
    Bell,
    HighShelf,
}

impl Coefficients {
    /// A band at a frequency as a share of the rate, boosting or cutting by `db`.
    fn new(shape: Shape, frequency: f32, db: f32, q: f32) -> Self {
        let a = 10_f32.powf(db / 40.0);
        let tangent = (PI * frequency.clamp(1e-5, HIGHEST)).tan();
        let (g, k, m0, m1, m2) = match shape {
            Shape::Bell => {
                let k = 1.0 / (q * a);
                (tangent, k, 1.0, k * (a * a - 1.0), 0.0)
            }
            Shape::LowShelf => {
                let k = 1.0 / q;
                (tangent / a.sqrt(), k, 1.0, k * (a - 1.0), a * a - 1.0)
            }
            Shape::HighShelf => {
                let k = 1.0 / q;
                (tangent * a.sqrt(), k, a * a, k * (1.0 - a) * a, 1.0 - a * a)
            }
        };
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            g,
            k,
            a1,
            a2,
            a3: g * a2,
            m0,
            m1,
            m2,
        }
    }
}

impl Coefficients {
    /// How loud the band leaves a frequency, as a share of the rate, in power. The filter is the
    /// analog one through the bilinear transform, so at a frequency it's the analog one at
    /// `tan(pi f) / g`, mixing its input, band-pass and low-pass: `m0 + (m1 s + m2) / (s^2 + k s + 1)`.
    fn power(&self, frequency: f32) -> f32 {
        let w = (PI * frequency.min(HIGHEST)).tan() / self.g;
        let below = [1.0 - w * w, self.k * w];
        let above = [
            self.m0 * below[0] + self.m2,
            self.m0 * below[1] + self.m1 * w,
        ];
        (above[0] * above[0] + above[1] * above[1]) / (below[0] * below[0] + below[1] * below[1])
    }
}

/// The bands' coefficients, from each band's frequency and boost and the middle's Q, on the `00`
/// to `FF` scale.
fn bands(settings: [f32; 7], rate: u32) -> [Coefficients; 3] {
    // From 20 Hz to 20 kHz, evenly in pitch, as the cutoff is.
    let share = |value: f32| 20.0 * 1000_f32.powf(value / 255.0) / rate as f32;
    let [
        low_frequency,
        low,
        mid_frequency,
        mid,
        q,
        high_frequency,
        high,
    ] = settings;
    [
        Coefficients::new(Shape::LowShelf, share(low_frequency), band_db(low), SHELF_Q),
        Coefficients::new(Shape::Bell, share(mid_frequency), band_db(mid), band_q(q)),
        Coefficients::new(
            Shape::HighShelf,
            share(high_frequency),
            band_db(high),
            SHELF_Q,
        ),
    ]
}

/// The settings as the bands take them, in their order.
fn values(settings: Settings) -> [f32; 7] {
    [
        settings.low_frequency,
        settings.low,
        settings.mid_frequency,
        settings.mid,
        settings.mid_q,
        settings.high_frequency,
        settings.high,
    ]
    .map(f32::from)
}

/// How loud the EQ leaves a frequency, in dB, once it's settled on its settings: worked out from the
/// bands' own coefficients, for drawing its curve.
pub fn response_db(settings: Settings, rate: u32, hertz: f32) -> f32 {
    let frequency = hertz / rate as f32;
    let power: f32 = bands(values(settings), rate)
        .iter()
        .map(|band| band.power(frequency))
        .product();
    10.0 * power.max(1e-12).log10()
}

/// One band's state, on one side.
#[derive(Debug, Clone, Copy, Default)]
struct Band {
    ic1: f32,
    ic2: f32,
}

impl Band {
    #[inline]
    fn process(&mut self, c: &Coefficients, input: f32) -> f32 {
        let v3 = input - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        c.m0 * input + c.m1 * v1 + c.m2 * v2
    }
}

/// The three bands on both sides, with each setting moving smoothly to where it's set.
#[derive(Debug, Clone, Copy)]
pub struct ThreeBands {
    bands: [[Band; 3]; 2],
    coefficients: [Coefficients; 3],
    /// Each band's frequency and boost, and the middle's Q, on the `00` to `FF` scale.
    settings: [Smoothed; 7],
    /// What the coefficients were worked out from, so they're only worked out again as it moves.
    worked_out: [f32; 7],
}

impl Default for ThreeBands {
    fn default() -> Self {
        let mut eq = Self {
            bands: [[Band::default(); 3]; 2],
            coefficients: [Coefficients::default(); 3],
            settings: [Smoothed::at(0.0); 7],
            worked_out: [f32::NAN; 7],
        };
        eq.follow(Settings::DEFAULT);
        eq.settle();
        eq
    }
}

impl ThreeBands {
    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        for (smoothed, value) in self.settings.iter_mut().zip(values(settings)) {
            smoothed.target = value;
        }
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        for smoothed in &mut self.settings {
            smoothed.settle();
        }
    }

    /// Forget what's ringing, for a fresh start.
    pub fn clear(&mut self) {
        self.bands = [[Band::default(); 3]; 2];
    }

    /// One frame through the bands.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let now = self
            .settings
            .each_mut()
            .map(|smoothed| smoothed.next(smoothing));
        if now != self.worked_out {
            self.worked_out = now;
            self.coefficients = bands(now, rate);
        }
        let mut out = input;
        for (side, sample) in out.iter_mut().enumerate() {
            for (band, coefficients) in self.bands[side].iter_mut().zip(&self.coefficients) {
                *sample = band.process(coefficients, *sample);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// How loud a sine at this frequency comes out, against going in, in dB.
    fn response(settings: Settings, hertz: f32) -> f32 {
        let mut eq = ThreeBands::default();
        eq.follow(settings);
        eq.settle();
        let frames = RATE as usize;
        let (mut input, mut output) = (0.0_f32, 0.0_f32);
        for frame in 0..frames {
            let sample = (frame as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin();
            let out = eq.process([sample; 2], 1.0, RATE)[0];
            if frame > frames / 2 {
                input += sample * sample;
                output += out * out;
            }
        }
        10.0 * (output / input).log10()
    }

    #[test]
    fn flat_leaves_every_frequency_as_it_is() {
        for hertz in [40.0, 200.0, 1_000.0, 5_000.0, 15_000.0] {
            let db = response(Settings::DEFAULT, hertz);
            assert!(db.abs() < 0.05, "{hertz} Hz: {db}");
        }
    }

    #[test]
    fn each_band_boosts_where_it_sits_and_not_far_from_it() {
        let full = 0xFF;
        let low = Settings {
            low: full,
            ..Settings::DEFAULT
        };
        assert!(response(low, 30.0) > 13.0, "{}", response(low, 30.0));
        assert!(response(low, 5_000.0).abs() < 0.5);
        let mid = Settings {
            mid: full,
            ..Settings::DEFAULT
        };
        assert!(
            (response(mid, 1_000.0) - BAND).abs() < 0.5,
            "{}",
            response(mid, 1_000.0)
        );
        assert!(response(mid, 60.0).abs() < 1.0);
        let high = Settings {
            high: 0x00,
            ..Settings::DEFAULT
        };
        assert!(
            response(high, 15_000.0) < -13.0,
            "{}",
            response(high, 15_000.0)
        );
        assert!(response(high, 100.0).abs() < 0.5);
    }

    #[test]
    fn the_worked_out_response_is_what_a_sine_through_it_comes_out_at() {
        let settings = [
            Settings::DEFAULT,
            Settings {
                low: 0xFF,
                mid: 0x20,
                mid_q: 0xC0,
                high: 0x30,
                ..Settings::DEFAULT
            },
            Settings {
                low_frequency: 0xA0,
                low: 0x00,
                mid_frequency: 0x40,
                mid: 0xE0,
                mid_q: 0x10,
                high_frequency: 0x60,
                high: 0xFF,
                ..Settings::DEFAULT
            },
        ];
        for settings in settings {
            for hertz in [30.0, 120.0, 500.0, 1_000.0, 3_000.0, 9_000.0, 18_000.0] {
                let (worked_out, heard) = (
                    response_db(settings, RATE, hertz),
                    response(settings, hertz),
                );
                assert!(
                    (worked_out - heard).abs() < 0.1,
                    "{settings:?} at {hertz} Hz: {worked_out} against {heard}"
                );
            }
        }
    }

    /// The most a band boosts.
    const BAND: f32 = vt_model::settings::BAND_DB;

    #[test]
    fn a_narrower_middle_reaches_less_far() {
        let wide = Settings {
            mid: 0xFF,
            mid_q: 0x00,
            ..Settings::DEFAULT
        };
        let narrow = Settings {
            mid_q: 0xFF,
            ..wide
        };
        assert!(response(wide, 2_000.0) > response(narrow, 2_000.0) + 6.0);
    }

    #[test]
    fn sweeping_a_band_stays_finite_and_bounded() {
        let mut eq = ThreeBands::default();
        for frame in 0..RATE as usize {
            let swing = (frame / 100 % 256) as u8;
            eq.follow(Settings {
                mid_frequency: swing,
                mid: 0xFF,
                mid_q: 0xFF,
                low: swing,
                high: 255 - swing,
                ..Settings::DEFAULT
            });
            let sample = (frame as f32 * 0.05).sin() * 0.5;
            let out = eq.process([sample; 2], 0.01, RATE);
            assert!(
                out.iter().all(|s| s.is_finite() && s.abs() < 4.0),
                "{out:?}"
            );
        }
    }
}
