//! The picker's state, keys and matching: one list for every choice, after snacks.nvim's picker.
//! Items can sit in categories, a tree one level deep that folds as snacks' explorer does, and
//! typing searches across them all. `rank` is adapted from gmuse's `src/ui/picker.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::crossterm::event::KeyCode;
use vt_model::envelope::Envelope;
use vt_model::insert::Insert;
use vt_model::lfo::{Lfo, Rate, Shape as Wave};
use vt_model::scale::ScaleKind;
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, Where};
use vt_model::song::{FilterMode, FilterType, Machine};
use vt_model::step::{FxCommand, GateKind};

use crate::audio::AudioChoice;
use crate::guides::Guide;
use crate::input::keys::Key;
use crate::input::mode::View;

/// What choosing a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    View(View),
    Sample(PathBuf),
    /// Make a kit's sounds the ones `Space 5` lists first.
    Kit(PathBuf),
    /// Put a kit's sounds on tracks from the first.
    KitTracks(PathBuf),
    /// Put this command in an FX slot, or empty it.
    FxCommand {
        slot: usize,
        command: Option<FxCommand>,
    },
    /// Put this gate type in the gate cell, or empty it.
    Gate(Option<GateKind>),
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
    /// Choose how a track's filter responds.
    FilterMode {
        track: usize,
        mode: FilterMode,
    },
    /// Replace one of a track's envelopes.
    Envelope {
        track: usize,
        slot: usize,
        envelope: Envelope,
    },
    /// A sound from the current kit, for this track, and the label the track takes as its name,
    /// its level, its pitch and its choke group.
    KitSound {
        path: PathBuf,
        label: String,
        level: u8,
        pitch: u8,
        choke: Option<u8>,
    },
    /// Go to this track.
    Track(usize),
    /// What makes a track's sound.
    Machine {
        track: usize,
        machine: Machine,
    },
    /// A value for one of a track's machine's settings, such as 6-op's patch.
    Setting {
        track: usize,
        setting: SettingId,
        value: u8,
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
    /// Play through another output, from the audio page.
    Audio(AudioChoice),
}

/// Something to show beside the list for the item under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// A kit's sounds, each with its waveform.
    Kit(PathBuf),
}

/// One choice: its name, what it means, drawn dim beside it, and the category it's listed under,
/// or none for the top of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub meaning: String,
    pub category: String,
    pub pick: Pick,
    /// What `L` chooses, the wider choice, as a kit's sounds put on tracks rather than listed.
    pub wider: Option<Pick>,
    pub preview: Option<Preview>,
}

impl Item {
    /// A choice with only a name.
    pub fn new(label: impl Into<String>, pick: Pick) -> Self {
        Self {
            label: label.into(),
            meaning: String::new(),
            category: String::new(),
            pick,
            wider: None,
            preview: None,
        }
    }

    /// With what it means beside it.
    pub fn meaning(self, meaning: impl Into<String>) -> Self {
        Self {
            meaning: meaning.into(),
            ..self
        }
    }

    /// Listed under a category.
    pub fn category(self, category: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            ..self
        }
    }

    /// With a wider choice for `L`.
    pub fn wider(self, pick: Pick) -> Self {
        Self {
            wider: Some(pick),
            ..self
        }
    }

    /// With something to show beside the list.
    pub fn preview(self, preview: Preview) -> Self {
        Self {
            preview: Some(preview),
            ..self
        }
    }

    /// What a search looks through: its name, its meaning and its category, so `filter` finds
    /// everything in Filter and `send b` finds `SNB`.
    fn haystack(&self) -> String {
        format!("{} {} {}", self.label, self.meaning, self.category)
    }
}

/// A line of the list as it's shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Category(usize),
    Item(usize),
}

/// A category and the items in it, in order.
#[derive(Debug, Clone)]
struct Category {
    name: String,
    items: Vec<usize>,
    open: bool,
}

/// Where keys go. In the list, `j` `k` `h` `l` move and any other letter starts a search with
/// itself. In the search, every letter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Search,
}

/// What a key did to an open picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Choose(Pick),
    Close,
}

/// What the preview shows for the item under the cursor, once it's been loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The item it's for.
    pub item: usize,
    pub heading: String,
    /// Dim, under the heading, such as where a kit came from.
    pub note: String,
    pub lines: Vec<PreviewLine>,
}

/// One line of a preview, such as a sound in a kit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    pub label: String,
    pub name: String,
    /// The rows a waveform covers in each of its columns of dots, if it loaded.
    pub wave: Option<Vec<(usize, usize)>>,
}

/// An open picker: a search line over a list, with categories that fold, after snacks.nvim's
/// picker and its explorer.
#[derive(Debug, Clone)]
pub struct Picker {
    pub title: &'static str,
    pub query: String,
    pub focus: Focus,
    /// The row the cursor is on.
    pub selected: usize,
    /// A hint for keys this list has beyond the usual, drawn under it.
    pub hint: Option<&'static str>,
    items: Vec<Item>,
    categories: Vec<Category>,
    /// Which category each item is in, if it's in one of more than one item.
    item_categories: Vec<Option<usize>>,
    /// The top of the tree, in order: categories, and items in none.
    top: Vec<Row>,
    rows: Vec<Row>,
    /// While searching, where in each row's text its matched letters are.
    matched: Vec<Vec<u32>>,
    /// The item already chosen, which the list marks.
    current: Option<usize>,
    /// Digits typed for a count, and a `g` waiting for another.
    count: String,
    pending_g: bool,
    /// The preview of the item under the cursor, and the images of its lines once sent.
    pub preview: Option<Arc<Shown>>,
    pub preview_images: Vec<Option<u32>>,
    /// Waveforms already worked out, by file, so moving back to a kit loads nothing again.
    pub waves: HashMap<PathBuf, Option<Vec<(usize, usize)>>>,
}

impl Picker {
    /// A picker over these items, on the list, with every category folded and the cursor on the
    /// first row. A category's items should be next to each other, and the categories come in the
    /// order their first items do. A category of one item is listed as the item itself.
    pub fn new(title: &'static str, items: Vec<Item>) -> Self {
        let mut categories: Vec<Category> = Vec::new();
        for (at, item) in items.iter().enumerate() {
            if item.category.is_empty() {
                continue;
            }
            match categories.iter_mut().find(|c| c.name == item.category) {
                Some(category) => category.items.push(at),
                None => categories.push(Category {
                    name: item.category.clone(),
                    items: vec![at],
                    open: false,
                }),
            }
        }
        categories.retain(|category| category.items.len() > 1);
        let mut item_categories = vec![None; items.len()];
        for (index, category) in categories.iter().enumerate() {
            for item in &category.items {
                item_categories[*item] = Some(index);
            }
        }
        let mut top = Vec::new();
        for (at, category) in item_categories.iter().enumerate() {
            match category {
                None => top.push(Row::Item(at)),
                Some(category) if categories[*category].items[0] == at => {
                    top.push(Row::Category(*category));
                }
                Some(_) => {}
            }
        }
        let mut picker = Self {
            title,
            query: String::new(),
            focus: Focus::List,
            selected: 0,
            hint: None,
            items,
            categories,
            item_categories,
            top,
            rows: Vec::new(),
            matched: Vec::new(),
            current: None,
            count: String::new(),
            pending_g: false,
            preview: None,
            preview_images: Vec::new(),
            waves: HashMap::new(),
        };
        picker.rebuild();
        picker
    }

    /// Open on the item already chosen, unfolding its category, and mark it.
    pub fn opened_on(mut self, item: Option<usize>) -> Self {
        self.current = item.filter(|item| *item < self.items.len());
        if let Some(item) = self.current {
            self.reveal(item);
        }
        self
    }

    /// Open on an item without marking it as chosen, such as the first of a kit's sounds.
    pub fn revealing(mut self, item: Option<usize>) -> Self {
        if let Some(item) = item.filter(|item| *item < self.items.len()) {
            self.reveal(item);
        }
        self
    }

    /// Open on the search, with this already typed.
    pub fn searching(mut self, query: &str) -> Self {
        self.focus = Focus::Search;
        if !query.is_empty() {
            self.set_query(query.to_owned());
        }
        self
    }

    /// With a hint for its keys.
    pub fn hinted(self, hint: &'static str) -> Self {
        Self {
            hint: Some(hint),
            ..self
        }
    }

    /// Whether any item is in a category.
    pub fn has_categories(&self) -> bool {
        !self.categories.is_empty()
    }

    /// Whether any item has a preview.
    pub fn has_previews(&self) -> bool {
        self.items.iter().any(|item| item.preview.is_some())
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn item(&self, item: usize) -> &Item {
        &self.items[item]
    }

    /// The rows shown now: the tree, or the matches while searching.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Where a row's matched letters are in its label, then its meaning, then its category, each
    /// after a space.
    pub fn matched(&self, row: usize) -> &[u32] {
        self.matched.get(row).map_or(&[], Vec::as_slice)
    }

    /// A category's name, how many items it holds, and whether it's open.
    pub fn category(&self, category: usize) -> (&str, usize, bool) {
        let category = &self.categories[category];
        (&category.name, category.items.len(), category.open)
    }

    /// Whether an item sits inside an open category, rather than at the top.
    pub fn is_nested(&self, item: usize) -> bool {
        self.item_categories[item].is_some()
    }

    /// The item already chosen.
    pub fn marked(&self) -> Option<usize> {
        self.current
    }

    /// Digits typed toward a count.
    pub fn count(&self) -> &str {
        &self.count
    }

    /// The items shown now, without the categories' headings.
    pub fn shown(&self) -> impl Iterator<Item = &Item> {
        self.rows.iter().filter_map(|row| match row {
            Row::Item(item) => Some(&self.items[*item]),
            Row::Category(_) => None,
        })
    }

    /// How many items match the search, which is all of them before typing.
    pub fn matching(&self) -> usize {
        if self.is_searching() {
            self.rows.len()
        } else {
            self.items.len()
        }
    }

    /// Whether the list is the search's matches rather than the tree.
    pub fn is_searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// The item under the cursor, which is none on a category's heading.
    pub fn current(&self) -> Option<&Item> {
        self.current_index().map(|item| &self.items[item])
    }

    pub fn current_index(&self) -> Option<usize> {
        match self.rows.get(self.selected)? {
            Row::Item(item) => Some(*item),
            Row::Category(_) => None,
        }
    }

    /// Move the cursor, stopping at the ends.
    pub fn move_highlight(&mut self, by: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
    }

    /// Change the search. Matches are ranked best first, and an empty search goes back to the
    /// tree with the cursor still on what it was on.
    pub fn set_query(&mut self, query: String) {
        let kept = self.current_index();
        self.query = query;
        self.rebuild();
        match kept {
            Some(item) if !self.is_searching() => self.reveal(item),
            _ => self.selected = 0,
        }
    }

    /// Work out the rows again.
    fn rebuild(&mut self) {
        if self.is_searching() {
            let haystacks: Vec<String> = self.items.iter().map(Item::haystack).collect();
            let haystacks: Vec<&str> = haystacks.iter().map(String::as_str).collect();
            let (rows, matched) = rank_with_positions(&haystacks, &self.query)
                .into_iter()
                .map(|(item, positions)| (Row::Item(item), positions))
                .unzip();
            self.rows = rows;
            self.matched = matched;
        } else {
            self.matched.clear();
            self.rows = self
                .top
                .iter()
                .flat_map(|row| {
                    let inside = match row {
                        Row::Category(category) if self.categories[*category].open => {
                            self.categories[*category].items.as_slice()
                        }
                        _ => &[],
                    };
                    std::iter::once(*row).chain(inside.iter().map(|item| Row::Item(*item)))
                })
                .collect();
        }
        self.move_highlight(0);
    }

    /// Show an item in the tree, opening its category, with the cursor on it.
    fn reveal(&mut self, item: usize) {
        if let Some(category) = self.item_categories[item] {
            self.categories[category].open = true;
        }
        self.rebuild();
        if let Some(row) = self.rows.iter().position(|row| *row == Row::Item(item)) {
            self.selected = row;
        }
    }

    /// `l` and `Enter`: open a category and go into it, or choose an item.
    fn enter(&mut self, wider: bool) -> Outcome {
        match self.rows.get(self.selected).copied() {
            Some(Row::Category(category)) => {
                self.categories[category].open = true;
                self.rebuild();
                self.move_highlight(1);
                Outcome::Stay
            }
            Some(Row::Item(item)) => {
                let item = &self.items[item];
                let pick = if wider { item.wider.as_ref() } else { None };
                Outcome::Choose(pick.unwrap_or(&item.pick).clone())
            }
            None => Outcome::Close,
        }
    }

    /// `h`: close the category the cursor is in, going up to it, or the one it's on.
    fn close_category(&mut self) {
        let category = match self.rows.get(self.selected).copied() {
            Some(Row::Category(category)) => category,
            Some(Row::Item(item)) => match self.item_categories[item] {
                Some(category) => category,
                None => return,
            },
            None => return,
        };
        self.categories[category].open = false;
        self.rebuild();
        if let Some(row) = self
            .rows
            .iter()
            .position(|row| *row == Row::Category(category))
        {
            self.selected = row;
        }
    }

    /// Act on a key.
    pub fn key(&mut self, key: Key) -> Outcome {
        if key.alt || key.command {
            return Outcome::Stay;
        }
        match self.focus {
            Focus::List => self.list_key(key),
            Focus::Search => self.search_key(key),
        }
    }

    fn list_key(&mut self, key: Key) -> Outcome {
        let after_g = std::mem::take(&mut self.pending_g);
        let digits = std::mem::take(&mut self.count);
        let count = digits.parse::<usize>().ok();
        let by = count.unwrap_or(1).min(isize::MAX as usize) as isize;
        match key.code {
            // A count, as in Vim, where `0` alone isn't one.
            KeyCode::Char(c @ '0'..='9') if !key.ctrl && (c != '0' || !digits.is_empty()) => {
                self.count = format!("{digits}{c}");
            }
            KeyCode::Char('n') if key.ctrl => self.move_highlight(by),
            KeyCode::Char('p') if key.ctrl => self.move_highlight(-by),
            _ if key.ctrl => {}
            KeyCode::Char('j') | KeyCode::Down => self.move_highlight(by),
            KeyCode::Char('k') | KeyCode::Up => self.move_highlight(-by),
            KeyCode::Char('g') if after_g => self.selected = 0,
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                self.selected = count.map_or(usize::MAX, |row| row.saturating_sub(1));
                self.move_highlight(0);
            }
            // Digits then `Enter` choose by number, as a search for them.
            KeyCode::Enter if !digits.is_empty() => {
                self.set_query(digits);
                return self.enter(false);
            }
            KeyCode::Char('l') | KeyCode::Enter => return self.enter(false),
            // Only a list with a wider choice keeps `L` from the search.
            KeyCode::Char('L') if self.items.iter().any(|item| item.wider.is_some()) => {
                return self.enter(true);
            }
            KeyCode::Char('h') => self.close_category(),
            KeyCode::Char('/') => self.focus = Focus::Search,
            // A count or a `g` is let go first, as in Vim.
            KeyCode::Esc if !digits.is_empty() || after_g => {}
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Char(c) => {
                self.focus = Focus::Search;
                self.set_query(format!("{digits}{c}"));
            }
            _ => {}
        }
        Outcome::Stay
    }

    fn search_key(&mut self, key: Key) -> Outcome {
        match key.code {
            KeyCode::Char('n') if key.ctrl => self.move_highlight(1),
            KeyCode::Char('p') if key.ctrl => self.move_highlight(-1),
            KeyCode::Down => self.move_highlight(1),
            KeyCode::Up => self.move_highlight(-1),
            KeyCode::Enter => return self.enter(false),
            // Clearing the search goes back to the list, on what was found.
            KeyCode::Esc if !self.query.is_empty() => {
                self.set_query(String::new());
                self.focus = Focus::List;
            }
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Backspace => {
                let mut query = self.query.clone();
                query.pop();
                self.set_query(query);
            }
            KeyCode::Char(c) if !key.ctrl => {
                let query = format!("{}{c}", self.query);
                self.set_query(query);
            }
            _ => {}
        }
        Outcome::Stay
    }
}

/// The positions of the labels that match, best first. The order they came in breaks ties, so a
/// list in order of recent use keeps that order, and an empty search keeps every label.
pub fn rank(labels: &[&str], needle: &str) -> Vec<usize> {
    rank_with_positions(labels, needle)
        .into_iter()
        .map(|(at, _)| at)
        .collect()
}

/// [`rank`], with where in each label its matched letters are, counted in characters.
pub fn rank_with_positions(labels: &[&str], needle: &str) -> Vec<(usize, Vec<u32>)> {
    if needle.trim().is_empty() {
        return (0..labels.len()).map(|at| (at, Vec::new())).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(needle.trim(), CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut scored: Vec<(u32, usize, Vec<u32>)> = labels
        .iter()
        .enumerate()
        .filter_map(|(at, label)| {
            let haystack = Utf32Str::new(label, &mut buffer);
            let mut positions = Vec::new();
            let score = pattern.indices(haystack, &mut matcher, &mut positions)?;
            positions.sort_unstable();
            positions.dedup();
            Some((score, at, positions))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .map(|(_, at, positions)| (at, positions))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::keys::parse;

    const VIEWS: [&str; 6] = ["table", "song", "chain", "phrase", "instrument", "groove"];

    /// Press keys as typed, with a named key in angle brackets, such as `<Esc>`.
    fn press(picker: &mut Picker, keys: &str) -> Outcome {
        let mut outcome = Outcome::Stay;
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            let (key, after) = match rest.find('>').filter(|_| c == '<') {
                Some(end) => (parse(&rest[1..end]).unwrap(), &rest[end + 1..]),
                None => (Key::char(c), &rest[c.len_utf8()..]),
            };
            outcome = picker.key(key);
            rest = after;
        }
        outcome
    }

    fn label(picker: &Picker) -> &str {
        picker.current().map_or("", |item| item.label.as_str())
    }

    /// `Off` on its own, Filter with three, Output with one, and Amp with two.
    fn tree() -> Picker {
        let items = [
            ("Off", ""),
            ("Cut", "Filter"),
            ("Res", "Filter"),
            ("Drv", "Filter"),
            ("Pan", "Output"),
            ("Vol", "Amp"),
            ("Gain", "Amp"),
        ]
        .map(|(label, category)| Item::new(label, Pick::Track(0)).category(category))
        .to_vec();
        Picker::new("Dest", items)
    }

    #[test]
    fn categories_start_folded_and_one_of_one_item_is_the_item() {
        let picker = tree();
        assert_eq!(
            picker.rows(),
            [
                Row::Item(0),
                Row::Category(0),
                Row::Item(4),
                Row::Category(1)
            ]
        );
        assert_eq!(picker.category(0), ("Filter", 3, false));
        assert!(!picker.is_nested(4), "Pan stands alone");
    }

    #[test]
    fn it_opens_on_the_current_value_inside_its_category() {
        let picker = tree().opened_on(Some(2));
        assert_eq!(label(&picker), "Res");
        assert_eq!(picker.marked(), Some(2));
        assert!(picker.category(0).2, "its category is open");
        assert!(!picker.category(1).2, "the others are folded");
        assert_eq!(picker.shown().count(), 5);
    }

    #[test]
    fn l_goes_into_a_category_and_h_closes_it_from_inside() {
        let mut picker = tree();
        press(&mut picker, "j");
        assert_eq!(picker.current(), None, "on Filter's heading");
        press(&mut picker, "l");
        assert_eq!(label(&picker), "Cut");
        press(&mut picker, "j");
        assert_eq!(label(&picker), "Res");
        press(&mut picker, "h");
        assert_eq!(picker.rows()[picker.selected], Row::Category(0));
        assert!(!picker.category(0).2);
        // Enter opens one too, and h on an open heading folds it.
        press(&mut picker, "<Enter>k");
        assert!(picker.category(0).2);
        press(&mut picker, "h");
        assert!(!picker.category(0).2);
        // h at the top does nothing.
        press(&mut picker, "kh");
        assert_eq!(label(&picker), "Off");
    }

    #[test]
    fn counts_gg_and_g_move_through_the_rows() {
        let mut picker = tree();
        press(&mut picker, "3j");
        assert_eq!(picker.selected, 3);
        press(&mut picker, "gg");
        assert_eq!(picker.selected, 0);
        press(&mut picker, "G");
        assert_eq!(picker.selected, 3);
        press(&mut picker, "2G");
        assert_eq!(picker.selected, 1);
        press(&mut picker, "<C-n>");
        assert_eq!(picker.selected, 2);
        press(&mut picker, "<C-p>");
        assert_eq!(picker.selected, 1);
        assert_eq!(picker.focus, Focus::List, "none of them type");
        assert!(picker.query.is_empty());
    }

    #[test]
    fn a_search_flattens_the_tree_and_escape_clears_it_then_closes() {
        let mut picker = tree();
        press(&mut picker, "dr");
        assert_eq!(
            picker.focus,
            Focus::Search,
            "a letter that isn't a key searches"
        );
        assert_eq!(picker.query, "dr");
        assert_eq!(label(&picker), "Drv", "inside a folded category");
        assert!(picker.rows().iter().all(|row| matches!(row, Row::Item(_))));
        assert_eq!(picker.matched(0), [0, 1], "the letters it matched");
        // j and k type in the search, and a category's name counts.
        press(&mut picker, "<Esc>/amp");
        assert_eq!(picker.matching(), 2);
        press(&mut picker, "<Esc>");
        assert_eq!(picker.focus, Focus::List);
        assert!(picker.query.is_empty());
        assert_eq!(
            label(&picker),
            "Vol",
            "back on what it found, its category open"
        );
        assert!(picker.category(1).2);
        assert_eq!(press(&mut picker, "<Esc>"), Outcome::Close);
    }

    #[test]
    fn enter_chooses_and_l_chooses_wider_where_it_can() {
        let items = vec![
            Item::new("909", Pick::Kit("909.ron".into())).wider(Pick::KitTracks("909.ron".into())),
            Item::new("808", Pick::Kit("808.ron".into())),
        ];
        let mut picker = Picker::new("Kits", items);
        assert_eq!(
            press(&mut picker, "L"),
            Outcome::Choose(Pick::KitTracks("909.ron".into()))
        );
        assert_eq!(
            press(&mut picker, "<Enter>"),
            Outcome::Choose(Pick::Kit("909.ron".into()))
        );
        assert_eq!(
            press(&mut picker, "jL"),
            Outcome::Choose(Pick::Kit("808.ron".into())),
            "nothing wider is the usual choice"
        );
    }

    #[test]
    fn digits_then_enter_choose_by_number_and_zero_starts_a_search() {
        let items = (0..12)
            .map(|at| Item::new(format!("{at:02}"), Pick::Track(at)))
            .collect();
        let mut picker = Picker::new("Tracks", items);
        assert_eq!(
            press(&mut picker, "11<Enter>"),
            Outcome::Choose(Pick::Track(11))
        );
        let mut picker = Picker::new("Tracks", picker.items.clone());
        press(&mut picker, "04");
        assert_eq!(picker.query, "04");
        assert_eq!(label(&picker), "04");
    }

    #[test]
    fn a_search_can_open_already_typed() {
        let picker = tree().searching("pan");
        assert_eq!(picker.focus, Focus::Search);
        assert_eq!(label(&picker), "Pan");
        let picker = tree().searching("");
        assert_eq!(picker.focus, Focus::Search);
        assert_eq!(picker.rows().len(), 4, "the tree, before typing");
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
        let items = VIEWS.map(|label| Item::new(label, Pick::View(View::Phrase)));
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
