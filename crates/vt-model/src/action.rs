//! Recorded edits. `edit` is the only path that changes a song.

use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;

use crate::accumulator::{Accumulation, MOST_DEGREES, MOST_LOOPS};
use crate::arpeggio::{Arpeggio, MOST_OCTAVES};
use crate::bank::BankEntry;
use crate::bloom::{Bloom, MAX_BRANCHES};
use crate::constants::{
    FX_SLOTS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_PROJECT_NAME_CHARS, MAX_SOUND_TRACKS,
    MAX_TRACK_RESET, MAX_TRANSPOSE, MIN_TRANSPOSE,
};
use crate::generator::Span;
use crate::groove::{CUSTOM_GROOVES, Groove, GrooveId};
use crate::insert::Insert;
use crate::kit::CHOKE_GROUPS;
use crate::label::GROUP_NAME_CHARS;
use crate::lane::{LANE_RATES, LaneMove};
use crate::macros::{MACRO_NAME_LENGTH, MACROS, MacroMap, MacroTarget};
use crate::modulation::ModSlot;
use crate::order::Order;
use crate::preset::{Patch, Placement, SoundPreset};
use crate::scale::Scale;
use crate::selection::{Block, Scope, Shift, StepRange};
use crate::settings::SettingId;
use crate::shape::{Shape, ShapeColumn, Source};
use crate::song::{
    ChainEntry, FilterMode, FilterType, GlideMode, Lane, Launch, Machine, Phase, Phrase, Repeat,
    Role, Section, SectionStart, Song, Track, VoiceMode,
};
use crate::speed::Speed;
use crate::step::{FxSlot, Gate, Mark, Note, Step};

mod bank;
mod chains;
mod generators;
mod phrases;
mod sections;
mod song_wide;
mod sound;
mod steps;
mod tracks;

use generators::follow_generators;
pub use generators::{hold_generators, rescale_generators};

/// A new value for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue {
    Velocity(Option<u8>),
    /// A row's nudge, in 24ths of a sixteenth, kept within its track's range.
    Nudge(i8),
    Gate(Option<Gate>),
    Slot(usize, FxSlot),
}

/// A value written across a selection. A velocity means nothing without a note, and
/// an FX value nothing without a command, so those only go on steps that have one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// A note, with a velocity for steps that don't have one.
    Note {
        note: Note,
        velocity: u8,
    },
    Velocity(Option<u8>),
    /// A gate cell, which goes on every row, as a row without a note still takes its time.
    Gate(Option<Gate>),
    /// A gate cell's length, which only goes on rows with a note, as it's how long the note lasts.
    /// A row without a gate cell gets one pulse of `SGL` to hold it.
    GateLength(Option<u8>),
    /// A nudge, which only goes on rows with a note, as velocity does. 0 takes it away anywhere.
    Nudge(i8),
    Slot(usize, FxSlot),
    /// An FX value's coarse digits, keeping its fine part.
    SlotValue(usize, u8),
    /// An FX value's fine digits, where it has a fine part.
    SlotFine(usize, u8),
}

/// A sound from a kit going on a track: the bank entry it plays, and the name and choke group its
/// track takes. Its gain and tune are the entry's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitSound {
    pub sample: u8,
    pub name: String,
    pub choke: Option<u8>,
}

/// One edit, as plain data so it can be replayed and tested without a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write a note.
    SetNote {
        track: usize,
        phrase: u32,
        step: usize,
        note: Note,
    },
    /// Replace a whole step.
    SetStep {
        track: usize,
        phrase: u32,
        step: usize,
        value: Step,
    },
    /// Several edits as one change, or none if any is refused.
    Batch(Vec<Action>),
    /// Set the mark on every value in a selection.
    SetMark {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
        mark: Mark,
    },
    /// Set one field of a step.
    SetField {
        track: usize,
        phrase: u32,
        step: usize,
        value: FieldValue,
    },
    /// Fill a column across steps with a ramp or random values.
    Shape {
        track: usize,
        phrase: u32,
        range: StepRange,
        column: ShapeColumn,
        shape: Shape,
    },
    /// Keep a shape live on a column of a phrase, on rows or on the column's lane, starting from
    /// the values there.
    Generate {
        track: usize,
        phrase: u32,
        column: ShapeColumn,
        span: Span,
        shape: Shape,
    },
    /// Give one of a phrase's live generators, by its place in the phrase's list, a new shape.
    Reshape {
        track: usize,
        phrase: u32,
        generator: usize,
        shape: Shape,
    },
    /// Apply one of a phrase's live generators, leaving its values as plain ones.
    ApplyGenerator {
        track: usize,
        phrase: u32,
        generator: usize,
    },
    /// Take one of a phrase's live generators away, putting back what was under it.
    RemoveGenerator {
        track: usize,
        phrase: u32,
        generator: usize,
    },
    /// Write one value across steps.
    Fill {
        track: usize,
        phrase: u32,
        range: StepRange,
        fill: Fill,
    },
    /// Clear fields across steps.
    Clear {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
    },
    /// Move values across steps.
    Shift {
        track: usize,
        phrase: u32,
        range: StepRange,
        scope: Scope,
        by: i32,
        kind: Shift,
        /// The scale notes move through, if it's on.
        scale: Option<Scale>,
    },
    /// Move the notes in a phrase to the nearest in a scale.
    FitToScale {
        track: usize,
        phrase: u32,
        scale: Scale,
    },
    /// Write a copied block some number of times end to end. Past the last stored step this stores
    /// more without changing the loop. Merging leaves steps alone where the copy is empty, and
    /// otherwise the copy overwrites them.
    Paste {
        track: usize,
        phrase: u32,
        at: usize,
        group: usize,
        block: Block,
        times: usize,
        merge: bool,
    },
    /// Double a phrase's loop by repeating it.
    Double {
        track: usize,
        phrase: u32,
    },
    SetSkipSize(u32),
    SetLimiter(bool),
    /// Name the project.
    SetSongName(String),
    /// Set the project's random seed.
    SetSeed(u64),
    /// When launched chains start.
    SetLaunch(Launch),
    /// What a chain is called on screen.
    SetChainLabel {
        track: usize,
        chain: u32,
        label: String,
    },
    /// Set the groove every track plays with unless it has its own.
    SetGroove(GrooveId),
    /// Give a track its own groove, or `None` to follow the project.
    SetTrackGroove {
        track: usize,
        groove: Option<GrooveId>,
    },
    /// Make or change one of the project's own grooves, counted from 0.
    SetCustomGroove {
        custom: usize,
        groove: Groove,
    },
    /// Set where one of a track's held values starts.
    SetTrackBase {
        track: usize,
        control: TrackControl,
        value: u8,
    },
    /// Replace one of a track's mod slots: its type, where it goes, its switch, or any type's own
    /// settings.
    SetMod {
        track: usize,
        slot: usize,
        modulator: ModSlot,
    },
    /// Turn one of a track's macros, from `0000` to `FF00`.
    SetMacro {
        track: usize,
        at: usize,
        value: u16,
    },
    /// Put a setting on one of a track's macros, or change its mapping there. A setting is on one
    /// macro at most, so it comes off any other.
    MapMacro {
        track: usize,
        at: usize,
        map: MacroMap,
    },
    /// Take a setting off whichever of a track's macros holds it: on the track itself, or with
    /// `member` on the track at that place in a group track's group.
    UnmapMacro {
        track: usize,
        target: MacroTarget,
        member: Option<u8>,
    },
    /// Name one of a track's macros, or give it back its first mapping's name with `None`.
    NameMacro {
        track: usize,
        at: usize,
        name: Option<String>,
    },
    /// Put an effect, or nothing, in one of a track's slots.
    SetInsert {
        track: usize,
        slot: usize,
        insert: Insert,
    },
    /// Make a sound track's sound and patch a sound preset's, its sampler playing this bank
    /// entry, keeping its place in the song and its phrases.
    LoadSound {
        track: usize,
        preset: Box<SoundPreset>,
        sample: u8,
    },
    /// Add a patch preset's mod slots, macros, accumulator and arpeggio to a sound track where
    /// `placement` says, keeping its sound. Its generators go on with `Generate`.
    LoadPatch {
        track: usize,
        patch: Box<Patch>,
        placement: Placement,
    },
    /// Put an effect preset's effect in one of a track's slots, with the fine parts of its
    /// settings there, in place of the slot's own.
    LoadEffect {
        track: usize,
        slot: usize,
        insert: Insert,
        fine: BTreeMap<SettingId, u8>,
    },
    /// Switch one of a track's slots out, or back in.
    SetBypass {
        track: usize,
        slot: usize,
        bypassed: bool,
    },
    /// Send every track back to its start after this many steps, or `None` to let them run.
    SetReset {
        section: usize,
        reset: Option<usize>,
    },
    /// Add a section after a position, with no chains.
    AddSection {
        at: usize,
    },
    /// Copy a section after itself, using the same chains.
    CloneSection {
        at: usize,
    },
    /// Remove a section, keeping at least one. Its chains stay for reuse.
    RemoveSection {
        at: usize,
    },
    /// Set how long a section plays, or follow its longest chain.
    SetSectionLength {
        section: usize,
        length: Option<usize>,
    },
    /// Set how many times a section plays before moving on.
    SetSectionRepeat {
        section: usize,
        repeat: Repeat,
    },
    /// Set what a section does to the tracks when it starts.
    SetSectionStart {
        section: usize,
        start: SectionStart,
    },
    /// Give a section a scale of its own, or none to play in the song's.
    SetSectionScale {
        section: usize,
        scale: Option<Scale>,
    },
    /// Choose a section's chain for a track, or none.
    SetSectionChain {
        section: usize,
        track: usize,
        chain: Option<u32>,
    },
    /// Set a track's level.
    SetTrackLevel {
        track: usize,
        level: u8,
    },
    /// Set a track's trim, the gain at its input.
    SetTrackTrim {
        track: usize,
        trim: u8,
    },
    /// Say what a track is for, which its colour shows, or `Auto` to go by its machine.
    SetTrackRole {
        track: usize,
        role: Role,
    },
    /// Put a track in a choke group, from 1 to `CHOKE_GROUPS`, or in none.
    SetTrackChoke {
        track: usize,
        choke: Option<u8>,
    },
    /// Put the sound tracks from `first` to `last` in a group, from 1 to `MAX_GROUPS`, or take
    /// them out of any, as one change.
    SetTrackGroup {
        first: usize,
        last: usize,
        group: Option<u8>,
    },
    /// Give a track its own `Gate`, on the `00` to `FF` scale, or `None` for its machine's.
    SetTrackGateLength {
        track: usize,
        gate_length: Option<u8>,
    },
    /// Set how long each of a track's steps is.
    SetTrackSpeed {
        track: usize,
        speed: Speed,
    },
    /// Set whether a synth track's oscillators run on from note to note or start again on each.
    SetTrackPhase {
        track: usize,
        phase: Phase,
    },
    /// Set how a track's notes join: each struck, or tied into one whose gate is still open.
    SetTrackVoiceMode {
        track: usize,
        mode: VoiceMode,
    },
    /// Set how many of the song's steps a track's chain plays before starting again, from 1 to
    /// `MAX_TRACK_RESET`, or `None` to play on.
    SetTrackReset {
        track: usize,
        reset: Option<u16>,
    },
    /// Set how long a track's ties glide into the next note, on the `00` to `FF` scale of
    /// [`glide_ms`](crate::settings::glide_ms), where `00` is off.
    SetTrackGlide {
        track: usize,
        glide: u8,
    },
    /// Set which notes a track's `Glide` slides into: only ties, or every note from the one
    /// sounding.
    SetTrackGlideMode {
        track: usize,
        mode: GlideMode,
    },
    /// Switch a track filter's `Safe` on or off, which keeps `Res` short of ringing by itself.
    SetTrackSafe {
        track: usize,
        on: bool,
    },
    /// Switch a track filter's `Key track` on or off, which has its cutoff follow the note.
    SetTrackKeyTrack {
        track: usize,
        on: bool,
    },
    /// Set how a track's `ACU`s move its notes.
    SetAccumulation {
        track: usize,
        accumulation: Accumulation,
    },
    /// Set how a track's `ARP`s play.
    SetArpeggio {
        track: usize,
        arpeggio: Arpeggio,
    },
    /// Set what a track's `Length` and `Rate` for an FX column move every phrase's lane on that
    /// column by.
    SetLaneMove {
        track: usize,
        slot: usize,
        moved: LaneMove,
    },
    /// Change the length of a phrase's loop, keeping every step.
    SetLength {
        track: usize,
        phrase: u32,
        length: usize,
    },
    /// Play a phrase's rows in another order.
    SetOrder {
        track: usize,
        phrase: u32,
        order: Order,
    },
    /// Give an FX column of a phrase a lane of its own, or none to follow the phrase's loop.
    SetLane {
        track: usize,
        phrase: u32,
        slot: usize,
        lane: Option<Lane>,
    },
    /// Move a phrase's loop by some steps, keeping its length.
    SlideLoop {
        track: usize,
        phrase: u32,
        by: isize,
    },
    /// Move a phrase's loop steps later or earlier, wrapping round within the loop.
    RotateLoop {
        track: usize,
        phrase: u32,
        by: isize,
    },
    /// Change the tempo, in thousandths of a BPM.
    SetMilliBpm(u32),
    /// Add tracks at the end, each with a new chain and phrase in the first section.
    AddTracks(usize),
    /// Remove the last track and its chains and phrases, keeping at least one.
    RemoveLastTrack,
    /// Choose the bank entry a track's sampler plays, or `00` for none.
    SetTrackSample {
        track: usize,
        sample: u8,
    },
    /// Put an entry in the bank at a number, replacing any there, or empty the number.
    SetBankEntry {
        number: u8,
        entry: Option<BankEntry>,
    },
    /// Cut a bank entry into slices starting at these frames of its file, or into none, as
    /// [`Points::reslice`](crate::bank::Points::reslice) does, and have the steps that chose a
    /// slice of it by number choose the one holding the sound they played, as one change.
    SetSlices {
        number: u8,
        starts: Vec<u64>,
    },
    /// Split the slice of a bank entry holding a frame of its file in two there, as
    /// [`Points::split_starts`](crate::bank::Points::split_starts) does. The part before keeps the
    /// slice's id and marks, and the steps' `SLI` locks follow their sound as a re-slice's do.
    SplitSlice {
        number: u8,
        at: u64,
    },
    /// Join a bank entry's slice, by its number, to the next, which it takes in with its marks, as
    /// [`Points::join_starts`](crate::bank::Points::join_starts) does, the steps' `SLI` locks
    /// following their sound.
    JoinSlices {
        number: u8,
        slice: u8,
    },
    /// Name a track, or leave it with no name.
    SetTrackName {
        track: usize,
        name: String,
    },
    /// Put a kit's sounds on tracks from the first, as one change, adding tracks if it needs more.
    LoadKit(Vec<KitSound>),
    /// Choose what makes a track's sound.
    SetTrackMachine {
        track: usize,
        machine: Machine,
    },
    /// Set one of a track's machine's settings.
    SetTrackSetting {
        track: usize,
        setting: SettingId,
        value: u8,
    },
    /// Set the fine part of one of a track's own settings or its machine's.
    SetTrackFine {
        track: usize,
        setting: SettingId,
        fine: u8,
    },
    /// Choose a track's filter.
    SetFilterType {
        track: usize,
        filter: FilterType,
    },
    /// Choose how a track's filter responds.
    SetFilterMode {
        track: usize,
        mode: FilterMode,
    },
    /// Change a chain row. A phrase that doesn't exist yet is created.
    SetChainEntry {
        track: usize,
        chain: u32,
        slot: usize,
        entry: ChainEntry,
    },
    /// Add a chain row. A phrase that doesn't exist yet is created.
    InsertChainEntry {
        track: usize,
        chain: u32,
        at: usize,
        entry: ChainEntry,
    },
    /// Remove a chain row, keeping at least one. Its phrase stays for reuse.
    RemoveChainEntry {
        track: usize,
        chain: u32,
        at: usize,
    },
    /// Copy the phrase in a chain row to a new ID, and point the row at the copy.
    ClonePhrase {
        track: usize,
        chain: u32,
        slot: usize,
        new: u32,
    },
    /// Copy a track's chain and its phrases to new IDs, and point the track at the copy.
    CloneChain {
        section: usize,
        track: usize,
    },
    /// Grow branches from the phrase in a chain row, after Qu-Bit's Bloom: each a new phrase, a
    /// copy of it whose Note column is a live `Follow` of the branch before, mutating as it
    /// copies; and put the path through the trunk and its branches in the row's place, each with
    /// its transpose.
    Bloom {
        track: usize,
        chain: u32,
        slot: usize,
        /// The scale the branches' notes move in.
        scale: Scale,
        bloom: Bloom,
    },
    /// Point a track at a chain. A chain that doesn't exist yet gets a new phrase.
    SetTrackChain {
        section: usize,
        track: usize,
        chain: u32,
    },
}

/// One of the values a track holds, set from its page rather than a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackControl {
    Cutoff,
    Resonance,
    Pan,
    SendA,
    SendB,
    Drive,
    Punch,
    LowCut,
    HighCut,
    Pitch,
    VelocityDelta,
    Quantise,
    Jump,
    Order,
    Reverse,
    OrderX,
    OrderY,
    PhraseLength,
    PhraseStart,
    ChanceDelta,
    ChanceTarget,
    Cue,
    DiceLoop,
    DiceMutate,
    DiceJump,
    Swing,
}

impl TrackControl {
    /// Every one, in the order they're declared.
    pub const ALL: [Self; 26] = [
        Self::Cutoff,
        Self::Resonance,
        Self::Pan,
        Self::SendA,
        Self::SendB,
        Self::Drive,
        Self::Punch,
        Self::LowCut,
        Self::HighCut,
        Self::Pitch,
        Self::VelocityDelta,
        Self::Quantise,
        Self::Jump,
        Self::Order,
        Self::Reverse,
        Self::OrderX,
        Self::OrderY,
        Self::PhraseLength,
        Self::PhraseStart,
        Self::ChanceDelta,
        Self::ChanceTarget,
        Self::Cue,
        Self::DiceLoop,
        Self::DiceMutate,
        Self::DiceJump,
        Self::Swing,
    ];

    /// The setting this is the base of, if steps can set it too.
    pub fn setting(self) -> Option<SettingId> {
        match self {
            Self::Cutoff => Some(SettingId::CUT),
            Self::Resonance => Some(SettingId::RES),
            Self::Pan => Some(SettingId::PAN),
            Self::SendA => Some(SettingId::SEND_A),
            Self::SendB => Some(SettingId::SEND_B),
            Self::Pitch => Some(SettingId::PITCH),
            Self::VelocityDelta => Some(SettingId::VELOCITY_DELTA),
            Self::Quantise => Some(SettingId::QUANTISE),
            Self::Jump => Some(SettingId::JUMP),
            Self::Drive => Some(SettingId::DRIVE),
            Self::Punch => Some(SettingId::PUNCH),
            Self::LowCut => Some(SettingId::LOW_CUT),
            Self::HighCut => Some(SettingId::HIGH_CUT),
            Self::Order => Some(SettingId::ORDER),
            Self::Reverse => Some(SettingId::REVERSE),
            Self::OrderX => Some(SettingId::ORDER_X),
            Self::OrderY => Some(SettingId::ORDER_Y),
            Self::PhraseLength => Some(SettingId::PHRASE_LENGTH),
            Self::PhraseStart => Some(SettingId::PHRASE_START),
            Self::ChanceDelta => Some(SettingId::CHANCE_DELTA),
            Self::ChanceTarget => Some(SettingId::CHANCE_TARGET),
            Self::Cue => Some(SettingId::CUE),
            Self::DiceLoop => Some(SettingId::DICE_LOOP),
            Self::DiceMutate => Some(SettingId::DICE_MUTATE),
            Self::DiceJump => Some(SettingId::DICE_JUMP),
            Self::Swing => Some(SettingId::SWING),
        }
    }
}

fn section_mut(song: &mut Song, section: usize) -> Result<&mut Section, EditError> {
    song.sections
        .get_mut(section)
        .ok_or(EditError::NoSection(section))
}

/// A choke group is 1 to `CHOKE_GROUPS`, or none.
fn check_choke(choke: Option<u8>) -> Result<(), EditError> {
    match choke {
        Some(group) if !(1..=CHOKE_GROUPS).contains(&group) => Err(EditError::Choke(group)),
        _ => Ok(()),
    }
}

fn track_mut(song: &mut Song, track: usize) -> Result<&mut Track, EditError> {
    song.tracks.get_mut(track).ok_or(EditError::NoTrack(track))
}

/// Why an edit was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("no section {0}")]
    NoSection(usize),
    #[error("a song needs at least one section")]
    LastSection,
    #[error("no phrase {0:02X}")]
    NoPhrase(u32),
    #[error("phrase {phrase:02X} has no step {step}")]
    NoStep { phrase: u32, step: usize },
    #[error("phrase {phrase:02X} has no live generator {generator}")]
    NoGenerator { phrase: u32, generator: usize },
    #[error("a live generator on a lane needs an FX column with a lane")]
    NoLane,
    #[error("there's no phrase {0} to follow")]
    NoSource(Source),
    #[error("a phrase can't follow itself")]
    FollowsItself,
    #[error("a bloom grows from 1 to {MAX_BRANCHES} branches")]
    Branches,
    #[error("the track has no phrase IDs left for {0} branches")]
    NoPhrasesLeft(u8),
    #[error("{0} follows this phrase already, directly or through others, so this can't follow it")]
    FollowCycle(Source),
    #[error("tempo must be from 20 to 400 BPM")]
    Tempo,
    #[error("a project can have at most {MAX_SOUND_TRACKS} tracks")]
    TooManyTracks,
    #[error("a project needs at least one track")]
    LastTrack,
    #[error("those columns don't match here")]
    ColumnMismatch,
    #[error(
        "a mod slot's depth goes to FF either way, an envelope's times to {MAX_ENVELOPE_MS} ms and its sustain to 100%, and an LFO's rate and phase are in range"
    )]
    Mod,
    #[error("no chain {0:02X}")]
    NoChain(u32),
    #[error("chain {chain:02X} has no row {slot:02X}")]
    NoRow { chain: u32, slot: usize },
    #[error("a chain needs at least one phrase")]
    LastRow,
    #[error("transpose goes from {MIN_TRANSPOSE} to {MAX_TRANSPOSE}")]
    Transpose,
    #[error("a project has {CUSTOM_GROOVES} grooves of its own")]
    CustomGroove,
    #[error("no track {0}")]
    NoTrack(usize),
    #[error("the bank has no sample {0:02X}")]
    NoSample(u8),
    #[error("a slice already starts there, or it's outside the sample")]
    Split,
    #[error("slice {0:02X} has no slice after it to join")]
    Join(u8),
    #[error("a reset needs at least 1 step")]
    Reset,
    #[error("a track's reset is from 1 to {MAX_TRACK_RESET} steps, or off")]
    TrackReset,
    #[error("choke group {0} isn't one of 1 to {CHOKE_GROUPS}")]
    Choke(u8),
    #[error("group {0} isn't one of 1 to {MAX_GROUPS}")]
    Group(u8),
    #[error("only tracks that play sounds go in a group")]
    GroupMember,
    #[error("a group's name is 1 to {GROUP_NAME_CHARS} letters or digits")]
    GroupName,
    #[error(
        "an accumulator's limits go from 1 to {MOST_DEGREES}, and its loops from 1 to {MOST_LOOPS}"
    )]
    Accumulation,
    #[error("an arpeggio climbs from 1 to {MOST_OCTAVES} octaves")]
    Arpeggio,
    #[error("a lane's Rate is one of its {} choices, on FX1 to FX{FX_SLOTS}", LANE_RATES.len())]
    LaneMove,
    #[error(
        "a track has macros 1 to {MACROS}, each turning settings within their range, never another macro, and named with up to {MACRO_NAME_LENGTH} plain characters"
    )]
    Macro,
    #[error(
        "a project's name is up to {MAX_PROJECT_NAME_CHARS} characters, with no / and not starting with a dot"
    )]
    SongName,
    #[error("only a track that plays sounds takes a sound preset")]
    NotSound,
}

/// Apply an action, returning a new song that shares everything unchanged with the old one.
/// Each family of actions is applied in a module of its own.
pub fn edit(song: &Arc<Song>, action: &Action) -> Result<Arc<Song>, EditError> {
    if let Action::Batch(actions) = action {
        let mut current = Arc::clone(song);
        for action in actions {
            current = edit(&current, action)?;
        }
        return Ok(current);
    }
    let mut next = Song::clone(song);
    match action {
        Action::Batch(_) => {}
        Action::SetNote { .. }
        | Action::SetStep { .. }
        | Action::SetField { .. }
        | Action::Shape { .. }
        | Action::Fill { .. }
        | Action::SetMark { .. }
        | Action::Clear { .. }
        | Action::Shift { .. }
        | Action::FitToScale { .. }
        | Action::Paste { .. } => steps::edit(&mut next, action)?,
        Action::Generate { .. }
        | Action::Reshape { .. }
        | Action::ApplyGenerator { .. }
        | Action::RemoveGenerator { .. }
        | Action::Double { .. }
        | Action::SetLength { .. }
        | Action::SlideLoop { .. }
        | Action::SetOrder { .. }
        | Action::SetLane { .. }
        | Action::RotateLoop { .. } => phrases::edit(&mut next, action)?,
        Action::SetSkipSize(_)
        | Action::SetLimiter(_)
        | Action::SetSongName(_)
        | Action::SetSeed(_)
        | Action::SetLaunch(_)
        | Action::SetGroove(_)
        | Action::SetCustomGroove { .. }
        | Action::SetMilliBpm(_) => song_wide::edit(&mut next, action)?,
        Action::SetReset { .. }
        | Action::AddSection { .. }
        | Action::CloneSection { .. }
        | Action::RemoveSection { .. }
        | Action::SetSectionLength { .. }
        | Action::SetSectionRepeat { .. }
        | Action::SetSectionStart { .. }
        | Action::SetSectionScale { .. }
        | Action::SetSectionChain { .. }
        | Action::SetTrackChain { .. } => sections::edit(&mut next, action)?,
        Action::SetChainLabel { .. }
        | Action::SetChainEntry { .. }
        | Action::InsertChainEntry { .. }
        | Action::RemoveChainEntry { .. }
        | Action::ClonePhrase { .. }
        | Action::CloneChain { .. }
        | Action::Bloom { .. } => chains::edit(&mut next, action)?,
        Action::SetTrackGroove { .. }
        | Action::SetTrackBase { .. }
        | Action::SetTrackLevel { .. }
        | Action::SetTrackTrim { .. }
        | Action::SetTrackRole { .. }
        | Action::SetTrackChoke { .. }
        | Action::SetTrackGroup { .. }
        | Action::SetTrackGateLength { .. }
        | Action::SetTrackSpeed { .. }
        | Action::SetTrackPhase { .. }
        | Action::SetTrackVoiceMode { .. }
        | Action::SetTrackReset { .. }
        | Action::SetTrackGlide { .. }
        | Action::SetTrackGlideMode { .. }
        | Action::SetTrackSafe { .. }
        | Action::SetTrackKeyTrack { .. }
        | Action::SetAccumulation { .. }
        | Action::SetArpeggio { .. }
        | Action::SetLaneMove { .. }
        | Action::AddTracks(_)
        | Action::RemoveLastTrack
        | Action::SetTrackName { .. } => tracks::edit(&mut next, action)?,
        Action::SetInsert { .. }
        | Action::LoadSound { .. }
        | Action::LoadPatch { .. }
        | Action::LoadEffect { .. }
        | Action::SetBypass { .. }
        | Action::SetMod { .. }
        | Action::SetMacro { .. }
        | Action::MapMacro { .. }
        | Action::UnmapMacro { .. }
        | Action::NameMacro { .. }
        | Action::SetTrackMachine { .. }
        | Action::SetTrackSetting { .. }
        | Action::SetTrackFine { .. }
        | Action::SetFilterType { .. }
        | Action::SetFilterMode { .. } => sound::edit(&mut next, action)?,
        Action::SetTrackSample { .. }
        | Action::SetBankEntry { .. }
        | Action::SetSlices { .. }
        | Action::SplitSlice { .. }
        | Action::JoinSlices { .. }
        | Action::LoadKit(_) => bank::edit(&mut next, action)?,
    }
    hold_generators(&mut next);
    follow_generators(song, &mut next);
    Ok(Arc::new(next))
}

fn phrase_mut(song: &mut Song, track: usize, phrase: u32) -> Result<&mut Phrase, EditError> {
    let target = track_mut(song, track)?
        .phrases
        .get_mut(&phrase)
        .ok_or(EditError::NoPhrase(phrase))?;
    Ok(Arc::make_mut(target))
}
