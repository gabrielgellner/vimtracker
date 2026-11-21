//! The topic guides: markdown files in `guides/`, built into the program and opened with
//! `Space ?`. Only the markdown they use is understood: headings, tables, fences, `code` and bold.
//! A line that's an HTML comment, as the anchors the manual includes a guide in pieces by, isn't
//! shown.

pub(crate) mod draw;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::controls::Page;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::overlays::Overlay;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::widgets::theme;

/// The guide that's open, if one is, how far it has scrolled, and how many of its rows fit and
/// how wide its text is, as the screen was when last drawn, and the words a search of the guides
/// opened it on, which it marks and `n` and `N` go between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideView {
    pub open: Option<Guide>,
    pub scroll: usize,
    pub page: usize,
    pub width: usize,
    pub found: Vec<String>,
}

impl Default for GuideView {
    fn default() -> Self {
        Self {
            open: None,
            scroll: 0,
            page: 16,
            width: 76,
            found: Vec::new(),
        }
    }
}

/// A key of an open guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideKey {
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
    /// `?`: the list of guides, on this one.
    Guides,
    /// `/`: search the text of every guide.
    Search,
    /// `n` and `N`: the next or the last passage the search found in this guide, going round.
    Next,
    Previous,
    Close,
}

impl GuideView {
    /// Open a guide at its top.
    pub(crate) fn show(&mut self, guide: Guide) {
        self.open = Some(guide);
        self.scroll = 0;
        self.found.clear();
    }

    /// Open a guide at a passage a search found, by its place among the guide's passages, which
    /// is the same at any width, marking the words it was for.
    pub(crate) fn show_found(&mut self, guide: Guide, passage: usize, words: &str) {
        self.show(guide);
        self.found = search_words(words);
        let line = guide
            .passages(self.width)
            .get(passage)
            .map_or(0, |passage| passage.line);
        let last = guide.lines(self.width).len().saturating_sub(self.page);
        self.scroll = line.min(last);
    }

    /// Scroll to the next passage the search found after the top line, or the last before it,
    /// going round past either end, and say where it went.
    fn step_found(&mut self, guide: Guide, forward: bool) -> &'static str {
        if self.found.is_empty() {
            return "No search: / searches every guide";
        }
        let lines: Vec<usize> = guide
            .passages(self.width)
            .iter()
            .filter(|passage| passage.has(&self.found))
            .map(|passage| passage.line)
            .collect();
        let next = if forward {
            lines.iter().find(|&&line| line > self.scroll)
        } else {
            lines.iter().rev().find(|&&line| line < self.scroll)
        };
        let (line, said) = match next {
            Some(line) => (*line, ""),
            None if forward => match lines.first() {
                Some(line) => (*line, "Search went round to the top"),
                None => return "Not in this guide",
            },
            None => match lines.last() {
                Some(line) => (*line, "Search went round to the bottom"),
                None => return "Not in this guide",
            },
        };
        let last = guide.lines(self.width).len().saturating_sub(self.page);
        self.scroll = line.min(last);
        said
    }

    /// Open a guide at a heading, or at its top if it has none by that title. Whether it had one.
    pub(crate) fn show_at(&mut self, guide: Guide, heading: &str) -> bool {
        self.show(guide);
        let lines = guide.lines(self.width);
        let Some(at) = lines.iter().position(|line| {
            line.style == self::heading(theme::BLUE)
                && line.spans.iter().map(|span| &*span.content).eq([heading])
        }) else {
            return false;
        };
        self.scroll = at.min(lines.len().saturating_sub(self.page));
        true
    }
}

impl Overlay for GuideView {
    const MODE: Mode = Mode::Guide;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::InGuide(GuideKey::Down), "Scroll down"),
        ("Down", Cmd::InGuide(GuideKey::Down), "Scroll down"),
        ("k", Cmd::InGuide(GuideKey::Up), "Scroll up"),
        ("Up", Cmd::InGuide(GuideKey::Up), "Scroll up"),
        (
            "C-d",
            Cmd::InGuide(GuideKey::HalfPageDown),
            "Half a page down",
        ),
        ("C-u", Cmd::InGuide(GuideKey::HalfPageUp), "Half a page up"),
        ("g", Cmd::InGuide(GuideKey::Top), "The top"),
        ("G", Cmd::InGuide(GuideKey::Bottom), "The bottom"),
        ("?", Cmd::InGuide(GuideKey::Guides), "The list of guides"),
        ("/", Cmd::InGuide(GuideKey::Search), "Search every guide"),
        ("n", Cmd::InGuide(GuideKey::Next), "The next found"),
        ("N", Cmd::InGuide(GuideKey::Previous), "The last found"),
        ("q", Cmd::InGuide(GuideKey::Close), "Close"),
        ("Esc", Cmd::InGuide(GuideKey::Close), "Close"),
    ];

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Some(guide) = self.open else {
            return Outcome::Close;
        };
        let Cmd::InGuide(key) = cmd else {
            return Outcome::Passed;
        };
        // It stops when its last row reaches the bottom of the window.
        let last = guide.lines(self.width).len().saturating_sub(self.page);
        let half = (self.page / 2).max(1);
        self.scroll = match key {
            GuideKey::HalfPageDown => self.scroll + half,
            GuideKey::HalfPageUp => self.scroll.saturating_sub(half),
            GuideKey::Down => self.scroll + 1,
            GuideKey::Up => self.scroll.saturating_sub(1),
            GuideKey::Top => 0,
            GuideKey::Bottom => last,
            GuideKey::Guides => {
                self.open = None;
                return Outcome::Open(Box::new(picker(guide)));
            }
            GuideKey::Search => {
                self.open = None;
                return Outcome::Open(Box::new(search(self.width)));
            }
            GuideKey::Next | GuideKey::Previous => {
                let said = self.step_found(guide, key == GuideKey::Next);
                ctx.services.status = said.to_owned();
                return Outcome::Done;
            }
            GuideKey::Close => return Outcome::Close,
        }
        .min(last);
        Outcome::Done
    }

    /// Only `Ctrl` with `d` or `u` is read with what's held: the rest mean what they do alone.
    fn reads(&self, key: Key) -> Key {
        use ratatui::crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('d' | 'u') if key.ctrl => Key {
                ctrl: true,
                ..Key::new(key.code)
            },
            code => Key::new(code),
        }
    }

    /// Any other key does nothing, but for keeping the scroll within the guide.
    fn text(&mut self, _key: Key, _ctx: &mut CtxMut) -> Outcome {
        let Some(guide) = self.open else {
            return Outcome::Close;
        };
        let last = guide.lines(self.width).len().saturating_sub(self.page);
        self.scroll = self.scroll.min(last);
        Outcome::Done
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _ctx: &Ctx) {
        self.page = draw::page(area.height);
        self.width = draw::text_width(area.width);
        if let Some(guide) = self.open {
            draw::draw(frame, area, guide, self.scroll, &self.found);
        }
    }
}

/// One guide. Its title is its first heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub source: &'static str,
}

const FIRST_LOOP: Guide = Guide {
    source: include_str!("../../guides/first-loop.md"),
};
const NAVIGATION: Guide = Guide {
    source: include_str!("../../guides/navigation.md"),
};
const NOTE_ENTRY: Guide = Guide {
    source: include_str!("../../guides/note-entry.md"),
};
const EDITING: Guide = Guide {
    source: include_str!("../../guides/editing.md"),
};
const PATTERNS: Guide = Guide {
    source: include_str!("../../guides/patterns.md"),
};
const FX: Guide = Guide {
    source: include_str!("../../guides/fx.md"),
};
const CHAINS: Guide = Guide {
    source: include_str!("../../guides/chains.md"),
};
const SOUNDS: Guide = Guide {
    source: include_str!("../../guides/sounds.md"),
};
const SAMPLE_EDITOR: Guide = Guide {
    source: include_str!("../../guides/sample-editor.md"),
};
const TRACK_SOUND: Guide = Guide {
    source: include_str!("../../guides/track-sound.md"),
};
const TRACK_MOD: Guide = Guide {
    source: include_str!("../../guides/track-mod.md"),
};
const TRACK_FX: Guide = Guide {
    source: include_str!("../../guides/track-fx.md"),
};
const TRACK_GEN: Guide = Guide {
    source: include_str!("../../guides/track-gen.md"),
};
const GENERATORS: Guide = Guide {
    source: include_str!("../../guides/generators.md"),
};
const SAVING: Guide = Guide {
    source: include_str!("../../guides/saving.md"),
};
const METER: Guide = Guide {
    source: include_str!("../../guides/meter.md"),
};

/// Every guide, in the order to learn them.
pub(crate) const GUIDES: [Guide; 16] = [
    FIRST_LOOP,
    NAVIGATION,
    NOTE_ENTRY,
    EDITING,
    PATTERNS,
    FX,
    CHAINS,
    SOUNDS,
    SAMPLE_EDITOR,
    TRACK_SOUND,
    TRACK_MOD,
    TRACK_FX,
    TRACK_GEN,
    GENERATORS,
    SAVING,
    METER,
];

/// The list of guides, in the order to learn them, which `Space ?` opens on `on`.
pub(crate) fn picker(on: Guide) -> Picker {
    let items = GUIDES
        .iter()
        .map(|guide| Item::new(guide.title(), Pick::Guide(*guide)))
        .collect();
    Picker::new("Guides", items).revealing(GUIDES.iter().position(|guide| *guide == on))
}

/// A search of the text of every guide, at the width their text is shown: a row for each
/// sentence, table row, heading and line of a fence, under its guide, its heading beside it.
/// Each word typed is found exactly, whatever its case, in any order, as `grep` would find them
/// all, and choosing one opens its guide there with the words marked, which `n` and `N` go
/// between.
pub(crate) fn search(width: usize) -> Picker {
    let items = GUIDES
        .iter()
        .flat_map(|guide| {
            guide
                .passages(width)
                .into_iter()
                .enumerate()
                .map(|(at, passage)| {
                    Item::new(
                        passage.text,
                        Pick::GuideAt {
                            guide: *guide,
                            passage: at,
                            words: String::new(),
                        },
                    )
                    .meaning(passage.heading)
                    .category(guide.title())
                })
        })
        .collect();
    Picker::new("Search guides", items).exact().searching("")
}

/// The words of a search, as a passage is checked for them.
fn search_words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|word| word.trim_start_matches('\'').to_ascii_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

/// The guide for the shape panel's fill, and the heading to open it at: the fill's own section
/// in **Generators in depth**, or for a fill without one **Shaping a column**. Whether it had one.
pub(crate) fn for_fill(fill: &str) -> (Guide, &str, bool) {
    if GENERATORS.has_heading(fill) {
        (GENERATORS, fill, true)
    } else {
        (EDITING, "Shaping a column", false)
    }
}

/// The guide for where the program is: the track page's own for each of its pages, the one
/// with the mixer, the project page or the sample page in it, on the phrase view the moves, and
/// on Gen the guide the Macros page shares with it.
pub(crate) fn for_view(view: View, page: Page) -> Guide {
    match view {
        View::Phrase => NAVIGATION,
        View::Gen => TRACK_GEN,
        View::Chain | View::Song => CHAINS,
        View::Track => match page {
            Page::Sound => TRACK_SOUND,
            Page::Mod => TRACK_MOD,
            Page::Fx => TRACK_FX,
            Page::Macros | Page::Gen => TRACK_GEN,
        },
        View::Mixer => TRACK_SOUND,
        View::Audio => METER,
        View::Project => SAVING,
        View::Sample => SAMPLE_EDITOR,
    }
}

impl Guide {
    /// Whether a `##` or `###` heading has this title.
    fn has_heading(self, title: &str) -> bool {
        self.source.lines().any(|line| {
            line.strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
                .is_some_and(|heading| heading == title)
        })
    }

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
        self.layout(width).0
    }

    /// What a search of the guides looks through, at a width: each sentence, table row, heading
    /// and line of a fence, with the line it starts on.
    pub(crate) fn passages(self, width: usize) -> Vec<Passage> {
        self.layout(width).1
    }

    /// The lines, and the passages in them.
    fn layout(self, width: usize) -> (Vec<Line<'static>>, Vec<Passage>) {
        let mut laid = Laid {
            lines: Vec::new(),
            passages: Vec::new(),
            heading: self.title(),
        };
        let mut table: Vec<&str> = Vec::new();
        let mut paragraph: Vec<&str> = Vec::new();
        let mut fenced = false;
        for line in self.source.lines() {
            if !fenced && line.starts_with("<!--") {
                continue;
            }
            if !fenced && line.starts_with('|') {
                table.push(line);
                continue;
            }
            laid.table(&table);
            table.clear();
            let prose =
                !fenced && !line.is_empty() && !line.starts_with("```") && !line.starts_with('#');
            if prose {
                paragraph.push(line);
                continue;
            }
            laid.paragraph(&paragraph.join(" "), width);
            paragraph.clear();
            if line.starts_with("```") {
                fenced = !fenced;
            } else if fenced {
                laid.add(line, Line::styled(line.to_owned(), code()));
            } else if let Some(title) = line.strip_prefix("# ") {
                laid.add(title, Line::styled(title.to_owned(), heading(theme::MAUVE)));
            } else if let Some(title) = line
                .strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
            {
                laid.heading = title;
                laid.add(title, Line::styled(title.to_owned(), heading(theme::BLUE)));
            } else {
                laid.lines.push(Line::raw(""));
            }
        }
        laid.table(&table);
        laid.paragraph(&paragraph.join(" "), width);
        (laid.lines, laid.passages)
    }
}

/// A piece of a guide a search finds: a sentence, a table's row, a heading or a line of a fence,
/// as it reads on screen, under the heading it's in, starting on this line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Passage {
    pub text: String,
    pub heading: &'static str,
    pub line: usize,
}

impl Passage {
    /// Whether every word is in it, whatever their case, in any order.
    pub(crate) fn has(&self, words: &[String]) -> bool {
        let text = self.text.to_ascii_lowercase();
        !words.is_empty() && words.iter().all(|word| text.contains(word.as_str()))
    }
}

/// A guide being laid out: its lines so far, the passages in them, and the heading they're under.
struct Laid {
    lines: Vec<Line<'static>>,
    passages: Vec<Passage>,
    heading: &'static str,
}

impl Laid {
    /// A line that's a passage of its own.
    fn add(&mut self, text: &str, line: Line<'static>) {
        self.passage(text.trim(), self.lines.len());
        self.lines.push(line);
    }

    fn passage(&mut self, text: &str, line: usize) {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            self.passages.push(Passage {
                text,
                heading: self.heading,
                line,
            });
        }
    }

    /// A table, a passage a row.
    fn table(&mut self, rows: &[&str]) {
        for line in table_lines(rows) {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            self.add(&text, line);
        }
    }

    /// A paragraph, wrapped, a passage a sentence, each starting on the line its first word is.
    fn paragraph(&mut self, text: &str, width: usize) {
        let start = self.lines.len();
        let lines = wrapped(text, width);
        let shown: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        self.lines.extend(lines);
        // Where each line starts in the paragraph as shown, its lines joined by a space.
        let mut starts = Vec::with_capacity(shown.len());
        let mut at = 0;
        for line in &shown {
            starts.push(at);
            at += line.len() + 1;
        }
        let whole = shown.join(" ");
        // The line a sentence starting here is on, past the space before it.
        let line_of = |from: usize| {
            let begins = from + whole[from..].len() - whole[from..].trim_start().len();
            start + starts.partition_point(|&at| at <= begins).max(1) - 1
        };
        let mut from = 0;
        for (end, _) in whole.match_indices(['.', '?', '!']) {
            let after = &whole[end + 1..];
            if after.is_empty() || after.starts_with(' ') {
                self.passage(&whole[from..=end], line_of(from));
                from = end + 1;
            }
        }
        if !whole[from..].trim().is_empty() {
            self.passage(&whole[from..], line_of(from));
        }
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
    fn every_passage_starts_on_its_line_and_together_they_keep_every_word() {
        for guide in GUIDES {
            for width in [ROOM, 96] {
                let lines = guide.lines(width);
                let passages = guide.passages(width);
                for passage in &passages {
                    let first = passage.text.split_whitespace().next().unwrap();
                    assert!(
                        text(&lines[passage.line]).contains(first),
                        "{}: {:?} isn't on line {}",
                        guide.title(),
                        passage.text,
                        passage.line
                    );
                }
                let shown: Vec<String> = lines
                    .iter()
                    .flat_map(|line| {
                        text(line)
                            .split_whitespace()
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let found: Vec<String> = passages
                    .iter()
                    .flat_map(|passage| passage.text.split_whitespace().map(str::to_owned))
                    .collect();
                assert_eq!(found, shown, "{}", guide.title());
            }
        }
    }

    #[test]
    fn a_paragraph_is_a_passage_a_sentence_and_a_number_is_no_end() {
        let guide = Guide {
            source: "# T\n\nOne is `1.5` long. Is two? Three!\n",
        };
        let texts: Vec<String> = guide.passages(ROOM).into_iter().map(|p| p.text).collect();
        assert_eq!(texts, ["T", "One is 1.5 long.", "Is two?", "Three!"]);
    }

    #[test]
    fn a_search_marks_its_words_whatever_their_case_and_leaves_the_rest() {
        let line = Line::from(vec![Span::raw("A Loop and "), Span::styled("loop", code())]);
        let marked = draw::marked(line, &["loop".to_owned()]);
        let reversed: Vec<&str> = marked
            .spans
            .iter()
            .filter(|span| span.style.add_modifier.contains(Modifier::REVERSED))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(reversed, ["Loop", "loop"]);
        assert_eq!(text(&marked), "A Loop and loop");
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
            // A code span ending in a space, as `:preset save `, leaves a lone mark, no word.
            text.split_whitespace()
                .map(|word| word.replace(['`', '*'], ""))
                .filter(|word| !word.is_empty())
                .collect()
        };
        for guide in GUIDES {
            let source: Vec<String> = guide
                .source
                .lines()
                .filter(|line| {
                    !line.starts_with('|') && !line.starts_with("```") && !line.starts_with("<!--")
                })
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
                assert!(!shown.contains("<!--"), "{}: {shown}", guide.title());
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
            // The guides write `gg` and `Space fb` for `g g` and `Space f b`, and name the
            // modifiers as they're shown, `Cmd-l` for `D-l`.
            let keys = crate::input::keys::shown(binding.keys);
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
        for command in crate::overlays::command::parse::COMMANDS {
            let named = format!("`:{}", command.name);
            let found = guides
                .match_indices(&named)
                .any(|(at, _)| guides[at + named.len()..].starts_with(['`', ' ', '=']));
            assert!(found, ":{} isn't in a guide", command.name);
        }
    }

    #[test]
    fn every_effect_its_types_and_its_settings_are_in_the_fx_page_guide() {
        let effects = TRACK_FX.source;
        assert_eq!(TRACK_FX.title(), "The FX page and its effects");
        let mut missing = Vec::new();
        for effect in vt_model::insert::Insert::ALL {
            // An empty slot says `Empty` on the FX page, not its name `---`.
            if effect == vt_model::insert::Insert::Empty {
                continue;
            }
            if !names(effects, effect.name()) {
                missing.push(effect.name().to_owned());
            }
            // Each type or mode has settings of its own, so every one is checked.
            let kinds: Vec<_> = match effect.type_param() {
                Some(at) => {
                    let param = effect.params()[at];
                    (0..param.choices().len())
                        .map(|choice| effect.with_value(at, choice as u8))
                        .collect()
                }
                None => vec![effect],
            };
            for kind in kinds {
                // The type is named by what it is, as `Glue`, and its settings by theirs.
                let type_name = kind
                    .type_param()
                    .map(|at| kind.params()[at].choices()[usize::from(kind.value(at))]);
                let mut named: Vec<&str> = kind.params().iter().map(|param| param.name).collect();
                if let (Some(at), Some(type_name)) = (kind.type_param(), type_name) {
                    named[at] = type_name;
                }
                for name in named {
                    if !names(effects, name) {
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
        let sounds = SOUNDS.source;
        assert_eq!(SOUNDS.title(), "Sounds, samples and kits");
        for machine in vt_model::song::Machine::ALL {
            assert!(names(sounds, machine.name()), "{}", machine.name());
            for setting in machine.settings() {
                // An operator's settings end in its number, which the guide writes as `N`.
                let name = setting.name();
                let name = match name.rsplit_once(' ') {
                    Some((base, n)) if n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()) => {
                        format!("{base} N")
                    }
                    _ => name.to_owned(),
                };
                assert!(
                    names(sounds, &name),
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
