//! The shapes the track page's drawings follow, from 0 to 1 across them, which
//! [`kitty::plot`](crate::kitty::plot) draws.

use vt_model::constants::MAX_ENVELOPE_MS;
use vt_model::envelope::{Adsr, fall, rise};

/// The most of a diagram's width each stage can take, with a sustain always taking this much.
const STAGE_SHARE: f32 = 0.25;

/// How much of a diagram's width a stage takes.
fn stage_width(ms: u16) -> f32 {
    STAGE_SHARE * (1.0 + f32::from(ms)).ln() / (1.0 + f32::from(MAX_ENVELOPE_MS)).ln()
}

/// An envelope's level at `x` from 0 to 1 across its diagram. A stage's width depends only on its
/// own time, on a log scale so a 1 ms attack shows beside a long release, and changing a stage only
/// moves what comes after it. A short envelope leaves room on the right.
pub(crate) fn level_at(envelope: &Adsr, x: f32) -> f32 {
    let (attack, decay, release) = (
        stage_width(envelope.attack_ms),
        stage_width(envelope.decay_ms),
        stage_width(envelope.release_ms),
    );
    let mut at = x;
    let sustain = envelope.sustain_level();
    if at < attack {
        return rise(at / attack);
    }
    at -= attack;
    if at < decay {
        return 1.0 + (sustain - 1.0) * fall(at / decay);
    }
    at -= decay;
    if at < STAGE_SHARE {
        return sustain;
    }
    at -= STAGE_SHARE;
    if at < release {
        return sustain * (1.0 - fall(at / release));
    }
    0.0
}

/// Where each of an envelope's stages ends across its diagram, from 0 to 1, as [`level_at`] has
/// them: its attack, decay, sustain and release. A drawing puts a point at each, so a corner is
/// sharp and an attack of a millisecond still reaches full.
pub(crate) fn stage_ends(envelope: &Adsr) -> [f32; 4] {
    let attack = stage_width(envelope.attack_ms);
    let decay = attack + stage_width(envelope.decay_ms);
    let sustain = decay + STAGE_SHARE;
    [
        attack,
        decay,
        sustain,
        sustain + stage_width(envelope.release_ms),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Adsr {
        Adsr {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            ..Adsr::note_shape()
        }
    }

    /// Its level at points across its diagram.
    fn levels(envelope: &Adsr, points: usize) -> Vec<f32> {
        (0..=points)
            .map(|point| level_at(envelope, point as f32 / points as f32))
            .collect()
    }

    #[test]
    fn it_rises_to_full_settles_on_the_sustain_and_ends_at_silence() {
        let envelope = adsr(100, 100, 50, 100);
        let levels = levels(&envelope, 200);
        assert_eq!(levels[0], 0.0);
        assert_eq!(level_at(&envelope, stage_ends(&envelope)[0]), 1.0);
        assert_eq!(*levels.last().unwrap(), 0.0);
        // The sustain is the flat part, all of a quarter of the way.
        let sustain = envelope.sustain_level();
        assert!(
            levels.iter().filter(|level| **level == sustain).count() >= 45,
            "{levels:?}"
        );
    }

    #[test]
    fn a_short_attack_still_shows() {
        let envelope = adsr(1, 180, 0, 50);
        let [attack, decay, ..] = stage_ends(&envelope);
        assert!(attack < 0.03, "the attack is near the start, at {attack}");
        assert_eq!(level_at(&envelope, attack), 1.0);
        assert!(
            level_at(&envelope, (attack + decay) / 2.0) < 1.0,
            "and it comes back down"
        );
    }

    #[test]
    fn a_stage_only_moves_what_comes_after_it() {
        let peak = |attack_ms, decay_ms| stage_ends(&adsr(attack_ms, decay_ms, 0, 50))[0];
        for decay_ms in [0, 1, 50, 200, 8000] {
            assert_eq!(peak(1, decay_ms), peak(1, 0), "with a decay of {decay_ms}");
        }
        assert!(peak(200, 0) > peak(1, 0));
    }

    #[test]
    fn the_longest_envelope_fits_and_a_short_one_leaves_room() {
        let longest = stage_ends(&adsr(8000, 8000, 50, 8000));
        assert!(longest[3] <= 1.0, "{longest:?}");
        // A short envelope has reached silence by half way.
        let short = adsr(1, 0, 100, 10);
        assert!(stage_ends(&short)[3] < 0.5);
        assert!(levels(&short, 100)[50..].iter().all(|level| *level == 0.0));
    }

    #[test]
    fn an_instant_envelope_is_safe_to_draw() {
        let envelope = adsr(0, 0, 100, 0);
        assert!(
            levels(&envelope, 100)
                .iter()
                .all(|level| (0.0..=1.0).contains(level))
        );
        assert!(stage_ends(&envelope).iter().all(|end| end.is_finite()));
        assert_eq!(level_at(&envelope, 0.0), envelope.sustain_level());
    }
}
