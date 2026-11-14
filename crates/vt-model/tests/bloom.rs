//! Bloom: branches grown from a phrase, each a new phrase following the one before it live and
//! mutating as it copies, with a path through them written into the chain (decision 307).

use std::sync::Arc;

use vt_model::action::{Action, EditError, edit};
use vt_model::bloom::{Bloom, MAX_BRANCHES, Path};
use vt_model::project::{load_song, save_song};
use vt_model::scale::{Scale, ScaleKind};
use vt_model::shape::{Mutations, Source};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};

const C_MINOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Minor,
};

/// Two bars in C minor, with rests for notes to be added on.
const TRUNK: [&str; 32] = [
    "C-3 7F --- --- ---",
    "--- -- --- --- ---",
    "D#3 50 --- --- ---",
    "G-3 64 --- --- ---",
    "--- -- --- --- ---",
    "A#3 50 --- --- ---",
    "C-4 7F --- --- ---",
    "--- -- --- --- ---",
    "G-3 64 --- --- ---",
    "F-3 50 --- --- ---",
    "--- -- --- --- ---",
    "D#3 64 --- --- ---",
    "D-3 50 --- --- ---",
    "--- -- --- --- ---",
    "C-3 64 --- --- ---",
    "G-2 50 --- --- ---",
    "C-3 7F --- --- ---",
    "D-3 50 --- --- ---",
    "D#3 64 --- --- ---",
    "--- -- --- --- ---",
    "F-3 64 --- --- ---",
    "G-3 50 --- --- ---",
    "--- -- --- --- ---",
    "G#3 64 --- --- ---",
    "G-3 7F --- --- ---",
    "--- -- --- --- ---",
    "F-3 50 --- --- ---",
    "D#3 64 --- --- ---",
    "D-3 50 --- --- ---",
    "--- -- --- --- ---",
    "C-3 64 --- --- ---",
    "--- -- --- --- ---",
];

fn steps(texts: &[&str]) -> Vec<Step> {
    texts.iter().map(|text| text.parse().unwrap()).collect()
}

/// One track whose chain `00` plays the trunk, phrase `00`, transposed by 2.
fn song() -> Arc<Song> {
    let mut song = Song::new(1);
    song.scale = C_MINOR;
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps(&TRUNK))));
    let chain = Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap());
    chain.entries[0].transpose = 2;
    Arc::new(song)
}

fn bloom(branches: u8, mutation: u8, mutate: Mutations, path: Path, seed: u64) -> Action {
    Action::Bloom {
        track: 0,
        chain: 0,
        slot: 0,
        scale: C_MINOR,
        bloom: Bloom {
            branches,
            mutation,
            mutate,
            path,
            seed,
        },
    }
}

fn phrase(song: &Song, id: u32) -> Vec<Step> {
    song.tracks[0].phrases[&id].steps().to_vec()
}

fn differing(a: &[Step], b: &[Step]) -> usize {
    a.iter().zip(b).filter(|(a, b)| a != b).count()
}

fn chain(song: &Song) -> Vec<(u32, i8)> {
    song.tracks[0].chains[&0]
        .entries
        .iter()
        .map(|entry| (entry.phrase, entry.transpose))
        .collect()
}

/// Only the kinds that change a row a time, so a mutation's size is the rows it changes.
const ROW_AT_A_TIME: Mutations = Mutations {
    repitch: true,
    shift: false,
    retro: false,
    ratchet: true,
    drop: true,
};

#[test]
fn each_branch_is_a_new_phrase_following_the_one_before_it() {
    let grown = edit(
        &song(),
        &bloom(3, 2, Mutations::default(), Path::Forward, 9),
    )
    .unwrap();
    let phrases: Vec<u32> = grown.tracks[0].phrases.keys().copied().collect();
    assert_eq!(phrases, [0, 1, 2, 3]);
    for branch in 1..=3 {
        let generators = grown.tracks[0].phrases[&branch].generators();
        assert_eq!(generators.len(), 1);
        assert_eq!(
            generators[0].source(),
            Some(Source {
                track: 0,
                phrase: branch - 1
            }),
        );
        let follow = generators[0].shape.follow;
        assert!(
            follow.fx,
            "a branch copies the FX, as it's on the same track"
        );
        assert_eq!(follow.mutation, 2);
    }
    assert!(grown.tracks[0].phrases[&0].generators().is_empty());
    assert_eq!(chain(&grown), [(0, 2), (1, 2), (2, 2), (3, 2)]);
}

#[test]
fn a_branch_differs_from_the_one_before_by_about_mutation_and_from_the_trunk_by_more() {
    let trunk = steps(&TRUNK);
    for mutation in [1u8, 2, 4] {
        let (mut step_apart, mut from_trunk_first, mut from_trunk_last) = (0, 0, 0);
        let seeds = 24;
        for seed in 0..seeds {
            let grown = edit(
                &song(),
                &bloom(MAX_BRANCHES, mutation, ROW_AT_A_TIME, Path::Forward, seed),
            )
            .unwrap();
            for branch in 1..=u32::from(MAX_BRANCHES) {
                let apart = differing(&phrase(&grown, branch), &phrase(&grown, branch - 1));
                assert!(
                    apart <= usize::from(mutation),
                    "a change a row: {apart} rows for {mutation}"
                );
                step_apart += apart;
            }
            from_trunk_first += differing(&phrase(&grown, 1), &trunk);
            from_trunk_last += differing(&phrase(&grown, u32::from(MAX_BRANCHES)), &trunk);
        }
        let mean = step_apart as f64 / f64::from(seeds as u32 * u32::from(MAX_BRANCHES));
        assert!(
            mean >= 0.6 * f64::from(mutation),
            "about {mutation} rows a branch, but {mean}"
        );
        assert!(
            from_trunk_last > 2 * from_trunk_first,
            "the last branch strays further: {from_trunk_last} rows against {from_trunk_first}"
        );
    }
}

#[test]
fn every_branch_keeps_its_notes_in_the_scale() {
    let mutate = Mutations {
        shift: true,
        ..Mutations::default()
    };
    for seed in 0..16 {
        let grown = edit(
            &song(),
            &bloom(MAX_BRANCHES, 6, mutate, Path::Forward, seed),
        )
        .unwrap();
        for branch in 1..=u32::from(MAX_BRANCHES) {
            for step in phrase(&grown, branch) {
                if let Note::On(note) = step.note {
                    assert!(C_MINOR.contains(note), "{step} in branch {branch}");
                }
            }
        }
    }
}

#[test]
fn a_seed_grows_the_same_branches_and_another_seed_others() {
    let grow = |seed| {
        edit(
            &song(),
            &bloom(4, 3, Mutations::default(), Path::Shuffle, seed),
        )
    };
    let once = grow(5).unwrap();
    let again = grow(5).unwrap();
    for branch in 1..=4 {
        assert_eq!(phrase(&once, branch), phrase(&again, branch));
    }
    assert_eq!(chain(&once), chain(&again));
    let other = grow(6).unwrap();
    assert!((1..=4).any(|branch| phrase(&once, branch) != phrase(&other, branch)));
}

#[test]
fn an_edit_to_the_trunk_grows_every_branch_again_in_one_step() {
    let grown = edit(&song(), &bloom(3, 2, ROW_AT_A_TIME, Path::Forward, 3)).unwrap();
    // The trunk's G-3 on row 8 becomes an F-3: every branch that didn't mutate that row has it.
    let edited = edit(
        &grown,
        &Action::SetNote {
            track: 0,
            phrase: 0,
            step: 8,
            note: Note::On(65),
        },
    )
    .unwrap();
    let mut reached = 0;
    for branch in 1..=3 {
        let before = phrase(&grown, branch);
        let after = phrase(&edited, branch);
        assert_ne!(before, after, "branch {branch} grew again");
        if after[8].note == Note::On(65) {
            reached += 1;
        }
        assert_eq!(
            edited.tracks[0].phrases[&branch].generators().len(),
            1,
            "still live"
        );
    }
    assert!(reached >= 2, "{reached} branches play the new note");
}

#[test]
fn each_path_takes_the_trunks_row_in_the_chain() {
    let mut song = Song::clone(&song());
    let chain_now = Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap());
    // A chain of the trunk, then phrase 0A after it, which stays after the path.
    chain_now.entries.push(vt_model::song::ChainEntry {
        phrase: 0x0A,
        transpose: 0,
    });
    song.tracks[0]
        .phrases
        .insert(0x0A, Arc::new(Phrase::from_steps(steps(&TRUNK[..4]))));
    let song = Arc::new(song);
    let cases = [
        (Path::Forward, vec![0, 1, 2]),
        (Path::Trunk, vec![0, 1, 0, 2]),
        (Path::Pendulum, vec![0, 1, 2, 1]),
        (Path::PingPong, vec![0, 1, 2, 2, 1, 0]),
    ];
    for (path, places) in cases {
        let grown = edit(&song, &bloom(2, 1, Mutations::default(), path, 1)).unwrap();
        let mut wanted: Vec<(u32, i8)> = places.into_iter().map(|id| (id, 2)).collect();
        wanted.push((0x0A, 0));
        assert_eq!(chain(&grown), wanted, "{path}");
    }
}

#[test]
fn a_bloom_takes_the_lowest_free_ids_and_is_refused_past_seven_branches_or_ff() {
    let mut song = Song::clone(&song());
    song.tracks[0]
        .phrases
        .insert(2, Arc::new(Phrase::from_steps(steps(&TRUNK[..4]))));
    let song = Arc::new(song);
    let grown = edit(&song, &bloom(2, 1, Mutations::default(), Path::Forward, 1)).unwrap();
    assert_eq!(chain(&grown), [(0, 2), (1, 2), (3, 2)]);
    assert_eq!(
        edit(&song, &bloom(8, 1, Mutations::default(), Path::Forward, 1)),
        Err(EditError::Branches)
    );
    assert_eq!(
        edit(&song, &bloom(0, 1, Mutations::default(), Path::Forward, 1)),
        Err(EditError::Branches)
    );
    let mut full = Song::clone(&song);
    for id in 1..=0xFF {
        full.tracks[0]
            .phrases
            .insert(id, Arc::new(Phrase::from_steps(steps(&TRUNK[..1]))));
    }
    assert_eq!(
        edit(
            &Arc::new(full),
            &bloom(1, 1, Mutations::default(), Path::Forward, 1)
        ),
        Err(EditError::NoPhrasesLeft(1))
    );
}

#[test]
fn branches_save_and_load_still_live_and_mutating() {
    let grown = edit(&song(), &bloom(3, 2, Mutations::default(), Path::Trunk, 4)).unwrap();
    let folder = std::env::temp_dir().join(format!("vt-bloom-{}", std::process::id()));
    save_song(&folder, &grown).unwrap();
    let text = std::fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(text.contains("mutation: 2"), "the mutation is saved");
    let loaded = Arc::new(load_song(&folder).unwrap());
    let _ = std::fs::remove_dir_all(&folder);
    for branch in 1..=3 {
        assert_eq!(phrase(&loaded, branch), phrase(&grown, branch));
        assert_eq!(
            loaded.tracks[0].phrases[&branch].generators(),
            grown.tracks[0].phrases[&branch].generators()
        );
    }
    let edited = edit(
        &loaded,
        &Action::SetNote {
            track: 0,
            phrase: 0,
            step: 0,
            note: Note::On(55),
        },
    )
    .unwrap();
    assert_ne!(phrase(&edited, 1), phrase(&loaded, 1));
}
