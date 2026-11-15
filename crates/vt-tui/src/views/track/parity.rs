//! TEMPORARY (U7, #739): the track page's values as they were worked out before `view`, kept only
//! to check the new path draws every cell and title, says every status line and gives the mixer
//! and the Roto the same text. Removed with the old path in the next commit.

#![allow(clippy::all)]

use std::borrow::Cow;
use std::sync::Arc;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_engine::link::Realtime;
use vt_engine::track_state::LiveValues;
use vt_model::accumulator::Accumulation;
use vt_model::action::TrackControl;
use vt_model::arpeggio::{Arpeggio, octaves_text, retrig_meaning};
use vt_model::constants::BAR_STEPS;
use vt_model::envelope::Hold;
use vt_model::insert::Insert;
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::{ModKind, depth_text, note_shape_slot};
use vt_model::settings::{Kind, SettingId, gate_percent, glide_ms};
use vt_model::shape::params::Units;
use vt_model::shape::{Context, GenParam, Where};
use vt_model::song::{
    Machine, Phrase, Role, Song, SynthVoice, Track, TrackKind, reset_name, trim_text,
};
use vt_model::speed::Speed;
use vt_model::step::Step;
use vt_model::steps::StepsMode;

use crate::app::App;
use crate::automating::{as_heard, fader_of};
use crate::banking::{sample_shown, sample_text};
use crate::input::mode::{Mode, View};
use crate::macroing::macro_name;
use crate::mixer_page::MIXER;
use crate::page::Ctx;
use crate::shaping::{LiveGen, live_shown};
use crate::theme;
use crate::track_page::tuning::{TuneOf, Tuning, tuning, tuning_shown, tuning_text};
use crate::track_page::{
    AccumulatorField, ArpField, Control, DeviceSpec, ModField, Page, TrackPage,
};
use crate::views::widgets::grid::{CellParts, CellSource, CellWidths};
use crate::views::widgets::group;
use crate::views::widgets::param::{Face, ParamCell, ParamState, Place, Tone, heard, heard_byte};
use crate::views::widgets::pin::{app_with, press};
use crate::views::widgets::setting::{self, CellLook, Focus};

use super::cells::TrackCells;
use super::{EMPTY_SLOT, macro_range, toggle_on};

/// How wide a value is on the Sound page, its coarse digits and its fine ones, as `A000`.
const VALUE_WIDTH: usize = 4;
/// How wide `Tune`'s value is before whose it is, as `-48 st` and two spaces.
const TUNE_WIDTH: usize = 8;
/// What comes before a mod slot's destination in its title, so `LFO  Trig  → Cut` doesn't read
/// as "Trig Cut".
const TO: &str = "→ ";

/// The track page's cells, read from the track as the page draws it.
pub(super) struct OldCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    /// The track with what its macros give its settings, and on the Sound page its faders where
    /// the steps have them while they drive them.
    track: &'a Track,
    /// The generator a Gen device's cells are of, with what it works from.
    generator: Option<(&'a LiveGen, Context<'a>)>,
}

impl<'a> OldCells<'a> {
    pub(super) fn new(page: &'a TrackPage, ctx: &'a Ctx<'a>, track: &'a Track) -> Self {
        Self {
            page,
            ctx,
            track,
            generator: None,
        }
    }

    /// The cells of one generator's device.
    pub(super) fn of_generator(self, live: &'a LiveGen) -> Self {
        Self {
            generator: Some((live, live.context())),
            ..self
        }
    }

    /// A setting's cell.
    fn param(&self, control: Control) -> ParamCell<'a> {
        let ctx = self.ctx;
        let here = control == self.page.control_here(ctx);
        let place = self.place(control);
        let face = self.face(control, place.is_some());
        let fine = matches!(face, Face::HexFine { .. }) && self.page.fine;
        let state = ParamState {
            heard: place.map_or(0, |place| self.heard(control, place.set)),
            fader: fader_of(control).is_some(),
            macro_range: self.macro_range(control),
            tone: self.tone(control),
            taken: ctx.taken_over(ctx.cursor().track, control),
            after: self.after(control),
        };
        ParamCell {
            name: self.name(control),
            face,
            text: self.text(control, place.is_some()),
            place,
            state,
            look: ctx.look_on(control, Focus::at(here, fine)),
        }
    }

    /// A device's title controls, each in its own style, the cursor's colour on the one it's on.
    pub(super) fn titles(&self, device: &DeviceSpec) -> Vec<Line<'static>> {
        let current = self.page.control_here(self.ctx);
        device
            .title
            .iter()
            .flatten()
            .map(|&control| {
                let look = self
                    .ctx
                    .look_on(control, Focus::at(control == current, false));
                self.title(control, look)
            })
            .collect()
    }

    /// A title control, as the modes show it this way: on a slot, its effect, `Empty` when there's
    /// none, and its type; a machine and its type, the sample a sampler plays or the choice a
    /// synth's is on; the filter's type and mode; a mod slot's type, mode and destination, after
    /// an arrow; a generator's fill, its `Where` and its `Vary`. One that's left at its default,
    /// an empty slot or a destination of `---` is dim.
    fn title(&self, control: Control, look: CellLook<'a>) -> Line<'static> {
        let track = self.track;
        let empty =
            matches!(control, Control::Insert(slot) if track.inserts[slot] == Insert::Empty);
        let nowhere = goes_nowhere(control, track);
        let default = empty || nowhere || at_default(control, track) || self.gen_default(control);
        let tone = if default { Tone::Default } else { Tone::Plain };
        let cell = ParamCell {
            name: Cow::Borrowed(""),
            face: Face::Word,
            text: self.title_text(control),
            place: None,
            state: ParamState {
                macro_range: self.macro_range(control),
                ..ParamState::new(tone)
            },
            look,
        };
        let mut line = cell.value();
        if matches!(control, Control::Mod(_, ModField::Destination)) {
            let arrow = setting::resting(nowhere);
            line.spans.insert(0, Span::styled(TO, arrow));
        }
        line
    }

    /// A title control's text, as the zoom's strip has it, with no cursor: an empty slot's
    /// `Empty`, a generator's setting as it's set, or the control's word.
    pub(super) fn title_text(&self, control: Control) -> String {
        match (control, &self.generator) {
            (Control::Insert(slot), _) if self.track.inserts[slot] == Insert::Empty => {
                EMPTY_SLOT.to_owned()
            }
            (Control::Gen { param, .. }, Some((live, context))) => {
                param.shown(&live.generator.shape, context)
            }
            _ => self.word(control),
        }
    }

    /// A generator's `Where` that's its column's own, and a `Vary` of `Once`, which are dim.
    fn gen_default(&self, control: Control) -> bool {
        let (Control::Gen { param, .. }, Some((live, context))) = (control, &self.generator) else {
            return false;
        };
        let shape = &live.generator.shape;
        match param {
            GenParam::Where => shape.place == context.default_where(),
            GenParam::Vary => shape.vary == 0,
            _ => false,
        }
    }

    /// What names a setting: an effect's setting by its full name, a macro by the name it's been
    /// given, and the rest by their labels.
    fn name(&self, control: Control) -> Cow<'a, str> {
        let track = self.track;
        match control {
            Control::InsertParam(..) => Cow::Borrowed(control.name_on(track)),
            Control::Macro(at) => Cow::Owned(macro_name(&track.macros[at], at)),
            Control::Gen { param, .. } => Cow::Borrowed(param.name()),
            _ => Cow::Borrowed(control.label_on(track)),
        }
    }

    /// How a setting's value reads: a toggle, a value with a fine part, hex digits with a bar,
    /// a number in its units, or a word.
    fn face(&self, control: Control, barred: bool) -> Face {
        let track = self.track;
        if let Some(on) = toggle_on(control, track, self.ctx.song()) {
            return Face::Toggle { on };
        }
        if let Some(id) = control.fine_on(track) {
            return Face::HexFine {
                fine: track.fine(id),
            };
        }
        match control {
            Control::Gen { .. }
            | Control::Mod(..)
            | Control::Accumulator(_)
            | Control::Arpeggio(_) => Face::Number,
            _ if barred => Face::Hex,
            _ => Face::Word,
        }
    }

    /// A setting's value as text: a generator's brief beside its bar and in full without one, a
    /// macro where it's heard, a machine's list by the choice it's on, and the rest as the status
    /// line has them.
    fn text(&self, control: Control, barred: bool) -> String {
        match (control, &self.generator) {
            // As it's heard, with what any macro holding its settings gives them (decision 294).
            (Control::Gen { param, .. }, Some((live, context))) => {
                let shape = &live.generator.heard();
                match barred {
                    true => param.brief(shape, context),
                    false => param.shown(shape, context),
                }
            }
            (Control::Macro(at), _) => format!("{:04X}", self.macro_values(at).1),
            // A slice is its number, with its name after it.
            _ if control.setting_on(self.track) == Some(SettingId::SLICE) => {
                old_value(control, self.track, self.ctx.song())
            }
            _ => self.word(control),
        }
    }

    /// A setting's value as the status line has it, or a machine's list by the choice it's on.
    fn word(&self, control: Control) -> String {
        choice_name(control, self.track)
            .unwrap_or_else(|| old_value(control, self.track, self.ctx.song()))
    }

    /// Where a setting with a bar is set along it: an effect's setting that isn't a list, a macro,
    /// a generator's setting in its bounds, or one of the Sound page's levels.
    fn place(&self, control: Control) -> Option<Place> {
        let track = self.track;
        let plain = |set| {
            Some(Place {
                set,
                centred: false,
            })
        };
        match control {
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                if !insert.params().get(param)?.choices().is_empty() {
                    return None;
                }
                plain(insert.value(param))
            }
            Control::Macro(at) => plain((track.macros[at].value >> 8) as u8),
            Control::Gen { param, .. } => {
                let (live, context) = self.generator.as_ref()?;
                if matches!(param.units(), Units::Named) {
                    return None;
                }
                let bounds = param.bounds(context)?;
                let number = param.number(&live.generator.heard(), context)?;
                Some(Place {
                    set: share(number, bounds),
                    centred: matches!(param.units(), Units::Bend | Units::Interval),
                })
            }
            _ => bar_value(control, track).map(|(set, centred)| Place { set, centred }),
        }
    }

    /// What a setting with a bar is heard at now, set where it's set: from the engine's report,
    /// moved by what the setting has moved since (#421). A generator's bar shows where it's set.
    fn heard(&self, control: Control, set: u8) -> u8 {
        let (live, report) = (self.ctx.live(), self.ctx.reported_track());
        match control {
            Control::InsertParam(slot, param) => {
                let insert = self.track.inserts[slot];
                // Only from a report of the same effect, as another's settings mean something
                // else.
                let then = report
                    .map(|track| track.with_macros().inserts[slot])
                    .filter(|then| std::mem::discriminant(then) == std::mem::discriminant(&insert));
                heard_byte(
                    set,
                    live.map(|live| live.inserts[slot][param]),
                    then.map(|then| then.value(param)),
                )
            }
            Control::Macro(at) => (self.macro_values(at).1 >> 8) as u8,
            Control::Gen { .. } => set,
            _ => heard_byte(
                set,
                live.and_then(|live| live_value(control, &live)),
                report
                    .and_then(|track| bar_value(control, &track.with_macros()))
                    .map(|(then, _)| then),
            ),
        }
    }

    /// Where a macro is set, and where it's heard, which a lock or modulation may move it from.
    fn macro_values(&self, at: usize) -> (u16, u16) {
        let set = self.track.macros[at].value;
        let live = self.ctx.live().map(|live| live.macros[at]);
        let then = self.ctx.reported_track().map(|then| then.macros[at].value);
        (set, heard(set, live, then))
    }

    /// The range a macro holding a setting can take it over, on its bar's 0 to 255. Only a bar
    /// shows it.
    fn macro_range(&self, control: Control) -> Option<(u8, u8)> {
        let Control::Gen { param, .. } = control else {
            return macro_range(control, self.track);
        };
        let (live, context) = self.generator.as_ref()?;
        let bounds = param.bounds(context)?;
        let target = MacroTarget::Gen {
            column: GenColumn::of(live.generator.column),
            param,
        };
        let ctx = self.ctx;
        let (_, map) = ctx
            .song()
            .tracks
            .get(ctx.cursor().track)?
            .macro_holding(target)?;
        Some((
            share(map.number_at(0.0), bounds),
            share(map.number_at(255.0), bounds),
        ))
    }

    /// How a setting's value is coloured while the cursor isn't on it: a track's role in the
    /// role's colour, set back while it's `Auto`, the group it's in in the group's, a fader the
    /// steps drive in the playing colour, and one left at its default dim. A macro that holds
    /// nothing is dim.
    fn tone(&self, control: Control) -> Tone {
        let (ctx, track) = (self.ctx, self.track);
        if let Control::Macro(at) = control {
            return match track.macros[at].maps.is_empty() {
                true => Tone::Default,
                false => Tone::Plain,
            };
        }
        let role = theme::role(track.role()).filter(|_| control == Control::Role);
        if let Some(colour) = role {
            let auto = track.role.is_auto();
            return Tone::Tint(Style::new().fg(if auto { theme::dimmed(colour) } else { colour }));
        }
        match group::style(ctx.song(), ctx.cursor().track) {
            Some(tint) if control == Control::Group => Tone::Tint(tint),
            _ if ctx.follows_steps(ctx.cursor().track, control) => Tone::Automated,
            _ if at_default(control, track) || untuned(ctx, track, control) => Tone::Default,
            _ => Tone::Plain,
        }
    }

    /// What comes after a value where its bar would be: whose `Tune` is, or a slice's name.
    fn after(&self, control: Control) -> Option<(Span<'static>, usize)> {
        let (ctx, track) = (self.ctx, self.track);
        if control == Control::Tune {
            return tune_owner(ctx, track).map(|owner| (owner, TUNE_WIDTH));
        }
        (control.setting_on(track) == Some(SettingId::SLICE)).then(|| {
            let name = Span::styled(ctx.slice_choice(track), Style::new().fg(theme::MAUVE));
            (name, VALUE_WIDTH + 1)
        })
    }
}

impl CellSource for OldCells<'_> {
    type Id = Control;

    fn cell(&self, control: Control, widths: CellWidths) -> CellParts {
        self.param(control).parts(widths)
    }
}

/// Where a number is in its bounds, on a bar's 0 to 255.
fn share(number: i32, (low, high): (i32, i32)) -> u8 {
    let share = f64::from(number - low) / f64::from((high - low).max(1));
    (share * 255.0).round().clamp(0.0, 255.0) as u8
}

/// What a control with a bar is playing with right now.
fn live_value(control: Control, live: &LiveValues) -> Option<u8> {
    match control {
        Control::Level => Some(live.level),
        Control::Base(TrackControl::Pan) => Some(live.pan),
        Control::Setting(setting) => live.machine.get(setting).copied(),
        Control::Base(TrackControl::Cutoff) => Some(live.cutoff),
        Control::Base(TrackControl::Resonance) => Some(live.resonance),
        Control::Base(TrackControl::SendA) => Some(live.send_a),
        Control::Base(TrackControl::SendB) => Some(live.send_b),
        _ => None,
    }
}

/// The choice a machine's setting that's a list is on, such as the sampler's `Play` or 6-op's
/// patch.
fn choice_name(control: Control, track: &Track) -> Option<String> {
    let id = control.setting_on(track)?;
    let Kind::Stepped { choices, .. } = id.entry().kind else {
        return None;
    };
    choices
        .get(usize::from(track.setting(id)))
        .map(|choice| (*choice).to_owned())
}

/// The value one of the Sound page's settings shows a bar for, and how. True is a mark either side
/// of a centre.
fn bar_value(control: Control, track: &Track) -> Option<(u8, bool)> {
    let level = |value| Some((value, false));
    match control {
        Control::Level => level(track.level),
        // 0 dB is in the middle, so it shows a cut or a boost either side of it, as pan does.
        Control::Trim => Some((track.trim, true)),
        Control::Base(TrackControl::Pan) => Some((track.pan, true)),
        Control::Base(TrackControl::Pitch) => Some((track.pitch, true)),
        Control::Base(TrackControl::VelocityDelta) => Some((track.velocity_delta, true)),
        // Those with a middle that's a setting of its own show where they are around it.
        Control::Setting(_) => {
            let id = control.setting_on(track)?;
            match id.entry().kind {
                Kind::Continuous { middle: false, .. } => level(track.setting(id)),
                Kind::Continuous { middle: true, .. } => Some((track.setting(id), true)),
                Kind::Stepped { .. } => None,
            }
        }
        Control::Base(TrackControl::Cutoff) => level(track.cutoff),
        Control::Base(TrackControl::Resonance) => level(track.resonance),
        Control::Base(TrackControl::SendA) => level(track.send_a),
        Control::Base(TrackControl::SendB) => level(track.send_b),
        Control::Base(TrackControl::Drive) => level(track.drive),
        Control::Base(TrackControl::Punch) => level(track.punch),
        Control::Base(TrackControl::LowCut) => level(track.low_cut),
        Control::Base(TrackControl::HighCut) => level(track.high_cut),
        Control::Base(TrackControl::Quantise) => level(track.quantise),
        _ => None,
    }
}

/// Whether a sampler's `Tune` or `Cents` is dim: with no sample to tune, or the sample's own left
/// at none.
fn untuned(ctx: &Ctx, track: &Track, control: Control) -> bool {
    matches!(control, Control::Tune | Control::Cents)
        && tuning(ctx.song(), track).is_none_or(|tuning| tuning.at_rest(control))
}

/// Whose `Tune` is, after its value where a bar would be, on a sample with slices: the slice's
/// own, `slice 03`, in the slices' colour, or the sample's, dim.
fn tune_owner(ctx: &Ctx, track: &Track) -> Option<Span<'static>> {
    let tuning = tuning(ctx.song(), track).filter(Tuning::sliced)?;
    Some(match tuning.of {
        TuneOf::Slice(slice) => {
            Span::styled(format!("slice {slice:02X}"), Style::new().fg(theme::MAUVE))
        }
        TuneOf::Sample => Span::styled("sample", theme::dim()),
    })
}

/// Whether a control is a destination that's set to nothing, `---`, which dims it as an empty
/// slot's `Empty` is dim.
fn goes_nowhere(control: Control, track: &Track) -> bool {
    match control {
        Control::Mod(slot, ModField::Destination) => track.mods[slot].destination.is_none(),
        _ => false,
    }
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
}

const IN_SCALE_PITCH: &str = "Pitch in scale";

/// A setting's value as text.
pub(crate) fn old_value(control: Control, track: &Track, song: &Song) -> String {
    let hex = |value: u8| format!("{value:02X}");
    match control {
        Control::Gen {
            phrase,
            generator,
            param,
        } => live_shown(song, track, phrase, generator, param),
        Control::NewGenerator(_) => "Empty".to_owned(),
        Control::Machine => track.machine.name().to_owned(),
        Control::Setting(_) => control
            .setting_on(track)
            .map_or_else(String::new, |id| hex(track.setting(id))),
        Control::Base(TrackControl::Cutoff) => hex(track.cutoff),
        Control::Base(TrackControl::Resonance) => hex(track.resonance),
        Control::Base(TrackControl::Pan) => hex(track.pan),
        Control::Base(TrackControl::SendA) => hex(track.send_a),
        Control::Base(TrackControl::SendB) => hex(track.send_b),
        Control::Base(TrackControl::Drive) => hex(track.drive),
        Control::Base(TrackControl::Punch) => hex(track.punch),
        Control::Base(TrackControl::LowCut) => hex(track.low_cut),
        Control::Base(TrackControl::HighCut) => hex(track.high_cut),
        Control::Base(TrackControl::Pitch) => hex(track.pitch),
        Control::Base(TrackControl::VelocityDelta) => hex(track.velocity_delta),
        Control::Base(TrackControl::Quantise) => hex(track.quantise),
        Control::Group => track
            .group
            .map_or_else(|| "Off".to_owned(), |group| format!("G{group}")),
        // `Auto` shows what it goes by, which the page dims as its default.
        Control::Role => track.role().name().to_owned(),
        Control::Choke => track
            .choke
            .map_or_else(|| "Off".to_owned(), |group| group.to_string()),
        Control::GateLength => format!("{}%", gate_percent(track.gate_length())),
        Control::Speed => track.speed.name().to_owned(),
        Control::Reset => reset_name(track.reset),
        // Off, `Acid` slides by its own `Slide`, as a 303 does.
        Control::Glide if track.glide == 0 && track.voice() == Some(SynthVoice::Acid) => {
            "Slide".to_owned()
        }
        Control::Glide => SettingId::GLIDE
            .entry()
            .kind
            .shown(u16::from(track.glide) << 8),
        Control::Sample => sample_text(song, track.sample()),
        // With no sample, there's nothing to tune.
        Control::Tune | Control::Cents => tuning(song, track).map_or_else(
            || "--".to_owned(),
            |tuning| tuning_text(control, tuning.value(control)),
        ),
        Control::Mute | Control::Solo => String::new(),
        Control::Limiter => if song.limiter { "on" } else { "off" }.to_owned(),
        Control::Safe => if track.safe { "on" } else { "off" }.to_owned(),
        Control::FilterType => track.filter.name().to_owned(),
        // The MS-20's have none, so it doesn't apply.
        Control::FilterMode if !track.filter.has_modes() => "--".to_owned(),
        Control::FilterMode => track.filter.plays(track.filter_mode).name().to_owned(),
        Control::Level => hex(track.level),
        Control::Trim => hex(track.trim),
        Control::Groove => track.groove.unwrap_or(song.groove).name().to_owned(),
        Control::Mod(slot, field) => old_mod_value(track, slot, field),
        Control::Accumulator(field) => {
            let accumulation = track.accumulation;
            match field {
                AccumulatorField::Up => accumulation.up.to_string(),
                AccumulatorField::Down => accumulation.down.to_string(),
                AccumulatorField::Moves => accumulation.moves.name().to_owned(),
                AccumulatorField::AtLimit => accumulation.at_limit.name().to_owned(),
                AccumulatorField::Adds => accumulation.adds.name().to_owned(),
                AccumulatorField::Reset => accumulation.reset.name(),
            }
        }
        Control::Arpeggio(field) => {
            let arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => arpeggio.rate.name().to_owned(),
                ArpField::Mode => arpeggio.mode.name().to_owned(),
                ArpField::Octaves => arpeggio.octaves.to_string(),
                ArpField::Retrig => if arpeggio.retrig { "on" } else { "off" }.to_owned(),
            }
        }
        Control::Insert(slot) => track.inserts[slot].name().to_owned(),
        Control::Macro(at) => format!("{:04X}", track.macros[at].value),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            match insert.params().get(param) {
                Some(found) if !found.choices().is_empty() => found
                    .choices()
                    .get(usize::from(insert.value(param)))
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                Some(_) => hex(insert.value(param)),
                None => String::new(),
            }
        }
    }
}

/// A mod slot's setting as the page shows it, under the slot's type.
fn old_mod_value(track: &Track, slot: usize, field: ModField) -> String {
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
    let on = |on: bool| if on { "on" } else { "off" }.to_owned();
    match field {
        ModField::Type => modulator.kind.name().to_owned(),
        ModField::Mode => modulator.mode_name().unwrap_or("--").to_owned(),
        // The note-shaping slot's level is the note's, one way only.
        ModField::Polarity if note_shape_slot(&track.mods) == Some(slot) => "--".to_owned(),
        ModField::Polarity => modulator.polarity().name().to_owned(),
        ModField::Destination if modulator.goes_in_scale() => IN_SCALE_PITCH.to_owned(),
        ModField::Destination => modulator
            .destination
            .map_or_else(|| "---".to_owned(), |d| d.name_on(track)),
        // The slot that shapes each note does so with its level, rather than moving a value.
        ModField::Depth if note_shape_slot(&track.mods) == Some(slot) => "--".to_owned(),
        ModField::Depth => depth_text(modulator.depth),
        ModField::Attack if ahd_kind => ms(ahd.attack_ms),
        ModField::Attack if lpg_kind => ms(lpg.attack_ms),
        ModField::Decay if lpg_kind => ms(lpg.decay_ms),
        ModField::Colour => format!("{:02X}", lpg.colour),
        ModField::Attack => ms(adsr.attack_ms),
        ModField::Decay if ahd_kind => ms(ahd.decay_ms),
        ModField::Decay if modulator.kind == ModKind::Drum => ms(drum.decay_ms),
        ModField::Peak => ms(drum.peak_ms),
        ModField::Body => format!("{}%", drum.body),
        ModField::Ratchet => drum.ratchet.to_string(),
        ModField::Decay => ms(adsr.decay_ms),
        ModField::Hold => match ahd.hold {
            Hold::Note => "Note".to_owned(),
            Hold::Ms(hold) => ms(hold),
        },
        ModField::Sustain => format!("{}%", adsr.sustain),
        ModField::Release => ms(adsr.release_ms),
        ModField::Reset if ahd_kind => on(ahd.reset),
        ModField::Reset if lpg_kind => on(lpg.reset),
        ModField::Reset => on(adsr.reset),
        ModField::Shape => lfo.shape.name().to_owned(),
        // One a note, Steps have no rate.
        ModField::Rate if modulator.kind == ModKind::Steps => match steps.mode {
            StepsMode::Note => "--".to_owned(),
            _ => steps.rate.to_string(),
        },
        ModField::Rate => lfo.rate.to_string(),
        ModField::Length => steps.length.to_string(),
        ModField::Start => (steps.start + 1).to_string(),
        ModField::Loop => steps.looping.to_string(),
        ModField::Mutate => format!("{}%", steps.mutate),
        ModField::Smooth => depth_text(steps.smooth),
        ModField::Step(at) => depth_text(steps.values[usize::from(at)]),
        // An attractor's is the axis heard, named where it's one alone.
        ModField::Phase if lfo.shape.is_attractor() => match lfo.phase {
            0 => "x".to_owned(),
            120 => "y".to_owned(),
            240 => "z".to_owned(),
            phase => format!("{phase}°"),
        },
        ModField::Phase => format!("{}°", lfo.phase),
        ModField::Deform => depth_text(lfo.deform),
        ModField::Fade => lfo.fade.to_string(),
        ModField::Low => {
            let tracking = &modulator.track;
            tracking.source.text(tracking.bounds().low)
        }
        ModField::High => {
            let tracking = &modulator.track;
            tracking.source.text(tracking.bounds().high)
        }
    }
}

/// The value under the cursor in its setting's units, like `1.21 kHz`, where it's a setting the
/// track has: a lock in the grid, or a row of the track page.
pub(crate) fn old_value_shown(app: &App) -> Option<String> {
    let track = app.services.track_here()?;
    if app.modes.view == View::Track
        && let Control::Accumulator(field) = app.track_control()
    {
        return Some(accumulator_shown(field, track.accumulation, app.scale.on));
    }
    if app.modes.view == View::Track
        && let Control::Arpeggio(field) = app.track_control()
    {
        return Some(arpeggio_shown(field, track.arpeggio));
    }
    if app.modes.view == View::Track
        && let Control::Gen {
            phrase,
            generator,
            param,
        } = app.track_control()
    {
        return Some(gen_shown(app, phrase, generator, param));
    }
    if app.modes.view == View::Track && app.track_control() == Control::Glide {
        return Some(glide_shown(track));
    }
    if app.modes.view == View::Track && app.track_control() == Control::Sample {
        return Some(sample_shown(app.song(), track.sample()));
    }
    if app.modes.view == View::Track
        && matches!(app.track_control(), Control::Tune | Control::Cents)
    {
        return Some(tuning_shown(app.song(), track, app.track_control()));
    }
    if app.modes.view == View::Track && app.track_control() == Control::Limiter {
        let on = if app.song().limiter { "on" } else { "off" };
        return Some(on.to_owned());
    }
    if app.modes.view == View::Track && app.track_control() == Control::Safe {
        return Some(safe_shown(track));
    }
    if app.modes.view == View::Track && app.track_control() == Control::Reset {
        return Some(reset_shown(track.reset));
    }
    if matches!(app.modes.view, View::Track | View::Mixer) && app.track_control() == Control::Trim {
        return Some(trim_text(track.trim));
    }
    if !matches!(app.modes.view, View::Track | View::Mixer) {
        return None;
    }
    // A fader the steps drive reads where they have it, and says so, as one a hand has taken over
    // from them does (decision 174).
    let control = app.track_control();
    let heard = app.services.track_as_heard(app.services.cursor.track)?;
    // A setting a macro holds reads where the macro puts it, and says which and how far.
    let heard = heard.with_macros();
    let (id, value) = control.value_on(&heard)?;
    let note = app
        .fader_note(app.services.cursor.track, control)
        .unwrap_or("");
    let held = match control {
        Control::Macro(at) => format!(": holds {}", crate::macroing::holds(track, at)),
        _ => crate::macroing::held_text(track, vt_model::macros::MacroTarget::Setting(id))
            .map_or_else(String::new, |held| format!(": {held}")),
    };
    id.exists_on(track)
        .then(|| format!("{}{note}{held}", shown(id, value)))
}

/// What one of the track's accumulator settings does, as it's set.
fn accumulator_shown(
    field: AccumulatorField,
    accumulation: Accumulation,
    scale_on: bool,
) -> String {
    let unit = if scale_on { "degrees" } else { "semitones" };
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

/// How long a track's ties glide, and what off does on its voice.
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

fn safe_shown(track: &Track) -> String {
    let on = if track.safe { "on" } else { "off" };
    if !track.filter.has_safe() {
        format!("{on}: {} never rings by itself", track.filter.name())
    } else if track.safe {
        "on: Res stops short of ringing by itself".to_owned()
    } else {
        "off: Res rings by itself from about D9".to_owned()
    }
}

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

/// A meaning to follow a name on the status line, its first letter small.
fn lowered(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}

/// A value of a setting in its units.
fn shown(id: SettingId, value: u16) -> String {
    id.entry().kind.shown(value)
}

/// A generator's setting in its units, as `9 of the 16 rows` for Euclid's hits.
fn gen_shown(app: &App, phrase: u32, generator: usize, param: GenParam) -> String {
    let Some(live) = app.live_generator(app.services.cursor.track, phrase, generator) else {
        return String::new();
    };
    let context = live.context();
    // As its macros turn it, and which macro holds it (decisions 294 and 300).
    let shape = &live.generator.heard();
    let shown = match (param, shape.place) {
        (GenParam::Hits, Where::Euclid { hits, .. }) => {
            let open = context.open;
            format!("{} of the {open} rows", usize::from(hits).min(open))
        }
        _ => param.shown(shape, &context),
    };
    let target = vt_model::macros::MacroTarget::Gen {
        column: vt_model::macros::GenColumn::of(live.generator.column),
        param,
    };
    let held = app
        .services
        .track_here()
        .and_then(|track| crate::macroing::held_text(track, target));
    match held {
        Some(held) => format!("{shown}: {held}"),
        None => shown,
    }
}

/// A setting's value as the app writes it, for the Roto to show as its knob turns: in its units
/// where it has them, as the status line says it (`-14.8 dB`, `1.21 kHz`), and otherwise as the
/// track page shows it (`1/16`, `Overdrive`, `40 ms`). The Roto cuts it to 12 characters.
pub(crate) fn old_value_text(control: Control, track: &Track, song: &Song) -> String {
    if control == Control::Trim {
        return trim_text(track.trim);
    }
    match control.value_on(track) {
        Some((id, set)) if id.exists_on(track) => id.entry().kind.shown(set),
        _ => old_value(control, track, song),
    }
}

// ---- The check ----

/// The cell widths the pages draw in.
const WIDTHS: [CellWidths; 6] = [
    CellWidths::SOUND,
    CellWidths::FX,
    CellWidths::ZOOM,
    CellWidths::COMPACT,
    CellWidths {
        value: 11,
        ..CellWidths::COMPACT
    },
    CellWidths {
        bar_cells: 3,
        gap: 2,
        ..CellWidths::SOUND
    },
];

/// The first generator a device's settings are of.
fn generator_of(device: &DeviceSpec) -> Option<(u32, usize)> {
    device
        .title
        .iter()
        .chain(device.grid.iter().flatten())
        .flatten()
        .find_map(|control| match control {
            Control::Gen {
                phrase, generator, ..
            } => Some((*phrase, *generator)),
            _ => None,
        })
}

/// Every cell and title of every device on the page, through the old path and the new, with the
/// cursor where it is, and with `all` off only the cell under the cursor; then the status line,
/// the Roto's text for every setting on the page and the mixer's for every track. Gives back how
/// many were compared.
fn compare(app: &mut App, all: bool, why: &str) -> usize {
    let mut count = 0;
    {
        let ctx = app.ctx();
        let page = &app.track_page;
        let Some(raw) = ctx.track_here() else {
            return 0;
        };
        let shown = raw.with_macros();
        let track = shown.as_ref();
        let heard = as_heard(track, &ctx.faders(ctx.cursor().track));
        let current = page.control_here(&ctx);
        for device in page.devices(Some(track)) {
            let on = if page.page == Page::Sound {
                &heard
            } else {
                track
            };
            let live: Option<LiveGen> = generator_of(&device).and_then(|(phrase, generator)| {
                ctx.live_generator(ctx.cursor().track, phrase, generator)
            });
            let (old, new) = match &live {
                Some(live) => (
                    OldCells::new(page, &ctx, on).of_generator(live),
                    TrackCells::new(page, &ctx, on).of_generator(live),
                ),
                None => (
                    OldCells::new(page, &ctx, on),
                    TrackCells::new(page, &ctx, on),
                ),
            };
            assert_eq!(
                old.titles(&device),
                new.titles(&device),
                "{why}: the title of {} on {:?}",
                device.identity,
                page.page
            );
            for control in device.title.iter().flatten() {
                assert_eq!(
                    old.title_text(*control),
                    new.title_text(*control),
                    "{why}: {control:?}'s tile"
                );
                count += 1;
            }
            let controls = device.grid.iter().flatten().flatten().copied();
            for control in controls.filter(|control| all || *control == current) {
                for widths in WIDTHS {
                    assert_eq!(
                        old.cell(control, widths),
                        new.cell(control, widths),
                        "{why}: {control:?} on {:?} in {widths:?}",
                        page.page
                    );
                    count += 1;
                }
            }
        }
        // The Roto's text for every setting on the page, as the song has it.
        let song = Arc::clone(ctx.song());
        for control in page.rows(Some(raw)).into_iter().flatten().flatten() {
            assert_eq!(
                old_value_text(control, raw, &song),
                crate::roto::edits::value_text(control, raw, &song),
                "{why}: the Roto's {control:?}"
            );
            count += 1;
        }
        // The mixer's, on every track.
        for each in &song.tracks {
            for control in MIXER {
                if !control.exists_on(each) || matches!(control, Control::Mute | Control::Solo) {
                    continue;
                }
                let new = crate::track_page::view(control, each, &song, false)
                    .map(|view| view.cell)
                    .unwrap_or_default();
                assert_eq!(
                    old_value(control, each, &song),
                    new,
                    "{why}: the mixer's {control:?}"
                );
                count += 1;
            }
        }
    }
    // The status line, on the track page and on the mixer.
    assert_eq!(
        old_value_shown(app),
        crate::views::status::value_shown(app),
        "{why}: the status line on {:?}",
        app.track_control()
    );
    let (view, row) = (app.modes.view, app.mixer_page.row);
    app.modes.view = View::Mixer;
    for at in 0..MIXER.len() {
        app.mixer_page.row = at;
        assert_eq!(
            old_value_shown(app),
            crate::views::status::value_shown(app),
            "{why}: the mixer's status line on {:?}",
            app.track_control()
        );
        count += 1;
    }
    app.modes.view = view;
    app.mixer_page.row = row;
    count + 1
}

/// Every page of every track in turn: every cell with the cursor on the first, then the cursor on
/// each in turn, on its fine part too where it has one, and typing in each.
fn every_page(app: &mut App, why: &str) -> usize {
    let mut count = 0;
    for track in 0..app.services.song().tracks.len() {
        app.services.cursor.track = track;
        for page in Page::ALL {
            app.track_page.page = page;
            app.modes.mode = Mode::Track;
            let rows = app.track_page.rows(app.ctx().track_here());
            app.track_page.row = 0;
            app.track_page.field = 0;
            app.track_page.fine = false;
            count += compare(app, true, why);
            for (row, places) in rows.iter().enumerate() {
                for (field, place) in places.iter().enumerate() {
                    if place.is_none() {
                        continue;
                    }
                    app.track_page.row = row;
                    app.track_page.field = field;
                    for fine in [false, true] {
                        app.track_page.fine =
                            fine && app.track_page.has_fine_here(app.ctx().track_here());
                        for (mode, typed) in [(Mode::Track, ""), (Mode::Insert, "4")] {
                            app.modes.mode = mode;
                            app.modes.entry.text = typed.into();
                            count += compare(app, false, why);
                        }
                    }
                }
            }
            app.modes.mode = Mode::Track;
            app.modes.entry.text.clear();
            app.track_page.fine = false;
        }
    }
    count
}

/// A song with every machine, every effect and every mod type on its tracks, settings away from
/// their defaults, macros holding the cutoff, pan, glide and an accumulator's limit, and a track
/// in a group with a role.
fn every_kind() -> Song {
    let mut song = Song::new(Machine::ALL.len());
    let (effects, kinds) = (Insert::ALL, ModKind::ALL);
    let shapes = vt_model::lfo::Shape::ALL;
    let sound = song
        .tracks
        .iter_mut()
        .filter(|track| track.kind == TrackKind::Sound);
    for (at, track) in sound.enumerate() {
        track.machine = Machine::ALL[at % Machine::ALL.len()];
        let count = track.inserts.len();
        for (slot, insert) in track.inserts.iter_mut().enumerate() {
            *insert = effects[(at * count + slot + 1) % effects.len()];
        }
        let count = track.mods.len();
        for (slot, modulator) in track.mods.iter_mut().enumerate() {
            modulator.kind = kinds[(at * count + slot) % kinds.len()];
            modulator.adsr.attack_ms = (slot * 37) as u16;
            modulator.lfo.phase = [0, 120, 240, 45][slot % 4];
            modulator.lfo.shape = shapes[(at + slot) % shapes.len()];
            modulator.depth = (slot as i16 - 2) * 0x11;
            if slot == 1 {
                modulator.destination = None;
            }
        }
        track.glide = ((at * 0x30) % 0xC0) as u8;
        track.reset = (at % 3 > 0).then_some(at as u16 * 7);
        track.groove =
            (at % 2 == 1).then(|| vt_model::groove::GrooveId::all().nth(at % 3).unwrap());
        track.gate_length = (at % 2 == 0).then_some(0x40);
        track.choke = (at % 4 == 1).then_some(2);
        track.safe = at % 2 == 0;
        track.accumulation.up = 1 + at as u8 % 5;
        track.arpeggio.octaves = 1 + at as u8 % 3;
        track.arpeggio.retrig = at % 2 == 0;
        track.speed = Speed::ALL[at % Speed::ALL.len()];
    }
    // An empty slot.
    song.tracks[3].inserts[2] = Insert::Empty;
    let maps = [
        MacroTarget::Setting(SettingId::CUT),
        MacroTarget::Setting(SettingId::PAN),
        MacroTarget::Setting(SettingId::GLIDE),
        MacroTarget::Setting(SettingId::accumulation(0).unwrap()),
    ];
    for (at, target) in maps.into_iter().enumerate() {
        song.tracks[0].macros[at].value = 0x6000;
        song.tracks[0].macros[at].maps.push(MacroMap {
            target,
            min: 0x2000,
            max: 0xC000,
            curve: 0,
            s: 0,
        });
    }
    song.tracks[1].role = Role::Bass;
    song.tracks[1].send_a = 0x40;
    song.tracks[2].send_b = 0x80;
    song.tracks[1].group = Some(1);
    song
}

#[test]
fn every_setting_reads_from_its_view_as_it_did() {
    let (mut app, mut realtime) = app_with(every_kind());
    // A macro on an effect's setting, through the keys, as a song can't name one directly.
    press(&mut app, " 5LL");
    assert_eq!(app.track_page.page, Page::Fx);
    let first = app.services.song().tracks[0].inserts[0];
    let rows = app.track_page.rows(app.ctx().track_here());
    let (row, field) = rows
        .iter()
        .enumerate()
        .find_map(|(row, places)| {
            let field = places.iter().position(|place| {
                matches!(place, Some(Control::InsertParam(0, param))
                    if first.params()[*param].choices().is_empty())
            })?;
            Some((row, field))
        })
        .unwrap();
    app.track_page.row = row;
    app.track_page.field = field;
    press(&mut app, "M5");
    let mut count = every_page(&mut app, "no report");
    // The scale lock the other way, which an accumulator's limits are counted in.
    app.scale.on = !app.scale.on;
    count += every_page(&mut app, "the scale");
    // With a report, and what's heard moved off what's set.
    let mut out = vec![0.0; 256 * 2];
    realtime.process(&mut out);
    app.tick();
    let mut live = app.services.live.unwrap_or_default();
    live.cutoff = 0x10;
    live.pan = 0xF0;
    live.level = 0x30;
    live.resonance = 0x90;
    live.send_a = 0x70;
    for slot in &mut live.inserts {
        for value in slot.iter_mut() {
            *value = 0x33;
        }
    }
    for value in live.machine.iter_mut() {
        *value = 0x22;
    }
    live.macros[0] = 0x9000;
    live.macros[2] = 0x1000;
    app.services.live = Some(live);
    count += every_page(&mut app, "a report");
    // Faders the steps drive, and one taken over.
    for faders in &mut app.services.faders {
        faders.automated = [true; vt_model::settings::FADER_COUNT];
        faders.taken[0] = true;
        faders.at[0] = 0x1234;
    }
    count += every_page(&mut app, "faders");
    // A jump under way.
    app.services.cursor.track = 0;
    app.track_page.page = Page::Sound;
    press(&mut app, "/c");
    assert!(app.modes.field_jump.is_some());
    for page in Page::ALL {
        app.track_page.page = page;
        count += compare(&mut app, true, "a jump");
    }
    assert!(count > 10_000, "{count}");
}

#[test]
fn a_generators_settings_read_from_their_view_as_they_did() {
    let mut song = Song::new(1);
    let empty: Step = "--- -- --- --- ---".parse().unwrap();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![empty; 8])));
    let (mut app, _realtime): (App, Realtime) = app_with(song);
    press(&mut app, "vGSjjjjjj===L");
    press(&mut app, " 5LLL}");
    press(&mut app, "M1");
    let mut count = 0;
    let rows = app.track_page.rows(app.ctx().track_here());
    for (row, places) in rows.iter().enumerate() {
        for (field, place) in places.iter().enumerate() {
            if place.is_none() {
                continue;
            }
            app.track_page.row = row;
            app.track_page.field = field;
            for (mode, typed) in [(Mode::Track, ""), (Mode::Insert, "4")] {
                app.modes.mode = mode;
                app.modes.entry.text = typed.into();
                count += compare(&mut app, true, "a generator");
            }
        }
    }
    assert!(count > 100, "{count}");
}
