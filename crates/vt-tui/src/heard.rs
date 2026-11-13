//! What the engine heard of live generators (decision 300): each row a generator played while a
//! macro's lock, glide or modulation, or `Vary`, moved it, as the engine reports it. The grid shows
//! what's written; the playhead's row, the overview, the phrase's title and the status line show
//! what was heard, and `Space g a` and `Space g A` keep it.

use vt_engine::gens::GenReport;
use vt_model::action::Action;
use vt_model::generator::Generator;
use vt_model::macros::{GenColumn, MacroTarget};
use vt_model::song::Track;
use vt_model::step::Step;

use crate::app::App;

/// Whether anything could move a live generator as it plays: a macro holding one of its column's
/// settings the engine can hold, or `Vary`. Without, what was heard of it before an edit is
/// what's written now.
fn reachable(track: &Track, generator: &Generator) -> bool {
    let column = GenColumn::of(generator.column);
    generator.heard().varies()
        || track
            .macros
            .iter()
            .flat_map(|held| &held.maps)
            .any(|map| match map.target {
                MacroTarget::Gen { column: on, param } => on == column && param.held_in_engine(),
                MacroTarget::Setting(_) => false,
            })
}

/// A generator's rows as they were last heard, kept for `Space g a`: the rows heard otherwise than
/// written, each with its step as heard, and how many of its rows were heard at all.
pub(crate) struct HeardRows {
    pub rows: Vec<(usize, Step)>,
    pub heard: usize,
    pub unheard: usize,
    /// The pass the last of them was heard on.
    pub pass: i64,
}

impl App {
    /// Take in what the engine heard of a track's generators.
    pub(crate) fn hear_generator(&mut self, track: usize, report: GenReport) {
        match report {
            GenReport::Pass(pass) => {
                self.heard_passes
                    .insert((track, pass.phrase, pass.generator), pass);
            }
            GenReport::Row(row) => {
                self.heard_rows
                    .insert((track, row.phrase, row.generator, row.row), row);
            }
            GenReport::Rest { phrase } => self
                .heard_rows
                .retain(|(on, at, ..), _| (*on, *at) != (track, phrase)),
        }
    }

    /// A row of a live generator as it was last heard, if that wasn't what's written: the step
    /// as heard, and on which pass.
    pub(crate) fn heard_cell(
        &self,
        track: usize,
        phrase: u32,
        generator: usize,
        row: usize,
    ) -> Option<(Step, i64)> {
        let heard = self.heard_rows.get(&(track, phrase, generator, row))?;
        let owner = self.song().tracks.get(track)?;
        let held = owner.phrases.get(&phrase)?;
        let live = held.generators().get(generator)?;
        // Another generator may be in its place since.
        if live.column != heard.column || !reachable(owner, live) {
            return None;
        }
        let step = live.heard_row(held.steps(), row, heard.cell?)?;
        (step != held.steps()[row]).then_some((step, heard.pass))
    }

    /// A row with each live generator on it laid as last heard, if any was heard otherwise than
    /// written.
    pub(crate) fn heard_step(&self, track: usize, phrase: u32, row: usize) -> Option<Step> {
        let held = self.song().tracks.get(track)?.phrases.get(&phrase)?;
        let mut step = held.steps().get(row)?.clone();
        let mut moved = false;
        for (index, generator) in held.generators().iter().enumerate() {
            if self.heard_cell(track, phrase, index, row).is_some()
                && let Some(cell) = self.heard_rows[&(track, phrase, index, row)].cell
            {
                generator.lay_heard(row, cell, &mut step);
                moved = true;
            }
        }
        moved.then_some(step)
    }

    /// Whether a live generator was heard otherwise than written on any of its rows, which the
    /// phrase's title marks with `~`.
    pub(crate) fn generator_moving(&self, track: usize, phrase: u32, generator: usize) -> bool {
        self.heard_rows.keys().any(|&(on, at, index, row)| {
            (on, at, index) == (track, phrase, generator)
                && self.heard_cell(track, phrase, generator, row).is_some()
        })
    }

    /// A live generator's rows as last heard, if any was heard otherwise than written.
    pub(crate) fn heard_rows_of(
        &self,
        track: usize,
        phrase: u32,
        generator: usize,
    ) -> Option<HeardRows> {
        let held = self.song().tracks.get(track)?.phrases.get(&phrase)?;
        let span = held.generators().get(generator)?.rows()?;
        let (mut rows, mut heard, mut pass) = (Vec::new(), 0, 0);
        for row in span.first..=span.last {
            let Some(report) = self.heard_rows.get(&(track, phrase, generator, row)) else {
                continue;
            };
            heard += 1;
            pass = pass.max(report.pass);
            if let Some((step, _)) = self.heard_cell(track, phrase, generator, row) {
                rows.push((row, step));
            }
        }
        (!rows.is_empty()).then(|| HeardRows {
            rows,
            heard,
            unheard: span.len() - heard,
            pass,
        })
    }

    /// The edit that applies a live generator as it was last heard: applied, then each row heard
    /// otherwise than written as it was heard. None if it wasn't, so it applies as written.
    pub(crate) fn apply_as_heard(
        &self,
        track: usize,
        phrase: u32,
        generator: usize,
    ) -> Option<(Action, HeardRows)> {
        let heard = self.heard_rows_of(track, phrase, generator)?;
        let mut actions = vec![Action::ApplyGenerator {
            track,
            phrase,
            generator,
        }];
        actions.extend(heard.rows.iter().map(|(row, step)| Action::SetStep {
            track,
            phrase,
            step: *row,
            value: step.clone(),
        }));
        Some((Action::Batch(actions), heard))
    }

    /// `Space g a` on a live generator, `name`: applied as it was last heard where something moved
    /// it, each row as heard and the rows not heard yet as written, or else as written, leaving
    /// plain values either way, as one undo step.
    pub(crate) fn apply_generator(
        &mut self,
        track: usize,
        phrase: u32,
        generator: usize,
        name: &str,
    ) {
        let Some((action, heard)) = self.apply_as_heard(track, phrase, generator) else {
            self.apply(&Action::ApplyGenerator {
                track,
                phrase,
                generator,
            });
            self.status = format!("{name} is applied, as plain values");
            return;
        };
        self.apply(&action);
        let unheard = match heard.unheard {
            0 => String::new(),
            1 => ", and 1 not heard yet as written".to_owned(),
            count => format!(", and {count} not heard yet as written"),
        };
        self.status = format!(
            "{name} is applied as heard: {} rows as last heard, the last on pass {}{unheard}",
            heard.heard,
            heard.pass + 1
        );
    }
}
