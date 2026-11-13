//! `Grids`: a drum part from Mutable Instruments' Grids, its map of 25 real drum patterns. `X` and
//! `Y` pick a place on the map and the four patterns nearest it are blended, a level for each step
//! of each part, the kick's, the snare's and the hats'. A part's density is a threshold: a step
//! plays where its level is over it, so turning the density up adds the next most important hit
//! first, and a step whose level is over a second threshold, Grids' own 192, is accented. `Chaos`
//! raises each step's level by a random amount up to a quarter of the way, drawn from the seed.
//!
//! The map's data and the way it's read are Emilie Gillet's, from the Grids firmware
//! (`grids/pattern_generator.cc` and `grids/resources.cc`, GPL-3, copyright 2011 and 2012), with
//! their notice kept in `grids/map.rs`. `ReadDrumMap`'s blend is here as it is there, in the same
//! 8-bit maths, so a part plays on the same steps as the module's. One thing differs: the module
//! draws one perturbation a part at the start of each pass and adds it to every step of the pass,
//! where a render is one fixed pass, so here each step draws its own.
//!
//! Across a kit's tracks, each track's generator plays one part of the same place on the map
//! ([`DrumPart`], [`parts`]), so `S` on a kick, a snare and a hat writes one beat.

mod map;

use serde::{Deserialize, Serialize};

use super::{GenCell, ShapeColumn, mix};
use crate::constants::DEFAULT_NOTE_VELOCITY;
use crate::selection::StepRange;
use crate::song::{Machine, SynthVoice};
use crate::step::{ACCENT_VELOCITY, MAX_VELOCITY, Note, Step};

pub use map::STEPS as GRIDS_STEPS;

/// A step over this level is accented, as Grids' accent outputs are.
pub const ACCENT_LEVEL: u8 = 192;
/// The rows a bar of the map takes when each row is a 16th: every other step of its 32.
const SIXTEENTHS: usize = map::STEPS / 2;
/// What the chaos draws are for, so they don't follow another fill's.
const CHAOS: u64 = 0x6121_D500;

/// One of Grids' three parts, a column of its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DrumPart {
    #[default]
    Kick,
    Snare,
    Hats,
}

impl DrumPart {
    /// Every part, in the module's order.
    pub const ALL: [Self; 3] = [Self::Kick, Self::Snare, Self::Hats];

    /// The name on screen, as the Rytm names its drums.
    pub fn name(self) -> &'static str {
        match self {
            Self::Kick => "BD",
            Self::Snare => "SD",
            Self::Hats => "HH",
        }
    }

    /// What it plays, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Kick => "The kick's part",
            Self::Snare => "The snare's part",
            Self::Hats => "The hats' part",
        }
    }

    /// Its place among the parts.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The part a track's sound plays, if it says: an 808 voice's, or a kit sound's label as the
    /// Rytm names its drums and a kit names its tracks (`BD`, `SD`, `CP`, `RS`, `CH`, `OH`, `CY`,
    /// `RD`, `CB`), or a name starting with a word for one.
    pub fn of_sound(machine: Machine, name: &str) -> Option<Self> {
        match machine {
            Machine::Plaits(SynthVoice::Kick) => return Some(Self::Kick),
            Machine::Plaits(SynthVoice::Snare) => return Some(Self::Snare),
            Machine::Plaits(SynthVoice::Hat) => return Some(Self::Hats),
            _ => {}
        }
        let word = name
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|word| !word.is_empty())?
            .to_ascii_uppercase();
        let word = word.trim_end_matches(|c: char| c.is_ascii_digit());
        match word {
            "BD" | "KD" | "KICK" | "BASSDRUM" => Some(Self::Kick),
            "SD" | "SN" | "SNARE" | "CP" | "CLAP" | "RS" | "RIM" | "RIMSHOT" => Some(Self::Snare),
            "HH" | "CH" | "OH" | "HAT" | "HATS" | "HIHAT" | "CY" | "CYMBAL" | "RD" | "RIDE"
            | "CB" | "SHAKER" => Some(Self::Hats),
            _ => None,
        }
    }
}

/// The part each of several tracks plays, in order, from the part its sound plays where it says
/// one (`known`): a track whose sound says none takes the first part no track has yet, so a kick,
/// a pad and a hat give the pad the snare; once all three are taken, the rest go round them by
/// their place.
pub fn parts(known: &[Option<DrumPart>]) -> Vec<DrumPart> {
    let mut taken = [false; 3];
    for part in known.iter().flatten() {
        taken[part.index()] = true;
    }
    known
        .iter()
        .enumerate()
        .map(|(nth, part)| {
            part.unwrap_or_else(|| match taken.iter().position(|taken| !taken) {
                Some(free) => {
                    taken[free] = true;
                    DrumPart::ALL[free]
                }
                None => DrumPart::ALL[nth % DrumPart::ALL.len()],
            })
        })
        .collect()
}

/// Where a `Grids` fill reads the map and how much of it plays. Saved with a live generator's
/// shape, and left out while it's as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grids {
    /// The place on the map, from `00` to `FF` each way, as the module's knobs read.
    pub x: u8,
    pub y: u8,
    /// Each part's density, the kick's first: a step plays where its level is over `FF` less this.
    pub density: [u8; 3],
    /// How far each step's level is raised at random, up to a quarter of the way at `FF`.
    pub chaos: u8,
    /// The part this track plays. None only while the panel shapes several tracks at once, each
    /// taking its own.
    pub part: Option<DrumPart>,
    /// Whether a row is one of the map's 32 steps a bar, for a track at `1/32`, rather than two of
    /// them, a 16th.
    pub fine: bool,
}

impl Default for Grids {
    fn default() -> Self {
        Self {
            x: 0x80,
            y: 0x80,
            density: [0x80; 3],
            chaos: 0,
            part: Some(DrumPart::Kick),
            fine: false,
        }
    }
}

impl Grids {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The part it plays, the kick if it's yet to be given one.
    pub fn part(&self) -> DrumPart {
        self.part.unwrap_or_default()
    }

    /// A step's level on the map, for a part, before chaos: the four patterns around the place
    /// blended, as Grids' `ReadDrumMap` blends them.
    pub fn level(&self, part: DrumPart, step: usize) -> u8 {
        let (i, j) = (usize::from(self.x >> 6), usize::from(self.y >> 6));
        // The place within its square of the map, from 0 to 252.
        let (across, down) = (self.x << 2, self.y << 2);
        let at = part.index() * map::STEPS + step % map::STEPS;
        let node = |i: usize, j: usize| map::NODES[map::MAP[i][j]][at];
        blend(
            blend(node(i, j), node(i + 1, j), across),
            blend(node(i, j + 1), node(i + 1, j + 1), across),
            down,
        )
    }

    /// The map's step a row reads, counting from the first row: a 16th is every other one.
    fn step_of(&self, offset: usize) -> usize {
        if self.fine {
            offset % map::STEPS
        } else {
            offset % SIXTEENTHS * 2
        }
    }

    /// A row's level, its step's raised by chaos as Grids raises it: up to `chaos` over 4, and to
    /// the top where that would go past it.
    fn heard(&self, part: DrumPart, seed: u64, offset: usize) -> u8 {
        let level = self.level(part, self.step_of(offset));
        let draw = (mix(seed ^ CHAOS ^ part.index() as u64, offset) >> 56) as u8;
        let raised = scaled(draw, self.chaos >> 2);
        if level < u8::MAX - raised {
            level + raised
        } else {
            u8::MAX
        }
    }

    /// Whether a row plays, and whether it's accented.
    pub fn hit(&self, part: DrumPart, seed: u64, offset: usize) -> Option<bool> {
        let level = self.heard(part, seed, offset);
        let threshold = !self.density[part.index()];
        (level > threshold).then_some(level > ACCENT_LEVEL)
    }
}

/// Grids' `U8Mix`: `a` and `b` mixed, `balance` of 255 the way from `a`.
fn blend(a: u8, b: u8, balance: u8) -> u8 {
    let mixed = u32::from(a) * u32::from(u8::MAX - balance) + u32::from(b) * u32::from(balance);
    (mixed >> 8) as u8
}

/// Grids' `U8U8MulShift8`: `a` times `b` over 256.
fn scaled(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b)) >> 8) as u8
}

/// A velocity that isn't an accent, as `Acid`'s plain notes take one.
fn plain(velocity: u8) -> u8 {
    if velocity < ACCENT_VELOCITY {
        velocity
    } else {
        DEFAULT_NOTE_VELOCITY
    }
}

/// A `Grids` fill's cells for `range`, one for each row, the first in `cells[0]`. A row that plays
/// is a note, `7F` where it's accented: the note there if there is one, or else the first in the
/// rows, or `from` where there's none; a plain one keeps the velocity there, or takes the first
/// note's, or the one notes are typed with on a column that had none. A row that doesn't play is
/// cleared, and one with nothing there gets its own, so a render laid over an earlier one leaves
/// nothing of it. An `OFF` stays. It writes notes; on any other column it leaves the rows alone.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    from: u8,
    seed: u64,
    grids: Grids,
    cells: &mut [GenCell],
) {
    let ShapeColumn::Note { writes, .. } = column else {
        return;
    };
    let part = grids.part();
    let rows = steps.iter().skip(range.first).take(cells.len());
    let first = rows.clone().find_map(|step| match step.note {
        Note::On(note) => Some((note, step)),
        _ => None,
    });
    let (pitch, velocity, gate) = match first {
        Some((note, step)) => (
            note,
            plain(step.velocity.unwrap_or(MAX_VELOCITY)),
            step.gate,
        ),
        None => (from, plain(writes.unwrap_or(DEFAULT_NOTE_VELOCITY)), None),
    };
    for (offset, (cell, step)) in cells.iter_mut().zip(rows).enumerate() {
        *cell = match (step.note, grids.hit(part, seed, offset)) {
            (Note::Off, _) => GenCell::Under,
            (Note::On(note), Some(accent)) => GenCell::Note {
                note: Note::On(note),
                velocity: Some(if accent {
                    MAX_VELOCITY
                } else {
                    plain(step.velocity.unwrap_or(MAX_VELOCITY))
                }),
                gate: step.gate,
            },
            (Note::Empty, Some(accent)) => GenCell::Note {
                note: Note::On(pitch),
                velocity: Some(if accent { MAX_VELOCITY } else { velocity }),
                gate: first.map_or(step.gate, |_| gate),
            },
            (Note::On(_), None) => GenCell::Note {
                note: Note::Empty,
                velocity: None,
                gate: None,
            },
            (Note::Empty, None) => GenCell::Note {
                note: Note::Empty,
                velocity: step.velocity,
                gate: step.gate,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;
    use crate::shape::{
        Acid, Bits, DEFAULT_RATE, FillKind, Follow, Pink, Shape, Values, Walk, Where, written,
    };

    fn shape(grids: Grids, seed: u64) -> Shape {
        Shape {
            from: 36,
            to: 48,
            curve: 0,
            s: 0,
            kind: FillKind::Grids,
            wave: crate::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::AllRows,
            seed,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids,
            vary: 0,
        }
    }

    fn writing() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x50),
        }
    }

    /// The rows after a render, drawn as `X` for an accent, `x` for a note and `.` for none.
    fn drawn(steps: &[Step], column: ShapeColumn, shape: &Shape) -> String {
        let mut out = steps.to_vec();
        let range = StepRange::between(0, steps.len() - 1);
        for (row, step) in written(steps, range, column, shape) {
            out[row] = step;
        }
        out.iter()
            .map(|step| match (step.note, step.velocity) {
                (Note::On(_), Some(MAX_VELOCITY)) => 'X',
                (Note::On(_), _) => 'x',
                (Note::Off, _) => 'o',
                _ => '.',
            })
            .collect()
    }

    fn at(x: u8, y: u8, part: DrumPart, density: u8) -> Grids {
        let mut grids = Grids {
            x,
            y,
            part: Some(part),
            ..Grids::default()
        };
        grids.density[part.index()] = density;
        grids
    }

    /// A node's level as the blend reads it on the node itself: twice through `blend` with no
    /// balance, which takes a little off, as on the module.
    fn on_node(level: u8) -> u8 {
        blend(blend(level, 0, 0), 0, 0)
    }

    #[test]
    fn on_a_node_of_the_map_a_part_plays_that_nodes_pattern() {
        // The nodes a place on the map reaches exactly: X and Y at 00, 40, 80 and C0.
        for i in 0..4 {
            for j in 0..4 {
                let node = map::NODES[map::MAP[i][j]];
                for part in DrumPart::ALL {
                    let grids = Grids {
                        fine: true,
                        ..at(i as u8 * 0x40, j as u8 * 0x40, part, 0xFF)
                    };
                    let steps = vec![Step::default(); map::STEPS];
                    let wanted: String = (0..map::STEPS)
                        .map(
                            |step| match on_node(node[part.index() * map::STEPS + step]) {
                                0 => '.',
                                level if level > ACCENT_LEVEL => 'X',
                                _ => 'x',
                            },
                        )
                        .collect();
                    let found = drawn(&steps, writing(), &shape(grids, 7));
                    assert_eq!(found, wanted, "node {i},{j} {}", part.name());
                }
            }
        }
    }

    #[test]
    fn the_middle_of_the_map_is_a_beat_and_a_row_is_a_sixteenth() {
        let steps = vec![Step::default(); 16];
        let beat: Vec<String> = DrumPart::ALL
            .into_iter()
            .map(|part| drawn(&steps, writing(), &shape(at(0x80, 0x80, part, 0x80), 1)))
            .collect();
        assert_eq!(
            beat,
            ["X.....x.X.x.....", "....X.....x.XXx.", "..X.x.X...X.x.x."]
        );
        // At the top of the map, a kick on every beat.
        let kick = drawn(
            &steps,
            writing(),
            &shape(at(0x80, 0, DrumPart::Kick, 0x40), 1),
        );
        assert_eq!(kick, "X...X...X...X...");
    }

    #[test]
    fn density_adds_the_hits_in_order_of_their_level() {
        for (x, y) in [
            (0, 0),
            (0x80, 0x80),
            (0x37, 0xC9),
            (0xFF, 0xFF),
            (0xA0, 0x10),
        ] {
            for part in DrumPart::ALL {
                let grids = Grids {
                    fine: true,
                    ..at(x, y, part, 0)
                };
                let levels: Vec<u8> = (0..map::STEPS)
                    .map(|step| grids.level(part, step))
                    .collect();
                let mut before: Vec<usize> = Vec::new();
                for density in 0..=0xFF_u8 {
                    let grids = Grids {
                        fine: true,
                        ..at(x, y, part, density)
                    };
                    let hits: Vec<usize> = (0..map::STEPS)
                        .filter(|step| grids.hit(part, 0, *step).is_some())
                        .collect();
                    // Only the steps over the threshold, so each comes in as the density passes
                    // its level, the most important first, and none goes.
                    let wanted: Vec<usize> = (0..map::STEPS)
                        .filter(|step| levels[*step] > !density)
                        .collect();
                    assert_eq!(hits, wanted, "{x},{y} {} at {density}", part.name());
                    assert!(before.iter().all(|step| hits.contains(step)));
                    let (weakest_in, strongest_out) = (
                        hits.iter().map(|step| levels[*step]).min(),
                        (0..map::STEPS)
                            .filter(|step| !hits.contains(step))
                            .map(|step| levels[step])
                            .max(),
                    );
                    if let (Some(weakest), Some(strongest)) = (weakest_in, strongest_out) {
                        assert!(weakest > strongest, "a hit plays before a stronger one");
                    }
                    before = hits;
                }
                assert!(
                    (0..map::STEPS).all(|step| grids.hit(part, 0, step).is_none()),
                    "none at 00"
                );
            }
        }
    }

    #[test]
    fn chaos_is_the_same_for_a_seed_and_new_for_another() {
        let steps = vec![Step::default(); 64];
        let mut grids = at(0x80, 0x80, DrumPart::Hats, 0x60);
        let calm = drawn(&steps, writing(), &shape(grids, 1));
        assert_eq!(
            calm,
            drawn(&steps, writing(), &shape(grids, 99)),
            "no chaos, no seed"
        );
        grids.chaos = 0xFF;
        let wild = drawn(&steps, writing(), &shape(grids, 1));
        assert_eq!(
            wild,
            drawn(&steps, writing(), &shape(grids, 1)),
            "repeatable"
        );
        assert_ne!(wild, calm);
        assert_ne!(wild, drawn(&steps, writing(), &shape(grids, 2)));
        // It only adds: every hit without it is still there.
        for (calm, wild) in calm.chars().zip(wild.chars()) {
            assert!(calm == '.' || wild != '.');
        }
        // Each bar of the span draws anew, as a pass of the module does.
        assert_ne!(wild[..16], wild[16..32]);
    }

    /// Its chaos is drawn from its seed, so `Vary` gives each pass chaos of its own (decision
    /// 293): the first pass is the one written, and the next draws again.
    #[test]
    fn vary_draws_its_chaos_again_each_pass() {
        let steps = vec![Step::default(); 32];
        let mut grids = at(0x80, 0x80, DrumPart::Hats, 0x60);
        grids.chaos = 0xFF;
        let varying = Shape {
            vary: 1,
            ..shape(grids, 1)
        };
        assert!(varying.draws() && varying.varies());
        assert!(crate::shape::GenParam::Vary.exists_for(&varying, false));
        let pass = |pass| drawn(&steps, writing(), &varying.on_pass(pass));
        assert_eq!(pass(0), drawn(&steps, writing(), &shape(grids, 1)));
        assert_ne!(pass(1), pass(0));
        assert_eq!(pass(1), pass(1), "and the same again");
    }

    #[test]
    fn a_row_of_its_own_is_a_32nd_with_fine_on() {
        // Node 23, at X `C0` and Y `00`, ends its kick on a roll of 32nds.
        assert_eq!(map::MAP[3][0], 23);
        let node = map::NODES[23];
        assert!((0..map::STEPS).any(|step| step % 2 == 1 && node[step] > 0));
        let grids = Grids {
            fine: true,
            ..at(0xC0, 0, DrumPart::Kick, 0xFF)
        };
        let fine = drawn(&vec![Step::default(); 32], writing(), &shape(grids, 1));
        let coarse = drawn(
            &vec![Step::default(); 16],
            writing(),
            &shape(
                Grids {
                    fine: false,
                    ..grids
                },
                1,
            ),
        );
        let even: String = fine.chars().step_by(2).collect();
        assert_eq!(even, coarse, "a 16th is every other step");
        assert!(fine.chars().skip(1).step_by(2).any(|hit| hit != '.'));
    }

    #[test]
    fn notes_there_keep_their_pitch_and_new_ones_take_the_first() {
        let mut steps = vec![Step::default(); 16];
        steps[0].note = Note::On(40);
        steps[0].velocity = Some(0x30);
        steps[3].note = Note::On(41);
        steps[5].note = Note::Off;
        let column = ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        };
        let grids = at(0x80, 0, DrumPart::Kick, 0x40);
        let range = StepRange::between(0, 15);
        let out: Vec<(usize, Step)> = written(&steps, range, column, &shape(grids, 1));
        let get = |row: usize| out.iter().find(|(at, _)| *at == row).map(|(_, step)| step);
        // Four on the floor: row 0's note stays, the others take its pitch, and row 3's goes.
        assert_eq!(get(0).unwrap().note, Note::On(40));
        assert_eq!(get(4).unwrap().note, Note::On(40));
        assert_eq!(get(3).unwrap().note, Note::Empty);
        assert!(get(5).is_none(), "an OFF stays");
        // The accents are 7F, and a plain hit the first note's velocity.
        let accents: Vec<bool> = [0, 4, 8, 12]
            .map(|row| get(row).unwrap().velocity == Some(MAX_VELOCITY))
            .to_vec();
        assert!(accents.iter().any(|accent| *accent));
        for row in [0, 4, 8, 12] {
            let velocity = get(row).unwrap().velocity;
            assert!(velocity == Some(MAX_VELOCITY) || velocity == Some(0x30));
        }
        // On an empty column, From at the velocity notes are typed with.
        let empty = vec![Step::default(); 16];
        let grids = at(0x80, 0x80, DrumPart::Kick, 0xFF);
        let out = written(&empty, range, writing(), &shape(grids, 1));
        let plain = out
            .iter()
            .find(|(_, step)| step.velocity == Some(0x50))
            .unwrap();
        assert_eq!(plain.1.note, Note::On(36));
    }

    #[test]
    fn other_columns_are_left_alone() {
        let steps = vec![Step::default(); 16];
        let range = StepRange::between(0, 15);
        let shape = shape(Grids::default(), 1);
        assert!(written(&steps, range, ShapeColumn::Velocity, &shape).is_empty());
    }

    #[test]
    fn a_tracks_sound_says_its_part() {
        let sampler = Machine::Sampler;
        assert_eq!(
            DrumPart::of_sound(Machine::Plaits(SynthVoice::Snare), "X"),
            Some(DrumPart::Snare)
        );
        for (name, part) in [
            ("BD", Some(DrumPart::Kick)),
            ("bd2", Some(DrumPart::Kick)),
            ("Kick 909", Some(DrumPart::Kick)),
            ("CP", Some(DrumPart::Snare)),
            ("SD", Some(DrumPart::Snare)),
            ("OH", Some(DrumPart::Hats)),
            ("CH", Some(DrumPart::Hats)),
            ("", None),
            ("LT", None),
            ("Bass", None),
        ] {
            assert_eq!(DrumPart::of_sound(sampler, name), part, "{name}");
        }
    }

    #[test]
    fn tracks_take_the_parts_their_sounds_leave_in_order() {
        use DrumPart::*;
        assert_eq!(parts(&[None, None, None]), [Kick, Snare, Hats]);
        assert_eq!(parts(&[Some(Hats), None, None]), [Hats, Kick, Snare]);
        assert_eq!(parts(&[Some(Kick), None, Some(Hats)]), [Kick, Snare, Hats]);
        assert_eq!(
            parts(&[Some(Kick), Some(Hats), Some(Hats), None]),
            [Kick, Hats, Hats, Snare]
        );
        assert_eq!(
            parts(&[None, None, None, None, None]),
            [Kick, Snare, Hats, Kick, Snare]
        );
    }
}
