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
pub const GUIDES: [Guide; 11] = [
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
        source: include_str!("../guides/sounds.md"),
    },
    Guide {
        source: include_str!("../guides/sample-editor.md"),
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

    /// The guide as lines on screen, with paragraphs wrapped to a width. Tables and fences aren't
    /// wrapped, so they need to fit the narrowest window.
    pub fn lines(self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut table: Vec<&str> = Vec::new();
        let mut paragraph: Vec<&str> = Vec::new();
        let mut fenced = false;
        for line in self.source.lines() {
            if !fenced && line.starts_with('|') {
                table.push(line);
                continue;
            }
            lines.append(&mut table_lines(&table));
            table.clear();
            let prose =
                !fenced && !line.is_empty() && !line.starts_with("```") && !line.starts_with('#');
            if prose {
                paragraph.push(line);
                continue;
            }
            lines.append(&mut wrapped(&paragraph.join(" "), width));
            paragraph.clear();
            if line.starts_with("```") {
                fenced = !fenced;
            } else if fenced {
                lines.push(Line::styled(line.to_owned(), code()));
            } else if let Some(title) = line.strip_prefix("# ") {
                lines.push(Line::styled(title.to_owned(), heading(theme::MAUVE)));
            } else if let Some(title) = line.strip_prefix("## ") {
                lines.push(Line::styled(title.to_owned(), heading(theme::BLUE)));
            } else {
                lines.push(Line::raw(""));
            }
        }
        lines.append(&mut table_lines(&table));
        lines.append(&mut wrapped(&paragraph.join(" "), width));
        lines
    }
}

/// A paragraph as lines up to a width, breaking between words. A piece of `code` stays whole, and
/// punctuation stays with what it follows.
fn wrapped(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut words: Vec<Vec<Span<'static>>> = Vec::new();
    let mut joined = false;
    for span in inline(text) {
        let pieces: Vec<&str> = if span.style == code() {
            vec![span.content.as_ref()]
        } else {
            span.content.split_inclusive(' ').collect()
        };
        for piece in pieces {
            let word = piece.trim_end_matches(' ');
            if !word.is_empty() {
                let piece = Span::styled(word.to_owned(), span.style);
                match words.last_mut() {
                    Some(last) if joined => last.push(piece),
                    _ => words.push(vec![piece]),
                }
            }
            joined = word.len() == piece.len();
        }
    }
    let mut lines = Vec::new();
    let mut line: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for word in words {
        let length: usize = word.iter().map(Span::width).sum();
        if used > 0 && used + 1 + length > width {
            lines.push(Line::from(std::mem::take(&mut line)));
            used = 0;
        }
        if used > 0 {
            line.push(Span::raw(" "));
            used += 1;
        }
        line.extend(word);
        used += length;
    }
    if !line.is_empty() {
        lines.push(Line::from(line));
    }
    lines
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
            for line in guide.lines(ROOM) {
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
    fn paragraphs_wrap_to_the_window_and_lose_no_words() {
        let words = |text: &str| -> Vec<String> {
            text.split_whitespace()
                .map(|word| word.replace(['`', '*'], ""))
                .collect()
        };
        for guide in GUIDES {
            let source: Vec<String> = guide
                .source
                .lines()
                .filter(|line| !line.starts_with('|') && !line.starts_with("```"))
                .flat_map(|line| words(line.trim_start_matches('#')))
                .collect();
            for width in [ROOM, 96] {
                let shown: Vec<String> = guide
                    .lines(width)
                    .iter()
                    .flat_map(|line| words(&text(line)))
                    .collect();
                for word in &source {
                    assert!(shown.contains(word), "{}: lost {word}", guide.title());
                }
            }
        }
    }

    #[test]
    fn a_wide_window_fills_its_lines_and_keeps_breaks_where_they_belong() {
        let guide = Guide {
            source: "# T\n\nOne two\nthree `four five` six.\n\nNext.\n\n```\nkeep\n  this\n```\n",
        };
        let at = |width| -> Vec<String> { guide.lines(width).iter().map(text).collect() };
        assert_eq!(
            at(40),
            [
                "T",
                "",
                "One two three four five six.",
                "",
                "Next.",
                "",
                "keep",
                "  this"
            ]
        );
        // Code stays whole, however it falls.
        assert_eq!(
            at(16),
            [
                "T",
                "",
                "One two three",
                "four five six.",
                "",
                "Next.",
                "",
                "keep",
                "  this"
            ]
        );
    }

    #[test]
    fn markers_never_reach_the_screen() {
        for guide in GUIDES {
            for line in guide.lines(ROOM) {
                let shown = text(&line);
                assert!(!shown.contains('`'), "{}: {shown}", guide.title());
                assert!(!shown.contains("**"), "{}: {shown}", guide.title());
            }
        }
    }

    /// Every guide's text, as one.
    fn every_guide() -> String {
        GUIDES.map(|guide| guide.source).join("\n")
    }

    /// Whether a guide names this as `code` of its own, as `Comp` and not `Compressor`.
    fn names(guide: &str, name: &str) -> bool {
        guide.contains(&format!("`{name}`"))
    }

    #[test]
    fn every_key_sequence_is_in_a_guide() {
        let guides = every_guide();
        let mut missing = Vec::new();
        for binding in crate::input::bindings::bindings() {
            // The guides write `gg` and `Space fb` for `g g` and `Space f b`, and name Option and
            // Shift in full.
            let keys = binding
                .keys
                .replace("M-", "Option-")
                .replace("S-Tab", "Shift-Tab");
            let joined = match keys.split_once(' ') {
                Some(("Space", rest)) => format!("Space {}", rest.replace(' ', "")),
                _ => keys.replace(' ', ""),
            };
            let found = [&keys, &joined].iter().any(|spelled| {
                [
                    format!("`{spelled}`"),
                    format!("`{spelled} "),
                    format!(" {spelled}`"),
                ]
                .iter()
                .any(|written| guides.contains(written))
            });
            if !found && !missing.contains(&binding.keys) {
                missing.push(binding.keys);
            }
        }
        assert!(missing.is_empty(), "not in a guide: {missing:?}");
    }

    #[test]
    fn every_command_is_in_a_guide() {
        let guides = every_guide();
        for command in crate::input::command::COMMANDS {
            let named = format!("`:{}", command.name);
            let found = guides
                .match_indices(&named)
                .any(|(at, _)| guides[at + named.len()..].starts_with(['`', ' ', '=']));
            assert!(found, ":{} isn't in a guide", command.name);
        }
    }

    #[test]
    fn every_effect_its_types_and_its_settings_are_in_the_track_page_guide() {
        let tracks = GUIDES[8].source;
        assert_eq!(GUIDES[8].title(), "The track page, envelopes and LFOs");
        let mut missing = Vec::new();
        for effect in vt_model::insert::Insert::ALL {
            if !names(tracks, effect.name()) {
                missing.push(effect.name().to_owned());
            }
            // Each type or mode has settings of its own, so every one is checked.
            let kinds: Vec<_> = match effect.type_param() {
                Some(at) => {
                    let param = effect.params()[at];
                    (0..param.choices.len())
                        .map(|choice| effect.write(param.setting, choice as f32))
                        .collect()
                }
                None => vec![effect],
            };
            for kind in kinds {
                // The type is named by what it is, as `Glue`, and its settings by theirs.
                let type_name = kind
                    .type_param()
                    .map(|at| kind.params()[at].choices[usize::from(kind.value(at))]);
                let mut named: Vec<&str> = kind.params().iter().map(|param| param.name).collect();
                if let (Some(at), Some(type_name)) = (kind.type_param(), type_name) {
                    named[at] = type_name;
                }
                for name in named {
                    if !names(tracks, name) {
                        let kind = type_name.unwrap_or(effect.name());
                        missing.push(format!("{} {kind}: {name}", effect.name()));
                    }
                }
            }
        }
        missing.dedup();
        assert!(missing.is_empty(), "not in the guide: {missing:?}");
    }

    #[test]
    fn every_machine_and_its_settings_are_in_the_sounds_guide() {
        let sounds = GUIDES[6].source;
        assert_eq!(GUIDES[6].title(), "Sounds, samples and kits");
        for machine in vt_model::song::Machine::ALL {
            assert!(names(sounds, machine.name()), "{}", machine.name());
            for setting in machine.settings() {
                assert!(
                    names(sounds, setting.name()),
                    "{}'s {}",
                    machine.name(),
                    setting.name()
                );
            }
        }
    }

    #[test]
    fn every_fx_command_a_new_track_has_is_in_a_guide() {
        let guides = every_guide();
        let track = vt_model::song::Track::default();
        for command in vt_model::step::FxCommand::on_track(&track) {
            assert!(names(&guides, &command.name()), "{}", command.name());
        }
    }

    #[test]
    fn tables_line_up_and_lose_their_rule() {
        let guide = Guide {
            source: "| Key | Does |\n|---|---|\n| `j` | Down |\n| `Space x` | Pause |\n",
        };
        let lines: Vec<String> = guide.lines(ROOM).iter().map(text).collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("Key      Does"));
        assert!(lines[1].starts_with("j        Down"));
        assert!(lines[2].starts_with("Space x  Pause"));
    }
}
