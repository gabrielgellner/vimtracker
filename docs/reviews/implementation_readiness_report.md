# Stage 9: Implementation Readiness Test

**Target:** `docs/designs/vimtracker.md`, lines 590-1007

**Verdict: FAIL against the Zero-Decision Rule.** The M1 foundations are close. Three gaps
sit before G1: project and structure authoring, the ramp anchor model, and field-level entry.

**Reviewers:** implementation readiness (4 Blockers, 24 Majors, 8 Nits), code details (3
Blockers, 23 Majors listed, though its summary says 18, and 7 Nits), style (0, 20, 16). Findings
are merged below, with reviewer tags [R], [C] and [S].

**Nothing was compiled or listened to.** Reviewers searched the manuals and Polyend source
rather than reading them in full. "Confirmed to build together" (L603) was not verified by
reviewers. Most plan items are concrete, and the checks at the end held up.

## Blockers

### 1. No way to author song structure, and the new-project state is undefined [R, C]

*   **Scope:** L256, L686-688, L732, L739-740, L749, L896-903, L916-924
*   `:new` doesn't say what exists. Issue 7 has no phrase to open and no way to pick an instrument,
    and value entry isn't in scope. "Current instrument" and "last-used instrument" have no starting
    value or mechanism.
*   Nothing creates a phrase or chain, assigns a chain to a track, adds a song row, or edits a chain
    entry, transpose or instrument setting. `gd` on an empty cell is undefined, while the M8 has "new
    unused number" (`m8.txt` 985-988).
*   `vimtracker` with no or a missing project, and `:sample` or `:w` without a folder, are undefined.
*   **Fix:** specify the `:new` skeleton, per-view entry and operators, the ID rule, and no-project behaviour.

### 2. The ramp anchor model contradicts the Technical Plan [R, C, S]

*   **Scope:** L692-695, L820, L933-938, L946, against L384-388
*   "A segment between anchors runs from 0 to 1" conflicts with `t` running "across the selection".
    Anchors are undefined: which cells, whether interior values survive, what `r` swaps, where links
    store them, whether `.` reads them. Which columns can be shaped, and `V` or multi-column selections,
    have no rule.
*   **Fix:** write the algorithm with one multi-anchor vector, or remove anchors as Stage 8 proposes.

### 3. Table columns defeat CUJ 3 [R, C]

*   **Scope:** L157-160, L327-330, L643, L756, L758, L960-965
*   Transpose and volume columns have playheads but no slot for `HOP` or `TIC`, so the arpeggio doesn't
    loop. End-of-table behaviour and how `table speed` relates to `TIC` are undefined. M8 loops by default
    (`m8.txt` 1336-1338).
*   **Fix:** make them follow FX1, add a column-targeted command, or have the instrument set their rate.

### 4. Mid-song sample equality cannot pass [R, C]

*   **Scope:** L193, L315-318, L343, L969-971
*   Counting rows doesn't restore held FX, `REP`, RNG, `TIC00` positions, `IFC` counts, voices, smoothers or tails.
*   **Fix:** narrow the assertion, or specify a chase pass for held state and RNG and state what tails do.

### 5. The plan won't compile, or can't pass, as written [R, C]

*   **`rc` feature (L607, L653, L876):** `BTreeMap<u32, Arc<T>>` with derived serde needs serde's `rc` feature
    (`serde_core-1.0.229/src/ser/impls.rs:515`). Only `derive` is listed, and L599 forbids changing the table
    without updating the document. The code-details reviewer rated this a Blocker, the readiness reviewer a Major.
*   **Type before it exists (L880, L939-941, L592):** Issue 4 tests a generator link that Issue 14 creates. Issue
    3's `Phrase` has the same dependency.

## Major: dependencies and project layout

6.  **`hound` is in the wrong crate [R, C].** L610, L830, L844. `render.rs` in `vt-engine` writes WAV. Render format is
    also unspecified.
7.  **`vt-tui` has no test location [R, C].** L850-862, L910-915. It's binary-only, with no `lib.rs` or `tests/`, yet
    needs key-sequence tests. Issue 1's `cargo test` needs a stub `main.rs` that Issue 2 adds.
8.  **Resampling is undecided [C, R].** L353, L825, L879-883. No algorithm or dependency, though `gmuse` pins `rubato`
    5.0.0. It changes sound and golden hashes. WAVs with more than two channels and unsupported formats are undefined.
9.  **Golden hashes have no algorithm [C, R].** L420, L845, L951. `std` hashers aren't stable across releases. `gmuse`
    has `twox-hash`. Name it, plus file format and what's hashed.
10. **Argument parsing and errors are undecided [C].** L749, L831. No `clap` or `thiserror`, though `gmuse` uses both.
11. **The format version check can't work as described [C, R].** L406, L653, L881. A real schema change gives a field error
    before the version is read. Define a version-first read.
12. **The allocator test needs per-thread arming [C].** L846, L894. A process-wide counter sees other test threads.
    `gmuse`'s `rt_guard.rs` already solves this and should be adopted.
13. **Existing files are described as new [C, S].** L789-802, L866-867. `README.md`, `.gitignore`, `LICENSE` and the design
    already exist. Mark files Create or Modify.

## Major: scheduling

14. **Commands in no issue [R, C, S].** L743-745. `:bpm`, `:len`, `:set step`, `:set scale`, and Insert value entry for `VOL` and
    `INST`. G1 needs tempo and length. `TRSP` has no sign key. M4's new fields have no way to be edited.
15. **Leader and `apply` ordering [R, C, S].** L860-861, L898, L907, L920. Issue 7 needs leader keys that Issue 11 adds. Issue 7
    edits notes before `apply` exists in Issue 8. The binding table, which-key popup and `Space ?` have no file or owner.
16. **G8 has no owner [R, C, S].** L593-595, L953-956, L1002-1003. No issue runs it. The go/no-go gate doesn't say whether it must
    be measured first. Track Capacity has no owner either.
17. **G1 has no pass definition [S].** L594-595, L953-956, L958, L973, L983. "Only if G1 passes" gates three milestones, yet Issue
    17 has three outcomes and no threshold or decider.
18. **Issue 11 is too big [R, S, C].** L592, L916-924, and CUJ 5's folding depends on Issue 21 (L175, L976).

## Major: data shapes and timing

19. **Core contracts are named, not defined [R, C].** L810-846, L890-892. `Song`, `Track`, `SongRow`, `Instrument`, `Action`, `Outcome`,
    the error type, `Engine`, `EngineHandle`, `process`, command and telemetry messages, `SamplePool`. Field names are the file format.
    `Note` range and `C--1` don't fit. Defaults for step, octave, seed and table rate are missing.
20. **Sample positions have no rule [R, C].** L633, L887-889. At 44100 Hz a row is 5512.5 samples. Specify integer arithmetic from an
    absolute tick, rounding, BPM type and range, tempo changes, and the BPM bound that sizes the delay buffer.
21. **Grooves and speed don't compose [R].** L765, L966-968. A unit at 3/2 speed is 26.67 ticks. Define scaling for grooves and `RET`.
22. **`DEL`, `KIL` and `OFF` are out of scale [R, C].** L762-764. Raw ticks mean `FF` reaches about one row, against about 42 rows on the
    M8. Confirm or rescale.
23. **Held FX and combination rules are missing [R, C].** L648, L754-775, L780-783. Starting values, reset rules, how cell, table, FX and
    instrument volume combine (M8 multiplies, `m8.txt` 1342), and transpose overflow.
24. **Real-time state can be lost [R, C].** L741, L835, L876, L890-893. Newest-wins can skip a version carrying grown `Vec<TrackState>`.
    Deleting a middle track misaligns state, song rows and RNG. Growth needs return-queue room for two items.
25. **Sample replacement under a playing voice [R].** L825, L885-886. Position may overrun the new sample.

## Major: FX, editing and keys

26. **Two-digit values contradict "no fixed limits" [R, C].** L650, L756-761, L876, L961. `TBL`, `TBX`, `GRV` and `THO` stop at `FF`, `HOP xy`
    reaches rows 0-F, `x` in phrase `HOP` is undefined, and `TBX00` makes table 00 unusable. Accept the limit or give a widening rule.
27. **FX semantics are incomplete [R, C, S].** L643, L752, L765-769, L777. `RET` with `y=0` is a zero interval in a callback that may not panic,
    and the row-text example `RET10` has it. M8 defines `y=0` as single retrig. `REP` signs and no-previous case, `RND` scale, whether `CHA`
    also skips FX to its left, `IFC` bases, `DEL` past the next row, and `RES` range.
28. **"Note held" feature was dropped silently [C].** L335-336, L754-775, L960.
29. **No cursor field model [R, S].** L665-666, L678, L689, L700-703, L812. `h l` and `w b` have the same description. "Cell" means a row in
    `cell.rs` and a field in `x` and Visual, so `x` might equal `dd`. Value and FX entry are undefined.
30. **Grammar semantics are undefined [R, C, S].** L361, L675-681, L690-691, L730, L911-914. `3>ib`, `dj`, `p` overflow, whether `d` fills a
    register, edge motions, `+` and `-` by field, operators in other views, `.` replaying Insert, and `.` on a selection.
31. **Keys missing from the table [R, C, S].** L658-706, L976-977. Command mode, Visual `Esc`, the `z` keys, and `grr`'s display.
32. **Play scope is underspecified [R, C, S].** L686-688, L727, L900-901. Whether phrase scope sounds other tracks, looping, song end, `Enter` in
    other views or while playing. "Resuming from its start" contradicts itself, and two toggles only cancel when starting stopped.
33. **Note preview [R, C].** L372, L732. Which voice, track, FX and table, and how it reaches the engine.
34. **Clone repointing [R, C].** L684, L917, L921. CUJ 4 needs `gc` to rewrite the referring chain slot. Which one, and when opened via `gt`?
    The parent may itself be shared. New-ID rule and `grr` UI are unspecified.
35. **`probe-keys` has no decision rule [R, C].** L681, L683, L869-874. Without enhancement, `Ctrl-I` arrives as Tab (`crossterm-0.29.0/src/event/sys/unix/parse.rs:102`).
    The Kitty flag policy is absent, and `gmuse` pushes only `REPORT_EVENT_TYPES` because others change encoding. `KeyEventKind::Repeat` handling
    is unset, and repeat on `Enter` would retrigger.
36. **Generator links [R, C, S].** L685, L817, L941, L945-947. A rerender deletes its own link under the `apply` rule. Whether previews use `apply`, partial
    overlap, and `:len` truncation are undefined, and `gv` fails after any other selection.
37. **Render and audio gaps [R, C].** L745, L749, L780-787, L845, L853, L950-952. Seed option, default length, song end, tails, WAV depth, mono devices,
    stream errors, device loss, block-size bounds, CPU and late-block formulas. `gmuse`'s `device.rs:467-485` separates `DeviceChanged` and `Xrun`,
    and warns never to print from the callback. Chamberlin SVF is unstable near 20 kHz at 44.1 kHz. Freeverb tunings are absent.
38. **Quit and undo behaviour [R].** `:q` and `:e` with unsaved changes, `:w path` copying samples, Insert undo granularity.

## Major: wording

39. **Terms collide [S].** "Page" and "view" name one thing, and "page" also means a screenful (L661-662, L671, L706, L709, L728). "Row" and "step" unqualified
    (L686, L743, L919, L939, L968-969). "Gain" is both signal level and a curve parameter (L648, L693, L770, L936). "Onset and S-shape" has no mapping.
40. **The stated key rule conflicts with the table [S, R, C].** L662, L670, L721-723. `gT` and `H`/`L` break "never a different action". Exempt Vim-inherited
    keys and cite the guide. "No key is both" is stricter than the guide.
41. **Master stage has two names [S, R, C].** L350, L783, L842. "Limiter" vs `tanh` clip.
42. **Constants lack meaning and units [S].** L616-637. Add a Meaning column. Several are never referenced elsewhere.
43. **Audio defaults don't fix values [S].** L780-783. Give values, or say they're chosen at implementation and pinned by golden hashes.
44. **Subcommand list is wrong [S, C].** L411, L749, L987. It omits `export-json`, with two signatures.
45. **Smaller overstatements [S].** L909: "history stops" reads as stopping recording. L980: "readable" isn't checkable. L990, L999-1000: "slices" appears nowhere else, and "every rule has a test" isn't supported.

## Nit

*   **Tests [R, S, C]:** `sg` tests a page that arrives in Issue 19 (L922, use an injected list). `3gt` numbering shifts with pages (L662, L710). L889, L967-968 and L986 name no asserted values. 1/8/64 appears only in Issue 5 (L887-889).
*   **Pinning [C, S, R]:** `nucleo-matcher` is `0.3` while others are exact, and `gmuse` locks 0.3.1 (L614). No Rust version pinned. Line 616's path omits `crates/`.
*   **License [C]:** "GPL-3.0" vs `gmuse`'s `GPL-3.0-or-later` (L798). Crate `license` field unspecified.
*   **Scale rules [R, C]:** intervals, which pentatonic, root syntax, whether Insert snaps (L747-748).
*   **Curves [R, C]:** state the formulas and 0.5 default. All vector values were reproduced with `f(t)=t/((1/b−2)(1−t)+1)`, Schlick's gain and round-half-up (L692-693, L935-936).
*   **Audio [C]:** `80` isn't an exact pan centre, and no pan law (L771-773). `gmuse`'s `Pattern::parse` at `src/ui/picker.rs:245-267` is direct reuse.
*   **Row format wording [S]:** L639, L642, L650, L756, L265.
*   **Wording [S]:** L662-663, L672, L675-676, L682-683, L709-714, L723, L726-732, L745, L758, L762-764, L796-797, L826, L871-874, L923-924, L948, L951, L963, L966-967, L985, L1007.
*   **Layout [S]:** G8 result goes in `g1-result.md` (L801). Uneven wrap and orphaned labels (L897-911, L976-978). L809's "values have one source" overstates.

## Checked and accurate

*   Curve vectors at L935-936, reproduced by two reviewers.
*   Timing: 240 ticks per row, 40 per groove unit, 960 PPQ, and all seven speeds whole for straight rows.
*   Note key layouts are chromatic and complete.
*   No key is both an action and a prefix within the Normal table.
*   Engine-resolved toggle, binding table, and cursor and playhead split match `gmuse` D-8, D-2 and D-17.
*   `ratatui::crossterm` at 0.30.2 and the F32 rule (`device.rs:44`) match `gmuse`.
*   `ron` 0.12.3, `hound` 3.5.1, `rtrb` 0.4.1 and `serde_json` 1.0.154 exist in the local registry.
*   The repository remote matches. `HOP` and `TBX00` wording matches the M8.
