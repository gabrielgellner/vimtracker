//! What can be modulated. Things that want moving register here, and sources add to a value per
//! target without knowing what any of them are.

use serde::{Deserialize, Serialize};

/// Something an LFO or an envelope can move, in the same units as its FX command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Cut,
    Res,
    Pan,
    Vol,
    Dly,
    Rev,
    /// In semitones, where the others are in their hex units.
    Pitch,
    Har,
    Tim,
    Mor,
}

/// What a target declares about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetInfo {
    /// The name on screen.
    pub name: &'static str,
    /// What it's listed with.
    pub group: &'static str,
    /// What it does, for a list to choose from.
    pub meaning: &'static str,
    /// The step command that sets it, if one does.
    pub command: Option<&'static str>,
}

impl Target {
    /// Every target, in the order `+` goes through them.
    pub const ALL: [Self; 10] = [
        Self::Cut,
        Self::Res,
        Self::Pan,
        Self::Vol,
        Self::Dly,
        Self::Rev,
        Self::Pitch,
        Self::Har,
        Self::Tim,
        Self::Mor,
    ];

    /// How many places sources add to.
    pub const COUNT: usize = Self::ALL.len();

    /// Its place among those.
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// What it declares.
    pub const fn info(self) -> TargetInfo {
        const fn info(
            name: &'static str,
            group: &'static str,
            meaning: &'static str,
            command: Option<&'static str>,
        ) -> TargetInfo {
            TargetInfo {
                name,
                group,
                meaning,
                command,
            }
        }
        match self {
            Self::Cut => info("Cut", "Filter", "Filter cutoff", Some("CUT")),
            Self::Res => info("Res", "Filter", "Filter resonance", Some("RES")),
            Self::Pan => info("Pan", "Output", "Pan", Some("PAN")),
            Self::Vol => info("Vol", "Output", "Volume", Some("VOL")),
            Self::Dly => info("Dly", "Output", "Delay send", Some("DLY")),
            Self::Rev => info("Rev", "Output", "Reverb send", Some("REV")),
            Self::Pitch => info("Pitch", "Machine", "Pitch, with depth in semitones", None),
            Self::Har => info("Har", "Machine", "A synth's first setting", Some("HAR")),
            Self::Tim => info("Tim", "Machine", "A synth's second setting", Some("TIM")),
            Self::Mor => info("Mor", "Machine", "A synth's third setting", Some("MOR")),
        }
    }

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        self.info().name
    }

    /// What's on a track to choose from.
    pub fn on_track() -> impl Iterator<Item = Self> {
        Self::ALL.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::FxCommand;

    #[test]
    fn every_target_has_its_own_place() {
        for (at, target) in Target::ALL.into_iter().enumerate() {
            assert_eq!(target.index(), at, "{target:?}");
        }
        assert_eq!(Target::COUNT, Target::ALL.len());
    }

    #[test]
    fn no_two_targets_share_a_name_or_a_command() {
        for (at, target) in Target::ALL.into_iter().enumerate() {
            for other in &Target::ALL[at + 1..] {
                let (one, two) = (target.info(), other.info());
                assert_ne!(one.name, two.name);
                if one.command.is_some() {
                    assert_ne!(one.command, two.command, "{target:?} and {other:?}");
                }
            }
        }
    }

    #[test]
    fn a_targets_command_is_a_real_one() {
        for target in Target::ALL {
            if let Some(command) = target.info().command {
                assert!(
                    FxCommand::ALL.iter().any(|real| real.name() == command),
                    "{command}"
                );
            }
        }
    }

    #[test]
    fn every_target_is_in_a_group_and_says_what_it_does() {
        for target in Target::ALL {
            let info = target.info();
            assert!(!info.group.is_empty(), "{target:?}");
            assert!(!info.meaning.is_empty(), "{target:?}");
        }
    }

    #[test]
    fn a_saved_target_keeps_its_name_in_a_song() {
        assert_eq!(ron::to_string(&Target::Cut).unwrap(), "Cut");
        assert_eq!(ron::from_str::<Target>("Pitch").unwrap(), Target::Pitch);
    }
}
