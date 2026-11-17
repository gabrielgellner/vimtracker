//! What a track is playing with now, and its faders as they're heard, for the screens and the
//! Roto to show.

use vt_model::constants::UNITY_LEVEL;
use vt_model::insert::{INSERTS, MAX_PARAMS};
use vt_model::macros::MACROS;
use vt_model::modulation::MOD_SLOTS;
use vt_model::settings::{FADER_COUNT, FADERS, MAX_MACHINE_SETTINGS, SettingId};
use vt_model::steps::STEPS;

use super::TrackState;
use super::values::scaled;

/// What a track is playing with right now, after step commands and modulation, on the same `00` to
/// `FF` scale as its settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveValues {
    pub level: u8,
    pub pan: u8,
    pub cutoff: u8,
    pub resonance: u8,
    pub send_a: u8,
    pub send_b: u8,
    /// The machine's settings, in its order.
    pub machine: [u8; MAX_MACHINE_SETTINGS],
    /// What each slot's settings are playing with, in the effect's order.
    pub inserts: [[u8; MAX_PARAMS]; INSERTS],
    /// How far each slot's compressor or limiter is turning down, in dB, for a meter.
    pub reduction: [f32; INSERTS],
    /// How far each slot's low-pass gate is open, its level and cutoff from 0 to 1, for the FX
    /// page's zoom to draw its curve: none for any other effect.
    pub openings: [Option<(f32, f32)>; INSERTS],
    /// The note the slots follow, with the track's `Pitch`, as a MIDI note: middle C before the
    /// track's first, for the zoom to draw a resonator's, comb's or vowel filter's curve at.
    pub note: f32,
    /// Whether each slot is switched out as it's heard, a step's lock of it included.
    pub bypassed: [bool; INSERTS],
    /// The highest level of each side since the last report, right after the trim and before the
    /// filter, as a gain: what the Output device's meter shows, so the trim can be set by eye.
    /// The report fills it in from `Engine::take_after_trim`.
    pub after_trim: [f32; 2],
    /// Where each mod slot that's an LFO is in its wave now, or a Track slot's last note on its
    /// map, for one that's running: none while the song is stopped, before a Track slot's first
    /// note, or for one with nowhere to go, no depth, or switched out once it has faded.
    pub mods: [Option<LfoNow>; MOD_SLOTS],
    /// Which step each mod slot that's Steps is playing now, counted from 0, for the Mod page to
    /// light: none for one that isn't running, as an LFO's place is.
    pub steps: [Option<u8>; MOD_SLOTS],
    /// What each mod slot that's Steps has drawn with `Mutate` in place of its written values, as
    /// it's heard, for the Mod page to show and `Space g a` to keep: kept at `Mutate` 0%, which
    /// locks it.
    pub drawn: [Drawn; MOD_SLOTS],
    /// Where each macro is as it's heard, with a lock or modulation, from `0000` to `FF00`.
    pub macros: [u16; MACROS],
}

/// The values a Steps slot's `Mutate` has drawn in place of the written ones, and which steps they
/// are, a bit a step: a step drawn is heard at its value here until it's drawn again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Drawn {
    pub which: u16,
    pub values: [i16; STEPS],
}

impl Drawn {
    /// The value drawn for a step, if it's been drawn.
    pub fn value(&self, step: usize) -> Option<i16> {
        (step < STEPS && self.which & (1 << step) != 0).then(|| self.values[step])
    }
}

/// Written out, as an array as long as `4-op`'s settings has no `Default` of its own.
impl Default for LiveValues {
    fn default() -> Self {
        Self {
            level: 0,
            pan: 0,
            cutoff: 0,
            resonance: 0,
            send_a: 0,
            send_b: 0,
            machine: [0; MAX_MACHINE_SETTINGS],
            inserts: Default::default(),
            reduction: Default::default(),
            openings: Default::default(),
            note: 60.0,
            bypassed: Default::default(),
            after_trim: Default::default(),
            mods: Default::default(),
            steps: Default::default(),
            drawn: Default::default(),
            macros: Default::default(),
        }
    }
}

/// A track's faders as they're heard, `LEV`, `PAN`, `SNA` and `SNB` in [`FADERS`]' order, for the
/// mixer, the Output device and the Roto to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Faders {
    /// Where each is: a step's value while the steps drive it, or the track page's, with a coarse
    /// byte and a fine one.
    pub at: [u16; FADER_COUNT],
    /// What's heard with `VOL` and modulation on top, on the `00` to `FF` scale, for the thin
    /// mark beside a fader.
    pub moved: [u8; FADER_COUNT],
    /// Whether a step has set each since playing last started from the top, so the steps drive it.
    pub automated: [bool; FADER_COUNT],
    /// Whether a hand has taken each over from the steps, until it's given back.
    pub taken: [bool; FADER_COUNT],
}

impl Faders {
    /// Whether a fader follows the steps now: they drive it, and no hand has taken it over.
    pub fn follows_steps(&self, fader: usize) -> bool {
        self.automated[fader] && !self.taken[fader]
    }
}

/// One of a track's faders as the steps and a hand move it.
#[derive(Debug, Clone, Copy)]
pub(super) struct FaderRun {
    /// The track page's value, which a hand sets.
    pub(super) song: u16,
    /// Where it's heard, on the `00` to `FF` scale with any fine part as the fraction.
    pub(super) at: f32,
    /// Whether a step has set it since playing last started from the top.
    pub(super) automated: bool,
    /// The last value a step held it at since then, if one has: what's under a lock when it's
    /// given back, whatever the track page's value did meanwhile, as an edit replaces a hold.
    pub(super) hold: Option<u16>,
    /// Whether a hand has taken it over: the steps' values are kept underneath, unheard.
    pub(super) taken: bool,
}

impl FaderRun {
    pub(super) fn at(id: SettingId) -> Self {
        let value = u16::from(id.entry().default) << 8;
        Self {
            song: value,
            at: scaled(value),
            automated: false,
            hold: None,
            taken: false,
        }
    }
}

/// Where an LFO is in its wave and what the wave gives there, before its depth, for the Mod page's
/// moving dot, or where a Track slot's last note is on its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LfoNow {
    /// How far round its wave it is, its `Phase` included, from `0` to a whole turn at
    /// `u16::MAX + 1`.
    pub phase: u16,
    /// The wave's value there, from `-i16::MAX` at its bottom to `i16::MAX` at its top.
    pub value: i16,
}

impl LfoNow {
    /// Where a wave is from its turns, `cycles` with its phase added, and its value from -1 to 1.
    pub fn of(cycles: f64, value: f32) -> Self {
        let turn = f64::from(u16::MAX) + 1.0;
        Self {
            phase: (cycles.rem_euclid(1.0) * turn) as u32 as u16,
            value: (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        }
    }

    /// Where an attractor is in its picture: `across` it from -1 at the left to 1 at the right, as
    /// the phase, and what's heard of it, from -1 to 1, as the value.
    pub fn across(across: f32, value: f32) -> Self {
        Self {
            phase: ((across.clamp(-1.0, 1.0) + 1.0) / 2.0 * f32::from(u16::MAX)).round() as u16,
            value: (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        }
    }

    /// Where a Track slot's last note is on its map: `along` its source's span, from 0 to 1, as
    /// the phase, and the map's level there, from 0 to 1, as the value.
    pub fn along(along: f32, level: f32) -> Self {
        Self {
            phase: (along.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16,
            value: (level.clamp(0.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        }
    }
}

impl TrackState {
    /// Its faders as they're heard: where each is, and what's heard with `VOL` and modulation on
    /// top, which is shown against the fader's `Level`, as they both set how loud it is. The first
    /// envelope on `Vol` shapes notes, and isn't a movement of the level.
    pub(crate) fn faders(&self) -> Faders {
        let offset = |destination: SettingId| self.offsets[destination.index()];
        let on_level = |held: f32, destination| {
            (held + offset(destination) * 255.0)
                .clamp(0.0, 255.0)
                .round() as u8
        };
        let vol = self.vol + offset(SettingId::VOL) * 255.0 - f32::from(UNITY_LEVEL);
        let level = self.faders[0].at;
        Faders {
            at: std::array::from_fn(|fader| {
                let top = f32::from(FADERS[fader].entry().kind.top());
                (self.faders[fader].at.clamp(0.0, top) * 256.0).round() as u16
            }),
            moved: [
                (level + vol).clamp(0.0, 255.0).round() as u8,
                ((self.pan.target + offset(SettingId::PAN)).clamp(0.0, 1.0) * 255.0).round() as u8,
                on_level(self.send_a, SettingId::SEND_A),
                on_level(self.send_b, SettingId::SEND_B),
            ],
            automated: self.faders.map(|run| run.automated),
            taken: self.faders.map(|run| run.taken),
        }
    }

    /// What this track is playing with right now, its faders as [`TrackState::faders`] hears
    /// them.
    pub(crate) fn live(&self) -> LiveValues {
        let offset = |destination: SettingId| self.offsets[destination.index()];
        let moved = |held: f32, destination| {
            ((held + offset(destination)).clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let synth = self.synth_settings();
        let shown = |value: f32| (value * 255.0).round() as u8;
        let [level, pan, send_a, send_b] = self.faders().moved;
        LiveValues {
            level,
            pan,
            cutoff: moved(self.cutoff.target + self.key_track.value, SettingId::CUT),
            resonance: moved(self.resonance.target, SettingId::RES),
            send_a,
            send_b,
            machine: match self.loaded_voice {
                Some(_) => std::array::from_fn(|place| shown(synth.at(place))),
                None => std::array::from_fn(|place| {
                    SettingId::sampler(place).map_or(0, |id| self.sampler_value(id).round() as u8)
                }),
            },
            inserts: self.resolved.map(|insert| {
                let mut values = [0; MAX_PARAMS];
                for (param, value) in values.iter_mut().enumerate() {
                    *value = insert.value(param);
                }
                values
            }),
            reduction: std::array::from_fn(|slot| self.inserts[slot].reduction()),
            openings: std::array::from_fn(|slot| self.inserts[slot].opening()),
            note: self.inserts[0].note(),
            bypassed: self.bypassed,
            // Taken, as it's the highest since it last was, by `Engine::take_after_trim`, which the
            // report fills this in from.
            after_trim: [0.0; 2],
            mods: self.slots.map(|run| run.now),
            steps: self.slots.map(|run| run.playing),
            drawn: self.slots.map(|run| Drawn {
                which: run.drawn,
                values: run.drift,
            }),
            macros: std::array::from_fn(|at| {
                let value = self.macro_heard[at].unwrap_or(self.macro_held[at]);
                (value * 256.0).round().clamp(0.0, f32::from(0xFF00_u16)) as u16
            }),
        }
    }
}
