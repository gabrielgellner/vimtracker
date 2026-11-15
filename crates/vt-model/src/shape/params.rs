//! A generator's settings, declared once: what each is called and stored by, the values it takes,
//! which shapes have it, and how it's shown, stepped, typed, chosen from a list and put back. The
//! shape panel's rows are these, and so are the Gen page's cells, its `K` lists and the Roto's
//! knobs; a macro will reach them by their keys (the design's `generators-page.md` §3.3).
//!
//! They're the registry's idea, [`crate::settings`], in a form of their own: a generator's
//! settings go past a byte (a `Run` of 256, `Hits` among thousands of rows), some are signed, and
//! what `From`, `To`, `Hits` and `Rotate` mean hangs on the column filled, which a [`Context`]
//! says. Each read and change is a pure function of the shape and its context, so the edit thread
//! and, later, the engine work them out alike.
//!
//! What's fixed of each setting is a row of [`GEN_PARAMS`]: its key, name and units, how it's
//! typed, its bounds and steps, and how it's read and set as a number. What hangs on the shape
//! besides, which shapes have it and how it's shown, put back, typed and chosen from a list, is
//! [`GenParam`]'s own, as each fill's own settings' places are its [`FillSpec`](super::FillSpec)'s.

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

mod spec;
mod table;

pub use table::GEN_PARAMS;

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
        // A large step is sixteen choices, stopping at the ends rather than going round.
        GenParam::Vary
            .step(&mut random, &velocity, 1, true)
            .unwrap();
        assert_eq!(random.vary, 8, "it stops at the last");
        GenParam::Vary
            .step(&mut random, &velocity, -1, true)
            .unwrap();
        assert_eq!(random.vary, 0, "and at the first");
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

    /// `Random`'s own settings after Marbles: `Spread` and `Bias` on every column, and `Steps` and
    /// `Weights` on notes, where there are degrees; as they start, they're left out of the song.
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
