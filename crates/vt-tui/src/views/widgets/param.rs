//! A setting's cell, as the track page's grids and devices' titles draw one: its name, its value
//! and its bar, from what the setting reads as, how it is now and how the modes show it (the UI
//! plan's §3.2, #739). The cell doesn't format the setting's value: the page passes the text in,
//! so the widget reads nothing of the track or the program. What it does is lay the parts out the
//! one way every page has them: a jump's labels and a taken-over mark on the name, what's typed in
//! the value's place, the cursor's colour and the value's tone, a fine part set back, a word after
//! the value in the bar's room, and a bar filled to what's heard with a mark where it's set.

use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::glyphs;
use crate::theme;
use crate::views::widgets::bar::ValueBar;
use crate::views::widgets::grid::{CellParts, CellWidths};
use crate::views::widgets::setting::{self, CellLook, Focus, name_line, value_spans};
use crate::views::widgets::toggle;

/// How wide a value's coarse digits are, before its fine ones, so what's typed in their place
/// keeps the fine digits where they are.
const COARSE_WIDTH: usize = 2;

/// How a setting's value reads in its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Hex digits, as `80` or a macro's `8000`, with a bar.
    Hex,
    /// Two coarse hex digits and the fine ones set back after them, dim, as `A000`.
    HexFine { fine: u8 },
    /// A word or a choice, as `1/16` or `One-shot`, across the bar's room.
    Word,
    /// Off or on, as a toggle in its own colour.
    Toggle { on: bool },
    /// A number in its units, as `120 ms` or a generator's `9`, which has a bar where its bounds
    /// give one.
    Number,
}

/// How a value is coloured while the cursor isn't on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Plain, or a toggle in its own colour.
    Plain,
    /// Left at its default, or doing nothing on this track: dim.
    Default,
    /// A fader the steps drive now, in the playing colour.
    Automated,
    /// A group's colour, or a role's.
    Tint(Style),
}

/// Where a setting is set along its bar, 0 to 255, and whether its bar is a mark either side of a
/// centre, as pan's is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub set: u8,
    pub centred: bool,
}

/// What the page knows of a setting now: worked out from the engine's report, the macros and the
/// faders by the page, never by the widget.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamState {
    /// What it plays with now, on the bar's 0 to 255, from [`heard`] (#421).
    pub heard: u8,
    /// Filled to the fader with a mark at what's heard, rather than the other way (decision 174).
    pub fader: bool,
    /// The range a macro holding it can take it over, which its bar draws behind the fill.
    pub macro_range: Option<(u8, u8)>,
    /// How its value is coloured while the cursor isn't on it.
    pub tone: Tone,
    /// A hand has taken this fader over from the steps.
    pub taken: bool,
    /// A word after the value, in the bar's room, and the room the value is padded to before it:
    /// a slice's name, or whose tune it is.
    pub after: Option<(Span<'static>, usize)>,
}

impl ParamState {
    /// A setting heard as it's set, held by no macro, in this tone.
    pub fn new(tone: Tone) -> Self {
        Self {
            heard: 0,
            fader: false,
            macro_range: None,
            tone,
            taken: false,
            after: None,
        }
    }
}

/// One setting's cell: its name and how its value reads, where it sits on its bar if it has one,
/// how it is now, and how the modes show it.
#[derive(Debug, Clone)]
pub struct ParamCell<'a> {
    pub name: Cow<'a, str>,
    pub face: Face,
    /// Its value as text, as the page formats it. A toggle's is the word the status line says,
    /// which only shows while Insert types over it.
    pub text: String,
    /// Where it's set along its bar, or nothing for a value with no bar.
    pub place: Option<Place>,
    pub state: ParamState,
    pub look: CellLook<'a>,
}

impl ParamCell<'_> {
    /// Its name, value and bar, for a grid to place in cells of these widths.
    pub fn parts(&self, widths: CellWidths) -> CellParts {
        CellParts {
            name: self.name(widths.name),
            value: self.value(),
            bar: self.bar(widths.bar_cells),
        }
    }

    /// Its name, dim, with any labels a jump has put on it, in this much room, and a fader a hand
    /// has taken over from the steps marked after it.
    pub fn name(&self, room: u16) -> Line<'static> {
        let mut line = name_line(&self.look, &self.name, room);
        if self.state.taken {
            line.push_span(Span::raw(" "));
            line.push_span(Span::styled(glyphs::TAKEN.to_string(), theme::taken()));
        }
        line
    }

    /// Its value: what Insert has typed in its place, a toggle, or its text, in the cursor's colour
    /// under the cursor and in its tone otherwise, with a jump's label over its first letter; then
    /// a fine part, set back, and a word after it where it has them.
    pub fn value(&self) -> Line<'static> {
        let look = &self.look;
        let (text, style) = match self.face {
            Face::Toggle { on } if look.typed.is_none() => (toggle::text(on), self.style(Some(on))),
            Face::HexFine { .. } => {
                let text = setting::text(look, || self.text.clone());
                (format!("{text:<COARSE_WIDTH$}"), self.style(None))
            }
            _ => (setting::text(look, || self.text.clone()), self.style(None)),
        };
        let mut spans = value_spans(look, text, style);
        if let Face::HexFine { fine } = self.face {
            let fine = setting::fine_text(look, || format!("{fine:02X}"));
            spans.push(Span::styled(fine, setting::fine_style(look)));
        }
        if let Some((after, room)) = &self.state.after {
            let width: usize = spans.iter().map(Span::width).sum();
            spans.push(Span::raw(" ".repeat(room.saturating_sub(width))));
            spans.push(after.clone());
        }
        Line::from(spans)
    }

    /// The value's style: the cursor's under the cursor, and otherwise its tone, a toggle that
    /// isn't dim or tinted being in its own colour.
    fn style(&self, toggle: Option<bool>) -> Style {
        if self.look.focus == Focus::Value {
            return theme::cursor(self.look.mode);
        }
        match (self.state.tone, toggle) {
            (Tone::Tint(style), _) => style,
            (Tone::Automated, _) => theme::automated(),
            (Tone::Default, _) => setting::resting(true),
            (Tone::Plain, Some(on)) => toggle::style(on),
            (Tone::Plain, None) => setting::resting(false),
        }
    }

    /// Its bar in this many cells, filled to what's heard with a mark where it's set, a fader's the
    /// other way round, with a macro's range behind the fill; or nothing for a compact cell or a
    /// value with no bar, whose value then takes the bar's room.
    fn bar(&self, cells: usize) -> Option<ValueBar> {
        let place = self.place.filter(|_| cells > 0)?;
        let (set, heard) = (place.set, self.state.heard);
        // `new` is where the dim mark goes and `playing` what the fill follows. A fader is filled
        // to what's set, with the mark at what's heard.
        let (marked, filled) = match self.state.fader {
            true => (heard, set),
            false => (set, heard),
        };
        Some(
            ValueBar::new(marked, cells)
                .playing(filled)
                .centred(place.centred)
                .range(self.state.macro_range),
        )
    }
}

/// What a value plays with now, as the engine's report says it, moved by as much as its setting
/// has moved since the version the report was of: what it's set to now, what the report says it
/// plays with, and what it was set to then. A report is a frame or so behind a setting being
/// changed, so the fill would trail the hand and a mark for the setting would flicker at its edge;
/// this way it follows the hand at once, and what modulation adds keeps moving it (#421). With no
/// report of the same setting, it's what's set. It's in a macro's 16 bits, 0 to `FF00`; a byte is
/// the same shifted up by 8.
pub fn heard(set: u16, live: Option<u16>, then: Option<u16>) -> u16 {
    match (live, then) {
        (Some(live), Some(then)) => {
            (i32::from(live) + i32::from(set) - i32::from(then)).clamp(0, 0xFF00) as u16
        }
        _ => set,
    }
}

/// [`heard`] for a byte, as the bars have their values.
pub fn heard_byte(set: u8, live: Option<u8>, then: Option<u8>) -> u8 {
    let wide = |value: u8| u16::from(value) << 8;
    (heard(wide(set), live.map(wide), then.map(wide)) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whats_heard_follows_the_setting_from_the_report() {
        assert_eq!(heard_byte(0x48, Some(0x50), Some(0x40)), 0x58);
        assert_eq!(
            heard_byte(0x00, Some(0x10), Some(0x40)),
            0x00,
            "held at the bottom"
        );
        assert_eq!(
            heard_byte(0x80, Some(0xF0), Some(0x40)),
            0xFF,
            "and the top"
        );
        assert_eq!(
            heard_byte(0x80, None, Some(0x40)),
            0x80,
            "with no report, what's set"
        );
        assert_eq!(heard(0x8000, Some(0x9000), Some(0x8000)), 0x9000);
        assert_eq!(heard(0xF000, Some(0xF000), Some(0x1000)), 0xFF00);
    }
}
