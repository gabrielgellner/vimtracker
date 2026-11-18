//! A mod slot's settings read and set by what they are, wherever its type keeps them, each type's
//! spec, and the track's own controls (#726). The track page and the Roto read these rather than
//! choosing the struct field themselves.

use vt_model::action::TrackControl;
use vt_model::constants::MAX_LEVEL;
use vt_model::envelope::{Adsr, Ahd, Curve, Drum, Hold, Lpg};
use vt_model::lfo::{Fade, Lfo, LfoMode, Rate, Shape};
use vt_model::modulation::{
    MOD_LOCKED_FIELDS, MOD_OWN_FIELDS, MOD_SPECS, ModField, ModKind, ModSlot, Polarity,
};
use vt_model::param::Reading;
use vt_model::settings::{MOD_OWN_SETTINGS, SettingId};
use vt_model::song::Track;
use vt_model::steps::{Loop, StepRate, Steps, StepsMode};
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

#[test]
fn each_types_spec_is_pinned() {
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
fn each_types_title_is_in_order_and_its_grid_places_each_setting() {
    use ModField::*;
    for kind in ModKind::ALL {
        let spec = kind.spec();
        let title: Vec<ModField> = [
            Type,
            Mode,
            Loop,
            From,
            Jump,
            Spread,
            Bias,
            Polarity,
            Destination,
        ]
        .into_iter()
        .filter(|field| spec.title.contains(field))
        .collect();
        assert_eq!(spec.title, title, "{kind:?}");
        assert_eq!(spec.title.first(), Some(&Type), "{kind:?}");
        assert_eq!(spec.title.last(), Some(&Destination), "{kind:?}");
        assert!(spec.fields.contains(&Depth), "{kind:?}");
        let mut placed: Vec<usize> = spec.grid.settings().collect();
        // Steps' values are on every row, so `l` from any of its grid goes in.
        placed.sort_unstable();
        placed.dedup();
        assert_eq!(
            placed,
            (0..spec.fields.len()).collect::<Vec<_>>(),
            "{kind:?}"
        );
    }
}

/// Where a value is in a list.
fn place<T: PartialEq>(items: &[T], value: T) -> i32 {
    items.iter().position(|item| *item == value).unwrap() as i32
}

/// How a setting that more than one type has is kept by each of them: read, then set from a value
/// in [`ModSlot::field`]'s units.
type Kept = (
    ModKind,
    ModField,
    fn(&ModSlot) -> i32,
    fn(&mut ModSlot, i32),
);

const KEPT: [Kept; 18] = [
    (
        ModKind::Adsr,
        ModField::Attack,
        |slot| i32::from(slot.adsr.attack_ms),
        |slot, at| slot.adsr.attack_ms = at as u16,
    ),
    (
        ModKind::Ahd,
        ModField::Attack,
        |slot| i32::from(slot.ahd.attack_ms),
        |slot, at| slot.ahd.attack_ms = at as u16,
    ),
    (
        ModKind::Lpg,
        ModField::Attack,
        |slot| i32::from(slot.lpg.attack_ms),
        |slot, at| slot.lpg.attack_ms = at as u16,
    ),
    (
        ModKind::Adsr,
        ModField::Decay,
        |slot| i32::from(slot.adsr.decay_ms),
        |slot, at| slot.adsr.decay_ms = at as u16,
    ),
    (
        ModKind::Ahd,
        ModField::Decay,
        |slot| i32::from(slot.ahd.decay_ms),
        |slot, at| slot.ahd.decay_ms = at as u16,
    ),
    (
        ModKind::Drum,
        ModField::Decay,
        |slot| i32::from(slot.drum.decay_ms),
        |slot, at| slot.drum.decay_ms = at as u16,
    ),
    (
        ModKind::Lpg,
        ModField::Decay,
        |slot| i32::from(slot.lpg.decay_ms),
        |slot, at| slot.lpg.decay_ms = at as u16,
    ),
    (
        ModKind::Adsr,
        ModField::Reset,
        |slot| i32::from(slot.adsr.reset),
        |slot, at| slot.adsr.reset = at != 0,
    ),
    (
        ModKind::Ahd,
        ModField::Reset,
        |slot| i32::from(slot.ahd.reset),
        |slot, at| slot.ahd.reset = at != 0,
    ),
    (
        ModKind::Lpg,
        ModField::Reset,
        |slot| i32::from(slot.lpg.reset),
        |slot, at| slot.lpg.reset = at != 0,
    ),
    (
        ModKind::Adsr,
        ModField::Mode,
        |slot| place(&Curve::ALL, slot.adsr.curve),
        |slot, at| slot.adsr.curve = Curve::ALL[at as usize],
    ),
    (
        ModKind::Ahd,
        ModField::Mode,
        |slot| place(&Curve::ALL, slot.ahd.curve),
        |slot, at| slot.ahd.curve = Curve::ALL[at as usize],
    ),
    (
        ModKind::Drum,
        ModField::Mode,
        |slot| place(&Curve::ALL, slot.drum.curve),
        |slot, at| slot.drum.curve = Curve::ALL[at as usize],
    ),
    (
        ModKind::Lfo,
        ModField::Mode,
        |slot| place(&LfoMode::ALL, slot.lfo.mode),
        |slot, at| slot.lfo.mode = LfoMode::ALL[at as usize],
    ),
    (
        ModKind::Steps,
        ModField::Mode,
        |slot| place(&StepsMode::ALL, slot.steps.mode),
        |slot, at| slot.steps.mode = StepsMode::ALL[at as usize],
    ),
    (
        ModKind::Track,
        ModField::Mode,
        |slot| place(&Source::ALL, slot.track.source),
        |slot, at| slot.track.source = Source::ALL[at as usize],
    ),
    // Its map is the source's it follows, a note's in `distinct`.
    (
        ModKind::Track,
        ModField::Low,
        |slot| i32::from(slot.track.note.low),
        |slot, at| slot.track.note.low = at as u8,
    ),
    (
        ModKind::Track,
        ModField::High,
        |slot| i32::from(slot.track.note.high),
        |slot, at| slot.track.note.high = at as u8,
    ),
];

#[test]
fn a_setting_two_types_have_is_read_and_set_where_each_keeps_it() {
    for (kind, field, read, keep) in KEPT {
        let slot = distinct(kind);
        assert_eq!(slot.field(field), Some(read(&slot)), "{kind:?} {field:?}");
        let (lowest, highest) = slot.bounds(field).unwrap();
        for to in [lowest, highest] {
            let (mut set, mut kept) = (slot, slot);
            set.set_field(field, to);
            keep(&mut kept, to);
            assert_eq!(set, kept, "{kind:?} {field:?} {to}");
        }
    }
}

#[test]
fn a_slot_has_only_its_types_settings_and_setting_one_moves_only_it() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        for field in FIELDS {
            let Some((lowest, highest)) = slot.bounds(field) else {
                assert_eq!(slot.field(field), None, "{kind:?} {field:?}");
                let mut set = slot;
                set.set_field(field, 1);
                assert_eq!(set, slot, "{kind:?} {field:?} is left alone");
                continue;
            };
            assert!(field.on(kind), "{kind:?} {field:?}");
            for value in [lowest, highest, (lowest + highest) / 2] {
                let mut set = slot;
                set.set_field(field, value);
                assert_eq!(set.field(field), Some(value), "{kind:?} {field:?} {value}");
                // A type is what the rest mean, and Track's source is which map its ends are.
                let follows = |other: ModField| {
                    field == ModField::Type
                        || (kind == ModKind::Track
                            && field == ModField::Mode
                            && matches!(other, ModField::Low | ModField::High))
                };
                for other in FIELDS.into_iter().filter(|other| *other != field) {
                    if !follows(other) {
                        assert_eq!(set.field(other), slot.field(other), "{kind:?} {field:?}");
                    }
                }
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
fn a_types_modes_are_its_lists_in_the_order_its_mode_counts_them() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        let modes = kind.modes();
        assert_eq!(modes.is_empty(), !kind.has_mode(), "{kind:?}");
        if let Some((lowest, highest)) = slot.bounds(ModField::Mode) {
            assert_eq!((lowest, highest), (0, modes.len() as i32 - 1), "{kind:?}");
        }
    }
    let names =
        |kind: ModKind| -> Vec<&str> { kind.modes().into_iter().map(|(name, _)| name).collect() };
    assert_eq!(names(ModKind::Ahd), Curve::ALL.map(Curve::name));
    assert_eq!(names(ModKind::Lfo), LfoMode::ALL.map(LfoMode::name));
    assert_eq!(names(ModKind::Steps), StepsMode::ALL.map(StepsMode::name));
    assert_eq!(names(ModKind::Track), Source::ALL.map(Source::name));
    assert_eq!(ModKind::Track.modes()[0].1, Source::ALL[0].meaning());
}

#[test]
fn each_setting_a_type_has_shows_as_text_and_a_lists_choices_are_its_values() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        for field in FIELDS.into_iter().filter(|field| field.on(kind)) {
            let text = slot.text(field);
            let listed = slot.reading(field) == Reading::Listed;
            match field {
                // Where it goes is its track's to name and list.
                ModField::Destination => assert_eq!(text, None),
                ModField::Step(16) => assert_eq!(text, None, "past the last"),
                _ => assert!(
                    text.is_some_and(|text| !text.is_empty()),
                    "{kind:?} {field:?}"
                ),
            }
            let choices = slot.choices(field);
            if listed && field != ModField::Destination {
                let (lowest, highest) = slot.bounds(field).unwrap();
                assert_eq!(
                    (lowest, highest),
                    (0, choices.len() as i32 - 1),
                    "{field:?}"
                );
                let at = slot.field(field).unwrap() as usize;
                assert_eq!(slot.text(field).as_deref(), Some(&*choices[at].0));
            } else {
                assert!(choices.is_empty(), "{kind:?} {field:?}");
            }
        }
    }
    let steps = distinct(ModKind::Steps);
    assert_eq!(
        steps.text(ModField::Rate).as_deref(),
        Some("--"),
        "a note each"
    );
    assert_eq!(steps.text(ModField::Start).as_deref(), Some("4"), "from 1");
    let mut lfo = distinct(ModKind::Lfo);
    lfo.lfo.phase = 120;
    lfo.lfo.shape = Shape::Lorenz;
    assert_eq!(lfo.text(ModField::Phase).as_deref(), Some("y"));
    lfo.lfo.shape = Shape::Sine;
    assert_eq!(lfo.text(ModField::Phase).as_deref(), Some("120°"));
    let ahd = distinct(ModKind::Ahd);
    assert_eq!(ahd.text(ModField::Depth).as_deref(), Some("-42"));
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
        // A mode is each type's own, named as its type names it.
        let kinds: Vec<ModKind> = ModKind::ALL
            .into_iter()
            .filter(|kind| field.on(*kind) && field.own(*kind) == Some(at))
            .collect();
        assert!(!kinds.is_empty(), "{at} is no type's");
        for kind in kinds {
            let name = match field {
                ModField::Mode => kind.mode_label(),
                _ => field.name(),
            };
            assert_eq!(id.entry().name, name, "{at}");
            assert_eq!(field.setting(2, kind), Some(id));
        }
    }
    assert_eq!(SettingId::mod_own(0, MOD_OWN_SETTINGS), None);
    for (at, field) in MOD_LOCKED_FIELDS.into_iter().enumerate() {
        let id = SettingId::modulator(4, at).unwrap();
        assert_eq!(id.entry().name, field.name(), "{at}");
        assert_eq!(field.locked(), Some(at));
        assert_eq!(field.setting(4, ModKind::Lfo), Some(id));
    }
    assert_eq!(SettingId::modulator(0, MOD_LOCKED_FIELDS.len()), None);
    // Track's source and a gate's want no list of their own.
    assert_eq!(ModField::Mode.own(ModKind::Track), None);
    assert_eq!(ModField::Mode.setting(0, ModKind::Lpg), None);
    assert_eq!(ModField::Start.setting(0, ModKind::Steps), None);
}

/// A type's own lists, its hold, peak, fade and ratchet are on the registry's scale and steps lock
/// them, each by a label with the slot's number after it; they go there and back exactly, and
/// each setting's default is a new slot's (decision 399).
#[test]
fn the_lists_and_odd_times_are_lockable_own_settings_that_go_there_and_back() {
    let own = |kind: ModKind, field: ModField| SettingId::mod_own(3, field.own(kind).unwrap());
    let labels = [
        (ModKind::Lfo, ModField::Shape, "SHP4"),
        (ModKind::Steps, ModField::Loop, "LOOP4"),
        (ModKind::Steps, ModField::From, "FROM4"),
        (ModKind::Adsr, ModField::Mode, "CRV4"),
        (ModKind::Ahd, ModField::Mode, "CRV4"),
        (ModKind::Drum, ModField::Mode, "CRV4"),
        (ModKind::Lfo, ModField::Mode, "LMOD4"),
        (ModKind::Steps, ModField::Mode, "SMOD4"),
        (ModKind::Ahd, ModField::Hold, "HOLD4"),
        (ModKind::Drum, ModField::Peak, "PEAK4"),
        (ModKind::Lfo, ModField::Fade, "FADE4"),
        (ModKind::Drum, ModField::Ratchet, "RTCH4"),
    ];
    for (kind, field, label) in labels {
        let id = own(kind, field).unwrap();
        assert_eq!(id.label().as_deref(), Some(label), "{kind:?} {field:?}");
        assert!(id.lockable());
        // Every value of the setting goes to the scale and comes back as it was.
        let slot = distinct(kind);
        let at = field.own(kind).unwrap();
        let (low, high) = slot.bounds(field).unwrap();
        // An envelope's time comes back exactly up to a second (decision 283).
        let high = if field == ModField::Peak { 1000 } else { high };
        for value in low..=high {
            let mut set = slot;
            set.set_field(field, value);
            let mut back = slot;
            back.set_own(at, f32::from(set.own_value(at).unwrap()) / 256.0);
            assert_eq!(back.field(field), Some(value), "{kind:?} {field:?} {value}");
        }
        // A new slot of the type is where the entry starts.
        let fresh = ModSlot {
            kind,
            ..vt_model::modulation::default_mod(3)
        };
        assert_eq!(
            fresh.own_value(at).map(|value| (value + 0x80) >> 8),
            Some(u16::from(id.entry().default)),
            "{kind:?} {field:?}"
        );
    }
    // Each list's names are its own, in its order.
    let shown = |kind: ModKind, field: ModField, value: u16| {
        own(kind, field).unwrap().entry().kind.shown(value)
    };
    assert_eq!(shown(ModKind::Lfo, ModField::Shape, 0x0400), "Square");
    assert_eq!(shown(ModKind::Steps, ModField::Loop, 0x0200), "Bounce");
    assert_eq!(shown(ModKind::Steps, ModField::From, 0x0400), "5");
    assert_eq!(shown(ModKind::Lfo, ModField::Fade, 0x1600), "Off");
    assert_eq!(shown(ModKind::Lfo, ModField::Fade, 0x2300), "+1/4");
    assert_eq!(shown(ModKind::Ahd, ModField::Hold, 0xFF00), "Note");
    assert_eq!(shown(ModKind::Ahd, ModField::Hold, 0xFE00), "2000 ms");
    assert_eq!(shown(ModKind::Ahd, ModField::Hold, 0x0000), "0 ms");
    assert_eq!(shown(ModKind::Drum, ModField::Ratchet, 0x0300), "4");
    let choices = |field: ModField, kind: ModKind| -> Vec<String> {
        distinct(kind)
            .choices(field)
            .into_iter()
            .map(|(name, _)| name.into_owned())
            .collect()
    };
    for (kind, field) in [
        (ModKind::Lfo, ModField::Shape),
        (ModKind::Steps, ModField::Loop),
        (ModKind::Adsr, ModField::Mode),
        (ModKind::Lfo, ModField::Mode),
        (ModKind::Steps, ModField::Mode),
    ] {
        assert_eq!(
            own(kind, field).unwrap().entry().choices(),
            choices(field, kind),
            "{kind:?} {field:?}"
        );
    }
    let fades: Vec<String> = vt_model::lfo::Fade::ALL
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        own(ModKind::Lfo, ModField::Fade).unwrap().entry().choices(),
        fades
    );
    // One type's mode isn't another's: an LFO has no curve, and an envelope no LFO mode.
    let lfo = distinct(ModKind::Lfo);
    let curve = ModField::Mode.own(ModKind::Adsr).unwrap();
    assert_eq!(lfo.own_value(curve), None);
    let mut moved = lfo;
    moved.set_own(curve, 3.0);
    assert_eq!(moved, lfo);
}

/// Modulation sets a slot's own settings from past either end of the `00` to `FF` scale, as it
/// adds its swing to the value held, and each stops at that end. A type's own settings it hasn't
/// are left alone.
#[test]
fn own_values_set_from_past_the_scales_ends_stop_there() {
    for kind in ModKind::ALL {
        let slot = distinct(kind);
        for at in 0..=MOD_OWN_SETTINGS {
            let set = |value: f32| {
                let mut set = slot;
                set.set_own(at, value);
                set
            };
            if slot.own_value(at).is_none() {
                assert_eq!(set(100.0), slot, "{kind:?} {at}");
                continue;
            }
            for below in [-0.4, -40.0, f32::NEG_INFINITY] {
                assert_eq!(set(below), set(0.0), "{kind:?} {at} {below}");
            }
            for above in [255.4, 300.0, f32::INFINITY] {
                assert_eq!(set(above), set(255.0), "{kind:?} {at} {above}");
            }
        }
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
    }
    assert_eq!(track.cutoff, track.control(TrackControl::Cutoff));
    assert_eq!(track.send_b, track.control(TrackControl::SendB));
    assert_eq!(track.quantise, track.control(TrackControl::Quantise));
    for control in TrackControl::ALL {
        let mut set = track.clone();
        set.set_control(control, 0xFF);
        let top = match control {
            TrackControl::SendA | TrackControl::SendB => MAX_LEVEL,
            TrackControl::Swing => vt_model::groove::MOST_SWING,
            _ => 0xFF,
        };
        assert_eq!(set.control(control), top, "{control:?}");
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
