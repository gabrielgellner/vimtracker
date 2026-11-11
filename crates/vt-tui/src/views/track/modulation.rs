//! The Mod page: a rack of half-width devices, each envelope on the left beside the LFO of its
//! number on the right, and the accumulator beside the arpeggio under them. An envelope's or an
//! LFO's title has its switch, then its destination, and a panel on its right draws an envelope's
//! shape or one cycle of an LFO, which the terminal places a dot on where the LFO is now. One
//! switched out is faint but for the cursor, as a bypassed FX slot is.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use vt_model::lfo::LFOS_PER_TRACK;
use vt_model::song::Track;

use crate::app::App;
use crate::theme;
use crate::track_page::{
    Control, DeviceSpec, DeviceWidth, EnvelopeField, LfoField, Page, modulator_bypassed,
    rack_places,
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

/// Which envelope the cursor is on, if any.
pub(crate) fn envelope_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Envelope(slot, _) => Some(slot),
        _ => None,
    }
}

/// Which LFO the cursor is on, if any.
pub(crate) fn lfo_being_edited(app: &App) -> Option<usize> {
    match app.track_control() {
        Control::Lfo(slot, _) => Some(slot),
        _ => None,
    }
}

/// Where each LFO's cycle is on screen, by the character at its top left, for the terminal to
/// place its dot over: none for one not drawn.
pub type CyclesAt = [Option<(u16, u16)>; LFOS_PER_TRACK];

/// What the Mod page's cells show for a modulator's settings.
struct ModCells<'a> {
    app: &'a App,
    track: &'a Track,
}

impl CellSource for ModCells<'_> {
    type Id = Control;

    fn name(&self, control: Control, room: u16) -> Line<'static> {
        let labels = jump_label(self.app, control);
        name_line(self.app, control.label(), &labels, room)
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

    /// A modulator's settings have no bars: the panel beside them shows what they shape.
    fn bar(&self, _control: Control) -> Option<ValueBar> {
        None
    }
}

/// Whether a control is a destination that's set to nothing, `---`, which dims it as an empty
/// slot's `Empty` is dim.
fn goes_nowhere(control: Control, track: &Track) -> bool {
    match control {
        Control::Envelope(slot, EnvelopeField::Destination) => {
            track.envelopes[slot].destination.is_none()
        }
        Control::Lfo(slot, LfoField::Destination) => track.lfos[slot].destination.is_none(),
        _ => false,
    }
}

/// A modulator's title controls, each in its own style: its destination.
fn title_controls(app: &App, track: &Track, device: &DeviceSpec) -> Vec<Line<'static>> {
    let current = app.track_control();
    device
        .title
        .iter()
        .flatten()
        .map(|control| {
            let here = *control == current;
            let text = setting::text(app, here, || value(*control, track, app.song()));
            let style = setting::style(app, here, goes_nowhere(*control, track));
            Line::from(value_spans(app, *control, text, style))
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

/// One of the Mod page's devices: its frame and title, with an envelope's or an LFO's switch, its
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
    let (panel, lfo) = match device.title[0] {
        Some(Control::Envelope(slot, _)) => {
            (panel(app.envelope_images.map(|ids| ids[slot])), false)
        }
        Some(Control::Lfo(slot, _)) => (panel(app.lfo_images.map(|ids| ids[slot])), true),
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
        frame.render_widget(panel, at);
    }
    let widths = match device.grid[0][0] {
        Some(Control::Arpeggio(_)) => ARPEGGIO_CELLS,
        _ => CellWidths::COMPACT,
    };
    let rows = device.grid.iter().copied().map(Vec::from).collect();
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
        if let Some(Control::Lfo(slot, _)) = device.title[0] {
            cycles[slot] = cycle_at;
        }
    }
    cycles
}
