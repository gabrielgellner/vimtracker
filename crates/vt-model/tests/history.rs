use std::sync::Arc;

use vt_model::action::{Action, EditError, edit};
use vt_model::history::History;
use vt_model::song::Song;
use vt_model::step::Note;

fn set_note(phrase: u32, step: usize, midi: u8) -> Action {
    Action::SetNote {
        phrase,
        step,
        note: Note::On(midi),
        instrument: Some(0),
    }
}

#[test]
fn undo_and_redo_restore_exact_versions() {
    let first = Arc::new(Song::new(2));
    let mut history = History::new(Arc::clone(&first));
    let second = edit(history.current(), &set_note(0, 0, 60)).unwrap();
    history.commit(Arc::clone(&second));
    let third = edit(history.current(), &set_note(0, 1, 62)).unwrap();
    history.commit(Arc::clone(&third));
    assert!(Arc::ptr_eq(history.undo().unwrap(), &second));
    assert!(Arc::ptr_eq(history.undo().unwrap(), &first));
    assert!(history.undo().is_none());
    assert!(Arc::ptr_eq(history.redo().unwrap(), &second));
    assert!(Arc::ptr_eq(history.redo().unwrap(), &third));
    assert!(history.redo().is_none());
}

#[test]
fn a_new_edit_clears_redo() {
    let mut history = History::new(Arc::new(Song::new(1)));
    let second = edit(history.current(), &set_note(0, 0, 60)).unwrap();
    history.commit(second);
    history.undo();
    let other = edit(history.current(), &set_note(0, 0, 64)).unwrap();
    history.commit(other);
    assert!(history.redo().is_none());
}

#[test]
fn unchanged_items_stay_the_same_arc() {
    let song = Arc::new(Song::new(2));
    let next = edit(&song, &set_note(0, 0, 60)).unwrap();
    assert!(!Arc::ptr_eq(&song.phrases[&0], &next.phrases[&0]));
    assert!(Arc::ptr_eq(&song.phrases[&1], &next.phrases[&1]));
    assert!(Arc::ptr_eq(&song.chains[&0], &next.chains[&0]));
    assert!(Arc::ptr_eq(&song.instruments[&0], &next.instruments[&0]));
    assert_eq!(song.phrases[&0].steps()[0].note, Note::Empty);
}

#[test]
fn the_oldest_version_drops_past_the_limit() {
    let first = Arc::new(Song::new(1));
    let mut history = History::with_limit(Arc::clone(&first), 3);
    for midi in 60..66 {
        let next = edit(history.current(), &set_note(0, 0, midi)).unwrap();
        history.commit(next);
    }
    assert_eq!(history.undo_depth(), 3);
    while history.undo().is_some() {}
    assert_eq!(history.current().phrases[&0].steps()[0].note, Note::On(62));
}

#[test]
fn bad_edits_are_refused() {
    let song = Arc::new(Song::new(1));
    assert_eq!(
        edit(&song, &set_note(9, 0, 60)).unwrap_err(),
        EditError::NoPhrase(9)
    );
    assert_eq!(
        edit(&song, &set_note(0, 99, 60)).unwrap_err(),
        EditError::NoStep {
            phrase: 0,
            step: 99
        }
    );
    assert_eq!(
        edit(&song, &Action::SetMilliBpm(10_000)).unwrap_err(),
        EditError::Tempo
    );
}

#[test]
fn set_sample_creates_the_instrument() {
    let song = Arc::new(Song::new(1));
    let next = edit(
        &song,
        &Action::SetSample {
            instrument: 5,
            sample: "samples/a.wav".into(),
        },
    )
    .unwrap();
    assert_eq!(
        next.instruments[&5].sample.as_deref(),
        Some("samples/a.wav")
    );
}
