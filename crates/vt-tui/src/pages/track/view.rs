//! Each of the track page's settings resolved once, where it is, into what a widget draws and
//! edits it by (`docs/designs/device-interface.md` §2.5, the UI plan's U7, #739): a
//! [`ParamView`], from [`view`], and the action that writes a value back, from [`set`]. The cells,
//! the titles, the status line, the mixer and the Roto's value text read the view, so a setting's
//! value is worked out here and nowhere else.
//!
//! The view lives in vt-tui, not the model: it's resolved from the page's [`Control`], which is the
//! program's, and a Gen setting's from the program's live generator, and the status line's text of
//! an accumulator's limit says degrees or semitones by the scale lock, which the program keeps
//! outside the song until it saves. What it holds is description only, with nothing of the
//! cursor, the modes or the engine's report: those are the page's, in the cell's `ParamState`.

use std::borrow::Cow;

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::{Action, TrackControl};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES, octaves_text, retrig_meaning};
use vt_model::constants::{BAR_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::{Hold, MAX_HOLD_MS, MAX_RATCHET};
use vt_model::generator::Generator;
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::Shape;
use vt_model::listed::Listed;
use vt_model::modulation::{
    MOD_SLOTS, ModField, ModKind, ModSlot, Polarity, default_mod, depth_text, note_shape_slot,
};
use vt_model::settings::{
    Kind, LONGEST_GLIDE_MS, SettingId, Units, gate_percent, gate_value, glide_ms,
};
use vt_model::shape::params::{Typing, Units as GenUnits};
use vt_model::shape::{Context, GenParam, Where};
use vt_model::song::{FilterType, Machine, Role, Song, SynthVoice, Track, reset_name, trim_text};
use vt_model::speed::Speed;
use vt_model::steps::{Loop, MAX_MUTATE, STEPS, StepsMode};
use vt_model::tracking::Source;

use crate::overlays::mapping::macro_name;
use crate::pages::track::draw::toggle_on;
use crate::roto::layouts::key;
use crate::services::banking::{sample_shown, sample_text};
use crate::services::generators::with_live;

use super::tuning::{tuning, tuning_shown, tuning_text};
use super::{AccumulatorField, ArpField, Control};
use crate::widgets::param::view::{Choice, Pace, ParamView, Reads, Steps};

/// What a slot going to `Pitch` in scale shows as its destination.
pub(crate) const IN_SCALE_PITCH: &str = "Pitch in scale";

/// How far a small and a large step move a phase, in degrees, and an attractor's large one, from
/// one axis to the next: `x` at 0°, `y` at 120° and `z` at 240°.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
const AXIS_JUMP: i32 = 120;
/// How many small steps a large one is: a hex value's, a percentage's, an octave's and an
/// accumulator limit's, which is an octave of a seven-note scale.
const HEX_JUMP: i32 = 0x10;
const PERCENT_JUMP: i32 = 10;
const OCTAVE: i32 = 12;
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
        Self {
            value: i32::from(value),
            bounds: (0, i32::from(top)),
            default: i32::from(default),
            nudge: Steps::even(1),
            jump: Steps::even(HEX_JUMP),
            bar: Some(value),
            cell: format!("{value:02X}"),
            typing: Typing::Hex,
            reads: Reads::Hex { shift: 0 },
            ..self
        }
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
        let choices: Vec<Choice> = choices.into_iter().collect();
        let cell = choices
            .get(at)
            .map_or_else(String::new, |choice| choice.name.clone().into_owned());
        Self {
            value: at as i32,
            bounds: (0, choices.len().saturating_sub(1) as i32),
            default: default as i32,
            nudge: Steps::even(1),
            jump: Steps::even(1),
            wraps,
            choices,
            cell,
            typing: Typing::Listed,
            ..self
        }
    }

    /// Off or on, as `on` and `off` in words, and as a switch in its cell: `=` puts it on and
    /// `-` takes it off.
    fn switch(self, on: bool, default: bool) -> Self {
        Self {
            value: i32::from(on),
            bounds: (0, 1),
            default: i32::from(default),
            nudge: Steps::even(1),
            jump: Steps::even(1),
            toggle: true,
            cell: on_off(on),
            typing: Typing::Toggled,
            ..self
        }
    }

    /// A whole number from `low` to `high`, reading as `cell`, typed in decimal.
    fn whole(self, value: i32, (low, high): (i32, i32), default: i32, cell: String) -> Self {
        Self {
            value,
            bounds: (low, high),
            default,
            nudge: Steps::even(1),
            jump: Steps::even(1),
            cell,
            typing: Typing::Whole { signed: low < 0 },
            reads: NUMBER,
            ..self
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

/// A whole number as it's typed, nothing before it or after it.
const NUMBER: Reads = Reads::Number {
    before: None,
    after: "",
    off: false,
    from_one: false,
};

/// The same followed by a percent sign, or not.
const PERCENT: Reads = Reads::Number {
    before: None,
    after: "%",
    off: false,
    from_one: false,
};

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
            let of = of.hex(value, entry.kind.top(), entry.default);
            match entry.kind {
                // A list of a machine's still steps as hex does, stopping at its last, and reads
                // by the choice it's on. A slice reads as its number, with its name after it.
                Kind::Stepped { choices, .. } => ParamView {
                    bar: None,
                    choices: choices.iter().copied().map(Choice::named).collect(),
                    numbered: id != SettingId::SLICE,
                    cell: match choices.get(usize::from(value)) {
                        Some(choice) if id != SettingId::SLICE => (*choice).to_owned(),
                        _ => of.cell.clone(),
                    },
                    ..of
                },
                Kind::Continuous { middle, .. } => ParamView {
                    centred: middle,
                    ..of
                },
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
            let found = *insert.params().get(param)?;
            let value = insert.value(param);
            if found.choices().is_empty() {
                of.hex(value, 0xFF, found.default)
            } else {
                let list = of
                    .listed(
                        found.choices().iter().copied().map(Choice::named),
                        usize::from(value),
                        usize::from(found.default),
                        found.wraps(),
                    );
                ParamView {
                    toggle: found.is_toggle(),
                    typing: if found.is_toggle() {
                        Typing::Toggled
                    } else {
                        Typing::Listed
                    },
                    ..list
                }
            }
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
        || control == Control::FilterMode && !track.filter.has_modes()
        || control == Control::Base(TrackControl::Punch) && !track.filter.has_punch()
        || control == Control::Safe && !track.filter.has_safe()
        || control == Control::Role && track.role.is_auto()
        || matches!(control, Control::Mod(slot, ModField::Destination) if track.mods[slot].destination.is_none())
}

/// Whether a sampler's `Tune` or `Cents` is dim: with no sample to tune, or the sample's own left
/// at none.
fn untuned(song: &Song, track: &Track, control: Control) -> bool {
    matches!(control, Control::Tune | Control::Cents)
        && tuning(song, track).is_none_or(|tuning| tuning.at_rest(control))
}

/// One of a mod slot's settings, under its type.
fn mod_view(of: ParamView, track: &Track, slot: usize, field: ModField) -> ParamView {
    let modulator = &track.mods[slot];
    let (adsr, ahd, drum, lpg, lfo) = (
        &modulator.adsr,
        &modulator.ahd,
        &modulator.drum,
        &modulator.lpg,
        &modulator.lfo,
    );
    let steps = &modulator.steps;
    let ahd_kind = modulator.kind == ModKind::Ahd;
    let lpg_kind = modulator.kind == ModKind::Lpg;
    let ms = |value: u16| format!("{value} ms");
    // The slot as it starts under its type, with Track's map of the source it follows.
    let mut fresh = ModSlot {
        kind: modulator.kind,
        ..default_mod(slot)
    };
    if matches!(field, ModField::Low | ModField::High) {
        fresh.track.source = modulator.track.source;
    }
    let value = modulator.field(field);
    let of = ParamView {
        value: value.unwrap_or(0),
        bounds: modulator.bounds(field).unwrap_or((0, 0)),
        default: fresh.field(field).unwrap_or(0),
        nudge: Steps::even(1),
        jump: Steps::even(HEX_JUMP),
        typing: Typing::Whole { signed: false },
        ..of
    };
    let list = |of: ParamView| ParamView {
        jump: Steps::even(1),
        wraps: true,
        typing: Typing::Listed,
        ..of
    };
    // A signed amount in hex, as a depth is.
    let signed = |of: ParamView, why: &'static str| {
        ParamView {
            typing: Typing::Whole { signed: true },
            ..of
        }
        .reads(Reads::HexWithin { signed: true }, why)
    };
    let times = format!("Times are milliseconds from 0 to {MAX_ENVELOPE_MS}");
    // An envelope's times step finer while they're short.
    let time = |of: ParamView| of.paced(Pace::Time).reads(NUMBER, times.clone());
    let shape_slot = note_shape_slot(&track.mods) == Some(slot);
    match field {
        ModField::Type => ParamView {
            nudge: Steps::NONE,
            jump: Steps::NONE,
            choices: ModKind::ALL.iter().map(|kind| Choice::of(*kind)).collect(),
            cell: modulator.kind.name().to_owned(),
            typing: Typing::Listed,
            ..of
        },
        // An envelope's curve, how notes start an LFO or Steps, or what Track follows.
        ModField::Mode => list(ParamView {
            choices: modulator
                .kind
                .modes()
                .into_iter()
                .map(|(name, meaning)| Choice::named(name).meaning(meaning))
                .collect(),
            cell: modulator.mode_name().unwrap_or("--").to_owned(),
            title: match modulator.kind {
                ModKind::Lfo | ModKind::Steps => MODE_TITLES[slot],
                ModKind::Track => SOURCE_TITLES[slot],
                _ => CURVE_TITLES[slot],
            },
            ..of
        }),
        // The note-shaping slot's level is the note's, one way only.
        ModField::Polarity => list(ParamView {
            choices: Polarity::ALL.iter().map(|each| Choice::of(*each)).collect(),
            cell: match shape_slot {
                true => "--".to_owned(),
                false => modulator.polarity().name().to_owned(),
            },
            title: POLARITY_TITLES[slot],
            ..of
        }),
        ModField::Destination => {
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
        // The slot that shapes each note does so with its level, rather than moving a value.
        ModField::Depth => signed(
            ParamView {
                cell: match shape_slot {
                    true => "--".to_owned(),
                    false => depth_text(modulator.depth),
                },
                ..of
            },
            "Depth is hex from -FF to +FF",
        ),
        ModField::Attack if ahd_kind => time(ParamView {
            cell: ms(ahd.attack_ms),
            ..of
        }),
        ModField::Attack if lpg_kind => time(ParamView {
            cell: ms(lpg.attack_ms),
            ..of
        }),
        ModField::Decay if lpg_kind => time(ParamView {
            cell: ms(lpg.decay_ms),
            ..of
        }),
        ModField::Colour => ParamView {
            cell: format!("{:02X}", lpg.colour),
            typing: Typing::Hex,
            ..of
        }
        .reads(
            Reads::HexWithin { signed: false },
            "Colour is hex from 00 to FF",
        ),
        ModField::Attack => time(ParamView {
            cell: ms(adsr.attack_ms),
            ..of
        }),
        ModField::Decay if ahd_kind => time(ParamView {
            cell: ms(ahd.decay_ms),
            ..of
        }),
        ModField::Decay if modulator.kind == ModKind::Drum => time(ParamView {
            cell: ms(drum.decay_ms),
            ..of
        }),
        ModField::Peak => time(ParamView {
            cell: ms(drum.peak_ms),
            ..of
        }),
        ModField::Body => ParamView {
            cell: format!("{}%", drum.body),
            jump: Steps::even(PERCENT_JUMP),
            ..of
        }
        .reads(PERCENT, "Body is a percent from 0 to 100"),
        ModField::Ratchet => ParamView {
            cell: drum.ratchet.to_string(),
            jump: Steps::even(1),
            ..of
        }
        .reads(
            NUMBER,
            format!("Ratchet is how many strikes, from 1 to {MAX_RATCHET}"),
        ),
        ModField::Decay => time(ParamView {
            cell: ms(adsr.decay_ms),
            ..of
        }),
        // `Note` is below no time, so `-` from 0 ms holds for the note.
        ModField::Hold => ParamView {
            cell: match ahd.hold {
                Hold::Note => "Note".to_owned(),
                Hold::Ms(hold) => ms(hold),
            },
            ..of
        }
        .paced(Pace::Hold)
        .reads(
            Reads::Hold,
            format!("Hold is milliseconds from 0 to {MAX_HOLD_MS}, or note"),
        ),
        ModField::Sustain => ParamView {
            cell: format!("{}%", adsr.sustain),
            jump: Steps::even(PERCENT_JUMP),
            ..of
        }
        .reads(PERCENT, "Sustain is a percent from 0 to 100"),
        ModField::Release => time(ParamView {
            cell: ms(adsr.release_ms),
            ..of
        }),
        ModField::Reset => list(ParamView {
            cell: on_off(match modulator.kind {
                ModKind::Ahd => ahd.reset,
                ModKind::Lpg => lpg.reset,
                _ => adsr.reset,
            }),
            typing: Typing::Toggled,
            ..of
        }),
        // The attractors say what they are, and Rössler's name is found without its umlaut.
        ModField::Shape => list(ParamView {
            choices: Shape::ALL
                .iter()
                .map(|shape| match shape {
                    Shape::Lorenz => {
                        Choice::of(*shape).meaning("chaos round two wings, never quite repeating")
                    }
                    Shape::Rossler => Choice::of(*shape).meaning("Rossler's chaos round one loop"),
                    _ => Choice::of(*shape),
                })
                .collect(),
            cell: lfo.shape.name().to_owned(),
            title: "LFO shape",
            ..of
        }),
        // A rate steps through its own kinds, a division or hertz. One a note, Steps have no
        // rate.
        ModField::Rate => ParamView {
            nudge: Steps::NONE,
            jump: Steps::NONE,
            cell: match (modulator.kind, steps.mode) {
                (ModKind::Steps, StepsMode::Note) => "--".to_owned(),
                (ModKind::Steps, _) => steps.rate.to_string(),
                _ => lfo.rate.to_string(),
            },
            typing: Typing::Rate,
            ..of
        },
        ModField::Length => ParamView {
            cell: steps.length.to_string(),
            jump: Steps::even(1),
            ..of
        }
        .reads(
            NUMBER,
            format!("Length is how many steps, from 1 to {STEPS}"),
        ),
        // Typed from 1, as the page shows it.
        ModField::Start => ParamView {
            cell: (steps.start + 1).to_string(),
            jump: Steps::even(1),
            ..of
        }
        .reads(
            Reads::Number {
                before: None,
                after: "",
                off: false,
                from_one: true,
            },
            format!("Start is a step, from 1 to {STEPS}"),
        ),
        ModField::Loop => list(ParamView {
            choices: Loop::ALL
                .iter()
                .map(|each| Choice::named(each.to_string()).meaning(each.meaning()))
                .collect(),
            cell: steps.looping.to_string(),
            title: "Steps loop",
            ..of
        }),
        ModField::Mutate => ParamView {
            cell: format!("{}%", steps.mutate),
            jump: Steps::even(PERCENT_JUMP),
            ..of
        }
        .reads(
            PERCENT,
            format!("Mutate is a percent from 0 to {MAX_MUTATE}"),
        ),
        ModField::Smooth => signed(
            ParamView {
                cell: depth_text(steps.smooth),
                ..of
            },
            "Smooth is hex from -FF to +FF",
        ),
        // A large step is an octave, as a step's value on `Pitch` is semitones.
        ModField::Step(at) => signed(
            ParamView {
                cell: depth_text(steps.values[usize::from(at)]),
                jump: Steps::even(OCTAVE),
                ..of
            },
            "A step is hex from -FF to +FF",
        ),
        // An attractor's is the axis heard, named where it's one alone, and its large step goes
        // from one axis to the next.
        ModField::Phase => {
            let attractor = lfo.shape.is_attractor();
            let cell = match lfo.phase {
                0 if attractor => "x".to_owned(),
                120 if attractor => "y".to_owned(),
                240 if attractor => "z".to_owned(),
                phase => format!("{phase}°"),
            };
            ParamView {
                nudge: Steps::even(PHASE_NUDGE),
                jump: Steps::even(if attractor { AXIS_JUMP } else { PHASE_JUMP }),
                wraps: true,
                cell,
                ..of
            }
            .reads(NUMBER, "Phase is degrees from 0 to 359")
        }
        ModField::Deform => signed(
            ParamView {
                cell: depth_text(lfo.deform),
                ..of
            },
            "Deform is hex from -FF to +FF",
        ),
        // A fade steps through its own kinds.
        ModField::Fade => ParamView {
            nudge: Steps::NONE,
            jump: Steps::NONE,
            cell: lfo.fade.to_string(),
            ..of
        },
        // A large step on a map of notes is an octave.
        ModField::Low | ModField::High => {
            let tracking = &modulator.track;
            let bounds = tracking.bounds();
            let end = if field == ModField::Low {
                bounds.low
            } else {
                bounds.high
            };
            let source = tracking.source;
            let notes = source == Source::Note;
            let what = Control::Mod(slot, field).label();
            let (lowest, highest) = source.span();
            let (lowest, highest) = (source.text(lowest), source.text(highest));
            let (reads, why) = match notes {
                true => (
                    Reads::Note,
                    format!("{what} is a note from {lowest} to {highest}"),
                ),
                false => (
                    Reads::HexWithin { signed: false },
                    format!("{what} is hex from {lowest} to {highest}"),
                ),
            };
            ParamView {
                jump: Steps::even(if notes { OCTAVE } else { HEX_JUMP }),
                cell: tracking.source.text(end),
                typing: if notes {
                    Typing::Named { list: false }
                } else {
                    Typing::Hex
                },
                ..of
            }
            .reads(reads, why)
        }
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

/// One of a live generator's settings, as it's heard, with what any macro holding its settings
/// gives them (decision 294): a brief beside its bar, and in full without one.
fn gen_view(of: ParamView, generator: &Generator, context: &Context, param: GenParam) -> ParamView {
    let shape = generator.heard();
    let choices = param.choices(context);
    let units = param.units();
    let bounds = param.bounds(context);
    let number = param.number(&shape, context);
    let bar = match (units, bounds, number) {
        (GenUnits::Named, ..) => None,
        (_, Some(bounds), Some(number)) => Some(share(number, bounds)),
        _ => None,
    };
    // How far a step moves it from here, each way, as the page's keys move it.
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
    let listed = choices.is_some();
    let step = |large| match listed {
        true => Steps::even(1),
        false => Steps {
            up: moved(1, large),
            down: moved(-1, large),
        },
    };
    let cell = match bar {
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
    let value = param
        .chosen(&shape, context)
        .map(|at| at as i32)
        .or(number)
        .unwrap_or(0);
    ParamView {
        value,
        bounds: bounds.unwrap_or((value, value)),
        default: value,
        nudge: step(false),
        jump: step(true),
        wraps: listed,
        centred: matches!(units, GenUnits::Bend | GenUnits::Interval),
        bar,
        choices: choices
            .unwrap_or_default()
            .into_iter()
            .map(|choice| Choice::named(choice.name()))
            .collect(),
        cell,
        text,
        typing: param.typing(context.column),
        engaged,
        ..of
    }
}

/// Where a number is in its bounds, on a bar's 0 to 255.
pub(crate) fn share(number: i32, (low, high): (i32, i32)) -> u8 {
    let share = f64::from(number - low) / f64::from((high - low).max(1));
    (share * 255.0).round().clamp(0.0, 255.0) as u8
}

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
    let (low, high) = shown.bounds;
    let value = match shown.wraps && high > low {
        true => low + (value - low).rem_euclid(high - low + 1),
        false => value.clamp(low, high),
    };
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
    use vt_model::song::TrackKind;

    use super::*;
    use crate::input::mode::{Mode, View};
    use crate::pages::track::Page;
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
                        for (key, by) in [
                            ("=", nudge.up),
                            ("-", -nudge.down),
                            ("+", jump.up),
                            ("_", -jump.down),
                        ] {
                            press(&mut app, key);
                            let expected =
                                set(control, track, here, &before, shown.value + by, None)
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
