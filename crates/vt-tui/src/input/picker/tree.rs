//! The explorer's tree: its folders, read as they're opened and closed, and the
//! `Backspace`, `H` and `h` that go up, show hidden files and close a folder.

use std::path::{Path, PathBuf};

use crate::explorer::{self, Icon, Purpose};

use super::{Item, Outcome, Pick, Picker, Row};

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
    roots: Vec<Root>,
    home: Option<PathBuf>,
    pub(super) nodes: Vec<Node>,
    /// The roots' nodes, in order.
    pub(super) top: Vec<usize>,
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
    /// what's been read that matches and the folders it's in, open or not.
    pub(super) fn tree_rows(&self) -> (Vec<Row>, Vec<Vec<u32>>) {
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
}
