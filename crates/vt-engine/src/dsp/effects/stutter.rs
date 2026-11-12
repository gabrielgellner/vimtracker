//! A stutter: when a step switches `Trig` on, the next piece of what comes in, a note division
//! long, is taken and played again a number of times on the grid, as a roll or a glitch, each
//! time a little quieter if `Decay` says. The piece is first heard as it comes in, and recorded as
//! it is; each repeat then starts it again from the beginning.
//!
//! The splices are after sst-effects' Microgate (`voice-effects/delay/Microgate.h`): the piece
//! starts where what comes in crosses zero, and the way it crossed is kept, so each repeat starts
//! where what's playing crosses zero the same way, and what's heard goes on without a step.
//! Microgate is armed by a threshold and starts each repeat from where the last ended, so its
//! waits add up; here a step arms it, and each repeat is due a whole number of pieces from the
//! step, so it keeps to the grid however long each wait was. A wait never lasts longer than about
//! a period of 80 Hz: a sound with no crossing in that time, as a low bass or a held offset, is
//! spliced where it is. A short fade under every splice takes what isn't quite at zero.
//!
//! sst-effects - an open source library of audio effects built by Surge Synth Team.
//! Copyright 2018-2023, various authors, as described in the GitHub transaction log.
//!
//! sst-effects is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later).
//! The license is found in the "LICENSE" file in the root of this repository, or at
//! <https://www.gnu.org/licenses/gpl-3.0.en.html>.

use vt_model::constants::MIN_MILLIBPM;
use vt_model::insert::{HOLD_REPEATS, REPEAT_LENGTHS, Stutter as Settings, repeat_decay_db};
use vt_model::lfo::DIVISION_TICKS;

use crate::dsp::blocks::clip::within;
use crate::engine::sample_of_tick;

/// The longest it waits for a crossing, in seconds: a little over a period of 80 Hz.
const WAIT_SECONDS: f32 = 0.013;
/// How long each splice fades over, in seconds.
const FADE_SECONDS: f32 = 0.002;
/// The shortest piece, in seconds, so a `1/64` at a fast tempo still has room for its fades.
const SHORTEST_SECONDS: f32 = 0.005;
/// How far what's recorded can go, 18 dB over full scale. Nothing finite gets near it.
const HEADROOM: f32 = 8.0;

/// What a side is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// What comes in, as it comes.
    Live,
    /// The piece, this many frames in.
    Piece(usize),
}

/// Where a side is in a stutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    /// Passing what comes in.
    Idle,
    /// Triggered, and waiting for what comes in to cross zero, to start the piece there. What
    /// was playing carries on meanwhile.
    Waiting,
    /// Recording the piece, or repeating it.
    Repeating,
}

/// What's the same for both sides through a stutter: what was set when it was triggered.
#[derive(Debug, Clone, Copy)]
struct Taken {
    /// Frames since the trigger.
    since: usize,
    /// How long the piece is, in frames.
    length: usize,
    /// How many repeats after the first time, or none for as long as `Trig` is on.
    repeats: Option<u32>,
    /// How much each repeat is turned down from the last.
    step: f32,
}

/// One side's piece, and what it's playing.
#[derive(Debug, Clone)]
struct Side {
    piece: Vec<f32>,
    /// How much of the piece is recorded.
    taken: usize,
    taking: bool,
    run: Run,
    playing: Source,
    gain: f32,
    /// What it's fading out from, and how many frames of the fade are left.
    fading: Source,
    fading_gain: f32,
    fade_left: u32,
    /// The last sample that came in, and the last of what's playing, before its gain.
    last_in: f32,
    last_out: f32,
    /// Which way the piece starts across zero.
    rising: bool,
    /// How long it's waited for a crossing.
    waited: u32,
    /// Which time the piece is playing: the first, as it comes in, is `0`.
    pass: u32,
}

impl Side {
    fn new(frames: usize) -> Self {
        Self {
            piece: vec![0.0; frames],
            taken: 0,
            taking: false,
            run: Run::Idle,
            playing: Source::Live,
            gain: 1.0,
            fading: Source::Live,
            fading_gain: 1.0,
            fade_left: 0,
            last_in: 0.0,
            last_out: 0.0,
            rising: true,
            waited: 0,
            pass: 0,
        }
    }

    /// Back to passing what comes in. What's in the piece is left, as nothing reads it before
    /// it's recorded again.
    fn clear(&mut self) {
        let piece = std::mem::take(&mut self.piece);
        *self = Self {
            piece,
            ..Self::new(0)
        };
    }

    /// What a source is now, with `input` coming in.
    #[inline]
    fn value(&self, source: Source, input: f32) -> f32 {
        match source {
            Source::Live => input,
            Source::Piece(at) => self.piece.get(at).copied().unwrap_or(0.0),
        }
    }

    /// Start playing something else, fading out of what was playing.
    #[inline]
    fn splice(&mut self, to: Source, gain: f32, fade: u32) {
        self.fading = self.playing;
        self.fading_gain = self.gain;
        self.fade_left = fade;
        self.playing = to;
        self.gain = gain;
        self.waited = 0;
    }

    /// One frame in and out.
    #[inline]
    fn process(&mut self, input: f32, taken: &Taken, on: bool, wait: u32, fade: u32) -> f32 {
        if self.run == Run::Waiting {
            let crossed = (self.last_in < 0.0) != (input < 0.0);
            if crossed || self.waited >= wait {
                self.rising = !crossed || input >= 0.0;
                self.taken = 0;
                self.taking = true;
                self.pass = 0;
                self.run = Run::Repeating;
                if self.playing == Source::Live {
                    self.waited = 0;
                } else {
                    self.splice(Source::Live, 1.0, fade);
                }
            } else {
                self.waited += 1;
            }
        }
        if self.taking {
            if let Some(sample) = self.piece.get_mut(self.taken) {
                *sample = input;
                self.taken += 1;
            } else {
                self.taking = false;
            }
        }
        if self.run == Run::Repeating {
            let now = self.value(self.playing, input);
            let ending = taken.repeats.is_none() && !on;
            let due = ending || taken.since >= (self.pass as usize + 1) * taken.length;
            if due {
                let crossed = if self.rising {
                    self.last_out < 0.0 && now >= 0.0
                } else {
                    self.last_out >= 0.0 && now < 0.0
                };
                if crossed || self.waited >= wait {
                    let more = !ending && taken.repeats.is_none_or(|most| self.pass < most);
                    if more {
                        self.pass += 1;
                        self.splice(Source::Piece(0), self.gain * taken.step, fade);
                    } else {
                        self.run = Run::Idle;
                        self.taking = false;
                        self.splice(Source::Live, 1.0, fade);
                    }
                } else {
                    self.waited += 1;
                }
            }
        }
        let now = self.value(self.playing, input);
        let mut out = now * self.gain;
        if self.fade_left > 0 {
            let old = self.fade_left as f32 / fade as f32;
            out += (self.value(self.fading, input) * self.fading_gain - out) * old;
            self.fade_left -= 1;
            if let Source::Piece(at) = &mut self.fading {
                *at += 1;
            }
        }
        if let Source::Piece(at) = &mut self.playing {
            *at += 1;
        }
        self.last_in = input;
        self.last_out = now;
        out
    }

    /// Whether it's doing anything but passing what comes in.
    fn busy(&self) -> bool {
        self.run != Run::Idle || self.fade_left > 0
    }
}

/// The stereo stutter.
#[derive(Debug, Clone)]
pub struct Stutter {
    sides: [Side; 2],
    taken: Taken,
    /// What's set now, which a trigger takes.
    length: usize,
    repeats: u8,
    decay: u8,
    /// Whether `Trig` is on.
    on: bool,
    wait: u32,
    fade: u32,
    shortest: usize,
}

impl Stutter {
    /// A piece long enough for a beat at the slowest tempo, with room to wait and fade after it.
    pub fn new(rate: u32) -> Self {
        let frames = |seconds: f32| (seconds * rate as f32).ceil() as u32;
        let wait = frames(WAIT_SECONDS);
        let fade = frames(FADE_SECONDS).max(1);
        let longest = sample_of_tick(
            u64::from(DIVISION_TICKS[REPEAT_LENGTHS - 1]),
            rate,
            MIN_MILLIBPM,
        ) as usize;
        let room = longest + (wait + fade) as usize + 2;
        let mut stutter = Self {
            sides: [Side::new(room), Side::new(room)],
            taken: Taken {
                since: 0,
                length: longest,
                repeats: Some(0),
                step: 1.0,
            },
            length: longest,
            repeats: 0,
            decay: 0,
            on: false,
            wait,
            fade,
            shortest: frames(SHORTEST_SECONDS) as usize,
        };
        stutter.follow(Settings::DEFAULT, 1.0);
        stutter
    }

    /// The longest piece it can hold, in frames.
    pub fn longest(&self) -> usize {
        self.sides[0].piece.len() - (self.wait + self.fade) as usize - 2
    }

    /// Forget where it is, and pass what comes in.
    pub fn clear(&mut self) {
        for side in &mut self.sides {
            side.clear();
        }
        self.on = false;
    }

    /// Take the settings, at a tempo. `Trig` going on triggers it.
    pub fn follow(&mut self, settings: Settings, frames_per_tick: f64) {
        let at = usize::from(settings.length).min(REPEAT_LENGTHS - 1);
        let frames = (f64::from(DIVISION_TICKS[at]) * frames_per_tick).round() as usize;
        self.length = frames.clamp(self.shortest, self.longest());
        self.repeats = settings.repeats.min(HOLD_REPEATS);
        self.decay = settings.decay;
        let on = settings.trig != 0;
        if on && !self.on {
            self.strike();
        }
        self.on = on;
    }

    /// Start again: take the next piece of what comes in, at what's set now.
    pub fn strike(&mut self) {
        self.taken = Taken {
            since: 0,
            length: self.length,
            repeats: (self.repeats < HOLD_REPEATS).then_some(u32::from(self.repeats) + 1),
            step: 10_f32.powf(-repeat_decay_db(f32::from(self.decay)) / 20.0),
        };
        for side in &mut self.sides {
            side.run = Run::Waiting;
            side.waited = 0;
        }
    }

    /// Whether it's been triggered and has a piece to take, or is still playing one, so it has
    /// to keep going with nothing coming in.
    pub fn waking(&self) -> bool {
        self.sides.iter().any(Side::busy)
    }

    /// How long it can go with nothing coming in and still have something to play: none once
    /// it's passing what comes in, and its longest piece and a wait while it's stuttering.
    pub fn quiet_frames(&self) -> u32 {
        if self.waking() {
            self.sides[0].piece.len() as u32
        } else {
            0
        }
    }

    /// Feed one frame in and get what's heard out: what comes in while it's not stuttering, and
    /// the piece while it is.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut out = [0.0; 2];
        for (side, state) in self.sides.iter_mut().enumerate() {
            let sample = within(input[side], -HEADROOM, HEADROOM);
            out[side] = state.process(sample, &self.taken, self.on, self.wait, self.fade);
        }
        self.taken.since = self.taken.since.saturating_add(1);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    /// 120 BPM, as `sample_of_tick` has it: 6000 frames a step.
    const FRAMES_PER_TICK: f64 = 25.0;
    const SIXTEENTH: usize = 6000;

    fn made(settings: Settings) -> Stutter {
        let mut stutter = Stutter::new(RATE);
        stutter.follow(settings, FRAMES_PER_TICK);
        stutter
    }

    fn sine(hertz: f32, frame: usize) -> f32 {
        (frame as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.5
    }

    /// A sound that's different everywhere: a sine sweeping up.
    fn sweep(frame: usize) -> f32 {
        let t = frame as f32 / RATE as f32;
        (t * (200.0 + 400.0 * t) * std::f32::consts::TAU).sin() * 0.5
    }

    #[test]
    fn off_it_passes_what_comes_in() {
        let mut stutter = made(Settings::DEFAULT);
        for frame in 0..RATE as usize {
            let input = [sweep(frame), -sweep(frame)];
            assert_eq!(stutter.process(input), input);
        }
        assert!(!stutter.waking());
    }

    #[test]
    fn a_trigger_repeats_the_next_piece_on_the_grid_then_lets_the_sound_back() {
        let mut stutter = made(Settings::DEFAULT);
        let (wait, fade) = (stutter.wait as usize, stutter.fade as usize);
        stutter.strike();
        let mut splices = Vec::new();
        let out: Vec<f32> = (0..SIXTEENTH * 6)
            .map(|frame| {
                let pass = stutter.sides[0].pass;
                let out = stutter.process([sweep(frame); 2])[0];
                if stutter.sides[0].pass != pass {
                    splices.push(frame);
                }
                out
            })
            .collect();
        // The first sixteenth is the sound itself.
        for (frame, sample) in out.iter().enumerate().take(SIXTEENTH) {
            assert_eq!(*sample, sweep(frame), "{frame}");
        }
        // The piece starts where the sweep first crossed zero, and each of the three repeats
        // starts within a wait of its sixteenth and plays the piece from there.
        let start = (1..).find(|frame| sweep(*frame) < 0.0).unwrap();
        assert_eq!(splices.len(), 3);
        for (repeat, begun) in splices.into_iter().enumerate() {
            let due = SIXTEENTH * (repeat + 1);
            assert!((due..due + wait).contains(&begun), "{repeat}: {begun}");
            // It splices where the sound crosses zero going down, as the piece starts.
            assert!(out[begun - 1] >= 0.0 && sweep(start) < 0.0);
            for at in fade..SIXTEENTH - wait {
                assert!(
                    (out[begun + at] - sweep(start + at)).abs() < 1e-6,
                    "repeat {repeat}, {at}"
                );
            }
        }
        // Then the sound again.
        assert!(stutter.sides.iter().all(|side| side.run == Run::Idle));
        let back = SIXTEENTH * 4 + wait + fade + 2;
        for (frame, sample) in out.iter().enumerate().skip(back) {
            assert_eq!(*sample, sweep(frame), "{frame}");
        }
        assert!(!stutter.waking());
    }

    #[test]
    fn its_splices_are_where_the_sound_crosses_zero_the_same_way() {
        // A low sine whose crossings don't fall on the grid.
        let mut stutter = made(Settings {
            repeats: 3,
            ..Settings::DEFAULT
        });
        stutter.strike();
        let out: Vec<f32> = (0..SIXTEENTH * 5)
            .map(|frame| stutter.process([sine(97.0, frame + 40); 2])[0])
            .collect();
        // Up to the last repeat's end, where it fades back to the sound wherever that is, no
        // frame jumps further than the sine moves in a frame, give or take a little.
        let most = 0.5 * std::f32::consts::TAU * 97.0 / RATE as f32;
        for pair in out.windows(2) {
            assert!((pair[1] - pair[0]).abs() < most * 1.5, "{pair:?}");
        }
    }

    #[test]
    fn decay_turns_each_repeat_down_and_hold_repeats_while_trig_is_on() {
        let mut stutter = made(Settings {
            decay: 0x80,
            ..Settings::DEFAULT
        });
        stutter.strike();
        let out: Vec<f32> = (0..SIXTEENTH * 4)
            .map(|frame| stutter.process([sine(1000.0, frame); 2])[0])
            .collect();
        let loudest = |at: usize| {
            out[at * SIXTEENTH + 500..(at + 1) * SIXTEENTH]
                .iter()
                .fold(0.0_f32, |most, sample| most.max(sample.abs()))
        };
        let step = 10_f32.powf(-repeat_decay_db(128.0) / 20.0);
        for at in 1..4 {
            assert!((loudest(at) / loudest(at - 1) - step).abs() < 0.01, "{at}");
        }

        let mut held = made(Settings {
            trig: 1,
            repeats: HOLD_REPEATS,
            ..Settings::DEFAULT
        });
        for frame in 0..SIXTEENTH * 40 {
            held.process([sweep(frame); 2]);
        }
        assert!(held.sides.iter().all(|side| side.pass > 30));
        held.follow(
            Settings {
                trig: 0,
                repeats: HOLD_REPEATS,
                ..Settings::DEFAULT
            },
            FRAMES_PER_TICK,
        );
        for frame in 0..held.wait as usize + held.fade as usize + 2 {
            held.process([sweep(frame); 2]);
        }
        assert!(!held.waking());
    }

    #[test]
    fn trig_going_on_triggers_it_once_and_a_strike_starts_it_again() {
        let mut stutter = made(Settings::DEFAULT);
        let on = Settings {
            trig: 1,
            ..Settings::DEFAULT
        };
        stutter.follow(on, FRAMES_PER_TICK);
        assert!(stutter.waking());
        for frame in 0..SIXTEENTH * 5 {
            stutter.process([sweep(frame); 2]);
        }
        // Still on, it doesn't start again by itself.
        stutter.follow(on, FRAMES_PER_TICK);
        assert!(!stutter.waking());
        // A step's lock does, on or not.
        stutter.strike();
        assert!(stutter.waking());
        for frame in 0..SIXTEENTH {
            stutter.process([sweep(frame); 2]);
        }
        // And again part way, where the next piece starts from what comes in.
        stutter.strike();
        let out: Vec<f32> = (0..SIXTEENTH * 2)
            .map(|frame| stutter.process([sweep(frame + 1000); 2])[0])
            .collect();
        let fade = stutter.wait as usize + stutter.fade as usize + 2;
        for (frame, sample) in out.iter().enumerate().take(SIXTEENTH).skip(fade) {
            assert_eq!(*sample, sweep(frame + 1000), "{frame}");
        }
    }

    #[test]
    fn its_length_follows_the_tempo_and_holds_a_beat_at_the_slowest() {
        let mut stutter = made(Settings {
            length: 9,
            ..Settings::DEFAULT
        });
        stutter.strike();
        assert_eq!(stutter.taken.length, 480 * 25);
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let mut stutter = Stutter::new(rate);
            let slowest = sample_of_tick(960, rate, MIN_MILLIBPM) as usize;
            assert!(stutter.longest() >= slowest);
            // At a tempo slower still, or a length past the longest, it holds as much as it can.
            stutter.follow(
                Settings {
                    length: 0xFF,
                    ..Settings::DEFAULT
                },
                1e6,
            );
            assert_eq!(stutter.length, stutter.longest());
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for length in [0, REPEAT_LENGTHS as u8 - 1] {
            for repeats in [0, 15, HOLD_REPEATS] {
                for decay in [0x00, 0xFF] {
                    for frames_per_tick in [0.01, 25.0, 1e6] {
                        let mut stutter = made(Settings {
                            length,
                            repeats,
                            decay,
                            trig: 1,
                            ..Settings::DEFAULT
                        });
                        stutter.follow(
                            Settings {
                                length,
                                repeats,
                                decay,
                                trig: 1,
                                ..Settings::DEFAULT
                            },
                            frames_per_tick,
                        );
                        for frame in 0..RATE as usize * 2 {
                            // A square 12 dB over full scale, a NaN and an infinity in it, struck
                            // again and again.
                            let input = match frame {
                                1000 => f32::NAN,
                                2000 => f32::INFINITY,
                                _ if (frame / 50) % 2 == 0 => 4.0,
                                _ => -4.0,
                            };
                            if frame % 7919 == 0 {
                                stutter.strike();
                            }
                            let out = stutter.process([input, -input]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() <= HEADROOM),
                                "{length} {repeats} {decay} {frames_per_tick}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
