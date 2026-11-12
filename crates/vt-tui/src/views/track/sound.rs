//! The Sound page. A sound track's is a rack of four devices, as the FX page's slots are: its
//! machine, with the machine and its type in the title; its filter, with the filter's type and
//! mode in the title and its curve beside its settings; its output; and how it plays its notes.
//! A group's, a return's and the master's is the same Output, with holes where they lack a
//! setting, and under a group's the tracks it sums, and under a return's the tracks that send to
//! it (the track pages study, `docs/designs/track-pages.md`, §2.3).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use vt_engine::track_state::LiveValues;
use vt_model::action::TrackControl;
use vt_model::grid::GRID_ROWS;
use vt_model::settings::{Kind, SettingId};
use vt_model::song::{Machine, Track, TrackKind};

use crate::app::App;
use crate::automating::{as_heard, fader_of};
use crate::glyphs;
use crate::theme;
use crate::track_page::{Control, DEVICE_COLUMNS, DeviceSpec, Page};
use crate::views::widgets::bar::{ValueBar, moved_since};
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellSource, CellWidths, ROW_STEP, SettingsGrid};
use crate::views::widgets::group;
use crate::views::widgets::image;
use crate::views::widgets::meter::level_across;
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::take_top;

use super::{
    BAR_CELLS, FILTER_CURVE_HEIGHT, FILTER_CURVE_WIDTH, LARGE_PANEL_WIDTH, WAVEFORM_HEIGHT,
    WAVEFORM_WIDTH, at_default, value, value_line,
};

/// How wide a value is on the Sound page, its coarse digits and its fine ones, as `A000`, and its
/// coarse digits alone.
const VALUE_WIDTH: usize = 4;
const COARSE_WIDTH: usize = 2;

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
        Control::Base(TrackControl::VelocityDelta) => Some((track.velocity_delta, true)),
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
        Control::Base(TrackControl::Punch) => level(track.punch),
        Control::Base(TrackControl::LowCut) => level(track.low_cut),
        Control::Base(TrackControl::HighCut) => level(track.high_cut),
        _ => None,
    }
}

/// A setting's value's style on the Sound page: as any page's, but the group a track is in is in
/// its colour, as on the mixer, and a fader the steps drive is in the playing colour.
fn value_style(app: &App, track: &Track, control: Control, here: bool) -> Style {
    let style = setting::style(app, here, at_default(control, track));
    match group::style(app.song(), app.cursor.track) {
        Some(tint) if control == Control::Group && !here => tint,
        _ if !here && app.follows_steps(app.cursor.track, control) => theme::automated(),
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

    /// Its name, with a fader a hand has taken over from the steps marked after it.
    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        let mut line = name_line(self.app, control.label_on(self.track), &labels, room);
        if self.app.taken_over(self.app.cursor.track, control) {
            line.push_span(Span::raw(" "));
            line.push_span(Span::styled(glyphs::TAKEN.to_string(), theme::taken()));
        }
        line
    }

    /// A value and its fine part, set back, as `A000`, with the cursor on either; a word, as
    /// `1/16` or the sampler's `One-shot`; or a slice's number with its name after it where a bar
    /// would be, as `00   whole`.
    fn value(&self, control: Control) -> Line<'static> {
        let (app, track) = (self.app, self.track);
        let here = control == app.track_control();
        // The master's `Limiter` and the filter's `Safe` are on or off, so they're toggles.
        if matches!(control, Control::Limiter | Control::Safe) {
            return value_line(app, track, control, here, at_default(control, track));
        }
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

    /// A bar filled to what's heard, with a mark where it's set. A fader's is the other way round:
    /// filled to the fader, which `VOL` and modulation don't move, with a thin mark at what's heard
    /// (decision 174).
    fn bar(&self, control: Control) -> Option<ValueBar> {
        let (value, centred) = bar_value(control, self.track)?;
        let then = self
            .app
            .reported_track()
            .and_then(|track| bar_value(control, track));
        let live = match (
            self.app.live.and_then(|live| live_value(control, &live)),
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
        Some(bar.centred(centred))
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
/// the Output's meter. The master's Output has none, as the main meter beside it already shows
/// what it plays (the study's §1.5, rule 5).
fn panel(app: &App, track: &Track, device: &DeviceSpec) -> GraphicPanel {
    // The Output is the device that starts with `Trim`.
    if device.grid[0][0] == Some(Control::Trim) {
        if track.kind == TrackKind::Master {
            return GraphicPanel::new();
        }
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
    // Its faders show where the steps have them while they drive them.
    let faders = app.faders(app.cursor.track);
    let heard = as_heard(track, &faders);
    let source = SoundCells { app, track: &heard };
    let grid = SettingsGrid::new(&source, rows, CellWidths::SOUND);
    frame.render_widget(grid, Device::settings_area(area));
    let panel = panel(app, track, device);
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        frame.render_widget(panel, at);
    }
}

/// The Sound page's devices, stacked down the page: a sound track's four, which fill the rows
/// inside the keyboard's box, or a fixed track's Output. Where fewer fit, the page goes by whole
/// devices to keep the cursor's in view.
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
fn feed_name(app: &App, track: usize) -> Span<'static> {
    let song = app.song();
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
    app: &'a App,
    /// Which of the sends goes to this return.
    send: SettingId,
}

impl SendsIn<'_> {
    /// How much a track sends here, its coarse and fine parts.
    fn amount(&self, track: usize) -> (u8, u8) {
        let this = &self.app.song().tracks[track];
        (this.coarse(self.send).unwrap_or(0), this.fine(self.send))
    }
}

impl CellSource for SendsIn<'_> {
    type Id = Feed;

    /// The track, cut short of the value after it.
    fn name(&self, feed: Feed, room: u16) -> Line<'static> {
        let fit = |span: Span<'static>| {
            let text: String = span.content.chars().take(usize::from(room) - 1).collect();
            Line::from(Span::styled(text, span.style))
        };
        match feed {
            Feed::Track(track) => fit(feed_name(self.app, track)),
            Feed::More(more) => Line::styled(format!("+{more} more"), theme::faint()),
            Feed::Nothing => Line::styled("None", theme::dim()),
        }
    }

    /// Its send, as the Output shows one, `0C00`, with the fine part dim.
    fn value(&self, feed: Feed) -> Line<'static> {
        let Feed::Track(track) = feed else {
            return Line::default();
        };
        let (coarse, fine) = self.amount(track);
        Line::from(vec![
            Span::styled(
                format!("{coarse:02X}"),
                setting::style(self.app, false, false),
            ),
            Span::styled(format!("{fine:02X}"), setting::style(self.app, false, true)),
        ])
    }

    fn bar(&self, feed: Feed) -> Option<ValueBar> {
        let Feed::Track(track) = feed else {
            return None;
        };
        Some(ValueBar::new(self.amount(track).0, BAR_CELLS))
    }
}

/// A return's Sends in, under its Output: every track and group that sends to it, with how much,
/// as the Output's settings are laid out, so they're found here rather than strip by strip on the
/// mixer. It's worked out from the song each time it's drawn, and the cursor doesn't stop in it.
fn draw_sends_in(frame: &mut Frame, area: Rect, app: &App, send: SettingId) {
    frame.render_widget(Device::new("Sends in"), area);
    let source = SendsIn { app, send };
    let song = app.song();
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
fn draw_tracks(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Device::new("Tracks"), area);
    let members = app.song().group_members(app.cursor.track).collect();
    let rows = feed_rows(&feeds(members));
    let inside = Device::settings_area(area);
    let width = CellWidths::SOUND.column();
    for (row, places) in rows.iter().enumerate() {
        for (column, feed) in places.iter().enumerate() {
            let line = match feed {
                Some(Feed::Track(member)) => member_line(app, *member),
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
fn member_line(app: &App, member: usize) -> Line<'static> {
    let live = app.levels.get(member).copied().unwrap_or_default();
    let mut name = feed_name(app, member);
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
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    draw_rack(frame, area, app, track);
    let mut rest = area;
    take_top(&mut rest, DEVICE_HEIGHT);
    let below = take_top(&mut rest, DEVICE_HEIGHT);
    match track.kind {
        TrackKind::Group(_) => draw_tracks(frame, below, app),
        TrackKind::ReturnA => draw_sends_in(frame, below, app, SettingId::SEND_A),
        TrackKind::ReturnB => draw_sends_in(frame, below, app, SettingId::SEND_B),
        TrackKind::Sound | TrackKind::Master => {}
    }
}
