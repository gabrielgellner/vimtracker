//! The voices, which make a track's sound from its notes. Most are ported from Mutable Instruments'
//! Plaits, and share its blocks and band-limited steps through [`virtual_analog`]; the
//! `braids_` ones are from Braids, its older macro-oscillator, and run at twice the rate through
//! `braids_parts`. The tables they read are in `voices/tables/`. What each implements,
//! [`SynthEngine`], and the [`Settings`] it's given are here.

pub(crate) mod acid;
pub(crate) mod additive;
pub(crate) mod braids_analog;
pub(crate) mod braids_digital;
pub(crate) mod braids_filter_fm;
pub(crate) mod braids_noise;
mod braids_parts;
pub(crate) mod braids_physical;
pub(crate) mod braids_vocal;
pub(crate) mod braids_waves;
pub(crate) mod chiptune;
pub(crate) mod chord;
mod chord_bank;
pub(crate) mod drums;
pub(crate) mod fm;
pub(crate) mod four_op;
pub(crate) mod grain;
pub(crate) mod kick;
pub(crate) mod modal;
pub(crate) mod noise;
pub(crate) mod particle;
pub(crate) mod phase_distortion;
pub(crate) mod six_op;
pub(crate) mod speech;
pub(crate) mod string_model;
pub(crate) mod supersaw;
pub(crate) mod swarm;
mod variable_shape;
pub(crate) mod virtual_analog;
pub(crate) mod virtual_analog_vcf;
pub(crate) mod wave_terrain;
pub(crate) mod waveshaping;
mod waveshaping_tables;
pub(crate) mod wavetable;

use vt_model::settings::MAX_MACHINE_SETTINGS;

/// What a synth does to play a note on a voice. Each is plain data, held inline in a `Synth`, so
/// the audio thread never allocates for one and reaches it through a match, never `dyn`. What
/// most synths don't do has a default.
pub(crate) trait SynthEngine: Copy {
    /// Whether it drifts as an analog synth does, which a seed for each note gives it.
    const DRIFTS: bool = false;
    /// Whether it draws noise, which a seed for each note chooses.
    const NOISE: bool = false;

    /// A new note, at a sample rate.
    fn new(rate: u32) -> Self;

    /// The next frame, at a frequency as a fraction of the sample rate.
    fn next(&mut self, frequency: f32, settings: Settings) -> f32;

    /// The next frame, on both sides: the same on each unless it spreads them, as `Supersaw` does.
    #[inline]
    fn next_sides(&mut self, frequency: f32, settings: Settings) -> [f32; 2] {
        let sample = self.next(frequency, settings);
        [sample, sample]
    }

    /// Start a note, for a synth with envelopes of its own that an accent changes.
    fn trigger(&mut self, _accent: bool) {}

    /// Let go of the note, for a synth with envelopes of its own.
    fn release(&mut self) {}

    /// Whether nothing it carries from note to note is left, so a voice let go can stop. `Acid`'s
    /// accent sweep drains between notes, as a 303's does, and the next note carries on from it.
    fn settled(&self) -> bool {
        true
    }

    /// Roll a new note's drift, and its noise floor, from a seed, for a synth that drifts.
    /// `Drift` as the note starts says whether its oscillators start anywhere in their cycles.
    fn seed_drift(&mut self, _seed: u32, _settings: Settings) {}

    /// What `Drift` multiplies the note's envelope times by. `Acid`'s own envelopes take it
    /// themselves.
    fn time(&self, _settings: Settings) -> f32 {
        1.0
    }

    /// Choose the noise of a synth that draws any, so each hit differs but a render repeats.
    fn seed_noise(&mut self, _seed: u32) {}
}

/// A voice's settings, each on `0..=1`, in the order of its entries in `vt-model`'s
/// `settings/voices.rs`. A voice reads each with [`Settings::at`], at a constant
/// `voice_place` works out from the key the setting is stored under, so a key its entries don't
/// have doesn't build and their order can change without a voice reading the wrong one. The first
/// four have the names of Plaits' knobs, which the voices' tests set them by; what each is
/// depends on the voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// The first: `VA`'s detuning of its saw from its square, say.
    pub harmonics: f32,
    /// The second.
    pub timbre: f32,
    /// The third.
    pub morph: f32,
    /// The fourth: how hard a note is struck, for the voices with an `Accent`, and `Drift` for
    /// the analog ones. Plaits' default is 0.8.
    pub accent: f32,
    /// The settings after the first four, for a voice with more.
    pub further: [f32; MAX_MACHINE_SETTINGS - 4],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            harmonics: 0.5,
            timbre: 0.0,
            morph: 0.5,
            accent: 0.8,
            further: [0.0; MAX_MACHINE_SETTINGS - 4],
        }
    }
}

impl Settings {
    /// A setting by its place among the voice's.
    #[inline]
    pub(crate) fn at(&self, place: usize) -> f32 {
        match place {
            0 => self.harmonics,
            1 => self.timbre,
            2 => self.morph,
            3 => self.accent,
            _ => self.further.get(place - 4).copied().unwrap_or(0.0),
        }
    }

    /// The settings in their places, as the track holds them.
    pub(crate) fn from_places(places: [f32; MAX_MACHINE_SETTINGS]) -> Self {
        let [harmonics, timbre, morph, accent, further @ ..] = places;
        Self {
            harmonics,
            timbre,
            morph,
            accent,
            further,
        }
    }
}
