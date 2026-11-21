//! What the track page's settings show, in their grids' cells and in their devices' titles, worked
//! out one way for all four pages (the UI plan's U4 and U7): each setting becomes a [`ParamCell`]
//! from its [`ParamView`], which says what it is and reads as on the track drawn, and from what
//! only the page knows: the engine's report, the macros, the faders and the modes. The cell lays
//! itself out.

use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::modulation::ModField;
use vt_model::settings::SettingId;
use vt_model::shape::Context;
use vt_model::song::Track;

use crate::controls::param::ParamView;
use crate::controls::share;
use crate::controls::tuning::{TuneOf, Tuning, tuning};
use crate::controls::{Control, DeviceSpec};
use crate::overlays::mapping::{holding, target_of};
use crate::pages::Ctx;
use crate::pages::track::TrackPage;
use crate::services::automating::fader_of;
use crate::services::generators::LiveGen;
use crate::widgets::grid::{CellParts, CellSource, CellWidths};
use crate::widgets::group;
use crate::widgets::param::{Face, ParamCell, ParamState, Place, Tone, heard, heard_byte};
use crate::widgets::setting::{self, CellLook, Focus};
use crate::widgets::theme;

use super::{EMPTY_SLOT, macro_range};

/// How wide a value is on the Sound page, its coarse digits and its fine ones, as `A000`.
const VALUE_WIDTH: usize = 4;
/// How wide `Tune`'s value is before whose it is, as `-48 st` and two spaces.
const TUNE_WIDTH: usize = 8;
/// What comes before a mod slot's destination in its title, so `LFO  Trig  → Cut` doesn't read
/// as "Trig Cut".
const TO: &str = "→ ";
/// What a Steps slot's `Jump`, or the Play device's, is called in its title, before its chance.
const JUMP: &str = "Jump ";
/// What the Play device's `CDLT` is called in its title, before its value.
const CHANCE: &str = "Chance ";
/// What a Steps slot's `Spread` and `Bias` are called in its title, before their amounts.
const SPREAD: &str = "Spread ";
const BIAS: &str = "Bias ";
/// What a track's dice loop's settings are called in the Play device's title, before their
/// values (decision 410).
const DICE_LOOP: &str = "Dice ";
const DICE_MUTATE: &str = "Mutate ";
const DICE_JUMP: &str = "Dice jump ";
/// What a track's swing is called in the Play device's title, before its value (decision 420).
const SWING: &str = "Swing ";

/// The track page's cells, read from the track as the page draws it.
pub(super) struct TrackCells<'a> {
    /// The setting the cursor is on, which is drawn in the cursor's colour, if it's on one here.
    here: Option<Control>,
    /// Whether the keys are in another area, so the cursor sits back (decision 475).
    away: bool,
    /// Whether the cursor is on its setting's fine digits.
    fine: bool,
    ctx: &'a Ctx<'a>,
    /// The track with what its macros give its settings, and on the Sound page its faders where
    /// the steps have them while they drive them.
    track: &'a Track,
    /// The generator a Gen device's cells are of, with what it works from, for the range a macro
    /// holding one of its settings takes it over.
    generator: Option<(&'a LiveGen, Context<'a>)>,
}

impl<'a> TrackCells<'a> {
    pub(super) fn new(page: &'a TrackPage, ctx: &'a Ctx<'a>, track: &'a Track) -> Self {
        Self {
            here: Some(page.control_here(ctx)),
            away: !page.has_keys(ctx),
            fine: page.fine,
            ctx,
            track,
            generator: None,
        }
    }

    /// The cells of a track drawn away from the track page, as the macro panel draws its macros,
    /// with the cursor on this setting, or on none.
    pub(super) fn apart(ctx: &'a Ctx<'a>, track: &'a Track, here: Option<Control>) -> Self {
        Self {
            here,
            away: false,
            fine: false,
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

    /// A setting as the track drawn has it, or one with nothing to show, as a mute's on a page
    /// that hasn't one.
    fn view(&self, control: Control) -> ParamView {
        self.ctx
            .param_view(control, self.track)
            .unwrap_or_else(|| ParamView::of(control, self.track))
    }

    /// A setting's cell, from its view.
    fn param(&self, control: Control) -> ParamCell<'a> {
        let ctx = self.ctx;
        let here = Some(control) == self.here;
        let view = self.view(control);
        let place = view.bar.map(|set| Place {
            set,
            centred: view.centred,
        });
        let face = face(&view);
        let fine = matches!(face, Face::HexFine { .. }) && self.fine;
        let state = ParamState {
            heard: place.map_or(0, |place| self.heard(control, place.set)),
            fader: fader_of(control).is_some(),
            macro_range: self.macro_range(control),
            tone: self.tone(control, &view),
            taken: ctx.taken_over(ctx.cursor().track, control),
            after: self.after(control),
        };
        let text = match control {
            // Where it's heard, which a lock or modulation may move it from.
            Control::Macro(at) => format!("{:04X}", self.macro_values(at).1),
            _ => view.cell,
        };
        ParamCell {
            name: view.label,
            face,
            text,
            place,
            state,
            look: CellLook {
                away: self.away,
                ..ctx.look_on(control, Focus::at(here, fine))
            },
        }
    }

    /// A device's title controls, each in its own style, the cursor's colour on the one it's on.
    pub(super) fn titles(&self, device: &DeviceSpec) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        for &control in device.title.iter().flatten() {
            let look = CellLook {
                away: self.away,
                ..self
                    .ctx
                    .look_on(control, Focus::at(Some(control) == self.here, false))
            };
            let line = self.title(control, look);
            // Steps' `From` is the loop's number, a space after it, `Loop 1`, as the title has no
            // room for its name (decision 428).
            match (control, lines.last_mut()) {
                (Control::Mod(_, ModField::From), Some(looping)) => {
                    looping.spans.push(Span::raw(" "));
                    looping.spans.extend(line.spans);
                }
                _ => lines.push(line),
            }
        }
        lines
    }

    /// A title control, as the modes show it this way: on a slot, its effect, `Empty` when there's
    /// none, and its type; a machine and its type, the sample a sampler plays or the choice a
    /// synth's is on; the filter's type and mode; a mod slot's type, mode and destination, after
    /// an arrow; a generator's fill, its `Where` and its `Vary`. One its view says does nothing,
    /// as an empty slot, a destination of `---` or a setting left at its default, is dim.
    fn title(&self, control: Control, look: CellLook<'a>) -> Line<'static> {
        let view = self.view(control);
        let tone = match view.engaged {
            true => Tone::Plain,
            false => Tone::Default,
        };
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
        // A destination follows an arrow, and Steps' `Jump`, `Spread` and `Bias` and the Play
        // device's `Jump` and dice loop, numbers among words, their names.
        let before = match control {
            Control::Mod(_, ModField::Destination) => Some(TO),
            Control::Mod(_, ModField::Jump) | Control::Base(TrackControl::Jump) => Some(JUMP),
            Control::Base(TrackControl::ChanceDelta) => Some(CHANCE),
            Control::Mod(_, ModField::Spread) => Some(SPREAD),
            Control::Mod(_, ModField::Bias) => Some(BIAS),
            Control::Base(TrackControl::DiceLoop) => Some(DICE_LOOP),
            Control::Base(TrackControl::DiceMutate) => Some(DICE_MUTATE),
            Control::Base(TrackControl::DiceJump) => Some(DICE_JUMP),
            Control::Base(TrackControl::Swing) => Some(SWING),
            _ => None,
        };
        if let Some(before) = before {
            line.spans
                .insert(0, Span::styled(before, setting::resting(!view.engaged)));
        }
        line
    }

    /// A title control's text, as the zoom's strip has it, with no cursor: an empty slot's
    /// `Empty`, a generator's setting as it's set rather than as a macro holding it puts it, and
    /// otherwise its view's.
    pub(super) fn title_text(&self, control: Control) -> String {
        match (control, &self.generator) {
            (Control::Insert(slot), _) if self.track.inserts[slot] == Insert::Empty => {
                EMPTY_SLOT.to_owned()
            }
            (Control::Gen { param, .. }, Some((live, context))) => {
                param.shown(&live.generator.shape, context)
            }
            _ => self.view(control).cell,
        }
    }

    /// What a setting with a bar is heard at now, set where it's set: from the engine's report,
    /// moved by what the setting has moved since. A generator's bar shows where it's set.
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
                    .and_then(|track| self.ctx.param_view(control, &track.with_macros()))
                    .and_then(|then| then.bar),
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

    /// The range a macro holding a setting can take it over, on its bar's 0 to 255, which a bar
    /// draws and a cell with none marks it held by. A generator's setting with no bounds, such as
    /// its `Wave`, is held across its whole list.
    fn macro_range(&self, control: Control) -> Option<(u8, u8)> {
        // Its group's macro has the last word, so its range is the one drawn (decision 408).
        let held = target_of(control, self.track).and_then(|target| {
            let song = self.ctx.song();
            song.group_holding(self.ctx.cursor().track, target)
                .map(|(_, map)| *map)
        });
        let Control::Gen { param, .. } = control else {
            return match held {
                Some(map) => map
                    .target
                    .setting()
                    .map(|_| (((map.min >> 8) as u8), ((map.max >> 8) as u8))),
                None => macro_range(control, self.track),
            };
        };
        let map = held.or_else(|| holding(control, self.track).next().map(|(_, map)| *map))?;
        let (_, context) = self.generator.as_ref()?;
        let Some(bounds) = param.bounds(context) else {
            return Some((0, u8::MAX));
        };
        Some((
            share(map.number_at(0.0), bounds),
            share(map.number_at(255.0), bounds),
        ))
    }

    /// How a setting's value is coloured while the cursor isn't on it: a track's role in the
    /// role's colour, set back while it's `Auto`, the group it's in in the group's, a fader the
    /// steps drive in the playing colour, and one its view says does nothing dim, as a setting
    /// left at its default or a macro that holds nothing.
    fn tone(&self, control: Control, view: &ParamView) -> Tone {
        let (ctx, track) = (self.ctx, self.track);
        let role = theme::role(track.role()).filter(|_| control == Control::Role);
        if let Some(colour) = role {
            let auto = track.role.is_auto();
            return Tone::Tint(Style::new().fg(if auto { theme::dimmed(colour) } else { colour }));
        }
        match group::style(ctx.song(), ctx.cursor().track) {
            Some(tint) if control == Control::Group => Tone::Tint(tint),
            _ if ctx.follows_steps(ctx.cursor().track, control) => Tone::Automated,
            _ if !view.engaged => Tone::Default,
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

/// How a setting's value reads in its cell, from its view: a switch, a value with a fine part,
/// hex digits with a bar, a word from a list, or a number in its units.
fn face(view: &ParamView) -> Face {
    match view.fine {
        _ if view.toggle => Face::Toggle {
            on: view.value != 0,
        },
        Some(fine) => Face::HexFine { fine },
        None if view.bar.is_some() => Face::Hex,
        None if !view.choices.is_empty() => Face::Word,
        None => Face::Number,
    }
}

/// What a control with a bar is playing with right now.
fn live_value(control: Control, live: &LiveValues) -> Option<u8> {
    match control {
        Control::Level => Some(live.level),
        Control::Base(TrackControl::Pan) => Some(live.pan),
        Control::Setting(setting) => live.machine.get(setting).copied(),
        Control::Base(TrackControl::Cutoff) => Some(live.cutoff),
        Control::Base(TrackControl::Resonance) => Some(live.resonance),
        Control::Base(TrackControl::Drive) => Some(live.filter.drive),
        Control::Base(TrackControl::Punch) => Some(live.filter.punch),
        Control::Base(TrackControl::LowCut) => Some(live.filter.low_cut),
        Control::Base(TrackControl::HighCut) => Some(live.filter.high_cut),
        Control::Base(TrackControl::SendA) => Some(live.send_a),
        Control::Base(TrackControl::SendB) => Some(live.send_b),
        _ => None,
    }
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
