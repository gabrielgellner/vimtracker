//! The explorer's tree: its folders, read as they're opened and closed, and the
//! `Backspace`, `H` and `h` that go up, show hidden files and close a folder. In the kit explorer,
//! the kit dialog, `Space` or `x` marks one of a kit's sounds, `a` marks them all, and `Enter`
//! loads what's marked as new tracks (decision 306).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vt_model::tag::{self, Tag, TagLibrary};

use crate::explorer::{self, Icon, Purpose};

use super::{Item, KitChoice, Outcome, Pick, Picker, Row};

/// The most a search by tag reads below the open folders, so one over the home folder stops.
pub const MOST_READ: usize = 20_000;

/// The top of the explorer's tree: a folder it starts from, with why it's there dim beside it, or
/// items given up front under a name, such as the kit's sounds.
#[derive(Debug, Clone)]
pub enum Root {
    Place { path: PathBuf, meaning: String },
    Group { label: String, items: Vec<Item> },
}

/// A file or folder in the explorer's tree, beside its item.
#[derive(Debug, Clone)]
pub(super) struct Node {
    /// None for a group, and for what's in one.
    path: Option<PathBuf>,
    pub(super) parent: Option<usize>,
    pub(super) depth: usize,
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
    /// In the kit dialog, the sounds marked to load, by their places under their kit's path, all
    /// in one kit.
    pub marked: Vec<PathBuf>,
    /// The tags given by hand, for what's listed (decision 315).
    pub library: Arc<TagLibrary>,
    /// Whether a search by tag stopped reading at [`MOST_READ`].
    pub read_short: bool,
    roots: Vec<Root>,
    home: Option<PathBuf>,
    pub(super) nodes: Vec<Node>,
    /// The roots' nodes, in order.
    pub(super) top: Vec<usize>,
}

/// The kit dialog's hint, in a kit's sounds or with any marked.
pub const MARKING_HINT: &str = "Space mark  a all  Enter load";

/// The hint while `T` types an item's tags.
pub const TAGGING_HINT: &str = "Enter tag  Tab complete  Esc back";

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

impl Picker {
    /// The file explorer for a purpose, over these roots, every folder folded and every group
    /// open, with the cursor on the first row. `home` is the folder named `~`.
    pub fn explorer(purpose: Purpose, roots: Vec<Root>, home: Option<PathBuf>) -> Self {
        let mut picker = Self::new(purpose.title(), Vec::new());
        picker.tree = Some(Tree {
            purpose,
            hidden: false,
            preview: purpose.previews(),
            audition: purpose.auditions(),
            marked: Vec::new(),
            library: Arc::default(),
            read_short: false,
            roots,
            home,
            nodes: Vec::new(),
            top: Vec::new(),
        });
        picker.build();
        picker.rebuild();
        picker
    }

    /// With the tags given by hand, for what it lists. It's read again from its roots, so this
    /// comes before anything is opened.
    pub fn with_tags(mut self, library: Arc<TagLibrary>) -> Self {
        if let Some(tree) = self.tree.as_mut() {
            tree.library = library;
        }
        self.build();
        self.rebuild();
        self
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

    pub(super) fn node(&self, item: usize) -> Option<&Node> {
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
            if let Pick::Sample(file) | Pick::KitSound(KitChoice { path: file, .. }) = &item.pick
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
        let library = Arc::clone(&tree.library);
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
                        .map(|mut item| {
                            // The kit in use's sounds, each warned of if its file has gone.
                            let icon = match &item.pick {
                                Pick::KitSound(sound) => {
                                    item.tags =
                                        explorer::file_tags(&sound.path, &sound.label, &library);
                                    explorer::kit_sound_icon(&sound.path, false)
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
        let Some(entries) = explorer::contents(path, tree.purpose, tree.hidden, &tree.library)
        else {
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
    pub(super) fn set_open(&mut self, at: usize, open: bool) {
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
    /// what's been read that matches and the folders it's in, open or not. With each, where the
    /// search matched its name, and whether it matched.
    pub(super) fn tree_rows(&self) -> (Vec<Row>, Vec<Vec<u32>>, Vec<bool>) {
        let mut rows = Vec::new();
        let mut matched = Vec::new();
        let mut hits = Vec::new();
        let search = self.search();
        if let Some(tree) = &self.tree {
            for root in &tree.top {
                self.visit(*root, &search, &mut rows, &mut matched, &mut hits);
            }
        }
        (rows, matched, hits)
    }

    /// The search as the explorer reads it: its tags, each a word starting `#` where the
    /// explorer lists things with tags, and the rest, the name.
    pub(super) fn search(&self) -> Search {
        let tags = self.tree.as_ref().is_some_and(|tree| tree.purpose.tags());
        Search::of(&self.query, tags)
    }

    /// Whether an item is something chosen rather than a folder or a heading, so it can match a
    /// search by tag.
    fn is_chosen(&self, item: usize) -> bool {
        !matches!(self.items[item].pick, Pick::Folder(_))
    }

    /// Add a node's row and those under it, and say whether any were kept.
    fn visit(
        &self,
        at: usize,
        search: &Search,
        rows: &mut Vec<Row>,
        matched: &mut Vec<Vec<u32>>,
        hits: &mut Vec<bool>,
    ) -> bool {
        let searching = !search.is_empty();
        let found = if searching {
            search.find(&self.items[at], self.is_chosen(at))
        } else {
            Some(Vec::new())
        };
        let mark = rows.len();
        rows.push(Row::Item(at));
        hits.push(searching && found.is_some());
        matched.push(found.clone().unwrap_or_default());
        let mut kept = found.is_some();
        let open = self
            .node(at)
            .and_then(|node| node.folder.as_ref())
            .is_some_and(|folder| folder.open);
        if open || searching {
            for child in self.children(at).to_vec() {
                kept |= self.visit(child, search, rows, matched, hits);
            }
        }
        if !kept {
            rows.truncate(mark);
            matched.truncate(mark);
            hits.truncate(mark);
        }
        kept
    }

    /// Before a search by tag: read everything below the open folders, kits and sounds
    /// included, up to [`MOST_READ`] things, so `#kick` finds every kick under them rather than
    /// only those in the folders opened by hand.
    pub(super) fn read_below_open(&mut self) {
        let Some(tree) = self.tree.as_ref() else {
            return;
        };
        let opens = |node: &Node| node.path.is_some() && node.folder.is_some();
        let mut waiting: Vec<usize> = tree
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| opens(node) && node.folder.as_ref().is_some_and(|f| f.open))
            .map(|(at, _)| at)
            .collect();
        let mut short = false;
        while let Some(at) = waiting.pop() {
            if self.items.len() >= MOST_READ {
                short = true;
                break;
            }
            self.load(at);
            let children = self.children(at).to_vec();
            waiting.extend(
                children
                    .into_iter()
                    .filter(|child| self.node(*child).is_some_and(opens)),
            );
        }
        if let Some(tree) = self.tree.as_mut() {
            tree.read_short |= short;
        }
    }

    /// While a search's last word is a tag being typed, the tags starting with it on what the
    /// rest of the search keeps, each with how many have it, most first: the explorer's tag row.
    pub fn tag_row(&self) -> Option<Vec<(String, usize)>> {
        if self.focus != super::Focus::Search {
            return None;
        }
        let search = self.search();
        let typing = search.typing.as_deref()?;
        let rest = search.without_typing();
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (at, item) in self.items.iter().enumerate() {
            if !self.is_chosen(at) || rest.find(item, true).is_none() {
                continue;
            }
            for tag in item.tags.iter().filter(|tag| tag.name.starts_with(typing)) {
                match counts.iter_mut().find(|(name, _)| *name == tag.name) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((tag.name.clone(), 1)),
                }
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Some(counts)
    }

    /// How many things a search by tag found, or none for a search by name alone, whose count
    /// would only be what's been opened.
    pub fn tag_found(&self) -> Option<usize> {
        if self.tree.is_none() || self.search().tags.is_empty() {
            return None;
        }
        let found =
            self.rows.iter().zip(&self.hits).filter(|(row, hit)| {
                **hit && matches!(row, Row::Item(item) if self.is_chosen(*item))
            });
        Some(found.count())
    }

    /// `Tab` in the search: finish the tag being typed as the tag row's first, and start the
    /// next word.
    pub(super) fn complete_tag(&mut self) {
        let first = self
            .tag_row()
            .and_then(|row| row.into_iter().next())
            .map(|(name, _)| name);
        if let Some(first) = first
            && let Some(at) = self.query.rfind('#')
        {
            let query = format!("{}#{first} ", &self.query[..at]);
            self.set_query(query);
        }
    }

    /// The items listing a sample file, as a file or as a kit's sound, so a tag given to it shows
    /// wherever it's listed.
    pub fn items_of_file(&self, path: &Path) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| match &item.pick {
                Pick::Sample(file) | Pick::KitSound(KitChoice { path: file, .. }) => file == path,
                _ => false,
            })
            .map(|(at, _)| at)
            .collect()
    }

    /// Give an item these tags, and a sample the loop's icon if they say it's one, as the rows
    /// and the preview then show.
    pub fn set_tags(&mut self, item: usize, tags: Vec<Tag>) {
        let looped = tag::has(&tags, tag::LOOP);
        if let Some(node) = self.node_mut(item)
            && matches!(node.icon, Icon::Sample | Icon::Loop)
        {
            node.icon = if looped { Icon::Loop } else { Icon::Sample };
        }
        if let Some(found) = self.items.get_mut(item) {
            found.tags = tags;
        }
        if self
            .preview
            .as_ref()
            .is_some_and(|shown| shown.item == item)
        {
            self.preview = None;
        }
    }

    /// The item under the cursor if `T` can tag it: a sample, a kit's sound or a preset, in an
    /// explorer that lists things with tags.
    pub fn taggable(&self) -> Option<usize> {
        let tree = self.tree.as_ref()?;
        let item = self.current_index()?;
        let tags = matches!(
            self.items[item].pick,
            Pick::Sample(_) | Pick::KitSound(_) | Pick::Preset { .. }
        );
        (tree.purpose.tags() && tags).then_some(item)
    }

    /// `T`: type the tags of the item under the cursor, starting from those it has that can be
    /// changed, so a wrong guess is taken off by deleting it.
    pub(super) fn start_tagging(&mut self) {
        let Some(item) = self.taggable() else {
            return;
        };
        let names: Vec<&str> = self.items[item]
            .tags
            .iter()
            .filter(|tag| tag.source != tag::Source::Fixed)
            .map(|tag| tag.name.as_str())
            .collect();
        self.tag_line = names.join(" ");
        if !self.tag_line.is_empty() {
            self.tag_line.push(' ');
        }
        self.focus = super::Focus::Tags;
    }

    /// A key while typing tags: `Enter` gives them, `Tab` finishes the word from the tags in
    /// use, and `Esc` goes back to the list with nothing changed.
    pub(super) fn tag_key(&mut self, key: crate::input::keys::Key) -> Outcome {
        use ratatui::crossterm::event::KeyCode;
        match key.code {
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.tag_line);
                self.focus = super::Focus::List;
                let Some(item) = self.taggable() else {
                    return Outcome::Stay;
                };
                let mut tags: Vec<String> = Vec::new();
                for name in line.split_whitespace().filter_map(tag::tag_name) {
                    if !tags.contains(&name) {
                        tags.push(name);
                    }
                }
                return Outcome::Tag { item, tags };
            }
            KeyCode::Esc => {
                self.tag_line.clear();
                self.focus = super::Focus::List;
            }
            KeyCode::Backspace => {
                self.tag_line.pop();
            }
            KeyCode::Tab => {
                let start = self
                    .tag_line
                    .rfind(char::is_whitespace)
                    .map_or(0, |at| at + 1);
                let typed = self.tag_line[start..]
                    .trim_start_matches('#')
                    .to_lowercase();
                let found = self
                    .tags_in_use()
                    .into_iter()
                    .find(|name| !typed.is_empty() && name.starts_with(&typed));
                if let Some(name) = found {
                    self.tag_line.truncate(start);
                    self.tag_line.push_str(&name);
                    self.tag_line.push(' ');
                }
            }
            KeyCode::Char(c) if !key.ctrl => self.tag_line.push(c),
            _ => {}
        }
        Outcome::Stay
    }

    /// Use these tags given by hand for what's read from now on.
    pub fn set_library(&mut self, library: Arc<TagLibrary>) {
        if let Some(tree) = self.tree.as_mut() {
            tree.library = library;
        }
    }

    /// Every tag on what's been read, and every one given by hand anywhere, for `Tab` to
    /// complete one from as it's typed.
    pub fn tags_in_use(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .items
            .iter()
            .flat_map(|item| item.tags.iter().map(|tag| tag.name.clone()))
            .collect();
        if let Some(tree) = &self.tree {
            names.extend(tree.library.given().map(str::to_owned));
        }
        names.sort();
        names.dedup();
        names
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
    pub(super) fn up(&mut self) {
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
    pub(super) fn toggle_hidden(&mut self) {
        let remembered = self.remember();
        if let Some(tree) = self.tree.as_mut() {
            tree.hidden = !tree.hidden;
        }
        self.restore(remembered);
    }

    /// `l` and `Enter` in the explorer: open a folder and go into it, or choose a file. `Enter`
    /// chooses a folder where that's what's being chosen, and chooses what opens to more of its
    /// own, such as a kit's sound with variants, which `l` opens.
    pub(super) fn enter_tree(&mut self, wider: bool, confirm: bool) -> Outcome {
        let Some(at) = self.current_index() else {
            return Outcome::Stay;
        };
        let Some(tree) = self.tree.as_ref() else {
            return Outcome::Stay;
        };
        let node = &tree.nodes[at];
        let chooses = !matches!(self.items[at].pick, Pick::Folder(_));
        if self.marks_sounds() {
            if confirm
                && !wider
                && let Some(load) = self.marked_load()
            {
                return Outcome::Choose(load);
            }
            // A sound, or one of its files, is loaded alone.
            if let Pick::KitSound(sound) = &self.items[at].pick
                && (node.folder.is_none() || confirm)
                && let Some(kit) = self.kit_of(at).and_then(|kit| self.path(kit))
            {
                return Outcome::Choose(Pick::KitLoad {
                    kit: kit.to_path_buf(),
                    sounds: vec![sound.clone()],
                });
            }
        }
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
    pub(super) fn close_tree(&mut self) {
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

    /// Whether this is the kit dialog, which marks a kit's sounds to load as tracks.
    pub fn marks_sounds(&self) -> bool {
        self.tree
            .as_ref()
            .is_some_and(|tree| tree.purpose == Purpose::Kit)
    }

    /// The kit an item is or is in, as its node: the nearest with a kit's icon.
    fn kit_of(&self, item: usize) -> Option<usize> {
        let mut at = Some(item);
        while let Some(node) = at {
            if self.icon(node) == Some(Icon::Kit) {
                return Some(node);
            }
            at = self.node(node).and_then(|node| node.parent);
        }
        None
    }

    /// Whether an item is one of a kit's sounds that can be marked: right under its kit, with
    /// its file there.
    fn markable(&self, item: usize) -> bool {
        let Some(node) = self.node(item) else {
            return false;
        };
        node.icon != Icon::Missing
            && node
                .parent
                .is_some_and(|kit| self.icon(kit) == Some(Icon::Kit))
            && matches!(self.items[item].pick, Pick::KitSound(_))
    }

    /// Whether an item is marked to load.
    pub fn is_marked(&self, item: usize) -> bool {
        let (Some(tree), Some(path)) = (self.tree.as_ref(), self.path(item)) else {
            return false;
        };
        tree.marked.iter().any(|marked| marked == path)
    }

    /// How many sounds are marked.
    pub fn marked_count(&self) -> usize {
        self.tree.as_ref().map_or(0, |tree| tree.marked.len())
    }

    /// The hint under the list: the kit dialog's marking keys while the cursor is in a kit's
    /// sounds or any are marked, and otherwise the list's own.
    pub fn shown_hint(&self) -> Option<&'static str> {
        if self.focus == super::Focus::Tags {
            return Some(TAGGING_HINT);
        }
        let inside = self.marks_sounds()
            && self
                .current_index()
                .is_some_and(|item| self.kit_of(item).is_some_and(|kit| kit != item));
        if inside || self.marked_count() > 0 {
            Some(MARKING_HINT)
        } else {
            self.hint
        }
    }

    /// `Space` or `x` in the kit dialog: mark the sound under the cursor, or take its mark off,
    /// and go on to the next, as many times as the count says. A sound in another kit takes the
    /// marks off that kit's.
    pub(super) fn toggle_marks(&mut self, count: usize) {
        for _ in 0..count {
            let Some(at) = self.current_index() else {
                return;
            };
            if self.markable(at) {
                let kit = self
                    .kit_of(at)
                    .and_then(|kit| self.path(kit).map(Path::to_path_buf));
                let path = self.path(at).map(Path::to_path_buf);
                if let (Some(kit), Some(path), Some(tree)) = (kit, path, self.tree.as_mut()) {
                    tree.marked
                        .retain(|marked| marked.parent() == Some(kit.as_path()));
                    match tree.marked.iter().position(|marked| *marked == path) {
                        Some(found) => {
                            tree.marked.remove(found);
                        }
                        None => tree.marked.push(path),
                    }
                }
            }
            self.move_highlight(1);
        }
    }

    /// `a` in the kit dialog: mark every sound of the kit the cursor is on or in, opening it, or
    /// take their marks off if they're all marked already.
    pub(super) fn mark_all(&mut self) {
        let Some(kit) = self.current_index().and_then(|item| self.kit_of(item)) else {
            return;
        };
        self.set_open(kit, true);
        self.rebuild();
        let sounds: Vec<PathBuf> = self
            .children(kit)
            .iter()
            .filter(|sound| self.markable(**sound))
            .filter_map(|sound| self.path(*sound).map(Path::to_path_buf))
            .collect();
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let all = !sounds.is_empty()
            && tree.marked.len() == sounds.len()
            && sounds.iter().all(|sound| tree.marked.contains(sound));
        tree.marked = if all { Vec::new() } else { sounds };
    }

    /// What `Enter` loads with sounds marked: them, in their kit's order.
    fn marked_load(&self) -> Option<Pick> {
        let tree = self.tree.as_ref()?;
        let kit_path = tree.marked.first()?.parent()?;
        let kit = (0..tree.nodes.len())
            .find(|at| self.icon(*at) == Some(Icon::Kit) && self.path(*at) == Some(kit_path))?;
        let sounds = self
            .children(kit)
            .iter()
            .filter(|sound| self.is_marked(**sound))
            .filter_map(|sound| match &self.items[*sound].pick {
                Pick::KitSound(choice) => Some(choice.clone()),
                _ => None,
            })
            .collect();
        Some(Pick::KitLoad {
            kit: kit_path.to_path_buf(),
            sounds,
        })
    }
}

/// A search in the explorer: the tags it wants, each a word that started `#`, and the rest, which
/// a name has to hold. The last word, while it's a tag being typed, is kept apart too, for the tag
/// row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    pub tags: Vec<String>,
    pub name: String,
    pub typing: Option<String>,
}

impl Search {
    /// A search as typed. Where nothing listed has tags, `#` is only a letter of a name.
    pub fn of(query: &str, tags: bool) -> Self {
        let mut search = Self::default();
        if !tags {
            search.name = query.trim().to_owned();
            return search;
        }
        let mut name = Vec::new();
        for word in query.split_whitespace() {
            match word.strip_prefix('#') {
                Some(tag) => search.tags.push(tag.to_lowercase()),
                None => name.push(word),
            }
        }
        search.name = name.join(" ");
        let last = query.split_whitespace().last();
        if !query.ends_with(char::is_whitespace) && last.is_some_and(|w| w.starts_with('#')) {
            search.typing = search.tags.last().cloned();
        }
        search
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.name.is_empty()
    }

    /// The search without the tag being typed.
    fn without_typing(&self) -> Self {
        let mut tags = self.tags.clone();
        if self.typing.is_some() {
            tags.pop();
        }
        Self {
            tags,
            name: self.name.clone(),
            typing: None,
        }
    }

    /// Whether an item matches, and where in its name: it has a tag starting with each of the
    /// search's, which only something chosen can have, and its name holds the rest exactly,
    /// ignoring case. A `#` alone is anything chosen, so it lists all that's below the open
    /// folders.
    pub fn find(&self, item: &Item, chosen: bool) -> Option<Vec<u32>> {
        if !self.tags.is_empty() {
            let has = |wanted: &String| {
                wanted.is_empty()
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.name.starts_with(wanted.as_str()))
            };
            if !chosen || !self.tags.iter().all(has) {
                return None;
            }
        }
        if self.name.is_empty() {
            return Some(Vec::new());
        }
        exact(&item.label, &self.name)
    }
}
