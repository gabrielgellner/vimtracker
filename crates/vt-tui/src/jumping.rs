//! `/` on the track page or the mixer: type part of a setting's name, then a label to land on its
//! value. The rules for labels are those of flash.nvim's `labeler.lua`.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::track_page::{Control, MIXER_ROWS};

/// The labels, with the easiest keys first.
const LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// A search under way.
#[derive(Debug, Clone, Default)]
pub struct FieldJump {
    pub query: String,
    labels: Vec<(Control, char)>,
}

impl FieldJump {
    /// The label on a setting, if it has one.
    pub fn label(&self, control: Control) -> Option<char> {
        self.labels
            .iter()
            .find(|(labelled, _)| *labelled == control)
            .map(|(_, label)| *label)
    }

    fn control(&self, label: char) -> Option<Control> {
        self.labels
            .iter()
            .find(|(_, used)| *used == label)
            .map(|(control, _)| *control)
    }
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
    /// The settings a search looks through. The mixer's are those of the cursor's track, whose row
    /// the cursor stays on.
    fn jump_rows(&self) -> &'static [&'static [Control]] {
        if self.view == View::Mixer {
            &MIXER_ROWS
        } else {
            self.page_rows()
        }
    }

    /// The settings on this page whose names hold the search, nearest the cursor first, with their
    /// names in lower case.
    fn jump_hits(&self, query: &str) -> Vec<(Control, String)> {
        if query.is_empty() {
            return Vec::new();
        }
        let voice = self
            .song()
            .tracks
            .get(self.cursor.track)
            .map(|track| track.synth_voice)
            .unwrap_or_default();
        let (from_row, from_field) = if self.view == View::Mixer {
            (0, self.mixer_column)
        } else {
            (self.track_row, self.track_field)
        };
        let mut hits: Vec<_> = self
            .jump_rows()
            .iter()
            .enumerate()
            .flat_map(|(row, controls)| {
                controls
                    .iter()
                    .enumerate()
                    .map(move |(field, control)| (row, field, *control))
            })
            .map(|(row, field, control)| {
                let name = control.label_on(voice).to_lowercase();
                (row, field, control, name)
            })
            .filter(|(_, _, _, name)| name.contains(query))
            .collect();
        hits.sort_by_key(|(row, field, _, _)| (row.abs_diff(from_row), field.abs_diff(from_field)));
        hits.into_iter()
            .map(|(_, _, control, name)| (control, name))
            .collect()
    }

    /// Start a search.
    pub(crate) fn open_field_jump(&mut self) {
        self.count = None;
        self.field_jump = Some(FieldJump::default());
        self.mode = Mode::Jump;
    }

    fn close_field_jump(&mut self) {
        self.field_jump = None;
        self.mode = Mode::Normal;
    }

    /// Search for this, if anything on the page matches it. A typing slip doesn't lose the search.
    fn set_jump_query(&mut self, query: String) {
        let hits = self.jump_hits(&query);
        if hits.is_empty() && !query.is_empty() {
            return;
        }
        if let Some(jump) = &mut self.field_jump {
            jump.labels = labelled(&hits, &query, &jump.labels);
            jump.query = query;
        }
    }

    /// Land on a setting's value, in a place `Ctrl-O` can come back from.
    fn land_on(&mut self, control: Control) {
        self.close_field_jump();
        let place = self
            .jump_rows()
            .iter()
            .enumerate()
            .find_map(|(row, controls)| {
                let field = controls.iter().position(|field| *field == control)?;
                Some((row, field))
            });
        if let Some((row, field)) = place {
            self.record_jump();
            if self.view == View::Mixer {
                self.mixer_column = field;
            } else {
                self.move_control(row, field);
            }
        }
    }

    /// Handle a key during a search.
    pub(crate) fn jump_key(&mut self, key: Key, now: Instant) {
        let Some(jump) = self.field_jump.clone() else {
            return;
        };
        match key.code {
            KeyCode::Tab => self.run(Cmd::PauseOrResume, key, now),
            KeyCode::BackTab => self.run(Cmd::PlayFromStart, key, now),
            KeyCode::Esc => self.close_field_jump(),
            KeyCode::Enter => match self.jump_hits(&jump.query).first() {
                Some((control, _)) => self.land_on(*control),
                None => self.close_field_jump(),
            },
            KeyCode::Backspace if jump.query.is_empty() => self.close_field_jump(),
            KeyCode::Backspace => {
                let mut query = jump.query;
                query.pop();
                self.set_jump_query(query);
            }
            KeyCode::Char(c) => match jump.control(c) {
                Some(control) => self.land_on(control),
                None => {
                    let query = format!("{}{}", jump.query, c.to_lowercase());
                    self.set_jump_query(query);
                }
            },
            _ => {}
        }
    }
}
