//! Key names, adapted from gmuse's `src/cmd/keys.rs`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use thiserror::Error;

/// A bindable key: a code plus the modifiers that matter.
///
/// Deliberately not crossterm's `KeyEvent`, which also carries a kind and state that can differ
/// between events a user would call the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    /// Only ever set on an arrow, so nothing else has to care about it.
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

    /// Normalise a terminal event. Shift is folded away except on arrows: the terminal reports `J`
    /// with SHIFT set, and treating that as different from a bound `J` would break capitals.
    pub fn from_event(event: KeyEvent) -> Self {
        Self {
            code: event.code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
            alt: event.modifiers.contains(KeyModifiers::ALT),
            shift: event.modifiers.contains(KeyModifiers::SHIFT)
                && matches!(
                    event.code,
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
                ),
        }
    }
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

/// Parse one key, such as `x`, `C-r` or `Enter`.
pub fn parse(text: &str) -> Result<Key, KeyParseError> {
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
        alt: false,
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
        for name in ["x", "C-r", "Enter", "Space", "Esc", "G", "Tab", "Backspace"] {
            assert_eq!(parse(name).unwrap().to_string(), name);
        }
    }

    #[test]
    fn a_shifted_event_matches_its_capital() {
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(Key::from_event(event), parse("G").unwrap());
    }

    #[test]
    fn unknown_names_are_errors() {
        assert!(parse("Nope").is_err());
        assert!(parse_sequence("  ").is_err());
    }
}
