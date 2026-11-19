//! What an FX slot's drawing shows, in the rack and zoomed in on: a compressor's or an EQ's curve,
//! a distortion's transfer curve, a filter effect's response, a delay's repeats or a reverb's tail,
//! with the caption a zoomed one has under its box.

use vt_engine::pictures::{
    DEFAULT_NOTE, Tail, comb, delay, echo, hall, lpg, plate, resonator, reverb, space, spring,
    transfer, vowel,
};
use vt_engine::track_state::LiveValues;
use vt_model::insert::{
    Comb, Compressor, Delay, Distort, Echo, Equaliser, GateMode, Insert, LowPassGate, MAX_PARAMS,
    Resonator, Vowel, shift_semitones,
};

use super::plot::{self, Filtering, Plot};
use crate::pages::track::draw::{
    DYNAMICS_CURVE_WIDTH, EQ_CURVE_WIDTH, ZOOM_CURVE_WIDTH, ZOOM_EQ_WIDTH, ZOOM_PANEL_HEIGHT,
};

/// What an FX slot's drawing shows, which is all that's compared to tell if it's changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotDrawing {
    /// A compressor's curve, from its type, threshold and ratio, with what doesn't bend it left
    /// as it starts.
    Compressor(Compressor),
    /// An EQ's curve, from all of its settings.
    Eq(Equaliser),
    /// A distortion's transfer curve, which only the FX page's zoom draws.
    Distort(Distort),
    /// The filter effects' curves, which only the zoom draws too: a low-pass gate's as far as it's
    /// open now, and a resonator's, comb's or vowel filter's at the note the track is playing.
    Lpg(LowPassGate, Opening),
    Reso(Resonator, Cents),
    Comb(Comb, Cents),
    Vowel(Vowel, Cents),
    /// A delay's repeats, at the song's tempo in thousandths of a BPM, and a Space Echo's.
    Delay(Delay, u32),
    Echo(Echo, u32),
    /// A reverb's tail: a `Reverb`, `Plate`, `Hall`, `Spring` or `Space`.
    Tail(Insert),
}

/// What a delay's drawing shows: the sound and each repeat, when and how loud on each side, how
/// long a beat is and how many seconds across it goes, all in seconds.
struct Repeated {
    sounds: Vec<(f32, [f32; 2])>,
    beat: f32,
    span: f32,
}

impl Repeated {
    fn of(settings: Delay, millibpm: u32, rate: u32) -> Self {
        let shown = delay::repeats(settings, delay::frames_per_tick(millibpm, rate), rate);
        Self::shown(&shown, millibpm)
    }

    /// A Space Echo's, whose repeats come at each of its heads and as they go round.
    fn of_echo(settings: Echo, millibpm: u32, rate: u32) -> Self {
        let shown = echo::repeats(settings, delay::frames_per_tick(millibpm, rate), rate);
        Self::shown(&shown, millibpm)
    }

    fn shown(shown: &delay::Repeats, millibpm: u32) -> Self {
        let mut sounds = vec![(0.0, [shown.dry; 2])];
        sounds.extend(shown.taps.iter().map(|tap| (tap.seconds, tap.levels)));
        // A repeat's time past the last, so it isn't on the edge.
        let time = shown.taps.first().map_or(0.0, |tap| tap.seconds);
        let beat = 60_000.0 / millibpm.max(1) as f32;
        let last = shown.taps.last().map_or(0.0, |tap| tap.seconds);
        Self {
            sounds,
            beat,
            span: (last + time).max(beat),
        }
    }

    fn caption(&self) -> String {
        format!("{:.2} s across, a line a beat", self.span)
    }
}

/// A reverb's tail, if it has one worked out: a `Reverb`'s, `Plate`'s, `Hall`'s, `Spring`'s or
/// `Space`'s.
fn tail_of(insert: Insert, rate: u32) -> Option<Tail> {
    match insert {
        Insert::Reverb(settings) => reverb::tail(settings, rate),
        Insert::Plate(settings) => plate::tail(settings, rate),
        Insert::Hall(settings) => hall::tail(settings, rate),
        Insert::Spring(settings) => spring::tail(settings, rate),
        Insert::Space(settings) => space::tail(settings, rate),
        _ => None,
    }
}

/// How many seconds across a reverb's tail goes, a little past where its slowest part has fallen
/// 60 dB, and how far apart the lines down it are: a second, or ten for a tail over 15 s.
fn tail_span(tail: Tail) -> (f32, f32) {
    let longest = tail.seconds[0].max(tail.seconds[1]);
    let span = tail.pre + longest * 1.15;
    (span, if span > 15.0 { 10.0 } else { 1.0 })
}

/// What a reverb's tail says under its box: how long it takes to fall 60 dB, at its lows and at
/// its highs, and how far apart the lines down it are.
fn tail_caption(tail: Tail) -> String {
    let (_, every) = tail_span(tail);
    let lines = if every > 1.0 {
        "a line every 10 s"
    } else {
        "a line a second"
    };
    format!(
        "falls 60 dB in {:.1} s, highs {:.1} s, {lines}",
        tail.seconds[0], tail.seconds[1]
    )
}

/// How many steps a low-pass gate's level and cutoff are drawn in, from 0 to 1 each, so a gate
/// that has all but stopped moving isn't drawn again: 1/1024 of its cutoff is a hundredth of an
/// octave.
const OPENING_STEPS: f32 = 1024.0;

/// A low-pass gate's level and cutoff, from 0 to `OPENING_STEPS` each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Opening([u16; 2]);

impl Opening {
    fn of((level, cutoff): (f32, f32)) -> Self {
        let step = |value: f32| (value.clamp(0.0, 1.0) * OPENING_STEPS).round() as u16;
        Self([step(level), step(cutoff)])
    }

    fn level_and_cutoff(self) -> (f32, f32) {
        let [level, cutoff] = self.0.map(|step| f32::from(step) / OPENING_STEPS);
        (level, cutoff)
    }
}

/// A note in hundredths of a semitone, as a MIDI note, so it can be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cents(i32);

impl Cents {
    fn of(note: f32) -> Self {
        Self((note * 100.0).round() as i32)
    }

    fn note(self) -> f32 {
        self.0 as f32 / 100.0
    }

    /// Its frequency, `semitones` up, in Hz.
    fn hertz(self, semitones: f32) -> f32 {
        440.0 * ((self.note() + semitones - 69.0) / 12.0).exp2()
    }
}

/// An effect as it's playing with these values, which a step or an LFO can move from what's set.
fn playing(insert: Insert, live: Option<[u8; MAX_PARAMS]>) -> Insert {
    live.map_or(insert, |values| {
        (0..insert.params().len()).fold(insert, |playing, param| {
            playing.with_value(param, values[param])
        })
    })
}

impl SlotDrawing {
    /// The drawing for an effect playing with these values in the FX page's rack, if it has one.
    pub(crate) fn of(insert: Insert, live: Option<[u8; MAX_PARAMS]>) -> Option<Self> {
        match playing(insert, live) {
            Insert::Compressor(settings) => Some(Self::Compressor(Compressor {
                kind: settings.kind,
                threshold: settings.threshold,
                ratio: settings.ratio,
                console_ratio: settings.console_ratio,
                // Glue settles lower the slower its attack is against its release, and its range
                // caps it, so all of them bend its curve.
                glue_ratio: settings.glue_ratio,
                glue_attack: settings.glue_attack,
                glue_release: settings.glue_release,
                range: settings.range,
                ..Compressor::DEFAULT
            })),
            Insert::Eq(settings) => Some(Self::Eq(settings)),
            _ => None,
        }
    }

    /// The drawing for the effect in a slot zoomed in on, if it has one, as it's playing: the
    /// rack's, a distortion's transfer curve for a type that shapes each sample alone, or a filter
    /// effect's curve, at the note the track's slots follow and, for a low-pass gate, as far as
    /// it's open. Before the engine says, it's at middle C, and a gate where it rests.
    pub(crate) fn zoomed(
        insert: Insert,
        live: Option<&LiveValues>,
        slot: usize,
        (rate, millibpm): (u32, u32),
    ) -> Option<Self> {
        let values = live.map(|live| live.inserts[slot]);
        let note = Cents::of(live.map_or(DEFAULT_NOTE, |live| live.note));
        match playing(insert, values) {
            Insert::Distort(settings) => transfer(settings, 0.0).map(|_| Self::Distort(settings)),
            Insert::Lpg(settings) => {
                let opening = live
                    .and_then(|live| live.openings[slot])
                    .unwrap_or_else(|| lpg::resting(settings));
                Some(Self::Lpg(settings, Opening::of(opening)))
            }
            Insert::Reso(settings) => Some(Self::Reso(settings, note)),
            Insert::Comb(settings) => Some(Self::Comb(settings, note)),
            Insert::Vowel(settings) => Some(Self::Vowel(settings, note)),
            Insert::Delay(settings) => Some(Self::Delay(settings, millibpm)),
            Insert::Echo(settings) => Some(Self::Echo(settings, millibpm)),
            playing @ (Insert::Reverb(_)
            | Insert::Plate(_)
            | Insert::Hall(_)
            | Insert::Spring(_)
            | Insert::Space(_)) => tail_of(playing, rate).map(|_| Self::Tail(playing)),
            _ => Self::of(insert, values),
        }
    }

    /// How many characters across it is in the rack.
    pub(crate) fn width(self) -> usize {
        match self {
            Self::Compressor(_) | Self::Distort(_) => DYNAMICS_CURVE_WIDTH,
            Self::Eq(_)
            | Self::Lpg(..)
            | Self::Reso(..)
            | Self::Comb(..)
            | Self::Vowel(..)
            | Self::Delay(..)
            | Self::Echo(..)
            | Self::Tail(_) => EQ_CURVE_WIDTH,
        }
    }

    /// How many characters across and down it is zoomed in on: a square for a curve of a level
    /// in against a level out, and the whole width for one over frequency.
    pub(crate) fn zoomed_size(self) -> (usize, usize) {
        let width = match self {
            Self::Compressor(_) | Self::Distort(_) => ZOOM_CURVE_WIDTH,
            Self::Eq(_)
            | Self::Lpg(..)
            | Self::Reso(..)
            | Self::Comb(..)
            | Self::Vowel(..)
            | Self::Delay(..)
            | Self::Echo(..)
            | Self::Tail(_) => ZOOM_EQ_WIDTH,
        };
        (width, ZOOM_PANEL_HEIGHT)
    }

    /// Its picture, this many pixels across and down.
    pub(crate) fn plot(self, rate: u32, size: (usize, usize)) -> Plot {
        match self {
            Self::Eq(settings) => plot::eq(settings, rate, size),
            Self::Compressor(settings) => plot::compressor(settings, size),
            Self::Distort(settings) => plot::distort(settings, size),
            Self::Lpg(settings, opening) => {
                let (level, cutoff) = opening.level_and_cutoff();
                let filtering = Filtering {
                    db_at: lpg::response(settings, (level, cutoff), rate),
                    // Its cutoff on the track filter's scale, as the plot's is, unless it's a VCA.
                    marked: (settings.mode != GateMode::Vca).then(|| plot::frequency_at(cutoff)),
                    opened: Some(lpg::response(settings, lpg::opened(settings), rate)),
                };
                plot::filtering(filtering, size)
            }
            Self::Reso(settings, note) => {
                let tune = shift_semitones(f32::from(settings.tune));
                plot::filtering(
                    tuned(
                        resonator::response(settings, note.note(), rate),
                        note.hertz(tune),
                    ),
                    size,
                )
            }
            Self::Comb(settings, note) => {
                let tune = shift_semitones(f32::from(settings.tune));
                plot::filtering(
                    tuned(
                        comb::response(settings, note.note(), rate),
                        note.hertz(tune),
                    ),
                    size,
                )
            }
            Self::Delay(settings, millibpm) => {
                let shown = Repeated::of(settings, millibpm, rate);
                plot::repeats(&shown.sounds, shown.beat, shown.span, size)
            }
            Self::Echo(settings, millibpm) => {
                let shown = Repeated::of_echo(settings, millibpm, rate);
                plot::repeats(&shown.sounds, shown.beat, shown.span, size)
            }
            Self::Tail(insert) => match tail_of(insert, rate) {
                Some(tail) => {
                    let (span, every) = tail_span(tail);
                    plot::tail(tail.pre, tail.seconds, span, every, size)
                }
                None => Plot::new(size.0, size.1),
            },
            Self::Vowel(settings, note) => {
                let filtering = Filtering {
                    db_at: vowel::response(settings, note.note(), rate),
                    marked: None,
                    opened: None::<fn(f32) -> f32>,
                };
                plot::filtering(filtering, size)
            }
        }
    }
}

impl SlotDrawing {
    /// What's said under its box zoomed in, if anything: a delay's and a reverb's time scale.
    pub(crate) fn caption(self, rate: u32) -> Option<String> {
        match self {
            Self::Delay(settings, millibpm) => {
                Some(Repeated::of(settings, millibpm, rate).caption())
            }
            Self::Echo(settings, millibpm) => {
                Some(Repeated::of_echo(settings, millibpm, rate).caption())
            }
            Self::Tail(insert) => tail_of(insert, rate).map(tail_caption),
            _ => None,
        }
    }
}

/// A curve tuned to a note, with its dashed line at the note's frequency.
fn tuned<F: Fn(f32) -> f32>(db_at: F, hertz: f32) -> Filtering<F, fn(f32) -> f32> {
    Filtering {
        db_at,
        marked: Some(hertz),
        opened: None,
    }
}
