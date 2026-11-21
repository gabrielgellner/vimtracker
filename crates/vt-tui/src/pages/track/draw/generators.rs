//! The Macros page (decision 465), the track's eight macros and the mappings of the cursor's, and
//! the phrase view's Gen page (decision 466): a device for each live generator of the phrase under
//! the cursor, each FX column's `Lane` where it has one, `Phrase` and `Empty`, stacked down the
//! area it's given as the FX page's slots are. Each has its preview at its right, the large panel
//! the Sound page's filter has, of what it writes across its span, with the row playing as a dot
//! the terminal places (`graphics/generators.rs`). The macro panel's strip is drawn here too.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use vt_model::generator::{Generator, Span as Spanned};
use vt_model::grid::GRID_COLUMNS;
use vt_model::macros::MACROS;
use vt_model::notation::{Field, Table};
use vt_model::shape::{GenParam, ShapeColumn, degree_count};
use vt_model::song::Track;

use crate::controls::{Control, DeviceSpec};
use crate::graphics::PanelKey;
use crate::input::mode::Mode;
use crate::overlays::mapping;
use crate::pages::Ctx;
use crate::pages::track::TrackPage;
use crate::pages::track::following::{column_name, empty_named};
use crate::services::generators::LiveGen;
use crate::services::{Services, walked_order};
use crate::widgets::device::{DEVICE_HEIGHT, Device};
use crate::widgets::grid::{CellParts, CellSource, CellWidths, ROW_STEP, SettingsGrid};
use crate::widgets::image;
use crate::widgets::panel::GraphicPanel;
use crate::widgets::setting::{self, CellLook, Focus};
use crate::widgets::table::take_top;
use crate::widgets::theme;
use crate::widgets::token_buffer::{self, Buffer, Marks};
use crate::widgets::tokens::{Layout, buffer_hint, has_rows_line};

use super::{BAR_CELLS, LARGE_PANEL_HEIGHT, LARGE_PANEL_WIDTH, TrackCells};

/// The room between the grid and the preview.
const PANEL_GAP: u16 = 2;

/// The Gen page's cells, the Sound page's, so three columns fit beside the preview: a value with
/// a bar is brief, as `9` for hits, whose bar shows how many of how many.
const GEN_CELLS: CellWidths = CellWidths::SOUND;

/// A device of four columns, a `Follow` that mutates, has shorter bars, so the fourth fits beside
/// the preview too: 22 columns a cell.
const GEN_FOUR_CELLS: CellWidths = CellWidths {
    bar_cells: 3,
    gap: 2,
    ..CellWidths::SOUND
};

/// The cells of a device of settings alone, `Phrase` or a lane: the Sound page's, unless its
/// columns don't fit in the room, as beside the grid, where it takes the four-column cells the
/// generators have (decision 467).
fn cells_in(device: &DeviceSpec, room: u16) -> CellWidths {
    let columns = device
        .grid
        .iter()
        .filter_map(|row| row.iter().rposition(Option::is_some))
        .max()
        .map_or(0, |last| last as u16 + 1);
    let wide = (columns * GEN_CELLS.column()).saturating_sub(GEN_CELLS.gap);
    if wide > room {
        GEN_FOUR_CELLS
    } else {
        GEN_CELLS
    }
}

/// The macros' cells: a name as long as the Roto's, four digits and the Sound page's bars.
const MACRO_CELLS: CellWidths = CellWidths {
    name: 13,
    value: 5,
    bar_cells: BAR_CELLS,
    gap: 3,
};

/// How tall the macros' device is: a device's frame round the band, and under the band the
/// table's heading and a row for each mapping of the macro with the most, or a line saying there
/// are none, so the device keeps its height as the cursor goes along the band. It's no taller
/// than `room`, but always a device's height.
fn macros_height(track: &Track, room: u16) -> u16 {
    let most = track.macros.iter().map(|held| held.maps.len()).max();
    let rows = most.unwrap_or(0).max(1) as u16;
    // The frame, the room above the band, the band's three lines, the room under it, the heading
    // and the room under the rows.
    (rows + 9).min(room).max(DEVICE_HEIGHT)
}

/// The macro whose mappings the Macros page lists: the cursor's, on the band.
fn macro_listed(page: &TrackPage, ctx: &Ctx) -> Option<usize> {
    match page.control_here(ctx) {
        Control::Macro(at) => Some(at),
        _ => None,
    }
}

/// The macros' device (decision 465): the eight in two rows of four, and under them every setting
/// the listed macro holds, with its range, its bends and its zone, as `gm`'s view lists them.
fn draw_macros(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    let listed = macro_listed(page, ctx);
    let mut widget = Device::new(device.identity.clone())
        .focused(focused)
        .held(page.framed(ctx));
    // Which macro is listed, its name after its number once it has one, and how many it holds.
    if let Some(at) = listed {
        let held = &track.macros[at];
        let mut after = format!("· Macro {}", at + 1);
        if let Some(name) = held.shown_name() {
            after.push_str(&format!(" {name}"));
        }
        match held.maps.len() {
            0 => {}
            1 => after.push_str(", 1 mapping"),
            count => after.push_str(&format!(", {count} mappings")),
        }
        widget = widget.after(Line::styled(after, theme::dim()));
    }
    frame.render_widget(widget, area);
    let cells = TrackCells::new(page, ctx, track);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let settings = Device::settings_area(area);
    frame.render_widget(SettingsGrid::new(&cells, rows, MACRO_CELLS), settings);
    // Under the band, down to the room above the frame's bottom, in from the frame's edge as the
    // mapping view's rows are from its own, so the `Setting` column lines up with the band.
    let top = settings.y + 2 * ROW_STEP;
    let left = settings.x.saturating_sub(TABLE_INDENT);
    let mut below = Rect::new(
        left,
        top,
        settings.right().saturating_sub(left),
        area.bottom().saturating_sub(top + 2),
    );
    let maps = listed.map_or(&[][..], |at| &track.macros[at].maps[..]);
    let hint = match listed {
        None => Some("On a macro, every setting it holds is listed here"),
        Some(_) if maps.is_empty() => Some("It holds nothing yet: M on a setting maps it here"),
        Some(_) => None,
    };
    if let Some(hint) = hint {
        let line = Line::styled(format!("  {hint}"), theme::dim());
        frame.render_widget(line, take_top(&mut below, 1));
        return;
    }
    let heading = Line::styled(mapping::draw::heading(), theme::dim());
    frame.render_widget(heading, take_top(&mut below, 1));
    let room = usize::from(below.height);
    let shown = if maps.len() > room {
        room.saturating_sub(1)
    } else {
        maps.len()
    };
    for map in &maps[..shown] {
        let row = mapping::draw::mapping_row(ctx, map, false, None);
        frame.render_widget(row, take_top(&mut below, 1));
    }
    if shown < maps.len() {
        let more = format!("  and {} more: gm lists them all", maps.len() - shown);
        frame.render_widget(Line::styled(more, theme::dim()), take_top(&mut below, 1));
    }
}

/// How far the mappings' table starts left of the band's settings, its `›`'s room, so its
/// settings' names line up with the macros' names.
const TABLE_INDENT: u16 = 2;

/// How many rows the macro panel takes at the bottom of the screen: its rule with the track's
/// name, the macros' names, and their values and bars.
pub(crate) const STRIP_HEIGHT: u16 = 3;

/// The macro panel's cells, eight across the screen's width: four digits and the band's bars
/// under a name, 20 columns a macro with the gap.
const STRIP_CELLS: CellWidths = CellWidths {
    name: 16,
    value: 5,
    bar_cells: BAR_CELLS,
    gap: 4,
};

/// Where the macro panel's first macro starts, in from the screen's edge as a device's settings
/// are.
const STRIP_INDENT: u16 = 2;

/// The macro panel (decision 385): the cursor's track's eight macros in a row, each its name over
/// its value and bar, drawn as the Macros page's band draws them, under a rule that names the track.
/// `cursor` is the macro the keys are on, while they're in the panel, whose value is in the
/// cursor's colour and whose rule is in the focus colour.
pub(crate) fn draw_macro_strip(frame: &mut Frame, area: Rect, ctx: &Ctx, cursor: Option<usize>) {
    let index = ctx.cursor().track;
    let Some(track) = ctx.song().tracks.get(index) else {
        return;
    };
    let shown = track.with_macros();
    let track = shown.as_ref();
    let name = if track.is_sound() {
        format!("{} {}", track.title(index), track.name)
            .trim_end()
            .to_owned()
    } else {
        crate::widgets::group::title(track)
    };
    // Its rule is in the focus colour while the keys are in it (decision 475).
    let (rule, title) = match cursor {
        Some(_) => (theme::FOCUS, Style::new().add_modifier(Modifier::BOLD)),
        None => (theme::SURFACE1, Style::new()),
    };
    let heading = Line::from(vec![
        Span::raw(" "),
        Span::styled("Macros", theme::dim()),
        Span::raw("  "),
        Span::styled(name, title),
        Span::raw(" "),
    ]);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(rule))
        .title(heading);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let cells = TrackCells::apart(ctx, track, cursor.map(Control::Macro));
    let column = STRIP_CELLS.value + STRIP_CELLS.bar() + STRIP_CELLS.gap;
    for at in 0..MACROS {
        let x = inner.x + STRIP_INDENT + at as u16 * column;
        if x >= inner.right() || inner.height < 2 {
            break;
        }
        let parts = cells.cell(Control::Macro(at), STRIP_CELLS);
        let mut put = |line: Line<'static>, dx: u16, dy: u16, width: u16| {
            let left = x + dx;
            if left < inner.right() {
                let width = width.min(inner.right() - left);
                frame.render_widget(line, Rect::new(left, inner.y + dy, width, 1));
            }
        };
        put(parts.name, 0, 0, STRIP_CELLS.name);
        put(parts.value, 0, 1, STRIP_CELLS.value);
        if let Some(bar) = parts.bar {
            put(Line::from(bar), STRIP_CELLS.value, 1, STRIP_CELLS.bar());
        }
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
        _ => crate::services::generators::generator_label(generator),
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
            .chain_position(chain, place, ctx.dealer(track), ctx.walk(track))?
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

/// One generator as a device, giving back where its preview's picture is, and in `open` where the
/// buffer open on one of its cells goes, for it to be drawn over the devices after it.
#[allow(clippy::too_many_arguments)]
fn draw_generator(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
    open: &mut Option<OpenAt>,
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
        .focused(focused)
        .held(page.framed(ctx));
    frame.render_widget(widget, area);
    let mut settings = Device::settings_area(area);
    let mut preview = None;
    // Beside the grid there's no preview: the grid's column is beside it (decision 467).
    let id = page
        .images
        .get(PanelKey::Gen(generator))
        .filter(|_| !page.beside);
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
    *open = draw_fields(frame, settings, widths, page, ctx, device, &live).or(*open);
    preview
}

/// The device's settings in a grammar, their text on their cells as typed, cut to the room to
/// the next cell in the row or the grid's edge, the token playing in the playhead's colour; and
/// where the buffer open on one of them goes (decision 463).
fn draw_fields(
    frame: &mut Frame,
    settings: Rect,
    widths: CellWidths,
    page: &TrackPage,
    ctx: &Ctx,
    device: &DeviceSpec,
    live: &LiveGen,
) -> Option<OpenAt> {
    let here = Some(page.control_here(ctx));
    let cursor = match page.has_keys(ctx) {
        true => theme::cursor(Mode::Normal),
        false => theme::cursor_away(),
    };
    let context = live.context();
    let mut open_at = None;
    for (row, places) in device.grid.iter().enumerate() {
        for (column, place) in places.iter().enumerate() {
            let Some(control @ Control::Gen { param, .. }) = *place else {
                continue;
            };
            let Some(grammar) = param.grammar(context.column) else {
                continue;
            };
            let grammar = grammar.grammar();
            let text = (grammar.from_setting)(&param.shown(&live.generator.shape, &context));
            let field = Field::new(grammar, &text, context.column).in_kit(&live.names);
            let x = settings.x + column as u16 * widths.column() + widths.name;
            let y = settings.y + row as u16 * ROW_STEP;
            // To the next setting in the row, or the grid's edge.
            let next = places[column + 1..].iter().position(Option::is_some);
            let end = next.map_or(settings.right(), |after| {
                settings.x + (column + 1 + after) as u16 * widths.column()
            });
            if page
                .tokens
                .as_ref()
                .is_some_and(|tokens| tokens.control == control)
            {
                open_at = Some(OpenAt {
                    control,
                    x: x.min(end),
                    y,
                });
            }
            if x >= end || y >= settings.bottom() {
                continue;
            }
            let room = end - x;
            let lit = (here == Some(control)).then_some(cursor);
            let playing = playing_token(ctx, control, live, &field);
            let line = token_buffer::cell_line(&field, playing, room.into(), lit);
            let buffer = frame.buffer_mut();
            Line::raw(" ".repeat(room.into())).render(Rect::new(x, y, room, 1), buffer);
            Line::from(line).render(Rect::new(x, y, room, 1), buffer);
        }
    }
    open_at
}

/// Where the buffer open on a cell goes: under the cell's value.
#[derive(Debug, Clone, Copy)]
struct OpenAt {
    control: Control,
    x: u16,
    y: u16,
}

/// The buffer open on a Gen page cell (decision 463), under its value, its rows line under the
/// text and the list `K` or `Tab` opened under the cursor's token, over the devices.
fn draw_buffer(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx, at: OpenAt) {
    let OpenAt { control, x, y } = at;
    let (
        Some(open),
        Control::Gen {
            phrase,
            generator,
            param,
        },
    ) = (page.tokens.as_ref(), control)
    else {
        return;
    };
    let track = ctx.cursor().track;
    let Some(live) = ctx.live_generator(track, phrase, generator) else {
        return;
    };
    let live = &live;
    let context = live.context();
    let field = &open.tokens.field;
    let layout = Layout::of(param, field, &live.generator.shape, &context);
    let room = Buffer::room(area);
    let table = ctx
        .song()
        .tracks
        .get(track)
        .and_then(|found| found.phrases.get(&phrase))
        .and_then(|found| Table::of_generator(&live.generator, found.steps(), 0));
    let rows = table.filter(|_| has_rows_line(param)).map(|table| {
        let lit = table.rows(field, open.at);
        let playing = ctx
            .gen_playing_row(&live.generator)
            .map(|row| table.range().first + row);
        token_buffer::rows_line(&table, field, &lit, playing, room)
    });
    let buffer = Buffer {
        title: param.name(),
        hint: buffer_hint(param),
        field,
        layout: &layout,
        marks: Marks {
            cursor: Some(open.at),
            typing_at: field.inserting().map(|insert| insert.to),
            playing: playing_token(ctx, control, live, field),
        },
        rows,
        menu: open.tokens.menu.as_ref(),
    };
    buffer.draw(frame, area, y, x.saturating_sub(2));
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
        .focused(focused)
        .held(page.framed(ctx));
    frame.render_widget(widget, area);
    let cells = TrackCells::new(page, ctx, track).of_generator(&live);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, GEN_CELLS);
    frame.render_widget(grid, Device::settings_area(area));
}

/// A `Random`'s `Weights` on notes: a cell for each degree, named by its note, and at the end of
/// its title, dim, how many of them `Steps` lets through.
fn draw_weights(
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
    let after = match live.generator.column {
        ShapeColumn::Note { scale, .. } => {
            let kept = live.generator.heard().random.kept(scale);
            format!(
                "· from the root, Steps lets {kept} of {} through",
                degree_count(scale)
            )
        }
        _ => String::new(),
    };
    let widget = Device::new(device.identity.clone())
        .after(Line::styled(after, theme::dim()))
        .focused(focused)
        .held(page.framed(ctx));
    frame.render_widget(widget, area);
    let playing = playing_weight(ctx, phrase, generator, &live).map(|degree| Control::Gen {
        phrase,
        generator,
        param: GenParam::WEIGHTS[degree],
    });
    let cells = Playing {
        cells: TrackCells::new(page, ctx, track).of_generator(&live),
        playing,
    };
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, GEN_CELLS);
    frame.render_widget(grid, Device::settings_area(area));
}

/// A live generator's `Weights` as a field of tokens, from its shape, for its token table to be
/// read by: none for a generator without them.
pub(crate) fn weights_field(live: &LiveGen) -> Option<Field> {
    let column = live.generator.column;
    let grammar = GenParam::Weights.typing(column).grammar()?.grammar();
    if !GenParam::Weights.exists_for(&live.generator.shape, column, false) {
        return None;
    }
    let text = GenParam::Weights.shown(&live.generator.shape, &live.context());
    Some(Field::new(grammar, &text, column))
}

/// The degree whose weight wrote the row a live generator is playing, while its phrase plays:
/// from the token table of the pass it's on, which the edit thread renders itself (decision 460).
fn playing_weight(ctx: &Ctx, phrase: u32, generator: usize, live: &LiveGen) -> Option<usize> {
    let track = ctx.cursor().track;
    let row = ctx.gen_playing_row(&live.generator)?;
    let steps = ctx.song().tracks.get(track)?.phrases.get(&phrase)?.steps();
    let field = weights_field(live)?;
    let pass = ctx.heard_pass(track, phrase, generator);
    let table = Table::of_generator(&live.generator, steps, pass)?;
    table.token_of(&field, table.range().first + row)
}

/// The token of a field on the Gen page that wrote the row its generator is playing, while its
/// phrase plays, from the token table of the pass it's on (decision 460).
fn playing_token(ctx: &Ctx, control: Control, live: &LiveGen, field: &Field) -> Option<usize> {
    let Control::Gen {
        phrase, generator, ..
    } = control
    else {
        return None;
    };
    let track = ctx.cursor().track;
    let row = ctx.gen_playing_row(&live.generator)?;
    let steps = ctx.song().tracks.get(track)?.phrases.get(&phrase)?.steps();
    let pass = ctx.heard_pass(track, phrase, generator);
    let table = Table::of_generator(&live.generator, steps, pass)?;
    table.token_of(field, table.range().first + row)
}

/// A device's cells with one named in the playhead's colour: on `Weights`, the weight of the
/// degree the row playing is on.
struct Playing<'a> {
    cells: TrackCells<'a>,
    playing: Option<Control>,
}

impl CellSource for Playing<'_> {
    type Id = Control;

    fn cell(&self, control: Control, widths: CellWidths) -> CellParts {
        let mut parts = self.cells.cell(control, widths);
        if self.playing == Some(control) {
            for span in &mut parts.name.spans {
                span.style = span.style.patch(theme::active());
            }
        }
        parts
    }
}

/// A lane's length and division in words, as `4 rows` or `4 rows of 2 steps`.
fn lane_words(length: usize, division: usize) -> String {
    let rows = if length == 1 { "row" } else { "rows" };
    match division {
        1 => format!("{length} {rows}"),
        _ => format!("{length} {rows} of {division} steps"),
    }
}

/// An FX column's `Lane`: the track's `Length` and `Rate` for the column, which move every
/// phrase's lane on it, and at the end of its title, dim, how the lane of the phrase on the page
/// is heard against its own.
fn draw_lane(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    let Some(slot) = device.lane_slot() else {
        return;
    };
    let phrase = page.phrase.and_then(|id| track.phrases.get(&id));
    let after = phrase.and_then(|phrase| {
        let lane = phrase.lane(slot)?;
        let own = lane_words(lane.length, lane.division);
        let heard = ctx.heard_lanes(phrase)[slot]?;
        Some(
            if (heard.length, heard.division) == (lane.length, lane.division) {
                format!("· here {own}, as written")
            } else {
                let heard = lane_words(heard.length, heard.division);
                format!("· here {heard}, its own {own}")
            },
        )
    });
    let widget = Device::new(device.identity.clone())
        .after(Line::styled(after.unwrap_or_default(), theme::dim()))
        .focused(focused)
        .held(page.framed(ctx));
    frame.render_widget(widget, area);
    let cells = TrackCells::new(page, ctx, track);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let settings = Device::settings_area(area);
    let grid = SettingsGrid::new(&cells, rows, cells_in(device, settings.width));
    frame.render_widget(grid, settings);
}

/// `Phrase`, the track's settings that move its phrases' walks, with the order the phrase under the
/// cursor plays in at the end of its title, dim: `· here Hopscotch, its own Jump 3`, as the
/// engine last read the settings while it plays and as the track holds them when stopped.
fn draw_phrase(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    let phrase = page.phrase.and_then(|id| track.phrases.get(&id));
    let after = phrase.map(|phrase| {
        let own = phrase.order().to_string();
        match walked_order(phrase, ctx.walk(ctx.cursor().track)) {
            Some(heard) => format!("· here {heard}, its own {own}"),
            None => format!("· here {own}, as written"),
        }
    });
    let widget = Device::new(device.identity.clone())
        .after(Line::styled(after.unwrap_or_default(), theme::dim()))
        .focused(focused)
        .held(page.framed(ctx));
    frame.render_widget(widget, area);
    let cells = TrackCells::new(page, ctx, track);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let settings = Device::settings_area(area);
    let grid = SettingsGrid::new(&cells, rows, cells_in(device, settings.width));
    frame.render_widget(grid, settings);
}

fn lcm(a: u64, b: u64) -> u64 {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    a / gcd(a, b).max(1) * b
}

/// The column `Empty` names beside the grid: the grid's cursor's, when it has no generator and no
/// lane.
fn empty_beside(page: &TrackPage, ctx: &Ctx) -> Option<vt_model::selection::Field> {
    let cursor = ctx.cursor();
    page.beside
        .then(|| empty_named(ctx.phrase_here()?, cursor.field, cursor.step))
        .flatten()
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
    let look = CellLook {
        away: !page.has_keys(ctx),
        ..ctx.look(Focus::at(here, false))
    };
    let text = setting::text(&look, || "Empty".to_owned());
    let style = setting::style(&look, true);
    // Beside the grid, it names the grid's cursor's column when that has nothing live, which `K`
    // makes live (decision 468).
    let named = empty_beside(page, ctx);
    let mut widget = Device::new(String::new())
        .controls(vec![Line::styled(text, style)])
        .focused(focused)
        .held(page.framed(ctx));
    if let Some(field) = named {
        widget = widget.after(Line::styled(
            format!("· {}", column_name(field)),
            theme::dim(),
        ));
    }
    frame.render_widget(widget, area);
    let mut row = Device::settings_area(area);
    take_top(&mut row, ROW_STEP);
    let hint = match named {
        Some(field) => format!("  make {} live over the phrase", column_name(field)),
        None => EMPTY_HINT.to_owned(),
    };
    let hint = Line::from(vec![Span::raw("K"), Span::styled(hint, theme::dim())]);
    frame.render_widget(hint, take_top(&mut row, 1));
}

/// The Macros page (decision 465): the macros' device alone, as tall as its mappings need.
pub(super) fn draw_macros_page(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
) {
    let mut rest = area;
    if let Some(device) = page.devices(Some(track)).first() {
        let area = take_top(&mut rest, macros_height(track, area.height));
        draw_macros(frame, area, page, ctx, track, device, true);
    }
}

/// Why Gen has no devices on a track: there's no phrase under the cursor, or it's a fixed track,
/// which has no phrases.
pub(crate) fn no_generators(track: &Track) -> &'static str {
    if track.is_sound() {
        "There's no phrase here: put one in the chain to make its columns live"
    } else {
        "A fixed track has no phrases, so it has no generators"
    }
}

/// The Gen page (decision 466): the generators' devices, the lanes', `Phrase` and `Empty`, stacked
/// down the area it's given and going by whole devices to keep the cursor's in view. With no
/// phrase there are none, and it says why. It gives back where each generator's preview is
/// drawn, by its place in its phrase's list.
pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
) -> Vec<(usize, (u16, u16))> {
    let devices = page.devices(Some(track));
    if devices.is_empty() {
        let why = Line::styled(no_generators(track), theme::dim());
        frame.render_widget(Paragraph::new(why), area);
        return Vec::new();
    }
    let height = |_: usize| DEVICE_HEIGHT;
    let cursor = page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    // From the cursor's device up, as many as fit above it, so it's as low as it can be.
    let mut first = cursor;
    let mut taken = height(cursor);
    while first > 0 && taken + height(first - 1) <= area.height {
        first -= 1;
        taken += height(first);
    }
    let mut rest = area;
    let mut previews = Vec::new();
    let mut open = None;
    for (at, device) in devices.iter().enumerate().skip(first) {
        if at > cursor && rest.height < height(at) {
            break;
        }
        let area = take_top(&mut rest, height(at));
        match (device.title[0], device.grid[0][0]) {
            (Some(Control::NewGenerator(_)), _) => {
                draw_empty(frame, area, page, ctx, device, at == cursor);
            }
            _ if device.is_loops() => {
                draw_loops(frame, area, page, ctx, track, device, at == cursor);
            }
            _ if device.is_weights() => {
                draw_weights(frame, area, page, ctx, track, device, at == cursor);
            }
            (_, Some(Control::Lane(..))) => {
                draw_lane(frame, area, page, ctx, track, device, at == cursor);
            }
            _ if device.is_phrase() => {
                draw_phrase(frame, area, page, ctx, track, device, at == cursor);
            }
            (Some(Control::Gen { generator, .. }), _) => {
                let focused = at == cursor;
                let drawn =
                    draw_generator(frame, area, page, ctx, track, device, focused, &mut open);
                if let Some(corner) = drawn {
                    previews.push((generator, corner));
                }
            }
            _ => {}
        }
    }
    // Beside the grid, the buffer opens over the panel, never the grid (decision 467); on the
    // whole page, wherever it has room.
    if let Some(at) = open {
        let room = if page.beside { area } else { frame.area() };
        draw_buffer(frame, room, page, ctx, at);
    }
    previews
}
