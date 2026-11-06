# Stage 8: Intermediate zoom-out review

Target: `docs/designs/vimtracker.md`, focused on "Direction: a track is one sound" (lines 518-619) and "Milestones after M3" (lines 1033-1044).

**Verdict: do not fix them all.** The direction is at the right altitude. Six small edits and one 10-minute measurement clear the way for code. Everything else can wait for its milestone, or be dropped.

Finding IDs: Z = zoom-out report, C = comprehension report, D/T/S = critic report (design, structural, style).

## What fixing all 103 findings would add

It would raise a 100-line direction to the M1-M3 spec standard, for features that have no code yet. That means a parameter metadata schema, a mark state table, a routing contract, an effect-state architecture, stems defined per track kind, new Goals, at least four new CUJs, more Risks rows and Alternatives entries, and a reconciliation pass over the whole document.

Three things argue against this:

*   **The wrong standard was applied.** C tests against Re-Implementation, but line 743 limits the readiness bar to M1-M3. C says so itself.
*   **Plans here get overtaken in play.** LFOs arrived "by a change of plan" (338). The shape panel displaced live sweeps (458). Issue 18 is on hold (607).
*   **Marks are explicitly an experiment** ("meant to be tried in play", 587). A full behaviour matrix would pin down rules that playing is supposed to decide.

## Settle before any code

**1. Blocker: state one order and one gate.** Lines 73-84, 607-608, 730, 743, 1033-1044. Rules on Z2, C4, D7, T1, T4, T8.
*   Line 743 literally forbids starting M4, so this is the only true blocker.
*   Replace it with one short passage, not a new Goal plus CUJs.
*   Keep G1 and its Stop test, and run it after M6 on songs. Line 607 already leans this way.
*   Change only the "2 loops" criterion (79-80), and make G1 the gate before M7. M4-M6 run ungated, which is an acceptable risk given line 521.
*   Reject D7's option of running G1 now.
*   Add one sentence closing M3 without Issue 18, parking G4 and CUJ 4.

**2. Major: add one sentence saying M4-M10 aren't ready to implement, and each gets a short design when it starts.** Lines 730, 743. Extending line 730's rule this way answers most of C6-C10, C15, C17, C19-C21, T7, T9, T11 and S7. This is the most valuable sentence available.

**3. Major: decide what happens to existing jams.** Lines 112-114, 521, M4. Rules on Z11, D13.
*   Removing `INST` breaks positional `Step::from_str` (`steptext.rs:130`). Multi-instrument tracks have no clean mapping.
*   Default: accept the loss, and keep a built v1 binary to open and render old projects.
*   Reject the converter. Also reject Z11's bundling of format breaks: line 112 already allows several breaks, and a mark can be an optional suffix that the parser adds.

**4. Major: run G7 now.** Lines 100-102, 132, 527. Rules on Z3, D8.
*   It is one 10-minute run, against a refactor of about 490 matching lines across 40 `.rs` files.
*   Reject the scheduled reruns and the effect budget. CPU load is always on screen (884), so a problem will show.

**5. Major: fix the order, with no added scope.** Rules on Z4, Z5, C5, D3, D9, T8, Z13.
*   **Swap M7 and M8.** This is simpler than the reports' proposal to split M8. Slots on sound tracks need no routing and can be heard straight away. Send tracks then reuse them. Sidechain comes last, as it's the only part of M8 that touches routing.
*   **Move M9's cross-track selections and operators to directly after M4.** Folding stays in M9. Line 538 already names this cost.
*   **Keep M4 before M6, and write down why:** songs shouldn't be made in a format about to break.

**6. Nit: small text edits.**
*   Rename the effect position "insert", because this word becomes a type name. Rules on Z12, C1, D12, S1.
*   Use "signal path" at lines 527 and 615 (S2).
*   Soften line 520's "built today" (T2, C13, S13).
*   Delete the Filters and Issue 50 clause at 1035 (Z18, C12).
*   Delete row 582 (Z15). This removes the apparent 3-slot contradiction and T9's folding dependency.
*   Delete line 539's "should allow more" (Z16).
*   Remove line 538's mixer promise, and let a jam request it. Rules on Z4, D9.

## Wait for the milestone

| When | Findings | Simplest answer to take in |
|---|---|---|
| Marks | Z9, Z10, C3, C17, D4, D14, D15 | Rules below. Everything else goes to the fallback rule (449) |
| M4 | C9, Z14, D9 | `Sound` is `Track` minus mute, solo, swing and name. Repoint the browser (`picking.rs:333`) and extend `KitSound`. Accept duplicated sounds across a chord. If choke is missed, line 535's `SMP` already answers it |
| Stems | Z17, C8, D6 | See New finding 3 |
| First insert | Z6, C14, D2 | See New finding 2 |
| First insert | Z1, C2, D1, Z8, C10 | See New finding 1 |
| M6 | C15, T7 | Section advance, last-section behaviour and transport scope. Line 735 already holds the core decision |
| M7 | Z7, C6, C7, D5 | Send and mix tracks only, in a fixed topology, so no cycle check is needed. Add the jam gate that M8 and M10 have. Buses when a jam asks |
| M10 | Machine tails and voice copy cost (D2) | Nothing before a synth exists |

**Rules for marks, in four sentences.** They replace D4's state table:

*   A lock ends at the next note row, or the next value of the same parameter. This matches per-parameter `set_held` (`track_state.rs:366`).
*   A glide targets the next value of that parameter in its slot within the phrase's own loop, which is what line 600 already says. A phrase then means the same everywhere, which removes the chain, section and shared-context cases.
*   When a parameter appears in two slots, the rightmost wins, since slots already run left to right (333).
*   A glide's arrival takes the arrival row's own mark.

## Reject as over-specification

*   **D11, T10, T11, T3:** new Alternatives, Risks, CUJs and a cross-reference sweep. Each milestone's design can add the one it needs.
*   **D2:** effect-instance IDs, a non-version handoff and a memory budget.
*   **D4:** the mark state table.
*   **D6, Z17, C8:** stem definitions per track kind before those kinds exist.
*   **D9:** linked sounds. **Z4:** a cross-track choke and a mixer milestone.
*   **D10:** rework of long moves. A long phrase is fine until a song shows otherwise.
*   **C11:** scheduling every orphan. Delete the promise instead.
*   **C15, Z2:** tests for M9 and M10, and a falsifiable "jam" exit. For a solo experiment, line 1035 is enough, and G1 holds the Stop test.
*   **Z6:** settling effect state "before M4". M4 has no effect state.
*   **S8, S10, S12-S28, T5 (other than "slot"), T6, T12-T18, Z19, C16, C18, C21-C23, D15:** wording and citation tidying. Fix only what a milestone's design touches. Add Glossary terms when their feature is built.

## New findings

**1. Major: M5 builds the parameter list before anything uses it.** Lines 543, 579, M5.
*   By `track_page.rs`, 9 of 37 page controls are two-hex values (`is_hex`, line 151). Moving the page onto the list buys little.
*   The list's payoff, "no new UI", arrives with the first effect or machine. Reports Z8 and C10 argue for extending the list, but building it should wait.
*   Dissolve M5. Page splitting is a small change to the `ROWS` constant, so Sound and Mod ship with M4, which adds a sample setting to a full page. The FX page ships with inserts.
*   Until then, extending `FxCommand` covers the existing hex values.
*   At the first insert, one rule replaces D1's list: slots name a fixed global parameter enum, only one-byte continuous parameters can be named, a parameter the track lacks does nothing and shows dim, and a track holds one effect of each type. This downgrades Z1 and D1 from Blockers. Line 933's load error survives as is.

**2. Major: the effect-state problem is mostly self-inflicted, and an existing pattern solves the rest.** Lines 562, 1041.
*   The 147-369 MB figures assume heavy delays or granular in all 192 inserts. Those effects are in line 562 but on no milestone.
*   Rule: inserts hold small fixed state inline. Compressor, saturation, bitcrush and EQ need almost no buffer, and a 50 ms stereo chorus is about 19 KB, or under 4 MB across all inserts. Large buffers live only on a small fixed number of send tracks, preallocated as `engine.rs:71-72` does today. Character delay and granular go there, if wanted.
*   Swaps reuse the filter's existing rule: when the type differs, start from silence (`track_state.rs:247-251`). Comparing each block makes this immune to skipped versions and undo. A reorder resets that state, which is acceptable for now.

**3. Nit: stems may not need tap points.** Lines 537, M4, M6.
*   `render_loop` already sets mute and solo per track (`render.rs:55-58`), so a stem can be a solo render.
*   That needs no new engine path. Each stem carries its own wet signal, which removes the "without sends" exception. With the limiter off, the stems may sum to the full render.
*   Stems could ship before M4.
*   **Unverified:** the reviewer didn't read `delay.rs` or `reverb.rs` for linearity, or check whether muted tracks still draw `CHA` random numbers. Test both. The sum test ends when a nonlinear mix effect arrives, which is fine.

**4. Nit: M4 doesn't need a machine enum.** Lines 534, 539, 1037.
*   A one-variant enum and standalone sound save and load are unearned so far. `Instrument` holds only `sample`, `volume` and `labelled` (`song.rs:336-343`).
*   Add the enum with the slicer. Add standalone sound files if a jam asks, and M4's test then shrinks to kits and track sounds saved in the song.

**5. Nit: build glide after lock and hold.** Lines 581, 585, 600, Decision 5.
*   This changes the order only, not the decision. Glide is the only mark that needs look-ahead and ramp state.
*   Line 133's listening question is still open, so that result can inform it. D14's objections can then be judged by ear.

## Revised sequence (changes to Z's)

1. Run G7.
2. Marks: lock and hold on the six held values, then glide. These are independent of M4, and the smallest experiment goes first.
3. M4 with Sound and Mod pages, and stems.
4. Cross-track editing from M9.
5. M6.
6. G1 on songs, which gates the rest.
7. Inserts with one small effect, the FX page and the parameter list.
8. Send and mix tracks.
9. Effects, buses, the mixer and machines, as jams ask.
