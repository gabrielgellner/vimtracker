//! Write a project folder that plays one Acid track from a take laid out as rows, for
//! `tools/compare/take.py`, which compares a riff played on Acid V in Ableton Live with the same
//! riff on ours (#565). The harness works out the rows; this only builds the song from them with
//! the program's own model, so the file is one the program opens, and `vimtracker render` plays.
//!
//! ```sh
//! cargo run --release -p vt-tui --example take_song -- rows.toml out/takes/riff/ours
//! ```
//!
//! The rows file:
//!
//! ```toml
//! name = "riff"            # the song's name
//! bpm = 128.0
//! rows = 64                # how many of the steps are the phrase's loop, its rows of notes
//! lanes = 3                # how many FX columns have a lane of their own, as long as `steps`
//! steps = ["C-2 63 SGL1 80 acid.cutoff:4080g --- ---", "..."]   # every stored step, as text
//! [settings]               # the Acid's knobs where the take starts, by the keys songs store
//! "acid.cutoff" = [128, 0] # coarse and fine
//! ```
//!
//! An existing folder's `song.ron` is replaced only if the folder holds nothing else, so a
//! project of the user's is never written over.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use serde::Deserialize;
use vt_model::constants::{FX_SLOTS, MAX_PHRASE_STEPS};
use vt_model::project;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Machine, Phrase, Song, SynthVoice};
use vt_model::step::Step;

#[derive(Deserialize)]
struct Rows {
    name: String,
    bpm: f64,
    rows: usize,
    #[serde(default)]
    lanes: usize,
    steps: Vec<String>,
    #[serde(default)]
    settings: BTreeMap<String, (u8, u8)>,
}

fn song(rows: &Rows) -> Result<Song, String> {
    if rows.steps.is_empty() || rows.steps.len() > MAX_PHRASE_STEPS {
        return Err(format!(
            "a take is laid out in 1 to {MAX_PHRASE_STEPS} steps, and this has {}",
            rows.steps.len()
        ));
    }
    if rows.rows == 0 || rows.rows > rows.steps.len() || rows.lanes > FX_SLOTS {
        return Err("`rows` must be within the steps, and `lanes` at most 3".to_owned());
    }
    let steps = rows
        .steps
        .iter()
        .enumerate()
        .map(|(at, text)| {
            text.parse::<Step>()
                .map_err(|error| format!("step {at}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut song = Song::new(1);
    song.name = rows.name.clone();
    // What's compared is the voice, so nothing clips it, as with `vimtracker probe`.
    song.limiter = false;
    song.millibpm = (rows.bpm * 1000.0).round() as u32;
    let track = &mut song.tracks[0];
    track.machine = Machine::Plaits(SynthVoice::Acid);
    for (key, (coarse, fine)) in &rows.settings {
        let id = SettingId::from_key(key).ok_or_else(|| format!("no setting called {key}"))?;
        track.set_setting(id, *coarse);
        track.set_fine(id, *fine);
    }
    let mut phrase = Phrase::from_steps(steps);
    let stored = phrase.steps().len();
    phrase.set_len(rows.rows);
    for slot in 0..rows.lanes {
        phrase.set_lane(
            slot,
            Some(Lane {
                start: 0,
                length: stored,
            }),
        );
    }
    track.phrases.insert(0, Arc::new(phrase));
    Ok(song)
}

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [given, folder] = args.as_slice() else {
        eprintln!("usage: take_song <rows.toml> <project folder>");
        return ExitCode::FAILURE;
    };
    let built = std::fs::read_to_string(given)
        .map_err(|error| format!("{}: {error}", given.display()))
        .and_then(|text| toml::from_str::<Rows>(&text).map_err(|error| error.to_string()))
        .and_then(|rows| song(&rows));
    let song = match built {
        Ok(song) => song,
        Err(error) => {
            eprintln!("{}: {error}", given.display());
            return ExitCode::FAILURE;
        }
    };
    let others = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name() != "song.ron")
        })
        .unwrap_or(false);
    if others {
        eprintln!(
            "{} holds more than a song.ron; give an empty folder or one this wrote",
            folder.display()
        );
        return ExitCode::FAILURE;
    }
    match project::save_song(folder, &song) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
