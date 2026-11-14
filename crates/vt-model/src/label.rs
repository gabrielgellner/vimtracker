//! Track names, like `BD`, and group names, like `DRUMS`.

/// The most characters a group's name has, as `DRUMS`: five, and the fold mark beside them, fit a
/// mixer strip and a song view's group column (decision 308).
pub const GROUP_NAME_CHARS: usize = 5;

/// Whether a name can be used: two letters or digits, the first a letter.
pub fn is_name(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    matches!(
        chars[..],
        [first, second] if first.is_ascii_alphabetic() && second.is_ascii_alphanumeric()
    )
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
        for good in ["BD", "ch", "S1", "x9"] {
            assert!(is_name(good), "{good}");
        }
        for bad in ["", "B", "BDX", "1A", "B-", "é1"] {
            assert!(!is_name(bad), "{bad}");
        }
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
