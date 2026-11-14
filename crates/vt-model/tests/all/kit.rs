//! Kit files: what they say about a kit and each of its sounds, and what they refuse.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::kit::{KitFile, SoundFile, load_kit};

fn kit(text: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
        "vt-kit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("808 Core.ron");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn a_kit_says_where_it_came_from_and_what_each_sound_is() {
    let path = kit(r#"(
        name: "808 Core Kit",
        source: "Ableton Core Library",
        category: "Drum Machines",
        sounds: [
            (label: "bd", name: "Bass Drum 808", path: "Kick/Kick 808 Tone1.wav",
             variants: ["Kick/Kick 808 Tone5.wav"],
             layers: ["Kick/Soft.wav", "/library/Kick/Hard.wav"],
             choke: None, gain: -3.5, tune: -12),
            (label: "CH", name: "Closed Hat", path: "~/hats/closed.wav", choke: Some(1)),
            (label: "OH", name: "Open Hat", path: "hats/open.wav", choke: Some(1), tune: 2),
        ],
    )"#);
    let folder = path.parent().unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let kit = load_kit(&path).unwrap();
    assert_eq!(kit.name, "808 Core Kit");
    assert_eq!(kit.source, "Ableton Core Library");
    assert_eq!(kit.category, "Drum Machines");
    let [bd, ch, oh] = &kit.sounds[..] else {
        panic!("three sounds, not {}", kit.sounds.len());
    };
    assert_eq!(bd.label, "BD");
    assert_eq!(bd.name, "Bass Drum 808");
    assert_eq!(bd.path, folder.join("Kick/Kick 808 Tone1.wav"));
    assert_eq!(bd.variants, [folder.join("Kick/Kick 808 Tone5.wav")]);
    assert_eq!(
        bd.layers,
        [
            folder.join("Kick/Soft.wav"),
            Path::new("/library/Kick/Hard.wav").to_owned()
        ]
    );
    assert_eq!((bd.choke, bd.gain, bd.tune), (None, -3.5, -12));
    assert_eq!(ch.path, home.join("hats/closed.wav"));
    assert_eq!((ch.choke, ch.gain, ch.tune), (Some(1), 0.0, 0));
    assert_eq!((oh.choke, oh.tune), (Some(1), 2));
}

#[test]
fn a_label_and_a_path_are_enough() {
    let path = kit(r#"(sounds: [(label: "BD", path: "kick.wav"), (path: "fx/zap.wav")])"#);
    let kit = load_kit(&path).unwrap();
    assert_eq!(kit.name, "808 Core", "the file's name");
    assert_eq!((kit.source.as_str(), kit.category.as_str()), ("", ""));
    assert_eq!(kit.sounds[0].name, "kick");
    assert_eq!(kit.sounds[1].label, "");
    assert_eq!(kit.sounds[1].name, "zap");
    assert!(kit.sounds.iter().all(|s| s.variants.is_empty()
        && s.layers.is_empty()
        && s.choke.is_none()
        && s.gain == 0.0
        && s.tune == 0));
}

#[test]
fn a_sound_out_of_range_is_refused_by_its_label_or_path() {
    for (sound, expected) in [
        (
            r#"(label: "OH", path: "a.wav", choke: Some(0))"#,
            "OH: choke group 0",
        ),
        (
            r#"(label: "OH", path: "a.wav", choke: Some(17))"#,
            "OH: choke group 17",
        ),
        (r#"(path: "a.wav", tune: 49)"#, "a.wav: tune 49"),
        (r#"(label: "BD", path: "a.wav", tune: -49)"#, "BD: tune -49"),
        (r#"(label: "B", path: "a.wav")"#, "\"B\" can't be a name"),
        (r#"(label: "BD", path: "a.wav", pitch: 2)"#, "pitch"),
    ] {
        let path = kit(&format!("(sounds: [{sound}])"));
        let error = load_kit(&path).unwrap_err().to_string();
        assert!(error.contains(expected), "{sound}: {error}");
        assert!(error.contains("808 Core.ron"), "{error}");
    }
    let path = kit(r#"(sounds: [(path: "a.wav", choke: Some(16), tune: 48)])"#);
    assert!(load_kit(&path).is_ok(), "the ends are allowed");
}

#[test]
fn a_kit_file_writes_only_what_it_says_and_reads_back() {
    let file = KitFile {
        name: "909".into(),
        source: "Ableton Core Library".into(),
        category: String::new(),
        sounds: vec![
            SoundFile {
                label: "CH".into(),
                path: "/909/Hihat Closed 909.wav".into(),
                choke: Some(1),
                ..SoundFile::default()
            },
            SoundFile {
                path: "/909/Kick 909 1.wav".into(),
                variants: vec!["/909/Kick 909 2.wav".into()],
                gain: 1.5,
                tune: -2,
                ..SoundFile::default()
            },
        ],
    };
    let text = ron::to_string(&file).unwrap();
    assert!(
        !text.contains("category") && !text.contains("layers"),
        "{text}"
    );
    assert_eq!(ron::from_str::<KitFile>(&text).unwrap(), file);
}

#[test]
fn a_sounds_gain_and_tune_are_read_as_written_and_default_to_zero() {
    let path = kit(r#"(sounds: [
        (path: "a.wav"),
        (path: "b.wav", gain: -6.0, tune: 3),
        (path: "c.wav", gain: 20.0, tune: -48),
        (path: "d.wav", gain: -100.0, tune: 48),
    ])"#);
    let kit = load_kit(&path).unwrap();
    let read: Vec<(f32, i8)> = kit.sounds.iter().map(|s| (s.gain, s.tune)).collect();
    assert_eq!(
        read,
        [(0.0, 0), (-6.0, 3), (20.0, -48), (-100.0, 48)],
        "in decibels and semitones, as the file has them, out to the tune's ends"
    );
}
