//! A group track's macros holding settings on the tracks of its group (decision 408): a turn, a
//! step's `MAC` lock on the group's row and an LFO on the group's macro each move what it holds on
//! every track it reaches, generators' settings too, and what it lets go of goes back to the
//! track's own.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::render::render_frames;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::{Lfo, Rate};
use vt_model::macros::{GenColumn, MacroMap, MacroTarget, Zone};
use vt_model::modulation::ModSlot;
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, GenParam, Shape, ShapeColumn, Where};
use vt_model::song::{Machine, Phrase, Song, SynthVoice, TrackKind};
use vt_model::step::Step;

use crate::support::repeats;
use crate::support::steady::version;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// A mapping of a setting on the track at `member` in the group, from `min` to `max`.
fn on(member: u8, target: SettingId, min: u16, max: u16) -> MacroMap {
    MacroMap {
        member: Some(member),
        min,
        max,
        ..MacroMap::whole(MacroTarget::Setting(target))
    }
}

/// Three tracks, each with its `Cut` at `80` and `Res` at `40`, the first two in group 1 and the
/// third on its own, each playing these steps. Group 1's macro 1 holds `Cut` on both of its
/// tracks, from `30` to `A0` on the first and the other way, `C0` to `20`, on the second.
fn song(texts: &[&str]) -> (Song, usize) {
    let mut song = Song::new(3);
    for track in 0..3 {
        song.sample_on(track, "s");
        let track = &mut song.tracks[track];
        track.cutoff = 0x80;
        track.resonance = 0x40;
        let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
        for (index, text) in texts.iter().enumerate() {
            *phrase.step_mut(index).unwrap() = text.parse().unwrap();
        }
    }
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    song.tracks[group].macros[0].maps = vec![
        on(0, SettingId::CUT, 0x3000, 0xA000),
        on(1, SettingId::CUT, 0xC000, 0x2000),
    ];
    (song, group)
}

/// Each sound track's `Cut` and `Res` as heard after a block of this many frames.
fn heard(engine: &mut Engine, frames: usize) -> [(u8, u8); 3] {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    std::array::from_fn(|track| {
        let live = engine.live(track);
        (live.cutoff, live.resonance)
    })
}

#[test]
fn one_group_macro_turns_a_setting_on_two_of_its_tracks() {
    let (mut song, group) = song(&[]);
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(
        heard(&mut engine, 64),
        [(0x30, 0x40), (0xC0, 0x40), (0x80, 0x40)],
        "at 00, each at its min, and the track outside the group as it was"
    );
    for (value, first, second) in [(0xFF00, 0xA0, 0x20), (0x8000, 0x68, 0x70)] {
        song.tracks[group].macros[0].value = value;
        engine.swap(version(&song));
        let now = heard(&mut engine, 64);
        assert_eq!(
            now,
            [(first, 0x40), (second, 0x40), (0x80, 0x40)],
            "at {value:04X}"
        );
        assert_eq!(engine.live(group).macros[0], value);
    }
    // The group's track's own `Cut` is left alone.
    assert_eq!(engine.live(group).cutoff, song.tracks[group].cutoff);
}

#[test]
fn a_setting_a_group_lets_go_of_goes_back_to_the_tracks_own() {
    let (mut song, group) = song(&[]);
    song.tracks[group].macros[0].value = 0xFF00;
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(heard(&mut engine, 64)[0].0, 0xA0);
    song.tracks[group].macros[0].maps.remove(0);
    engine.swap(version(&song));
    assert_eq!(heard(&mut engine, 64)[..2], [(0x80, 0x40), (0x20, 0x40)]);
    // A track leaving the group is let go of too, and the next takes its place.
    song.tracks[0].group = None;
    engine.swap(version(&song));
    assert_eq!(
        heard(&mut engine, 64)[..2],
        [(0x80, 0x40), (0x80, 0x40)],
        "the second track is the group's first now, which macro 1 no longer holds"
    );
}

#[test]
fn a_track_in_the_group_turned_by_its_own_macro_too_hears_the_groups_last() {
    let (mut song, group) = song(&[]);
    song.tracks[0].macros[0].maps = vec![MacroMap::whole(MacroTarget::Setting(SettingId::CUT))];
    song.tracks[0].macros[0].value = 0x1000;
    song.tracks[group].macros[0].value = 0xFF00;
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(heard(&mut engine, 64)[0].0, 0xA0);
    song.tracks[0].macros[0].value = 0x2000;
    engine.swap(version(&song));
    assert_eq!(heard(&mut engine, 64)[0].0, 0xA0, "the group's still");
}

#[test]
fn a_zoned_group_macro_is_a_crossfader_over_its_tracks() {
    // `Cut` on the first track comes down from A0 to 30 over the left half, and `Res` on the
    // second goes up from 20 to C0 over the right.
    let (mut song, group) = song(&[]);
    song.tracks[group].macros[0].maps = vec![
        on(0, SettingId::CUT, 0xA000, 0x3000).zoned(Zone::new(0x00, 0x7F).unwrap()),
        on(1, SettingId::RES, 0x2000, 0xC000).zoned(Zone::new(0x80, 0xFF).unwrap()),
    ];
    let mut engine = Engine::new(RATE, version(&song));
    for (value, first, second) in [
        (0x0000, (0xA0, 0x40), (0x80, 0x20)),
        (0x7F00, (0x30, 0x40), (0x80, 0x20)),
        (0xFF00, (0x30, 0x40), (0x80, 0xC0)),
    ] {
        song.tracks[group].macros[0].value = value;
        engine.swap(version(&song));
        let now = heard(&mut engine, 64);
        assert_eq!(now[..2], [first, second], "at {value:04X}");
    }
}

#[test]
fn a_mac_lock_on_the_groups_row_reaches_its_tracks() {
    let (mut song, group) = song(&["C-3 -- --- --- ---", "C-3 -- --- --- ---"]);
    song.tracks[group].add_chain(0);
    song.sections[0].chains[group] = Some(0);
    let phrase = Arc::make_mut(song.tracks[group].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "--- -- macro1:FF --- ---".parse().unwrap();
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let now = heard(&mut engine, 64);
    assert_eq!((now[0].0, now[1].0), (0xA0, 0x20), "on the lock's own row");
}

#[test]
fn an_lfo_on_the_groups_macro_sweeps_its_tracks_together() {
    let (mut song, group) = song(&["C-3 -- --- --- ---"]);
    song.tracks[group].macros[0].value = 0x8000;
    song.tracks[group].mods[3] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(4),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let seen: Vec<_> = (0..64).map(|_| heard(&mut engine, STEP / 8)).collect();
    let first: Vec<u8> = seen.iter().map(|heard| heard[0].0).collect();
    let (lowest, highest) = (*first.iter().min().unwrap(), *first.iter().max().unwrap());
    assert!(lowest >= 0x30 && highest <= 0xA0, "{first:?}");
    assert!(highest - lowest > 0x50, "it sweeps: {first:?}");
    for heard in &seen {
        // The second the same way along its range, turned the other way.
        let along = f32::from(heard[0].0 - 0x30) / f32::from(0xA0 - 0x30_u8);
        let second = f32::from(0xC0 - heard[1].0) / f32::from(0xC0 - 0x20_u8);
        assert!((along - second).abs() < 0.03, "{seen:?}");
        assert_eq!(heard[2], (0x80, 0x40), "the track outside the group stays");
    }
    repeats("an LFO on a group's macro", || {
        render_frames(version(&song), RATE, STEP * 8)
    });
}

#[test]
fn a_group_whose_macros_hold_nothing_of_its_tracks_plays_as_one_with_none() {
    let (mut song, group) = song(&["C-3 -- --- --- ---", "C-3 -- --- --- ---"]);
    song.tracks[group].macros[0].maps.clear();
    let mut with = song.clone();
    with.tracks[group].macros[2].value = 0x8000;
    // One holding the group's own `Cut`, which isn't its tracks'.
    with.tracks[group].macros[1].maps = vec![MacroMap::whole(MacroTarget::Setting(SettingId::RES))];
    song.tracks[group].macros[1] = with.tracks[group].macros[1].clone();
    let render = |song: &Song| render_frames(version(song), RATE, STEP * 4);
    assert!(render(&song) == render(&with));
}

fn apply(song: Song, action: &Action) -> Song {
    Arc::unwrap_or_clone(edit(&Arc::new(song), action).expect("the edit applies"))
}

/// Three hits of `7F` spread over sixteen rows.
fn tresillo() -> Shape {
    Shape {
        from: 0x7F,
        to: 0x7F,
        kind: FillKind::Alternate,
        place: Where::Euclid { hits: 3, rotate: 0 },
        seed: 7,
        ..Shape::default()
    }
}

/// Two synth tracks in group 1, each with sixteen notes at `40` and the tresillo live on their
/// velocities, and the group's macro 1 holding `Hits` on both, from 1 to 16 on the first and 16
/// to 1 on the second, turned to `value`.
fn kit(value: u16) -> (Song, usize) {
    let mut song = Song::new(2);
    let hits = MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Hits,
    };
    for track in 0..2 {
        song.tracks[track].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
        song.tracks[track].group = Some(1);
        let steps: Vec<Step> = vec!["C-3 40 --- --- ---".parse().unwrap(); 16];
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
        song = apply(
            song,
            &Action::Generate {
                track,
                phrase: 0,
                column: ShapeColumn::Velocity,
                span: Span::rows(StepRange::between(0, 15)),
                shape: tresillo(),
            },
        );
    }
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    for (member, ends) in [(0, (1, 16)), (1, (16, 1))] {
        let map = MacroMap {
            member: Some(member),
            ..MacroMap::numbers(hits, ends)
        };
        song = apply(
            song,
            &Action::MapMacro {
                track: group,
                at: 0,
                map,
            },
        );
    }
    let song = apply(
        song,
        &Action::SetMacro {
            track: group,
            at: 0,
            value,
        },
    );
    (song, group)
}

/// How many rows of a track's phrase are written with the tresillo's `7F`.
fn hits(song: &Song, track: usize) -> usize {
    song.tracks[track].phrases[&0]
        .steps()
        .iter()
        .filter(|step| step.velocity == Some(0x7F))
        .count()
}

#[test]
fn a_group_macro_holds_a_generators_settings_on_each_of_its_tracks() {
    // 1 + 15 × 0x44 / 0xFF is 5, and the other way 16 - 4 is 12.
    let (song, group) = kit(0x4400);
    assert_eq!(
        (hits(&song, 0), hits(&song, 1)),
        (5, 12),
        "its turn is written"
    );
    // At its turn, what's written is what's heard, and nothing renders.
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut block = vec![0.0; STEP * 2];
    for _ in 0..16 {
        engine.process(&mut block);
    }
    for track in 0..2 {
        assert!(engine.generators_heard(track).all(|run| run.renders == 0));
    }
    // Moved by an LFO on the group's macro, each track's generator renders again, its own way.
    let mut moving = song.clone();
    moving.tracks[group].mods[3] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(4),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let mut engine = Engine::new(RATE, version(&moving));
    engine.play_from(0);
    for _ in 0..16 {
        engine.process(&mut block);
    }
    for track in 0..2 {
        assert!(
            engine.generators_heard(track).any(|run| run.renders > 0),
            "track {track} renders"
        );
    }
    repeats("a group's LFO on its tracks' generators", || {
        render_frames(version(&moving), RATE, STEP * 16)
    });
}
