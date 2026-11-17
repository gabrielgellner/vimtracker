//! What the track's own settings do that the registry doesn't hold, as [`Entry::meaning`] says it
//! for those it does: in a few plain words starting in lower case, for the line that describes
//! the setting under the cursor (#969). The pages describe these settings themselves, so their
//! words are here, in one place, rather than beside a descriptor.
//!
//! [`Entry::meaning`]: super::Entry::meaning

pub const MACHINE: &str = "what makes the track's sound";
pub const SAMPLE: &str = "the sample from the bank the sampler plays";
pub const TUNE: &str = "the sample's own tuning in semitones, or its slice's where it has one";
pub const CENTS: &str = "the sample's own fine tuning, in cents, on top of its tune";
pub const FILTER_TYPE: &str = "which filter the sound goes through";
pub const FILTER_MODE: &str = "how the filter's type responds: its slope, or what it lets through";
pub const SAFE: &str = "whether Res stops just short of where the filter rings by itself";
pub const TRIM: &str = "turns what comes in up or down before anything else, for gain staging";
pub const ROLE: &str = "what the track is for, which its colour shows";
pub const CHOKE: &str = "a note here cuts off the other tracks in the same choke group";
pub const GROUP: &str = "the group the track plays through, or straight to the master";
pub const RESET: &str =
    "how many of the song's steps the track's chain plays before starting again";
pub const LIMITER: &str = "whether the song's limiter keeps the master from clipping";
pub const EFFECT: &str = "the effect in this slot";
pub const NEW_GENERATOR: &str = "K makes another column of the phrase live";
pub const ACCUMULATOR_MOVES: &str = "whose note an accumulator moves";
pub const ACCUMULATOR_AT_LIMIT: &str = "what an accumulator does when it reaches a limit";
pub const ACCUMULATOR_ADDS: &str = "when an accumulator adds its amount";
pub const ACCUMULATOR_RESET: &str = "when the accumulators go back to the notes as written";
pub const ARPEGGIO_MODE: &str = "the order the arpeggio plays the held notes in";
pub const ARPEGGIO_RETRIG: &str =
    "whether each arpeggiated note starts the envelopes again, rather than only changing the pitch";
pub const DRIVE: &str = "how hard the sound is pushed into the filter";
pub const PUNCH: &str =
    "how far a ringing peak starts higher and falls in pitch as it dies away, 00 off";
pub const LOW_CUT: &str = "a high-pass after the filter, to thin the low end; 00 is off";
pub const HIGH_CUT: &str = "a low-pass after the filter, to soften the top; FF is off";
