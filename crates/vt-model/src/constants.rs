//! Design constants and tuning values. Tuning values can change by ear with no document update.

/// Current `song.ron` format.
pub const FORMAT_VERSION: u32 = 5;
/// The level value that leaves a sound as it is, at 0 dB. Above it is headroom.
pub const UNITY_LEVEL: u8 = 0xE0;
/// The highest level value, at +6 dB as on Ableton Live's faders. Anything above it snaps to it.
pub const MAX_LEVEL: u8 = 0xF8;
/// How many dB one step of a level value is.
pub const DB_PER_LEVEL: f32 = 0.25;
/// Tracks that play sounds a song can have.
pub const MAX_SOUND_TRACKS: usize = 64;
/// The tracks every song has after those: two returns and the master.
pub const FIXED_TRACKS: usize = 3;
/// Tracks the engine prepares state for. Raising this does not affect saved songs.
pub const MAX_TRACKS: usize = MAX_SOUND_TRACKS + FIXED_TRACKS;
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
/// The most steps a phrase keeps, so a big count on a paste or a slide can't use up memory.
pub const MAX_PHRASE_STEPS: usize = 4096;
/// Steps per beat, for `ib` and `{ }`.
pub const BEAT_STEPS: usize = 4;
/// Steps per bar, for launch timing and large steps through lengths.
pub const BAR_STEPS: usize = 16;
/// The most characters a chain's label can have.
pub const MAX_LABEL_CHARS: usize = 32;
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
/// Long buffers on their way to or from the callback at once. More wait their turn.
pub const MEMORY_QUEUE: usize = 256;
/// Versions waiting to be freed.
pub const RETURN_QUEUE: usize = 16;
/// Transport, preview and mute messages.
pub const COMMAND_QUEUE: usize = 64;
/// Playhead and load messages. New ones are dropped when it is full.
pub const TELEMETRY_QUEUE: usize = 64;
/// Samples of the main output on their way to the scope, about a third of a second at 48 kHz. New
/// ones are dropped when it is full.
pub const SCOPE_QUEUE: usize = 16_384;

/// Tuning: smoother time for filter, volume and pan changes.
pub const SMOOTHING_MS: u32 = 10;
/// Tuning: smoothing on what envelopes move, short enough to keep a fast snap.
pub const FAST_SMOOTHING_MS: u32 = 1;
/// Tuning: fade length when a voice is cut.
pub const DECLICK_FRAMES: u32 = 64;
/// The velocity new notes are written with until changed, leaving room above it for accents.
pub const DEFAULT_NOTE_VELOCITY: u8 = 0x64;
/// Tuning: how long an entered note previews.
pub const PREVIEW_MS: u32 = 250;
/// Tuning: how many notes the keyboard can sound at once, for chords. A ninth takes over the
/// oldest.
pub const PREVIEW_VOICES: usize = 8;
/// Tuning: time between redraws.
pub const FRAME_MS: u64 = 33;

/// The lowest swing, in tenths of a percent, which is straight time.
pub const MIN_SWING: u16 = 500;

/// The highest swing, in tenths of a percent.
pub const MAX_SWING: u16 = 750;

/// The longest attack or release, in milliseconds.
pub const MAX_ENVELOPE_MS: u16 = 8000;
/// How many envelopes a track has.
pub const TRACK_ENVELOPES: usize = 2;

/// The lowest chain transpose, in semitones.
pub const MIN_TRANSPOSE: i8 = -48;

/// The highest chain transpose, in semitones.
pub const MAX_TRANSPOSE: i8 = 48;

/// How often unsaved work is written aside.
pub const AUTOSAVE_SECONDS: u64 = 60;
