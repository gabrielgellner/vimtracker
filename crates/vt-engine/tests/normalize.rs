//! Normalising a bank entry is a gain worked out from its file's analysis, the same for every
//! channel, as Bitwig's Normalize writes a gain rather than audio.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
const LENGTH: usize = RATE as usize;
const HASH: ContentHash = ContentHash(0x5eed);

/// A stereo file whose left channel peaks at 0.5 and right at 0.2, with its analysis in the pool
/// or not.
fn pool(analysed: bool) -> Arc<SamplePool> {
    let frames: Vec<[f32; 2]> = (0..LENGTH)
        .map(|at| {
            let wave = (at as f32 * 0.01).sin();
            [0.5 * wave, 0.2 * wave]
        })
        .collect();
    let mut pool = SamplePool::default();
    if analysed {
        let decoded = Decoded {
            samples: frames.iter().flatten().copied().collect(),
            channels: 2,
            rate: RATE,
        };
        pool.insert_analysis(Arc::new(Analysis::of(HASH, &decoded)));
    }
    pool.insert("uneven", SampleData { frames });
    Arc::new(pool)
}

/// One track playing the file from the first step, normalised to a level or not.
fn render(normalize: Option<f32>, analysed: bool) -> Vec<f32> {
    let mut song = Song::new(1);
    song.limiter = false;
    let mut entry = BankEntry::of_file("uneven", HASH);
    entry.edit.play.normalize = normalize;
    song.bank.set(1, Some(entry));
    let track = &mut song.tracks[0];
    track.set_setting(SettingId::SAMPLE, 1);
    track.envelopes[0].attack_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-4 -- --- --- ---".parse().unwrap();
    let version = Published::new(Arc::new(song), pool(analysed));
    render_frames(Arc::new(version), RATE, 4000)
}

#[test]
fn normalising_scales_both_channels_alike_to_bring_the_louder_to_the_level() {
    let plain = render(None, true);
    let normalised = render(Some(-6.0), true);
    // The left channel's peak of 0.5 comes to -6 dBFS, and the right takes the same gain.
    let gain = 10_f32.powf(-6.0 / 20.0) / 0.5;
    let at = 3000 * 2;
    for channel in 0..2 {
        let ratio = normalised[at + channel] / plain[at + channel];
        assert!((ratio - gain).abs() < 1e-4, "channel {channel}: {ratio}");
    }
    let analysis = &pool(true).analysis(HASH).cloned().unwrap();
    let play = vt_model::bank::PlayParams {
        normalize: Some(-0.3),
        ..Default::default()
    };
    let peak = analysis.peak * play.normalize_gain(Some(analysis.peak));
    assert!((20.0 * peak.log10() + 0.3).abs() < 1e-4, "{peak}");
}

#[test]
fn a_sample_plays_as_it_is_until_its_analysis_is_in() {
    let plain = render(None, false);
    let waiting = render(Some(-6.0), false);
    assert_eq!(plain, waiting);
    assert_eq!(
        render(None, true),
        plain,
        "an analysis alone changes nothing"
    );
}

#[test]
fn an_entry_is_worked_out_again_when_its_analysis_comes_in() {
    // The same sound in both pools, as the app clones its pool to take an analysis in, so only
    // the analysis tells the two versions apart.
    let waiting = pool(false);
    let mut analysed = SamplePool::clone(&waiting);
    let frames = &waiting.shared(0).unwrap().frames;
    let decoded = Decoded {
        samples: frames.iter().flatten().copied().collect(),
        channels: 2,
        rate: RATE,
    };
    analysed.insert_analysis(Arc::new(Analysis::of(HASH, &decoded)));

    let mut song = Song::new(1);
    let mut normalised = BankEntry::of_file("uneven", HASH);
    normalised.edit.play.normalize = Some(-6.0);
    song.bank.set(1, Some(normalised));
    song.bank.set(2, Some(BankEntry::of_file("uneven", HASH)));
    let song = Arc::new(song);
    let first = Published::new(Arc::clone(&song), waiting);
    let next = Published::after(song, Arc::new(analysed), Some(&first));
    let (old, new) = (&first.bank, &next.bank);
    assert_eq!(old[1].as_ref().unwrap().span(0, false).gain, 1.0);
    let gain = new[1].as_ref().unwrap().span(0, false).gain;
    let wanted = 10_f32.powf(-6.0 / 20.0) / 0.5;
    assert!((gain - wanted).abs() < 1e-4, "the peak came in: {gain}");
    assert!(
        Arc::ptr_eq(old[2].as_ref().unwrap(), new[2].as_ref().unwrap()),
        "an entry that isn't normalised is kept"
    );
}
