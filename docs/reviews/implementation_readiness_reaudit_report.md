# Stage 9 Re-audit: Implementation Readiness

**Target:** `docs/designs/vimtracker.md`, lines 547-822, after the first resolution

**Reviewers:** implementation readiness (3 Blockers, 13 Majors, 8 Nits), code details (3, 10, 8), style (0, 13, 18).
Findings are merged below, with reviewer tags [R], [C] and [S]. This is the one re-audit EGM allows for this stage.

**Verdict: NOT READY, but the failures have narrowed.** Architecture, constants, step format and DSP arithmetic held up.
The gaps are in the interaction model, which is what G1 tests, plus two places where the plan doesn't match real library
behaviour. The M1-M3 readiness scope was judged sound. The fallback rule was judged too broad.

**Limits:** nothing was compiled or run. The code-details reviewer read `gmuse`, `rtrb`, `crossterm`, `cpal` and `rubato`
source. The other two read only `gmuse`'s spec.

## Blockers

### 1. Phrase view has no model of where the cursor is in the song [R]

*   **Scope:** L617, L623, L637, L773, Issues 8 and 12, spec L287, L376-377, CUJ 1
*   A cursor is on a phrase, and a phrase can be in several chain slots or none, but playback runs at section level.
*   Undefined: where `Enter` starts for a shared phrase or one reached via `grr` or `gc`; `Enter` in chain, instrument and help
    views; which phrases neighbour columns show; where `W B` lands; whether the view scrolls with the playhead (playback "never
    moves the cursor", yet CUJ 1 says a playhead follows); how many playheads a shared phrase draws; the opening view.
*   **Guidance:** define a view context such as `(track, chain slot, phrase, step)`, derive a section step from it, and define
    neighbours and playheads from that step. Give a rule for a phrase with no context.

### 2. Transport states are undefined, and the `gmuse` reference contradicts the plan [R, C]

*   **Scope:** L637-639, L777, L786, spec L287-288, L324-328
*   Undefined: `Space x` from Stopped, `Enter` while Paused, the position after `Enter` stops. `Space c` returns to the start, but
    `Enter` always plays from the cursor, so there's no specified way to start from the top.
*   Whether a loop wrap counts as "starts from the top" decides whether `CHA` repeats the same pattern every pass, or held values
    and random state drift. Both are audible.
*   Voice and tail behaviour on stop and pause is unspecified, and nothing declicks them. `gmuse`'s `Fade` exists for this
    (`device.rs:109-119`).
*   `audio::cued` paused path gives `MarkHere`, and `audio::transported` `Toggle` from `Playing` gives `Paused`, so neither
    describes this plan. L786's "two `Toggle`s" is false under `transported` from Stopped, and `Toggle` isn't a defined command here.
*   **Guidance:** a state table with Stopped, Playing and Paused against `Enter`, `Space x` and `Space c`, giving position, voices and
    tails, and resets. State whether a loop wrap is a restart. Delete L777, keeping only that the engine resolves verbs from its own state.

### 3. Chain view has no editing specification [R]

*   **Scope:** Issue 11, L607-656, L794, CUJ 5
*   CUJ 5 needs entering an existing phrase ID into a second slot, and no route is specified. Undefined: Insert on phrase ID and
    transpose, transpose range and display, adding and removing rows, operators on chain rows, an empty slot (M8 treats it as chain
    end, `m8.txt` 867-869, while the plan's list can't represent one), a chain with no phrases, what `gc` does on a chain row, and what
    `:track add` creates.
*   **Guidance:** a chain-view key table and row definition, and the operator rules that apply there.

### 4. Repeat filtering can't work with no keyboard flags [C, R]

*   **Scope:** L637-639, L662, L768
*   `KeyEventKind::Repeat` comes only from the kitty/CSI-u field (`crossterm` `event/sys/unix/parse.rs:339-346`). Without flags, a held key
    arrives as repeated presses. Holding `Enter` flutters play and stop, and repeats after `Space x` reach Normal `x` and clear fields.
    `gmuse`'s spec notes the same (D-7), and its copied `Key` type discards `kind` (`src/cmd/keys.rs:9-14`). Issue 2 doesn't test held keys.
*   **Guidance:** a time-based debounce on transport keys, accepted flutter with the sentence removed, or pushing `REPORT_EVENT_TYPES`.

### 5. `rtrb` can't replace a waiting version [C, R]

*   **Scope:** L564, L585, L749, spec L260-261
*   `rtrb` 0.4.1's `Producer` has no `pop`; only the `Consumer` removes items (`src/lib.rs:346-482`).
*   **Guidance:** keep the newest version in a local slot on the edit thread and try to push on each edit and frame, or use another
    primitive such as `arc-swap`, which `gmuse` has, checked against the no-free rule. Test that the newest version always arrives.

## Major: real code

6.  **Copying `rt_guard.rs` alone isn't enough [C].** L758, L760, L786, L806, L814. It needs the `#[cfg(test)]` module and `#[global_allocator]`
    lines from `gmuse-engine/src/lib.rs:30-35`, or it always reads zero. Being `cfg(test)`, it isn't visible from `tests/*.rs`, so guard tests must be
    unit tests. `VIOLATIONS` is process-wide, so a test that allocates while armed can fail others intermittently; a thread-local counter fixes that.
7.  **The `device.rs` copy conflicts and mostly doesn't apply [C].** L715, L766. It forces `BufferSize::Fixed(max(512))` against "default configuration", and
    most of it serves `gmuse`'s ring design. It lacks `stream.play()`, `AudioError` and channel zero-filling. Name what's copied and settle buffer size.
8.  **The copied error closure discards what G7 measures [C].** L715, L766, L809, G7. `device.rs:479-481` drops `Xrun`, and `cpal` 0.18.2 reports CoreAudio
    overload as `Xrun` (`host/coreaudio/macos/mod.rs:100-115`). G7 needs a stream-error counter on the status line, plus a device-lost flag.
9.  **`thiserror` missing from `vt-tui` [C].** L563, L768. The `keys.rs` copy derives it.
10. **Rate conversion is undecided [C].** L713, L742. `rubato` offers `Fft` and sinc variants, and the choice changes hashes. `gmuse`'s `resample.rs` wraps this version and
    trims `output_delay`, keeping transients on their sample, but its streaming shape pads tails. `process_all_into_buffer` (`lib.rs:323`) appears to fit whole clips.
11. **Binding reuse is missed, and the grammar's fit with the table is undecided [C].** L666, L769. `src/cmd/bind.rs` has `resolve` and `continuations`, and `which_key_rows`
    is at `ui/mod.rs:8703`. Counts, registers and operator plus motion aren't fixed sequences, so how the table and `input/mode.rs` divide work must be decided.
12. **Copied parser spellings [C].** L609-640, L768. `Space ff` must be `<leader> f f`, `Ctrl-A` is `C-a`, and `C-A` parses but never fires.

## Major: interaction

13. **The fallback rule is too broad [R].** Spec L378. Almost any key behaviour affects flow, so the guard hands design decisions to the implementer. "Follow Vim" also misleads:
    Vim's jumplist includes `{`, `}` and `H M L`, so `Ctrl-O` would step through beat moves. Narrow it to error wording, out-of-range refusal and clamping, and state which moves are jumps.
14. **Mute state conflicts with undo [R].** L737, Issue 12, spec L238, L297-298, L374. Stored on `Track` in `Song` but excluded from undo, so undo publishes an older `Song` with old mutes.
    Say where live state lives, how the engine learns it on load, solo precedence, and whether a mute cuts a voice. Test: mute, edit, `u`, and the mute persists.
15. **No key for mute or solo [R].** L607-640, L681, spec L297. Only `:mute` plus `Enter`, with no clear-all. M8 has direct chords in every view (`m8.txt` 796-798). Add leader keys or state the deferral.
16. **FX slot partial states [R].** L601, L626, L654-655, Issues 9, 15. What clearing only a value or command does, whether `---40` parses, and when a typed command takes effect. If it starts at `00`, typing `CUT` during playback closes the filter until the value is typed.
17. **`:render` is undecided in places [R].** Issue 21, L584, L689, L693, CUJ 7. Session samples are at device rate, so rendering at 44100 needs re-decoding, or `:render` and `vimtracker render` differ. Also blocking vs worker, unsaved vs saved, mute handling, the `--tail` threshold, and exact length at fractional tempos.
18. **Instrument view and samples [R].** Issue 13, L678, spec L242. Ranges for volume, attack and release, `:inst` base, "current" vs "last-used", missing instruments. No path completion, so CUJ 1 means typing each path, while M8 and Digitakt browse with preview. A sample picker can be deferred, but it's the first thing a session does, so decide it.
19. **Project lifecycle [R, C, S].** L603, L672, L676-678, L691. Pathless `:new` conflicts with `:new path`. `:sample` and autosave have no folder in an unsaved project. Bare `:w` with no path, `:w path` onto an existing folder, `:e` during playback, and sample collisions.
20. **Autosave details [R].** Issue 22. No prompt mode or keys, no pathless location, and deletion, re-offering, version mismatch and in-progress Shape sessions are undefined.
21. **Issue ordering [R, C].** Issues 11, 13, 14, 15, 17-18. No way into chain or instrument views until Issue 14. `grr`'s window also arrives then. Issue 15's fallback needs Issues 17-18, so the listening test must use hand-typed values or move. "Ramps toward its next value" also needs a rule across phrase boundaries, swing and `DEL`.
22. **FX timing rules [R].** L705-711. Whether `KIL`, `OFF` and `RET` count from a delayed note, what they do with no note, whether a new note cancels a pending delayed note, and `CHA` with `DEL`.
23. **`RET` amounts [S, C, R].** L708. No mapping for `x=0` or `x=8`, or whether 10% compounds.
24. **Engine API and tests [R, C].** L748, L816. Instrument-to-sample lookup (paths vs IDs in the callback), `Phrase` visible-length field for `:len`, 64 track levels vs a `TELEMETRY_QUEUE` of 64, and Issues 13 and 19 have no tests. Transport keys are undefined in Visual, Insert and Shape.
25. **Pause and stop behaviour in other modes [C, R].** L637-639. Stopping mid-entry is a flow question and isn't defined outside Normal.

## Major: wording

26. **`Keys inherited from Vim keep Vim's meaning` is false [S].** L668. `+`, `-`, `=`, `{ }`, `p`, `Ctrl-E`, `Ctrl-D`, and `dj` all differ. "Losing a timeout" has nothing to apply to, as the leader has none.
27. **"Binding table" names two things [S].** L605, L666, L782. The document's table and the code's. Say which is authoritative.
28. **Advertised keys that can't work [S, C, R].** L611-612. `Space 1`, `4` and `6` target views that don't exist before G1, against your "Don't advertise what can't work" rule. Hide them.
29. **M8 names with different meanings [S].** L705-709. `CHA` and `RET y=0` differ from the M8, your stated reference. Note the deviations.
30. **Small issues [S].** L549 "readiness bar" is undefined. L711 "a timed value". L796's "without `:sample` alone". L816-817: "each issue lists its tests" is false for Issues 1, 2, 13, 19 and 23.

## Nit

Nits are in the reviewers' lists: `:bpm` wording, `%` on swing, `--rows` vs "steps", "Insert step" reading as a verb, file list and heading format, the 300 ms delay and swing type missing from constants, `+` and `-` on empty fields, tuning start values for delay and `RES`, no branch if G7 fails, a `gmuse` location, rounding asymmetry on falling curves, gain formula not fully stated, and acceptance wording in several issues.

## Checked and sound

*   Curve vectors, sample position formula, 80-tick swing, note range, 6 groove units per step, and the 2.25 s delay buffer.
*   All named `gmuse` files and symbols exist. `rank` supports the picker's ordering.
*   All listed versions are in the local registry. `gmuse`'s lockfile matches except `rtrb` (0.4.0 there, 0.4.1 here).
*   Binding table has no duplicates or leaf-and-prefix conflicts. `gc`/`gC` and `]p`/`]P` follow your rule.
*   The M1-M3 readiness scope hides nothing that matters.
