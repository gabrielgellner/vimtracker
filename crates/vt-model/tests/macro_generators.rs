//! Macros onto generator settings by hand (decision 294): a turn of a macro holding a generator's
//! setting writes every live column of the track with a generator on that column again, from the
//! generator's shape with the macro's value put in, its own shape kept underneath; letting it go
//! gives the column back its own.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::project::{load_song, save_song};
use vt_model::selection::StepRange;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, GenParam, Pink, Shape, ShapeColumn, Values, Walk,
    Where,
};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;

const NOTE: &str = "C-3 40 --- --- ---";

/// A tresillo on the Velocity column of eight notes, `7F` on its hits and `40` between, on two
/// phrases of one track.
fn song() -> Arc<Song> {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..8).map(|_| NOTE.parse().unwrap()).collect();
    for id in [0, 1] {
        song.tracks[0]
            .phrases
            .insert(id, Arc::new(Phrase::from_steps(steps.clone())));
    }
    let mut song = Arc::new(song);
    for phrase in [0, 1] {
        song = edit(
            &song,
            &Action::Generate {
                track: 0,
                phrase,
                column: ShapeColumn::Velocity,
                span: Span::rows(StepRange::between(0, 7)),
                shape: tresillo(),
            },
        )
        .unwrap();
    }
    song
}

fn tresillo() -> Shape {
    Shape {
        from: 0x7F,
        to: 0x7F,
        curve: 0,
        s: 0,
        kind: FillKind::Alternate,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::Euclid { hits: 3, rotate: 0 },
        seed: 7,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        vary: 0,
    }
}

/// How many rows of a phrase are a hit, at `7F`.
fn hits(song: &Song, phrase: u32) -> usize {
    song.tracks[0].phrases[&phrase]
        .steps()
        .iter()
        .filter(|step| step.velocity == Some(0x7F))
        .count()
}

fn hits_target() -> MacroTarget {
    MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Hits,
    }
}

fn turn(song: &Arc<Song>, value: u16) -> Arc<Song> {
    edit(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value,
        },
    )
    .unwrap()
}

#[test]
fn a_macro_on_a_generators_hits_writes_every_phrases_column_again_as_it_turns() {
    let song = song();
    assert_eq!((hits(&song, 0), hits(&song, 1)), (3, 3));
    let mapped = edit(
        &song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits_target(), (1, 8)),
        },
    )
    .unwrap();
    // At `00`, its min: one hit on both phrases.
    assert_eq!((hits(&mapped, 0), hits(&mapped, 1)), (1, 1));
    let up = turn(&mapped, 0xFF00);
    assert_eq!((hits(&up, 0), hits(&up, 1)), (8, 8));
    let half = turn(&mapped, 0x8000);
    assert_eq!(hits(&half, 0), 5, "1 + 7 halfway, to the nearest");
    // The generator keeps its own, and is heard with the macro's.
    let generator = &half.tracks[0].phrases[&0].generators()[0];
    assert_eq!(generator.shape.place, Where::Euclid { hits: 3, rotate: 0 });
    assert!(generator.is_held());
    assert_eq!(
        generator.heard().place,
        Where::Euclid { hits: 5, rotate: 0 }
    );
    // Let go, it's its own again.
    let free = edit(
        &half,
        &Action::UnmapMacro {
            track: 0,
            target: hits_target(),
        },
    )
    .unwrap();
    assert_eq!((hits(&free, 0), hits(&free, 1)), (3, 3));
    assert!(!free.tracks[0].phrases[&0].generators()[0].is_held());
}

#[test]
fn a_reshape_of_a_held_generator_keeps_the_macros_setting_and_changes_the_rest() {
    let song = song();
    let mapped = edit(
        &song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits_target(), (2, 2)),
        },
    )
    .unwrap();
    assert_eq!(hits(&mapped, 0), 2);
    let mut rotated = tresillo();
    rotated.place = Where::Euclid { hits: 3, rotate: 1 };
    let reshaped = edit(
        &mapped,
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape: rotated,
        },
    )
    .unwrap();
    let phrase = &reshaped.tracks[0].phrases[&0];
    assert_eq!(hits(&reshaped, 0), 2, "still the macro's hits");
    assert_eq!(phrase.steps()[0].velocity, Some(0x40), "and rotated by one");
    assert_eq!(phrase.generators()[0].shape, rotated);
}

#[test]
fn a_generator_mapping_is_saved_by_its_column_and_key_and_read_back() {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vt-model-macro-gen-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let song = edit(
        &song(),
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits_target(), (1, 8)),
        },
    )
    .unwrap();
    let song = turn(&song, 0xFF00);
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(text.contains("Gen("), "{text}");
    assert!(text.contains("param: \"hits\""), "{text}");
    let loaded = load_song(&dir).unwrap();
    assert_eq!(loaded.tracks[0].macros, song.tracks[0].macros);
    assert_eq!(hits(&loaded, 0), 8, "the column as it was heard");
    // A turn after loading writes it again from the macro, the generator's own underneath.
    let loaded = turn(&Arc::new(loaded), 0);
    assert_eq!(hits(&loaded, 0), 1);
}

#[test]
fn the_fill_and_the_where_arent_for_a_macro() {
    for param in [
        GenParam::Fill,
        GenParam::Where,
        GenParam::Source,
        GenParam::Values,
    ] {
        let target = MacroTarget::Gen {
            column: GenColumn::Note,
            param,
        };
        assert!(!target.mappable(), "{param:?}");
    }
    assert!(hits_target().mappable());
}
