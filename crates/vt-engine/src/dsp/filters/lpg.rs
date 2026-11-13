//! A low-pass gate and envelope filter, from Mutable Instruments' Streams. Ported to Rust from
//! `streams/follower.cc` and `follower.h` (its three-band envelope follower, which rides each
//! band's peaks and smooths them, slower to rise on the low band and slower to fall on the high),
//! `streams/vactrol.cc` and `vactrol.h` (its vactrol model: a first and second order response
//! that opens quickly and closes slowly, a memory that makes it sluggish after it's rested, a
//! little overshoot, and the Gompertz curve its level follows), `streams/svf.cc` and `svf.h` for
//! the follower's crossover, `streams/meta_parameters.h` and `filter_controller.h` for how the
//! cutoff rests and opens, and the curves and time table from `streams/resources/`.
//!
//! What drives it is a follower on what it hears: the slot's own sound, or another track's. In
//! `Gate` the follower excites the vactrol, which moves the level and the cutoff together, as
//! Streams' vactrol does. Streams' plucked mode, which pings the vactrol from a trigger, can't be
//! here, as a slot has no triggers: the follower's rise stands in. In `Filter` and `VCA` the
//! follower moves the cutoff or the level alone, as Streams' follower does. Streams moves its
//! follower's cutoff by where the sound's spectrum sits; here it follows the level, as an envelope
//! filter does.
//!
//! Streams is fixed point, at 31.089 kHz, and drives an analogue filter and VCA. Here it's all
//! float, at the program's rate: each of Streams' per-sample constants is changed to take as long
//! here, and the filter is a two-pole state variable filter. The follower's crossover is the same
//! filter in place of Streams' Chamberlin one, which isn't stable at every rate.
//!
//! Copyright 2014 Emilie Gillet.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use vt_model::insert::{GateMode, LowPassGate as Settings};

use crate::dsp::blocks::plaits_parts::{Svf, tan_accurate};
use crate::dsp::blocks::sides::{Sides, same_bits_by_eq, same_bits_by_fields};
use crate::dsp::blocks::smoother::Smoothed;

/// The rate Streams runs at, which its per-sample constants are for.
const STREAMS_RATE: f64 = 31_089.0;
/// Where the follower's crossover splits it into three bands, as MIDI notes: about 110 Hz and
/// 1.2 kHz, as Streams has them.
const LOW_SPLIT: f32 = 45.0;
const MEDIUM_SPLIT: f32 = 86.0;
/// The crossover's Q, as Streams' filters with no resonance have it.
const SPLIT_Q: f32 = 0.5;
/// How far up Streams' time table each band's attack and decay are, so the low band rises more
/// slowly and the high band falls more slowly, where there's more noise.
const ATTACK_OFFSETS: [f32; 3] = [39.0, 19.0, 0.0];
const DECAY_OFFSETS: [f32; 3] = [39.0, 19.0, 99.0];
/// How hard the follower excites the vactrol at its loudest: as hard as a 5 V gate does Streams'
/// with its knob fully open, three quarters of the way to full of the 8 V it reads.
const DRIVE: f32 = 0.75 * 5.0 / 8.0;
/// Streams' vactrol, per sample at its rate: how fast what excites it rises, how fast its
/// overshoot follows, and how fast it becomes sensitised and recovers.
const EXCITE_RISE: f64 = 0.5;
const OVERSHOOT: f64 = 67_976_239.0 / 2_147_483_648.0;
const SENSITISE: f64 = 138_132.0 / 2_147_483_648.0;
const DESENSITISE: f64 = 1_151.0 / 2_147_483_648.0;
/// How much of its cutoff's range the vactrol keeps open as it closes, so its tail rings.
const RINGING_TAIL: f32 = 8_192.0 / 65_535.0;
/// The Gompertz curve's lowest and highest, at `0` and `1`, to scale it to that range.
const GOMPERTZ_LOW: f32 = 4.539_993e-5;
const GOMPERTZ_HIGH: f32 = 1.099_977_4;
/// The first step of Streams' table of the curve.
const GOMPERTZ_STEP: f32 = 1.0 / 1024.0;
/// The filter's cutoff from `0` to `1`: from 20 Hz to 20 kHz, evenly in pitch.
const LOWEST_HERTZ: f32 = 20.0;
const OCTAVES: f32 = 9.965_784;
/// The filter's Q, from flat at `00`, rising evenly in ratio to a strong peak at `FF`.
const LEAST_Q: f32 = 0.5;
const MOST_Q: f32 = 10.0;

/// A time in Streams' table of vactrol times, by its place: a millisecond at `0`, ten times as
/// long every 128, with the first few shorter.
fn table_seconds(index: f32) -> f64 {
    match index.round() as i32 {
        ..=0 => 0.000_1,
        1 => 0.000_2,
        2 => 0.000_5,
        3 => 0.001,
        index => 0.001 * 10_f64.powf(f64::from(index) / 128.0),
    }
}

/// How much a one-pole filter moves each sample at Streams' rate, for a time in its table.
fn streams_coefficient(index: f32) -> f64 {
    1.0 - (-1.0 / (table_seconds(index) * STREAMS_RATE)).exp()
}

/// A coefficient for Streams' rate, changed to take as long at another.
fn at_rate(coefficient: f64, rate: u32) -> f32 {
    (1.0 - (1.0 - coefficient.min(1.0)).powf(STREAMS_RATE / f64::from(rate))) as f32
}

/// The follower's attack and decay for Streams' shape knob, from `0` to `1`, as places in the
/// table: a fast attack and a decay from 10 ms to 100 ms on the first half, then up to 20 ms and
/// 200 ms.
fn follower_times(shape: f32) -> (f32, f32) {
    if shape < 0.5 {
        let along = shape * 2.0;
        (39.0 * along, 128.0 + 128.0 * along)
    } else {
        let along = (shape - 0.5) * 2.0;
        (39.0 + 128.0 * along, 256.0 + 39.0 * along)
    }
}

/// The vactrol's attack and decay for the shape knob, as places in the table: a 10 ms attack and
/// a decay from 50 ms to 2 s on the first half, then the attack slowing to half a second as the
/// decay comes back to one, then both quickening.
fn vactrol_times(shape: f32) -> (f32, f32) {
    let shape = shape * 65_536.0;
    if shape < 32_768.0 {
        (128.0, 227.0 + 196.0 * shape / 32_768.0)
    } else if shape < 49_512.0 {
        let along = (shape - 32_768.0) / 32_768.0;
        (128.0 + 227.0 * along, 423.0 - 89.0 * along)
    } else {
        let into = shape - 49_512.0;
        (355.0 - into / 128.0, 384.0 - 128.0 * into / 32_768.0)
    }
}

/// The curve a vactrol's level follows as it's lit, from `0` to `1`: slow to start, then steep,
/// then leaning gently up to full. Streams reads it from a table of 1024 steps, so below the first
/// step it falls in a straight line to nothing, where the curve alone would linger.
fn gompertz(x: f32) -> f32 {
    let curve = |x: f32| {
        let raw = (-10.0 * (-13.0 * x).exp()).exp() + 0.1 * x.powf(0.05);
        (raw - GOMPERTZ_LOW) / (GOMPERTZ_HIGH - GOMPERTZ_LOW)
    };
    if x < GOMPERTZ_STEP {
        curve(GOMPERTZ_STEP) * x.max(0.0) / GOMPERTZ_STEP
    } else {
        curve(x)
    }
}

/// Streams' envelope follower: three bands, each riding its peaks and then smoothed.
#[derive(Debug, Clone, Copy, Default)]
struct Follower {
    low: Svf,
    medium: Svf,
    /// Each band's last two energies, for finding its peaks.
    energy: [[f32; 2]; 3],
    /// Each band's peak, and the same smoothed.
    peak: [f32; 3],
    smoothed: [f32; 3],
    attack: [f32; 3],
    decay: [f32; 3],
}

impl Follower {
    /// Split at the same frequencies at any rate, and rise and fall over these places in the time
    /// table.
    fn tune(&mut self, attack: f32, decay: f32, rate: u32) {
        for (svf, note) in [(&mut self.low, LOW_SPLIT), (&mut self.medium, MEDIUM_SPLIT)] {
            let hertz = 440.0 * ((note - 69.0) / 12.0).exp2();
            svf.set_g_q(tan_accurate(hertz / rate as f32), SPLIT_Q);
        }
        for band in 0..3 {
            self.attack[band] = at_rate(streams_coefficient(attack + ATTACK_OFFSETS[band]), rate);
            self.decay[band] = at_rate(streams_coefficient(decay + DECAY_OFFSETS[band]), rate);
        }
    }

    /// How loud it's been lately, from `0` to `1`, on Streams' square root curve, so a full-scale
    /// sound is all the way.
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let (low_high, _, low) = self.low.process_all(input);
        let (high, _, medium) = self.medium.process_all(low_high);
        let mut envelope = 0.0;
        for (band, sample) in [low, medium, high].into_iter().enumerate() {
            let energy = sample * sample;
            let [before, last] = self.energy[band];
            // Ride a rising peak, and otherwise hold, snapping to each local peak.
            if before < last && last < energy && energy > self.peak[band] {
                self.peak[band] = energy;
            }
            if before <= last && last >= energy {
                self.peak[band] = last;
            }
            self.energy[band] = [last, energy];
            let error = self.peak[band] - self.smoothed[band];
            let coefficient = if error > 0.0 {
                self.attack[band]
            } else {
                self.decay[band]
            };
            self.smoothed[band] += error * coefficient;
            envelope += self.smoothed[band];
        }
        (envelope * 2.0).clamp(0.0, 1.0).sqrt()
    }
}

/// Streams' vactrol, lit by what excites it.
#[derive(Debug, Clone, Copy, Default)]
struct Vactrol {
    /// What excites it, with its falls slowed so a short pulse still lights it.
    excite: f32,
    /// Its first and second order responses, how sensitised it is, and its overshoot's follower.
    first: f32,
    second: f32,
    sensitised: f32,
    overshoot: f32,
    attack: f32,
    decay: f32,
    fast_attack: f32,
    fast_decay: f32,
    excite_fall: f32,
    /// The constants Streams has per sample, for this rate.
    excite_rise: f32,
    overshoot_rate: f32,
    sensitise: f32,
    desensitise: f32,
}

same_bits_by_fields!(Follower {
    low,
    medium,
    energy,
    peak,
    smoothed,
    attack,
    decay,
});
same_bits_by_fields!(Vactrol {
    excite,
    first,
    second,
    sensitised,
    overshoot,
    attack,
    decay,
    fast_attack,
    fast_decay,
    excite_fall,
    excite_rise,
    overshoot_rate,
    sensitise,
    desensitise,
});
same_bits_by_eq!(Settings);
same_bits_by_fields!(LowPassGate {
    follower,
    vactrol,
    filters,
    amount,
    base,
    resonance,
    modes,
    level,
    cutoff,
    tuned,
});

impl Vactrol {
    /// Take these places in the time table for its attack and decay, at a rate.
    fn tune(&mut self, attack: f32, decay: f32, rate: u32) {
        let decay_coefficient = streams_coefficient(decay);
        self.attack = at_rate(streams_coefficient(attack), rate);
        self.fast_attack = at_rate(streams_coefficient(attack - 128.0), rate);
        // Out of its plucked mode, Streams halves its decay's coefficient, doubling its time.
        self.decay = at_rate(decay_coefficient / 2.0, rate);
        self.fast_decay = at_rate(streams_coefficient(decay - 128.0), rate);
        self.excite_fall = at_rate(decay_coefficient, rate);
        self.excite_rise = at_rate(EXCITE_RISE, rate);
        self.overshoot_rate = at_rate(OVERSHOOT, rate);
        self.sensitise = at_rate(SENSITISE, rate);
        self.desensitise = at_rate(DESENSITISE, rate);
    }

    /// Its level and how far its filter opens, each from `0` to `1`, for what excites it now.
    #[inline]
    fn process(&mut self, excite: f32) -> (f32, f32) {
        let error = excite.max(0.0) - self.excite;
        self.excite += error
            * if error > 0.0 {
                self.excite_rise
            } else {
                self.excite_fall
            };
        let input = DRIVE * self.excite;
        self.overshoot += (input - self.overshoot) * self.overshoot_rate;
        let error = input - self.first;
        let coefficient = if error > 0.0 {
            if self.second > 0.0 {
                // Rested, it's less sensitive, and takes longer to light.
                let slower = 1.0 + 255.0 * (1.0 - self.sensitised) / 64.0;
                (self.attack * slower).min(1.0)
            } else {
                self.fast_attack
            }
        } else if self.second < 0.0 {
            self.decay
        } else {
            self.fast_decay
        };
        self.first += error * coefficient;
        self.second += (error - self.second) * coefficient;
        // Its memory: lit, it becomes sensitised in about a second, and recovers over a minute.
        let sensitivity = if self.first > 0.125 {
            1.0
        } else {
            self.first * 8.0
        };
        let error = sensitivity - self.sensitised;
        self.sensitised += error
            * if error > 0.0 {
                self.sensitise
            } else {
                self.desensitise
            };
        // A little overshoot, then the curves its level and cutoff follow.
        let lit = (self.first + 2.0 * self.overshoot * self.second).clamp(0.0, 2.0);
        let level = if lit < 0.5 { gompertz(lit * 2.0) } else { 1.0 };
        let cutoff = (lit * 2.0).min(1.0);
        (level, cutoff * cutoff)
    }
}

/// A low-pass gate: Streams' follower and vactrol, and a filter for each side.
#[derive(Debug, Clone, Copy, Default)]
pub struct LowPassGate {
    follower: Follower,
    vactrol: Vactrol,
    /// Each side's filter, worked out once for both while a sound is the same on both.
    filters: Sides<Svf>,
    amount: Smoothed,
    base: Smoothed,
    resonance: Smoothed,
    /// How far into each mode it is, so changing mode fades rather than jumps.
    modes: [Smoothed; 3],
    /// Its level and cutoff now, for a meter or a test.
    level: f32,
    cutoff: f32,
    /// What its times were worked out for: the settings and the rate.
    tuned: Option<(Settings, u32)>,
}

impl LowPassGate {
    /// Its level and cutoff now, each from `0` to `1`.
    pub fn opening(&self) -> (f32, f32) {
        (self.level, self.cutoff)
    }

    /// Take what's set now, jumping to it the first time.
    fn follow(&mut self, settings: Settings, rate: u32) {
        if self.tuned == Some((settings, rate)) {
            return;
        }
        let first = self.tuned.is_none();
        let retune = self.tuned.is_none_or(|(was, was_rate)| {
            was.response != settings.response || was.mode != settings.mode || was_rate != rate
        });
        self.amount.target = f32::from(settings.amount) / 255.0;
        self.base.target = f32::from(settings.base) / 255.0;
        self.resonance.target = f32::from(settings.resonance) / 255.0;
        for (mode, weight) in GateMode::ALL.into_iter().zip(&mut self.modes) {
            weight.target = if mode == settings.mode { 1.0 } else { 0.0 };
        }
        if first {
            self.amount.settle();
            self.base.settle();
            self.resonance.settle();
            self.modes.iter_mut().for_each(Smoothed::settle);
        }
        if retune {
            let shape = f32::from(settings.response) / 255.0;
            // As a gate, the vactrol shapes it, so the follower is as quick as it goes.
            let (attack, decay) = follower_times(if settings.mode == GateMode::Gate {
                0.0
            } else {
                shape
            });
            self.follower.tune(attack, decay, rate);
            let (attack, decay) = vactrol_times(shape);
            self.vactrol.tune(attack, decay, rate);
        }
        self.tuned = Some((settings, rate));
    }

    /// One frame through the gate, all wet, opened by `key`: the frame itself unless it hears
    /// another track. `same` says the frame's two sides are the same, bit for bit.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        same: bool,
        key: [f32; 2],
        settings: Settings,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        self.follow(settings, rate);
        let amount = self.amount.next(smoothing);
        let base = self.base.next(smoothing);
        let resonance = self.resonance.next(smoothing);
        let [gate, filter, vca] = self.modes.each_mut().map(|weight| weight.next(smoothing));
        let followed = self.follower.process((key[0] + key[1]) * 0.5);
        let (lit, opened) = self.vactrol.process(followed);
        // What it opens across, from where it rests. The vactrol keeps a little open as its tail.
        let span = (1.0 - base) * amount;
        let tail = RINGING_TAIL.min(span);
        let gate_cutoff = base + tail + (span - tail) * opened;
        let filter_cutoff = base + span * followed;
        self.cutoff = gate * gate_cutoff + filter * filter_cutoff + vca;
        self.level = gate * lit + filter + vca * (base + span * followed);
        let hertz = LOWEST_HERTZ * (self.cutoff.min(1.0) * OCTAVES).exp2();
        let g = tan_accurate((hertz / rate as f32).min(0.45));
        let q = LEAST_Q * (MOST_Q / LEAST_Q).powf(resonance);
        let (filtered, level) = (1.0 - vca, self.level);
        self.filters.process(input, same, |filter, sample| {
            filter.set_g_q(g, q);
            let low = filter.low_pass(sample);
            (low * filtered + sample * vca) * level
        })
    }
}

#[cfg(test)]
mod tests {
    use vt_model::constants::UNITY_LEVEL;

    use super::*;

    const RATE: u32 = 48_000;

    fn settings(mode: GateMode) -> Settings {
        Settings {
            mode,
            ..Settings::DEFAULT
        }
    }

    fn sine(hertz: f32, frame: usize, rate: u32) -> f32 {
        (frame as f32 / rate as f32 * hertz * std::f32::consts::TAU).sin()
    }

    /// A band-limited sawtooth: its first twenty harmonics.
    fn saw(hertz: f32, frame: usize, rate: u32) -> f32 {
        (1..=20)
            .map(|harmonic| sine(hertz * harmonic as f32, frame, rate) / harmonic as f32)
            .sum::<f32>()
            * 0.5
    }

    #[test]
    fn the_gompertz_curve_runs_from_nothing_to_full() {
        assert_eq!(gompertz(0.0), 0.0);
        assert!((gompertz(1.0) - 1.0).abs() < 1e-6);
        let low = (-10.0_f32).exp();
        let high = (-10.0 * (-13.0_f32).exp()).exp() + 0.1;
        assert!((GOMPERTZ_LOW - low).abs() < 1e-9 && (GOMPERTZ_HIGH - high).abs() < 1e-6);
        assert!((OCTAVES - 1000_f32.log2()).abs() < 1e-5);
        // It rises all the way.
        let mut last = 0.0;
        for step in 1..=100 {
            let now = gompertz(step as f32 / 100.0);
            assert!(now > last, "{step}");
            last = now;
        }
    }

    #[test]
    fn its_times_are_streams_and_take_as_long_at_any_rate() {
        // A second is 384 places along, and a coefficient at Streams' own rate is unchanged.
        assert!((table_seconds(384.0) - 1.0).abs() < 1e-9);
        let coefficient = streams_coefficient(256.0);
        assert!((f64::from(at_rate(coefficient, 31_089)) - coefficient).abs() < 1e-7);
        // Twice the rate, half as much each sample, so the same time.
        let (slow, fast) = (at_rate(coefficient, 48_000), at_rate(coefficient, 96_000));
        assert!((slow / fast - 2.0).abs() < 0.01, "{slow} {fast}");
        // The vactrol's decay grows to 2 s at the middle of its knob, as Streams' comments have it.
        assert_eq!(vactrol_times(0.0), (128.0, 227.0));
        assert!((vactrol_times(0.5).1 - 423.0).abs() < 1e-3);
        assert!((table_seconds(423.0) - 2.0).abs() < 0.05);
        assert_eq!(follower_times(0.0), (0.0, 128.0));
    }

    /// The gate's level, frame by frame, for a tone held for `held` frames and then stopped.
    fn burst(settings: Settings, held: usize, frames: usize, rate: u32) -> Vec<f32> {
        let mut gate = LowPassGate::default();
        (0..frames)
            .map(|frame| {
                let sample = if frame < held {
                    sine(220.0, frame, rate) * 0.8
                } else {
                    0.0
                };
                gate.process([sample; 2], true, [sample; 2], settings, 1.0, rate);
                gate.opening().0
            })
            .collect()
    }

    #[test]
    fn a_pluck_opens_the_gate_then_it_closes_more_slowly_than_it_opened() {
        let held = RATE as usize / 20;
        let levels = burst(settings(GateMode::Gate), held, RATE as usize * 4, RATE);
        let most = levels.iter().fold(0.0_f32, |most, level| most.max(*level));
        assert!(most > 0.9, "{most}");
        let opened = levels.iter().position(|level| *level > most * 0.9).unwrap();
        let peaked = levels.iter().position(|level| *level == most).unwrap();
        let closed = peaked
            + levels[peaked..]
                .iter()
                .position(|level| *level < most * 0.1)
                .unwrap();
        // It opens within the burst, and closes long after, as a vactrol does.
        assert!(opened < held, "{opened}");
        let closing = closed - held;
        assert!(closing > opened * 5, "{opened} {closing}");
        assert!(closing > RATE as usize / 10, "{closing}");
        // And it shuts in the end.
        assert!(*levels.last().unwrap() < 0.01, "{}", levels.last().unwrap());
        // Its cutoff falls with it.
        let mut gate = LowPassGate::default();
        let mut cutoffs = Vec::new();
        for frame in 0..RATE as usize {
            let sample = if frame < held {
                sine(220.0, frame, RATE) * 0.8
            } else {
                0.0
            };
            gate.process(
                [sample; 2],
                true,
                [sample; 2],
                settings(GateMode::Gate),
                1.0,
                RATE,
            );
            cutoffs.push(gate.opening().1);
        }
        assert!(
            cutoffs[held] > cutoffs[RATE as usize - 1] + 0.3,
            "{cutoffs:?}"
        );
    }

    #[test]
    fn a_longer_response_closes_more_slowly() {
        let closing = |response: u8| {
            let levels = burst(
                Settings {
                    response,
                    ..Settings::DEFAULT
                },
                RATE as usize / 20,
                RATE as usize * 6,
                RATE,
            );
            levels.iter().rposition(|level| *level > 0.1).unwrap()
        };
        let (short, long) = (closing(0x00), closing(0x80));
        assert!(long > short * 3, "{short} {long}");
    }

    /// How much of the output's energy is above 2 kHz and how loud its 110 Hz fundamental is, for
    /// a sawtooth at a level, settled.
    fn brightness_and_fundamental(settings: Settings, level: f32) -> (f32, f32) {
        let mut gate = LowPassGate::default();
        let mut high = Svf::default();
        high.set_g_q(tan_accurate(2_000.0 / RATE as f32), 0.707);
        let (mut energy, mut real, mut imaginary) = (0.0_f64, 0.0_f64, 0.0_f64);
        let frames = RATE as usize;
        for frame in 0..frames {
            let sample = saw(110.0, frame, RATE) * level;
            let out = gate.process([sample; 2], true, [sample; 2], settings, 1.0, RATE)[0];
            let (above, _, _) = high.process_all(out);
            if frame >= frames / 2 {
                energy += f64::from(above * above);
                let phase =
                    f64::from(frame as u32) / f64::from(RATE) * 110.0 * std::f64::consts::TAU;
                real += f64::from(out) * phase.cos();
                imaginary += f64::from(out) * phase.sin();
            }
        }
        let fundamental = (real * real + imaginary * imaginary).sqrt() * 4.0 / frames as f64;
        // The sawtooth's fundamental is at half its level.
        let fundamental = fundamental as f32 / (level * 0.5);
        ((energy / f64::from(level * level)) as f32, fundamental)
    }

    #[test]
    fn as_a_filter_louder_is_brighter_and_the_level_stays() {
        // Resting at about 400 Hz, above the fundamental.
        let filter = Settings {
            amount: 0xFF,
            base: 0x70,
            ..settings(GateMode::Filter)
        };
        let (quiet_bright, quiet_level) = brightness_and_fundamental(filter, 0.05);
        let (loud_bright, loud_level) = brightness_and_fundamental(filter, 0.8);
        // Relative to what went in, far more of the top comes through when it's loud.
        assert!(
            loud_bright > quiet_bright * 20.0,
            "{quiet_bright} {loud_bright}"
        );
        // While its fundamental stays within a dB or so of what went in, either way.
        for level in [quiet_level, loud_level] {
            let db = 20.0 * level.log10();
            assert!(db.abs() < 1.0, "{quiet_level} {loud_level}");
        }
        // As a gate, the quiet one is turned down as well.
        let (_, gated) = brightness_and_fundamental(settings(GateMode::Gate), 0.05);
        assert!(gated < quiet_level * 0.7, "{gated} {quiet_level}");
        // As a VCA, the loud one isn't darkened.
        let vca = Settings {
            mode: GateMode::Vca,
            ..filter
        };
        let (bright, _) = brightness_and_fundamental(vca, 0.8);
        assert!(bright > loud_bright * 0.9, "{bright} {loud_bright}");
    }

    #[test]
    fn hearing_silence_it_stays_closed() {
        for mode in [GateMode::Gate, GateMode::Vca] {
            let mut gate = LowPassGate::default();
            for frame in 0..RATE as usize / 2 {
                let sample = sine(220.0, frame, RATE) * 0.8;
                let out = gate.process([sample; 2], true, [0.0; 2], settings(mode), 1.0, RATE);
                assert_eq!(out, [0.0; 2], "{mode:?} {frame}");
            }
        }
        // As a filter it lets the sound through, closed down to where it rests.
        let mut gate = LowPassGate::default();
        for frame in 0..RATE as usize / 2 {
            let sample = sine(220.0, frame, RATE) * 0.8;
            gate.process(
                [sample; 2],
                true,
                [0.0; 2],
                settings(GateMode::Filter),
                1.0,
                RATE,
            );
        }
        assert_eq!(gate.opening(), (1.0, 0.0));
    }

    #[test]
    fn moving_its_settings_glides_without_a_step() {
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        for mode in GateMode::ALL {
            let mut gate = LowPassGate::default();
            let mut last = 0.0;
            for frame in 0..RATE as usize * 2 {
                // A jump every 64 frames, as a knob or an LFO steps it block by block.
                let step = (frame / 64 % 2) as u8;
                let jumpy = Settings {
                    mode,
                    amount: 0x20 + step * 0xD0,
                    base: 0x10 + step * 0x60,
                    resonance: step * 0xC0,
                    response: 0x10 + step * 0xE0,
                    ..Settings::DEFAULT
                };
                // A steady low tone, so what moves is the gate and not the sound.
                let sample = sine(55.0, frame, RATE) * 0.5;
                let out = gate.process([sample; 2], true, [sample; 2], jumpy, smoothing, RATE)[0];
                if frame > RATE as usize / 2 {
                    assert!(
                        (out - last).abs() < 0.03,
                        "{mode:?} {frame}: {last} to {out}"
                    );
                }
                last = out;
            }
        }
        // And changing mode fades between them.
        let mut gate = LowPassGate::default();
        let mut last = 0.0;
        for frame in 0..RATE as usize * 2 {
            let mode = GateMode::ALL[frame / 4800 % 3];
            let sample = sine(55.0, frame, RATE) * 0.5;
            let out = gate.process(
                [sample; 2],
                true,
                [sample; 2],
                settings(mode),
                smoothing,
                RATE,
            )[0];
            if frame > RATE as usize / 2 {
                assert!(
                    (out - last).abs() < 0.03,
                    "{mode:?} {frame}: {last} to {out}"
                );
            }
            last = out;
        }
    }

    #[test]
    fn it_stays_bounded_at_every_extreme_at_any_rate() {
        for rate in [44_100, 192_000] {
            for mode in GateMode::ALL {
                for (amount, base, resonance, response) in [
                    (0x00, 0x00, 0x00, 0x00),
                    (0xFF, 0x00, 0xFF, 0xFF),
                    (0xFF, 0xFF, 0xFF, 0x00),
                    (0xFF, 0x00, 0xFF, 0x80),
                    (0x00, 0xFF, 0x00, 0xFF),
                ] {
                    let settings = Settings {
                        mode,
                        amount,
                        base,
                        resonance,
                        response,
                        gain: UNITY_LEVEL,
                        ..Settings::DEFAULT
                    };
                    let mut gate = LowPassGate::default();
                    let mut most = 0.0_f32;
                    for frame in 0..rate as usize / 2 {
                        // Loud noise-like bursts and a full-scale square.
                        let sample = if frame % 9000 < 4000 {
                            if sine(97.0, frame, rate) > 0.0 {
                                1.0
                            } else {
                                -1.0
                            }
                        } else {
                            0.0
                        };
                        let out = gate.process(
                            [sample, -sample],
                            false,
                            [sample; 2],
                            settings,
                            1.0,
                            rate,
                        );
                        assert!(out.iter().all(|s| s.is_finite()), "{rate} {mode:?}");
                        most = most.max(out[0].abs()).max(out[1].abs());
                    }
                    // A full resonance peaks at its Q, and no further.
                    assert!(most < MOST_Q * 1.5, "{rate} {mode:?} {settings:?}: {most}");
                    let (level, cutoff) = gate.opening();
                    assert!((0.0..=1.0).contains(&level), "{level}");
                    assert!((0.0..=1.0).contains(&cutoff), "{cutoff}");
                }
            }
        }
    }

    #[test]
    fn a_pluck_takes_as_long_at_any_rate() {
        let closing = |rate: u32| {
            let levels = burst(
                settings(GateMode::Gate),
                rate as usize / 20,
                rate as usize * 2,
                rate,
            );
            levels.iter().rposition(|level| *level > 0.1).unwrap() as f32 / rate as f32
        };
        let (slow, fast) = (closing(44_100), closing(192_000));
        assert!((slow - fast).abs() < slow * 0.05, "{slow} {fast}");
    }
}
