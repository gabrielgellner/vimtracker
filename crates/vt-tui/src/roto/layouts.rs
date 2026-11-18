//! Where each of vimtracker's devices puts its settings on the Roto, declared as data: a
//! [`DeviceLayout`] for every kind of device, each pages of eight knobs over eight buttons naming
//! settings by key (`vt-roto`'s [`Layout`]). The mapping in [`super::devices`] reads only these,
//! never the track page's grid, so the screen and the Roto can each be laid out for themselves
//! (the design note's §4.3).
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

use std::sync::LazyLock;

use vt_model::action::TrackControl;
use vt_model::generator::Generator;
use vt_model::insert::{CompressorType, DistortType, EffectKind, Insert};
use vt_model::song::{Machine, SynthVoice, Track, TrackKind};
use vt_roto::layout::{Layout, Page, Slot};
use vt_roto::sysex::CONTROLS;

use super::devices::Family;
use vt_model::modulation::{ModField, ModKind};
use vt_model::settings::{FOUR_OP_EACH, FOUR_OP_SHARED};
use vt_model::shape::{FillKind, GenParam, Where};

use crate::controls::{Control, DeviceSpec, STEP_KEYS, key};

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
            s("reverse"),
            s("start"),
            s("length"),
            s("tune"),
            s("cents"),
        ],
    )],
);
const VA: DeviceLayout = live(
    "machine/va",
    &[knobs(
        SynthVoice::VirtualAnalog.name(),
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
        SynthVoice::VirtualAnalogVcf.name(),
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
            SynthVoice::Acid.name(),
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
/// The stack's spread and blend first, then how many saws, how they're laid out and shaped and
/// where they start, with `Drift` on a page of its own, as `Acid`'s is.
const SUPERSAW: DeviceLayout = live(
    "machine/supersaw",
    &[
        knobs(
            SynthVoice::Supersaw.name(),
            [
                s("detune"),
                s("mix"),
                s("saws"),
                s("warp"),
                s("shape"),
                s("width"),
                s("stack"),
                s("phase"),
            ],
        ),
        knobs("Drift", [s("drift"), __, __, __, __, __, __, __]),
    ],
);
const FM: DeviceLayout = live(
    "machine/fm",
    &[knobs(
        SynthVoice::Fm.name(),
        [s("ratio"), s("index"), s("feedback"), __, __, __, __, __],
    )],
);
const PD: DeviceLayout = live(
    "machine/pd",
    &[knobs(
        SynthVoice::PhaseDistortion.name(),
        [s("ratio"), s("amount"), s("skew"), __, __, __, __, __],
    )],
);
const FOLD: DeviceLayout = live(
    "machine/fold",
    &[knobs(
        SynthVoice::Waveshaping.name(),
        [s("shape"), s("fold"), s("slope"), __, __, __, __, __],
    )],
);
const FORMANT: DeviceLayout = live(
    "machine/formant",
    &[knobs(
        SynthVoice::Formant.name(),
        [s("formant"), s("width"), s("interval"), __, __, __, __, __],
    )],
);
const ADDITIVE: DeviceLayout = live(
    "machine/additive",
    &[knobs(
        SynthVoice::Additive.name(),
        [s("bumps"), s("peak"), s("shape"), __, __, __, __, __],
    )],
);
const WAVES: DeviceLayout = live(
    "machine/waves",
    &[knobs(
        SynthVoice::Wavetable.name(),
        [s("bank"), s("row"), s("wave"), __, __, __, __, __],
    )],
);
const TERRAIN: DeviceLayout = live(
    "machine/terrain",
    &[knobs(
        SynthVoice::WaveTerrain.name(),
        [s("terrain"), s("size"), s("offset"), __, __, __, __, __],
    )],
);
/// What the operators share, the Machine device's own; each operator's are on its own device's.
const FOUR_OP: DeviceLayout = live(
    "machine/4op",
    &[knobs(
        SynthVoice::FourOp.name(),
        [
            s("algorithm"),
            s("feedback"),
            s("ring"),
            s("beat"),
            __,
            __,
            __,
            __,
        ],
    )],
);
/// One of 4-op's operators' devices, `$n` its number: its pitch, level and sound, then its
/// envelope in its order, its sustain and release on a second page.
macro_rules! operator {
    ($n:literal) => {
        live(
            concat!("machine/4op/op", $n),
            &[
                knobs(
                    concat!("Op ", $n),
                    [
                        s(concat!("ratio", $n)),
                        s(concat!("fine", $n)),
                        s(concat!("level", $n)),
                        s(concat!("wave", $n)),
                        s(concat!("warp", $n)),
                        s(concat!("amount", $n)),
                        s(concat!("attack", $n)),
                        s(concat!("decay", $n)),
                    ],
                ),
                knobs(
                    concat!("Env ", $n),
                    [
                        s(concat!("sustain", $n)),
                        s(concat!("release", $n)),
                        __,
                        __,
                        __,
                        __,
                        __,
                        __,
                    ],
                ),
            ],
        )
    };
}
/// 4-op's operators' devices, `Op 1` to `Op 4`.
const FOUR_OP_OPERATORS: [DeviceLayout; 4] = [
    operator!("1"),
    operator!("2"),
    operator!("3"),
    operator!("4"),
];
const SIX_OP: DeviceLayout = live(
    "machine/6-op",
    &[knobs(
        SynthVoice::SixOp.name(),
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
        SynthVoice::Chord.name(),
        [s("inversion"), s("wave"), s("chord"), __, __, __, __, __],
    )],
);
const CHIP: DeviceLayout = live(
    "machine/chip",
    &[knobs(
        SynthVoice::Chiptune.name(),
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
        SynthVoice::Swarm.name(),
        [s("spread"), s("density"), s("overlap"), __, __, __, __, __],
    )],
);
/// A struck or plucked body: what it's made of and how it sounds, then how hard it's struck.
const STRING: DeviceLayout = live(
    "machine/string",
    &[knobs(
        SynthVoice::String.name(),
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
        SynthVoice::Modal.name(),
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
        SynthVoice::Dust.name(),
        [s("density"), s("spread"), s("filter"), __, __, __, __, __],
    )],
);
const NOISE: DeviceLayout = live(
    "machine/noise",
    &[knobs(
        SynthVoice::Noise.name(),
        [s("type"), s("clock"), s("ring"), __, __, __, __, __],
    )],
);
/// An 808's drums as its knobs are: tone and decay, then the voice's own, then accent.
const KICK: DeviceLayout = live(
    "machine/kick",
    &[knobs(
        SynthVoice::Kick.name(),
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
        SynthVoice::Snare.name(),
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
        SynthVoice::Hat.name(),
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
        SynthVoice::CSaw.name(),
        [s("notch"), s("depth"), __, __, __, __, __, __],
    )],
);
const MORPH: DeviceLayout = live(
    "machine/morph",
    &[knobs(
        SynthVoice::Morph.name(),
        [s("shape"), s("fuzz"), __, __, __, __, __, __],
    )],
);
const SAW_SQUARE: DeviceLayout = live(
    "machine/sawsquare",
    &[knobs(
        SynthVoice::SawSquare.name(),
        [s("width"), s("mix"), __, __, __, __, __, __],
    )],
);
const SYNC: DeviceLayout = live(
    "machine/sync",
    &[knobs(
        SynthVoice::HardSync.name(),
        [s("interval"), s("mix"), s("wave"), __, __, __, __, __],
    )],
);
const TRIPLE: DeviceLayout = live(
    "machine/triple",
    &[knobs(
        SynthVoice::Triple.name(),
        [s("second"), s("third"), s("wave"), __, __, __, __, __],
    )],
);
const RING: DeviceLayout = live(
    "machine/ring",
    &[knobs(
        SynthVoice::Ring.name(),
        [s("first"), s("second"), __, __, __, __, __, __],
    )],
);
const SAW_COMB: DeviceLayout = live(
    "machine/sawcomb",
    &[knobs(
        SynthVoice::SawComb.name(),
        [s("tune"), s("feedback"), __, __, __, __, __, __],
    )],
);
const Z_FILTER: DeviceLayout = live(
    "machine/zfilter",
    &[knobs(
        SynthVoice::ZFilter.name(),
        [s("cutoff"), s("shape"), s("type"), __, __, __, __, __],
    )],
);
const FB_FM: DeviceLayout = live(
    "machine/fbfm",
    &[knobs(
        SynthVoice::FeedbackFm.name(),
        [s("amount"), s("ratio"), __, __, __, __, __, __],
    )],
);
const CHAOS_FM: DeviceLayout = live(
    "machine/chaosfm",
    &[knobs(
        SynthVoice::ChaoticFm.name(),
        [s("amount"), s("ratio"), __, __, __, __, __, __],
    )],
);
const VOSIM: DeviceLayout = live(
    "machine/vosim",
    &[knobs(
        SynthVoice::Vosim.name(),
        [s("formant"), s("second"), __, __, __, __, __, __],
    )],
);
const FOF: DeviceLayout = live(
    "machine/fof",
    &[knobs(
        SynthVoice::Fof.name(),
        [s("vowel"), s("voice"), __, __, __, __, __, __],
    )],
);
const BOWED: DeviceLayout = live(
    "machine/bowed",
    &[knobs(
        SynthVoice::Bowed.name(),
        [s("pressure"), s("position"), __, __, __, __, __, __],
    )],
);
const BLOWN: DeviceLayout = live(
    "machine/blown",
    &[knobs(
        SynthVoice::Blown.name(),
        [s("breath"), s("tone"), __, __, __, __, __, __],
    )],
);
const FLUTED: DeviceLayout = live(
    "machine/fluted",
    &[knobs(
        SynthVoice::Fluted.name(),
        [s("breath"), s("jet"), __, __, __, __, __, __],
    )],
);
const STRUCK_BELL: DeviceLayout = live(
    "machine/bell",
    &[knobs(
        SynthVoice::Bell.name(),
        [s("decay"), s("detune"), __, __, __, __, __, __],
    )],
);
const STRUCK_DRUM: DeviceLayout = live(
    "machine/drum",
    &[knobs(
        SynthVoice::Drum.name(),
        [s("decay"), s("tone"), __, __, __, __, __, __],
    )],
);
const WAVE_MAP: DeviceLayout = live(
    "machine/wavemap",
    &[knobs(
        SynthVoice::WaveMap.name(),
        [s("across"), s("down"), __, __, __, __, __, __],
    )],
);
const WAVE_LINE: DeviceLayout = live(
    "machine/waveline",
    &[knobs(
        SynthVoice::WaveLine.name(),
        [s("scan"), s("steps"), __, __, __, __, __, __],
    )],
);
const PARAPHONIC: DeviceLayout = live(
    "machine/paraphonic",
    &[knobs(
        SynthVoice::Paraphonic.name(),
        [s("wave"), s("chord"), __, __, __, __, __, __],
    )],
);
const TWIN_PEAKS: DeviceLayout = live(
    "machine/twinpeaks",
    &[knobs(
        SynthVoice::TwinPeaks.name(),
        [s("sharpness"), s("spacing"), __, __, __, __, __, __],
    )],
);
const CLOCKED: DeviceLayout = live(
    "machine/clocked",
    &[knobs(
        SynthVoice::Clocked.name(),
        [s("loop"), s("steps"), __, __, __, __, __, __],
    )],
);
const GRAIN_CLOUD: DeviceLayout = live(
    "machine/graincloud",
    &[knobs(
        SynthVoice::GrainCloud.name(),
        [s("length"), s("spread"), __, __, __, __, __, __],
    )],
);
const MODEM: DeviceLayout = live(
    "machine/modem",
    &[knobs(
        SynthVoice::Modem.name(),
        [s("baud"), s("data"), __, __, __, __, __, __],
    )],
);
/// Plaits' speech, its prosody and speed after its three knobs.
const SPEECH: DeviceLayout = live(
    "machine/speech",
    &[knobs(
        SynthVoice::Speech.name(),
        [
            s("model"),
            s("species"),
            s("word"),
            s("prosody"),
            s("speed"),
            __,
            __,
            __,
        ],
    )],
);
const TOY: DeviceLayout = live(
    "machine/toy",
    &[knobs(
        SynthVoice::Toy.name(),
        [s("clock"), s("bend"), __, __, __, __, __, __],
    )],
);

// The track's own devices.

/// The filter itself and its punch, its type and mode, then the utility filter's cuts, with
/// `Key track` on the button under `Cutoff`, as it moves it, and `Safe` under `Res`, as it tames
/// it.
const FILTER: DeviceLayout = stored(
    "filter",
    &[page(
        "Filter",
        [
            s("cutoff"),
            s("resonance"),
            s("drive"),
            s("punch"),
            s("type"),
            s("mode"),
            s("lowcut"),
            s("highcut"),
        ],
        [s("keytrack"), s("safe"), __, __, __, __, __, __],
    )],
);

/// The fader and pan first, as a mixer strip has them, then the sends, then the gain at its
/// input, the group it plays through and what it's for. `Level` is knob 1 and `Trim` knob 5 on
/// every track.
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
            s("role"),
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
/// how much louder or softer than written its notes play, a live dynamics knob. How near the grid
/// its rows' nudges play, `Quantise`, past eight knobs, is on a second page, a live timing knob,
/// with how its notes join, `Mode`, which of them glide, `Glide mode`, how often its steps
/// jump to any row, `Jump`, how far its rows' chances to play are moved, `Chance`, and whether
/// they decide rows or pulses, `Target`, how late every other step lands, `Swing`, a live timing
/// knob (decision 420), and its dice loop on a page of its own, `Dice`.
const PLAY: DeviceLayout = live(
    "play",
    &[
        PLAY_PAGE,
        knobs(
            "Timing",
            [
                s("quantise"),
                s("voicemode"),
                s("glidemode"),
                s("jump"),
                s("chance"),
                s("chancetarget"),
                s("swing"),
                __,
            ],
        ),
        DICE_PAGE,
    ],
);

/// The Play device's third page, the track's dice loop: how many steps' dice repeat, how often
/// they roll again and how often a step takes another's (decisions 410 and 411).
const DICE_PAGE: Page<Setting> = knobs(
    "Dice",
    [
        s("diceloop"),
        s("dicemutate"),
        s("dicejump"),
        __,
        __,
        __,
        __,
        __,
    ],
);

/// The Play device's first page, a synth's and a sampler's.
const PLAY_PAGE: Page<Setting> = knobs(
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
);

/// A synth's: the sampler's, with whether its oscillators start again on each note between
/// `Quantise` and `Mode`, and `Glide mode`, `Jump`, `Chance`, `Target` and `Swing` after,
/// and the dice loop's page.
const SYNTH_PLAY: DeviceLayout = live(
    "synthplay",
    &[
        PLAY_PAGE,
        knobs(
            "Timing",
            [
                s("quantise"),
                s("phase"),
                s("voicemode"),
                s("glidemode"),
                s("jump"),
                s("chance"),
                s("chancetarget"),
                s("swing"),
            ],
        ),
        DICE_PAGE,
    ],
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
            ModKind::Adsr.name(),
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
            ModKind::Ahd.name(),
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
            ModKind::Drum.name(),
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
            ModKind::Lpg.name(),
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
/// layout as it is, with its polarity on the button under `Depth`, lit for `Bi`; where it goes on
/// the next page.
const LFO: DeviceLayout = stored(
    "lfo",
    &[
        page(
            ModKind::Lfo.name(),
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

/// A Steps slot's: how fast, how many and how far, then where each note starts it, what it does at
/// its end and its mode, musical lists that leave the layout as it is, then how it drifts and how
/// it goes from step to step; then its values, eight to a page, each with a centre notch, a small
/// step sequencer under the hand; then how often it jumps and how `Mutate`'s draws spread and
/// lean, with the step its loop goes round from under `Loop`'s knob, a page of its own as Steps'
/// first is full (decisions 403 and 428); and where it goes last (decided in the study's §12,
/// question 3).
/// It has no off/on settings, so it's live.
const STEPS_LAYOUT: DeviceLayout = live(
    "steps",
    &[
        knobs(
            ModKind::Steps.name(),
            [
                s("rate"),
                s("length"),
                s("depth"),
                s("start"),
                s("loop"),
                s("mode"),
                s("mutate"),
                s("smooth"),
            ],
        ),
        knobs(
            "Steps 1-8",
            [
                s("step1"),
                s("step2"),
                s("step3"),
                s("step4"),
                s("step5"),
                s("step6"),
                s("step7"),
                s("step8"),
            ],
        ),
        knobs(
            "Steps 9-16",
            [
                s("step9"),
                s("step10"),
                s("step11"),
                s("step12"),
                s("step13"),
                s("step14"),
                s("step15"),
                s("step16"),
            ],
        ),
        knobs(
            "Walk",
            [
                s("jump"),
                s("spread"),
                s("bias"),
                GAP,
                s("from"),
                __,
                __,
                __,
            ],
        ),
        DEST_PAGE,
    ],
);

/// A Track slot's: where its map starts and ends and how far it goes, then what it follows, a
/// musical list of three that leaves the layout as it is, with its polarity on the button under
/// `Depth`, lit for `Bi`, as an LFO's; where it goes on the next page. The study had it live, as
/// slots had no polarity then; the button stores it, as it does the LFO.
const TRACK: DeviceLayout = stored(
    "track",
    &[
        page(
            ModKind::Track.name(),
            [s("low"), s("high"), s("depth"), s("mode"), __, __, __, __],
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

/// The track's eight macros, knob 1 to 8 as they're numbered, each named by its macro.
const MACROS: DeviceLayout = live(
    "macros",
    &[knobs(
        "Macros",
        [
            s("macro1"),
            s("macro2"),
            s("macro3"),
            s("macro4"),
            s("macro5"),
            s("macro6"),
            s("macro7"),
            s("macro8"),
        ],
    )],
);

// The effects: their own settings, with `Mix` and `Gain` on knobs 7 and 8. The effect and its type
// or mode are chosen in the app.

const EMPTY: DeviceLayout = live("fx/empty", &[]);

/// A distortion that shapes the wave: how hard, its mode, then what comes out.
const SHAPING: DeviceLayout = live(
    "fx/distort",
    &[knobs(
        EffectKind::Distort.name(),
        [s("amount"), GAP, GAP, GAP, GAP, GAP, s("mix"), s("gain")],
    )],
);
/// Its bits, with mu-law on the button under them.
const CRUSH: DeviceLayout = stored(
    "fx/distort-crush",
    &[page(
        DistortType::Crush.name(),
        [s("bits"), GAP, GAP, GAP, GAP, GAP, s("mix"), s("gain")],
        button(0, "mulaw"),
    )],
);
/// Its rate, with following the note on the button under it.
const DOWN: DeviceLayout = stored(
    "fx/distort-down",
    &[page(
        DistortType::Down.name(),
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
            EffectKind::Delay.name(),
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
        EffectKind::Reverb.name(),
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
        EffectKind::Chorus.name(),
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
        EffectKind::Eq.name(),
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
        EffectKind::Compressor.name(),
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
        EffectKind::Limit.name(),
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
        EffectKind::Phaser.name(),
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
        EffectKind::Flanger.name(),
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
        EffectKind::Tape.name(),
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
        EffectKind::Plate.name(),
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
        EffectKind::Pitch.name(),
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
            EffectKind::Grains.name(),
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
/// The module's knobs in its panel's order, its reverb, the job of its `Blend` most played after
/// the mix, then `Blend` where a mix goes, with `Freeze` on the button under `Position`, as it
/// holds what it reads; then `Blend`'s other jobs and the quality on a page of their own.
const CLOUDS: DeviceLayout = stored(
    "fx/clouds",
    &[
        page(
            EffectKind::Clouds.name(),
            [
                s("position"),
                s("size"),
                tinted("pitch", Family::Time),
                s("density"),
                s("texture"),
                s("reverb"),
                s("blend"),
                s("gain"),
            ],
            button(0, "freeze"),
        ),
        knobs(
            "Blend",
            [s("spread"), s("feedback"), s("quality"), __, __, __, __, __],
        ),
    ],
);
/// The module's four big knobs, its density, its reverb and its dry/wet, with `Freeze` on the
/// button under `Time`, as `Grains` and `Clouds` have it; then what its attenuverters stray, its
/// feedback, the clock it follows and its quality on a page of their own.
const BEADS: DeviceLayout = stored(
    "fx/beads",
    &[
        page(
            EffectKind::Beads.name(),
            [
                s("time"),
                s("size"),
                s("shape"),
                tinted("pitch", Family::Time),
                s("density"),
                s("reverb"),
                s("blend"),
                s("gain"),
            ],
            button(0, "freeze"),
        ),
        knobs(
            "Random",
            [
                s("random"),
                s("feedback"),
                s("clock"),
                s("quality"),
                __,
                __,
                __,
                __,
            ],
        ),
    ],
);
/// The RE-201's front panel in its order: the tape's speed and the loop's feedback, the echo's
/// and the reverb's levels, the tone, then what comes out; then the tape's other workings on a
/// page of their own. Its mode chooses what it is, so it's chosen in the app, as a type is.
const ECHO: DeviceLayout = live(
    "fx/echo",
    &[
        knobs(
            EffectKind::Echo.name(),
            [
                s("rate"),
                s("intensity"),
                s("echo"),
                s("reverb"),
                s("bass"),
                s("treble"),
                s("mix"),
                s("gain"),
            ],
        ),
        knobs(
            "Tape",
            [
                s("sync"),
                s("inertia"),
                s("input"),
                s("wow"),
                __,
                __,
                __,
                __,
            ],
        ),
    ],
);
const WARPS: DeviceLayout = live(
    "fx/warps",
    &[knobs(
        EffectKind::Warps.name(),
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
        EffectKind::Lpg.name(),
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
        EffectKind::Reso.name(),
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
        EffectKind::Comb.name(),
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
        EffectKind::Vowel.name(),
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
        EffectKind::FreqShift.name(),
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
        EffectKind::Spring.name(),
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
/// How long a piece it repeats, how many times and how it fades, with the trigger on the button
/// under the length, so a press repeats what's playing.
const STUTTER: DeviceLayout = stored(
    "fx/stutter",
    &[page(
        EffectKind::Stutter.name(),
        [
            s("length"),
            s("repeats"),
            s("decay"),
            GAP,
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
        button(0, "trig"),
    )],
);
/// How hard it drives and where the highs start, then how its detector rises and falls.
const EXCITER: DeviceLayout = live(
    "fx/exciter",
    &[knobs(
        EffectKind::Exciter.name(),
        [
            s("drive"),
            s("tone"),
            GAP,
            s("attack"),
            s("release"),
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);
/// How long the rotors take to change, the horn's Doppler and swell, the drive and the width,
/// with the fast and slow switch on the button under the inertia, as an organist's half-moon.
const ROTARY: DeviceLayout = stored(
    "fx/rotary",
    &[page(
        EffectKind::Rotary.name(),
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
/// How long it rings and how big and dark it is, then its wobble, as the reverbs have theirs.
const SPACE: DeviceLayout = live(
    "fx/space",
    &[knobs(
        EffectKind::Space.name(),
        [
            s("decay"),
            s("size"),
            s("damp"),
            s("mod"),
            GAP,
            GAP,
            s("mix"),
            s("gain"),
        ],
    )],
);

// Live generators, after the Mod page's devices: a layout a fill, from the declared table
// (`GenParam`), its settings in their musical order, and its `Where`'s on knobs 7 and 8 as an
// effect's `Mix` and `Gain` are, `where1` being Euclid's `Hits`, an `Every`, a `Chance`, the
// amount of one of Marbles' rhythms, a named rhythm's `Child` or a `Cells`' `Rule`, and `where2`
// Euclid's `Rotate` or a `Cells`' `Start`. The fill and the `Where`, and a `Follow`'s source, are
// what the device is, so they're chosen in the app (the generators study, `generators-page.md` §6). A
// generator has no off/on settings of its own, so it's live, and `Acid`'s `Pitches` is a
// two-step knob. `Vary` is knob 6, beside the `Where`, on every fill; `Wave`, `Acid` and
// `Follow`, which draws only once it mutates, have it on a second page. A setting the shape hasn't now, as `Hits`
// without Euclid or `Vary` on a ramp, leaves its knob dark.

/// Knob 7 and 8's keys: what the `Where` has, whichever it is.
pub const WHERE_FIRST: &str = "where1";
pub const WHERE_SECOND: &str = "where2";
/// `Grids`' density knob's key: its own part's, whichever it plays.
pub(crate) const PART_DENSITY: &str = "partdensity";

const GEN_RAMP: DeviceLayout = live(
    "gen/ramp",
    &[knobs(
        FillKind::Ramp.name(),
        [
            s("from"),
            s("to"),
            s("curve"),
            s("s"),
            s("run"),
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
/// `Random`'s ends, then its spread and bias, as Marbles' X section has them, and on the Note
/// column its `Steps` on a second page, as the first is full; its `Weights` are typed, and each
/// weight is a knob on `Weights`' own device.
const GEN_RANDOM: DeviceLayout = live(
    "gen/random",
    &[
        knobs(
            FillKind::Random.name(),
            [
                s("from"),
                s("to"),
                s("spread"),
                s("bias"),
                s("run"),
                s("vary"),
                s(WHERE_FIRST),
                s(WHERE_SECOND),
            ],
        ),
        knobs("Steps", [__, __, s("steps"), __, __, __, __, __]),
    ],
);
/// A wave fills its first page, so a random one's `Vary` is on a second.
const GEN_WAVE: DeviceLayout = live(
    "gen/wave",
    &[
        knobs(
            FillKind::Wave.name(),
            [
                s("from"),
                s("to"),
                s("wave"),
                s("rate"),
                s("phase"),
                s("run"),
                s(WHERE_FIRST),
                s(WHERE_SECOND),
            ],
        ),
        knobs("Vary", [__, __, __, __, __, s("vary"), __, __]),
    ],
);
const GEN_ALTERNATE: DeviceLayout = live(
    "gen/alternate",
    &[knobs(
        FillKind::Alternate.name(),
        [
            s("from"),
            s("to"),
            s("run"),
            GAP,
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
/// A list's values are typed, so only its run and its rows are on the Roto.
const GEN_LIST: DeviceLayout = live(
    "gen/list",
    &[knobs(
        FillKind::List.name(),
        [
            s("run"),
            GAP,
            GAP,
            GAP,
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
/// `Acid`'s riff first, as a 303's randomiser has it, then its range, the spread and bias that
/// lean its notes, as a random's are placed, and `Vary` on a page of their own, as it has more
/// than eight.
const GEN_ACID: DeviceLayout = live(
    "gen/acid",
    &[
        knobs(
            FillKind::Acid.name(),
            [
                s("notes"),
                s("octave"),
                s("accent"),
                s("slide"),
                s("density"),
                s("pitches"),
                s(WHERE_FIRST),
                s(WHERE_SECOND),
            ],
        ),
        knobs(
            "Range",
            [
                s("from"),
                s("to"),
                s("spread"),
                s("bias"),
                __,
                s("vary"),
                __,
                __,
            ],
        ),
    ],
);
/// An `Acid`'s loops, its own device as on the Gen page: the rhythm's, then the notes', octaves',
/// accents' and slides', in the order the fill's chances come.
const GEN_ACID_LOOPS: DeviceLayout = live(
    "gen/acid-loops",
    &[knobs(
        "Loops",
        [
            s("trigloop"),
            s("noteloop"),
            s("octloop"),
            s("accloop"),
            s("slideloop"),
            GAP,
            GAP,
            GAP,
        ],
    )],
);
/// A `Random`'s weights on notes, its own device as on the Gen page: a knob for each degree from
/// the root, the first eight on one page and the last four on a second.
const GEN_WEIGHTS: DeviceLayout = live(
    "gen/weights",
    &[
        knobs(
            "Weights 1-8",
            [
                s("weight1"),
                s("weight2"),
                s("weight3"),
                s("weight4"),
                s("weight5"),
                s("weight6"),
                s("weight7"),
                s("weight8"),
            ],
        ),
        knobs(
            "Weights 9-12",
            [
                s("weight9"),
                s("weight10"),
                s("weight11"),
                s("weight12"),
                GAP,
                GAP,
                GAP,
                GAP,
            ],
        ),
    ],
);
/// An FX column's lane's `Length` and `Rate`, its own device as on the Gen page.
const GEN_LANE: DeviceLayout = live(
    "gen/lane",
    &[knobs(
        "Lane",
        [
            s("length"),
            s("rate"),
            s("start"),
            s("jump"),
            s("hold"),
            s("order"),
            s("reverse"),
            GAP,
        ],
    )],
);
/// The track's settings that move its phrases' walks, its own device as on the Gen page
/// (decisions 400 to 402), and on a second page its chain's (decisions 423 and 424).
const GEN_PHRASE: DeviceLayout = live(
    "gen/phrase",
    &[
        knobs(
            "Phrase",
            [
                s("order"),
                s("reverse"),
                s("orderx"),
                s("ordery"),
                s("phraselength"),
                s("phrasestart"),
                s("cue"),
                GAP,
            ],
        ),
        knobs(
            "Chain",
            [
                s("chainorder"),
                s("chainjump"),
                GAP,
                GAP,
                GAP,
                GAP,
                GAP,
                GAP,
            ],
        ),
    ],
);
/// A canon's moves; it writes every row, so it has no `Where`. Then its mutations, a Bloom
/// branch's, on a page of their own, with `Vary` on knob 6 as elsewhere, as it draws once it
/// mutates.
const GEN_FOLLOW: DeviceLayout = live(
    "gen/follow",
    &[
        knobs(
            FillKind::Follow.name(),
            [
                s("transpose"),
                s("invert"),
                s("axis"),
                s("reverse"),
                s("late"),
                s("fx"),
                __,
                __,
            ],
        ),
        knobs(
            "Mutation",
            [
                s("mutation"),
                s("repitch"),
                s("shift"),
                s("retro"),
                s("ratchet"),
                s("vary"),
                s("drop"),
                __,
            ],
        ),
    ],
);
const GEN_SCALE: DeviceLayout = live(
    "gen/scale",
    &[knobs(
        FillKind::Scale.name(),
        [
            s("from"),
            s("to"),
            s("curve"),
            s("s"),
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
const GEN_WALK: DeviceLayout = live(
    "gen/walk",
    &[knobs(
        FillKind::Walk.name(),
        [
            s("from"),
            s("to"),
            s("step"),
            s("edge"),
            s("run"),
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
const GEN_PINK: DeviceLayout = live(
    "gen/pink",
    &[knobs(
        FillKind::Pink.name(),
        [
            s("from"),
            s("to"),
            s("depth"),
            s("run"),
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
/// The rhythm there multiplied; it reads the rhythm itself, so it has no `Where`.
const GEN_MULTIPLY: DeviceLayout = live(
    "gen/multiply",
    &[knobs(
        FillKind::Multiply.name(),
        [s("factor"), s("masks"), __, __, __, __, __, __],
    )],
);
/// Grids' knobs as the module has them, the map first, then the part's density and chaos; it
/// makes its own rhythm, so it has no `Where`, and its part is what it is.
const GEN_GRIDS: DeviceLayout = live(
    "gen/grids",
    &[knobs(
        FillKind::Grids.name(),
        [
            s("x"),
            s("y"),
            s(PART_DENSITY),
            s("chaos"),
            GAP,
            s("vary"),
            s("note"),
            s("rows"),
        ],
    )],
);
/// A chance for each note division, as the module's sliders, the straight ones first, then the
/// bar and the note; it makes its own rhythm, so it has no `Where`, and its channel is what it is.
/// Channel `B`'s chances, which only `A+B` has, are on a second page, with `Vary` after them.
const GEN_DIVISIONS: DeviceLayout = live(
    "gen/divisions",
    &[
        knobs(
            FillKind::Divisions.name(),
            [
                s("d4"),
                s("d8"),
                s("d16"),
                s("d4t"),
                s("d8t"),
                s("d16t"),
                s("bar"),
                s("note"),
            ],
        ),
        knobs(
            "Channel B",
            [
                s("b4"),
                s("b8"),
                s("b16"),
                s("b4t"),
                s("b8t"),
                s("b16t"),
                __,
                s("vary"),
            ],
        ),
    ],
);
/// A noise ring: its ends, then the module's two knobs, `Change` and `Chance`, and the loop's
/// `Length`; its register makes its rhythm, so it has no `Where`, and it runs on from pass to pass
/// by itself, so it has no `Vary`.
const GEN_RING: DeviceLayout = live(
    "gen/ring",
    &[knobs(
        FillKind::Ring.name(),
        [
            s("from"),
            s("to"),
            s("change"),
            s("ones"),
            s("length"),
            __,
            __,
            __,
        ],
    )],
);
/// Chords on the Tonnetz: the first one's root and whether it's major or minor, where the moves
/// come from and whether they keep to the scale, then on a second page each move's weight and a
/// loop's length. Its list of moves is typed, and its voice is what it is, chosen in the app.
const GEN_TONNETZ: DeviceLayout = live(
    "gen/tonnetz",
    &[
        knobs(
            FillKind::Tonnetz.name(),
            [
                s("from"),
                s("triad"),
                s("moves"),
                s("inscale"),
                GAP,
                s("vary"),
                s(WHERE_FIRST),
                s(WHERE_SECOND),
            ],
        ),
        knobs("Moves", [s("p"), s("l"), s("r"), s("loop"), __, __, __, __]),
    ],
);
/// A Markov chain learned from the notes there: how closely it copies them and how far it strays;
/// it learns its rows with its notes, so it has no `Where`.
const GEN_VARIATION: DeviceLayout = live(
    "gen/variation",
    &[knobs(
        FillKind::Variation.name(),
        [
            s("order"),
            s("temperature"),
            GAP,
            GAP,
            GAP,
            s("vary"),
            __,
            __,
        ],
    )],
);
/// The values there as bits: each bit transform has one of the three, and the others' knobs are
/// dark.
const GEN_BITS: DeviceLayout = live(
    "gen/bits",
    &[knobs(
        "Bits",
        [
            s("flip"),
            s("keep"),
            s("turn"),
            GAP,
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);
/// A transform reworks what's there, with no settings of its own but its rows.
const GEN_REWORK: DeviceLayout = live(
    "gen/rework",
    &[knobs(
        "Rework",
        [
            GAP,
            GAP,
            GAP,
            GAP,
            GAP,
            s("vary"),
            s(WHERE_FIRST),
            s(WHERE_SECOND),
        ],
    )],
);

/// A table by kind, from each kind's place and what it holds there, every place given once,
/// which the compiler checks.
const fn by_kind<T: Copy, const N: usize>(entries: [(usize, T); N]) -> [T; N] {
    let mut table = [entries[0].1; N];
    let mut given = [false; N];
    let mut at = 0;
    while at < N {
        let (place, value) = entries[at];
        assert!(!given[place], "a kind is given twice");
        given[place] = true;
        table[place] = value;
        at += 1;
    }
    table
}

/// Each fill's live layout, by [`FillKind`]: the bit fills share one, as do the fills that rework
/// what's there.
const BY_FILL: [&DeviceLayout; FillKind::ALL.len()] = by_kind([
    (FillKind::Ramp as usize, &GEN_RAMP),
    (FillKind::Random as usize, &GEN_RANDOM),
    (FillKind::Walk as usize, &GEN_WALK),
    (FillKind::Pink as usize, &GEN_PINK),
    (FillKind::Wave as usize, &GEN_WAVE),
    (FillKind::Alternate as usize, &GEN_ALTERNATE),
    (FillKind::List as usize, &GEN_LIST),
    (FillKind::Acid as usize, &GEN_ACID),
    (FillKind::Grids as usize, &GEN_GRIDS),
    (FillKind::Divisions as usize, &GEN_DIVISIONS),
    (FillKind::Ring as usize, &GEN_RING),
    (FillKind::Tonnetz as usize, &GEN_TONNETZ),
    (FillKind::Follow as usize, &GEN_FOLLOW),
    (FillKind::Variation as usize, &GEN_VARIATION),
    (FillKind::Scale as usize, &GEN_SCALE),
    (FillKind::Invert as usize, &GEN_REWORK),
    (FillKind::Shuffle as usize, &GEN_REWORK),
    (FillKind::Reverse as usize, &GEN_REWORK),
    (FillKind::Snap as usize, &GEN_REWORK),
    (FillKind::Multiply as usize, &GEN_MULTIPLY),
    (FillKind::InvertBits as usize, &GEN_BITS),
    (FillKind::MaskBits as usize, &GEN_BITS),
    (FillKind::RotateBits as usize, &GEN_BITS),
]);

/// Each voice's layout, by [`SynthVoice`].
const BY_VOICE: [&DeviceLayout; SynthVoice::ALL.len()] = by_kind([
    (SynthVoice::VirtualAnalog as usize, &VA),
    (SynthVoice::VirtualAnalogVcf as usize, &VA_VCF),
    (SynthVoice::Acid as usize, &ACID),
    (SynthVoice::Supersaw as usize, &SUPERSAW),
    (SynthVoice::Fm as usize, &FM),
    (SynthVoice::PhaseDistortion as usize, &PD),
    (SynthVoice::Waveshaping as usize, &FOLD),
    (SynthVoice::Formant as usize, &FORMANT),
    (SynthVoice::Additive as usize, &ADDITIVE),
    (SynthVoice::Wavetable as usize, &WAVES),
    (SynthVoice::WaveTerrain as usize, &TERRAIN),
    (SynthVoice::SixOp as usize, &SIX_OP),
    (SynthVoice::FourOp as usize, &FOUR_OP),
    (SynthVoice::Chord as usize, &CHORD),
    (SynthVoice::Chiptune as usize, &CHIP),
    (SynthVoice::Swarm as usize, &SWARM),
    (SynthVoice::String as usize, &STRING),
    (SynthVoice::Modal as usize, &MODAL),
    (SynthVoice::Dust as usize, &DUST),
    (SynthVoice::Noise as usize, &NOISE),
    (SynthVoice::Kick as usize, &KICK),
    (SynthVoice::Snare as usize, &SNARE),
    (SynthVoice::Hat as usize, &HAT),
    (SynthVoice::Speech as usize, &SPEECH),
    (SynthVoice::CSaw as usize, &CSAW),
    (SynthVoice::Morph as usize, &MORPH),
    (SynthVoice::SawSquare as usize, &SAW_SQUARE),
    (SynthVoice::HardSync as usize, &SYNC),
    (SynthVoice::Triple as usize, &TRIPLE),
    (SynthVoice::Ring as usize, &RING),
    (SynthVoice::SawComb as usize, &SAW_COMB),
    (SynthVoice::Toy as usize, &TOY),
    (SynthVoice::ZFilter as usize, &Z_FILTER),
    (SynthVoice::FeedbackFm as usize, &FB_FM),
    (SynthVoice::ChaoticFm as usize, &CHAOS_FM),
    (SynthVoice::Vosim as usize, &VOSIM),
    (SynthVoice::Fof as usize, &FOF),
    (SynthVoice::Bowed as usize, &BOWED),
    (SynthVoice::Blown as usize, &BLOWN),
    (SynthVoice::Fluted as usize, &FLUTED),
    (SynthVoice::Bell as usize, &STRUCK_BELL),
    (SynthVoice::Drum as usize, &STRUCK_DRUM),
    (SynthVoice::WaveMap as usize, &WAVE_MAP),
    (SynthVoice::WaveLine as usize, &WAVE_LINE),
    (SynthVoice::Paraphonic as usize, &PARAPHONIC),
    (SynthVoice::TwinPeaks as usize, &TWIN_PEAKS),
    (SynthVoice::Clocked as usize, &CLOCKED),
    (SynthVoice::GrainCloud as usize, &GRAIN_CLOUD),
    (SynthVoice::Modem as usize, &MODEM),
]);

/// Distort's layouts, by its type: those that shape the wave share one, as do the pedals.
const BY_DISTORT_TYPE: [&DeviceLayout; DistortType::ALL.len()] = by_kind([
    (DistortType::Soft as usize, &SHAPING),
    (DistortType::Hard as usize, &SHAPING),
    (DistortType::Fold as usize, &SHAPING),
    (DistortType::Sine as usize, &SHAPING),
    (DistortType::Crush as usize, &CRUSH),
    (DistortType::Down as usize, &DOWN),
    (DistortType::Overdrive as usize, &PEDAL),
    (DistortType::Distortion as usize, &PEDAL),
]);

/// Comp's layouts, by its type.
const BY_COMPRESSOR_TYPE: [&DeviceLayout; CompressorType::ALL.len()] = by_kind([
    (CompressorType::Clean as usize, &CLEAN),
    (CompressorType::Console as usize, &CONSOLE),
    (CompressorType::Glue as usize, &GLUE),
]);

/// Each effect's layouts, by [`EffectKind`], one for each value of its type where its settings
/// change with it: Distort's by [`DistortType`], Comp's by [`CompressorType`].
const BY_EFFECT: [&[&DeviceLayout]; EffectKind::COUNT] = by_kind([
    (EffectKind::Distort as usize, &BY_DISTORT_TYPE),
    (EffectKind::Delay as usize, &[&DELAY]),
    (EffectKind::Reverb as usize, &[&REVERB]),
    (EffectKind::Chorus as usize, &[&CHORUS]),
    (EffectKind::Eq as usize, &[&EQ]),
    (EffectKind::Compressor as usize, &BY_COMPRESSOR_TYPE),
    (EffectKind::Limit as usize, &[&LIMIT]),
    (EffectKind::Phaser as usize, &[&PHASER]),
    (EffectKind::Flanger as usize, &[&FLANGER]),
    (EffectKind::Tape as usize, &[&TAPE]),
    (EffectKind::Plate as usize, &[&PLATE]),
    (EffectKind::Pitch as usize, &[&PITCH]),
    (EffectKind::Grains as usize, &[&GRAINS]),
    (EffectKind::Warps as usize, &[&WARPS]),
    (EffectKind::Lpg as usize, &[&LPG]),
    (EffectKind::Reso as usize, &[&RESO]),
    (EffectKind::Comb as usize, &[&COMB]),
    (EffectKind::Vowel as usize, &[&VOWEL]),
    (EffectKind::FreqShift as usize, &[&FREQ_SHIFT]),
    (EffectKind::Spring as usize, &[&SPRING]),
    (EffectKind::Space as usize, &[&SPACE]),
    (EffectKind::Rotary as usize, &[&ROTARY]),
    (EffectKind::Exciter as usize, &[&EXCITER]),
    (EffectKind::Stutter as usize, &[&STUTTER]),
    (EffectKind::Clouds as usize, &[&CLOUDS]),
    (EffectKind::Beads as usize, &[&BEADS]),
    (EffectKind::Echo as usize, &[&ECHO]),
]);

/// Each mod slot's layout, by its type's [`ModKind`].
const BY_MOD: [&DeviceLayout; ModKind::ALL.len()] = by_kind([
    (ModKind::Adsr as usize, &ADSR),
    (ModKind::Ahd as usize, &AHD),
    (ModKind::Drum as usize, &DRUM),
    (ModKind::Lpg as usize, &LPG_ENVELOPE),
    (ModKind::Lfo as usize, &LFO),
    (ModKind::Steps as usize, &STEPS_LAYOUT),
    (ModKind::Track as usize, &TRACK),
]);

/// The layouts of the devices every track has of its own, apart from its machine's.
const TRACKS_OWN: [&DeviceLayout; 10] = [
    &FILTER,
    &OUTPUT,
    &GROUP_OUTPUT,
    &RETURN_OUTPUT,
    &MASTER_OUTPUT,
    &PLAY,
    &SYNTH_PLAY,
    &ACCUMULATOR,
    &ARPEGGIO,
    &MACROS,
];

/// These layouts, each once, in their order, as a table by kind can hold one several times.
fn each_once(
    layouts: impl IntoIterator<Item = &'static DeviceLayout>,
) -> Vec<&'static DeviceLayout> {
    let mut found: Vec<&'static DeviceLayout> = Vec::new();
    for layout in layouts {
        if !found.contains(&layout) {
            found.push(layout);
        }
    }
    found
}

/// Every live generator's layout, a fill's each, and `Acid`'s loops', apart from [`ALL`], as a
/// generator is a phrase's rather than a kind of device a track always has.
pub static GENERATORS: LazyLock<Vec<&'static DeviceLayout>> = LazyLock::new(|| {
    each_once(
        BY_FILL
            .into_iter()
            .chain([&GEN_ACID_LOOPS, &GEN_WEIGHTS, &GEN_LANE, &GEN_PHRASE]),
    )
});

/// A live generator's layout, by its fill.
pub(crate) fn generator_layout(kind: FillKind) -> &'static DeviceLayout {
    BY_FILL[kind as usize]
}

/// Every kind of device's layout, for the tests and the stored setups: the machines', the track's
/// own, the mod slots' and the effects'.
pub static ALL: LazyLock<Vec<&'static DeviceLayout>> = LazyLock::new(|| {
    let operators: &'static [DeviceLayout] = &FOUR_OP_OPERATORS;
    let effects = BY_EFFECT.into_iter().flatten().copied();
    each_once(
        [&SAMPLER]
            .into_iter()
            .chain(BY_VOICE)
            .chain(operators)
            .chain(TRACKS_OWN)
            .chain(BY_MOD)
            .chain([&EMPTY])
            .chain(effects),
    )
});

/// A machine's layout.
pub(crate) fn machine_layout(machine: Machine) -> &'static DeviceLayout {
    match machine {
        Machine::Sampler => &SAMPLER,
        Machine::Plaits(voice) => BY_VOICE[voice as usize],
    }
}

/// An effect's layout, in the type or mode it's in, as its settings change with it.
pub(crate) fn effect_layout(insert: Insert) -> &'static DeviceLayout {
    let Some(kind) = insert.kind() else {
        return &EMPTY;
    };
    let by_type = BY_EFFECT[kind as usize];
    let at = insert
        .type_param()
        .map_or(0, |param| usize::from(insert.value(param)));
    by_type[at.min(by_type.len() - 1)]
}

/// A device's layout, by what it holds: its machine, its effect, or which of the track's own
/// devices it is.
pub fn layout_of(spec: &DeviceSpec, track: &Track) -> &'static DeviceLayout {
    let first = spec.rows().flatten().flatten().next();
    match first {
        Some(Control::Machine) => machine_layout(track.machine),
        // An operator's device, after 4-op's own.
        Some(Control::Setting(at)) if track.machine == Machine::Plaits(SynthVoice::FourOp) => {
            &FOUR_OP_OPERATORS[at.saturating_sub(FOUR_OP_SHARED) / FOUR_OP_EACH]
        }
        Some(Control::Insert(slot)) => effect_layout(track.inserts[slot]),
        Some(Control::FilterType) => &FILTER,
        // The Play device, by its title's `Jump`.
        Some(Control::Base(TrackControl::Jump)) if track.voice().is_some() => &SYNTH_PLAY,
        Some(Control::Base(TrackControl::Jump)) => &PLAY,
        Some(Control::Base(TrackControl::Order)) => &GEN_PHRASE,
        Some(Control::Mod(slot, _)) => BY_MOD[track.mods[slot].kind as usize],
        Some(Control::Accumulator(_)) => &ACCUMULATOR,
        Some(Control::Arpeggio(_)) => &ARPEGGIO,
        Some(Control::Macro(_)) => &MACROS,
        // An `Acid`'s loops, on a device of their own, and an FX column's lane.
        Some(Control::Gen { param, .. }) if param.is_loop() => &GEN_ACID_LOOPS,
        Some(Control::Gen { param, .. }) if param.weight_degree().is_some() => &GEN_WEIGHTS,
        Some(Control::Lane(..)) => &GEN_LANE,
        Some(control @ Control::Gen { .. }) => generator_layout(
            generator_of(control, track).map_or(FillKind::Ramp, |found| found.shape.kind),
        ),
        _ => match track.kind {
            TrackKind::Sound => &OUTPUT,
            TrackKind::Group(_) => &GROUP_OUTPUT,
            TrackKind::ReturnA | TrackKind::ReturnB => &RETURN_OUTPUT,
            TrackKind::Master => &MASTER_OUTPUT,
        },
    }
}

/// The generator a generator's setting is of.
fn generator_of(control: Control, track: &Track) -> Option<&Generator> {
    let Control::Gen {
        phrase, generator, ..
    } = control
    else {
        return None;
    };
    track.phrases.get(&phrase)?.generators().get(generator)
}

/// Whether a control is the app's alone, never on the Roto: what a device is, its machine, its
/// effect, the effect's type or mode, or a mod slot's type, as each swaps the device's whole
/// layout, and a generator's settings that are what it is or are typed.
pub fn app_only(control: Control, track: &Track) -> bool {
    match control {
        Control::Machine | Control::Insert(_) | Control::Mod(_, ModField::Type) => true,
        Control::Gen { param, .. } => param.spec().app_only,
        Control::InsertParam(slot, param) => track.inserts[slot].type_param() == Some(param),
        _ => false,
    }
}

/// The control of a device a layout's setting names, if the device has it. Steps' values are one
/// cursor stop on the page, so each of their keys finds the slot's by its number.
pub fn resolve(setting: &Setting, spec: &DeviceSpec, track: &Track) -> Option<Control> {
    let controls = || spec.rows().flatten().flatten();
    if let Some(at) = STEP_KEYS.iter().position(|step| *step == setting.key) {
        return controls().find_map(|control| match control {
            Control::Mod(slot, ModField::Step(_)) => {
                Some(Control::Mod(slot, ModField::Step(at as u8)))
            }
            _ => None,
        });
    }
    // A generator's `Where`'s first or second setting the Roto turns, whichever it has, and the
    // density of the part a `Grids` plays.
    let shape = || {
        let first = controls().find(|control| matches!(control, Control::Gen { .. }))?;
        generator_of(first, track).map(|found| found.shape)
    };
    let on_roto = |place: Where| {
        GenParam::of_where(place)
            .into_iter()
            .flatten()
            .filter(|param| !param.spec().app_only)
    };
    let wanted = match setting.key {
        WHERE_FIRST => on_roto(shape()?.place).next(),
        WHERE_SECOND => on_roto(shape()?.place).nth(1),
        PART_DENSITY => shape()?.grids.part.map(GenParam::density_of),
        _ => return controls().find(|control| key(*control, track) == setting.key),
    }?;
    controls().find(|control| matches!(control, Control::Gen { param, .. } if *param == wanted))
}
