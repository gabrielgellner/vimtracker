//! A setting as the settings pages show one: its name dim, with any labels from a jump on it, and
//! its value, what's been typed in its place in Insert, in the cursor's colour under the cursor,
//! and dim where it's left at its default. A page puts these in a grid's cells, with a fine part
//! and a `ValueBar` beside them where the setting has those.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::input::mode::Mode;
use crate::page::Ctx;
use crate::theme;
use crate::track_page::Control;

/// A setting's value or fine part as text: while Insert has the cursor on it, what's been typed,
/// with a `_` where the next character goes, and otherwise what it's set to.
pub fn text(ctx: &Ctx, here: bool, value: impl FnOnce() -> String) -> String {
    if here && ctx.mode() == Mode::Insert {
        format!("{}_", ctx.entry().text)
    } else {
        value()
    }
}

/// A setting's value's style: the cursor's under the cursor, dim where it's left at its default,
/// which matters less, and plain otherwise.
pub fn style(ctx: &Ctx, here: bool, default: bool) -> Style {
    if here {
        theme::cursor(ctx.mode())
    } else if default {
        theme::dim()
    } else {
        Style::new()
    }
}

/// The label a jump has put on a setting, or nothing.
pub fn jump_label(ctx: &Ctx, control: Control) -> String {
    ctx.field_jump()
        .and_then(|jump| jump.label(control))
        .map_or_else(String::new, String::from)
}

/// A setting's value, with a jump's label over its first letter if it has one.
pub fn value_spans(ctx: &Ctx, control: Control, text: String, style: Style) -> Vec<Span<'static>> {
    let label = ctx.field_jump().and_then(|jump| jump.label(control));
    match label {
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
pub fn name_line(ctx: &Ctx, name: &str, labels: &str, room: u16) -> Line<'static> {
    let letters: Vec<char> = name.chars().collect();
    let typed = ctx.field_jump().and_then(|jump| {
        let at = name.to_lowercase().find(&jump.query)?;
        Some(at + jump.query.len())
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
    use crate::views::widgets::pin::{app, assert_drawn, drawn, press};

    const CUT: Control = Control::Base(TrackControl::Cutoff);
    const PAN: Control = Control::Base(TrackControl::Pan);

    /// Settings a row each, their names in ten cells and their values after them, as a grid's
    /// cells have them.
    fn cells(ctx: &Ctx, settings: &[(Control, &str, &str, Style)]) -> Buffer {
        let area = Rect::new(0, 0, 12, settings.len() as u16);
        let mut buffer = Buffer::empty(area);
        for (y, (control, name, value, style)) in (0..).zip(settings) {
            let labels = jump_label(ctx, *control);
            name_line(ctx, name, &labels, 10).render(Rect::new(0, y, 10, 1), &mut buffer);
            let value = Line::from(value_spans(ctx, *control, (*value).to_owned(), *style));
            value.render(Rect::new(10, y, 2, 1), &mut buffer);
        }
        buffer
    }

    #[test]
    fn a_value_is_in_the_cursors_colour_under_it_dim_at_its_default_and_plain_otherwise() {
        let (mut app, _realtime) = app();
        let ctx = app.ctx();
        assert_eq!(style(&ctx, true, true), theme::cursor(Mode::Normal));
        assert_eq!(style(&ctx, false, true), theme::dim());
        assert_eq!(style(&ctx, false, false), Style::new());
        let line = Line::from(vec![
            Span::styled("A0", style(&ctx, true, false)),
            Span::raw(" "),
            Span::styled("80", style(&ctx, false, true)),
            Span::raw(" "),
            Span::styled("C0", style(&ctx, false, false)),
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
        assert_eq!(style(&app.ctx(), true, false), theme::cursor(Mode::Insert));
    }

    #[test]
    fn insert_shows_whats_typed_under_the_cursor_and_the_value_elsewhere() {
        let (mut app, _realtime) = app();
        assert_eq!(text(&app.ctx(), true, || "A0".to_owned()), "A0");
        press(&mut app, " 5}ji4");
        assert_eq!(app.modes.mode, Mode::Insert);
        let ctx = app.ctx();
        assert_eq!(text(&ctx, true, || "A0".to_owned()), "4_");
        assert_eq!(text(&ctx, false, || "A0".to_owned()), "A0");
    }

    #[test]
    fn a_jump_labels_the_letter_after_whats_typed_and_the_values_first_letter() {
        let (mut app, _realtime) = app();
        let ctx = app.ctx();
        assert_eq!(jump_label(&ctx, CUT), "");
        assert_drawn(
            &cells(&ctx, &[(CUT, "Cut", "40", Style::new())]),
            &["Cut       40", "aaaaaaaaaa..", "a fg OVERLAY2"],
        );
        press(&mut app, " 5/cu");
        let ctx = app.ctx();
        assert_eq!(jump_label(&ctx, CUT), "f");
        assert_eq!(jump_label(&ctx, PAN), "", "Pan doesn't match");
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
        let ctx = app.ctx();
        assert_drawn(
            &drawn(name_line(&ctx, "Cut", "fj", 3), 5, 1),
            &[
                "Cfj  ",
                "abb..",
                "a fg OVERLAY2",
                "b fg BASE bg PEACH +BOLD",
            ],
        );
    }
}
