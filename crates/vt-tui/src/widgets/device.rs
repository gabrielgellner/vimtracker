//! A device: one thing in a page's rack, such as an FX slot, in a rounded frame 9 rows tall with
//! its title on the top border. The title says which device it is, dim, then has its switch, or a
//! place for one, then the controls that change its character, such as an effect and its type.
//! Inside are its settings grid on the left and its graphic panel, if it has one, on the right.
//! The device with the cursor has a brighter frame and its title's controls bold.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::widgets;
use crate::widgets::grid::GRID_HEIGHT;
use crate::widgets::theme;
use crate::widgets::toggle;

/// How tall every device is, whatever it holds, so nothing moves when what's in it changes: its
/// frame, a blank line, three rows of settings with a blank line after each, and its frame.
pub const DEVICE_HEIGHT: u16 = GRID_HEIGHT + 4;
/// The room between the frame and what's inside it, at each side.
const PADDING: u16 = 2;
/// Where the title starts along the top border, after the corner and a line.
const TITLE_INDENT: u16 = 2;
/// The room between the title's parts.
const TITLE_GAP: &str = "  ";
/// The room between two half-width devices side by side.
pub const HALF_GAP: u16 = 2;

/// What's in a title's place for its switch.
#[derive(Clone, Copy)]
enum Switch {
    /// A toggle, on or off.
    Toggle(bool),
    /// Nothing, but its room.
    Room,
}

/// A device's frame and title.
pub struct Device {
    identity: String,
    switch: Option<Switch>,
    controls: Vec<Line<'static>>,
    /// What comes after the controls, a space on, as a generator's `· Note 00–0F`.
    after: Option<Line<'static>>,
    focused: bool,
}

impl Device {
    /// A device named by this, such as a slot's digit.
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
            switch: None,
            controls: Vec::new(),
            after: None,
            focused: false,
        }
    }

    /// The same with its switch after its name, on or off.
    #[must_use]
    pub fn switch(self, on: bool) -> Self {
        Self {
            switch: Some(Switch::Toggle(on)),
            ..self
        }
    }

    /// The same with the room for a switch after its name, as one that has nothing to switch
    /// leaves, so what's after it is where it is on the others.
    #[must_use]
    pub fn switch_room(self) -> Self {
        Self {
            switch: Some(Switch::Room),
            ..self
        }
    }

    /// The same with these controls in its title, each already in its own style.
    #[must_use]
    pub fn controls(self, controls: Vec<Line<'static>>) -> Self {
        Self { controls, ..self }
    }

    /// The same with this after its controls, a space on, in its own style: where a generator
    /// acts, which isn't one of its controls.
    #[must_use]
    pub fn after(self, after: Line<'static>) -> Self {
        Self {
            after: Some(after),
            ..self
        }
    }

    /// The same with the cursor in it, or not.
    #[must_use]
    pub fn focused(self, focused: bool) -> Self {
        Self { focused, ..self }
    }

    /// Where a device's settings go in its area: in from its frame, after a blank line.
    pub fn settings_area(area: Rect) -> Rect {
        let inner = inner(area);
        let x = inner.x + PADDING;
        Rect::new(
            x,
            inner.y + 1,
            inner.right().saturating_sub(x),
            GRID_HEIGHT.min(inner.height.saturating_sub(1)),
        )
    }

    /// Where a graphic panel this wide goes in a device's area: against its right side, in from
    /// its frame, as tall as all inside it.
    pub fn panel_area(area: Rect, width: u16) -> Rect {
        let inner = inner(area);
        let right = inner.right().saturating_sub(PADDING);
        let width = width.min(right.saturating_sub(inner.x));
        Rect::new(right - width, inner.y, width, inner.height)
    }
}

/// What's inside a device's frame.
fn inner(area: Rect) -> Rect {
    widgets::window_border().inner(area)
}

/// Where each device of a rack goes in an area, from its band, top to bottom, and its side: none
/// for the whole width, or 0 or 1 for the left or the right half, with a gap between. Each band is
/// a device tall. Where fewer bands fit than there are, the rack goes by whole bands to keep the
/// band `keep` in view, as low as it can be, and a device out of view has no area.
pub fn rack_areas(area: Rect, places: &[(usize, Option<usize>)], keep: usize) -> Vec<Option<Rect>> {
    let fit = usize::from(area.height / DEVICE_HEIGHT).max(1);
    let first = (keep + 1).saturating_sub(fit);
    let half = area.width.saturating_sub(HALF_GAP) / 2;
    places
        .iter()
        .map(|(band, side)| {
            let shown = band.checked_sub(first).filter(|shown| *shown < fit)?;
            let y = area.y + shown as u16 * DEVICE_HEIGHT;
            let height = DEVICE_HEIGHT.min(area.bottom().saturating_sub(y));
            Some(match side {
                None => Rect::new(area.x, y, area.width, height),
                Some(0) => Rect::new(area.x, y, half, height),
                Some(_) => {
                    let x = area.x + half + HALF_GAP;
                    Rect::new(x, y, area.right().saturating_sub(x), height)
                }
            })
        })
        .collect()
}

impl Widget for Device {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frame = if self.focused {
            theme::OVERLAY2
        } else {
            theme::SURFACE1
        };
        widgets::window_border()
            .border_style(Style::new().fg(frame))
            .render(area, buf);
        // A device with nothing to name it, as a generator, starts with its controls.
        let named = !self.identity.is_empty();
        let mut title = vec![Span::raw(" "), Span::styled(self.identity, theme::dim())];
        let mut gap = if named { TITLE_GAP } else { "" };
        // A switch is three cells, its glyph and the two spaces its picture covers, the last of
        // which is the first column of the gap after it, and the room for one is as wide.
        let switch = match self.switch {
            Some(Switch::Toggle(on)) => Some(toggle::span(on)),
            Some(Switch::Room) => Some(Span::raw(" ".repeat(toggle::WIDTH))),
            None => None,
        };
        if let Some(switch) = switch {
            title.push(Span::raw(TITLE_GAP));
            title.push(switch);
            gap = &TITLE_GAP[1..];
        }
        for control in self.controls {
            title.push(Span::raw(std::mem::replace(&mut gap, TITLE_GAP)));
            title.extend(control.spans.into_iter().map(|span| {
                if self.focused {
                    span.patch_style(Modifier::BOLD)
                } else {
                    span
                }
            }));
        }
        if let Some(after) = self.after {
            title.push(Span::raw(" "));
            title.extend(after.spans);
        }
        title.push(Span::raw(" "));
        let left = area.x + TITLE_INDENT;
        // The title stops short of the far corner and the line before it.
        let room = area.right().saturating_sub(left + TITLE_INDENT);
        // In the text's colour, not the frame's under it, and only as wide as it is, so the frame
        // after it keeps its own.
        let title = Line::from(title).style(Style::new().fg(theme::TEXT));
        let width = (title.width() as u16).min(room);
        if width > 0 && area.height > 0 {
            title.render(Rect::new(left, area.y, width, 1), buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::mode::Mode;
    use crate::widgets::pin::{assert_drawn, drawn};

    /// A slot's controls: its effect under the cursor, and its type, dim.
    fn controls() -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled("Comp", theme::cursor(Mode::Normal))),
            Line::from(Span::styled("Glue", theme::dim())),
        ]
    }

    #[test]
    fn a_device_with_the_cursor_has_a_brighter_frame_and_its_controls_bold() {
        let device = Device::new("1")
            .switch(true)
            .controls(controls())
            .focused(true);
        assert_drawn(
            &drawn(device, 30, 3),
            &[
                "╭─ 1  \u{f205}   Comp  Glue ────────╮",
                "aababbcccbddddbbeeeebaaaaaaaaa",
                "│                            │",
                "a............................a",
                "╰────────────────────────────╯",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "a fg OVERLAY2",
                "b fg TEXT",
                "c fg GREEN",
                "d fg BASE bg ROSEWATER +BOLD",
                "e fg OVERLAY2 +BOLD",
            ],
        );
    }

    #[test]
    fn a_device_without_it_has_a_faint_frame_and_its_switch_off() {
        let device = Device::new("1").switch(false).controls(controls());
        assert_drawn(
            &drawn(device, 30, 3),
            &[
                "╭─ 1  \u{f204}   Comp  Glue ────────╮",
                "aabcbbdddbeeeebbccccbaaaaaaaaa",
                "│                            │",
                "a............................a",
                "╰────────────────────────────╯",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "a fg SURFACE1",
                "b fg TEXT",
                "c fg OVERLAY2",
                "d fg OVERLAY0",
                "e fg BASE bg ROSEWATER",
            ],
        );
    }

    #[test]
    fn room_for_a_switch_keeps_the_controls_where_they_are() {
        let device = Device::new("2").switch_room().controls(controls());
        assert_drawn(
            &drawn(device, 30, 3),
            &[
                "╭─ 2      Comp  Glue ────────╮",
                "aabcbbbbbbddddbbccccbaaaaaaaaa",
                "│                            │",
                "a............................a",
                "╰────────────────────────────╯",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "a fg SURFACE1",
                "b fg TEXT",
                "c fg OVERLAY2",
                "d fg BASE bg ROSEWATER",
            ],
        );
    }

    #[test]
    fn a_device_with_no_name_starts_with_its_controls_and_what_comes_after() {
        let device = Device::new("")
            .controls(vec![Line::raw("Euclid")])
            .after(Line::from(Span::styled("· Note 00–0F", theme::dim())));
        assert_drawn(
            &drawn(device, 30, 3),
            &[
                "╭─ Euclid · Note 00–0F ──────╮",
                "aabbbbbbbbccccccccccccbaaaaaaa",
                "│                            │",
                "a............................a",
                "╰────────────────────────────╯",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "a fg SURFACE1",
                "b fg TEXT",
                "c fg OVERLAY2",
            ],
        );
    }

    #[test]
    fn a_title_stops_short_of_the_far_corner() {
        let device = Device::new("1").switch(true).controls(controls());
        assert_drawn(
            &drawn(device, 14, 3),
            &[
                "╭─ 1  \u{f205}   Co─╮",
                "aabcbbdddbeeaa",
                "│            │",
                "a............a",
                "╰────────────╯",
                "aaaaaaaaaaaaaa",
                "a fg SURFACE1",
                "b fg TEXT",
                "c fg OVERLAY2",
                "d fg GREEN",
                "e fg BASE bg ROSEWATER",
            ],
        );
    }

    #[test]
    fn its_settings_go_in_from_its_frame_after_a_blank_line_and_its_panel_against_its_right() {
        let area = Rect::new(10, 4, 80, DEVICE_HEIGHT);
        assert_eq!(DEVICE_HEIGHT, 9);
        assert_eq!(Device::settings_area(area), Rect::new(13, 6, 76, 5));
        assert_eq!(Device::panel_area(area, 20), Rect::new(67, 5, 20, 7));
        assert_eq!(
            Device::panel_area(Rect::new(0, 0, 10, 9), 20),
            Rect::new(1, 1, 6, 7),
            "no wider than the inside"
        );
    }

    #[test]
    fn a_rack_puts_bands_a_device_tall_and_halves_side_by_side_and_keeps_a_band_in_view() {
        let area = Rect::new(0, 2, 100, 2 * DEVICE_HEIGHT + 3);
        let places = [(0, None), (1, Some(0)), (1, Some(1)), (2, None)];
        assert_eq!(
            rack_areas(area, &places, 0),
            [
                Some(Rect::new(0, 2, 100, 9)),
                Some(Rect::new(0, 11, 49, 9)),
                Some(Rect::new(51, 11, 49, 9)),
                None,
            ]
        );
        assert_eq!(
            rack_areas(area, &places, 2),
            [
                None,
                Some(Rect::new(0, 2, 49, 9)),
                Some(Rect::new(51, 2, 49, 9)),
                Some(Rect::new(0, 11, 100, 9)),
            ]
        );
    }
}
