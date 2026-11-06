# Stage 7: Critic review

Target: `docs/designs/vimtracker.md`, lines 1-740, focused on "Direction: a track is one sound" (lines 518-619). Reviewers: design critic, structural and style. Findings repeated in Stages 5 and 6 are kept short here.

## Design critic

**Verdict: not ready to build as written.** The direction is coherent as a musical idea, and keeping FX columns instead of separate lanes is well argued. 2 Blockers, 9 Majors, 4 Nits. Limits: read lines 1-740 and 1025-1054, and the three baseline files, but not `step.rs` or the sequencer.

### Blockers

**D1. Slot values cannot represent "any parameter".** Lines 543, 579, 591-595, M5; `track_page.rs`; `track_state.rs:366`.
*   **Encoding:** a slot is a command and two hex digits, and `set_held` takes a `u8`. The page has times to 8000 ms, signed depths, degrees, tenths of a percent, text rates and enums.
*   **Metadata:** beyond name, range, units and default, it needs typed or picked, nudge sizes, value-dependent steps, picker contents, value curves (dB levels, piecewise pan), smoothing class, whether glide is meaningful, and how depth maps.
*   **Addressing, the worst part:** `DEC40` could be envelope, reverb or kick decay. Two compressors are ambiguous. Addressing by position means reorder changes phrases, and by type means duplicates collide. Loading a sound or changing an effect leaves values naming nothing. A phrase can be used on tracks with different machines, so line 558's "self-contained" is false.
*   **Recommendation:** before M5, specify a stable parameter identity, the slot-value mapping, which parameters can't be locked, behaviour of an unresolved command on play, display, paste and load, and the duplicate-effect rule.

**D2. No real-time design for effect state, and machine state as written does not scale.** Lines 276-277, 539, 562, 680, 1040-1043; `track_state.rs:78-81, 346-352`.
*   Line 539 covers machines only, not the slots carrying heavy buffers.
*   Inline preallocation for 192 slots: about 147 MB for 2 s delays, 369 MB for 5 s granular (illustrative).
*   `TrackState` is `Copy`, and `cut_to_tail` copies a whole voice on the audio thread, so large machine state makes every note expensive. The tail becomes a second machine and the preview a third.
*   Allocating on demand needs a handoff like the one rejected at line 680, and the depth-1 replacing queue (269) is where versions get skipped.
*   Undefined: state following a reordered effect, what undo restores, a skipped version that adds then removes an effect.
*   `sounding()` looks only at voices, so an idle check would cut off a ringing effect. Pause and stop for effect tails are undefined. CUJ 7's silent pass names only delay and reverb.
*   **Recommendation:** an architecture subsection on where state lives, stable effect-instance IDs, a handoff not dependent on song versions, a memory budget and idle slot cost.

### Major

**D3. M7 needs M8.** Lines 553-556, 562, 1040-1041. Move the slot container into M7, leaving effect implementations in M8.

**D4. Glide is missing from the layer model, and marks are underspecified where they meet song structure.** Lines 598-603, 1038; `track_state.rs:366-378`.
*   Is a glide in progress a lock or a hold? Does a note stop it? What remains on arrival?
*   With lock the default, a sweep ending in `CUTC0 .` jumps away from where it landed at the next note.
*   A slot can mix commands (465): what does `CUT80 g` do before `DLY30`? Does `DLY` end a `CUT` lock? If FX1 and FX2 set `CUT`, which wins? Make the rule per-parameter or restrict mixing.
*   Chain and section boundaries: wrapping is defined for a phrase, and locks and holds leak across them.
*   Does a lock end on `RET`, a `CHA`-skipped note, or a `DEL` note's row or delayed start?
*   Edits, loop slides and rotates can move a glide's target. Page edits go inaudible under a hold or lock.
*   `set_held` keeps one target per parameter, so this is an engine state change. "Lands exactly" needs defining given the smoother, 32-frame updates and LFO offsets. "Rules can change cheaply" is true of storage only.
*   **Recommendation:** a small state table: each mark against note, value, event, wrap, chain boundary, section change, edit and transport.

**D5. Non-sound tracks inherit rules that assume notes.** Lines 549-558, 598; `song.rs:442-460, 475-483`; lines 284, 351, 366, 378-380. Does a note on a bus do nothing, or trigger envelopes? A lock with no notes behaves like a hold. `KIL` and `RET` have no meaning. Do delay, reverb and mix count towards the 8 tracks and limit of 64? A sound added after a send has no defined position, given end-only add and remove and RNG by index. Solo and mute through routing. "Sends or a bus" reads as exclusive. Bus-to-bus and send-to-send are unspecified. Sidechain adds cycles not covered by the feedback test. Processing order must be derived without allocating. `ShapedFrame`, `Destination` and `Track` hardcode delay and reverb, so how does a track address N sends?

**D6. The stem-sum guarantee does not survive the plan.** Lines 527, 537, 1037, 1039, 393. The master clips, wet signal from sends is in no stem, a bus stem double-counts, a mix compressor is nonlinear, and sidechain means stems must be tapped during one render. Define a stem's tap point and a guarantee that survives M7 and M8.

**D7. G1 no longer works as the gate, and the new aim has no success test.** Lines 73-84, 522-523, 607-608, 730, 1035. Either run G1 now (5 sessions), or formally replace it with a song-scale G1 with stop criteria, and fix lines 73 and 730. Write the decision down.

**D8. "Tracks are plentiful" is unmeasured.** Lines 100-102, 132, 527, 607. A plausible song needs more than G7's 16: 8 drums, bass, lead, a 3-note pad, 2 sends, a bus and a mix is 17, before effects. Run G7 before M4, repeat at M8 with a realistic song, and set an effect budget.

**D9. The cost of one sound per track is deferred or unmitigated.** Line 538, 1037-1043; `song.rs:393`. No milestone has a mixer. For M4-M8, editing a drum part means moving focus, and cloning a drum variation means cloning one phrase per voice. **An unlisted cost:** instruments are global today, so tracks can share one. After M4, a chord across three tracks needs three separately edited copies of a sound, which affects G3's "polyphony uses several tracks" while real polyphony waits for M10. Add a mixer, move minimal multi-track editing earlier, and decide on linked sounds or accept the limit.

**D10. "Nothing longer than a phrase" is not really fixed.** Lines 583, 554, 562, 627. A long phrase gives up reuse, and a section-long filter sweep can't use bus or send steps, as those have no filter. Line 627's rejection of lanes may be right, but it overstates: slot limit becomes a per-track count, and values between steps are only straight lines.

**D11. Alternatives and Risks requirements aren't met.** Lines 620-634, 503-513. No entry for keeping instruments, dedicated bus and FX objects, other mark designs, or chains versus patterns. The Risks table has nothing on Direction. No CUJs for making a song, swapping a machine or routing to a bus.

### Nits

**D12. "Slot" has two meanings.** See S1. Also, what does a shared phrase do with columns beyond a track's count, and do hidden values still play?

**D13. Default marks change behaviour, and old files have no stated handling.** Lines 112-114, 466, 580, 585; `song.rs:537, 678`. Sparse shape-panel values change meaning too. Multi-instrument tracks can't be mapped, and the author now has jams they value, so confirm that acceptance.

**D14. Glide and smoothing claims are ahead of the evidence.** Lines 133, 399, 468, 581, 585. A dense curve still steps unless every row is `g`, and two `g` endpoints lose Curve and S. Locks make per-note jumps routine, and a jump through the 10 ms smoother sits on a drum's attack, so test by ear. Line 580 sits beside line 136's unconfirmed "appear to revert".

**D15. Small details.** "Nearest value above" can copy a mark from a different command. `~` in Visual over mixed marks. Line 490's step text needs marks. Elektron references aren't in Additional Resources.

## Structural

**Verdict: not structurally ready.** 1 Blocker, 10 Major, 7 Nit. Direction adds a second plan on top of the first without replacing or reconciling it.

### Blocker

**T1. Two conflicting rules for what happens next.** Lines 73, 522, 607-608, 730, 743, 1021, 1033. A reader can't tell whether M4 may start before G1, or whether Stop still ends the project. "Before G1" is used as a scope boundary at lines 260, 284, 294, 327, 332 and 445, and no longer has a known position. One passage needs to state the new order.

### Major

**T2. The precedence rule rests on a false premise.** Lines 4, 520, 1047. Much of the rest isn't built: live state, CUJ 4, G4, the scope. Line 1047's Known Gaps override rule also covers Direction. Status says Proposed over mixed material.

**T3. "What this changes" is far from complete.** Lines 605-608. Also affected, with no pointer: After G1 numbering (730-737), Open Questions (133-137), CUJ 1 (146), Glossary Field (59-60), Song model (246, 255), held values (333-336), the gain formula (385), `>` skipping instrument IDs (420-421), the instrument view (445).

**T4. M3's exit depends on on-hold work.** Lines 90-93, 169-176, 607, 1026-1027. Issue 19 is done when CUJ 3 and CUJ 4 work, and CUJ 4 needs live sweeps. G4 requires them, and line 675 rejects one-shot generators, yet the shape panel leaves plain values. M3 can't formally close.

**T5. Words carry several meanings.** "Slot": a step's value, an effect insert, an ID position (250-252) and a chain position (180). "Sound": four senses. "Lock": sample lock and value mark. "Send": a level and a track kind.

**T6. New terms are missing from the Glossary.** Kit, sound, bus, send track, mix track, stem, mark, lock, hold, glide.

**T7. The first decision has no design section.** Lines 525-603, 614, 735, 1039. Decision 1 ranks sections highest, and Direction has no subsection on them. "Full songs" has the thinnest design. The section's title covers only M4.

**T8. Milestone order conflicts with Direction's dependencies.** M7 needs M8, and the score view is M9. No rationale beyond Decision 1.

**T9. Commitments with no milestone.** Mixer, per-track slot count, character delay and granular (though line 121 says planned), the shape panel's glide. Folding is a fix for M5 but ships at M9. M9 and M10 have no tests. The document should say M4-M10 aren't implementation-ready.

**T10. Direction has no goal or user journey.** Lines 71-105, 141-199, 522-523. No CUJ covers stems, marks, a song, a bus or a send.

**T11. Architecture, risks and alternatives are thin.** Components doesn't place machines, the list, routing or kits. The Song model has no target. Pages are defined for sound tracks only. Decision 2's fixed order, Decisions 3 and 4, machines versus instruments and separate delay and reverb have no Alternatives entries. "Track types" is still rejected (651-657) without pointing to the track kinds.

### Nits

**T12.** "M8" is a milestone and the Dirtywave M8 (13, 322, 735, 1041).
**T13.** "Marks" also at 737, and `.` is both lock and repeat.
**T14.** After G1 and Additional Resources nest under Non-Functional Requirements. "That list" and "the milestones below" should be named.
**T15.** Decisions is partial (machines and "everything is a track" are absent) and restates the body. Subsection order doesn't follow milestones.
**T16.** The example shows two FX columns against line 535's three. "Empty slots show nothing" reads oddly beside `---00`. "Column mode" is undefined.
**T17.** Sources are missing from Additional Resources: Syntakt, Tonverk, Heat, Plaits' MIT claim, Griesinger.
**T18.** Alternatives entries use inconsistent shapes, and line 627's "See Direction" names no subsection.

## Style

**Verdict: changes needed.** 1 Blocker. Sentences are mostly short, and the problems are overloaded terms, unsupported claims and "decided" language the section hedges.

### Blocker

**S1. "Slot" has two meanings, and the count reads as a contradiction.** Lines 61, 535, 562, 579, 582, 587, 615. "Slots are rows" (562) adds to it, as row means a phrase row and line 591 shows slots as columns. Give the holder its own term and a Glossary entry.

### Major

**S2. "Chain" conflicts with the Glossary.** Lines 57, 527, 615. Use "signal path" for 527 and 615.
**S3. "Sound" has at least four meanings.** Lines 518, 527, 536, 553, 566. "Sound locks" is undefined, and Decision 3 says "sample locks".
**S4. "Voices" has a second sense.** Lines 53, 534, 539. "After Plaits" is ambiguous beside first, then, then.
**S5. "Lock" is hard to parse, and may contradict the document.** Lines 136, 580, 598. Line 136 says Elektron locks "appear to revert after their step", unconfirmed. State the behaviour without the comparison.
**S6. "Core effects, in order:" is ambiguous, and line 562 packs too much into one paragraph.**
**S7. "Decided" is stronger than the text supports.** Lines 4, 520, 535, 539, 587, 602, 607. Separate fixed decisions from those to try.
**S8. Subjective claims.** Line 521 "exceeds expectations" sits beside an unrun G1. Line 527 "rarely wanted" needs attributing. Lines 538 "much more valuable", 562 "real settings", 570 "only half", 573 "lose all of this", and 617 "not just another groovebox".
**S9. Glide doesn't answer the smoothing question.** Lines 133, 399-402, 581. It's a remedy for a question a listening test settles.
**S10. Products lack citations.** Lines 522, 547, 558, 562. "Scale of a Maschine" and "UI as the deciding concern" are undefined.
**S11. Line 549 overstates uniformity.** Lines 549, 553-556, 566.

### Nits

**S12.** "Drifted" and "on purpose" (522).
**S13.** "The rest describes what is built" (520).
**S14.** Lines 538 and 607 are loose: name what remains of Issues 17 and 19.
**S15.** "They aren't one thing" (570) has an unclear antecedent.
**S16.** "Weakness" (575-577): rows at 582-583 are fragments.
**S17.** "It needs only to learn glide" (585) minimises the work.
**S18.** "The author's idea" (587).
**S19.** `.` is lock and repeat (598, 602).
**S20.** "Marks" at 737.
**S21.** "Base" is used before it's defined (599, 601).
**S22.** "Cursor stops" and "column mode" are undefined (602).
**S23.** "Empty slots show nothing" (603).
**S24.** Table cells at 555-556 hold misplaced content.
**S25.** Forward references (543, 566, 607-608) lack pointers.
**S26.** "Section" means the document's and a song's within six lines (608, 614).
**S27.** Decisions 2, 3 and 5 repeat the body (615-618).
**S28.** The Glossary is out of step.
**Outside the focus:** line 373 has a stray double backtick.
