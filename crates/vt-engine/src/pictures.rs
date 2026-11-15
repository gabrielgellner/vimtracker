//! What the screen draws of the sound: the dynamics' curves, the filters' and EQ's responses,
//! Distort's transfer, the delays' repeats and the reverbs' tails. Each is worked out by the code
//! that plays it, so a picture can't drift from what's heard.
//!
//! Names several effects share are kept in a module named after the effect, as `lpg::response`.

pub use crate::dsp::dynamics::compressor::curve_db;
pub use crate::dsp::effects::distort::transfer;
pub use crate::dsp::effects::slot::DEFAULT_NOTE;
pub use crate::dsp::effects::tail::Tail;
pub use crate::dsp::filters::filter::track_response_db;

/// The delay's repeats, and how long a tick lasts at a tempo.
pub mod delay {
    pub use crate::dsp::effects::delay::{Repeats, frames_per_tick, repeats};
}

/// The Space Echo's repeats.
pub mod echo {
    pub use crate::dsp::effects::echo::repeats;
}

/// The EQ's response.
pub mod eq {
    pub use crate::dsp::dynamics::eq::response_db;
}

/// The plate's tail.
pub mod plate {
    pub use crate::dsp::effects::plate::tail;
}

/// The reverb's tail.
pub mod reverb {
    pub use crate::dsp::effects::reverb::tail;
}

/// Space's tail.
pub mod space {
    pub use crate::dsp::effects::space::tail;
}

/// The spring's tail.
pub mod spring {
    pub use crate::dsp::effects::spring::tail;
}

/// The comb filter's response at a note.
pub mod comb {
    pub use crate::dsp::filters::comb::response;
}

/// The low-pass gate's response, and where it rests and opens.
pub mod lpg {
    pub use crate::dsp::filters::lpg::{opened, response, resting};
}

/// The resonator's response at a note.
pub mod resonator {
    pub use crate::dsp::effects::resonator::response;
}

/// The vowel filter's response at a note.
pub mod vowel {
    pub use crate::dsp::filters::vowel::response;
}
