//! Decoding sample files into stereo `f32` at the rate they will play at.

use std::fs::File;
use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use thiserror::Error;

/// Input frames per resampler chunk.
const CHUNK: usize = 1024;

/// The file extensions a sample can have, in lower case.
pub const SAMPLE_EXTENSIONS: [&str; 4] = ["wav", "aif", "aiff", "flac"];

/// Why a sample could not be loaded.
#[derive(Debug, Error)]
pub enum SampleError {
    #[error("{path}: {source}")]
    Read { path: String, source: hound::Error },
    #[error("{path}: {detail}")]
    Decode { path: String, detail: String },
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

/// Whether a file's extension is one a sample can have.
pub fn is_sample_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        SAMPLE_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

/// Interleaved samples as a file holds them, before they are made stereo at the program's rate.
struct Decoded {
    samples: Vec<f32>,
    channels: u16,
    rate: u32,
}

/// What a sample file holds, as it was before it was made stereo at the program's rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleInfo {
    /// Frames a second.
    pub rate: u32,
    pub channels: u16,
    pub frames: usize,
}

impl SampleInfo {
    /// How long it plays, in seconds.
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.rate.max(1))
    }
}

/// Read a WAV, AIFF or FLAC file and convert it to stereo at `rate`. AIFF and FLAC are told
/// apart by their extension, and anything else is read as a WAV: by `hound`, as it always has
/// been, or by `symphonia` if `hound` can't read it.
pub fn load_sample(path: &Path, rate: u32) -> Result<SampleData, SampleError> {
    load_sample_with_info(path, rate).map(|(sample, _)| sample)
}

/// [`load_sample`], with what the file held: its own rate, its channels and its length.
pub fn load_sample_with_info(
    path: &Path,
    rate: u32,
) -> Result<(SampleData, SampleInfo), SampleError> {
    let name = || path.display().to_string();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    let decoded = match extension.as_deref() {
        Some(other @ ("aif" | "aiff" | "flac")) => decode_other(path, other),
        // `hound` doesn't skip the pad byte after a chunk of odd length before the data, as
        // eight of Ableton's WAVs have, and `symphonia` does.
        _ => decode_wav(path).or_else(|error| match error {
            SampleError::Read { .. } => decode_other(path, "wav").map_err(|_| error),
            error => Err(error),
        }),
    }?;
    let Decoded {
        samples,
        channels,
        rate: file_rate,
    } = decoded;
    let info = SampleInfo {
        rate: file_rate,
        channels,
        frames: samples.len() / usize::from(channels.max(1)),
    };
    let stereo: Vec<f32> = if channels == 1 {
        samples.iter().flat_map(|s| [*s, *s]).collect()
    } else {
        samples
    };
    let converted = if file_rate == rate {
        stereo
    } else {
        resample(&stereo, file_rate, rate).map_err(|detail| SampleError::Resample {
            path: name(),
            detail,
        })?
    };
    let sample = SampleData {
        frames: converted.as_chunks::<2>().0.to_vec(),
    };
    Ok((sample, info))
}

/// Read a WAV file with `hound`.
fn decode_wav(path: &Path) -> Result<Decoded, SampleError> {
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
    Ok(Decoded {
        samples,
        channels,
        rate: spec.sample_rate,
    })
}

/// Read a file with `symphonia`, which scales integers to floats as `hound` is made to above.
fn decode_other(path: &Path, extension: &str) -> Result<Decoded, SampleError> {
    let name = || path.display().to_string();
    let failed = |detail: &dyn std::fmt::Display| SampleError::Decode {
        path: name(),
        detail: detail.to_string(),
    };
    let file = File::open(path).map_err(|e| failed(&e))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| failed(&e))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| failed(&"no audio in the file"))?;
    let track_id = track.id;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(|| failed(&"no audio in the file"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .map_err(|e| failed(&e))?;
    let mut samples = Vec::new();
    let mut packet_samples: Vec<f32> = Vec::new();
    let mut shape = None;
    while let Some(packet) = format.next_packet().map_err(|e| failed(&e))? {
        if packet.track_id != track_id {
            continue;
        }
        let buffer = decoder.decode(&packet).map_err(|e| failed(&e))?;
        let spec = buffer.spec();
        let channels = u16::try_from(spec.channels().count()).unwrap_or(u16::MAX);
        if channels != 1 && channels != 2 {
            return Err(SampleError::Channels {
                path: name(),
                channels,
            });
        }
        shape = Some((channels, spec.rate()));
        buffer.copy_to_vec_interleaved(&mut packet_samples);
        samples.extend_from_slice(&packet_samples);
    }
    let (channels, rate) = shape.ok_or_else(|| failed(&"no audio in the file"))?;
    Ok(Decoded {
        samples,
        channels,
        rate,
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
