//! Make the manual's screenshots: `just screenshots`, or with a folder to write them to.
//!
//! Each shot builds a song, presses keys as a player would, plays a few steps if it needs the
//! playhead, and writes the screen as an SVG, with the images the terminal would be sent in it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Song, SynthVoice};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::kitty::Graphics;
use vt_tui::run::draw_screen;
use vt_tui::screenshot::{sent_images, to_svg};

const RATE: u32 = 48_000;
/// Frames in one step at 120 BPM.
const STEP: usize = 6000;
/// The screen's size in cells.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 24;

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
}

const GATES: &[&str] = &[
    "C-4 64 HLD3 --- --- ---",
    "E-4 64 --- --- ---",
    "G-4 50 MLT2 --- --- ---",
    "C-5 64 SGL0 --- --- ---",
    "--- -- RST2 --- --- ---",
    "A-3 64 --- --- ---",
];

const LANE: &[&str] = &[
    "C-3 64 sna:20h --- ---",
    "--- -- sna:60h --- ---",
    "--- -- sna:A0h --- ---",
    "--- -- --- --- ---",
    "C-3 64 --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "G-3 64 --- --- ---",
];

const SHOTS: &[Shot] = &[
    Shot {
        name: "gates",
        rows: GATES,
        keys: "ll",
        play: 0,
        voice: None,
    },
    Shot {
        name: "gate-list",
        rows: GATES,
        keys: "llK",
        play: 0,
        voice: None,
    },
    Shot {
        name: "lane",
        rows: LANE,
        keys: "lll:lane 3<Enter><Tab>",
        play: 21,
        voice: Some(SynthVoice::VirtualAnalog),
    },
    Shot {
        name: "track-sound",
        rows: GATES,
        keys: " 4jjji60<Enter>ji90<Enter>k",
        play: 0,
        voice: None,
    },
    Shot {
        name: "track-mod",
        rows: GATES,
        keys: " 4Lli20<Enter>li300<Enter>li60<Enter>li400<Enter>",
        play: 0,
        voice: None,
    },
];

/// A song whose first track plays these rows in a phrase of 16, with this synth if it's given.
fn song(rows: &[&str], voice: Option<SynthVoice>) -> Song {
    let mut song = Song::new(4);
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

/// Press keys as they're written, with `<Name>` for a key that isn't a character.
fn press(app: &mut App, keys: &str) {
    let now = Instant::now();
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
fn play(app: &mut App, realtime: &mut Realtime, steps: usize) {
    let mut out = vec![0.0; STEP * 2];
    app.tick();
    for _ in 0..steps {
        realtime.process(&mut out);
        app.tick();
    }
}

fn take(shot: &Shot, folder: &Path) -> std::io::Result<PathBuf> {
    let song = song(shot.rows, shot.voice);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(RATE, version));
    let mut app = App::new(song, None, samples, handle, RATE);
    press(&mut app, shot.keys);
    play(&mut app, &mut realtime, shot.play);
    let mut sent = Vec::new();
    Graphics::new().sync(&mut app, &mut sent)?;
    let Ok(mut terminal) = Terminal::new(TestBackend::new(WIDTH, HEIGHT));
    let Ok(_) = terminal.draw(|frame| draw_screen(frame, &mut app, None, Instant::now()));
    let path = folder.join(format!("{}.svg", shot.name));
    std::fs::write(
        &path,
        to_svg(terminal.backend().buffer(), &sent_images(&sent)),
    )?;
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
    Ok(())
}
