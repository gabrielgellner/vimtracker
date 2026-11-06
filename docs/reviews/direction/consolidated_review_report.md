# Consolidated review: Direction and Milestones after M3

Target: `docs/designs/vimtracker.md`, focused on "Direction: a track is one sound" (lines 518-619) and "Milestones after M3" (lines 1033-1044). Line numbers are for the current uncommitted file. No edits have been applied.

**Verdict: don't start M4 yet, but the distance is short.** Three things block any code, and all three are short edits to the document: the order and gate, a statement of how ready M4-M10 are, and the M7/M8 order. Four more block their own milestones and can wait until then. The review raised 172 raw findings, which reduce to the 24 tasks below.

**The main risk in acting on this report is over-correcting.** Stages 5-7 and 9 hold a 100-line direction to the standard of a finished spec. Stage 8 argues that most of it should wait, because plans here get overtaken in play and marks are meant to be tried by ear. I followed Stage 8 on timing, and overruled it where listed below.

## Reports

*   [Stage 5, zoom-out](zoom_out_report.md): Z1-Z19
*   [Stage 6, comprehension](comprehension_report.md): C1-C23
*   [Stage 7, critic](critic_report.md): D1-D15, T1-T18, S1-S28
*   [Stage 8, intermediate zoom-out](intermediate_zoom_out_report.md): O1-O6 (its "settle before any code" items), ON1-ON5 (its new findings)
*   [Stage 9, readiness](implementation_readiness_report.md): I-B, I-M, I-N, K-B, K-M, K-N, Y1-Y17

## Where I overruled the reviewers

I checked these against the repository, GitHub and the rules already agreed for this project.

*   **The "dangling" Filters milestone is real (Y1, K-B1, I-N1, C12, Z18, O6).** Two reviewers rated this a Blocker and Stage 8 said to delete the clause. GitHub milestone 4 "Filters" exists, and #50 "Diode and MS-20 style filters" is open in it. The reviewers could only see the repository. It's now a Nit: say that it's on GitHub.
*   **"Decide what happens to existing jams" is already decided (O3, Z11, D13, I-M3).** Standing rule: no backwards compatibility until you say otherwise. So there's no converter and no bundling of format breaks, and I've dropped Stage 8's suggestion to keep a v1 binary. What's left is to say plainly that M4 stops old songs loading. Z10's complaint that lock-by-default changes old songs falls away for the same reason.
*   **The size of removing instruments (Z14, O, I-M2).** The reviewers gave three different counts. Measured: 486 lines across 40 `.rs` files mention "instrument", case-insensitively, including tests and comments.
*   **Stage 8's mixer and one-effect rules touch your own calls.** It would delete the mixer promise and limit a track to one effect of each type. Reviewers can't settle either, so both are questions below.

Also confirmed directly: the text of lines 743 and 1035, `TrackState` deriving `Copy`, `FORMAT_VERSION` being 1, and `Space 2`-`Space 5` being the only numbered view bindings. All other findings are as the reviewers reported them and I didn't re-check them. That includes Stage 8's stem idea, which it marks as unverified.

## Questions for you

1.  **When does G1 run?** After M6 on songs, keeping its Stop test and changing only "2 loops" (Stage 8's suggestion). Or run it now, in five sessions on loops (D7's option), which gives evidence before a refactor touching 40 files but tests the old aim. Or retire it, on the view that you've already answered "is this worth continuing?".
2.  **Does the parameter list wait?** Stage 8 would dissolve M5: page splitting ships with M4, marks go first on the existing values, and the list arrives with the first effect. The cost is that the track page keeps its parallel lists until then. The counter-argument is that `Control`, `TrackControl`, `Destination` and `FxCommand` are already parallel, and each new parameter adds to all of them.
3.  **Can a track hold two of the same effect?** One of each type makes `CMP40` unambiguous and needs no parameter addressing. It rules out two compressors or two EQs on a track, and you chose 3 reorderable slots with that possibility open. Allowing duplicates needs names per insert, such as `CMP` plus a position.
4.  **When does the mixer come?** Moved up, it pays for M4, as one sound per track spreads a kit's levels across a page per track. Stage 8 would drop the promise until a jam asks. You'd know quickly: a mixer is a page you either keep wanting or don't.

## 🔴 Blockers: settle before any code

**1. State one order and one gate.** Lines 73-84, 522, 607-608, 730, 743, 1033-1044.
Line 743 reads "Work stops at Issue 23 until the G1 outcome is recorded", so it forbids M4. Line 730 has G1 choose what's next, and line 607 defers G1. "Before G1" is also a scope label at lines 260, 284, 294, 327, 332 and 445.
*Do:* replace these with one short passage, depending on question 1. Add one sentence closing M3 without Issue 18, and park G4 and CUJ 4 (M3 has 6 open issues on GitHub, so check which are really open). Say whether Issues 20-22 must finish first. No new Goal or CUJ.

**2. Say that M4-M10 aren't ready to implement.** Lines 730, 743, 1033.
Each gets a short design when it starts, with its issues and file lists. Stage 8 calls this the most valuable sentence available, as it legitimately defers most of the gaps listed below. It costs nothing, and it means later work must honour it.

**3. Fix the milestone order.** Lines 553-556, 562, 1040-1042.
M7 needs M8's slots. Swap them: inserts on sound tracks need no routing and can be heard at once, send tracks then reuse them, and sidechain comes last. Keep M4 before M6, and write down why: songs shouldn't be made in a format about to break. Move M9's cross-track selections and operators to directly after M4, as line 538 already names that cost; folding stays in M9.

## 🔴 Blockers: settle before the named milestone

**4. Mark rules, before marks are built.** Lines 587-603, 1038.
Stage 8's four rules replace a full state table, and fit what you chose (lock until next note or value, slide from here):
*   A lock ends at the next note row, or the next value of the same parameter.
*   A glide targets the next value of that parameter in its slot, within the phrase's own loop.
*   When a parameter appears in two slots, the rightmost wins.
*   A glide's arrival takes the arrival row's own mark.

Everything else falls to the fallback rule (449) until playing says otherwise. **Worth trying early:** a sweep ending `CUTC0 .` jumps back at the next note under the default mark, which may be the first thing you notice.
*Build:* split marks from the parameter list and do lock and hold first, on the six held values. Glide comes after, as it needs look-ahead and ramp state that don't exist (`engine.rs:605`). Lock needs a new layer, since base and held share one target today (`track_state.rs:366-378`), and `VOL` has no page base (`track_state.rs:215, 501`).

**5. Parameter identity and values, before a step can name a new parameter.** Lines 543, 558, 579, 933.
`DEC40` has no defined meaning across shared phrases, and a two-hex value can't hold most page settings. Stage 8's rule: a fixed global parameter enum; only one-byte continuous parameters can be named; a parameter the track lacks does nothing and shows dim; line 933's load error stays. Depends on questions 2 and 3. Correct line 558's "self-contained".

**6. Effect state, before the first insert.** Lines 539, 562, 680; `track_state.rs:78`.
Preallocating heavy effects in all 192 inserts costs 147-369 MB, and `TrackState` is `Copy`. Stage 8's rule: inserts hold small fixed state inline (a 50 ms chorus is about 19 KB); large buffers live only on a small fixed number of send tracks; on a type change, start from silence, as the filter does (`track_state.rs:247-251`). The cost is that a delay or granular can't be an insert on a sound track, so character delay on a single track means a dedicated send.

**7. Routing, before M7.** Lines 549-558, 1040.
Start with send and mix tracks in a fixed topology, so no cycle check is needed. Buses when a jam asks. Its design must answer: what a note, `KIL`, `RET` or a lock means with no machine; whether these count against the 64 tracks; and `:track add` given index-based RNG. It must also cover solo and sends, what replaces `Track.delay` and `DLY`, and how `Engine::render` handles a track with no voice, since it skips non-`sounding()` tracks today (`engine.rs:749`).

## 🟡 Major

**8. Rename the effect holder "insert".** Lines 61, 535, 562, 579, 582, 587, 615, 1038, 1041. Use "signal path" at 527 and 615. Several reviewers rated this a Blocker because of how much misreading it causes, but it's a short edit. It also dissolves the apparent contradiction between a per-track setting and 3 slots.

**9. Run G7 now.** Lines 100-102, 132, 527. One 10-minute run against a 40-file refactor. A plausible song needs 17 tracks, over G7's 16, before effects. No scheduled reruns or effect budget.

**10. Correct what the document says is built.** Lines 520, 806, 935, 939-997, 1018; also Y14, Y15.
Soften line 520. Fix Target Files, the `Space` table (with the track page added), attack and release on instruments, the shape panel description, `VOL` and `VEL`, and the transport keys. M4 needs this list, as it's the only map of the code.

**11. Add the rest of Direction's effects to the old text.** Lines 133-137, 246, 255, 333-336, 385, 420-421, 445, 730-737. Add forward pointers, and say that glide is a remedy for the smoothing question rather than its answer. Do this only as far as each milestone touches them, not as a sweep.

**12. M4's scope, in its short design.** Lines 527, 534-536, 1037.
*   `Sound` is `Track` minus mute, solo, swing and name, plus `sample` and `volume`.
*   List what's retired: `:inst`, `:label`, `:sample`, `Space i K`, column `K`, `Field::Instrument`, the instrument view.
*   FX column indices shift (`selection.rs:12-54`), and the preview voice bypasses the track chain (`engine.rs:767`), so it won't sound like the track.
*   Repoint the existing browser (`picking.rs:21-53`), and extend `KitSound`.
*   Consider leaving out a one-variant machine enum and standalone sound files until something needs them (ON4).
*   Accept separately edited copies across a chord (D9), and check whether you miss open and closed hat choke (Z4).

**13. Say the format break plainly.** Lines 112-114, 512. M4 stops old songs loading: removing `INST` breaks `steptext.rs:130-141`, and `FORMAT_VERSION` should go up. Render anything you want to keep first.

**14. Stems, in M4's design.** Lines 537, 1037, 1039.
Stage 8's idea is that a stem is a solo render, needing no new engine path and carrying its own wet signal. To check first: the limiter off, as it clips (`mixer.rs:14-23`); different file lengths from `--tail` (`render.rs:72-90`); saved mute and solo (`render.rs:55-58`); a tolerance for the sum; delay and reverb linearity; and `CHA` random numbers under mute. Stems could ship before M4. The sum test ends when a nonlinear mix effect arrives, so reword line 1039.

**15. M6's scope, in its short design.** Lines 260-263, 294, 735, 1039.
This is bigger than it reads. There's no per-track position to carry, only a global `Engine.place` (`engine.rs:59-61`, `602`). `Section.reset` means something else today (`song.rs:154-156`), and about 20 sites assume the first section (K-M4). A whole-song render has no loop to warm up with. It also needs end-of-song, section length, what crosses a boundary, and which section the views follow. Direction has no design section for songs, its top-ranked decision.

**16. Pages.** Lines 543, 566, 1038. Depends on question 2. Page splitting is a small `ROWS` change and M4 needs the room. `H` and `L` need an override against line 882's rule, plus a table entry.

**17. Delete or schedule the orphans.** Lines 121, 538, 539, 562, 582, 585. Mixer (question 4), character delay and granular (121 says planned), shape panel glide, and the per-track slot count (delete row 582 unless you want it). Delete line 539's "should allow more".

**18. M10 machines, in their short design.** Lines 534, 539, 733, 1043. How a slicer picks slices without sample locks, and voices for Plaits-style.

## 🟢 Nit

**19. Filters clause.** Line 1035: "The Filters milestone on GitHub stays open for #50."

**20. Word clashes.** "M8" (write "Dirtywave M8" for the hardware); "sound"; "voices"; `.` as lock glyph and repeat; "marks" at 737 (likely Vim's); "section" at 608; "core effects, in order"; `H` and `L`.

**21. Mark editing details, when building marks.** Line 603 against the example, `~` on an event or empty slot, `~` in Visual over mixed marks, whether "nearest above" wraps or copies across commands, whether `y`, `p`, `d`, rotate and Shape carry marks, and the on-file form.

**22. Writing.** Claims to attribute or cut (521, 527, 538, 562, 570, 573, 617), "decided" where the text hedges (535, 539, 587, 602), and small wording at 522, 538, 575-577, 582-583, 585, 587, 607-608, 1035-1043. Fix in passing, not as a pass of its own. Add Glossary terms as features are built. Cite sources (Syntakt, Tonverk, Heat, Griesinger, Plaits) as they're used.

**23. Code details.** Line 562's ping-pong is a setting that's missing, as feedback already cross-couples (`delay.rs:29`). Line 573's rotate moves whole steps (`song.rs:317-323`), so "rotate ... work on it" overstates. Line 531's "any of these" needs `Destination` to grow. Line 566's rows are about 21. Reorder with `dd` and `p` overwrites a neighbour (422), so settle that in the first insert's design.

**24. Outside the focus.** Stray backticks at 373, doubled table rows (824/835, 828/836), `+` and `=`, and vague phrases at 749, 1018, 1031, 1059.

## Dropped unless you disagree

*   **New Alternatives, Risks and CUJs for Direction (D11, T10, T11).** Each milestone's design can add the one it needs.
*   **Full mark state table (D4), parameter schema before use (D1's list), effect-instance IDs and memory budget (D2), per-kind stem definitions (D6).** Replaced by the short rules above.
*   **Linked sounds (D9), cross-track choke (Z4).** Wait for a jam to ask.
*   **Long-move rework (D10).** A long phrase is fine until a song shows otherwise. Line 627 does overstate, so soften it in passing.
*   **Tests for M9 and M10, and a jam exit that can fail (I-M10, Y4, Y5, Y10).** Tests come with each milestone's design. For a solo experiment, "used it in a jam" is enough and G1 holds the Stop test.
*   **Bundling format breaks (Z11), a converter, a v1 binary (O3).** See "Where I overruled the reviewers".

## Suggested sequence

Depends on the questions above.

1.  Edit the document: tasks 1, 2, 3, 8, 19.
2.  Run G7 (task 9).
3.  Marks: lock and hold, then glide (task 4).
4.  M4 with pages and stems (tasks 10, 12, 13, 14, 16).
5.  Cross-track editing from M9.
6.  M6 (task 15).
7.  G1 on songs, if that's the answer to question 1.
8.  Inserts with one small effect, the FX page and the parameter list (tasks 5, 6).
9.  Send and mix tracks (task 7).
10. Effects, buses, the mixer and machines as jams ask (tasks 17, 18).

## Traceability matrix

| Raw finding | Task |
|---|---|
| Z1 | 5 |
| Z2 | 1 |
| Z3 | 9 |
| Z4 | 3, 12, dropped in part |
| Z5, Z13 | 3 |
| Z6 | 6 |
| Z7 | 7 |
| Z8 | 5 |
| Z9, Z10 | 4 |
| Z11 | 13, dropped in part |
| Z12 | 8 |
| Z14 | 12 |
| Z15, Z16 | 17 |
| Z17 | 14 |
| Z18 | 11, 19 |
| Z19 | 20 |
| C1 | 8 |
| C2, C10 | 5 |
| C3 | 4 |
| C4 | 1 |
| C5 | 3 |
| C6, C7 | 7 |
| C8 | 14 |
| C9 | 12 |
| C11 | 17 |
| C12 | 19 |
| C13 | 10 |
| C14 | 6 |
| C15 | 15 |
| C16, C17 | 21 |
| C18, C21 | 20 |
| C19 | 23 |
| C20 | 16 |
| C22 | 22 |
| C23 | 11 |
| D1 | 5 |
| D2 | 6 |
| D3 | 3 |
| D4, D14 | 4 |
| D5 | 7 |
| D6 | 14 |
| D7 | 1 |
| D8 | 9 |
| D9 | 3, 12 |
| D10, D11 | dropped |
| D12 | 8 |
| D13 | 13 |
| D15 | 21 |
| T1, T4 | 1 |
| T2 | 10 |
| T3 | 11 |
| T5 | 8, 20 |
| T6 | 22 |
| T7 | 15 |
| T8 | 3 |
| T9 | 2, 17 |
| T10 | 1, dropped in part |
| T11 | dropped |
| T12, T13 | 20 |
| T14, T15, T17, T18 | 22 |
| T16 | 21 |
| S1, S2 | 8 |
| S3, S4, S6, S19, S20, S26 | 20 |
| S5 | 4 |
| S7 | 2, 22 |
| S8, S10, S12-S18, S21, S22, S24, S25, S27, S28 | 22 |
| S9 | 11 |
| S11 | 7 |
| S23 | 21 |
| O1 | 1 |
| O2 | 2 |
| O3 | 13, dropped in part |
| O4 | 9 |
| O5 | 3 |
| O6 | 8, 10, 17, 19 |
| ON1 | 5, 16 |
| ON2 | 6 |
| ON3 | 14 |
| ON4 | 12 |
| ON5 | 4 |
| I-B1, I-M10 | 2 |
| I-B2 | 1 |
| I-B3 | 3 |
| I-B4, I-B5 | 5 |
| I-B6 | 4 |
| I-B7 | 6 |
| I-B8, I-N3 | 7 |
| I-M1, I-M2, I-N4 | 12 |
| I-M3 | 13 |
| I-M4 | 14 |
| I-M5 | 15 |
| I-M6 | 16 |
| I-M7 | 21 |
| I-M8 | 17 |
| I-M9 | 8 |
| I-M11 | 18 |
| I-N1 | 19 |
| I-N2 | 11 |
| I-N5 | 20 |
| K-B1 | 19 |
| K-B2, K-B3 | 10 |
| K-M1, K-M2 | 12 |
| K-M3, K-M4 | 15 |
| K-M5 | 14 |
| K-M6 | 5 |
| K-M7, K-M8 | 4 |
| K-M9 | 6, 7 |
| K-N1 | 8 |
| K-N2 | 16 |
| K-N3, K-N4 | 23 |
| K-N5 | 20, 23 |
| Y1 | 19 |
| Y2, Y7 | 20 |
| Y3 | 1 |
| Y4, Y5, Y10 | 2, dropped in part |
| Y6 | 8 |
| Y8 | 22 |
| Y9 | 14 |
| Y11, Y12, Y13 | 22 |
| Y14, Y15 | 10 |
| Y16, Y17 | 24 |
