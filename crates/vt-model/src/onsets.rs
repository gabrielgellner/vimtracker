//! Where the sounds in a file start: an onset function and its peaks, worked out once with the
//! rest of a file's analysis and kept with it.
//!
//! The onset function is the spectral flux of Bello et al., "A Tutorial on Onset Detection in
//! Music Signals" (IEEE TSAP 2005): how much each bin of a short spectrum grew since the last, on a
//! log scale, so a quiet hit counts as well as a loud one, summed over the bins. Its peaks over a
//! median of their neighbours are where sounds start, each placed to the frame where the attack
//! rises out of what was sounding before. How far a peak stands above its median is its strength,
//! which the entry's `Sens` keeps or drops, so changing it needs nothing worked out again.
//!
//! The Deluge's perc measure, one byte of "percussiveness" for each 128 frames, is a different
//! thing: the slope of a smoothed first difference against its level, which steers its stretcher's
//! grains rather than marking where sounds start (decision 226).

use std::ops::Range;

use realfft::RealFftPlanner;

use crate::analysis::{Pyramid, SILENCE_DB};

/// How many frames apart the onset function's values are: one for each of the pyramid's finest
/// buckets, about 6 ms at 44.1 kHz.
pub const ONSET_HOP: usize = 256;

/// How many frames each spectrum is of, centred on its value's frame: about 23 ms at 44.1 kHz.
const WINDOW: usize = 1024;

/// How much the log scale lifts quiet bins, as `ln(1 + COMPRESSION * magnitude)`, where a sine at
/// full scale has a magnitude of 1.
const COMPRESSION: f32 = 100.0;

/// A peak is the highest value this long either side of it.
const PEAK_SECONDS: f64 = 0.03;

/// The median a peak must stand above reaches this long before it and after it.
const MEDIAN_BEFORE: f64 = 0.1;
const MEDIAN_AFTER: f64 = 0.07;

/// No two onsets are closer than this: the weaker goes. It's a 1/32 at 240 BPM, so it keeps the
/// hits of a drum roll and drops the ripples of one hit.
pub const MIN_GAP_SECONDS: f64 = 0.03;

/// The weakest peak kept at all, as a share of the onset function's highest value, which only
/// the most sensitive `Sens` comes near.
const WEAKEST: f32 = 0.0005;

/// `Sens` goes from 0, the strongest onsets only, to this, nearly every peak.
pub const MOST_SENS: u8 = 100;

/// `Sens` as a new entry has it.
pub const DEFAULT_SENS: u8 = 50;

/// A sound has decayed where its level has fallen this far, in dB, below its peak, or into
/// silence.
pub const DECAY_DB: f32 = 30.0;

/// Placing an onset to the frame: the attack is looked for in blocks of this many frames, each
/// against the loudest of the frames before it.
const BLOCK: usize = 16;
const CONTEXT_BLOCKS: usize = 8;

/// Where one sound starts, and how strong its onset is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onset {
    /// The frame its attack rises from, at the file's own rate.
    pub frame: u64,
    /// How far its peak in the onset function stands above the median around it, as a share of
    /// the function's highest value, from 0 to 1. `Sens` keeps those above
    /// [`sens_threshold`].
    pub strength: f32,
}

/// The least strength an onset needs to count at a `Sens`: 1, the strongest alone, at 0, falling
/// by a thousand times to [`MOST_SENS`], so each step of 10 is the same ratio.
pub fn sens_threshold(sens: u8) -> f32 {
    let sens = f32::from(sens.min(MOST_SENS)) / f32::from(MOST_SENS);
    10_f32.powf(-3.0 * sens)
}

/// The frames of interleaved samples in this many channels, as one channel, their mean.
fn mono(samples: &[f32], channels: usize) -> Vec<f32> {
    let scale = 1.0 / channels as f32;
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() * scale)
        .collect()
}

/// The onset function of interleaved samples: one value every [`ONSET_HOP`] frames, from the
/// first frame, each how much the spectrum centred there grew from the one before.
pub fn onset_function(samples: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let mono = mono(samples, channels);
    let hops = mono.len().div_ceil(ONSET_HOP);
    if hops == 0 {
        return Vec::new();
    }
    let window: Vec<f32> = (0..WINDOW)
        .map(|at| {
            let phase = std::f32::consts::PI * at as f32 / WINDOW as f32;
            phase.sin().powi(2)
        })
        .collect();
    // A sine's peak in the spectrum is half the window's sum times its amplitude.
    let scale = 2.0 / window.iter().sum::<f32>();
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    let mut last = vec![0.0_f32; spectrum.len()];
    let mut flux = Vec::with_capacity(hops);
    for hop in 0..hops {
        let first = (hop * ONSET_HOP) as isize - (WINDOW / 2) as isize;
        for (at, (sample, weight)) in input.iter_mut().zip(&window).enumerate() {
            let frame = first + at as isize;
            *sample = usize::try_from(frame)
                .ok()
                .and_then(|frame| mono.get(frame))
                .map_or(0.0, |value| value * weight);
        }
        // The lengths are the planner's own, so it can't fail.
        let _ = fft.process_with_scratch(&mut input, &mut spectrum, &mut scratch);
        let mut grew = 0.0;
        for (bin, before) in spectrum.iter().zip(last.iter_mut()) {
            let level = (1.0 + COMPRESSION * bin.norm() * scale).ln();
            grew += (level - *before).max(0.0);
            *before = level;
        }
        flux.push(grew);
    }
    flux
}

/// Where the sounds in interleaved samples start, at a rate, each with its strength, in order.
pub fn find_onsets(samples: &[f32], channels: usize, rate: u32) -> Vec<Onset> {
    let channels = channels.max(1);
    let flux = onset_function(samples, channels);
    let highest = flux.iter().copied().fold(0.0_f32, f32::max);
    if highest <= f32::EPSILON {
        return Vec::new();
    }
    let hops =
        |seconds: f64| ((seconds * f64::from(rate) / ONSET_HOP as f64).round() as usize).max(1);
    let (reach, before, after) = (hops(PEAK_SECONDS), hops(MEDIAN_BEFORE), hops(MEDIAN_AFTER));
    let mut window = Vec::with_capacity(before + after + 1);
    let mut onsets: Vec<Onset> = Vec::new();
    let gap = (MIN_GAP_SECONDS * f64::from(rate)).round() as u64;
    for (hop, value) in flux.iter().enumerate() {
        let near = hop.saturating_sub(reach)..(hop + reach + 1).min(flux.len());
        // The first of equal values is the peak, so a flat top gives one.
        let is_peak = near.clone().all(|other| match other.cmp(&hop) {
            std::cmp::Ordering::Less => flux[other] < *value,
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Greater => flux[other] <= *value,
        });
        if !is_peak {
            continue;
        }
        window.clear();
        window.extend_from_slice(
            &flux[hop.saturating_sub(before)..(hop + after + 1).min(flux.len())],
        );
        window.sort_unstable_by(f32::total_cmp);
        let median = window[window.len() / 2];
        let strength = (value - median) / highest;
        if strength < WEAKEST {
            continue;
        }
        let onset = Onset {
            frame: attack(samples, channels, hop),
            strength,
        };
        // Of two too close together, the stronger stays.
        match onsets.last_mut() {
            Some(last) if onset.frame < last.frame + gap => {
                if onset.strength > last.strength {
                    *last = onset;
                }
            }
            _ => onsets.push(onset),
        }
    }
    onsets
}

/// The frame an attack rises from, near a peak of the onset function. The spectrum centred on the
/// peak's frame grows most as the attack comes into its window's nearer half, so it's looked for
/// from a hop before the frame to a hop past the window's end: in the block of [`BLOCK`] frames
/// rising furthest above the loudest of the frames before it, among those near the loudest, at
/// the first frame louder than those before.
fn attack(samples: &[f32], channels: usize, hop: usize) -> u64 {
    let frames = samples.len() / channels;
    let level = |frame: usize| {
        samples[frame * channels..(frame + 1) * channels]
            .iter()
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    };
    let loudest = |frames: Range<usize>| frames.map(level).fold(0.0_f32, f32::max);
    let centre = hop * ONSET_HOP;
    let from = centre.saturating_sub(ONSET_HOP);
    let to = (centre + WINDOW / 2 + ONSET_HOP).min(frames);
    if from >= to {
        return centre.min(frames.saturating_sub(1)) as u64;
    }
    let blocks: Vec<(usize, f32, f32)> = (from..to)
        .step_by(BLOCK)
        .map(|start| {
            let context = start.saturating_sub(BLOCK * CONTEXT_BLOCKS)..start;
            (
                start,
                loudest(start..(start + BLOCK).min(to)),
                loudest(context),
            )
        })
        .collect();
    let top = blocks
        .iter()
        .map(|(_, level, _)| *level)
        .fold(0.0, f32::max);
    let floor = 10_f32.powf(SILENCE_DB / 20.0);
    let rise = |(_, level, before): &(usize, f32, f32)| (level - before) / (before + floor);
    let Some(&(start, _, before)) = blocks
        .iter()
        .filter(|(_, level, _)| *level >= top * 0.25)
        .max_by(|one, other| rise(one).total_cmp(&rise(other)))
    else {
        return from as u64;
    };
    let above = before.max(floor);
    (start..(start + BLOCK).min(to))
        .find(|frame| level(*frame) > above)
        .unwrap_or(start) as u64
}

/// The onsets at least as strong as a `Sens` keeps, as frames.
pub fn kept(onsets: &[Onset], sens: u8) -> impl Iterator<Item = u64> + '_ {
    let threshold = sens_threshold(sens);
    onsets
        .iter()
        .filter(move |onset| onset.strength >= threshold)
        .map(|onset| onset.frame)
}

/// Where a sound starting at a frame has decayed, before `until`, the next sound's start or the
/// end: the first of the pyramid's finest buckets after its loudest where it's fallen
/// [`DECAY_DB`] below that, or into silence, or `until` if it never does.
pub fn decay(pyramid: &Pyramid, start: u64, until: u64) -> u64 {
    let until = until.min(pyramid.frames());
    if start >= until {
        return until;
    }
    let size = Pyramid::bucket_frames(0);
    let bucket = |at: u64| (at * size).max(start)..((at + 1) * size).min(until);
    // The pyramid reads a stretch as the whole buckets it touches, so a bucket `until` cuts would
    // hear the next sound: the buckets go up to the last whole one.
    let buckets = start / size..(until / size).max(start / size + 1);
    let (loudest, peak) = buckets
        .clone()
        .map(|at| (at, pyramid.peak(bucket(at))))
        .fold((buckets.start, 0.0_f32), |most, this| {
            if this.1 > most.1 { this } else { most }
        });
    let quiet = (peak * 10_f32.powf(-DECAY_DB / 20.0)).max(10_f32.powf(SILENCE_DB / 20.0));
    (loudest + 1..buckets.end)
        .find(|at| pyramid.peak(bucket(*at)) < quiet)
        .map_or(until, |at| bucket(at).start.max(start + 1))
}

/// How far under a file's peak counts as the silence between sounds, for `:slice silence`, unless
/// that's under [`SILENCE_DB`], so a hiss under the hits is silence too.
pub const QUIET_UNDER_PEAK_DB: f32 = 48.0;

/// The shortest silence that parts two sounds, so a dip inside one doesn't.
pub const MIN_SILENCE_SECONDS: f64 = 0.05;

/// The level under which a file with this peak is silent between its sounds.
pub fn quiet_level(peak: f32) -> f32 {
    (peak * 10_f32.powf(-QUIET_UNDER_PEAK_DB / 20.0)).max(10_f32.powf(SILENCE_DB / 20.0))
}

/// Where sounds start inside a stretch after silence, to the pyramid's finest bucket: its first
/// bucket louder than `quiet`, and each after at least `gap` frames of buckets that aren't, as
/// each bucket's first frame, or the stretch's for the bucket it starts in.
pub fn after_silence(pyramid: &Pyramid, frames: Range<u64>, quiet: f32, gap: u64) -> Vec<u64> {
    let end = frames.end.min(pyramid.frames());
    let size = Pyramid::bucket_frames(0);
    let mut starts = Vec::new();
    // The quiet frames since the last loud bucket, which the stretch starts with plenty of.
    let mut silent = gap;
    for bucket in frames.start / size..end.div_ceil(size) {
        let from = (bucket * size).max(frames.start);
        let to = ((bucket + 1) * size).min(end);
        if pyramid.peak(bucket * size..(bucket + 1) * size) > quiet {
            if silent >= gap {
                starts.push(from);
            }
            silent = 0;
        } else {
            silent += to - from;
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Silence with a short burst of noise at each of these frames, at a level, in two channels.
    fn clicks(frames: usize, at: &[(usize, f32)]) -> Vec<f32> {
        let mut samples = vec![0.0_f32; frames * 2];
        let mut seed = 99_u32;
        for (start, level) in at {
            for frame in *start..(*start + 400).min(frames) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0;
                let fade = 1.0 - (frame - start) as f32 / 400.0;
                samples[frame * 2] = noise * level * fade;
                samples[frame * 2 + 1] = noise * level * fade * 0.5;
            }
        }
        samples
    }

    #[test]
    fn onsets_land_within_a_few_frames_of_each_click() {
        let hits = [
            (1000, 0.8),
            (12_345, 0.5),
            (24_000, 0.9),
            (30_001, 0.3),
            (47_777, 0.6),
        ];
        let samples = clicks(60_000, &hits);
        let onsets = find_onsets(&samples, 2, 44_100);
        let found: Vec<u64> = kept(&onsets, DEFAULT_SENS).collect();
        assert_eq!(found.len(), hits.len(), "{onsets:?}");
        for ((hit, _), frame) in hits.iter().zip(&found) {
            assert!(frame.abs_diff(*hit as u64) <= 4, "{hit} found at {frame}");
        }
    }

    #[test]
    fn sens_keeps_fewer_onsets_as_it_falls() {
        // Loud hits with quiet ghost notes between them.
        let mut hits = Vec::new();
        for beat in 0..8 {
            hits.push((2000 + beat * 11_025, 0.9));
            hits.push((2000 + beat * 11_025 + 5512, 0.03));
        }
        let samples = clicks(92_000, &hits);
        let onsets = find_onsets(&samples, 2, 44_100);
        let count = |sens| kept(&onsets, sens).count();
        assert_eq!(count(MOST_SENS), 16);
        assert_eq!(count(5), 8, "only the loud hits");
        assert!((0..MOST_SENS).all(|sens| count(sens) <= count(sens + 1)));
    }

    #[test]
    fn silence_has_no_onsets_and_onsets_are_never_too_close() {
        assert!(find_onsets(&vec![0.0; 20_000], 2, 48_000).is_empty());
        assert!(find_onsets(&[], 2, 48_000).is_empty());
        let samples = clicks(40_000, &[(5000, 0.5), (5300, 0.9), (20_000, 0.5)]);
        let onsets = find_onsets(&samples, 2, 48_000);
        let frames: Vec<u64> = kept(&onsets, MOST_SENS).collect();
        let gap = (MIN_GAP_SECONDS * 48_000.0) as u64;
        assert!(
            frames.windows(2).all(|pair| pair[1] - pair[0] >= gap),
            "{frames:?}"
        );
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn a_sound_decays_where_it_falls_far_below_its_peak() {
        // A hit decaying by 6 dB every 1000 frames, then silence: 30 dB below is 5000 frames on.
        let mut samples = vec![0.0_f32; 40_000];
        for (frame, sample) in samples.iter_mut().enumerate().skip(10_000) {
            let gain = 0.5_f32.powf((frame - 10_000) as f32 / 1000.0);
            *sample = if frame % 2 == 0 { gain } else { -gain };
        }
        let pyramid = Pyramid::build(&samples, 1);
        let end = decay(&pyramid, 10_000, 40_000);
        assert!((14_900..=15_400).contains(&end), "decays at {end}");
        // A sound that doesn't decay before the next lasts until it.
        assert_eq!(decay(&pyramid, 10_000, 12_000), 12_000);
        assert_eq!(decay(&pyramid, 40_000, 50_000), 40_000);
    }

    #[test]
    fn sounds_after_silence_start_at_their_bucket_and_a_short_dip_parts_nothing() {
        let mut samples = vec![0.0_f32; 30_000];
        // A hit, a dip of 1000 frames inside it, then silence, then two more.
        for (from, to) in [
            (1000, 3000),
            (4000, 6000),
            (12_000, 14_000),
            (20_100, 22_000),
        ] {
            samples[from..to].fill(0.5);
        }
        let pyramid = Pyramid::build(&samples, 1);
        let quiet = quiet_level(0.5);
        let gap = 2400;
        assert_eq!(
            after_silence(&pyramid, 0..30_000, quiet, gap),
            [768, 11_776, 19_968]
        );
        // A stretch starting in sound starts a sound.
        assert_eq!(
            after_silence(&pyramid, 5000..30_000, quiet, gap),
            [5000, 11_776, 19_968]
        );
        assert!(after_silence(&pyramid, 7000..11_000, quiet, gap).is_empty());
    }

    #[test]
    fn sens_is_a_threshold_from_the_strongest_to_nearly_every_peak() {
        assert_eq!(sens_threshold(0), 1.0);
        assert!((sens_threshold(MOST_SENS) - 0.001).abs() < 1e-6);
        assert!(sens_threshold(DEFAULT_SENS) > sens_threshold(DEFAULT_SENS + 1));
    }
}
