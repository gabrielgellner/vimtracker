//! The Mod page: a rack of half-width devices, the six mod slots down two columns, `Mod 1` to
//! `Mod 3` on the left beside `Mod 4` to `Mod 6` on the right, and the accumulator beside the
//! arpeggio under them. A slot's title has its switch, its type, its type's mode if it has one,
//! and `→` its destination, and a panel on its right draws an envelope's shape, one cycle of an
//! LFO, which the terminal places a dot on where the LFO is now, Steps' values as bars, or a Track
//! slot's map, with a dot at its last note. One switched out is faint but for the cursor, as a
//! bypassed FX slot is.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_model::modulation::{MOD_SLOTS, ModKind};
use vt_model::song::Track;
use vt_model::steps::MAX_STEP;

use crate::app::App;
use crate::theme;
use crate::track_page::{
    Control, DeviceSpec, DeviceWidth, ModField, Page, modulator_bypassed, rack_places,
};
use crate::views::widgets::bar::ValueBar;
use crate::views::widgets::device::{Device, rack_areas};
use crate::views::widgets::grid::{CellSource, CellWidths, SettingsGrid, faded};
use crate::views::widgets::image;
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};

use super::{at_default, value, value_line};

/// The size of an envelope's shape and an LFO's cycle, in characters: the graphic panel's half
/// size, for half-width devices, which is 360 by 195 pixels at the author's 15 by 39.
pub(crate) const MOD_PANEL_WIDTH: usize = 24;
pub(crate) const MOD_PANEL_HEIGHT: usize = 5;
/// The room left between a device's settings and its panel.
const PANEL_GAP: u16 = 2;

/// The arpeggio's cells, whose value has room for `Up-down-inc`. It's alone in its column, with
/// no panel, so it lines up with nothing else.
const ARPEGGIO_CELLS: CellWidths = CellWidths {
    value: 11,
    ..CellWidths::COMPACT
};

/// The macros' cells: a name as long as the Roto's, four digits and a bar. That's 30 columns a
/// cell, four to a row.
const MACRO_CELLS: CellWidths = CellWidths {
    name: 13,
    value: 5,
    bar_cells: 9,
    gap: 3,
};

/// Which mod slot the cursor is on, if any.
pub(crate) fn mod_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Mod(slot, _) => Some(slot),
        _ => None,
    }
}

/// Where each LFO's cycle or Track slot's map is on screen, by its slot and the character at its
/// top left, for the terminal to place its dot over: none for a slot of another type or one that
/// isn't drawn.
pub type CyclesAt = [Option<(u16, u16)>; MOD_SLOTS];

/// What comes before a slot's destination in its title, so `LFO  Trig  → Cut` doesn't read as
/// "Trig Cut".
const TO: &str = "→ ";

/// What the Mod page's cells show for a modulator's settings.
struct ModCells<'a> {
    app: &'a App,
    track: &'a Track,
}

impl CellSource for ModCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        match control {
            Control::Macro(at) => {
                let name = crate::macroing::macro_name(&self.track.macros[at], at);
                name_line(self.app, &name, &labels, room)
            }
            _ => name_line(self.app, control.label(), &labels, room),
        }
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.app.track_control();
        value_line(
            self.app,
            self.track,
            control,
            here,
            at_default(control, self.track),
        )
    }

    /// A modulator's settings have no bars: the panel beside them shows what they shape. A
    /// macro has one, as a knob.
    fn bar(&self, control: Control) -> Option<ValueBar> {
        let Control::Macro(at) = control else {
            return None;
        };
        let set = (self.track.macros[at].value >> 8) as u8;
        Some(ValueBar::new(set, MACRO_CELLS.bar_cells))
    }
}

/// Whether a control is a destination that's set to nothing, `---`, which dims it as an empty
/// slot's `Empty` is dim.
fn goes_nowhere(control: Control, track: &Track) -> bool {
    match control {
        Control::Mod(slot, ModField::Destination) => track.mods[slot].destination.is_none(),
        _ => false,
    }
}

/// A slot's title controls, each in its own style: its type, its mode, and its destination after
/// an arrow in the destination's style.
fn title_controls(app: &App, track: &Track, device: &DeviceSpec) -> Vec<Line<'static>> {
    let current = app.track_control();
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let text = setting::text(app, here, || value(*control, track, app.song()));
            let nowhere = goes_nowhere(*control, track);
            let style = setting::style(app, here, nowhere);
            let mut spans = value_spans(app, *control, text, style);
            if matches!(control, Control::Mod(_, ModField::Destination)) {
                let arrow = setting::style(app, false, nowhere);
                spans.insert(0, Span::styled(TO, arrow));
            }
            Line::from(spans)
        })
        .collect()
}

/// A modulator's panel: an envelope's shape or an LFO's cycle, in a box, once its image has been
/// sent.
fn panel(image: Option<u32>) -> GraphicPanel {
    let Some(id) = image else {
        return GraphicPanel::new();
    };
    let lines = image::rows(id, MOD_PANEL_WIDTH, MOD_PANEL_HEIGHT)
        .into_iter()
        .map(Line::from)
        .collect();
    GraphicPanel::new().boxed(lines, MOD_PANEL_WIDTH as u16, None)
}

/// How many rows of dots the Steps panel's bars have either side of none, at braille's four a
/// character.
const BAR_DOTS: usize = MOD_PANEL_HEIGHT * 4 / 2;

/// A Steps slot's panel: its values played as a row of thin bars up or down from none, one
/// character each, in the middle of the box, spartan and in one colour, faint while the slot is
/// switched out, with the step playing lit in the playhead's colour. Steps past its `Length` keep
/// their values and aren't drawn. It gives back where the panel's first step is across it.
fn steps_panel(
    track: &Track,
    slot: usize,
    playing: Option<u8>,
    faint: bool,
) -> (GraphicPanel, usize) {
    let steps = &track.mods[slot].steps;
    let values = steps.played();
    let first = (MOD_PANEL_WIDTH - values.len()) / 2;
    let base = if faint { theme::faint() } else { theme::dim() };
    let lines = (0..MOD_PANEL_HEIGHT)
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(first))];
            for (at, value) in values.iter().enumerate() {
                let style = if playing == Some(at as u8) && !faint {
                    theme::active()
                } else {
                    base
                };
                spans.push(Span::styled(bar_cell(*value, row).to_string(), style));
            }
            spans.push(Span::raw(
                " ".repeat(MOD_PANEL_WIDTH - first - values.len()),
            ));
            Line::from(spans)
        })
        .collect();
    let panel = GraphicPanel::new().boxed(lines, MOD_PANEL_WIDTH as u16, None);
    (panel, first)
}

/// One row of characters of a step's bar, in braille's left column of dots: up from the middle
/// for a value above none, down for one below, and a short tick across the middle for none.
fn bar_cell(value: i16, row: usize) -> char {
    let middle = BAR_DOTS;
    let reach =
        (f32::from(value.unsigned_abs()) / f32::from(MAX_STEP) * BAR_DOTS as f32).round() as usize;
    let reach = reach.clamp(1, BAR_DOTS);
    let lit = |dot: usize| match value {
        0 => dot == middle - 1 || dot == middle,
        value if value > 0 => (middle - reach..middle).contains(&dot),
        _ => (middle..middle + reach).contains(&dot),
    };
    // Braille's left column, top to bottom.
    const LEFT: [u32; 4] = [0x01, 0x02, 0x04, 0x40];
    let bits = (0..4)
        .filter(|at| lit(row * 4 + at))
        .fold(0, |bits, at| bits | LEFT[at]);
    char::from_u32(0x2800 + bits).unwrap_or(' ')
}

/// One of the Mod page's devices: its frame and title, with a slot's switch, its
/// settings in compact cells, and an envelope's shape or an LFO's cycle in its panel, with the
/// settings stopping short of it. One switched out is faint, its title's destination too, but for
/// the cursor. It gives back where an LFO's cycle is, if it's drawn whole.
fn draw_device(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) -> Option<(u16, u16)> {
    let bypassed = device.switchable
        && device.title[0].and_then(|control| modulator_bypassed(control, track)) == Some(true);
    let fade = bypassed.then(|| theme::cursor(app.mode));
    let mut controls = title_controls(app, track, device);
    if let Some(keep) = fade {
        controls = controls.into_iter().map(|line| faded(line, keep)).collect();
    }
    let mut widget = Device::new(device.identity.clone());
    if device.switchable {
        widget = widget.switch(!bypassed);
    }
    frame.render_widget(widget.controls(controls).focused(focused), area);
    // A Steps slot's cursor, if it's on one of its values, and its first step across the panel.
    let mut marked = None;
    let (panel, lfo) = match device.title[0] {
        Some(Control::Mod(slot, _)) if track.mods[slot].kind == ModKind::Steps => {
            let playing = app.live.and_then(|live| live.steps[slot]);
            let (panel, first) = steps_panel(track, slot, playing, bypassed);
            if let Control::Mod(at, ModField::Step(step)) = app.track_control()
                && at == slot
            {
                marked = Some(first + usize::from(step));
            }
            (panel, false)
        }
        Some(Control::Mod(slot, _)) => (
            panel(app.mod_images.map(|ids| ids[slot])),
            matches!(track.mods[slot].kind, ModKind::Lfo | ModKind::Track),
        ),
        _ => (GraphicPanel::new(), false),
    };
    let mut settings = Device::settings_area(area);
    let mut cycle_at = None;
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        settings.width =
            at.x.saturating_sub(settings.x + PANEL_GAP)
                .min(settings.width);
        let whole = at.width >= panel.width() && at.height >= panel.height();
        if lfo && whole {
            cycle_at = Some(panel.inside_at(at));
        }
        let inside = panel.inside_at(at);
        frame.render_widget(panel, at);
        // The step being edited, marked in the box's bottom border in the cursor's colour.
        if let Some(column) = marked {
            let place = (inside.0 + column as u16, inside.1 + MOD_PANEL_HEIGHT as u16);
            if let Some(cell) = frame.buffer_mut().cell_mut(place) {
                let colour = theme::cursor(app.mode).bg.unwrap_or(theme::ROSEWATER);
                cell.set_char('▲').set_style(Style::new().fg(colour));
            }
        }
    }
    let widths = match device.grid[0][0] {
        Some(Control::Arpeggio(_)) => ARPEGGIO_CELLS,
        Some(Control::Macro(_)) => MACRO_CELLS,
        _ => CellWidths::COMPACT,
    };
    // Steps' values are drawn in the panel, not as a cell.
    let rows = device
        .grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|place| {
                    place.filter(|control| !matches!(control, Control::Mod(_, ModField::Step(_))))
                })
                .collect()
        })
        .collect();
    let source = ModCells { app, track };
    let grid = SettingsGrid::new(&source, rows, widths).faded(fade);
    frame.render_widget(grid, settings);
    cycle_at
}

/// The Mod page: its devices in their rack, two to a band. Four bands fill the rows inside the
/// keyboard's box; where fewer fit, the page goes by whole bands to keep the cursor's in view. It
/// gives back where each LFO's cycle is drawn.
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, track: &Track) -> CyclesAt {
    let devices = Page::Mod.devices(track);
    let places = rack_places(&devices);
    let cursor = app
        .track_page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let keep = places.get(cursor).map_or(0, |place| place.band);
    let sides: Vec<_> = devices
        .iter()
        .zip(&places)
        .map(|(device, place)| {
            let side = (device.width == DeviceWidth::Half).then_some(place.side);
            (place.band, side)
        })
        .collect();
    let mut cycles = CyclesAt::default();
    for (at, (device, shown)) in devices
        .iter()
        .zip(rack_areas(area, &sides, keep))
        .enumerate()
    {
        let Some(shown) = shown else {
            continue;
        };
        let cycle_at = draw_device(frame, shown, app, track, device, at == cursor);
        if let Some(Control::Mod(slot, _)) = device.title[0] {
            cycles[slot] = cycle_at;
        }
    }
    cycles
}
