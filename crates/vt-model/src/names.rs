//! Random names for projects and recordings, such as `dusty-breakdown`: an adjective and a noun,
//! or one time in ten two nouns, as tracker-ce's name generator makes them. The word lists are our
//! own, in `names/`, one word to a line.

use std::time::{SystemTime, UNIX_EPOCH};

/// The adjectives that start most names.
pub const ADJECTIVES: &str = include_str!("names/adjectives.txt");

/// The nouns that end every name, and start one in ten.
pub const NOUNS: &str = include_str!("names/nouns.txt");

/// How many names are tried before one that's taken is given a number instead.
const TRIES: usize = 1000;

/// The words in a list, one to a line.
pub fn words(list: &'static str) -> impl Iterator<Item = &'static str> {
    list.lines().map(str::trim).filter(|word| !word.is_empty())
}

/// Makes random names, from a seed that each name moves on.
#[derive(Debug, Clone)]
pub struct Names {
    state: u64,
}

impl Names {
    /// Start from a seed, so the same names come each time.
    pub fn seeded(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Start from the clock, so each run names things differently. It's apart from the song's
    /// seed, so naming something doesn't change what the song's random settings do.
    pub fn from_clock() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64);
        Self::seeded(nanos ^ u64::from(std::process::id()).rotate_left(32))
    }

    /// The next random number, by SplitMix64.
    fn next_number(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A random word from a list.
    fn word(&mut self, list: &'static str) -> &'static str {
        let count = words(list).count() as u64;
        let at = (self.next_number() % count) as usize;
        words(list).nth(at).expect("the index is in the list")
    }

    /// A new name, two different words joined by a hyphen.
    pub fn name(&mut self) -> String {
        loop {
            let first = if self.next_number().is_multiple_of(10) {
                self.word(NOUNS)
            } else {
                self.word(ADJECTIVES)
            };
            let second = self.word(NOUNS);
            if first != second {
                return format!("{first}-{second}");
            }
        }
    }

    /// A new name that isn't taken, rolled again until one isn't. If every try is taken, the last
    /// is given the lowest number that frees it, as in `dusty-breakdown-2`.
    pub fn fresh(&mut self, taken: impl Fn(&str) -> bool) -> String {
        let mut name = self.name();
        for _ in 1..TRIES {
            if !taken(&name) {
                return name;
            }
            name = self.name();
        }
        numbered(&name, taken)
    }
}

/// A name, or if it's taken, the name with the lowest number after it from 2 that isn't.
pub fn numbered(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    (2..)
        .map(|number| format!("{name}-{number}"))
        .find(|name| !taken(name))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_lists_have_only_letters_and_no_word_twice() {
        for list in [ADJECTIVES, NOUNS] {
            let mut seen = HashSet::new();
            for word in words(list) {
                assert!(
                    word.chars().all(|c| c.is_ascii_lowercase()),
                    "{word:?} isn't plain lower case letters"
                );
                assert!(seen.insert(word), "{word:?} is in a list twice");
            }
            assert!(seen.len() >= 200, "a few hundred words, not {}", seen.len());
        }
    }

    #[test]
    fn a_name_is_two_listed_words_joined_by_a_hyphen() {
        let adjectives: HashSet<_> = words(ADJECTIVES).collect();
        let nouns: HashSet<_> = words(NOUNS).collect();
        let mut names = Names::seeded(7);
        let mut two_nouns = 0;
        for _ in 0..2000 {
            let name = names.name();
            let (first, second) = name.split_once('-').expect("a hyphen");
            assert_ne!(first, second, "{name}");
            assert!(nouns.contains(second), "{name} ends with a noun");
            assert!(
                adjectives.contains(first) || nouns.contains(first),
                "{name} starts with a listed word"
            );
            if !adjectives.contains(first) {
                two_nouns += 1;
            }
        }
        assert!(
            (100..300).contains(&two_nouns),
            "about one in ten is two nouns, not {two_nouns} in 2000"
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_names_and_they_vary() {
        let mut one = Names::seeded(42);
        let mut two = Names::seeded(42);
        assert_eq!(one.name(), two.name());
        let mut many = Names::seeded(1);
        let distinct: HashSet<_> = (0..50).map(|_| many.name()).collect();
        assert!(distinct.len() > 45, "only {} of 50 differ", distinct.len());
    }

    #[test]
    fn a_taken_name_is_rolled_again() {
        let first = Names::seeded(3).name();
        let second = Names::seeded(3).fresh(|name| name == first);
        assert_ne!(second, first);
        // With every name taken but numbered ones, it's given a number.
        let numbered = Names::seeded(3).fresh(|name| !name.ends_with("-2"));
        assert!(numbered.ends_with("-2"), "{numbered}");
    }

    #[test]
    fn a_number_frees_a_taken_name() {
        let taken = ["song", "song-2"];
        assert_eq!(numbered("song", |name| taken.contains(&name)), "song-3");
        assert_eq!(numbered("other", |name| taken.contains(&name)), "other");
    }
}
