//! Named rhythms from around the world as a `Where`, after Noise Engineering's Zularic Repetitor,
//! which William Sethares' *Rhythm and Transforms* (2007) set going: a traditional pattern, one
//! part of it a track (its bell, a low drum or a high drum), and `Child`, the chance each of the
//! part's hits plays, so a part varies about the pattern by its seed.
//!
//! The patterns are this program's own transcriptions from published notation, one pulse a row,
//! each from the source its comment names; none is taken from the module. Where a source gives a
//! pattern's bell alone, its drums are the beat it's heard against and the pulses between: the
//! four beats and the offbeats in a 4/4 or 2/4 cycle, and in a 12/8 cycle the four dotted
//! quarters and the six quarters, the three against two its music is known for.
//!
//! - Toussaint, "A Mathematical Analysis of African, Brazilian, and Cuban Clave Rhythms",
//!   *Bridges* 2002: the son and rumba claves, the bossa nova, gahu, shiko and soukous bells, in
//!   sixteen pulses, the son's "at time units 0, 3, 6, 10 and 12".
//! - Toussaint, "Classification and Phylogenetic Analysis of African Ternary Rhythm Timelines",
//!   *Bridges* 2003: the bembé, `[x . x . x x . x . x . x]`.
//! - Toussaint, "The Euclidean Algorithm Generates Traditional Musical Rhythms", *Bridges* 2005:
//!   the tresillo `[x . . x . . x .]`, the cinquillo `[x . x x . x x .]` and the samba bell
//!   `[x . x . . x . x . x . . x . x .]`.
//! - The son's drums, as the Wikipedia article "Tumbao" sets them out: the bass's tumbao, the two
//!   offbeats of each tresillo, *bombo* and *ponche* (Peñalosa, *The Clave Matrix*, 2009, p. 40);
//!   the conga's open tones on the last two offbeats of each two beats (Mauleón, *Salsa
//!   Guidebook*, 1993, p. 63).
//! - Motorik, the Wikipedia article's notated beat and its MIDI file: hats on every eighth, the
//!   snare on 2 and 4, and the kick on every eighth but those.

use serde::{Deserialize, Serialize};

use super::mix;

/// What a `Child`'s draws are for, so they don't follow a `Chance`'s.
const CHILD: u64 = 0xC41D_0000;
/// The most pulses a pattern goes round in.
pub const MAX_PULSES: usize = 16;

/// One of a rhythm's parts, which a track plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Role {
    /// The timeline, the bell or clave the others are counted from.
    #[default]
    Bell,
    Low,
    High,
}

impl Role {
    /// Every part, in the order `S` across tracks gives them.
    pub const ALL: [Self; 3] = [Self::Bell, Self::Low, Self::High];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bell => "Bell",
            Self::Low => "Low drum",
            Self::High => "High drum",
        }
    }

    /// What it plays, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Bell => "The timeline, its bell or clave",
            Self::Low => "The low drum, or a kit's kick",
            Self::High => "The high drum, or a kit's snare",
        }
    }

    /// Its place among the parts.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The part `nth` along from this one, going round.
    pub fn after(self, nth: usize) -> Self {
        Self::ALL[(self.index() + nth) % Self::ALL.len()]
    }
}

/// A named rhythm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum World {
    #[default]
    SonClave,
    RumbaClave,
    BossaNova,
    Gahu,
    Shiko,
    Soukous,
    Samba,
    Tresillo,
    Cinquillo,
    Bembe,
    Motorik,
}

/// A pattern's onsets, by pulse.
struct Pattern {
    pulses: usize,
    bell: &'static [u8],
    low: &'static [u8],
    high: &'static [u8],
}

/// The four beats of sixteen pulses, and the offbeats between them.
const BEATS: &[u8] = &[0, 4, 8, 12];
const OFFBEATS: &[u8] = &[2, 6, 10, 14];
/// The two beats of eight, and the offbeats.
const BEATS_OF_8: &[u8] = &[0, 4];
const OFFBEATS_OF_8: &[u8] = &[2, 6];

/// A bell of sixteen pulses, against the beat.
const fn sixteen(bell: &'static [u8]) -> Pattern {
    Pattern {
        pulses: 16,
        bell,
        low: BEATS,
        high: OFFBEATS,
    }
}

/// A bell of eight pulses, against the beat.
const fn eight(bell: &'static [u8]) -> Pattern {
    Pattern {
        pulses: 8,
        bell,
        low: BEATS_OF_8,
        high: OFFBEATS_OF_8,
    }
}

impl World {
    /// Every rhythm, in the order a list offers them: the 4/4 bells, Cuban, Brazilian and
    /// African, then the 2/4 ones, then the 12/8, then a kit's.
    pub const ALL: [Self; 11] = [
        Self::SonClave,
        Self::RumbaClave,
        Self::BossaNova,
        Self::Gahu,
        Self::Shiko,
        Self::Soukous,
        Self::Samba,
        Self::Tresillo,
        Self::Cinquillo,
        Self::Bembe,
        Self::Motorik,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::SonClave => "Son clave",
            Self::RumbaClave => "Rumba clave",
            Self::BossaNova => "Bossa nova",
            Self::Gahu => "Gahu",
            Self::Shiko => "Shiko",
            Self::Soukous => "Soukous",
            Self::Samba => "Samba",
            Self::Tresillo => "Tresillo",
            Self::Cinquillo => "Cinquillo",
            Self::Bembe => "Bembé",
            Self::Motorik => "Motorik",
        }
    }

    /// Where it's from and how long it is, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::SonClave => "Cuba, 3-2, with the son's bass and conga, 16 rows",
            Self::RumbaClave => "Cuba, 3-2, 16 rows",
            Self::BossaNova => "Brazil, 16 rows",
            Self::Gahu => "Ghana, the bell of the Ewe dance, 16 rows",
            Self::Shiko => "Africa, 16 rows",
            Self::Soukous => "Africa, 16 rows",
            Self::Samba => "Brazil, 16 rows",
            Self::Tresillo => "Cuba, 3+3+2, 8 rows",
            Self::Cinquillo => "Cuba, 8 rows",
            Self::Bembe => "West Africa and Cuba, the 12/8 bell, 12 rows: a 1/8T track",
            Self::Motorik => "Germany, a kit: hats, kick and snare, 16 rows",
        }
    }

    /// Its place among the rhythms.
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|world| *world == self)
            .unwrap_or(0)
    }

    fn pattern(self) -> Pattern {
        match self {
            // Toussaint 2002, Figure 3 and §2; the son's drums after "Tumbao".
            Self::SonClave => Pattern {
                pulses: 16,
                bell: &[0, 3, 6, 10, 12],
                low: &[3, 6, 11, 14],
                high: &[6, 7, 14, 15],
            },
            Self::RumbaClave => sixteen(&[0, 3, 7, 10, 12]),
            Self::BossaNova => sixteen(&[0, 3, 6, 10, 13]),
            Self::Gahu => sixteen(&[0, 3, 6, 10, 14]),
            Self::Shiko => sixteen(&[0, 4, 6, 10, 12]),
            Self::Soukous => sixteen(&[0, 3, 6, 10, 11]),
            // Toussaint 2005, E(7,16) started on its last onset.
            Self::Samba => sixteen(&[0, 2, 5, 7, 9, 12, 14]),
            // Toussaint 2005, E(3,8) and E(5,8).
            Self::Tresillo => eight(&[0, 3, 6]),
            Self::Cinquillo => eight(&[0, 2, 3, 5, 6]),
            // Toussaint 2003, Figure 2, against its four beats and the six of the three-two.
            Self::Bembe => Pattern {
                pulses: 12,
                bell: &[0, 2, 4, 5, 7, 9, 11],
                low: &[0, 3, 6, 9],
                high: &[0, 2, 4, 6, 8, 10],
            },
            // Wikipedia's "Motorik", its staff and its MIDI file.
            Self::Motorik => Pattern {
                pulses: 16,
                bell: &[0, 2, 4, 6, 8, 10, 12, 14],
                low: &[0, 2, 6, 8, 10, 14],
                high: &[4, 12],
            },
        }
    }

    /// How many pulses, a row each, it goes round in.
    pub fn pulses(self) -> usize {
        self.pattern().pulses
    }

    /// The pulses a part strikes on, in order.
    pub fn onsets(self, part: Role) -> &'static [u8] {
        let pattern = self.pattern();
        match part {
            Role::Bell => pattern.bell,
            Role::Low => pattern.low,
            Role::High => pattern.high,
        }
    }

    /// Whether a part strikes on the row `offset` rows from the first, going round.
    pub fn strikes(self, part: Role, offset: usize) -> bool {
        let pulse = offset % self.pulses();
        self.onsets(part)
            .iter()
            .any(|onset| usize::from(*onset) == pulse)
    }

    /// Whether its pulses are a bar's triplet eighths, so it fits a bar on a `1/8T` track.
    pub fn ternary(self) -> bool {
        self.pulses().is_multiple_of(3)
    }
}

/// Whether a part's hit on the row `offset` plays: `child` percent of them do, each by a draw
/// from the seed, so at `100` every one and at `0` none.
pub(super) fn plays(rhythm: World, part: Role, child: u8, seed: u64, offset: usize) -> bool {
    rhythm.strikes(part, offset)
        && (child >= 100 || mix(seed ^ CHILD, offset) % 100 < u64::from(child))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::StepRange;
    use crate::shape::{FillKind, Follow, Shape, ShapeColumn, Where, on_track, targets};
    use crate::step::Step;

    /// A pattern in box notation, `x` a hit and `.` a rest.
    fn boxes(rhythm: World, part: Role) -> String {
        (0..rhythm.pulses())
            .map(|pulse| {
                if rhythm.strikes(part, pulse) {
                    'x'
                } else {
                    '.'
                }
            })
            .collect()
    }

    fn place(rhythm: World, part: Role, child: u8) -> Where {
        Where::Rhythm {
            rhythm,
            part,
            child,
        }
    }

    /// Which of `count` empty rows a `Where` chooses for notes of its own, with this seed.
    fn pattern(place: Where, count: usize, seed: u64) -> Vec<bool> {
        let steps = vec![Step::default(); count];
        let column = ShapeColumn::Note {
            scale: crate::scale::Scale::default(),
            writes: Some(0x64),
        };
        let mut hit = vec![false; count];
        for row in targets(
            &steps,
            StepRange::between(0, count - 1),
            column,
            place,
            seed,
        ) {
            hit[row] = true;
        }
        hit
    }

    #[test]
    fn each_bell_is_its_published_notation() {
        // Toussaint's box notation, as the papers print it.
        let bells = [
            (World::SonClave, "x..x..x...x.x..."),
            (World::RumbaClave, "x..x...x..x.x..."),
            (World::BossaNova, "x..x..x...x..x.."),
            (World::Gahu, "x..x..x...x...x."),
            (World::Shiko, "x...x.x...x.x..."),
            (World::Soukous, "x..x..x...xx...."),
            (World::Samba, "x.x..x.x.x..x.x."),
            (World::Tresillo, "x..x..x."),
            (World::Cinquillo, "x.xx.xx."),
            (World::Bembe, "x.x.xx.x.x.x"),
            (World::Motorik, "x.x.x.x.x.x.x.x."),
        ];
        assert_eq!(bells.len(), World::ALL.len());
        for (rhythm, notation) in bells {
            assert_eq!(boxes(rhythm, Role::Bell), notation, "{}", rhythm.name());
        }
    }

    #[test]
    fn the_six_clave_bells_are_toussaints_by_his_distance_matrix() {
        // Toussaint 2002's Figure 9: the Euclidean distances between the interval vectors of
        // Shiko, Son, Soukous, Rumba, Bossa-Nova and Gahu, which only his six patterns give.
        let six = [
            World::Shiko,
            World::SonClave,
            World::Soukous,
            World::RumbaClave,
            World::BossaNova,
            World::Gahu,
        ];
        let matrix = [
            [0.0, 1.41, 2.0, 2.45, 2.0, 3.16],
            [1.41, 0.0, 1.41, 1.41, 1.41, 2.83],
            [2.0, 1.41, 0.0, 2.0, 2.83, 4.24],
            [2.45, 1.41, 2.0, 0.0, 2.0, 3.16],
            [2.0, 1.41, 2.83, 2.0, 0.0, 1.41],
            [3.16, 2.83, 4.24, 3.16, 1.41, 0.0],
        ];
        let intervals = |rhythm: World| -> Vec<f64> {
            let onsets = rhythm.onsets(Role::Bell);
            (0..onsets.len())
                .map(|at| {
                    let next = onsets.get(at + 1).map_or(16, |next| usize::from(*next));
                    (next - usize::from(onsets[at])) as f64
                })
                .collect()
        };
        for (a, row) in six.iter().zip(matrix) {
            for (b, wanted) in six.iter().zip(row) {
                let distance: f64 = intervals(*a)
                    .iter()
                    .zip(intervals(*b))
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f64>()
                    .sqrt();
                assert!((distance - wanted).abs() < 0.01, "{a:?} {b:?}: {distance}");
            }
        }
    }

    #[test]
    fn the_drums_are_their_sources() {
        // The son's bass on the tresillo's two offbeats and its conga's open tones on the last
        // two offbeats of each two beats.
        assert_eq!(boxes(World::SonClave, Role::Low), "...x..x....x..x.");
        assert_eq!(boxes(World::SonClave, Role::High), "......xx......xx");
        // Motorik's kick and snare.
        assert_eq!(boxes(World::Motorik, Role::Low), "x.x...x.x.x...x.");
        assert_eq!(boxes(World::Motorik, Role::High), "....x.......x...");
        // The rest: the beat and the offbeats, and in 12/8 four beats against six.
        assert_eq!(boxes(World::Gahu, Role::Low), "x...x...x...x...");
        assert_eq!(boxes(World::Gahu, Role::High), "..x...x...x...x.");
        assert_eq!(boxes(World::Tresillo, Role::Low), "x...x...");
        assert_eq!(boxes(World::Bembe, Role::Low), "x..x..x..x..");
        assert_eq!(boxes(World::Bembe, Role::High), "x.x.x.x.x.x.");
        for rhythm in World::ALL {
            for part in Role::ALL {
                let onsets = rhythm.onsets(part);
                assert!(onsets.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(onsets.iter().all(|at| usize::from(*at) < rhythm.pulses()));
                assert!(rhythm.pulses() <= MAX_PULSES);
            }
            assert_eq!(rhythm.ternary(), rhythm == World::Bembe);
            assert_eq!(World::ALL[rhythm.index()], rhythm);
        }
    }

    #[test]
    fn a_rhythm_goes_round_from_the_first_row() {
        let hit = pattern(place(World::Tresillo, Role::Bell, 100), 20, 1);
        let shown: String = hit.iter().map(|hit| if *hit { 'x' } else { '.' }).collect();
        assert_eq!(shown, "x..x..x.x..x..x.x..x");
        let bembe = pattern(place(World::Bembe, Role::Bell, 100), 24, 1);
        assert_eq!(bembe[..12], bembe[12..]);
    }

    #[test]
    fn children_fire_at_their_chance_and_only_on_the_patterns_hits() {
        for rhythm in [World::SonClave, World::Bembe, World::Motorik] {
            for part in Role::ALL {
                let mother = pattern(place(rhythm, part, 100), 48, 0);
                for child in [0, 25, 50, 80] {
                    let (mut played, mut hits) = (0, 0);
                    for seed in 0..400 {
                        let hit = pattern(place(rhythm, part, child), 48, seed);
                        for (row, played_here) in hit.iter().enumerate() {
                            // A child never plays where its mother rests.
                            assert!(!played_here || mother[row], "{rhythm:?} {part:?} {row}");
                            hits += usize::from(mother[row]);
                            played += usize::from(*played_here);
                        }
                    }
                    let share = played as f64 / hits as f64;
                    let wanted = f64::from(child) / 100.0;
                    assert!(
                        (share - wanted).abs() < 0.03,
                        "{rhythm:?} {child}%: {share}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_child_repeats_with_its_seed_and_varies_with_another() {
        let child = place(World::Samba, Role::Bell, 60);
        let one = pattern(child, 64, 3);
        assert_eq!(one, pattern(child, 64, 3));
        assert_ne!(one, pattern(child, 64, 4));
        let shape = Shape {
            kind: FillKind::Ramp,
            place: child,
            ..Shape::following(Follow::default(), 3)
        };
        assert!(shape.draws());
    }

    #[test]
    fn across_tracks_each_takes_the_next_part_and_a_seed_of_its_own() {
        let shape = Shape {
            kind: FillKind::Ramp,
            place: place(World::Gahu, Role::Low, 70),
            ..Shape::following(Follow::default(), 9)
        };
        let parts: Vec<Where> = (0..4).map(|nth| on_track(&shape, nth).place).collect();
        assert_eq!(
            parts,
            [
                place(World::Gahu, Role::Low, 70),
                place(World::Gahu, Role::High, 70),
                place(World::Gahu, Role::Bell, 70),
                place(World::Gahu, Role::Low, 70),
            ]
        );
        assert_ne!(on_track(&shape, 0).seed, on_track(&shape, 1).seed);
    }
}
