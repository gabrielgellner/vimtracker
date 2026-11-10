//! The picker: a centred box with a search line over a list, after snacks.nvim's `select` layout,
//! and a preview beside the list for lists that have one, after its `default` layout.
//!
//! It's one colour, as the rest is: categories are dim headings with a count, items are plain with
//! their meaning dim beside them, and the letters a search matched are in the picker's colour.
//!
//! The file explorer is the same box, a fixed size as its tree grows, with relative line numbers
//! dim down the left, as the author's snacks explorer has, so `12j` can be read off the screen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::glyphs;
use crate::input::picker::{Picker, Row};
use crate::kitty;
use crate::theme;

/// The list's width, inside its border, at least and at most.
const MIN_LIST_WIDTH: usize = 32;
const MAX_LIST_WIDTH: usize = 72;
/// The most rows a list shows before it scrolls.
const MAX_ROWS: usize = 16;
/// The command line's list of matches: its narrowest, and the most rows it shows before it
/// scrolls, as Vim's popup menu keeps to a few.
const MIN_MENU_WIDTH: usize = 12;
const MAX_MENU_ROWS: usize = 10;
/// The preview's width, and the rows a box with one has.
const PREVIEW_WIDTH: usize = 40;
pub const PREVIEW_ROWS: usize = 18;
/// The size of each waveform in a kit's preview, in characters.
pub const PREVIEW_WAVE_WIDTH: usize = 8;
pub const PREVIEW_WAVE_HEIGHT: usize = 1;
/// The size of a sample's waveform in its preview, in characters. It was as wide as the program's
/// placeholders could number at first; they now number 297, and it stays 28 to fit the preview.
pub const PREVIEW_SAMPLE_WIDTH: usize = 28;
pub const PREVIEW_SAMPLE_HEIGHT: usize = 4;
/// Room for a label in a kit's preview, such as `BD`.
const PREVIEW_LABEL_WIDTH: usize = 4;
/// The explorer's list's width, inside its border, as its tree grows and shrinks. It's 54 so the
/// sample explorer's hint, with its three switches, fits under it with the preview off.
const EXPLORER_WIDTH: usize = 54;
/// Room for a relative line number in the explorer, and the space after it.
const NUMBER_WIDTH: usize = 3;
/// Where an item's name starts: after a category's arrow, or the mark on the item already chosen.
const GUTTER: usize = 2;
/// How far an item in a category is indented.
const INDENT: usize = 2;
/// The space between an item's name, its meaning and its category.
const GAP: usize = 2;

/// Draw the window over whatever is underneath.
pub fn draw(frame: &mut Frame, area: Rect, picker: &mut Picker) {
    let columns = Columns::of(picker);
    let preview = picker.has_previews();
    let explorer = picker.tree().is_some();
    let list_width = if explorer {
        EXPLORER_WIDTH
    } else {
        (GUTTER + columns.indent + columns.label + columns.meaning + columns.category)
            .clamp(MIN_LIST_WIDTH, MAX_LIST_WIDTH)
    };
    let rows = if preview || explorer {
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
    let mut title = vec![Span::styled(
        format!(" {} ", picker.title),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    // The explorer's count would only be what's been read so far.
    if !explorer {
        title.push(Span::styled(
            format!("{}/{} ", picker.matching(), picker.items().len()),
            theme::dim(),
        ));
    }
    let mut block = Block::new()
        .style(theme::window())
        .borders(Borders::ALL)
        .title(Line::from(title));
    if let Some(hint) = picker.hint {
        block = block.title_bottom(hint_line(picker, hint));
    }
    let inner = block.inner(window);
    frame.render_widget(Clear, window);
    frame.render_widget(block, window);
    let list_area = Rect {
        width: inner.width.min(list_width as u16),
        ..inner
    };
    // Under the search line.
    picker.scroll_within(usize::from(list_area.height).saturating_sub(1));
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

/// The hint under the list, and in the explorer its `H`, `P` and `A` after it, each with a switch
/// that shows whether it's on: green when it is, and faint when it isn't. The gaps are two spaces,
/// so the sample explorer's fits under its list with the preview off.
fn hint_line(picker: &Picker, hint: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(format!(" {hint}"), theme::dim())];
    if let Some(tree) = picker.tree() {
        let mut switch = |key: char, on: bool, name: &str| {
            let (glyph, style) = if on {
                (glyphs::SWITCH_ON, theme::active())
            } else {
                (glyphs::SWITCH_OFF, theme::faint())
            };
            spans.push(Span::styled(format!("  {key} "), theme::dim()));
            spans.push(Span::styled(glyph.to_string(), style));
            spans.push(Span::styled(format!(" {name}"), theme::dim()));
        };
        switch('H', tree.hidden, "hidden");
        if tree.purpose.previews() {
            switch('P', tree.preview, "preview");
        }
        if tree.purpose.auditions() {
            switch('A', tree.audition, "audition");
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Draw the list of matches the command line's Tab shows, as a small box that sits on the status
/// line, above the command line in the bottom row of `status`, with its names under the word
/// they finish, which starts `column` characters in. It's after Vim's popup menu, with no search
/// line, as the command line is its search.
pub fn draw_menu(frame: &mut Frame, status: Rect, column: usize, picker: &mut Picker) {
    let area = frame.area();
    let columns = Columns::of(picker);
    let list_width =
        (GUTTER + columns.label + columns.meaning).clamp(MIN_MENU_WIDTH, MAX_LIST_WIDTH);
    let rows_shown = picker.items().len().clamp(1, MAX_MENU_ROWS);
    // The border all round, and a space inside it on the right.
    let width = ((list_width + 3) as u16).min(area.width);
    let bottom = status.bottom().saturating_sub(1);
    let height = ((rows_shown + 2) as u16).min(bottom.saturating_sub(area.y));
    let left = (column.saturating_sub(GUTTER + 1) as u16).min(area.width - width);
    let window = Rect {
        x: area.x + left,
        y: bottom - height,
        width,
        height,
    };
    let block = Block::new()
        .style(theme::window())
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(
                format!(" {} ", picker.title),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{} ", picker.items().len()), theme::dim()),
        ]));
    let inner = block.inner(window);
    frame.render_widget(Clear, window);
    frame.render_widget(block, window);
    picker.scroll_within(usize::from(inner.height));
    let lines = rows(
        picker,
        &columns,
        usize::from(inner.width),
        usize::from(inner.height),
    );
    frame.render_widget(Paragraph::new(lines), inner);
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

/// The search line and the rows that fit, from the picker's first row shown.
fn list(picker: &Picker, columns: &Columns, area: Rect) -> Vec<Line<'static>> {
    let query = if picker.focus == crate::input::picker::Focus::Search {
        Line::raw(format!("> {}_", picker.query))
    } else {
        Line::styled(format!("> {}", picker.count()), theme::dim())
    };
    let room = usize::from(area.height).saturating_sub(1);
    let mut lines = vec![query];
    lines.extend(rows(picker, columns, usize::from(area.width), room));
    lines
}

/// The rows that fit in `room`, from the picker's first row shown.
fn rows(picker: &Picker, columns: &Columns, width: usize, room: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (at, row) in picker
        .rows()
        .iter()
        .enumerate()
        .skip(picker.first_shown())
        .take(room)
    {
        let spans = match row {
            Row::Item(item) if picker.tree().is_some() => tree_spans(picker, at, *item, width),
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

/// A line of the explorer: its distance from the cursor's, or its own number on the cursor's, as
/// Vim's `relativenumber` with `number` shows them, then the file or folder indented by its
/// depth, a folder with its arrow, the item already chosen with its mark, and its meaning dim.
fn tree_spans(picker: &Picker, row: usize, index: usize, width: usize) -> Vec<Span<'static>> {
    let item = picker.item(index);
    let number = if row == picker.selected {
        row + 1
    } else {
        row.abs_diff(picker.selected)
    };
    let mark = match picker.folder_open(index) {
        Some(true) => "▾ ",
        Some(false) => "▸ ",
        None if picker.marked() == Some(index) => "• ",
        None => "  ",
    };
    let indent = INDENT * picker.depth(index);
    let mut spans = vec![
        Span::styled(format!("{number:>NUMBER_WIDTH$} "), theme::faint()),
        Span::raw(format!("{}{mark}", " ".repeat(indent))),
    ];
    let meaning = if item.meaning.is_empty() {
        0
    } else {
        GAP + item.meaning.chars().count()
    };
    let room = width.saturating_sub(NUMBER_WIDTH + 1 + indent + GUTTER + meaning);
    let length = item.label.chars().count();
    if length > room {
        // A long path keeps its end, which says most about where it is.
        let kept: String = item.label.chars().skip(length + 1 - room.max(1)).collect();
        spans.push(Span::raw(format!("…{kept}")));
    } else {
        spans.extend(matched_spans(
            &item.label,
            0,
            picker.matched(row),
            Style::new(),
        ));
    }
    if !item.meaning.is_empty() {
        spans.push(Span::styled(
            format!("{}{}", " ".repeat(GAP), item.meaning),
            theme::dim(),
        ));
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
    ];
    lines.extend(
        shown
            .details
            .iter()
            .map(|detail| Line::raw(format!(" {detail}"))),
    );
    lines.push(Line::raw(""));
    if shown.wave.is_some()
        && let Some(id) = picker.preview_wave
    {
        let style = Style::new().fg(kitty::id_color(id));
        for row in kitty::placeholder_lines(PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT) {
            lines.push(Line::from(vec![Span::raw(" "), Span::styled(row, style)]));
        }
    }
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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use crate::input::keys::Key;
    use crate::input::picker::{Item, Pick, Picker};

    fn sounds(count: usize) -> Picker {
        let items = (0..count)
            .map(|at| Item::new(format!("{at:02} sound"), Pick::Track(at)))
            .collect();
        Picker::new("Sounds", items)
    }

    /// The screen's text, a line to a row.
    fn text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let width = usize::from(buffer.area.width);
        buffer
            .content
            .chunks(width)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>() + "\n")
            .collect()
    }

    #[test]
    fn the_list_shows_rows_below_the_cursor_as_it_goes_down() {
        let mut picker = sounds(40);
        let mut terminal = Terminal::new(TestBackend::new(163, 42)).unwrap();
        terminal
            .draw(|frame| super::draw(frame, frame.area(), &mut picker))
            .unwrap();
        for c in "13j".chars() {
            picker.key(Key::char(c));
        }
        terminal
            .draw(|frame| super::draw(frame, frame.area(), &mut picker))
            .unwrap();
        let shown = text(&terminal);
        assert!(shown.contains("13 sound"), "the cursor's row\n{shown}");
        assert!(shown.contains("16 sound"), "three rows under it\n{shown}");
        assert!(!shown.contains("00 sound"), "scrolled by one\n{shown}");
        assert!(!shown.contains("17 sound"));
    }

    #[test]
    fn the_command_lines_list_keeps_the_same_margin() {
        let mut picker = sounds(20);
        picker.selected = 9;
        let mut terminal = Terminal::new(TestBackend::new(163, 42)).unwrap();
        let status = Rect::new(0, 40, 163, 2);
        terminal
            .draw(|frame| super::draw_menu(frame, status, 0, &mut picker))
            .unwrap();
        let shown = text(&terminal);
        assert!(shown.contains("12 sound"), "three rows under it\n{shown}");
        assert!(
            shown.contains("03 sound"),
            "ten rows from the fourth\n{shown}"
        );
        assert!(!shown.contains("02 sound"));
    }
}
