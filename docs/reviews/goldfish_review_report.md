Status: resolved

# Goldfish Annotated Review Report

**Target:** `docs/designs/vimtracker.md` (full file; Stage 9 range is lines 590-1007)

**Scope note:** The Stage 9 flow covers `implementation_readiness_report.md`. Stages 5-8 found
the same problems in the feature section, so fixes here also update the matching text above
line 590, and six findings from those stages that affect the build are added as Issues 46-51.

**How to read this:** `ESCALATE` items need your decision. Several `PROPOSE FIX` edits depend on
those decisions, and each says so. Each fix describes the change exactly, and the final wording is
applied in the document's style.

**Counts:** 5 ESCALATE, 51 PROPOSE FIX, 1 CONTEST, 4 DECLINE (groups of nits). Numbering follows
`implementation_readiness_report.md`, except that its item 43 is merged into Issue 53.

---

## Prioritized Findings & AI Suggestions

## Part A: Your decisions (ESCALATE)

### [Issue A1] [`zoom-out`, `critic`, `style`] - Severity: High

*   **Affected Scope:** [Lines 42-58](docs/designs/vimtracker.md#L42-L58), [Lines 953-956](docs/designs/vimtracker.md#L953-L956) (G1 gate)
*   **Discrepancy Description:** G1 has no pass rule, tool or loop, and it runs before tables and multi-track editing.
*   **Rationale:** A "no" could come from unfinished features, and "passes" gates all later work.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** Only you know your usual tool, the loop and what would change your mind. The rule must be written before the result exists.
*   ***Proposed Edit:***
    *   Location: The Problem, G1, Issue 17
    *   Change: You supply the tool, the loop and the rule. Then L42-51 say G1 decides the grammar, live sweeps and shared phrases, and tables get a later gate. Outcomes become continue, redirect, stop or inconclusive. vimtracker runs first. A slower time alone isn't failure, because you're fluent in the other tool. Add read-only note columns for the other tracks, aligned by step, so you aren't working blind. If the loop can't be recreated without tables, tell me and a simple table comes before G1.

### [Issue A2] [`zoom-out`, `readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 90-93](docs/designs/vimtracker.md#L90-L93), [Lines 375-400](docs/designs/vimtracker.md#L375-L400), [Lines 939-947](docs/designs/vimtracker.md#L939-L947) (generator links)
*   **Discrepancy Description:** Stage 8 says to delete links before G1. Stage 9 finds a rerender deletes its own link, and `gv` fails after any other selection.
*   **Rationale:** Links add ownership rules, persistence and a re-entry path that CUJ 2 doesn't need.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** You asked for reshaping later, and you chose to keep links at the Zoom Out check. Reviewers couldn't see that decision. Either answer works.
*   ***Proposed Edit:***
    *   Location: G11, CUJ 8, Generators, Issues 14-15
    *   Change, **if kept:** a rerender is exempt from link removal. Ownership is per column (`CUT` slot, for example), so note edits don't break it. Linked columns get a marker. `gl` in Normal mode selects the link under the cursor and enters Shape mode. **If moved after G1:** remove G11, CUJ 8, masks and Issue 14. A committed sweep is plain values, and `gv =` redoes it from its end values.

### [Issue A3] [`zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 660-664](docs/designs/vimtracker.md#L660-L664), [Lines 708-714](docs/designs/vimtracker.md#L708-L714) (page picker)
*   **Discrepancy Description:** At G1 the picker fuzzy-matches about four page names.
*   **Rationale:** `gt`, `3gt` and `gd` already reach everything.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** You asked for this specifically, and it's a small feature you may want from day one. It's your call whether it's worth doing before G1.
*   ***Proposed Edit:***
    *   Location: Keys table, Page picker, Issue 11
    *   Change, **if kept:** it becomes its own issue, tested with an injected list. Fix the `sg` test. **If deferred:** it moves after G1, and `Space ff`, `Space Space` and `nucleo-matcher` go with it.

### [Issue A4] [`zoom-out`, `critic`, `readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 73-77](docs/designs/vimtracker.md#L73-L77), [Lines 283-286](docs/designs/vimtracker.md#L283-L286), [Lines 890-893](docs/designs/vimtracker.md#L890-L893) (track count)
*   **Discrepancy Description:** Growing tracks live can lose state when a version is skipped, and deleting a middle track misaligns everything.
*   **Rationale:** Live growth needs a state-transfer protocol through the real-time path, and stable IDs on top.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** You asked not to design into a corner. A raisable constant may meet that at much lower cost, but it isn't "no limit", so only you can say whether it's enough.
*   ***Proposed Edit:***
    *   Location: G6, Passing edits, Issue 6, `:track`
    *   Change, **if constant:** the engine preallocates state for `MAX_TRACKS = 64`, and raising it is a one-line change. Remove the transfer. `:track del` removes only the last track. G6 is reworded honestly. Song data stays a growable list, so raising the limit never changes the file format. **If live growth:** stable track IDs that are never reused, state keyed to them, and the newest version always carries any pending state.

### [Issue A5] [`zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 916-924](docs/designs/vimtracker.md#L916-L924) (song view before G1)
*   **Discrepancy Description:** The song view and section navigation aren't needed for a single-section loop.
*   **Rationale:** Deferring them shrinks Issue 11.
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** This depends on the loop you pick in A1.
*   ***Proposed Edit:***
    *   Location: Issue 11, Keys table
    *   Change: If the loop is one section, defer the song view and `]r [r`, and the model keeps one section. If it has several, keep the view.

---

## Part B: Blockers and build faults (PROPOSE FIX)

### [Issue 1] [`readiness`, `code-details`] - Severity: High

*   **Affected Scope:** [Lines 739-740](docs/designs/vimtracker.md#L739-L740), [Lines 896-903](docs/designs/vimtracker.md#L896-L903) (authoring)
*   **Discrepancy Description:** Nothing creates song structure, and `:new`'s result is undefined.
*   **Rationale:** You couldn't make a song without inventing the editing model.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** This blocks CUJ 1 and G1.
*   ***Proposed Edit:***
    *   Location: Commands, Issues 7 and 11
    *   Change: `:new` creates one section, one chain and 16-step phrase per track, and instrument `00` with no sample. Typing an unused ID in a phrase, chain or instrument field creates it. `gd` on an empty reference creates the next unused ID, assigns it and opens it. Last-used instrument starts at `00`. A note with `--` uses the last-used instrument, and the instrument view sets the current one. Running with no argument opens an unsaved project, and `:w path` names it. No item deletion before G1.

### [Issue 2] [`readiness`, `code-details`, `style`] - Severity: High

*   **Affected Scope:** [Lines 933-938](docs/designs/vimtracker.md#L933-L938), [Lines 384-388](docs/designs/vimtracker.md#L384-L388) (anchors)
*   **Discrepancy Description:** Anchors are undefined and contradict `t` running across the selection.
*   **Rationale:** G5 is a core differentiator.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Your request was to select a range and interpolate, which needs only two end values. Anchors are my own extension and can come later.
*   ***Proposed Edit:***
    *   Location: Curves, Issue 13, Future Improvements
    *   Change: Remove anchors. A sweep covers one column, its first and last fields must have values, and interior fields are overwritten. FX columns must share one command at both ends. A one-step, multi-column or mixed-command selection is refused with a message. Start at bias and gain 0.5. State the formulas: `bias(t,a)=t/((1/a−2)(1−t)+1)`, the standard gain, and round half up. "Onset" is bias, and "S-shape" is gain. Move keyframes to Future Improvements.

### [Issue 3] [`readiness`, `code-details`] - Severity: High

*   **Affected Scope:** [Lines 327-330](docs/designs/vimtracker.md#L327-L330), [Lines 960-965](docs/designs/vimtracker.md#L960-L965) (table columns)
*   **Discrepancy Description:** Transpose and volume columns have playheads but no place for `HOP` or `TIC`.
*   **Rationale:** CUJ 3's arpeggio doesn't loop as written.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Tables are after G1, so the detail can wait, but the bug shouldn't stay written down.
*   ***Proposed Edit:***
    *   Location: FX interpreter and tables, Issues 18-20
    *   Change: Replace the post-G1 mechanics with short candidates plus known issues: the column model, end-of-table behaviour and the note-held test stay open for a design pass after G1. Keep three constraints: a tick-based loop, an interpreter with no phrase-only assumptions, and `apply` as the only edit path. Your drifting-cycles idea stays recorded as a requirement of that pass.

### [Issue 4] [`readiness`, `code-details`] - Severity: High

*   **Affected Scope:** [Lines 315-318](docs/designs/vimtracker.md#L315-L318), [Lines 969-971](docs/designs/vimtracker.md#L969-L971) (mid-song)
*   **Discrepancy Description:** Sample equality can't be met by counting rows.
*   **Rationale:** Held FX, RNG, voices and tails aren't reconstructed.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Every reviewer agrees. You chose Reset and Carry as behaviour, and neither needs this guarantee.
*   ***Proposed Edit:***
    *   Location: CUJ 7, Song rows and looping, Issue 20
    *   Change: Keep your Reset and Carry decision in the post-G1 design notes. Narrow the promise to track positions. Before G1, state that playing from the cursor doesn't chase held values or tails.

### [Issue 5] [`readiness`, `code-details`] - Severity: High

*   **Affected Scope:** [Line 607](docs/designs/vimtracker.md#L607), [Lines 876-880](docs/designs/vimtracker.md#L876-L880) (doesn't compile)
*   **Discrepancy Description:** `Arc<T>` needs serde's `rc` feature, and Issue 4 tests a type from Issue 14.
*   **Rationale:** The plan fails as written.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** My build test never serialized an `Arc`, so this was missed.
*   ***Proposed Edit:***
    *   Location: Dependency table, Issues 3-4
    *   Change: Add `rc` to serde. Note that `rc` writes each `Arc` in full and reloads it as separate values, which is safe because the song stores IDs rather than shared pointers. Rerun the build test with `Arc` and record the result. Move the generator link test to the link issue.

---

## Part C: Major findings (PROPOSE FIX)

### [Issue 6] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 610](docs/designs/vimtracker.md#L610), [Line 830](docs/designs/vimtracker.md#L830)
*   **Discrepancy Description:** `hound` is listed for `vt-model`, but `render.rs` writes WAV.
*   **Rationale:** The crate graph is wrong.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple correction.
*   ***Proposed Edit:***
    *   Location: Dependency table
    *   Change: `hound` for both crates. Renders are 32-bit float stereo.

### [Issue 7] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 850-862](docs/designs/vimtracker.md#L850-L862)
*   **Discrepancy Description:** `vt-tui` has no test location, and Issue 1 needs a stub `main.rs`.
*   **Rationale:** Key tests have nowhere to go.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple correction.
*   ***Proposed Edit:***
    *   Location: `vt-tui` file list, Issue 1
    *   Change: Add `src/lib.rs` and `tests/`, with `main.rs` as a thin wrapper. Issue 1 creates stubs.

### [Issue 8] [`code-details`, `readiness`] - Severity: Medium

*   **Affected Scope:** [Line 825](docs/designs/vimtracker.md#L825)
*   **Discrepancy Description:** No resampler is chosen.
*   **Rationale:** It changes sound and hashes.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** You've already chosen one in `gmuse`.
*   ***Proposed Edit:***
    *   Location: `sample.rs`, dependency table
    *   Change: Use `rubato` 5.0.0 at load. Accept 16-bit, 24-bit and float WAV, mono or stereo, and refuse others with a message.

### [Issue 9] [`code-details`, `readiness`] - Severity: Medium

*   **Affected Scope:** [Line 845](docs/designs/vimtracker.md#L845), [Line 951](docs/designs/vimtracker.md#L951)
*   **Discrepancy Description:** Golden hashes have no algorithm.
*   **Rationale:** `std` hashers aren't stable across releases.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Needed for G9.
*   ***Proposed Edit:***
    *   Location: Testing, Issue 16
    *   Change: Hash the little-endian bytes of samples with FNV-1a 64, written in test support with no dependency. Each golden file holds one hex value.

### [Issue 10] [`code-details`] - Severity: Medium

*   **Affected Scope:** [Line 749](docs/designs/vimtracker.md#L749)
*   **Discrepancy Description:** Argument parsing and errors are undecided.
*   **Rationale:** An implementer would choose.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** `gmuse` has already settled this.
*   ***Proposed Edit:***
    *   Location: Dependency table
    *   Change: Add `clap` and `thiserror`, at the versions `gmuse` locks.

### [Issue 11] [`code-details`, `readiness`] - Severity: Medium

*   **Affected Scope:** [Line 653](docs/designs/vimtracker.md#L653), [Line 881](docs/designs/vimtracker.md#L881)
*   **Discrepancy Description:** A schema change gives a field error before the version is read.
*   **Rationale:** This defeats the purpose of the version number.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Needed for your no-migration choice to fail clearly.
*   ***Proposed Edit:***
    *   Location: Saving and loading, Issue 4
    *   Change: Load in two steps: read only `format_version`, compare, then parse `Song`. Test with a changed field, not just a changed number.

### [Issue 12] [`code-details`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Line 846](docs/designs/vimtracker.md#L846), [Line 894](docs/designs/vimtracker.md#L894)
*   **Discrepancy Description:** A process-wide allocator counter sees other test threads.
*   **Rationale:** The test would give false failures.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** You've authorized reuse and already solved this.
*   ***Proposed Edit:***
    *   Location: Target Files, Issue 6
    *   Change: Copy `gmuse`'s `rt_guard.rs`. Also mark the other copies in Target Files: `src/cmd/keys.rs`, `which_key_due`, `audio/device.rs`, and `rank` if the picker stays.

### [Issue 13] [`code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 789-802](docs/designs/vimtracker.md#L789-L802)
*   **Discrepancy Description:** Existing files are described as new.
*   **Rationale:** The repo already has them.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple correction.
*   ***Proposed Edit:***
    *   Location: Target Files
    *   Change: Mark files Create or Modify. `README.md` is Modify.

### [Issue 14] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 743-745](docs/designs/vimtracker.md#L743-L745)
*   **Discrepancy Description:** `:bpm`, `:len`, `:set` and value entry are in no issue.
*   **Rationale:** G1 needs tempo and length.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Scheduling gap.
*   ***Proposed Edit:***
    *   Location: Issues 7 and 10
    *   Change: Issue 7 gets `:bpm` and `:len`, and Issue 10 gets value entry and `:set step`. `:set scale` moves with scales to after G1. `:len` shrinking keeps hidden steps until saved, so undo restores them.

### [Issue 15] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Line 898](docs/designs/vimtracker.md#L898), [Line 907](docs/designs/vimtracker.md#L907), [Line 920](docs/designs/vimtracker.md#L920)
*   **Discrepancy Description:** Issue 7 needs leader keys from Issue 11, and edits notes before `apply` exists.
*   **Rationale:** The order doesn't work.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Scheduling gap.
*   ***Proposed Edit:***
    *   Location: M1 and M2
    *   Change: Move `action.rs` and history before Issue 7. Issue 7 adds the leader and binding table. Add `input/bindings.rs` and `views/help.rs`, and give the popup to Issue 9.

### [Issue 16] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 1002-1003](docs/designs/vimtracker.md#L1002-L1003)
*   **Discrepancy Description:** The G8 run has no owner.
*   **Rationale:** The gate is ambiguous.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Scheduling gap.
*   ***Proposed Edit:***
    *   Location: Issue 17
    *   Change: Issue 17 runs G8 and records track capacity. "Late" means callback time over buffer duration. CPU load is callback time divided by buffer duration. G8 becomes a zero count, and `cpal` stream errors are also counted.

### [Issue 17] [`style`] - Severity: Medium

*   **Affected Scope:** [Lines 594-595](docs/designs/vimtracker.md#L594-L595), [Line 958](docs/designs/vimtracker.md#L958)
*   **Discrepancy Description:** "Only if G1 passes" doesn't match Issue 17's outcomes.
*   **Rationale:** The gate wording is unclear.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Follows A1.
*   ***Proposed Edit:***
    *   Location: Plan intro, milestone headings
    *   Change: Use "only if G1's outcome is continue".

### [Issue 18] [`readiness`, `style`, `code-details`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 916-924](docs/designs/vimtracker.md#L916-L924)
*   **Discrepancy Description:** Issue 11 is too big, and CUJ 5's folding depends on Issue 21.
*   **Rationale:** It contradicts "one pull request".
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Scheduling fault.
*   ***Proposed Edit:***
    *   Location: Issue 11, CUJ 5
    *   Change: Split into chain and tree navigation, then track commands, plus instrument view. CUJ 5 before G1 scrolls sideways, and folding stays later.

### [Issue 19] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 810-846](docs/designs/vimtracker.md#L810-L846), [Lines 247-257](docs/designs/vimtracker.md#L247-L257)
*   **Discrepancy Description:** Core types are named, not defined.
*   **Rationale:** Field names are the file format.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Only the data the first version saves needs fixing, because the format can change freely.
*   ***Proposed Edit:***
    *   Location: Song model
    *   Change: Add `format_version`, `bpm`, `seed` and `step`. List fields for `Track`, `Section`, `Chain`, `Phrase`, `Instrument`. Instrument holds sample path, volume, attack and release. Notes run `C-0` (MIDI 12) to `G-9` (MIDI 127), starting octave 4. Default step 1. Add `src/samples.rs` for the pool. Rust API shapes are left to the implementer under the fallback rule in Issue 46.

### [Issue 20] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 887](docs/designs/vimtracker.md#L887)
*   **Discrepancy Description:** At 44100 Hz a row is 5512.5 samples, with no rounding rule.
*   **Rationale:** The test's expected value is a guess.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** This is one of the expensive-to-change choices.
*   ***Proposed Edit:***
    *   Location: Time, Issue 5
    *   Change: Event sample = `floor(tick × 60 × rate / (960 × bpm))`, in integers, from the tempo's start tick, never accumulated. BPM is an integer from 20 to 400. The delay buffer is sized for 3/16 at 20 BPM.

### [Issue 21] [`readiness`, `critic`] - Severity: Medium

*   **Affected Scope:** [Lines 966-968](docs/designs/vimtracker.md#L966-L968)
*   **Discrepancy Description:** Groove units at 3/2 speed aren't whole ticks.
*   **Rationale:** No rounding rule exists.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Grooves are after G1, so the claim should be limited.
*   ***Proposed Edit:***
    *   Location: Time, post-G1 notes
    *   Change: Say whole ticks holds only for straight rows, and list groove-times-speed as an open item for after G1.

### [Issue 22] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 762-764](docs/designs/vimtracker.md#L762-L764)
*   **Discrepancy Description:** `DEL`, `KIL` and `OFF` at `FF` reach only about one row.
*   **Rationale:** A kill longer than a row is impossible.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Matches the M8 and `RET`.
*   ***Proposed Edit:***
    *   Location: FX table
    *   Change: All three count in groove units of 40 ticks. A value over the row's end keeps running across the next row.

### [Issue 23] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 754-775](docs/designs/vimtracker.md#L754-L775)
*   **Discrepancy Description:** Starting values, resets and volume combination are missing.
*   **Rationale:** Needed for any sound.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Only for commands that survive.
*   ***Proposed Edit:***
    *   Location: Audio path, FX table
    *   Change: Per-track starts: `VOL FF`, `PAN 80`, `CUT FF`, `RES 00`, sends `00`. All reset on play from the start. Final gain is cell × `VOL` × instrument, and `--` means `FF`. Samples are one-shots. Transposed notes clamp to the note range.

### [Issue 24] [`readiness`, `code-details`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 890-893](docs/designs/vimtracker.md#L890-L893), [Lines 277-278](docs/designs/vimtracker.md#L277-L278)
*   **Discrepancy Description:** Queue depth and return rules are incomplete.
*   **Rationale:** Skipped versions need returning.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simplification, whichever way A4 goes.
*   ***Proposed Edit:***
    *   Location: Passing edits
    *   Change: Publish queue depth is 1, and the edit thread keeps only the newest pending version. The callback takes a version only when the return queue has room. Update the diagram with the command queue.

### [Issue 25] [`readiness`, `critic`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 885-886](docs/designs/vimtracker.md#L885-L886), [Lines 280-281](docs/designs/vimtracker.md#L280-L281)
*   **Discrepancy Description:** A voice can jump into a replaced sample.
*   **Rationale:** Out-of-bounds risk in a callback that can't panic.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** One rule closes it.
*   ***Proposed Edit:***
    *   Location: Audio path, Issue 5
    *   Change: The sample pool only grows during a session and sample IDs are stable, so a voice keeps its own sample until it ends. Replacing a sample affects new notes only. The cost is memory held until the project reloads, which is accepted. One top-level `Arc` wraps the song and pool.

### [Issue 26] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 756-761](docs/designs/vimtracker.md#L756-L761)
*   **Discrepancy Description:** Two-digit values contradict "no fixed limits".
*   **Rationale:** The goal is false as written.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** These commands are after G1, so state the limit and revisit it.
*   ***Proposed Edit:***
    *   Location: G6, FX table
    *   Change: G6 covers tracks and item storage, not FX addressing. List FX addressing as an open item for the tables design.

### [Issue 27] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Line 643](docs/designs/vimtracker.md#L643), [Lines 765-769](docs/designs/vimtracker.md#L765-L769)
*   **Discrepancy Description:** `RET y=0` is a zero interval, and other FX semantics are incomplete.
*   **Rationale:** The example could hang or panic the callback.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** My own row example triggers this.
*   ***Proposed Edit:***
    *   Location: FX table, row format
    *   Change: Pre-G1 commands are `VOL PAN CUT RES DLY REV DEL KIL OFF RET`, adjusted to the loop in A1. `RET xy` with `y=0` does nothing, `x` is 0-7 quieter or 8-F louder by 10% steps, and the example becomes `RET14`. `CHA`, `RND`, `REP`, `IFC`, `HOP`, `THO`, `TIC`, `TBL`, `TBX` and `GRV` move after G1, and `rng.rs` goes with them.

### [Issue 28] [`code-details`, `critic`] - Severity: Medium

*   **Affected Scope:** [Lines 335-336](docs/designs/vimtracker.md#L335-L336)
*   **Discrepancy Description:** "Note held" has no command, and the M8 comparison is inferred.
*   **Rationale:** An advertised feature doesn't exist.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Follows Issue 3.
*   ***Proposed Edit:***
    *   Location: Tables section
    *   Change: Move to the post-G1 design notes, worded "the M8 manual documents no such test".

### [Issue 29] [`readiness`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 665-666](docs/designs/vimtracker.md#L665-L666), [Lines 700-703](docs/designs/vimtracker.md#L700-L703)
*   **Discrepancy Description:** There's no cursor model, and "cell" has two meanings.
*   **Rationale:** `x` could equal `dd`.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Needed before CUJ 2.
*   ***Proposed Edit:***
    *   Location: Keys table, Glossary
    *   Change: A **step** has **fields**: note, volume, instrument, and for each FX slot a command and a value. `h l` moves by field, and `w b` by column group (note, volume, instrument, slot). Value entry takes two hex digits, then commits and moves down by the step setting. Instrument IDs accept more digits, and `Enter` commits them. FX names are typed in uppercase, and an invalid name is refused with a message.

### [Issue 30] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 675-681](docs/designs/vimtracker.md#L675-L681), [Line 361](docs/designs/vimtracker.md#L361)
*   **Discrepancy Description:** Grammar semantics are undefined.
*   **Rationale:** Issue 9's tests can't be written, and this is the first differentiator.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** This is the most important addition. It follows Vim wherever Vim has an answer.
*   ***Proposed Edit:***
    *   Location: Editing, Keys table, new CUJ
    *   Change: Operators are `d y > <`. Motions and `ic` act on the current column. `dd`, `yy`, `>>`, `<<`, `ib` and `ip` act on whole steps. A count extends the range: `d4j` is five steps, and `3ib` is three beats. `+` and `-` take a count as an amount, on the cursor or selection. `>` and `<` move notes by a semitone and other values by 1, and they skip empty and `OFF`, FX names and instrument IDs. `d` and `y` also fill the unnamed register. `p` overwrites from the cursor, clips at the phrase end, and refuses a mismatched column type. Motions clamp at edges. `.` repeats operators, value changes and shapes, in Normal or Visual. Insert sessions aren't repeated. Remove `it` until a track scope exists. Add a CUJ for operator, target, count and repeat. Chain view gets motions, entry, `d`, `y`, `p`, `gd` and `gc`.

### [Issue 31] [`readiness`, `code-details`, `style`] - Severity: Medium

*   **Affected Scope:** [Lines 658-706](docs/designs/vimtracker.md#L658-L706)
*   **Discrepancy Description:** Command mode, Visual `Esc` and `z` keys are missing.
*   **Rationale:** The table is meant to be authoritative.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple addition.
*   ***Proposed Edit:***
    *   Location: Keys table
    *   Change: Add them. `Esc` after the leader cancels silently. `grr` opens a list with `j k`, `Enter` and `Esc`.

### [Issue 32] [`readiness`, `code-details`, `style`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 686-688](docs/designs/vimtracker.md#L686-L688), [Lines 900-901](docs/designs/vimtracker.md#L900-L901)
*   **Discrepancy Description:** Play scope is underspecified, and my `Space x` wording contradicts itself.
*   **Rationale:** Writing a multi-track loop needs a defined scope.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** One scope before G1 removes most of the questions. It also follows `gmuse`'s D-9, D-10 and D-14.
*   ***Proposed Edit:***
    *   Location: Keys table, new Transport section
    *   Change: Before G1 there's one scope: the current section, on all tracks, looping. `Enter` plays from the cursor's step and restarts if already playing. `Space x` pauses in place and resumes. `Space c` returns to the start and stops if playing. Commands: `PlayFrom`, `Toggle`, `Return`, resolved by the engine. The test sends two `Toggle`s and checks the original state returns.

### [Issue 33] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 732](docs/designs/vimtracker.md#L732)
*   **Discrepancy Description:** Note preview is undefined.
*   **Rationale:** One voice per track.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Needed for note entry.
*   ***Proposed Edit:***
    *   Location: Fixed Specifications
    *   Change: A `Preview` command plays on the focused track's voice with no FX. While that track is playing, the next note replaces it.

### [Issue 34] [`readiness`, `code-details`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Line 684](docs/designs/vimtracker.md#L684), [Line 921](docs/designs/vimtracker.md#L921)
*   **Discrepancy Description:** Clone repointing is undefined.
*   **Rationale:** CUJ 4 depends on it.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** One rule is enough.
*   ***Proposed Edit:***
    *   Location: Moving through the tree
    *   Change: `gc` creates the next unused ID and repoints the reference you came from through `gd`. If there's no such reference, it opens the clone and leaves references alone. A shared parent chain is edited as is, and a status message says so. `gC` also clones the chain's phrases and repoints the section's reference.

### [Issue 35] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 683](docs/designs/vimtracker.md#L683), [Lines 869-874](docs/designs/vimtracker.md#L869-L874)
*   **Discrepancy Description:** `Ctrl-I` arrives as Tab, and `probe-keys` has no decision rule.
*   **Rationale:** The keymap includes an unreachable binding.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** `gmuse` has this answer.
*   ***Proposed Edit:***
    *   Location: Keys table, Issue 2
    *   Change: Bind jump-forward to `Tab`, which is what Vim's `Ctrl-I` sends anyway. Push no keyboard flags before G1, as nothing needs release events. Ignore `Repeat` on transport keys. A key that doesn't arrive gets a replacement chosen in Issue 2 and recorded. Also record Esc latency.

### [Issue 36] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 941](docs/designs/vimtracker.md#L941)
*   **Discrepancy Description:** A rerender deletes its own link.
*   **Rationale:** The rule is self-defeating.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Covered by A2 if links stay, and disappears if they don't.
*   ***Proposed Edit:***
    *   Location: Issue 14
    *   Change: See A2.

### [Issue 37] [`readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 745-749](docs/designs/vimtracker.md#L745-L749), [Lines 780-787](docs/designs/vimtracker.md#L780-L787)
*   **Discrepancy Description:** Render and audio gaps.
*   **Rationale:** G9 needs them, and a Chamberlin filter is unstable near 20 kHz at 44.1 kHz.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Fix what affects behaviour, and leave tuning to the ear.
*   ***Proposed Edit:***
    *   Location: Audio defaults, render
    *   Change: `render` plays one pass of the song with no tail, unless `--rows` is given, overwriting the output file. Use a TPT state-variable filter. A device with fewer than two channels, or lost mid-session, exits with a message. Never print from the callback. Reverb and delay tuning values are picked at implementation and pinned by golden hashes.

### [Issue 38] [`readiness`, `comprehension`] - Severity: Medium

*   **Affected Scope:** [Lines 738-745](docs/designs/vimtracker.md#L738-L745)
*   **Discrepancy Description:** Quit, undo and error behaviour.
*   **Rationale:** Unspecified.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Follow Vim.
*   ***Proposed Edit:***
    *   Location: Commands
    *   Change: `:q`, `:e` and `Space qq` refuse with unsaved changes, as in Vim. `:w path` copies samples. An Insert session is one undo step. Mute and solo are undoable edits. Errors go to the status line.

### [Issue 39] [`style`, `structural`, `critic`] - Severity: Medium

*   **Affected Scope:** [Lines 661-662](docs/designs/vimtracker.md#L661-L662), [Lines 187-192](docs/designs/vimtracker.md#L187-L192)
*   **Discrepancy Description:** "Row", "step", "cell", "page", "view", "gain" and "action" collide.
*   **Rationale:** Reset and Carry depend on these distinctions.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Do this last, on surviving text.
*   ***Proposed Edit:***
    *   Location: New Glossary, whole document
    *   Change: About 10 lines defining track, section (song row), chain, phrase, step, field, table, view, operator and action. "Curve bias" and "curve gain". Use "view" throughout.

### [Issue 40] [`style`, `readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Lines 721-723](docs/designs/vimtracker.md#L721-L723)
*   **Discrepancy Description:** The stated key rule conflicts with `gT` and `H`/`L`.
*   **Rationale:** Your guide names direction-by-case as wrong.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Your own guide already makes this exception for `vN`.
*   ***Proposed Edit:***
    *   Location: Key conventions
    *   Change: "Keys inherited from Vim keep Vim's meaning. All others follow the author's Neovim rules." Cite the guide, and soften "no key is both" to the guide's actual test: what happens when the timeout wins.

### [Issue 41] [`style`, `readiness`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 350](docs/designs/vimtracker.md#L350), [Line 783](docs/designs/vimtracker.md#L783)
*   **Discrepancy Description:** "Limiter" vs `tanh` clip.
*   **Rationale:** They differ.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple correction.
*   ***Proposed Edit:***
    *   Location: Audio path
    *   Change: "`tanh` soft clip" in both places.

### [Issue 42] [`style`] - Severity: Medium

*   **Affected Scope:** [Lines 616-637](docs/designs/vimtracker.md#L616-L637)
*   **Discrepancy Description:** Constants lack meaning and units.
*   **Rationale:** Reader can't tell what they are.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Small and useful.
*   ***Proposed Edit:***
    *   Location: Constants table
    *   Change: Add a Meaning column. Keep only constants the design relies on. `DECLICK_FRAMES` and other tuning values may change without a document update.

### [Issue 44] [`style`, `code-details`] - Severity: Medium

*   **Affected Scope:** [Line 411](docs/designs/vimtracker.md#L411), [Line 749](docs/designs/vimtracker.md#L749)
*   **Discrepancy Description:** The subcommand list is wrong.
*   **Rationale:** `export-json` has two signatures.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** JSON export is still after G1, so it's one consistent line.
*   ***Proposed Edit:***
    *   Location: Commands
    *   Change: One signature, `vimtracker export-json <project> [out.json]`, writing to stdout by default. Keep it after G1, and drop `serde_json` from the pre-G1 table.

### [Issue 45] [`style`] - Severity: Medium

*   **Affected Scope:** [Line 909](docs/designs/vimtracker.md#L909), [Line 980](docs/designs/vimtracker.md#L980), [Lines 999-1000](docs/designs/vimtracker.md#L999-L1000)
*   **Discrepancy Description:** Overstated or unmeasurable wording.
*   **Rationale:** These aren't checkable.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Small.
*   ***Proposed Edit:***
    *   Location: Those lines
    *   Change: "History drops the oldest version past the limit." Remove "readable" with M5. "Every behaviour issue lists its tests." Remove "slices" or add to Future Improvements.

---

## Part D: Findings from earlier stages that affect the build

### [Issue 46] [`zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 958-991](docs/designs/vimtracker.md#L958-L991) (post-G1 and fallback)
*   **Discrepancy Description:** M4-M6 are over-specified, while G1 may redirect them. Reviewers also want many small behaviours fixed.
*   **Rationale:** Specifying everything adds text the first test never runs.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** This is what stops the plan growing past what a prototype needs. Your ideas stay recorded.
*   ***Proposed Edit:***
    *   Location: Plan, new "After G1" section
    *   Change: Replace M4-M6 with candidates in your priority order: tables, then score view, then Reset/Carry and track speed, generators, waveform. Keep your design intent in each. Add one rule: "If a behaviour isn't specified and doesn't affect what G1 tests, follow Vim, or choose the simplest option and record it." Set the readiness bar at M1-M3 only.

### [Issue 47] [`critic`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 125-126](docs/designs/vimtracker.md#L125-L126), [Lines 351-352](docs/designs/vimtracker.md#L351-L352) (smoothing)
*   **Discrepancy Description:** A 10 ms smoother holds for about 115 ms per row at 120 BPM.
*   **Rationale:** Sweeps may sound stepped, and could fail G1 for the wrong reason.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Nobody has heard it, so test before building more.
*   ***Proposed Edit:***
    *   Location: Audio path, Issue 12
    *   Change: Change the claim to a hypothesis. End Issue 12 with an ear test on a sustained sample. If stepped, smooth over the row length with no look-ahead, and add a note that this is the fallback.

### [Issue 48] [`critic`, `zoom-out`] - Severity: Medium

*   **Affected Scope:** [Lines 34-37](docs/designs/vimtracker.md#L34-L37), [Line 47](docs/designs/vimtracker.md#L47) (differentiator)
*   **Discrepancy Description:** Live shaping may already exist in automation envelopes.
*   **Rationale:** From general knowledge, unverified, but it undermines the claim.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The narrower claim is what we're actually building, whether or not the counter-example holds.
*   ***Proposed Edit:***
    *   Location: The Problem
    *   Change: Narrow it to "shaping by keyboard, in the grid, while the loop plays." Say automation envelopes in other tools weren't reviewed.

### [Issue 49] [`zoom-out`, `critic`, `structural`] - Severity: Medium

*   **Affected Scope:** [Lines 451-460](docs/designs/vimtracker.md#L451-L460) (terminal premise)
*   **Discrepancy Description:** Extending a tracker is rejected on an unstated premise.
*   **Rationale:** Terminal isn't a stated differentiator.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** You asked for "as console as possible" from your first message, so it's a real premise that just isn't written down. New evaluations of GUI and grammar alternatives aren't needed, since G1 tests them in practice.
*   ***Proposed Edit:***
    *   Location: The Problem, Alternatives
    *   Change: State terminal-first as your premise, and say "cheapest given that premise". The rejection then follows from it. No new alternatives sections.

### [Issue 50] [`style`, `critic`] - Severity: Medium

*   **Affected Scope:** [Lines 445-446](docs/designs/vimtracker.md#L445-L446), [Lines 798-799](docs/designs/vimtracker.md#L798-L799) (facts)
*   **Discrepancy Description:** "Its synth engines are closed binaries" is too broad, and "GPL-3.0" doesn't match `gmuse`'s `GPL-3.0-or-later`.
*   **Rationale:** Both are inaccurate.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Simple corrections. The license file itself is the same text.
*   ***Proposed Edit:***
    *   Location: Those lines, crate manifests
    *   Change: "Some of its synth and DSP code ships only as closed binaries." Use `GPL-3.0-or-later` in the design and crate `license` fields.

### [Issue 51] [`structural`, `critic`] - Severity: Medium

*   **Affected Scope:** [Lines 215-229](docs/designs/vimtracker.md#L215-L229), [Line 111](docs/designs/vimtracker.md#L111) (risks and limits)
*   **Discrepancy Description:** Risks omit device loss, missing samples, memory and crash loss.
*   **Rationale:** The standard asks for limits.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Accept these as prototype limits, in one sentence each, rather than building for them.
*   ***Proposed Edit:***
    *   Location: Risks
    *   Change: Add: device loss exits the program. A missing sample is an error naming the path. Memory isn't bounded beyond track count and sample size. No autosave, so a crash loses unsaved work.

---

## Part E: Contested, tuning and nits

### [Issue 52] [`critic`] - Severity: Medium

*   **Affected Scope:** [Lines 438-545](docs/designs/vimtracker.md#L438-L545) (more alternatives)
*   **Discrepancy Description:** Reviewers want evaluations of terminal vs GUI, grammar vs selection-first, and bias/gain vs breakpoint envelopes.
*   **Rationale:** The standard's Alternatives Rule.
*   ***AI Suggestion:*** CONTEST
*   ***Justification:*** The Stage 8 report (`intermediate_zoom_out_report.md`, Z8) rejects this on checkable grounds: G1 tests the first two in practice, and the premise is stated by Issue 49. Comparing them on paper adds length without improving the answer. The curve model can change later with no compatibility cost (L100-102).
*   ***Proposed Edit:***
    *   Location: None
    *   Change: None.

### [Issue 53] [`comprehension`, `readiness`] - Severity: Medium

*   **Affected Scope:** [Lines 780-783](docs/designs/vimtracker.md#L780-L783) (exact tuning values)
*   **Discrepancy Description:** Pan law, envelope and smoother shapes, resonance range, wet levels and reverb tunings are unspecified.
*   **Rationale:** A re-implementation wouldn't sound identical.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The finding is valid against the standard, but the fix is to state the rule rather than the values, because these get tuned by ear. Equal-power pan is the one behaviour worth fixing now.
*   ***Proposed Edit:***
    *   Location: Audio defaults
    *   Change: Equal-power pan. Everything else is chosen at implementation and pinned by golden hashes.

### Nits [`all`] - Severity: Low

*   **Affected Scope:** Nit sections of `implementation_readiness_report.md`, `critic_report.md`, `comprehension_report.md`, `intermediate_zoom_out_report.md`
*   ***AI Suggestion:*** DECLINE for four groups: layout and wrap drift, vague-word lists, repetition across sections, and heading format. These can wait for an editorial pass.
*   ***Applied through other fixes:*** curve formulas (Issue 2), `nucleo-matcher` pin at 0.3.1 and the `sg` test (A3), scale rules moved with scales (Issue 14), `gmuse` reuse (Issue 12), Esc latency (Issue 35).
