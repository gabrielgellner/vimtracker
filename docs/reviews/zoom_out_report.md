# Stage 5: Zoom Out Review

**Target:** `docs/designs/vimtracker.md`, full file

**Verdict: does not pass.** 1 Blocker, 7 Major, 10 Nit. The objective and motivation are
clear, so the context check passes. The main failure is that the G1 gate does not test
the hypothesis it is meant to decide, and the work before it is much larger than "as
cheaply as possible" (line 18).

**Read:** persona, standard, target, `gmuse` spec, keymap guide and README. The M8
manual, Digitakt manual and Tracker CE source were searched and read in part.

## Blocker

### 1. G1 cannot answer the question that decides the project

*   **Scope:** Problem, lines 29, 42-51; G1, lines 55-58; lines 593-595, 730-731; Issue
    17, lines 953-956; M4 and M5, lines 958, 973
*   **Rationale:**
    *   Line 42 says the project is justified only if three things "together" make a
        clear difference: the grammar, live sweeps, and tables with shared phrases.
    *   G1 runs before tables exist, so the third differentiator is partly untested.
    *   G1 also runs before the score view exists, so the prototype has the limitation
        line 29 holds against the M8: one track's phrase at a time.
    *   The Problem's example, "clear this track" (line 45), has no real meaning at G1,
        because lines 730-731 make `it` the same as `ip` in the phrase view.
    *   The comparison tool shows many tracks at once, so a negative result is likely
        and uninformative, yet lines 58 and 594 make that result stop the work.
    *   Issue 17 records the confound without removing it.
*   **Simplest fix:** Narrow the gate to what it tests (grammar, sweeps, shared phrases),
    move a minimal multi-track view before G1, and keep tables behind a later gate. Or
    keep the "together" claim and put a minimal single-playhead table before G1. Problem,
    G1 and Issue 17 must make the same claim.

## Major

### 2. G1 has no pass criterion and no chosen test material

*   **Scope:** G1, lines 55-58; Issue 17, lines 953-956
*   **Rationale:** "Clearly better" is undefined, and the tool and loop are unnamed. The
    author built vimtracker and has years of fluency in the other tool, and whichever
    tool goes second gains from repetition. The rule must be written before the result
    exists. The loop also constrains scope: without synths, grooves or tables, some loops
    cannot be recreated.
*   **Simplest fix:** Name the tool and loop, write down time and preference thresholds,
    and derive the M3 feature set from that loop.

### 3. The "existing tracker" alternative is rejected on an unstated premise

*   **Scope:** Lines 39-48, 451-460
*   **Rationale:** The document calls extending a tracker "possibly a much faster way to
    test the vim grammar", then rejects it because "the goal is a terminal-native tool".
    Terminal operation is not one of the three justifying differentiators, and line 41
    concedes existing trackers are the strongest argument against the project.
*   **Simplest fix:** State terminal-first as an explicit premise, or time-box a spike of
    grammar plus sweep in Renoise or a fork of an open-source tracker. The reviewer did
    not verify whether Renoise's tool system can capture modal keys.

### 4. Generator links and their abstractions are built before G1, which does not need them

*   **Scope:** G11, lines 90-93; CUJ 8, lines 195-203; lines 375-400, 491-496; Issues
    13-15, lines 933-947
*   **Rationale:**
    *   Differentiator 2 and G5 are met by Shape mode. CUJ 2 passes without a link.
    *   Links add stored ownership, a rule inside every `apply`, persistence, undo
        semantics, a risk row and a `gv =` resume path.
    *   The mask and value-source split has one instance of each before G1.
    *   The stored seed has no consumer, because a ramp is not random.
    *   Note ramps and `scale.rs` serve no CUJ before G1, and anchors go beyond CUJ 2.
    *   The rejected alternative at lines 491-496 calls later reshaping "a requirement",
        but no goal outside G11 supports that.
*   **Simplest fix:** Before G1, build `curve.rs`, Shape mode and last-used-shape `.` only.
    Move G11, CUJ 8 and masks after G1. Links are cheap to add later because backward
    compatibility is a non-goal.

### 5. Pre-G1 scope includes navigation and sound features the hypothesis does not need

*   **Scope:** Lines 660-664, 708-714, 856-858; Issue 11, lines 916-924; Issue 12, lines
    928-932; lines 779-783
*   **Rationale:**
    *   At G1 the picker fuzzy-matches about four page names, with two bindings. `gt`,
        `3gt` and `gd` already reach everything.
    *   Issue 11 bundles three views, chain transpose, tree navigation, cloning, tracks,
        leader handling and the picker, contradicting "small enough for one pull request".
    *   Issue 12 ships 13 commands plus a reverb and stereo delay. `CHA` and `RND` sit
        under the post-G1 Tables goal.
*   **Simplest fix:** Defer the picker and split Issue 11. Keep the binding table and
    which-key popup. Limit Issue 12 to `VOL`, `PAN`, `CUT`, `RES` and what the G1 loop uses.

### 6. G6's "no limit" costs real complexity and is not achieved by the design

*   **Scope:** G6, lines 73-77; lines 259-261, 283-286, 650, 756-761; Issue 6, lines
    890-895; Issue 18, line 961
*   **Rationale:**
    *   FX values are two hex digits, so `TBL`, `TBX` and `GRV` address 256 IDs against
        32-bit IDs at line 260.
    *   `HOP xy` reaches rows `0`-`F` only, while phrases and tables may have any length.
        `THO xx` stops at `FF`. M8's `HOP` comes from a machine with 16-row tables.
    *   The goal is false as written, and it also pays for widening hex columns and live
        `TrackState` growth through the real-time path.
    *   G6 is not a justifying differentiator, and G8 only asks for 16 tracks.
*   **Simplest fix:** Create the engine with fixed capacity for 64 tracks and one constant
    to raise it, use two-digit IDs, and decide what `HOP` does beyond row `F`.

### 7. "Same result from mid-song" cannot be met by the proposed function

*   **Scope:** CUJ 7, lines 192-193; lines 315-318, 331-334, 343-345, 686, 823; Issue 20,
    line 970
*   **Rationale:** A `vt-model` function that counts rows cannot reproduce state from
    interpreting FX: held values, `HOP` jumps, `TIC00` positions, `IFC` counts, and random
    or `CHA`-gated results. That interpreter is in `vt-engine`, which `vt-model` cannot
    depend on. The gap exists before G1 too: `Enter` plays from the cursor row, and nothing
    says what a held `CUT` value is at that point.
*   **Simplest fix:** Narrow the guarantee to track positions and state that held FX values
    are not chased in the prototype, or implement chase in the engine as a silent run.

### 8. Post-G1 work is over-specified, and `gmuse` reuse is undecided

*   **Scope, post-G1:** G10, lines 87-89; lines 305-345, 756-761; M4-M6, lines 958-988;
    lines 990-991, 510-511
*   **Rationale, post-G1:** Lines 990-991 defer slices and generators because M4 "may
    change what they should do". The same argument applies to M4-M6, since G1 can return
    "redirect". Carry rows, per-column transpose and volume playheads, and per-note
    `TIC00` are speculative, and "some songs need" has no evidence.
*   **Fix, post-G1:** Keep only structural constraints the pre-G1 model must not block, and
    re-plan M4-M6 after the G1 result.
*   **Scope, reuse:** Lines 568-571, 798-799, against Target Files at lines 825, 846, 854, 856
*   **Rationale, reuse:** `gmuse` is listed as a reference, but every matching file is new:

    | Planned | Existing in `gmuse` | Note |
    |---|---|---|
    | `tests/support/alloc.rs` | `crates/gmuse-engine/src/rt_guard.rs` | Direct copy; solves the panic-in-allocator and `--release` traps |
    | `input/keys.rs`, binding table | `src/cmd/keys.rs`, `src/cmd/bind.rs` | `keys.rs` normalises events and handles leader; `bind.rs` not read in detail |
    | `nucleo` ranking in `input/picker.rs` | `rank` in `src/ui/picker.rs` | Copy `rank` only; `Picker` is tied to library types |
    | which-key delay | `which_key_due` in `src/ui/mod.rs` | Small pure function with tests |
    | `audio.rs` device setup | `crates/gmuse-engine/src/audio/device.rs` | Same default-config and F32 rule |
    | Resampling in `sample.rs` | `crates/gmuse-engine/src/audio/resample.rs`, `rubato` | The plan names no resampler; `gmuse`'s is a chunked stream, so adapt it or call `rubato` |

*   **Fix, reuse:** `gmuse`'s UI code has no shared crate, so copy rather than depend, and
    say so per file in Target Files.

## Nit

9.  **Grammar and curve definitions disagree.** Lines 361, 679, 691: line 361 lists `p` and
    `=` as operators, but the key table has `p` as paste and `=` as Visual-only. Lines
    384-387 and 933: ramp position runs across the selection, but Issue 13 uses per-anchor
    segments.
10. **Crate boundaries and dependencies disagree.** Lines 383, 607, 610, 749, 821, 830,
    843, 844, 851, 876: `render.rs` writes WAV, but `hound` is listed for `vt-model` only.
    Seeded generators live in `vt-model`, while PCG32 lives in `vt-engine`. `Arc<T>` in
    serialized `Song` needs serde's `rc` feature. Subcommands with flags have no `clap`.
11. **Ordering and ownership gaps.** Line 880 vs 939: Issue 4 tests a generator link that
    Issue 14 creates. Lines 743-744: `:bpm`, `:set step`, `:set scale` and `:len` are in no
    issue. Lines 1002-1003: the G8 measurement has no issue.
12. **The publish path is slightly larger than needed.** Lines 219-228, 274-278, 634-637,
    832: with newest-wins at both ends, publish depth 8 adds handling without reducing
    latency. Depth 1 plus the stash does the same job. The diagram omits the command queue
    and track-state transfer.
13. **Key conventions are stricter than the keys.** Lines 662, 670, 721-722: `gT` carries
    direction and `H`/`L` are not wider `h`/`l`. Add a one-line exemption for Vim keys.
14. **JSON export and the waveform view serve no CUJ.** Lines 98-99, 411-414, 609, 985-988.
    Drop `serde_json` from the pinned table until a consumer exists.
15. **Some detail is too high or unjustified.** Lines 387-390: the curve formula belongs in
    Fixed Specifications. Lines 630-633, 779-783: tuning values such as `DECLICK_FRAMES`
    should be free to change by ear without a document update.
16. **The sale non-goal has a weak rationale.** Lines 108-109: the license restricts selling
    the complete application or a direct port and permits component reuse. Drop the
    reasoning or the non-goal.
17. **Two statements carry little weight.** Line 61: "reachable without a mouse" is vacuous.
    Lines 36-37: "none describes" live curve shaping is limited to the sources reviewed,
    which appear not to include DAW or tracker automation editors.
18. **The license identifier is ambiguous.** Lines 798-799: `gmuse` declares
    `GPL-3.0-or-later`, while this document says "GPL-3.0" and "matches".

## Reviewed and not flagged

*   Snapshot publishing over a mutex: more setup, but it provides G2 undo and preview and
    serves G8.
*   Three crates: keeps model and engine testable without a terminal at little cost.
*   Single-file RON with no migration, a data-driven binding table without runtime
    remapping, the key-release-free design, and `probe-keys` first.
*   Citations checked: M8 equal-length advice (`m8.txt` 762-765), track speeds and `RESET`
    `INF` (`dt2.txt` 2409-2413), and `RND`, `REP` and `OFF` semantics (`m8.txt` 3056-3081,
    3154-3157).

## Limits

*   **Not read:** web references, including the Waterloo tutorial, Metropolix, Ableton,
    Ardour, Tracktion, Furnace and Schism.
*   **Not verified:** Tracker CE line counts (lines 444-446), the "Polyend Fill" attribution
    (lines 381, 493), and the statement that Furnace and Schism use locks.
*   **Not assessed:** whether the pinned crate versions build together.
