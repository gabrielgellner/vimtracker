# Song flow and session workflow

**Status:** proposed on 2026-11-06, not yet built, with its choices approved by the author after a review. **Milestones:** M8d, then M8c, between M8b and M9 in [the main design](vimtracker.md). **Builds on:** that document's M8 and M8b designs and Decisions 10 to 13.

**Replaces:** the main design's carry rule and default, launches waiting for a chain's end, its limits on playheads, and `gd` on `--` opening a chain. Where the two documents differ, this one wins. Tests that pin the old behaviour change with it, such as `tests/sections.rs:116`, which pins carry as the default.

vimtracker is a keyboard-driven terminal tracker for making full songs at about a Maschine's scale. It isn't a DAW. Detailed arrangement happens in Ableton, from exported stems. There's one author and no compatibility promise, so saved songs can change format freely.

## The model

The song view already works as a restricted version of Ableton's Session View:

| vimtracker | Closest Ableton idea | Notes |
|---|---|---|
| Song | Session grid | There's no separate linear Arrangement |
| Row, called a section in code | Scene | It has a length and a start rule |
| Cell | Clip slot | `--` is empty |
| Chain | Clip | What's launched, and one plays per track |
| Phrase | Section of a clip | Chains can reuse it and transpose it, which clips don't offer |

Each track owns its chains and phrases. A chain is a list of phrases, each with a transpose.

### Terms

*   **Running count (`place`):** the engine's step count, used for swing and `:reset`. `:reset` and `Start` `reset` set it to 0.
*   **Row step (`section_place`):** how far the current row has played, counted from its start across its repeats. Only starting a row sets it back to 0.
*   **Chain position:** where a track is in its chain.
*   **Part:** a musical idea on one track, such as a bassline, played by one chain.
*   **Empty row:** a row with no chains and no set length. It's skipped.
*   **Self-loop:** a row playing again after itself, through `:loop`, or because it's the song's only non-empty row.
*   **Wrap:** moving from the last non-empty row to the first, when they differ.
*   **`Start` `reset` and `:reset N`:** two different settings on a row. See M8c.
*   **The Dirtywave M8:** the hardware tracker, not milestones M8, M8b, M8c or M8d. This document calls it "the Dirtywave M8" throughout.

## The problem

1.  **A chain can enter in its middle.** Under the current `carry`, a chain position is the running count modulo the chain's length. A 32-step chain entering at count 16 starts at its step 16, not as written.
2.  **Launches follow a different rule.** A launched chain plays from its own start, measured from the count when it started.
3.  **Repeating a row means copying it.** Playing a row four times means making it four times as long or copying it.
4.  **Launches are slow.** A launch waits for the track's chain to end, which on a long chain can be many bars.
5.  **Two levels are a chore for one loop.** In the author's experience, most parts in a jam are one phrase, yet you pass through the chain view to reach it.
6.  **Numbers don't say what a part is.** A jam depends on knowing what you're about to launch, and `03` doesn't tell you.
7.  **Variations are risky.** Editing a phrase changes everywhere it's used, so a quick variation means copying first, which the song view has no key for.

## Goals

*   A chain plays from its top when a row brings it in, unless the row keeps it from the row before.
*   One rule covers rows and launches.
*   A part can span rows without a timeline.
*   A row's repeat count is one column in the song view's grid.
*   Launches can be timed on a beat or bar grid.
*   `gd` from a cell reaches a one-phrase chain's phrase in one step.
*   The chain under the cursor shows its label, and can be copied from the song view.
*   A render always ends, and all stems of a render line up.

## Non-goals

*   A linear timeline, or clips placed freely over rows.
*   Time signatures in the model. See Decision 12 in the main design.
*   Jumps between rows and chance on rows for now. See "Later".
*   Actions that move one track through its chains on its own.
*   Per-track speed and grooves, which stay in After the milestones.

## M8d: session workflow

This comes first. It's cheaper, it needs nothing from M8c, and playing with it informs M8c's choices.

**Data.** In `vt-model`, `Song` gains `launch`, saved and undoable like other song settings. `Chain` gains an optional `label`. A new constant, `BAR_STEPS`, is 16, joins the main design's constants table, and replaces the local `LARGE_LENGTH` in `song_view.rs`. In `vt-engine`, `start_cued` tests the new setting.

### Launch timing

`:launch` takes `step`, `1/4` (4 steps), `1/2` (8), `1bar` (16), `2bar`, `4bar`, `8bar`, a step count, or `chain`. The default is `1bar`. A bare `:launch` shows the value, and a bad argument gets an error like `:reset`'s.

A cued launch or stop happens on the next step whose row step is a multiple of that length. Every row start is therefore a grid line. A line past the row's end doesn't arrive, so `1bar` on a 12-step row fires only at its start. `:reset` doesn't touch the row step, so it doesn't move the grid.

`chain` is the current behaviour, waiting for the track's own chain to end, and a track playing nothing fires on the next step. It's also the [Dirtywave M8's default `LIVE QUANTIZE`](https://cdn.shopify.com/s/files/1/0455/0485/6229/files/m8_operation_manual_v20230630.pdf). The grid belongs to the row rather than the chain, so a chain shorter than the grid loops until the line, and a longer one is cut there. A pending cue uses a new `:launch` value at once.

### Launching rules

*   A launched chain always plays from its top. This uses the existing `Launched.started` (`engine.rs:233`).
*   Launching the chain a track is already playing restarts it from its top at the next launch time. It needs no special case, as the cue simply fires.
*   The first launch holds the song on the playing row, as `:loop` does, which is unchanged from M8b.
*   Turning `:loop` off hands back to the song. The row plays to its end, then moves on.
*   A different row starting ends running launches, but a pending cue survives and fires on the new row's first step, whatever `:launch` says.

Within one step, the order is: row change and its `Start`, then `:reset N`, then cues, then notes.

### Capture

`C` adds a row holding what each track is playing. The row copies the source row's `length`, `start` and `:reset N`, so realigning carries on unchanged. Capture switches the engine's current row without running its `Start`, so no chain position changes and nothing audible changes at that moment.

### One-phrase chains

*   `gd` on `--` adds a chain holding a new phrase and opens the phrase, as one undo.
*   `gd` on a chain with one phrase opens that phrase.
*   `gd` on a chain with no phrases adds one and opens it.
*   `gd` on a chain with several opens the chain view, as it does now.
*   `Space 2` already opens the chain view from a phrase, and `C-o` jumps back.
*   `gc` in the phrase still repoints the chain's slot, so it changes every row using that chain. `gC` makes a variation for just this cell, and works from the phrase view.

### Labels

The number stays a chain's identity, and what rows and saved songs refer to. A label needn't be unique. `:label` sets it on the chain under the cursor, in the song and chain views, with any text up to 32 characters, and is undoable. `:label` alone clears it. It's refused on `--` and outside a track's column. `:name` already sets a track's name, so it isn't reused, and `:label` itself was retired in M5 so is free.

Song view cells don't change, as `TRACK_WIDTH` is 4. The status line shows the label of the chain under the cursor. `K` on a cell opens a picker of the track's chains and `--`, searchable by number and label, and choosing one sets the cell, as one undo. Wider cells can come later if a jam needs them.

### Cloning

`gC` in the song view copies the cell's chain, its label and its phrases, and points the cell at the copy, as one undo. Other cells keep the original. It reuses `Action::CloneChain`, in a song view handler that uses the column's track rather than `cursor.track`. It's refused on `--` and outside a track's column.

### Not included

*   **A launch timing per chain:** more to manage mid-jam, for little gain over one setting.
*   **Launching on the next phrase:** waits until the grid has been played.
*   **Legato launches:** go with `carry`, below.
*   **Whole-row launches and a "leave this track alone" cell:** wait for evidence from play.
*   **Labels on phrases:** chains first.

### Tests

Each `:launch` value, a stop on the grid, the grid unmoved by `:reset`, a chain shorter and longer than the grid, a 12-step row, launching the playing chain, a cue surviving a row change, the same-step order, capture copying `:reset N` and staying inaudible, each case of `gd`, labels in the status line, the picker finding labels and setting a cell, each refusal, a clone from the right track leaving other cells and phrases alone, one undo for a clone, and no allocation in the callback.

## M8c: song flow

**Data.** In `vt-model`, `Section` gains `repeat`, a `u16` of at least 1, default 1. Loading rejects 0, as actions reject a zero length, and the engine treats it as 1. `SectionStart` becomes `New`, the default, and `Reset`. In `vt-engine`, each track gains `started`, and `Launched` drops its own. `JamState` gains `started`.

### One position rule

Each track remembers `started`, the running count at which its chain began. Its chain position is `place - started`, saturating at 0, modulo the chain's length, and a chain with no length plays nothing. `started <= place` always holds, as anything that sets `place` sets `started` too, including `rewind`, `return_to_start` and `play_section`.

A row's `Start` column picks what happens when the row begins:

| Start | What it does |
|---|---|
| `new`, the default | A track whose chain changes starts it from its top, so `started = place`. A track whose chain is the same keeps going. |
| `reset` | `place` and every track's `started` go to 0, each time the row's length is reached, including on repeats and self-loops. So `Rpt 4` plays four identical passes. |

A track's chain changes when the chain the row assigns differs from the one the track was playing, including a launched chain. Going from `--` to a chain counts as a change. Going from a chain to `--` silences the track without cutting what's ringing, as it does now.

So the same chain in neighbouring rows runs across the boundary, which is how a part spans rows. Those rows need set lengths, since a row without one lasts its longest chain's full length. Example: a 64-step chain on two rows of length 32 plays through once.

### Starting and wrapping

*   **Playing from a row:** this walks the rows above it, from the top or the last row with `Start` `reset` or `:reset N`, to find when each track's chain began, and sets `place` and `started` to match playing through. The walk is bounded by the row count and doesn't allocate.
*   **Playing from a chain step:** this sets that chain position on the cursor's track, with the row step as that step modulo the row's length. Other tracks follow the walk.
*   **A wrap:** every chain restarts from its top, with `started = place`. `place` keeps counting, so swing doesn't change. Held values, locks, glides, generators and tails carry over, as at any boundary.
*   **A self-loop with `Start` `new`:** `place` and `started` are unaffected, so short chains keep moving against long ones, and one-section golden renders must not change. Empty rows don't count, so adding one to a one-row song changes nothing.
*   **`:loop` moved to a different row:** that row starts, running its `Start` and ending launches.
*   **Changing a cell while its row plays:** the track keeps its chain position.

### `:reset N`

This works as it does now: when `place` reaches N, `place` and every `started` go to 0, including launched chains. `place` counts from the last reset rather than the row's start, so entering a row when `place` is already past its N resets on the row's first step.

### Repeat

The song view gains an `Rpt` column after `Length`, with the same keys: `=` and `-`, `+` and `_`, and `i` to type. `x` goes back to 1. `K` and `x` on `Start` follow `new` and `reset`.

There's no pass counter. A row ends when its row step reaches at least `length x repeat`, so lowering `repeat` while playing ends the row at the next step if it's already past. A hold or `:loop` overrides this, and the row loops after `length x repeat`. Undo and row changes follow the existing rules for the playing section. Capture sets `repeat` to 1.

### Launches meeting rows

When a different row ends a launch, its `Start` applies as usual, comparing the launched chain with the row's, so a launched chain matching the new row keeps going.

### Playheads

The phrase and chain views use one shared position, shown only when the row being edited is playing. Each track's position now differs. `JamState` carries `started` on the existing queue. A track's chain position on screen is the reported position minus `started`, by the rule above.

The two queues can disagree for a moment, and a full queue delays a report but doesn't lose it (`link.rs:195-207`), so a playhead may be briefly wrong. A view shows a playhead when the chain it's showing is the one the track is playing, whichever row the cursor is on. Read-only columns for other tracks use their own tracks' positions. This fixes M8b's limit of playheads following the row rather than a launched chain.

### Render

A render covers each non-empty row once in order, each for `length x repeat`, through `song_steps`. So it always ends, and no route needs working out. Its warm-up is the same length, and `rewind` restores `started`.

If the song has one non-empty row, it still renders as its loop: the row's `:reset N` if it has one, otherwise `length x repeat`. `--rows N` renders N steps, wrapping as playback does. `render_stems` only changes mute, solo and the limiter, none of which affect this, so stems line up.

### Tests

Each `Start` value, a chain spanning two rows, a different-length chain starting from its top, `reset` on each repeat, wrapping with swing unchanged, one section unchanged, an added empty row changing nothing, `:reset N` including entering past it, repeats, lowering `repeat` while playing, `gc` copying `repeat`, `:loop` overriding it, `:loop` to another row, playing from each row of a spanning part and from a chain step, playheads per track and on a launched chain, a launch ending into a matching row, a captured row reached later, render content matching a second pass, render length and stems with repeats, `--rows`, loading `repeat` 0, and no allocation in the callback.

## Risks and limits

| Risk or limit | Treatment |
|---|---|
| New logic in the audio callback | Only scalars are added and every walk is bounded by the row count, so nothing allocates. Both milestones test this. |
| A bad value in a song file | `repeat` 0 is rejected on load, and the engine treats it as 1. |
| A jam-state report delayed by a full queue | A playhead may be stale until the next report, and sound isn't affected. |
| A jam's phase is lost on capture | Chains launched on different bars are often out of phase, and this is lost when the captured row is reached in order, including in stems. It's an accepted cost of `1bar`, to judge in play. |
| A spanning row can't be looped alone | `:loop` on a spanning row lets its chain run on. `Start` `reset` restarts it instead. To judge in play. |
| A short chain's restart | Launching over a chain cuts it at the grid. `chain` remains for those who prefer the wait. |

**Security:** there's no network surface, and the song file is the only input. **Scale:** at most `MAX_TRACKS` tracks, with rows bounded by memory.

## Later: jumps and chance

A row could gain `then`, an optional row to go to next, and a second `then` with a chance, as [Ableton's Follow Actions](https://www.ableton.com/en/live-manual/12/launching-clips/) have A and B. This would write "intro once, then loop verse and chorus" directly. The author accepts randomness in a render, as one performance of the song. It would need:

*   **Keeping `new`'s promise:** a jump must count as a change, or a shared chain re-enters part-way depending on the route.
*   **One next-row function** for the engine and the render, pure and non-allocating.
*   **A way to mark the song's end,** with renders stopping on any move to a row already played otherwise.
*   **A row generator of its own,** seeded from the song, so every stem of one render takes the same route.
*   **Checks on load,** as a `then` past the last row mustn't reach the callback.
*   **Rules for row changes:** renumbering `then` when rows are added or removed, and walking past empty rows.

## Alternatives considered

*   **Dirtywave M8-style per-track lists instead of rows.** Each track moves down its own list when its chain ends. This allows pickups across a boundary, but alignment becomes the writer's job, and it replaces the song view. Decision 10 kept sections.
*   **[Maschine-style clips](https://docs.native-instruments.com/online-guides/maschine-mk3-manual/en/working-with-patterns-and-clips) over rows.** These solve parts crossing boundaries, but [users report them splitting and leaving gaps](https://community.native-instruments.com/discussion/3546/adding-scenes-or-changing-scene-length-with-existing-clips) when scenes change, and a free-placed layer doesn't suit a text grid. The same chain on neighbouring rows covers sequenced parts.
*   **Keeping carry as the default.** It needs no new engine state, but a chain doesn't play as written.
*   **Keeping carry as an option.** It's like Ableton's Legato mode, and differs from `new` only when a row's length isn't a multiple of the chain's. No listed problem needs it, and it's one enum variant to restore, as `place - started` already supports it.
*   **Jumps now.** See "Later". Copied rows share chains, so most song forms need only `repeat`.
*   **A classic order list of rows.** This writes A B A C without copied rows, but it replaces the song view for less benefit than `repeat`, since `gc` copies share chains.
*   **Removing chains or phrases.** Removing phrases makes chains into plain clips, losing shared editing and transposed reuse. Removing chains moves a part's structure up into rows.
*   **A scene layer under rows, as Maschine has.** Chains already share content between rows, so only a row's choice of chain stays local.
*   **Clip-style Follow Actions per track.** Ableton's groups rely on neighbouring slots, which for us are rows meant to line up. It would make tracks wander out of their rows, which is the Dirtywave M8's model.
*   **A launch grid from the running count.** `:reset` and wraps would move it, and lines would drift after a 12-step row, so `1bar` wouldn't mean a bar.
*   **Wider cells for labels.** Widening to about 8 characters cuts visible tracks at 80 columns from about 15 to about 6, which costs more in a jam than the label gains.
*   **Dropping a cue when a row changes.** Simpler, but it loses a press with no feedback, which is the worse outcome mid-jam.

## Choices to judge in play

*   Whether `1bar` is the right default.
*   How much a jam's lost phase matters on capture.
*   Whether labels are enough in the status line and picker, or belong in the cells.
*   Whether spanning rows need looping alone.

## Detailed Implementation Plan

M8d is planned to the file. M8c is an outline, as it should be finished once M8d has been played. Each is one issue and one PR, on its own branch.

### Findings left open by the review

The review's last stage ended with these not applied to the design above. Where they differ, the plan's treatment applies when the work is built, and the design text is updated in the same PR.

| Finding | Milestone | Treatment |
|---|---|---|
| Capture can be audible. A captured row shorter than the source ends at once, as `continue_in` leaves the row step alone, and a source with `Rpt` does the same. | M8d | Capture writes the source's effective length as a set length, and `ContinueIn` wraps the row step to fit. A longer launched chain is cut when the row is reached in order. Tests: a `reset` source with no set length, and later a repeated source. |
| A saved `launch` step count of 0 would panic on `%` in the callback. | M8d | Use a type or a load check that rules out 0. |
| `clear_launches` both ends launches and clears cues. | M8d | Split it. A different row keeps cues, and starting playback clears them. |
| A row with a set length and only `--` cells appears to stall playback, as `frames_until_next_step` waits forever when the row has no chain steps (`engine.rs:598`). Not yet reproduced. | Before M8c | Reproduce it with a test first. Then test the row's length there. It may be its own small PR. |
| Playing from a chain step can need a negative `started`, which breaks `started <= place`. | M8c | Make `started` signed, use `rem_euclid`, and drop the invariant and saturation. Chain step 40 from an earlier row's cell plays that row against the chain's second half, which is an accepted limit. |
| The play-from-row walk starts wrongly at a `:reset N` row, and copies the transition rules. | M8c | Start from the top or the last `Start` `reset`. Share one pure, non-allocating row-entry function with the engine, and test that the walk's state equals playing through. A row with no set length costs tracks times `chain_steps` in the walk, so correct the risk wording. |
| `Rpt` puts bar lines mid-pass. | M8c | Test the grid against `section_place % length`, and count a self-loop as a row starting. |
| `Start` `reset` and `:reset N` overlap within a row. | M8c | Decide before building. The current lean is to keep both, as `:reset N` longer than a row still has a use on a self-loop. |
| The playing chain can't be read back from the previous row, as `self.section` is an index. | M8c | Keep a per-track scalar for the playing chain, which playheads also need. |
| `repeat` has two defences against 0. | M8c | Use `NonZeroU16`, so serde rejects it and the engine needs no case. |
| Audition (`engine.rs:880`, `tree.rs:414`) and moving between tracks (`app.rs:883`) treat one row step as every chain's position. | M8c | Decide per case. A spanning row otherwise differs from playback. `PlaySection` needs a track field. |
| "Render content matching a second pass" holds only for even song lengths or no swing. | M8c | Use an even length, or no swing, in that test. |
| The milestone names run against the build order, and "Replaces" misses captured rows' set length. | M8d | Add the second to "Replaces". Swapping the names is the author's call. |

### M8d: files

| File | Change |
|---|---|
| `crates/vt-model/src/constants.rs` | Add `BAR_STEPS`, for a bar of 16 steps. |
| `crates/vt-model/src/song.rs` | Add the launch timing type and `Song.launch`, default `1bar`, saved. Add `Chain.label`. Bump `FORMAT_VERSION`. |
| `crates/vt-model/src/action.rs` | Add `SetLaunch` and `SetChainLabel`, like `SetSeed` (`action.rs:137`). `CloneChain` (`action.rs:732`) copies the label already, as it copies the whole chain. |
| `crates/vt-engine/src/engine.rs` | `start_cued` (`engine.rs:218`) tests `launch` against the row step, with `chain` as now. Split `clear_launches` (`engine.rs:200`): the different-row call at `engine.rs:287` keeps cues, which fire on the new row's first step, and the calls at 267, 365 and 454 clear them. `continue_in` wraps the row step. |
| `crates/vt-engine/tests/sections.rs` | Launch grid, cue and capture-length tests. Existing launch tests change where they rely on chain-end timing, by setting `chain`. |
| `crates/vt-engine/src/link.rs` | Extend the no-allocation guard test to a grid launch. |
| `crates/vt-tui/src/input/command.rs` | Parse `:launch` and `:label` into new `Line` values (`command.rs:10`), with their errors and tests. |
| `crates/vt-tui/src/app.rs` | Apply them, next to `Line::Seed` (`app.rs:1372`). Report a bare `:launch`. |
| `crates/vt-tui/src/song_view.rs` | Capture copies `length`, `start` and `reset`. `open_section_chain` opens the phrase for one phrase, adding one if there are none. Add a `gC` handler using `song_column`, with refusals. `K` on a track's column opens the chain picker. `LARGE_LENGTH` uses `BAR_STEPS`. |
| `crates/vt-tui/src/tree.rs` | Add the chain picker, on the pattern of `open_phrase_picker` (`tree.rs:338`) and `pick_row_phrase` (`tree.rs:377`). |
| `crates/vt-tui/src/views/status.rs` | Show the label of the chain under the cursor in the song view. |
| `crates/vt-tui/src/input/bindings.rs` | Add `g C` to `SONG`, and update `K`'s help text. |
| `crates/vt-tui/tests/app.rs`, `crates/vt-tui/tests/grammar.rs` | Tests for keys, refusals, labels, the picker, `gd` and undo, as M8d's Tests list. |
| `crates/vt-model/tests/history.rs`, `crates/vt-model/tests/project.rs` | Undo for the new actions, and saving `launch` and labels. |
| `crates/vt-tui/guides/chains.md` | Document launch timing, labels, `gd`, `gC` and the picker. |
| `docs/designs/song-flow.md`, `docs/designs/vimtracker.md` | Apply the findings handled here, and mark M8d built. |

`Track::add_chain` already gives a new chain a phrase, so `gd` on `--` only changes which view opens.

### M8c: outline

1.  Fix the silent row, if the test reproduces it.
2.  Decide the `Start` `reset` and `:reset N` overlap.
3.  Add `started` and the playing chain per track, replacing `Launched.started`, with a row-entry function the engine and the walk share.
4.  Change `SectionStart` to `New` and `Reset`, and update `sections.rs:116`.
5.  Add `repeat`, the `Rpt` column and `song_steps`, and move the grid to each pass.
6.  Add the play-from-row walk and playheads per track.
7.  Update render, the guide and both design documents.

### Verification

For each PR: all tests in debug and release, clippy clean, and goldens unchanged. Then drive the real app for keys and display. Whether launch timing and capture feel right in a jam needs the author's ears, as Claude can't hear the result.
