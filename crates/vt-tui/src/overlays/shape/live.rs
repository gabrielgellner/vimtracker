//! The live generators' life cycle: a column made live over its loop, a patch's generator put
//! live, the one under the cursor reopened in the panel, and applied as plain values.

use std::sync::Arc;

use vt_model::action::Action;
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::shape::{FillKind, GenParam, Shape, ShapeColumn, Where, is_empty};
use vt_model::step::FxCommand;

use crate::app::{Selection, TrackSpan};
use crate::input::mode::View;
use crate::overlays::picker::{Pick, Picker};
use crate::pages::Outcome;
use crate::services::Services;
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
    // A kit line is its tracks', so a patch's goes on as a `Mini` of its own.
    Ok(Shape {
        place,
        kit: None,
        ..shape
    })
}

impl Services {
    /// Make a column of the phrase under the cursor live over its loop, or its lane, with the
    /// shape the panel opens with, as `vG`, `S` and `L` would, for the Gen page's `Empty`. An
    /// empty FX column lists the commands first: the error holds the list to open, or none where
    /// the column can't be made live, the status line saying why.
    pub(crate) fn make_live(
        &mut self,
        field: Field,
        command: Option<FxCommand>,
        view: View,
    ) -> Result<(), Option<Box<Picker>>> {
        let selection = self.loop_selection(field);
        let panel = match ShapePanel::new(selection, field, command, self, view) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => {
                self.status = reason.to_owned();
                return Err(None);
            }
            Err(Refusal::Said) => return Err(None),
            Err(Refusal::Command(track)) => {
                let (_, items) =
                    self.fx_command_items(track, |command| Pick::GenCommand { field, command });
                self.status = "Choose the command to write on the empty FX column".to_owned();
                return Err(Some(Box::new(
                    Picker::new("FX commands", items).searching(""),
                )));
            }
        };
        self.apply(&panel.live_action());
        let name = column_label(panel.parts[0].column);
        self.status = format!("{name} is live: turn it here, Enter opens the panel on it");
        Ok(())
    }

    /// A column of the phrase under the cursor over its loop, as `vG` selects it.
    fn loop_selection(&self, field: Field) -> Selection {
        let extent = self.extent();
        let track = self.cursor.track;
        Selection {
            range: StepRange::between(extent.start, extent.start + extent.length.max(1) - 1),
            scope: Scope::column(field.group()),
            tracks: TrackSpan::between(track, track),
        }
    }

    /// The edit that puts a patch's generator live on a column of the phrase under the cursor,
    /// over its loop or its lane, judged as `S` judges the column: an empty Note column is
    /// generated and notes are reshaped, and the generator already live there, `live`, is
    /// reshaped in place. A `List` goes through the patch's list, and an `Own` rhythm plays its
    /// text. Or why it can't go there.
    pub(crate) fn patch_generator(
        &mut self,
        field: Field,
        command: Option<FxCommand>,
        (shape, list, rhythm): (Shape, &[u8], &str),
        live: Option<usize>,
        view: View,
    ) -> Result<Action, String> {
        let track = self.cursor.track;
        let phrase = self
            .phrase_id()
            .ok_or("This track has no phrase here for the patch's generators")?;
        if let Some(generator) = live {
            let column = self
                .phrase_here()
                .and_then(|found| found.generators().get(generator))
                .map(|found| found.column)
                .ok_or("That generator has gone")?;
            return Ok(Action::Reshape {
                track,
                phrase,
                generator,
                shape: fitted(shape, column)?,
                list: Some(list.into()),
                rhythm: Some(rhythm.into()),
            });
        }
        let selection = self.loop_selection(field);
        let mut panel = match ShapePanel::new(selection, field, command, self, view) {
            Ok(panel) => panel,
            Err(Refusal::Reason(reason)) => return Err(reason.to_owned()),
            Err(Refusal::Said) => return Err(self.status.clone()),
            Err(Refusal::Command(_)) => {
                return Err("The patch's FX generator has no command".to_owned());
            }
        };
        panel.shape = fitted(shape, panel.parts[0].column)?;
        panel.list = list.to_vec();
        match panel.shape.kind {
            FillKind::Mini => rhythm.clone_into(&mut panel.mini),
            _ => rhythm.clone_into(&mut panel.rhythm),
        }
        panel.own().settle(&mut panel.shape);
        Ok(panel.live_action())
    }

    /// The live generator under the cursor: its phrase, and its place in the phrase's list. On the
    /// cursor's column, it's the one covering the cursor's row, or else the nearest.
    pub(crate) fn generator_here(&self) -> Option<(u32, usize)> {
        let phrase = self.phrase_id()?;
        let step = self.cursor.step;
        self.phrase_here()?
            .generators()
            .iter()
            .enumerate()
            .filter(|(_, generator)| on_column(self.cursor.field, generator.column))
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

    /// Apply the live generator under the cursor, leaving its values as plain ones: as last heard
    /// where something moved it as it played.
    pub(crate) fn apply_generator_here(&mut self) {
        let Some((phrase, generator)) = self.generator_here() else {
            self.status = "No live column here to apply".to_owned();
            return;
        };
        let track = self.cursor.track;
        let name =
            generator_label(&self.song().tracks[track].phrases[&phrase].generators()[generator]);
        self.apply_generator(track, phrase, generator, &name);
    }
}

impl ShapePanel {
    /// The live generator `L` kept on the cursor's track, in the phrase under the cursor: its
    /// place in the phrase's list. Nothing if it was kept as plain values, or elsewhere.
    pub(crate) fn kept_generator(&self, services: &Services) -> Option<usize> {
        let track = services.cursor.track;
        let part = self.parts.iter().find(|part| part.track == track)?;
        if !self.kept_live || services.phrase_id() != Some(part.phrase) {
            return None;
        }
        let first = part.range.first;
        let generators = services.song().tracks[track]
            .phrases
            .get(&part.phrase)?
            .generators();
        // The one on its rows, or, on a lane, which has no rows, the column's.
        let on_rows = generators.iter().position(|generator| {
            generator.column == part.column
                && generator
                    .rows()
                    .is_some_and(|rows| rows.first <= first && first <= rows.last)
        });
        on_rows.or_else(|| {
            generators
                .iter()
                .position(|generator| generator.column == part.column)
        })
    }

    /// The panel open again on the live generator under the cursor, on its saved shape, or
    /// nothing, the status line saying why.
    pub(crate) fn reopen(services: &mut Services, view: View) -> Outcome {
        let Some((phrase, index)) = services.generator_here() else {
            services.status =
                "No live column here. Select rows, press S, then L to keep it live".to_owned();
            return Outcome::Done;
        };
        let base = Arc::clone(services.song());
        let track = services.cursor.track;
        let found = &base.tracks[track].phrases[&phrase];
        let generator = &found.generators()[index];
        let Some(range) = generator.rows() else {
            return Outcome::Done;
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
        let list = generator.list().to_vec();
        let (rhythm, mini) = match shape.kind {
            FillKind::Mini => (String::new(), generator.rhythm().to_owned()),
            _ => (generator.rhythm().to_owned(), String::new()),
        };
        let (sources, default_source) = match part.column {
            ShapeColumn::Note { .. } => services.follow_sources(&base, &[part], view),
            _ => (Vec::new(), None),
        };
        // On a kit line, its line on every track it spans (decision 464).
        let names = shape.kit.map_or_else(Vec::new, |kit| base.kit_names(kit));
        Outcome::Shape(Box::new(Self {
            selection: Selection {
                range,
                scope: Scope::column(services.cursor.field.group()),
                tracks: TrackSpan::between(track, track),
            },
            parts: vec![part],
            column: part.column,
            shape,
            list,
            rhythm,
            mini,
            kit: shape.kit,
            names,
            kit_notes: Vec::new(),
            kit_from: shape.from,
            row: GenParam::Fill,
            typing: None,
            weight: None,
            tokens: None,
            live: Some(index),
            base,
            under: Some(under),
            sources,
            default_source,
            kept: false,
            kept_live: false,
            pattern: None,
        }))
    }
}
