//! Kit lines (decision 464): one `Mini` line across a group's drum tracks, its hits their names,
//! kept the same on every track through `edit`.

use std::sync::Arc;

use vt_model::action::{Action, EditError, edit};
use vt_model::generator::Span;
use vt_model::history::History;
use vt_model::project::{load_song, save_song};
use vt_model::scale::Scale;
use vt_model::selection::StepRange;
use vt_model::shape::{FillKind, KitLine, Own, Shape, ShapeColumn, on_track};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};

const ROWS: usize = 8;
const KIT: KitLine = KitLine { group: 1, line: 7 };
const NAMES: [&str; 3] = ["BD", "SN", "HH"];

fn column() -> ShapeColumn {
    ShapeColumn::Note {
        scale: Scale::default(),
        writes: Some(0x64),
    }
}

/// Three named tracks in group 1, each with an empty phrase of so many rows.
fn kit_song(rows: usize) -> Arc<Song> {
    let mut song = Song::new(3);
    for (track, name) in NAMES.iter().enumerate() {
        song.tracks[track].name = (*name).to_owned();
        song.tracks[track].group = Some(1);
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(vec![Step::default(); rows])));
    }
    Arc::new(song)
}

fn shape(from: u8) -> Shape {
    Shape {
        kind: FillKind::Mini,
        from,
        seed: 9,
        kit: Some(KIT),
        ..Shape::default()
    }
}

/// `S` across the three tracks: a line on each, one text, each its own note.
fn with_line(text: &str) -> Arc<Song> {
    with_line_of(text, ROWS, 1)
}

/// The same over so many rows, a token `step` rows.
fn with_line_of(text: &str, rows: usize, step: u8) -> Arc<Song> {
    let actions = (0..NAMES.len())
        .map(|track| Action::Generate {
            track,
            phrase: 0,
            column: column(),
            span: Span::rows(StepRange::between(0, rows - 1)),
            shape: Shape {
                mini: Own {
                    step,
                    ..Own::default()
                },
                ..shape(36 + track as u8)
            },
            list: Box::default(),
            rhythm: text.into(),
        })
        .collect();
    edit(&kit_song(rows), &Action::Batch(actions)).unwrap()
}

/// The guide's kit line, as it says it plays.
#[test]
fn the_guides_kit_line_plays_as_it_says() {
    let song = with_line_of("bd . [. bd] . sn . bd ., hh*2 [. hh]", 16, 2);
    assert_eq!(rows(&song, 0), "x....x......x...");
    assert_eq!(rows(&song, 1), "........x.......");
    assert_eq!(rows(&song, 2), "xx.xxx.xxx.xxx.x");
    // The tutorial's, on the `euclid` song's drums, here named `BD`, `SN` and `HH`.
    let song = with_line_of("bd . . bd . . bd ., . . sn . . . sn [. sn], hh*2", 16, 2);
    assert_eq!(rows(&song, 0), "x.....x.....x...");
    assert_eq!(rows(&song, 1), "....x.......x..x");
    assert_eq!(rows(&song, 2), "xxxxxxxxxxxxxxxx");
}

/// The rows a track plays, `x` for a note and `.` for none.
fn rows(song: &Song, track: usize) -> String {
    song.tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            Note::On(_) => 'x',
            _ => '.',
        })
        .collect()
}

fn text(song: &Song, track: usize) -> &str {
    song.tracks[track].phrases[&0].generators()[0].rhythm()
}

fn reline(song: &Arc<Song>, track: usize, text: &str) -> Arc<Song> {
    let generator = &song.tracks[track].phrases[&0].generators()[0];
    edit(
        song,
        &Action::Reshape {
            track,
            phrase: 0,
            generator: 0,
            shape: generator.shape,
            list: None,
            rhythm: Some(text.into()),
        },
    )
    .unwrap()
}

/// Each track plays the rows its name is on, at its own note and the velocity notes are typed
/// with, and every other row rests.
#[test]
fn each_track_plays_the_rows_of_its_name() {
    let song = with_line("bd . sn . bd bd Sn hh");
    assert_eq!(rows(&song, 0), "x...xx..");
    assert_eq!(rows(&song, 1), "..x...x.");
    assert_eq!(rows(&song, 2), ".......x");
    let step = &song.tracks[1].phrases[&0].steps()[2];
    assert_eq!((step.note, step.velocity), (Note::On(37), Some(0x64)));
    assert_eq!(song.kit_names(KIT), NAMES.map(str::to_owned));
}

/// A stack's layers are other tracks', so `bd, hh*4` plays both on their own tracks.
#[test]
fn a_stack_puts_each_layer_on_its_tracks() {
    let song = with_line("bd . . . bd . sn ., hh hh . hh");
    assert_eq!(rows(&song, 0), "x...x...");
    assert_eq!(rows(&song, 1), "......x.");
    assert_eq!(rows(&song, 2), "xx.xxx.x");
}

/// An edit to the line on one track is made on every track, as one edit and one undo step; its
/// `Step` and seed with it, each track keeping its own note.
#[test]
fn an_edit_on_one_track_is_made_on_all() {
    let song = with_line("bd . sn .");
    let edited = reline(&song, 2, "bd hh sn hh");
    for track in 0..NAMES.len() {
        assert_eq!(text(&edited, track), "bd hh sn hh");
    }
    assert_eq!(rows(&edited, 2), ".x.x.x.x");
    let generator = &edited.tracks[0].phrases[&0].generators()[0];
    let shape = Shape {
        mini: Own {
            step: 2,
            ..generator.shape.mini
        },
        seed: 3,
        ..generator.shape
    };
    let stepped = edit(
        &edited,
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape,
            list: None,
            rhythm: None,
        },
    )
    .unwrap();
    let hh = &stepped.tracks[2].phrases[&0].generators()[0].shape;
    assert_eq!((hh.mini.step, hh.seed, hh.from), (2, 3, 38));
    assert_eq!(rows(&stepped, 2), "..x...x.");
}

/// `:name` renames a track's tokens in the line, in the same edit.
#[test]
fn a_rename_renames_its_tokens() {
    let song = with_line("bd [. sn] Bd");
    let renamed = edit(
        &song,
        &Action::SetTrackName {
            track: 0,
            name: "KK".to_owned(),
        },
    )
    .unwrap();
    for track in 0..NAMES.len() {
        assert_eq!(text(&renamed, track), "KK [. sn] KK");
    }
    assert_eq!(rows(&renamed, 0), rows(&song, 0));
}

/// A track that leaves the group leaves the line, its rows kept as plain notes, and the rest
/// play on; `dd` of one track's generator leaves the rest too.
#[test]
fn a_track_leaving_takes_it_out_of_the_line() {
    let song = with_line("bd sn hh .");
    let left = edit(
        &song,
        &Action::SetTrackGroup {
            first: 1,
            last: 1,
            group: None,
        },
    )
    .unwrap();
    assert!(left.tracks[1].phrases[&0].generators().is_empty());
    assert_eq!(rows(&left, 1), rows(&song, 1), "as heard");
    assert_eq!(left.kit_names(KIT), ["BD", "HH"]);
    let edited = reline(&left, 0, "bd bd hh sn");
    assert_eq!(text(&edited, 2), "bd bd hh sn");
    assert_eq!(rows(&edited, 1), rows(&song, 1), "out of the line");
    let removed = edit(
        &song,
        &Action::RemoveGenerator {
            track: 2,
            phrase: 0,
            generator: 0,
        },
    )
    .unwrap();
    assert_eq!(removed.kit_names(KIT), ["BD", "SN"]);
    assert_eq!(text(&removed, 0), "bd sn hh .");
}

/// One seed for the line: each track shaped at once keeps the panel's, and a `?` draws the same
/// on every track, so two tracks of one name play the same rows of it.
#[test]
fn one_seed_for_the_line() {
    for nth in 0..NAMES.len() {
        assert_eq!(on_track(&shape(36), nth).seed, 9);
    }
    let song = with_line("bd? bd? bd? bd? bd? bd? bd? bd?");
    let twins = edit(
        &song,
        &Action::SetTrackName {
            track: 1,
            name: "BD".to_owned(),
        },
    )
    .unwrap();
    let bd = rows(&twins, 0);
    assert!(bd.contains('x') && bd.contains('.'), "{bd}");
    assert_eq!(rows(&twins, 1), bd);
}

/// Saved and read back, each generator takes its track's name again and plays as it did.
#[test]
fn a_kit_line_saves_and_reads_back() {
    let song = with_line("bd . sn hh");
    let folder = std::env::temp_dir().join(format!("vt-kit-line-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    save_song(&folder, &song).unwrap();
    let text = std::fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(text.contains("kit: Some(("), "{text}");
    let loaded = load_song(&folder).unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    for track in 0..NAMES.len() {
        assert_eq!(rows(&loaded, track), rows(&song, track));
        assert_eq!(
            loaded.tracks[track].phrases[&0].generators()[0].kit_name(),
            NAMES[track]
        );
    }
}

/// A track's name is always a token of a kit line: two characters, a letter then a letter, a
/// digit, `-` or `_`, wherever it's set; and two names of one group can't differ only by case,
/// as the line reads them without case (decision 472).
#[test]
fn a_name_is_always_a_token() {
    let song = with_line("bd sn hh .");
    let name = |track: usize, name: &str| {
        edit(
            &song,
            &Action::SetTrackName {
                track,
                name: name.to_owned(),
            },
        )
    };
    for wrong in ["B D", "B*", "B.", "KICK", "1A", "é1"] {
        let refused = name(0, wrong).unwrap_err();
        assert_eq!(refused, EditError::TrackName(wrong.to_owned()), "{wrong}");
        assert!(refused.to_string().contains("a letter then"), "{refused}");
    }
    let dashed = name(0, "B-").unwrap();
    assert_eq!(text(&dashed, 1), "B- sn hh .");
    assert_eq!(rows(&dashed, 0), rows(&song, 0));
    assert_eq!(
        name(1, "bd").unwrap_err(),
        EditError::NameCase("BD".to_owned(), "bd".to_owned())
    );
    assert!(name(1, "BD").is_ok(), "the same name, both playing it");
    // Out of the group a name is the track's own; joining with it is refused.
    let mut apart = Song::clone(&song);
    apart.add_track();
    apart.tracks[3].name = "hh".to_owned();
    assert_eq!(
        join(&Arc::new(apart), 3).unwrap_err(),
        EditError::NameCase("HH".to_owned(), "hh".to_owned())
    );
}

/// A kit line can't be made on a track with no name, as its number isn't a token: the edit says
/// which track (decision 472).
#[test]
fn a_kit_line_needs_its_tracks_named() {
    let mut song = Song::clone(&kit_song(ROWS));
    song.tracks[1].name.clear();
    let refused = edit(
        &Arc::new(song),
        &Action::Generate {
            track: 1,
            phrase: 0,
            column: column(),
            span: Span::rows(StepRange::between(0, ROWS - 1)),
            shape: shape(37),
            list: Box::default(),
            rhythm: "bd 02".into(),
        },
    )
    .unwrap_err();
    assert_eq!(refused, EditError::Unnamed("track 02".to_owned()));
    assert!(refused.to_string().contains(":name it first"), "{refused}");
}

/// The line and a fourth track out of its group, named `CP`, with a phrase of its own, a note on
/// one row if given.
fn with_outsider(text: &str, note: Option<(usize, u8)>) -> Arc<Song> {
    let mut song = Song::clone(&with_line(text));
    song.add_track();
    song.tracks[3].name = "CP".to_owned();
    let mut steps = vec![Step::default(); ROWS];
    if let Some((row, note)) = note {
        steps[row].note = Note::On(note);
    }
    song.tracks[3]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    Arc::new(song)
}

fn join(song: &Arc<Song>, track: usize) -> Result<Arc<Song>, EditError> {
    edit(
        song,
        &Action::SetTrackGroup {
            first: track,
            last: track,
            group: Some(1),
        },
    )
}

/// A track that joins a group with a kit line joins the line in the same edit: it gets the line
/// over the same rows and plays where its name is typed, nothing until then, at its first note
/// there; leaving takes it out again (decision 472).
#[test]
fn a_track_joining_the_group_joins_the_line() {
    let song = with_outsider("bd sn hh .", Some((5, 50)));
    let joined = join(&song, 3).unwrap();
    assert_eq!(text(&joined, 3), "bd sn hh .");
    assert_eq!(
        rows(&joined, 3),
        "........",
        "nothing until its name is typed"
    );
    assert_eq!(joined.kit_names(KIT), ["BD", "SN", "HH", "CP"]);
    let typed = reline(&joined, 0, "bd sn hh cp");
    for track in 0..4 {
        assert_eq!(text(&typed, track), "bd sn hh cp");
    }
    assert_eq!(rows(&typed, 3), "...x...x");
    let step = &typed.tracks[3].phrases[&0].steps()[3];
    assert_eq!(step.note, Note::On(50), "its own note");
    // A name typed before it joined plays as it joins.
    let early = join(&with_outsider("bd cp hh .", None), 3).unwrap();
    assert_eq!(rows(&early, 3), ".x...x..");
    assert_eq!(
        early.tracks[3].phrases[&0].steps()[1].note,
        Note::On(36),
        "the line's From, with no note of its own"
    );
    let left = edit(
        &typed,
        &Action::SetTrackGroup {
            first: 3,
            last: 3,
            group: None,
        },
    )
    .unwrap();
    assert!(left.tracks[3].phrases[&0].generators().is_empty());
    assert_eq!(left.kit_names(KIT), ["BD", "SN", "HH"]);
}

/// Joining is one undo step with the join, and a track with no name can't join a line.
#[test]
fn joining_is_one_edit_and_needs_a_name() {
    let song = with_outsider("bd sn hh cp", None);
    let mut history = History::new(Arc::clone(&song));
    history.commit(join(&song, 3).unwrap());
    assert_eq!(rows(history.current(), 3), "...x...x");
    history.undo();
    assert_eq!(history.current().tracks[3].group, None);
    assert!(
        history.current().tracks[3].phrases[&0]
            .generators()
            .is_empty()
    );
    let mut unnamed = Song::clone(&song);
    unnamed.tracks[3].name.clear();
    assert_eq!(
        join(&Arc::new(unnamed), 3).unwrap_err(),
        EditError::Unnamed("track 04".to_owned())
    );
}
