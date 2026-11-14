//! The picker's state, keys and matching: one list for every choice, after snacks.nvim's picker.
//! Items can sit in categories, a tree one level deep that folds as snacks' explorer does, and
//! typing searches across them all. `rank` is adapted from gmuse's `src/ui/picker.rs`.
//!
//! In tree mode it's the file explorer, after snacks' explorer: folders that open and close, read
//! as they're opened, with `Backspace` going up, `H` for hidden files, `P` for the preview and an
//! exact search across what's been opened. `explorer.rs` says what each purpose lists.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Style;
use vt_model::accumulator::Accumulation;
use vt_model::arpeggio::Arpeggio;
use vt_model::bank::PlayParams;
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::modulation::ModSlot;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::Field;
use vt_model::settings::SettingId;
use vt_model::shape::params::Choice;
use vt_model::song::{FilterMode, FilterType, Machine, Role};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, GateKind};

use crate::audio::AudioChoice;
use crate::guides::Guide;
use crate::input::command::Line;
use crate::input::keys::Key;
use crate::input::mode::View;
use crate::track_page::Page;

mod tree;
pub use tree::{Root, Tree};

/// How many rows the list keeps in view beyond the cursor, as Vim's `scrolloff` does, so moving
/// down a long list shows what's coming. It's the context Vim's popup menu keeps, and is shrunk to
/// fit a short list, as snacks' list shrinks the window's `scrolloff`.
pub const SCROLL_MARGIN: usize = 3;

/// One of a kit's sounds as it's chosen: its file, what its bank entry is called and how it
/// plays, and the label its track takes as its name and its choke group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitChoice {
    pub path: PathBuf,
    pub label: String,
    pub name: String,
    pub play: PlayParams,
    pub choke: Option<u8>,
}

/// What choosing a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    View(View),
    /// Show the track page on one of its pages.
    TrackPage(Page),
    /// Put the setting under the cursor on this macro, move it there, or take it off.
    Macro(usize),
    Sample(PathBuf),
    /// Make a kit's sounds the ones `:sample` lists first.
    Kit(PathBuf),
    /// Put a kit's sounds on tracks from the first.
    KitTracks(PathBuf),
    /// Load a preset on the cursor's track: a sound, or an effect into the slot under the cursor.
    Preset {
        kind: vt_model::preset::PresetKind,
        path: PathBuf,
    },
    /// Open the command line with this typed, to finish there, as `:preset save ` is.
    Prompt(String),
    /// Put this command in an FX slot, or empty it.
    FxCommand {
        slot: usize,
        command: Option<FxCommand>,
    },
    /// Put this gate type in the gate cell, or empty it.
    Gate(Option<GateKind>),
    /// Give the gate cell this length, on the `00` to `FF` scale, or leave it to the track's `Gate`.
    Length(Option<u8>),
    /// Put a `CND` with this condition, by its value, in an FX slot.
    Condition {
        slot: usize,
        value: u8,
    },
    /// Put an `ACU` moving its note this far, by its value, in an FX slot.
    Amount {
        slot: usize,
        value: u8,
    },
    /// Give a track these accumulator settings.
    Accumulation {
        track: usize,
        accumulation: Accumulation,
    },
    /// Give a track these arpeggio settings.
    Arpeggio {
        track: usize,
        arpeggio: Arpeggio,
    },
    /// Replace one of a track's mod slots.
    Mod {
        track: usize,
        slot: usize,
        modulator: ModSlot,
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
    /// Choose how long a track's steps are.
    Speed {
        track: usize,
        speed: Speed,
    },
    /// Say what a track is for, which its colour shows.
    Role {
        track: usize,
        role: Role,
    },
    /// A sound from the current kit, for this track.
    KitSound(KitChoice),
    /// Some of a kit's sounds, each on a new track after the last, in a new group named after
    /// the kit, folded: the kit dialog's `Enter`.
    KitLoad {
        kit: PathBuf,
        sounds: Vec<KitChoice>,
    },
    /// Write a beat from Grids' map on these tracks, each its own part, or nothing for none.
    KitBeat(Vec<usize>),
    /// A bank entry for a track's sampler to play, or `00` for none.
    BankSample {
        track: usize,
        number: u8,
    },
    /// A bank entry for a step's `SMP` to lock, in this FX column.
    SampleLock {
        slot: usize,
        number: u8,
    },
    /// A slice for a step's `SLI` to lock, in this FX column.
    SliceLock {
        slot: usize,
        number: u8,
    },
    /// A slice for a step's `SLI` to lock by the letter naming it, in this FX column.
    SliceMarkLock {
        slot: usize,
        letter: char,
    },
    /// The slice a letter names, to go to on the sample page, as `'a` goes.
    Mark(char),
    /// One of the sample page's settings, to change on its settings rows.
    SampleSetting(crate::sample_settings::SampleSetting),
    /// A value for one of the sample page's settings, or none for a slice to play as its sample.
    SampleValue {
        setting: crate::sample_settings::SampleSetting,
        value: Option<i32>,
    },
    /// A value for the Sampler device's `Tune` or `Cents` on the cursor's track, or none for a
    /// slice's tune to play as its sample's.
    Tuning {
        control: crate::track_page::Control,
        value: Option<i8>,
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
    /// A choice for the shape panel's setting: its fill, its wave and the wave's length, or
    /// which rows it fills.
    ShapeChoice(Choice),
    /// A choice for a setting of one of a phrase's live generators, on the Gen page, by its
    /// place in the phrase's list.
    GenChoice {
        phrase: u32,
        generator: usize,
        choice: Choice,
    },
    /// A column of the phrase under the cursor to make live, from the Gen page's `Empty`.
    GenColumn(Field),
    /// The same for an empty FX column, with the command it writes.
    GenCommand {
        field: Field,
        command: FxCommand,
    },
    /// The command the shape panel writes on an FX column with none.
    ShapeCommand(FxCommand),
    Use {
        section: usize,
        track: usize,
        slot: usize,
    },
    Guide(Guide),
    /// Play through another output, from the audio page.
    Audio(AudioChoice),
    /// Put this in the command line in place of the word being completed.
    Completion(String),
    /// A folder in the file explorer, which opens, or is chosen to save or render in.
    Folder(PathBuf),
    /// A project's folder, to open.
    Project(PathBuf),
    /// Run a command line, from a list of a project setting's values.
    Line(Line),
    /// The scale as a whole: its root, its kind and whether it's on.
    ScaleTo(Scale),
    /// The groove the project page draws and changes.
    ShowGroove(GrooveId),
}

/// Something to show beside the list for the item under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// A kit's sounds, each with its waveform.
    Kit(PathBuf),
    /// A sample's waveform, length, rate and channels.
    Sample(PathBuf),
    /// A project's name, tempo, tracks and when it was saved.
    Project(PathBuf),
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
    /// How its name is drawn, as a track in a group is in the group's colour.
    pub style: Style,
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
            style: Style::new(),
        }
    }

    /// With its name drawn in this style.
    pub fn styled(self, style: Style) -> Self {
        Self { style, ..self }
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
    /// Plain lines under the note, such as when a project was saved.
    pub details: Vec<String>,
    /// A waveform across the preview, as the rows it covers in each column of dots, such as a
    /// sample's.
    pub wave: Option<Vec<(usize, usize)>>,
    pub lines: Vec<PreviewLine>,
}

impl Shown {
    /// A heading and a note, with nothing else yet.
    pub fn new(item: usize, heading: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            item,
            heading: heading.into(),
            note: note.into(),
            details: Vec::new(),
            wave: None,
            lines: Vec::new(),
        }
    }
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
    /// The first row shown, which stays put until the cursor comes within the margin of an edge,
    /// and the rows the list had room for when last drawn.
    first_shown: usize,
    /// Whether `zz` or `zt` showed space after the last row, which moving then keeps.
    past_end: bool,
    room: usize,
    /// A hint for keys this list has beyond the usual, drawn under it. The explorer's `H`, `P`
    /// and `A` follow it there, each with its switch showing whether it's on.
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
    /// Digits typed for a count, and a `g` or a `z` waiting for another key.
    count: String,
    pending_g: bool,
    pending_z: bool,
    /// The preview of the item under the cursor, and the images of its lines once sent.
    pub preview: Option<Arc<Shown>>,
    pub preview_images: Vec<Option<u32>>,
    /// Waveforms already worked out, by file, so moving back to a kit loads nothing again.
    pub waves: HashMap<PathBuf, Option<Vec<(usize, usize)>>>,
    /// The image of the preview's own waveform, once sent.
    pub preview_wave: Option<u32>,
    /// In tree mode, the explorer's tree.
    tree: Option<Tree>,
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
            first_shown: 0,
            past_end: false,
            room: 0,
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
            pending_z: false,
            preview: None,
            preview_images: Vec::new(),
            waves: HashMap::new(),
            preview_wave: None,
            tree: None,
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

    /// Whether there's a preview beside the list: in the explorer, while `P` has it on, and
    /// elsewhere where any item has one.
    pub fn has_previews(&self) -> bool {
        match &self.tree {
            Some(tree) => tree.preview,
            None => self.items.iter().any(|item| item.preview.is_some()),
        }
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
        match &self.tree {
            Some(tree) => tree.nodes[item].depth > 0,
            None => self.item_categories[item].is_some(),
        }
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
        if self.tree.is_some() && self.is_searching() {
            self.matched
                .iter()
                .filter(|found| !found.is_empty())
                .count()
        } else if self.is_searching() {
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

    /// The first row shown.
    pub fn first_shown(&self) -> usize {
        self.first_shown
    }

    /// Fit the list to the rows it's drawn in, which `zz` and `C-d` then go by, scrolling only as
    /// far as keeps the cursor in view with its margin.
    pub fn scroll_within(&mut self, room: usize) {
        self.room = room;
        self.scroll(self.first_shown);
    }

    /// Show the list from a row, or as near it as keeps the margin round the cursor. Moving
    /// leaves no space after the last row, unless `zz` or `zt` already put some there. Before
    /// it's drawn, it waits to know its room.
    fn scroll(&mut self, first: usize) {
        let last = self.rows.len().saturating_sub(self.room);
        let end = if self.past_end {
            last.max(self.first_shown)
        } else {
            last
        };
        self.scroll_until(first, end);
        self.past_end = self.first_shown > last;
    }

    /// `zz` and `zt`: show the list from a row even past its end, as Vim shows the space after
    /// a buffer's last line, so the last rows can sit in the middle or at the top.
    fn scroll_past_end(&mut self, first: usize) {
        self.scroll_until(first, usize::MAX);
        self.past_end = self.first_shown > self.rows.len().saturating_sub(self.room);
    }

    fn scroll_until(&mut self, first: usize, end: usize) {
        if self.room == 0 {
            return;
        }
        let margin = SCROLL_MARGIN.min((self.room - 1) / 2);
        let lowest = (self.selected + margin + 1).saturating_sub(self.room);
        let highest = self.selected.saturating_sub(margin);
        self.first_shown = first.max(lowest).min(highest).min(end);
    }

    /// `C-d` and `C-u`: the cursor and the view half the list down or up, as in Vim.
    fn half_page(&mut self, direction: isize) {
        let by = (self.room / 2).max(1) as isize * direction;
        self.move_highlight(by);
        self.scroll(self.first_shown.saturating_add_signed(by));
    }

    /// Change the search. Matches are ranked best first, and an empty search goes back to the
    /// tree with the cursor still on what it was on.
    pub fn set_query(&mut self, query: String) {
        let kept = self.current_index();
        self.query = query;
        self.rebuild();
        match kept {
            Some(item) if !self.is_searching() => self.reveal(item),
            // In the explorer, the first match rather than a folder it's in.
            _ if self.tree.is_some() => {
                self.selected = self
                    .matched
                    .iter()
                    .position(|found| !found.is_empty())
                    .unwrap_or(0);
            }
            _ => self.selected = 0,
        }
    }

    /// Work out the rows again. Space `zz` or `zt` left after the last row goes with them.
    fn rebuild(&mut self) {
        self.past_end = false;
        if self.tree.is_some() {
            (self.rows, self.matched) = self.tree_rows();
        } else if self.is_searching() {
            let haystacks: Vec<String> = self.items.iter().map(Item::haystack).collect();
            let texts: Vec<(&str, &str)> = self
                .items
                .iter()
                .zip(&haystacks)
                .map(|(item, haystack)| (item.label.as_str(), haystack.as_str()))
                .collect();
            let (rows, matched) = rank_with_positions(&texts, &self.query)
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
        if self.tree.is_some() {
            let mut parent = self.node(item).and_then(|node| node.parent);
            while let Some(at) = parent {
                self.set_open(at, true);
                parent = self.node(at).and_then(|node| node.parent);
            }
        } else if let Some(category) = self.item_categories[item] {
            self.categories[category].open = true;
        }
        self.rebuild();
        if let Some(row) = self.rows.iter().position(|row| *row == Row::Item(item)) {
            self.selected = row;
        }
    }

    /// `l` and `Enter`: open a category and go into it, or choose an item.
    fn enter(&mut self, wider: bool, confirm: bool) -> Outcome {
        if self.tree.is_some() {
            return self.enter_tree(wider, confirm);
        }
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
        if self.tree.is_some() {
            self.close_tree();
            return;
        }
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
        let outcome = match self.focus {
            Focus::List => self.list_key(key),
            Focus::Search => self.search_key(key),
        };
        self.scroll(self.first_shown);
        outcome
    }

    fn list_key(&mut self, key: Key) -> Outcome {
        let after_g = std::mem::take(&mut self.pending_g);
        let after_z = std::mem::take(&mut self.pending_z);
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
            KeyCode::Char('d') if key.ctrl => self.half_page(1),
            KeyCode::Char('u') if key.ctrl => self.half_page(-1),
            _ if key.ctrl => {}
            KeyCode::Char('j') | KeyCode::Down => self.move_highlight(by),
            KeyCode::Char('k') | KeyCode::Up => self.move_highlight(-by),
            // The cursor's row to the middle, top or bottom, as Vim's `zz`, `zt` and `zb` put it,
            // keeping the margin and the list's ends.
            KeyCode::Char('z') if after_z => {
                self.scroll_past_end(
                    self.selected
                        .saturating_sub(self.room.saturating_sub(1) / 2),
                );
            }
            KeyCode::Char('t') if after_z => self.scroll_past_end(self.selected),
            KeyCode::Char('b') if after_z => {
                self.scroll((self.selected + 1).saturating_sub(self.room))
            }
            KeyCode::Char('z') => self.pending_z = true,
            KeyCode::Char('g') if after_g => self.selected = 0,
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                self.selected = count.map_or(usize::MAX, |row| row.saturating_sub(1));
                self.move_highlight(0);
            }
            // Digits then `Enter` choose by number, as a search for them.
            KeyCode::Enter if !digits.is_empty() => {
                self.set_query(digits);
                return self.enter(false, true);
            }
            KeyCode::Char('l') => return self.enter(false, false),
            KeyCode::Enter => return self.enter(false, true),
            // Only a list with a wider choice keeps `L` from the search.
            KeyCode::Char('L') if self.items.iter().any(|item| item.wider.is_some()) => {
                return self.enter(true, true);
            }
            // The explorer's own keys, after snacks' explorer.
            KeyCode::Right if self.tree.is_some() => return self.enter(false, false),
            KeyCode::Left if self.tree.is_some() => self.close_tree(),
            KeyCode::Backspace if self.tree.is_some() => self.up(),
            KeyCode::Char('H') if self.tree.is_some() => self.toggle_hidden(),
            KeyCode::Char('P') if let Some(tree) = self.tree.as_mut() => {
                tree.preview = !tree.preview;
            }
            KeyCode::Char('A')
                if let Some(tree) = self.tree.as_mut()
                    && tree.purpose.auditions() =>
            {
                tree.audition = !tree.audition;
            }
            // The kit dialog marks sounds to load, as nnn's `Space` and `a` select files.
            KeyCode::Char(' ' | 'x') if self.marks_sounds() => {
                self.toggle_marks(count.unwrap_or(1));
            }
            KeyCode::Char('a') if self.marks_sounds() => self.mark_all(),
            KeyCode::Char('h') => self.close_category(),
            KeyCode::Char('/') => self.focus = Focus::Search,
            // A count, a `g` or a `z` is let go first, as in Vim.
            KeyCode::Esc if !digits.is_empty() || after_g || after_z => {}
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
            KeyCode::Char('d') if key.ctrl => self.half_page(1),
            KeyCode::Char('u') if key.ctrl => self.half_page(-1),
            KeyCode::Down => self.move_highlight(1),
            KeyCode::Up => self.move_highlight(-1),
            KeyCode::Enter => return self.enter(false, true),
            // Back to the list, as `Esc` leaves Insert mode, keeping what's found so `j` and `k`
            // move through it. Only the list's own `Esc` closes.
            KeyCode::Esc => self.focus = Focus::List,
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
    let texts: Vec<(&str, &str)> = labels.iter().map(|label| (*label, *label)).collect();
    rank_with_positions(&texts, needle)
        .into_iter()
        .map(|(at, _)| at)
        .collect()
}

/// [`rank`], for items found by more than their name: each is its name, then all the text a
/// search looks through, which starts with the name, as an item's name, meaning and category do.
/// Each word of the search matches on its own, its letters in order, so `pi sc` finds `Pitch in
/// scale`. A name the search is, then one it starts, then one all its words are in, come before
/// a match that needs the rest, so `note` takes `Note` before `Trig`, whose meaning says `note`.
/// With where in each text its matched letters are, counted in characters.
pub fn rank_with_positions(texts: &[(&str, &str)], needle: &str) -> Vec<(usize, Vec<u32>)> {
    if needle.trim().is_empty() {
        return (0..texts.len()).map(|at| (at, Vec::new())).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(needle.trim(), CaseMatching::Ignore, Normalization::Smart);
    let words = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let searched = words(needle);
    let mut buffer = Vec::new();
    let mut found = |text: &str| {
        let mut positions = Vec::new();
        let score = pattern.indices(
            Utf32Str::new(text, &mut buffer),
            &mut matcher,
            &mut positions,
        )?;
        positions.sort_unstable();
        positions.dedup();
        Some((score, positions))
    };
    // How good a match is, best first: the name itself, the start of the name, the name, the rest.
    let mut scored: Vec<(u8, u32, usize, Vec<u32>)> = texts
        .iter()
        .enumerate()
        .filter_map(|(at, (name, text))| {
            if let Some((score, positions)) = found(name) {
                let name = words(name);
                let tier = if name == searched {
                    0
                } else if name.starts_with(&searched) {
                    1
                } else {
                    2
                };
                return Some((tier, score, at, positions));
            }
            let (score, positions) = found(text)?;
            Some((3, score, at, positions))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    scored
        .into_iter()
        .map(|(_, _, at, positions)| (at, positions))
        .collect()
}

#[cfg(test)]
mod tests;
