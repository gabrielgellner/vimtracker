//! The track page, `Space 4`: a track's Sound, Mod and FX pages. `TrackPage` is the page as a
//! component, which page shows and where its cursor is, with the moves that need only the cursor's
//! track; the edits, which change the song, are `App`'s. Each page's settings are in a file of its
//! own (`sound`, `modulation`, `fx`), each gives its devices to `Page::devices` if it's a rack
//! (`device`), and `editing`, `pickers` and `typing` change the setting under the cursor.

mod controls;
mod device;
mod editing;
mod fx;
mod mixer;
mod modulation;
mod pickers;
mod sound;
mod typing;

pub use controls::{AccumulatorField, ArpField, Control, EnvelopeField, LfoField};
pub use device::{
    DEVICE_COLUMNS, DEVICE_ROWS, DeviceCursor, DeviceSpec, DeviceWidth, RackPlace, rack_places,
};
pub use fx::TakenInsert;
pub use mixer::{MIXER, MIXER_ROWS, MIXER_START};
pub(crate) use modulation::modulator_bypassed;

use std::sync::Arc;

use vt_model::selection::Motion;
use vt_model::song::Track;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use device::{device_beside, rack_column};

/// A part of the track page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Sound,
    Mod,
    Fx,
}

impl Page {
    /// Every page, in order.
    pub const ALL: [Self; 3] = [Self::Sound, Self::Mod, Self::Fx];

    /// The page's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Mod => "Mod",
            Self::Fx => "FX",
        }
    }

    /// The page next to this one, or this one at either end.
    fn beside(self, next: bool) -> Self {
        let at = Self::ALL.iter().position(|page| *page == self).unwrap_or(0);
        let to = if next { at + 1 } else { at.saturating_sub(1) };
        Self::ALL[to.min(Self::ALL.len() - 1)]
    }

    /// The page's devices, in the order they're read, each page's from its own file: the Sound
    /// page's machine, filter, output and play, or a fixed track's output alone, the Mod page's
    /// envelopes, LFOs, accumulator and arpeggio, and the FX page's slots. The cursor, `j`, `k`,
    /// `h`, `l`, `{` and `}` go through whatever devices a page gives, laid out as its rack
    /// ([`rack_places`]).
    pub fn devices(self, track: &Track) -> Vec<DeviceSpec> {
        match self {
            Self::Sound => sound::devices(track),
            Self::Mod => modulation::devices(track),
            Self::Fx => fx::devices(track),
        }
    }
}

/// The place nearest a row and field that holds a setting: in a row with none, the nearest row
/// above with some, and on a hole, the nearest setting along the row, the left one of two as near.
/// In a rack it's the nearest row in the same device, above or else below, so a device's title
/// with nothing in it, as the Sound page's `Output` and the accumulator have, stands for its first
/// row with some, and `}` onto it lands in the device rather than above it.
fn place_in(rows: &[Vec<Option<Control>>], rack: bool, row: usize, field: usize) -> (usize, usize) {
    let row = row.min(rows.len().saturating_sub(1));
    let filled = |row: &usize| {
        rows.get(*row)
            .is_some_and(|row| row.iter().any(Option::is_some))
    };
    let row = if rack {
        let top = row - row % DEVICE_ROWS;
        let bottom = (top + DEVICE_ROWS).min(rows.len());
        let above = (top..=row).rev().find(filled);
        above.or_else(|| (row..bottom).find(filled)).unwrap_or(row)
    } else {
        (0..=row).rev().find(filled).unwrap_or(0)
    };
    let places = rows.get(row).map_or(&[][..], Vec::as_slice);
    let field = (0..places.len())
        .filter(|at| places[*at].is_some())
        .min_by_key(|at| (at.abs_diff(field), *at > field))
        .unwrap_or(0);
    (row, field)
}

/// What a key did on the track page, as far as the page alone can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The page did it.
    Done,
    /// It can't be done here, and the status line says why.
    Refused(&'static str),
    /// It isn't one of the page's own keys: an edit, which needs the song, or what it means
    /// anywhere.
    Passed,
}

/// The track page as a component: which of its pages shows, and where its cursor is, kept while
/// other views are shown. What's on a page depends on the cursor's track, so each move is given
/// that track, or none, and the page holds no song of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackPage {
    /// The page showing: Sound, Mod or FX.
    pub page: Page,
    /// The row and field the cursor is kept at. It can be on a hole or past the end of a shorter
    /// row, as after `}` onto an empty slot, so where it's seen is [`TrackPage::place`].
    pub row: usize,
    pub field: usize,
    /// Where `j` and `k` last left the cursor, as its row and field, and the field it was on before
    /// a shorter row moved it in. Moving up or down from there goes back to that field where a row
    /// has it, as Vim keeps its column.
    pub(crate) wanted: Option<(usize, usize, usize)>,
    /// Whether the cursor is on the fine digits of the setting.
    pub fine: bool,
}

impl TrackPage {
    /// The page's devices, or none with no track.
    pub fn devices(&self, track: Option<&Track>) -> Vec<DeviceSpec> {
        track.map_or_else(Vec::new, |track| self.page.devices(track))
    }

    /// The page's rows as the cursor goes through them, each device's in turn, each a list of
    /// places that can be holes it steps over.
    pub fn rows(&self, track: Option<&Track>) -> Vec<Vec<Option<Control>>> {
        self.devices(track)
            .iter()
            .flat_map(DeviceSpec::rows)
            .map(Vec::from)
            .collect()
    }

    /// Where the cursor is, as a row and a place in it that hold a setting. In a row with none,
    /// such as an empty slot's grid, it's the nearest row above with some, which in a slot is its
    /// title at least, and in a device whose title has nothing, such as the accumulator's, its
    /// first row. On a hole, it's the nearest setting along the row, the left one of two as near.
    pub fn place(&self, track: Option<&Track>) -> (usize, usize) {
        self.nearest(track, self.row, self.field)
    }

    /// The place nearest a row and field that holds a setting, as [`TrackPage::place`] finds it.
    fn nearest(&self, track: Option<&Track>, row: usize, field: usize) -> (usize, usize) {
        let rack = !self.devices(track).is_empty();
        place_in(&self.rows(track), rack, row, field)
    }

    /// On a rack, the devices in a device's column, top to bottom, which `j`, `k`, `{`, `}`, `gg`
    /// and `G` keep to. Elsewhere, nothing.
    fn column(&self, track: Option<&Track>, device: usize) -> Option<Vec<usize>> {
        let devices = self.devices(track);
        (!devices.is_empty()).then(|| rack_column(&devices, device.min(devices.len() - 1)))
    }

    /// The first row of the page, or its last, for `gg` and `G`: on a rack, of the top or the
    /// bottom device in the cursor's column.
    fn end_row(&self, last: bool, track: Option<&Track>) -> usize {
        let Some(column) = self.column(track, self.row / DEVICE_ROWS) else {
            return if last { usize::MAX } else { 0 };
        };
        match (last, column.first(), column.last()) {
            (false, Some(top), _) => top * DEVICE_ROWS,
            (true, _, Some(bottom)) => bottom * DEVICE_ROWS + DEVICE_ROWS - 1,
            _ => 0,
        }
    }

    /// What the cursor is on.
    pub fn control(&self, track: Option<&Track>) -> Control {
        let (row, field) = self.place(track);
        let rows = self.rows(track);
        let place = rows
            .get(row)
            .and_then(|row| row.get(field).copied().flatten());
        place.unwrap_or(Control::Machine)
    }

    /// Where the cursor is in the page's rack, on a page that's one.
    pub fn device_cursor(&self, track: Option<&Track>) -> Option<DeviceCursor> {
        if self.devices(track).is_empty() {
            return None;
        }
        let (row, column) = self.place(track);
        Some(DeviceCursor {
            device: row / DEVICE_ROWS,
            row: row % DEVICE_ROWS,
            column,
        })
    }

    /// Put the cursor at the place nearest a row and field that holds a setting, off any fine
    /// digits.
    pub fn move_to(&mut self, track: Option<&Track>, row: usize, field: usize) {
        (self.row, self.field) = self.nearest(track, row, field);
        self.fine = false;
    }

    /// Run one of the page's own keys: the moves, `{` and `}` across a rack's devices, and the
    /// keys to the pages either side. `times` is its count, at least 1.
    pub fn key(&mut self, cmd: Cmd, times: usize, track: Option<&Track>) -> Outcome {
        let field = self.field;
        match cmd {
            Cmd::Move(Motion::Down) => (0..times).for_each(|_| self.step_row(1, track)),
            Cmd::Move(Motion::Up) => (0..times).for_each(|_| self.step_row(-1, track)),
            Cmd::Move(Motion::NextBeat | Motion::PreviousBeat)
                if self.devices(track).is_empty() =>
            {
                return Outcome::Refused("{ and } go between devices, and this page has none");
            }
            Cmd::Move(Motion::NextBeat) => self.move_device(times as isize, track),
            Cmd::Move(Motion::PreviousBeat) => self.move_device(-(times as isize), track),
            Cmd::Move(Motion::First) => {
                self.move_row_keeping_field(self.end_row(false, track), track);
            }
            Cmd::Move(Motion::Last) => {
                self.move_row_keeping_field(self.end_row(true, track), track);
            }
            Cmd::Move(Motion::Right) => (0..times).for_each(|_| self.step_right(track)),
            Cmd::Move(Motion::Left) => (0..times).for_each(|_| self.step_left(track)),
            Cmd::Move(Motion::RowStart) => self.row_end(false, track),
            Cmd::Move(Motion::RowEnd) => self.row_end(true, track),
            Cmd::PreviousPage | Cmd::NextPage => {
                self.page = self.page.beside(cmd == Cmd::NextPage);
                self.move_to(track, 0, field);
            }
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Put the cursor where it's seen, keeping the column `j` and `k` aim for. Left elsewhere,
    /// as after `}` onto an empty slot, an edit could bring back the setting it was kept for.
    pub fn settle(&mut self, track: Option<&Track>) {
        let wanted = self.wanted_field();
        let place = self.place(track);
        if place != (self.row, self.field) {
            (self.row, self.field) = place;
            self.wanted = Some((place.0, place.1, wanted));
        }
    }

    /// Whether the setting under the cursor has fine digits to stop on.
    fn has_fine_here(&self, track: Option<&Track>) -> bool {
        let control = self.control(track);
        match track {
            Some(track) => control.fine_on(track).is_some(),
            None => control.fine_on(&Track::default()).is_some(),
        }
    }

    /// The row in the device beside the cursor's, on its right or its left, that's level with the
    /// cursor's row, on a rack with a device there.
    fn row_beside(&self, track: Option<&Track>, row: usize, right: bool) -> Option<usize> {
        let devices = self.devices(track);
        let beside = device_beside(&devices, row / DEVICE_ROWS, right)?;
        Some(beside * DEVICE_ROWS + row % DEVICE_ROWS)
    }

    /// `l`: onto the fine digits of a setting that has them, and then on to the next, over any
    /// holes. Past the last, it goes into the device on the right, if there's one, on the same row.
    fn step_right(&mut self, track: Option<&Track>) {
        if !self.fine && self.has_fine_here(track) {
            self.fine = true;
            return;
        }
        let (row, field) = self.place(track);
        let next = self.rows(track)[row]
            .iter()
            .enumerate()
            .skip(field + 1)
            .find_map(|(at, place)| place.and(Some(at)));
        match (next, self.row_beside(track, row, true)) {
            (None, Some(beside)) => self.move_to(track, beside, 0),
            _ => self.move_to(track, row, next.unwrap_or(field)),
        }
    }

    /// `h`: back off the fine digits, and onto the previous setting's if it has them. Before the
    /// first, it goes into the device on the left, if there's one, to its last on the same row.
    fn step_left(&mut self, track: Option<&Track>) {
        if self.fine {
            self.fine = false;
            return;
        }
        let (row, field) = self.place(track);
        let before = self.rows(track)[row][..field]
            .iter()
            .rposition(Option::is_some);
        let beside = self.row_beside(track, row, false);
        match (before, beside) {
            (None, Some(beside)) => self.move_to(track, beside, usize::MAX),
            _ => self.move_to(track, row, before.unwrap_or(field)),
        }
        self.fine = (before.is_some() || beside.is_some()) && self.has_fine_here(track);
    }

    /// Move a row up or down, past any with nothing in them, as an empty slot's grid is, aiming
    /// for the field `j` and `k` aim for. On a rack it keeps to the cursor's column of devices,
    /// going on into the title of the one below, or the last row of the one above.
    fn step_row(&mut self, by: isize, track: Option<&Track>) {
        let wanted = self.wanted_field();
        let rows = self.rows(track);
        let here = self.place(track).0;
        let order: Vec<usize> = match self.column(track, here / DEVICE_ROWS) {
            Some(column) => column
                .iter()
                .flat_map(|device| device * DEVICE_ROWS..(device + 1) * DEVICE_ROWS)
                .collect(),
            None => (0..rows.len()).collect(),
        };
        let mut at = order.iter().position(|row| *row == here).unwrap_or(0);
        while let Some(next) = at.checked_add_signed(by).filter(|next| *next < order.len()) {
            at = next;
            if rows[order[at]].iter().any(Option::is_some) {
                self.move_to(track, order[at], wanted);
                break;
            }
        }
        self.wanted = Some((self.row, self.field, wanted));
    }

    /// The field `j` and `k` aim for: the one before a shorter row moved the cursor in, if it hasn't
    /// moved since.
    fn wanted_field(&self) -> usize {
        match self.wanted {
            Some((row, field, wanted)) if (row, field) == (self.row, self.field) => wanted,
            _ => self.field,
        }
    }

    /// Up or down to a row, aiming for the field `j` and `k` aim for.
    fn move_row_keeping_field(&mut self, row: usize, track: Option<&Track>) {
        let wanted = self.wanted_field();
        self.move_to(track, row, wanted);
        self.wanted = Some((self.row, self.field, wanted));
    }

    /// `0` and `$`: the first or the last setting on the cursor's row, over any holes.
    fn row_end(&mut self, last: bool, track: Option<&Track>) {
        let row = self.place(track).0;
        let field = if last { usize::MAX } else { 0 };
        self.move_to(track, row, field);
    }

    /// Go to a device some way up or down from the cursor's, in its column of the rack, on the
    /// same row and column, stopping at either end. The row and column are kept as they were, so
    /// they come back after a device that hasn't got them, such as an empty slot.
    fn move_device(&mut self, by: isize, track: Option<&Track>) {
        let count = self.devices(track).len();
        let here = (self.row / DEVICE_ROWS).min(count.saturating_sub(1));
        let Some(column) = self.column(track, here) else {
            return;
        };
        let at = column
            .iter()
            .position(|device| *device == here)
            .unwrap_or(0);
        let to = at
            .saturating_add_signed(by)
            .min(column.len().saturating_sub(1));
        self.row = column[to] * DEVICE_ROWS + self.row % DEVICE_ROWS;
    }
}

impl App {
    /// What the cursor is on, on the track page or the mixer.
    pub fn track_control(&self) -> Control {
        if self.view == View::Mixer {
            return MIXER[self.mixer_row_here()];
        }
        self.track_page.control(self.track_here())
    }

    /// Put the track page's cursor at the place nearest a row and field that holds a setting.
    pub(crate) fn move_control(&mut self, row: usize, field: usize) {
        let song = Arc::clone(self.song());
        self.track_page
            .move_to(song.tracks.get(self.cursor.track), row, field);
    }

    /// Run a command on the track page. Returns false if it means the same as anywhere.
    pub(crate) fn run_track_page(&mut self, cmd: Cmd) -> bool {
        let song = Arc::clone(self.song());
        let track = song.tracks.get(self.cursor.track);
        // Only `{` and `}` go on from where the cursor was kept, so they come back to it.
        let kept = self.track_page.clone();
        let braces = matches!(cmd, Cmd::Move(Motion::NextBeat | Motion::PreviousBeat));
        if !braces && self.view != View::Mixer {
            self.track_page.settle(track);
        }
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        match self.track_page.key(cmd, times, track) {
            Outcome::Done => return true,
            Outcome::Refused(why) => {
                self.status = why.to_owned();
                return true;
            }
            Outcome::Passed => {}
        }
        let fine = self.track_page.fine;
        match cmd {
            Cmd::FieldJump => self.open_field_jump(),
            Cmd::Audition => {
                self.mode = Mode::Keys;
                self.status = "a w s e d f play, < > change octave, ; stops".to_owned();
            }
            Cmd::RemoveRow | Cmd::Paste if self.track_page.page != Page::Fx => {
                self.status = "dd and p move effects, on the FX page".to_owned();
            }
            Cmd::Bypass => match self.track_page.page {
                Page::Fx => self.toggle_bypass(),
                Page::Mod => self.toggle_modulator(),
                Page::Sound => {
                    self.status =
                        "b switches an effect or a modulator, on the FX and Mod pages".to_owned();
                }
            },
            Cmd::RemoveRow => self.take_insert(),
            Cmd::Paste => self.put_insert(),
            Cmd::Increase if fine => self.adjust_fine(true, times, false),
            Cmd::Decrease if fine => self.adjust_fine(false, times, false),
            Cmd::IncreaseLarge if fine => self.adjust_fine(true, times, true),
            Cmd::DecreaseLarge if fine => self.adjust_fine(false, times, true),
            Cmd::ClearField if fine => self.set_fine_here(0),
            Cmd::Increase => self.adjust_control(true, times, false),
            Cmd::Decrease => self.adjust_control(false, times, false),
            Cmd::IncreaseLarge => self.adjust_control(true, times, true),
            Cmd::DecreaseLarge => self.adjust_control(false, times, true),
            Cmd::ClearField => self.reset_control(),
            Cmd::PickValue => self.open_control_picker(),
            Cmd::EnterInsert if self.track_control().is_choice(&self.page_track()) => {
                self.open_control_picker();
            }
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
            _ => {
                // What it means elsewhere, such as going to another track, keeps the cursor
                // where it was kept.
                self.track_page = kept;
                self.count = count;
                return false;
            }
        }
        true
    }
}
