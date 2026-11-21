//! Track names, like `BD`, and group names, like `DRUMS`.

/// The most characters a group's name has, as `DRUMS`: five, and the fold mark beside them, fit a
/// mixer strip and a song view's group column.
pub const GROUP_NAME_CHARS: usize = 5;

/// What a track's name can be, for whoever refuses one.
pub const NAME_RULE: &str = "two characters, a letter then a letter, a digit, - or _";

/// Whether a track can be called this: two characters, a letter then a letter, a digit, `-` or
/// `_`, so it's always one token of a kit line (decision 472). Every edit naming a track is held
/// to it.
pub fn is_name(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    matches!(
        chars[..],
        [first, second] if first.is_ascii_alphabetic()
            && (second.is_ascii_alphanumeric() || matches!(second, '-' | '_'))
    )
}

/// Whether two names are one to a kit line, which matches them without case, without being the
/// same name, as `BD` and `bd`: two tracks of one group can't have them (decision 472).
pub fn differ_by_case(name: &str, other: &str) -> bool {
    name != other && name.eq_ignore_ascii_case(other)
}

/// Whether a group can be called this: from one to [`GROUP_NAME_CHARS`] letters or digits, as
/// `DRUMS` or a kit's `909`.
pub fn is_group_name(name: &str) -> bool {
    (1..=GROUP_NAME_CHARS).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_two_characters_starting_with_a_letter() {
        for good in ["BD", "ch", "S1", "x9", "B-", "h_"] {
            assert!(is_name(good), "{good}");
        }
        for bad in [
            "", "B", "BDX", "1A", "-B", "_B", "B ", "B*", "B.", "B,", "B[", "B?", "é1",
        ] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn names_that_differ_only_by_case() {
        assert!(differ_by_case("BD", "bd"));
        assert!(differ_by_case("Bd", "bD"));
        assert!(!differ_by_case("BD", "BD"));
        assert!(!differ_by_case("BD", "SN"));
    }

    #[test]
    fn a_group_name_is_up_to_five_letters_or_digits() {
        for good in ["D", "BD", "909", "RYTM", "DRUMS", "perc2"] {
            assert!(is_group_name(good), "{good}");
        }
        for bad in ["", "DRUMSS", "KIT-8", "A B", "éé"] {
            assert!(!is_group_name(bad), "{bad}");
        }
    }
}
