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
//!
//! What's fixed of each setting is a row of [`GEN_PARAMS`]: its key, name and units, how it's
//! typed, its bounds and steps, and how it's read and set as a number (decision 342). What hangs
//! on the shape besides, which shapes have it and how it's shown, put back, typed and chosen from
//! a list, is [`GenParam`]'s own, as each fill's own settings' places are its
//! [`FillSpec`](super::FillSpec)'s.

use crate::grid::{GRID_COLUMNS, GRID_ROWS, Grid};
use crate::lfo::{Shape as Wave, note_rates};
use crate::scale::Scale;
use crate::step::Step;

use super::{
    Acid, Bar, Bits, CellStart, Channel, DEFAULT_RATE, DEFAULT_RULE, Degrees, Division, Divisions,
    DrumPart, EUCLID_HITS, Edge, FillKind, Follow, Grids, MAX_ACID_NOTES, MAX_BEND, MAX_BIAS,
    MAX_DEPTH, MAX_FACTOR, MAX_LOOP, MAX_MOVE_LOOP, MAX_MOVE_WEIGHT, MAX_MUTATION, MAX_ORDER,
    MAX_RING, MAX_SPREAD, MAX_TRANSPOSE, MAX_TURN, MIN_DEPTH, MoveList, Moves, Pink, Quality,
    Random, Ring, Shape, ShapeColumn, Source, Tonnetz, TonnetzVoice, VARIES, Values, Variation,
    WAVES, WaveRate, Where, default_step, degree_count, degree_name, first_note,
};
use super::{CHILD, CLUSTERS, MARKOV, Role, SHARE, STATES, World};

/// How far a small and a large step move a curve, here and in a macro mapping's `Curve` and `S`.
pub const BEND_NUDGE: i32 = 5;
pub const BEND_JUMP: i32 = 25;
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
/// How far a large step moves a `Cells`' rule.
const RULE_JUMP: i32 = 10;
/// How far a large step moves a `Tonnetz`' weights and its loop.
const WEIGHT_JUMP: i32 = 3;
const LOOP_JUMP: i32 = 4;

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
/// ends of what's there or, on a column generated on, the ends it starts with, a `Follow`'s
/// `Source` to the phrase it started on, and a `Grids`' `Part` to the one its track's sound plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Starts {
    pub ends: (u8, u8),
    pub source: Option<Source>,
    pub part: Option<DrumPart>,
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
    Part(DrumPart),
    Channel(Channel),
    Bar(Bar),
    World(World),
    Role(Role),
    CellStart(CellStart),
    Moves(Moves),
    Voice(TonnetzVoice),
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
            Self::Part(part) => part.name().to_owned(),
            Self::Channel(channel) => channel.name().to_owned(),
            Self::Bar(bar) => bar.name().to_owned(),
            Self::World(rhythm) => rhythm.name().to_owned(),
            Self::Role(part) => part.name().to_owned(),
            Self::CellStart(start) => start.name().to_owned(),
            Self::Moves(moves) => moves.name().to_owned(),
            Self::Voice(voice) => voice.name().to_owned(),
            Self::Vary(passes) => vary_name(passes),
        }
    }

    /// What it does, where a list says.
    pub fn description(self) -> Option<&'static str> {
        match self {
            Self::Fill(kind) => Some(kind.description()),
            Self::Part(part) => Some(part.description()),
            Self::Channel(channel) => Some(channel.description()),
            Self::Bar(bar) => Some(bar.description()),
            Self::World(rhythm) => Some(rhythm.description()),
            Self::Role(part) => Some(part.description()),
            Self::CellStart(start) => Some(start.description()),
            Self::Moves(moves) => Some(moves.description()),
            Self::Voice(voice) => Some(voice.description()),
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
            Self::Part(part) => shape.grids.part = Some(part),
            Self::Channel(channel) => shape.divisions.channel = Some(channel),
            Self::Bar(bar) => shape.divisions = shape.divisions.with_bar(bar),
            Self::World(chosen) => {
                if let Where::Rhythm { rhythm, .. } = &mut shape.place {
                    *rhythm = chosen;
                }
            }
            Self::Role(chosen) => {
                if let Where::Rhythm { part, .. } = &mut shape.place {
                    *part = chosen;
                }
            }
            Self::CellStart(start) => {
                if let Where::Cells(cells) = &mut shape.place {
                    cells.start = start;
                }
            }
            Self::Moves(moves) => shape.tonnetz.moves = moves,
            Self::Voice(voice) => shape.tonnetz.voice = Some(voice),
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
    /// A hex digit for each of the scale's degrees, from the root, kept with `Enter`.
    Weights,
    /// The letters of a `Tonnetz`' moves, `P`, `L` and `R`, kept with `Enter`.
    Moves,
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
    /// A byte in hex, `00` to `FF`, as a knob of Grids reads.
    Byte,
    /// One of a list, by name.
    Named,
    /// A byte's bits, in hex, as a mask of them.
    Bits,
    /// A hex digit for each of the scale's degrees, from the root, how often it comes.
    Weights,
}

/// A generator's setting. [`GenParam::ALL`] is every one, in the order the shape panel lists
/// them; [`GenParam::exists_for`] says which a shape has, and [`GEN_PARAMS`] describes each, in
/// the order they're declared here.
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
    /// How many rows each part of an `Acid` line loops on, after Kria: its rhythm on an empty
    /// column, its notes, octaves, accents and slides; none for a part drawn afresh each row.
    TrigLoop,
    NoteLoop,
    OctaveLoop,
    AccentLoop,
    SlideLoop,
    /// A `Follow` fill's own: the phrase it follows, the canon's moves on the way (transposed,
    /// inverted about a note, backwards and late), and whether it copies the FX columns too.
    Source,
    Transpose,
    Invert,
    Axis,
    Reverse,
    Late,
    CopyFx,
    /// A `Follow` that mutates, as a Bloom branch does: how many changes it makes to what it
    /// copies, and which kinds it may make (a note moved in the scale, the line shifted, a span
    /// backwards, a ratchet, a note dropped or added).
    Mutation,
    Repitch,
    Shift,
    Retro,
    Ratchet,
    DropAdd,
    /// A `Walk`'s largest move and what it does at the ends, and how many values a `1/f` sums.
    Step,
    Edge,
    Depth,
    /// A `Random` fill's own, after Marbles: how widely its values spread and which way they
    /// lean, and on notes how many degrees it lets through, the most important first, and how
    /// often each comes. `Spread` and `Bias` also lean the notes an `Acid` riff's set holds.
    Spread,
    Bias,
    Steps,
    Weights,
    /// What `Multiply` multiplies the rhythm by, and whether factors 2 and 3 mask it first.
    Factor,
    Masks,
    /// A `Grids` fill's own: the part its track plays, the note it writes on an empty column, the
    /// place on the map, each part's density, how far chaos raises each step, and whether a row
    /// is a 32nd.
    Part,
    Pitch,
    MapX,
    MapY,
    KickDensity,
    SnareDensity,
    HatDensity,
    Chaos,
    Rows,
    /// A `Divisions` fill's own: the channel its track plays, the bar its dice are fixed for and
    /// how long a row is, and each division's chance, its own channel's (or `A`'s) and then `B`'s,
    /// which only `A+B` plays.
    Channel,
    Bar,
    Quarters,
    Eighths,
    Sixteenths,
    QuarterTriplets,
    EighthTriplets,
    SixteenthTriplets,
    QuartersB,
    EighthsB,
    SixteenthsB,
    QuarterTripletsB,
    EighthTripletsB,
    SixteenthTripletsB,
    /// A `Ring` fill's own: how likely a bit coming round is replaced, how likely a new bit is a
    /// 1, and how many bits go round.
    Change,
    Ones,
    RingLength,
    /// A `Tonnetz`' own: whether its first chord is major or minor, what a track writes of the
    /// chords, where its moves come from, the list of them, each move's weight, the loop's length,
    /// and whether it keeps to the scale.
    Triad,
    Voice,
    Moves,
    MoveList,
    PWeight,
    LWeight,
    RWeight,
    LoopLength,
    InScale,
    /// A `Variation`'s own: how many rows before each row its next hangs on, and how far from
    /// the likeliest it may stray.
    Order,
    Temperature,
    /// The bits `Invert bits` flips and `Mask bits` keeps, and how far `Rotate bits` turns them.
    Flip,
    Keep,
    Turn,
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
    /// Euclid's hits and rotation, or a `Cells`' rule and the row it starts from.
    Where,
    Every,
    Chance,
    Hits,
    Rotate,
    /// The amounts of Marbles' rhythms: how many rows `Clusters` plays, the Bias of `Markov` and
    /// of `3 states`, and the first track's share of `Complementary`'s rows, then which side of
    /// its coin this track is.
    Clusters,
    Markov,
    States,
    Share,
    Side,
    /// A named rhythm's settings: which rhythm, which of its parts this track plays, and
    /// `Child`, the chance each of the part's hits plays.
    Rhythm,
    Role,
    Child,
    Rule,
    CellStart,
    /// How many passes each of its variations lasts, a new seed each time, for a shape that
    /// draws from its seed: none, each pass, or every two, four or eight.
    Vary,
}

impl GenParam {
    /// Every setting, in the order the shape panel lists them.
    pub const ALL: [Self; 99] = [
        Self::Fill,
        Self::From,
        Self::To,
        Self::Notes,
        Self::Spread,
        Self::Bias,
        Self::Octave,
        Self::Density,
        Self::Accent,
        Self::Slide,
        Self::Pitches,
        Self::TrigLoop,
        Self::NoteLoop,
        Self::OctaveLoop,
        Self::AccentLoop,
        Self::SlideLoop,
        Self::Source,
        Self::Transpose,
        Self::Invert,
        Self::Axis,
        Self::Reverse,
        Self::Late,
        Self::CopyFx,
        Self::Mutation,
        Self::Repitch,
        Self::Shift,
        Self::Retro,
        Self::Ratchet,
        Self::DropAdd,
        Self::Step,
        Self::Edge,
        Self::Depth,
        Self::Steps,
        Self::Weights,
        Self::Factor,
        Self::Masks,
        Self::Part,
        Self::Pitch,
        Self::MapX,
        Self::MapY,
        Self::KickDensity,
        Self::SnareDensity,
        Self::HatDensity,
        Self::Chaos,
        Self::Rows,
        Self::Channel,
        Self::Bar,
        Self::Quarters,
        Self::Eighths,
        Self::Sixteenths,
        Self::QuarterTriplets,
        Self::EighthTriplets,
        Self::SixteenthTriplets,
        Self::QuartersB,
        Self::EighthsB,
        Self::SixteenthsB,
        Self::QuarterTripletsB,
        Self::EighthTripletsB,
        Self::SixteenthTripletsB,
        Self::Change,
        Self::Ones,
        Self::RingLength,
        Self::Triad,
        Self::Voice,
        Self::Moves,
        Self::MoveList,
        Self::PWeight,
        Self::LWeight,
        Self::RWeight,
        Self::LoopLength,
        Self::InScale,
        Self::Order,
        Self::Temperature,
        Self::Flip,
        Self::Keep,
        Self::Turn,
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
        Self::Clusters,
        Self::Markov,
        Self::States,
        Self::Share,
        Self::Side,
        Self::Rhythm,
        Self::Role,
        Self::Child,
        Self::Rule,
        Self::CellStart,
        Self::Vary,
    ];

    /// Its description, which the rest of these read.
    #[inline]
    pub const fn spec(self) -> &'static GenParamSpec {
        &GEN_PARAMS[self as usize]
    }

    /// The name a song or a mapping stores it by, which never changes.
    pub const fn key(self) -> &'static str {
        self.spec().key
    }

    /// The setting stored by a key.
    pub fn by_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|param| param.key() == key)
    }

    /// Its name on screen. An `Every`'s step, a `Chance`'s percent and the amounts of Marbles'
    /// rhythms are each its `Where`'s amount.
    pub const fn name(self) -> &'static str {
        self.spec().name
    }

    /// How its values read.
    pub const fn units(self) -> Units {
        self.spec().units
    }

    /// Whether the engine can hold it as it plays, so a macro's lock or modulation reaches it
    /// (decision 300): not a list setting, whose choices are a `Vec` built as they're asked for,
    /// nor a `List`'s values or a `Random`'s weights, which have no number. A macro on `Wave` or
    /// `Rate` writes its column by hand instead (decision 294).
    pub const fn held_in_engine(self) -> bool {
        self.spec().engine
    }
    /// Whether a shape has it. A wave and `Alternate` have their own settings in place of a
    /// curve, a `List` has its values in place of the two ends, a transform but `Scale` works from
    /// the values there with no settings of its own, and a `Where`'s amount only comes with the
    /// choices that have one. `Acid` has its riff's settings, its `Density` only where it
    /// `generates` notes of its own, and keeping the pitches there leaves it no ends or notes to
    /// choose. A `Follow` writes every row from its source, so it has no `Where`, turns about a
    /// note only while it inverts, and has its kinds of mutation only while it mutates. `Walk` has its `Step` and `Edge`, and `1/f` its
    /// `Depth`. `Multiply` reads the rhythm itself, so it has its `Factor` and `Masks` and no
    /// `Where`; each bit transform has its one setting. `Grids` makes its own rhythm, so it has
    /// no `Where` either; it has the density of its own part alone, or while the panel shapes
    /// several tracks, each taking its own part, every part's and no `Part`; and its `Note` only
    /// where it `generates`, as a hit takes the note there or the first one. `Random` has its
    /// `Spread` and `Bias` on every `column`, and its `Steps` and `Weights` on the Note column,
    /// where there are degrees to let through; `Acid` has `Spread` and `Bias` for its riff's
    /// notes, so wherever it has `Notes`. A `Variation` learns its rows with its notes, so
    /// it has its `Order` and `Temperature` and no `Where`. `Divisions` makes its own rhythm too:
    /// it has its bar and its own channel's chances, and channel `B`'s as well where it plays
    /// `A+B`, or while the panel shapes several tracks, each taking its own channel, when it has
    /// no `Channel`; and its `Note` where it `generates`, as `Grids` has. A `Ring` makes its own
    /// rhythm from its register, so it has its ends, `Change`, `Chance` and `Length` and no
    /// `Where`. A `Tonnetz` has its first chord's root, `From`, and no `To`; its `Triad`, its
    /// `Moves` and its `Scale`; its list of moves while it goes through them, and the moves'
    /// weights while it draws them, with a loop's `Length` while it loops; and its `Voice` but
    /// while the panel shapes several tracks, each taking its own. Only a shape that draws from its
    /// seed has `Vary`, which a `Ring` hasn't, as it runs on from pass to pass by itself.
    pub fn exists_for(self, shape: &Shape, column: ShapeColumn, generates: bool) -> bool {
        let kind = shape.kind;
        let grids = kind == FillKind::Grids;
        let divisions = kind == FillKind::Divisions;
        let both =
            divisions && !matches!(shape.divisions.channel, Some(Channel::A) | Some(Channel::B));
        let density = |part: DrumPart| grids && shape.grids.part.is_none_or(|own| own == part);
        let acid = kind == FillKind::Acid;
        let riff = acid && (generates || !shape.acid.keep);
        let rows = kind.has_where();
        let follow = kind == FillKind::Follow;
        let tonnetz = kind == FillKind::Tonnetz;
        match self {
            Self::Fill => true,
            Self::Where => rows,
            Self::Source
            | Self::Transpose
            | Self::Invert
            | Self::Reverse
            | Self::Late
            | Self::CopyFx
            | Self::Mutation => follow,
            Self::Axis => follow && shape.follow.invert,
            Self::Repitch | Self::Shift | Self::Retro | Self::Ratchet | Self::DropAdd => {
                follow && shape.follow.mutation > 0
            }
            Self::From if tonnetz => true,
            Self::From | Self::To => {
                riff || matches!(
                    kind,
                    FillKind::Ring
                        | FillKind::Ramp
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
            Self::Density | Self::TrigLoop => acid && generates,
            Self::NoteLoop => riff,
            Self::OctaveLoop | Self::AccentLoop | Self::SlideLoop => acid,
            Self::Step | Self::Edge => kind == FillKind::Walk,
            Self::Depth => kind == FillKind::Pink,
            Self::Spread | Self::Bias => riff || kind == FillKind::Random,
            Self::Steps | Self::Weights => {
                kind == FillKind::Random && matches!(column, ShapeColumn::Note { .. })
            }
            Self::Factor | Self::Masks => kind == FillKind::Multiply,
            Self::Part => grids && shape.grids.part.is_some(),
            Self::Pitch => (grids || divisions) && generates,
            Self::Channel => divisions && shape.divisions.channel.is_some(),
            Self::Bar
            | Self::Quarters
            | Self::Eighths
            | Self::Sixteenths
            | Self::QuarterTriplets
            | Self::EighthTriplets
            | Self::SixteenthTriplets => divisions,
            Self::QuartersB
            | Self::EighthsB
            | Self::SixteenthsB
            | Self::QuarterTripletsB
            | Self::EighthTripletsB
            | Self::SixteenthTripletsB => both,
            Self::MapX | Self::MapY | Self::Chaos | Self::Rows => grids,
            Self::KickDensity => density(DrumPart::Kick),
            Self::SnareDensity => density(DrumPart::Snare),
            Self::HatDensity => density(DrumPart::Hats),
            Self::Change | Self::Ones | Self::RingLength => kind == FillKind::Ring,
            Self::Triad | Self::Moves | Self::InScale => tonnetz,
            Self::Voice => tonnetz && shape.tonnetz.voice.is_some(),
            Self::MoveList => tonnetz && shape.tonnetz.moves == Moves::List,
            Self::PWeight | Self::LWeight | Self::RWeight => tonnetz && shape.tonnetz.draws(),
            Self::LoopLength => tonnetz && shape.tonnetz.moves == Moves::Loop,
            Self::Order | Self::Temperature => kind == FillKind::Variation,
            Self::Flip => kind == FillKind::InvertBits,
            Self::Keep => kind == FillKind::MaskBits,
            Self::Turn => kind == FillKind::RotateBits,
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
            Self::Clusters => rows && matches!(shape.place, Where::Clusters(_)),
            Self::Markov => rows && matches!(shape.place, Where::Markov(_)),
            Self::States => rows && matches!(shape.place, Where::States(_)),
            Self::Share | Self::Side => rows && matches!(shape.place, Where::Complementary { .. }),
            Self::Rhythm | Self::Role | Self::Child => {
                rows && matches!(shape.place, Where::Rhythm { .. })
            }
            Self::Rule => rows && matches!(shape.place, Where::Cells(_)),
            // Across tracks, the row they start from is worked out once, for all of them.
            Self::CellStart => {
                rows && matches!(shape.place, Where::Cells(cells) if cells.across.is_none())
            }
            Self::Vary => shape.can_vary(),
        }
    }

    /// The settings in a generator's title on the Gen page, the ones that change what it is: its
    /// fill, then its `Where`, or for a `Follow`, the phrase it follows, and for `Acid`, its
    /// `Pitches`, as the second die is a mode of the riff's, `Multiply` its fill alone and `Grids`
    /// its part, which drum it is, and a `Variation` and a `Ring` their fill alone; and last, for a shape that draws from its seed, `Vary`, what
    /// it is from pass to pass. It's a row as wide as the grid under it.
    pub fn title(shape: &Shape) -> [Option<Self>; GRID_COLUMNS] {
        let mut title = match shape.kind {
            FillKind::Follow => [Some(Self::Fill), Some(Self::Source), None, None],
            FillKind::Multiply | FillKind::Variation | FillKind::Ring => {
                [Some(Self::Fill), None, None, None]
            }
            FillKind::Grids => [Some(Self::Fill), Some(Self::Part), None, None],
            FillKind::Divisions => [Some(Self::Fill), Some(Self::Channel), Some(Self::Bar), None],
            FillKind::Acid => [
                Some(Self::Fill),
                Some(Self::Where),
                Some(Self::Pitches),
                None,
            ],
            _ => [Some(Self::Fill), Some(Self::Where), None, None],
        };
        // A shape has `Vary` where it draws from its seed or evolves, whatever its column.
        if let Some(free) = title.iter_mut().find(|place| place.is_none())
            && shape.can_vary()
        {
            *free = Some(Self::Vary);
        }
        title
    }

    /// Where a shape's other settings sit on the Gen page, in columns of rows, top to bottom (the
    /// study's §3.2): the fill's own in the first two, as its [`FillSpec`](super::FillSpec) places
    /// them, and its `Where`'s at the foot of the third, so they stay put as the fill changes,
    /// with `Acid`'s `Density` or a random fill's `Weights` above them. A random fill's curve does
    /// nothing, so it isn't placed, and only the settings the shape has on its `column` are.
    /// `Grids` has the map and chaos first, then its part's density, and has no `Where`.
    /// `Divisions` has its channel's chances, then `B`'s where it plays `A+B`. A `Follow` has no
    /// `Where` either, so its `Mutation` and the kinds it may make take the third column and the
    /// fourth. `Acid`'s three are full, so its riff's `Spread` and `Bias` take the fourth. A
    /// `Tonnetz` has its first chord and its voice, then its moves, and its moves' weights in the
    /// fourth.
    pub fn grid(shape: &Shape, column: ShapeColumn, generates: bool) -> Grid<Self> {
        use GenParam::*;
        let none = [None; GRID_ROWS];
        let has =
            |param: Option<Self>| param.filter(|param| param.exists_for(shape, column, generates));
        let mut fill = shape.kind.spec().grid;
        let mut last = none;
        if shape.kind.has_where() {
            last = Self::of_where(shape.place);
        }
        let mut more = none;
        // A named rhythm's name, where the fill takes the top of the third column, goes to the
        // fourth.
        let rhythm = last[0];
        match shape.kind {
            FillKind::Grids => match shape.grids.part {
                Some(part) => fill[1] = [Some(Self::density_of(part)), Some(Pitch), Some(Rows)],
                None => last = [Some(Pitch), Some(Rows), None],
            },
            FillKind::Divisions => match shape.divisions.channel {
                // `A+B`'s twelve chances fill the page, so its `Note` is the panel's alone.
                Some(super::Channel::A | super::Channel::B) => last = [Some(Pitch), None, None],
                _ => {
                    last = [Some(QuartersB), Some(EighthsB), Some(SixteenthsB)];
                    more = [
                        Some(QuarterTripletsB),
                        Some(EighthTripletsB),
                        Some(SixteenthTripletsB),
                    ];
                }
            },
            FillKind::Acid => {
                last[0] = Some(Density);
                more = [Some(Spread), Some(Bias), rhythm];
            }
            FillKind::Random => {
                last[0] = Some(Weights);
                more[0] = rhythm;
            }
            FillKind::Follow => {
                last = [Some(Mutation), Some(Repitch), Some(Shift)];
                more = [Some(Retro), Some(Ratchet), Some(DropAdd)];
            }
            FillKind::Tonnetz => {
                if shape.tonnetz.moves == super::Moves::Loop {
                    fill[1][1] = Some(LoopLength);
                }
                more = [Some(PWeight), Some(LWeight), Some(RWeight)];
            }
            _ => {}
        }
        let columns = [fill[0], fill[1], last, more];
        Grid::from_columns(&columns.map(|places| places.map(has)))
    }

    /// A `Where`'s own settings, as the foot of the Gen page's third column has them, top to
    /// bottom: none for one that's only its choice of rows.
    pub fn of_where(place: Where) -> [Option<Self>; GRID_ROWS] {
        match place {
            Where::Every(_) => [None, None, Some(Self::Every)],
            Where::Chance(_) => [None, None, Some(Self::Chance)],
            Where::Euclid { .. } => [None, Some(Self::Hits), Some(Self::Rotate)],
            Where::Clusters(_) => [None, None, Some(Self::Clusters)],
            Where::Markov(_) => [None, None, Some(Self::Markov)],
            Where::States(_) => [None, None, Some(Self::States)],
            Where::Complementary { .. } => [None, Some(Self::Share), Some(Self::Side)],
            Where::Rhythm { .. } => [Some(Self::Rhythm), Some(Self::Role), Some(Self::Child)],
            Where::Cells(_) => [None, Some(Self::Rule), Some(Self::CellStart)],
            Where::Notes | Where::NoNotes | Where::Commands | Where::AllRows => [None; GRID_ROWS],
        }
    }

    /// Whether it's one of an `Acid` line's loops.
    pub fn is_loop(self) -> bool {
        matches!(
            self,
            Self::TrigLoop | Self::NoteLoop | Self::OctaveLoop | Self::AccentLoop | Self::SlideLoop
        )
    }

    /// Where an `Acid` line's loops sit on the Gen page, on a device of their own under the
    /// fill's, as its own grid is full: the rhythm's and the notes' first, then the octaves',
    /// accents' and slides', each level with its chance in the device above. Only the loops the
    /// shape has on its `column` are placed, and a fill with no loops has none.
    pub fn loops_grid(shape: &Shape, column: ShapeColumn, generates: bool) -> Option<Grid<Self>> {
        if shape.kind != FillKind::Acid {
            return None;
        }
        let has =
            |param: Option<Self>| param.filter(|param| param.exists_for(shape, column, generates));
        let grid = [
            [Some(Self::TrigLoop), Some(Self::NoteLoop), None],
            [
                Some(Self::OctaveLoop),
                Some(Self::AccentLoop),
                Some(Self::SlideLoop),
            ],
        ];
        Some(Grid::from_columns(&grid.map(|places| places.map(has))))
    }

    /// A part's density.
    pub fn density_of(part: DrumPart) -> Self {
        match part {
            DrumPart::Kick => Self::KickDensity,
            DrumPart::Snare => Self::SnareDensity,
            DrumPart::Hats => Self::HatDensity,
        }
    }

    /// The settings a shape has on a column, in order.
    pub fn of(shape: &Shape, column: ShapeColumn, generates: bool) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|param| param.exists_for(shape, column, generates))
            .collect()
    }

    /// Its value as the panel shows it: a note's name, `+20` for a bend, `9 of 16` for hits.
    /// A random fill has no curve, so it shows `--` there.
    pub fn shown(self, shape: &Shape, context: &Context) -> String {
        let column = context.column;
        let number = || self.number(shape, context);
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
            Self::Clusters | Self::Markov | Self::States | Self::Share | Self::Child => {
                number().map_or_else(String::new, |amount| format!("{amount}%"))
            }
            Self::Side => match shape.place {
                Where::Complementary { second: true, .. } => "Second".to_owned(),
                Where::Complementary { .. } => "First".to_owned(),
                _ => String::new(),
            },
            Self::Rhythm => match shape.place {
                Where::Rhythm { rhythm, .. } => rhythm.name().to_owned(),
                _ => String::new(),
            },
            Self::Role => match shape.place {
                Where::Rhythm { part, .. } => part.name().to_owned(),
                _ => String::new(),
            },
            Self::Rule => match shape.place {
                Where::Cells(cells) => cells.rule.to_string(),
                _ => String::new(),
            },
            Self::CellStart => match shape.place {
                Where::Cells(cells) => cells.start.name().to_owned(),
                _ => String::new(),
            },
            Self::Notes => shape.acid.notes.to_string(),
            Self::Octave | Self::Density | Self::Accent | Self::Slide => {
                format!("{}%", number().unwrap_or(0))
            }
            Self::Pitches if shape.acid.keep => "Keep".to_owned(),
            Self::Pitches => "Roll".to_owned(),
            Self::TrigLoop
            | Self::NoteLoop
            | Self::OctaveLoop
            | Self::AccentLoop
            | Self::SlideLoop => match number().unwrap_or(0) {
                0 => "Off".to_owned(),
                1 => "1 row".to_owned(),
                rows => format!("{rows} rows"),
            },
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
            Self::Mutation => match shape.follow.mutation {
                0 => "Off".to_owned(),
                1 => "1 change".to_owned(),
                changes => format!("{changes} changes"),
            },
            Self::Repitch | Self::Shift | Self::Retro | Self::Ratchet | Self::DropAdd => {
                on_off(number() == Some(1))
            }
            Self::Step => {
                let step = shape.walk.step;
                match column {
                    ShapeColumn::Note { scale, .. } => {
                        let unit = if scale.on { "degree" } else { "semitone" };
                        let plural = if step == 1 { "" } else { "s" };
                        format!("{step} {unit}{plural}")
                    }
                    ShapeColumn::Nudge { .. } => step.to_string(),
                    _ => format!("{step:02X}"),
                }
            }
            Self::Edge => shape.walk.edge.name().to_owned(),
            Self::Depth => shape.pink.depth.to_string(),
            Self::Spread => format!("{}%", shape.random.spread),
            Self::Bias => bend(shape.random.bias),
            Self::Steps => match column {
                ShapeColumn::Note { scale, .. } => steps_name(shape.random, scale),
                _ => String::new(),
            },
            Self::Weights => match column {
                ShapeColumn::Note { scale, .. } => shape.random.weights_text(scale),
                _ => String::new(),
            },
            Self::Factor => shape.bits.factor.to_string(),
            Self::Masks => on_off(shape.bits.masks),
            Self::Part => shape
                .grids
                .part
                .map_or("By track", DrumPart::name)
                .to_owned(),
            Self::Pitch => column.show(shape.from),
            Self::MapX
            | Self::MapY
            | Self::KickDensity
            | Self::SnareDensity
            | Self::HatDensity
            | Self::Chaos => format!("{:02X}", number().unwrap_or(0)),
            Self::Rows if shape.grids.fine => "1/32".to_owned(),
            Self::Rows => "1/16".to_owned(),
            Self::Channel => shape
                .divisions
                .channel
                .map_or("By track", Channel::name)
                .to_owned(),
            Self::Bar => shape.divisions.bar.name().to_owned(),
            Self::Quarters
            | Self::Eighths
            | Self::Sixteenths
            | Self::QuarterTriplets
            | Self::EighthTriplets
            | Self::SixteenthTriplets
            | Self::QuartersB
            | Self::EighthsB
            | Self::SixteenthsB
            | Self::QuarterTripletsB
            | Self::EighthTripletsB
            | Self::SixteenthTripletsB => {
                format!("{}%", number().unwrap_or(0))
            }
            Self::Order => shape.variation.order.to_string(),
            Self::Temperature => format!("{}%", shape.variation.temperature),
            Self::Change => format!("{}%", shape.ring.change),
            Self::Ones => format!("{}%", shape.ring.chance),
            Self::RingLength => shape.ring.length.to_string(),
            Self::Triad => shape.tonnetz.quality.name().to_owned(),
            Self::Voice => shape
                .tonnetz
                .voice
                .map_or("By track", TonnetzVoice::name)
                .to_owned(),
            Self::Moves => shape.tonnetz.moves.name().to_owned(),
            Self::MoveList => shape.tonnetz.list.to_string(),
            Self::PWeight | Self::LWeight | Self::RWeight => number().unwrap_or(0).to_string(),
            Self::LoopLength => match shape.tonnetz.length {
                1 => "1 move".to_owned(),
                moves => format!("{moves} moves"),
            },
            Self::InScale if shape.tonnetz.in_scale => "In scale".to_owned(),
            Self::InScale => "Free".to_owned(),
            Self::Flip => format!("{:02X}", shape.bits.flip),
            Self::Keep => format!("{:02X}", shape.bits.keep),
            Self::Turn => match shape.bits.turn {
                1 => "1 bit".to_owned(),
                turn => format!("{turn} bits"),
            },
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
            _ if self.is_loop() => match number() {
                Some(0) | None => "Off".to_owned(),
                Some(rows) => rows.to_string(),
            },
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

    /// How its value is typed: an end, a walk's step and a `List`'s values in hex on a column
    /// whose values are.
    pub fn typing(self, column: ShapeColumn) -> Typing {
        match self {
            Self::From | Self::To | Self::Step if typed_in_hex(column) => Typing::Hex,
            Self::Values if typed_in_hex(column) => Typing::HexList,
            _ => self.spec().typing,
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
            Self::Part => DrumPart::ALL.into_iter().map(Choice::Part).collect(),
            Self::Channel => Channel::ALL.into_iter().map(Choice::Channel).collect(),
            Self::Bar => Bar::ALL.into_iter().map(Choice::Bar).collect(),
            Self::Rhythm => World::ALL.into_iter().map(Choice::World).collect(),
            Self::Role => Role::ALL.into_iter().map(Choice::Role).collect(),
            Self::CellStart => CellStart::ALL.into_iter().map(Choice::CellStart).collect(),
            Self::Moves => Moves::ALL.into_iter().map(Choice::Moves).collect(),
            Self::Voice => TonnetzVoice::ALL.into_iter().map(Choice::Voice).collect(),
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
                Choice::Part(part) => Some(part) == shape.grids.part,
                Choice::Channel(channel) => Some(channel) == shape.divisions.channel,
                Choice::Bar(bar) => bar == shape.divisions.bar,
                Choice::World(chosen) => {
                    matches!(shape.place, Where::Rhythm { rhythm, .. } if rhythm == chosen)
                }
                Choice::Role(chosen) => {
                    matches!(shape.place, Where::Rhythm { part, .. } if part == chosen)
                }
                Choice::CellStart(start) => {
                    matches!(shape.place, Where::Cells(cells) if cells.start == start)
                }
                Choice::Moves(moves) => moves == shape.tonnetz.moves,
                Choice::Voice(voice) => Some(voice) == shape.tonnetz.voice,
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
        let column = context.column;
        match self {
            Self::From => shape.from = stepped(column, shape.from, sign, large),
            Self::To => shape.to = stepped(column, shape.to, sign, large),
            Self::Pitch => shape.from = stepped(column, shape.from, sign, large),
            Self::Rate if large => shape.rate = shape.rate.jumped(sign > 0),
            Self::Rate => shape.rate = shape.rate.nudged(sign > 0),
            Self::Values => {
                return Err(match column {
                    ShapeColumn::Note { .. } => "Type the notes with i, like C-3 E-3 G-3",
                    ShapeColumn::Nudge { .. } => "Type the nudges with i, like -3 0 +3",
                    _ => "Type the values with i, as hex values like 70 40 55",
                });
            }
            // From the first note, it moves as an end does.
            Self::Axis => {
                let from = shape.follow.axis.or(context.source.and_then(first_note));
                shape.follow.axis = from.map(|note| stepped(column, note, sign, large));
            }
            Self::Weights => {
                return Err(
                    "Type the weights with i, a hex digit a degree from the root, like F8C4",
                );
            }
            Self::MoveList => return Err("Type the moves with i, like P L R L"),
            _ if self.spec().typing == Typing::Listed => {
                let choices = self.choices(context).unwrap_or_default();
                if !choices.is_empty() {
                    let at = self.chosen(shape, context).unwrap_or(0);
                    let next = if sign > 0 { 1 } else { choices.len() - 1 };
                    choices[(at + next) % choices.len()].apply(shape);
                }
            }
            _ => {
                if let (Some(stepping), Some(number)) =
                    (self.spec().stepping, self.number(shape, context))
                {
                    let by = if large {
                        stepping.jump.on(column)
                    } else {
                        stepping.nudge
                    };
                    self.put(shape, context, number + sign * by);
                }
            }
        }
        Ok(())
    }

    /// Set it to a number stepped or typed: taken round within its bounds where it goes round
    /// past its ends, then kept within them by [`set_number`](Self::set_number), as everything
    /// that sets it is.
    fn put(self, shape: &mut Shape, context: &Context, number: i32) {
        let round = self.spec().stepping.is_some_and(|stepping| stepping.round);
        let number = match self.bounds(context) {
            Some((low, high)) if round => low + (number - low).rem_euclid(high - low + 1),
            _ => number,
        };
        self.set_number(shape, context, number);
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
                let start = Shape {
                    acid: Acid::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
            Self::Pitches => shape.acid.keep = false,
            Self::TrigLoop
            | Self::NoteLoop
            | Self::OctaveLoop
            | Self::AccentLoop
            | Self::SlideLoop => (self.spec().set)(shape, context, 0),
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
            Self::Mutation => shape.follow.mutation = 0,
            Self::Repitch | Self::Shift | Self::Retro | Self::Ratchet | Self::DropAdd => {
                let start = Shape {
                    follow: Follow::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
            Self::Step => shape.walk.step = default_step(context.column),
            Self::Edge => shape.walk.edge = Edge::default(),
            Self::Depth => shape.pink.depth = Pink::default().depth,
            Self::Spread => shape.random.spread = Random::default().spread,
            Self::Bias => shape.random.bias = Random::default().bias,
            Self::Steps => shape.random.steps = Random::default().steps,
            Self::Weights => shape.random.weights = Random::default().weights,
            Self::Factor => shape.bits.factor = Bits::default().factor,
            Self::Masks => shape.bits.masks = Bits::default().masks,
            Self::Part => shape.grids.part = Some(starts.part.unwrap_or_default()),
            Self::Pitch => shape.from = from,
            Self::MapX
            | Self::MapY
            | Self::KickDensity
            | Self::SnareDensity
            | Self::HatDensity
            | Self::Chaos => {
                let start = Shape {
                    grids: Grids::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
            Self::Rows => shape.grids.fine = Grids::default().fine,
            Self::Channel => shape.divisions.channel = Some(Channel::default()),
            Self::Bar => shape.divisions = shape.divisions.with_bar(Bar::default()),
            Self::Quarters
            | Self::Eighths
            | Self::Sixteenths
            | Self::QuarterTriplets
            | Self::EighthTriplets
            | Self::SixteenthTriplets
            | Self::QuartersB
            | Self::EighthsB
            | Self::SixteenthsB
            | Self::QuarterTripletsB
            | Self::EighthTripletsB
            | Self::SixteenthTripletsB => {
                let start = Shape {
                    divisions: Divisions::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
            Self::Order => shape.variation.order = Variation::default().order,
            Self::Temperature => shape.variation.temperature = Variation::default().temperature,
            Self::Change => shape.ring.change = Ring::default().change,
            Self::Ones => shape.ring.chance = Ring::default().chance,
            Self::RingLength => shape.ring.length = Ring::default().length,
            Self::Triad => shape.tonnetz.quality = Quality::default(),
            Self::Moves => shape.tonnetz.moves = Moves::default(),
            Self::MoveList => shape.tonnetz.list = MoveList::default(),
            Self::PWeight | Self::LWeight | Self::RWeight => {
                let start = Shape {
                    tonnetz: Tonnetz::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
            Self::LoopLength => shape.tonnetz.length = Tonnetz::default().length,
            Self::InScale => shape.tonnetz.in_scale = false,
            Self::Flip => shape.bits.flip = Bits::default().flip,
            Self::Keep => shape.bits.keep = Bits::default().keep,
            Self::Turn => shape.bits.turn = Bits::default().turn,
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
            Self::Clusters | Self::Markov | Self::States | Self::Share | Self::Child => {
                let start = match self {
                    Self::Clusters => CLUSTERS,
                    Self::Markov => MARKOV,
                    Self::States => STATES,
                    Self::Child => CHILD,
                    _ => SHARE,
                };
                (self.spec().set)(shape, context, i32::from(start));
            }
            // Which side of the coin a track is, so the other track of the two keeps its own, and
            // which part of a rhythm or voice of a chord it plays, so the others keep theirs.
            Self::Side | Self::Role | Self::Voice => {}
            Self::Rhythm => Choice::World(World::default()).apply(shape),
            Self::Rule => {
                if let Where::Cells(cells) = &mut shape.place {
                    cells.rule = DEFAULT_RULE;
                }
            }
            Self::CellStart => {
                if let Where::Cells(cells) = &mut shape.place {
                    cells.start = CellStart::default();
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
        let unknown = match column {
            ShapeColumn::Note { .. } => "A note is like C-3, C#3 or c3",
            ShapeColumn::Nudge { .. } => "A nudge is a sign and hex, like -3, 0 or +B",
            _ => "A value is two hex digits",
        };
        // Whether a number typed is one it takes, for a setting that refuses the rest rather
        // than keeping them within its bounds.
        let within = |number: u16| {
            self.bounds(context)
                .is_some_and(|(low, high)| (low..=high).contains(&i32::from(number)))
        };
        let number: i32 = match self {
            Self::From | Self::To | Self::Pitch => {
                let value = column.parse(text).ok_or(unknown)?;
                if self != Self::To {
                    shape.from = value;
                } else {
                    shape.to = value;
                }
                return Ok(());
            }
            Self::Curve | Self::S => text
                .parse::<i16>()
                .map_err(|_| "Curve and S run from -100 to 100")?
                .into(),
            // The amount of the `Where` it has, whichever of the two was typed into.
            Self::Every | Self::Chance => {
                let (param, amount) = match (shape.place, text.parse::<u16>()) {
                    (Where::Every(_), Ok(every)) if every > 0 => (Self::Every, every),
                    (Where::Chance(_), Ok(percent)) => (Self::Chance, percent),
                    _ => return Err("That amount isn't a whole number above 0"),
                };
                param.put(shape, context, amount.into());
                return Ok(());
            }
            Self::Hits | Self::Rotate => match (shape.place, text.parse::<u16>()) {
                (Where::Euclid { .. }, Ok(number)) => number.into(),
                _ => return Err("Hits and Rotate are whole numbers"),
            },
            Self::Clusters | Self::Markov | Self::States | Self::Share | Self::Child => text
                .parse::<u16>()
                .map_err(|_| "An amount runs from 0 to 100")?
                .into(),
            Self::Rule => match (shape.place, text.parse::<u8>()) {
                (Where::Cells(_), Ok(rule)) => rule.into(),
                _ => return Err("Rule runs from 0 to 255"),
            },
            // A wave here is laid out by rows, so a rate has to be in time with them.
            Self::Rate => {
                shape.rate = text
                    .parse::<WaveRate>()
                    .map_err(|_| "A rate here is like 1/4, 1/8d, 2 bars, 7st, or x3 cycles")?;
                return Ok(());
            }
            Self::Phase => match text.parse::<u16>() {
                Ok(phase) if within(phase) => phase.into(),
                _ => return Err("Phase runs from 0 to 359"),
            },
            Self::Notes => match text.parse::<u8>() {
                Ok(notes) if notes > 0 => notes.into(),
                _ => return Err("Notes runs from 1 to 8"),
            },
            Self::Octave | Self::Density | Self::Accent | Self::Slide => text
                .parse::<u16>()
                .map_err(|_| "A chance runs from 0 to 100")?
                .into(),
            Self::TrigLoop
            | Self::NoteLoop
            | Self::OctaveLoop
            | Self::AccentLoop
            | Self::SlideLoop => match text.parse::<u8>() {
                Ok(rows) if within(rows.into()) => rows.into(),
                _ => return Err("A loop runs from 0, off, to 64 rows"),
            },
            Self::Run => match text.parse::<i16>() {
                Ok(run) if run > 0 => run.into(),
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
                return Ok(());
            }
            Self::Transpose => text
                .parse::<i16>()
                .map_err(|_| "Transpose runs from -48 to 48")?
                .into(),
            Self::Axis => {
                shape.follow.axis = Some(column.parse(text).ok_or(unknown)?);
                return Ok(());
            }
            Self::Late => text
                .parse::<u16>()
                .map_err(|_| "Rotate is a whole number of rows")?
                .into(),
            Self::Mutation => match text.parse::<u8>() {
                Ok(changes) if within(changes.into()) => changes.into(),
                _ => return Err("Mutation runs from 0 to 32 changes"),
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
                    Ok(step) if step > 0 => step.into(),
                    _ if hex => return Err("Step is a hex value above 00"),
                    _ => return Err("Step is a whole number above 0"),
                }
            }
            Self::Spread => text
                .parse::<u16>()
                .map_err(|_| "Spread runs from 0 to 100")?
                .into(),
            Self::Bias => text
                .parse::<i16>()
                .map_err(|_| "Bias runs from -100 to 100")?
                .into(),
            Self::Steps => match (column, text.parse::<u8>()) {
                (ShapeColumn::Note { .. }, Ok(kept)) if kept > 0 => kept.into(),
                _ => return Err("Steps is how many degrees it lets through, from 1"),
            },
            Self::Weights => {
                let ShapeColumn::Note { scale, .. } = column else {
                    return Ok(());
                };
                let digits: Option<Vec<u8>> = text
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .map(|c| c.to_digit(16).map(|digit| digit as u8))
                    .collect();
                match digits {
                    Some(digits) if !digits.is_empty() && digits.len() <= degree_count(scale) => {
                        shape.random.weights[..digits.len()].copy_from_slice(&digits);
                    }
                    _ => return Err("Weights are a hex digit a degree from the root, like F8C4"),
                }
                return Ok(());
            }
            Self::Depth => match text.parse::<u8>() {
                Ok(depth) if within(depth.into()) => depth.into(),
                _ => return Err("Depth runs from 2 to 8"),
            },
            Self::Factor => match text.parse::<u8>() {
                Ok(factor) if within(factor.into()) => factor.into(),
                _ => return Err("Factor runs from 1 to 16"),
            },
            Self::Flip | Self::Keep => u8::from_str_radix(text, 16)
                .map_err(|_| "Bits are two hex digits")?
                .into(),
            Self::Turn => match text.parse::<u8>() {
                Ok(turn) if within(turn.into()) => turn.into(),
                _ => return Err("Turn runs from 0 to 7"),
            },
            Self::Order => match text.parse::<u8>() {
                Ok(order) if within(order.into()) => order.into(),
                _ => return Err("Order runs from 1 to 3"),
            },
            Self::Temperature => text
                .parse::<u16>()
                .map_err(|_| "Temp runs from 0 to 100")?
                .into(),
            Self::Change => text
                .parse::<u16>()
                .map_err(|_| "Change runs from 0 to 100")?
                .into(),
            Self::Ones => text
                .parse::<u16>()
                .map_err(|_| "Chance runs from 0 to 100")?
                .into(),
            Self::RingLength => match text.parse::<u8>() {
                Ok(length) if within(length.into()) => length.into(),
                _ => return Err("Length runs from 1 to 16"),
            },
            Self::MoveList => {
                shape.tonnetz.list = MoveList::parse(text)
                    .ok_or("Moves are P, L and R, up to 8 of them, like P L R L")?;
                return Ok(());
            }
            Self::PWeight | Self::LWeight | Self::RWeight => match text.parse::<u8>() {
                Ok(weight) if within(weight.into()) => weight.into(),
                _ => return Err("A weight runs from 0 to 9"),
            },
            Self::LoopLength => match text.parse::<u8>() {
                Ok(length) if within(length.into()) => length.into(),
                _ => return Err("Length runs from 1 to 16"),
            },
            Self::Quarters
            | Self::Eighths
            | Self::Sixteenths
            | Self::QuarterTriplets
            | Self::EighthTriplets
            | Self::SixteenthTriplets
            | Self::QuartersB
            | Self::EighthsB
            | Self::SixteenthsB
            | Self::QuarterTripletsB
            | Self::EighthTripletsB
            | Self::SixteenthTripletsB => text
                .parse::<u16>()
                .map_err(|_| "A chance runs from 0 to 100")?
                .into(),
            Self::Fill
            | Self::Wave
            | Self::Where
            | Self::Pitches
            | Self::Source
            | Self::Invert
            | Self::Reverse
            | Self::CopyFx
            | Self::Repitch
            | Self::Shift
            | Self::Retro
            | Self::Ratchet
            | Self::DropAdd
            | Self::Edge
            | Self::Masks
            | Self::Part
            | Self::Rows
            | Self::Side
            | Self::Rhythm
            | Self::Role
            | Self::Channel
            | Self::Bar
            | Self::CellStart
            | Self::Triad
            | Self::Voice
            | Self::Moves
            | Self::InScale
            | Self::Vary => return Ok(()),
            Self::MapX
            | Self::MapY
            | Self::KickDensity
            | Self::SnareDensity
            | Self::HatDensity
            | Self::Chaos => u8::from_str_radix(text, 16)
                .map_err(|_| "A value is two hex digits")?
                .into(),
        };
        self.put(shape, context, number);
        Ok(())
    }

    /// The lowest and highest it takes as a number, for a knob or a macro's range: a list
    /// setting's are its choices' places, one that's off or on is `0` or `1` (`Pitches` `1` to
    /// keep), and `About` is a note. A `List`'s values have none.
    pub fn bounds(self, context: &Context) -> Option<(i32, i32)> {
        let column = context.column;
        match self.spec().bounds {
            Bounds::Fixed(low, high) => Some((low, high)),
            Bounds::Column => Some((i32::from(column.lowest()), i32::from(column.highest()))),
            Bounds::Listed => Some((0, self.choices(context)?.len().checked_sub(1)? as i32)),
            Bounds::Contextual(bounds) => bounds(context),
            Bounds::Typed => None,
        }
    }

    /// Its value as a number within its [`bounds`](Self::bounds), if it has one now: not a
    /// `Where`'s amount the `Where` doesn't have, or a rate typed that isn't listed.
    pub fn number(self, shape: &Shape, context: &Context) -> Option<i32> {
        (self.spec().get)(shape, context)
    }

    /// Set it to a number, kept within its bounds. A list setting already on that choice keeps
    /// what it has, so a `Where` keeps its amount; one the shape doesn't have changes nothing.
    pub fn set_number(self, shape: &mut Shape, context: &Context, number: i32) {
        let Some((low, high)) = self.bounds(context) else {
            return;
        };
        (self.spec().set)(shape, context, number.clamp(low, high));
    }

    /// Set it to what it is on `start`, a shape with the part it belongs to as that starts.
    fn take(self, shape: &mut Shape, context: &Context, start: &Shape) {
        if let Some(number) = (self.spec().get)(start, context) {
            (self.spec().set)(shape, context, number);
        }
    }

    /// Where the shape is among a list setting's choices, as a number.
    fn chosen_number(self, shape: &Shape, context: &Context) -> Option<i32> {
        self.chosen(shape, context).map(|at| at as i32)
    }

    /// Give it the choice at a place, unless it's on that choice already, so a `Where` keeps its
    /// amount.
    fn choose(self, shape: &mut Shape, context: &Context, at: i32) {
        if self.chosen(shape, context) != Some(at as usize)
            && let Some(choice) = self
                .choices(context)
                .and_then(|choices| choices.get(at as usize).copied())
        {
            choice.apply(shape);
        }
    }
}

/// One generator setting, described once: the part of it that's the same whatever the shape and
/// its column. What hangs on them besides, which shapes have it and how it's shown, put back,
/// typed and chosen from a list, is [`GenParam`]'s own (the design's `device-interface.md` §4.2).
#[derive(Debug, Clone, Copy)]
pub struct GenParamSpec {
    /// What a song or a mapping stores it by, which never changes.
    pub key: &'static str,
    /// Its name on screen.
    pub name: &'static str,
    /// How its values read.
    pub units: Units,
    /// How its value is typed, on a column whose values aren't in hex.
    pub typing: Typing,
    /// The lowest and highest it takes as a number.
    pub bounds: Bounds,
    /// How far it moves stepped as a number, or none for one stepped as it is: an end, a list, a
    /// rate and what's only typed.
    pub stepping: Option<Stepping>,
    /// Whether the engine can hold it as it plays.
    pub engine: bool,
    /// Whether it's chosen in the app alone, never on a controller's knob: what the generator
    /// is, as its fill, its rows, what it follows and which part, channel, side, rhythm or voice
    /// it plays, and what's typed, as a `List`'s values, a `Random`'s weights and a `Tonnetz`'
    /// moves.
    pub app_only: bool,
    /// Its value as a number, if it has one now.
    pub get: fn(&Shape, &Context) -> Option<i32>,
    /// Set it to a number already within its bounds.
    pub set: fn(&mut Shape, &Context, i32),
}

/// The lowest and highest a setting takes as a number.
#[derive(Debug, Clone, Copy)]
pub enum Bounds {
    /// These, whatever it acts on.
    Fixed(i32, i32),
    /// The column's own lowest and highest values.
    Column,
    /// Its choices' places.
    Listed,
    /// Worked out from what it acts on: the rows a column could change, the source a `Follow`
    /// reads, or the column's range or scale.
    Contextual(fn(&Context) -> Option<(i32, i32)>),
    /// None: it's only typed, as a `List`'s values are.
    Typed,
}

/// How a setting stepped as a number moves: by a small step and a large one, and whether past an
/// end it goes round to the other, as a phase, a rotation and a switch do, rather than stopping
/// there.
#[derive(Debug, Clone, Copy)]
pub struct Stepping {
    pub nudge: i32,
    pub jump: Jump,
    pub round: bool,
}

/// How far a setting's large step moves it.
#[derive(Debug, Clone, Copy)]
pub enum Jump {
    /// So far, whatever the column.
    By(i32),
    /// An octave, of the scale's degrees on notes while it's on, and otherwise of semitones.
    Octave,
    /// As far as a large step moves the column's own values: an octave of notes, of degrees
    /// while the scale is on, four 24ths of a nudge, or `10` in hex.
    Column,
}

impl Jump {
    /// How far it moves on a column.
    fn on(self, column: ShapeColumn) -> i32 {
        let degrees = match column {
            ShapeColumn::Note { scale, .. } if scale.on => {
                Some(scale.kind.intervals().len() as i32)
            }
            _ => None,
        };
        match self {
            Self::By(jump) => jump,
            Self::Octave => degrees.unwrap_or(12),
            Self::Column => degrees.unwrap_or_else(|| large_step(column)),
        }
    }
}

/// Stepped by `nudge` and `jump`, stopping at its ends.
const fn stops(nudge: i32, jump: i32) -> Option<Stepping> {
    Some(Stepping {
        nudge,
        jump: Jump::By(jump),
        round: false,
    })
}

/// Stepped by `nudge` and `jump`, going round past its ends.
const fn round(nudge: i32, jump: i32) -> Option<Stepping> {
    Some(Stepping {
        nudge,
        jump: Jump::By(jump),
        round: true,
    })
}

/// A switch, off or on, which a step turns over.
const SWITCH: Option<Stepping> = round(1, 1);
/// A percent, by one or [`CHANCE_JUMP`].
const PERCENT: Option<Stepping> = stops(1, CHANCE_JUMP);
/// A bend, by [`BEND_NUDGE`] or [`BEND_JUMP`].
const BEND: Option<Stepping> = stops(BEND_NUDGE, BEND_JUMP);
/// A byte in hex, by one or `10`.
const HEX: Option<Stepping> = stops(1, HEX_JUMP);
/// A count, by one or [`EUCLID_JUMP`].
const COUNT: Option<Stepping> = stops(1, EUCLID_JUMP);
const WHOLE: Typing = Typing::Whole { signed: false };
const SIGNED: Typing = Typing::Whole { signed: true };

/// A `Divisions` chance, of its set, the track's own channel's or `B`'s, and its division.
fn division_chance(shape: &Shape, set: usize, division: Division) -> Option<i32> {
    Some(i32::from(shape.divisions.chances[set][division.index()]))
}

fn set_division_chance(shape: &mut Shape, set: usize, division: Division, number: i32) {
    shape.divisions.chances[set][division.index()] = number as u8;
}

/// Every generator setting's description, by [`GenParam`].
pub const GEN_PARAMS: [GenParamSpec; GenParam::ALL.len()] = [
    GenParamSpec {
        key: "fill",
        name: "Fill",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Listed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |shape, context| GenParam::Fill.chosen_number(shape, context),
        set: |shape, context, at| GenParam::Fill.choose(shape, context, at),
    },
    GenParamSpec {
        key: "from",
        name: "From",
        units: Units::Column,
        typing: Typing::Named { list: false },
        bounds: Bounds::Column,
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.from)),
        set: |shape, _, number| shape.from = number as u8,
    },
    GenParamSpec {
        key: "to",
        name: "To",
        units: Units::Column,
        typing: Typing::Named { list: false },
        bounds: Bounds::Column,
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.to)),
        set: |shape, _, number| shape.to = number as u8,
    },
    GenParamSpec {
        key: "notes",
        name: "Notes",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_ACID_NOTES as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.notes)),
        set: |shape, _, number| shape.acid.notes = number as u8,
    },
    GenParamSpec {
        key: "octave",
        name: "Octave",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.octave)),
        set: |shape, _, number| shape.acid.octave = number as u8,
    },
    GenParamSpec {
        key: "density",
        name: "Density",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.density)),
        set: |shape, _, number| shape.acid.density = number as u8,
    },
    GenParamSpec {
        key: "accent",
        name: "Accent",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.accent)),
        set: |shape, _, number| shape.acid.accent = number as u8,
    },
    GenParamSpec {
        key: "slide",
        name: "Slide",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.slide)),
        set: |shape, _, number| shape.acid.slide = number as u8,
    },
    // `1` to keep the pitches.
    GenParamSpec {
        key: "pitches",
        name: "Pitches",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.keep)),
        set: |shape, _, number| shape.acid.keep = number != 0,
    },
    GenParamSpec {
        key: "trigloop",
        name: "Trig loop",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_LOOP as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.loops.trigs)),
        set: |shape, _, number| shape.acid.loops.trigs = number as u8,
    },
    GenParamSpec {
        key: "noteloop",
        name: "Note loop",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_LOOP as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.loops.notes)),
        set: |shape, _, number| shape.acid.loops.notes = number as u8,
    },
    GenParamSpec {
        key: "octloop",
        name: "Oct loop",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_LOOP as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.loops.octaves)),
        set: |shape, _, number| shape.acid.loops.octaves = number as u8,
    },
    GenParamSpec {
        key: "accloop",
        name: "Acc loop",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_LOOP as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.loops.accents)),
        set: |shape, _, number| shape.acid.loops.accents = number as u8,
    },
    GenParamSpec {
        key: "slideloop",
        name: "Sld loop",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_LOOP as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.acid.loops.slides)),
        set: |shape, _, number| shape.acid.loops.slides = number as u8,
    },
    GenParamSpec {
        key: "source",
        name: "Source",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Listed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |shape, context| GenParam::Source.chosen_number(shape, context),
        set: |shape, context, at| GenParam::Source.choose(shape, context, at),
    },
    GenParamSpec {
        key: "transpose",
        name: "Transpose",
        units: Units::Interval,
        typing: SIGNED,
        bounds: Bounds::Fixed(-(MAX_TRANSPOSE as i32), MAX_TRANSPOSE as i32),
        stepping: Some(Stepping {
            nudge: 1,
            jump: Jump::Octave,
            round: false,
        }),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.transpose)),
        set: |shape, _, number| shape.follow.transpose = number as i8,
    },
    GenParamSpec {
        key: "invert",
        name: "Invert",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.invert)),
        set: |shape, _, number| shape.follow.invert = number != 0,
    },
    // The note it turns about: the first note of the source until it's set.
    GenParamSpec {
        key: "axis",
        name: "About",
        units: Units::Column,
        typing: Typing::Named { list: false },
        bounds: Bounds::Column,
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, context| {
            let axis = shape.follow.axis.or(context.source.and_then(first_note));
            axis.map(i32::from)
        },
        set: |shape, _, number| shape.follow.axis = Some(number as u8),
    },
    GenParamSpec {
        key: "reverse",
        name: "Reverse",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.reverse)),
        set: |shape, _, number| shape.follow.reverse = number != 0,
    },
    // How many rows late it is, round the source's loop.
    GenParamSpec {
        key: "late",
        name: "Rotate",
        units: Units::Rows,
        typing: WHOLE,
        bounds: Bounds::Contextual(|context| Some((0, context.source_rows() as i32 - 1))),
        stepping: round(1, EUCLID_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.rotate)),
        set: |shape, _, number| shape.follow.rotate = number as u16,
    },
    GenParamSpec {
        key: "fx",
        name: "FX",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.fx)),
        set: |shape, _, number| shape.follow.fx = number != 0,
    },
    GenParamSpec {
        key: "mutation",
        name: "Mutation",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_MUTATION as i32),
        stepping: stops(1, 4),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutation)),
        set: |shape, _, number| shape.follow.mutation = number as u8,
    },
    GenParamSpec {
        key: "repitch",
        name: "Repitch",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutate.repitch)),
        set: |shape, _, number| shape.follow.mutate.repitch = number != 0,
    },
    GenParamSpec {
        key: "shift",
        name: "Shift",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutate.shift)),
        set: |shape, _, number| shape.follow.mutate.shift = number != 0,
    },
    GenParamSpec {
        key: "retro",
        name: "Retro",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutate.retro)),
        set: |shape, _, number| shape.follow.mutate.retro = number != 0,
    },
    GenParamSpec {
        key: "ratchet",
        name: "Ratchet",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutate.ratchet)),
        set: |shape, _, number| shape.follow.mutate.ratchet = number != 0,
    },
    GenParamSpec {
        key: "drop",
        name: "Drop/add",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.follow.mutate.drop)),
        set: |shape, _, number| shape.follow.mutate.drop = number != 0,
    },
    // In hex on a column whose values are, and as a number on notes and nudges.
    GenParamSpec {
        key: "step",
        name: "Step",
        units: Units::Step,
        typing: WHOLE,
        bounds: Bounds::Contextual(|context| Some((1, i32::from(longest_step(context.column))))),
        stepping: Some(Stepping {
            nudge: 1,
            jump: Jump::Column,
            round: false,
        }),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.walk.step)),
        set: |shape, _, number| shape.walk.step = number as u8,
    },
    // `1` to wrap.
    GenParamSpec {
        key: "edge",
        name: "Edge",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.walk.edge == Edge::Wrap)),
        set: |shape, _, number| {
            shape.walk.edge = if number != 0 {
                Edge::Wrap
            } else {
                Edge::Reflect
            };
        },
    },
    GenParamSpec {
        key: "depth",
        name: "Depth",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(MIN_DEPTH as i32, MAX_DEPTH as i32),
        stepping: stops(1, 2),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.pink.depth)),
        set: |shape, _, number| shape.pink.depth = number as u8,
    },
    GenParamSpec {
        key: "spread",
        name: "Spread",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_SPREAD as i32),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.random.spread)),
        set: |shape, _, number| shape.random.spread = number as u8,
    },
    GenParamSpec {
        key: "bias",
        name: "Bias",
        units: Units::Bend,
        typing: SIGNED,
        bounds: Bounds::Fixed(-(MAX_BIAS as i32), MAX_BIAS as i32),
        stepping: BEND,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.random.bias)),
        set: |shape, _, number| shape.random.bias = number as i8,
    },
    // How many of the scale's degrees it lets through, on notes alone.
    GenParamSpec {
        key: "steps",
        name: "Steps",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Contextual(|context| match context.column {
            ShapeColumn::Note { scale, .. } => Some((1, degree_count(scale) as i32)),
            _ => None,
        }),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, context| match context.column {
            ShapeColumn::Note { scale, .. } => Some(shape.random.kept(scale) as i32),
            _ => None,
        },
        set: |shape, context, number| {
            if let ShapeColumn::Note { scale, .. } = context.column {
                shape.random.steps = steps_kept(number, scale);
            }
        },
    },
    GenParamSpec {
        key: "weights",
        name: "Weights",
        units: Units::Weights,
        typing: Typing::Weights,
        bounds: Bounds::Typed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |_, _| None,
        set: |_, _, _| {},
    },
    GenParamSpec {
        key: "factor",
        name: "Factor",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_FACTOR as i32),
        stepping: stops(1, FACTOR_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.bits.factor)),
        set: |shape, _, number| shape.bits.factor = number as u8,
    },
    GenParamSpec {
        key: "masks",
        name: "Masks",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.bits.masks)),
        set: |shape, _, number| shape.bits.masks = number != 0,
    },
    // Its parts' places, without a list, so the engine can hold it.
    GenParamSpec {
        key: "part",
        name: "Part",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, DrumPart::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: true,
        get: |shape, _| Some(shape.grids.part?.index() as i32),
        set: |shape, _, at| shape.grids.part = Some(DrumPart::ALL[at as usize]),
    },
    // The note a `Grids` or `Divisions` writes on an empty column, its `From`.
    GenParamSpec {
        key: "note",
        name: "Note",
        units: Units::Column,
        typing: Typing::Named { list: false },
        bounds: Bounds::Column,
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.from)),
        set: |shape, _, number| shape.from = number as u8,
    },
    GenParamSpec {
        key: "x",
        name: "X",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.x)),
        set: |shape, _, number| shape.grids.x = number as u8,
    },
    GenParamSpec {
        key: "y",
        name: "Y",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.y)),
        set: |shape, _, number| shape.grids.y = number as u8,
    },
    GenParamSpec {
        key: "bd",
        name: "BD fill",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.density[DrumPart::Kick.index()])),
        set: |shape, _, number| shape.grids.density[DrumPart::Kick.index()] = number as u8,
    },
    GenParamSpec {
        key: "sd",
        name: "SD fill",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.density[DrumPart::Snare.index()])),
        set: |shape, _, number| shape.grids.density[DrumPart::Snare.index()] = number as u8,
    },
    GenParamSpec {
        key: "hh",
        name: "HH fill",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.density[DrumPart::Hats.index()])),
        set: |shape, _, number| shape.grids.density[DrumPart::Hats.index()] = number as u8,
    },
    GenParamSpec {
        key: "chaos",
        name: "Chaos",
        units: Units::Byte,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.chaos)),
        set: |shape, _, number| shape.grids.chaos = number as u8,
    },
    // `1` for a row of a 32nd.
    GenParamSpec {
        key: "rows",
        name: "Rows",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.grids.fine)),
        set: |shape, _, number| shape.grids.fine = number != 0,
    },
    // Its choices' places, without a list, so the engine can hold it, as for each list below.
    GenParamSpec {
        key: "channel",
        name: "Channel",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, Channel::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: true,
        get: |shape, _| Some(shape.divisions.channel?.index() as i32),
        set: |shape, _, at| shape.divisions.channel = Some(Channel::ALL[at as usize]),
    },
    GenParamSpec {
        key: "bar",
        name: "Bar",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, Bar::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(shape.divisions.bar.index() as i32),
        set: |shape, _, at| shape.divisions = shape.divisions.with_bar(Bar::ALL[at as usize]),
    },
    GenParamSpec {
        key: "d4",
        name: "1/4",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::Quarter),
        set: |shape, _, number| set_division_chance(shape, 0, Division::Quarter, number),
    },
    GenParamSpec {
        key: "d8",
        name: "1/8",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::Eighth),
        set: |shape, _, number| set_division_chance(shape, 0, Division::Eighth, number),
    },
    GenParamSpec {
        key: "d16",
        name: "1/16",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::Sixteenth),
        set: |shape, _, number| set_division_chance(shape, 0, Division::Sixteenth, number),
    },
    GenParamSpec {
        key: "d4t",
        name: "1/4T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::QuarterTriplet),
        set: |shape, _, number| set_division_chance(shape, 0, Division::QuarterTriplet, number),
    },
    GenParamSpec {
        key: "d8t",
        name: "1/8T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::EighthTriplet),
        set: |shape, _, number| set_division_chance(shape, 0, Division::EighthTriplet, number),
    },
    GenParamSpec {
        key: "d16t",
        name: "1/16T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 0, Division::SixteenthTriplet),
        set: |shape, _, number| set_division_chance(shape, 0, Division::SixteenthTriplet, number),
    },
    GenParamSpec {
        key: "b4",
        name: "B 1/4",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::Quarter),
        set: |shape, _, number| set_division_chance(shape, 1, Division::Quarter, number),
    },
    GenParamSpec {
        key: "b8",
        name: "B 1/8",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::Eighth),
        set: |shape, _, number| set_division_chance(shape, 1, Division::Eighth, number),
    },
    GenParamSpec {
        key: "b16",
        name: "B 1/16",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::Sixteenth),
        set: |shape, _, number| set_division_chance(shape, 1, Division::Sixteenth, number),
    },
    GenParamSpec {
        key: "b4t",
        name: "B 1/4T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::QuarterTriplet),
        set: |shape, _, number| set_division_chance(shape, 1, Division::QuarterTriplet, number),
    },
    GenParamSpec {
        key: "b8t",
        name: "B 1/8T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::EighthTriplet),
        set: |shape, _, number| set_division_chance(shape, 1, Division::EighthTriplet, number),
    },
    GenParamSpec {
        key: "b16t",
        name: "B 1/16T",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| division_chance(shape, 1, Division::SixteenthTriplet),
        set: |shape, _, number| set_division_chance(shape, 1, Division::SixteenthTriplet, number),
    },
    GenParamSpec {
        key: "change",
        name: "Change",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.ring.change)),
        set: |shape, _, number| shape.ring.change = number as u8,
    },
    GenParamSpec {
        key: "ones",
        name: "Chance",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.ring.chance)),
        set: |shape, _, number| shape.ring.chance = number as u8,
    },
    GenParamSpec {
        key: "length",
        name: "Length",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_RING as i32),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.ring.length)),
        set: |shape, _, number| shape.ring.length = number as u8,
    },
    // `1` for a minor first chord.
    GenParamSpec {
        key: "triad",
        name: "Triad",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.quality == Quality::Minor)),
        set: |shape, _, number| {
            shape.tonnetz.quality = if number != 0 {
                Quality::Minor
            } else {
                Quality::Major
            };
        },
    },
    GenParamSpec {
        key: "voice",
        name: "Voice",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, TonnetzVoice::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: true,
        get: |shape, _| Some(shape.tonnetz.voice?.index() as i32),
        set: |shape, _, at| shape.tonnetz.voice = Some(TonnetzVoice::ALL[at as usize]),
    },
    GenParamSpec {
        key: "moves",
        name: "Moves",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, Moves::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(shape.tonnetz.moves.index() as i32),
        set: |shape, _, at| shape.tonnetz.moves = Moves::ALL[at as usize],
    },
    GenParamSpec {
        key: "list",
        name: "List",
        units: Units::Named,
        typing: Typing::Moves,
        bounds: Bounds::Typed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |_, _| None,
        set: |_, _, _| {},
    },
    GenParamSpec {
        key: "p",
        name: "P",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_MOVE_WEIGHT as i32),
        stepping: stops(1, WEIGHT_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.weights[0])),
        set: |shape, _, number| shape.tonnetz.weights[0] = number as u8,
    },
    GenParamSpec {
        key: "l",
        name: "L",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_MOVE_WEIGHT as i32),
        stepping: stops(1, WEIGHT_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.weights[1])),
        set: |shape, _, number| shape.tonnetz.weights[1] = number as u8,
    },
    GenParamSpec {
        key: "r",
        name: "R",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_MOVE_WEIGHT as i32),
        stepping: stops(1, WEIGHT_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.weights[2])),
        set: |shape, _, number| shape.tonnetz.weights[2] = number as u8,
    },
    GenParamSpec {
        key: "loop",
        name: "Length",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_MOVE_LOOP as i32),
        stepping: stops(1, LOOP_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.length)),
        set: |shape, _, number| shape.tonnetz.length = number as u8,
    },
    GenParamSpec {
        key: "inscale",
        name: "Scale",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.tonnetz.in_scale)),
        set: |shape, _, number| shape.tonnetz.in_scale = number != 0,
    },
    GenParamSpec {
        key: "order",
        name: "Order",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_ORDER as i32),
        stepping: stops(1, 1),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.variation.order)),
        set: |shape, _, number| shape.variation.order = number as u8,
    },
    GenParamSpec {
        key: "temperature",
        name: "Temp",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.variation.temperature)),
        set: |shape, _, number| shape.variation.temperature = number as u8,
    },
    GenParamSpec {
        key: "flip",
        name: "Flip",
        units: Units::Bits,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, u8::MAX as i32),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.bits.flip)),
        set: |shape, _, number| shape.bits.flip = number as u8,
    },
    GenParamSpec {
        key: "keep",
        name: "Keep",
        units: Units::Bits,
        typing: Typing::Hex,
        bounds: Bounds::Fixed(0, u8::MAX as i32),
        stepping: HEX,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.bits.keep)),
        set: |shape, _, number| shape.bits.keep = number as u8,
    },
    GenParamSpec {
        key: "turn",
        name: "Turn",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, MAX_TURN as i32),
        stepping: round(1, 4),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.bits.turn)),
        set: |shape, _, number| shape.bits.turn = number as u8,
    },
    GenParamSpec {
        key: "curve",
        name: "Curve",
        units: Units::Bend,
        typing: SIGNED,
        bounds: Bounds::Fixed(-(MAX_BEND as i32), MAX_BEND as i32),
        stepping: BEND,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.curve)),
        set: |shape, _, number| shape.curve = number as i8,
    },
    GenParamSpec {
        key: "s",
        name: "S",
        units: Units::Bend,
        typing: SIGNED,
        bounds: Bounds::Fixed(-(MAX_BEND as i32), MAX_BEND as i32),
        stepping: BEND,
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.s)),
        set: |shape, _, number| shape.s = number as i8,
    },
    GenParamSpec {
        key: "wave",
        name: "Shape",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Listed,
        stepping: None,
        engine: false,
        app_only: false,
        get: |shape, context| GenParam::Wave.chosen_number(shape, context),
        set: |shape, context, at| GenParam::Wave.choose(shape, context, at),
    },
    GenParamSpec {
        key: "rate",
        name: "Rate",
        units: Units::Named,
        typing: Typing::Rate,
        bounds: Bounds::Listed,
        stepping: None,
        engine: false,
        app_only: false,
        get: |shape, context| GenParam::Rate.chosen_number(shape, context),
        set: |shape, context, at| GenParam::Rate.choose(shape, context, at),
    },
    GenParamSpec {
        key: "phase",
        name: "Phase",
        units: Units::Degrees,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 359),
        stepping: round(PHASE_NUDGE, PHASE_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.phase)),
        set: |shape, _, number| shape.phase = number as u16,
    },
    GenParamSpec {
        key: "values",
        name: "Values",
        units: Units::Column,
        typing: Typing::Named { list: true },
        bounds: Bounds::Typed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |_, _| None,
        set: |_, _, _| {},
    },
    GenParamSpec {
        key: "run",
        name: "Run",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_RUN as i32),
        stepping: stops(1, RUN_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| Some(i32::from(shape.run)),
        set: |shape, _, number| shape.run = number as u16,
    },
    GenParamSpec {
        key: "where",
        name: "Where",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Listed,
        stepping: None,
        engine: false,
        app_only: true,
        get: |shape, context| GenParam::Where.chosen_number(shape, context),
        set: |shape, context, at| GenParam::Where.choose(shape, context, at),
    },
    // The amount of the `Where` it's named for, as each below is, and none while it has another.
    GenParamSpec {
        key: "every",
        name: "Amount",
        units: Units::Rows,
        typing: WHOLE,
        bounds: Bounds::Fixed(1, MAX_EVERY as i32),
        stepping: stops(1, EVERY_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Every(every) => Some(i32::from(every)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Every(every) = &mut shape.place {
                *every = number as u16;
            }
        },
    },
    GenParamSpec {
        key: "chance",
        name: "Amount",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Chance(percent) => Some(i32::from(percent)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Chance(percent) = &mut shape.place {
                *percent = number as u8;
            }
        },
    },
    // Of the rows the column could change, so no more than them.
    GenParamSpec {
        key: "hits",
        name: "Hits",
        units: Units::OfRows,
        typing: WHOLE,
        bounds: Bounds::Contextual(|context| Some((0, context.open as i32))),
        stepping: COUNT,
        engine: true,
        app_only: false,
        get: |shape, context| match shape.place {
            Where::Euclid { hits, .. } => Some(i32::from(hits).min(context.open as i32)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Euclid { hits, .. } = &mut shape.place {
                *hits = number as u16;
            }
        },
    },
    GenParamSpec {
        key: "rotate",
        name: "Rotate",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Contextual(|context| Some((0, (context.open as i32 - 1).max(0)))),
        stepping: round(1, EUCLID_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Euclid { rotate, .. } => Some(i32::from(rotate)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Euclid { rotate, .. } = &mut shape.place {
                *rotate = number as u16;
            }
        },
    },
    GenParamSpec {
        key: "clusters",
        name: "Amount",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Clusters(amount) => Some(i32::from(amount)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Clusters(amount) = &mut shape.place {
                *amount = number as u8;
            }
        },
    },
    GenParamSpec {
        key: "markov",
        name: "Amount",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Markov(amount) => Some(i32::from(amount)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Markov(amount) = &mut shape.place {
                *amount = number as u8;
            }
        },
    },
    GenParamSpec {
        key: "states",
        name: "Amount",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::States(amount) => Some(i32::from(amount)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::States(amount) = &mut shape.place {
                *amount = number as u8;
            }
        },
    },
    GenParamSpec {
        key: "share",
        name: "Amount",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Complementary { share, .. } => Some(i32::from(share)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Complementary { share, .. } = &mut shape.place {
                *share = number as u8;
            }
        },
    },
    // `1` for the second side of the coin.
    GenParamSpec {
        key: "side",
        name: "Side",
        units: Units::Named,
        typing: Typing::Toggled,
        bounds: Bounds::Fixed(0, 1),
        stepping: SWITCH,
        engine: true,
        app_only: true,
        get: |shape, _| match shape.place {
            Where::Complementary { second, .. } => Some(i32::from(second)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Complementary { second, .. } = &mut shape.place {
                *second = number != 0;
            }
        },
    },
    GenParamSpec {
        key: "rhythm",
        name: "Rhythm",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, World::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: true,
        get: |shape, _| match shape.place {
            Where::Rhythm { rhythm, .. } => Some(rhythm.index() as i32),
            _ => None,
        },
        set: |shape, _, at| Choice::World(World::ALL[at as usize]).apply(shape),
    },
    GenParamSpec {
        key: "role",
        name: "Part",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, Role::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: true,
        get: |shape, _| match shape.place {
            Where::Rhythm { part, .. } => Some(part.index() as i32),
            _ => None,
        },
        set: |shape, _, at| Choice::Role(Role::ALL[at as usize]).apply(shape),
    },
    GenParamSpec {
        key: "child",
        name: "Child",
        units: Units::Percent,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 100),
        stepping: PERCENT,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Rhythm { child, .. } => Some(i32::from(child)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Rhythm { child, .. } = &mut shape.place {
                *child = number as u8;
            }
        },
    },
    GenParamSpec {
        key: "rule",
        name: "Rule",
        units: Units::Count,
        typing: WHOLE,
        bounds: Bounds::Fixed(0, 0xFF),
        stepping: stops(1, RULE_JUMP),
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Cells(cells) => Some(i32::from(cells.rule)),
            _ => None,
        },
        set: |shape, _, number| {
            if let Where::Cells(cells) = &mut shape.place {
                cells.rule = number as u8;
            }
        },
    },
    GenParamSpec {
        key: "start",
        name: "Start",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, CellStart::ALL.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| match shape.place {
            Where::Cells(cells) => Some(cells.start.index() as i32),
            _ => None,
        },
        set: |shape, _, at| {
            if let Where::Cells(cells) = &mut shape.place {
                cells.start = CellStart::ALL[at as usize];
            }
        },
    },
    // Its choices' places, worked out without listing them, so the engine can hold it.
    GenParamSpec {
        key: "vary",
        name: "Vary",
        units: Units::Named,
        typing: Typing::Listed,
        bounds: Bounds::Fixed(0, VARIES.len() as i32 - 1),
        stepping: None,
        engine: true,
        app_only: false,
        get: |shape, _| Some(VARIES.iter().position(|passes| *passes == shape.vary)? as i32),
        set: |shape, _, at| shape.vary = VARIES[at as usize],
    },
];
/// How long each of a generator's variations lasts, as `Vary` reads: `Once` for none, as the seed
/// written plays every pass.
fn vary_name(passes: u8) -> String {
    match passes {
        0 => "Once".to_owned(),
        1 => "Each pass".to_owned(),
        passes => format!("Every {passes} passes"),
    }
}

/// `Steps` as the panel shows it: `All 7`, or how many of the degrees it lets through and which,
/// the most important first, as `3 of 7: C G E`.
fn steps_name(random: Random, scale: Scale) -> String {
    let (kept, degrees) = (random.kept(scale), degree_count(scale));
    if kept >= degrees {
        return format!("All {degrees}");
    }
    let names: Vec<&str> = Degrees::of(scale).order()[..kept]
        .iter()
        .map(|degree| degree_name(scale, usize::from(*degree)))
        .collect();
    format!("{kept} of {degrees}: {}", names.join(" "))
}

/// What `Steps` keeps for letting `kept` degrees through: 0 for all of them, so letting every
/// degree through is as it starts.
fn steps_kept(kept: i32, scale: Scale) -> u8 {
    let degrees = degree_count(scale) as i32;
    if kept >= degrees {
        0
    } else {
        kept.max(1) as u8
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

/// An end moved a small or a large step: a note by a semitone or an octave, or by a degree or an
/// octave of them while the scale is on; a nudge by one or four 24ths; anything else by one or
/// `10` in hex.
fn stepped(column: ShapeColumn, value: u8, sign: i32, large: bool) -> u8 {
    if let ShapeColumn::Note { scale, .. } = column
        && scale.on
    {
        let degrees = if large {
            scale.kind.intervals().len()
        } else {
            1
        };
        return scale.moved(value, degrees as i32 * sign);
    }
    let by = sign * if large { large_step(column) } else { 1 };
    let (low, top) = (i32::from(column.lowest()), i32::from(column.highest()));
    (i32::from(value) + by).clamp(low, top) as u8
}

/// How far a large step moves a value of a column, with the scale off: an octave of notes, four
/// 24ths of a nudge, or `10` in hex.
fn large_step(column: ShapeColumn) -> i32 {
    match column {
        ShapeColumn::Note { .. } => 12,
        ShapeColumn::Nudge { .. } => 4,
        _ => HEX_JUMP,
    }
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
    !matches!(column, ShapeColumn::Note { .. } | ShapeColumn::Nudge { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;

    fn shape(kind: FillKind) -> Shape {
        Shape {
            from: 0x20,
            to: 0x80,
            kind,
            place: Where::Notes,
            seed: 1,
            ..Shape::default()
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
        let of = |kind, generates| GenParam::of(&shape(kind), note(), generates);
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
            of(FillKind::Variation, false),
            [Fill, Order, Temperature, Vary],
            "a Variation learns its rows with its notes, so it has no Where"
        );
        assert_eq!(of(FillKind::InvertBits, false), [Fill, Flip, Where]);
        assert_eq!(of(FillKind::MaskBits, false), [Fill, Keep, Where]);
        assert_eq!(of(FillKind::RotateBits, false), [Fill, Turn, Where]);
        assert_eq!(
            of(FillKind::Acid, true),
            [
                Fill, From, To, Notes, Spread, Bias, Octave, Density, Accent, Slide, Pitches,
                TrigLoop, NoteLoop, OctaveLoop, AccentLoop, SlideLoop, Where, Vary
            ]
        );
        let mut kept = shape(FillKind::Acid);
        kept.acid.keep = true;
        assert_eq!(
            GenParam::of(&kept, note(), false),
            [
                Fill, Octave, Accent, Slide, Pitches, OctaveLoop, AccentLoop, SlideLoop, Where,
                Vary
            ],
            "keeping the pitches there leaves no notes to loop, and no rows to loop on"
        );
        // An `Acid` riff's `Spread` and `Bias` take the Gen page's fourth column, as its three are
        // full with Euclid's, and keeping the pitches there leaves them nothing to lean.
        let mut riff = shape(FillKind::Acid);
        riff.place = crate::shape::Where::Euclid { hits: 5, rotate: 0 };
        assert_eq!(
            GenParam::grid(&riff, note(), true),
            Grid::from_columns(&[
                [Some(From), Some(To), Some(Notes)],
                [Some(Octave), Some(Accent), Some(Slide)],
                [Some(Density), Some(Hits), Some(Rotate)],
                [Some(Spread), Some(Bias), None],
            ])
        );
        assert_eq!(
            GenParam::grid(&kept, note(), false).column(3),
            [None, None, None]
        );
        let mut euclid = shape(FillKind::Alternate);
        euclid.place = crate::shape::Where::Euclid { hits: 3, rotate: 0 };
        assert_eq!(
            GenParam::of(&euclid, note(), false),
            [Fill, From, To, Run, Where, Hits, Rotate]
        );
        euclid.place = crate::shape::Where::Chance(50);
        assert!(
            GenParam::of(&euclid, note(), false).ends_with(&[Where, Chance, Vary]),
            "rows chosen by chance are drawn, so they vary"
        );
        let mut random = shape(FillKind::Wave);
        random.wave = crate::lfo::Shape::Smooth;
        assert!(GenParam::of(&random, note(), false).ends_with(&[Where, Vary]));
        let mut follow = shape(FillKind::Follow);
        assert_eq!(
            GenParam::of(&follow, note(), false),
            [
                Fill, Source, Transpose, Invert, Reverse, Late, CopyFx, Mutation
            ],
            "a Follow writes every row, so it has no Where"
        );
        follow.follow.invert = true;
        assert!(GenParam::of(&follow, note(), false).contains(&Axis));
        // Mutating, it has the kinds it may make, and draws from its seed, so it varies.
        follow.follow.mutation = 2;
        assert!(
            GenParam::of(&follow, note(), false)
                .ends_with(&[Mutation, Repitch, Shift, Retro, Ratchet, DropAdd, Vary])
        );
        follow.follow.mutate = crate::shape::Mutations {
            repitch: false,
            shift: false,
            retro: false,
            ratchet: false,
            drop: false,
        };
        assert!(
            !GenParam::of(&follow, note(), false).contains(&Vary),
            "with no kind allowed it copies, drawing nothing"
        );
    }

    /// `Mutation` counts changes, and each kind is a switch that `x` puts back as it starts.
    #[test]
    fn a_follows_mutation_counts_changes_and_its_kinds_switch() {
        let mut follow = shape(FillKind::Follow);
        let notes = context(note());
        assert_eq!(GenParam::Mutation.shown(&follow, &notes), "Off");
        GenParam::Mutation
            .step(&mut follow, &notes, 1, false)
            .unwrap();
        assert_eq!(GenParam::Mutation.shown(&follow, &notes), "1 change");
        GenParam::Mutation
            .step(&mut follow, &notes, 1, true)
            .unwrap();
        assert_eq!(GenParam::Mutation.shown(&follow, &notes), "5 changes");
        GenParam::Mutation
            .typed(&mut follow, &notes, "40")
            .unwrap_err();
        GenParam::Mutation.typed(&mut follow, &notes, "32").unwrap();
        GenParam::Mutation
            .step(&mut follow, &notes, 1, true)
            .unwrap();
        assert_eq!(follow.follow.mutation, crate::shape::MAX_MUTATION);
        assert_eq!(GenParam::Shift.shown(&follow, &notes), "Off");
        GenParam::Shift.step(&mut follow, &notes, 1, false).unwrap();
        assert!(follow.follow.mutate.shift);
        GenParam::Ratchet.set_number(&mut follow, &notes, 0);
        assert!(!follow.follow.mutate.ratchet);
        assert_eq!(GenParam::Ratchet.number(&follow, &notes), Some(0));
        for kind in [GenParam::Shift, GenParam::Ratchet] {
            kind.reset(&mut follow, &notes, Starts::default());
        }
        assert!(follow.follow.mutate.is_default());
        assert_eq!(GenParam::by_key("drop"), Some(GenParam::DropAdd));
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
        assert!(!GenParam::Vary.exists_for(&shape(FillKind::Ramp), note(), false));
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

    /// A `Cells` `Where` has its rule, `0` to `255`, and where it starts, a list, under it on the
    /// Gen page as Euclid's are; a generator across tracks has no `Start`, as theirs is worked
    /// out once for all of them; and it has `Vary` on any fill, as each pass steps it on.
    #[test]
    fn a_cells_rule_and_start() {
        use GenParam::*;
        let velocity = context(ShapeColumn::Velocity);
        let mut ramp = shape(FillKind::Ramp);
        let cells = GenParam::Where
            .choices(&velocity)
            .unwrap()
            .into_iter()
            .find(|choice| choice.name() == "Cells")
            .expect("Cells is a Where");
        cells.apply(&mut ramp);
        assert_eq!(
            GenParam::of(&ramp, ShapeColumn::Velocity, false),
            [Fill, From, To, Curve, S, Run, Where, Rule, CellStart, Vary]
        );
        assert_eq!(Rule.shown(&ramp, &velocity), "30");
        assert_eq!(CellStart.shown(&ramp, &velocity), "Seed");
        Rule.step(&mut ramp, &velocity, 1, true).unwrap();
        assert_eq!(Rule.number(&ramp, &velocity), Some(40));
        Rule.typed(&mut ramp, &velocity, "110").unwrap();
        assert_eq!(Rule.brief(&ramp, &velocity), "110");
        assert!(Rule.typed(&mut ramp, &velocity, "256").is_err());
        Rule.set_number(&mut ramp, &velocity, 300);
        assert_eq!(
            Rule.number(&ramp, &velocity),
            Some(255),
            "within its bounds"
        );
        Rule.step(&mut ramp, &velocity, 1, false).unwrap();
        assert_eq!(Rule.number(&ramp, &velocity), Some(255));
        Rule.reset(&mut ramp, &velocity, Starts::default());
        assert_eq!(Rule.number(&ramp, &velocity), Some(30));
        let names: Vec<String> = CellStart
            .choices(&velocity)
            .unwrap()
            .into_iter()
            .map(Choice::name)
            .collect();
        assert_eq!(names, ["Seed", "One", "Hits"]);
        CellStart.step(&mut ramp, &velocity, 1, false).unwrap();
        assert_eq!(CellStart.shown(&ramp, &velocity), "One");
        assert_eq!(CellStart.bounds(&velocity), Some((0, 2)));
        CellStart.set_number(&mut ramp, &velocity, 2);
        assert_eq!(CellStart.shown(&ramp, &velocity), "Hits");
        assert_eq!(CellStart.typing(ShapeColumn::Velocity), Typing::Listed);
        CellStart.reset(&mut ramp, &velocity, Starts::default());
        assert_eq!(CellStart.number(&ramp, &velocity), Some(0));
        assert!(Rule.held_in_engine() && CellStart.held_in_engine());
        assert_eq!(
            GenParam::grid(&ramp, ShapeColumn::Velocity, false).column(2),
            [None, Some(Rule), Some(CellStart)]
        );
        assert_eq!(GenParam::title(&ramp)[2], Some(Vary));
        // Across tracks.
        let super::Where::Cells(own) = ramp.place else {
            unreachable!()
        };
        ramp.place = super::Where::Cells(own.for_track(0, 1, &[false; 4]));
        assert!(!CellStart.exists_for(&ramp, ShapeColumn::Velocity, false));
        assert!(Rule.exists_for(&ramp, ShapeColumn::Velocity, false));
        // Without it, neither.
        assert_eq!(Rule.number(&shape(FillKind::Ramp), &velocity), None);
    }

    /// A `Divisions`' chances run from 0 to 100%, its channel and bar are lists, a track of its own
    /// has its own channel's chances and its `Note`, and `A+B` has `B`'s as well, its twelve
    /// filling the Gen page.
    #[test]
    fn a_divisions_chances_channel_and_bar() {
        use GenParam::*;
        let notes = context(note());
        let mut divisions = shape(FillKind::Divisions);
        let own = [
            Quarters,
            Eighths,
            Sixteenths,
            QuarterTriplets,
            EighthTriplets,
            SixteenthTriplets,
        ];
        let theirs = [
            QuartersB,
            EighthsB,
            SixteenthsB,
            QuarterTripletsB,
            EighthTripletsB,
            SixteenthTripletsB,
        ];
        let mut wanted = vec![Fill, Pitch, Channel, Bar];
        wanted.extend(own);
        wanted.push(Vary);
        assert_eq!(GenParam::of(&divisions, note(), true), wanted);
        assert!(!GenParam::of(&divisions, note(), false).contains(&Pitch));
        assert_eq!(Quarters.shown(&divisions, &notes), "75%");
        assert_eq!(Quarters.name(), "1/4");
        assert_eq!(EighthTripletsB.name(), "B 1/8T");
        assert_eq!(Channel.shown(&divisions, &notes), "A");
        assert_eq!(Bar.shown(&divisions, &notes), "4/4 16ths");
        for _ in 0..4 {
            Quarters.step(&mut divisions, &notes, 1, true).unwrap();
        }
        assert_eq!(divisions.divisions.chances[0][0], 100);
        EighthTriplets.typed(&mut divisions, &notes, "250").unwrap();
        assert_eq!(divisions.divisions.chances[0][4], 100);
        assert!(Sixteenths.typed(&mut divisions, &notes, "lots").is_err());
        assert_eq!(Quarters.bounds(&notes), Some((0, 100)));
        Quarters.reset(&mut divisions, &notes, Starts::default());
        EighthTriplets.reset(&mut divisions, &notes, Starts::default());
        assert!(divisions.divisions.is_default());
        // The channel and the bar are lists the engine can hold by their places.
        Bar.step(&mut divisions, &notes, 1, false).unwrap();
        assert_eq!(divisions.divisions.bar, super::Bar::Three);
        assert_eq!(Bar.number(&divisions, &notes), Some(1));
        Channel.set_number(&mut divisions, &notes, 2);
        assert_eq!(divisions.divisions.channel, Some(super::Channel::Both));
        assert!(Channel.held_in_engine() && Bar.held_in_engine());
        assert_eq!(
            Channel
                .choices(&notes)
                .unwrap()
                .into_iter()
                .map(Choice::name)
                .collect::<Vec<_>>(),
            ["A", "B", "A+B"]
        );
        // `A+B` has B's chances, and its twelve fill the page.
        let mut both = GenParam::of(&divisions, note(), false);
        both.retain(|param| param.units() == Units::Percent);
        assert_eq!(both, [own, theirs].concat());
        assert_eq!(
            GenParam::title(&divisions),
            [Some(Fill), Some(Channel), Some(Bar), Some(Vary)]
        );
        let grid = GenParam::grid(&divisions, note(), true);
        assert_eq!(
            grid.column(2),
            [Some(QuartersB), Some(EighthsB), Some(SixteenthsB)]
        );
        assert!(grid.settings().all(|param| param != Pitch));
        // On a track of its own, its own six, then its note.
        divisions.divisions.channel = Some(super::Channel::B);
        let grid = GenParam::grid(&divisions, note(), true);
        assert_eq!(
            grid.column(0),
            [Some(Quarters), Some(Eighths), Some(Sixteenths)]
        );
        assert_eq!(grid.column(2), [Some(Pitch), None, None]);
        assert_eq!(grid.column(3), [None, None, None]);
        // Across tracks, no channel and every chance.
        divisions.divisions.channel = None;
        assert!(!GenParam::of(&divisions, note(), false).contains(&Channel));
        assert!(GenParam::of(&divisions, note(), false).contains(&QuartersB));
        // It writes notes, so only a Note column offers it.
        assert!(notes.fills().contains(&FillKind::Divisions));
        assert!(
            !context(ShapeColumn::Velocity)
                .fills()
                .contains(&FillKind::Divisions)
        );
    }

    /// A `Ring` has its ends, `Change` and `Chance` from 0 to 100% and `Length` from 1 to 16, and
    /// no `Where` or `Vary`, as it makes its rhythm and runs on from pass to pass by itself.
    #[test]
    fn a_rings_change_chance_and_length() {
        use GenParam::*;
        let notes = context(note());
        let mut ring = shape(FillKind::Ring);
        assert_eq!(
            GenParam::of(&ring, note(), true),
            [Fill, From, To, Change, Ones, RingLength]
        );
        assert_eq!(GenParam::title(&ring), [Some(Fill), None, None, None]);
        assert_eq!(
            GenParam::grid(&ring, note(), true).column(1),
            [Some(Change), Some(Ones), Some(RingLength)]
        );
        assert_eq!(
            (Change.name(), Ones.name(), RingLength.name()),
            ("Change", "Chance", "Length")
        );
        assert_eq!(
            (Change.key(), Ones.key(), RingLength.key()),
            ("change", "ones", "length")
        );
        assert_eq!(Change.shown(&ring, &notes), "10%");
        assert_eq!(Ones.shown(&ring, &notes), "50%");
        assert_eq!(RingLength.shown(&ring, &notes), "8");
        for _ in 0..12 {
            Change.step(&mut ring, &notes, 1, true).unwrap();
            RingLength.step(&mut ring, &notes, 1, true).unwrap();
        }
        assert_eq!((ring.ring.change, ring.ring.length), (100, 16));
        Ones.typed(&mut ring, &notes, "250").unwrap();
        assert_eq!(ring.ring.chance, 100);
        assert!(RingLength.typed(&mut ring, &notes, "17").is_err());
        assert_eq!(RingLength.bounds(&notes), Some((1, 16)));
        assert_eq!(Ones.bounds(&notes), Some((0, 100)));
        RingLength.set_number(&mut ring, &notes, 3);
        assert_eq!(RingLength.number(&ring, &notes), Some(3));
        for param in [Change, Ones, RingLength] {
            assert!(param.held_in_engine());
            param.reset(&mut ring, &notes, Starts::default());
        }
        assert!(ring.ring.is_default());
        // It writes notes, so only a Note column offers it.
        assert!(notes.fills().contains(&FillKind::Ring));
        assert!(
            !context(ShapeColumn::Velocity)
                .fills()
                .contains(&FillKind::Ring)
        );
    }

    /// `Order` goes from 1 to 3 and `Temperature` from 0 to 100%, each put back to where it starts.
    #[test]
    fn a_variations_order_and_temperature_stay_within_their_ranges() {
        let notes = context(note());
        let mut variation = shape(FillKind::Variation);
        assert_eq!(GenParam::Order.shown(&variation, &notes), "1");
        assert_eq!(GenParam::Temperature.shown(&variation, &notes), "50%");
        for _ in 0..4 {
            GenParam::Order
                .step(&mut variation, &notes, 1, true)
                .unwrap();
        }
        assert_eq!(variation.variation.order, 3);
        GenParam::Temperature
            .step(&mut variation, &notes, -1, true)
            .unwrap();
        assert_eq!(variation.variation.temperature, 40);
        assert!(GenParam::Order.typed(&mut variation, &notes, "4").is_err());
        GenParam::Temperature
            .typed(&mut variation, &notes, "250")
            .unwrap();
        assert_eq!(variation.variation.temperature, 100);
        assert_eq!(GenParam::Order.bounds(&notes), Some((1, 3)));
        GenParam::Order.reset(&mut variation, &notes, Starts::default());
        GenParam::Temperature.reset(&mut variation, &notes, Starts::default());
        assert!(variation.variation.is_default());
        assert_eq!(
            GenParam::grid(&variation, note(), false).column(0),
            [Some(GenParam::Order), Some(GenParam::Temperature), None]
        );
        assert_eq!(
            GenParam::title(&variation),
            [Some(GenParam::Fill), Some(GenParam::Vary), None, None]
        );
        // It learns from the notes there, so a column with none doesn't offer it.
        let generated = Context {
            generates: true,
            ..notes
        };
        assert!(notes.fills().contains(&FillKind::Variation));
        assert!(!generated.fills().contains(&FillKind::Variation));
        assert!(
            !context(ShapeColumn::Velocity)
                .fills()
                .contains(&FillKind::Variation)
        );
    }

    #[test]
    fn the_gen_page_places_every_setting_a_shape_has_once() {
        for kind in FillKind::ALL {
            for place in note().wheres() {
                for generates in [false, true] {
                    let mut base = shape(kind);
                    base.place = place;
                    base.follow.invert = true;
                    base.follow.mutation = 2;
                    let placed: Vec<GenParam> = GenParam::title(&base)
                        .into_iter()
                        .chain(
                            GenParam::grid(&base, note(), generates)
                                .settings()
                                .map(Some),
                        )
                        // An `Acid`'s loops, on a device of their own under it.
                        .chain(
                            GenParam::loops_grid(&base, note(), generates)
                                .into_iter()
                                .flat_map(Grid::settings)
                                .map(Some),
                        )
                        .flatten()
                        .collect();
                    let mut wanted = GenParam::of(&base, note(), generates);
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
                base.follow.mutation = 2;
                base.follow.transpose = -48;
                base.run = MAX_RUN;
                for param in GenParam::of(&base, note(), true) {
                    // Lists show in full, as they have no bar beside them.
                    if matches!(param.units(), Units::Named | Units::Weights)
                        || param == GenParam::Values
                    {
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
        // An `Every` typed past the most rows it can skip stops there, as a step does.
        ramp.place = Where::Every(4);
        GenParam::Every.typed(&mut ramp, &velocity, "1000").unwrap();
        assert_eq!(ramp.place, Where::Every(MAX_EVERY));
        assert!(GenParam::Every.typed(&mut ramp, &velocity, "0").is_err());
        GenParam::From.typed(&mut ramp, &velocity, "4a").unwrap();
        assert_eq!(ramp.from, 0x4A);
        let starts = Starts {
            ends: (0x10, 0x70),
            source: None,
            part: None,
        };
        GenParam::From.reset(&mut ramp, &velocity, starts);
        assert_eq!(ramp.from, 0x10);
        assert_eq!(GenParam::From.typing(ShapeColumn::Velocity), Typing::Hex);
        assert_eq!(GenParam::From.typing(note()), Typing::Named { list: false });
        assert_eq!(GenParam::Where.typing(note()), Typing::Listed);
    }

    #[test]
    fn an_acids_loops_read_in_rows_and_off_and_sit_on_a_device_of_their_own() {
        let generated = Context {
            generates: true,
            ..context(note())
        };
        let mut acid = shape(FillKind::Acid);
        let shown = |acid: &Shape, param: GenParam| {
            (param.shown(acid, &generated), param.brief(acid, &generated))
        };
        assert_eq!(
            shown(&acid, GenParam::NoteLoop),
            ("Off".into(), "Off".into())
        );
        GenParam::NoteLoop
            .typed(&mut acid, &generated, "7")
            .unwrap();
        assert_eq!(acid.acid.loops.notes, 7);
        assert_eq!(
            shown(&acid, GenParam::NoteLoop),
            ("7 rows".into(), "7".into())
        );
        GenParam::TrigLoop
            .step(&mut acid, &generated, 1, false)
            .unwrap();
        assert_eq!(
            shown(&acid, GenParam::TrigLoop),
            ("1 row".into(), "1".into())
        );
        assert!(
            GenParam::TrigLoop
                .typed(&mut acid, &generated, "65")
                .is_err()
        );
        GenParam::TrigLoop
            .typed(&mut acid, &generated, "64")
            .unwrap();
        GenParam::TrigLoop
            .step(&mut acid, &generated, 1, true)
            .unwrap();
        assert_eq!(acid.acid.loops.trigs, 64, "64 rows at most");
        GenParam::TrigLoop.reset(&mut acid, &generated, Starts::default());
        assert_eq!(acid.acid.loops.trigs, 0, "x turns it off");
        assert_eq!(GenParam::NoteLoop.bounds(&generated), Some((0, 64)));
        // The fill's own grid is full, so they're a device of their own; none for another fill.
        assert_eq!(
            GenParam::loops_grid(&acid, note(), true),
            Some(Grid::from_columns(&[
                [Some(GenParam::TrigLoop), Some(GenParam::NoteLoop), None],
                [
                    Some(GenParam::OctaveLoop),
                    Some(GenParam::AccentLoop),
                    Some(GenParam::SlideLoop)
                ],
            ]))
        );
        assert_eq!(
            GenParam::loops_grid(&acid, note(), false)
                .unwrap()
                .column(0),
            [None, Some(GenParam::NoteLoop), None],
            "on notes there, the rhythm is theirs"
        );
        assert_eq!(
            GenParam::loops_grid(&shape(FillKind::Random), note(), true),
            None
        );
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
                for param in GenParam::of(&base, note(), true) {
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
    fn whatever_is_typed_lands_within_its_bounds() {
        let columns = [
            note(),
            ShapeColumn::Velocity,
            ShapeColumn::Nudge { most: 11 },
            ShapeColumn::Length,
        ];
        let texts = [
            "0", "1", "-1", "8", "17", "65", "101", "256", "257", "360", "1000", "40000", "-200",
            "FF", "C-3", "C-9",
        ];
        for column in columns {
            let typing = context(column);
            for kind in column.fills() {
                for place in column.wheres() {
                    let mut base = shape(kind);
                    base.place = place;
                    for param in GenParam::of(&base, column, false) {
                        let Some((low, high)) = param.bounds(&typing) else {
                            continue;
                        };
                        for text in texts {
                            let mut typed = base;
                            if param.typed(&mut typed, &typing, text).is_err() {
                                continue;
                            }
                            if let Some(number) = param.number(&typed, &typing) {
                                assert!(
                                    (low..=high).contains(&number),
                                    "{} typed {text} on {} is {number}",
                                    param.key(),
                                    kind.name()
                                );
                            }
                        }
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

    /// `Random`'s own settings after Marbles (decision 302): `Spread` and `Bias` on every column,
    /// and `Steps` and `Weights` on notes, where there are degrees; as they start, they're left
    /// out of the song.
    #[test]
    fn a_random_fill_spreads_leans_and_weighs_its_degrees() {
        use GenParam::*;
        let mut random = shape(FillKind::Random);
        assert_eq!(
            GenParam::of(&random, note(), false),
            [
                Fill, From, To, Spread, Bias, Steps, Weights, Curve, S, Run, Where, Vary
            ]
        );
        assert_eq!(
            GenParam::of(&random, ShapeColumn::Velocity, false),
            [Fill, From, To, Spread, Bias, Curve, S, Run, Where, Vary]
        );
        assert_eq!(
            GenParam::grid(&random, note(), false),
            Grid::from_columns(&[
                [Some(From), Some(To), Some(Run)],
                [Some(Spread), Some(Bias), Some(Steps)],
                [Some(Weights), None, None],
                [None, None, None],
            ])
        );
        let notes = context(ShapeColumn::Note {
            scale: Scale {
                on: true,
                ..Scale::default()
            },
            writes: None,
        });
        assert_eq!(Spread.shown(&random, &notes), "50%");
        assert_eq!(Bias.shown(&random, &notes), "0");
        assert_eq!(Steps.shown(&random, &notes), "All 7");
        assert_eq!(Weights.shown(&random, &notes), "FFFFFFF");
        assert!(random.random.is_default());

        Bias.step(&mut random, &notes, 1, true).unwrap();
        assert_eq!(Bias.shown(&random, &notes), "+25");
        Spread.typed(&mut random, &notes, "150").unwrap();
        assert_eq!(random.random.spread, 100);
        Steps.step(&mut random, &notes, -1, false).unwrap();
        Steps.step(&mut random, &notes, -1, false).unwrap();
        Steps.step(&mut random, &notes, -1, false).unwrap();
        Steps.step(&mut random, &notes, -1, false).unwrap();
        assert_eq!(Steps.shown(&random, &notes), "3 of 7: C G E");
        assert_eq!(Steps.brief(&random, &notes), "3");
        Steps.typed(&mut random, &notes, "9").unwrap();
        assert_eq!(random.random.steps, 0, "past the last, every degree");
        assert!(Steps.typed(&mut random, &notes, "0").is_err());
        Steps.set_number(&mut random, &notes, 2);
        assert_eq!(Steps.number(&random, &notes), Some(2));
        assert_eq!(Steps.bounds(&notes), Some((1, 7)));
        assert_eq!(Steps.bounds(&context(ShapeColumn::Velocity)), None);

        Weights.typed(&mut random, &notes, "f0 8").unwrap();
        assert_eq!(Weights.shown(&random, &notes), "F08FFFF");
        assert!(
            Weights.typed(&mut random, &notes, "FFFFFFFF").is_err(),
            "eight"
        );
        assert!(Weights.typed(&mut random, &notes, "G").is_err());
        assert!(Weights.step(&mut random, &notes, 1, false).is_err());
        assert_eq!(Weights.typing(note()), Typing::Weights);
        assert_eq!(Weights.bounds(&notes), None);
        let chromatic = context(note());
        assert_eq!(Weights.shown(&random, &chromatic), "F08FFFFFFFFF");
        assert_eq!(Steps.shown(&random, &chromatic), "2 of 12: C G");

        // Spread and Bias are numbers the engine holds, so macros reach them; Weights isn't.
        assert!(Spread.held_in_engine() && Bias.held_in_engine() && Steps.held_in_engine());
        assert!(!Weights.held_in_engine());
        for param in [Spread, Bias, Steps, Weights] {
            param.reset(&mut random, &notes, Starts::default());
        }
        assert!(random.random.is_default());
    }

    #[test]
    fn a_named_rhythm_has_its_rhythm_part_and_child() {
        use GenParam::*;
        let column = ShapeColumn::Velocity;
        let context = context(column);
        let mut ramp = shape(FillKind::Ramp);
        ramp.place = super::Where::Rhythm {
            rhythm: World::Bembe,
            part: super::Role::Low,
            child: 70,
        };
        let have = GenParam::of(&ramp, column, false);
        for param in [Rhythm, Role, Child, Vary] {
            assert!(have.contains(&param), "{param:?}");
        }
        assert_eq!(
            (Rhythm.key(), Role.key(), Child.key()),
            ("rhythm", "role", "child")
        );
        assert_eq!(
            (Rhythm.name(), Role.name(), Child.name()),
            ("Rhythm", "Part", "Child")
        );
        assert_eq!(Rhythm.shown(&ramp, &context), "Bembé");
        assert_eq!(Role.shown(&ramp, &context), "Low drum");
        assert_eq!(Child.shown(&ramp, &context), "70%");
        assert_eq!(Rhythm.typing(column), Typing::Listed);
        assert_eq!(Role.typing(column), Typing::Listed);
        assert!(Rhythm.held_in_engine() && Role.held_in_engine() && Child.held_in_engine());
        // As numbers, the choices' places, so a macro reaches them.
        assert_eq!(
            Rhythm.bounds(&context),
            Some((0, World::ALL.len() as i32 - 1))
        );
        assert_eq!(
            Rhythm.number(&ramp, &context),
            Some(World::Bembe.index() as i32)
        );
        Rhythm.set_number(&mut ramp, &context, 0);
        Role.set_number(&mut ramp, &context, 2);
        Child.set_number(&mut ramp, &context, 500);
        let son = |part, child| super::Where::Rhythm {
            rhythm: World::SonClave,
            part,
            child,
        };
        assert_eq!(ramp.place, son(super::Role::High, 100));
        Child.typed(&mut ramp, &context, "35").unwrap();
        Child.step(&mut ramp, &context, 1, true).unwrap();
        assert_eq!(ramp.place, son(super::Role::High, 45));
        Rhythm.step(&mut ramp, &context, -1, false).unwrap();
        assert!(matches!(
            ramp.place,
            super::Where::Rhythm {
                rhythm: World::Motorik,
                ..
            }
        ));
        // `x` puts the rhythm and `Child` back and leaves the part, which the track's own is.
        let starts = Starts::default();
        Rhythm.reset(&mut ramp, &context, starts);
        Child.reset(&mut ramp, &context, starts);
        Role.reset(&mut ramp, &context, starts);
        assert_eq!(ramp.place, son(super::Role::High, 100));
        // At the foot of the third column; where a fill takes its top, the rhythm tops the fourth.
        let grid = GenParam::grid(&ramp, column, false);
        assert_eq!(grid.column(2), [Some(Rhythm), Some(Role), Some(Child)]);
        let mut random = ramp;
        random.kind = FillKind::Random;
        let grid = GenParam::grid(&random, column, false);
        assert_eq!(grid.column(2), [None, Some(Role), Some(Child)]);
        assert_eq!(grid.column(3)[0], Some(Rhythm));
        let mut acid = ramp;
        acid.kind = FillKind::Acid;
        let notes = ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x64),
        };
        let grid = GenParam::grid(&acid, notes, true);
        assert_eq!(grid.column(2), [Some(Density), Some(Role), Some(Child)]);
        assert_eq!(grid.column(3), [Some(Spread), Some(Bias), Some(Rhythm)]);
        // The list, each with where it's from.
        let listed = Rhythm.choices(&context).unwrap();
        assert_eq!(listed.len(), World::ALL.len());
        assert!(listed.iter().all(|choice| choice.description().is_some()));
        assert_eq!(Rhythm.chosen(&ramp, &context), Some(0));
    }
}
