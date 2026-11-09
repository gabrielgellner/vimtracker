//! Each track's filter. The clean modes share a TPT state-variable filter, which stays stable near
//! the top of the range. The ladder, diode, Korg 35 and Vital's `Dirty` are the character filters,
//! and cost more.

use vt_model::song::{FilterType, Track};

use crate::dsp::diode_filter::{self, DiodeFilter, RESONANT_SHARE};
use crate::dsp::dirty_filter::{self, DirtyFilter};
use crate::dsp::korg35::{self, Korg35};
use crate::dsp::oversampler::Oversampler;

/// How far `Drive` turns up what goes into the filter, at full.
const DRIVE_GAIN: f32 = 7.0;
/// Where the ladder's feedback starts to self-oscillate, just past 4.
const LADDER_FEEDBACK: f32 = 4.3;
/// A stage's slope can't fall below this, or a loud signal would stop it responding.
const LADDER_MIN_SLOPE: f32 = 0.01;
/// What goes into the diode ladder, and what comes out of it at no resonance, as gains. Its last
/// stage clips at 1, so a full-level sound goes in at half, and comes out about as loud as it went
/// in.
const DIODE_INPUT: f32 = 0.5;
const DIODE_OUTPUT: f32 = 1.04;
/// How much higher the diode ladder's cutoff is set for each fraction of its rate. Its stages hear
/// each other a sample late, which pulls its resonance flat as the cutoff rises.
const DIODE_HIGH_TUNING: f32 = 2.2;
/// How far its feedback high-pass pulls its resonance sharp near the bottom, in Hz.
const DIODE_LOW_TUNING: f32 = 12.0;
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
pub(crate) fn soft_clip(x: f32) -> f32 {
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
    /// The diode ladder's cutoff as a fraction of twice the rate, set so it resonates at the cutoff
    /// asked for, and its feedback high-pass at that rate.
    diode_cutoff: f32,
    diode_feedback: f32,
    resonance: f32,
    /// The Korg 35's one-pole coefficient at twice the rate, `g / (1 + g)`, and its feedback.
    korg_coefficient: f32,
    korg_feedback: f32,
    /// The `Dirty` filter's tuning at twice the rate.
    dirty: dirty_filter::Tuning,
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
            diode_cutoff: {
                let share = frequency / RESONANT_SHARE / (2.0 * rate as f32);
                let low = (1.0 - DIODE_LOW_TUNING / frequency).max(0.5);
                share * (1.0 + DIODE_HIGH_TUNING * share) * low
            },
            diode_feedback: diode_filter::feedback_coefficient(2.0 * rate as f32),
            resonance,
            korg_coefficient: double_g / (1.0 + double_g),
            korg_feedback: resonance * korg35::MOST_FEEDBACK,
            dirty: dirty_filter::Tuning::new(double_g / (1.0 + double_g), resonance),
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
        self.tick_tuned(input, c.g, c.a1)
    }

    /// The same, tuned by `g`, `tan(pi f)` for its frequency `f` as a fraction of the rate, and
    /// `a1`, `1 / (1 + g (g + k))` for its damping `k`, so a bank of them can share theirs between
    /// sides.
    #[inline]
    pub(crate) fn tick_tuned(&mut self, input: f32, g: f32, a1: f32) -> (f32, f32) {
        let v3 = input - self.ic2;
        let v1 = a1 * self.ic1 + a1 * g * v3;
        let v2 = self.ic2 + g * v1;
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
    diode: DiodeFilter,
    korg: Korg35,
    dirty: DirtyFilter,
    oversampler: Oversampler,
}

impl Channel {
    /// Filter one sample. `drive` runs from 0, which takes the clean path, to 1.
    #[inline]
    pub fn process(&mut self, input: f32, kind: FilterType, drive: f32, c: &Coefficients) -> f32 {
        if drive <= 0.0 && !kind.saturates() {
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
        if kind == FilterType::Diode {
            // The diode ladder takes the drive itself, and evens out its level.
            let diode = &mut self.diode;
            return self.oversampler.process(input * DIODE_INPUT, |x| {
                diode.next(x, c.diode_cutoff, c.resonance, gain, c.diode_feedback) * DIODE_OUTPUT
            });
        }
        if kind == FilterType::Dirty {
            // Its drive goes only to what comes straight into its loop, and adds a little
            // resonance.
            let settings = dirty_filter::Settings::new(&c.dirty, drive, gain);
            let dirty = &mut self.dirty;
            return self
                .oversampler
                .process(input, |x| dirty.next(x, &settings))
                / gain.sqrt();
        }
        let output = if kind == FilterType::Ladder {
            let ladder = &mut self.ladder;
            self.oversampler
                .process(input * gain, |x| ladder.step(x, c))
        } else if kind == FilterType::Ms20LowPass {
            let korg = &mut self.korg;
            self.oversampler.process(input * gain, |x| {
                korg.low_pass(x, c.korg_coefficient, c.korg_feedback)
            })
        } else if kind == FilterType::Ms20HighPass {
            let korg = &mut self.korg;
            self.oversampler.process(input * gain, |x| {
                korg.high_pass(x, c.korg_coefficient, c.korg_feedback)
            })
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

/// How much a filter lets through at a frequency, in dB, for a quiet signal. This is the filter's
/// exact response, so a drawing of it shows what's heard.
///
/// `Drive` only changes the level and the saturation of the others, but it adds resonance to
/// `Dirty`, which changes its shape, so that's drawn. Its level isn't, as for the others.
pub fn response_db(
    kind: FilterType,
    cutoff: f32,
    resonance: f32,
    drive: f32,
    rate: u32,
    hertz: f32,
) -> f32 {
    let c = Coefficients::new(cutoff, resonance, rate);
    let hertz = hertz.min(rate as f32 * 0.499);
    let power = if kind == FilterType::Dirty {
        let settings = dirty_filter::Settings::new(&c.dirty, drive, 1.0);
        dirty_filter::response(&settings, hertz / (2.0 * rate as f32))
    } else if kind == FilterType::Diode {
        let gain = DIODE_INPUT * DIODE_OUTPUT;
        gain * gain
            * diode_filter::response(
                c.diode_cutoff,
                resonance,
                c.diode_feedback,
                hertz / (2.0 * rate as f32),
            )
    } else if matches!(kind, FilterType::Ms20LowPass | FilterType::Ms20HighPass) {
        let along = (std::f32::consts::PI * hertz / (2.0 * rate as f32)).tan() / c.double_g;
        korg35::response(kind == FilterType::Ms20HighPass, c.korg_feedback, along)
    } else if kind == FilterType::Ladder {
        // Four one-poles in a loop: 1 / ((1 + jw)^4 + k).
        let w = (std::f32::consts::PI * hertz / (2.0 * rate as f32)).tan() / c.double_g;
        let w2 = w * w;
        let real = 1.0 - 6.0 * w2 + w2 * w2 + c.ladder_k;
        let imaginary = 4.0 * w - 4.0 * w * w2;
        1.0 / (real * real + imaginary * imaginary)
    } else {
        let w = (std::f32::consts::PI * hertz / rate as f32).tan() / c.g;
        let (w2, k2) = (w * w, c.k * c.k);
        let rest = (1.0 - w2) * (1.0 - w2);
        let top = match kind {
            FilterType::HighPass => w2 * w2,
            FilterType::BandPass => k2 * w2,
            FilterType::Notch => rest,
            FilterType::LowPass
            | FilterType::Ladder
            | FilterType::Diode
            | FilterType::Ms20LowPass
            | FilterType::Ms20HighPass
            | FilterType::Dirty => 1.0,
        };
        top / (rest + k2 * w2)
    };
    10.0 * power.max(1e-12).log10()
}

/// How much a track's filter and utility filter let through at a frequency, in dB, from their
/// settings alone. A filter with nothing to do is out of the way, as it is when playing.
pub fn track_response_db(track: &Track, rate: u32, hertz: f32) -> f32 {
    let value = |setting: u8| f32::from(setting) / 255.0;
    let open = track.filter == FilterType::LowPass
        && track.cutoff == u8::MAX
        && track.resonance == 0
        && track.drive == 0;
    let mut db = 0.0;
    if !open {
        let (cutoff, resonance) = (value(track.cutoff), value(track.resonance));
        db += response_db(
            track.filter,
            cutoff,
            resonance,
            value(track.drive),
            rate,
            hertz,
        );
    }
    if track.low_cut > 0 {
        db += response_db(
            FilterType::HighPass,
            value(track.low_cut),
            0.0,
            0.0,
            rate,
            hertz,
        );
    }
    if track.high_cut < u8::MAX {
        db += response_db(
            FilterType::LowPass,
            value(track.high_cut),
            0.0,
            0.0,
            rate,
            hertz,
        );
    }
    db
}

/// One channel of a track's utility filter: a high-pass and a low-pass, flat and without resonance,
/// for thinning out a sound after its main filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Trim {
    high_pass: Svf,
    low_pass: Svf,
}

impl Trim {
    /// Pass through the high-pass.
    #[inline]
    pub fn low_cut(&mut self, input: f32, c: &Coefficients) -> f32 {
        let (band, low) = self.high_pass.tick(input, c);
        input - c.k * band - low
    }

    /// Pass through the low-pass.
    #[inline]
    pub fn high_cut(&mut self, input: f32, c: &Coefficients) -> f32 {
        self.low_pass.tick(input, c).1
    }

    /// Get ready to start cutting highs at this input, so its output doesn't start from silence.
    #[inline]
    pub fn prime_high_cut(&mut self, input: f32) {
        self.low_pass = Svf {
            ic1: 0.0,
            ic2: input,
        };
    }

    /// Forget the low cut, which then starts with nothing to take away.
    #[inline]
    pub fn clear_low_cut(&mut self) {
        self.high_pass = Svf::default();
    }
}

/// The output a mode takes from the state-variable filter.
#[inline]
fn svf_output(kind: FilterType, input: f32, band: f32, low: f32, c: &Coefficients) -> f32 {
    match kind {
        FilterType::HighPass => input - c.k * band - low,
        FilterType::BandPass => c.k * band,
        FilterType::Notch => input - c.k * band,
        FilterType::LowPass
        | FilterType::Ladder
        | FilterType::Diode
        | FilterType::Ms20LowPass
        | FilterType::Ms20HighPass
        | FilterType::Dirty => low,
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
    fn the_worked_out_response_matches_the_filter_itself() {
        for kind in FilterType::ALL {
            for res in [0.0, 0.5, 0.8] {
                for hertz in [100.0, 400.0, 1000.0, 2500.0, 8000.0] {
                    let worked_out = response_db(kind, cutoff(1000.0), res, 0.0, RATE, hertz);
                    // A notch at its very centre is too deep to measure.
                    if worked_out < -60.0 {
                        continue;
                    }
                    let measured = gain_db(kind, 1000.0, res, 0.0, hertz, 0.01);
                    // The diode's stages hear each other a sample late, which the analog model
                    // doesn't, so it's a little out at its peak.
                    let within = if kind == FilterType::Diode { 1.0 } else { 0.5 };
                    assert!(
                        (worked_out - measured).abs() < within,
                        "{kind:?} res {res} at {hertz}: {worked_out} and {measured}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_tracks_response_includes_its_utility_filter_and_is_flat_when_open() {
        let mut track = Track {
            filter: FilterType::LowPass,
            cutoff: 0xFF,
            resonance: 0,
            drive: 0,
            low_cut: 0,
            high_cut: 0xFF,
            ..Track::default()
        };
        for hertz in [20.0, 1000.0, 20_000.0] {
            near(track_response_db(&track, RATE, hertz), 0.0, 0.001);
        }
        track.low_cut = 0x80;
        assert!(track_response_db(&track, RATE, 20.0) < -20.0);
        near(track_response_db(&track, RATE, 20_000.0), 0.0, 0.1);
        track.low_cut = 0;
        track.high_cut = 0x80;
        assert!(track_response_db(&track, RATE, 20_000.0) < -20.0);
        near(track_response_db(&track, RATE, 20.0), 0.0, 0.1);
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
        for kind in [
            FilterType::LowPass,
            FilterType::Ladder,
            FilterType::Diode,
            FilterType::Ms20LowPass,
            FilterType::Ms20HighPass,
            FilterType::Dirty,
        ] {
            // Open, so only the drive changes the level.
            let cut = if kind == FilterType::Ms20HighPass {
                20.0
            } else {
                20_000.0
            };
            for drive in [0.25, 0.5, 1.0] {
                let db = gain_db(kind, cut, 0.0, drive, 200.0, 0.5);
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
        driven_ringing_at(kind, cut, res, 0.0)
    }

    /// The same with some drive.
    fn driven_ringing_at(kind: FilterType, cut: f32, res: f32, drive: f32) -> f32 {
        let c = Coefficients::new(cutoff(cut), res, RATE);
        let mut channel = Channel::default();
        channel.process(0.5, kind, drive, &c);
        let frames = RATE as usize * 2;
        let (mut crossings, mut last, mut peak) = (0, 0.0_f32, 0.0_f32);
        for frame in 0..frames {
            let output = channel.process(0.0, kind, drive, &c);
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
    fn the_diode_passes_the_bass_at_its_level_and_loses_it_as_resonance_rises() {
        let db = |res| gain_db(FilterType::Diode, 2000.0, res, 0.0, 100.0, 0.05);
        near(db(0.0), 0.0, 1.0);
        assert!(db(0.8) < db(0.0) - 3.0, "{} and {}", db(0.8), db(0.0));
    }

    #[test]
    fn the_diode_self_oscillates_near_its_cutoff() {
        for cut in [200.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0] {
            let pitch = ringing_at(FilterType::Diode, cut, 1.0);
            let cents = 1200.0 * (pitch / cut).log2();
            assert!(cents.abs() < 50.0, "{cut} rang at {pitch}");
        }
    }

    #[test]
    fn every_type_survives_the_extremes_and_fast_changes() {
        for kind in FilterType::ALL {
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
        let kinds = [
            FilterType::Ladder,
            FilterType::LowPass,
            FilterType::Diode,
            FilterType::Ms20LowPass,
            FilterType::Ms20HighPass,
            FilterType::Dirty,
        ];
        for kind in kinds {
            for (alias, most) in [(3000.0, -50.0), (13_000.0, -50.0), (23_000.0, -30.0)] {
                let found = db(kind, alias);
                assert!(found < most, "{kind:?} {alias} is at {found} dB");
            }
        }
    }

    fn trimmed(hertz: f32, low_cut: Option<f32>, high_cut: Option<f32>) -> f32 {
        let low = low_cut.map(|cut| Coefficients::new(cutoff(cut), 0.0, RATE));
        let high = high_cut.map(|cut| Coefficients::new(cutoff(cut), 0.0, RATE));
        let mut trim = Trim::default();
        let frames = RATE as usize / 2;
        let (mut power_in, mut power_out) = (0.0_f64, 0.0_f64);
        for frame in 0..frames {
            let t = frame as f32 / RATE as f32;
            let input = (t * hertz * std::f32::consts::TAU).sin();
            let mut output = input;
            if let Some(c) = &low {
                output = trim.low_cut(output, c);
            }
            if let Some(c) = &high {
                output = trim.high_cut(output, c);
            }
            if frame > frames / 2 {
                power_in += f64::from(input * input);
                power_out += f64::from(output * output);
            }
        }
        10.0 * (power_out / power_in).log10() as f32
    }

    #[test]
    fn the_utility_filter_is_flat_and_cuts_at_both_ends() {
        let band = |hertz| trimmed(hertz, Some(200.0), Some(4000.0));
        near(band(900.0), 0.0, 0.3);
        near(band(200.0), -3.0, 0.5);
        near(band(4000.0), -3.0, 0.5);
        assert!(band(20.0) < -35.0, "{}", band(20.0));
        assert!(band(15_000.0) < -20.0, "{}", band(15_000.0));
        // No peak near either cutoff.
        for hertz in [100.0, 300.0, 500.0, 2000.0, 3000.0, 5000.0] {
            assert!(band(hertz) < 0.2, "{hertz} is at {}", band(hertz));
        }
    }

    #[test]
    fn a_primed_high_cut_starts_without_a_jump() {
        let c = Coefficients::new(cutoff(5000.0), 0.0, RATE);
        let mut trim = Trim::default();
        trim.prime_high_cut(0.8);
        let first = trim.high_cut(0.8, &c);
        assert!((first - 0.8).abs() < 1e-3, "{first}");
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

    #[test]
    fn the_korg_35_low_pass_keeps_its_bass_and_slopes_at_12_db() {
        let db = |res, hertz| gain_db(FilterType::Ms20LowPass, 1000.0, res, 0.0, hertz, 0.01);
        // Its feedback goes through a high-pass, so unlike the ladder's it leaves the bass alone.
        for res in [0.0, 0.5, 0.9] {
            near(db(res, 50.0), 0.0, 0.1);
        }
        // Two one-poles at the cutoff, each 3 dB down there.
        near(db(0.0, 1000.0), -6.0, 0.2);
        near(db(0.0, 4000.0) - db(0.0, 8000.0), 12.0, 1.0);
    }

    #[test]
    fn the_korg_35_high_pass_keeps_its_top_and_slopes_at_6_db() {
        let db = |res, hertz| gain_db(FilterType::Ms20HighPass, 1000.0, res, 0.0, hertz, 0.01);
        for res in [0.0, 0.5, 0.9] {
            near(db(res, 16_000.0), 0.0, 0.2);
        }
        // The MS-20's high-pass falls at 6 dB an octave under its peak, however high `Res` goes.
        for res in [0.0, 0.9] {
            near(db(res, 100.0) - db(res, 50.0), 6.0, 0.3);
        }
    }

    #[test]
    fn the_korg_35_peaks_at_its_cutoff_and_self_oscillates_there() {
        for kind in [FilterType::Ms20LowPass, FilterType::Ms20HighPass] {
            for cut in [200.0, 1000.0, 4000.0] {
                // With strong resonance, its peak is louder at `Cut` than a semitone either side.
                let db = |hertz| gain_db(kind, cut, 0.9, 0.0, hertz, 0.01);
                let semitone = 2_f32.powf(1.0 / 12.0);
                assert!(db(cut) > db(cut * semitone) && db(cut) > db(cut / semitone));
            }
            for cut in [50.0, 200.0, 1000.0, 4000.0, 8000.0, 12_000.0, 16_000.0] {
                let pitch = ringing_at(kind, cut, 1.0);
                // A semitone is 100 cents, and counting whole cycles in a second is 9 cents out at
                // 200 Hz.
                let cents = 1200.0 * (pitch / cut).log2();
                assert!(cents.abs() < 15.0, "{kind:?} {cut} rang at {pitch}");
            }
        }
    }

    #[test]
    fn the_korg_35_stays_in_bounds_at_full_resonance_with_loud_input_and_fast_sweeps() {
        for kind in [FilterType::Ms20LowPass, FilterType::Ms20HighPass] {
            for drive in [0.0, 1.0] {
                let mut channel = Channel::default();
                let mut most = 0.0_f32;
                for frame in 0..RATE as usize * 2 {
                    // Up and down the whole range eight times a second, with a full-level saw in.
                    let along = frame as f32 / RATE as f32;
                    let sweep = (along * 8.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                    let c = Coefficients::new(sweep, 1.0, RATE);
                    let input = (along * 55.0).fract() * 2.0 - 1.0;
                    let output = channel.process(input, kind, drive, &c);
                    assert!(output.is_finite(), "{kind:?} {output}");
                    most = most.max(output.abs());
                }
                assert!(most < 1.5, "{kind:?} at drive {drive} reached {most}");
            }
        }
    }

    #[test]
    fn moving_a_sallen_keys_settings_in_steps_glides_without_a_step() {
        // The biggest jump between frames from a tone through it, with the settings smoothed and its
        // coefficients worked out every 32 frames, as a track does. They're held at one of two, or
        // jump between them every 64 frames, as a knob or an LFO steps them block by block.
        let biggest_jump = |kind: FilterType, at: fn(usize) -> bool| {
            let smoothing = crate::dsp::smoother::coefficient(RATE);
            let (mut cut, mut res, mut drive) = (
                crate::dsp::smoother::Smoothed::at(cutoff(500.0)),
                crate::dsp::smoother::Smoothed::at(0.3),
                crate::dsp::smoother::Smoothed::at(0.0),
            );
            let mut channel = Channel::default();
            let mut c = Coefficients::new(cut.value, res.value, RATE);
            let (mut last, mut most) = (0.0_f32, 0.0_f32);
            for frame in 0..RATE as usize * 2 {
                if at(frame) {
                    (cut.target, res.target, drive.target) = (cutoff(2000.0), 0.9, 0.6);
                } else {
                    (cut.target, res.target, drive.target) = (cutoff(500.0), 0.3, 0.0);
                }
                let (cut, res, drive) = (
                    cut.next(smoothing),
                    res.next(smoothing),
                    drive.next(smoothing),
                );
                if frame % 32 == 0 {
                    c = Coefficients::new(cut, res, RATE);
                }
                let tone = (frame as f32 / RATE as f32 * 700.0 * std::f32::consts::TAU).sin();
                let output = channel.process(tone * 0.5, kind, drive, &c);
                if frame > RATE as usize / 2 {
                    most = most.max((output - last).abs());
                }
                last = output;
            }
            most
        };
        for kind in [
            FilterType::Ms20LowPass,
            FilterType::Ms20HighPass,
            FilterType::Dirty,
        ] {
            let held = biggest_jump(kind, |_| false).max(biggest_jump(kind, |_| true));
            let moving = biggest_jump(kind, |frame| frame / 64 % 2 == 1);
            assert!(
                moving < held * 1.5,
                "{kind:?}: {held} held and {moving} moving"
            );
        }
    }

    #[test]
    fn the_dirty_filter_slopes_at_24_db_and_loses_bass_as_resonance_rises() {
        let db = |res, hertz| gain_db(FilterType::Dirty, 1000.0, res, 0.0, hertz, 0.01);
        // A quiet sound comes through at its level with no resonance.
        near(db(0.0, 50.0), 0.0, 0.1);
        // Four one-poles at the cutoff, each 3 dB down there.
        near(db(0.0, 1000.0), -12.0, 0.6);
        near(db(0.0, 4000.0) - db(0.0, 8000.0), 24.0, 2.0);
        // Its loop takes from what comes straight in as it rises, as the ladder's does.
        assert!(db(0.5, 50.0) < -4.0, "{}", db(0.5, 50.0));
        assert!(db(0.75, 50.0) < db(0.5, 50.0) - 1.0, "{}", db(0.75, 50.0));
    }

    #[test]
    fn the_dirty_filter_peaks_at_its_cutoff_and_self_oscillates_there() {
        for cut in [200.0, 1000.0, 4000.0] {
            // Just short of ringing by itself, its peak is louder at `Cut` than a semitone either
            // side.
            let db = |hertz| gain_db(FilterType::Dirty, cut, 0.85, 0.0, hertz, 0.001);
            let semitone = 2_f32.powf(1.0 / 12.0);
            assert!(db(cut) > db(cut * semitone) && db(cut) > db(cut / semitone));
        }
        // It rings by itself from about `Res DB`, and drive, which adds resonance, rings it louder
        // and so flatter, until the loop's tuning is raised to meet it.
        for drive in [0.0, 1.0] {
            for res in [0.9, 1.0] {
                for cut in [50.0, 200.0, 1000.0, 4000.0, 8000.0, 12_000.0, 16_000.0] {
                    if res < 1.0 && cut > 4000.0 {
                        // It's eased off at high cutoffs, so it needs more there.
                        continue;
                    }
                    let pitch = driven_ringing_at(FilterType::Dirty, cut, res, drive);
                    let cents = 1200.0 * (pitch / cut).log2();
                    assert!(cents.abs() < 15.0, "{drive} {res} {cut} rang at {pitch}");
                }
            }
        }
    }

    #[test]
    fn the_dirty_filters_curve_shows_drives_resonance() {
        // Drive adds resonance, which changes its shape, so its curve takes that, though not the
        // level drive leaves it at.
        let (cut, res) = (1000.0, 0.75);
        for drive in [0.0, 1.0] {
            let worked_out =
                |hertz| response_db(FilterType::Dirty, cutoff(cut), res, drive, RATE, hertz);
            let measured = |hertz| gain_db(FilterType::Dirty, cut, res, drive, hertz, 0.0005);
            for hertz in [400.0, 1000.0, 2500.0] {
                near(
                    measured(hertz) - measured(100.0),
                    worked_out(hertz) - worked_out(100.0),
                    0.5,
                );
            }
        }
        let peak = |drive| response_db(FilterType::Dirty, cutoff(cut), res, drive, RATE, cut);
        assert!(
            peak(1.0) > peak(0.0) + 3.0,
            "{} and {}",
            peak(1.0),
            peak(0.0)
        );
    }

    #[test]
    fn the_dirty_filter_stays_in_bounds_at_full_resonance_with_loud_input_and_fast_sweeps() {
        for drive in [0.0, 1.0] {
            let mut channel = Channel::default();
            let mut most = 0.0_f32;
            for frame in 0..RATE as usize * 2 {
                // Up and down the whole range eight times a second, with a full-level saw in.
                let along = frame as f32 / RATE as f32;
                let sweep = (along * 8.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                let c = Coefficients::new(sweep, 1.0, RATE);
                let input = (along * 55.0).fract() * 2.0 - 1.0;
                let output = channel.process(input, FilterType::Dirty, drive, &c);
                assert!(output.is_finite(), "{output}");
                most = most.max(output.abs());
            }
            assert!(most < 0.8, "at drive {drive} it reached {most}");
        }
    }
}
