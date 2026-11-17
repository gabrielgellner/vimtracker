//! What each mod type is and which settings it has, written once in [`MOD_SPECS`], and a slot's
//! settings read and set by what they are rather than where its type keeps them. This is the one
//! place that knows an ADSR's `Attack` is `adsr.attack_ms` and an AHD's is `ahd.attack_ms`.

use std::borrow::Cow;

use crate::constants::MAX_ENVELOPE_MS;
use crate::envelope::{Curve, Hold, MAX_HOLD_MS, MAX_RATCHET};
use crate::grid::Grid;
use crate::lfo::{Fade, LfoMode, MAX_DEFORM, Shape};
use crate::listed::Listed;
use crate::param::{Choice, Descriptor, HEX_JUMP, NUMBER, PERCENT, Pace, Reading, Reads, Typing};
use crate::settings::{MOD_OWN_SETTINGS, SettingId};
use crate::shape::{BEND_JUMP, BEND_NUDGE, MAX_BIAS, MAX_SPREAD};
use crate::steps::{Loop, MAX_JUMP, MAX_MUTATE, MAX_STEP, STEPS, StepsMode};
use crate::tracking::Source;

use super::{MAX_DEPTH, ModKind, ModSlot, Polarity, default_mod, depth_text};

/// How far a small and a large step move a phase, in degrees, and an attractor's large one, from
/// one axis to the next: `x` at 0°, `y` at 120° and `z` at 240°.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
const AXIS_JUMP: i32 = 120;
/// How many small steps a percentage's large one is, and an octave's, of semitones.
const PERCENT_JUMP: i32 = 10;
const OCTAVE: i32 = 12;

/// One setting of a mod slot: its type, its mode, where it goes and how far, which every type has
/// in its own way, then each type's own, which mean what they do under the slot's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModField {
    Type,
    /// The type's mode, as an envelope's curve or an LFO's `Free`, `Trig`, `Hold`, `One` or
    /// `Half`, for a type that has one.
    Mode,
    /// Whether it moves what it reaches one way or both.
    Polarity,
    Destination,
    Depth,
    /// An envelope's: an ADSR's and an AHD's attack, decay and reset, an ADSR's sustain and
    /// release, and an AHD's hold.
    Attack,
    Decay,
    Sustain,
    Release,
    Reset,
    Hold,
    /// A drum envelope's: how long its peak takes, the level it falls to, and how many times it
    /// strikes. Its `Decay` is the body's.
    Peak,
    Body,
    Ratchet,
    /// A low-pass gate's: how far its fall closes the filter. Its `Attack`, `Decay` and `Reset`
    /// are its own.
    Colour,
    /// An LFO's. `Rate` is Steps' too.
    Shape,
    Rate,
    Phase,
    Deform,
    Fade,
    /// Steps': how many it plays, where each note starts it, and what it does at the end, which
    /// is in its title.
    Length,
    Start,
    Loop,
    /// Steps': the chance each value is drawn again as it comes round, the chance each step it
    /// moves on to is one at random, and how it goes from one to the next.
    Mutate,
    Jump,
    /// Steps': how widely the values `Mutate` draws spread and which way they lean, as Marbles'
    /// X section's (decision 403).
    Spread,
    Bias,
    Smooth,
    /// One of Steps' values, by its place, counted from 0: what the cursor is on in its panel.
    Step(u8),
    /// Track's: where its map starts and ends, in its source's values, a note or hex.
    Low,
    High,
}

/// A slot's type's own settings that macros and modulation move, in the registry's order
/// ([`SettingId::mod_own`]): an ADSR's, an AHD's and an LPG's `Attack` are one. This is the only
/// place that order is written; the registry's entries follow it.
pub const MOD_OWN_FIELDS: [ModField; MOD_OWN_SETTINGS] = [
    ModField::Attack,
    ModField::Decay,
    ModField::Sustain,
    ModField::Release,
    ModField::Body,
    ModField::Colour,
    ModField::Length,
    ModField::Mutate,
    ModField::Smooth,
    ModField::Rate,
    ModField::Jump,
    ModField::Spread,
    ModField::Bias,
];

/// A slot's settings that steps can lock, whatever its type, in the registry's order
/// ([`SettingId::modulator`]).
pub const MOD_LOCKED_FIELDS: [ModField; 4] = [
    ModField::Depth,
    ModField::Phase,
    ModField::Polarity,
    ModField::Deform,
];

impl ModField {
    /// Whether a slot of this type has it: its spec has it in its title or its grid. Every one of
    /// Steps' values is there, by the first's place in its panel.
    pub fn on(self, kind: ModKind) -> bool {
        let spec = kind.spec();
        let field = match self {
            Self::Step(_) => Self::Step(0),
            field => field,
        };
        spec.title.contains(&field) || spec.fields.contains(&field)
    }

    /// The name on screen. A mode's is its type's, [`ModKind::mode_label`].
    pub fn name(self) -> &'static str {
        match self {
            Self::Type => "Type",
            Self::Mode => "Mode",
            Self::Polarity => "Polarity",
            Self::Destination => "Dest",
            Self::Depth => "Depth",
            Self::Attack => "Attack",
            Self::Decay => "Decay",
            Self::Sustain => "Sustain",
            Self::Release => "Release",
            Self::Reset => "Reset",
            Self::Hold => "Hold",
            Self::Peak => "Peak",
            Self::Body => "Body",
            Self::Ratchet => "Ratchet",
            Self::Colour => "Colour",
            Self::Shape => "Shape",
            Self::Rate => "Rate",
            Self::Phase => "Phase",
            Self::Deform => "Deform",
            Self::Fade => "Fade",
            Self::Length => "Length",
            Self::Start => "Start",
            Self::Loop => "Loop",
            Self::Mutate => "Mutate",
            Self::Jump => "Jump",
            Self::Spread => "Spread",
            Self::Bias => "Bias",
            Self::Smooth => "Smooth",
            Self::Step(_) => "Step",
            Self::Low => "Low",
            Self::High => "High",
        }
    }

    /// What it does under a slot's type, in a few plain words starting in lower case, for the line
    /// that describes the setting under the cursor ([`Descriptor::meaning`]).
    #[allow(
        clippy::match_same_arms,
        reason = "a meaning for each setting, in the slot's order"
    )]
    pub fn meaning(self, kind: ModKind) -> &'static str {
        let envelope = kind.is_envelope();
        match self {
            Self::Type => "what kind of modulator the slot is",
            Self::Mode if envelope => "how the envelope's rise and fall bend",
            Self::Mode if kind == ModKind::Lfo => "how notes start the cycle, or leave it running",
            Self::Mode if kind == ModKind::Steps => {
                "whether its values go by in time or one a note"
            }
            Self::Mode => "what of each note it follows",
            Self::Polarity => "whether it moves what it reaches one way or both ways",
            Self::Destination => "the setting it moves",
            Self::Depth => "how far it moves its setting at most, a minus turning it over",
            Self::Attack if kind == ModKind::Lpg => "how long the gate takes to open",
            Self::Attack => "how long it takes to rise to full",
            Self::Decay if kind == ModKind::Drum => "how long its body takes to die away",
            Self::Decay if kind == ModKind::Lpg => "how long the gate takes to close again",
            Self::Decay if kind == ModKind::Adsr => "how long it takes to fall to its sustain",
            Self::Decay => "how long it takes to fall back once it's held",
            Self::Sustain => "the level it holds at while the note does",
            Self::Release => "how long it takes to fall away once the note ends",
            Self::Reset => "whether each note starts it from nothing rather than where it is",
            Self::Hold => "how long it stays at full, or for as long as the note",
            Self::Peak => "how long its first strike lasts before the body",
            Self::Body => "the level the strike falls to before it decays",
            Self::Ratchet => "how many times it strikes on each note",
            Self::Colour => "how far the gate's fall closes the filter as well as the level",
            Self::Shape => "the wave it cycles through",
            Self::Rate if kind == ModKind::Steps => "how long each value lasts",
            Self::Rate => "how fast it cycles",
            Self::Phase => "where in its cycle a note starts it",
            Self::Deform => "how its wave bends away from its plain shape",
            Self::Fade => "how it fades in or out after each note starts it",
            Self::Length => "how many of its values it plays",
            Self::Start => "the value each note starts it on",
            Self::Loop => "what it does once it reaches its last value",
            Self::Mutate => "the chance each value is drawn again as it comes round",
            Self::Jump => "the chance each step goes to one of its values at random",
            Self::Spread => "how far the values Mutate draws stray, from one value to the two ends",
            Self::Bias => "which way the values Mutate draws lean, low or high",
            Self::Smooth => "how it goes from one value to the next, gliding or jumping",
            Self::Step(_) => "this value, how far it moves its setting while it plays",
            Self::Low => "where its map starts, in what it follows",
            Self::High => "where its map ends, in what it follows",
        }
    }

    /// Its place among a slot's type's own settings in the registry, if macros and modulation
    /// move it: an index of [`MOD_OWN_FIELDS`].
    pub fn own(self) -> Option<usize> {
        MOD_OWN_FIELDS.iter().position(|field| *field == self)
    }

    /// Its place among a slot's settings that steps can lock, if it's one: an index of
    /// [`MOD_LOCKED_FIELDS`].
    pub fn locked(self) -> Option<usize> {
        MOD_LOCKED_FIELDS.iter().position(|field| *field == self)
    }

    /// The registry's setting for it on a slot, if it has one, whose entry describes it on the
    /// `00` to `FF` scale. A type's own is on the slot only while its type has it.
    pub fn setting(self, slot: usize) -> Option<SettingId> {
        match (self.locked(), self.own()) {
            (Some(at), _) => SettingId::modulator(slot, at),
            (None, Some(at)) => SettingId::mod_own(slot, at),
            (None, None) => None,
        }
    }
}

/// One mod type described: its name and what it is, and which settings it has and where they sit
/// on its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModSpec {
    pub kind: ModKind,
    /// The name in the slot's title.
    pub name: &'static str,
    /// What it does, for the type picker.
    pub meaning: &'static str,
    /// Whether it's an envelope, which each note starts and which can shape the notes themselves.
    pub envelope: bool,
    /// What its mode is called, as the status line and the Roto name it.
    pub mode_label: &'static str,
    /// Which way it moves what it reaches unless its slot says otherwise.
    pub polarity: Polarity,
    /// What its title holds after its switch, in the order `h` and `l` go through them: its type,
    /// its mode if it has one, Steps' loop, its polarity if it has one to choose, and where it
    /// goes.
    pub title: &'static [ModField],
    /// Its settings under the title, which its grid places by their index here.
    pub fields: &'static [ModField],
    pub grid: Grid,
}

/// A title with a mode and a polarity, as most types have.
const TITLE: [ModField; 4] = [
    ModField::Type,
    ModField::Mode,
    ModField::Polarity,
    ModField::Destination,
];

/// Two columns of a grid, the right one the half-width device's last.
const fn two([left, right]: [[Option<usize>; 3]; 2]) -> Grid {
    Grid::new([left, right, [None; 3], [None; 3]])
}

/// Every type's description, in [`ModKind::ALL`]'s order.
pub const MOD_SPECS: [ModSpec; ModKind::ALL.len()] = [
    // Its times down the left, and its levels down the right.
    ModSpec {
        kind: ModKind::Adsr,
        name: "ADSR",
        meaning: "held while the note is",
        envelope: true,
        mode_label: "Curve",
        polarity: Polarity::Uni,
        title: &TITLE,
        fields: &[
            ModField::Attack,
            ModField::Decay,
            ModField::Release,
            ModField::Sustain,
            ModField::Depth,
            ModField::Reset,
        ],
        grid: two([[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]]),
    },
    // Its times down the left, and how far it goes and its reset down the right, with a hole left
    // for a `Delay` before the attack, Vital's DAHD, if one is missed.
    ModSpec {
        kind: ModKind::Ahd,
        name: "AHD",
        meaning: "a hold at full, for a time or the note",
        envelope: true,
        mode_label: "Curve",
        polarity: Polarity::Uni,
        title: &TITLE,
        fields: &[
            ModField::Attack,
            ModField::Hold,
            ModField::Decay,
            ModField::Depth,
            ModField::Reset,
        ],
        grid: two([[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]]),
    },
    // Its peak, body and decay down the left, and how far it goes and how many times its peak
    // strikes down the right, with a hole. It has no attack or reset, as it always strikes from
    // full.
    ModSpec {
        kind: ModKind::Drum,
        name: "Drum",
        meaning: "a fast peak onto a body that decays",
        envelope: true,
        mode_label: "Curve",
        polarity: Polarity::Uni,
        title: &TITLE,
        fields: &[
            ModField::Peak,
            ModField::Body,
            ModField::Decay,
            ModField::Depth,
            ModField::Ratchet,
        ],
        grid: two([[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]]),
    },
    // Its rise and fall and how far it closes the filter down the left, and how far it goes and its
    // reset down the right, with a hole. It has no mode, as its curve is the point of it.
    ModSpec {
        kind: ModKind::Lpg,
        name: "LPG",
        meaning: "a vactrol's fall, closing the filter",
        envelope: true,
        mode_label: "Curve",
        polarity: Polarity::Uni,
        title: &[ModField::Type, ModField::Polarity, ModField::Destination],
        fields: &[
            ModField::Attack,
            ModField::Decay,
            ModField::Colour,
            ModField::Depth,
            ModField::Reset,
        ],
        grid: two([[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]]),
    },
    // What it plays down the left, and where it starts, how it fades after each note and how its
    // wave bends down the right. How notes start it is its mode, in the title.
    ModSpec {
        kind: ModKind::Lfo,
        name: "LFO",
        meaning: "a cycle, free or from each note",
        envelope: false,
        mode_label: "Mode",
        polarity: Polarity::Bi,
        title: &TITLE,
        fields: &[
            ModField::Shape,
            ModField::Rate,
            ModField::Depth,
            ModField::Phase,
            ModField::Fade,
            ModField::Deform,
        ],
        grid: two([[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]]),
    },
    // How fast, how many and how far down the left, and where each note starts it, how it drifts
    // and how it goes from step to step down the right; then its values, a cursor stop of its own
    // in the panel beside them, on every row. What it does at its end is in its title, with its
    // mode. Its values are signed, so they say which way themselves, and it has no polarity.
    ModSpec {
        kind: ModKind::Steps,
        name: "Steps",
        meaning: "up to 16 values, in time or a note each",
        envelope: false,
        mode_label: "Mode",
        polarity: Polarity::Bi,
        title: &[
            ModField::Type,
            ModField::Mode,
            ModField::Loop,
            ModField::Jump,
            ModField::Spread,
            ModField::Bias,
            ModField::Destination,
        ],
        fields: &[
            ModField::Rate,
            ModField::Length,
            ModField::Depth,
            ModField::Start,
            ModField::Mutate,
            ModField::Smooth,
            ModField::Step(0),
        ],
        grid: Grid::new([
            [Some(0), Some(1), Some(2)],
            [Some(3), Some(4), Some(5)],
            [Some(6), Some(6), Some(6)],
            [None; 3],
        ]),
    },
    // Where its map starts and ends and how far it goes down the left, with the right left empty.
    // What it follows is its mode, in the title, and its map is drawn beside them. It rises from
    // the setting, as a velocity of none moves nothing.
    ModSpec {
        kind: ModKind::Track,
        name: "Track",
        meaning: "the note, its velocity or a roll, each note",
        envelope: false,
        mode_label: "Source",
        polarity: Polarity::Uni,
        title: &TITLE,
        fields: &[ModField::Low, ModField::High, ModField::Depth],
        grid: two([[Some(0), Some(1), Some(2)], [None; 3]]),
    },
];

impl ModKind {
    /// What its mode can be, by name and meaning, in the order [`ModSlot::field`] counts them: an
    /// envelope's curves, an LFO's and Steps' modes, and Track's sources. None for a type without
    /// one.
    pub fn modes(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::Adsr | Self::Ahd | Self::Drum => named::<Curve>().collect(),
            Self::Lfo => named::<LfoMode>().collect(),
            Self::Steps => named::<StepsMode>().collect(),
            Self::Track => named::<Source>().collect(),
            Self::Lpg => Vec::new(),
        }
    }
}

/// The names and meanings of every one of `X`.
fn named<X: Listed>() -> impl Iterator<Item = (&'static str, &'static str)> {
    X::ALL.iter().map(|each| (each.name(), each.meaning()))
}

/// A count as a setting's value, which every count here fits.
fn whole(count: usize) -> i32 {
    count as i32
}

/// Where a value is in a list, as a setting's value.
fn place<T: PartialEq>(items: &[T], value: &T) -> Option<i32> {
    items.iter().position(|item| item == value).map(whole)
}

impl ModSlot {
    /// The lowest and highest one of its settings can be, as [`ModSlot::field`] gives it, if its
    /// type has the setting and it's one number. `Phase` goes round past either end.
    pub fn bounds(&self, field: ModField) -> Option<(i32, i32)> {
        if !field.on(self.kind) {
            return None;
        }
        let time = (0, i32::from(MAX_ENVELOPE_MS));
        let signed = |most: i16| (-i32::from(most), i32::from(most));
        let list = |count: usize| (0, whole(count) - 1);
        Some(match field {
            ModField::Type => list(ModKind::ALL.len()),
            ModField::Mode => list(match self.kind {
                ModKind::Adsr | ModKind::Ahd | ModKind::Drum => Curve::ALL.len(),
                ModKind::Lfo => LfoMode::ALL.len(),
                ModKind::Steps => StepsMode::ALL.len(),
                ModKind::Track => Source::ALL.len(),
                ModKind::Lpg => return None,
            }),
            ModField::Polarity | ModField::Reset => (0, 1),
            ModField::Depth => signed(MAX_DEPTH),
            ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => time,
            // `Note` is below no time, as `-` from 0 ms holds for the note.
            ModField::Hold => (-1, i32::from(MAX_HOLD_MS)),
            ModField::Sustain | ModField::Body => (0, 100),
            ModField::Ratchet => (1, i32::from(MAX_RATCHET)),
            ModField::Colour => (0, 0xFF),
            ModField::Shape => list(Shape::ALL.len()),
            ModField::Phase => (0, 359),
            ModField::Deform => signed(MAX_DEFORM),
            ModField::Fade => list(Fade::ALL.len()),
            ModField::Length => (1, whole(STEPS)),
            ModField::Start => list(STEPS),
            ModField::Loop => list(Loop::ALL.len()),
            ModField::Mutate => (0, i32::from(MAX_MUTATE)),
            ModField::Jump => (0, i32::from(MAX_JUMP)),
            ModField::Spread => (0, i32::from(MAX_SPREAD)),
            ModField::Bias => (-i32::from(MAX_BIAS), i32::from(MAX_BIAS)),
            ModField::Smooth => signed(MAX_STEP),
            ModField::Step(at) if usize::from(at) < STEPS => signed(MAX_STEP),
            ModField::Low | ModField::High => {
                let (lowest, highest) = self.track.source.span();
                (i32::from(lowest), i32::from(highest))
            }
            // A rate is a kind of rate, a division or hertz, and a destination a setting: neither
            // is one number, so they're read as the slot keeps them.
            ModField::Rate | ModField::Destination | ModField::Step(_) => return None,
        })
    }

    /// One of its settings as it keeps it, under its type: a time in ms, a percentage, degrees, a
    /// count, a signed amount, a note or hex for Track's map, 0 or 1 for a switch, `-1` for an
    /// AHD's hold for the note, and a choice by its place in its list (`Type` in
    /// [`ModKind::ALL`], an envelope's mode in [`Curve::ALL`], and so on). None if its type hasn't
    /// it, or it isn't one number (see [`ModSlot::bounds`]).
    pub fn field(&self, field: ModField) -> Option<i32> {
        self.bounds(field)?;
        let (adsr, ahd, drum, lpg, lfo, steps) = (
            &self.adsr,
            &self.ahd,
            &self.drum,
            &self.lpg,
            &self.lfo,
            &self.steps,
        );
        let kind = self.kind;
        Some(match field {
            ModField::Type => place(&ModKind::ALL, &kind)?,
            ModField::Mode => match kind {
                ModKind::Adsr => place(&Curve::ALL, &adsr.curve)?,
                ModKind::Ahd => place(&Curve::ALL, &ahd.curve)?,
                ModKind::Drum => place(&Curve::ALL, &drum.curve)?,
                ModKind::Lfo => place(&LfoMode::ALL, &lfo.mode)?,
                ModKind::Steps => place(&StepsMode::ALL, &steps.mode)?,
                ModKind::Track => place(&Source::ALL, &self.track.source)?,
                ModKind::Lpg => return None,
            },
            ModField::Polarity => i32::from(self.polarity().index()),
            ModField::Depth => i32::from(self.depth),
            ModField::Attack => i32::from(match kind {
                ModKind::Ahd => ahd.attack_ms,
                ModKind::Lpg => lpg.attack_ms,
                _ => adsr.attack_ms,
            }),
            ModField::Decay => i32::from(match kind {
                ModKind::Ahd => ahd.decay_ms,
                ModKind::Drum => drum.decay_ms,
                ModKind::Lpg => lpg.decay_ms,
                _ => adsr.decay_ms,
            }),
            ModField::Reset => i32::from(match kind {
                ModKind::Ahd => ahd.reset,
                ModKind::Lpg => lpg.reset,
                _ => adsr.reset,
            }),
            ModField::Sustain => i32::from(adsr.sustain),
            ModField::Release => i32::from(adsr.release_ms),
            ModField::Hold => match ahd.hold {
                Hold::Note => -1,
                Hold::Ms(ms) => i32::from(ms),
            },
            ModField::Peak => i32::from(drum.peak_ms),
            ModField::Body => i32::from(drum.body),
            ModField::Ratchet => i32::from(drum.ratchet),
            ModField::Colour => i32::from(lpg.colour),
            ModField::Shape => place(&Shape::ALL, &lfo.shape)?,
            ModField::Phase => i32::from(lfo.phase),
            ModField::Deform => i32::from(lfo.deform),
            ModField::Fade => place(&Fade::ALL, &lfo.fade)?,
            ModField::Length => i32::from(steps.length),
            ModField::Start => i32::from(steps.start),
            ModField::Loop => place(&Loop::ALL, &steps.looping)?,
            ModField::Mutate => i32::from(steps.mutate),
            ModField::Jump => i32::from(steps.jump),
            ModField::Spread => i32::from(steps.spread),
            ModField::Bias => i32::from(steps.bias),
            ModField::Smooth => i32::from(steps.smooth),
            ModField::Step(at) => i32::from(steps.values[usize::from(at)]),
            ModField::Low => i32::from(self.track.bounds().low),
            ModField::High => i32::from(self.track.bounds().high),
            ModField::Rate | ModField::Destination => return None,
        })
    }

    /// What the `slot`th slot's setting goes back to under this type, as the slot starts, with
    /// Track's map's ends those of the source it follows.
    pub(crate) fn default_field(&self, slot: usize, field: ModField) -> Option<i32> {
        let mut fresh = ModSlot {
            kind: self.kind,
            ..default_mod(slot)
        };
        if matches!(field, ModField::Low | ModField::High) {
            fresh.track.source = self.track.source;
        }
        fresh.field(field)
    }

    /// What one of its lists can be, by name and meaning, in the order [`ModSlot::field`] counts
    /// them: its type, its mode, its polarity, an LFO's shape and Steps' loop. None for the
    /// others; where it goes is its track's.
    pub fn choices(&self, field: ModField) -> Vec<(Cow<'static, str>, &'static str)> {
        let borrowed = |(name, meaning)| (Cow::Borrowed(name), meaning);
        match field {
            ModField::Type => named::<ModKind>().map(borrowed).collect(),
            ModField::Mode => self.kind.modes().into_iter().map(borrowed).collect(),
            ModField::Polarity => named::<Polarity>().map(borrowed).collect(),
            // The attractors say what they are, and Rössler's name is found without its umlaut.
            ModField::Shape => Shape::ALL
                .iter()
                .map(|shape| {
                    let meaning = match shape {
                        Shape::Lorenz => "chaos round two wings, never quite repeating",
                        Shape::Rossler => "Rossler's chaos round one loop",
                        _ => "",
                    };
                    borrowed((shape.name(), meaning))
                })
                .collect(),
            ModField::Loop => Loop::ALL
                .iter()
                .map(|each| (Cow::Owned(each.to_string()), each.meaning()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// One of its settings as its cell shows it, as it's described: `120 ms`, `63%`, `-42`,
    /// `Note` for an AHD's hold, and an attractor's phase by the axis heard where it's one alone.
    /// None for one its type hasn't, or for where it goes, which its track names.
    pub fn text(&self, field: ModField) -> Option<String> {
        let Some(value) = self.field(field) else {
            // A rate isn't one number. One a note, Steps have none.
            return (field == ModField::Rate && field.on(self.kind)).then(|| {
                match (self.kind, self.steps.mode) {
                    (ModKind::Steps, StepsMode::Note) => "--".to_owned(),
                    (ModKind::Steps, _) => self.steps.rate.to_string(),
                    _ => self.lfo.rate.to_string(),
                }
            });
        };
        // What it's put back to doesn't change how it reads, so any slot's will do.
        Some(self.described(0, field).shown(value, None).cell)
    }

    /// How one of its settings reads under its type: by a list's names, `on` or `off`, a time,
    /// a percentage, a count, hex, a signed amount, degrees, a rate, a fade, or Track's map by what
    /// it follows. Where it goes is its track's to say.
    pub fn reading(&self, field: ModField) -> Reading {
        match field {
            ModField::Type
            | ModField::Mode
            | ModField::Polarity
            | ModField::Destination
            | ModField::Shape
            | ModField::Loop => Reading::Listed,
            ModField::Reset => Reading::Switch,
            ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => {
                Reading::Time
            }
            ModField::Hold => Reading::Hold,
            ModField::Sustain
            | ModField::Body
            | ModField::Mutate
            | ModField::Jump
            | ModField::Spread => Reading::Percent,
            ModField::Bias => Reading::Bend,
            ModField::Ratchet => Reading::Count {
                what: "how many strikes",
                from_one: false,
            },
            ModField::Length => Reading::Count {
                what: "how many steps",
                from_one: false,
            },
            ModField::Start => Reading::Count {
                what: "a step",
                from_one: true,
            },
            ModField::Colour => Reading::Hex,
            ModField::Depth | ModField::Smooth | ModField::Deform | ModField::Step(_) => {
                Reading::SignedHex
            }
            // An attractor's phase is heard alone on an axis at each third of the way round.
            ModField::Phase => Reading::Degrees {
                axes: self.lfo.shape.is_attractor(),
            },
            ModField::Rate => Reading::Rate,
            ModField::Fade => Reading::Fade,
            ModField::Low | ModField::High => Reading::Mapped(self.track.source),
        }
    }

    /// One of the `slot`th slot's settings described under its type, as it reads, steps and is
    /// typed. A rate and where it goes aren't one number, nor is a setting its type hasn't: they're
    /// described with no bounds, and stepped and listed by their own rules. The registry's name
    /// and key for it are the page's; this names it by its field.
    pub fn described(&self, slot: usize, field: ModField) -> Descriptor {
        let (low, high) = self.bounds(field).unwrap_or((0, 0));
        let name = field.name();
        let reading = self.reading(field);
        let of = Descriptor {
            reading,
            bounds: (low, high),
            default: self.default_field(slot, field).unwrap_or(0),
            nudge: 1,
            jump: HEX_JUMP,
            choices: self
                .choices(field)
                .into_iter()
                .map(|(name, meaning)| Choice::named(name).meaning(meaning))
                .collect(),
            typing: Typing::Whole { signed: false },
            reads: Reads::No,
            meaning: Cow::Borrowed(field.meaning(self.kind)),
            ..Descriptor::whole(name, name, (low, high), 0)
        };
        // A signed amount in hex, as a depth is.
        let signed = |of: Descriptor, what: &str| Descriptor {
            typing: Typing::Whole { signed: true },
            reads: Reads::HexWithin { signed: true },
            why_not: Cow::Owned(format!(
                "{what} is hex from {} to {}",
                depth_text(low as i16),
                depth_text(high as i16)
            )),
            ..of
        };
        let why = |text: String| Cow::Owned(text);
        match reading {
            Reading::Listed | Reading::Switch => Descriptor {
                wraps: true,
                typing: Typing::Listed,
                ..of
            },
            // An envelope's times step finer while they're short.
            Reading::Time => Descriptor {
                pace: Pace::Time,
                reads: NUMBER,
                why_not: why(format!("Times are milliseconds from {low} to {high}")),
                ..of
            },
            // `Note` is below no time, so `-` from 0 ms holds for the note.
            Reading::Hold => Descriptor {
                pace: Pace::Hold,
                reads: Reads::Hold,
                why_not: why(format!("{name} is milliseconds from 0 to {high}, or note")),
                ..of
            },
            Reading::Percent => Descriptor {
                jump: PERCENT_JUMP,
                reads: PERCENT,
                why_not: why(format!("{name} is a percent from {low} to {high}")),
                ..of
            },
            Reading::Count { what, from_one } => {
                let first = i32::from(from_one);
                Descriptor {
                    jump: 1,
                    reads: Reads::Number {
                        before: None,
                        after: "",
                        off: false,
                        from_one,
                    },
                    why_not: why(format!(
                        "{name} is {what}, from {} to {}",
                        low + first,
                        high + first
                    )),
                    ..of
                }
            }
            Reading::Hex => Descriptor {
                typing: Typing::Hex,
                reads: Reads::HexWithin { signed: false },
                why_not: why(format!("{name} is hex from {low:02X} to {high:02X}")),
                ..of
            },
            // One of Steps' values steps an octave large, as on `Pitch` it's semitones.
            Reading::SignedHex => match field {
                ModField::Step(_) => signed(Descriptor { jump: OCTAVE, ..of }, "A step"),
                _ => signed(of, name),
            },
            // An attractor's large step goes from one axis to the next.
            Reading::Degrees { axes } => Descriptor {
                nudge: PHASE_NUDGE,
                jump: if axes { AXIS_JUMP } else { PHASE_JUMP },
                wraps: true,
                reads: NUMBER,
                why_not: why(format!("{name} is degrees from {low} to {high}")),
                ..of
            },
            // Steps' `Bias` steps as a `Random` fill's does, from its middle.
            Reading::Bend => Descriptor {
                nudge: BEND_NUDGE,
                jump: BEND_JUMP,
                centred: true,
                typing: Typing::Whole { signed: true },
                reads: NUMBER,
                why_not: why(format!("{name} is a number from {low} to {high}")),
                ..of
            },
            // These step through their own kinds.
            Reading::Rate => Descriptor {
                nudge: 0,
                jump: 0,
                typing: Typing::Rate,
                ..of
            },
            Reading::Fade => Descriptor {
                nudge: 0,
                jump: 0,
                ..of
            },
            // A large step on a map of notes is an octave.
            Reading::Mapped(source) => {
                let (lowest, highest) = (source.text(low as u8), source.text(high as u8));
                match source == Source::Note {
                    true => Descriptor {
                        jump: OCTAVE,
                        typing: Typing::Named { list: false },
                        reads: Reads::Note,
                        why_not: why(format!("{name} is a note from {lowest} to {highest}")),
                        ..of
                    },
                    false => Descriptor {
                        typing: Typing::Hex,
                        reads: Reads::HexWithin { signed: false },
                        why_not: why(format!("{name} is hex from {lowest} to {highest}")),
                        ..of
                    },
                }
            }
            // No setting of a slot reads as these.
            _ => of,
        }
    }

    /// Set one of its settings, as [`ModSlot::field`] gives it, kept within its
    /// [`ModSlot::bounds`]; a phase goes round. One its type hasn't, or that isn't one number, is
    /// left alone. `Type` changes only the type, keeping every type's settings.
    pub fn set_field(&mut self, field: ModField, value: i32) {
        let Some((lowest, highest)) = self.bounds(field) else {
            return;
        };
        let value = if field == ModField::Phase {
            value.rem_euclid(highest + 1)
        } else {
            value.clamp(lowest, highest)
        };
        // Each fits what keeps it, being within its bounds.
        let byte = value as u8;
        let ms = value as u16;
        let signed = value as i16;
        let at = value as usize;
        let on = value != 0;
        let kind = self.kind;
        match field {
            ModField::Type => self.kind = ModKind::ALL[at],
            ModField::Mode => match kind {
                ModKind::Adsr => self.adsr.curve = Curve::ALL[at],
                ModKind::Ahd => self.ahd.curve = Curve::ALL[at],
                ModKind::Drum => self.drum.curve = Curve::ALL[at],
                ModKind::Lfo => self.lfo.mode = LfoMode::ALL[at],
                ModKind::Steps => self.steps.mode = StepsMode::ALL[at],
                ModKind::Track => self.track.source = Source::ALL[at],
                ModKind::Lpg => {}
            },
            ModField::Polarity => self.polarity = Some(Polarity::ALL[at]),
            ModField::Depth => self.depth = signed,
            ModField::Attack => match kind {
                ModKind::Ahd => self.ahd.attack_ms = ms,
                ModKind::Lpg => self.lpg.attack_ms = ms,
                _ => self.adsr.attack_ms = ms,
            },
            ModField::Decay => match kind {
                ModKind::Ahd => self.ahd.decay_ms = ms,
                ModKind::Drum => self.drum.decay_ms = ms,
                ModKind::Lpg => self.lpg.decay_ms = ms,
                _ => self.adsr.decay_ms = ms,
            },
            ModField::Reset => match kind {
                ModKind::Ahd => self.ahd.reset = on,
                ModKind::Lpg => self.lpg.reset = on,
                _ => self.adsr.reset = on,
            },
            ModField::Sustain => self.adsr.sustain = byte,
            ModField::Release => self.adsr.release_ms = ms,
            ModField::Hold => {
                self.ahd.hold = if value < 0 { Hold::Note } else { Hold::Ms(ms) };
            }
            ModField::Peak => self.drum.peak_ms = ms,
            ModField::Body => self.drum.body = byte,
            ModField::Ratchet => self.drum.ratchet = byte,
            ModField::Colour => self.lpg.colour = byte,
            ModField::Shape => self.lfo.shape = Shape::ALL[at],
            ModField::Phase => self.lfo.phase = ms,
            ModField::Deform => self.lfo.deform = signed,
            ModField::Fade => self.lfo.fade = Fade::ALL[at],
            ModField::Length => self.steps.length = byte,
            ModField::Start => self.steps.start = byte,
            ModField::Loop => self.steps.looping = Loop::ALL[at],
            ModField::Mutate => self.steps.mutate = byte,
            ModField::Jump => self.steps.jump = byte,
            ModField::Spread => self.steps.spread = byte,
            ModField::Bias => self.steps.bias = value as i8,
            ModField::Smooth => self.steps.smooth = signed,
            ModField::Step(step) => self.steps.values[usize::from(step)] = signed,
            ModField::Low => self.track.bounds_mut().low = byte,
            ModField::High => self.track.bounds_mut().high = byte,
            ModField::Rate | ModField::Destination => {}
        }
    }
}
