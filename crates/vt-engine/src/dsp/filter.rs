//! Each track's filter. The clean modes share a TPT state-variable filter, which stays stable near
//! the top of the range. The ladder is the character filter, and costs more.

use vt_model::song::FilterType;

/// How far `Drive` turns up what goes into the filter, at full.
const DRIVE_GAIN: f32 = 7.0;
/// Where the ladder's feedback starts to self-oscillate, just past 4.
const LADDER_FEEDBACK: f32 = 4.3;
/// A stage's slope can't fall below this, or a loud signal would stop it responding.
const LADDER_MIN_SLOPE: f32 = 0.01;
/// What the rational `tanh` is good for.
const TANH_RANGE: f32 = 4.0;

/// `tanh(x) / x`, as a rational function, for `x` from -4 to 4.
#[inline]
fn tanh_over_x(x: f32) -> f32 {
    let x2 = x * x;
    ((x2 + 105.0) * x2 + 945.0) / ((15.0 * x2 + 420.0) * x2 + 945.0)
}

/// A soft clip that leaves quiet signals alone.
#[inline]
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-TANH_RANGE, TANH_RANGE);
    x * tanh_over_x(x)
}

/// Filter coefficients. Cutoff is `0..=1`, exponential from 20 Hz to 20 kHz.
#[derive(Debug, Clone, Copy)]
pub struct Coefficients {
    g: f32,
    k: f32,
    a1: f32,
    /// The same at twice the rate, for what saturates.
    double_g: f32,
    double_a1: f32,
    /// The ladder's feedback.
    ladder_k: f32,
}

impl Coefficients {
    /// Work out coefficients.
    pub fn new(cutoff: f32, resonance: f32, rate: u32) -> Self {
        let frequency = 20.0 * 1000_f32.powf(cutoff);
        let nyquist_safe = frequency.min(rate as f32 * 0.45);
        let g = (std::f32::consts::PI * nyquist_safe / rate as f32).tan();
        // Q runs from a flat response to a strong but stable peak.
        let q = 0.707 + resonance * 9.3;
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let double_g = (std::f32::consts::PI * nyquist_safe / (2.0 * rate as f32)).tan();
        Self {
            g,
            k,
            a1,
            double_g,
            double_a1: 1.0 / (1.0 + double_g * (double_g + k)),
            ladder_k: resonance * LADDER_FEEDBACK,
        }
    }
}

/// A state-variable filter's state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// One sample's band and low outputs.
    #[inline]
    fn tick(&mut self, input: f32, c: &Coefficients) -> (f32, f32) {
        let v3 = input - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a1 * c.g * v3;
        let v2 = self.ic2 + c.g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v1, v2)
    }

    /// The same at twice the rate with a soft clip in the feedback, so resonance saturates instead
    /// of growing.
    #[inline]
    fn tick_driven(&mut self, input: f32, c: &Coefficients) -> (f32, f32) {
        let v3 = input - self.ic2;
        let v1 = c.double_a1 * self.ic1 + c.double_a1 * c.double_g * v3;
        let v2 = self.ic2 + c.double_g * v1;
        self.ic1 = 2.0 * soft_clip(v1) - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v1, v2)
    }
}

/// A chain of first-order allpass filters, `(a + z^-1) / (1 + a z^-1)`.
#[derive(Debug, Clone, Copy, Default)]
struct Allpass {
    state: [f32; 3],
}

impl Allpass {
    #[inline]
    fn process(&mut self, mut x: f32, coefficients: &[f32; 3]) -> f32 {
        for (state, a) in self.state.iter_mut().zip(coefficients) {
            let y = a * x + *state;
            *state = x - a * y;
            x = y;
        }
        x
    }
}

/// A polyphase halfband's two branches, for going to and from twice the rate. The coefficients are
/// from VCV Fundamental's VCF, GPL 3 or later.
#[allow(clippy::excessive_precision)]
const HALFBAND_A: [f32; 3] = [0.062_822_416_060, 0.424_380_855_72, 0.781_861_460_39];
#[allow(clippy::excessive_precision)]
const HALFBAND_B: [f32; 3] = [0.223_807_330_34, 0.616_534_435_04, 0.927_473_594_87];

/// Going to twice the rate and back, so that what saturates aliases less.
#[derive(Debug, Clone, Copy, Default)]
struct Oversampler {
    up: [Allpass; 2],
    down: [Allpass; 2],
}

impl Oversampler {
    /// One sample at the normal rate, with `step` run twice at the higher one.
    #[inline]
    fn process(&mut self, input: f32, mut step: impl FnMut(f32) -> f32) -> f32 {
        let first = step(self.up[0].process(input, &HALFBAND_A));
        let second = step(self.up[1].process(input, &HALFBAND_B));
        // The branches swap on the way down, for unity gain in the passband.
        let a = self.down[0].process(second, &HALFBAND_A);
        let b = self.down[1].process(first, &HALFBAND_B);
        0.5 * (a + b)
    }
}

/// A saturating four-pole transistor ladder at twice the rate, so its saturation aliases less.
/// Huovilainen's model, with Zavalishin's trapezoidal integrators and Mystran's per-stage
/// linearisation of `tanh`, after VCV Fundamental's VCF.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ladder {
    state: [f32; 4],
    last: [f32; 4],
    last_input: f32,
}

impl Ladder {
    /// One step at twice the rate. Each stage's `tanh` is replaced by its slope at the last output,
    /// which leaves a linear system that can be solved for the feedback within the same sample, so
    /// the pitch stays true at high cutoffs. The input's saturation comes from the same idea.
    #[inline]
    fn step(&mut self, input: f32, c: &Coefficients) -> f32 {
        let g = c.double_g;
        let slope = |y: f32| tanh_over_x(y.clamp(-TANH_RANGE, TANH_RANGE)).max(LADDER_MIN_SLOPE);
        let slopes = self.last.map(slope);
        let input_slope = slope(self.last_input);
        // Each stage's output is `gain * what it's fed + offset`. Follow those through the four.
        let mut stage = [(0.0, 0.0); 4];
        let (mut fed_gain, mut fed_offset) = (g * input_slope, 0.0);
        for (index, slot) in stage.iter_mut().enumerate() {
            let denominator = 1.0 / (1.0 + g * slopes[index]);
            let gain = fed_gain * denominator;
            let offset = (fed_offset + self.state[index]) * denominator;
            *slot = (gain, offset);
            fed_gain = g * slopes[index] * gain;
            fed_offset = g * slopes[index] * offset;
        }
        let (out_gain, out_offset) = stage[3];
        let u = (input - c.ladder_k * out_offset) / (1.0 + c.ladder_k * out_gain);
        self.last_input = u;
        for (index, (gain, offset)) in stage.iter().enumerate() {
            let y = gain * u + offset;
            self.last[index] = y;
            self.state[index] = 2.0 * y - self.state[index];
        }
        let y3 = self.last[3];
        // Soft-clipped to 2, which is gentle at normal levels.
        y3 * tanh_over_x((y3 * 0.5).clamp(-TANH_RANGE, TANH_RANGE))
    }
}

/// One channel of a track's filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Channel {
    svf: Svf,
    ladder: Ladder,
    oversampler: Oversampler,
}

impl Channel {
    /// Filter one sample. `drive` runs from 0, which takes the clean path, to 1.
    #[inline]
    pub fn process(&mut self, input: f32, kind: FilterType, drive: f32, c: &Coefficients) -> f32 {
        if drive <= 0.0 && kind != FilterType::Ladder {
            let (band, low) = self.svf.tick(input, c);
            return svf_output(kind, input, band, low, c);
        }
        self.saturating(input, kind, drive, c)
    }

    /// What saturates, at twice the rate. It's kept apart so the clean path stays small enough to
    /// inline.
    #[inline(never)]
    fn saturating(&mut self, input: f32, kind: FilterType, drive: f32, c: &Coefficients) -> f32 {
        let gain = 1.0 + drive * DRIVE_GAIN;
        let output = if kind == FilterType::Ladder {
            let ladder = &mut self.ladder;
            self.oversampler
                .process(input * gain, |x| ladder.step(x, c))
        } else {
            let svf = &mut self.svf;
            self.oversampler.process(input * gain, |x| {
                let (band, low) = svf.tick_driven(x, c);
                soft_clip(svf_output(kind, x, band, low, c))
            })
        };
        output / gain.sqrt()
    }
}

/// The output a mode takes from the state-variable filter.
#[inline]
fn svf_output(kind: FilterType, input: f32, band: f32, low: f32, c: &Coefficients) -> f32 {
    match kind {
        FilterType::HighPass => input - c.k * band - low,
        FilterType::BandPass => c.k * band,
        FilterType::Notch => input - c.k * band,
        FilterType::LowPass | FilterType::Ladder => low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A cutoff value for a frequency in hertz.
    fn cutoff(hertz: f32) -> f32 {
        (hertz / 20.0).log10() / 3.0
    }

    /// The gain of a sine through a filter, in dB, once it has settled.
    fn gain_db(kind: FilterType, cut: f32, res: f32, drive: f32, hertz: f32, level: f32) -> f32 {
        let c = Coefficients::new(cutoff(cut), res, RATE);
        let mut channel = Channel::default();
        let frames = RATE as usize / 2;
        let (mut power_in, mut power_out) = (0.0_f64, 0.0_f64);
        for frame in 0..frames {
            let t = frame as f32 / RATE as f32;
            let input = (t * hertz * std::f32::consts::TAU).sin() * level;
            let output = channel.process(input, kind, drive, &c);
            assert!(output.is_finite());
            if frame > frames / 2 {
                power_in += f64::from(input * input);
                power_out += f64::from(output * output);
            }
        }
        10.0 * (power_out / power_in).log10() as f32
    }

    fn near(actual: f32, expected: f32, within: f32) {
        assert!(
            (actual - expected).abs() < within,
            "{actual} and {expected}"
        );
    }

    #[test]
    fn low_pass_passes_lows_and_slopes_at_12_db() {
        let db = |hertz| gain_db(FilterType::LowPass, 1000.0, 0.0, 0.0, hertz, 0.1);
        near(db(100.0), 0.0, 0.1);
        near(db(1000.0), -3.0, 0.3);
        near(db(4000.0) - db(8000.0), 12.0, 3.0);
    }

    #[test]
    fn high_pass_is_the_mirror() {
        let db = |hertz| gain_db(FilterType::HighPass, 1000.0, 0.0, 0.0, hertz, 0.1);
        near(db(10_000.0), 0.0, 0.2);
        near(db(1000.0), -3.0, 0.3);
        assert!(db(100.0) < -35.0, "{}", db(100.0));
    }

    #[test]
    fn band_pass_peaks_at_unity_and_rejects_either_side() {
        for res in [0.0, 0.5, 1.0] {
            let db = |hertz| gain_db(FilterType::BandPass, 1000.0, res, 0.0, hertz, 0.1);
            near(db(1000.0), 0.0, 0.2);
            assert!(db(100.0) < -10.0, "{}", db(100.0));
            assert!(db(10_000.0) < -10.0, "{}", db(10_000.0));
        }
    }

    #[test]
    fn notch_cuts_only_at_the_cutoff() {
        let db = |hertz| gain_db(FilterType::Notch, 1000.0, 0.5, 0.0, hertz, 0.1);
        assert!(db(1000.0) < -50.0, "{}", db(1000.0));
        near(db(100.0), 0.0, 0.2);
        near(db(10_000.0), 0.0, 0.2);
    }

    #[test]
    fn resonance_raises_a_peak_at_the_cutoff() {
        let db = |res| gain_db(FilterType::LowPass, 1000.0, res, 0.0, 1000.0, 0.1);
        assert!(db(1.0) > db(0.0) + 15.0, "{} and {}", db(1.0), db(0.0));
    }

    #[test]
    fn drive_at_zero_changes_nothing_and_above_it_adds_harmonics() {
        let c = Coefficients::new(cutoff(5000.0), 0.3, RATE);
        let tone =
            |frame: usize| (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin();
        let harmonic = |drive: f32| {
            let mut channel = Channel::default();
            let (mut real, mut imaginary) = (0.0_f32, 0.0_f32);
            for frame in 0..RATE as usize {
                let output = channel.process(tone(frame) * 0.5, FilterType::LowPass, drive, &c);
                let phase = frame as f32 / RATE as f32 * 660.0 * std::f32::consts::TAU;
                real += output * phase.cos();
                imaginary += output * phase.sin();
            }
            (real * real + imaginary * imaginary).sqrt() / RATE as f32
        };
        assert!(harmonic(0.0) < 1e-3, "{}", harmonic(0.0));
        assert!(harmonic(1.0) > 0.02, "{}", harmonic(1.0));
    }

    #[test]
    fn drive_keeps_the_level_in_hand() {
        for kind in [FilterType::LowPass, FilterType::Ladder] {
            for drive in [0.25, 0.5, 1.0] {
                let db = gain_db(kind, 20_000.0, 0.0, drive, 200.0, 0.5);
                near(db, 0.0, 8.0);
            }
        }
    }

    #[test]
    fn the_ladder_slopes_at_24_db_and_has_little_resonance_when_low() {
        let db = |hertz| gain_db(FilterType::Ladder, 500.0, 0.0, 0.0, hertz, 0.05);
        near(db(50.0), 0.0, 0.5);
        eprintln!(
            "slope: {} {} {} {}",
            db(1000.0),
            db(2000.0),
            db(4000.0),
            db(8000.0)
        );
        near(db(2000.0) - db(4000.0), 24.0, 4.0);
    }

    #[test]
    fn the_ladder_loses_bass_as_resonance_rises() {
        let db = |res| gain_db(FilterType::Ladder, 2000.0, res, 0.0, 100.0, 0.05);
        assert!(db(0.8) < db(0.0) - 3.0, "{} and {}", db(0.8), db(0.0));
    }

    /// The frequency of what's left once a filter has been struck once.
    fn ringing_at(kind: FilterType, cut: f32, res: f32) -> f32 {
        let c = Coefficients::new(cutoff(cut), res, RATE);
        let mut channel = Channel::default();
        channel.process(0.5, kind, 0.0, &c);
        let frames = RATE as usize * 2;
        let (mut crossings, mut last, mut peak) = (0, 0.0_f32, 0.0_f32);
        for frame in 0..frames {
            let output = channel.process(0.0, kind, 0.0, &c);
            assert!(output.is_finite());
            if frame > frames / 2 {
                if last <= 0.0 && output > 0.0 {
                    crossings += 1;
                }
                peak = peak.max(output.abs());
            }
            last = output;
        }
        assert!(peak > 0.05, "it keeps ringing: {peak}");
        crossings as f32
    }

    #[test]
    fn the_ladder_self_oscillates_near_its_cutoff() {
        for cut in [200.0, 1000.0, 4000.0] {
            let pitch = ringing_at(FilterType::Ladder, cut, 1.0);
            let cents = 1200.0 * (pitch / cut).log2();
            assert!(cents.abs() < 100.0, "{cut} rang at {pitch}");
        }
    }

    #[test]
    fn every_type_survives_the_extremes_and_fast_changes() {
        let kinds = [
            FilterType::LowPass,
            FilterType::HighPass,
            FilterType::BandPass,
            FilterType::Notch,
            FilterType::Ladder,
        ];
        for kind in kinds {
            for drive in [0.0, 1.0] {
                let mut channel = Channel::default();
                for frame in 0..RATE as usize {
                    // The cutoff and resonance sweep fast, and the input is loud.
                    let sweep = (frame as f32 / 37.0).sin() * 0.5 + 0.5;
                    let c = Coefficients::new(sweep, 1.0 - sweep, RATE);
                    let input = if frame % 40 < 20 { 1.0 } else { -1.0 };
                    let output = channel.process(input, kind, drive, &c);
                    assert!(
                        output.is_finite() && output.abs() < 20.0,
                        "{kind:?} {output}"
                    );
                }
            }
        }
    }

    /// How loud a frequency is in a filter's output, from a sine at another.
    fn level_at(kind: FilterType, drive: f32, tone: f32, listen: f32) -> f32 {
        let c = Coefficients::new(1.0, 0.0, RATE);
        let mut channel = Channel::default();
        let frames = RATE as usize;
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        let tau = std::f64::consts::TAU;
        for frame in 0..frames {
            let at = frame as f64 / f64::from(RATE);
            let output = channel.process(
                (at * f64::from(tone) * tau).sin() as f32 * 0.9,
                kind,
                drive,
                &c,
            );
            real += f64::from(output) * (at * f64::from(listen) * tau).cos();
            imaginary += f64::from(output) * (at * f64::from(listen) * tau).sin();
        }
        ((real * real + imaginary * imaginary).sqrt() / frames as f64) as f32
    }

    #[test]
    fn oversampling_keeps_aliasing_from_drive_well_down() {
        // A 5 kHz sine, driven hard. Its 7th harmonic folds back to 13 kHz and its 9th to 3 kHz, where
        // they'd be heard. Its 5th folds to 23 kHz, too close to the top for a halfband to catch.
        let db = |kind, alias| {
            20.0 * (level_at(kind, 1.0, 5000.0, alias) / level_at(kind, 1.0, 5000.0, 5000.0))
                .log10()
        };
        for kind in [FilterType::Ladder, FilterType::LowPass] {
            for (alias, most) in [(3000.0, -50.0), (13_000.0, -50.0), (23_000.0, -30.0)] {
                let found = db(kind, alias);
                assert!(found < most, "{kind:?} {alias} is at {found} dB");
            }
        }
    }

    #[test]
    fn the_ladders_delay_is_small() {
        let c = Coefficients::new(1.0, 0.0, RATE);
        let mut channel = Channel::default();
        let mut peak_at = 0;
        let mut peak = 0.0_f32;
        for frame in 0..200 {
            let input = if frame == 0 { 1.0 } else { 0.0 };
            let output = channel.process(input, FilterType::Ladder, 0.0, &c);
            if output.abs() > peak {
                peak = output.abs();
                peak_at = frame;
            }
        }
        assert!(
            peak_at <= 12,
            "{peak_at} frames is about {} ms",
            peak_at as f32 / 48.0
        );
    }
}
