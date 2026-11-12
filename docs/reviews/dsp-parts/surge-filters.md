# Surge XT's filters against vimtracker's

**Where the source came from.** `../dsp/surge` has every submodule empty (`libs/sst/*` has 0 files; `git submodule status` shows `-` for each). I cloned the pinned commits into the scratchpad, outside both repos, and changed nothing in either repo:

- sst-filters `6bcf08bb8ecd177b161281e8147f30b10f0b5db7`
- sst-basic-blocks `7209f14567b37a0db6986c7cb1680089e97c8516`
- sst-waveshapers `dd12f31a5a9016c9895e52d1a00eee0e1eebe6ce`

The clones are in `scratchpad/sst/`. A path like `VintageLadders.h:173` means `sst-filters/include/sst/filters/VintageLadders.h`. `../dsp/OB-Xf/src/engine/Filter.h` is a local **scalar** copy of the OB-Xd filter, which makes that port easier.

**Context that changes the comparison.** Surge runs the whole voice, filters included, at 2x the host rate (`src/common/globals.h:53-54`, `SurgeVoice.cpp:924` sets the coefficient maker to `dsamplerate_os`). So every "clamp to 0.3·sr" in sst means 0.6 of the host's Nyquist, about 28.8 kHz at 48k: no clamp within the audio band. Every Surge filter also pays the 2x cost. We run the clean path at 1x and only the saturating types at 2x (`filter.rs:497-506`, `:517`). Our CPU baseline comes from decision 61: one driven channel, release mode, on an M5, per sample: `MS20 LP` 30 ns, `MS20 HP` 34, `Ladder` 49, `Diode` 59, driven `LP` 25. The Surge CPU figures below are my estimates from op counts against those numbers. I did not measure them.

**We already took one Surge filter.** `korg35.rs` is sst's K35 (decision 61), so K35/MS-20 is done.

---

## Ranked: parts worth taking

### 1. OB-Xd 2-pole: a 12 dB multimode SVF that self-oscillates (new filter family)
- **Source:** `OBXDFilter.h:174-255` (SIMD). It is easier to port from the scalar `../dsp/OB-Xf/src/engine/Filter.h:111-172`.
- **What it adds:** a filter we don't have, a 12 dB LP/BP/HP/Notch that sings. Our clean types top out at Q 10 and "never ring" (`filter.rs:67`, the comment at `:93-94`). Our driven SVF only soft-clips the band state (`filter.rs:407-414`).
- **How it works:**
  - It is a TPT/ZDF SVF whose damping depends on amplitude. `R = 1 - res` (`OBXDFilter.h:100`).
  - The damping term adds `poly(0.0876·s1) - 1` (`:193-221`, polynomial at `:174-191`), evaluated from the previous band state. `poly` is a Taylor fit of a mismatched diode pair.
  - So the resonance limits itself with no tanh, and the odd term (`0.05x`) makes it asymmetric, which gives even harmonics.
  - The "push" subtypes (`:106`, `:201`) subtract 1.035 instead of 1, so at full Res the damping goes negative at small amplitude and it self-oscillates.
  - Its modes are a continuous LP→Notch→HP morph, or LP→BP→HP with `bandpass` set (`:236-253`).
- **Quality:**
  - I simulated it at 96 kHz, our 2x rate (`scratchpad/sim/obxd.py`). With push at full Res it self-oscillates at 100 Hz −1.3 cents, 1 kHz 0.0, 5 kHz +5.3, 10 kHz +11.4 and 16 kHz +17.3 cents.
  - Its self-limited amplitude is about 4.3 (×0.74 output gain already applied), so it needs a level trim.
  - Without push at Res 1 it decays and doesn't sing.
  - Tuning is exact in the linear region (`g = tan(πf/fs)`).
  - It stays stable under fast modulation, like any TPT SVF. The nonlinearity is one sample late, but it only modulates damping, not pitch.
- **CPU:** one SVF tick, a 4-term polynomial and one division per step. At 2x that's about our driven `LP`, roughly 25-30 ns per channel.
- **Size:**
  - About 100 lines of DSP.
  - Coefficients and `response_db`: the small-signal response is the plain SVF with `k = 2R` (or `2(R − 0.035)`), so the curve is exact at low level, as ours are.
  - A `Res` taper fitted with `tools/compare` against The Drop/Filter SEM (decision 137's pairing), plus tests.
  - About 300 lines in all.
- **Risk:** low. The SIMD-to-scalar step is trivial, since OB-Xf is scalar already.
- **License:** sst-filters is GPL-3.0-or-later, and so is OB-Xf. The original OB-Xd was released under GPL-3 (OB-Xf `Filter.h:1-17`). The upstream `reales/OB-Xd` repo now holds only a README, so I couldn't read its headers myself.

### 2. The Xpander pole-mix table on our existing Ladder (new modes, nearly free)
- **Source:** `OBXDFilter.h:282-470`. The 15-row table is clearest in OB-Xf `Filter.h:29-45`.
- **What it adds:** LP6 and LP18, HP6 and HP18, a 12 dB BP2 beside our 24 dB band, N2, a 3-pole phaser (PH3), and HP+LP blends (HP2+LP1, HP3+LP1, N2+LP1, PH3+LP1).
- **Why it's cheap:**
  - Our `Ladder::step` already mixes `[u, s1, s2, s3, s4]` by a weight vector (`filter.rs:458-463`, constants `:72-78`). That is the same vector OB-Xd mixes, `[y0..y4]`.
  - `response_db` already folds arbitrary weights (`filter.rs:609-619`), so the curves come for free.
  - The work is new `FilterMode` variants (`vt-model/src/song.rs:170-188`), band-makeup levels like `Mix::new`'s (`filter.rs:305-345`), and list text.
- **CPU:** zero.
- **Size:** about 60-100 lines, mostly model and UI.
- **Risk:** low. Each mode needs level fitting, as `LADDER_BAND` got `BAND_MAKEUP`.
- **License:** the table is plain coefficients ((1−L)^n expansions), GPL-3+ in both sources.

### 3. Cutoff Warp and Resonance Warp: Jatin Chowdhury's nonlinear biquads (new "Warp" family)
- **Source:** `CutoffWarp.h:93-258` and `ResonanceWarp.h:55-181`.
- **What they model:**
  - Both are an RBJ biquad in TDF-II, 1 to 4 stages in series, with LP/HP/BP/N/AP.
  - **Cutoff Warp** saturates the output that feeds the poles (`CutoffWarp.h:98-120`). Louder signals shift the effective cutoff, which "warps" it.
  - **Resonance Warp** saturates both states (`ResonanceWarp.h:59-79`), which compresses the peak.
  - Saturators: fast tanh, cubic soft clip, or OJD's asymmetric piecewise clipper (`CutoffWarp.h:56-91`).
- **Quality:**
  - Tuning is exact, since the RBJ coefficients carry the bilinear warp.
  - Both are bounded by construction: feedback passes only through a saturator, so the states stay bounded under any modulation.
  - Neither self-oscillates. Q runs `reso³·18 + 0.1` (`CutoffWarp.h:155`, `ResonanceWarp.h:113`), so Res 0 is Q 0.1, a drooping passband. Our taper would replace that.
  - Level uses a hand-tuned table (`CutoffWarp.h:178-200`), which needs fitting again here.
  - It is aliasing-prone at 1x, so it belongs on our 2x path, as Surge's own 2x voice has it.
- **CPU:** one biquad plus one rational tanh (one division) per stage, two for Resonance Warp. At 2x, about 15 ns for 1 stage and 40-60 ns for 4.
- **Size:** about 150 lines of DSP plus taper and level fitting, about 300 in all.
- **Risk:** medium. It's character that's hard to fit against a reference, as nothing in The Drop matches it.
- **License:** GPL-3.0-or-later (headers). The idea is from Chowdhury's CCRMA articles, linked in the headers.

### 4. Tri-pole: Ian Fritz's "Threeler" OTA filter (optional, costly)
- **Source:** `TriPoleFilter.h`, 512 lines.
- **What it is:**
  - Three OTA one-poles (18 dB/oct) with nonlinear global feedback.
  - Four LP/HP orderings by three output taps, for 12 subtypes.
  - The global loop is solved by 3 Newton iterations (`:44`, `:415`), with one per stage.
  - The sigmoid is `x/sqrt(x²+β)` (`:78-84`).
  - It deliberately mismatches its parts: each stage's coefficient is perturbed by 0.998 and 1.0012 (`:343-350`), and each OTA has its own bias constants (`:47-53`).
- **Quality:**
  - Pole tuning comes from an exponential map `g ∝ e^{wc} − 1` (`:340`), which is fine at 2x.
  - Resonance spans `k = −(10^{3.25·res} + 1)`, from −2 to about −1760 (`:357`), held in by a soft limiter (`:276-300`).
  - The Newton derivative uses `1 − y²`, which is tanh's slope, for a sigmoid that isn't tanh (`:86-90`), so convergence is approximate.
- **CPU:** per step, 3 iterations of about 4 divisions, 3 square roots and 2 exps. At 2x that's very roughly 100-150 ns per channel, 2-3x our Ladder.
- **Size:** about 350 lines.
- **Risk:** medium-high.
- **License:** GPL-3.0-or-later, sst's own derivation.
- **Verdict:** only if a distinctive 18 dB character is wanted. Otherwise skip.

### Not worth taking, and why

| Surge type | What it is | Against ours | Evidence |
|---|---|---|---|
| **K35 LP/HP** | Odin 2 / Pirkle AN-5 Sallen-Key, ZDF, tanh at the sum | **Already ported** (`korg35.rs`, decision 61) | `K35Filter.h:129-172` |
| **Diode Ladder** | Odin 2 port of Pirkle's diode ladder, linear ZDF solve, 6/12/18/24 dB taps | **Ours beats it.** Theirs has *no nonlinearity at all* (`DiodeLadder.h:126-215`); its only "character" is an input gain `(0.3k+1)` (`:186`). Ours (Vital's) saturates its first stage, clips the last and high-passes its feedback (`diode_filter.rs:123-173`), fitted to Acid V | |
| **Vintage Ladder type 1 (RK)** | Runge-Kutta 4 on the analog ODE with a cubic clip on every state, 4x inside Surge's 2x | **Duplicates ours, at 2-3x the cost.** 32 derivative evaluations per host sample (`VintageLadders.h:115`, `:201-245`); input zero-stuffing and a 4-tap reconstruction that its own comment calls "a bit of a hack" (`:248-268`). Only its cubic-clip flavour differs | Puckette BSD notice at `:79-97` |
| **Vintage Ladder type 2 (Huovilainen)** | Classic Huovilainen, feedback one sample late, half-sample phase fix, 2x (input held, last sample taken) | **Ours beats it.** `thermal = 1/70` (`:361`) leaves it almost linear at normal levels; resonance clamped by an ear-tuned high-cutoff trim (`:334-337`); feedback delayed, not solved. Ours solves the loop in the same sample (ZDF + Mystran, `filter.rs:428-466`) | CSound origin, LGPL per MoogLadders (compatible) |
| **Vintage Ladder type 3 (Huov 2010)** | Välimäki–Huovilainen 2006: forward-Euler one-poles with a zero at −0.3, polynomial g/res compensation, 1x | **Worse**, but about 3-4x cheaper than ours. A CPU fallback at most (see tricks) | `:435-520`; also `sc = 70` (`:474`) makes it nearly linear |
| **OB-Xd 4-pole** | ZDF 4-pole with a linear feedback solve, `atan` damping on stage 1 only | **Duplicates our Ladder** (theirs barely saturates: `rcor24 ≈ 0.022`, so `atan` bites only around ±45; `:130`, `:345-361`). Take its mode table (#2), not the filter | |
| **Legacy LP12/24, HP, BP, Notch** | Chamberlin SVF run twice per sample (F1 clamped at 2·sin(0.11π)), lattice and coupled-form biquads | **Ours beats them.** Our TPT SVF is exact to 0.45·sr (`filter.rs:220`, `:395-402`) | `QuadFilterUnit_Impl.h:34-546`, `FilterCoefficientMaker_Impl.h:488-512` |
| **Legacy "Moog" (LPMOOG)** | Forward-Euler 4 one-poles, feedback averaged over two samples | **Ours beats it** | `QuadFilterUnit_Impl.h:550-570` |
| **Cytomic SVF** | Andy Simper's SVF with bell, shelf and allpass modes | **Same algorithm as our `Svf::tick_tuned`** (`filter.rs:395-402` vs `CytomicSVF.h:383-404`). Its block ramp is worth stealing (trick T1) | Header `CytomicSVF.h:16-24` also permits MIT use |
| **Comb** | Feedback comb with a 12-tap windowed-sinc read and a soft clip | **Ours is better tuned** (the band's delay is compensated, `comb.rs:15-20`). A historic tuning-offset bug is still behind a flag (`FilterCoefficientMaker_Impl.h:774-781`). Its soft clip is trick T4 | `QuadFilterUnit_Impl.h:591-660` |
| **S&H** | Sample-rate reducer with soft-clipped feedback | **Mostly duplicates Distort's `Down`** (`effects/distort.rs:30`) | `QuadFilterUnit_Impl.h:572-589` |
| **Formant / LPG** | Surge has **no** formant filter type and no LPG (`FilterConfigurationLabels.h`) | Our `vowel.rs` and `lpg.rs` have no Surge counterpart | |
| **HalfRateFilter** (the oversampler) | Polyphase allpass halfband, orders 2-12 | **About the same as ours:** our VCV order-6 set (`blocks/oversampler.rs:24-28`) is within a few percent of sst's soft order-6 set (rejection about 80 dB, `HalfRateFilter.h:748-755`). The order-8 or order-10 sets (106 or 133 dB) are drop-ins if aliasing tests ever ask, at 1-2 more allpasses per branch | |

---

## Tricks worth stealing (ranked by value per effort)

| # | Trick | Theirs | Ours, where it goes | Gain | Size |
|---|---|---|---|---|---|
| **T1** | **Ramp the filter coefficients per sample across each update block.** Surge one-pole-smooths each target by 0.2, then sets `dC = (target − C)/block`, and every filter does `C += dC` each sample. | `FilterCoefficientMaker_Impl.h:28`, `:839-857`; per sample e.g. `K35Filter.h:123-127`; the SVF version is `CytomicSVF.h:411-460` | `track_state.rs:69` (`FILTER_UPDATE_FRAMES = 32`) and `:1914-1918` recompute `Coefficients` and **hold them**, so cutoff moves in a 32-frame staircase (a 1.5 kHz step rate at 48k) under fast envelopes and LFOs. Keep the previous and next `Coefficients` (a `Copy` struct, about 25 f32) and lerp. | Sound: no stepping sidebands on fast high-Res sweeps; faster filter modulation becomes possible. Stability: interpolating between two stable TPT sets is fine in practice (Surge does it on every type). CPU: about 25 FMAs per track frame. Check first with a spectrum test (a fast env sweep at high Res, sidebands at rate/32); the existing step test (`filter.rs:1347-1396`) checks only the biggest jump, on 3 types. | about 40 lines |
| **T2** | **A more accurate rational tanh.** The (7,6) Padé clamped at ±5, against our (5,4) clamped at ±4. Theirs costs two more multiply-adds and still one division. | `sst-basic-blocks/.../dsp/FastMath.h:208-214`, clamp `:235-239` | `filter.rs:158-168` (`tanh_over_x`, `soft_clip`), used by the ladder's slopes (`:435`), its output (`:465`), the Korg sum (`korg35.rs:62-64`) and the driven SVF (`:411`) | My check: worst error 2.3e-3 → 1e-4. Ours **overshoots to 1.0017 at the clamp** and then goes flat (a kink, so a little extra aliasing on heavy drive); theirs reaches 0.99934 at 4 and 1.00001 at 5. It shifts saturated levels slightly, so rerun the `tools/compare` taper and level checks. | 5 lines |
| **T3** | **Resonance compensation by a resonance-dependent input gain.** Surge's "Compensated" ladders feed back `y4 − c·x` (c = 0.666, 0.5 or −0.5 per type); Odin's diode multiplies the input by `1 + 0.3k`. Both are **linearly identical to an input gain of `1 + c·k`**. In a saturating ladder the bass stays up while the louder peak runs into the stages' saturation. | `VintageLadders.h:120`, `:173`; `:321`, `:349-350`; `:463`, `:497-498`; `DiodeLadder.h:186` | Ladder input at `filter.rs:538-541`, or in the solve at `:450`: `u = (input·(1 + k·c) − k·out_offset)/(1 + k·out_gain)`. Multiply `response_db`'s ladder power (`:609-619`) by `(1+kc)²`. LP modes only, since it lifts the HP top too. | Sound: a "fat" ladder. At full Res (k = 4.3) and c = 0.5, DC goes from −14.5 dB to −4.5 dB (+10 dB of bass). Keep today's behaviour as the default, since `filter.rs:1006` asserts the Moog-like bass loss, and offer this as a mode. | 5-10 lines plus a mode |
| **T4** | **A cubic soft clip in comb feedback in place of a hard clip.** `x − (4/27)x³` on ±1.5, no division, smooth in slope. Surge's comb soft-clips what it feeds back. | `Clippers.h:38-55`; comb use `QuadFilterUnit_Impl.h:640-641` | `comb.rs:466-467` (`(band * feedback).clamp(-1, 1)`, Vital's hard clip) | Sound: full-feedback runaway rounds off instead of squaring. CPU: the same. It departs from Vital's behaviour, so A/B it first. | 2 lines |
| **T5** | **Component mismatch.** Perturb each stage's coefficient slightly (×0.998, ×1.0012) and give each saturator a slightly different positive and negative scale (asymmetry). | `TriPoleFilter.h:343-350` (stage g), `:47-53` (per-OTA bias); OB-Xd's asymmetric diode poly `OBXDFilter.h:174-191` | Our Ladder uses one `g` for all four stages (`filter.rs:434`); the per-stage solve (`:438-446`) already loops per stage, so a per-stage `g` is trivial. Asymmetry: scale `y` differently by sign in `slope` (`:435`). | Sound: a little even-harmonic "analog" grain and slightly smeared poles. CPU: about 0. Risk: the self-oscillation tuning test (`filter.rs:1039`) needs a check. | 10 lines |
| **T6** | **A near-exact `tan` at one division.** The continued fraction for `tan`, exact to < 1e-6 cents up to 0.45·fs in my check. | `FastMath.h:177-183` | Matters only if T1 or a port computes `g` per sample. `filter.rs:222`, `:227` use `f32::tan` at the 32-frame rate, which is fine. Our `tan_accurate` (`blocks/plaits_parts.rs:55-63`) is a polynomial, which can't follow tan's pole near 0.45. | CPU, only if per-sample. | 5 lines |
| **T7** | **A cheap 1x ladder** (Välimäki–Huovilainen): forward-Euler one-poles with a zero at −0.3, `(x + 0.3x₋₁)/1.3`, plus cubic fits for `g(ω)` and `gres(ω)` that undo the delayed feedback's detuning. One tanh per sample. | `VintageLadders.h:461-462`, `:479-492`, `:495-511` | A CPU fallback for `Ladder` (49 ns at 2x) if many ladder tracks ever strain the meter. Not otherwise. | CPU about 3-4x lower (est.); sound worse at high cutoffs | about 60 lines |
| **T8** | **Smooth what goes into a waveshaper with a one-pole at 2x.** `ws = 0.5·(ws + x)` before the shaper. | `src/common/dsp/QuadFilterChain.cpp:63`, `:80` | Not a filter: `effects/distort.rs` when oversampled | A little less aliasing, nearly free | 2 lines |
| **T9** | **Trim resonance with cutoff above about 12.4 kHz.** `reso *= 1 − (note − 58)·0.05`, where note 58 above A440 is about 12.4 kHz. Plus output makeup `1 − 0.25·res²`. | `FilterCoefficientMaker_Impl.h:415-455`, `:457-485`; OB-Xd 4-pole makeup `OBXDFilter.h:483-485` | Not needed: our per-family tapers and levels are fitted to The Drop and Arturia (decisions 120, 137), which is better than these ear-tuned curves. Listed so it isn't re-researched. | none | — |
| **T10** | **Flush denormals.** Surge leans on the host or JUCE's `ScopedNoDenormals` (`surge-fx/SurgeFXProcessor.cpp:316`); its filters have no denormal guards of their own. | — | We set nothing globally (only `effects/pedal.rs:93`, `:369` guard theirs). As far as I know, Apple Silicon handles subnormals in hardware with little penalty, so measure before adding FPCR.FZ through inline asm. | Probably none on the M5 | — |

Considered and rejected: a half-sample-averaged feedback, as in Huovilainen (`VintageLadders.h:419`), for our Vital diode, whose feedback is one sample late (`diode_filter.rs:154`). The average also low-passes the loop and lowers resonance at high cutoffs. Our fitted correction (`filter.rs:37-52`) already keeps the diode in tune, so there's no clear gain.

---

## Details

### Our filters (baseline)
- **Clean LP/HP/BP/Notch.**
  - A TPT SVF (Zavalishin/Simper) at 1x with `g = tan(πf/fs)` clamped at 0.45·fs (`filter.rs:220-222`, tick `:395-402`), so tuning is exact to the clamp.
  - 24 dB is a second SVF with Q 0.707 in series (`:57`, `:497-505`).
  - Q runs from 0.5 (or 0.707 for BP) to 10, on tapers fitted to The Drop (`:61-67`, `:97-107`). They never self-oscillate.
  - Drive moves to 2x with a soft clip on the band state (`:407-414`, `:553-563`).
- **Ladder.**
  - Huovilainen's model with ZDF integrators and Mystran's linearisation, after VCV, at 2x (`:417-466`).
  - The loop is solved in the same sample (`:450`), so pitch stays true near the top.
  - k reaches 4.3 (`:29`). Mixes use Vital's weights (`:72-78`).
- **Diode.** Vital's, with a saturating first stage, a clipped last stage and a feedback high-pass (`diode_filter.rs:123-173`). Its stages hear each other a sample late, and an empirical tuning fit corrects that (`filter.rs:37-52`, `:238-243`). At 2x.
- **MS20 LP/HP.** sst's K35 (`korg35.rs`), saturating at the sum, within 5 cents up to 16 kHz (decision 61). At 2x.
- **Dirty.** Vital's Sallen-Key with saturating integrators, retuned so it rings within 3 cents (`dirty_filter.rs:1-23`). At 2x.
- **Others:**
  - The VA VCF voice uses Plaits' SVF (`voices/virtual_analog_vcf.rs`).
  - Comb (Vital, tuned like Rings), vowel (Plaits' formants), LPG (Streams).
  - The oversampler is a 3+3 allpass polyphase halfband (`blocks/oversampler.rs:22-50`).
  - Coefficients are recomputed every 32 frames (`track_state.rs:69`, `:1914-1918`). Parameters are smoothed per sample before that.

### Porting Surge's SIMD to our per-channel f32: easy
- Each `QuadFilterUnitState` lane is an independent voice (`QuadFilterUnit.h`). No filter here mixes lanes, except the HalfRateFilter's stereo shuffles, which we don't need.
- **Coefficients:** `f->C[]` becomes a field and `f->dC[]` becomes its ramp (or drops, if T1 is done once for the track).
- **Arithmetic:** `f->R[]` becomes state, the `M/A/S/D` macros become `* + - /`, and compare/and/andnot masks become `if` or a select.
- **Per-lane loops:** they are already scalar (OB-Xd's `atan` at `OBXDFilter.h:352-361`, comb reads).
- **The real work is elsewhere:**
  - Surge's cutoff is semitones from A440 (`provider_note_to_pitch(freq + 69)`), which has to map to our `20·1000^c`.
  - Its 0.3·sr clamps assume its 2x rate.
  - Each new type needs a `Res` taper and level fitted with `tools/compare`, plus a `response_db`, as decisions 120 and 137 require. Budget most of each port's time there, not on the DSP.

### Licenses
| Code | License | Evidence | Notes |
|---|---|---|---|
| sst-filters (every file cited) | GPL-3.0-or-later | `LICENSE.md`; every header ("version 3 or later") | Same as vimtracker (`Cargo.toml:8`, GPL-3.0-or-later). Adapt with attribution, as `korg35.rs:14-19` does. |
| `CytomicSVF.h` | GPL-3+, **and** offered for MIT use | `CytomicSVF.h:16-24` | |
| sst-basic-blocks (`FastMath.h`, `Clippers.h`) | GPL-3.0-or-later | `LICENSE.md`; `FastMath.h:12-21` | Only headers explicitly marked (e.g. `LanczosResampler.h`) are also MIT. FastMath and Clippers are not. The Padé formulas themselves are textbook. |
| sst-waveshapers | GPL-3 | `LICENSE.md`, README | Not recommended here |
| Surge `src/` (`QuadFilterChain.cpp`) | GPL-3.0-or-later | file header `:1-20` | |
| OB-Xd origin | GPL-3; OB-Xf distributes it as GPL-3.0-or-later | OB-Xf `Filter.h:1-17`, `LICENSE` | The original `reales/OB-Xd` repo now has only a README, so I couldn't read its headers myself. Either way it is compatible with vimtracker, which is distributed under GPL-3. |
| Odin 2 origin (K35, Diode) | GPL-3.0-or-later | Fetched `odin2/Source/audio/Filters/DiodeFilter.cpp` header | Based on Pirkle's book diagrams |
| Vintage RK origin | BSD-2 (Miller Puckette) | `VintageLadders.h:79-97` | Keep the notice if ported |
| Vintage Huovilainen origin | LGPL (CSound, per the MoogLadders README table) | `VintageLadders.h:297`; MoogLadders README | Compatible with GPL-3. Not recommended anyway. |
| Cutoff/Resonance Warp, Tri-pole | GPL-3.0-or-later (sst's own) | headers | The underlying ideas come from Chowdhury's CCRMA articles and Fritz's circuit notes |

Nothing I recommend is GPL-2-only or unclear.
