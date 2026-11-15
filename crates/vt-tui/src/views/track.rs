//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::macros::MacroTarget;
use vt_model::song::{Song, Track, TrackKind};

use crate::app::App;
use crate::glyphs;
use crate::input::mode::Mode;
use crate::kitty::PanelKey;
use crate::page::Ctx;
use crate::theme;
use crate::track_page::{ArpField, Control, DeviceSpec, ModField, Page, TrackPage};
use crate::views::phrase::level_glyph;
use crate::views::widgets;
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellWidths, ROW_STEP, SettingsGrid, faded};
use crate::views::widgets::image;
use crate::views::widgets::meter::{hanging_cell, hanging_eighths};
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::table::take_top;
use crate::views::widgets::{group, role};

use cells::TrackCells;

mod cells;
mod generators;
mod modulation;
#[cfg(test)]
mod parity;
mod sound;
mod zoom;

pub(crate) use generators::playing_row as gen_playing_row;

pub use modulation::CyclesAt;
pub(crate) use modulation::{MOD_PANEL_HEIGHT, MOD_PANEL_WIDTH, mod_being_edited};
pub(crate) use zoom::{ZOOM_CURVE_WIDTH, ZOOM_EQ_WIDTH, ZOOM_PANEL_HEIGHT};

/// The size of the Sound page's large panels, in characters: the filter's curve, a sampler's
/// thumbnail and the Output's meter, each beside a grid of two columns. At the author's 15 by 39
/// pixels a character the pictures are 735 by 195.
pub(crate) const LARGE_PANEL_WIDTH: usize = 49;
pub(crate) const LARGE_PANEL_HEIGHT: usize = 5;
/// The size of the filter's curve, in characters, the large panel's.
pub(crate) const FILTER_CURVE_WIDTH: usize = LARGE_PANEL_WIDTH;
pub(crate) const FILTER_CURVE_HEIGHT: usize = LARGE_PANEL_HEIGHT;
/// The size of a sampler's thumbnail, in the Machine's panel, as big as the filter's curve under
/// it, so the two boxes line up.
pub(crate) const WAVEFORM_WIDTH: usize = LARGE_PANEL_WIDTH;
pub(crate) const WAVEFORM_HEIGHT: usize = LARGE_PANEL_HEIGHT;

/// The size of a compressor's curve on the FX page, in characters, the graphic panel's small
/// size, which is square at the author's 15 by 39 pixels a character: 195 by 195. The rows a
/// slot's drawing takes are all of a panel's, inside its box.
pub(crate) const DYNAMICS_CURVE_WIDTH: usize = 13;
pub(crate) const SLOT_DRAWING_HEIGHT: usize = 5;
/// The size of an EQ's curve on the FX page, in characters, the graphic panel's middle size: 495
/// by 195 pixels at the author's.
pub(crate) const EQ_CURVE_WIDTH: usize = 33;
/// How much a gain reduction meter covers, in dB, from the top of its rows to the bottom, which
/// is half a dB an eighth of a row, and how wide it is, which leaves room for its box's caption.
const REDUCTION_METER_DB: f32 = 20.0;
const REDUCTION_METER_WIDTH: usize = 2;
/// The caption on a gain reduction meter's box.
const REDUCTION_CAPTION: &str = "GR";
/// The meter's scale beside its box, a line for each row: the dB at the top of every other row.
const REDUCTION_SCALE: [&str; SLOT_DRAWING_HEIGHT] = ["   0", "", "  -8", "", " -16"];
const REDUCTION_SCALE_WIDTH: u16 = 4;

/// What an empty slot says, under its title.
const EMPTY_SLOT: &str = "Empty";
/// The row of an empty slot's settings its hint is on, the middle one.
const EMPTY_HINT_ROW: u16 = ROW_STEP;
/// Cells in a value's bar, each split into eighths. An odd number, so a centre has a cell.
const BAR_CELLS: usize = 9;

/// Whether a setting that's off or on, and nothing else, is on, or nothing for any other: an
/// effect's `Off`/`On` list, an envelope's `Reset`, the arpeggio's `Retrig`, the filter's
/// `Safe`, and the master's `Limiter`, which is the song's. They show as a toggle, and the status
/// line still says the word.
pub(crate) fn toggle_on(control: Control, track: &Track, song: &Song) -> Option<bool> {
    match control {
        Control::Limiter => Some(song.limiter),
        Control::Safe => Some(track.safe),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            found.is_toggle().then(|| insert.value(param) != 0)
        }
        Control::Mod(slot, ModField::Reset) => Some(track.mods[slot].envelope().reset()),
        Control::Arpeggio(ArpField::Retrig) => Some(track.arpeggio.retrig),
        _ => None,
    }
}

/// Whether the cursor is in a sound track's Machine device, on the machine, its type, its
/// sample or one of its settings, so its thumbnail is the focused one.
pub(crate) fn machine_being_edited(page: &TrackPage, ctx: &Ctx) -> bool {
    page.page == Page::Sound
        && matches!(
            page.control_here(ctx),
            Control::Machine | Control::Sample | Control::Setting(_)
        )
}

/// Whether the cursor is on something the filter's curve shows.
pub(crate) fn filter_being_edited(page: &TrackPage, ctx: &Ctx) -> bool {
    matches!(
        page.control_here(ctx),
        Control::FilterType
            | Control::FilterMode
            | Control::Safe
            | Control::Base(
                TrackControl::Cutoff
                    | TrackControl::Resonance
                    | TrackControl::Drive
                    | TrackControl::Punch
                    | TrackControl::LowCut
                    | TrackControl::HighCut
            )
    )
}

/// Which FX slot the cursor is on, if any.
pub(crate) fn slot_being_edited(page: &TrackPage, ctx: &Ctx) -> Option<usize> {
    match page.control_here(ctx) {
        Control::Insert(slot) | Control::InsertParam(slot, _) => Some(slot),
        _ => None,
    }
}

/// Whether a slot is switched out as it's heard: while the song plays, a step's lock of its bypass
/// counts, so the slot fades and comes back with the lock, and otherwise it's the slot's own.
pub(crate) fn heard_bypassed(ctx: &Ctx, track: &Track, slot: usize) -> bool {
    match ctx.live() {
        Some(live) if ctx.transport() == Transport::Playing => live.bypassed[slot],
        _ => track.bypassed[slot],
    }
}

/// The range a macro holding a setting can take it over, in coarse values, for its bar.
pub(crate) fn macro_range(control: Control, track: &Track) -> Option<(u8, u8)> {
    let id = control.setting_for_macro(track)?;
    let (_, map) = track.macro_holding(MacroTarget::Setting(id))?;
    Some(((map.min >> 8) as u8, (map.max >> 8) as u8))
}

/// A slot as a device: its frame and its title, with its bypass's toggle, then its settings in its
/// effect's grid, or a hint if it's empty, and its graphic panel. A bypassed slot's contents are
/// faint but for the cursor and its images.
fn draw_slot(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
) {
    let Some(slot) = slot_frame(frame, area, page, ctx, track, device) else {
        return;
    };
    slot_grid(
        frame,
        Device::settings_area(area),
        (page, ctx, track),
        device,
        CellWidths::FX,
    );
    if track.inserts[slot] == Insert::Empty {
        return;
    }
    let panel = slot_panel(page, ctx, track, slot);
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        frame.render_widget(panel, at);
    }
}

/// A slot's frame and its title, with its bypass's toggle, as the rack and the zoom both draw it,
/// giving back which slot it is.
fn slot_frame(
    frame: &mut Frame,
    area: Rect,
    page: &TrackPage,
    ctx: &Ctx,
    track: &Track,
    device: &DeviceSpec,
) -> Option<usize> {
    let Some(Control::Insert(slot)) = device.title[0] else {
        return None;
    };
    let cursor = theme::cursor(ctx.mode());
    let fade = heard_bypassed(ctx, track, slot).then_some(cursor);
    let mut controls = TrackCells::new(page, ctx, track).titles(device);
    if let Some(keep) = fade {
        controls = controls.into_iter().map(|line| faded(line, keep)).collect();
    }
    let frame_widget = Device::new(device.identity.clone());
    // Its bypass as it's heard, so a lock flips it with its step. An empty slot has none.
    let frame_widget = match track.inserts[slot] {
        Insert::Empty => frame_widget.switch_room(),
        _ => frame_widget.switch(fade.is_none()),
    };
    let frame_widget = frame_widget
        .controls(controls)
        .focused(slot_being_edited(page, ctx) == Some(slot));
    frame.render_widget(frame_widget, area);
    Some(slot)
}

/// A slot's settings in its effect's grid, in cells of these widths, or a hint if it's empty,
/// faint while it's bypassed but for the cursor.
fn slot_grid(
    frame: &mut Frame,
    settings: Rect,
    (page, ctx, track): (&TrackPage, &Ctx, &Track),
    device: &DeviceSpec,
    widths: CellWidths,
) {
    let Some(Control::Insert(slot)) = device.title[0] else {
        return;
    };
    if track.inserts[slot] == Insert::Empty {
        let hint = Line::from(vec![
            Span::raw("K"),
            Span::styled("  pick an effect", theme::dim()),
        ]);
        let mut row = settings;
        take_top(&mut row, EMPTY_HINT_ROW);
        frame.render_widget(hint, take_top(&mut row, 1));
        return;
    }
    let fade = heard_bypassed(ctx, track, slot).then_some(theme::cursor(ctx.mode()));
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let source = TrackCells::new(page, ctx, track);
    let grid = SettingsGrid::new(&source, rows, widths).faded(fade);
    frame.render_widget(grid, settings);
}

/// The FX page: a device for each slot, stacked down the page. Each is as tall whatever it holds,
/// so four fill the rows inside the keyboard's box. Where fewer fit, the page goes by whole devices
/// to keep the cursor's in view. Zoomed, it's the cursor's slot alone, bigger, under a strip of
/// them all.
fn draw_fx(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx, track: &Track) {
    if page.zoomed {
        zoom::draw(frame, area, page, ctx, track);
        return;
    }
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    for device in Page::Fx.devices(track).iter().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        draw_slot(frame, area, page, ctx, track, device);
    }
}

/// How tall a gain reduction meter is in rows, and how many dB it covers top to bottom.
#[derive(Debug, Clone, Copy)]
struct MeterScale {
    rows: usize,
    db: f32,
}

/// The rack's: half a dB an eighth.
const RACK_METER: MeterScale = MeterScale {
    rows: SLOT_DRAWING_HEIGHT,
    db: REDUCTION_METER_DB,
};

/// Where a gain reduction meter's held mark goes for the deepest it has turned down lately, if it
/// has: the row that depth ends in, and a faint line at that row's top or bottom, whichever is
/// nearer, so it's within a quarter of a row, 1 dB in the rack's.
fn held_mark(held: f32, scale: MeterScale) -> Option<(usize, char)> {
    let rows = scale.rows;
    let eighths = ((held / scale.db).clamp(0.0, 1.0) * (rows * 8) as f32).round();
    let eighths = eighths as usize;
    let row = eighths.checked_sub(1)? / 8;
    let into = eighths - row * 8;
    Some((row, if into <= 4 { '▔' } else { '▁' }))
}

/// One row of a gain reduction meter, which hangs down from the top of its rows as it turns down
/// more, as the main meter's does, with a faint mark where it went deepest lately, while it isn't
/// as deep now. An empty one that holds nothing shows a faint mark at the top, so it's seen to be
/// there.
fn reduction_cell(
    db: f32,
    held: f32,
    row: usize,
    scale: MeterScale,
    color: Color,
) -> Span<'static> {
    let filled = hanging_eighths(db / scale.db, row, scale.rows);
    if filled == 0 {
        let mark = match held_mark(held, scale) {
            Some((at, mark)) => (at == row).then_some(mark),
            None => (row == 0).then_some('▔'),
        };
        if let Some(mark) = mark {
            return Span::styled(
                mark.to_string().repeat(REDUCTION_METER_WIDTH),
                theme::faint(),
            );
        }
    }
    hanging_cell(filled, color, theme::BASE, REDUCTION_METER_WIDTH)
}

/// A slot's graphic panel on the FX page: a compressor's curve and how far it's turning down, a
/// limiter's meter alone, or an EQ's curve, each in its box, with the meter's scale beside it. A
/// curve's box is left out until its image has been sent. A bypassed slot's meter is faint, and
/// so is its curve, which is an image of its own.
fn slot_panel(page: &TrackPage, ctx: &Ctx, track: &Track, slot: usize) -> GraphicPanel {
    let style = if slot_being_edited(page, ctx) == Some(slot) {
        theme::active()
    } else {
        theme::dim()
    };
    let color = style.fg.unwrap_or(theme::TEXT);
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
    let meter = || {
        let lines = (0..SLOT_DRAWING_HEIGHT)
            .map(|row| {
                let cell = reduction_cell(reduction, held, row, RACK_METER, color);
                fade(Line::from(cell))
            })
            .collect();
        (lines, REDUCTION_METER_WIDTH as u16, Some(REDUCTION_CAPTION))
    };
    let curve = |width: usize| {
        page.images.get(PanelKey::Slot(slot)).map(|id| {
            let lines = image::rows(id, width, SLOT_DRAWING_HEIGHT)
                .into_iter()
                .map(Line::from)
                .collect();
            (lines, width as u16, None)
        })
    };
    let (parts, scale) = match track.inserts[slot] {
        Insert::Compressor(_) => ([curve(DYNAMICS_CURVE_WIDTH), Some(meter())], true),
        Insert::Limit(_) => ([Some(meter()), None], true),
        Insert::Eq(_) => ([curve(EQ_CURVE_WIDTH), None], false),
        _ => ([None, None], false),
    };
    let panel = parts
        .into_iter()
        .flatten()
        .fold(GraphicPanel::new(), |panel, (lines, width, caption)| {
            panel.boxed(lines, width, caption)
        });
    if !scale {
        return panel;
    }
    let scale = REDUCTION_SCALE
        .iter()
        .map(|text| Line::styled(format!("{text:>4}"), theme::faint()))
        .collect();
    panel.beside(scale, REDUCTION_SCALE_WIDTH)
}

/// Draw the view: the program's track page, given the rest of the program.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    app.track_page.draw(frame, area, &app.ctx());
}

/// Where the pictures that carry a dot are, as the track page drew them.
#[derive(Debug, Clone, Default)]
pub struct Drawn {
    /// The Mod page's LFO cycles and Track maps, by slot.
    pub cycles: CyclesAt,
    /// The Gen page's previews, by their generator's place in its phrase's list.
    pub previews: Vec<(usize, (u16, u16))>,
}

impl TrackPage {
    /// Draw the page, giving back where the Mod page's LFO cycles and the Gen page's previews
    /// are, for their dots.
    pub fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Drawn {
        draw_page(frame, area, self, ctx)
    }
}

/// Draw the page, as [`TrackPage::draw`].
fn draw_page(frame: &mut Frame, area: Rect, page: &TrackPage, ctx: &Ctx) -> Drawn {
    // While the keyboard plays the sound, the page sits in a box of that mode's colour.
    let area = if ctx.mode() == Mode::Keys {
        let block = widgets::window_border()
            .border_style(Style::new().fg(theme::YELLOW))
            .title(" Keys ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else {
        area
    };
    let Some(track) = ctx.song().tracks.get(ctx.cursor().track) else {
        frame.render_widget(Paragraph::new("This track doesn't exist."), area);
        return Drawn::default();
    };
    // Each setting a macro holds shows where the macro puts it.
    let shown = track.with_macros();
    let track = shown.as_ref();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let tabs = Page::ALL.map(|tab| {
        let text = format!(" {} ", tab.name());
        if tab == page.page {
            Span::styled(text, bold)
        } else {
            Span::styled(text, theme::dim())
        }
    });
    // A sound track's number and name, with what it's for as the rule under them, as on the mixer,
    // or a fixed track's title over its rule.
    let (lead, name) = if track.is_sound() {
        let name = format!("{:02} {}", ctx.cursor().track + 1, track.name);
        ("Track ", name.trim_end().to_owned())
    } else {
        ("", group::title(track))
    };
    // The track's machine or kind as a glyph before it, a group's in its colour, and whether it's
    // muted or soloed after.
    let icon = match track.kind {
        TrackKind::Group(_) => group::style(ctx.song(), ctx.cursor().track).unwrap_or_default(),
        _ => theme::dim(),
    };
    let mut title = vec![
        Span::styled(format!("{} ", glyphs::track(track)), icon),
        Span::styled(lead, bold),
    ];
    title.extend(role::ruled(ctx.song(), ctx.cursor().track, &name, bold));
    title.push(Span::styled(" ", bold));
    // A track in a group carries its mark, in the group's colour, as on the mixer.
    if let Some(mark) = group::mark(ctx.song(), ctx.cursor().track) {
        title.extend([mark.patch_style(bold), Span::raw(" ")]);
    }
    if let Some((glyph, style)) = level_glyph(ctx, ctx.cursor().track) {
        title.push(Span::styled(format!("{glyph} "), style));
    }
    title.extend(tabs);
    if page.page == Page::Gen
        && let Some(phrase) = page.phrase
    {
        title.push(Span::styled(format!("  Phrase {phrase:02X}"), bold));
        if let Some(playing) = generators::playing_phrase(ctx).filter(|id| *id != phrase) {
            title.push(Span::styled(format!("  ▶ {playing:02X}"), theme::dim()));
        }
    }
    let mut rest = area;
    frame.render_widget(Paragraph::new(Line::from(title)), take_top(&mut rest, 2));
    match page.page {
        Page::Sound => sound::draw(frame, rest, page, ctx, track),
        Page::Fx => draw_fx(frame, rest, page, ctx, track),
        Page::Mod => {
            return Drawn {
                cycles: modulation::draw(frame, rest, page, ctx, track),
                ..Drawn::default()
            };
        }
        Page::Gen => {
            return Drawn {
                previews: generators::draw(frame, rest, page, ctx, track),
                ..Drawn::default()
            };
        }
    }
    Drawn::default()
}
