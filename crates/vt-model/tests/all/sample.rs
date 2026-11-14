//! AIFF and FLAC load to the same frames as the same sound in a WAV.
//!
//! The files in `samples/` were made from the WAVs: the AIFFs with macOS's `afconvert`, and the
//! FLACs with `ffmpeg` in 256-frame blocks so that each takes several packets. Each WAV is a
//! decaying chirp, different on each side, that starts on the lowest value and then the highest.

use std::path::{Path, PathBuf};

use vt_model::sample::{SampleData, is_sample_file, load_sample};

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/samples")
        .join(name)
}

fn load(name: &str, rate: u32) -> SampleData {
    load_sample(&sample(name), rate).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn a_16_bit_stereo_aiff_and_flac_match_the_wav() {
    let wav = load("stereo-16.wav", 48_000);
    assert_eq!(wav.frames.len(), 1200);
    assert_eq!(wav.frames[0], [-1.0, -1.0]);
    assert_ne!(wav.frames[10][0], wav.frames[10][1]);
    assert_eq!(load("stereo-16.aif", 48_000), wav);
    assert_eq!(load("stereo-16.flac", 48_000), wav);
}

#[test]
fn a_24_bit_mono_aiff_and_flac_match_the_wav() {
    let wav = load("mono-24.wav", 44_100);
    assert_eq!(wav.frames.len(), 1200);
    assert_eq!(wav.frames[0], [-1.0, -1.0]);
    assert_eq!(wav.frames[1][0], 8_388_607.0 / 8_388_608.0);
    assert_eq!(load("mono-24.aif", 44_100), wav);
    assert_eq!(load("mono-24.flac", 44_100), wav);
}

#[test]
fn an_aiff_and_a_flac_convert_their_rate_as_a_wav_does() {
    let wav = load("mono-24.wav", 48_000);
    let length = wav.frames.len() as i64;
    assert!((length - 1306).abs() <= 2, "got {length} frames");
    assert_eq!(load("mono-24.aif", 48_000), wav);
    assert_eq!(load("mono-24.flac", 48_000), wav);
}

#[test]
fn a_wav_with_an_odd_chunk_before_its_data_still_loads() {
    // Eight of Ableton's WAVs have a 7-byte `inst` chunk before the data, and its pad byte,
    // which `hound` doesn't skip, so it reads the data's header a byte early.
    let bytes = std::fs::read(sample("stereo-16.wav")).unwrap();
    let data = 36;
    assert_eq!(&bytes[data..data + 4], b"data");
    let mut odd = bytes[..data].to_vec();
    odd.extend_from_slice(b"inst");
    odd.extend_from_slice(&7_u32.to_le_bytes());
    odd.extend_from_slice(&[60, 0, 24, 0, 127, 1, 127, 0]);
    odd.extend_from_slice(&bytes[data..]);
    let riff = u32::try_from(odd.len() - 8).unwrap();
    odd[4..8].copy_from_slice(&riff.to_le_bytes());
    let path = std::env::temp_dir().join(format!("vt-model-{}-odd.wav", std::process::id()));
    std::fs::write(&path, &odd).unwrap();
    assert!(hound::WavReader::open(&path).is_err());
    assert_eq!(
        load_sample(&path, 48_000).unwrap(),
        load("stereo-16.wav", 48_000)
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_extension_is_not_case_sensitive() {
    for name in ["kick.wav", "kick.WAV", "kick.aif", "kick.AIFF", "kick.flac"] {
        assert!(is_sample_file(Path::new(name)), "{name}");
    }
    for name in ["kick.mp3", "kick.ron", "kick"] {
        assert!(!is_sample_file(Path::new(name)), "{name}");
    }
}

#[test]
fn a_file_that_is_not_audio_names_its_path() {
    let path = std::env::temp_dir().join(format!("vt-model-{}-noise.flac", std::process::id()));
    std::fs::write(&path, b"not a flac file").unwrap();
    let error = load_sample(&path, 48_000).unwrap_err().to_string();
    assert!(error.contains("noise.flac"), "{error}");
    let _ = std::fs::remove_file(&path);
}
