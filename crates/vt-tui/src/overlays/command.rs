//! The `:` command line: typing, Tab completing, after Vim's `wildmenu`, and `Up` and `Down` going
//! back through what was run before, as Vim's history does, and `C-r` rolling a new random name on
//! a name line.

pub(crate) mod completion;
pub(crate) mod history;
pub mod parse;

use std::path::PathBuf;
use std::time::Instant;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::overlays::Overlay;
use crate::overlays::command::completion::{CommandMenu, Completions, Places, complete, replaced};
use crate::overlays::command::history::CommandHistory;
use crate::overlays::picker::exploring::{rolled_line, taken_in};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::config::home_folder;

/// The `:` line as typed so far, the list of matches above it, and the lines run before.
#[derive(Default)]
pub struct CommandLine {
    /// What's typed after the `:`.
    pub line: String,
    /// The list of matches above the command line, while there's more than one.
    pub menu: Option<CommandMenu>,
    /// The command lines run before, which `Up` and `Down` go through.
    pub history: CommandHistory,
    /// The folder of the presets `:preset` lists here, for completion. `App` gives it before each
    /// key, as what `:preset` acts on is the track page's cursor.
    pub(crate) presets: Option<PathBuf>,
    /// Why the line just run couldn't be kept in the history, for `App` to say once the line has
    /// run, if it said nothing.
    pub(crate) unkept: Option<String>,
}

/// A key of the `:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKey {
    /// Run the line, or with the list of matches open, put its match in the line.
    Run,
    /// Leave the line, or close the list of matches, keeping what's typed.
    Close,
    /// Complete the word, or move down the list of matches.
    Complete,
    /// Complete the word from the last match, or move up the list.
    CompleteBack,
    /// An older line run before, or up the list.
    Older,
    /// A newer line run before, or down the list.
    Newer,
    /// Down the list of matches, while it's open.
    NextMatch,
    /// Up the list of matches, while it's open.
    PreviousMatch,
    /// Erase a character, or leave the line once it's empty.
    Erase,
    /// Another random name, on a name line.
    RollName,
}

impl Overlay for CommandLine {
    const MODE: Mode = Mode::Command;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        (
            "Enter",
            Cmd::InCommand(CommandKey::Run),
            "Run the line, or put in the highlighted match",
        ),
        (
            "Esc",
            Cmd::InCommand(CommandKey::Close),
            "Close, or close the list of matches",
        ),
        (
            "Tab",
            Cmd::InCommand(CommandKey::Complete),
            "Complete the word, or the next match",
        ),
        (
            "S-Tab",
            Cmd::InCommand(CommandKey::CompleteBack),
            "Complete the word, or the match before",
        ),
        (
            "Up",
            Cmd::InCommand(CommandKey::Older),
            "An older line starting with what's typed, or the match before",
        ),
        (
            "Down",
            Cmd::InCommand(CommandKey::Newer),
            "A newer line, or the next match",
        ),
        (
            "C-n",
            Cmd::InCommand(CommandKey::NextMatch),
            "In the list of matches: the next",
        ),
        (
            "C-p",
            Cmd::InCommand(CommandKey::PreviousMatch),
            "In the list of matches: the one before",
        ),
        (
            "Backspace",
            Cmd::InCommand(CommandKey::Erase),
            "Erase a character, or close once the line is empty",
        ),
        (
            "C-r",
            Cmd::InCommand(CommandKey::RollName),
            "On a name line: another random name",
        ),
    ];

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Cmd::InCommand(key) = cmd else {
            return Outcome::Passed;
        };
        let menu_was_open = self.menu.is_some();
        if menu_was_open && self.menu_key(key) {
            return Outcome::Done;
        }
        let outcome = match key {
            CommandKey::Close => {
                self.leave();
                Outcome::Close
            }
            CommandKey::Run => self.run(ctx),
            CommandKey::Complete => {
                self.complete(false, ctx);
                Outcome::Done
            }
            CommandKey::CompleteBack => {
                self.complete(true, ctx);
                Outcome::Done
            }
            CommandKey::Older => {
                if let Some(line) = self.history.older(&self.line) {
                    self.line = line;
                }
                Outcome::Done
            }
            CommandKey::Newer => {
                if let Some(line) = self.history.newer() {
                    self.line = line;
                }
                Outcome::Done
            }
            CommandKey::Erase => {
                self.history.stop();
                if self.line.pop().is_none() {
                    self.leave();
                    Outcome::Enter(Mode::Normal)
                } else {
                    Outcome::Done
                }
            }
            CommandKey::RollName => {
                self.roll_name(ctx);
                Outcome::Done
            }
            // Only the list moves with these.
            CommandKey::NextMatch | CommandKey::PreviousMatch => return Outcome::Passed,
        };
        if menu_was_open {
            self.narrow_menu(ctx);
        }
        outcome
    }

    /// Every key but `C-r`, `C-n` and `C-p` means what it does with nothing held, and those only
    /// with `Ctrl`.
    fn reads(&self, key: Key) -> Key {
        match key.code {
            KeyCode::Char(_) => Key {
                ctrl: key.ctrl,
                ..Key::new(key.code)
            },
            code => Key::new(code),
        }
    }

    /// A character is typed into the line, and any other key does nothing.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        let menu_was_open = self.menu.is_some();
        if let KeyCode::Char(c) = key.code
            && !key.ctrl
        {
            self.history.stop();
            self.line.push(c);
        }
        if menu_was_open {
            self.narrow_menu(ctx);
        }
        Outcome::Done
    }

    /// The list of matches, over the status line the `:` line is in, under the word they finish.
    /// The line itself is the status line's.
    fn draw(&mut self, frame: &mut Frame, area: Rect, _ctx: &Ctx) {
        if let Some(menu) = &mut self.menu {
            // Under the names it finishes, after the `:`.
            let column = 1 + menu.column(&self.line);
            crate::overlays::picker::draw::draw_menu(frame, area, column, &mut menu.picker);
        }
    }
}

impl CommandLine {
    /// Close the list and stop going through the history, as the line is left.
    fn leave(&mut self) {
        self.menu = None;
        self.history.stop();
    }

    /// `Enter`: leave the line and run it, keeping it in the history.
    fn run(&mut self, ctx: &mut CtxMut) -> Outcome {
        self.leave();
        let text = std::mem::take(&mut self.line);
        if let Err(error) = self.history.add(&text) {
            self.unkept = Some(format!("Couldn't keep the command history: {error}"));
        }
        match parse::parse(&text) {
            Ok(line) => Outcome::Run(line),
            Err(error) => {
                ctx.services.status = error.to_string();
                Outcome::Close
            }
        }
    }

    /// A key while the list of matches is open, which says whether the list took it. Tab and
    /// Shift-Tab, the arrows, and `C-n` `C-p` move through it, `Enter` puts the highlighted match
    /// in the line, and `Esc` closes it, keeping what's typed.
    fn menu_key(&mut self, key: CommandKey) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        match key {
            CommandKey::Complete | CommandKey::Newer | CommandKey::NextMatch => menu.step(1),
            CommandKey::CompleteBack | CommandKey::Older | CommandKey::PreviousMatch => {
                menu.step(-1);
            }
            CommandKey::Run => {
                if let Some(text) = menu.highlighted() {
                    self.line = replaced(&self.line, menu.start, text);
                }
                self.menu = None;
            }
            CommandKey::Close => self.menu = None,
            CommandKey::Erase | CommandKey::RollName => return false,
        }
        true
    }

    /// Where completion looks: the settings' kit folders, the presets of what `:preset` acts on
    /// here, and the home folder for `~`.
    fn completions(&self, ctx: &CtxMut) -> Completions {
        let kit_folders = ctx.services.settings.kit_folders();
        let patches = ctx
            .services
            .settings
            .preset_folder()
            .map(|folder| folder.join(vt_model::preset::PresetKind::Patch.folder()));
        let home = home_folder();
        complete(
            &self.line,
            Places {
                kit_folders: &kit_folders,
                home: home.as_deref(),
                presets: self.presets.as_deref(),
                patches: patches.as_deref(),
            },
        )
    }

    /// Tab: put the only match in place, or fill in the part every match shares and list them,
    /// with the first highlighted, or the last for Shift-Tab.
    fn complete(&mut self, backwards: bool, ctx: &CtxMut) {
        self.history.stop();
        let found = self.completions(ctx);
        match found.candidates.as_slice() {
            [] => {}
            [only] => self.line = replaced(&self.line, found.start, &only.text),
            _ => {
                self.line = replaced(&self.line, found.start, found.common());
                let mut menu = CommandMenu {
                    picker: found.picker(),
                    start: found.start,
                };
                if backwards {
                    menu.step(-1);
                }
                self.menu = Some(menu);
            }
        }
    }

    /// After typing with the list open: list what still matches, keeping the highlight on the
    /// same match if it's still there, or close it once nothing does or a new word starts.
    fn narrow_menu(&mut self, ctx: &CtxMut) {
        let Some(menu) = &self.menu else {
            return;
        };
        let kept = menu.highlighted().map(str::to_owned);
        let found = self.completions(ctx);
        if found.start != menu.start || found.candidates.is_empty() {
            self.menu = None;
            return;
        }
        let mut picker = found.picker();
        if let Some(at) = found
            .candidates
            .iter()
            .position(|candidate| Some(&candidate.text) == kept.as_ref())
        {
            picker.selected = at;
        }
        self.menu = Some(CommandMenu {
            picker,
            start: found.start,
        });
    }

    /// `C-r` on a name line: put a new random name in place of the one there, keeping the folder,
    /// the extension and any flags, and not one already in that folder.
    fn roll_name(&mut self, ctx: &mut CtxMut) {
        let names = &mut ctx.services.names;
        let rolled = rolled_line(&self.line, |folder| {
            let folder = if folder.is_empty() {
                PathBuf::from(".")
            } else {
                crate::services::config::expand_home(folder)
            };
            let taken = taken_in(&folder);
            names.fresh(|name| taken.contains(name))
        });
        if let Some(line) = rolled {
            self.line = line;
            self.menu = None;
            self.history.stop();
        }
    }
}

impl App {
    /// Give the `:` line a key. Once it's left by `Esc` or run, the sections selected for it are
    /// done with, as Vim's `:'<,'>` ends Visual, and a line the history couldn't keep says so if
    /// what it ran said nothing.
    pub(crate) fn command_line_key(&mut self, key: Key, now: Instant) {
        self.overlays.command.presets = self
            .track_page
            .presets_here(self.modes.view, &self.services);
        let outcome = crate::overlays::give(
            &mut self.overlays.command,
            &self.table,
            &mut self.grammar,
            CtxMut {
                services: &mut self.services,
                modes: &self.modes,
                wheel: &mut self.wheel,
            },
            key,
            now,
        );
        let left = matches!(outcome, Outcome::Close | Outcome::Run(_));
        if let Outcome::Run(_) = outcome {
            // The line is left before it runs, so what it opens is what's open after.
            self.modes.mode = Mode::Normal;
        }
        self.settled(outcome, false);
        if left {
            self.song_view.anchor = None;
            if let Some(unkept) = self.overlays.command.unkept.take()
                && self.services.status.is_empty()
            {
                self.services.status = unkept;
            }
        }
    }
}
