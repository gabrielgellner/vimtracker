//! Design constants and tuning values. Tuning values can change by ear with no document update.

/// Current `song.ron` format.
pub const FORMAT_VERSION: u32 = 1;
/// Tracks the engine prepares state for. Raising this does not affect saved songs.
pub const MAX_TRACKS: usize = 64;
/// Tracks in a new project.
pub const DEFAULT_TRACKS: usize = 8;
/// Clock ticks per step, at 960 per quarter note.
pub const TICKS_PER_STEP: u64 = 240;
/// Clock ticks per quarter note.
pub const TICKS_PER_QUARTER: u64 = 960;
/// One unit for `DEL`, `KIL`, `OFF` and `RET`, 6 per step.
pub const GROOVE_UNIT_TICKS: u64 = 40;
/// Steps in a new phrase.
pub const DEFAULT_PHRASE_STEPS: usize = 16;
/// Steps per beat, for `ib` and `{ }`.
pub const BEAT_STEPS: usize = 4;
/// FX slots per step.
pub const FX_SLOTS: usize = 3;
/// New project tempo, in thousandths of a BPM.
pub const DEFAULT_MILLIBPM: u32 = 120_000;
/// Lowest tempo, in thousandths of a BPM.
pub const MIN_MILLIBPM: u32 = 20_000;
/// Highest tempo, in thousandths of a BPM.
pub const MAX_MILLIBPM: u32 = 400_000;
/// Undo versions kept.
pub const HISTORY_LIMIT: usize = 1000;
/// Sample rate for tests and `render`, in Hz.
pub const DEFAULT_RENDER_RATE: u32 = 44_100;
/// Song versions waiting for the callback.
pub const PUBLISH_QUEUE: usize = 1;
/// Versions waiting to be freed.
pub const RETURN_QUEUE: usize = 16;
/// Transport, preview and mute messages.
pub const COMMAND_QUEUE: usize = 64;
/// Playhead and load messages. New ones are dropped when it is full.
pub const TELEMETRY_QUEUE: usize = 64;

/// Tuning: smoother time for filter, volume and pan changes.
pub const SMOOTHING_MS: u32 = 10;
/// Tuning: fade length when a voice is cut.
pub const DECLICK_FRAMES: u32 = 64;
/// Tuning: how long an entered note previews.
pub const PREVIEW_MS: u32 = 250;
/// Tuning: time between redraws.
pub const FRAME_MS: u64 = 33;
/// Tuning: how long a transport key ignores its own repeats.
pub const TRANSPORT_GUARD_MS: u64 = 450;
/// Tuning: master gain before the soft clip, about -6 dB.
pub const MASTER_GAIN: f32 = 0.5;

/// The lowest swing, in tenths of a percent, which is straight time.
pub const MIN_SWING: u16 = 500;

/// The highest swing, in tenths of a percent.
pub const MAX_SWING: u16 = 750;

/// The longest attack or release, in milliseconds.
pub const MAX_ENVELOPE_MS: u16 = 5000;

/// The lowest chain transpose, in semitones.
pub const MIN_TRANSPOSE: i8 = -48;

/// The highest chain transpose, in semitones.
pub const MAX_TRANSPOSE: i8 = 48;

/// How often unsaved work is written aside.
pub const AUTOSAVE_SECONDS: u64 = 60;
