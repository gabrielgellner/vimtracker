//! A generator's settings, declared once: what each is called and stored by, the values it takes,
//! which shapes have it, and how it's shown, stepped, typed, chosen from a list and put back. The
//! shape panel's rows are these, and so are the Gen page's cells, its `K` lists and the Roto's
//! knobs; a macro will reach them by their keys (the design's `generators-page.md` §3.3, decision
//! 278).
//!
//! They're the registry's idea, [`crate::settings`], in a form of their own: a generator's
//! settings go past a byte (a `Run` of 256, `Hits` among thousands of rows), some are signed, and
//! what `From`, `To`, `Hits` and `Rotate` mean hangs on the column filled, which a [`Context`]
//! says. Each read and change is a pure function of the shape and its context, so the edit thread
//! and, later, the engine work them out alike.

use crate::lfo::{Shape as Wave, note_rates};
use crate::step::Step;

use super::{
    Acid, Bits, DEFAULT_RATE, EUCLID_HITS, Edge, FillKind, MAX_ACID_NOTES, MAX_BEND, MAX_DEPTH,
    MAX_FACTOR, MAX_TRANSPOSE, MIN_DEPTH, Pink, Shape, ShapeColumn, Source, VARIES, Values, WAVES,
    WaveRate, Where, default_step, first_note,
};

/// How far a small and a large step move a curve.
const BEND_NUDGE: i32 = 5;
const BEND_JUMP: i32 = 25;
/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i32 = 0x10;
/// How far a large step moves an `Every` or a `Chance`, and any other percent.
const EVERY_JUMP: i32 = 4;
const CHANCE_JUMP: i32 = 10;
/// How far a small and a large step move a phase, in degrees, as on an LFO.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
/// The longest `Run`, and how far a large step moves it.
pub const MAX_RUN: u16 = 256;
const RUN_JUMP: i32 = 4;
/// The most rows an `Every` can skip.
pub const MAX_EVERY: u16 = 256;
/// How far a large step moves a Euclidean `Where`'s hits or rotation, or an `Acid`'s notes.
const EUCLID_JUMP: i32 = 4;
/// The counts of cycles across the rows that `Rate`'s list offers after the times.
const LISTED_CYCLES: [u16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// The longest `Step` a walk of notes takes: two octaves of semitones, or of degrees.
const MAX_NOTE_STEP: u8 = 24;
/// How far a large step moves `Multiply`'s factor.
const FACTOR_JUMP: i32 = 4;

/// How many columns and rows a generator's settings take on the Gen page.
pub const GEN_COLUMNS: usize = 3;
pub const GEN_ROWS: usize = 3;
/// How many settings a generator's title on the Gen page has room for.
pub const GEN_TITLE: usize = 4;

/// What a generator's settings act on, which what some of them mean hangs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context<'a> {
    /// The column it fills, which `From` and `To` are values of.
    pub column: ShapeColumn,
    /// How many rows the column could change, which Euclid's `Hits` are spread over and its
    /// `Rotate` goes round.
    pub open: usize,
    /// Whether the column was empty, so it's generated on rather than reshaped: a transform has
    /// nothing to rework then, and no command is there to choose rows by.
    pub generates: bool,
    /// The phrases a `Follow` can read, and the loop of the one it reads now, which its `Rotate`
    /// goes round and its `Invert` turns about the first note of. Elsewhere, none.
    pub sources: &'a [Source],
    pub source: Option<&'a [Step]>,
}

/// What a setting goes back to with `x` where that hangs on what's there: `From` and `To` to the
/// ends of what's there or, on a column generated on, the ends it starts with, and a `Follow`'s
/// `Source` to the phrase it started on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Starts {
    pub ends: (u8, u8),
    pub source: Option<Source>,
}

impl Context<'_> {
    /// The fills it offers. A transform reworks what's there, so it's left out when nothing is,
    /// and `Follow` when there's no other phrase to follow.
    pub fn fills(&self) -> Vec<FillKind> {
        self.column
            .fills()
            .into_iter()
            .filter(|kind| !(self.generates && kind.reworks()))
            .filter(|kind| *kind != FillKind::Follow || !self.sources.is_empty())
            .collect()
    }

    /// How many rows the source a `Follow` reads has, at least one.
    fn source_rows(&self) -> usize {
        self.source.map_or(1, <[Step]>::len).max(1)
    }

    /// The choices of rows. Where nothing is there, the rows already holding a command aren't one.
    pub fn wheres(&self) -> Vec<Where> {
        self.column
            .wheres()
            .into_iter()
            .filter(|place| !(self.generates && *place == Where::Commands))
            .collect()
    }

    /// The rows it chooses unless told otherwise: every row where nothing is there.
    pub fn default_where(&self) -> Where {
        if self.generates {
            Where::AllRows
        } else {
            self.column.default_where()
        }
    }
}

/// The wave rates a list offers: the note lengths, then whole numbers of cycles.
pub fn listed_rates() -> Vec<WaveRate> {
    note_rates()
        .into_iter()
        .map(WaveRate::Time)
        .chain(LISTED_CYCLES.map(WaveRate::Cycles))
        .collect()
}

/// One of a list setting's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Fill(FillKind),
    Wave(Wave),
    Rate(WaveRate),
    Where(Where),
    Source(Source),
    Vary(u8),
}

impl Choice {
    /// The name on screen.
    pub fn name(self) -> String {
        match self {
            Self::Fill(kind) => kind.name().to_owned(),
            Self::Wave(wave) => wave.name().to_owned(),
            Self::Rate(rate) => rate.to_string(),
            Self::Where(place) => place.name().to_owned(),
            Self::Source(source) => source.to_string(),
            Self::Vary(passes) => vary_name(passes),
        }
    }

    /// What it does, where a list says.
    pub fn description(self) -> Option<&'static str> {
        match self {
            Self::Fill(kind) => Some(kind.description()),
            _ => None,
        }
    }

    /// Give a shape this choice. A `Where` chosen comes with its amount as it starts.
    pub fn apply(self, shape: &mut Shape) {
        match self {
            Self::Fill(kind) => shape.kind = kind,
            Self::Wave(wave) => shape.wave = wave,
            Self::Rate(rate) => shape.rate = rate,
            Self::Where(place) => shape.place = place,
            Self::Source(source) => shape.follow.source = source,
            Self::Vary(passes) => shape.vary = passes,
        }
    }
}

/// How a setting's value is typed after `i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// It isn't: `i` lists its choices, as `K` does.
    Listed,
    /// It isn't: it's one of two, so `i` turns it over.
    Toggled,
    /// Two hex digits, kept as soon as the second is typed.
    Hex,
    /// Hex values, up to [`super::MAX_VALUES`], a space coming by itself after each two digits.
    HexList,
    /// A note's name or a nudge, alone or in a list with spaces between, kept with `Enter`.
    Named { list: bool },
    /// A rate, like `1/4`, `2 bars` or `x3`.
    Rate,
    /// A whole number of up to four digits, with a sign before it if `signed`.
    Whole { signed: bool },
}

/// How a setting's values read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    /// The column's own values: a note's name, a nudge with its sign, or hex.
    Column,
    /// A bend either way, with its sign, from `-100` to `100`.
    Bend,
    /// Semitones, or the scale's degrees while it's on, with a sign.
    Interval,
    /// Round a cycle, in degrees.
    Degrees,
    /// A whole number of rows or notes.
    Count,
    /// So many rows, as an `Every` skips.
    Rows,
    /// So many of the rows the column could change, as Euclid's hits are: `9 of 16`.
    OfRows,
    /// A walk's largest move: on notes in semitones, or the scale's degrees while it's on, and
    /// elsewhere in the column's own values, without a sign.
    Step,
    /// A chance or share, in percent.
    Percent,
    /// One of a list, by name.
    Named,
}

/// A generator's setting. [`GenParam::ALL`] is every one, in the order the shape panel lists
/// them; [`GenParam::exists_for`] says which a shape has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenParam {
    /// What goes in the rows, which decides the rest.
    Fill,
    /// The two values a fill goes between.
    From,
    To,
    /// An `Acid` fill's own: how many notes its riff has; the chances of an octave, of a row
    /// playing on an empty column, of an accent and of a slide; and whether a roll keeps the
    /// pitches.
    Notes,
    Octave,
    Density,
    Accent,
    Slide,
    Pitches,
    /// A `Follow` fill's own: the phrase it follows, the canon's moves on the way (transposed,
    /// inverted about a note, backwards and late), and whether it copies the FX columns too.
    Source,
    Transpose,
    Invert,
    Axis,
    Reverse,
    Late,
    CopyFx,
    /// A `Walk`'s largest move and what it does at the ends, and how many values a `1/f` sums.
    Step,
    Edge,
    Depth,
    /// What `Multiply` multiplies the rhythm by, and whether factors 2 and 3 mask it first.
    Factor,
    Masks,
    /// How a ramp bends, early or late, and how it eases at both ends.
    Curve,
    S,
    /// A `Wave`'s shape, the length of its cycle, and where in it the first row falls.
    Wave,
    Rate,
    Phase,
    /// A `List`'s values, in place of `From` and `To`.
    Values,
    /// How many rows in a row take each value.
    Run,
    /// Which rows get a value, and its amount: an `Every`'s step, a `Chance`'s percent, or
    /// Euclid's hits and rotation.
    Where,
    Every,
    Chance,
    Hits,
    Rotate,
    /// How many passes each of its variations lasts, a new seed each time, for a shape that
    /// draws from its seed: none, each pass, or every two, four or eight.
    Vary,
}

impl GenParam {
    /// Every setting, in the order the shape panel lists them.
    pub const ALL: [Self; 34] = [
        Self::Fill,
        Self::From,
        Self::To,
        Self::Notes,
        Self::Octave,
        Self::Density,
        Self::Accent,
        Self::Slide,
        Self::Pitches,
        Self::Source,
        Self::Transpose,
        Self::Invert,
        Self::Axis,
        Self::Reverse,
        Self::Late,
        Self::CopyFx,
        Self::Step,
        Self::Edge,
        Self::Depth,
        Self::Factor,
        Self::Masks,
        Self::Curve,
        Self::S,
        Self::Wave,
        Self::Rate,
        Self::Phase,
        Self::Values,
        Self::Run,
        Self::Where,
        Self::Every,
        Self::Chance,
        Self::Hits,
        Self::Rotate,
        Self::Vary,
    ];

    /// The name a song or a mapping stores it by, which never changes.
    pub fn key(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::From => "from",
            Self::To => "to",
            Self::Notes => "notes",
            Self::Octave => "octave",
            Self::Density => "density",
            Self::Accent => "accent",
            Self::Slide => "slide",
            Self::Pitches => "pitches",
            Self::Source => "source",
            Self::Transpose => "transpose",
            Self::Invert => "invert",
            Self::Axis => "axis",
            Self::Reverse => "reverse",
            Self::Late => "late",
            Self::CopyFx => "fx",
            Self::Step => "step",
            Self::Edge => "edge",
            Self::Depth => "depth",
            Self::Factor => "factor",
            Self::Masks => "masks",
            Self::Curve => "curve",
            Self::S => "s",
            Self::Wave => "wave",
            Self::Rate => "rate",
            Self::Phase => "phase",
            Self::Values => "values",
            Self::Run => "run",
            Self::Where => "where",
            Self::Every => "every",
            Self::Chance => "chance",
            Self::Hits => "hits",
            Self::Rotate => "rotate",
            Self::Vary => "vary",
        }
    }

    /// The setting stored by a key.
    pub fn by_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|param| param.key() == key)
    }

    /// Its name on screen. An `Every`'s step and a `Chance`'s percent are both its `Where`'s
    /// amount.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fill => "Fill",
            Self::From => "From",
            Self::To => "To",
            Self::Notes => "Notes",
            Self::Octave => "Octave",
            Self::Density => "Density",
            Self::Accent => "Accent",
            Self::Slide => "Slide",
            Self::Pitches => "Pitches",
            Self::Source => "Source",
            Self::Transpose => "Transpose",
            Self::Invert => "Invert",
            Self::Axis => "About",
            Self::Reverse => "Reverse",
            Self::Late => "Rotate",
            Self::CopyFx => "FX",
            Self::Step => "Step",
            Self::Edge => "Edge",
            Self::Depth => "Depth",
            Self::Factor => "Factor",
            Self::Masks => "Masks",
            Self::Curve => "Curve",
            Self::S => "S",
            Self::Wave => "Shape",
            Self::Rate => "Rate",
            Self::Phase => "Phase",
            Self::Values => "Values",
            Self::Run => "Run",
            Self::Where => "Where",
            Self::Every | Self::Chance => "Amount",
            Self::Hits => "Hits",
            Self::Rotate => "Rotate",
            Self::Vary => "Vary",
        }
    }

    /// How its values read.
    pub fn units(self) -> Units {
        match self {
            Self::From | Self::To | Self::Values | Self::Axis => Units::Column,
            Self::Curve | Self::S => Units::Bend,
            Self::Transpose => Units::Interval,
            Self::Phase => Units::Degrees,
            Self::Notes | Self::Run | Self::Rotate | Self::Depth | Self::Factor => Units::Count,
            Self::Step => Units::Step,
            Self::Every | Self::Late => Units::Rows,
            Self::Hits => Units::OfRows,
            Self::Octave | Self::Density | Self::Accent | Self::Slide | Self::Chance => {
                Units::Percent
            }
            Self::Fill
            | Self::Wave
            | Self::Rate
            | Self::Where
            | Self::Pitches
            | Self::Source
            | Self::Invert
            | Self::Reverse
            | Self::CopyFx
            | Self::Edge
            | Self::Masks
            | Self::Vary => Units::Named,
        }
    }

    /// Whether a shape has it. A wave and `Alternate` have their own settings in place of a
    /// curve, a `List` has its values in place of the two ends, a transform but `Scale` works from
    /// the values there with no settings of its own, and a `Where`'s amount only comes with the
    /// choices that have one. `Acid` has its riff's settings, its `Density` only where it
    /// `generates` notes of its own, and keeping the pitches there leaves it no ends or notes to
    /// choose. A `Follow` writes every row from its source, so it has no `Where`, and turns
    /// about a note only while it inverts. `Walk` has its `Step` and `Edge`, and `1/f` its
    /// `Depth`. `Multiply` reads the rhythm itself, so it has its `Factor` and `Masks` and no
    /// `Where`. Only a shape that draws from its seed has `Vary`.
    pub fn exists_for(self, shape: &Shape, generates: bool) -> bool {
        let kind = shape.kind;
        let acid = kind == FillKind::Acid;
        let riff = acid && (generates || !shape.acid.keep);
        let rows = kind.has_where();
        let follow = kind == FillKind::Follow;
        match self {
            Self::Fill => true,
            Self::Where => rows,
            Self::Source
            | Self::Transpose
            | Self::Invert
            | Self::Reverse
            | Self::Late
            | Self::CopyFx => follow,
            Self::Axis => follow && shape.follow.invert,
            Self::From | Self::To => {
                riff || matches!(
                    kind,
                    FillKind::Ramp
                        | FillKind::Random
                        | FillKind::Walk
                        | FillKind::Pink
                        | FillKind::Wave
                        | FillKind::Alternate
                        | FillKind::Scale
                )
            }
            Self::Notes => riff,
            Self::Octave | Self::Accent | Self::Slide | Self::Pitches => acid,
            Self::Density => acid && generates,
            Self::Step | Self::Edge => kind == FillKind::Walk,
            Self::Depth => kind == FillKind::Pink,
            Self::Factor | Self::Masks => kind == FillKind::Multiply,
            Self::Curve | Self::S => {
                matches!(kind, FillKind::Ramp | FillKind::Random | FillKind::Scale)
            }
            Self::Wave | Self::Rate | Self::Phase => kind == FillKind::Wave,
            Self::Values => kind == FillKind::List,
            Self::Run => matches!(
                kind,
                FillKind::Ramp
                    | FillKind::Random
                    | FillKind::Walk
                    | FillKind::Pink
                    | FillKind::Wave
                    | FillKind::Alternate
                    | FillKind::List
            ),
            Self::Every => rows && matches!(shape.place, Where::Every(_)),
            Self::Chance => rows && matches!(shape.place, Where::Chance(_)),
            Self::Hits | Self::Rotate => rows && matches!(shape.place, Where::Euclid { .. }),
            Self::Vary => shape.draws(),
        }
    }

    /// The settings in a generator's title on the Gen page, the ones that change what it is: its
    /// fill, then its `Where`, or for a `Follow`, the phrase it follows, and for `Acid`, its
    /// `Pitches`, as the second die is a mode of the riff's, `Multiply` its fill alone; and last,
    /// for a shape that draws from its seed, `Vary`, what it is from pass to pass.
    pub fn title(shape: &Shape) -> [Option<Self>; GEN_TITLE] {
        let mut title = match shape.kind {
            FillKind::Follow => [Some(Self::Fill), Some(Self::Source), None, None],
            FillKind::Multiply => [Some(Self::Fill), None, None, None],
            FillKind::Acid => [
                Some(Self::Fill),
                Some(Self::Where),
                Some(Self::Pitches),
                None,
            ],
            _ => [Some(Self::Fill), Some(Self::Where), None, None],
        };
        if let Some(free) = title.iter_mut().find(|place| place.is_none())
            && Self::Vary.exists_for(shape, false)
        {
            *free = Some(Self::Vary);
        }
        title
    }

    /// Where a shape's other settings sit on the Gen page, in columns of rows, top to bottom (the
    /// study's §3.2): the fill's own in the first two, and its `Where`'s at the foot of the last,
    /// so they stay put as the fill changes, with `Acid`'s `Density` above them. A random fill's
    /// curve does nothing, so it isn't placed, and only the settings the shape has are.
    pub fn grid(shape: &Shape, generates: bool) -> [[Option<Self>; GEN_ROWS]; GEN_COLUMNS] {
        use GenParam::*;
        let none = [None; GEN_ROWS];
        let fill: [[Option<Self>; GEN_ROWS]; 2] = match shape.kind {
            FillKind::Ramp => [
                [Some(From), Some(To), Some(Run)],
                [Some(Curve), Some(S), None],
            ],
            FillKind::Random | FillKind::Alternate => [[Some(From), Some(To), Some(Run)], none],
            FillKind::Walk => [
                [Some(From), Some(To), Some(Run)],
                [Some(Step), Some(Edge), None],
            ],
            FillKind::Pink => [[Some(From), Some(To), Some(Run)], [Some(Depth), None, None]],
            FillKind::Wave => [
                [Some(From), Some(To), Some(Wave)],
                [Some(Rate), Some(Phase), Some(Run)],
            ],
            FillKind::List => [[Some(Values), Some(Run), None], none],
            FillKind::Acid => [
                [Some(From), Some(To), Some(Notes)],
                [Some(Octave), Some(Accent), Some(Slide)],
            ],
            FillKind::Follow => [
                [Some(Transpose), Some(Invert), Some(Axis)],
                [Some(Reverse), Some(Late), Some(CopyFx)],
            ],
            FillKind::Scale => [[Some(From), Some(To), Some(Curve)], [Some(S), None, None]],
            FillKind::Multiply => [[Some(Factor), Some(Masks), None], none],
            FillKind::Invert | FillKind::Shuffle | FillKind::Reverse | FillKind::Snap => {
                [none, none]
            }
        };
        let mut last = match shape.place {
            super::Where::Every(_) => [None, None, Some(Every)],
            super::Where::Chance(_) => [None, None, Some(Chance)],
            super::Where::Euclid { .. } => [None, Some(Hits), Some(Rotate)],
            _ => none,
        };
        if shape.kind == FillKind::Acid {
            last[0] = Some(Density);
        }
        let has = |param: Option<Self>| param.filter(|param| param.exists_for(shape, generates));
        [fill[0].map(has), fill[1].map(has), last.map(has)]
    }

    /// The settings a shape has, in order.
    pub fn of(shape: &Shape, generates: bool) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|param| param.exists_for(shape, generates))
            .collect()
    }

    /// Its value as the panel shows it: a note's name, `+20` for a bend, `9 of 16` for hits.
    /// A random fill has no curve, so it shows `--` there.
    pub fn shown(self, shape: &Shape, context: &Context) -> String {
        let column = context.column;
        match self {
            Self::From => column.show(shape.from),
            Self::To => column.show(shape.to),
            Self::Curve | Self::S if shape.kind == FillKind::Random => "--".to_owned(),
            Self::Curve => bend(shape.curve),
            Self::S => bend(shape.s),
            Self::Fill => shape.kind.name().to_owned(),
            Self::Wave => shape.wave.name().to_owned(),
            Self::Rate => shape.rate.to_string(),
            Self::Phase => format!("{}°", shape.phase),
            Self::Values => column.show_list(&shape.values),
            Self::Run => shape.run.to_string(),
            Self::Where => shape.place.name().to_owned(),
            Self::Every => match shape.place {
                Where::Every(every) => format!("{every} rows"),
                _ => String::new(),
            },
            Self::Chance => match shape.place {
                Where::Chance(percent) => format!("{percent}%"),
                _ => String::new(),
            },
            Self::Hits => match shape.place {
                Where::Euclid { hits, .. } => {
                    let open = context.open;
                    format!("{} of {open}", usize::from(hits).min(open))
                }
                _ => String::new(),
            },
            Self::Rotate => match shape.place {
                Where::Euclid { rotate, .. } => rotate.to_string(),
                _ => String::new(),
            },
            Self::Notes => shape.acid.notes.to_string(),
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                format!("{}%", acid_chance(shape.acid, self).unwrap_or(0))
            }
            Self::Pitches if shape.acid.keep => "Keep".to_owned(),
            Self::Pitches => "Roll".to_owned(),
            Self::Source => shape.follow.source.to_string(),
            Self::Transpose => {
                let by = shape.follow.transpose;
                let unit = match column {
                    ShapeColumn::Note { scale, .. } if scale.on => "degree",
                    _ => "semitone",
                };
                match by {
                    0 => "0".to_owned(),
                    1 | -1 => format!("{by:+} {unit}"),
                    _ => format!("{by:+} {unit}s"),
                }
            }
            Self::Invert => on_off(shape.follow.invert),
            Self::Axis => match shape.follow.axis {
                Some(note) => column.show(note),
                None => match context.source.and_then(first_note) {
                    Some(note) => format!("First, {}", column.show(note)),
                    None => "First".to_owned(),
                },
            },
            Self::Reverse => on_off(shape.follow.reverse),
            Self::Late => match shape.follow.rotate {
                0 => "0".to_owned(),
                1 => "1 row late".to_owned(),
                rows => format!("{rows} rows late"),
            },
            Self::CopyFx => on_off(shape.follow.fx),
            Self::Step => {
                let step = shape.walk.step;
                match column {
                    ShapeColumn::Note { scale, .. } => {
                        let unit = if scale.on { "degree" } else { "semitone" };
                        let plural = if step == 1 { "" } else { "s" };
                        format!("{step} {unit}{plural}")
                    }
                    ShapeColumn::Nudge => step.to_string(),
                    _ => format!("{step:02X}"),
                }
            }
            Self::Edge => shape.walk.edge.name().to_owned(),
            Self::Depth => shape.pink.depth.to_string(),
            Self::Factor => shape.bits.factor.to_string(),
            Self::Masks => on_off(shape.bits.masks),
            Self::Vary => vary_name(shape.vary),
        }
    }

    /// Its value in four characters or fewer, for a cell with a bar beside it that shows the
    /// rest: a count of rows or hits alone, as `9` for `9 of 16`; an interval with its sign; a
    /// walk's step of notes alone; and `About` as the note it turns about, the first one's while
    /// it's that.
    pub fn brief(self, shape: &Shape, context: &Context) -> String {
        let number = || self.number(shape, context);
        match self.units() {
            Units::Count | Units::Rows | Units::OfRows => {
                number().map_or_else(String::new, |number| number.to_string())
            }
            Units::Interval => number().map_or_else(String::new, |number| format!("{number:+}")),
            // A walk's step of notes in semitones or degrees, as its bar is of two octaves.
            Units::Step if matches!(context.column, ShapeColumn::Note { .. }) => {
                number().map_or_else(String::new, |number| number.to_string())
            }
            Units::Column if self == Self::Axis => {
                number().map_or_else(|| "--".to_owned(), |note| context.column.show(note as u8))
            }
            _ => self.shown(shape, context),
        }
    }

    /// How its value is typed.
    pub fn typing(self, column: ShapeColumn) -> Typing {
        let hex = typed_in_hex(column);
        match self {
            Self::Fill | Self::Wave | Self::Where | Self::Source | Self::Vary => Typing::Listed,
            Self::Pitches
            | Self::Invert
            | Self::Reverse
            | Self::CopyFx
            | Self::Edge
            | Self::Masks => Typing::Toggled,
            Self::Axis => Typing::Named { list: false },
            Self::From | Self::To | Self::Step if hex => Typing::Hex,
            Self::From | Self::To => Typing::Named { list: false },
            Self::Values if hex => Typing::HexList,
            Self::Values => Typing::Named { list: true },
            Self::Rate => Typing::Rate,
            Self::Curve | Self::S | Self::Transpose => Typing::Whole { signed: true },
            _ => Typing::Whole { signed: false },
        }
    }

    /// The choices a list setting offers, in order, or none for a setting that isn't one.
    pub fn choices(self, context: &Context) -> Option<Vec<Choice>> {
        Some(match self {
            Self::Fill => context.fills().into_iter().map(Choice::Fill).collect(),
            Self::Wave => WAVES.into_iter().map(Choice::Wave).collect(),
            Self::Rate => listed_rates().into_iter().map(Choice::Rate).collect(),
            Self::Where => context.wheres().into_iter().map(Choice::Where).collect(),
            Self::Source => context
                .sources
                .iter()
                .copied()
                .map(Choice::Source)
                .collect(),
            Self::Vary => VARIES.into_iter().map(Choice::Vary).collect(),
            _ => return None,
        })
    }

    /// Where the shape is among a list setting's choices: a `Where` by its kind, whatever its
    /// amount. A rate typed that isn't listed is nowhere.
    pub fn chosen(self, shape: &Shape, context: &Context) -> Option<usize> {
        self.choices(context)?
            .into_iter()
            .position(|choice| match choice {
                Choice::Fill(kind) => kind == shape.kind,
                Choice::Wave(wave) => wave == shape.wave,
                Choice::Rate(rate) => rate == shape.rate,
                Choice::Where(place) => {
                    std::mem::discriminant(&place) == std::mem::discriminant(&shape.place)
                }
                Choice::Source(source) => source == shape.follow.source,
                Choice::Vary(passes) => passes == shape.vary,
            })
    }

    /// Move it a small step, or a `large` one, up for a positive `sign` and down otherwise: a note
    /// by a semitone or an octave, or by a degree or an octave of them while the scale is on; a
    /// list to its next choice, going round; a phase and a rotation round their cycle. A `List`'s
    /// values are only typed, so they say how.
    pub fn step(
        self,
        shape: &mut Shape,
        context: &Context,
        sign: i16,
        large: bool,
    ) -> Result<(), &'static str> {
        let sign = i32::from(sign.signum());
        let by = |small: i32, jump: i32| sign * if large { jump } else { small };
        let column = context.column;
        let open = context.open as i32;
        match self {
            Self::From => shape.from = stepped(column, shape.from, sign, large),
            Self::To => shape.to = stepped(column, shape.to, sign, large),
            Self::Curve => shape.curve = bent(shape.curve, by(BEND_NUDGE, BEND_JUMP)),
            Self::S => shape.s = bent(shape.s, by(BEND_NUDGE, BEND_JUMP)),
            Self::Fill | Self::Wave | Self::Where | Self::Source | Self::Vary => {
                let choices = self.choices(context).unwrap_or_default();
                if !choices.is_empty() {
                    let at = self.chosen(shape, context).unwrap_or(0);
                    let next = if sign > 0 { 1 } else { choices.len() - 1 };
                    choices[(at + next) % choices.len()].apply(shape);
                }
            }
            Self::Rate if large => shape.rate = shape.rate.jumped(sign > 0),
            Self::Rate => shape.rate = shape.rate.nudged(sign > 0),
            Self::Phase => {
                shape.phase =
                    (i32::from(shape.phase) + by(PHASE_NUDGE, PHASE_JUMP)).rem_euclid(360) as u16;
            }
            Self::Values => {
                return Err(match column {
                    ShapeColumn::Note { .. } => "Type the notes with i, like C-3 E-3 G-3",
                    ShapeColumn::Nudge => "Type the nudges with i, like -3 0 +3",
                    _ => "Type the values with i, as hex values like 70 40 55",
                });
            }
            Self::Run => {
                let most = i32::from(MAX_RUN);
                shape.run = moved(i32::from(shape.run), by(1, RUN_JUMP), 1, most) as u16;
            }
            Self::Every => {
                if let Where::Every(every) = shape.place {
                    let most = i32::from(MAX_EVERY);
                    let every = moved(i32::from(every), by(1, EVERY_JUMP), 1, most);
                    shape.place = Where::Every(every as u16);
                }
            }
            Self::Chance => {
                if let Where::Chance(percent) = shape.place {
                    let percent = moved(i32::from(percent), by(1, CHANCE_JUMP), 0, 100);
                    shape.place = Where::Chance(percent as u8);
                }
            }
            Self::Hits => {
                if let Where::Euclid { hits, rotate } = shape.place {
                    let hits = i32::from(hits).min(open);
                    let hits = moved(hits, by(1, EUCLID_JUMP), 0, open) as u16;
                    shape.place = Where::Euclid { hits, rotate };
                }
            }
            Self::Rotate => {
                if let Where::Euclid { hits, rotate } = shape.place {
                    let rotate =
                        (i32::from(rotate) + by(1, EUCLID_JUMP)).rem_euclid(open.max(1)) as u16;
                    shape.place = Where::Euclid { hits, rotate };
                }
            }
            Self::Notes => {
                let most = i32::from(MAX_ACID_NOTES);
                let notes = moved(i32::from(shape.acid.notes), by(1, EUCLID_JUMP), 1, most);
                shape.acid.notes = notes as u8;
            }
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                if let Some(percent) = acid_chance_mut(&mut shape.acid, self) {
                    *percent = moved(i32::from(*percent), by(1, CHANCE_JUMP), 0, 100) as u8;
                }
            }
            Self::Pitches => shape.acid.keep = !shape.acid.keep,
            Self::Transpose => {
                let octave = match column {
                    ShapeColumn::Note { scale, .. } if scale.on => {
                        scale.kind.intervals().len() as i32
                    }
                    _ => 12,
                };
                let most = i32::from(MAX_TRANSPOSE);
                let by = by(1, octave);
                shape.follow.transpose =
                    moved(i32::from(shape.follow.transpose), by, -most, most) as i8;
            }
            Self::Invert => shape.follow.invert = !shape.follow.invert,
            // From the first note, it moves as an end does.
            Self::Axis => {
                let from = shape.follow.axis.or(context.source.and_then(first_note));
                shape.follow.axis = from.map(|note| stepped(column, note, sign, large));
            }
            Self::Reverse => shape.follow.reverse = !shape.follow.reverse,
            Self::Late => {
                let rows = context.source_rows() as i32;
                shape.follow.rotate =
                    (i32::from(shape.follow.rotate) + by(1, EUCLID_JUMP)).rem_euclid(rows) as u16;
            }
            Self::CopyFx => shape.follow.fx = !shape.follow.fx,
            // On notes by one or an octave, of degrees while the scale is on; on a nudge by one
            // or four 24ths; anything else by one or `10` in hex.
            Self::Step => {
                let jump = match column {
                    ShapeColumn::Note { scale, .. } if scale.on => {
                        scale.kind.intervals().len() as i32
                    }
                    ShapeColumn::Note { .. } => 12,
                    ShapeColumn::Nudge => 4,
                    _ => HEX_JUMP,
                };
                let most = i32::from(longest_step(column));
                shape.walk.step = moved(i32::from(shape.walk.step), by(1, jump), 1, most) as u8;
            }
            Self::Edge => shape.walk.edge = shape.walk.edge.other(),
            Self::Depth => {
                let (low, high) = (i32::from(MIN_DEPTH), i32::from(MAX_DEPTH));
                shape.pink.depth = moved(i32::from(shape.pink.depth), by(1, 2), low, high) as u8;
            }
            Self::Factor => {
                let most = i32::from(MAX_FACTOR);
                let by = by(1, FACTOR_JUMP);
                shape.bits.factor = moved(i32::from(shape.bits.factor), by, 1, most) as u8;
            }
            Self::Masks => shape.bits.masks = !shape.bits.masks,
        }
        Ok(())
    }

    /// Put it back to what it starts as: `From`, `To` and `Source` to where they `start`, a
    /// `Where` to its column's, and an amount to its own.
    pub fn reset(self, shape: &mut Shape, context: &Context, starts: Starts) {
        let (from, to) = starts.ends;
        match self {
            Self::From => shape.from = from,
            Self::To => shape.to = to,
            Self::Curve => shape.curve = 0,
            Self::S => shape.s = 0,
            Self::Fill => shape.kind = FillKind::Ramp,
            Self::Wave => shape.wave = Wave::Sine,
            Self::Rate => shape.rate = DEFAULT_RATE,
            Self::Phase => shape.phase = 0,
            Self::Values => shape.values = Values::new(&[shape.from, shape.to]),
            Self::Run => shape.run = 1,
            Self::Where => shape.place = context.default_where(),
            Self::Every | Self::Chance => {
                shape.place = match shape.place {
                    Where::Every(_) => Where::Every(4),
                    Where::Chance(_) => Where::Chance(50),
                    place => place,
                };
            }
            Self::Notes => shape.acid.notes = Acid::default().notes,
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                let start = acid_chance(Acid::default(), self);
                if let (Some(percent), Some(start)) =
                    (acid_chance_mut(&mut shape.acid, self), start)
                {
                    *percent = start;
                }
            }
            Self::Pitches => shape.acid.keep = false,
            Self::Source => {
                if let Some(source) = starts.source {
                    shape.follow.source = source;
                }
            }
            Self::Transpose => shape.follow.transpose = 0,
            Self::Invert => shape.follow.invert = false,
            Self::Axis => shape.follow.axis = None,
            Self::Reverse => shape.follow.reverse = false,
            Self::Late => shape.follow.rotate = 0,
            Self::CopyFx => shape.follow.fx = false,
            Self::Step => shape.walk.step = default_step(context.column),
            Self::Edge => shape.walk.edge = Edge::default(),
            Self::Depth => shape.pink.depth = Pink::default().depth,
            Self::Factor => shape.bits.factor = Bits::default().factor,
            Self::Masks => shape.bits.masks = Bits::default().masks,
            Self::Vary => shape.vary = 0,
            Self::Hits => {
                if let Where::Euclid { rotate, .. } = shape.place {
                    shape.place = Where::Euclid {
                        hits: EUCLID_HITS,
                        rotate,
                    };
                }
            }
            Self::Rotate => {
                if let Where::Euclid { hits, .. } = shape.place {
                    shape.place = Where::Euclid { hits, rotate: 0 };
                }
            }
        }
    }

    /// Give it a typed value, or say why the text isn't one. A list setting and `Pitches` aren't
    /// typed, so their text changes nothing.
    pub fn typed(
        self,
        shape: &mut Shape,
        context: &Context,
        text: &str,
    ) -> Result<(), &'static str> {
        let column = context.column;
        let open = context.open as u16;
        let unknown = match column {
            ShapeColumn::Note { .. } => "A note is like C-3, C#3 or c3",
            ShapeColumn::Nudge => "A nudge is like -3, 0 or +B, up to B either way",
            _ => "A value is two hex digits",
        };
        match self {
            Self::From | Self::To => {
                let value = column.parse(text).ok_or(unknown)?;
                if self == Self::From {
                    shape.from = value;
                } else {
                    shape.to = value;
                }
            }
            Self::Curve | Self::S => {
                let value = text
                    .parse::<i16>()
                    .map_err(|_| "Curve and S run from -100 to 100")?;
                let most = i16::from(MAX_BEND);
                let value = value.clamp(-most, most) as i8;
                if self == Self::Curve {
                    shape.curve = value;
                } else {
                    shape.s = value;
                }
            }
            Self::Every | Self::Chance => match (shape.place, text.parse::<u16>()) {
                (Where::Every(_), Ok(every)) if every > 0 => shape.place = Where::Every(every),
                (Where::Chance(_), Ok(percent)) => {
                    shape.place = Where::Chance(percent.min(100) as u8);
                }
                _ => return Err("That amount isn't a whole number above 0"),
            },
            Self::Hits | Self::Rotate => match (shape.place, text.parse::<u16>()) {
                (Where::Euclid { rotate, .. }, Ok(hits)) if self == Self::Hits => {
                    shape.place = Where::Euclid {
                        hits: hits.min(open),
                        rotate,
                    };
                }
                (Where::Euclid { hits, .. }, Ok(rotate)) => {
                    shape.place = Where::Euclid {
                        hits,
                        rotate: rotate % open.max(1),
                    };
                }
                _ => return Err("Hits and Rotate are whole numbers"),
            },
            // A wave here is laid out by rows, so a rate has to be in time with them.
            Self::Rate => {
                shape.rate = text
                    .parse::<WaveRate>()
                    .map_err(|_| "A rate here is like 1/4, 1/8d, 2 bars, 7st, or x3 cycles")?;
            }
            Self::Phase => match text.parse::<u16>() {
                Ok(phase) if phase < 360 => shape.phase = phase,
                _ => return Err("Phase runs from 0 to 359"),
            },
            Self::Notes => match text.parse::<u8>() {
                Ok(notes) if notes > 0 => shape.acid.notes = notes.min(MAX_ACID_NOTES),
                _ => return Err("Notes runs from 1 to 8"),
            },
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                match (acid_chance_mut(&mut shape.acid, self), text.parse::<u16>()) {
                    (Some(percent), Ok(typed)) => *percent = typed.min(100) as u8,
                    _ => return Err("A chance runs from 0 to 100"),
                }
            }
            Self::Run => match text.parse::<i16>() {
                Ok(run) if run > 0 => shape.run = (run as u16).min(MAX_RUN),
                _ => return Err("Run is a whole number above 0"),
            },
            Self::Values => {
                let typed: Option<Vec<u8>> = text
                    .split_whitespace()
                    .map(|value| column.parse(value))
                    .collect();
                match typed {
                    Some(list) if !list.is_empty() => shape.values = Values::new(&list),
                    _ if !typed_in_hex(column) => return Err(unknown),
                    _ => return Err("Values are hex values like 70 40 55, up to 8 of them"),
                }
            }
            Self::Transpose => match text.parse::<i16>() {
                Ok(by) => {
                    let most = i16::from(MAX_TRANSPOSE);
                    shape.follow.transpose = by.clamp(-most, most) as i8;
                }
                Err(_) => return Err("Transpose runs from -48 to 48"),
            },
            Self::Axis => shape.follow.axis = Some(column.parse(text).ok_or(unknown)?),
            Self::Late => match text.parse::<u16>() {
                Ok(rows) => shape.follow.rotate = rows % context.source_rows() as u16,
                Err(_) => return Err("Rotate is a whole number of rows"),
            },
            // In hex on a column whose values are, and as a number on notes and nudges.
            Self::Step => {
                let hex = typed_in_hex(column);
                let typed = if hex {
                    u8::from_str_radix(text, 16)
                } else {
                    text.parse::<u8>()
                };
                match typed {
                    Ok(step) if step > 0 => shape.walk.step = step.min(longest_step(column)),
                    _ if hex => return Err("Step is a hex value above 00"),
                    _ => return Err("Step is a whole number above 0"),
                }
            }
            Self::Depth => match text.parse::<u8>() {
                Ok(depth) if (MIN_DEPTH..=MAX_DEPTH).contains(&depth) => shape.pink.depth = depth,
                _ => return Err("Depth runs from 2 to 8"),
            },
            Self::Factor => match text.parse::<u8>() {
                Ok(factor) if (1..=MAX_FACTOR).contains(&factor) => shape.bits.factor = factor,
                _ => return Err("Factor runs from 1 to 16"),
            },
            Self::Fill
            | Self::Wave
            | Self::Where
            | Self::Pitches
            | Self::Source
            | Self::Invert
            | Self::Reverse
            | Self::CopyFx
            | Self::Edge
            | Self::Masks
            | Self::Vary => {}
        }
        Ok(())
    }

    /// The lowest and highest it takes as a number, for a knob or a macro's range: a list
    /// setting's are its choices' places, one that's off or on is `0` or `1` (`Pitches` `1` to
    /// keep), and `About` is a note. A `List`'s values have none.
    pub fn bounds(self, context: &Context) -> Option<(i32, i32)> {
        let column = context.column;
        let open = context.open as i32;
        let most = i32::from(MAX_BEND);
        Some(match self {
            Self::From | Self::To => (i32::from(column.lowest()), i32::from(column.highest())),
            Self::Curve | Self::S => (-most, most),
            Self::Fill | Self::Wave | Self::Rate | Self::Where | Self::Source => {
                (0, self.choices(context)?.len().checked_sub(1)? as i32)
            }
            Self::Axis => (i32::from(column.lowest()), i32::from(column.highest())),
            Self::Transpose => (-i32::from(MAX_TRANSPOSE), i32::from(MAX_TRANSPOSE)),
            Self::Late => (0, context.source_rows() as i32 - 1),
            Self::Invert | Self::Reverse | Self::CopyFx | Self::Edge | Self::Masks => (0, 1),
            Self::Factor => (1, i32::from(MAX_FACTOR)),
            Self::Step => (1, i32::from(longest_step(column))),
            Self::Depth => (i32::from(MIN_DEPTH), i32::from(MAX_DEPTH)),
            Self::Phase => (0, 359),
            Self::Values => return None,
            Self::Run => (1, i32::from(MAX_RUN)),
            Self::Every => (1, i32::from(MAX_EVERY)),
            Self::Chance | Self::Octave | Self::Density | Self::Accent | Self::Slide => (0, 100),
            Self::Hits => (0, open),
            Self::Rotate => (0, (open - 1).max(0)),
            Self::Notes => (1, i32::from(MAX_ACID_NOTES)),
            Self::Pitches => (0, 1),
            // Its choices' places, worked out without listing them, so the engine can hold it.
            Self::Vary => (0, VARIES.len() as i32 - 1),
        })
    }

    /// Its value as a number within its [`bounds`](Self::bounds), if it has one now: not a
    /// `Where`'s amount the `Where` doesn't have, or a rate typed that isn't listed.
    pub fn number(self, shape: &Shape, context: &Context) -> Option<i32> {
        Some(match self {
            Self::From => i32::from(shape.from),
            Self::To => i32::from(shape.to),
            Self::Curve => i32::from(shape.curve),
            Self::S => i32::from(shape.s),
            Self::Fill | Self::Wave | Self::Rate | Self::Where | Self::Source => {
                self.chosen(shape, context)? as i32
            }
            Self::Axis => i32::from(shape.follow.axis.or(context.source.and_then(first_note))?),
            Self::Transpose => i32::from(shape.follow.transpose),
            Self::Late => i32::from(shape.follow.rotate),
            Self::Invert => i32::from(shape.follow.invert),
            Self::Reverse => i32::from(shape.follow.reverse),
            Self::CopyFx => i32::from(shape.follow.fx),
            Self::Step => i32::from(shape.walk.step),
            Self::Edge => i32::from(shape.walk.edge == Edge::Wrap),
            Self::Depth => i32::from(shape.pink.depth),
            Self::Factor => i32::from(shape.bits.factor),
            Self::Masks => i32::from(shape.bits.masks),
            Self::Phase => i32::from(shape.phase),
            Self::Values => return None,
            Self::Run => i32::from(shape.run),
            Self::Every => match shape.place {
                Where::Every(every) => i32::from(every),
                _ => return None,
            },
            Self::Chance => match shape.place {
                Where::Chance(percent) => i32::from(percent),
                _ => return None,
            },
            Self::Hits => match shape.place {
                Where::Euclid { hits, .. } => i32::from(hits).min(context.open as i32),
                _ => return None,
            },
            Self::Rotate => match shape.place {
                Where::Euclid { rotate, .. } => i32::from(rotate),
                _ => return None,
            },
            Self::Notes => i32::from(shape.acid.notes),
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                i32::from(acid_chance(shape.acid, self)?)
            }
            Self::Pitches => i32::from(shape.acid.keep),
            Self::Vary => VARIES.iter().position(|passes| *passes == shape.vary)? as i32,
        })
    }

    /// Set it to a number, kept within its bounds. A list setting already on that choice keeps
    /// what it has, so a `Where` keeps its amount; one the shape doesn't have changes nothing.
    pub fn set_number(self, shape: &mut Shape, context: &Context, number: i32) {
        let Some((low, high)) = self.bounds(context) else {
            return;
        };
        let number = number.clamp(low, high);
        match self {
            Self::From => shape.from = number as u8,
            Self::To => shape.to = number as u8,
            Self::Curve => shape.curve = number as i8,
            Self::S => shape.s = number as i8,
            Self::Fill | Self::Wave | Self::Rate | Self::Where | Self::Source => {
                if self.chosen(shape, context) != Some(number as usize)
                    && let Some(choice) = self
                        .choices(context)
                        .and_then(|choices| choices.get(number as usize).copied())
                {
                    choice.apply(shape);
                }
            }
            Self::Phase => shape.phase = number as u16,
            Self::Values => {}
            Self::Run => shape.run = number as u16,
            Self::Every => {
                if let Where::Every(_) = shape.place {
                    shape.place = Where::Every(number as u16);
                }
            }
            Self::Chance => {
                if let Where::Chance(_) = shape.place {
                    shape.place = Where::Chance(number as u8);
                }
            }
            Self::Hits => {
                if let Where::Euclid { rotate, .. } = shape.place {
                    shape.place = Where::Euclid {
                        hits: number as u16,
                        rotate,
                    };
                }
            }
            Self::Rotate => {
                if let Where::Euclid { hits, .. } = shape.place {
                    shape.place = Where::Euclid {
                        hits,
                        rotate: number as u16,
                    };
                }
            }
            Self::Notes => shape.acid.notes = number as u8,
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                if let Some(percent) = acid_chance_mut(&mut shape.acid, self) {
                    *percent = number as u8;
                }
            }
            Self::Pitches => shape.acid.keep = number != 0,
            Self::Axis => shape.follow.axis = Some(number as u8),
            Self::Transpose => shape.follow.transpose = number as i8,
            Self::Late => shape.follow.rotate = number as u16,
            Self::Invert => shape.follow.invert = number != 0,
            Self::Reverse => shape.follow.reverse = number != 0,
            Self::CopyFx => shape.follow.fx = number != 0,
            Self::Step => shape.walk.step = number as u8,
            Self::Edge => {
                shape.walk.edge = if number != 0 {
                    Edge::Wrap
                } else {
                    Edge::Reflect
                };
            }
            Self::Depth => shape.pink.depth = number as u8,
            Self::Factor => shape.bits.factor = number as u8,
            Self::Masks => shape.bits.masks = number != 0,
            Self::Vary => shape.vary = VARIES[number as usize],
        }
    }
}

/// How long each of a generator's variations lasts, as `Vary` reads: `Once` for none, as the seed
/// written plays every pass.
fn vary_name(passes: u8) -> String {
    match passes {
        0 => "Once".to_owned(),
        1 => "Each pass".to_owned(),
        passes => format!("Every {passes} passes"),
    }
}

/// Off or on, as a switch reads.
fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.to_owned()
}

/// A bend, with its sign.
fn bend(value: i8) -> String {
    if value > 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}

/// A bend moved, and kept within `MAX_BEND` either way.
fn bent(value: i8, by: i32) -> i8 {
    let most = i32::from(MAX_BEND);
    moved(i32::from(value), by, -most, most) as i8
}

/// A value moved and kept from `low` to `high`.
fn moved(value: i32, by: i32, low: i32, high: i32) -> i32 {
    (value + by).clamp(low, high)
}

/// An end moved a small or a large step: a note by a semitone or an octave, or by a degree or an
/// octave of them while the scale is on; a nudge by one or four 24ths; anything else by one or
/// `10` in hex.
fn stepped(column: ShapeColumn, value: u8, sign: i32, large: bool) -> u8 {
    let jump = match column {
        ShapeColumn::Note { scale, .. } if scale.on => {
            let degrees = if large {
                scale.kind.intervals().len()
            } else {
                1
            };
            return scale.moved(value, degrees as i32 * sign);
        }
        ShapeColumn::Note { .. } => 12,
        ShapeColumn::Nudge => 4,
        _ => HEX_JUMP,
    };
    let (low, top) = (i32::from(column.lowest()), i32::from(column.highest()));
    moved(
        i32::from(value),
        sign * if large { jump } else { 1 },
        low,
        top,
    ) as u8
}

/// The longest `Step` a walk takes on a column: two octaves of notes, or its whole range.
fn longest_step(column: ShapeColumn) -> u8 {
    match column {
        ShapeColumn::Note { .. } => MAX_NOTE_STEP,
        column => column.highest() - column.lowest(),
    }
}

/// Whether a column's values are typed as two hex digits, rather than as notes or nudges.
fn typed_in_hex(column: ShapeColumn) -> bool {
    !matches!(column, ShapeColumn::Note { .. } | ShapeColumn::Nudge)
}

/// The chance in percent an `Acid` setting is: of an octave, a row playing, an accent or a slide.
fn acid_chance(acid: Acid, param: GenParam) -> Option<u8> {
    match param {
        GenParam::Octave => Some(acid.octave),
        GenParam::Density => Some(acid.density),
        GenParam::Accent => Some(acid.accent),
        GenParam::Slide => Some(acid.slide),
        _ => None,
    }
}

fn acid_chance_mut(acid: &mut Acid, param: GenParam) -> Option<&mut u8> {
    match param {
        GenParam::Octave => Some(&mut acid.octave),
        GenParam::Density => Some(&mut acid.density),
        GenParam::Accent => Some(&mut acid.accent),
        GenParam::Slide => Some(&mut acid.slide),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;
    use crate::shape::Follow;

    fn shape(kind: FillKind) -> Shape {
        Shape {
            from: 0x20,
            to: 0x80,
            curve: 0,
            s: 0,
            kind,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::Notes,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: crate::shape::Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            vary: 0,
        }
    }

    fn context(column: ShapeColumn) -> Context<'static> {
        Context {
            column,
            open: 16,
            generates: false,
            sources: &[],
            source: None,
        }
    }

    fn note() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        }
    }

    #[test]
    fn every_setting_has_a_key_of_its_own_and_is_found_by_it() {
        for param in GenParam::ALL {
            assert_eq!(GenParam::by_key(param.key()), Some(param));
            let same = GenParam::ALL.iter().filter(|p| p.key() == param.key());
            assert_eq!(same.count(), 1, "{}", param.key());
        }
        assert_eq!(GenParam::by_key("cutoff"), None);
    }

    #[test]
    fn each_fill_has_its_own_settings_in_the_panels_order() {
        use GenParam::*;
        let of = |kind, generates| GenParam::of(&shape(kind), generates);
        assert_eq!(
            of(FillKind::Ramp, false),
            [Fill, From, To, Curve, S, Run, Where]
        );
        assert_eq!(
            of(FillKind::Wave, false),
            [Fill, From, To, Wave, Rate, Phase, Run, Where]
        );
        assert_eq!(of(FillKind::List, false), [Fill, Values, Run, Where]);
        assert_eq!(
            of(FillKind::Walk, false),
            [Fill, From, To, Step, Edge, Run, Where, Vary]
        );
        assert_eq!(
            of(FillKind::Pink, false),
            [Fill, From, To, Depth, Run, Where, Vary]
        );
        assert_eq!(
            of(FillKind::Scale, false),
            [Fill, From, To, Curve, S, Where]
        );
        assert_eq!(of(FillKind::Invert, false), [Fill, Where]);
        assert_eq!(
            of(FillKind::Multiply, false),
            [Fill, Factor, Masks],
            "Multiply reads the rhythm itself, so it has no Where"
        );
        assert_eq!(
            of(FillKind::Acid, true),
            [
                Fill, From, To, Notes, Octave, Density, Accent, Slide, Pitches, Where, Vary
            ]
        );
        let mut kept = shape(FillKind::Acid);
        kept.acid.keep = true;
        assert_eq!(
            GenParam::of(&kept, false),
            [Fill, Octave, Accent, Slide, Pitches, Where, Vary]
        );
        let mut euclid = shape(FillKind::Alternate);
        euclid.place = crate::shape::Where::Euclid { hits: 3, rotate: 0 };
        assert_eq!(
            GenParam::of(&euclid, false),
            [Fill, From, To, Run, Where, Hits, Rotate]
        );
        euclid.place = crate::shape::Where::Chance(50);
        assert!(
            GenParam::of(&euclid, false).ends_with(&[Where, Chance, Vary]),
            "rows chosen by chance are drawn, so they vary"
        );
        let mut random = shape(FillKind::Wave);
        random.wave = crate::lfo::Shape::Smooth;
        assert!(GenParam::of(&random, false).ends_with(&[Where, Vary]));
        let mut follow = shape(FillKind::Follow);
        assert_eq!(
            GenParam::of(&follow, false),
            [Fill, Source, Transpose, Invert, Reverse, Late, CopyFx],
            "a Follow writes every row, so it has no Where"
        );
        follow.follow.invert = true;
        assert!(GenParam::of(&follow, false).contains(&Axis));
    }

    /// `Vary` lists its choices, goes round them, and as a number is their place, set and read
    /// without listing them, so the engine can hold it.
    #[test]
    fn vary_is_a_list_of_how_many_passes_each_variation_lasts() {
        let mut random = shape(FillKind::Random);
        let velocity = context(ShapeColumn::Velocity);
        let names: Vec<String> = GenParam::Vary
            .choices(&velocity)
            .unwrap()
            .into_iter()
            .map(Choice::name)
            .collect();
        assert_eq!(
            names,
            [
                "Once",
                "Each pass",
                "Every 2 passes",
                "Every 4 passes",
                "Every 8 passes"
            ]
        );
        assert_eq!(GenParam::Vary.shown(&random, &velocity), "Once");
        GenParam::Vary
            .step(&mut random, &velocity, 1, false)
            .unwrap();
        assert_eq!(random.vary, 1);
        GenParam::Vary
            .step(&mut random, &velocity, -1, false)
            .unwrap();
        GenParam::Vary
            .step(&mut random, &velocity, -1, false)
            .unwrap();
        assert_eq!(random.vary, 8, "it goes round");
        assert_eq!(GenParam::Vary.bounds(&velocity), Some((0, 4)));
        assert_eq!(GenParam::Vary.number(&random, &velocity), Some(4));
        GenParam::Vary.set_number(&mut random, &velocity, 2);
        assert_eq!(random.vary, 2);
        assert_eq!(GenParam::Vary.typing(ShapeColumn::Velocity), Typing::Listed);
        GenParam::Vary.reset(&mut random, &velocity, Starts::default());
        assert_eq!(random.vary, 0);
        // A ramp writes the same whatever its seed, so it has nothing to vary.
        assert!(!GenParam::Vary.exists_for(&shape(FillKind::Ramp), false));
        assert_eq!(
            GenParam::title(&random),
            [
                Some(GenParam::Fill),
                Some(GenParam::Where),
                Some(GenParam::Vary),
                None
            ]
        );
        let acid = shape(FillKind::Acid);
        assert_eq!(GenParam::title(&acid)[3], Some(GenParam::Vary));
    }

    #[test]
    fn the_gen_page_places_every_setting_a_shape_has_once() {
        for kind in FillKind::ALL {
            for place in note().wheres() {
                for generates in [false, true] {
                    let mut base = shape(kind);
                    base.place = place;
                    base.follow.invert = true;
                    let placed: Vec<GenParam> = GenParam::title(&base)
                        .into_iter()
                        .chain(GenParam::grid(&base, generates).into_iter().flatten())
                        .flatten()
                        .collect();
                    let mut wanted = GenParam::of(&base, generates);
                    // A random fill's curve does nothing, so the page leaves it out.
                    if kind == FillKind::Random {
                        wanted.retain(|param| !matches!(param, GenParam::Curve | GenParam::S));
                    }
                    for param in &wanted {
                        let times = placed.iter().filter(|found| *found == param).count();
                        assert_eq!(times, 1, "{} on {}", param.key(), kind.name());
                    }
                    assert_eq!(placed.len(), wanted.len(), "{}", kind.name());
                }
            }
        }
    }

    #[test]
    fn a_brief_value_fits_four_characters() {
        let generated = Context {
            generates: true,
            ..context(note())
        };
        for kind in FillKind::ALL {
            for place in note().wheres() {
                let mut base = shape(kind);
                base.place = place;
                base.follow.invert = true;
                base.follow.transpose = -48;
                base.run = MAX_RUN;
                for param in GenParam::of(&base, true) {
                    if param.units() == Units::Named || param == GenParam::Values {
                        continue;
                    }
                    let brief = param.brief(&base, &generated);
                    assert!(brief.chars().count() <= 4, "{}: {brief}", param.key());
                }
            }
        }
    }

    #[test]
    fn values_read_in_the_columns_units() {
        let mut ramp = shape(FillKind::Ramp);
        ramp.from = 60;
        ramp.curve = 20;
        ramp.place = Where::Euclid { hits: 9, rotate: 2 };
        let notes = context(note());
        assert_eq!(GenParam::From.shown(&ramp, &notes), "C-3");
        assert_eq!(GenParam::Curve.shown(&ramp, &notes), "+20");
        assert_eq!(GenParam::Hits.shown(&ramp, &notes), "9 of 16");
        assert_eq!(GenParam::Hits.units(), Units::OfRows);
        ramp.kind = FillKind::Random;
        assert_eq!(GenParam::Curve.shown(&ramp, &notes), "--");
        let velocity = context(ShapeColumn::Velocity);
        assert_eq!(GenParam::To.shown(&ramp, &velocity), "80");
    }

    #[test]
    fn a_step_stays_within_bounds_and_lists_go_round() {
        let velocity = context(ShapeColumn::Velocity);
        let mut ramp = shape(FillKind::Ramp);
        ramp.to = 0x7F;
        GenParam::To.step(&mut ramp, &velocity, 1, true).unwrap();
        assert_eq!(ramp.to, 0x7F, "a velocity stops at its top");
        GenParam::Curve
            .step(&mut ramp, &velocity, -1, true)
            .unwrap();
        assert_eq!(ramp.curve, -25);
        let fills = velocity.fills();
        GenParam::Fill
            .step(&mut ramp, &velocity, -1, false)
            .unwrap();
        assert_eq!(
            ramp.kind,
            *fills.last().unwrap(),
            "back past the first is the last"
        );
        assert!(
            GenParam::Values
                .step(&mut ramp, &velocity, 1, false)
                .is_err()
        );

        ramp.place = Where::Euclid {
            hits: 3,
            rotate: 15,
        };
        GenParam::Rotate
            .step(&mut ramp, &velocity, 1, false)
            .unwrap();
        assert_eq!(ramp.place, Where::Euclid { hits: 3, rotate: 0 });
        GenParam::Hits.step(&mut ramp, &velocity, 1, true).unwrap();
        GenParam::Hits.step(&mut ramp, &velocity, 1, true).unwrap();
        GenParam::Hits.step(&mut ramp, &velocity, 1, true).unwrap();
        GenParam::Hits.step(&mut ramp, &velocity, 1, true).unwrap();
        assert_eq!(
            ramp.place,
            Where::Euclid {
                hits: 16,
                rotate: 0
            }
        );
    }

    #[test]
    fn typing_and_putting_back() {
        let velocity = context(ShapeColumn::Velocity);
        let mut ramp = shape(FillKind::Ramp);
        GenParam::Curve.typed(&mut ramp, &velocity, "-500").unwrap();
        assert_eq!(ramp.curve, -100);
        assert!(GenParam::Phase.typed(&mut ramp, &velocity, "360").is_err());
        GenParam::From.typed(&mut ramp, &velocity, "4a").unwrap();
        assert_eq!(ramp.from, 0x4A);
        let starts = Starts {
            ends: (0x10, 0x70),
            source: None,
        };
        GenParam::From.reset(&mut ramp, &velocity, starts);
        assert_eq!(ramp.from, 0x10);
        assert_eq!(GenParam::From.typing(ShapeColumn::Velocity), Typing::Hex);
        assert_eq!(GenParam::From.typing(note()), Typing::Named { list: false });
        assert_eq!(GenParam::Where.typing(note()), Typing::Listed);
    }

    #[test]
    fn every_number_lands_within_its_bounds_and_reads_back() {
        let generated = Context {
            generates: true,
            ..context(note())
        };
        for kind in FillKind::ALL {
            for place in note().wheres() {
                let mut base = shape(kind);
                base.place = place;
                for param in GenParam::of(&base, true) {
                    let Some((low, high)) = param.bounds(&generated) else {
                        continue;
                    };
                    for number in [low, (low + high) / 2, high] {
                        let mut changed = base;
                        param.set_number(&mut changed, &generated, number);
                        assert_eq!(
                            param.number(&changed, &generated),
                            Some(number),
                            "{} on {}",
                            param.key(),
                            kind.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_list_already_on_its_choice_keeps_its_amount() {
        let velocity = context(ShapeColumn::Velocity);
        let mut ramp = shape(FillKind::Ramp);
        ramp.place = Where::Every(7);
        let at = GenParam::Where.number(&ramp, &velocity).unwrap();
        GenParam::Where.set_number(&mut ramp, &velocity, at);
        assert_eq!(ramp.place, Where::Every(7));
    }
}
