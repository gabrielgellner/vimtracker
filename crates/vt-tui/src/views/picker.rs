//! The picker: a centred box with a search line over a list, after snacks.nvim's `select` layout,
//! and a preview beside the list for lists that have one, after its `default` layout.
//!
//! It's one colour, as the rest is: categories are dim headings with a count, items are plain with
//! their meaning dim beside them, and the letters a search matched are in the picker's colour.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::input::picker::{Picker, Row};
use crate::kitty;
use crate::theme;

/// The list's width, inside its border, at least and at most.
const MIN_LIST_WIDTH: usize = 32;
const MAX_LIST_WIDTH: usize = 72;
/// The most rows a list shows before it scrolls.
const MAX_ROWS: usize = 16;
/// The preview's width, and the rows a box with one has.
const PREVIEW_WIDTH: usize = 40;
pub const PREVIEW_ROWS: usize = 18;
/// The size of each waveform in a kit's preview, in characters.
pub const PREVIEW_WAVE_WIDTH: usize = 8;
pub const PREVIEW_WAVE_HEIGHT: usize = 1;
/// Room for a label in a kit's preview, such as `BD`.
const PREVIEW_LABEL_WIDTH: usize = 4;
/// Where an item's name starts: after a category's arrow, or the mark on the item already chosen.
const GUTTER: usize = 2;
/// How far an item in a category is indented.
const INDENT: usize = 2;
/// The space between an item's name, its meaning and its category.
const GAP: usize = 2;

/// Draw the window over whatever is underneath.
pub fn draw(frame: &mut Frame, area: Rect, picker: &Picker) {
    let columns = Columns::of(picker);
    let preview = picker.has_previews();
    let list_width = (GUTTER + columns.indent + columns.label + columns.meaning + columns.category)
        .clamp(MIN_LIST_WIDTH, MAX_LIST_WIDTH);
    let rows = if preview {
        PREVIEW_ROWS
    } else {
        picker.items().len().clamp(1, MAX_ROWS)
    };
    let preview_width = if preview { PREVIEW_WIDTH + 1 } else { 0 };
    // The search line, and the border all round.
    let width = ((list_width + preview_width + 2) as u16).min(area.width);
    let height = ((rows + 3) as u16).min(area.height);
    let window = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let mut block = Block::new()
        .style(theme::window())
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(
                format!(" {} ", picker.title),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/{} ", picker.matching(), picker.items().len()),
                theme::dim(),
            ),
        ]));
    if let Some(hint) = picker.hint {
        block = block.title_bottom(Span::styled(format!(" {hint} "), theme::dim()));
    }
    let inner = block.inner(window);
    frame.render_widget(Clear, window);
    frame.render_widget(block, window);
    let list_area = Rect {
        width: inner.width.min(list_width as u16),
        ..inner
    };
    frame.render_widget(Paragraph::new(list(picker, &columns, list_area)), list_area);
    if preview && inner.width > list_area.width {
        let preview_area = Rect {
            x: list_area.x + list_area.width,
            width: inner.width - list_area.width,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(preview_lines(picker, preview_area)).block(
                Block::new()
                    .borders(Borders::LEFT)
                    .border_style(theme::faint()),
            ),
            preview_area,
        );
    }
}

/// How wide each part of an item's line is, from every item, so the box doesn't change size as
/// the list is searched or opened.
struct Columns {
    indent: usize,
    label: usize,
    meaning: usize,
    category: usize,
}

impl Columns {
    fn of(picker: &Picker) -> Self {
        let widest = |part: fn(&crate::input::picker::Item) -> &str| {
            picker
                .items()
                .iter()
                .map(|item| part(item).chars().count())
                .max()
                .unwrap_or(0)
        };
        let gapped = |width: usize| if width == 0 { 0 } else { width + GAP };
        Self {
            indent: if picker.has_categories() { INDENT } else { 0 },
            label: widest(|item| &item.label),
            meaning: gapped(widest(|item| &item.meaning)),
            category: gapped(widest(|item| &item.category)),
        }
    }
}

/// The search line and the rows that fit, scrolled just enough to keep the cursor in view.
fn list(picker: &Picker, columns: &Columns, area: Rect) -> Vec<Line<'static>> {
    let width = usize::from(area.width);
    let room = usize::from(area.height).saturating_sub(1);
    let query = if picker.focus == crate::input::picker::Focus::Search {
        Line::raw(format!("> {}_", picker.query))
    } else {
        Line::styled(format!("> {}", picker.count()), theme::dim())
    };
    let first = (picker.selected + 1).saturating_sub(room);
    let mut lines = vec![query];
    for (at, row) in picker.rows().iter().enumerate().skip(first).take(room) {
        let spans = match row {
            Row::Category(category) => {
                let (name, count, open) = picker.category(*category);
                let arrow = if open { '▾' } else { '▸' };
                vec![Span::styled(
                    format!("{arrow} {name}  {count}"),
                    theme::dim(),
                )]
            }
            Row::Item(item) => item_spans(picker, columns, at, *item, width),
        };
        lines.push(if at == picker.selected {
            selected(spans, width)
        } else {
            Line::from(spans)
        });
    }
    lines
}

/// An item's line: its mark, its name, its meaning dim, and its category dim at the right while
/// searching or when it stands alone, with the letters a search matched picked out.
fn item_spans(
    picker: &Picker,
    columns: &Columns,
    row: usize,
    index: usize,
    width: usize,
) -> Vec<Span<'static>> {
    let item = picker.item(index);
    let matched = picker.matched(row);
    let nested = picker.is_nested(index) && !picker.is_searching();
    let mark = if picker.marked() == Some(index) {
        "• "
    } else {
        "  "
    };
    let mut spans = vec![Span::raw(format!(
        "{}{mark}",
        if nested {
            " ".repeat(INDENT)
        } else {
            String::new()
        }
    ))];
    let label_length = item.label.chars().count();
    spans.extend(matched_spans(&item.label, 0, matched, Style::new()));
    let mut used = GUTTER + if nested { INDENT } else { 0 } + label_length;
    if !item.meaning.is_empty() {
        // Meanings line up whether their items are in a category or not.
        let indent = if nested { INDENT } else { 0 };
        let pad = (columns.indent + columns.label + GAP).saturating_sub(indent + label_length);
        spans.push(Span::raw(" ".repeat(pad)));
        let start = label_length as u32 + 1;
        spans.extend(matched_spans(&item.meaning, start, matched, theme::dim()));
        used += pad + item.meaning.chars().count();
    }
    let shows_category = picker.is_searching() || !picker.is_nested(index);
    let category_length = item.category.chars().count();
    if shows_category && category_length > 0 && used + GAP + category_length <= width {
        spans.push(Span::raw(" ".repeat(width - used - category_length)));
        let start = (label_length + item.meaning.chars().count()) as u32 + 2;
        spans.extend(matched_spans(&item.category, start, matched, theme::dim()));
    }
    spans
}

/// Text in a style, with the characters at these positions, counted from `start`, in the
/// picker's colour.
fn matched_spans(text: &str, start: u32, matched: &[u32], style: Style) -> Vec<Span<'static>> {
    let hit = style.fg(theme::TEAL).add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_hit = false;
    for (at, c) in text.chars().enumerate() {
        let is_hit = matched.binary_search(&(start + at as u32)).is_ok();
        if is_hit != run_hit && !run.is_empty() {
            let style = if run_hit { hit } else { style };
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        run_hit = is_hit;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, if run_hit { hit } else { style }));
    }
    spans
}

/// The row under the cursor, picked out across the list's width.
fn selected(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let mut spans: Vec<Span<'static>> = spans
        .into_iter()
        .map(|span| {
            let style = theme::selected().patch(span.style);
            span.style(style.bg(theme::SURFACE1))
        })
        .collect();
    spans.push(Span::styled(
        " ".repeat(width.saturating_sub(used)),
        theme::selected(),
    ));
    Line::from(spans)
}

/// What the preview shows: the item's name, a note under it, and its lines, such as a kit's
/// sounds with their waveforms.
fn preview_lines(picker: &Picker, area: Rect) -> Vec<Line<'static>> {
    let Some(shown) = picker.preview.as_deref() else {
        return Vec::new();
    };
    let width = usize::from(area.width).saturating_sub(2);
    let mut lines = vec![
        Line::raw(format!(" {}", shown.heading)),
        Line::styled(format!(" {}", shown.note), theme::dim()),
        Line::raw(""),
    ];
    let room = usize::from(area.height).saturating_sub(lines.len());
    let more = shown.lines.len().saturating_sub(room);
    let fits = if more > 0 {
        room.saturating_sub(1)
    } else {
        room
    };
    let name_width = width.saturating_sub(PREVIEW_LABEL_WIDTH + PREVIEW_WAVE_WIDTH + GAP);
    let wave = &kitty::placeholder_lines(PREVIEW_WAVE_WIDTH, PREVIEW_WAVE_HEIGHT)[0];
    for (at, line) in shown.lines.iter().take(fits).enumerate() {
        let name: String = line.name.chars().take(name_width).collect();
        let mut spans = vec![
            Span::raw(format!(" {:<PREVIEW_LABEL_WIDTH$}", line.label)),
            Span::raw(format!("{name:<name_width$}")),
            Span::raw(" ".repeat(GAP)),
        ];
        if let Some(Some(id)) = picker.preview_images.get(at) {
            spans.push(Span::styled(
                wave.clone(),
                Style::new().fg(kitty::id_color(*id)),
            ));
        }
        lines.push(Line::from(spans));
    }
    if more > 0 {
        lines.push(Line::styled(
            format!(" and {} more", shown.lines.len() - fits),
            theme::dim(),
        ));
    }
    lines
}
