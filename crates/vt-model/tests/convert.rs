//! Converting a format 10 project, whose nudges were 24ths of their track's step, to format 11,
//! where they're 24ths of a sixteenth (decision 299).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::action::{Action, edit};
use vt_model::convert::{ConvertError, convert_project, convert_text};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::project::{load_song, save_song};
use vt_model::selection::StepRange;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, Grids, Pink, Shape, ShapeColumn, Values, Walk,
    Where,
};
use vt_model::song::{Phrase, Song};
use vt_model::speed::Speed;
use vt_model::step::Step;

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-convert-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn ramp(from: u8, to: u8) -> Shape {
    Shape {
        from,
        to,
        curve: 0,
        s: 0,
        kind: FillKind::Ramp,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::new(&[8, 11, 22]),
        place: Where::Notes,
        seed: 7,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: Grids::default(),
        vary: 0,
    }
}

/// Three tracks, at a sixteenth, an eighth and a 32nd, with the same nudges, and on the eighth a
/// live ramp of nudges from `-3` to `+3` as format 10 made it.
fn song() -> Song {
    let mut song = Song::new(3);
    let rows: Vec<Step> = ["C-3 40 -B", "C-3 40 +6", "C-3 40 -1", "--- -- +3 cut:40"]
        .iter()
        .map(|row| row.parse().unwrap())
        .collect();
    let speeds = [Speed::Sixteenth, Speed::Eighth, Speed::ThirtySecond];
    for (track, speed) in speeds.into_iter().enumerate() {
        song.tracks[track].speed = speed;
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(rows.clone())));
        song.tracks[track]
            .phrases
            .insert(1, Arc::new(Phrase::from_steps(rows.clone())));
    }
    let generate = Action::Generate {
        track: 1,
        phrase: 1,
        column: ShapeColumn::Nudge { most: 11 },
        span: Span::rows(StepRange::between(0, 2)),
        shape: ramp(8, 14),
    };
    Arc::unwrap_or_clone(edit(&Arc::new(song), &generate).unwrap())
}

/// A song's text as format 10 wrote it: its number, and a Nudge column with nothing in it.
fn format_10(song: &Song) -> String {
    ron::to_string(song)
        .unwrap()
        .replace("format_version:11", "format_version:10")
        .replace("column:Nudge(most:11)", "column:Nudge")
}

fn nudges(song: &Song, track: usize, phrase: u32) -> Vec<i8> {
    song.tracks[track].phrases[&phrase]
        .steps()
        .iter()
        .map(|step| step.nudge)
        .collect()
}

#[test]
fn a_nudge_keeps_its_time_and_a_sixteenth_is_left_as_it_was() {
    let before = song();
    let (after, done) = convert_text(&format_10(&before), "song.ron").unwrap();
    assert_eq!(
        after.tracks[0], before.tracks[0],
        "a sixteenth's track is the same"
    );
    assert_eq!(
        nudges(&after, 1, 0),
        [-22, 12, -2, 6],
        "twice as long at an eighth"
    );
    // Half as long at a 32nd, where `B` is 5.5 and kept to 5.
    assert_eq!(nudges(&after, 2, 0), [-5, 3, -1, 2]);
    assert_eq!(done.clamped, 2, "on both its phrases");
    assert_eq!(done.generators, 1);
    // The generator's cells, which are in the steps, and what it keeps under them.
    assert_eq!(nudges(&after, 1, 1), [-6, 0, 6, 6]);
    let generator = &after.tracks[1].phrases[&1].generators()[0];
    assert_eq!(generator.column, ShapeColumn::Nudge { most: 23 });
    // `-3` to `+3` of an eighth's 24ths is `-6` to `+6` of a sixteenth's, held from 23.
    assert_eq!((generator.shape.from, generator.shape.to), (17, 29));
    assert_eq!(generator.shape.values.as_slice(), [17, 23, 45]);
    assert_eq!(generator.shape.walk.step, 4);
    let under = generator.under(after.tracks[1].phrases[&1].steps());
    assert_eq!(
        under.iter().map(|s| s.nudge).collect::<Vec<_>>(),
        [-22, 12, -2, 6],
        "what was under it is converted too"
    );
}

#[test]
fn a_project_is_converted_once_beside_its_old_file() {
    let dir = folder();
    let before = song();
    save_song(&dir, &before).unwrap();
    let path = dir.join("song.ron");
    fs::write(&path, format_10(&before)).unwrap();
    assert!(load_song(&dir).is_err(), "format 10 isn't read as it is");
    let done = convert_project(&dir).unwrap();
    assert_eq!(done.generators, 1);
    let loaded = load_song(&dir).unwrap();
    assert_eq!(nudges(&loaded, 1, 0), [-22, 12, -2, 6]);
    assert_eq!(
        fs::read_to_string(dir.join("song.format-10.ron")).unwrap(),
        format_10(&before)
    );
    // Run again, it's refused: the song is format 11 now.
    assert!(matches!(
        convert_project(&dir),
        Err(ConvertError::Version { found: 11, .. })
    ));
}
