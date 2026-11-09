//! The `:` command line's keys: typing, Tab completing, after Vim's `wildmenu`, and `Up` and
//! `Down` going back through what was run before, as Vim's history does, and `C-r` rolling a new
//! random name on a name line.

use ratatui::crossterm::event::KeyCode;

use crate::app::App;
use crate::config::home_folder;
use crate::input::command;
use crate::input::completion::{CommandMenu, Places, complete, replaced};
use crate::input::keys::Key;
use crate::input::mode::Mode;

impl App {
    pub(crate) fn command_key(&mut self, key: Key) {
        if self.command_menu.is_some() && self.menu_key(key) {
            return;
        }
        let menu_was_open = self.command_menu.is_some();
        match key.code {
            KeyCode::Esc => self.leave_command_line(),
            KeyCode::Enter => {
                self.leave_command_line();
                let text = std::mem::take(&mut self.command_line);
                let kept = self.command_history.add(&text);
                match command::parse(&text) {
                    Ok(line) => self.line(line),
                    Err(error) => self.status = error.to_string(),
                }
                if let Err(error) = kept
                    && self.status.is_empty()
                {
                    self.status = format!("Couldn't keep the command history: {error}");
                }
            }
            KeyCode::Tab => self.complete(false),
            KeyCode::BackTab => self.complete(true),
            KeyCode::Up => {
                if let Some(line) = self.command_history.older(&self.command_line) {
                    self.command_line = line;
                }
            }
            KeyCode::Down => {
                if let Some(line) = self.command_history.newer() {
                    self.command_line = line;
                }
            }
            KeyCode::Backspace => {
                self.command_history.stop();
                if self.command_line.pop().is_none() {
                    self.leave_command_line();
                }
            }
            KeyCode::Char('r') if key.ctrl => self.roll_name(),
            KeyCode::Char(c) if !key.ctrl => {
                self.command_history.stop();
                self.command_line.push(c);
            }
            _ => {}
        }
        if menu_was_open {
            self.narrow_menu();
        }
    }

    fn leave_command_line(&mut self) {
        self.mode = Mode::Normal;
        self.command_menu = None;
        self.command_history.stop();
    }

    /// A key while the list of matches is open, which says whether the list took it. Tab and
    /// Shift-Tab, the arrows, and `C-n` `C-p` move through it, `Enter` puts the highlighted match
    /// in the line, and `Esc` closes it, keeping what's typed.
    fn menu_key(&mut self, key: Key) -> bool {
        let Some(menu) = self.command_menu.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Tab | KeyCode::Down => menu.step(1),
            KeyCode::Char('n') if key.ctrl => menu.step(1),
            KeyCode::BackTab | KeyCode::Up => menu.step(-1),
            KeyCode::Char('p') if key.ctrl => menu.step(-1),
            KeyCode::Enter => {
                if let Some(text) = menu.highlighted() {
                    self.command_line = replaced(&self.command_line, menu.start, text);
                }
                self.command_menu = None;
            }
            KeyCode::Esc => self.command_menu = None,
            _ => return false,
        }
        true
    }

    /// Where completion looks: the settings' kit folders, and the home folder for `~`.
    fn complete_line(&self) -> crate::input::completion::Completions {
        let kit_folders = self.settings.kit_folders();
        let home = home_folder();
        complete(
            &self.command_line,
            Places {
                kit_folders: &kit_folders,
                home: home.as_deref(),
            },
        )
    }

    /// Tab: put the only match in place, or fill in the part every match shares and list them,
    /// with the first highlighted, or the last for Shift-Tab.
    fn complete(&mut self, backwards: bool) {
        self.command_history.stop();
        let found = self.complete_line();
        match found.candidates.as_slice() {
            [] => {}
            [only] => {
                self.command_line = replaced(&self.command_line, found.start, &only.text);
            }
            _ => {
                self.command_line = replaced(&self.command_line, found.start, found.common());
                let mut menu = CommandMenu {
                    picker: found.picker(),
                    start: found.start,
                };
                if backwards {
                    menu.step(-1);
                }
                self.command_menu = Some(menu);
            }
        }
    }

    /// After typing with the list open: list what still matches, keeping the highlight on the
    /// same match if it's still there, or close it once nothing does or a new word starts.
    fn narrow_menu(&mut self) {
        let Some(menu) = &self.command_menu else {
            return;
        };
        let kept = menu.highlighted().map(str::to_owned);
        let found = self.complete_line();
        if found.start != menu.start || found.candidates.is_empty() || self.mode != Mode::Command {
            self.command_menu = None;
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
        self.command_menu = Some(CommandMenu {
            picker,
            start: found.start,
        });
    }
}
