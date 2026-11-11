//! Trig conditions: whether a row plays this time round, after Elektron's. A `CND` in an FX slot
//! holds one as its value, so a part can change from pass to pass without each variation being
//! written out.

/// What a `CND` value asks before its row plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    /// While fill is on, or with `false`, while it's off.
    Fill(bool),
    /// Whether the last condition decided on this track passed, or with `false`, failed.
    Previous(bool),
    /// Likewise for the last condition decided on the track to the left.
    Neighbour(bool),
    /// Only the first time the track plays the row after its chain starts, or with `false`, every
    /// time but the first.
    First(bool),
    /// On pass `pass` of every `of`, counting from 1, so `1:2` is every other pass and `4:4` the
    /// last of four.
    Passes { pass: u8, of: u8 },
    /// From this pass on, counting from 1, so a part joins a build and stays: `>2` is `!1ST`.
    From(u8),
    /// Only on the passes before this one, so a part drops out of a build: `<2` is `1ST`.
    Until(u8),
}

/// The most passes a cycle can count, as on Elektron's machines.
pub const MOST_PASSES: u8 = 8;

/// The latest pass a build can start or end on. `9n` and `An` count from 2 to 15 by their second
/// digit, and `Bn` and `Cn` carry on from 16 to 31.
pub const LATEST_BUILD: u8 = 31;

/// The named conditions, in Elektron's order, whose values are `01` to `08`.
const NAMED: [Condition; 8] = [
    Condition::Fill(true),
    Condition::Fill(false),
    Condition::Previous(true),
    Condition::Previous(false),
    Condition::Neighbour(true),
    Condition::Neighbour(false),
    Condition::First(true),
    Condition::First(false),
];

/// What a condition is decided on, as a row's first pulse lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Situation {
    /// How many times the track has played the row before this since its chain started, so the
    /// first time is 0.
    pub pass: i64,
    /// Whether fill is on.
    pub fill: bool,
    /// Whether the last condition decided on this track passed.
    pub previous: bool,
    /// Whether the last condition decided on the track to the left passed.
    pub neighbour: bool,
}

impl Condition {
    /// Every condition, in the order a list shows them: the named ones, then each pass of each
    /// cycle from 2 to 8, as Elektron lists them, then the builds, from each pass and until each.
    pub fn all() -> impl Iterator<Item = Self> {
        NAMED
            .into_iter()
            .chain(
                (2..=MOST_PASSES)
                    .flat_map(|of| (1..=of).map(move |pass| Self::Passes { pass, of })),
            )
            .chain((2..=LATEST_BUILD).map(Self::From))
            .chain((2..=LATEST_BUILD).map(Self::Until))
    }

    /// The condition a `CND` value holds, if it's one: `01` to `08` for the named ones, two
    /// digits `xy` for pass `x` of every `y`, so `12` is `1:2`, `9n` from pass `n` on and `An`
    /// before it, so `95` is `>5` and `A3` is `<3`, and `Bn` and `Cn` the same from pass 16, so
    /// `B4` is `>20`.
    pub fn from_value(value: u8) -> Option<Self> {
        let (pass, of) = (value >> 4, value & 0x0F);
        match pass {
            0 => NAMED.get(usize::from(of).checked_sub(1)?).copied(),
            0x9 if of >= 2 => Some(Self::From(of)),
            0xA if of >= 2 => Some(Self::Until(of)),
            0xB => Some(Self::From(of + 16)),
            0xC => Some(Self::Until(of + 16)),
            _ if (2..=MOST_PASSES).contains(&of) && pass <= of => Some(Self::Passes { pass, of }),
            _ => None,
        }
    }

    /// Its value in a `CND` slot.
    pub fn value(self) -> u8 {
        let build = |low: u8, high: u8, pass: u8| match pass {
            ..16 => low << 4 | pass,
            _ => high << 4 | (pass - 16),
        };
        match self {
            Self::Passes { pass, of } => pass << 4 | of,
            Self::From(pass) => build(0x9, 0xB, pass),
            Self::Until(pass) => build(0xA, 0xC, pass),
            named => NAMED
                .iter()
                .position(|n| *n == named)
                .map_or(0, |at| at as u8 + 1),
        }
    }

    /// Its name, as Elektron's machines show it.
    pub fn label(self) -> String {
        let named = |name: &str, holds: bool| {
            if holds {
                name.to_owned()
            } else {
                format!("!{name}")
            }
        };
        match self {
            Self::Fill(holds) => named("FILL", holds),
            Self::Previous(holds) => named("PRE", holds),
            Self::Neighbour(holds) => named("NEI", holds),
            Self::First(holds) => named("1ST", holds),
            Self::Passes { pass, of } => format!("{pass}:{of}"),
            Self::From(pass) => format!(">{pass}"),
            Self::Until(pass) => format!("<{pass}"),
        }
    }

    /// When its row plays, for a list and the status line.
    pub fn meaning(self) -> String {
        match self {
            Self::Fill(true) => "While fill is on".to_owned(),
            Self::Fill(false) => "While fill is off".to_owned(),
            Self::Previous(true) => "When this track's last condition passed".to_owned(),
            Self::Previous(false) => "When this track's last condition failed".to_owned(),
            Self::Neighbour(true) => "When the left track's last condition passed".to_owned(),
            Self::Neighbour(false) => "When the left track's last condition failed".to_owned(),
            Self::First(true) => "The first time round only".to_owned(),
            Self::First(false) => "Every time round but the first".to_owned(),
            Self::Passes { pass, of } => format!("Pass {pass} of every {of}"),
            Self::From(pass) => format!("From pass {pass} on"),
            Self::Until(pass) => format!("Before pass {pass} only"),
        }
    }

    /// The group a list shows it in.
    pub fn group(self) -> &'static str {
        match self {
            Self::Fill(_) => "Fill",
            Self::Previous(_) => "Previous",
            Self::Neighbour(_) => "Neighbour",
            Self::First(_) => "First",
            Self::Passes { .. } => "Passes",
            Self::From(_) => "From",
            Self::Until(_) => "Until",
        }
    }

    /// Whether its row plays here.
    pub fn holds(self, at: Situation) -> bool {
        match self {
            Self::Fill(holds) => at.fill == holds,
            Self::Previous(holds) => at.previous == holds,
            Self::Neighbour(holds) => at.neighbour == holds,
            Self::First(holds) => (at.pass == 0) == holds,
            Self::Passes { pass, of } => at.pass.rem_euclid(i64::from(of)) == i64::from(pass) - 1,
            // `at.pass` counts from 0, and the build's pass from 1.
            Self::From(pass) => at.pass + 1 >= i64::from(pass),
            Self::Until(pass) => at.pass + 1 < i64::from(pass),
        }
    }

    /// Whether it reads what the last condition decided rather than adding to it, so it's left
    /// out of what `PRE` and `NEI` read next, as on Elektron's machines. That's what lets `PRE`
    /// and `!PRE` after a `1:2` play either side of it.
    pub fn reads_last(self) -> bool {
        matches!(self, Self::Previous(_) | Self::Neighbour(_))
    }

    /// The condition this many places on in the list, stopping at either end. A large step moves
    /// a pass to a longer or shorter cycle, keeping its place where it can, so `1:2` goes to `1:3`,
    /// and a build's pass by four, so `>5` goes to `>9`.
    pub fn moved(self, by: i32, large: bool) -> Self {
        let build = |pass: u8| (i32::from(pass) + by * 4).clamp(2, i32::from(LATEST_BUILD)) as u8;
        match self {
            Self::Passes { pass, of } if large => {
                let of = (i32::from(of) + by).clamp(2, i32::from(MOST_PASSES)) as u8;
                return Self::Passes {
                    pass: pass.min(of),
                    of,
                };
            }
            Self::From(pass) if large => return Self::From(build(pass)),
            Self::Until(pass) if large => return Self::Until(build(pass)),
            _ => {}
        }
        let list: Vec<Self> = Self::all().collect();
        let at = list.iter().position(|c| *c == self).unwrap_or(0) as i32;
        list[(at + by).clamp(0, list.len() as i32 - 1) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_condition_has_one_value_and_reads_back() {
        let all: Vec<Condition> = Condition::all().collect();
        assert_eq!(all.len(), 8 + (2..=8).sum::<usize>() + 2 * 30);
        for condition in &all {
            assert_eq!(Condition::from_value(condition.value()), Some(*condition));
        }
        let values: std::collections::HashSet<u8> = all.iter().map(|c| c.value()).collect();
        assert_eq!(values.len(), all.len());
        assert_eq!(
            Condition::from_value(0x12),
            Some(Condition::Passes { pass: 1, of: 2 })
        );
        assert_eq!(Condition::from_value(0x07), Some(Condition::First(true)));
        assert_eq!(Condition::from_value(0x95), Some(Condition::From(5)));
        assert_eq!(Condition::from_value(0xA3), Some(Condition::Until(3)));
        assert_eq!(Condition::from_value(0xB0), Some(Condition::From(16)));
        assert_eq!(Condition::from_value(0xCF), Some(Condition::Until(31)));
        for value in [
            0x00, 0x09, 0x11, 0x21, 0x19, 0x90, 0x91, 0xA0, 0xA1, 0xD0, 0xFF,
        ] {
            assert_eq!(Condition::from_value(value), None, "{value:02X}");
        }
    }

    #[test]
    fn labels_are_elektrons() {
        let labels: Vec<String> = Condition::all().take(10).map(Condition::label).collect();
        assert_eq!(
            labels,
            [
                "FILL", "!FILL", "PRE", "!PRE", "NEI", "!NEI", "1ST", "!1ST", "1:2", "2:2"
            ]
        );
    }

    #[test]
    fn a_pass_of_a_cycle_holds_once_in_each() {
        let holds = |condition: Condition| -> Vec<bool> {
            (0..8)
                .map(|pass| {
                    condition.holds(Situation {
                        pass,
                        ..Situation::default()
                    })
                })
                .collect()
        };
        let pattern = |text: &str| -> Vec<bool> { text.chars().map(|c| c == 'x').collect() };
        assert_eq!(
            holds(Condition::Passes { pass: 1, of: 2 }),
            pattern("x.x.x.x.")
        );
        assert_eq!(
            holds(Condition::Passes { pass: 2, of: 2 }),
            pattern(".x.x.x.x")
        );
        assert_eq!(
            holds(Condition::Passes { pass: 3, of: 4 }),
            pattern("..x...x.")
        );
        assert_eq!(
            holds(Condition::Passes { pass: 4, of: 4 }),
            pattern("...x...x")
        );
        assert_eq!(holds(Condition::First(true)), pattern("x......."));
        assert_eq!(holds(Condition::First(false)), pattern(".xxxxxxx"));
    }

    #[test]
    fn a_build_holds_from_its_pass_on_or_only_before_it() {
        let holds = |condition: Condition| -> String {
            (0..8)
                .map(|pass| {
                    let at = Situation {
                        pass,
                        ..Situation::default()
                    };
                    if condition.holds(at) { 'x' } else { '.' }
                })
                .collect()
        };
        assert_eq!(holds(Condition::From(5)), "....xxxx");
        assert_eq!(holds(Condition::Until(3)), "xx......");
        assert_eq!(holds(Condition::From(2)), holds(Condition::First(false)));
        assert_eq!(holds(Condition::Until(2)), holds(Condition::First(true)));
        assert_eq!(
            (Condition::From(5).label(), Condition::Until(16).label()),
            (">5".to_owned(), "<16".to_owned())
        );
    }

    #[test]
    fn moving_goes_through_the_list_and_a_large_step_changes_the_cycle() {
        let one_of_two = Condition::Passes { pass: 1, of: 2 };
        assert_eq!(Condition::First(false).moved(1, false), one_of_two);
        assert_eq!(
            Condition::Fill(true).moved(-1, false),
            Condition::Fill(true)
        );
        assert_eq!(
            one_of_two.moved(1, true),
            Condition::Passes { pass: 1, of: 3 }
        );
        assert_eq!(
            Condition::Passes { pass: 3, of: 3 }.moved(-1, true),
            Condition::Passes { pass: 2, of: 2 }
        );
        assert_eq!(
            Condition::Passes { pass: 8, of: 8 }.moved(1, false),
            Condition::From(2)
        );
        assert_eq!(Condition::From(31).moved(1, false), Condition::Until(2));
        assert_eq!(Condition::From(5).moved(1, true), Condition::From(9));
        assert_eq!(Condition::Until(30).moved(1, true), Condition::Until(31));
        assert_eq!(Condition::From(3).moved(-1, true), Condition::From(2));
        assert_eq!(
            Condition::Until(31).moved(1, false),
            Condition::Until(31),
            "the list's last"
        );
    }
}
