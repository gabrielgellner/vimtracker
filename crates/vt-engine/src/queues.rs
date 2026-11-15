//! How long each queue between the edit thread and the audio callback is. They're the engine's
//! own, as it makes every one of them in [`crate::link::connect`] but the input's, which the
//! program makes at this length and hands to the link.

/// Song versions waiting for the callback.
pub(crate) const PUBLISH_QUEUE: usize = 1;
/// Long buffers on their way to or from the callback at once. More wait their turn.
pub(crate) const MEMORY_QUEUE: usize = 256;
/// Versions waiting to be freed.
pub(crate) const RETURN_QUEUE: usize = 16;
/// Transport, preview and mute messages.
pub(crate) const COMMAND_QUEUE: usize = 64;
/// One-off files on their way to the callback to be heard, as the sample browser sends them, and
/// on their way back to be freed.
pub(crate) const FILE_QUEUE: usize = 4;
/// Recordings on their way to the callback's recorder.
pub(crate) const TAKE_QUEUE: usize = 4;
/// The recorder's starts and ends on their way back, each end with its recording to be freed.
pub(crate) const RECORDER_QUEUE: usize = 16;
/// The audio input's samples on their way to the callback, interleaved stereo: about a sixth of a
/// second at 48 kHz, as the input's callback and the output's take turns.
pub const INPUT_QUEUE: usize = 16_384;
/// The most input frames kept waiting for the output's callback. Past it, the oldest are dropped,
/// so the two devices' clocks drifting apart can't make the input later and later.
pub(crate) const INPUT_MOST_WAITING: usize = 4096;
/// Playhead and load messages. New ones are dropped when it is full.
pub(crate) const TELEMETRY_QUEUE: usize = 64;
/// The pitch each mod slot moved a row's note by, on their way to the app for `Space g A`. New
/// ones are dropped when it is full.
pub(crate) const NOTE_PITCH_QUEUE: usize = 256;
/// What was heard of live generators: each pass of one that varies, each row a moving one played and
/// each come to rest, on their way to the app for `Space g a` and the playhead's row. New ones are
/// dropped when it is full.
pub(crate) const GEN_HEARD_QUEUE: usize = 256;
/// Samples of the main output on their way to the scope, about a third of a second at 48 kHz. New
/// ones are dropped when it is full.
pub(crate) const SCOPE_QUEUE: usize = 16_384;
