//! The picker's state and matching. `rank` is adapted from gmuse's `src/ui/picker.rs`.

use std::path::PathBuf;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use vt_model::envelope::Envelope;
use vt_model::insert::Insert;
use vt_model::lfo::{Lfo, Rate, Shape as Wave};
use vt_model::scale::ScaleKind;
use vt_model::shape::{FillKind, Where};
use vt_model::song::{FilterType, Machine, SynthVoice};
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
    /// A sound from the current kit, for this track, and the name the track takes.
    KitSound {
        path: PathBuf,
        name: String,
    },
    /// Go to this track.
    Track(usize),
    /// What makes a track's sound.
    Machine {
        track: usize,
        machine: Machine,
    },
    /// Which synth a track plays.
    SynthVoice {
        track: usize,
        voice: SynthVoice,
    },
    /// Play this phrase on a row of the chain.
    RowPhrase(u32),
    /// A chain, or none, for the song view cell under the cursor.
    SectionChain(Option<u32>),
    /// What goes in one of a track's slots.
    Insert {
        track: usize,
        slot: usize,
        insert: Insert,
    },
    /// The kind of scale.
    Scale(ScaleKind),
    /// What the shape panel fills with.
    ShapeFill(FillKind),
    /// The wave the shape panel follows.
    ShapeWave(Wave),
    /// The length of the shape panel's wave.
    ShapeRate(Rate),
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
    /// Where keys go. A plain list is only ever searched.
    pub focus: Focus,
    /// The names of the groups, if it has any, in the order they first appear.
    groups: Vec<String>,
    /// Which group each item is in.
    item_groups: Vec<usize>,
    /// The group being browsed.
    pub group: usize,
}

/// Which part of a picker with groups keys go to, after the snacks explorer: `/` goes to the search
/// and back, and `h` and `l` between the groups and the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Search,
    List,
    Groups,
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
            focus: Focus::Search,
            groups: Vec::new(),
            item_groups: Vec::new(),
            group: 0,
        }
    }

    /// A picker that can be browsed by group as well as searched, opening on the list at this row.
    /// `groups` has each item's group, and a group's items should be next to each other.
    pub fn grouped(
        title: &'static str,
        items: Vec<Item>,
        groups: Vec<String>,
        current: usize,
    ) -> Self {
        let mut names: Vec<String> = Vec::new();
        let item_groups = groups
            .into_iter()
            .map(|group| {
                names
                    .iter()
                    .position(|name| *name == group)
                    .unwrap_or_else(|| {
                        names.push(group);
                        names.len() - 1
                    })
            })
            .collect();
        let mut picker = Self {
            focus: Focus::List,
            groups: names,
            item_groups,
            ..Self::new(title, items)
        };
        picker.browse_at(current);
        picker
    }

    /// Whether it has groups to browse.
    pub fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Show an item's group, with it highlighted.
    fn browse_at(&mut self, item: usize) {
        self.group = self.item_groups.get(item).copied().unwrap_or(0);
        self.show_group();
        self.selected = self.shown.iter().position(|at| *at == item).unwrap_or(0);
    }

    fn show_group(&mut self) {
        let group = self.group;
        self.shown = (0..self.items.len())
            .filter(|item| self.item_groups[*item] == group)
            .collect();
        self.selected = 0;
    }

    /// Choose another group, stopping at the ends.
    pub fn move_group(&mut self, by: isize) {
        let last = self.groups.len().saturating_sub(1);
        self.group = self.group.saturating_add_signed(by).min(last);
        self.show_group();
    }

    /// How many items are in the group being browsed.
    pub fn in_group(&self) -> usize {
        self.shown.len()
    }

    /// What the groups' column shows: a group's name, or a lone item's own name, so it doesn't take
    /// opening to find.
    pub fn group_names(&self) -> Vec<String> {
        (0..self.groups.len())
            .map(|group| {
                let mut members =
                    (0..self.items.len()).filter(|item| self.item_groups[*item] == group);
                match (members.next(), members.next()) {
                    (Some(only), None) => self.items[only]
                        .label
                        .split("  ")
                        .next()
                        .unwrap_or("")
                        .to_owned(),
                    _ => self.groups[group].clone(),
                }
            })
            .collect()
    }

    /// Go to the search, or back to the list on what was highlighted there.
    pub fn toggle_search(&mut self) {
        if self.focus == Focus::Search {
            let item = self.shown.get(self.selected).copied().unwrap_or(0);
            self.query.clear();
            self.focus = Focus::List;
            self.browse_at(item);
        } else {
            self.focus = Focus::Search;
            let item = self.shown.get(self.selected).copied();
            self.set_query(String::new());
            self.selected = item
                .and_then(|item| self.shown.iter().position(|at| *at == item))
                .unwrap_or(0);
        }
    }

    /// An item's text, with its group while searching across them all.
    pub fn text_of(&self, shown: usize) -> String {
        let Some(item) = self.shown.get(shown).copied() else {
            return String::new();
        };
        let label = &self.items[item].label;
        if self.focus == Focus::Search && self.has_groups() {
            format!("{}  {label}", self.groups[self.item_groups[item]])
        } else {
            label.clone()
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
        // A group's name counts, so `fx 2` finds what's in that slot.
        let labels: Vec<String> = self
            .items
            .iter()
            .enumerate()
            .map(|(at, item)| match self.item_groups.get(at) {
                Some(group) => format!("{} {}", self.groups[*group], item.label),
                None => item.label.clone(),
            })
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
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

    fn grouped() -> Picker {
        let item = |label: &str| Item {
            label: label.to_owned(),
            pick: Pick::Track(0),
        };
        let items = ["---  Off", "Cut  A", "Res  B", "Pan  C"]
            .map(item)
            .to_vec();
        let groups = ["---", "Filter", "Filter", "Output"]
            .map(str::to_owned)
            .to_vec();
        Picker::grouped("Dest", items, groups, 2)
    }

    #[test]
    fn it_opens_on_the_list_at_the_current_choice() {
        let picker = grouped();
        assert_eq!(picker.focus, Focus::List);
        assert_eq!(picker.group, 1);
        assert_eq!(picker.shown().count(), 2);
        assert_eq!(picker.current().unwrap().label, "Res  B");
    }

    #[test]
    fn a_group_of_one_shows_what_is_in_it() {
        assert_eq!(grouped().group_names(), ["---", "Filter", "Pan"]);
    }

    #[test]
    fn groups_stop_at_each_end_and_show_their_own_items() {
        let mut picker = grouped();
        picker.move_group(1);
        assert_eq!(picker.current().unwrap().label, "Pan  C");
        picker.move_group(1);
        assert_eq!(picker.group, 2);
        picker.move_group(-5);
        assert_eq!(picker.current().unwrap().label, "---  Off");
    }

    #[test]
    fn a_search_reaches_every_group_and_shows_each_matchs_group_and_goes_back() {
        let mut picker = grouped();
        picker.toggle_search();
        assert_eq!(picker.focus, Focus::Search);
        assert_eq!(picker.shown().count(), 4, "all of them, before typing");
        assert_eq!(picker.current().unwrap().label, "Res  B", "still there");
        picker.set_query("pan".to_owned());
        assert_eq!(picker.current().unwrap().label, "Pan  C");
        assert_eq!(picker.text_of(0), "Output  Pan  C");
        // The name of a group counts.
        picker.set_query("filter".to_owned());
        assert_eq!(picker.shown().count(), 2);
        picker.set_query("pan".to_owned());
        picker.toggle_search();
        assert_eq!(picker.focus, Focus::List);
        assert_eq!(picker.group, 2, "back on what it found");
        assert!(picker.query.is_empty());
    }

    #[test]
    fn a_plain_list_is_only_searched() {
        let picker = Picker::new("Plain", Vec::new());
        assert_eq!(picker.focus, Focus::Search);
        assert!(!picker.has_groups());
    }

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
