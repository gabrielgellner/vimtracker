# Stage 5: Initial zoom-out review

Target: `docs/designs/vimtracker.md`, focused on "Direction: a track is one sound" (lines 518-619) and "Milestones after M3" (lines 1033-1044).

**Verdict: not ready to implement as sequenced.** 1 Blocker, scoped to M5, and 10 Majors. The objective is clear, so the context check passes. The core ideas hold up: value columns as automation lanes, chains kept under sections, and "only what's wanted in a jam" gating on M8 and M10. The problems are unresolved identity and state questions, milestone order, and a stale decision gate.

## Blocker

**1. A stored FX name has no defined meaning once parameters depend on the track.** Lines 543, 558, 562, 579; M5, M8. Context: lines 933, 57, G3 (line 87).
`FxCommand` is a closed 11-variant enum saved by three-letter name (`step.rs:29`), and an unknown name is a load error (line 933). "A slot can name any parameter in the track's list" makes a name's meaning depend on the track, and the document doesn't say how it resolves. Unanswered: the same name on different things (`DEC` on a reverb and an envelope); two choruses on one track; a step naming an effect later removed or moved; a phrase reused on a track that lacks the parameter (nothing stops a chain or phrase being used on two tracks); validation at load, as `Step` parses alone with no track in sight. Line 558's "keeps a track's phrase self-contained" is false, as the phrase now depends on what its track has loaded. This sets the file format, so it must be settled before M5.
Options: a small fixed global namespace of parameter IDs with an unresolved one a silent no-op shown dimmed; or restrict phrases to one track kind. Replace the load-error rule either way.

## Major

**2. The G1 gate is stale, and nothing replaces it.** Lines 520-522, 607-608, 1035, against 17-19, 73-83, 730, 743.
Line 743 still says work stops at Issue 23 until G1 is recorded, and line 730 says G1 decides what's next, yet Direction schedules seven milestones first. Line 520's precedence rule doesn't help, as these are live rules. The new aim has no Goal or CUJ, G1's Continue, Redirect and Stop have no successor, and "used in a jam" can't fail. Fix: rewrite 730 and 743, add a song goal with a CUJ, and place G1 as a named step after M6 at the latest with stop criteria kept.

**3. "Tracks are plentiful" is an unmeasured premise, and the test is cheap.** Lines 527, 538, 607, 1040. Context: 100-102, 132.
Track capacity is still an Open Question and G7 hasn't run, while Direction raises load on every front. G7 needs no song structure. Fix: separate G7 from G1, run it before M4, and rerun with a realistic load before M7 and M10.

**4. M4 creates the cost, but its remedies arrive much later or never.** Line 538 against M4, M9. Context: 389, 415-417.
The score view is M9, five milestones later, and the mixer overview has no milestone. Until M9 a drum part spans tracks while `dd`, `ib`, `.`, `>` and selections reach only one, weakening the project's first idea. Spreading drums also loses open and closed hat choke, which comes free from one voice per track (line 259), with no cross-track choke planned. Fix: move the multi-track subset of M9 to directly after M4, and give the mixer a slot or cut it.

**5. M7 depends on M8.** M7, M8, lines 553-556, 558, 562.
Bus, send and mix tracks are defined by holding FX slots, which arrive a milestone later, and M7's test needs them. Fix: slot mechanism, then track kinds, then individual effects, splitting M8.

**6. No plan for where effect and machine state lives on the audio thread.** Line 562, 539, M7, M8, M10. Context: 276-277, 680, 935.
Line 539 answers "no allocation" for machines only. The engine has one `Delay` and one `Reverb` (`engine.rs:71-72`) and a flat `Box<[TrackState; MAX_TRACKS]>` of a `Copy` struct. Line 935's 2.25 s stereo delay is about 864 KB, so 64 x 3 slots is about 166 MB before granular or a real reverb. Building state on the edit thread and handing it across is the handoff "Growing tracks live" rejected (line 680). Choosing an effect with `K` during playback is therefore open. Fix: decide before M4 fixes the machine enum's shape, perhaps a small preallocated buffer pool with a stated limit, and light effects inline.

**7. M7 builds four track kinds and routing up front, breaking the jam-gating used elsewhere.** Lines 549-558, M7. Context: 115-117, 284-285, 378-380, 654.
"Same page, phrases and grammar" hides differences: the note column, `VEL`, `KIL`, `RET`, `CHA`, audition and preview have no meaning on a track with no machine, the objection used to reject track types (line 654). A lock lasts "until the next note", and a note-less track has none. Where does a mix track sit on `:track add`, given only the last track can be removed and RNG is seeded by index? Sends and buses need an ordering the engine lacks, and sidechain adds an edge. Solo needs a rule that keeps sends audible. Simpler first cut: send tracks and a mix track, with a fixed topology and no cycle check, and buses when a jam asks.

**8. The shared parameter list is justified, but specified only as a UI feature.** Lines 543, 601, M5.
`Control`, `TrackControl`, `Destination` and `FxCommand` are parallel lists, so consolidation is warranted. But the real cost is on the engine side: each lockable, modulatable parameter needs base, hold, lock, LFO and envelope layers, today hand-written per parameter. M5's tests only exercise keys. "Name, range, units and default" doesn't cover `Rate`, `time_step`, signed depth or swing, most of which don't fit a one-byte FX value, so "any parameter" can't literally hold. Machines live in `vt-engine`, but `vt-tui` and step parsing need their declarations in `vt-model`. Fix: define the list as u8-valued continuous parameters, leave odd settings bespoke, and state the engine runtime model.

**9. M5 ties the cheapest experiment to the largest refactor.** M5, line 587.
Marks are meant to be tried in play, need neither the list nor the page split, and work on the six existing held values plus one field per `FxSlot`. They also answer two Open Questions. Fix: split marks out and build them first.

**10. Lock and glide rest on "the next value in the slot", which isn't defined across real playback.** Lines 598-601, M5. Context: 402, 87.
Undefined: a phrase in a multi-phrase chain doesn't loop on itself; the next value may name a different parameter; two slots may name one parameter with different marks; a `CHA`-skipped note; line 402's "no look-ahead" no longer holds for glide, which needs a chain-aware scan from the callback up to `MAX_PHRASE_STEPS`; a shared phrase can glide differently per context; the default silently changes every existing `CUT` from hold to lock.

**11. M4 is the first real format break, and the policy predates the new aim.** M4, lines 603, 112-114.
`FORMAT_VERSION` is still 1. Removing `INST` changes the positional step string, so `Step::from_str` rejects every existing jam, as will marks if they enter the text. The non-goal was written for a throwaway prototype, and line 521 says jams now exceed expectations. WAV doesn't keep an editable project. Fix: group M4 and M5 step-format changes into one break before M6, and decide whether a small one-off converter is worth it.

**12. "Slot" now means two different things.** Lines 61, 530, 562, 579-582, 603, 615, M5, M8.
A step's command and value, and a track's effect insert. Line 582's "per-track setting" and line 615's "3 reorderable slots" look contradictory but differ. Fix: use "insert" for the effect position, and add Bus, Send and Mix to the glossary.

## Nit

**13. Song-first order is not argued.** Two restructuring milestones precede M6, while the model already has `sections`. The format break justifies M4 first, but not M5's list. State the reason, or move the list after M6.

**14. Reuse is available.** M4's sample browser should repoint the existing one (`picking.rs:333-354`). `kit.rs`'s `KitSound` should be extended. `Instrument` holds only `sample`, `volume` and `labelled`, while `Track` holds nearly all of the sound, so `Sound` is mostly a split of `Track`. M4 should list what it retires, as it's unscoped: `instruments.rs`, `views/instrument.rs`, `label.rs`, `:inst`, `:label`, `Space i K`. About 510 `instrument` references across 49 files.

**15. Per-track slot count (582) is speculative and unscheduled.** Delete the row.

**16. The polyphony interface (539) is speculative and at odds with the document** (line 53, G3, line 655). M10 already says it's decided when needed.

**17. Stems are tied to M4 unnecessarily, and their test weakens later.** Per-track stems could ship now. "Sum without sends" doesn't say where wet signal goes, and "still sum" stops being true with buses and a nonlinear mix compressor. Define a stem for each track kind.

**18. Dangling or stale text.** Line 1035's "Filters milestone" and "Issue 50" aren't in M4-M10. Line 562 lists character delay and granular, which M8 and M10 don't. Line 607 is silent on Issue 20, and line 406 says a scope is still to come. G4 and CUJ 4 still require live sweeps on hold. Open Questions 133-137 aren't updated for marks and glide.

**19. Name collisions.** "Sound" is a track kind, a type, a saved item and a page. "Marks" collide with Vim-style marks (737). `.` is both the lock glyph and repeat. `H`/`L` for pages reuse Vim's screen-position keys under a rule that inherited keys keep their meaning (882).

## Suggested sequence

1. Run G7 now.
2. M4 with stems, and bundle the format change with marks.
3. Multi-track editing from M9, and a mixer decision.
4. M6 Songs.
5. G1 with a song-scale criterion.
6. Parameter list and pages.
7. Slot mechanism, after settling findings 1 and 6.
8. Send and mix tracks.
9. Effects and machines as jams ask for them.
