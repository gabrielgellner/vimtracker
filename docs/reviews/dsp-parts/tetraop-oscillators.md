# TetraOP and Surge XT oscillators and modulators: what vimtracker could take

Research only; nothing changed in either repo. Paths are shortened as follows:
`TOP/` = `/Users/gabrielgellner/dev/dsp/tetraop/src/`, `SXT/` = `/Users/gabrielgellner/dev/dsp/surge/src/common/`,
`VT/` = `/Users/gabrielgellner/dev/vimtracker/crates/`. "Ours" means `VT/vt-engine/src/dsp/voices/` unless a path is given.

**Licences in one line:** TetraOP is GPL-3 (`tetraop/LICENSE`, header "Copyright © tilr 2026" in
`TOP/engine/Unison.h:1-3`). Surge XT is GPL-3.0-or-later (every file header, e.g.
`SXT/dsp/oscillators/ModernOscillator.cpp:1-21`). **sst-basic-blocks is GPL-3.0-or-later too**, except
three headers that its README marks as dual GPL-3/MIT: `dsp/LanczosResampler.h`, `dsp/HilbertTransform.h`
and `dsp/EllipticBlepOscillators.h`. That last one wraps Signalsmith's `elliptic-blep`, which is MIT.
Both are compatible with vimtracker's GPL-3; nothing here needs a permissive licence. One caveat: the
sst submodules aren't checked out in either local tree (`surge/libs/sst/*` and `OB-Xf/libs/sst/*` are
empty). So the sst licence facts and code come from GitHub, `surge-synthesizer/sst-basic-blocks` main,
fetched 2026-11-11, and I can't give local line numbers for sst code.

---

## Part A: ranked ports (whole voices or modulator types)

| # | What | Adds or replaces | Licence | Size in Rust | CPU | Risk |
|---|---|---|---|---|---|---|
| 1 | **Surge Alias oscillator**, as a new lo-fi voice | Adds an 8-bit "digital" voice: mask, threshold, wrap and bitcrush. Nothing we have does it; `chiptune.rs` is Plaits' square chords. | GPL-3 | ~200 lines; integer per-sample loop, no SIMD needed | very low: u32 phase and byte ops | low |
| 2 | **Unison and drift VA** ("supersaw"), built on our own polyBLEP oscillators with sst's `UnisonSetup` and `DriftLFO` and TetraOP's unison modes, *not* a port of Surge's Classic or Modern | Adds the detuned-stack VA we lack; every VA we have is a single Plaits oscillator pair | GPL-3 (sst and TetraOP), our code MIT-derived | ~250 lines | N times one VA (7 voices is about 7x `virtual_analog.rs`) | low to medium |
| 3 | **An editable 4-op FM voice with TetraOP's topology**: a 4x4 matrix that is one sample late, a waveform per operator, ring mod, phase distortion per operator | Adds an M8 or Digitone-style FM you program yourself; `six_op.rs` only plays DX7 presets | GPL-3 | ~400 lines DSP, plus UI pages | 4 table reads per frame at 2x; f32x4 across the operators | medium (mostly the parameter count and UI) |
| 4 | **Surge String's sustained exciters, second string and negative feedback**, as a "bowed/blown" string | Adds sustained physical models (bowed, blown, clarinet-like); `string_model.rs` only plucks | GPL-3 | ~400 lines | 2 delay lines with interpolated reads | medium (memory, retuning) |
| 5 | **Elliptic BLEP** (`EllipticBlepOscillators.h`, dual GPL-3/MIT; Signalsmith MIT) in place of polyBLEP in the VA oscillators | Replaces 2-point polyBLEP with an 11th-order elliptic residue: much less aliasing on high notes | MIT | ~300 lines (lib plus wrapper) | higher: a sum of one-pole states per oscillator, SIMD-friendly | medium: changes the faithful Plaits sound; IIR phase |
| 6 | **A future "Shape" mod type, MSEG-lite**: 8 fixed points with a curve type per segment, taken from TetraOP's `Pattern` and Surge's MSEG segment types | Adds drawn envelopes and LFOs, later, as a 7th or 8th type | GPL-3 | ~250 lines plus an editor panel | trivial (one evaluation every 32 frames) | low DSP risk, high UI cost |

**Not recommended** (reasons in Part C): Surge Classic, Wavetable, Window, Sine, FM2 and FM3 as whole
voices; S&H noise; Twist (it is Plaits, which we already have, except Speech); Audio Input; the Formula
modulator (LuaJIT: it allocates and isn't real-time safe); a full MSEG.

**Side finding:** the one Plaits engine we lack is **Speech** ("Vowels/Speech",
`SXT/dsp/oscillators/TwistOscillator.cpp:54`). Port it straight from `../dsp/eurorack` under its MIT
licence, not through Surge.

---

## Part B: tricks worth stealing (ranked by value per effort)

Each is a technique to graft onto our existing voices or mod slots.

| # | Trick | Theirs | Ours, to improve | Gain | Size |
|---|---|---|---|---|---|
| 1 | **Smooth random as Catmull-Rom over 4 keyed values** instead of a smoothstep between 2 | Surge `lt_noise`: `cubic_ipol(wf_history[3..0], phase)`, `SXT/dsp/modulators/LFOModulationSource.cpp:1029-1033` | `VT/vt-model/src/lfo.rs:345-350` (`Shape::Smooth` eases from `random(key,c)` to `random(key,c+1)` with `3t²-2t³`, which has zero slope at every node, so it reads as stop-go) | sound: a truly smooth wander. It stays stateless, because `random(key, c-1..c+2)` (lfo.rs:355) can be computed on the spot. Clamp to ±1, since Catmull-Rom overshoots slightly. | ~6 lines |
| 2 | **Use the sine table in `fm.rs`** instead of `f32::sin()` | Plaits' own `SinePM` reads `lut_sine`; sst `FastMath.h` has a Padé `fastsin` (GPL-3) | `fm.rs:56-58` calls `sin()` twice per sub-sample at 4x, 8 per output frame (`fm.rs:152-164`). `six_op.rs:703-711` already reads `blocks/sine_table.rs` with a lerp. | CPU: probably the largest per-voice cost in `fm.rs`. A 512-step lerp has an error of about 5e-6 (below -100 dB). | ~10 lines |
| 3 | **LFO Deform: one bend for every shape**, `x ← x − a·x² + a` applied twice, `a = d/2`; the deform is the width for Square | `bend1`, `LFOModulationSource.cpp:111-120`; Square `phase > 0.5+0.5·d`, `:984-989`; sst `SimpleLFO.h` | `VT/vt-model/src/lfo.rs:321-352` (`wave`). The LFO grid has a hole: Shape, Rate, Depth \| Phase, Fade, hole (`docs/designs/mod-slots.md:167`). | sound: one cell turns Sine, Tri and Saw into skewed or log/exp shapes and Square into a pulse. Stateless and cheap. | ~15 lines plus the cell |
| 4 | **Steps' `Smooth` as a continuous control** (Surge's step-sequencer deform): −1..−0.5 rises from 0 within each step; −0.5..0 decays within each step; 0..0.5 holds, then slews over the last part of the step; 0.5..1 blends linear into cubic | `LFOModulationSource.cpp:1095-1180` (type 2 is a B-spline, `:1098-1149`); the per-step trigger mask is `StepSequencerStorage::trigmask`, `SXT/SurgeStorage.h:904-910` | The Steps design, `mod-slots.md:197-208` (`Smooth` on/off) and `:380` | sound: one existing cell covers stepped, slewed, smooth and per-step "pluck" envelopes. No new cell. | ~40 lines |
| 5 | **Analog drift**: per-voice one-pole-filtered noise, stepped once a block, added to the pitch in semitones × `drift` | sst `DriftLFO` (one-pole, coefficient 1e-5, output ×≈316; `next()` once a block), used at `ModernOscillator.cpp:346-351` and `AliasOscillator.cpp:300` | `virtual_analog.rs:414-421` (frequency in), `virtual_analog_vcf.rs:90`, `acid.rs`, `chiptune.rs:82`. Seed it per note, as `string_model.rs:95` already does, so renders stay reproducible. | sound: the VAs come alive; costs nothing per sample | ~12 lines plus a seed |
| 6 | **Unison layout**: detune positions linear on −1..1, gain `1/√N`, alternating pan `1∓d`; TetraOP adds a spread-warp exponent and Fifths/Sub on odd voices, plus random start phases | sst `UnisonSetup` (detune `2/(N−1)·v − 1`, `attenuation = 1/√N`, `panLaw`); TetraOP `TOP/engine/Unison.h:35-70` (warp `:46-55`, Sub ×0.5 and Fifths ×1.5 `:58-66`), phases `TOP/engine/OSC.cpp:50-54` | New, for port #2; also `swarm.rs` (8 saws) as a check on its spread | sound: a standard supersaw spread, plus octave and fifth stacks for free | ~40 lines |
| 7 | **Retriggering an attack by inverting its curve**: from level `L`, start the attack at `t₀ = rise⁻¹(L)` instead of stretching a full attack from `L` | TetraOP `getMatchingAttackTimeFromRelease`, `TOP/dsp/Envelope.cpp:73-81`; Surge's analog mode just keeps charging from where it is (`ADSRModulationSource.h:116-119`) | `VT/vt-model/src/envelope.rs:83-88` (`from + (1-from)·rise(t/attack)` always takes the whole attack time, and the curve's shape changes with `from`) | sound: fast retriggers and legato keep their punch, as analog envelopes do; the shape stays the same | ~6 lines (`rise⁻¹(L) = L^(1/3.32)`) |
| 8 | **A closed-form "Analog" envelope curve**: charge toward 1.5 and switch to decay at 1.0, so `rise(t) = 1.5·(1 − 3^(−t))`, exact at t=1; decay and release are true RC curves toward the sustain, Surge using **S²** | `ADSRModulationSource.h:159-245` (`v_cc = 1.5f` at `:176`, `S = S·S` at `:198`, 1 V switch at `:182-192`) | A 4th curve mode beside Exp, Lin and Snap (`mod-slots.md:95-101`, `envelope.rs:14-23`) | sound: the familiar fast-start analog attack. Ours (`t^3.32`) starts slow. | ~15 lines |
| 9 | **Ring-mod crossfade between operators**: `out = out·(1−r) + out·other·r` | `TOP/engine/FmMatrix.cpp:513-519` | port #3; also `virtual_analog.rs` (saw × square) as a `further` setting | sound: bell and metal tones | 1–3 lines |
| 10 | **Phase-distortion functions on any phase-read voice**: Skew, Bias, Pulse, Sync, Quantize, Fold, and Formant (sync × half-sine window) | `TOP/engine/PhaseDist.h:34-127`; Formant = `sync` plus `windowHalfSine`, `FmMatrix.cpp:188-191, 521` | `fm.rs:163-164` (warp the carrier or modulator phase); port #3. Not `wavetable.rs:197`, because its integrated-wave trick (`wavetable.rs:1-6`) breaks under a warped phase unless the output is scaled by `g'(φ)`. | sound: a cheap formant, VOSIM-like or lo-fi stepped timbre from a plain sine. Bend's `exp`/`log` is heavy (`PhaseDist.h:59-70`); the rest are a few ops. | 5–15 lines each |
| 11 | **Squared negative feedback**: feeding back `y²` (or the 2-sample average squared) instead of `y` when the feedback is negative | Surge Sine `SineOscillator.cpp:799-806`; FM2 and FM3 `FM3Oscillator.cpp:164-165` | port #3. `fm.rs:144-148` already gives negative Morph a meaning (frequency feedback on the modulator), so leave it. | sound: a second, square-ish feedback colour | 2 lines |
| 12 | **TX81Z-style operator waves from sin and cos by quadrant** (half, abs, quarter sines and more) | `SineOscillator.cpp:150-330` (`calcquadrant`, `valueFromSinAndCosForMode<1..>`); Alias `aow_sine_tx2..8`, `AliasOscillator.h:57-63` | `fm.rs:56-58` (one wave per operator, chosen by a `further` setting); port #3 | sound: classic TX and Digitone brightness; `fm.rs` runs at 4x, which tames their corners | ~25 lines |
| 13 | **Ratchet segment**: N repeated `(1−e^(−kt))/(1−e^(−k))` charges within one segment, k=3 by default | `SXT/dsp/modulators/MSEGModulationHelper.cpp:1061-1105`. It's our own `fall()` (`envelope.rs:21-23`, k=4). | Drum type (`mod-slots.md:121-133`): a `Ratchet` count for 808 claps and flams; it uses Drum's spare hole | sound: claps from one slot | ~15 lines |
| 14 | **Modulators read full-band tables; only carriers get the mipmap for their note** | `TOP/engine/FmMatrix.cpp:720, 729, 737` (`useBandLimiting = oscIsOut`) | port #3, if its operator waves are tables | CPU and memory; it's harmless only because TetraOP oversamples 2x | tiny |
| 15 | **A fixed offset in Hz between two modulators**, `+sh` and `−sh` | `FM2Oscillator.cpp:102-115` (`m12offset`) | `fm.rs:123` (modulator frequency): a beating "detune" that holds across the keyboard | sound: a chorused FM pad | ~4 lines |
| 16 | **Skip silent operators**: switch the 4 operators by template on whether their level is above 0 | `FmMatrix.cpp:340-381` | `six_op.rs:886-946`: skip `render_operators` when both the operator's amplitude and its target are tiny, still advancing the phase by `frequency·BLOCK` | CPU, on patches with silent operators | ~10 lines |
| 17 | **Ramp in new unison voices** over 50 samples (at 44.1k) | `SineOscillator.cpp` `prepare_unison` (`dplaying`, `playingramp`) | port #2 | stability: no clicks when the voice count changes | ~8 lines |
| 18 | **Keep an LFO's phase when its rate changes**: re-offset the phase so it doesn't jump | `TOP/dsp/LFO.cpp:18-23` | `lfo.rs:321` works the phase out from the song's position, so a rate lock jumps it. **Leave it:** that buys determinism (decision recorded at `lfo.rs:316-319`). Just note the behaviour. | n/a | n/a |
| 19 | **Correlated noise**: two cascaded one-poles with a signed coefficient (`lastval2 = r(1−w) − w·lastval2`; `lastval = lastval2(1−w) − w·lastval`; ×`1/√(1−w)`) | sst `CorrelatedNoise.h`, used at `LFOModulationSource.cpp:339-358` | `noise.rs` (a "colour" from smooth to anti-correlated hiss); not the LFOs, which must stay stateless | sound: noise colour from one setting | ~10 lines |

**Already done well on our side, so nothing to steal:**

* `six_op.rs:911-917` averages the last two feedback samples, as the DX7 does. TetraOP feeds back one
  raw sample (`FmMatrix.cpp:498-504`).
* `fm.rs:124-127` tames the index with pitch. TetraOP has no index taming; its only defence against
  aliasing is 2x oversampling.
* Surge Modern's "analytic away from the edge, numeric near it" shortcut (`ModernOscillator.cpp:146-158`)
  is the same idea as our polyBLEP's correction near each edge (`virtual_analog.rs:84-104`).

---

## Part C: details

### C1. TetraOP: what it is

**It's a 4-operator wavetable phase-modulation synth** after Ableton Operator, built on JUCE and Gin.
The README calls it "a wavetable synth with four oscillators, FM, Unison, ring modulation". It's SIMD
across 4 *voices* (`SIMDSZ = 4`, `Globals.h:4`; `SIMDVox`, `TOP/engine/FmMatrix.h:16-20`).

**Operators:**

* Each is a wavetable reader with morphing (`renderWaveLinear`, `FmMatrix.h:132-156`). It's linear
  for the main path, every `renderX` choice at `FmMatrix.cpp:470-477` resolving to `renderWaveLinear`;
  cubic is used only for unison (`FmMatrix.cpp:247-282`).
* The tables can be Basic Shapes, a file, user data, white noise or pink noise
  (`TOP/engine/TablesManager.h:11-18`).
* Per operator: level, pan, octave, semitone and cent, fixed frequency (440 Hz,
  `TOP/engine/OSC.cpp:46`), phase and phase randomisation, morph with frame snap, phase-distortion
  mode and amount, feedback, and unison of up to 16 voices in 5 modes (parameters at
  `TOP/PluginProcessor.cpp:96-121`).

**Algorithms:**

* 10 fixed layouts plus Custom (`FmMatrix.h:39-52`, filled at `FmMatrix.cpp:29-60`).
* Custom is a full 4x4 FM matrix with each amount on 0..1, an output flag per operator, and a
  separate 4x4 ring-mod matrix including self ring mod (`PluginProcessor.cpp:52-91`). The matrix can
  be modulated per block (`FmMatrix.cpp:80-108`).

**How it does FM:**

* An operator's phase offset is the sum of the *previous sample's* outputs of every operator times
  the matrix entries: `offsetA = la·fbA + lb·ba + lc·ca + ld·da` (`FmMatrix.cpp:494-504`).
* Every connection is therefore one sample late. Order doesn't matter, and cycles (B→A→B) are legal.
* The offset is in turns. An output is `wave·level` (`:511`), so the largest index per modulator is
  1 turn, 2π rad. That's modest beside the DX7.
* **For us:** with everything one sample late, the 4 operators are independent within a sample. A
  monophonic 4-op voice is then a natural `f32x4` kernel across the operators: 4 phases, 4 table
  reads, and one 4x4 matrix-vector product a sample. That suits our one voice per track better than
  TetraOP's SIMD across voices.

**Feedback:** one raw previous sample on the matrix diagonal (`FmMatrix.cpp:498-504`), 0..1 turns.
There's no averaging and no index taming.

**Antialiasing:**

* The whole synth runs at 2x in release builds, through JUCE's polyphase halfband IIR
  (`PluginProcessor.cpp:318-329`, `osrate` `:645-650`). There's an extra 2x for the distortion effect
  (`:312-316`).
* Tables are mipmapped by note, but **only for carriers**. Modulators read the full-band table
  (`FmMatrix.cpp:720-737`).

**Phase distortion:** 8 warps applied to an operator's phase before its table read
(`TOP/engine/PhaseDist.h:13-127`).

* Bend: exponential, split into positive and negative paths for CPU (`FmMatrix.cpp:171-186`).
* Skew, Bias, Pulse, Sync and Quantize.
* Fold: a cubic fold of the phase.
* Formant: sync × a half-sine window (`:188-191`).

**Unison:** carriers only (`OSC.cpp:232-234`), batched 4 lanes at a time (`FmMatrix.cpp:284-327`).
Its detune positions are warped by an exponent (`Unison.h:46-55`); Fifths and Sub change every other
voice (`:58-66`).

**Envelopes:**

* Operators have none built in; levels come from the modulation matrix.
* The `Envelope` class is a closed-form, stateless `f(t)`: ADSR, AHD, DADSR or DAHDSR, with a power
  "tension" per stage (`TOP/dsp/Envelope.cpp:4-63`). That's the same approach as our
  `envelope.rs:83-108`.
* A retrigger matches the curve's time to its level (`Envelope.cpp:73-81`).

**LFOs:**

* They're drawn `Pattern`s (`TOP/dsp/Pattern.h:16-26`). The point types are Hold, Curve, SCurve,
  Pulse, Wave, Triangle, Stairs, SmoothSt and SinCurve, each with a tension.
* Modes are Trigger, Sync and Envelope (one-shot), with delay, rise (a fade-in) and an RC smoother on
  the output (`TOP/dsp/LFO.cpp:11-102`).
* The pattern code uses `std::vector` and a mutex (`Pattern.h:64-66, 114-117`), so it isn't real-time
  clean. Take the ideas, not the code.

### C2. TetraOP against our `six_op.rs` and `fm.rs`

| | TetraOP | `six_op.rs` | `fm.rs` |
|---|---|---|---|
| Operators | 4 wavetable (`FmMatrix.h:19`) | 6 sine (`six_op.rs:36`) | 2 sine (`fm.rs:69-82`) |
| Wave | any table or noise, morphable | Plaits sine table, lerp (`:703-711`) | `f32::sin()` (`:56-58`) |
| Routing | 10 layouts plus a modulatable 4x4 matrix and RM | DX7's 32 algorithms compiled to opcodes (`:567-695`) | fixed 1→1 |
| Order | everything one sample late | serial within a sample (`:913-930`) | serial |
| Index | ≤ 1 turn per link | up to 32 turns (`:700-702`), DX7 level law `2^(−14+level)` (`:870`) | `2·timbre²·taming²` turns (`:127`) |
| Feedback | 1 raw sample | DX7 2-sample average, `2^fb/512` (`:906-917`) | one-pole-smoothed output, as phase *or* frequency feedback (`:144-165`) |
| Antialiasing | 2x the whole synth; mipmaps on carriers | none; it runs at 1x, as Plaits does | 4x with a 4-tap FIR down (`:26-29, 152-168`) and pitch taming |
| Envelopes | from the matrix | DX7 4-rate, 4-level per operator plus pitch EG (`:311-420`) | the track's own |
| Programmable | fully | presets only (`Harm` picks a patch, `:47`; `Timbre` is brightness, `Morph` envelope speed, `:819-829`) | 3 settings |

**What it means:**

* Our two FMs are a faithful DX7 player and a Plaits 2-op. Neither lets you *build* an FM sound,
  which the M8's FM synth and the Digitone are about.
* TetraOP is the best local model for that (port #3).
* Don't take its weaknesses: the single-sample feedback, and no index taming. Instead use our DX7
  averaging, `fm.rs`'s taming, and our `blocks/oversampler.rs` 2x halfband.

**The real cost of port #3 is the interface.** It needs about 14 settings or more: the algorithm or
matrix, 4 ratios, 4 levels, feedback, and 4 waves. `Settings` carries 8 (`virtual_analog.rs:32-43`:
`harmonics`, `timbre`, `morph`, `accent` and `further[4]`). Per the "limits don't drop features"
rule, that means reopening the Sound page (more rows or a second page), not dropping operator controls.

### C3. Surge's oscillators, one by one

Surge runs every oscillator at 2x (`OSC_OVERSAMPLING = 2`, `SXT/globals.h:53-54`) and decimates with a
halfband. Keep that in mind when comparing its aliasing with our 1x voices.

* **Classic**: BLIT. A windowed-sinc impulse at 256 sub-sample steps, with a first-order Taylor term
  per step (`ClassicOscillator.cpp:29-141`), run through a ring-buffer convolution state machine.
  * Saw-to-pulse shape, two widths, sub, sync, up to 16 unison voices and drift.
  * Superb under FM, but about 1000 lines, a stateful buffer and sinc tables.
  * Its musical value is the unison and the shape morph, which port #2 gets more cheaply. **Not
    recommended.**
* **Modern**: second-order DPW, computed afresh every sample so it holds up under FM and sync
  (`ModernOscillator.cpp:24-120`).
  * Saw, pulse and a "multi" wave (triangle, square or a parabolic "sine") mixed, with a sub, sync,
    unison, drift and a character filter (`:304-615`).
  * DPW-2 aliases about as much as our polyBLEP-2, so it doesn't beat `virtual_analog.rs`. What it
    has that we lack is the unison and drift, which port #2 covers. **Not recommended as a port.**
* **Wavetable**: BLIT wavetables with `.wt` loading. We already have Plaits' integrated-wave
  wavetables (`wavetable.rs:1-6`), cheap and alias-safe. **Not now.**
* **Window**: reads a wave at a "formant" ratio, under a window that runs at the fundamental (9
  windows, `WindowOscillator.cpp:297-380`). Fixed-point, mipmapped. This is windowed-sync or VOSIM
  formant synthesis, and it overlaps `grain.rs` (Plaits' grainlet formants). **Maybe later**,
  medium-sized.
* **Sine**: 28 quadrant shapes from sin and cos, feedback (squared when negative), unison, and
  low-cut and high-cut filters (`SineOscillator.cpp:30-130, 770-845`). The shapes are worth more as
  operator waves (tricks 11 and 12) than as a voice.
* **FM2 and FM3**: a carrier with 2 ratio modulators (FM2 adds a Hz offset between them,
  `FM2Oscillator.cpp:102-115`), or 2 ratio modulators plus one fixed-frequency modulator (FM3,
  `FM3Oscillator.cpp:100-170`). Depth is `32π·d³`. Both use sst quadrature oscillators (a rotating
  phasor, no `sin()`). `fm.rs` covers them; take tricks 11 and 15.
* **String**: two delay lines seeded alike, with a mix of two taps, feedback through a tone filter
  and a soft clip (`StringOscillator.cpp:26-42`).
  * 16 exciters: Burst noise, pink, sine, ramp, triangle, square and sweep, the same as Constant,
    plus audio in (`:51-90`).
  * Feedback runs 0.85..1.0, and **negative** in extended range (`:620-690`). A negative loop gain
    gives odd harmonics only, like a closed tube; the delay must halve to keep the pitch.
  * The pitch compensation for the tone filter is an empirical table (`:405-440`). It was tuned at
    Surge's 2x rate, so it must be measured again at ours.
  * The delay line is `SSESincDelayLine<16384>` ×2 (`:135-136`). Use our Hermite read instead
    (`string_model.rs:227-231`).
  * **Memory:** about 4096 ×2 f32 (32 KB) would cover 20 Hz at 48 kHz. Our voices are inline enum
    variants (`VT/vt-engine/src/voice.rs:100-120`), so the enum grows to its largest variant; today
    the string's is `LINE = 1024` (`string_model.rs:34`). Either box it when the track is built, or
    accept 32 KB a track. That's port #4.
* **Twist**: Plaits. We already have 13 of its 16 engines; it adds only Speech (see the side finding).
* **Alias**: u32 phase; the top 8 bits XORed with a mask, with a threshold (triangle fold, or the
  pulse's level, or the noise's rate), a wrap multiplier, and a bitcrush (`AliasOscillator.cpp:307-400`).
  * Waves: Sine, Ramp, Pulse, Noise, TX2–TX8, "memory" waves read from the synth's own state (skip
    those), and Additive (`AliasOscillator.h:42-67`).
  * It has its own 8-bit random generator (`AliasOscillator.h:110-130`).
  * Run it at 1x and it aliases more than Surge's, which is the point. Or put it through our 2x
    halfband for Surge's exact sound. That's port #1.
* **S&H noise**: pitched sample-and-hold of correlated noise, with width, sync and filters
  (`SampleAndHoldOscillator.cpp:61-175`). It overlaps `noise.rs` (Plaits' clocked noise); take
  trick 19.
* **Audio Input**: vimtracker has no live input in the voice path. Skip it.

### C4. Surge's modulators against our mod slot types

* **LFO** (`LFOModulationSource.cpp`): each shape has a deform with up to three deform types.
  * Type 1, `bend1`, applies a quadratic twice (`:111-120`).
  * Type 2, `bend2`, adds a sine (`:122-128`).
  * Type 3, `bend3`, adds an offset sine (`:131-139`).
  * The deform sets Square's width (`:984-989`), S&H's slew or linear-to-cubic blend (`:991-1027`),
    Noise's correlation (`:406-433`), and the Envelope shape's power curve or noise (`:827-870`).
  * **What fits our LFO type:**
    * Type 1 for every shape, plus Square's width: stateless, and one cell (trick 3).
    * The 4-point cubic for `Smooth` (trick 1).
  * **What doesn't:** correlated noise is stateful, and our LFOs are a pure function of the song's
    position (`lfo.rs:316-321`).
* **Step sequencer**: its deform is the model for Steps' `Smooth` (trick 4). The per-step trigger
  mask (`SurgeStorage.h:909`) is a possible later Steps feature: a step that retriggers the envelopes.
* **MSEG**: up to 256 segments (`max_msegs`, `SurgeStorage.h:912`).
  * Segment types are Linear, Quad Bezier, SCurve, Sine, Stairs, Brownian, Square, Triangle, Hold,
    Saw, Bump, Smooth Stairs and Ratchet 1–8 (`:931-954`).
  * Each segment has its own deform. Loop modes are One-shot, Loop and Gated Loop; edit modes are
    Envelope and LFO (`:956-980`).
  * At about 14 KB a slot it doesn't fit the budget of about 120 B a slot (`mod-slots.md:295`).
  * An 8-point MSEG-lite (about 32 B) as a later type is reasonable (port #6). Take Bump
    (`MSEGModulationHelper.cpp:836-851`) and Ratchet (`:1061-1105`) as its segment types, and
    TetraOP's tension per point.
* **Formula**: Lua. **Rejected**; it allocates and is JIT-compiled, not real-time safe.
* **Envelope** (`ADSRModulationSource.h`):
  * *Digital* mode: attack sqrt, linear or square (`:283-296`); decay with a curve (`:303-348`);
    release `phase^(n+1)` (`:356-372`).
  * *Analog* mode: an RC capacitor charged toward 1.5 V and switched to decay at 1 V, discharging
    toward S² (`:159-245`).
  * The design already chose one curve per envelope (#46), so add *Analog* as a 4th curve (trick 8)
    and the curve-inverting retrigger (trick 7). Skip the digital mode's curve per stage.
  * Surge runs its envelopes once a 32-sample block, as our `modulate` does (`LFO_UPDATE_FRAMES = 32`,
    `VT/vt-engine/src/engine.rs:45`). Nothing to change there.

### C5. sst-basic-blocks, piece by piece

| Header | Licence | Use |
|---|---|---|
| `dsp/OscillatorDriftUnisonCharacter.h` (`DriftLFO`, `UnisonSetup`, `CharacterFilter`) | GPL-3 | tricks 5 and 6. `CharacterFilter` is a 5 kHz one-pole low-pass ("warm") or high-pass ("bright"), trivial and probably unnecessary beside our track filters. |
| `modulators/SimpleLFO.h` | GPL-3 | the same deform maths as the LFO (trick 3); block-based with a switch point mid-block |
| `dsp/CorrelatedNoise.h` | GPL-3 | trick 19 |
| `dsp/EllipticBlepOscillators.h` | **GPL-3 or MIT** | port #5: saw, triangle, triangle-to-saw, pulse and two approximate sines, with order-1 (step) and order-2 (slope) BLEPs; scalar, no oversampling |
| `dsp/LanczosResampler.h`, `dsp/HilbertTransform.h` | **GPL-3 or MIT** | not needed here |
| `dsp/FastMath.h` (`fastsin`, `fastcos`, `clampToPiRange`) | GPL-3 | trick 2. Prefer our own `sine_table.rs`. |
| `modulators/` (`ADSREnvelope`, `AHDSRShapedSC`, `DAHDEnvelope`, `StepLFO` and more) | GPL-3 | listed on GitHub but not read; the Surge-local copies above were enough |

### C6. Notes on porting

* **Determinism:** every random source (drift, Alias's 8-bit generator, unison phases, correlated
  noise) must be seeded per note from the song, as `string_model.rs:95` is. Surge and TetraOP use
  `rand()` (`TOP/engine/OSC.cpp:51`, sst `DriftLFO::init`).
* **SIMD:** Surge's SSE is across unison voices and TetraOP's across polyphonic voices; neither
  layout carries over to us. Use `f32x4` across unison voices (port #2) or operators (port #3), or
  plain scalar code. The Alias loop is integer and scalar anyway.
* **Allocation:** nothing in ports 1–5 needs a heap at play time. The String's lines must be sized
  when the track is built.
