//! TEMPORARY (U4, #739): the track page's six `CellSource`s and five title builders as they were
//! before `TrackCells`, kept only to check the new path draws every cell the same, styles and all.
//! Removed with the old path in the same PR.

use std::sync::Arc;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_engine::link::Realtime;
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::settings::{Kind, SettingId};
use vt_model::shape::params::Units;
use vt_model::shape::{Context, GenParam};
use vt_model::song::{Machine, Phrase, Role, Song, Track, TrackKind};
use vt_model::step::Step;

use super::cells::TrackCells;
use super::{BAR_CELLS, EMPTY_SLOT, at_default, macro_range, toggle_on, value};
use crate::app::App;
use crate::automating::{as_heard, fader_of};
use crate::glyphs;
use crate::input::mode::Mode;
use crate::macroing::macro_name;
use crate::page::Ctx;
use crate::shaping::LiveGen;
use crate::theme;
use crate::track_page::tuning::{TuneOf, Tuning, tuning};
use crate::track_page::{Control, DeviceSpec, ModField, Page, TrackPage};
use crate::views::widgets::bar::ValueBar;
use crate::views::widgets::grid::{CellParts, CellSource, CellWidths};
use crate::views::widgets::group;
use crate::views::widgets::pin::{app_with, press};
use crate::views::widgets::setting::{self, CellLook, Focus, name_line, value_spans};
use crate::views::widgets::toggle;

trait OldCells {
    fn name(&self, control: Control, room: u16) -> Line<'static>;
    fn value(&self, control: Control) -> Line<'static>;
    fn bar(&self, control: Control) -> Option<ValueBar>;
}

fn moved_since(live: u8, then: u8, now: u8) -> u8 {
    (i16::from(live) + i16::from(now) - i16::from(then)).clamp(0, 255) as u8
}

fn value_line(
    ctx: &Ctx,
    track: &Track,
    control: Control,
    here: bool,
    default: bool,
) -> Line<'static> {
    let look = ctx.look_on(control, Focus::at(here, false));
    let (text, style) = match toggle_on(control, track, ctx.song()) {
        Some(on) if look.typed.is_none() => {
            let style = if here {
                theme::cursor(look.mode)
            } else if default {
                setting::resting(true)
            } else {
                toggle::style(on)
            };
            (toggle::text(on), style)
        }
        _ => (
            setting::text(&look, || value(control, track, ctx.song())),
            setting::style(&look, default),
        ),
    };
    Line::from(value_spans(&look, text, style))
}

// ---- FX ----

struct SlotCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    track: &'a Track,
    bar_cells: usize,
}

impl OldCells for SlotCells<'_> {
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let look = self.ctx.look_on(control, Focus::Off);
        name_line(&look, control.name_on(self.track), room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.page.control_here(self.ctx);
        value_line(self.ctx, self.track, control, here, false)
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let Control::InsertParam(slot, param) = control else {
            return None;
        };
        let insert = self.track.inserts[slot];
        if !insert.params().get(param)?.choices.is_empty() {
            return None;
        }
        let set = insert.value(param);
        let then = self
            .ctx
            .reported_track()
            .map(|track| track.with_macros().inserts[slot])
            .filter(|then| std::mem::discriminant(then) == std::mem::discriminant(&insert));
        let live = match (self.ctx.live(), then) {
            (Some(live), Some(then)) => {
                moved_since(live.inserts[slot][param], then.value(param), set)
            }
            _ => set,
        };
        Some(
            ValueBar::new(set, self.bar_cells)
                .playing(live)
                .range(macro_range(control, self.track)),
        )
    }
}

fn fx_titles(
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
) -> Vec<Line<'static>> {
    let current = page.control_here(ctx);
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let empty =
                matches!(*control, Control::Insert(slot) if track.inserts[slot] == Insert::Empty);
            let look = ctx.look_on(*control, Focus::at(here, false));
            let text = setting::text(&look, || match empty {
                true => EMPTY_SLOT.to_owned(),
                false => value(*control, track, ctx.song()),
            });
            let style = setting::style(&look, empty);
            Line::from(value_spans(&look, text, style))
        })
        .collect()
}

fn tile_titles(ctx: &Ctx, track: &Track, device: &DeviceSpec) -> Vec<Line<'static>> {
    let Some(Control::Insert(slot)) = device.title[0] else {
        return Vec::new();
    };
    let empty = track.inserts[slot] == Insert::Empty;
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let text = match empty {
                true => EMPTY_SLOT.to_owned(),
                false => value(*control, track, ctx.song()),
            };
            Line::styled(text, setting::resting(empty))
        })
        .collect()
}

// ---- Sound ----

const VALUE_WIDTH: usize = 4;
const COARSE_WIDTH: usize = 2;
const TUNE_WIDTH: usize = 8;

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

fn choice_name(control: Control, track: &Track) -> Option<String> {
    let id = control.setting_on(track)?;
    let Kind::Stepped { choices, .. } = id.entry().kind else {
        return None;
    };
    choices
        .get(usize::from(track.setting(id)))
        .map(|choice| (*choice).to_owned())
}

fn bar_value(control: Control, track: &Track) -> Option<(u8, bool)> {
    let level = |value| Some((value, false));
    match control {
        Control::Level => level(track.level),
        Control::Trim => Some((track.trim, true)),
        Control::Base(TrackControl::Pan) => Some((track.pan, true)),
        Control::Base(TrackControl::Pitch) => Some((track.pitch, true)),
        Control::Base(TrackControl::VelocityDelta) => Some((track.velocity_delta, true)),
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

fn value_style(ctx: &Ctx, look: &CellLook, track: &Track, control: Control) -> Style {
    let here = look.focus == Focus::Value;
    let style = setting::style(
        look,
        at_default(control, track) || untuned(ctx, track, control),
    );
    let role = theme::role(track.role()).filter(|_| control == Control::Role && !here);
    if let Some(colour) = role {
        let auto = track.role.is_auto();
        return Style::new().fg(if auto { theme::dimmed(colour) } else { colour });
    }
    match group::style(ctx.song(), ctx.cursor().track) {
        Some(tint) if control == Control::Group && !here => tint,
        _ if !here && ctx.follows_steps(ctx.cursor().track, control) => theme::automated(),
        _ => style,
    }
}

fn untuned(ctx: &Ctx, track: &Track, control: Control) -> bool {
    matches!(control, Control::Tune | Control::Cents)
        && tuning(ctx.song(), track).is_none_or(|tuning| tuning.at_rest(control))
}

fn tune_owner(ctx: &Ctx, track: &Track) -> Option<Span<'static>> {
    let tuning = tuning(ctx.song(), track).filter(Tuning::sliced)?;
    Some(match tuning.of {
        TuneOf::Slice(slice) => {
            Span::styled(format!("slice {slice:02X}"), Style::new().fg(theme::MAUVE))
        }
        TuneOf::Sample => Span::styled("sample", theme::dim()),
    })
}

struct SoundCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    track: &'a Track,
}

impl OldCells for SoundCells<'_> {
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let look = self.ctx.look_on(control, Focus::Off);
        let mut line = name_line(&look, control.label_on(self.track), room);
        if self.ctx.taken_over(self.ctx.cursor().track, control) {
            line.push_span(Span::raw(" "));
            line.push_span(Span::styled(glyphs::TAKEN.to_string(), theme::taken()));
        }
        line
    }

    fn value(&self, control: Control) -> Line<'static> {
        let (page, ctx, track) = (self.page, self.ctx, self.track);
        let here = control == page.control_here(ctx);
        if matches!(control, Control::Limiter | Control::Safe) {
            return value_line(ctx, track, control, here, at_default(control, track));
        }
        let look = ctx.look_on(control, Focus::at(here, page.fine));
        let style = value_style(ctx, &look, track, control);
        let slice = control.setting_on(track) == Some(SettingId::SLICE);
        let word = match control.setting_on(track).map(|id| id.entry().kind) {
            Some(Kind::Stepped { .. }) if !slice => choice_name(control, track),
            _ => None,
        };
        let text = setting::text(&look, || {
            word.unwrap_or_else(|| value(control, track, ctx.song()))
        });
        let fine = control.fine_on(track);
        let text = match fine {
            Some(_) => format!("{text:<COARSE_WIDTH$}"),
            None => text,
        };
        let mut spans = value_spans(&look, text, style);
        if let Some(id) = fine {
            let fine = setting::fine_text(&look, || format!("{:02X}", track.fine(id)));
            spans.push(Span::styled(fine, setting::fine_style(&look)));
        }
        if control == Control::Tune
            && let Some(owner) = tune_owner(ctx, track)
        {
            let width: usize = spans.iter().map(Span::width).sum();
            spans.push(Span::raw(" ".repeat(TUNE_WIDTH.saturating_sub(width))));
            spans.push(owner);
        }
        if slice {
            let room = VALUE_WIDTH + 1;
            let width: usize = spans.iter().map(Span::width).sum();
            spans.push(Span::raw(" ".repeat(room.saturating_sub(width))));
            spans.push(Span::styled(
                ctx.slice_choice(track),
                Style::new().fg(theme::MAUVE),
            ));
        }
        Line::from(spans)
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let (value, centred) = bar_value(control, self.track)?;
        let then = self
            .ctx
            .reported_track()
            .and_then(|track| bar_value(control, &track.with_macros()));
        let live = match (
            self.ctx.live().and_then(|live| live_value(control, &live)),
            then,
        ) {
            (Some(live), Some((then, _))) => moved_since(live, then, value),
            _ => value,
        };
        let bar = if fader_of(control).is_some() {
            ValueBar::new(live, BAR_CELLS).playing(value)
        } else {
            ValueBar::new(value, BAR_CELLS).playing(live)
        };
        Some(bar.centred(centred).range(macro_range(control, self.track)))
    }
}

fn sound_titles(
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
) -> Vec<Line<'static>> {
    let current = page.control_here(ctx);
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let look = ctx.look_on(*control, Focus::at(here, false));
            let text = setting::text(&look, || {
                choice_name(*control, track).unwrap_or_else(|| value(*control, track, ctx.song()))
            });
            let style = setting::style(&look, at_default(*control, track));
            Line::from(value_spans(&look, text, style))
        })
        .collect()
}

// ---- Mod ----

struct ModCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    track: &'a Track,
}

impl OldCells for ModCells<'_> {
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let look = self.ctx.look_on(control, Focus::Off);
        name_line(&look, control.label_on(self.track), room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.page.control_here(self.ctx);
        value_line(
            self.ctx,
            self.track,
            control,
            here,
            at_default(control, self.track),
        )
    }

    fn bar(&self, _control: Control) -> Option<ValueBar> {
        None
    }
}

fn goes_nowhere(control: Control, track: &Track) -> bool {
    match control {
        Control::Mod(slot, ModField::Destination) => track.mods[slot].destination.is_none(),
        _ => false,
    }
}

fn mod_titles(
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
) -> Vec<Line<'static>> {
    let current = page.control_here(ctx);
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let look = ctx.look_on(*control, Focus::at(here, false));
            let text = setting::text(&look, || value(*control, track, ctx.song()));
            let nowhere = goes_nowhere(*control, track);
            let style = setting::style(&look, nowhere);
            let mut spans = value_spans(&look, text, style);
            if matches!(control, Control::Mod(_, ModField::Destination)) {
                let arrow = setting::resting(nowhere);
                spans.insert(0, Span::styled("→ ", arrow));
            }
            Line::from(spans)
        })
        .collect()
}

// ---- Gen ----

struct MacroCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    track: &'a Track,
}

impl MacroCells<'_> {
    fn values(&self, at: usize) -> (u16, u16) {
        let set = self.track.macros[at].value;
        let heard = match (self.ctx.live(), self.ctx.reported_track()) {
            (Some(live), Some(then)) => {
                let moved = i32::from(live.macros[at]) - i32::from(then.macros[at].value);
                (i32::from(set) + moved).clamp(0, 0xFF00) as u16
            }
            _ => set,
        };
        (set, heard)
    }
}

impl OldCells for MacroCells<'_> {
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let look = self.ctx.look_on(control, Focus::Off);
        let Control::Macro(at) = control else {
            return Line::default();
        };
        let name = macro_name(&self.track.macros[at], at);
        name_line(&look, &name, room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let Control::Macro(at) = control else {
            return Line::default();
        };
        let here = control == self.page.control_here(self.ctx);
        let look = self.ctx.look_on(control, Focus::at(here, false));
        let (_, heard) = self.values(at);
        let text = setting::text(&look, || format!("{heard:04X}"));
        let empty = self.track.macros[at].maps.is_empty();
        let style = setting::style(&look, empty);
        Line::from(value_spans(&look, text, style))
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let Control::Macro(at) = control else {
            return None;
        };
        let (set, heard) = self.values(at);
        Some(ValueBar::new((set >> 8) as u8, BAR_CELLS).playing((heard >> 8) as u8))
    }
}

struct GenCells<'a> {
    page: &'a TrackPage,
    ctx: &'a Ctx<'a>,
    live: &'a LiveGen,
    context: Context<'a>,
    bar_cells: usize,
}

impl GenCells<'_> {
    fn param(control: Control) -> Option<GenParam> {
        match control {
            Control::Gen { param, .. } => Some(param),
            _ => None,
        }
    }
}

impl OldCells for GenCells<'_> {
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let look = self.ctx.look_on(control, Focus::Off);
        let name = Self::param(control).map_or("", GenParam::name);
        name_line(&look, name, room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.page.control_here(self.ctx);
        let look = self.ctx.look_on(control, Focus::at(here, false));
        let shape = &self.live.generator.heard();
        let text = setting::text(&look, || {
            Self::param(control).map_or_else(String::new, |param| {
                if self.bar(control).is_some() {
                    param.brief(shape, &self.context)
                } else {
                    param.shown(shape, &self.context)
                }
            })
        });
        let style = setting::style(&look, false);
        Line::from(value_spans(&look, text, style))
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let param = Self::param(control)?;
        if matches!(param.units(), Units::Named) {
            return None;
        }
        let (low, high) = param.bounds(&self.context)?;
        let number = param.number(&self.live.generator.heard(), &self.context)?;
        let place = |number: i32| {
            let share = (number - low) as f64 / (high - low).max(1) as f64;
            (share * 255.0).round().clamp(0.0, 255.0) as u8
        };
        let centred = matches!(param.units(), Units::Bend | Units::Interval);
        let target = MacroTarget::Gen {
            column: GenColumn::of(self.live.generator.column),
            param,
        };
        let range = self
            .ctx
            .song()
            .tracks
            .get(self.ctx.cursor().track)
            .and_then(|track| track.macro_holding(target))
            .map(|(_, map)| (place(map.number_at(0.0)), place(map.number_at(255.0))));
        Some(
            ValueBar::new(place(number), self.bar_cells)
                .centred(centred)
                .range(range),
        )
    }
}

fn gen_titles(
    page: &TrackPage,
    ctx: &Ctx,
    cells: &GenCells,
    device: &DeviceSpec,
) -> Vec<Line<'static>> {
    let current = page.control_here(ctx);
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let shape = &cells.live.generator.shape;
            let param = GenCells::param(*control);
            let look = ctx.look_on(*control, Focus::at(here, false));
            let text = setting::text(&look, || {
                param.map_or_else(String::new, |param| param.shown(shape, &cells.context))
            });
            let default = match param {
                Some(GenParam::Where) => shape.place == cells.context.default_where(),
                Some(GenParam::Vary) => shape.vary == 0,
                _ => false,
            };
            let style = setting::style(&look, default);
            Line::from(value_spans(&look, text, style))
        })
        .collect()
}

// ---- The check ----

/// Every cell and title of every device on the page showing, through the old path and the new,
/// with the cursor where it is; with `all` off, only the cell and title under the cursor. Gives
/// back how many were compared.
fn compare(app: &App, all: bool, why: &str) -> usize {
    let ctx = app.ctx();
    let page = &app.track_page;
    let Some(raw) = ctx.track_here() else {
        return 0;
    };
    let shown = raw.with_macros();
    let track = shown.as_ref();
    let heard = as_heard(track, &ctx.faders(ctx.cursor().track));
    let current = page.control_here(&ctx);
    let wanted = |control: Control| all || control == current;
    let mut count = 0;
    let mut check = |old: CellParts, new: CellParts, control: Control| {
        assert_eq!(old, new, "{why}: {control:?} on {:?}", page.page);
        count += 1;
    };
    let check_titles = |old: Vec<Line<'static>>, new: Vec<Line<'static>>, device: &DeviceSpec| {
        assert_eq!(old, new, "{why}: the title of {}", device.identity);
    };
    let old_parts = |source: &dyn OldCells, control: Control, widths: CellWidths| CellParts {
        name: source.name(control, widths.name),
        value: source.value(control),
        bar: source.bar(control),
    };
    for device in page.devices(Some(track)) {
        let controls: Vec<Control> = device.grid.iter().flatten().flatten().copied().collect();
        match page.page {
            Page::Fx => {
                let cells = TrackCells::new(page, &ctx, track);
                check_titles(
                    fx_titles(page, &ctx, track, &device),
                    cells.titles(&device),
                    &device,
                );
                // The zoom's strip, whose device reads its titles' spans alone.
                let old = tile_titles(&ctx, track, &device);
                let new: Vec<_> = device
                    .title
                    .iter()
                    .flatten()
                    .map(|control| Line::raw(cells.title_text(*control)))
                    .collect();
                assert_eq!(old.len(), new.len());
                for (old, new) in old.into_iter().zip(new) {
                    assert_eq!(old.spans, new.spans, "{why}: a tile of {}", device.identity);
                }
                for widths in [CellWidths::FX, CellWidths::ZOOM] {
                    let source = SlotCells {
                        page,
                        ctx: &ctx,
                        track,
                        bar_cells: widths.bar_cells,
                    };
                    for control in controls.iter().copied().filter(|c| wanted(*c)) {
                        check(
                            old_parts(&source, control, widths),
                            cells.cell(control, widths),
                            control,
                        );
                    }
                }
            }
            Page::Sound => {
                check_titles(
                    sound_titles(page, &ctx, track, &device),
                    TrackCells::new(page, &ctx, &heard).titles(&device),
                    &device,
                );
                let source = SoundCells {
                    page,
                    ctx: &ctx,
                    track: &heard,
                };
                let cells = TrackCells::new(page, &ctx, &heard);
                for control in controls.iter().copied().filter(|c| wanted(*c)) {
                    check(
                        old_parts(&source, control, CellWidths::SOUND),
                        cells.cell(control, CellWidths::SOUND),
                        control,
                    );
                }
            }
            Page::Mod => {
                let cells = TrackCells::new(page, &ctx, track);
                check_titles(
                    mod_titles(page, &ctx, track, &device),
                    cells.titles(&device),
                    &device,
                );
                let source = ModCells {
                    page,
                    ctx: &ctx,
                    track,
                };
                let widths = match device.grid[0][0] {
                    Some(Control::Arpeggio(_)) => CellWidths {
                        value: 11,
                        ..CellWidths::COMPACT
                    },
                    _ => CellWidths::COMPACT,
                };
                for control in controls.iter().copied().filter(|c| wanted(*c)) {
                    if matches!(control, Control::Mod(_, ModField::Step(_))) {
                        continue;
                    }
                    check(
                        old_parts(&source, control, widths),
                        cells.cell(control, widths),
                        control,
                    );
                }
            }
            Page::Gen => {
                if let Some(Control::Macro(_)) = device.grid[0][0] {
                    let widths = CellWidths {
                        name: 13,
                        value: 5,
                        bar_cells: BAR_CELLS,
                        gap: 3,
                    };
                    let source = MacroCells {
                        page,
                        ctx: &ctx,
                        track,
                    };
                    let cells = TrackCells::new(page, &ctx, track);
                    for control in controls.iter().copied().filter(|c| wanted(*c)) {
                        check(
                            old_parts(&source, control, widths),
                            cells.cell(control, widths),
                            control,
                        );
                    }
                    continue;
                }
                let first = device
                    .title
                    .iter()
                    .chain(device.grid.iter().flatten())
                    .flatten()
                    .find_map(|control| match control {
                        Control::Gen {
                            phrase, generator, ..
                        } => Some((*phrase, *generator)),
                        _ => None,
                    });
                let Some((phrase, generator)) = first else {
                    continue;
                };
                let Some(live) = ctx.live_generator(ctx.cursor().track, phrase, generator) else {
                    continue;
                };
                let cells = TrackCells::new(page, &ctx, track).of_generator(&live);
                for widths in [
                    CellWidths::SOUND,
                    CellWidths {
                        bar_cells: 3,
                        gap: 2,
                        ..CellWidths::SOUND
                    },
                ] {
                    let source = GenCells {
                        page,
                        ctx: &ctx,
                        live: &live,
                        context: live.context(),
                        bar_cells: widths.bar_cells,
                    };
                    if !device.is_loops() {
                        check_titles(
                            gen_titles(page, &ctx, &source, &device),
                            cells.titles(&device),
                            &device,
                        );
                    }
                    for control in controls.iter().copied().filter(|c| wanted(*c)) {
                        check(
                            old_parts(&source, control, widths),
                            cells.cell(control, widths),
                            control,
                        );
                    }
                }
            }
        }
    }
    count
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
                            app.modes.entry.text = typed.to_owned();
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

/// A song with every machine, every effect and every mod type on its tracks, a macro holding a
/// machine's setting, an effect's and the cutoff, and a track in a group with a role.
fn every_kind() -> Song {
    let mut song = Song::new(Machine::ALL.len());
    let (effects, kinds) = (Insert::ALL, vt_model::modulation::ModKind::ALL);
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
        }
    }
    // An empty slot.
    song.tracks[3].inserts[2] = Insert::Empty;
    let maps = [
        MacroTarget::Setting(SettingId::CUT),
        MacroTarget::Setting(SettingId::PAN),
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
fn track_cells_draw_every_cell_as_the_old_cell_sources_did() {
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
            let field = places
                .iter()
                .position(|place| matches!(place, Some(Control::InsertParam(0, param)) if first.params()[*param].choices.is_empty()))?;
            Some((row, field))
        })
        .unwrap();
    app.track_page.row = row;
    app.track_page.field = field;
    press(&mut app, "M3");
    assert!(
        app.services.status.contains("macro 3"),
        "{}",
        app.services.status
    );
    let held = &app.services.song().tracks[0].macros[2].maps;
    assert!(
        matches!(held.as_slice(), [one] if matches!(one.target, MacroTarget::Setting(_))),
        "{held:?} {}",
        app.services.status
    );
    let mut count = every_page(&mut app, "no report");
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
        count += compare(&app, true, "a jump");
    }
    assert!(count > 10_000, "{count}");
}

#[test]
fn track_cells_draw_a_generators_cells_as_gen_cells_did() {
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
                app.modes.entry.text = typed.to_owned();
                count += compare(&app, true, "a generator");
            }
        }
    }
    assert!(count > 100, "{count}");
}
