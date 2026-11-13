//! A live generator's preview on the Gen page: what it writes across its span, three pixels' worth
//! of characters a row, drawn from the phrase's steps as they are, so it shows what's heard: notes
//! as short lines at their pitch, a mini piano roll; velocities and lengths as bars up from the
//! bottom, and nudges either side of the middle; and an FX column as its values' line, as the
//! FX page draws a curve. Its picture is sent again only when what it shows changes, and the row
//! playing rides it as a dot, placed as an LFO's is.

use vt_model::generator::Generator;
use vt_model::shape::ShapeColumn;
use vt_model::step::{MAX_VELOCITY, Note, Step};

use super::plot::Plot;

/// How a column's values are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenLook {
    /// A short line a note, at its pitch.
    Notes,
    /// A bar up from the bottom a row.
    Bars,
    /// A bar from the middle, up or down, a row.
    Centred,
    /// A line through the values.
    Line,
}

/// What a generator's preview shows: how it's drawn, and each row's value from 0 at the bottom
/// to 1 at the top, or nothing where the row has none.
#[derive(Debug, Clone, PartialEq)]
pub struct GenDrawing {
    pub look: GenLook,
    pub levels: Vec<Option<f32>>,
}

/// The value a row has in a column, as a share of its range: a note as itself, the others
/// already from 0 to 1.
fn value(column: ShapeColumn, step: &Step) -> Option<f32> {
    let note = matches!(step.note, Note::On(_));
    match column {
        ShapeColumn::Note { .. } => match step.note {
            Note::On(pitch) => Some(f32::from(pitch)),
            _ => None,
        },
        ShapeColumn::Velocity => {
            note.then(|| f32::from(step.velocity.unwrap_or(MAX_VELOCITY)) / f32::from(MAX_VELOCITY))
        }
        ShapeColumn::Nudge { most } => note.then(|| {
            let most = f32::from(most.max(1));
            ((f32::from(step.nudge) + most) / (2.0 * most)).clamp(0.0, 1.0)
        }),
        ShapeColumn::Length => step
            .length()
            .filter(|_| note)
            .map(|length| f32::from(length) / f32::from(u8::MAX)),
        ShapeColumn::Fx { slot, command } => step
            .fx
            .get(slot)
            .filter(|fx| fx.command == Some(command))
            .map(|fx| f32::from(fx.value) / (f32::from(command.highest()) * 256.0).max(1.0)),
    }
}

impl GenDrawing {
    /// A live generator's, from its phrase's steps. Notes go from the lowest in its span at the
    /// bottom to the highest at the top, or across the middle if there's one pitch.
    pub fn of(generator: &Generator, steps: &[Step]) -> Option<Self> {
        let rows = generator.rows()?;
        let column = generator.column;
        let mut levels: Vec<Option<f32>> = (rows.first..=rows.last)
            .map(|row| steps.get(row).and_then(|step| value(column, step)))
            .collect();
        let look = match column {
            ShapeColumn::Note { .. } => GenLook::Notes,
            ShapeColumn::Velocity | ShapeColumn::Length => GenLook::Bars,
            ShapeColumn::Nudge { .. } => GenLook::Centred,
            ShapeColumn::Fx { .. } => GenLook::Line,
        };
        if look == GenLook::Notes {
            let present = levels.iter().flatten();
            let lowest = present.clone().copied().fold(f32::MAX, f32::min);
            let highest = present.copied().fold(f32::MIN, f32::max);
            for level in levels.iter_mut().flatten() {
                *level = if highest > lowest {
                    (*level - lowest) / (highest - lowest)
                } else {
                    0.5
                };
            }
        }
        Some(Self { look, levels })
    }

    /// Where a row's middle is across, from 0 to 1.
    pub fn x_of(&self, row: usize) -> f32 {
        (row as f32 + 0.5) / self.levels.len().max(1) as f32
    }

    /// Where the dot rides on a row: at its value, or with none, at the bottom, or the middle
    /// for nudges.
    pub fn point(&self, row: usize) -> (f32, f32) {
        let rest = if self.look == GenLook::Centred {
            0.5
        } else {
            0.0
        };
        let level = self.levels.get(row).copied().flatten().unwrap_or(rest);
        (self.x_of(row), level)
    }

    /// Its picture, this many pixels across and down.
    pub fn plot(&self, size: (usize, usize)) -> Plot {
        let mut plot = Plot::new(size.0, size.1);
        let half = 0.35 / self.levels.len().max(1) as f32;
        let mut line = Vec::new();
        for (row, level) in self.levels.iter().enumerate() {
            let x = self.x_of(row);
            let Some(level) = *level else {
                if self.look == GenLook::Line && line.len() > 1 {
                    plot.line(&line);
                }
                line.clear();
                continue;
            };
            match self.look {
                GenLook::Notes => {
                    plot.line(&[plot.at(x - half, level), plot.at(x + half, level)]);
                }
                GenLook::Bars => plot.line(&[plot.at(x, 0.0), plot.at(x, level)]),
                GenLook::Centred => plot.line(&[plot.at(x, 0.5), plot.at(x, level)]),
                GenLook::Line => line.push(plot.at(x, level)),
            }
        }
        if line.len() > 1 {
            plot.line(&line);
        }
        plot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::generator::Span;
    use vt_model::shape::{
        Acid, Bits, DEFAULT_RATE, FillKind, Follow, Pink, Shape, Values, Walk, Where,
    };
    use vt_model::song::Phrase;

    #[test]
    fn notes_go_from_the_lowest_to_the_highest_and_rests_have_none() {
        let steps: Vec<Step> = [
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-4 -- --- --- ---",
        ]
        .iter()
        .map(|step| step.parse().unwrap())
        .collect();
        let mut phrase = Phrase::from_steps(steps);
        let shape = Shape {
            from: 60,
            to: 72,
            curve: 0,
            s: 0,
            kind: FillKind::Reverse,
            wave: vt_model::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::Notes,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: vt_model::shape::Grids::default(),
            vary: 0,
        };
        let column = ShapeColumn::Note {
            scale: vt_model::scale::Scale::default(),
            writes: None,
        };
        let generator = Generator::new(column, Span::Rows { first: 0, last: 2 }, shape);
        assert!(phrase.add_generator(generator, None));
        let generator = &phrase.generators()[0];
        let drawing = GenDrawing::of(generator, phrase.steps()).unwrap();
        assert_eq!(drawing.look, GenLook::Notes);
        // Reversed, so the high note comes first.
        assert_eq!(drawing.levels, [Some(1.0), None, Some(0.0)]);
        assert_eq!(
            drawing.point(1),
            (0.5, 0.0),
            "a rest's dot sits at the bottom"
        );
        let plot = drawing.plot((49 * 3, 5 * 3));
        assert!(plot.cover.iter().any(|cover| *cover > 0));
    }
}
