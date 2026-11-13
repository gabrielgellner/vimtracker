//! The generators tutorial's songs, in `docs/manual/src/tutorials/songs`: each is built here
//! through the model, as the chapter builds it by keys, and kept on disk as a project folder the
//! manual links to and the program opens. A test checks each file is what its builder makes, so a
//! change to the song format can't leave them stale, and another plays every one.
//!
//! After a change to the format or to a builder, `just tutorial-songs` writes them again.

use std::path::PathBuf;
use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::accumulator::{Accumulation, AtLimit};
use vt_model::action::{Action, edit};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::modulation::ModKind;
use vt_model::order::Order;
use vt_model::project::{load_song, save_song};
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, Edge, FillKind, Follow, Pink, Shape, ShapeColumn, Source, Values,
    Walk, WaveRate, Where,
};
use vt_model::song::{Lane, Machine, Phrase, Song, SynthVoice};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Note, Step};
use vt_model::steps::StepsMode;

const RATE: u32 = 48_000;

/// Where the songs are kept, a folder each.
fn songs_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/src/tutorials/songs")
}

/// Every song, by its folder's name, in the chapter's order.
fn songs() -> Vec<(&'static str, Song)> {
    vec![
        ("turing-machine", turing_machine()),
        ("deja-vu", deja_vu()),
        ("euclid", euclid()),
        ("fugue", fugue()),
        ("kria", kria()),
        ("metropolix", metropolix()),
        ("live-generators", live_generators()),
        ("acid-lines", acid_lines()),
        ("walks", walks()),
    ]
}

// Building blocks.

/// A song of so many tracks, at a tempo and in a scale.
fn song(name: &str, bpm: u32, root: u8, kind: ScaleKind, tracks: usize) -> Song {
    let mut song = Song::new(tracks);
    song.name = name.to_owned();
    song.millibpm = bpm * 1000;
    song.scale = Scale {
        on: true,
        root,
        kind,
    };
    song
}

/// Give a track a name, a synth voice and its phrase `00`, from rows in the steps' text.
fn voice(song: &mut Song, track: usize, name: &str, voice: SynthVoice, rows: &[&str]) {
    let track = &mut song.tracks[track];
    track.name = name.to_owned();
    track.machine = Machine::Plaits(voice);
    let steps: Vec<Step> = rows.iter().map(|row| row.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
}

/// Set one of a track's settings by the key its file saves it under, such as `acid.cutoff`.
fn set(song: &mut Song, track: usize, key: &str, value: u8) {
    let id = SettingId::from_key(key).unwrap_or_else(|| panic!("no setting {key}"));
    song.tracks[track].set_setting(id, value);
}

/// A rest, and a note at the velocity notes are typed with.
const REST: &str = "--- -- --- --- ---";

/// Rows of one note where `pattern` has an `x`, and rests elsewhere.
fn hits(pattern: &str, note: &str) -> Vec<String> {
    pattern
        .chars()
        .map(|c| match c {
            'x' => format!("{note} 64 --- --- ---"),
            'X' => format!("{note} 7F --- --- ---"),
            _ => REST.to_owned(),
        })
        .collect()
}

fn texts(rows: &[String]) -> Vec<&str> {
    rows.iter().map(String::as_str).collect()
}

/// A quiet ADSR on the note's volume, for a pluck that dies away to `sustain` percent.
fn pluck(song: &mut Song, track: usize, decay_ms: u16, sustain: u8, release_ms: u16) {
    let adsr = &mut song.tracks[track].mods[0].adsr;
    adsr.decay_ms = decay_ms;
    adsr.sustain = sustain;
    adsr.release_ms = release_ms;
}

/// Make a mod slot a `Steps` slot going to `Pitch`, in the scale or not, as the Mod page does,
/// with these values.
#[allow(clippy::too_many_arguments)]
fn steps_on_pitch(
    song: &mut Song,
    track: usize,
    slot: usize,
    mode: StepsMode,
    in_scale: bool,
    depth: i16,
    values: &[i16],
    mutate: u8,
) {
    let slot = &mut song.tracks[track].mods[slot];
    slot.kind = ModKind::Steps;
    slot.destination = Some(SettingId::PITCH);
    slot.in_scale = in_scale;
    slot.depth = depth;
    slot.steps.mode = mode;
    slot.steps.length = values.len() as u8;
    slot.steps.values[..values.len()].copy_from_slice(values);
    slot.steps.mutate = mutate;
}

/// Run an edit on the song, as a key in the program would.
fn apply(song: Song, action: &Action) -> Song {
    let next = edit(&Arc::new(song), action).expect("the edit applies");
    Arc::unwrap_or_clone(next)
}

/// The shape panel as it opens, with these ends and rows chosen.
fn shape(kind: FillKind, from: u8, to: u8, place: Where) -> Shape {
    Shape {
        from,
        to,
        curve: 0,
        s: 0,
        kind,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place,
        seed: 1,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
    }
}

/// Keep a shape live on phrase `00`'s rows, from the first to `rows`, as `L` does in the panel.
fn generate(song: Song, track: usize, column: ShapeColumn, rows: usize, shape: Shape) -> Song {
    apply(
        song,
        &Action::Generate {
            track,
            phrase: 0,
            column,
            span: Span::rows(StepRange::between(0, rows - 1)),
            shape,
        },
    )
}

/// The Note column of a track with no notes, writing at the velocity notes are typed with.
fn empty_notes(song: &Song) -> ShapeColumn {
    ShapeColumn::Note {
        scale: song.scale,
        writes: Some(0x64),
    }
}

/// A note's number, from its name.
fn note(name: &str) -> u8 {
    match format!("{name} -- --- --- ---")
        .parse::<Step>()
        .unwrap()
        .note
    {
        Note::On(number) => number,
        _ => panic!("no note {name}"),
    }
}

/// A kick on every beat, quietly enough to sit under a line.
fn four_on_the_floor(song: &mut Song, track: usize) {
    voice(
        song,
        track,
        "BD",
        SynthVoice::Kick,
        &texts(&hits("x...x...x...x...", "C-1")),
    );
    set(song, track, "kick.decay", 0x70);
    song.tracks[track].level = 0xF0;
}

/// A closed hat on the off-beats.
fn off_beat_hats(song: &mut Song, track: usize) {
    voice(
        song,
        track,
        "HH",
        SynthVoice::Hat,
        &texts(&hits("..x...x...x...x.", "C-4")),
    );
    set(song, track, "hat.decay", 0x30);
    song.tracks[track].level = 0xC8;
}

// The songs.

/// A Turing Machine: a written rhythm, and a Steps slot choosing each note's pitch in the scale,
/// its `Mutate` the big knob.
fn turing_machine() -> Song {
    let mut song = song("turing-machine", 118, 0, ScaleKind::Minor, 3);
    song.seed = 0x7A;
    let mut rows = hits("x.xx.x.xx.xx.xx.", "C-2");
    rows[0] = "C-2 7F --- --- ---".to_owned();
    rows[7] = "C-2 64 HLD1 --- --- ---".to_owned();
    rows[8] = "C-2 7F --- --- ---".to_owned();
    voice(&mut song, 0, "TM", SynthVoice::Acid, &texts(&rows));
    set(&mut song, 0, "acid.cutoff", 0x58);
    set(&mut song, 0, "acid.resonance", 0xB0);
    set(&mut song, 0, "acid.envmod", 0xA0);
    set(&mut song, 0, "acid.decay", 0x60);
    song.tracks[0].level = 0xC4;
    song.tracks[0].send_a = 0xA0;
    // The loop it drifted to when it was kept: root, root, a third, root, the sixth below, the
    // sixth above, root, the fifth, each moving the note by up to an octave.
    steps_on_pitch(
        &mut song,
        0,
        3,
        StepsMode::Note,
        true,
        0x0C,
        &[0x00, 0x00, 0x40, 0x00, -0x55, 0xAA, 0x00, 0x95],
        20,
    );
    four_on_the_floor(&mut song, 1);
    off_beat_hats(&mut song, 2);
    song
}

/// Marbles' Déjà vu: a locked loop of eight notes played in a random order, with a little
/// `Mutate` bringing in new ones, and chance on the hats for its random gates.
fn deja_vu() -> Song {
    let mut song = song("deja-vu", 100, 2, ScaleKind::Dorian, 3);
    song.seed = 0xD7;
    voice(
        &mut song,
        0,
        "X1",
        SynthVoice::String,
        &[
            "D-3 64 --- --- ---",
            "F-3 50 --- --- ---",
            "A-3 64 --- --- ---",
            "C-4 50 --- --- ---",
            "E-4 64 --- --- ---",
            "A-3 50 --- --- ---",
            "G-3 64 --- --- ---",
            "D-4 50 --- --- ---",
        ],
    );
    let mut phrase = (*song.tracks[0].phrases[&0]).clone();
    phrase.set_order(Order::Random);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    set(&mut song, 0, "string.decay", 0xB0);
    set(&mut song, 0, "string.bright", 0x50);
    song.tracks[0].level = 0xC8;
    song.tracks[0].send_a = 0xB0;
    song.tracks[0].send_b = 0xB8;
    steps_on_pitch(&mut song, 0, 3, StepsMode::Note, true, 0x0C, &[0; 8], 12);
    // A drone under it, its filter wandering, as Marbles' Y does.
    voice(
        &mut song,
        1,
        "Y1",
        SynthVoice::VirtualAnalog,
        &["D-1 50 SGL8 --- --- ---", "A-1 40 SGL8 --- --- ---"],
    );
    let drone = &mut song.tracks[1];
    drone.cutoff = 0x60;
    drone.resonance = 0x40;
    drone.level = 0xD0;
    drone.mods[3].destination = Some(SettingId::CUT);
    drone.mods[3].depth = 0x30;
    drone.mods[3].lfo.shape = Wave::Smooth;
    drone.mods[3].lfo.rate = vt_model::lfo::Rate::Note(7680);
    // Marbles' T: a hat on every step, each with its own chance of playing, from the shape panel.
    voice(
        &mut song,
        2,
        "T1",
        SynthVoice::Hat,
        &texts(&hits(&"x".repeat(16), "C-4")),
    );
    let chances = shape(FillKind::Random, 0x50, 0xF0, Where::AllRows);
    let mut song = apply(
        song,
        &Action::Shape {
            track: 2,
            phrase: 0,
            range: StepRange::between(0, 15),
            column: ShapeColumn::Fx {
                slot: 0,
                command: FxCommand::Cha,
            },
            shape: chances,
        },
    );
    set(&mut song, 2, "hat.decay", 0x28);
    song.tracks[2].send_a = 0x90;
    song.tracks[2].level = 0xC0;
    song
}

/// Euclidean rhythms: each drum's hits spread as evenly as they go, kept live so `Rotate` can turn
/// them, with the hats on triplets for a real polyrhythm, and a bass made the same way.
fn euclid() -> Song {
    let mut song = song("euclid", 124, 0, ScaleKind::Minor, 4);
    song.seed = 0xE5;
    let blank = vec![REST; 16];
    voice(&mut song, 0, "BD", SynthVoice::Kick, &blank);
    voice(&mut song, 1, "SD", SynthVoice::Snare, &blank);
    voice(&mut song, 2, "HH", SynthVoice::Hat, &blank[..12]);
    voice(&mut song, 3, "BS", SynthVoice::Acid, &blank);
    let notes = empty_notes(&song);
    let on = |name: &str, hits: u16, rotate: u16| {
        shape(
            FillKind::Ramp,
            note(name),
            note(name),
            Where::Euclid { hits, rotate },
        )
    };
    let mut song = generate(song, 0, notes, 16, on("C-1", 5, 0));
    song = generate(song, 1, notes, 16, on("G-2", 2, 4));
    song = generate(song, 2, notes, 12, on("C-4", 7, 0));
    let mut octaves = shape(
        FillKind::Alternate,
        note("C-2"),
        note("C-3"),
        Where::Euclid { hits: 7, rotate: 2 },
    );
    octaves.run = 1;
    song = generate(song, 3, notes, 16, octaves);
    set(&mut song, 0, "kick.decay", 0x90);
    song.tracks[0].level = 0xF0;
    set(&mut song, 1, "snare.decay", 0x60);
    song.tracks[1].level = 0xCC;
    song.tracks[1].send_b = 0x98;
    song.tracks[2].speed = Speed::EighthTriplet;
    set(&mut song, 2, "hat.decay", 0x38);
    song.tracks[2].level = 0xCC;
    set(&mut song, 3, "acid.cutoff", 0x50);
    set(&mut song, 3, "acid.resonance", 0x90);
    set(&mut song, 3, "acid.envmod", 0x90);
    song.tracks[3].level = 0xD4;
    song
}

/// The fugue's theme, on its first track.
const FUGUE_THEME: [&str; 8] = [
    "A-3 64 --- --- ---",
    "E-4 50 --- --- ---",
    "D-4 64 --- --- ---",
    "C-4 50 --- --- ---",
    "B-3 64 --- --- ---",
    "C-4 50 --- --- ---",
    "G#3 64 --- --- ---",
    "B-3 50 --- --- ---",
];

/// Fugue Machine: one theme on the first track, and three tracks following it live with `Follow`,
/// each at its own speed and turned its own way: a fifth up, inverted and an octave down, and
/// backwards and two rows late. An edit to the theme reaches all three.
fn fugue() -> Song {
    let mut song = song("fugue", 92, 9, ScaleKind::HarmonicMinor, 4);
    song.seed = 0xF0;
    voice(&mut song, 0, "V1", SynthVoice::Fm, &FUGUE_THEME);
    let theme = Source::default();
    let voices = [
        (
            "V2",
            SynthVoice::String,
            Speed::Sixteenth,
            Follow {
                transpose: 4,
                ..Follow::default()
            },
        ),
        (
            "V3",
            SynthVoice::VirtualAnalog,
            Speed::Quarter,
            Follow {
                invert: true,
                transpose: -7,
                ..Follow::default()
            },
        ),
        (
            "V4",
            SynthVoice::Modal,
            Speed::EighthTriplet,
            Follow {
                reverse: true,
                rotate: 2,
                ..Follow::default()
            },
        ),
    ];
    for (track, (name, sound, speed, follow)) in voices.into_iter().enumerate() {
        let track = track + 1;
        voice(&mut song, track, name, sound, &[REST; 8]);
        song.tracks[track].speed = speed;
        let mut answer = shape(FillKind::Follow, 0, 0, Where::AllRows);
        answer.follow = Follow {
            source: theme,
            ..follow
        };
        let notes = empty_notes(&song);
        song = generate(song, track, notes, 8, answer);
    }
    // The fastest voice runs up and down what it follows, as one of Fugue Machine's playheads can.
    let mut phrase = (*song.tracks[1].phrases[&0]).clone();
    phrase.set_order(Order::UpDown);
    song.tracks[1].phrases.insert(0, Arc::new(phrase));
    set(&mut song, 0, "fm.ratio", 0x80);
    set(&mut song, 0, "fm.index", 0x40);
    pluck(&mut song, 0, 400, 40, 200);
    song.tracks[0].level = 0xC8;
    song.tracks[0].send_b = 0xA0;
    set(&mut song, 1, "string.decay", 0x90);
    song.tracks[1].level = 0xC0;
    song.tracks[1].send_a = 0xA0;
    song.tracks[1].pan = 0x60;
    song.tracks[2].cutoff = 0x70;
    pluck(&mut song, 2, 900, 60, 300);
    song.tracks[2].level = 0xD8;
    set(&mut song, 3, "modal.decay", 0x90);
    song.tracks[3].level = 0xD8;
    song.tracks[3].send_b = 0xB0;
    song.tracks[3].pan = 0xA0;
    song
}

/// Kria: each part of a note, its trigger, pitch, octave and length, a loop of its own length,
/// drifting against the others.
fn kria() -> Song {
    let mut song = song("kria", 120, 4, ScaleKind::MinorPentatonic, 4);
    song.seed = 0x4B;
    // The trigger loop, 16 steps, its notes all the scale's root.
    let mut lead = hits("x.x.xx.x.xx.x.xx", "E-3");
    // The length loop, a lane of 4 on FX 1: short, long, a stab, nearly legato.
    for (row, length) in ["20", "C0", "08", "E6"].iter().enumerate() {
        lead[row] = lead[row].replacen(" --- --- ---", &format!(" gate:{length} --- ---"), 1);
    }
    voice(&mut song, 0, "KR", SynthVoice::VirtualAnalog, &texts(&lead));
    let mut phrase = (*song.tracks[0].phrases[&0]).clone();
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 4,
        }),
    );
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    pluck(&mut song, 0, 300, 50, 80);
    song.tracks[0].cutoff = 0x90;
    song.tracks[0].resonance = 0x50;
    song.tracks[0].level = 0xD0;
    song.tracks[0].send_a = 0xA4;
    // The pitch loop, 5 steps, and the octave loop, 3, both running on sixteenths.
    steps_on_pitch(
        &mut song,
        0,
        3,
        StepsMode::Free,
        true,
        0xFF,
        &[0x00, 0x03, 0x07, 0x05, 0x0A],
        0,
    );
    steps_on_pitch(
        &mut song,
        0,
        4,
        StepsMode::Free,
        false,
        0xFF,
        &[0x00, 0x00, 0x0C],
        0,
    );
    // A second track: a trigger loop of 12 and a pitch loop of 7.
    voice(
        &mut song,
        1,
        "BS",
        SynthVoice::Acid,
        &texts(&hits("x..x.xx..x.x", "E-2")),
    );
    set(&mut song, 1, "acid.cutoff", 0x48);
    set(&mut song, 1, "acid.resonance", 0xA0);
    song.tracks[1].level = 0xD0;
    steps_on_pitch(
        &mut song,
        1,
        3,
        StepsMode::Free,
        true,
        0xFF,
        &[0x00, 0x00, 0x07, 0x00, 0x03, 0x00, 0x0A],
        0,
    );
    four_on_the_floor(&mut song, 2);
    // Hats in threes against the fours.
    voice(
        &mut song,
        3,
        "HH",
        SynthVoice::Hat,
        &[
            "C-4 60 --- --- ---",
            "C-4 20 --- --- ---",
            "C-4 38 --- --- ---",
        ],
    );
    set(&mut song, 3, "hat.decay", 0x30);
    song.tracks[3].level = 0xC4;
    song
}

/// The Metropolix: eight stages with their own pulses and gates, an accumulator moving two of
/// them, a pendulum order, and a second track arpeggiating at half the clock.
fn metropolix() -> Song {
    let mut song = song("metropolix", 126, 9, ScaleKind::Minor, 4);
    song.seed = 0x3E;
    voice(
        &mut song,
        0,
        "MX",
        SynthVoice::Acid,
        &[
            "A-1 7F SGL2 --- --- ---",
            "A-1 50 MLT2 --- --- ---",
            "C-2 64 HLD2 --- --- ---",
            "E-2 64 SGL1 acu:01 --- ---",
            "--- -- RST2 --- --- ---",
            "G-1 64 SGL2 --- --- ---",
            "A-1 7F MLT3 --- --- ---",
            "D-2 64 SGL2 acu:FF --- ---",
        ],
    );
    let mut phrase = (*song.tracks[0].phrases[&0]).clone();
    phrase.set_order(Order::UpDown);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song.tracks[0].accumulation = Accumulation {
        up: 4,
        down: 4,
        at_limit: AtLimit::Bounce,
        ..Accumulation::default()
    };
    set(&mut song, 0, "acid.cutoff", 0x50);
    set(&mut song, 0, "acid.resonance", 0xC0);
    set(&mut song, 0, "acid.envmod", 0xB0);
    set(&mut song, 0, "acid.decay", 0x50);
    song.tracks[0].level = 0xC0;
    song.tracks[0].send_a = 0x98;
    // Chords as arpeggios, a stage each half bar on a track at half the clock.
    voice(
        &mut song,
        1,
        "AR",
        SynthVoice::VirtualAnalog,
        &[
            "A-3 50 SGL4 arp:24 --- ---",
            "F-3 50 SGL4 arp:24 --- ---",
            "C-4 50 SGL4 arp:24 --- ---",
            "G-3 50 SGL4 arp:24 --- ---",
        ],
    );
    let arp = &mut song.tracks[1];
    arp.speed = Speed::Eighth;
    arp.arpeggio = Arpeggio {
        rate: ArpRate::ThirtySecond,
        mode: ArpMode::UpDown,
        octaves: 2,
        retrig: true,
    };
    arp.cutoff = 0x88;
    arp.level = 0xE0;
    arp.send_a = 0xB0;
    arp.send_b = 0x98;
    pluck(&mut song, 1, 120, 20, 60);
    four_on_the_floor(&mut song, 2);
    voice(
        &mut song,
        3,
        "HH",
        SynthVoice::Hat,
        &[
            REST,
            REST,
            "C-4 50 --- --- ---",
            REST,
            REST,
            REST,
            "C-4 50 --- --- ---",
            REST,
            REST,
            REST,
            "C-4 50 --- --- ---",
            REST,
            REST,
            REST,
            "C-4 50 MLT2 --- --- ---",
            REST,
        ],
    );
    set(&mut song, 3, "hat.decay", 0x30);
    song.tracks[3].level = 0xC8;
    song
}

/// Live generators: Euclidean accents on the hats, a bass line written by the shape panel, and a
/// filter wave on a lane of its own, all still tied to their settings.
fn live_generators() -> Song {
    let mut song = song("live-generators", 122, 5, ScaleKind::Minor, 4);
    song.seed = 0x1C;
    four_on_the_floor(&mut song, 0);
    voice(
        &mut song,
        1,
        "HH",
        SynthVoice::Hat,
        &texts(&hits(&"x".repeat(16), "C-4")),
    );
    let hats = &mut song.tracks[1];
    for step in 0..16 {
        let mut phrase = (*hats.phrases[&0]).clone();
        phrase.step_mut(step).unwrap().velocity = Some(0x34);
        hats.phrases.insert(0, Arc::new(phrase));
    }
    set(&mut song, 1, "hat.decay", 0x2C);
    song.tracks[1].level = 0xCC;
    voice(&mut song, 2, "BS", SynthVoice::Acid, &[REST; 16]);
    voice(
        &mut song,
        3,
        "SD",
        SynthVoice::Snare,
        &texts(&hits("....x.......x...", "G-2")),
    );
    song.tracks[3].level = 0xC8;
    song.tracks[3].send_b = 0x98;
    let accents = shape(
        FillKind::Ramp,
        0x7F,
        0x7F,
        Where::Euclid { hits: 5, rotate: 0 },
    );
    let mut song = generate(song, 1, ShapeColumn::Velocity, 16, accents);
    let mut line = shape(
        FillKind::Random,
        note("F-1"),
        note("F-2"),
        Where::Euclid { hits: 9, rotate: 0 },
    );
    line.seed = 3;
    let notes = empty_notes(&song);
    song = generate(song, 2, notes, 16, line);
    // A wave on Acid's cutoff, on a lane of 12 steps that drifts against the line.
    song = apply(
        song,
        &Action::SetLane {
            track: 2,
            phrase: 0,
            slot: 0,
            lane: Some(Lane {
                start: 0,
                length: 12,
            }),
        },
    );
    let cutoff = FxCommand::Set(SettingId::from_key("acid.cutoff").unwrap());
    let mut wave = shape(FillKind::Wave, 0x30, 0xA0, Where::AllRows);
    wave.rate = WaveRate::Cycles(1);
    song = apply(
        song,
        &Action::Generate {
            track: 2,
            phrase: 0,
            column: ShapeColumn::Fx {
                slot: 0,
                command: cutoff,
            },
            span: Span::Lane,
            shape: wave,
        },
    );
    set(&mut song, 2, "acid.resonance", 0xB0);
    set(&mut song, 2, "acid.envmod", 0x70);
    song.tracks[2].level = 0xC8;
    song.tracks[2].send_a = 0xA0;
    song
}

/// An acid line from the shape panel's `Acid` fill: notes, accents and slides rolled together on
/// an empty Note column, and kept live so `gl` and `r` can roll another.
fn acid_lines() -> Song {
    let mut song = song("acid-lines", 126, 0, ScaleKind::Minor, 4);
    song.seed = 0xAC;
    voice(&mut song, 0, "TB", SynthVoice::Acid, &[REST; 16]);
    let mut riff = shape(FillKind::Acid, note("C-2"), note("C-3"), Where::AllRows);
    riff.seed = 5;
    let notes = empty_notes(&song);
    let mut song = generate(song, 0, notes, 16, riff);
    set(&mut song, 0, "acid.cutoff", 0x50);
    set(&mut song, 0, "acid.resonance", 0xC0);
    set(&mut song, 0, "acid.envmod", 0xA0);
    set(&mut song, 0, "acid.decay", 0x58);
    song.tracks[0].level = 0xC4;
    song.tracks[0].send_a = 0xA0;
    four_on_the_floor(&mut song, 1);
    off_beat_hats(&mut song, 2);
    voice(
        &mut song,
        3,
        "SD",
        SynthVoice::Snare,
        &texts(&hits("....x.......x...", "G-2")),
    );
    song.tracks[3].level = 0xC8;
    song.tracks[3].send_b = 0x98;
    song
}

/// Wandering lines from the shape panel's `Walk` and `1/f` fills, each kept live: a `1/f` tune,
/// a bass that walks a degree at a time, and hats whose velocity walks.
fn walks() -> Song {
    let mut song = song("walks", 96, 9, ScaleKind::Minor, 4);
    song.seed = 0x1F;
    voice(&mut song, 0, "LD", SynthVoice::String, &[REST; 32]);
    set(&mut song, 0, "string.decay", 0xA0);
    set(&mut song, 0, "string.bright", 0x60);
    song.tracks[0].level = 0xC0;
    song.tracks[0].send_a = 0x90;
    song.tracks[0].send_b = 0xA8;
    voice(&mut song, 1, "BS", SynthVoice::VirtualAnalog, &[REST; 16]);
    song.tracks[1].cutoff = 0x58;
    song.tracks[1].resonance = 0x30;
    song.tracks[1].level = 0xC8;
    voice(
        &mut song,
        2,
        "HH",
        SynthVoice::Hat,
        &texts(&hits(&"x".repeat(16), "C-4")),
    );
    set(&mut song, 2, "hat.decay", 0x2C);
    song.tracks[2].level = 0xCC;
    four_on_the_floor(&mut song, 3);
    // The tune: `1/f` on every other row of two bars, between A-2 and A-4.
    let mut tune = shape(FillKind::Pink, note("A-2"), note("A-4"), Where::Every(2));
    tune.seed = 4;
    let notes = empty_notes(&song);
    let mut song = generate(song, 0, notes, 32, tune);
    // The bass: a walk of a degree at most a note, on a Euclidean 7 of 16.
    let mut bass = shape(
        FillKind::Walk,
        note("A-1"),
        note("A-2"),
        Where::Euclid { hits: 7, rotate: 0 },
    );
    bass.walk = Walk {
        step: 1,
        edge: Edge::Reflect,
    };
    bass.seed = 2;
    let notes = empty_notes(&song);
    song = generate(song, 1, notes, 16, bass);
    // The hats' velocity, a step of up to `10` a hat between `30` and `70`.
    let mut hats = shape(FillKind::Walk, 0x30, 0x70, Where::Notes);
    hats.walk = Walk {
        step: 0x10,
        edge: Edge::Reflect,
    };
    hats.seed = 6;
    song = generate(song, 2, ShapeColumn::Velocity, 16, hats);
    song
}

// The tests.

/// What `save_song` writes for a song, read back as text.
fn saved_text(song: &Song, scratch: &std::path::Path) -> String {
    save_song(scratch, song).unwrap();
    std::fs::read_to_string(scratch.join("song.ron")).unwrap()
}

#[test]
fn each_tutorial_song_on_disk_is_what_its_builder_makes() {
    let scratch = std::env::temp_dir().join(format!("vt-tutorial-{}", std::process::id()));
    let bless = std::env::var_os("VT_BLESS").is_some();
    for (name, song) in songs() {
        let folder = songs_folder().join(name);
        if bless {
            save_song(&folder, &song).unwrap();
            continue;
        }
        let made = saved_text(&song, &scratch);
        let kept = std::fs::read_to_string(folder.join("song.ron"))
            .unwrap_or_else(|_| panic!("{name} is missing: run just tutorial-songs"));
        assert!(
            made == kept,
            "{name} on disk isn't what its builder makes: run just tutorial-songs"
        );
    }
    let _ = std::fs::remove_dir_all(scratch);
}

#[test]
fn every_tutorial_song_loads_plays_and_stays_below_full_scale() {
    let mut names: Vec<String> = std::fs::read_dir(songs_folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), songs().len(), "a folder for each song");
    for name in names {
        let song = load_song(&songs_folder().join(&name)).unwrap();
        assert!(
            song.bank.is_empty(),
            "{name} plays synths only, so it needs no samples"
        );
        let version = Arc::new(Published::new(
            Arc::new(song),
            Arc::new(SamplePool::default()),
        ));
        let out = render_loop(version, RATE, RenderOptions::default());
        assert!(out.len() > RATE as usize, "{name} plays for over a second");
        let peak = out.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.02, "{name} is nearly silent: RMS {rms}");
        assert!(peak < 0.99, "{name} reaches full scale: peak {peak}");
    }
}

#[test]
fn the_fugues_three_voices_follow_its_theme_and_an_edit_to_it_reaches_them() {
    let song = Arc::new(load_song(&songs_folder().join("fugue")).unwrap());
    let notes = |song: &Song, track: usize| -> Vec<String> {
        song.tracks[track].phrases[&0]
            .steps()
            .iter()
            .map(|step| step.to_string()[..3].to_owned())
            .collect()
    };
    // A fifth up in A harmonic minor, four degrees.
    assert_eq!(
        notes(&song, 1),
        ["E-4", "B-4", "A-4", "G#4", "F-4", "G#4", "D-4", "F-4"]
    );
    // Inverted about A-3, then an octave down.
    assert_eq!(
        notes(&song, 2),
        ["A-2", "D-2", "E-2", "F-2", "G#2", "F-2", "B-2", "G#2"]
    );
    // Backwards, then two rows late.
    assert_eq!(
        notes(&song, 3),
        ["E-4", "A-3", "B-3", "G#3", "C-4", "B-3", "C-4", "D-4"]
    );
    // The theme's E up to F: one edit, and every voice has it.
    let edited = edit(
        &song,
        &Action::SetNote {
            track: 0,
            phrase: 0,
            step: 1,
            note: Note::On(note("F-4")),
        },
    )
    .unwrap();
    assert_eq!(notes(&edited, 1)[1], "C-5");
    assert_eq!(notes(&edited, 2)[1], "C-2");
    assert_eq!(notes(&edited, 3)[0], "F-4");
    for track in &edited.tracks[1..4] {
        assert_eq!(track.phrases[&0].generators().len(), 1);
    }
}
