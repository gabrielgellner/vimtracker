//! What the DSP tests play and measure: a sine, and how loud a sound is or how far apart two are.
//! A test's own signals, such as a sweep, a saw or a burst of noise, stay in its file.

use std::borrow::Borrow;
use std::f32::consts::TAU;

/// A sine at `hertz` and `level`, at a frame.
pub(crate) fn sine(frame: usize, hertz: f32, level: f32, rate: u32) -> f32 {
    (frame as f32 / rate as f32 * hertz * TAU).sin() * level
}

/// So many frames of a sine at `hertz` and `level`, from the first.
pub(crate) fn sines(frames: usize, hertz: f32, level: f32, rate: u32) -> impl Iterator<Item = f32> {
    (0..frames).map(move |frame| sine(frame, hertz, level, rate))
}

/// The root of the mean of the squares: how loud a sound is on average.
pub(crate) fn rms(samples: impl IntoIterator<Item = impl Borrow<f32>>) -> f32 {
    let (sum, count) = samples.into_iter().fold((0.0, 0), |(sum, count), s| {
        let s = *s.borrow();
        (sum + s * s, count + 1)
    });
    (sum / count as f32).sqrt()
}

/// The loudest a sound gets, either side of zero.
pub(crate) fn peak(samples: impl IntoIterator<Item = impl Borrow<f32>>) -> f32 {
    samples
        .into_iter()
        .fold(0.0, |most: f32, s| most.max(s.borrow().abs()))
}

/// How far apart two sounds are, frame by frame, as the RMS of what's left when one is taken
/// from the other.
pub(crate) fn differs(a: &[f32], b: &[f32]) -> f32 {
    let total: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
    (total / a.len() as f32).sqrt()
}
