//! Live generators on the Roto-Control: a device each after the Mod page's, laid out by its
//! fill, every setting the app doesn't choose on one knob, a knob's turn reshaping the column
//! as the Gen page's keys do.

use std::sync::Arc;

use vt_model::action::{Action, edit};
use vt_model::generator::{Generator, Span};
use vt_model::scale::Scale;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, GenParam, Pink, Random, Shape, ShapeColumn, Values,
    Walk, Where,
};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_roto::live::{Feel, KNOB_TOP};
use vt_tui::roto::devices::{control_at, knob, live_devices, placement, track_devices};
use vt_tui::roto::edits::turned;
use vt_tui::roto::layouts::{GENERATORS, WHERE_FIRST, WHERE_SECOND, app_only, layout_of, resolve};
use vt_tui::track_page::{Control, Page};

fn shape(kind: FillKind, place: Where) -> Shape {
    Shape {
        from: 0x30,
        to: 0x48,
        curve: 0,
        s: 0,
        kind,
        wave: vt_model::lfo::Shape::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::new(&[0x30, 0x40]),
        place,
        seed: 1,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: vt_model::shape::Grids::default(),
        random: Random::default(),
        vary: 0,
        variation: vt_model::shape::Variation::default(),
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
    let generator = Generator::new(note(), Span::Rows { first: 0, last: 15 }, shape);
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
    ];
    for kind in FillKind::ALL {
        for place in wheres {
            let mut shape = shape(kind, place);
            // A `Follow` mutating, as a Bloom branch does, has its kinds of mutation and `Vary`.
            shape.follow.mutation = 2;
            let song = song_with(shape);
            let track = &song.tracks[0];
            let devices = track_devices(track, Some(0));
            let (page, _, spec) = devices.last().unwrap();
            assert_eq!(*page, Page::Gen, "{}", kind.name());
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
    let all: std::collections::BTreeSet<&str> =
        GENERATORS.iter().map(|layout| layout.kind).collect();
    assert_eq!(used, all, "every generator layout is some fill's");
}

#[test]
fn a_generator_comes_after_the_mod_pages_devices_named_by_its_column_and_fill() {
    let song = song_with(shape(FillKind::Ramp, Where::Euclid { hits: 3, rotate: 0 }));
    let track = &song.tracks[0];
    let without = live_devices(track, None, &song, false);
    let with = live_devices(track, Some(0), &song, false);
    assert_eq!(with.len(), without.len() + 1, "one device, and no Empty");
    let device = with.last().unwrap();
    assert_eq!(device.name.to_string(), "Note Ramp");
    assert!(!device.stored);
}

#[test]
fn turning_a_knob_reshapes_the_column_as_the_pages_keys_do() {
    let song = song_with(shape(FillKind::Ramp, Where::Euclid { hits: 3, rotate: 0 }));
    let track = &song.tracks[0];
    let device = track_devices(track, Some(0)).len() - 1;
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
fn acids_pitches_is_a_two_step_knob() {
    let song = song_with(shape(FillKind::Acid, Where::Notes));
    let track = &song.tracks[0];
    let device = track_devices(track, Some(0)).len() - 1;
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
