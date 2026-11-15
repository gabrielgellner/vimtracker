//! The live generators' life cycle: a column made live over its loop, a patch's generator put
//! live, the one under the cursor reopened in the panel, and applied as plain values.

use std::sync::Arc;

use vt_model::action::Action;
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::shape::{GenParam, Shape, ShapeColumn, Where, is_empty};
use vt_model::step::FxCommand;

use crate::app::{App, Selection, TrackSpan};
use crate::input::mode::Mode;
use crate::overlays::picker::{Pick, Picker};
use crate::services::generators::{Part, column_label, generator_label, on_column};

use super::{Refusal, ShapePanel};

/// A shape saved elsewhere, as a patch's generator, made to fit a column: a `Where` the column
/// doesn't offer becomes the one it starts with, as a reshape's `Notes` is a generated column's
/// `All rows`. A fill it doesn't offer is refused, saying so.
fn fitted(shape: Shape, column: ShapeColumn) -> Result<Shape, String> {
    if !column.fills().contains(&shape.kind) {
        return Err(format!(
            "The patch's {} generator can't go on {}",
            shape.kind.name(),
            column_label(column)
        ));
    }
    let same =
        |place: &Where| std::mem::discriminant(place) == std::mem::discriminant(&shape.place);
    let place = if column.wheres().iter().any(same) {
        shape.place
    } else {
        column.default_where()
    };
    Ok(Shape { place, ..shape })
}

impl App {
    /// Make a column of the phrase under the cursor live over its loop, or its lane, with the
    /// shape the panel opens with, as `vG`, `S` and `L` would, for the Gen page's `Empty`. An
    /// empty FX column lists the commands first. Says whether it did.
    pub(crate) fn make_live(&mut self, field: Field, command: Option<FxCommand>) -> bool {
        let selection = self.loop_selection(field);
        let panel = match self.new_panel(selection, field, command) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => {
                self.services.status = reason.to_owned();
                return false;
            }
            Err(Refusal::Command(track)) => {
                let (_, items) = self
                    .services
                    .fx_command_items(track, |command| Pick::GenCommand { field, command });
                self.show_picker(Picker::new("FX commands", items).searching(""));
                self.services.status =
                    "Choose the command to write on the empty FX column".to_owned();
                return false;
            }
        };
        self.services.apply(&panel.live_action());
        let name = column_label(panel.parts[0].column);
        self.services.status = format!("{name} is live: turn it here, Enter opens the panel on it");
        true
    }

    /// A column of the phrase under the cursor over its loop, as `vG` selects it.
    fn loop_selection(&self, field: Field) -> Selection {
        let extent = self.services.extent();
        let track = self.services.cursor.track;
        Selection {
            range: StepRange::between(extent.start, extent.start + extent.length.max(1) - 1),
            scope: Scope::column(field.group()),
            tracks: TrackSpan::between(track, track),
        }
    }

    /// The edit that puts a patch's generator live on a column of the phrase under the cursor,
    /// over its loop or its lane, judged as `S` judges the column: an empty Note column is
    /// generated and notes are reshaped, and the generator already live there, `live`, is
    /// reshaped in place. Or why it can't go there.
    pub(crate) fn patch_generator(
        &mut self,
        field: Field,
        command: Option<FxCommand>,
        shape: Shape,
        live: Option<usize>,
    ) -> Result<Action, String> {
        let track = self.services.cursor.track;
        let phrase = self
            .services
            .phrase_id()
            .ok_or("This track has no phrase here for the patch's generators")?;
        if let Some(generator) = live {
            let column = self
                .services
                .phrase_here()
                .and_then(|found| found.generators().get(generator))
                .map(|found| found.column)
                .ok_or("That generator has gone")?;
            return Ok(Action::Reshape {
                track,
                phrase,
                generator,
                shape: fitted(shape, column)?,
            });
        }
        let selection = self.loop_selection(field);
        let mut panel = match self.new_panel(selection, field, command) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => return Err(reason.to_owned()),
            Err(Refusal::Command(_)) => {
                return Err("The patch's FX generator has no command".to_owned());
            }
        };
        panel.shape = fitted(shape, panel.parts[0].column)?;
        Ok(panel.live_action())
    }

    /// The live generator under the cursor: its phrase, and its place in the phrase's list. On the
    /// cursor's column, it's the one covering the cursor's row, or else the nearest.
    pub(crate) fn generator_here(&self) -> Option<(u32, usize)> {
        let phrase = self.services.phrase_id()?;
        let step = self.services.cursor.step;
        self.services
            .phrase_here()?
            .generators()
            .iter()
            .enumerate()
            .filter(|(_, generator)| on_column(self.services.cursor.field, generator.column))
            .filter_map(|(index, generator)| {
                let rows = generator.rows()?;
                let away = if step < rows.first {
                    rows.first - step
                } else {
                    step.saturating_sub(rows.last)
                };
                Some((away, index))
            })
            .min()
            .map(|(_, index)| (phrase, index))
    }

    /// Open the panel again on the live generator under the cursor, on its saved shape.
    pub(crate) fn reopen_generator(&mut self) {
        let Some((phrase, index)) = self.generator_here() else {
            self.services.status =
                "No live column here. Select rows, press S, then L to keep it live".to_owned();
            return;
        };
        let base = Arc::clone(self.song());
        let track = self.services.cursor.track;
        let found = &base.tracks[track].phrases[&phrase];
        let generator = &found.generators()[index];
        let Some(range) = generator.rows() else {
            return;
        };
        // It generates if its column is empty without it.
        let under = generator.under(found.steps());
        let part = Part {
            track,
            phrase,
            range,
            column: generator.column,
            generate: is_empty(&under, range, generator.column),
            drum: generator.shape.grids.part(),
        };
        let shape = generator.shape;
        let (sources, default_source) = match part.column {
            ShapeColumn::Note { .. } => {
                self.services
                    .follow_sources(&base, &[part], self.modes.view)
            }
            _ => (Vec::new(), None),
        };
        self.overlays.shape = Some(ShapePanel {
            selection: Selection {
                range,
                scope: Scope::column(self.services.cursor.field.group()),
                tracks: TrackSpan::between(track, track),
            },
            parts: vec![part],
            column: part.column,
            shape,
            row: GenParam::Fill,
            typing: None,
            live: Some(index),
            base,
            under: Some(under),
            sources,
            default_source,
        });
        self.modes.mode = Mode::Shape;
        self.preview_shape();
    }

    /// Apply the live generator under the cursor, leaving its values as plain ones: as last heard
    /// where something moved it as it played.
    pub(crate) fn apply_generator_here(&mut self) {
        let Some((phrase, generator)) = self.generator_here() else {
            self.services.status = "No live column here to apply".to_owned();
            return;
        };
        let track = self.services.cursor.track;
        let name =
            generator_label(&self.song().tracks[track].phrases[&phrase].generators()[generator]);
        self.services
            .apply_generator(track, phrase, generator, &name);
    }
}
