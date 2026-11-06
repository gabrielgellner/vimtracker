//! Line graphs in Braille characters, which have 2 by 4 dots in each cell.

use vt_model::constants::MAX_ENVELOPE_MS;
use vt_model::envelope::{Envelope, fall, rise};

/// The dot in a Braille character for a position within its cell.
const DOTS: [[u8; 2]; 4] = [[0, 3], [1, 4], [2, 5], [6, 7]];
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

/// The rows an envelope's line covers in each column of a grid of dots, top first, with row 0 at
/// the top. They join up, so a steep part is a line rather than scattered dots.
pub(crate) fn envelope_columns(
    envelope: &Envelope,
    columns: usize,
    rows: usize,
) -> Vec<(usize, usize)> {
    let bottom = rows - 1;
    let mut last = bottom;
    // An attack ends on full, however short it is and whatever comes next.
    let peak = (envelope.attack_ms > 0).then(|| stage_width(envelope.attack_ms) * columns as f32);
    (0..columns)
        .map(|column| {
            // Look across the whole column, so a peak between two columns isn't missed.
            let (mut high, mut low) = (last, last);
            for sample in 0..=SAMPLES_PER_COLUMN {
                let x =
                    (column as f32 + sample as f32 / SAMPLES_PER_COLUMN as f32) / columns as f32;
                let level = level_at(envelope, x).clamp(0.0, 1.0);
                let row = ((1.0 - level) * bottom as f32).round() as usize;
                high = high.min(row);
                low = low.max(row);
                last = row;
            }
            if peak.is_some_and(|peak| peak.floor() as usize == column) {
                high = 0;
            }
            (high, low)
        })
        .collect()
}

/// An envelope's shape as lines of Braille, from silence up to full and back down to silence.
pub fn envelope_lines(envelope: &Envelope, width: usize, height: usize) -> Vec<String> {
    let mut cells = vec![vec![0_u8; width]; height];
    let spans = envelope_columns(envelope, width * 2, height * 4);
    for (column, (high, low)) in spans.into_iter().enumerate() {
        for y in high..=low {
            cells[y / 4][column / 2] |= 1 << DOTS[y % 4][column % 2];
        }
    }
    cells
        .into_iter()
        .map(|cells| {
            cells
                .into_iter()
                .filter_map(|dots| char::from_u32(0x2800 + u32::from(dots)))
                .collect()
        })
        .collect()
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
    fn tops(lines: &[String]) -> Vec<Option<usize>> {
        let height = lines.len() * 4;
        let width = lines[0].chars().count() * 2;
        (0..width)
            .map(|column| {
                (0..height).find_map(|y| {
                    let cell = lines[y / 4].chars().nth(column / 2)? as u32 - 0x2800;
                    (cell & (1 << DOTS[y % 4][column % 2]) != 0).then_some(height - 1 - y)
                })
            })
            .collect()
    }

    #[test]
    fn it_is_the_size_asked_for_and_all_braille() {
        let lines = envelope_lines(&adsr(10, 100, 50, 100), 28, 4);
        assert_eq!(lines.len(), 4);
        for line in &lines {
            assert_eq!(line.chars().count(), 28);
            assert!(line.chars().all(|c| ('\u{2800}'..='\u{28FF}').contains(&c)));
        }
    }

    #[test]
    fn it_rises_to_full_settles_on_the_sustain_and_ends_at_silence() {
        let tops = tops(&envelope_lines(&adsr(100, 100, 50, 100), 28, 4));
        let levels: Vec<usize> = tops.iter().map(|top| top.unwrap_or(0)).collect();
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
        let tops = tops(&envelope_lines(&adsr(1, 180, 0, 50), 28, 4));
        let peak = tops.iter().position(|top| *top == Some(15)).unwrap();
        assert!(peak < 8, "the attack is near the start, at {peak}");
        assert!(tops[peak + 8].unwrap() < 15, "and it comes back down");
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
        let lines = envelope_lines(&adsr(0, 0, 100, 0), 28, 4);
        let tops = tops(&lines);
        assert!(tops.iter().all(Option::is_some));
        assert_eq!(tops[1], Some(15));
    }
}
