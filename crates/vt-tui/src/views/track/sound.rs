//! The Sound page. A sound track's is a rack of four devices, as the FX page's slots are: its
//! machine, with the machine and its type in the title; its filter, with the filter's type and
//! mode in the title and its curve beside its settings; its output; and how it plays its notes.
//! A group's, a return's and the master's are rows of their output's settings, a group's beside
//! the tracks it sums.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Row;
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::settings::{Kind, SettingId};
use vt_model::song::{Machine, Track};

use crate::app::App;
use crate::glyphs;
use crate::theme;
use crate::track_page::{Control, DeviceSpec, Page};
use crate::views::widgets::bar::{BAR_ENDS, ValueBar};
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellSource, CellWidths, SettingsGrid};
use crate::views::widgets::group;
use crate::views::widgets::image;
use crate::views::widgets::meter::level_across;
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::{Cells, Columns, take_top};

use super::{
    BAR_CELLS, FILTER_CURVE_HEIGHT, FILTER_CURVE_WIDTH, LARGE_PANEL_WIDTH, WAVEFORM_HEIGHT,
    WAVEFORM_WIDTH, at_default, value,
};

/// How wide a value is on the Sound page, its coarse digits and its fine ones, as `A000`, and its
/// coarse digits alone.
const VALUE_WIDTH: usize = 4;
const COARSE_WIDTH: usize = 2;

/// The rows of a fixed track's Sound page, in each half: a setting's name, its value, its fine
/// part and a gap, and its bar. A word takes all of those after the name, and the room after the
/// bar; a choice's name takes the bar's room and what's after it. A gap parts the halves.
const SOUND_LABEL: u16 = 10;
const SOUND_VALUE: u16 = 2;
const SOUND_FINE: u16 = 3;
const SOUND_BAR: u16 = BAR_CELLS as u16 + BAR_ENDS;
const SOUND_AFTER_BAR: u16 = FILE_WIDTH - SOUND_VALUE - SOUND_FINE - SOUND_BAR;
const SOUND_GAP: u16 = 2;
const SOUND_HALF: [u16; 6] = [
    SOUND_LABEL,
    SOUND_VALUE,
    SOUND_FINE,
    SOUND_BAR,
    SOUND_AFTER_BAR,
    SOUND_GAP,
];
/// The room for a half's word, such as a long name.
const FILE_WIDTH: u16 = 24;

/// What a control with a bar is playing with right now.
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

/// The choice a machine's setting that's a list is on, such as the sampler's `Play` or 6-op's
/// patch.
fn choice_name(control: Control, track: &Track) -> Option<String> {
    let id = control.setting_on(track)?;
    let Kind::Stepped { choices, .. } = id.entry().kind else {
        return None;
    };
    choices
        .get(usize::from(track.setting(id)))
        .map(|choice| (*choice).to_owned())
}

/// The value a control shows a bar for, and how. True is a mark either side of a centre.
fn bar_value(control: Control, track: &Track) -> Option<(u8, bool)> {
    let level = |value| Some((value, false));
    match control {
        Control::Level => level(track.level),
        // 0 dB is in the middle, so it shows a cut or a boost either side of it, as pan does.
        Control::Trim => Some((track.trim, true)),
        Control::Base(TrackControl::Pan) => Some((track.pan, true)),
        Control::Base(TrackControl::Pitch) => Some((track.pitch, true)),
        // Those with a middle that's a setting of its own show where they are around it.
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
        Control::Base(TrackControl::LowCut) => level(track.low_cut),
        Control::Base(TrackControl::HighCut) => level(track.high_cut),
        _ => None,
    }
}

/// A setting's value's style on the Sound page: as any page's, but the group a track is in is in
/// its colour, as on the mixer.
fn value_style(app: &App, track: &Track, control: Control, here: bool) -> Style {
    let style = setting::style(app, here, at_default(control, track));
    match group::style(app.song(), app.cursor.track) {
        Some(tint) if control == Control::Group && !here => tint,
        _ => style,
    }
}

/// What a Sound device's cells show, read from the track.
struct SoundCells<'a> {
    app: &'a App,
    track: &'a Track,
}

impl CellSource for SoundCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        name_line(self.app, control.label_on(self.track), &labels, room)
    }

    /// A value and its fine part, set back, as `A000`, with the cursor on either; a word, as
    /// `1/16` or the sampler's `One-shot`; or a slice's number with its name after it where a bar
    /// would be, as `00   whole`.
    fn value(&self, control: Control) -> Line<'static> {
        let (app, track) = (self.app, self.track);
        let here = control == app.track_control();
        let on_fine = here && app.track_page.fine;
        let on_value = here && !on_fine;
        let style = value_style(app, track, control, on_value);
        let slice = control.setting_on(track) == Some(SettingId::SLICE);
        let word = match control.setting_on(track).map(|id| id.entry().kind) {
            Some(Kind::Stepped { .. }) if !slice => choice_name(control, track),
            _ => None,
        };
        let text = setting::text(app, on_value, || {
            word.unwrap_or_else(|| value(control, track, app.song()))
        });
        let fine = control.fine_on(track);
        // What's being typed keeps the fine digits where they are, as `_ 00`.
        let text = match fine {
            Some(_) => format!("{text:<COARSE_WIDTH$}"),
            None => text,
        };
        let mut spans = value_spans(app, control, text, style);
        if let Some(id) = fine {
            let fine = setting::text(app, on_fine, || format!("{:02X}", track.fine(id)));
            spans.push(Span::styled(fine, setting::style(app, on_fine, true)));
        }
        if slice {
            let room = VALUE_WIDTH + 1;
            let width: usize = spans.iter().map(Span::width).sum();
            spans.push(Span::raw(" ".repeat(room.saturating_sub(width))));
            spans.push(Span::styled(
                app.slice_choice(track),
                Style::new().fg(theme::MAUVE),
            ));
        }
        Line::from(spans)
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let (value, centred) = bar_value(control, self.track)?;
        let live = self
            .app
            .live
            .and_then(|live| live_value(control, &live))
            .unwrap_or(value);
        Some(
            ValueBar::new(value, BAR_CELLS)
                .playing(live)
                .centred(centred),
        )
    }
}

/// A device's title controls, each in its own style: the machine and its type, the type being the
/// sample a sampler plays or the choice a synth's is on, or the filter's type and mode.
fn title_controls(app: &App, track: &Track, device: &DeviceSpec) -> Vec<Line<'static>> {
    let current = app.track_control();
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let text = setting::text(app, here, || {
                choice_name(*control, track).unwrap_or_else(|| value(*control, track, app.song()))
            });
            let style = setting::style(app, here, at_default(*control, track));
            Line::from(value_spans(app, *control, text, style))
        })
        .collect()
}

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
fn output_meter(app: &App) -> Vec<Line<'static>> {
    let blank = || Line::raw(" ".repeat(LARGE_PANEL_WIDTH));
    let mut lines = vec![blank()];
    for (side, held) in ["L", "R"].into_iter().zip(app.after_trim) {
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
/// the Output's meter.
fn panel(app: &App, track: &Track, device: &DeviceSpec) -> GraphicPanel {
    // The Output is the device that starts with `Trim`.
    if device.grid[0][0] == Some(Control::Trim) {
        return GraphicPanel::new().boxed(output_meter(app), LARGE_PANEL_WIDTH as u16, None);
    }
    let image = match device.title[0] {
        Some(Control::FilterType) => app
            .filter_image
            .map(|id| (id, FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT)),
        Some(Control::Machine) if track.machine == Machine::Sampler => app
            .waveform_image
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
    app: &App,
    track: &Track,
    device: &DeviceSpec,
    focused: bool,
) {
    let widget = Device::new(device.identity.clone())
        .controls(title_controls(app, track, device))
        .focused(focused);
    frame.render_widget(widget, area);
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let source = SoundCells { app, track };
    let grid = SettingsGrid::new(&source, rows, CellWidths::SOUND);
    frame.render_widget(grid, Device::settings_area(area));
    let panel = panel(app, track, device);
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        frame.render_widget(panel, at);
    }
}

/// A sound track's Sound page: its four devices, stacked down the page, which fill the rows
/// inside the keyboard's box. Where fewer fit, the page goes by whole devices to keep the cursor's
/// in view.
fn draw_rack(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = app
        .track_page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    let devices = Page::Sound.devices(track);
    for (at, device) in devices.iter().enumerate().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        draw_device(frame, area, app, track, device, at == cursor);
    }
}

/// A fixed track's Sound page columns: its two halves side by side.
fn sound_columns() -> Columns {
    Columns::new([SOUND_HALF, SOUND_HALF].concat())
}

/// A row of a fixed track's Sound page: a control on the left, and on the right perhaps one.
fn sound_row(app: &App, track: &Track, controls: &[Control], columns: &Columns) -> Row<'static> {
    let mut cells = columns.row();
    sound_cells(app, track, controls, &mut cells);
    cells.done()
}

/// A fixed track's Sound page row's controls, put in their halves.
fn sound_cells(app: &App, track: &Track, controls: &[Control], cells: &mut Cells<'_>) {
    let current = app.track_control();
    for control in controls {
        // A group's output is on the right, with its tracks on the left. A return or the master
        // has only output, so it's all on the left.
        let half = usize::from(control.column() > 0 && track.is_group());
        cells.to(half * SOUND_HALF.len());
        cells.cell(name_line(
            app,
            control.label_on(track),
            &jump_label(app, *control),
            SOUND_LABEL,
        ));
        let here = *control == current;
        let on_fine = here && app.track_page.fine;
        let on_value = here && !on_fine;
        let text = setting::text(app, on_value, || value(*control, track, app.song()));
        let style = value_style(app, track, *control, on_value);
        let value = Line::from(value_spans(app, *control, text, style));
        // A setting that sweeps shows its fine part, set back.
        let fine = control.fine_on(track).map(|id| {
            let fine = setting::text(app, on_fine, || format!("{:02X}", track.fine(id)));
            Line::styled(fine, setting::style(app, on_fine, true))
        });
        let bar = bar_value(*control, track);
        if bar.is_none() && fine.is_none() {
            // A word, with all the room after the name.
            cells.span(value, 4);
            continue;
        }
        cells.cell(value).cell(fine.unwrap_or_default());
        if let Some((value, centred)) = bar {
            let live = app
                .live
                .and_then(|live| live_value(*control, &live))
                .unwrap_or(value);
            cells.cell(
                ValueBar::new(value, BAR_CELLS)
                    .playing(live)
                    .centred(centred),
            );
        }
    }
}

/// A group's Sound page: the tracks it sums down the left, each with its own mute or solo, and its
/// output on the right. A list longer than the page ends with how many more.
fn group_rows(app: &App, track: &Track, columns: &Columns, room: usize) -> Vec<Row<'static>> {
    let song = app.song();
    let members: Vec<usize> = song.group_members(app.cursor.track).collect();
    let controls = app.track_page.page_rows(Some(track));
    let lines = members.len().max(1).min(room).max(controls.len());
    let cut = members.len() > lines;
    let mut rows = Vec::new();
    for line in 0..lines {
        let mut cells = columns.row();
        let label = if line == 0 { "Tracks" } else { "" };
        cells.cell(Line::styled(label, theme::dim()));
        let member = members.get(line).copied();
        let text = match member {
            _ if cut && line == lines - 1 => {
                Line::styled(format!("+{} more", members.len() - line), theme::faint())
            }
            None if line == 0 => Line::styled("None", theme::dim()),
            None => Line::default(),
            Some(member) => member_line(app, member),
        };
        cells.span(text, 4);
        if let Some(controls) = controls.get(line) {
            sound_cells(app, track, controls, &mut cells);
        }
        rows.push(cells.done());
    }
    rows
}

/// One of a group's tracks, by its number and name, with its own mute or solo after it, dim while
/// it's muted.
fn member_line(app: &App, member: usize) -> Line<'static> {
    let song = app.song();
    let name = format!("{:02} {}", member + 1, song.tracks[member].name);
    let live = app.levels.get(member).copied().unwrap_or_default();
    let style = if live.mute {
        theme::dim()
    } else {
        Style::new()
    };
    let mut spans = vec![Span::styled(name.trim_end().to_owned(), style)];
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

/// Draw the Sound page in the area under the track page's title.
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    let mut rest = area;
    if track.is_sound() {
        draw_rack(frame, rest, app, track);
    } else if track.is_group() {
        let columns = sound_columns();
        let rows = group_rows(app, track, &columns, usize::from(rest.height));
        columns.stack(frame, &mut rest, rows);
    } else {
        let columns = sound_columns();
        let rows = app
            .track_page
            .page_rows(Some(track))
            .iter()
            .map(|controls| sound_row(app, track, controls, &columns))
            .collect();
        columns.stack(frame, &mut rest, rows);
    }
}
