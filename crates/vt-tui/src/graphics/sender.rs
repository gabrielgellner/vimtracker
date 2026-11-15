//! `Graphics`, which sends the pictures the screen shows and keeps track of which images the
//! terminal has. It reads no `App`: each page says which of its panels' pictures it wants as
//! [`PanelWant`]s and is given back their ids, the scope and the picker's preview are handed what
//! they're drawn from, and what's placed directly comes as [`Placements`].

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::crossterm::terminal;
use ratatui::style::Color;
use vt_engine::engine::PLAYHEADS;
use vt_engine::track_state::LfoNow;
use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot};

use super::encoding::{Picture, scope_png, spans_png};
use super::panels::{Found, PanelIds, PanelImages, PanelWant};
use super::plot::{self, LfoDrawing, Shade, TrackDrawing};
use super::protocol::{
    DEFAULT_CELL, Image, Placement, forget, place_at, store, transmit, unplace, unplace_all,
};
use super::switches::{self, Look};
use super::{CELL_DOTS_ACROSS, CELL_DOTS_DOWN, rgb};
use crate::overlays::picker::Shown;
use crate::overlays::picker::draw::{
    PREVIEW_ROWS, PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH, PREVIEW_WAVE_HEIGHT,
    PREVIEW_WAVE_WIDTH,
};
use crate::pages::sample::SamplePage;
use crate::pages::sample::draw::{MarkKind, WAVE_ROWS, WaveOnScreen, mark_reach};
use crate::pages::track::draw::{MOD_PANEL_HEIGHT, MOD_PANEL_WIDTH};
use crate::screen::meter::{SCOPE_HEIGHT, SCOPE_WIDTH};
use crate::services::scope::Scope;
use crate::widgets::theme;

/// The scope is brought up to date once in this many frames, about 15 times a second. Each new
/// picture is a new image, which costs the terminal more than the program: at every frame, kitty
/// took 4.5% of a core to show it, and at every other frame, under 3%.
const SCOPE_FRAMES: usize = 2;
/// A panel's picture that changes is sent again at most once in this many frames, as the scope is,
/// so one that follows an LFO costs the terminal no more than the scope does.
const PANEL_FRAMES: usize = SCOPE_FRAMES;

/// A mark in the sample page's ruler as a picture, for characters of this size: a triangle
/// pointing down at the bottom of a character, its tip as wide as its line in the waveform, so
/// placed over the line it sits on it. It's [`mark_reach`] pixels either side of the line across,
/// and a pixel more than that down, so its sides slope at 45 degrees, as `▾`'s do.
pub fn mark_png(kind: MarkKind, cell: (usize, usize)) -> Vec<u8> {
    mark_picture(kind, cell).png()
}

/// A mark as [`mark_png`] draws it, before it's a PNG.
fn mark_picture(kind: MarkKind, cell: (usize, usize)) -> Picture {
    let reach = mark_reach(cell.0);
    let width = kind.line_pixels() + 2 * reach;
    let height = cell.1.max(1);
    let mut picture = Picture::new(width, height, vec![rgb(kind.color())]);
    let rows = (reach + 1).min(height);
    for (row, y) in (height - rows..height).enumerate() {
        let inset = row + reach + 1 - rows;
        picture.pixels[y * width + inset..(y + 1) * width - inset].fill(1);
    }
    picture
}

/// How many pixels across an LFO's dot is for characters of this size: a little over half a
/// character, an odd number so it has a middle pixel, and never under 5. At the author's 15 by 39
/// it's 9, four and a half times the line it rides, so it's found at a glance while it moves.
pub fn dot_pixels(cell: (usize, usize)) -> usize {
    ((cell.0 * 3 / 5) | 1).max(5)
}

/// Where an LFO's dot goes, for characters of this size, its cycle's picture having its top left
/// at `corner`: centred where the LFO is now, across at its place in the cycle drawn and up at its
/// wave's value, upside down for a negative depth as the cycle is, and kept inside the picture. A
/// Track slot's goes on its map where its last note is.
pub fn dot_placement(
    cell: (usize, usize),
    corner: (u16, u16),
    modulator: &ModSlot,
    now: LfoNow,
    diameter: usize,
) -> Placement {
    let (x, level) = if modulator.kind == ModKind::Track {
        let drawing = TrackDrawing::of(modulator);
        let x = f32::from(now.phase) / f32::from(u16::MAX);
        (
            x,
            drawing.level_of(f32::from(now.value) / f32::from(i16::MAX)),
        )
    } else {
        let drawing =
            LfoDrawing::of(&modulator.lfo, modulator.depth).in_polarity(modulator.polarity());
        let x = drawing.x_of(f64::from(now.phase) / (f64::from(u16::MAX) + 1.0));
        // A random wave's cycle isn't flipped, as it's only its centre line, but its dot is.
        let signed = LfoDrawing {
            flipped: modulator.depth < 0,
            ..drawing
        };
        (
            x,
            signed.level_of(f32::from(now.value) / f32::from(i16::MAX)),
        )
    };
    placed_dot(
        cell,
        corner,
        (MOD_PANEL_WIDTH, MOD_PANEL_HEIGHT),
        (x, level),
        diameter,
    )
}

/// Where a dot this many pixels across goes, centred on a point `(x, level)` from 0 to 1 of a
/// picture this many characters across and down whose top left is at `corner`, and kept inside
/// it.
pub fn placed_dot(
    cell: (usize, usize),
    corner: (u16, u16),
    (columns, rows): (usize, usize),
    (x, level): (f32, f32),
    diameter: usize,
) -> Placement {
    let size = (columns * cell.0, rows * cell.1);
    let (centre_x, centre_y) = plot::point_at(size, x, level);
    let half = diameter as f32 / 2.0;
    let left = (centre_x - half)
        .round()
        .clamp(0.0, size.0.saturating_sub(diameter) as f32) as usize;
    let top = (centre_y - half)
        .round()
        .clamp(0.0, size.1.saturating_sub(diameter) as f32) as usize;
    let (across, down) = (cell.0.max(1), cell.1.max(1));
    Placement {
        column: corner.0 + (left / across) as u16,
        row: corner.1 + (top / down) as u16,
        across: left % across,
        down: top % down,
        height: diameter,
    }
}

/// How wide the sample page's cursor is as a line, in pixels.
const CURSOR_LINE_PIXELS: usize = 2;

/// How wide each of the sample page's playheads is as a line, in pixels, as wide as the cursor's.
const PLAYHEAD_LINE_PIXELS: usize = 2;

/// The images of a picker's preview: its lines' waveforms, a line each, and its own.
type PreviewImages = (Vec<Option<u32>>, Option<u32>);

/// The colour of a drawing, with the cursor on it or not.
fn color(active: bool) -> [u8; 3] {
    let style = if active {
        theme::active()
    } else {
        theme::dim()
    };
    match style.fg {
        Some(Color::Rgb(red, green, blue)) => [red, green, blue],
        _ => [255; 3],
    }
}

/// What's placed directly over the screen once it's drawn, rather than through placeholders: the
/// dots on the track page's pictures, by slot, each in its shade, and the sample page's lines and
/// marks.
pub struct Placements<'a> {
    pub dots: [Option<(Shade, Placement)>; MOD_SLOTS],
    pub sample: SampleLines<'a>,
}

/// What of the sample page is placed directly: its page, for where its marks, playheads and
/// cursor are, and which of them are placed now.
pub struct SampleLines<'a> {
    pub page: &'a SamplePage,
    /// Whether the ruler's marks are placed at their lines' pixels.
    pub marks: bool,
    /// Whether the playheads are placed as thin lines.
    pub playheads: bool,
    /// Whether the cursor is placed as a thin line, and whether it's set back, as it is while the
    /// focus is on the settings rows.
    pub cursor: bool,
    pub dimmed: bool,
}

/// The images the terminal has. Each graphic panel keeps one for each of its shades, so moving the
/// cursor sends nothing.
///
/// A changed image gets a new id, rather than replacing the one with the old id. Kitty only looks at
/// placeholders on lines that have changed, and replacing an image disconnects them, so it would stay
/// blank. A new id changes the placeholders' colour, which makes kitty look again.
pub struct Graphics {
    base: u32,
    next: u32,
    /// The track page's panels: the filter's curve and the sample's waveform, the envelopes and
    /// the FX slots' curves.
    panels: PanelImages,
    /// The scope's picture, as the rows it covers in each column, and its image.
    scope: Option<(Vec<(usize, usize)>, u32)>,
    /// The picker's preview and its images.
    preview: Option<(Arc<Shown>, PreviewImages)>,
    /// The sample page's waveform and its overview, and the character size they were drawn for.
    /// They're held apart from the track page's panels, as each is sent again as soon as what it
    /// shows changes, and only then: they follow the keys, not a modulated setting, and a window
    /// that changes size leaves them be.
    sample_panels: PanelImages,
    sample_cell: (usize, usize),
    /// The image of the sample page's cursor as a line, and how tall it is in pixels.
    cursor_line: Option<(usize, u32)>,
    /// Where the line is placed, if it's on screen.
    cursor_placed: Option<Placement>,
    /// Whether the line's image is set back, as it is while the focus is on the settings rows.
    cursor_dimmed: bool,
    /// The image of the sample page's playhead as a line, and how tall it is in pixels, placed once
    /// for each playhead.
    playhead_line: Option<(usize, u32)>,
    /// Where each playhead's line is placed, by its placement id less one.
    playheads_placed: [Option<Placement>; PLAYHEADS],
    /// The images of the ruler's marks, one for each kind of point in [`MarkKind::ALL`]'s order,
    /// and the character size they're drawn for.
    mark_images: Option<((usize, usize), [u32; MarkKind::ALL.len()])>,
    /// Where each of the ruler's marks is placed, by its kind and its placement id.
    marks_placed: BTreeMap<(MarkKind, u32), Placement>,
    /// The images of the dot that rides an LFO's cycle, dim and focused, and how many pixels
    /// across they're drawn.
    dot_images: Option<(usize, [u32; 2])>,
    /// Where each LFO's dot is placed, and in which shade, by its slot, its placement id less one.
    dots_placed: [Option<(Shade, Placement)>; MOD_SLOTS],
    /// The pictures of the on/off switches, one for each look shown so far, drawn for the
    /// character size in `cell` (decision 256).
    pub(super) switch_images: BTreeMap<Look, u32>,
    /// The switches on the screen as last drawn, by the cell of each one's glyph, which
    /// [`Graphics::find_switches`] finds once ratatui has drawn it.
    switches_wanted: BTreeMap<(u16, u16), Look>,
    /// Where switches are placed, and in which look, by the cell of each one's glyph.
    switches_placed: BTreeMap<(u16, u16), Look>,
    /// A character's size in pixels, which the sample page's pictures are drawn for.
    cell: (usize, usize),
    /// The window's size in characters, as last told.
    window: Option<(u16, u16)>,
    /// How many times the screen has been brought up to date, for how often the scope is.
    frames: usize,
    /// Images to forget once the screen has moved on from them.
    pub(super) retired: Vec<u32>,
}

impl Graphics {
    /// Ready to send images, with ids another running copy won't be using.
    pub fn new() -> Self {
        Self {
            base: (std::process::id() & 0xFFFF) << 8,
            next: 0,
            panels: PanelImages::default(),
            scope: None,
            preview: None,
            sample_panels: PanelImages::default(),
            sample_cell: DEFAULT_CELL,
            cursor_line: None,
            cursor_placed: None,
            cursor_dimmed: false,
            playhead_line: None,
            playheads_placed: [None; PLAYHEADS],
            mark_images: None,
            marks_placed: BTreeMap::new(),
            dot_images: None,
            dots_placed: [None; MOD_SLOTS],
            switch_images: BTreeMap::new(),
            switches_wanted: BTreeMap::new(),
            switches_placed: BTreeMap::new(),
            cell: DEFAULT_CELL,
            window: None,
            frames: 0,
            retired: Vec::new(),
        }
    }

    /// Draw the pictures for characters of this many pixels, as the terminal says its window has.
    /// The panels' pictures are drawn at those pixels, so when they change size, as the font's
    /// size does, each is drawn and sent again.
    pub fn use_cell(&mut self, cell: (usize, usize)) {
        if cell.0 > 0 && cell.1 > 0 && cell != self.cell {
            self.cell = cell;
            self.retired.extend(self.panels.clear());
            self.retire_switches();
        }
    }

    /// Take in the window's size as the terminal says it, in characters and pixels. ratatui clears
    /// a window that's changed size, and in kitty and Ghostty a clear frees an image that isn't
    /// shown through placeholders, so the cursor's line and the ruler's marks are sent again. The
    /// panels' pictures are too, with new ids, so none is left blank after the clear.
    pub fn use_window(&mut self, size: terminal::WindowSize) {
        self.use_cell((
            usize::from(size.width) / usize::from(size.columns.max(1)),
            usize::from(size.height) / usize::from(size.rows.max(1)),
        ));
        let characters = (size.columns, size.rows);
        if self.window.is_some_and(|was| was != characters) {
            self.retired
                .extend(self.cursor_line.take().map(|(_, id)| id));
            self.cursor_placed = None;
            self.retired
                .extend(self.playhead_line.take().map(|(_, id)| id));
            self.playheads_placed = [None; PLAYHEADS];
            self.retired
                .extend(self.mark_images.take().into_iter().flat_map(|(_, ids)| ids));
            self.marks_placed.clear();
            self.retired
                .extend(self.dot_images.take().into_iter().flat_map(|(_, ids)| ids));
            self.dots_placed = [None; MOD_SLOTS];
            self.retired.extend(self.panels.clear());
            self.retire_switches();
        }
        self.window = Some(characters);
    }

    /// An id no image has at the moment.
    fn new_id(&mut self) -> u32 {
        loop {
            self.next = self.next % 0xFF + 1;
            let id = self.base | self.next;
            let in_use = self.panels.ids().any(|held| held == id)
                || self.scope.as_ref().is_some_and(|image| image.1 == id)
                || self.sample_panels.ids().any(|held| held == id)
                || self.cursor_line.is_some_and(|image| image.1 == id)
                || self.playhead_line.is_some_and(|image| image.1 == id)
                || self.mark_images.is_some_and(|(_, ids)| ids.contains(&id))
                || self.dot_images.is_some_and(|(_, ids)| ids.contains(&id))
                || self.switch_images.values().any(|held| *held == id)
                || self
                    .preview
                    .iter()
                    .flat_map(|(_, (ids, wave))| ids.iter().chain(std::iter::once(wave)).flatten())
                    .any(|image| *image == id)
                || self.retired.contains(&id);
            if !in_use {
                return id;
            }
        }
    }

    /// A character's size in pixels, which the pictures are drawn for.
    pub fn cell(&self) -> (usize, usize) {
        self.cell
    }

    /// Start a frame: have the terminal forget the images the screen has moved on from. Then the
    /// pictures the screen is about to show are sent, with [`Graphics::panels`] and the rest,
    /// and [`Graphics::finish`] ends it.
    pub fn begin(&mut self, out: &mut impl Write) -> io::Result<()> {
        for id in self.retired.drain(..) {
            forget(out, id)?;
        }
        Ok(())
    }

    /// End a frame's sending, before ratatui draws.
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.frames = self.frames.wrapping_add(1);
        out.flush()
    }

    /// Place what the terminal places itself rather than through placeholders, once the screen
    /// is drawn: the sample page's ruler marks, its playheads and its cursor as lines, the
    /// cursor's last so it's over a playhead it meets, the dots on the track page's pictures, and
    /// the on/off switches [`Graphics::find_switches`] found. After the drawing, so they go where
    /// the page is now, and so a clear as the window changes size has already happened.
    pub fn place(&mut self, placed: &Placements, out: &mut impl Write) -> io::Result<()> {
        self.sync_marks(&placed.sample, out)?;
        self.sync_playheads(&placed.sample, out)?;
        self.sync_cursor_line(&placed.sample, out)?;
        self.sync_dots(placed.dots, out)?;
        self.sync_switches(out)?;
        out.flush()
    }

    /// Find the on/off switches on the screen ratatui has just drawn, for [`Graphics::place`] to
    /// place their pictures over, and hide their glyphs under them (decision 256). Called on the
    /// frame being drawn, before it reaches the terminal.
    pub fn find_switches(&mut self, buffer: &mut Buffer) {
        self.switches_wanted = switches::find(buffer);
    }

    /// Let go of the switches' pictures, as after the screen is cleared or the characters change
    /// size, so each is drawn and stored again, and placed again where it's wanted.
    fn retire_switches(&mut self) {
        self.retired
            .extend(std::mem::take(&mut self.switch_images).into_values());
        self.switches_placed.clear();
    }

    /// Place a picture over each switch on the screen, and take it off where one has gone or
    /// changed. Each look's picture is stored the first time it's shown and kept, and each switch
    /// is a placement of it under an id from its cell, so a switch flipping places one picture
    /// and takes off another, and nothing is sent while the switches stay as they are.
    pub(super) fn sync_switches(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.switches_wanted == self.switches_placed {
            return Ok(());
        }
        let placement = |(column, row): (u16, u16)| (u32::from(row) << 16 | u32::from(column)) + 1;
        for (at, look) in &self.switches_placed {
            if self.switches_wanted.get(at) != Some(look)
                && let Some(id) = self.switch_images.get(look)
            {
                unplace(out, *id, placement(*at))?;
            }
        }
        let wanted = self.switches_wanted.clone();
        for (at, look) in &wanted {
            if self.switches_placed.get(at) == Some(look) {
                continue;
            }
            let id = match self.switch_images.get(look) {
                Some(id) => *id,
                None => {
                    let id = self.new_id();
                    store(out, id, &switches::png(*look, self.cell))?;
                    self.switch_images.insert(*look, id);
                    id
                }
            };
            let (column, row) = *at;
            let at = Placement {
                column,
                row,
                across: 0,
                down: 0,
                height: self.cell.1,
            };
            place_at(out, id, placement((column, row)), at)?;
        }
        self.switches_placed = wanted;
        Ok(())
    }

    /// Place each LFO's dot on its cycle, or take it off. The dot is an image of its own in each
    /// of two shades, sent once, and placed once for each LFO, so moving it is placing it again,
    /// about 60 bytes, and nothing is sent while nothing moves.
    fn sync_dots(
        &mut self,
        wanted: [Option<(Shade, Placement)>; MOD_SLOTS],
        out: &mut impl Write,
    ) -> io::Result<()> {
        if wanted == self.dots_placed {
            return Ok(());
        }
        let diameter = dot_pixels(self.cell);
        let ids = match self.dot_images {
            Some((drawn, ids)) if drawn == diameter => ids,
            // None wanted, and none drawn at this size: any placed go, with their images.
            _ if wanted.iter().all(Option::is_none) => {
                self.retired
                    .extend(self.dot_images.take().into_iter().flat_map(|(_, ids)| ids));
                self.dots_placed = [None; MOD_SLOTS];
                return Ok(());
            }
            old => {
                // Drawn for another size of character: the old ones go, with their placements.
                for id in old.into_iter().flat_map(|(_, ids)| ids) {
                    forget(out, id)?;
                }
                self.dots_placed = [None; MOD_SLOTS];
                let mut ids = [0; 2];
                for (index, shade) in [Shade::Dim, Shade::Focused].into_iter().enumerate() {
                    // Held as each is given its id, so the next can't be given the same.
                    self.dot_images = Some((diameter, ids));
                    let id = self.new_id();
                    store(out, id, &plot::dot(diameter, shade.color()))?;
                    ids[index] = id;
                }
                self.dot_images = Some((diameter, ids));
                ids
            }
        };
        let image = |shade: Shade| ids[usize::from(shade == Shade::Focused)];
        for (slot, (now, was)) in wanted.iter().zip(&self.dots_placed).enumerate() {
            let placement = slot as u32 + 1;
            match (now, was) {
                (Some(now), Some(was)) if now == was => {}
                (Some((shade, at)), was) => {
                    // A dot changing shade moves to the other image.
                    if let Some((before, _)) = was
                        && before != shade
                    {
                        unplace(out, image(*before), placement)?;
                    }
                    place_at(out, image(*shade), placement, *at)?;
                }
                (None, Some((before, _))) => unplace(out, image(*before), placement)?,
                (None, None) => {}
            }
        }
        self.dots_placed = wanted;
        Ok(())
    }

    /// Where a line at a frame goes on the sample page's waveform: at its pixel's character, the
    /// pixel's place in it across, the waveform's height down. None if it's out of view.
    fn line_at(&self, page: &SamplePage, screen: &WaveOnScreen, frame: u64) -> Option<Placement> {
        let across = self.cell.0 as u128;
        let after = frame.checked_sub(page.first_frame())?;
        let pixels = u128::from(after) * across / u128::from(page.zoom.max(1));
        let column = i32::try_from(i128::from(screen.left) + (pixels / across) as i128).ok()?;
        screen.columns.contains(&column).then(|| Placement {
            column: column as u16,
            row: screen.top,
            across: (pixels % across) as usize,
            down: 0,
            height: usize::from(screen.rows) * self.cell.1,
        })
    }

    /// Place the sample page's playheads as thin green lines at their frames, as the cursor's line
    /// is placed, or take them away when nothing plays, they're out of view, or the cursor is shown
    /// as its column, when they're their columns too. One image is placed once for each, so a
    /// playhead moving is placing it again, about 60 bytes, and nothing is sent while nothing
    /// moves, so a page with nothing playing sends nothing.
    fn sync_playheads(&mut self, sample: &SampleLines, out: &mut impl Write) -> io::Result<()> {
        let page = sample.page;
        let mut wanted = [None; PLAYHEADS];
        if let Some(screen) = &page.on_screen
            && sample.playheads
        {
            let placed = screen
                .playheads
                .iter()
                .filter_map(|frame| self.line_at(page, screen, *frame));
            for (slot, at) in wanted.iter_mut().zip(placed) {
                *slot = Some(at);
            }
        }
        if wanted == self.playheads_placed {
            return Ok(());
        }
        let height = usize::from(WAVE_ROWS) * self.cell.1;
        let id = match self.playhead_line.take() {
            Some((drawn, id)) if drawn == height => id,
            old => {
                // Drawn for another size: the old one goes, with its placements.
                if let Some((_, id)) = old {
                    forget(out, id)?;
                }
                self.playheads_placed = [None; PLAYHEADS];
                let id = self.new_id();
                let mut line = Picture::new(PLAYHEAD_LINE_PIXELS, height, vec![rgb(theme::GREEN)]);
                line.pixels.fill(1);
                store(out, id, &line.png())?;
                id
            }
        };
        self.playhead_line = Some((height, id));
        for (index, (at, was)) in wanted.iter().zip(&self.playheads_placed).enumerate() {
            let placement = index as u32 + 1;
            match at {
                Some(at) if was != &Some(*at) => place_at(out, id, placement, *at)?,
                None if was.is_some() => unplace(out, id, placement)?,
                _ => {}
            }
        }
        self.playheads_placed = wanted;
        Ok(())
    }

    /// Send the waveforms in the picker's preview, if the terminal lacks them, and forget those of a
    /// preview it has moved on from, giving back the ids of the preview shown. Only as many as the
    /// preview has room for are sent.
    pub fn preview(
        &mut self,
        shown: Option<Arc<Shown>>,
        out: &mut impl Write,
    ) -> io::Result<Option<&PreviewImages>> {
        let same = match (&self.preview, &shown) {
            (Some((held, _)), Some(shown)) => Arc::ptr_eq(held, shown),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some((_, (ids, wave))) = self.preview.take() {
                self.retired.extend(ids.into_iter().flatten());
                self.retired.extend(wave);
            }
            if let Some(shown) = shown {
                let mut ids = Vec::new();
                for line in shown.lines.iter().take(PREVIEW_ROWS) {
                    let Some(spans) = &line.wave else {
                        ids.push(None);
                        continue;
                    };
                    let id = self.new_id();
                    let png = spans_png(spans, PREVIEW_WAVE_HEIGHT, color(false));
                    self.send(out, id, &png, PREVIEW_WAVE_WIDTH, PREVIEW_WAVE_HEIGHT)?;
                    ids.push(Some(id));
                }
                let wave = match &shown.wave {
                    Some(spans) => {
                        let id = self.new_id();
                        let png = spans_png(spans, PREVIEW_SAMPLE_HEIGHT, color(false));
                        self.send(out, id, &png, PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT)?;
                        Some(id)
                    }
                    None => None,
                };
                self.preview = Some((shown, (ids, wave)));
            }
        }
        Ok(self.preview.as_ref().map(|(_, images)| images))
    }

    /// Send the scope's picture, if it has changed since the last one sent, and give back its id.
    /// It's brought up to date once in [`SCOPE_FRAMES`] frames, and gives back none in between. A
    /// steady note, or silence, sends nothing.
    pub fn scope(&mut self, scope: &Scope, out: &mut impl Write) -> io::Result<Option<u32>> {
        if !self.frames.is_multiple_of(SCOPE_FRAMES) {
            return Ok(None);
        }
        let spans = scope.spans(
            SCOPE_WIDTH * CELL_DOTS_ACROSS,
            SCOPE_HEIGHT * CELL_DOTS_DOWN,
        );
        let id = match self.scope.take() {
            Some((drawn, id)) if drawn == spans => {
                self.scope = Some((drawn, id));
                id
            }
            held => {
                // Retired first, so the new one can't be given its id.
                self.retired.extend(held.map(|(_, old)| old));
                let id = self.new_id();
                let png = scope_png(&spans, color(false));
                self.send(out, id, &png, SCOPE_WIDTH, SCOPE_HEIGHT)?;
                self.scope = Some((spans, id));
                id
            }
        };
        Ok(Some(id))
    }

    /// Place the sample page's cursor as a thin line at its frame, or take it away when it isn't
    /// on screen, is shown as its column, or something is drawn over the page. It's an image of its
    /// own, sent once, and moving it is placing it again, so the waveform isn't sent again.
    fn sync_cursor_line(&mut self, sample: &SampleLines, out: &mut impl Write) -> io::Result<()> {
        let page = sample.page;
        let height = usize::from(WAVE_ROWS) * self.cell.1;
        let wanted = match &page.on_screen {
            Some(screen) if sample.cursor => {
                self.line_at(page, screen, page.cursor.max(page.first_frame()))
            }
            _ => None,
        };
        let dimmed = sample.dimmed;
        if wanted == self.cursor_placed && dimmed == self.cursor_dimmed {
            return Ok(());
        }
        let Some(at) = wanted else {
            if let Some((_, id)) = self.cursor_line {
                unplace(out, id, 1)?;
            }
            self.cursor_placed = None;
            return Ok(());
        };
        let id = match self.cursor_line.take() {
            Some((drawn, id)) if drawn == height && dimmed == self.cursor_dimmed => id,
            old => {
                self.retired.extend(old.map(|(_, id)| id));
                let id = self.new_id();
                let color = if dimmed {
                    theme::dimmed(theme::ROSEWATER)
                } else {
                    theme::ROSEWATER
                };
                let mut line = Picture::new(CURSOR_LINE_PIXELS, height, vec![rgb(color)]);
                line.pixels.fill(1);
                store(out, id, &line.png())?;
                id
            }
        };
        self.cursor_line = Some((height, id));
        self.cursor_dimmed = dimmed;
        place_at(out, id, 1, at)?;
        self.cursor_placed = Some(at);
        Ok(())
    }

    /// Place the sample page's ruler marks, each a small triangle at its line's pixels, or take
    /// them away when the pictures aren't drawn or something is drawn over the page. Each kind of
    /// point's mark is an image of its own, sent once, and placed once for each of its points in
    /// view, so a scroll, a zoom or a moved point only places the marks that moved, and moving the
    /// cursor sends nothing.
    fn sync_marks(&mut self, sample: &SampleLines, out: &mut impl Write) -> io::Result<()> {
        let page = sample.page;
        let (across, down) = self.cell;
        let mut wanted = BTreeMap::new();
        if let Some(screen) = &page.on_screen
            && screen.top > 0
            && sample.marks
        {
            let left = i64::from(screen.left);
            let shown = i64::from(screen.columns.start)..i64::from(screen.columns.end);
            for mark in &screen.marks {
                // Only a mark whose line is in view, as the waveform has it.
                let line = mark.line_pixel(page, across);
                if !shown.contains(&(left + line.div_euclid(across as i64))) {
                    continue;
                }
                let (first, _) = mark.pixels(page, across);
                let Ok(pixel) = usize::try_from(left * across as i64 + first) else {
                    continue;
                };
                wanted.insert(
                    (mark.kind, mark.placement),
                    Placement {
                        column: (pixel / across) as u16,
                        row: screen.top - 1,
                        across: pixel % across,
                        down: 0,
                        height: down,
                    },
                );
            }
        }
        if wanted == self.marks_placed {
            return Ok(());
        }
        if wanted.is_empty() {
            // Taken off all at once, a command for each kind rather than each mark.
            if let Some((_, ids)) = self.mark_images {
                for kind in MarkKind::ALL {
                    if self.marks_placed.keys().any(|(placed, _)| *placed == kind) {
                        unplace_all(out, ids[kind.index()])?;
                    }
                }
            }
            self.marks_placed.clear();
            return Ok(());
        }
        let ids = match self.mark_images {
            Some((drawn, ids)) if drawn == self.cell => ids,
            old => {
                // Drawn for another size of character: the old ones go, with their placements.
                for id in old.into_iter().flat_map(|(_, ids)| ids) {
                    forget(out, id)?;
                }
                self.marks_placed.clear();
                let mut ids = [0; MarkKind::ALL.len()];
                for kind in MarkKind::ALL {
                    // Held as each is given its id, so the next kind can't be given the same.
                    self.mark_images = Some((self.cell, ids));
                    let id = self.new_id();
                    store(out, id, &mark_png(kind, self.cell))?;
                    ids[kind.index()] = id;
                }
                self.mark_images = Some((self.cell, ids));
                ids
            }
        };
        for key in self.marks_placed.keys() {
            if !wanted.contains_key(key) {
                unplace(out, ids[key.0.index()], key.1)?;
            }
        }
        for (key, at) in &wanted {
            if self.marks_placed.get(key) != Some(at) {
                place_at(out, ids[key.0.index()], key.1, *at)?;
            }
        }
        self.marks_placed = wanted;
        Ok(())
    }

    /// Send whichever of a page's pictures the terminal lacks, each drawn at the terminal's own
    /// pixels, and give back each one's id, in the order wanted. A picture that changes is sent
    /// at most once in [`PANEL_FRAMES`] frames, with the last kept in between, so a curve
    /// following an LFO costs no more than the scope; the image it replaces is forgotten once the
    /// screen has moved on from it.
    pub(crate) fn panels(
        &mut self,
        wants: &[PanelWant],
        rate: u32,
        out: &mut impl Write,
    ) -> io::Result<PanelIds> {
        let mut ids = PanelIds::default();
        for want in wants {
            ids.insert(want.key, self.panel(false, want, rate, out)?);
        }
        Ok(ids)
    }

    /// Send whichever of the sample page's pictures, its waveform and its overview, the terminal
    /// lacks, and give back their ids. Each is sent again the moment what it shows changes, the
    /// zoom, the view, the points or the size of the characters, never for the cursor alone.
    pub(crate) fn sample_panels(
        &mut self,
        wants: &[PanelWant],
        rate: u32,
        out: &mut impl Write,
    ) -> io::Result<PanelIds> {
        let mut ids = PanelIds::default();
        if !wants.is_empty() && self.sample_cell != self.cell {
            // Drawn for another size of character: the old ones go, retired before the new ones
            // are given ids.
            self.retired.extend(self.sample_panels.clear());
            self.sample_cell = self.cell;
        }
        for want in wants {
            ids.insert(want.key, self.panel(true, want, rate, out)?);
        }
        Ok(ids)
    }

    /// The id of a panel's picture in a shade, sending it first if the terminal lacks one that
    /// shows its shape: one of the sample page's, or another page's, which waits a little after a
    /// change before it's sent again.
    fn panel(
        &mut self,
        sample: bool,
        want: &PanelWant,
        rate: u32,
        out: &mut impl Write,
    ) -> io::Result<u32> {
        let PanelWant {
            key,
            shade,
            shape,
            cells,
        } = want;
        let (images, every) = if sample {
            (&self.sample_panels, 0)
        } else {
            (&self.panels, PANEL_FRAMES)
        };
        match images.find(*key, *shade, shape, self.frames, every) {
            Found::Current(id) | Found::Waiting(id) => return Ok(id),
            Found::Missing => {}
        }
        let id = self.new_id();
        let png = shape.png(*cells, self.cell, *shade, rate);
        self.send(out, id, &png, cells.0, cells.1)?;
        let images = if sample {
            &mut self.sample_panels
        } else {
            &mut self.panels
        };
        let replaced = images.hold(*key, *shade, shape.clone(), id, self.frames);
        self.retired.extend(replaced);
        Ok(id)
    }

    /// Send one image.
    fn send(
        &self,
        out: &mut impl Write,
        id: u32,
        png: &[u8],
        columns: usize,
        rows: usize,
    ) -> io::Result<()> {
        let image = Image {
            id,
            png,
            columns,
            rows,
        };
        transmit(out, &image)
    }

    /// Have the terminal forget the images.
    pub fn forget(&self, out: &mut impl Write) -> io::Result<()> {
        let panels: Vec<u32> = self.panels.ids().chain(self.sample_panels.ids()).collect();
        let scope = self.scope.iter().map(|image| &image.1);
        let sample = self
            .cursor_line
            .iter()
            .map(|image| &image.1)
            .chain(self.playhead_line.iter().map(|image| &image.1))
            .chain(self.mark_images.iter().flat_map(|(_, ids)| ids))
            .chain(self.dot_images.iter().flat_map(|(_, ids)| ids))
            .chain(self.switch_images.values());
        for id in panels
            .iter()
            .chain(scope)
            .chain(sample)
            .chain(&self.retired)
        {
            forget(out, *id)?;
        }
        out.flush()
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}
