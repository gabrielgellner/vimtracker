//! What a track offers: which settings it has now, for LFOs and envelopes to move and steps to
//! lock, and what they're called on it. A setting another voice or effect owns stays where it was
//! chosen, and does nothing until its owner is back.

use crate::insert::INSERTS;
use crate::lfo::LFOS_PER_TRACK;
use crate::settings::{Effect, Owner, SettingId, voice_entries};
use crate::song::{Track, TrackKind};
use crate::step::FxCommand;

/// What each of the track's own settings does, in the table's order.
const TRACK_MEANINGS: [&str; 11] = [
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
];

/// The track's own settings in the order lists show them: the filter, then output, then pitch
/// and gate.
const TRACK_ORDER: [SettingId; 11] = [
    SettingId::CUT,
    SettingId::RES,
    SettingId::FILTER_MODE,
    SettingId::PAN,
    SettingId::VOL,
    SettingId::SEND_A,
    SettingId::SEND_B,
    SettingId::PITCH,
    SettingId::GATE_LENGTH,
    SettingId::GATE_DELTA,
    SettingId::GROOVE,
];

impl SettingId {
    /// Whether this track has it now: a voice's settings while it plays that voice, and an
    /// effect's while that effect is in its slot, in a mode that has it. A fixed track has no sound of its own to filter
    /// or tune, and sends nowhere, and the master has no pan.
    pub fn exists_on(self, track: &Track) -> bool {
        match self.owner() {
            (Owner::Track, _) if self == Self::VOL => true,
            (Owner::Track, _) if self == Self::PAN => track.kind != TrackKind::Master,
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
            (Owner::Lfo(_), _) => true,
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
            (Owner::Slot { slot, .. }, _) if !here => format!("{} ---", slot + 1),
            (Owner::Slot { slot, .. }, _) => format!("{} {}", slot + 1, self.name()),
            (Owner::Lfo(lfo), _) => format!("L{} {}", lfo + 1, self.name()),
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
            (Owner::Lfo(lfo), 0) => format!("How far LFO {} swings", lfo + 1),
            (Owner::Lfo(lfo), _) => format!("Where LFO {} is", lfo + 1),
        }
    }

    /// What's on a track to choose from, in order: its own, its machine's, and its slots' effects',
    /// in the modes they're in.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let mut settings: Vec<Self> = TRACK_ORDER
            .into_iter()
            .filter(|id| id.exists_on(track))
            .collect();
        if let Some(voice) = track.voice() {
            settings
                .extend((0..voice_entries(voice).len()).filter_map(|at| Self::voice(voice, at)));
        }
        for slot in 0..INSERTS {
            if let Some(effect) = Effect::of(track.inserts[slot]) {
                settings.extend(
                    (0..effect.params().len())
                        .filter_map(|at| Self::slot(slot, effect, at))
                        .filter(|id| id.exists_on(track)),
                );
            }
        }
        settings
    }

    /// What an envelope can move: what's on the track, and either LFO's settings, as envelopes run
    /// first.
    pub fn for_envelope(track: &Track) -> Vec<Self> {
        let mut settings = Self::to_modulate(track);
        settings.extend(lfo_settings());
        settings
    }

    /// What an LFO can move: what's on the track, and the settings of the LFOs after it, as each
    /// runs in order, so there's never a way back round.
    pub fn for_lfo(track: &Track, lfo: usize) -> Vec<Self> {
        let mut settings = Self::to_modulate(track);
        settings.extend(lfo_settings().skip((lfo + 1) * 2));
        settings
    }

    /// What's on a track for modulation to move. `LDLT` is left out, as moving it would only move
    /// `Gate`, which modulation already moves up or down from each note's own length, and so is
    /// `GRV`, as a groove is chosen rather than moved through.
    fn to_modulate(track: &Track) -> Vec<Self> {
        let mut settings = Self::on_track(track);
        settings.retain(|id| *id != Self::GATE_DELTA && *id != Self::GROOVE);
        settings
    }
}

/// Every LFO's settings, in order.
fn lfo_settings() -> impl Iterator<Item = SettingId> {
    (0..LFOS_PER_TRACK).flat_map(|lfo| (0..2).filter_map(move |at| SettingId::lfo(lfo, at)))
}

impl FxCommand {
    /// The commands a step on this track can hold: what it can lock, its LFOs' settings, then
    /// the events. A track with no sound has no note for an event to act on.
    pub fn on_track(track: &Track) -> Vec<Self> {
        let events = Self::EVENTS.into_iter().filter(|_| track.is_sound());
        SettingId::on_track(track)
            .into_iter()
            .chain(lfo_settings())
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
        assert_eq!(offered.len(), own + 4 + 9);
        assert_eq!(
            offered[own + 4],
            SettingId::slot(1, Effect::Delay, 0).unwrap()
        );
    }

    #[test]
    fn a_step_can_move_a_notes_length_but_modulation_moves_gate_itself() {
        let track = Track::default();
        let delta = FxCommand::Set(SettingId::GATE_DELTA);
        assert!(FxCommand::on_track(&track).contains(&delta));
        assert_eq!(delta.name(), "LDLT");
        for offered in [
            SettingId::for_lfo(&track, 0),
            SettingId::for_envelope(&track),
        ] {
            assert!(offered.contains(&SettingId::GATE_LENGTH));
            assert!(!offered.contains(&SettingId::GATE_DELTA));
        }
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
    fn a_fixed_track_only_offers_what_it_has() {
        let song = Song::new(1);
        let (return_a, master) = (&song.tracks[1], &song.tracks[3]);
        let offered = SettingId::on_track(return_a);
        assert_eq!(offered[..2], [SettingId::PAN, SettingId::VOL]);
        assert_eq!(offered.len(), 2 + 9, "and its delay's settings");
        assert_eq!(SettingId::on_track(master)[0], SettingId::VOL);
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
        assert!(commands.contains(&FxCommand::Set(SettingId::lfo(1, 0).unwrap())));
        assert!(commands.contains(&FxCommand::Del));
        assert_eq!(
            FxCommand::typed("dep2", &track),
            Some(FxCommand::Set(SettingId::lfo(1, 0).unwrap()))
        );
    }

    #[test]
    fn lfos_can_only_reach_those_after_them_and_envelopes_reach_both() {
        let track = Track::default();
        let lfos = |list: Vec<SettingId>| -> Vec<String> {
            list.into_iter()
                .filter(|id| matches!(id.owner().0, Owner::Lfo(_)))
                .map(SettingId::key)
                .collect()
        };
        assert_eq!(
            lfos(SettingId::for_lfo(&track, 0)),
            ["lfo2.depth", "lfo2.phase"]
        );
        assert!(lfos(SettingId::for_lfo(&track, 1)).is_empty());
        assert_eq!(lfos(SettingId::for_envelope(&track)).len(), 4);
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
        song.tracks[0].lfos[0].destination = Some(feedback);
        let text = ron::to_string(&song).unwrap();
        assert!(text.contains("2.delay.feedback"), "{text}");
        let back: Song = ron::from_str(&text).unwrap();
        assert_eq!(back.tracks[0].lfos[0].destination, Some(feedback));
    }
}
