//! The phrase view's timeline (decision 122): each row of the loop takes a line for each of its
//! pulses, so a line is one of the track's own steps, and the other tracks' columns beside the
//! grid line up by time rather than by row number. `z a` collapses it back to a line a row.
//!
//! The cursor, counts, selections and the view's `top` all stay in rows. Only how many lines a
//! row takes on screen changes, so scrolling works the screen out in lines here.

use std::ops::Range;

use vt_model::song::Phrase;
use vt_model::step::Step;

use crate::app::App;

/// When each of these rows starts, in steps from the first, as each lasts its pulses. A row of no
/// pulses starts with the row after it.
pub fn starts(steps: &[Step]) -> Vec<usize> {
    steps
        .iter()
        .scan(0, |time, step| {
            let start = *time;
            *time += step.pulses();
            Some(start)
        })
        .collect()
}

/// How many lines a stored row takes: one for each of its pulses in the timeline while it plays,
/// and one otherwise. A skipped row still takes one, so it can be edited.
pub fn row_lines(phrase: &Phrase, index: usize, timeline: bool) -> usize {
    match phrase.steps().get(index) {
        Some(step) if timeline && phrase.extent().plays(index) => step.pulses().max(1),
        _ => 1,
    }
}

/// One line of the phrase view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    /// The row it's part of, which is past the stored rows for a line kept for a longer track.
    pub index: usize,
    /// Which of the row's pulses: 0 for the row's own line, and more for its continuation lines.
    pub pulse: usize,
    /// When it starts, in the track's steps from row `00`. A row outside the loop doesn't play,
    /// so it goes by its number before the loop, and carries on from the loop's end after it.
    pub time: isize,
    /// Whether it takes any time. A skipped row takes none, so nothing starts beside it.
    pub lasts: bool,
}

/// The lines of a phrase's first `rows` rows, its stored ones and any past them, laid out in time
/// or a line a row.
pub fn lines(phrase: &Phrase, timeline: bool, rows: usize) -> Vec<Line> {
    let extent = phrase.extent();
    let starts = starts(phrase.loop_steps());
    let loop_end = extent.start + extent.length;
    let mut lines = Vec::with_capacity(rows);
    for index in 0..rows {
        if extent.plays(index) {
            let start = starts[index - extent.start] as isize;
            let lasts = phrase.steps()[index].pulses() > 0;
            for pulse in 0..row_lines(phrase, index, timeline) {
                lines.push(Line {
                    index,
                    pulse,
                    time: start + pulse as isize,
                    lasts,
                });
            }
        } else {
            let time = if timeline && index >= loop_end {
                (phrase.time() + index - loop_end) as isize
            } else {
                extent.row(index)
            };
            lines.push(Line {
                index,
                pulse: 0,
                time,
                lasts: true,
            });
        }
    }
    lines
}

/// The ticks a line covers, from row `00`, at a track's step of this many ticks: none before row
/// `00` or on a line that takes no time.
pub fn ticks_of(line: &Line, ticks: u64) -> Option<Range<u64>> {
    let time = u64::try_from(line.time).ok().filter(|_| line.lasts)?;
    Some(time * ticks..(time + 1) * ticks)
}

/// The rows of another track's phrase that start within these ticks of its row `00`, at its own
/// step of `ticks`, given when each row starts: the first, and how many. A skipped row doesn't
/// play, so it doesn't count.
pub fn starting_in(
    steps: &[Step],
    starts: &[usize],
    ticks: u64,
    within: &Range<u64>,
) -> Option<(usize, usize)> {
    let tick = |row: usize| starts[row] as u64 * ticks;
    let from = starts.partition_point(|start| (*start as u64 * ticks) < within.start);
    let mut found = (from..starts.len())
        .take_while(|row| tick(*row) < within.end)
        .filter(|row| steps[*row].pulses() > 0);
    let first = found.next()?;
    Some((first, 1 + found.count()))
}

impl App {
    /// Whether the phrase view is laid out in time rather than collapsed to a line a row.
    pub fn timeline_on(&self) -> bool {
        !self.rows_collapsed
    }

    /// Collapse the phrase view to a line a row, or open it out into its timeline again.
    pub(crate) fn toggle_timeline(&mut self) {
        self.count = None;
        self.rows_collapsed = !self.rows_collapsed;
        self.status = if self.rows_collapsed {
            "Rows: a line each, beside the other tracks' rows. z i lays them out in time"
        } else {
            "Timeline: a line for each pulse, beside the other tracks in time. z i collapses it"
        }
        .to_owned();
    }

    /// Show the row inspector, or hide it.
    pub(crate) fn toggle_inspector(&mut self) {
        self.count = None;
        self.inspector = !self.inspector;
        self.status = if self.inspector {
            "Row inspector: the cursor's row in words, while there's room. Space i hides it"
        } else {
            "Row inspector hidden. Space i shows it"
        }
        .to_owned();
    }

    /// How many lines each stored row of the cursor's phrase takes on screen.
    pub(crate) fn row_line_counts(&self) -> Vec<usize> {
        let timeline = self.timeline_on();
        self.phrase_here().map_or_else(Vec::new, |phrase| {
            (0..phrase.steps().len())
                .map(|index| row_lines(phrase, index, timeline))
                .collect()
        })
    }
}

/// The lines these rows take, a row past the counts taking one.
pub(crate) fn lines_over(counts: &[usize], rows: Range<usize>) -> usize {
    rows.map(|row| counts.get(row).copied().unwrap_or(1)).sum()
}

/// The row whose lines hold this line, counting from the first line of row `top`.
pub(crate) fn row_at_line(counts: &[usize], top: usize, line: usize) -> usize {
    let mut row = top;
    let mut passed = 0;
    while row < counts.len() && passed + counts[row] <= line {
        passed += counts[row];
        row += 1;
    }
    row
}

/// The first row that leaves this many lines or fewer from it to before row `end`, but no further
/// back than any row would.
pub(crate) fn row_lines_before(counts: &[usize], end: usize, lines: usize) -> usize {
    let mut row = end.min(counts.len());
    let mut taken = 0;
    while row > 0 && taken + counts[row - 1] <= lines {
        row -= 1;
        taken += counts[row];
    }
    row
}

/// How many rows from `from`, down or up, it takes to pass this many lines, as far as there are
/// rows.
pub(crate) fn rows_over(counts: &[usize], from: usize, lines: usize, down: bool) -> usize {
    let mut rows = 0;
    let mut passed = 0;
    while passed < lines {
        let next = if down {
            from + rows
        } else {
            match from.checked_sub(rows + 1) {
                Some(row) => row,
                None => break,
            }
        };
        let Some(count) = counts.get(next) else {
            break;
        };
        passed += count;
        rows += 1;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::step::{Gate, GateKind};

    fn step(pulses: u8) -> Step {
        Step {
            gate: Some(Gate {
                kind: GateKind::Single,
                pulses,
                length: None,
            }),
            ..Step::default()
        }
    }

    #[test]
    fn starts_add_up_the_pulses_before() {
        let steps = [step(2), step(0), step(3), step(1)];
        assert_eq!(starts(&steps), vec![0, 2, 2, 5]);
    }

    #[test]
    fn a_line_finds_the_first_row_to_start_in_it_and_how_many_do() {
        // A track at twice the speed: each of its rows of one pulse is half a line.
        let steps = [step(1), step(1), step(2), step(0), step(1)];
        let starts = starts(&steps);
        assert_eq!(starting_in(&steps, &starts, 120, &(0..240)), Some((0, 2)));
        // The skipped row starts with the next, and doesn't count.
        assert_eq!(starting_in(&steps, &starts, 120, &(240..480)), Some((2, 1)));
        assert_eq!(starting_in(&steps, &starts, 120, &(480..720)), Some((4, 1)));
        assert_eq!(starting_in(&steps, &starts, 120, &(720..960)), None);
    }

    #[test]
    fn rows_over_counts_rows_until_the_lines_are_passed() {
        let counts = [1, 3, 1, 4];
        assert_eq!(rows_over(&counts, 0, 3, true), 2);
        assert_eq!(rows_over(&counts, 3, 2, false), 2);
        assert_eq!(
            rows_over(&counts, 1, 9, false),
            1,
            "only as far as there are rows"
        );
        assert_eq!(row_at_line(&counts, 0, 3), 1);
        assert_eq!(row_at_line(&counts, 0, 4), 2);
        assert_eq!(row_lines_before(&counts, 4, 5), 2);
    }
}
