//! The picker's state and matching. `rank` is adapted from gmuse's `src/ui/picker.rs`.

use std::path::PathBuf;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use vt_model::envelope::Envelope;
use vt_model::lfo::Lfo;
use vt_model::shape::{FillKind, Where};
use vt_model::song::FilterType;
use vt_model::step::FxCommand;

use crate::guides::Guide;
use crate::input::mode::View;

/// What choosing a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    View(View),
    Sample(PathBuf),
    Kit(PathBuf),
    /// Put this command in an FX slot, or empty it.
    FxCommand {
        slot: usize,
        command: Option<FxCommand>,
    },
    /// Replace one of a track's LFOs.
    Lfo {
        track: usize,
        slot: usize,
        lfo: Lfo,
    },
    /// Choose a track's filter.
    FilterType {
        track: usize,
        filter: FilterType,
    },
    /// Replace one of a track's envelopes.
    Envelope {
        track: usize,
        slot: usize,
        envelope: Envelope,
    },
    /// Play this phrase on a row of the chain.
    RowPhrase(u32),
    /// What the shape panel fills with.
    ShapeFill(FillKind),
    /// Which rows the shape panel fills.
    ShapeWhere(Where),
    Use {
        section: usize,
        track: usize,
        slot: usize,
    },
    Guide(Guide),
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub pick: Pick,
}

/// An open picker.
#[derive(Debug, Clone)]
pub struct Picker {
    pub title: &'static str,
    pub query: String,
    pub selected: usize,
    items: Vec<Item>,
    shown: Vec<usize>,
}

impl Picker {
    /// A picker over these rows, already in the order an empty search shows them.
    pub fn new(title: &'static str, items: Vec<Item>) -> Self {
        let shown = (0..items.len()).collect();
        Self {
            title,
            query: String::new(),
            selected: 0,
            items,
            shown,
        }
    }

    /// The rows matching the search, best first.
    pub fn shown(&self) -> impl Iterator<Item = &Item> {
        self.shown.iter().map(|index| &self.items[*index])
    }

    /// The highlighted row.
    pub fn current(&self) -> Option<&Item> {
        self.items.get(*self.shown.get(self.selected)?)
    }

    /// Move the highlight, stopping at the ends.
    pub fn move_highlight(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
    }

    /// Change the search and rank again.
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        let labels: Vec<&str> = self.items.iter().map(|i| i.label.as_str()).collect();
        self.shown = rank(&labels, &self.query);
        self.selected = 0;
    }
}

/// The positions of the labels that match, best first. The order they came in breaks ties, so a
/// list in order of recent use keeps that order, and an empty search keeps every label.
pub fn rank(labels: &[&str], needle: &str) -> Vec<usize> {
    if needle.trim().is_empty() {
        return (0..labels.len()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(needle.trim(), CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut scored: Vec<(u32, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(at, label)| {
            let haystack = Utf32Str::new(label, &mut buffer);
            pattern.score(haystack, &mut matcher).map(|s| (s, at))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, at)| at).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWS: [&str; 6] = ["table", "song", "chain", "phrase", "instrument", "groove"];

    #[test]
    fn letters_match_in_order_only() {
        let found = rank(&VIEWS, "sg");
        assert_eq!(
            found,
            [1],
            "sg is in song, but groove has them the wrong way round"
        );
    }

    #[test]
    fn an_empty_search_keeps_the_given_order() {
        assert_eq!(rank(&VIEWS, " "), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn ties_go_to_the_earlier_row() {
        assert_eq!(rank(&["ab one", "ab two"], "ab"), [0, 1]);
        assert_eq!(rank(&["ab two", "ab one"], "ab"), [0, 1]);
    }

    #[test]
    fn the_highlight_stays_in_the_list() {
        let items = VIEWS.map(|label| Item {
            label: label.to_owned(),
            pick: Pick::View(View::Phrase),
        });
        let mut picker = Picker::new("Views", items.to_vec());
        picker.move_highlight(-1);
        assert_eq!(picker.selected, 0);
        picker.move_highlight(100);
        assert_eq!(picker.selected, 5);
        picker.set_query("sg".to_owned());
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.current().map(|i| i.label.as_str()), Some("song"));
        picker.set_query("zzz".to_owned());
        assert!(picker.current().is_none());
    }
}
