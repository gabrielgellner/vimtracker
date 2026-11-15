//! `/` on the track page or the mixer: type part of a setting's name, then a label to land on its
//! value. The rules for labels are those of flash.nvim's `labeler.lua`.

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;

use crate::app::App;
use crate::controls::Control;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::overlays::Overlay;
use crate::pages::mixer::MIXER_ROWS;
use crate::pages::{Ctx, CtxMut, Outcome};

/// The labels, with the easiest keys first.
const LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// A search under way.
#[derive(Debug, Clone, Default)]
pub struct FieldJump {
    pub query: String,
    labels: Vec<(Control, char)>,
    /// The page's settings, by row and field, and the row and field the cursor is on, which
    /// `App` gives it before each key, as only the page knows them.
    page: JumpPage,
}

/// The settings a search looks through, by row and field, and where the cursor is among them.
#[derive(Debug, Clone, Default)]
pub(crate) struct JumpPage {
    pub rows: Vec<Vec<Option<Control>>>,
    pub from: (usize, usize),
}

/// A key of a field jump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpKey {
    /// `Tab` and `Shift-Tab` play as they do in Normal.
    Play,
    PlayFromStart,
    /// Land on the nearest match.
    Land,
    /// Erase a letter of the search, or leave it once it's empty.
    Erase,
    Close,
}

impl FieldJump {
    /// The label on a setting, if it has one.
    pub fn label(&self, control: Control) -> Option<char> {
        self.labels
            .iter()
            .find(|(labelled, _)| *labelled == control)
            .map(|(_, label)| *label)
    }

    /// The label on a setting as text, or nothing: one of `LABELS`, as a cell draws it.
    pub(crate) fn label_text(&self, control: Control) -> &'static str {
        self.label(control)
            .and_then(|label| {
                LABELS
                    .find(label)
                    .map(|at| &LABELS[at..at + label.len_utf8()])
            })
            .unwrap_or("")
    }

    fn control(&self, label: char) -> Option<Control> {
        self.labels
            .iter()
            .find(|(_, used)| *used == label)
            .map(|(control, _)| *control)
    }

    /// Give it the page's settings and the cursor's place among them, as they are now.
    pub(crate) fn lend(&mut self, page: JumpPage) {
        self.page = page;
    }

    /// The settings on this page whose names hold the search, nearest the cursor first, with
    /// their names in lower case.
    fn hits(&self, query: &str, ctx: &CtxMut) -> Vec<(Control, String)> {
        if query.is_empty() {
            return Vec::new();
        }
        let Some(track) = ctx.track_here() else {
            return Vec::new();
        };
        let (from_row, from_field) = self.page.from;
        let mut hits: Vec<_> = self
            .page
            .rows
            .iter()
            .enumerate()
            .flat_map(|(row, controls)| {
                controls
                    .iter()
                    .enumerate()
                    .filter_map(move |(field, control)| Some((row, field, (*control)?)))
            })
            .filter(|(_, _, control)| control.exists_on(track))
            .map(|(row, field, control)| {
                let name = control.name_on(track).to_lowercase();
                (row, field, control, name)
            })
            .filter(|(_, _, _, name)| name.contains(query))
            .collect();
        hits.sort_by_key(|(row, field, _, _)| (row.abs_diff(from_row), field.abs_diff(from_field)));
        hits.into_iter()
            .map(|(_, _, control, name)| (control, name))
            .collect()
    }

    /// Search for this, if anything on the page matches it. A typing slip doesn't lose the search.
    fn set_query(&mut self, query: String, ctx: &CtxMut) {
        let hits = self.hits(&query, ctx);
        if hits.is_empty() && !query.is_empty() {
            return;
        }
        self.labels = labelled(&hits, &query, &self.labels);
        self.query = query;
    }

    /// Land on a setting's value, which `App` does, in a place `Ctrl-O` can come back from.
    fn land_on(&self, control: Control) -> Outcome {
        let place = self
            .page
            .rows
            .iter()
            .enumerate()
            .find_map(|(row, controls)| {
                let field = controls.iter().position(|field| *field == Some(control))?;
                Some((row, field))
            });
        match place {
            Some((row, field)) => Outcome::Land { row, field },
            None => Outcome::Close,
        }
    }
}

impl Overlay for FieldJump {
    const MODE: Mode = Mode::Jump;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("Tab", Cmd::InJump(JumpKey::Play), "Play, or pause in place"),
        (
            "S-Tab",
            Cmd::InJump(JumpKey::PlayFromStart),
            "Play from the start",
        ),
        (
            "Enter",
            Cmd::InJump(JumpKey::Land),
            "Land on the nearest match",
        ),
        (
            "Backspace",
            Cmd::InJump(JumpKey::Erase),
            "Erase a letter, or close once the search is empty",
        ),
        ("Esc", Cmd::InJump(JumpKey::Close), "Close"),
    ];

    fn key(&mut self, cmd: Cmd, ctx: &mut CtxMut) -> Outcome {
        let Cmd::InJump(key) = cmd else {
            return Outcome::Passed;
        };
        match key {
            JumpKey::Play => Outcome::Do(Cmd::PauseOrResume),
            JumpKey::PlayFromStart => Outcome::Do(Cmd::PlayFromStart),
            JumpKey::Close => Outcome::Close,
            JumpKey::Land => match self.hits(&self.query, ctx).first() {
                Some((control, _)) => self.land_on(*control),
                None => Outcome::Close,
            },
            JumpKey::Erase if self.query.is_empty() => Outcome::Close,
            JumpKey::Erase => {
                let mut query = self.query.clone();
                query.pop();
                self.set_query(query, ctx);
                Outcome::Done
            }
        }
    }

    /// Every key means what it does with nothing held.
    fn reads(&self, key: Key) -> Key {
        Key::new(key.code)
    }

    /// A label lands on its setting, and any other character is the search's next letter.
    fn text(&mut self, key: Key, ctx: &mut CtxMut) -> Outcome {
        let KeyCode::Char(c) = key.code else {
            return Outcome::Done;
        };
        match self.control(c) {
            Some(control) => self.land_on(control),
            None => {
                let query = format!("{}{}", self.query, c.to_lowercase());
                self.set_query(query, ctx);
                Outcome::Done
            }
        }
    }

    /// Nothing of its own: the pages draw its labels over their settings, and the status line its
    /// search.
    fn draw(&mut self, _frame: &mut Frame, _area: Rect, _ctx: &Ctx) {}
}

/// The labels for some matches, nearest first, with their names. A label that could be the next
/// letter of a search isn't used, and a match keeps a label it already has.
fn labelled(
    hits: &[(Control, String)],
    query: &str,
    kept: &[(Control, char)],
) -> Vec<(Control, char)> {
    let next_letters: Vec<char> = hits
        .iter()
        .flat_map(|(_, name)| {
            name.match_indices(query)
                .filter_map(|(at, _)| name[at + query.len()..].chars().next())
        })
        .collect();
    let mut free: Vec<char> = LABELS
        .chars()
        .filter(|c| !next_letters.contains(c))
        .collect();
    let mut labels: Vec<(Control, char)> = kept
        .iter()
        .filter(|(control, label)| {
            free.contains(label) && hits.iter().any(|(hit, _)| hit == control)
        })
        .copied()
        .collect();
    free.retain(|label| !labels.iter().any(|(_, used)| used == label));
    let mut free = free.into_iter();
    for (control, _) in hits {
        if !labels.iter().any(|(labelled, _)| labelled == control) {
            let Some(label) = free.next() else {
                break;
            };
            labels.push((*control, label));
        }
    }
    labels
}

impl App {
    /// The settings a search looks through, and the cursor's place among them. The mixer's are
    /// those of the cursor's track, whose row the cursor stays on.
    pub(crate) fn jump_page(&self) -> JumpPage {
        if self.modes.view == View::Mixer {
            JumpPage {
                rows: MIXER_ROWS
                    .iter()
                    .map(|row| row.iter().copied().map(Some).collect())
                    .collect(),
                from: (0, self.mixer_page.row),
            }
        } else {
            JumpPage {
                rows: self.track_page.rows(self.services.track_here()),
                from: (self.track_page.row, self.track_page.field),
            }
        }
    }

    /// Start a search.
    pub(crate) fn open_field_jump(&mut self) {
        self.modes.count = None;
        self.overlays.field_jump = Some(FieldJump::default());
        self.modes.mode = Mode::Jump;
    }
}
