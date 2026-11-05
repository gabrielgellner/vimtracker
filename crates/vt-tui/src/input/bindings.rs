//! The binding table: one list of mode, key sequence, command and description.
//!
//! The input handler resolves keys against this rather than matching on them, so help can be
//! generated from it and can't disagree with it.

use crate::input::keys::{Key, parse_sequence};
use crate::input::mode::Mode;

/// What a Normal mode key sequence does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    MoveLeft,
    MoveRight,
    MoveDown,
    MoveUp,
    EnterInsert,
    EnterCommand,
    PlayOrStop,
    PauseOrResume,
    ReturnToStart,
    Quit,
    NextTrack,
    PreviousTrack,
    ToggleMute,
    ToggleSolo,
    ClearMutes,
    Undo,
    Redo,
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub mode: Mode,
    pub keys: &'static str,
    pub cmd: Cmd,
    pub description: &'static str,
}

const fn normal(keys: &'static str, cmd: Cmd, description: &'static str) -> Binding {
    Binding {
        mode: Mode::Normal,
        keys,
        cmd,
        description,
    }
}

/// Every default key.
pub const BINDINGS: &[Binding] = &[
    normal("h", Cmd::MoveLeft, "Previous field"),
    normal("l", Cmd::MoveRight, "Next field"),
    normal("j", Cmd::MoveDown, "Next step"),
    normal("k", Cmd::MoveUp, "Previous step"),
    normal("i", Cmd::EnterInsert, "Insert"),
    normal(":", Cmd::EnterCommand, "Command"),
    normal("Enter", Cmd::PlayOrStop, "Play from the cursor, or stop"),
    normal("Space x", Cmd::PauseOrResume, "Pause in place, or resume"),
    normal("Space c", Cmd::ReturnToStart, "Return to the start"),
    normal("Space q q", Cmd::Quit, "Quit"),
    normal("W", Cmd::NextTrack, "Next track"),
    normal("B", Cmd::PreviousTrack, "Previous track"),
    normal("Space m", Cmd::ToggleMute, "Mute or unmute this track"),
    normal("Space s", Cmd::ToggleSolo, "Solo or unsolo this track"),
    normal("Space M", Cmd::ClearMutes, "Clear all mutes and solos"),
    normal("u", Cmd::Undo, "Undo"),
    normal("C-r", Cmd::Redo, "Redo"),
];

/// What a key sequence means so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Command(Cmd),
    /// More keys could complete it.
    Prefix,
    Unbound,
}

/// The table, with key names parsed.
#[derive(Debug, Clone)]
pub struct Table {
    rows: Vec<(Mode, Vec<Key>, Cmd)>,
}

impl Table {
    /// Build a table from rows. The default table's names are checked by a test.
    pub fn new(bindings: &[Binding]) -> Self {
        Self {
            rows: bindings
                .iter()
                .map(|b| {
                    let keys = parse_sequence(b.keys).expect("binding table has a bad key name");
                    (b.mode, keys, b.cmd)
                })
                .collect(),
        }
    }

    /// Resolve the keys typed so far.
    pub fn resolve(&self, mode: Mode, keys: &[Key]) -> Resolved {
        let mut prefix = false;
        for (row_mode, row_keys, cmd) in &self.rows {
            if *row_mode != mode {
                continue;
            }
            if row_keys == keys {
                return Resolved::Command(*cmd);
            }
            prefix |= row_keys.starts_with(keys);
        }
        if prefix {
            Resolved::Prefix
        } else {
            Resolved::Unbound
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new(BINDINGS)
    }
}

/// The notes Insert mode's keys play, as semitones above the current octave's C.
pub fn note_key(key: char) -> Option<u8> {
    Some(match key {
        'z' => 0,
        's' => 1,
        'x' => 2,
        'd' => 3,
        'c' => 4,
        'v' => 5,
        'g' => 6,
        'b' => 7,
        'h' => 8,
        'n' => 9,
        'j' => 10,
        'm' => 11,
        'q' => 12,
        '2' => 13,
        'w' => 14,
        '3' => 15,
        'e' => 16,
        'r' => 17,
        '5' => 18,
        't' => 19,
        '6' => 20,
        'y' => 21,
        '7' => 22,
        'u' => 23,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn no_key_sequence_is_bound_twice_in_a_mode() {
        let mut seen = HashSet::new();
        for binding in BINDINGS {
            let keys = parse_sequence(binding.keys).unwrap();
            assert!(
                seen.insert((binding.mode, keys)),
                "{} is bound twice",
                binding.keys
            );
        }
    }

    #[test]
    fn no_key_is_both_an_action_and_a_prefix() {
        let table = Table::default();
        for binding in BINDINGS {
            let keys = parse_sequence(binding.keys).unwrap();
            for end in 1..keys.len() {
                assert_eq!(
                    table.resolve(binding.mode, &keys[..end]),
                    Resolved::Prefix,
                    "{}",
                    binding.keys
                );
            }
        }
    }

    #[test]
    fn sequences_resolve() {
        let table = Table::default();
        let space = parse_sequence("Space").unwrap();
        assert_eq!(table.resolve(Mode::Normal, &space), Resolved::Prefix);
        let quit = parse_sequence("Space q q").unwrap();
        assert_eq!(
            table.resolve(Mode::Normal, &quit),
            Resolved::Command(Cmd::Quit)
        );
        let none = parse_sequence("Space z").unwrap();
        assert_eq!(table.resolve(Mode::Normal, &none), Resolved::Unbound);
    }

    #[test]
    fn note_keys_run_chromatically() {
        let low: Vec<u8> = "zsxdcvgbhnjm".chars().filter_map(note_key).collect();
        let high: Vec<u8> = "q2w3er5t6y7u".chars().filter_map(note_key).collect();
        assert_eq!(low, (0..12).collect::<Vec<u8>>());
        assert_eq!(high, (12..24).collect::<Vec<u8>>());
    }
}
