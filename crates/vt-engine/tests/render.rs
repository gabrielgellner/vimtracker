use std::path::PathBuf;
use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Instrument, Song};

const RATE: u32 = 48_000;
/// 16 steps at 120 BPM.
const LOOP: usize = 96_000;

/// A short decaying tone, computed so no audio file is needed.
fn pool() -> Arc<SamplePool> {
    let frames = (0..4800)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let value = (t * 220.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 4800.0) * 0.8;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("tone", SampleData { frames });
    Arc::new(pool)
}

fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(2);
    song.instruments.insert(
        0,
        Arc::new(Instrument {
            sample: Some("tone".to_owned()),
            ..Instrument::default()
        }),
    );
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn render(song: Song, options: RenderOptions) -> Vec<f32> {
    render_loop(
        Arc::new(Published::new(Arc::new(song), pool())),
        RATE,
        options,
    )
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()))
}

/// FNV-1a over the samples' exact bits, so a golden catches any change at all.
fn hash(samples: &[f32]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Compare with a saved hash. `VT_BLESS=1` updates it after an intended change to the sound.
///
/// These hold on one platform: the filter and reverb use library math that can differ elsewhere.
fn golden(name: &str, samples: &[f32]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.hash"));
    let found = hash(samples);
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::write(&path, format!("{found}\n")).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    assert_eq!(
        saved.trim(),
        found,
        "{name} changed: run with VT_BLESS=1 if that was intended"
    );
}

const FX_LOOP: [&str; 4] = [
    "C-4 60 00 CUT60 RES40 PAN40",
    "--- -- -- DLY80 ---00 ---00",
    "G-4 40 00 REV60 RET14 ---00",
    "C-3 -- 00 CHA80 DEL02 KIL04",
];

#[test]
fn golden_renders_stay_the_same() {
    golden(
        "plain",
        &render(
            song(&["C-4 -- 00 ---00 ---00 ---00"]),
            RenderOptions::default(),
        ),
    );
    let mut swung = song(&FX_LOOP);
    swung.swing = 667;
    swung.seed = 7;
    golden("fx", &render(swung, RenderOptions::default()));
}

#[test]
fn a_render_is_exactly_the_loops_length() {
    let out = render(song(&FX_LOOP), RenderOptions::default());
    assert_eq!(out.len(), LOOP * 2);
    // With a fractional step length, the render is the difference of the two exact pass ends.
    let mut odd = song(&FX_LOOP);
    odd.millibpm = 137_630;
    let out = render(odd, RenderOptions::default());
    assert_eq!(out.len(), 2 * (167_405 - 83_702));
}

#[test]
fn rows_set_the_length_and_start_from_the_top() {
    let options = RenderOptions {
        rows: Some(4),
        tail: false,
    };
    let out = render(song(&["C-4 -- 00 ---00 ---00 ---00"]), options);
    assert_eq!(out.len(), 24_000 * 2);
    assert!(
        peak(&out[..200]) > 0.0,
        "the note on step 0 opens the render"
    );
}

#[test]
fn the_first_pass_fills_effects_so_the_start_hears_the_end() {
    // A delay tail only sounds at the top if something came before it.
    let mut texts = vec!["--- -- -- ---00 ---00 ---00"; 16];
    texts[15] = "C-4 -- 00 DLYFF ---00 ---00";
    let out = render(song(&texts), RenderOptions::default());
    assert!(
        peak(&out[..2 * 18_000]) > 0.01,
        "{}",
        peak(&out[..2 * 18_000])
    );
}

#[test]
fn the_tail_extends_the_render_and_ends_at_silence() {
    // A hit on the last step is still ringing when the capture ends.
    let mut texts = vec!["--- -- -- ---00 ---00 ---00"; 16];
    texts[15] = "C-4 -- 00 REVFF ---00 ---00";
    let plain = render(song(&texts), RenderOptions::default());
    let tailed = render(
        song(&texts),
        RenderOptions {
            rows: None,
            tail: true,
        },
    );
    assert!(tailed.len() > plain.len());
    assert!(tailed.len() <= plain.len() + 2 * 10 * RATE as usize);
    assert!(
        peak(&tailed[tailed.len() - 2..]) > 0.0,
        "silence is trimmed"
    );
    // With nothing ringing, there is no tail at all.
    let dry = render(
        song(&["--- -- -- ---00 ---00 ---00"]),
        RenderOptions {
            rows: None,
            tail: true,
        },
    );
    assert_eq!(dry.len(), LOOP * 2);
}

#[test]
fn with_the_limiter_off_a_render_keeps_levels_over_full() {
    // The same tone on eight tracks at once adds up to well past full level.
    let mut loud = song(&[]);
    loud = Song {
        instruments: loud.instruments,
        ..Song::new(8)
    };
    for id in 0..8 {
        let phrase = Arc::make_mut(loud.phrases.get_mut(&id).unwrap());
        *phrase.step_mut(0).unwrap() = "C-4 -- 00 ---00 ---00 ---00".parse().unwrap();
    }
    let limited = peak(&render(loud.clone(), RenderOptions::default()));
    assert_eq!(limited, 1.0);
    loud.limiter = false;
    let open = peak(&render(loud, RenderOptions::default()));
    assert!(open > 1.2, "{open}");
}

#[test]
fn a_render_repeats_and_respects_mute() {
    let first = render(song(&FX_LOOP), RenderOptions::default());
    assert_eq!(first, render(song(&FX_LOOP), RenderOptions::default()));
    let mut muted = song(&FX_LOOP);
    muted.tracks[0].mute = true;
    assert_eq!(peak(&render(muted, RenderOptions::default())), 0.0);
}

#[test]
fn a_render_writes_float_stereo_and_replaces_the_file() {
    let path = std::env::temp_dir().join(format!("vt-render-{}.wav", std::process::id()));
    std::fs::write(&path, "old").unwrap();
    let samples = render(song(&FX_LOOP), RenderOptions::default());
    write_wav(&path, RATE, &samples).unwrap();
    let reader = hound::WavReader::open(&path).unwrap();
    let spec = reader.spec();
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.bits_per_sample),
        (2, RATE, 32)
    );
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    assert_eq!(reader.len() as usize, samples.len());
    std::fs::remove_file(&path).unwrap();
}
