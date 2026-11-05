//! The binding table: one list of mode, key sequence, command and description.
//!
//! The input handler resolves keys against this rather than matching on them, so the popup and
//! `Space ?` are generated from it and can't disagree with it. Counts and registers come before a
//! sequence and aren't in the table; an operator enters `OperatorPending`, which has its own rows.

use vt_model::selection::{Motion, Target};

use crate::input::keys::{Key, parse_sequence};
use crate::input::mode::Mode;

/// An operator key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Clear,
    Copy,
    ShiftUp,
    ShiftDown,
}

/// What a key sequence does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Move(Motion),
    ViewTop,
    ViewMiddle,
    ViewBottom,
    HalfPageDown,
    HalfPageUp,
    ScrollDown,
    ScrollUp,
    Operator(Op),
    /// The doubled operator key: `dd`, `yy`, `>>`, `<<`.
    WholeStep,
    On(Target),
    ClearField,
    Paste,
    Increase,
    Decrease,
    Repeat,
    Undo,
    Redo,
    VisualField,
    VisualStep,
    Reselect,
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
    Double,
    Help,
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub mode: Mode,
    pub keys: &'static str,
    pub cmd: Cmd,
    pub description: &'static str,
}

/// Motions work in Normal mode, after an operator, and in Visual modes.
const MOTIONS: &[(&str, Cmd, &str)] = &[
    ("h", Cmd::Move(Motion::Left), "Previous field"),
    ("l", Cmd::Move(Motion::Right), "Next field"),
    ("j", Cmd::Move(Motion::Down), "Next step"),
    ("k", Cmd::Move(Motion::Up), "Previous step"),
    ("w", Cmd::Move(Motion::NextGroup), "Next column group"),
    (
        "b",
        Cmd::Move(Motion::PreviousGroup),
        "Previous column group",
    ),
    ("}", Cmd::Move(Motion::NextBeat), "Next beat"),
    ("{", Cmd::Move(Motion::PreviousBeat), "Previous beat"),
    ("g g", Cmd::Move(Motion::First), "First step, or step n"),
    ("G", Cmd::Move(Motion::Last), "Last step, or step n"),
];

/// Targets work after an operator and in Visual modes.
const TARGETS: &[(&str, Cmd, &str)] = &[
    ("i b", Cmd::On(Target::Beat), "This beat, whole steps"),
    ("i c", Cmd::On(Target::Column), "This column, whole phrase"),
    ("i p", Cmd::On(Target::Phrase), "This phrase, whole steps"),
];

/// Viewport keys work wherever the cursor can move on screen.
const VIEWPORT: &[(&str, Cmd, &str)] = &[
    ("H", Cmd::ViewTop, "Cursor to the top of the view"),
    ("M", Cmd::ViewMiddle, "Cursor to the middle of the view"),
    ("L", Cmd::ViewBottom, "Cursor to the bottom of the view"),
    ("C-d", Cmd::HalfPageDown, "Half a screen down, centred"),
    ("C-u", Cmd::HalfPageUp, "Half a screen up, centred"),
    ("C-e", Cmd::ScrollDown, "Scroll the view down"),
    ("C-y", Cmd::ScrollUp, "Scroll the view up"),
];

/// Keys for Normal mode only.
const NORMAL: &[(&str, Cmd, &str)] = &[
    ("d", Cmd::Operator(Op::Clear), "Clear"),
    ("y", Cmd::Operator(Op::Copy), "Copy"),
    (">", Cmd::Operator(Op::ShiftUp), "Shift up"),
    ("<", Cmd::Operator(Op::ShiftDown), "Shift down"),
    ("x", Cmd::ClearField, "Clear this field"),
    ("p", Cmd::Paste, "Paste over from the cursor"),
    ("+", Cmd::Increase, "Increase the value"),
    ("-", Cmd::Decrease, "Decrease the value"),
    ("C-a", Cmd::Increase, "Increase the value"),
    ("C-x", Cmd::Decrease, "Decrease the value"),
    (".", Cmd::Repeat, "Repeat the last change"),
    ("u", Cmd::Undo, "Undo"),
    ("C-r", Cmd::Redo, "Redo"),
    ("v", Cmd::VisualField, "Select fields"),
    ("V", Cmd::VisualStep, "Select whole steps"),
    ("g v", Cmd::Reselect, "Reselect the last selection"),
    ("i", Cmd::EnterInsert, "Insert"),
    (":", Cmd::EnterCommand, "Command"),
    ("Enter", Cmd::PlayOrStop, "Play from the cursor, or stop"),
    ("Space x", Cmd::PauseOrResume, "Pause in place, or resume"),
    ("Space c", Cmd::ReturnToStart, "Return to the start"),
    ("Space q q", Cmd::Quit, "Quit"),
    ("Space m", Cmd::ToggleMute, "Mute or unmute this track"),
    ("Space s", Cmd::ToggleSolo, "Solo or unsolo this track"),
    ("Space M", Cmd::ClearMutes, "Clear all mutes and solos"),
    ("Space p d", Cmd::Double, "Double this phrase"),
    ("Space ?", Cmd::Help, "Show every key"),
    ("W", Cmd::NextTrack, "Next track"),
    ("B", Cmd::PreviousTrack, "Previous track"),
];

/// Keys that act on a selection.
const VISUAL: &[(&str, Cmd, &str)] = &[
    ("d", Cmd::Operator(Op::Clear), "Clear the selection"),
    ("y", Cmd::Operator(Op::Copy), "Copy the selection"),
    (">", Cmd::Operator(Op::ShiftUp), "Shift the selection up"),
    (
        "<",
        Cmd::Operator(Op::ShiftDown),
        "Shift the selection down",
    ),
    ("x", Cmd::Operator(Op::Clear), "Clear the selection"),
    ("+", Cmd::Increase, "Increase the selection's values"),
    ("-", Cmd::Decrease, "Decrease the selection's values"),
    ("C-a", Cmd::Increase, "Increase the selection's values"),
    ("C-x", Cmd::Decrease, "Decrease the selection's values"),
    (".", Cmd::Repeat, "Repeat the last change on the selection"),
];

/// A doubled operator key acts on the current step.
const WHOLE_STEP: &[(&str, Cmd, &str)] = &[
    ("d", Cmd::WholeStep, "This step"),
    ("y", Cmd::WholeStep, "This step"),
    (">", Cmd::WholeStep, "This step"),
    ("<", Cmd::WholeStep, "This step"),
];

/// Every default key, in the order help shows them.
pub fn bindings() -> Vec<Binding> {
    let mut rows = Vec::new();
    let mut add = |mode: Mode, group: &[(&'static str, Cmd, &'static str)]| {
        for (keys, cmd, description) in group {
            rows.push(Binding {
                mode,
                keys,
                cmd: *cmd,
                description,
            });
        }
    };
    add(Mode::Normal, MOTIONS);
    add(Mode::Normal, VIEWPORT);
    add(Mode::Normal, NORMAL);
    add(Mode::OperatorPending, MOTIONS);
    add(Mode::OperatorPending, TARGETS);
    add(Mode::OperatorPending, WHOLE_STEP);
    for mode in [Mode::VisualField, Mode::VisualStep] {
        add(mode, MOTIONS);
        add(mode, VIEWPORT);
        add(mode, TARGETS);
        add(mode, VISUAL);
    }
    rows
}

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
    rows: Vec<(Binding, Vec<Key>)>,
}

impl Table {
    /// Build a table from rows. The default table's names are checked by a test.
    pub fn new(bindings: Vec<Binding>) -> Self {
        Self {
            rows: bindings
                .into_iter()
                .map(|b| {
                    let keys = parse_sequence(b.keys).expect("binding table has a bad key name");
                    (b, keys)
                })
                .collect(),
        }
    }

    /// Resolve the keys typed so far.
    pub fn resolve(&self, mode: Mode, keys: &[Key]) -> Resolved {
        let mut prefix = false;
        for (binding, row_keys) in &self.rows {
            if binding.mode != mode {
                continue;
            }
            if row_keys == keys {
                return Resolved::Command(binding.cmd);
            }
            prefix |= row_keys.starts_with(keys);
        }
        if prefix {
            Resolved::Prefix
        } else {
            Resolved::Unbound
        }
    }

    /// What could come next after these keys, for the popup.
    pub fn continuations(&self, mode: Mode, keys: &[Key]) -> Vec<(String, &'static str)> {
        self.rows
            .iter()
            .filter(|(binding, row)| {
                binding.mode == mode && row.len() > keys.len() && row.starts_with(keys)
            })
            .map(|(binding, row)| {
                let rest: Vec<String> = row[keys.len()..].iter().map(Key::to_string).collect();
                (rest.join(" "), binding.description)
            })
            .collect()
    }

    /// Every row, for `Space ?`.
    pub fn rows(&self) -> impl Iterator<Item = &Binding> {
        self.rows.iter().map(|(binding, _)| binding)
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new(bindings())
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
        for binding in bindings() {
            let keys = parse_sequence(binding.keys).unwrap();
            assert!(
                seen.insert((binding.mode, keys)),
                "{} is bound twice in {:?}",
                binding.keys,
                binding.mode
            );
        }
    }

    #[test]
    fn no_key_is_both_an_action_and_a_prefix() {
        let table = Table::default();
        for binding in bindings() {
            let keys = parse_sequence(binding.keys).unwrap();
            for end in 1..keys.len() {
                assert_eq!(
                    table.resolve(binding.mode, &keys[..end]),
                    Resolved::Prefix,
                    "{} in {:?}",
                    binding.keys,
                    binding.mode
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
    fn the_popup_lists_what_can_come_next() {
        let table = Table::default();
        let space = parse_sequence("Space").unwrap();
        let next = table.continuations(Mode::Normal, &space);
        assert!(next.iter().any(|(k, d)| k == "x" && d.contains("Pause")));
        assert!(next.iter().any(|(k, _)| k == "q q"));
    }

    #[test]
    fn note_keys_run_chromatically() {
        let low: Vec<u8> = "zsxdcvgbhnjm".chars().filter_map(note_key).collect();
        let high: Vec<u8> = "q2w3er5t6y7u".chars().filter_map(note_key).collect();
        assert_eq!(low, (0..12).collect::<Vec<u8>>());
        assert_eq!(high, (12..24).collect::<Vec<u8>>());
    }
}
