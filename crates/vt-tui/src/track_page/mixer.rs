//! The mixer's cursor, which goes up and down a strip's settings, and across the strips with `h`
//! and `l`, through the track page's controls.

use vt_model::action::TrackControl;
use vt_model::selection::Motion;
use vt_model::song::Track;

use crate::app::App;
use crate::input::bindings::Cmd;

use super::Control;

/// The mixer's columns.
pub const MIXER: [Control; 7] = [
    Control::Trim,
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::SendA),
    Control::Base(TrackControl::SendB),
    Control::Mute,
    Control::Solo,
];

/// The mixer's row it opens on, its `Level`, as it's for the mix.
pub const MIXER_START: usize = 1;

/// The mixer's columns, as rows like a page's.
pub const MIXER_ROWS: [&[Control]; 1] = [&MIXER];

impl App {
    /// The mixer's rows this track has: a fixed track has no sends, and the master no pan, mute or
    /// solo.
    fn mixer_rows_here(&self) -> Vec<usize> {
        let track = self.song().tracks.get(self.cursor.track);
        (0..MIXER.len())
            .filter(|row| track.is_none_or(|track| MIXER[*row].exists_on(track)))
            .collect()
    }

    /// The row the cursor is on. On a track without the row it was on, it's the nearest above it,
    /// and the row it was on is kept, so going back to a track that has it finds it again.
    pub(super) fn mixer_row_here(&self) -> usize {
        let rows = self.mixer_rows_here();
        let wanted = self.mixer_row.min(MIXER.len() - 1);
        rows.iter()
            .rev()
            .find(|row| **row <= wanted)
            .or(rows.first())
            .copied()
            .unwrap_or(0)
    }

    /// Go to the first or last track that has the row the cursor is on, staying on that row.
    fn mixer_row_end(&mut self, last: bool) {
        let row = self.mixer_row_here();
        let has = |track: &Track| MIXER[row].exists_on(track);
        let tracks = &self.song().tracks;
        // The first or last strip shown, as a folded one is stepped over.
        let found = {
            let mut shown = self
                .folds
                .shown(self.song())
                .filter(|&track| has(&tracks[track]));
            if last { shown.last() } else { shown.next() }
        };
        if let Some(to) = found {
            self.go_to_track_at(to);
            self.mixer_row = row;
        }
    }

    /// Move up or down the mixer's rows, going only through those this track has.
    fn move_mixer_row(&mut self, by: isize) {
        let rows = self.mixer_rows_here();
        let here = self.mixer_row_here();
        let at = rows.iter().position(|row| *row == here).unwrap_or(0);
        let to = (at as isize)
            .saturating_add(by)
            .clamp(0, rows.len() as isize - 1);
        self.mixer_row = rows.get(to as usize).copied().unwrap_or(0);
    }

    /// Run a command on the mixer. `h` and `l` go between its strips, and `j` and `k` up and down
    /// them. Everything else means what it does on the track page.
    pub(crate) fn run_mixer(&mut self, cmd: Cmd) -> bool {
        let times = self.count.unwrap_or(1).max(1) as isize;
        match cmd {
            Cmd::Move(Motion::Right) => self.move_track(times),
            Cmd::Move(Motion::Left) => self.move_track(-times),
            Cmd::Move(Motion::RowStart) => self.mixer_row_end(false),
            Cmd::Move(Motion::RowEnd) => self.mixer_row_end(true),
            Cmd::Move(Motion::Down) => self.move_mixer_row(times),
            Cmd::Move(Motion::Up) => self.move_mixer_row(-times),
            Cmd::Move(Motion::First) => self.move_mixer_row(isize::MIN),
            Cmd::Move(Motion::Last) => self.move_mixer_row(isize::MAX),
            _ => return self.run_track_page(cmd),
        }
        self.count = None;
        true
    }
}
