//! A setting as a widget draws and edits it (the UI plan's §3.1): a [`ParamView`], which is the
//! model's [`Descriptor`] of it with its value read where it is and what only the program knows.
//! [`view`] resolves each setting into one, and `widgets::param::edit` reads only this, so it knows
//! no page and no device. How a setting steps, lists, is typed and reads is the model's
//! (`vt_model::param`), re-exported here.
//!
//! [`view`]: super::view()

use std::borrow::Cow;

pub use vt_model::param::{Choice, Pace, Reads, Steps, Typing};
use vt_model::param::{Descriptor, Reading};
use vt_model::settings::Units;

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
    /// What it does, as the model says it, for the line that describes the setting under the
    /// cursor ([`Descriptor::meaning`]).
    pub meaning: Cow<'static, str>,
}

impl ParamView {
    /// Its description at `value`, as the model describes the setting: its range, steps, list,
    /// typing and how the value reads in its cell. Its names, title, units and fine part are left
    /// as they are.
    pub fn described(self, described: Descriptor, value: i32) -> Self {
        let (nudge, jump) = described.steps(value);
        let shown = described.shown(value, self.fine);
        let (bar, toggle) = (described.bar(value), described.reading == Reading::Switch);
        Self {
            value,
            bounds: described.bounds,
            default: described.default,
            nudge,
            jump,
            pace: described.pace,
            wraps: described.wraps,
            centred: described.centred,
            bar,
            toggle,
            choices: described.choices,
            cell: shown.cell,
            text: shown.text,
            typing: described.typing,
            reads: described.reads,
            why_not: described.why_not,
            meaning: match described.meaning.is_empty() {
                true => self.meaning,
                false => described.meaning,
            },
            ..self
        }
    }
}
