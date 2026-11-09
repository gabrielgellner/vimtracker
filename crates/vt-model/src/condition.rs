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
}

/// The most passes a cycle can count, as on Elektron's machines.
pub const MOST_PASSES: u8 = 8;

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
    /// cycle from 2 to 8, as Elektron lists them.
    pub fn all() -> impl Iterator<Item = Self> {
        NAMED.into_iter().chain(
            (2..=MOST_PASSES).flat_map(|of| (1..=of).map(move |pass| Self::Passes { pass, of })),
        )
    }

    /// The condition a `CND` value holds, if it's one: `01` to `08` for the named ones, or two
    /// digits `xy` for pass `x` of every `y`, so `12` is `1:2`.
    pub fn from_value(value: u8) -> Option<Self> {
        let (pass, of) = (value >> 4, value & 0x0F);
        match pass {
            0 => NAMED.get(usize::from(of).checked_sub(1)?).copied(),
            _ if (2..=MOST_PASSES).contains(&of) && pass <= of => Some(Self::Passes { pass, of }),
            _ => None,
        }
    }

    /// Its value in a `CND` slot.
    pub fn value(self) -> u8 {
        match self {
            Self::Passes { pass, of } => pass << 4 | of,
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
        }
    }

    /// Whether it reads what the last condition decided rather than adding to it, so it's left
    /// out of what `PRE` and `NEI` read next, as on Elektron's machines. That's what lets `PRE`
    /// and `!PRE` after a `1:2` play either side of it.
    pub fn reads_last(self) -> bool {
        matches!(self, Self::Previous(_) | Self::Neighbour(_))
    }

    /// The condition this many places on in the list, stopping at either end. A large step moves
    /// a pass to a longer or shorter cycle, keeping its place where it can, so `1:2` goes to `1:3`.
    pub fn moved(self, by: i32, large: bool) -> Self {
        if large && let Self::Passes { pass, of } = self {
            let of = (i32::from(of) + by).clamp(2, i32::from(MOST_PASSES)) as u8;
            return Self::Passes {
                pass: pass.min(of),
                of,
            };
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
        assert_eq!(all.len(), 8 + (2..=8).sum::<usize>());
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
        for value in [0x00, 0x09, 0x11, 0x21, 0x19, 0x99, 0xFF] {
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
            Condition::Passes { pass: 8, of: 8 }
        );
    }
}
