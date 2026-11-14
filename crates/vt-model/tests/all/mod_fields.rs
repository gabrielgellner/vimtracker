//! A mod slot's settings read and set by what they are, and the track's own controls, checked
//! against how the track page and the Roto chose each one's field before they moved into the
//! model (#726). Each `today_` function is that code as it was, so the pages can switch over to
//! [`ModSlot::field`], [`ModSpec`] and [`Track::control`] without changing what they do.

use vt_model::action::TrackControl;
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_LEVEL};
use vt_model::envelope::{
    Adsr, Ahd, Curve, Drum, Hold, Lpg, MAX_HOLD_MS, MAX_RATCHET, time_at, time_value,
};
use vt_model::grid::Grid;
use vt_model::lfo::{Fade, Lfo, LfoMode, MAX_DEFORM, Rate, Shape};
use vt_model::modulation::{
    MAX_DEPTH, MOD_LOCKED_FIELDS, MOD_OWN_FIELDS, MOD_SPECS, ModField, ModKind, ModSlot, Polarity,
    depth_of, depth_value,
};
use vt_model::settings::{MOD_OWN_SETTINGS, SettingId};
use vt_model::song::Track;
use vt_model::steps::{Loop, MAX_MUTATE, MAX_STEP, STEPS, StepRate, Steps, StepsMode};
use vt_model::tracking::{Bounds, Source, Tracking};

/// Every setting a slot can have, with Steps' first and last values and one past them.
const FIELDS: [ModField; 30] = [
    ModField::Type,
    ModField::Mode,
    ModField::Polarity,
    ModField::Destination,
    ModField::Depth,
    ModField::Attack,
    ModField::Decay,
    ModField::Sustain,
    ModField::Release,
    ModField::Reset,
    ModField::Hold,
    ModField::Peak,
    ModField::Body,
    ModField::Ratchet,
    ModField::Colour,
    ModField::Shape,
    ModField::Rate,
    ModField::Phase,
    ModField::Deform,
    ModField::Fade,
    ModField::Length,
    ModField::Start,
    ModField::Loop,
    ModField::Mutate,
    ModField::Smooth,
    ModField::Step(0),
    ModField::Step(15),
    ModField::Step(16),
    ModField::Low,
    ModField::High,
];

/// A slot whose types' settings all differ from each other, so reading or setting the wrong
/// type's shows.
fn distinct(kind: ModKind) -> ModSlot {
    let mut steps = Steps {
        mode: StepsMode::Note,
        length: 11,
        start: 3,
        rate: StepRate::ALL[4],
        looping: Loop::Bounce,
        mutate: 37,
        smooth: -70,
        ..Steps::default()
    };
    for (at, value) in steps.values.iter_mut().enumerate() {
        *value = at as i16 * 13 - 100;
    }
    ModSlot {
        kind,
        destination: Some(SettingId::CUT),
        depth: -0x42,
        bypassed: false,
        polarity: Some(Polarity::Bi),
        in_scale: false,
        adsr: Adsr {
            attack_ms: 11,
            decay_ms: 44,
            sustain: 63,
            release_ms: 88,
            reset: false,
            curve: Curve::Lin,
        },
        ahd: Ahd {
            attack_ms: 22,
            hold: Hold::Ms(99),
            decay_ms: 55,
            reset: true,
            curve: Curve::Snap,
        },
        drum: Drum {
            peak_ms: 12,
            body: 71,
            decay_ms: 66,
            ratchet: 3,
            curve: Curve::Analog,
        },
        lpg: Lpg {
            attack_ms: 33,
            decay_ms: 77,
            colour: 0x9A,
            reset: false,
        },
        lfo: Lfo {
            shape: Shape::Saw,
            rate: Rate::Hertz(250),
            phase: 135,
            mode: LfoMode::One,
            deform: 0x31,
            fade: Fade::ALL[2],
        },
        steps,
        track: Tracking {
            source: Source::Note,
            note: Bounds { low: 40, high: 90 },
            ..Tracking::default()
        },
    }
}

/// Where a value is in a list.
fn place<T: PartialEq>(items: &[T], value: T) -> i32 {
    items.iter().position(|item| *item == value).unwrap() as i32
}

/// `ModField::on`, as the track page had it.
fn today_on(field: ModField, kind: ModKind) -> bool {
    match field {
        ModField::Type | ModField::Destination | ModField::Depth => true,
        ModField::Polarity => kind != ModKind::Steps,
        ModField::Mode => kind != ModKind::Lpg,
        ModField::Decay => matches!(
            kind,
            ModKind::Adsr | ModKind::Ahd | ModKind::Drum | ModKind::Lpg
        ),
        ModField::Attack | ModField::Reset => {
            matches!(kind, ModKind::Adsr | ModKind::Ahd | ModKind::Lpg)
        }
        ModField::Peak | ModField::Body | ModField::Ratchet => kind == ModKind::Drum,
        ModField::Colour => kind == ModKind::Lpg,
        ModField::Sustain | ModField::Release => kind == ModKind::Adsr,
        ModField::Hold => kind == ModKind::Ahd,
        ModField::Shape | ModField::Phase | ModField::Deform | ModField::Fade => {
            kind == ModKind::Lfo
        }
        ModField::Rate => matches!(kind, ModKind::Lfo | ModKind::Steps),
        ModField::Length
        | ModField::Start
        | ModField::Loop
        | ModField::Mutate
        | ModField::Smooth
        | ModField::Step(_) => kind == ModKind::Steps,
        ModField::Low | ModField::High => kind == ModKind::Track,
    }
}

/// The field the track page's `adjust_mod` and `mod_at_default` and the Roto's knobs read for a
/// setting under a type, in [`ModSlot::field`]'s units.
fn today_value(slot: &ModSlot, field: ModField) -> Option<i32> {
    let kind = slot.kind;
    let (adsr, ahd, drum, lpg, lfo, steps) = (
        &slot.adsr,
        &slot.ahd,
        &slot.drum,
        &slot.lpg,
        &slot.lfo,
        &slot.steps,
    );
    let ahd_kind = kind == ModKind::Ahd;
    let lpg_kind = kind == ModKind::Lpg;
    let drum_kind = kind == ModKind::Drum;
    Some(match field {
        ModField::Type => place(&ModKind::ALL, kind),
        ModField::Mode => match kind {
            ModKind::Adsr => place(&Curve::ALL, adsr.curve),
            ModKind::Ahd => place(&Curve::ALL, ahd.curve),
            ModKind::Drum => place(&Curve::ALL, drum.curve),
            ModKind::Lfo => place(&LfoMode::ALL, lfo.mode),
            ModKind::Steps => place(&StepsMode::ALL, steps.mode),
            ModKind::Track => place(&Source::ALL, slot.track.source),
            ModKind::Lpg => return None,
        },
        ModField::Polarity => i32::from(slot.polarity().index()),
        ModField::Depth => i32::from(slot.depth),
        ModField::Attack if ahd_kind => i32::from(ahd.attack_ms),
        ModField::Attack if lpg_kind => i32::from(lpg.attack_ms),
        ModField::Attack => i32::from(adsr.attack_ms),
        ModField::Decay if ahd_kind => i32::from(ahd.decay_ms),
        ModField::Decay if drum_kind => i32::from(drum.decay_ms),
        ModField::Decay if lpg_kind => i32::from(lpg.decay_ms),
        ModField::Decay => i32::from(adsr.decay_ms),
        ModField::Reset if ahd_kind => i32::from(ahd.reset),
        ModField::Reset if lpg_kind => i32::from(lpg.reset),
        ModField::Reset => i32::from(adsr.reset),
        ModField::Colour => i32::from(lpg.colour),
        ModField::Peak => i32::from(drum.peak_ms),
        ModField::Body => i32::from(drum.body),
        ModField::Ratchet => i32::from(drum.ratchet),
        ModField::Hold => match ahd.hold {
            Hold::Note => -1,
            Hold::Ms(ms) => i32::from(ms),
        },
        ModField::Release => i32::from(adsr.release_ms),
        ModField::Sustain => i32::from(adsr.sustain),
        ModField::Shape => place(&Shape::ALL, lfo.shape),
        ModField::Phase => i32::from(lfo.phase),
        ModField::Deform => i32::from(lfo.deform),
        ModField::Fade => place(&Fade::ALL, lfo.fade),
        ModField::Length => i32::from(steps.length),
        ModField::Start => i32::from(steps.start),
        ModField::Loop => place(&Loop::ALL, steps.looping),
        ModField::Mutate => i32::from(steps.mutate),
        ModField::Smooth => i32::from(steps.smooth),
        ModField::Step(at) => i32::from(*steps.values.get(usize::from(at))?),
        ModField::Low => i32::from(slot.track.bounds().low),
        ModField::High => i32::from(slot.track.bounds().high),
        ModField::Rate | ModField::Destination => return None,
    })
}

/// The ends the track page's `adjust_mod` stopped each setting at.
fn today_bounds(slot: &ModSlot, field: ModField) -> Option<(i32, i32)> {
    let list = |count: usize| (0, count as i32 - 1);
    Some(match field {
        ModField::Type => list(ModKind::ALL.len()),
        ModField::Mode => list(match slot.kind {
            ModKind::Lfo => LfoMode::ALL.len(),
            ModKind::Steps => StepsMode::ALL.len(),
            ModKind::Track => Source::ALL.len(),
            ModKind::Lpg => return None,
            _ => Curve::ALL.len(),
        }),
        ModField::Polarity | ModField::Reset => (0, 1),
        ModField::Depth => (-i32::from(MAX_DEPTH), i32::from(MAX_DEPTH)),
        ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => {
            (0, i32::from(MAX_ENVELOPE_MS))
        }
        ModField::Hold => (-1, i32::from(MAX_HOLD_MS)),
        ModField::Sustain | ModField::Body => (0, 100),
        ModField::Ratchet => (1, i32::from(MAX_RATCHET)),
        ModField::Colour => (0, 0xFF),
        ModField::Shape => list(Shape::ALL.len()),
        ModField::Phase => (0, 359),
        ModField::Deform => (-i32::from(MAX_DEFORM), i32::from(MAX_DEFORM)),
        ModField::Fade => list(Fade::ALL.len()),
        ModField::Length => (1, STEPS as i32),
        ModField::Start => list(STEPS),
        ModField::Loop => list(Loop::ALL.len()),
        ModField::Mutate => (0, i32::from(MAX_MUTATE)),
        ModField::Smooth => (-i32::from(MAX_STEP), i32::from(MAX_STEP)),
        ModField::Step(at) if usize::from(at) < STEPS => {
            (-i32::from(MAX_STEP), i32::from(MAX_STEP))
        }
        ModField::Low | ModField::High => {
            let (lowest, highest) = slot.track.source.span();
            (i32::from(lowest), i32::from(highest))
        }
        ModField::Rate | ModField::Destination | ModField::Step(_) => return None,
    })
}

#[test]
fn each_types_spec_says_what_its_matches_said() {
    for (at, kind) in ModKind::ALL.into_iter().enumerate() {
        assert_eq!(MOD_SPECS[at].kind, kind);
        assert_eq!(kind.spec().kind, kind);
    }
    let names = ModKind::ALL.map(ModKind::name);
    assert_eq!(
        names,
        ["ADSR", "AHD", "Drum", "LPG", "LFO", "Steps", "Track"]
    );
    let meanings = ModKind::ALL.map(ModKind::meaning);
    assert_eq!(
        meanings,
        [
            "held while the note is",
            "a hold at full, for a time or the note",
            "a fast peak onto a body that decays",
            "a vactrol's fall, closing the filter",
            "a cycle, free or from each note",
            "up to 16 values, in time or a note each",
            "the note, its velocity or a roll, each note",
        ]
    );
    let labels = ModKind::ALL.map(ModKind::mode_label);
    assert_eq!(
        labels,
        ["Curve", "Curve", "Curve", "Curve", "Mode", "Mode", "Source"]
    );
    for kind in ModKind::ALL {
        let envelope = matches!(
            kind,
            ModKind::Adsr | ModKind::Ahd | ModKind::Drum | ModKind::Lpg
        );
        assert_eq!(kind.is_envelope(), envelope, "{kind:?}");
        assert_eq!(kind.has_mode(), kind != ModKind::Lpg, "{kind:?}");
        assert_eq!(kind.has_polarity(), kind != ModKind::Steps, "{kind:?}");
        let polarity = if matches!(kind, ModKind::Lfo | ModKind::Steps) {
            Polarity::Bi
        } else {
            Polarity::Uni
        };
        assert_eq!(kind.polarity(), polarity, "{kind:?}");
    }
}

#[test]
fn a_type_has_the_fields_the_track_page_gave_it() {
    for kind in ModKind::ALL {
        for field in FIELDS {
            assert_eq!(field.on(kind), today_on(field, kind), "{kind:?} {field:?}");
        }
    }
}

/// A grid's columns, top to bottom.
type Columns = [[Option<usize>; 3]];

#[test]
fn each_types_title_settings_and_grid_are_the_mod_pages() {
    use ModField::*;
    let today: [(&[ModField], &Columns); 7] = [
        (
            &[Attack, Decay, Release, Sustain, Depth, Reset],
            &[[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]],
        ),
        (
            &[Attack, Hold, Decay, Depth, Reset],
            &[[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]],
        ),
        (
            &[Peak, Body, Decay, Depth, Ratchet],
            &[[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]],
        ),
        (
            &[Attack, Decay, Colour, Depth, Reset],
            &[[Some(0), Some(1), Some(2)], [Some(3), Some(4), None]],
        ),
        (
            &[Shape, Rate, Depth, Phase, Fade, Deform],
            &[[Some(0), Some(1), Some(2)], [Some(3), Some(4), Some(5)]],
        ),
        (
            &[Rate, Length, Depth, Start, Mutate, Smooth, Step(0)],
            &[
                [Some(0), Some(1), Some(2)],
                [Some(3), Some(4), Some(5)],
                [Some(6), Some(6), Some(6)],
            ],
        ),
        (
            &[Low, High, Depth],
            &[[Some(0), Some(1), Some(2)], [None, None, None]],
        ),
    ];
    for (kind, (fields, columns)) in ModKind::ALL.into_iter().zip(today) {
        let spec = kind.spec();
        assert_eq!(spec.fields, fields, "{kind:?}");
        assert_eq!(spec.grid, Grid::from_columns(columns), "{kind:?}");
        let title: Vec<ModField> = [Type, Mode, Loop, Polarity, Destination]
            .into_iter()
            .filter(|field| today_on(*field, kind))
            .collect();
        assert_eq!(spec.title, title, "{kind:?}");
    }
}

#[test]
fn a_field_reads_the_struct_field_its_type_keeps_it_in() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        for field in FIELDS {
            let wanted = if today_on(field, kind) {
                today_value(&slot, field)
            } else {
                None
            };
            assert_eq!(slot.field(field), wanted, "{kind:?} {field:?}");
            let bounds = if wanted.is_some() {
                today_bounds(&slot, field)
            } else {
                None
            };
            assert_eq!(slot.bounds(field), bounds, "{kind:?} {field:?}");
        }
    }
}

/// The field the track page's `adjust_mod` and the Roto's knobs set for a setting under a type,
/// from a value within its bounds in [`ModSlot::field`]'s units.
fn today_set(slot: &mut ModSlot, field: ModField, value: i32) {
    let kind = slot.kind;
    let ahd_kind = kind == ModKind::Ahd;
    let lpg_kind = kind == ModKind::Lpg;
    let drum_kind = kind == ModKind::Drum;
    let (ms, byte, signed, at) = (value as u16, value as u8, value as i16, value as usize);
    match field {
        ModField::Type => slot.kind = ModKind::ALL[at],
        ModField::Mode => match kind {
            ModKind::Adsr => slot.adsr.curve = Curve::ALL[at],
            ModKind::Ahd => slot.ahd.curve = Curve::ALL[at],
            ModKind::Drum => slot.drum.curve = Curve::ALL[at],
            ModKind::Lfo => slot.lfo.mode = LfoMode::ALL[at],
            ModKind::Steps => slot.steps.mode = StepsMode::ALL[at],
            ModKind::Track => slot.track.source = Source::ALL[at],
            ModKind::Lpg => {}
        },
        ModField::Polarity => slot.polarity = Some(Polarity::ALL[at]),
        ModField::Depth => slot.depth = signed,
        ModField::Attack if ahd_kind => slot.ahd.attack_ms = ms,
        ModField::Attack if lpg_kind => slot.lpg.attack_ms = ms,
        ModField::Attack => slot.adsr.attack_ms = ms,
        ModField::Decay if ahd_kind => slot.ahd.decay_ms = ms,
        ModField::Decay if drum_kind => slot.drum.decay_ms = ms,
        ModField::Decay if lpg_kind => slot.lpg.decay_ms = ms,
        ModField::Decay => slot.adsr.decay_ms = ms,
        ModField::Reset if ahd_kind => slot.ahd.reset = value != 0,
        ModField::Reset if lpg_kind => slot.lpg.reset = value != 0,
        ModField::Reset => slot.adsr.reset = value != 0,
        ModField::Colour => slot.lpg.colour = byte,
        ModField::Peak => slot.drum.peak_ms = ms,
        ModField::Body => slot.drum.body = byte,
        ModField::Ratchet => slot.drum.ratchet = byte,
        ModField::Hold if value < 0 => slot.ahd.hold = Hold::Note,
        ModField::Hold => slot.ahd.hold = Hold::Ms(ms),
        ModField::Release => slot.adsr.release_ms = ms,
        ModField::Sustain => slot.adsr.sustain = byte,
        ModField::Shape => slot.lfo.shape = Shape::ALL[at],
        ModField::Phase => slot.lfo.phase = ms,
        ModField::Deform => slot.lfo.deform = signed,
        ModField::Fade => slot.lfo.fade = Fade::ALL[at],
        ModField::Length => slot.steps.length = byte,
        ModField::Start => slot.steps.start = byte,
        ModField::Loop => slot.steps.looping = Loop::ALL[at],
        ModField::Mutate => slot.steps.mutate = byte,
        ModField::Smooth => slot.steps.smooth = signed,
        ModField::Step(step) => slot.steps.values[usize::from(step)] = signed,
        ModField::Low => slot.track.bounds_mut().low = byte,
        ModField::High => slot.track.bounds_mut().high = byte,
        ModField::Rate | ModField::Destination => {}
    }
}

#[test]
fn setting_a_field_sets_the_struct_field_its_type_keeps_it_in_and_nothing_else() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        for field in FIELDS {
            let Some((lowest, highest)) = slot.bounds(field) else {
                let mut set = slot;
                set.set_field(field, 1);
                assert_eq!(set, slot, "{kind:?} {field:?} is left alone");
                continue;
            };
            for value in [lowest, highest, (lowest + highest) / 2] {
                let (mut set, mut today) = (slot, slot);
                set.set_field(field, value);
                today_set(&mut today, field, value);
                assert_eq!(set, today, "{kind:?} {field:?} {value}");
                assert_eq!(set.field(field), Some(value), "{kind:?} {field:?} {value}");
            }
            // Past either end, it stops there, and a phase goes round.
            let mut set = slot;
            set.set_field(field, highest + 25);
            let past = if field == ModField::Phase {
                24
            } else {
                highest
            };
            assert_eq!(set.field(field), Some(past), "{kind:?} {field:?}");
            set.set_field(field, lowest - 25);
            let past = if field == ModField::Phase {
                335
            } else {
                lowest
            };
            assert_eq!(set.field(field), Some(past), "{kind:?} {field:?}");
        }
    }
}

#[test]
fn an_ahds_hold_for_the_note_is_below_no_time() {
    let mut slot = distinct(ModKind::Ahd);
    slot.set_field(ModField::Hold, -1);
    assert_eq!(slot.ahd.hold, Hold::Note);
    slot.set_field(ModField::Hold, 0);
    assert_eq!(slot.ahd.hold, Hold::Ms(0));
}

#[test]
fn the_registrys_own_settings_and_locked_ones_are_in_the_fields_order() {
    for (at, field) in MOD_OWN_FIELDS.into_iter().enumerate() {
        let id = SettingId::mod_own(2, at).unwrap();
        assert_eq!(id.entry().name, field.name(), "{at}");
        assert_eq!(field.own(), Some(at));
        assert_eq!(field.setting(2), Some(id));
    }
    assert_eq!(SettingId::mod_own(0, MOD_OWN_SETTINGS), None);
    for (at, field) in MOD_LOCKED_FIELDS.into_iter().enumerate() {
        let id = SettingId::modulator(4, at).unwrap();
        assert_eq!(id.entry().name, field.name(), "{at}");
        assert_eq!(field.locked(), Some(at));
        assert_eq!(field.setting(4), Some(id));
    }
    assert_eq!(SettingId::modulator(0, MOD_LOCKED_FIELDS.len()), None);
    // The places the track page's `own_place` gave them.
    let own_place = [
        (ModField::Attack, 0),
        (ModField::Decay, 1),
        (ModField::Sustain, 2),
        (ModField::Release, 3),
        (ModField::Body, 4),
        (ModField::Colour, 5),
        (ModField::Length, 6),
        (ModField::Mutate, 7),
        (ModField::Smooth, 8),
        (ModField::Rate, 9),
    ];
    for (field, at) in own_place {
        assert_eq!(field.own(), Some(at), "{field:?}");
    }
    assert_eq!(ModField::Hold.own(), None);
    assert_eq!(ModField::Hold.setting(0), None);
}

/// `ModSlot::own_value` as it was, before it read [`ModSlot::field`].
fn today_own_value(slot: &ModSlot, at: usize) -> Option<u16> {
    let whole = |value: u8| u16::from(value) << 8;
    let kind = slot.kind;
    Some(match at {
        0 => time_value(match kind {
            ModKind::Adsr => slot.adsr.attack_ms,
            ModKind::Ahd => slot.ahd.attack_ms,
            ModKind::Lpg => slot.lpg.attack_ms,
            _ => return None,
        }),
        1 => time_value(match kind {
            ModKind::Adsr => slot.adsr.decay_ms,
            ModKind::Ahd => slot.ahd.decay_ms,
            ModKind::Drum => slot.drum.decay_ms,
            ModKind::Lpg => slot.lpg.decay_ms,
            _ => return None,
        }),
        2 if kind == ModKind::Adsr => whole(slot.adsr.sustain),
        3 if kind == ModKind::Adsr => time_value(slot.adsr.release_ms),
        4 if kind == ModKind::Drum => whole(slot.drum.body),
        5 if kind == ModKind::Lpg => whole(slot.lpg.colour),
        6 if kind == ModKind::Steps => whole(slot.steps.length.saturating_sub(1)),
        7 if kind == ModKind::Steps => whole(slot.steps.mutate),
        8 if kind == ModKind::Steps => depth_value(slot.steps.smooth),
        9 if kind == ModKind::Lfo => slot.lfo.rate.place(),
        9 if kind == ModKind::Steps => slot.steps.rate.place()?,
        _ => return None,
    })
}

/// `ModSlot::set_own` as it was, before it went through [`ModSlot::set_field`].
fn today_set_own(slot: &mut ModSlot, at: usize, value: f32) {
    let whole = |top: u8| value.round().clamp(0.0, f32::from(top)) as u8;
    let most = f32::from(MAX_STEP);
    match (at, slot.kind) {
        (0, ModKind::Adsr) => slot.adsr.attack_ms = time_at(value),
        (0, ModKind::Ahd) => slot.ahd.attack_ms = time_at(value),
        (0, ModKind::Lpg) => slot.lpg.attack_ms = time_at(value),
        (1, ModKind::Adsr) => slot.adsr.decay_ms = time_at(value),
        (1, ModKind::Ahd) => slot.ahd.decay_ms = time_at(value),
        (1, ModKind::Drum) => slot.drum.decay_ms = time_at(value),
        (1, ModKind::Lpg) => slot.lpg.decay_ms = time_at(value),
        (2, ModKind::Adsr) => slot.adsr.sustain = whole(100),
        (3, ModKind::Adsr) => slot.adsr.release_ms = time_at(value),
        (4, ModKind::Drum) => slot.drum.body = whole(100),
        (5, ModKind::Lpg) => slot.lpg.colour = whole(0xFF),
        (6, ModKind::Steps) => slot.steps.length = whole(STEPS as u8 - 1) + 1,
        (7, ModKind::Steps) => slot.steps.mutate = whole(MAX_MUTATE),
        (8, ModKind::Steps) => {
            slot.steps.smooth = depth_of(value).round().clamp(-most, most) as i16;
        }
        (9, ModKind::Lfo) => slot.lfo.rate = slot.lfo.rate.placed(value),
        (9, ModKind::Steps) => slot.steps.rate = slot.steps.rate.placed(value),
        _ => {}
    }
}

#[test]
fn own_values_read_and_set_exactly_as_before() {
    let values = [
        -40.0,
        -0.4,
        0.0,
        0.3,
        0.5,
        0.99,
        1.0,
        1.5,
        7.25,
        63.5,
        100.0,
        100.4,
        127.9,
        128.0,
        200.6,
        254.5,
        255.0,
        255.9,
        300.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut slots: Vec<ModSlot> = ModKind::ALL.into_iter().map(distinct).collect();
    // And the slots a new track has, and one at every end.
    slots.extend(vt_model::modulation::default_mods());
    let mut ends = distinct(ModKind::Steps);
    ends.steps.length = 0;
    ends.steps.rate = StepRate::Pass;
    slots.push(ends);
    for kind in ModKind::ALL {
        let mut slot = slots[0];
        slot.kind = kind;
        slots.push(slot);
    }
    for slot in slots {
        for at in 0..=MOD_OWN_SETTINGS {
            assert_eq!(
                slot.own_value(at),
                today_own_value(&slot, at),
                "{:?} {at}",
                slot.kind
            );
            for value in values {
                let (mut set, mut today) = (slot, slot);
                set.set_own(at, value);
                today_set_own(&mut today, at, value);
                assert_eq!(set, today, "{:?} {at} {value}", slot.kind);
            }
            for coarse in 0..=255_u16 {
                let value = f32::from(coarse << 8 | 0x5A) / 256.0;
                let (mut set, mut today) = (slot, slot);
                set.set_own(at, value);
                today_set_own(&mut today, at, value);
                assert_eq!(set, today, "{:?} {at} {value}", slot.kind);
            }
        }
    }
}

/// The track page's and the Roto's match from a control to its field, as they were.
fn today_control(track: &Track, control: TrackControl) -> u8 {
    match control {
        TrackControl::Cutoff => track.cutoff,
        TrackControl::Resonance => track.resonance,
        TrackControl::Pan => track.pan,
        TrackControl::SendA => track.send_a,
        TrackControl::SendB => track.send_b,
        TrackControl::Drive => track.drive,
        TrackControl::Punch => track.punch,
        TrackControl::LowCut => track.low_cut,
        TrackControl::HighCut => track.high_cut,
        TrackControl::Pitch => track.pitch,
        TrackControl::VelocityDelta => track.velocity_delta,
        TrackControl::Quantise => track.quantise,
    }
}

#[test]
fn a_tracks_controls_read_and_set_their_own_fields() {
    let mut track = Track::default();
    for (at, control) in TrackControl::ALL.into_iter().enumerate() {
        track.set_control(control, 0x10 + at as u8);
    }
    for (at, control) in TrackControl::ALL.into_iter().enumerate() {
        assert_eq!(track.control(control), 0x10 + at as u8, "{control:?}");
        assert_eq!(track.control(control), today_control(&track, control));
    }
    for control in TrackControl::ALL {
        let mut set = track.clone();
        set.set_control(control, 0xFF);
        let top = match control {
            TrackControl::SendA | TrackControl::SendB => MAX_LEVEL,
            _ => 0xFF,
        };
        assert_eq!(today_control(&set, control), top, "{control:?}");
        for other in TrackControl::ALL {
            if other != control {
                assert_eq!(set.control(other), track.control(other), "{control:?}");
            }
        }
        // Nothing else on the track moved.
        set.set_control(control, track.control(control));
        assert_eq!(set, track, "{control:?}");
    }
}
