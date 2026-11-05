# vimtracker

*   **Author:** gabrielgellner
*   **Status:** Proposed
*   **Date:** 2026-11-05

--------------------------------------------------------------------------------

## Overview

vimtracker is an experimental music tracker that runs in a terminal, is written in Rust,
and is edited with a keyboard grammar modelled on Neovim. It plays samples on a laptop.
It takes ideas from the Dirtywave M8, Elektron sequencers, the Polyend Tracker and
Jeskola Buzz, but it is a new program. It does not port their code, copy their
appearance, read their files or try to match their sound.

The first goal is to find out whether this can become an instrument the author wants to
pick up. The prototype is built to answer that as cheaply as possible, given the premise
below, while avoiding decisions that would be expensive to undo.

## The Problem

Hardware grooveboxes such as the M8 and Elektron boxes give the author a flow state that a
keyboard-and-DAW setup does not. They are immediate, focused on one task, and quick to use
without a mouse. Ableton Live stays the author's DAW. This project is another fun way to
make music, alongside it, as hardware is.

The question is whether a Neovim-style keyboard grammar in a terminal can provide some of
that feeling on a laptop. It may not. A laptop keyboard is not a dedicated instrument with
physical controls, and that is a legitimate way for this to fail.

Renoise and similar software trackers are capable, but to the author Renoise's interface
feels like a keyboard plus a DAW, which is what Ableton already provides. That is the
author's experience, not a measured comparison.

**Terminal-first is the author's premise.** It is part of what is being tried, not something
the test decides.

The project rests on three ideas working together:

1.  An editing language that combines an operator with a target, such as "transpose this
    beat" or "clear this column", so a small set of keys covers many edits and any edit can
    be repeated.
2.  Shaping sweeps by keyboard, in the grid, while the loop plays. DAW automation envelopes
    offer similar curve editing, using a mouse and a separate editor. This is not claimed to
    be new, only quicker and closer to the grid.
3.  Shared phrases, reachable from that same editing language.

Per-note tables are a further idea, and they get their own decision after G1.

## Glossary

*   **Track:** one voice's lane through the song. Each track plays one note at a time.
*   **Section:** one row of the song, holding one chain per track.
*   **Chain:** a list of phrases, each with a transpose.
*   **Phrase:** one track's sequence of steps, reusable by ID.
*   **Step:** one position in a phrase. One step is a sixteenth note at normal speed.
*   **Field:** one editable part of a step: note, velocity, instrument, or a slot's command or
    value.
*   **Slot:** one FX command and its value. Column group means a slot, or the note, velocity
    or instrument column.
*   **View:** a screen you can jump to.
*   **Operator:** a key that combines with a motion or target. **Action:** the recorded edit
    it produces.
*   **Live / applied:** a generator's state. A live generator can still be changed. Applying
    it leaves plain values.
*   **Curve bias** (the "onset" control) and **curve gain** (the "S-shape" control). "Gain"
    alone means signal level.

## Goals

*   **G1: Does it become an instrument the author wants to pick up?** After M3, the author
    uses vimtracker to make new loops, over at least 5 sessions across 2 weeks, with no
    vimtracker development during those sessions. The reference for feel is the M8 and an
    Elektron box. After each session the author writes a few lines: what broke the flow, what
    they reached for that wasn't there, and whether they would rather have been on hardware.
    *   **Continue:** the author opened it by choice, not to test it, in at least 3 sessions;
        at least once they stopped thinking about the keys; and at least 2 loops were good
        enough to keep or bring into Ableton.
    *   **Redirect:** the flow is there, but one missing thing keeps breaking it. Build that
        next, starting with tables if they are what was missed, then run the test once more.
    *   **Stop:** it feels like work, or the author stops opening it, without one fixable cause.
    *   **Inconclusive:** too few sessions happened to judge. Extend it once.
*   **G2: Editing grammar.** Operators combine with motions and targets, counts work, `.`
    repeats the last change, `u` and `Ctrl-R` undo and redo, and registers hold copied steps.
*   **G3: Reusable song structure.** Every track uses one shared grid model. A song is built
    from sections, chains and phrases referred to by number. A phrase can be used in several
    places, it shows when it is shared, and it can be cloned. Polyphony uses several tracks.
*   **G4: Live sweeps.** A selected range in one column can be filled with a curve. The user
    adjusts its onset and S-shape while the loop plays. The sweep stays live, and can be
    adjusted again, until a hand edit to a column it owns or an explicit command applies it.
    Each curve reaches its exact end values and behaves the same rising or falling.
*   **G5: Track count is not fixed at 8.** A project chooses its track count, up to a limit the
    code can raise by changing one constant, set to 64. The file format has no limit. A new
    project starts with 8 tracks, and the core tests pass with 1, 8 and 64 tracks. FX commands
    address a limited range, which is settled when tables are designed.
*   **G6: Terminal first.** The program runs in a normal terminal, including inside tmux, and
    graphics are drawn with text characters. No core feature depends on key release.
*   **G7: Steady timing.** Sequencing does not depend on redraw timing. On the author's laptop,
    16 active tracks run for 10 minutes while editing and redrawing, with a late-block count of
    zero, no stream errors, and nothing audible.
*   **G8: Rendering to WAV.** A project can be rendered to a WAV file with no terminal or sound
    device, which is also how a jam reaches Ableton. The same project and seed produce identical
    samples on the same build.

## Non-Goals

*   Replacing Ableton Live or any other DAW.
*   Porting code from other trackers.
*   Reading or writing the file format of any other tracker or DAW.
*   Backward compatibility while prototyping. A format change can make older projects
    unloadable. Files carry a version number so the program can say why, and rendering to WAV
    is how to keep a jam.
*   Track types, such as chord or stage-sequencer columns. Chords and stage sequences are
    generators that write to ordinary tracks.
*   Matching the sound of any other tracker exactly.
*   Using the names, logos or visual design of Polyend, Dirtywave or Elektron products.
*   Synth engines, granular or wavetable playback, sample recording, streaming long samples,
    MIDI, plugin hosting and live performance mode in the prototype.
*   Image graphics. Text-mode graphics come first. Kitty's image protocol remains possible
    later if text cannot do what is needed.
*   Live-coding, where running code generates the music during playback.
*   Official support for Linux or Windows. The code avoids depending on macOS without a reason,
    but only macOS is tested.

## Open Questions

*   **Key input:** Which keys do the author's terminal and tmux setup deliver? Issue 2 tests this.
*   **Track capacity:** How many active tracks does the author's laptop sustain? Issue 23 measures it.
*   **Sweep smoothing:** Do sweeps sound stepped on a sustained sample? Issue 15 ends with a
    listening test, and the Audio path section states the fallback.
*   **FX persistence:** Mixer and filter values hold until changed, and note events apply once.
    Elektron parameter locks appear to revert after their step. This needs confirming by ear,
    particularly for sparse values.
*   **Braille graphics:** Can text-mode graphics deliver the desired feel, such as an old MPC
    screen or an M8-style scope? A small throwaway demo can answer this before Issue 20.

## Critical User Journeys (CUJs)

### CUJ 1: Write a loop

*   **Given**: A new project with 8 tracks and a folder of WAV files.
*   **When**: The user assigns samples to instruments, enters notes on several tracks using the
    keyboard, and presses `Enter`.
*   **Then**: The loop plays in time, a playhead follows it, the other tracks' notes are visible
    beside the focused track, and edits during playback are heard on the next pass.

### CUJ 2: Edit with the grammar

*   **Given**: A phrase with notes on every beat in the first bar and the cursor on a note.
*   **When**: The user presses `V`, selects a beat with `ib`, presses `>` to transpose it, moves
    to the next beat, selects it, and presses `.`.
*   **Then**: Both beats are a semitone higher, with empty fields and `OFF` unchanged, and `u`
    undoes one change at a time. `3+` on one value adds 3, and `d3j` clears four steps in the
    current column without touching other columns.

### CUJ 3: Shape a filter sweep

*   **Given**: A looping phrase with `CUT` values at the first and last steps of a selected column.
*   **When**: The user presses `=`, adjusts onset and S-shape, then presses `Enter`.
*   **Then**: Every adjustment is heard during playback and shown in the column. `Enter` closes the
    controls and leaves the sweep live, one `u` restores the original values, and `Esc` cancels
    with no change. `.` applies the same shape to another selection.

### CUJ 4: Adjust a live sweep, then take over by hand

*   **Given**: A phrase with a live sweep in its `CUT` column, marked in the grid.
*   **When**: The user moves away and returns, presses `gl` on the column and reshapes it, and later
    edits one of its fields by hand.
*   **Then**: Reshaping rerenders the column while the loop plays. The hand edit applies the sweep,
    leaving plain values, and the status line says so. `u` restores both the old value and the live
    sweep. Editing notes in the same phrase does not apply it. `Space ga` applies it explicitly.

### CUJ 5: Reuse and vary a phrase

*   **Given**: A phrase used in two chain slots.
*   **When**: The user jumps into it from a chain with `gd`, sees that it is shared, clones it with
    `gc` and edits the clone, then presses `Ctrl-O`.
*   **Then**: The edit affects only the clone, the other use is unchanged, and the cursor returns to
    where the jump started.

### CUJ 6: Add more tracks

*   **Given**: A project using all 8 of its tracks.
*   **When**: The user adds tracks beyond 8 and writes parts on them.
*   **Then**: New tracks can be navigated, played, muted and saved like the first 8. The display
    scrolls sideways when they do not fit, and CPU load is visible.

### CUJ 7: Bring a loop into Ableton

*   **Given**: A saved project with a loop the author likes.
*   **When**: The user runs `:render loop.wav`.
*   **Then**: The WAV is exactly the loop's length, with delay and reverb already filled from a silent
    first pass, so it loops cleanly in Ableton. The same project and seed give identical samples on
    the same build, including with no display or sound device.

## Technical Plan (Architecture)

### Summary

The program has one thread that owns and edits the song and one audio thread that plays it. The
edit thread never changes a published song version. Each edit creates a new version that shares
all unchanged data with the old one, and sends it to the audio thread. Sequencing happens inside
the audio callback, so screen redraws do not affect timing.

```text
 Keyboard
    |
    v
+--------------------------+   new song version     +-------------------------+
| Edit thread (vt-tui)     |----------------------->| Audio callback          |
|                          |   commands: play,      | (vt-engine)             |
| modes -> actions         |   pause, return,       |                         |
| song + undo history      |   preview, mute, solo  | playheads               |
| generators, file saving  |----------------------->| FX interpreter          |
| screen drawing           |   old versions, to     | voices -> mixer -> out  |
|                          |<----------------------- be freed here            |
|                          |   playhead, CPU load,  |                         |
|                          |<----------------------- late count, scope audio  |
+--------------------------+                        +-------------------------+
```

The same engine runs without a sound device to render a song to WAV.

### Components

| Crate | Owns | May depend on |
|---|---|---|
| `vt-model` | Song data, actions, generators, row text format, saving and loading, sample decoding | No audio device or terminal libraries |
| `vt-engine` | Sequencing, FX interpreter, voices, mixer, offline render | `vt-model` |
| `vt-tui` | Modes and key handling, views, audio device, the `vimtracker` program | `vt-model`, `vt-engine` |

### Song model

```text
Song
 |- format_version, bpm (thousandths), seed, step, swing
 |- tracks[]          mute, solo, optional swing override
 |- sections[]        one chain ID per track
 |- chains{id}        list of (phrase ID, transpose)
 |- phrases{id}       steps[], generator links[]
 '- instruments{id}   sample path, volume, attack, release
```

*   **Limits.** Tracks are a list. Chains, phrases and instruments are maps from 32-bit ID to value.
    A label is a two-character name that is itself a slot. A hex name like `BD` is that number, and
    any other, like `CH`, has a fixed slot above the numbered ones that is never shown as a number,
    so a label means the same slot in every kit and song. Instrument IDs widen past two hex digits
    when needed. Phrases can have any length of 1 step or
    more, and new phrases default to 16 steps. The engine prepares state for `MAX_TRACKS`.
*   **Steps.** A step has a note or note-off, a velocity, an instrument and a list of FX slots. The list
    has 3 slots for now, and it is saved as a list so the count can grow without changing the format.
*   **Reuse.** Chains refer to phrases by ID. A shared phrase displays a marker, and an edit changes
    every use. Clone creates a new ID, using the lowest unused number.
*   **One voice per track.** A new note on a track replaces the note already playing.
*   **Sections before G1.** A project has one section. Shorter chains loop until the longest chain
    ends, then all tracks restart together.

### Passing edits to audio

1.  An action takes the current song and returns a new song, copying only changed items and their
    parents. Unchanged items are shared through `Arc`.
2.  The edit thread publishes the new version through a queue of depth 1. If a newer version arrives
    while one is waiting, it replaces the waiting one, and the edit thread frees the old one.
3.  At the start of each block, the callback takes a waiting version only if the return queue has
    room, and sends the version it replaces back.
4.  The edit thread frees returned versions. The callback never allocates, frees or takes a lock.
    One top-level `Arc` wraps the song and sample pool, and the callback returns it without dropping it.

**Per-track playback state.** Playheads, voices and FX state belong to the audio thread, in storage
for `MAX_TRACKS` made ahead of time.

**When a new version changes what is playing.** A playhead stores IDs and a position, and it checks
them when a new version arrives:

*   If the phrase got shorter, the position wraps to fit.
*   If an ID no longer exists, which undo can cause, the track is silent until the section restarts.
*   If the track no longer exists, its voice stops with a short fade. Before G1, only the last track
    can be removed, so positions stay stable.

**Samples.** The pool only grows during a session, and sample IDs stay stable. A voice stores its
sample ID when it starts and keeps it until it ends, so replacing a sample affects new notes only.
Sample reads are bounds-checked, and a voice past its sample's end stops. Replaced samples stay in
memory until the project is reloaded.

### Transport

Before G1 there is one scope: the current section, on all tracks, looping. Transport actions are
commands that the engine resolves against its own state, so two quick presses cannot act on an
out-of-date view. `Enter` plays from the cursor's step when stopped, and stops when playing. `Space x`
pauses in place and resumes. `Space c` returns to the start, and stops if playing.

Playing from the cursor does not chase held FX values or tails. The cursor and playhead are
independent, so playback never moves the cursor.

**Note preview.** Entering a note sends a preview command. The engine has one dedicated preview voice,
which plays through the focused track's held filter, pan and volume, without running that step's FX. A
preview never cuts a sequenced note, and a sequenced note never cuts a preview.

**Mute and solo.** These are live controls, not edits. They go through the command queue, take effect
immediately, and are not in undo history. Their state is saved with the project.

### Time

The song clock runs at 960 ticks per quarter note, which is 240 ticks per step. Tempo is stored as an
integer in thousandths of a BPM, from `20.000` to `400.000`, so a break's tempo can be matched closely.
An event's sample is calculated from the tick where the current tempo began, and never accumulated:

```text
sample = floor(tick x 60 x rate x 1000 / (960 x millibpm))     using 128-bit integers
```

**Swing** uses the MPC scale of 50-75%, which is the share of time given to the first of each pair of
sixteenths. 66.7% is triplet feel. Project swing applies to all tracks, and a track can have its own
value instead, as on the M8, where a track uses one groove. Nothing combines. Odd-numbered steps counted
from the section start are delayed by `round(480 x swing% / 100 - 240)` ticks, and the step's FX move
with its note. Because pairs count from the section start, a short looping phrase swings different steps
on each pass.

Whole-tick arithmetic holds for straight rows at normal speed. Track speeds and M8-style grooves are
after G1.

### FX interpreter

One interpreter runs FX commands, once per step before G1. It must not assume it is only running for a
phrase, because tables will use it at their own rate. Slots run left to right. Held values start at `VOL
FF`, `PAN 80`, `CUT FF`, `RES 00`, `DLY 00` and `REV 00`, per track. They reset on `Space c` and when
playback starts from the top. `Space x` resume and `Enter` keep them.

**Randomness.** Each track has its own PCG32 generator, seeded from the project seed and the track's index.
Generators restart when playback starts from the top. Because tracks are only added or removed at the end
of the list, this does not change other tracks.

### Audio path

Each track has one voice: one-shot sample playback, a filter, a volume envelope and equal-power pan, with
`80` defined as exact centre. Final gain is step velocity x `VOL` x track level x instrument volume, and `--` in the step
means full. Velocity runs `00` to `7F`, matching MIDI, while the other level values run `00` to `FF`. Track level comes before the sends, so it scales them too, and it belongs on mixer pages
once those exist. Transposed notes clamp to the note range.

Tracks feed a mixer with shared delay and reverb sends, then a fixed master gain of about -6 dB, then a
`tanh` soft clip. Processing is 32-bit float at the device's sample rate. Samples are converted to that
rate when loaded and kept fully in memory. All other tuning values are chosen at implementation, tuned by
ear, and pinned by golden hashes.

**Smoothing hypothesis.** Changes to filter, volume and pan pass through a smoother of about 10 ms. At 120
BPM a step lasts 125 ms, so a full filter sweep may sound stepped on a held sound. Issue 15 tests this by
ear. If it does, a column under a live sweep ramps across each step toward its next value, using the
sweep's own values with no look-ahead into the audio, and hand-entered values keep the 10 ms smoother.

**Scope and meters.** The callback copies a master signal into a preallocated ring buffer that drops frames
when full, so it never waits on the UI. Track levels go through telemetry.

### Editing

Keys go through a mode state machine (Normal, Insert, Visual, Shape, Command, Picker) that produces actions.
Actions are plain data, so `.` can replay them and tests can run editing without a terminal.

*   **Operators and targets.** Operators are `d y > <`. Motions and `ic` act on the current column. `dd yy >>
    <<`, `ib` and `ip` act on whole steps. This differs from Vim on purpose: `dj` clears a column rather than
    whole steps, because editing one column is the everyday tracker action.
*   **Counts.** A count extends the range: `d4j` covers five steps, and `3ib` three beats. With `+` and `-`, a
    count is the amount, on the cursor or selection.
*   **Values.** `>` and `<` move notes by a semitone and other values by 1. They skip empty fields, `OFF`, FX
    names and instrument IDs.
*   **Registers and paste.** `d` and `y` also fill the unnamed register. `p` overwrites from the cursor and clips
    at the phrase end. Empty copied fields overwrite too. A column type mismatch is refused with a message.
*   **Edges and repeat.** Motions stop at the edge. `.` repeats operators, value changes and shapes, in Normal or
    Visual. It does not repeat Insert sessions.
*   **Doubling.** `Space pd` doubles a phrase by repeating its visible steps, as one undo step.
*   **Moving through the tree.** `gd` opens the item under the cursor, and on an empty reference it creates the
    next unused ID, assigns it and opens it. `Ctrl-O` jumps back, `Tab` jumps forward, and `grr` lists uses in the
    picker window.
*   **Cloning.** `gc` copies a phrase to a new ID. If you arrived through `gd`, it repoints that reference and stays
    in the clone. Otherwise it opens the clone, changes no references, and a status message says so. If the parent
    chain is shared, a status message says the change affects all its uses. `gC` on a chain also clones its phrases
    and repoints the section's reference.
*   **Undo.** Undo history is a list of song versions, and undo publishes an older one. It drops the oldest version
    past `HISTORY_LIMIT`. One Insert session and one Shape session are each one undo step.
*   **Views.** Before G1: phrase, chain and instrument, and a help view. The phrase view shows the focused track's
    full steps beside read-only note columns for the other tracks, aligned by step.
*   **Fallback rule.** If a behaviour is not specified and does not affect what G1 tests, follow Vim, or else choose
    the simplest option and record it in the issue.

### Generators

A generator is a pure function of its parameters and target, and later a seed. The live sweep is the first one.
Later generators combine a **mask**, which chooses the steps to change (as with Polyend's Fill tool), with a **value
source** that chooses what to write. Those are built when a second generator needs them.

**Sweep.** A sweep covers one column. Its first and last fields must have values, and interior fields are overwritten.
FX columns must use the same command at both ends. A one-step, multi-column, instrument-column or mixed-command
selection is refused with a message.

**Curve.** `f(t)` applies bias then gain, where `t` runs from 0 to 1 across the selection, and the value is `start +
(end - start) x f(t)`. `bias(t,a) = t / ((1/a - 2)(1 - t) + 1)`, and gain is Schlick's gain built from it. Both start at
0.5, which gives a straight line. Results round half up and clamp to the column's range.

**Live state.** A live generator is saved on the phrase with its parameters, column, range and both end values, so
changing its shape never depends on what is currently in the fields. It owns every column it touches.

*   Changing parameters rerenders it. A rerender never applies it.
*   A hand edit to a field in an owned column applies it, shows a status message, and leaves plain values. Editing any
    other column does not.
*   `Space ga` or `:gen apply` applies the live generator under the cursor.
*   Doubling a phrase, or shortening it past an owned range, applies it. A new sweep overlapping a live one replaces it.
*   Undo restores the live state, because it is part of the phrase. Cloning copies it, and each clone renders on its own.
*   Live columns are marked in the grid. `gl` on a live column reopens its controls, from anywhere and after a reload.

### Saving and loading

A project is a folder holding `song.ron`, and a `samples/` folder once it has been bundled. Samples are referred to where they are, by a full path, or a relative one if they are inside the project. `:w path` copies
samples to a new folder. Saving writes to a temporary file and renames it, so a process crash cannot leave a partial song,
and a failed save leaves the previous file intact. Loading first reads only `format_version` and compares it, then parses
the full `Song`. A different version is refused with a message naming both versions and suggesting rendering to WAV before
updating. There is no migration code. Steps are saved as one readable string per step.

**Autosave.** Every 60 seconds, if changed, `song.autosave.ron` is written beside `song.ron` on the edit thread. On opening,
if it is newer, the program offers to restore it.

### Testing

*   **Editing:** apply actions to a song and compare, using the step text format for inputs and output.
*   **Sound:** render fixed songs at `DEFAULT_RENDER_RATE` and compare FNV-1a 64 hashes of the samples' little-endian bytes.
    Hashes change when DSP changes on purpose, through `VT_BLESS=1`.
*   **Real-time safety:** a per-thread guard counts any allocation or free in `process`, and tests run in release as well as debug.
*   **Track count:** core model and engine tests run at 1, 8 and 64 tracks.

### Risks

| Risk | Effect | Mitigation |
|---|---|---|
| A version is dropped on the audio thread | Rare glitches that are hard to reproduce | The callback returns versions through a queue, and the guard test fails on any free |
| The return queue is full | The callback keeps playing the old version | It takes a new version only when there is room, and this needs a stalled edit thread |
| A CPU spike with many tracks | Dropouts | CPU load and a late count stay on screen, where late means callback time exceeded buffer duration. The check can miss some OS-level underruns, so ears still matter |
| A keystroke applies a live sweep by accident | Generator settings are lost | A status message appears, and `u` restores it |
| An edit changes a shared phrase everywhere | Unintended changes elsewhere | Shared phrases show a marker |
| An old project stops loading | A jam can be lost | Accepted. Rendering keeps the audio, and the message says why |
| Terminal key limits | Keys cannot be held for chords | The design needs no key release, and Issue 2 tests real behaviour |

**Accepted prototype limits.** Losing the audio device exits the program. A missing or unreadable sample is an error naming the
path, and the rest of the project still loads. Memory is not bounded beyond track count and sample size.

## Alternatives Considered

### Port Polyend Tracker CE to Rust

*   **Pros**: A complete feature list and known behaviour, with most of its source under an MIT-based license.
*   **Cons**: About 330,000 lines of platform code and vendor drivers and 58,000 lines of hardware-shaped UI would be dropped anyway.
    The sequencer includes UI headers, and some of its synth and DSP code ships only as closed binaries.
*   **Reason for Rejection**: The question is whether a new way of working feels good, and a port would spend most of its effort on
    things that do not answer it. The code is used as a reference.

### Extend an existing software tracker

*   **Pros**: A finished audio engine and file handling, and possibly a much faster way to test the grammar alone.
*   **Cons**: Terminal-first operation and the song structure would have to fit around another program's model.
*   **Reason for Rejection**: Renoise's interface feels to the author like a keyboard plus a DAW, which Ableton already provides. A grammar
    test inside it would not test the flow this project is after. This is the author's judgement and premise, and it was not tested.

### Track types

*   **Pros**: Chord and stage-sequencer tracks could each have their own editor.
*   **Cons**: Every type must define every editing action, its copy and paste, and its display. Variable-length stages do not line up with grid
    steps. On a polyphonic track, commands such as `KIL` need a rule for which note they act on.
*   **Reason for Rejection**: In the author's judgement, it pulls the program away from being a tracker. Generators that write to ordinary tracks
    give most of the same results with one editor and one engine.

### One grid with chord cells

*   **Pros**: Chord names stay visible in one column, with no extra tracks.
*   **Cons**: Every field and command must handle chords, even on tracks that never use them, and chord names need wider columns.
*   **Reason for Rejection**: Chords across several tracks give each note its own commands and need no new field type.

### Generators as an edit layer

*   **Pros**: Hand edits and generator control could both be kept.
*   **Cons**: Every edit path must write to the layer, and edits can land on steps that no longer make musical sense.
*   **Reason for Rejection**: Hand editing should need no syncing. An edit to an owned column applies the generator, a model close to Elektron's Euclidean mode.

### One-shot generators with remembered settings

*   **Pros**: Nothing to store or keep linked. This is how the Polyend Fill tool works.
*   **Cons**: Settings are not tied to a part, so an older part cannot be adjusted.
*   **Reason for Rejection**: The author wants a generated part to stay adjustable until they take over by hand.

### Growing tracks live

*   **Pros**: No limit in the engine, and no memory for idle tracks.
*   **Cons**: It needs stable track IDs and a state handoff through the real-time path, which reviewers found could lose state when a version is skipped.
*   **Reason for Rejection**: 64 tracks is plenty, and raising the constant is one line and does not affect saved songs.

### Edit messages to the audio thread

*   **Pros**: The audio thread never waits, and each edit is small.
*   **Cons**: Two copies of the song, and each edit type needs code on both sides. A rerender can send many fields at once. None of the programs read for this
    design (Polyend Tracker CE, Furnace, Schism Tracker, MilkyTracker, Ardour, Tracktion Engine) uses this for all song edits.
*   **Reason for Rejection**: Sweep previews need to replace many fields at once, and a bug here would make the sound differ from the screen.

### A mutex around the song

*   **Pros**: Less code and only one copy. Furnace and Schism Tracker use locks in practice.
*   **Cons**: Timing depends on how long the edit thread holds the lock, and moving away later changes how every edit reaches the song. No lock hold times were measured.
*   **Reason for Rejection**: Snapshots avoid the lock, simplify undo and preview, and match the approach in Ardour and Tracktion Engine, at the cost of more setup.

### Other save formats

*   **Binary, as on the Polyend and M8**: Compact, but samples are stored separately, so the size benefit is small, and it cannot be diffed or inspected.
*   **Zipped XML or JSON with samples inside, as in Renoise**: One portable file, but compression saves little, and it gives up `git diff`, `grep` and direct editing.
*   **JSON**: Works with every tool, but has no comments and is fussier to hand-edit.
*   **Folder with one text file per phrase**: Clean per-phrase history, but more code and files can go missing or mismatch.
*   **Reason for Rejection**: For jams with no compatibility requirement, the author prefers one readable RON file.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Performance:** G7 is the target. The audio callback must not allocate, free, lock, do file or terminal I/O, print, or panic.
*   **Security and privacy:** The program reads local WAV and project files. Loaders return errors on bad input rather than panic.
*   **Rollout:** There is none. Older project files can stop loading, as stated in Non-Goals.

--------------------------------------------------------------------------------

### Additional Resources

*   [Polyend Tracker CE](https://github.com/polyend/tracker-ce): feature reference. See `core/source/mtSequencer_patternEdit.cpp` for fill behaviour.
*   [Dirtywave M8 operation manual, June 2023](https://cdn.shopify.com/s/files/1/0455/0485/6229/files/m8_operation_manual_v20230630.pdf)
*   [Working with M8 tables](https://cs.uwaterloo.ca/~plragde/flane/TM8C/Working_with_tables.html)
*   [Digitakt II user manual, OS 1.17](https://www.elektron.se/wp-content/uploads/2026/10/Digitakt-2-User-Manual_ENG_OS1.17_260930.pdf)
*   [Ableton Live 12 MIDI tools](https://www.ableton.com/en/live-manual/12/midi-tools/)
*   [TextBeat](https://github.com/flipcoder/textbeat): chord notation ideas.
*   [MPC manual, swing](https://www.manualslib.com/manual/724425/Akai-Mpc.html?page=92)
*   `gmuse`: the author's own terminal music player, on the same libraries and license. It is the source of the copied files named in the plan, and of the transport and keymap rules. The author expects overlap when DJ work starts there.
*   The author's Neovim keymap conventions guide, the source of the key rules below.
*   [Metropolix manual](https://www.analoguehaven.com/intellijel-designs/metropolix-solo/manual.pdf): needs a full read before a stage generator is designed.
*   Synchronization references: [Ardour `rcu.h`](https://raw.githubusercontent.com/Ardour/ardour/master/libs/pbd/pbd/rcu.h), [Tracktion Engine](https://github.com/Tracktion/tracktion_engine) (GPL or commercial), [Furnace](https://github.com/tildearrow/furnace), [Schism Tracker](https://github.com/schismtracker/schismtracker).

### After G1

The G1 outcome decides what happens next. **Continue** works through this list in order. **Redirect** builds the named missing feature first, then reruns the test once. **Stop** ends the project. **Inconclusive** extends the test once. Each entry gets its own short design, informed by the session notes.

1.  **Tables.** Per-note tables with their own speed, independent column rates, and aux tables, modelled on the M8. The author wants cycles that drift across notes, such as a 5-step table cycle against a 16-step phrase, which the M8's `TIC00` per-note mode allows. Open questions: how transpose and volume columns get a rate and loop point, end-of-table behaviour, when a cycle's position resets, a command that tests whether a note is held (a community M8 tutorial says tables cannot detect note-off, and the manual documents no such test), FX addressing such as `HOP` beyond row `F` and IDs above `FF`, table `00` as an aux table, and a safe rule for zero-time `HOP` loops. First command candidates: `HOP`, `THO`, `TIC`, `TBL`, `TBX`, `RND`, `REP`, `IFC`.
2.  **Sample slicing and waveform view.** Chopping a break into slices, triggering them from the grid, `:bpm fit <bars>` to set the tempo from a sample's length, and Braille waveforms. The M8 and Polyend are the references.
3.  **Score view.** Full multi-track editing, column folding (`za zc zo zM zR`), and selections across tracks.
4.  **Song view and looping.** Song view and section navigation. Unequal looping with the author's decision: each section has a length, tracks loop until it ends, and a section either **resets** tracks or **carries** their positions on, like Elektron's `RESET` at `INF`. Any mid-song guarantee covers track positions only. Also per-track speed, M8-style grooves with `GRV`, how groove units scale with speed, a rounding rule, and sections ending mid-step.
5.  **Generators.** Masks and value sources, Euclidean with two pulse generators and boolean operators, stage sequences, chords across tracks with strum, and transformers modelled on Ableton's MIDI tools. When one generator writes to several tracks, decide whether editing one applies them all.
6.  **Smaller items.** Chasing held values on play, scales and `:set scale`, a fine-timing nudge command, keyframes in sweeps, track groups, macros, marks, `/` search, sequence increments across cursors, Harpoon-style assignable view slots, Elektron-style conditions and fill mode, swing at group level as in Maschine, MIDI, device recovery, a portable project file, and `vimtracker export-json <project> [out.json]`, which an earlier test showed round-trips between RON 0.12.3 and JSON. Comments are not kept.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is split into issues, each intended as one pull request. Every issue leaves the workspace building with `cargo test` passing. **Work stops at Issue 23 until the G1 outcome is recorded.** The rest of the work is listed under After G1. The readiness bar applies to M1-M3 only.

### Fixed Specifications

These apply to every issue. Change them by updating this document.

**Toolchain and dependencies.** Rust 2024 edition. `gmuse` builds with most of these versions. A separate test confirmed the `ron` version with `serde`'s `rc` feature, including an `Arc` round trip. The `rc` feature writes each `Arc` in full and does not preserve sharing on reload. This is safe because reuse is by ID, so a shared phrase is stored once. Issue 1 confirms the full set builds together.

| Crate | Version | Used by |
|---|---|---|
| `serde` (with `derive`, `rc`) | 1.0.229 | `vt-model` |
| `ron` | 0.12.3 | `vt-model` |
| `hound` | 3.5.1 | `vt-model`, `vt-engine` |
| `rubato` | 5.0.0 | `vt-model` |
| `thiserror` | 2.0.20 | `vt-model`, `vt-engine` |
| `rtrb` | 0.4.1 | `vt-engine` |
| `ratatui` | 0.30.2 (use its `ratatui::crossterm` re-export) | `vt-tui` |
| `cpal` | 0.18.2 | `vt-tui` |
| `clap` (with `derive`) | 4.6.6 | `vt-tui` |
| `nucleo-matcher` | 0.3.1 | `vt-tui` |

**Design constants**, in `vt-model/src/constants.rs`. Changing these needs a document update.

| Constant | Value | Meaning |
|---|---|---|
| `FORMAT_VERSION` | 1 | Current `song.ron` format |
| `MAX_TRACKS` | 64 | Tracks the engine prepares state for |
| `DEFAULT_TRACKS` | 8 | Tracks in a new project |
| `TICKS_PER_STEP` | 240 | Clock ticks per step, at 960 per quarter note |
| `GROOVE_UNIT_TICKS` | 40 | One unit for `DEL`, `KIL`, `OFF` and `RET`, 6 per step |
| `DEFAULT_PHRASE_STEPS` | 16 | Steps in a new phrase |
| `BEAT_STEPS` | 4 | Steps per beat, for `ib` and `{ }` |
| `FX_SLOTS` | 3 | FX slots per step |
| `DEFAULT_BPM` | 120.000 | New project tempo |
| `HISTORY_LIMIT` | 1000 | Undo versions kept |
| `DEFAULT_RENDER_RATE` | 44100 | Hz, for tests and `render` |
| `PUBLISH_QUEUE` | 1 | Song versions waiting for the callback |
| `RETURN_QUEUE` | 16 | Versions waiting to be freed |
| `COMMAND_QUEUE` | 64 | Transport, preview and mute messages |
| `TELEMETRY_QUEUE` | 64 | Playhead and load messages; newest dropped when full |

**Tuning values** can change by ear with no document update: `SMOOTHING_MS` (10), `DECLICK_FRAMES` (64), `PREVIEW_MS` (250), `FRAME_MS` (33), `AUTOSAVE_SECONDS` (60), and master gain.

**Step text format.** Fields are separated by one space, and an FX slot is a command and value with no space.

```text
Step:  NOTE VOL INST FX FX FX     C-4 64 01 CUT40 ---00 RET14
```

*   `NOTE`: `C-4` or `C#4`, from `C-0` (MIDI 12) to `G-9` (MIDI 127). `C-4` is MIDI 60. `---` is empty and `OFF` is a note-off. MIDI 0-11 are not available.
*   `VEL`: `00` to `7F`, or `--` for full. Gain is the value divided by 127.
*   `INST`: 2 or more hex digits, or `--`. A note with `--` keeps the track's previous instrument, and is silent if there is none.
*   `FX`: a three-character uppercase command and a two-digit hex value. `---00` is empty.

**Project format.** `song.ron` serializes `Song`, and `format_version` can be read alone. Samples are stored by path relative to the project folder. `:new` and launching with no argument create one section, one chain and one 16-step phrase per track, plus instrument `00` with no sample. Without a path, the project is unsaved until `:w path`.

**Keys.** The binding table is the single source for keys, including what `Space k` shows. Any key that Issue 2 finds does not arrive gets a replacement recorded there.

| Mode | Key | Action |
|---|---|---|
| Normal | `Space` | Leader. It waits for the next key with no timeout. After 300 ms a popup lists the next keys, as which-key does. `Esc` cancels silently, and an unbound key cancels with a status message |
| Normal | `Space Space`, `Space ff` | Open the view picker |
| Normal | `Space 1` to `Space 6` | Jump to song, chain, phrase, table, instrument or groove view, when that view exists |
| Normal | `Space ?` | Open a guide: a short tutorial built into the program |
| Normal | `Space k` | Show every key, generated from the binding table |
| Normal | `Space qq` | Quit, the same as `:q` |
| Normal | `h l` | Previous or next field |
| Normal | `0 $` | Start or end of the row. After an operator, they cover this step from the cursor's column to that end, including the cursor's column |
| Normal | `j k` | Next or previous step |
| Normal | `w b` | Next or previous column group |
| Normal | `W B` | Next or previous track |
| Normal | `{ }` | Previous or next beat |
| Normal | `gg G` | First or last step |
| Normal | `H M L` | Cursor to the top, middle or bottom of the view |
| Normal | `Ctrl-D`, `Ctrl-U` | Half a screen down or up, keeping the cursor mid-screen |
| Normal | `Ctrl-E`, `Ctrl-Y` | Scroll the view down or up 3 steps without moving the cursor, unless it reaches the edge |
| Normal | `]p [p`, `]P [P` | Next or previous phrase in the chain; last or first phrase |
| Normal | `d y > <` + motion or target | Clear, copy, or shift values |
| Normal | `dd yy >> <<` | Apply to the current step |
| Normal | `x` | Clear the current field |
| Normal | `p` | Paste over from the cursor |
| Normal | `K` | On the Instrument column, choose this step's instrument from those that exist, labels first |
| Normal | `;` | Audition: hear this row on every track while not playing, with no FX, changing nothing playback relies on. In the chain view, the start of that row. In the instrument view, that instrument at C-4 |
| Normal | `s` | Clear this field and start Insert there, as in Vim. An FX value keeps its command for the value typed next. `.` doesn't repeat it |
| Normal | `r` | Replace this field as in Vim, with no move, and return to Normal. A note takes one note key and keeps its velocity and instrument. Other columns take a whole value, and an FX command goes on to its value. `.` repeats it |
| Normal | `"a` before `d`, `y` or `p` | Use register `a` to `z` |
| Normal | `+`, `-` | Increase or decrease the value. `Ctrl-A` and `Ctrl-X` do the same where the terminal delivers them |
| Normal | `.`, `u`, `Ctrl-R` | Repeat, undo, redo |
| Normal | `gd`, `Ctrl-O`, `Tab`, `grr` | Open item, jump back, jump forward, list uses. `gr` is only a prefix |
| Normal | `gc`, `gC` | Clone, deep-clone |
| Normal | `gv` | Reselect the last selection |
| Normal | `gl` | Reopen controls for the live generator under the cursor |
| Normal | `Space ga` | Apply the live generator under the cursor |
| Normal | `Space pd` | Double the current phrase |
| Normal | `Enter` | Play from the cursor's step, or stop if playing |
| Normal | `Space x` | Pause in place, or resume |
| Normal | `Space c` | Return to the start. If playing, stop there |
| Normal | `i`, `v`, `V`, `:` | Insert, field Visual, step Visual, Command |
| Visual | Motions, targets, `d y > < + - .` | Extend, then apply to the selection |
| Visual | `=` | Enter Shape mode |
| Visual | `Esc` | Normal mode |
| Shape | `h l` | Onset (curve bias) down or up by 0.05, within 0.05 to 0.95 |
| Shape | `j k` | S-shape (curve gain) down or up by 0.05, within 0.05 to 0.95 |
| Shape | `r` | Swap the end values |
| Shape | `Enter` | Close the controls. The sweep stays live |
| Shape | `Esc` | Cancel, restoring the version from before Shape mode |
| Insert (note) | `a w s e d f t g y h u j` | C to B, current octave, as in Ableton's computer keyboard |
| Insert (note) | `k o l p ;` | C to E, one octave up |
| Insert (note) | `1` | Note-off |
| Insert (note) | `z x` | Octave down or up. Starts at octave 4, where `a` is `C-4`, which Live calls C3 |
| Insert (note) | `c v` | Lower or raise the velocity written with each note, like the octave. It starts at `64`, leaving room for accents, and moves on a grid of `10` to `70`, then `7F`, stopping at `10`. The note heard while typing uses it |
| Insert | `Enter` | In the phrase view, play from the cursor or stop, except that it finishes an instrument ID that has been typed |
| Insert | `Up Down` | Move one step without writing |
| Insert | `Left Right` | Move by the skip setting, which starts at 4, without writing. It counts from the entry just written, so the entry's offset in the beat is kept, or else from the cursor |
| Insert | `Shift-Left Shift-Right` | Move to the previous or next multiple of the skip setting, without writing |
| Insert (note) | `Backspace` | Clear the field and move up |
| Insert (value) | `0-9 a-f` | Two digits commit and move down by the step setting |
| Insert (instrument) | `0-9 a-z` | Two characters commit a number or label and move down. `Enter` commits one digit |
| Insert (FX command) | Letters | Typed in uppercase. An invalid name is refused with a message and leaves the field unchanged |
| Insert | `Esc` | Normal mode |
| Command | `Enter`, `Esc`, `Backspace`, `Up`, `Down` | Run, cancel, edit, recall history |
| Picker | Printable keys, `Backspace` | Edit the search text |
| Picker | `Ctrl-N`, `Ctrl-P`, arrows | Move the highlight |
| Picker | `Enter`, `Esc` | Open the highlighted item, close without changing anything |

Targets are `ib` and `ic`, and `ip` before G1, as in the Technical Plan. In Insert, a note moves down by the step setting, which starts at 1, and uses the last-used instrument, which starts at `00`. Key repeat is ignored on transport keys and allowed on motions and `+`/`-`. The program pushes no keyboard enhancement flags before G1.

**View picker.** A centred floating window has a search line above a list, modelled on the Snacks picker for Neovim. With no search text, views are ordered by most recent use. With search text, `nucleo-matcher` ranks view names, and ties go to the most recent use. Audio keeps playing, and the view underneath does not change until `Enter`. `grr` uses the same window to list uses.

**Binding table.** Default keys are one data table of mode, key sequence, action and description, not a `match` in the input handler. The popup and `Space k` are generated from it. A duplicate key within a mode fails a test. Remapping at runtime is not in this plan.

**Key conventions.** Keys inherited from Vim keep Vim's meaning. New bindings follow the author's Neovim rules: a capital is the wider version of its lowercase key, and a doubled key is the ordinary case. A key may be both an action and a prefix only if losing a timeout does nothing wrong.

**Status line.** It always shows the mode, CPU load (callback time divided by buffer duration), the late count, and swing and BPM. Errors and notices go here.

**Commands.** `:q`, `:e` and `Space qq` refuse with unsaved changes, as in Vim.

| Command | Action |
|---|---|
| `:w [path]`, `:e path`, `:q`, `:q!` | Save, open, quit, quit without saving |
| `:new path` | Create a project folder. An existing folder is refused |
| `:sample path` | Assign a WAV to the current instrument, referring to it where it is |
| `:inst n` | Set the current instrument, by number or label |
| `:label xx`, `:label off` | Mark a numbered slot as labelled and make it current, or stop |
| `:track add [n]`, `:track del` | Add `n` tracks at the end, up to `MAX_TRACKS`, or remove the last track, keeping at least one |
| `:mute`, `:solo` | Toggle on the current track |
| `:bpm n` | Tempo, in thousandths: `:bpm 137.63` |
| `:swing n`, `:tswing n`, `:tswing off` | Project swing, track swing, clear track swing. 50-75 |
| `:tvol xx` | This track's level in hex |
| `:kit [name or path]` | Fill instruments from `00` with the sounds a kit file lists, in order, as one undo step. A labelled sound goes in its label's slot and others fill upward from `00`, and replacing a different sample is reported. With no argument, choose from `samples/kits` |
| `:seed n` | Project random seed |
| `:set step=n` | Insert step |
| `:set skip=n` | How far `Left` and `Right` move in Insert |
| `:len n` | Phrase length. Shortening hides steps, which come back if lengthened in the session and are dropped on save |
| `:double` | Double the current phrase |
| `:gen apply` | Apply the live generator under the cursor |
| `:render path [--rows n] [--tail]` | Render to WAV |

The subcommands are `vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` and `vimtracker probe-keys`. `vimtracker [project]` starts the program.

**Rendering.** A render plays one pass silently to fill delay and reverb, then captures the next pass, so the file is exactly the loop's length. `--tail` appends up to 10 seconds, ending at silence. `--rows n` sets the length in steps. An existing file is overwritten. Output is 32-bit float stereo.

**FX commands.** Values are hex. `xx` values map linearly unless stated.

| Command | Meaning | Persistence |
|---|---|---|
| `VOL xx` | Track gain | Holds |
| `PAN xx` | Pan: `00` left, `80` centre, `FF` right | Holds |
| `CUT xx` | Low-pass cutoff, exponential from 20 Hz to 20 kHz | Holds |
| `RES xx` | Resonance | Holds |
| `DLY xx` | Delay send | Holds |
| `REV xx` | Reverb send | Holds |
| `DEL xx` | Delay this step by `xx` groove units | Once |
| `KIL xx` | Cut the note after `xx` groove units | Once |
| `OFF xx` | Start release after `xx` groove units | Once |
| `RET xy` | Retrigger every `y` groove units. `y=0` does nothing. `x` of 0-7 gets quieter and 8-F louder, in 10% steps per retrigger | Once |
| `CHA xx` | Play this step's note with chance `xx/FF`, drawing one random number. The step's other FX still apply | Once |

A timed value past the step's end keeps running across the next step, and a new note on the track cancels it. An unknown command in a loaded file is a load error.

**Audio.** Voice: linear-interpolated playback, with `C-4` at original pitch, and a 1 ms attack and 50 ms release as instrument defaults. A new note fades the old one over `DECLICK_FRAMES`. Filter: a TPT state-variable low-pass. Delay: stereo at 3/16 note time, with its buffer allocated for 20 BPM, about 2.25 seconds. Reverb: Freeverb-style. Sample import accepts 16-bit, 24-bit and 32-bit float WAV, mono or stereo, and refuses others with a message naming the file. Rate conversion uses `rubato`.

**Audio output.** Use the default output device and its default configuration. If its format is not `F32`, or it has fewer than two channels, exit with a message. Mix to the first two channels. Late means callback time exceeded buffer duration. Nothing prints from the callback.

### Target Files to Create/Modify

Issues create the files they name. Files marked **copy** come from `gmuse`, are adapted where needed, and note their origin at the top.

#### Workspace

*   `Cargo.toml` (Create): workspace with the three crates, each licensed `GPL-3.0-or-later`.
*   `README.md` (Modify): build and run steps, and the license line. It currently says there is no code.
*   `.gitignore`, `LICENSE`, `docs/designs/vimtracker.md`: already exist, and no action is needed.
*   `docs/g1-result.md` (Create): session notes and the G1 outcome.
*   `docs/terminal-notes.md` (Create): `probe-keys` results.

#### `crates/vt-model` (Create)

*   `Cargo.toml`, `src/lib.rs`: manifest and module list.
*   `src/constants.rs`: the design constants and tuning values.
*   `src/song.rs`: `Song`, `Track`, `Section`, `Chain`, `Phrase` and `Instrument`.
*   `src/step.rs`: `Note`, `Step`, `FxCommand` and `FxSlot`, with value ranges and persistence.
*   `src/steptext.rs`: step text parser and printer, for files and tests.
*   `src/selection.rs`: motions and targets resolved to field ranges.
*   `src/action.rs`: `Action` data and the `edit` function, the only path that changes a song.
*   `src/history.rs`: undo and redo version list.
*   `src/curve.rs`: bias, gain and sweep values.
*   `src/generator.rs`: live generators and rerendering.
*   `src/project.rs`: folder loading and saving, version check, sample copying and autosave.
*   `src/sample.rs`: WAV decoding, stereo conversion and resampling.
*   `src/samples.rs`: the session sample pool.
*   `tests/*.rs`: behaviour tests.

#### `crates/vt-engine` (Create)

*   `Cargo.toml`, `src/lib.rs`: manifest and public engine, handle and render API. Internal shapes are the implementer's choice.
*   `src/link.rs`: publish, return, command, telemetry and scope queues.
*   `src/engine.rs`: the block loop, processing events on exact samples, and the sample position formula.
*   `src/playhead.rs`: per-track position, swing and version-change rules.
*   `src/fx.rs`: the FX interpreter.
*   `src/track_state.rs`: fixed-size per-track state.
*   `src/voice.rs`: sample playback, envelope and the preview voice.
*   `src/dsp/filter.rs`, `src/dsp/smoother.rs`, `src/dsp/delay.rs`, `src/dsp/reverb.rs`: effects.
*   `src/mixer.rs`: track mix, sends, master gain and soft clip.
*   `src/rng.rs`: PCG32.
*   `src/rt_guard.rs` (**copy** of `crates/gmuse-engine/src/rt_guard.rs`): per-thread allocation counting in tests.
*   `src/render.rs`: offline WAV rendering.
*   `tests/*.rs`, `tests/golden/*.txt`: behaviour tests and one hex hash per golden file, with an FNV-1a 64 helper in test support.

#### `crates/vt-tui` (Create)

*   `Cargo.toml`, `src/lib.rs`, `src/main.rs`: manifest, library, and a thin `main.rs` handling subcommands.
*   `src/app.rs`: main loop, state, terminal setup and restore.
*   `src/audio.rs` (**copy**, adapted from `crates/gmuse-engine/src/audio/device.rs`, without device recovery): `cpal` stream.
*   `src/input/mode.rs`: mode state machine.
*   `src/input/keys.rs` (**copy**, adapted from `src/cmd/keys.rs`): key parsing.
*   `src/input/bindings.rs`: the binding table, and `which_key_due` (**copy** from `src/ui/mod.rs`).
*   `src/input/command.rs`: `:` command parser.
*   `src/input/picker.rs`: picker state and matching, with `rank` (**copy**, adapted from `src/ui/picker.rs`).
*   `src/shape.rs`: Shape mode session.
*   `src/views/phrase.rs`, `chain.rs`, `instrument.rs`, `status.rs`, `picker.rs`, `help.rs`, `scope.rs`: one view per file.
*   `src/probe.rs`: the `probe-keys` subcommand.
*   `tests/*.rs`: key-sequence tests.

Transport behaviour follows `audio::transported` and `audio::cued` in `gmuse` where they apply.

### Milestone M1: Hear a loop

*   **Issue 1: Workspace.** Create the workspace and a stub for each crate. **Done when:** `cargo test` passes and the dependency set builds together.
*   **Issue 2: Key probe.** Add `probe.rs`. Print each key event with no keyboard flags pushed. `Ctrl-C` exits and restores the terminal. **Done when:** the author records in `docs/terminal-notes.md`, with and without tmux, whether `Ctrl-A`, `Ctrl-X`, `Ctrl-O`, `Ctrl-R`, `Ctrl-N`, `Ctrl-P`, `Ctrl-D`, `Ctrl-U`, `Ctrl-E`, `Ctrl-Y` and `Tab` arrive, plus `Esc` latency. Any key that does not arrive gets a replacement recorded there and in the key table.
*   **Issue 3: Model and step text.** Add `constants.rs`, `step.rs`, `song.rs` and `steptext.rs`. Each `Song` map uses `BTreeMap<u32, Arc<T>>`. **Tests:** every field form round-trips; invalid notes, widths and unknown commands give errors.
*   **Issue 4: Project files.** Add `project.rs`, `sample.rs` and `samples.rs`. **Tests:** a song round-trips; a file with a changed field and a different version gives an error naming both versions; 16-bit mono and 24-bit stereo WAVs load at a requested rate; a missing sample is an error naming the path and the rest loads; a save failure leaves the previous file intact. WAV fixtures are generated inside tests.
*   **Issue 5: Engine and render.** Add `engine.rs`, `playhead.rs`, `track_state.rs`, `voice.rs`, `mixer.rs` and `render.rs`, with no FX commands yet. **Tests:** a note starts on its exact sample at 44100 Hz and 48000 Hz, including at a fractional tempo; a new note fades the old one; two renders match by hash; a section with 8-step and 16-step chains restarts together; replacing a sample does not move a playing voice; reads stop at a sample's end.
*   **Issue 6: Publishing.** Add `link.rs` and `rt_guard.rs`. **Tests:** no allocations or frees in `process` across 1,000 rapid publishes, track count changes and phrase removal; the playhead rules in the Technical Plan hold; the core engine tests run at 1, 8 and 64 tracks; two `Toggle`s return to the original state.
*   **Issue 7: Actions and history.** Add `action.rs` and `history.rs`, with only the note and instrument actions Issue 8 needs. **Tests:** `u` and `Ctrl-R` restore exact versions; unchanged phrases stay the same `Arc`; the oldest version drops past `HISTORY_LIMIT`.
*   **Issue 8: Minimal program.** Add `app.rs`, `audio.rs`, `input/mode.rs`, `input/keys.rs`, `input/bindings.rs`, `input/command.rs`, `views/phrase.rs` and `views/status.rs`. Support the leader, `h j k l`, Insert note entry with the preview voice, `Enter`, `Space x`, `Space c`, `Space qq`, `:new`, `:sample`, `:inst`, `:bpm`, `:len`, `:w`, `:e`, `:q` and `:q!`. **Tests:** a duplicate binding fails; a preview during playback leaves rendered track notes unchanged. **Done when:** the author can write a loop and hear it, and an edit during playback is heard on the next pass.

### Milestone M2: Editing grammar

*   **Issue 9: Motions and operators.** Add `selection.rs`, the Normal mode grammar, viewport keys, counts, Visual modes, the which-key popup, `views/help.rs` and `Space ?`. **Tests:** table-driven key sequences, starting and ending as step text, covering every rule under Operators and targets, Counts and Values, including `d4j`, `3ib`, `dj` on one column, and `>` skipping `OFF`. **Done when:** CUJ 2 works up to `.`.
*   **Issue 10: Repeat, registers and values.** Add `.` in Normal and Visual, `"a` to `"z`, `p`, `+`, `-`, `Ctrl-A`, `Ctrl-X`, Insert value entry, `:set step`, `Space pd` and `:double`. **Tests:** `.` after each operator; values clamp to their ranges; registers keep separate contents; `p` clips at the phrase end and refuses a type mismatch; doubling repeats visible steps and is one undo step. **Done when:** CUJ 2 works.
*   **Issue 11: Chain and tree navigation.** Add `views/chain.rs`, chain transpose, `gd` including item creation, `Ctrl-O`, `Tab`, `grr`, `gc`, `gC`, the shared marker and `]p [p ]P [P`. **Tests:** cloning leaves the original unchanged and repoints the reference; a deep clone copies the chain's phrases; creation uses the lowest unused ID. **Done when:** CUJ 5 works.
*   **Issue 12: Tracks.** Add read-only neighbour note columns, sideways scrolling, `W B`, `:track add`, `:track del`, and mute and solo as commands. **Tests:** added tracks save, load and play; `:track del` removes only the last track and keeps at least one; mute is not in undo history but is saved. **Done when:** CUJ 1 and CUJ 6 work.
*   **Issue 13: Instrument view.** Add `views/instrument.rs` with sample assignment, the current instrument, and volume, attack and release. **Done when:** samples can be assigned and adjusted without `:sample` alone.
*   **Issue 14: View picker.** Add `input/picker.rs`, `views/picker.rs`, `Space Space`, `Space ff` and `Space 1` to `Space 6`, and use the window for `grr`. **Tests:** with an injected list, `sg` matches `song` but not `groove`; ties go to most recent use; an empty search lists by most recent use; `Esc` leaves the view unchanged.

### Milestone M3: Sweeps, then the G1 test

*   **Issue 15: FX and audio effects.** Add `fx.rs`, `rng.rs`, `dsp/*`, the commands in the FX table, FX entry in Insert mode, `:seed`, held values and equal-power pan. **Tests:** each command on a rendered result or engine state; `RET` with `y=0` does nothing; held values reset as specified; a fixed seed gives the same `CHA` results; adding a track does not change another track's results. **Done when:** the author has listened to a full `CUT` sweep over a sustained sample at 120 and 170 BPM, recorded the result in Open Questions, and added the step-length fallback if it sounds stepped.
*   **Issue 16: Swing.** Add `:swing` and `:tswing`. **Tests:** 66.7% gives exactly 80 ticks; 50% gives none; a track override does not move another track.
*   **Issue 17: Curves.** Add `curve.rs`. **Tests:** end values are exact in both directions; curves stay monotonic; over 9 steps from `00` to `7F`, curve bias 0.8 gives `00 2E 49 5A 66 6E 75 7B 7F` and curve gain 0.2 gives `00 05 0D 1B 40 64 72 7A 7F`; refused selections give a message.
*   **Issue 18: Live generators.** Add `generator.rs`, the `Phrase` field for them, their marker and `:gen apply`. `edit` applies any live generator whose owned columns it changes, except for a rerender. **Tests:** the rules under Live state; a rerender changes owned columns only and keeps it live; a live generator round-trips through saving; two generators on different columns are independent.
*   **Issue 19: Shape mode.** Add `shape.rs`, Visual `=`, `gl`, and `Space ga`. Each change rerenders and publishes without adding history. `Enter` adds one history entry and `Esc` restores the starting version. `.` applies the last shape to the current selection. **Done when:** CUJ 3 and CUJ 4 work with audio.
*   **Issue 20: Scope and meters.** Add the scope tap to `link.rs` and `views/scope.rs`, with a small master scope and per-track level meters using Braille and block characters. **Tests:** the real-time guard stays at zero with the tap active; a full tap drops frames rather than waiting.
*   **Issue 21: Render command.** Add `vimtracker render` and `:render`. **Tests:** golden hash tests; a render is exactly the loop's length; `--tail` extends it; `VT_BLESS=1` updates hashes. **Done when:** CUJ 7 works.
*   **Issue 22: Autosave.** Add autosave to `project.rs`. **Tests:** only a changed project is written; a newer autosave is offered on opening; audio is unaffected.
*   **Issue 23: G7 and G1.** First run G7, and record how many tracks the laptop sustains. Once that passes, run G1 as written in Goals. Record each session and the outcome in `docs/g1-result.md`, including which missing feature was wanted most and whether any bad-sounding sweep was a tuning fault.

### Known Gaps From Review

The EGM review concluded on 2026-11-05 with its re-audit limit used, and these findings were not resolved. They are accepted for now, and must be decided in the issue named before that issue is done. They override anything above that they contradict.

*   **`rtrb` cannot replace a waiting version (Issue 6).** Its `Producer` cannot remove a queued item. The publish rule needs another mechanism, such as a newest-version slot on the edit thread retried each frame.
*   **Key repeat cannot be filtered with no keyboard flags (Issue 8).** Without flags, a held key arrives as repeated presses. A held `Enter` flutters, and repeats after `Space x` can clear fields.
*   **Cursor context (Issue 8).** A phrase can be in several chain slots or none, so where `Enter` starts, what neighbour columns show, where `W B` lands, how playheads are drawn, and the opening view need one model.
*   **Transport states (Issues 6 and 8).** `Space x` from stopped, `Enter` while paused, starting from the top, voices and tails on stop, whether a loop wrap resets `CHA` and held values, and transport keys outside Normal mode are undefined. `gmuse`'s `audio::transported` and `audio::cued` do not describe this design, and "two `Toggle`s" in Issue 6 should be rewritten once these are set.
*   **Chain view editing (Issue 11).** Entering existing phrase IDs, transpose range, adding and removing rows, operators on rows, empty slots, `gc` on a row, and what `:track add` creates are undefined.
*   **Copied `gmuse` code (Issues 6, 8 and 23).** `rt_guard.rs` also needs the `#[global_allocator]` lines from `gmuse-engine/src/lib.rs`, and guard tests must be unit tests. `device.rs` forces a fixed buffer size and drops `Xrun`, which G7 needs counted. `vt-tui` needs `thiserror` if `keys.rs` is copied. Copied key parsing uses spellings such as `C-a`.
*   **Mute and undo (Issue 12).** Undo publishes an older `Song` with old mutes, so live mute state needs to live outside undo. Mute and solo also have no direct key.
*   **FX details (Issue 15).** Slot partial states, including typing `CUT` closing the filter during playback; `RET` amounts; and `KIL`, `OFF`, `RET` and `CHA` with `DEL` or no note. `CHA` and `RET y=0` differ from the M8. The listening test must use hand-typed values, or move after Issue 19.
*   **Samples and rendering (Issues 4, 13 and 21).** `rubato` settings, a sample browser (only typed paths are specified), instrument ranges, and rendering at a rate other than the device's, with its threading, source version and tail threshold.
*   **Project lifecycle (Issues 8 and 22).** An unsaved project has no `samples/` folder or autosave location. Bare `:w`, `:w path` onto an existing folder, `:e` during playback, and the autosave prompt are undefined.
*   **Fallback rule.** It is broader than a reviewer considered safe. Use it for error wording, out-of-range refusal and clamping. Anything affecting feel gets recorded in the issue for the author to check.
*   **Other wording findings.** "Keys inherited from Vim keep Vim's meaning" overstates, because `+`, `-`, `=`, `{ }`, `p`, `Ctrl-E`, `Ctrl-D` and `dj` differ. `Space 1`, `Space 4` and `Space 6` should be hidden until their views exist. The reports in `docs/reviews/` hold the remaining minor findings.

### Verification Plan

*   **Compilation/Build:** `cargo build --workspace`
*   **Unit and integration tests:** `cargo test --workspace`, and `cargo test --workspace --release` for the real-time guard
*   **Static analysis and linting:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`
*   **Test coverage requirement:** no percentage target. Each issue lists its tests.
*   **E2E/Integration:** golden render tests and each issue's "Done when" check.
*   **Performance (G7):** run in Issue 23, as specified in G7.

--------------------------------------------------------------------------------

*Drafted with EGM*
