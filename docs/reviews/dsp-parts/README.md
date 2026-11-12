# DSP parts: Surge XT, OB-Xf, TetraOP and TAL-NoiseMaker

Four evaluations, written on 2026-11-11, of the open source synths in `../dsp` against vimtracker's
DSP in `crates/vt-engine/src/dsp`: what's worth porting, the small tricks worth grafting onto our
own code, and each part's licence. The issues they led to are in the M31 milestone.

| Report | Covers |
|---|---|
| `surge-filters.md` | Surge XT's filters (sst-filters, OB-Xd, Odin), against ours |
| `surge-effects.md` | Surge XT's effects, sst-effects and the bundled Airwindows, against our effects and dynamics |
| `obxf-tal.md` | OB-Xf and TAL-NoiseMaker: oscillators, filters, chorus, envelopes, analog drift |
| `tetraop-oscillators.md` | TetraOP's FM, and Surge XT's oscillators and modulators, against our voices and mod slots |

Notes for reading them:

- Line numbers refer to the code as it was that day, in both repos.
- Surge's `libs/sst` submodules were empty in the local checkout, so the sst code was read from
  clones at the commits each report names, which weren't kept.
- `scratchpad/` paths name throwaway measurement programs from that session (an aliasing
  benchmark, an SVF benchmark, a Python simulation of the OB-Xd filter), which weren't kept. The
  aliasing benchmark held a transcription of TAL-NoiseMaker's oscillator, which is GPL-2-only and
  can't be added to this GPL-3 repository.
- **TAL-NoiseMaker's DSP is GPL-2-only.** Anything taken from it must be written afresh from the
  idea, never translated from its code.
- CPU figures are estimates from operation counts unless a report says it measured them.
