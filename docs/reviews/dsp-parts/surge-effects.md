# Surge XT, sst and Airwindows effects compared with vimtracker's

Research only. Nothing was changed in either repo.

**Sources read**
- vimtracker `crates/vt-engine/src/dsp/{effects,dynamics,filters,blocks}` at `37b9153`, plus
  `docs/designs/decisions.md` decisions 34 to 46 (where each effect came from).
- Surge `/Users/gabrielgellner/dev/dsp/surge` at `e2a299e`: `src/common/dsp/effects/**`,
  `libs/airwindows/**`.
- **The sst submodules are empty in the Surge checkout** (`libs/sst/*` are bare directories,
  and `git submodule status` shows them uninitialised). I read clones that were already in the
  scratchpad at `scratchpad/sst/`: `sst-basic-blocks` 7209f14, `sst-filters` 6bcf08b and
  `sst-waveshapers` dd12f31, which are Surge's pinned commits, and `sst-effects` 987b39d, which
  is **newer** than Surge's pin (5f2bd1a). Surge's pinned commit isn't in that clone, so I
  couldn't check it out. The sst-effects line numbers below are from 987b39d. The Dual Delay's
  `Clean` line mode (sst-effects #258) came in after Surge's pin, so it's in the clone but not in
  Surge's own build.
- I estimated CPU costs from operation counts. I didn't benchmark anything.

---

## 1. Ranked recommendations: what to port

| # | What | From | Licence | Port size | Risk | Replaces or adds |
|---|------|------|---------|-----------|------|------------------|
| 1 | **Frequency shifter** (Hilbert single-sideband shift, with feedback, key tracking, and an optional delay in the loop for barber-pole effects) | `sst-basic-blocks/dsp/HilbertTransform.h:66-170`, `sst-effects/voice-effects/modulation/FreqShiftMod.h:119-151`; for the delay-loop design, Surge `FrequencyShifterEffect.cpp:96-172` | HilbertTransform.h is **MIT or GPL-3** (dual, stated in the file at lines 30-32 and in the README). FreqShiftMod.h and Surge's file are GPL-3+ | S: about 150 lines of Rust | Low | **Adds.** Nothing we have does this. It sits next to Warps' ring mod, and it can follow the track's note the way Warps' oscillator does |
| 2 | **Spring reverb** (ChowDSP's: 16 nested second-order Schroeder all-passes with Thiran reads, a tanh feedback loop, a 4-line Householder reflection network, a DC blocker and a low-pass, plus the "shake" drip) | Surge `chowdsp/spring_reverb/SpringReverbProc.cpp:91-175`, `SchroederAllpass.h`, `ReflectionNetwork.h:55-100`, `shared/chowdsp_DelayInterpolation.h:192-222` (Thiran) | GPL-3+ (Surge header on every file) | M: about 350 to 450 lines | Medium. The four-lane SIMD state packing has to be unrolled, `std::random_device` replaced with `rng.rs`, and the `std::vector` shake buffer sized at construction | **Adds.** The Octatrack has a spring reverb, and dub and techno lean on one. Our Clouds reverb and Dattorro plate don't make that sound |
| 3 | **Galactic** as a third reverb, or a `Type` on Reverb ("Space") | Airwindows `libs/airwindows/src/GalacticProc.cpp:11-235` | MIT (`libs/airwindows/LICENSE`, © 2018 Chris Johnson). Note below | S-M: about 200 lines | Low | **Adds** the huge, nearly endless pad wash. Its tank is three 4x4 "one minus the others" mixes per side with cross-fed sides and no interpolation, so it's cheap |
| 4 | **BBD ensemble** (Holters and Parker's bucket-brigade model, Juno-style) as a `Type` on Chorus | Surge `BBDEnsembleEffect.cpp:221-412`, `chowdsp/bbd_utils/BBDDelayLine.h:30-90`, `BBDFilterBank.h`, `BBDNonlin.h` | GPL-3+ | M-L: about 450 lines (4-wide complex arithmetic becomes `[f32; 4]` real and imaginary pairs) | Medium. Complex pole maths, and the cost depends on the clock | **Adds character** next to Rings' chorus and ensemble. Ours is clean and very cheap; this is the analog one, at roughly 5 to 10 times the CPU |
| 5 | **Stutter or repeat** (gated, zero-crossing-spliced repeats) | `sst-effects/voice-effects/delay/Microgate.h:200-300` | GPL-3+ | S-M: about 150 lines | Low-Medium. It should be redesigned around the grid, with a step lock as the trigger rather than a threshold | **Adds** a live-performance effect a groovebox user reaches for |
| 6 | **Exciter** (high-pass, drive, tanh scaled by a diode-rectifier envelope, at 2x) | Surge `chowdsp/ExciterEffect.cpp:55-90`, `ExciterEffect.h:70-80`, `exciter/LevelDetector.h` | GPL-3+ | S: about 80 lines on our `Oversampler` | Low | **Adds** air for hats and vocals. Surge's version has a bug at `ExciterEffect.h:75`: the right side's level reads channel 0's state, so one detector is stepped twice. Use one per side |
| 7 | **Rotary speaker** | `sst-effects/effects/RotarySpeaker.h:195-433` | GPL-3+ | S-M: about 180 lines | Low | **Adds** an organ effect. Surge's has no rotor inertia (speed jumps), so add a slew on the rate ourselves |
| 8 | Airwindows dynamics flavours as `Comp` types: ButterComp2, Pressure4, DrumSlam | `ButterComp2Proc.cpp`, `Pressure4Proc.cpp`, `DrumSlamProc.cpp` | MIT | S each, about 100 lines | Low | **Adds** one-knob, colourful types. They don't replace anything: our Clean, Console and Glue are measured models and better than anything Surge has |

**Not worth porting**, with reasons in §3: Surge Chorus, Flanger, Phaser, Ring Modulator,
Vocoder, Resonator, Treemonster, Nimbus, Reverb 1, Bonsai, Neuron, Conditioner, Chow Tape's
hysteresis, Combulator (take its idea instead), and Reverb 2 (take its tricks instead).

**Airwindows consoles:** Surge doesn't bundle any Console (no Console7, 8 or 9 under
`libs/airwindows/src`), and there's no Airwindows checkout in `../dsp`. Our `Console` is an SSL
bus compressor, not an Airwindows console. If console summing is wanted, it would come from
upstream airwindows (MIT); I didn't evaluate it.

---

## 2. Tricks worth stealing (ranked by value per effort)

| # | Trick | Theirs | Ours, to improve | Gain | Size |
|---|-------|--------|------------------|------|------|
| 1 | **NaN-safe clamp in feedback paths.** sst clips with `min_ps`/`max_ps` "so a non-finite input lands on the limit rather than passing through". In Rust, `f32::clamp(NaN)` returns NaN, but `x.max(lo).min(hi)` returns the limit, because `max` and `min` return the non-NaN operand | `sst-basic-blocks/dsp/Clippers.h:115-129` | `filters/filter.rs:166` (`soft_clip` uses `clamp`; the flanger and phaser feed back through it), `voices/drums.rs:32-40` (the delay's feedback `soft_clip`: NaN falls through to the rational), `effects/distort.rs:186` (`Hard`) | **Stability.** At the moment one NaN in a feedback line stays there forever, silencing the slot's tail. With this it decays like a click | 1-3 lines |
| 2 | **Flush denormals on the audio thread.** Surge FX wraps processing in JUCE's `ScopedNoDenormals`, which sets FTZ and DAZ on x86 and FPCR.FZ on ARM. Airwindows guards each IIR state by hand: `if (fabs(iirAL)<1.18e-37) iirAL = 0.0;` | `src/surge-fx/SurgeFXProcessor.cpp:316`; `libs/airwindows/src/GalacticProc.cpp:81-84`, `CabsProc.cpp:53` | No FTZ anywhere in vimtracker (grepped). Set it at the top of the cpal callbacks, `crates/vt-tui/src/audio.rs:525` and `:707`, through `asm!("msr fpcr, …")` on aarch64 and `_mm_setcsr` on x86_64. `pedal.rs:369` resets by hand today | **CPU insurance:** decaying IIR states (EQ, compressor filters, reverb damping, the phaser) on silence. The penalty is large on x86 (CI and Linux), probably small on Apple Silicon, so measure | about 10 lines |
| 3 | **Rotating-phasor (quadrature) LFOs instead of `sin()` per tap.** One oscillator step is 4 multiplies, and phase offsets are fixed rotations. Reverb 2 takes four LFO phases from one oscillator as `r, i, -r, -i` | `sst-basic-blocks/dsp/QuadratureOscillators.h:98-132` (`SurgeQuadrOsc`), `:39-59` (Vicanek's amplitude-stable recurrence); `sst-effects/effects/Reverb2.h:447-450` | `effects/chorus.rs:141-182`: Ensemble calls `sin()` **12 times a frame** (6 wobbles, 2 sines each) and Chorus 4 times. Two phasors and precomputed rotations by 1/6, 1/3, 1/2 and so on do the same work. The plate already does this (`plate.rs:334-336`) | **CPU:** most of the chorus's cost, roughly 3 to 5 times cheaper | about 25 lines |
| 4 | **"Clean" delay retime.** When the time changes, crossfade linearly from the old read tap to the new one over half the new delay (between 10 ms and 0.5 s), and ignore requests while a fade is running. It's linear rather than equal-power because the two taps are correlated, and the comments derive why | `sst-effects/effects/Delay.h:200-271` (rationale and `CleanRetime`), `:279-286` (`cleanFadeSamples`), `:338-378` (`readRun`) | `effects/delay.rs:151`: we always glide (50 ms half-life), so a step-locked time change bends the pitch of the repeats. Add this as a `Glide`/`Jump` switch | **Sound**, for step-locked time: repeats keep their pitch. This matters for a groovebox | about 60 lines |
| 5 | **Decay as RT60 seconds, per line.** `g = 0.001^(loop_seconds / T60)`, so `Size` and the rate don't change the tail length, and lines of different lengths die together, so fewer modes ring on | `Reverb2.h:410-412`; `Reverb1.h:436-437` (per-line gain on a 16-line FDN); `SpringReverbProc.cpp:104`, `ReflectionNetwork.h:65-67` | `effects/reverb.rs:188` and `effects/plate.rs:274` (`decay = value * 0.98`, a raw loop gain). The plate's `Size` up to 2x also doubles its tail today. Our "no backwards compatibility" rule allows re-meaning `Decay` | **Sound and usability:** `Decay` means seconds at every `Size`; it can also show "2.4 s" | about 10 lines |
| 6 | **Low-frequency damping inside the reverb loop** (a one-pole high-pass next to the high-frequency low-pass) | `Reverb2.h:461-464` (`process_highpass`); spring: a 40 Hz DC blocker in the loop, `SpringReverbProc.cpp:52`, `:135-137` | `reverb.rs:266-279` and `plate.rs:366-367` damp only the highs. Our `Low cut` is on the input, so the bass still rings longest | **Sound:** long tails stop going muddy under kicks | about 10 lines plus a setting |
| 7 | **First-order ADAA** (antiderivative anti-aliasing) for piecewise-linear shapers: `y = (F(x) - F(x_prev)) / (x - x_prev)`, falling back to `f(x)` when `abs(dx) < 1e-4`. The folder's antiderivative is built from its breakpoints | `sst-waveshapers/ADAA.h:24-56`, `Wavefolders.h:22-110` | `effects/distort.rs:186-190`: `Hard` and `Fold` driven by up to +30 dB at only 2x oversampling alias heavily. Use ADAA inside the existing 2x | **Aliasing:** in the ADAA literature, ADAA1 plus 2x is often on a par with 4x to 8x for hard clips | about 40 lines |
| 8 | **Keytracked sample-rate reduction, continuous bit depth, and mu-law.** The bit-crusher's hold rate follows the note, so the aliasing is harmonic; bits are `2^bd` with a float `bd`, so they modulate smoothly. DeRez2 smooths the rate, moves the sampling instant within the sample, and companding with `log(1+255x)/log(256)` gives the 8-bit-sampler sound | `sst-effects/voice-effects/distortion/BitCrusher.h:145-194`; Airwindows `DeRez2Proc.cpp:39-56`, `:94-95` | `effects/distort.rs:677-690` (`Crush` takes whole bits, "isn't smoothed"; `Down`'s period isn't tied to the note). `InsertState.note` (`distort.rs:93`) is already there | **Sound**, plus modulation without steps | about 20 lines |
| 9 | **DC blocker in the feedback of a modulated delay** | `BBDEnsembleEffect.cpp:273-275`; `SpringReverbProc.cpp:135-137` | `effects/flanger.rs:155-158`: at 0.95 feedback, DC in the input is amplified 20 times inside the loop and pushes the soft clip off centre. The same applies to Comb's positive feedback, which already high-passes its input | **Stability and sound** | about 5 lines |
| 10 | **Power-of-two buffers and masks instead of `%`** on every read and write. Surge masks with `& (len-1)` everywhere and mirrors the first N samples past the end, so its FIR reads never wrap | `Delay.h:293`; `ChorusEffectImpl.h:180-182` (buffer mirroring) | `effects/delay.rs:131`, `:174` (four integer divisions a frame), `effects/reverb.rs:232-234` (pre-delay) | **CPU**, minor: tens of cycles a frame | about 5 lines (the delay's buffer is sized ahead already) |
| 11 | **Soft-limited, shelved feedback for pitch-moving loops:** `fb = sqrt(2) * fb / (1 + fb^2)` after a shelf, then a DC filter on the output | `FreqShiftMod.h:131-144` | the new frequency shifter, and `effects/pitch_shifter.rs` (its feedback reuses the flanger's soft clip) | **Stability:** shimmer stacks stay bounded and lose their lows | about 10 lines |
| 12 | **Width (M/S) on reverb outputs** | `sst-effects-shared/WidthProvider.h:31-40`, used at the end of every sst reverb (`Reverb2.h:487`) | `reverb.rs:281`, `plate.rs` output. Delay already has width | **Sound** | about 5 lines plus a setting |
| 13 | **Slow modulation of the delay time** (a leaky-integrated triangle, opposite on L and R) for tape-like repeats | `Delay.h:509-529`, `:540-541` | `effects/delay.rs` has no modulation | **Sound** | about 15 lines plus a setting |
| 14 | **More LFO shapes for sweeps:** S&H, smoothed random, a skewed saw | `sst-effects/effects/Flanger.h:331-398` | `flanger.rs:34`, `phaser.rs:51` (triangle only) | **Sound** | about 20 lines plus a setting |
| 15 | **Run the reverb tank at a base rate at high device rates** (one reverb sample every `cycleEnd = floor(rate / 44.1k)` frames, interpolated between) | `GalacticProc.cpp:23-26`, `:82`, `:172-212` | `reverb.rs`, `plate.rs`, which run at the device rate and allocate 4x at 192 kHz | **CPU and memory**, only above 88 kHz. Low value at our usual 48 kHz | about 30 lines |
| 16 | **Thiran all-pass interpolation** for slowly moving reads in loops: flat magnitude, one multiply. It shifts its integer part when `frac < 0.618` to stay stable | `chowdsp/shared/chowdsp_DelayInterpolation.h:192-222` | `filters/comb.rs:434` (Catmull-Rom, whose magnitude dips between samples, so a comb's brightness varies by note). Comb already compensates the delay (`read_for`, `:133`) | **Sound:** more even ringing across notes. Not for fast sweeps, as it's stateful | about 30 lines |

Rejected as tricks: Surge's 12-tap windowed-sinc reads (`ChorusEffectImpl.h:115-131`). They're
better than our linear and Hermite reads in theory, but about 3 times the cost, and a chorus
wants the slight high-frequency loss. Airwindows' `fpd` dither to 32-bit float
(`FireAmpProc.cpp:844-849`) does nothing in an all-f32 engine, and its noise would break our
"same in, same out" rule (`tape.rs:13-15`).

---

## 3. Effect by effect: is theirs clearly better?

### Chorus and ensemble (ours: Rings, `effects/chorus.rs`)
- **Ours:** 2 taps (Chorus) or 3 per side (Ensemble), sine wobbles, **linear** reads
  (`chorus.rs:116-124`), no feedback and no filters. It costs 4 or 12 `sin()` calls a frame plus
  4 or 6 reads.
- **Surge Chorus** (`ChorusEffectImpl.h:94-188`): 4 voices with a triangle LFO per voice, worked
  out **per block** and lagged (`:89-97`). The voices are panned with a sqrt law (`:61`), read
  with 12-tap sinc, with a hard-clipped feedback (`:163`) and wet LP and HP biquads. It's better
  on paper (feedback, filters, more voices) but has no clear edge in sound, and costs about 3
  times ours. **Not better.**
- **Surge BBD Ensemble** (`BBDEnsembleEffect.cpp:297-412`): a real bucket-brigade model (Holters
  and Parker, DAFx 2018) with 128 to 4096 stages, Juno-60 anti-aliasing and reconstruction poles
  (`BBDFilterBank.h:35-48`), and a WDF diode nonlinearity (`BBDNonlin.h:37-90`). It has two LFOs
  in three phases, as Rings' ensemble does, a DC-blocked feedback (`:273-275`), and a clock-rate
  parameter that couples delay and bandwidth.
  - Cost: per line and per sample, about `2·stages / (delay·fs)` clock events, around 3.3 at the
    defaults. Each event is a 4-wide complex multiply-accumulate, times 4 lines, plus the diode.
    Surge's own note says the nonlinearity is "2.7% → 1.8% after SSE" (`BBDNonlin.h:31-32`).
  - **Clearly better in character, clearly worse in CPU.** Add it as a type; don't replace ours.
  - `#if !defined(_M_ARM64EC)` (`:23`) is a Windows-on-ARM build quirk, not an issue for Rust.

### Flanger (ours: Vital, `effects/flanger.rs`)
- **Ours:** Hermite read (`:119-134`), a triangle sweep, a soft-clipped feedback at ±0.95, and a
  third of a turn between the sides.
- **sst Flanger** (`Flanger.h:401-613`): up to 4 combs per side **tuned to pitches**, with
  arpeggio modes, 7 LFO shapes, and a cubic soft clip and high-pass on the feedback. It also
  **clamps the output to ±1 and soft-clips it, always** (`:595-599`), which colours hot signals.
  **Not better.** Its LFO shapes are worth stealing (trick 14).

### Phaser (ours: Vital, `effects/phaser.rs`)
- **Ours:** 12 one-pole all-passes, a band-passed and soft-clipped feedback, and a coefficient
  interpolated every 16 frames, at about 0.5% of a core on an M5 (decision 39).
- **sst Phaser** (`Phaser.h:260-299`): 1 to 16 **biquad** all-pass stages with Q ("Sharpness")
  and spread, feedback only **clamped to ±32** (`:276-277`) rather than soft-clipped, and a tone
  filter on the output. **Not better.** It's an alternative voicing at roughly 2 to 3 times the
  maths per stage.

### Delay (ours: Vital-style, `effects/delay.rs`)
- **Ours:** Hermite read, a glide to a new time (`:151`), LC and HC with soft clip and `Drive` in
  the feedback, ping-pong and width.
- **sst Delay** (`Delay.h:381-694`): sinc reads, feedback-clip choices (soft, tanh7, hard, hard
  at ±8; `:650-671`), negative feedback, separate crossfeed, an LFO on the time, input pan, and
  the **Clean retime** mode. The engine isn't clearly better, but two of its behaviours are:
  Clean retime (trick 4) and time modulation (trick 13). Graft those. Don't port the effect.

### Reverbs (ours: Clouds/Griesinger `reverb.rs`, Dattorro `plate.rs`)
- **sst Reverb 2** (`Reverb2.h:398-489`): 4 input all-passes, then a 4-block ring, each block 2
  all-passes, an HF low-pass, an **LF high-pass**, a modulated delay with 2 output taps, and a
  decay gain. The decay is **RT60-calibrated** (`:412`), and a 0.25 Hz LFO is used in 4
  quadrature phases (`:425-450`). Modulated reads use 8-bit fixed-point linear interpolation
  (`:307-315`).
  - Memory: Surge statically allocates 16 lines of 128K floats plus a 1.5M-float pre-delay (about
    14 MB). A port would size from the rate.
  - **Better than our Clouds reverb for halls; not better than the plate.** Its value is mostly
    tricks 5, 6 and 12, so graft those first. Port it as a type only if a third algorithm is
    wanted.
- **sst Reverb 1** (`Reverb1.h:216-334`): a 16-line FDN with Householder feedback
  (`-2/N · sum`, `:291`) and per-line RT60 gains (`:436`). Its fixed integer delay times aren't
  scaled to the rate (`:345-417`: the same sample counts at any rate) and it has **no
  modulation**, so it's metallic. **Not better.** The Householder and per-line RT60 ideas are
  noted under trick 5.
- **Spring** (ChowDSP): **adds**; see recommendation 2.
- **Airwindows Galactic, Verbity and Chamber** (`GalacticProc.cpp`, `VerbityProc.cpp`,
  `ChamberProc.cpp`): 12 lines per side in three 4x4 `x_i - sum(others)` stages, sides cross-fed,
  a vibrato pre-delay (`GalacticProc.cpp:56-77`), one-pole low-passes outside the tank only, and
  base-rate tank processing.
  - Very cheap, with no fractional reads in the tank.
  - Galactic's regen 0.0625 to 0.125 gives near-infinite pads. Chamber spaces its lines by the
    golden ratio (`ChamberProc.cpp:44-55`).
  - **They add a sound** neither of ours makes (recommendation 3).
  - Upstream has Galactic2 and Galactic3. They're not local, so I didn't check them.
- **Nimbus** = Clouds' `GranularProcessor` run at Clouds' own rate, **16-bit shorts**, with
  Lanczos resampling in and out (`sst-effects/effects/NimbusImpl.h:37`, `:101-213`). **Worse**
  than our float, native-rate ports of Beads and Clouds. If Clouds' WSOLA stretch, looping delay
  or spectral modes are wanted, port them from MIT
  `../dsp/eurorack/clouds/dsp/{wsola_sample_player.h,looping_sample_player.h,pvoc/}` directly.

### Tape (ours: Airwindows ToTape9 with Beads' wow, `effects/tape.rs`)
- **Chow Tape** (`chowdsp/TapeEffect.cpp`, `tape/HysteresisProcessor.cpp:131-263`): a
  Jiles-Atherton hysteresis solved with RK2, RK4 or Newton-Raphson in **double precision** at
  **2x** (`:136-142`), plus a head-loss FIR, degrade and chew. It's more physical and much
  heavier: RK4 is 4 hysteresis derivative evaluations per oversampled sample, each with
  transcendental functions.
  - **Not clearly better** for this tool. ToTape9 is the newer Airwindows design and costs a
    fraction as much. Surge bundles only ToTape6, which is older than ours.
  - Optional lo-fi extras: Chew's random dropouts (`tape/ChewProcessor.cpp:56-115`, a power law
    on `abs(x)` with an LPF) and Degrade. Each is S, low value.
- **Bonsai** (`sst-effects/effects/Bonsai.h:1542-1590`): a Reaktor-derived chain of bass boost,
  tape saturation, noise and "dull". It overlaps with Tape and Distort. **Not better.**

### Distort and pedals (ours: Vital and Plaits shapers at 2x, `distort.rs`; circuit pedals, `pedal.rs`)
- **sst-waveshapers:** a broad set of shapes, with ADAA on the folders and rectifiers
  (`Wavefolders.h`, `Rectifiers.h:39-104`). The shapes aren't needed, but **ADAA is** (trick 7).
- **Airwindows saturators** (Density, Spiral2, Drive, Mojo, Focus and others): cheap, unoversampled
  sine and `sin(x·abs(x_prev))` shapes with alternating ("flip") interleaved high-pass states
  (`DensityProc.cpp:45-60`, `Spiral2Proc.cpp:44-60`). Each has its own flavour, but **none is
  better** than our oversampled shapers and circuit pedals, and all alias more.
- **Neuron** (Chow, GRU-style): skip.
- `Crush` and `Down` take improvements from BitCrusher.h and DeRez2 (trick 8).

### Pitch and Grains (Clouds, Beads)
- Nothing in Surge is better. Airwindows' GlitchShifter (`GlitchShifterProc.cpp`) is a different,
  glitchy shifter and low priority.

### Resonator, Comb, Warps (ring mod and vocoder), LPG, Vowel
- **Surge Resonator** is 3 band-pass filters from sst-filters. Ours is Plaits' modal bank.
  **Not better.**
- **Combulator** (`CombulatorEffect.cpp:178-330`): 3 sst comb filters at 2x, with noise injection
  gated by an envelope follower. It isn't better per comb (ours is tuned for the interpolation
  delay). **Stealable idea:** let our Comb run 3 voices at intervals, for chords, and add
  "excite with followed noise".
- **Ring Modulator** (`RingModulatorEffect.cpp:63-205`): Parker's diode model, as our Warps uses
  (`warps.rs:98-141`), with unison carriers and a cubic clip at 2x. **Not better.**
- **Vocoder:** ours is Warps'. Not compared further.
- **Treemonster** (`sst-effects-shared/TreemonsterCore.h:176-318`): a zero-crossing pitch tracker
  driving a sine for ring mod. Warps' oscillator already follows the track's *sequenced* note
  exactly, which beats tracking it from the audio. Skip.

### Dynamics (ours: Clean after Giannoulis, Massberg and Reiss; Console and Glue fitted to Bus FORCE and Live; EQ3 on Simper's SVF; look-ahead limiter)
- Surge has no compressor effect apart from the Airwindows ones and Conditioner's simple
  limiter. **Nothing is better.** Ours are measured models.
- The Airwindows comps are flavours:
  - **ButterComp2** (`ButterComp2Proc.cpp:44-98`): one knob; positive and negative half-waves
    compressed separately with interleaved "flip" states, and recovery slowed while the output
    is high.
  - **Pressure4** (`Pressure4Proc.cpp:23-110`): program-dependent release, where the speed
    variable grows with the input, and "mewiness".
  - **VariMu**, **DrumSlam**.
  - They're cheap (one or two `sqrt` or `pow` calls a sample) and worth having only as extra
    one-knob types (recommendation 8).

---

## 4. Effect types a groovebox user would miss, ranked

The user targets the Octatrack, whose FX list includes a **Spring reverb**, a **Comb**,
**Lo-fi** (bit-rate and sample-rate reduction plus AM), **Dark** and **Gatebox** reverbs, and a
**DJ EQ**.

1. **Frequency shifter.** Missing; recommendation 1; cheap; a classic for drums, dub and
   barber-pole effects.
2. **Spring reverb.** Missing; recommendation 2; on the Octatrack itself.
3. **Stutter or beat-repeat.** Missing; recommendation 5. It's the most "performance" of these.
4. **Huge ambient reverb** (Galactic). Missing as a sound; recommendation 3.
5. **BBD or analog ensemble.** We have the Rings ensemble; recommendation 4 adds the analog one.
6. **Gated reverb** (Octatrack Gatebox). Missing. No direct Surge source: our reverb plus a
   key-able gate would do it, and `Source` routing already exists on Comp and LPG.
7. **Exciter.** Recommendation 6.
8. **Rotary.** Recommendation 7.
9. **DJ kill EQ or tilt EQ.** Missing as a type. sst has `voice-effects/eq/TiltEQ.h` (GPL), but
   our SVF EQ could take a kill mode in a few lines.

**Already covered:** ring mod and vocoder (Warps), bit-crusher and decimator (`Crush`, `Down`),
comb, vowel, envelope filter and auto-wah (LPG, Streams), pitch shift, granular, tape, plate.

---

## 5. Licences

| Source | Licence | Notes |
|--------|---------|-------|
| Surge `src/common/dsp/effects/**`, including `chowdsp/**` | GPL-3.0-or-later | Surge's header is on every file I opened, including the ChowDSP-derived ones (BBD, spring, tape, exciter). Upstream ChowDSP AnalogTapeModel (`../dsp/AnalogTapeModel/LICENSE`) is GPL-3 too. Compatible; keep "Surge Synth Team / Jatin Chowdhury" attribution |
| sst-effects | GPL-3.0-or-later | `LICENSE` and file headers (for example `Reverb2.h:1-19`) |
| sst-basic-blocks | GPL-3.0-or-later, **except** `dsp/LanczosResampler.h`, `dsp/HilbertTransform.h` and `dsp/EllipticBlepOscillators.h`, which are MIT or GPL-3 | README.md:6-18; `HilbertTransform.h:30-32` says so in the file |
| sst-filters | GPL-3.0 (`LICENSE`); header says "Gnu General Public Licens[e]" | fine |
| sst-waveshapers | GPL-3 (`LICENSE.md`) | **Flag:** `ADAA.h:8` says "sst-jucegui is released under the GNU General Public License 3 or later", a copy-paste of another repo's name. The licence is unambiguous (GPL-3 either way); only the name is wrong |
| Airwindows in Surge (`libs/airwindows`) | MIT, © 2018 Chris Johnson (`libs/airwindows/LICENSE`) | **Flag:** each file still carries Airwindows' template line "Copyright (c) 2016 airwindows, All rights reserved" (for example `GalacticProc.cpp:1-4`). The MIT LICENSE governs, as it did for our ToTape9 port (decision 40). Surge's copies are lightly modified for its wrapper (README steps 5-7). For a clean MIT chain, port from upstream `airwindows/airwindows` LinuxVST sources, as tape.rs did. GPL-3 would be fine for us either way |
| Holters and Parker BBD poles; Dattorro; Hutchins and Costello Hilbert coefficients | published algorithms | fine |

vimtracker is GPL-3, so every source above can be adapted with notices kept.

---

## 6. Porting notes: block-based SIMD to per-sample Rust f32

- **Block mechanics fall away.** Surge's `lipol_ps` and `lipol_sse` ramp a parameter across a
  32-sample block; our `Smoothed` one-pole per sample replaces them. `fade_2_blocks_inplace`
  becomes our `wet * mix + dry * (1 - mix)`. `setvars` running once a block maps to our
  `follow()`, which runs from `modulate` every 32 frames (`engine.rs:45`).
- **SSE lanes become arrays.** The spring's `simdState[4]` is L, R, and each side's feedback
  copy; the BBD's `SSEComplex` is 4 filter poles; Reverb 1 runs 16 lines 4 at a time. All of them
  become `[f32; 4]` loops, which LLVM usually auto-vectorises on aarch64 NEON. No `std::simd` is
  needed.
- **Allocation.** Surge allocates fixed `1 << 18` lines per object: every Chow `DelayLine`, the
  sst Delay, Reverb 2's 16 by 128K floats. Ours must size them from the rate, with spring and
  Galactic in the effect pool (`memory.rs`) like the plate. The spring's `shakeBuffer`
  (`SpringReverbProc.cpp:65`) is a `std::vector` resized in `prepare` and must be sized once.
  `std::random_device` and `std::bind` (`:39-41`) become `rng.rs`.
- **Determinism.** Chow's spring and chew use random generators seeded from the system. Seed
  them from the song or slot, so renders stay bit-identical (our tape left out Airwindows' noise
  for this reason, `tape.rs:13-15`).
- **Precision.** Airwindows works in `long double` and `double`. Galactic and the comps are fine
  in f32. Chow hysteresis needs f64, which is one more reason not to port it.
- **Per-sample cost** (estimates from operation counts, at 48 kHz):
  - Frequency shifter: 12 biquads plus a phasor, about 0.1% of a core.
  - Spring: about 64 all-passes plus about 6 reads, under 1%.
  - Galactic: 24 integer reads and writes plus mixing, about 0.2%.
  - BBD: about 4 lines × 3.3 events × a complex 4-MAC, plus the WDF diode, about 1 to 2%.
  - Exciter: 2x oversampling, a biquad, `tanh` and `exp`, about 0.2%.
