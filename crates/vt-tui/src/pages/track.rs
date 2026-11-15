//! The track page, `Space 5` to `Space 8`: a track's Sound, Mod, FX and Gen pages. `TrackPage` is
//! the page as a component, which page shows, where its cursor is and the images it shows, with
//! the moves that need only the cursor's track, its edits, [`TrackPage::edit`], given the
//! [`Services`], and its drawing, in `draw`, given a [`Ctx`]. Each page's settings are in a
//! file of its own (`sound`, `modulation`, `fx`), each gives its devices to `Page::devices` if it's
//! a rack (`device`), and `editing`, `typing` and `tuning` change the setting under the cursor, as
//! functions over the services that the mixer shares; `pictures` says which pictures the page
//! wants the terminal to have and where their dots go, as data for `Graphics`; `pickers`, which
//! open the program's picker, and the Gen page's `generators`, which reshape through the program's
//! shape panel, are still `App`'s. `TrackPage` is a [`crate::pages::Page`], with its rows of the
//! binding table, `Page::KEYS`, here beside it.

mod controls;
mod device;
pub mod draw;
pub(crate) mod editing;
mod fx;
mod generators;
mod modulation;
pub(crate) mod patches;
mod pickers;
mod pictures;
pub(crate) mod presets;
mod sound;
pub(crate) mod tuning;
mod typing;
pub(crate) mod view;

pub use controls::{AccumulatorField, ArpField, Control};
pub(crate) use device::{DEVICE_COLUMNS, DEVICE_ROWS, DeviceWidth, rack_places};
pub use device::{DeviceCursor, DeviceSpec};
pub(crate) use fx::TakenInsert;
pub(crate) use generators::macros_device;
pub(crate) use modulation::modulator_bypassed;
pub use view::{set, view};

use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_model::insert::INSERTS;
use vt_model::modulation::ModField;
use vt_model::selection::Motion;
use vt_model::song::Track;
use vt_model::steps::STEPS;

use crate::app::{App, Entry};
use crate::graphics::slots::SlotDrawing;
use crate::graphics::{PanelIds, PanelWant};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::pages::track::draw::{CyclesAt, Drawn};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;
use crate::services::meter::{FLOOR_DB, Held, ReductionHistory};
use device::{device_beside, rack_column};
use editing::{adjust_control, adjust_fine, reset_control, set_fine_here};
use modulation::toggle_modulator;

/// A part of the track page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Sound,
    Mod,
    Fx,
    /// The live generators of the phrase under the cursor.
    Gen,
}

impl Page {
    /// Every page, in order.
    pub const ALL: [Self; 4] = [Self::Sound, Self::Mod, Self::Fx, Self::Gen];

    /// The page's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Mod => "Mod",
            Self::Fx => "FX",
            Self::Gen => "Gen",
        }
    }

    /// Where the page is in [`Page::ALL`].
    fn index(self) -> usize {
        self as usize
    }

    /// The page next to this one, or this one at either end.
    fn beside(self, next: bool) -> Self {
        let at = Self::ALL.iter().position(|page| *page == self).unwrap_or(0);
        let to = if next { at + 1 } else { at.saturating_sub(1) };
        Self::ALL[to.min(Self::ALL.len() - 1)]
    }

    /// The page's devices, in the order they're read, each page's from its own file: the Sound
    /// page's machine, filter, output and play, or a fixed track's output alone, the Mod page's
    /// mod slots, accumulator and arpeggio, and the FX page's slots. The cursor, `j`, `k`,
    /// `h`, `l`, `{` and `}` go through whatever devices a page gives, laid out as its rack
    /// ([`rack_places`]). The Gen page's hang on a phrase as well, so they're
    /// [`Page::devices_on`]'s, and here there are none.
    pub fn devices(self, track: &Track) -> Vec<DeviceSpec> {
        self.devices_on(track, None)
    }

    /// The page's devices, the Gen page's being a device for each live generator of this phrase
    /// of the track, then `Empty`.
    pub(crate) fn devices_on(self, track: &Track, phrase: Option<u32>) -> Vec<DeviceSpec> {
        match self {
            Self::Sound => sound::devices(track),
            Self::Mod => modulation::devices(track),
            Self::Fx => fx::devices(track),
            Self::Gen => generators::devices(track, phrase),
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

/// The track page as a component: which of its pages shows, and where its cursor is, kept while
/// other views are shown. Each page keeps its own place, which the cursor goes back to when that
/// page shows again, and starts on its first setting. What's on a page depends on the cursor's track, so each move is given
/// that track, or none, and the page holds no song of its own. It also keeps the ids of the
/// terminal's images it shows and where it last drew those that carry a dot, the effect `dd`
/// took, and the cursor's track's meters it shows, held as they fall. It's drawn with
/// its `Page::draw`, given the rest of the program as a [`Ctx`].
#[derive(Debug, Clone, PartialEq)]
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
    /// Which of a Steps slot's values the cursor is on, counted from 0, while it's in the slot's
    /// panel, which is one stop on the page that `h` and `l` go across.
    pub step: u8,
    /// The row and field the cursor was kept at on each page, by [`Page::index`], the showing
    /// page's being `row` and `field`.
    kept: [(usize, usize); Page::ALL.len()],
    /// The phrase the Gen page shows: the one under the app's cursor, which the app gives it
    /// as each key comes, so the page itself holds no song.
    pub phrase: Option<u32>,
    /// Whether the FX page is zoomed in on the cursor's slot, `z i`, rather than showing the rack
    /// (decision 329). It's kept while other pages show, as the FX page's own.
    pub zoomed: bool,
    /// The ids of the terminal's images of the pictures the page last wanted, by panel: the
    /// filter's curve and a sampler's thumbnail, the Mod page's slots, the FX page's slots or its
    /// zoomed one, the Gen page's previews by each generator's place in its phrase's list.
    pub images: PanelIds,
    /// What's said under the zoomed slot's box, a delay's or a reverb's time scale, and the
    /// drawing it was worked out for.
    pub(crate) zoom_caption: Option<(SlotDrawing, Option<String>)>,
    /// Where each LFO's cycle was last drawn, for the terminal to place its dot on.
    pub lfo_cycles_at: CyclesAt,
    /// Where each preview the Gen page drew is, its generator's place and its picture's top left,
    /// for the terminal to place the row playing on.
    pub gen_panels_at: Vec<(usize, (u16, u16))>,
    /// An effect taken out of a slot with `dd`, for `p` to put somewhere.
    pub(crate) taken_insert: Option<TakenInsert>,
    /// How far each FX slot of the cursor's track has turned down lately, in dB, for the mark its
    /// gain reduction meter holds at the deepest.
    pub slot_reduction: [Held; INSERTS],
    /// What each FX slot of the cursor's track turned down over the last two bars, for the FX
    /// page's zoom.
    pub reduction_history: ReductionHistory,
    /// The cursor's track's level right after its trim, each side's in dB, for the meter in its
    /// Output device: where it is now and the highest it was lately.
    pub after_trim: [Held; 2],
}

impl Default for TrackPage {
    fn default() -> Self {
        Self {
            page: Page::default(),
            row: 0,
            field: 0,
            wanted: None,
            fine: false,
            step: 0,
            kept: Default::default(),
            phrase: None,
            zoomed: false,
            images: PanelIds::default(),
            zoom_caption: None,
            lfo_cycles_at: Default::default(),
            gen_panels_at: Vec::new(),
            taken_insert: None,
            slot_reduction: [Held::new(0.0); INSERTS],
            reduction_history: ReductionHistory::default(),
            after_trim: [Held::new(FLOOR_DB); 2],
        }
    }
}

impl TrackPage {
    /// The page's devices, or none with no track.
    pub fn devices(&self, track: Option<&Track>) -> Vec<DeviceSpec> {
        track.map_or_else(Vec::new, |track| self.page.devices_on(track, self.phrase))
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

    /// What the cursor is on. In a Steps slot's panel, it's the value it's on there.
    pub fn control(&self, track: Option<&Track>) -> Control {
        let (row, field) = self.place(track);
        let rows = self.rows(track);
        let place = rows
            .get(row)
            .and_then(|row| row.get(field).copied().flatten());
        match place {
            Some(Control::Mod(slot, ModField::Step(_))) => {
                let last = steps_played(track, slot) - 1;
                Control::Mod(slot, ModField::Step(self.step.min(last)))
            }
            place => place.unwrap_or(Control::Machine),
        }
    }

    /// What the cursor is on, on the cursor's track.
    pub(crate) fn control_here(&self, ctx: &Ctx) -> Control {
        self.control(ctx.track_here())
    }

    /// The Steps slot whose panel the cursor is in, and how many values it plays, if it's in one.
    fn in_steps(&self, track: Option<&Track>) -> Option<(usize, u8)> {
        match self.control(track) {
            Control::Mod(slot, ModField::Step(_)) => Some((slot, steps_played(track, slot))),
            _ => None,
        }
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

    /// Show a page, with the cursor where it was kept on that page, and this page's place kept for
    /// coming back to it. A place is kept as it was, which can be on a hole, and is seen where
    /// [`TrackPage::place`] puts it on the page as it is now.
    pub fn show(&mut self, page: Page) {
        if page == self.page {
            return;
        }
        self.kept[self.page.index()] = (self.row, self.field);
        self.page = page;
        (self.row, self.field) = self.kept[page.index()];
        self.wanted = None;
        self.fine = false;
    }

    /// Put the cursor at the place nearest a row and field that holds a setting, off any fine
    /// digits.
    pub fn move_to(&mut self, track: Option<&Track>, row: usize, field: usize) {
        (self.row, self.field) = self.nearest(track, row, field);
        self.fine = false;
    }

    /// Run one of the page's own keys on a track: the moves, `{` and `}` across a rack's devices,
    /// and the keys to the pages either side. `times` is its count, at least 1.
    pub fn key_on(&mut self, cmd: Cmd, times: usize, track: Option<&Track>) -> Outcome {
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
                self.show(self.page.beside(cmd == Cmd::NextPage));
                self.settle(track);
            }
            Cmd::ZoomSlot | Cmd::ZoomRack if self.page != Page::Fx => {
                return Outcome::Refused("z i zooms in on an effect, on the FX page");
            }
            Cmd::ZoomSlot => self.zoomed = !self.zoomed,
            Cmd::ZoomRack => self.zoomed = false,
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Run one of the page's edits on the setting under the cursor, `control`, which on the mixer
    /// is the mixer's: `b`, `dd` and `p`, `=` and `-` and their large steps, on the fine digits if
    /// the cursor is on them, and `x`. `times` is its count, at least 1.
    pub fn edit(
        &mut self,
        cmd: Cmd,
        times: usize,
        control: Control,
        services: &mut Services,
    ) -> Outcome {
        let fine = self.fine;
        match cmd {
            Cmd::RemoveRow | Cmd::Paste if self.page != Page::Fx => {
                return Outcome::Refused("dd and p move effects, on the FX page");
            }
            Cmd::Bypass => match self.page {
                Page::Fx => self.toggle_bypass(services),
                Page::Mod => toggle_modulator(services, control),
                Page::Sound | Page::Gen => {
                    return Outcome::Refused(
                        "b switches an effect or a modulator, on the FX and Mod pages",
                    );
                }
            },
            Cmd::RemoveRow => self.take_insert(services),
            Cmd::Paste => self.put_insert(services),
            Cmd::Increase if fine => adjust_fine(services, control, true, times, false),
            Cmd::Decrease if fine => adjust_fine(services, control, false, times, false),
            Cmd::IncreaseLarge if fine => adjust_fine(services, control, true, times, true),
            Cmd::DecreaseLarge if fine => adjust_fine(services, control, false, times, true),
            Cmd::ClearField if fine => set_fine_here(services, control, 0),
            Cmd::Increase => adjust_control(services, control, true, times, false),
            Cmd::Decrease => adjust_control(services, control, false, times, false),
            Cmd::IncreaseLarge => adjust_control(services, control, true, times, true),
            Cmd::DecreaseLarge => adjust_control(services, control, false, times, true),
            Cmd::ClearField => reset_control(services, control),
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
    pub(crate) fn has_fine_here(&self, track: Option<&Track>) -> bool {
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
        // Across a Steps slot's values, then on out of its panel past the last.
        let inside = self.in_steps(track);
        if let Some((_, played)) = inside
            && self.step + 1 < played
        {
            self.step += 1;
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
        // Into a panel from its left, on its first value.
        if inside.is_none() && self.in_steps(track).is_some() {
            self.step = 0;
        }
    }

    /// `h`: back off the fine digits, and onto the previous setting's if it has them. Before the
    /// first, it goes into the device on the left, if there's one, to its last on the same row.
    fn step_left(&mut self, track: Option<&Track>) {
        if self.fine {
            self.fine = false;
            return;
        }
        // Back across a Steps slot's values, then out of its panel to the grid before the first.
        let inside = self.in_steps(track);
        if let Some((_, played)) = inside
            && self.step > 0
        {
            self.step = self.step.min(played - 1) - 1;
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
        // Into a panel from its right, on its last value.
        if inside.is_none()
            && let Some((_, played)) = self.in_steps(track)
        {
            self.step = played - 1;
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
        // In a Steps slot's panel, its first or last value, and from the first, the grid's start.
        if let Some((_, played)) = self.in_steps(track)
            && (last || self.step > 0)
        {
            self.step = if last { played - 1 } else { 0 };
            return;
        }
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

impl crate::pages::Page for TrackPage {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::Move(Motion::Down), "Next row"),
        ("k", Cmd::Move(Motion::Up), "Previous row"),
        (
            "g g",
            Cmd::Move(Motion::First),
            "First row, of this column of devices",
        ),
        (
            "G",
            Cmd::Move(Motion::Last),
            "Last row, of this column of devices",
        ),
        ("H", Cmd::PreviousPage, "Previous page: Sound, Mod, FX, Gen"),
        ("L", Cmd::NextPage, "Next page: Sound, Mod, FX, Gen"),
        (
            "d d",
            Cmd::RemoveRow,
            "Take an effect out of its slot, or a generator away",
        ),
        ("p", Cmd::Paste, "Put the effect taken in this slot"),
        (
            "b",
            Cmd::Bypass,
            "Switch this effect or modulator out, or back in",
        ),
        (
            "z i",
            Cmd::ZoomSlot,
            "On the FX page, zoom in on this slot, or back out",
        ),
        (
            "z o",
            Cmd::ZoomRack,
            "On the FX page, back out to every slot",
        ),
        ("/", Cmd::FieldJump, "Jump to a setting by its name"),
        (
            "M",
            Cmd::MapMacro,
            "Map this setting to a macro, or move it, or take it off",
        ),
        (
            "g m",
            Cmd::MacroMapping,
            "This macro's mapping, or the one this setting is on",
        ),
        (
            "g d",
            Cmd::OpenItem,
            "Open Sample's sample page, or a macro-held setting's macro",
        ),
        (
            "Space g a",
            Cmd::ApplyGenerator,
            "On a Steps slot, keep what Mutate drew; on Gen, apply it",
        ),
        (
            "r",
            Cmd::RollGenerator,
            "On the Gen page, roll the generator again",
        ),
        (
            "Enter",
            Cmd::ReopenGenerator,
            "On the Gen page, open the shape panel on it",
        ),
        (
            "}",
            Cmd::Move(Motion::NextBeat),
            "Next device down, in the same column",
        ),
        (
            "{",
            Cmd::Move(Motion::PreviousBeat),
            "Previous device up, in the same column",
        ),
        (
            "h",
            Cmd::Move(Motion::Left),
            "Previous setting, or the device on the left",
        ),
        (
            "l",
            Cmd::Move(Motion::Right),
            "Next setting, or the device on the right",
        ),
        ("0", Cmd::Move(Motion::RowStart), "First setting"),
        ("$", Cmd::Move(Motion::RowEnd), "Last setting"),
        ("=", Cmd::Increase, "Increase the value"),
        ("-", Cmd::Decrease, "Decrease the value"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        ("C-a", Cmd::Increase, "Increase the value"),
        ("C-x", Cmd::Decrease, "Decrease the value"),
        ("x", Cmd::ClearField, "Back to its default"),
        ("K", Cmd::PickValue, "Choose this setting from a list"),
        (
            ";",
            Cmd::Audition,
            "Play the sound on a w s e d f, until ; again",
        ),
    ];

    type Drawn = Drawn;

    /// Run a key as the program gives it to the page, on the cursor's track: the cursor first
    /// goes where it's seen, but for `{` and `}`, which go on from where it was kept so they come
    /// back to it, and on the mixer, whose cursor this isn't. Then it's one of the page's own
    /// keys, [`TrackPage::key_on`], or passed back, for an edit or what it means across pages.
    fn key(&mut self, cmd: Cmd, count: usize, ctx: &mut CtxMut) -> Outcome {
        let track = ctx.track_here();
        let braces = matches!(cmd, Cmd::Move(Motion::NextBeat | Motion::PreviousBeat));
        if !braces && ctx.modes.view != View::Mixer {
            self.settle(track);
        }
        self.key_on(cmd, count, track)
    }

    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Drawn {
        crate::pages::track::draw::draw(frame, area, self, ctx)
    }

    fn column(&self, ctx: &Ctx) -> String {
        let track = ctx.track_here();
        let control = self.control(track);
        track.map_or_else(
            || control.label().to_owned(),
            |track| control.name_on(track).to_owned(),
        )
    }

    fn hint(&self, ctx: &Ctx) -> Option<String> {
        self.zoom_hint(ctx)
    }

    fn panels(&self, ctx: &Ctx) -> Vec<PanelWant> {
        self.pictures(ctx)
    }

    fn take_panels(&mut self, ids: PanelIds, wants: &[PanelWant], rate: u32) {
        self.take_images(ids, wants, rate);
    }
}

/// How many values a track's Steps slot plays, 1 to 16, or all 16 with no track.
fn steps_played(track: Option<&Track>, slot: usize) -> u8 {
    track.map_or(STEPS as u8, |track| {
        track.mods[slot].steps.played_count() as u8
    })
}

impl App {
    /// What the cursor is on, on the track page or the mixer.
    pub fn track_control(&self) -> Control {
        if self.modes.view == View::Mixer {
            return self.mixer_page.control_here(&self.ctx());
        }
        self.track_page.control(self.services.track_here())
    }

    /// Put the track page's cursor at the place nearest a row and field that holds a setting.
    pub(crate) fn move_control(&mut self, row: usize, field: usize) {
        let song = Arc::clone(self.song());
        self.track_page
            .move_to(song.tracks.get(self.services.cursor.track), row, field);
    }

    /// `Space 5` to `Space 8`: the track page on one of its pages, where its cursor was left, as a
    /// jump `C-o` comes back from.
    pub(crate) fn show_track_page(&mut self, page: Page) {
        if self.modes.view != View::Track || self.track_page.page != page {
            self.record_jump();
        }
        self.track_page.show(page);
        self.track_page.phrase = self.services.phrase_id();
        let song = Arc::clone(self.song());
        self.track_page
            .settle(song.tracks.get(self.services.cursor.track));
        self.show_view(View::Track);
    }

    /// What a key the track page or the mixer passed back means, `times` being its count: `App`'s
    /// commands across pages, `/`, `M`, `gm`, the Keys mode, the Gen page's generators, the
    /// pickers and Insert, and then the track page's edits on the setting under the cursor, which
    /// the mixer shares. Returns false if it means what it does elsewhere, such as going to
    /// another track.
    pub(crate) fn run_across_pages(&mut self, cmd: Cmd, times: usize) -> bool {
        if self.refuses_turn(cmd) {
            return true;
        }
        match cmd {
            Cmd::FieldJump => self.open_field_jump(),
            Cmd::MapMacro => self.open_macro_picker(),
            Cmd::MacroMapping => self.open_mapping(),
            Cmd::Audition => {
                self.modes.mode = Mode::Keys;
                self.services.status = "a w s e d f play, < > change octave, ; stops".to_owned();
            }
            // The macros' band, first on the Gen page, turns as the other pages' settings do.
            _ if self.track_page.page == Page::Gen
                && !matches!(self.track_control(), Control::Macro(_))
                && self.run_gen_page(cmd, times) => {}
            Cmd::RollGenerator | Cmd::ReopenGenerator => {
                self.services.status =
                    "r and Enter work on a generator, on the Gen page".to_owned();
            }
            Cmd::ApplyGenerator if self.track_page.page == Page::Mod => self.keep_drift(),
            Cmd::ApplyGenerator => {
                self.services.status =
                    "Space g a keeps a Steps slot's drift on the Mod page, or applies a generator \
                     on the Gen page"
                        .to_owned();
            }
            Cmd::PickValue => self.open_control_picker(),
            Cmd::EnterInsert if self.track_control().is_choice(&self.services.page_track()) => {
                self.open_control_picker();
            }
            Cmd::EnterInsert => {
                self.modes.entry = Entry::default();
                self.modes.mode = Mode::Insert;
            }
            _ => {
                let control = self.track_control();
                let outcome = self
                    .track_page
                    .edit(cmd, times, control, &mut self.services);
                // The count was taken before the page had the key.
                if !self.settled(outcome, false) {
                    return false;
                }
            }
        }
        true
    }
}
