use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::link::{Command, connect};
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_engine::track_state::LiveValues;
use vt_model::insert::{Distort, DistortType, Insert};
use vt_model::lfo::{Lfo, LfoMode, Rate, Shape};
use vt_model::modulation::{MOD_SLOTS, ModSlot};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{Effect, SettingId};
use vt_model::song::{Machine, Song, SynthVoice, level_gain};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A quarter note, as ticks.
const QUARTER: u32 = 960;
/// Held `VOL 80`, which leaves room for an LFO either way.
const HELD: f32 = 128.0;
/// What `Depth 40` swings by, in level values.
const SWING: f32 = 64.0;

fn pool(frames: Vec<[f32; 2]>) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn flat() -> Arc<SamplePool> {
    pool(vec![[1.0, 1.0]; RATE as usize * 20])
}

/// A slow wave, so a jump in the output can only come from the filter.
fn slow_sine() -> Arc<SamplePool> {
    let wave = |frame: usize| {
        let at = frame as f32 / RATE as f32;
        let value = (at * 200.0 * std::f32::consts::TAU).sin() * 0.5;
        [value, value]
    };
    pool((0..RATE as usize * 8).map(wave).collect())
}

/// Every frame alternating, which a low-pass filter clearly turns down.
fn buzz() -> Arc<SamplePool> {
    let wave = |frame: usize| {
        let value = if frame.is_multiple_of(2) { 0.5 } else { -0.5 };
        [value, value]
    };
    pool((0..RATE as usize * 8).map(wave).collect())
}

fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// A song holding `VOL 80` on one long note, with this LFO moving the volume.
fn moving(lfo: Lfo) -> Song {
    let mut song = song(&["C-3 -- vol:80 --- ---"]);
    song.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::VOL), lfo).with_depth(0x40);
    song
}

fn version(song: Song, samples: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), samples))
}

fn render(song: Song, samples: Arc<SamplePool>, frames: usize) -> Vec<f32> {
    render_frames(version(song, samples), RATE, frames)
}

fn left(out: &[f32], frame: usize) -> f32 {
    out[frame * 2]
}

/// Within a dB and a half, since smoothing means a fast swing lags slightly at its peaks.
fn close(actual: f32, expected: f32) {
    let apart = 20.0 * (actual / expected).log10();
    assert!(apart.abs() < 1.5, "{actual} and {expected}");
}

#[test]
fn a_synced_lfo_moves_around_the_held_value_in_time() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP * 8);
    close(left(&out, STEP), level_gain(HELD + SWING));
    close(left(&out, STEP * 2), level_gain(HELD));
    close(left(&out, STEP * 3), level_gain(HELD - SWING));
    close(left(&out, STEP * 5), level_gain(HELD + SWING));
}

#[test]
fn deform_widens_a_square_and_a_step_locks_it() {
    // A square a quarter long, four steps: high for two and low for two, or with `Deform +80`
    // high for three.
    let square = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let middle = |step: usize| STEP * step + STEP / 2;
    let (high, low) = (level_gain(HELD + SWING), level_gain(HELD - SWING));
    let plain = render(moving(square), flat(), STEP * 4);
    close(left(&plain, middle(2)), low);
    let wide = Lfo {
        deform: 0x80,
        ..square
    };
    let out = render(moving(wide), flat(), STEP * 4);
    close(left(&out, middle(1)), high);
    close(left(&out, middle(2)), high);
    close(left(&out, middle(3)), low);
    // `DFM4 C0` from the second cycle is the same width: `80` is none.
    let mut locked = song(&[
        "C-3 -- vol:80 --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- mod4.deform:C0 --- ---",
    ]);
    locked.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::VOL), square).with_depth(0x40);
    let out = render(locked, flat(), STEP * 8);
    close(left(&out, middle(2)), low);
    close(left(&out, middle(6)), high);
    close(left(&out, middle(7)), low);
}

#[test]
fn a_held_vol_of_e0_is_exactly_unity_and_the_top_is_6_db() {
    let out = render(song(&["C-3 -- --- --- ---"]), flat(), STEP);
    assert!((left(&out, STEP - 1) - 1.0).abs() < 1e-6);
    let out = render(song(&["C-3 -- vol:E0 --- ---"]), flat(), STEP);
    assert!((left(&out, STEP - 1) - 1.0).abs() < 1e-6);
    let mut limitless = song(&["C-3 -- vol:F8 --- ---"]);
    limitless.limiter = false;
    let out = render(limitless, flat(), STEP);
    let top = left(&out, STEP - 1);
    assert!((20.0 * top.log10() - 6.0).abs() < 0.01, "{top}");
}

#[test]
fn a_synced_rate_follows_the_tempo_and_hertz_does_not() {
    let synced = Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let hertz = Lfo {
        rate: Rate::Hertz(100),
        ..Lfo::default()
    };
    // At 60 BPM a quarter note takes a second, so both peak at the same frame.
    let at_60 = |lfo| {
        let mut song = moving(lfo);
        song.millibpm = 60_000;
        render(song, flat(), RATE as usize)
    };
    let peak = RATE as usize / 4;
    close(left(&at_60(synced), peak), level_gain(HELD + SWING));
    close(left(&at_60(hertz), peak), level_gain(HELD + SWING));

    // At 120 BPM, a quarter note takes half the time and hertz doesn't change.
    let out = render(moving(synced), flat(), RATE as usize);
    close(left(&out, peak), level_gain(HELD));
    let out = render(moving(hertz), flat(), RATE as usize);
    close(left(&out, peak), level_gain(HELD + SWING));
}

#[test]
fn a_reset_leaves_lfos_running() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER * 3),
        ..Lfo::default()
    };
    let free = render(moving(lfo), flat(), STEP * 12);
    let mut reset = moving(lfo);
    reset.sections[0].reset = Some(1000);
    // The step holding VOL isn't reached again, so only the reset's own effect could differ.
    let reset_late = render(reset.clone(), flat(), STEP * 12);
    assert_eq!(free, reset_late);

    reset.sections[0].reset = Some(5);
    // Each reset replays the first step, which holds the same value, so the LFO's path shows.
    let out = render(reset, flat(), STEP * 12);
    for frame in [STEP * 3 + 1000, STEP * 7 + 1000, STEP * 11 + 1000] {
        close(left(&out, frame), left(&free, frame));
    }
}

#[test]
fn retrig_restarts_it_on_each_note() {
    let texts = [
        "C-3 -- vol:80h --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ];
    let ramp = |mode| {
        let mut song = song(&texts);
        let lfo = Lfo {
            shape: Shape::Ramp,
            rate: Rate::Note(QUARTER * 4),
            mode,
            ..Lfo::default()
        };
        song.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::VOL), lfo).with_depth(0x40);
        render(song, flat(), STEP * 6)
    };
    let frame = STEP * 4 + 2000;
    close(
        left(&ramp(LfoMode::Free), frame),
        level_gain(HELD - SWING * 0.5),
    );
    close(left(&ramp(LfoMode::Trig), frame), level_gain(HELD - SWING));
}

#[test]
fn phase_and_negative_depth_move_the_wave() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER),
        phase: 90,
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP);
    close(left(&out, 100), level_gain(HELD + SWING));

    let mut flipped = moving(Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    });
    flipped.tracks[0].mods[3].depth = -0x40;
    let out = render(flipped, flat(), STEP * 2);
    close(left(&out, STEP), level_gain(HELD - SWING));
}

#[test]
fn two_lfos_add_up_and_no_destination_does_nothing() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER * 4),
        ..Lfo::default()
    };
    let mut both = moving(lfo);
    both.tracks[0].mods[4] = both.tracks[0].mods[3];
    both.tracks[0].mods[4].depth = 0x20;
    let out = render(both, flat(), STEP * 5);
    close(left(&out, STEP * 4), level_gain(HELD + SWING * 1.5));

    let mut none = moving(lfo);
    none.tracks[0].mods[3].destination = None;
    let out = render(none, flat(), STEP * 5);
    close(left(&out, STEP * 4), level_gain(HELD));
}

#[test]
fn a_random_wave_holds_for_a_cycle_and_follows_the_seed() {
    let lfo = Lfo {
        shape: Shape::Random,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP * 12);
    close(left(&out, STEP), left(&out, STEP * 3));
    assert!((left(&out, STEP * 3) - left(&out, STEP * 7)).abs() > 0.001);
    assert_eq!(out, render(moving(lfo), flat(), STEP * 12));

    let mut reseeded = moving(lfo);
    reseeded.seed += 1;
    assert_ne!(out, render(reseeded, flat(), STEP * 12));
}

#[test]
fn a_smooth_random_wave_moves_within_a_cycle_and_follows_the_seed() {
    let lfo = |shape| Lfo {
        shape,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let smooth = lfo(Shape::Smooth);
    let held = render(moving(lfo(Shape::Random)), flat(), STEP * 12);
    let out = render(moving(smooth), flat(), STEP * 12);
    // A quarter is four steps. It starts each on the value Random then settles at for that quarter,
    // and is elsewhere in between.
    close(left(&out, STEP * 4), left(&held, STEP * 5));
    assert!((left(&out, STEP * 6) - left(&out, STEP * 4)).abs() > 0.001);
    assert!((left(&out, STEP * 6) - left(&held, STEP * 6)).abs() > 0.001);
    assert_eq!(out, render(moving(smooth), flat(), STEP * 12));

    let mut reseeded = moving(smooth);
    reseeded.seed += 1;
    assert_ne!(out, render(reseeded, flat(), STEP * 12));
}

#[test]
fn block_size_does_not_change_the_result() {
    let lfo = Lfo {
        shape: Shape::Sine,
        rate: Rate::Hertz(370),
        ..Lfo::default()
    };
    let whole = render(moving(lfo), flat(), STEP * 4);
    let mut engine = Engine::new(RATE, version(moving(lfo), flat()));
    engine.play_from(0);
    let mut pieces = vec![0.0; whole.len()];
    for chunk in pieces.chunks_mut(2 * 37) {
        engine.process(chunk);
    }
    assert_eq!(whole, pieces);
}

#[test]
fn the_filter_stays_running_when_an_lfo_takes_it_to_the_top() {
    // Half of each cycle sits at the top of the range. If the filter stopped there, it would come
    // back with old state and jump.
    let mut moved = song(&["C-3 -- --- --- ---"]);
    moved.tracks[0].cutoff = 0xFF;
    let lfo = Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    moved.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::CUT), lfo).with_depth(0xC0);
    let out = render(moved, slow_sine(), STEP * 8);
    let biggest = out
        .as_chunks::<2>()
        .0
        .iter()
        .map(|frame| frame[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max);
    assert!(biggest < 0.03, "jumped {biggest}");
}

#[test]
fn a_tracks_base_is_where_its_held_values_start() {
    let from_step = render(song(&["C-3 -- cut:40 pan:20 ---"]), buzz(), STEP);
    let mut from_page = song(&["C-3 -- --- --- ---"]);
    from_page.tracks[0].cutoff = 0x40;
    from_page.tracks[0].pan = 0x20;
    assert_eq!(from_step, render(from_page, buzz(), STEP));
}

#[test]
fn a_page_change_is_heard_now_and_leaves_what_a_step_set() {
    let start = song(&["C-3 -- pan:00h --- ---"]);
    let samples = flat();
    let mut engine = Engine::new(RATE, version(start.clone(), Arc::clone(&samples)));
    engine.play_from(0);
    let mut out = vec![0.0; 4000 * 2];
    engine.process(&mut out);
    let left_only = out[out.len() - 2];
    assert!(out[out.len() - 1].abs() < 1e-3);

    // Changing something else leaves the step's pan alone.
    let mut other = start.clone();
    other.tracks[0].cutoff = 0xF0;
    engine.swap(version(other.clone(), Arc::clone(&samples)));
    engine.process(&mut out);
    close(out[out.len() - 2], left_only);

    // Changing the pan itself moves it, until a step sets it again.
    other.tracks[0].pan = 0xFF;
    engine.swap(version(other, Arc::clone(&samples)));
    engine.process(&mut out);
    assert!(out[out.len() - 2].abs() < 1e-3, "{}", out[out.len() - 2]);
    assert!(out[out.len() - 1] > 0.5);
}

/// A running engine's live values for a track, after this many steps.
fn live_after(song: Song, steps: usize) -> LiveValues {
    let mut engine = Engine::new(RATE, version(song, flat()));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    for _ in 0..steps {
        engine.process(&mut out);
    }
    engine.live(0)
}

#[test]
fn live_values_are_the_settings_when_nothing_moves_them() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    let track = &mut song.tracks[0];
    (track.level, track.pan, track.cutoff, track.resonance) = (0x90, 0x30, 0x70, 0x20);
    (track.send_a, track.send_b) = (0x11, 0x22);
    track.machine = Machine::Plaits(SynthVoice::Kick);
    for (setting, value) in [0x33, 0x44, 0x55, 0x66].into_iter().enumerate() {
        track.set_machine_value(setting, value);
    }
    let expected = LiveValues {
        level: 0x90,
        pan: 0x30,
        cutoff: 0x70,
        resonance: 0x20,
        send_a: 0x11,
        send_b: 0x22,
        machine: [0x33, 0x44, 0x55, 0x66, 0, 0, 0, 0, 0],
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
        after_trim: Default::default(),
        mods: Default::default(),
    };
    assert_eq!(live_after(song, 3), expected);
}

#[test]
fn live_values_follow_lfos_and_step_commands_and_stop_with_the_transport() {
    // A square LFO a quarter long is at its top for the first two steps, and bottom for the next.
    let moved = |destination, base: fn(&mut Song)| {
        let mut song = song(&["C-3 -- --- --- ---"]);
        base(&mut song);
        let lfo = Lfo {
            shape: Shape::Square,
            rate: Rate::Note(QUARTER),
            ..Lfo::default()
        };
        song.tracks[0].mods[3] = ModSlot::lfo(Some(destination), lfo).with_depth(0x20);
        song
    };
    let cut = moved(SettingId::CUT, |song| song.tracks[0].cutoff = 0x80);
    assert_eq!(live_after(cut.clone(), 1).cutoff, 0xA0);
    assert_eq!(live_after(cut.clone(), 3).cutoff, 0x60);

    let har = moved(
        SettingId::voice(SynthVoice::default(), 0).unwrap(),
        |song| {
            song.tracks[0].machine = Machine::Plaits(SynthVoice::default());
            song.tracks[0].set_machine_value(0, 0x80);
        },
    );
    assert_eq!(live_after(har, 1).machine[0], 0xA0);

    let dly = moved(SettingId::SEND_A, |song| song.tracks[0].send_a = 0x40);
    assert_eq!(live_after(dly, 1).send_a, 0x60);

    // `Vol` shows on the track's level, around its own setting.
    let vol = moved(SettingId::VOL, |song| song.tracks[0].level = 0x90);
    assert_eq!(live_after(vol, 1).level, 0xB0);

    // It clamps at the ends of the range.
    let high = moved(SettingId::CUT, |song| song.tracks[0].cutoff = 0xF0);
    assert_eq!(live_after(high, 1).cutoff, 0xFF);

    // A step's command is what's playing as well.
    let stepped = song(&["C-3 -- cut:40 --- ---"]);
    assert_eq!(live_after(stepped, 1).cutoff, 0x40);

    // Once stopped, an LFO adds nothing.
    let mut engine = Engine::new(RATE, version(cut, flat()));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.live(0).cutoff, 0xA0);
    engine.return_to_start();
    engine.process(&mut out);
    assert_eq!(engine.live(0).cutoff, 0x80);
}

#[test]
fn the_first_envelope_on_vol_does_not_move_the_level() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].level = 0x90;
    song.tracks[0].mods[0].adsr.attack_ms = 500;
    assert_eq!(live_after(song, 1).level, 0x90);
}

#[test]
fn an_envelope_on_cut_moves_the_live_value() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].cutoff = 0x40;
    song.tracks[0].mods[1].destination = Some(SettingId::CUT);
    song.tracks[0].mods[1].adsr.attack_ms = 0;
    song.tracks[0].mods[1].adsr.decay_ms = 0;
    song.tracks[0].mods[1].adsr.sustain = 100;
    song.tracks[0].mods[1].depth = 0x40;
    assert_eq!(live_after(song, 1).cutoff, 0x80);
}

#[test]
fn the_callback_reports_the_watched_tracks_live_values() {
    let mut song = Song::new(2);
    song.tracks[0].cutoff = 0x10;
    song.tracks[1].cutoff = 0x20;
    let (mut handle, mut realtime) = connect(Engine::new(RATE, version(song, flat())));
    let mut out = vec![0.0; 256];
    realtime.process(&mut out);
    let report = handle.latest().unwrap();
    assert_eq!((report.watched, report.live.cutoff), (0, 0x10));
    assert!(handle.send(Command::Watch { track: 1 }));
    realtime.process(&mut out);
    let report = handle.latest().unwrap();
    assert_eq!((report.watched, report.live.cutoff), (1, 0x20));
    // A track that doesn't exist is safe to ask about.
    assert!(handle.send(Command::Watch { track: 99 }));
    realtime.process(&mut out);
    assert_eq!(handle.latest().unwrap().live, LiveValues::default());
}

fn with_distort(mut song: Song, kind: DistortType) -> Song {
    song.tracks[0].inserts[0] = Insert::Distort(Distort {
        kind,
        amount: 0x40,
        mix: 0xC0,
        gain: 0xD0,
        ..Distort::DEFAULT
    });
    song
}

const AMOUNT: usize = 1;
const MIX: usize = 2;
const GAIN: usize = 3;
const AMOUNT_TARGET: SettingId = SettingId::slot(0, Effect::Distort, 1).unwrap();
const BITS_TARGET: SettingId = SettingId::slot(0, Effect::Distort, 2).unwrap();

fn slot_live(song: Song, steps: usize) -> [u8; vt_model::insert::MAX_PARAMS] {
    live_after(song, steps).inserts[0]
}

#[test]
fn a_slots_settings_read_as_set_with_nothing_moving_them() {
    let live = slot_live(
        with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft),
        1,
    );
    assert_eq!((live[AMOUNT], live[MIX], live[GAIN]), (0x40, 0xC0, 0xD0));
}

#[test]
fn a_step_locks_a_slots_setting_until_the_next_note() {
    let locked = song(&[
        "C-3 -- 1.distort.amount:80 --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    let locked = with_distort(locked, DistortType::Soft);
    assert_eq!(slot_live(locked.clone(), 1)[AMOUNT], 0x80);
    assert_eq!(slot_live(locked.clone(), 2)[AMOUNT], 0x80, "it lasts");
    assert_eq!(slot_live(locked, 3)[AMOUNT], 0x40, "until the next note");
}

#[test]
fn a_step_holds_a_slots_setting_past_the_next_note() {
    let held = song(&[
        "C-3 -- 1.distort.mix:20h --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    assert_eq!(
        slot_live(with_distort(held, DistortType::Soft), 3)[MIX],
        0x20
    );
}

#[test]
fn a_step_glides_a_slots_setting() {
    let glide = song(&[
        "C-3 -- 1.distort.gain:00g --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- 1.distort.gain:80 --- ---",
    ]);
    let glide = with_distort(glide, DistortType::Soft);
    let halfway = slot_live(glide.clone(), 2)[GAIN];
    assert!((0x38..=0x48).contains(&halfway), "{halfway:02X}");
    assert_eq!(slot_live(glide, 5)[GAIN], 0x80);
}

#[test]
fn only_the_slot_a_command_names_changes() {
    let mut other = with_distort(
        song(&["C-3 -- 2.distort.amount:80 --- ---"]),
        DistortType::Soft,
    );
    other.tracks[0].inserts[1] = other.tracks[0].inserts[0];
    let live = live_after(other, 1);
    assert_eq!(live.inserts[0][AMOUNT], 0x40);
    assert_eq!(live.inserts[1][AMOUNT], 0x80);
}

#[test]
fn a_command_does_nothing_in_an_empty_slot() {
    let empty = song(&["C-3 -- 1.distort.amount:80 --- ---"]);
    assert_eq!(slot_live(empty, 1), [0; vt_model::insert::MAX_PARAMS]);
}

#[test]
fn bits_are_crushs_own_and_an_amount_does_nothing_there() {
    // Bits are a place in the list, from 2 up, so 6 bits is the fifth. On Crush, Bits is second.
    let crushed = with_distort(
        song(&["C-3 -- 1.distort.bits:04 --- ---"]),
        DistortType::Crush,
    );
    assert_eq!(slot_live(crushed, 1)[AMOUNT], 4);
    let amount = with_distort(
        song(&["C-3 -- 1.distort.amount:80 --- ---"]),
        DistortType::Crush,
    );
    assert_eq!(slot_live(amount, 1)[AMOUNT], 7, "still 9 bits");
}

fn moved_by_lfo(
    song_with: Song,
    target: SettingId,
    depth: i16,
    steps: usize,
) -> [u8; vt_model::insert::MAX_PARAMS] {
    let mut moved = song_with;
    let lfo = Lfo {
        shape: Shape::Square,
        rate: Rate::Steps(4),
        phase: 0,
        mode: LfoMode::Free,
        deform: 0,
    };
    moved.tracks[0].mods[3] = ModSlot::lfo(Some(target), lfo).with_depth(depth);
    slot_live(moved, steps)
}

#[test]
fn an_lfo_moves_a_slots_setting_both_ways() {
    let base = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft);
    // A square: up for the first half of its 4 steps, and down for the second.
    let up = moved_by_lfo(base.clone(), AMOUNT_TARGET, 0x20, 2)[AMOUNT];
    let down = moved_by_lfo(base.clone(), AMOUNT_TARGET, 0x20, 4)[AMOUNT];
    assert_eq!((up, down), (0x60, 0x20));
    assert_eq!(
        moved_by_lfo(base, AMOUNT_TARGET, 0x20, 2)[MIX],
        0xC0,
        "not the others"
    );
}

#[test]
fn an_lfo_on_a_slots_setting_stops_at_its_range() {
    let base = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft);
    assert_eq!(
        moved_by_lfo(base.clone(), AMOUNT_TARGET, 0xFF, 2)[AMOUNT],
        0xFF
    );
    assert_eq!(moved_by_lfo(base, AMOUNT_TARGET, 0xFF, 4)[AMOUNT], 0x00);
}

#[test]
fn an_lfo_on_bits_lands_on_whole_bits_and_one_on_type_jumps_between_them() {
    let crushed = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Crush);
    // A setting of 9 bits is place 7, and half of full depth is 5 of the 10 places.
    let moved = moved_by_lfo(crushed.clone(), BITS_TARGET, 0x80, 2)[AMOUNT];
    assert_eq!(moved, 10, "stops at the most bits");
    let moved = moved_by_lfo(crushed, BITS_TARGET, 0x80, 4)[AMOUNT];
    assert_eq!(moved, 2);
    let soft = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft);
    let kind = moved_by_lfo(
        soft,
        SettingId::slot(0, Effect::Distort, 0).unwrap(),
        0x80,
        2,
    )[0];
    assert_eq!(
        kind, 4,
        "half way up the 7 steps from the first, 3.5, rounded"
    );
}

#[test]
fn an_lfo_on_an_empty_slot_does_nothing() {
    let plain = song(&["C-3 -- --- --- ---"]);
    assert_eq!(
        moved_by_lfo(plain, AMOUNT_TARGET, 0x80, 2),
        [0; vt_model::insert::MAX_PARAMS]
    );
}

#[test]
fn an_envelope_moves_a_slots_setting() {
    let mut moved = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft);
    moved.tracks[0].mods[1].destination = Some(AMOUNT_TARGET);
    moved.tracks[0].mods[1].depth = 0x40;
    moved.tracks[0].mods[1].adsr.attack_ms = 0;
    moved.tracks[0].mods[1].adsr.decay_ms = 0;
    moved.tracks[0].mods[1].adsr.sustain = 100;
    assert_eq!(slot_live(moved, 1)[AMOUNT], 0x80);
}

#[test]
fn modulating_a_slot_is_heard_and_renders_the_same_each_time() {
    let render = |depth: i16| {
        let mut moved = with_distort(song(&["C-3 -- --- --- ---"]), DistortType::Soft);
        moved.tracks[0].mods[3] =
            ModSlot::lfo(Some(AMOUNT_TARGET), Lfo::default()).with_depth(depth);
        render_frames(version(moved, flat()), RATE, STEP * 8)
    };
    assert_eq!(render(0x80), render(0x80));
    assert_ne!(render(0x80), render(0));
}

/// How far a track's level swings in decibels over the last of a render.
fn swing_db(out: &[f32]) -> f32 {
    let left: Vec<f32> = out.iter().step_by(2).map(|sample| sample.abs()).collect();
    let tail = &left[left.len() / 2..];
    let most = tail.iter().copied().fold(0.0, f32::max);
    let least = tail.iter().copied().fold(1.0, f32::min);
    20.0 * (most / least).log10()
}

#[test]
fn a_fast_lfo_keeps_its_depth() {
    let swing = |hertz: u32, destination| {
        let mut moved = song(&["C-3 -- --- --- ---"]);
        let lfo = Lfo {
            shape: Shape::Sine,
            rate: Rate::Hertz(hertz * 100),
            ..Lfo::default()
        };
        moved.tracks[0].mods[3] = ModSlot::lfo(Some(destination), lfo).with_depth(0x40);
        swing_db(&render_frames(
            version(moved, flat()),
            RATE,
            RATE as usize / 2,
        ))
    };
    for destination in [SettingId::VOL, SettingId::PAN] {
        let (slow, fast) = (swing(2, destination), swing(50, destination));
        assert!(
            fast > slow - 1.5,
            "{destination:?}: {slow} dB slow, {fast} dB fast"
        );
    }
}

#[test]
fn a_target_that_is_not_on_the_track_does_nothing() {
    // Har on a sample track, and a slot with nothing in it.
    let plain = render_frames(version(song(&["C-3 -- --- --- ---"]), flat()), RATE, STEP);
    for destination in [
        SettingId::voice(SynthVoice::default(), 0).unwrap(),
        AMOUNT_TARGET,
    ] {
        let mut dead = song(&["C-3 -- --- --- ---"]);
        dead.tracks[0].mods[3] = ModSlot::lfo(Some(destination), Lfo::default()).with_depth(0xFF);
        dead.tracks[0].mods[1].destination = Some(destination);
        dead.tracks[0].mods[1].depth = 0xFF;
        assert_eq!(
            render_frames(version(dead, flat()), RATE, STEP),
            plain,
            "{destination:?}"
        );
    }
}

/// LFO 1 moving something of LFO 2's, which is on `Cut`.
fn lfo_on_lfo(target: SettingId, depth: i16, second_depth: i16) -> Song {
    let mut moved = song(&["C-3 -- --- --- ---"]);
    moved.tracks[0].cutoff = 0x80;
    let first = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER * 2),
        ..Lfo::default()
    };
    moved.tracks[0].mods[3] = ModSlot::lfo(Some(target), first).with_depth(depth);
    let second = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    moved.tracks[0].mods[4] = ModSlot::lfo(Some(SettingId::CUT), second).with_depth(second_depth);
    moved
}

#[test]
fn an_lfo_moves_another_lfos_depth() {
    // LFO 2 has no depth of its own, so it only moves while LFO 1 gives it some. A half note is 8
    // steps, so LFO 1 is up for the first 4, and down for the next.
    let moved = lfo_on_lfo(SettingId::modulator(4, 0).unwrap(), 0x20, 0);
    // LFO 2 is a quarter long, up for 2 steps and down for 2.
    assert_eq!(live_after(moved.clone(), 1).cutoff, 0xA0);
    assert_eq!(live_after(moved.clone(), 3).cutoff, 0x60);
    // With LFO 1 down, the depth is negative, so LFO 2 flips.
    assert_eq!(live_after(moved.clone(), 5).cutoff, 0x60);
    assert_eq!(live_after(moved, 7).cutoff, 0xA0);
}

#[test]
fn an_lfo_moves_another_lfos_phase() {
    let second = 0x20;
    // A depth of 0x80 is half a cycle, either way, so LFO 2 is flipped from where it would be.
    let moved = lfo_on_lfo(SettingId::modulator(4, 1).unwrap(), 0x80, second);
    let plain = lfo_on_lfo(SettingId::PITCH, 0, second);
    assert_eq!(live_after(plain, 1).cutoff, 0xA0);
    assert_eq!(live_after(moved, 1).cutoff, 0x60);
}

#[test]
fn an_lfo_moving_another_goes_one_way_only() {
    // LFO 2 on LFO 1's depth, set in a file, does nothing, as LFO 1 has already run.
    let mut back = song(&["C-3 -- --- --- ---"]);
    back.tracks[0].cutoff = 0x80;
    let square = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    back.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::CUT), square).with_depth(0x20);
    let plain = back.clone();
    back.tracks[0].mods[4] =
        ModSlot::lfo(Some(SettingId::modulator(3, 0).unwrap()), Lfo::default()).with_depth(0x40);
    assert_eq!(
        render_frames(version(back, flat()), RATE, STEP * 8),
        render_frames(version(plain, flat()), RATE, STEP * 8)
    );
}

#[test]
fn an_envelope_moves_an_lfos_depth() {
    let mut moved = song(&["C-3 -- --- --- ---"]);
    moved.tracks[0].cutoff = 0x80;
    let square = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    moved.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::CUT), square);
    let envelope = &mut moved.tracks[0].mods[1];
    envelope.destination = Some(SettingId::modulator(3, 0).unwrap());
    envelope.depth = 0x20;
    envelope.adsr.attack_ms = 0;
    envelope.adsr.decay_ms = 0;
    envelope.adsr.sustain = 100;
    assert_eq!(live_after(moved, 1).cutoff, 0xA0);
}

#[test]
fn an_lfo_moving_another_renders_the_same_each_time() {
    let render = || {
        let moved = lfo_on_lfo(SettingId::modulator(4, 0).unwrap(), 0x20, 0);
        render_frames(version(moved, flat()), RATE, STEP * 8)
    };
    assert_eq!(render(), render());
}

/// LFO 1 swinging the cutoff by a square a quarter long, with no depth of its own.
fn waiting_lfo(texts: &[&str]) -> Song {
    let mut song = song(texts);
    song.tracks[0].cutoff = 0x80;
    let square = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    song.tracks[0].mods[3] = ModSlot::lfo(Some(SettingId::CUT), square);
    song
}

#[test]
fn a_step_locks_an_lfos_depth_until_the_next_note() {
    // `9FE0` is a depth of +40.
    let locked = waiting_lfo(&[
        "C-3 -- mod4.depth:9FE0 --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    assert_eq!(live_after(locked.clone(), 1).cutoff, 0xC0);
    assert_eq!(
        live_after(locked.clone(), 3).cutoff,
        0x40,
        "the square's down half"
    );
    assert_eq!(live_after(locked, 5).cutoff, 0x80, "the next note ends it");
}

#[test]
fn a_step_locks_an_lfos_phase() {
    let mut turned = waiting_lfo(&["C-3 -- mod4.phase:7F80 --- ---"]);
    turned.tracks[0].mods[3].depth = 0x40;
    let mut plain = turned.clone();
    plain.tracks[0]
        .phrases
        .get_mut(&0)
        .map(Arc::make_mut)
        .unwrap()
        .step_mut(0)
        .unwrap()
        .fx[0] = Default::default();
    // Half a turn flips the square.
    assert_eq!(live_after(plain, 1).cutoff, 0xC0);
    assert_eq!(live_after(turned, 1).cutoff, 0x40);
}

#[test]
fn an_lfo_on_a_locked_depth_adds_to_it() {
    let mut moved = lfo_on_lfo(SettingId::modulator(4, 0).unwrap(), 0x20, 0);
    // LFO 2 alone has a depth of +20 from LFO 1. A lock of +20 more makes +40.
    let phrase = moved.tracks[0]
        .phrases
        .get_mut(&0)
        .map(Arc::make_mut)
        .unwrap();
    *phrase.step_mut(0).unwrap() = "C-3 -- mod5.depth:8FF0 --- ---".parse().unwrap();
    assert_eq!(live_after(moved, 1).cutoff, 0xC0);
}

#[test]
fn the_last_slot_moves_its_destination_and_a_lock_and_the_first_lfo_move_it() {
    let last = MOD_SLOTS - 1;
    let depth = SettingId::modulator(last, 0).unwrap();
    assert_eq!(depth.key(), format!("mod{MOD_SLOTS}.depth"));
    assert_eq!(depth.label(), Some(format!("DEP{MOD_SLOTS}")));
    let on_cut = |texts: &[&str]| {
        let mut song = waiting_lfo(texts);
        song.tracks[0].mods.swap(3, last);
        song
    };
    // `9FE0` is a depth of +40, as on the first LFO.
    let lock = format!("C-3 -- mod{MOD_SLOTS}.depth:9FE0 --- ---");
    let locked = on_cut(&[&lock]);
    assert_eq!(live_after(locked.clone(), 1).cutoff, 0xC0);
    assert_eq!(live_after(locked, 3).cutoff, 0x40);
    // The first LFO, a half note up and then down, gives it a depth of +20 and then -20.
    let mut moved = on_cut(&["C-3 -- --- --- ---"]);
    let half = Lfo {
        shape: Shape::Square,
        rate: Rate::Note(QUARTER * 2),
        ..Lfo::default()
    };
    moved.tracks[0].mods[3] = ModSlot::lfo(Some(depth), half).with_depth(0x20);
    assert_eq!(live_after(moved.clone(), 1).cutoff, 0xA0);
    assert_eq!(live_after(moved, 5).cutoff, 0x60);
}

#[test]
fn a_pitch_lock_plays_its_semitones_up() {
    let octave = render(song(&["C-3 -- pitch:8C --- ---"]), slow_sine(), STEP);
    let higher = render(song(&["C-4 -- --- --- ---"]), slow_sine(), STEP);
    let apart = |from: usize| {
        (from..STEP)
            .map(|frame| (left(&octave, frame) - left(&higher, frame)).abs())
            .fold(0.0, f32::max)
    };
    assert!(apart(STEP / 2) < 1e-3, "{}", apart(STEP / 2));
    let plain = render(song(&["C-3 -- pitch:8C80 --- ---"]), slow_sine(), STEP);
    assert_ne!(plain, octave, "the fine part bends it");
}
