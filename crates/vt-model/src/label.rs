//! Track names, like `BD`.

/// Whether a name can be used: two letters or digits, the first a letter.
pub fn is_name(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    matches!(
        chars[..],
        [first, second] if first.is_ascii_alphabetic() && second.is_ascii_alphanumeric()
    )
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
}
