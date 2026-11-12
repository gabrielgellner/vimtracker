//! Where each of vimtracker's devices puts its settings on the Roto, declared as data: a
//! [`DeviceLayout`] for every kind of device, each pages of eight knobs over eight buttons naming
//! settings by key (`vt-roto`'s [`Layout`]). The mapping in [`super::devices`] reads only these,
//! never the track page's grid, so the screen and the Roto can each be laid out for themselves
//! (issue #419, the design note's §4.3).
//!
//! **The order** is musical, the same rules for every device:
//!
//! *   The settings played most come first, in the order the sound meets them: an envelope's
//!     attack, decay, sustain and release; a filter's cutoff and resonance; a compressor's
//!     threshold and ratio, then its times.
//! *   What comes out ends a page: an effect's `Mix` and `Gain` are always knobs 7 and 8, and
//!     every track's `Level` is knob 1 and `Trim` knob 5.
//! *   A modulator's first page is what it plays: an envelope's times and sustain and its depth,
//!     an LFO's rate, shape, depth and phase. Where it goes, a long list that changes with the
//!     track, is on a page of its own, one arrow away.
//! *   A button is always under a knob: the Roto draws a button's label only under a knob that's
//!     set (firmware 3.3.0), so a restart is under what it starts again, `Reset` under `Attack`
//!     and the arpeggio's `Retrig` under its `Mode`.
//! *   A mod slot's layout is its type's: an ADSR's, an AHD's or an LFO's, swapped when the app changes its
//!     type, which is never chosen on the Roto.
//! *   **The Roto plays and tweaks; the app builds** (the author's rule, 2026-11-11). What a
//!     device is, its machine, its effect or an effect's type or mode, is chosen in the app with
//!     `K`, never on the Roto: each changes what the device is and swaps its whole layout under
//!     the hand. Musical lists stay, such as a ratio's steps, an LFO's shape, Acid's `Wave` and
//!     the filter's type and mode.
//! *   A device keeps to one page unless it has a reason not to: more than eight knobs or eight
//!     buttons, or a modulator's `Dest` on a page of its own. Its off/on settings are on its first
//!     page's buttons, beside its main knobs; a page with few knobs and many buttons, or the
//!     reverse, is fine.
//! *   Off/on settings are on buttons, under a knob of what they belong with. A device with any
//!     is stored on the Roto, the only way it shows buttons (`stored`); a live device has its
//!     buttons on two-step knobs instead, as [`Layout::live_knobs`] places them.
//!
//! **Feel and colour** come from each setting's kind (`devices::family`, `devices::feel`), with
//! an override where a slot needs one, such as a pitch an effect chooses from a list, which is
//! still a pitch's colour.

use vt_model::action::TrackControl;
use vt_model::insert::{CompressorType, DistortType, Insert};
use vt_model::song::{Machine, SynthVoice, Track, TrackKind};
use vt_roto::layout::{Layout, Page, Slot};
use vt_roto::sysex::CONTROLS;

use super::devices::Family;
use vt_model::modulation::ModKind;

use crate::track_page::{AccumulatorField, ArpField, Control, DeviceSpec, ModField};

/// A setting in a layout's slot: its key, and how it differs from what its kind gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Setting {
    /// The setting's key on its device, as [`key`] gives it.
    pub key: &'static str,
    /// Its colour, if it isn't its kind's.
    pub family: Option<Family>,
    /// How its knob turns, if not as its kind's does.
    pub turn: Option<Turn>,
}

/// How a knob turns, where a layout says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Turn {
    /// Smoothly, without clicks, even for a list.
    Smooth,
    /// Smoothly, with a notch at its middle.
    CentreNotch,
}

/// One kind of device's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceLayout {
    /// What this kind of device is known by, the same in every build, such as `fx/delay`: a
    /// stored setup's hash is made from it.
    pub kind: &'static str,
    /// Whether it's stored on the Roto, for its buttons, rather than shown live.
    pub stored: bool,
    pub layout: Layout<Setting>,
}

impl DeviceLayout {
    /// Whether any of its pages has a button.
    pub fn has_buttons(&self) -> bool {
        self.layout
            .pages
            .iter()
            .any(|page| page.buttons.iter().any(|slot| slot.setting().is_some()))
    }
}

/// Nothing here, nor after it on the row.
const __: Slot<Setting> = Slot::Unused;
/// Empty on purpose, to keep a group together.
const GAP: Slot<Setting> = Slot::Gap;
const NO_BUTTONS: [Slot<Setting>; CONTROLS] = [__; CONTROLS];

/// A setting by its key, as its kind has it.
const fn s(key: &'static str) -> Slot<Setting> {
    Slot::Setting(Setting {
        key,
        family: None,
        turn: None,
    })
}

/// A setting in another family's colour than its kind gives.
const fn tinted(key: &'static str, family: Family) -> Slot<Setting> {
    Slot::Setting(Setting {
        key,
        family: Some(family),
        turn: None,
    })
}

const fn page(
    name: &'static str,
    knobs: [Slot<Setting>; CONTROLS],
    buttons: [Slot<Setting>; CONTROLS],
) -> Page<Setting> {
    Page {
        name,
        knobs,
        buttons,
    }
}

/// A page of knobs alone.
const fn knobs(name: &'static str, knobs: [Slot<Setting>; CONTROLS]) -> Page<Setting> {
    page(name, knobs, NO_BUTTONS)
}

/// Buttons with one setting, on button `at`, the rest empty on purpose before it.
const fn button(at: usize, key: &'static str) -> [Slot<Setting>; CONTROLS] {
    let mut buttons = [__; CONTROLS];
    let mut before = 0;
    while before < at {
        buttons[before] = GAP;
        before += 1;
    }
    buttons[at] = s(key);
    buttons
}

const fn live(kind: &'static str, pages: &'static [Page<Setting>]) -> DeviceLayout {
    DeviceLayout {
        kind,
        stored: false,
        layout: Layout { pages },
    }
}

const fn stored(kind: &'static str, pages: &'static [Page<Setting>]) -> DeviceLayout {
    DeviceLayout {
        kind,
        stored: true,
        layout: Layout { pages },
    }
}

// The machines: their sound, on one page, but for `Acid`'s `Drift`, which a 303's eight knobs
// leave no room for. The machine itself is chosen in the app.

const SAMPLER: DeviceLayout = live(
    "machine/sampler",
    &[knobs(
        "Sample",
        [
            s("sample"),
            s("slice"),
            s("play"),
            GAP,
            s("start"),
            s("length"),
            __,
            __,
        ],
    )],
);
const VA: DeviceLayout = live(
    "machine/va",
    &[knobs(
        "VA",
        [
            s("detune"),
            s("square"),
            s("saw"),
            s("drift"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const VA_VCF: DeviceLayout = live(
    "machine/va-vcf",
    &[knobs(
        "VA VCF",
        [
            s("frequency"),
            s("resonance"),
            s("wave"),
            s("drift"),
            __,
            __,
            __,
            __,
        ],
    )],
);
/// A TB-303's knobs in its order, cutoff, resonance, env mod, decay and accent, then slide, the
/// Devil Fish's drive and the wave, with `Drift` on a page of its own.
const ACID: DeviceLayout = live(
    "machine/acid",
    &[
        knobs(
            "Acid",
            [
                s("cutoff"),
                s("resonance"),
                s("envmod"),
                s("decay"),
                s("accent"),
                s("slide"),
                s("drive"),
                s("wave"),
            ],
        ),
        knobs("Drift", [s("drift"), __, __, __, __, __, __, __]),
    ],
);
const FM: DeviceLayout = live(
    "machine/fm",
    &[knobs(
        "FM",
        [s("ratio"), s("index"), s("feedback"), __, __, __, __, __],
    )],
);
const PD: DeviceLayout = live(
    "machine/pd",
    &[knobs(
        "PD",
        [s("ratio"), s("amount"), s("skew"), __, __, __, __, __],
    )],
);
const FOLD: DeviceLayout = live(
    "machine/fold",
    &[knobs(
        "Fold",
        [s("shape"), s("fold"), s("slope"), __, __, __, __, __],
    )],
);
const FORMANT: DeviceLayout = live(
    "machine/formant",
    &[knobs(
        "Formant",
        [s("formant"), s("width"), s("interval"), __, __, __, __, __],
    )],
);
const ADDITIVE: DeviceLayout = live(
    "machine/additive",
    &[knobs(
        "Additive",
        [s("bumps"), s("peak"), s("shape"), __, __, __, __, __],
    )],
);
const WAVES: DeviceLayout = live(
    "machine/waves",
    &[knobs(
        "Waves",
        [s("bank"), s("row"), s("wave"), __, __, __, __, __],
    )],
);
const TERRAIN: DeviceLayout = live(
    "machine/terrain",
    &[knobs(
        "Terrain",
        [s("terrain"), s("size"), s("offset"), __, __, __, __, __],
    )],
);
const SIX_OP: DeviceLayout = live(
    "machine/6-op",
    &[knobs(
        "6-op",
        [
            s("bright"),
            s("time"),
            s("accent"),
            s("patch"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const CHORD: DeviceLayout = live(
    "machine/chord",
    &[knobs(
        "Chord",
        [s("inversion"), s("wave"), s("chord"), __, __, __, __, __],
    )],
);
const CHIP: DeviceLayout = live(
    "machine/chip",
    &[knobs(
        "Chip",
        [
            s("inversion"),
            s("shape"),
            s("chord"),
            s("drift"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const SWARM: DeviceLayout = live(
    "machine/swarm",
    &[knobs(
        "Swarm",
        [s("spread"), s("density"), s("overlap"), __, __, __, __, __],
    )],
);
/// A struck or plucked body: what it's made of and how it sounds, then how hard it's struck.
const STRING: DeviceLayout = live(
    "machine/string",
    &[knobs(
        "String",
        [
            s("material"),
            s("bright"),
            s("decay"),
            s("accent"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const MODAL: DeviceLayout = live(
    "machine/modal",
    &[knobs(
        "Modal",
        [
            s("material"),
            s("bright"),
            s("decay"),
            s("accent"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const DUST: DeviceLayout = live(
    "machine/dust",
    &[knobs(
        "Dust",
        [s("density"), s("spread"), s("filter"), __, __, __, __, __],
    )],
);
const NOISE: DeviceLayout = live(
    "machine/noise",
    &[knobs(
        "Noise",
        [s("type"), s("clock"), s("ring"), __, __, __, __, __],
    )],
);
/// An 808's drums as its knobs are: tone and decay, then the voice's own, then accent.
const KICK: DeviceLayout = live(
    "machine/kick",
    &[knobs(
        "Kick",
        [
            s("tone"),
            s("decay"),
            s("punch"),
            s("accent"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const SNARE: DeviceLayout = live(
    "machine/snare",
    &[knobs(
        "Snare",
        [
            s("tone"),
            s("snappy"),
            s("decay"),
            s("accent"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const HAT: DeviceLayout = live(
    "machine/hat",
    &[knobs(
        "Hat",
        [
            s("tone"),
            s("decay"),
            s("noise"),
            s("accent"),
            __,
            __,
            __,
            __,
        ],
    )],
);
const CSAW: DeviceLayout = live(
    "machine/csaw",
    &[knobs(
        "CSaw",
        [s("notch"), s("depth"), __, __, __, __, __, __],
    )],
);
const MORPH: DeviceLayout = live(
    "machine/morph",
    &[knobs(
        "Morph",
        [s("shape"), s("fuzz"), __, __, __, __, __, __],
    )],
);
const SAW_SQUARE: DeviceLayout = live(
    "machine/sawsquare",
    &[knobs(
        "Saw Sq",
        [s("width"), s("mix"), __, __, __, __, __, __],
    )],
);
const SYNC: DeviceLayout = live(
    "machine/sync",
    &[knobs(
        "Sync",
        [s("interval"), s("mix"), s("wave"), __, __, __, __, __],
    )],
);
const TRIPLE: DeviceLayout = live(
    "machine/triple",
    &[knobs(
        "Triple",
        [s("second"), s("third"), s("wave"), __, __, __, __, __],
    )],
);
const RING: DeviceLayout = live(
    "machine/ring",
    &[knobs(
        "Ring",
        [s("first"), s("second"), __, __, __, __, __, __],
    )],
);
const SAW_COMB: DeviceLayout = live(
    "machine/sawcomb",
    &[knobs(
        "Saw Comb",
        [s("tune"), s("feedback"), __, __, __, __, __, __],
    )],
);
const Z_FILTER: DeviceLayout = live(
    "machine/zfilter",
    &[knobs(
        "Z Filter",
        [s("cutoff"), s("shape"), s("type"), __, __, __, __, __],
    )],
);
const FB_FM: DeviceLayout = live(
    "machine/fbfm",
    &[knobs(
        "FB FM",
        [s("amount"), s("ratio"), __, __, __, __, __, __],
    )],
);
const CHAOS_FM: DeviceLayout = live(
    "machine/chaosfm",
    &[knobs(
        "Chaos FM",
        [s("amount"), s("ratio"), __, __, __, __, __, __],
    )],
);
const TOY: DeviceLayout = live(
    "machine/toy",
    &[knobs(
        "Toy",
        [s("clock"), s("bend"), __, __, __, __, __, __],
    )],
);

// The track's own devices.

/// The filter itself, its type and mode, then the utility filter's cuts.
const FILTER: DeviceLayout = live(
    "filter",
    &[knobs(
        "Filter",
        [
            s("cutoff"),
            s("resonance"),
            s("drive"),
            GAP,
            s("type"),
            s("mode"),
            s("lowcut"),
            s("highcut"),
        ],
    )],
);

/// The fader and pan first, as a mixer strip has them, then the sends, then the gain at its
/// input and the group it plays through. `Level` is knob 1 and `Trim` knob 5 on every track.
const OUTPUT: DeviceLayout = live(
    "output",
    &[knobs(
        "Output",
        [
            s("level"),
            s("pan"),
            s("senda"),
            s("sendb"),
            s("trim"),
            s("group"),
            __,
            __,
        ],
    )],
);
const GROUP_OUTPUT: DeviceLayout = live(
    "output/group",
    &[knobs(
        "Output",
        [
            s("level"),
            s("pan"),
            s("senda"),
            s("sendb"),
            s("trim"),
            __,
            __,
            __,
        ],
    )],
);
const RETURN_OUTPUT: DeviceLayout = live(
    "output/return",
    &[knobs(
        "Output",
        [s("level"), s("pan"), GAP, GAP, s("trim"), __, __, __],
    )],
);
/// The master's, with the song's limiter on the button under `Level`, as it comes after it.
const MASTER_OUTPUT: DeviceLayout = stored(
    "output/master",
    &[page(
        "Output",
        [s("level"), GAP, GAP, GAP, s("trim"), __, __, __],
        button(0, "limiter"),
    )],
);

/// A note's pitch and how it joins and ends, what cuts it off, then the steps' timing, and last
/// how much louder or softer than written its notes play, a live dynamics knob.
const PLAY: DeviceLayout = live(
    "play",
    &[knobs(
        "Play",
        [
            s("pitch"),
            s("glide"),
            s("gate"),
            s("choke"),
            s("speed"),
            s("groove"),
            s("reset"),
            s("velocity"),
        ],
    )],
);

/// Where a modulator goes, on a page of its own: a long list that changes with the track, easily
/// turned by mistake, one arrow away.
const DEST_PAGE: Page<Setting> = knobs("Dest", [s("dest"), __, __, __, __, __, __, __]);

/// An ADSR slot's: attack, decay, sustain and release, then how far it goes and its curve, a
/// musical list that leaves the layout as it is, with its reset on the button under `Attack`, as
/// it starts the envelope again from nothing, and its polarity under `Depth`, lit for `Bi`; where
/// it goes on the next page.
const ADSR: DeviceLayout = stored(
    "adsr",
    &[
        page(
            "ADSR",
            [
                s("attack"),
                s("decay"),
                s("sustain"),
                s("release"),
                s("depth"),
                s("mode"),
                __,
                __,
            ],
            [s("reset"), GAP, GAP, GAP, s("polarity"), __, __, __],
        ),
        DEST_PAGE,
    ],
);

/// An AHD slot's: its attack, hold and decay, a gap, then how far it goes and its curve, as an
/// ADSR's, with the same buttons: its reset under `Attack` and its polarity under `Depth`.
const AHD: DeviceLayout = stored(
    "ahd",
    &[
        page(
            "AHD",
            [
                s("attack"),
                s("hold"),
                s("decay"),
                GAP,
                s("depth"),
                s("mode"),
                __,
                __,
            ],
            [s("reset"), GAP, GAP, GAP, s("polarity"), __, __, __],
        ),
        DEST_PAGE,
    ],
);

/// A drum slot's: its peak, body and decay, then how many times its peak strikes, a list of eight,
/// in the gap an AHD leaves, as it shapes the strike; then how far it goes and its curve, with its
/// polarity under `Depth` as the other envelopes'. It has no reset to put under `Peak`, as it
/// always strikes from full.
const DRUM: DeviceLayout = stored(
    "drum",
    &[
        page(
            "Drum",
            [
                s("peak"),
                s("body"),
                s("decay"),
                s("ratchet"),
                s("depth"),
                s("mode"),
                __,
                __,
            ],
            [GAP, GAP, GAP, GAP, s("polarity"), __, __, __],
        ),
        DEST_PAGE,
    ],
);

/// A low-pass gate slot's: its rise and fall and how far it closes the filter, a gap, then how
/// far it goes, with its reset under `Attack` and its polarity under `Depth`, as the envelopes'.
/// It has no curve.
const LPG_ENVELOPE: DeviceLayout = stored(
    "lpg",
    &[
        page(
            "LPG",
            [
                s("attack"),
                s("decay"),
                s("colour"),
                GAP,
                s("depth"),
                __,
                __,
                __,
            ],
            [s("reset"), GAP, GAP, GAP, s("polarity"), __, __, __],
        ),
        DEST_PAGE,
    ],
);

/// An LFO slot's: its rate and shape, how far it goes, where in its cycle it starts, how its wave
/// bends, how it fades after each note, then its mode, a musical list of five that leaves the
/// layout as it is (#86), with its polarity on the button under `Depth`, lit for `Bi`; where it
/// goes on the next page.
const LFO: DeviceLayout = stored(
    "lfo",
    &[
        page(
            "LFO",
            [
                s("rate"),
                s("shape"),
                s("depth"),
                s("phase"),
                s("deform"),
                s("fade"),
                s("mode"),
                __,
            ],
            [GAP, GAP, s("polarity"), __, __, __, __, __],
        ),
        DEST_PAGE,
    ],
);

/// How far it goes either way, what it does there and when it starts again, then what it moves
/// and by how much.
const ACCUMULATOR: DeviceLayout = live(
    "accumulator",
    &[knobs(
        "Accumulator",
        [
            s("up"),
            s("down"),
            s("atlimit"),
            s("reset"),
            s("moves"),
            s("adds"),
            __,
            __,
        ],
    )],
);

/// Its mode, rate and range, as an arpeggiator's are, with its retrigger on the button under
/// `Mode`, as it starts the pattern again.
const ARPEGGIO: DeviceLayout = stored(
    "arpeggio",
    &[page(
        "Arpeggio",
        [s("mode"), s("rate"), s("octaves"), __, __, __, __, __],
        button(0, "retrig"),
    )],
);

// The effects: their own settings, with `Mix` and `Gain` on knobs 7 and 8. The effect and its type
// or mode are chosen in the app.

const EMPTY: DeviceLayout = live("fx/empty", &[]);

/// A distortion that shapes the wave: how hard, its mode, then what comes out.
const SHAPING: DeviceLayout = live(
    "fx/distort",
    &[knobs(
        "Distort",
        [s("amount"), GAP, GAP, GAP, GAP, GAP, s("mix"), s("gain")],
    )],
);
/// Its bits, with mu-law on the button under them.
const CRUSH: DeviceLayout = stored(
    "fx/distort-crush",
    &[page(
        "Crush",
        [s("bits"), GAP, GAP, GAP, GAP, GAP, s("mix"), s("gain")],
        button(0, "mulaw"),
    )],
);
/// Its rate, with following the note on the button under it.
const DOWN: DeviceLayout = stored(
    "fx/distort-down",
    &[page(
        "Down",
        [s("rate"), GAP, GAP, GAP, GAP, GAP, s("mix"), s("gain")],
        button(0, "track"),
    )],
);
/// A pedal's: its gain and tone knobs, then its mode.
const PEDAL: DeviceLayout = live(
    "fx/distort-pedal",
    &[knobs(
        "Pedal",
        [
            s("amount"),
            s("tone"),
            GAP,
            GAP,
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// The repeats, their stereo and their tone, with ping-pong on the button under `Width`. How its
/// time moves, `Retime` and `Wow`, past eight knobs, is on a second page.
const DELAY: DeviceLayout = stored(
    "fx/delay",
    &[
        page(
            "Delay",
            [
                s("time"),
                s("feedback"),
                s("width"),
                s("drive"),
                s("lowcut"),
                s("highcut"),
                s("mix"),
                s("gain"),
            ],
            button(2, "pingpong"),
        ),
        knobs("Time", [s("retime"), s("wow"), __, __, __, __, __, __]),
    ],
);
const REVERB: DeviceLayout = live(
    "fx/reverb",
    &[knobs(
        "Reverb",
        [
            s("decay"),
            s("pre"),
            s("damp"),
            GAP,
            s("lowcut"),
            s("highcut"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const CHORUS: DeviceLayout = live(
    "fx/chorus",
    &[knobs(
        "Chorus",
        [
            s("rate"),
            s("depth"),
            GAP,
            GAP,
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// Each band's boost on knobs 1 to 3 and its frequency four knobs on, under it, with the middle's
/// width between.
const EQ: DeviceLayout = live(
    "fx/eq3",
    &[knobs(
        "EQ3",
        [
            s("low"),
            s("mid"),
            s("high"),
            s("midq"),
            s("lowfreq"),
            s("midfreq"),
            s("highfreq"),
            s("gain"),
        ],
    )],
);

/// A compressor's threshold and ratio, its times, its makeup with `Auto` on the button under it,
/// then its mix and what it hears.
const CLEAN: DeviceLayout = stored(
    "fx/comp-clean",
    &[page(
        "Comp",
        [
            s("threshold"),
            s("ratio"),
            s("attack"),
            s("release"),
            s("makeup"),
            GAP,
            s("mix"),
            s("source"),
        ],
        button(4, "auto"),
    )],
);
const CONSOLE: DeviceLayout = stored(
    "fx/comp-console",
    &[page(
        "Console",
        [
            s("threshold"),
            s("consoleratio"),
            s("consoleattack"),
            s("consolerelease"),
            s("makeup"),
            GAP,
            s("mix"),
            s("source"),
        ],
        button(4, "auto"),
    )],
);
/// Glue's as Clean's, with its range beside the makeup and `Peak Clip` on the button under `Mix`,
/// as it clips what comes out.
const GLUE: DeviceLayout = stored(
    "fx/comp-glue",
    &[page(
        "Glue",
        [
            s("threshold"),
            s("glueratio"),
            s("glueattack"),
            s("gluerelease"),
            s("makeup"),
            s("range"),
            s("mix"),
            s("source"),
        ],
        [GAP, GAP, GAP, GAP, s("auto"), GAP, s("peakclip"), __],
    )],
);
const LIMIT: DeviceLayout = live(
    "fx/limit",
    &[knobs(
        "Limit",
        [
            s("input"),
            s("ceiling"),
            s("release"),
            GAP,
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
const PHASER: DeviceLayout = live(
    "fx/phaser",
    &[knobs(
        "Phaser",
        [
            s("rate"),
            s("depth"),
            s("feedback"),
            s("centre"),
            s("stages"),
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
const FLANGER: DeviceLayout = live(
    "fx/flanger",
    &[knobs(
        "Flanger",
        [
            s("rate"),
            s("depth"),
            s("feedback"),
            s("delay"),
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// How hard the tape is driven and how it colours, then how it wobbles.
const TAPE: DeviceLayout = live(
    "fx/tape",
    &[knobs(
        "Tape",
        [
            s("drive"),
            s("bias"),
            s("tone"),
            s("bump"),
            s("wow"),
            s("flutter"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const PLATE: DeviceLayout = live(
    "fx/plate",
    &[knobs(
        "Plate",
        [
            s("decay"),
            s("size"),
            s("pre"),
            s("damp"),
            s("highcut"),
            s("mod"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const PITCH: DeviceLayout = live(
    "fx/pitch",
    &[knobs(
        "Pitch",
        [
            tinted("pitch", Family::Time),
            s("fine"),
            s("size"),
            s("feedback"),
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// Where and how long its grains are and how often, with `Freeze` on the button under `Time`,
/// as it holds what they read; their pitch and spread; and their shape on a page of its own.
const GRAINS: DeviceLayout = stored(
    "fx/grains",
    &[
        page(
            "Grains",
            [
                s("time"),
                s("size"),
                s("density"),
                tinted("pitch", Family::Time),
                s("spread"),
                s("random"),
                s("mix"),
                s("gain"),
            ],
            button(0, "freeze"),
        ),
        knobs("Shape", [s("shape"), __, __, __, __, __, __, __]),
    ],
);
const WARPS: DeviceLayout = live(
    "fx/warps",
    &[knobs(
        "Warps",
        [
            s("algo"),
            s("timbre"),
            s("level"),
            s("shape"),
            tinted("pitch", Family::Time),
            s("source"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const LPG: DeviceLayout = live(
    "fx/lpg",
    &[knobs(
        "LPG",
        [
            s("amount"),
            s("response"),
            s("base"),
            s("res"),
            GAP,
            s("source"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const RESO: DeviceLayout = live(
    "fx/reso",
    &[knobs(
        "Reso",
        [
            tinted("tune", Family::Time),
            s("decay"),
            s("material"),
            s("bright"),
            s("position"),
            s("width"),
            s("mix"),
            s("gain"),
        ],
    )],
);
const COMB: DeviceLayout = live(
    "fx/comb",
    &[knobs(
        "Comb",
        [
            tinted("tune", Family::Time),
            s("feedback"),
            s("spread"),
            s("tone"),
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
const VOWEL: DeviceLayout = live(
    "fx/vowel",
    &[knobs(
        "Vowel",
        [
            s("vowel"),
            s("res"),
            s("size"),
            s("track"),
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// The shift and how it's spread and follows the note, then the loop's delay and feedback, as the
/// page has its columns.
const FREQ_SHIFT: DeviceLayout = live(
    "fx/freqshift",
    &[knobs(
        "Freq Shift",
        [
            s("shift"),
            s("spread"),
            s("track"),
            GAP,
            s("time"),
            s("feedback"),
            s("mix"),
            s("gain"),
        ],
    )],
);
/// How long it rings and how big and dark it is, then its chirp, reflections and wander, with the
/// knock under the wander, as both shake the springs.
const SPRING: DeviceLayout = stored(
    "fx/spring",
    &[page(
        "Spring",
        [
            s("decay"),
            s("size"),
            s("damp"),
            s("spin"),
            s("reflect"),
            s("chaos"),
            s("mix"),
            s("gain"),
        ],
        button(5, "knock"),
    )],
);
/// How long the rotors take to change, the horn's Doppler and swell, the drive and the width,
/// with the fast and slow switch on the button under the inertia, as an organist's half-moon.
const ROTARY: DeviceLayout = stored(
    "fx/rotary",
    &[page(
        "Rotary",
        [
            s("inertia"),
            s("doppler"),
            s("tremolo"),
            s("drive"),
            s("width"),
            GAP,
            s("mix"),
            s("gain"),
        ],
        button(0, "fast"),
    )],
);

/// Every kind of device's layout, for the tests and the stored setups.
pub const ALL: [&DeviceLayout; 73] = [
    &SAMPLER,
    &VA,
    &VA_VCF,
    &ACID,
    &FM,
    &PD,
    &FOLD,
    &FORMANT,
    &ADDITIVE,
    &WAVES,
    &TERRAIN,
    &SIX_OP,
    &CHORD,
    &CHIP,
    &SWARM,
    &STRING,
    &MODAL,
    &DUST,
    &NOISE,
    &KICK,
    &SNARE,
    &HAT,
    &CSAW,
    &MORPH,
    &SAW_SQUARE,
    &SYNC,
    &TRIPLE,
    &RING,
    &SAW_COMB,
    &TOY,
    &Z_FILTER,
    &FB_FM,
    &CHAOS_FM,
    &FILTER,
    &OUTPUT,
    &GROUP_OUTPUT,
    &RETURN_OUTPUT,
    &MASTER_OUTPUT,
    &PLAY,
    &ADSR,
    &AHD,
    &DRUM,
    &LPG_ENVELOPE,
    &LFO,
    &ACCUMULATOR,
    &ARPEGGIO,
    &EMPTY,
    &SHAPING,
    &CRUSH,
    &DOWN,
    &PEDAL,
    &DELAY,
    &REVERB,
    &CHORUS,
    &EQ,
    &CLEAN,
    &CONSOLE,
    &GLUE,
    &LIMIT,
    &PHASER,
    &FLANGER,
    &TAPE,
    &PLATE,
    &PITCH,
    &GRAINS,
    &WARPS,
    &LPG,
    &RESO,
    &COMB,
    &VOWEL,
    &FREQ_SHIFT,
    &SPRING,
    &ROTARY,
];

/// A machine's layout.
pub fn machine_layout(machine: Machine) -> &'static DeviceLayout {
    match machine {
        Machine::Sampler => &SAMPLER,
        Machine::Plaits(voice) => match voice {
            SynthVoice::VirtualAnalog => &VA,
            SynthVoice::VirtualAnalogVcf => &VA_VCF,
            SynthVoice::Acid => &ACID,
            SynthVoice::Fm => &FM,
            SynthVoice::PhaseDistortion => &PD,
            SynthVoice::Waveshaping => &FOLD,
            SynthVoice::Formant => &FORMANT,
            SynthVoice::Additive => &ADDITIVE,
            SynthVoice::Wavetable => &WAVES,
            SynthVoice::WaveTerrain => &TERRAIN,
            SynthVoice::SixOp => &SIX_OP,
            SynthVoice::Chord => &CHORD,
            SynthVoice::Chiptune => &CHIP,
            SynthVoice::Swarm => &SWARM,
            SynthVoice::String => &STRING,
            SynthVoice::Modal => &MODAL,
            SynthVoice::Dust => &DUST,
            SynthVoice::Noise => &NOISE,
            SynthVoice::Kick => &KICK,
            SynthVoice::Snare => &SNARE,
            SynthVoice::Hat => &HAT,
            SynthVoice::CSaw => &CSAW,
            SynthVoice::Morph => &MORPH,
            SynthVoice::SawSquare => &SAW_SQUARE,
            SynthVoice::HardSync => &SYNC,
            SynthVoice::Triple => &TRIPLE,
            SynthVoice::Ring => &RING,
            SynthVoice::SawComb => &SAW_COMB,
            SynthVoice::Toy => &TOY,
            SynthVoice::ZFilter => &Z_FILTER,
            SynthVoice::FeedbackFm => &FB_FM,
            SynthVoice::ChaoticFm => &CHAOS_FM,
        },
    }
}

/// An effect's layout, in the type or mode it's in, as its settings change with it.
pub fn effect_layout(insert: Insert) -> &'static DeviceLayout {
    match insert {
        Insert::Empty => &EMPTY,
        Insert::Distort(distort) => match distort.kind {
            DistortType::Crush => &CRUSH,
            DistortType::Down => &DOWN,
            DistortType::Overdrive | DistortType::Distortion => &PEDAL,
            DistortType::Soft | DistortType::Hard | DistortType::Fold | DistortType::Sine => {
                &SHAPING
            }
        },
        Insert::Delay(_) => &DELAY,
        Insert::Reverb(_) => &REVERB,
        Insert::Chorus(_) => &CHORUS,
        Insert::Eq(_) => &EQ,
        Insert::Compressor(compressor) => match compressor.kind {
            CompressorType::Clean => &CLEAN,
            CompressorType::Console => &CONSOLE,
            CompressorType::Glue => &GLUE,
        },
        Insert::Limit(_) => &LIMIT,
        Insert::Phaser(_) => &PHASER,
        Insert::Flanger(_) => &FLANGER,
        Insert::Tape(_) => &TAPE,
        Insert::Plate(_) => &PLATE,
        Insert::Pitch(_) => &PITCH,
        Insert::Grains(_) => &GRAINS,
        Insert::Warps(_) => &WARPS,
        Insert::Lpg(_) => &LPG,
        Insert::Reso(_) => &RESO,
        Insert::Comb(_) => &COMB,
        Insert::Vowel(_) => &VOWEL,
        Insert::FreqShift(_) => &FREQ_SHIFT,
        Insert::Spring(_) => &SPRING,
        Insert::Rotary(_) => &ROTARY,
    }
}

/// A device's layout, by what it holds: its machine, its effect, or which of the track's own
/// devices it is.
pub fn layout_of(spec: &DeviceSpec, track: &Track) -> &'static DeviceLayout {
    let first = spec.rows().flatten().flatten().next();
    match first {
        Some(Control::Machine) => machine_layout(track.machine),
        Some(Control::Insert(slot)) => effect_layout(track.inserts[slot]),
        Some(Control::FilterType) => &FILTER,
        Some(Control::Base(TrackControl::Pitch)) => &PLAY,
        Some(Control::Mod(slot, _)) => match track.mods[slot].kind {
            ModKind::Adsr => &ADSR,
            ModKind::Ahd => &AHD,
            ModKind::Drum => &DRUM,
            ModKind::Lpg => &LPG_ENVELOPE,
            ModKind::Lfo => &LFO,
        },
        Some(Control::Accumulator(_)) => &ACCUMULATOR,
        Some(Control::Arpeggio(_)) => &ARPEGGIO,
        _ => match track.kind {
            TrackKind::Sound => &OUTPUT,
            TrackKind::Group(_) => &GROUP_OUTPUT,
            TrackKind::ReturnA | TrackKind::ReturnB => &RETURN_OUTPUT,
            TrackKind::Master => &MASTER_OUTPUT,
        },
    }
}

/// Whether a control is the app's alone, never on the Roto: what a device is, its machine, its
/// effect, the effect's type or mode, or a mod slot's type, as each swaps the device's whole
/// layout.
pub fn app_only(control: Control, track: &Track) -> bool {
    match control {
        Control::Machine | Control::Insert(_) | Control::Mod(_, ModField::Type) => true,
        Control::InsertParam(slot, param) => track.inserts[slot].type_param() == Some(param),
        _ => false,
    }
}

/// The control of a device a layout's setting names, if the device has it.
pub fn resolve(setting: &Setting, spec: &DeviceSpec, track: &Track) -> Option<Control> {
    spec.rows()
        .flatten()
        .flatten()
        .find(|control| key(*control, track) == setting.key)
}

/// The key a layout names a control by on this track: a machine's setting and an effect's by the
/// key songs store them under, and the track's own settings by a name of their own.
pub fn key(control: Control, track: &Track) -> &'static str {
    match control {
        Control::Machine => "machine",
        Control::Sample => "sample",
        Control::Setting(_) => control
            .setting_on(track)
            .map_or("setting", |id| id.entry().key),
        Control::InsertParam(slot, param) => track.inserts[slot]
            .params()
            .get(param)
            .map_or("setting", |found| found.key),
        Control::Insert(_) => "effect",
        Control::Mute => "mute",
        Control::Solo => "solo",
        Control::Base(which) => match which {
            TrackControl::Cutoff => "cutoff",
            TrackControl::Resonance => "resonance",
            TrackControl::Drive => "drive",
            TrackControl::LowCut => "lowcut",
            TrackControl::HighCut => "highcut",
            TrackControl::Pan => "pan",
            TrackControl::SendA => "senda",
            TrackControl::SendB => "sendb",
            TrackControl::Pitch => "pitch",
            TrackControl::VelocityDelta => "velocity",
        },
        Control::Level => "level",
        Control::Trim => "trim",
        Control::Groove => "groove",
        Control::Choke => "choke",
        Control::Group => "group",
        Control::GateLength => "gate",
        Control::Speed => "speed",
        Control::Reset => "reset",
        Control::Glide => "glide",
        Control::Limiter => "limiter",
        Control::FilterType => "type",
        Control::FilterMode => "mode",
        Control::Mod(_, field) => match field {
            ModField::Type => "type",
            ModField::Mode => "mode",
            ModField::Polarity => "polarity",
            ModField::Destination => "dest",
            ModField::Depth => "depth",
            ModField::Attack => "attack",
            ModField::Decay => "decay",
            ModField::Sustain => "sustain",
            ModField::Release => "release",
            ModField::Reset => "reset",
            ModField::Hold => "hold",
            ModField::Peak => "peak",
            ModField::Body => "body",
            ModField::Ratchet => "ratchet",
            ModField::Colour => "colour",
            ModField::Shape => "shape",
            ModField::Rate => "rate",
            ModField::Phase => "phase",
            ModField::Deform => "deform",
            ModField::Fade => "fade",
        },
        Control::Accumulator(field) => match field {
            AccumulatorField::Up => "up",
            AccumulatorField::Down => "down",
            AccumulatorField::Moves => "moves",
            AccumulatorField::AtLimit => "atlimit",
            AccumulatorField::Adds => "adds",
            AccumulatorField::Reset => "reset",
        },
        Control::Arpeggio(field) => match field {
            ArpField::Rate => "rate",
            ArpField::Mode => "mode",
            ArpField::Octaves => "octaves",
            ArpField::Retrig => "retrig",
        },
    }
}
