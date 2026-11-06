//! The topic guides: markdown files in `guides/`, built into the program and opened with
//! `Space ?`. Only the markdown they use is understood: headings, tables, fences, `code` and bold.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;

/// One guide. Its title is its first heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub source: &'static str,
}

/// Every guide, in the order to learn them.
pub const GUIDES: [Guide; 10] = [
    Guide {
        source: include_str!("../guides/first-loop.md"),
    },
    Guide {
        source: include_str!("../guides/navigation.md"),
    },
    Guide {
        source: include_str!("../guides/note-entry.md"),
    },
    Guide {
        source: include_str!("../guides/editing.md"),
    },
    Guide {
        source: include_str!("../guides/fx.md"),
    },
    Guide {
        source: include_str!("../guides/chains.md"),
    },
    Guide {
        source: include_str!("../guides/instruments.md"),
    },
    Guide {
        source: include_str!("../guides/tracks.md"),
    },
    Guide {
        source: include_str!("../guides/saving.md"),
    },
    Guide {
        source: include_str!("../guides/meter.md"),
    },
];

impl Guide {
    /// The first heading.
    pub fn title(self) -> &'static str {
        self.source
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or("Guide")
    }

    /// The guide as lines on screen.
    pub fn lines(self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut table: Vec<&str> = Vec::new();
        let mut fenced = false;
        for line in self.source.lines() {
            if line.starts_with('|') {
                table.push(line);
                continue;
            }
            lines.append(&mut table_lines(&table));
            table.clear();
            if line.starts_with("```") {
                fenced = !fenced;
            } else if fenced {
                lines.push(Line::styled(line.to_owned(), code()));
            } else if let Some(title) = line.strip_prefix("# ") {
                lines.push(Line::styled(title.to_owned(), heading(theme::MAUVE)));
            } else if let Some(title) = line.strip_prefix("## ") {
                lines.push(Line::styled(title.to_owned(), heading(theme::BLUE)));
            } else {
                lines.push(Line::from(inline(line)));
            }
        }
        lines.append(&mut table_lines(&table));
        lines
    }
}

fn heading(color: ratatui::style::Color) -> Style {
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}

fn code() -> Style {
    Style::new().fg(theme::PEACH)
}

/// Split text on `code` and `**bold**`, dropping the markers.
fn inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let marker = [
            ("`", code()),
            ("**", Style::new().add_modifier(Modifier::BOLD)),
        ]
        .into_iter()
        .filter_map(|(mark, style)| {
            let start = rest.find(mark)?;
            let end = rest[start + mark.len()..].find(mark)?;
            Some((
                start,
                start + mark.len(),
                start + mark.len() + end,
                mark,
                style,
            ))
        })
        .min_by_key(|(start, ..)| *start);
        let Some((start, from, to, mark, style)) = marker else {
            spans.push(Span::raw(rest.to_owned()));
            break;
        };
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_owned()));
        }
        spans.push(Span::styled(rest[from..to].to_owned(), style));
        rest = &rest[to + mark.len()..];
    }
    spans
}

/// Lay out table rows in aligned columns, with the first row as a heading.
fn table_lines(rows: &[&str]) -> Vec<Line<'static>> {
    let cells: Vec<Vec<Vec<Span<'static>>>> = rows
        .iter()
        .filter(|row| !row.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')))
        .map(|row| {
            row.trim_matches('|')
                .split('|')
                .map(|c| inline(c.trim()))
                .collect()
        })
        .collect();
    let width = |cell: &[Span]| cell.iter().map(Span::width).sum::<usize>();
    let mut widths: Vec<usize> = Vec::new();
    for row in &cells {
        for (column, cell) in row.iter().enumerate() {
            if column == widths.len() {
                widths.push(0);
            }
            widths[column] = widths[column].max(width(cell));
        }
    }
    let bold = Style::new().add_modifier(Modifier::BOLD);
    cells
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut spans = Vec::new();
            for (column, cell) in row.into_iter().enumerate() {
                let padding = widths[column] + 2 - width(&cell);
                spans.extend(cell);
                spans.push(Span::raw(" ".repeat(padding)));
            }
            let line = Line::from(spans);
            if index == 0 { line.style(bold) } else { line }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What fits in the window, inside its border, on an 80-column terminal.
    const ROOM: usize = 76;

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn every_guide_has_a_title_and_fits_the_window() {
        for guide in GUIDES {
            assert_ne!(guide.title(), "Guide");
            for line in guide.lines() {
                let shown = text(&line);
                assert!(
                    shown.trim_end().chars().count() <= ROOM,
                    "{}: too wide: {shown}",
                    guide.title()
                );
            }
        }
    }

    #[test]
    fn markers_never_reach_the_screen() {
        for guide in GUIDES {
            for line in guide.lines() {
                let shown = text(&line);
                assert!(!shown.contains('`'), "{}: {shown}", guide.title());
                assert!(!shown.contains("**"), "{}: {shown}", guide.title());
            }
        }
    }

    #[test]
    fn tables_line_up_and_lose_their_rule() {
        let guide = Guide {
            source: "| Key | Does |\n|---|---|\n| `j` | Down |\n| `Space x` | Pause |\n",
        };
        let lines: Vec<String> = guide.lines().iter().map(text).collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("Key      Does"));
        assert!(lines[1].starts_with("j        Down"));
        assert!(lines[2].starts_with("Space x  Pause"));
    }
}
