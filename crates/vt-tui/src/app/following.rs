//! Gen beside the grid follows the grid's cursor (decision 468): while the keys are in the grid,
//! the panel's cursor goes to the device of the cursor's column and the status line names it;
//! while they're in the panel, its device leads, lighting its rows in the grid, and the column it
//! writes shows its values as the bars `S` draws, with what they were when the keys came into the
//! device fainter behind.

use std::sync::Arc;

use vt_model::selection::StepRange;
use vt_model::shape::ShapeColumn;
use vt_model::song::Song;

use crate::app::App;
use crate::input::mode::View;
use crate::pages::track::following::{Followed, followed, followed_said};
use crate::widgets::value_bars::ValueBars;

/// The device the keys are in beside the grid, by its track, its phrase and its live generator's
/// place in the phrase's list, with the song as it was when they came into it.
#[derive(Debug, Clone)]
pub(crate) struct Entered {
    track: usize,
    phrase: u32,
    generator: usize,
    song: Arc<Song>,
}

impl App {
    /// What the panel follows on the grid's cursor, beside the grid, on the phrase under it.
    fn followed_here(&self) -> Option<Followed> {
        let phrase = self.services.phrase_here()?;
        let cursor = self.services.cursor;
        Some(followed(phrase, cursor.field, cursor.step))
    }

    /// The panel's cursor onto the device the grid's cursor chooses, as `Space 8` opens it
    /// beside the grid.
    pub(super) fn follow_grid_cursor(&mut self) {
        let Some(followed) = self.followed_here() else {
            return;
        };
        self.gen_page.phrase = self.services.phrase_id();
        let track = self.services.track_here().cloned();
        self.gen_page.follow(track.as_ref(), followed);
    }

    /// After a key, beside the grid: with the keys in the grid, the panel follows its cursor and
    /// the status line says what it's on as that changes, unless the key said something of its
    /// own;
    /// with them in the panel, the device they're in is noted with the song `before` the key, the
    /// first time they're in it, for its bars' faint values.
    pub(super) fn follow_after_key(&mut self, before: &Arc<Song>, said: &str) {
        if !self.gen_beside_shown() {
            self.gen_entered = None;
            self.gen_followed = None;
            return;
        }
        let track = self.services.cursor.track;
        let phrase = self.services.phrase_id();
        if self.modes.view == View::Phrase {
            self.gen_entered = None;
            let Some((followed, phrase)) = self.followed_here().zip(phrase) else {
                return;
            };
            self.follow_grid_cursor();
            let now = Some((track, phrase, followed));
            if self.gen_followed != now {
                self.gen_followed = now;
                let quiet = self.services.status.is_empty() || self.services.status == said;
                if quiet && let Some(found) = self.services.phrase_here()
                {
                    self.services.status = followed_said(found, followed);
                }
            }
            return;
        }
        self.gen_followed = None;
        let found = self.services.track_here().cloned();
        let generator = self.gen_page.device_generator(found.as_ref());
        let here = phrase.zip(generator);
        let same = self.gen_entered.as_ref().is_some_and(|entered| {
            Some((entered.phrase, entered.generator)) == here && entered.track == track
        });
        if !same {
            self.gen_entered = here.map(|(phrase, generator)| Entered {
                track,
                phrase,
                generator,
                song: Arc::clone(before),
            });
        }
    }

    /// The rows the panel's device lights in the grid while the keys are in it, and their
    /// column.
    pub(crate) fn gen_lit(&self) -> Option<(ShapeColumn, StepRange)> {
        if self.modes.view != View::Gen || !self.gen_beside_shown() {
            return None;
        }
        self.gen_page.lit_rows(self.services.track_here())
    }

    /// The bars of the column the device the keys are in writes, beside the grid: each row of its
    /// span as it is now, and as it was when the keys came into the device. None for a column
    /// whose values aren't amounts, or with the keys in the grid.
    pub(crate) fn gen_bars(&self) -> Option<ValueBars> {
        if self.modes.view != View::Gen || !self.gen_beside_shown() {
            return None;
        }
        let entered = self.gen_entered.as_ref()?;
        let phrase = self.services.phrase_here()?;
        if self.services.cursor.track != entered.track
            || self.services.phrase_id() != Some(entered.phrase)
        {
            return None;
        }
        let generator = phrase.generators().get(entered.generator)?;
        let before = entered
            .song
            .tracks
            .get(entered.track)
            .and_then(|track| track.phrases.get(&entered.phrase))
            .map_or(phrase.steps(), |was| was.steps());
        ValueBars::of_steps(generator.column, generator.rows()?, before, phrase.steps())
    }
}
