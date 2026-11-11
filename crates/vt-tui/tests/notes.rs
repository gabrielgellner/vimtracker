//! `vimtracker notes`: a track's notes from a song, once through its chain, timed as the sequencer
//! plays them, for tools/compare to play on every side (decision 141).

use std::path::PathBuf;
use std::sync::Arc;

use vt_model::song::{Chain, ChainEntry, Machine, Phrase, Song, SynthVoice};
use vt_model::step::{FxCommand, FxSlot, Step};
use vt_tui::dsp_probe::{Loaded, parse, prepare};
use vt_tui::track_notes::{Choice, Line, LineNote, line, write};

/// A sixteenth at 120 BPM, in seconds.
const STEP: f64 = 0.125;
/// The Acid's own gate, `80`, as a share of a pulse.
const ACID_SHARE: f64 = 128.0 / 255.0;

/// A song at 120 BPM whose first track plays the Acid, its first phrase these rows.
fn acid_song(rows: &[&str]) -> Song {
    let mut song = Song::new(2);
    song.millibpm = 120_000;
    let track = &mut song.tracks[0];
    track.name = "AC".to_owned();
    track.machine = Machine::Plaits(SynthVoice::Acid);
    let steps = rows
        .iter()
        .map(|row| row.parse::<Step>().unwrap())
        .collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn acid(song: &Song) -> Line {
    let choice = Choice {
        machine: Some("acid".to_owned()),
        ..Choice::default()
    };
    line(song, &choice).expect("the track's line is read")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// A note, plain or accented, as the line has it.
fn note(time: f64, length: f64, note: u8, velocity: u8, slide: bool) -> LineNote {
    LineNote {
        time,
        length,
        note,
        velocity,
        accent: velocity >= 0x70,
        slide,
    }
}

fn assert_notes(found: &[LineNote], expected: &[LineNote]) {
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (found, expected) in found.iter().zip(expected) {
        assert!(
            close(found.time, expected.time) && close(found.length, expected.length),
            "{found:?} isn't {expected:?}"
        );
        assert_eq!(
            (found.note, found.velocity, found.accent, found.slide),
            (
                expected.note,
                expected.velocity,
                expected.accent,
                expected.slide
            ),
        );
    }
}

#[test]
fn a_line_is_timed_as_the_sequencer_plays_it() {
    let song = acid_song(&[
        "C-3 7F SGL2",
        "--- --",
        "D-3 64 HLD1",
        "E-3 64",
        "OFF --",
        "G-3 64 MLT2",
        "A-3 64 RST1",
    ]);
    let line = acid(&song);
    assert_eq!(line.track, "AC");
    assert_eq!(line.machine, "acid");
    assert!(close(line.bpm, 120.0));
    // Seven rows over nine pulses.
    assert!(close(line.seconds, 9.0 * STEP));
    assert_notes(
        &line.notes,
        &[
            // A `Single` over two pulses closes halfway through its second, at the Acid's gate.
            note(0.0, (1.0 + ACID_SHARE) * STEP, 48, 0x7F, false),
            // A `Hold` lasts to the next row's note, and slides into it.
            note(3.0 * STEP, STEP, 50, 0x64, true),
            // The `OFF` after comes once its gate has closed.
            note(4.0 * STEP, ACID_SHARE * STEP, 52, 0x64, false),
            // A `Multi` sounds on each of its pulses, and a `Rest` plays nothing.
            note(6.0 * STEP, ACID_SHARE * STEP, 55, 0x64, false),
            note(7.0 * STEP, ACID_SHARE * STEP, 55, 0x64, false),
        ],
    );
}

#[test]
fn a_full_gate_holds_to_the_next_note_and_a_chain_transposes() {
    let mut song = acid_song(&["C-3 64", "--- --", "D-3 --"]);
    let track = &mut song.tracks[0];
    track.gate_length = Some(0xFF);
    track.chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![
                ChainEntry {
                    phrase: 0,
                    transpose: 0,
                },
                ChainEntry {
                    phrase: 0,
                    transpose: 12,
                },
            ],
            label: String::new(),
        }),
    );
    let line = acid(&song);
    assert!(close(line.seconds, 6.0 * STEP));
    assert_notes(
        &line.notes,
        &[
            note(0.0, 2.0 * STEP, 48, 0x64, false),
            // A row with no velocity plays at full velocity, an accent on the Acid.
            note(2.0 * STEP, STEP, 50, 0x7F, false),
            note(3.0 * STEP, 2.0 * STEP, 60, 0x64, false),
            // The last note lasts to the chain's end.
            note(5.0 * STEP, STEP, 62, 0x7F, false),
        ],
    );
}

#[test]
fn a_track_is_chosen_by_its_name_or_its_machine() {
    let mut song = acid_song(&["C-3 64"]);
    song.tracks[1].name = "BD".to_owned();
    let named = Choice {
        track: Some("bd".to_owned()),
        ..Choice::default()
    };
    assert_eq!(line(&song, &named).unwrap().machine, "sampler");
    assert_eq!(line(&song, &Choice::default()).unwrap().track, "AC");
    let missing = Choice {
        track: Some("BD".to_owned()),
        machine: Some("acid".to_owned()),
    };
    let refused = line(&song, &missing).unwrap_err().to_string();
    assert_eq!(refused, "the song has no sound track named BD on acid");
}

#[test]
fn the_machine_settings_come_with_their_defaults() {
    let mut song = acid_song(&["C-3 64"]);
    let cutoff = vt_model::settings::SettingId::from_key("acid.cutoff").unwrap();
    song.tracks[0].set_setting(cutoff, 0x85);
    let line = acid(&song);
    assert_eq!(line.settings["acid.cutoff"], 0x85);
    assert!(line.settings.contains_key("acid.resonance"));
    assert!(line.settings.keys().all(|key| key.starts_with("acid.")));
}

#[test]
fn rows_the_line_cant_read_are_refused() {
    let mut song = acid_song(&["C-3 64", "D-3 64"]);
    let mut steps = song.tracks[0].phrases[&0].steps().to_vec();
    steps[1].fx = vec![FxSlot {
        command: Some(FxCommand::Del),
        value: 0x0300,
        ..FxSlot::default()
    }];
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let refused = line(&song, &Choice::default()).unwrap_err().to_string();
    assert!(refused.contains("FX on phrase 00's row 01"), "{refused}");

    let mut nudged = acid_song(&["C-3 64 +2"]);
    nudged.tracks[0].name.clear();
    let refused = line(&nudged, &Choice::default()).unwrap_err().to_string();
    assert!(refused.contains("track 1 nudges"), "{refused}");
}

#[test]
fn the_written_notes_play_in_a_probe_spec() {
    let song = acid_song(&["C-3 7F SGL2", "D-3 64 HLD1", "E-3 64"]);
    let out = std::env::temp_dir().join(format!("vt-notes-{}.toml", std::process::id()));
    write(&acid(&song), &out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    std::fs::remove_file(&out).ok();
    let notes = &text[text.find("[[notes]]").expect("the notes are written")..];
    let spec = format!("duration = 1.0\n[ours]\nmachine = \"acid\"\n\n{notes}");
    let loaded = Loaded {
        spec: parse(&spec).expect("the notes read as a probe spec's"),
        folder: PathBuf::new(),
    };
    assert_eq!(loaded.spec.notes.len(), 3);
    assert!(loaded.spec.notes[1].slide);
    prepare(&loaded, None).expect("the spec prepares");
}
