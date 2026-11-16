//! Each of the track page's settings resolved once, where it is, into what a widget draws and
//! edits it by (`docs/designs/device-interface.md` §2.5, the UI plan's U7): a [`ParamView`], from
//! [`view`], and the action that writes a value back, from [`set`]. The cells, the titles, the
//! status line, the mixer and the Roto's value text read the view, so a setting's value is worked
//! out here and nowhere else.
//!
//! The view lives in vt-tui, not the model: it's resolved from a [`Control`], which is the
//! program's, and a Gen setting's from the program's live generator, and the status line's text of
//! an accumulator's limit says degrees or semitones by the scale lock, which the program keeps
//! outside the song until it saves. What it holds is description only, with nothing of the
//! cursor, the modes or the engine's report: those are the page's, in the cell's `ParamState`.

use std::borrow::Cow;

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::{Action, TrackControl};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES, octaves_text, retrig_meaning};
use vt_model::constants::{BAR_STEPS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::generator::Generator;
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lane::{LANE_RATES, LaneMove, RATE_AS_WRITTEN, length_rows};
use vt_model::listed::Listed;
use vt_model::modulation::{MOD_SLOTS, ModField, ModKind, default_mod, note_shape_slot};
use vt_model::param::{Descriptor, HEX_JUMP, NUMBER, PERCENT, Reading, kept};
use vt_model::settings::{
    Kind, LONGEST_GLIDE_MS, SettingId, Units, gate_percent, gate_value, glide_ms, rows_text,
};
use vt_model::shape::params::Typing;
use vt_model::shape::{Context, GenParam, Where};
use vt_model::song::{
    FilterType, Machine, Phase, Role, Song, SynthVoice, Track, reset_name, trim_text,
};
use vt_model::speed::Speed;

use crate::services::banking::{sample_shown, sample_text};
use crate::services::generators::with_live;

use super::param::{Choice, Pace, ParamView, Reads, Steps};
use super::tuning::{tuning, tuning_shown, tuning_text};
use super::{AccumulatorField, ArpField, Control, LaneField, key, macro_name};

/// What a slot going to `Pitch` in scale shows as its destination.
pub(crate) const IN_SCALE_PITCH: &str = "Pitch in scale";

/// How many small steps a large one is: a percentage's and an accumulator limit's, which is an
/// octave of a seven-note scale.
const PERCENT_JUMP: i32 = 10;
const LIMIT_JUMP: i32 = 7;

impl ParamView {
    /// A setting with nothing yet but its names: a value of 0 that doesn't move, isn't a list,
    /// and reads as nothing.
    pub fn of(control: Control, track: &Track) -> Self {
        let name = control.name_on(track);
        Self {
            key: key(control, track),
            name,
            label: label(control, track),
            value: 0,
            fine: None,
            bounds: (0, 0),
            default: 0,
            nudge: Steps::NONE,
            jump: Steps::NONE,
            pace: Pace::Even,
            wraps: false,
            centred: false,
            bar: None,
            toggle: false,
            choices: Vec::new(),
            numbered: false,
            title: name,
            cell: String::new(),
            text: String::new(),
            typing: Typing::Listed,
            reads: Reads::No,
            why_not: Cow::Borrowed(""),
            units: None,
            engaged: true,
        }
    }

    /// Two hex digits from `00` to `top`, with a bar.
    fn hex(self, value: u8, top: u8, default: u8) -> Self {
        let described = Descriptor::hex(self.key, self.name, top, default);
        self.described(described, i32::from(value))
    }

    /// One of a list, by its place, one step at a time either way, going round past the ends if
    /// it `wraps`.
    fn listed(
        self,
        choices: impl IntoIterator<Item = Choice>,
        at: usize,
        default: usize,
        wraps: bool,
    ) -> Self {
        let described = Descriptor::listed(self.key, self.name, choices, default, wraps);
        self.described(described, at as i32)
    }

    /// Off or on, as `on` and `off` in words, and as a switch in its cell: `=` puts it on and
    /// `-` takes it off.
    fn switch(self, on: bool, default: bool) -> Self {
        let described = Descriptor::switch(self.key, self.name, default);
        self.described(described, i32::from(on))
    }

    /// A whole number from `low` to `high`, reading as `cell`, typed in decimal.
    fn whole(self, value: i32, bounds: (i32, i32), default: i32, cell: String) -> Self {
        let described = Descriptor::whole(self.key, self.name, bounds, default);
        Self {
            cell,
            ..self.described(described, value)
        }
    }

    fn steps(self, nudge: Steps, jump: Steps) -> Self {
        Self {
            nudge,
            jump,
            ..self
        }
    }

    /// Its steps by a pace that depends on where it is, from where it is now.
    fn paced(self, pace: Pace) -> Self {
        let (nudge, jump) = pace.steps(self.value).unwrap_or((self.nudge, self.jump));
        Self {
            nudge,
            jump,
            pace,
            ..self
        }
    }

    fn text(self, text: String) -> Self {
        Self { text, ..self }
    }

    /// Typed as `reads` says, and refused with `why_not`.
    fn reads(self, reads: Reads, why_not: impl Into<Cow<'static, str>>) -> Self {
        Self {
            reads,
            why_not: why_not.into(),
            ..self
        }
    }

    fn titled(self, title: &'static str) -> Self {
        Self { title, ..self }
    }
}

/// A number from 1, or `off` for none.
const OFF_OR: Reads = Reads::Number {
    before: None,
    after: "",
    off: true,
    from_one: false,
};

/// The titles of each mod slot's lists, as a list's title is fixed text.
const MODE_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 mode",
    "Mod 2 mode",
    "Mod 3 mode",
    "Mod 4 mode",
    "Mod 5 mode",
    "Mod 6 mode",
];
const SOURCE_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 source",
    "Mod 2 source",
    "Mod 3 source",
    "Mod 4 source",
    "Mod 5 source",
    "Mod 6 source",
];
const CURVE_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 curve",
    "Mod 2 curve",
    "Mod 3 curve",
    "Mod 4 curve",
    "Mod 5 curve",
    "Mod 6 curve",
];
const POLARITY_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 polarity",
    "Mod 2 polarity",
    "Mod 3 polarity",
    "Mod 4 polarity",
    "Mod 5 polarity",
    "Mod 6 polarity",
];
const DESTINATION_TITLES: [&str; MOD_SLOTS] = [
    "Mod 1 destination",
    "Mod 2 destination",
    "Mod 3 destination",
    "Mod 4 destination",
    "Mod 5 destination",
    "Mod 6 destination",
];

/// A setting's names in its cell: an effect's setting by its full name, as its slot's grid has
/// room, a macro by the name it's been given, and the rest by their labels.
fn label(control: Control, track: &Track) -> Cow<'static, str> {
    match control {
        Control::InsertParam(..) => Cow::Borrowed(control.name_on(track)),
        Control::Macro(at) => Cow::Owned(macro_name(&track.macros[at], at)),
        Control::Gen { param, .. } => Cow::Borrowed(param.name()),
        _ => Cow::Borrowed(control.label_on(track)),
    }
}

/// Whether a setting that's off or on, and nothing else, is on, or nothing for any other: an
/// effect's `Off`/`On` list, an envelope's `Reset`, the arpeggio's `Retrig`, the filter's
/// `Safe`, and the master's `Limiter`, which is the song's. They show as a toggle, and the status
/// line still says the word.
pub(crate) fn toggle_on(control: Control, track: &Track, song: &Song) -> Option<bool> {
    match control {
        Control::Limiter => Some(song.limiter),
        Control::Safe => Some(track.safe),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            found.is_toggle().then(|| insert.value(param) != 0)
        }
        Control::Mod(slot, ModField::Reset) => Some(track.mods[slot].envelope().reset()),
        Control::Arpeggio(ArpField::Retrig) => Some(track.arpeggio.retrig),
        _ => None,
    }
}

/// One of the page's settings on this track, as a widget draws and edits it; `scale` is whether
/// the scale lock is on, which an accumulator's limits are counted in. Nothing for a mute or a
/// solo, which are the jam's, or a setting the track hasn't: a machine's past its last, an effect's
/// past its last, or a generator that isn't there.
pub fn view(control: Control, track: &Track, song: &Song, scale: bool) -> Option<ParamView> {
    let of = ParamView::of(control, track);
    let fresh = Track::default();
    let view = match control {
        Control::Mute | Control::Solo => return None,
        Control::Setting(_) => {
            let id = control.setting_on(track)?;
            let entry = id.entry();
            let value = track.setting(id);
            let described = entry.described();
            match entry.kind {
                // A list of a machine's still steps as hex does, stopping at its last, is typed in
                // hex, and reads by the choice it's on. A slice reads as its number, with its name
                // after it.
                Kind::Stepped { .. } => {
                    let hex = Descriptor {
                        reading: Reading::Listed,
                        barred: false,
                        choices: described.choices,
                        ..Descriptor::hex(entry.key, entry.name, entry.kind.top(), entry.default)
                    };
                    let view = of.described(hex, i32::from(value));
                    ParamView {
                        numbered: id != SettingId::SLICE,
                        cell: match usize::from(value) >= view.choices.len() || id == SettingId::SLICE {
                            true => format!("{value:02X}"),
                            false => view.cell.clone(),
                        },
                        ..view
                    }
                }
                Kind::Continuous { .. } => entry_view(of, control, track, described, value),
            }
        }
        Control::Base(which) => {
            let value = track.control(which);
            ParamView {
                centred: matches!(
                    which,
                    TrackControl::Pan | TrackControl::Pitch | TrackControl::VelocityDelta
                ),
                ..of.hex(value, 0xFF, fresh.control(which))
            }
        }
        Control::Level => of.hex(track.level, 0xFF, fresh.level),
        // 0 dB is in the middle, so it shows a cut or a boost either side of it, as pan does.
        Control::Trim => ParamView {
            centred: true,
            ..of.hex(track.trim, 0xFF, fresh.trim)
        }
        .text(trim_text(track.trim)),
        Control::Macro(at) => {
            let value = track.macros[at].value;
            ParamView {
                value: i32::from(value),
                bounds: (0, 0xFF00),
                nudge: Steps::even(0x100),
                jump: Steps::even(0x1000),
                bar: Some((value >> 8) as u8),
                cell: format!("{value:04X}"),
                typing: Typing::Hex,
                reads: Reads::Hex { shift: 8 },
                engaged: !track.macros[at].maps.is_empty(),
                ..of
            }
        }
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            // An effect's value runs to `FF` on its bar, as its slot keeps it, and its bar isn't
            // drawn from the middle, whatever its entry says.
            let described = match found.kind {
                Kind::Continuous { .. } => Descriptor {
                    bounds: (0, 0xFF),
                    centred: false,
                    ..found.described()
                },
                Kind::Stepped { .. } => found.described(),
            };
            entry_view(of, control, track, described, insert.value(param))
        }
        Control::Insert(slot) => {
            let insert = track.inserts[slot];
            let at = Insert::ALL
                .iter()
                .position(|each| each.name() == insert.name())
                .unwrap_or(0);
            let empty = Insert::ALL.iter().position(|each| *each == Insert::Empty);
            let effects = Insert::ALL.iter().map(|each| Choice::of(*each));
            ParamView {
                engaged: insert != Insert::Empty,
                ..of.listed(effects, at, empty.unwrap_or(0), true)
            }
        }
        Control::Machine => listed_of(of, track.machine, fresh.machine, true),
        Control::FilterType => listed_of(of, track.filter, fresh.filter, true)
            .titled("Filter type"),
        Control::FilterMode => {
            let modes = track.filter.modes();
            let plays = track.filter.plays(track.filter_mode);
            let at = modes.iter().position(|mode| *mode == plays).unwrap_or(0);
            let list = of
                .listed(modes.iter().map(|mode| Choice::of(*mode)), at, 0, true)
                .titled("Filter mode");
            // The MS-20's have none, so it doesn't apply.
            let cell = match track.filter.has_modes() {
                true => plays.name().to_owned(),
                false => "--".to_owned(),
            };
            ParamView { cell, ..list }
        }
        Control::Role => {
            let at = place_of(track.role);
            // `Auto` says what the machine makes it.
            let roles = Role::ALL.iter().map(|role| match role {
                Role::Auto => Choice::of(*role).meaning(format!(
                    "As its machine is for: {}",
                    track.machine.role().name()
                )),
                _ => Choice::of(*role),
            });
            // `Auto` shows what it goes by, which the page dims as its default.
            ParamView {
                cell: track.role().name().to_owned(),
                ..of.listed(roles, at, place_of(Role::Auto), true)
            }
        }
        Control::Speed => listed_of(of, track.speed, Speed::Sixteenth, false),
        Control::Phase => listed_of(of, track.phase, Phase::Free, false),
        Control::Groove => {
            let plays = track.groove.unwrap_or(song.groove);
            of.listed(
                GrooveId::all().map(|groove| Choice::named(groove.name())),
                plays.index(),
                song.groove.index(),
                false,
            )
        }
        Control::Choke => of
            .whole(
                i32::from(track.choke.unwrap_or(0)),
                (0, i32::from(CHOKE_GROUPS)),
                0,
                track
                    .choke
                    .map_or_else(|| "Off".to_owned(), |group| group.to_string()),
            )
            .reads(
                OFF_OR,
                format!("Choke is a group from 1 to {CHOKE_GROUPS}, or off"),
            ),
        Control::Group => of
            .whole(
                i32::from(track.group.unwrap_or(0)),
                (0, MAX_GROUPS as i32),
                0,
                track
                    .group
                    .map_or_else(|| "Off".to_owned(), |group| format!("G{group}")),
            )
            .reads(
                Reads::Number {
                    before: Some('G'),
                    after: "",
                    off: true,
                    from_one: false,
                },
                format!("Group is a group from 1 to {MAX_GROUPS}, or off"),
            ),
        Control::GateLength => {
            let percent = i32::from(gate_percent(track.gate_length()));
            of.whole(
                percent,
                (1, 100),
                i32::from(gate_percent(fresh.gate_length())),
                format!("{percent}%"),
            )
            .steps(Steps::even(1), Steps::even(PERCENT_JUMP))
            .reads(PERCENT, "Gate is a percent from 1 to 100")
        }
        // A large step goes to the next whole bar either way.
        Control::Reset => of
            .whole(
                i32::from(track.reset.unwrap_or(0)),
                (0, i32::from(MAX_TRACK_RESET)),
                0,
                reset_name(track.reset),
            )
            .paced(Pace::Bars)
            .text(reset_shown(track.reset))
            .reads(
                Reads::Reset,
                format!(
                    "Reset is a number of steps from 1 to {MAX_TRACK_RESET}, of bars like 2bar, or off"
                ),
            ),
        Control::Glide => {
            let shown = SettingId::GLIDE
                .entry()
                .kind
                .shown(u16::from(track.glide) << 8);
            // Off, `Acid` slides by its own `Slide`, as a 303 does.
            let cell = match track.glide == 0 && track.voice() == Some(SynthVoice::Acid) {
                true => "Slide".to_owned(),
                false => shown,
            };
            ParamView {
                bar: None,
                cell,
                ..of.hex(track.glide, 0xFF, fresh.glide)
            }
            .text(glide_shown(track))
            .reads(
                Reads::Glide,
                format!("Glide is a time in ms up to {LONGEST_GLIDE_MS}, or off"),
            )
        }
        Control::Limiter => of.switch(song.limiter, true).text(on_off(song.limiter)),
        Control::Safe => of.switch(track.safe, fresh.safe).text(safe_shown(track)),
        Control::Sample => ParamView {
            value: i32::from(track.sample()),
            bounds: (0, 0xFF),
            cell: sample_text(song, track.sample()),
            text: sample_shown(song, track.sample()),
            ..of
        },
        Control::Tune | Control::Cents => {
            let tuned = tuning(song, track);
            // With no sample, there's nothing to tune.
            let cell = tuned.as_ref().map_or_else(
                || "--".to_owned(),
                |tuning| tuning_text(control, tuning.value(control)),
            );
            ParamView {
                value: tuned.map_or(0, |tuning| i32::from(tuning.value(control))),
                cell,
                text: tuning_shown(song, track, control),
                typing: Typing::Whole { signed: true },
                ..of
            }
        }
        Control::NewGenerator(_) => ParamView {
            cell: "Empty".to_owned(),
            ..of
        },
        Control::Mod(slot, field) => mod_view(of, track, slot, field),
        Control::Accumulator(field) => accumulator_view(of, track.accumulation, field, scale),
        Control::Arpeggio(field) => arpeggio_view(of, track.arpeggio, field),
        Control::Lane(slot, field) => lane_view(of, *track.lanes.get(slot)?, slot, field),
        Control::Gen {
            phrase,
            generator,
            param,
        } => with_live(song, track, phrase, generator, |generator, context| {
            gen_view(of, generator, context, param)
        })?,
    };
    let view = ParamView {
        fine: control.fine_on(track).map(|id| track.fine(id)),
        units: view
            .units
            .or_else(|| registry(control, track).and_then(|(id, _)| units(id))),
        engaged: view.engaged && !at_default(control, track) && !untuned(song, track, control),
        ..view
    };
    // A switch that's off or on, whatever it's built from.
    let view = match toggle_on(control, track, song) {
        Some(on) => ParamView {
            toggle: true,
            value: i32::from(on),
            ..view
        },
        None => view,
    };
    // The registry's settings say their values in their units.
    Some(match (view.text.is_empty(), registry(control, track)) {
        (true, Some((id, value))) => view.text(id.entry().kind.shown(value)),
        _ => view,
    })
}

/// The registry's setting a control is on this track and its value there, where the track has
/// it.
fn registry(control: Control, track: &Track) -> Option<(SettingId, u16)> {
    control
        .value_on(track)
        .filter(|(id, _)| id.exists_on(track))
}

/// A registry setting's view at its value, as its entry describes it, its text in its units read
/// with the fine part the track keeps for it. The view's own fine part is the tail's to say.
fn entry_view(
    of: ParamView,
    control: Control,
    track: &Track,
    described: Descriptor,
    value: u8,
) -> ParamView {
    let fine = registry(control, track).map(|(_, value)| value as u8);
    let view = ParamView { fine, ..of }.described(described, i32::from(value));
    ParamView { fine: None, ..view }
}

/// A registry setting's units, if it reads in any rather than by a list's names.
fn units(id: SettingId) -> Option<Units> {
    match id.entry().kind {
        Kind::Continuous { units, .. } => Some(units),
        Kind::Stepped { .. } => None,
    }
}

/// One of a [`Listed`] kind, by its place in its list.
fn listed_of<X: Listed>(of: ParamView, now: X, default: X, wraps: bool) -> ParamView {
    of.listed(
        X::ALL.iter().map(|each| Choice::of(*each)),
        place_of(now),
        place_of(default),
        wraps,
    )
}

/// Where one of a [`Listed`] kind is in its list.
fn place_of<X: Listed>(now: X) -> usize {
    X::ALL.iter().position(|each| *each == now).unwrap_or(0)
}

/// `on` or `off`.
fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.to_owned()
}

/// Whether a setting is left at its default, which matters less, so its value is dim: a track
/// following the project's groove shows what it plays, dimmed as it isn't its own.
fn at_default(control: Control, track: &Track) -> bool {
    control == Control::Groove && track.groove.is_none()
        || control == Control::GateLength && track.gate_length.is_none()
        || control == Control::Speed && track.speed == Speed::Sixteenth
        || control == Control::Reset && track.reset.is_none()
        || control == Control::Glide && track.glide == 0
        // `Free`, and on a voice whose sound has no phase, where it does nothing.
        || control == Control::Phase
            && (track.phase == Phase::Free || !track.voice().is_some_and(SynthVoice::has_phase))
        || control == Control::FilterMode && !track.filter.has_modes()
        || control == Control::Base(TrackControl::Punch) && !track.filter.has_punch()
        || control == Control::Safe && !track.filter.has_safe()
        || control == Control::Role && track.role.is_auto()
        || matches!(control, Control::Mod(slot, ModField::Destination) if track.mods[slot].destination.is_none())
        || matches!(control, Control::Lane(slot, field) if track.lanes.get(slot).is_some_and(|moved| moved.value(field.at()) == LaneMove::default().value(field.at())))
}

/// Whether a sampler's `Tune` or `Cents` is dim: with no sample to tune, or the sample's own left
/// at none.
fn untuned(song: &Song, track: &Track, control: Control) -> bool {
    matches!(control, Control::Tune | Control::Cents)
        && tuning(song, track).is_none_or(|tuning| tuning.at_rest(control))
}

/// One of a mod slot's settings, as the model describes it under its type, with the page's own:
/// each slot's titles, where it goes, the note-shaping slot's `--`, and a type only `K` picks.
fn mod_view(of: ParamView, track: &Track, slot: usize, field: ModField) -> ParamView {
    let modulator = &track.mods[slot];
    let view = match modulator.field(field) {
        Some(value) => of.described(modulator.described(slot, field), value),
        // A rate, where it goes, and a setting its type hasn't, which reads as its slot says or
        // as nothing.
        None => ParamView {
            cell: modulator.text(field).unwrap_or_else(|| "--".to_owned()),
            ..of.described(modulator.described(slot, field), 0)
        },
    };
    // The slot that shapes each note does so with its level, one way only, rather than moving
    // a value.
    let view = match field {
        ModField::Polarity | ModField::Depth if note_shape_slot(&track.mods) == Some(slot) => {
            ParamView {
                cell: "--".to_owned(),
                ..view
            }
        }
        _ => view,
    };
    match field {
        // Only `K` picks a type, as a type changes what the device is.
        ModField::Type => ParamView {
            nudge: Steps::NONE,
            jump: Steps::NONE,
            wraps: false,
            ..view
        },
        // An envelope's curve, how notes start an LFO or Steps, or what Track follows.
        ModField::Mode => view.titled(match modulator.kind {
            ModKind::Lfo | ModKind::Steps => MODE_TITLES[slot],
            ModKind::Track => SOURCE_TITLES[slot],
            _ => CURVE_TITLES[slot],
        }),
        ModField::Polarity => view.titled(POLARITY_TITLES[slot]),
        ModField::Shape => view.titled("LFO shape"),
        ModField::Loop => view.titled("Steps loop"),
        ModField::Destination => destination_view(view, track, slot),
        _ => view,
    }
}

/// Where a mod slot goes, by its place in [`destinations`].
fn destination_view(of: ParamView, track: &Track, slot: usize) -> ParamView {
    let modulator = &track.mods[slot];
    // `Pitch` comes twice, as it is and moving the note along the song's scale.
    let places = destinations(track, slot);
    let here = (modulator.destination, modulator.goes_in_scale());
    let at = places.iter().position(|place| *place == here).unwrap_or(0);
    let choices = places.iter().map(|place| match place {
        (_, true) => Choice {
            group: Cow::Owned(SettingId::PITCH.group()),
            ..Choice::named(IN_SCALE_PITCH).meaning("Moves the note along the song's scale")
        },
        (Some(destination), false) => Choice {
            group: Cow::Owned(destination.group()),
            ..Choice::named(destination.name_on(track)).meaning(destination.meaning())
        },
        (None, false) => Choice::named("---").meaning("Off"),
    });
    let default = places
        .iter()
        .position(|(destination, _)| *destination == default_mod(slot).destination)
        .unwrap_or(0);
    ParamView {
        cell: match modulator.destination {
            _ if modulator.goes_in_scale() => IN_SCALE_PITCH.to_owned(),
            Some(destination) => destination.name_on(track),
            None => "---".to_owned(),
        },
        ..of.listed(choices, at, default, true)
            .titled(DESTINATION_TITLES[slot])
    }
}

/// Where a mod slot can go on this track, by its place in `K`'s list: nowhere, then each
/// setting, with `Pitch` twice, as it is and in scale.
fn destinations(track: &Track, slot: usize) -> Vec<(Option<SettingId>, bool)> {
    let mut places = vec![(None, false)];
    for destination in SettingId::for_mod(track, slot) {
        places.push((Some(destination), false));
        if destination == SettingId::PITCH {
            places.push((Some(destination), true));
        }
    }
    places
}

/// One of the track's accumulator settings.
fn accumulator_view(
    of: ParamView,
    accumulation: Accumulation,
    field: AccumulatorField,
    scale: bool,
) -> ParamView {
    let default = Accumulation::default();
    let limit = |of: ParamView, value: u8, default: u8| {
        of.whole(
            i32::from(value),
            (1, i32::from(MOST_DEGREES)),
            i32::from(default),
            value.to_string(),
        )
        .steps(Steps::even(1), Steps::even(LIMIT_JUMP))
        .reads(
            NUMBER,
            format!("A limit is a number of degrees from 1 to {MOST_DEGREES}"),
        )
    };
    let view = match field {
        AccumulatorField::Up => limit(of, accumulation.up, default.up),
        AccumulatorField::Down => limit(of, accumulation.down, default.down),
        AccumulatorField::Moves => listed_of(of, accumulation.moves, default.moves, true),
        AccumulatorField::AtLimit => listed_of(of, accumulation.at_limit, default.at_limit, true),
        AccumulatorField::Adds => listed_of(of, accumulation.adds, default.adds, true),
        AccumulatorField::Reset => {
            let resets: Vec<Reset> = Reset::all().collect();
            let at = |reset| resets.iter().position(|each| *each == reset).unwrap_or(0);
            of.listed(
                resets
                    .iter()
                    .map(|reset| Choice::named(reset.name()).meaning(reset.meaning())),
                at(accumulation.reset),
                at(default.reset),
                false,
            )
        }
    };
    view.text(accumulator_shown(field, accumulation, scale))
}

/// One of the track's arpeggio settings.
fn arpeggio_view(of: ParamView, arpeggio: Arpeggio, field: ArpField) -> ParamView {
    let default = Arpeggio::default();
    let view = match field {
        // The rates stop at either end, as they run from the shortest note to the longest.
        ArpField::Rate => listed_of(of, arpeggio.rate, default.rate, false),
        ArpField::Mode => listed_of(of, arpeggio.mode, default.mode, true),
        ArpField::Octaves => of
            .whole(
                i32::from(arpeggio.octaves),
                (1, i32::from(MOST_OCTAVES)),
                i32::from(default.octaves),
                arpeggio.octaves.to_string(),
            )
            .reads(
                NUMBER,
                format!("Octaves is a number from 1 to {MOST_OCTAVES}"),
            ),
        // Either way turns it over.
        ArpField::Retrig => ParamView {
            wraps: true,
            choices: [false, true]
                .map(|retrig| Choice::named(on_off(retrig)).meaning(retrig_meaning(retrig)))
                .to_vec(),
            ..of.switch(arpeggio.retrig, default.retrig)
        },
    };
    view.text(arpeggio_shown(field, arpeggio))
}

/// One of an FX column's lane's settings: `Length` in rows either side of none, centred as
/// `Pitch` is, and `Rate` from its list.
fn lane_view(of: ParamView, moved: LaneMove, slot: usize, field: LaneField) -> ParamView {
    let view = match field {
        LaneField::Length => {
            let rows = length_rows(moved.length);
            ParamView {
                centred: true,
                bar: Some(moved.length),
                ..of.whole(rows, (-128, 127), 0, format!("{rows:+}"))
            }
            .steps(Steps::even(1), Steps::even(HEX_JUMP))
            .reads(NUMBER, "Length is a number of rows from -128 to +127")
        }
        LaneField::Rate => of.listed(
            LANE_RATES.iter().map(|rate| Choice::named(*rate)),
            usize::from(moved.rate),
            usize::from(RATE_AS_WRITTEN),
            false,
        ),
    };
    view.text(lane_shown(slot, field, moved))
}

/// What one of an FX column's lane's settings does, as it's set.
fn lane_shown(slot: usize, field: LaneField, moved: LaneMove) -> String {
    let column = format!("FX{}", slot + 1);
    match field {
        LaneField::Length => {
            let rows = length_rows(moved.length);
            let by = rows_text(rows);
            match rows {
                0 => format!("{by}: each phrase's lane on {column} plays its own rows"),
                1.. => {
                    format!("{by}: each phrase's lane on {column} plays {rows} more, its own again")
                }
                _ => format!(
                    "{by}: each phrase's lane on {column} plays {} fewer of its own",
                    -rows
                ),
            }
        }
        LaneField::Rate => {
            let rate = LANE_RATES[usize::from(moved.rate).min(LANE_RATES.len() - 1)];
            match moved.rate.cmp(&RATE_AS_WRITTEN) {
                std::cmp::Ordering::Equal => {
                    format!("{rate}: each row of a lane on {column} lasts its own steps")
                }
                std::cmp::Ordering::Less => format!(
                    "{rate}: each row of a lane on {column} lasts {} times its own steps, slower",
                    RATE_AS_WRITTEN + 1 - moved.rate
                ),
                std::cmp::Ordering::Greater => format!(
                    "{rate}: each row of a lane on {column} lasts its own steps over {}, faster, \
                     but a row a step is as fast as a lane goes",
                    moved.rate - RATE_AS_WRITTEN + 1
                ),
            }
        }
    }
}

/// One of a live generator's settings, as the model describes it on the shape heard, with what
/// any macro holding its settings gives them: a brief beside its bar, and in full without one.
/// How far a step moves it is measured, by stepping it as the page's keys do, as a step can stop
/// short at an end or go by a rule of the setting's own.
fn gen_view(of: ParamView, generator: &Generator, context: &Context, param: GenParam) -> ParamView {
    let shape = generator.heard();
    let described = param.described(&shape, context);
    let listed = described.wraps;
    let view = of.described(described, param.value(&shape, context));
    // How far a step moves it from here, each way, as the page's keys move it.
    let number = param.number(&shape, context);
    let moved = |sign: i16, large: bool| {
        let mut moved = shape;
        if param.step(&mut moved, context, sign, large).is_err() {
            return 0;
        }
        match (param.number(&moved, context), number) {
            (Some(to), Some(from)) => (to - from).abs(),
            _ => 0,
        }
    };
    let step = |large: bool| Steps {
        up: moved(1, large),
        down: moved(-1, large),
    };
    let (nudge, jump) = match listed {
        true => (view.nudge, view.jump),
        false => (step(false), step(true)),
    };
    let cell = match view.bar {
        Some(_) => param.brief(&shape, context),
        None => param.shown(&shape, context),
    };
    let text = match (param, shape.place) {
        (GenParam::Hits, Where::Euclid { hits, .. }) => {
            let open = context.open;
            format!("{} of the {open} rows", usize::from(hits).min(open))
        }
        _ => param.shown(&shape, context),
    };
    // A `Where` that's its column's own, and a `Vary` of `Once`, are dim, as they're set.
    let set = &generator.shape;
    let engaged = match param {
        GenParam::Where => set.place != context.default_where(),
        GenParam::Vary => set.vary != 0,
        _ => true,
    };
    ParamView {
        nudge,
        jump,
        cell,
        text,
        engaged,
        ..view
    }
}

/// Where a number is in its bounds, on a bar's 0 to 255.
pub(crate) use vt_model::param::share;

/// The action that puts one of the page's settings on the `at`th track at a value, as its view
/// counts them, kept within its bounds, with a fine part where it has one. Nothing for those
/// written by hand: a sample, its tune, a mute, a solo and a generator yet to be made; or a
/// setting the track hasn't.
pub fn set(
    control: Control,
    at: usize,
    track: &Track,
    song: &Song,
    value: i32,
    fine: Option<u8>,
) -> Option<Action> {
    let shown = view(control, track, song, false)?;
    let value = kept(i64::from(value), shown.bounds, shown.wraps);
    // Each fits what keeps it, being within its bounds.
    let byte = value as u8;
    let index = value as usize;
    let with_fine = |action: Action| match (fine, control.fine_on(track)) {
        (Some(fine), Some(setting)) => Action::Batch(vec![
            action,
            Action::SetTrackFine {
                track: at,
                setting,
                fine,
            },
        ]),
        _ => action,
    };
    Some(match control {
        Control::Mute
        | Control::Solo
        | Control::Sample
        | Control::Tune
        | Control::Cents
        | Control::NewGenerator(_) => return None,
        Control::Setting(_) => with_fine(Action::SetTrackSetting {
            track: at,
            setting: control.setting_on(track)?,
            value: byte,
        }),
        Control::Base(which) => with_fine(Action::SetTrackBase {
            track: at,
            control: which,
            value: byte,
        }),
        Control::Level => Action::SetTrackLevel {
            track: at,
            level: byte,
        },
        Control::Trim => Action::SetTrackTrim {
            track: at,
            trim: byte,
        },
        Control::Macro(place) => Action::SetMacro {
            track: at,
            at: place,
            value: value as u16,
        },
        Control::InsertParam(slot, param) => Action::SetInsert {
            track: at,
            slot,
            insert: track.inserts[slot].with_value(param, byte),
        },
        // Choosing what's already there keeps its settings.
        Control::Insert(slot) => Action::SetInsert {
            track: at,
            slot,
            insert: match Insert::ALL[index] {
                chosen if chosen.name() == track.inserts[slot].name() => track.inserts[slot],
                chosen => chosen,
            },
        },
        Control::Machine => Action::SetTrackMachine {
            track: at,
            machine: Machine::ALL[index],
        },
        Control::FilterType => Action::SetFilterType {
            track: at,
            filter: FilterType::ALL[index],
        },
        Control::FilterMode => Action::SetFilterMode {
            track: at,
            mode: *track.filter.modes().get(index)?,
        },
        Control::Role => Action::SetTrackRole {
            track: at,
            role: Role::ALL[index],
        },
        Control::Speed => Action::SetTrackSpeed {
            track: at,
            speed: Speed::from_index(index),
        },
        Control::Phase => Action::SetTrackPhase {
            track: at,
            phase: Phase::from_index(index as u8),
        },
        Control::Groove => Action::SetTrackGroove {
            track: at,
            groove: GrooveId::from_index(index),
        },
        Control::Choke => Action::SetTrackChoke {
            track: at,
            choke: (byte > 0).then_some(byte),
        },
        Control::Group => Action::SetTrackGroup {
            first: at,
            last: at,
            group: (byte > 0).then_some(byte),
        },
        Control::GateLength => Action::SetTrackGateLength {
            track: at,
            gate_length: Some(gate_value(byte)),
        },
        Control::Reset => Action::SetTrackReset {
            track: at,
            reset: (value > 0).then_some(value as u16),
        },
        Control::Glide => Action::SetTrackGlide {
            track: at,
            glide: byte,
        },
        Control::Limiter => Action::SetLimiter(value != 0),
        Control::Safe => Action::SetTrackSafe {
            track: at,
            on: value != 0,
        },
        Control::Mod(slot, field) => {
            let mut modulator = track.mods[slot];
            match field {
                ModField::Destination => {
                    (modulator.destination, modulator.in_scale) =
                        *destinations(track, slot).get(index)?;
                }
                // A rate isn't one number, and goes its own way.
                ModField::Rate => return None,
                _ => modulator.set_field(field, value),
            }
            Action::SetMod {
                track: at,
                slot,
                modulator,
            }
        }
        Control::Accumulator(field) => {
            let mut accumulation = track.accumulation;
            match field {
                AccumulatorField::Up => accumulation.up = byte,
                AccumulatorField::Down => accumulation.down = byte,
                AccumulatorField::Moves => accumulation.moves = Moves::ALL[index],
                AccumulatorField::AtLimit => accumulation.at_limit = AtLimit::ALL[index],
                AccumulatorField::Adds => accumulation.adds = Adds::ALL[index],
                AccumulatorField::Reset => accumulation.reset = Reset::all().nth(index)?,
            }
            Action::SetAccumulation {
                track: at,
                accumulation,
            }
        }
        Control::Lane(slot, field) => {
            let mut moved = *track.lanes.get(slot)?;
            match field {
                LaneField::Length => moved.length = (value + 128).clamp(0, 255) as u8,
                LaneField::Rate => moved.rate = byte,
            }
            Action::SetLaneMove {
                track: at,
                slot,
                moved,
            }
        }
        Control::Arpeggio(field) => {
            let mut arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => arpeggio.rate = ArpRate::ALL[index],
                ArpField::Mode => arpeggio.mode = ArpMode::ALL[index],
                ArpField::Octaves => arpeggio.octaves = byte,
                ArpField::Retrig => arpeggio.retrig = value != 0,
            }
            Action::SetArpeggio {
                track: at,
                arpeggio,
            }
        }
        Control::Gen {
            phrase,
            generator,
            param,
        } => {
            let shape = with_live(song, track, phrase, generator, |found, context| {
                let mut shape = found.shape;
                param.set_number(&mut shape, context, value);
                shape
            })?;
            Action::Reshape {
                track: at,
                phrase,
                generator,
                shape,
            }
        }
    })
}

/// What a track's own `Reset` does, as it's set.
fn reset_shown(reset: Option<u16>) -> String {
    let Some(steps) = reset else {
        return "Off: its chain plays on".to_owned();
    };
    let bars = usize::from(steps) / BAR_STEPS;
    let length = match bars {
        _ if !usize::from(steps).is_multiple_of(BAR_STEPS) => format!("{steps} steps"),
        1 => "bar".to_owned(),
        _ => format!("{bars} bars"),
    };
    format!("its chain starts again every {length} of the song")
}

/// What the filter's `Safe` does, as it's set, on its type.
fn safe_shown(track: &Track) -> String {
    let on = on_off(track.safe);
    if !track.filter.has_safe() {
        format!("{on}: {} never rings by itself", track.filter.name())
    } else if track.safe {
        "on: Res stops short of ringing by itself".to_owned()
    } else {
        "off: Res rings by itself from about D9".to_owned()
    }
}

/// How long a track's ties glide, and what off does on its voice.
fn glide_shown(track: &Track) -> String {
    match glide_ms(f32::from(track.glide)) {
        Some(_) => SettingId::GLIDE
            .entry()
            .kind
            .shown(u16::from(track.glide) << 8),
        None if track.voice() == Some(SynthVoice::Acid) => {
            "Off: a tie slides by Acid's own Slide".to_owned()
        }
        None => "Off: a tie jumps to the next note".to_owned(),
    }
}

/// What one of the track's accumulator settings does, as it's set.
fn accumulator_shown(field: AccumulatorField, accumulation: Accumulation, scale: bool) -> String {
    let unit = if scale { "degrees" } else { "semitones" };
    match field {
        AccumulatorField::Up => format!("{} {unit} above the note as written", accumulation.up),
        AccumulatorField::Down => {
            format!("{} {unit} below the note as written", accumulation.down)
        }
        AccumulatorField::Moves => lowered(accumulation.moves.meaning()),
        AccumulatorField::AtLimit => lowered(accumulation.at_limit.meaning()),
        AccumulatorField::Adds => lowered(accumulation.adds.meaning()),
        AccumulatorField::Reset => lowered(&accumulation.reset.meaning()),
    }
}

/// What one of the track's arpeggio settings does, as it's set.
fn arpeggio_shown(field: ArpField, arpeggio: Arpeggio) -> String {
    match field {
        ArpField::Rate => lowered(arpeggio.rate.meaning()),
        ArpField::Mode => lowered(arpeggio.mode.meaning()),
        ArpField::Octaves if arpeggio.octaves == 1 => {
            "1 octave: the notes alone, not climbing".to_owned()
        }
        ArpField::Octaves => format!(
            "{}: the notes climb that many before it starts again",
            octaves_text(arpeggio.octaves)
        ),
        ArpField::Retrig => lowered(retrig_meaning(arpeggio.retrig)),
    }
}

/// A meaning to follow a name on the status line, its first letter small.
fn lowered(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vt_model::action::edit;
    use vt_model::envelope::Hold;
    use vt_model::lfo::Shape;
    use vt_model::song::TrackKind;

    use super::*;
    use crate::controls::Page;
    use crate::input::mode::{Mode, View};
    use crate::widgets::pin::{app_with, press};

    /// A song with every machine, effect and mod type, and settings where a step's size depends on
    /// the value or the way: times either side of 10 ms and 100 ms, an AHD holding for the note
    /// and for none, an attractor's phase, a reset part way through a bar.
    fn varied() -> Song {
        let mut song = Song::new(Machine::ALL.len());
        let sound = song
            .tracks
            .iter_mut()
            .filter(|track| track.kind == TrackKind::Sound);
        for (at, track) in sound.enumerate() {
            track.machine = Machine::ALL[at % Machine::ALL.len()];
            let count = track.inserts.len();
            for (slot, insert) in track.inserts.iter_mut().enumerate() {
                *insert = Insert::ALL[(at * count + slot + 1) % Insert::ALL.len()];
            }
            let count = track.mods.len();
            for (slot, modulator) in track.mods.iter_mut().enumerate() {
                modulator.kind = ModKind::ALL[(at * count + slot) % ModKind::ALL.len()];
                let ms = [9, 10, 100, 0][slot % 4];
                (modulator.adsr.attack_ms, modulator.ahd.attack_ms) = (ms, ms);
                (modulator.adsr.decay_ms, modulator.lpg.decay_ms) = (ms + 1, ms);
                modulator.ahd.hold = [Hold::Note, Hold::Ms(0), Hold::Ms(10), Hold::Ms(99)][at % 4];
                modulator.lfo.shape = Shape::ALL[(at + slot) % Shape::ALL.len()];
                modulator.lfo.phase = [0, 120, 350, 45][slot % 4];
            }
            track.reset = [None, Some(7), Some(16), Some(17)][at % 4];
            track.glide = [0, 0x08, 0xF8][at % 3];
            track.accumulation.up = [1, 3, MOST_DEGREES][at % 3];
            track.arpeggio.octaves = 1 + at as u8 % MOST_OCTAVES;
        }
        song
    }

    /// Whether a setting's keys go their own way, so its view has no step for them to follow:
    /// a mod slot's type, which only `K` picks, a rate and a fade, which step through their
    /// kinds, and those `set` leaves to their own code.
    fn own_way(control: Control) -> bool {
        matches!(
            control,
            Control::Mod(_, ModField::Type | ModField::Rate | ModField::Fade)
                | Control::Sample
                | Control::Tune
                | Control::Cents
                | Control::Mute
                | Control::Solo
                | Control::Gen { .. }
                | Control::NewGenerator(_)
        )
    }

    #[test]
    fn each_way_a_step_sets_what_the_keys_set() {
        let (mut app, _realtime) = app_with(varied());
        app.modes.view = View::Track;
        let mut count = 0;
        for track in 0..app.services.song().tracks.len() {
            app.services.cursor.track = track;
            for page in [Page::Sound, Page::Mod, Page::Fx] {
                app.track_page.page = page;
                app.modes.mode = Mode::Track;
                let rows = app.track_page.rows(app.ctx().track_here());
                for (row, places) in rows.iter().enumerate() {
                    for (field, place) in places.iter().enumerate() {
                        if place.is_none() {
                            continue;
                        }
                        app.track_page.row = row;
                        app.track_page.field = field;
                        let control = app.track_control();
                        if own_way(control) {
                            continue;
                        }
                        let before = Arc::clone(app.services.song());
                        let here = &before.tracks[track];
                        let shown = view(control, here, &before, false).unwrap();
                        let (nudge, jump) = (shown.nudge, shown.jump);
                        for (key, by, large) in [
                            ("=", nudge.up, false),
                            ("-", -nudge.down, false),
                            ("+", jump.up, true),
                            ("_", -jump.down, true),
                        ] {
                            press(&mut app, key);
                            // A large step on a list stops at its ends.
                            let list = !shown.choices.is_empty() && !shown.toggle;
                            let to = match large && list {
                                true => (shown.value + by).clamp(shown.bounds.0, shown.bounds.1),
                                false => shown.value + by,
                            };
                            let expected = set(control, track, here, &before, to, None)
                                .and_then(|action| edit(&before, &action).ok())
                                .unwrap_or_else(|| Arc::clone(&before));
                            let (got, wanted) = (
                                format!("{:?}", app.services.song().tracks[track]),
                                format!("{:?}", expected.tracks[track]),
                            );
                            let from = got
                                .chars()
                                .zip(wanted.chars())
                                .position(|(a, b)| a != b)
                                .unwrap_or(0)
                                .saturating_sub(40);
                            assert!(
                                got == wanted,
                                "{key} on {control:?} at {}: {} against {}",
                                shown.value,
                                &got[from..(from + 120).min(got.len())],
                                &wanted[from..(from + 120).min(wanted.len())]
                            );
                            assert_eq!(app.services.song().limiter, expected.limiter);
                            if app.services.song() != &before {
                                press(&mut app, "u");
                            }
                            assert_eq!(app.services.song(), &before, "undo on {control:?}");
                            count += 1;
                        }
                    }
                }
            }
        }
        assert!(count > 1000, "{count}");
    }

    #[test]
    fn an_envelope_time_steps_finer_going_down_from_where_its_step_grows() {
        let mut track = Track::default();
        let song = Song::new(1);
        track.mods[0].kind = ModKind::Adsr;
        let at = |track: &Track| view(Control::Mod(0, ModField::Attack), track, &song, false);
        for (ms, up, down) in [
            (9, 1, 1),
            (10, 5, 1),
            (99, 5, 5),
            (100, 25, 5),
            (101, 25, 25),
        ] {
            track.mods[0].adsr.attack_ms = ms;
            let steps = at(&track).unwrap();
            assert_eq!((steps.nudge.up, steps.nudge.down), (up, down), "{ms} ms");
            assert_eq!(
                (steps.jump.up, steps.jump.down),
                (up * 10, down * 10),
                "{ms} ms"
            );
        }
    }

    #[test]
    fn an_attractors_large_step_goes_from_axis_to_axis() {
        let mut track = Track::default();
        let song = Song::new(1);
        track.mods[0].kind = ModKind::Lfo;
        let phase = Control::Mod(0, ModField::Phase);
        track.mods[0].lfo.shape = Shape::Sine;
        assert_eq!(
            view(phase, &track, &song, false).unwrap().jump,
            Steps::even(90)
        );
        let attractor = Shape::ALL.into_iter().find(|shape| shape.is_attractor());
        track.mods[0].lfo.shape = attractor.unwrap();
        let shown = view(phase, &track, &song, false).unwrap();
        assert_eq!(shown.jump, Steps::even(120));
        assert_eq!(shown.cell, "x");
    }

    #[test]
    fn a_machines_own_lists_are_numbered_and_a_slice_reads_as_its_number() {
        let song = Song::new(Machine::ALL.len());
        let song = &song;
        let numbered = song.tracks.iter().flat_map(|track| {
            (0..vt_model::settings::MAX_MACHINE_SETTINGS).filter_map(move |at| {
                let control = Control::Setting(at);
                let id = control.setting_on(track)?;
                Some((id, view(control, track, song, false)?))
            })
        });
        for (id, shown) in numbered {
            let listed = matches!(id.entry().kind, Kind::Stepped { .. });
            assert_eq!(
                shown.numbered,
                listed && id != SettingId::SLICE,
                "{}",
                id.key()
            );
            assert_eq!(shown.bar.is_some(), !listed, "{}", id.key());
        }
        let shown = view(Control::Machine, &song.tracks[0], song, false).unwrap();
        assert!(!shown.numbered && shown.choices.len() == Machine::ALL.len());
    }

    #[test]
    fn the_status_line_says_more_than_the_cell_only_where_it_has_more_to_say() {
        let song = Song::new(1);
        let track = &song.tracks[0];
        let text = |control| view(control, track, &song, false).unwrap().text;
        assert_eq!(text(Control::Base(TrackControl::Cutoff)), "20.0 kHz");
        assert_eq!(
            view(Control::Base(TrackControl::Cutoff), track, &song, false)
                .unwrap()
                .cell,
            "FF"
        );
        assert_eq!(text(Control::Groove), "");
        assert_eq!(text(Control::Reset), "Off: its chain plays on");
        assert_eq!(
            view(
                Control::Accumulator(AccumulatorField::Up),
                track,
                &song,
                true
            )
            .unwrap()
            .text,
            format!(
                "{} degrees above the note as written",
                track.accumulation.up
            )
        );
    }
}
