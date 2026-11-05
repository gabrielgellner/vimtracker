//! Instrument labels. A label is a two-character name that is itself a slot: `BD` is slot `0xBD`,
//! and a name that isn't hex, like `CH`, has a fixed slot of its own above the numbered ones. So
//! the same label is the same slot in every kit and song.

/// Where the slots for names that aren't hex begin.
const NAMED_BASE: u32 = 0x1_0000;
const RADIX: u32 = 36;

/// The slot two characters name, in either case: a hex number or a label. A name that isn't hex
/// has to start with a letter.
pub fn slot_of(name: &str) -> Option<u32> {
    let chars: Vec<char> = name.chars().collect();
    let [first, second] = chars[..] else {
        return None;
    };
    if let (Some(high), Some(low)) = (first.to_digit(16), second.to_digit(16)) {
        return Some(high * 16 + low);
    }
    if !first.is_ascii_alphabetic() {
        return None;
    }
    Some(NAMED_BASE + first.to_digit(RADIX)? * RADIX + second.to_digit(RADIX)?)
}

/// Whether a name can be a label. It starts with a letter, so `1A` always means the number.
pub fn is_label(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic()) && slot_of(name).is_some()
}

/// Whether a slot is one that only a label can name.
pub fn is_named_slot(slot: u32) -> bool {
    (NAMED_BASE..NAMED_BASE + RADIX * RADIX).contains(&slot)
}

/// How a slot is shown: its label if only a label can name it, or else its number.
pub fn name_of(slot: u32) -> String {
    if !is_named_slot(slot) {
        return format!("{slot:02X}");
    }
    let index = slot - NAMED_BASE;
    let digit = |value: u32| {
        char::from_digit(value, RADIX)
            .unwrap_or('?')
            .to_ascii_uppercase()
    };
    [digit(index / RADIX), digit(index % RADIX)]
        .iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_names_are_their_numbers() {
        assert_eq!(slot_of("BD"), Some(0xBD));
        assert_eq!(slot_of("bd"), Some(0xBD));
        assert_eq!(slot_of("1A"), Some(0x1A));
        assert_eq!(slot_of("00"), Some(0));
        assert_eq!(name_of(0xBD), "BD");
        assert_eq!(name_of(0x05), "05");
        assert_eq!(name_of(0x1A2), "1A2");
    }

    #[test]
    fn other_names_have_their_own_slots_above_the_numbers() {
        let ch = slot_of("CH").unwrap();
        assert!(is_named_slot(ch));
        assert!(ch > 0xFF);
        assert_eq!(slot_of("ch"), Some(ch));
        assert_ne!(slot_of("OH"), Some(ch));
        assert_eq!(name_of(ch), "CH");
        assert!(!is_named_slot(0xBD));
    }

    #[test]
    fn every_name_goes_there_and_back() {
        for first in 'A'..='Z' {
            for second in ('A'..='Z').chain('0'..='9') {
                let name: String = [first, second].iter().collect();
                assert_eq!(name_of(slot_of(&name).unwrap()), name);
            }
        }
    }

    #[test]
    fn labels_start_with_a_letter_and_have_two_characters() {
        assert!(is_label("BD"));
        assert!(is_label("K1"));
        assert!(!is_label("1A"));
        assert!(!is_label("1H"));
        assert!(!is_label("B"));
        assert!(!is_label("BDX"));
        assert!(!is_label("B-"));
        assert_eq!(slot_of("1H"), None);
        assert_eq!(slot_of("B-"), None);
    }
}
