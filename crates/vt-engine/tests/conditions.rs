//! Trig conditions, after Elektron's: a `CND` plays its row only on some passes, while fill is on
//! or off, or as the last condition on this track or the one to its left went. A pass is each time
//! the track plays the row since its chain started, so it counts the track's own steps at its own
//! speed, and starts again wherever the chain does.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Lane, Phrase, Song};
use vt_model::speed::Speed;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// A short hit, so each note's start shows.
fn hits_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; 10],
        },
    );
    Arc::new(pool)
}

/// A long flat sample, so a note's level shows for as long as it lasts.
fn flat_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[0.5, 0.5]; RATE as usize * 8],
        },
    );
    Arc::new(pool)
}

/// Tracks each playing these rows, panned apart when there are two so each channel is one
/// track's.
fn song(tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.mods[0].adsr.attack_ms = 0;
        track.mods[0].adsr.release_ms = 0;
        track.pan = match (tracks.len(), index) {
            (1, _) => 0x80,
            (_, 0) => 0x00,
            _ => 0xFF,
        };
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

fn version(song: &Song, pool: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool))
}

/// Which steps a note starts on, in one channel, over this many steps.
fn hits(song: &Song, steps: usize, channel: usize) -> Vec<usize> {
    let out = render_frames(version(song, hits_pool()), RATE, steps * STEP);
    (0..steps)
        .filter(|step| out[step * STEP * 2 + channel].abs() > 0.001)
        .collect()
}

/// The steps of a four-row phrase's first row, for this many passes, as a pattern of `x` for a
/// note and `.` for none.
fn passes(song: &Song, count: usize) -> String {
    let heard = hits(song, count * 4, 0);
    (0..count)
        .map(|pass| {
            if heard.contains(&(pass * 4)) {
                'x'
            } else {
                '.'
            }
        })
        .collect()
}

/// A four-row phrase with this on its first row's note.
fn first_row(fx: &str) -> Song {
    let row = format!("C-3 -- {fx}");
    song(&[&[&row, "--- --", "--- --", "--- --"]])
}

#[test]
fn a_pass_of_a_cycle_plays_once_in_each() {
    for (value, pattern) in [
        ("12", "x.x.x.x."),
        ("22", ".x.x.x.x"),
        ("13", "x..x..x."),
        ("33", "..x..x.."),
        ("14", "x...x..."),
        ("24", ".x...x.."),
        ("34", "..x...x."),
        ("44", "...x...x"),
        ("88", ".......x"),
    ] {
        let song = first_row(&format!("cnd:{value}"));
        assert_eq!(passes(&song, 8), pattern, "{value}");
    }
}

#[test]
fn first_plays_only_the_first_time_round_and_not_first_after() {
    assert_eq!(passes(&first_row("cnd:07"), 6), "x.....");
    assert_eq!(passes(&first_row("cnd:08"), 6), ".xxxxx");
}

#[test]
fn a_value_that_isnt_a_condition_plays_as_none() {
    assert_eq!(passes(&first_row("cnd:09"), 4), "xxxx");
    assert_eq!(passes(&first_row("cnd:21"), 4), "xxxx");
}

#[test]
fn a_phrase_played_twice_in_its_chain_counts_each_time() {
    let mut twice = first_row("cnd:12");
    twice.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![
                ChainEntry {
                    phrase: 0,
                    transpose: 0,
                },
                ChainEntry {
                    phrase: 0,
                    transpose: 0,
                },
            ],
            label: String::new(),
        }),
    );
    assert_eq!(passes(&twice, 8), "x.x.x.x.");
    // With another phrase between, each time round the chain is one pass of this one's.
    let mut between = twice.clone();
    between.tracks[0].phrases.insert(
        1,
        Arc::new(Phrase::from_steps(vec!["--- --".parse().unwrap(); 4])),
    );
    Arc::make_mut(between.tracks[0].chains.get_mut(&0).unwrap()).entries[1].phrase = 1;
    assert_eq!(passes(&between, 8), "x...x...");
}

#[test]
fn a_track_at_another_speed_counts_its_own_passes() {
    // At an eighth, the four rows last eight sixteenths, so each pass is two of the song's bars
    // of four.
    let mut slow = first_row("cnd:12");
    slow.tracks[0].speed = Speed::Eighth;
    assert_eq!(hits(&slow, 32, 0), [0, 16]);
    let mut fast = first_row("cnd:22");
    fast.tracks[0].speed = Speed::ThirtySecond;
    let out = render_frames(version(&fast, hits_pool()), RATE, 8 * STEP);
    // In 32nds, each pass is four of them, two of the song's steps.
    let thirty_second = STEP / 2;
    let starts: Vec<usize> = (0..16)
        .filter(|at| out[at * thirty_second * 2].abs() > 0.001)
        .collect();
    assert_eq!(starts, [4, 12]);
}

#[test]
fn tracks_count_passes_of_their_own_lengths() {
    // A phrase of two rows against one of four: each counts its own.
    let two = ["C-3 -- cnd:12", "--- --"];
    let four = ["C-3 -- cnd:12", "--- --", "--- --", "--- --"];
    let both = song(&[&two, &four]);
    assert_eq!(hits(&both, 16, 0), [0, 4, 8, 12]);
    assert_eq!(hits(&both, 16, 1), [0, 8]);
}

#[test]
fn a_reset_starts_the_passes_again() {
    let mut reset = first_row("cnd:12");
    reset.sections[0].reset = Some(12);
    // Passes at 0 and 8, then the reset at 12 starts it at its first again.
    assert_eq!(hits(&reset, 24, 0), [0, 8, 12, 20]);
    let mut first = first_row("cnd:07");
    first.sections[0].reset = Some(8);
    assert_eq!(hits(&first, 24, 0), [0, 8, 16]);
}

#[test]
fn realigning_starts_the_passes_again() {
    let song = first_row("cnd:07");
    let mut engine = Engine::new(RATE, version(&song, hits_pool()));
    engine.play_from(0);
    let mut out = vec![0.0; 10 * STEP * 2];
    engine.process(&mut out);
    let heard = |out: &[f32], step: usize| out[step * STEP * 2].abs() > 0.001;
    assert!(heard(&out, 0) && !heard(&out, 4) && !heard(&out, 8));
    // From the next step, the track is at its chain's start, and its first pass.
    engine.realign();
    let mut after = vec![0.0; 2 * STEP * 2];
    engine.process(&mut after);
    assert!(heard(&after, 0) || heard(&after, 1), "1ST plays again");
}

#[test]
fn playing_from_a_step_counts_the_passes_before_it() {
    // A longer track makes the section 16 steps long, so there's a step 4 to play from.
    let rows = ["C-3 -- cnd:12", "--- --", "--- --", "--- --"];
    let song = song(&[&rows, &["--- --"; 16]]);
    let mut engine = Engine::new(RATE, version(&song, hits_pool()));
    // Step 4 is the second pass, so it's silent, and step 8 the third.
    engine.play_from(4);
    let mut out = vec![0.0; 8 * STEP * 2];
    engine.process(&mut out);
    assert!(out[0].abs() < 0.001, "the second pass is skipped");
    assert!(out[4 * STEP * 2].abs() > 0.001, "the third plays");
}

#[test]
fn previous_follows_the_last_condition_and_leaves_it_alone() {
    // `PRE` plays with the 1:2 and `!PRE` without, as `PRE` itself isn't counted.
    let rows = ["C-3 -- cnd:12", "C-3 -- cnd:03", "C-3 -- cnd:04", "--- --"];
    let heard = hits(&song(&[&rows]), 16, 0);
    assert_eq!(heard, [0, 1, 6, 8, 9, 14]);
}

#[test]
fn previous_follows_a_chance() {
    let rows = vec!["C-3 -- cha:80", "C-3 -- cnd:03", "C-3 -- cnd:04", "--- --"];
    let heard = hits(&song(&[&rows]), 64, 0);
    let mut played = 0;
    for pass in 0..16 {
        let chance = heard.contains(&(pass * 4));
        played += usize::from(chance);
        assert_eq!(heard.contains(&(pass * 4 + 1)), chance, "pass {pass}");
        assert_eq!(heard.contains(&(pass * 4 + 2)), !chance, "pass {pass}");
    }
    assert!(played > 0 && played < 16, "the chance went both ways");
}

#[test]
fn previous_starts_from_nothing_passed() {
    let heard = hits(&song(&[&["C-3 -- cnd:04", "C-3 -- cnd:03"]]), 2, 0);
    assert_eq!(heard, [0]);
}

#[test]
fn neighbour_follows_the_track_to_the_left() {
    let left = ["C-3 -- cnd:13", "--- --"];
    let right = ["C-3 -- cnd:05", "C-3 -- cnd:06"];
    let both = song(&[&left, &right]);
    assert_eq!(hits(&both, 12, 0), [0, 6]);
    assert_eq!(hits(&both, 12, 1), [0, 3, 5, 6, 9, 11]);
    // The first track has none to its left, so its `NEI` never plays.
    let alone = song(&[&["C-3 -- cnd:05", "C-3 -- cnd:06"]]);
    assert_eq!(hits(&alone, 4, 0), [1, 3]);
}

#[test]
fn fill_plays_its_rows_only_while_on() {
    let rows = ["C-3 -- cnd:01", "C-3 -- cnd:02"];
    let song = song(&[&rows]);
    let mut engine = Engine::new(RATE, version(&song, hits_pool()));
    engine.play_from(0);
    let play = |engine: &mut Engine| {
        let mut out = vec![0.0; 4 * STEP * 2];
        engine.process(&mut out);
        (0..4)
            .filter(|step| out[step * STEP * 2].abs() > 0.001)
            .collect::<Vec<_>>()
    };
    assert_eq!(play(&mut engine), [1, 3], "!FILL plays");
    engine.set_fill(true);
    assert!(engine.fill());
    assert_eq!(play(&mut engine), [0, 2], "FILL plays");
    engine.set_fill(false);
    assert_eq!(play(&mut engine), [1, 3], "let go, it's back");
}

#[test]
fn a_failed_condition_skips_the_rows_locks_and_events_but_not_its_lanes() {
    let level = |song: &Song, step: usize| {
        let out = render_frames(version(song, flat_pool()), RATE, (step + 1) * STEP);
        out[(step * STEP + STEP / 2) * 2].abs()
    };
    // A row with no note: its `VOL` locks only on the passes its condition holds.
    let rows = ["C-3 -- ---", "--- -- cnd:22 vol:00"];
    let locks = song(&[&rows]);
    assert!(level(&locks, 1) > 0.1, "the first pass skips the lock");
    assert!(level(&locks, 3) < 0.001, "the second locks it");
    // `KIL` is skipped with it.
    let kills = song(&[&["C-3 -- ---", "--- -- cnd:22 kil:00"]]);
    assert!(level(&kills, 1) > 0.1);
    assert!(level(&kills, 3) < 0.001);
    // A failed `CHA` skips only the note, as it always has.
    let chance = song(&[&["C-3 -- ---", "--- -- cha:00 vol:00"]]);
    assert!(level(&chance, 1) < 0.001);
    // A column with a lane of its own keeps its own loop.
    let mut lane = locks.clone();
    let phrase = Arc::make_mut(lane.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        1,
        Some(Lane {
            start: 0,
            length: 2,
        }),
    );
    assert!(
        level(&lane, 1) < 0.001,
        "the lane locks it on a skipped row"
    );
}

#[test]
fn a_condition_in_a_lane_counts_the_lanes_own_loops() {
    // A lane of one step goes round on every step, so its 1:2 holds on every other one.
    let mut lane = song(&[&["C-3 -- cnd:12"; 4]]);
    let phrase = Arc::make_mut(lane.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 1,
        }),
    );
    assert_eq!(hits(&lane, 8, 0), [0, 2, 4, 6]);
}

#[test]
fn a_chance_rolls_the_same_whatever_the_conditions() {
    let mut alone = song(&[&["C-3 -- cha:80"]]);
    alone.seed = 5;
    let mut gated = song(&[&["C-3 -- cha:80 cnd:12"]]);
    gated.seed = 5;
    let rolled = hits(&alone, 32, 0);
    let expected: Vec<usize> = rolled.iter().copied().filter(|s| s % 2 == 0).collect();
    assert!(!expected.is_empty() && expected.len() < 16);
    assert_eq!(hits(&gated, 32, 0), expected);
}

#[test]
fn renders_with_conditions_repeat() {
    let rows = [
        "C-3 -- cha:80 cnd:12",
        "C-3 -- cnd:03",
        "C-3 -- cnd:24",
        "C-3 -- cnd:08",
    ];
    let song = song(&[&rows, &["C-3 -- cnd:05", "C-3 -- cnd:06"]]);
    let render = || render_frames(version(&song, hits_pool()), RATE, 32 * STEP);
    assert_eq!(render(), render());
}
