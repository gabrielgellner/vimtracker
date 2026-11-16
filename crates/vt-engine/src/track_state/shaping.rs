//! The audio path of a track, a group, a return and the master: the trim, the filter, the
//! slots, the level and the pan.

use vt_model::settings::SettingId;
use vt_model::song::{FilterMode, FilterType};

use crate::dsp::blocks::sides::same_sides;
use crate::dsp::blocks::smoother::{coefficient, fast_coefficient};
use crate::dsp::effects::slot::SidechainFrames;
use crate::dsp::filters::filter::{Coefficients, Ramp, safe_resonance};

use super::TrackState;

/// How often filter coefficients are recalculated while the cutoff moves, in frames. The track's
/// filter ramps to each new set over as many frames.
const FILTER_UPDATE_FRAMES: u32 = 32;
/// How close to wide open the utility filter counts as out of the way, on `0..=1`.
const TRIM_OPEN: f32 = 0.001;
/// How far an octave moves the cutoff on its `0..=1` scale, which spans 20 Hz to 20 kHz, a
/// thousandfold: `Key track` moves it an octave for each octave of the note.
const CUTOFF_PER_OCTAVE: f32 = 1.0 / 9.965_784;

/// The pan gains for a `0..=1` position. The centre is unity, and it keeps equal power.
#[inline]
pub(super) fn pan_gains(position: f32) -> (f32, f32) {
    let angle = position * std::f32::consts::FRAC_PI_2;
    (
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    )
}

/// Map a `PAN` value to a `0..=1` position, with `80` as exact centre.
pub(super) fn pan_position(value: f32) -> f32 {
    if value >= 128.0 {
        0.5 + (value - 128.0) / 127.0 * 0.5
    } else {
        value / 128.0 * 0.5
    }
}

/// What every frame of a run is worked out with: the usual smoother, the fast one for what
/// modulation moves, and the sample rate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RunContext {
    pub smoothing: f32,
    pub fast: f32,
    pub rate: u32,
}

impl RunContext {
    /// The smoothers at a sample rate.
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            smoothing: coefficient(rate),
            fast: fast_coefficient(rate),
            rate,
        }
    }

    /// Which smoother a value follows: the fast one while modulation moves it, so it keeps up, and
    /// otherwise the usual one, so a change on its own doesn't jump.
    #[inline]
    fn smoother(self, moving: bool) -> f32 {
        if moving { self.fast } else { self.smoothing }
    }
}

/// What the master does every frame while it rests: what it takes in after its trim, what its
/// slots give, and what it gives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MasterRest {
    trimmed: [f32; 2],
    sidechain: [f32; 2],
    pub out: [f32; 2],
}

/// One track's frame, and what it sends to the returns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShapedFrame {
    pub dry: [f32; 2],
    pub send_a: [f32; 2],
    pub send_b: [f32; 2],
}

impl ShapedFrame {
    /// Nothing, as a group with nothing coming in gives.
    pub(crate) const SILENT: Self = Self {
        dry: [0.0; 2],
        send_a: [0.0; 2],
        send_b: [0.0; 2],
    };
}

impl TrackState {
    /// The pan gains for a position, worked out again only when it moves.
    #[inline]
    fn pan_gains(&mut self, position: f32) -> (f32, f32) {
        if position.to_bits() != self.panned.0.to_bits() {
            self.panned = (position, pan_gains(position));
        }
        self.panned.1
    }

    /// Which smoother a setting's gain follows, by whether modulation moves it now.
    #[inline]
    fn smoother_for(&self, target: SettingId, run: RunContext) -> f32 {
        run.smoother(self.offsets[target.index()] != 0.0)
    }

    /// The utility filter. It's out of the way when wide open, and coefficients are only worked out
    /// while it moves.
    #[inline]
    fn trimmed(&mut self, input: [f32; 2], run: RunContext) -> [f32; 2] {
        let low_cut = self.low_cut.next(run.smoothing);
        let high_cut = self.high_cut.next(run.smoothing);
        let (low_on, high_on) = (low_cut > TRIM_OPEN, high_cut < 1.0 - TRIM_OPEN);
        if !low_on && !high_on {
            self.low_cut_coefficients = None;
            self.high_cut_coefficients = None;
            return input;
        }
        if self.trim_countdown == 0 {
            self.trim_countdown = FILTER_UPDATE_FRAMES;
            let update = |slot: &mut Option<(f32, Coefficients)>, value: f32| {
                let current = slot.is_some_and(|(at, _)| (at - value).abs() < TRIM_OPEN);
                if !current {
                    *slot = Some((value, Coefficients::new(value, 0.0, run.rate)));
                }
            };
            if low_on {
                if self.low_cut_coefficients.is_none() {
                    self.trim[0].clear_low_cut();
                    self.trim[1].clear_low_cut();
                }
                update(&mut self.low_cut_coefficients, low_cut);
            } else {
                self.low_cut_coefficients = None;
            }
            if high_on {
                if self.high_cut_coefficients.is_none() {
                    self.trim[0].prime_high_cut(input[0]);
                    self.trim[1].prime_high_cut(input[1]);
                }
                update(&mut self.high_cut_coefficients, high_cut);
            } else {
                self.high_cut_coefficients = None;
            }
        }
        self.trim_countdown -= 1;
        let mut output = input;
        for (sample, trim) in output.iter_mut().zip(&mut self.trim) {
            if let Some((_, c)) = &self.low_cut_coefficients {
                *sample = trim.low_cut(*sample, c);
            }
            if let Some((_, c)) = &self.high_cut_coefficients {
                *sample = trim.high_cut(*sample, c);
            }
        }
        output
    }

    /// The mode the filter plays: held `MODE` and what modulation adds, as a whole place in the
    /// list, or its type's first if its type doesn't offer that.
    fn playing_mode(&self) -> FilterMode {
        let moved = self.filter_mode + self.offsets[SettingId::FILTER_MODE.index()] * 255.0;
        let place = moved.round().clamp(0.0, f32::from(u8::MAX)) as u8;
        self.filter_type
            .plays(FilterMode::at(place).unwrap_or_default())
    }

    /// What `Key track` adds to the cutoff this frame: the voice's note in octaves from `C-3`, an
    /// octave of cutoff for each, smoothed as `CUT` is, and nothing while it's off. Its first
    /// note after the filter has settled is taken at once, as the cutoff is (decision 381).
    #[inline]
    fn followed_key(&mut self, on: bool, run: RunContext) -> f32 {
        if on {
            self.key_track.target = self.voice.key_octaves() * CUTOFF_PER_OCTAVE;
        } else {
            self.key_track.target = 0.0;
        }
        if self.key_settles && self.voice.active() {
            self.key_settles = false;
            self.key_track.settle();
        }
        self.key_track.next(run.smoothing)
    }

    /// Trim, filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub(crate) fn shape(
        &mut self,
        input: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> ShapedFrame {
        // The trim comes first, as a console's input gain does, so it sets how hard the filter,
        // its drive and the slots are hit. At 0 dB it's exactly 1, which leaves the voice as it is.
        let trim = self.input_trim.next(run.smoothing);
        // A NaN or an infinity from a voice would stay in the filter, and the utility filter
        // after it, for good, so it's heard as a frame of silence instead, as the slots do. Level,
        // pan and the sends only scale the frame and keep nothing of it.
        let input = [input[0] * trim, input[1] * trim]
            .map(|sample| if sample.is_finite() { sample } else { 0.0 });
        self.take_in_after_trim(input);
        let key_tracks = self.held_key_track >= 0.5;
        let cutoff = (self.cutoff.next(run.smoothing)
            + self.cut_offset.next(run.fast)
            + self.cut_envelope.next(run.fast)
            + self.followed_key(key_tracks, run))
        .clamp(0.0, 1.0);
        let resonance = (self.resonance.next(run.smoothing)
            + self.res_offset.next(run.fast)
            + self.res_envelope.next(run.fast))
        .clamp(0.0, 1.0);
        let position = (self.pan.next(run.smoothing)
            + self.pan_offset.next(run.fast)
            + self.pan_envelope.next(run.fast))
        .clamp(0.0, 1.0);
        let drive = self.drive.next(run.smoothing);
        let punch = self.punch.next(run.smoothing);
        let safe = self.safe.next(run.smoothing);
        // Following the key, the filter keeps running at the top, so a lower note doesn't start it
        // from where it stopped.
        let open = self.filter_type == FilterType::LowPass
            && !key_tracks
            && cutoff > 0.999
            && resonance < 0.001
            && drive < 0.001
            && !self.filter_moving;
        let filtered = if open {
            self.coefficients = None;
            input
        } else {
            if self.filter_countdown == 0 || self.coefficients.is_none() {
                let mode = self.playing_mode();
                // So little `Safe` is none, so the filter is as it was without it.
                let resonance = if safe >= 0.001 {
                    safe_resonance(self.filter_type, resonance, cutoff, drive, safe)
                } else {
                    resonance
                };
                let mut c = Coefficients::with_mode(cutoff, resonance, mode, run.rate);
                // A punch this small is none, so the filter is as it was without it.
                if punch >= 0.001 && self.filter_type.has_punch() {
                    c = c.punched(punch, run.rate);
                }
                match &mut self.coefficients {
                    Some(ramp) => ramp.toward(&c, FILTER_UPDATE_FRAMES),
                    None => self.coefficients = Some(Ramp::at(c)),
                }
                self.filter_countdown = FILTER_UPDATE_FRAMES;
            }
            self.filter_countdown -= 1;
            // A drive this small takes the clean path rather than a very slight clip.
            let drive = if drive < 0.001 { 0.0 } else { drive };
            match &mut self.coefficients {
                Some(ramp) => {
                    let c = ramp.now();
                    let filtered = [
                        self.filter[0].process(input[0], self.filter_type, drive, c),
                        self.filter[1].process(input[1], self.filter_type, drive, c),
                    ];
                    ramp.advance();
                    filtered
                }
                None => input,
            }
        };
        let filtered = self.trimmed(filtered, run);
        let filtered = self.through_slots(filtered, run, sidechains);
        self.sidechain = filtered;
        // What's moved by modulation keeps up, and a change on its own doesn't jump.
        let keeping_up = |target| self.smoother_for(target, run);
        let (vol, to_a, to_b) = (
            keeping_up(SettingId::VOL),
            keeping_up(SettingId::SEND_A),
            keeping_up(SettingId::SEND_B),
        );
        let gain = self.gain.next(vol) * self.level.next(run.smoothing);
        let (left, right) = self.pan_gains(position);
        let dry = [filtered[0] * gain * left, filtered[1] * gain * right];
        let send_a = self.send_a_gain.next(to_a);
        let send_b = self.send_b_gain.next(to_b);
        ShapedFrame {
            dry,
            send_a: [dry[0] * send_a, dry[1] * send_a],
            send_b: [dry[0] * send_b, dry[1] * send_b],
        }
    }

    /// Keep the highest level of each side right after the trim, for the Output device's meter.
    /// It's a comparison a side, and a sample that isn't a number leaves it as it was.
    #[inline]
    fn take_in_after_trim(&mut self, frame: [f32; 2]) {
        for (highest, sample) in self.after_trim.iter_mut().zip(frame) {
            *highest = highest.max(sample.abs());
        }
    }

    /// Run a frame through the slots in order. Whether its two sides are the same is looked at
    /// once, and each slot passes it on to the next, so a chain of effects that keep a mono sound
    /// mono works out one side all the way through.
    #[inline]
    fn through_slots(
        &mut self,
        mut frame: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> [f32; 2] {
        let mut same = same_sides(frame);
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            if let Some(passed) = insert.passed_through(frame, same) {
                (frame, same) = passed;
                continue;
            }
            // A setting being modulated keeps up, and a change on its own doesn't jump.
            let keeping_up = run.smoother(self.slot_moving[slot]);
            (frame, same) = self.slot_times.time(slot, !insert.is_empty(), || {
                insert.process_sides(frame, same, keeping_up, run.rate, sidechains)
            });
        }
        frame
    }

    /// Run a frame through the trim, slots and level of a track with no voice of its own.
    #[inline]
    fn slots_and_level(
        &mut self,
        input: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> [f32; 2] {
        let trim = self.input_trim.next(run.smoothing);
        let frame = [input[0] * trim, input[1] * trim];
        self.take_in_after_trim(frame);
        let frame = self.through_slots(frame, run, sidechains);
        self.sidechain = frame;
        let vol = self.smoother_for(SettingId::VOL, run);
        let gain = self.gain.next(vol) * self.level.next(run.smoothing);
        [frame[0] * gain, frame[1] * gain]
    }

    /// What a return adds to the mix for what's sent to it: its trim, slots, level and pan, then
    /// whether it's heard, faded so muting doesn't click. With nothing going in and nothing left in
    /// its slots, it does no work.
    #[inline]
    pub(crate) fn process_return(
        &mut self,
        input: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> [f32; 2] {
        if input == [0.0, 0.0] && !self.ringing(run.rate) {
            self.sidechain = [0.0, 0.0];
            // With nothing to fade, a mute is heard at once when something comes in.
            self.heard.settle();
            return [0.0, 0.0];
        }
        let frame = self.slots_and_level(input, run, sidechains);
        let position = (self.pan.next(run.smoothing)
            + self.pan_offset.next(run.fast)
            + self.pan_envelope.next(run.fast))
        .clamp(0.0, 1.0);
        // At the centre, what the effect gave comes through untouched, where `pan_gains` is only close.
        #[allow(
            clippy::float_cmp,
            reason = "only the exact centre leaves the sides untouched"
        )]
        let (left, right) = if position == 0.5 {
            (1.0, 1.0)
        } else {
            self.pan_gains(position)
        };
        let heard = self.heard.next(run.smoothing);
        let out = [frame[0] * left * heard, frame[1] * right * heard];
        self.reading.add(out, out);
        out
    }

    /// What a group adds to the mix for the sum of its tracks, as a return does, and what it sends
    /// to the returns from there, after its level and pan.
    #[inline]
    pub(crate) fn process_group(
        &mut self,
        input: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> ShapedFrame {
        let dry = self.process_return(input, run, sidechains);
        let keeping_up = |target| self.smoother_for(target, run);
        let (to_a, to_b) = (keeping_up(SettingId::SEND_A), keeping_up(SettingId::SEND_B));
        let send_a = self.send_a_gain.next(to_a);
        let send_b = self.send_b_gain.next(to_b);
        ShapedFrame {
            dry,
            send_a: [dry[0] * send_a, dry[1] * send_a],
            send_b: [dry[0] * send_b, dry[1] * send_b],
        }
    }

    /// Whether a group with nothing coming in would give silence and change nothing, frame after
    /// frame, so it can be skipped until something comes in: its slots aren't ringing, so it does
    /// no work, and what it sends isn't moving.
    pub(crate) fn group_rests(&self, run: RunContext) -> bool {
        let keeping_up = |target| self.smoother_for(target, run);
        !self.ringing(run.rate)
            && self.send_a_gain.still(keeping_up(SettingId::SEND_A))
            && self.send_b_gain.still(keeping_up(SettingId::SEND_B))
    }

    /// What a group or return with nothing coming in and nothing ringing does for a run of frames,
    /// done once: it lets its mute's fade go and gives nothing to a compressor that hears it.
    pub(crate) fn rest_bus(&mut self) {
        self.sidechain = [0.0, 0.0];
        self.heard.settle();
    }

    /// What the master gives for silence coming in, if it would give the same and change nothing
    /// frame after frame: its trim and level aren't moving, and each of its slots has settled
    /// (`InsertState::settled`) on what the one before gives, hearing only what other tracks give
    /// now. It's usually silence, but a filter can be left holding a value too small to hear for
    /// good, and then that's what it gives.
    pub(crate) fn master_rests(
        &self,
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> Option<MasterRest> {
        let vol = self.smoother_for(SettingId::VOL, run);
        if !self.input_trim.still(run.smoothing)
            || !self.gain.still(vol)
            || !self.level.still(run.smoothing)
        {
            return None;
        }
        let trimmed = [0.0 * self.input_trim.value; 2];
        let mut frame = trimmed;
        let mut same = same_sides(frame);
        for (slot, insert) in self.inserts.iter().enumerate() {
            let keeping_up = run.smoother(self.slot_moving[slot]);
            (frame, same) = insert.settled(frame, same, keeping_up, run.rate, sidechains)?;
        }
        let gain = self.gain.value * self.level.value;
        Some(MasterRest {
            trimmed,
            sidechain: frame,
            out: [frame[0] * gain, frame[1] * gain],
        })
    }

    /// What the master, once it rests, does for a run of `frames`, done once.
    pub(crate) fn rest_master(&mut self, rest: &MasterRest, frames: u32) {
        self.take_in_after_trim(rest.trimmed);
        self.sidechain = rest.sidechain;
        for insert in &mut self.inserts {
            insert.rested(frames);
        }
    }

    /// The master's slots and level, for the whole mix.
    #[inline]
    pub(crate) fn process_master(
        &mut self,
        input: [f32; 2],
        run: RunContext,
        sidechains: &SidechainFrames,
    ) -> [f32; 2] {
        self.slots_and_level(input, run, sidechains)
    }
}
