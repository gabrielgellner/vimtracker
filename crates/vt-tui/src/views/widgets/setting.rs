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
