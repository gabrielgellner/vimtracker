//! A window's hint, along the bottom of its border: what its keys do, and after that the keys that
//! flip something on and off, each with a switch that shows whether it's on (decision 101).

use ratatui::text::{Line, Span};

use crate::theme;
use crate::views::widgets::toggle;

/// A key that flips something on and off, and the word that names what.
pub struct Switch {
    pub key: char,
    pub on: bool,
    pub name: &'static str,
}

/// The hint, dim, then each switch's key, its switch, green when it's on and faint when it isn't,
/// and its name. The gaps are two spaces, and the switch, two cells wide, has a space either side,
/// so the sample explorer's fits under its list with the preview off.
pub fn hint_line(hint: &str, switches: &[Switch]) -> Line<'static> {
    let mut spans = vec![Span::styled(format!(" {hint}"), theme::dim())];
    for switch in switches {
        spans.push(Span::styled(format!("  {} ", switch.key), theme::dim()));
        spans.push(toggle::span(switch.on));
        spans.push(Span::styled(format!(" {}", switch.name), theme::dim()));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}
