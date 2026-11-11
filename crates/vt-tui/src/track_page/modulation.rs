//! The Mod page: the envelopes, the LFOs, the accumulator and the arpeggio, each a row.

use vt_model::constants::TRACK_ENVELOPES;
use vt_model::lfo::LFOS_PER_TRACK;

use super::{AccumulatorField, ArpField, Control, EnvelopeField, LfoField};

const fn lfo_row(slot: usize) -> [Control; 6] {
    [
        Control::Lfo(slot, LfoField::Destination),
        Control::Lfo(slot, LfoField::Shape),
        Control::Lfo(slot, LfoField::Rate),
        Control::Lfo(slot, LfoField::Depth),
        Control::Lfo(slot, LfoField::Phase),
        Control::Lfo(slot, LfoField::Retrig),
    ]
}

const fn envelope_row(slot: usize) -> [Control; 7] {
    [
        Control::Envelope(slot, EnvelopeField::Destination),
        Control::Envelope(slot, EnvelopeField::Attack),
        Control::Envelope(slot, EnvelopeField::Decay),
        Control::Envelope(slot, EnvelopeField::Sustain),
        Control::Envelope(slot, EnvelopeField::Release),
        Control::Envelope(slot, EnvelopeField::Depth),
        Control::Envelope(slot, EnvelopeField::Reset),
    ]
}

/// Each envelope's row.
pub static ENVELOPES: [[Control; 7]; TRACK_ENVELOPES] = {
    let mut rows = [envelope_row(0); TRACK_ENVELOPES];
    let mut slot = 0;
    while slot < TRACK_ENVELOPES {
        rows[slot] = envelope_row(slot);
        slot += 1;
    }
    rows
};

/// Each LFO's row.
pub static LFOS: [[Control; 6]; LFOS_PER_TRACK] = {
    let mut rows = [lfo_row(0); LFOS_PER_TRACK];
    let mut slot = 0;
    while slot < LFOS_PER_TRACK {
        rows[slot] = lfo_row(slot);
        slot += 1;
    }
    rows
};

/// The accumulators' row, under the LFOs, as they move notes from pass to pass as the LFOs move
/// settings over time.
pub const ACCUMULATOR: [Control; 6] = [
    Control::Accumulator(AccumulatorField::Up),
    Control::Accumulator(AccumulatorField::Down),
    Control::Accumulator(AccumulatorField::Moves),
    Control::Accumulator(AccumulatorField::AtLimit),
    Control::Accumulator(AccumulatorField::Adds),
    Control::Accumulator(AccumulatorField::Reset),
];

/// The arpeggio's row, under the accumulator's, as both play a row's note on from what's written.
pub const ARPEGGIO: [Control; 4] = [
    Control::Arpeggio(ArpField::Rate),
    Control::Arpeggio(ArpField::Mode),
    Control::Arpeggio(ArpField::Octaves),
    Control::Arpeggio(ArpField::Retrig),
];

/// The Mod page's rows, top to bottom.
pub static MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2] = {
    let mut rows: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2] =
        [&[]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2];
    let mut row = 0;
    while row < FIXED_MOD_ROWS.len() {
        rows[row] = FIXED_MOD_ROWS[row];
        row += 1;
    }
    rows[row] = &ACCUMULATOR;
    rows[row + 1] = &ARPEGGIO;
    rows
};

/// The Mod page of a return or the master, which have no notes for an accumulator or an arpeggio
/// to play: each envelope's row, then each LFO's.
pub static FIXED_MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] = {
    let mut rows: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] =
        [&[]; TRACK_ENVELOPES + LFOS_PER_TRACK];
    let mut slot = 0;
    while slot < TRACK_ENVELOPES {
        rows[slot] = &ENVELOPES[slot];
        slot += 1;
    }
    let mut slot = 0;
    while slot < LFOS_PER_TRACK {
        rows[TRACK_ENVELOPES + slot] = &LFOS[slot];
        slot += 1;
    }
    rows
};
