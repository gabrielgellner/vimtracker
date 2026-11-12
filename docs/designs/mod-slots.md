# Mod slots: six typed modulators per track

**Status:** a design study for #422, step 1 of milestone M20, "Modulation slots". It covers
#422 and the milestone's other issues, #235 (Steps), #86 (LFO fade and one-shot), #85 (envelope
hold), #46 (stage curves) and #45 (LPG mode). The author's answers to §12 are at the end, as
**Decided**. PR 1 of §11 (#427, decision 165) built the slots with ADSR and LFO; the rest
follows.

**The author's decision (2026-11-11):** replace the fixed modulators (3 envelopes and 3 LFOs,
decision 138) with **6 typed mod slots per track**, as the M8 has. Each slot takes a type, in any
combination. The defaults match today's: 3 ADSRs, the first a volume envelope, then 3 LFOs.

**It builds on:**

* the Mod page as built (decisions 151, 152 and 154);
* [`track-pages.md`](track-pages.md) §1, the device standard, and its Decided note 4: modes go in
  titles, and the page scrolls rather than shrinking its panels;
* the in-flight Roto layouts (#419, decision 161 on the `roto-layouts` branch);
* the code: `vt-model`'s `lfo.rs`, `envelope.rs`, `settings.rs` and `target.rs`; `vt-engine`'s
  `track_state.rs` (`modulate`) and `engine.rs` (`note_shape`, `start_modulation`); and
  `vt-tui`'s `track_page/modulation.rs`, `views/track/modulation.rs` and `kitty/plot.rs`.

**The recommendation in one paragraph.**

* **The track:** it has `mods: [ModSlot; 6]` in place of `lfos` and `envelopes`. A slot is a fixed,
  `Copy` struct. Its **Type**, **Dest**, **Depth** and **switch** are common to every type. It also
  keeps each type's own settings, so switching a slot to another type and back finds the first as
  it was, as a track keeps every voice's settings.
* **Seven types:** ADSR, AHD, Drum, LPG, LFO, Steps and Track.
  * Hold (#85) is AHD's.
  * Curve (#46) is the envelope types' *mode*, in the title.
  * LPG (#45) is a type of its own.
  * The LFO's trigger modes (#86) are its mode, and Fade takes the cell `Retrig` leaves.
  * Steps (#235) is a type, edited in its panel.
* **The engine:** it works the slots out in order, 1 to 6, so each reaches only those after it, as
  today's envelopes reach the LFOs. Nothing is allocated. A type change glides over 5 ms and keeps
  the slot's phase and its place in the note.
* **Locks:** they address slots: `DEP1`–`DEP6` and `PHS1`–`PHS6`, with keys `mod1.depth` and so on.
* **The page:** six half-width devices, `Mod 1`–`Mod 3` on the left and `Mod 4`–`Mod 6` on the
  right, so the default page looks as it does now. Each title is `Mod n`, the switch, the type, its
  mode and `→ Dest`.
* **The song:** the format goes to 9, and a one-off script converts the author's three songs.
* **Accumulator and Arpeggio stay where they are.**

---

## 1. Prior art

| | Slots | Types | Mod of mod | Notes |
|---|---|---|---|---|
| **Dirtywave M8** (manual v6.5.2, "Instrument Modulation View"; firmware 3.0 changelog) | 4 per instrument, `MOD1`–`MOD4` | AHD, ADSR, Drum, LFO, Trig env, Tracking | `DEST` MOD AMT, RATE, BOTH or BINV, on the *next* slot in a ring, 1→2→3→4→1 | It made our move in 3.0.0: "Envelopes and LFOs have been replaced with 4 configurable modulation slots". Each slot has `AMT` and its type's own settings, all hex, times in ticks. FX commands are numbered per slot and their names follow the type (`AT2` is slot 2's attack; an LFO's are `LA LO LS LF LT`). The manual doesn't say what a type change does to a slot's settings. |
| **Elektron Digitakt II** (manual §11.7, §11.9–11.11) | 3 LFOs a track, and the amp and filter envelopes | fixed | LFO 2 and 3 reach those before them | The LFO's `MODE` is `FRE`, `TRG`, `HLD`, `ONE` or `HLF`. `FADE` runs −64 to +63, negative fading in. `SPH` is the start phase. The amp envelope's `MODE` is AHD or ADSR, and its `HOLD` is a time or `NOTE`. |
| **Octatrack** (manual OS1.40A §11.4) | 3 LFOs a track | fixed, plus 8 waves drawn in the 16-step LFO designer | LFO 3 reaches 1 and 2 | The amp envelope is AHR. |
| **Bitwig Studio** (user guide ch. 16) | 3 to start, growing without limit | 43, in 8 families | any modulator reaches any | Any type goes in any slot. The amount is set at each target, with a transfer curve. `Steps` has a count, a direction and the trigger modes Free, Note/Restart and Note/Advance. |
| **Ableton Live 12** (Reference ch. 33) | one device per modulator | LFO, Shaper, Envelope Follower, Envelope MIDI | none | Each device maps to up to 8 targets, each with its own amount. Drift has a 3-slot matrix and Meld a grid. |
| **Vital** (source, `synth_constants.h`, `synth_lfo.h`) | 8 LFOs, 6 envelopes, 4 random | fixed | any | LFO sync modes: Trigger, Sync, Envelope, Sustain Envelope, Loop Point, Loop Hold. Fade-in and delay. DAHDSR envelopes with a power curve per stage (#46's comment). |
| **tracker-ce** (Polyend, `instrumentEditor.h`, `docs/wavetable_envelopes.md`) | one automation per destination | Envelope, LFO, WT Env, WT LFO | none | Its fields are shared across types and read again under the new one: `kVolumeAttack_ShapeLFO` is an envelope's attack or an LFO's shape. |

**What we take:**

* **From the M8:** the slot model, the type list (ADSR, AHD, Drum, Tracking) and one amount per
  slot. We don't take its ring: our order, earlier slots reaching later ones, is decision 138's and
  can't loop.
* **From Elektron:** the LFO's five trigger modes and Fade, and AHD with a `Note` hold.
* **From Vital:** the curve as a power, which our decay already is (#46's comment).
* **From Bitwig:** `Steps`' Free, Restart and Advance triggers.
* **From tracker-ce, as a warning:** its shared fields mean a type change makes nonsense of the
  numbers, so we keep each type's own settings instead.

---

## 2. The types

Every slot has the **common** part, which a type change keeps:

| Setting | Values | Default |
|---|---|---|
| Type | ADSR, AHD, Drum, LPG, LFO, Steps, Track | slots 1–3 ADSR, 4–6 LFO |
| Dest | any setting the slot can reach (§5), or `---` | slot 1 `Vol`, slot 2 `Cut`, the rest `---` |
| Depth | `-FF` to `+FF`, in the destination's units (semitones on `Pitch`) | `00` |
| On/off | decision 152's switch, `b` | on |

Each type then has at most **five settings of its own, plus `Depth`**, which fills a half-width
device's two columns of three. Its mode, if it has one, goes in the title. The defaults are
musical, and those of ADSR and LFO are exactly today's.

### ADSR: today's envelope

* **Mode** (`Curve`): `Exp`, `Lin` or `Snap`. The default is `Exp`.
* **Grid:** `Attack`, `Decay` and `Release` down the left; `Sustain`, `Depth` and `Reset` down the
  right. That's today's grid.
* **Defaults** are today's three: slot 1 is `note_shape`, slot 2 `filter_shape` and slot 3
  `unassigned`.

The **curves**, after #46's comment:

| Curve | Attack | Decay and release |
|---|---|---|
| `Exp` | `t^3.32` | Vital's `powerScale` with `p = -4`: today's Peaks curves |
| `Lin` | straight | `p = 0`: straight |
| `Snap` | `t^3.32` | `p = -8`: about twice as fast to fall, then a longer tail |

It's a mode rather than a cell: it changes the envelope's character, it's set once, and ADSR's six
cells are full. That's the FX page's rule for an effect's `Type`.

### AHD: attack, hold, decay (#85)

* **Mode** (`Curve`): as ADSR's.
* **Grid:** `Attack`, `Hold` and `Decay` down the left; `Depth` and `Reset` down the right, and a
  hole.
* **Defaults:** 1 ms, 30 ms and 200 ms; `Reset` on.

**Hold** is `Note` or from 0 to 2000 ms:

* **A time:** a one-shot. It holds at full, then decays, whatever the note's length. That's a
  drum's punch, the M8's AHD and the Digitakt's AHD amp.
* **`Note`:** it holds while the note does, then decays, which is the Digitakt's `HOLD NOTE`.

The hole is left for a `Delay` before the attack, Vital's DAHD, if one is missed.

### Drum: a peak onto a body (the M8's Drum env)

* **Mode** (`Curve`): as ADSR's. It shapes the body's decay; the peak's fall is fixed and steep.
* **Grid:** `Peak`, `Body` and `Decay` down the left; `Depth` and `Reset` down the right, and a
  hole.
* **Defaults:** 10 ms, 40% and 300 ms.

**The shape:** a fall from full to `Body` over `Peak` ms, then a decay from there to nothing over
`Decay`. It's a one-shot. It's for a kick's or a tom's pitch: the click's sweep, then the note's
drop.

The M8's `PEAK` sets both the dip and its time; splitting them into `Peak` (time) and `Body`
(level) reads more plainly.

### LPG: a vactrol's fall (#45)

* **No mode.** Its curve is the point of it.
* **Grid:** `Attack`, `Decay` and `Colour` down the left; `Depth` and `Reset` down the right, and a
  hole.
* **Defaults:** 1 ms, 400 ms and `C0`.

**The shape:** a one-shot ping after Plaits' `LowPassGateEnvelope`. Its decay slows as it nears
silence. Plaits' coefficients are tuned to one sample rate, so ours are scaled by rate.

**`Colour`** is how far its fall also closes the track's filter, in `Cut`'s units at silence: `00`
moves only the level, and `FF` closes it all the way. So one slot drives level and brightness
together, as #45 asks, whatever its `Dest`.

**What it does to the filter:** the `Cut` part goes on top of the track's `Cut` and any other slot
on `Cut`, through the envelopes' fast smoothing. On a track without a filter, `Colour` does nothing.

**Named `LPG`,** beside the FX slot's `LPG` effect (decision 45). They're in different lists, one
the type picker's and the other the effect picker's, and the status line says "LPG envelope".

### LFO (#86)

* **Mode:**

  | Mode | What it does |
  |---|---|
  | `Free` | runs from the song's position. It's today's `Retrig` off, and the default. |
  | `Trig` | each note starts it at `Phase`. It's today's `Retrig` on. |
  | `Hold` | runs free underneath, and each note holds the value it's at until the next note. It's a sample and hold of the wave. |
  | `One` | each note starts it, and it plays one cycle and then holds where it ended. |
  | `Half` | the same, for half a cycle. |

* **Grid:** `Shape`, `Rate` and `Depth` down the left; `Phase` and `Fade` down the right, and a
  hole. `Fade` fills the hole decision 151 kept for it; `Retrig` is gone into the mode.
* **Defaults:** today's: `Tri`, `1/4`, depth `00` and `0°`. `Fade` is `Off`.

**`Fade`** is `Off` or a signed musical length: the note divisions and bars of `SWEEP_NAMES`, from
`1/64` to `16 bars`, never Elektron's speed and multiplier.

* `+1 bar` fades in over a bar after each note.
* `-1 bar` fades out over a bar after each note.

It counts from the last note in every mode. In `Free` only the fade starts again, not the wave.

**Why the modes replace `Retrig`:** `Retrig` on or off against `Hold`, `One` and `Half` gives
pairs that mean nothing, such as `One` with no note to start it. Elektron's single list of five
has none. LFOs are worked out from the song's position, which `Free` and `Trig` already are. The
other three need two more things:

* the time since the last note, which `Fade` needs too;
* for `Hold`, the wave's value at the note, kept in the slot's running state (§4).

### Steps (#235)

* **Mode:**

  | Mode | What it does |
  |---|---|
  | `Trig` | each note starts it at `Start`. The default. |
  | `Free` | runs from the song's position, as an LFO in `Free` does. |
  | `Note` | moves on one step with each note. It's the M8's `TIC00` and Bitwig's Note/Advance. |

* **Grid:** `Rate`, `Length` and `Depth` down the left; `Start`, `Loop` and `Smooth` down the
  right.
* **The values** are edited in the panel (§7).

| Setting | Values | Default |
|---|---|---|
| Values | 16 of them, `-FF` to `+FF`, in the destination's units at a `Depth` of `+FF` | all `00` |
| Rate | `Tick` (a sixth of the track's step, as `DEL` counts), or a division from `1/64` to `1 bar`. `--` in `Note` mode. | `1/16` |
| Length | 1 to 16 | 8 |
| Start | 1 to 16 | 1 |
| Loop | `Once` (play to the end and hold the last), `from 1` to `from 16` (play once, then loop from that step, the M8's `HOP`), or `Bounce` | `from 1` |
| Smooth | off: steps; on: glides across each step's length | off |

**Its output** is the step's value times `Depth / FF`. At `+FF` the values are heard as written:
`+07` on `Pitch` is seven semitones up. A slot that becomes Steps with a depth of `00` is given
`+FF` in the same undo step. The status line says why.

**Scale degrees** on `Pitch` with the scale on, as `ACU` and `ARP` move, are left for later. They
need the note to move from, which pitch modulation doesn't have.

### Track: the note as a source

* **Mode** (`Source`):

  | Source | `Low` and `High` |
  |---|---|
  | `Note` | notes, `C-1` to `G-9` |
  | `Velocity` | `00` to `7F` |
  | `Random` | a value rolled at each note, `00` to `FF` |

* **Grid:** `Low`, `High` and `Depth` down the left.
* **Defaults:** `Velocity`, `00` and `7F`.
* **The output** is 0 at `Low` and 1 at `High`, clamped, then times `Depth`. `Low` above `High`
  inverts it, as the M8's does.

It takes a value at each note and holds it. `Random` rolls from a stream of the slot's own,
seeded from the project's seed after the others (decision 86's rule). So renders repeat, and
`CHA`'s rolls are as they were.

### Not in M20

* **An envelope follower:** the track's level, or another's as `Comp`'s `Source` hears it. The
  engine has the key it needs (`TrackState::key`), a block late.
* **The M8's trigger envelope:** an AHD started by another track's notes.
* **A separate sample and hold:** an LFO with the `Random` shape in `Hold` mode covers it.

Each of these would be one more type later, with no change to the slot.

---

## 3. Where M20's issues land

| Issue | Lands as |
|---|---|
| #85, envelope hold | the **AHD** type, with `Hold` as a time or `Note` |
| #46, stage curves | the envelope types' **mode**: `Exp` (today's), `Lin`, `Snap`, one curve per envelope rather than one per stage, as #46 suggested trying first |
| #45, LPG mode | the **LPG** type, with `Colour` closing the filter |
| #86, LFO fade and one-shot | the LFO's **mode** (`Free`, `Trig`, `Hold`, `One`, `Half`) and its **`Fade`** cell |
| #235, Steps | the **Steps** type, with its values in its panel |

---

## 4. The model and the engine

### The model (`vt-model/src/modulation.rs`, replacing `lfo.rs`'s `Lfo` and `envelope.rs`'s `Envelope`)

```rust
pub const MOD_SLOTS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModSlot {
    pub kind: ModKind,                     // Adsr, Ahd, Drum, Lpg, Lfo, Steps, Track
    pub destination: Option<SettingId>,
    pub depth: i16,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bypassed: bool,
    // Each type's own settings, kept while another type plays, saved only where they differ
    // from the type's defaults.
    #[serde(default, skip_serializing_if = "is_default")] pub adsr: Adsr,   // A, D, S, R, reset, curve
    #[serde(default, skip_serializing_if = "is_default")] pub ahd: Ahd,     // A, hold (ms or Note), D, reset, curve
    #[serde(default, skip_serializing_if = "is_default")] pub drum: Drum,   // peak, body, decay, reset, curve
    #[serde(default, skip_serializing_if = "is_default")] pub lpg: Lpg,     // attack, decay, colour, reset
    #[serde(default, skip_serializing_if = "is_default")] pub lfo: LfoSettings, // shape, rate, phase, mode, fade
    #[serde(default, skip_serializing_if = "is_default")] pub steps: Steps, // values [i16; 16], rate, length, start, loop, smooth, mode
    #[serde(default, skip_serializing_if = "is_default")] pub track: Tracking, // source, low, high
}
```

* **`Track`:** `mods: [ModSlot; MOD_SLOTS]`, with `default_mods()` giving today's six.
* **Constants:** `LFOS_PER_TRACK` and `TRACK_ENVELOPES` go, and `MOD_SLOTS` is the one count.
* **Actions:** `SetLfo` and `SetEnvelope` become one `SetMod { track, slot, modulator }`, so `b`,
  `K` and every edit is one undo step, as now.
* **Why keep every type's settings,** rather than an enum holding only the type that plays:
  * a type change can be undone and redone anyway, but trying Steps on a slot and going back to
    its LFO should find the LFO as it was. A track keeps every voice's settings for the same
    reason (decision 29).
  * tracker-ce's shared fields show the alternative: numbers that mean something else under the
    new type.
* **Size:** about 120 bytes a slot, Steps' 16 values being 32 of them. That's 720 bytes a track
  against today's 96, so a `Track` grows from 344 bytes to about 970.
* **Undo:** History keeps whole songs, so a song of 15 tracks costs about 9 KB more per undo
  version, and at most about 9 MB more over the 1,000 kept. PR 1 measures it. If it's too much,
  the fallback is an enum holding only the type that plays, which gives up finding a type as it
  was.

### The engine (`track_state.rs`)

**State:** one fixed array, `slots: [SlotRun; MOD_SLOTS]`, replaces `envelopes`, `lfo_starts`,
`lfo_values`, `envelope_shares`, `lfo_shares` and `lfo_now`.

```rust
struct SlotRun {
    seen: ModKind,        // the type it last ran, to notice a change
    envelope: EnvelopeRun,// the envelope types' run, as now
    start: LfoStart,      // where an LFO's or Steps' cycles count from; kept across types
    latched: f32,         // LFO Hold's value at the note; Track's value at the note
    count: u32,           // Steps' notes in Note mode
    value: f32,           // the depth and phase held from steps' locks, as lfo_values is
    phase: f32,
    share: f32,           // decision 152's on/off glide
    out: f32,             // what it added last block
    glide: f32,           // a type change's difference, fading to 0 over 5 ms
    now: Option<ModNow>,  // for LiveValues: an LFO's place and value, Steps' step
}
```

Beside it, the track keeps **the last note's start and release** (clock and tick), which `Fade`,
`One`, `Half` and a type change all need. No type allocates. Steps' values are in the song, and
the engine reads them where they are.

**The order:** `modulate` runs the slots once, 1 to 6, every 32 frames, as now (`LFO_UPDATE_FRAMES`).

* **Reading:** a slot first reads its own depth and phase. That's the held value from a step's lock,
  plus what earlier slots added to them, which is today's `moved_by`.
* **Adding:** it then adds its output to its destination.
* **Two kinds of smoothing,** as today:
  * the "snappy" types (the envelopes, Steps without `Smooth`, and Track) go through the fast
    smoothers (`cut_envelope`, `FAST_SMOOTHING_MS`, today's `moved`);
  * an LFO goes through the slow ones (`cut_offset`, `SMOOTHING_MS`).
* **Bookkeeping:** `Moved` and `offset_marks` are sized by `MOD_SLOTS`.
* **Same results:** with the default types, slots 1–3 run before 4–6 exactly as envelopes run
  before LFOs now. So every song with the default types renders as it does now.
* **One exception:** a `Random` or `Smooth` LFO's values are keyed by the slot (`seed ^ (track <<
  8 | slot)`), and an LFO's slot is now 4 to 6 rather than 0 to 2. Its random values change once.
  The author's songs use only `Tri`, so they're unchanged.

**The note shape:** the first slot that is an envelope type on `Vol` and switched in is each note's
own shape. That's today's `Env 1` on `Vol` (`Engine::note_shape`), made general.

* **What it can be:** ADSR, AHD, Drum or LPG.
* **The voice:** `Voice::shape: Envelope` becomes a small `Copy` enum with `held` and `released`
  for each.
* **Its depth** shows `--` and does nothing, as now.
* **An AHD with a time hold, a Drum or an LPG** ends the note when its decay ends, whatever the
  gate.
* **Changing it while a note sounds** leaves that note with the shape it started with, as decision
  152 does for switching it out.

### Changing a slot's type

* **Kept:** the slot's `Dest`, `Depth` and switch, and its running state. The new type's own
  settings come back as this slot last had them, or are the type's defaults the first time.
* **The engine sees** `kind != seen` at the next block and picks the new type up where it would be
  if it had always been there, as a switched-out modulator keeps running (decision 152):

  | New type | Where it picks up |
  |---|---|
  | LFO | phase from its kept `start`, so LFO → Steps → LFO stays in phase |
  | Steps | position from its `start` or its `count` |
  | Envelope types | from the sounding note's start, or its release, as though triggered then; from nothing if no note sounds |
  | Track | from the sounding note |

* **Glide:** the old type's last output less the new one's first is kept in `glide`, added on top
  and faded in a straight line to 0 over `BYPASS_FADE_MS`, 5 ms. So a type change never clicks. It
  needs no second type worked out at once.
* **Cost:** one compare per slot per block, and on a change, a handful of stores. There's nothing
  to send but the new song version, as with any edit.

**Cost overall:**

* **Per block:** six slots worked out where six modulators were, through one `match`. A slot with
  no destination, no depth or switched out is skipped before its type's work, as now.
* **The types' own work:**
  * Steps is an index into its values, plus a lerp when `Smooth` is on;
  * LPG is an `exp` a block;
  * Track is a constant a note.
* **Memory:** a `TrackState` grows by under 400 bytes, against its 78 KB.
* **Measuring it:** PR 1 measures as decision 138 did, 16 synth tracks with a filter envelope and
  an LFO each, best of five, in a release build, and expects nothing measurable.

---

## 5. Destinations, depth and on/off

* **Destinations:** a slot can reach `SettingId::to_modulate(track)`, and the depth and phase of
  every slot after it (`SettingId::for_mod(track, n)`, replacing `for_envelope` and `for_lfo`).
  * Today's reach is kept: envelopes reach every LFO (slots 1–3 before 4–6), and LFO *n* reaches
    the LFOs after it.
  * It's widened in one place: an envelope can now reach a later envelope's depth.
  * There's never a way back round, unlike the M8's ring.
* **A destination pointing backwards:** a slot's destination can only point backwards if slots are
  moved (`dd`/`p`, below). The engine then skips it, as it skips a destination not on the track,
  and the title shows it dim with `---` after the slot's name (`M2 Depth ---`).
* **Depth:**
  * it's one per slot, common to every type, signed, in the destination's units;
  * Steps reads it as a scale, `+FF` as written (§2);
  * a lock's `DEPn` moves it whatever the type.
* **On/off:** decision 152 holds per slot, for every type:
  * `b` from anywhere in the device, and the toggle in its title;
  * a 5 ms glide to nothing, and running underneath;
  * no step lock;
  * the note-shape slot, switched out, leaves notes the plain shape.
* **Moving slots:** `yy`, `dd` and `p` copy, cut and paste a whole slot, as on the FX page. Locks
  stay with the slot number, as FX slots' do. This is optional and not in PR 1.

---

## 6. Labels, keys and locks

Today's registry has `Owner::Lfo(n)` with two entries, `depth` and `phase`. It becomes
`Owner::Mod(n)` for `n` in 0..6, with the same two entries:

| What | Today | Mod slots |
|---|---|---|
| Key songs store | `lfo1.depth`, `lfo3.phase` | `mod4.depth`, `mod6.phase` (`mod1`–`mod6`) |
| Grid label | `DEP1`, `PHS3` | `DEP4`, `PHS6` (`DEP1`–`DEP6`, `PHS1`–`PHS6`) |
| An envelope's depth | not lockable | `DEP1`–`DEP3` for the default ADSRs |
| Name in `Dest` lists and titles (`name_on`) | `L3 Depth` | `M6 Depth` |
| Group in lists (`group`) | `LFO 3` | `Mod 6` |
| Meaning | "How far LFO 3 swings" | "How far Mod 6, an LFO, moves" |
| `SettingId` constructor | `SettingId::lfo(lfo, at)` | `SettingId::modulator(slot, at)` |

* **Which entries apply where:** `phase` exists on LFO slots, where it's the phase, and on Steps
  slots, where it's `Start` (the value times `Length` over 256 picks the step, so a lock picks
  where a note's sequence starts, as #235 asks).
  * On any other type, `PHS` *n* is drawn faint, as a lock for a voice that isn't playing is
    (decision 29).
  * `DEP` *n* exists for every type, but is faint on the note-shape slot.
* **Clashes, checked against every label in `settings.rs`, `insert.rs` and the events:**
  * the only labels ending in a digit are these;
  * an FX slot's begin with one (`1BYP`, `4MIX`);
  * no track, voice or sampler label is `DEP` or `PHS`;
  * the events (`DEL KIL OFF RET CHA CND ACU ARP`) are three letters with no digit.
  * The tests `every_label_is_three_or_four_letters_after_a_slots_digit` and
    `no_two_labels_on_a_track_match` change their `Lfo` arm to `Mod` over six.
* **What moves for the author:** `DEP1` meant LFO 1 and now means Mod 1. Songs are converted
  (§9), so only habit changes. `/dep` still finds all six.
* **Kept out of the registry,** as today: every type's other settings, which a step can't lock.
  More locks (an envelope's `Decay`, an LFO's `Rate`) are each a new entry in `MOD`, if missed.

---

## 7. The Mod page

**The layout:** the rack of half-width devices of decision 151, now laid out **down the columns**.

* `Mod 1`–`Mod 3` are on the left and `Mod 4`–`Mod 6` on the right, so the default page (three
  ADSRs, then three LFOs) is today's picture.
* The accumulator and the arpeggio are under them.
* That's four bands of 9, which fill the 36 rows exactly, so nothing scrolls yet.
* A return, a group or the master has three bands.
* **Past four bands** (more slots, or a taller device) it scrolls by whole bands, as Decided note
  4 says and `rack_areas` already does.

**The title:** `╭─ Mod 3  ⟨toggle⟩  Steps  Trig  → Pitch ─`.

* `Mod n` is the identity, dim.
* Then the switch, then the **type**, then its **mode** if the type has one, then the
  **destination**.
* `→` keeps "what it is" apart from "where it goes", so `LFO  Trig  → Cut` doesn't read as
  "Trig Cut".
* A destination of `---` is dim, as now.

**The grids** are §2's, two columns of three in compact cells, with holes where a type has fewer.

**The graphics** are per type, all at size H (24 by 5), drawn by `kitty::plot` and sent only when
what shapes them changes (`PanelKey::Mod(n)` with a `ModDrawing` per type, replacing `Env(n)` and
`Lfo(n)`):

| Type | Panel | Moving part |
|---|---|---|
| ADSR, AHD, Drum | the shape with its curve, a baseline and dotted lines between stages (a hold has its own stage) | later: a riding dot, once `LiveValues` has each envelope's stage and level |
| LPG | its fall, worked out by running the model at a full ping | as the envelopes |
| LFO | one cycle from `Phase`, flipped for a negative depth. `Half` draws half a cycle, then flat. | decision 154's dot. In `Hold` it sits at the held value. |
| Steps | the steps as a staircase (lines between when `Smooth` is on), to the largest value's height over a centre line, `Length` wide, with a dotted line before the step `Loop` returns to | the dot, placed on the step playing |
| Track | the map: flat to `Low`, rising to `High`, flat after | the dot at the last note's value |

* **Dots:** a dot is a placement of the stored dot image (about 60 bytes a move), with ids 1 to 6
  by slot. `LiveValues::lfos` becomes `mods: [Option<ModNow>; 6]`.
* **What doesn't move the picture:** `Fade` and `Depth` don't change it, as `Rate` doesn't; the
  grid says them.

### Mockup 1: a bass track, every slot a different look

* **The cursor:** in Mod 3's panel, on its second step.
* **Mod 1:** an ADSR shaping each note.
* **Mod 2:** an AHD with `Snap` on the filter.
* **Mod 3:** a Steps flourish of +12, +7 and 0 on `Pitch` at `1/32`, played once. The `▲` under
  the panel marks the step being edited.
* **Mod 4:** an LFO in `Trig` that fades in over a bar, with its dot.
* **Mod 5:** an LFO switched off: a faint toggle, contents and wave.
* **Mod 6:** velocity opening the filter, with the last note's dot on the map.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Mod 1    ADSR  Exp  → Vol ───────────────────────────────────────────╮  ╭─ Mod 4    LFO  Trig  → Cut ───────────────────────────────────────────╮    ⠈⠉       
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │             
│  Attack    2 ms       Sustain   70%        │ ⡟⣄                     │  │  │  Shape     Tri        Phase     0°         │    ⣠⠖⠳●⡀               │  │     Main    
│                                            │ ⡇ ⠉⠓⠒⠲⠤⠤⠤⠤⠤⠤⠤⠤⠤⢤       │  │  │                                            │ ⢀⡴⠋⠁   ⠙⠦⣄             │  │    -9 -8    
│  Decay     300 ms     Depth     --         │ ⡇              ⠈⢇      │  │  │  Rate      1/4        Fade      +1 bar     │⠞⠁        ⠈⠓⢤⡀        ⢀⡤│  │  +6       ▔▔
│                                            │⢀⠇               ⠘⢆     │  │  │                                            │             ⠙⠲⣄   ⢀⣠⠞⠁ │  │             
│  Release   80 ms      Reset               │⣸                  ⠑⠦⢤⣀⣀│  │  │  Depth     +30                             │               ⠈⠑⢦⠴⠋    │  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯             
╭─ Mod 2    AHD  Snap  → Cut ───────────────────────────────────────────╮  ╭─ Mod 5    LFO  Free  → Pan ───────────────────────────────────────────╮             
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │  -6         
│  Attack    1 ms       Depth     +48        │⢸⠉⠉⠉⢹                   │  │  │  Shape     Sine       Phase     90°        │⠙⠒⢦⡀                ⢀⡴⠒⠋│  │             
│                                            │⢸   ⠘⡆                  │  │  │                                            │   ⠉⢦⡀            ⢀⡴⠉   │  │             
│  Hold      30 ms      Reset               │⢸    ⢳                  │  │  │  Rate      3/16       Fade      Off        │     ⠙⢆          ⡰⠋     │  │             
│                                            │⢸    ⠈⢣⡀                │  │  │                                            │       ⠳⣄      ⣠⠞       │  │ -12         
│  Decay     180 ms                          │⡼      ⠙⠲⠤⢄⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +20                             │        ⠈⠓⠦⣄⣠⠴⠚⠁        │  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Mod 3    Steps  Trig  → Pitch ───────────────────────────────────────╮  ╭─ Mod 6    Track  Velocity  → Cut ─────────────────────────────────────╮ -18 ██ ██   
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │     ██ ██   
│  Rate      1/32       Start     1          │⠉⠉⠉⠉⠉⠉⠉⠉⡇               │  │  │  Low       20                              │                    ⢀⣠⠴⠚│  │     ██ ██   
│                                            │        ⠉⠉⠉⠉⠉⠉⠉⢹        │  │  │                                            │                 ⣀●⠚⠉   │  │     ██ ██   
│  Length    3          Loop      Once       │               ⠸⠤⠤⠤⠤⠤⠤⠤⠤│  │  │  High      7F                              │             ⣀⠤⠒⠉       │  │ -24 ██ ██   
│                                            │                        │  │  │                                            │         ⣀⡤⠒⠉           │  │     ██ ██   
│  Depth     +FF        Smooth              │                        │  │  │  Depth     +30                             │⣀⣀⣀⣀⣀⣀⡤⠖⠋⠁              │  │     ██ ██   
│                                            └────────────▲───────────┘  │  │                                            └────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Accumulator ──────────────────────────────────────────────────────────╮  ╭─ Arpeggio ─────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Up        7          Limit     Wrap                                   │  │  Rate      Tick          Oct       1                                   │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Down      7          Adds      Row                                    │  │  Mode      Up            Retrig                                       │ -48 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Moves     Row        Reset     Chain                                  │  │                                                                        │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Step 2, +07  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                          
Mod 3's step 2 of 3 is 7 semitones up: = and - move it, + and _ by an octave, i types one                                                                          
```

**Editing Steps' values:**

* **Moving in:** the panel is a cursor stop in a Steps device. `l` from the right column's cells
  goes into it, on the step playing or else step 1.
* **Moving across:** `h` and `l` go across the steps, with counts. `0` and `$` go to the first
  and last. `h` from the first goes back to the grid.
* **Editing:** `=` and `-` move a value, `+` and `_` move it by an octave (`0C`), `i` types a
  signed hex value, and `x` sets it to `00`.
* **What shows:** the `▲` in the box's bottom border marks the step, in the cursor's colour, and
  the status line names it in units.
* **What doesn't change:** steps past `Length` keep their values and aren't drawn. The picture is
  sent again as a value changes, at most every other frame (§1.5's rule 3), and the cursor and
  the playing dot cost nothing more.

### Mockup 2: a percussion track, the other types

The cursor is on Mod 4's type.

* **Mod 1:** an LPG, the note shape, closing the filter as it falls.
* **Mod 2:** a Drum envelope dropping the pitch two octaves.
* **Mod 3:** an ADSR going nowhere, so it's dim.
* **Mod 4:** Steps in `Note` mode, giving each note the next of eight cutoffs (#235's test). Its
  `Rate` is `--`.
* **Mod 5:** an LFO in `Half`, a rise and fall on `Res` once a note.
* **Mod 6:** a random LFO in `Hold`, a new delay feedback each note. Its dot is at the held value
  over the centre line.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Mod 1    LPG  → Vol ─────────────────────────────────────────────────╮  ╭─ Mod 4    ›Steps  Note  → Cut ────────────────────────────────────────╮    ⠈⠉       
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │             
│  Attack    1 ms       Depth     --         │⢸⣆                      │  │  │  Rate      --         Start     1          │      ⡏⠉⠉⡇    ⢀⣀⣀⣀      │  │     Main    
│                                            │⢸⠘⢦                     │  │  │                                            │⠒⠒⠒⡆  ⡇  ⡇    ⢸  ⢸⣀⣀⣀   │  │    -9 -8    
│  Decay     400 ms     Reset               │⢸  ⠳⣄                   │  │  │  Length    8          Loop      from 1     │   ⡇  ⡇  ⠧⠤⠤⡄ ⢸     ⢸   │  │  +6       ▔▔
│                                            │⢸   ⠈⠑⠦⣄⡀               │  │  │                                            │   ⠉⠉⠉⠁     ⠧⠤⠼     ⢸   │  │             
│  Colour    C0                              │⡞       ⠉⠙⠒⠒⠦⠤⠤⠤⠤⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +FF        Smooth              │                    ⠈⠉⠉⠉│  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯             
╭─ Mod 2    Drum  Exp  → Pitch ─────────────────────────────────────────╮  ╭─ Mod 5    LFO  Half  → Res ───────────────────────────────────────────╮             
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │  -6         
│  Peak      8 ms       Depth     +18        │⡇                       │  │  │  Shape     Tri        Phase     0°         │    ⣠⠖⠳⢄⡀               │  │             
│                                            │⡇                       │  │  │                                            │ ⢀⡴⠋⠁   ⠙⠦⣄             │  │             
│  Body      30%        Reset               │⣇                       │  │  │  Rate      1/2        Fade      Off        │⠞⠁        ⠈⠓⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │             
│                                            │⡟⠲⣄                     │  │  │                                            │                        │  │ -12         
│  Decay     120 ms                          │⡇  ⠙⠒⠦⠤⢄⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +40                             │                        │  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Mod 3    ADSR  Exp  → --- ───────────────────────────────────────────╮  ╭─ Mod 6    LFO  Hold  → 1 Feedback ────────────────────────────────────╮ -18 ██ ██   
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │     ██ ██   
│  Attack    1 ms       Sustain   0%         │⢸⡀                      │  │  │  Shape     Random     Phase     0°         │                        │  │     ██ ██   
│                                            │⢸⢧                      │  │  │                                            │               ●        │  │     ██ ██   
│  Decay     200 ms     Depth     00         │⢸⠘⣆                     │  │  │  Rate      1/16       Fade      Off        │⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │ -24 ██ ██   
│                                            │⢸ ⠘⢆⡀                   │  │  │                                            │                        │  │     ██ ██   
│  Release   50 ms      Reset               │⡞   ⠙⠲⠤⣄⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +28                             │                        │  │     ██ ██   
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Accumulator ──────────────────────────────────────────────────────────╮  ╭─ Arpeggio ─────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Up        7          Limit     Wrap                                   │  │  Rate      Tick          Oct       1                                   │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Down      7          Adds      Row                                    │  │  Mode      Up            Retrig                                       │ -48 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Moves     Row        Reset     Chain                                  │  │                                                                        │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Type, Steps  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                          
Mod 4 is Steps: up to 16 values, one a note here. K picks another type                                                                                             
```

### Mockup 3: `K` on the type

`K` on the type opens the picker (decision 73), a small centred box, on the current type, which is
marked `•`.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Mod 1    LPG  → Vol ─────────────────────────────────────────────────╮  ╭─ Mod 4    ›Steps  Note  → Cut ────────────────────────────────────────╮    ⠈⠉       
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │             
│  Attack    1 ms       Depth     --         │⢸⣆                      │  │  │  Rate      --         Start     1          │      ⡏⠉⠉⡇    ⢀⣀⣀⣀      │  │     Main    
│                                            │⢸⠘⢦                     │  │  │                                            │⠒⠒⠒⡆  ⡇  ⡇    ⢸  ⢸⣀⣀⣀   │  │    -9 -8    
│  Decay     400 ms     Reset               │⢸  ⠳⣄                   │  │  │  Length    8          Loop      from 1     │   ⡇  ⡇  ⠧⠤⠤⡄ ⢸     ⢸   │  │  +6       ▔▔
│                                            │⢸   ⠈⠑⠦⣄⡀               │  │  │                                            │   ⠉⠉⠉⠁     ⠧⠤⠼     ⢸   │  │             
│  Colour    C0                              │⡞       ⠉⠙⠒⠒⠦⠤⠤⠤⠤⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +FF        Smooth              │                    ⠈⠉⠉⠉│  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯             
╭─ Mod 2    Drum  Exp  → Pitch ─────────────────────────────────────────╮  ╭─ Mod 5    LFO  Half  → Res ───────────────────────────────────────────╮             
│                                            ┌──╭─ Mod 4 type  7/7 ─────────────────────────────────╮                    ┌────────────────────────┐  │  -6         
│  Peak      8 ms       Depth     +18        │⡇ │ >                                                 │hase     0°         │    ⣠⠖⠳⢄⡀               │  │             
│                                            │⡇ │    ADSR   held while the note is                  │                    │ ⢀⡴⠋⠁   ⠙⠦⣄             │  │             
│  Body      30%        Reset               │⣇ │    AHD    the same whatever the note's length     │ade      Off        │⠞⠁        ⠈⠓⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │             
│                                            │⡟⠲│    Drum   a fast peak onto a body that decays     │                    │                        │  │ -12         
│  Decay     120 ms                          │⡇ │    LPG    a vactrol's fall, closing the filter    │                    │                        │  │             
│                                            └──│    LFO    a cycle, free or from each note         │                    └────────────────────────┘  │     ▅▅ ▃▃   
╰───────────────────────────────────────────────│ ›• Steps  up to 16 values, in time or a note each │────────────────────────────────────────────────╯     ██ ██   
╭─ Mod 3    ADSR  Exp  → --- ──────────────────│    Track  the note's pitch or velocity, or a roll │ 1 Feedback ────────────────────────────────────╮ -18 ██ ██   
│                                            ┌──╰───────────────────────────────────────────────────╯                    ┌────────────────────────┐  │     ██ ██   
│  Attack    1 ms       Sustain   0%         │⢸⡀                      │  │  │  Shape     Random     Phase     0°         │                        │  │     ██ ██   
│                                            │⢸⢧                      │  │  │                                            │               ●        │  │     ██ ██   
│  Decay     200 ms     Depth     00         │⢸⠘⣆                     │  │  │  Rate      1/16       Fade      Off        │⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │ -24 ██ ██   
│                                            │⢸ ⠘⢆⡀                   │  │  │                                            │                        │  │     ██ ██   
│  Release   50 ms      Reset               │⡞   ⠙⠲⠤⣄⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀│  │  │  Depth     +28                             │                        │  │     ██ ██   
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Accumulator ──────────────────────────────────────────────────────────╮  ╭─ Arpeggio ─────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Up        7          Limit     Wrap                                   │  │  Rate      Tick          Oct       1                                   │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Down      7          Adds      Row                                    │  │  Mode      Up            Retrig                                       │ -48 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Moves     Row        Reset     Chain                                  │  │                                                                        │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Type, Steps  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                          
Choosing keeps Mod 4's destination, depth and switch; a type it had before comes back as it was                                                                    
```

**Changing a type, step by step:**

1. `k` from the grid's left column goes up to the type, and from the right column to the
   destination. `h` and `l` go along the title: type, mode, destination.
2. `K` on the type lists the seven, each with its meaning dim, opening on the current one.
3. `Enter` chooses. That's one undo step. The slot keeps its `Dest`, `Depth` and switch, the
   type's own settings come back as they were (or are its defaults), and the sound glides over
   5 ms (§4).
4. The status line says what it kept, and for Steps with no depth, that it set `+FF`.

**The other title keys:**

* `K` on the mode lists its modes, and `K` on the destination lists `Dest` as now.
* `=` and `-` move the mode, as any list does.
* They don't move the type. A type changes what the device is, so only `K` chooses it, as only
  `K` chooses an effect.
* `x` on the type sets the slot back to its default type and that type's defaults, as one undo
  step.

**Width in Keys mode:** a half device is 73 wide. The grid takes columns 3 to 42 and the panel's
box starts at 44, as decision 151 measured. The longest title is
`Mod 6  ⟨toggle⟩  Track  Velocity  → 1 Feedback`, 46 columns, and fits with room.

**Navigation**, decision 151's moves, by column:

* `l` from Mod 1 goes to Mod 4 on the same row.
* `}` goes down a column, Mod 1 → 2 → 3 → Accumulator.
* `3}` goes from Mod 4 to the Arpeggio.
* `gg` and `G` go to the top and bottom of the cursor's column.
* The devices are read as `Mod 1, Mod 4, Mod 2, Mod 5, Mod 3, Mod 6, Accumulator, Arpeggio`, which
  `rack_places` pairs into bands.

---

## 8. Roto layouts per type

**The rules** are #419's:

* the Roto plays and tweaks, and the app builds, so **a slot's type is never on the Roto**: it
  swaps the whole layout under the hand;
* musical order;
* off/on settings on page 1's buttons, each under a knob it goes with;
* `Dest` alone on a page of its own;
* ASCII only.

**What carries over:** the approved ADSR layout is kept. The LFO's is kept but for `Retrig`, which
became the mode.

**Musical lists stay knobs:** the curve, the LFO's and Steps' modes, and Track's source. None
changes the layout.

**Device names:** `M1 ADSR` to `M6 Track`, beside the FX slots' `1 Delay`.

| Type | Page 1 knobs 1–8 | Page 1 buttons | Then | Kind |
|---|---|---|---|---|
| ADSR | Attack, Decay, Sustain, Release, Depth, Curve, –, – | `Reset` under Attack | page 2: Dest | stored (approved) |
| AHD | Attack, Hold, Decay, gap, Depth, Curve, –, – | `Reset` under Attack | page 2: Dest | stored |
| Drum | Peak, Body, Decay, gap, Depth, Curve, –, – | `Reset` under Peak | page 2: Dest | stored |
| LPG | Attack, Decay, Colour, gap, Depth, –, –, – | `Reset` under Attack | page 2: Dest | stored |
| LFO | Rate, Shape, Depth, Phase, Fade, Mode, –, – | none | page 2: Dest | **live**, as it has no button left |
| Steps | Rate, Length, Depth, Start, Loop, Mode, –, – | `Smooth` under Rate | pages 2 and 3: steps 1–8 and 9–16; page 4: Dest | stored |
| Track | Low, High, Depth, Source, –, –, –, – | none | page 2: Dest | live |

**Steps on the Roto** is a small step sequencer on the hardware, which is the Roto's job. Its
values come before `Dest`, so Dest is page 4 rather than 2 (question 3).

* Each knob turns a step's value through `-FF` to `+FF` with a centre notch.
* A step past `Length` is still mapped, so lengthening it finds the values there.

**Stored setups:** today's nine (three compressor types, Delay, Grains, envelope, LFO, Arpeggio
and the master's output) become twelve. The envelope's one becomes four (ADSR, AHD, Drum, LPG),
the LFO's goes, and Steps' comes. That's still far from the Roto's 64 beside the author's five.

**Colours** are by family, as decision 161 has them. Time settings and `Rate` are yellow, `Depth` mauve as
modulation is, and the Steps values mauve with a centre notch.

**Switching:** SEL tapped switches a slot on and off, as it does an envelope or an LFO now
(decision 160).

---

## 9. Song format and conversion

* **The format:** `FORMAT_VERSION` goes from 8 to 9. `Track` saves `mods` as a fixed tuple of six,
  and `lfos` and `envelopes` are gone. A format-8 song fails with the clear message decision 138
  set up ("uses format 8, and this version reads format 9"). There's no compatibility code and no
  tolerant loading, by the author's rule.
* **A slot as saved:** only what differs from defaults, beyond its type, destination and depth:

  ```ron
  mods: ((
      kind: Adsr,
      destination: Some("vol"),
      depth: 0,
  ), (
      kind: Adsr,
      destination: Some("cut"),
      depth: 48,
      adsr: (attack_ms: 1, decay_ms: 200, sustain: 0, release_ms: 50, reset: true, curve: Exp),
  ), (
      kind: Lfo,
      destination: Some("cut"),
      depth: 48,
      lfo: (shape: Sine, rate: Steps(7), phase: 0, mode: Free, fade: Off),
  ), ...)
  ```

**The one-off conversion** is for the author's songs: `confusion-pump-panel`, `daft-punk-03` and
`sweet-groove` in `~/Music/vimtracker/projects/303-patterns`, all format 8. The
`samples/g7-*/song.ron` files are format 1 and already don't load.

1. **The script:** a Python script, written in PR 1's branch and removed before it merges, so
   nothing in `main` reads format 8. It rewrites each song's text:
   * `envelopes: (E1, E2, E3)` become slots 1–3 as `kind: Adsr`, with `attack_ms`, `decay_ms`,
     `sustain`, `release_ms` and `reset` under `adsr:`, and `destination`, `depth` and `bypassed`
     on the slot;
   * `lfos: (L1, L2, L3)` become slots 4–6 as `kind: Lfo`, with `shape`, `rate` and `phase` under
     `lfo:`, and `retrig: true` as `mode: Trig`, otherwise `Free`;
   * lock keys `lfoN.depth` and `lfoN.phase` become `mod{N+3}.…`. The author's songs have none
     today, but the script handles them;
   * `format_version: 8` becomes `9`.
2. **Safety:** it writes each converted song beside the original as `song.ron.v9`. It never
   writes over a song. The author swaps them in by hand once the check passes.
   `confusion-pump-panel` is theirs and mustn't be lost.
3. **The check:** render each song with the last format-8 build and the new one, and compare the
   files. They should match bit for bit, as the slots run in the old order and all 114 LFOs are
   `Tri`.

---

## 10. Accumulator and Arpeggio stay separate

They aren't slots and can't be chosen as a type. They move notes, from pass to pass and from note
to note, where modulators move settings over time.

* **The accumulator** keeps a walk across passes (decision 85).
* **The arpeggio** replaces a note's pitch with a sequence of notes, and can strike each again
  (decision 86).

Neither has a destination or a depth, and neither is in the registry, so making them slot types
would give six slots two kinds that share nothing with the other seven. They keep their own
devices in the Mod page's last band, unchanged.

**Where Steps meets them:** Steps on `Pitch` can sound like an arpeggio, but it moves the pitch of
one sounding note, never strikes it again, and doesn't follow the scale, so `ARP` stays the way
to arpeggiate.

---

## 11. Plan of PRs

Each PR carries its decision, the guide and the manual's Mod screenshot, and keeps both golden
renders unchanged unless it says otherwise.

| # | PR | Covers | Depends on | Size |
|---|---|---|---|---|
| 1 | **Mod slots:** `ModSlot` with ADSR and LFO only, behaving exactly as today, in 6 slots. The registry's `mod1`–`mod6`, `SetMod`, `for_mod`, `SlotRun` and one loop in `modulate`, the type-change glide, the generalised note shape, `LiveValues::mods`, `PanelKey::Mod`. The Mod page by columns with `Mod n`, the type and `→ Dest` in the title, and `K` on the type. The Roto's ADSR and LFO by slot. Format 9, and the conversion run and checked. | #422's core | #419 merged | L: about 1,600 changed, much of it renames, and about 500 of tests |
| 2 | **Envelope curves and AHD:** `Curve` as the envelope mode (`Exp`/`Lin`/`Snap`), the voice's shape enum, the AHD type with a time or `Note` hold, stage lines for a hold, and the Roto's `Curve` knob and AHD layout | #46, #85 | 1 | M: 500–700 |
| 3 | **LFO modes and Fade:** `Free`, `Trig`, `Hold`, `One` and `Half` in place of `Retrig`; `Fade`; the last note's time in the state; `Half`'s picture and `Hold`'s dot; the Roto's LFO going live with `Fade` and `Mode` | #86 | 1 | M: 400–550 |
| 4 | **Drum and LPG types:** Plaits' LPG scaled by rate, with `Colour` on `Cut`; their pictures and Roto layouts; a `probe` spec for each, against Plaits' own output for the LPG | #45 | 2 (the curve and the voice's shape enum) | M: 500–700 |
| 5 | **Steps:** the type, its three modes, `Loop` and `Smooth`, `PHS` as `Start`, the panel as a cursor stop with its keys, the staircase and the playing dot, `Note` mode's count, and the Roto's value pages | #235 | 1 (3 for the last note's time) | L: 900–1,200 |
| 6 | **Track type:** `Note`, `Velocity` and `Random` sources, the map's picture and dot, and its stream | #422's "possibly" | 1 | S–M: 300–400 |

**Order:** 1, then 2, 3, 5 and 6, then 4 after 2. They all edit `modulation.rs` on both sides, so
they go one at a time rather than in parallel. 2, 3 and 6 are small enough not to wait long.

**Later, as issues with no milestone:**

* an envelope follower type;
* a trigger envelope from another track;
* Steps in scale degrees;
* the envelopes' riding dot;
* `yy`, `dd` and `p` for slots;
* more locks (`Rate`, an envelope's times).

---

## 12. Questions for the author

1. **LFO modes in place of `Retrig`.** Should Elektron's five modes (`Free`, `Trig`, `Hold`, `One`,
   `Half`) replace the `Retrig` switch, as §2 recommends? On the Roto the LFO then loses the
   approved `Retrig` button under `Phase`: `Mode` becomes a five-step knob, and the LFO a live
   device. The other way is to keep `Retrig` as a switch and add a separate `Once` setting
   (`Loop`, `One`, `Half`, `Hold`), which keeps the button but allows pairs that mean nothing.
2. **Which slot shapes the notes.** Should it be the first envelope-type slot on `Vol`, wherever
   it is, as §4 recommends, or only slot 1, as today's rule has only `Env 1`? The first is more
   forgiving when slot 1 becomes an LFO. The second is simpler to explain.
3. **Steps on the Roto.** Should Steps' 16 values be on pages 2 and 3, putting its `Dest` on page
   4, as §8 recommends? Or should `Dest` stay on page 2 for every modulator, with the values on
   pages 3 and 4?
4. **Which types are in M20.** §2 proposes seven: ADSR, AHD, Drum, LPG, LFO, Steps and Track.
   Should `Track` (note, velocity or a roll each note) be in M20, or left for later with the
   envelope follower and the M8's trigger envelope?

**Decided** (the author, 2026-11-11, taking all four suggestions):

1. **LFO modes replace `Retrig`.** The modes are `Free`, `Trig`, `Hold`, `One` and `Half`. On the
   Roto, the approved `Retrig` button becomes a five-step `Mode` knob.
2. **The notes' volume shape comes from the first envelope-type slot routed to `Vol`,** whichever
   slot that is.
3. **On the Roto, Steps' values come before `Dest`,** which stays last.
4. **The Track type is in M20,** as its last PR (#428).

**As built in PR 1 (#427, decision 165):**

* **The mode:** `LfoMode` has `Free` and `Trig` for now, in the title, in place of `Retrig`, and
  #86 adds `Hold`, `One` and `Half`. While there are two, the Roto keeps its button under `Phase`
  for the mode, lit for `Trig`; #86 makes it the five-step knob.
* **The title's order** is type, mode and destination for `h` and `l`, as on screen. So `k` from
  an LFO's right column reaches its mode, and from an ADSR's, which has no mode yet, its
  destination.
* **The random waves' keys:** a slot's key is `(slot + 3) % 6`, so slots 4 to 6 have the keys LFOs
  1 to 3 had, and every default song, a `Random` or `Smooth` LFO's included, renders as before.
* **Polarity (#434, the author's addition):** a common setting, `Uni` or `Bi`, saved only once
  chosen; until then a slot follows its type's, an envelope `Uni` and an LFO `Bi`, so every song
  sounds as it did. `Uni` takes an LFO from 0 to 1, and `Bi` centres an envelope on its setting.
  It's in the title, after the mode, as ADSR's grid is full; a step locks it (`POL1`–`POL6`), and
  nothing modulates it. The pictures' dotted line is a baseline for `Uni` and a centre line for
  `Bi`, and on the Roto it's a button under `Depth`, lit for `Bi`.
* **Memory:** a `Track` grows from 344 bytes to 432 with the ADSR and LFO settings and the polarity, so a song
  of 15 tracks costs about 1.9 KB more per undo version, not §4's 9 KB; Steps' values will add
  the most.

**As built for #46 (decision 176),** the first half of §11's PR 2, which went as two pull
requests, curves then AHD:

* **The curve** is `envelope::Curve`, saved in `Adsr` with a default of `Exp`, so the format
  stays at 9 and every song sounds as it did.
* **The name:** the status line and the Roto call an envelope's mode `Curve`, and an LFO's
  `Mode`.
* **Not a lock,** as §6 keeps a type's own settings out of the registry.

**As built for #85 (decision 177),** the second half:

* **The hold** is `envelope::Hold`, `Note` or `Ms(u16)`, and `Note` is below `0 ms` for `=` and
  `-`.
* **The voice's shape** is `envelope::Envelope`, an `Adsr` or an `Ahd`, as §4 has it.
* **The Roto** stores AHD as a setup of its own, so there are 10.

**As built for #467 (decision 192):**

* **A fourth curve, `Analog`:** an attack charging toward 1.5 and switched at 1, `1.5 (1 - 3^-t)`,
  and falls that are true RC curves, 30 dB down halfway. The sustain isn't squared, as Surge's is.
* **An attack from a level** (`Reset` off) starts where its curve reaches that level and takes only
  what's left of its time. It's how `Reset` off always works, not a setting.

**As built for #468 (decision 193):**

* **`Smooth`** is a Catmull-Rom curve through four of `Random`'s values, still stateless.
* **The LFO's grid** is `Shape`, `Rate` and `Depth` down the left, and `Phase`, the hole for
  `Fade`, then `Deform` down the right. `Deform` bends the wave between its peaks, or sets a
  square's width, and is a lock, `DFM4`–`DFM6`, and a destination for earlier slots.

**As built for #45's Drum (decision 194),** the first half of §11's PR 4, which went as two pull
requests, Drum then LPG:

* **No `Reset`:** a drum always strikes from full, so there's nothing to start from where it is.
  Its grid is `Peak`, `Body` and `Decay` down the left, and `Depth` and `Ratchet` down the right.
* **`Ratchet`,** 1 to 8, strikes the peak again before the body, for claps and flams (the DSP
  parts review's oscillators Part B, trick 13).
* **The peak's fall** is `Snap`'s, whatever the curve, which bends the body's decay.
* **The Roto** stores Drum as a setup of its own, `Ratchet` on knob 4, so there are 11.

**As built for #45's LPG (decision 195),** the second half:

* **The vactrol is solved,** not stepped: Plaits' equation in closed form, `v⁴ = E / (R + (1 - R) E)`,
  the same at any rate, with Plaits' middle ratio and a `Decay` timed to 60 dB.
* **`Colour`** closes `Cut` by Plaits' brightness in pitch, wherever the slot goes, but not from
  `---`.
* **The Roto** stores LPG as a setup of its own, so there are 12.

**As built for #86 (decision 202):**

* **Stateless:** `Hold` works its wave out at the time of the track's last note rather than
  keeping the value, and `One` and `Half` are `Trig` held just short of a cycle or half of one.
  Before any note, `Hold` holds where its wave starts and the one-shots are at their end.
* **Fade** is `lfo::Fade`, `Off`, `In(ticks)` or `Out(ticks)`, a straight line from the last note;
  before any note a fade in is done and a fade out is over.
* **The grid** is `Shape`, `Rate` and `Depth` down the left, and `Phase`, `Fade` and `Deform`
  down the right.
* **The Roto** keeps the LFO stored, for the polarity's button: `Fade` and the five-step `Mode`
  are knobs 6 and 7.
