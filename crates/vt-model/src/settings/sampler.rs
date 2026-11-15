//! The sampler's settings, and what its stepped ones can be.

use super::{Entry, Kind, Units};

/// Each choice's two hex digits, for [`BANK_CHOICES`] to point into.
static BANK_DIGITS: [[u8; 2]; 256] = {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut digits = [[0; 2]; 256];
    let mut at = 0;
    while at < 256 {
        digits[at] = [HEX[at >> 4], HEX[at & 0x0F]];
        at += 1;
    }
    digits
};

/// What a sampler's `Sample` can be: `Off`, no sample, then a bank entry's number, `01` to `FF`.
/// What each entry is called is the song's, so it's shown from the bank where there's a song.
pub const BANK_CHOICES: [&str; 256] = {
    let mut choices = ["Off"; 256];
    let mut at = 1;
    while at < 256 {
        choices[at] = match std::str::from_utf8(&BANK_DIGITS[at]) {
            Ok(digits) => digits,
            Err(_) => panic!("hex digits are text"),
        };
        at += 1;
    }
    choices
};

/// What a sampler's `Slice` can be: a slice's number, `00` to `FF`, as the sample page's ruler
/// shows it.
pub const SLICE_CHOICES: [&str; 256] = {
    let mut choices = [""; 256];
    let mut at = 0;
    while at < 256 {
        choices[at] = match std::str::from_utf8(&BANK_DIGITS[at]) {
            Ok(digits) => digits,
            Err(_) => panic!("hex digits are text"),
        };
        at += 1;
    }
    choices
};

/// How a sampler's note plays its sample: through once, round its loop, back and forth in it, on
/// through the slices after its own, or round its loop in time with the song, as [`PlayMode`]
/// says.
pub const PLAY_CHOICES: [&str; 5] = ["One-shot", "Loop", "Ping-pong", "Thru", "Tempo"];

/// How a sampler's note plays its sample, its `Play`. A loop is the entry's own if it has one,
/// or else the whole of what plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlayMode {
    /// From its start to its end, once.
    #[default]
    OneShot,
    /// From its start, then round its loop until the note ends.
    Loop,
    /// From its start, then back and forth in its loop.
    PingPong,
    /// From its slice's start on past the slice's end, to the entry's, once, as tracker-ce's Play
    /// Thru and Simpler's Thru do. Reversed, it plays back past the slice's start to the entry's.
    Thru,
    /// Round its loop as `Loop` does, at the speed that has the entry last as many of the song's
    /// steps as its hints say, whatever the tempo: loop to tempo, as the Deluge's audio clips and
    /// the Octatrack's and Simpler's synced loops play. Its warp says how it gets there. An entry
    /// that doesn't say how long it is loops at its own speed.
    Tempo,
}

impl PlayMode {
    /// The mode at a place in [`PLAY_CHOICES`], or the last for any past it.
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Self::OneShot,
            1 => Self::Loop,
            2 => Self::PingPong,
            3 => Self::Thru,
            _ => Self::Tempo,
        }
    }
}

/// A share of a sample, as `Start` and `Length` take it: `00` none of it and `FF` all of it.
const fn share(key: &'static str, name: &'static str, label: &'static str, default: u8) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default,
    }
}

/// How many settings the sampler has.
pub const SAMPLER_SETTINGS: usize = SAMPLER.len();

/// The sampler's own, which it keeps whatever it plays. `Length` isn't `LEN`, which is `Gate`'s.
pub(super) const SAMPLER: [Entry; 5] = [
    Entry {
        key: "sample",
        name: "Sample",
        label: Some("SMP"),
        kind: Kind::Stepped {
            choices: &BANK_CHOICES,
            wraps: false,
        },
        default: 0,
    },
    Entry {
        key: "slice",
        name: "Slice",
        label: Some("SLI"),
        kind: Kind::Stepped {
            choices: &SLICE_CHOICES,
            wraps: false,
        },
        default: 0,
    },
    share("start", "Start", "STA", 0),
    share("length", "Length", "SLEN", 0xFF),
    Entry {
        key: "play",
        name: "Play",
        label: Some("PLY"),
        kind: Kind::Stepped {
            choices: &PLAY_CHOICES,
            wraps: false,
        },
        default: 0,
    },
];
