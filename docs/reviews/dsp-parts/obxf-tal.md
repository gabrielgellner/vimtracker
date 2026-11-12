# OB-Xf and TAL-NoiseMaker against vimtracker's analog-style DSP

Research only. Nothing was changed in either repo. Throwaway measurement programs are in the
scratchpad (`aliasbench/`, `svfbench/`, `tables/`), next to this file.

Paths are written `OBXF/` = `/Users/gabrielgellner/dev/dsp/OB-Xf/src/engine/`,
`TAL/` = `/Users/gabrielgellner/dev/dsp/tal-noisemaker/src/`, `VT/` =
`/Users/gabrielgellner/dev/vimtracker/crates/vt-engine/src/`.

## Bottom line

* **Licenses.** OB-Xf is **GPL-3.0-or-later**, so its code can be ported and adapted into
  vimtracker if the notice is kept. TAL-NoiseMaker's DSP is **GPL-2-only**, which is **not
  compatible** with GPL-3. We can learn from it and reimplement its ideas, but we can't copy or
  translate its code or tables.
* **The one big win is OB-Xf's oscillator anti-aliasing.** Measured on a saw, its table BLEP keeps
  aliasing **84 to 96 dB** under the fundamental and is flat to 20 kHz. Our Plaits two-point
  polyBLEP gets only **21 to 35 dB**, and it rolls the top off (-1.3 dB at 10 kHz, -2.9 dB at
  15 kHz). It costs about 3x our oscillator's CPU, which is still well under 1% of a core for every
  track. A table I generated myself (a 32-tap Blackman-windowed sinc) matched OB-Xf's to 0.3 dB, so
  we don't even need its data.
* **Second: a Juno chorus type, reimplemented from the idea.** Our chorus is Rings' long-delay
  chorus at 16.7 and 25 ms. The Juno sound (short 1 to 5 ms delays, a triangle LFO, the two sides
  in antiphase, dark filtering) is missing.
* **Third: OB-style "slop"** for the virtual analog voice. These are cheap per-note random offsets:
  oscillator tuning, start phase, cutoff, level and envelope time, plus a tiny per-sample pitch
  jitter. All of it is GPL-3-or-later in OB-Xf, and all of it is a few lines each.
* **The OB-X 2-pole SVF's diode-pair nonlinear damping** is a few lines. It gives our clean SVF
  self-oscillation that is stable and in tune, at the base rate, with no oversampling (measured).
* **Not worth taking:** OB-Xf's 4-pole (our Mystran ladder is better), all of TAL's filters, TAL's
  reverb (worse than our Griesinger and Dattorro), TAL's ADSR, LFO and portamento, OB-Xf's LFO,
  and OB-Xf's HQ 2x mode.

---

## 1. Licenses (exact wording)

### OB-Xf: GPL-3.0-or-later, compatible

* `/Users/gabrielgellner/dev/dsp/OB-Xf/LICENSE` is the full text of "GNU GENERAL PUBLIC LICENSE
  Version 3, 29 June 2007".
* Every file in `src/engine/` has the same header, for example `OBXF/Filter.h:2-14`:
  > OB-Xd was originally written by Vadim Filatov, and then a version was released under the GPL3
  > at https://github.com/reales/OB-Xd. ... This repository is a successor to OB-Xd version 2.11.
  > ... OB-Xf is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later).
* The README says: "OB-Xf is GPLv3 software".
* This matches the precedent in `docs/designs/decisions.md` 61 and 71, where GPL-3-or-later
  sst-filters and Vital code were ported with their notices kept.
* **Third-party code inside the engine:**
  * `OBXF/Decimator.h:22-24` says "MusicDsp / T.Rochebois". It's a musicdsp.org snippet with no
    stated licence. We don't need it, since our `blocks/oversampler.rs` covers this.
  * `OBXF/Noise.h` pink noise is "Adapted from Phil Burk's copyleft code". We don't need it.
  * `libs/` holds git submodules that aren't checked out here: JUCE, sst-*, simde, fmt,
    MTS-ESP, pybind11, catch2, clap-juce-extensions and melatonin_inspector. Nothing recommended
    below touches them.
* **The BLEP tables are OB-Xd's own, not TAL's.** OB-Xf's (`OBXF/BlepData.h:22-30`) is a
  linear-phase table: 16-sample half-width, 64 phases, 65 rows of 32. TAL's is a 32-tap
  minimum-phase table, 2048+ values that rise from 0 to 1. I extracted both and they share no
  data.

### TAL-NoiseMaker: GPL-2-only in its own DSP, NOT compatible

Every TAL-authored DSP file, for example `TAL/Engine/BlepData.h:1-21`, `TAL/Engine/FilterLp24db.h`,
`TAL/Engine/OscSaw.h`, `TAL/Effects/Chorus/Chorus.h` and `TAL/Effects/Reverb/Reverb.h`, says:

> This file is part of Tal-NoiseMaker by Patrick Kunz. Copyright(c) 2005-2010 Patrick Kunz, TAL
> Togu Audio Line, Inc. ... This file may be licensed under the terms of of the
> **GNU General Public License Version 2** (the ``GPL'').

* **There is no "or (at your option) any later version".** Under GPL-2 §9, a program that names a
  version without "any later version" is under that version only. So these files are GPL-2-only.
  GPL-2-only code can't be combined into a GPL-3 work. The FSF lists GPLv2-only as incompatible
  with GPLv3.
* The only files that do say "either version 2 of the License, or (at your option) any later
  version" are the JUCE-template plugin wrappers: `TAL/TalCore.h:9-11`, `TAL/TalCore.cpp`,
  `TAL/TalComponent.h` and `TAL/includes.h`. Their headers credit Raw Material Software (JUCE).
  `TAL/JucePluginCharacteristics.h:9-10` is "GNU General Public License (Version 2)". **None of
  these files contains DSP.**
* **Files with no licence header:**
  * `TAL/Engine/Lfo.cpp` and `Lfo.h` say "@author Remy Muller, 20030822".
  * `TAL/Engine/Decimator.h` is T. Rochebois's code from musicdsp.
  * `TAL/Engine/Interpolatorlinear.h` has no header at all.
  * The licence of these three is unknown; treat them as not usable.
* **Other bundled material:**
  * `vstsdk2.4/` is Steinberg's proprietary SDK.
  * `juce/` is a newer JUCE, for example `juce/modules/juce_core/juce_core.h`, which is ISC.
  * `docs/` holds `Antti_CMJ.pdf` (Huovilainen's paper), `EffectDesignPart1.pdf` (Dattorro's
    paper) and `blesser-patent.pdf`. These are papers. Their ideas are free to use; the PDFs are
    not ours to ship.
* **Consequence:** every TAL item below is marked **idea only**. It must be reimplemented from the
  idea or from public sources (papers, circuit analyses), not translated line by line. Constants
  that are TAL's own tuning choices should be re-derived or fitted against a reference rather
  than copied. One option is to ask Patrick Kunz for a GPL-3 relicence of specific files, but
  nothing here needs it.

---

## 2. Ranked recommendations (wholesale ports and additions)

### R1. A table BLEP/BLAMP oscillator core for the virtual analog voice

| | |
|---|---|
| **Adds or replaces** | Replaces the two-point polyBLEP (`VT/dsp/voices/virtual_analog.rs:83-105`, used by `SyncedSquare` at 201-281 and `VariableSaw` at 322-374) |
| **From** | OB-Xf `OBXF/SawOsc.h:44-117`, `PulseOsc.h:48-201`, `TriangleOsc.h` (BLAMP), `BlepData.h:22-30` (tables), `DelayLine.h:43-50` |
| **Port size** | About 200-300 lines of Rust. The tables are 4 × 2080 floats; we can generate them ourselves in about 20 lines (below) instead of embedding OB-Xf's. |
| **Risk** | Medium. It changes the VA's sound: brighter top, much less aliasing. It adds 16 samples (0.33 ms) of latency. The Plaits `this_sample`/`next_sample` structure becomes a 32-sample ring. Anything sample-exact in tests moves by 16 frames. |
| **License** | GPL-3-or-later if ported. **No licence question at all** if we generate our own table, which I verified reproduces OB-Xf's quality. |

**Measured** with a 48 kHz saw, a 65,536-point FFT and exact-bin fundamentals. The figures are the
alias power summed under 20 kHz, and the worst single alias, both relative to the fundamental
(`aliasbench`):

| f0 | naive | **ours (2-pt polyBLEP)** | **OB-Xf table BLEP** | TAL minBLEP | own table, d=16 | own table, d=8, cutoff 0.9 |
|---|---|---|---|---|---|---|
| 1 kHz | -14.8 / -28.9 | **-34.5 / -40.1** | **-96.1 / -99.1** | -73.6 / -88.8 | -96.4 / -99.4 | -73.1 / -73.3 |
| 2 kHz | -11.7 / -22.9 | -30.8 / -34.0 | -90.3 / -92.0 | -70.5 / -82.4 | -90.6 / -92.3 | -66.9 |
| 4 kHz | -8.5 / -16.9 | -26.5 / -27.8 | -85.6 / -86.2 | -67.3 / -76.4 | -85.9 / -86.4 | -60.9 |
| 6 kHz | -6.8 / -14.0 | -26.1 / -26.6 | -87.6 / -89.3 | -65.7 / -76.3 | -87.8 / -89.3 | -87.7 |
| 10 kHz | -4.5 / -9.5 | -21.3 / -21.4 | -84.0 / -84.8 | -63.7 / -72.2 | -84.1 / -84.9 | -84.1 |

**Harmonic accuracy** at f0 = 1 kHz, as each harmonic's level against an ideal saw's 2/(πk):

| | 5 kHz | 10 kHz | 15 kHz | 18 kHz | 20 kHz |
|---|---|---|---|---|---|
| Ours (polyBLEP) | -0.31 | -1.26 | -2.89 | -4.23 | -5.29 |
| OB-Xf | 0.00 | 0.00 | 0.00 | 0.00 | -0.01 |
| TAL | 0.00 | 0.00 | 0.00 | -0.01 | -0.01 |

So ours is audibly duller **and** aliases more.

**CPU** per oscillator-sample on an Apple M5, from a naive scalar transcription:

| | Run 1 | Run 2 |
|---|---|---|
| Polyblep | 4.0 ns | 1.45 ns |
| OB-Xf | 13.5–16.4 ns | 4.4–5.1 ns |
| TAL | 12–21 ns | 5.7–7.1 ns |

That's about 3x ours. In absolute terms it's 2 oscillators × ~3.5 ns extra × 48,000 × 16 tracks,
or about 5 ms of CPU a second, about 0.5% of a core. Most of the cost is the per-sample ring and
delay line, not the 32-tap adds, which happen only on edges.

**How it works.** OB-Xf delays the naive waveform by `B_SAMPLES = 16` (`DelayLine::feedReturn`,
`OBXF/DelayLine.h:43-50`). On each discontinuity it mixes a symmetric, linear-phase residual into
a 32-slot ring "into the past and future" (`mixInImpulseCenter`, `OBXF/SawOsc.h:87-108`). It
interpolates between the two nearest of 64 sub-sample phase rows. Triangle corners use a BLAMP
table the same way (`OBXF/TriangleOsc.h`, `mixInBlampCenter`, scale `4 * B_SAMPLES * delta`).

**PWM** uses the same moving-width edge time as ours: `sum = delta - (pw - pwWas)` at
`OBXF/PulseOsc.h:50` matches ours at `VT/dsp/voices/virtual_analog.rs:254` and `:351`.

**Generating our own table** (verified in `aliasbench/src/main.rs`, `make_table`):
1. Take a Blackman-windowed sinc over ±16 samples at 64 phases.
2. Integrate it.
3. Normalise it.
4. Subtract the unit step.

A half-width of 8 at a cutoff of 0.9 × Nyquist halves the work and the latency (8 samples) and
still gets 61 to 88 dB, losing 0.9 dB at 18 kHz.

**Scope.** Do the VA voice first. `this_blep` and `next_blep` are shared by chord, swarm,
waveshaping, noise, grain, variable_shape and acid (`grep this_blep VT/dsp/voices`), so a shared
`blep` block could replace them later. It would also let `MAX_FREQUENCY` rise from 0.25
(`virtual_analog.rs:25`) toward OB-Xf's 0.45 (`OBXF/OscillatorBlock.h:178`). That matters for
hard sync, which today stops rising at 12 kHz.

### R2. A Juno-style chorus type

| | |
|---|---|
| **Adds** | A `ChorusType` beside `Chorus` and `Ensemble` (`/Users/gabrielgellner/dev/vimtracker/crates/vt-model/src/insert.rs:82-88`), probably `Juno I`, `Juno II` and `Juno I+II` |
| **From** | The idea from TAL `TAL/Effects/Chorus/Chorus.h:60-150`, `ChorusEngine.h:83-111`, `OnePoleLP.h:40-45`, `DCBlock.h:38-44`, cross-checked against public Juno-60 circuit write-ups |
| **Port size** | About 80-120 lines in `VT/dsp/effects/chorus.rs`, plus a model enum entry |
| **Risk** | Low. It's a new type, so existing songs are untouched. |
| **License** | **Idea only** (TAL is GPL-2-only). Write it from the description below, and fit delay range, rates and tone against a Juno reference recording, as decision 137 does for filters. |

**What TAL does:**
* Two delay lines, one per side.
* The delay is `(lfo*0.3 + 0.4) * 7 ms`, so 0.7 to 4.9 ms, moved by a **triangle** LFO
  (`Chorus.h:108`, `nextLFO` at 137-149).
* **Chorus I** runs at 0.5 Hz and **Chorus II** at 0.83 Hz. Left and right start at opposite
  phase, so they move in **antiphase** (`ChorusEngine.h:85-88`).
* Each read is **first-order allpass interpolated**: `out = x[n-1] + (1-frac)*(x[n] - z1)`
  (`Chorus.h:120-122`). That's Dattorro's interpolation, from a public paper.
* A fixed one-pole low-pass follows. Its coefficient is `p = (0.95·0.98)^4 ≈ 0.75`, about a 2 kHz
  corner at 44.1 kHz, and it isn't scaled for the sample rate (`Chorus.h:125`, `OnePoleLP.h:42`).
* Then a DC blocker (`ChorusEngine.h:99-107`).
* The output is `dry + 1.4·wet` (`ChorusEngine.h:109-110`).
* I+II simply sums the two choruses (`ChorusEngine.h:95-108`). As far as I know, the real Juno-60
  I+II is a distinct faster, shallower mode instead, so verify this against a reference.

**Ours:** the Rings chorus. It sums both sides to mono into one line
(`VT/dsp/effects/chorus.rs:147`). Its taps are at 1200 and 800 frames, which is **25 and 16.7 ms**
at 48 kHz (`:37`). Its wobble is a sine at about 0.2 and 0.26 Hz (`:41`), read with linear
interpolation (`:116-124`). That is closer to a slow doubler. The short-delay, faster,
stereo-antiphase Juno shimmer is a different and very familiar sound.

**CPU:** the Juno type is cheaper than ours. It needs no `sin()`, where ours calls it 4 to 6 times
a frame (`:141`, `:150-153`, `:165-176`). It reads two delays and runs a triangle, a one-pole and a
DC blocker per side.

### R3. Analog "slop" for the virtual analog voice (a `Drift` setting)

| | |
|---|---|
| **Adds** | A per-note random offset set and per-sample jitter on `VirtualAnalog` (`VT/dsp/voices/virtual_analog.rs:377-459`). `Settings.further` (`:41`) already gives a VA four more places, so the model needs no new struct. |
| **From** | OB-Xf `OBXF/OscillatorBlock.h:160-175, 231-236`, `OBXF/Voice.h:200-220, 350, 427-431`, `OBXF/VoiceControlQuad.h:385-395, 490, 660, 671`, `OBXF/SynthEngine.h:388-392, 600-615` |
| **Port size** | About 30-60 lines in the voice, plus a model and UI setting |
| **Risk** | Low. Use the existing seeded noise (`VT/voice.rs:323-334`, `seed_noise`) so renders stay repeatable. Default it to 0 so the sound doesn't change unasked. |
| **License** | GPL-3-or-later. The techniques are also just ideas. |

OB-Xf's model of "analog", in order of audibility:
1. Static per-voice, per-oscillator tuning offsets.
2. Free-running random start phases.
3. Per-voice cutoff, level, envelope-time and portamento-time offsets.
4. White per-sample pitch jitter (±0.05 st).
5. A tiny cutoff jitter.
6. A noise floor that is always present.

**There is no slow random-walk drift** in either synth. For a monophonic tracker voice the per-voice
offsets become **per-note** offsets, re-rolled at note-on, as TAL does with its detune
(`TAL/Engine/SynthVoice.h:165-168, 245, 493`). The details are under Tricks T2-T5.

### R4. An OB-X 2-pole SVF behaviour on our clean SVF (nonlinear damping)

| | |
|---|---|
| **Adds or replaces** | Either the saturating path of the clean types (`VT/dsp/filters/filter.rs:407-416`, `tick_driven`, a `soft_clip` on `ic1` at 2x), or a self-oscillating range above today's `CLEAN_MOST_Q = 10` (`filter.rs:67`) |
| **From** | OB-Xf `OBXF/Filter.h:111-171` (`setResonance`, `diodePairResistanceApprox`, `resolveFeedback2Pole`, `apply2Pole`) |
| **Port size** | About 30-40 lines. It reuses our `c.g`, so it avoids OB-Xf's per-sample `tanf` at `Filter.h:146`. |
| **Risk** | Low to medium. Decision 137 fits the clean types against The Drop's OSR and Arturia's Filter SEM, so any change needs a re-fit there. OB-X's 12 dB filter is SEM-derived, so it's the same family. |
| **License** | GPL-3-or-later |

Measured by transcribing `apply2Pole` (`svfbench`). At full resonance with the "push" on, it
self-oscillates with a **bounded** amplitude of about 5.9 and stays **in tune**:

| Cutoff | Rings at |
|---|---|
| 1 kHz | 1001 Hz |
| 5 kHz | 5032 Hz (+11 cents) |
| 15 kHz | 15154 Hz (+17 cents) |

That's at the base rate, with no oversampling, and it never blew up. Without the push it sits at
the edge of oscillation. Its gain falls only slightly with level (3.57 → 2.73 dB from a saw at
0.25 to one at 4), which is the classic OB/SEM "squelch" where resonance tames on loud input.
Details are under T9.

### R5. An OB-style two-oscillator voice: cross-mod, sync, ring and noise (builds on R1)

| | |
|---|---|
| **Adds** | A new `SynthVoice`: two oscillators each saw, pulse or triangle, with osc2 synced to osc1, osc1 → osc2 exponential cross-mod, ring mod and coloured noise. It's an "OB" machine for the track filter to shape. |
| **From** | OB-Xf `OBXF/OscillatorBlock.h:171-300`, `Noise.h` |
| **Port size** | About 300-400 lines on top of R1 |
| **Risk** | Medium. It's a new voice, but per-sample pitch needs an `exp2` per sample for cross-mod. Our voices move frequency in a line per 16-frame block (`virtual_analog.rs:117-153`). |
| **License** | GPL-3-or-later |

Our VA (Plaits) has sync on the square and a detuned saw, but no cross-mod, ring or triangle sync.
The useful details:
* Cross-mod is osc1's output in **semitones** added to osc2's pitch (`OscillatorBlock.h:235`).
* Osc2's pitch is put through the same 16-sample delay as the BLEP output, so the FM lines up
  (`:231`, comment "this will give us less aliasing on crossmod").
* The sync flag and fraction are delayed too (`:212-213`), and osc1 is delayed before the ring
  mod (`:274`).
* Mix gains include a permanent `0.0006` noise (`:297`).

### R6. Xpander pole-mix modes for the ladder (data only)

| | |
|---|---|
| **Adds** | More `FilterMode`s for `Ladder` |
| **From** | `OBXF/Filter.h:29-44`, 15 rows of five weights on `[input, pole1..pole4]` |
| **Port size** | About 10 lines of data, plus UI names |
| **Risk** | Low. Our ladder already mixes `[input·slope, s1..s4]` by weights (`VT/dsp/filters/filter.rs:72-78`, used at `:459-464`), so the rows drop straight in. Each needs a makeup and a check against the response drawing. |
| **License** | GPL-3-or-later. These are binomial pole mixes, effectively math. |

The new mixes would be LP3, LP1, HP3, HP1, BP2 from four poles, N2, PH3 (a phaser), HP2+LP1,
HP3+LP1, N2+LP1 and PH3+LP1. Decision 72 keeps `Mode` shared across types, so this is mainly a
UI question.

### R7. Unison: not recommended as a port

* OB-Xf's unison stacks whole voices per key (`OBXF/Motherboard.h:328`). Each voice gets its static
  random `tuningSlop × unisonDetune` (`OBXF/OscillatorBlock.h:175, 236`; detune range
  `logsc(0.001..1)` semitone, `OBXF/SynthEngine.h:388-392`) and a per-voice pan
  (`OBXF/Motherboard.h:888`).
* Our tracks are monophonic: one voice and one tail (`VT/track_state.rs:635-636`). The synth path
  is mono until panning (`VT/voice.rs:517`, `[sample, sample]`).
* Unison would therefore mean N oscillator pairs inside the VA voice, plus a stereo synth path.
  That costs N× the CPU and widens the voice interface.
* `Swarm` (8 saws, `VT/dsp/voices/swarm.rs:1-3`) already covers the supersaw-ish use.
* Revisit only if the user asks for a stereo unison VA.

### Not worth taking

* **OB-Xf 4-pole** (`OBXF/Filter.h:174-241`). It's four TPT one-poles, with an `atan` on the first
  state only (`:200`), feedback `3.5·res` (`:114`), and a sample-rate "resCorrection" (`:103-108`).
  Our ladder is better modelled: Mystran per-stage linearisation, a zero-delay solve and 2x
  (`VT/dsp/filters/filter.rs:417-466`).
* **OB-Xf HQ mode.** It runs the whole voice at 2x into a 17-tap halfband
  (`OBXF/Motherboard.h:736-760, 905-907`; `Decimator.h`). R1 makes it unnecessary for the
  oscillators. Our filters oversample where they saturate.
* **OB-Xf LFO** (`OBXF/Lfo.h`). It blends sine/tri, saw/square and S&H/glide, computes every 8
  samples (`:38`) and smooths at 250 Hz (`:36`). Our stateless, tempo-true LFO
  (`/Users/gabrielgellner/dev/vimtracker/crates/vt-model/src/lfo.rs:321-355`) is already at least
  this good, and memory says to keep musical rate parameters.
* **TAL filters** (`TAL/Engine/FilterLp24db.h` and its siblings).
  * They're Huovilainen-style, but **only the feedback** saturates; the stages are linear
    (`:112-134`).
  * They run at **4x** with a *linear* upsampler and three halfbands (`FilterHandler.h:112-166`),
    so they're expensive.
  * The 12/18/6 dB variants only change taps and constants (`diff`: `:101-102`, `:141`).
  * Our ladder, diode, MS-20 and Dirty filters are all better. GPL-2-only anyway.
* **TAL reverb** (`TAL/Effects/Reverb/Reverb.h:188-231`). It's 4 modulated combs into 5 allpasses,
  a Schroeder/Moorer reverb, much plainer than our Griesinger (`VT/dsp/effects/reverb.rs`) and
  Dattorro plate (`VT/dsp/effects/plate.rs`).
* **TAL ADSR** (`TAL/Engine/Adsr.h`). Its coefficients come from `powf(value, 22..24)` curves and
  the "exp attack" is `value^5`. Ours is exactly timed. Only its attack *idea* is good (T1).
* **TAL portamento** (`TAL/Engine/Portamento.h:41-73`). It's a constant-rate linear glide in
  semitones. Ours is an RC, like OB-Xf's.
* **TAL LFO** (`TAL/Engine/Lfo.cpp:29-61`). It's a 256-point lookup table with a one-pole smoother
  that isn't scaled for sample rate. Its author and licence are unknown.

---

## 3. Tricks worth stealing (grafts onto our code, ranked by value per effort)

Each entry gives their code, our code, the gain, the size and the licence.

**T1. A concave RC attack ("analog attack") as an envelope curve.** *Value: high. Size: 3-5 lines.*
* **Theirs:**
  * OB-Xf charges exponentially toward 1 at a rate that reaches 90% at the attack time, then hands
    over to decay (`OBXF/AdsrEnvelope.h:31` constants, `:208-218` coefficient, `:254-262` step,
    `:178-205` retrigger). It also offers a lin/exp blend (`attackCurve`).
  * TAL's linear-mode attack is the same RC idea, aimed at an overshoot target `1.04 + 0.5·attack`
    (`TAL/Engine/Adsr.h:126`).
* **Ours:** `rise(t) = t^3.32`, the Peaks curve, which is **convex**: slow, then fast
  (`/Users/gabrielgellner/dev/vimtracker/crates/vt-model/src/envelope.rs:9-16`). On pads and
  plucks it sounds like a fade-in rather than an analog "bloom".
* **Graft:** `rise_rc(t) = (1 - exp(-k t)) / (1 - exp(-k))` with `k = ln 10 ≈ 2.30`. It's the same
  closed form as our `fall` (`envelope.rs:21-23`), lands exactly at 1 and keeps the times exact.
  Offer it as a curve choice, or make it the default.
* **Gain:** sound.
* **License:** GPL-3-or-later from OB-Xf, and the idea is a capacitor charging.

**T2. A free-running or random start phase per note for the VA's oscillators.** *Value: high.
Size: 5-10 lines.*
* **Theirs:** OB-Xf seeds random phases once (`OBXF/OscillatorBlock.h:167-168`) and never resets
  them on note-on, so the oscillators run free. Each note's attack and the osc1/osc2 phase
  relation differ.
* **Ours:** each note builds a new `VirtualAnalog::new()` (`VT/voice.rs:100`;
  `virtual_analog.rs:182-194, 298-309, 390-399`), so every note starts at phase 0. That's
  identical attacks, a "machine" sound.
* **Graft:** seed the square's, slave's and saw's `phase` from the voice's seeded RNG when Drift is
  above 0. Keep phase 0 at Drift 0, because kicks and basses want the consistent punch.
* **Gain:** sound. **License:** idea.

**T3. Static per-note tuning offsets per oscillator.** *Value: high. Size: about 10 lines.*
* **Theirs:** `tuningSlop` is one white value per oscillator in ±0.52 (`OscillatorBlock.h:164-165`).
  It's scaled by a detune amount in semitones and added to each oscillator's pitch (`:175`, `:236`;
  range `logsc(val, 0.001, 1)` st at `SynthEngine.h:390`).
* **Ours:** the saw's and square's frequencies (`virtual_analog.rs:421`, `:441`).
* **Graft:** multiply each by `2^(offset/12)`, worked out once per note, so it costs nothing per
  sample.
* **Gain:** sound, the beating between the oscillators that defines "analog". **License:**
  GPL-3-or-later, or the idea.

**T4. Per-sample pitch jitter.** *Value: medium. Size: about 10 lines.*
* **Theirs:** `oscPitchNoise` (fixed 0.1) × white noise in ±0.52 is added to every oscillator's
  pitch every sample, about ±0.05 st (`OscillatorBlock.h:112`, `:173`, `:232`). Integrated into
  phase, it becomes phase noise that keeps the oscillators' relation from ever locking.
* **Ours:** the inner loops take `fm.next()` per sample (`virtual_analog.rs:223-224`, `:340`).
* **Graft:** multiply by `(1 + 0.0578·jitter)`, a linearised `2^(x/12)`, with an LCG. That's two
  flops and an integer multiply-add per oscillator-sample.
* **Gain:** sound, subtle. **License:** idea.

**T5. Per-note cutoff, level, envelope-time and glide-time offsets.** *Value: medium. Size: 5-15
lines each.*
* **Theirs:**
  * One random value per voice for each (`OBXF/Voice.h:202-206`).
  * Cutoff offset up to ±9 st at full (`SynthEngine.h:601-605`, applied at
    `VoiceControlQuad.h:660`).
  * Level up to ±33% (`SynthEngine.h:611-615`, `Voice.h:350`).
  * Envelope times scaled by `1 ± 0.5·amount` (`Voice.h:427-431`, `AdsrEnvelope.h:88-97`).
  * Portamento time ±37% (`VoiceControlQuad.h:490`, `SynthEngine.h:606-610`).
* **Ours:**
  * Cutoff goes into `Coefficients::with_mode` in `VT/track_state.rs:1913-1917`.
  * Level is the voice gain at `VT/voice.rs:546`.
  * Envelope times come from `held`/`released` (`envelope.rs:83-106`).
  * Glide share is set at `VT/voice.rs:394-416`.
* **Gain:** sound. **Cost:** it reaches across track state as well as the voice, so it's slightly
  more work than T2-T4. **License:** idea.

**T6. Split the BLEP residual table at the step.** *Value: high, and needed for R1. Size: comes
with R1.*
* **Theirs:** OB-Xf stores one row per phase and *adds* the first half and *subtracts* the second
  (`OBXF/SawOsc.h:100-104`, layout comment at `BlepData.h:25-30`). The jump at τ = 0 is never
  linearly interpolated.
* I hit exactly this bug in my own table: when the residual is interpolated across the step,
  aliasing only reached -28 dB. Fixed, it reached -96 dB.
* **Gain:** stability and quality. **License:** idea.

**T7. Delay everything that "sees" the oscillator by the BLEP latency.** *Value: high, part of R1.
Size: about 10 lines.*
* **Theirs:** the sync flag and fraction (`OscillatorBlock.h:212-213`), the pitch fed to the
  follower (`:231`), osc1's output used for ring mod (`:274`), and the amp and filter envelopes
  (`OBXF/Voice.h:175`, cleared at note start at `:515-521`) all go through the same 16-sample
  delay.
* **Ours:** if R1 lands, the voice's envelope (`VT/voice.rs:525-530`) would otherwise run 16
  samples ahead of the audio. That's 0.33 ms, harmless for most sounds but it softens the first
  millisecond.
* **Optimisation (mine, not OB-Xf's):** add the naive sample into the residual ring at `pos + 16`
  instead of keeping a separate delay line. It saves one buffer per oscillator.

**T8. Diode-pair nonlinear damping in a TPT SVF.** *Value: medium-high. Size: about 15 lines.*
* **Theirs:** `OBXF/Filter.h:117-141`. The damping is
  `2·(1 - res + D(pole1·0.0876) - 1 - push·0.035)`, where `D` is an asymmetric quartic
  `1 + 0.05x + 0.185x² + 0.0092x³ + 0.0104x⁴`, a "slightly mismatched diode pair". It's evaluated
  on the **previous** band state and solved linearly within the sample. The odd terms give even
  harmonics.
* **Ours:** `Svf::tick_tuned` and `tick_driven` (`VT/dsp/filters/filter.rs:395-416`) use fixed
  damping and a `soft_clip` on `ic1` at 2x.
* **Graft:** compute the damping per sample from `ic1`, and recompute `a1 = 1/(1+g(g+k))`. That's
  one division per sample.
* **Gain:** sound. Resonance that tames on loud input, plus stable self-oscillation (measured
  above, R4) on the clean path without the 2x oversampler. **License:** GPL-3-or-later.

**T9. A small noise floor so resonance starts from silence.** *Value: medium. Size: 2-5 lines.*
* **Theirs:**
  * OB-Xf always mixes `0.0006 ×` noise (about -70 dBFS) into the oscillator sum
    (`OscillatorBlock.h:297`).
  * It adds a ±1.75 Hz white jitter to the cutoff (`VoiceControlQuad.h:385-395, 671`).
  * TAL injects `0.001·noise·(1-cutoff)` into the first stage and the cutoff
    (`TAL/Engine/FilterLp24db.h:108-114`).
* **Ours:** our filters' zero state with zero input stays at zero, so a ladder at full Res is
  silent until something plays.
* **Graft:** add a seeded noise of about -90 dBFS at the ladder's and diode's input
  (`filter.rs:516-568`, `saturating`).
* **Gain:** sound. Analog-like ringing from nothing, and a little protection against denormals;
  nothing in the engine flushes them (a grep found only `pedal.rs:93, 369`).
* **Caveat:** it must not keep the "open" bypass (`VT/track_state.rs:1905-1912`) or idle tracks
  from sleeping. Use the seeded RNG so renders stay exact. **License:** idea.

**T10. Per-sample (interpolated) filter coefficients.** *Value: medium. Size: about 15 lines.*
* **Theirs:** OB-Xf computes cutoff in pitch, exponentiates it and runs `tan` **every sample**
  (`VoiceControlQuad.h:654-671`, `Filter.h:146`, `:188`).
* **Ours:** coefficients are recomputed every 32 frames (`VT/track_state.rs:69`, `:1913-1917`) and
  held between. A fast filter envelope steps at 1.5 kHz, about 1/7 octave a step on a 5-octave,
  5 ms sweep.
* **Graft:** interpolate `g` and `a1` linearly across the 32 frames. TPT SVFs and ladders tolerate
  per-sample `g`. That's about 2 adds per sample, with no `tan`.
* **Gain:** sound (no stepping or zipper on snappy envelopes) at near-zero CPU. **License:** idea.

**T11. Allpass-interpolated delay reads for the chorus.** *Value: medium. Size: about 5 lines.*
* **Theirs:** `TAL/Effects/Chorus/Chorus.h:120-122`, and Dattorro's paper, which is public.
* **Ours:** the linear interpolation at `VT/dsp/effects/chorus.rs:116-124` low-passes by an
  amount that changes with the fraction. At frac 0.5 it nulls at Nyquist. As the LFO sweeps, that
  makes a wobbling dullness.
* **Graft:** allpass interpolation keeps the magnitude flat at the same cost. Each tap needs one
  `z1` state, and it suits slow modulation only.
* **Gain:** sound. **License:** idea.

**T12. Triangle LFO instead of `sin()` in the chorus.** *Value: low-medium. Size: about 5 lines.*
* **Theirs:** `TAL/Effects/Chorus/Chorus.h:137-149`, the Juno uses a triangle.
* **Ours:** `chorus.rs:141`, `:150-153` and `:165-176` make 4 to 6 `sin` calls a frame.
* **Gain:** CPU, plus Juno authenticity for R2.
* **License:** idea, and a triangle is trivial.

**T13. A 12 Hz DC block and a "brightness" low-pass between oscillators and filter.** *Value:
low-medium. Size: about 10 lines.*
* **Theirs:** `OBXF/Voice.h:382-383` with `dcScale` at `:459`, and `setBrightness`
  (`SynthEngine.h:496-500`, range 7-26 kHz).
* **Ours:** PWM and sync move the VA's DC, and that DC biases the asymmetric saturating filters
  (Diode, Dirty, the MS-20) differently as PW moves. A one-pole high-pass on the VA's output
  (`virtual_analog.rs:455-457`) fixes it.
* **Gain:** sound and stability. **License:** idea.

**T14. A recursive envelope instead of closed form per frame.** *Value: low. Size: about 30 lines.*
* **Theirs:** OB-Xf takes one multiply-add per sample (`AdsrEnvelope.h:254-290`).
* **Ours:** `held` and `released` cost a `powf` or `exp` plus two `u64` multiply-divides
  (`frames_of`) per frame during attack, decay and release (`envelope.rs:83-106`, `:122-125`;
  called from `VT/voice.rs:526`).
* **Assessment:** there's only one voice per track, so this is about 0.1% of a core. The closed
  form also gives exact times and stateless seeking. Keep ours. If needed, evaluate the closed
  form every 16 frames and interpolate.

**Things we already do as well as they do (no action):**
* PW narrowing as pitch rises. TAL has `pw = (pw-0.5)·(1-2.5fs)+0.5` (`TAL/Engine/OscPulse.h:119`);
  ours clamps to `[2f, 1-2f]` (`virtual_analog.rs:204-208`, `:324-328`).
* Moving-PW edge timing (above).
* Retrigger from the current level. OB-Xf has `AdsrEnvelope.h:178-205`; ours has
  `envelope.rs:86` with `Stage::Held { from }` at `VT/voice.rs:425-438`.
* Click-free voice steal. Ours fades (`VT/voice.rs:456-462`); TAL pre-settles its filter while
  idle (`TAL/Engine/SynthVoice.h:170-187`).
* Resonance passband compensation. OB-Xf multiplies by `(1+0.45·k)` (`Filter.h:240`); TAL has
  pre/post gains (`FilterLp24db.h:82-91, 141`). Ours is fitted with taper tables against two
  references (`filter.rs:84-148`, decision 137).
* RC portamento. OB-Xf runs a TPT one-pole in semitones (`VoiceControlQuad.h:624-628`); ours is
  an RC in log pitch (`VT/voice.rs:405-411, 554`).

**A "don't":** **minBLEP leaves a DC offset proportional to frequency.** TAL's minimum-phase table
delays the band-limited edge but not the naive ramp. Measured on a 2 kHz saw, the output's mean is
+0.137 on a ±1 scale; the polyBLEP and OB-Xf saws average about 0. If anyone considers minBLEP for
zero latency, it needs a DC correction. Linear-phase (R1) doesn't have this problem.

---

## 4. Details by synth

### OB-Xf

**Oscillators** (`OBXF/OscillatorBlock.h`, `SawOsc.h`, `PulseOsc.h`, `TriangleOsc.h`, and the
SIMD four-voice version in `OscillatorQuad.h`):
* Per voice: two oscillators, each saw, pulse, or triangle when neither is on (`:183-195`,
  `:251-262`); osc2 hard-synced to osc1 (`processFollower`); cross-mod in semitones (`:235`);
  ring mod (`:290`); and white, pink or red noise (`Noise.h`).
* Pitch is converted with `getPitch` **every sample** (`:173`, `:231`) and capped at 0.45·fs
  (`:178`, `:238`).
* **Aliasing:** best in class here, -84 to -96 dB, measured in R1.
* **Tuning:** exact. The ideal-saw RMS error is 0.009 at a 15-sample alignment
  (`aliasbench` comparison).
* **Latency:** 16 samples.
* **Sync:** BLEP on the follower's reset, with height `x - fracMaster` (`SawOsc.h:78-84`). The
  triangle uses both BLEP and BLAMP (`TriangleOsc.h`, `processFollower`). The follower's own edge
  is cancelled when the reset comes first ("De Morgan" logic, `SawOsc.h:64-77`).
* **CPU:** about 3x a two-point polyBLEP, measured. The engine runs four voices at once in SIMD
  (`OscillatorQuad.h`).

**Filters** (`OBXF/Filter.h`):
* **2-pole (OB-X):**
  * A trapezoidal SVF solved with a diode-pair nonlinear damping from the previous state
    (`:123-141`).
  * Outputs are LP→notch (`(1-m)·y2 + m·v`) or a LP→BP→HP blend (`:158-170`).
  * Resonance `res2Pole = 1 - res` (`:113`), and "push" adds 0.035 of negative damping (`:129`)
    for self-oscillation.
  * It calls `tanf` per sample (`:146`).
  * Measured: bounded, in-tune self-oscillation at 1x (R4).
* **4-pole:**
  * A TPT cascade with feedback `3.5·res`.
  * A single `atan` on stage 1's state, with a rate-dependent `resCorrection` (`:103-108`, `:200`).
  * A continuous LP4→LP3→LP2→LP1 morph (`:217-238`) or 15 Xpander mixes (`:29-44`, `:209-222`).
  * Volume compensation `1 + 0.45·k` (`:240`).
  * Cheaper and less accurate than our ladder.
* Cutoff is limited to `sr/2 - 120` (`VoiceControlQuad.h:513`), or 19 kHz with push
  (`:515`). There's a 12 Hz DC block and a "brightness" one-pole before the filter
  (`Voice.h:382-383`).

**Envelopes** (`OBXF/AdsrEnvelope.h`):
* RC-style throughout.
  * The attack is concave toward 1, ending at 0.9, optionally blended with linear (`:31`,
    `:226-236`, `:254-262`).
  * Decay and release are exponential, with coefficients from logs (`:70-74`, `:144`).
* It costs one multiply-add per sample.
* Quirks: sustain is capped at 0.9 (`:278`), and the attack "time" is the time to reach 0.9.
* Slop scales all times per voice (`:88-97`).
* Velocity, legato modes and retrigger are at `Voice.h:510-556`.

**Slop / analog variation:** listed in T2-T5. OB-Xf calls them Envelope, Filter, Portamento and
Level slop (`SynthEngine.h:600-615`), plus the always-on tuning slop, phase and pitch jitter.

**Unison:** see R7.

**LFO and HQ mode:** see "Not worth taking".

### TAL-NoiseMaker (all idea only)

**Oscillators:**
* `OscSaw.h`, `OscPulse.h` and `OscTriangle.h` are minimum-phase BLEP: 32 taps × 64-phase
  oversampling over a 2048-entry integrated table (`OscSaw.h:49-57`, `:125-138`). The triangle
  uses a sinc table and a 15-sample delay (`OscTriangle.h:139-205`).
* Measured: -64 to -74 dB of aliasing (the sum), flat to 20 kHz, about 4x a polyBLEP's CPU, and a
  frequency-proportional DC offset (T14's "don't").
* Code quality:
  * Each oscillator `new`s a `BlepData` and leaks it, then `delete[]`s a pointer to a *static*
    array in its destructor, which is undefined behaviour (`OscSaw.h:56-57`, `:66-67`).
  * Frequency is capped at 22 kHz regardless of rate (`OscSaw.h:90`).
* `Vco.h` holds osc1, osc2 and osc3. Osc3 is a sub pulse two octaves down that is also the sync
  master (`:84-88`, `:193`, `:207`).
* FM is *linear*, from a hidden sine at osc1's frequency: `freq += fm·10·fmFreq·(1+sin)`
  (`OscSaw.h:84-89`).
* There's a bitcrusher (`Vco.h:217-221`).
* Random detune is re-rolled each note but **multiplies the note number**, so detune grows with
  pitch (`SynthVoice.h:165-168`, `:493`). That's a bug.
* Not worth more than the ideas already noted.

**Filters:** see "Not worth taking". The cost per sample is 4 filter steps + a linear 4x upsample
+ 3 `Decimator9` calls (`FilterHandler.h:112-166`).

**Chorus:** see R2, T11 and T12.

**Reverb:** see "Not worth taking".

**ADSR, LFO and portamento:** see "Not worth taking".

### Ours, for reference

* **VA:** Plaits VA variant 2 (`VT/dsp/voices/virtual_analog.rs`): a synced variable square plus
  a variable saw.
  * Two-point polyBLEP and integrated BLEP (`:83-105`).
  * 16-frame blocks with settings moving in a line (`:22`, `:117-153`).
  * Capped at 0.25·fs (`:25`).
* **VCF voice:** Plaits' (`virtual_analog_vcf.rs:90-153`), two SVFs with soft clips.
* **Track filter:** nine types (`filter.rs`), with clean TPT SVFs (`:384-416`) and a 2x Mystran
  ladder (`:417-466`), all fitted to references (decision 137).
* **Chorus:** Rings' chorus and ensemble (`chorus.rs`).
* **Reverbs:** Griesinger (`reverb.rs`) and a Dattorro plate (`plate.rs`).
* **Envelopes:** closed-form Peaks curves (`/Users/gabrielgellner/dev/vimtracker/crates/vt-model/src/envelope.rs`).
* **Glide:** an RC in log pitch (`VT/voice.rs:394-416`, `:554`).
* **Voices:** one voice per track (`VT/track_state.rs:635-636`).

---

## 5. Method and caveats

* `aliasbench`
  (`scratchpad/aliasbench/src/main.rs`)
  transcribes four saws for measurement: naive; Plaits' two-point polyBLEP, as in
  `virtual_analog.rs:83-92`; OB-Xf's `SawOsc` with its extracted table; and TAL's `OscSaw` with
  its extracted table. It also has a self-generated windowed-sinc BLEP.
* `svfbench` (`.../scratchpad/svfbench/src/main.rs`) transcribes `Filter::apply2Pole`. I didn't
  test OB-Xf's 4-pole or TAL's filters numerically.
* Both use a radix-2 FFT, 65,536 points, rectangular window, exact-bin fundamentals at 48 kHz.
  Aliases are counted under 20 kHz.
* The CPU timings are rough single-thread micro-benchmarks and varied up to 3x between runs. Use
  the ratios, not the nanoseconds.
* I measured the saw only; pulse and triangle use the same machinery in each synth.
* The Juno-60 figures, beyond what TAL's code shows (the true I+II mode, the BBD tone), are from
  memory of public analyses. Check them against a reference before tuning.
