//! The binding table: one list of mode, key sequence, command and description.
//!
//! The input handler resolves keys against this rather than matching on them, so the popup and
//! `Space k` are generated from it and can't disagree with it. Counts and registers come before a
//! sequence and aren't in the table; an operator enters `OperatorPending`, which has its own rows.

use vt_model::selection::{Motion, Target};

use crate::input::keys::{Key, parse, parse_sequence};
use crate::input::mode::{Mode, View};
use crate::pages::Page as _;
use crate::pages::audio::AudioPage;
use crate::pages::chain::ChainView;
use crate::pages::mixer::MixerPage;
use crate::pages::phrase::PhraseView;
use crate::pages::project::ProjectPage;
use crate::pages::sample::{SampleKey, SamplePage};
use crate::pages::song::SongView;
use crate::pages::track::{Page, TrackPage};

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
    CycleMark,
    Paste,
    PasteAfter,
    PasteMerge,
    Increase,
    IncreaseLarge,
    Decrease,
    DecreaseLarge,
    Repeat,
    Undo,
    Redo,
    VisualField,
    VisualStep,
    Reselect,
    EnterInsert,
    EnterCommand,
    PauseOrResume,
    PlayFromStart,
    PlayFromCursor,
    Quit,
    NextTrack,
    PreviousTrack,
    /// Go to the next group, or the previous one, folded or open.
    NextGroupTrack,
    PreviousGroupTrack,
    ToggleMute,
    ToggleSolo,
    /// Switch the cursor's track's mute, or its solo, at the next line of the launch grid, or
    /// take back a switch waiting for it.
    MuteOnGrid,
    SoloOnGrid,
    ClearMutes,
    /// Turn fill on or off, for the conditions that play only with it or without it.
    ToggleFill,
    /// Send every track back to its start at the next line of the launch grid, or take back a
    /// realign waiting for it.
    Realign,
    /// Start the cursor's track's chain again at the next line of the launch grid, or take back
    /// one waiting for it.
    RealignTrack,
    /// Rotate the speeds round the tracks selected or the cursor's group, or with `true` round
    /// every track: each takes the next one's.
    RotateSpeeds(bool),
    /// Give the cursor's track's faders, or every track's, back to the steps' automation.
    GiveBack,
    GiveBackAll,
    /// Arm or disarm live recording.
    ToggleRecord,
    /// Record the master, a track or the input into a new sample, or stop.
    ToggleSampling,
    Double,
    /// Move the loop by this many steps, times the count.
    SlideLoop(isize),
    /// Rotate the steps within the loop, by this many steps for each of the count.
    RotateLoop(isize),
    /// Move the loop so that it starts on the cursor's step.
    StartLoopHere,
    /// End the loop on the cursor's step.
    EndLoopHere,
    Help,
    Guides,
    ShowView(View),
    /// Show the track page on one of its pages.
    ShowTrackPage(Page),
    /// Show the project page, on whichever of its sub-pages showed last.
    ShowProject,
    Picker,
    /// Choose a track to go to, like a buffer.
    TrackPicker,
    OpenItem,
    JumpBack,
    JumpForward,
    ListUses,
    ClonePhrase,
    CloneChain,
    NextSlot,
    PreviousSlot,
    LastSlot,
    FirstSlot,
    RemoveRow,
    CopyRow,
    /// Hear the row or note here.
    Audition,
    /// Switch an FX slot out or back in.
    Bypass,
    /// Zoom the FX page in on the cursor's slot, or back out to the rack.
    ZoomSlot,
    /// Zoom the FX page back out to the rack.
    ZoomRack,
    Replace,
    Substitute,
    /// Choose what's under the cursor from a list.
    PickValue,
    /// Fill the selected column with a ramp or random values.
    Shape,
    /// Open the shape panel again on the live generator under the cursor.
    ReopenGenerator,
    /// Roll the live generator under the cursor again, on the Gen page.
    RollGenerator,
    /// Apply the live generator under the cursor, leaving plain values; on the Mod page, keep what
    /// a Steps slot's `Mutate` drew as its values.
    ApplyGenerator,
    /// Write what the track's Steps and LFO slots on `Pitch` moved the notes to into them, and
    /// switch those slots out.
    BakeMods,
    /// On the track page, put the setting under the cursor on a macro, move it to another, or
    /// take it off.
    MapMacro,
    /// On the track page, open the mapping of the macro under the cursor, or of the one the
    /// setting under the cursor is on.
    MacroMapping,
    /// Add a section after this one.
    AddSection,
    /// Copy this section after itself.
    CloneSection,
    /// Play the chain under the cursor on its track, once its current one ends.
    Launch,
    /// Add a section holding what each track is playing, and carry on in it.
    Capture,
    /// Go to a part of the track page, or of the project page.
    PreviousPage,
    NextPage,
    /// Search this page for a setting to jump to.
    FieldJump,
    /// Halve or double how far the left and right arrows move in Insert.
    NarrowGrid,
    WidenGrid,
    /// Turn the scale on or off.
    ToggleScale,
    /// Collapse the phrase view to a line a row, or lay it out in time again.
    ToggleTimeline,
    /// Show the row inspector beside the phrase view's overview, or hide it.
    ToggleInspector,
    /// Fold the group the cursor is on or in, or open it.
    ToggleFold,
    /// Fold every group, or open every one.
    FoldAll(bool),
    /// Move a value, and a note by a semitone whatever the scale.
    IncreaseChromatic,
    DecreaseChromatic,
    /// In Insert on the Note column, accent the note just written, or the notes played while it's
    /// held.
    Accent,
    /// Likewise, slide from the note into the next row's, with `HLD` on its gate.
    Slide,
    /// Likewise, move the note an octave up, or down, as a 303's UP and DOWN keys do.
    OctaveUp,
    OctaveDown,
    /// A key of the sample page's own.
    Sample(SampleKey),
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub mode: Mode,
    pub keys: &'static str,
    pub cmd: Cmd,
    pub description: &'static str,
}

/// Keys that mean the same in every view.
const EVERY_VIEW: &[(&str, Cmd, &str)] = &[
    ("u", Cmd::Undo, "Undo"),
    ("C-r", Cmd::Redo, "Redo"),
    (":", Cmd::EnterCommand, "Command"),
    ("i", Cmd::EnterInsert, "Insert"),
    ("Tab", Cmd::PauseOrResume, "Play, or pause in place"),
    ("S-Tab", Cmd::PlayFromStart, "Play from the start"),
    ("Space c", Cmd::PlayFromCursor, "Play from the cursor"),
    ("Space q q", Cmd::Quit, "Quit"),
    (
        "Space m",
        Cmd::ToggleMute,
        "Mute or unmute; held, until let go",
    ),
    (
        "Space s",
        Cmd::ToggleSolo,
        "Solo or unsolo; held, until let go",
    ),
    (
        "Space g m",
        Cmd::MuteOnGrid,
        "Mute or unmute on the launch grid, or take it back",
    ),
    (
        "Space g s",
        Cmd::SoloOnGrid,
        "Solo or unsolo on the launch grid, or take it back",
    ),
    ("Space M", Cmd::ClearMutes, "Clear all mutes and solos"),
    (
        "Space R",
        Cmd::ToggleSampling,
        "Record the master, a track or the input into a new sample, or stop",
    ),
    (
        "Space l",
        Cmd::ToggleFill,
        "Fill on or off; held, until let go",
    ),
    (
        "Space A",
        Cmd::Realign,
        "Realign every track on the launch grid, or take it back",
    ),
    (
        "Space a",
        Cmd::RealignTrack,
        "Realign this track on the launch grid, or take it back",
    ),
    (
        "Space o",
        Cmd::RotateSpeeds(false),
        "Rotate the speeds round this group: each takes the next's",
    ),
    (
        "Space O",
        Cmd::RotateSpeeds(true),
        "Rotate the speeds round every track: each takes the next's",
    ),
    (
        "Space e",
        Cmd::GiveBack,
        "Give this track's faders back to the steps' automation",
    ),
    (
        "Space E",
        Cmd::GiveBackAll,
        "Give every track's faders back to the steps' automation",
    ),
    ("Space 1", Cmd::ShowView(View::Song), "Song view"),
    ("Space 2", Cmd::ShowView(View::Chain), "Chain view"),
    ("Space 3", Cmd::ShowView(View::Phrase), "Phrase view"),
    ("Space 4", Cmd::ShowView(View::Mixer), "Mixer"),
    (
        "Space 5",
        Cmd::ShowTrackPage(Page::Sound),
        "Sound page: machine, filter, output",
    ),
    (
        "Space 6",
        Cmd::ShowTrackPage(Page::Mod),
        "Mod page: envelopes, LFOs, mods",
    ),
    (
        "Space 7",
        Cmd::ShowTrackPage(Page::Fx),
        "FX page: the track's effects",
    ),
    (
        "Space 8",
        Cmd::ShowTrackPage(Page::Gen),
        "Gen page: the live generators",
    ),
    (
        "Space 9",
        Cmd::ShowProject,
        "Project: song settings, and audio",
    ),
    (
        "Space 0",
        Cmd::ShowView(View::Sample),
        "Sample page: the sample, big",
    ),
    ("Space Space", Cmd::Picker, "Open the view picker"),
    ("Space f f", Cmd::Picker, "Open the view picker"),
    ("Space f b", Cmd::TrackPicker, "Open the track picker"),
    ("Space ?", Cmd::Guides, "Open a guide"),
    ("Space k", Cmd::Help, "Show every key"),
    ("C-o", Cmd::JumpBack, "Jump back"),
    ("C-i", Cmd::JumpForward, "Jump forward"),
    ("W", Cmd::NextTrack, "Next track"),
    ("B", Cmd::PreviousTrack, "Previous track"),
    ("] b", Cmd::NextTrack, "Next track"),
    ("[ b", Cmd::PreviousTrack, "Previous track"),
    ("] g", Cmd::NextGroupTrack, "Next group, folded or open"),
    (
        "[ g",
        Cmd::PreviousGroupTrack,
        "Previous group, folded or open",
    ),
];

/// Cmd with the movement keys means the key that changes a value, like holding Edit on the M8,
/// so the hand that moves can nudge without leaving the home row: left and right a small step, up
/// and down a large one. This is the one place the rule is kept: every mode that binds `=`, `-`,
/// `+` or `_` gets its Cmd key beside it, meaning the same, unless the mode binds that Cmd key
/// itself, as the sample page's frame nudge does.
pub const CMD_ADJUSTS: [(&str, &str); 4] = [("=", "D-l"), ("-", "D-h"), ("+", "D-k"), ("_", "D-j")];

/// The key a Cmd key stands for by [`CMD_ADJUSTS`], for the keys read outside the table, as the
/// shape panel's are.
pub(crate) fn adjusting_key(key: Key) -> Option<Key> {
    CMD_ADJUSTS
        .iter()
        .find(|(_, with_cmd)| parse(with_cmd).is_ok_and(|named| named == key))
        .and_then(|(plain, _)| parse(plain).ok())
}

/// Keys to fold a group's tracks into it, after Vim's folds, in every view that shows tracks.
const FOLDS: &[(&str, Cmd, &str)] = &[
    (
        "z a",
        Cmd::ToggleFold,
        "Fold this group, or the one this track is in, or open it",
    ),
    ("z M", Cmd::FoldAll(true), "Fold every group"),
    (
        "z R",
        Cmd::FoldAll(false),
        "Open every group, the empty ones too",
    ),
];

/// Every default key, in the order help shows them. A page that's a component keeps its own rows
/// beside it, as its `Page::KEYS`, and they're put in here under its mode.
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
    add(Mode::Normal, PhraseView::MOTIONS);
    add(Mode::Normal, PhraseView::VIEWPORT);
    add(Mode::Normal, PhraseView::KEYS);
    add(Mode::Normal, FOLDS);
    add(Mode::Normal, EVERY_VIEW);
    add(Mode::Insert, PhraseView::INSERT_NOTE);
    add(Mode::Chain, ChainView::KEYS);
    add(Mode::Chain, FOLDS);
    add(Mode::Chain, EVERY_VIEW);
    add(Mode::Track, TrackPage::KEYS);
    add(Mode::Track, FOLDS);
    add(Mode::Track, EVERY_VIEW);
    add(Mode::Song, SongView::KEYS);
    add(Mode::Song, FOLDS);
    add(Mode::Song, EVERY_VIEW);
    add(Mode::Mixer, MixerPage::KEYS);
    add(Mode::Mixer, FOLDS);
    add(Mode::Mixer, EVERY_VIEW);
    add(Mode::Audio, AudioPage::KEYS);
    add(Mode::Audio, EVERY_VIEW);
    add(Mode::Project, ProjectPage::KEYS);
    add(Mode::Project, EVERY_VIEW);
    add(Mode::Sample, SamplePage::KEYS);
    add(Mode::Sample, EVERY_VIEW);
    // While the sample page selects, `i` starts a text object rather than the pads.
    let every_view_but_pads: Vec<_> = EVERY_VIEW
        .iter()
        .filter(|(keys, _, _)| *keys != "i")
        .copied()
        .collect();
    add(Mode::SampleVisual, SamplePage::KEYS);
    add(Mode::SampleVisual, SamplePage::OBJECTS);
    add(Mode::SampleVisual, &every_view_but_pads);
    add(Mode::OperatorPending, PhraseView::MOTIONS);
    add(Mode::OperatorPending, PhraseView::TARGETS);
    add(Mode::OperatorPending, PhraseView::WHOLE_STEP);
    for mode in [Mode::VisualField, Mode::VisualStep] {
        add(mode, PhraseView::MOTIONS);
        add(mode, PhraseView::VIEWPORT);
        add(mode, PhraseView::TARGETS);
        add(mode, PhraseView::VISUAL);
    }
    with_cmd_adjusts(rows)
}

/// Each row of `=`, `-`, `+` or `_` followed by its Cmd key's, by [`CMD_ADJUSTS`], in every mode
/// that doesn't bind that Cmd key itself.
fn with_cmd_adjusts(rows: Vec<Binding>) -> Vec<Binding> {
    let bound =
        |mode: Mode, keys: &str| rows.iter().any(|row| row.mode == mode && row.keys == keys);
    let mut all = Vec::with_capacity(rows.len() + 4 * 8);
    for row in &rows {
        all.push(*row);
        if let Some((_, with_cmd)) = CMD_ADJUSTS.iter().find(|(plain, _)| *plain == row.keys)
            && !bound(row.mode, with_cmd)
        {
            all.push(Binding {
                keys: with_cmd,
                ..*row
            });
        }
    }
    all
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
    pub(crate) fn continuations(&self, mode: Mode, keys: &[Key]) -> Vec<(String, &'static str)> {
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

    /// Every row, for `Space k`.
    pub fn rows(&self) -> impl Iterator<Item = &Binding> {
        self.rows.iter().map(|(binding, _)| binding)
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new(bindings())
    }
}

/// The notes Insert mode's keys play, as semitones above the current octave's C: a piano's
/// octave, the home row its white keys and the row above its black, as in Ableton.
pub(crate) fn note_key(key: char) -> Option<u8> {
    Some(match key {
        'a' => 0,
        'w' => 1,
        's' => 2,
        'e' => 3,
        'd' => 4,
        'f' => 5,
        't' => 6,
        'g' => 7,
        'y' => 8,
        'h' => 9,
        'u' => 10,
        'j' => 11,
        _ => return None,
    })
}

/// A key's place along a scale, with `a` on the root: the home row counts up through the first
/// seven notes and the row above through the next, as Push's In Key Sequential layout does.
pub(crate) fn degree_key(key: char) -> Option<usize> {
    "asdfghjqwertyu".chars().position(|degree| degree == key)
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
        assert!(next.iter().any(|(k, d)| k == "c" && d.contains("cursor")));
        assert!(next.iter().any(|(k, _)| k == "q q"));
    }

    /// Where the table is pinned, one row a line, as [`pinned_row`] writes it.
    const PINNED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/input/bindings.tsv");

    /// A row of the table as it's pinned: its mode, keys, command and description, by tabs.
    fn pinned_row(binding: &Binding) -> String {
        format!(
            "{:?}\t{}\t{:?}\t{}",
            binding.mode, binding.keys, binding.cmd, binding.description
        )
    }

    /// The table as it's assembled, from the pages' own keys and the rest, is the one pinned in
    /// `bindings.tsv`, row by row, so moving a page's rows loses or reorders none. A change to
    /// the keys on purpose pins the table again with `VT_PIN_BINDINGS=1` and shows in the diff.
    #[test]
    fn the_binding_table_is_as_pinned() {
        let rows: Vec<String> = bindings().iter().map(pinned_row).collect();
        if std::env::var_os("VT_PIN_BINDINGS").is_some() {
            std::fs::write(PINNED, rows.join("\n") + "\n").unwrap();
        }
        let pinned = std::fs::read_to_string(PINNED).unwrap();
        let pinned: Vec<&str> = pinned.lines().collect();
        for (at, (row, pin)) in rows.iter().zip(&pinned).enumerate() {
            assert_eq!(row, pin, "row {} differs from bindings.tsv", at + 1);
        }
        assert_eq!(rows.len(), pinned.len(), "the table has a different length");
    }

    #[test]
    fn note_keys_run_chromatically() {
        let notes: Vec<u8> = "awsedftgyhuj".chars().filter_map(note_key).collect();
        assert_eq!(notes, (0..12).collect::<Vec<u8>>());
        // The keyboard is one octave, C to B, and the keys past it do other things.
        assert_eq!("kolzxcvqrinmbp;',/".chars().filter_map(note_key).count(), 0);
    }

    #[test]
    fn scale_degrees_run_along_the_home_row_then_the_row_above() {
        let degrees: Vec<usize> = "asdfghjqwertyu".chars().filter_map(degree_key).collect();
        assert_eq!(degrees, (0..14).collect::<Vec<usize>>());
        assert_eq!("kol;',/zxcvi".chars().filter_map(degree_key).count(), 0);
    }
}
