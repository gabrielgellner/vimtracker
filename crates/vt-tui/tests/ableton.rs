//! Importing Ableton drum racks, from small racks made here rather than Live's own.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use flate2::Compression;
use flate2::write::GzEncoder;
use vt_model::kit::load_kit;
use vt_tui::ableton::{import, read_rack};

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-ableton-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn range(tag: &str, (min, max): (i32, i32)) -> String {
    format!(r#"<{tag}><Min Value="{min}" /><Max Value="{max}" /></{tag}>"#)
}

/// One sample in a Simpler: its keys, velocities and selector. It plays as recorded at its own
/// key, or at C3 if it covers more than one.
fn part(path: &str, keys: (i32, i32), velocities: (i32, i32), selector: (i32, i32)) -> String {
    format!(
        r#"<MultiSamplePart Id="0">{}{}{}<RootKey Value="{}" />
        <SampleRef><FileRef><RelativePathType Value="5" /><RelativePath Value="{path}" />
        <Path Value="/Volumes/build/Core Library/{path}" /></FileRef></SampleRef></MultiSamplePart>"#,
        range("KeyRange", keys),
        range("VelocityRange", velocities),
        range("SelectorRange", selector),
        if keys.0 == keys.1 { keys.0 } else { 60 },
    )
}

fn simpler(name: &str, volume: f64, transpose: i32, parts: &[String]) -> String {
    format!(
        r#"<OriginalSimpler Id="0"><UserName Value="{name}" />
        <Player><MultiSampleMap><SampleParts>{}</SampleParts></MultiSampleMap></Player>
        <Pitch><TransposeKey><LomId Value="0" /><Manual Value="{transpose}" /></TransposeKey>
        <TransposeFine><Manual Value="0" /></TransposeFine></Pitch>
        <VolumeAndPan><Volume><Manual Value="{volume}" /></Volume></VolumeAndPan>
        </OriginalSimpler>"#,
        parts.concat()
    )
}

fn drum_cell(path: &str, volume: f64, transpose: i32) -> String {
    format!(
        r#"<DrumCell Id="0"><UserName Value="" /><UserSample><Value><SampleRef><FileRef>
        <RelativePath Value="{path}" /></FileRef></SampleRef></Value></UserSample>
        <Voice_Transpose><Manual Value="{transpose}" /></Voice_Transpose>
        <Volume><Manual Value="{volume}" /></Volume></DrumCell>"#
    )
}

/// A pad on a MIDI note, its choke group, its mixer's volume as a gain, and its device.
fn pad(note: i32, choke: i32, mixer: f64, device: &str) -> String {
    format!(
        r#"<DrumBranchPreset Id="0"><Name Value="" />
        <DevicePresets><AbletonDevicePreset Id="0"><Device>{device}</Device></AbletonDevicePreset></DevicePresets>
        <MixerPreset><AbletonDevicePreset Id="0"><Device><AudioBranchMixerDevice Id="0">
        <Volume><Manual Value="{mixer}" /></Volume></AudioBranchMixerDevice></Device></AbletonDevicePreset></MixerPreset>
        <ZoneSettings><ReceivingNote Value="{}" /><SendingNote Value="60" /><ChokeGroup Value="{choke}" /></ZoneSettings>
        </DrumBranchPreset>"#,
        128 - note
    )
}

fn rack(pads: &[String]) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <Ableton MajorVersion="5" MinorVersion="12.0_12402"><GroupDevicePreset>
        <Device><DrumGroupDevice Id="0"><UserName Value="" /></DrumGroupDevice></Device>
        <BranchPresets>{}</BranchPresets></GroupDevicePreset></Ableton>"#,
        pads.concat()
    )
}

/// A library with these samples, as empty files, since only their names are read.
fn library(samples: &[&str]) -> PathBuf {
    let library = folder();
    for sample in samples {
        let path = library.join(sample);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
    library
}

fn write_rack(library: &Path, relative: &str, xml: &str) {
    let path = library.join("Racks/Drum Racks").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(xml.as_bytes()).unwrap();
    fs::write(path, gzip.finish().unwrap()).unwrap();
}

const KICKS: [&str; 4] = [
    "Samples/Kick/Kick Soft.wav",
    "Samples/Kick/Kick Hard.wav",
    "Samples/Kick/Kick Low Soft.wav",
    "Samples/Kick/Kick Low Hard.wav",
];

#[test]
fn a_pad_gives_its_sound_its_layers_its_tones_and_its_settings() {
    let library = library(&KICKS);
    // The note the pad sends, 60, plays the first two; 61 has the other tone.
    let kick = simpler(
        "Bass Drum",
        -9.0,
        -2,
        &[
            part(KICKS[0], (60, 60), (1, 63), (0, 127)),
            part(KICKS[1], (60, 60), (64, 127), (0, 127)),
            part(KICKS[2], (61, 61), (1, 63), (0, 127)),
            part(KICKS[3], (61, 61), (64, 127), (0, 127)),
        ],
    );
    let xml = rack(&[pad(36, 0, 0.5, &kick)]);
    let (kit, report) = read_rack(&xml, &library, "Test Kit", "Drum Machines").unwrap();
    assert_eq!(kit.name, "Test Kit");
    assert_eq!(kit.source, "Ableton Core Library");
    assert_eq!(kit.category, "Drum Machines");
    assert_eq!((report.skipped_pads, report.missing.len()), (0, 0));
    let [sound] = &kit.sounds[..] else {
        panic!("one sound, not {}", kit.sounds.len());
    };
    let at = |relative: &str| library.join(relative).display().to_string();
    assert_eq!(sound.label, "BD");
    assert_eq!(sound.name, "Bass Drum");
    assert_eq!(
        sound.path,
        at(KICKS[1]),
        "the hardest layer of what the pad plays"
    );
    assert_eq!(sound.layers, [at(KICKS[0]), at(KICKS[1])], "soft to hard");
    assert_eq!(
        sound.variants,
        [at(KICKS[3])],
        "the other tone, by its hardest layer"
    );
    assert_eq!(sound.choke, None);
    // Simpler at -9 dB is 3 dB over where it starts, and the pad's mixer at half is -6 dB.
    assert!((sound.gain - -3.0).abs() < 0.05, "{}", sound.gain);
    assert_eq!(sound.tune, -2);
}

#[test]
fn pads_are_named_labelled_and_ordered_by_note_and_synths_are_left_out() {
    let samples = [
        "Samples/Hihat/Hihat Open Dusty.wav",
        "Samples/Hihat/Hihat Closed Dusty.wav",
        "Samples/Kick/Kick One.wav",
        "Samples/Kick/Kick Two.wav",
        "Samples/Perc/Shaker Fast.wav",
    ];
    let library = library(&samples);
    let one = |path: &str| simpler("", -12.0, 0, &[part(path, (0, 127), (1, 127), (0, 127))]);
    let xml = rack(&[
        pad(46, 1, 1.0, &one(samples[0])),
        pad(42, 1, 1.0, &drum_cell(samples[1], -12.0, 3)),
        pad(37, 0, 1.0, &one(samples[3])),
        pad(36, 0, 1.0, &one(samples[2])),
        pad(39, 0, 1.0, &one(samples[4])),
        pad(
            40,
            0,
            1.0,
            r#"<Drift Id="0"><UserName Value="Synth Kick" /></Drift>"#,
        ),
        pad(41, 0, 1.0, &one("Samples/Gone/Missing.wav")),
    ]);
    let (kit, report) = read_rack(&xml, &library, "Dusty", "Sampled").unwrap();
    assert_eq!(
        report.skipped_pads, 2,
        "the synth, and the pad whose sample is missing"
    );
    assert_eq!(report.missing, ["Samples/Gone/Missing.wav"]);
    let rows: Vec<(&str, &str, Option<u8>, i8)> = kit
        .sounds
        .iter()
        .map(|s| (s.label.as_str(), s.name.as_str(), s.choke, s.tune))
        .collect();
    assert_eq!(
        rows,
        [
            ("BD", "Kick One", None, 0),
            ("", "Kick Two", None, 0),
            ("", "Shaker Fast", None, 0),
            ("CH", "Hihat Closed Dusty", Some(1), 3),
            ("OH", "Hihat Open Dusty", Some(1), 0),
        ],
        "by note, the second kick unlabelled, each named by its sample"
    );
}

#[test]
fn a_drum_rack_inside_an_instrument_rack_is_read_the_same() {
    let library = library(&["Samples/Snare/Snare Wide.wav"]);
    let snare = simpler(
        "Snare Drum",
        -12.0,
        0,
        &[part(
            "Samples/Snare/Snare Wide.wav",
            (0, 127),
            (1, 127),
            (0, 127),
        )],
    );
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Ableton><GroupDevicePreset>
        <Device><InstrumentGroupDevice Id="0" /></Device><BranchPresets>
        <InstrumentBranchPreset Id="0"><DevicePresets><GroupDevicePreset Id="0">
        <Device><DrumGroupDevice Id="0" /></Device><BranchPresets>{}</BranchPresets>
        </GroupDevicePreset></DevicePresets></InstrumentBranchPreset>
        </BranchPresets></GroupDevicePreset></Ableton>"#,
        pad(38, 0, 1.0, &snare)
    );
    let (kit, _) = read_rack(&xml, &library, "Wrapped", "Sampled").unwrap();
    assert_eq!(kit.sounds.len(), 1);
    assert_eq!(kit.sounds[0].label, "SD");
    assert_eq!(kit.sounds[0].name, "Snare Drum");
}

#[test]
fn importing_again_replaces_its_own_kits_and_nothing_else() {
    let library = library(&KICKS);
    let one = simpler(
        "Kick",
        -12.0,
        0,
        &[part(KICKS[0], (0, 127), (1, 127), (0, 127))],
    );
    write_rack(
        &library,
        "Drum Machines/Old Kit.adg",
        &rack(&[pad(36, 0, 1.0, &one)]),
    );
    write_rack(
        &library,
        "Electronic/Kept Kit.adg",
        &rack(&[pad(36, 0, 1.0, &one)]),
    );
    write_rack(
        &library,
        "Electronic/Synth Kit.adg",
        &rack(&[pad(36, 0, 1.0, "<Drift />")]),
    );
    let out = folder();
    let mine = out.join("Electronic/Kept Kit.ron");
    fs::create_dir_all(mine.parent().unwrap()).unwrap();
    fs::write(&mine, "(sounds: [])").unwrap();
    let notes = out.join("notes.txt");
    fs::write(&notes, "mine").unwrap();

    let report = import(&library, &out).unwrap();
    assert_eq!((report.kits, report.sounds), (1, 1));
    assert_eq!(
        report.kept,
        std::slice::from_ref(&mine),
        "a file it didn't make is left"
    );
    assert_eq!(report.empty.len(), 1, "the synth kit");
    let written = out.join("Drum Machines/Old Kit.ron");
    let kit = load_kit(&written).unwrap();
    assert_eq!(kit.name, "Old Kit");
    assert_eq!(
        kit.sounds[0].path,
        library.join(KICKS[0]),
        "into the library"
    );

    // Live's update takes the old kit away and brings a new one.
    fs::remove_file(library.join("Racks/Drum Racks/Drum Machines/Old Kit.adg")).unwrap();
    write_rack(
        &library,
        "Drum Machines/New Kit.adg",
        &rack(&[pad(36, 0, 1.0, &one)]),
    );
    let report = import(&library, &out).unwrap();
    assert_eq!(report.removed, std::slice::from_ref(&written));
    assert!(!written.exists());
    assert!(out.join("Drum Machines/New Kit.ron").is_file());
    assert_eq!(fs::read_to_string(&mine).unwrap(), "(sounds: [])");
    assert_eq!(fs::read_to_string(&notes).unwrap(), "mine");
    let again = import(&library, &out).unwrap();
    assert_eq!(
        (again.kits, again.removed.len()),
        (1, 0),
        "it replaces its own"
    );
}

#[test]
fn a_folder_that_isnt_a_library_is_refused() {
    let error = import(&folder(), &folder()).unwrap_err().to_string();
    assert!(error.contains("Racks/Drum Racks"), "{error}");
}
