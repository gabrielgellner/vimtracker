//! Opening the picker and acting on what it chooses.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;

use crate::app::App;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How many files the sample browser lists, and how many folders deep it looks.
const MAX_SAMPLES: usize = 2000;
const MAX_DEPTH: usize = 4;

/// Find WAV files under a folder, skipping hidden ones, in a stable order.
pub fn find_wavs(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(folder, 0, &mut found);
    found
}

fn walk(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MAX_SAMPLES {
            return;
        }
        let hidden = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        {
            found.push(path);
        }
    }
}

impl App {
    /// Open the view picker, with the view before this one highlighted so `Enter` flips back.
    pub(crate) fn open_view_picker(&mut self) {
        self.count = None;
        let items = self
            .recent_views
            .iter()
            .map(|view| Item {
                label: view.name().to_owned(),
                pick: Pick::View(*view),
            })
            .collect();
        let mut picker = Picker::new("Views", items);
        picker.move_highlight(1);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Open the sample browser on a folder. If nothing is there, offer a typed path instead.
    pub(crate) fn open_sample_picker(&mut self, roots: &[PathBuf]) {
        let items: Vec<Item> = roots
            .iter()
            .flat_map(|root| {
                find_wavs(root).into_iter().map(move |path| Item {
                    label: path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                    pick: Pick::Sample(path),
                })
            })
            .collect();
        if items.is_empty() {
            self.status = "No WAV files found here: type a path, or a folder of them".to_owned();
            self.command_line = "sample ".to_owned();
            self.mode = Mode::Command;
            return;
        }
        self.picker = Some(Picker::new("Samples", items));
        self.mode = Mode::Picker;
    }

    /// The folders the sample browser starts from: the project's samples, then where we ran.
    pub(crate) fn sample_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(folder) = &self.folder {
            roots.push(folder.join("samples"));
        }
        if let Ok(current) = std::env::current_dir()
            && !self
                .folder
                .as_deref()
                .is_some_and(|f| current.starts_with(f))
        {
            roots.push(current);
        }
        roots
    }

    pub(crate) fn picker_key(&mut self, key: Key) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.picker = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let pick = picker.current().map(|item| item.pick.clone());
                self.picker = None;
                self.mode = Mode::Normal;
                match pick {
                    Some(Pick::View(view)) => self.jump_to_view(view),
                    Some(Pick::Use { track, slot }) => self.open_use(track, slot),
                    Some(Pick::Sample(path)) => self.sample(&path),
                    None => {}
                }
            }
            KeyCode::Down => picker.move_highlight(1),
            KeyCode::Up => picker.move_highlight(-1),
            KeyCode::Char('n') if key.ctrl => picker.move_highlight(1),
            KeyCode::Char('p') if key.ctrl => picker.move_highlight(-1),
            KeyCode::Backspace => {
                let mut query = picker.query.clone();
                query.pop();
                picker.set_query(query);
            }
            KeyCode::Char(c) if !key.ctrl => {
                let query = format!("{}{c}", picker.query);
                picker.set_query(query);
            }
            _ => {}
        }
    }

    /// Record a view as most recently used.
    pub(crate) fn note_view(&mut self, view: View) {
        self.recent_views.retain(|other| *other != view);
        self.recent_views.insert(0, view);
    }
}
