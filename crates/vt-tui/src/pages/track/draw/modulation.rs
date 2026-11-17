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
use vt_engine::track_state::Drawn;
use vt_model::modulation::{MOD_SLOTS, ModField, ModKind};
use vt_model::song::Track;
use vt_model::steps::MAX_STEP;

use crate::controls::modulator_bypassed;
use crate::controls::{Control, DeviceSpec, DeviceWidth, Page};
use crate::graphics::PanelKey;
use crate::pages::Ctx;
use crate::pages::track::{TrackPage, rack_places};
use crate::widgets::device::{Device, rack_areas};
use crate::widgets::grid::{CellWidths, SettingsGrid, faded};
use crate::widgets::image;
use crate::widgets::panel::GraphicPanel;
use crate::widgets::theme;

use super::TrackCells;

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

/// Which mod slot the cursor is on, if any.
pub(crate) fn mod_being_edited(page: &TrackPage, ctx: &Ctx) -> Option<usize> {
    match page.control_here(ctx) {
        Control::Mod(slot, _) => Some(slot),
        _ => None,
    }
}

/// Where each LFO's cycle or Track slot's map is on screen, by its slot and the character at its
/// top left, for the terminal to place its dot over: none for a slot of another type or one that
/// isn't drawn.
pub(crate) type CyclesAt = [Option<(u16, u16)>; MOD_SLOTS];

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
/// switched out, with the step playing lit in the playhead's colour. A step `Mutate` has drawn is
/// drawn at the value drawn, which is what's heard, in the colour of a value something other than
/// the hand is playing, as a fader the steps drive is (decision 384). Steps past its `Length`
/// keep their values and aren't drawn. It gives back where the panel's first step is across it.
fn steps_panel(
    track: &Track,
    slot: usize,
    playing: Option<u8>,
    drawn: Drawn,
    faint: bool,
) -> (GraphicPanel, usize) {
    let steps = &track.mods[slot].steps;
    let values = steps.played();
    let first = (MOD_PANEL_WIDTH - values.len()) / 2;
    let base = if faint { theme::faint() } else { theme::dim() };
    let lines = (0..MOD_PANEL_HEIGHT)
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(first))];
            for (at, written) in values.iter().enumerate() {
                let heard = drawn.value(at);
                let style = match heard {
                    _ if faint => base,
                    _ if playing == Some(at as u8) => theme::active(),
                    Some(_) => theme::automated(),
                    None => base,
                };
                let value = heard.unwrap_or(*written);
                spans.push(Span::styled(bar_cell(value, row).to_string(), style));
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
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) -> Option<(u16, u16)> {
    let bypassed = device.switchable
        && device.title[0].and_then(|control| modulator_bypassed(control, track)) == Some(true);
    let fade = bypassed.then(|| theme::cursor(ctx.mode()));
    let mut controls = TrackCells::new(page, ctx, track).titles(device);
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
            let playing = ctx.live().and_then(|live| live.steps[slot]);
            let (panel, first) = steps_panel(track, slot, playing, ctx.drawn(slot), bypassed);
            if let Control::Mod(at, ModField::Step(step)) = page.control_here(ctx)
                && at == slot
            {
                marked = Some(first + usize::from(step));
            }
            (panel, false)
        }
        Some(Control::Mod(slot, _)) => (
            panel(page.images.get(PanelKey::Mod(slot))),
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
                let colour = theme::cursor(ctx.mode()).bg.unwrap_or(theme::ROSEWATER);
                cell.set_char('▲').set_style(Style::new().fg(colour));
            }
        }
    }
    let widths = match device.grid[0][0] {
        Some(Control::Arpeggio(_)) => ARPEGGIO_CELLS,
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
    let source = TrackCells::new(page, ctx, track);
    let grid = SettingsGrid::new(&source, rows, widths).faded(fade);
    frame.render_widget(grid, settings);
    cycle_at
}

/// The Mod page: its devices in their rack, two to a band. Four bands fill the rows inside the
/// keyboard's box; where fewer fit, the page goes by whole bands to keep the cursor's in view. It
/// gives back where each LFO's cycle is drawn.
pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
) -> CyclesAt {
    let devices = Page::Mod.devices(track);
    let places = rack_places(&devices);
    let cursor = page
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
        let cycle_at = draw_device(frame, shown, page, ctx, track, device, at == cursor);
        if let Some(Control::Mod(slot, _)) = device.title[0] {
            cycles[slot] = cycle_at;
        }
    }
    cycles
}
