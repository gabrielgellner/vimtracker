//! Each track's filter. The clean types share a TPT state-variable filter, which stays stable near
//! the top of the range. The ladder, diode, Korg 35 and Vital's `Dirty` are the character filters,
//! and cost more.
//!
//! A filter's mode is a mix of what it already has: a second state-variable filter after the
//! first, the ladder's and `Dirty`'s stages weighted as Vital weights them, or where the diode's
//! input high-pass sits. Each channel moves its mix toward the mode's over the smoothing time, so
//! changing mode doesn't click.

use vt_model::song::{FilterMode, FilterType, Track};

use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::smoother;
use crate::dsp::filters::diode_filter::{
    self, Complex, DiodeFilter, HIGH_PASS_RATIO, RESONANT_SHARE,
};
use crate::dsp::filters::dirty_filter::{self, DirtyFilter};
use crate::dsp::filters::korg35::{self, Korg35};

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
/// each other a sample late, which pulls its resonance flat as the cutoff rises, and much more so
/// near the top, which the second term follows: `1 + 2.1 s + 100 s^5`, fitted to where it rings at
/// full resonance.
const DIODE_HIGH_TUNING: f32 = 2.1;
const DIODE_TOP_TUNING: f32 = 100.0;
/// The highest its feedback high-pass's corner goes, as a share of the cutoff. Vital's sits at
/// 20 Hz, which near the bottom of the range takes so much of the loop's gain that it wouldn't ring
/// on its own under about 100 Hz (#333). So under 400 Hz the corner moves down with the cutoff,
/// which keeps the loop the same shape and its ringing as quick to start, in cycles, all the way
/// down.
const DIODE_FEEDBACK_SHARE: f32 = 0.05;
/// How far its feedback high-pass pulls its resonance sharp, which the cutoff is set that much
/// lower to undo: by `1 + 10.4 x^2`, for the corner `x` as a share of the cutoff, fitted to where
/// it rings at full resonance.
const DIODE_LOW_TUNING: f32 = 10.4;
/// What the rational `tanh` is good for.
const TANH_RANGE: f32 = 4.0;
/// The damping of the second filter a clean low-pass or high-pass goes through for 24 dB, which has
/// no resonance of its own: `1 / Q` for a Q of 0.707.
const FLAT_DAMPING: f32 = std::f32::consts::SQRT_2;

/// How the ladder's mixes weigh what goes into its first stage and what comes out of each of its
/// four, after Vital's `LadderFilter`. Each stage is a one-pole low-pass `L`, so `HP24` is
/// `(1 - L)^4`, and `BP` is `-L^2 (1 - L)^2`, turned over so its peak is in step with the others'.
const LADDER_LOW_24: [f32; 5] = [0.0, 0.0, 0.0, 0.0, 1.0];
const LADDER_LOW_12: [f32; 5] = [0.0, 0.0, 1.0, 0.0, 0.0];
const LADDER_BAND: [f32; 5] = [0.0, 0.0, -1.0, 2.0, -1.0];
const LADDER_HIGH_12: [f32; 5] = [1.0, -2.0, 1.0, 0.0, 0.0];
const LADDER_HIGH_24: [f32; 5] = [1.0, -4.0, 6.0, -4.0, 1.0];
/// `((1 - L)^2 + L^2)^2`, which is nothing at the cutoff: Vital's notch with its blend in the middle.
const LADDER_NOTCH: [f32; 5] = [1.0, -4.0, 8.0, -8.0, 4.0];
/// How far a band-pass from four poles is turned up at most: with no resonance, a quarter is all it
/// passes at its peak, as each pole passes 0.71 there.
const BAND_MAKEUP: f32 = 4.0;

/// How many points each family's `Res` taper is given at, a twentieth of the knob's turn apart.
const TAPER_POINTS: usize = 21;
/// How far each family's resonance is turned at each twentieth of `Res`, as a share of its most, so
/// each equal turn raises the peak by about as much as Cytomic's The Drop's knob does (#333). Each
/// was fitted with `tools/compare`'s `filter-taper-*` specs, where the first 85% of the knob, to
/// `D9`, is set against The Drop's 0 to 100%: by how far the peak stands over the passband, which
/// rises evenly from Res 0 and then steeply over the last few percent before it rings. The
/// saturating types ring by themselves from 85%, as The Drop's circuits do over 100%, and the last
/// 15% takes them on to their most, which is where they were before, so they ring in tune at full
/// `Res` as they did. The clean types never ring, and climb to their highest peak over the last 20%.
///
/// The clean state-variable types against The Drop's OSR, an OTA state-variable filter.
const CLEAN_TAPER: [f32; TAPER_POINTS] = [
    0.0, 0.021, 0.037, 0.051, 0.072, 0.096, 0.127, 0.161, 0.201, 0.243, 0.287, 0.331, 0.374, 0.413,
    0.45, 0.486, 0.52, 0.64, 0.76, 0.88, 1.0,
];
/// The ladder against The Drop's PRD, a transistor ladder like the Prodigy's.
const LADDER_TAPER: [f32; TAPER_POINTS] = [
    0.0, 0.055, 0.111, 0.166, 0.219, 0.271, 0.322, 0.372, 0.418, 0.462, 0.502, 0.536, 0.567, 0.593,
    0.615, 0.634, 0.651, 0.925, 0.95, 0.975, 1.0,
];
/// The Korg 35s against The Drop's MS1, the MS-20's first Korg 35. Its peak barely stands out
/// under half its feedback, so the taper starts quickly.
const KORG_TAPER: [f32; TAPER_POINTS] = [
    0.0, 0.213, 0.35, 0.428, 0.478, 0.522, 0.573, 0.624, 0.671, 0.713, 0.751, 0.785, 0.815, 0.842,
    0.866, 0.887, 0.905, 0.926, 0.955, 0.977, 1.0,
];
/// The diode ladder and `Dirty`, which The Drop has nothing like, against its PRD too, the peak
/// over the passband scaled to start from their own at Res 0 and reach where PRD's does just
/// before it rings. The diode's feedback is on a cube, so its taper starts quickly too.
const DIODE_TAPER: [f32; TAPER_POINTS] = [
    0.0, 0.274, 0.343, 0.384, 0.418, 0.455, 0.494, 0.54, 0.596, 0.643, 0.681, 0.712, 0.737, 0.757,
    0.773, 0.787, 0.798, 0.885, 0.975, 0.988, 1.0,
];
const DIRTY_TAPER: [f32; TAPER_POINTS] = [
    0.0, 0.128, 0.379, 0.463, 0.512, 0.556, 0.594, 0.628, 0.657, 0.682, 0.703, 0.72, 0.734, 0.746,
    0.756, 0.763, 0.77, 0.847, 0.909, 0.955, 1.0,
];

/// A family's resonance, as a share of its most, for `Res` from 0 to 1, read between its taper's
/// points.
#[inline]
fn tapered(taper: &[f32; TAPER_POINTS], resonance: f32) -> f32 {
    let along = resonance.clamp(0.0, 1.0) * (TAPER_POINTS - 1) as f32;
    let index = (along as usize).min(TAPER_POINTS - 2);
    let within = along - index as f32;
    taper[index] + (taper[index + 1] - taper[index]) * within
}

/// How `Dirty`'s mixes weigh each of its two-poles' input and its two one-poles, `L` and `L^2`, the
/// way Vital takes low-pass, band-pass and high-pass from them. Its 12 dB modes pass straight
/// through the two-pole before its loop.
const DIRTY_THROUGH: [f32; 3] = [1.0, 0.0, 0.0];
const DIRTY_LOW: [f32; 3] = [0.0, 0.0, 1.0];
const DIRTY_BAND: [f32; 3] = [0.0, 1.0, -1.0];
const DIRTY_HIGH: [f32; 3] = [1.0, -2.0, 1.0];
const DIRTY_NOTCH: [f32; 3] = [1.0, -2.0, 2.0];

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
    /// The diode ladder's `Res` after its taper, which it puts on a cube itself.
    diode_resonance: f32,
    /// The Korg 35's one-pole coefficient at twice the rate, `g / (1 + g)`, and its feedback.
    korg_coefficient: f32,
    korg_feedback: f32,
    /// The `Dirty` filter's tuning at twice the rate.
    dirty: dirty_filter::Tuning,
    /// The second clean filter's `a1` for a low-pass or high-pass at 24 dB, at the rate and twice
    /// it, with no resonance of its own, so the peak stays as it is. A band-pass or notch has the
    /// first's, as their peak and dip don't grow with resonance.
    flat_a1: f32,
    double_flat_a1: f32,
    /// The mix for the mode, which each channel moves toward, and how fast.
    mix: Mix,
    smoothing: f32,
}

impl Coefficients {
    /// Work out coefficients, for each type's first mode.
    pub fn new(cutoff: f32, resonance: f32, rate: u32) -> Self {
        Self::with_mode(cutoff, resonance, FilterMode::default(), rate)
    }

    /// Work out coefficients for a mode. A mode a family doesn't have leaves that family in its
    /// first, so this is the same for any type that plays it.
    pub fn with_mode(cutoff: f32, resonance: f32, mode: FilterMode, rate: u32) -> Self {
        let frequency = 20.0 * 1000_f32.powf(cutoff);
        let nyquist_safe = frequency.min(rate as f32 * 0.45);
        let g = (std::f32::consts::PI * nyquist_safe / rate as f32).tan();
        // Q runs from a flat response to a strong but stable peak.
        let q = 0.707 + tapered(&CLEAN_TAPER, resonance) * 9.3;
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let double_g = (std::f32::consts::PI * nyquist_safe / (2.0 * rate as f32)).tan();
        let ladder_k = tapered(&LADDER_TAPER, resonance) * LADDER_FEEDBACK;
        let diode_corner = (frequency * DIODE_FEEDBACK_SHARE).min(diode_filter::FEEDBACK_HIGH_PASS);
        let dirty_resonance = tapered(&DIRTY_TAPER, resonance);
        let dirty = dirty_filter::Tuning::new(double_g / (1.0 + double_g), dirty_resonance);
        Self {
            g,
            k,
            a1,
            double_g,
            double_a1: 1.0 / (1.0 + double_g * (double_g + k)),
            ladder_k,
            diode_cutoff: {
                let share = frequency / RESONANT_SHARE / (2.0 * rate as f32);
                let high = 1.0 + DIODE_HIGH_TUNING * share + DIODE_TOP_TUNING * share.powi(5);
                let low = 1.0 + DIODE_LOW_TUNING * (diode_corner / frequency).powi(2);
                share * high / low
            },
            diode_feedback: diode_filter::feedback_coefficient_at(diode_corner, 2.0 * rate as f32),
            diode_resonance: tapered(&DIODE_TAPER, resonance),
            korg_coefficient: double_g / (1.0 + double_g),
            korg_feedback: tapered(&KORG_TAPER, resonance) * korg35::MOST_FEEDBACK,
            dirty,
            flat_a1: 1.0 / (1.0 + g * (g + FLAT_DAMPING)),
            double_flat_a1: 1.0 / (1.0 + double_g * (double_g + FLAT_DAMPING)),
            mix: Mix::new(mode, ladder_k, &dirty),
            smoothing: smoother::coefficient(rate),
        }
    }

    /// The second clean filter's damping and `a1`, at the rate or twice it.
    #[inline]
    fn second(&self, kind: FilterType, double: bool) -> (f32, f32) {
        let flat = matches!(kind, FilterType::LowPass | FilterType::HighPass);
        match (flat, double) {
            (true, false) => (FLAT_DAMPING, self.flat_a1),
            (true, true) => (FLAT_DAMPING, self.double_flat_a1),
            (false, false) => (self.k, self.a1),
            (false, true) => (self.k, self.double_a1),
        }
    }
}

/// How a filter's outputs are mixed for its mode. Every family's weights are kept, as a track's
/// filter starts afresh when its type changes, and only its own are read.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mix {
    /// The ladder's: what goes into its first stage, then each stage.
    ladder: [f32; 5],
    /// `Dirty`'s two-poles', before its loop and where it goes in.
    before: [f32; 3],
    entry: [f32; 3],
    /// How much of a clean filter goes on through the second, from 12 dB at 0 to 24 dB at 1.
    series: f32,
    /// Where the diode's input high-pass sits, as a share of its cutoff.
    low_cut: f32,
}

impl Mix {
    /// The mix for a mode, with the ladder's feedback and `Dirty`'s tuning, which their
    /// band-passes' levels follow.
    fn new(mode: FilterMode, ladder_k: f32, dirty: &dirty_filter::Tuning) -> Self {
        let (ladder, before, entry) = match mode {
            FilterMode::LowPass12 => (LADDER_LOW_12, DIRTY_THROUGH, DIRTY_LOW),
            FilterMode::BandPass => {
                // A band-pass peaks as loud as the low-pass is at its loudest: its bass with
                // little resonance, and its peak with more. At the cutoff they pass the same, so
                // it's turned up by the bass over that, which resonance brings down to 1. The
                // ladder's loop is `1 + k L^4` there, which is `1 - k / 4`.
                let ladder = ((4.0 - ladder_k).abs() / (1.0 + ladder_k)).clamp(1.0, BAND_MAKEUP);
                let dirty = dirty_filter::band_makeup(dirty).clamp(1.0, BAND_MAKEUP);
                (
                    LADDER_BAND.map(|weight| weight * ladder),
                    DIRTY_BAND.map(|weight| weight * dirty),
                    DIRTY_BAND,
                )
            }
            FilterMode::HighPass12 => (LADDER_HIGH_12, DIRTY_THROUGH, DIRTY_HIGH),
            FilterMode::HighPass24 => (LADDER_HIGH_24, DIRTY_HIGH, DIRTY_HIGH),
            FilterMode::Notch => (LADDER_NOTCH, DIRTY_NOTCH, DIRTY_NOTCH),
            _ => (LADDER_LOW_24, DIRTY_LOW, DIRTY_LOW),
        };
        let low_cut = match mode {
            FilterMode::LowCut3 => RESONANT_SHARE / 8.0,
            FilterMode::LowCut2 => RESONANT_SHARE / 4.0,
            FilterMode::LowCut1 => RESONANT_SHARE / 2.0,
            _ => HIGH_PASS_RATIO,
        };
        Self {
            ladder,
            before,
            entry,
            series: if mode == FilterMode::Slope24 {
                1.0
            } else {
                0.0
            },
            low_cut,
        }
    }

    /// Move one frame toward another mix, and onto it once it's close enough not to hear.
    #[inline]
    fn approach(&mut self, target: &Self, coefficient: f32) {
        let mut furthest = 0.0_f32;
        let mut step = |value: &mut f32, to: f32| {
            *value += (to - *value) * coefficient;
            furthest = furthest.max((to - *value).abs());
        };
        for (value, to) in self.ladder.iter_mut().zip(target.ladder) {
            step(value, to);
        }
        for (value, to) in self.before.iter_mut().zip(target.before) {
            step(value, to);
        }
        for (value, to) in self.entry.iter_mut().zip(target.entry) {
            step(value, to);
        }
        step(&mut self.series, target.series);
        step(&mut self.low_cut, target.low_cut);
        if furthest < 1e-6 {
            *self = *target;
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
    /// Its output is its stages mixed by `weights`: what goes into the first, then each one's.
    #[inline]
    fn step(&mut self, input: f32, c: &Coefficients, weights: &[f32; 5]) -> f32 {
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
        // What goes into the first stage is the input's slope times what's solved for, as the
        // first stage hears it.
        let mixed = weights[0] * input_slope * u
            + weights[1] * self.last[0]
            + weights[2] * self.last[1]
            + weights[3] * self.last[2]
            + weights[4] * self.last[3];
        // Soft-clipped to 2, which is gentle at normal levels.
        mixed * tanh_over_x((mixed * 0.5).clamp(-TANH_RANGE, TANH_RANGE))
    }
}

/// One channel of a track's filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Channel {
    svf: Svf,
    /// The second clean filter, for 24 dB. It always runs on the clean types, so a change to 24 dB
    /// finds it settled.
    second: Svf,
    ladder: Ladder,
    diode: DiodeFilter,
    korg: Korg35,
    dirty: DirtyFilter,
    oversampler: Oversampler,
    /// The mode's mix it's playing, and whether it's moving toward the coefficients' yet, which a
    /// new channel doesn't, as it starts on theirs.
    mix: Mix,
    mixing: bool,
}

impl Channel {
    /// Filter one sample. `drive` runs from 0, which takes the clean path, to 1.
    #[inline]
    pub fn process(&mut self, input: f32, kind: FilterType, drive: f32, c: &Coefficients) -> f32 {
        if !self.mixing {
            self.mix = c.mix;
            self.mixing = true;
        } else if self.mix != c.mix {
            self.mix.approach(&c.mix, c.smoothing);
        }
        if drive <= 0.0 && !kind.saturates() {
            let (band, low) = self.svf.tick(input, c);
            let first = svf_output(kind, input, band, low, c.k);
            let (k, a1) = c.second(kind, false);
            let (band, low) = self.second.tick_tuned(first, c.g, a1);
            let second = svf_output(kind, first, band, low, k);
            return first + self.mix.series * (second - first);
        }
        self.saturating(input, kind, drive, c)
    }

    /// Start on the coefficients' mix next time, rather than moving to it, for a new sound.
    pub fn settle(&mut self) {
        self.mixing = false;
    }

    /// What saturates, at twice the rate. It's kept apart so the clean path stays small enough to
    /// inline.
    #[inline(never)]
    fn saturating(&mut self, input: f32, kind: FilterType, drive: f32, c: &Coefficients) -> f32 {
        let gain = 1.0 + drive * DRIVE_GAIN;
        let mix = self.mix;
        if kind == FilterType::Diode {
            // The diode ladder takes the drive itself, and evens out its level.
            let diode = &mut self.diode;
            return self.oversampler.process(input * DIODE_INPUT, |x| {
                let (cutoff, feedback) = (c.diode_cutoff, c.diode_feedback);
                diode.next(x, cutoff, c.diode_resonance, gain, feedback, mix.low_cut) * DIODE_OUTPUT
            });
        }
        if kind == FilterType::Dirty {
            // Its drive goes only to what comes straight into its loop, and adds a little
            // resonance.
            let settings = dirty_filter::Settings::new(&c.dirty, drive, gain);
            let dirty = &mut self.dirty;
            return self
                .oversampler
                .process(input, |x| dirty.next(x, &settings, &mix.before, &mix.entry))
                / gain.sqrt();
        }
        let output = if kind == FilterType::Ladder {
            let ladder = &mut self.ladder;
            self.oversampler
                .process(input * gain, |x| ladder.step(x, c, &mix.ladder))
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
            let (svf, second) = (&mut self.svf, &mut self.second);
            let (k, a1) = c.second(kind, true);
            self.oversampler.process(input * gain, |x| {
                let (band, low) = svf.tick_driven(x, c);
                let first = svf_output(kind, x, band, low, c.k);
                let (band, low) = second.tick_tuned(first, c.double_g, a1);
                let after = svf_output(kind, first, band, low, k);
                soft_clip(first + mix.series * (after - first))
            })
        };
        output / gain.sqrt()
    }
}

/// How much a filter lets through at a frequency, in dB, for a quiet signal, in the mode it plays
/// for `mode`. This is the filter's exact response, so a drawing of it shows what's heard.
///
/// `Drive` only changes the level and the saturation of the others, but it adds resonance to
/// `Dirty`, which changes its shape, so that's drawn. Its level isn't, as for the others.
pub fn response_db(
    kind: FilterType,
    mode: FilterMode,
    cutoff: f32,
    resonance: f32,
    drive: f32,
    rate: u32,
    hertz: f32,
) -> f32 {
    let c = Coefficients::with_mode(cutoff, resonance, kind.plays(mode), rate);
    let mix = &c.mix;
    let hertz = hertz.min(rate as f32 * 0.499);
    let power = if kind == FilterType::Dirty {
        let settings = dirty_filter::Settings::new(&c.dirty, drive, 1.0);
        let at = hertz / (2.0 * rate as f32);
        dirty_filter::response(&settings, &mix.before, &mix.entry, at)
    } else if kind == FilterType::Diode {
        let gain = DIODE_INPUT * DIODE_OUTPUT;
        gain * gain
            * diode_filter::response(
                c.diode_cutoff,
                c.diode_resonance,
                c.diode_feedback,
                mix.low_cut,
                hertz / (2.0 * rate as f32),
            )
    } else if matches!(kind, FilterType::Ms20LowPass | FilterType::Ms20HighPass) {
        let along = (std::f32::consts::PI * hertz / (2.0 * rate as f32)).tan() / c.double_g;
        korg35::response(kind == FilterType::Ms20HighPass, c.korg_feedback, along)
    } else if kind == FilterType::Ladder {
        // Four one-poles in a loop, `L = 1 / (1 + jw)`, each stage `L^i / (1 + k L^4)`, which is
        // `(1 + jw)^(4 - i) / ((1 + jw)^4 + k)`.
        let w = (std::f32::consts::PI * hertz / (2.0 * rate as f32)).tan() / c.double_g;
        let pole = Complex(1.0, w);
        let mixed = mix.ladder.iter().fold(Complex(0.0, 0.0), |mixed, weight| {
            mixed.times(pole).add(Complex(*weight, 0.0))
        });
        let squared = pole.times(pole);
        let loop_gain = squared.times(squared).add(Complex(c.ladder_k, 0.0));
        mixed.over(loop_gain).power()
    } else {
        let w = (std::f32::consts::PI * hertz / rate as f32).tan() / c.g;
        let single = |k: f32| {
            let (w2, k2) = (w * w, k * k);
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
        let first = single(c.k);
        let (k, _) = c.second(kind, false);
        // The second filter's mix is all or nothing once settled, so only its power is needed.
        first * (1.0 + mix.series * (single(k) - 1.0))
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
            track.filter_mode,
            cutoff,
            resonance,
            value(track.drive),
            rate,
            hertz,
        );
    }
    let flat = FilterMode::Slope12;
    if track.low_cut > 0 {
        let cut = value(track.low_cut);
        db += response_db(FilterType::HighPass, flat, cut, 0.0, 0.0, rate, hertz);
    }
    if track.high_cut < u8::MAX {
        let cut = value(track.high_cut);
        db += response_db(FilterType::LowPass, flat, cut, 0.0, 0.0, rate, hertz);
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

/// The output a type takes from a state-variable filter with damping `k`.
#[inline]
fn svf_output(kind: FilterType, input: f32, band: f32, low: f32, k: f32) -> f32 {
    match kind {
        FilterType::HighPass => input - k * band - low,
        FilterType::BandPass => k * band,
        FilterType::Notch => input - k * band,
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
        mode_gain_db(kind, FilterMode::default(), cut, res, hertz, drive, level)
    }

    /// The same in a mode.
    fn mode_gain_db(
        kind: FilterType,
        mode: FilterMode,
        cut: f32,
        res: f32,
        hertz: f32,
        drive: f32,
        level: f32,
    ) -> f32 {
        let c = Coefficients::with_mode(cutoff(cut), res, kind.plays(mode), RATE);
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

    /// Each type with each mode it offers, or its one way for a type with none.
    fn every_type_and_mode() -> Vec<(FilterType, FilterMode)> {
        FilterType::ALL
            .into_iter()
            .flat_map(|kind| {
                let modes = kind.modes();
                let modes = if modes.is_empty() {
                    vec![FilterMode::default()]
                } else {
                    modes.to_vec()
                };
                modes.into_iter().map(move |mode| (kind, mode))
            })
            .collect()
    }

    #[test]
    fn the_worked_out_response_matches_the_filter_itself() {
        for (kind, mode) in every_type_and_mode() {
            // Short of the last climb to ringing, where a quiet sound saturates at the peak.
            for res in [0.0, 0.5, 0.75] {
                for hertz in [100.0, 400.0, 1000.0, 2500.0, 8000.0] {
                    let worked_out = response_db(kind, mode, cutoff(1000.0), res, 0.0, RATE, hertz);
                    // A notch at its very centre is too deep to measure, and so is what's far
                    // down a steep slope.
                    if worked_out < -60.0 {
                        continue;
                    }
                    // `Dirty` and the Korg 35s saturate even a quiet signal a little, where it's
                    // loudest.
                    let level = if matches!(
                        kind,
                        FilterType::Dirty | FilterType::Ms20LowPass | FilterType::Ms20HighPass
                    ) {
                        0.001
                    } else {
                        0.01
                    };
                    let measured = mode_gain_db(kind, mode, 1000.0, res, hertz, 0.0, level);
                    // The diode's stages hear each other a sample late, which the analog model
                    // doesn't, so it's a little out at its peak.
                    let within = if kind == FilterType::Diode { 1.0 } else { 0.5 };
                    assert!(
                        (worked_out - measured).abs() < within,
                        "{kind:?} {mode:?} res {res} at {hertz}: {worked_out} and {measured}"
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

    /// How far a filter's peak, the loudest it is within an octave of `Cut` at 1 kHz, stands over
    /// its passband three octaves away, from its worked-out response, which is what a quiet sound
    /// hears and the Sound page draws.
    fn peak_over_passband(kind: FilterType, res: f32) -> f32 {
        let mode = kind.plays(FilterMode::default());
        let db = |hertz| response_db(kind, mode, cutoff(1000.0), res, 0.0, RATE, hertz);
        let peak = (0..=96)
            .map(|step| db(500.0 * 2_f32.powf(step as f32 / 48.0)))
            .fold(f32::MIN, f32::max);
        let high = matches!(kind, FilterType::HighPass | FilterType::Ms20HighPass);
        peak - if high { db(8000.0) } else { db(125.0) }
    }

    #[test]
    fn each_quarter_of_res_raises_the_peak_as_the_drops_knob_does() {
        // How far the peak has risen over its height at Res 0, at a quarter, half and three
        // quarters of the knob (#333), with The Drop's paired circuit's rise beside it, averaged
        // over 200 Hz, 988 Hz and 5 kHz in `tools/compare`'s `filter-taper-*` specs. The old
        // taper's clean low-pass had risen 9.9, 14.8 and 18.0 dB. The diode and `Dirty` follow
        // PRD's, scaled to reach where it does just before it rings. The Korg 35s' three quarters
        // is their linear response, which a quiet sweep hears 2 to 4 dB lower as it saturates, as
        // MS1 does.
        let expected = [
            // OSR's low-pass, 4.9, 11.0 and 14.8, and high-pass, 4.5, 10.6 and 14.2.
            (FilterType::LowPass, [4.7, 10.8, 14.5]),
            (FilterType::HighPass, [4.7, 10.8, 14.5]),
            // PRD's, 6.6, 12.3 and 16.4.
            (FilterType::Ladder, [6.6, 12.3, 16.5]),
            // MS1's low-pass, 4.6, 11.4 and 20.3, and high-pass, 4.0, 10.3 and 19.4.
            (FilterType::Ms20LowPass, [3.8, 10.1, 22.3]),
            (FilterType::Ms20HighPass, [5.1, 12.0, 24.4]),
            // PRD's scaled, 9.5, 17.7 and 23.6, and 6.5, 12.1 and 16.1.
            (FilterType::Diode, [9.9, 18.0, 23.9]),
            (FilterType::Dirty, [6.7, 12.7, 17.3]),
        ];
        for (kind, rises) in expected {
            let bottom = peak_over_passband(kind, 0.0);
            for (res, rise) in [0.25, 0.5, 0.75].into_iter().zip(rises) {
                let found = peak_over_passband(kind, res) - bottom;
                assert!(
                    (found - rise).abs() < 0.5,
                    "{kind:?} at Res {res} rose {found} dB, not {rise}"
                );
            }
        }
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
    fn the_diode_self_oscillates_at_its_cutoff_across_the_range() {
        // At full resonance, one quiet click and then silence, at every sixteenth of Cut's turn up
        // to 13 kHz. Under 100 Hz it used to die away, and elsewhere it rang 34 cents flat to 42
        // sharp (#333).
        for rate in [44_100, 48_000] {
            for knob in (0x00..=0xF0).step_by(0x10) {
                let value = knob as f32 / 255.0;
                let hertz = 20.0 * 1000_f32.powf(value);
                let c = Coefficients::new(value, 1.0, rate);
                let mut channel = Channel::default();
                channel.process(0.01, FilterType::Diode, 0.0, &c);
                let frames = rate as usize * 3;
                let out: Vec<f32> = (0..frames)
                    .map(|_| channel.process(0.0, FilterType::Diode, 0.0, &c))
                    .collect();
                // It's ringing steadily over the last second, at a sure level.
                let last = &out[rate as usize * 2..];
                let peak = last
                    .iter()
                    .fold(0.0_f32, |high, sample| high.max(sample.abs()));
                assert!(peak > 0.05, "{hertz} Hz at {rate}: {peak}");
                // Its pitch, from where it crosses zero rising, found between frames.
                let rising: Vec<f32> = last
                    .windows(2)
                    .enumerate()
                    .filter(|(_, pair)| pair[0] <= 0.0 && pair[1] > 0.0)
                    .map(|(frame, pair)| frame as f32 + pair[0] / (pair[0] - pair[1]))
                    .collect();
                let span = rising[rising.len() - 1] - rising[0];
                let pitch = (rising.len() - 1) as f32 * rate as f32 / span;
                let cents = 1200.0 * (pitch / hertz).log2();
                assert!(cents.abs() < 15.0, "{hertz} Hz at {rate} rang at {pitch}");
            }
        }
    }

    #[test]
    fn every_type_survives_the_extremes_and_fast_changes() {
        for (kind, mode) in every_type_and_mode() {
            for drive in [0.0, 1.0] {
                let mut channel = Channel::default();
                for frame in 0..RATE as usize {
                    // The cutoff and resonance sweep fast, and the input is loud.
                    let sweep = (frame as f32 / 37.0).sin() * 0.5 + 0.5;
                    let c = Coefficients::with_mode(sweep, 1.0 - sweep, mode, RATE);
                    let input = if frame % 40 < 20 { 1.0 } else { -1.0 };
                    let output = channel.process(input, kind, drive, &c);
                    assert!(
                        output.is_finite() && output.abs() < 20.0,
                        "{kind:?} {mode:?} {output}"
                    );
                }
            }
        }
    }

    /// How loud a frequency is in a filter's output, from a sine at another.
    fn level_at(kind: FilterType, mode: FilterMode, drive: f32, tone: f32, listen: f32) -> f32 {
        let c = Coefficients::with_mode(1.0, 0.0, mode, RATE);
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
        let db = |kind, mode, alias| {
            let level = |listen| level_at(kind, mode, 1.0, 5000.0, listen);
            20.0 * (level(alias) / level(5000.0)).log10()
        };
        let kinds = [
            FilterType::Ladder,
            FilterType::LowPass,
            FilterType::Diode,
            FilterType::Ms20LowPass,
            FilterType::Ms20HighPass,
            FilterType::Dirty,
        ];
        // The modes that pass a 5 kHz sine with `Cut` at the top mix the same stages, at 2x too.
        let modes = [
            FilterMode::Slope12,
            FilterMode::Slope24,
            FilterMode::LowPass24,
            FilterMode::LowPass12,
            FilterMode::Notch,
            FilterMode::Full,
            FilterMode::LowCut1,
        ];
        for kind in kinds {
            // Each type's first, and the others it offers.
            let first = kind.plays(FilterMode::default());
            let offered = |mode: &FilterMode| kind.plays(*mode) == *mode || *mode == first;
            for mode in modes.into_iter().filter(offered) {
                for (alias, most) in [(3000.0, -50.0), (13_000.0, -50.0), (23_000.0, -30.0)] {
                    let found = db(kind, mode, alias);
                    assert!(found < most, "{kind:?} {mode:?} {alias} is at {found} dB");
                }
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
        for res in [0.0, 0.5, 0.8] {
            near(db(res, 50.0), 0.0, 0.1);
        }
        // Two one-poles at the cutoff, each 3 dB down there.
        near(db(0.0, 1000.0), -6.0, 0.2);
        near(db(0.0, 4000.0) - db(0.0, 8000.0), 12.0, 1.0);
    }

    #[test]
    fn the_korg_35_high_pass_keeps_its_top_and_slopes_at_6_db() {
        let db = |res, hertz| gain_db(FilterType::Ms20HighPass, 1000.0, res, 0.0, hertz, 0.01);
        for res in [0.0, 0.5, 0.8] {
            near(db(res, 16_000.0), 0.0, 0.2);
        }
        // The MS-20's high-pass falls at 6 dB an octave under its peak, however high `Res` goes.
        for res in [0.0, 0.8] {
            near(db(res, 100.0) - db(res, 50.0), 6.0, 0.3);
        }
    }

    #[test]
    fn the_korg_35_peaks_at_its_cutoff_and_self_oscillates_there() {
        for kind in [FilterType::Ms20LowPass, FilterType::Ms20HighPass] {
            for cut in [200.0, 1000.0, 4000.0] {
                // With strong resonance, its peak is louder at `Cut` than a semitone either side.
                let db = |hertz| gain_db(kind, cut, 0.8, 0.0, hertz, 0.01);
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
            let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
            let (mut cut, mut res, mut drive) = (
                crate::dsp::blocks::smoother::Smoothed::at(cutoff(500.0)),
                crate::dsp::blocks::smoother::Smoothed::at(0.1),
                crate::dsp::blocks::smoother::Smoothed::at(0.0),
            );
            let mut channel = Channel::default();
            let mut c = Coefficients::new(cut.value, res.value, RATE);
            let (mut last, mut most) = (0.0_f32, 0.0_f32);
            for frame in 0..RATE as usize * 2 {
                if at(frame) {
                    (cut.target, res.target, drive.target) = (cutoff(2000.0), 0.8, 0.6);
                } else {
                    (cut.target, res.target, drive.target) = (cutoff(500.0), 0.1, 0.0);
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
        assert!(db(0.25, 50.0) < -4.0, "{}", db(0.25, 50.0));
        assert!(db(0.75, 50.0) < db(0.25, 50.0) - 1.0, "{}", db(0.75, 50.0));
    }

    #[test]
    fn the_dirty_filter_peaks_at_its_cutoff_and_self_oscillates_there() {
        for cut in [200.0, 1000.0, 4000.0] {
            // Just short of ringing by itself, its peak is louder at `Cut` than a semitone either
            // side.
            let db = |hertz| gain_db(FilterType::Dirty, cut, 0.8, 0.0, hertz, 0.001);
            let semitone = 2_f32.powf(1.0 / 12.0);
            assert!(db(cut) > db(cut * semitone) && db(cut) > db(cut / semitone));
        }
        // It rings by itself from about `Res D9`, and drive, which adds resonance, rings it louder
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
            let worked_out = |hertz| {
                let mode = FilterMode::LowPass24;
                response_db(
                    FilterType::Dirty,
                    mode,
                    cutoff(cut),
                    res,
                    drive,
                    RATE,
                    hertz,
                )
            };
            let measured = |hertz| gain_db(FilterType::Dirty, cut, res, drive, hertz, 0.0005);
            for hertz in [400.0, 1000.0, 2500.0] {
                near(
                    measured(hertz) - measured(100.0),
                    worked_out(hertz) - worked_out(100.0),
                    0.5,
                );
            }
        }
        let peak = |drive| {
            let mode = FilterMode::LowPass24;
            response_db(FilterType::Dirty, mode, cutoff(cut), res, drive, RATE, cut)
        };
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

    #[test]
    fn each_mode_slopes_as_it_says() {
        // How far a mode falls over the octave from `low` to `high`, with `Cut` at 1 kHz.
        let fall = |kind, mode, low: f32, high: f32| {
            let level = if kind == FilterType::Dirty {
                0.001
            } else {
                0.01
            };
            let db = |hertz| mode_gain_db(kind, mode, 1000.0, 0.0, hertz, 0.0, level);
            (db(low) - db(high)).abs()
        };
        use FilterMode::*;
        for kind in [FilterType::LowPass, FilterType::HighPass] {
            let (low, high) = if kind == FilterType::LowPass {
                (4000.0, 8000.0)
            } else {
                (125.0, 250.0)
            };
            near(fall(kind, Slope12, low, high), 12.0, 1.5);
            near(fall(kind, Slope24, low, high), 24.0, 2.0);
        }
        // A band-pass is twice as steep either side, and a notch twice as wide.
        near(
            fall(FilterType::BandPass, Slope12, 4000.0, 8000.0),
            6.0,
            1.0,
        );
        near(
            fall(FilterType::BandPass, Slope24, 4000.0, 8000.0),
            12.0,
            1.5,
        );
        let notch = |mode| mode_gain_db(FilterType::Notch, mode, 1000.0, 0.0, 500.0, 0.0, 0.1);
        near(notch(Slope24), 2.0 * notch(Slope12), 0.3);
        for kind in [FilterType::Ladder, FilterType::Dirty] {
            near(fall(kind, LowPass24, 4000.0, 8000.0), 24.0, 2.0);
            near(fall(kind, LowPass12, 4000.0, 8000.0), 12.0, 1.0);
            near(fall(kind, HighPass12, 125.0, 250.0), 12.0, 1.0);
            near(fall(kind, HighPass24, 125.0, 250.0), 24.0, 1.0);
            near(fall(kind, BandPass, 125.0, 250.0), 12.0, 1.5);
            near(fall(kind, BandPass, 4000.0, 8000.0), 12.0, 1.5);
            // A notch takes out the cutoff and leaves either end.
            let level = if kind == FilterType::Dirty {
                0.001
            } else {
                0.01
            };
            let db = |hertz| mode_gain_db(kind, Notch, 1000.0, 0.0, hertz, 0.0, level);
            assert!(db(1000.0) < -50.0, "{kind:?} {}", db(1000.0));
            assert!(db(50.0) > -1.0 && db(12_000.0) > -1.0, "{kind:?}");
            // The high-passes pass the top at the level the low-passes pass the bass.
            for mode in [HighPass12, HighPass24] {
                let top = mode_gain_db(kind, mode, 1000.0, 0.0, 12_000.0, 0.0, level);
                near(top, 0.0, 0.5);
            }
        }
    }

    #[test]
    fn the_diodes_low_cut_takes_more_bass_the_higher_it_sits_and_leaves_the_top() {
        let db = |mode, hertz| mode_gain_db(FilterType::Diode, mode, 1000.0, 0.0, hertz, 0.0, 0.01);
        let modes = FilterType::Diode.modes();
        for pair in modes.windows(2) {
            assert!(
                db(pair[1], 50.0) < db(pair[0], 50.0) - 10.0,
                "{:?} and {:?}",
                pair[0],
                pair[1]
            );
        }
        for mode in modes {
            near(db(*mode, 2000.0), db(FilterMode::Full, 2000.0), 1.0);
        }
        // `Full` is the diode `Acid` plays, with its low cut nine octaves under.
        let full = Coefficients::with_mode(0.5, 0.5, FilterMode::Full, RATE);
        assert_eq!(full.mix.low_cut, HIGH_PASS_RATIO);
    }

    #[test]
    fn a_band_pass_peaks_as_loud_as_the_low_pass_is_at_its_loudest() {
        for kind in [FilterType::Ladder, FilterType::Dirty] {
            let level = if kind == FilterType::Dirty {
                0.001
            } else {
                0.01
            };
            let db = |mode, res, hertz| mode_gain_db(kind, mode, 1000.0, res, hertz, 0.0, level);
            // With no resonance, as loud as its bass.
            near(db(FilterMode::BandPass, 0.0, 1000.0), 0.0, 0.6);
            // With a lot, as loud as its peak.
            near(
                db(FilterMode::BandPass, 0.8, 1000.0),
                db(FilterMode::LowPass24, 0.8, 1000.0),
                0.5,
            );
        }
    }

    #[test]
    fn every_mode_of_a_character_filter_rings_where_its_low_pass_does() {
        for kind in [FilterType::Ladder, FilterType::Dirty] {
            for cut in [200.0, 1000.0, 4000.0] {
                let ringing = |mode| {
                    let c = Coefficients::with_mode(cutoff(cut), 1.0, mode, RATE);
                    let mut channel = Channel::default();
                    channel.process(0.5, kind, 0.0, &c);
                    let (mut crossings, mut last) = (0_u32, 0.0_f32);
                    for frame in 0..RATE as usize * 2 {
                        let output = channel.process(0.0, kind, 0.0, &c);
                        if frame > RATE as usize && last <= 0.0 && output > 0.0 {
                            crossings += 1;
                        }
                        last = output;
                    }
                    crossings
                };
                let low = ringing(FilterMode::LowPass24);
                for &mode in kind.modes() {
                    // The ladder's notch takes out the very pitch it rings at.
                    if kind == FilterType::Ladder && mode == FilterMode::Notch {
                        continue;
                    }
                    let found = ringing(mode);
                    assert!(
                        found.abs_diff(low) <= 2,
                        "{kind:?} {mode:?} at {cut}: {found} and {low}"
                    );
                }
            }
        }
    }

    #[test]
    fn changing_mode_glides_without_a_step() {
        // The biggest jump between frames from a tone through it, with its coefficients worked
        // out every 32 frames, as a track does. The mode is held at one of two, or jumps between
        // them every 64 frames, as a lock or an LFO might.
        let biggest_jump = |kind: FilterType, drive: f32, at: &dyn Fn(usize) -> FilterMode| {
            let mut channel = Channel::default();
            let mut c = Coefficients::with_mode(cutoff(800.0), 0.6, at(0), RATE);
            let (mut last, mut most) = (0.0_f32, 0.0_f32);
            for frame in 0..RATE as usize {
                if frame % 32 == 0 {
                    c = Coefficients::with_mode(cutoff(800.0), 0.6, at(frame), RATE);
                }
                let tone = (frame as f32 / RATE as f32 * 300.0 * std::f32::consts::TAU).sin();
                let output = channel.process(tone * 0.5, kind, drive, &c);
                if frame > RATE as usize / 4 {
                    most = most.max((output - last).abs());
                }
                last = output;
            }
            most
        };
        for kind in FilterType::ALL.into_iter().filter(|kind| kind.has_modes()) {
            for drive in [0.0, 0.6] {
                let modes = kind.modes();
                for pair in modes
                    .windows(2)
                    .chain([[modes[0], modes[modes.len() - 1]].as_slice()])
                {
                    let (one, other) = (pair[0], pair[1]);
                    let held =
                        biggest_jump(kind, drive, &|_| one)
                            .max(biggest_jump(kind, drive, &|_| other));
                    let moving = biggest_jump(kind, drive, &|frame| {
                        if frame / 64 % 2 == 1 { other } else { one }
                    });
                    assert!(
                        moving < held * 1.5,
                        "{kind:?} {one:?} and {other:?} at drive {drive}: {held} held and \
                         {moving} moving"
                    );
                }
            }
        }
    }

    #[test]
    fn every_mode_stays_in_bounds_at_full_resonance_with_loud_input_and_fast_sweeps() {
        for (kind, mode) in every_type_and_mode() {
            for drive in [0.0, 1.0] {
                let mut channel = Channel::default();
                let mut most = 0.0_f32;
                for frame in 0..RATE as usize {
                    // Up and down the whole range eight times a second, with a full-level saw in.
                    let along = frame as f32 / RATE as f32;
                    let sweep = (along * 8.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                    let c = Coefficients::with_mode(sweep, 1.0, mode, RATE);
                    let input = (along * 55.0).fract() * 2.0 - 1.0;
                    let output = channel.process(input, kind, drive, &c);
                    assert!(output.is_finite(), "{kind:?} {mode:?} {output}");
                    most = most.max(output.abs());
                }
                // The clean filters' resonance is a peak of 20 dB, which a saw rings up to.
                let bound = if kind.saturates() { 2.0 } else { 4.0 };
                assert!(
                    most < bound,
                    "{kind:?} {mode:?} at drive {drive} reached {most}"
                );
            }
        }
    }
}
