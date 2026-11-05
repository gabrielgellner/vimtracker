# Stage 6: Comprehension Test

**Target:** `docs/designs/vimtracker.md`, FeatureRange 1-589, DetailedRange 590-1007

**Verdict: FAIL against the re-implementation standard.** A fresh engineer would have to
make independent design choices in the sweep, the editing grammar, the views and tables,
which are the features the G1 test exists to judge. 6 Blockers, 18 Majors, 7 Nits.

**Coverage:** target, persona, standard, `gmuse` spec and keymap guide read in full. M8
manual, Digitakt manual and Polyend source checked by targeted search and section reads.

**Not verified:** source line counts (L444); M8 tutorial claim (L328-330), page not
supplied; "the M8's tables cannot" (L336), an absence claim the sections read do not
contradict; crate compatibility (L602-603); the RON/JSON test claim (L412-414).

## Blockers

### B1. "Anchors" and "keyframe" are undefined (L388, L820, L933-938)

L388 says `t` runs from 0 to 1 across the whole selection. Issue 13 says a segment between
anchors runs from 0 to 1, and `curve.rs` does "keyframe interpolation". Neither term is
defined. A reader must guess whether interior non-empty cells are anchors, whether they
survive the ramp, whether bias and gain restart per segment, and what `r` (L694) swaps with
more than two anchors. CUJ 2 covers only two endpoints. This decides what G5 does, and G5
is one of the three things justifying the project (L47).

### B2. The rest of the sweep contract is incomplete (L69-72, L387-390, L692-695, L944-947)

*   **Neutral values:** initial bias and gain are never stated; 0.5 can only be inferred.
*   **Curve formulas:** given only as "Schlick's", with no formula or link. The Issue 13
    vectors match the standard rational forms, but a reader can only recover the
    definition by fitting them.
*   **Rounding:** no mode stated. The only evidence is 63.5 becoming `40` in the gain vector.
*   **Selection shapes:** one cell (`t = i/(n-1)` divides by zero), multiple columns including
    `V`, selections crossing note and FX columns, and which columns count as value columns.
*   **Vocabulary:** G5 and CUJ 2 say "onset and S-shape", elsewhere bias and gain, with no
    stated mapping.

### B3. Editing grammar semantics are not specified (L44-46, L361-362, L675-680, L730, L910-915)

Issue 9 requires table-driven tests such as `3>ib` and `d4j`, but their expected text
cannot be derived from the document.

*   **`d`, `y`, `>` plus a motion:** does `dj` act on the cursor column, the row or the track?
    `ib` and `ic` raise the same question.
*   **Counts:** is `3>ib` three semitones or three beats? Is `3dd` three rows?
*   **`p`:** L361 lists it as an operator; L679 says paste at the cursor. Do empty source
    cells overwrite? What happens at a phrase boundary or on a column mismatch?
*   **Default register:** behaviour without `"a` is unstated.
*   **`=`:** a general operator at L361, Visual-only at L691.
*   **`>` and `+`:** behaviour on `---`, `OFF`, `--`, INST and FX-name columns, and the note
    clamp range (L915), are unstated.
*   **`.` on a selection:** CUJ 2 and Issue 15 need it; the key table lists `.` only in Normal
    and Issue 10 defines it "at the cursor".
*   **Other views:** grammar in chain, song, instrument, table and groove views is absent.

### B4. Views, item creation and the initial project are unspecified (L139-143, L164-168, L256, L368-371, L739-740, L897, L916-924)

*   **New project:** not stated whether it has a song row, chain or phrase, so Issue 7's
    phrase view has nothing defined to show.
*   **Phrase view:** one track or several? `W B` implies other tracks are reachable, and Issue
    17 says "single-track". CUJ 1 needs several tracks, but Issue 7 ships without `W B`.
    Issue 11 needs CUJ 5's "scrolls or folds", and folding arrives in Issue 21, after G1.
*   **Creating items:** nothing creates a chain, phrase, table, groove or instrument. There
    is no ID rule for creation or cloning, no way to delete an item (though Issue 6 tests
    phrase removal), and `gd` on an empty cell is undefined.
*   **Instruments:** "base settings" has no fields, ranges or defaults. "Current instrument"
    and the initial "last-used instrument" have no selection mechanism.
*   **No way to edit:** track speed, row length, repeat count, Reset or Carry, row tempo, row
    mutes and table speed.

### B5. Table column mechanics make CUJ 3 unbuildable as written (L157-160, L325-336, L643, L756-758, L960-965)

*   L327-328 gives transpose and volume columns their own playhead, rate and loop point, but
    they hold only a number, and `HOP` and `TIC` act on "this column". Nothing can loop the
    transpose column, so CUJ 3's jump back does not loop the arpeggio.
*   End-of-table behaviour is unstated. The M8 loops by default (`m8.txt` L1336-1338).
*   L335 says tables can test whether a note is held, but no command does so and Issue 18 has
    no test for it.
*   L325 ("from row 0") and L332 (per-note columns keep position) conflict with no exception.
*   Aux tables are undefined: how many, how their transpose and volume combine, default rate.

### B6. Mid-song equivalence has no defined mechanism (L192-193, L315-318, L970-971)

CUJ 7 and Issue 20 require matching "the samples from playing through", but the function
computes positions only. Equality would also need held FX values, `REP` accumulation,
`TIC00` positions, `IFC` counts, RNG state, ringing voices, delay and reverb tails, and
smoother state. The document neither explains how nor narrows the requirement.

## Major

*   **M1. "Tick" and "row" are overloaded (L255, L298-302, L323, L758, L762-764).** "Tick" is
    1/960 of a quarter note, a 40-tick groove unit, and an unqualified `xx` in `DEL`, `KIL`
    and `OFF`. `DEL FF` is about one row here and about 42 rows on the M8. "Row" means song,
    phrase, table and normal-speed row. It never says one row is a sixteenth note.
*   **M2. FX values cannot express their targets (L260, L759-760, L756).** `TBL`, `TBX`, `GRV`
    and `THO` take two hex digits, against 32-bit IDs and any-length tables. `HOP xy` reaches
    rows 0-F, and `HOPFF` overlaps `x=F, y=F`. `TBX00` makes table `00` unusable as aux.
*   **M3. Incomplete FX semantics (L758-769).** `RET` step size and `y=0`; `REP` signed encoding
    and no-previous-value case; `RND` units and distribution; `CHA` scope and RNG policy;
    `IFC` base and `y=0`; edge values like `KIL00`; ranges for `RES`, `DLY` and `REV`.
*   **M4. "Holds" and volume combination (L343-345, L648, L754, L770).** No rule for when a held
    value resets, or whether it is per slot or track. How cell, table, FX and instrument volume
    combine, what `--` is unchanged from, and how transpose stacks are all unstated.
*   **M5. Song schema and save format (L247-257, L653-654).** No field list. Missing: project
    seed, song tempo (`:bpm` has no target), scale and root, step, `format_version`, generator
    parameters and ownership encoding. Chain and song rows have no text format. Rows with no
    chain and groove edge cases are unstated.
*   **M6. Seeds (L85, L181, L340-342, L749).** `render` has no `--seed` and no `:seed` exists.
    Seed selection and PCG32 derivation are unstated. L341 uses list position, so `:track del`
    shifts other tracks' randomness, contrary to the stated intent.
*   **M7. Render output (L84-86, L420-421, L745, L749, L845, L950-952).** WAV format, default
    length, tail handling, `:render` rate, existing-file behaviour, and hash algorithm and
    golden format are unspecified. `hound` is listed for `vt-model`, but `vt-engine/src/render.rs`
    writes WAV.
*   **M8. Playback and transport (L307-313, L686-688).** After the last song row; whether phrase
    scope plays other tracks; "resuming ... from its start" conflicts, and the `gmuse` spec
    separates pause from return; `Enter` while playing; scope when the view changes; default row
    length has no rounding (7 rows at 3/4 is 9.33 normal-speed rows).
*   **M9. Insert mode (L696-703, L731-732).** Default step and octave; how the sub-mode is chosen;
    cursor movement; when value entry completes; FX validation; wide INST digits; note preview's
    track, voice behaviour and path. `Esc` from Visual and Command-line keys are missing.
*   **M10. `gc` and jumps (L164-168, L363-364, L683-684).** No rule says cloning repoints the use
    you jumped from, or covers phrases reached by picker or `gc` on a chain cell. Jump list
    contents and `grr`'s UI are unstated. The picker's chain or phrase page does not say which
    item it opens.
*   **M11. Commands have no error behaviour (L738-745, L749).** `:q` and `:e` with unsaved changes,
    `:w` with no project, launch with no argument, `:new` on an existing folder, `:sample` errors,
    `:track del` on the last track, `:len 0` and shrink, invalid `:bpm`, and what is undoable.
*   **M12. Commands assigned to no issue (L738-745, L897-899, L913, L918-919, L929, L933).** `:bpm`,
    `:set step`, `:set scale`, `:len`, Insert value entry for `VOL` and `INST`, and `gv`.
*   **M13. Generator re-entry (L198-200, L91-93, L685, L941, L946).** `gv` only reaches the last
    selection, though returning is called a requirement. "`apply` removes any link whose owned
    cells it changes" would delete the link on its own rerender. Partial overlap, overlapping
    links and `:len` removal are unstated, and owned ranges have no display marker.
*   **M14. G1 pass criterion (L50-51, L55-58, L953-956).** "Clearly better" has no threshold, and
    G1 runs without tables or the score view despite the "together" claim. Issue 17 does not say
    how the single-track note affects a pass.
*   **M15. Dropout and CPU metrics (L83, L228, L432, L900, L1002-1003).** "A block that runs late"
    has no clock or threshold, CPU load has no formula, and "no audible dropouts" differs from a
    zero counter.
*   **M16. Audio defaults (L349-353, L779-783, L825).** "Limiter" vs `tanh` clip. Sample end and
    looping, resampling method, pan law, smoother and envelope shapes, resonance range, delay wet
    level and reverb settings are unspecified.
*   **M17. Serde dependency (L607, L876).** `BTreeMap<u32, Arc<T>>` serialized directly needs serde's
    `rc` feature, to the reviewer's knowledge; only `derive` is listed. Not tested.
*   **M18. Types at the crate boundary (L831, L891-894).** `SamplePool`, `Engine`, `EngineHandle`, the
    command type and telemetry schema have no shape or home. Version-change rules omit a sample
    removed or replaced while its voice plays.

## Nit

*   **N1. Outside terms lack definitions or links:** tracker, Buzz, Renoise, OpenMPT, Elektron,
    parameter locks, `A:B`, `RESET`/`INF`, Polyend Fill, stage sequencer, Euclidean, Schlick,
    `basedrop`, Snacks, which-key, Freeverb, Kitty flags, Sixel. `gmuse` has no link or path, and
    the author's keymap rules and transport reference are not cited.
*   **N2. L722 vs L662:** `gT` is a direction partner, which the stated rule forbids. L723's "no key
    is both" also drops the guide's nuance about what happens when the timeout wins.
*   **N3. L337-339 vs L761:** `GRV` is in the FX table but not the first command set.
*   **N4. L121-122, L681, L872:** `probe-keys` has no decision rule or fallbacks except for `Ctrl-A`
    and `Ctrl-X`.
*   **N5. L791 vs L798, L800:** files "created by the issue that names them" already exist.
*   **N6. L743, L747-748, L822:** scale rules omit the root format and default, which pentatonic and
    minor, top-of-range snapping, and whether Insert notes snap.
*   **N7. L662, L710, L422:** `3gt` numbering changes as pages arrive. The counting allocator does not
    say how it scopes to the callback thread.

## Checked and accurate

*   Bias and gain test vectors match the standard rational formulas.
*   M8 equal-length advice (`m8.txt` L762-766) and 6 units per row (`m8.txt` L1438-1441).
*   Digitakt `RESET` `INF` (`dt2.txt` L2424-2428) and seven track speeds (`dt2.txt` L2408-2411).
*   Polyend license restriction on paid direct ports (`LICENSE` L64-76).
*   The cited fill source file exists.
