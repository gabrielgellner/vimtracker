//! `Follow`: a live generator on one track's Note column mirroring another track's phrase, kept in
//! step through `edit`, so an edit to the source rewrites every follower in the same edit.

use std::fs;
use std::sync::Arc;

use vt_model::action::{Action, EditError, FieldValue, edit};
use vt_model::generator::{Span, applied, reads, unfollowed};
use vt_model::lfo::Shape as Wave;
use vt_model::project::{load_song, save_song};
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::shape::{
    Acid, DEFAULT_RATE, FillKind, Follow, Pink, Shape, ShapeColumn, Source, Values, Walk, Where,
};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};

const EMPTY: &str = "--- -- --- --- ---";
const C_MINOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Minor,
};

/// The theme: C-3 D#3 G-3 F-3, the D# accented and sliding into the G.
const THEME: [&str; 4] = [
    "C-3 50 --- --- ---",
    "D#3 7F HLD1 --- --- ---",
    "G-3 50 --- --- ---",
    "F-3 40 --- --- ---",
];

fn steps(texts: &[&str]) -> Vec<Step> {
    texts.iter().map(|text| text.parse().unwrap()).collect()
}

/// A song of `tracks` tracks, the theme on the first's phrase `00` and an empty phrase of
/// `rows` rows on each other.
fn canon(tracks: usize, rows: usize) -> Arc<Song> {
    let mut song = Song::new(tracks);
    song.scale = C_MINOR;
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps(&THEME))));
    for track in song.tracks.iter_mut().take(tracks).skip(1) {
        track
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps(&vec![EMPTY; rows]))));
    }
    Arc::new(song)
}

const fn at(track: usize) -> Source {
    Source { track, phrase: 0 }
}

fn note_column() -> ShapeColumn {
    ShapeColumn::Note {
        scale: C_MINOR,
        writes: Some(0x64),
    }
}

fn following(source: Source, follow: Follow) -> Shape {
    Shape {
        from: 0,
        to: 0,
        curve: 0,
        s: 0,
        kind: FillKind::Follow,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::AllRows,
        seed: 1,
        acid: Acid::default(),
        follow: Follow { source, ..follow },
        walk: Walk::default(),
        pink: Pink::default(),
        vary: 0,
    }
}

fn plain(source: Source) -> Shape {
    following(source, Follow::default())
}

fn follow_on(
    song: &Arc<Song>,
    track: usize,
    rows: usize,
    shape: Shape,
) -> Result<Arc<Song>, EditError> {
    edit(
        song,
        &Action::Generate {
            track,
            phrase: 0,
            column: note_column(),
            span: Span::rows(StepRange::between(0, rows - 1)),
            shape,
        },
    )
}

fn phrase(song: &Song, track: usize) -> &Arc<Phrase> {
    &song.tracks[track].phrases[&0]
}

fn texts(song: &Song, track: usize) -> Vec<String> {
    phrase(song, track)
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn notes(song: &Song, track: usize) -> Vec<String> {
    texts(song, track)
        .iter()
        .map(|text| text[..3].to_owned())
        .collect()
}

fn set_note(song: &Arc<Song>, track: usize, step: usize, note: Note) -> Arc<Song> {
    edit(
        song,
        &Action::SetNote {
            track,
            phrase: 0,
            step,
            note,
        },
    )
    .unwrap()
}

#[test]
fn a_follower_copies_the_source_with_its_velocities_and_gates() {
    let song = follow_on(&canon(2, 4), 1, 4, plain(at(0))).unwrap();
    assert_eq!(texts(&song, 1), texts(&song, 0));
    assert_eq!(phrase(&song, 1).generators().len(), 1);
    assert_eq!(phrase(&song, 1).generators()[0].source(), Some(at(0)));
}

#[test]
fn an_edit_to_the_source_rewrites_every_follower_in_the_same_edit() {
    let start = canon(4, 4);
    let up = Follow {
        transpose: 4,
        ..Follow::default()
    };
    let late = Follow {
        rotate: 1,
        ..Follow::default()
    };
    let one = follow_on(&start, 1, 4, plain(at(0))).unwrap();
    let two = follow_on(&one, 2, 4, following(at(0), up)).unwrap();
    let three = follow_on(&two, 3, 4, following(at(0), late)).unwrap();
    // A fifth up in C minor is four degrees.
    assert_eq!(notes(&three, 2), ["G-3", "A#3", "D-4", "C-4"]);
    assert_eq!(notes(&three, 3), ["F-3", "C-3", "D#3", "G-3"]);
    // One edit to the theme, and every voice has it: one song, so one undo step.
    let edited = set_note(&three, 0, 2, Note::On(70));
    assert_eq!(notes(&edited, 0), ["C-3", "D#3", "A#3", "F-3"]);
    assert_eq!(notes(&edited, 1), ["C-3", "D#3", "A#3", "F-3"]);
    assert_eq!(notes(&edited, 2), ["G-3", "A#3", "F-4", "C-4"]);
    assert_eq!(notes(&edited, 3), ["F-3", "C-3", "D#3", "A#3"]);
    for track in 1..4 {
        assert_eq!(phrase(&edited, track).generators().len(), 1, "{track}");
    }
    assert!(applied(&three, &edited).is_empty());
    // Undo is the song before, where every voice still had the G.
    assert_eq!(notes(&three, 1), ["C-3", "D#3", "G-3", "F-3"]);
    // A velocity and a gate follow too.
    let softer = edit(
        &edited,
        &Action::SetField {
            track: 0,
            phrase: 0,
            step: 0,
            value: FieldValue::Velocity(Some(0x20)),
        },
    )
    .unwrap();
    assert_eq!(texts(&softer, 1)[0], "C-3 20 --- --- ---");
    assert_eq!(texts(&softer, 3)[1], "C-3 20 --- --- ---");
}

#[test]
fn an_edit_elsewhere_leaves_the_followers_as_they_were() {
    let start = follow_on(&canon(3, 4), 1, 4, plain(at(0))).unwrap();
    let edited = set_note(&start, 2, 0, Note::On(60));
    assert!(Arc::ptr_eq(phrase(&start, 1), phrase(&edited, 1)));
    // And an edit to the follower's other rows, past its own, leaves it live.
    let long = follow_on(&canon(2, 6), 1, 4, plain(at(0))).unwrap();
    let beside = set_note(&long, 1, 5, Note::On(72));
    assert_eq!(phrase(&beside, 1).generators().len(), 1);
}

#[test]
fn a_chain_of_followers_follows_all_the_way_along() {
    let start = canon(4, 4);
    let up = Follow {
        transpose: 2,
        ..Follow::default()
    };
    // The second track follows the fourth, which follows the third, which follows the theme, so
    // going through the tracks in turn meets each follower before what it follows.
    let one = follow_on(&start, 2, 4, plain(at(0))).unwrap();
    let two = follow_on(&one, 3, 4, following(at(2), up)).unwrap();
    let three = follow_on(&two, 1, 4, following(at(3), up)).unwrap();
    // Two degrees up in C minor, twice.
    assert_eq!(notes(&three, 3), ["D#3", "G-3", "A#3", "G#3"]);
    assert_eq!(notes(&three, 1), ["G-3", "A#3", "D-4", "C-4"]);
    let edited = set_note(&three, 0, 0, Note::On(62));
    assert_eq!(notes(&edited, 2)[0], "D-3");
    assert_eq!(notes(&edited, 3)[0], "F-3");
    assert_eq!(notes(&edited, 1)[0], "G#3");
}

#[test]
fn a_cycle_is_refused_directly_or_through_others() {
    let start = canon(3, 4);
    assert_eq!(
        follow_on(&start, 0, 4, plain(at(0))).unwrap_err(),
        EditError::FollowsItself
    );
    let one = follow_on(&start, 1, 4, plain(at(0))).unwrap();
    assert_eq!(
        follow_on(&one, 0, 4, plain(at(1))).unwrap_err(),
        EditError::FollowCycle(at(1))
    );
    let two = follow_on(&one, 2, 4, plain(at(1))).unwrap();
    assert!(reads(&two, at(2), at(0)));
    assert!(!reads(&two, at(0), at(2)));
    assert_eq!(
        follow_on(&two, 0, 4, plain(at(2))).unwrap_err(),
        EditError::FollowCycle(at(2))
    );
    // Reshaping onto a cycle is refused as well.
    let reshaped = edit(
        &two,
        &Action::Reshape {
            track: 1,
            phrase: 0,
            generator: 0,
            shape: plain(at(2)),
        },
    );
    assert_eq!(reshaped.unwrap_err(), EditError::FollowCycle(at(2)));
    // A source that isn't there is refused.
    assert_eq!(
        follow_on(
            &start,
            1,
            4,
            plain(Source {
                track: 0,
                phrase: 9
            })
        )
        .unwrap_err(),
        EditError::NoSource(Source {
            track: 0,
            phrase: 9
        })
    );
}

#[test]
fn removing_the_sources_track_applies_its_followers_as_plain_notes() {
    let start = canon(3, 4);
    let mut song = Song::clone(&start);
    // The theme on the last track, followed from the first.
    song.tracks[2]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps(&THEME))));
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps(&[EMPTY; 4]))));
    let live = follow_on(&Arc::new(song), 0, 4, plain(at(2))).unwrap();
    let removed = edit(&live, &Action::RemoveLastTrack).unwrap();
    assert!(phrase(&removed, 0).generators().is_empty());
    assert_eq!(texts(&removed, 0), texts(&live, 0));
    assert_eq!(unfollowed(&live, &removed), [at(2)]);
    assert_eq!(applied(&live, &removed), [note_column()]);
    // Undo has it live again.
    assert_eq!(phrase(&live, 0).generators().len(), 1);
}

#[test]
fn typing_over_a_followers_note_takes_it_over_by_hand() {
    let live = follow_on(&canon(2, 4), 1, 4, plain(at(0))).unwrap();
    let typed = set_note(&live, 1, 1, Note::On(48));
    assert!(phrase(&typed, 1).generators().is_empty());
    assert_eq!(notes(&typed, 1), ["C-3", "C-2", "G-3", "F-3"]);
    assert_eq!(applied(&live, &typed), [note_column()]);
    // Now it's plain, the theme can change without it.
    let edited = set_note(&typed, 0, 0, Note::On(62));
    assert_eq!(notes(&edited, 1)[0], "C-3");
}

#[test]
fn a_short_source_loops_across_the_follower_and_a_long_one_is_cut_off() {
    let looped = follow_on(&canon(2, 8), 1, 8, plain(at(0))).unwrap();
    assert_eq!(
        notes(&looped, 1),
        ["C-3", "D#3", "G-3", "F-3", "C-3", "D#3", "G-3", "F-3"]
    );
    let cut = follow_on(&canon(2, 2), 1, 2, plain(at(0))).unwrap();
    assert_eq!(notes(&cut, 1), ["C-3", "D#3"]);
    // The source's loop is what's read: shortening it rewrites the follower.
    let shorter = edit(
        &looped,
        &Action::SetLength {
            track: 0,
            phrase: 0,
            length: 3,
        },
    )
    .unwrap();
    assert_eq!(
        notes(&shorter, 1),
        ["C-3", "D#3", "G-3", "C-3", "D#3", "G-3", "C-3", "D#3"]
    );
}

#[test]
fn reshaping_a_follower_changes_its_transforms_and_keeps_it_live() {
    let live = follow_on(&canon(2, 4), 1, 4, plain(at(0))).unwrap();
    let mirrored = Follow {
        invert: true,
        reverse: true,
        ..Follow::default()
    };
    let reshaped = edit(
        &live,
        &Action::Reshape {
            track: 1,
            phrase: 0,
            generator: 0,
            shape: following(at(0), mirrored),
        },
    )
    .unwrap();
    // Backwards, F G D# C, then about C in C minor: F is three degrees up so goes three down.
    assert_eq!(notes(&reshaped, 1), ["G-2", "F-2", "G#2", "C-3"]);
    let edited = set_note(&reshaped, 0, 3, Note::On(67));
    assert_eq!(notes(&edited, 1)[0], "F-2");
}

#[test]
fn a_cloned_follower_follows_the_same_source() {
    let live = follow_on(&canon(2, 4), 1, 4, plain(at(0))).unwrap();
    let cloned = edit(
        &live,
        &Action::ClonePhrase {
            track: 1,
            chain: 0,
            slot: 0,
            new: 1,
        },
    )
    .unwrap();
    let copy = &cloned.tracks[1].phrases[&1];
    assert_eq!(copy.generators()[0].source(), Some(at(0)));
    let edited = set_note(&cloned, 0, 0, Note::On(62));
    assert_eq!(notes(&edited, 1)[0], "D-3");
    assert_eq!(edited.tracks[1].phrases[&1].steps()[0].note, Note::On(62));
}

#[test]
fn doubling_a_followers_phrase_applies_it() {
    let live = follow_on(&canon(2, 4), 1, 4, plain(at(0))).unwrap();
    let doubled = edit(
        &live,
        &Action::Double {
            track: 1,
            phrase: 0,
        },
    )
    .unwrap();
    assert!(phrase(&doubled, 1).generators().is_empty());
    assert_eq!(notes(&doubled, 1).len(), 8);
}

#[test]
fn a_follower_stays_live_through_saving_with_its_settings() {
    let late = Follow {
        rotate: 2,
        transpose: -7,
        ..Follow::default()
    };
    let live = follow_on(&canon(2, 4), 1, 4, following(at(0), late)).unwrap();
    let folder = std::env::temp_dir().join(format!("vt-follow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    save_song(&folder, &live).unwrap();
    let text = fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(text.contains("Follow") && text.contains("rotate"), "{text}");
    let loaded = Arc::new(load_song(&folder).unwrap());
    let _ = fs::remove_dir_all(&folder);
    assert_eq!(phrase(&loaded, 1), phrase(&live, 1));
    let edited = set_note(&loaded, 0, 0, Note::On(62));
    // Down a scale's worth of degrees is an octave down; two rows late, the D lands on row 2.
    assert_eq!(notes(&edited, 1), ["G-2", "F-2", "D-2", "D#2"]);
}
