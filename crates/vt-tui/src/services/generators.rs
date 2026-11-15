//! The live generators as everything but the panel reads them: a phrase's generator with what
//! its settings act on, the phrases a `Follow` can read, where a shape on an empty column starts,
//! and their names in the phrase's heading. The Gen page, the shape panel, the Roto, macros and
//! the status line read them here.

use std::sync::Arc;

use vt_model::generator::{Generator, applied, reads, unfollowed};
use vt_model::scale::Scale;
use vt_model::selection::{Field, StepRange};
use vt_model::shape::{
    Context, DrumPart, FillKind, GenParam, Shape, ShapeColumn, Source, Starts, VARIATION_NOTES,
    Values, default_ends, is_empty, open_rows, variation_notes,
};
use vt_model::song::{Song, Track};
use vt_model::step::{MAX_NOTE, Step};

use crate::input::mode::View;
use crate::services::Services;

/// One track's part of what the panel or a live generator shapes: the phrase it's on, its rows,
/// and its column, whose FX command is the first on its own rows, or the one chosen for a column
/// with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub track: usize,
    pub phrase: u32,
    pub range: StepRange,
    pub column: ShapeColumn,
    /// Whether its column was empty, so it's generated rather than reshaped: notes of its own on
    /// the Note column, or the chosen command on an FX column.
    pub generate: bool,
    /// The part of Grids' beat it plays: the one its sound plays, or across tracks, the first
    /// the others leave.
    pub drum: DrumPart,
}

/// A shape made whole after a change: a list starts as the two ends, so it sounds as `Alternate`
/// did until it's typed, and a `Follow` follows a phrase it can, the one it starts on if what it
/// holds isn't one.
pub(crate) fn settle(shape: &mut Shape, sources: &[Source], start: Option<Source>) {
    if shape.kind == FillKind::List && shape.values.is_empty() {
        shape.values = Values::new(&[shape.from, shape.to]);
    }
    if shape.kind == FillKind::Follow
        && !sources.contains(&shape.follow.source)
        && let Some(source) = start.or(sources.first().copied())
    {
        shape.follow.source = source;
    }
}

/// Why a `Variation` with so many notes to learn from is refused, if it is.
pub(crate) fn too_few(shape: &Shape, notes: usize) -> Option<String> {
    (shape.kind == FillKind::Variation && notes < VARIATION_NOTES).then(|| match notes {
        0 => format!("Variation learns from the notes there: it needs {VARIATION_NOTES}"),
        1 => format!("Variation needs {VARIATION_NOTES} notes to learn from, and there's 1"),
        notes => {
            format!("Variation needs {VARIATION_NOTES} notes to learn from, and there are {notes}")
        }
    })
}

/// A live generator as the Gen page edits it: a copy of it, with its phrase's steps as they'd be
/// without it, the phrases a `Follow` can read and the loop it reads, and what `x` puts back.
#[derive(Debug, Clone)]
pub(crate) struct LiveGen {
    pub generator: Generator,
    pub under: Vec<Step>,
    sources: Vec<Source>,
    source: Option<Vec<Step>>,
    pub starts: Starts,
}

impl LiveGen {
    /// What its settings act on. It always covers rows, as [`Services::live_generator`] gives
    /// one only then.
    pub(crate) fn context(&self) -> Context<'_> {
        let rows = self.generator.rows().unwrap_or(StepRange::between(0, 0));
        let source = (self.generator.shape.kind == FillKind::Follow)
            .then_some(self.source.as_deref())
            .flatten();
        self.generator
            .context(&self.under, &self.sources, source)
            .unwrap_or(Context {
                column: self.generator.column,
                open: open_rows(&self.under, rows, self.generator.column),
                generates: false,
                sources: &self.sources,
                source,
            })
    }

    /// A shape it's given, made whole as the panel makes it: a list starts as the two ends, and
    /// a `Follow` reads a phrase it can.
    pub(crate) fn settle(&self, shape: &mut Shape) {
        settle(shape, &self.sources, self.starts.source);
    }
}

impl LiveGen {
    /// Why a shape it's given would be refused: a `Variation` with too few notes under it.
    pub(crate) fn refusal(&self, shape: &Shape) -> Option<String> {
        let rows = self.generator.rows()?;
        too_few(shape, variation_notes(&self.under, rows))
    }
}

/// Read a song's live generator, one of a track's phrase's by its place in its list, with what
/// its settings act on, from the song alone, as the Roto and the page's text do: `None` if
/// there's no such generator. A `Follow`'s sources aren't listed here, so its `Source` has no
/// choices; [`Services::live_generator`] lists them, for the app's own edits.
pub(crate) fn with_live<R>(
    song: &Song,
    track: &Track,
    phrase: u32,
    index: usize,
    read: impl FnOnce(&Generator, &Context) -> R,
) -> Option<R> {
    let found = track.phrases.get(&phrase)?;
    let generator = found.generators().get(index)?;
    let under = generator.under(found.steps());
    let source = generator
        .source()
        .and_then(|source| song.source_phrase(source))
        .map(|followed| followed.loop_steps());
    let context = generator.context(&under, &[], source)?;
    Some(read(generator, &context))
}

/// A live generator's setting as the Gen page shows it, from the song alone, or nothing if
/// there's no such generator.
pub(crate) fn live_shown(
    song: &Song,
    track: &Track,
    phrase: u32,
    index: usize,
    param: GenParam,
) -> String {
    with_live(song, track, phrase, index, |generator, context| {
        param.shown(&generator.shape, context)
    })
    .unwrap_or_default()
}

/// A live generator's name in the phrase's heading: its column, and for a `Follow` what it
/// follows, like `Note←T02:05`.
pub(crate) fn generator_label(generator: &Generator) -> String {
    let column = column_label(generator.column);
    match generator.source() {
        Some(source) => format!("{column}←{source}"),
        None => column,
    }
}

/// A column's short name, as the phrase's heading names its live ones.
pub(crate) fn column_label(column: ShapeColumn) -> String {
    match column {
        ShapeColumn::Note { .. } => "Note".to_owned(),
        ShapeColumn::Velocity => "Vel".to_owned(),
        ShapeColumn::Nudge { .. } => "Nudge".to_owned(),
        ShapeColumn::Length => "Len".to_owned(),
        ShapeColumn::Fx { slot, .. } => format!("FX{}", slot + 1),
    }
}

/// Whether the cursor on a field is on a live generator's column.
pub(crate) fn on_column(field: Field, column: ShapeColumn) -> bool {
    match (field, column) {
        (Field::Note, ShapeColumn::Note { .. })
        | (Field::Velocity, ShapeColumn::Velocity)
        | (Field::Nudge, ShapeColumn::Nudge { .. })
        | (Field::Gate | Field::GateLength, ShapeColumn::Length) => true,
        (
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot),
            ShapeColumn::Fx { slot: owned, .. },
        ) => slot == owned,
        _ => false,
    }
}

impl Services {
    /// Say so if an edit applied a live generator, as it changed one of its cells by hand.
    pub(crate) fn say_applied(&mut self, next: &Arc<Song>) {
        if let Some(source) = unfollowed(self.song(), next).first() {
            self.status = format!(
                "{source} is gone, so what followed it is plain notes now. u brings it back"
            );
        } else if let Some(column) = applied(self.song(), next).first() {
            self.status = format!(
                "{} was live: it's plain values now. u makes it live again",
                column_label(*column)
            );
        }
    }

    /// Roll a shape's seed again, as `r` does: with `Acid`'s pitches kept, only its second die,
    /// the accents, slides and octaves; and a `Ring` starts again from its new seed, letting go of
    /// a register it kept, as an `Acid`'s loops start again from their first rows.
    pub(crate) fn roll(&mut self, shape: &mut Shape) {
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = self.shape_seed;
        if shape.kind == FillKind::Acid && shape.acid.keep {
            shape.acid.roll = shape.acid.roll.wrapping_add(seed).wrapping_add(1);
        } else {
            shape.seed = self.song().seed.wrapping_add(seed);
            shape.ring.kept = None;
            shape.acid.at = 0;
        }
    }

    /// The part of Grids' beat a track's sound plays, if it says: an 808 voice's, or its name's.
    pub(crate) fn drum_of(&self, track: usize) -> Option<DrumPart> {
        let track = self.song().tracks.get(track)?;
        DrumPart::of_sound(track.machine, &track.name)
    }

    /// The phrases these parts can follow, and the one to start on: the phrase beside them on the
    /// nearest track that has one, looking before the first part's track and then after it, as a
    /// canon's theme is usually above its answers, or what that phrase follows, if it does. The
    /// phrase beside is level with the cursor on a view.
    pub(crate) fn follow_sources(
        &self,
        song: &Song,
        parts: &[Part],
        view: View,
    ) -> (Vec<Source>, Option<Source>) {
        let own: Vec<Source> = parts
            .iter()
            .map(|part| Source {
                track: part.track,
                phrase: part.phrase,
            })
            .collect();
        let sources: Vec<Source> = song
            .tracks
            .iter()
            .enumerate()
            .take(song.sound_tracks())
            .flat_map(|(track, owner)| {
                owner.phrases.keys().map(move |phrase| Source {
                    track,
                    phrase: *phrase,
                })
            })
            .filter(|source| !own.iter().any(|here| reads(song, *source, *here)))
            .collect();
        let first = parts.first().map_or(0, |part| part.track);
        let before = (0..first).rev();
        let after = first + 1..song.sound_tracks();
        let mut beside = before.chain(after).find_map(|track| {
            let (phrase, _) = self.phrase_beside(track, view)?;
            let source = Source { track, phrase };
            sources.contains(&source).then_some(source)
        });
        // One that follows another itself gives way to what it follows, so each voice of a canon
        // starts on the theme. Cycles are refused, so this ends.
        while let Some(followed) = beside
            .and_then(|source| song.source_phrase(source))
            .and_then(|phrase| phrase.generators().iter().find_map(Generator::source))
            .filter(|source| sources.contains(source))
        {
            beside = Some(followed);
        }
        (sources, beside)
    }

    /// Where a shape on an empty column starts and ends: for notes, an octave up from the scale's
    /// root, or C with it off, in the octave notes are typed in; and the command's whole range.
    pub(crate) fn generated_ends(&self, column: ShapeColumn) -> (u8, u8) {
        match column {
            ShapeColumn::Note { scale, .. } => {
                let root = Scale {
                    root: if scale.on { scale.root } else { 0 },
                    ..scale
                };
                let low = root.degree(0, self.live.octave).min(MAX_NOTE - 12);
                (low, low + 12)
            }
            column => (column.lowest(), column.highest()),
        }
    }

    /// One of the track's live generators, with what its settings act on, if there's one there:
    /// a `Follow` starts on the phrase beside it, level with the cursor on a view.
    pub(crate) fn live_generator(
        &self,
        track: usize,
        phrase: u32,
        index: usize,
        view: View,
    ) -> Option<LiveGen> {
        let song = self.song();
        let found = song.tracks.get(track)?.phrases.get(&phrase)?;
        let generator = found.generators().get(index)?.clone();
        let rows = generator.rows()?;
        let under = generator.under(found.steps());
        let generates = is_empty(&under, rows, generator.column);
        let part = Part {
            track,
            phrase,
            range: rows,
            column: generator.column,
            generate: generates,
            drum: generator.shape.grids.part(),
        };
        let (sources, default_source) = match generator.column {
            ShapeColumn::Note { .. } => self.follow_sources(song, &[part], view),
            _ => (Vec::new(), None),
        };
        let source = song
            .source_phrase(generator.shape.follow.source)
            .map(|phrase| phrase.loop_steps().to_vec());
        let ends = if generates {
            self.generated_ends(generator.column)
        } else {
            default_ends(&under, rows, generator.column)
        };
        Some(LiveGen {
            generator,
            under,
            sources,
            source,
            starts: Starts {
                ends,
                source: default_source,
                part: self.drum_of(track),
            },
        })
    }
}
