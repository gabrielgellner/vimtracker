//! A generator's render: `render_into` writes what `written_from` wrote before it, for every fill
//! on every column, over many seeds and settings, and allocates nothing.
//!
//! What was written is kept as a hash for each fill and column, in `golden/renders.txt`, made from
//! the render as it was before `render_into`. `VT_BLESS=1` writes it again after an intended
//! change to a fill.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vt_model::constants::MAX_PHRASE_STEPS;
use vt_model::lfo::Rate;
use vt_model::random::SplitMix64;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::shape::OwnRhythm;
use vt_model::shape::{
    Acid, Across, Bar, Bits, CellStart, Cells, Channel, Divisions, DrumPart, Edge, FillKind,
    Follow, GenCell, Grids, Kept, MAX_DEPTH, MAX_FACTOR, MAX_LIST, MAX_MOVE_LOOP, MAX_MOVE_WEIGHT,
    MAX_MUTATION, MAX_RING, MAX_WEIGHT, MoveList, Moves, Mutations, Pink, Quality, Random, Ring,
    Role, RowView, Shape, ShapeColumn, Tonnetz, TonnetzMove, TonnetzVoice, Variation, WAVES, Walk,
    WaveRate, Where, World, render_into, render_tokens_into, written_from,
};
use vt_model::step::{FxCommand, FxSlot, Gate, GateKind, Mark, Note, Step};

use crate::rt_guard::forbidden;

/// SplitMix64, so the cases are the same on every run.
struct Dice(SplitMix64);

impl Dice {
    fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    fn next(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn below(&mut self, count: u64) -> u64 {
        self.next() % count.max(1)
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.below(from.len() as u64) as usize]
    }
}

const CUT: FxCommand = FxCommand::CUT;
const COMMANDS: [FxCommand; 4] = [
    FxCommand::CUT,
    FxCommand::Del,
    FxCommand::VOL,
    FxCommand::Ret,
];
const MARKS: [Mark; 3] = [Mark::Lock, Mark::Hold, Mark::Glide];
const KINDS: [GateKind; 4] = [
    GateKind::Single,
    GateKind::Multi,
    GateKind::Hold,
    GateKind::Rest,
];

/// A row of anything: a note, an `OFF` or nothing, with or without a velocity, nudge, gate and FX.
fn step(dice: &mut Dice) -> Step {
    let mut step = Step {
        note: match dice.below(10) {
            0..=2 => Note::Empty,
            3 => Note::Off,
            _ => Note::On(if dice.chance(70) {
                48 + dice.below(36) as u8
            } else {
                12 + dice.below(116) as u8
            }),
        },
        ..Step::default()
    };
    if dice.chance(60) {
        step.velocity = Some(dice.below(0x80) as u8);
    }
    if dice.chance(20) {
        step.nudge = dice.below(23) as i8 - 11;
    }
    if dice.chance(40) {
        step.gate = Some(Gate {
            kind: dice.pick(&KINDS),
            pulses: 1 + dice.below(8) as u8,
            length: dice.chance(50).then(|| dice.byte()),
        });
    }
    for slot in &mut step.fx {
        if dice.chance(45) {
            let command = dice.pick(&COMMANDS);
            let coarse = u16::from(dice.byte().min(command.highest()));
            let fine = if command.has_fine() && dice.chance(50) {
                u16::from(dice.byte())
            } else {
                0
            };
            *slot = FxSlot {
                command: Some(command),
                value: coarse << 8 | fine,
                mark: dice.pick(&MARKS),
            }
            .with_valid_mark();
        }
    }
    step
}

fn steps(dice: &mut Dice, count: usize) -> Vec<Step> {
    (0..count).map(|_| step(dice)).collect()
}

fn scale(dice: &mut Dice, on: bool) -> Scale {
    Scale {
        on,
        root: dice.below(12) as u8,
        kind: dice.pick(&[ScaleKind::Major, ScaleKind::Minor, ScaleKind::Dorian]),
    }
}

/// Every column a generator writes, by a name for the hashes.
fn columns(dice: &mut Dice) -> Vec<(&'static str, ShapeColumn)> {
    vec![
        (
            "Note, in key",
            ShapeColumn::Note {
                scale: scale(dice, true),
                writes: None,
            },
        ),
        (
            "Note, chromatic",
            ShapeColumn::Note {
                scale: scale(dice, false),
                writes: None,
            },
        ),
        (
            "Note of its own, in key",
            ShapeColumn::Note {
                scale: scale(dice, true),
                writes: Some(0x50),
            },
        ),
        (
            "Note of its own, chromatic",
            ShapeColumn::Note {
                scale: scale(dice, false),
                writes: Some(0x78),
            },
        ),
        ("Velocity", ShapeColumn::Velocity),
        ("Nudge", ShapeColumn::Nudge { most: 11 }),
        ("Length", ShapeColumn::Length),
        (
            "FX, fine",
            ShapeColumn::Fx {
                slot: 0,
                command: CUT,
            },
        ),
        (
            "FX, event",
            ShapeColumn::Fx {
                slot: 1,
                command: FxCommand::Del,
            },
        ),
        (
            "FX, last slot",
            ShapeColumn::Fx {
                slot: 2,
                command: FxCommand::VOL,
            },
        ),
    ]
}

fn place(dice: &mut Dice) -> Where {
    match dice.below(7) {
        0 => Where::Notes,
        1 => Where::NoNotes,
        2 => Where::Commands,
        3 => Where::AllRows,
        4 => Where::Every(dice.below(6) as u16),
        5 => Where::Chance(dice.below(101) as u8),
        _ => Where::Euclid {
            hits: dice.below(24) as u16,
            rotate: dice.below(40) as u16,
        },
    }
}

/// How many values a `List` drew at most, less one, when the list was in the shape: still drawn
/// so, so the draws after stay as they were.
const DRAWN_LIST: u64 = 10;

/// A shape of this fill with every other setting drawn, and a list of values for a `List`.
fn shape(dice: &mut Dice, kind: FillKind, column: ShapeColumn) -> (Shape, Vec<u8>) {
    let end = |dice: &mut Dice| {
        if dice.chance(80) {
            dice.below(u64::from(column.highest()) + 1) as u8
        } else {
            dice.byte()
        }
    };
    let bend = |dice: &mut Dice| {
        if dice.chance(30) {
            0
        } else {
            (dice.below(201) as i16 - 100) as i8
        }
    };
    let list: Vec<u8> = (0..dice.below(DRAWN_LIST)).map(|_| end(dice)).collect();
    let mut shape = Shape {
        from: end(dice),
        to: end(dice),
        curve: bend(dice),
        s: bend(dice),
        kind,
        wave: dice.pick(&WAVES),
        rate: if dice.chance(50) {
            WaveRate::Time(Rate::Note(
                dice.pick(&[120, 240, 480, 960, 1920, 3840, 640]),
            ))
        } else {
            WaveRate::Cycles(1 + dice.below(9) as u16)
        },
        phase: dice.below(360) as u16,
        run: if dice.chance(60) {
            1
        } else {
            dice.below(6) as u16
        },
        place: place(dice),
        seed: dice.next(),
        acid: Acid {
            notes: dice.below(10) as u8,
            octave: dice.below(101) as u8,
            // Its density, now `Where::Beats`', still drawn so the draws after stay as they were.
            accent: {
                dice.below(120);
                dice.below(101) as u8
            },
            slide: dice.below(101) as u8,
            keep: dice.chance(40),
            roll: if dice.chance(50) { 0 } else { dice.next() },
            // The loops are newer than the hashes, and tested in `shape/acid.rs`.
            ..Acid::default()
        },
        follow: Follow {
            source: Default::default(),
            transpose: dice.below(31) as i8 - 15,
            invert: dice.chance(40),
            axis: dice.chance(30).then(|| 24 + dice.below(80) as u8),
            reverse: dice.chance(40),
            rotate: dice.below(40) as u16,
            fx: dice.chance(50),
            ..Follow::default()
        },
        ..Shape::default()
    };
    // Settings added since are drawn from the seed, so the other draws, and the other fills'
    // hashes, stay as they were.
    let mut more = Dice::new(shape.seed ^ 0x3A1C);
    shape.walk = Walk {
        step: more.below(40) as u8,
        edge: more.pick(&[Edge::Reflect, Edge::Wrap]),
    };
    shape.pink = Pink {
        depth: more.below(u64::from(MAX_DEPTH) + 3) as u8,
    };
    shape.bits = Bits {
        factor: more.below(u64::from(MAX_FACTOR) + 2) as u8,
        masks: more.chance(40),
        flip: more.byte(),
        keep: more.byte(),
        turn: more.below(10) as u8,
    };
    shape.grids = Grids {
        x: more.byte(),
        y: more.byte(),
        density: [more.byte(), more.byte(), more.byte()],
        chaos: if more.chance(40) { 0 } else { more.byte() },
        part: more.chance(90).then(|| more.pick(&DrumPart::ALL)),
        fine: more.chance(30),
    };
    shape.variation = Variation {
        order: more.below(5) as u8,
        temperature: if more.chance(30) {
            0
        } else {
            more.below(120) as u8
        },
    };
    let mut chance = || match more.below(4) {
        0 => 0,
        1 => 100,
        _ => more.below(101) as u8,
    };
    let chances = [
        std::array::from_fn(|_| chance()),
        std::array::from_fn(|_| chance()),
    ];
    shape.divisions = Divisions {
        chances,
        channel: more.chance(90).then(|| more.pick(&Channel::ALL)),
        bar: more.pick(&Bar::ALL),
        // At 1/16 or 1/32, whose rows are the bar's, so every fill writes what it wrote before.
        ticks: None,
    };
    // A ring at any pass, kept on one now and then.
    let percent = |more: &mut Dice| match more.below(4) {
        0 => 0,
        1 => 100,
        _ => more.below(101) as u8,
    };
    shape.ring = Ring {
        change: percent(&mut more),
        chance: percent(&mut more),
        length: 1 + more.below(u64::from(MAX_RING)) as u8,
        kept: more.chance(20).then(|| Kept {
            bits: more.next() as u16,
            pass: more.below(1000) as u32,
        }),
        pass: if more.chance(30) {
            0
        } else {
            more.below(100_000) as u32
        },
    };
    // A Tonnetz with any moves, voice and first chord, in scale or free.
    let moves: Vec<TonnetzMove> = (0..1 + more.below(8))
        .map(|_| more.pick(&TonnetzMove::ALL))
        .collect();
    shape.tonnetz = Tonnetz {
        quality: more.pick(&[Quality::Major, Quality::Minor]),
        moves: more.pick(&Moves::ALL),
        list: MoveList::new(&moves),
        weights: std::array::from_fn(|_| more.below(u64::from(MAX_MOVE_WEIGHT) + 1) as u8),
        length: 1 + more.below(u64::from(MAX_MOVE_LOOP)) as u8,
        voice: Some(more.pick(&TonnetzVoice::ALL)),
        in_scale: more.chance(50),
    };
    (shape, list)
}

/// A fill's settings added since its line was made, drawn, each such fill hashed again under a name
/// of its own (decision 302), so its own line, with them as they start, stays as it was.
type Variant = (&'static str, FillKind, fn(&mut Shape));

const VARIANTS: [Variant; 13] = [
    ("Random shaped", FillKind::Random, shaped_random),
    ("Follow mutated", FillKind::Follow, mutated_follow),
    ("Acid shaped", FillKind::Acid, shaped_random),
    ("Ramp, Marbles' rows", FillKind::Ramp, marbles_rows),
    ("Random, Marbles' rows", FillKind::Random, marbles_rows),
    ("Acid, Marbles' rows", FillKind::Acid, marbles_rows),
    ("Shuffle, Marbles' rows", FillKind::Shuffle, marbles_rows),
    ("Ramp, a named rhythm's rows", FillKind::Ramp, world_rows),
    (
        "Random, a named rhythm's rows",
        FillKind::Random,
        world_rows,
    ),
    ("Acid, a named rhythm's rows", FillKind::Acid, world_rows),
    ("Ramp on cells", FillKind::Ramp, on_cells),
    ("Random on cells", FillKind::Random, on_cells),
    ("List on cells", FillKind::List, on_cells),
];

/// A `Where` of `Cells` (decision 326), drawn from the seed: any rule, any start, on one column
/// or as one of up to eight tracks across, on any pass.
fn on_cells(shape: &mut Shape) {
    let mut more = Dice::new(shape.seed ^ 0xCE11);
    let rule = if more.chance(40) {
        more.pick(&[30, 90, 110, 184])
    } else {
        more.byte()
    };
    let start = more.pick(&CellStart::ALL);
    let across = more.chance(40).then(|| {
        let cells = 2 + more.below(7) as u8;
        Across {
            cell: more.below(u64::from(cells)) as u8,
            cells,
            row: more.next(),
        }
    });
    let stepped = if more.chance(30) {
        0
    } else {
        more.below(5000) as u16
    };
    shape.place = Where::Cells(Cells {
        rule,
        start,
        across,
        stepped,
    });
}

/// A named rhythm as the `Where` (decision 327), its rhythm, part and `Child` drawn from the seed,
/// some children at the ends.
fn world_rows(shape: &mut Shape) {
    let mut more = Dice::new(shape.seed ^ 0x2017);
    let child = if more.chance(30) {
        more.pick(&[0, 100])
    } else {
        more.below(101) as u8
    };
    shape.place = Where::Rhythm {
        rhythm: more.pick(&World::ALL),
        part: more.pick(&Role::ALL),
        child,
    };
}

/// One of Marbles' rhythms as the `Where` (decision 321), drawn from the seed, some at the ends of
/// its amount.
fn marbles_rows(shape: &mut Shape) {
    let mut more = Dice::new(shape.seed ^ 0x7A11);
    let amount = if more.chance(20) {
        more.pick(&[0, 100])
    } else {
        more.below(101) as u8
    };
    shape.place = match more.below(4) {
        0 => Where::Clusters(amount),
        1 => Where::Markov(amount),
        2 => Where::States(amount),
        _ => Where::Complementary {
            share: amount,
            second: more.chance(50),
        },
    };
}

/// A `Follow`'s mutations (decision 307), drawn from its seed: up to the most, each kind on or
/// off, as a Bloom branch's are.
fn mutated_follow(shape: &mut Shape) {
    let mut more = Dice::new(shape.seed ^ 0xB10F);
    shape.follow.mutation = more.below(u64::from(MAX_MUTATION) + 1) as u8;
    shape.follow.mutate = Mutations {
        repitch: more.chance(60),
        shift: more.chance(40),
        retro: more.chance(50),
        ratchet: more.chance(50),
        drop: more.chance(60),
    };
}

/// A `Random`'s spread, bias, steps and weights, drawn from its seed, some at their ends; an
/// `Acid`'s riff takes the spread and bias (decision 322).
fn shaped_random(shape: &mut Shape) {
    let mut more = Dice::new(shape.seed ^ 0x6A2B);
    let spread = if more.chance(25) {
        more.pick(&[0, 50, 100])
    } else {
        more.below(101) as u8
    };
    let bias = if more.chance(30) {
        0
    } else {
        (more.below(201) as i16 - 100) as i8
    };
    let steps = more.below(14) as u8;
    let weights = std::array::from_fn(|_| {
        if more.chance(50) {
            MAX_WEIGHT
        } else {
            more.below(u64::from(MAX_WEIGHT) + 1) as u8
        }
    });
    shape.random = Random {
        spread,
        bias,
        steps,
        weights,
    };
}

/// One case: the phrase's steps, the rows, and what a `Follow` reads.
struct Case {
    steps: Vec<Step>,
    range: StepRange,
    shape: Shape,
    /// What a `List` goes through, its first eight as the shape kept them, as a generator keeps
    /// the list beside its shape.
    list: Vec<u8>,
    /// A `Mini`'s line, compiled, as its generator keeps it beside its shape.
    line: OwnRhythm,
    source: Option<Vec<Step>>,
}

/// A `Mini`'s line on a column, of its values drawn from the dice, with groups, a choice, a
/// chance, a Euclid and a ratchet, at a `Step` of 1 to 4 rows.
fn mini_line(dice: &mut Dice, shape: &mut Shape, column: ShapeColumn) -> OwnRhythm {
    let words: Vec<String> = match column {
        ShapeColumn::Note { .. } => ["C-3", "3", "D#2", "8", "c4", "1"]
            .map(String::from)
            .to_vec(),
        ShapeColumn::Velocity => ["7F", "40", "20"].map(String::from).to_vec(),
        ShapeColumn::Nudge { .. } => ["0", "+3", "-2"].map(String::from).to_vec(),
        ShapeColumn::Length => ["40", "_", "80"].map(String::from).to_vec(),
        ShapeColumn::Fx { .. } => vt_model::notation::values(column)
            .into_iter()
            .take(4)
            .map(|offer| offer.text)
            .collect(),
    };
    let mut word = || words[dice.below(words.len() as u64) as usize].clone();
    let line = format!(
        "{} [{} ~ {}] <{} {}>*2 {}?30 . {}(3,8) [{} {} {}] {}*3",
        word(),
        word(),
        word(),
        word(),
        word(),
        word(),
        word(),
        word(),
        word(),
        word(),
        word()
    );
    shape.mini.step = 1 + dice.below(4) as u8;
    let kept = OwnRhythm::kept(shape, column, &line);
    kept.settle(shape);
    kept
}

fn case(dice: &mut Dice, kind: FillKind, column: ShapeColumn) -> Case {
    let count = dice.pick(&[1, 2, 5, 8, 16, 17, 33, 64, 100, 256]);
    let steps = steps(dice, count);
    let first = if dice.chance(60) {
        0
    } else {
        dice.below(count as u64) as usize
    };
    let last = if dice.chance(60) {
        count - 1
    } else if dice.chance(15) {
        // Past the stored rows, as nothing should ask but the render takes.
        count + dice.below(8) as usize
    } else {
        first + dice.below((count - first) as u64) as usize
    };
    let source = match dice.below(6) {
        0 => None,
        1 => Some(Vec::new()),
        _ => {
            let length = 1 + dice.below(40) as usize;
            Some(self::steps(dice, length))
        }
    };
    let (mut shape, mut list) = shape(dice, kind, column);
    list.truncate(8);
    let line = match kind {
        FillKind::Mini => mini_line(dice, &mut shape, column),
        _ => OwnRhythm::default(),
    };
    Case {
        steps,
        range: StepRange::between(first, last),
        shape,
        list,
        line,
        source,
    }
}

/// FNV-1a, as the engine's goldens hash.
fn fnv(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
}

const CASES: usize = 160;

/// A fill to hash by its name, with what a variant draws on top of its case, if it's one.
type Hashed = (&'static str, FillKind, Option<fn(&mut Shape)>);

/// Every fill by its name, then the variants, each with what it draws on top of a fill's case.
fn fills() -> Vec<Hashed> {
    FillKind::ALL
        .into_iter()
        .map(|kind| (kind.name(), kind, None))
        .chain(
            VARIANTS
                .into_iter()
                .map(|(name, kind, more)| (name, kind, Some(more))),
        )
        .collect()
}

/// What each fill writes on each column over its cases, hashed, with how many rows it wrote, keyed
/// by the two names. Each fill and column draws its cases from its names, so a fill added changes
/// no other's.
fn hashes(
    write: impl Fn(&Case, ShapeColumn) -> Vec<(usize, Step)>,
) -> BTreeMap<String, (String, usize)> {
    let mut found = BTreeMap::new();
    for (fill, kind, more) in fills() {
        for at in 0..columns(&mut Dice::new(0)).len() {
            let name = format!("{fill} on {}", columns(&mut Dice::new(0))[at].0);
            let mut seed = 0xcbf2_9ce4_8422_2325_u64;
            fnv(&mut seed, name.as_bytes());
            let mut dice = Dice::new(seed);
            let (mut hash, mut rows) = (0xcbf2_9ce4_8422_2325_u64, 0);
            for _ in 0..CASES {
                let column = columns(&mut dice)[at].1;
                let mut case = case(&mut dice, kind, column);
                if let Some(more) = more {
                    more(&mut case.shape);
                }
                for (row, step) in write(&case, column) {
                    fnv(&mut hash, format!("{row} {step:?}\n").as_bytes());
                    rows += 1;
                }
                fnv(&mut hash, b"|");
            }
            found.insert(name, (format!("{hash:016x}"), rows));
        }
    }
    found
}

/// The rows `render_into` writes, each with its step as its cell leaves it, which is also the row
/// as the engine reads it with the cell laid over.
fn rendered(case: &Case, column: ShapeColumn) -> Vec<(usize, Step)> {
    let mut cells = vec![GenCell::Pitch(0); case.range.len()];
    render_into(
        &case.steps,
        case.range,
        column,
        &case.shape,
        case.line.texts(&case.list),
        case.source.as_deref(),
        &mut cells,
    );
    let written = cells
        .into_iter()
        .enumerate()
        .filter(|(_, cell)| *cell != GenCell::Under)
        .map(|(offset, cell)| {
            let row = case.range.first + offset;
            let mut step = case.steps[row].clone();
            cell.apply(column, &mut step);
            // The engine's view of the row, with the cell laid over it, is the step it writes.
            let mut view = RowView::of(&case.steps[row]);
            view.lay(column, cell);
            assert_eq!(view, RowView::of(&step), "{cell:?} laid on row {row}");
            (row, step)
        })
        .collect();
    // The edit thread's wrapper writes the same.
    let wrapped = written_from(
        &case.steps,
        case.range,
        column,
        &case.shape,
        case.line.texts(&case.list),
        case.source.as_deref(),
    );
    assert_eq!(written, wrapped);
    written
}

#[test]
fn every_fill_writes_what_it_wrote_before_on_every_column() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/renders.txt");
    let found = hashes(rendered);
    // Every fill a column offers writes something over its cases, so each hash means something.
    for (fill, kind, _) in fills() {
        for (name, column) in columns(&mut Dice::new(0)) {
            let (_, rows) = found[&format!("{fill} on {name}")];
            assert!(
                rows > 0 || !column.fills().contains(&kind),
                "{fill} on {name} wrote nothing"
            );
        }
    }
    let text: String = found
        .iter()
        .map(|(name, (hash, _))| format!("{hash} {name}\n"))
        .collect();
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    let saved: BTreeMap<&str, &str> = saved
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(hash, name)| (name, hash))
        .collect();
    let changed: Vec<&str> = found
        .iter()
        .filter(|(name, (hash, _))| saved.get(name.as_str()) != Some(&hash.as_str()))
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(
        changed.is_empty() && saved.len() == found.len(),
        "these write something else: {changed:?}; run with VT_BLESS=1 if that was intended"
    );
}

/// A cell stays small, as the engine keeps one for each row of a moving generator's span: a whole
/// phrase of them is 128 KB. A `Follow` copying FX, with its three slots, is the largest.
#[test]
fn a_cell_is_at_most_32_bytes() {
    assert!(size_of::<GenCell>() <= 32, "{}", size_of::<GenCell>());
}

/// And with its token table (decision 460), which writes the same cells.
#[test]
fn every_fill_renders_on_every_column_without_allocating() {
    let mut dice = Dice::new(0xA110C);
    let mut cells = vec![GenCell::Under; MAX_PHRASE_STEPS];
    let mut kept = vec![GenCell::Under; MAX_PHRASE_STEPS];
    let mut tokens = vec![0; MAX_PHRASE_STEPS];
    for kind in FillKind::ALL {
        for (name, column) in columns(&mut dice) {
            // The longest phrase there is, and short ones, each `Where` among them.
            let cases: Vec<Case> = (0..8)
                .map(|nth| {
                    let mut case = case(&mut dice, kind, column);
                    if nth < 2 {
                        case.steps = steps(&mut dice, MAX_PHRASE_STEPS);
                        case.range = StepRange::between(0, MAX_PHRASE_STEPS - 1);
                        // A `List` as long as a phrase, which the longest goes round.
                        case.list = (0..MAX_LIST).map(|_| dice.byte()).collect();
                    }
                    if nth % 4 == 3 {
                        case.shape.place = Where::Euclid {
                            hits: 1 + dice.below(64) as u16,
                            rotate: dice.below(64) as u16,
                        };
                    }
                    // And Marbles' rhythms, the longest phrase's among them.
                    if nth % 4 == 0 {
                        marbles_rows(&mut case.shape);
                    }
                    // And the named rhythms.
                    if nth % 4 == 2 {
                        world_rows(&mut case.shape);
                    }
                    // Half of each fill's with its variants' settings drawn, weighed notes
                    // among them.
                    if nth % 2 == 1 {
                        for (_, varied, more) in VARIANTS {
                            if varied == kind {
                                more(&mut case.shape);
                            }
                        }
                    }
                    case
                })
                .collect();
            let ((), count) = forbidden(|| {
                for case in &cases {
                    render_into(
                        &case.steps,
                        case.range,
                        column,
                        &case.shape,
                        case.line.texts(&case.list),
                        case.source.as_deref(),
                        &mut cells[..case.range.len()],
                    );
                    let len = case.range.len();
                    render_tokens_into(
                        &case.steps,
                        case.range,
                        column,
                        &case.shape,
                        case.line.texts(&case.list),
                        case.source.as_deref(),
                        &mut kept[..len],
                        &mut tokens[..len],
                    );
                    assert!(
                        cells[..len] == kept[..len],
                        "the table's render is the same"
                    );
                }
            });
            assert_eq!(count, 0, "{} on {name} allocated", kind.name());
        }
    }
}
