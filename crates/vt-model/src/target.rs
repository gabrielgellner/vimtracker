//! What can be modulated. Things that want moving register here, and sources add to a value per
//! target without knowing what any of them are.

use serde::{Deserialize, Serialize};

use crate::insert::{INSERTS, MAX_PARAMS};
use crate::song::Track;

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
    /// One of the settings of what's in a track's slot, by its place in the effect's list.
    Slot {
        slot: u8,
        param: u8,
    },
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
    /// The targets every track has, in the order `+` goes through them.
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
    pub const COUNT: usize = Self::ALL.len() + INSERTS * MAX_PARAMS;

    /// Its place among those.
    #[inline]
    pub const fn index(self) -> usize {
        match self {
            Self::Cut => 0,
            Self::Res => 1,
            Self::Pan => 2,
            Self::Vol => 3,
            Self::Dly => 4,
            Self::Rev => 5,
            Self::Pitch => 6,
            Self::Har => 7,
            Self::Tim => 8,
            Self::Mor => 9,
            Self::Slot { slot, param } => {
                Self::ALL.len() + slot as usize * MAX_PARAMS + param as usize
            }
        }
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
            Self::Slot { .. } => info("Setting", "FX", "A setting of what's in a slot", None),
        }
    }

    /// The name on screen, on this track, as a slot's settings depend on what's in it.
    pub fn name_on(self, track: &Track) -> String {
        let Self::Slot { slot, param } = self else {
            return self.info().name.to_owned();
        };
        let found = track
            .inserts
            .get(usize::from(slot))
            .and_then(|insert| insert.params().get(usize::from(param)));
        found.map_or_else(
            || format!("{} ---", slot + 1),
            |found| format!("{} {}", slot + 1, found.name),
        )
    }

    /// What it does, for a list to choose from, on this track.
    pub fn meaning_on(self, track: &Track) -> String {
        let Self::Slot { slot, .. } = self else {
            return self.info().meaning.to_owned();
        };
        let effect = track
            .inserts
            .get(usize::from(slot))
            .map_or("---", |insert| insert.name());
        format!("{effect} in slot {}", slot + 1)
    }

    /// What's on a track to choose from: what every track has, and the settings of what's in its
    /// slots, in order.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let mut targets = Self::ALL.to_vec();
        for (slot, insert) in track.inserts.iter().enumerate() {
            targets.extend((0..insert.params().len()).map(|param| Self::Slot {
                slot: slot as u8,
                param: param as u8,
            }));
        }
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::Insert;
    use crate::step::FxCommand;

    #[test]
    fn every_target_has_its_own_place() {
        let track = Track {
            inserts: [Insert::ALL[1]; INSERTS],
            ..Track::default()
        };
        let targets = Target::on_track(&track);
        for (at, target) in targets.iter().enumerate() {
            assert_eq!(target.index(), at, "{target:?}");
        }
        assert_eq!(targets.len(), Target::COUNT, "full slots use every place");
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
    fn a_track_only_offers_the_settings_of_what_is_in_its_slots() {
        let mut track = Track::default();
        assert_eq!(Target::on_track(&track), Target::ALL.to_vec());
        track.inserts[1] = Insert::ALL[1];
        let targets = Target::on_track(&track);
        assert_eq!(targets.len(), Target::ALL.len() + 4);
        assert_eq!(
            targets[Target::ALL.len()],
            Target::Slot { slot: 1, param: 0 }
        );
        let names: Vec<String> = targets[Target::ALL.len()..]
            .iter()
            .map(|target| target.name_on(&track))
            .collect();
        assert_eq!(names, ["2 Type", "2 Amount", "2 Mix", "2 Gain"]);
        assert_eq!(Target::Cut.name_on(&track), "Cut");
        // What's chosen still reads if the slot empties.
        track.inserts[1] = Insert::Empty;
        assert_eq!(Target::Slot { slot: 1, param: 2 }.name_on(&track), "2 ---");
    }

    #[test]
    fn a_saved_slot_target_comes_back() {
        let target = Target::Slot { slot: 2, param: 3 };
        let text = ron::to_string(&target).unwrap();
        assert_eq!(ron::from_str::<Target>(&text).unwrap(), target);
    }

    #[test]
    fn a_saved_target_keeps_its_name_in_a_song() {
        assert_eq!(ron::to_string(&Target::Cut).unwrap(), "Cut");
        assert_eq!(ron::from_str::<Target>("Pitch").unwrap(), Target::Pitch);
    }
}
