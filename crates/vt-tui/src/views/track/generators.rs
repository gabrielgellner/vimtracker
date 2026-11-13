//! The Gen page: a device for each live generator of the phrase under the cursor, then `Empty`,
//! stacked down the page as the FX page's slots are.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::shape::params::Units;
use vt_model::shape::{Context, GenParam, ShapeColumn};
use vt_model::song::Track;

use crate::app::App;
use crate::macroing::{holds, macro_name};
use crate::shaping::LiveGen;
use crate::theme;
use crate::track_page::{Control, DeviceSpec};
use crate::views::widgets::bar::ValueBar;
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellSource, CellWidths, ROW_STEP, SettingsGrid};
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::take_top;

use super::BAR_CELLS;

/// The Gen page's cells: a name, a value as long as `9 of 16` or `First, C-3`, and the Sound
/// page's bars.
const GEN_CELLS: CellWidths = CellWidths {
    value: 11,
    ..CellWidths::SOUND
};

/// The macros' cells: a name as long as the Roto's, four digits and the Sound page's bars.
const MACRO_CELLS: CellWidths = CellWidths {
    name: 13,
    value: 5,
    bar_cells: BAR_CELLS,
    gap: 3,
};

/// The macros' band's cells: each macro's name, where it's heard, which a lock or modulation may
/// move from where it's set, and its bar, filled to what's heard with a mark where it's set.
struct MacroCells<'a> {
    app: &'a App,
    track: &'a Track,
}

impl MacroCells<'_> {
    /// Where a macro is set, and where it's heard, from the engine's report of this track.
    fn values(&self, at: usize) -> (u16, u16) {
        let set = self.track.macros[at].value;
        let heard = match (self.app.live, self.app.reported_track()) {
            (Some(live), Some(then)) => {
                let moved = i32::from(live.macros[at]) - i32::from(then.macros[at].value);
                (i32::from(set) + moved).clamp(0, 0xFF00) as u16
            }
            _ => set,
        };
        (set, heard)
    }
}

impl CellSource for MacroCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        let Control::Macro(at) = control else {
            return Line::default();
        };
        let name = macro_name(&self.track.macros[at], at);
        name_line(self.app, &name, &labels, room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let Control::Macro(at) = control else {
            return Line::default();
        };
        let here = control == self.app.track_control();
        let (_, heard) = self.values(at);
        let text = setting::text(self.app, here, || format!("{heard:04X}"));
        let empty = self.track.macros[at].maps.is_empty();
        let style = setting::style(self.app, here, empty);
        Line::from(value_spans(self.app, control, text, style))
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let Control::Macro(at) = control else {
            return None;
        };
        let (set, heard) = self.values(at);
        Some(ValueBar::new((set >> 8) as u8, MACRO_CELLS.bar_cells).playing((heard >> 8) as u8))
    }
}

/// The macros' band: the eight in two rows of four, and under them what the cursor's holds.
fn draw_macros(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    frame.render_widget(Device::new(device.identity.clone()).focused(focused), area);
    let cells = MacroCells { app, track };
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let mut settings = Device::settings_area(area);
    frame.render_widget(SettingsGrid::new(&cells, rows, MACRO_CELLS), settings);
    if let Control::Macro(at) = app.track_control() {
        take_top(&mut settings, 2 * ROW_STEP);
        let line = format!(
            "{} → {}",
            macro_name(&track.macros[at], at),
            holds(track, at)
        );
        frame.render_widget(Line::styled(line, theme::dim()), take_top(&mut settings, 1));
    }
}

/// What `Empty` says, under its title.
const EMPTY_HINT: &str = "  make a column of this phrase live";

/// A generator's cells, read from its shape through the declared table.
struct GenCells<'a> {
    app: &'a App,
    live: &'a LiveGen,
    context: Context<'a>,
}

impl GenCells<'_> {
    fn param(control: Control) -> Option<GenParam> {
        match control {
            Control::Gen { param, .. } => Some(param),
            _ => None,
        }
    }
}

impl CellSource for GenCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        let name = Self::param(control).map_or("", GenParam::name);
        name_line(self.app, name, &labels, room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.app.track_control();
        let shape = &self.live.generator.shape;
        let text = setting::text(self.app, here, || {
            Self::param(control).map_or_else(String::new, |param| param.shown(shape, &self.context))
        });
        let style = setting::style(self.app, here, false);
        Line::from(value_spans(self.app, control, text, style))
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let param = Self::param(control)?;
        if matches!(param.units(), Units::Named) {
            return None;
        }
        let (low, high) = param.bounds(&self.context)?;
        let number = param.number(&self.live.generator.shape, &self.context)?;
        let share = (number - low) as f64 / (high - low).max(1) as f64;
        let set = (share * 255.0).round().clamp(0.0, 255.0) as u8;
        let centred = matches!(param.units(), Units::Bend | Units::Interval);
        Some(ValueBar::new(set, BAR_CELLS).centred(centred))
    }
}

/// Where a generator acts, for the end of its title, dim: its column and its rows, as `Note
/// 00–0F`, or an FX column's command and its lane, as `FX1 CUT lane 0C`.
fn acts_on(live: &LiveGen) -> String {
    let generator = &live.generator;
    let column = match generator.column {
        ShapeColumn::Fx { slot, command } => format!("FX{} {}", slot + 1, command.name()),
        _ => crate::shaping::generator_label(generator),
    };
    match (generator.span, generator.rows()) {
        (vt_model::generator::Span::Lane, Some(rows)) => {
            format!("{column} lane {:02X}", rows.len())
        }
        (_, Some(rows)) => format!("{column} {:02X}–{:02X}", rows.first, rows.last),
        (_, None) => column,
    }
}

/// A generator's title: its fill and its `Where` or its source, and `Acid`'s `Pitches`, each in
/// its own style, a `Where` that's its column's own dim.
fn title_controls(app: &App, cells: &GenCells, device: &DeviceSpec) -> Vec<Line<'static>> {
    let current = app.track_control();
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let shape = &cells.live.generator.shape;
            let param = GenCells::param(*control);
            let text = setting::text(app, here, || {
                param.map_or_else(String::new, |param| param.shown(shape, &cells.context))
            });
            let default =
                param == Some(GenParam::Where) && shape.place == cells.context.default_where();
            let style = setting::style(app, here, default);
            Line::from(value_spans(app, *control, text, style))
        })
        .collect()
}

/// The phrase the cursor's track is playing, if it's playing one, which the page's title names
/// beside the one it shows when they differ.
pub(super) fn playing_phrase(app: &App) -> Option<u32> {
    let track = app.cursor.track;
    let owner = app.song().tracks.get(track)?;
    let chain = app.jam.get(track)?.chain?;
    let place = app.playing_place(track, chain)?;
    Some(
        owner
            .chain_position(chain, place, app.dealer(track))?
            .phrase,
    )
}

/// One generator as a device.
fn draw_generator(frame: &mut Frame, area: Rect, app: &App, device: &DeviceSpec, focused: bool) {
    let Some(Control::Gen {
        phrase, generator, ..
    }) = device.title[0]
    else {
        return;
    };
    let Some(live) = app.live_generator(app.cursor.track, phrase, generator) else {
        return;
    };
    let cells = GenCells {
        app,
        live: &live,
        context: live.context(),
    };
    let widget = Device::new(String::new())
        .controls(title_controls(app, &cells, device))
        .after(Line::styled(format!("· {}", acts_on(&live)), theme::dim()))
        .focused(focused);
    frame.render_widget(widget, area);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, GEN_CELLS);
    frame.render_widget(grid, Device::settings_area(area));
}

/// `Empty`, which `K` makes another column live from.
fn draw_empty(frame: &mut Frame, area: Rect, app: &App, device: &DeviceSpec, focused: bool) {
    let here = device.title[0] == Some(app.track_control());
    let text = setting::text(app, here, || "Empty".to_owned());
    let style = setting::style(app, here, true);
    let widget = Device::new(String::new())
        .controls(vec![Line::styled(text, style)])
        .focused(focused);
    frame.render_widget(widget, area);
    let mut row = Device::settings_area(area);
    take_top(&mut row, ROW_STEP);
    let hint = Line::from(vec![Span::raw("K"), Span::styled(EMPTY_HINT, theme::dim())]);
    frame.render_widget(hint, take_top(&mut row, 1));
}

/// The Gen page: its devices stacked down the page, going by whole devices to keep the cursor's
/// in view. A fixed track has no phrases, so it has none.
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    let devices = app.track_page.devices(Some(track));
    // The macros alone, with no phrase for generators, say why under them.
    let alone = devices.len() == 1;
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = app
        .track_page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    for (at, device) in devices.iter().enumerate().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        match (device.title[0], device.grid[0][0]) {
            (_, Some(Control::Macro(_))) => {
                draw_macros(frame, area, app, track, device, at == cursor);
            }
            (Some(Control::NewGenerator(_)), _) => {
                draw_empty(frame, area, app, device, at == cursor);
            }
            _ => draw_generator(frame, area, app, device, at == cursor),
        }
    }
    if alone {
        let why = if track.is_sound() {
            "There's no phrase here: put one in the chain to make its columns live"
        } else {
            "A fixed track has no phrases, so it has no generators"
        };
        frame.render_widget(Paragraph::new(Line::styled(why, theme::dim())), rest);
    }
}
