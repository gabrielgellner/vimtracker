//! The FX page zoomed in on one slot, `z i` (decision 329, option C of the FX page study,
//! `docs/designs/fx-page.md`): a strip of every slot across the top, each a small device with its
//! switch, its effect and the first row of its settings, then the cursor's slot as a device the
//! page's width, its graphics 16 rows tall over its settings with bars nearly twice as long.
//!
//! A compressor's curve is a 42 by 16 square, beside a gain reduction meter 16 rows tall and the
//! last two bars of what it turned down, which shows the pumping. A limiter has the meter and the
//! two bars alone, an EQ its curve across the whole width, and a distortion that shapes each sample
//! alone its transfer curve. A slot with nothing to draw is its settings alone, with the longer
//! bars, as tall as a device in the rack, so `{` and `}` through the chain never leave the zoom.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use vt_model::insert::Insert;
use vt_model::song::Track;

use super::{
    MeterScale, REDUCTION_CAPTION, REDUCTION_METER_WIDTH, TrackCells, heard_bypassed,
    reduction_cell, slot_frame, slot_grid,
};
use crate::kitty::PanelKey;
use crate::page::Ctx;
use crate::theme;
use crate::track_page::{Control, DeviceSpec, Page, TrackPage};
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellWidths, GRID_HEIGHT, faded};
use crate::views::widgets::image;
use crate::views::widgets::meter::{amount_across, hanging_cell, hanging_eighths};
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::table::take_top;

/// How tall the zoomed slot's graphics are inside their boxes, in rows: the meter's 24 dB at 1.5
/// a row, and at the author's 15 by 39 pixels a character, 624 pixels.
pub(crate) const ZOOM_PANEL_HEIGHT: usize = 16;
/// How wide a zoomed curve of a level in against a level out is, a compressor's or a distortion's:
/// square at the author's, 630 by 624 pixels.
pub(crate) const ZOOM_CURVE_WIDTH: usize = 42;
/// How wide a zoomed EQ's curve is: all the room inside the device in the keyboard's box.
pub(crate) const ZOOM_EQ_WIDTH: usize = 140;
/// The zoomed gain reduction meter and its history: 24 dB down its rows, 0.1875 dB an eighth.
const ZOOM_METER: MeterScale = MeterScale {
    rows: ZOOM_PANEL_HEIGHT,
    db: 24.0,
};
/// The meter's scale beside its box: the dB at the top of every fourth row, 6 dB apart.
const ZOOM_SCALE: [(usize, &str); 4] = [(0, "0"), (4, "-6"), (8, "-12"), (12, "-18")];
const ZOOM_SCALE_WIDTH: u16 = 4;
/// How wide the history of a compressor is, beside its curve, its meter and its scale, and of a
/// limiter, beside its meter and scale alone, so each ends where the EQ's curve does.
const HISTORY_BESIDE_CURVE: usize = 84;
const HISTORY_ALONE: usize = 130;
/// The caption on the history's box: how long it goes back.
const HISTORY_CAPTION: &str = "2 bars";
/// How tall each slot's tile in the strip is: its frame round one line.
const TILE_HEIGHT: u16 = 3;
/// The room between two tiles, as between two half-width devices.
const TILE_GAP: u16 = 2;
/// The room inside a tile's frame, each side.
const TILE_PADDING: u16 = 1;
/// How long a tile's gain reduction bar is, and how much it covers: 4 dB a cell, as the rack's
/// meter is 4 dB a row.
const TILE_METER_CELLS: usize = 5;
const TILE_METER_DB: f32 = 20.0;
/// The room between the parts of a tile's line.
const TILE_SPACE: &str = "  ";

/// Draw the FX page zoomed in on the cursor's slot: the strip, a blank line, and the slot.
pub(super) fn draw(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx, track: &Track) {
    let devices = Page::Fx.devices(track);
    let Some(last) = devices.len().checked_sub(1) else {
        return;
    };
    let zoomed = page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device)
        .min(last);
    let mut rest = area;
    strip(
        frame,
        take_top(&mut rest, TILE_HEIGHT),
        (page, ctx, track),
        &devices,
        zoomed,
    );
    take_top(&mut rest, 1);
    let device = &devices[zoomed];
    let panel = match track.inserts[zoomed] {
        Insert::Empty => GraphicPanel::new(),
        _ => zoom_panel(page, ctx, track, zoomed),
    };
    // The graphics and a blank line under them, over the settings.
    let tall = match panel.is_empty() {
        true => 0,
        false => panel.height() + 1,
    };
    let area = take_top(&mut rest, DEVICE_HEIGHT + tall);
    if slot_frame(frame, area, page, ctx, track, device).is_none() {
        return;
    }
    let top = Device::settings_area(area);
    if !panel.is_empty() {
        let at = Rect::new(top.x, top.y, top.width, panel.height());
        frame.render_widget(panel, at);
    }
    let below = top.y + tall;
    let settings = Rect::new(
        top.x,
        below,
        top.width,
        GRID_HEIGHT.min(area.bottom().saturating_sub(below)),
    );
    slot_grid(
        frame,
        settings,
        (page, ctx, track),
        device,
        CellWidths::ZOOM,
    );
}

/// Every slot as a tile, side by side across the page, the zoomed one's frame brighter, so the
/// chain stays in sight.
fn strip(
    frame: &mut Frame,
    area: Rect,
    (page, ctx, track): (&TrackPage, &Ctx, &Track),
    devices: &[DeviceSpec],
    zoomed: usize,
) {
    let count = devices.len() as u16;
    let width = area.width.saturating_sub(TILE_GAP * (count - 1)) / count;
    for (at, device) in devices.iter().enumerate() {
        let x = area.x + at as u16 * (width + TILE_GAP);
        let tile = Rect::new(x, area.y, width, area.height);
        draw_tile(frame, tile, (page, ctx, track), device, at == zoomed);
    }
}

/// A slot's tile: its digit, its switch, its effect and its type on its frame, as its device has
/// them but with no cursor, and inside, the first row of its settings, each by its short label,
/// as many as fit, with a compressor's or a limiter's gain reduction as a bar at the end. A
/// bypassed slot's is faint.
fn draw_tile(
    frame: &mut Frame,
    area: Rect,
    (page, ctx, track): (&TrackPage, &Ctx, &Track),
    device: &DeviceSpec,
    zoomed: bool,
) {
    let Some(Control::Insert(slot)) = device.title[0] else {
        return;
    };
    let insert = track.inserts[slot];
    let empty = insert == Insert::Empty;
    let bypassed = heard_bypassed(ctx, track, slot);
    let fade = |line: Line<'static>| match bypassed {
        true => faded(line, theme::faint()),
        false => line,
    };
    let cells = TrackCells::new(page, ctx, track);
    let controls = device
        .title
        .iter()
        .flatten()
        .map(|control| fade(Line::raw(cells.title_text(*control))))
        .collect();
    let tile = Device::new(device.identity.clone());
    let tile = match empty {
        true => tile.switch_room(),
        false => tile.switch(!bypassed),
    };
    frame.render_widget(tile.controls(controls).focused(zoomed), area);
    if empty || area.height < TILE_HEIGHT {
        return;
    }
    let inner = Rect::new(
        area.x + 1 + TILE_PADDING,
        area.y + 1,
        area.width.saturating_sub(2 + 2 * TILE_PADDING),
        1,
    );
    let meter = match insert {
        Insert::Compressor(_) | Insert::Limit(_) => {
            let reduction = ctx
                .live()
                .filter(|_| !bypassed)
                .map_or(0.0, |live| live.reduction[slot]);
            let style = match zoomed {
                true => theme::active(),
                false => theme::dim(),
            };
            Some(vec![
                Span::styled(REDUCTION_CAPTION, theme::dim()),
                Span::raw(" "),
                amount_across(reduction / TILE_METER_DB, TILE_METER_CELLS, style),
            ])
        }
        _ => None,
    };
    let meter_width = meter.as_ref().map_or(0, |spans| {
        spans.iter().map(Span::width).sum::<usize>() + TILE_SPACE.len()
    });
    let room = usize::from(inner.width).saturating_sub(meter_width);
    let mut spans = Vec::new();
    let mut used = 0;
    for control in device.grid[0].iter().flatten() {
        let Control::InsertParam(_, param) = *control else {
            continue;
        };
        let Some(label) = insert.params().get(param).and_then(|found| found.label) else {
            continue;
        };
        let shown = ctx
            .param_view(*control, track)
            .map(|view| view.cell)
            .unwrap_or_default();
        let gap = if spans.is_empty() {
            0
        } else {
            TILE_SPACE.len()
        };
        let width = gap + label.chars().count() + 1 + shown.chars().count();
        if used + width > room {
            break;
        }
        if gap > 0 {
            spans.push(Span::raw(TILE_SPACE));
        }
        spans.push(Span::styled(label, theme::dim()));
        spans.push(Span::raw(format!(" {shown}")));
        used += width;
    }
    frame.render_widget(fade(Line::from(spans)), inner);
    if let Some(meter) = meter {
        let width = (meter_width - TILE_SPACE.len()) as u16;
        let at = Rect::new(inner.right().saturating_sub(width), inner.y, width, 1);
        frame.render_widget(fade(Line::from(meter)), at);
    }
}

/// The zoomed slot's graphics, left to right in their boxes: a compressor's curve, its gain
/// reduction meter with its scale, and the last two bars of it; a limiter's meter and two bars; an
/// EQ's curve; or a distortion's transfer curve. A curve's box is left out until its image has
/// been sent. A bypassed slot's meter and history are faint, as its curve is.
fn zoom_panel(page: &TrackPage, ctx: &Ctx, track: &Track, slot: usize) -> GraphicPanel {
    let color = theme::active().fg.unwrap_or(theme::TEXT);
    let bypassed = heard_bypassed(ctx, track, slot);
    let reduction = ctx
        .live()
        .filter(|_| !bypassed)
        .map_or(0.0, |live| live.reduction[slot]);
    let held = if bypassed {
        0.0
    } else {
        page.slot_reduction[slot].top
    };
    let cursor = theme::cursor(ctx.mode());
    let fade = |line: Line<'static>| if bypassed { faded(line, cursor) } else { line };
    let curve = |panel: GraphicPanel, width: usize| match page.images.get(PanelKey::Zoom) {
        Some(id) => {
            let lines = image::rows(id, width, ZOOM_PANEL_HEIGHT)
                .into_iter()
                .map(Line::from)
                .collect();
            panel.boxed(lines, width as u16, page.zoom_caption())
        }
        None => panel,
    };
    let meter = |panel: GraphicPanel| {
        let lines = (0..ZOOM_PANEL_HEIGHT)
            .map(|row| {
                let cell = reduction_cell(reduction, held, row, ZOOM_METER, color);
                fade(Line::from(cell))
            })
            .collect();
        let scale = (0..ZOOM_PANEL_HEIGHT)
            .map(|row| {
                let text = ZOOM_SCALE
                    .iter()
                    .find(|(at, _)| *at == row)
                    .map_or("", |(_, text)| text);
                Line::styled(format!("{text:>4}"), theme::faint())
            })
            .collect();
        panel
            .boxed(lines, REDUCTION_METER_WIDTH as u16, Some(REDUCTION_CAPTION))
            .beside(scale, ZOOM_SCALE_WIDTH)
    };
    let history = |panel: GraphicPanel, width: usize| {
        let lines = history_lines(page, slot, width, color)
            .into_iter()
            .map(fade)
            .collect();
        panel.boxed(lines, width as u16, Some(HISTORY_CAPTION))
    };
    let panel = GraphicPanel::new();
    match track.inserts[slot] {
        Insert::Compressor(_) => {
            history(meter(curve(panel, ZOOM_CURVE_WIDTH)), HISTORY_BESIDE_CURVE)
        }
        Insert::Limit(_) => history(meter(panel), HISTORY_ALONE),
        Insert::Eq(_)
        | Insert::Lpg(_)
        | Insert::Reso(_)
        | Insert::Comb(_)
        | Insert::Vowel(_)
        | Insert::Delay(_)
        | Insert::Echo(_)
        | Insert::Reverb(_)
        | Insert::Plate(_)
        | Insert::Spring(_)
        | Insert::Space(_) => curve(panel, ZOOM_EQ_WIDTH),
        Insert::Distort(_) => curve(panel, ZOOM_CURVE_WIDTH),
        _ => panel,
    }
}

/// The last two bars of a slot's gain reduction, a line for each row, this many columns wide:
/// each column the deepest it turned down in its share of the sweep, hanging from the top as the
/// meter does, with a blank column after the newest, where the sweep writes over the oldest.
fn history_lines(page: &TrackPage, slot: usize, width: usize, color: Color) -> Vec<Line<'static>> {
    let history = &page.reduction_history;
    let head = history.head(width);
    let gap = (head + 1) % width.max(1);
    let deepest: Vec<f32> = (0..width)
        .map(|column| history.column(slot, column, width))
        .collect();
    (0..ZOOM_METER.rows)
        .map(|row| {
            let cells: Vec<Span<'static>> = deepest
                .iter()
                .enumerate()
                .map(|(column, db)| {
                    if column == gap {
                        return Span::styled(" ", Style::new());
                    }
                    let filled = hanging_eighths(db / ZOOM_METER.db, row, ZOOM_METER.rows);
                    hanging_cell(filled, color, theme::BASE, 1)
                })
                .collect();
            Line::from(cells)
        })
        .collect()
}
