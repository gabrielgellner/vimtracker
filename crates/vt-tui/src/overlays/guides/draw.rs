//! An open guide, in a window over the screen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::overlays::guides::Guide;
use crate::widgets::popup;

/// The widest the window gets, since long lines are hard to read.
const WIDTH: u16 = 100;
/// The border, and a margin on the right.
const AROUND_TEXT: u16 = 4;

/// How wide a guide's text is in an area of this width.
pub(crate) fn text_width(width: u16) -> usize {
    usize::from(WIDTH.min(width).saturating_sub(AROUND_TEXT)).max(1)
}

/// How many rows of a guide fit in an area of this height, inside the border.
pub(crate) fn page(height: u16) -> usize {
    usize::from(height.saturating_sub(2)).max(1)
}

/// Draw the guide from a scroll position, with the words a search found marked.
pub(crate) fn draw(frame: &mut Frame, area: Rect, guide: Guide, scroll: usize, found: &[String]) {
    let window = popup::centred(area, WIDTH, area.height);
    let lines: Vec<_> = guide
        .lines(text_width(area.width))
        .into_iter()
        .skip(scroll)
        .map(|line| marked(line, found))
        .collect();
    let border = popup::border()
        .title(format!(" {} ", guide.title()))
        .title_bottom(
            " j k scroll  Ctrl-d Ctrl-u page  g G ends  / search  n N found  ? guides  q close ",
        )
        .title_style(Style::new().add_modifier(Modifier::BOLD));
    let inside = popup::draw(frame, window, border);
    frame.render_widget(Paragraph::new(lines), inside);
}

/// A line with every place a word of the search is in it shown reversed, whatever its case.
pub(crate) fn marked(line: Line<'static>, found: &[String]) -> Line<'static> {
    if found.is_empty() {
        return line;
    }
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .to_ascii_lowercase();
    let mut hit = vec![false; text.len()];
    for word in found {
        for (at, _) in text.match_indices(word.as_str()) {
            hit[at..at + word.len()].fill(true);
        }
    }
    if !hit.contains(&true) {
        return line;
    }
    let mut spans = Vec::new();
    let mut at = 0;
    for span in line.spans {
        let content = span.content.as_ref();
        let mut from = 0;
        for (offset, _) in content.char_indices().skip(1) {
            if hit[at + offset] != hit[at + from] {
                spans.push(piece(&span, from, offset, hit[at + from]));
                from = offset;
            }
        }
        if from < content.len() {
            spans.push(piece(&span, from, content.len(), hit[at + from]));
        }
        at += content.len();
    }
    Line { spans, ..line }
}

/// Part of a span, reversed where a search found it.
fn piece(span: &Span<'static>, from: usize, to: usize, found: bool) -> Span<'static> {
    let style = if found {
        span.style.add_modifier(Modifier::REVERSED)
    } else {
        span.style
    };
    Span::styled(span.content[from..to].to_owned(), style)
}
