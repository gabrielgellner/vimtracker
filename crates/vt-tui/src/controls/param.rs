//! A setting as a widget draws and edits it (the UI plan's §3.1): a [`ParamView`], with how it
//! steps ([`Steps`], [`Pace`]), what its text reads ([`Reads`]) and its [`Choice`]s. [`view`]
//! resolves each setting into one, and `widgets::param::edit` reads only this, so it knows no page
//! and no device.
//!
//! [`view`]: super::view()

use std::borrow::Cow;

use vt_model::constants::BAR_STEPS;
use vt_model::listed::Listed;
use vt_model::settings::Units;
use vt_model::shape::params::Typing;

/// How many small steps an envelope time's large one is.
const TIME_JUMP: i32 = 10;

/// How far one step moves a value from where it is now, up and down. They differ where the step
/// depends on the value, as an envelope's time steps by 5 ms from 10 ms up but by 1 ms below it,
/// so `-` from 10 ms goes to 9 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Steps {
    pub up: i32,
    pub down: i32,
}

impl Steps {
    /// A step of the same size either way.
    pub const fn even(by: i32) -> Self {
        Self { up: by, down: by }
    }

    /// No step: a setting moved by its own rule, as a rate through its kinds, or not at all.
    pub const NONE: Self = Self::even(0);
}

/// How a count's steps follow one another, as `3=` takes three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pace {
    /// Each the same, wherever it gets to: three steps go three times as far.
    #[default]
    Even,
    /// An envelope's time, whose steps grow as it gets longer.
    Time,
    /// An AHD's hold, which steps as a time and has `Note` below 0 ms.
    Hold,
    /// The track's `Reset`, whose large steps go from one whole bar to the next.
    Bars,
}

impl Pace {
    /// The small and the large step from a value, where they depend on it.
    pub fn steps(self, value: i32) -> Option<(Steps, Steps)> {
        let time = |value: i32| {
            let nudge = Steps {
                up: time_step(value),
                down: time_step(value - 1),
            };
            let jump = Steps {
                up: nudge.up * TIME_JUMP,
                down: nudge.down * TIME_JUMP,
            };
            (nudge, jump)
        };
        Some(match self {
            Self::Even => return None,
            Self::Time => time(value),
            // Up from `Note` is 0 ms, down from no time is `Note`, and a time steps down no
            // further than 0 ms.
            Self::Hold => match value {
                ..0 => (Steps::even(1), Steps::even(1)),
                0 => (
                    Steps::even(1),
                    Steps {
                        up: TIME_JUMP,
                        down: 1,
                    },
                ),
                _ => {
                    let (nudge, jump) = time(value);
                    let down = |steps: Steps| Steps {
                        down: steps.down.min(value),
                        ..steps
                    };
                    (down(nudge), down(jump))
                }
            },
            Self::Bars => {
                let bar = BAR_STEPS as i32;
                let jump = Steps {
                    up: (value / bar + 1) * bar - value,
                    down: value - ((value + bar - 1) / bar - 1).max(0) * bar,
                };
                (Steps::even(1), jump)
            }
        })
    }
}

/// How `i`'s text reads as a value. Where it doesn't, or the value is outside its bounds, the
/// status line says the view's `why_not`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reads {
    /// Not at all: `i` lists it or turns it over, or it's typed its own way.
    #[default]
    No,
    /// Two hex digits, kept to its bounds, each worth `1 << shift` of it, as a macro's are its
    /// coarse byte.
    Hex { shift: u32 },
    /// Hex refused outside its bounds, with a sign before it where `signed`.
    HexWithin { signed: bool },
    /// A whole number, with any of the letter `before` it and any of the text `after` it, `off`
    /// for its lowest where `off` (a number then starting above it), counted from 1 where
    /// `from_one`.
    Number {
        before: Option<char>,
        after: &'static str,
        off: bool,
        from_one: bool,
    },
    /// A glide's time in ms, or `off`.
    Glide,
    /// A track's `Reset`: steps, bars like `2bar`, or `off`.
    Reset,
    /// An AHD's hold in ms, or `note`.
    Hold,
    /// A note by its name.
    Note,
}

/// One of the choices `K` lists for a setting: its name, what it does and the heading it's
/// listed under, either of them empty for none.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub name: Cow<'static, str>,
    pub meaning: Cow<'static, str>,
    pub group: Cow<'static, str>,
}

impl Choice {
    /// A choice with only a name.
    pub fn named(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            meaning: Cow::Borrowed(""),
            group: Cow::Borrowed(""),
        }
    }

    /// With what it does.
    pub fn meaning(self, meaning: impl Into<Cow<'static, str>>) -> Self {
        Self {
            meaning: meaning.into(),
            ..self
        }
    }

    /// One of a [`Listed`] kind, as its list names it, says what it does and heads it.
    pub fn of<X: Listed>(each: X) -> Self {
        Self {
            group: Cow::Borrowed(each.category()),
            ..Self::named(each.name()).meaning(each.meaning())
        }
    }
}

/// One setting as a widget draws and edits it, resolved where it is: on this track, in its slot,
/// under its mod slot's type, or on its generator's column. Every cell, title, the status line,
/// the mixer and the Roto's value read only this.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamView {
    /// What the Roto's layouts name it by.
    pub key: &'static str,
    /// Its full name, for the status line.
    pub name: &'static str,
    /// Its name in its cell: its label, an effect's setting by its full name, a macro by the name
    /// it's been given.
    pub label: Cow<'static, str>,
    /// What it's set to now, within `bounds`: a byte, a time, a count, a choice by its place in
    /// `choices`, or 0 and 1 for a switch.
    pub value: i32,
    /// Its fine byte, where it has one.
    pub fine: Option<u8>,
    pub bounds: (i32, i32),
    /// What `x` puts back.
    pub default: i32,
    /// `=` and `-`, and `+` and `_`, from where it is now.
    pub nudge: Steps,
    pub jump: Steps,
    /// How a count's steps follow one another.
    pub pace: Pace,
    /// Goes round past either end: a phase, a list that wraps.
    pub(crate) wraps: bool,
    /// Its bar draws from the middle: pan, trim, a band's boost.
    pub centred: bool,
    /// Where it sits on its bar, 0 to 255, or none for a value with no bar.
    pub bar: Option<u8>,
    /// Off or on: drawn as a switch.
    pub toggle: bool,
    /// What `K` lists for it, in order, or none for a setting that isn't a list.
    pub choices: Vec<Choice>,
    /// Whether `K` lists its choices by number, `03  Brass`, as a machine's own lists are.
    pub numbered: bool,
    /// What `K`'s list is called.
    pub title: &'static str,
    /// Its value in its cell: `80`, `1/16`, `120 ms`, `Overdrive`.
    pub cell: String,
    /// Its value as the status line says it after its name, in real units or what it does:
    /// `1.21 kHz`, `-6.0 dB`, `on: Res stops short of ringing by itself`. Empty where the cell
    /// already says it all.
    pub text: String,
    /// How `i` takes it.
    pub typing: Typing,
    /// How what `i` takes reads as a value.
    pub reads: Reads,
    /// What the status line says when it doesn't.
    pub(crate) why_not: Cow<'static, str>,
    /// The registry's units, for one of its settings.
    pub units: Option<Units>,
    /// Not dim: away from its default, or doing something on this track.
    pub(crate) engaged: bool,
}

/// How far a small step moves an envelope time. Short times need finer steps than long ones.
fn time_step(value: i32) -> i32 {
    match value {
        ..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}
