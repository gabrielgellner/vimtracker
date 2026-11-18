//! A mod slot's lists, its AHD's hold, its drum's peak and ratchet and its LFO's fade, locked by a
//! step, turned by a macro and moved by another slot (decision 399): a lock is heard from the row
//! it's on until the track's next note, as any lock is, and is heard as the slot written that way;
//! a wave or a mode swapped keeps where the slot is, its phase or its count, and glides what it adds
//! from where it was, so a swap doesn't click; and a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::{Ahd, Curve, Drum, Hold};
use vt_model::lfo::{Fade, Lfo, LfoMode, Rate, Shape, wave};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::{ModField, ModKind, ModSlot};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_model::steps::{Loop, StepRate, Steps, StepsMode};

use crate::support::steady::flat;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 16 to the bar.
const STEP: usize = 6000;
const BAR: usize = 16 * STEP;
/// A whole turn of an LFO's reported phase.
const TURN: i64 = 1 << 16;

/// One track of a sample playing these steps, held at `CUT 80`, an empty text a step left empty.
fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.cutoff = 0x80;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (index, text) in texts
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn version(song: &Song, pool: &Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(pool)))
}

/// A slot's own setting of its type that a step locks, by its field.
fn own(slot: usize, kind: ModKind, field: ModField) -> SettingId {
    SettingId::mod_own(slot, field.own(kind).unwrap()).unwrap()
}

/// An LFO on `Cut` in Mod 4, a bar a cycle unless said.
fn lfo(shape: Shape, rate: Rate) -> ModSlot {
    ModSlot::lfo(
        Some(SettingId::CUT),
        Lfo {
            shape,
            rate,
            ..Lfo::default()
        },
    )
    .with_depth(0x40)
}

/// An engine playing a song from the top, and how many frames it has played.
struct Playing {
    engine: Engine,
    played: usize,
}

impl Playing {
    fn new(song: &Song) -> Self {
        let mut engine = Engine::new(RATE, version(song, &flat()));
        engine.play_from(0);
        Self { engine, played: 0 }
    }

    /// Play on to a frame.
    fn to(&mut self, frame: usize) {
        let mut out = vec![0.0; (frame - self.played) * 2];
        self.engine.process(&mut out);
        self.played = frame;
    }

    /// Where Mod 4 is in its wave at a frame, and its value there from -1 to 1.
    fn lfo_at(&mut self, frame: usize) -> (i64, f32) {
        self.to(frame + 1);
        let now = self.engine.live(0).mods[3].expect("the LFO runs");
        (
            i64::from(now.phase),
            f32::from(now.value) / f32::from(i16::MAX),
        )
    }

    /// The cutoff heard at a frame.
    fn cut_at(&mut self, frame: usize) -> u8 {
        self.to(frame + 1);
        self.engine.live(0).cutoff
    }

    /// The step Mod 3 plays at a frame.
    fn step_at(&mut self, frame: usize) -> u8 {
        self.to(frame + 1);
        self.engine.live(0).steps[2].expect("Steps runs")
    }
}

/// How far apart two places in a turn are, the short way round.
fn apart(a: i64, b: i64) -> i64 {
    let gap = (a - b).rem_euclid(TURN);
    gap.min(TURN - gap)
}

/// A lock of an LFO's `Shape` swaps its wave where it is in its cycle: the phase runs on as it
/// would have, the value is the new wave's there, and the track's next note ends the lock.
#[test]
fn a_shape_lock_swaps_the_wave_and_keeps_the_phase() {
    let texts = [
        "C-3 -- --- --- ---",
        "",
        "",
        "--- -- mod4.shape:04 --- ---",
        "",
        "",
        "",
        "",
        "C-3 -- --- --- ---",
    ];
    let mut locked = song(&texts);
    locked.tracks[0].mods[3] = lfo(Shape::Ramp, Rate::Note(3840));
    let mut plain = locked.clone();
    *Arc::make_mut(plain.tracks[0].phrases.get_mut(&0).unwrap())
        .step_mut(3)
        .unwrap() = "--- -- --- --- ---".parse().unwrap();
    let (mut locked, mut plain) = (Playing::new(&locked), Playing::new(&plain));
    for at in 0..12 * 8 {
        let frame = at * STEP / 8;
        let ((phase, value), (was, ramp)) = (locked.lfo_at(frame), plain.lfo_at(frame));
        assert!(apart(phase, was) <= 1, "{frame}: {phase} against {was}");
        let cycles = phase as f64 / TURN as f64;
        let square = wave(Shape::Square, cycles, 0);
        match frame {
            // The rows themselves, which the update before them may not have heard yet.
            frame if frame == 3 * STEP || frame == 8 * STEP => {}
            // From the lock's row to the next note's, it's a square at the ramp's phase.
            frame if (3 * STEP..8 * STEP).contains(&frame) => {
                assert!((value - square).abs() < 1e-3, "{frame}: {value} {square}");
            }
            _ => assert!((value - ramp).abs() < 1e-4, "{frame}: {value} {ramp}"),
        }
    }
}

/// A lock on the first row is heard just as the slot written that way, each of the lists, the
/// hold, the peak, the fade and the ratchet, and differs from the slot unlocked.
#[test]
fn a_lock_on_each_is_heard_as_the_slot_written_that_way() {
    let ahd = ModSlot::ahd(
        Some(SettingId::CUT),
        Ahd {
            attack_ms: 0,
            hold: Hold::Ms(0),
            decay_ms: 400,
            ..Ahd::default()
        },
    )
    .with_depth(0x40);
    let drum = ModSlot::drum(
        Some(SettingId::CUT),
        Drum {
            peak_ms: 40,
            body: 20,
            decay_ms: 4000,
            ..Drum::default()
        },
    )
    .with_depth(0x60);
    let free_lfo = lfo(Shape::Square, Rate::Note(960));
    let steps = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values: [
                0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            length: 6,
            mode: StepsMode::Free,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
    let written = |slot: ModSlot, set: fn(&mut ModSlot)| {
        let mut slot = slot;
        set(&mut slot);
        slot
    };
    // Each: the slot in Mod 4, the setting, the value locked, and the slot written with it.
    let cases: Vec<(&str, ModSlot, ModField, u8, ModSlot)> = vec![
        ("Shape", free_lfo, ModField::Shape, 0x00, {
            written(free_lfo, |slot| slot.lfo.shape = Shape::Sine)
        }),
        ("LFO mode", free_lfo, ModField::Mode, 0x02, {
            written(free_lfo, |slot| slot.lfo.mode = LfoMode::Hold)
        }),
        ("Fade", free_lfo, ModField::Fade, 0x28, {
            written(free_lfo, |slot| slot.lfo.fade = Fade::ALL[0x28])
        }),
        ("Loop", steps, ModField::Loop, 0x02, {
            written(steps, |slot| slot.steps.looping = Loop::Bounce)
        }),
        ("From", steps, ModField::From, 0x03, {
            written(steps, |slot| slot.steps.from = 3)
        }),
        ("Steps mode", steps, ModField::Mode, 0x02, {
            written(steps, |slot| slot.steps.mode = StepsMode::Note)
        }),
        ("Curve", ahd, ModField::Mode, 0x01, {
            written(ahd, |slot| slot.ahd.curve = Curve::Lin)
        }),
        ("Hold", ahd, ModField::Hold, 0xFF, {
            written(ahd, |slot| slot.ahd.hold = Hold::Note)
        }),
        // `80` on the envelopes' time scale is 89 ms.
        ("Peak", drum, ModField::Peak, 0x80, {
            written(drum, |slot| slot.drum.peak_ms = 89)
        }),
        ("Ratchet", drum, ModField::Ratchet, 0x03, {
            written(drum, |slot| slot.drum.ratchet = 4)
        }),
    ];
    for (name, slot, field, value, as_written) in cases {
        let id = own(3, slot.kind, field);
        let lock = format!("C-3 -- {}:{value:02X} --- ---", id.key());
        let texts = [lock.as_str(), "", "", "", "", "", "", "", "", "", "", ""];
        let mut locked = song(&texts);
        locked.tracks[0].mods[3] = slot;
        let mut unlocked = song(&["C-3 -- --- --- ---"]);
        unlocked.tracks[0].mods[3] = slot;
        let mut wrote = unlocked.clone();
        wrote.tracks[0].mods[3] = as_written;
        let mut playing = [&locked, &wrote, &unlocked].map(Playing::new);
        let mut differs = false;
        for at in 1..10 * 8 {
            let frame = at * STEP / 8;
            let [heard, meant, plain] = playing.each_mut().map(|playing| playing.cut_at(frame));
            assert!(
                heard.abs_diff(meant) <= 1,
                "{name} at {frame}: {heard:02X} locked, {meant:02X} written"
            );
            differs |= heard.abs_diff(plain) > 2;
        }
        assert!(differs, "{name}'s lock moves nothing");
    }
}

/// A Steps slot's mode locked to `Trig` on a note's row starts it again from there, and back in
/// `Free` it runs on from the step it's on rather than jumping to where the song is; a lock of its
/// `Loop` takes the count where it is on through the new end.
#[test]
fn a_steps_mode_or_loop_lock_keeps_the_count_it_is_on() {
    let free = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values: [0x10; 16],
            length: 8,
            mode: StepsMode::Free,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
    let song_of = |texts: &[&str]| {
        let mut song = song(texts);
        song.tracks[0].mods[2] = free;
        song
    };
    let played = |song: &Song, steps: usize| {
        let mut playing = Playing::new(song);
        (0..steps)
            .map(|step| playing.step_at(step * STEP + STEP / 2))
            .collect::<Vec<_>>()
    };
    let note = "C-3 -- --- --- ---";
    let trig = "C-3 -- mod3.stepsmode:00 --- ---";
    let plain = played(&song_of(&[note, "", "", "", note, "", "", "", note]), 12);
    assert_eq!(plain, [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3], "free runs on");
    let locked = played(&song_of(&[note, "", "", "", trig, "", "", "", note]), 12);
    assert_eq!(
        locked,
        [0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 6, 7],
        "Trig starts it again at its note, and Free runs on from there"
    );
    // A `Loop` of `Bounce` from the eleventh step turns back from the count it's on.
    let bounce = "--- -- mod3.loop:02 --- ---";
    let mut texts = [""; 16];
    texts[0] = note;
    texts[10] = bounce;
    let bounced = played(&song_of(&texts), 16);
    assert_eq!(
        bounced,
        [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 4, 3, 2, 1, 0, 1],
        "count 10 on a bounce of eight is the fifth step"
    );
}

/// A lock of an LFO's mode to `Trig` on a note's row starts its cycle there; the next note, with no
/// lock, leaves it running free from there.
#[test]
fn an_lfo_mode_lock_restarts_the_cycle_at_its_note() {
    let note = "C-3 -- --- --- ---";
    let mut song = song(&[
        note,
        "",
        "",
        "",
        "",
        "C-3 -- mod4.lfomode:01 --- ---",
        "",
        "",
        note,
    ]);
    song.tracks[0].mods[3] = lfo(Shape::Ramp, Rate::Note(3840));
    let mut playing = Playing::new(&song);
    let (before, _) = playing.lfo_at(5 * STEP - 1);
    assert!(apart(before, 5 * TURN / 16) < 300, "{before}");
    let (restarted, _) = playing.lfo_at(5 * STEP + 64);
    assert!(apart(restarted, 0) < 300, "{restarted}");
    let (on, _) = playing.lfo_at(9 * STEP);
    assert!(apart(on, 4 * TURN / 16) < 300, "runs on from there: {on}");
}

/// A macro turns an LFO's `Shape` through its list, as it turns any list.
#[test]
fn a_macro_turns_an_lfos_shape() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].mods[3] = lfo(Shape::Ramp, Rate::Note(3840));
    song.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(own(3, ModKind::Lfo, ModField::Shape)),
        min: 0x0000,
        max: 0x0800,
        curve: 0,
        s: 0,
        zone: Default::default(),
        member: None,
    }];
    for (turn, shape) in [(0x0000, Shape::Sine), (0x8000, Shape::Square)] {
        song.tracks[0].macros[0].value = turn;
        let mut playing = Playing::new(&song);
        for at in 1..32 {
            let frame = at * BAR / 32;
            let (phase, value) = playing.lfo_at(frame);
            let meant = wave(shape, phase as f64 / TURN as f64, 0);
            assert!(
                (value - meant).abs() < 1e-3,
                "{shape:?} {frame}: {value} {meant}"
            );
        }
    }
}

/// An LFO in Mod 2 moves Mod 3's `Loop` on it: a square a bar long, its top turning the loop up
/// one place from `Loop` to `Bounce`, and its bottom down to `Once`.
#[test]
fn an_lfo_moves_another_slots_loop() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    let track = &mut song.tracks[0];
    track.mods[1] = ModSlot::lfo(
        Some(own(2, ModKind::Steps, ModField::Loop)),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Note(3840),
            ..Lfo::default()
        },
    )
    .with_depth(0x01);
    track.mods[2] = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values: [0x10; 16],
            length: 4,
            mode: StepsMode::Free,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
    let mut playing = Playing::new(&song);
    let played: Vec<u8> = (0..32)
        .map(|step| playing.step_at(step * STEP + STEP / 2))
        .collect();
    let (top, bottom) = ([0, 1, 2, 3, 2, 1, 0, 1], [3; 8]);
    assert_eq!(played[..8], top, "{played:?}");
    assert_eq!(played[8..16], bottom, "{played:?}");
    assert_eq!(played[16..24], [2, 1, 0, 1, 2, 3, 2, 1], "{played:?}");
    assert_eq!(played[24..], bottom, "{played:?}");
}

/// The largest second difference of a render over some frames, where a click stands out.
fn sharpest(out: &[f32], from: usize, to: usize) -> f32 {
    (from.max(2)..to)
        .map(|at| (out[at] - 2.0 * out[at - 1] + out[at - 2]).abs())
        .fold(0.0, f32::max)
}

/// A lock that swaps an LFO on `Vol` from a ramp at its bottom to a square at its top glides what
/// it adds there, as a change of type does: gentler than the square's own edge, which the slow
/// smoothers round off, and fifty times gentler than a step from one frame to the next; and a
/// render with locks repeats.
#[test]
fn a_shape_swap_doesnt_click_and_a_render_repeats() {
    // The ramp is at -0.75 an eighth of the way round, where the square is at its top.
    // One note held through every row, the third's locking the square.
    let mut texts = ["C-3 -- HLD1 --- ---"; 16];
    texts[2] = "C-3 -- HLD1 mod4.shape:04 ---";
    let mut song = song(&texts);
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.mods[3] = ModSlot::lfo(
        Some(SettingId::VOL),
        Lfo {
            shape: Shape::Ramp,
            rate: Rate::Note(3840),
            ..Lfo::default()
        },
    )
    .with_depth(0x60);
    let left = |song: &Song| -> Vec<f32> {
        render_frames(version(song, &flat()), RATE, 10 * STEP)
            .chunks(2)
            .map(|frame| frame[0])
            .collect()
    };
    let out = left(&song);
    assert_eq!(out, left(&song), "a render with locks repeats");
    let swap = sharpest(&out, 2 * STEP - 64, 2 * STEP + 2048);
    let running = sharpest(&out, STEP, 2 * STEP - 256);
    // How far the level moves at the swap, which a step from one frame to the next would have as
    // its second difference.
    let jump = (out[2 * STEP + 1024] - out[2 * STEP - 64]).abs();
    // The square written, past its edge half a bar in.
    let mut square = song.clone();
    square.tracks[0].mods[3].lfo.shape = Shape::Square;
    *Arc::make_mut(square.tracks[0].phrases.get_mut(&0).unwrap())
        .step_mut(2)
        .unwrap() = "C-3 -- HLD1 --- ---".parse().unwrap();
    let edge = sharpest(&left(&square), 8 * STEP - 64, 8 * STEP + 2048);
    println!("swap {swap:e}, running {running:e}, a square's edge {edge:e}, jump {jump}");
    assert!(jump > 1.0, "the swap moves the level: {jump}");
    assert!(swap < edge, "swap {swap:e}, edge {edge:e}");
    assert!(swap * 50.0 < jump, "swap {swap:e}, a step of {jump}");
}

/// Four steps in `Free` at `1/16`, as Mod 3, looping from `From 1` unless a step says.
fn free_four(song: &mut Song) {
    song.tracks[0].mods[2] = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values: [0x10; 16],
            length: 4,
            mode: StepsMode::Free,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
}

/// The steps Mod 3 plays over so many steps of a song.
fn walked(song: &Song, steps: usize) -> Vec<u8> {
    let mut playing = Playing::new(song);
    (0..steps)
        .map(|step| playing.step_at(step * STEP + STEP / 2))
        .collect()
}

/// A lock of Steps' `From` (`FROM3`) waits for the next time round: one part way through a lap
/// lets it play out, and one on a lap's first step is taken there (decision 428).
#[test]
fn a_from_lock_is_taken_up_as_the_next_lap_starts() {
    let note = "C-3 -- --- --- ---";
    let lock = "--- -- mod3.from:02 --- ---";
    let with_lock_at = |row: usize| {
        let mut texts = [""; 16];
        texts[0] = note;
        texts[row] = lock;
        let mut song = song(&texts);
        free_four(&mut song);
        song
    };
    // Locked on the second step of the second lap, it plays that lap out.
    assert_eq!(
        walked(&with_lock_at(5), 16),
        [0, 1, 2, 3, 0, 1, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3]
    );
    // On a lap's first step, it's heard there.
    assert_eq!(
        walked(&with_lock_at(8), 16),
        [0, 1, 2, 3, 0, 1, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3]
    );
    assert_eq!(
        walked(&with_lock_at(4), 12),
        [0, 1, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3]
    );
    // In the intro it's taken at once, which the intro doesn't play.
    assert_eq!(walked(&with_lock_at(2), 8), [0, 1, 2, 3, 2, 3, 2, 3]);
    // A `From` past the `Length` goes round from the last step.
    let mut texts = [""; 16];
    texts[0] = note;
    texts[1] = "--- -- mod3.from:0C --- ---";
    let mut song = song(&texts);
    free_four(&mut song);
    assert_eq!(walked(&song, 8), [0, 1, 2, 3, 3, 3, 3, 3]);
    // And a render repeats.
    assert_eq!(walked(&with_lock_at(5), 16), walked(&with_lock_at(5), 16));
}

/// An LFO in Mod 2 moves Mod 3's `From`: a square a bar long, its top four steps up from
/// `From 1`, its bottom at `From 1`. The bottom comes as the intro ends, so the eight go round
/// whole; the top comes as a lap starts, so the last four go round twice; then all eight again
/// (decision 428).
#[test]
fn an_lfo_moves_another_slots_from_a_lap_at_a_time() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    let track = &mut song.tracks[0];
    track.mods[1] = ModSlot::lfo(
        Some(own(2, ModKind::Steps, ModField::From)),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Note(3840),
            ..Lfo::default()
        },
    )
    .with_depth(0x04);
    track.mods[2] = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values: [0x10; 16],
            length: 8,
            mode: StepsMode::Free,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
    let played = walked(&song, 32);
    let all: Vec<u8> = (0..8).collect();
    assert_eq!(played[..8], all, "the intro, {played:?}");
    assert_eq!(played[8..16], all, "{played:?}");
    assert_eq!(played[16..24], [4, 5, 6, 7, 4, 5, 6, 7], "{played:?}");
    assert_eq!(played[24..32], all, "{played:?}");
    assert_eq!(played, walked(&song, 32), "a render repeats");
}
