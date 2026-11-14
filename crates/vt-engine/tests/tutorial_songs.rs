//! The generators tutorial's songs, in `docs/manual/src/tutorials/songs`: each is built here
//! through the model, as the chapter builds it by keys, and kept on disk as a project folder the
//! manual links to and the program opens. A test checks each file is what its builder makes, so a
//! change to the song format can't leave them stale, and another plays every one. Each is
//! read-only, so a `:w` in the program can't change the repository's copy by accident (decision
//! 313): `:w` and a folder saves a copy to carry on in.
//!
//! The patches that come with the program (decision 318) are made here too, each from one of
//! the songs' tracks with the macros a module's panel would have, and kept in
//! `crates/vt-model/factory/patches`, which the program reads them from as it's built. A test
//! checks each file is what its builder makes, as the songs are.
//!
//! After a change to the format or to a builder, `just tutorial-songs` writes them again.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::accumulator::{Accumulation, AtLimit};
use vt_model::action::{Action, edit};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio};
use vt_model::bloom::{Bloom, Path};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::macros::{GEN_ZERO, GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::ModKind;
use vt_model::order::{Cartesian, Order, Snake};
use vt_model::preset::{FACTORY_PATCHES, PatchGen, PatchPreset};
use vt_model::project::{load_song, save_song};
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{
    Acid, Bar, Bits, DEFAULT_RATE, Divisions, DrumPart, Edge, FillKind, Follow, GenParam, Grids,
    Mutations, Pink, Random, Role, Shape, ShapeColumn, Source, Values, Walk, WaveRate, Where,
    World, divisions_seed, on_track, track_seed,
};
use vt_model::song::{Lane, Machine, Phrase, Song, SynthVoice, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, Note, Step};
use vt_model::steps::StepsMode;

const RATE: u32 = 48_000;

/// Where the songs are kept, a folder each.
fn songs_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/src/tutorials/songs")
}

/// Every song, by its folder's name, in the chapter's order, each read-only.
fn songs() -> Vec<(&'static str, Song)> {
    let songs = vec![
        ("turing-machine", turing_machine()),
        ("deja-vu", deja_vu()),
        ("euclid", euclid()),
        ("fugue", fugue()),
        ("kria", kria()),
        ("metropolix", metropolix()),
        ("eloquencer", eloquencer()),
        ("live-generators", live_generators()),
        ("acid-lines", acid_lines()),
        ("walks", walks()),
        ("grids", grids()),
        ("random-rhythm", random_rhythm()),
        ("marbles", marbles()),
        ("marbles-t", marbles_t()),
        ("world-rhythms", world_rhythms()),
        ("variations", variations()),
        ("rene", rene()),
        ("bloom", bloom()),
    ];
    songs
        .into_iter()
        .map(|(name, song)| {
            let song = Song {
                read_only: true,
                ..song
            };
            (name, song)
        })
        .collect()
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

/// Put one of track 1's settings, by its key, on a macro with a name, between two ends in the
/// setting's own values, as `M`, `gm` and `:macro` do, with the macro turned to `value`.
fn knob(song: &mut Song, at: usize, name: &str, key: &str, ends: (u16, u16), value: u16) {
    let id = SettingId::from_key(key).unwrap_or_else(|| panic!("no setting {key}"));
    let held = &mut song.tracks[0].macros[at];
    held.name = Some(name.to_owned());
    held.value = value;
    held.maps.push(MacroMap {
        target: MacroTarget::Setting(id),
        min: ends.0,
        max: ends.1,
        curve: 0,
        s: 0,
    });
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
        grids: Grids::default(),
        random: Random::default(),
        vary: 0,
        variation: vt_model::shape::Variation::default(),
        divisions: vt_model::shape::Divisions::default(),
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
    // Its knobs, as the hardware's: `Change` its big knob, from locked to every step new;
    // `Length` its switch, from 2 steps to 16; and `Range` its scale knob, how far the line goes,
    // from none to two octaves either way. Each is turned to where the slot is, so the macros
    // play the loop as it was kept.
    let knobs = [
        ("Change", "mod4.mutate", (0x0000, 0x6400), 0x3300),
        ("Length", "mod4.length", (0x0100, 0x0F00), 0x6D00),
        ("Range", "mod4.depth", (0x8000, 0x8C00), 0x7F00),
    ];
    for (at, (name, key, ends, value)) in knobs.into_iter().enumerate() {
        knob(&mut song, at, name, key, ends, value);
    }
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

/// Winter Modular's Eloquencer: a written line whose rows each have a chance of moving their note
/// and of a longer or shorter gate, and hats with a chance of ratcheting and a velocity that
/// wanders, all with `RND` (decision 323).
fn eloquencer() -> Song {
    let mut song = song("eloquencer", 116, 2, ScaleKind::Minor, 3);
    song.seed = 0xE1;
    let root = "D-3 64 rnd:31 gate:60 rnd:A4";
    voice(
        &mut song,
        0,
        "EQ",
        SynthVoice::VirtualAnalog,
        &[
            root,
            REST,
            "A-3 50 rnd:62 --- ---",
            "F-3 50 --- --- ---",
            root,
            "C-4 50 rnd:A2 --- ---",
            REST,
            "A-3 50 rnd:62 --- ---",
            root,
            REST,
            "E-3 50 rnd:83 --- ---",
            "F-3 50 --- --- ---",
            "G-3 64 rnd:62 gate:40 rnd:A6",
            "A-3 50 rnd:A2 --- ---",
            REST,
            "C-4 50 rnd:F3 --- ---",
        ],
    );
    let line = &mut song.tracks[0];
    line.cutoff = 0x90;
    line.level = 0xD0;
    line.send_a = 0xA0;
    line.send_b = 0x90;
    pluck(&mut song, 0, 260, 45, 80);
    // A hat on each off-beat whose velocity wanders, and one before each beat that ratchets now
    // and then.
    let wanders = "C-4 50 velocitydelta:80 rnd:F3 ---";
    let ratchets = "C-4 40 ret:02 rnd:61 ---";
    voice(
        &mut song,
        1,
        "HH",
        SynthVoice::Hat,
        &[
            REST, REST, wanders, ratchets, REST, REST, wanders, ratchets, REST, REST, wanders,
            ratchets, REST, REST, wanders, ratchets,
        ],
    );
    set(&mut song, 1, "hat.decay", 0x30);
    song.tracks[1].level = 0xC8;
    four_on_the_floor(&mut song, 2);
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

/// A beat from Grids' map, after Mutable Instruments' Grids: `S` across a kick's, a snare's and a
/// hat's tracks, each with one note typed on its first row for its pitch, kept live, each track
/// its own part of the same place on the map, with chaos so its two bars differ; and an `Acid`
/// bass.
fn grids() -> Song {
    let mut song = song("grids", 118, 9, ScaleKind::Minor, 4);
    song.seed = 0x6D;
    let first = |note: &str| {
        let mut rows = vec![format!("{note} 64 --- --- ---")];
        rows.extend((1..32).map(|_| REST.to_owned()));
        rows
    };
    voice(&mut song, 0, "BD", SynthVoice::Kick, &texts(&first("C-1")));
    set(&mut song, 0, "kick.decay", 0x70);
    song.tracks[0].level = 0xF0;
    voice(&mut song, 1, "SD", SynthVoice::Snare, &texts(&first("G-2")));
    song.tracks[1].level = 0xC8;
    song.tracks[1].send_b = 0x90;
    voice(&mut song, 2, "HH", SynthVoice::Hat, &texts(&first("C-4")));
    set(&mut song, 2, "hat.decay", 0x30);
    song.tracks[2].level = 0xC0;
    voice(
        &mut song,
        3,
        "BS",
        SynthVoice::Acid,
        &texts(&hits("x..x..x...x.x...", "A-1")),
    );
    set(&mut song, 3, "acid.cutoff", 0x48);
    set(&mut song, 3, "acid.resonance", 0x90);
    song.tracks[3].level = 0xB8;
    // The beat: X 40, Y 40, the hats busier than the others, and chaos so the bars differ.
    let mut beat = shape(FillKind::Grids, note("C-3"), note("C-4"), Where::AllRows);
    beat.grids = Grids {
        x: 0x40,
        y: 0x40,
        density: [0x80, 0x80, 0xA0],
        chaos: 0xC0,
        part: None,
        ..Grids::default()
    };
    let column = ShapeColumn::Note {
        scale: song.scale,
        writes: None,
    };
    let parts = [DrumPart::Kick, DrumPart::Snare, DrumPart::Hats];
    for (track, part) in parts.into_iter().enumerate() {
        let shape = Shape {
            seed: track_seed(beat.seed, track),
            grids: Grids {
                part: Some(part),
                ..beat.grids
            },
            ..beat
        };
        song = generate(song, track, column, 32, shape);
    }
    song
}

/// After Vermona's randomRHYTHM: three drums at `1/32`, each a live `Divisions` over a bar, the
/// kick channel `A`, a woodblock channel `B` heavy with triplets, and the hats `A+B`, the strikes
/// of both, all from one seed; under a snare on the backbeat and a bass, written.
fn random_rhythm() -> Song {
    let mut song = song("random-rhythm", 108, 9, ScaleKind::Minor, 5);
    song.seed = 0x60;
    let first = |note: &str| {
        let mut rows = vec![format!("{note} 64 --- --- ---")];
        rows.extend((1..32).map(|_| REST.to_owned()));
        rows
    };
    voice(&mut song, 0, "BD", SynthVoice::Kick, &texts(&first("C-1")));
    set(&mut song, 0, "kick.decay", 0x68);
    song.tracks[0].level = 0xF0;
    voice(&mut song, 1, "WB", SynthVoice::Modal, &texts(&first("E-4")));
    song.tracks[1].level = 0xA8;
    song.tracks[1].send_b = 0x70;
    voice(&mut song, 2, "HH", SynthVoice::Hat, &texts(&first("C-4")));
    set(&mut song, 2, "hat.decay", 0x28);
    song.tracks[2].level = 0xB0;
    for track in &mut song.tracks[..3] {
        track.speed = Speed::ThirtySecond;
    }
    voice(
        &mut song,
        3,
        "SD",
        SynthVoice::Snare,
        &texts(&hits("....x.......x...", "G-2")),
    );
    song.tracks[3].level = 0xC0;
    song.tracks[3].send_b = 0x90;
    let bass = [
        "A-1 64 --- --- ---",
        REST,
        REST,
        "A-1 50 --- --- ---",
        REST,
        REST,
        "C-2 64 --- --- ---",
        REST,
        REST,
        REST,
        "A-1 64 --- --- ---",
        REST,
        "G-1 64 --- --- ---",
        REST,
        "E-1 50 --- --- ---",
        REST,
    ];
    voice(&mut song, 4, "BS", SynthVoice::VirtualAnalog, &bass);
    song.tracks[4].cutoff = 0x48;
    song.tracks[4].resonance = 0x40;
    song.tracks[4].level = 0xC8;
    // The panel's sliders: channel A on the beats, with a few eighths and sixteenths; channel B
    // on the triplets, with a few eighths; each three tracks take A, B and A+B, and one seed.
    let mut dice = shape(
        FillKind::Divisions,
        note("C-3"),
        note("C-4"),
        Where::AllRows,
    );
    dice.seed = 13;
    dice.divisions = Divisions {
        chances: [[90, 20, 10, 0, 10, 0], [0, 25, 0, 35, 40, 10]],
        channel: None,
        bar: Bar::FourFine,
    };
    let column = ShapeColumn::Note {
        scale: song.scale,
        writes: None,
    };
    for track in 0..3 {
        let shape = Shape {
            seed: divisions_seed(dice.seed, track),
            divisions: dice.divisions.for_track(track),
            ..dice
        };
        song = generate(song, track, column, 32, shape);
    }
    song
}

/// Shaped random values, after Mutable Instruments' Marbles' X section, each a live `Random`: a
/// tune its `Steps` keeps to a chord's four notes, its `Spread` a bell round the middle of two
/// octaves; a bass at its range's two ends alone, an octave apart, the low one more often; and
/// hats whose velocities gather toward the top.
fn marbles() -> Song {
    let mut song = song("marbles", 112, 2, ScaleKind::Dorian, 4);
    song.seed = 0x3B;
    voice(&mut song, 0, "X1", SynthVoice::String, &[REST; 32]);
    set(&mut song, 0, "string.decay", 0x90);
    set(&mut song, 0, "string.bright", 0x58);
    song.tracks[0].level = 0xC0;
    song.tracks[0].send_a = 0xA0;
    song.tracks[0].send_b = 0xA0;
    voice(&mut song, 1, "X2", SynthVoice::VirtualAnalog, &[REST; 16]);
    song.tracks[1].cutoff = 0x50;
    song.tracks[1].resonance = 0x38;
    song.tracks[1].level = 0xC8;
    voice(
        &mut song,
        2,
        "HH",
        SynthVoice::Hat,
        &texts(&hits(&"x".repeat(16), "C-4")),
    );
    set(&mut song, 2, "hat.decay", 0x2C);
    song.tracks[2].level = 0xC8;
    four_on_the_floor(&mut song, 3);
    // The tune: D, A, F and C, the four most important degrees of D dorian, a Dm7, on every other
    // row of two bars, gathered round the middle of D-3 to D-5.
    let mut tune = shape(FillKind::Random, note("D-3"), note("D-5"), Where::Every(2));
    tune.random = Random {
        spread: 40,
        steps: 4,
        ..Random::default()
    };
    tune.seed = 3;
    let notes = empty_notes(&song);
    let mut song = generate(song, 0, notes, 32, tune);
    // The bass: the two ends alone, D-1 seven notes in ten and D-2 the rest, on a Euclidean 7 of
    // 16.
    let mut bass = shape(
        FillKind::Random,
        note("D-1"),
        note("D-2"),
        Where::Euclid { hits: 7, rotate: 0 },
    );
    bass.random = Random {
        spread: 100,
        bias: -40,
        ..Random::default()
    };
    bass.seed = 2;
    let notes = empty_notes(&song);
    song = generate(song, 1, notes, 16, bass);
    // The hats' velocity, a bell leaning toward the top of `30` to `7F`.
    let mut hats = shape(FillKind::Random, 0x30, 0x7F, Where::Notes);
    hats.random = Random {
        spread: 25,
        bias: 50,
        ..Random::default()
    };
    hats.seed = 6;
    song = generate(song, 2, ShapeColumn::Velocity, 16, hats);
    song
}

/// Marbles' rhythms, after Mutable Instruments' Marbles' T section (decision 321), each the
/// `Where` of a live generator writing notes of its own: a hat and a modal percussion line
/// trading sixteenths on one `Complementary` coin, a kick on a `Markov` eight-row loop, a bass
/// whose `3 states` hold some notes into the next, and a string's notes in `Clusters`.
fn marbles_t() -> Song {
    let mut song = song("marbles-t", 120, 9, ScaleKind::Minor, 5);
    song.seed = 0x7B;
    let blank = vec![REST; 16];
    voice(&mut song, 0, "HH", SynthVoice::Hat, &blank);
    set(&mut song, 0, "hat.decay", 0x2C);
    song.tracks[0].level = 0xB8;
    voice(&mut song, 1, "PC", SynthVoice::Modal, &blank);
    song.tracks[1].level = 0xC0;
    song.tracks[1].send_b = 0x70;
    voice(&mut song, 2, "BD", SynthVoice::Kick, &blank);
    set(&mut song, 2, "kick.decay", 0x70);
    song.tracks[2].level = 0xF0;
    voice(&mut song, 3, "BS", SynthVoice::VirtualAnalog, &blank);
    song.tracks[3].cutoff = 0x48;
    song.tracks[3].resonance = 0x30;
    song.tracks[3].level = 0xC8;
    voice(&mut song, 4, "ST", SynthVoice::String, &blank);
    set(&mut song, 4, "string.decay", 0x80);
    song.tracks[4].level = 0xB0;
    song.tracks[4].send_a = 0x90;
    song.tracks[4].send_b = 0x90;
    let notes = empty_notes(&song);
    // The hat and the percussion: one coin a row, the hat taking 65 rows in a hundred, `S` across
    // both, then the percussion's fill made a `Random` between A-3 and A-4.
    let coin = Where::Complementary {
        share: 65,
        second: false,
    };
    let mut hats = shape(FillKind::Ramp, note("C-4"), note("C-4"), coin);
    hats.seed = 4;
    let mut song = generate(song, 0, notes, 16, on_track(&hats, 0));
    let mut tuned = on_track(&hats, 1);
    tuned.kind = FillKind::Random;
    (tuned.from, tuned.to) = (note("A-3"), note("A-4"));
    song = generate(song, 1, notes, 16, tuned);
    // The kick: Markov at 90%, an eight-row loop that changes now and then.
    let mut kick = shape(FillKind::Ramp, note("C-1"), note("C-1"), Where::Markov(90));
    kick.seed = 4;
    song = generate(song, 2, notes, 16, kick);
    // The bass: three states at 80%, mostly ties, between A-1 and A-2.
    let mut bass = shape(
        FillKind::Random,
        note("A-1"),
        note("A-2"),
        Where::States(80),
    );
    bass.seed = 2;
    song = generate(song, 3, notes, 16, bass);
    // The string: clusters on a third of the rows, a `Random` between A-3 and A-5.
    let mut string = shape(
        FillKind::Random,
        note("A-3"),
        note("A-5"),
        Where::Clusters(35),
    );
    string.seed = 0;
    song = generate(song, 4, notes, 16, string);
    song
}

/// Named rhythms, after Noise Engineering's Zularic Repetitor (decision 327), each a live
/// generator's `Where` writing notes of its own: the son clave's three parts across three tracks,
/// the clave, the bass's tumbao and the conga's open tones, the conga a child; and the 12/8 bembé
/// bell and its four beats on two tracks at `1/8T`, twelve rows a bar, the bell a child that
/// varies each pass.
fn world_rhythms() -> Song {
    let mut song = song("world-rhythms", 100, 9, ScaleKind::Minor, 5);
    song.seed = 0x5A;
    let sixteen = vec![REST; 16];
    let twelve = vec![REST; 12];
    voice(&mut song, 0, "CL", SynthVoice::Modal, &sixteen);
    set(&mut song, 0, "modal.decay", 0x30);
    song.tracks[0].level = 0xA8;
    song.tracks[0].send_b = 0x60;
    voice(&mut song, 1, "BS", SynthVoice::VirtualAnalog, &sixteen);
    song.tracks[1].cutoff = 0x40;
    song.tracks[1].resonance = 0x28;
    song.tracks[1].level = 0xD0;
    voice(&mut song, 2, "CG", SynthVoice::Modal, &sixteen);
    song.tracks[2].level = 0xC0;
    song.tracks[2].send_b = 0x50;
    voice(&mut song, 3, "BL", SynthVoice::Hat, &twelve);
    set(&mut song, 3, "hat.decay", 0x30);
    song.tracks[3].level = 0x98;
    voice(&mut song, 4, "BD", SynthVoice::Kick, &twelve);
    set(&mut song, 4, "kick.decay", 0x60);
    song.tracks[4].level = 0xD8;
    for track in &mut song.tracks[3..] {
        track.speed = Speed::EighthTriplet;
    }
    let notes = empty_notes(&song);
    // The son: `S` across the first three tracks, each the next part, then each fill its own.
    let son = Where::Rhythm {
        rhythm: World::SonClave,
        part: Role::Bell,
        child: 100,
    };
    let mut trio = shape(FillKind::Ramp, note("E-5"), note("E-5"), son);
    trio.seed = 4;
    song = generate(song, 0, notes, 16, on_track(&trio, 0));
    let mut bass = on_track(&trio, 1);
    bass.kind = FillKind::Alternate;
    (bass.from, bass.to) = (note("A-1"), note("E-2"));
    song = generate(song, 1, notes, 16, bass);
    let mut conga = on_track(&trio, 2);
    conga.kind = FillKind::Alternate;
    (conga.from, conga.to) = (note("E-3"), note("A-3"));
    if let Where::Rhythm { child, .. } = &mut conga.place {
        *child = 80;
    }
    song = generate(song, 2, notes, 16, conga);
    // The bembé at `1/8T`: its bell a child at 75%, a new one each pass, and its four beats.
    let bembe = Where::Rhythm {
        rhythm: World::Bembe,
        part: Role::Bell,
        child: 75,
    };
    let mut pair = shape(FillKind::Ramp, note("C-4"), note("C-4"), bembe);
    pair.seed = 3;
    pair.vary = 1;
    song = generate(song, 3, notes, 12, on_track(&pair, 0));
    let mut beats = on_track(&pair, 1);
    (beats.from, beats.to) = (note("C-1"), note("C-1"));
    beats.vary = 0;
    if let Where::Rhythm { child, .. } = &mut beats.place {
        *child = 100;
    }
    song = generate(song, 4, notes, 12, beats);
    song
}

/// An `Acid` line written by hand, and a live `Variation` of it at order 2, a new one every four
/// passes: something like the line, learned from its own notes, accents and slides alone.
fn variations() -> Song {
    let mut song = song("variations", 124, 0, ScaleKind::Minor, 4);
    song.seed = 0x5A;
    let line = [
        "C-2 7F --- --- ---",
        "C-2 50 --- --- ---",
        "C-3 50 HLD1 --- --- ---",
        "C-2 50 --- --- ---",
        REST,
        "D#2 50 --- --- ---",
        "C-2 7F --- --- ---",
        "G-2 50 HLD1 --- --- ---",
        "A#1 50 --- --- ---",
        REST,
        "C-2 50 --- --- ---",
        "C-3 7F --- --- ---",
        REST,
        "G-2 50 HLD1 --- --- ---",
        "D#2 50 --- --- ---",
        "C-2 50 --- --- ---",
    ];
    voice(&mut song, 0, "TB", SynthVoice::Acid, &line);
    set(&mut song, 0, "acid.cutoff", 0x58);
    set(&mut song, 0, "acid.resonance", 0xB0);
    set(&mut song, 0, "acid.envmod", 0x90);
    set(&mut song, 0, "acid.decay", 0x60);
    song.tracks[0].level = 0xC4;
    song.tracks[0].send_a = 0x90;
    let mut variation = shape(FillKind::Variation, 0, 0, Where::AllRows);
    variation.seed = 3;
    variation.vary = 4;
    let column = ShapeColumn::Note {
        scale: song.scale,
        writes: None,
    };
    let mut song = generate(song, 0, column, 16, variation);
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

/// René: sixteen notes read as a 4×4 grid, a line a chord, walked by the Cartesian order's two
/// counters at different rates; a bass of two lines, an octave apart, wound through by a snake
/// down its columns; and a beat.
fn rene() -> Song {
    let mut song = song("rene", 112, 2, ScaleKind::Minor, 4);
    song.seed = 0x5E;
    // A line of the grid each: Dm7, Bbmaj7, Fmaj7 and C7, so X moves through a chord and Y moves
    // to the next.
    voice(
        &mut song,
        0,
        "RN",
        SynthVoice::VirtualAnalog,
        &[
            "D-3 7F --- --- ---",
            "F-3 50 --- --- ---",
            "A-3 64 --- --- ---",
            "C-4 50 --- --- ---",
            "A#2 7F --- --- ---",
            "D-3 50 --- --- ---",
            "F-3 64 --- --- ---",
            "A-3 50 --- --- ---",
            "F-3 7F --- --- ---",
            "A-3 50 --- --- ---",
            "C-4 64 --- --- ---",
            "E-4 50 --- --- ---",
            "C-3 7F --- --- ---",
            "E-3 50 --- --- ---",
            "G-3 64 --- --- ---",
            "A#3 50 --- --- ---",
        ],
    );
    let mut phrase = (*song.tracks[0].phrases[&0]).clone();
    phrase.set_order(Order::Cartesian(Cartesian::new(1, 3)));
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    let lead = &mut song.tracks[0];
    lead.cutoff = 0x90;
    lead.level = 0xD0;
    lead.send_a = 0xB0;
    lead.send_b = 0x90;
    pluck(&mut song, 0, 140, 20, 80);
    // Two lines of four, the second an octave up, read down the columns: root and octave.
    voice(
        &mut song,
        1,
        "BS",
        SynthVoice::Acid,
        &[
            "D-1 7F --- --- ---",
            "D-1 64 --- --- ---",
            "A#0 64 --- --- ---",
            "C-1 64 --- --- ---",
            "D-2 64 --- --- ---",
            "F-1 64 --- --- ---",
            "A#1 7F --- --- ---",
            "C-2 64 --- --- ---",
        ],
    );
    let mut phrase = (*song.tracks[1].phrases[&0]).clone();
    phrase.set_order(Order::Snake(Snake::Columns));
    song.tracks[1].phrases.insert(0, Arc::new(phrase));
    set(&mut song, 1, "acid.cutoff", 0x48);
    set(&mut song, 1, "acid.resonance", 0xA0);
    set(&mut song, 1, "acid.envmod", 0x90);
    song.tracks[1].level = 0xB8;
    four_on_the_floor(&mut song, 2);
    off_beat_hats(&mut song, 3);
    song
}

/// Bloom's trunk, a bar in D dorian.
const BLOOM_TRUNK: [&str; 16] = [
    "D-3 7F --- --- ---",
    REST,
    "A-3 50 --- --- ---",
    "F-3 64 --- --- ---",
    REST,
    "C-4 50 --- --- ---",
    "A-3 64 --- --- ---",
    REST,
    "G-3 7F --- --- ---",
    REST,
    "E-3 50 --- --- ---",
    "F-3 64 --- --- ---",
    REST,
    "D-3 50 --- --- ---",
    "C-3 64 --- --- ---",
    REST,
];

/// Qu-Bit's Bloom: a trunk of a bar on the first track and four branches grown from it with
/// `:bloom 4 2 trunk`, each a new phrase following the one before it live, with two changes; the
/// chain plays the trunk before each branch, eight bars that stray and come home. A bass, a kick
/// and hats under it.
fn bloom() -> Song {
    let mut song = song("bloom", 116, 2, ScaleKind::Dorian, 4);
    song.seed = 0xB1;
    voice(&mut song, 0, "BL", SynthVoice::VirtualAnalog, &BLOOM_TRUNK);
    let scale = song.scale;
    let mut song = apply(
        song,
        &Action::Bloom {
            track: 0,
            chain: 0,
            slot: 0,
            scale,
            bloom: Bloom {
                branches: 4,
                mutation: 2,
                mutate: Mutations::default(),
                path: Path::Trunk,
                seed: 0xB1,
            },
        },
    );
    let lead = &mut song.tracks[0];
    lead.cutoff = 0x88;
    lead.level = 0xC8;
    lead.send_a = 0xA8;
    lead.send_b = 0x90;
    pluck(&mut song, 0, 220, 25, 120);
    voice(
        &mut song,
        1,
        "BS",
        SynthVoice::Acid,
        &texts(&hits("X..x..x...x..x..", "D-1")),
    );
    set(&mut song, 1, "acid.cutoff", 0x40);
    set(&mut song, 1, "acid.resonance", 0x90);
    set(&mut song, 1, "acid.envmod", 0x70);
    song.tracks[1].level = 0xB4;
    four_on_the_floor(&mut song, 2);
    off_beat_hats(&mut song, 3);
    song
}

// The shipped patches.

/// Where the shipped patches are kept, a file each, which the program includes as it's built.
fn patches_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vt-model/factory/patches")
}

/// Put one of a track's settings on a macro with a name, between two ends in the setting's own
/// values, turned to where the setting is, as the first mapping on a macro is, so the patch
/// plays as the song does.
fn panel_knob(track: &mut Track, at: usize, name: &str, key: &str, ends: (u16, u16)) {
    let id = SettingId::from_key(key).unwrap_or_else(|| panic!("no setting {key}"));
    let map = MacroMap {
        target: MacroTarget::Setting(id),
        min: ends.0,
        max: ends.1,
        curve: 0,
        s: 0,
    };
    let value = map.macro_for(track.value_of(id));
    let held = &mut track.macros[at];
    held.name = Some(name.to_owned());
    held.value = value;
    held.maps.push(map);
}

/// Put a setting of the live generator on a column on a macro with a name, between two of the
/// setting's own numbers, turned to `number`, where the generator has it.
fn gen_knob(
    track: &mut Track,
    at: usize,
    name: &str,
    (column, param): (GenColumn, GenParam),
    ends: (i32, i32),
    number: i32,
) {
    let map = MacroMap::numbers(MacroTarget::Gen { column, param }, ends);
    let value = map.macro_for((number + GEN_ZERO) as u16);
    assert_eq!(
        map.number_at(f32::from(value) / 256.0),
        number,
        "{name} turned to where its generator is"
    );
    let held = &mut track.macros[at];
    held.name = Some(name.to_owned());
    held.value = value;
    held.maps.push(map);
}

/// A track's phrase `00`.
fn first_phrase(track: &Track) -> &Phrase {
    &track.phrases[&0]
}

/// A track's patch, with the live generators its macros hold on phrase `00`.
fn patch_of(track: &Track) -> PatchPreset {
    PatchPreset::of(track, Some(first_phrase(track))).expect("its macros hold something")
}

/// The shape of the first live generator on phrase `00`.
fn first_shape(track: &Track) -> Shape {
    first_phrase(track).generators()[0].shape
}

/// Euclid's hits and rotation in a shape.
fn euclid_of(shape: Shape) -> (i32, i32) {
    match shape.place {
        Where::Euclid { hits, rotate } => (i32::from(hits), i32::from(rotate)),
        place => panic!("not Euclid: {place:?}"),
    }
}

/// `Hits` and `Rotate` on the first two macros, for a Euclidean generator on a column.
fn euclid_knobs(track: &mut Track, column: GenColumn) {
    let (hits, rotate) = euclid_of(first_shape(track));
    gen_knob(track, 0, "Hits", (column, GenParam::Hits), (0, 16), hits);
    gen_knob(
        track,
        1,
        "Rotate",
        (column, GenParam::Rotate),
        (0, 15),
        rotate,
    );
}

/// The Turing Machine: the song's Steps slot with its three macros, `Change`, `Length` and
/// `Range`, as they are.
fn turing_machine_patch() -> PatchPreset {
    patch_of(&turing_machine().tracks[0])
}

/// Déjà vu: the song's Steps slot, with `Deja vu` turning its `Mutate` from all new to locked,
/// so up is more memory, as the module's knob is from its left to noon; and `Spread` its
/// `Depth`, how far a new note goes, from none to two octaves either way. The song's
/// `:order random` is its phrase's, which a patch doesn't carry.
fn deja_vu_patch() -> PatchPreset {
    let mut track = deja_vu().tracks[0].clone();
    panel_knob(&mut track, 0, "Deja vu", "mod4.mutate", (0x6400, 0x0000));
    panel_knob(&mut track, 1, "Spread", "mod4.depth", (0x8000, 0x8C00));
    patch_of(&track)
}

/// Kria: the pitch loop and the octave loop, `Steps` slots in `Free`, each with its loop's
/// length on a macro from 1 step to 16; and the length loop, the lane of 4 on FX 1, made live as
/// the `List` of its four values, which comes with its lane though no macro holds it, as a lane's
/// length isn't a setting to turn.
fn kria_patch() -> PatchPreset {
    let song = kria();
    let written = first_phrase(&song.tracks[0]).steps().to_vec();
    let gate = FxCommand::Set(SettingId::from_key("gate").unwrap());
    let mut lengths = shape(FillKind::List, 0, 0, Where::AllRows);
    lengths.values = Values::new(&[0x20, 0xC0, 0x08, 0xE6]);
    let song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Fx {
                slot: 0,
                command: gate,
            },
            span: Span::Lane,
            shape: lengths,
        },
    );
    let mut track = song.tracks[0].clone();
    assert_eq!(
        first_phrase(&track).steps(),
        written,
        "the live length loop writes what the song has"
    );
    panel_knob(&mut track, 0, "Pitch loop", "mod4.length", (0x0000, 0x0F00));
    panel_knob(
        &mut track,
        1,
        "Octave loop",
        "mod5.length",
        (0x0000, 0x0F00),
    );
    let mut preset = patch_of(&track);
    let phrase = first_phrase(&track);
    preset
        .patch
        .generators
        .push(PatchGen::of(&phrase.generators()[0], phrase));
    preset
}

/// Euclid: the bass's `Alternate` of `C-2` and `C-3` on Euclid 7 of 16, with `Hits` and
/// `Rotate`.
fn euclid_patch() -> PatchPreset {
    let mut track = euclid().tracks[3].clone();
    euclid_knobs(&mut track, GenColumn::Note);
    patch_of(&track)
}

/// Accents: the hats' Velocity, `7F` on Euclid 5 of their notes, with `Hits` and `Rotate`.
fn accents_patch() -> PatchPreset {
    let mut track = live_generators().tracks[1].clone();
    euclid_knobs(&mut track, GenColumn::Velocity);
    patch_of(&track)
}

/// An acid line: the `Acid` fill from `C-2` to `C-3`, with its five dice on macros, each over
/// all it takes: `Density`, `Notes`, `Octave`, `Accent` and `Slide`.
fn acid_line_patch() -> PatchPreset {
    let mut track = acid_lines().tracks[0].clone();
    let acid = first_shape(&track).acid;
    let knobs = [
        ("Density", GenParam::Density, (0, 100), acid.density),
        ("Notes", GenParam::Notes, (1, 8), acid.notes),
        ("Octave", GenParam::Octave, (0, 100), acid.octave),
        ("Accent", GenParam::Accent, (0, 100), acid.accent),
        ("Slide", GenParam::Slide, (0, 100), acid.slide),
    ];
    for (at, (name, param, ends, number)) in knobs.into_iter().enumerate() {
        let knob = (GenColumn::Note, param);
        gen_knob(&mut track, at, name, knob, ends, i32::from(number));
    }
    patch_of(&track)
}

/// A drunk walk: the walks' bass, a `Walk` of a degree at most on Euclid 7 of 16, with `Step`
/// from a degree to a leap of up to seven, an octave in a scale of seven, and `Hits`.
fn drunk_walk_patch() -> PatchPreset {
    let mut track = walks().tracks[1].clone();
    let shape = first_shape(&track);
    let step = i32::from(shape.walk.step);
    gen_knob(
        &mut track,
        0,
        "Step",
        (GenColumn::Note, GenParam::Step),
        (1, 7),
        step,
    );
    let (hits, _) = euclid_of(shape);
    gen_knob(
        &mut track,
        1,
        "Hits",
        (GenColumn::Note, GenParam::Hits),
        (0, 16),
        hits,
    );
    patch_of(&track)
}

/// Marbles' X: the marbles' tune, a `Random` on every other row, with its `Spread`, `Bias` and
/// `Steps`, the module's three knobs, `Steps` from the root alone to all seven degrees.
fn marbles_x_patch() -> PatchPreset {
    let mut track = marbles().tracks[0].clone();
    let random = first_shape(&track).random;
    let knobs = [
        (
            "Spread",
            GenParam::Spread,
            (0, 100),
            i32::from(random.spread),
        ),
        ("Bias", GenParam::Bias, (-100, 100), i32::from(random.bias)),
        ("Steps", GenParam::Steps, (1, 7), i32::from(random.steps)),
    ];
    for (at, (name, param, ends, number)) in knobs.into_iter().enumerate() {
        gen_knob(&mut track, at, name, (GenColumn::Note, param), ends, number);
    }
    patch_of(&track)
}

/// Variations: the `Variation` the song keeps live over its own line, with its `Order` and its
/// `Temperature`.
fn variations_patch() -> PatchPreset {
    let mut track = variations().tracks[0].clone();
    let variation = first_shape(&track).variation;
    let order = i32::from(variation.order);
    let temperature = i32::from(variation.temperature);
    gen_knob(
        &mut track,
        0,
        "Order",
        (GenColumn::Note, GenParam::Order),
        (1, 3),
        order,
    );
    let knob = (GenColumn::Note, GenParam::Temperature);
    gen_knob(&mut track, 1, "Temperature", knob, (0, 100), temperature);
    patch_of(&track)
}

/// Every shipped patch, by its name, in the chapter's order, each tagged `generative` and with
/// what it acts on, as the study's §5.4 has them.
fn patches() -> Vec<(&'static str, PatchPreset)> {
    let patches = [
        ("Turing Machine", turing_machine_patch(), "pitch"),
        ("Deja vu", deja_vu_patch(), "pitch"),
        ("Euclid", euclid_patch(), "rhythm"),
        ("Kria", kria_patch(), "pitch"),
        ("Accents", accents_patch(), "accent"),
        ("Acid line", acid_line_patch(), "303"),
        ("Drunk walk", drunk_walk_patch(), "pitch"),
        ("Marbles X", marbles_x_patch(), "pitch"),
        ("Variations", variations_patch(), "pitch"),
    ];
    patches
        .into_iter()
        .map(|(name, preset, acts_on)| {
            let tags = vec!["generative".to_owned(), acts_on.to_owned()];
            (name, PatchPreset { tags, ..preset })
        })
        .collect()
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
fn each_tutorial_songs_patch_on_disk_is_what_its_builder_makes() {
    let scratch = std::env::temp_dir().join(format!("vt-factory-{}", std::process::id()));
    let bless = std::env::var_os("VT_BLESS").is_some();
    let made = patches();
    for (name, preset) in &made {
        let path = patches_folder().join(format!("{name}.ron"));
        if bless {
            vt_model::preset::save(&path, preset, true).unwrap();
            continue;
        }
        let fresh = scratch.join(format!("{name}.ron"));
        vt_model::preset::save(&fresh, preset, true).unwrap();
        let kept = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{name} is missing: run just tutorial-songs"));
        assert!(
            std::fs::read_to_string(&fresh).unwrap() == kept,
            "{name} on disk isn't what its builder makes: run just tutorial-songs"
        );
    }
    let _ = std::fs::remove_dir_all(scratch);
    if bless {
        return;
    }
    let names: Vec<&str> = made.iter().map(|(name, _)| *name).collect();
    let shipped: Vec<&str> = FACTORY_PATCHES.iter().map(|(name, _)| *name).collect();
    assert_eq!(shipped, names, "the program ships each, in this order");
    let mut files: Vec<String> = std::fs::read_dir(patches_folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut wanted: Vec<String> = names.iter().map(|name| format!("{name}.ron")).collect();
    wanted.sort();
    assert_eq!(files, wanted, "a file for each, and no others");
}

#[test]
fn every_tutorial_song_is_read_only_loads_plays_and_stays_below_full_scale() {
    let mut names: Vec<String> = std::fs::read_dir(songs_folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), songs().len(), "a folder for each song");
    for name in names {
        let song = load_song(&songs_folder().join(&name)).unwrap();
        assert!(song.read_only, "{name} is read-only, so :w can't change it");
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

/// Their live generators rest, so the engine plays each as written, sample for sample as before
/// it could render them (decision 288).
#[test]
fn every_tutorial_song_renders_as_it_did_before_the_engine_ran_generators() {
    for (name, song) in songs() {
        let version = Arc::new(Published::new(
            Arc::new(song),
            Arc::new(SamplePool::default()),
        ));
        let out = render_loop(version, RATE, RenderOptions::default());
        support::golden(&format!("tutorial-{name}"), &out);
    }
}

/// In `marbles-t`, the hat and the percussion toss one coin, so every row is one's or the
/// other's, and a turn of the share on both keeps it so, as the chapter's first thing to try
/// says.
#[test]
fn marbles_t_hat_and_percussion_take_every_row_between_them() {
    let song = load_song(&songs_folder().join("marbles-t")).unwrap();
    let plays = |song: &Song, track: usize| -> Vec<bool> {
        song.tracks[track].phrases[&0]
            .steps()
            .iter()
            .map(|step| matches!(step.note, Note::On(_)))
            .collect()
    };
    let between = |song: &Song| {
        let (hat, percussion) = (plays(song, 0), plays(song, 1));
        hat.iter().zip(&percussion).all(|(a, b)| a != b)
    };
    assert!(between(&song));
    let mut turned = song.clone();
    for track in 0..2 {
        let mut shape = turned.tracks[track].phrases[&0].generators()[0].shape;
        if let Where::Complementary { share, .. } = &mut shape.place {
            *share = 30;
        }
        turned = apply(
            turned,
            &Action::Reshape {
                track,
                phrase: 0,
                generator: 0,
                shape,
            },
        );
    }
    assert!(between(&turned));
    assert!(plays(&turned, 0).iter().filter(|hit| **hit).count() < 8);
}

/// In `world-rhythms`, the clave, bass and conga are the son's three parts as written, the conga a
/// child of its open tones, and the bembé's bell a child of its twelve pulses over its four
/// beats, as the chapter's table shows.
#[test]
fn world_rhythms_parts_are_their_patterns_and_the_children_play_some_of_them() {
    let song = load_song(&songs_folder().join("world-rhythms")).unwrap();
    let plays = |track: usize| -> String {
        song.tracks[track].phrases[&0]
            .steps()
            .iter()
            .map(|step| match step.note {
                Note::On(_) => 'x',
                _ => '.',
            })
            .collect()
    };
    assert_eq!(plays(0), "x..x..x...x.x...");
    assert_eq!(plays(1), "...x..x....x..x.");
    assert_eq!(plays(4), "x..x..x..x..");
    let within = |child: String, mother: &str| {
        let fewer = child.matches('x').count() < mother.matches('x').count();
        let inside = child
            .chars()
            .zip(mother.chars())
            .all(|(a, b)| a == '.' || b == 'x');
        fewer && inside && child.contains('x')
    };
    assert!(within(plays(2), "......xx......xx"), "{}", plays(2));
    assert!(within(plays(3), "x.x.xx.x.x.x"), "{}", plays(3));
    assert_eq!(song.tracks[3].speed, Speed::EighthTriplet);
}

/// The variation is something like the line under it, and order 3 at temperature 0 gives the
/// line back, as the chapter's first thing to try says.
#[test]
fn the_variation_is_like_its_line_and_order_3_cold_writes_the_line_back() {
    let song = Arc::new(load_song(&songs_folder().join("variations")).unwrap());
    let phrase = &song.tracks[0].phrases[&0];
    let generator = &phrase.generators()[0];
    let line: Vec<String> = generator
        .under(phrase.steps())
        .iter()
        .map(ToString::to_string)
        .collect();
    let written: Vec<String> = phrase.steps().iter().map(ToString::to_string).collect();
    assert_ne!(written, line, "a variation, not the line");
    assert!(written.iter().all(|row| line.contains(row)), "{written:?}");
    let shape = Shape {
        variation: vt_model::shape::Variation {
            order: 3,
            temperature: 0,
        },
        ..generator.shape
    };
    let close = edit(
        &song,
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape,
        },
    )
    .unwrap();
    let back: Vec<String> = close.tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(back, line);
}

/// The kick plays channel A and the woodblock B, each with triplets nudged between the rows,
/// and the hats exactly the strikes of both, as the chapter's bar shows.
#[test]
fn the_random_rhythms_hats_play_the_kicks_and_the_woodblocks_strikes() {
    let song = load_song(&songs_folder().join("random-rhythm")).unwrap();
    let strikes = |track: usize| -> Vec<(usize, i8)> {
        song.tracks[track].phrases[&0]
            .steps()
            .iter()
            .enumerate()
            .filter(|(_, step)| matches!(step.note, Note::On(_)))
            .map(|(row, step)| (row, step.nudge))
            .collect()
    };
    let drawn = |track: usize| -> String {
        let mut bar = vec!['.'; 32];
        for (row, nudge) in strikes(track) {
            bar[row] = match nudge {
                0 => 'x',
                nudge if nudge < 0 => '<',
                _ => '>',
            };
        }
        bar.into_iter().collect()
    };
    assert_eq!(drawn(0), "x.....x.x.......x.......x.......");
    assert_eq!(drawn(1), "x..<x......<x>..x.......x..<.>..");
    assert_eq!(drawn(2), "x..<x.x.x..<x>..x.......x..<.>..");
    let mut both = strikes(0);
    both.extend(strikes(1));
    both.sort();
    both.dedup_by_key(|(row, _)| *row);
    assert_eq!(strikes(2), both);
    let channels: Vec<_> = (0..3)
        .map(|track| {
            song.tracks[track].phrases[&0].generators()[0]
                .shape
                .divisions
                .channel
        })
        .collect();
    use vt_model::shape::Channel;
    assert_eq!(
        channels,
        [Some(Channel::A), Some(Channel::B), Some(Channel::Both)]
    );
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

/// The chapter's Bloom: each branch follows the one before it and strays further from the trunk,
/// the chain plays the trunk before each, and an edit to the trunk reaches every branch.
#[test]
fn the_blooms_branches_stray_further_and_an_edit_to_the_trunk_reaches_them() {
    let song = Arc::new(load_song(&songs_folder().join("bloom")).unwrap());
    let rows = |song: &Song, phrase: u32| -> Vec<String> {
        song.tracks[0].phrases[&phrase]
            .steps()
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    let apart = |a: &[String], b: &[String]| a.iter().zip(b).filter(|(a, b)| a != b).count();
    let chain: Vec<u32> = song.tracks[0].chains[&0]
        .entries
        .iter()
        .map(|entry| entry.phrase)
        .collect();
    assert_eq!(chain, [0, 1, 0, 2, 0, 3, 0, 4]);
    let trunk = rows(&song, 0);
    let mut strayed = Vec::new();
    for branch in 1..=4 {
        let generator = &song.tracks[0].phrases[&branch].generators()[0];
        assert_eq!(generator.source().unwrap().phrase, branch - 1);
        assert!(apart(&rows(&song, branch), &rows(&song, branch - 1)) <= 2 * 4);
        strayed.push(apart(&rows(&song, branch), &trunk));
    }
    assert!(strayed[3] > strayed[0], "{strayed:?}");
    // The trunk's G-3 on row 8 down to an F-3: every branch plays it.
    let edited = edit(
        &song,
        &Action::SetNote {
            track: 0,
            phrase: 0,
            step: 8,
            note: Note::On(note("F-3")),
        },
    )
    .unwrap();
    for branch in 1..=4 {
        assert_eq!(&rows(&edited, branch)[8][..3], "F-3", "branch {branch}");
    }
}
