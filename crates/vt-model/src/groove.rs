//! Grooves: a track's feel, as a short pattern of pushes, how late or early each of its steps
//! lands, with a velocity for each and a little random timing if wanted. A groove only moves
//! steps about the grid, so it never changes the tempo or how long a loop takes. Its first step
//! always lands on the grid: that's the M8's rule that a groove's step lengths add up to the
//! pattern's, put as where each step lands. Swing is a groove of two steps.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::constants::TICKS_PER_STEP;

/// The most steps a groove's pattern has: a bar.
pub const MAX_GROOVE_STEPS: usize = 16;
/// The furthest a step is pushed either way, in ticks: half a step, as far as swing at 75% went.
pub const MAX_PUSH: i16 = (TICKS_PER_STEP / 2) as i16;
/// The most random timing a groove adds either way, in ticks: a tenth of a step.
pub const MAX_HUMANISE: u8 = (TICKS_PER_STEP / 10) as u8;
/// A step's velocity left as it is, in percent.
pub const FULL_VELOCITY: u8 = 100;
/// The highest a step's velocity can be, in percent.
pub const MAX_GROOVE_VELOCITY: u8 = 200;
/// How many grooves of its own a project can keep.
pub const CUSTOM_GROOVES: usize = 8;

/// How late swing puts every other step, in ticks, for a swing in tenths of a percent: 66% puts
/// the second step of each pair 66% of the pair's way along. It's how vimtracker swung before
/// grooves, so the swing grooves play as that did.
pub const fn swing_push(tenths: u16) -> i16 {
    let tenths = tenths as u64;
    ((TICKS_PER_STEP * 2 * tenths + 500) / 1000) as i16 - TICKS_PER_STEP as i16
}

/// A pattern of pushes, from 1 to 16 steps long, counted on the running count every track shares,
/// from the song's start or the last reset. Each step has a push in ticks, 240 to a step, and a
/// velocity in percent, and the whole has a humanise, how far either way each step may land at
/// random. The first step's push is always 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GrooveFile", into = "GrooveFile")]
pub struct Groove {
    steps: u8,
    pushes: [i16; MAX_GROOVE_STEPS],
    velocities: [u8; MAX_GROOVE_STEPS],
    humanise: u8,
}

/// A groove as songs keep it, with only its steps, and its velocities and humanise only where
/// they aren't left alone.
#[derive(Serialize, Deserialize)]
struct GrooveFile {
    pushes: Vec<i16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    velocities: Vec<u8>,
    #[serde(default, skip_serializing_if = "is_zero")]
    humanise: u8,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

/// Why a groove read from a song can't be played.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrooveError {
    #[error("a groove has from 1 to {MAX_GROOVE_STEPS} steps")]
    Steps,
    #[error("a groove's first step lands on the grid, so its push is 0")]
    FirstStep,
    #[error("a push is from -{MAX_PUSH} to {MAX_PUSH} ticks")]
    Push,
    #[error("a groove has a velocity for each step, from 0 to {MAX_GROOVE_VELOCITY}%")]
    Velocity,
    #[error("a groove's humanise is up to {MAX_HUMANISE} ticks")]
    Humanise,
}

impl TryFrom<GrooveFile> for Groove {
    type Error = GrooveError;

    fn try_from(file: GrooveFile) -> Result<Self, GrooveError> {
        let steps = file.pushes.len();
        if !(1..=MAX_GROOVE_STEPS).contains(&steps) {
            return Err(GrooveError::Steps);
        }
        if file.pushes[0] != 0 {
            return Err(GrooveError::FirstStep);
        }
        if file.pushes.iter().any(|push| push.abs() > MAX_PUSH) {
            return Err(GrooveError::Push);
        }
        let velocities_fit = file.velocities.is_empty() || file.velocities.len() == steps;
        if !velocities_fit || file.velocities.iter().any(|v| *v > MAX_GROOVE_VELOCITY) {
            return Err(GrooveError::Velocity);
        }
        if file.humanise > MAX_HUMANISE {
            return Err(GrooveError::Humanise);
        }
        let mut groove = Groove::pattern(&file.pushes);
        groove.velocities[..file.velocities.len()].copy_from_slice(&file.velocities);
        groove.humanise = file.humanise;
        Ok(groove)
    }
}

impl From<Groove> for GrooveFile {
    fn from(groove: Groove) -> Self {
        let velocities = groove.velocities();
        let shaped = velocities.iter().any(|velocity| *velocity != FULL_VELOCITY);
        Self {
            pushes: groove.pushes().to_vec(),
            velocities: if shaped {
                velocities.to_vec()
            } else {
                Vec::new()
            },
            humanise: groove.humanise,
        }
    }
}

impl Default for Groove {
    /// A bar of steps on the grid, which a groove of a project's own starts as.
    fn default() -> Self {
        Self::pattern(&[0; MAX_GROOVE_STEPS])
    }
}

impl Groove {
    /// A groove with these pushes, at full velocity with no humanise. The first push is taken as
    /// 0 and the rest are kept within reach.
    pub const fn pattern(pushes: &[i16]) -> Self {
        let mut groove = Self {
            steps: 1,
            pushes: [0; MAX_GROOVE_STEPS],
            velocities: [FULL_VELOCITY; MAX_GROOVE_STEPS],
            humanise: 0,
        };
        let mut at = 1;
        while at < pushes.len() && at < MAX_GROOVE_STEPS {
            let push = pushes[at];
            groove.pushes[at] = if push > MAX_PUSH {
                MAX_PUSH
            } else if push < -MAX_PUSH {
                -MAX_PUSH
            } else {
                push
            };
            at += 1;
        }
        if at > 1 {
            groove.steps = at as u8;
        }
        groove
    }

    /// Swing at a percentage in tenths, as two steps with the second pushed late.
    pub const fn swing(tenths: u16) -> Self {
        Self::pattern(&[0, swing_push(tenths)])
    }

    /// How many steps its pattern has.
    pub fn steps(&self) -> usize {
        usize::from(self.steps)
    }

    /// Each step's push, in order.
    pub fn pushes(&self) -> &[i16] {
        &self.pushes[..self.steps()]
    }

    /// Each step's velocity, in percent, in order.
    pub fn velocities(&self) -> &[u8] {
        &self.velocities[..self.steps()]
    }

    /// How far either way each step may land at random, in ticks.
    pub fn humanise(&self) -> u8 {
        self.humanise
    }

    /// How late a step lands, in ticks, or early if it's below 0. The step is counted from the
    /// section's start, and the pattern repeats.
    pub fn push(&self, step: usize) -> i16 {
        self.pushes[step % self.steps()]
    }

    /// The velocity a step's note plays at, in percent of its own.
    pub fn velocity(&self, step: usize) -> u8 {
        self.velocities[step % self.steps()]
    }

    /// How long a step lasts, in ticks, from where it lands to where the next one does.
    pub fn length(&self, step: usize) -> i64 {
        TICKS_PER_STEP as i64 + i64::from(self.push(step + 1)) - i64::from(self.push(step))
    }

    /// How far before its place on the grid a step can land, in ticks, with its humanise: what
    /// the engine has to work ahead by.
    pub fn lead(&self) -> u64 {
        let earliest = self.pushes().iter().copied().min().unwrap_or(0).min(0);
        let lead = i64::from(self.humanise) - i64::from(earliest);
        lead.clamp(0, i64::from(MAX_PUSH)) as u64
    }

    /// The same, with one step's push changed. The first step stays on the grid, and a push
    /// beyond half a step stops there.
    pub fn with_push(mut self, step: usize, push: i16) -> Self {
        if step > 0 && step < self.steps() {
            self.pushes[step] = push.clamp(-MAX_PUSH, MAX_PUSH);
        }
        self
    }

    /// The same, with one step's velocity changed, up to the most.
    pub fn with_velocity(mut self, step: usize, velocity: u8) -> Self {
        if step < self.steps() {
            self.velocities[step] = velocity.min(MAX_GROOVE_VELOCITY);
        }
        self
    }

    /// The same, with this many steps. Steps added are on the grid at full velocity.
    pub fn with_steps(mut self, steps: usize) -> Self {
        let steps = steps.clamp(1, MAX_GROOVE_STEPS);
        for at in steps..MAX_GROOVE_STEPS {
            self.pushes[at] = 0;
            self.velocities[at] = FULL_VELOCITY;
        }
        self.steps = steps as u8;
        self
    }

    /// The same, with this humanise, up to the most.
    pub fn with_humanise(mut self, ticks: u8) -> Self {
        self.humanise = ticks.min(MAX_HUMANISE);
        self
    }
}

/// The grooves every project has, in the order lists show them, with the project's own after.
/// `GRV` on a step has "Project" before them, for the project's groove.
pub const GROOVE_CHOICES: [&str; 27] = [
    "Project",
    "Straight",
    "Swing 52",
    "Swing 54",
    "Swing 56",
    "Swing 58",
    "Swing 60",
    "Swing 62",
    "Swing 64",
    "Swing 66",
    "Swing 70",
    "MPC 54",
    "MPC 58",
    "MPC 62",
    "MPC 66",
    "MPC 71",
    "Late last",
    "Push",
    "Pull",
    "Custom 1",
    "Custom 2",
    "Custom 3",
    "Custom 4",
    "Custom 5",
    "Custom 6",
    "Custom 7",
    "Custom 8",
];

/// Every groove's name, without the step's "Project".
pub const GROOVE_NAMES: &[&str] = match GROOVE_CHOICES.split_first() {
    Some((_, names)) => names,
    None => &GROOVE_CHOICES,
};

/// The swing percentages that have a groove, in order.
pub const SWING_PERCENTS: [u16; 9] = [52, 54, 56, 58, 60, 62, 64, 66, 70];

/// An MPC 60's swing, at one of its own percentages, on its own clock of 96 to a quarter: its
/// second sixteenth is moved to the nearest of its ticks, 10 of ours each.
const fn mpc(percent: u64) -> Groove {
    const MPC_TICK: u64 = TICKS_PER_STEP / 24;
    // A pair of sixteenths is 48 of its ticks.
    let at = (48 * percent + 50) / 100;
    let push = ((at - 24) * MPC_TICK) as i16;
    Groove::pattern(&[
        0, push, 0, push, 0, push, 0, push, 0, push, 0, push, 0, push, 0, push,
    ])
}

/// The grooves that come with the program, in the order of [`GROOVE_NAMES`].
const PRESETS: [Groove; 18] = [
    Groove::pattern(&[0; MAX_GROOVE_STEPS]),
    Groove::swing(520),
    Groove::swing(540),
    Groove::swing(560),
    Groove::swing(580),
    Groove::swing(600),
    Groove::swing(620),
    Groove::swing(640),
    Groove::swing(660),
    Groove::swing(700),
    mpc(54),
    mpc(58),
    mpc(62),
    mpc(66),
    mpc(71),
    // The M8's "swing last step": a bar on the grid but its last step, a sixth of a step late.
    Groove::pattern(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40]),
    // Ahead of the beat: the off-beat eighths early, and the sixteenths between a little less.
    Groove::pattern(&[
        0, -6, -12, -6, 0, -6, -12, -6, 0, -6, -12, -6, 0, -6, -12, -6,
    ]),
    // Laid back: the backbeats late, and the sixteenths between a little late.
    Groove::pattern(&[0, 8, 0, 8, 20, 8, 0, 8, 0, 8, 0, 8, 20, 8, 0, 8]),
];

/// One of the grooves a project can choose: those that come with the program, then its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GrooveId(u8);

impl GrooveId {
    /// Every step on the grid, as a new project plays.
    pub const STRAIGHT: Self = Self(0);

    /// Every groove, in the order lists show them.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..GROOVE_NAMES.len() as u8).map(Self)
    }

    /// Its place in the list.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// The groove at a place in the list.
    pub fn from_index(index: usize) -> Option<Self> {
        (index < GROOVE_NAMES.len()).then_some(Self(index as u8))
    }

    /// One of the project's own, counted from 0.
    pub fn custom(number: usize) -> Option<Self> {
        (number < CUSTOM_GROOVES).then_some(Self((PRESETS.len() + number) as u8))
    }

    /// Which of the project's own it is, counted from 0, if it's one.
    pub fn custom_number(self) -> Option<usize> {
        self.index().checked_sub(PRESETS.len())
    }

    /// The groove that comes with the program, if it's one of those.
    pub fn preset(self) -> Option<Groove> {
        PRESETS.get(self.index()).copied()
    }

    /// Its name on screen and in files.
    pub fn name(self) -> &'static str {
        GROOVE_NAMES[self.index()]
    }

    /// The groove with this name, ignoring case, spaces and hyphens, so `swing-66`, `swing66` and
    /// `Swing 66` all match.
    pub fn named(text: &str) -> Option<Self> {
        let squeezed = |name: &str| -> String {
            name.chars()
                .filter(|c| !c.is_whitespace() && *c != '-')
                .map(|c| c.to_ascii_lowercase())
                .collect()
        };
        let wanted = squeezed(text);
        Self::all().find(|id| squeezed(id.name()) == wanted)
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> String {
        let name = self.name();
        let late = |groove: Groove| groove.push(1);
        match name.split_once(' ') {
            _ if self == Self::STRAIGHT => "Every step on the grid".to_owned(),
            Some(("Swing", _)) => format!(
                "Every other step {} ticks late",
                self.preset().map_or(0, late)
            ),
            Some(("MPC", _)) => format!(
                "The MPC 60's swing, {} ticks late",
                self.preset().map_or(0, late)
            ),
            Some(("Custom", _)) => "One of the project's own".to_owned(),
            _ => match name {
                "Late last" => "A bar's last step a sixth of a step late",
                "Push" => "Ahead of the beat: off-beat eighths early",
                "Pull" => "Laid back: the backbeats late",
                _ => "",
            }
            .to_owned(),
        }
    }

    /// The swing groove at a whole percentage, with 50 being straight.
    pub fn swing(percent: u16) -> Option<Self> {
        if percent == 50 {
            return Some(Self::STRAIGHT);
        }
        let at = SWING_PERCENTS.iter().position(|known| *known == percent)?;
        Some(Self(at as u8 + 1))
    }
}

impl fmt::Display for GrooveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Songs keep a groove by its name, so a new one in the list doesn't move the others.
impl Serialize for GrooveId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for GrooveId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::named(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("there's no groove called {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_and_the_presets_line_up() {
        assert_eq!(GROOVE_NAMES.len(), PRESETS.len() + CUSTOM_GROOVES);
        assert_eq!(GROOVE_NAMES[0], "Straight");
        assert_eq!(GROOVE_NAMES.last(), Some(&"Custom 8"));
        assert_eq!(GrooveId::custom(0).map(GrooveId::name), Some("Custom 1"));
        assert_eq!(GrooveId::custom(CUSTOM_GROOVES), None);
        for (at, percent) in SWING_PERCENTS.iter().enumerate() {
            let id = GrooveId::swing(*percent).unwrap();
            assert_eq!(id.name(), format!("Swing {percent}"));
            assert_eq!(id.preset(), Some(Groove::swing(*percent * 10)), "{at}");
        }
        assert_eq!(GrooveId::swing(68), None);
    }

    #[test]
    fn swing_pushes_every_other_step_as_it_did() {
        assert_eq!(swing_push(500), 0);
        assert_eq!(swing_push(660), 77);
        assert_eq!(swing_push(667), 80);
        assert_eq!(swing_push(750), 120);
        let swing = Groove::swing(660);
        assert_eq!(swing.pushes(), &[0, 77]);
        assert_eq!((swing.push(4), swing.push(5)), (0, 77));
        assert_eq!(swing.length(0) + swing.length(1), 480);
    }

    #[test]
    fn the_mpc_swings_land_on_its_own_clock() {
        let pushes = |name: &str| GrooveId::named(name).unwrap().preset().unwrap().push(1);
        assert_eq!(pushes("MPC 54"), 20);
        assert_eq!(pushes("MPC 58"), 40);
        assert_eq!(pushes("MPC 66"), 80, "a triplet");
        assert_eq!(pushes("MPC 71"), 100);
    }

    #[test]
    fn the_first_step_stays_on_the_grid_so_a_pass_takes_as_long() {
        for id in GrooveId::all() {
            let Some(groove) = id.preset() else {
                continue;
            };
            assert_eq!(groove.push(0), 0, "{id}");
            let pass: i64 = (0..groove.steps()).map(|step| groove.length(step)).sum();
            assert_eq!(pass, groove.steps() as i64 * 240, "{id}");
        }
        let edited = Groove::default().with_push(0, 30).with_push(3, 500);
        assert_eq!(edited.push(0), 0);
        assert_eq!(edited.push(3), MAX_PUSH);
    }

    #[test]
    fn a_groove_is_kept_with_only_what_it_changes() {
        let groove = Groove::pattern(&[0, 20, -10]);
        let text = ron::to_string(&groove).unwrap();
        assert_eq!(text, "(pushes:[0,20,-10])");
        assert_eq!(ron::from_str::<Groove>(&text), Ok(groove));
        let shaped = groove.with_velocity(1, 80).with_humanise(4);
        let text = ron::to_string(&shaped).unwrap();
        assert_eq!(
            text,
            "(pushes:[0,20,-10],velocities:[100,80,100],humanise:4)"
        );
        assert_eq!(ron::from_str::<Groove>(&text), Ok(shaped));
        assert!(ron::from_str::<Groove>("(pushes:[5,0])").is_err());
        assert!(ron::from_str::<Groove>("(pushes:[])").is_err());
        assert!(ron::from_str::<Groove>("(pushes:[0,121])").is_err());
    }

    #[test]
    fn a_groove_is_named_ignoring_case_and_spaces() {
        assert_eq!(GrooveId::named("swing66"), GrooveId::swing(66));
        assert_eq!(GrooveId::named("CUSTOM 2"), GrooveId::custom(1));
        assert_eq!(GrooveId::named("project"), None);
        let id = GrooveId::named("Pull").unwrap();
        assert_eq!(ron::to_string(&id).unwrap(), "\"Pull\"");
    }

    #[test]
    fn every_groove_says_what_it_does() {
        for id in GrooveId::all() {
            assert!(!id.meaning().is_empty(), "{id}");
        }
        assert_eq!(
            GrooveId::swing(66).unwrap().meaning(),
            "Every other step 77 ticks late"
        );
    }

    #[test]
    fn the_lead_is_the_earliest_a_step_lands() {
        assert_eq!(Groove::swing(660).lead(), 0);
        assert_eq!(
            GrooveId::named("Push").unwrap().preset().unwrap().lead(),
            12
        );
        assert_eq!(Groove::swing(660).with_humanise(5).lead(), 5);
    }
}
