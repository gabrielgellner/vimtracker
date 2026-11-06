# Stage 9: Implementation readiness

Target: `docs/designs/vimtracker.md`, lines 741 to end, focused on "Milestones after M3" (lines 1033-1044) and read with "Direction" (lines 518-619). Reviewers: implementation readiness (I), codebase details (K) and style (Y). All three read statically; nothing was compiled or run.

**Correction to these reports:** I-N1, K-B1 and Y1 say line 1035's "Filters milestone" and "Issue 50" point at nothing. They do exist, on GitHub: milestone 4 "Filters" holds open issue #50, "Diode and MS-20 style filters". The reviewers could only see the repository. The real defect is that the document doesn't say this lives on GitHub.

## Implementation readiness

**Verdict: not ready. M4-M10 fail the zero-decision rule.** Line 743 exempts everything after M3 from the readiness bar, so the plan doesn't claim otherwise. Either extend the bar and split the milestones into issues, or state that each needs its own design first.

### Blockers

**I-B1. M4-M10 have no executable work breakdown.** Lines 743, 1035-1043. No issues, file targets, types, or action, command and key lists. Target Files (939-998) predates current code.

**I-B2. The entry gate contradicts itself.** Lines 73-84, 522, 607-608, 730, 743, 1035. Line 743 stops work at Issue 23 until G1 is recorded, line 730 has G1 choose what's next, and line 607 defers G1 until a full song can be made. An implementer can't tell whether M4 may start, what Stop or Redirect does to M7-M10, or whether Issues 20-22 and Known Gaps must close first.

**I-B3. M7 depends on M8.** Lines 555, 562, 1040-1041. M7 tests "a track's steps sequence its effects", and line 555 defines a send as having FX slots. The slot container, effect selection and settings page are in M8. Decision 1 requires only that sections come before effects, so reordering doesn't conflict with it.

**I-B4. A parameter's meaning depends on the track, but phrases are shared across tracks.** Lines 257, 543, 558, 579, 933. Undefined: a shared phrase's `DEC40` on a track with no `DEC`, pasting such a slot, changing a machine with existing values, what `K` lists for a phrase with several users, whether an unknown name is still a load error, and whether two machines can share a three-letter name. This fixes the file format, so it must precede M5.

**I-B5. The parameter list has no schema, and current settings don't fit one u8.** Line 543, M5; `step.rs:130-133`. "Any parameter" must include `Rate` (enum with typed forms such as `2.5hz`), `depth` (`i16`), phase in degrees, envelope times up to 8000 ms, sustain, swing in tenths of a percent, and `FilterType`. Undefined: the descriptor type, the mapping from two hex digits to each range, which parameters can't be locked and which can be modulated. Locking `Rate` or `Attack` has nowhere to live, as LFOs and envelopes read directly from the song's `Track` (`track_state.rs:229, 259, 289`). `offsets` is indexed by `Destination::ALL` (`track_state.rs:123`), so open modulation needs a declared maximum and a real-time-safe lookup.

**I-B6. Lock, hold and glide are incomplete for the engine.** Lines 587-603, 1038; `track_state.rs:366`.
*   Slots naming one parameter: "next value in that slot" gives per-slot state, while Layers (601) implies one lock per parameter. This decides lock storage.
*   What counts as the next note: a failed `CHA`, a `DEL` note, `RET`, `OFF`, a swung step.
*   Glide target: whether a value naming a different command counts, and the scope of wrapping across chains, transpose, sections and `:reset`.
*   Glide state: which layer it writes, what happens on landing, and what a note does during a glide.
*   Glide timing: swing, tempo changes, "lands exactly" against `SMOOTHING_MS`, and finding the target without an unbounded scan of up to 4096 steps in the callback.
*   Transport and audition: pause, resume, `Space c` mid-phrase, and `;`.

**I-B7. Real-time storage for machines and effects is unspecified.** Lines 539, 562; `track_state.rs:78`; `engine.rs:54, 71-72`. Decide edit-thread allocation inside the published version or a bounded pool, the behaviour when a pool is exhausted, and how effect state survives a new published version.

**I-B8. M7's routing model is undefined.** Lines 549-558, 1040.
*   Ordering: how routes are stored given an index list where only the last track can be removed, and how the order is computed without allocating.
*   Mix track: mandatory or not, whether it counts against `MAX_TRACKS`, `:track del`, and G5's 1, 8 and 64 matrix.
*   Sends: how many per track, and what happens to `Track.delay`, `Track.reverb`, `DLY`, `REV`, `Destination::Dly` and `Rev`.
*   Feedback: refused on edit, refused on load, or a one-block delay. Sidechain adds an edge to the same graph.
*   Also mute and solo on buses (`any_solo` is flat, `engine.rs:352`), notes and `Env 1` with no note, choosing a track kind, and the default project.

### Major

**I-M1. M4's `Sound` boundary is undefined.** Lines 527, 536, 1037. Which `Track` fields move, file formats and locations, sample paths, undo granularity, loading a sound during playback. Line 536 redefines kits with no replacement for labels (250-252) or `:kit` behaviour.

**I-M2. M4 has no removal list.** Needs a stated fate for `Song.instruments`, `Step.instrument` and `INST`, `:inst`, `:label`, `:sample`, `Space i K`, instrument-column `K`, Insert instrument entry, `TrackState.instrument`, the preview voice, `;`, `views/instrument.rs`, `instruments.rs`, `guides/instruments.md` and the gain formula (385). Sample browser behaviour is absent too.

**I-M3. Format version and existing-project policy aren't stated.** `FORMAT_VERSION` is 1. M4 removes a step field and M5 adds marks. Say "no conversion" explicitly. `steptext` changes, goldens get re-blessed, and default-lock changes how existing values sound.

**I-M4. The stem definition is ambiguous.** Lines 537, 1037, 1039. "Without sends" has two readings. Undefined: where wet signal goes, file naming, mute and solo (`render.rs:56-57`), the limiter, one render or per-track, equality or tolerance, `--tail`, `--rows`. After M7, a mix compressor makes "still sum" false.

**I-M5. M6 lacks transport and section rules.** Lines 260-263, 294, 735, 1039. End of song; section length and its storage; how reset and carry relate to `Section.reset` and `:reset`; carry across different chain lengths; swing pairs, tails, locks, holds and LFOs at a boundary; empty chains; which section the phrase view and playhead use; song view keys; queueing; a whole-song render has no single loop to warm up with. Line 608's "song and score views move up" doesn't say if per-track speed and `GRV` come along.

**I-M6. Pages are incomplete and collide with keys.** Lines 566, 1038. `H` and `L` are `ViewTop` and `ViewBottom` (`bindings.rs:136-138`), against line 882's rule. No assignment of controls to Sound, Mod and FX, cursor retention, or layout metadata for the envelope rows and diagrams.

**I-M7. Mark editing and display details conflict or are absent.** Lines 587-603, 790. Line 603 against the example; the persisted form against the fixed text format at 787-796; `~` on an event, empty slot or Visual range; whether "nearest above" wraps; `>` and shape panel writes; whether `dd`, `y` and `p` carry marks.

**I-M8. Direction items assigned to no milestone.** Shape panel glide (585), per-track slot count (582), mixer overview (538), saving a sound alone (536), character delay and granular (121, 562). M4 creates the need for the score view and M9 delivers it five milestones later.

**I-M9. "Slot" names two things.** Lines 61, 560-562, 579-582, 615, 1038, 1041. Also whether step locks follow the effect or the position on reorder, and how reorder declicks.

**I-M10. Test plans are too thin.** M9 and M10 have none. M4 and M5 have only headline scenarios. "Idle cost is measured" has no threshold, "off costing nothing" no definition. The real-time guard and track matrix aren't restated for new paths. Line 1035's exit can't fail.

**I-M11. M10 machines are underspecified.** Lines 534, 539, 733, 1043. How the slicer picks slices without sample locks, how slices are stored, `:bpm fit`, and whether M4's singular `voice` and `tail` force a redesign. "Plaits-style" needs a list of voices and an attribution note.

### Nits

*   **I-N1.** Line 1035's clause. See the correction above.
*   **I-N2.** Superseded text with no marker: 856, G4, CUJ 4, Persistence column (921-926), lines 133 and 135, Issue 20.
*   **I-N3.** Line 1040's "a better reverb" is weaker than line 562. Specify delay settings there.
*   **I-N4.** Line 536: track names have no length, charset, uniqueness rule, command or display location.
*   **I-N5.** Line 583 relies on `MAX_PHRASE_STEPS` without stating it. Line 608's "smaller items" conflates After G1 marks with Direction's marks.

## Codebase details

**Verdict: not accurate enough yet for M4 onward.** Most problems are stale statements about the code, and the Direction section is mostly sound as design. `app.rs`, `tree.rs`, `action.rs`, `selection.rs`, `voice.rs` and `picking.rs` were checked by search or partial read.

### Blockers

**K-B1. Line 1035's Filters clause.** See the correction above.

**K-B2. Line 520's "what is built today" is false, and Target Files (939-997) has drifted.**

| Crate | Listed but absent | Present but unlisted |
|---|---|---|
| `vt-model` | `curve.rs`, `generator.rs` | `shape.rs`, `envelope.rs`, `lfo.rs`, `kit.rs`, `label.rs` |
| `vt-engine` | `playhead.rs`, `fx.rs` | `published.rs`, `dsp/lfo.rs` |
| `vt-tui` | `shape.rs` (it's `shaping.rs`), `views/scope.rs` (it's `meter.rs`, `views/meter.rs`, `views/braille.rs`) | `track_page.rs`, `views/track.rs`, `grammar.rs`, `tree.rs`, `picking.rs`, `instruments.rs`, `run.rs`, `autosave.rs`, `theme.rs`, `guides.rs`, `guides/*.md`, `views/guide.rs`, `views/system.rs`, `views/shape.rs` |

Line 978 describes `mixer.rs` as track mix and sends, but it holds only `master` and `Reading`; mixing and sends are in `Engine::render` (`engine.rs:740-778`). Line 973 promises "per-track position", which doesn't exist. Line 982's goldens are `*.hash`, not `*.txt`.

**K-B3. Sound statements contradict the code.**
*   Lines 935 and 1018 give attack and release as instrument settings. `Instrument` holds only `sample`, `volume` and `labelled` (`song.rs:336-343`).
*   Line 806 gives `Space 1`-`Space 6`. The code binds `Space 2` to `Space 5`, and `Space 4` is the track page, which the table doesn't list (`bindings.rs:158-165`).
*   Lines 850-856, 1025 and 1027 describe onset, S-shape and live sweeps. The built panel is `FillKind`, `Where` and `ShapeColumn` (`shape.rs:10-54`).

### Major

**K-M1. M4 lists a sample browser as new, but one exists.** `find_wavs` and `Pick::Sample` (`picking.rs:21-53, 344, 392`). What's new is a `Space 5` view assigning to a track.

**K-M2. `Sound` doesn't say what moves, and most of the sound is already on `Track`.** `song.rs:75-114`. Only `sample` and `volume` are left to absorb. `Field::Instrument` is column group 2, so FX column indices shift (`selection.rs:12-54`). The preview voice bypasses the track chain (`engine.rs:767`), so a previewed note won't sound like the track.

**K-M3. M6's "carrying track positions" has no position to carry.** One global `Engine.place` (`engine.rs:59-61`), and each track derives `place % chain length` (`engine.rs:602`). A section change with different lengths jumps. `Section.reset` is a periodic realign (`song.rs:154-156`), not a choice on entering a section, and render treats it as a length (`render.rs:48`).

**K-M4. M6's real work is removing first-section assumptions.** `engine.rs:150, 161, 596, 701`; `render.rs:41`; `tree.rs:35, 163, 184`; `views/phrase.rs:86, 123`; `views/status.rs:108`; `app.rs:262, 288, 781`; `action.rs:636, 663, 680`. `:reset` overwrites every section (`action.rs:495`). `tree.rs:163, 184` would undercount chains used later.

**K-M5. Stem tests can fail for reasons the document doesn't address.** The limiter clips (`mixer.rs:14-23`), so test with it off. `append_tail` cuts at each engine's own silence (`render.rs:72-90`), giving files of different lengths. `render_loop` applies saved mute and solo (`render.rs:55-58`). Goldens hash exact bits, so a float sum needs a tolerance. Decide whether stems are dry or wet: a solo render carries its own wet signal, and the effects look linear, so it might sum.

**K-M6. "Every parameter works with each key from the list alone" doesn't fit.** `Rate` has its own stepping and parser (`lfo.rs:46-102`), `Swing` is `Option<u16>` with a fallback (`track_page.rs:246-255`), envelope times step by value (`track_page.rs:725-731`), and `K` deliberately refuses numeric fields (`track_page.rs:439-442`).

**K-M7. Lock and hold layers don't exist.** Base and held share one target (`track_state.rs:211-222, 366-378`). A page edit during playback overwrites a held value (`engine.rs:181-199`). `VOL` has no page base, as it resets to unity and multiplies a separate `level` (`track_state.rs:215, 501`).

**K-M8. "It needs only to learn glide" understates the job.** Line 585. `fire_track` reads one step (`engine.rs:605`), and nothing looks ahead or ramps over steps. The mark must also go on `FxSlot`, in step text (`steptext.rs:79-112`) and at `shape.rs:119`.

**K-M9. Per-track effect state doesn't fit `TrackState`.** It derives `Copy`, while `Delay` and `Reverb` own `Vec`s. `Engine::render` skips any track that isn't `sounding()` (`engine.rs:749`), so a bus would never be processed. Tracks run in index order with one shared send pair (`engine.rs:71-72, 762-771`), so M7 replaces this loop. `Voice` plays samples directly (`engine.rs:752`).

### Nits

*   **K-N1.** Two meanings of "slots", and M5 creates an FX page with no content until M8.
*   **K-N2.** `H` and `L` fall through on the track page (`track_page.rs:311-314`), so M5 needs an intercept and a table entry.
*   **K-N3.** Line 562's delay "needs" ping-pong, but feedback already cross-couples channels (`delay.rs:29`). What's missing is making it a setting.
*   **K-N4.** Line 573: `Phrase::rotate` moves whole steps (`song.rs:317-323`), so a value column can't rotate apart from its notes.
*   **K-N5.** Line 1056's "the M8" reads as the milestone. Line 531 says modulation reaches "any of these", but `Destination` has no drive or utility-filter target (`lfo.rs:220-229`). Line 566's "about 24 rows" is about 21 before chrome.

### Code to reuse

*   **M4:** `find_wavs` and `Pick::Sample`; `kits_in`, `open_kit_picker`, `load_named_kit` (`picking.rs:56, 126, 143`); `KitSound` (`kit.rs:18-25`), `load_kit`, `resolve`; `Action::SetSamples` for one-undo loads (`action.rs:176`); `is_label` for track names (`label.rs:26`); `Published::new` to resolve samples off the audio thread (`published.rs:20-34`); `import_sample` (`project.rs:133`); `render_loop`, `set_solo`, `write_wav` for stems; the golden helper and `VT_BLESS` (`tests/render.rs:56-84`); the `FORMAT_VERSION` refusal (`project.rs:63-69`).
*   **M5:** replace the parallel lists: `Control` and its `label`, `is_hex`, `is_choice` (`track_page.rs:55-167`), the matches at `track_page.rs:225-234, 327-336` and `views/track.rs:28-77`, `meaning` (`track_page.rs:712`), `FxCommand` metadata (`step.rs:60-99`), `Destination::ALL`. Keep `next_in`, `adjust_lfo`, `adjust_envelope`, `parse_depth`, `Rate`'s parser and `reset_control`'s single undo step. `Where` and `ShapeColumn` remain the fill logic. `Vec<FxSlot>` already allows a variable count, but shared phrases need a rule.
*   **M6:** `Section.reset`, `section_steps`, `Engine::realign`, `past_reset`, `chain_position`.
*   **M7, M8:** `Delay`, `Reverb`, `ShapedFrame`, `master`, `Reading`; the bypass in `TrackState::shape` and `trimmed` (`track_state.rs:411-419, 476-484`); `update_gains` (`track_state.rs:316`).
*   **M9:** neighbour column rendering in `views/phrase.rs:123` (not read in detail).

## Style

**Verdict: not ready.** No style standard was supplied, so this used the persona's audit vectors. 1 Blocker, 9 Major, 7 Nit.

### Focus: Milestones after M3

*   **Y1 (Blocker).** Line 1035's Filters clause. See the correction above.
*   **Y2 (Major).** "M8" is a milestone and the Dirtywave M8 (75, 1041, 1056).
*   **Y3 (Major).** Lines 608, 743 and 1033 state the sequence inconsistently. Line 743 says the rest is under After G1, and M4-M10 are elsewhere.
*   **Y4 (Major).** Line 1035: "it" has no clear referent, "used" has no threshold, and "jam" is undefined.
*   **Y5 (Major).** Lines 1041 and 1043: "wanted in a jam" leaves M8 and M10 with no defined finish.
*   **Y6 (Major).** Lines 1038, 1041, 776: "slots" and "FX" name different things.
*   **Y7 (Major).** Lines 1037-1038: "sound" and "marks" are overloaded.
*   **Y8 (Major).** Lines 1040-1041: "real settings" and "a better reverb" are judgments; idle cost has no limit; "off costing nothing" has no method.
*   **Y9 (Major).** Lines 1037-1039: "without sends" is ambiguous, "still sum" drops its target, and "each key" names no keys.
*   **Y10 (Major).** M9 and M10 have no tests, with no explanation. M4's list mixes grammatical forms.
*   **Y11 (Nit).** "From Direction." is a fragment.
*   **Y12 (Nit).** "Renders stay repeatable" is looser than line 104's wording.
*   **Y13 (Nit).** "What's underneath" (1038), "when one needs it" and "Plaits-style" (1043) against "after Plaits" (534), and line 1042 gives no keys where line 734 has them.

### Rest of the range

*   **Y14 (Major).** Lines 790, 794, 921: `VOL` and `VEL` for the same column, and `VOL` for an FX command.
*   **Y15 (Major).** Lines 844-846, 1010, 1016, 1052: the key table uses `Tab`, `Shift-Tab` and `Space c`; Issue 8 and line 1052 use `Enter` and `Space x`.
*   **Y16 (Nit).** `x` and `K` each appear twice in the Normal table (824/835, 828/836). `+` and `-` against `=` and `-` (833-834, 876, 1015).
*   **Y17 (Nit).** Vague phrases at lines 749, 743, 1018, 1031 and 1059.
