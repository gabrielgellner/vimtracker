//! `SLD`, the Analog Four's parameter slide (decision 417): a row's locks of continuous settings,
//! and the next note's, slide over the time to where each next changes, rather than jumping.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::render::render_frames;
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};

use crate::support::steady::version;
use crate::support::{peak, repeats};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

const EMPTY: &str = "--- -- --- --- ---";

/// One track on the steady sample, its `Cut` at `base`, playing these rows of a phrase of eight.
fn song(rows: &[&str], base: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.cutoff = base;
    let mut steps: Vec<_> = rows
        .iter()
        .map(|text| if text.is_empty() { EMPTY } else { text })
        .map(|text| text.parse().unwrap())
        .collect();
    steps.resize(8, EMPTY.parse().unwrap());
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The cutoff heard every half step, from a half step in, for these many steps.
fn cutoffs(song: &Song, steps: usize) -> Vec<u8> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    // Stereo, so half a step of frames.
    let mut out = vec![0.0; STEP];
    (0..steps * 2)
        .map(|_| {
            engine.process(&mut out);
            engine.live(0).cutoff
        })
        .collect()
}

/// The cutoff `step` steps in, and a half step more with `half`, from what [`cutoffs`] gives.
fn at(heard: &[u8], step: usize, half: bool) -> u8 {
    heard[step * 2 + usize::from(half) - 1]
}

#[track_caller]
fn near(actual: u8, expected: u8) {
    assert!(
        actual.abs_diff(expected) <= 2,
        "{actual:02X} should be about {expected:02X}"
    );
}

#[test]
fn a_slid_cutoff_is_between_the_two_values_mid_row_and_arrives_at_the_next_note() {
    let heard = cutoffs(
        &song(
            &[
                "C-3 -- cut:40 sld:01 ---",
                "",
                "",
                "",
                "C-3 -- cut:C0 --- ---",
            ],
            0xFF,
        ),
        6,
    );
    // Four steps from 40 to C0, so 20 a step, heard a half step after each.
    near(at(&heard, 0, true), 0x50);
    near(at(&heard, 1, true), 0x70);
    near(at(&heard, 2, false), 0x80);
    let middle = at(&heard, 2, false);
    assert!(middle > 0x40 && middle < 0xC0, "{middle:02X}");
    near(at(&heard, 3, true), 0xB0);
    assert_eq!(at(&heard, 4, true), 0xC0);
    assert_eq!(at(&heard, 5, true), 0xC0, "the next note's lock holds");
}

#[test]
fn a_row_without_a_slide_jumps() {
    let rows = ["C-3 -- cut:40 --- ---", "", "", "", "C-3 -- cut:C0 --- ---"];
    let heard = cutoffs(&song(&rows, 0xFF), 5);
    for step in 0..4 {
        assert_eq!(at(&heard, step, true), 0x40, "step {step}");
    }
    assert_eq!(at(&heard, 4, true), 0xC0);
    // `SLD 00` is no slide, bit for bit.
    let render = |song: &Song| render_frames(version(song), RATE, STEP * 8);
    let off = [
        "C-3 -- cut:40 sld:00 ---",
        "",
        "",
        "",
        "C-3 -- cut:C0 --- ---",
    ];
    assert_eq!(render(&song(&rows, 0xFF)), render(&song(&off, 0xFF)));
}

#[test]
fn a_slide_to_a_note_with_no_lock_goes_back_to_the_tracks_own_value() {
    let heard = cutoffs(
        &song(
            &["C-3 -- cut:40 sld:01 ---", "", "", "", "C-3 -- --- --- ---"],
            0xC0,
        ),
        6,
    );
    near(at(&heard, 2, false), 0x80);
    assert_eq!(at(&heard, 4, true), 0xC0);
    assert_eq!(at(&heard, 5, true), 0xC0);
}

#[test]
fn a_value_the_next_note_locks_slides_in_from_where_it_is() {
    let heard = cutoffs(
        &song(
            &["C-3 -- sld:01 --- ---", "", "", "", "C-3 -- cut:C0"],
            0x40,
        ),
        5,
    );
    near(at(&heard, 2, false), 0x80);
    assert_eq!(at(&heard, 4, true), 0xC0);
}

#[test]
fn a_slide_arrives_where_a_row_with_no_note_locks_it_first() {
    let rows = [
        "C-3 -- cut:40 sld:01 ---",
        "",
        "--- -- cut:C0",
        "",
        "C-3 -- --- --- ---",
    ];
    let heard = cutoffs(&song(&rows, 0xFF), 5);
    near(at(&heard, 1, false), 0x80);
    assert_eq!(at(&heard, 2, true), 0xC0);
    assert_eq!(
        at(&heard, 3, true),
        0xC0,
        "the row's own lock holds, unslid"
    );
    assert_eq!(at(&heard, 4, true), 0xFF, "and the note ends it");
}

#[test]
fn lists_jump_under_a_slide() {
    // The filter's type is a list: slid, the song plays as it would without the slide.
    let render = |song: &Song| render_frames(version(song), RATE, STEP * 8);
    let slid = [
        "C-3 -- type:04 sld:01 ---",
        "",
        "",
        "",
        "C-3 -- type:01 ---",
    ];
    let jumped = ["C-3 -- type:04 --- ---", "", "", "", "C-3 -- type:01 ---"];
    assert_eq!(render(&song(&slid, 0x80)), render(&song(&jumped, 0x80)));
}

#[test]
fn a_thinned_row_still_slides_as_its_locks_still_run() {
    let rows = [
        "C-3 -- cut:40 sld:01 chancedelta:00",
        "",
        "",
        "",
        "C-3 -- cut:C0 --- ---",
    ];
    let thinned = song(&rows, 0xFF);
    let out = render_frames(version(&thinned), RATE, STEP * 4);
    assert!(peak(&out) < 1e-6, "the row's note is thinned out");
    let heard = cutoffs(&thinned, 5);
    near(at(&heard, 2, false), 0x80);
    assert_eq!(at(&heard, 4, true), 0xC0);
}

#[test]
fn a_slid_macro_lock_moves_what_it_holds_along_with_it() {
    let mut song = song(
        &[
            "C-3 -- macro1:00 sld:01 ---",
            "",
            "",
            "",
            "C-3 -- macro1:FF",
        ],
        0xFF,
    );
    song.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::CUT),
        min: 0x3000,
        max: 0xA000,
        curve: 0,
        s: 0,
        zone: Default::default(),
        member: None,
    }];
    let heard = cutoffs(&song, 5);
    near(at(&heard, 0, true), 0x3E);
    near(at(&heard, 2, false), 0x68);
    near(at(&heard, 4, true), 0xA0);
}

#[test]
fn with_cue_on_a_window_turned_under_a_slide_waits_so_the_slide_lands_where_it_was_aimed() {
    // The window turned to four rows on the third, under a slide to the fifth.
    let rows = [
        "C-3 -- cut:40 sld:01 ---",
        "",
        "--- -- phraselength:7C",
        "",
        "C-3 -- cut:C0 --- ---",
    ];
    let mut cued = song(&rows, 0xFF);
    cued.tracks[0].cue = 1;
    let heard = cutoffs(&cued, 5);
    near(at(&heard, 3, false), 0xA0);
    assert_eq!(at(&heard, 4, true), 0xC0);
}

#[test]
fn a_slide_renders_the_same_each_time() {
    let rows = [
        "C-3 -- cut:40 sld:01 vol:A0",
        "",
        "C-3 -- sld:01 cha:80 ---",
        "",
        "C-3 -- cut:C0 vol:60 ---",
    ];
    let song = song(&rows, 0xFF);
    repeats("a slide", || render_frames(version(&song), RATE, STEP * 16));
}
