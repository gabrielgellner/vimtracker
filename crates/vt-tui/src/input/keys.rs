//! Key names, adapted from gmuse's `src/cmd/keys.rs`.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventState, KeyModifiers, KeyboardEnhancementFlags,
};
use thiserror::Error;

/// What the program asks of the kitty keyboard protocol, from start to quitting: every key as an
/// escape code, with whether it's a press, a repeat or a release, and the shifted character, so
/// `>` reads as `>`.
pub(crate) const KEYBOARD_FLAGS: KeyboardEnhancementFlags =
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
    /// Cmd on a Mac, the super or Windows key elsewhere. Only `Cmd-h` `Cmd-j` `Cmd-k` `Cmd-l` are
    /// bound; any other key held with it is dropped rather than read as the key alone.
    pub command: bool,
    /// Only ever set on an arrow, so nothing else has to care about it. Elsewhere Shift is in the
    /// character, as `G` or `:`, and away from `Enter`, `Space` and the like, so a Shift held a
    /// moment early still types them.
    pub shift: bool,
}

impl Key {
    /// A plain key.
    pub fn new(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            command: false,
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
        let (ctrl, alt, command) = (
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
            event.modifiers.contains(KeyModifiers::SUPER),
        );
        let shifted = event.modifiers.contains(KeyModifiers::SHIFT);
        // Caps Lock types capitals, as it does without the protocol, but leaves `Ctrl-D` alone.
        let capitals = event.state.contains(KeyEventState::CAPS_LOCK) && !ctrl && !alt && !command;
        let code = match event.code {
            KeyCode::Char(c) if shifted => KeyCode::Char(shift_of(c).unwrap_or(c)),
            KeyCode::Char(c) if capitals => KeyCode::Char(c.to_ascii_uppercase()),
            code => code,
        };
        Self {
            code,
            ctrl,
            alt,
            command,
            shift: shifted
                && matches!(
                    code,
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
                ),
        }
    }

    /// A key the program never acts on: one that only changes others, as Shift and Caps Lock do,
    /// which the protocol reports as keys of their own, or a media key, left to the system.
    pub(crate) fn ignored(self) -> bool {
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
    pub(crate) fn physical(self) -> KeyCode {
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
            f.write_str("Option-")?;
        }
        if self.ctrl {
            f.write_str("Ctrl-")?;
        }
        if self.command {
            f.write_str("Cmd-")?;
        }
        if self.shift {
            f.write_str("Shift-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("Shift-Tab"),
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

/// A modifier a key name can carry.
#[derive(Clone, Copy)]
enum Modifier {
    Alt,
    Ctrl,
    Command,
    Shift,
}

/// Each modifier's prefix, as Vim writes it and as it's shown.
const MODIFIERS: [(&str, &str, Modifier); 4] = [
    ("M-", "Option-", Modifier::Alt),
    ("C-", "Ctrl-", Modifier::Ctrl),
    ("D-", "Cmd-", Modifier::Command),
    ("S-", "Shift-", Modifier::Shift),
];

/// A key sequence as it's shown, such as `Cmd-l` for `D-l`, or the text itself if it isn't one.
pub fn shown(text: &str) -> String {
    parse_sequence(text).map_or_else(
        |_| text.to_owned(),
        |keys| {
            keys.iter()
                .map(Key::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        },
    )
}

/// A key name that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a key name: {0:?}")]
pub struct KeyParseError(pub String);

/// Parse one key, such as `x`, `C-r`, `M-1`, `C-i`, `S-Up` or `Enter`. `D-` is Cmd, as in Vim.
/// They can also be written as they're shown: `Option-`, `Ctrl-`, `Cmd-` and `Shift-`.
/// The modifiers can come in any order, and `S-` is only for an arrow, as Shift is in the
/// character elsewhere: `G`, not `S-g`. `S-Tab` is the key `Shift-Tab` makes.
pub fn parse(text: &str) -> Result<Key, KeyParseError> {
    let mut key = Key::char(' ');
    let mut name = text;
    loop {
        if name == "S-Tab" || name == "Shift-Tab" {
            break;
        }
        let Some((which, prefix)) = MODIFIERS.iter().find_map(|(short, long, which)| {
            [*short, *long]
                .into_iter()
                .find(|prefix| name.starts_with(prefix))
                .map(|prefix| (*which, prefix.len()))
        }) else {
            break;
        };
        let flag = match which {
            Modifier::Alt => &mut key.alt,
            Modifier::Ctrl => &mut key.ctrl,
            Modifier::Command => &mut key.command,
            Modifier::Shift => &mut key.shift,
        };
        if *flag || name.len() == prefix {
            return Err(KeyParseError(text.to_owned()));
        }
        *flag = true;
        name = &name[prefix..];
    }
    let mut chars = name.chars();
    key.code = match (chars.next(), chars.next()) {
        (Some(c), None) => KeyCode::Char(c),
        _ => match name {
            "Space" => KeyCode::Char(' '),
            "Enter" => KeyCode::Enter,
            "Tab" => KeyCode::Tab,
            "S-Tab" | "Shift-Tab" => KeyCode::BackTab,
            "Backspace" => KeyCode::Backspace,
            "Esc" => KeyCode::Esc,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            _ => return Err(KeyParseError(text.to_owned())),
        },
    };
    let arrow = matches!(
        key.code,
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
    );
    if key.shift && !arrow {
        return Err(KeyParseError(text.to_owned()));
    }
    Ok(key)
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
    fn names_are_shown_in_full_and_read_back() {
        for (vim, shown) in [
            ("x", "x"),
            ("C-r", "Ctrl-r"),
            ("Enter", "Enter"),
            ("Space", "Space"),
            ("Esc", "Esc"),
            ("G", "G"),
            ("Tab", "Tab"),
            ("S-Tab", "Shift-Tab"),
            ("Backspace", "Backspace"),
            ("M-1", "Option-1"),
            ("C-i", "Ctrl-i"),
            ("C-j", "Ctrl-j"),
            ("C-m", "Ctrl-m"),
            ("C-1", "Ctrl-1"),
            ("C-0", "Ctrl-0"),
            ("C-A", "Ctrl-A"),
            ("D-1", "Cmd-1"),
            ("M-C-x", "Option-Ctrl-x"),
            ("S-Up", "Shift-Up"),
            ("C-S-Left", "Ctrl-Shift-Left"),
            ("M--", "Option--"),
        ] {
            let key = parse(vim).unwrap();
            assert_eq!(key.to_string(), shown);
            assert_eq!(parse(shown).unwrap(), key);
        }
    }

    #[test]
    fn a_sequence_is_shown_in_full() {
        assert_eq!(super::shown("D-l"), "Cmd-l");
        assert_eq!(super::shown("Space f b"), "Space f b");
        assert_eq!(super::shown("C-o M-s"), "Ctrl-o Option-s");
        assert_eq!(super::shown("its key"), "its key");
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
    fn keys_the_kitty_protocol_tells_apart_stay_apart() {
        let ctrl = |code| Key::from_event(KeyEvent::new(code, KeyModifiers::CONTROL));
        let plain = |code| Key::from_event(KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(ctrl(KeyCode::Char('i')), parse("C-i").unwrap());
        assert_ne!(ctrl(KeyCode::Char('i')), plain(KeyCode::Tab));
        assert_ne!(ctrl(KeyCode::Char('j')), plain(KeyCode::Enter));
        assert_ne!(ctrl(KeyCode::Char('m')), plain(KeyCode::Enter));
        for digit in '0'..='9' {
            let key = ctrl(KeyCode::Char(digit));
            assert_eq!(key, parse(&format!("C-{digit}")).unwrap());
            assert_ne!(key, Key::char(digit));
        }
    }

    #[test]
    fn cmd_is_carried_and_shown_as_cmd() {
        let event = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::SUPER);
        let key = Key::from_event(event);
        assert!(key.command);
        assert_ne!(key, Key::char('1'));
        assert_eq!(key.to_string(), "Cmd-1");
        assert_eq!(parse("D-1").unwrap(), key);
    }

    #[test]
    fn shift_is_dropped_from_enter_and_space() {
        for (code, name) in [(KeyCode::Enter, "Enter"), (KeyCode::Char(' '), "Space")] {
            let event = KeyEvent::new(code, KeyModifiers::SHIFT);
            assert_eq!(Key::from_event(event), parse(name).unwrap());
        }
    }

    #[test]
    fn unknown_names_are_errors() {
        assert!(parse("Nope").is_err());
        assert!(parse_sequence("  ").is_err());
        // Shift is in the character, so these can never arrive.
        assert!(parse("S-g").is_err());
        assert!(parse("S-Enter").is_err());
        assert!(parse("C-C-a").is_err());
        assert!(parse("C-").is_err());
        assert!(parse("Cmd-").is_err());
        assert!(parse("Ctrl-C-a").is_err());
    }
}
