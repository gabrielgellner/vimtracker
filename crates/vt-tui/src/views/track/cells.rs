//! What the track page's settings show, in their grids' cells and in their devices' titles, worked
//! out one way for all four pages (the UI plan's U4, #739): each setting becomes a
//! [`ParamCell`], from what the track holds, the engine's report, the macros, the faders and the
//! modes, and the cell lays itself out.

use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::macros::{GenColumn, MacroTarget};
use vt_model::settings::{Kind, SettingId};
use vt_model::shape::params::Units;
use vt_model::shape::{Context, GenParam};
use vt_model::song::Track;

use crate::automating::fader_of;
use crate::macroing::macro_name;
use crate::page::Ctx;
use crate::shaping::LiveGen;
use crate::theme;
use crate::track_page::tuning::{TuneOf, Tuning, tuning};
use crate::track_page::{Control, DeviceSpec, ModField, TrackPage};
use crate::views::widgets::grid::{CellParts, CellSource, CellWidths};
use crate::views::widgets::group;
use crate::views::widgets::param::{Face, ParamCell, ParamState, Place, Tone, heard, heard_byte};
use crate::views::widgets::setting::{self, CellLook, Focus};

use super::{EMPTY_SLOT, at_default, macro_range, toggle_on, value};

/// How wide a value is on the Sound page, its coarse digits and its fine ones, as `A000`.
const VALUE_WIDTH: usize = 4;
/// How wide `Tune`'s value is before whose it is, as `-48 st` and two spaces.
const TUNE_WIDTH: usize = 8;
/// What comes before a mod slot's destination in its title, so `LFO  Trig  → Cut` doesn't read
/// as "Trig Cut".
const TO: &str = "→ ";

/// The track page's cells, read from the track as the page draws it.
pub(super) struct TrackCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    /// The track with what its macros give its settings, and on the Sound page its faders where
    /// the steps have them while they drive them.
    track: &'a Track,
    /// The generator a Gen device's cells are of, with what it works from.
    generator: Option<(&'a LiveGen, Context<'a>)>,
}

impl<'a> TrackCells<'a> {
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
                value(control, self.track, self.ctx.song())
            }
            _ => self.word(control),
        }
    }

    /// A setting's value as the status line has it, or a machine's list by the choice it's on.
    fn word(&self, control: Control) -> String {
        choice_name(control, self.track)
            .unwrap_or_else(|| value(control, self.track, self.ctx.song()))
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

impl CellSource for TrackCells<'_> {
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
