//! How a reverb's tail dies away, for a drawing of it on the FX page's zoom: nothing for its
//! pre-delay, then a fall in a straight line in dB, at two frequencies, as each reverb works it out
//! from its own loop: how long a trip round it takes and what each trip keeps.
//!
//! A loop's all-passes keep all of what goes through them but hold some frequencies back longer
//! than others, so its modes don't all die away at one rate: one whose trip is long loses as much
//! a trip as one whose trip is short, but over longer. A tail is then many falls at once, and
//! those that fall slowest last longest, so it falls more slowly than its average trip says. So
//! a loop's tail is worked out as what a band of it around a frequency would measure: each
//! frequency in the band falls by what the loop keeps there over the loop's delay there, all
//! starting together, as a click's do, and the straight line through their sum from 5 to 35 dB
//! down, as Schroeder's backward integration measures it, is the tail.

/// The frequencies a reverb's tail is measured at: below its damping, and up where damping takes
/// the most.
pub(crate) const TAIL_HZ: [f32; 2] = [250.0, 4_000.0];

/// How a reverb's tail dies away, for a drawing: nothing for its pre-delay, `pre`, in seconds,
/// then falling in a straight line in dB, `seconds[i]` to fall 60 dB at `TAIL_HZ[i]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tail {
    pub pre: f32,
    pub seconds: [f32; 2],
}

/// How sharp the band a tail is worked out and measured in is: two band-passes of this Q, about an
/// octave wide together, as rooms' tails are measured in octaves, so it holds enough modes for
/// a big tank's lows.
pub(crate) const BAND_Q: f64 = 1.4;
/// How many frequencies across the band a loop's tail is worked out from.
const BAND_POINTS: usize = 4096;
/// The range of the straight line through a tail, in dB down from its start, as T30 takes it.
const FIT_DB: (f64, f64) = (5.0, 35.0);

/// How much a band, two band-passes of `BAND_Q` at `centre`, both radians a frame, passes of a
/// frequency's power.
fn band_power(omega: f64, centre: f64) -> f64 {
    let w = (omega / 2.0).tan() / (centre / 2.0).tan();
    let k = 1.0 / BAND_Q;
    let one = (w * k) * (w * k) / ((1.0 - w * w) * (1.0 - w * w) + (w * k) * (w * k));
    one * one
}

/// How long a loop's tail takes to fall 60 dB in a band around `hertz`, in seconds, from what it
/// keeps of each frequency, as a gain, and how long it holds it back, in frames, `at` its radians
/// a frame: as a band of a click through it would measure it.
pub(crate) fn loop_seconds(hertz: f32, rate: u32, at: impl Fn(f64) -> (f64, f64)) -> f32 {
    let centre = std::f64::consts::TAU * f64::from(hertz) / f64::from(rate);
    // Three octaves either side, where the band passes 42 dB less.
    let (low, high) = (
        centre / 8.0,
        (centre * 8.0).min(std::f64::consts::PI * 0.98),
    );
    let mut falls = Vec::with_capacity(BAND_POINTS);
    for point in 0..BAND_POINTS {
        let omega = low * (high / low).powf((point as f64 + 0.5) / BAND_POINTS as f64);
        let (kept, frames) = at(omega);
        if kept >= 1.0 {
            return f32::INFINITY;
        }
        // Its power falls by `kept` squared each trip of `frames`, and spaced evenly in pitch,
        // each stands for a share of the band as wide as it is high.
        let rate = -2.0 * kept.max(1e-12).ln() / frames.max(1.0);
        falls.push((band_power(omega, centre) * omega, rate));
    }
    // What's left to come out after a time, in frames, as a share of all of it.
    let left = |time: f64| -> f64 {
        falls
            .iter()
            .map(|(weight, rate)| weight * (-rate * time).exp() / rate)
            .sum()
    };
    let all = left(0.0);
    let total: f64 = falls.iter().map(|(weight, _)| weight).sum();
    let average = falls
        .iter()
        .map(|(weight, rate)| weight * rate)
        .sum::<f64>()
        / total;
    // As long as the average would take to fall 60 dB, in steps of a hundredth of it, until it's
    // fallen past the line's end, or for ten times as long, when the line is what it's fallen.
    let step = (1e6_f64).ln() / average / 100.0;
    let (mut n, mut sum_t, mut sum_d, mut sum_tt, mut sum_td) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for at in 0..1_000 {
        let time = at as f64 * step;
        let db = 10.0 * (left(time) / all).log10();
        if db < -FIT_DB.1 {
            break;
        }
        if db <= -FIT_DB.0 {
            n += 1.0;
            sum_t += time;
            sum_d += db;
            sum_tt += time * time;
            sum_td += time * db;
        }
    }
    let slope = (n * sum_td - sum_t * sum_d) / (n * sum_tt - sum_t * sum_t);
    (-60.0 / slope / f64::from(rate)) as f32
}

/// A complex number in double precision, for a loop's response round thousands of frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Complex(pub(crate) f64, pub(crate) f64);

impl Complex {
    pub(crate) const ONE: Self = Self(1.0, 0.0);

    /// A delay of this many frames at `omega` radians a frame.
    pub(crate) fn delay(omega: f64, frames: f64) -> Self {
        let (sin, cos) = (omega * frames).sin_cos();
        Self(cos, -sin)
    }

    /// A read this many frames back between two samples, in a straight line from one to the
    /// next, as the reverbs' lines read: a delay, and a little darker between samples.
    pub(crate) fn read(omega: f64, frames: f64) -> Self {
        let whole = frames.floor();
        let between = frames - whole;
        Self::delay(omega, whole)
            .scale(1.0 - between)
            .add(Self::delay(omega, whole + 1.0).scale(between))
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1)
    }

    pub(crate) fn scale(self, by: f64) -> Self {
        Self(self.0 * by, self.1 * by)
    }

    pub(crate) fn times(self, other: Self) -> Self {
        Self(
            self.0 * other.0 - self.1 * other.1,
            self.0 * other.1 + self.1 * other.0,
        )
    }

    pub(crate) fn over(self, other: Self) -> Self {
        let size = other.0 * other.0 + other.1 * other.1;
        Self(
            (self.0 * other.0 + self.1 * other.1) / size,
            (self.1 * other.0 - self.0 * other.1) / size,
        )
    }

    pub(crate) fn size(self) -> f64 {
        self.0.hypot(self.1)
    }

    pub(crate) fn angle(self) -> f64 {
        self.1.atan2(self.0)
    }
}

/// What a loop keeps of a frequency and how long it holds it back, in frames, from its response
/// `at` each radians a frame: its size, and its phase's slope there.
pub(crate) fn kept_and_held(omega: f64, at: impl Fn(f64) -> Complex) -> (f64, f64) {
    let step = 1e-6;
    let (before, here, after) = (at(omega - step), at(omega), at(omega + step));
    let turned = after.over(before).angle();
    (here.size(), -turned / (2.0 * step))
}

/// What measures a tail in the reverbs' tests.
#[cfg(test)]
pub(crate) mod measure {
    /// How long an impulse response takes to fall 60 dB around a frequency, in seconds: its
    /// energy in a band there, by two band-passes of `BAND_Q`, integrated backwards from
    /// its end as Schroeder does, then the straight line through it from 5 to `down` dB under its
    /// start, by least squares, carried on to 60 dB.
    pub(crate) fn seconds(response: &[f32], rate: u32, hertz: f32, down: f64) -> f32 {
        let g = (std::f64::consts::PI * f64::from(hertz) / f64::from(rate)).tan();
        let k = 1.0 / super::BAND_Q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let mut states = [[0.0_f64; 2]; 2];
        let banded: Vec<f64> = response
            .iter()
            .map(|&sample| {
                let mut x = f64::from(sample);
                for [ic1, ic2] in &mut states {
                    let v3 = x - *ic2;
                    let v1 = a1 * *ic1 + a1 * g * v3;
                    let v2 = *ic2 + g * v1;
                    *ic1 = 2.0 * v1 - *ic1;
                    *ic2 = 2.0 * v2 - *ic2;
                    // The band-pass, `k` times the band, passes its centre at unity.
                    x = k * v1;
                }
                x
            })
            .collect();
        let mut left = 0.0;
        let mut schroeder: Vec<f64> = banded
            .iter()
            .rev()
            .map(|x| {
                left += x * x;
                left
            })
            .collect();
        schroeder.reverse();
        let total = schroeder[0];
        let (mut n, mut sum_t, mut sum_d, mut sum_tt, mut sum_td) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (at, energy) in schroeder.iter().enumerate() {
            let db = 10.0 * (energy / total).log10();
            if db <= -5.0 && db >= -down {
                let t = at as f64 / f64::from(rate);
                n += 1.0;
                sum_t += t;
                sum_d += db;
                sum_tt += t * t;
                sum_td += t * db;
            }
        }
        let slope = (n * sum_td - sum_t * sum_d) / (n * sum_tt - sum_t * sum_t);
        (-60.0 / slope) as f32
    }

    /// How a tail's seconds compare with what's measured of a click through the reverb, its two
    /// sides added, at each of `TAIL_HZ`: what's measured over what's said.
    pub(crate) fn ratios(
        tail: super::Tail,
        mut click: impl FnMut(f32) -> [f32; 2],
        rate: u32,
    ) -> [f32; 2] {
        let longest = tail.seconds[0].max(tail.seconds[1]);
        // Long enough for the slowest to have fallen past 60 dB, so the backward integration
        // isn't cut short.
        let frames = ((tail.pre + longest * 1.2 + 0.25) * rate as f32) as usize;
        let response: Vec<f32> = (0..frames)
            .map(|at| {
                let out = click(if at == 0 { 1.0 } else { 0.0 });
                out[0] + out[1]
            })
            .collect();
        std::array::from_fn(|at| {
            seconds(&response, rate, super::TAIL_HZ[at], 35.0) / tail.seconds[at]
        })
    }

    /// The first frame at least -40 dB under the loudest, in seconds: when it's first heard.
    pub(crate) fn onset(response: &[f32], rate: u32) -> f32 {
        let loudest = response.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
        let first = response
            .iter()
            .position(|s| s.abs() >= loudest * 0.01)
            .unwrap_or(0);
        first as f32 / rate as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::random::NumericalRecipesLcg;

    #[test]
    fn a_measured_exponential_falls_as_it_was_made() {
        // White noise falling 60 dB in 1.3 s is measured at 1.3 s at either frequency.
        let rate = 48_000;
        let mut rng = NumericalRecipesLcg::new(1);
        let response: Vec<f32> = (0..rate as usize * 2)
            .map(|at| {
                let sample = (rng.next_u32() >> 16) as f32 / 32_768.0 - 1.0;
                sample * 10_f32.powf(-3.0 * at as f32 / (1.3 * rate as f32))
            })
            .collect();
        for hertz in TAIL_HZ {
            let measured = measure::seconds(&response, rate, hertz, 35.0);
            assert!((measured / 1.3 - 1.0).abs() < 0.03, "{hertz}: {measured}");
        }
    }

    #[test]
    fn a_loop_of_plain_delay_falls_at_one_rate() {
        // With nothing holding some frequencies back longer than others, it's the trip's.
        // Half each trip of a tenth of a second is 6 dB, so 60 dB is ten trips.
        let seconds = loop_seconds(1_000.0, 48_000, |_| (0.5, 4_800.0));
        let trips = 60.0 / (20.0 * 2_f32.log10());
        assert!((seconds / (trips * 0.1) - 1.0).abs() < 1e-3, "{seconds}");
        let held = kept_and_held(0.3, |omega| Complex::delay(omega, 4_800.0).scale(0.5));
        assert!(
            (held.0 - 0.5).abs() < 1e-9 && (held.1 - 4_800.0).abs() < 1e-3,
            "{held:?}"
        );
    }

    #[test]
    fn an_allpass_in_a_loop_makes_it_fall_more_slowly_than_its_average_trip() {
        // An all-pass of 1000 frames spreading by 0.6 holds each frequency back from 250 to
        // 4000 frames, 1000 on average, so with 3800 frames of delay the trip is 4800 on
        // average, but the frequencies it holds longest fall slowest, and last.
        let response = |omega: f64| {
            let delayed = Complex::delay(omega, 1_000.0);
            let allpass = delayed
                .add(Complex(-0.6, 0.0))
                .over(Complex::ONE.add(delayed.scale(-0.6)));
            allpass.times(Complex::delay(omega, 3_800.0)).scale(0.5)
        };
        let seconds = loop_seconds(1_000.0, 48_000, |omega| kept_and_held(omega, response));
        let average = 60.0 / (20.0 * 2_f32.log10()) * 0.1;
        assert!(seconds > average * 1.1, "{seconds} against {average}");
    }
}
