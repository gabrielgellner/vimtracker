//! Slicing a bank entry: slices numbered by where they start, as Vim numbers lines, ids that are
//! never used twice, and a re-slice that has the steps choosing a slice by number follow the
//! sound they played, on tracks playing the entry and no others, as one undo step.

use std::sync::Arc;

use vt_model::action::{Action, EditError, edit};
use vt_model::bank::{BankEntry, ContentHash, MAX_SLICES, Points, SliceParams};
use vt_model::history::History;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_model::step::FxCommand;

/// A second's entry, at 48 kHz.
const LENGTH: u64 = 48_000;

fn starts(points: &Points) -> Vec<u64> {
    points.slices.iter().map(|slice| slice.start).collect()
}

fn ids(points: &Points) -> Vec<u32> {
    points.slices.iter().map(|slice| slice.id).collect()
}

#[test]
fn slices_are_numbered_by_where_they_start_and_end_at_the_next() {
    let mut points = Points {
        start: 1000,
        end: Some(41_000),
        ..Points::default()
    };
    assert_eq!(points.slice_count(), 1, "with none, the entry is one");
    assert_eq!(points.slice_frames(5, LENGTH), 1000..41_000);
    points.reslice(&[21_000, 1000, 11_000, 31_000, 11_000]);
    assert_eq!(
        starts(&points),
        [1000, 11_000, 21_000, 31_000],
        "in order, once"
    );
    assert_eq!(points.slice_frames(1, LENGTH), 11_000..21_000);
    assert_eq!(points.slice_frames(3, LENGTH), 31_000..41_000);
    assert_eq!(
        points.slice_frames(0xFF, LENGTH),
        31_000..41_000,
        "clamped to the last"
    );
    assert_eq!(points.slice_at(500), 0);
    assert_eq!(points.slice_at(21_000), 2);
    assert_eq!(
        points.even_starts(4, LENGTH),
        [1000, 11_000, 21_000, 31_000]
    );
    assert_eq!(points.even_starts(1000, LENGTH).len(), MAX_SLICES);
}

#[test]
fn ids_are_never_used_twice_and_a_slice_where_one_was_keeps_its_id_and_settings() {
    let mut points = Points::default();
    let four = points.even_starts(4, LENGTH);
    points.reslice(&four);
    assert_eq!(ids(&points), [0, 1, 2, 3]);
    points.slices[2].over.gain_db = Some(-3.0);
    // Eight slices keep the four that start where they did.
    let eight = points.even_starts(8, LENGTH);
    let renumbered = points.reslice(&eight);
    assert_eq!(renumbered.as_deref(), Some(&[0, 2, 4, 6][..]));
    assert_eq!(ids(&points), [0, 4, 1, 5, 2, 6, 3, 7]);
    assert_eq!(points.slices[4].over.gain_db, Some(-3.0));
    // Clearing and slicing again gives new ids.
    assert_eq!(points.reslice(&[]), None);
    assert_eq!(points.slice_count(), 1);
    points.reslice(&[0, 24_000]);
    assert_eq!(ids(&points), [8, 9]);
    let text = ron::to_string(&points).unwrap();
    assert_eq!(ron::from_str::<Points>(&text).unwrap(), points);
}

#[test]
fn a_reslice_moves_each_slice_to_the_one_holding_its_start() {
    let mut points = Points::default();
    points.reslice(&[0, 12_000, 24_000, 36_000]);
    // Two slices: the first two old ones are in the first, the others in the second.
    let renumbered = points.reslice(&[0, 20_000]);
    assert_eq!(renumbered.as_deref(), Some(&[0, 0, 1, 1][..]));
}

/// A song with three sampler tracks: two playing entry `01`, one with `SMP 02` locked on a step,
/// and one playing `02`, each with `SLI` locks.
fn song() -> Song {
    let mut song = Song::new(3);
    for number in [1, 2] {
        let mut entry = BankEntry::of_file(format!("{number}.wav"), ContentHash::UNKNOWN);
        let four = entry.edit.points.even_starts(4, LENGTH);
        entry.edit.points.reslice(&four);
        song.bank.set(number, Some(entry));
    }
    let rows = [
        [
            "C-4 -- sampler.slice:01 --- ---",
            "C-4 -- sampler.slice:03 --- ---",
            "C-4 -- sampler.slice:09 --- ---",
            "C-4 -- sampler.sample:02 sampler.slice:03 ---",
        ],
        [
            "C-4 -- sampler.slice:02 --- ---",
            "C-4 -- --- --- ---",
            "C-4 -- --- --- ---",
            "C-4 -- --- --- ---",
        ],
    ];
    for (track, number) in [(0, 1), (1, 2)] {
        song.tracks[track].machine = Machine::Sampler;
        song.tracks[track].set_setting(SettingId::SAMPLE, number);
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        for (at, text) in rows[track].iter().enumerate() {
            *phrase.step_mut(at).unwrap() = text.parse().unwrap();
        }
    }
    song
}

/// The `SLI` each step of a track's first phrase locks.
fn locks(song: &Song, track: usize) -> Vec<Option<u8>> {
    song.tracks[track].phrases[&0]
        .steps()
        .iter()
        .take(4)
        .map(|step| {
            step.fx
                .iter()
                .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SLICE)))
                .map(|slot| slot.coarse())
        })
        .collect()
}

#[test]
fn a_reslice_rewrites_the_locks_on_steps_playing_the_entry_and_undo_takes_both_back() {
    let first = Arc::new(song());
    let mut history = History::new(Arc::clone(&first));
    // Halving 01's four slices into eight: its slice 1 is now 2, 3 is 6, and 9, past the last,
    // played the last, which is now 6.
    let starts = first
        .bank
        .get(1)
        .unwrap()
        .edit
        .points
        .even_starts(8, LENGTH);
    let next = edit(history.current(), &Action::SetSlices { number: 1, starts }).unwrap();
    history.commit(Arc::clone(&next));
    assert_eq!(
        locks(&next, 0),
        [Some(2), Some(6), Some(6), Some(3)],
        "SMP 02 is left"
    );
    assert_eq!(
        locks(&next, 1),
        [Some(2), None, None, None],
        "a track playing 02 is left"
    );
    assert_eq!(next.bank.get(1).unwrap().edit.points.slices.len(), 8);
    assert!(Arc::ptr_eq(
        &next.tracks[1].phrases[&0],
        &first.tracks[1].phrases[&0]
    ));
    // Through a step's own SMP: slicing 02 moves that step's lock, and track 1's.
    let other = first
        .bank
        .get(2)
        .unwrap()
        .edit
        .points
        .even_starts(2, LENGTH);
    let both = edit(
        &next,
        &Action::SetSlices {
            number: 2,
            starts: other,
        },
    )
    .unwrap();
    assert_eq!(locks(&both, 0), [Some(2), Some(6), Some(6), Some(1)]);
    assert_eq!(locks(&both, 1), [Some(1), None, None, None]);
    let back = history.undo().unwrap();
    assert!(Arc::ptr_eq(back, &first));
    assert_eq!(locks(back, 0), [Some(1), Some(3), Some(9), Some(3)]);
}

#[test]
fn clearing_or_slicing_for_the_first_time_leaves_the_locks() {
    let first = Arc::new(song());
    let cleared = edit(
        &first,
        &Action::SetSlices {
            number: 1,
            starts: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(cleared.bank.get(1).unwrap().edit.points.slice_count(), 1);
    assert_eq!(locks(&cleared, 0), locks(&first, 0));
    let again = edit(
        &cleared,
        &Action::SetSlices {
            number: 1,
            starts: vec![0, 24_000],
        },
    )
    .unwrap();
    assert_eq!(
        locks(&again, 0),
        locks(&first, 0),
        "SLI 00 to 0F, written first, play in order"
    );
    let ids: Vec<u32> = again
        .bank
        .get(1)
        .unwrap()
        .edit
        .points
        .slices
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [4, 5], "after the four cleared");
    assert_eq!(
        edit(
            &first,
            &Action::SetSlices {
                number: 9,
                starts: vec![0]
            }
        ),
        Err(EditError::NoSample(9))
    );
}

#[test]
fn a_slices_settings_are_its_entrys_where_it_has_none() {
    let mut entry = BankEntry::of_file("a.wav", ContentHash::UNKNOWN);
    entry.edit.play.gain_db = -6.0;
    entry.edit.play.cents = 20;
    let over = SliceParams {
        tune: Some(7),
        ..SliceParams::default()
    };
    let play = over.over(entry.edit.play);
    assert_eq!((play.gain_db, play.tune, play.cents), (-6.0, 7, 20));
    assert_eq!(SliceParams::default().pan_gains(), [1.0, 1.0]);
    let right = SliceParams {
        pan: Some(50),
        ..SliceParams::default()
    }
    .pan_gains();
    assert!(right[0].abs() < 1e-6 && (right[1] - std::f32::consts::SQRT_2).abs() < 1e-6);
}
