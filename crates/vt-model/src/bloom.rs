//! Branches grown from a phrase, after Qu-Bit's Bloom (decision 307). The phrase is the trunk;
//! each branch is a new phrase on the same track whose Note column is a live `Follow` of the
//! branch before it, the first of the trunk, mutating what it copies, so each strays a little
//! further from the trunk and an edit to the trunk grows every branch again. A path, the order
//! the trunk and its branches play in, takes the trunk's place in its chain.

use crate::shape::{Follow, Mutations, Shape, Source, track_seed};

/// The most branches a bloom grows, as Bloom's `Branches` knob goes to 7.
pub const MAX_BRANCHES: u8 = 7;
/// The highest phrase ID, as the grid shows two hex digits.
pub const LAST_PHRASE: u32 = 0xFF;

/// Each path's word, in the order [`Path::ALL`] has them, for completing `:bloom`.
pub const PATH_WORDS: [&str; 5] = ["forward", "trunk", "pendulum", "ping-pong", "shuffle"];

/// The order a trunk and its branches play in, written into their chain. The fixed ones take the
/// names `:order` gives the same moves (decision 87).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Path {
    /// The trunk, then each branch in turn.
    #[default]
    Forward,
    /// The trunk before each branch, so it keeps coming back.
    Trunk,
    /// Out to the last branch and back, the ends once: `T 1 2 3 2 1`.
    Pendulum,
    /// Out and back, the ends twice: `T 1 2 3 3 2 1 T`.
    PingPong,
    /// The trunk, then each branch once, in an order drawn from the seed.
    Shuffle,
}

impl Path {
    /// Every path, in the order a list shows them.
    pub const ALL: [Self; 5] = [
        Self::Forward,
        Self::Trunk,
        Self::Pendulum,
        Self::PingPong,
        Self::Shuffle,
    ];

    /// Its word, as `:bloom` takes it.
    pub fn word(self) -> &'static str {
        PATH_WORDS[self as usize]
    }

    /// The path a word names.
    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|path| path.word() == word)
    }

    /// What it plays, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Forward => "The trunk, then each branch",
            Self::Trunk => "The trunk before each branch",
            Self::Pendulum => "Out to the last branch and back",
            Self::PingPong => "Out and back, the ends twice",
            Self::Shuffle => "The trunk, then the branches in a drawn order",
        }
    }

    /// What it plays with so many branches, by place: `0` the trunk and `n` the `n`th branch.
    pub fn places(self, branches: usize, seed: u64) -> Vec<usize> {
        let out: Vec<usize> = (1..=branches).collect();
        match self {
            Self::Forward => std::iter::once(0).chain(out).collect(),
            Self::Trunk => out.into_iter().flat_map(|branch| [0, branch]).collect(),
            Self::Pendulum => {
                let back = (1..branches).rev();
                std::iter::once(0).chain(out).chain(back).collect()
            }
            Self::PingPong => {
                let back = (0..=branches).rev();
                std::iter::once(0).chain(out).chain(back).collect()
            }
            Self::Shuffle => {
                // Fisher and Yates, from the last, each swap drawn from the seed.
                let mut dealt = out;
                for last in (1..dealt.len()).rev() {
                    let draw = track_seed(seed, last) % (last as u64 + 1);
                    dealt.swap(last, draw as usize);
                }
                std::iter::once(0).chain(dealt).collect()
            }
        }
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

/// What a bloom grows: how many branches, how much each mutates and how, the path through them,
/// and the seed each branch's draws are mixed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bloom {
    pub branches: u8,
    pub mutation: u8,
    pub mutate: Mutations,
    pub path: Path,
    pub seed: u64,
}

impl Default for Bloom {
    fn default() -> Self {
        Self {
            branches: 3,
            mutation: 2,
            mutate: Mutations::default(),
            path: Path::Forward,
            seed: 0,
        }
    }
}

impl Bloom {
    /// The live shape of the `nth` branch, counted from 1, following `source`: a copy of it, FX
    /// and all, as both are on one track, with this bloom's mutations drawn from a seed of the
    /// branch's own.
    pub fn branch(&self, source: Source, nth: usize) -> Shape {
        Shape::following(
            Follow {
                source,
                fx: true,
                mutation: self.mutation,
                mutate: self.mutate,
                ..Follow::default()
            },
            track_seed(self.seed, nth),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_path_plays_the_trunk_and_its_branches_in_its_order() {
        assert_eq!(Path::Forward.places(3, 0), [0, 1, 2, 3]);
        assert_eq!(Path::Trunk.places(3, 0), [0, 1, 0, 2, 0, 3]);
        assert_eq!(Path::Pendulum.places(3, 0), [0, 1, 2, 3, 2, 1]);
        assert_eq!(Path::PingPong.places(3, 0), [0, 1, 2, 3, 3, 2, 1, 0]);
        assert_eq!(Path::Pendulum.places(1, 0), [0, 1]);
        for seed in 0..20 {
            let mut dealt = Path::Shuffle.places(7, seed);
            assert_eq!(dealt[0], 0, "the trunk first");
            assert_eq!(dealt, Path::Shuffle.places(7, seed), "a seed repeats");
            dealt.sort_unstable();
            assert_eq!(dealt, [0, 1, 2, 3, 4, 5, 6, 7], "each branch once");
        }
        let orders: std::collections::BTreeSet<Vec<usize>> =
            (0..20).map(|seed| Path::Shuffle.places(7, seed)).collect();
        assert!(orders.len() > 10, "seeds deal other orders");
    }

    #[test]
    fn a_path_is_named_by_its_word() {
        for path in Path::ALL {
            assert_eq!(Path::named(path.word()), Some(path));
        }
        assert_eq!(Path::named("sideways"), None);
    }
}
