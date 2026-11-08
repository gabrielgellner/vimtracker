//! A sample's waveform, folded down to columns of dots, with no drawing.
//!
//! Each column keeps the lowest and highest sample of both channels in its stretch of the sample,
//! rather than one sample, so a long sample doesn't alias into something it isn't. It's drawn at
//! its real level, as the scope is, so full height is `0` and silence is a line half way down.

use crate::scope::{fold, rows_of};

/// The rows a sample's waveform covers in each of `columns` columns, as the track page's curves are
/// given, with a line from top to bottom at each of `markers`, given as frames into the sample.
/// The sample bank's start, end, loop and slice points (#189) are what markers are for; the Sample
/// machine has none of them yet.
pub fn spans(
    frames: &[[f32; 2]],
    markers: &[usize],
    columns: usize,
    rows: usize,
) -> Vec<(usize, usize)> {
    let mut spans = if frames.is_empty() {
        vec![(rows / 2, rows / 2); columns]
    } else {
        rows_of(&fold(frames.as_flattened(), columns), rows)
    };
    for marker in markers {
        let column = marker * columns / frames.len().max(1);
        if let Some(span) = spans.get_mut(column) {
            *span = (0, rows - 1);
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_column_keeps_its_lowest_and_highest_of_either_channel() {
        // Four frames to each of two columns, with the peaks in one channel or the other.
        let frames = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, -1.0],
            [0.0, 0.0],
            [0.0, 0.0],
            [0.0, 0.5],
            [0.0, 0.0],
            [0.0, 0.0],
        ];
        assert_eq!(spans(&frames, &[], 2, 5), [(0, 4), (1, 2)]);
    }

    #[test]
    fn a_fast_wave_folds_to_its_full_height_rather_than_aliasing() {
        // A tone near the top of hearing, with far more cycles than columns: one sample a column
        // would land on wherever its phase happens to be, and draw a slow wave that isn't there.
        let frames: Vec<[f32; 2]> = (0..48_000)
            .map(|at| {
                let sample = (std::f32::consts::TAU * 15_001.0 * at as f32 / 48_000.0).sin();
                [sample, sample]
            })
            .collect();
        let spans = spans(&frames, &[], 144, 36);
        assert!(spans.iter().all(|span| *span == (0, 35)), "{spans:?}");
    }

    #[test]
    fn silence_and_an_empty_sample_are_a_line_half_way_down() {
        assert!(
            spans(&[[0.0; 2]; 100], &[], 24, 37)
                .iter()
                .all(|span| *span == (18, 18))
        );
        assert_eq!(spans(&[], &[], 24, 37), vec![(18, 18); 24]);
        // Fewer frames than columns still gives each column something.
        assert_eq!(spans(&[[1.0; 2], [-1.0; 2]], &[], 6, 37).len(), 6);
    }

    #[test]
    fn a_marker_is_a_line_from_top_to_bottom_where_it_falls() {
        let frames = [[0.0; 2]; 1000];
        let spans = spans(&frames, &[0, 500, 999], 10, 12);
        assert_eq!(spans[0], (0, 11));
        assert_eq!(spans[5], (0, 11));
        assert_eq!(spans[9], (0, 11));
        assert_eq!(spans[4], (6, 6));
    }
}
