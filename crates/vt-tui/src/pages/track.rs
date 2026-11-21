//! The track page, `Space 5` to `Space 7` and `L` on to Macros: a track's Sound, Mod, FX and Macros
//! pages; and the phrase view's Gen page, `Space 8`, a rack walked as they are, so it's a `TrackPage`
//! of its own, `App`'s `gen_page` (decision 466).
//!
//! `TrackPage` is the page as a component, which page shows, where its cursor is and the images it
//! shows, with the moves that need only the cursor's track, its edits, [`TrackPage::edit`] and the
//! Gen page's [`TrackPage::gen_edit`], given the [`Services`], and its drawing, in `draw`, given a
//! [`Ctx`]. What's on each page, its devices and their settings, is [`crate::controls`]'s; `device` has where
//! they sit in the page's rack and where the cursor is among them, `modulation`, `fx` and
//! `generators` what each page's keys do, and
//! `editing`, `typing` and `tuning` change the setting under the cursor, as functions over the
//! services that the mixer shares; `pictures` says which pictures the page wants the terminal to
//! have and where their dots go, as data for `Graphics`; `pickers` builds the lists `K` opens.
//! What a key the page passes back means across it and the mixer is [`TrackPage::across`], which
//! asks `App` for the picker, the shape panel, the mapping view or the field jump with an
//! [`Outcome`]. Presets and patches are `presets` and `patches`.
//! `TrackPage` is a [`crate::pages::Page`], with its rows of the binding table, `Page::KEYS`, here
//! beside it.

mod device;
pub mod draw;
pub(crate) mod editing;
pub(crate) mod following;
mod fx;
mod generators;
mod modulation;
pub(crate) mod patches;
mod pickers;
mod pictures;
pub(crate) mod presets;
mod tokens;
pub(crate) mod tuning;
pub(crate) mod typing;

pub use device::DeviceCursor;
pub(crate) use device::rack_places;
pub(crate) use fx::TakenInsert;
pub use tokens::GenTokens;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_model::insert::INSERTS;
use vt_model::modulation::ModField;
use vt_model::selection::Motion;
use vt_model::song::Track;
use vt_model::steps::STEPS;

use crate::app::Entry;
use crate::controls::{Control, DEVICE_ROWS, DeviceSpec, Page};
use crate::graphics::slots::SlotDrawing;
use crate::graphics::{PanelIds, PanelWant};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::tokens::Opening;
use crate::pages::track::draw::{CyclesAt, Drawn};
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::services::Services;
use crate::services::meter::{FLOOR_DB, Held, ReductionHistory};
use device::{device_beside, rack_column};
use editing::{adjust_control, adjust_fine, reset_control, set_fine_here, to_nothing};
use modulation::toggle_modulator;
use pickers::control_picker;

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

/// The track page as a component.
///
/// It holds which of its pages shows and where its cursor is, kept while other views are shown.
/// Each page keeps its own place, which the cursor goes back to when that page shows again, and
/// starts on its first setting. What's on a page depends on the cursor's track, so each move is
/// given that track, or none, and the page holds no song of its own. It also keeps the ids of the
/// terminal's images it shows and where it last drew those that carry a dot, the effect `dd` took,
/// and the cursor's track's meters it shows, held as they fall. It's drawn with its `Page::draw`,
/// given the rest of the program as a [`Ctx`].
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPage {
    /// The page showing: Sound, Mod, FX or Macros, or Gen for the phrase view's Gen page.
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
    kept: [(usize, usize); Page::ALL.len() + 1],
    /// The phrase the Gen page shows: the one under the app's cursor, which the app gives it
    /// as each key comes, so the page itself holds no song.
    pub phrase: Option<u32>,
    /// Whether the FX page is zoomed in on the cursor's slot, `z i`, rather than showing the rack.
    /// It's kept while other pages show, as the FX page's own.
    pub zoomed: bool,
    /// Which FX slots are turned to their second page, `t`, for an effect that has one (decision
    /// 452). It's the page's, not the song's, as a hardware page button is.
    pub slot_pages: [bool; INSERTS],
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
    /// The Gen page setting whose tokens the cursor is in, after `l` on it (decision 459).
    pub tokens: Option<GenTokens>,
    /// Gen's form, kept for the session: beside the grid, the side form it starts in, or the
    /// whole page (decision 467). Beside the grid it draws no previews, as the grid's column is
    /// beside it, and a device whose columns don't fit takes the four-column cells.
    pub beside: bool,
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
            slot_pages: [false; INSERTS],
            images: PanelIds::default(),
            zoom_caption: None,
            lfo_cycles_at: Default::default(),
            gen_panels_at: Vec::new(),
            taken_insert: None,
            slot_reduction: [Held::new(0.0); INSERTS],
            reduction_history: ReductionHistory::default(),
            after_trim: [Held::new(FLOOR_DB); 2],
            tokens: None,
            beside: false,
        }
    }
}

impl TrackPage {
    /// The phrase view's Gen page, a rack of its own on [`Page::Gen`], which it never leaves
    /// (decision 466), beside the grid to begin with (decision 467).
    pub fn on_gen_page() -> Self {
        Self {
            page: Page::Gen,
            beside: true,
            ..Self::default()
        }
    }

    /// The view the page is drawn as: Gen's, or the track page's.
    fn drawn_as(&self) -> View {
        match self.page {
            Page::Gen => View::Gen,
            _ => View::Track,
        }
    }

    /// Whether the keys are in the page as it's drawn: but for Gen beside the grid while they're
    /// in the grid (decision 467), and a panel or buffer that has them, where its cursor sits
    /// back (decision 475).
    pub(crate) fn has_keys(&self, ctx: &Ctx) -> bool {
        ctx.keys_in(self.drawn_as())
    }

    /// The cursor's style on the page: the mode's, or sat back while the keys are elsewhere.
    pub(crate) fn cursor_style(&self, ctx: &Ctx) -> ratatui::style::Style {
        ctx.cursor_on(self.drawn_as())
    }

    /// Whether the device with the cursor is framed in the focus colour: the page has the keys
    /// and another area that could take them is on screen (decision 475).
    pub(crate) fn framed(&self, ctx: &Ctx) -> bool {
        ctx.framed(self.drawn_as())
    }

    /// The page's devices, or none with no track. An FX slot turned to its second page has that
    /// page's grid.
    pub fn devices(&self, track: Option<&Track>) -> Vec<DeviceSpec> {
        let mut devices =
            track.map_or_else(Vec::new, |track| self.page.devices_on(track, self.phrase));
        if self.page == Page::Fx {
            for (device, turned) in devices.iter_mut().zip(self.slot_pages) {
                if turned {
                    *device = device.clone().turned();
                }
            }
        }
        devices
    }

    /// The page's rows as the cursor goes through them, each device's in turn, each a list of
    /// places that can be holes it steps over.
    pub fn rows(&self, track: Option<&Track>) -> Vec<Vec<Option<Control>>> {
        self.devices(track)
            .iter()
            .flat_map(DeviceSpec::rows)
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
            // Gen is the phrase view's page after the grid: `H` goes back to the grid, as it goes
            // back a page on the track page, and there's none after it.
            Cmd::PreviousPage if self.page == Page::Gen => return Outcome::Show(View::Phrase),
            Cmd::NextPage if self.page == Page::Gen => {
                return Outcome::Refused("Gen is the phrase view's last page: H goes to the grid");
            }
            Cmd::PreviousPage | Cmd::NextPage => {
                self.show(self.page.beside(cmd == Cmd::NextPage));
                self.settle(track);
            }
            Cmd::ZoomSlot | Cmd::ZoomRack if self.page != Page::Fx => {
                return Outcome::Refused("z i zooms in on an effect, on the FX page");
            }
            Cmd::SlotPage(_) if self.page != Page::Fx => {
                return Outcome::Refused("]p and [p turn an effect's pages, on the FX page");
            }
            // With two pages at most, the next and the previous are the other, going round.
            Cmd::SlotPage(_) => {
                let Some(cursor) = self.device_cursor(track) else {
                    return Outcome::Refused("]p and [p turn an effect's pages, on the FX page");
                };
                let devices = self.devices(track);
                if devices
                    .get(cursor.device)
                    .is_none_or(|device| device.second.is_none())
                {
                    return Outcome::Refused("This effect has one page");
                }
                if let Some(turned) = self.slot_pages.get_mut(cursor.device) {
                    *turned = !*turned;
                }
                self.move_to(track, self.row, self.field);
            }
            Cmd::ZoomSlot => self.zoomed = !self.zoomed,
            Cmd::ZoomRack => self.zoomed = false,
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Run one of the page's edits on the setting under the cursor, `control`, which on the mixer
    /// is the mixer's: `b`, `p`, `=` and `-` and their large steps, on the fine digits if the
    /// cursor is on them, `x`, and `dd`, which sets it to nothing, or on an effect's name takes the
    /// effect out. `times` is its count, at least 1.
    pub fn edit(
        &mut self,
        cmd: Cmd,
        times: usize,
        control: Control,
        services: &mut Services,
    ) -> Outcome {
        let fine = self.fine;
        match cmd {
            Cmd::Paste if self.page != Page::Fx => {
                return Outcome::Refused("p puts an effect taken with dd, on the FX page");
            }
            Cmd::Bypass => match self.page {
                Page::Fx => self.toggle_bypass(services),
                Page::Mod => toggle_modulator(services, control),
                Page::Sound | Page::Macros | Page::Gen => {
                    return Outcome::Refused(
                        "b switches an effect or a modulator, on the FX and Mod pages",
                    );
                }
            },
            // On an effect's name, `dd` takes the effect out, as `p` puts it back.
            Cmd::ToNothing if matches!(control, Control::Insert(_)) => {
                self.take_insert(services);
            }
            Cmd::ToNothing => to_nothing(services, control),
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
        (
            "H",
            Cmd::PreviousPage,
            "Previous page: Sound, Mod, FX, Macros",
        ),
        ("L", Cmd::NextPage, "Next page: Sound, Mod, FX, Macros"),
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
        (
            "] p",
            Cmd::SlotPage(true),
            "On the FX page, this slot's next page",
        ),
        (
            "[ p",
            Cmd::SlotPage(false),
            "On the FX page, this slot's previous page",
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
            "On a Steps slot, keep what Mutate drew",
        ),
        (
            "Enter",
            Cmd::ReopenGenerator,
            "On a Steps slot's step, open its buffer",
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
        (
            "d d",
            Cmd::ToNothing,
            "To nothing; on an effect's name or a generator's Fill, take it out",
        ),
        ("K", Cmd::PickValue, "Choose this setting from a list"),
        (
            "a",
            Cmd::Append,
            "Type; on a text, its buffer typing at the end, keeping it",
        ),
        (
            "S",
            Cmd::Retype,
            "Type; on a text, its buffer typing it all again",
        ),
        (
            "c c",
            Cmd::Retype,
            "Type; on a text, its buffer typing it all again",
        ),
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
        // On a Gen page setting in a grammar, or a Steps slot's step on the Mod page, `Enter`
        // opens its buffer on the first token, or the step, `i` and `a` typing at the end and
        // `cc` and `S` typing it all again (decision 463).
        let control = self.control(track);
        let opening = match cmd {
            Cmd::ReopenGenerator => Some(Opening::Tokens),
            Cmd::EnterInsert | Cmd::Append => Some(Opening::Append),
            Cmd::Retype => Some(Opening::Replace),
            _ => None,
        };
        // A step's own value is still typed with `i`, as any value is.
        let steps = self.page == Page::Mod
            && matches!(control, Control::Mod(_, ModField::Step(_)))
            && cmd != Cmd::EnterInsert;
        if let Some(how) = opening
            && (self.on_gen(ctx.modes.view, control) || steps)
            && GenTokens::opens(ctx.services, ctx.modes.view, control)
        {
            self.tokens = GenTokens::open(ctx.services, ctx.modes.view, control, how);
            return Outcome::Enter(Mode::Tokens);
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

/// Whether a key works on the setting under the cursor, which Gen with no phrase hasn't: a turn,
/// a list, typing, a macro's keys, the generators' own and the search.
fn takes_a_setting(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Increase
            | Cmd::Decrease
            | Cmd::IncreaseLarge
            | Cmd::DecreaseLarge
            | Cmd::ClearField
            | Cmd::ToNothing
            | Cmd::PickValue
            | Cmd::EnterInsert
            | Cmd::MapMacro
            | Cmd::MacroMapping
            | Cmd::OpenItem
            | Cmd::FieldJump
            | Cmd::ApplyGenerator
            | Cmd::RollGenerator
            | Cmd::ReopenGenerator
            | Cmd::Paste
    )
}

/// Why Gen has no devices, on the cursor's track: it has no phrase here, or is a fixed track.
fn no_phrase_said(services: &Services) -> String {
    draw::no_generators(&services.page_track()).to_owned()
}

/// How many values a track's Steps slot plays, 1 to 16, or all 16 with no track.
fn steps_played(track: Option<&Track>, slot: usize) -> u8 {
    track.map_or(STEPS as u8, |track| {
        track.mods[slot].steps.played_count() as u8
    })
}

impl TrackPage {
    /// What a key the track page or the mixer passed back means, `times` being its count, on the
    /// setting under the cursor, `control`, in this order:
    /// 0. On Gen with no phrase here, a key on a setting is refused, saying why.
    /// 1. A turn of a setting a macro holds is refused, saying where to turn it
    ///    ([`Services::refuses_turn`]), before anything can turn it.
    /// 2. The Gen page's edits, on the generator under the cursor ([`TrackPage::gen_edit`]), only
    ///    while Gen is on screen.
    /// 3. What only `App` opens for the page, `/`, `M` and `gm`, and the Keys mode, none of which
    ///    the Gen page takes, and `Enter` on the Gen page, which opens the shape panel.
    /// 4. `r`, `Enter` and `Space g a` off the Gen page, which say where they work, but for
    ///    `Space g a` on the Mod page, which keeps a Steps slot's drift; then the pickers and
    ///    Insert.
    /// 5. The track page's edits on the setting under the cursor, which the mixer shares.
    ///
    /// Passes it back if it means what it does elsewhere, such as going to another track.
    pub(crate) fn across(
        &mut self,
        cmd: Cmd,
        times: usize,
        control: Control,
        ctx: &mut CtxMut,
    ) -> Outcome {
        // Off a setting in a grammar, `a`, `cc` and `S` type as `i` does.
        let cmd = match cmd {
            Cmd::Append | Cmd::Retype => Cmd::EnterInsert,
            cmd => cmd,
        };
        // Gen with no phrase here has nothing to turn, and the cursor stands on no setting.
        if self.page == Page::Gen
            && self.device_cursor(ctx.services.track_here()).is_none()
            && takes_a_setting(cmd)
        {
            ctx.services.status = no_phrase_said(ctx.services);
            return Outcome::Done;
        }
        if ctx.services.refuses_turn(cmd, control) {
            return Outcome::Done;
        }
        match self.gen_edit(cmd, times, control, ctx) {
            Outcome::Passed => {}
            outcome => return outcome,
        }
        let view = ctx.modes.view;
        let services = &mut *ctx.services;
        match cmd {
            Cmd::FieldJump => Outcome::FieldJump,
            Cmd::MapMacro => services.macro_picker(control),
            Cmd::MacroMapping => services.mapping_of(control),
            Cmd::Audition => {
                services.status = "a w s e d f play, < > change octave, ; stops".to_owned();
                Outcome::Enter(Mode::Keys)
            }
            Cmd::ReopenGenerator if self.on_gen(view, control) => {
                Self::gen_panel(control, services)
            }
            Cmd::RollGenerator | Cmd::ReopenGenerator => {
                services.status =
                    "Enter opens a generator's shape panel, on Gen: Space 8".to_owned();
                Outcome::Done
            }
            Cmd::ApplyGenerator if self.page == Page::Mod => {
                services.keep_drift(control);
                Outcome::Done
            }
            Cmd::ApplyGenerator => {
                services.status =
                    "Space g a keeps a Steps slot's drift on the Mod page, or applies a generator \
                     on Gen"
                        .to_owned();
                Outcome::Done
            }
            Cmd::PickValue => control_picker(control, services),
            Cmd::EnterInsert if control.is_choice(&services.page_track()) => {
                control_picker(control, services)
            }
            Cmd::EnterInsert => Outcome::Insert(Entry::default()),
            _ => self.edit(cmd, times, control, services),
        }
    }
}
