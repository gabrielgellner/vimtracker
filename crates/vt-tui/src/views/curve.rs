//! Lines through a grid of dots, which the track page's drawings are made from.

use vt_model::constants::MAX_ENVELOPE_MS;
use vt_model::envelope::{Envelope, fall, rise};

/// How many places to look within each column of dots.
const SAMPLES_PER_COLUMN: usize = 8;
/// The most of a diagram's width each stage can take, with a sustain always taking this much.
const STAGE_SHARE: f32 = 0.25;

/// How much of a diagram's width a stage takes.
fn stage_width(ms: u16) -> f32 {
    STAGE_SHARE * (1.0 + f32::from(ms)).ln() / (1.0 + f32::from(MAX_ENVELOPE_MS)).ln()
}

/// An envelope's level at `x` from 0 to 1 across its diagram. A stage's width depends only on its
/// own time, on a log scale so a 1 ms attack shows beside a long release, and changing a stage only
/// moves what comes after it. A short envelope leaves room on the right.
pub(crate) fn level_at(envelope: &Envelope, x: f32) -> f32 {
    let (attack, decay, release) = (
        stage_width(envelope.attack_ms),
        stage_width(envelope.decay_ms),
        stage_width(envelope.release_ms),
    );
    let mut at = x;
    let sustain = envelope.sustain_level();
    if at < attack {
        return rise(at / attack);
    }
    at -= attack;
    if at < decay {
        return 1.0 + (sustain - 1.0) * fall(at / decay);
    }
    at -= decay;
    if at < STAGE_SHARE {
        return sustain;
    }
    at -= STAGE_SHARE;
    if at < release {
        return sustain * (1.0 - fall(at / release));
    }
    0.0
}

/// The rows a line covers in each column of a grid of dots, top first, with row 0 at the top. It
/// runs across `x` from 0 to 1, starting from `start`, and each level is from 0 to 1. It joins up,
/// so a steep part is a line rather than scattered dots.
pub(crate) fn line_columns(
    level_at: impl Fn(f32) -> f32,
    start: f32,
    columns: usize,
    rows: usize,
) -> Vec<(usize, usize)> {
    let bottom = rows - 1;
    let row_of = |level: f32| ((1.0 - level.clamp(0.0, 1.0)) * bottom as f32).round() as usize;
    let mut last = row_of(start);
    (0..columns)
        .map(|column| {
            // Look across the whole column, so a peak between two columns isn't missed.
            let (mut high, mut low) = (last, last);
            for sample in 0..=SAMPLES_PER_COLUMN {
                let x =
                    (column as f32 + sample as f32 / SAMPLES_PER_COLUMN as f32) / columns as f32;
                let row = row_of(level_at(x));
                high = high.min(row);
                low = low.max(row);
                last = row;
            }
            (high, low)
        })
        .collect()
}

/// The rows an envelope's line covers in each column, from silence.
pub(crate) fn envelope_columns(
    envelope: &Envelope,
    columns: usize,
    rows: usize,
) -> Vec<(usize, usize)> {
    let mut spans = line_columns(|x| level_at(envelope, x), 0.0, columns, rows);
    // An attack ends on full, however short it is and whatever comes next.
    if envelope.attack_ms > 0 {
        let peak = (stage_width(envelope.attack_ms) * columns as f32) as usize;
        spans[peak].0 = 0;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Envelope {
        Envelope {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            ..Envelope::note_shape()
        }
    }

    /// The highest dot in each column, as a level from 0 at the bottom row.
    fn tops(envelope: &Envelope, columns: usize, rows: usize) -> Vec<usize> {
        envelope_columns(envelope, columns, rows)
            .into_iter()
            .map(|(high, _)| rows - 1 - high)
            .collect()
    }

    #[test]
    fn it_rises_to_full_settles_on_the_sustain_and_ends_at_silence() {
        let levels = tops(&adsr(100, 100, 50, 100), 56, 16);
        assert_eq!(levels.len(), 56);
        assert_eq!(levels[0], 0);
        assert_eq!(*levels.iter().max().unwrap(), 15, "reaches the top row");
        assert_eq!(*levels.last().unwrap(), 0);
        // The sustain sits on the flat part, about half way up.
        assert!(
            levels.iter().filter(|level| **level == 8).count() >= 4,
            "{levels:?}"
        );
    }

    #[test]
    fn a_short_attack_still_shows() {
        let levels = tops(&adsr(1, 180, 0, 50), 56, 16);
        let peak = levels.iter().position(|level| *level == 15).unwrap();
        assert!(peak < 8, "the attack is near the start, at {peak}");
        assert!(levels[peak + 8] < 15, "and it comes back down");
    }

    #[test]
    fn a_stage_only_moves_what_comes_after_it() {
        let peak = |attack_ms, decay_ms| {
            let spans = envelope_columns(&adsr(attack_ms, decay_ms, 0, 50), 112, 32);
            spans.iter().position(|(high, _)| *high == 0).unwrap()
        };
        for decay_ms in [0, 1, 50, 200, 8000] {
            assert_eq!(peak(1, decay_ms), peak(1, 0), "with a decay of {decay_ms}");
        }
        assert!(peak(200, 0) > peak(1, 0));
    }

    #[test]
    fn the_longest_envelope_fits_and_a_short_one_leaves_room() {
        let longest = envelope_columns(&adsr(8000, 8000, 50, 8000), 112, 32);
        assert_eq!(longest.len(), 112);
        assert_eq!(longest[111].1, 31);
        // A short envelope's line has reached silence by half way.
        let short = envelope_columns(&adsr(1, 0, 100, 10), 112, 32);
        assert!(
            short[56..]
                .iter()
                .all(|(high, low)| (*high, *low) == (31, 31))
        );
    }

    #[test]
    fn an_instant_envelope_is_safe_to_draw() {
        let spans = envelope_columns(&adsr(0, 0, 100, 0), 56, 16);
        assert_eq!(spans.len(), 56);
        assert!(spans.iter().all(|(high, low)| high <= low && *low < 16));
        assert_eq!(spans[2].0, 0);
    }
}
