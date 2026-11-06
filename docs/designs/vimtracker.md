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

Per-note tables are a further idea, and they get their own decision after the milestones in this plan.

## Glossary

*   **Track:** one voice's lane through the song. Each track plays one note at a time.
*   **Section:** one row of the song, holding one chain per track.
*   **Machine:** what makes a track's sound, under Direction. The sampler is the first.
*   **Chain:** a list of phrases, each with a transpose.
*   **Phrase:** one track's sequence of steps, reusable by ID.
*   **Step:** one position in a phrase. One step is a sixteenth note at normal speed.
*   **Field:** one editable part of a step: note, velocity, instrument, or a slot's command or
    value.
*   **Slot:** one FX command and its value. Column group means a slot, or the note, velocity
    or instrument column.
*   **Insert:** one effect on a track, in the signal path. Not to be confused with a slot, or
    with Insert mode.
*   **View:** a screen you can jump to.
*   **Operator:** a key that combines with a motion or target. **Action:** the recorded edit
    it produces.
*   **Live / applied:** a generator's state. A live generator can still be changed. Applying
    it leaves plain values.
*   **Curve bias** (the "onset" control) and **curve gain** (the "S-shape" control). "Gain"
    alone means signal level.

## Goals

*   **G1: Does it become an instrument the author wants to pick up?** **Retired on 2026-11-06,
    and won't be run:** the author's jams answered it, and they want to continue. The original
    test is kept here for the record. After M3, the author
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
    from sections, and each track's chains and phrases referred to by number. A phrase can be
    used in several places on its track, it shows when it is shared, and it can be cloned. Polyphony uses several tracks.
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
*   Grid column types, such as chord or stage-sequencer columns. Chords and stage sequences are
    generators that write to ordinary tracks. Machines and bus tracks, under Direction, differ in
    what a track sounds like or takes in. Every track keeps the same steps and the same grammar.
*   Matching the sound of any other tracker exactly.
*   Using the names, logos or visual design of Polyend, Dirtywave or Elektron products.
*   Sample recording, streaming long samples, MIDI, plugin hosting and live performance mode for now.
    Synth engines and granular effects are planned under Direction, and were non-goals for the first prototype.
*   Becoming a DAW, or an MPC-style workstation. The scale in mind is full songs at about Maschine's size.
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
    keyboard, and presses `Tab`.
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
*   **When**: The user presses `S`, adjusts onset and S-shape, then presses `Enter`.
*   **Then**: Every adjustment is heard during playback and shown in the column. `Enter` closes the
    controls and leaves plain values, one `u` restores the original values, and `Esc` cancels
    with no change. `.` applies the same shape to another selection, with its own ends. Leaving the sweep
    live is deferred: see Generators.

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
*   **Sections before G1.** A project has one section. Each chain loops at its own length for as long
    as the song plays, so unequal lengths drift, like Elektron's master length at `INF`. `:reset n`
    sends every track back to its start every `n` steps, and is off by default. The tracks' place is
    counted apart from the step clock, so a reset doesn't move the beat.

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
*   If an ID no longer exists, which undo can cause, the track is silent until it's restored.
*   If the track no longer exists, its voice stops with a short fade. Before G1, only the last track
    can be removed, so positions stay stable.

**Samples.** The pool only grows during a session, and sample IDs stay stable. A voice stores its
sample ID when it starts and keeps it until it ends, so replacing a sample affects new notes only.
Sample reads are bounds-checked, and a voice past its sample's end stops. Replaced samples stay in
memory until the project is reloaded.

### Transport

Before G1 there is one scope: the current section, on all tracks, looping. Transport actions are
commands that the engine resolves against its own state, so two quick presses cannot act on an
out-of-date view. `Tab` plays and pauses in place, like a DAW's transport or an Elektron's `PLAY`. `Shift-Tab` plays
from the start and `Space c` from the cursor's step, restarting if already playing. There is no stop key: pausing does that job,
and opening a project is what returns to stopped. `Enter` only ever confirms what is being typed or picked.

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
from the start or the last reset are delayed by `round(480 x swing% / 100 - 240)` ticks, and the step's FX move
with its note. Because pairs don't count from a track's own loop, an odd-length phrase swings different steps
on each pass and stays on the same grid as everything else.

Whole-tick arithmetic holds for straight rows at normal speed. Track speeds and M8-style grooves are
after G1.

### FX interpreter

One interpreter runs FX commands, once per step before G1. It must not assume it is only running for a
phrase, because tables will use it at their own rate. Slots run left to right. Held values start at `VOL
E0`, and at the track page's `Cut`, `Res`, `Pan`, `Delay` and `Reverb`, which default to `FF`, `00`, `80`, `00`
and `00`. They reset when playback starts from the top, by `Shift-Tab` or `Space c` on row `00`. `Tab` resuming keeps them.
Changing one of those on the track page sets its held value straight away, until a step sets it again.

**LFOs.** Added ahead of the M3 sweeps, by a change of plan: the author reached for this while playing, and
sweeps and LFOs do different jobs. FX and sweeps belong to steps, and an LFO runs apart from them. Each track has
two, on its page rather than on an instrument, since its filter, pan and sends are the track's, and it shouldn't care which
sample plays. Elektron's per-track idea is kept, and its `SPD` and `MULT` are not: settings use standard names and units.

*   **Destination:** `Cut`, `Res`, `Pan`, `Vol`, `Dly` or `Rev`, which are the values a track holds. `K` lists them. An LFO adds to
    the held value, so a step's FX moves the centre and the LFO carries on around it. The result is clamped to the
    value's range. Two LFOs on one destination add.
*   **Rate:** the length of one cycle. A note division in exact ticks (`1/4`, `1/8d`, `1/16t`), whole bars, a count of
    steps (`7st`) for lengths that slide against the bar, or hertz (`2.5hz`), which ignores the tempo. `+` is always faster.
*   **Depth:** how far it swings either way, in hex in the destination's units, so `Cut 80` and `Depth +40` moves between
    `40` and `C0`. Negative flips the shape. **Phase:** where in the cycle it starts, in degrees.
*   **Shape:** `Sine`, `Tri`, `Saw` (falls), `Ramp` (rises), `Square`, `Random` (a new value each cycle, from the seed) or `Smooth` (the same values, eased from one to the next).
*   **Retrig:** when on, each note on the track restarts it. Otherwise playing from the top is the only restart. A
    `:reset` doesn't touch LFOs, as it's about patterns, and anything that should follow steps uses Retrig.
*   **Timing:** position is worked out from the song's ticks, or from frames played for hertz, rather than added up, so
    it can't drift and a render repeats exactly. It's worked out every 32 frames and smoothed like held values. A
    stopped track sits at its held values, so auditions hear those, and pausing freezes it.
*   **The filter stays running** while an LFO moves `Cut` or `Res`, even at the top of its range, or it would come back
    with old state and jump.

**Envelopes.** Each track has two ADSR envelopes on its page, as Elektron's tracks do, rather than on instruments as on the M8 and Polyend. `Env 1` defaults to
`Vol`, where it shapes each note in the voice, and replaced the instrument's attack and release. `Env 2` defaults to `Cut` with no depth. Both can go to any LFO destination or to `Pitch`,
whose depth is in semitones. A second envelope on `Vol` moves the level by its depth on the level scale. The shape of `Env 1`, if it's not on `Vol`, is a plain 1 ms attack and 50 ms release.

*   **Curves:** taken from Mutable Instruments' Peaks rather than Polyend's straight segments, which on level hang on and then drop. An attack follows `t^3.32`, and decay and release
    `(1 - e^-4t) / (1 - e^-4)`, so times are exact and stages land exactly on their levels.
*   **Reset:** on by default, as on Elektron, starting each note's envelope from zero. Off goes on from the current level, as Peaks does without its hard reset. Release starts from the current level either way.
*   **Notes:** a note holds until the next note or an `OFF`. The next note fades the last with the usual short fade rather than running its release, so `Sustain 0%` suits a drum. `OFF` and `KIL` start the release of mod envelopes.
*   **Speed:** position is worked out from frames played, like LFOs. What envelopes add to `Cut`, `Res` and `Pan` passes through a `FAST_SMOOTHING_MS` smoother rather than `SMOOTHING_MS`, and `Pitch` slews per frame, so a fast snap survives.
*   **Times:** milliseconds up to `MAX_ENVELOPE_MS`, 8 seconds, as on Peaks. Not tempo-synced for now, and neither are Elektron's as far as is known.
*   **Diagram:** the track page draws each envelope. A stage's width depends only on its own time, on a log scale so a 1 ms attack shows beside a long release, and changing a stage only moves what comes after it.

**Filter.** A track has one, with `Filter`, `Cut`, `Res` and `Drive` on its page. `LP`, `HP`, `BP` and `Notch` are outputs of one TPT state-variable filter, the same design as Mutable Instruments' `stmlib::Svf`, so they cost about the same. `Ladder` is the
character filter: a saturating four-pole ladder at 2x, from Huovilainen's model, Zavalishin's integrators and Mystran's linearisation, after VCV Fundamental's VCF, which is GPL 3 or later. It self-oscillates, and its cost only falls on tracks using it.
``Drive` turns up the input, soft-clips the feedback and the output at 2x, and compensates for level. At `00` the state-variable path is untouched and not oversampled. Changing `Filter` starts the new filter from silence. A low-pass with nothing to do is skipped.

**Utility filter.** Each track also has `Low cut` and `High cut` after its main filter, as Elektron's Digitakt II and Digitone II pair a swappable filter with a base-width one. They use the same
state-variable filter at 12 dB and Q 0.707 on the same 20 Hz to 20 kHz scale, with no resonance, modulation or FX for now. `00` and `FF` are off, cost nothing, and leave renders unchanged. The position is fixed after the main filter, so it can tame a driven ladder, rather than movable as on the Digitone II.

**Randomness.** Each track has its own PCG32 generator, seeded from the project seed and the track's index.
Generators restart when playback starts from the top. Because tracks are only added or removed at the end
of the list, this does not change other tracks.

### Audio path

Each track has one voice: one-shot sample playback, a filter, a volume envelope and equal-power pan, with
`80` defined as exact centre. Final gain is step velocity x `VOL` x track level x instrument volume, and `--` in the step
means full. Velocity runs `00` to `7F`, matching MIDI, and is still linear. Level values run `00` to `F8` on an even scale in dB, as a fader
does, since a linear one covers only 6 dB in its top half: 4 steps is 1 dB, `E0` is 0 dB and the default, `F8` is +6 dB of headroom, `01` is -55.75 dB
and `00` is silent, so every `10` in hex is 4 dB. Anything entered above `F8` snaps to it, and the engine treats any higher value in a file the same way. Track level, instrument volume, `VOL`, `DLY` and `REV` use it, as do LFOs moving those. The scale follows
Polyend Tracker's gain tables and Ableton Live's fader, which run about -36 to +6 dB and -70 to +6 dB. Track level comes before the sends, so it scales them too, and it belongs on mixer pages
once those exist. Transposed notes clamp to the note range.

Tracks feed a mixer with shared delay and reverb sends, then a
safety limiter: nothing changes up to full level, and anything above it clips hard, since the fix for hearing it is
the mix. A non-finite sample becomes silence. `:set limiter=off` lets a float render keep levels over full, and the
live output clips at full either way. Processing is 32-bit float at the device's sample rate. Samples are converted to that
rate when loaded and kept fully in memory. All other tuning values are chosen at implementation, tuned by
ear, and pinned by golden hashes.

**Smoothing hypothesis.** Changes to filter, volume and pan pass through a smoother of about 10 ms. At 120
BPM a step lasts 125 ms, so a full filter sweep may sound stepped on a held sound. Issue 15 tests this by
ear. If it does, a column under a live sweep ramps across each step toward its next value, using the
sweep's own values with no look-ahead into the audio, and hand-entered values keep the 10 ms smoother.

**Scope and meters.** The main output meter is built: each report carries the master's peak before and after the
limiter and its summed power, and reports merge so a hit between two frames isn't missed. The meter's own motion is
in `meter.rs`, apart from its drawing, so a mixer can have one per track. A scope is still to come: the callback copies a
master signal into a preallocated ring buffer that drops frames when full, so it never waits on the UI. Track levels go
through telemetry.

### Editing

Keys go through a mode state machine (Normal, Insert, Visual, Shape, Command, Picker) that produces actions.
Actions are plain data, so `.` can replay them and tests can run editing without a terminal.

*   **Operators and targets.** Operators are `d y > <`. Motions and `ic` act on the current column. `dd yy >>
    <<`, `ib` and `ip` act on whole steps. This differs from Vim on purpose: `dj` clears a column rather than
    whole steps, because editing one column is the everyday tracker action.
*   **Counts.** A count extends the range: `d4j` covers five steps, and `3ib` three beats. With `=`, `-`, `+` and `_`, a
    count is the amount, on the cursor or selection.
*   **Values.** `>` and `<` move notes by a semitone and other values by 1. They skip empty fields, `OFF`, FX
    names and instrument IDs.
*   **Registers and paste.** `d` and `y` also fill the unnamed register, and only `y` fills `0`. `p` overwrites from
    the cursor and stores more past the last step, leaving the loop alone. Empty copied fields overwrite too, while `gm` leaves what is there alone.
    A column type mismatch is refused with a message.
*   **Edges and repeat.** Motions stop at the edge. `.` repeats operators, value changes and shapes, in Normal or
    Visual. It does not repeat Insert sessions.
*   **Doubling.** `Space pd` doubles a phrase's loop by repeating it, as one undo step. Steps after the loop move
    down rather than being overwritten.
*   **The loop.** A phrase stores more steps than it plays, like a clip: a loop with a start and a length, saved with
    everything outside it. Chains only see the loop, so moving it needs nothing from them. Rows on screen count from the
    loop's start, so `00` is always beat 1, beats stay at multiples of 4, and rows above it are negative. There is no
    separate play marker as in Ableton: the loop's start is the 1. Internally the cursor and edits use stored positions,
    and only the view, the beat motions and `chain_position` convert. `]l` `[l` and `]L` `[L` slide it, keeping its
    length, `gs` starts it on the cursor, and `ge` ends it there, so the row can be chosen rather than read off and converted. `]r` `[r` rotate whole steps within the loop, wrapping round and leaving steps outside it alone. Slides work during playback and stay in time, since position comes from
    the section's step count. Length changes are refused while playing, as a new length moves where playback lands.
*   **Moving through the tree.** `gd` opens the item under the cursor, and on an empty reference it creates the
    next unused ID, assigns it and opens it. `Ctrl-O` jumps back, `Ctrl-N` jumps forward, and `grr` lists uses in the
    picker window.
*   **Cloning.** `gc` copies a phrase to a new ID. If you arrived through `gd`, it repoints that reference and stays
    in the clone. Otherwise it opens the clone, changes no references, and a status message says so. If the parent
    chain is shared, a status message says the change affects all its uses. `gC` on a chain also clones its phrases
    and repoints the section's reference.
*   **Undo.** Undo history is a list of song versions, and undo publishes an older one. It drops the oldest version
    past `HISTORY_LIMIT`. One Insert session and one Shape session are each one undo step.
*   **Views.** Before G1: phrase, chain, track and instrument, and a help view. The phrase view shows the focused track's
    full steps beside read-only note columns for the other tracks. Each column shows that track's own current phrase from its own
    loop start, with rows blank past its end, and its own `>`. Rows line up by number, not by time, since unequal
    lengths drift. Rows continue past a short focused phrase for longer tracks. `;` and play still work in song time.
*   **Fallback rule.** If a behaviour is not specified and does not affect what G1 tests, follow Vim, or else choose
    the simplest option and record it in the issue.

### Generators

A generator is a pure function of its parameters and target, and later a seed. The live sweep is the first one.
Later generators combine a **mask**, which chooses the steps to change (as with Polyend's Fill tool), with a **value
source** that chooses what to write. Those are built when a second generator needs them.

**Shape panel.** Built ahead of live sweeps, by the author's decision: Visual `S` opens a panel whose result is a preview
with no undo step, `Enter` keeps it as one undo and leaves plain values, and `Esc` restores what was there. Issue 18's live
state is deferred until plain values are found wanting. Selecting and pressing `S` again reshapes, from the values now there.

**Sweep.** A sweep covers one column, Velocity or FX, across at least two rows. A multi-column, note-column or instrument-column
selection is refused with a message. `From` and `To` default to the first and last values selected, or if those match the lowest and
highest, or if those match the column's range, so a fade doesn't need swapping and a selection needn't end on a value. An FX column uses the
first command selected, and rows holding another command keep it. A `Fill` of `Random` picks from the range instead. `Where` chooses the
rows, as in Polyend's Fill tool: notes, no notes, those with the command, all, every n, or a percent chance. A velocity only ever goes on a note.

**Curve.** `f(t)` applies bias then gain, where `t` runs by row position from 0 at the first row filled to 1 at the last, and the
value is `start + (end - start) x f(t)`. `bias(t,a) = t / ((1/a - 2)(1 - t) + 1)`, and gain is Schlick's gain built from it. The panel shows
them as `Curve` and `S`, from -100 to 100 with 0 a straight line, where `a = 0.5 + Curve/200` and `g = 0.5 - S/200`. The distance travelled rounds half up, so
falling mirrors rising exactly, and results clamp to the column's range.

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

## Direction: a track is one sound

This section is the plan from the author's design review after playing with the prototype, and it is **decided, not yet built**. Where it differs from
the rest of this document, this section wins. The rest describes what's built or was planned earlier, and has drifted from the code in places. The author reports the instrument already exceeds their expectations in
jams, and has drifted toward an Elektron feel on purpose, as those are the machines they know best. The aim is to make full songs at
about the scale of a Maschine, with the UI as the deciding concern, and not to replace Ableton.

### Machines replace instruments

A track owns everything about its sound, and steps no longer pick an instrument. Sound locks are rarely wanted when tracks are plentiful, and one sound per track makes stems simple. The signal path through a track becomes:

```text
Machine -> Filter -> Utility filter -> FX inserts -> Level, Pan -> Sends or a bus
           (2 envelopes and 2 LFOs can reach any of these)
```

*   A **machine** makes the sound: first the one-shot sampler, then a slicer, then a kick synth, then voices after Plaits, whose code is MIT licensed.
*   A step becomes `NOTE VEL FX FX FX`. An `SMP` FX could lock a sample later, if it's missed.
*   Tracks get names, like `BD`, in place of instrument labels. A **kit** becomes a set of track sounds, and a **sound** can be saved and loaded alone.
*   **Stems:** `:render --stems` writes one file per track.
*   **Cost:** drum parts spread over several tracks, so editing across tracks and a mixer page come right after this, as M6 and M7.
*   **Polyphony:** a track stays one voice. The machine interface should allow more, for pads and chords, but none is built until a synth machine needs it. The audio thread still can't allocate, so machines are an enum with state made ahead of time.

### A shared parameter list

Each machine and effect declares its settings: name, range, units and default. The track page, stepping, `x`, `K`, typing, modulation `Dest` lists and FX on steps all read that list, so a new machine or effect needs no new UI. It's built with the first effect, in M9, when something new first needs it. Until then, the existing controls stay as they are.

### Everything is a track

Elektron's newer machines suggest one model for effects. The Syntakt's FX track has its own sequencer, whose locks drive the effects, and audio routed to it doesn't bring locks along. The Tonverk has audio tracks, bus tracks, sends and a mix track, with two insert slots on audio and bus tracks, and LFOs on the buses, sends and mix. The Analog Heat +FX has a Flow page that rearranges up to eight blocks and sets their mix and bypass.

So every place that processes sound is a track of some kind, with the same page, phrases and grammar:

| Track kind | Takes in | Has |
|---|---|---|
| **Sound** | Its machine | Filter, FX inserts, sends |
| **Bus** | Tracks routed to it | FX inserts, sends |
| **Send** | What tracks send it | FX inserts. Delay and reverb become these |
| **Mix** | Everything | FX inserts, such as a master compressor |

A bus, send or mix track has steps like any other, so writing `DEC40` on a reverb's track sequences the reverb. Only a track's own steps control it, as on the Syntakt. Locks on other tracks can't reach it, which keeps a track's phrase self-contained.

### FX inserts

A track has 3 inserts, each holding one effect chosen with `K`, and their settings have their own page. Inserts are rows on that page, so `dd` and `p` move an effect to change the order, in place of the Heat's Flow page. A fixed number keeps it readable in a terminal, with no patching. A track can hold two of the same effect, though that won't be common: a step's command reaches the first one, and a second is set from its page and through modulation. Core effects, in order: compressor with sidechain from another track, saturation and bitcrush, 3-band EQ, chorus, then character delay and granular. Delay and reverb move to send tracks, and need real settings: decay, damping and pre-delay on a reverb after Mutable's Griesinger design, replacing the fixed Freeverb, and time, feedback, a filter and ping-pong on the delay.

### Pages

The track page is already full at about 24 rows. It splits into **Sound**, **Mod** and **FX** pages, with `H` and `L` moving between them under the capital-means-wider rule, and `Space 5` becomes a sample browser. Sound and Mod arrive in M5, and FX with the first effect in M9.

### FX columns are the automation lanes

The question was whether tracker FX should give way to Elektron locks or DAW automation lanes. They aren't one thing, and only half of them are parameters.

*   **Events** are `DEL`, `RET`, `KIL`, `OFF` and `CHA`. They do something once, at a moment, to a note. Automation can't express these, and they stay as they are.
*   **Values** are `CUT`, `PAN` and so on. A column of these already is an automation lane, written as text and edited by the grammar: `d`, `y`, `p`, `.`, rotate and the shape panel all work on it, it's seen beside its notes, and it travels with a phrase when it's shared, copied or drifting against other tracks. Lanes kept apart from the grid would lose all of this, and need a second way to edit.

So keep the columns, and remove what's weak about them:

| Weakness | Fix |
|---|---|
| Only a few commands can be named | A slot can name any parameter in the track's list, with `K` to choose |
| Values always hold | Each value has a **lock**, **hold** or **glide** mark, below, with lock the default as an Elektron p-lock is |
| Values step, with nothing between rows | A glide moves across the steps between values, which is the line between points in DAW automation. This also answers the sweep smoothing question |
| 3 slots | The number of slots becomes a per-track setting, and folding hides what isn't needed |
| Nothing longer than a phrase | Long moves use a long phrase, or a bus or send track's own steps |

The shape panel is already what makes ramps here. It needs only to learn glide.

**Marks.** Each value in a slot has a one-character mark beside it, the author's idea. Marks are meant to be tried in play, and are stored as one small value per slot so the rules can change cheaply.

```text
    Note V  FX1      FX2
00  C-4 64  CUT40 .  ---00
01  --- --  ---00    ---00
02  C-4 64  CUT80 g  DLY30 h
03  --- --  ---00    ---00
04  C-4 64  CUTC0 .  ---00
```

*   **`.` lock**, the default: lasts until the next note on the track or the next value of that parameter, so it covers a whole note rather than snapping back while it rings. A lock on a row with no note works the same way, which changes a sound mid-note.
*   **`h` hold**: replaces the base until playback next starts from the top, as all values do today.
*   **`g` glide**: slides from this row to the next value of that parameter in the slot, wrapping through the phrase's loop. With no other value, it stays put.
*   **Layers:** the track page's value is the base, a hold replaces it, and a lock or glide sits on top of those. M4's design has the full rules. LFOs and envelopes add on top of everything, and carry on around whatever the steps do.
*   **Editing:** `~` cycles the mark on the slot under the cursor, in Visual as well, and `.` repeats it. Marks aren't cursor stops, as those would add a stop per slot to every `l`. A newly typed value copies the mark of the nearest value above it in that column, which gives most of a column mode with no setting. A stop can be added if `~` feels wrong in play.
*   **Events** and empty slots show no mark, and each slot grows by two characters.

### What this changes in the existing plan

*   **G1:** retired, as its question has been answered in play. No test gates the milestones, and "before G1" elsewhere in this document now means as built through M3.
*   **M3:** `S` already covers most of Issues 17 and 19, leaving plain values. Issue 18's live sweeps stay on hold, with G4 and CUJ 4 parked alongside them. M3's open work doesn't block M4. G7 still runs first, as M4 onward lean on having many tracks.
*   **After the milestones:** this section goes ahead of that list. Slicing and the song and score views move up from it into the milestones below. Tables, generators and the smaller items stay after them.

### Decisions

Made by the author on 2026-11-05:

1.  **Sections come before effects**, as a project with one looping section can't become a song however good it sounds.
2.  **3 reorderable inserts** rather than a free signal path or a fixed order.
3.  **No sample locks at first.** An `SMP` FX can come if they're missed in a jam.
4.  **Chains and shared phrases stay**, under sections, rather than Elektron-style patterns. They and the grammar are what make this not just another groovebox. Decision 10 narrows sharing to within a track.
5.  **Marks** as above: lock by default, lasting until the next note or value, glide from the marked row, and `~` to change them.

Made by the author on 2026-11-06, after a review of this section:

6.  **G1 is retired.** The author wants to continue, so milestones have no gate.
7.  **The shared parameter list waits** for the first effect, as nothing needs it before then.
8.  **Two of the same effect are allowed on a track**, though that won't be common.
9.  **A mixer page comes soon**, before songs, as it's such a common part of a workflow.
10. **Each track owns its chains and phrases**, numbered `00` to `FF` within it. A track is one sound, so a phrase's notes and values only mean something on its own track, unlike the M8, whose phrases name their instruments. Parts can be shared within a track, and copied but not linked between tracks. Made on 2026-11-06, along with keeping sections rather than the M8's per-track lists, and adding a row leaving it empty.
11. **The song view is a restricted Session View:** a row is a scene, a chain is a clip, and phrases are reusable parts inside it. There is no linear timeline, as detailed arrangement happens in Ableton from stems.
12. **No time signature in the model.** Meter comes from lengths, as on the M8, Polyend Tracker and Elektron machines. A bar is 16 steps, from one constant, until a song-wide setting is needed.
13. **A render is one performance of the song.** Random trigs, step chances and any later chance on rows may differ between seeds, but all stems of one render take the same route.
14. **A kit is a list of sounds to choose from**, so `:kit 909` adds no tracks and `:kit 909 all` does. Direct jumps are what keep lots of sounds quick on Elektron and Maschine, so tracks get a picker, `Space fb`, like Vim buffers. A drum rack would reverse "a track is one sound", so it waits for evidence from play.
15. **The first synth comes before FX inserts**, so a song can have bass as well as drums while song mode is tested: Plaits' virtual analog model, its main output, as the `VA` machine, one note at a time. A track holds a `machine`, and its Harm, Timbre and Morph work as `Cut` does, with step commands, LFOs and envelopes. The Sound page is two columns with a bar beside each value, after the M8's instrument page. The M8's Macrosynth is a port of Braids, an earlier Mutable module, so this differs from it. Added on 2026-11-06, ahead of M9.
16. **FM and Kick follow it**, ported from Plaits as well, for a wider palette with no large tables: FM for bass, bells and keys, and an 808-style bass drum. The page names a synth's three settings for what they do on each machine, and step commands stay `HAR`, `TIM` and `MOR` across all of them. Added on 2026-11-06.
17. **Plaits is one machine with a choice of voices**, as the M8 has Macrosynth with its `SHAPE`, rather than each voice being a machine. Added on 2026-11-06.
18. **Snare and Hat complete a synth kit** with Kick. Their noise comes from the track's seeded generator, so each hit differs while a render repeats for a seed. Added on 2026-11-06.
19. **Fold adds a third tone colour**, from Plaits' waveshaping model, its main output. Its tables are generated from the formulas in `lookup_tables.py` and kept in the source. Added on 2026-11-06.
20. **Small drawings use kitty's graphics protocol where it's available**, as plain one-colour lines of solid dots, after Elektron's screens: the envelopes, each LFO's shape, and the filter's curve, worked out from the filter's exact response. Other terminals get Braille diagrams and no icons, as does `--no-graphics`. Inside tmux, this needs `allow-passthrough on`. Placeholder characters mark where an image goes, so it stays in its pane and windows can draw over it. A changed image takes a new id, because kitty doesn't reconnect unchanged placeholders to a replaced image. Added on 2026-11-06.
21. **A value's bar moves with what's playing**, as in modern plugins: its fill follows the value after step commands and modulation, with a dim mark where it's set, and its number stays the setting. The callback reports these values for the track the cursor is on. Added on 2026-11-06.

## Alternatives Considered

### Automation lanes apart from the grid

*   **Description**: Give parameters DAW-style lanes in their own view or below the grid, and drop value commands from FX slots.
*   **Pros**: Any parameter, no slot limit, and values between steps.
*   **Cons**: A second kind of editing beside the grammar, values out of sight of their notes, and more to carry when a phrase is shared or copied. It also can't replace event commands, so slots would stay anyway.
*   **Reason for Rejection**: The columns can gain all three pros. See Direction.

### A free FX chain

*   **Description**: Any number of effects on a track, patched freely.
*   **Pros**: Flexible.
*   **Cons**: Hard to read in a terminal, with more to manage than the music needs at this scale.
*   **Reason for Rejection**: 3 reorderable inserts and bus tracks cover the need.

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

### After the milestones

This list comes after Milestones after M3, in order, as jams ask for it. Each entry gets its own short design.

1.  **Tables.** Per-note tables with their own speed, independent column rates, and aux tables, modelled on the M8. The author wants cycles that drift across notes, such as a 5-step table cycle against a 16-step phrase, which the M8's `TIC00` per-note mode allows. Open questions: how transpose and volume columns get a rate and loop point, end-of-table behaviour, when a cycle's position resets, a command that tests whether a note is held (a community M8 tutorial says tables cannot detect note-off, and the manual documents no such test), FX addressing such as `HOP` beyond row `F` and IDs above `FF`, table `00` as an aux table, and a safe rule for zero-time `HOP` loops. First command candidates: `HOP`, `THO`, `TIC`, `TBL`, `TBX`, `RND`, `REP`, `IFC`.
2.  **Sample slicing and waveform view.** The slicer moves up to M11, and `:bpm fit` and waveforms stay here. Chopping a break into slices, triggering them from the grid, `:bpm fit <bars>` to set the tempo from a sample's length, and Braille waveforms. The M8 and Polyend are the references.
3.  **Score view.** Moved up: full multi-track editing and selections across tracks to M6, and column folding (`za zc zo zM zR`) to M11.
4.  **Song view and looping.** Moved up to M8, apart from per-track speed and grooves, which stay here. Song view and section navigation. Unequal looping with the author's decision: each section has a length, and a section either **resets** tracks or **carries** their positions on. The default is to carry, and `:reset` is the one-section form of this. Any mid-song guarantee covers track positions only. Also per-track speed, M8-style grooves with `GRV`, how groove units scale with speed, a rounding rule, and sections ending mid-step.
5.  **Generators.** Masks and value sources, Euclidean with two pulse generators and boolean operators, stage sequences, chords across tracks with strum, and transformers modelled on Ableton's MIDI tools. When one generator writes to several tracks, decide whether editing one applies them all.
6.  **Smaller items.** Chasing held values on play, scales and `:set scale`, a fine-timing nudge command, keyframes in sweeps, track groups, macros, Vim-style marks, `/` search, sequence increments across cursors, Harpoon-style assignable view slots, Elektron-style conditions and fill mode, swing at group level as in Maschine, MIDI, device recovery, a portable project file, and `vimtracker export-json <project> [out.json]`, which an earlier test showed round-trips between RON 0.12.3 and JSON. Comments are not kept.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is split into issues, each intended as one pull request. Every issue leaves the workspace building with `cargo test` passing. The order after M3 is under Milestones after M3, and the rest is listed under After the milestones. The readiness bar applies to M1-M3 only. **M4 onward aren't ready to implement as written:** each gets a short design with its issues and file lists when it starts.

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
| `MAX_PHRASE_STEPS` | 4096 | The most steps a phrase keeps, so a big count can't use up memory |
| `BEAT_STEPS` | 4 | Steps per beat, for `ib` and `{ }` |
| `FX_SLOTS` | 3 | FX slots per step |
| `DEFAULT_BPM` | 120.000 | New project tempo |
| `HISTORY_LIMIT` | 1000 | Undo versions kept |
| `DEFAULT_RENDER_RATE` | 44100 | Hz, for tests and `render` |
| `PUBLISH_QUEUE` | 1 | Song versions waiting for the callback |
| `RETURN_QUEUE` | 16 | Versions waiting to be freed |
| `COMMAND_QUEUE` | 64 | Transport, preview and mute messages |
| `TELEMETRY_QUEUE` | 64 | Playhead and load messages; newest dropped when full |

**Tuning values** can change by ear with no document update: `SMOOTHING_MS` (10), `DECLICK_FRAMES` (64), `PREVIEW_MS` (250), `FRAME_MS` (33), and `AUTOSAVE_SECONDS` (60).

**Step text format.** Fields are separated by one space, and an FX slot is a command and value with no space. A hold or glide adds one letter, as in `CUT40h`.

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
| Normal | `Space i K` | From any view or column, choose the current instrument from those that exist, labels first. Writes nothing to the song |
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
| Normal | `p`, `3p` | Paste over from the cursor, some number of times end to end |
| Normal | `gp` | Paste over, and land just after it |
| Normal | `gm` | Merge paste: empty cells in the copy leave what is there alone |
| Normal | `K` | On the Instrument column, choose this step's instrument from those that exist, labels first |
| Normal | `;` | Audition: hear this row on every track while not playing, with no FX, changing nothing playback relies on. In the chain view, the start of that row. In the instrument view, that instrument at C-4 |
| Normal | `s` | Clear this field and start Insert there, as in Vim. An FX value keeps its command for the value typed next. `.` doesn't repeat it |
| Normal | `r` | Replace this field as in Vim, with no move, and return to Normal. A note takes one note key and keeps its velocity and instrument. Other columns take a whole value, and an FX command goes on to its value. `.` repeats it |
| Normal | `"a` before `d`, `y` or `p` | Use register `a` to `z`. `"0` holds the last `y`, which a cut can't replace |
| Normal | `=`, `-` | Up or down a small step: a semitone, or 1 on a value. `Ctrl-A` and `Ctrl-X` do the same where the terminal delivers them |
| Normal | `+`, `_` | Up or down a large step, the same keys with Shift: an octave, or `10` in hex. On `RET`, the small step changes only the rate digit and the large step only the fade digit, with no carry |
| Normal | `x` | Clear a step's field. On the track page, instrument view and chain Transpose, back to the default |
| Normal | `K` | Choose the field from a list: Instrument and FX columns, a chain row's phrase, a sample, and an LFO's `Dest`, `Shape`, `Rate` and `Retrig` |
| Normal | `~` | Cycle a value's mark: lock, hold, glide. In Visual too |
| Normal | `.`, `u`, `Ctrl-R` | Repeat, undo, redo |
| Normal | `gd`, `Ctrl-O`, `Ctrl-N`, `grr` | Open item, jump back, jump forward, list uses. `gr` is only a prefix |
| Normal | `gc`, `gC` | Clone, deep-clone |
| Normal | `gv` | Reselect the last selection |
| Normal | `gl` | Reopen controls for the live generator under the cursor |
| Normal | `Space ga` | Apply the live generator under the cursor |
| Normal | `Space pd` | Double the current phrase |
| Normal | `Tab` | Play, or pause in place |
| Normal | `Shift-Tab` | Play from the start |
| Normal | `Space c` | Play from the cursor's step |
| Normal | `i`, `v`, `V`, `:` | Insert, field Visual, step Visual, Command |
| Visual | Motions, targets, `d y > < = - + _ .` | Extend, then apply to the selection |
| Visual | `r`, then a value | Fill the cursor's column across the selection, as one undo. `vGra` writes C on each step. Velocity and instrument only go on notes, and FX values only on commands |
| Visual | `S` | Open the shape panel on one Velocity or FX column |
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
| Insert (note) | `c v` | Lower or raise the velocity written with each note, like the octave. It starts at `64`, leaving room for accents, and moves by the top digit from `04` up to `74`, then `7F`. The note heard while typing uses it |
| Insert | `Tab`, `Shift-Tab` | Play or pause, and play from the start |
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

**Status line.** It always shows the mode, swing and BPM. Errors and notices go here. How the program itself is running, meaning CPU load (callback time divided by buffer duration), the late count and xruns, sits at the top right of the view instead, and turns red on a dropout.

**Step numbers.** The phrase view shows each step's number in hex and, more faintly to its left, its distance from the cursor in decimal, which is the count to type before `j` or `k`.

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
| `:reset n`, `:reset off` | Send every track to its start every `n` steps, or let them run. Off by default |
| `:kit [name or path]` | Make a kit's sounds the ones `Space 5` lists first, or choose a kit. With `all` after it, put them on tracks from the first as one undo step. See Decision 14 |
| `:seed n` | Project random seed |
| `:set step=n` | Insert step |
| `:set skip=n` | How far `Left` and `Right` move in Insert |
| `:set limiter=on\|off` | Whether a render clips at full level or keeps levels above it. The live output always clips at full. It starts on and is saved with the project |
| `:len n` | How many steps the loop plays, while stopped. Decimal, or hex after `0x` as in Rust: `:len 0x10`. `:reset` reads numbers the same way. Steps outside it are kept and saved |
| `:double` | Double the loop, while stopped |
| `:gen apply` | Apply the live generator under the cursor |
| `:render path [--rows n] [--tail]` | Render to WAV |

The subcommands are `vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` and `vimtracker probe-keys`. `vimtracker [project]` starts the program.

**Rendering.** A render plays one pass silently to fill delay and reverb, then captures the next pass, so the file is exactly the loop's length. `--tail` appends up to 10 seconds, ending at silence. `--rows n` sets the length in steps. An existing file is overwritten. Output is 32-bit float stereo.

**FX commands.** Values are hex. `xx` values map linearly unless stated.

| Command | Meaning | Persistence |
|---|---|---|
| `VOL xx` | Track gain, on the level scale | Holds |
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
*   `docs/g7-result.md` (Create): the G7 result.
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
*   `src/mixer.rs`: track mix, sends and limiter.
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
*   **Issue 5: Engine and render.** Add `engine.rs`, `playhead.rs`, `track_state.rs`, `voice.rs`, `mixer.rs` and `render.rs`, with no FX commands yet. **Tests:** a note starts on its exact sample at 44100 Hz and 48000 Hz, including at a fractional tempo; a new note fades the old one; two renders match by hash; a 7-step chain keeps looping past a 16-step one, and a reset brings them together; replacing a sample does not move a playing voice; reads stop at a sample's end.
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

### Milestone M3: Sweeps

*   **Issue 15: FX and audio effects.** Add `fx.rs`, `rng.rs`, `dsp/*`, the commands in the FX table, FX entry in Insert mode, `:seed`, held values and equal-power pan. **Tests:** each command on a rendered result or engine state; `RET` with `y=0` does nothing; held values reset as specified; a fixed seed gives the same `CHA` results; adding a track does not change another track's results. **Done when:** the author has listened to a full `CUT` sweep over a sustained sample at 120 and 170 BPM, recorded the result in Open Questions, and added the step-length fallback if it sounds stepped.
*   **Issue 16: Swing.** Add `:swing` and `:tswing`. **Tests:** 66.7% gives exactly 80 ticks; 50% gives none; a track override does not move another track.
*   **Issue 17: Curves.** Add `curve.rs`. **Tests:** end values are exact in both directions; curves stay monotonic; over 9 steps from `00` to `7F`, curve bias 0.8 gives `00 2E 49 5A 66 6E 75 7B 7F` and curve gain 0.2 gives `00 05 0D 1B 40 64 72 7A 7F`; refused selections give a message.
*   **Issue 18: Live generators.** Add `generator.rs`, the `Phrase` field for them, their marker and `:gen apply`. `edit` applies any live generator whose owned columns it changes, except for a rerender. **Tests:** the rules under Live state; a rerender changes owned columns only and keeps it live; a live generator round-trips through saving; two generators on different columns are independent.
*   **Issue 19: Shape mode.** Add `shape.rs`, Visual `S`, `gl`, and `Space ga`. Each change rerenders and publishes without adding history. `Enter` adds one history entry and `Esc` restores the starting version. `.` applies the last shape to the current selection. **Done when:** CUJ 3 and CUJ 4 work with audio.
*   **Issue 20: Scope and meters.** Add the scope tap to `link.rs` and `views/scope.rs`, with a small master scope and per-track level meters using Braille and block characters. **Tests:** the real-time guard stays at zero with the tap active; a full tap drops frames rather than waiting.
*   **Issue 21: Render command.** Add `vimtracker render` and `:render`. **Tests:** golden hash tests; a render is exactly the loop's length; `--tail` extends it; `VT_BLESS=1` updates hashes. **Done when:** CUJ 7 works.
*   **Issue 22: Autosave.** Add autosave to `project.rs`. **Tests:** only a changed project is written; a newer autosave is offered on opening; audio is unaffected.
*   **Issue 23: G7.** Run G7, and record how many tracks the laptop sustains in `docs/g7-result.md`. G1 is retired.

### Milestones after M3

These milestones build the section Direction: a track is one sound, in order, and G7 runs before M4. Each gets a short design when it starts, and ends when the author has used it in a jam. M5 comes before M8 so songs aren't made in a format about to break, and M9 comes before M10 as send tracks reuse inserts. The Filters milestone on GitHub stays open for #50 alongside these.

*   **M4: Marks.** Lock and hold on the existing value commands, with `~` to change them, then glide. **Tests:** a lock returns to the value beneath it at the next note or value, a hold stays until playing from the top, and a glide lands exactly on its next value.
*   **M5: A track is one sound.** Remove instruments from steps, name tracks, kits as sets of sounds, a `Sound` type with the one-shot sampler as its first machine, a sample browser, the Sound and Mod pages, and `:render --stems`. This stops old songs loading. **Tests:** a track's sound saves and loads, stems sum to the full render without sends, and renders stay repeatable.
*   **M6: Editing across tracks.** Selections and operators that reach across tracks, as drum parts now spread over several.
*   **M7: Mixer.** One page of level, pan, sends, mute and solo across all tracks. **Tests:** an edit here matches the same edit on the track page, and is one undo step.
*   **M8: Songs.** Sections, a song view, section lengths, and each section resetting or carrying track positions. **Tests:** a song plays its sections in order, a render covers a whole song, and stems cover a whole song.
*   **M8b: Jamming.** Launch a chain on one track while playing, and capture what's playing as a new section. **Tests:** a launch waits for the track's chain to end, stopping a track, a launch ending with its section, capturing without a break, and no allocation in the callback.
*   **M8d: Session workflow.** Launch timing on a beat or bar grid, one-phrase chains opening straight to their phrase, chain labels, and cloning a chain with its phrases from the song view. **Tests:** as [its design](song-flow.md) lists.
*   **M8c: Song flow.** One position rule for rows and launches, so a changed chain starts from its top and a part can span rows, and rows that repeat. **Tests:** as [its design](song-flow.md) lists.
*   **M9: FX inserts.** 3 inserts on a track, the first effect, the FX page, the shared parameter list, and slots naming any parameter on that list. **Tests:** every parameter on the list works with each key from the list alone, an effect's response, reordering, and off costing nothing.
*   **M10: Tracks that process.** Send and mix tracks in a fixed routing, moving delay and reverb to send tracks with real settings, and a better reverb. **Tests:** a track's steps sequence its effects, and the cost of an idle track is measured.
*   **M11: More effects and machines.** Compressor with sidechain, saturation and bitcrush, 3-band EQ and chorus, then buses, column folding, the slicer, a kick synth and Plaits-style voices, with polyphony decided when one needs it. Only what's wanted in a jam gets built.

### M4 design: marks

Marks are meant to be judged in play, so these rules are the simplest that can be built and heard, and they may change. Anything not covered follows the Fallback rule. This applies only to the six value commands, `VOL`, `PAN`, `CUT`, `RES`, `DLY` and `REV`. There is no shared parameter list yet.

**Data.** `FxSlot` gains `mark: Mark`, one of `Lock`, `Hold` or `Glide`, with `Lock` the default. An empty slot or an event always has the default. In step text, a lock adds nothing, and the others add one letter: `CUT40`, `CUT40h`, `CUT40g`. A mark on an event or empty slot is a load error. `FORMAT_VERSION` stays at 1, so existing songs load but their values become locks and sound different.

**Layers.** For each of the six values, `TrackState` keeps a base, an optional lock and an optional glide. What's heard is the glide if there is one, then the lock, then the base. The result goes through today's `set_held` targets and smoothers, and LFOs and envelopes still add on top.

| Event | Effect |
|---|---|
| A `.` value | Sets that parameter's lock, and ends its glide |
| An `h` value | Sets its base, and ends its lock and glide |
| A `g` value | Starts a glide from this value, and ends its lock |
| A row with a note that plays | Ends every lock on the track, before that row's values run. It doesn't end a glide, so a sweep can cross notes |
| A note a failed `CHA` skips, `OFF`, `RET`, `KIL` | Ends nothing, as the same note is still ringing or fading |
| A `DEL` note | Ends locks at its row, not when it sounds |
| An edit on the track page | Sets the base, replacing a hold. A lock or glide stays on top until it ends |
| Starting from the top | Clears locks and glides, and bases return to the page's values, as held values do today |
| Pause and resume, `:reset`, wrapping, chain moves | No effect |

Since `CHA` decides whether a row's note plays, it is read before locks end. It draws its random numbers in the same order as today. Slots still run left to right, so when two slots on a row name a parameter, the rightmost wins. Locks and holds on a track are per parameter, so `DLY30` doesn't end a `CUT` lock.

**Glide.** At a `g` value, the engine looks forward through the phrase's loop in that slot, wrapping, for the next value with the same command, and skips other commands. With none, the value stays where it is, as a lock would. Otherwise the value moves in a straight line in slot units, from this row to that row, following song position so it keeps up with tempo changes. It updates on the same schedule as the LFOs, through the existing smoothers.

*   **Arrival:** the arrival row runs as normal, with its own mark. It lands exactly, as the row's value replaces the glide.
*   **Chains:** the search stays in the phrase's own loop, so a phrase means the same everywhere. If a chain moves on first, the glide finishes on time and takes the mark of the value it was heading for.
*   **Cost:** the search is at most `MAX_PHRASE_STEPS` rows, doesn't allocate, and runs only on a glide row. If that shows up on the CPU readout, it can be worked out when a version is published.

**Editing.**

*   **`~`** in Normal cycles lock, hold, glide on the slot under the cursor, on either of its fields. On an event or empty slot it does nothing and says why.
*   **`~` in Visual** moves every value in the selection to the mark after the first value's, so a mixed selection ends up the same.
*   **`.` and undo:** `~` repeats with `.`, and each use is one undo step.
*   **New values:** a value typed into an empty slot copies the mark of the nearest value above it in that column in the phrase, without wrapping, or is a lock if there is none. A value typed over another keeps its mark.
*   **Moving and clearing:** `y`, `p`, `d`, rotate and slide carry marks, as they move whole slots. `x` clears the mark with the slot.
*   **Shape panel:** it keeps a mark already on a row, and otherwise writes locks. Writing glides is left out for now.

**Display.** Each slot grows by two characters: `CUT40 . ---00   CUT80 g`. A lock's `.` is dim, `h` and `g` are normal, and events and empty slots show a space. Marks aren't cursor stops.

**To judge by ear, as they may change the rules:**

*   A sweep ending in a lock jumps back at the next note. If that's wrong most of the time, a glide's arrival could hold instead.
*   A lock ending on a drum's attack passes through the 10 ms smoother, which may soften the hit.
*   Whether lock is the right default in practice, and whether a column of dim dots is too busy.

**Issues.**

1.  **Marks in the model and phrase view:** the data, step text, display and editing above, with marks stored but not yet heard. **Tests:** step text round-trips each mark, a mark on an event is refused, `~` cycles and is one undo step, `.` repeats it, Visual `~` makes a mixed selection the same, and a new value copies the mark above.
2.  **Lock and hold in the engine:** the layers and the table above. Existing tests that expect values to hold gain `h`, which also leaves the goldens unchanged. **Tests:** each row of the table, the rightmost slot winning, an LFO carrying on around a lock, and renders staying repeatable. The author plays this before glide starts.
3.  **Glide in the engine.** **Tests:** the value at the halfway row, landing exactly, wrapping, a skipped command, no other value, crossing a note, a tempo change, a chain moving on, and no allocation under the real-time guard.

### M5 design: a track is one sound

Made with the author's agreement on 2026-11-06. It keeps each change small, and leaves anything a later machine or file format would decide until then.

**Data.** `Track` gains `sample: Option<String>` and `name: String`. `Step.instrument`, `Song.instruments`, `Instrument` and instrument volume go, as `Level` already does that job. A note's gain is its velocity times the track's values. `FORMAT_VERSION` becomes 2, and older songs are refused with a message, with no conversion. A track that played several samples has to be rebuilt as several tracks.

**No `Sound` type or machine enum yet.** A track's sound is everything on `Track` apart from mute, solo, swing and name, and that's saved in the song. A separate type comes when a sound can be saved alone or a second machine exists, as nothing needs one before then. That's M11, or sooner if a jam asks.

**Step text.** `NOTE VEL FX FX FX`, as in `C-4 64 CUT40 ---00 RET14`. Field and column numbers shift down by one.

**Names and kits.** `:name BD` names the current track, under the existing label rule of two characters starting with a letter, and `:name` alone clears it. A named track shows its name where its number was. `:kit` puts a kit's sounds on tracks from the first in order, as one undo step, adding tracks if it has more sounds than there are tracks. A sound's `label` becomes its track's name, and other settings on those tracks stay.

**Retired.** `:inst`, `:label`, `Space i K`, the Instrument column and its `K`, `gd` to an instrument, the instrument view, its mode and guide, and `Space 5`'s old meaning. `:sample` now sets the current track's sample.

**Audition.** `;` and notes played while editing go through the current track's own filter, levels and sends, so what's heard matches what plays.

**Pages.** The track page splits into Sound (Sample, Filter, Output) and Mod (Envelopes, LFOs, diagrams), and `H` and `L` move between them there, overriding their usual meaning on this page only. The FX page arrives with inserts in M9. `Space 5` opens the existing sample picker for the current track, as a picker rather than a new view, following the picker-first rule. On the Sample row, `K` opens it too, and `x` clears the sample.

**Stems.** `vimtracker render <project> <folder> --stems` and `:render <folder> --stems` write `01-BD.wav`, `02.wav` and so on, one file per track that would be heard. Each is a render of the song with only that track soloed and the limiter off, so it carries its own delay and reverb, and needs no new engine path. They sum to a full render with the limiter off, to within a small tolerance. With `--tail`, shorter stems are padded with silence so they all end together.

**Issues.**

1.  **Tracks own their sounds:** the data, step text, names, kits, retired items and audition above. **Tests:** golden renders unchanged, a song older than version 2 refused, kit loading adds tracks and is one undo step, `:name` refuses bad names, each track plays its own sample, audition goes through the track's filter and allocates nothing.
2.  **Sound and Mod pages, and the sample picker.** **Tests:** `H` and `L` change pages and the cursor stays valid, every control is on exactly one page, `Space 5` and `K` set this track's sample, and `x` clears it.
3.  **Stems.** **Tests:** the stems sum to the full render, a muted track gets no file, a soloed project writes only those tracks, tails end together, and renders stay repeatable.

### M6 design: editing across tracks

Column folding stays in M11. This adds one thing: a Visual selection can reach other tracks with `W` and `B`, and the operators that already work on a selection then work on all of them, as one undo step.

*   **Rows.** A selection is rows of the focused phrase and a run of tracks. `W` and `B` extend the run and leave the cursor on its track, so reaching a shorter track doesn't change the rows. On another track those rows count from that phrase's own loop start, as the neighbour columns show them, on the phrase it's on level with the cursor. Rows a phrase doesn't have are left out.
*   **Columns.** The same columns on each track. A neighbour column only shows notes, so it lights up by row even when the selection is FX columns.
*   **Operators.** `d`, `y`, `>`, `<`, `=`, `-`, `+`, `_`, `r` and `~` work across tracks. Shape refuses, as a shape belongs to one column.
*   **Copy and paste.** A block keeps each track's steps, and `p` puts them on the tracks from the cursor's onward at the same row. Tracks past the last are dropped.
*   **Repeat.** `.` repeats on the same number of tracks from the cursor, apart from `r`, which repeats on one track outside Visual.

**Issue.** **Tests:** `d`, `>`, `~` and `r` across tracks, one undo step, a shorter phrase, a slid loop, `y` then `p`, paste past the last track, `.` repeating, `gv`, Shape refusing, and the neighbour columns lighting up.

### M7 design: mixer

`Space 6` shows a row per track with `Level`, `Pan`, `Delay`, `Reverb`, mute and solo, scrolling to keep the cursor's track in view. It edits the same settings with the same keys as the track page, with `j` and `k` moving between tracks. A row per track rather than a strip per track, as that fits the 64-track limit and the grammar in a terminal. There's no new engine work, and level meters per track can follow if a jam asks.

**Issue.** **Tests:** moving, stepping, typing and `x` on each kind of column, one undo step matching the track page's edit, mute and solo reaching the engine, and a long list scrolling.

### M8 design: songs

Made without the author present, on 2026-11-06, following their earlier decisions: each section has a length, and either resets tracks or carries their positions on, with carry the default. These choices can change once played.

**Data.** `Section` gains `length: Option<usize>`, with none meaning its longest chain, and `start`, carry or reset. `reset` stays, as the periodic realign `:reset` sets, and now belongs to one section.

**Playback.** Sections play in order, and the song wraps to the first. A set length cuts chains short, or lets them loop on. The engine's global step keeps counting, so timing and swing don't change at a boundary. **Carry** means a track's position stays that count modulo its new chain's length, which is exact and repeatable, but a chain of a different length lands wherever the count falls. **Reset** sends the count to 0. Held values, locks and glides cross boundaries untouched. A section with no length is skipped. One section wrapping onto itself behaves exactly as before, as the goldens confirm.

**Transport.** `Shift-Tab` plays the song from the top, `Space c` from the cursor in the section being edited, and `Tab` pauses in place. `:loop` keeps playing the section being edited.

**Editor.** `Cursor` gains `section`, and the phrase, chain, track and mixer views edit that section. Playheads show only when it's the one playing. Shared phrase and chain counts, and `grr`, cover the track in every section. The status line shows `Sec 2/4` when there's more than one.

**Song view.** `Space 1` shows a row per section with Length, Start and each track's chain. `a` adds an empty section, which is skipped until it has a chain or a length, `gc` copies one using the same chains, and `dd` removes one. `gd` opens a chain, adding one on `--`.

**Render.** With more than one section, a render covers the song once. Its warm-up is one silent pass through the song, and one section still renders as its loop.

**Not included.** Per-track speed, grooves and sections ending mid-step stay in After the milestones.

**Issue.** **Tests:** sections in order and wrapping, lengths, carry and reset, one section unchanged, `:loop`, playing from a section, empty sections, removing the playing one, renders and stems covering a song, and each song view key.

### M8b design: jamming

From the author on 2026-11-06, after Ableton's Capture and Insert Scene, which needs tracks that can be launched one at a time, as in the M8's Live Mode.

**Launching.** `Enter` in the song view, while playing, cues the chain under the cursor on its track. It starts when the track's current chain next ends, the M8's default, and plays from its own start. On a `--` it stops the track at that point and cuts its voice. The first launch keeps the song on the section playing, so the jam doesn't move on.

**State.** Launches live in the engine rather than the song, so undo doesn't touch them. The engine reports what changes to the edit thread, on a queue the callback can't block on. A launch ends when another section starts, or when playing starts again, and survives a section wrapping onto itself. A reset starts a launched chain over with everything else.

**Capture.** `C` adds a section below the one playing, with each track's launched chain, or its row's chain if it has none, as one undo. A stopped track is `--`, and it's refused if everything is stopped. Rows point at the same chains rather than copies, and have no set length. Playback carries on in the new row, with launched chains running, so nothing audible changes.

**Limits to judge in play.** Tracks launched at different times can be out of step, so a captured row can sound different from its start. A launch can't be queued for a whole row at once, and there's no shorter launch quantise than the chain's end. The phrase and chain views' playheads still follow the section's chains rather than a launched one.

**Issue.** **Tests:** as the milestone lists, and each key's refusals.

### M8d and M8c designs

Built, from [Song flow and session workflow](song-flow.md), which replaces parts of the M8 and M8b designs above.

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
