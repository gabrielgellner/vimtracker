//! Marks: letters naming a bank entry's slices, as Vim's marks name lines, which follow their
//! slices through a split, a join and a re-slice as one undo step with the edit, and `SLI 'a`,
//! a step locking a slice by its letter, which needs no rewriting as slices renumber.

use std::sync::Arc;

use vt_model::action::{Action, EditError, Fill, edit};
use vt_model::bank::{BankEntry, ContentHash, Points};
use vt_model::history::History;
use vt_model::selection::{FX_GROUP, Scope, Shift, StepRange, shift_step};
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Song};
use vt_model::step::{FxCommand, FxSlot, Mark, Step};

/// A second's entry, at 48 kHz.
const LENGTH: u64 = 48_000;

/// A song whose track 0 plays entry `01`, cut into four slices of 12 000 frames, with `a` on
/// slice 1, `b` and `c` on slice 2 and `d` on slice 3, and steps locking `SLI 'b`, `SLI 02` and
/// `SLI 'z`, which names nothing.
fn song() -> Song {
    let mut song = Song::new(1);
    let mut entry = BankEntry::of_file("break.wav", ContentHash::UNKNOWN);
    let points = &mut entry.edit.points;
    let four = points.even_starts(4, LENGTH);
    points.reslice(&four);
    for (letter, slice) in [('a', 1), ('b', 2), ('c', 2), ('d', 3)] {
        assert!(points.set_mark(letter, slice));
    }
    song.bank.set(1, Some(entry));
    song.tracks[0].machine = Machine::Sampler;
    song.tracks[0].set_setting(SettingId::SAMPLE, 1);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (at, text) in [
        "C-3 -- sampler.slice:'b --- ---",
        "C-3 -- sampler.slice:02 --- ---",
        "C-3 -- sampler.slice:'z --- ---",
    ]
    .iter()
    .enumerate()
    {
        *phrase.step_mut(at).unwrap() = text.parse().unwrap();
    }
    song
}

fn points(song: &Song) -> &Points {
    &song.bank.get(1).unwrap().edit.points
}

/// Each letter with the number of the slice it names, in letter order.
fn marked(song: &Song) -> Vec<(char, Option<usize>)> {
    let points = points(song);
    points
        .marks
        .keys()
        .map(|letter| (*letter, points.marked(*letter)))
        .collect()
}

/// The `SLI` on each of the first three steps, as its text.
fn locks(song: &Song) -> Vec<String> {
    song.tracks[0].phrases[&0]
        .steps()
        .iter()
        .take(3)
        .map(|step| step.fx[0].to_string())
        .collect()
}

/// An edit committed to the history, then undone and redone, checking each gives back the song
/// it should, and the edited song.
fn committed(action: &Action) -> (Arc<Song>, Arc<Song>) {
    let first = Arc::new(song());
    let mut history = History::new(Arc::clone(&first));
    let next = edit(history.current(), action).unwrap();
    history.commit(Arc::clone(&next));
    let back = history.undo().unwrap();
    assert!(Arc::ptr_eq(back, &first), "one undo takes it all back");
    assert_eq!(marked(back), marked(&first));
    let again = history.redo().unwrap();
    assert!(Arc::ptr_eq(again, &next), "and redo brings it all back");
    (first, next)
}

#[test]
fn a_split_leaves_the_marks_on_the_part_that_keeps_its_start() {
    // Slice 1, named a, from 12 000, split at 18 000.
    let (first, next) = committed(&Action::SplitSlice {
        number: 1,
        at: 18_000,
    });
    let before = &points(&first).slices[1];
    let after = points(&next);
    assert_eq!(after.slices.len(), 5);
    assert_eq!(after.slices[1].id, before.id, "the left part keeps the id");
    assert_eq!(after.slices[2].start, 18_000);
    assert_eq!(
        marked(&next),
        [
            ('a', Some(1)),
            ('b', Some(3)),
            ('c', Some(3)),
            ('d', Some(4))
        ]
    );
    // `SLI 'b` needs no rewriting, and `SLI 02` follows its sound, which is now slice 3.
    assert_eq!(
        locks(&next),
        ["sampler.slice:'b", "sampler.slice:03", "sampler.slice:'z"]
    );
    assert_eq!(
        after.marked('b'),
        Some(3),
        "'b resolves to its slice's new number"
    );
}

#[test]
fn a_join_gives_the_slice_left_the_marks_of_the_one_it_takes_in() {
    // Slice 1, named a, joined to slice 2, named b and c.
    let (first, next) = committed(&Action::JoinSlices {
        number: 1,
        slice: 1,
    });
    let after = points(&next);
    assert_eq!(after.slices.len(), 3);
    assert_eq!(after.slices[1].id, points(&first).slices[1].id);
    assert_eq!(
        marked(&next),
        [
            ('a', Some(1)),
            ('b', Some(1)),
            ('c', Some(1)),
            ('d', Some(2))
        ],
        "b and c move to the survivor"
    );
    assert_eq!(
        locks(&next),
        ["sampler.slice:'b", "sampler.slice:01", "sampler.slice:'z"]
    );
    assert_eq!(
        edit(
            &first,
            &Action::JoinSlices {
                number: 1,
                slice: 3
            }
        ),
        Err(EditError::Join(3)),
        "the last has none after it"
    );
    assert_eq!(
        edit(
            &first,
            &Action::SplitSlice {
                number: 1,
                at: 24_000
            }
        ),
        Err(EditError::Split),
        "a slice starts there already"
    );
}

#[test]
fn a_reslice_moves_each_mark_to_the_slice_holding_its_old_slices_start() {
    // From four slices to three, at 0, 16 000 and 32 000: slice 1 started at 12 000, in the new
    // first; slice 2 at 24 000, in the second; slice 3 at 36 000, in the third.
    let (_, next) = committed(&Action::SetSlices {
        number: 1,
        starts: vec![0, 16_000, 32_000],
    });
    assert_eq!(
        marked(&next),
        [
            ('a', Some(0)),
            ('b', Some(1)),
            ('c', Some(1)),
            ('d', Some(2))
        ]
    );
    assert_eq!(
        locks(&next),
        ["sampler.slice:'b", "sampler.slice:01", "sampler.slice:'z"]
    );
    // Slicing into sixteen keeps every old start, so every mark stays on its sound.
    let sixteen = points(&next).even_starts(16, LENGTH);
    let more = edit(
        &next,
        &Action::SetSlices {
            number: 1,
            starts: sixteen,
        },
    )
    .unwrap();
    assert_eq!(points(&more).marked('b'), Some(5));
}

#[test]
fn clearing_the_slices_drops_the_marks_and_undo_brings_them_back() {
    let (first, next) = committed(&Action::SetSlices {
        number: 1,
        starts: Vec::new(),
    });
    assert!(points(&next).marks.is_empty());
    assert_eq!(points(&first).marks.len(), 4);
    // The step keeps its letter, which names nothing now.
    assert_eq!(locks(&next)[0], "sampler.slice:'b");
}

#[test]
fn marks_list_in_the_order_of_their_slices_and_save_with_the_entry() {
    let song = song();
    let points = points(&song);
    assert_eq!(
        points.marks_in_order(),
        [('a', 1), ('b', 2), ('c', 2), ('d', 3)]
    );
    assert_eq!(points.marks_of(2), ['b', 'c']);
    let text = ron::to_string(points).unwrap();
    assert!(text.contains("marks:{'a':1,'b':2,'c':2,'d':3}"), "{text}");
    assert_eq!(&ron::from_str::<Points>(&text).unwrap(), points);
    let mut renamed = points.clone();
    assert!(renamed.set_mark('a', 3), "a letter moves to another slice");
    assert_eq!(renamed.marked('a'), Some(3));
    assert!(!renamed.set_mark('A', 0), "only a to z");
    assert!(!renamed.set_mark('e', 4), "no slice 4");
}

#[test]
fn an_sli_by_a_letter_reads_back_moves_through_the_letters_and_typing_makes_it_a_number() {
    let step: Step = "C-3 -- sampler.slice:'ah --- ---".parse().unwrap();
    assert_eq!(step.fx[0], FxSlot::slice_by_mark('a', Mark::Hold));
    assert_eq!(step.fx[0].slice_mark(), Some('a'));
    assert_eq!(step.to_string(), "C-3 -- sampler.slice:'ah --- ---");
    for bad in [
        "C-3 -- sampler.slice:'A",
        "C-3 -- sampler.slice:'ab",
        "C-3 -- sampler.slice:'ag",
        "C-3 -- cut:'a",
    ] {
        assert!(bad.parse::<Step>().is_err(), "{bad}");
    }
    let mut moved = step.clone();
    shift_step(
        &mut moved,
        Scope::column(FX_GROUP),
        2,
        Shift::Increment,
        None,
        0,
        11,
    );
    assert_eq!(moved.fx[0].slice_mark(), Some('c'));
    shift_step(
        &mut moved,
        Scope::column(FX_GROUP),
        -9,
        Shift::Increment,
        None,
        0,
        11,
    );
    assert_eq!(moved.fx[0].slice_mark(), Some('a'), "it stops at a");
    // Typing two digits over it locks by number.
    let mut song = song();
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = step;
    let song = Arc::new(song);
    let typed = edit(
        &song,
        &Action::Fill {
            track: 0,
            phrase: 0,
            range: StepRange { first: 0, last: 0 },
            fill: Fill::SlotValue(0, 3),
        },
    )
    .unwrap();
    let slot = typed.tracks[0].phrases[&0].steps()[0].fx[0];
    assert_eq!(slot.command, Some(FxCommand::Set(SettingId::SLICE)));
    assert_eq!((slot.slice_mark(), slot.value), (None, 0x0300));
}
