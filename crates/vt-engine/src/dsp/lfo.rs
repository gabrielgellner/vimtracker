//! LFO waves. They're worked out from where the song is rather than stepped along, so they can't
//! drift, and the same project gives the same result.

use vt_model::lfo::Shape;

/// A wave's value from -1 to 1, some number of cycles from its start. `key` picks a random wave's
/// values, so tracks and LFOs differ.
#[inline]
pub fn wave(shape: Shape, cycles: f64, key: u64) -> f32 {
    let position = cycles.rem_euclid(1.0) as f32;
    match shape {
        Shape::Sine => (position * std::f32::consts::TAU).sin(),
        Shape::Tri => {
            if position < 0.25 {
                position * 4.0
            } else if position < 0.75 {
                2.0 - position * 4.0
            } else {
                position * 4.0 - 4.0
            }
        }
        Shape::Saw => 1.0 - position * 2.0,
        Shape::Ramp => position * 2.0 - 1.0,
        Shape::Square => {
            if position < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Shape::Random => random(key, cycles.floor() as i64),
    }
}

/// A value from -1 to 1 for one cycle, from a stateless mix so nothing has to be stepped.
fn random(key: u64, cycle: i64) -> f32 {
    let mut mixed = key
        .wrapping_add((cycle as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^= mixed >> 31;
    (mixed >> 40) as f32 / (1_u64 << 23) as f32 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} and {b}");
    }

    #[test]
    fn each_shape_goes_where_its_name_says() {
        close(wave(Shape::Sine, 0.25, 0), 1.0);
        close(wave(Shape::Sine, 0.75, 0), -1.0);
        close(wave(Shape::Tri, 0.0, 0), 0.0);
        close(wave(Shape::Tri, 0.25, 0), 1.0);
        close(wave(Shape::Tri, 0.75, 0), -1.0);
        close(wave(Shape::Saw, 0.0, 0), 1.0);
        close(wave(Shape::Saw, 0.5, 0), 0.0);
        close(wave(Shape::Ramp, 0.0, 0), -1.0);
        close(wave(Shape::Ramp, 0.5, 0), 0.0);
        close(wave(Shape::Square, 0.25, 0), 1.0);
        close(wave(Shape::Square, 0.75, 0), -1.0);
    }

    #[test]
    fn a_wave_repeats_and_runs_before_its_start() {
        close(wave(Shape::Tri, 3.25, 0), 1.0);
        close(wave(Shape::Tri, -0.75, 0), 1.0);
    }

    #[test]
    fn random_holds_for_a_cycle_and_changes_by_cycle_and_key() {
        let held = wave(Shape::Random, 2.1, 5);
        assert_eq!(held, wave(Shape::Random, 2.9, 5));
        assert_ne!(held, wave(Shape::Random, 3.1, 5));
        assert_ne!(held, wave(Shape::Random, 2.1, 6));
        for cycle in 0..1000 {
            let value = wave(Shape::Random, f64::from(cycle), 1);
            assert!((-1.0..=1.0).contains(&value), "{value}");
        }
    }
}
