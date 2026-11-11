//! The binding table: one list of mode, key sequence, command and description.
//!
//! The input handler resolves keys against this rather than matching on them, so the popup and
//! `Space k` are generated from it and can't disagree with it. Counts and registers come before a
//! sequence and aren't in the table; an operator enters `OperatorPending`, which has its own rows.

use vt_model::constants::BEAT_STEPS;
use vt_model::selection::{Motion, Target};

use crate::input::keys::{Key, parse, parse_sequence};
use crate::input::mode::{Mode, View};
use crate::placing::Edge;
use crate::sample_page::SampleKey;

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
    ToggleMute,
    ToggleSolo,
    ClearMutes,
    /// Turn fill on or off, for the conditions that play only with it or without it.
    ToggleFill,
    /// Send every track back to its start at the next line of the launch grid, or take back a
    /// realign waiting for it.
    Realign,
    /// Start the cursor's track's chain again at the next line of the launch grid, or take back
    /// one waiting for it.
    RealignTrack,
    /// Arm or disarm live recording.
    ToggleRecord,
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
    Replace,
    Substitute,
    /// Choose what's under the cursor from a list.
    PickValue,
    /// Fill the selected column with a ramp or random values.
    Shape,
    /// Choose this track's sample from a list.
    PickSample,
    /// Choose a kit from a list.
    PickKit,
    /// Add a section after this one.
    AddSection,
    /// Copy this section after itself.
    CloneSection,
    /// Play the chain under the cursor on its track, once its current one ends.
    Launch,
    /// Add a section holding what each track is playing, and carry on in it.
    Capture,
    /// Go to a part of the track page.
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
    /// Fold the group the cursor is on or in, or open it, in the song view and the mixer.
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

/// Motions work in Normal mode, after an operator, and in Visual modes.
const MOTIONS: &[(&str, Cmd, &str)] = &[
    ("h", Cmd::Move(Motion::Left), "Previous field"),
    ("l", Cmd::Move(Motion::Right), "Next field"),
    ("j", Cmd::Move(Motion::Down), "Next step"),
    ("k", Cmd::Move(Motion::Up), "Previous step"),
    (
        "w",
        Cmd::Move(Motion::NextGroup),
        "Next part: the note, then each FX",
    ),
    (
        "b",
        Cmd::Move(Motion::PreviousGroup),
        "Previous part: an FX, or the note",
    ),
    ("0", Cmd::Move(Motion::RowStart), "Start of the row"),
    ("$", Cmd::Move(Motion::RowEnd), "End of the row"),
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
    ("Space M", Cmd::ClearMutes, "Clear all mutes and solos"),
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
    ("Space 1", Cmd::ShowView(View::Song), "Song view"),
    ("Space 2", Cmd::ShowView(View::Chain), "Chain view"),
    ("Space 3", Cmd::ShowView(View::Phrase), "Phrase view"),
    ("Space 4", Cmd::ShowView(View::Track), "Track page"),
    ("Space 5", Cmd::PickSample, "Choose this track's sample"),
    ("Space 6", Cmd::ShowView(View::Mixer), "Mixer"),
    ("Space 8", Cmd::PickKit, "Choose a kit"),
    (
        "Space 0",
        Cmd::ShowView(View::Sample),
        "Sample page: this track's sample, big",
    ),
    (
        "Space 7",
        Cmd::ShowView(View::Audio),
        "Audio: the device, rate and buffer",
    ),
    (
        "Space 9",
        Cmd::ShowView(View::Project),
        "Project: tempo, grooves, scale, saving and rendering",
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
];

/// One beat, as a slide.
const BEAT: isize = BEAT_STEPS as isize;

/// Cmd with the movement keys means the key that changes a value, like holding Edit on the M8,
/// so the hand that moves can nudge without leaving the home row: left and right a small step, up
/// and down a large one. This is the one place the rule is kept: every mode that binds `=`, `-`,
/// `+` or `_` gets its Cmd key beside it, meaning the same, unless the mode binds that Cmd key
/// itself, as the sample page's frame nudge does (decision 147).
pub const CMD_ADJUSTS: [(&str, &str); 4] = [("=", "D-l"), ("-", "D-h"), ("+", "D-k"), ("_", "D-j")];

/// The key a Cmd key stands for by [`CMD_ADJUSTS`], for the keys read outside the table, as the
/// shape panel's are.
pub fn adjusting_key(key: Key) -> Option<Key> {
    CMD_ADJUSTS
        .iter()
        .find(|(_, with_cmd)| parse(with_cmd).is_ok_and(|named| named == key))
        .and_then(|(plain, _)| parse(plain).ok())
}

/// Keys for Normal mode only.
const NORMAL: &[(&str, Cmd, &str)] = &[
    ("d", Cmd::Operator(Op::Clear), "Clear"),
    ("y", Cmd::Operator(Op::Copy), "Copy"),
    (">", Cmd::Operator(Op::ShiftUp), "Shift up"),
    ("<", Cmd::Operator(Op::ShiftDown), "Shift down"),
    ("x", Cmd::ClearField, "Clear this field"),
    ("p", Cmd::Paste, "Paste over from the cursor"),
    ("g p", Cmd::PasteAfter, "Paste over, and land just after it"),
    (
        "g m",
        Cmd::PasteMerge,
        "Paste, keeping what the copy leaves empty",
    ),
    (";", Cmd::Audition, "Audition this row on every track"),
    ("r", Cmd::Replace, "Replace this field, and stay here"),
    ("s", Cmd::Substitute, "Clear this field and start Insert"),
    ("=", Cmd::Increase, "Increase the value"),
    ("-", Cmd::Decrease, "Decrease the value"),
    ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
    ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
    (
        "C-a",
        Cmd::IncreaseChromatic,
        "Increase the value, by a semitone in a scale",
    ),
    (
        "C-x",
        Cmd::DecreaseChromatic,
        "Decrease the value, by a semitone in a scale",
    ),
    (
        "~",
        Cmd::CycleMark,
        "Cycle a value's mark: lock, hold, glide",
    ),
    (".", Cmd::Repeat, "Repeat the last change"),
    ("v", Cmd::VisualField, "Select fields"),
    ("V", Cmd::VisualStep, "Select whole steps"),
    ("g v", Cmd::Reselect, "Reselect the last selection"),
    (
        "M-1",
        Cmd::NarrowGrid,
        "Narrower grid for the arrows in Insert",
    ),
    ("M-2", Cmd::WidenGrid, "Wider grid for the arrows in Insert"),
    ("M-s", Cmd::ToggleScale, "Scale on or off"),
    (
        "z a",
        Cmd::ToggleTimeline,
        "Collapse the timeline to a line a row, or open it again",
    ),
    ("Space p d", Cmd::Double, "Double this phrase"),
    (
        "Space r",
        Cmd::ToggleRecord,
        "Record notes played in Insert, while playing",
    ),
    ("K", Cmd::PickValue, "Choose this field from a list"),
    (
        "g d",
        Cmd::OpenItem,
        "On an SLI's value, open the sample page on its slice",
    ),
    ("g r r", Cmd::ListUses, "List where this phrase plays"),
    ("g c", Cmd::ClonePhrase, "Copy this phrase here"),
    ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
    ("] p", Cmd::NextSlot, "Next phrase in the chain"),
    ("[ p", Cmd::PreviousSlot, "Previous phrase in the chain"),
    ("] P", Cmd::LastSlot, "Last phrase in the chain"),
    ("[ P", Cmd::FirstSlot, "First phrase in the chain"),
    ("] l", Cmd::SlideLoop(1), "Slide the loop a step later"),
    ("[ l", Cmd::SlideLoop(-1), "Slide the loop a step earlier"),
    ("] L", Cmd::SlideLoop(BEAT), "Slide the loop a beat later"),
    (
        "[ L",
        Cmd::SlideLoop(-BEAT),
        "Slide the loop a beat earlier",
    ),
    ("] r", Cmd::RotateLoop(1), "Rotate the loop's steps later"),
    (
        "[ r",
        Cmd::RotateLoop(-1),
        "Rotate the loop's steps earlier",
    ),
    ("g s", Cmd::StartLoopHere, "Start the loop on this step"),
    ("g e", Cmd::EndLoopHere, "End the loop on this step"),
];

/// Keys for the chain view.
const CHAIN: &[(&str, Cmd, &str)] = &[
    ("j", Cmd::Move(Motion::Down), "Next row"),
    ("k", Cmd::Move(Motion::Up), "Previous row"),
    ("g g", Cmd::Move(Motion::First), "First row"),
    ("G", Cmd::Move(Motion::Last), "The empty row at the end"),
    ("h", Cmd::Move(Motion::Left), "Phrase column"),
    ("l", Cmd::Move(Motion::Right), "Transpose column"),
    ("0", Cmd::Move(Motion::RowStart), "Phrase column"),
    ("$", Cmd::Move(Motion::RowEnd), "Transpose column"),
    ("=", Cmd::Increase, "Next phrase, or transpose up"),
    ("-", Cmd::Decrease, "Previous phrase, or transpose down"),
    (
        "+",
        Cmd::IncreaseLarge,
        "Next phrase, or transpose up, a large step",
    ),
    (
        "_",
        Cmd::DecreaseLarge,
        "Previous phrase, or transpose down, a large step",
    ),
    ("C-a", Cmd::Increase, "Next phrase, or transpose up"),
    ("C-x", Cmd::Decrease, "Previous phrase, or transpose down"),
    ("x", Cmd::ClearField, "Transpose back to 0"),
    ("K", Cmd::PickValue, "Choose this row's phrase from a list"),
    ("d d", Cmd::RemoveRow, "Remove this row"),
    ("y y", Cmd::CopyRow, "Copy this row"),
    ("p", Cmd::Paste, "Add the copied row below"),
    (
        ";",
        Cmd::Audition,
        "Audition the start of this row, every track",
    ),
    ("g d", Cmd::OpenItem, "Open this phrase, or add one"),
    ("g r r", Cmd::ListUses, "List where this phrase plays"),
    ("g c", Cmd::ClonePhrase, "Copy this phrase here"),
    ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
];

const TRACK_PAGE: &[(&str, Cmd, &str)] = &[
    ("j", Cmd::Move(Motion::Down), "Next row"),
    ("k", Cmd::Move(Motion::Up), "Previous row"),
    ("g g", Cmd::Move(Motion::First), "First row"),
    ("G", Cmd::Move(Motion::Last), "Last row"),
    ("H", Cmd::PreviousPage, "Previous page: Sound, Mod, FX"),
    ("L", Cmd::NextPage, "Next page: Sound, Mod, FX"),
    ("d d", Cmd::RemoveRow, "Take an effect out of its slot"),
    ("p", Cmd::Paste, "Put the effect taken in this slot"),
    ("b", Cmd::Bypass, "Bypass this slot, or bring it back"),
    ("/", Cmd::FieldJump, "Jump to a setting by its name"),
    (
        "g d",
        Cmd::OpenItem,
        "On Sample, open the sample page on it",
    ),
    (
        "}",
        Cmd::Move(Motion::NextBeat),
        "Next slot, on the FX page",
    ),
    (
        "{",
        Cmd::Move(Motion::PreviousBeat),
        "Previous slot, on the FX page",
    ),
    ("h", Cmd::Move(Motion::Left), "Previous setting"),
    ("l", Cmd::Move(Motion::Right), "Next setting"),
    ("0", Cmd::Move(Motion::RowStart), "First setting"),
    ("$", Cmd::Move(Motion::RowEnd), "Last setting"),
    ("=", Cmd::Increase, "Increase the value"),
    ("-", Cmd::Decrease, "Decrease the value"),
    ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
    ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
    ("C-a", Cmd::Increase, "Increase the value"),
    ("C-x", Cmd::Decrease, "Decrease the value"),
    ("x", Cmd::ClearField, "Back to its default"),
    ("K", Cmd::PickValue, "Choose this setting from a list"),
    (
        ";",
        Cmd::Audition,
        "Play the sound on a w s e d f, until ; again",
    ),
];

const SONG: &[(&str, Cmd, &str)] = &[
    ("j", Cmd::Move(Motion::Down), "Next section"),
    ("k", Cmd::Move(Motion::Up), "Previous section"),
    ("g g", Cmd::Move(Motion::First), "First section"),
    ("G", Cmd::Move(Motion::Last), "Last section"),
    ("h", Cmd::Move(Motion::Left), "Previous column"),
    ("l", Cmd::Move(Motion::Right), "Next column"),
    ("0", Cmd::Move(Motion::RowStart), "Length column"),
    ("$", Cmd::Move(Motion::RowEnd), "Last track's column"),
    ("=", Cmd::Increase, "Increase the value"),
    ("-", Cmd::Decrease, "Decrease the value"),
    ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
    ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
    ("C-a", Cmd::Increase, "Increase the value"),
    ("C-x", Cmd::Decrease, "Decrease the value"),
    (
        "x",
        Cmd::ClearField,
        "Follow the longest chain, new, or no chain",
    ),
    (
        "K",
        Cmd::PickValue,
        "Switch new and reset, or choose this cell's chain",
    ),
    (
        "g d",
        Cmd::OpenItem,
        "Open this chain's phrase, its rows, or add one",
    ),
    ("g C", Cmd::CloneChain, "Copy this chain and its phrases"),
    ("a", Cmd::AddSection, "Add an empty section after this one"),
    ("g c", Cmd::CloneSection, "Copy this section after itself"),
    (
        "Enter",
        Cmd::Launch,
        "Play this chain on its track next, or stop on --",
    ),
    (
        "C",
        Cmd::Capture,
        "Add a section of what's playing, and stay on it",
    ),
    ("d d", Cmd::RemoveRow, "Remove this section"),
    (";", Cmd::Audition, "Hear the start of this section"),
];

/// Keys for the sample page. A sample is a buffer and a cell column a character, and the slice is
/// the line, but the cursor is a line placed to the pixel, so `h` and `l` move a pixel at the zoom
/// shown, speeding up as they're held.
const SAMPLE: &[(&str, Cmd, &str)] = &[
    ("h", Cmd::Move(Motion::Left), "A pixel earlier, faster held"),
    ("l", Cmd::Move(Motion::Right), "A pixel later, faster held"),
    ("0", Cmd::Move(Motion::RowStart), "Start of the slice"),
    ("$", Cmd::Move(Motion::RowEnd), "End of the slice"),
    (
        "^",
        Cmd::Sample(SampleKey::FirstSound),
        "First sound in the slice",
    ),
    (
        "g _",
        Cmd::Sample(SampleKey::LastSound),
        "Last sound in the slice",
    ),
    ("j", Cmd::Move(Motion::Down), "Next slice"),
    ("k", Cmd::Move(Motion::Up), "Previous slice"),
    (
        "g g",
        Cmd::Move(Motion::First),
        "Start of the sample, or slice n",
    ),
    (
        "G",
        Cmd::Move(Motion::Last),
        "End of the sample, or slice n",
    ),
    (
        "}",
        Cmd::Move(Motion::NextBeat),
        "Next beat, from the start, at the tempo",
    ),
    (
        "{",
        Cmd::Move(Motion::PreviousBeat),
        "Previous beat, from the start, at the tempo",
    ),
    (
        "] ]",
        Cmd::Sample(SampleKey::NextPoint),
        "Next point: a slice, the start, end or loop",
    ),
    (
        "[ [",
        Cmd::Sample(SampleKey::PreviousPoint),
        "Previous point: a slice, the start, end or loop",
    ),
    (
        "'",
        Cmd::Sample(SampleKey::Mark),
        "Then a letter: the slice it names, as a jump",
    ),
    (
        "] '",
        Cmd::Sample(SampleKey::NextMark),
        "Next slice a letter names, as a jump",
    ),
    (
        "[ '",
        Cmd::Sample(SampleKey::PreviousMark),
        "Previous slice a letter names, as a jump",
    ),
    ("H", Cmd::ViewTop, "Cursor to the left of the view"),
    ("M", Cmd::ViewMiddle, "Cursor to the middle of the view"),
    ("L", Cmd::ViewBottom, "Cursor to the right of the view"),
    ("C-d", Cmd::HalfPageDown, "Half a screen later"),
    ("C-u", Cmd::HalfPageUp, "Half a screen earlier"),
    ("C-e", Cmd::ScrollDown, "Scroll the view a column later"),
    ("C-y", Cmd::ScrollUp, "Scroll the view a column earlier"),
    (
        "z z",
        Cmd::Sample(SampleKey::CursorMiddle),
        "View with the cursor in its middle",
    ),
    (
        "z s",
        Cmd::Sample(SampleKey::CursorLeft),
        "View with the cursor at its left",
    ),
    (
        "z e",
        Cmd::Sample(SampleKey::CursorRight),
        "View with the cursor at its right",
    ),
    (
        "z i",
        Cmd::Sample(SampleKey::ZoomIn),
        "Zoom in by 2, on the cursor",
    ),
    (
        "z o",
        Cmd::Sample(SampleKey::ZoomOut),
        "Zoom out by 2, on the cursor",
    ),
    ("z O", Cmd::Sample(SampleKey::ZoomAll), "The whole sample"),
    (
        "z v",
        Cmd::Sample(SampleKey::ZoomSlice),
        "The slice, filling the view",
    ),
    ("M-1", Cmd::NarrowGrid, "Zoom in by 2, as z i"),
    ("M-2", Cmd::WidenGrid, "Zoom out by 2, as z o"),
    (
        "z c",
        Cmd::Sample(SampleKey::CursorStyle),
        "The cursor as a line, or as its column",
    ),
    (
        ";",
        Cmd::Audition,
        "Hear the slice, or the selection; held, until let go",
    ),
    (
        "Enter",
        Cmd::Sample(SampleKey::Split),
        "Split the slice here, or at the playhead as it plays",
    ),
    (
        "J",
        Cmd::Sample(SampleKey::Join),
        "Join the slice with the next",
    ),
    (
        "x",
        Cmd::ClearField,
        "Take the point here away, or a setting back",
    ),
    (
        "=",
        Cmd::Increase,
        "Nudge the point here a pixel later, or a setting up",
    ),
    (
        "-",
        Cmd::Decrease,
        "Nudge the point here a pixel earlier, or a setting down",
    ),
    (
        "+",
        Cmd::IncreaseLarge,
        "Nudge the point here a column later, or a setting up more",
    ),
    (
        "_",
        Cmd::DecreaseLarge,
        "Nudge the point here a column earlier, or a setting down more",
    ),
    (
        "D-l",
        Cmd::Sample(SampleKey::NudgeFrame { later: true }),
        "Nudge the point here a frame later",
    ),
    (
        "D-h",
        Cmd::Sample(SampleKey::NudgeFrame { later: false }),
        "Nudge the point here a frame earlier",
    ),
    (
        "g s",
        Cmd::Sample(SampleKey::SetPoint(Edge::Start)),
        "Start the sample here",
    ),
    (
        "g e",
        Cmd::Sample(SampleKey::SetPoint(Edge::End)),
        "End the sample here",
    ),
    (
        "g l",
        Cmd::Sample(SampleKey::SetPoint(Edge::LoopStart)),
        "Start the loop here",
    ),
    (
        "g L",
        Cmd::Sample(SampleKey::SetPoint(Edge::LoopEnd)),
        "End the loop here",
    ),
    (
        "m",
        Cmd::Sample(SampleKey::NameSlice),
        "Then a letter: name the slice here with it",
    ),
    ("v", Cmd::VisualField, "Select frames, or end the selection"),
    ("g v", Cmd::Reselect, "Select the last selection again"),
    (
        "o",
        Cmd::Sample(SampleKey::OtherEnd),
        "To the selection's other end",
    ),
    (
        ">",
        Cmd::Sample(SampleKey::GainUp),
        "The slice's gain up a dB",
    ),
    (
        "<",
        Cmd::Sample(SampleKey::GainDown),
        "The slice's gain down a dB",
    ),
    ("~", Cmd::Sample(SampleKey::Reverse), "Reverse the slice"),
    (
        "K",
        Cmd::PickValue,
        "Choose a setting to change, or list its values",
    ),
];

const AUDIO: &[(&str, Cmd, &str)] = &[
    ("j", Cmd::Move(Motion::Down), "Next setting"),
    ("k", Cmd::Move(Motion::Up), "Previous setting"),
    ("g g", Cmd::Move(Motion::First), "First setting"),
    ("G", Cmd::Move(Motion::Last), "Last setting"),
    ("=", Cmd::Increase, "The next choice"),
    ("-", Cmd::Decrease, "The previous choice"),
    // There's no large step through a list of choices, so `+` and `_` aren't bound, but Cmd-k
    // and Cmd-j stepped through it before the one rule for Cmd keys, and still do (decision 147).
    ("D-k", Cmd::IncreaseLarge, "The next choice, as D-l"),
    ("D-j", Cmd::DecreaseLarge, "The previous choice, as D-h"),
    ("x", Cmd::ClearField, "Back to the system's own choice"),
    ("K", Cmd::PickValue, "List the choices"),
];

const PROJECT: &[(&str, Cmd, &str)] = &[
    ("j", Cmd::Move(Motion::Down), "Next row"),
    ("k", Cmd::Move(Motion::Up), "Previous row"),
    ("g g", Cmd::Move(Motion::First), "First row, or row n"),
    ("G", Cmd::Move(Motion::Last), "Last row, or row n"),
    ("h", Cmd::Move(Motion::Left), "Previous setting"),
    (
        "l",
        Cmd::Move(Motion::Right),
        "Next setting, or the tempo's fine part",
    ),
    ("0", Cmd::Move(Motion::RowStart), "First setting"),
    ("$", Cmd::Move(Motion::RowEnd), "Last setting"),
    ("=", Cmd::Increase, "Increase the value, or switch on"),
    ("-", Cmd::Decrease, "Decrease the value, or switch off"),
    ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
    ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
    ("C-a", Cmd::Increase, "Increase the value"),
    ("C-x", Cmd::Decrease, "Decrease the value"),
    ("x", Cmd::ClearField, "Back to a new project's"),
    ("K", Cmd::PickValue, "Choose from a list, switch, or do it"),
    (
        "Enter",
        Cmd::PickValue,
        "Do this action, or open the audio page",
    ),
];

const MIXER: &[(&str, Cmd, &str)] = &[
    ("h", Cmd::Move(Motion::Left), "Previous track"),
    ("l", Cmd::Move(Motion::Right), "Next track"),
    ("0", Cmd::Move(Motion::RowStart), "First track"),
    ("$", Cmd::Move(Motion::RowEnd), "Last track"),
    ("j", Cmd::Move(Motion::Down), "Next setting"),
    ("k", Cmd::Move(Motion::Up), "Previous setting"),
    ("g g", Cmd::Move(Motion::First), "First setting"),
    ("G", Cmd::Move(Motion::Last), "Last setting"),
    ("=", Cmd::Increase, "Increase the value, or switch on"),
    ("-", Cmd::Decrease, "Decrease the value, or switch off"),
    ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
    ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
    ("C-a", Cmd::Increase, "Increase the value"),
    ("C-x", Cmd::Decrease, "Decrease the value"),
    ("x", Cmd::ClearField, "Back to its default"),
    ("K", Cmd::PickValue, "Switch mute or solo"),
    ("/", Cmd::FieldJump, "Jump to a setting by its name"),
];

/// Keys for the views that fold a group's tracks into it, the song view and the mixer, after Vim's
/// folds (decision 128).
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

/// Keys that act on a selection.
const VISUAL: &[(&str, Cmd, &str)] = &[
    ("W", Cmd::NextTrack, "Reach the next track"),
    ("B", Cmd::PreviousTrack, "Reach the previous track"),
    ("] b", Cmd::NextTrack, "Reach the next track"),
    ("[ b", Cmd::PreviousTrack, "Reach the previous track"),
    ("d", Cmd::Operator(Op::Clear), "Clear the selection"),
    ("y", Cmd::Operator(Op::Copy), "Copy the selection"),
    (">", Cmd::Operator(Op::ShiftUp), "Shift the selection up"),
    (
        "<",
        Cmd::Operator(Op::ShiftDown),
        "Shift the selection down",
    ),
    ("x", Cmd::Operator(Op::Clear), "Clear the selection"),
    (
        "r",
        Cmd::Replace,
        "Replace this column across the selection",
    ),
    (
        "S",
        Cmd::Shape,
        "Fill this column with a ramp or random values",
    ),
    ("=", Cmd::Increase, "Increase the selection's values"),
    ("-", Cmd::Decrease, "Decrease the selection's values"),
    (
        "+",
        Cmd::IncreaseLarge,
        "Increase the selection's values, a large step",
    ),
    (
        "_",
        Cmd::DecreaseLarge,
        "Decrease the selection's values, a large step",
    ),
    (
        "C-a",
        Cmd::IncreaseChromatic,
        "Increase the selection's values, by a semitone in a scale",
    ),
    (
        "C-x",
        Cmd::DecreaseChromatic,
        "Decrease the selection's values, by a semitone in a scale",
    ),
    ("~", Cmd::CycleMark, "Cycle the selected values' marks"),
    (".", Cmd::Repeat, "Repeat the last change on the selection"),
    (
        "u",
        Cmd::Undo,
        "Undo the last change, keeping the selection",
    ),
    ("C-r", Cmd::Redo, "Redo, keeping the selection"),
];

/// Keys for Insert on the Note column, beside the note keys, which are its own and aren't listed
/// here. As a 303 has its ACCENT and SLIDE keys past the top of its keyboard, these are past
/// ours: `'` is a stress mark, and `/` a slide in guitar tab.
const INSERT_NOTE: &[(&str, Cmd, &str)] = &[
    (
        "'",
        Cmd::Accent,
        "Note column: accent the note just written; held, the notes played",
    ),
    (
        "/",
        Cmd::Slide,
        "Note column: slide from the note just written; held, the notes played",
    ),
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
    add(Mode::Normal, EVERY_VIEW);
    add(Mode::Insert, INSERT_NOTE);
    add(Mode::Chain, CHAIN);
    add(Mode::Chain, EVERY_VIEW);
    add(Mode::Track, TRACK_PAGE);
    add(Mode::Track, EVERY_VIEW);
    add(Mode::Song, SONG);
    add(Mode::Song, FOLDS);
    add(Mode::Song, EVERY_VIEW);
    add(Mode::Mixer, MIXER);
    add(Mode::Mixer, FOLDS);
    add(Mode::Mixer, EVERY_VIEW);
    add(Mode::Audio, AUDIO);
    add(Mode::Audio, EVERY_VIEW);
    add(Mode::Project, PROJECT);
    add(Mode::Project, EVERY_VIEW);
    add(Mode::Sample, SAMPLE);
    add(Mode::Sample, EVERY_VIEW);
    add(Mode::OperatorPending, MOTIONS);
    add(Mode::OperatorPending, TARGETS);
    add(Mode::OperatorPending, WHOLE_STEP);
    for mode in [Mode::VisualField, Mode::VisualStep] {
        add(mode, MOTIONS);
        add(mode, VIEWPORT);
        add(mode, TARGETS);
        add(mode, VISUAL);
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

/// The notes Insert mode's keys play, as semitones above the current octave's C.
pub fn note_key(key: char) -> Option<u8> {
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
        'k' => 12,
        'o' => 13,
        'l' => 14,
        _ => return None,
    })
}

/// The home row's place along a scale, with `a` on the root.
pub fn degree_key(key: char) -> Option<usize> {
    "asdfghjkl".chars().position(|degree| degree == key)
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

    #[test]
    fn note_keys_run_chromatically() {
        let notes: Vec<u8> = "awsedftgyhujkol".chars().filter_map(note_key).collect();
        assert_eq!(notes, (0..15).collect::<Vec<u8>>());
        // As in Ableton, the keyboard stops at `l`, and `;` toggles auto-jump.
        assert_eq!("zxcvqrinmbp;'/".chars().filter_map(note_key).count(), 0);
    }
}
