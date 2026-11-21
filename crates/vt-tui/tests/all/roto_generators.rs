//! Live generators on the Roto-Control: a device each after the Mod page's, laid out by its
//! fill, every setting the app doesn't choose on one knob, a knob's turn reshaping the column
//! as the Gen page's keys do.

use std::sync::Arc;

use vt_model::action::{Action, edit};
use vt_model::generator::{Generator, Span};
use vt_model::scale::Scale;
use vt_model::shape::{FillKind, GenParam, Role, Shape, ShapeColumn, Where, World};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_roto::live::{Feel, KNOB_TOP};
use vt_tui::controls::{Control, Page};
use vt_tui::roto::devices::{control_at, knob, live_devices, placement, track_devices};
use vt_tui::roto::edits::turned;
use vt_tui::roto::layouts::{GENERATORS, WHERE_FIRST, WHERE_SECOND, app_only, layout_of, resolve};

fn shape(kind: FillKind, place: Where) -> Shape {
    Shape {
        from: 0x30,
        to: 0x48,
        kind,
        place,
        seed: 1,
        ..Shape::default()
    }
}

/// The Note column, re-pitching the notes there.
fn note() -> ShapeColumn {
    ShapeColumn::Note {
        scale: Scale::default(),
        writes: None,
    }
}

/// A song whose first track's phrase `00` has 16 notes, live with this shape on its Note column.
fn song_with(shape: Shape) -> Song {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16)
        .map(|_| "C-3 40 --- --- ---".parse().unwrap())
        .collect();
    let mut phrase = Phrase::from_steps(steps);
    let generator =
        Generator::new(note(), Span::Rows { first: 0, last: 15 }, shape).with_list(&[0x30, 0x40]);
    assert!(phrase.add_generator(generator, None));
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song
}

#[test]
fn every_generator_puts_each_setting_on_one_knob_and_the_app_keeps_what_it_is() {
    let mut used = std::collections::BTreeSet::new();
    let wheres = [
        Where::Notes,
        Where::Every(4),
        Where::Chance(50),
        Where::Euclid { hits: 3, rotate: 0 },
        Where::Clusters(50),
        Where::Markov(80),
        Where::States(25),
        Where::Complementary {
            share: 50,
            second: true,
        },
        Where::Rhythm {
            rhythm: World::Bembe,
            part: Role::High,
            child: 70,
        },
    ];
    for kind in FillKind::ALL {
        for place in wheres {
            let mut shape = shape(kind, place);
            // A `Follow` mutating, as a Bloom branch does, has its kinds of mutation and `Vary`.
            shape.follow.mutation = 2;
            let song = song_with(shape);
            let track = &song.tracks[0];
            let devices = track_devices(track, Some(0));
            // `Phrase` comes last.
            let (page, _, _) = devices.iter().rev().nth(1).unwrap();
            assert_eq!(*page, Page::Macros, "{}", kind.name());
            // The generator's device, and an `Acid`'s `Loops` after it.
            let generators = devices.iter().filter(|(page, _, spec)| {
                *page == Page::Macros && !matches!(spec.grid[0][0], Some(Control::Macro(_)))
            });
            for (_, _, spec) in generators {
                let layout = layout_of(spec, track);
                used.insert(layout.kind);
                let wanted: Vec<Control> = spec
                    .rows()
                    .flatten()
                    .flatten()
                    .filter(|control| !app_only(*control, track))
                    .collect();
                let shown: Vec<Control> = placement(spec, track, false)
                    .knobs
                    .iter()
                    .flatten()
                    .map(|(control, _)| *control)
                    .collect();
                for control in &wanted {
                    let times = shown.iter().filter(|found| *found == control).count();
                    assert_eq!(times, 1, "{control:?} on {}", kind.name());
                    assert!(knob(*control, track, &song).is_some(), "{control:?}");
                }
                assert_eq!(shown.len(), wanted.len(), "{} {place:?}", kind.name());
                // A key the device hasn't is the `Where`'s, which comes and goes with it, or a setting
                // the shape hasn't now, as `Density` where nothing is generated.
                let shape = track.phrases[&0].generators()[0].shape;
                for setting in layout.layout.settings() {
                    if resolve(setting, spec, track).is_none() {
                        let absent = GenParam::by_key(setting.key)
                            .is_some_and(|param| !param.exists_for(&shape, note(), false));
                        assert!(
                            absent || [WHERE_FIRST, WHERE_SECOND].contains(&setting.key),
                            "{} has no {}",
                            kind.name(),
                            setting.key
                        );
                    }
                }
                // Live, as a generator has no buttons.
                assert!(!layout.stored);
            }
        }
    }
    // An FX column's lane's layout is checked on its own, below.
    used.insert("gen/lane");
    let all: std::collections::BTreeSet<&str> =
        GENERATORS.iter().map(|layout| layout.kind).collect();
    assert_eq!(
        used, all,
        "every generator layout is some fill's, or a lane's"
    );
}

#[test]
fn an_fx_columns_lane_is_a_device_of_its_own_with_its_length_and_rate_on_knobs() {
    use vt_tui::controls::LaneField;
    let mut song = song_with(shape(FillKind::Ramp, Where::Notes));
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(0, Some(vt_model::song::Lane::new(0, 4)));
    let track = &song.tracks[0];
    let devices = live_devices(track, Some(0), &song, false);
    let device = devices.iter().rev().nth(1).unwrap();
    assert_eq!(device.name.to_string(), "FX1 Lane");
    assert!(!device.stored, "live, as it has no buttons");
    let listed = track_devices(track, Some(0));
    let spec = &listed.iter().rev().nth(1).unwrap().2;
    assert_eq!(layout_of(spec, track).kind, "gen/lane");
    let knobs: Vec<Control> = placement(spec, track, false)
        .knobs
        .iter()
        .flatten()
        .map(|(control, _)| *control)
        .collect();
    assert_eq!(
        knobs,
        [
            Control::Lane(0, LaneField::Length),
            Control::Lane(0, LaneField::Rate),
            Control::Lane(0, LaneField::Start),
            Control::Lane(0, LaneField::Jump),
            Control::Lane(0, LaneField::Hold),
            Control::Lane(0, LaneField::Order),
            Control::Lane(0, LaneField::Reverse)
        ]
    );
    // Turning `Length` to its top is the most rows it adds, as the page's `=` would.
    let length = knobs[0];
    let action = turned(length, 0, &song, KNOB_TOP).unwrap();
    let next = edit(&Arc::new(song.clone()), &action).unwrap();
    assert_eq!(next.tracks[0].lanes[0].length, 0xFF);
    assert!(knob(knobs[1], track, &song).is_some(), "Rate's list");
}

#[test]
fn a_generator_comes_after_the_mod_pages_devices_named_by_its_column_and_fill() {
    let song = song_with(shape(FillKind::Ramp, Where::Euclid { hits: 3, rotate: 0 }));
    let track = &song.tracks[0];
    let without = live_devices(track, None, &song, false);
    let with = live_devices(track, Some(0), &song, false);
    assert_eq!(
        with.len(),
        without.len() + 2,
        "one device, then Phrase, and no Empty"
    );
    let device = with.iter().rev().nth(1).unwrap();
    assert_eq!(device.name.to_string(), "Note Ramp");
    assert!(!device.stored);
}

#[test]
fn turning_a_knob_reshapes_the_column_as_the_pages_keys_do() {
    let song = song_with(shape(FillKind::Ramp, Where::Euclid { hits: 3, rotate: 0 }));
    let track = &song.tracks[0];
    let device = track_devices(track, Some(0)).len() - 2;
    // Knob 7 is the `Where`'s first: Euclid's hits, 0 to the 16 rows.
    let hits = control_at(track, Some(0), device, 7, false).unwrap();
    assert!(matches!(
        hits,
        Control::Gen {
            param: GenParam::Hits,
            ..
        }
    ));
    let action = turned(hits, 0, &song, KNOB_TOP).unwrap();
    assert!(matches!(action, Action::Reshape { .. }));
    let next = edit(&Arc::new(song.clone()), &action).unwrap();
    let shape = next.tracks[0].phrases[&0].generators()[0].shape;
    assert_eq!(
        shape.place,
        Where::Euclid {
            hits: 16,
            rotate: 0
        }
    );
    // Where it already is, nothing.
    let at = knob(hits, &next.tracks[0], &next).unwrap().position;
    assert!(turned(hits, 0, &next, at).is_none());
    // A bend has a notch at its middle.
    let curve = control_at(track, Some(0), device, 3, false).unwrap();
    assert_eq!(knob(curve, track, &song).unwrap().feel, Feel::CentreNotch);
}

#[test]
fn knob_7_turns_the_amount_of_marbles_rhythms_and_the_app_keeps_complementarys_side() {
    for place in [
        Where::Clusters(50),
        Where::Markov(80),
        Where::States(25),
        Where::Complementary {
            share: 50,
            second: true,
        },
    ] {
        let song = song_with(shape(FillKind::Ramp, place));
        let track = &song.tracks[0];
        let device = track_devices(track, Some(0)).len() - 2;
        let amount = control_at(track, Some(0), device, 7, false).unwrap();
        let Control::Gen { param, .. } = amount else {
            panic!("{amount:?}");
        };
        assert_eq!(param.name(), "Amount", "{place:?}");
        let action = turned(amount, 0, &song, KNOB_TOP).unwrap();
        let next = edit(&Arc::new(song.clone()), &action).unwrap();
        let shape = next.tracks[0].phrases[&0].generators()[0].shape;
        let turned_to = match shape.place {
            Where::Clusters(amount)
            | Where::Markov(amount)
            | Where::States(amount)
            | Where::Complementary { share: amount, .. } => amount,
            other => panic!("{other:?}"),
        };
        assert_eq!(turned_to, 100, "{place:?}");
        assert_eq!(
            std::mem::discriminant(&shape.place),
            std::mem::discriminant(&place)
        );
        // Which side of the coin a track is, is what it is, as a `Grids`' part is.
        let side = Control::Gen {
            phrase: 0,
            generator: 0,
            param: GenParam::Side,
        };
        assert!(app_only(side, track));
    }
}

#[test]
fn knob_7_turns_a_rhythms_child_and_the_app_keeps_the_rhythm_and_its_part() {
    let place = Where::Rhythm {
        rhythm: World::Gahu,
        part: Role::Low,
        child: 40,
    };
    let song = song_with(shape(FillKind::Ramp, place));
    let track = &song.tracks[0];
    let device = track_devices(track, Some(0)).len() - 2;
    let child = control_at(track, Some(0), device, 7, false).unwrap();
    let Control::Gen { param, .. } = child else {
        panic!("{child:?}");
    };
    assert_eq!(param, GenParam::Child);
    let action = turned(child, 0, &song, KNOB_TOP).unwrap();
    let next = edit(&Arc::new(song.clone()), &action).unwrap();
    let shape = next.tracks[0].phrases[&0].generators()[0].shape;
    assert_eq!(
        shape.place,
        Where::Rhythm {
            rhythm: World::Gahu,
            part: Role::Low,
            child: 100,
        }
    );
    // Which rhythm and which of its parts a track plays are what it is, as a `Grids`' part is.
    for param in [GenParam::Rhythm, GenParam::Role] {
        let control = Control::Gen {
            phrase: 0,
            generator: 0,
            param,
        };
        assert!(app_only(control, track), "{param:?}");
    }
}

#[test]
fn an_acids_loops_are_a_device_of_their_own_after_it() {
    let song = song_with(shape(FillKind::Acid, Where::Notes));
    let track = &song.tracks[0];
    let devices = live_devices(track, Some(0), &song, false);
    let names: Vec<String> = devices
        .iter()
        .rev()
        .take(3)
        .map(|device| device.name.to_string())
        .collect();
    assert_eq!(names, ["Phrase", "Note Loops", "Note Acid"]);
    // Its knobs in the order the fill's chances come; on notes there, no rhythm of its own.
    let last = track_devices(track, Some(0)).len() - 2;
    let at = |index| control_at(track, Some(0), last, index, false);
    assert_eq!(at(1), None, "the rhythm is the notes'");
    let Some(Control::Gen { param, .. }) = at(2) else {
        panic!("the notes' loop");
    };
    assert_eq!(param, GenParam::NoteLoop);
    let action = turned(at(4).unwrap(), 0, &song, KNOB_TOP).unwrap();
    let next = edit(&Arc::new(song.clone()), &action).unwrap();
    assert_eq!(
        next.tracks[0].phrases[&0].generators()[0]
            .shape
            .acid
            .loops
            .accents,
        64
    );
}

#[test]
fn acids_pitches_is_a_two_step_knob() {
    let song = song_with(shape(FillKind::Acid, Where::Notes));
    let track = &song.tracks[0];
    // Before its `Loops`, the last device.
    let device = track_devices(track, Some(0)).len() - 3;
    let pitches = control_at(track, Some(0), device, 6, false).unwrap();
    let Feel::Steps(names) = knob(pitches, track, &song).unwrap().feel else {
        panic!("a list");
    };
    let names: Vec<String> = names.iter().map(ToString::to_string).collect();
    assert_eq!(names, ["Roll", "Keep"]);
    let action = turned(pitches, 0, &song, KNOB_TOP).unwrap();
    let next = edit(&Arc::new(song.clone()), &action).unwrap();
    assert!(next.tracks[0].phrases[&0].generators()[0].shape.acid.keep);
}
