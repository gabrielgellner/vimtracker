//! The track page's pictures, as data: which of its panels' pictures it wants on screen this frame,
//! worked out from its state and what it's given, the ids it's given back for them, the zoomed
//! slot's caption, and where the dots on its LFO cycles and generator previews go. `Graphics`
//! sends them and places the dots; the page reads no `App`.

use std::sync::Arc;
use std::time::Instant;

use vt_model::insert::INSERTS;
use vt_model::modulation::{MOD_SLOTS, ModKind};
use vt_model::song::{Machine, Track};

use crate::controls::{Control, Page};

use super::TrackPage;
use crate::graphics::generators::GenDrawing;
use crate::graphics::plot::{LfoDrawing, Shade, TrackDrawing};
use crate::graphics::slots::SlotDrawing;
use crate::graphics::{
    PanelIds, PanelKey, PanelWant, Placement, Shape, dot_pixels, dot_placement, placed_dot,
};
use crate::pages::Ctx;
use crate::pages::sample::wave::Thumbnail;
use crate::pages::track::draw::{
    FILTER_CURVE_HEIGHT, FILTER_CURVE_WIDTH, LARGE_PANEL_HEIGHT, LARGE_PANEL_WIDTH,
    MOD_PANEL_HEIGHT, MOD_PANEL_WIDTH, SLOT_DRAWING_HEIGHT, WAVEFORM_HEIGHT, WAVEFORM_WIDTH,
    filter_being_edited, heard_bypassed, machine_being_edited, mod_being_edited, slot_being_edited,
};

impl TrackPage {
    /// The pictures the page wants on screen for the cursor's track, in the order they're sent:
    /// the Sound page's filter curve and a sampler's thumbnail, the Mod page's slots, the FX
    /// page's slots or its zoomed one, or the Gen page's previews. Each shows what's playing, as a
    /// value's bar does, and is drawn from the settings it shows, so it's sent again only when
    /// they change.
    pub(crate) fn pictures(&self, ctx: &Ctx) -> Vec<PanelWant> {
        let Some(track) = ctx.track_here() else {
            return Vec::new();
        };
        match self.page {
            Page::Sound if track.is_sound() => self.sound_panels(ctx, track),
            Page::Sound => Vec::new(),
            Page::Mod => self.mod_panels(ctx, track),
            Page::Fx => self.fx_panels(ctx, track),
            Page::Gen => self.gen_panels(ctx, track),
        }
    }

    /// Keep the ids of the pictures wanted, for the drawing, and the zoomed slot's caption, worked
    /// out again only when its drawing changes, as a reverb's tail takes a few milliseconds.
    pub(crate) fn take_images(&mut self, ids: PanelIds, wants: &[PanelWant], rate: u32) {
        self.images = ids;
        if self.page != Page::Fx || !self.zoomed {
            return;
        }
        let shown = wants.iter().find_map(|want| match (want.key, &want.shape) {
            (PanelKey::Zoom, Shape::Slot(drawing)) => Some(*drawing),
            _ => None,
        });
        if self.zoom_caption.as_ref().map(|(of, _)| of) != shown.as_ref() {
            self.zoom_caption = shown.map(|shown| (shown, shown.caption(rate)));
        }
    }

    /// What's said under the zoomed slot's box: a delay's or a reverb's time scale.
    pub(crate) fn zoom_caption(&self) -> Option<&str> {
        self.zoom_caption
            .as_ref()
            .and_then(|(_, caption)| caption.as_deref())
    }

    /// The filter's curve, as it's playing, and a sampler's thumbnail once its file has been
    /// analysed: the slice its `Slice` is on, with its `Start` and `Length` as the sample page's
    /// start and end lines.
    fn sound_panels(&self, ctx: &Ctx, track: &Track) -> Vec<PanelWant> {
        // As the engine hears them, a step's lock and modulation included, or as they're set
        // before it has said.
        let settings = ctx.live().map_or(
            (
                track.filter,
                track.filter.plays(track.filter_mode),
                track.cutoff,
                track.resonance,
                track.drive,
                track.safe,
                track.low_cut,
                track.high_cut,
            ),
            |live| {
                let filter = live.filter;
                (
                    filter.kind,
                    filter.mode,
                    live.cutoff,
                    live.resonance,
                    filter.drive,
                    filter.safe,
                    filter.low_cut,
                    filter.high_cut,
                )
            },
        );
        let mut wants = vec![PanelWant {
            key: PanelKey::Filter,
            shade: Shade::of(filter_being_edited(self, ctx), false),
            shape: Shape::Filter(settings),
            cells: (FILTER_CURVE_WIDTH, FILTER_CURVE_HEIGHT),
        }];
        if track.machine == Machine::Sampler
            && let Some(thumbnail) = thumbnail(ctx, track)
        {
            wants.push(PanelWant {
                key: PanelKey::Waveform,
                shade: Shade::of(machine_being_edited(self, ctx), false),
                shape: Shape::Waveform(thumbnail),
                cells: (WAVEFORM_WIDTH, WAVEFORM_HEIGHT),
            });
        }
        wants
    }

    /// Each mod slot's picture, faint while it's switched out: an envelope's shape, an LFO's
    /// cycle or a Track slot's map, by its type. An LFO's cycle is sent again only when its wave,
    /// its `Phase` or its depth's sign changes, and a slot's when its type does. Steps' values are
    /// a row of bars drawn as text, which the cursor edits, so they have no picture.
    fn mod_panels(&self, ctx: &Ctx, track: &Track) -> Vec<PanelWant> {
        let edited = mod_being_edited(self, ctx);
        let mut wants = Vec::new();
        for (slot, modulator) in track.mods.iter().enumerate() {
            let shape = match modulator.kind {
                ModKind::Adsr | ModKind::Ahd | ModKind::Drum | ModKind::Lpg => {
                    Shape::Envelope(modulator.envelope(), modulator.polarity())
                }
                ModKind::Lfo => Shape::Lfo(
                    LfoDrawing::of(&modulator.lfo, modulator.depth)
                        .in_polarity(modulator.polarity()),
                ),
                ModKind::Track => Shape::Track(TrackDrawing::of(modulator)),
                ModKind::Steps => continue,
            };
            wants.push(PanelWant {
                key: PanelKey::Mod(slot),
                shade: Shade::of(edited == Some(slot), modulator.bypassed),
                shape,
                cells: (MOD_PANEL_WIDTH, MOD_PANEL_HEIGHT),
            });
        }
        wants
    }

    /// The FX slots' drawings, a compressor's or an EQ's curve, faint while its slot is switched
    /// out, or, zoomed in, the zoomed slot's alone, bigger, which can be a distortion's transfer
    /// curve, a filter effect's response, a delay's repeats or a reverb's tail too.
    fn fx_panels(&self, ctx: &Ctx, track: &Track) -> Vec<PanelWant> {
        if self.zoomed {
            let slot = self
                .device_cursor(Some(track))
                .map_or(0, |cursor| cursor.device)
                .min(INSERTS - 1);
            let tempo = (ctx.rate(), ctx.song().millibpm);
            let live = ctx.live();
            let Some(shown) = SlotDrawing::zoomed(track.inserts[slot], live.as_ref(), slot, tempo)
            else {
                return Vec::new();
            };
            // The zoomed slot always has the cursor.
            return vec![PanelWant {
                key: PanelKey::Zoom,
                shade: Shade::of(true, heard_bypassed(ctx, track, slot)),
                shape: Shape::Slot(shown),
                cells: shown.zoomed_size(),
            }];
        }
        let edited = slot_being_edited(self, ctx);
        let mut wants = Vec::new();
        for (slot, insert) in track.inserts.iter().enumerate() {
            let live = ctx.live().map(|live| live.inserts[slot]);
            let Some(shown) = SlotDrawing::of(*insert, live) else {
                continue;
            };
            wants.push(PanelWant {
                key: PanelKey::Slot(slot),
                shade: Shade::of(edited == Some(slot), heard_bypassed(ctx, track, slot)),
                shape: Shape::Slot(shown),
                cells: (shown.width(), SLOT_DRAWING_HEIGHT),
            });
        }
        wants
    }

    /// Each live generator's preview, the cursor's focused, sent again as its column's values
    /// change.
    fn gen_panels(&self, ctx: &Ctx, track: &Track) -> Vec<PanelWant> {
        let Some(phrase) = self.phrase.and_then(|phrase| track.phrases.get(&phrase)) else {
            return Vec::new();
        };
        let edited = self.generator_here(ctx);
        let mut wants = Vec::new();
        for (at, generator) in phrase.generators().iter().enumerate() {
            let Some(drawing) = GenDrawing::of(generator, phrase.steps()) else {
                continue;
            };
            wants.push(PanelWant {
                key: PanelKey::Gen(at),
                shade: Shade::of(edited == Some(at), false),
                shape: Shape::Gen(drawing),
                cells: (LARGE_PANEL_WIDTH, LARGE_PANEL_HEIGHT),
            });
        }
        wants
    }

    /// The place in its phrase's list of the generator the cursor is on, on the Gen page.
    fn generator_here(&self, ctx: &Ctx) -> Option<usize> {
        match self.control_here(ctx) {
            Control::Gen { generator, .. } => Some(generator),
            _ => None,
        }
    }

    /// Where each dot goes, by slot, and in which shade, for characters of `cell` pixels: on the
    /// Mod page, each LFO's on its cycle where it is now, while it runs, and a Track slot's on its
    /// map at its last note; on the Gen page, the row playing on each live generator's preview,
    /// while the phrase it shows is the one playing. They take the places the cycles were last
    /// drawn at, and none is placed while something is drawn over the page, a slot is switched
    /// out or going nowhere, or the song is stopped.
    pub(crate) fn dots(
        &self,
        ctx: &Ctx,
        cell: (usize, usize),
        now: Instant,
    ) -> [Option<(Shade, Placement)>; MOD_SLOTS] {
        match self.page {
            Page::Gen => self.gen_dots(ctx, cell, now),
            Page::Mod => self.mod_dots(ctx, cell, now),
            Page::Sound | Page::Fx => [None; MOD_SLOTS],
        }
    }

    /// Where each LFO's dot goes on the Mod page.
    fn mod_dots(
        &self,
        ctx: &Ctx,
        cell: (usize, usize),
        now: Instant,
    ) -> [Option<(Shade, Placement)>; MOD_SLOTS] {
        let mut wanted = [None; MOD_SLOTS];
        let (Some(track), Some(live)) = (ctx.track_here(), ctx.live()) else {
            return wanted;
        };
        if ctx.overlaid(now) {
            return wanted;
        }
        let edited = mod_being_edited(self, ctx);
        let diameter = dot_pixels(cell);
        for (slot, modulator) in track.mods.iter().enumerate() {
            let (Some(corner), Some(at)) = (self.lfo_cycles_at[slot], live.mods[slot]) else {
                continue;
            };
            if !matches!(modulator.kind, ModKind::Lfo | ModKind::Track)
                || modulator.bypassed
                || modulator.destination.is_none()
            {
                continue;
            }
            let shade = Shade::of(edited == Some(slot), false);
            let placement = dot_placement(cell, corner, modulator, at, diameter);
            wanted[slot] = Some((shade, placement));
        }
        wanted
    }

    /// Where the row playing goes on each live generator's preview on the Gen page. They take the
    /// places the LFOs' dots have on the Mod page, a preview on screen each.
    fn gen_dots(
        &self,
        ctx: &Ctx,
        cell: (usize, usize),
        now: Instant,
    ) -> [Option<(Shade, Placement)>; MOD_SLOTS] {
        let mut wanted = [None; MOD_SLOTS];
        if ctx.overlaid(now) {
            return wanted;
        }
        let Some(phrase) = self
            .phrase
            .and_then(|phrase| ctx.track_here()?.phrases.get(&phrase))
        else {
            return wanted;
        };
        let edited = self.generator_here(ctx);
        let diameter = dot_pixels(cell);
        let size = (LARGE_PANEL_WIDTH, LARGE_PANEL_HEIGHT);
        for (place, (at, corner)) in wanted.iter_mut().zip(&self.gen_panels_at) {
            let Some(generator) = phrase.generators().get(*at) else {
                continue;
            };
            let (Some(drawing), Some(row)) = (
                GenDrawing::of(generator, phrase.steps()),
                ctx.gen_playing_row(generator),
            ) else {
                continue;
            };
            let shade = Shade::of(edited == Some(*at), false);
            let point = drawing.point(row);
            *place = Some((shade, placed_dot(cell, *corner, size, point, diameter)));
        }
        wanted
    }
}

/// What a sampler track plays, as its thumbnail shows it, once its file has been analysed.
fn thumbnail(ctx: &Ctx, track: &Track) -> Option<Thumbnail> {
    let entry = ctx.song().bank.get(track.sample())?;
    let source = entry.source()?;
    let samples = ctx.samples();
    let analysis = Arc::clone(samples.analysis(source.hash)?);
    let frames = samples.id_of(&source.path).and_then(|id| {
        let data = samples.shared(id)?;
        Some((Arc::clone(data), samples.rates(id)))
    });
    Some(Thumbnail::of(entry, analysis, frames, track))
}
