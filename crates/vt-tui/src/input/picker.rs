//! The picker's state, keys and matching: one list for every choice, after snacks.nvim's picker.
//! Items can sit in categories, a tree one level deep that folds as snacks' explorer does, and
//! typing searches across them all. `rank` is adapted from gmuse's `src/ui/picker.rs`.
//!
//! In tree mode it's the file explorer, after snacks' explorer: folders that open and close, read
//! as they're opened, with `Backspace` going up, `H` for hidden files, `P` for the preview and an
//! exact search across what's been opened. `explorer.rs` says what each purpose lists.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::crossterm::event::KeyCode;
use vt_model::accumulator::Accumulation;
use vt_model::arpeggio::Arpeggio;
use vt_model::bank::PlayParams;
use vt_model::envelope::Envelope;
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::lfo::{Lfo, Rate, Shape as Wave};
use vt_model::scale::{Scale, ScaleKind};
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, Where};
use vt_model::song::{FilterMode, FilterType, Machine};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, GateKind};

use crate::audio::AudioChoice;
use crate::explorer::{self, Icon, Purpose};
use crate::guides::Guide;
use crate::input::command::Line;
use crate::input::keys::Key;
use crate::input::mode::View;

/// How many rows the list keeps in view beyond the cursor, as Vim's `scrolloff` does, so moving
/// down a long list shows what's coming. It's the context Vim's popup menu keeps, and is shrunk to
/// fit a short list, as snacks' list shrinks the window's `scrolloff`.
pub const SCROLL_MARGIN: usize = 3;

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
    /// Choose how long a track's steps are.
    Speed {
        track: usize,
        speed: Speed,
    },
    /// Replace one of a track's envelopes.
    Envelope {
        track: usize,
        slot: usize,
        envelope: Envelope,
    },
    /// A sound from the current kit, for this track: what its bank entry is called and how it
    /// plays, and the label the track takes as its name and its choke group.
    KitSound {
        path: PathBuf,
        label: String,
        name: String,
        play: PlayParams,
        choke: Option<u8>,
    },
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

/// The top of the explorer's tree: a folder it starts from, with why it's there dim beside it, or
/// items given up front under a name, such as the kit's sounds.
#[derive(Debug, Clone)]
pub enum Root {
    Place { path: PathBuf, meaning: String },
    Group { label: String, items: Vec<Item> },
}

/// A file or folder in the explorer's tree, beside its item.
#[derive(Debug, Clone)]
struct Node {
    /// None for a group, and for what's in one.
    path: Option<PathBuf>,
    parent: Option<usize>,
    depth: usize,
    /// What it is, as its icon shows.
    icon: Icon,
    /// A folder's or a group's: whether it's open, and what's in it once it's been read.
    folder: Option<Folder>,
}

#[derive(Debug, Clone, Default)]
struct Folder {
    open: bool,
    children: Option<Vec<usize>>,
}

/// The explorer's tree, beside the picker's items, and its switches.
#[derive(Debug, Clone)]
pub struct Tree {
    pub purpose: Purpose,
    /// Whether hidden files are listed.
    pub hidden: bool,
    /// Whether the preview is shown.
    pub preview: bool,
    /// Whether moving onto a sample plays it.
    pub audition: bool,
    roots: Vec<Root>,
    home: Option<PathBuf>,
    nodes: Vec<Node>,
    /// The roots' nodes, in order.
    top: Vec<usize>,
}

/// Where the cursor was and which folders were open, kept while the tree is read again.
struct Remembered {
    open: Vec<PathBuf>,
    closed_groups: Vec<usize>,
    cursor: Option<PathBuf>,
    cursor_item: Option<usize>,
}

/// A path as it is on disk, so `/tmp` and `/private/tmp` are the same place, as far as it's there.
fn real(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => real(parent).join(name),
        _ => path.to_path_buf(),
    }
}

/// Where a search's letters are in a name, matched exactly but ignoring case, as the explorer's
/// search is, counted in characters.
fn exact(label: &str, query: &str) -> Option<Vec<u32>> {
    let label: Vec<char> = label.chars().collect();
    let query: Vec<char> = query.trim().chars().collect();
    let same = |a: char, b: char| a.to_lowercase().eq(b.to_lowercase());
    let start = (0..=label.len().checked_sub(query.len())?).find(|start| {
        label[*start..start + query.len()]
            .iter()
            .zip(&query)
            .all(|(a, b)| same(*a, *b))
    })?;
    Some((start..start + query.len()).map(|at| at as u32).collect())
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

    /// The file explorer for a purpose, over these roots, every folder folded and every group
    /// open, with the cursor on the first row. `home` is the folder named `~`.
    pub fn explorer(purpose: Purpose, roots: Vec<Root>, home: Option<PathBuf>) -> Self {
        let mut picker = Self::new(purpose.title(), Vec::new());
        picker.tree = Some(Tree {
            purpose,
            hidden: false,
            preview: purpose.previews(),
            audition: purpose.auditions(),
            roots,
            home,
            nodes: Vec::new(),
            top: Vec::new(),
        });
        picker.build();
        picker.rebuild();
        picker
    }

    /// Open on a file or folder, opening the folders down to it, and mark it. A folder is opened
    /// too, to show what's in it. Where it isn't listed, the cursor goes as near as it can.
    pub fn following(mut self, path: Option<&Path>) -> Self {
        if let Some(path) = path
            && let Some(at) = self.find(path)
        {
            let found = self.node(at).and_then(|node| node.path.as_deref());
            if found.is_some_and(|found| real(found) == real(path)) {
                self.current = Some(at);
                self.set_open(at, true);
            }
            self.reveal(at);
        }
        self
    }

    /// With the first folder at the top of the explorer's tree open, and the cursor on the first
    /// thing in it, unless a folder is open already, so there's something to see straight away.
    pub fn opening_first(mut self) -> Self {
        let Some(tree) = &self.tree else {
            return self;
        };
        let open = |node: &Node| node.folder.as_ref().is_some_and(|folder| folder.open);
        if tree
            .nodes
            .iter()
            .any(|node| node.path.is_some() && open(node))
        {
            return self;
        }
        let Some(first) = tree
            .top
            .iter()
            .copied()
            .find(|at| tree.nodes[*at].path.is_some())
        else {
            return self;
        };
        self.set_open(first, true);
        self.rebuild();
        if let Some(child) = self.children(first).first().copied() {
            self.reveal(child);
        }
        self
    }

    /// With every folder at the top of the explorer's tree open.
    pub fn opening_roots(mut self) -> Self {
        let top = self
            .tree
            .as_ref()
            .map(|tree| tree.top.clone())
            .unwrap_or_default();
        for root in top {
            self.set_open(root, true);
        }
        self.rebuild();
        self
    }

    /// The explorer's tree, in tree mode.
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub fn tree_mut(&mut self) -> Option<&mut Tree> {
        self.tree.as_mut()
    }

    fn node(&self, item: usize) -> Option<&Node> {
        self.tree.as_ref()?.nodes.get(item)
    }

    fn node_mut(&mut self, item: usize) -> Option<&mut Node> {
        self.tree.as_mut()?.nodes.get_mut(item)
    }

    /// How deep an item is in the explorer's tree, from 0 for a root.
    pub fn depth(&self, item: usize) -> usize {
        self.node(item).map_or(0, |node| node.depth)
    }

    /// Whether an item in the explorer is a folder, and if so whether it's open.
    pub fn folder_open(&self, item: usize) -> Option<bool> {
        self.node(item)?.folder.as_ref().map(|folder| folder.open)
    }

    /// The file or folder an item in the explorer is.
    pub fn path(&self, item: usize) -> Option<&Path> {
        self.node(item)?.path.as_deref()
    }

    /// What an item in the explorer is, as its icon shows.
    pub fn icon(&self, item: usize) -> Option<Icon> {
        self.node(item).map(|node| node.icon)
    }

    /// A sample that couldn't be read, which the explorer then shows with a warning wherever
    /// it's listed.
    pub fn mark_unreadable(&mut self, path: &Path) {
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        for (item, node) in self.items.iter().zip(tree.nodes.iter_mut()) {
            if let Pick::Sample(file) | Pick::KitSound { path: file, .. } = &item.pick
                && file == path
            {
                node.icon = Icon::Missing;
            }
        }
    }

    /// Add an item, and its node in the tree.
    fn push(&mut self, item: Item, node: Node) -> usize {
        self.items.push(item);
        self.item_categories.push(None);
        if let Some(tree) = self.tree.as_mut() {
            tree.nodes.push(node);
        }
        self.items.len() - 1
    }

    /// Make the tree's roots again, with nothing read.
    fn build(&mut self) {
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        tree.nodes.clear();
        tree.top.clear();
        let roots = tree.roots.clone();
        let home = tree.home.clone();
        self.items.clear();
        self.item_categories.clear();
        for root in roots {
            let at = match root {
                Root::Place { path, meaning } => {
                    let label = explorer::place_name(&path, home.as_deref());
                    let item = Item::new(label, Pick::Folder(path.clone())).meaning(meaning);
                    self.push(
                        item,
                        Node {
                            path: Some(path),
                            parent: None,
                            depth: 0,
                            icon: Icon::Folder,
                            folder: Some(Folder::default()),
                        },
                    )
                }
                Root::Group { label, items } => {
                    let group = self.push(
                        Item::new(label, Pick::Folder(PathBuf::new())),
                        Node {
                            path: None,
                            parent: None,
                            depth: 0,
                            icon: Icon::Kit,
                            folder: Some(Folder {
                                open: true,
                                children: Some(Vec::new()),
                            }),
                        },
                    );
                    let children = items
                        .into_iter()
                        .map(|item| {
                            // The kit in use's sounds, each warned of if its file has gone.
                            let icon = match &item.pick {
                                Pick::KitSound { path, .. } => {
                                    explorer::kit_sound_icon(path, false)
                                }
                                _ => Icon::KitSound,
                            };
                            self.push(
                                item,
                                Node {
                                    path: None,
                                    parent: Some(group),
                                    depth: 1,
                                    icon,
                                    folder: None,
                                },
                            )
                        })
                        .collect();
                    if let Some(folder) = self.node_mut(group).and_then(|node| node.folder.as_mut())
                    {
                        folder.children = Some(children);
                    }
                    group
                }
            };
            if let Some(tree) = self.tree.as_mut() {
                tree.top.push(at);
            }
        }
    }

    /// Read what's in a folder, or a kit, if it hasn't been read.
    fn load(&mut self, at: usize) {
        let Some(tree) = self.tree.as_ref() else {
            return;
        };
        let node = &tree.nodes[at];
        let (Some(folder), Some(path)) = (&node.folder, &node.path) else {
            return;
        };
        if folder.children.is_some() {
            return;
        }
        let depth = node.depth + 1;
        let Some(entries) = explorer::contents(path, tree.purpose, tree.hidden) else {
            // It can't be read, so it's empty, and says so.
            if let Some(node) = self.node_mut(at) {
                node.icon = Icon::Missing;
                if let Some(folder) = node.folder.as_mut() {
                    folder.children = Some(Vec::new());
                }
            }
            return;
        };
        let children = entries
            .into_iter()
            .map(|entry| {
                self.push(
                    entry.item,
                    Node {
                        path: Some(entry.path),
                        parent: Some(at),
                        depth,
                        icon: entry.icon,
                        folder: entry.opens.then(Folder::default),
                    },
                )
            })
            .collect();
        if let Some(folder) = self.node_mut(at).and_then(|node| node.folder.as_mut()) {
            folder.children = Some(children);
        }
    }

    /// What's in a folder or group, as far as it's been read.
    fn children(&self, at: usize) -> &[usize] {
        self.node(at)
            .and_then(|node| node.folder.as_ref()?.children.as_deref())
            .unwrap_or(&[])
    }

    /// Open or close a folder.
    fn set_open(&mut self, at: usize, open: bool) {
        if open {
            self.load(at);
        }
        if let Some(folder) = self.node_mut(at).and_then(|node| node.folder.as_mut()) {
            folder.open = open;
        }
    }

    /// The node a path is at, or the deepest folder on the way to it that's listed, reading the
    /// folders down to it from the root that holds it most closely.
    fn find(&mut self, path: &Path) -> Option<usize> {
        let target = real(path);
        let top = self.tree.as_ref()?.top.clone();
        let (mut at, rest) = top
            .into_iter()
            .filter_map(|root| {
                let place = real(self.path(root)?);
                let rest = target.strip_prefix(&place).ok()?.to_path_buf();
                Some((root, rest))
            })
            .min_by_key(|(_, rest)| rest.components().count())?;
        for part in rest.components() {
            self.load(at);
            let next = self.children(at).iter().copied().find(|child| {
                self.path(*child)
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == part.as_os_str())
            });
            match next {
                Some(next) => at = next,
                None => break,
            }
        }
        Some(at)
    }

    /// The rows of the tree, from its roots: every open folder's contents, or while searching,
    /// what's been read that matches and the folders it's in, open or not.
    fn tree_rows(&self) -> (Vec<Row>, Vec<Vec<u32>>) {
        let mut rows = Vec::new();
        let mut matched = Vec::new();
        let query = self.query.trim().to_owned();
        if let Some(tree) = &self.tree {
            for root in &tree.top {
                self.visit(*root, &query, &mut rows, &mut matched);
            }
        }
        (rows, matched)
    }

    /// Add a node's row and those under it, and say whether any were kept.
    fn visit(
        &self,
        at: usize,
        query: &str,
        rows: &mut Vec<Row>,
        matched: &mut Vec<Vec<u32>>,
    ) -> bool {
        let searching = !query.is_empty();
        let found = if searching {
            exact(&self.items[at].label, query)
        } else {
            Some(Vec::new())
        };
        let mark = rows.len();
        rows.push(Row::Item(at));
        matched.push(found.clone().unwrap_or_default());
        let mut kept = found.is_some();
        let open = self
            .node(at)
            .and_then(|node| node.folder.as_ref())
            .is_some_and(|folder| folder.open);
        if open || searching {
            for child in self.children(at).to_vec() {
                kept |= self.visit(child, query, rows, matched);
            }
        }
        if !kept {
            rows.truncate(mark);
            matched.truncate(mark);
        }
        kept
    }

    /// Which folders are open and where the cursor is, to put back once it's read again.
    fn remember(&self) -> Remembered {
        let tree = self.tree.as_ref();
        let nodes = tree.map_or(&[][..], |tree| tree.nodes.as_slice());
        let open = |node: &Node| node.folder.as_ref().is_some_and(|folder| folder.open);
        Remembered {
            open: nodes
                .iter()
                .filter(|node| open(node))
                .filter_map(|node| node.path.clone())
                .collect(),
            closed_groups: nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.path.is_none() && node.folder.is_some() && !open(node))
                .map(|(at, _)| at)
                .collect(),
            cursor: self
                .current_index()
                .and_then(|item| self.path(item).map(Path::to_path_buf)),
            cursor_item: self.current_index(),
        }
    }

    /// Read the tree again from its roots, with these folders open and the cursor back.
    fn restore(&mut self, remembered: Remembered) {
        let marked = self
            .current
            .and_then(|item| self.path(item).map(Path::to_path_buf));
        self.build();
        for group in remembered.closed_groups {
            self.set_open(group, false);
        }
        for path in &remembered.open {
            if let Some(at) = self.find(path)
                && self.path(at) == Some(path.as_path())
            {
                self.set_open(at, true);
            }
        }
        self.current = None;
        if let Some(marked) = marked
            && let Some(at) = self.find(&marked)
            && self.path(at) == Some(marked.as_path())
        {
            self.current = Some(at);
        }
        let cursor = match remembered.cursor {
            Some(path) => self.find(&path),
            None => remembered
                .cursor_item
                .filter(|item| *item < self.items.len()),
        };
        self.rebuild();
        match cursor {
            Some(at) => self.reveal(at),
            None => self.selected = 0,
        }
    }

    /// `Backspace`: make the root the cursor is under its own parent folder, with it open inside,
    /// as snacks' explorer goes up a level.
    fn up(&mut self) {
        let Some(mut root) = self.current_index() else {
            return;
        };
        while let Some(parent) = self.node(root).and_then(|node| node.parent) {
            root = parent;
        }
        let Some(tree) = self.tree.as_ref() else {
            return;
        };
        let Some(place) = tree.top.iter().position(|at| *at == root) else {
            return;
        };
        let Root::Place { path, meaning } = tree.roots[place].clone() else {
            return;
        };
        let Some(parent) = path.parent().map(Path::to_path_buf) else {
            return;
        };
        let mut remembered = self.remember();
        remembered.open.insert(0, path.clone());
        remembered.cursor.get_or_insert(path);
        if let Some(tree) = self.tree.as_mut() {
            tree.roots[place] = Root::Place {
                path: parent.clone(),
                meaning,
            };
            // The parent may already be another root, which this one now stands for.
            let mut index = 0;
            tree.roots.retain(|root| {
                let keep =
                    index == place || !matches!(root, Root::Place { path, .. } if *path == parent);
                index += 1;
                keep
            });
        }
        self.restore(remembered);
    }

    /// `H`: list hidden files or stop.
    fn toggle_hidden(&mut self) {
        let remembered = self.remember();
        if let Some(tree) = self.tree.as_mut() {
            tree.hidden = !tree.hidden;
        }
        self.restore(remembered);
    }

    /// `l` and `Enter` in the explorer: open a folder and go into it, or choose a file. `Enter`
    /// chooses a folder where that's what's being chosen, and chooses what opens to more of its
    /// own, such as a kit's sound with variants, which `l` opens.
    fn enter_tree(&mut self, wider: bool, confirm: bool) -> Outcome {
        let Some(at) = self.current_index() else {
            return Outcome::Stay;
        };
        let Some(tree) = self.tree.as_ref() else {
            return Outcome::Stay;
        };
        let node = &tree.nodes[at];
        let chooses = !matches!(self.items[at].pick, Pick::Folder(_));
        if node.folder.is_none() || (confirm && chooses) {
            let item = &self.items[at];
            let pick = if wider { item.wider.as_ref() } else { None };
            return Outcome::Choose(pick.unwrap_or(&item.pick).clone());
        }
        if confirm
            && tree.purpose.chooses_folder()
            && let Some(path) = node.path.clone()
        {
            return Outcome::Choose(Pick::Folder(path));
        }
        self.set_open(at, true);
        self.rebuild();
        if let Some(row) = self.rows.iter().position(|row| *row == Row::Item(at)) {
            self.selected = row;
            if self
                .children(at)
                .first()
                .is_some_and(|first| self.rows.get(row + 1) == Some(&Row::Item(*first)))
            {
                self.selected = row + 1;
            }
        }
        Outcome::Stay
    }

    /// `h` in the explorer: close the folder the cursor is on, or the one it's in, going up to it.
    fn close_tree(&mut self) {
        let Some(at) = self.current_index() else {
            return;
        };
        let target = if self.folder_open(at) == Some(true) {
            at
        } else {
            match self.node(at).and_then(|node| node.parent) {
                Some(parent) => parent,
                None => return,
            }
        };
        self.set_open(target, false);
        self.rebuild();
        if let Some(row) = self.rows.iter().position(|row| *row == Row::Item(target)) {
            self.selected = row;
        }
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

    /// A sample library: two folders of WAVs, a hidden one, and a file that isn't a sample.
    fn library(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("vt-tree-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["drums/kicks", "drums/hats", "loops", ".cache"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        for file in [
            "drums/kicks/deep.wav",
            "drums/kicks/short.wav",
            "drums/hats/closed.wav",
            "loops/break.wav",
            "notes.txt",
            ".cache/old.wav",
        ] {
            std::fs::write(root.join(file), "").unwrap();
        }
        root
    }

    fn explorer(root: &Path) -> Picker {
        let roots = vec![Root::Place {
            path: root.to_path_buf(),
            meaning: "samples".to_owned(),
        }];
        Picker::explorer(Purpose::Sample, roots, None)
    }

    /// The rows as they're shown: each indented by its depth, a folder with its arrow.
    fn shown_rows(picker: &Picker) -> Vec<String> {
        picker
            .rows()
            .iter()
            .map(|row| {
                let Row::Item(item) = row else {
                    return String::new();
                };
                let arrow = match picker.folder_open(*item) {
                    Some(true) => "v ",
                    Some(false) => "> ",
                    None => "",
                };
                let indent = "  ".repeat(picker.depth(*item));
                let label = &picker.item(*item).label;
                if picker.depth(*item) == 0 {
                    format!("{arrow}root")
                } else {
                    format!("{indent}{arrow}{label}")
                }
            })
            .collect()
    }

    #[test]
    fn the_explorer_opens_folders_with_l_and_closes_them_with_h() {
        let root = library("keys");
        let mut picker = explorer(&root);
        assert_eq!(shown_rows(&picker), ["> root"], "folded, nothing read");
        assert_eq!(picker.items().len(), 1);
        press(&mut picker, "l");
        assert_eq!(
            shown_rows(&picker),
            ["v root", "  > drums", "  > loops"],
            "folders first, and only samples, with hidden ones left out"
        );
        assert_eq!(label(&picker), "drums", "into the folder");
        press(&mut picker, "l");
        press(&mut picker, "l");
        assert_eq!(label(&picker), "closed.wav");
        assert_eq!(picker.depth(picker.current_index().unwrap()), 3);
        // h on a file closes the folder it's in, going up to it, and on an open folder closes it.
        press(&mut picker, "h");
        assert_eq!(label(&picker), "hats");
        assert_eq!(
            picker.folder_open(picker.current_index().unwrap()),
            Some(false)
        );
        press(&mut picker, "kh");
        assert_eq!(label(&picker), "drums");
        assert_eq!(shown_rows(&picker), ["v root", "  > drums", "  > loops"]);
        // l on a file chooses it, and Enter does too.
        press(&mut picker, "jl");
        assert_eq!(
            press(&mut picker, "l"),
            Outcome::Choose(Pick::Sample(root.join("loops/break.wav")))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn h_lists_hidden_files_and_keeps_the_tree_as_it_was() {
        let root = library("hidden");
        let mut picker = explorer(&root);
        press(&mut picker, "ljl");
        assert_eq!(label(&picker), "break.wav");
        press(&mut picker, "H");
        assert!(picker.tree().unwrap().hidden);
        assert_eq!(
            shown_rows(&picker),
            [
                "v root",
                "  > .cache",
                "  > drums",
                "  v loops",
                "    break.wav"
            ],
            "the hidden folder, with what was open still open"
        );
        assert_eq!(label(&picker), "break.wav", "the cursor where it was");
        press(&mut picker, "H");
        assert_eq!(shown_rows(&picker).len(), 4);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backspace_makes_the_root_its_parent_with_it_open_inside() {
        let root = library("up");
        let mut picker = explorer(&root.join("drums"));
        press(&mut picker, "l");
        assert_eq!(label(&picker), "hats");
        press(&mut picker, "<Backspace>");
        let top = picker.current_index().unwrap();
        assert_eq!(label(&picker), "hats", "the cursor stays");
        assert_eq!(picker.depth(top), 2, "a level deeper now");
        assert_eq!(
            shown_rows(&picker),
            [
                "v root",
                "  v drums",
                "    > hats",
                "    > kicks",
                "  > loops"
            ]
        );
        assert_eq!(picker.item(0).meaning, "samples", "it keeps why it's there");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_explorers_search_is_exact_across_what_has_been_read() {
        let root = library("search");
        let mut picker = explorer(&root);
        press(&mut picker, "lllhh");
        assert_eq!(label(&picker), "drums");
        // `drums/hats` has been read, `drums/kicks` and `loops` haven't.
        press(&mut picker, "/cl");
        assert_eq!(
            shown_rows(&picker),
            ["v root", "  > drums", "    > hats", "      closed.wav"],
            "a match, under the folders it's in, open or not"
        );
        assert_eq!(
            label(&picker),
            "closed.wav",
            "on the match, not its folders"
        );
        assert_eq!(picker.matched(3), [0, 1]);
        press(&mut picker, "<Backspace><Backspace>csd");
        assert!(
            picker.rows().is_empty(),
            "not fuzzy: no c, s and d in a row"
        );
        press(&mut picker, "<Esc>eep");
        assert!(picker.rows().is_empty(), "kicks hasn't been read");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn it_follows_a_file_opening_the_folders_down_to_it() {
        let root = library("follow");
        let picker = explorer(&root).following(Some(&root.join("drums/kicks/short.wav")));
        assert_eq!(label(&picker), "short.wav");
        assert_eq!(picker.marked(), picker.current_index(), "marked as chosen");
        assert_eq!(
            shown_rows(&picker),
            [
                "v root",
                "  v drums",
                "    > hats",
                "    v kicks",
                "      deep.wav",
                "      short.wav",
                "  > loops"
            ]
        );
        // A file that isn't listed goes as near as it can.
        let picker = explorer(&root).following(Some(&root.join("drums/gone.wav")));
        assert_eq!(label(&picker), "drums");
        assert_eq!(picker.marked(), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn p_and_a_switch_the_preview_and_playing_and_enter_chooses_a_folder_to_save_in() {
        let root = library("switches");
        let mut picker = explorer(&root);
        assert!(picker.has_previews(), "a sample has one");
        assert!(
            picker.tree().unwrap().audition,
            "and plays as it's moved onto"
        );
        press(&mut picker, "PA");
        assert!(!picker.has_previews());
        assert!(!picker.tree().unwrap().audition);
        assert!(picker.query.is_empty(), "neither searches");

        let roots = vec![Root::Place {
            path: root.clone(),
            meaning: String::new(),
        }];
        let mut picker = Picker::explorer(Purpose::Save, roots, None);
        assert!(!picker.has_previews(), "a folder has nothing to show");
        press(&mut picker, "l");
        assert_eq!(label(&picker), "drums", "l opens");
        press(&mut picker, "A");
        assert_eq!(picker.query, "A", "nothing plays, so A searches");
        press(&mut picker, "<Backspace><Esc>l");
        assert_eq!(
            press(&mut picker, "<Enter>"),
            Outcome::Choose(Pick::Folder(root.join("drums"))),
            "and Enter chooses"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_group_of_items_given_up_front_starts_open() {
        let root = library("group");
        let items = vec![
            Item::new("BD", Pick::Track(0)),
            Item::new("SD", Pick::Track(1)),
        ];
        let roots = vec![
            Root::Group {
                label: "Kit 909".to_owned(),
                items,
            },
            Root::Place {
                path: root.clone(),
                meaning: String::new(),
            },
        ];
        let mut picker = Picker::explorer(Purpose::Sample, roots, None).revealing(Some(1));
        assert_eq!(label(&picker), "BD");
        assert_eq!(shown_rows(&picker), ["v root", "  BD", "  SD", "> root"]);
        assert_eq!(
            press(&mut picker, "j<Enter>"),
            Outcome::Choose(Pick::Track(1))
        );
        press(&mut picker, "h");
        assert_eq!(shown_rows(&picker), ["> root", "> root"]);
        std::fs::remove_dir_all(&root).unwrap();
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
        press(&mut picker, "<Esc>amp");
        assert_eq!(picker.matching(), 2);
        // `Esc` leaves the search for the list, keeping what it found, as `Esc` leaves Insert.
        press(&mut picker, "<Esc>");
        assert_eq!(picker.focus, Focus::List);
        assert_eq!(picker.query, "amp");
        assert_eq!(picker.matching(), 2);
        // Typing again starts a new search, which emptied shows the whole list.
        press(&mut picker, "/<Backspace><Backspace><Backspace><Esc>");
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

    /// A list of forty items drawn sixteen rows tall, as the tallest list is.
    fn long() -> Picker {
        let items = (0..40)
            .map(|at| Item::new(format!("{at:02} sound"), Pick::Track(at)))
            .collect();
        let mut picker = Picker::new("Sounds", items);
        picker.scroll_within(16);
        picker
    }

    /// The first row shown and the cursor's row.
    fn view(picker: &Picker) -> (usize, usize) {
        (picker.first_shown(), picker.selected)
    }

    #[test]
    fn the_list_keeps_three_rows_beyond_the_cursor_each_way_and_holds_as_it_turns() {
        let mut picker = long();
        press(&mut picker, "12j");
        assert_eq!(view(&picker), (0, 12), "three rows still show under it");
        press(&mut picker, "j");
        assert_eq!(view(&picker), (1, 13), "then it scrolls");
        press(&mut picker, "8j");
        assert_eq!(view(&picker), (9, 21));
        press(&mut picker, "k");
        assert_eq!(view(&picker), (9, 20), "turning round doesn't move it");
        press(&mut picker, "8k");
        assert_eq!(view(&picker), (9, 12), "three rows still show above it");
        press(&mut picker, "k");
        assert_eq!(view(&picker), (8, 11));
    }

    #[test]
    fn the_margin_gives_way_at_the_lists_ends() {
        let mut picker = long();
        press(&mut picker, "G");
        assert_eq!(view(&picker), (24, 39), "no space after the last row");
        press(&mut picker, "3k");
        assert_eq!(view(&picker), (24, 36));
        press(&mut picker, "gg");
        assert_eq!(view(&picker), (0, 0));
        press(&mut picker, "<C-p>");
        assert_eq!(view(&picker), (0, 0));
        // A list shorter than its room never scrolls, and a short room keeps a smaller margin.
        let mut picker = tree();
        picker.scroll_within(16);
        press(&mut picker, "G");
        assert_eq!(view(&picker), (0, 3));
        let mut picker = long();
        picker.scroll_within(4);
        press(&mut picker, "4j");
        assert_eq!(view(&picker), (2, 4), "a margin of one in four rows");
    }

    #[test]
    fn zz_zt_and_zb_move_the_view_round_the_cursor_without_moving_it() {
        let mut picker = long();
        press(&mut picker, "21G");
        assert_eq!(view(&picker), (8, 20));
        press(&mut picker, "zt");
        assert_eq!(view(&picker), (17, 20), "at the top but for the margin");
        press(&mut picker, "zb");
        assert_eq!(view(&picker), (8, 20), "at the bottom but for the margin");
        press(&mut picker, "zz");
        assert_eq!(view(&picker), (13, 20), "in the middle");
        press(&mut picker, "jk");
        assert_eq!(view(&picker), (13, 20), "and it stays there");
        assert_eq!(picker.focus, Focus::List, "none of them search");
        // Nothing shows before the first row.
        press(&mut picker, "gg");
        press(&mut picker, "zz");
        assert_eq!(view(&picker), (0, 0));
        press(&mut picker, "zb");
        assert_eq!(view(&picker), (0, 0));
        // But `zz` and `zt` show space after the last, as Vim does after a buffer's last line,
        // and moving keeps it until the margin needs the view back.
        press(&mut picker, "G");
        assert_eq!(view(&picker), (24, 39), "moving leaves no space after it");
        press(&mut picker, "zz");
        assert_eq!(view(&picker), (32, 39), "in the middle");
        press(&mut picker, "zt");
        assert_eq!(view(&picker), (36, 39), "at the top but for the margin");
        press(&mut picker, "k");
        assert_eq!(
            view(&picker),
            (35, 38),
            "the margin above pulls it back a row"
        );
        press(&mut picker, "zb");
        assert_eq!(view(&picker), (26, 38));
        press(&mut picker, "G");
        assert_eq!(
            view(&picker),
            (26, 39),
            "and moving doesn't push it past the end"
        );
        // A `z` waiting is let go by `Esc` rather than closing the list.
        assert_eq!(press(&mut picker, "z<Esc>"), Outcome::Stay);
    }

    #[test]
    fn ctrl_d_and_ctrl_u_go_half_the_list() {
        let mut picker = long();
        press(&mut picker, "<C-d>");
        assert_eq!(view(&picker), (5, 8));
        press(&mut picker, "<C-d>");
        assert_eq!(
            view(&picker),
            (13, 16),
            "the view goes as far as the cursor"
        );
        press(&mut picker, "<C-u>");
        assert_eq!(view(&picker), (5, 8));
        press(&mut picker, "<C-u><C-u>");
        assert_eq!(view(&picker), (0, 0));
        press(&mut picker, "G<C-u>");
        assert_eq!(view(&picker), (19, 31), "the margin pulls the view up");
        press(&mut picker, "<C-d><C-d>");
        assert_eq!(view(&picker), (24, 39), "stopping at the end");
        // In the search too, as `C-n` and `C-p` are.
        press(&mut picker, "/sound<C-d>");
        assert_eq!(picker.query, "sound");
        assert_eq!(view(&picker), (5, 8));
    }

    #[test]
    fn searching_and_folding_after_a_scroll_keep_the_view_on_the_list() {
        let mut picker = long();
        press(&mut picker, "G/07");
        assert_eq!(picker.rows().len(), 1);
        assert_eq!(view(&picker), (0, 0));
        press(&mut picker, "<Esc>");
        assert_eq!(picker.rows().len(), 1, "Esc keeps the search");
        assert_eq!(view(&picker), (0, 0));
        // Closing a long category from far down it.
        let mut items: Vec<Item> = (0..5)
            .map(|at| Item::new(format!("{at}"), Pick::Track(at)))
            .collect();
        items
            .extend((5..35).map(|at| Item::new(format!("{at}"), Pick::Track(at)).category("Many")));
        let mut picker = Picker::new("Sounds", items);
        picker.scroll_within(16);
        press(&mut picker, "Gl20j");
        assert_eq!(view(&picker), (14, 26));
        press(&mut picker, "h");
        assert_eq!(
            view(&picker),
            (0, 5),
            "on the category, with the list at its top"
        );
    }
}
