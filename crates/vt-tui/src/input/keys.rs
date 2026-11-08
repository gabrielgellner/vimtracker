//! Key names, adapted from gmuse's `src/cmd/keys.rs`.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventState, KeyModifiers, KeyboardEnhancementFlags,
};
use thiserror::Error;

/// What the program asks of the kitty keyboard protocol, from start to quitting: every key as an
/// escape code, with whether it's a press, a repeat or a release, and the shifted character, so
/// `>` reads as `>`.
pub const KEYBOARD_FLAGS: KeyboardEnhancementFlags =
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        .union(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        .union(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS)
        .union(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES);

/// A bindable key: a code plus the modifiers that matter.
///
/// Deliberately not crossterm's `KeyEvent`, which also carries a kind and state that can differ
/// between events a user would call the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    /// Only ever set on an arrow, so nothing else has to care about it. Elsewhere Shift is in the
    /// character, as `G` or `:`.
    pub shift: bool,
}

/// The leader key.
pub const LEADER: Key = Key {
    code: KeyCode::Char(' '),
    ctrl: false,
    alt: false,
    shift: false,
};

impl Key {
    /// A plain key.
    pub fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    /// A character key.
    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c))
    }

    /// Normalise a terminal event. Shift is folded into the character, and away except on arrows:
    /// a bound `J` or `:` is the character, whatever the terminal says about Shift.
    ///
    /// The kitty protocol gives the shifted character where the terminal knows it, and crossterm
    /// then takes Shift off. Where it doesn't, as from kitty's remote control, Shift is still set
    /// on the plain key, `;` rather than `:`, and a US layout says what Shift makes of it.
    pub fn from_event(event: KeyEvent) -> Self {
        let (ctrl, alt) = (
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
        );
        let shifted = event.modifiers.contains(KeyModifiers::SHIFT);
        // Caps Lock types capitals, as it does without the protocol, but leaves `Ctrl-D` alone.
        let capitals = event.state.contains(KeyEventState::CAPS_LOCK) && !ctrl && !alt;
        let code = match event.code {
            KeyCode::Char(c) if shifted => KeyCode::Char(shift_of(c).unwrap_or(c)),
            KeyCode::Char(c) if capitals => KeyCode::Char(c.to_ascii_uppercase()),
            code => code,
        };
        Self {
            code,
            ctrl,
            alt,
            shift: shifted
                && matches!(
                    code,
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
                ),
        }
    }

    /// A key the program never acts on: one that only changes others, as Shift and Caps Lock do,
    /// which the protocol reports as keys of their own, or a media key, left to the system.
    pub fn ignored(self) -> bool {
        matches!(
            self.code,
            KeyCode::Modifier(_)
                | KeyCode::CapsLock
                | KeyCode::NumLock
                | KeyCode::ScrollLock
                | KeyCode::Media(_)
        )
    }

    /// The key on the keyboard, whatever was held with it: `:` is `;`, and `Shift-Tab` is `Tab`.
    /// A key can be let go after Shift, so its press and release can read differently.
    pub fn physical(self) -> KeyCode {
        match self.code {
            KeyCode::Char(c) => KeyCode::Char(unshift_of(c)),
            KeyCode::BackTab => KeyCode::Tab,
            code => code,
        }
    }
}

/// The characters Shift makes on a US layout, each after its plain one. Letters are left out.
const SHIFTED: [(char, char); 21] = [
    ('`', '~'),
    ('1', '!'),
    ('2', '@'),
    ('3', '#'),
    ('4', '$'),
    ('5', '%'),
    ('6', '^'),
    ('7', '&'),
    ('8', '*'),
    ('9', '('),
    ('0', ')'),
    ('-', '_'),
    ('=', '+'),
    ('[', '{'),
    (']', '}'),
    ('\\', '|'),
    (';', ':'),
    ('\'', '"'),
    (',', '<'),
    ('.', '>'),
    ('/', '?'),
];

/// What Shift makes of a plain character, or nothing if it's shifted already or has no shifted
/// form, as `:` and `G` and Space.
fn shift_of(c: char) -> Option<char> {
    if c.is_ascii_lowercase() {
        return Some(c.to_ascii_uppercase());
    }
    SHIFTED
        .iter()
        .find(|(plain, _)| *plain == c)
        .map(|(_, shifted)| *shifted)
}

/// The plain character a shifted one is typed with, or the character itself.
fn unshift_of(c: char) -> char {
    if c.is_ascii_uppercase() {
        return c.to_ascii_lowercase();
    }
    SHIFTED
        .iter()
        .find(|(_, shifted)| *shifted == c)
        .map_or(c, |(plain, _)| *plain)
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.alt {
            f.write_str("M-")?;
        }
        if self.ctrl {
            f.write_str("C-")?;
        }
        if self.shift {
            f.write_str("S-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("S-Tab"),
            KeyCode::Backspace => f.write_str("Backspace"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Up => f.write_str("Up"),
            KeyCode::Down => f.write_str("Down"),
            KeyCode::Left => f.write_str("Left"),
            KeyCode::Right => f.write_str("Right"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// A key name that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a key name: {0:?}")]
pub struct KeyParseError(pub String);

/// Parse one key, such as `x`, `C-r`, `M-1` or `Enter`.
pub fn parse(text: &str) -> Result<Key, KeyParseError> {
    let (alt, text) = match text.strip_prefix("M-") {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (ctrl, name) = match text.strip_prefix("C-") {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut chars = name.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) => KeyCode::Char(c),
        _ => match name {
            "Space" => KeyCode::Char(' '),
            "Enter" => KeyCode::Enter,
            "Tab" => KeyCode::Tab,
            "S-Tab" => KeyCode::BackTab,
            "Backspace" => KeyCode::Backspace,
            "Esc" => KeyCode::Esc,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            _ => return Err(KeyParseError(text.to_owned())),
        },
    };
    Ok(Key {
        code,
        ctrl,
        alt,
        shift: false,
    })
}

/// Parse a whitespace-separated key sequence.
pub fn parse_sequence(text: &str) -> Result<Vec<Key>, KeyParseError> {
    let keys: Vec<Key> = text
        .split_whitespace()
        .map(parse)
        .collect::<Result<_, _>>()?;
    if keys.is_empty() {
        return Err(KeyParseError(text.to_owned()));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for name in [
            "x",
            "C-r",
            "Enter",
            "Space",
            "Esc",
            "G",
            "Tab",
            "S-Tab",
            "Backspace",
            "M-1",
        ] {
            assert_eq!(parse(name).unwrap().to_string(), name);
        }
    }

    #[test]
    fn a_shifted_event_matches_its_capital() {
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(Key::from_event(event), parse("G").unwrap());
    }

    #[test]
    fn shift_on_a_plain_key_makes_its_shifted_character() {
        for (plain, shifted) in [
            (';', ":"),
            ('.', ">"),
            (',', "<"),
            ('4', "$"),
            ('g', "G"),
            ('[', "{"),
            (']', "}"),
            ('=', "+"),
            ('-', "_"),
            ('`', "~"),
            ('\'', "\""),
            ('/', "?"),
        ] {
            let event = KeyEvent::new(KeyCode::Char(plain), KeyModifiers::SHIFT);
            assert_eq!(Key::from_event(event), parse(shifted).unwrap());
        }
    }

    #[test]
    fn a_shifted_character_is_left_as_it_is() {
        for c in [':', '>', '$', 'K', ' '] {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
            assert_eq!(Key::from_event(event), Key::char(c));
        }
        let back_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(Key::from_event(back_tab), parse("S-Tab").unwrap());
        let up = Key::from_event(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
        assert!(up.shift);
    }

    #[test]
    fn a_key_with_or_without_shift_is_the_same_key_on_the_keyboard() {
        assert_eq!(Key::char(':').physical(), Key::char(';').physical());
        assert_eq!(Key::char('G').physical(), Key::char('g').physical());
        assert_eq!(
            Key::new(KeyCode::BackTab).physical(),
            Key::new(KeyCode::Tab).physical()
        );
        assert_ne!(Key::char('j').physical(), Key::char('k').physical());
    }

    #[test]
    fn unknown_names_are_errors() {
        assert!(parse("Nope").is_err());
        assert!(parse_sequence("  ").is_err());
    }
}
