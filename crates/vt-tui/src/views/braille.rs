//! Line graphs in Braille characters, which have 2 by 4 dots in each cell.

use vt_model::envelope::{Envelope, fall, rise};

/// The dot in a Braille character for a position within its cell.
const DOTS: [[u8; 2]; 4] = [[0, 3], [1, 4], [2, 5], [6, 7]];
/// How many places to look within each column of dots.
const SAMPLES_PER_COLUMN: usize = 8;
/// How much of the time axis a sustain takes, beside the other stages together.
const SUSTAIN_SHARE: f32 = 0.25;

/// An envelope's level at `x` from 0 to 1 across its diagram. Time is on a square-root scale, so a
/// 1 ms attack still shows beside a 500 ms release.
pub(crate) fn level_at(envelope: &Envelope, x: f32) -> f32 {
    let scale = |ms: u16| f32::from(ms).sqrt();
    let (attack, decay, release) = (
        scale(envelope.attack_ms),
        scale(envelope.decay_ms),
        scale(envelope.release_ms),
    );
    let sustain_length = ((attack + decay + release) * SUSTAIN_SHARE).max(1.0);
    let mut at = x * (attack + decay + sustain_length + release);
    let sustain = envelope.sustain_level();
    if at < attack {
        return rise(at / attack);
    }
    at -= attack;
    if at < decay {
        return 1.0 + (sustain - 1.0) * fall(at / decay);
    }
    at -= decay;
    if at < sustain_length {
        return sustain;
    }
    at -= sustain_length;
    if at < release {
        return sustain * (1.0 - fall(at / release));
    }
    0.0
}

/// An envelope's shape as lines of Braille, from silence up to full and back down to silence.
pub fn envelope_lines(envelope: &Envelope, width: usize, height: usize) -> Vec<String> {
    let (columns, rows) = (width * 2, height * 4);
    let mut cells = vec![vec![0_u8; width]; height];
    let bottom = rows - 1;
    let mut last = bottom;
    for column in 0..columns {
        // Look across the whole column, so a peak between two columns isn't missed.
        let (mut high, mut low) = (last, last);
        for sample in 0..=SAMPLES_PER_COLUMN {
            let x = (column as f32 + sample as f32 / SAMPLES_PER_COLUMN as f32) / columns as f32;
            let level = level_at(envelope, x).clamp(0.0, 1.0);
            let row = ((1.0 - level) * bottom as f32).round() as usize;
            high = high.min(row);
            low = low.max(row);
            last = row;
        }
        // Joined up, so a steep part is a line rather than scattered dots.
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
    fn an_instant_envelope_is_safe_to_draw() {
        let lines = envelope_lines(&adsr(0, 0, 100, 0), 28, 4);
        let tops = tops(&lines);
        assert!(tops.iter().all(Option::is_some));
        assert_eq!(tops[1], Some(15));
    }
}
