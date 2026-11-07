//! What can be modulated. Things that want moving register here, and sources add to a value per
//! target without knowing what any of them are.

use serde::{Deserialize, Serialize};

use crate::insert::{INSERTS, MAX_PARAMS};
use crate::lfo::LFOS_PER_TRACK;
use crate::song::{Machine, Track, TrackKind};
use crate::step::FxCommand;

/// Something an LFO or an envelope can move, in the same units as its FX command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Cut,
    Res,
    Pan,
    Vol,
    SendA,
    SendB,
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
    /// How far an LFO swings, from another moving it.
    LfoDepth {
        lfo: u8,
    },
    /// Where an LFO is in its cycle, from another moving it.
    LfoPhase {
        lfo: u8,
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
        Self::SendA,
        Self::SendB,
        Self::Pitch,
        Self::Har,
        Self::Tim,
        Self::Mor,
    ];

    /// How many places sources add to.
    pub const COUNT: usize = Self::ALL.len() + INSERTS * MAX_PARAMS + LFOS_PER_TRACK * 2;

    /// Its place among those.
    #[inline]
    pub const fn index(self) -> usize {
        match self {
            Self::Cut => 0,
            Self::Res => 1,
            Self::Pan => 2,
            Self::Vol => 3,
            Self::SendA => 4,
            Self::SendB => 5,
            Self::Pitch => 6,
            Self::Har => 7,
            Self::Tim => 8,
            Self::Mor => 9,
            Self::Slot { slot, param } => {
                Self::ALL.len() + slot as usize * MAX_PARAMS + param as usize
            }
            Self::LfoDepth { lfo } => Self::ALL.len() + INSERTS * MAX_PARAMS + lfo as usize * 2,
            Self::LfoPhase { lfo } => Self::ALL.len() + INSERTS * MAX_PARAMS + lfo as usize * 2 + 1,
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
            Self::SendA => info("Send A", "Output", "Send to return A", Some("SNA")),
            Self::SendB => info("Send B", "Output", "Send to return B", Some("SNB")),
            Self::Pitch => info("Pitch", "Machine", "Pitch, with depth in semitones", None),
            Self::Har => info("Har", "Machine", "A synth's first setting", Some("HAR")),
            Self::Tim => info("Tim", "Machine", "A synth's second setting", Some("TIM")),
            Self::Mor => info("Mor", "Machine", "A synth's third setting", Some("MOR")),
            Self::Slot { .. } => info("Setting", "FX", "A setting of what's in a slot", None),
            Self::LfoDepth { lfo: 0 } => info("L1 Depth", "LFO 1", "How far LFO 1 swings", None),
            Self::LfoPhase { lfo: 0 } => info("L1 Phase", "LFO 1", "Where LFO 1 is", None),
            Self::LfoDepth { .. } => info("L2 Depth", "LFO 2", "How far LFO 2 swings", None),
            Self::LfoPhase { .. } => info("L2 Phase", "LFO 2", "Where LFO 2 is", None),
        }
    }

    /// Which of a machine's settings this is, if it's one of them.
    const fn machine_setting(self) -> Option<usize> {
        match self {
            Self::Har => Some(0),
            Self::Tim => Some(1),
            Self::Mor => Some(2),
            _ => None,
        }
    }

    /// Whether this track has it: a machine's settings are on tracks playing that machine, and a
    /// slot's while the slot holds something with that setting. A fixed track has no sound of its
    /// own to filter or tune, and sends nowhere, and the master has no pan. Anything else does no
    /// work.
    pub fn exists_on(self, track: &Track) -> bool {
        match self {
            Self::Har | Self::Tim | Self::Mor => {
                track.is_sound() && track.machine == Machine::Plaits
            }
            Self::Cut | Self::Res | Self::SendA | Self::SendB | Self::Pitch => track.is_sound(),
            Self::Pan => track.kind != TrackKind::Master,
            Self::Slot { slot, param } => track
                .inserts
                .get(usize::from(slot))
                .is_some_and(|insert| usize::from(param) < insert.params().len()),
            _ => true,
        }
    }

    /// The name on screen, on this track, as settings depend on what's loaded. One chosen and no
    /// longer there says so.
    pub fn name_on(self, track: &Track) -> String {
        if let Some(setting) = self.machine_setting() {
            return if self.exists_on(track) {
                track.synth_voice.settings()[setting].to_owned()
            } else {
                format!("{} ---", self.info().name)
            };
        }
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

    /// What it's listed with, so each slot has its own group.
    pub fn group(self) -> String {
        match self {
            Self::Slot { slot, .. } => format!("FX {}", slot + 1),
            _ => self.info().group.to_owned(),
        }
    }

    /// What it does, for a list to choose from, on this track.
    pub fn meaning_on(self, track: &Track) -> String {
        if self.machine_setting().is_some() {
            return format!("A setting of {}", track.synth_voice.name());
        }
        let Self::Slot { slot, .. } = self else {
            return self.info().meaning.to_owned();
        };
        let effect = track
            .inserts
            .get(usize::from(slot))
            .map_or("---", |insert| insert.name());
        format!("{effect} in slot {}", slot + 1)
    }

    /// What an envelope can move: what's on the track, and either LFO's settings, as envelopes run
    /// first.
    pub fn for_envelope(track: &Track) -> Vec<Self> {
        let mut targets = Self::on_track(track);
        targets.extend(Self::lfo_settings());
        targets
    }

    /// What an LFO can move: what's on the track, and the settings of the LFOs after it, as each
    /// runs in order, so there's never a way back round.
    pub fn for_lfo(track: &Track, lfo: usize) -> Vec<Self> {
        let mut targets = Self::on_track(track);
        targets.extend(Self::lfo_settings().skip((lfo + 1) * 2));
        targets
    }

    /// Every LFO's settings, in order.
    fn lfo_settings() -> impl Iterator<Item = Self> {
        (0..LFOS_PER_TRACK as u8).flat_map(|lfo| [Self::LfoDepth { lfo }, Self::LfoPhase { lfo }])
    }

    /// What's on a track to choose from: what it has from its machine, and the settings of what's
    /// in its slots, in order.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let mut targets: Vec<Self> = Self::ALL
            .into_iter()
            .filter(|target| target.exists_on(track))
            .collect();
        for (slot, insert) in track.inserts.iter().enumerate() {
            targets.extend((0..insert.params().len()).map(|param| Self::Slot {
                slot: slot as u8,
                param: param as u8,
            }));
        }
        targets
    }
}

impl FxCommand {
    /// The name on screen, on this track: a slot's setting reads as it does in `Dest`, from what's
    /// in the slot.
    pub fn name_on(self, track: &Track) -> String {
        match self.target() {
            Some(target) if target.exists_on(track) => {
                if matches!(self, Self::Slot { .. } | Self::Har | Self::Tim | Self::Mor) {
                    target.name_on(track)
                } else {
                    self.name().to_owned()
                }
            }
            Some(target @ Target::Slot { .. }) => target.name_on(track),
            _ => self.name().to_owned(),
        }
    }

    /// What this sets, if it sets something that can be modulated.
    pub const fn target(self) -> Option<Target> {
        match self {
            Self::Vol => Some(Target::Vol),
            Self::Pan => Some(Target::Pan),
            Self::Cut => Some(Target::Cut),
            Self::Res => Some(Target::Res),
            Self::SendA => Some(Target::SendA),
            Self::SendB => Some(Target::SendB),
            Self::Har => Some(Target::Har),
            Self::Tim => Some(Target::Tim),
            Self::Mor => Some(Target::Mor),
            Self::Slot { slot, param } => Some(Target::Slot { slot, param }),
            Self::Del | Self::Kil | Self::Off | Self::Ret | Self::Cha => None,
        }
    }

    /// Whether this track has what this sets, so it belongs in a list to choose from.
    pub fn exists_on(self, track: &Track) -> bool {
        // Those with no value act on a note, and a track with no sound has none.
        self.target()
            .map_or(track.is_sound(), |target| target.exists_on(track))
    }

    /// The command for typed text: its name, or a slot's number and the start of a setting's name.
    pub fn typed(text: &str, track: &Track) -> Option<Self> {
        if let Some(command) = Self::from_name(text).filter(|command| command.name() == text) {
            return Some(command);
        }
        let slot = usize::from(text.bytes().next()?.checked_sub(b'1')?);
        let start = text.get(1..)?.to_lowercase();
        let param = track
            .inserts
            .get(slot)?
            .params()
            .iter()
            .position(|param| param.name.to_lowercase().starts_with(&start))?;
        Some(Self::Slot {
            slot: slot as u8,
            param: param as u8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::Insert;
    use crate::song::{Song, SynthVoice};

    #[test]
    fn every_target_has_its_own_place() {
        let track = Track {
            machine: Machine::Plaits,
            inserts: [Insert::ALL[1]; INSERTS],
            ..Track::default()
        };
        let targets = Target::for_envelope(&track);
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
        let mut track = Track {
            machine: Machine::Plaits,
            ..Track::default()
        };
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
    fn a_command_reads_as_its_setting_and_can_be_typed() {
        let mut track = Track::default();
        track.inserts[0] = Insert::ALL[1];
        let amount = FxCommand::Slot { slot: 0, param: 1 };
        assert_eq!(amount.name_on(&track), "1 Amount");
        assert_eq!(FxCommand::Cut.name_on(&track), "CUT");
        assert_eq!(FxCommand::typed("1AM", &track), Some(amount));
        assert_eq!(FxCommand::typed("CUT", &track), Some(FxCommand::Cut));
        // An empty slot has no settings to type, and nor does a name that isn't there.
        assert_eq!(FxCommand::typed("2AM", &track), None);
        assert_eq!(FxCommand::typed("1ZZ", &track), None);
        assert_eq!(FxCommand::typed("9AM", &track), None);
    }

    #[test]
    fn a_machines_settings_are_only_on_tracks_playing_it_under_their_real_names() {
        let mut track = Track::default();
        assert!(!Target::on_track(&track).contains(&Target::Har));
        assert!(Target::on_track(&track).contains(&Target::Pitch));
        // One chosen before switching says it isn't there, and comes back after.
        assert_eq!(Target::Har.name_on(&track), "Har ---");
        assert_eq!(FxCommand::Har.name_on(&track), "HAR");

        track.machine = Machine::Plaits;
        assert!(Target::on_track(&track).contains(&Target::Har));
        track.synth_voice = SynthVoice::Fm;
        assert_eq!(Target::Har.name_on(&track), "Ratio");
        assert_eq!(Target::Mor.name_on(&track), "Feedback");
        assert_eq!(FxCommand::Tim.name_on(&track), "Index");
        assert_eq!(FxCommand::Cut.name_on(&track), "CUT");
        track.synth_voice = SynthVoice::Kick;
        assert_eq!(Target::Har.name_on(&track), "Punch");
    }

    #[test]
    fn a_fixed_track_only_offers_what_it_has() {
        let song = Song::new(1);
        let offered = |track: &Track| Target::on_track(track);
        let (return_a, master) = (&song.tracks[1], &song.tracks[3]);
        let delay = [
            Target::Slot { slot: 0, param: 0 },
            Target::Slot { slot: 0, param: 1 },
        ];
        assert_eq!(
            offered(return_a),
            [Target::Pan, Target::Vol, delay[0], delay[1]]
        );
        assert_eq!(offered(master), [Target::Vol]);
        assert!(FxCommand::Vol.exists_on(return_a));
        assert!(
            !FxCommand::Ret.exists_on(return_a),
            "it has no note to repeat"
        );
        assert!(!FxCommand::SendA.exists_on(return_a), "no way round");
        assert!(!FxCommand::Pan.exists_on(master));
    }

    #[test]
    fn every_setting_a_command_sets_is_its_own_target() {
        for command in FxCommand::ALL {
            if let Some(target) = command.target() {
                assert!(target.index() < Target::COUNT, "{command:?}");
            }
        }
        assert_eq!(FxCommand::Del.target(), None);
    }

    #[test]
    fn lfos_can_only_reach_those_after_them_and_envelopes_reach_both() {
        let track = Track::default();
        let lfos = |list: &[Target]| {
            list.iter()
                .filter(|target| {
                    matches!(target, Target::LfoDepth { .. } | Target::LfoPhase { .. })
                })
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lfos(&Target::for_lfo(&track, 0)),
            [Target::LfoDepth { lfo: 1 }, Target::LfoPhase { lfo: 1 }]
        );
        assert!(lfos(&Target::for_lfo(&track, 1)).is_empty(), "no way back");
        assert_eq!(lfos(&Target::for_envelope(&track)).len(), 4);
        assert!(lfos(&Target::on_track(&track)).is_empty(), "nor on a step");
        assert_eq!(Target::LfoDepth { lfo: 1 }.name_on(&track), "L2 Depth");
        assert_eq!(Target::LfoPhase { lfo: 0 }.name_on(&track), "L1 Phase");
    }

    #[test]
    fn each_slot_and_lfo_is_its_own_group_in_a_row() {
        assert_eq!(Target::Cut.group(), "Filter");
        assert_eq!(Target::Slot { slot: 1, param: 0 }.group(), "FX 2");
        assert_eq!(Target::LfoPhase { lfo: 1 }.group(), "LFO 2");
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
