//! The Sound page. A sound track's is a rack of four devices, as the FX page's slots are: its
//! machine, with the machine and its type in the title; its filter, with the filter's type and
//! mode in the title and its curve beside its settings; its output; and how it plays its notes.
//! A group's, a return's and the master's is the same Output, with holes where they lack a
//! setting, and under a group's the tracks it sums, and under a return's the tracks that send to
//! it (the track pages study, `docs/designs/track-pages.md`, §2.3).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use vt_model::grid::GRID_ROWS;
use vt_model::settings::SettingId;
use vt_model::song::{Machine, Track, TrackKind};

use crate::graphics::PanelKey;
use crate::pages::Ctx;
use crate::pages::track::{Control, DEVICE_COLUMNS, DeviceSpec, Page, TrackPage};
use crate::services::automating::as_heard;
use crate::widgets::device::{DEVICE_HEIGHT, Device};
use crate::widgets::glyphs;
use crate::widgets::grid::{CellParts, CellSource, CellWidths, ROW_STEP, SettingsGrid};
use crate::widgets::group;
use crate::widgets::image;
use crate::widgets::meter::level_across;
use crate::widgets::panel::GraphicPanel;
use crate::widgets::param::{Face, ParamCell, ParamState, Place, Tone};
use crate::widgets::setting::Focus;
use crate::widgets::table::take_top;
use crate::widgets::theme;

use super::{
    FILTER_CURVE_HEIGHT, FILTER_CURVE_WIDTH, LARGE_PANEL_WIDTH, TrackCells, WAVEFORM_HEIGHT,
    WAVEFORM_WIDTH,
};

/// The Output meter's scale: from -60 dB at its left, 1.5 dB a character, to +6 dB, 44
/// characters, after the side's name.
const OUTPUT_METER_FLOOR_DB: f32 = -60.0;
const OUTPUT_METER_DB_PER_CELL: f32 = 1.5;
const OUTPUT_METER_CELLS: usize = 44;
/// The room before the bar for its side's name, as ` L `.
const OUTPUT_METER_LABEL: usize = 3;
/// The levels the scale under the bars names, each ending under the character it starts at.
const OUTPUT_METER_SCALE_DB: [i32; 5] = [-48, -36, -24, -12, 0];

/// The Output's meter, in its panel: each side's level right after the trim, so the trim can be
/// set by eye, as horizontal eighths in the level colours, with a faint mark at the highest it
/// was lately, and a faint scale under them. It's text, so it moves every frame for nothing.
fn output_meter(page: &TrackPage) -> Vec<Line<'static>> {
    let blank = || Line::raw(" ".repeat(LARGE_PANEL_WIDTH));
    let mut lines = vec![blank()];
    for (side, held) in ["L", "R"].into_iter().zip(page.after_trim) {
        let mut spans = vec![Span::styled(format!(" {side} "), theme::dim())];
        spans.extend(level_across(
            held.now,
            held.top,
            OUTPUT_METER_FLOOR_DB,
            OUTPUT_METER_DB_PER_CELL,
            OUTPUT_METER_CELLS,
        ));
        let rest = LARGE_PANEL_WIDTH - OUTPUT_METER_LABEL - OUTPUT_METER_CELLS;
        spans.push(Span::raw(" ".repeat(rest)));
        lines.push(Line::from(spans));
    }
    let mut scale = " ".repeat(LARGE_PANEL_WIDTH);
    for db in OUTPUT_METER_SCALE_DB {
        let text = db.to_string();
        let cell = ((db as f32 - OUTPUT_METER_FLOOR_DB) / OUTPUT_METER_DB_PER_CELL) as usize;
        let end = OUTPUT_METER_LABEL + cell + 1;
        scale.replace_range(end - text.len()..end, &text);
    }
    lines.push(Line::styled(scale, theme::faint()));
    lines.push(blank());
    lines
}

/// A device's graphic panel: the filter's curve beside the filter's settings, the thumbnail of
/// what a sampler plays beside the machine's, each in its box once its image has been sent, and
/// the Output's meter. The master's Output has none, as the main meter beside it already shows
/// what it plays (the study's §1.5, rule 5).
fn panel(page: &TrackPage, track: &Track, device: &DeviceSpec) -> GraphicPanel {
    // The Output is the device that starts with `Trim`.
    if device.grid[0][0] == Some(Control::Trim) {
        if track.kind == TrackKind::Master {
            return GraphicPanel::new();
        }
        return GraphicPanel::new().boxed(output_meter(page), LARGE_PANEL_WIDTH as u16, None);
    }
    let image = match device.title[0] {
        Some(Control::FilterType) => page
            .images
            .get(PanelKey::Filter)
            .map(|id| (id, FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT)),
        Some(Control::Machine) if track.machine == Machine::Sampler => page
            .images
            .get(PanelKey::Waveform)
            .map(|id| (id, WAVEFORM_WIDTH, WAVEFORM_HEIGHT)),
        _ => None,
    };
    match image {
        Some((id, width, height)) => {
            let lines = image::rows(id, width, height)
                .into_iter()
                .map(Line::from)
                .collect();
            GraphicPanel::new().boxed(lines, width as u16, None)
        }
        None => GraphicPanel::new(),
    }
}

/// One of the Sound page's devices: its frame and title, its settings in its grid, and its panel.
fn draw_device(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    // Its faders show where the steps have them while they drive them.
    let faders = ctx.faders(ctx.cursor().track);
    let heard = as_heard(track, &faders);
    let cells = TrackCells::new(page, ctx, &heard);
    let widget = Device::new(device.identity.clone())
        .controls(cells.titles(device))
        .focused(focused);
    frame.render_widget(widget, area);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let grid = SettingsGrid::new(&cells, rows, CellWidths::SOUND);
    frame.render_widget(grid, Device::settings_area(area));
    let panel = panel(page, track, device);
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        frame.render_widget(panel, at);
    }
}

/// The Sound page's devices, stacked down the page: a sound track's four, which fill the rows
/// inside the keyboard's box, or a fixed track's Output. Where fewer fit, the page goes by whole
/// devices to keep the cursor's in view.
fn draw_rack(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx, track: &Track) {
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    let devices = Page::Sound.devices(track);
    for (at, device) in devices.iter().enumerate().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        draw_device(frame, area, page, ctx, track, device, at == cursor);
    }
}

/// How many places a device of what feeds a fixed track has: its grid's, three down each of its
/// four columns. Past that, the last says how many more there are.
const FEED_PLACES: usize = GRID_ROWS * DEVICE_COLUMNS;

/// One place in a device of what feeds a group or a return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feed {
    /// A track, by its place in the song.
    Track(usize),
    /// How many more there are than fit.
    More(usize),
    /// Nothing at all feeds it.
    Nothing,
}

/// The places for the tracks that feed a fixed track, in order: as many as fit, the last saying
/// how many more there are when they don't, or a place saying there are none.
fn feeds(tracks: Vec<usize>) -> Vec<Feed> {
    if tracks.is_empty() {
        return vec![Feed::Nothing];
    }
    if tracks.len() <= FEED_PLACES {
        return tracks.into_iter().map(Feed::Track).collect();
    }
    let shown = FEED_PLACES - 1;
    let more = Feed::More(tracks.len() - shown);
    tracks[..shown]
        .iter()
        .copied()
        .map(Feed::Track)
        .chain(std::iter::once(more))
        .collect()
}

/// The places as a grid's rows, three down each column before the next, as the Output's settings
/// go.
fn feed_rows(feeds: &[Feed]) -> Vec<Vec<Option<Feed>>> {
    (0..GRID_ROWS)
        .map(|row| {
            (0..DEVICE_COLUMNS)
                .map(|column| feeds.get(column * GRID_ROWS + row).copied())
                .collect()
        })
        .collect()
}

/// How a track that feeds another is named: a sound track by its number and name, and a group by
/// its letters, as `G1`, in its group's colour if it has one.
fn feed_name(ctx: &Ctx, track: usize) -> Span<'static> {
    let song = ctx.song();
    let this = &song.tracks[track];
    let name = if this.is_sound() {
        format!("{:02} {}", track + 1, this.name)
    } else {
        this.kind.letter()
    };
    let style = group::style(song, track).unwrap_or_else(theme::dim);
    Span::styled(name.trim_end().to_owned(), style)
}

/// What the cells of a return's Sends in show: each track sending to it, and how much.
struct SendsIn<'a> {
    ctx: &'a Ctx<'a>,
    /// Which of the sends goes to this return.
    send: SettingId,
}

impl SendsIn<'_> {
    /// How much a track sends here, its coarse and fine parts.
    fn amount(&self, track: usize) -> (u8, u8) {
        let this = &self.ctx.song().tracks[track];
        (this.coarse(self.send).unwrap_or(0), this.fine(self.send))
    }

    /// The track, cut short of the value after it.
    fn name(&self, feed: Feed, room: u16) -> Line<'static> {
        let fit = |span: Span<'static>| {
            let text: String = span.content.chars().take(usize::from(room) - 1).collect();
            Line::from(Span::styled(text, span.style))
        };
        match feed {
            Feed::Track(track) => fit(feed_name(self.ctx, track)),
            Feed::More(more) => Line::styled(format!("+{more} more"), theme::faint()),
            Feed::Nothing => Line::styled("None", theme::dim()),
        }
    }
}

impl CellSource for SendsIn<'_> {
    type Id = Feed;

    /// The track, and its send as the Output shows one, `0C00` with the fine part dim, and its
    /// bar. The cursor never comes here.
    fn cell(&self, feed: Feed, widths: CellWidths) -> CellParts {
        let name = self.name(feed, widths.name);
        let Feed::Track(track) = feed else {
            return CellParts {
                name,
                value: Line::default(),
                bar: None,
            };
        };
        let (coarse, fine) = self.amount(track);
        let cell = ParamCell {
            name: "".into(),
            face: Face::HexFine { fine },
            text: format!("{coarse:02X}"),
            place: Some(Place {
                set: coarse,
                centred: false,
            }),
            state: ParamState {
                heard: coarse,
                ..ParamState::new(Tone::Plain)
            },
            look: self.ctx.look(Focus::Off),
        };
        CellParts {
            name,
            ..cell.parts(widths)
        }
    }
}

/// A return's Sends in, under its Output: every track and group that sends to it, with how much,
/// as the Output's settings are laid out, so they're found here rather than strip by strip on the
/// mixer. It's worked out from the song each time it's drawn, and the cursor doesn't stop in it.
fn draw_sends_in(frame: &mut Frame, area: Rect, ctx: &Ctx, send: SettingId) {
    frame.render_widget(Device::new("Sends in"), area);
    let source = SendsIn { ctx, send };
    let song = ctx.song();
    let sending = (0..song.tracks.len())
        .filter(|track| {
            let this = &song.tracks[*track];
            (this.is_sound() || this.is_group()) && source.amount(*track) != (0, 0)
        })
        .collect();
    let rows = feed_rows(&feeds(sending));
    let grid = SettingsGrid::new(&source, rows, CellWidths::SOUND);
    frame.render_widget(grid, Device::settings_area(area));
}

/// A group's Tracks, under its Output: the tracks it sums, by number and name in the group's
/// colour, each with its own mute or solo, three down each column, as the Output's settings are.
/// The cursor doesn't stop in it.
fn draw_tracks(frame: &mut Frame, area: Rect, ctx: &Ctx) {
    frame.render_widget(Device::new("Tracks"), area);
    let members = ctx.song().group_members(ctx.cursor().track).collect();
    let rows = feed_rows(&feeds(members));
    let inside = Device::settings_area(area);
    let width = CellWidths::SOUND.column();
    for (row, places) in rows.iter().enumerate() {
        for (column, feed) in places.iter().enumerate() {
            let line = match feed {
                Some(Feed::Track(member)) => member_line(ctx, *member),
                Some(Feed::More(more)) => Line::styled(format!("+{more} more"), theme::faint()),
                Some(Feed::Nothing) => Line::styled("None", theme::dim()),
                None => continue,
            };
            let x = inside.x + column as u16 * width;
            let y = inside.y + row as u16 * ROW_STEP;
            if x < inside.right() && y < inside.bottom() {
                let room = (width - CellWidths::SOUND.gap).min(inside.right() - x);
                frame.render_widget(line, Rect::new(x, y, room, 1));
            }
        }
    }
}

/// One of a group's tracks, by its number and name in the group's colour, with its own mute or
/// solo after it, dim while it's muted.
fn member_line(ctx: &Ctx, member: usize) -> Line<'static> {
    let live = ctx.levels().get(member).copied().unwrap_or_default();
    let mut name = feed_name(ctx, member);
    if live.mute {
        name.style = theme::dim();
    }
    let mut spans = vec![name];
    let glyph = if live.mute {
        Some(glyphs::MUTE)
    } else if live.solo {
        Some(glyphs::SOLO)
    } else {
        None
    };
    if let Some(glyph) = glyph {
        spans.push(Span::styled(format!(" {glyph}"), theme::active()));
    }
    Line::from(spans)
}

/// Draw the Sound page in the area under the track page's title: its rack, and under a group's or
/// a return's Output, what feeds it.
pub(super) fn draw(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx, track: &Track) {
    draw_rack(frame, area, page, ctx, track);
    let mut rest = area;
    take_top(&mut rest, DEVICE_HEIGHT);
    let below = take_top(&mut rest, DEVICE_HEIGHT);
    match track.kind {
        TrackKind::Group(_) => draw_tracks(frame, below, ctx),
        TrackKind::ReturnA => draw_sends_in(frame, below, ctx, SettingId::SEND_A),
        TrackKind::ReturnB => draw_sends_in(frame, below, ctx, SettingId::SEND_B),
        TrackKind::Sound | TrackKind::Master => {}
    }
}
