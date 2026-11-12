//! Seeds from the file (#279): a WAV's cue points become slices, its `smpl` loop the loop and its
//! root note a hint, and an Octatrack `.ot` beside it gives its trim, loop, gain, tempo and
//! slices. Every file here is made by the test, in a folder of its own.

use std::path::{Path, PathBuf};

use vt_model::bank::{Loop, Points};
use vt_model::octatrack::{OtFile, OtLoop, OtSlice};
use vt_model::riff::{Cue, LoopKind, Markers, SampleLoop, with_markers};
use vt_model::sample::load_sample;
use vt_model::seeding::{FileSeed, seed};

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("vt-seeding-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// The bytes of a 16-bit mono WAV of this many frames, as `hound` writes one.
fn plain_wav(frames: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for at in 0..frames {
        writer.write_sample((at % 100) as i16 * 100).unwrap();
    }
    writer.finalize().unwrap();
    bytes.into_inner()
}

fn cue(id: u32, frame: u64, label: Option<&str>) -> Cue {
    Cue {
        id,
        frame,
        label: label.map(str::to_owned),
    }
}

/// Write a WAV with these markers, and what it seeds.
fn seeded(folder: &Path, name: &str, frames: u32, markers: &Markers) -> (PathBuf, FileSeed) {
    let path = folder.join(name);
    let bytes = with_markers(&plain_wav(frames), markers).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let seed = seed(&path, &bytes, 0);
    (path, seed)
}

#[test]
fn a_sliced_wavs_cue_points_are_its_slices() {
    let folder = folder("cues");
    let markers = Markers {
        cues: [0, 2500, 5000, 7500]
            .into_iter()
            .enumerate()
            .map(|(at, frame)| cue(at as u32 + 1, frame, None))
            .collect(),
        ..Markers::default()
    };
    let (path, seed) = seeded(&folder, "chain.wav", 10_000, &markers);
    let starts: Vec<u64> = seed.points.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(starts, [0, 2500, 5000, 7500]);
    assert_eq!(seed.points.loop_, None);
    assert_eq!(seed.points.end, None);
    assert_eq!(seed.root, None);
    assert_eq!(seed.describe(), "4 slices, from its markers");
    assert!(seed.problems.is_empty());
    assert_eq!(load_sample(&path, 44_100).unwrap().frames.len(), 10_000);
}

#[test]
fn a_slice_starts_at_the_start_and_a_one_letter_label_is_a_mark() {
    let folder = folder("labels");
    let markers = Markers {
        cues: vec![
            cue(1, 3000, Some("a")),
            cue(2, 6000, Some("Snare")),
            cue(3, 9000, Some("K")),
            cue(4, 20_000, None),
        ],
        ..Markers::default()
    };
    let (_, seed) = seeded(&folder, "marked.wav", 10_000, &markers);
    let starts: Vec<u64> = seed.points.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(starts, [0, 3000, 6000, 9000], "past the end is left out");
    assert_eq!(seed.points.marked('a'), Some(1));
    assert_eq!(seed.points.marked('k'), Some(3));
    assert_eq!(seed.points.marks.len(), 2, "a longer label isn't a mark");
}

#[test]
fn a_smpl_loop_is_the_loop_and_its_root_a_hint() {
    let folder = folder("loop");
    let markers = Markers {
        loops: vec![SampleLoop {
            start: 2000,
            end: 4000,
            kind: LoopKind::Forward,
        }],
        root: Some(48),
        ..Markers::default()
    };
    let (_, seed) = seeded(&folder, "pad.wav", 10_000, &markers);
    assert_eq!(
        seed.points,
        Points {
            loop_: Some(Loop {
                start: 2000,
                end: 4000,
                crossfade: 0
            }),
            ..Points::default()
        },
        "a loop shorter than what follows keeps the tail"
    );
    assert_eq!(seed.root, Some(48));
    assert_eq!(seed.describe(), "a loop and its root C-2, from its markers");
}

#[test]
fn a_loop_as_long_as_what_follows_is_a_trim_too() {
    let folder = folder("trim");
    let markers = Markers {
        loops: vec![SampleLoop {
            start: 1000,
            end: 6000,
            kind: LoopKind::PingPong,
        }],
        ..Markers::default()
    };
    let (_, seed) = seeded(&folder, "held.wav", 10_000, &markers);
    assert_eq!(seed.points.end, Some(6000));
    assert_eq!(
        seed.points.loop_.map(|l| (l.start, l.end)),
        Some((1000, 6000))
    );
    let whole = Markers {
        loops: vec![SampleLoop {
            start: 0,
            end: 10_000,
            kind: LoopKind::Forward,
        }],
        ..Markers::default()
    };
    let (_, seed) = seeded(&folder, "whole.wav", 10_000, &whole);
    assert_eq!(seed.points.end, None, "a loop to the end needs no end");
}

#[test]
fn cues_and_a_loop_together() {
    let folder = folder("both");
    let markers = Markers {
        cues: vec![cue(1, 0, None), cue(2, 4000, None), cue(3, 8000, None)],
        loops: vec![SampleLoop {
            start: 4000,
            end: 8000,
            kind: LoopKind::Forward,
        }],
        root: Some(60),
    };
    let (_, seed) = seeded(&folder, "both.wav", 12_000, &markers);
    assert_eq!(seed.points.end, Some(8000));
    assert_eq!(
        seed.points.slices.len(),
        2,
        "a cue at the end starts nothing"
    );
    assert_eq!(
        seed.points.loop_.map(|l| (l.start, l.end)),
        Some((4000, 8000))
    );
    assert_eq!(seed.root, Some(60));
    assert_eq!(
        seed.describe(),
        "2 slices, an end, a loop and its root C-3, from its markers"
    );
}

#[test]
fn an_ot_beside_the_file_gives_its_trim_loop_gain_tempo_and_slices() {
    let folder = folder("ot");
    let markers = Markers {
        cues: vec![cue(1, 0, None), cue(2, 5000, None)],
        root: Some(36),
        ..Markers::default()
    };
    let (path, _) = seeded(&folder, "break.wav", 20_000, &markers);
    let ot = OtFile {
        loop_mode: OtLoop::On,
        gain: 54,
        trim_start: 1000,
        trim_end: 19_000,
        loop_start: 10_000,
        slices: vec![
            OtSlice {
                start: 1000,
                end: 6000,
                loop_start: None,
            },
            OtSlice {
                start: 6000,
                end: 19_000,
                loop_start: None,
            },
            OtSlice {
                start: 4000,
                end: 8000,
                loop_start: Some(5000),
            },
        ],
        ..OtFile::new(20_000, 90)
    };
    std::fs::write(folder.join("break.ot"), ot.to_bytes()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let seed = seed(&path, &bytes, 0);
    let starts: Vec<u64> = seed.points.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(starts, [1000, 4000, 6000], "the .ot's slices, in order");
    assert_eq!(seed.points.start, 1000);
    assert_eq!(seed.points.end, Some(19_000));
    assert_eq!(
        seed.points.loop_.map(|l| (l.start, l.end)),
        Some((10_000, 19_000))
    );
    assert_eq!(seed.gain_db, Some(3.0));
    assert_eq!(seed.millibpm, Some(90_000));
    assert_eq!(seed.root, Some(36), "the root still comes from the WAV");
    assert_eq!(
        seed.describe(),
        "3 slices, a start and end, a loop, a gain of +3.0 dB, its root C-1 and 90 BPM, \
         from break.ot and its markers"
    );
    let beside: Vec<_> = std::fs::read_dir(&folder).unwrap().collect();
    assert_eq!(beside.len(), 2, "nothing is written beside them");
}

#[test]
fn a_damaged_chunk_or_ot_is_skipped_and_said_and_the_audio_still_loads() {
    let folder = folder("damaged");
    let mut bytes = plain_wav(1000);
    let mut chunk = b"cue ".to_vec();
    chunk.extend_from_slice(&28_u32.to_le_bytes());
    chunk.extend_from_slice(&9_u32.to_le_bytes());
    chunk.extend_from_slice(&[0; 24]);
    bytes.extend_from_slice(&chunk);
    let size = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let path = folder.join("odd.wav");
    std::fs::write(&path, &bytes).unwrap();
    let mut ot = OtFile::new(1000, 120).to_bytes();
    ot[0x40] ^= 0xFF;
    std::fs::write(folder.join("odd.ot"), ot).unwrap();
    let seed = seed(&path, &bytes, 0);
    assert!(seed.is_empty(), "{seed:?}");
    assert_eq!(
        seed.problems,
        [
            "its `cue ` chunk is malformed, so it's skipped",
            "the odd.ot beside it is skipped: its checksum is wrong"
        ]
    );
    assert_eq!(load_sample(&path, 44_100).unwrap().frames.len(), 1000);
}
