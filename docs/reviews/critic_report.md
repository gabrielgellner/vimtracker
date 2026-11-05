# Stage 7: Critic Review

**Target:** `docs/designs/vimtracker.md`, lines 1-589

**Reviewers:** design (2 Blockers, 11 Majors, 8 Nits), structural (0, 12, 11), style (0, 12,
12). Findings are combined below, with duplicates merged and each reviewer's tag shown as
[D], [T] or [S]. All sections the template requires are present, so these are problems of
substance rather than missing sections.

**Not verified by any reviewer:** Tracker CE line counts (L444); URLs; claims about Buzz,
Renoise, OpenMPT, Furnace, Schism, Ardour and Tracktion; the Waterloo tutorial (L328-330);
the RON/JSON test (L412).

## Blockers

### 1. The go/no-go test cannot answer the question it is set [D, T, S]

*   **Scope:** L42-51, L55-58, L368-371, plan L594, L953-956
*   Lines 42-48 justify the project only if three things "together" make a difference, and
    G1 runs before tables and the score view. A multi-track loop is written in a one-track
    view, the limit L29 criticises in the M8.
*   "Clearly better" has no threshold. It does not say what happens if vimtracker is faster
    but not preferred. L51 says stop and L58 says stop or redirect.
*   "Usual tool" is unnamed, though the counter-argument is about Renoise, OpenMPT and Schism.
    Familiarity and bias are uncontrolled, and the second run gains from repetition.
*   **Recommendation:** Build a minimal table and multi-track view before G1, or rewrite L42-51
    to say what G1 can conclude. Set a written pass rule, add an "inconclusive" outcome, name
    the tool, and fix the order of runs or use a new loop.

### 2. The mid-song guarantee cannot be delivered [D]

*   **Scope:** L190-193, L237-238, L315-318, L333-334, L343-345, plan L970
*   A function that counts rows without audio cannot reproduce `HOP`, `HOPFF`, `CHA`, `RND`,
    grooves, `IFC`, `TIC00` positions, held values, `REP` ramps, random state, tails or smoothers.
    It sits in `vt-model`, which cannot see the interpreter in `vt-engine`. On a second pass
    through a song with no Reset rows, "from the last Reset row, or the start" is ambiguous.
*   This blocks G10, not M1-M3.
*   **Recommendation:** Narrow the promise to row and phrase position, or run the interpreter
    silently from the last Reset row in `vt-engine`, and exclude tails.

## Major: design substance

3.  **Alternatives are incomplete or circular [D, T].** L17-19, L42-48, L113, L438-545, L451-460,
    L491-496. Extending a tracker is rejected because "the goal is a terminal-native tool", though
    the terminal is not a justifying differentiator and L17 asks for the cheapest answer. One-shot
    generators are rejected because links are "a requirement", with no evidence. Terminal vs GUI,
    the action-then-target grammar (vs selection-first), the bias/gain model (vs breakpoint
    envelopes) and a fixed high track cap have no evaluation.
4.  **The live-shaping claim rests on a weak comparison [D, T].** L34-37, L47, L518, L559-577. The
    sources are not listed. From general knowledge, DAW and Renoise automation envelopes already
    allow reshaping while a loop plays, so the real claim may be narrower: keyboard shaping inside
    the grid.
5.  **A 10 ms smoother won't stop sweeps sounding stepped [D].** L125-126, L351-352, G5. At 120 BPM a
    row is 125 ms, so each value ramps for 10 ms and holds about 115 ms. A full 16-row `CUT` sweep
    moves about two-thirds of an octave per row. The fault is structural, not tuning. Compare ramping
    across the row with look-ahead, proportional smoothing, or tick-rate updates.
6.  **Independent table columns break CUJ 3 [D].** L157-160, L327-330, plan L756, L758. Transpose and
    volume columns have no FX slot for `HOP` or `TIC`, so the arpeggio doesn't loop. Tie them to the
    first FX column, as on the M8, or define how they get a loop point and rate.
7.  **Tick maths fails with grooves [D, S].** L255, L298-303, L307-313, L323. At 3/4 speed a groove
    unit is 53.33 ticks, so `07 05` gives 373.33 and 266.67, and no rounding rule exists. A section can
    end mid-row at non-1 speeds. "Tick" has two meanings, and `DEL`, `KIL` and `TIC` don't say which.
    Plan L968 tests only ticks per row, so it would hide this.
8.  **Tracks have no stable identity [D].** L252, L259, L283-294, L340-342, plan L741. Deleting a middle
    track shifts playheads, voices and random streams. Line 275 can also skip the version carrying
    extra per-track state. Give tracks stable IDs never reused, and key state and seeds to them.
9.  **Sample ownership and version-change gaps [D].** L256, L280-281, L288-294, L353, plan L885-886, L891.
    "One top-level Arc" conflicts with a second `Arc<SamplePool>`. Replacing a sample during playback
    moves a voice to a different buffer at the same position, risking a jump or out-of-bounds read.
    Removal rules omit instruments, samples, tables, grooves and song rows. There's no dangling-ID
    policy, and sample memory across 1,000 undo versions is unaddressed.
10. **Transport is missing from the architecture [D, T].** L142, L209-231, L270-294, plan L637, L686-688,
    L725-728. Play, stop, play-from-cursor, scope, note preview and seeking aren't in the Technical Plan
    or diagram. The diagram also omits the per-track state transfer. Whether preview interrupts a playing
    track's voice is unspecified.
11. **"Tables can test whether the note is held" has no mechanism [D, T].** L335-339. No command does it,
    and the M8 half is inferred. Add a command or move it to Future Improvements.
12. **Returning to a generator has no reliable path [D, T, S].** L198-203, L263, L392-400, L433-434, L494-496,
    plan L685, L946. `gv` means the last selection, and resuming needs exactly the linked range. Owned cells
    aren't visible. "Cell" means a whole row at L263, while CUJ 8 needs per-field ownership, or a note edit
    on a swept row would break the link. Add a motion to select the generator under the cursor, a visible
    marker, and ownership by field and slot.
13. **The song model is incomplete and mixes document and playback state [D, T].** L84-86, L180-183, L247-257,
    L340, L343, L352, L365, L390, plan L742, L749. Project seed, default tempo, scale and format version aren't
    listed. `render` has no seed argument, and CUJ 6 has no device to supply a rate. `current groove` looks like
    runtime state, though `GRV` changes it and versions are immutable. `mute` and `solo` as versioned edits fill
    undo, so `u` can unmute. Track and row mutes have no precedence. Separate persisted from runtime state and
    state what enters undo.
14. **The risk table omits failure modes [T].** L353, L366, L426-436, L553. Device loss or rate change, missing or
    undecodable samples, memory growth, and disk-full during save.

## Major: clarity and structure

15. **"Row" has several meanings [D, T, S].** L187-192, L250, L292, L298-300, L307-313. Song, phrase, table and
    time-unit rows, three senses within four lines at L187-190. Line 292's "next row of its chain" and L266's "rows
    refer to phrases" are wrong by the model. Call song rows "sections", already used at L88-89.
16. **"Step", "row", "cell", "value", "column" and "slot" drift [S, T].** L61, L71, L93, L115, L143, L147-151, L160, L175,
    L197, L263-265, L322, L329-332, L362, L378, L382. CUJ 8's "filter column" doesn't exist in the cell model, which has
    three FX slots. A reader can't tell what a sweep selects when `CUT` moves between slots. Chain, phrase, table, groove,
    aux table and score view are never defined.
17. **"Action" means two things [S].** L44, L59, L237, L272, L358, L361. Use "operator" for the key and "action" for the data.
18. **The grammar's two descriptions disagree, and no journey tests it [D, T].** L44-46, L59-61, L137-203, L361-362, plan
    L676-691. `p` and `=` are listed as operators but behave otherwise. Editing omits counts and registers. No CUJ exercises
    operator + target, counts or registers, though the grammar is the first differentiator. G1, G7 and G8 have no journey.
19. **Range depends on undefined things with no pointer [T, S].** L122, L141, L149, L181, L198, L280, L325, L337-339, L357-364.
    Play, the render command, `gv`, Shape and Picker behaviour, "first milestone", aux tables and most FX commands are defined
    only in the plan. Add cross-references.
20. **Absolute or subjective guarantees [S].** L82-83, L213, L405-406, L432. "No audible dropouts" should be a zero count.
    "Cannot move notes" conflicts with accepted late blocks. "Cannot leave a partial song" should name the failure covered.
21. **Unqualified rule contradicted later [S].** L160, L325, L327-328, L331-334. "A note starts its table from row 0" has a `TIC00`
    exception. Word it as the default and name the exception.
22. **Overstated claim [S, D].** L445-446. "Its synth engines are closed binaries" is too broad: `core/synth/engines/` holds MIT
    engines including CHIP. Say some synth and DSP code is closed.
23. **User terms aren't connected to technical terms [S, D].** L70, L149, L198, L385, L387. "Onset and S-shape" vs bias and gain.
24. **Problem claims lack a measure [S].** L28-29, L31-32, L34. "Slow" has no baseline, "strong" is evaluation, "most of those
    limits" lists none.
25. **Preference presented as a rejection reason [S].** L470, L489, L503, L510, L529, L545. "Least code" is claimed for both the mutex
    and RON with no count. Mark these as preference or give a comparison, as the `gmuse` spec does with "taste, not derivation".
26. **Speed, rate and tick vocabulary [S, T].** L159-160, L249, L255-256, L298-301, L323, L327-328. "Speed", "step rate", "tick rate" and
    "rate" overlap, and the instrument's rate is never stated as the column default.

## Nit

*   **Device changes [D]:** L241-243, L352-353. No rule for a headphone unplug or rate change, and no resampler listed. Close to Major if
    you switch outputs often.
*   **Curve edge cases [D]:** L71, L385-390, plan L820, L933. Selection-wide vs per-anchor `t`, one-row selections, and exact end values vs
    scale snapping.
*   **Shared phrase accidents [D]:** L45, L266, plan L730. "Clear this track" is `ip`, which clears a shared phrase in every use.
*   **Interpreter safety [D]:** L337, L552. A zero-time `HOP` cycle would hang the callback. A jump should consume time, or cap jumps per block.
*   **Queue wording [D]:** L277-278, L431. Draining must return every skipped version, so check room for the full drain.
*   **Failure handling [D]:** L405, L422, L432, L552. "Must not panic" has no defined outcome, timing a late block may miss OS underruns, and no
    autosave means a crash loses all unsaved work.
*   **Terminal input [D]:** L121-122, L149, L436. Esc latency under tmux `escape-time` affects Shape mode. Ctrl-I and Tab may be indistinguishable.
*   **Seeds, project terms and "preview" [S]:** L85, L91, L100-102, L181, L341, L367, L372, L378, L392, L435, L520, L544. Name the two seeds, use
    "project" and "song" consistently, drop "jam", and rename one sense of "preview".
*   **M8 and Elektron references [S, T]:** L329-330, L336, L502, L583. Cite the tutorial, say "the manual documents no such test", the manual says
    "the same number of phrases", and "the rest of Elektron's set" overclaims.
*   **Vague words [S]:** L18, L39, L50, L64, L299, L366, L431, L464, L500, L553.
*   **Imprecise wording and repetition [S]:** L61, L74-77, L80-82, L99-102, L212-213, L372-373, L412, L435-436, L518, L555, L569-571.
*   **Headings and layout [T]:** L426, L440-545, line wrap drift, G10 and G11 placed after G9. "Other save formats" packs four options in one heading.
*   **Smaller consistency [T, D]:** L129 and L343-345 classify only 9 of 19 commands. `GRV` is missing from the first command set. `basedrop` is rejected
    without introduction. JSON export sits in a Non-Goal with no CUJ. Track groups overlap CUJ 5's folding.
*   **Resources [T]:** L561-577. `gmuse` has no link or path, the transport spec and keymap guide aren't listed, and the Metropolix to-do belongs elsewhere.

## Checked and consistent

*   M8 chain length advice (`m8.txt` 762-766), groove units (1438-1441) and independent column rates (1350).
*   Elektron `RESET` `INF` (`dt2.txt` 2424-2425) and `A:B` (2525). The parameter-lock hedge at L128 is fair.
*   Polyend Fill masks (`mtSequencer_patternEdit.cpp` 245-294), the CE license restriction, and the sequencer's UI includes.
*   Tick arithmetic at L298-302 for straight rows. `gc`/`gC` and `grr` follow your keymap conventions.
