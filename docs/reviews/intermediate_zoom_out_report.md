# Stage 8: Intermediate Zoom Out

**Target:** `docs/designs/vimtracker.md`, full file

**Verdict: does not pass.** 1 Blocker, 7 Majors, 5 Nits. The context check passes. The fix is
mostly subtraction. About half of the Stage 6 and 7 additions disappear if the pre-G1 scope is
cut first. Applied as written, those reports would add specification to code the first test
never runs.

**Main judgement:** L18-19 asks to avoid decisions that are expensive to undo. L100-102 removes
file compatibility, so most model, FX and format choices are now cheap to undo. These are the
expensive choices, and the design should keep them: snapshot publishing, `apply` as the only
edit path, sequencing in the callback, the tick clock, and no dependence on key release.

## Blocker

### Z1. The gate is still undecided, and building tables is the wrong way to settle it

*   **Scope:** L42-51, L55-58, L368-371, L594, L730, L953-956. Stage 5 #1, Critic 1, Comprehension M14.
*   **Rationale:** Building a minimal table before G1 brings in Comprehension B5, Critic 6, 7 and 11
    and the `HOP`/`TIC` work, the least settled mechanics in the document. Reject it.
*   **Smallest fix:**
    *   Rewrite L42-51 so G1 decides the grammar, live sweeps and shared phrases. Tables get a later gate.
    *   Add a minimal multi-track context: the focused track is editable, and other tracks show read-only
        note columns aligned by absolute row. A full phrase row is 27 characters, so 8 full tracks need
        about 224 columns, while note-only neighbours fit in 120. This needs no cross-phrase selection,
        folding or M5 score view.
    *   Remove `it` until a real track scope exists, and change the L45 example.
    *   Move `W B` from Issue 11 to Issue 9, because CUJ 1 needs it earlier.
*   **Condition:** if no suitable existing loop works without tables, an M8-style single-playhead table
    goes before G1. Z2 tests this first.

## Major

### Z2. Choose the tool and loop first, and keep the protocol small

*   **Scope:** L51, L55-58, L953-956. Stage 5 #2, Critic 1.
*   **Keep:** name the tool and loop, write the pass rule before M3, reconcile L51 with L58, and add an
    "inconclusive" outcome.
*   **Simplify:** no repeated trials or new composition. Run vimtracker first, so repetition does not favour
    it. Fluency still favours the usual tool, so a slower time alone should not mean failure. The author
    sets the thresholds.
*   **Add:** Issue 17 records which deferred feature was missed, replacing the single-track note.

### Z3. Apply the re-implementation bar only to M1-M3

*   **Scope:** L305-345, L756-761, L958-991. Stage 5 #8, Stage 6 verdict.
*   **Evidence:** Comprehension B5, B6 and most of M2, plus Critic 2, 6, 7, 11 and 21, are defects in post-G1
    work. Critic 2 itself "blocks G10, not M1-M3".
*   **Smallest fix:** replace M4-M6 and post-G1 mechanics with a short candidate list. Keep three structural
    constraints: a tick-based block loop, an FX interpreter with no phrase-only assumptions, and `apply` as the
    single edit path.

### Z4. Delete generator links, and do not repair them

*   **Scope:** G11 L90-93, L105, L130-133, CUJ 8 L195-203, L375-400, L433-434, L491-496, L933-947. Stage 5 #4,
    Comprehension B1 and M13, Critic 12.
*   **Rationale:** Critic 12's motion, marker and per-field ownership add machinery to a feature G5 does not need.
    CUJ 2 passes without links. Committed sweeps are ordinary values, so `gv =` can redo a sweep from its end
    values with the last-used shape.
*   **Smallest fix:** keep `curve.rs`, Shape mode and last-used-shape `.`. Remove links, masks, value sources, the
    seed and `scale.rs` before G1. Remove anchors: a sweep is one field in one column, with non-empty first and
    last cells, overwriting interior cells. Other shapes are refused with a message. G11 stays a post-G1
    candidate: deferred, not rejected.

### Z5. Use fixed capacity instead of adding stable track IDs

*   **Scope:** G6 L73-77, L259-261, L275, L283-286, L341, L741, L890-895. Stage 5 #6, Critic 8, Comprehension M2, M6.
*   **Status:** FX addressing moves post-G1, because those commands are all in M4. The real-time half is stronger:
    newest-wins at L275 can skip the version carrying new track state.
*   **Smallest fix:** preallocate state for 64 tracks behind one constant, and remove the track-state transfer. Before
    G1, remove `:track del` or limit it to the last track, so indices stay stable. Reword G6 as a raisable constant.

### Z6. Some additions are justified and should be made in short form

| Source | Addition |
|---|---|
| Comprehension B3, Critic 18 | Grammar semantics for the phrase view: operators, motions, counts, `p`, default register, `.` on a selection. Chain view gets motions, entry, `gd` and `gc` only. **One new CUJ** for operator, target, count and repeat. |
| Comprehension B2, Critic 23 | Neutral bias and gain of 0.5, the formulas, rounding mode, and one mapping from "onset and S-shape" to bias and gain. |
| Comprehension B4 | Initial project contents. `gd` on an empty reference creates the next unused ID, adapting the M8's new-item shortcut (`m8.txt` L861-862, L946). No item deletion before G1. |
| Comprehension M10 | `gc` repoints the use you jumped from. CUJ 4 depends on it. |
| Critic 10, Comprehension M8 | One arrow and one sentence for the command queue. `Enter` plays all tracks. State whether preview interrupts a voice. Fix L687: default to `gmuse`, with `x` pausing in place and `c` returning (spec D-9, D-10, D-14). |
| Critic 9 | One rule: the session sample pool is append-only with stable sample IDs, and voices hold sample IDs. Accept the memory cost. Published state becomes one `Arc`, or reword L280-281. |
| Comprehension M1, M4, M16 | Only for surviving commands: what `xx` measures in `DEL`, `KIL` and `OFF`, when a held value resets, how volume combines, limiter vs `tanh`, samples as one-shots. |
| Comprehension M12, M15, M17, Stage 5 #7, #11 | Assign `:bpm`, `:len`, `:set step` and value entry to issues. Assign the G8 run to Issue 17. Define late as callback time exceeding buffer time. Add serde's `rc` feature. Say play-from-cursor does not chase held values. |
| Critics 15-17 | A glossary of about ten lines, written **after** the cuts, including "section" for song row. |

*   **Fallback rule:** one sentence allowing the implementer to choose behaviour that does not affect the hypothesis and
    to record it. This covers most of Comprehension M9, M11 and M18.

### Z7. The smoother risk is real before G1, but test it before building more

*   **Scope:** L125-126, L351-352, L629, L948. Critic 5.
*   **Check:** the arithmetic holds. It is audible mainly when a note sounds across rows, since a sweep over short hits
    changes at each hit anyway. Nobody has listened to it.
*   **Smallest fix:** keep the Open Question, but move the ear test from Issue 15 to the end of Issue 12, using a sustained
    sample. Decide the rule first: if stepped, ramp over the row length with no look-ahead. Do not plan look-ahead or
    tick-rate updates until that fails. Add the sustained-sample case to the G1 record.
*   **Related:** filled sweeps write every row, so persistence (L127-129) matters only for sparse values.

### Z8. Correct the justification without adding more alternatives

*   **Scope:** L17-19, L34-37, L47, L438-460, L491-496. Stage 5 #3, #17, Critic 3, 4.
*   **Smallest fix:** narrow differentiator 2 to shaping by keyboard inside the grid. State terminal-first as the author's
    premise and say "cheapest given that premise", which makes a tracker spike optional. Reject Critic 3's request for new
    terminal, grammar, curve-model and cap evaluations: G1 tests the first two, and Z5 and Z7 handle the rest. Critic 4's
    Renoise and DAW counter-example remains unverified.

### Z9. Issues 11 and 12 still carry material the loop may not need

*   **Scope:** L660-664, L708-714, L916-932. Stage 5 #5, Comprehension M6, M10.
*   **Smallest fix:**
    *   Defer the picker. Keep the binding table, which-key and `Space ?`.
    *   Defer the song view and `]r [r`. The model keeps `rows[]` with one section. If the loop has several sections, restore the view.
    *   Derive Issue 12's commands from the loop. If `CHA` and `RND` are unused, `rng.rs` and the seed work leave too.
    *   Split Issue 11 into chain and tree navigation, then track commands.

## Nit

*   **N1. Reuse is still undecided.** Confirmed present: `rt_guard.rs`, `src/cmd/keys.rs`, `rank`, `which_key_due`, `audio/device.rs`,
    `audio/resample.rs`. Transport rules exist as `audio::transported` and `audio::cued`. Mark copies per file. Don't copy `gmuse`'s
    device recovery or ring underrun counter.
*   **N2. Accept small prototype limits.** `:mute` is an undoable edit. A device loss requires a restart. No autosave. One sentence.
*   **N3. Do editorial work last.** Critics 19-21, 24-26 and the style nits apply only to surviving text.
*   **N4. Two factual fixes.** L445-446 overstates closed binaries. L798-799 should match `gmuse`'s `GPL-3.0-or-later`.
*   **N5. Add Esc latency and Ctrl-I vs Tab to `probe-keys` notes.**

## Smallest coherent set, in order

1.  **Author decides:** comparison tool, loop, pass rule (Z2).
2.  **Rewrite the claim:** L34-51 and G1 say the same thing, and the terminal premise is stated (Z1, Z8).
3.  **Cut:** links, masks, seed, scale, anchors, picker, song view, track growth, `:track del`, unused FX, M4-M6 detail (Z3, Z4, Z5, Z9).
4.  **Add:** the neighbour view and the Z6 rules, including one grammar CUJ.
5.  **Reorder:** the smoother test moves to Issue 12 (Z7).
6.  **Then:** glossary and editorial pass, and review only the changed M1-M3 scope.

## Leave undecided until after G1

*   Table column model, end-of-table behaviour, note-held tests and aux tables.
*   Grooves, track speeds and rounding.
*   Reset and Carry, and any mid-song guarantee.
*   `HOP` and ID addressing beyond two digits, jump-cycle safety, and edge cases for commands not built.
*   Generator links, ownership granularity, multi-track generators and masks.
*   Score view, folding, waveform view, JSON export and portable export.
*   Middle-track deletion and stable IDs, the 64-track capacity, persisted vs runtime state, device hot-swap and autosave.

## Reviewed and not flagged

Snapshot publishing still serves Shape previews and undo without links. Also kept: three crates, `probe-keys` first, binding table,
which-key, cursor and playhead separation, engine-resolved play and stop (`gmuse` D-8), and viewport keys.

## Limits

Read in full: persona, standard, target, three stage reports, `gmuse` transport spec, keymap guide, README. Targeted search only: M8
manual, Digitakt manual, Tracker CE source, `gmuse` code. Not verified: web sources, Renoise and DAW behaviour. Nothing was built or
listened to. The neighbour-view cost and sample-pool rule are design judgements, not tested results.
