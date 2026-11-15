//! The Gen page: a device for each live generator of the phrase under the cursor, then `Empty`,
//! stacked down the page as the FX page's slots are. Each has its preview at its right, the
//! large panel the Sound page's filter has, of what it writes across its span, with the row
//! playing as a dot the terminal places (`kitty/generators.rs`).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::generator::{Generator, Span as Spanned};
use vt_model::grid::GRID_COLUMNS;
use vt_model::shape::ShapeColumn;
use vt_model::song::Track;

use crate::kitty::PanelKey;
use crate::macroing::{holds, macro_name};
use crate::page::Ctx;
use crate::services::Services;
use crate::shaping::LiveGen;
use crate::theme;
use crate::track_page::{Control, DeviceSpec, TrackPage};
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellWidths, ROW_STEP, SettingsGrid};
use crate::views::widgets::image;
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::setting::{self, Focus};
use crate::views::widgets::table::take_top;

use super::{BAR_CELLS, LARGE_PANEL_HEIGHT, LARGE_PANEL_WIDTH, TrackCells};

/// The room between the grid and the preview.
const PANEL_GAP: u16 = 2;

/// The Gen page's cells, the Sound page's, so three columns fit beside the preview: a value with
/// a bar is brief, as `9` for hits, whose bar shows how many of how many.
const GEN_CELLS: CellWidths = CellWidths::SOUND;

/// A device of four columns, a `Follow` that mutates (decision 307), has shorter bars, so the
/// fourth fits beside the preview too: 22 columns a cell.
const GEN_FOUR_CELLS: CellWidths = CellWidths {
    bar_cells: 3,
    gap: 2,
    ..CellWidths::SOUND
};

/// The macros' cells: a name as long as the Roto's, four digits and the Sound page's bars.
const MACRO_CELLS: CellWidths = CellWidths {
    name: 13,
    value: 5,
    bar_cells: BAR_CELLS,
    gap: 3,
};

/// The macros' band: the eight in two rows of four, and under them what the cursor's holds.
fn draw_macros(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    frame.render_widget(Device::new(device.identity.clone()).focused(focused), area);
    let cells = TrackCells::new(page, ctx, track);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let mut settings = Device::settings_area(area);
    frame.render_widget(SettingsGrid::new(&cells, rows, MACRO_CELLS), settings);
    if let Control::Macro(at) = page.control_here(ctx) {
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

/// Where a generator acts, for the end of its title, dim: its column and its rows, as `Note
/// 00–0F`, or an FX column's command and its lane, as `FX1 CUT lane 0C`.
fn acts_on(live: &LiveGen) -> String {
    let generator = &live.generator;
    let column = match generator.column {
        ShapeColumn::Fx { slot, command } => format!("FX{} {}", slot + 1, command.name()),
        _ => crate::shaping::generator_label(generator),
    };
    match (generator.span, generator.rows()) {
        (Spanned::Lane, Some(rows)) => {
            format!("{column} lane {:02X}", rows.len())
        }
        (_, Some(rows)) => format!("{column} {:02X}–{:02X}", rows.first, rows.last),
        (_, None) => column,
    }
}

/// The phrase the cursor's track is playing, if it's playing one, which the page's title names
/// beside the one it shows when they differ.
pub(super) fn playing_phrase(ctx: &Ctx) -> Option<u32> {
    let track = ctx.cursor().track;
    let owner = ctx.song().tracks.get(track)?;
    let chain = ctx.jam().get(track)?.chain?;
    let place = ctx.playing_place(track, chain)?;
    Some(
        owner
            .chain_position(chain, place, ctx.dealer(track))?
            .phrase,
    )
}

/// The row of its span a live generator is playing, counted from its first, while the phrase
/// it's on is the one under the cursor and is playing: on a lane, where the lane is reading.
pub(crate) fn playing_row(services: &Services, generator: &Generator) -> Option<usize> {
    let rows = generator.rows()?;
    let step = match (generator.span, generator.column) {
        (Spanned::Lane, ShapeColumn::Fx { slot, .. }) => {
            let phrase = services.phrase_here()?;
            services.lane_playheads(phrase)[slot]?
        }
        _ => services.playhead_step()?,
    };
    (rows.first..=rows.last)
        .contains(&step)
        .then(|| step - rows.first)
}

/// One generator as a device, giving back where its preview's picture is.
fn draw_generator(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) -> Option<(u16, u16)> {
    let Some(Control::Gen {
        phrase, generator, ..
    }) = device.title[0]
    else {
        return None;
    };
    let live = ctx.live_generator(ctx.cursor().track, phrase, generator)?;
    let four = device
        .grid
        .iter()
        .any(|row| row.get(GRID_COLUMNS - 1).is_some_and(Option::is_some));
    let widths = if four { GEN_FOUR_CELLS } else { GEN_CELLS };
    let cells = TrackCells::new(page, ctx, track).of_generator(&live);
    let widget = Device::new(String::new())
        .controls(cells.titles(device))
        .after(Line::styled(format!("· {}", acts_on(&live)), theme::dim()))
        .focused(focused);
    frame.render_widget(widget, area);
    let mut settings = Device::settings_area(area);
    let mut preview = None;
    let id = page.images.get(PanelKey::Gen(generator));
    if let Some(id) = id {
        let lines = image::rows(id, LARGE_PANEL_WIDTH, LARGE_PANEL_HEIGHT)
            .into_iter()
            .map(Line::from)
            .collect();
        let panel = GraphicPanel::new().boxed(lines, LARGE_PANEL_WIDTH as u16, None);
        let at = Device::panel_area(area, panel.width());
        settings.width =
            at.x.saturating_sub(settings.x + PANEL_GAP)
                .min(settings.width);
        let whole = at.width >= panel.width() && at.height >= panel.height();
        preview = whole.then(|| panel.inside_at(at));
        frame.render_widget(panel, at);
    }
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, widths);
    frame.render_widget(grid, settings);
    preview
}

/// An `Acid`'s `Loops`, under its device: each part's loop, and at the end of the title, dim,
/// when the loops come round together, their least common multiple in rows, and when the
/// phrase plays the same pass again, in passes of its span.
fn draw_loops(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    let Some(Control::Gen {
        phrase, generator, ..
    }) = device.grid.iter().flatten().flatten().next().copied()
    else {
        return;
    };
    let Some(live) = ctx.live_generator(ctx.cursor().track, phrase, generator) else {
        return;
    };
    let heard = live.generator.heard();
    let rows = live.generator.rows().map_or(1, |rows| rows.len()) as u64;
    let after = if heard.acid.loops() {
        let period = heard.acid.loops.period();
        let passes = lcm(period, rows) / rows.max(1);
        format!("· in step again every {period} rows, the phrase every {passes} passes")
    } else {
        "· no loops: every pass the same".to_owned()
    };
    let widget = Device::new(device.identity.clone())
        .after(Line::styled(after, theme::dim()))
        .focused(focused);
    frame.render_widget(widget, area);
    let cells = TrackCells::new(page, ctx, track).of_generator(&live);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, GEN_CELLS);
    frame.render_widget(grid, Device::settings_area(area));
}

fn lcm(a: u64, b: u64) -> u64 {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    a / gcd(a, b).max(1) * b
}

/// `Empty`, which `K` makes another column live from.
fn draw_empty(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    device: &DeviceSpec,
    focused: bool,
) {
    let here = device.title[0] == Some(page.control_here(ctx));
    let look = ctx.look(Focus::at(here, false));
    let text = setting::text(&look, || "Empty".to_owned());
    let style = setting::style(&look, true);
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
/// in view. A fixed track has no phrases, so it has none. It gives back where each generator's
/// preview is drawn, by its place in its phrase's list.
pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
) -> Vec<(usize, (u16, u16))> {
    let devices = page.devices(Some(track));
    // The macros alone, with no phrase for generators, say why under them.
    let alone = devices.len() == 1;
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    let mut previews = Vec::new();
    for (at, device) in devices.iter().enumerate().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        match (device.title[0], device.grid[0][0]) {
            (_, Some(Control::Macro(_))) => {
                draw_macros(frame, area, page, ctx, track, device, at == cursor);
            }
            (Some(Control::NewGenerator(_)), _) => {
                draw_empty(frame, area, page, ctx, device, at == cursor);
            }
            _ if device.is_loops() => {
                draw_loops(frame, area, page, ctx, track, device, at == cursor);
            }
            (Some(Control::Gen { generator, .. }), _) => {
                let focused = at == cursor;
                if let Some(corner) = draw_generator(frame, area, page, ctx, track, device, focused)
                {
                    previews.push((generator, corner));
                }
            }
            _ => {}
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
    previews
}
