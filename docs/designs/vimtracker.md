# vimtracker

*   **Author:** gabrielgellner
*   **Status:** Proposed
*   **Date:** 2026-11-04

--------------------------------------------------------------------------------

## Overview

vimtracker is an experimental music tracker that runs in a terminal, is written
in Rust, and is edited with a keyboard grammar modelled on Neovim. It plays
samples on a laptop. It takes ideas from the Polyend Tracker, the Dirtywave M8
and Jeskola Buzz, but it is a new program: it does not port their code or copy
their appearance, and it does not aim to read their files or match their sound.

The first goal is to find out whether the idea is worth continuing. The prototype
is built to answer that question as cheaply as possible, while avoiding decisions
that would be expensive to undo if the answer is yes.

## The Problem

Hardware trackers shape their software around their hardware:

*   The Polyend Tracker's editing is built around pads, a jog wheel and screen
    buttons. Its patterns are pages of tracks, so reusing a part means reusing or
    copying a whole pattern.
*   The M8 has a compact structure of songs, chains, phrases and tables, and strong
    per-note sequencing. Its screen shows one track's phrase at a time, and its
    editing is limited to a small number of buttons. It has 8 tracks.
*   Both have fixed limits that come from their processors and screens. A laptop
    does not need most of those limits.

Moving a filter or volume across many steps is also slow. Buzz, Renoise and the M8
can interpolate between two values in a selected column, but the sources reviewed
for this design list only a few fixed curves or linear interpolation. None
describes adjusting the shape of the curve while listening to the result.

**Strongest counter-argument:** software trackers already exist on laptops, and
Renoise, OpenMPT and Schism Tracker are heavily keyboard-driven. If "keyboard
control on a laptop" were the whole idea, this project would not be needed. The
project is only justified if these things together make a clear difference:

1.  An editing language that combines an action with a target, such as "transpose
    this beat" or "clear this track", so that a small set of keys covers many edits
    and any edit can be repeated.
2.  Sweeps whose shape is adjusted live while the loop plays.
3.  Per-note tables and shared phrases available from that same editing language.

If these do not make editing clearly faster and more enjoyable, the right outcome
is to stop and use an existing tracker.

## Goals

*   **G1: Go/no-go test.** The author recreates a 16-bar loop they have already
    made, once in vimtracker and once in their usual tool, and records the time
    taken and which they preferred. This happens as soon as editing, FX and sweeps
    work. A result that is not clearly better stops or redirects the project.
*   **G2: Editing grammar.** Actions combine with motions and targets, counts work,
    `.` repeats the last change, `u` and `Ctrl-R` undo and redo, and registers hold
    copied steps. Every edit is also reachable without a mouse.
*   **G3: Reusable song structure.** Every track uses one shared grid model: a song
    is built from chains, phrases and tables that are referred to by number. A phrase
    can be used in several places, it shows when it is shared, and it can be cloned.
    Each track plays one voice. Polyphony uses several tracks.
*   **G4: Tables.** A note can start a small sequencer that runs at its own speed.
    Phrases and tables understand the same FX commands, including jumps, chance and
    randomness.
*   **G5: Shaped sweeps.** A selected range in a value column can be filled with a
    curve. The user can adjust the curve's onset and S-shape while the loop plays,
    then commit it as ordinary values or cancel it. Each curve reaches its exact end
    values and behaves the same rising or falling.
*   **G6: No built-in track limit.** Track count is part of the project and is not
    fixed in the code. A new project starts with 8 tracks. The same code and tests
    must pass with 1, 8 and 64 tracks. Chain, phrase, table and instrument counts
    follow the same rule. Hardware limits still apply, so the design must measure
    and report them rather than hide them.
*   **G7: Terminal first.** The program runs in a normal terminal, including inside
    tmux, and waveforms are drawn with text characters. No core feature depends on
    detecting that a key was released.
*   **G8: Steady timing.** Note timing is calculated in the audio thread, so redraws
    and file loading cannot move notes in time. On the author's laptop, the program
    plays 16 active tracks with no audible dropouts during a 10-minute session.
*   **G9: Testable without a screen.** A project can be rendered to a WAV file with
    no terminal or sound device. The same project and random seed produce identical
    samples on the same build.
*   **G10: Tracks can loop against each other.** Tracks can have different lengths
    and speeds, so they drift against each other within a section. Song rows decide
    when a section ends and whether track positions reset or carry on.
*   **G11: Generators render to the grid.** Tools fill notes and FX commands from
    parameters and a stored seed, and the sweep in G5 is the first of them. A
    generator stays adjustable until a cell it owns is edited by hand. That edit
    turns its output into ordinary cells.

## Non-Goals

*   Porting code from other trackers.
*   Reading or writing the file format of any other tracker or DAW. A JSON export
    exists for scripts, but file interoperability is not a goal.
*   Backward compatibility while prototyping. A format change can make older songs
    unloadable. Files carry a version number so the program can say why, and WAV
    export is how to keep a jam.
*   Track types, such as chord or stage-sequencer columns. Chords and stage sequences
    are generators that write to ordinary tracks.
*   Building generators other than the sweep before the G1 test.
*   Matching the sound of any other tracker exactly.
*   Using the names, logos or visual design of Polyend or Dirtywave products.
*   Selling the prototype. The Polyend Tracker CE license restricts paid direct
    ports, so a commercial release would require a separate review.
*   Synth engines, granular or wavetable playback, sample recording, streaming long
    samples from disk, MIDI input or output, plugin hosting and live performance
    mode in the prototype. These are candidates for later, once G1 passes.
*   A graphical interface, or Sixel or Kitty image rendering.
*   Live-coding, where running code generates the music during playback. Generators
    write ordinary steps that the user can edit.
*   Official support for Linux or Windows. The code avoids depending on macOS without
    a reason, but only macOS is tested.

## Open Questions

*   **Key input**: What key events do the author's terminal and tmux setup actually
    deliver? The prototype's first milestone must test this.
*   **Track capacity**: How many active tracks does the author's laptop sustain? G6
    removes the code limit, but this number needs measuring.
*   **Smoothing time**: Is about 10 ms enough to stop a filter sweep sounding stepped
    without softening fast changes? This needs listening tests.
*   **FX persistence**: Should each FX command hold its value until changed, or apply
    to one step and revert, as Elektron parameter locks appear to? The Technical Plan
    proposes one rule per command, and it needs confirming against how sweeps sound.
*   **Multi-track generators**: When one generator writes to several tracks, as a
    chord does, does editing one track's owned cell break the link on all of them?
    This can wait until after G1 because the prototype's only generator, the sweep,
    writes to one phrase.

## Critical User Journeys (CUJs)

### CUJ 1: Write a loop

*   **Given**: A new project with 8 tracks and a folder of WAV files.
*   **When**: The user assigns samples to instruments, enters notes on several
    tracks using the keyboard, and presses play.
*   **Then**: The loop plays in time, a playhead follows it on screen, and edits made
    during playback are heard on the next pass through the changed step.

### CUJ 2: Shape a filter sweep

*   **Given**: A looping phrase with filter values at the first and last steps of a
    selected column.
*   **When**: The user presses `=`, adjusts the onset and S-shape, then presses Enter.
*   **Then**: Every adjustment is heard during playback and shown in the column.
    Enter leaves ordinary editable values, one `u` restores the original values, and
    Esc cancels with no change. `.` applies the same shape to another selection.

### CUJ 3: Build an arpeggio with a table

*   **Given**: An instrument playing a held note in a phrase.
*   **When**: The user opens the instrument's table, enters semitone offsets, adds a
    jump back to the first row, and sets the table speed.
*   **Then**: Each new note repeats the arpeggio at the table's speed, independently
    of the phrase's step rate, and the table restarts on the next note.

### CUJ 4: Reuse and vary a phrase

*   **Given**: A phrase used in two places in the song.
*   **When**: The user jumps into the phrase from a chain, sees that it is shared, and
    clones it before editing. The user then jumps back.
*   **Then**: The edit affects only the clone, the other use is unchanged, and the
    cursor returns to where the jump started.

### CUJ 5: Add more tracks

*   **Given**: A project with 8 tracks that is using all of them.
*   **When**: The user adds tracks beyond 8 and writes parts on them.
*   **Then**: New tracks can be navigated, played, muted and saved like the first 8.
    The display scrolls or folds columns when they do not fit, and CPU load is
    visible so the user can see when the laptop is reaching its limit.

### CUJ 6: Test without a screen

*   **Given**: A saved project on a machine with no display session or sound output.
*   **When**: A test or the user runs the render command with a fixed random seed.
*   **Then**: It writes a WAV file whose samples match earlier renders from the same
    build.

### CUJ 7: Let tracks drift, then move on

*   **Given**: A song row with a 7-row phrase on one track and a 16-row phrase on
    another, and a row length of 64.
*   **When**: The song plays through that row and the next.
*   **Then**: The tracks drift against each other for 64 rows. If the row is set to
    reset, every track starts the next row from its beginning. If it is set to carry,
    each track keeps its position within its loop. Starting playback from the middle
    of the song produces the same result as reaching that point from the start.

### CUJ 8: Adjust a generator, then take over by hand

*   **Given**: A phrase with a committed sweep in its filter column.
*   **When**: The user reselects the sweep with `gv`, reshapes it with `=`, and later
    edits one of its cells by hand.
*   **Then**: Reshaping rerenders the owned cells while the loop plays. The hand edit
    turns the sweep into ordinary values and the status line says so. `u` restores
    both the old value and the generator. Editing the notes in the same phrase does
    not break the link.

## Technical Plan (Architecture)

### Summary

The program has one thread that owns and edits the song and one audio thread that
plays it. The edit thread never changes a song version that has been published. Each
edit creates a new version that shares all unchanged data with the old one, and sends
it to the audio thread. Sequencing happens inside the audio callback, so screen
redraws cannot move notes.

```text
 Keyboard
    |
    v
+--------------------------+   new song version    +-------------------------+
| Edit thread (vt-tui)     |---------------------->| Audio callback          |
|                          |   lock-free queue     | (vt-engine)             |
| modes -> actions         |                       |                         |
| song + undo history      |<----------------------| playheads               |
| generators, file saving  |   old versions, to    | FX interpreter          |
| screen drawing           |   be freed here       | voices -> mixer -> out  |
|                          |<----------------------|                         |
+--------------------------+   playhead, CPU load, +-------------------------+
                               dropout count
```

The same engine runs without a sound device to render a song to WAV.

### Components

| Crate | Owns | May depend on |
|---|---|---|
| `vt-model` | Song data, edit actions, generators, the row text format, saving and loading | No audio or terminal libraries |
| `vt-engine` | Sequencing, FX interpreter, voices, mixer, offline render | `vt-model` |
| `vt-tui` | Modes and key handling, views, audio device, the `vimtracker` program | `vt-model`, `vt-engine` |

Libraries: `ratatui` and `crossterm` for the terminal, `cpal` for audio output, `rtrb`
for lock-free queues, `serde` and `ron` for saving, `serde_json` for JSON export, and
`hound` for reading and writing WAV files.

### Song model

```text
Song
 |- tracks[]        mute, solo, speed, current groove
 |- rows[]          one chain per track, length, repeat count, Reset or Carry,
 |                  optional tempo, mutes
 |- chains{id}      list of (phrase, transpose)
 |- phrases{id}     cells[], generator links[]
 |- tables{id}      rows with transpose, volume and FX
 |- grooves{id}     list of tick counts
 '- instruments{id} sample, base settings, table, table speed
```

*   **No fixed limits.** Tracks are a growable list. Chains, phrases, tables, grooves
    and instruments are maps from ID to value, using 32-bit IDs. Hex columns widen when
    an ID needs more digits. Phrases can have any length of 1 row or more. New phrases
    default to 16 rows.
*   **Cells.** A phrase cell has a note or note-off, a volume, an instrument and three
    FX slots. A table row has a transpose, a volume and three FX slots. FX slots are
    named three-letter commands with a value.
*   **Reuse.** Chains and rows refer to phrases by ID. A shared phrase displays a
    marker, and an edit to it changes every use. Clone creates a new ID.
*   **One voice per track.** A new note on a track replaces the note already playing.

### Passing edits to audio

1.  An edit action takes the current song and returns a new song, copying only the
    changed items and their parents. Unchanged items are shared through `Arc`.
2.  The edit thread sends the new version through a lock-free queue. If the queue is
    full, it keeps only the newest version and sends that when space appears, so a
    fast run of edits cannot back up.
3.  At the start of each audio block, the callback takes the newest version and sends
    the one it replaces back through a second queue.
4.  The edit thread frees returned versions. The audio callback never allocates or
    frees memory, and it never takes a lock. This does not use `basedrop`, because
    the callback only holds one top-level `Arc` and returns it without dropping it.

**Per-track playback state.** Playheads, voices and table positions belong to the
audio thread, in storage made ahead of time. When a new version has more tracks than
the audio thread has storage for, the edit thread creates the extra state and sends it
with that version. Old state goes back to be freed.

**When a new version changes what is playing.** A playhead stores IDs and a position,
and it checks them when a new version arrives:

*   If the phrase got shorter, the position wraps to fit the new length.
*   If the phrase or chain no longer exists, the track moves to the next row of its
    chain and is silent if there is none.
*   If the track was removed, its voice stops with a short fade to avoid a click.

### Time

The song clock counts ticks, and it runs at 960 ticks per quarter note. That gives 240
ticks per row at normal speed, about 0.5 ms each at 120 BPM, which is fine enough for
strums. Grooves are written as 6 units per row, as on the M8, and each unit is 40
ticks. Track speeds of 1/8, 1/4, 1/2, 3/4, 1, 3/2 and 2 all give a whole number of
ticks per row. The engine converts ticks to sample positions and starts events on the
exact sample within a block.

### Song rows and looping

Each track plays its row's chain and loops it until the row ends. A row's length is
measured in normal-speed rows, and it defaults to its longest chain at that track's
speed. It can also repeat a set number of times.

At the end of a row, a **Reset** row sends every track to the start of the next row's
chain. A **Carry** row keeps each track's count of rows played. In the next row, that
count is wrapped to the new chain's total length.

Carry means a track's position depends on the rows before it, so playing from the
middle of a song calculates it first. A pure function in `vt-model` steps through from
the last Reset row, or from the start of the song, counting rows without producing any
audio. A test checks that starting mid-song matches playing from the start.

### FX interpreter and tables

One interpreter runs FX commands for both phrases and tables. A phrase runs it once
per row. A table runs it at the table's own tick rate.

*   **Starting a table.** A note starts its instrument's table from row 0. Aux tables
    run alongside it on the same track and are started by a command.
*   **Independent columns.** Each table column has its own playhead, its own rate and
    its own loop point, including the transpose and volume columns. On the M8, the
    tutorial's arpeggio example shows those two columns moving with the first FX
    column.
*   **Per-note mode.** A column at `TIC00` moves only when a note triggers, and it
    keeps its position between notes. This is how a 5-step cycle drifts over a 16-step
    phrase. Its position resets on play from the start, on a table change, and at a
    Reset row. It carries across Carry rows.
*   **Note-off.** Tables can test whether the note is still held, which allows a real
    sustain. The M8's tables cannot.
*   **First command set.** `HOP`, `THO`, `TIC`, `TBL`, `TBX`, `DEL`, `KIL`, `OFF`, `RET`,
    `REP`, `CHA`, `RND`, `IFC` (play on loop A of every B, like Elektron's `A:B`), `VOL`,
    `PAN`, `CUT`, `RES`, `DLY` and `REV`.
*   **Randomness.** Each track has its own random generator, seeded from the project
    seed and the track's position in the list, so adding a track does not change what
    other tracks do.
*   **Value persistence.** The proposal is that mixer and filter values (`VOL`, `PAN`,
    `CUT`, `RES`, `DLY`, `REV`) hold until changed, and note events (`DEL`, `KIL`, `RET`)
    apply once. See Open Questions.

### Audio path

Each track has one voice: sample playback, a filter, a volume envelope and pan. Tracks
feed a mixer with shared delay and reverb sends, followed by a limiter. Changes to
filter, volume and pan pass through a short smoother, about 10 ms to start, so sweeps
do not click or sound stepped. Processing is 32-bit float at the device's sample rate,
and samples are converted to that rate when loaded. Samples are kept fully in memory.

### Editing

Keys go through a mode state machine (Normal, Insert, Visual, Shape, Command, Picker)
that produces actions. Actions are plain data, which gives `.` and macros something to
replay and lets tests run editing without a terminal.

*   **Grammar.** Operators: `d`, `y`, `p`, `>`, `<`, `=`. Motions: `h j k l`, `w b`,
    `{ }`, `gg G`. Targets: `it` track, `ib` beat, `ic` column, `ip` phrase.
*   **Moving through the tree.** `gd` opens the item under the cursor, `Ctrl-O` jumps
    back, and `grr` lists where it is used. `gc` clones, and `gC` deep-clones a chain.
*   **Undo.** Undo history is a list of song versions, so undo publishes an older
    version. History keeps the last 1000 versions, and it costs little because versions
    share unchanged data. A preview session counts as one undo step.
*   **Views.** The phrase, chain, song and instrument views come first, followed by the
    table and groove views. The multi-track score view comes after tables, and the
    waveform view comes after that. Score view selections stop at phrase boundaries in
    its first version.
*   **Input limits.** Note preview plays for a fixed length, so no feature needs key
    release events.

### Generators

A generator is a pure function. It takes its parameters, a seed, and the size and
current contents of its target. It returns the cells it wants to write. It has no other
inputs, so the same inputs always give the same output.

Generators combine two independent parts, an idea taken from the Polyend Fill tool. A
**mask** chooses which steps to change, such as existing notes, empty steps, every Nth
step, or Euclidean. A **value source** chooses what to write, such as a constant, a
shaped ramp, or a seeded random value. Ramp positions come from a step's place in the
selection, so a sparse mask stays on the same curve.

**Curves.** `f(t)` applies Schlick's bias, then gain, with `t` running from 0 to 1 across
the selection, and the value is `start + (end - start) * f(t)`. Both functions reach 0
and 1 exactly, so a curve hits its end values in either direction. Results are rounded
and clamped to the column's range. Notes snap to the current scale when one is set.

**Linking.** A committed generator is saved on the phrase with its parameters, seed and
the list of cells it owns.

*   Changing parameters rerenders the owned cells and publishes a new version.
*   Editing an owned cell by hand deletes the link and shows a status message. The cell
    values stay as they are.
*   Editing a cell it does not own leaves the link in place.
*   Undo restores the link, because the link is part of the phrase.
*   Cloning a phrase copies the link, and the two render independently.

### Saving and loading

A project is a folder holding `song.ron` and a `samples/` folder. Importing a sample
copies it into `samples/`. Saving writes to a temporary file and renames it, so a crash
cannot leave a partial song. `song.ron` starts with a format version. A file with a
different version is refused with a message saying which version it needs, and there is
no migration code. Phrase cells are saved as one readable string per row, using a small
parser and printer that tests can also use.

The command `vimtracker export-json <project>` loads the song with the same data
types and writes JSON. A test program confirmed that one set of `serde` types converts
a song, including nested enums and optional values, from RON to JSON and back with no
change in value. Comments in a RON file are not kept.

### Testing

*   **Editing:** apply actions to a song and compare the result, using the row text
    format for inputs and expected output.
*   **Sound:** render fixed songs to samples and compare them with saved hashes. These
    hashes change when DSP changes on purpose.
*   **Real-time safety:** in tests, a counting allocator fails the test if the audio
    callback allocates or frees memory.
*   **Track limit:** the core model and engine tests run at 1, 8 and 64 tracks.

### Risks

| Risk | Effect | Mitigation |
|---|---|---|
| A version is dropped on the audio thread | Rare glitches that are hard to reproduce | The callback returns versions through a queue, and the allocator test fails on any free |
| The return queue is full | The callback has nowhere to send the old version | Queues are sized well above what can be in flight, and the callback does not take a new version until it can return the old one |
| A CPU spike with many tracks | Dropouts, with no code limit to prevent them | CPU load and a dropout count stay on screen, and a block that runs late is counted |
| Carry rows give surprising results | The song sounds different from what the user expected | The position rule is a tested pure function, and the song view shows Reset or Carry on every row |
| A hand edit breaks a link by accident | Generator settings disappear | A status message appears, and `u` restores the link |
| An old song stops loading | A jam can be lost | This is accepted for the prototype, and WAV export keeps the audio |
| Terminal key limits | Keys cannot be held for chords | The design needs no key release, and the first milestone tests real behaviour |

## Alternatives Considered

### Port Polyend Tracker CE to Rust

*   **Pros**: A complete feature list and known behaviour, with most of its source under
    an MIT-based license.
*   **Cons**: About 330,000 lines of platform code and 58,000 lines of hardware-shaped
    UI would be dropped anyway. The sequencer includes UI headers, and its synth engines
    are closed binaries. The license also restricts paid direct ports.
*   **Reason for Rejection**: The question is whether a new editing model works, and a
    port would spend most of its effort on things that do not answer it. The code is
    used as a reference.

### Extend an existing software tracker

*   **Pros**: A finished audio engine and file handling, and possibly a much faster way
    to test the vim grammar. Renoise, for example, has a tool system, and OpenMPT and
    Schism Tracker have public source.
*   **Cons**: Terminal-first operation, tables, song-row timing and generator links
    would all need to fit around another program's model.
*   **Reason for Rejection**: The goal is a terminal-native tool with its own structure.
    This alternative was not tested in practice. If G1 suggests that only the grammar
    matters, it should be revisited.

### Track types

*   **Pros**: Chord and stage-sequencer tracks could each have a natural editor, with no
    pressure to fit a grid.
*   **Cons**: Every type must define every editing action, its copy and paste, and how it
    appears in the score view. Sequences with variable-length stages do not line up with
    grid rows. On a polyphonic track, tables and commands such as `KIL` need a rule for
    which note they act on.
*   **Reason for Rejection**: It pulls the program away from being a tracker. Generators
    that render to ordinary tracks give most of the same results with one editor and one
    engine.

### One grid with chord cells

*   **Pros**: Chord names stay visible in one column, with no extra tracks.
*   **Cons**: Every cell and command must handle chords, even on tracks that never use
    them, and TextBeat-style chord names need wider columns.
*   **Reason for Rejection**: Chords across several tracks give each note its own table
    and commands and need no new cell type. Removing the track limit makes this cheaper.

### Generators as an edit layer

*   **Pros**: Hand edits and generator control could both be kept.
*   **Cons**: Every edit path must write to the layer, and the view must mark those
    cells. Edits can land on steps that no longer make musical sense after a length or
    rhythm change.
*   **Reason for Rejection**: Hand editing should need no syncing. Editing an owned cell
    ends the link, which is simpler to understand and implement.

### One-shot generators with remembered settings

*   **Pros**: Nothing to store or keep linked, and this is how the Polyend Fill tool works.
*   **Cons**: Settings are not tied to a part, so an older part cannot be adjusted.
*   **Reason for Rejection**: Returning to a generated part and changing a parameter is a
    requirement.

### Free-running tracks

*   **Pros**: No new concepts and the longest possible drift. This is how the M8 works.
*   **Cons**: Each track moves on when its own chain ends, so unequal lengths put later
    sections out of step. The M8 manual advises equal-length chains to avoid this.
*   **Reason for Rejection**: Drift should be available without losing control of the
    arrangement.

### Row length with no carry option

*   **Pros**: Every section starts cleanly, and starting mid-song needs no calculation.
*   **Cons**: A long polyrhythm always stops at a section boundary.
*   **Reason for Rejection**: Some songs need a pattern to keep drifting across sections,
    as Elektron's `RESET` set to `INF` allows.

### Edit messages to the audio thread

*   **Pros**: The audio thread never waits, and each edit is small.
*   **Cons**: There are two copies of the song, and each edit type needs code on both
    sides. A rerender can send many cells at once, which could overflow the queue or
    apply over several audio blocks. None of the programs read uses this for all song
    edits.
*   **Reason for Rejection**: Generator previews need to replace many cells at once, and
    a bug here would make the sound differ from the screen.

### A mutex around the song

*   **Pros**: The least code and only one copy. Furnace and Schism Tracker use locks.
*   **Cons**: Timing depends on how long the edit thread holds the lock. Heavy rerenders
    and more tracks make late audio more likely, and moving away later would change how
    every edit reaches the song.
*   **Reason for Rejection**: Snapshots avoid the lock, make undo and preview simple, and
    match the approach in Ardour and Tracktion Engine, at the cost of more initial setup.

### Other save formats

*   **Binary, as on the Polyend and M8**: Compact and quick to load, but the size benefit
    is small because samples are stored separately, and it cannot be diffed or inspected
    when debugging.
*   **Zipped XML or JSON with samples inside, as in Renoise**: One portable file, but
    compression saves little on a small song file, and it gives up `git diff`, `grep`
    and direct editing. An export command could add this later.
*   **JSON**: Works with every tool, but has no comments and is fussier to hand-edit. RON
    is easier to edit, and an export command gives scripts JSON when needed.
*   **Folder with one text file per phrase**: Gives clean per-phrase history, but needs
    more code and can leave files missing or mismatched.
*   **Reason for Rejection**: For throwaway jams with no compatibility requirement, one
    readable RON file is the least code and the easiest to debug.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Performance:** G8 is the target. The audio callback must not allocate, free, lock,
    do file or terminal I/O, or panic.
*   **Security and privacy:** None are critical. The program reads local WAV and project
    files. Loaders return errors on bad input rather than panic.
*   **Rollout:** There is none. Older song files can stop loading, as stated in Non-Goals.

--------------------------------------------------------------------------------

### Additional Resources

*   [Polyend Tracker CE](https://github.com/polyend/tracker-ce): feature reference. See
    `core/source/mtSequencer_patternEdit.cpp` for fill behaviour.
*   [Dirtywave M8 operation manual, June 2023](https://cdn.shopify.com/s/files/1/0455/0485/6229/files/m8_operation_manual_v20230630.pdf)
*   [Working with M8 tables](https://cs.uwaterloo.ca/~plragde/flane/TM8C/Working_with_tables.html)
*   [Digitakt II user manual, OS 1.17](https://www.elektron.se/wp-content/uploads/2026/10/Digitakt-2-User-Manual_ENG_OS1.17_260930.pdf)
*   [Ableton Live 12 MIDI tools](https://www.ableton.com/en/live-manual/12/midi-tools/)
*   [TextBeat](https://github.com/flipcoder/textbeat): chord notation ideas.
*   `gmuse`: the author's own terminal music player, on the same audio and terminal
    libraries. It is a reference for key design, its picker and its real-time allocation
    guard, and code may move between the two projects. Its future DJ work may also
    overlap with this project.
*   [Metropolix manual](https://www.analoguehaven.com/intellijel-designs/metropolix-solo/manual.pdf):
    needs a full read before a stage generator is designed.
*   Synchronization references: [Ardour `rcu.h`](https://raw.githubusercontent.com/Ardour/ardour/master/libs/pbd/pbd/rcu.h),
    [Tracktion Engine](https://github.com/Tracktion/tracktion_engine) (GPL or commercial,
    so read for ideas only), [Furnace](https://github.com/tildearrow/furnace), and
    [Schism Tracker](https://github.com/schismtracker/schismtracker).

### Future Improvements

*   **Generators:** Euclidean with two pulse generators and boolean operators, stage
    sequences, chords with strum, and transformers modelled on Ableton's MIDI tools.
*   **Conditions:** the rest of Elektron's set: `PRE`, `NEI`, `1ST`, `LST` and fill mode.
*   **Editing:** macros, marks, `/` search and track groups that fold together.
*   **Audio:** MIDI input, synth voices and streaming long samples from disk.
*   **Sharing:** a command that exports one portable file.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is split into issues, each small enough for one pull request. Every issue leaves
the workspace building with `cargo test` passing. Milestones M1 to M3 reach the G1 test,
and **work stops there until the result is recorded**. M4 and later happen only if G1
passes.

### Fixed Specifications

These apply to every issue, and an implementer should not change them without updating
this document.

**Toolchain and dependencies.** Rust 2024 edition, with these versions, which were
confirmed to build together:

| Crate | Version | Used by |
|---|---|---|
| `serde` (with `derive`) | 1.0.229 | `vt-model` |
| `ron` | 0.12.3 | `vt-model` |
| `serde_json` | 1.0.154 | `vt-model` |
| `hound` | 3.5.1 | `vt-model` |
| `rtrb` | 0.4.1 | `vt-engine` |
| `ratatui` | 0.30.2 (use its `ratatui::crossterm` re-export) | `vt-tui` |
| `cpal` | 0.18.2 | `vt-tui` |
| `nucleo-matcher` | 0.3 | `vt-tui` |

**Constants.** Put these in `vt-model/src/constants.rs`:

| Constant | Value |
|---|---|
| `FORMAT_VERSION` | 1 |
| `TICKS_PER_ROW` | 240 |
| `GROOVE_UNIT_TICKS` | 40 |
| `DEFAULT_TRACKS` | 8 |
| `DEFAULT_PHRASE_ROWS` | 16 |
| `BEAT_ROWS` | 4 |
| `DEFAULT_BPM` | 120 |
| `FX_SLOTS` | 3 |
| `HISTORY_LIMIT` | 1000 |
| `SMOOTHING_MS` | 10 |
| `DECLICK_FRAMES` | 64 |
| `PREVIEW_MS` | 250 |
| `FRAME_MS` | 33 |
| `DEFAULT_RENDER_RATE` | 44100 |
| `PUBLISH_QUEUE` | 8 |
| `RETURN_QUEUE` | 16 |
| `TELEMETRY_QUEUE` | 64 |
| `COMMAND_QUEUE` | 64 |

**Row text format.** Fields are separated by one space, and values are uppercase hex.

```text
Phrase row:  NOTE VOL INST FX FX FX    C-4 64 01 CUT40 ---00 ---00
Table row:   TRSP VOL FX FX FX         +03 -- RET10 HOP00 ---00
```

*   `NOTE`: `C-4` or `C#4`, where `C-4` is MIDI note 60. `---` is empty and `OFF` is a
    note-off.
*   `VOL`: `00` to `FF`, or `--` for unchanged. Gain is the value divided by 255.
*   `INST`: 2 or more hex digits, or `--` for none.
*   `FX`: a three-character command and a two-digit value. `---00` is empty.
*   `TRSP`: a signed semitone offset in hex: `+00` to `+7F`, or `-01` to `-7F`.

**Project format.** `song.ron` serializes `Song` directly, with `format_version` as its
first field. Samples are stored by path relative to the project folder.

**Keys.**

| Mode | Key | Action |
|---|---|---|
| Normal | `Space` | Leader. It waits for the next key with no timeout, and after 300 ms a popup lists the keys available next, as which-key does |
| Normal | `Space Space`, `Space ff` | Open the page picker |
| Normal | `gt`, `gT`, `3gt` | Next page, previous page, page by number in the order below |
| Normal | `Space ?` | Show every key, generated from the binding table |
| Normal | `Space qq` | Quit, the same as `:q` |
| Normal | `h j k l` | Move by column or row |
| Normal | `w b` | Next or previous column |
| Normal | `W B` | Next or previous track |
| Normal | `{ }` | Previous or next beat |
| Normal | `gg G` | First or last row |
| Normal | `H M L` | Cursor to the top, middle or bottom of the view |
| Normal | `Ctrl-D`, `Ctrl-U` | Half a page down or up, keeping the cursor mid-screen |
| Normal | `Ctrl-E`, `Ctrl-Y` | Scroll the view 3 rows without moving the cursor, unless it reaches the edge |
| Normal | `]p [p`, `]P [P` | Next or previous phrase in the chain; last or first phrase |
| Normal | `]r [r`, `]R [R` | Next or previous song row; last or first song row |
| Normal | Count + motion or operator | Repeat |
| Normal | `d y > <` + motion or target | Clear, copy, transpose (notes by semitone, values by 1) |
| Normal | `dd yy >> <<` | Apply to the current row |
| Normal | `x` | Clear the current cell |
| Normal | `p` | Paste at the cursor |
| Normal | `"a` before `d`, `y` or `p` | Use register `a` to `z` |
| Normal | `+`, `-` | Increase or decrease the value. `Ctrl-A` and `Ctrl-X` do the same where the terminal delivers them |
| Normal | `.`, `u`, `Ctrl-R` | Repeat, undo, redo |
| Normal | `gd`, `Ctrl-O`, `Ctrl-I`, `grr` | Open item, jump back, jump forward, list uses. `gr` is only a prefix |
| Normal | `gc`, `gC` | Clone, deep-clone |
| Normal | `gv` | Reselect the last selection |
| Normal | `Enter` | Play the current view's scope (phrase, chain or song) from the cursor's row |
| Normal | `Space x` | Play or stop, resuming the current scope from its start |
| Normal | `Space c` | Return to the start of the scope. If playing, stop there |
| Normal | `i`, `v`, `V`, `:` | Insert, cell Visual, row Visual, Command |
| Visual | Motions, operators | Extend, then apply to the selection |
| Visual | `=` | Enter Shape mode |
| Shape | `h l` | Bias down or up by 0.05, within 0.05 to 0.95 |
| Shape | `j k` | Gain down or up by 0.05, within 0.05 to 0.95 |
| Shape | `r` | Swap the end values |
| Shape | `Enter`, `Esc` | Commit, cancel |
| Insert (note) | `z s x d c v g b h n j m` | C to B, current octave |
| Insert (note) | `q 2 w 3 e r 5 t 6 y 7 u` | C to B, one octave up |
| Insert (note) | `1` | Note-off |
| Insert (note) | `[ ]` | Octave down or up |
| Insert (note) | `Backspace` | Clear the cell and move up |
| Insert (value) | `0-9 a-f` | Type hex digits |
| Insert (FX command) | Letters | Type the command name |
| Insert | `Esc` | Normal mode |
| Picker | Printable keys, `Backspace` | Edit the search text |
| Picker | `Ctrl-N`, `Ctrl-P`, arrows | Move the highlight |
| Picker | `Enter`, `Esc` | Open the highlighted page, close without changing page |

**Page picker.** A centred floating window has a search line above a list of pages,
modelled on the Snacks picker for Neovim. Pages are the views that exist at that point
in the plan, in the fixed order song, chain, phrase, table, instrument, groove, score,
waveform. With no search text, the list is ordered by most recent use. With search
text, `nucleo-matcher` ranks the page names, and ties go to the most recent use. Audio
keeps playing, and the view underneath does not change until `Enter`. An unbound key
after the leader cancels it and shows a status message.

**Binding table.** Default keys are one data table of mode, key sequence, action and
description, not a `match` in the input handler. The which-key popup and `Space ?` are
generated from it, so they cannot disagree with it. A duplicate key within a mode fails
a test. Remapping at runtime is not in this plan.

**Key conventions.** New bindings follow the author's Neovim rules. A capital is the
wider version of its lowercase key, never a different action. A doubled key is the
ordinary case. No key is both a complete action and a prefix.

**Cursor and playhead.** These are independent. Playback never moves the cursor, and the
playhead is drawn separately. The status line always shows the current mode. Play and
stop are sent as commands the engine resolves against its own state, so two quick
presses cannot act on an out-of-date view of whether it is playing.

Targets are `it`, `ib`, `ic` and `ip`, as in the Technical Plan. In the phrase view, `it`
behaves the same as `ip`. Insert mode moves down by the step setting after a note
is entered. A note uses the last-used instrument and plays a `PREVIEW_MS` preview.

**Commands.**

| Command | Action |
|---|---|
| `:w [path]`, `:e path`, `:q`, `:q!` | Save, open, quit, quit without saving |
| `:new path` | Create a project folder with `DEFAULT_TRACKS` tracks |
| `:sample path` | Copy a WAV into `samples/` and assign it to the current instrument |
| `:track add [n]`, `:track del` | Add `n` tracks (default 1), or remove the current track |
| `:mute`, `:solo` | Toggle on the current track |
| `:bpm n`, `:set step=n`, `:set scale=NAME root=NOTE` | Tempo, Insert step, scale |
| `:len n` | Set the current phrase's length |
| `:render path [--rows n]` | Render to WAV with the same engine |

Scales are `chromatic` (default), `major`, `minor` and `pentatonic`. A scaled note moves
up to the nearest note in the scale. The program also has these subcommands:
`vimtracker [project]`, `vimtracker render <project> <out.wav> [--rate n] [--rows n]`,
and `vimtracker probe-keys`.

**FX commands.** Values are hex. A range of `00` to `FF` maps linearly unless stated.

| Command | Meaning in a phrase | Meaning in a table | Persistence |
|---|---|---|---|
| `HOP xy` | Jump to row `y` of the next phrase. `HOPFF` stops the track | Jump this column to row `y`, `x` times (`0` is always) | Once |
| `THO xx` | Start the instrument table at row `xx` | Move all columns to row `xx` | Once |
| `TIC xx` | Set the table rate in ticks of 40 (`01` is one groove unit). `00` is per-note mode | Same, for this column | Holds |
| `TBL xx` | Change the instrument's table for this track | Same | Holds |
| `TBX xx` | Start aux table `xx`. `00` stops it | Same | Holds |
| `GRV xx` | Set the track's groove to `xx` | Same | Holds |
| `DEL xx` | Delay this row by `xx` ticks | Delay the column by `xx` ticks | Once |
| `KIL xx` | Cut the note after `xx` ticks | Same | Once |
| `OFF xx` | Start release after `xx` ticks | Same | Once |
| `RET xy` | Retrigger every `y` groove units. `x` is 0-7 quieter or 8-F louder per retrigger | Same | Once |
| `REP xx` | Repeat the previous FX value on this slot, adding `xx` as a signed value each row | Same, per table row | Until `REP00` or a new command |
| `CHA xx` | Play this row with chance `xx/FF` | Play this table row with that chance | Once |
| `RND xy` | Add a random amount to the slot's previous FX value. `x` lowers and `y` raises the range | Same | Once |
| `IFC xy` | Play this row on loop `x` of every `y` loops of its phrase | Same, counting table loops | Once |
| `VOL xx` | Track gain | Same | Holds |
| `PAN xx` | Pan: `00` left, `80` centre, `FF` right | Same | Holds |
| `CUT xx` | Low-pass cutoff, exponential from 20 Hz to 20 kHz | Same | Holds |
| `RES xx` | Resonance | Same | Holds |
| `DLY xx` | Delay send | Same | Holds |
| `REV xx` | Reverb send | Same | Holds |

Slots are processed left to right. An unknown command in a loaded file is a load error.

**Audio defaults.** Voice: linear-interpolated sample playback with `C-4` at original
pitch, a 1 ms attack, and a 50 ms release by default. A new note fades the old one over
`DECLICK_FRAMES`. Filter: a state-variable low-pass. Delay: stereo, 3/16 note time,
feedback 0.4. Reverb: Freeverb-style, with 4 combs and 2 allpass filters per channel,
fixed settings. Master: a `tanh` soft clip.

**Audio output.** Use the default output device and its default configuration. If its
format is not `F32`, exit with a message naming the format. Mix to the first two output
channels.

### Target Files to Create/Modify

Each file below is created by the issue that names it, unless the issue says otherwise.

#### Workspace

*   `Cargo.toml`: workspace with the three crates, for shared versions.
*   `.gitignore`: ignores `target/` so builds stay out of Git.
*   `README.md`: build steps, and how to run, to make the project usable.
*   `LICENSE`: GPL-3.0, added when the repository was created. It matches `gmuse`, so code
    can move between the two projects.
*   `docs/designs/vimtracker.md`: this design, moved here when finalized.
*   `docs/g1-result.md`: records the G1 result, because it decides whether work continues.
*   `docs/terminal-notes.md`: records `probe-keys` results for the author's terminal.

#### `crates/vt-model`

*   `Cargo.toml`: crate manifest with no audio or terminal libraries, keeping the model
    testable.
*   `src/lib.rs`: module list and public exports.
*   `src/constants.rs`: the constants table, so values have one source.
*   `src/song.rs`: `Song`, `Track`, `SongRow`, `Chain`, `Phrase`, `Table`, `Groove` and
    `Instrument`, which are the shared data shapes.
*   `src/cell.rs`: `Note`, `Cell`, `TableRow`, `FxCommand` and `FxSlot`, with value
    ranges and persistence, so the editor and engine agree on them.
*   `src/rowtext.rs`: row text parser and printer, for files and tests.
*   `src/selection.rs`: motions and targets resolved to cell ranges, so grammar logic
    needs no terminal.
*   `src/action.rs`: `Action` data and `apply(&Song, &Action) -> Result<Outcome>`, the only
    path that changes a song.
*   `src/history.rs`: undo and redo version list, which uses shared versions.
*   `src/curve.rs`: bias, gain and keyframe interpolation, kept pure so it can be tested.
*   `src/generator.rs`: masks, value sources, `GeneratorLink` and rendering, for G11.
*   `src/scale.rs`: scales and note snapping, used by note ramps.
*   `src/timeline.rs`: row timing and the mid-song position function, for G10.
*   `src/project.rs`: folder loading and saving, version checks and sample copying.
*   `src/sample.rs`: WAV loading, conversion to stereo `f32` and resampling.
*   `tests/*.rs`: one test file per issue area, to keep each behaviour specified.

#### `crates/vt-engine`

*   `Cargo.toml`: crate manifest depending on `vt-model` and `rtrb`.
*   `src/lib.rs`: public `Engine`, `EngineHandle` and `render`.
*   `src/link.rs`: version publishing, return, command and telemetry queues, for
    lock-free edits and transport.
*   `src/engine.rs`: the block loop, which processes events on exact samples.
*   `src/playhead.rs`: per-track position and version-change rules.
*   `src/fx.rs`: the single FX interpreter used by phrases and tables.
*   `src/table.rs`: per-column table playheads.
*   `src/track_state.rs`: fixed-size per-track state, which needs no allocation.
*   `src/voice.rs`: sample playback and envelope.
*   `src/dsp/filter.rs`, `src/dsp/smoother.rs`, `src/dsp/delay.rs`, `src/dsp/reverb.rs`:
    effects, one per file so each can be tested alone.
*   `src/mixer.rs`: track mix, sends and master clip.
*   `src/rng.rs`: small seeded generator (PCG32), so renders are repeatable.
*   `src/render.rs`: offline WAV rendering, for G9.
*   `tests/*.rs` and `tests/golden/*.txt`: behaviour tests and saved render hashes.
*   `tests/support/alloc.rs`: the counting allocator for the real-time safety test.

#### `crates/vt-tui`

*   `Cargo.toml`: crate manifest, with the `vimtracker` binary.
*   `src/main.rs`: subcommands and terminal setup and restore.
*   `src/app.rs`: main loop and application state.
*   `src/audio.rs`: `cpal` stream, which connects the engine to the device.
*   `src/input/mode.rs`, `src/input/keys.rs`: mode state machine and key parsing.
*   `src/input/command.rs`: `:` command parser.
*   `src/input/picker.rs`: page picker state and matching, kept separate from drawing so
    tests need no terminal.
*   `src/views/picker.rs`: draws the floating picker window.
*   `src/shape.rs`: Shape mode preview session, for G5.
*   `src/views/phrase.rs`, `chain.rs`, `song.rs`, `table.rs`, `instrument.rs`, `groove.rs`,
    `score.rs`, `wave.rs`, `status.rs`: one view per file.
*   `src/probe.rs`: the `probe-keys` subcommand, which tests real key events.

### Milestone M1: Hear a loop

*   **Issue 1: Workspace.** Create the workspace files and empty crates in the existing
    repository, `gabrielgellner/vimtracker`.
    **Done when:** `cargo test` passes on the empty workspace.
*   **Issue 2: Key probe.** Add `probe.rs` and `main.rs`. Print each key event, and try
    enabling the Kitty keyboard flags. Ctrl-C exits and restores the terminal. **Done
    when:** the author runs it in their terminal, with and without tmux, and records
    whether release events, `Ctrl-A`, `Ctrl-X`, `Ctrl-O`, `Ctrl-I`, `Ctrl-R`, `Ctrl-N`,
    `Ctrl-P`, `Ctrl-D`, `Ctrl-U`, `Ctrl-E` and `Ctrl-Y` arrive in
    `docs/terminal-notes.md`.
*   **Issue 3: Model and row text.** Add `constants.rs`, `cell.rs`, `song.rs` and
    `rowtext.rs`. Each `Song` map uses `BTreeMap<u32, Arc<T>>` and tracks use a `Vec`.
    **Tests:** every field form round-trips; invalid notes, widths and unknown commands
    give errors.
*   **Issue 4: Project files.** Add `project.rs` and `sample.rs`. Save using a temporary
    file and rename. **Tests:** a song round-trips with a nested generator link; a
    different version gives an error naming both versions; a 16-bit mono WAV loads as
    stereo at a requested rate; a missing sample is an error that names the path. WAV
    fixtures are generated inside tests.
*   **Issue 5: Engine and render.** Add `engine.rs`, `playhead.rs`, `track_state.rs`,
    `voice.rs`, `mixer.rs`, `rng.rs` and `render.rs`, with no FX commands yet. Voices store
    instrument IDs, not `Arc`s, and find samples through the current version.
    **Tests:** a note on a given row starts on its exact sample at 120 BPM; a new note
    fades the old one; two renders match by hash; the same song renders at 1, 8 and 64
    tracks.
*   **Issue 6: Publishing.** Add `link.rs`. A published version holds an `Arc<Song>` and
    an `Arc<SamplePool>`. Growing tracks sends a preallocated `Vec<TrackState>`, into which
    the callback moves existing states before returning the old `Vec`. The callback takes
    a new version only when the return queue has room. **Tests:** the counting allocator
    sees no allocations or frees in `process` across 1,000 publishes, track growth and
    phrase removal; the playhead rules in the Technical Plan hold.
*   **Issue 7: Minimal program.** Add `app.rs`, `audio.rs`, `input/mode.rs`, `input/keys.rs`,
    `input/command.rs`, `views/phrase.rs` and `views/status.rs`. Support phrase view,
    `h j k l`, Insert note entry, `Enter`, `Space x`, `Space c`, `Space qq`, `:new`, `:sample`,
    `:w`, `:e`, `:q` and `:q!`. Keys come from the binding table. The status line shows
    CPU load, dropouts and mode. A test sends two play-or-stop commands in one block and
    checks they cancel out. Telemetry drops messages
    when its queue is full. **Done when:** the author can write a loop and hear it, and
    an edit during playback is heard on the next pass.

### Milestone M2: Editing grammar

*   **Issue 8: Actions and history.** Add `action.rs` and `history.rs`. All edits go through
    `apply`. **Tests:** `u` and `Ctrl-R` restore exact versions; unchanged phrases stay the
    same `Arc`; history stops at `HISTORY_LIMIT`.
*   **Issue 9: Motions and operators.** Add `selection.rs` and key handling for the Normal
    mode grammar, viewport keys, counts and Visual modes. **Tests:** table-driven key sequences, such as
    `3>ib` and `d4j`, starting and ending as row text.
*   **Issue 10: Repeat, registers and values.** Add `.`, `"a` to `"z`, `p`, `+`, `-`, `Ctrl-A` and
    `Ctrl-X`. `.` replays the last changing action at the cursor. **Tests:** `.` after
    each operator; values clamp to their ranges; registers keep separate contents.
*   **Issue 11: Song structure.** Add `views/chain.rs`, `views/song.rs`, `views/instrument.rs`,
    chain transpose, `gd`, `Ctrl-O`, `Ctrl-I`, `grr`, `gc`, `gC`, the `]p [p ]r [r` family,
    `W B`, `gt`, `gT`, `Space ?`, the shared marker, and `:track add`,
    `:track del`, `:mute` and `:solo`. A row in this milestone plays until its longest
    chain ends, then resets. Add the leader key and page picker in `input/picker.rs` and
    `views/picker.rs`. **Tests:** cloning leaves the original unchanged; a deep clone
    copies the chain's phrases; added tracks save, load and play; `sg` matches `song` but
    not `groove`; results follow the specified sort order; an empty search lists pages by
    most recent use; `Esc` leaves the view unchanged. **Done when:** CUJ 4 and CUJ 5 work.

### Milestone M3: Sweeps, then the G1 test

*   **Issue 12: FX and audio effects.** Add `fx.rs`, `dsp/*` and the phrase behaviour of
    `DEL KIL OFF RET REP CHA RND VOL PAN CUT RES DLY REV`, plus FX entry in Insert mode.
    **Tests:** each command has a test on a rendered result or engine state; filter and
    volume changes pass through the smoother; a fixed seed gives the same `CHA` and `RND`
    results; adding a track does not change another track's random results.
*   **Issue 13: Curves.** Add `curve.rs` and `scale.rs`. Ramps use anchors, and a segment
    between anchors runs from 0 to 1. **Tests:** end values are exact in both directions;
    curves stay monotonic; over 9 rows from `00` to `7F`, `bias` 0.8 gives
    `00 2E 49 5A 66 6E 75 7B 7F` and `gain` 0.2 gives `00 05 0D 1B 40 64 72 7A 7F`; notes
    snap upward to the scale; mixed FX commands in anchors are refused; a selection whose first
    or last cell is empty is refused with a message.
*   **Issue 14: Generator links.** Add `generator.rs` with a ramp value source and an
    "all steps" mask. A phrase has a `Vec<GeneratorLink>`, each holding parameters, seed
    and owned cells. `apply` removes any link whose owned cells it changes. **Tests:**
    the link rules in the Technical Plan; a rerender changes owned cells only; undo
    restores the link; two links on different columns are independent.
*   **Issue 15: Shape mode.** Add `shape.rs` and `Visual` `=`. Each change renders and
    publishes a preview without adding history. `Enter` adds one history entry and `Esc`
    restores the starting version. `gv` then `=` on exactly a linked range resumes with its
    settings. `.` applies the last shape to the current selection. **Done when:** CUJ 2 and
    CUJ 8 work with audio, and the author judges the smoothing and FX persistence by ear,
    updating Open Questions.
*   **Issue 16: Render command.** Add `vimtracker render` and `:render`. **Tests:** golden
    hash tests run at fixed sample rate; changing DSP on purpose updates hashes through
    `VT_BLESS=1`. **Done when:** CUJ 6 works.
*   **Issue 17: G1 test.** The author recreates a 16-bar loop in vimtracker and in their
    usual tool, and records times and a preference in `docs/g1-result.md`. Because tables
    and the score view are not built yet, the record must also say whether the single-track
    view was what held it back. Decide to continue, redirect or stop.

### Milestone M4: Tables, grooves and looping (only if G1 passes)

*   **Issue 18: Tables.** Add `table.rs`, `views/table.rs`, and `HOP`, `THO`, `TIC`, `TBL`, `TBX`
    and `IFC`. Each table has 16 rows by default and any length of 1 or more. Each of its
    five columns (transpose, volume and three FX) has a playhead. **Tests:** a row of `HOP`
    loops a column; `TIC00` columns advance per note and keep position; three columns with
    different loop points stay independent; an instrument table restarts on a note; aux
    tables run alongside it. **Done when:** CUJ 3 works.
*   **Issue 19: Grooves and track speed.** Add `views/groove.rs`, the `GRV` command (`GRV xx`
    sets the track's groove and holds), and per-track speed. **Tests:** groove `07 05`
    gives the expected sample positions; all seven speeds give whole ticks per row.
*   **Issue 20: Row timing.** Add `timeline.rs` and row length, repeat count, Reset or Carry,
    tempo and mute fields. **Tests:** CUJ 7 in both modes; starting mid-song matches the
    samples from playing through; a Carry position wraps to a shorter chain.

### Milestone M5: Score view (only if G1 passes)

*   **Issue 21: Score view.** Add `views/score.rs`. It shows every track for the current song
    row and scrolls sideways. Selections stop at phrase boundaries. `za`, `zc` and `zo` toggle,
    fold or open a track's columns down to its notes, and `zM` and `zR` fold or open all
    tracks. **Tests:**
    editing through it matches editing the phrase directly; a shared phrase displays its
    marker on each use. **Done when:** 16 tracks are readable in a 120-column terminal with
    tracks folded.

### Milestone M6: Waveforms and export (only if G1 passes)

*   **Issue 22: Waveform view.** Add `views/wave.rs`, drawing peaks with Braille characters,
    with the playhead shown. **Tests:** the peak reduction function matches known values.
*   **Issue 23: JSON export.** Add `vimtracker export-json <project> [out.json]`. **Tests:** the
    JSON reads back into an equal `Song`.

Slices and generators other than the sweep need their own short design once M4 has shown
how tables feel in practice, because that may change what they should do.

### Verification Plan

*   **Compilation/Build:** `cargo build --workspace`
*   **Unit and integration tests:** `cargo test --workspace`
*   **Static analysis and linting:** `cargo clippy --workspace --all-targets -- -D warnings`
    and `cargo fmt --check`
*   **Test coverage requirement:** no percentage target. Every rule in Fixed Specifications
    and every "Tests" line above has at least one test.
*   **E2E/Integration:** golden render tests, plus the "Done when" manual checks.
*   **Performance (G8):** 16 active sample tracks for 10 minutes on the author's laptop, with
    the dropout counter still at zero, recorded in `docs/g1-result.md`.

--------------------------------------------------------------------------------

*Drafted with EGM*
