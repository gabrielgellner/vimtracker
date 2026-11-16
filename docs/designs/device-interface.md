# Device descriptions: one place for each fact about an effect, machine, generator and mod type

A read-only design study, 2026-11-14, against `main` at 1cc0ff7d, with PR #736 (Space Echo) read
at its head `0edd54db`. Nothing was built or run. "Places" means an enum variant, match arm, table
row, const term, struct field or forwarding function that a new device needs only to be
registered, apart from its own DSP or algorithm. Every Rust block below is a sketch and has not
been compiled.

**As built** (2026-11-15). The plan in §6 is built. The study below is kept as it was written, so
its "today" and its counts are `main` before the plan; the design's
[Architecture](vimtracker.md#architecture) describes the code as it is, and
[adding-a-device.md](adding-a-device.md) replaces §6.3 with each family's checklist as built.

| Step | Built in |
|---|---|
| P0a, the model's pins | #741 |
| P0b, the Roto's pins | #778 |
| E1 `EffectSpec` | #752 |
| E2 `effect_settings!` | #757 |
| E3 `Setting` retired | #765 |
| E4 the slot, `SlotEffect` and `memories!` | #760, #762 |
| E5 the slot's facts from the trait | #771 |
| V1 `VoiceSpec` | #758 |
| V2 `SynthEngine` and `synths!` | #763 |
| G1 steps from bounds, `grid` a `Grid` | #744, with #746 |
| G2 `GenParamSpec` and `FillSpec` | #766 |
| M1 `ModField` and `ModSpec` in the model | #745 |
| S1 `Listed` | #750 |
| M2 the track page from the descriptions | #761 |
| E6, V3, G3, the Roto's lookups by kind | #778 |
| S2, as the UI plan's U7 and U8 | #772, #782 |
| §0's macro for the effect list, `effects!` | #786 |
| S2's Roto `feel` from the view | #788 |

Where it differs from the plan:
- §0's one list for the effects came after the rest, as `effects!` in #786 (the third architecture
  review's item 6, #784), with the test literals of `Shape` ending in `..Shape::default()`, as G2
  meant.
- S2's "the Roto's `feel`, `pick` and `key` read it" came after too, in #788 (#784's item 3). A
  knob's name still comes from `label_on`.
- The Gen page keeps keys of its own, and `GenParam::step` its own steps (#784's item 11). #806
  gave it one generator choice picker; routing its keys through `param::edit` was judged not worth
  it.

**Reviews 3 and 4.** The third architecture review (#784, vt-tui) and the fourth (#809, vt-engine,
vt-model and vt-roto) finished what the plan touched:

| Review item | Built in |
|---|---|
| #784 item 3, the Roto's knob feel from the view | #788 |
| #784 item 6, `effects!` and `Shape` literals | #786 |
| #784 item 8, this block and [adding-a-device.md](adding-a-device.md) | #789, #808 |
| #784 item 11, the Gen page's choice picker | #806 |
| #809 item 1, a golden for every effect and voice (`tests/all/goldens.rs`) | #810 |
| #809 item 2, one copy of each DSP block in `dsp/blocks/` (`one_pole`, `clip`, `line`, `xorshift`, `coefficient`), steps 1 to 4 | #815, #817, #818, #819 |
| #809 item 3, the engine's surface: `pictures`, `parts`, `pub(crate)` and `unreachable_pub` | #813 |
| #809 item 7, the Roto's fake behind the `fake` feature | #816 |

Each of #809's steps is bit-identical: the goldens didn't change, in debug or release. The one
difference is a NaN's: the kick's and the six-op's soft clip took the drums' NaN-safe form, so a
NaN gives a value at the end of the range instead of passing on.

**Review 6's one descriptor (#873).** §0's one parameter type, which §2.3 sketched and decision
339 stopped short of, is `vt_model::param::Descriptor`: the value-free part of `ParamView`, which
each family produces itself, with one `Reading` for how a value shows. Steps, each one PR:

| Step | Family | Built in |
|---|---|---|
| 1, the dump of every view (`vt-tui/tests/golden/views.txt`) | | #897 |
| 2, `Descriptor`, `Reading`, and `Steps`, `Pace`, `Reads`, `Choice` and `Typing` in the model | the shared constructors | #900 |
| 3, `insert::Param` folded into `Entry` (decision 357); `Entry::described` | machine settings, effects | #904 |
| 4, `ModSlot::reading` and `described`; `ModUnits` goes | mod slots | #906 |
| 5, `GenParam::described`; the generators' `Units` goes, a step still measured | generators | #908 |
| 6, `gen_params!`: the enum, `ALL` and `GEN_PARAMS` from one list (#809's item 6) | generators | the PR that closes #873 |

Where it differs from the design note: `Control::Base` isn't described from the registry, as four
of its controls have no entry; a Gen setting's step is still measured on the shape, as the row's
`Stepping` gives a different step at an end; and `exists_for`, `shown`, `reset` and `typed` stay
`GenParam`'s matches (decision 342), outside the list.

What's left:
- **Decided by the user, 2026-11-15:**
  - #809 item 2's step 5 (one Hermite form, a shared `DcBlocker`) is **declined**: share DSP only
    where it's bit-identical, and keep the variants the sound needs.
  - The Roto's Source choices stay off the Roto for now: `with_live` still passes no follow
    sources, on purpose.
  - `vt_model::convert` and `convert-nudges` are removed (#822).

## 0. The answer in brief

**Recommendation.**
- One static description per device, as `const` data:
  - effects: `EffectSpec`;
  - voices: `VoiceSpec`;
  - generators: `FillSpec`, with one `GenParamSpec` per setting;
  - mod types: `ModSpec`.
- All four share one parameter type, `Param`, which is the settings registry's `Entry` grown a
  little, together with its `Kind` and `Units`. They also share one `Listed` trait for the
  name/meaning lists.
- A fixed enum gives each device kind a dense index (`EffectKind`, `SynthVoice`, `FillKind`,
  `ModKind`), so tables and pools are arrays rather than named fields.
- On the engine side, each family gets a trait with default methods:
  - `SlotEffect` for effects;
  - `SynthEngine` for voices.
- Dispatch stays as `match` over an enum. A small `macro_rules!` writes the enum and its
  forwarding arms from one list. There is no `dyn`, no proc-macro crate, and nothing new in the
  audio callback.

**Not one `Device` trait for all four families.** What effects, voices, generators and mod types
share is data:
- names, keys, ranges, units, steps, choices, grouping;
- the widget contract (`ParamView`, §2.5).

What they do differs completely:
- an effect processes a stereo frame in a slot, with a buffer from a pool;
- a voice makes a sample per note, as a `Copy` value in a fixed array;
- a generator is a pure render on the edit thread;
- a mod slot runs inside `TrackState`.

A trait shared across families would have one family using each method, which breaks #319's
rule.

**What it buys.** The places a new device needs, measured on the real PRs (§1) and estimated
after the plan:

| Device | Example PR | Places today | Places after |
|---|---|---|---|
| Effect | #736 | about 66, in 10 files | about 7, in 5 files |
| Voice | #532 | about 29 | about 8 |
| Generator | #710 | about 69 | about 20 |
| Mod type | #491 | about 69, in 15 files | about 12 |

The net change is about −3,000 lines. Behaviour stays identical, checked against the goldens,
the screenshots, `SettingId` keys and order, and the Roto's `PluginHash`es and setup versions.
Roto knob orders and `kind` strings stay hand-written (§5).

**What it costs.**
- Two `macro_rules!` shapes of about 60 lines each: one generates an effect's settings struct,
  the other generates an enum's dispatch. Code inside a macro is harder to read, and rustfmt
  won't format it.
- One extra level of indirection when reading code: a description table instead of a `match`.
- About 14 PRs, most of them S or M, and half of them must wait for #320's split.

**A bug the current design caused, found while reading #736.**
- Space Echo's `Input` uses `Setting::Input` (`ECHO_PARAMS`, PR #736, `insert/space.rs`). It
  therefore inherits the limiter's display: `param_entry` maps it to `Units::Input`
  (`settings.rs:1476`), shown as `input_db` = `value/255 × 24 dB` (`settings.rs:325`,
  `insert/dynamics.rs:440`). So the default `80` reads "+12.0 dB".
- The DSP plays it as `from_middle(input) × MOST_ECHO_INPUT_DB`: centred, so `80` is 0 dB (PR
  #736, `echo.rs`, the line `let input_db = from_middle(..) * MOST_ECHO_INPUT_DB`).
- `Bass` and `Treble` borrow `Setting::Low` and `Setting::High`, so they show the EQ's ±15 dB
  linear `Boost`. The DSP's shelves reach 23.15 dB and 15.38 dB, on a power curve (`BASS` and
  `TREBLE` in `echo.rs`).
- This is what happens when the units come from a setting shared across effects rather than
  from the parameter itself. Worth fixing in #736 before it merges.

## 1. What adding one device touches today

### 1.1 One effect: Space Echo, #736 (about 66 places, 176 registration lines, in 10 files)

The device's own code is `dsp/effects/echo.rs` (+772) and `Echo` in `insert/space.rs` (+248).
Everything else is registration.

| File | Places | What |
|---|---|---|
| `vt-model/src/insert.rs` | 13 | `Insert::Echo`; three new `Setting`s; an arm each in `highest` and `span` (`insert.rs:240`, `:277`); `ALL` 27→28 and its entry (`:438`); arms in `name` (`:481`), `meaning` (`:514`), `params` (`:551`), `read` (`:612`) and `write` (`:784`); the `type_param` or-arm (`:836`); a `grid` arm (`:868`) |
| `vt-model/src/settings.rs` | 10 | `Effect::Echo` and `Effect::ALL` with its entry (`:1268`); arms in `params`, `key`, `name` and `of`; an `effect_start` arm (`:1952`); a `SLOT_SIZE` term (`:2324`); the import list |
| `vt-engine/src/dsp/effects/memory.rs` | 19 | the `Memory` variant (`:55`) and its constructor; forwarding arms in `process` (`:175`), `quiet_frames` (`:200`), `follow` (`:225`), `seed`, `settle` (`:313`) and `clear` (`:337`); a `MemoryCount` field and arm (`:364`); a `MemoryPool` field, its capacity line and its fill loop (`:427`, `:456`); arms in `put` (`:541`) and `take` (`:573`); terms in `owe` (`:611`), `next_owed` (`:634`) and `spare` (`:753`); the import |
| `vt-engine/src/dsp/effects/distort.rs` | 5 | the import; `same_effect` (`:154`); the mix/gain arm in `set` (`:477`); a `Mono` arm (`:903`); the not-a-distortion arm in `effect` (`:987`) |
| `vt-engine/src/link.rs` | 2 | a `meet_memory_needs` block (`:316`); the return count |
| `vt-engine/src/dsp/effects.rs` | 1 | `pub mod echo` |
| `vt-tui/src/kitty.rs` | 9 | the slot drawing: a variant, `of`, draw and caption arms, imports |
| `vt-tui/src/roto/layouts.rs` | 4 | the `ECHO` layout, which is meant to be hand-written; `ALL` 102→103 and its entry (`:2031`); an `effect_layout` arm (`:2195`) |
| `vt-tui/src/views/track/zoom.rs` | 1 | an or-arm (`:302-316`) |
| `vt-engine/examples/costs.rs` | 2 | a bench entry |

Each of `Echo`'s 13 fields is also written five times in `space.rs`:
- the struct field;
- `DEFAULT`;
- the `ECHO_PARAMS` row;
- a `read` arm;
- a `written` arm.

**Compare a new mode of an existing effect**, Clouds' spectral mode in #714. It needs about 6
places: `CloudsMode::Spectral`, `ALL`, `CLOUDS_MODE_NAMES` and Cargo lines. There are 3 dispatch
arms inside `clouds.rs`, and nothing in `settings.rs`, the layouts or the TUI. A mode is one more
choice of a stepped setting, which every reader already handles through the description. That is
the evidence that description-driven code works here: where it already exists, the cost of a
device is about a tenth.

### 1.2 One voice: Supersaw, #532 (about 28 places, 29 now with `KINDS`)

Own code: `dsp/voices/supersaw.rs` (+434) and two helpers in `blocks/drift.rs`.

| File | Places | What |
|---|---|---|
| `vt-engine/src/voice.rs` | 9 | the import; the `Synth` variant (`:81`); a `new` arm (`:135`); the stereo path in `next_sides` (`:191`); a `next` arm (`:200`); `drifts`, `seed_drift` and `time` arms |
| `vt-model/src/song.rs` | 8 (9 now) | the `SynthVoice` variant; `ALL` and its entry; arms in `name`, `meaning` and `holds`; `Machine::type_param` and `Machine::grid` arms; now also a `KINDS` row |
| `vt-model/src/settings.rs` | 3 | the `SUPERSAW` entry table, which is the description itself; a `voice_entries` arm (`:1159`); a `voice_key` arm (`:1212`) |
| `vt-tui/src/roto/layouts.rs` | 4 | the layout, which is meant to be hand-written; `ALL` and its entry; a `machine_layout` arm (`:2137`) |
| Cargo, `dsp.rs`, `voices.rs` | 4 | modules and the dependency |

Voices are already half description-driven. Their settings are `Entry` tables, held in a
`BTreeMap<SettingId, u8>` on the track (`song.rs:417-420`), and they reach the engine as one
normalised `Settings` array (`dsp/voices.rs`). What's still repeated is the
voice's own facts: 6 matches of 49 arms in `song.rs`, plus 2 in `settings.rs`. Engine dispatch
is a further 3 matches of 49 arms.

### 1.3 One generator: Tonnetz, #710 (about 69 places, about 354 registration lines)

Own code: `shape/tonnetz.rs` (+651) and `progression()` in `views/shape.rs`.

- **`shape/params.rs`, about 34 places.** There are 9 new `GenParam` variants, and each needs
  arms in `key` (`:533`), `name` (`:644`), `units` (`:749`), `held_in_engine` (`:849`),
  `exists_for` (`:888`), `shown` (`:1346`), `typing` (`:1574`), `choices` (`:1623`), `step`
  (`:1683`), `reset` (`:1959`), `typed` (`:2126`), `bounds` (`:2420`), `number` (`:2509`) and
  `set_number` (`:2636`). On main, `Self::Triad` appears at 13 lines of that file.
- **`shape.rs`, 11 places:** the `FillKind` variant, `ALL`, `name`, `description`, `fills`, the
  `Shape` field and its default, and `draws`.
- **The Roto, 5 places:** the layout, the list, the lookup, and `app_only` (`layouts.rs:2286`).
- **`shaping.rs`, 9 places;** `views/shape.rs`, 3; `render.rs`, 3; `generator.rs`, 2.
- **Test ripple:** about 20 test struct literals gained `tonnetz: Tonnetz::default()`.

### 1.4 One mod type: Drum, #491 (about 69 places, in 15 files)

Own code: the `Drum` envelope in `envelope.rs` and its curve in `views/curve.rs`.

Each type's state is its own field of `ModSlot` (`modulation.rs:182-218`). `ModField`
(`track_page/controls.rs:15`) is dispatched in 8 places across `track_page/` and `roto/` (#726).
Each of those reads a tuple such as `let (adsr, ahd, drum, lpg, lfo) = (...)`
(`views/track.rs:212`), so every new type widens about 6 tuples across the TUI:
- `editing.rs`, 10 places;
- `modulation.rs` in the model, 16;
- `roto/devices.rs` and `roto/edits.rs`, 5 each;
- `typing.rs`, 4;
- `pickers.rs` and `controls.rs`, 3 each.

This is the least description-driven family of the four.

### 1.5 What is already description-driven, and stays

Much of the interface already reads descriptions, so this study changes less there than #718's
"4 to 6 places" suggests.

- **The FX page needs no per-effect code.** A slot's device is `DeviceSpec::slot`, built from
  `insert.type_param()`, `title_param()` and `grid()` (`track_page/fx.rs:27-43`). Stepping,
  typing, `K` and the value text all go through `params()`:
  - stepping: `editing.rs:580-596`;
  - typing: `typing.rs:93-98`;
  - `K`: `pickers.rs:164`;
  - value text: `views/track.rs:190-202`.
- **The registry.** `SettingId` (`settings.rs:2385-2716`) gives every lockable or modulatable
  setting a key, a name, a label, a `Kind` and a default. FX commands are
  `FxCommand::Set(SettingId)` (`step.rs:66-90`). The engine modulates every effect through
  `params()` and `write` (`track_state.rs:2050-2071`).
- **The Roto's devices** are made from the layouts by key, plus the registry: feel and colour
  come from units (`roto/devices.rs:163-202, 817-830`). There is no per-effect code in
  `devices.rs` or `edits.rs`.
- **Presets** carry `Insert` and `Machine` through serde (`preset.rs:305-371`), so a new device
  needs nothing there. #727's three preset kinds are a separate issue.

So the repetition sits in four places:
1. **The model's identity matches.** The same device is listed in parallel enums and matches:
   `Insert` and `Effect`, `SynthVoice`'s 6 matches, `GenParam`'s 13.
2. **The engine's forwarding.** `memory.rs`'s 19-way hand repetition, `distort.rs`'s arms and
   `voice.rs`'s 49-arm matches.
3. **Mod types,** which have no description at all.
4. **The Roto's lookups,** which could be indexed.

## 2. What a device interface should look like

### 2.1 Three descriptions already exist, and one place where none does

| Description | Where | Carries | Read by |
|---|---|---|---|
| `settings::Entry` | `settings.rs:459` | key, name, label, `Kind` (`Continuous { top, middle, units }` or `Stepped { choices, wraps }`), default | the registry, voices, track, mod own settings, sampler, macros |
| `insert::Param` | `insert.rs:316` | name, key, label, `setting: Setting`, choices, wraps, default; **no units** | effects; turned into an `Entry` by `param_entry`, a 35-arm units table keyed by `Setting` (`settings.rs:1457-1513`) |
| `GenParam` methods | `shape/params.rs:282` | 99 variants × about 13 matches: key, name, `Units`, `Typing`, step, bounds, number | the shape panel, the Gen page, `K`, the Roto, the engine's holds (`gens.rs:128-148`) |
| (none) | mod types | `ModKind` facts in matches (`modulation.rs:23-110`); fields in the TUI's `ModField` | 8 dispatches |

Two layout types already exist, and both stay:
- the track page's `DeviceSpec` (`track_page/device.rs:31`), made from a model `Grid`;
- the Roto's `DeviceLayout` (`roto/layouts.rs:75`), hand-written by key.

### 2.2 The options

| Option | For | Against | Verdict |
|---|---|---|---|
| **A. Static description tables plus enum indices.** `const EFFECTS: [EffectSpec; N]`, `kind as usize` to index | The model is data; `const` tables cost nothing at run time; prior art agrees (§3); rustfmt formats them; nothing is hidden | A table can't hold code, so per-field accessors and engine forwarding still need generating | **Yes, as the core** |
| **B. A `macro_rules!` per family.** One list generates the struct, `DEFAULT`, `PARAMS` and accessors, or an enum and its forwarding | Removes the 5× field writing and the 19×/49× forwarding; no new crate; #720(b) already proposes it, with 26 users | Code inside a macro is harder to read, compile errors point into it, rustfmt skips it | **Yes, for the two code-shaped repetitions only** |
| **C. A proc-macro derive, nih-plug style.** `#[derive(Effect)] struct Echo { #[param(key = "mode", ..)] mode: u8 }` | The nicest to write; attributes sit next to the fields; rustfmt formats the struct | A new `vt-derive` crate with `syn` and `quote` (seconds on every clean build); a second language to maintain; heavier than one shape needs | Not now. Revisit if B's macros grow past about 100 lines |
| **D. Traits with `dyn`.** `Box<dyn SlotEffect>` | No forwarding enum | Voices are `Copy`, in a fixed array ("The audio thread can't allocate, so each voice has room for any of them", `voice.rs:78`): a box per note would allocate in the callback. For effects it loses `Clone` (`InsertState` derives it, `distort.rs:78`), the `match` on kind that the pool's buckets need, and inlining of `process` into the slot loop. `follow` would need a downcast inside every impl | **No** |
| **E. `enum_dispatch` crate** | Generates the forwarding | A proc-macro dependency for about 40 lines that `macro_rules!` does | No |
| **F. Values as arrays.** `[u8; N]` per effect, as voices and Vital have them | The most generic | Changes the song and preset format (serde field names become keys: `kind` vs `"type"`, `ping_pong` vs `"pingpong"`), and every DSP `follow` reads typed fields. The user's own songs (`confusion-pump-panel`) would stop loading, and conversions are ruled out | No, for this cleanup |

So the design is **A + B**, with traits with default methods on the engine side, dispatched
statically through enums.

### 2.3 The shared parameter type

`Entry` becomes the one parameter description. It moves from `settings.rs` to a new
`vt-model/src/param.rs` with `Kind` and `Units`, and `settings` re-exports all three.
`insert::Param` is folded into it.

```rust
/// One setting, described once: everything the pages, `K`, typing, the grid, the Roto,
/// modulation and saving need to know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    /// What songs store it by, and the Roto's layouts name it by. Never changes.
    pub key: &'static str,
    /// Lists, the status line, the Roto's knob name.
    pub name: &'static str,
    /// The grid's 3- or 4-letter label. None: modulation moves it, a step can't lock it.
    pub label: Option<&'static str>,
    /// Its range and how it reads, or its choices.
    pub kind: Kind,
    pub default: u8,
    /// How far `+` and `_` move it. 0 means "as its kind has it": 0x10 on the hex scale,
    /// 1 for a list. Today this is re-stated by hand in editing.rs and the Roto (#726).
    pub jump: u8,
    /// What it is to its device.
    pub place: Place,
}

/// Where a setting sits in its device. These used to be read off `Setting` by name
/// (`Insert::type_param`, insert.rs:836; `default_grid`'s Mix/Gain test, insert.rs:955).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// One of the device's own: in the grid.
    Own,
    /// What the device is: in its title, not on the Roto, not lockable, and changing it
    /// starts the effect afresh (Distort's and Comp's type, an LPG's or Comb's mode).
    Type,
    /// In the title too, but a played setting: Beads' quality.
    Title,
    /// What comes out, Mix and Gain: the last column, and the Roto's knobs 7 and 8.
    Output,
}

// Kind and Units are settings.rs's own (settings.rs:40, :53), unchanged. Kind::shown (:295),
// has_fine, wraps, centred and top already give the widget what it needs (§2.5).
```

Two consequences:

**`param_entry`'s units table goes.** Each effect states its units on its own `Param`, which
fixes #736's Echo `Input` for good. The table is `settings.rs:1457-1513`.

**`Setting` loses its job of carrying units.** It is a flat 115-variant enum (`insert.rs:61-238`)
that every effect shares, which is how Echo borrowed the limiter's `Input`.
- Afterwards it only names a field for `read` and `write`, and after E3 (§6) not even that.
- `highest` and `span` (`insert.rs:240-310`) become `param.kind.top()`, which also settles their
  `Gain` disagreement.
- Outside vt-model, `Setting` is used in one place (`track_state.rs:10`). The engine's DSP never
  sees it.

### 2.4 Effects: the core types

**The model.** There is one `EffectKind`, which is today's `settings::Effect` renamed, with one
table row per effect.

```rust
// vt-model/src/insert/kind.rs
/// An effect apart from its settings: a dense index for tables and pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum EffectKind { Distort, Delay, Reverb, /* .. */ Clouds, Beads, Echo }

impl EffectKind {
    /// In the order SettingId numbers them, which is Insert::ALL's without Empty. Never
    /// reordered: the order of a track's saved `settings` map follows it.
    pub const ALL: [Self; 27] = [/* .. */];
    pub const COUNT: usize = Self::ALL.len();
    #[inline] pub const fn spec(self) -> &'static EffectSpec { &EFFECTS[self as usize] }
}

impl Insert {
    /// One match, the only per-effect one in the model besides serde's variants.
    pub const fn kind(self) -> Option<EffectKind> { /* Insert::X(_) => Some(EffectKind::X) */ }
}

/// One effect, described once.
pub struct EffectSpec {
    /// What its settings are stored under ("2.delay.feedback"): `Effect::key` today.
    pub key: &'static str,
    /// On screen ("Delay") and in the picker's line.
    pub name: &'static str,
    pub meaning: &'static str,
    /// Every setting it keeps, in any mode, in the order SettingId numbers them.
    pub params: &'static [Param],
    /// Which of them it shows: all of them, or by its type's value, places in `params`.
    /// Distort's and Comp's modes (insert.rs:551-585) become index lists, not copied arrays.
    pub shown: Shown,
    /// The FX page's columns by key, top to bottom, or None for the default grid
    /// (insert.rs:868-952 today, by `Setting`). By key, as `Machine::grid` and the Roto do.
    pub grid: Option<&'static [&'static [&'static str]]>,
}

pub enum Shown {
    All,
    ByType { type_at: u8, modes: &'static [&'static [u8]] },
}

pub const EFFECTS: [EffectSpec; EffectKind::COUNT] = [ /* one row each */ ];

// effect_start and SLOT_SIZE (settings.rs:1952-2349, 400 lines) become const loops, as
// VOICE_STARTS (settings.rs:1918) already is:
const EFFECT_STARTS: [usize; EffectKind::COUNT] = {
    let (mut starts, mut start, mut at) = ([0; EffectKind::COUNT], 0, 0);
    while at < EffectKind::COUNT { starts[at] = start; start += EFFECTS[at].params.len(); at += 1; }
    starts
};
```

After this, `Insert::name`, `meaning`, `params`, `type_param`, `title_param` and `grid`, and
`Effect::params`, `key`, `name` and `of`, are each one line over `kind().spec()`.

The repeated `position(setting)` searches go too:
- `controls.rs:365-370`;
- `track_state.rs:2056-2063`, every 32 frames for every slot;
- `settings.rs:2610`.

They become `spec.shown(...)[at]`, a direct index. The `track_state.rs` one is a small speed-up
as well.

**Each effect's fields, written once** (#720(b)). The macro keeps field names and serde
attributes exactly as written, so the save format doesn't change.

```rust
effect_settings! {
    /// A chorus's settings. One a file doesn't have starts as new.
    #[serde(default)]
    pub struct Chorus {
        kind:  ChorusType = ChorusType::Chorus => listed("Type", "type", None, &CHORUS_TYPE_NAMES, WRAPS).placed(Place::Type),
        rate:  u8 = 0x80        => hex("Rate", "rate", "RATE", Units::Percent),
        depth: u8 = 0x80        => hex("Depth", "depth", "DPTH", Units::Percent),
        mix:   u8 = 0x80        => WET_MIX,
        gain:  u8 = UNITY_LEVEL => GAIN,
    }
}
// Generates:
//   the struct with these fields and attributes;
//   `impl Default`;
//   `Chorus::DEFAULT`;
//   `Chorus::PARAMS: [Param; 5]`, each `default` filled from the `= ..` (`$default as u8`);
//   `fn get(self, at: usize) -> u8` and `fn set(self, at: usize, value: u8) -> Self`, by
//   place, through a two-line `Byte` trait: u8 as itself, an enum with ALL by place.
// Distort's `bits` (stored 2..=12, shown 0..=10) takes an escape hatch:
//   `bits: u8 = 9 => BITS via (bits_get, bits_set)`.
```

One line per field replaces five. The `read` and `written` pairs alone are about 735 lines across
the 27 effects. `Insert::read` and `Insert::write` (`insert.rs:612-834`) become one generated
match: `Insert::X(s) => s.get(at)`.

**The engine.** It gets one trait per effect DSP, with defaults for what most effects don't do:

```rust
/// What a slot effect's DSP does. Implemented by each effect's struct and dispatched by `match`
/// through `Memory` (or `Inline`, E5), never `dyn`. Nothing here may allocate but `new`, which
/// runs on the edit thread (link.rs's `meet_memory_needs`).
pub trait SlotEffect: Sized {
    /// Its settings, as the model has them.
    type Settings: Copy;
    /// What it does with a sound that's the same on both sides (decision 147; distort.rs:903).
    const MONO: Mono = Mono::Stereo;
    /// Made on the edit thread, with its long buffers.
    fn new(rate: u32) -> Self;
    /// Take what's set now. `now` carries where the song is and the frames rendered, which
    /// delay/flanger/stutter/beads/reverb take in different combinations today (memory.rs:225-263).
    fn follow(&mut self, settings: &Self::Settings, now: &Now);
    /// Jump to what's set, as it's only just started.
    fn settle(&mut self) {}
    /// Empty its buffers before the pool hands it out again.
    fn clear(&mut self);
    fn process(&mut self, input: [f32; 2], frame: &Frame) -> [f32; 2];
    /// Its tail: how many quiet frames in a row mean nothing's left (memory.rs:200).
    fn quiet_frames(&self, rate: u32) -> u32;
    /// Frames it delays the sound by: Clouds' spectral mode (clouds.rs:864), the limiter.
    fn latency(&self) -> u32 { 0 }
    /// What goes by the frame rather than by what it's done (memory.rs:260).
    fn keep_time(&mut self, _frames: u64, _song: SongPlace) {}
    /// A generator of its own, from the track's: Clouds' and Beads' grains.
    fn seed(&mut self, _rng: Pcg32) {}
    /// About to sound with nothing going in: a knocked spring, a struck stutter.
    fn waking(&self) -> bool { false }
    fn strike(&mut self) {}
}

pub struct Now { pub song: SongPlace, pub frames: u64 }
/// What `process` may read: the smoothing, the rate, the sidechains (`SidechainFrames`) and the note.
pub struct Frame<'a> { pub smoothing: f32, pub rate: u32, pub sidechains: &'a SidechainFrames, pub note: f32 }
```

**One list makes the buffered effects' enum and all its forwarding:**

```rust
// dsp/effects/memory.rs
memories! {
    Delay(Delay), Reverb(Reverb), Chorus(Chorus), Limit(Limiter), Flanger(Flanger),
    Tape(Tape), Plate(Plate), Pitch(PitchShifter), Grains(Granular), Warps(Warps),
    Reso(Resonator), Comb(Comb), FreqShift(FreqShifter), Spring(Spring), Space(Space),
    Rotary(Rotary), Stutter(Stutter), Clouds(Clouds), Beads(Beads), Echo(Echo),
}
// Generates:
//   pub enum Memory { $( $v(Box<$dsp>), )* }   -- boxed on the edit thread, as now
//   Memory::new(kind, rate) -> Option<Self>    -- the 20 constructors (memory.rs:80-172)
//   Memory::kind(&self) -> EffectKind
//   pub const BUFFERED: [bool; EffectKind::COUNT]   -- `kept_by` (memory.rs:281)
//   process, quiet_frames, follow, settle, clear, keep_time, seed, waking, strike:
//     each `match self { $( Self::$v(dsp) => dsp.method(..), )* }`
//   follow:
//     `match (self, insert) { $( (Self::$v(dsp), Insert::$v(s)) => dsp.follow(s, now), )* _ => {} }`
```

**The pool and counts become arrays indexed by kind**, so they hold no per-effect code at all:

```rust
pub struct MemoryCount(pub [u16; EffectKind::COUNT]);
pub struct MemoryPool { free: [Vec<Memory>; EffectKind::COUNT], owed: MemoryCount }

impl MemoryPool {
    pub fn take(&mut self, kind: EffectKind) -> Option<Memory> {
        let mut memory = self.free[kind as usize].pop()?;  // no allocation: pop from capacity
        memory.clear();
        Some(memory)
    }
    pub fn next_owed(&mut self) -> Option<Memory> {
        for kind in EffectKind::ALL {          // replaces 19 six-line blocks (memory.rs:634-750)
            let at = kind as usize;
            if self.owed.0[at] > 0 && let Some(memory) = self.free[at].pop() {
                self.owed.0[at] -= 1;
                return Some(memory);
            }
        }
        None
    }
}
// for_song, put, owe, spare and link.rs's meet_memory_needs (link.rs:316-374) become loops
// over EffectKind::ALL, filtered by BUFFERED.
```

**Why this stays allocation-free and lock-free:**
- `Memory::new` boxes on the edit thread, and the box crosses the existing `rtrb` queue, exactly
  as today.
- Every `Vec` in the pool is made with `POOL_CAPACITY` up front (`memory.rs:452`), and the
  callback only `pop`s and `push`es within that capacity. `put` keeps its `forget` guard.
- Dispatch is a `match` compiled to a jump table. There are no vtables, and `#[inline]`
  forwarding is kept.
- `MemoryCount` changes from 19 named `u16`s to an array of them. It is `Copy` either way.

`next_owed` would visit kinds in `EffectKind` order rather than its hand order, so buffers go
back to be freed in a different order. That isn't heard. The goldens and `cargo test --release`
(`rt_guard`) are the check.

**The slot (E5).** The rest of `InsertState`'s per-effect arms come from the trait and the
description:
- `MONO` replaces `mono()`'s 30 arms (`distort.rs:903-936`).
- `same_effect` (`distort.rs:154-188`) becomes `before.kind() == after.kind()`, plus, for an
  effect whose description shows each type's own settings (`Shown::ByType`: Distort and Comp),
  "its type is equal". A type change on any other effect (Chorus, LPG, Comb…) keeps it running,
  as before. That's the rule E5 (#771) built; a plain "its `Place::Type` param is equal" would
  have restarted those effects on a type change.
- `set`'s mix/gain extraction (`distort.rs:477-519`) reads the `Place::Output` params, with a
  `const MIXING: Mixing` on the trait (`Wet`, `Inside` for Clouds and Beads, `AllWet` for the
  EQ, `Parallel` for Comp).
- The 7 small effects kept inline (`eq`, `dynamics`, `phaser`, `gate`, `vowel`, `exciter`,
  `pedal`; `distort.rs:99-112`) implement the same trait, and #721's `follow_fresh` helper
  serves them all.

### 2.5 The widget contract: what a parameter must carry for a UI to draw and edit it

The parallel UI-components study builds its widgets on this. A widget gets one resolved value
per setting, so it needs no per-device code:

```rust
/// One setting as a widget draws and edits it, resolved where it is: an effect's in its slot,
/// a voice's on its track, a generator's on its column, a mod slot's own. Made by one function
/// per family; every page, `K`, typing, the status line and the Roto read only this.
pub struct ParamView {
    pub key: &'static str,           // what layouts name it by, and what's saved
    pub name: &'static str,          // status line, lists, the Roto's knob name
    pub label: Option<&'static str>, // grid label; None: steps can't lock it
    pub value: i32,                  // now, within `bounds`
    pub fine: Option<u8>,            // its fine byte, where its kind has one (Kind::has_fine)
    pub bounds: (i32, i32),          // resolved: a GenParam's From/To hang on the column
    pub default: i32,                // what `x` puts back (a generator's via `Starts`)
    pub nudge: i32,                  // `=` and `-`
    pub jump: i32,                   // `+` and `_`
    pub wraps: bool,                 // goes round past either end (a phase, a list that wraps)
    pub centred: bool,               // bars draw from the middle (pan, boost, bend)
    pub toggle: bool,                // off/on: drawn as a switch, a button on the Roto
    pub choices: Choices,            // None, a static list of names, or built (a Follow's sources)
    pub text: String,                // in real units: "1.21 kHz", "-6.0 dB", "Juno I", "1/16"
    pub typing: Typing,              // how `i` takes it: Hex, Whole{signed}, Listed, Toggled, Rate...
    pub units: Units,                // for colour and feel: the Roto's family (devices.rs:163)
    pub engaged: bool,               // not dimmed: not its default (views/track.rs:95-104 today)
}
```

| A widget must... | Field | Where it comes from |
|---|---|---|
| label it, short and long | `label`, `name` | `Param` / `GenParamSpec` |
| show the value in real units | `text`, from `Units` | `Kind::shown` (`settings.rs:295`); `GenParam::shown` (`params.rs:1346`) |
| know its range | `bounds`, `wraps` | `Kind::top`; generators resolve theirs from `Context` |
| step finely and coarsely | `nudge`, `jump`, `fine` | `Param.jump`, `GenParamSpec.nudge` and `jump` |
| offer choices | `choices`, `toggle` | `Kind::Stepped`; generator `choices` |
| take typing | `typing` | `Typing` (`params.rs:226`), generalised |
| colour it by meaning | `units` (to family) | `devices.rs:163` (spartan, semantic colour) |
| put it in rows, columns, a title, pages or knobs | not on the param: on its device | the screen: `Grid` and `Place` from the spec; the Roto: the hand-written `DeviceLayout` pages (§5) |

Grouping is the device's job, not the parameter's:
- on screen, the spec's `grid` and `Place` give rows and columns and the title;
- on the Roto, the hand-written 8-knob pages give its own order (§5).

So a widget renders a `ParamView` inside a cell that its device's `Grid` places.

**One gap.** `Control` (`track_page/controls.rs:113-160`) is dispatched in about 9 functions:
`value`, `name_on`, `label_on`, `step`, `type`, `pick`, the Roto's `feel`, `pick` and `key`.

The one-place fix is a single `fn view(control, track, song) -> Option<ParamView>`. Its
families:
- `InsertParam`: from `EffectSpec`;
- `Setting` and `Base`: from the registry;
- `Gen`: from `GenParamSpec` with its `Context`;
- `Mod`: from `ModSpec`.

That's the hand-off to the UI study (§7). The track's own fields without registry entries, such
as `Choke`, `Group` and `Speed`, get small `Param` rows of their own.

### 2.6 Saving and the Roto: what must stay byte-for-byte

These stay unchanged, each pinned by a test before any of this starts (P0 in §6):

| Fact | Why it matters | Where |
|---|---|---|
| `Insert`'s variant names and every effect struct's field names and serde attributes | songs and presets serialise `Insert` by serde | `insert.rs:28`, `insert/*.rs` |
| `SettingId` **keys** | songs store settings and locks by key | `settings.rs:2642-2661` |
| `SettingId` **order** | `SettingId: Ord` orders each track's saved `settings` and `fine` maps (`song.rs:417-424`), so reordering changes saved bytes, and the goldens catch it | `settings.rs:2385` |
| `GenParam` keys, `FillKind` and `ModKind` names | macro targets save `param_key`; shapes and mods serialise by name | `macros.rs:80-84` |
| Roto `kind` strings | `PluginHash::of("vimtracker:" + kind)` is the setup's identity on the device | `layouts.rs:75-84`, `vt-roto/src/store.rs:123` |
| knob order on each page | `ParamHash::of("{key}/knob/{n}")` | `store.rs:100` |
| knob names, steps and feel | the setup's `version` hashes every byte written. A change means a rewrite, not a new identity, but stays identical in a behaviour-unchanged PR | `store.rs:144-147` |

The user's "no backwards compatibility" rule means no migration code gets written. It does not
let a cleanup change the format: the user's own songs must keep loading. Prior art keeps a
version and a remap function per device for the day a format changes:
- Surge's `handleStreamingMismatches` (`surge/src/common/dsp/Effect.h:102`);
- sst-effects' `streamingVersion` and `remapParametersForStreamingVersion`
  (`surge/libs/sst/sst-effects/include/sst/effects/Bonsai.h:821, 942`).

This design leaves room for one (`EffectSpec.version`) and adds none.

## 3. Prior art

Read from the local sources in `/Users/gabrielgellner/dev/dsp`, except where marked
**(memory)**.

| Project | How a parameter is described | How devices are dispatched | Places a new device needs | What to take |
|---|---|---|---|---|
| **Vital** | One flat row per parameter: `ValueDetails { name (stable key), version_added, min, max, default_value, post_offset, display_multiply, value_scale (kIndexed/kLinear/kQuadratic…), display_units, display_name, string_lookup }` (`vital/src/common/synth_parameters.h:26-53`), rows from `synth_parameters.cpp:58`, e.g. `delay_feedback … "%", "Delay Feedback"` | A fixed `enum Effect` (`synth_constants.h:100-110`), a factory switch (`reorderable_effect_chain.cpp:83-104`), preallocated and virtual, controls wired by name at build time (`synth_module.cpp:25-50`) | About 8: the enum, rows, module, factory, `kEffectOrder`, destination names, UI section, load_save migrations | A const row per param keyed by a stable string, with scale and units in the row. Our `Param` plus `Kind` is the same shape |
| **Surge** (legacy) | `Parameter` (`Parameter.h:386`) with a `ctrltype` (`:50`) that `set_type` (`Parameter.cpp:587`) turns into range and display: reusable "kinds" | `enum fx_type` with three parallel name arrays (`SurgeStorage.h:422-501`), `spawn_effect` switch with `new` (`dsp/Effect.cpp:61-68`), an `Effect` base class with `init_ctrltypes`, `group_label`, `get_ringout_decay`, `process` (`Effect.h:56-90`) | Enum, three name arrays, factory, class | `ctrltype` is our `Units`: keep units reusable but declared per param. `get_ringout_decay` is our `quiet_frames` and `group_label` our grid columns. The three parallel name arrays are what #736's `Insert::name` and `Effect::name` repeat |
| **Surge sst-effects** (newer) | Per effect: `static constexpr streamingName{"bonsai"}`, `numParams`, `streamingVersion`, and `paramAt(idx)` building `ParamMetaData().withName().withRange().withDefault().withUnorderedMapFormatting(..)` (`libs/sst/sst-effects/include/sst/effects/Bonsai.h:821-890`); `groupName` "like VST3, AU and CLAP" (`sst-basic-blocks/.../ParamMetadata.h:144`) | Templates, static | One class | The best match: a stable streaming name and a const param list per device. OB-Xf uses the same builder (`OB-Xf/src/parameter/ParameterList.h:86-100`) |
| **Deluge** | Dense `enum` ids by kind (`modulation/params/param.h:42-218`). Display names (`param.cpp:201-215`), file names (`paramNameForFileConst`, `param.cpp:418-657`), defaults (`global_effectable.cpp:62-80`) and ranges (menu classes, `gui/menu_item/param.h:34`) are four separate places | Enums and switches; `ModFXType` (`definitions_cxx.hpp:410-420`); tails per type in a switch (`mod_controllable_audio.cpp:1496-1515`) | About 5 per param | Save keys separate from enum order, which our `SettingId` keys already do. The scattering is what to avoid |
| **tracker-ce** core | Parallel arrays: `enFxType` (`source/mtSequencer.h:124-158`), `fxNames` and `fxIDs` (`interface/core/interfaceDefs.h:364-416`), and switches for `getFxMax`, `Min` and `Default` (`mtSequencer_patternEdit.cpp:1745-1812`). Instruments saved as a raw struct (`FileManager/fileManagerDefs.h:19-29`) | Switches | About 8 in 5 files | The warning case for parallel arrays and layout-as-format |
| **tracker-ce** FX SDK | `PtfxParamDef { label[8], min, max, default, step, unit[6], display_type 'f'/'d'/'l' }`, 12 per plugin (`editor-fx-sdk/tracker_plugin.h:33-61`), with `PTFX_PARAM_LIST("Tail", 0, "Ring Out\|Cut Off")` helpers | Function pointers | One file | Almost exactly the widget row of §2.5: label, range, default, step, unit, display type, capped at the encoder count |
| **picoTracker** | `Variable(FourCC, int/float/bool/list)` (`Foundation/Variables/Variable.h:20-30`); "Runtime IDs are local to the firmware; variables are saved by name" (`Foundation/Types/Types.h:86-88`). Range and format are given at each view (`InstrumentView.cpp:410`: `"%2.2X", 0, 0xFF, 1, 0x10`) | Virtual `I_Instrument` over static pools (`InstrumentBank.h:48`) | Variable once, range per view | Our `SettingId` has the same split, a runtime index plus a saved key. Ranges per view are what `editing.rs`, `devices.rs` and `edits.rs` repeat for mod fields (#726), so put `jump` and bounds in the description |
| **Mutable Plaits** | No metadata: every engine gets the same `EngineParameters { trigger, note, timbre, morph, harmonics, accent }` (`plaits/dsp/engine/engine.h:52-59`) | `class Engine { Init, Reset, LoadUserData, Render(.., bool* already_enveloped) }` (`engine.h:78-91`), by-value engines in `Voice`, a registry with per-engine gains (`voice.cc:40-65`) | Class plus a registration line | Our `Synth` enum is this registry made static. Per-engine facts such as `already_enveloped` and `out_gain` go on the trait as consts, which is what `DRIFTS` and `NOISE` are below |
| **Mutable Clouds** | `struct Parameters { position, size, pitch, density, texture, dry_wet, stereo_spread, feedback, reverb; freeze, trigger, gate; granular{}, spectral{} }` (`clouds/dsp/parameters.h:36-61`); modes reinterpret the same knobs (`granular_processor.cc:94-104`) | A switch on mode | | A mode re-reading fixed knobs is our `Shown::ByType` |
| **nih-plug** (memory) | `#[derive(Params)]` with `#[id = "gain"]` stable ids, `FloatParam::new(name, default, FloatRange::Skewed{..}).with_unit(" dB").with_step_size(..).with_value_to_string(..)`, `EnumParam`, nested groups | `Plugin` trait: `initialize`, `reset`, `process -> ProcessStatus::{Normal, Tail(n), KeepAlive}`, latency via context | | Option C. The stable `#[id]` is our key; `Tail(n)` and latency are our `quiet_frames` and `latency` |
| **CLAP / VST3** (memory) | CLAP `clap_param_info { id, flags (STEPPED, IS_ENUM, AUTOMATABLE, MODULATABLE, BYPASS…), name, module (path for grouping), min, max, default }`, `value_to_text`. VST3 `ParameterInfo { id, title, shortTitle, units, stepCount, defaultNormalizedValue, unitId, flags }` | | | `shortTitle` is our `label`; `stepCount` and `IS_ENUM` are our `Kind::Stepped`; `module` and `unitId` grouping are our grid and pages |
| **VCV Rack** (memory) | `configParam(id, min, max, default, label, unit, displayBase, displayMultiplier, displayOffset)`, `configSwitch(.., {"A","B"})`, `ParamQuantity` subclassed for custom text | `Module::process(args)` per sample, params saved by index | | `displayBase` and `displayMultiplier` are our `Units`; saving by index is what we don't do |
| **Elektron** (memory) | A machine chooses which parameters appear on fixed pages (SRC/FLTR/AMP/FX/MOD) of 6 to 8 data-entry encoders; p-locks address a (page, slot) | | | Our Sound/FX/Mod pages and the Roto's 8-knob pages. We lock by stable key rather than slot, which is better: a machine change doesn't re-aim locks |
| **Dirtywave M8** (memory) | Instrument types (Wavsynth, Macrosynth, Sampler, FM, Hypersynth, MIDI, External) reinterpret an instrument's FX command columns; three-letter commands with two hex digits | | | Our `FxCommand::Set(SettingId)` with labels from the registry is already this, description-driven |

**What they agree on:**
1. One const row per parameter, carrying its stable key, display name, short label, range,
   scale, units, default, step and choices.
2. A device-level stable name.
3. Grouping as data.
4. Static or preallocated dispatch.
5. Saving by name, never by enum order or struct layout.

The design above is that. vimtracker already did points 4 and 5 well. Point 1 is three types
today and should be one, and point 2 is split between `Insert::name`, `Effect::key` and the Roto
`kind`.

## 4. Voices, generators and mod types

### 4.1 Machines and voices

**The description:**

```rust
pub struct VoiceSpec {
    pub key: &'static str,        // voice_key (settings.rs:1212): "va", "6op": never changes
    pub name: &'static str,       // SynthVoice::name
    pub meaning: &'static str,
    pub params: &'static [Param], // voice_entries (settings.rs:1159): already Entry tables
    pub holds: bool,              // song.rs:1527
    pub braids: bool,             // is_macro, song.rs:1576
    pub type_key: Option<&'static str>,                    // Machine::type_param's key
    pub grid: Option<&'static [&'static [&'static str]]>,  // Machine::grid's hand placements
    pub role: Role,               // Machine::role
    pub gate: u8,                 // Machine::gate_length (Acid's ACID_GATE)
}
pub const VOICES: [VoiceSpec; SynthVoice::ALL.len()] = [ /* in SynthVoice order */ ];
impl SynthVoice { #[inline] pub const fn spec(self) -> &'static VoiceSpec { &VOICES[self as usize] } }
```

`KINDS` (the picker's grouping, decision 265) stays hand-written as the list order. Its `kind()`
lookup is fine as is.

**The engine** gets a trait with defaults, and one list makes `Synth`:

```rust
pub(crate) trait SynthEngine: Copy {
    fn new(rate: u32) -> Self;
    fn next(&mut self, frequency: f32, settings: Settings) -> f32;
    /// Both sides: the same on each unless it spreads them, as Supersaw does.
    #[inline] fn next_sides(&mut self, f: f32, s: Settings) -> [f32; 2] { let x = self.next(f, s); [x, x] }
    fn release(&mut self) {}                    // 6-op, 4-op, Acid
    fn settled(&self) -> bool { true }          // Acid's accent sweep
    fn trigger(&mut self, _accent: bool) {}     // Acid
    const DRIFTS: bool = false;                 // VA, VA VCF, Acid, Chiptune, Supersaw
    fn seed_drift(&mut self, _seed: u32, _s: Settings) {}
    fn time(&self, _s: Settings) -> f32 { 1.0 }
    const NOISE: bool = false;                  // the 14 noise voices, listed once
    fn seed_noise(&mut self, _seed: u32) {}
}
synths! { VirtualAnalog(VirtualAnalog), VirtualAnalogVcf(VirtualAnalogVcf), Acid(Acid),
          Supersaw(Supersaw), Formant(Grain), Dust(ParticleNoise), /* .. 49 */ }
// Generates `enum Synth` (Copy, inline in each Voice: no allocation per note), `Synth::new`
// by SynthVoice, and the forwarding of every method above.
```

This replaces `voice.rs`'s 13 matches, three of them 49 arms long (`voice.rs:135-362`). #729 then
needs nothing of its own:
- the unreachable Supersaw arm (`voice.rs:201-205`) goes, because `next_sides` is Supersaw's own
  impl;
- the 14-voice noise list written twice (`voice.rs:474-532`) becomes `NOISE`;
- the stray doc line (`voice.rs:473`) goes with it.

`Settings` stays the normalised array it is (`dsp/voices.rs`), which is Plaits'
`EngineParameters` idea with room for more.

**The Roto.** `machine_layout`'s 49 arms (`layouts.rs:2137-2192`) become an array indexed by
`SynthVoice as usize`. The `kind` strings stay hand-written: several are not `"machine/" + key`:

| `kind` | Voice key |
|---|---|
| `machine/va-vcf` | `vcf` |
| `machine/6-op` | `6op` |
| `machine/chip` | `chiptune` |

### 4.2 Generators

Generators differ in a way that matters. Their settings:
- go past a byte;
- can be signed;
- have meanings and ranges that depend on the column (`Context`, `params.rs:65-81`).

So their description has a fixed part and a resolved part.

**The fixed part,** one row per `GenParam`:

```rust
pub struct GenParamSpec {
    pub key: &'static str,     // what macro mappings save (macros.rs:82): never changes
    pub name: &'static str,
    pub units: Units,          // the gen Units (params.rs:249)
    pub typing: Typing,        // params.rs:226
    pub bounds: Bounds,        // Fixed(lo, hi) | Column (From/To/Pitch) | Open (Hits, Rotate) | Listed
    pub nudge: i32,            // replaces most of `step` (params.rs:1683-1955): #724
    pub jump: i32,
    pub engine: bool,          // held_in_engine (params.rs:849)
    pub app_only: bool,        // never on the Roto (layouts.rs:2286-2307 today)
    pub get: fn(&Shape, &Context) -> Option<i32>,   // `number` (params.rs:2509)
    pub set: fn(&mut Shape, &Context, i32),         // `set_number` (params.rs:2636), clamped once
}
pub const GEN_PARAMS: [GenParamSpec; GenParam::ALL.len()] = [
    GenParamSpec { key: "step", name: "Step", units: Units::Step, typing: Typing::Whole { signed: false },
                   bounds: Bounds::Fixed(1, MAX_NOTE_STEP as i32), nudge: 1, jump: 4,
                   engine: true, app_only: false,
                   get: |s, _| Some(i32::from(s.walk.step)),
                   set: |s, _, n| s.walk.step = n as u8 },
    // ..
];
```

Non-capturing closures coerce to `fn` pointers in a `const` table. They are called on the edit
thread, and from the engine's `GenHolds::heard` (`gens.rs:128-139`) on each row read, which
already runs `set_number`.

**The resolved part stays hand-written,** because it's real per-context logic:
- `exists_for` (`params.rs:888`);
- `shown` (`:1346`);
- `reset` (`:1959`);
- `typed` (`:2126`);
- `choices` (`:1623`).

**`FillSpec`,** one row per `FillKind`:

```rust
pub struct FillSpec {
    pub name: &'static str,         // shape.rs:282
    pub description: &'static str,  // shape.rs:311
    pub reworks: bool, pub on_bits: bool, pub has_where: bool,
    /// The Gen page's columns (params.rs:1043-1188's per-fill part), contextual rows aside.
    pub grid: &'static [&'static [GenParam]],
}
```

That collapses 8 of `GenParam`'s 13 matches (`key`, `name`, `units`, `held_in_engine`, most of
`step`, most of `bounds`, `number`, `set_number`), about 975 lines, into a table of about 400.

The widget gets a `ParamView` from `spec` plus `Context`: the resolved bounds, its `shown` text
and its choices.

**Test hygiene for the struct-literal ripple:** tests build shapes as
`Shape { tonnetz: .., ..Shape::default() }`. Then a new fill doesn't touch 20 test files.

### 4.3 Mod types

Mod types have the most to gain. The plan has three parts.

**Move `ModField` into vt-model.** It is the model's fact of which settings a type has. Today it
lives in the TUI (`track_page/controls.rs:15-62`).

**Give each type a spec:**

```rust
pub struct ModSpec {
    pub name: &'static str, pub meaning: &'static str,   // modulation.rs:47-71
    pub envelope: bool, pub has_mode: bool, pub has_polarity: bool,
    pub mode_label: &'static str, pub polarity: Polarity,
    /// Its own settings, in the page's order, each a Param and where it sits.
    pub fields: &'static [ModFieldSpec],
    pub grid: Grid,   // track_page/modulation.rs:131 `type_grid` today
}
pub struct ModFieldSpec { pub field: ModField, pub param: Param, pub own: Option<u8> }
```

**Give `ModSlot` one accessor pair,** `field(f) -> Option<u16>` and `set_field(f, value)`.
These are the only place that knows `adsr.attack_ms` from `ahd.attack_ms` from `lpg.attack_ms`.
They generalise `own_value` and `set_own` (`modulation.rs:365-440`), whose indices get names,
since the `MOD_OWN` order is written 4 times today (#726).

Then the 8 `ModField` dispatches and the 6 tuples become reads of `ModSpec` and a `ParamView`.
A new type is:
- its variant and `ALL` entry;
- a spec row and its fields' rows;
- its own envelope or LFO code;
- a Roto layout.

That's about 12 places.

### 4.4 One description type or parallel ones?

**Share the parameter, keep the device specs parallel, and share no behaviour trait.**

**Shared: `Param`** (with `Kind` and `Units`). Effects, voices, the track's settings, mod own
settings, the sampler, macros, the accumulator and the arpeggio all already are, or become, an
`Entry`. The registry (`SettingId::entry`, `settings.rs:2622`) then covers every family without
`param_entry`'s conversion.

**Not shared: generators keep `GenParamSpec`.** Their values are `i32`, contextual and wider
than a byte, so forcing them into `Param` would mean `Context` everywhere. They meet the others
at `ParamView`, the widget's type, which is the level where sharing pays.

**Shared: a `Listed` trait** for the name/meaning lists that pickers, `K` and the Roto build:

```rust
pub trait Listed: Copy + PartialEq + 'static {
    const ALL: &'static [Self];
    fn name(self) -> &'static str;
    fn meaning(self) -> &'static str { "" }
    fn category(self) -> &'static str { "" }
}
```

It has about 20 users: `Insert`, `Machine`, `ModKind`, `FillKind`, `FilterType`, `Role`,
`Speed`, `ChorusType`, `CompressorType`, the LFO shapes and others. #726's
`choices_of::<X>()` is `X::ALL.iter().map(name)` over it. That replaces about 20
`list(X::ALL…)` in `roto/devices.rs`, about 25 `pick` in `roto/edits.rs` and about 15 picker
builders in `pickers.rs`.

**Parallel: `EffectSpec`, `VoiceSpec`, `FillSpec` and `ModSpec`.** They use the same field names
(`key`, `name`, `meaning`, `params` or `fields`, `grid`) so they read alike. A shared
`DeviceSpec<P>` generic would have one generic user, the picker, and `Listed` already serves it.

**No `trait Device` across families.**
- `SlotEffect::process(input, frame) -> [f32; 2]`, `SynthEngine::next(frequency, settings) -> f32`,
  a generator's pure `render` and `TrackState`'s mod run have nothing in common to abstract.
- Each family's trait has 20 to 49 implementors of its own, which clears #319's "several users"
  rule by a wide margin. A cross-family trait would have one family using each method.

## 5. What stays hand-written on purpose

- **Roto layouts: knob order, pages, buttons and gaps** (#419; `layouts.rs:1-37`;
  `roto-control.md` §4.3). They're the author's musical decisions: "the acid mapping is
  perfect", Mix and Gain on knobs 7 and 8, a button under its knob, the app builds and the Roto
  plays. They name settings by the descriptions' keys, which is the right coupling. Only the
  **lookups** get derived: `effect_layout`, `machine_layout` and `generator_layout` as arrays by
  kind, `ALL` built from them, and `resolve`'s and `app_only`'s lists read from `GenParamSpec`.
- **The `kind` strings,** such as `fx/comp-glue` and `machine/6-op`. They're hashed into the
  device's `PluginHash`, and they don't follow the model's keys (§4.1), so deriving them would
  change hashes. Pin the whole list in a test (P0).
- **The FX page's and Sound page's grids** for the effects and voices that have one, as data on
  the spec, by key: Comp, EQ, Delay, Spring, Stutter, Exciter, Rotary, Clouds, Beads
  (`insert.rs:868-952`), and Sampler, Acid, Supersaw, 4-op (`song.rs:1028-1063`). These are
  screen layout decisions, as the Roto's are for the hardware.
- **Each effect's and voice's DSP,** each generator's render, and the slot drawings: the kitty
  curves, tails and meters. These are real per-device code. What goes is the dispatch around
  them: `kitty.rs:838-883`, `zoom.rs:302-316` and `views/track.rs:683-685` read one
  `drawing_of(kind)` table in vt-tui.
- **Mode-dependent and context-dependent logic:**
  - Distort's and Comp's mode subsets as data (`Shown::ByType`);
  - `GenParam::exists_for`, `shown`, `reset` and `typed`;
  - the sidechain-reading effects' process paths (Comp, LPG, Warps:
    `distort.rs:997-1046`).
- **`SynthVoice::KINDS`,** the picker's grouping and order (decision 265).

## 6. Migration plan

**Rules for every PR:**
- #319's rules: one area per PR, behaviour unchanged, an abstraction only where several users
  exist.
- Effect files wait for #736 to merge, per #718's schedule constraint. Space Echo then converts
  with everything else.
- Engine PRs go in a gap between #187 perf PRs.
- vt-tui PRs wait for the #320 split.

**"Pins" means:**
- the goldens (`vt-model/tests/golden/renders.txt`, the engine hashes) and the screenshots;
- P0's new lists of `SettingId` (index, key), `Insert`/`EffectKind` names, `GenParam` keys, Roto
  `kind` with `PluginHash` and setup `version`;
- `cargo test --workspace --release` for any engine PR (`rt_guard`).

| # | PR | Size | Crates | Behaviour | Unblocks | Relation to #320 and #719–#733 |
|---|---|---|---|---|---|---|
| **P0a** | `test: pin what must not move (model)`: golden lists of every `SettingId`'s index and key, `Insert::ALL` names, `GenParam` keys, `FillKind` and `ModKind` names | S | vt-model tests | Identical; tests only | Every later model PR is checked mechanically | New. Any time; no conflicts |
| **P0b** | `test: pin the Roto's identities`: every layout's `kind`, its `PluginHash`, each stored setup's `version` (`stored_setups()`) | S | vt-tui tests (a new file) | Identical | E6, V3, G3 | New. A new test file; land it **before** the split PR opens or right after it merges (NOT DURING SPLIT) |
| **E1** | `refactor(insert): one EffectSpec per effect`. `settings::Effect` becomes `EffectKind`; `Insert::kind()`; `EFFECTS` table (key, name, meaning, params); `effect_start` and `SLOT_SIZE` as const loops; `Param` gains `kind` (units) and `place`; `param_entry`'s table goes; `Insert::read` collapses | M | vt-model, with one-line call-site renames in vt-engine `track_state.rs` and vt-tui `controls.rs`, `dsp_probe.rs`, `guides.rs` | Identical. The units each param states equal what `param_entry` gave it today, so Echo stays "wrong" until fixed on purpose, which is a separate `fix:` | E2, E3, E4's `EffectKind` index, V1's pattern | **Replaces #720(a)** in full, and goes further by making `Effect` the table index. The vt-tui renames are mechanical: do them before the split or rebase them into it |
| **E2** | `refactor(insert): each effect's settings written once`, the `effect_settings!` macro; `highest`/`span` from `Kind::top`, settling the `Gain` difference | M | vt-model `insert/` | Identical, unless `span`'s missing `Gain` arm (`insert.rs:277-310`) proves a bug. Then it's a stated one-line `fix:`, as #720 says | E3 | **Replaces #720(b)** |
| **E3** | `refactor(insert): retire Setting`. Modes as `Shown::ByType` index lists; grid placement by key; `type_param`, `title_param` and output from `Place` | M | vt-model, plus `track_state.rs:2056` and `controls.rs:365` call sites | Identical | Removes about 250 lines of `insert.rs:61-310`; `track_state`'s search becomes an index | New; optional but recommended. The vt-tui call site waits for the split, or keeps a shim `Insert::kept_at(at)` until then |
| **E4** | `refactor(effects): the slot in slot.rs, SlotEffect, and the pool by kind`. `InsertState`, `Mono` and dispatch move to `dsp/effects/slot.rs`; the `SlotEffect` trait on the 20 buffered effects; `memories!`; `MemoryCount` and `MemoryPool` as arrays; `meet_memory_needs` a loop; the unreachable `process_alone` arms go (`memory.rs:180, 186-189`); `Keys` moves beside the slot | L, or two Ms (move first, then trait) | vt-engine `dsp/effects/`, `link.rs`, one import in `track_state.rs` | Identical: goldens plus release `rt_guard`. Only the order of freeing changes, which isn't heard | E5; #723 | **Subsumes #721** in full. Not alongside a #187 perf PR |
| **E5** | `refactor(effects): the slot's per-effect facts from the trait`. `MONO`, `MIXING`, `same_effect` by kind and type; the 7 inline effects implement `SlotEffect`; `follow_fresh`; the 10 wet/dry lines into one `mixed` helper | M | vt-engine `slot.rs` | Identical | A new effect needs no `slot.rs` edit | Completes #721's `follow_fresh`/`mixed` |
| **V1** | `refactor(machine): one VoiceSpec per voice`. `VOICES` table; `SynthVoice::name`, `meaning`, `holds`, `is_macro`, `voice_key`, `voice_entries`, `Machine::type_param`, `grid`, `role`, `gate_length` read it; `Machine`, `Role` and `SynthVoice` move to `machine.rs` | M | vt-model `song.rs`, `settings.rs` | Identical | V2, V3 | **Takes #725's `song.rs` Machine part** and the voice-table part of its `settings.rs` split. After #736 merges (`settings.rs`) |
| **V2** | `refactor(voice): SynthEngine and synths!` | M | vt-engine `voice.rs`, each voice's `impl` | Identical: goldens plus release `rt_guard` | A new voice is one list line plus an impl | **Subsumes #729** |
| **G1** | `refactor(shape): step from bounds, grid returns Grid` | S-M | vt-model `shape/params.rs` | Identical | G2 | **Is #724's model half** |
| **G2** | `refactor(shape): one GenParamSpec per setting and FillSpec per fill`. The table with `get`/`set`; 8 of 13 matches go; `FillKind`'s facts; test literals switched to `..Shape::default()` | M | vt-model `shape/`; test files across crates for the literals, so do the vt-tui test edits before or after the split, or leave vt-tui's literals for G3 | Identical: `params.rs`' tests, `render.rs`, Gen page tests and screenshots | G3; generators-into-engine work (decision 300) reads `engine` and `get`/`set` | Extends #724. Generators are an active area (23 `feat(shape)` commits), so schedule it in a gap |
| **M1** | `refactor(modulation): ModField and ModSpec in the model`. `ModSlot::field` and `set_field`; named `MOD_OWN` indices; `Track::control(TrackControl)` and `set_control` | M | vt-model `modulation.rs`, `song.rs`; vt-tui imports only (re-export `ModField` from its old path) | Identical | M2 | **Is #726's model half** ("that half can land now") |
| **S1** | `refactor: Listed`, the one trait the lists are built from; `choices_of::<X>()` | S | vt-model; its vt-tui users switch in M2 and E6 | Identical | M2, E6, the UI study's pickers | **Is #726's helper**, generalised |
| **M2** | `refactor(track page): mod fields and track controls from their descriptions`. The 8 `ModField` dispatches and 4 `TrackControl` matches read `ModSpec` and the model accessors; `MACRO_LABELS` goes | M | vt-tui `track_page/`, `roto/` | Identical: track, mod and Roto tests | S2 | **Is #726's tui half. WAITS FOR SPLIT** |
| **E6, V3, G3** | `refactor(roto): look up layouts by kind`. `effect_layout`, `machine_layout` and `generator_layout` as arrays; `ALL` built; `app_only` and `resolve` from `GenParamSpec`; page names from the model where they equal it; plus `drawing_of(kind)` for `kitty.rs`, `zoom.rs` and `track.rs` | S-M | vt-tui `roto/`, `kitty.rs`, `views/track*` | Identical: P0b pins, `tests/roto_layouts.rs`, the PNG CRCs | | **Is #731**. The drawing table goes with **#728** if that lands first. NOT DURING SPLIT |
| **S2** | `refactor(track page): Control::view -> ParamView`. One resolution per `Control`; `value`, `name_on`, `label_on`, step, type, `K`, and the Roto's `feel`, `pick` and `key` read it | M-L | vt-tui | Identical: screenshots, app and Roto tests | The UI study's parameter widgets | New. After M2, and after the split has decided where pages live. **Precedes #733** (pub(crate) and grouping) |

### 6.1 Order

1. **Now, outside the effect files:** P0a, G1, M1, S1. #719 (one test binary per crate) is
   independent and worth doing first, since every PR here runs the tests many times.
2. **When #736 merges:** E1, then E2, then E3. Then E4 and E5 in a #187 gap, with **#723**
   (`track_state.rs` split) after E4 so `Keys` moves once, as #718 says. V1 and V2 can
   interleave. Run one model PR and one engine PR at a time, never two on the same files.
3. **Just before the split PR opens, or just after it merges:** P0b, and E1's few vt-tui renames.
4. **After the split:** M2, then E6/V3/G3 (#731), then S2, then #733.

### 6.2 The #719–#733 items this changes

| Item | Effect of this plan |
|---|---|
| #719 tests | Unchanged. Do first |
| #720 | Replaced by E1 and E2 (E3 goes further) |
| #721 | Replaced by E4 and E5 |
| #722 docs | Unchanged. The Architecture section's "A new setting needs no new interface" paragraph (`vimtracker.md:289-292`) and the DSP layout table (`:700`, `distort` "also `InsertState`") get updated by E1 and E4, as CONTRIBUTING requires for a structure change |
| #723 | Unchanged; after E4 |
| #724 | Becomes G1, extended by G2 and G3 |
| #725 | Shrinks: `song.rs`'s Machine part is V1; `settings.rs`'s `Effect` part disappears in E1; `units.rs` and `id.rs` remain |
| #726 | Becomes M1, S1 and M2 |
| #727 presets | Unchanged. Presets already need nothing per device |
| #728 | Takes E6's drawing table if it lands first |
| #729 | Replaced by V2 |
| #730, #732 | Unchanged |
| #731 | Becomes E6, V3 and G3 |
| #733 | After S2 |

**Decisions to append to `decisions.md`:**
- E1: one description type for parameters, `Param` with `Kind` and `Units`, with units per
  parameter;
- E4: effects dispatch statically through `SlotEffect` and `memories!`, never `dyn`;
- G2: generator settings are a table with accessors.

### 6.3 What a new device costs afterwards

- **An effect (about 7 places, in 5 files):**
  - vt-model: the `Insert` variant, which is the save format; its `effect_settings!` block, its
    own data; one `EFFECTS` row; the `EffectKind` variant;
  - vt-engine: `impl SlotEffect` on the DSP, its own code; one `memories!` line if it has a
    buffer;
  - vt-tui: its Roto layout, which is meant to be hand-written, plus one table entry; a
    `drawing_of` arm only if it draws.
- **A voice (about 8 places):** the `SynthVoice` variant and `ALL`; a `KINDS` row; a `VOICES`
  row with its `Param` table; one `synths!` line and `impl SynthEngine`; the Roto layout and
  table entry.
- **A generator (about 20 places):** the variant and `ALL`; a `FillSpec` row; one
  `GEN_PARAMS` row per new setting; its `exists_for` and `shown` cases; the `Shape` field; a
  render arm; the Roto layout and table entry.
- **A mod type (about 12 places):** the variant and `ALL`; a `ModSpec` row with its field rows;
  the `ModSlot` field and its `field`/`set_field` arms; its envelope or LFO code; the Roto
  layout and table entry.

## 7. Other things made "lots of"

The parallel UI-components study covers widgets, settings rows, grids, pickers, popups and
panels. This is only where they meet the descriptions.

- **Settings rows and parameter cells** should draw a `ParamView` (§2.5) in a cell placed by the
  device's `Grid`. `Control::view` (S2) is the one function between `Control` and the widgets.
- **Pickers.** There are 42 `open_*_picker` functions (`grep -c` across vt-tui). The lists of
  kinds (machines, effects, mod types, fills, filter types, roles and so on) should come from
  `Listed` (S1), with category headings from `category()`. Pickers over song data, such as
  samples, phrases and kits, stay as they are.
- **Roto lists:** `choices_of::<X>()` over `Listed` (S1), as above.
- **Commands.** FX commands are already the registry's (`FxCommand::Set(SettingId)`,
  `step.rs:66-90`), with labels from `Entry.label`, so a new setting needs no new command. Keys
  are a binding table (`input/bindings.rs:27`). Nothing to do.
- **Views per page** are the split's (#320) and the UI study's. The image side is #728.
- **Test helpers:** `..Shape::default()` (G2), and #719's shared `app`, `song` and `render`
  helpers.

## 8. Risks

- **`SettingId` order.** If any `const` loop numbers effects in a different order than
  `effect_start`'s, saved songs reorder their `settings` maps and the goldens fail. That's the
  intended alarm. P0a pins the order before E1.
- **Serde.** `effect_settings!` must re-emit `#[serde(default)]`, `default = "…"` and
  `skip_serializing_if` exactly (`insert/colour.rs:172-190`, `insert/space.rs:101-104`). The
  presets and patches tests (`tests/presets.rs`, `tests/patches.rs`) and the tutorial songs
  catch any slip.
- **Macro opacity.** Keep both macros small and documented in place, with one worked example in
  each doc comment. If either needs more than the escape hatch above (Distort's `bits`),
  hand-write that one effect rather than growing the macro.
- **Real time.** Nothing new runs in the callback except array indexing. The pool's `free` array
  is filled with `with_capacity` on the edit thread (`for_song`). `cargo test --release` runs
  `rt_guard` over every engine PR.
- **Schedule.** E1, E2 and V1 touch `settings.rs`, which every feature PR adding an effect or
  voice also touches. Announce a short freeze of new devices while E1 to E4 land, a few days,
  which fits "no new features, only cleanup".
