//! Make the manual's screenshots: `just screenshots`, or with a folder to write them to.
//!
//! Each shot builds a song, presses keys as a player would, plays a few steps if it needs the
//! playhead, and writes the screen as an SVG, with the images the terminal would be sent in it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::analysis::Analysis;
use vt_model::bank::{BankEntry, ContentHash, Loop, Points};
use vt_model::sample::{Decoded, SampleData};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song, SynthVoice, TrackKind};
use vt_tui::app::App;
use vt_tui::explorer::Purpose;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;
use vt_tui::input::picker::{Picker, Root};
use vt_tui::kitty::{DEFAULT_CELL, Graphics};
use vt_tui::run::draw_screen;
use vt_tui::screenshot::{sent_images, sent_placements, to_svg_placed};

const RATE: u32 = 48_000;
/// Frames in one step at 120 BPM.
const STEP: usize = 6000;
/// The screen's size in cells.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 24;
/// The project page's, which is taller, with its groove under its settings.
const PROJECT_HEIGHT: u16 = 32;

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
}

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
    Shot {
        name: "sample",
        rows: GATES,
        keys: " 0zizilllllzc",
        play: 0,
        voice: None,
        kick: false,
        drums: true,
    },
    Shot {
        name: "gates",
        rows: GATES,
        keys: "lll",
        play: 0,
        voice: None,
        kick: false,
        drums: false,
    },
    Shot {
        name: "gate-list",
        rows: GATES,
        keys: "lllK",
        play: 0,
        voice: None,
        kick: false,
        drums: false,
    },
    Shot {
        name: "lane",
        rows: LANE,
        keys: "lllll:lane 3<Enter><Tab>",
        play: 21,
        voice: Some(SynthVoice::VirtualAnalog),
        kick: false,
        drums: false,
    },
    Shot {
        name: "track-sound",
        rows: GATES,
        keys: " 5jli60<Enter>ji90<Enter>k",
        play: 0,
        voice: None,
        kick: true,
        drums: false,
    },
    Shot {
        name: "track-fx",
        rows: LANE,
        keys: " 5LLKComp<Enter>l==hji40}KEQ3<Enter>jjiD0li50}KLimit<Enter><Tab>",
        play: 6,
        voice: Some(SynthVoice::VirtualAnalog),
        kick: false,
        drums: false,
    },
    Shot {
        name: "project",
        rows: GATES,
        keys: " 9jjj=l+++:set scale=dorian<Enter>:groove pull<Enter>",
        play: 0,
        voice: None,
        kick: false,
        drums: false,
    },
    // Return B, past the four tracks, group 1 and return A, with what sends to it under its
    // Output (`sends_to_b`).
    Shot {
        name: "track-return",
        rows: GATES,
        keys: " 5WWWWWWgg0j<Tab>",
        play: 8,
        voice: Some(SynthVoice::VirtualAnalog),
        kick: false,
        drums: false,
    },
    // `Space` and a wait: the popup of what can come next, the views' digits among it, on the
    // author's screen, where it fits whole. The steps played are only the wait, as nothing is
    // playing.
    Shot {
        name: "which-key",
        rows: GATES,
        keys: " ",
        play: 4,
        voice: None,
        kick: false,
        drums: false,
    },
    Shot {
        name: "track-mod",
        rows: GATES,
        keys: " 5Lji20<Enter>ji300<Enter>ji400<Enter>lkki60<Enter>gglllK/cut<Enter>jjji+30<Enter><Tab>",
        play: 3,
        voice: None,
        kick: false,
        drums: false,
    },
];

/// The sample page's and the Sound, Mod and FX pages' shots are of the author's screen, kitty full
/// screen on a MacBook, where each track page's four rows of devices fit beside the main meter.
const SAMPLE_WIDTH: u16 = 163;
const SAMPLE_HEIGHT: u16 = 42;

/// The name the drum loop is played by, and the content it's analysed as.
const DRUMS: &str = "drums.wav";
const DRUMS_HASH: ContentHash = ContentHash(0xD2D2);

/// Two bars of drums at 120 BPM, made here so the screenshot needs no file: a kick on each beat, a
/// snare on two and four, and a closed hat between, in stereo with the hat to the right.
fn drums() -> Decoded {
    let beat = RATE as usize / 2;
    let mut noise = 0x1234_5678_u32;
    let mut samples = Vec::with_capacity(beat * 8 * 2);
    for index in 0..beat * 8 {
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (noise >> 8) as f32 / (1 << 23) as f32 - 1.0;
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

/// A song whose first track plays these rows in a phrase of 16, with this synth if it's given.
fn song(rows: &[&str], voice: Option<SynthVoice>) -> Song {
    let mut song = Song::new(4);
    // A name of its own, as a random one would differ on every run.
    song.name = "dusty-breakdown".to_owned();
    if let Some(voice) = voice {
        song.tracks[0].machine = Machine::Plaits(voice);
    }
    let phrase = Arc::make_mut(
        song.tracks[0]
            .phrases
            .get_mut(&0)
            .expect("a new song's phrase"),
    );
    for (at, text) in rows.iter().enumerate() {
        if let Some(step) = phrase.step_mut(at) {
            *step = text.parse().expect("a step's text");
        }
    }
    song
}

/// Name the four tracks, put the first two in group 1, and send three of them and the group to
/// return B, by different amounts, for its Sends in.
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
    app.audio.device = "MacBook Pro Speakers".to_owned();
    app.audio.running = true;
    (app, realtime)
}

/// Write the screen as it is now as an SVG, with the images the terminal would be sent in it,
/// and those the program places itself after drawing it, as the sample page's ruler marks.
fn write(app: &mut App, now: Instant, size: (u16, u16), path: &Path) -> std::io::Result<()> {
    let mut sent = Vec::new();
    let mut graphics = Graphics::new();
    graphics.sync(app, &mut sent)?;
    let Ok(mut terminal) = Terminal::new(TestBackend::new(size.0, size.1));
    let Ok(_) = terminal.draw(|frame| draw_screen(frame, app, None, now));
    graphics.place(app, &mut sent)?;
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
    if shot.name == "track-return" {
        sends_to_b(&mut song);
    }
    // Taken before the app, so its first frame measures no time since the app began.
    let start = Instant::now();
    let (mut app, mut realtime) = app_on(song, pool);
    std::thread::sleep(slower);
    press(&mut app, shot.keys, start);
    let now = play(&mut app, &mut realtime, shot.play, start);
    let size = match shot.name {
        "project" => (WIDTH, PROJECT_HEIGHT),
        "sample" | "track-fx" | "track-sound" | "track-mod" | "track-return" | "which-key" => {
            (SAMPLE_WIDTH, SAMPLE_HEIGHT)
        }
        _ => (WIDTH, HEIGHT),
    };
    let path = folder.join(format!("{}.svg", shot.name));
    write(&mut app, now, size, &path)?;
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

/// The sample explorer, `:sample`, over a folder of samples with a kit, some loops and a sound
/// whose file has gone, so each of its icons shows. The folder is made in the system's temporary
/// folder and named from it as `~`, so the picture is the same wherever it's taken. It's this
/// process's own, so two runs at once, as CI's on one machine, don't share it.
fn take_explorer(folder: &Path) -> std::io::Result<PathBuf> {
    let home = std::env::temp_dir().join(format!("vimtracker-screenshots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
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
    app.picker = Some(picker);
    app.mode = Mode::Picker;
    // Open the loops and the kit, and come back down to the kick, which is the track's.
    press(&mut app, "kkkljjjljjj", start);
    app.finish_loading();
    let path = folder.join("explorer.svg");
    write(&mut app, start, (WIDTH, HEIGHT), &path)?;
    std::fs::remove_dir_all(&home)?;
    Ok(path)
}

fn main() -> std::io::Result<()> {
    let folder = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("docs/manual/src/images"), PathBuf::from);
    std::fs::create_dir_all(&folder)?;
    for shot in SHOTS {
        println!("{}", take(shot, &folder)?.display());
    }
    println!("{}", take_explorer(&folder)?.display());
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
        let a = std::fs::read(take_explorer(&first).unwrap()).unwrap();
        let b = std::fs::read(take_explorer(&second).unwrap()).unwrap();
        assert!(a == b, "the explorer differs between runs");
        let _ = std::fs::remove_dir_all(&base);
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
