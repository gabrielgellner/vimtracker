//! MIX mode: the Roto as a mixer, as the vendor's manual and its Ableton and Bitwig scripts have
//! it (`docs/designs/roto-control.md` §3 and §4.4).
//!
//! The Roto chooses what MIX mode shows, and says so: `SET MIX ALL TRACKS MODE` when MODE chooses
//! MIX or SEL is held to change what the knobs and buttons do, `SET MIX TRACK MODE` for FUNC's
//! track focus, `SET ALL TRACKS MODE` when both arrows go to the returns and the master or back,
//! `SET FIRST TRACK` as the arrows page the banks of eight, and `ROTO SELECT TRACK` as a knob is
//! touched. The program answers each with what that view shows, as Ableton's script answers it:
//! how many sends there are, the tracks of the bank with their names and colours, the track
//! chosen, then each knob's position, each button's light and whether each meter is lit.
//!
//! *   **All tracks** shows a bank of eight tracks of one list, the tracks or the returns and
//!     the master: each knob the same fader of its track, Level, Pan or a send, and each button
//!     its track's mute, solo or arm. A mute button is lit while its track is muted, as both
//!     scripts light it.
//! *   **Track focus** shows one track: Level and Pan on knobs 1 and 2, then its sends, six on
//!     the first page and eight on each after it, as Ableton's script lays them out. Its meter
//!     is the first.
//!
//! [`MixView`] is that view with no I/O of its own: it's given the tracks as [`Strip`]s, and
//! says what to send for each message from the Roto and after each change, so the link and the
//! tests drive the same code.

use crate::bytes::Label;
use crate::cc::{self, Side};
use crate::sysex::{AllTracks, CONTROLS, FromRoto, KnobMode, SwitchMode, ToRoto, TrackDetails};

/// How many sends the Roto names, `A` to `L`.
pub const MAX_SENDS: usize = 12;

/// How many sends track focus has on its first page, after Level and Pan.
const FIRST_PAGE_SENDS: usize = CONTROLS - 2;

/// What a knob moves in MIX mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fader {
    Level,
    Pan,
    /// A send, from 0 for the first.
    Send(u8),
}

/// One track as MIX mode shows it: its name and colour, where each of its faders is from 0 to
/// 16383, and its switches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Strip {
    pub name: Label,
    /// An index into [`crate::colour::SCHEMES`].
    pub colour: u8,
    /// Whether it's a group.
    pub grouped: bool,
    pub level: Option<u16>,
    /// None for a track without one, as the master has no pan.
    pub pan: Option<u16>,
    /// Each send of the song, or None where the track has none, as a return can't send.
    pub sends: Vec<Option<u16>>,
    /// Whether it's muted, or None for a track that can't be, as the master.
    pub mute: Option<bool>,
    pub solo: Option<bool>,
    /// Whether it's heard, by every mute and solo, which lights its meter.
    pub heard: bool,
}

impl Strip {
    /// Where one of its faders is, if it has it.
    pub fn position(&self, fader: Fader) -> Option<u16> {
        match fader {
            Fader::Level => self.level,
            Fader::Pan => self.pan,
            Fader::Send(send) => self.sends.get(usize::from(send)).copied().flatten(),
        }
    }

    fn details(&self, index: u16) -> TrackDetails {
        TrackDetails {
            index,
            name: self.name.clone(),
            colour: self.colour,
            grouped: self.grouped,
        }
    }

    /// How many sends it has.
    fn send_count(&self) -> usize {
        self.sends.iter().filter(|send| send.is_some()).count()
    }
}

/// What MIX mode's knobs and buttons are doing, as the Roto last said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// A bank of eight tracks: each knob the same fader of its track, each button its track's
    /// mute, solo or arm. `send` is which send the knobs move when they're on sends.
    AllTracks {
        knobs: KnobMode,
        send: u8,
        switches: SwitchMode,
    },
    /// FUNC's track focus: one track's faders, from a page.
    Focus { page: u8 },
}

impl Default for Layout {
    /// The Roto's own first view: Level on the knobs, mutes on the buttons.
    fn default() -> Self {
        Self::AllTracks {
            knobs: KnobMode::Level,
            send: 0,
            switches: SwitchMode::Mute,
        }
    }
}

/// A track's place: which list, and where in it.
pub type Place = (AllTracks, u16);

/// What to send the Roto: SysEx, then knobs to move, each by its place on the page, then the
/// buttons' lights.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refresh {
    pub messages: Vec<ToRoto>,
    pub positions: Vec<(u8, u16)>,
    pub lights: Vec<(u8, bool)>,
}

impl Refresh {
    /// The knobs' positions and the lights, as the CCs that send them, in that order.
    pub fn ccs(&self) -> Vec<[u8; 3]> {
        let mut ccs: Vec<[u8; 3]> = self
            .positions
            .iter()
            .flat_map(|(knob, position)| cc::knob_position(*knob, *position))
            .collect();
        ccs.extend(
            self.lights
                .iter()
                .map(|(button, lit)| cc::switch_light(*button, *lit)),
        );
        ccs
    }
}

/// The two lists, by [`AllTracks`].
fn list_index(list: AllTracks) -> usize {
    match list {
        AllTracks::Audio => 0,
        AllTracks::MasterReturn => 1,
    }
}

/// What MIX mode shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixView {
    layout: Layout,
    /// Which list all tracks shows.
    channels: AllTracks,
    /// The tracks and the returns with the master.
    lists: [Vec<Strip>; 2],
    /// The first track of each list's bank shown, a multiple of 8.
    first: [u16; 2],
    /// The track the Roto has chosen, which track focus shows.
    selected: Place,
    /// The sends' names, as the returns are named.
    send_names: Vec<Label>,
    /// What was last sent, so only what differs goes: each knob's position, each button's
    /// light, which meters are lit, how many sends, and each side of each meter.
    positions: [Option<u16>; CONTROLS],
    lights: [Option<bool>; CONTROLS],
    states: Option<[bool; CONTROLS]>,
    sends: Option<u8>,
    meters: [Option<u8>; 2 * CONTROLS],
}

impl Default for MixView {
    fn default() -> Self {
        Self::new()
    }
}

impl MixView {
    pub fn new() -> Self {
        Self {
            layout: Layout::default(),
            channels: AllTracks::Audio,
            lists: [Vec::new(), Vec::new()],
            first: [0; 2],
            selected: (AllTracks::Audio, 0),
            send_names: Vec::new(),
            positions: [None; CONTROLS],
            lights: [None; CONTROLS],
            states: None,
            sends: None,
            meters: [None; 2 * CONTROLS],
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Which list all tracks shows: the tracks, or the returns and the master.
    pub fn channels(&self) -> AllTracks {
        self.channels
    }

    /// The first track of the bank shown.
    pub fn first(&self) -> u16 {
        self.first[list_index(self.channels)]
    }

    pub fn selected(&self) -> Place {
        self.selected
    }

    fn list(&self, list: AllTracks) -> &[Strip] {
        &self.lists[list_index(list)]
    }

    fn strip(&self, (list, index): Place) -> Option<&Strip> {
        self.list(list).get(usize::from(index))
    }

    /// Holds these tracks, keeping each list's bank where it still is.
    fn take(&mut self, lists: [Vec<Strip>; 2], send_names: Vec<Label>) {
        self.lists = lists.map(|mut list| {
            list.truncate(usize::from(crate::bytes::PAIR_MAX));
            list
        });
        self.send_names = send_names;
        for (first, list) in self.first.iter_mut().zip(&self.lists) {
            if usize::from(*first) >= list.len() {
                let last = list.len().saturating_sub(1) as u16;
                *first = last - last % CONTROLS as u16;
            }
        }
    }

    /// Chooses a track, as the Roto's own choice or FUNC's in PLUGIN mode, which track focus
    /// shows.
    pub fn select(&mut self, place: Place) {
        self.selected = place;
    }

    /// The fader on a knob, and the track it's of, if there's one there.
    pub fn knob(&self, knob: u8) -> Option<(Place, Fader)> {
        let (place, fader) = match self.layout {
            Layout::AllTracks { knobs, send, .. } => {
                let place = (self.channels, self.first() + u16::from(knob));
                let fader = match knobs {
                    KnobMode::Level => Fader::Level,
                    KnobMode::Pan => Fader::Pan,
                    KnobMode::Send => Fader::Send(send),
                };
                (place, fader)
            }
            Layout::Focus { page } => (self.selected, focus_fader(page, knob)?),
        };
        self.strip(place)?.position(fader)?;
        Some((place, fader))
    }

    /// What a button does, and the track it's of, if there's a track there. Track focus has
    /// nothing on its buttons, as in both vendor scripts.
    pub fn button(&self, button: u8) -> Option<(Place, SwitchMode)> {
        let Layout::AllTracks { switches, .. } = self.layout else {
            return None;
        };
        let place = (self.channels, self.first() + u16::from(button));
        self.strip(place)?;
        Some((place, switches))
    }

    /// The tracks whose meters are shown, each by its meter's place: the bank's, or track focus's
    /// track on the first.
    pub fn metered(&self) -> Vec<(u8, Place)> {
        match self.layout {
            Layout::AllTracks { .. } => (0..CONTROLS as u8)
                .map(|meter| (meter, (self.channels, self.first() + u16::from(meter))))
                .filter(|(_, place)| self.strip(*place).is_some())
                .collect(),
            Layout::Focus { .. } => self
                .strip(self.selected)
                .map(|_| (0, self.selected))
                .into_iter()
                .collect(),
        }
    }

    /// The meters' CCs for each meter's left and right levels, from 0 to 127, sending only those
    /// that changed.
    pub fn meter_ccs(&mut self, levels: &[(u8, [u8; 2])]) -> Vec<[u8; 3]> {
        let mut ccs = Vec::new();
        for &(meter, [left, right]) in levels {
            for (side, level) in [(Side::Left, left), (Side::Right, right)] {
                let at = usize::from(meter % CONTROLS as u8) * 2 + usize::from(side == Side::Right);
                let level = level.min(0x7F);
                if self.meters[at] != Some(level) {
                    self.meters[at] = Some(level);
                    ccs.push(cc::meter(meter, side, level));
                }
            }
        }
        ccs
    }

    /// What to send for a message from the Roto, holding these tracks, and the track the Roto
    /// chose, if it did. Anything that isn't MIX mode's is left alone.
    pub fn receive(
        &mut self,
        message: &FromRoto,
        lists: [Vec<Strip>; 2],
        send_names: Vec<Label>,
    ) -> (Refresh, Option<Place>) {
        self.take(lists, send_names);
        match *message {
            FromRoto::SetMixAllTracksMode {
                tracks,
                knobs,
                switches,
                send,
            } => {
                self.layout = Layout::AllTracks {
                    knobs,
                    send,
                    switches,
                };
                self.channels = tracks;
                (self.push(), None)
            }
            // The page's index comes in multiples of 8, as the SysEx API has it: `00` for the
            // first page, `08` for the second.
            FromRoto::SetMixTrackMode(index) => {
                self.layout = Layout::Focus {
                    page: index / CONTROLS as u8,
                };
                (self.push(), None)
            }
            FromRoto::SetAllTracksMode(tracks) => {
                self.channels = tracks;
                (self.push(), None)
            }
            FromRoto::SetFirstTrack(first)
                if usize::from(first) < self.list(self.channels).len() =>
            {
                self.first[list_index(self.channels)] = first - first % CONTROLS as u16;
                (self.push(), None)
            }
            FromRoto::RotoSelectTrack(index)
                if usize::from(index) < self.list(self.channels).len() =>
            {
                self.selected = (self.channels, index);
                // Track focus shows the track chosen; all tracks shows the same bank.
                let refresh = match self.layout {
                    Layout::Focus { .. } => self.push(),
                    Layout::AllTracks { .. } => Refresh::default(),
                };
                (refresh, Some(self.selected))
            }
            FromRoto::RequestSendTrackNames(first) => {
                let refresh = Refresh {
                    messages: vec![self.send_track_names(first - first % CONTROLS as u8)],
                    ..Refresh::default()
                };
                (refresh, None)
            }
            _ => (Refresh::default(), None),
        }
    }

    /// Eight sends' names from the first given, as Bitwig's script sends them.
    fn send_track_names(&self, first: u8) -> ToRoto {
        let mut names: [Label; CONTROLS] = Default::default();
        for (at, name) in names.iter_mut().enumerate() {
            if let Some(send) = self.send_names.get(usize::from(first) + at) {
                *name = send.clone();
            }
        }
        ToRoto::SendTrackNames { first, names }
    }

    /// The first send of the block of names the view needs, as the SysEx API has it: the block
    /// of eight with the send the knobs move, or, in track focus, the first send of its page.
    fn names_from(&self) -> u8 {
        match self.layout {
            Layout::AllTracks { send, .. } => send - send % CONTROLS as u8,
            Layout::Focus { page: 0 } => 0,
            Layout::Focus { page } => {
                (FIRST_PAGE_SENDS + (usize::from(page) - 1) * CONTROLS).min(0x7F) as u8
            }
        }
    }

    /// The sends' count and names, as the Roto is told them on connecting, before it says which
    /// mode it's in, as Ableton's script sends them then.
    pub fn sends(&mut self, send_names: Vec<Label>) -> Vec<ToRoto> {
        self.send_names = send_names;
        let count = self.send_names.len().min(MAX_SENDS) as u8;
        vec![ToRoto::NumSendTracks(count), self.send_track_names(0)]
    }

    /// How many sends the Roto offers: the song's for all tracks, and track focus's track's own,
    /// as Bitwig's script says none for the master.
    fn send_count(&self) -> u8 {
        let count = match self.layout {
            Layout::AllTracks { .. } => self.send_names.len(),
            Layout::Focus { .. } => self.strip(self.selected).map_or(0, Strip::send_count),
        };
        count.min(MAX_SENDS) as u8
    }

    /// The track list as the Roto shows it: how many tracks, the bank, each track in it, the
    /// end, and the track chosen, as Ableton's script sends them. Track focus's track is in its
    /// own list, which is all tracks' list's unless it's a return or the master.
    fn track_list(&self) -> Vec<ToRoto> {
        let list = match self.layout {
            Layout::AllTracks { .. } => self.channels,
            Layout::Focus { .. } => self.selected.0,
        };
        let strips = self.list(list);
        let count = strips.len() as u16;
        let first = self.first[list_index(list)];
        let mut messages = vec![ToRoto::NumTracks(count), ToRoto::FirstTrack(first)];
        let bank = first..(first + CONTROLS as u16).min(count);
        messages.extend(
            bank.map(|index| ToRoto::TrackDetails(strips[usize::from(index)].details(index))),
        );
        messages.push(ToRoto::TrackDetailsEnd);
        messages.extend(
            self.strip(self.selected)
                .map(|strip| ToRoto::DawSelectTrack(strip.details(self.selected.1))),
        );
        messages
    }

    /// What each knob, button and meter of the view shows now.
    fn wanted(&self) -> Wanted {
        let mut wanted = Wanted::default();
        for knob in 0..CONTROLS as u8 {
            wanted.positions[usize::from(knob)] = self
                .knob(knob)
                .and_then(|(place, fader)| self.strip(place)?.position(fader));
        }
        match self.layout {
            Layout::AllTracks { switches, .. } => {
                for button in 0..CONTROLS {
                    let strip = self.strip((self.channels, self.first() + button as u16));
                    // As Ableton's script lights them: a mute button lit while its track is
                    // muted, and one with no track lit too, as a strip that isn't playing;
                    // the master, which can't be muted, dark.
                    wanted.lights[button] = Some(match (switches, strip) {
                        (SwitchMode::Mute, Some(strip)) => strip.mute.unwrap_or(false),
                        (SwitchMode::Mute, None) => true,
                        (SwitchMode::Solo, strip) => strip.and_then(|s| s.solo).unwrap_or(false),
                        (SwitchMode::Arm, _) => false,
                    });
                    wanted.states[button] = strip.is_some_and(|strip| strip.heard);
                }
            }
            Layout::Focus { .. } => {
                wanted.states[0] = self.strip(self.selected).is_some_and(|strip| strip.heard);
            }
        }
        wanted.sends = self.send_count();
        wanted
    }

    /// Everything the view shows, sent afresh, as when the Roto changes what it shows.
    pub fn push(&mut self) -> Refresh {
        let wanted = self.wanted();
        let mut refresh = Refresh {
            messages: vec![ToRoto::NumSendTracks(wanted.sends)],
            ..Refresh::default()
        };
        refresh.messages.extend(self.track_list());
        refresh
            .messages
            .push(self.send_track_names(self.names_from()));
        refresh
            .messages
            .push(ToRoto::SetVuMeterStates(wanted.states));
        for (knob, position) in wanted.positions.iter().enumerate() {
            if let Some(position) = position {
                refresh.positions.push((knob as u8, *position));
            }
        }
        for (button, lit) in wanted.lights.iter().enumerate() {
            if let Some(lit) = lit {
                refresh.lights.push((button as u8, *lit));
            }
        }
        self.positions = wanted.positions;
        self.lights = wanted.lights;
        self.states = Some(wanted.states);
        self.sends = Some(wanted.sends);
        // The meters start again from nothing on a new view.
        self.meters = [None; 2 * CONTROLS];
        refresh
    }

    /// Shows the tracks as they are after a change, sending only what changed: the track list
    /// if a track of the bank was renamed, recoloured, added or taken away, a knob moved if its
    /// fader did, a light if its switch did, and the meters' states. A knob `touched` isn't
    /// moved under the hand, and keeps the position last sent, so it goes where it should once
    /// it's let go if something else moved its fader meanwhile. A knob just `turned` on the Roto
    /// takes its fader's position without being sent it, as it's there already.
    pub fn refresh(
        &mut self,
        lists: [Vec<Strip>; 2],
        send_names: Vec<Label>,
        touched: [bool; CONTROLS],
        turned: [bool; CONTROLS],
    ) -> Refresh {
        let before = self.track_list();
        self.take(lists, send_names);
        let wanted = self.wanted();
        let mut refresh = Refresh::default();
        if self.sends != Some(wanted.sends) {
            refresh.messages.push(ToRoto::NumSendTracks(wanted.sends));
            self.sends = Some(wanted.sends);
        }
        let list = self.track_list();
        if list != before {
            refresh.messages.extend(list);
        }
        if self.states != Some(wanted.states) {
            refresh
                .messages
                .push(ToRoto::SetVuMeterStates(wanted.states));
            self.states = Some(wanted.states);
        }
        for knob in 0..CONTROLS {
            let Some(position) = wanted.positions[knob] else {
                continue;
            };
            if turned[knob] {
                self.positions[knob] = Some(position);
            } else if !touched[knob] && self.positions[knob] != Some(position) {
                self.positions[knob] = Some(position);
                refresh.positions.push((knob as u8, position));
            }
        }
        for button in 0..CONTROLS {
            if let Some(lit) = wanted.lights[button]
                && self.lights[button] != Some(lit)
            {
                self.lights[button] = Some(lit);
                refresh.lights.push((button as u8, lit));
            }
        }
        refresh
    }

    /// Sends a button's light again as its switch is, as after it's pressed: the Roto draws a
    /// press itself.
    pub fn relight(&mut self, button: u8) -> Option<[u8; 3]> {
        let lit = self
            .wanted()
            .lights
            .get(usize::from(button))
            .copied()
            .flatten()?;
        self.lights[usize::from(button)] = Some(lit);
        Some(cc::switch_light(button, lit))
    }
}

/// What a view shows, worked out from the tracks.
#[derive(Debug, Clone, Copy, Default)]
struct Wanted {
    positions: [Option<u16>; CONTROLS],
    lights: [Option<bool>; CONTROLS],
    states: [bool; CONTROLS],
    sends: u8,
}

/// The fader on a knob of track focus's page: Level and Pan, then six sends on the first page,
/// and eight sends on each page after it, as Ableton's script lays them out.
pub fn focus_fader(page: u8, knob: u8) -> Option<Fader> {
    let knob = usize::from(knob);
    if knob >= CONTROLS {
        return None;
    }
    let send = match (page, knob) {
        (0, 0) => return Some(Fader::Level),
        (0, 1) => return Some(Fader::Pan),
        (0, knob) => knob - 2,
        (page, knob) => FIRST_PAGE_SENDS + (usize::from(page) - 1) * CONTROLS + knob,
    };
    (send < MAX_SENDS).then_some(Fader::Send(send as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(name: &str) -> Strip {
        Strip {
            name: Label::new(name),
            level: Some(100),
            pan: Some(8192),
            sends: vec![Some(0), Some(0)],
            mute: Some(false),
            solo: Some(false),
            heard: true,
            ..Strip::default()
        }
    }

    fn lists(tracks: usize) -> [Vec<Strip>; 2] {
        let tracks = (0..tracks).map(|at| strip(&format!("T{at}"))).collect();
        let mut master = strip("Master");
        master.pan = None;
        master.sends = vec![None, None];
        master.mute = None;
        master.solo = None;
        [tracks, vec![strip("Return A"), strip("Return B"), master]]
    }

    fn names() -> Vec<Label> {
        vec![Label::new("Return A"), Label::new("Return B")]
    }

    #[test]
    fn track_focus_has_level_and_pan_then_six_sends_then_eight_a_page() {
        assert_eq!(focus_fader(0, 0), Some(Fader::Level));
        assert_eq!(focus_fader(0, 1), Some(Fader::Pan));
        assert_eq!(focus_fader(0, 2), Some(Fader::Send(0)));
        assert_eq!(focus_fader(0, 7), Some(Fader::Send(5)));
        assert_eq!(focus_fader(1, 0), Some(Fader::Send(6)));
        assert_eq!(focus_fader(1, 5), Some(Fader::Send(11)));
        assert_eq!(focus_fader(1, 6), None, "the Roto names twelve sends");
    }

    #[test]
    fn the_arrows_page_the_banks_and_both_go_to_the_returns_and_master() {
        let mut view = MixView::new();
        let (refresh, _) = view.receive(&FromRoto::SetFirstTrack(8), lists(12), names());
        assert_eq!(view.first(), 8);
        assert!(refresh.messages.contains(&ToRoto::NumTracks(12)));
        assert!(refresh.messages.contains(&ToRoto::FirstTrack(8)));
        assert_eq!(refresh.positions.len(), 4, "the last bank has four tracks");
        let (refresh, _) = view.receive(
            &FromRoto::SetAllTracksMode(AllTracks::MasterReturn),
            lists(12),
            names(),
        );
        assert!(refresh.messages.contains(&ToRoto::NumTracks(3)));
        assert!(refresh.messages.contains(&ToRoto::FirstTrack(0)));
        // The master has a level, and no switch to mute it.
        assert_eq!(
            view.knob(2).map(|(place, _)| place),
            Some((AllTracks::MasterReturn, 2))
        );
        assert!(refresh.lights.contains(&(2, false)));
        assert!(refresh.lights.contains(&(3, true)), "no track there");
    }

    #[test]
    fn only_what_changed_is_sent_and_a_touched_knob_is_left_alone() {
        let mut view = MixView::new();
        view.receive(&FromRoto::SetFirstTrack(0), lists(8), names());
        let mut changed = lists(8);
        changed[0][0].level = Some(200);
        changed[0][1].level = Some(300);
        changed[0][2].level = Some(400);
        changed[0][3].mute = Some(true);
        let mut touched = [false; CONTROLS];
        touched[1] = true;
        let mut turned = [false; CONTROLS];
        turned[2] = true;
        let refresh = view.refresh(changed.clone(), names(), touched, turned);
        assert_eq!(refresh.positions, [(0, 200)]);
        assert_eq!(refresh.lights, [(3, true)]);
        assert!(refresh.messages.is_empty());
        // Let go, the touched knob goes where its fader is.
        let refresh = view.refresh(changed, names(), [false; CONTROLS], [false; CONTROLS]);
        assert_eq!(refresh.positions, [(1, 300)]);
    }

    #[test]
    fn track_focus_pages_come_in_eights_and_bring_their_sends_names() {
        let mut view = MixView::new();
        let names: Vec<Label> = (0..10).map(|at| Label::new(&format!("R{at}"))).collect();
        let mut tracks = lists(2);
        tracks[0][0].sends = vec![Some(5); 10];
        let (refresh, _) = view.receive(&FromRoto::SetMixTrackMode(8), tracks, names);
        assert_eq!(view.layout(), Layout::Focus { page: 1 });
        assert_eq!(view.knob(0).map(|(_, fader)| fader), Some(Fader::Send(6)));
        assert!(refresh.messages.iter().any(|message| matches!(
            message,
            ToRoto::SendTrackNames { first: 6, names } if names[0].as_str() == "R6"
        )));
        let (refresh, _) = view.receive(&FromRoto::RequestSendTrackNames(9), lists(2), names_ten());
        assert!(matches!(
            refresh.messages[..],
            [ToRoto::SendTrackNames { first: 8, .. }]
        ));
    }

    fn names_ten() -> Vec<Label> {
        (0..10).map(|at| Label::new(&format!("R{at}"))).collect()
    }

    #[test]
    fn meters_are_sent_only_as_they_change() {
        let mut view = MixView::new();
        view.receive(&FromRoto::SetFirstTrack(0), lists(2), names());
        assert_eq!(
            view.metered(),
            [(0, (AllTracks::Audio, 0)), (1, (AllTracks::Audio, 1))]
        );
        assert_eq!(
            view.meter_ccs(&[(1, [100, 90])]),
            [cc::meter(1, Side::Left, 100), cc::meter(1, Side::Right, 90)]
        );
        assert!(view.meter_ccs(&[(1, [100, 90])]).is_empty());
    }
}
