//! How the scope follows the main output: what it keeps of the sound, where it starts its picture,
//! and how the samples fold down to columns, with no drawing.
//!
//! It's triggered, as a hardware scope and the M8's are: the picture starts where the sound last
//! rose through zero, so a steady note stands still rather than smearing across.

/// Tuning: how much of the sound the scope shows, in milliseconds.
const WINDOW_MS: u32 = 20;
/// How many windows of sound are kept, so a crossing can be found for a note as low as about
/// 1000 / (`WINDOW_MS` * (`HISTORY_WINDOWS` - 1)) Hz, 17 Hz.
const HISTORY_WINDOWS: usize = 4;
/// Tuning: how far below zero the sound has to go, as a share of its peak, before rising through
/// zero counts. It keeps a small wiggle near zero from moving the picture.
const HYSTERESIS: f32 = 0.05;

/// The main output's recent sound, mixed to one channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    /// The newest sound, oldest first, always `HISTORY_WINDOWS` windows long.
    history: Vec<f32>,
    window: usize,
}

impl Scope {
    /// Silence, at this sample rate.
    pub fn new(rate: u32) -> Self {
        let window = (rate * WINDOW_MS / 1000).max(1) as usize;
        Self {
            history: vec![0.0; window * HISTORY_WINDOWS],
            window,
        }
    }

    /// The samples the picture shows, starting from the newest rise through zero that leaves a
    /// whole window after it, or the newest window if there isn't one.
    pub fn shown(&self) -> &[f32] {
        let start = trigger(&self.history, self.window).unwrap_or(self.history.len() - self.window);
        &self.history[start..start + self.window]
    }

    /// The rows the picture covers in each of `columns` columns, as the track page's curves are
    /// given, with full level at the top row and silence half way down.
    pub fn spans(&self, columns: usize, rows: usize) -> Vec<(usize, usize)> {
        rows_of(&fold(self.shown(), columns), rows)
    }
}

impl Extend<f32> for Scope {
    /// Add the newest sound, letting go of as much of the oldest.
    fn extend<T: IntoIterator<Item = f32>>(&mut self, samples: T) {
        let kept = self.history.len();
        self.history.extend(samples);
        self.history.drain(..self.history.len() - kept);
    }
}

/// Where the newest rise through zero is that has `window` samples from it to the end: a sample
/// at or above zero after one below it, once the sound has been far enough below zero.
pub(crate) fn trigger(samples: &[f32], window: usize) -> Option<usize> {
    let last = samples.len().checked_sub(window)?;
    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak == 0.0 {
        return None;
    }
    let below = -peak * HYSTERESIS;
    let mut armed = false;
    let mut found = None;
    for (at, sample) in samples.iter().enumerate().take(last + 1) {
        if *sample < below {
            armed = true;
        } else if armed && *sample >= 0.0 {
            found = Some(at);
            armed = false;
        }
    }
    found
}

/// The rows each column's lowest and highest sample cover, top first, with full level at the top
/// row and silence half way down.
pub(crate) fn rows_of(folded: &[(f32, f32)], rows: usize) -> Vec<(usize, usize)> {
    let bottom = (rows - 1) as f32;
    let row_of = |sample: f32| ((1.0 - sample.clamp(-1.0, 1.0)) * 0.5 * bottom).round() as usize;
    folded
        .iter()
        .map(|(lowest, highest)| (row_of(*highest), row_of(*lowest)))
        .collect()
}

/// The lowest and highest sample in each of `columns` columns. Each column also takes in the first
/// sample of the next, so the line joins up from one to the next.
pub(crate) fn fold(samples: &[f32], columns: usize) -> Vec<(f32, f32)> {
    let count = samples.len();
    (0..columns)
        .map(|column| {
            let start = (column * count / columns).min(count - 1);
            let end = ((column + 1) * count / columns + 1).clamp(start + 1, count);
            samples[start..end]
                .iter()
                .fold((f32::MAX, f32::MIN), |(lowest, highest), sample| {
                    (lowest.min(*sample), highest.max(*sample))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency: f32, phase: f32, count: usize) -> Vec<f32> {
        (0..count)
            .map(|at| {
                (std::f32::consts::TAU * (frequency * at as f32 / 48_000.0) + phase).sin() * 0.8
            })
            .collect()
    }

    #[test]
    fn the_trigger_finds_the_newest_rise_with_room_after_it() {
        // 100 Hz is 480 samples a cycle, and a phase of a half cycle puts the rises at 240, 720
        // and so on.
        let samples = sine(100.0, std::f32::consts::PI, 1920);
        let at = trigger(&samples, 960).unwrap();
        assert!(at <= 960, "{at}");
        assert!(samples[at - 1] < 0.0 && samples[at] >= 0.0, "{at}");
        assert!((at as i32 - 720).abs() <= 1, "{at}");
    }

    #[test]
    fn a_falling_crossing_and_a_wiggle_near_zero_dont_count() {
        let mut samples = vec![0.5; 100];
        samples.extend([-0.5; 100]);
        assert_eq!(trigger(&samples, 50), None, "only a fall");
        // A wiggle just under zero, much smaller than the peak, before the real rise.
        let mut samples = vec![0.0; 10];
        samples.extend([-0.01, 0.01, -0.01, 0.01]);
        samples.extend([-1.0, -0.5, 0.5, 1.0]);
        samples.extend([0.0; 20]);
        assert_eq!(trigger(&samples, 20), Some(16));
    }

    #[test]
    fn silence_and_too_little_sound_have_no_trigger() {
        assert_eq!(trigger(&[0.0; 100], 50), None);
        assert_eq!(trigger(&[-1.0, 1.0], 50), None);
    }

    #[test]
    fn a_steady_note_stands_still_however_much_arrives() {
        let mut scope = Scope::new(48_000);
        let note = sine(110.0, 0.3, 48_000);
        let mut pictures = Vec::new();
        for frame in note.chunks(1600) {
            scope.extend(frame.iter().copied());
            pictures.push(scope.spans(72, 36));
        }
        // Each starts on a rise, and so they're all much the same, give or take a dot.
        let first = &pictures[4];
        for picture in &pictures[5..] {
            let moved = picture
                .iter()
                .zip(first)
                .filter(|(now, then)| now.0.abs_diff(then.0) > 1 || now.1.abs_diff(then.1) > 1)
                .count();
            assert!(moved <= 2, "{moved} columns moved");
        }
    }

    #[test]
    fn folding_keeps_each_columns_lowest_and_highest_and_joins_them() {
        let samples = [0.0, 0.5, -0.5, 1.0, 0.25, -1.0, 0.0, 0.0];
        assert_eq!(
            fold(&samples, 4),
            [(-0.5, 0.5), (-0.5, 1.0), (-1.0, 0.25), (0.0, 0.0)]
        );
        // More columns than samples still gives each column something.
        let folded = fold(&[0.5, -0.5], 4);
        assert_eq!(folded.len(), 4);
        assert!(folded.iter().all(|(lowest, highest)| lowest <= highest));
    }

    #[test]
    fn silence_is_a_flat_line_half_way_down() {
        let scope = Scope::new(48_000);
        assert_eq!(scope.shown().len(), 960);
        let spans = scope.spans(72, 37);
        assert_eq!(spans.len(), 72);
        assert!(spans.iter().all(|span| *span == (18, 18)));
    }

    #[test]
    fn full_level_reaches_the_top_and_bottom_rows() {
        let mut scope = Scope::new(48_000);
        scope.extend(sine(400.0, 0.0, 4000).iter().map(|sample| sample * 1.25));
        let spans = scope.spans(72, 36);
        assert_eq!(spans.iter().map(|span| span.0).min(), Some(0));
        assert_eq!(spans.iter().map(|span| span.1).max(), Some(35));
    }
}
