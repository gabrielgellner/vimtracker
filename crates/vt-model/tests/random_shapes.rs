//! `Random`'s values shaped after Mutable Instruments' Marbles (decision 302), over many seeds:
//! `Spread` narrows and widens them, `Bias` leans them, and on notes `Steps` lets through the
//! scale's most important degrees, the root first, and `Weights` says how often each comes.

use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, Grids, Pink, Random, Shape, ShapeColumn, Values,
    WEIGHTS, Walk, Where, values,
};
use vt_model::step::Step;

/// How many seeds each count is over, a phrase of `ROWS` notes each.
const SEEDS: u64 = 200;
const ROWS: usize = 64;

fn shape(from: u8, to: u8, random: Random) -> Shape {
    Shape {
        from,
        to,
        curve: 0,
        s: 0,
        kind: FillKind::Random,
        wave: vt_model::lfo::Shape::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::Notes,
        seed: 1,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: Grids::default(),
        random,
        vary: 0,
    }
}

fn spread(spread: u8, bias: i8) -> Random {
    Random {
        spread,
        bias,
        ..Random::default()
    }
}

fn in_key(kind: ScaleKind) -> ShapeColumn {
    ShapeColumn::Note {
        scale: Scale {
            on: true,
            root: 0,
            kind,
        },
        writes: None,
    }
}

fn chromatic() -> ShapeColumn {
    ShapeColumn::Note {
        scale: Scale {
            on: false,
            root: 0,
            kind: ScaleKind::Major,
        },
        writes: None,
    }
}

/// Every value a shape writes over a phrase of notes, for each of `SEEDS` seeds.
fn drawn(column: ShapeColumn, shape: Shape) -> Vec<u8> {
    let steps: Vec<Step> = (0..ROWS)
        .map(|_| "C-3 40 --- --- ---".parse().unwrap())
        .collect();
    let range = StepRange::between(0, ROWS - 1);
    (0..SEEDS)
        .flat_map(|seed| {
            let seeded = Shape { seed, ..shape };
            values(&steps, range, column, &seeded).into_iter()
        })
        .map(|(_, value)| value)
        .collect()
}

fn mean(values: &[u8]) -> f64 {
    values.iter().map(|value| f64::from(*value)).sum::<f64>() / values.len() as f64
}

fn deviation(values: &[u8]) -> f64 {
    let middle = mean(values);
    let sum: f64 = values
        .iter()
        .map(|value| (f64::from(*value) - middle).powi(2))
        .sum();
    (sum / values.len() as f64).sqrt()
}

/// The notes' names without their octaves, in the order each first came, sorted.
fn names(notes: &[u8]) -> Vec<&'static str> {
    let mut found: Vec<&str> = notes
        .iter()
        .map(|note| vt_model::scale::ROOTS[usize::from(*note) % 12])
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

fn sorted(mut names: Vec<&'static str>) -> Vec<&'static str> {
    names.sort_unstable();
    names
}

/// As it starts, and with every degree let through and weighed alike, it draws exactly as
/// `Random` always did, so a song made before plays as it did.
#[test]
fn as_it_starts_it_draws_as_random_always_did() {
    let column = in_key(ScaleKind::Major);
    let plain = drawn(column, shape(48, 84, Random::default()));
    let alike = Random {
        steps: 7,
        weights: [7; WEIGHTS],
        ..Random::default()
    };
    assert_eq!(drawn(column, shape(48, 84, alike)), plain);
    let velocity = drawn(ShapeColumn::Velocity, shape(0, 0x7F, Random::default()));
    // An even draw spreads as widely as 128 values do.
    assert!((deviation(&velocity) - 36.95).abs() < 1.0);
}

#[test]
fn spread_goes_from_one_value_through_a_bell_to_the_two_ends() {
    let velocity = |random| drawn(ShapeColumn::Velocity, shape(0, 0x7F, random));
    assert!(velocity(spread(0, 0)).iter().all(|value| *value == 0x40));
    let narrow = velocity(spread(20, 0));
    let wide = velocity(spread(80, 0));
    let even = velocity(spread(50, 0));
    assert!(deviation(&narrow) < 0.6 * deviation(&even), "a bell");
    assert!(deviation(&wide) > 1.2 * deviation(&even), "a bowl");
    for shaped in [&narrow, &wide] {
        assert!((mean(shaped) - 63.5).abs() < 2.0, "round the middle");
    }
    let ends = velocity(spread(100, 0));
    assert!(ends.iter().all(|value| matches!(*value, 0 | 0x7F)));
    let high = ends.iter().filter(|value| **value == 0x7F).count() as f64;
    assert!((high / ends.len() as f64 - 0.5).abs() < 0.03);
}

#[test]
fn bias_leans_toward_the_low_or_the_high_end() {
    let velocity = |random| drawn(ShapeColumn::Velocity, shape(0, 0x7F, random));
    assert!(mean(&velocity(spread(50, 50))) > 80.0);
    assert!(mean(&velocity(spread(50, -50))) < 47.0);
    assert!(mean(&velocity(spread(25, 60))) > mean(&velocity(spread(25, 20))));
    // One value, at the bias point, three quarters of the way up.
    assert!(velocity(spread(0, 50)).iter().all(|value| *value == 0x60));
    // At the ends, the high one as often as the bias point is high.
    let ends = velocity(spread(100, 40));
    let high = ends.iter().filter(|value| **value == 0x7F).count() as f64;
    assert!((high / ends.len() as f64 - 0.7).abs() < 0.03);
    // From and To either way round lean the same way, toward the higher value.
    let swapped = drawn(ShapeColumn::Velocity, shape(0x7F, 0, spread(50, 50)));
    assert!(mean(&swapped) > 80.0);
}

/// Each `Steps` lets one more degree through, the most important first, as Marbles' scales weigh
/// them: in C major the root, the fifth, the third, the seventh, the sixth, the second and the
/// fourth; with the scale off, the semitones by consonance with the root.
#[test]
fn steps_let_the_most_important_degrees_through_first() {
    let order = ["C", "G", "E", "B", "A", "D", "F"];
    for steps in 1..=7 {
        let random = Random {
            steps,
            ..Random::default()
        };
        let notes = drawn(in_key(ScaleKind::Major), shape(60, 83, random));
        let wanted = sorted(order[..usize::from(steps)].to_vec());
        assert_eq!(names(&notes), wanted, "Steps {steps}");
    }
    let minor = Random {
        steps: 4,
        ..Random::default()
    };
    let notes = drawn(in_key(ScaleKind::Minor), shape(60, 83, minor));
    assert_eq!(names(&notes), sorted(vec!["C", "G", "D#", "A#"]));
    let order = [
        "C", "G", "F", "A", "E", "D#", "G#", "A#", "D", "B", "C#", "F#",
    ];
    for steps in 1..=12 {
        let random = Random {
            steps,
            ..Random::default()
        };
        let notes = drawn(chromatic(), shape(60, 83, random));
        let wanted = sorted(order[..usize::from(steps)].to_vec());
        assert_eq!(names(&notes), wanted, "Steps {steps}, the scale off");
    }
}

/// Each degree comes as often as its weight says, as each of the meloDICER's faders sets a
/// semitone's chance: none at 0, and three times as often at `F` as at `5`.
#[test]
fn weights_say_how_often_each_degree_comes() {
    let mut weights = [0; WEIGHTS];
    weights[0] = 0xF;
    weights[4] = 0x5;
    let random = Random {
        weights,
        ..Random::default()
    };
    // One octave of C major, each degree once.
    let notes = drawn(in_key(ScaleKind::Major), shape(60, 71, random));
    assert_eq!(names(&notes), ["C", "G"]);
    let roots = notes.iter().filter(|note| **note == 60).count() as f64;
    let fifths = notes.iter().filter(|note| **note == 67).count() as f64;
    assert!((roots / fifths - 3.0).abs() < 0.3, "{roots} to {fifths}");
    // With the scale off, a weight a semitone from the root.
    let mut weights = [0; WEIGHTS];
    weights[1] = 0xF;
    let random = Random {
        weights,
        ..Random::default()
    };
    let notes = drawn(chromatic(), shape(48, 83, random));
    assert_eq!(names(&notes), ["C#"]);
}

/// The notes let through are still spread and leaned: a narrow bell keeps to the middle of the
/// range, and on a degree left out, `Spread` 0 lands on the nearest one let through, the lower of
/// two as near.
#[test]
fn spread_and_bias_shape_the_notes_let_through() {
    let column = in_key(ScaleKind::Major);
    let three = |spread, bias| Random {
        spread,
        bias,
        steps: 3,
        ..Random::default()
    };
    // C-3 to B-3: F is the middle, which Steps 3 leaves out, so E and G are as near.
    let one = drawn(column, shape(60, 71, three(0, 0)));
    assert!(one.iter().all(|note| *note == 64), "E-3");
    // Over three octaves, a bell keeps to the middle one and a high bias to the top.
    let bell = drawn(column, shape(48, 83, three(5, 0)));
    let middle = bell.iter().filter(|note| (60..72).contains(*note)).count();
    assert!(middle as f64 > 0.8 * bell.len() as f64);
    let high = drawn(column, shape(48, 83, three(50, 80)));
    assert!(mean(&high) > 72.0);
    assert!(names(&high).len() == 3);
}

/// Like every fill, the same seed draws the same and another seed something else, a row's draw
/// its own.
#[test]
fn a_shaped_draw_is_seeded() {
    let column = in_key(ScaleKind::Dorian);
    let random = Random {
        spread: 30,
        bias: -20,
        steps: 5,
        weights: [9, 3, 0xF, 1, 0xF, 4, 2, 0, 0, 0, 0, 0],
    };
    let steps: Vec<Step> = (0..32)
        .map(|_| "C-3 40 --- --- ---".parse().unwrap())
        .collect();
    let range = StepRange::between(0, 31);
    let once = shape(48, 84, random);
    let again = values(&steps, range, column, &once);
    assert_eq!(values(&steps, range, column, &once), again);
    let other = Shape { seed: 2, ..once };
    assert_ne!(values(&steps, range, column, &other), again);
}
