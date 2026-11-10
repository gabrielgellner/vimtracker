//! How long a 5-minute stereo file's analysis takes to build, keep and read back, and how long a
//! full-width view of it takes at several zooms:
//!
//! `cargo run --release -p vt-model --example analysis_bench [cache folder]`
//!
//! The cache file is written into the folder given, or the system's temporary folder.

use std::hint::black_box;
use std::time::{Duration, Instant};

use vt_model::analysis::{Analysis, Pyramid};
use vt_model::bank::ContentHash;
use vt_model::sample::Decoded;

const RATE: u32 = 48_000;
const SECONDS: usize = 5 * 60;
/// The columns of a waveform as wide as the screen, at about 15 pixels a cell, and as wide as the
/// cells themselves.
const WIDTHS: [usize; 2] = [2400, 160];

/// The best of a few runs, as later runs aren't slowed by the first touch of memory.
fn best<T>(runs: usize, mut work: impl FnMut() -> T) -> (Duration, T) {
    let mut fastest = Duration::MAX;
    let mut last = None;
    for _ in 0..runs {
        let started = Instant::now();
        let result = work();
        fastest = fastest.min(started.elapsed());
        last = Some(result);
    }
    (fastest, last.expect("at least one run"))
}

fn main() {
    let folder = std::env::args()
        .nth(1)
        .map_or_else(std::env::temp_dir, Into::into);
    let frames = RATE as usize * SECONDS;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|at| {
            let t = at as f32 / RATE as f32;
            let envelope = (t * 2.0).fract();
            let wave = (t * 220.0 * std::f32::consts::TAU).sin() * (1.0 - envelope);
            [wave * 0.8, wave * 0.5]
        })
        .collect();
    let decoded = Decoded {
        samples,
        channels: 2,
        rate: RATE,
    };
    println!("A {SECONDS} s stereo file at {RATE} Hz: {frames} frames");

    let (pyramid_time, pyramid) = best(5, || Pyramid::build(&decoded.samples, 2));
    println!(
        "Pyramid:       {:>8.2} ms, {} levels",
        pyramid_time.as_secs_f64() * 1000.0,
        pyramid.levels()
    );
    let (analysis_time, analysis) = best(5, || Analysis::of(ContentHash(1), &decoded));
    println!(
        "Whole analysis {:>8.2} ms (the pyramid, the peak and the silence scan)",
        analysis_time.as_secs_f64() * 1000.0
    );

    let bytes = analysis.encode();
    let file = folder.join("vimtracker-analysis-bench");
    let (write_time, ()) = best(5, || std::fs::write(&file, &bytes).unwrap());
    let (read_time, read) = best(5, || {
        Analysis::decode(&std::fs::read(&file).unwrap(), ContentHash(1)).unwrap()
    });
    assert_eq!(read, analysis);
    let _ = std::fs::remove_file(&file);
    println!(
        "Cache file:    {:>8.2} MB, written in {:.2} ms, read back in {:.2} ms",
        bytes.len() as f64 / 1e6,
        write_time.as_secs_f64() * 1000.0,
        read_time.as_secs_f64() * 1000.0
    );

    println!("Full-width views, one channel, the best of 200:");
    let whole = frames as u64;
    for width in WIDTHS {
        for zoom in [1_u64, 4, 16, 64, 256, 1024, 4096] {
            let length = whole / zoom;
            let start = (whole - length) / 3;
            let (time, view) = best(200, || pyramid.view(0, start..start + length, width));
            black_box(view);
            println!(
                "  {width:>4} columns, {:>9} frames ({:>7.3} s) shown: {:>7.1} µs",
                length,
                length as f64 / f64::from(RATE),
                time.as_secs_f64() * 1e6
            );
        }
    }

    const PEAKS: u64 = 100_000;
    let (time, ()) = best(3, || {
        for at in 0..PEAKS {
            black_box(black_box(&pyramid).peak(at..whole - at * 7));
        }
    });
    println!(
        "The peak of nearly the whole file, from the pyramid: {:.0} ns",
        time.as_secs_f64() * 1e9 / PEAKS as f64
    );

    // As the loader thread does it from a file: read, hash, decode and build.
    let wav = folder.join("vimtracker-analysis-bench.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for sample in &decoded.samples {
        writer.write_sample((sample * 32_767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
    let hash = ContentHash::of_file(&wav).unwrap();
    let (time, _) = best(3, || vt_model::analysis::analyse(&wav, hash, None).unwrap());
    let _ = std::fs::remove_file(&wav);
    println!(
        "From a 16-bit WAV, reading, hashing and decoding it too: {:.0} ms",
        time.as_secs_f64() * 1000.0
    );
}
