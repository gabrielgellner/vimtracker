//! A file's analysis is kept in a cache folder by its content hash: read back instead of built
//! again, built again for a changed file or a cache file that can't be read, and never written
//! beside the file.

use std::path::{Path, PathBuf};

use vt_model::analysis::{ANALYSIS_VERSION, Analysis, Found, analyse, cache_file};
use vt_model::bank::{BankEntry, ContentHash, PlayParams, Points};
use vt_model::onsets::DEFAULT_SENS;
use vt_model::project::{analyse_samples, load_samples};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("vt-analysis-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// A stereo WAV whose left channel is twice as loud as its right, after some silence.
fn wav(path: &Path, level: i16) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for at in 0..10_000 {
        let on = (1000..9000).contains(&at);
        let value = if on && at % 7 == 0 { level } else { 0 };
        writer.write_sample(value).unwrap();
        writer.write_sample(value / 2).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn an_analysis_is_built_once_then_read_back_from_the_cache() {
    let folder = folder("once");
    let cache = folder.join("cache");
    let file = folder.join("hit.wav");
    wav(&file, 16_384);
    let hash = ContentHash::of_file(&file).unwrap();
    let (built, found) = analyse(&file, hash, Some(&cache)).unwrap();
    assert_eq!(found, Found::Built);
    assert_eq!(built.hash, hash);
    assert_eq!(built.peak, 0.5);
    assert_eq!(built.rate, 44_100);
    assert_eq!(built.sound, Some(1001..8996));
    assert!(cache_file(&cache, hash).exists());
    let beside: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(beside.len(), 2, "only the file and the cache: {beside:?}");
    let (read, found) = analyse(&file, hash, Some(&cache)).unwrap();
    assert_eq!(found, Found::Cached);
    assert_eq!(read, built);
    // Without a cache, it's only worked out.
    let (alone, found) = analyse(&file, hash, None).unwrap();
    assert_eq!((alone, found), (built, Found::Built));
}

#[test]
fn a_changed_file_gets_an_analysis_of_its_own() {
    let folder = folder("changed");
    let cache = folder.join("cache");
    let file = folder.join("hit.wav");
    wav(&file, 16_384);
    let old = ContentHash::of_file(&file).unwrap();
    let (first, _) = analyse(&file, old, Some(&cache)).unwrap();
    wav(&file, 8192);
    let new = ContentHash::of_file(&file).unwrap();
    assert_ne!(old, new);
    let (second, found) = analyse(&file, new, Some(&cache)).unwrap();
    assert_eq!(found, Found::Built);
    assert_eq!((second.hash, second.peak), (new, 0.25));
    assert_eq!(first.peak, 0.5, "the old content's is kept as it was");
    // Asked for by the old hash once the old analysis is gone, the file's own content is analysed
    // and kept under its own hash, never the old one's.
    std::fs::remove_file(cache_file(&cache, old)).unwrap();
    let (stale, found) = analyse(&file, old, Some(&cache)).unwrap();
    assert_eq!((stale.hash, found), (new, Found::Built));
    assert!(!cache_file(&cache, old).exists());
}

#[test]
fn a_cache_file_of_another_version_or_cut_short_is_built_again() {
    let folder = folder("version");
    let cache = folder.join("cache");
    let file = folder.join("hit.wav");
    wav(&file, 16_384);
    let hash = ContentHash::of_file(&file).unwrap();
    let (built, _) = analyse(&file, hash, Some(&cache)).unwrap();
    let kept = cache_file(&cache, hash);
    let mut bytes = std::fs::read(&kept).unwrap();
    bytes[4..8].copy_from_slice(&(ANALYSIS_VERSION + 1).to_le_bytes());
    std::fs::write(&kept, &bytes).unwrap();
    let (again, found) = analyse(&file, hash, Some(&cache)).unwrap();
    assert_eq!((found, &again), (Found::Built, &built));
    assert_eq!(
        Analysis::decode(&std::fs::read(&kept).unwrap(), hash),
        Some(built)
    );
    std::fs::write(&kept, &bytes[..20]).unwrap();
    assert_eq!(analyse(&file, hash, Some(&cache)).unwrap().1, Found::Built);
}

#[test]
fn normalising_and_trimming_are_settings_saved_only_when_set() {
    let plain = BankEntry::of_file("samples/hit.wav", ContentHash(3));
    let text = ron::to_string(&plain).unwrap();
    assert!(
        !text.contains("points") && !text.contains("normalize"),
        "{text}"
    );
    let mut set = plain.clone();
    set.edit.play.normalize = Some(-0.3);
    set.edit.points = Points {
        start: 4410,
        end: Some(39_690),
        ..Points::default()
    };
    let text = ron::to_string(&set).unwrap();
    assert_eq!(ron::from_str::<BankEntry>(&text).unwrap(), set);
    // Nothing asked for, a silent file, and a peak not known yet all leave the sound as it is.
    let play = set.edit.play;
    assert_eq!(plain.edit.play.normalize_gain(Some(0.5)), 1.0);
    assert_eq!(play.normalize_gain(Some(0.0)), 1.0);
    assert_eq!(play.normalize_gain(None), 1.0);
    assert!((play.normalize_gain(Some(0.25)) - 3.8637).abs() < 1e-3);
}

#[test]
fn a_songs_samples_are_analysed_into_the_pool_and_normalise_from_it() {
    let folder = folder("song");
    let cache = folder.join("cache");
    let file = folder.join("hit.wav");
    wav(&file, 16_384);
    let hash = ContentHash::of_file(&file).unwrap();
    let mut song = Song::new(1);
    let mut entry = BankEntry::of_file(file.to_string_lossy(), hash);
    entry.edit.play = PlayParams {
        normalize: Some(-0.3),
        ..PlayParams::default()
    };
    song.bank.set(1, Some(entry.clone()));
    song.bank
        .set(2, Some(BankEntry::of_file("gone.wav", ContentHash(1))));
    let mut pool = SamplePool::default();
    load_samples(None, &song, 48_000, &mut pool);
    let errors = analyse_samples(None, &song, Some(&cache), &mut pool);
    assert_eq!(errors.len(), 1, "the missing file is reported");
    let analysis = pool.analysis(hash).unwrap();
    // Both channels take the gain that brings the louder, the left, to -0.3 dBFS.
    let gain = entry.edit.play.normalize_gain(Some(analysis.peak));
    assert!((20.0 * (0.5 * gain).log10() + 0.3).abs() < 1e-4);
    assert!(pool.analysis(ContentHash(1)).is_none());
}

#[test]
fn where_sounds_start_is_kept_with_the_rest_and_read_back_not_found_again() {
    let folder = folder("onsets");
    let cache = folder.join("cache");
    let file = folder.join("clicks.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let clicks = [3000, 14_025, 25_050, 36_075];
    let mut writer = hound::WavWriter::create(&file, spec).unwrap();
    for at in 0..44_100 {
        let hit = clicks
            .iter()
            .any(|click| (*click..*click + 50).contains(&at));
        writer
            .write_sample(if hit { 20_000_i16 } else { 0 })
            .unwrap();
    }
    writer.finalize().unwrap();
    let hash = ContentHash::of_file(&file).unwrap();
    let (built, found) = analyse(&file, hash, Some(&cache)).unwrap();
    assert_eq!(found, Found::Built);
    assert_eq!(built.onsets(DEFAULT_SENS), clicks);
    let (read, found) = analyse(&file, hash, Some(&cache)).unwrap();
    assert_eq!(found, Found::Cached);
    assert_eq!(read.onsets, built.onsets);
}
