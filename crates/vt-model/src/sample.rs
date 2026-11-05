//! Decoding sample files into stereo `f32` at the rate they will play at.

use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use thiserror::Error;

/// Input frames per resampler chunk.
const CHUNK: usize = 1024;

/// Why a sample could not be loaded.
#[derive(Debug, Error)]
pub enum SampleError {
    #[error("{path}: {source}")]
    Read { path: String, source: hound::Error },
    #[error("{path}: only 16-bit, 24-bit and 32-bit float WAV files are supported")]
    Format { path: String },
    #[error("{path}: only mono and stereo files are supported, not {channels} channels")]
    Channels { path: String, channels: u16 },
    #[error("{path}: could not convert the sample rate: {detail}")]
    Resample { path: String, detail: String },
}

/// A decoded sample, as interleaved stereo frames.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    pub frames: Vec<[f32; 2]>,
}

/// Read a WAV file and convert it to stereo at `rate`.
pub fn load_wav(path: &Path, rate: u32) -> Result<SampleData, SampleError> {
    let name = || path.display().to_string();
    let read = |source| SampleError::Read {
        path: name(),
        source,
    };
    let mut reader = hound::WavReader::open(path).map_err(read)?;
    let spec = reader.spec();
    let channels = spec.channels;
    if channels != 1 && channels != 2 {
        return Err(SampleError::Channels {
            path: name(),
            channels,
        });
    }
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|s| f32::from(s) / 32_768.0))
            .collect::<Result<_, _>>()
            .map_err(read)?,
        (hound::SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .map(|s| s.map(|s| s as f32 / 8_388_608.0))
            .collect::<Result<_, _>>()
            .map_err(read)?,
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(read)?,
        _ => return Err(SampleError::Format { path: name() }),
    };
    let stereo: Vec<f32> = if channels == 1 {
        samples.iter().flat_map(|s| [*s, *s]).collect()
    } else {
        samples
    };
    let converted = if spec.sample_rate == rate {
        stereo
    } else {
        resample(&stereo, spec.sample_rate, rate).map_err(|detail| SampleError::Resample {
            path: name(),
            detail,
        })?
    };
    Ok(SampleData {
        frames: converted.as_chunks::<2>().0.to_vec(),
    })
}

/// Convert interleaved stereo in one go, trimming the filter's delay so transients stay on
/// their sample.
fn resample(input: &[f32], from: u32, to: u32) -> Result<Vec<f32>, String> {
    let text = |error: &dyn std::fmt::Display| error.to_string();
    let frames = input.len() / 2;
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, CHUNK, 2, FixedSync::Input)
        .map_err(|e| text(&e))?;
    let needed = resampler.process_all_needed_output_len(frames);
    let mut output = vec![0.0; needed * 2];
    let source = InterleavedSlice::new(input, 2, frames).map_err(|e| text(&e))?;
    let mut target = InterleavedSlice::new_mut(&mut output, 2, needed).map_err(|e| text(&e))?;
    let (_, written) = resampler
        .process_all_into_buffer(&source, &mut target, frames, None)
        .map_err(|e| text(&e))?;
    output.truncate(written * 2);
    Ok(output)
}
