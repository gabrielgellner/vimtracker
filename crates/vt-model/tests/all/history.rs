use std::sync::Arc;

use vt_model::action::{Action, EditError, KitSound, edit};
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::constants::{MAX_SOUND_TRACKS, UNITY_LEVEL};
use vt_model::history::History;
use vt_model::song::{Launch, Song, TrackKind};
use vt_model::step::Note;

fn set_note(phrase: u32, step: usize, midi: u8) -> Action {
    Action::SetNote {
        track: 0,
        phrase,
        step,
        note: Note::On(midi),
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
    assert!(!Arc::ptr_eq(
        &song.tracks[0].phrases[&0],
        &next.tracks[0].phrases[&0]
    ));
    assert!(Arc::ptr_eq(
        &song.tracks[1].phrases[&0],
        &next.tracks[1].phrases[&0]
    ));
    assert!(Arc::ptr_eq(
        &song.tracks[0].chains[&0],
        &next.tracks[0].chains[&0]
    ));
    assert_eq!(song.tracks[0].phrases[&0].steps()[0].note, Note::Empty);
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
    assert_eq!(
        history.current().tracks[0].phrases[&0].steps()[0].note,
        Note::On(62)
    );
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
fn a_tracks_sample_and_name_are_set_on_it() {
    let song = Arc::new(Song::new(2));
    let next = edit(
        &song,
        &Action::SetBankEntry {
            number: 3,
            entry: Some(BankEntry::of_file("samples/a.wav", ContentHash::of(b"a"))),
        },
    )
    .unwrap();
    assert_eq!(next.bank.get(3).unwrap().name, "a");
    let mut synth = Song::clone(&next);
    synth.tracks[1].machine = vt_model::song::Machine::Plaits(Default::default());
    let next = edit(
        &Arc::new(synth),
        &Action::SetTrackSample {
            track: 1,
            sample: 3,
        },
    )
    .unwrap();
    assert_eq!(next.tracks[1].sample(), 3);
    assert_eq!(
        next.tracks[1].machine,
        vt_model::song::Machine::Sampler,
        "choosing a sample means wanting to hear it"
    );
    assert_eq!(next.tracks[0].sample(), 0);
    let emptied = edit(
        &next,
        &Action::SetBankEntry {
            number: 3,
            entry: None,
        },
    )
    .unwrap();
    assert!(emptied.bank.is_empty());
    assert_eq!(
        emptied.tracks[1].sample(),
        3,
        "a track keeps its number, which plays nothing now"
    );
    let next = edit(
        &next,
        &Action::SetTrackName {
            track: 1,
            name: "BD".into(),
        },
    )
    .unwrap();
    assert_eq!(next.tracks[1].name, "BD");
    let bad = Action::SetTrackSample {
        track: 9,
        sample: 0,
    };
    assert_eq!(edit(&next, &bad), Err(EditError::NoTrack(9)));
}

#[test]
fn a_group_is_named_with_up_to_five_letters_or_digits() {
    let song = Arc::new(Song::new(1));
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    let name = |name: &str| Action::SetTrackName {
        track: group,
        name: name.into(),
    };
    let named = edit(&song, &name("DRUMS")).unwrap();
    assert_eq!(named.tracks[group].name, "DRUMS");
    assert_eq!(edit(&song, &name("DRUMSS")), Err(EditError::GroupName));
    assert_eq!(edit(&song, &name("K-8")), Err(EditError::GroupName));
    let cleared = edit(&named, &name("")).unwrap();
    assert_eq!(cleared.tracks[group].name, "G1", "back to its letter");
}

#[test]
fn a_kit_fills_tracks_from_the_first_and_adds_what_it_needs() {
    let mut start = Song::new(1);
    start.tracks[0].cutoff = 0x40;
    let start = Arc::new(start);
    let sound = |sample: u8, name: &str| KitSound {
        sample,
        name: name.into(),
        choke: None,
    };
    let hat = KitSound {
        choke: Some(1),
        ..sound(2, "CH")
    };
    let kit = Action::LoadKit(vec![sound(1, "BD"), hat]);
    let next = edit(&start, &kit).unwrap();
    assert_eq!(next.sound_tracks(), 2);
    assert_eq!(next.sections[0].chains.len(), next.tracks.len());
    assert_eq!(next.tracks[0].sample(), 1);
    assert_eq!(next.tracks[0].name, "BD");
    assert_eq!(next.tracks[0].cutoff, 0x40, "other settings stay");
    assert_eq!(next.tracks[1].sample(), 2);
    let hat = &next.tracks[1];
    assert_eq!(
        (hat.level, hat.pitch, hat.choke),
        (UNITY_LEVEL, 0x80, Some(1)),
        "a sound's gain and tune are its bank entry's, not the track's"
    );
    let huge = Action::LoadKit(vec![sound(1, ""); MAX_SOUND_TRACKS + 1]);
    assert_eq!(edit(&start, &huge), Err(EditError::TooManyTracks));
    let bad = Action::LoadKit(vec![KitSound {
        choke: Some(17),
        ..sound(1, "")
    }]);
    assert_eq!(edit(&start, &bad), Err(EditError::Choke(17)));
}

#[test]
fn a_choke_group_is_one_to_sixteen_or_none() {
    let start = Arc::new(Song::new(1));
    let choke = |choke| Action::SetTrackChoke { track: 0, choke };
    let next = edit(&start, &choke(Some(16))).unwrap();
    assert_eq!(next.tracks[0].choke, Some(16));
    assert_eq!(edit(&next, &choke(None)).unwrap().tracks[0].choke, None);
    assert_eq!(edit(&start, &choke(Some(0))), Err(EditError::Choke(0)));
    assert_eq!(edit(&start, &choke(Some(17))), Err(EditError::Choke(17)));
}

mod tree {
    use std::sync::Arc;

    use vt_model::action::{Action, EditError, edit};
    use vt_model::song::{ChainEntry, Song};

    fn entry(phrase: u32) -> ChainEntry {
        ChainEntry {
            phrase,
            transpose: 0,
        }
    }

    #[test]
    fn cloning_a_phrase_leaves_the_original_and_repoints_only_that_row() {
        let song = Arc::new(Song::new(1));
        let song = edit(
            &song,
            &Action::InsertChainEntry {
                track: 0,
                chain: 0,
                at: 1,
                entry: entry(0),
            },
        )
        .unwrap();
        let cloned = edit(
            &song,
            &Action::ClonePhrase {
                track: 0,
                chain: 0,
                slot: 1,
                new: 1,
            },
        )
        .unwrap();
        assert_eq!(cloned.tracks[0].chains[&0].entries[0].phrase, 0);
        assert_eq!(cloned.tracks[0].chains[&0].entries[1].phrase, 1);
        let edited = edit(
            &cloned,
            &Action::SetLength {
                track: 0,
                phrase: 1,
                length: 4,
            },
        )
        .unwrap();
        assert_eq!(
            edited.tracks[0].phrases[&0].len(),
            16,
            "the original is unchanged"
        );
        assert_eq!(edited.tracks[0].phrases[&1].len(), 4);
    }

    #[test]
    fn a_deep_clone_copies_each_phrase_once_and_repoints_the_track() {
        let song = Arc::new(Song::new(2));
        let song = edit(
            &song,
            &Action::InsertChainEntry {
                track: 0,
                chain: 0,
                at: 1,
                entry: entry(0),
            },
        )
        .unwrap();
        let cloned = edit(
            &song,
            &Action::CloneChain {
                section: 0,
                track: 0,
            },
        )
        .unwrap();
        assert_eq!(cloned.sections[0].chains[0], Some(1));
        assert_eq!(
            cloned.sections[0].chains[1],
            Some(0),
            "the other track's own"
        );
        let copy = &cloned.tracks[0].chains[&1].entries;
        assert_eq!([copy[0].phrase, copy[1].phrase], [1, 1]);
        assert_eq!(cloned.tracks[0].chains[&0].entries[0].phrase, 0);
    }

    #[test]
    fn a_new_reference_creates_the_phrase_and_rows_keep_one() {
        let song = Arc::new(Song::new(1));
        let song = edit(
            &song,
            &Action::SetChainEntry {
                track: 0,
                chain: 0,
                slot: 0,
                entry: entry(9),
            },
        )
        .unwrap();
        assert!(song.tracks[0].phrases.contains_key(&9));
        assert_eq!(
            edit(
                &song,
                &Action::RemoveChainEntry {
                    track: 0,
                    chain: 0,
                    at: 0
                }
            )
            .unwrap_err(),
            EditError::LastRow
        );
        let far = ChainEntry {
            phrase: 0,
            transpose: 49,
        };
        assert_eq!(
            edit(
                &song,
                &Action::SetChainEntry {
                    track: 0,
                    chain: 0,
                    slot: 0,
                    entry: far,
                }
            )
            .unwrap_err(),
            EditError::Transpose
        );
    }

    #[test]
    fn pointing_a_track_at_a_new_chain_creates_it_with_a_phrase() {
        let song = Arc::new(Song::new(1));
        let song = edit(
            &song,
            &Action::SetTrackChain {
                section: 0,
                track: 0,
                chain: 5,
            },
        )
        .unwrap();
        assert_eq!(song.sections[0].chains[0], Some(5));
        assert_eq!(song.tracks[0].chains[&5].entries[0].phrase, 1);
        assert!(song.tracks[0].phrases.contains_key(&1));
    }
}

#[test]
fn a_chain_label_and_launch_timing_are_edits() {
    let song = Arc::new(Song::new(1));
    let labelled = edit(
        &song,
        &Action::SetChainLabel {
            track: 0,
            chain: 0,
            label: "bass".to_owned(),
        },
    )
    .unwrap();
    assert_eq!(labelled.tracks[0].chains[&0].label, "bass");
    assert_eq!(song.tracks[0].chains[&0].label, "");
    let missing = edit(
        &song,
        &Action::SetChainLabel {
            track: 0,
            chain: 9,
            label: "bass".to_owned(),
        },
    );
    assert_eq!(missing, Err(EditError::NoChain(9)));
    let timed = edit(&song, &Action::SetLaunch(Launch::ChainEnd)).unwrap();
    assert_eq!(timed.launch, Launch::ChainEnd);
}

#[test]
fn a_version_taken_back_leaves_no_undo_step_and_nothing_to_redo() {
    let song = Arc::new(Song::new(1));
    let mut history = History::new(Arc::clone(&song));
    let first = edit(&song, &set_note(0, 0, 60)).unwrap();
    history.commit(Arc::clone(&first));
    let second = edit(&first, &set_note(0, 1, 62)).unwrap();
    history.commit(second);
    assert!(history.take_back().is_some());
    assert!(Arc::ptr_eq(history.current(), &first));
    assert_eq!(history.undo_depth(), 1);
    assert!(history.redo().is_none(), "nothing to redo");
    history.take_back();
    assert!(history.take_back().is_none(), "nothing before the first");
}
