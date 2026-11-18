//! What hangs on the shape of each generator setting: which shapes have it, and how it's
//! shown, stepped, typed, chosen from a list and put back.

use super::*;

/// How many choices a large step moves a list, as a machine's lists and the track's go.
const LIST_JUMP: usize = 0x10;

impl GenParam {
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

    /// What it does, for the line that describes it ([`GenParamSpec::meaning`]).
    pub const fn meaning(self) -> &'static str {
        self.spec().meaning
    }

    /// How its values read.
    pub const fn reading(self) -> Reading {
        self.spec().units
    }

    /// Whether the engine can hold it as it plays, so a macro's lock or modulation reaches it
    /// (decision 300): not a list setting, whose choices are a `Vec` built as they're asked for,
    /// nor a `List`'s values or a `Random`'s `Weights` typed at once, which have no number, though
    /// each weight is a setting of its own (decision 425). A macro on `Wave` or `Rate` writes its
    /// column by hand instead.
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
    #[allow(
        clippy::match_same_arms,
        reason = "in the settings' order, a fill's together"
    )]
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
            Self::Weight1
            | Self::Weight2
            | Self::Weight3
            | Self::Weight4
            | Self::Weight5
            | Self::Weight6
            | Self::Weight7
            | Self::Weight8
            | Self::Weight9
            | Self::Weight10
            | Self::Weight11
            | Self::Weight12 => match column {
                ShapeColumn::Note { scale, .. } => {
                    kind == FillKind::Random
                        && self
                            .weight_degree()
                            .is_some_and(|at| at < degree_count(scale))
                }
                _ => false,
            },
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

    /// The settings a shape has on a column, in order, as the shape panel's rows: each of a
    /// `Random`'s weights is a digit of its `Weights` row there, not a row of its own.
    pub fn of(shape: &Shape, column: ShapeColumn, generates: bool) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|param| param.weight_degree().is_none())
            .filter(|param| param.exists_for(shape, column, generates))
            .collect()
    }

    /// Each of a `Random`'s weights, by its degree from the root, as settings of their own.
    pub const WEIGHTS: [Self; WEIGHTS] = [
        Self::Weight1,
        Self::Weight2,
        Self::Weight3,
        Self::Weight4,
        Self::Weight5,
        Self::Weight6,
        Self::Weight7,
        Self::Weight8,
        Self::Weight9,
        Self::Weight10,
        Self::Weight11,
        Self::Weight12,
    ];

    /// The degree from the root it's the weight of, if it's one of a `Random`'s weights.
    pub fn weight_degree(self) -> Option<usize> {
        Self::WEIGHTS.iter().position(|each| *each == self)
    }

    /// Where a `Random`'s weights sit on the Gen page, on a device of their own under the fill's,
    /// as its grid is full: a cell each for the degrees its column's scale has, in rows of four
    /// from the root, so twelve fill it. None for a shape without them.
    pub fn weights_grid(shape: &Shape, column: ShapeColumn) -> Option<Grid<Self>> {
        let has = |param: Self| param.exists_for(shape, column, false).then_some(param);
        if !Self::Weights.exists_for(shape, column, false) {
            return None;
        }
        let mut columns = [[None; GRID_ROWS]; GRID_COLUMNS];
        for (at, param) in Self::WEIGHTS.into_iter().enumerate() {
            columns[at % GRID_COLUMNS][at / GRID_COLUMNS] = has(param);
        }
        Some(Grid::from_columns(&columns))
    }

    /// Its value as the panel shows it: a note's name, `+20` for a bend, `9 of 16` for hits.
    /// A random fill has no curve, so it shows `--` there.
    #[allow(
        clippy::match_same_arms,
        reason = "in the settings' order, a fill's together"
    )]
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
            Self::Weight1
            | Self::Weight2
            | Self::Weight3
            | Self::Weight4
            | Self::Weight5
            | Self::Weight6
            | Self::Weight7
            | Self::Weight8
            | Self::Weight9
            | Self::Weight10
            | Self::Weight11
            | Self::Weight12 => format!("{:X}", number().unwrap_or(0)),
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
        match self.reading() {
            _ if self.weight_degree().is_some() => {
                number().map_or_else(String::new, |weight| format!("{weight:X}"))
            }
            _ if self.is_loop() => match number() {
                Some(0) | None => "Off".to_owned(),
                Some(rows) => rows.to_string(),
            },
            Reading::Number | Reading::Rows | Reading::OfRows => {
                number().map_or_else(String::new, |number| number.to_string())
            }
            Reading::Interval => number().map_or_else(String::new, |number| format!("{number:+}")),
            // A walk's step of notes in semitones or degrees, as its bar is of two octaves.
            Reading::Walk if matches!(context.column, ShapeColumn::Note { .. }) => {
                number().map_or_else(String::new, |number| number.to_string())
            }
            Reading::Column if self == Self::Axis => {
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

    /// Its value as a page counts it: a list setting's by the place of its choice, anything else
    /// as its number, and 0 where it has neither.
    pub fn value(self, shape: &Shape, context: &Context) -> i32 {
        self.chosen(shape, context)
            .map(|at| at as i32)
            .or_else(|| self.number(shape, context))
            .unwrap_or(0)
    }

    /// It described on a shape, for whatever shows or edits it: its range on the column, its steps
    /// from its row's [`Stepping`], a list's choices, which go round, and how it's typed. A value
    /// with bounds has a bar, unless it's a list; a bend and an interval draw it from the middle.
    /// Nothing puts a generator's setting back to a default of its own, so its default is where
    /// it is. Its text is the setting's to say, by the shape ([`GenParam::shown`]). A list's choices
    /// say what they do where they do, as the shape panel's `K` says it.
    pub fn described(self, shape: &Shape, context: &Context) -> Descriptor {
        let spec = self.spec();
        let value = self.value(shape, context);
        let bounds = self.bounds(context);
        let choices = self.choices(context);
        let listed = choices.is_some();
        let (nudge, jump) = match (listed, spec.stepping) {
            (true, _) => (1, HEX_JUMP),
            (false, Some(stepping)) => (stepping.nudge, stepping.jump.on(context.column)),
            (false, None) => (0, 0),
        };
        Descriptor {
            reading: spec.units,
            bounds: bounds.unwrap_or((value, value)),
            default: value,
            nudge,
            jump,
            wraps: listed,
            centred: matches!(spec.units, Reading::Bend | Reading::Interval),
            choices: choices
                .unwrap_or_default()
                .into_iter()
                .map(|choice| {
                    ParamChoice::named(choice.name()).meaning(choice.description().unwrap_or(""))
                })
                .collect(),
            typing: self.typing(context.column),
            reads: Reads::No,
            barred: spec.units != Reading::Listed
                && bounds.is_some()
                && self.number(shape, context).is_some(),
            meaning: std::borrow::Cow::Borrowed(spec.meaning),
            ..Descriptor::whole(spec.key, spec.name, (0, 0), 0)
        }
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
    /// list to its next choice, going round, or sixteen on, stopping at its ends; a phase and a
    /// rotation round their cycle. A `List`'s
    /// values are only typed, so they say how.
    #[allow(
        clippy::match_same_arms,
        reason = "in the settings' order, a fill's together"
    )]
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
                    let (size, last) = (choices.len(), choices.len() - 1);
                    let next = match (large, sign > 0) {
                        (false, true) => (at + 1) % size,
                        (false, false) => (at + last) % size,
                        (true, true) => (at + LIST_JUMP).min(last),
                        (true, false) => at.saturating_sub(LIST_JUMP),
                    };
                    choices[next].apply(shape);
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
    pub(super) fn put(self, shape: &mut Shape, context: &Context, number: i32) {
        let round = self.spec().stepping.is_some_and(|stepping| stepping.round);
        let number = match self.bounds(context) {
            Some((low, high)) if round => low + (number - low).rem_euclid(high - low + 1),
            _ => number,
        };
        self.set_number(shape, context, number);
    }
    /// Put it back to what it starts as: `From`, `To` and `Source` to where they `start`, a
    /// `Where` to its column's, and an amount to its own.
    #[allow(
        clippy::match_same_arms,
        reason = "in the settings' order, a fill's together"
    )]
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
            Self::Weight1
            | Self::Weight2
            | Self::Weight3
            | Self::Weight4
            | Self::Weight5
            | Self::Weight6
            | Self::Weight7
            | Self::Weight8
            | Self::Weight9
            | Self::Weight10
            | Self::Weight11
            | Self::Weight12 => {
                let start = Shape {
                    random: Random::default(),
                    ..*shape
                };
                self.take(shape, context, &start);
            }
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
    #[allow(
        clippy::match_same_arms,
        reason = "in the settings' order, a fill's together"
    )]
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
            Self::Weight1
            | Self::Weight2
            | Self::Weight3
            | Self::Weight4
            | Self::Weight5
            | Self::Weight6
            | Self::Weight7
            | Self::Weight8
            | Self::Weight9
            | Self::Weight10
            | Self::Weight11
            | Self::Weight12 => match u8::from_str_radix(text.trim(), 16) {
                Ok(weight) if within(weight.into()) => weight.into(),
                _ => return Err("A weight is a hex digit, from 0, never, to F"),
            },
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
    pub(super) fn take(self, shape: &mut Shape, context: &Context, start: &Shape) {
        if let Some(number) = (self.spec().get)(start, context) {
            (self.spec().set)(shape, context, number);
        }
    }

    /// Where the shape is among a list setting's choices, as a number.
    pub(super) fn chosen_number(self, shape: &Shape, context: &Context) -> Option<i32> {
        self.chosen(shape, context).map(|at| at as i32)
    }

    /// Give it the choice at a place, unless it's on that choice already, so a `Where` keeps its
    /// amount.
    pub(super) fn choose(self, shape: &mut Shape, context: &Context, at: i32) {
        if self.chosen(shape, context) != Some(at as usize)
            && let Some(choice) = self
                .choices(context)
                .and_then(|choices| choices.get(at as usize).copied())
        {
            choice.apply(shape);
        }
    }
}
