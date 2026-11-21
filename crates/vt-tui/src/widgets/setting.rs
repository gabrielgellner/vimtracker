//! A setting as the settings pages show one: its name dim, with any labels from a jump on it, and
//! its value, what's been typed in its place in Insert, in the cursor's colour under the cursor,
//! and dim where it's left at its default. A page puts these in a grid's cells, with a fine part
//! and a `ValueBar` beside them where the setting has those. What the modes say about a cell, the
//! cursor, Insert's text and a jump's labels, comes in a `CellLook` the page builds, so these read
//! nothing of the program themselves.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::input::mode::Mode;
use crate::widgets::theme;

/// Where the cursor is on a setting's cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Off,
    /// On its value, or on the coarse part of one with a fine part.
    Value,
    /// On its fine part.
    Fine,
}

impl Focus {
    /// Where the cursor is on a cell: off it unless it's here, and on its fine part if it's there.
    pub fn at(here: bool, fine: bool) -> Self {
        match (here, fine) {
            (false, _) => Self::Off,
            (true, false) => Self::Value,
            (true, true) => Self::Fine,
        }
    }
}

/// What the modes say about a setting's cell: where the cursor is on it, the mode the cursor's
/// colour comes from, whether the keys are elsewhere, what Insert has typed, and a field jump's
/// query and labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellLook<'a> {
    pub focus: Focus,
    pub mode: Mode,
    /// Whether the keys are in another area on screen, so the cursor sits back (decision 475).
    pub away: bool,
    /// What Insert has typed, while it's on this cell.
    pub typed: Option<&'a str>,
    /// A field jump's query, while one is under way.
    pub query: Option<&'a str>,
    /// The labels a jump puts over the name, or nothing.
    pub(crate) name_labels: &'a str,
    /// The label a jump puts over the value's first letter.
    pub(crate) value_label: Option<char>,
}

impl CellLook<'_> {
    /// The cursor's style on this cell: the mode's, or sat back while the keys are elsewhere.
    pub(crate) fn cursor(&self) -> Style {
        if self.away {
            theme::cursor_away()
        } else {
            theme::cursor(self.mode)
        }
    }
}

/// A part of a setting's value as text: while Insert has the cursor on it, what's been typed, with
/// a `_` where the next character goes, and otherwise what it's set to.
fn part_text(look: &CellLook, part: Focus, value: impl FnOnce() -> String) -> String {
    match look.typed.filter(|_| look.focus == part) {
        Some(typed) => format!("{typed}_"),
        None => value(),
    }
}

/// A part of a setting's value's style: the cursor's under the cursor, and otherwise `resting`.
fn part_style(look: &CellLook, part: Focus, default: bool) -> Style {
    if look.focus == part {
        look.cursor()
    } else {
        resting(default)
    }
}

/// A setting's value as text: while Insert has the cursor on it, what's been typed, with a `_`
/// where the next character goes, and otherwise what it's set to.
pub fn text(look: &CellLook, value: impl FnOnce() -> String) -> String {
    part_text(look, Focus::Value, value)
}

/// A setting's fine part as text, as `text` has its value.
pub(crate) fn fine_text(look: &CellLook, value: impl FnOnce() -> String) -> String {
    part_text(look, Focus::Fine, value)
}

/// A setting's value's style: the cursor's under the cursor, dim where it's left at its default,
/// which matters less, and plain otherwise.
pub fn style(look: &CellLook, default: bool) -> Style {
    part_style(look, Focus::Value, default)
}

/// A setting's fine part's style: the cursor's under the cursor, and dim otherwise.
pub(crate) fn fine_style(look: &CellLook) -> Style {
    part_style(look, Focus::Fine, true)
}

/// A value's style away from the cursor: dim where it's left at its default, plain otherwise.
pub(crate) fn resting(default: bool) -> Style {
    if default { theme::dim() } else { Style::new() }
}

/// A setting's value, with a jump's label over its first letter if it has one.
pub(crate) fn value_spans(look: &CellLook, text: String, style: Style) -> Vec<Span<'static>> {
    match look.value_label {
        Some(label) => vec![
            Span::styled(label.to_string(), theme::jump_label()),
            Span::styled(text.chars().skip(1).collect::<String>(), style),
        ],
        None => vec![Span::styled(text, style)],
    }
}

/// A setting's name in its column, with any labels from a jump over the letters just after what's
/// been typed, as flash.nvim draws them, since that's where the eye is while searching. The labels
/// stay inside the room the column has.
pub(crate) fn name_line(look: &CellLook, name: &str, room: u16) -> Line<'static> {
    let labels = look.name_labels;
    let letters: Vec<char> = name.chars().collect();
    let typed = look.query.and_then(|query| {
        let at = name.to_lowercase().find(query)?;
        Some(at + query.len())
    });
    let Some(typed) = typed.filter(|_| !labels.is_empty()) else {
        return Line::styled(name.to_owned(), theme::dim());
    };
    let count = labels.chars().count();
    let before = typed.min(usize::from(room).saturating_sub(count));
    let after: String = letters.iter().skip(before + count).collect();
    Line::from(vec![
        Span::styled(
            letters[..before.min(letters.len())]
                .iter()
                .collect::<String>(),
            theme::dim(),
        ),
        Span::styled(labels.to_owned(), theme::jump_label()),
        Span::styled(after, theme::dim()),
    ])
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use vt_model::action::TrackControl;

    use super::*;
    use crate::controls::Control;
    use crate::pages::Ctx;
    use crate::widgets::pin::{app, assert_drawn, drawn, press};

    const CUT: Control = Control::Base(TrackControl::Cutoff);
    const PAN: Control = Control::Base(TrackControl::Pan);

    /// Settings a row each, their names in ten cells and their values after them, as a grid's
    /// cells have them.
    fn cells(ctx: &Ctx, settings: &[(Control, &str, &str, Style)]) -> Buffer {
        let area = Rect::new(0, 0, 12, settings.len() as u16);
        let mut buffer = Buffer::empty(area);
        for (y, (control, name, value, style)) in (0..).zip(settings) {
            let look = ctx.look_on(*control, Focus::Off);
            name_line(&look, name, 10).render(Rect::new(0, y, 10, 1), &mut buffer);
            let value = Line::from(value_spans(&look, (*value).to_owned(), *style));
            value.render(Rect::new(10, y, 2, 1), &mut buffer);
        }
        buffer
    }

    #[test]
    fn a_value_is_in_the_cursors_colour_under_it_dim_at_its_default_and_plain_otherwise() {
        let (mut app, _realtime) = app();
        let ctx = app.ctx();
        let (here, away) = (ctx.look(Focus::Value), ctx.look(Focus::Off));
        assert_eq!(style(&here, true), theme::cursor(Mode::Normal));
        assert_eq!(style(&away, true), theme::dim());
        assert_eq!(style(&away, false), Style::new());
        let line = Line::from(vec![
            Span::styled("A0", style(&here, false)),
            Span::raw(" "),
            Span::styled("80", style(&away, true)),
            Span::raw(" "),
            Span::styled("C0", style(&away, false)),
        ]);
        assert_drawn(
            &drawn(line, 8, 1),
            &[
                "A0 80 C0",
                "aa.bb...",
                "a fg BASE bg ROSEWATER",
                "b fg OVERLAY2",
            ],
        );
        press(&mut app, " 5}ji4");
        assert_eq!(
            style(&app.ctx().look(Focus::Value), false),
            theme::cursor(Mode::Insert)
        );
    }

    #[test]
    fn a_fine_part_is_in_the_cursors_colour_under_it_and_dim_otherwise() {
        let (app, _realtime) = app();
        let ctx = app.ctx();
        assert_eq!(
            fine_style(&ctx.look(Focus::Fine)),
            theme::cursor(Mode::Normal)
        );
        assert_eq!(fine_style(&ctx.look(Focus::Value)), theme::dim());
        assert_eq!(style(&ctx.look(Focus::Fine), false), Style::new());
    }

    #[test]
    fn insert_shows_whats_typed_under_the_cursor_and_the_value_elsewhere() {
        let (mut app, _realtime) = app();
        assert_eq!(
            text(&app.ctx().look(Focus::Value), || "A0".to_owned()),
            "A0"
        );
        press(&mut app, " 5}ji4");
        assert_eq!(app.modes.mode, Mode::Insert);
        let ctx = app.ctx();
        assert_eq!(text(&ctx.look(Focus::Value), || "A0".to_owned()), "4_");
        assert_eq!(text(&ctx.look(Focus::Off), || "A0".to_owned()), "A0");
        assert_eq!(fine_text(&ctx.look(Focus::Fine), || "00".to_owned()), "4_");
        assert_eq!(text(&ctx.look(Focus::Fine), || "A0".to_owned()), "A0");
    }

    #[test]
    fn a_jump_labels_the_letter_after_whats_typed_and_the_values_first_letter() {
        let (mut app, _realtime) = app();
        let ctx = app.ctx();
        assert_eq!(ctx.look_on(CUT, Focus::Off).name_labels, "");
        assert_drawn(
            &cells(&ctx, &[(CUT, "Cut", "40", Style::new())]),
            &["Cut       40", "aaaaaaaaaa..", "a fg OVERLAY2"],
        );
        press(&mut app, " 5/cu");
        let ctx = app.ctx();
        assert_eq!(ctx.look_on(CUT, Focus::Off).name_labels, "f");
        assert_eq!(
            ctx.look_on(PAN, Focus::Off).name_labels,
            "",
            "Pan doesn't match"
        );
        let drawn = cells(
            &ctx,
            &[
                (CUT, "Cut", "40", Style::new()),
                (PAN, "Pan", "80", theme::dim()),
            ],
        );
        assert_drawn(
            &drawn,
            &[
                "Cuf       f0",
                "aab.......b.",
                "Pan       80",
                "aaaaaaaaaaaa",
                "a fg OVERLAY2",
                "b fg BASE bg PEACH +BOLD",
            ],
        );
    }

    #[test]
    fn a_jumps_labels_stay_inside_the_room_the_column_has() {
        let (mut app, _realtime) = app();
        press(&mut app, " 5/cu");
        let look = CellLook {
            name_labels: "fj",
            ..app.ctx().look(Focus::Off)
        };
        assert_drawn(
            &drawn(name_line(&look, "Cut", 3), 5, 1),
            &[
                "Cfj  ",
                "abb..",
                "a fg OVERLAY2",
                "b fg BASE bg PEACH +BOLD",
            ],
        );
    }
}
