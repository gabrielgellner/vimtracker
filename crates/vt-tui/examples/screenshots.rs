//! Make the manual's screenshots: `just screenshots`, or with a folder to write them to.
//!
//! Each shot builds a song, presses keys as a player would, plays a few steps if it needs the
//! playhead, and writes the screen as an SVG, with the images the terminal would be sent in it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash, Loop, Points};
use vt_model::insert::Insert;
use vt_model::preset::{PresetKind, SoundPreset, preset_path, set_tags};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song, SynthVoice, TrackKind};
use vt_tui::app::App;
use vt_tui::graphics::{DEFAULT_CELL, Graphics};
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;
use vt_tui::overlays::picker::explorer::Purpose;
use vt_tui::overlays::picker::{Picker, Root};
use vt_tui::run::draw_screen;
use vt_tui::screenshot::{sent_images, sent_placements, to_svg_placed};
use vt_tui::services::audio::DeviceInfo;

const RATE: u32 = 48_000;
/// Frames in one step at 120 BPM.
const STEP: usize = 6000;
/// The screen's size in cells.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 24;
/// The project page's, which is taller, with its groove under its settings.
const PROJECT_HEIGHT: u16 = 32;
/// The sample page's, the track pages' and the mixer's shots are of the author's screen, kitty
/// full screen on a MacBook, where each track page's four rows of devices fit beside the main
/// meter.
const FULL_WIDTH: u16 = 163;
const FULL_HEIGHT: u16 = 42;

const SMALL: (u16, u16) = (WIDTH, HEIGHT);
const PROJECT: (u16, u16) = (WIDTH, PROJECT_HEIGHT);
const FULL: (u16, u16) = (FULL_WIDTH, FULL_HEIGHT);

/// One picture for the manual.
struct Shot {
    name: &'static str,
    /// The first track's phrase, one row of step text each.
    rows: &'static [&'static str],
    /// Keys as typed, with `<Esc>`, `<Enter>` and `<Tab>` for those keys.
    keys: &'static str,
    /// Steps to play after the keys, to show the playhead.
    play: usize,
    /// The synth the first track plays, so the meter and the scope show its sound, or its sample.
    voice: Option<SynthVoice>,
    /// Whether the first track plays a kick drum, so its Sound page shows a waveform.
    kick: bool,
    /// Whether the first track plays a drum loop, analysed, with a start, an end and a loop, for
    /// the sample page.
    drums: bool,
    /// The screen's size in cells.
    size: (u16, u16),
    /// What else the song needs before the app starts on it, as the sends to a return.
    song: Option<fn(&mut Song)>,
    /// What else the app needs before the keys, as the audio devices it lists.
    app: Option<fn(&mut App)>,
}

/// What a shot is unless it says otherwise: the gates' phrase, played by a sampler with nothing
/// in it, with nothing pressed, on the small screen.
const PLAIN: Shot = Shot {
    name: "",
    rows: GATES,
    keys: "",
    play: 0,
    voice: None,
    kick: false,
    drums: false,
    size: SMALL,
    song: None,
    app: None,
};

const GATES: &[&str] = &[
    "C-3 64 HLD3 --- --- ---",
    "E-3 64 --- --- ---",
    "G-3 50 MLT2 80 --- --- ---",
    "C-4 64 SGL0 --- --- ---",
    "--- -- RST2 --- --- ---",
    "A-2 64 -2 --- --- ---",
];

const LANE: &[&str] = &[
    "C-2 64 sna:20h --- ---",
    "--- -- sna:60h --- ---",
    "--- -- sna:A0h --- ---",
    "--- -- --- --- ---",
    "C-2 64 --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "G-2 64 --- --- ---",
];

const SHOTS: &[Shot] = &[
    // Zoomed in once, so the start, the end and the loop all show.
    Shot {
        name: "sample",
        keys: " 0zilllllzc",
        drums: true,
        size: FULL,
        ..PLAIN
    },
    Shot {
        name: "gates",
        keys: "lll",
        ..PLAIN
    },
    Shot {
        name: "gate-list",
        keys: "lllK",
        ..PLAIN
    },
    Shot {
        name: "lane",
        rows: LANE,
        keys: "lllll:lane 3<Enter><Tab>",
        play: 21,
        voice: Some(SynthVoice::VirtualAnalog),
        ..PLAIN
    },
    // The same lane turned from the track, `FX1 Length` at `+2` and `Rate` at `÷2`: the title
    // shows what's heard and its own, and its playhead the row a copy reads.
    Shot {
        name: "lane-moved",
        rows: LANE,
        keys: "lllll:lane 3<Enter><Tab>",
        play: 21,
        voice: Some(SynthVoice::VirtualAnalog),
        song: Some(lane_moved),
        ..PLAIN
    },
    // The Gen page's `FX1 Lane` for that lane, its `Length` turned to `+2` and `Rate` to `÷2`.
    Shot {
        name: "track-gen-lane",
        rows: LANE,
        keys: "lllll:lane 3<Enter> 8}==l-",
        play: 21,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        ..PLAIN
    },
    Shot {
        name: "track-sound",
        keys: " 5jli60<Enter>ji90<Enter>k",
        kick: true,
        size: FULL,
        ..PLAIN
    },
    Shot {
        name: "track-fx",
        rows: LANE,
        keys: " 5LLKComp<Enter>l==hji40}KEQ3<Enter>jjiD0li50}KLimit<Enter><Tab>",
        play: 6,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        ..PLAIN
    },
    // The FX page zoomed in on a Glue keyed from a kick on the next track, after two bars and a
    // half of playing, so its last two bars of pumping fill its history (`sidechained`).
    Shot {
        name: "track-fx-zoom",
        rows: LANE,
        keys: " 7zi<Tab>",
        play: 40,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        song: Some(sidechained),
        ..PLAIN
    },
    // The FX page zoomed in on a comb a fifth over a bass, after the four filter effects
    // (`filtering`): its peaks on the note it rings at and its harmonics.
    Shot {
        name: "track-fx-zoom-comb",
        rows: LANE,
        keys: " 7zi}}<Tab>",
        play: 40,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        song: Some(filtering),
        ..PLAIN
    },
    Shot {
        name: "project",
        keys: " 9jjj=l+++:set scale=dorian<Enter>:groove pull<Enter>",
        size: PROJECT,
        ..PLAIN
    },
    // The project page's second page, with two outputs to choose from.
    Shot {
        name: "project-audio",
        keys: " 9L",
        size: PROJECT,
        app: Some(two_outputs),
        ..PLAIN
    },
    // Return B, past the four tracks, group 1 and return A, with what sends to it under its
    // Output (`sends_to_b`).
    Shot {
        name: "track-return",
        keys: " 5WWWWWWgg0j<Tab>",
        play: 8,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        song: Some(sends_to_b),
        ..PLAIN
    },
    // `Space` and a wait: the popup of what can come next, the views' digits among it, on the
    // author's screen, where it fits whole. The steps played are only the wait, as nothing is
    // playing.
    Shot {
        name: "which-key",
        keys: " ",
        play: 4,
        size: FULL,
        ..PLAIN
    },
    // Mod 1's envelope shaped, and Mod 4's LFO sent to the cutoff.
    Shot {
        name: "track-mod",
        keys: " 5Lji20<Enter>ji300<Enter>ji400<Enter>lkki60<Enter>ggllll$K/cut<Enter>0jjji+30<Enter><Tab>",
        play: 3,
        size: FULL,
        ..PLAIN
    },
    // Clouds in the first slot, a little larger grains, playing what comes in.
    Shot {
        name: "track-clouds",
        rows: LANE,
        keys: " 7KClouds<Enter>jjiC0<Enter><Tab>",
        play: 12,
        voice: Some(SynthVoice::VirtualAnalog),
        size: FULL,
        ..PLAIN
    },
    // An acid line made live on the empty phrase from the shape panel; `Cut` and `Res` on macro
    // 1, named `Bite`, from the Sound page; then the Gen page, with the acid's `Spread` on macro
    // 2, `Dense`, with its `Vary` too, in its title, where a bar can't show the macro, and playing.
    Shot {
        name: "track-gen",
        rows: &[],
        keys: "vGSK/acid<Enter>L 5}ji60M1jM1:macro Bite<Enter> 8}kj$M2:macro Dense<Enter>k$M2<Tab>",
        play: 6,
        voice: Some(SynthVoice::Acid),
        size: FULL,
        ..PLAIN
    },
    // The same, then macro 1's mapping from its place in the band: `Res` kept below `A0`, and
    // `Cut`'s response bent to come early.
    Shot {
        name: "mapping",
        rows: &[],
        keys: "vGSK/acid<Enter>L 5}ji60M1jM1:macro Bite<Enter> 8}kj$M2:macro Dense<Enter>gg0gmjliA0kl++",
        voice: Some(SynthVoice::Acid),
        size: FULL,
        ..PLAIN
    },
    // The same macros from the phrase view, in the macro panel at the bottom: the keys in it on
    // macro 2, `Dense`, turned down, and playing.
    Shot {
        name: "macro-panel",
        rows: &[],
        keys: "vGSK/acid<Enter>L 5}ji60M1jM1:macro Bite<Enter> 8}kj$M2:macro Dense<Enter>k$M2 3 Tl__<Tab>",
        play: 6,
        voice: Some(SynthVoice::Acid),
        size: FULL,
        ..PLAIN
    },
    // The shape panel's random fill on an empty Note column in C minor pentatonic, its `Weights`
    // gone into with `l` and `F08` typed, so the cursor is on the fourth digit, G's, each digit
    // over its note.
    Shot {
        name: "shape-weights",
        rows: &[],
        keys: ":set scale=minor-pentatonic<Enter>vGSKrandom<Enter>jjjjjjlf08",
        size: FULL,
        ..PLAIN
    },
    // The same acid's Sound page with the macro panel showing, and `Space d`'s line above it
    // describing `Res`, the cursor's setting, which macro 1 holds.
    Shot {
        name: "description",
        rows: &[],
        keys: "vGSK/acid<Enter>L 5}ji60M1jM1:macro Bite<Enter> t d",
        voice: Some(SynthVoice::Acid),
        size: FULL,
        ..PLAIN
    },
    // A band of seven tracks, the drums folded into their group, `z a` from the kick, playing.
    Shot {
        name: "mixer-folded",
        rows: &[],
        keys: " 4za<Tab>",
        play: 10,
        size: FULL,
        song: Some(band),
        ..PLAIN
    },
];

/// The name the drum loop is played by, and the content it's analysed as.
const DRUMS: &str = "drums.wav";
const DRUMS_HASH: ContentHash = ContentHash(0xD2D2);

/// Two bars of drums at 120 BPM, made here so the screenshot needs no file: a kick on each beat, a
/// snare on two and four, and a closed hat between, in stereo with the hat to the right.
fn drums() -> Decoded {
    let beat = RATE as usize / 2;
    let mut noise = vt_model::random::NumericalRecipesLcg::new(0x1234_5678);
    let mut samples = Vec::with_capacity(beat * 8 * 2);
    for index in 0..beat * 8 {
        let white = (noise.next_u32() >> 8) as f32 / (1 << 23) as f32 - 1.0;
        let in_beat = (index % beat) as f32 / RATE as f32;
        let in_half = (index % (beat / 2)) as f32 / RATE as f32;
        let kick = (std::f32::consts::TAU * 55.0 * in_beat).sin() * (-in_beat * 9.0).exp();
        let snare = if (index / beat) % 2 == 1 {
            white * 0.5 * (-in_beat * 14.0).exp()
        } else {
            0.0
        };
        let hat = white * 0.25 * (-in_half * 60.0).exp();
        let left = (kick * 0.8 + snare + hat * 0.4).clamp(-1.0, 1.0);
        let right = (kick * 0.8 + snare + hat).clamp(-1.0, 1.0);
        samples.extend([left, right]);
    }
    Decoded {
        samples,
        channels: 2,
        rate: RATE,
    }
}

/// The name the kick is played by, and its content's hash, so its analysis is found for the
/// Sound page's thumbnail.
const KICK: &str = "kick.wav";
const KICK_HASH: ContentHash = ContentHash(0x4B1C);

/// Half a second of a kick drum, made here so the screenshots need no file: a sine falling from
/// 150 Hz to 50 Hz as it dies away.
fn kick() -> SampleData {
    let frames = (0..RATE / 2)
        .scan(0.0_f32, |phase, at| {
            let time = at as f32 / RATE as f32;
            let frequency = 50.0 + 100.0 * (-time * 30.0).exp();
            *phase += std::f32::consts::TAU * frequency / RATE as f32;
            let sample = phase.sin() * 0.9 * (-time * 8.0).exp();
            Some([sample, sample])
        })
        .collect();
    SampleData { frames }
}

/// Write rows into a track's first phrase, one row of step text each.
fn write_rows(song: &mut Song, track: usize, rows: &[&str]) {
    let phrase = Arc::make_mut(
        song.tracks[track]
            .phrases
            .get_mut(&0)
            .expect("a new song's phrase"),
    );
    for (at, text) in rows.iter().enumerate() {
        if let Some(step) = phrase.step_mut(at) {
            *step = text.parse().expect("a step's text");
        }
    }
}

/// A song whose first track plays these rows in a phrase of 16, with this synth if it's given.
fn song(rows: &[&str], voice: Option<SynthVoice>) -> Song {
    let mut song = Song::new(4);
    // A name of its own, as a random one would differ on every run.
    song.name = "dusty-breakdown".to_owned();
    if let Some(voice) = voice {
        song.tracks[0].machine = Machine::Plaits(voice);
    }
    write_rows(&mut song, 0, rows);
    song
}

/// Name the four tracks, put the first two in group 1, and send three of them and the group to
/// return B, by different amounts, for its Sends in.
/// FX 1's `Length` two rows up and its `Rate` at `÷2`, half as fast, which move every lane on that
/// column.
fn lane_moved(song: &mut Song) {
    song.tracks[0].lanes[0] = vt_model::lane::LaneMove {
        length: 0x82,
        rate: 14,
    };
}

fn sends_to_b(song: &mut Song) {
    for (at, name) in ["Lead", "Snare", "Hats", "Pad"].into_iter().enumerate() {
        song.tracks[at].name = name.to_owned();
    }
    for track in [0, 1] {
        song.tracks[track].group = Some(1);
    }
    for (track, send) in [(0, 0x60), (2, 0x0C), (3, 0x30)] {
        song.tracks[track].send_b = send;
    }
    let group = song.fixed_track(TrackKind::Group(1)).expect("a group");
    song.tracks[group].send_b = 0x18;
}

/// A pad ducked by a kick: the first track, `Pad`, through a Glue at 10:1 hearing the second,
/// `Kick`, on every beat, then an EQ, a Soft distortion and a limiter.
fn sidechained(song: &mut Song) {
    use vt_model::insert::{Compressor, CompressorType, Distort, DistortType, Equaliser, Limiter};
    song.tracks[0].name = "Pad".to_owned();
    // A little under 0 dB, so the pad and the kick together don't reach the master's limiter.
    song.tracks[0].level = 0xD8;
    song.tracks[1].name = "Kick".to_owned();
    song.tracks[1].machine = Machine::Plaits(SynthVoice::Kick);
    let beats: Vec<&str> = (0..16)
        .map(|row| match row % 4 {
            0 => "C-3 64 --- --- ---",
            _ => "--- -- --- --- ---",
        })
        .collect();
    write_rows(song, 1, &beats);
    song.tracks[0].inserts = [
        Insert::Compressor(Compressor {
            kind: CompressorType::Glue,
            threshold: 0x40,
            glue_ratio: 2,
            glue_attack: 3,
            glue_release: 1,
            source: 2,
            ..Compressor::DEFAULT
        }),
        Insert::Eq(Equaliser {
            low: 0x60,
            high: 0xA0,
            ..Equaliser::DEFAULT
        }),
        Insert::Distort(Distort {
            kind: DistortType::Soft,
            amount: 0x90,
            ..Distort::DEFAULT
        }),
        Insert::Limit(Limiter::DEFAULT),
    ];
}

/// A bass through the four filter effects: a gate opened by its own sound, a resonator, a comb a
/// fifth up and a vowel filter at `E`, ringing.
fn filtering(song: &mut Song) {
    use vt_model::insert::{Comb, LowPassGate, MOST_SHIFT, Resonator, Vowel};
    song.tracks[0].inserts = [
        Insert::Lpg(LowPassGate {
            resonance: 0x90,
            ..LowPassGate::DEFAULT
        }),
        Insert::Reso(Resonator::DEFAULT),
        Insert::Comb(Comb {
            tune: MOST_SHIFT + 7,
            ..Comb::DEFAULT
        }),
        Insert::Vowel(Vowel {
            vowel: 0x40,
            resonance: 0xC0,
            ..Vowel::DEFAULT
        }),
    ];
}

/// Seven tracks, each playing: four drums in group 1, named `909`, then an acid bass, a lead and
/// a pad, so each role's colour shows under its name.
fn band(song: &mut Song) {
    let name = std::mem::take(&mut song.name);
    *song = Song::new(7);
    song.name = name;
    const KICKS: &[&str] = &["C-3 64", "", "", "", "C-3 64", "", "", "", "C-3 64"];
    const SNARES: &[&str] = &[
        "", "", "", "", "C-3 64", "", "", "", "", "", "", "", "C-3 64",
    ];
    const HATS: &[&str] = &["", "", "C-3 50", "", "", "", "C-3 50", "", "", "", "C-3 50"];
    const OPEN: &[&str] = &[
        "", "", "", "", "", "", "", "", "", "", "", "", "", "", "C-3 40",
    ];
    const BASS: &[&str] = &[
        "C-2 64", "C-2 50", "", "C-3 64", "", "G-2 64", "C-2 64", "", "D#2 64",
    ];
    const LEAD: &[&str] = &["G-4 50", "", "", "", "", "", "D#4 50", "", "", "", "C-4 50"];
    const PAD: &[&str] = &["C-3 40"];
    let parts: [(&str, SynthVoice, &[&str]); 7] = [
        ("BD", SynthVoice::Kick, KICKS),
        ("SD", SynthVoice::Snare, SNARES),
        ("CH", SynthVoice::Hat, HATS),
        ("OH", SynthVoice::Hat, OPEN),
        ("AC", SynthVoice::Acid, BASS),
        ("LD", SynthVoice::VirtualAnalog, LEAD),
        ("PD", SynthVoice::Chord, PAD),
    ];
    for (track, (label, voice, rows)) in parts.into_iter().enumerate() {
        song.tracks[track].name = label.to_owned();
        song.tracks[track].machine = Machine::Plaits(voice);
        // Down from 0 dB, so seven together don't clip the master.
        song.tracks[track].level = 0xA0;
        let rows: Vec<String> = rows
            .iter()
            .map(|row| {
                if row.is_empty() {
                    "--- -- --- --- ---".to_owned()
                } else {
                    format!("{row} --- --- ---")
                }
            })
            .collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        write_rows(song, track, &rows);
    }
    for track in song.tracks.iter_mut().take(4) {
        track.group = Some(1);
    }
    let group = song.fixed_track(TrackKind::Group(1)).expect("a group");
    song.tracks[group].name = "909".to_owned();
}

/// Two outputs for the audio page to list, the laptop's own playing, at a buffer of 256.
fn two_outputs(app: &mut App) {
    app.audio_page.audio.devices = vec![
        DeviceInfo {
            id: "coreaudio:speakers".to_owned(),
            name: "MacBook Pro Speakers".to_owned(),
            rates: vec![44_100, 48_000, 96_000],
            buffers: vec![64, 128, 256, 512, 1024],
            default: true,
        },
        DeviceInfo {
            id: "coreaudio:interface".to_owned(),
            name: "Audio Interface".to_owned(),
            rates: vec![44_100, 48_000],
            buffers: vec![32, 64, 128, 256, 512],
            default: false,
        },
    ];
    app.audio_page.audio.buffer = Some(256);
    app.audio_page.audio.latency_us = 9_800;
}

/// Press keys as they're written, with `<Name>` for a key that isn't a character, all at this
/// time. It's the shot's clock, never the wall clock's, so how long the machine took to get here
/// can't change what's drawn, as whether the which-key popup's delay has passed (#660).
fn press(app: &mut App, keys: &str, now: Instant) {
    let mut rest = keys;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let code = match &rest[1..end] {
                "Esc" => KeyCode::Esc,
                "Enter" => KeyCode::Enter,
                "Tab" => KeyCode::Tab,
                other => panic!("no key called <{other}>"),
            };
            app.key(Key::new(code), now);
            rest = &rest[end + 1..];
        } else {
            app.key(Key::char(c), now);
            rest = &rest[c.len_utf8()..];
        }
    }
}

/// Play some steps, so the playhead and the engine's reports reach the screen.
/// The clock moves on by exactly one step's audio each frame, so the meters fall the same on every run.
fn play(app: &mut App, realtime: &mut Realtime, steps: usize, start: Instant) -> Instant {
    let mut out = vec![0.0; STEP * 2];
    let step = Duration::from_secs_f64(STEP as f64 / f64::from(RATE));
    let mut now = start;
    app.tick_at(now);
    for _ in 0..steps {
        realtime.process(&mut out);
        now += step;
        app.tick_at(now);
    }
    now
}

/// An app on this song, with the samples it plays, and the audio side, from `start`.
fn app_on(song: Song, pool: SamplePool) -> (App, Realtime) {
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(RATE, version));
    let mut app = App::new(song, None, samples, handle, RATE);
    // The output as a laptop plays it, for the project page's audio row.
    app.audio_page.audio.device = "MacBook Pro Speakers".to_owned();
    app.audio_page.audio.running = true;
    (app, realtime)
}

/// Write the screen as it is now as an SVG, with the images the terminal would be sent in it,
/// and those the program places itself after drawing it, as the sample page's ruler marks.
fn write(app: &mut App, now: Instant, size: (u16, u16), path: &Path) -> std::io::Result<()> {
    let mut sent = Vec::new();
    let mut graphics = Graphics::new();
    app.send_pictures(&mut graphics, &mut sent)?;
    let Ok(mut terminal) = Terminal::new(TestBackend::new(size.0, size.1));
    let Ok(_) = terminal.draw(|frame| draw_screen(frame, app, None, now));
    app.place_pictures(&mut graphics, &mut sent)?;
    std::fs::write(
        path,
        to_svg_placed(
            terminal.backend().buffer(),
            &sent_images(&sent),
            &sent_placements(&sent, DEFAULT_CELL),
        ),
    )
}

fn take(shot: &Shot, folder: &Path) -> std::io::Result<PathBuf> {
    take_slowed(shot, folder, Duration::ZERO)
}

/// Take a shot on a machine this much slower at making the app, as one busy with other builds.
fn take_slowed(shot: &Shot, folder: &Path, slower: Duration) -> std::io::Result<PathBuf> {
    let mut song = song(shot.rows, shot.voice);
    let mut pool = SamplePool::default();
    if shot.kick {
        let kick = kick();
        let decoded = Decoded {
            samples: kick.frames.concat(),
            channels: 2,
            rate: RATE,
        };
        pool.insert_analysis(Arc::new(Analysis::of(KICK_HASH, &decoded)));
        pool.insert(KICK, kick);
        let number = song.bank.add(BankEntry::of_file(KICK, KICK_HASH));
        song.tracks[0].machine = Machine::Sampler;
        song.tracks[0].set_setting(SettingId::SAMPLE, number.expect("room in the bank"));
    }
    if shot.drums {
        let decoded = drums();
        pool.insert(
            DRUMS,
            SampleData {
                frames: decoded.samples.as_chunks::<2>().0.to_vec(),
            },
        );
        pool.insert_analysis(Arc::new(Analysis::of(DRUMS_HASH, &decoded)));
        let mut entry = BankEntry::of_file(DRUMS, DRUMS_HASH);
        entry.name = "Drums 120".to_owned();
        let beat = u64::from(RATE / 2);
        entry.edit.points = Points {
            start: beat / 16,
            end: Some(beat * 8 - beat / 4),
            loop_: Some(Loop {
                start: beat * 4,
                end: beat * 6,
                crossfade: 0,
            }),
            ..Points::default()
        };
        // A slice on each half beat, as `:slice 16` would nearly cut it.
        let halves: Vec<u64> = (0..16)
            .map(|half| (half * beat / 2).max(beat / 16))
            .collect();
        entry.edit.points.reslice(&halves);
        // Letters naming two of them, as `:mark a 04` and `:mark b 0C` would.
        entry.edit.points.set_mark('a', 4);
        entry.edit.points.set_mark('b', 12);
        let number = song.bank.add(entry).expect("room in the bank");
        song.tracks[0].machine = Machine::Sampler;
        song.tracks[0].set_setting(SettingId::SAMPLE, number);
    }
    if let Some(prepare) = shot.song {
        prepare(&mut song);
    }
    // Taken before the app, so its first frame measures no time since the app began.
    let start = Instant::now();
    let (mut app, mut realtime) = app_on(song, pool);
    if let Some(prepare) = shot.app {
        prepare(&mut app);
    }
    std::thread::sleep(slower);
    press(&mut app, shot.keys, start);
    let now = play(&mut app, &mut realtime, shot.play, start);
    let path = folder.join(format!("{}.svg", shot.name));
    write(&mut app, now, shot.size, &path)?;
    Ok(path)
}

/// Write the kick as a WAV file, a little louder or softer.
fn kick_file(path: &Path, gain: f32) -> std::io::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(std::io::Error::other)?;
    for [sample, _] in kick().frames {
        let value = (sample * gain * f32::from(i16::MAX)) as i16;
        writer.write_sample(value).map_err(std::io::Error::other)?;
    }
    writer.finalize().map_err(std::io::Error::other)
}

/// A folder of the system's temporary folder for one picture of the explorer, named from it as
/// `~`, so the picture is the same wherever it's taken. It's this process's own, so two runs at
/// once, as CI's on one machine, don't share it, and this call's own, so two tests of one process
/// taking the same picture at once don't either.
fn scratch_home(picture: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let home = std::env::temp_dir().join(format!(
        "vimtracker-screenshots-{picture}-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    home
}

/// Show the explorer for a purpose over this folder, on its first row, as the app opens it.
fn explore(app: &mut App, purpose: Purpose, folder: &Path, meaning: &str, home: &Path) {
    let roots = vec![Root::Place {
        path: folder.to_path_buf(),
        meaning: meaning.to_owned(),
    }];
    let hint = match purpose {
        Purpose::Kit => "Enter use  L on tracks  T tag",
        Purpose::Preset(_) => "Enter load  T tag",
        _ => "Enter choose  T tag",
    };
    app.overlays.picker = Some(
        Picker::explorer(purpose, roots, Some(home.to_path_buf()))
            .with_tags(Arc::clone(&app.overlays.explorer.tag_library))
            .opening_first()
            .hinted(hint),
    );
    app.modes.mode = Mode::Picker;
}

/// The sample explorer, `:sample`, over a folder of samples with a kit, some loops and a sound
/// whose file has gone, so each of its icons shows.
fn take_explorer(folder: &Path) -> std::io::Result<PathBuf> {
    let home = scratch_home("explorer");
    let samples = home.join("Samples");
    for inside in ["Drums", "Loops", "Vocals"] {
        std::fs::create_dir_all(samples.join(inside))?;
    }
    kick_file(&samples.join("Drums/kick 909.wav"), 0.9)?;
    kick_file(&samples.join("Drums/kick 909 soft.wav"), 0.4)?;
    kick_file(&samples.join("Drums/snare 909.wav"), 0.7)?;
    kick_file(&samples.join("Loops/break.wav"), 0.8)?;
    kick_file(&samples.join("Loops/fill.wav"), 0.6)?;
    kick_file(&samples.join("kick.wav"), 0.9)?;
    kick_file(&samples.join("riser 128 bpm.wav"), 0.5)?;
    std::fs::write(
        samples.join("909 Core Kit.ron"),
        r#"(name: "909 Core Kit", sounds: [
            (label: "BD", name: "Kick 909", path: "Drums/kick 909.wav",
             layers: ["Drums/kick 909 soft.wav", "Drums/kick 909.wav"]),
            (label: "SD", name: "Snare 909", path: "Drums/snare 909.wav"),
            (label: "CH", name: "Closed Hat 909", path: "Drums/hat 909.wav"),
        ])"#,
    )?;
    let start = Instant::now();
    let (mut app, _realtime) = app_on(song(&[], None), SamplePool::default());
    let roots = vec![Root::Place {
        path: samples.clone(),
        meaning: "samples".to_owned(),
    }];
    let picker = Picker::explorer(Purpose::Sample, roots, Some(home.clone()))
        .following(Some(&samples.join("kick.wav")))
        .hinted("Enter choose");
    app.overlays.picker = Some(picker);
    app.modes.mode = Mode::Picker;
    // Open the loops and the kit, and come back down to the kick, which is the track's.
    press(&mut app, "kkkljjjljjj", start);
    app.finish_loading();
    let path = folder.join("explorer.svg");
    write(&mut app, start, (WIDTH, HEIGHT), &path)?;
    std::fs::remove_dir_all(&home)?;
    Ok(path)
}

/// The kit dialog, `:kit`, opened to a kit's sounds with four of them marked to load as tracks.
fn take_kit(folder: &Path) -> std::io::Result<PathBuf> {
    let home = scratch_home("kit");
    let kits = home.join("Kits");
    let drums = home.join("Samples/Drums");
    std::fs::create_dir_all(&kits)?;
    std::fs::create_dir_all(&drums)?;
    for (file, gain) in [
        ("kick.wav", 0.9),
        ("snare.wav", 0.7),
        ("closed hat.wav", 0.4),
        ("open hat.wav", 0.5),
        ("clap.wav", 0.6),
        ("rim.wav", 0.3),
        ("low tom.wav", 0.8),
    ] {
        kick_file(&drums.join(file), gain)?;
    }
    std::fs::write(
        kits.join("909 Core Kit.ron"),
        r#"(name: "909 Core Kit", category: "Drum Machines", sounds: [
            (label: "BD", name: "Kick 909", path: "../Samples/Drums/kick.wav"),
            (label: "SD", name: "Snare 909", path: "../Samples/Drums/snare.wav"),
            (label: "CH", name: "Closed Hat 909", path: "../Samples/Drums/closed hat.wav",
             choke: Some(1)),
            (label: "OH", name: "Open Hat 909", path: "../Samples/Drums/open hat.wav",
             choke: Some(1)),
            (label: "CP", name: "Clap 909", path: "../Samples/Drums/clap.wav"),
            (label: "RS", name: "Rim 909", path: "../Samples/Drums/rim.wav"),
            (label: "LT", name: "Low Tom 909", path: "../Samples/Drums/low tom.wav", tune: -5),
        ])"#,
    )?;
    std::fs::write(
        kits.join("Rytm Analog.ron"),
        r#"(sounds: [(label: "BD", path: "../Samples/Drums/kick.wav")])"#,
    )?;
    let start = Instant::now();
    let (mut app, _realtime) = app_on(song(&[], None), SamplePool::default());
    explore(&mut app, Purpose::Kit, &kits, "kits", &home);
    // Into the 909 kit, the kick, snare and both hats marked, and on to the clap.
    press(&mut app, "l4x", start);
    app.finish_loading();
    let path = folder.join("kit.svg");
    write(&mut app, start, (WIDTH, HEIGHT), &path)?;
    std::fs::remove_dir_all(&home)?;
    Ok(path)
}

/// The sound presets' browser, `:preset`, over a few saved in folders, each tagged by what it
/// is, with tags of their own on the basses.
fn take_presets(folder: &Path) -> std::io::Result<PathBuf> {
    let home = scratch_home("presets");
    let presets = home.join("Presets");
    let mut sounds = Song::new(1);
    let saved: [(&str, SynthVoice, Option<Insert>, &[&str]); 5] = [
        (
            "bass/Bite",
            SynthVoice::Acid,
            Some(Insert::RETURN_DELAY),
            &["squelch"],
        ),
        (
            "bass/Round Sub",
            SynthVoice::VirtualAnalogVcf,
            None,
            &["sub"],
        ),
        (
            "keys/Glass Bell",
            SynthVoice::Fm,
            Some(Insert::RETURN_REVERB),
            &[],
        ),
        ("keys/Hoover", SynthVoice::Supersaw, None, &[]),
        (
            "Dust Cloud",
            SynthVoice::Dust,
            Some(Insert::RETURN_REVERB),
            &[],
        ),
    ];
    for (name, voice, insert, tags) in saved {
        let track = &mut sounds.tracks[0];
        track.machine = Machine::Plaits(voice);
        track.inserts[0] = insert.unwrap_or_default();
        let path = preset_path(&presets, PresetKind::Sound, name).map_err(std::io::Error::other)?;
        let preset = SoundPreset::of(track, None);
        vt_model::preset::save(&path, &preset, true).map_err(std::io::Error::other)?;
        if !tags.is_empty() {
            let tags = tags.iter().map(|tag| (*tag).to_owned()).collect();
            set_tags::<SoundPreset>(&path, tags).map_err(std::io::Error::other)?;
        }
    }
    let start = Instant::now();
    let (mut app, _realtime) = app_on(song(&[], None), SamplePool::default());
    let sounds = presets.join(PresetKind::Sound.folder());
    explore(
        &mut app,
        Purpose::Preset(PresetKind::Sound),
        &sounds,
        "sounds",
        &home,
    );
    // Open the basses and the keys, and stand on the acid.
    press(&mut app, "ljjlkkk", start);
    let path = folder.join("presets.svg");
    write(&mut app, start, (WIDTH, HEIGHT), &path)?;
    std::fs::remove_dir_all(&home)?;
    Ok(path)
}

/// Make one picture in a folder, saying where it went.
type Take = fn(&Path) -> std::io::Result<PathBuf>;

/// Each picture made from files of its own rather than a song, by its name.
const EXPLORERS: [(&str, Take); 3] = [
    ("explorer", take_explorer),
    ("kit", take_kit),
    ("presets", take_presets),
];

/// Make every picture in a folder, saying where each went. `just screenshots` and the test
/// against the committed ones both make them here, so they write the same bytes.
fn take_all(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(folder)?;
    let mut taken = Vec::new();
    for shot in SHOTS {
        taken.push(take(shot, folder)?);
    }
    for (_, take) in EXPLORERS {
        taken.push(take(folder)?);
    }
    Ok(taken)
}

fn main() -> std::io::Result<()> {
    let folder = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("docs/manual/src/images"), PathBuf::from);
    for path in take_all(&folder)? {
        println!("{}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each screenshot comes out the same twice, so `just screenshots` changes nothing without a code change.
    #[test]
    fn screenshots_are_the_same_on_every_run() {
        let base = std::env::temp_dir().join(format!("vt-screenshots-{}", std::process::id()));
        let (first, second) = (base.join("first"), base.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        for shot in SHOTS {
            let a = std::fs::read(take(shot, &first).unwrap()).unwrap();
            let b = std::fs::read(take(shot, &second).unwrap()).unwrap();
            assert!(a == b, "{} differs between runs", shot.name);
        }
        for (name, take) in EXPLORERS {
            let a = std::fs::read(take(&first).unwrap()).unwrap();
            let b = std::fs::read(take(&second).unwrap()).unwrap();
            assert!(a == b, "{name} differs between runs");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Each screenshot is the one committed in the manual, byte for byte, and the manual has no
    /// picture that nothing makes, so a change to what's drawn fails here rather than waiting
    /// for someone to run `just screenshots`. `VT_BLESS=1` writes them again, as that does,
    /// after a change on purpose; the diff of the SVGs then shows what moved.
    #[test]
    fn screenshots_match_the_committed_ones() {
        let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/src/images");
        if std::env::var_os("VT_BLESS").is_some() {
            take_all(&committed).unwrap();
            return;
        }
        let fresh = std::env::temp_dir().join(format!("vt-committed-{}", std::process::id()));
        let name = |path: &Path| path.file_name().unwrap().to_string_lossy().into_owned();
        let taken: Vec<String> = take_all(&fresh)
            .unwrap()
            .iter()
            .map(|path| name(path))
            .collect();
        let changed: Vec<&String> = taken
            .iter()
            .filter(|name| {
                let now = std::fs::read(fresh.join(name)).unwrap();
                std::fs::read(committed.join(name)).ok() != Some(now)
            })
            .collect();
        let mut unmade: Vec<String> = std::fs::read_dir(&committed)
            .unwrap()
            .map(|entry| name(&entry.unwrap().path()))
            .filter(|name| name.ends_with(".svg") && !taken.contains(name))
            .collect();
        unmade.sort();
        assert!(
            changed.is_empty() && unmade.is_empty(),
            "the screenshots differ from those in {}: changed or new {changed:?}, made by no shot \
             {unmade:?}. The new ones are in {}. If the change is meant, run with VT_BLESS=1 or \
             `just screenshots`, and delete any no shot makes.",
            committed.display(),
            fresh.display(),
        );
        let _ = std::fs::remove_dir_all(&fresh);
    }

    /// The which-key popup shows once its delay has passed since `Space`, and the shot is drawn
    /// four steps after the shot's clock starts. The keys used to be pressed at the wall clock's
    /// time, so when a busy machine was slow to make the app they fell so late that the delay
    /// hadn't passed by the drawing, and the popup was missing (#660). They're pressed on the
    /// shot's own clock now, so a slow machine draws the same picture.
    #[test]
    fn which_key_is_the_same_on_a_slow_machine() {
        let base = std::env::temp_dir().join(format!("vt-which-key-{}", std::process::id()));
        let (quick, slow) = (base.join("quick"), base.join("slow"));
        std::fs::create_dir_all(&quick).unwrap();
        std::fs::create_dir_all(&slow).unwrap();
        let shot = SHOTS
            .iter()
            .find(|shot| shot.name == "which-key")
            .expect("the which-key shot");
        let a = std::fs::read(take(shot, &quick).unwrap()).unwrap();
        let slower = Duration::from_millis(400);
        let b = std::fs::read(take_slowed(shot, &slow, slower).unwrap()).unwrap();
        assert!(a == b, "which-key differs when the app is slow to make");
        let _ = std::fs::remove_dir_all(&base);
    }
}
