//! What a track offers: which settings it has now, for its mod slots to move and steps to
//! lock, and what they're called on it. A setting another voice or effect owns stays where it was
//! chosen, and does nothing until its owner is back.

use crate::insert::{INSERTS, Insert};
use crate::macros::MACROS;
use crate::modulation::{MOD_SLOTS, ModKind, note_shape_slot};
use crate::settings::{Effect, Owner, SettingId, voice_entries};
use crate::song::{Machine, Track, TrackKind};
use crate::step::FxCommand;

/// What each of the track's own settings does, in the table's order.
const TRACK_MEANINGS: [&str; 16] = [
    "Volume",
    "Pan",
    "Filter cutoff",
    "Filter resonance",
    "Send to return A",
    "Send to return B",
    "Pitch, with depth in semitones",
    "Filter mode: its slope or response",
    "Gate: how much of its last pulse a note lasts",
    "Gate length, up or down from the note's own",
    "Groove: how late or early the steps land, from this one",
    "Speed: how long the steps are, from this one",
    "Glide: how long a tied note takes to reach the next",
    "Velocity, up or down from the note's own",
    "Level: the fader, under Volume",
    "Quantise: how far the rows' nudges are brought to the grid",
];

/// The track's own settings in the order lists show them: the filter, then output, then pitch,
/// velocity, gate, time and glide.
const TRACK_ORDER: [SettingId; 16] = [
    SettingId::CUT,
    SettingId::RES,
    SettingId::FILTER_MODE,
    SettingId::PAN,
    SettingId::LEVEL,
    SettingId::VOL,
    SettingId::SEND_A,
    SettingId::SEND_B,
    SettingId::PITCH,
    SettingId::VELOCITY_DELTA,
    SettingId::GATE_LENGTH,
    SettingId::GATE_DELTA,
    SettingId::GROOVE,
    SettingId::SPEED,
    SettingId::QUANTISE,
    SettingId::GLIDE,
];

impl SettingId {
    /// Whether this track has it now: a voice's settings while it plays that voice, and an
    /// effect's while that effect is in its slot, in a mode that has it. A fixed track has no sound of its own to filter
    /// or tune, only a group of them sends, and the master has no pan.
    pub fn exists_on(self, track: &Track) -> bool {
        match self.owner() {
            (Owner::Track, _) if self == Self::VOL || self == Self::LEVEL => true,
            (Owner::Track, _) if self == Self::PAN => track.kind != TrackKind::Master,
            (Owner::Track, _) if self == Self::SEND_A || self == Self::SEND_B => {
                track.is_sound() || track.is_group()
            }
            // The MS-20's filters have no modes.
            (Owner::Track, _) if self == Self::FILTER_MODE => {
                track.is_sound() && track.filter.has_modes()
            }
            (Owner::Track, _) => track.is_sound(),
            (Owner::Voice(voice), _) => track.voice() == Some(voice),
            (Owner::Slot { slot, effect }, at) => track.inserts.get(slot).is_some_and(|insert| {
                Effect::of(*insert) == Some(effect)
                    && insert
                        .params()
                        .iter()
                        .any(|param| param.setting == effect.params()[at].setting)
            }),
            // A slot's depth and polarity do nothing on the one shaping the notes, only an LFO has
            // a deform, an LFO's phase is Steps' start, and Steps have no polarity to choose.
            (Owner::Mod(slot), 1) => {
                matches!(track.mods[slot].kind, ModKind::Lfo | ModKind::Steps)
            }
            (Owner::Mod(slot), 3) => track.mods[slot].kind == ModKind::Lfo,
            (Owner::Mod(slot), 2) if track.mods[slot].kind == ModKind::Steps => false,
            (Owner::Mod(slot), _) => note_shape_slot(&track.mods) != Some(slot),
            (Owner::Sampler, _) => track.is_sound() && track.machine == Machine::Sampler,
            (Owner::Bypass(slot), _) => track
                .inserts
                .get(slot)
                .is_some_and(|insert| *insert != Insert::Empty),
            (Owner::Macro(_), _) => true,
            (Owner::ModOwn(slot), at) => track.mods[slot].own_value(at).is_some(),
            (Owner::Accumulation | Owner::Arpeggio, _) => track.is_sound(),
        }
    }

    /// Whether a step can lock it: it has a label.
    pub fn lockable(self) -> bool {
        self.entry().label.is_some()
    }

    /// Its name on this track. One the track doesn't have now says so.
    pub fn name_on(self, track: &Track) -> String {
        let here = self.exists_on(track);
        match self.owner() {
            // A slot's setting with the effect gone says only which slot, so it fits a column.
            (Owner::Slot { slot, .. } | Owner::Bypass(slot), _) if !here => {
                format!("{} ---", slot + 1)
            }
            (Owner::Slot { slot, .. } | Owner::Bypass(slot), _) => {
                format!("{} {}", slot + 1, self.name())
            }
            (Owner::Mod(slot), _) if !here => format!("M{} {} ---", slot + 1, self.name()),
            // A Steps slot's phase is where each note starts it.
            (Owner::Mod(slot), 1) if track.mods[slot].kind == ModKind::Steps => {
                format!("M{} Start", slot + 1)
            }
            (Owner::Mod(slot), _) => format!("M{} {}", slot + 1, self.name()),
            (Owner::ModOwn(slot), _) if !here => format!("M{} {} ---", slot + 1, self.name()),
            (Owner::ModOwn(slot), _) => format!("M{} {}", slot + 1, self.name()),
            // A macro with a name, or holding something, says so after its number.
            (Owner::Macro(at), _) => match track.macros[at].shown_name() {
                Some(name) => format!("{} {name}", self.name()),
                None => self.name().to_owned(),
            },
            (Owner::Accumulation, _) => format!("ACU {}", self.name()),
            (Owner::Arpeggio, _) => format!("ARP {}", self.name()),
            _ if here => self.name().to_owned(),
            _ => format!("{} ---", self.name()),
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> String {
        match self.owner() {
            (Owner::Track, at) => TRACK_MEANINGS[at].to_owned(),
            (Owner::Voice(voice), _) => format!("A setting of {}", voice.name()),
            (Owner::Slot { slot, effect }, _) => format!("{} in slot {}", effect.name(), slot + 1),
            (Owner::Mod(slot), 0) => format!("How far Mod {} moves", slot + 1),
            (Owner::Mod(slot), 1) => format!("Where Mod {} is in its cycle, or starts", slot + 1),
            (Owner::Mod(slot), 3) => format!("How far Mod {} bends its wave", slot + 1),
            (Owner::Mod(slot), _) => {
                format!("Whether Mod {} moves one way or both", slot + 1)
            }
            (Owner::Bypass(slot), _) => format!("Switch slot {} out, or back in", slot + 1),
            (Owner::Macro(at), _) => format!("Macro {}, and every setting it turns", at + 1),
            (Owner::ModOwn(slot), at) => {
                format!("{} of Mod {}", self.name(), slot + 1)
                    + match at {
                        0..=3 => ", an envelope's",
                        4 => ", a drum's level after its peak",
                        5 => ", how far a low-pass gate closes the filter",
                        6 => ", how many Steps play",
                        7 => ", the chance a Steps value is drawn again",
                        8 => ", how Steps go from one to the next",
                        _ => ", how fast an LFO or Steps go",
                    }
            }
            (Owner::Accumulation, 0) => "How far above its note an accumulator goes".to_owned(),
            (Owner::Accumulation, _) => "How far below its note an accumulator goes".to_owned(),
            (Owner::Arpeggio, 0) => "How fast an arpeggio plays its notes".to_owned(),
            (Owner::Arpeggio, _) => "How many octaves an arpeggio climbs".to_owned(),
            (Owner::Sampler, 0) => "Which of the bank's samples plays".to_owned(),
            (Owner::Sampler, 1) => "Which of the sample's slices plays".to_owned(),
            (Owner::Sampler, 2) => "Where in the slice a note starts".to_owned(),
            (Owner::Sampler, 3) => "How much of the slice a note plays".to_owned(),
            (Owner::Sampler, _) => {
                "Once through, round its loop, ping-pong, or on through the slices after".to_owned()
            }
        }
    }

    /// What it's listed with on this track: a slot's bypass goes with the effect in the slot.
    pub fn group_on(self, track: &Track) -> String {
        match self.owner().0 {
            Owner::Bypass(slot) => Effect::of(track.inserts[slot]).map_or_else(
                || self.group(),
                |effect| {
                    SettingId::slot(slot, effect, 0).map_or_else(|| self.group(), SettingId::group)
                },
            ),
            _ => self.group(),
        }
    }

    /// What's on a track to choose from, in order: its own, its machine's, and its slots' effects',
    /// in the modes they're in, each slot's with its bypass after them.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let mut settings: Vec<Self> = TRACK_ORDER
            .into_iter()
            .filter(|id| id.exists_on(track))
            .collect();
        if let Some(voice) = track.voice() {
            settings
                .extend((0..voice_entries(voice).len()).filter_map(|at| Self::voice(voice, at)));
        }
        if Self::SAMPLE.exists_on(track) {
            settings.extend((0..).map_while(Self::sampler));
        }
        for slot in 0..INSERTS {
            if let Some(effect) = Effect::of(track.inserts[slot]) {
                settings.extend(
                    (0..effect.params().len())
                        .filter_map(|at| Self::slot(slot, effect, at))
                        .filter(|id| id.exists_on(track)),
                );
                settings.extend(Self::bypass(slot));
            }
        }
        settings
    }

    /// What a mod slot can move: what's on the track, and the depths, phases and deforms the
    /// slots after it have, as the slots run in order, so there's never a way back round. A
    /// polarity is chosen, so steps lock it but nothing moves it.
    pub fn for_mod(track: &Track, slot: usize) -> Vec<Self> {
        let mut settings = Self::to_modulate(track);
        settings.extend(mod_settings().filter(|id| {
            matches!(id.owner(), (Owner::Mod(other), 0 | 1 | 3) if other > slot)
                && id.exists_on(track)
        }));
        settings.extend(
            (slot + 1..MOD_SLOTS)
                .flat_map(|other| (0..).map_while(move |at| Self::mod_own(other, at)))
                .chain(Self::played())
                .filter(|id| id.exists_on(track)),
        );
        settings.extend(Self::macros());
        settings
    }

    /// The accumulator's and the arpeggio's, which only macros and modulation move.
    fn played() -> impl Iterator<Item = Self> {
        (0..)
            .map_while(Self::accumulation)
            .chain((0..).map_while(Self::arpeggio))
    }

    /// The track's eight macros, in order.
    pub fn macros() -> impl Iterator<Item = Self> {
        (0..MACROS).filter_map(Self::macro_at)
    }

    /// What a macro can reach on this track (decision 282): everything a step or a mod slot
    /// can, every slot's own settings and the accumulator's and arpeggio's, but never a macro.
    pub fn for_macro(track: &Track) -> Vec<Self> {
        let mut settings = Self::on_track(track);
        settings.extend(mod_settings().filter(|id| id.exists_on(track)));
        settings.extend(
            (0..MOD_SLOTS)
                .flat_map(|slot| (0..).map_while(move |at| Self::mod_own(slot, at)))
                .chain(Self::played())
                .filter(|id| id.exists_on(track)),
        );
        settings
    }

    /// Whether a mod slot can move this: it's on the track and, if it's another slot's, that slot
    /// comes after it. A slot pointing back, as after slots move, does nothing.
    pub fn reached_from(self, slot: usize, track: &Track) -> bool {
        match self.owner().0 {
            Owner::Mod(other) | Owner::ModOwn(other) if other <= slot => false,
            _ => self.exists_on(track),
        }
    }

    /// What's on a track for modulation to move. `LEV` is left out, as `Vol` is where modulation
    /// moves the volume, on top of the fader. `LDLT` is too, as moving it would only move
    /// `Gate`, which modulation already moves up or down from each note's own length, and so are
    /// `GRV`, `RATE`, `PLY` and a slot's bypass, as a groove, a speed, a way of playing and whether
    /// a slot's in are chosen rather than moved through, and `GLD`, which is read only as a tie
    /// comes.
    fn to_modulate(track: &Track) -> Vec<Self> {
        let mut settings = Self::on_track(track);
        settings.retain(|id| {
            ![
                Self::LEVEL,
                Self::GATE_DELTA,
                Self::GROOVE,
                Self::SPEED,
                Self::GLIDE,
                Self::PLAY,
            ]
            .contains(id)
                && !matches!(id.owner().0, Owner::Bypass(_))
        });
        settings
    }
}

/// Every mod slot's settings, in order.
fn mod_settings() -> impl Iterator<Item = SettingId> {
    (0..MOD_SLOTS).flat_map(|slot| (0..).map_while(move |at| SettingId::modulator(slot, at)))
}

impl FxCommand {
    /// The commands a step on this track can hold: what it can lock, its mod slots' settings that
    /// do something with their types, then the events. A track with no sound has no note for an
    /// event to act on.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let events = Self::EVENTS.into_iter().filter(|_| track.is_sound());
        SettingId::on_track(track)
            .into_iter()
            .chain(mod_settings().filter(|id| id.exists_on(track)))
            .chain(SettingId::macros())
            .filter(|id| id.lockable())
            .map(Self::Set)
            .chain(events)
            .collect()
    }

    /// Whether this track has what this sets, so it does something here.
    pub fn exists_on(self, track: &Track) -> bool {
        match self {
            Self::Set(id) => id.exists_on(track),
            _ => track.is_sound(),
        }
    }

    /// Its name on this track, for lists and the status line: a setting's long name, or an
    /// event's label.
    pub fn name_on(self, track: &Track) -> String {
        match self {
            Self::Set(id) => id.name_on(track),
            event => event.name(),
        }
    }

    /// The command a typed label names on this track, whatever its case.
    pub fn typed(text: &str, track: &Track) -> Option<Self> {
        let text = text.to_uppercase();
        Self::on_track(track)
            .into_iter()
            .chain(Self::EVENTS)
            .find(|command| command.name() == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::{DistortType, Insert};
    use crate::song::{Machine, Song, SynthVoice};

    #[test]
    fn a_track_offers_its_own_then_its_machines_then_its_slots_settings() {
        let mut track = Track {
            machine: Machine::Plaits(SynthVoice::String),
            ..Track::default()
        };
        let offered = SettingId::on_track(&track);
        let own = TRACK_ORDER.len();
        assert_eq!(offered[..own], TRACK_ORDER);
        assert_eq!(offered.len(), own + 4);
        assert_eq!(
            offered[own],
            SettingId::voice(SynthVoice::String, 0).unwrap()
        );
        track.inserts[1] = Insert::ALL[2];
        let offered = SettingId::on_track(&track);
        assert_eq!(offered.len(), own + 4 + 11 + 1);
        assert_eq!(
            offered[own + 4],
            SettingId::slot(1, Effect::Delay, 0).unwrap()
        );
        assert_eq!(
            offered[own + 4 + 11],
            SettingId::bypass(1).unwrap(),
            "the slot's bypass after its effect's"
        );
        assert_eq!(SettingId::bypass(1).unwrap().name_on(&track), "2 Bypass");
        assert_eq!(SettingId::bypass(0).unwrap().name_on(&track), "1 ---");
    }

    #[test]
    fn a_step_can_set_the_fader_but_modulation_moves_vol_instead() {
        let song = Song::new(1);
        let master = song.tracks.last().unwrap().clone();
        assert_eq!(master.kind, TrackKind::Master);
        for track in [Track::default(), master] {
            assert!(FxCommand::on_track(&track).contains(&FxCommand::Set(SettingId::LEVEL)));
            for slot in 0..MOD_SLOTS {
                let offered = SettingId::for_mod(&track, slot);
                assert!(!offered.contains(&SettingId::LEVEL));
                assert!(offered.contains(&SettingId::VOL));
            }
        }
    }

    #[test]
    fn a_step_can_move_a_notes_length_but_modulation_moves_gate_itself() {
        let track = Track::default();
        let delta = FxCommand::Set(SettingId::GATE_DELTA);
        assert!(FxCommand::on_track(&track).contains(&delta));
        assert_eq!(delta.name(), "LDLT");
        for offered in [SettingId::for_mod(&track, 3), SettingId::for_mod(&track, 0)] {
            assert!(offered.contains(&SettingId::GATE_LENGTH));
            assert!(!offered.contains(&SettingId::GATE_DELTA));
        }
    }

    #[test]
    fn velocity_is_moved_by_steps_and_modulation_alike() {
        let track = Track::default();
        let delta = FxCommand::Set(SettingId::VELOCITY_DELTA);
        assert!(FxCommand::on_track(&track).contains(&delta));
        assert_eq!(delta.name(), "VDLT");
        for offered in [SettingId::for_mod(&track, 3), SettingId::for_mod(&track, 0)] {
            assert!(offered.contains(&SettingId::VELOCITY_DELTA));
        }
        let group = Track {
            kind: TrackKind::Group(1),
            ..Track::default()
        };
        assert!(
            !SettingId::VELOCITY_DELTA.exists_on(&group),
            "a group plays no notes"
        );
    }

    #[test]
    fn quantise_is_locked_by_steps_and_moved_by_modulation() {
        let track = Track::default();
        let quantise = FxCommand::Set(SettingId::QUANTISE);
        assert!(FxCommand::on_track(&track).contains(&quantise));
        assert_eq!(quantise.name(), "QNT");
        for offered in [SettingId::for_mod(&track, 3), SettingId::for_mod(&track, 0)] {
            assert!(offered.contains(&SettingId::QUANTISE));
        }
        let group = Track {
            kind: TrackKind::Group(1),
            ..Track::default()
        };
        assert!(
            !SettingId::QUANTISE.exists_on(&group),
            "a group plays no notes"
        );
    }

    #[test]
    fn a_setting_another_voice_or_effect_owns_says_it_isnt_here() {
        let mut track = Track {
            machine: Machine::Plaits(SynthVoice::Fm),
            ..Track::default()
        };
        let ratio = SettingId::voice(SynthVoice::Fm, 0).unwrap();
        assert_eq!(ratio.name_on(&track), "Ratio");
        track.machine = Machine::Plaits(SynthVoice::Kick);
        assert!(!ratio.exists_on(&track));
        assert_eq!(ratio.name_on(&track), "Ratio ---");

        let feedback = SettingId::slot(1, Effect::Delay, 1).unwrap();
        assert_eq!(feedback.name_on(&track), "2 ---");
        track.inserts[1] = Insert::ALL[2];
        assert_eq!(feedback.name_on(&track), "2 Feedback");
        track.inserts[1] = Insert::ALL[3];
        assert!(!feedback.exists_on(&track), "a Reverb isn't the Delay");
    }

    #[test]
    fn a_distorts_modes_each_have_their_own_setting() {
        let mut track = Track::default();
        track.inserts[0] = Insert::ALL[1];
        let amount = SettingId::slot(0, Effect::Distort, 1).unwrap();
        let bits = SettingId::slot(0, Effect::Distort, 2).unwrap();
        assert_eq!(
            (amount.name_on(&track), bits.name_on(&track)),
            ("1 Amount".into(), "1 ---".into())
        );
        if let Insert::Distort(distort) = &mut track.inserts[0] {
            distort.kind = DistortType::Crush;
        }
        assert_eq!(bits.name_on(&track), "1 Bits");
        assert!(!amount.exists_on(&track), "Crush has no Amount");
        let offered = FxCommand::on_track(&track);
        assert!(offered.contains(&FxCommand::Set(bits)));
        assert!(!offered.contains(&FxCommand::Set(amount)));
        let kind = SettingId::slot(0, Effect::Distort, 0).unwrap();
        assert!(!kind.lockable(), "a mode isn't locked from a step");
        assert_eq!(FxCommand::typed("1type", &track), None);
    }

    #[test]
    fn a_group_offers_its_level_pan_and_sends() {
        let song = Song::new(1);
        let group = &song.tracks[song.fixed_track(TrackKind::Group(1)).unwrap()];
        assert_eq!(
            SettingId::on_track(group)[..5],
            [
                SettingId::PAN,
                SettingId::LEVEL,
                SettingId::VOL,
                SettingId::SEND_A,
                SettingId::SEND_B
            ]
        );
    }

    #[test]
    fn a_fixed_track_only_offers_what_it_has() {
        let song = Song::new(1);
        let return_a = &song.tracks[song.fixed_track(TrackKind::ReturnA).unwrap()];
        let master = &song.tracks[song.fixed_track(TrackKind::Master).unwrap()];
        let offered = SettingId::on_track(return_a);
        assert_eq!(
            offered[..3],
            [SettingId::PAN, SettingId::LEVEL, SettingId::VOL]
        );
        assert_eq!(
            offered.len(),
            3 + 11 + 1,
            "and its delay's settings and bypass"
        );
        assert_eq!(
            SettingId::on_track(master)[..2],
            [SettingId::LEVEL, SettingId::VOL]
        );
        assert!(!SettingId::on_track(master).contains(&SettingId::PAN));
        assert!(
            FxCommand::on_track(return_a)
                .iter()
                .all(|command| command.holds()),
            "no events without a note"
        );
    }

    #[test]
    fn steps_lock_only_what_has_a_label() {
        let track = Track::default();
        let commands = FxCommand::on_track(&track);
        assert!(commands.contains(&FxCommand::CUT));
        assert!(commands.contains(&FxCommand::Set(SettingId::PITCH)));
        assert!(commands.contains(&FxCommand::Set(SettingId::modulator(4, 0).unwrap())));
        assert!(commands.contains(&FxCommand::Del));
        assert_eq!(
            FxCommand::typed("dep5", &track),
            Some(FxCommand::Set(SettingId::modulator(4, 0).unwrap()))
        );
        assert_eq!(
            FxCommand::typed("dep2", &track),
            Some(FxCommand::Set(SettingId::modulator(1, 0).unwrap())),
            "an envelope's depth can be locked"
        );
        assert_eq!(
            FxCommand::typed("dep1", &track),
            None,
            "not the note's shape"
        );
        assert_eq!(
            FxCommand::typed("phs2", &track),
            None,
            "an envelope has no phase"
        );
    }

    #[test]
    fn a_slot_can_only_reach_those_after_it() {
        let mut track = Track::default();
        let mods = |list: Vec<SettingId>| -> Vec<String> {
            list.into_iter()
                .filter(|id| matches!(id.owner().0, Owner::Mod(_)))
                .map(SettingId::key)
                .collect()
        };
        assert_eq!(
            mods(SettingId::for_mod(&track, 3)),
            [
                "mod5.depth",
                "mod5.phase",
                "mod5.deform",
                "mod6.depth",
                "mod6.phase",
                "mod6.deform"
            ]
        );
        assert_eq!(
            mods(SettingId::for_mod(&track, 4)),
            ["mod6.depth", "mod6.phase", "mod6.deform"]
        );
        assert!(mods(SettingId::for_mod(&track, MOD_SLOTS - 1)).is_empty());
        assert_eq!(
            mods(SettingId::for_mod(&track, 0)),
            [
                "mod2.depth",
                "mod3.depth",
                "mod4.depth",
                "mod4.phase",
                "mod4.deform",
                "mod5.depth",
                "mod5.phase",
                "mod5.deform",
                "mod6.depth",
                "mod6.phase",
                "mod6.deform"
            ],
            "envelopes reach every LFO and the envelopes after them"
        );
        track.mods[4].kind = ModKind::Adsr;
        assert_eq!(
            mods(SettingId::for_mod(&track, 3)),
            ["mod5.depth", "mod6.depth", "mod6.phase", "mod6.deform"],
            "a phase and a deform only on an LFO"
        );
        let depth = |slot| SettingId::modulator(slot, 0).unwrap();
        assert!(depth(5).reached_from(3, &track));
        assert!(!depth(3).reached_from(3, &track), "not itself");
        assert!(!depth(2).reached_from(3, &track), "never back round");
        assert_eq!(depth(5).name_on(&track), "M6 Depth");
        assert_eq!(depth(0).name_on(&track), "M1 Depth ---");
        assert_eq!(depth(0).group(), "Mod 1");
    }

    #[test]
    fn a_typed_label_finds_its_command_on_the_track() {
        let track = Track {
            machine: Machine::Plaits(SynthVoice::Kick),
            ..Track::default()
        };
        assert_eq!(FxCommand::typed("cut", &track), Some(FxCommand::CUT));
        assert_eq!(FxCommand::typed("del", &track), Some(FxCommand::Del));
        assert_eq!(
            FxCommand::typed("acc", &track),
            Some(FxCommand::Set(
                SettingId::voice(SynthVoice::Kick, 3).unwrap()
            ))
        );
        assert_eq!(FxCommand::typed("xyz", &track), None);
    }

    #[test]
    fn a_saved_destination_keeps_its_key_in_a_song() {
        let mut song = Song::new(1);
        let feedback = SettingId::slot(1, Effect::Delay, 1).unwrap();
        song.tracks[0].mods[3].destination = Some(feedback);
        let text = ron::to_string(&song).unwrap();
        assert!(text.contains("2.delay.feedback"), "{text}");
        let back: Song = ron::from_str(&text).unwrap();
        assert_eq!(back.tracks[0].mods[3].destination, Some(feedback));
    }
}
