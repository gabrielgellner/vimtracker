//! Clouds' and Beads' modes and qualities, Distort's type and the Chorus's, locked by a step and
//! turned by a macro (decision 414): a lock is heard from its row until the track's next note, as
//! any lock is; a macro steps through the choices, a zone each; a switch while a sound plays steps
//! no harder than either choice does by itself; and a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::render::render_frames;
use vt_model::insert::{
    Beads, BeadsMode, Chorus, ChorusType, Clouds, CloudsMode, Distort, DistortType, Insert,
};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

use crate::support::{self, pool, song, version};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
const EMPTY: &str = "--- -- --- --- ---";

/// A low sine on the left and a fifth above it on the right, as long as any render here, so
/// what a switch does stands out from the sound's own steps, and the sides differ.
fn tones() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 8)
        .map(|frame| {
            let t = frame as f32 / RATE as f32;
            [
                (t * 110.0 * std::f32::consts::TAU).sin() * 0.4,
                (t * 165.0 * std::f32::consts::TAU).sin() * 0.3,
            ]
        })
        .collect();
    pool(frames)
}

/// Each setting this locks: the effect as written, the setting's key among its own, and how
/// many choices it has.
fn settings() -> [(Insert, &'static str, u8); 6] {
    [
        (Insert::Clouds(Clouds::DEFAULT), "mode", 4),
        (Insert::Clouds(Clouds::DEFAULT), "quality", 4),
        (Insert::Beads(Beads::DEFAULT), "mode", 2),
        (Insert::Beads(Beads::DEFAULT), "quality", 4),
        (distort(), "type", 8),
        (Insert::Chorus(Chorus::DEFAULT), "type", 5),
    ]
}

/// A distortion that drives hard enough for its types to differ.
fn distort() -> Insert {
    Insert::Distort(Distort {
        amount: 0x60,
        ..Distort::DEFAULT
    })
}

/// A setting's place among its effect's, by its key.
fn place(insert: Insert, key: &str) -> usize {
    insert
        .kind()
        .unwrap()
        .params()
        .iter()
        .position(|param| param.key == key)
        .unwrap()
}

/// The setting in a slot, for a step's lock or a macro.
fn id(insert: Insert, slot: usize, key: &str) -> SettingId {
    SettingId::slot(slot, insert.kind().unwrap(), place(insert, key)).unwrap()
}

/// A step locking a setting of the first slot's effect to a choice, after a note or not.
fn lock(insert: Insert, key: &str, value: u8, note: bool) -> String {
    let note = if note { "C-3" } else { "---" };
    format!("{note} -- {}:{value:02X} --- ---", id(insert, 0, key).key())
}

fn song_with(insert: Insert, texts: &[&str]) -> Song {
    let mut song = song(texts);
    song.tracks[0].inserts[0] = insert;
    song
}

/// What the first slot's setting is heard as, row by row, at the middle of each.
fn heard(song: &Song, insert: Insert, key: &str, rows: usize) -> Vec<u8> {
    let at = place(insert, key);
    let mut engine = Engine::new(RATE, version(song, tones()));
    engine.play_from(0);
    let mut played = 0;
    (0..rows)
        .map(|row| {
            let frame = row * STEP + STEP / 2;
            let mut out = vec![0.0; (frame - played) * 2];
            engine.process(&mut out);
            played = frame;
            engine.live(0).inserts[0][at]
        })
        .collect()
}

/// A lock is heard from its row until the track's next note, and the setting as written after.
#[test]
fn a_lock_switches_the_choice_on_its_row_only() {
    for (insert, key, count) in settings() {
        let value = count - 1;
        let texts = [
            &lock(insert, key, value, true)[..],
            EMPTY,
            "C-3 -- --- --- ---",
            &lock(insert, key, value, false)[..],
            EMPTY,
        ];
        let song = song_with(insert, &texts);
        let written = insert.read(place(insert, key)).unwrap();
        assert_eq!(
            heard(&song, insert, key, 5),
            [value, value, written, value, value],
            "{insert:?} {key}"
        );
    }
}

/// A lock on the first row is heard just as the effect written that way, from the first frame.
#[test]
fn a_lock_on_the_first_row_is_heard_as_the_effect_written_that_way() {
    for (insert, key, count) in settings() {
        let value = count - 1;
        let locked = song_with(insert, &[&lock(insert, key, value, true)]);
        let written = song_with(
            insert.write(place(insert, key), f32::from(value)),
            &["C-3 -- --- --- ---"],
        );
        let render = |song: &Song| render_frames(version(song, tones()), RATE, 2 * STEP);
        assert!(
            render(&locked) == render(&written),
            "{insert:?} {key} locked to {value}"
        );
    }
}

/// A macro on the setting steps through its choices in order, a zone each, as it's turned.
#[test]
fn a_macro_sweeps_through_the_choices() {
    for (insert, key, count) in settings() {
        let turns: Vec<String> = (0..16)
            .map(|row| {
                let note = if row == 0 { "C-3" } else { "---" };
                format!("{note} -- macro1:{:02X} --- ---", row * 0x11)
            })
            .collect();
        let texts: Vec<&str> = turns.iter().map(String::as_str).collect();
        let mut song = song_with(insert, &texts);
        song.tracks[0].macros[0]
            .maps
            .push(MacroMap::whole(MacroTarget::Setting(id(insert, 0, key))));
        let mut stepped = heard(&song, insert, key, 16);
        stepped.dedup();
        assert_eq!(stepped, (0..count).collect::<Vec<_>>(), "{insert:?} {key}");
    }
}

/// The largest step from one frame to the next on one side, over a stretch.
fn steepest(out: &[f32], side: usize, frames: std::ops::Range<usize>) -> f32 {
    frames
        .map(|at| (out[at * 2 + side] - out[(at - 1) * 2 + side]).abs())
        .fold(0.0, f32::max)
}

/// How much one switch steps beyond what either choice does by itself: the steepest step for a
/// step after the switch, against the steepest of the effect written as it was and as it's
/// switched to over the same stretch, and of the second as it starts, so a choice that starts a
/// new recording is held to what starting one does.
fn click(insert: Insert, key: &str, from: u8, to: u8) -> f32 {
    let at = place(insert, key);
    let from = insert.write(at, f32::from(from));
    let rendered = |insert: Insert, row: &str| {
        let texts = ["C-3 -- --- --- ---", EMPTY, EMPTY, row, EMPTY];
        render_frames(version(&song_with(insert, &texts), tones()), RATE, 5 * STEP)
    };
    let switched = rendered(from, &lock(from, key, to, false));
    let before = rendered(from, EMPTY);
    let after = rendered(from.write(at, f32::from(to)), EMPTY);
    let switch = 3 * STEP;
    let around = switch - 64..switch + STEP;
    (0..2)
        .map(|side| {
            let own = steepest(&before, side, around.clone())
                .max(steepest(&after, side, around.clone()))
                .max(steepest(&after, side, 1..STEP));
            steepest(&switched, side, around.clone()) / own
        })
        .fold(0.0, f32::max)
}

/// Each choice to the next and back to the first, as a sound plays through it. The engine fades
/// what a switch cuts, a mode's filters and pitch shifter started afresh, a new recording, a new
/// type's delay or shaper, so no switch steps more than twice as hard as either choice does by
/// itself. Before the fades, a switch into Clouds' `Looping` stepped 17 times as hard, and one of
/// Beads' qualities 7.
fn each_switch_is_clean(insert: Insert, key: &str, count: u8) {
    for from in 0..count {
        for to in [(from + 1) % count, 0] {
            if to == from {
                continue;
            }
            let click = click(insert, key, from, to);
            assert!(click < 2.0, "{insert:?} {key} {from} to {to}: {click}");
        }
    }
}

#[test]
fn clouds_switches_mode_and_quality_cleanly() {
    each_switch_is_clean(Insert::Clouds(Clouds::DEFAULT), "mode", 4);
    each_switch_is_clean(Insert::Clouds(Clouds::DEFAULT), "quality", 4);
}

#[test]
fn beads_switches_mode_and_quality_cleanly() {
    each_switch_is_clean(Insert::Beads(Beads::DEFAULT), "mode", 2);
    each_switch_is_clean(Insert::Beads(Beads::DEFAULT), "quality", 4);
}

#[test]
fn distort_switches_type_cleanly() {
    each_switch_is_clean(distort(), "type", 8);
}

#[test]
fn the_chorus_switches_type_cleanly() {
    each_switch_is_clean(Insert::Chorus(Chorus::DEFAULT), "type", 5);
}

/// A render whose locks switch every row, with Clouds' grains and Beads' drawn at random,
/// repeats.
#[test]
fn renders_with_locked_types_repeat() {
    let clouds = Insert::Clouds(Clouds {
        mode: CloudsMode::Granular,
        ..Clouds::DEFAULT
    });
    let beads = Insert::Beads(Beads {
        mode: BeadsMode::Granular,
        ..Beads::DEFAULT
    });
    let distort = Insert::Distort(Distort {
        kind: DistortType::Fold,
        ..Distort::DEFAULT
    });
    let chorus = Insert::Chorus(Chorus {
        kind: ChorusType::JunoI,
        ..Chorus::DEFAULT
    });
    let one = |insert: Insert, slot: usize, key: &str, value: u8| {
        format!("{}:{value:02X}", id(insert, slot, key).key())
    };
    let rows: Vec<String> = (0..8u8)
        .map(|row| {
            let note = if row % 4 == 0 { "C-3" } else { "---" };
            let [first, second, third] = if row % 2 == 0 {
                [
                    one(clouds, 0, "mode", row % 4),
                    one(beads, 1, "quality", (row / 2) % 4),
                    one(distort, 2, "type", row % 8),
                ]
            } else {
                [
                    one(clouds, 0, "quality", row % 4),
                    one(beads, 1, "mode", 1),
                    one(chorus, 3, "type", row % 5),
                ]
            };
            format!("{note} -- {first} {second} {third}")
        })
        .collect();
    let texts: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut song = song(&texts);
    song.tracks[0].inserts = [clouds, beads, distort, chorus];
    support::repeats("locked types", || {
        render_frames(version(&song, tones()), RATE, 8 * STEP)
    });
}
