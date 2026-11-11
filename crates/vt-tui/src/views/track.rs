//! The track page: what a track holds apart from its steps, grouped by theme.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::lfo::depth_text;
use vt_model::settings::{SettingId, gate_percent};
use vt_model::song::{Song, SynthVoice, Track, TrackKind, reset_name};
use vt_model::speed::Speed;

use crate::app::App;
use crate::banking::sample_text;
use crate::glyphs;
use crate::input::mode::Mode;
use crate::theme;
use crate::track_page::{
    AccumulatorField, ArpField, Control, DeviceSpec, EnvelopeField, LfoField, Page,
};
use crate::views::phrase::level_glyph;
use crate::views::widgets;
use crate::views::widgets::bar::ValueBar;
use crate::views::widgets::device::{DEVICE_HEIGHT, Device};
use crate::views::widgets::grid::{CellSource, CellWidths, ROW_STEP, SettingsGrid, faded};
use crate::views::widgets::group;
use crate::views::widgets::image;
use crate::views::widgets::meter::{hanging_cell, hanging_eighths};
use crate::views::widgets::panel::GraphicPanel;
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::take_top;
use crate::views::widgets::toggle;

mod modulation;
mod sound;

pub use modulation::CyclesAt;
pub(crate) use modulation::{
    MOD_PANEL_HEIGHT, MOD_PANEL_WIDTH, envelope_being_edited, lfo_being_edited,
};

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

/// Whether a setting is left at its default, which matters less, so its value is dim: a track
/// following the project's groove shows what it plays, dimmed as it isn't its own.
fn at_default(control: Control, track: &Track) -> bool {
    control == Control::Groove && track.groove.is_none()
        || control == Control::GateLength && track.gate_length.is_none()
        || control == Control::Speed && track.speed == Speed::Sixteenth
        || control == Control::Reset && track.reset.is_none()
        || control == Control::Glide && track.glide == 0
        || control == Control::FilterMode && !track.filter.has_modes()
}

/// A setting's value as text.
pub(crate) fn value(control: Control, track: &Track, song: &Song) -> String {
    let hex = |value: u8| format!("{value:02X}");
    match control {
        Control::Machine => track.machine.name().to_owned(),
        Control::Setting(_) => control
            .setting_on(track)
            .map_or_else(String::new, |id| hex(track.setting(id))),
        Control::Base(TrackControl::Cutoff) => hex(track.cutoff),
        Control::Base(TrackControl::Resonance) => hex(track.resonance),
        Control::Base(TrackControl::Pan) => hex(track.pan),
        Control::Base(TrackControl::SendA) => hex(track.send_a),
        Control::Base(TrackControl::SendB) => hex(track.send_b),
        Control::Base(TrackControl::Drive) => hex(track.drive),
        Control::Base(TrackControl::LowCut) => hex(track.low_cut),
        Control::Base(TrackControl::HighCut) => hex(track.high_cut),
        Control::Base(TrackControl::Pitch) => hex(track.pitch),
        Control::Group => track
            .group
            .map_or_else(|| "Off".to_owned(), |group| format!("G{group}")),
        Control::Choke => track
            .choke
            .map_or_else(|| "Off".to_owned(), |group| group.to_string()),
        Control::GateLength => format!("{}%", gate_percent(track.gate_length())),
        Control::Speed => track.speed.name().to_owned(),
        Control::Reset => reset_name(track.reset),
        // Off, `Acid` slides by its own `Slide`, as a 303 does.
        Control::Glide if track.glide == 0 && track.voice() == Some(SynthVoice::Acid) => {
            "Slide".to_owned()
        }
        Control::Glide => SettingId::GLIDE
            .entry()
            .kind
            .shown(u16::from(track.glide) << 8),
        Control::Sample => sample_text(song, track.sample()),
        Control::Mute | Control::Solo => String::new(),
        Control::Limiter => if song.limiter { "on" } else { "off" }.to_owned(),
        Control::FilterType => track.filter.name().to_owned(),
        // The MS-20's have none, so it doesn't apply.
        Control::FilterMode if !track.filter.has_modes() => "--".to_owned(),
        Control::FilterMode => track.filter.plays(track.filter_mode).name().to_owned(),
        Control::Level => hex(track.level),
        Control::Trim => hex(track.trim),
        Control::Groove => track.groove.unwrap_or(song.groove).name().to_owned(),
        Control::Envelope(slot, field) => {
            let envelope = &track.envelopes[slot];
            let ms = |value: u16| format!("{value} ms");
            match field {
                EnvelopeField::Destination => envelope
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name_on(track)),
                EnvelopeField::Attack => ms(envelope.attack_ms),
                EnvelopeField::Decay => ms(envelope.decay_ms),
                EnvelopeField::Sustain => format!("{}%", envelope.sustain),
                EnvelopeField::Release => ms(envelope.release_ms),
                // The first envelope on `Vol` shapes each note, rather than moving a value.
                EnvelopeField::Depth
                    if slot == 0 && envelope.destination == Some(SettingId::VOL) =>
                {
                    "--".to_owned()
                }
                EnvelopeField::Depth => depth_text(envelope.depth),
                EnvelopeField::Reset => if envelope.reset { "on" } else { "off" }.to_owned(),
            }
        }
        Control::Accumulator(field) => {
            let accumulation = track.accumulation;
            match field {
                AccumulatorField::Up => accumulation.up.to_string(),
                AccumulatorField::Down => accumulation.down.to_string(),
                AccumulatorField::Moves => accumulation.moves.name().to_owned(),
                AccumulatorField::AtLimit => accumulation.at_limit.name().to_owned(),
                AccumulatorField::Adds => accumulation.adds.name().to_owned(),
                AccumulatorField::Reset => accumulation.reset.name(),
            }
        }
        Control::Arpeggio(field) => {
            let arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => arpeggio.rate.name().to_owned(),
                ArpField::Mode => arpeggio.mode.name().to_owned(),
                ArpField::Octaves => arpeggio.octaves.to_string(),
                ArpField::Retrig => if arpeggio.retrig { "on" } else { "off" }.to_owned(),
            }
        }
        Control::Insert(slot) => track.inserts[slot].name().to_owned(),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            match insert.params().get(param) {
                Some(found) if !found.choices.is_empty() => found
                    .choices
                    .get(usize::from(insert.value(param)))
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
                Some(_) => hex(insert.value(param)),
                None => String::new(),
            }
        }
        Control::Lfo(slot, field) => {
            let lfo = &track.lfos[slot];
            match field {
                LfoField::Destination => lfo
                    .destination
                    .map_or_else(|| "---".to_owned(), |d| d.name_on(track)),
                LfoField::Shape => lfo.shape.name().to_owned(),
                LfoField::Rate => lfo.rate.to_string(),
                LfoField::Depth => depth_text(lfo.depth),
                LfoField::Phase => format!("{}°", lfo.phase),
                LfoField::Retrig => if lfo.retrig { "on" } else { "off" }.to_owned(),
            }
        }
    }
}

/// Whether a setting that's off or on, and nothing else, is on, or nothing for any other: an
/// effect's `Off`/`On` list, an envelope's `Reset`, an LFO's or the arpeggio's `Retrig`, and the
/// master's `Limiter`, which is the song's. They show as a toggle, and the status line still says
/// the word.
pub(crate) fn toggle_on(control: Control, track: &Track, song: &Song) -> Option<bool> {
    match control {
        Control::Limiter => Some(song.limiter),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            found.is_toggle().then(|| insert.value(param) != 0)
        }
        Control::Envelope(slot, EnvelopeField::Reset) => Some(track.envelopes[slot].reset),
        Control::Lfo(slot, LfoField::Retrig) => Some(track.lfos[slot].retrig),
        Control::Arpeggio(ArpField::Retrig) => Some(track.arpeggio.retrig),
        _ => None,
    }
}

/// A setting's value as a page shows it, with a jump's label over it: what's being typed in its
/// place, a toggle in its own colour if it's off or on, or its value in the cursor's colour under
/// the cursor and dim at its default.
fn value_line(
    app: &App,
    track: &Track,
    control: Control,
    here: bool,
    default: bool,
) -> Line<'static> {
    let typing = here && app.mode == Mode::Insert;
    let (text, style) = match toggle_on(control, track, app.song()) {
        Some(on) if !typing => {
            let style = if here {
                theme::cursor(app.mode)
            } else {
                toggle::style(on)
            };
            (toggle::text(on), style)
        }
        _ => (
            setting::text(app, here, || value(control, track, app.song())),
            setting::style(app, here, default),
        ),
    };
    Line::from(value_spans(app, control, text, style))
}

/// Whether the cursor is in a sound track's Machine device, on the machine, its type, its
/// sample or one of its settings, so its thumbnail is the focused one.
pub(crate) fn machine_being_edited(app: &App) -> bool {
    app.track_page.page == Page::Sound
        && matches!(
            app.track_control(),
            Control::Machine | Control::Sample | Control::Setting(_)
        )
}

/// Whether the cursor is on something the filter's curve shows.
pub(crate) fn filter_being_edited(app: &App) -> bool {
    matches!(
        app.track_control(),
        Control::FilterType
            | Control::FilterMode
            | Control::Base(
                TrackControl::Cutoff
                    | TrackControl::Resonance
                    | TrackControl::Drive
                    | TrackControl::LowCut
                    | TrackControl::HighCut
            )
    )
}

/// Which FX slot the cursor is on, if any.
pub(crate) fn slot_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Insert(slot) | Control::InsertParam(slot, _) => Some(slot),
        _ => None,
    }
}

/// Whether a slot is switched out as it's heard: while the song plays, a step's lock of its bypass
/// counts, so the slot fades and comes back with the lock, and otherwise it's the slot's own.
pub(crate) fn heard_bypassed(app: &App, track: &Track, slot: usize) -> bool {
    match app.live {
        Some(live) if app.transport == Transport::Playing => live.bypassed[slot],
        _ => track.bypassed[slot],
    }
}

/// What the FX page's cells show for a slot's settings, read from what's in the slot.
struct SlotCells<'a> {
    app: &'a App,
    track: &'a Track,
}

impl CellSource for SlotCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        name_line(self.app, control.name_on(self.track), &labels, room)
    }

    fn value(&self, control: Control) -> Line<'static> {
        let here = control == self.app.track_control();
        value_line(self.app, self.track, control, here, false)
    }

    fn bar(&self, control: Control) -> Option<ValueBar> {
        let Control::InsertParam(slot, param) = control else {
            return None;
        };
        let insert = self.track.inserts[slot];
        if !insert.params().get(param)?.choices.is_empty() {
            return None;
        }
        let set = insert.value(param);
        let live = self.app.live.map_or(set, |live| live.inserts[slot][param]);
        Some(ValueBar::new(set, BAR_CELLS).playing(live))
    }
}

/// A device's title controls, each in its own style: on a slot, its effect, `Empty` when there's
/// none, and its type if it has one.
fn title_controls(app: &App, track: &Track, device: &DeviceSpec) -> Vec<Line<'static>> {
    let current = app.track_control();
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let empty =
                matches!(*control, Control::Insert(slot) if track.inserts[slot] == Insert::Empty);
            let text = setting::text(app, here, || match empty {
                true => EMPTY_SLOT.to_owned(),
                false => value(*control, track, app.song()),
            });
            let style = setting::style(app, here, empty);
            Line::from(value_spans(app, *control, text, style))
        })
        .collect()
}

/// A slot as a device: its frame and its title, with its bypass's toggle, then its settings in its
/// effect's grid, or a hint if it's empty, and its graphic panel. A bypassed slot's contents are
/// faint but for the cursor and its images.
fn draw_slot(frame: &mut Frame, area: Rect, app: &App, track: &Track, device: &DeviceSpec) {
    let Some(Control::Insert(slot)) = device.title[0] else {
        return;
    };
    let cursor = theme::cursor(app.mode);
    let fade = heard_bypassed(app, track, slot).then_some(cursor);
    let mut controls = title_controls(app, track, device);
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
        .focused(slot_being_edited(app) == Some(slot));
    frame.render_widget(frame_widget, area);
    let settings = Device::settings_area(area);
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
    let rows = device.grid.iter().copied().map(Vec::from).collect();
    let source = SlotCells { app, track };
    let grid = SettingsGrid::new(&source, rows, CellWidths::FX).faded(fade);
    frame.render_widget(grid, settings);
    let panel = slot_panel(app, track, slot);
    if !panel.is_empty() {
        let at = Device::panel_area(area, panel.width());
        frame.render_widget(panel, at);
    }
}

/// The FX page: a device for each slot, stacked down the page. Each is as tall whatever it holds,
/// so four fill the rows inside the keyboard's box. Where fewer fit, the page goes by whole devices
/// to keep the cursor's in view.
fn draw_fx(frame: &mut Frame, area: Rect, app: &App, track: &Track) {
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let cursor = app
        .track_page
        .device_cursor(Some(track))
        .map_or(0, |cursor| cursor.device);
    let first = (cursor + 1).saturating_sub(fit);
    let mut rest = area;
    for device in Page::Fx.devices(track).iter().skip(first).take(fit) {
        let area = take_top(&mut rest, DEVICE_HEIGHT);
        draw_slot(frame, area, app, track, device);
    }
}

/// Where a gain reduction meter's held mark goes for the deepest it has turned down lately, if it
/// has: the row that depth ends in, and a faint line at that row's top or bottom, whichever is
/// nearer, so it's within a quarter of a row, 1 dB.
fn held_mark(held: f32) -> Option<(usize, char)> {
    let rows = SLOT_DRAWING_HEIGHT;
    let eighths = ((held / REDUCTION_METER_DB).clamp(0.0, 1.0) * (rows * 8) as f32).round();
    let eighths = eighths as usize;
    let row = eighths.checked_sub(1)? / 8;
    let into = eighths - row * 8;
    Some((row, if into <= 4 { '▔' } else { '▁' }))
}

/// One row of a gain reduction meter, which hangs down from the top of its rows as it turns down
/// more, as the main meter's does, with a faint mark where it went deepest lately, while it isn't
/// as deep now. An empty one that holds nothing shows a faint mark at the top, so it's seen to be
/// there.
fn reduction_cell(db: f32, held: f32, row: usize, color: Color) -> Span<'static> {
    let filled = hanging_eighths(db / REDUCTION_METER_DB, row, SLOT_DRAWING_HEIGHT);
    if filled == 0 {
        let mark = match held_mark(held) {
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
fn slot_panel(app: &App, track: &Track, slot: usize) -> GraphicPanel {
    let style = if slot_being_edited(app) == Some(slot) {
        theme::active()
    } else {
        theme::dim()
    };
    let color = style.fg.unwrap_or(theme::TEXT);
    let bypassed = heard_bypassed(app, track, slot);
    let reduction = app
        .live
        .filter(|_| !bypassed)
        .map_or(0.0, |live| live.reduction[slot]);
    let held = if bypassed {
        0.0
    } else {
        app.slot_reduction[slot].top
    };
    let cursor = theme::cursor(app.mode);
    let fade = |line: Line<'static>| if bypassed { faded(line, cursor) } else { line };
    let meter = || {
        let lines = (0..SLOT_DRAWING_HEIGHT)
            .map(|row| fade(Line::from(reduction_cell(reduction, held, row, color))))
            .collect();
        (lines, REDUCTION_METER_WIDTH as u16, Some(REDUCTION_CAPTION))
    };
    let curve = |width: usize| {
        app.slot_images[slot].map(|id| {
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

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    draw_finding_cycles(frame, area, app);
}

/// Draw the view, giving back where the Mod page's LFO cycles are, for their dots.
pub fn draw_finding_cycles(frame: &mut Frame, area: Rect, app: &App) -> CyclesAt {
    // While the keyboard plays the sound, the page sits in a box of that mode's colour.
    let area = if app.mode == Mode::Keys {
        let block = widgets::window_border()
            .border_style(Style::new().fg(theme::YELLOW))
            .title(" Keys ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else {
        area
    };
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        frame.render_widget(Paragraph::new("This track doesn't exist."), area);
        return CyclesAt::default();
    };
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let tabs = Page::ALL.map(|page| {
        let text = format!(" {} ", page.name());
        if page == app.track_page.page {
            Span::styled(text, bold)
        } else {
            Span::styled(text, theme::dim())
        }
    });
    let name = if track.is_sound() {
        let name = format!("Track {:02} {}", app.cursor.track + 1, track.name);
        format!("{} ", name.trim_end())
    } else {
        format!("{} ", track.kind.title())
    };
    // The track's machine or kind as a glyph before it, a group's in its colour, and whether it's
    // muted or soloed after.
    let icon = match track.kind {
        TrackKind::Group(_) => group::style(app.song(), app.cursor.track).unwrap_or_default(),
        _ => theme::dim(),
    };
    let mut title = vec![
        Span::styled(format!("{} ", glyphs::track(track)), icon),
        Span::styled(name, bold),
    ];
    // A track in a group carries its mark, in the group's colour, as on the mixer.
    if let Some(mark) = group::mark(app.song(), app.cursor.track) {
        title.extend([mark.patch_style(bold), Span::raw(" ")]);
    }
    if let Some((glyph, style)) = level_glyph(app, app.cursor.track) {
        title.push(Span::styled(format!("{glyph} "), style));
    }
    title.extend(tabs);
    let mut rest = area;
    frame.render_widget(Paragraph::new(Line::from(title)), take_top(&mut rest, 2));
    match app.track_page.page {
        Page::Sound => sound::draw(frame, rest, app, track),
        Page::Fx => draw_fx(frame, rest, app, track),
        Page::Mod => return modulation::draw(frame, rest, app, track),
    }
    CyclesAt::default()
}
