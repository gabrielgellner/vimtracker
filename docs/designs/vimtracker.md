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
*   **Step:** one position in a phrase, and one sixteenth note at normal speed. From M13, a
    phrase's positions are called **rows**, and a step is only the unit of time: a row with a
    note lasts as many steps as its pulses.
*   **Pulse:** one step of time, as a note's length is counted, from M13.
*   **Lane:** one column of a phrase playing on its own loop, from M12. The note column's loop is
    the phrase's loop.
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
*   Grid column types, such as chord columns. Chords are generators that write to ordinary
    tracks, and so are note transformers like Ableton's. Machines and bus tracks, under
    Direction, differ in what a track sounds like or takes in. Every track keeps the same rows and
    the same grammar. The exception, decided with the author on 2026-11-07, is how a phrase's
    columns play in time: lanes (M12) and pulses (M13) change how the grid itself plays, and the
    phasing that comes from that is the point, which a generator writing rows can't give.
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

**Note preview.** Entering a note sends a preview command. The engine has a bank of eight preview voices
apart from the tracks' own (decision 54), which play through the focused track's held filter, pan and
volume, without running that step's FX. A preview never cuts a sequenced note, and a sequenced note never
cuts a preview.

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
character filter: a saturating four-pole ladder at 2x, from Huovilainen's model, Zavalishin's integrators and Mystran's linearisation, after VCV Fundamental's VCF, which is GPL 3 or later. It self-oscillates, and its cost only falls on tracks using it. `Diode` is Vital's diode ladder, the one in `Acid`, at 2x too (decision 47). `MS20 LP` and `MS20 HP` are the Korg 35, the MS-20's filter, from sst-filters' K35, at 2x too (decision 61). `Dirty` is Vital's Dirty filter, a 24 dB low-pass whose resonance loop saturates, at 2x too (decision 71). `Mode`, under `Filter`, gives each type its other responses from what it already has, rather than more types: `12 dB` or `24 dB` for the clean four, a second state-variable filter after the first; `LP24`, `LP12`, `BP`, `HP12`, `HP24` and `Notch` for `Ladder` and `Dirty`, their stages mixed as Vital mixes them; and where the diode's input high-pass sits. The MS-20's have none. A track keeps its mode across types, a type that doesn't offer it plays its first, steps can lock `MODE`, and a change glides (decision 72).
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
    next unused ID, assigns it and opens it. `Ctrl-O` jumps back, `Ctrl-I` jumps forward, and `grr` lists uses in the
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
first command selected, and rows holding another command keep it. A `Fill` of `Random` picks from the range instead, `Wave` follows an LFO's shape, rate and phase by where each row falls in time, and `Alternate` gives the rows it changes each value in turn, for `Run` rows at a time. `Where` chooses the
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

Each machine and effect declares its settings: name, range, units and default. The track page, stepping, `x`, `K`, typing, modulation `Dest` lists and FX on steps all read that list, so a new machine or effect needs no new UI. It's built with the first effect, in M9, when something new first needs it, and at first covers effects only, as the M9 design says. Until then, the existing controls stay as they are.

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
7.  **The shared parameter list waits** for the first effect, as nothing needs it before then. M14 carries it out for every setting.
8.  **Two of the same effect are allowed on a track**, though that won't be common.
9.  **A mixer page comes soon**, before songs, as it's such a common part of a workflow.
10. **Each track owns its chains and phrases**, numbered `00` to `FF` within it. A track is one sound, so a phrase's notes and values only mean something on its own track, unlike the M8, whose phrases name their instruments. Parts can be shared within a track, and copied but not linked between tracks. Made on 2026-11-06, along with keeping sections rather than the M8's per-track lists, and adding a row leaving it empty.
11. **The song view is a restricted Session View:** a row is a scene, a chain is a clip, and phrases are reusable parts inside it. There is no linear timeline, as detailed arrangement happens in Ableton from stems.
12. **No time signature in the model.** Meter comes from lengths, as on the M8, Polyend Tracker and Elektron machines. A bar is 16 steps, from one constant, until a song-wide setting is needed.
13. **A render is one performance of the song.** Random trigs, step chances and any later chance on rows may differ between seeds, but all stems of one render take the same route.
14. **A kit is a list of sounds to choose from**, so `:kit 909` adds no tracks and `:kit 909 all` does. Direct jumps are what keep lots of sounds quick on Elektron and Maschine, so tracks get a picker, `Space fb`, like Vim buffers. A drum rack would reverse "a track is one sound", so it waits for evidence from play.
15. **The first synth comes before FX inserts**, so a song can have bass as well as drums while song mode is tested: Plaits' virtual analog model, its main output, as the `VA` machine, one note at a time. A track holds a `machine`, and its Harm, Timbre and Morph work as `Cut` does, with step commands, LFOs and envelopes. The Sound page is two columns with a bar beside each value, after the M8's instrument page. The M8's Macrosynth is a port of Braids, an earlier Mutable module, so this differs from it. Added on 2026-11-06, ahead of M9.
16. **FM and Kick follow it**, ported from Plaits as well, for a wider palette with no large tables: FM for bass, bells and keys, and an 808-style bass drum. The page names a synth's three settings for what they do on each machine, and step commands stay `HAR`, `TIM` and `MOR` across all of them. Added on 2026-11-06. Changed by decision 29: each voice registers its own settings and labels.
17. **Plaits is one machine with a choice of voices**, as the M8 has Macrosynth with its `SHAPE`, rather than each voice being a machine. Added on 2026-11-06. Reversed by decision 29.
18. **Snare and Hat complete a synth kit** with Kick. Their noise comes from the track's seeded generator, so each hit differs while a render repeats for a seed. Added on 2026-11-06.
19. **Fold adds a third tone colour**, from Plaits' waveshaping model, its main output. Its tables are generated from the formulas in `lookup_tables.py` and kept in the source. Added on 2026-11-06.
20. **Small drawings use kitty's graphics protocol where it's available**, as plain one-colour lines of solid dots, after Elektron's screens: the envelopes, each LFO's shape, and the filter's curve, worked out from the filter's exact response. Other terminals get Braille diagrams and no icons, as does `--no-graphics`. Inside tmux, this needs `allow-passthrough on`. Placeholder characters mark where an image goes, so it stays in its pane and windows can draw over it. A changed image takes a new id, because kitty doesn't reconnect unchanged placeholders to a replaced image. Added on 2026-11-06. Decision 56 replaces the parts about other terminals, tmux and `--no-graphics`.
21. **A value's bar moves with what's playing**, as in modern plugins: its fill follows the value after step commands and modulation, with a dim mark where it's set, and its number stays the setting. The callback reports these values for the track the cursor is on. The filter's curve moves the same way, sending a new image whenever `Cut` or `Res` changes, at most once a frame. Added on 2026-11-06.
22. **`/` on the track page or mixer jumps to a setting by name**, following the author's flash.nvim: labels go over the values of the matches on this page, nearest the cursor first, a label is never a letter that could carry on the search, and a match keeps its label as you type. The mixer labels the cursor's track, which it stays on. It lands in Normal mode, and `Ctrl-O` comes back. Added on 2026-11-06.
23. **`Option-1` and `Option-2` narrow and widen the grid the arrows move by in Insert**, after Ableton's `Cmd-1` and `Cmd-2`. Terminals don't deliver `Ctrl` with a number: `Ctrl-1` arrives as `1` and `Ctrl-3` as `Esc`. Rows are 16ths, so there's no triplet grid. Added on 2026-11-06.
25. **The mixer is a strip per track**, as on Polyend Tracker: a name on top, a level meter and a fader side by side, and the settings below. The callback reports each track's level after its level and pan with no allocation. `h` and `l` move between strips, and `j` and `k` up and down them. Added on 2026-11-06.
24. **A scale is an aid to input, for the whole song.** It has a root and a kind held apart, and an on and off that keeps them. While it's on, the home row is the scale's degrees from `a` on the root, after Elektron's `KB FOLD`, and the row above is silent. `=` `-` `>` `<` move through it, and `C-a` `C-x` stay chromatic. Written notes never move on their own, after Ableton, and `:scale fit` moves them when asked. Nothing that plays the song reads it. It's outside undo and written on save, as mutes are. Added on 2026-11-06.
26. **Waves and 6-op follow from issue #134**, from Plaits' wavetable and six-operator engines, their main outputs. Their waves and DX7 patch banks are taken from Plaits' generated `resources.cc` into binary files built into the program, as regenerating them needs Plaits' Python 2 scripts. 6-op plays one note with no second voice for overlapping tails, which the track's envelope gives instead, and it releases its own envelopes when the track's does. `Patch` is the patch's number, `00` to `5F` across the three banks, rather than a level cut into 96 pieces, so `=` moves one patch and a step's `HAR20` plays patch `20`; the track page names the one chosen, and `K` lists them by name. Each costs about 0.2% of a core on an M5, less than `FM`, as both read a sine table. Added on 2026-11-07.
27. **String, Modal, Dust and Noise follow from issue #134**, Plaits' struck and noisy voices, their main outputs, through Plaits' limiter as it has them. String and Modal ring out from a strike and die away by themselves, as the drums do. Dust and Noise sound until the note ends, shaped by the track's envelope rather than Plaits' low-pass gate. Plaits keeps three strings so one rings under the next note, and here a track plays one, as it does every voice. Each new note starts with its settings where they are, rather than sweeping from the last note's, as each note is a new voice. A pluck puts little into Plaits' string, so it's about 20 dB quieter than the other voices; a gain of 8 before the limiter brings it level, and the limiter catches the brightest plucks. Their shared parts of stmlib, with the drums' filter, are in one file. Each costs at most 0.5% of a core on an M5, and Dust's diffuser takes a voice to 16.8 KB, allocated ahead of time like the rest. Added on 2026-11-07.
28. **Accent is a synth's own setting, apart from velocity**, after Elektron, where velocity and tone lock apart. It's Plaits' accent, how hard a sound is struck, which Plaits takes from its level input along with its loudness. Here velocity stays loudness alone, so a quiet note can be struck hard. Accent is a track setting below the three, from `00` to `FF`, starting at `CC`, Plaits' 0.8 with nothing patched, so songs made before it sound the same. `ACC` sets it from a step and holds, and LFOs and envelopes can move it. It changes the voices Plaits gives an accent to: the drums' strike, `String` and `Modal`'s brightness and ring, and `6-op`'s velocity. The others ignore it, so it's dimmed on them and isn't offered as a destination. Added on 2026-11-07. Changed by decision 29: Accent becomes a setting of the voices that use it, and `ACC` its label.
29. **Settings are registered once, and steps lock them by id**, after Elektron's locks and DAW automation, rather than through a list of named commands. Each has a long name for finding it and a label of 3 or 4 letters for the grid, typed and confirmed with `Enter`. Each Plaits voice is a machine with its own settings under their real names, which a track keeps for each voice, and a lock belongs to its voice or effect, drawn faint where it doesn't apply. Events (`DEL`, `KIL`, `OFF`, `RET`, `CHA`) stay a short list of their own. This changes decisions 16 and 28, reverses 17, and carries out 7. The [M14 design](#m14-design-one-list-of-settings) has the rest. Added on 2026-11-07.
30. **A setting's kind decides what its value means**, after tracker-ce's parameters and CLAP's stepped flag. A value is stored as its kind's own number: a `u16` of coarse and fine bytes for a continuous setting, and an integer for a stepped one, so a list can grow without moving saved values. A mode, like Distort's Type, owns its own settings as a voice does, rather than changing another setting's kind, and isn't lockable. The grid stays in hex, with coarse digits bold and fine ones set back, and the status line shows real units. Controllers and micro-timing are separate. The [M14b design](#m14b-design-kinds-of-setting) has the rest. Added on 2026-11-07.
31. **Plaits' pitched voices follow from issue #134**: `VA VCF`, `PD`, `Formant`, `Additive` and `Swarm`, from Plaits' filtered virtual analog, phase distortion, granular formant, harmonic and swarm engines, their main outputs. Each is a voice with three settings of its own, named for what they do, and each holds its note until the track's envelope ends it. `Swarm` takes each note as Plaits takes a trigger, so a note starts a fresh burst of grains. Each was set to about the loudness of `VA` at its own defaults, and none costs more than about 0.1% of a core on an M5. Formant's Z and VOSIM oscillators, and Additive's organ output, feed only Plaits' second output, so they're left out. Added on 2026-11-07.
32. **Terrain, Chord and Chip finish issue #134**, from Plaits' wave terrain, chord and chiptune engines, their main outputs. `Chord` and `Chip` choose their chord from a list of Plaits' 11, so `=` steps one chord at a time and `K` lists them by name, and each lands in the middle of Plaits' quantiser so it never flickers. `Chip` plays its chord's notes together, Plaits' path with no clock patched: the arpeggiator and the triangle bass on the second output are left out, as the rows sequence the notes. Terrain and Chip leave an offset at some settings, which Plaits' output coupling takes away in hardware, so each has a DC blocker. Speech and String Machine stay out. Added on 2026-11-07.
33. **An audio page chooses the output, from issue #122**, after Ableton's audio preferences: the device, its rate and its buffer, on `Space 7`. A change takes effect at once, as the main loop stops the stream, opens the new output, loads the samples again at its rate and builds a new engine, keeping the song, undo, mutes and solos. The choice belongs to the machine, so it's kept in `audio.ron` in the config folder, and anything chosen that isn't there now falls back to the system's own. A device that changes or goes away stops audio and says so, rather than the program following it or quitting. The page shows the latency the device reports from each callback, the buffer and the device's own delay together, beside the CPU, late and xrun counts, so a buffer can be lowered until it strains. The sound doesn't depend on the buffer, as steps fire on their frame and modulation moves at a fixed rate. Added on 2026-11-07.
34. **The effects come in an order, from what's at hand**, in GitHub's "More effects and machines" milestone: an ensemble and chorus from Rings (#158), a 3-band EQ on the state-variable filter after Andrew Simper (#159), a compressor with a sidechain after Giannoulis, Massberg and Reiss (#160), Clouds' pitch shifter (#161), Vital's phaser and flanger (#162), a look-ahead limiter after x42's (#42), Airwindows' tape and console (#163) and Beads' granular engine (#164), then a Dattorro plate (#165), Warps' cross-modulation (#166) and Streams' low-pass gate (#167). Mutable's pieces come first where they fit, as they're small, MIT, and built as the reverb is. `Chorus` is the first: Rings' chorus and ensemble as its two types, in delay lines from the pool scaled to the rate, with the ensemble's right side a sixth of a turn on from its left so a single sound spreads. Added on 2026-11-08.
35. **`EQ3` is three bands of Andrew Simper's state variable filter** (#159): a low shelf, a bell with its own Q and a high shelf, each mixing the filter's outputs into its response, after Cytomic's "Linear Trapezoidal Integrated State Variable Filter With Low Noise Optimisation". It's small enough to keep in every slot, so it needs nothing from the effect pool. Each band's boost is 15 dB either way of `80`, its frequency runs from 20 Hz to 20 kHz as the cutoff does, and the status line shows them in dB and Hz. Its coefficients are only worked out again while a setting moves, so a still EQ costs three filters a side, and a flat one passes the sound through exactly. Added on 2026-11-08.
36. **`Comp` is a feed-forward compressor with a sidechain from any track** (#160), after Giannoulis, Massberg and Reiss (JAES 2012): a soft knee of 6 dB worked out in dB, then their smooth branching peak detector on how far it turns down, with both sides linked. Its `Source` is itself, a track or a return. The engine keeps each track's sound after its slots from the latest frame, and a compressor reads its source's. Tracks render in order, so a source later than the listener is a frame late, about 0.02 ms, and two keyed from each other still work, so nothing is reordered. The author chose the tap Ableton calls Post FX: after the source's slots, before its level and mute. So a muted track a compressor hears keeps playing its notes, faded out of the mix and the sends, and only those do; muting any other track still stops it as before. `Auto`, off to start, measures how loud the sound goes in and how loud it comes out, each over about a second, and makes up the difference, up to 60 dB, holding still when what goes in is silent. So it comes out as loud as it went in at any setting, and switching it in or out compares like with like. Two rules were tried first and dropped. The paper's, making up what the curve takes from a sound at 0 dB, added up to 60 dB to everything under the peaks with the threshold right down. Giving back the average of how far it turned down left hits quieter and the gaps between them louder, about 12 dB off overall on a drum loop under a limiter. Auto is left off for ducking, which shouldn't be louder unducked. Threshold and ratio glide as other settings do, so turning them doesn't click. A meter for how far it's turning down, a filter on what it hears, and a look-ahead are left for later. Added on 2026-11-08.
37. **Any FX slot can be bypassed with `b`** on the FX page, to hear a track with and without it. The slot fades over the smoothing time, so switching doesn't click, and keeps running underneath, so a tail or a compressor's state carries on as if it had never gone; Ableton's device switch cuts a reverb's tail instead. Bypass is saved with the song as part of the track, one flag a slot, and emptying a slot clears it. Added on 2026-11-08.
38. **`Limit` is a look-ahead peak limiter** (#42), after Geraint Luff's "Designing a straightforward limiter" for Signalsmith Audio, with x42's `peaklim.cc` read for comparison. The sound waits 2 ms in a line from the effect pool, sized at the pool's rate as the chorus's is, while the gain it needs is worked out ahead of it: each frame asks for what brings its louder side to the ceiling, a running minimum holds the least asked for over the look-ahead, a one-pole lets go of it over `Release`, and a moving average as long as the look-ahead smooths the turn down. So it's fully down when a peak comes out, and nothing gets over the ceiling, which a test checks to within 0.01%. Both sides share the gain. The sound is driven and divided by the ceiling as it goes in, and the ceiling put back as it comes out, so moving `Input` or `Ceiling` can't let a peak already in the line through. `Input` goes from 0 to +24 dB, `Ceiling` from -24 to 0 dB, starting at about -0.3 dB, and `Release` from 1 ms to a second, evenly in ratio, starting at 100 ms. `Mix` blends in the dry sound from the same line, so it's as late and doesn't comb. It's a slot effect, meant for the master's first slot, and the master's own limiter stays as it is after it. Looking ahead makes the track it's on 2 ms late, and as nothing here compensates for delay, one limited track sits that much behind the rest; on the master everything is late together. It doesn't oversample, so a peak between samples can go slightly over; true-peak limiting is left for later. Added on 2026-11-08.
39. **`Phaser` and `Flanger` are two effects after Vital's** (#162, from #91), each with a `Rate` that's a note length: the note divisions, then 2 to 16 bars, from a list of its own, so a sweep keeps time with the song as the delay's time does. The sweep runs free from when the slot starts rather than locking to the bar, and each side sweeps a third of a turn from the other, as Vital starts, so a sound in the middle comes out wide. Both start half wet, which is where the dry sound cancels deepest; all wet is a pure pitch wobble for the flanger and a phase shift for the phaser. `Phaser` is Vital's `PhaserFilter` taken out of its processor graph: twelve first-order all-passes in a row, whose frequency a triangle sweeps in pitch, up to 48 semitones either way of a `Centre` on the cutoff's 20 Hz to 20 kHz scale, with what comes out band-passed two decades either side, softly clipped and fed back to the front. `Stages` chooses taking the sound after 4, 8 or 12 of them, fading between them, so it can change while it plays. The soft clip in the feedback holds it bounded even with feedback full and the notches at their sharpest. It's small, so it's kept in every slot as the EQ is, and its coefficient is worked out every 16 frames and moved in a straight line between, which costs it about 0.5% of a core on an M5. `Flanger` is a short line from the effect pool read between samples, as the chorus's, whose delay a triangle sweeps from 0.5 ms up to twice `Delay` at full `Depth`, as Vital's does, with `Delay` from 0.5 to 10 ms. Its `Feedback` is none at `80` and runs to 0.95 of what's read, either way up, so it can never run away; a soft clip with 12 dB of room on what goes into the line catches anything very loud. Every continuous setting glides over the smoothing time. Added on 2026-11-08.
40. **`Tape` is Airwindows' ToTape9 with Beads' wow and flutter** (#163), the newest ToTape at the time, ported from its LinuxVST source under MIT with Chris Johnson's notice kept. In order it drives the sound, lifts its highs through ToTape's "Dubly" encoder, wobbles it in a short line from the effect pool, biases it, adds a tiny hysteresis, darkens what moves fast before and after a saturation that's the start of a sine's series, adds the heads' bump, takes the highs back down through the decoder, and holds it under about -0.5 dB with ToTape's ClipOnly3. Of ToTape's nine controls, `Input` is `Drive`, from -18 to +18 dB with 0 dB at `80`, `Tilt` is `Tone`, flat at `80`, and `Bias` and `HeadBmp` are `Bias` and `Bump`; all start where ToTape's do, half way. `Bias`'s top is held at three quarters of ToTape's, as its full overbias closes the slew limits and falls silent. `Shape` and `HeadFrq` stay half way, a head bump at 68.75 Hz, to keep to six settings besides `Mix` and `Gain`, which take the place of ToTape's output and dry/wet. ToTape's flutter is left out for Beads' `WowAndFlutter`: a slow 0.23 Hz sine and a steady 1.99 Hz one, with a slower drift at a random speed changing the slow one's speed and depth. Beads' amount is `80` on a squared scale, so `FF` is four times Beads', about 9 ms of slow and 1.4 ms of fast movement; both start at `40`, a sixteenth of the most, which is barely heard. Its line is read between samples with a cubic, and Beads' low-pass after it is left out, as `Tone` does that job. The wobble is worked out every 32 frames, as Beads does once a block, and moved in a straight line between. Airwindows' noise is left out: its dither, its trick against denormals, and the noise that moves ClipOnly3's level, so the same sound always comes out the same. ToTape's shift loops in ClipOnly3 leave every intermediate sample the newest, so it's ported as it behaves: a frame late, at any rate. It's about 0.7% of a core at 48 kHz on an M5. Density, Airwindows' saturator, was looked at for an extra setting and left out: it's another sine shaper alongside `Drive`, and seven settings is more than this effect wants; it could be a slot of its own. Console, a channel-and-bus pair for how the master sums, is left for later, in #175. Added on 2026-11-08.
41. **`Plate` is Jon Dattorro's plate, a second reverb beside the Clouds one** (#165), from his "Effect Design, Part 1" (JAES 1997), ported from el-visio's `dattorro-verb` under MIT with Pauli Pölkki's notice kept, and checked against the paper. It's its own slot effect rather than a `Type` on `Reverb`, as its settings differ. Both sides are summed into a pre-delay, a one-pole band-limit, and four input all-passes, then a tank of two halves that feed each other, each a wobbling all-pass, a delay, a damping one-pole, a second all-pass and a second delay; each side comes out as the paper's seven taps from all over the tank, which is what makes it wide from a single sound. The paper's lengths are for 29761 Hz and scaled to the rate, and its lines share one buffer from the effect pool, as the reverb's do, long enough for the biggest plate at the pool's rate: about 0.5 MB at 48 kHz and 2 MB at 192 kHz. el-visio's port steps its wobble one sample every 2048 frames and adds the last tap of each side; here the wobble is a 1 Hz sine read between samples, the halves a quarter turn apart, and the taps' signs are the paper's. `Pre`, `Decay`, `Damp` and `High cut` are named and scaled as `Reverb`'s, so `Pre` goes to a quarter of a second, `Damp` to the same coefficient, and `High cut` is the paper's bandwidth filter on the cutoff's 20 Hz to 20 kHz scale, off at `FF`. `Decay` is the paper's decay, up to 0.98 at `FF`, which rings for about a minute and always dies away, as everything round the tank keeps less than it's given; `80` is about the paper's 0.5, near two seconds. The second all-passes follow it, as the paper has them. `Size` scales the tank's lengths and taps from half to twice the paper's, evenly in ratio with his at `80`, and `Mod` is the wobble's depth, the paper's 16 samples at `80` and twice that at `FF`. Every setting glides; `Size` and `Pre` also move no faster than their whole range in a second, so a change bends the tail's pitch by at most a quarter rather than clicking, and the input diffusers stay the same size. It starts with no pre-delay, Dattorro's decay, size and wobble, a little damping at `20`, `High cut` off, and half wet, and at those it's within half a dB of `Reverb` at its own. It rings and goes quiet as the reverb does, and costs about 0.15% of a core at 48 kHz on an M5, three times the Clouds reverb. Added on 2026-11-08.
42. **`Pitch` is Clouds' pitch shifter** (#161), ported from `clouds/dsp/fx/pitch_shifter.h` under MIT with Émilie Gillet's notice kept. Each side goes down its own short line from the effect pool, sized for the longest window at the pool's rate as the chorus's is, and two grains half a window apart read it back: each one's delay sweeps across the window at one less the ratio a frame and jumps back, fading in and out under a triangle so the other covers the jump. Clouds' straight-line reads are a Hermite cubic here, as the flanger's are. `Pitch` is a list of whole semitones from -24 to +24, written as the track's `Pitch` shows them (`+12 st`), rather than a value on the `00` to `FF` scale, since that scale has no bottom but `00`; so a step or an LFO moves it a semitone at a time, and the shifter glides between them over the smoothing time. `Fine` adds -50 to +50 cents, none at `80`, and is continuous. `Size` is the window, 10 to 100 ms evenly in ratio, starting at `CE`, about 64 ms, Clouds' own 2047 frames at 32 kHz; it's smoothed and also moves no faster than its whole range in a second, as the plate's `Size` does, so a change bends the pitch slightly rather than clicking. `Feedback` sends what comes out back into the line, up to 0.85 at `FF`, so each trip round is shifted again for shimmer-like stacks, with the flanger's soft clip on what goes in; it starts at `00`. It starts with no shift and all wet, so on its own it only delays the sound by half a window, about 32 ms. A pitch shifter before a reverb gives a shimmer, which the guide suggests rather than building one in. A tone whose cycles don't fit the window wobbles in phase each time a grain jumps, which is how this kind of shifter sounds; Signalsmith Stretch is left for studio-quality pitch later. Added on 2026-11-08.
43. **`Grains` is the granular mode of Beads** (#164), ported from Beads' `granular_player.h`, `grain.h`, `delay_grain.h`, `grain_envelope.h`, `voice_stealing_envelope.h`, the free-running half of `trigger_generator.cc`, the freeze tail of `delay_line.h`, `randomized_parameter.h` and `limiter.h`, under MIT with Émilie Gillet's notice kept. It records all the time into a stereo line of floats from the effect pool, Beads' four seconds at whatever rate it runs at: about 1.5 MB at 48 kHz and 6.2 MB at 192 kHz. Up to 32 grains, Beads' number, read it back between samples with a Hermite cubic; each new grain takes the place of the one best to steal, and the four nearest their end are faded out as Beads does, over 5 ms or 100 ms depending on how fast grains are coming, so a stolen grain never clicks. Each grain's envelope is Beads': `Shape` runs from a square through a decay at `80` and a bell at `AA` to a swell at `FF`, with an attack of at least 0.5 ms and a release of at least 10 ms, and grains fade out near either end of the line. Each grain is as loud as one over the root of how many are playing when it starts, and Beads' output limiter, a fast peak follower letting go over six seconds, holds what comes out under full scale, with a soft clip at four times that to catch what gets through first; so it stays bounded however many overlap. There are eight settings besides `Mix` and `Gain`, which means `MAX_PARAMS` went from 9 to 10 and the FX page's second row gained a fifth column. They're laid out in pairs as `Time` and `Size`, `Density` and `Spread`, `Freeze` and `Random`, then `Pitch` and `Shape`, whose short names fit the narrow fourth column, so no column had to widen. `Time` is how far back grains read, evenly from just behind the recording to four seconds. `Size` is Beads' knob: from four seconds backwards at `00`, shortening to about 8 ms at `55`, then lengthening forwards to four seconds at `FF`; it starts at `A8`, about 150 ms. `Pitch` is the pitch shifter's list of whole semitones, ±24, so a step or LFO moves it a semitone at a time. `Density` is Beads' knob with its seed held, so it runs by itself: evenly from Beads' 130.8 Hz at `00`, slowing over 16 octaves to none in the middle band (`7B` to `84`), then at random intervals above, from slow to about 65 a second on average at `FF`. It starts at `CC`, about seven a second at random. `Spread` was chosen over Beads' feedback: Beads spreads its grains across the sides by an amount that grows with how many play, and that width is most of its sound in stereo; it also works while frozen, when feedback does nothing as nothing's recorded, and slots already have delays for feedback. Here the spread is a setting of its own rather than following the randomness, as Beads' does; it starts at `80`. `Freeze` is a switch, `Off` to start; frozen, nothing is recorded and the grains go on reading what's held, and when it's turned off what came in just after freezing is faded into what's coming in over about 5 ms, as Beads does, so the recording joins up without a click. While frozen, the slot never counts as quiet, so it keeps playing with nothing coming in. `Random` stands in for Beads' four attenuverters: at each grain's start its time, size and shape stray towards anywhere in their range by that share, as Beads' do with nothing patched; pitch doesn't stray, so the grains stay in tune with the song. It starts at `00`. `Mix` starts half wet. Every continuous setting is smoothed, and `Time`, `Size`, `Pitch` and `Shape` are only read as a grain starts, so moving them can't click. Its random numbers come from a fixed seed, so a song renders the same each time. Left for later: Beads' delay mode (its `Size` past 0.99), wavetable mode, its 12-bit, 8-bit and mu-law tape qualities with their sample-rate conversion, its feedback, its reverb (the slot can go before `Reverb` or `Plate`), its monophonic mode, and a clock input to sync `Density` to the song. It costs about 0.4% of a core at 48 kHz on an M5 at its defaults and 0.8% with the densest, longest grains, and 0.4% and 2.4% at 192 kHz. Added on 2026-11-08.
44. **`Warps` is Mutable Instruments' Warps, cross-modulation and a vocoder in a slot** (#166), ported from `warps/dsp/modulator.cc` and `modulator.h`, `vocoder.cc` and `vocoder.h`, `filter_bank.cc` and `filter_bank.h`, `limiter.h`, `oscillator.cc` and `parameters.h` under MIT with Émilie Gillet's notice kept, with Warps' wavefolder table taken from its generated `resources.cc` into a 16 KB table file. The slot's own sound is the carrier, Warps' first input, and the modulator, its second, is another track chosen by `Source`, with the compressor's choices and code: a track's sound after its slots and before its level and mute, and a muted one keeps playing silently so it can still be heard. In place of the compressor's `Self`, `Source` starts on `Osc`, Warps' own oscillator, which follows the track's note and its `Pitch` (and its modulation, at the LFOs' block rate), so it's in tune with what the track plays; its own `Pitch` is the pitch shifter's list of semitones, ±24, starting at `+12 st`, so the default ring modulation keeps the note's pitch, and `Shape` picks its wave: Warps' sine, triangle and saw, a square (the triangle's own square before it's integrated), and Warps' noise, low-passed at four times the pitch. Warps' pulse, a narrow impulse train for vocoding, is left out, as here the oscillator is the modulator and not the carrier. `Pitch` and `Shape` only matter with `Osc`. `Algo` is Warps' algorithm knob and stays continuous, blending each algorithm into the next as Warps does: crossfade, fold, the diode ring modulator, the multiplying one, XOR and the comparator, each an eighth of the way from the last, then a fade through the bare modulator into the vocoder. It reads its value out of 256 rather than 255, so each cross-modulation lands on a hex value, `00`, `20`, `40`, `60`, `80` and `A0`; the bare modulator is at about `B3`, the vocoder is full from `BA`, and its release runs from fast to held, which is Warps' freeze, reaching held at `FF` rather than just past it. The status line names where it is, as `Fold`, `Digital>XOR` or `Vocoder hold`. `Timbre` is Warps' timbre knob, with what each algorithm does with it, and the vocoder's formant shift, none at `80`; it's bent towards its square as `Algo` turns, as the hardware does, since Warps' `skewed_modulation_parameter` compares a knob that reads from 0 to 1 against 1, 4 and 5. `Level` is Warps' second level knob, the modulator's drive into its saturating amplifier, starting at `A0`, a little over unity, and soft-clipping harder above; the carrier goes in as it is, with no drive of its own, so `Mix` and `Gain` finish the eight settings. It works in float at the program's rate. Warps runs at 96 kHz, its algorithms at six times that, and its filter bank's lower bands at a twelfth and a third of it, with sample-rate converters between; here the algorithms run at twice the rate through the same halfband oversampler `Distort` uses, given a second input, and every band runs at the full rate, so none of Warps' converters were needed. The bank is worked out for the rate as Warps' table was: twenty bands a third of an octave apart from 87 Hz to 7 kHz, a fourth-order low-pass at the bottom, a fourth-order high-pass at the top and third-octave band-passes between, each two state variable filters in a row; Warps' delay compensation between its decimated groups is left out, as nothing here is decimated. Warps' per-sample constants for 96 kHz, in its amplifier's gate, the limiter and the saw's smoothing, are changed to take as long at any rate, and its 60-frame blocks, over which the vocoder's band gains move, are kept as 0.625 ms. It's mono, as Warps is: the carrier and the modulator are each the middle of their two sides, and what comes out goes to both, with `Mix` bringing back the stereo dry sound. Warps' outputs are AC-coupled, so a 5 Hz high-pass takes out the offsets XOR and the comparator leave, and what comes out is held to full scale, as Warps' converter holds it. Every continuous setting is smoothed, and its filter bank, about 2.5 KB, comes from the effect pool rather than sitting in every slot, which would have quadrupled each slot's size; it starts as the fade towards the vocoder does, so its bands are full by the time it's heard. The slot keeps running while either its own sound or the track it hears has something in it, then fades out over a second, so the vocoder's release and the oscillator never stop dead. With a silent modulator, both ring modulators and the vocoder are silent; the crossfade gives the carrier at 3 dB down at `Timbre` `00`, fading to nothing at `FF`; the fold folds the carrier alone; XOR gives the carrier, at 0.7 of it at `00` up to all of it at `FF`; and the comparator gives parts of the carrier, such as its half below zero at `00`. Warps' diode ring modulator is even in the carrier, so it moves the modulator by twice the carrier's frequency and lets the modulator through, and its diodes' dead zone wants loud sounds; it's ported as it is. The multiplying one at `60`, where it starts, gives the sum and difference. Left for later: Warps' easter egg, a frequency shifter, and a level for the carrier. It costs about 0.2% of a core at 48 kHz on an M5 cross-modulating a track, 0.3% with its oscillator, and 0.5% vocoding, and 0.45% and 1.9% at 192 kHz. Added on 2026-11-08.
45. **`LPG` is Mutable Instruments' Streams, a low-pass gate and envelope filter in a slot** (#167), ported from `streams/follower.cc` and `follower.h`, `vactrol.cc` and `vactrol.h`, `svf.cc` and `svf.h`, `meta_parameters.h` and `filter_controller.h`, with the Gompertz curve and time table worked out as `streams/resources/` does, under MIT with Émilie Gillet's notice kept. What drives it is Streams' envelope follower on what it hears, with `Source` the compressor's choices and code, so `Self` follows its own sound, for an envelope filter or auto-wah, and a track follows that track, after its slots and before its level and mute, for gating a pad with drums; a muted source keeps playing silently. The follower is Streams': a crossover at 110 Hz and 1.2 kHz into three bands, each riding its rising peaks and snapping to each local peak of its energy, then smoothed, the low band slower to rise and the high band slower to fall, summed and put on Streams' square-root curve. `Mode` picks between Streams' processors. `Gate` is Streams' vactrol, excited by the follower: its first and second order responses, quick to open and slow to close, its memory, which makes it slower to light after resting and recovers over a minute, its little overshoot, and its level on the Gompertz curve with its cutoff on a square, so the cutoff falls first and the level after; it's Streams' LPG character. `Filter` is the follower on the cutoff alone and `VCA` on the level alone, as Streams' follower offers one or the other besides both. Streams moves its follower's cutoff by where the spectrum's centre sits; here `Filter` follows the level, as an envelope filter does, so that part of `follower.cc` is left out. Streams' vactrol also has a plucked mode, pinged by a trigger; a slot has no triggers, so the follower's rise stands in and the plucked mode is left out. `Amount` is how far it opens from `Base` to all the way, and `Base` is where it rests closed: the cutoff, on the track filter's 20 Hz to 20 kHz scale, or the level in `VCA`; these split Streams' one knob, whose `ComputeAmountOffset` sets both, and the vactrol keeps Streams' ringing tail, an eighth of the range left open as it closes. `Response` is Streams' shape knob, one knob as Streams has it: as a gate, the vactrol's decay grows from about 0.1 s to about 4 s at `80` as Streams' halves its coefficient out of plucked mode, then its attack slows towards half a second and both quicken again; in `Filter` and `VCA` it lengthens the follower's attack from 1 to 20 ms and decay from 10 to 200 ms, and in `Gate` the follower stays at its quickest so the vactrol shapes it. It starts at `10`. `Res` is the filter's resonance, Q from 0.5 at `00`, which doesn't peak, to 10 at `FF`, evenly in ratio; it does nothing in `VCA`. Streams works in fixed point at 31.089 kHz and drives an analogue filter and VCA; here it's float at the program's rate, with each of Streams' per-sample constants changed to take as long, and a two-pole state variable filter on each side for the analogue one. The follower's crossover is the same filter, as Streams' Chamberlin one isn't stable at every rate. The follower excites the vactrol as hard, at its loudest, as a 5 V gate excites Streams with its knob fully open. Streams reads its Gompertz curve from a table of 1024 steps, which falls in a straight line to nothing below its first step; that's kept, as the curve alone lingers at about -25 dB. `Amount`, `Base` and `Res` are smoothed, `Response` only changes how fast the follower and vactrol move, and changing `Mode` fades between the modes, so nothing clicks. Its state is under 300 bytes and sits in each slot, as the compressor's does, rather than coming from the effect pool; that takes each slot from 648 to 920 bytes. It starts as a gate on its own sound, `Amount` at `C0`, `Base` and `Res` at `00`, all wet. It costs about 0.15% of a core at 48 kHz on an M5 as a filter or VCA and 0.35% as a gate, and about 0.6% at 192 kHz. Added on 2026-11-08.
46. **`Acid` is our own TB-303-style voice, after reading how the 303 works** (#171). tracker-ce's ACD can't be read or linked, so it's built from what's published: Robin Whittle's notes on the 303 and his Devil Fish manual, Tim Stinchcombe's analysis of its diode ladder, and Robin Schmidt's Open303, whose licence was checked first and is MIT, so its measured values are used with its notice kept. What they say: the 303 has one oscillator, a saw, and its square is that saw driven hard into a transistor with an offset, so it's a little off even (Open303 models it as `tanh(70 saw + 4.37)`, high 53% of the time). The filter is a four-pole diode ladder, about 24 dB an octave rather than the 18 dB Roland claimed, with its poles spread out because the stages load each other, coupling high-passes around its loop that thin the bass as the resonance rises, and no keyboard tracking. Its cutoff knob runs from about 314 Hz to 2.4 kHz (Open303's measurement), and Env Mod can't reach zero. The filter envelope (the MEG) has an instant attack and an exponential decay of 200 ms to 2 s from the Decay knob, but always 200 ms on an accent. The level envelope (the VEG) is fixed: a few milliseconds' attack, a decay over three to four seconds, and an end about 16 ms after the gate closes; the MEG also reaches the level through 47k and 33 nF, softened. Accent makes a note louder and gives it more envelope on the cutoff, through the accent sweep circuit: the MEG, scaled by the Accent knob, charges a 1 µF capacitor through a diode and 147k, which drains through 100k into the cutoff and the level. A second gang of the Resonance knob chooses between that capacitor's slow swell and the pulse itself, and as the capacitor hasn't drained when the next accent comes, accents in a row swell higher each time, the more with resonance up. Slide is the sequencer holding the gate across two steps, with the pitch through a lag so it takes a fixed time, about 60 ms, whatever the interval.

    All of that is kept, with Vital's `DiodeFilter` (GPL 3 or later, Matt Tytel's notice kept) as the ladder, run at twice the rate through the halfband oversampler, with Vital's rational `tanh`. Vital's ladder resonates at about 0.6 of the cutoff it's given, so the voice gives it 1/0.6 of the 303's; it self-oscillates only at the very top of `Resonance`, as a 303 nearly does. Open303's measured mapping of how Env Mod and Cutoff together set the envelope's depth and the centre it swings around is used as it is, and its 44.5 Hz high-pass before the filter. The sweep circuit is modelled as its parts: a capacitor charging through the diode with a 147 ms time constant and draining with 100 ms, its level brought up by 2.47 to meet the pulse's where they balance, and `Resonance` blending from the pulse to the capacitor. It moves the cutoff up to two octaves and adds to the level up to 2.5 times the VEG's, so a full accent is about 9 dB over a plain note (Open303 adds 4 times, about 11 dB); a soft clip rounds off the loudest. Accent comes from velocity, so no new notation is needed: a note at `70` or above is accented, as notes are written at `64`, so one step up the velocity ladder, to `74`, accents one, and `7F` and none do too; Open303 takes 100 (`64`) and above, which here would accent every note as written. Slide is the `HLD` gate, which already ties a note into the next row's: on `Acid` the pitch glides there with a time constant a third of the slide's time and neither envelope starts again, and the note slid to brings its own accent. It plays one note at a time, and a new note retriggers the one already sounding rather than starting a fresh voice and fading the old one out, so the filter and the accent's capacitor carry on as on a 303; the track's note envelope starts from where it is, so this never clicks.

    It's a voice of the synth machine beside the Plaits ones, named `Acid`, with its settings stored under `acid`. It has eight settings, more than the four a voice had, so `MAX_MACHINE_SETTINGS` went to 8 and the Sound page has a third layout, with the later settings carrying on down the left under the four: `Wave` (`Saw` or `Square`, a list), `Cutoff`, `Resonance`, `Env mod`, `Decay` and `Accent` as the 303's knobs, then two of the Devil Fish's: `Slide`, 60 ms at `00` to 360 ms, and `Drive`, up to 24 dB more into the ladder, which Vital's filter answers with its own saturation. All but `Wave` are smoothed and can be locked and moved by LFOs and envelopes. It starts with the saw, every knob at `80`, no extra slide and no drive. Left out: the Devil Fish's separate accent decay, soft attack, filter tracking, filter FM and muffler, the 303's tuning knob (the track's `Pitch` does that), and the 303's own gate of half a step, as a note here holds until the next one or an `OFF`. The track's own filter still follows it, as for any voice, and is best left open. It costs about 0.3% of a core at 48 kHz on an M5. Added on 2026-11-08.

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
*   **Security and privacy:** The program reads local WAV, AIFF and FLAC files and project files. Loaders return errors on bad input rather than panic.
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
2.  **Sample slicing and waveform view.** The slicer moves up to M15, and `:bpm fit` and waveforms stay here. Chopping a break into slices, triggering them from the grid, `:bpm fit <bars>` to set the tempo from a sample's length, and waveforms drawn as images (decision 56). The M8 and Polyend are the references.
3.  **Score view.** Moved up: full multi-track editing and selections across tracks to M6, and column folding (`za zc zo zM zR`) to M15.
4.  **Song view and looping.** Moved up to M8, apart from per-track speed and grooves, which stay here. Song view and section navigation. Unequal looping with the author's decision: each section has a length, and a section either **resets** tracks or **carries** their positions on. The default is to carry, and `:reset` is the one-section form of this. Any mid-song guarantee covers track positions only. Also per-track speed, M8-style grooves with `GRV`, how groove units scale with speed, a rounding rule, and sections ending mid-step.
5.  **Generators.** Masks and value sources, Euclidean with two pulse generators and boolean operators, stage sequences, chords across tracks with strum, and transformers modelled on Ableton's MIDI tools. When one generator writes to several tracks, decide whether editing one applies them all.
6.  **Smaller items.** Chasing held values on play, a fine-timing nudge command, keyframes in sweeps, track groups, macros, Vim-style marks, `/` search, sequence increments across cursors, Harpoon-style assignable view slots, Elektron-style conditions and fill mode, swing at group level as in Maschine, MIDI, device recovery, a portable project file, and `vimtracker export-json <project> [out.json]`, which an earlier test showed round-trips between RON 0.12.3 and JSON. Comments are not kept.

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
| Normal | `W B`, `]b [b` | Next or previous track |
| Normal | `{ }` | Previous or next beat |
| Normal | `gg G` | First or last step |
| Normal | `H M L` | Cursor to the top, middle or bottom of the view |
| Normal, Insert | `M-1 M-2` | Halve or double the skip setting, like Ableton's `Cmd-1` and `Cmd-2`. `1/16` up to a bar |
| Normal, Insert | `M-s` | Scale on or off, keeping its root and kind |
| Normal, Visual | `C-a C-x` | Move a value, and a note by a semitone whatever the scale |
| Track, Mixer | `/` | Jump to a setting by part of its name, with labels as in flash.nvim |
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
| Normal | `=`, `-` | Up or down a small step: a semitone, or 1 on a value. `Ctrl-A` and `Ctrl-X` do the same |
| Normal | `+`, `_` | Up or down a large step, the same keys with Shift: an octave, or `10` in hex. On `RET`, the small step changes only the rate digit and the large step only the fade digit, with no carry |
| Normal | `x` | Clear a step's field. On the track page, instrument view and chain Transpose, back to the default |
| Normal | `K` | Choose the field from a list: Instrument and FX columns, a chain row's phrase, a sample, and an LFO's `Dest`, `Shape`, `Rate` and `Retrig` |
| Normal | `~` | Cycle a value's mark: lock, hold, glide. In Visual too |
| Normal | `.`, `u`, `Ctrl-R` | Repeat, undo, redo |
| Normal | `gd`, `Ctrl-O`, `Ctrl-I`, `grr` | Open item, jump back, jump forward, list uses. `gr` is only a prefix |
| Normal | `gc`, `gC` | Clone, deep-clone |
| Normal | `gv` | Reselect the last selection |
| Normal | `gl` | Reopen controls for the live generator under the cursor |
| Normal | `Space ga` | Apply the live generator under the cursor |
| Normal | `Space pd` | Double the current phrase |
| Normal | `Tab` | Play, or pause in place |
| Normal | `Shift-Tab` | Play from the start |
| Normal | `Space c` | Play from the cursor's step |
| Normal | `Space r` | Arm or disarm live recording, in the phrase view while playing (decision 55) |
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
| Insert | `Left Right` | Move by the grid (the skip setting), which starts at 4, without writing (decision 48) |
| Insert | `Shift-Left Shift-Right` | Move to the previous or next multiple of the skip setting, without writing |
| Insert | `;` | Auto-jump on or off: whether entering moves on by the grid (decision 48) |
| Insert (note) | `Backspace` | Clear the field. With auto-jump, move back a grid step first |
| Insert (value) | `0-9 a-f` | Two digits commit, and auto-jump moves on by the grid |
| Insert (instrument) | `0-9 a-z` | Two characters commit a number or label and move down. `Enter` commits one digit |
| Insert (FX command) | Letters | Typed in uppercase. An invalid name is refused with a message and leaves the field unchanged |
| Insert | `Esc` | Normal mode |
| Command | `Enter`, `Esc`, `Backspace`, `Up`, `Down` | Run, cancel, edit, recall history |
| Picker | Printable keys, `Backspace` | Edit the search text |
| Picker | `Ctrl-N`, `Ctrl-P`, arrows | Move the highlight |
| Picker | `Enter`, `Esc` | Open the highlighted item, close without changing anything |

Targets are `ib` and `ic`, and `ip` before G1, as in the Technical Plan. In Insert, a note moves down by the step setting, which starts at 1, and uses the last-used instrument, which starts at `00`. Key repeat is ignored on transport keys and allowed on motions and `+`/`-`. The program pushes the kitty keyboard protocol's flags from start to quitting, and won't start in a terminal without it (decision 50).

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
| `:sample path` | Assign a WAV, AIFF or FLAC file to the current instrument, referring to it where it is |
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
| `:set skip=n` | The grid: how far `Left`, `Right` and auto-jump move in Insert, shown on the status line |
| `:set scale=kind`, `:set root=note` | A scale's kind and root, either turning it on |
| `:set scale=on`, `off` | Turn the scale on or off |
| `:scale`, `:scale fit` | Choose a kind from a list, or move this phrase's notes into the scale |
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

**Audio.** Voice: linear-interpolated playback, with `C-4` at original pitch, and a 1 ms attack and 50 ms release as instrument defaults. A new note fades the old one over `DECLICK_FRAMES`. Filter: a TPT state-variable low-pass. Delay: stereo at 3/16 note time, with its buffer allocated for 20 BPM, about 2.25 seconds. Reverb: Freeverb-style. Sample import accepts 16-bit, 24-bit and 32-bit float WAV, mono or stereo, and refuses others with a message naming the file; AIFF and FLAC are read too (decision 63). Rate conversion uses `rubato`.

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
*   `src/sample.rs`: WAV, AIFF and FLAC decoding, stereo conversion and resampling.
*   `src/samples.rs`: the session sample pool.
*   `tests/*.rs`: behaviour tests.

#### `crates/vt-engine` (Create)

*   `Cargo.toml`, `src/lib.rs`: manifest and public engine, handle and render API. Internal shapes are the implementer's choice.
*   `src/link.rs`: publish, return, command, telemetry and scope queues.
*   `src/engine.rs`: the block loop, processing events on exact samples, and the sample position formula.
*   `src/playhead.rs`: per-track position, swing and version-change rules.
*   `src/fx.rs`: the FX interpreter.
*   `src/track_state.rs`: fixed-size per-track state.
*   `src/voice.rs`: sample playback and envelope. `src/preview.rs`: the preview voices.
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
*   **M9: FX inserts.** 3 inserts on a track, the first effect, the FX page, the shared parameter list for effects, and slots naming any parameter on that list. **Tests:** as [its design](#m9-design-fx-inserts) lists.
*   **M10: Tracks that process.** Returns and a master in a fixed routing, delay and reverb as effects with real settings, and a better reverb. **Tests:** as [its design](#m10-design-tracks-that-process) lists.
*   **M11: Live lengths.** A track's length changes while it plays, and the playhead stays on its row. **Tests:** as [its design](#m11-design-live-lengths) lists.
*   **M12: Lanes.** FX columns with their own loops, which drift against the notes. **Tests:** as [its design](#m12-design-lanes) lists.
*   **M13: Pulses.** A note gets a length in pulses and a gate type, so rows take their own time. **Tests:** as [its design](#m13-design-pulses) lists.
*   **M14: One list of settings.** Every setting a step can lock or a source can move registers once, with a long name and a short label for the grid, and each machine, Plaits' voices included, registers its own. **Tests:** as [its design](#m14-design-one-list-of-settings) lists.
*   **M14b: Kinds of setting.** Each setting's kind says what its value means and how it's shown. Continuous values get a fine part beside their coarse digits, the status line shows real units, and LFO settings and Pitch become lockable. **Tests:** as [its design](#m14b-design-kinds-of-setting) lists.
*   **M15: More effects and machines.** Compressor with sidechain, saturation and bitcrush, 3-band EQ and chorus, then buses, column folding, the slicer, a kick synth and Plaits-style voices, with polyphony decided when one needs it. Only what's wanted in a jam gets built.

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

**No `Sound` type or machine enum yet.** A track's sound is everything on `Track` apart from mute, solo, swing and name, and that's saved in the song. A separate type comes when a sound can be saved alone or a second machine exists, as nothing needs one before then. That's M15, or sooner if a jam asks.

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

Column folding stays in M15. This adds one thing: a Visual selection can reach other tracks with `W` and `B`, and the operators that already work on a selection then work on all of them, as one undo step.

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

### M9 design: FX inserts

Made with the author's agreement on 2026-11-06. Not built yet. It adds 3 insert slots to each track, one effect to put in them, and an FX page, and sets up what later effects reuse.

**Prior art.** The boxes nearest this workflow mostly have a fixed chain per track: the Digitakt II has bit reduction, overdrive and sample rate reduction before its filter, the M8 has `DEGRADE` and a limiter per instrument, and tracker-ce has overdrive and bit depth per instrument. Slots come from Maschine, which has as many as you like on sounds, groups and master, and the Tonverk, which has two. Slots are kept here because order matters once M14 adds a compressor, and a fixed chain would get long.

**Signal path.** `Machine -> Filter -> Utility filter -> Insert 1, 2, 3 -> Level, Pan -> Sends`, as in Direction above. In the engine, this goes in `TrackState::shape`, after `trimmed` and before the gain. `Drive` stays on the filter.

**Data.** `Track` gains `inserts: [Insert; 3]`, with a serde default of all empty, and `FORMAT_VERSION` stays as it is, so existing songs load and sound the same. `Insert` is an enum of `Empty` and `Distort`, whose settings are plain copyable values. Changes go through `Action`, so each is one undo step.

**Engine.** `TrackState` gains a preallocated, copyable state for each slot, as machines are, so the callback doesn't allocate. An empty slot is a match and nothing else, and a track with no inserts renders exactly as before.

**The first effect: `Distort`.** One effect with a `Type`, after #87.

| Setting | Values | Default |
|---|---|---|
| `Type` | `Soft`, `Hard`, `Fold`, `Sine`, `Crush`, `Down` | `Soft` |
| `Amount` | `00` to `FF`, more of what `Type` does. `Crush` has `Bits` here | `40` |
| `Mix` | `00` dry to `FF` wet | `FF` |
| `Gain` | On the level scale, after `Mix` | `E0`, which is 0 dB |

The shapes come from Vital's `distortion.cpp`, which is GPL 3 or later as we are, so its copyright notice goes in `dsp/distort.rs`. With `d` from `Amount`:

*   `Soft`: the overdrive from Plaits' `dsp/fx/overdrive.h`, MIT, which is curved so a little does little, and holds a loud sound within about 3 dB as it's turned up. A quieter sound still comes up, by about 9 dB from half level and 22 dB from a tenth at `FF`, as a clip can't hold back what doesn't reach it. `Amount` covers its top half, as below that it gets quieter.
*   `Hard`: `x * d`, clamped to -1 and 1.
*   `Fold`: a triangle wave of `x * d`, from Vital's linear fold.
*   `Sine`: a sine of `x * d`, from Vital's sine fold.
*   `Crush`: `x` rounded to steps for whole `Bits`, from `12` to `2`, in place of `Amount`, by the author's decision. Bit depth is a set of whole values, so it changes at once, with nothing in between to smooth. Nothing is lost, as a step's lock gives the same instant change, and there's no special rule: an LFO can still be assigned to it, and just steps.
*   `Down`: holds a sample for a time set by `Amount`, in time rather than samples, so it sounds the same at any device rate.

`Soft`, `Hard`, `Fold` and `Sine` run inside the `Oversampler` at 2x, which moves out of `filter.rs` to be shared. `Crush` and `Down` are meant to alias, and don't. `Mix` is a straight blend outside the oversampling. Vital's file doesn't define how it scales drive, or have a mix or level compensation, so these are ours: for the types other than `Soft`, `d` runs from 0 dB at `00` to +30 dB at `FF`, and there's no compensation to begin with. `Amount` and `Mix` are smoothed as other values are, so they can move without clicks.

**Every effect ends with a `Gain`**, by the author's decision after hearing `Distort`, so a change in level can be fixed while the knob is being turned, with no change of page. Matching levels automatically is left out: a level measured over 300 to 500 ms would make a drum loop pump, and a fixed curve depends on how loud the sound going in is.

**The parameter list.** An effect declares its settings in a static list: name, step command, modulation destination, highest value and default. The FX page, stepping, `x`, `K`, typing, step commands and `Dest` all read it, so a new effect needs no new page code. This covers effects only. The Sound and Mod pages' existing controls stay as they are, which goes back on the wording under A shared parameter list above: a file of about 1,000 lines that works isn't rewritten until there's a reason. That leaves two systems side by side for now.

**The FX page.** A third page on the track page, with one row per slot:

```text
1  Distort   Type   Soft             Mix    FF ▕█████████▏
             Amount 40 ▕██▎      ▏   Gain   E0 ▕███████▉ ▏

2  ---

3  ---
```

*   **`H` and `L`** become the previous and next page, stopping at each end, in place of naming a page each. Sound to FX is `LL`.
*   **Settings are in columns of two**, by the author's decision, filling top then bottom, so a setting's neighbours are as close as they can be: 1 over 2, then 3 over 4. Rows stay fixed, as `page_rows` returns static lists, and the cursor stops at the last setting an effect uses. An empty slot only has its effect.
*   **`j` and `k`** move as seen, and on the effect they go between slots. **`{` and `}`** go to the same setting in the previous or next slot.
*   **`K` or `i` on the effect** chooses it from a list, and `=` and `-` step through. `x` on the effect empties the slot, and `x` on a setting returns it to its default.
*   **`dd`** takes the effect out of its slot, and **`p`** puts the last one taken into the slot under the cursor, swapping with what's there, to change the order. This only works on this page. Moving an effect starts its state afresh, which may click if it's playing.
*   **`/`** finds these as it does the other pages, from their names, and the bars move with what's playing, as on the Sound page.

**Steps and modulation.** See the modulation table below. This replaces the first plan, of `DST` and `DMX` commands reaching only the first `Distort`, by the author's decision.

**Left out.** Other effects, which are M15. Send, mix and bus tracks, which are M10. Saving an effect's settings alone. Moving `Drive` onto the list.

**To judge by ear, as they may change the rules:**

*   How `Amount` feels across its range, and whether `+30 dB` is too much for `Hard` and the folds.
*   Whether a quiet sound coming up with `Amount` is a problem on `Soft` and `Hard`. `Level` is there if not.
*   What `Down` should span, which is set by ear, and whether `Crush` should stop short of `2` bits, as quiet sounds drop out.
*   Whether the click from moving an effect while playing matters.

**Issues.**

1.  **Inserts and `Distort` in the engine:** the data, the state in `TrackState`, `dsp/distort.rs` and the shared `Oversampler`. **Tests:** golden renders unchanged with empty slots, each type's response, `Mix` at each end, oversampling keeps aliasing down for the types that use it, slots run in order, a saved insert loads, and no allocation under the real-time guard.
2.  **The parameter list and FX page:** the list, the page, `H` and `L`, `x`, `K`, typing and bars. **Tests:** each key works on a setting from the list alone, the cursor stays valid across effects and machines, `/` finds the settings, and a change is one undo step.
3.  **Order:** `dd` and `p`. **Tests:** moving swaps slots and is one undo step, and the sound follows the new order.
4.  **The modulation table:** as below, with its own issues.

**Done when** the author has used it in a jam, and the questions above are answered. The CPU cost of a track with `Soft` is measured in a release build.

### M9 design: the modulation table

Made with the author's agreement on 2026-11-06. Not built yet. Things that want to be modulated register for it, and an object can be a source and a target at once, as an LFO's `Depth` can be moved by another LFO.

**Prior art.** Vital's modules register named controls, like `createMonoModControl("distortion_drive")`, and modulation connects a source to one by name. Its LFOs' frequency and phase can be modulated. The Digitakt II's LFO destinations include `LFO1: Speed`, `Depth`, `Fade` and more, one way: LFO 2 can reach LFO 1, and LFO 3 can reach both, but never backwards. tracker-ce has a fixed `parameterList`, with a table per track of who controls each one. Here, `Destination` is a fixed list, and `TrackState` wires each one by hand, though its `offsets` array is already a simple form of the table.

**The table.** Sources and targets don't know about each other. `Destination` becomes `Target`, a fixed enum giving each target a place in an array. Sources add to the array, and targets read from it:

```text
Sources                         The table                    Targets read it
Envelopes  ┐
LFOs       ├─ add to ──>  [ a value per target ] ──>  Cut, Res, Pan, Har...
                                                      A slot's settings
                                                      An LFO's Depth and Phase
```

**Registering.** An owner declares its targets in a static table: name, group, highest value and default, as effects already do in the parameter list. The `Dest` list and step commands are built from these, on the edit thread, for what's on the track, so an empty slot offers nothing. The callback only sees places in the array, with no names or lookups. A test checks that no two settings of an effect share a name.

**Targets at first.**

| Target | `Dest` | Step command |
|---|---|---|
| The existing ten | `Cut`, `Pan` and so on | `CUT`, `PAN` and so on |
| A slot's settings | `1 Amount`, `2 Mix`, `3 Gain` | Named the same |
| An LFO's settings | `L2 Depth`, `L2 Phase` | None |

A step can set any of these. A slot's setting has one command, holding the slot and its place in the effect's list, and it reads as the setting's name, as `Dest` does, so nothing has to be generated or learned, and a new effect adds no commands. FX columns are 8 characters wide for this, as small terminals aren't supported. A file holds the slot and place, like `1.1`. It takes locks, holds and glides, and `K` lists what's on the track, or `1am` types it by the slot and the start of a name. A command belongs to the slot and place, and not the effect: after changing what's in a slot, it reads as whatever's now there, and does nothing in an empty slot.

There's no special case for what moves a target. A stepped setting rounds the result to a whole step, so an LFO on `Bits` just steps, and one on `Type` jumps between types. Choosing to do that is the author's.

**Choosing a target.** The list grows with every effect, so a picker with groups can be browsed as well as searched. It takes after two things. Snacks' explorer, the author's `<leader>fe`, is one picker with two places to be: it opens on the list, where `h` and `l` close and open a folder, `/` goes to the search and back, and `i` goes to it too. Its search keeps a match's parents showing, so the tree doesn't disappear. The Digitakt II's preset browser has a list with a `FILTER` list beside it, and a separate text search.

```text
Dest
  Filter    │ > Amount
  Machine   │   Mix
  Output    │   Gain
> FX 1      │   Type
  FX 2      │
  LFO 1     │
```

| In | Keys | Does |
|---|---|---|
| The list | `j` `k` | Move, and `Enter` chooses |
| The list | `h` | Go to the groups |
| The groups | `j` `k` | Choose a group, showing what's in it |
| The groups | `l` or `Enter` | Go to the list |
| Either | `/` | Go to the search, or back |
| The search | Typing | Narrows across every group, showing each match's group, and `Enter` chooses the highlighted one |
| Any | `Esc` | Close, changing nothing |

It opens on the list, on the current choice. A group comes from what's on the track, so an empty slot isn't there, and a group with one thing in it shows that thing directly. A short list with no groups stays as pickers are now, opening on the search, so `K` then `pl` still works. Two levels are enough for targets. More levels, and tags that cut across groups as the Digitakt II's do, wait for something that needs them, like a sample browser. Step commands and the sample picker can use this later.

**What's on a track.** The lists only hold what's loaded: a machine's settings on tracks playing it, under their names on that voice, and a slot's while it holds something. A target that isn't on the track does no work, and a choice already made stays, reading like `Har ---`, so trying a machine doesn't lose its modulation.

**Keeping up.** Modulation moves values through the 1 ms smoother envelopes use, as a 10 ms one takes 16 dB off an LFO at 100 Hz. A change with nothing moving that value still takes 10 ms, so it doesn't jump. LFOs stop at 100 Hz, and true audio rate is left out: it needs working out waves on every sample and updating filters far more often, and the `FM` voice already does what it's most wanted for.

**Order.** Each update runs envelopes, then LFO 1, then LFO 2, then everything else, and each object reads what's before it. `LFO 2`'s `Dest` list has `LFO 1`'s settings, and not the reverse, so there's no feedback to detect and a project always sounds the same.

**Resolving a slot's settings.** Each setting of a filled slot has the same layers as `Cut`: a setting, a lock or glide on top, then what modulation adds, then smoothing. The engine works out a finished copy of the effect each update and hands that over, so an effect needs no code for any of this. Empty slots are skipped. `LiveValues` gains each slot's settings, so the FX page's bars move with what's playing.

**On a trig.** A later kind of target reads its value once when a step fires, and isn't smoothed, as an LFO added to a Bernoulli gate's chance is in a modular. Nothing in the table prevents it, and none is built until the first one is wanted. `CHA` comes first, with hats getting busier and sparser over some bars. The Digitakt II and Polyend set these per step, and as far as was read don't modulate them, so this goes past them.

**Memory.** The rule is that the callback doesn't allocate, not that everything is loaded at the start. Measured on 2026-11-06, the release binary is 3.9 MB, a `TrackState` is 2.2 KB, or 139 KB for all 64 tracks, and an `InsertState` is 136 bytes. A slot's state is as large as the largest effect's, so anything with a big buffer, like a reverb or delay, doesn't go in it: the edit thread allocates that when the effect is chosen, hands it over with the song's next version, and takes the old one back to free, as versions already do. Effects cost nothing until they're used.

**Left out.** `Rate` as a target: LFOs work out where they are from the song's position, so they can't drift, and a changed rate would make the wave jump. It needs them to add up their position instead, which changes how they restart and stay in sync, and gets its own decision. Moving the Sound page's controls onto the list: only their modulation moves over. Trig targets, as above.

**Issues.**

1.  **The table, and the existing targets on it:** `Target`, registering, and the `Dest` list built from it. **Tests:** golden renders unchanged, every existing destination still does what it did, no two codes are the same, and the list holds only what's on the track.
2.  **A slot's settings as targets:** the layers, step commands and moving bars. **Tests:** a lock, hold and glide on a setting, an LFO and envelope on one, each slot alone, nothing from a slot without that setting, an LFO on `Bits` landing on whole bits, renders repeatable, and no allocation in the callback.
3.  **An LFO's `Depth` and `Phase` as targets:** the order and the one-way lists. **Tests:** `LFO 1` moving `LFO 2`, the list offering no way back, and a song rendering the same each time.
4.  **A picker with groups, for `Dest`:** the two columns and `/`. **Tests:** each key in each place, opening on the current choice, a search reaching every group, one thing in a group shown directly, `Esc` changing nothing, and flat pickers behaving as they did.

**Done when** the author has used it in a jam, and the cost of a track with its slots full and moving is measured in a release build.

Measured on 2026-11-07 by timing a render in a release build on an M5, as a share of real time:

| Song | CPU |
|---|---|
| 1 track, no effects | 0.33% |
| 1 track, 3 `Soft` moving | 0.78% |
| 8 tracks, 3 `Soft` moving on each | 3.54% |
| 8 tracks, 3 `Sine` moving on each | 5.06% |
| 64 silent tracks with moving slots | 1.01% |

### M10 design: tracks that process

Made with the author's agreement on 2026-11-07. Not built yet. A send is a track, as a return track is in Ableton, and the output goes through a master track.

**Prior art.** Ableton's return tracks are empty tracks at the end, with whatever devices are put on them, and each track has a send knob per return. The Digitakt II and M8 have fixed global send effects, chorus as well as delay and reverb. The M8 has commands any step can use to change them, like `XDT` for delay time and `XRD` for reverb decay, and `XDR` sends its delay into its reverb. The Digitakt II's reverb has `PRE`, `DEC`, `FREQ`, `GAIN`, `HPF`, `LPF` and `VOL`, and its delay has `TIME` in 128ths, ping-pong and width among others. Vital's delay filters and saturates its feedback, and its reverb is a 16-line network. The Griesinger reverb in `clouds/dsp/fx/reverb.h`, MIT, is about 180 lines, and Rings, Elements and Beads use the same design.

**Tracks.** Three fixed tracks sit after the others and can't be removed: `A` and `B`, which are returns, and `M`, the master.

```text
Track 1..n -> slots -> Level, Pan ----------------> M -> slots -> Level -> limiter -> out
                          |-- Send A --> A -> slots -> Level, Pan --^
                          |-- Send B --> B -> slots -> Level, Pan --^
```

*   **A return or the master is a `Track` with no machine.** It makes no sound of its own, so it has no Machine or filter rows on its Sound page, and notes on its steps do nothing. It has everything else a track has: 3 slots, LFOs, envelopes, phrases, a mixer strip and a meter, with no new code.
*   **Its own steps reach it.** A `1 Decay 40` on `B`'s phrase sequences its reverb, and no other track's steps can, as on the Syntakt, which keeps a phrase self-contained.
*   **Sends.** The `Delay` and `Reverb` settings on a track become `Send A` and `Send B`, and the `DLY` and `REV` commands follow. A return has no sends, so there's no feedback between them.
*   **Solo and mute.** Soloing a track keeps the returns and master sounding, as in Ableton. Soloing a return plays what comes back from it, with what sends to it still playing but not heard. Muting a return silences it, and the master can't be muted or soloed. **Names.** The commands are `SNA` and `SNB`.
*   **Where they show.** In track lists, the song view and the mixer, labelled `A`, `B` and `M`. `W`, `B` and `]b` go through them. They're left out of `:track add` and of how many tracks a kit fills.

**`Delay` and `Reverb` are effects.** They're chosen for a slot like `Distort`, so any slot on any track can hold them. A new song has `Delay` in `A`'s first slot and `Reverb` in `B`'s, each with `Mix` at `FF`. The fixed global delay and reverb go.

| Effect | Settings |
|---|---|
| `Delay` | `Time`, `Feedback`, `Ping-pong`, `Width`, `Low cut`, `High cut`, `Drive`, `Mix`, `Gain` |
| `Reverb` | `Pre`, `Decay`, `Damp`, `Low cut`, `High cut`, `Mix`, `Gain` |

`MAX_PARAMS` grows to 9, and the FX page keeps its columns of two, top then bottom. The places in `Target` and the commands for slots grow with it.

**`Delay`.** After #88, and built. `Time` is a place in the list of note divisions, from `1/64` to `1 bar`, so a step or an LFO moves it in whole divisions. It reads between samples through four of them, easing to a new time with a half life of 50 ms, so it glides. What comes round goes through `Low cut`, `High cut` and a soft clip, so repeats darken and degrade, and the clip keeps `Feedback` at `FF` bounded. `Drive` pushes into the clip and brings it back down by as much, which leaves quiet repeats alone. `Ping-pong` on sums what goes in to one side, and each side feeds the other. `Width` narrows what comes out. Its buffer holds a bar at 20 BPM, about 4.6 MB at 48 kHz. Beads' wow and flutter would suit it, and is left for later.

**`Reverb`.** After #89, and built, from Clouds' Griesinger design, with its lengths scaled from 32 kHz to the device's rate. `Decay` and `Damp` set the loop's feedback and low-pass, with damping worked out so it's the same at any rate, `Pre` delays what goes in by up to 250 ms, and the cuts shape what goes in. One change from Clouds: its smear reads below 0 for about half of each wobble, out of its line and into old memory, which adds a path `Decay` doesn't control, so here the read stays in the line. It costs what the reverb before it did, about 0.05 to 0.1% of one core on an M5. At its default `Decay` of `50` a tail lasts about 1.8 seconds, against 1.1 before, at about the same level. Vital's reverb is the alternative if this one is found wanting, at more cost.

**Memory.** Both need long buffers, which can't be in a slot's state, as that's as large as the largest effect's for every slot of every track. When a version needs more than the callback has, the edit thread allocates them and sends them ahead of it, into a pool slots take from. When it needs fewer, the edit thread asks for that many back, and the callback returns them as they come free, to be freed there. Nothing is tied to a slot, so moving an effect needs nothing new, and an effect costs nothing until it's used.

**Idle cost.** A slot with silence going in, and quiet coming out for a whole delay or 0.1 seconds of reverb, does no work, and a track or return with nothing else to do skips it all. This cuts what's below -100 dB, which is the only change it makes to a render.

Measured on 2026-11-07 by timing a render in a release build on an M5, 8 tracks of Plaits `VA`, as a share of one core, before and after `Delay` and `Reverb` moved into slots:

| Song | Before | After |
|---|---|---|
| Stopped, nothing sent | 0.39% | 0.49% |
| Playing, nothing sent | 1.21% | 1.30% |
| Playing, sending to `A` and `B` | 1.21% | 1.49% |
| Playing, `Delay` and `Reverb` in slots on all 8 | | 1.77% |
| Stopped, `Delay` and `Reverb` in slots on all 8 | | 0.51% |

Sleeping doesn't save anything overall yet, as checking every slot of every track each frame costs about what it saves. It's still needed, for a slot's tail to carry on after its note ends.

**Left out.** Buses, which are M15. Sends between returns, so a delay can't feed the reverb as the M8's can, though a slot on `A` can hold a `Reverb` after its `Delay`. Pre-fader sends. Stems for the returns: a track's stem still carries what it sends, as now. Letting other tracks' steps reach a return's settings.

**Issues.**

1.  **The fixed tracks and routing:** kinds of track, `A`, `B` and `M`, sends, and the master's slots, with `A` and `B` running today's delay and reverb from the engine until issue 2. **Tests:** golden renders unchanged, the routing above, solo and mute, the fixed tracks can't be removed and aren't counted by `:track add`, steps on a return reach only that return, and a song without them loads with them.
2.  **Long buffers through the edit thread, and `Delay` and `Reverb` as effects in slots,** holding today's sound. A slot can't hold them until then. **Tests:** golden renders unchanged apart from what's below -100 dB, choosing an effect allocates only on the edit thread, old buffers come back to free, no allocation in the callback, and rates from 44.1 to 96 kHz.
3.  **The new `Delay`:** the settings and feedback path. **Tests:** time in note divisions, a change of time gliding without a click, feedback staying stable at its top, each cut, ping-pong and width.
4.  **The new `Reverb`:** the design and settings. **Tests:** decay time against the setting, stability at the top of `Decay`, pre-delay, and the cost against the old one.

**Done when** the author has used it in a jam, the cost is measured, and how each effect's settings feel by ear is settled. Golden renders change at issues 3 and 4, by design.

### M11 design: live lengths

Made with the author's agreement on 2026-11-07, after a review. Not built yet. A track's length changes while it plays, and its playhead stays on the row it's on. This comes before more effects because lanes (M12) and pulses (M13) build on it, and both change the engine's sense of where a track is.

**Prior art.** Metropolix, whose manual the author shared, keeps each track's place as it plays, so its lengths change live. Here, a track's place is worked out from the running count, the step the song has reached: `(running count - where its chain started) % chain length`, in `Engine::track_place`. The app works out the same place to draw the playhead, in `App::chain_playhead`, from `started`, which the engine reports through `JamState`.

**The problem.** A new length changes that `%`, so the playhead lands on another row. `:len`, `ge` and `:double` wait until playback is paused for this reason. Undo, redo and edits to a chain's rows change a chain's length too, aren't held back, and jump today.

**The rule.** When a new version arrives while playing, `Engine::swap` keeps each track on the same row by moving `started`. Everything else still uses the formula.

*   **The same row.** The track's slot in its chain and its row in that phrase's loop come from the old version, with `chain_position`. If that slot and row still exist in the new version, `started` moves so the formula gives that place again. Shortening an earlier phrase, or adding a row to the chain before the playhead, keeps the place.
*   **No row to stay on.** If the row is gone, the track goes to its chain's first row on its next step, as the existing `after_steps` rule does for a count past a reset.
*   **The screen follows.** `started` already reaches the app through `JamState`, and `App::chain_playhead` draws from it, so nothing new is reported. For one frame, the app can draw the new song with the old `started`, which is a known gap.
*   **Lengths change at once.** `can_change_length` goes, with its status message, and `:len`, `ge` and `:double` work while playing. `gs` already does, as moving a loop's start keeps its length.
*   **Sections.** A section with no fixed length is as long as its longest chain, so a live change can change it. The section's place keeps counting. If its length is now shorter than its place, the next section comes in on the next step, and a `Launch::Every` grid follows the new length.
*   **Unchanged:** playing from a step, launches, realigning, swing and renders, which all still use the formula.

**Alternatives considered.** Each track could keep a counter that moves on each step and wraps at the length. It needs new state in the callback, a list of the nine places that would reset it, a way to report it every step, and care with swing, and playing from a step would no longer match playing to it. Moving `started` when a version arrives needs none of this.

**Issues.**

1.  **Keeping the place across versions:** the rule in `swap`, and the section rule.
    **Tests:**
    *   Golden renders unchanged.
    *   A chain of 16 at place 12, shortened to 8, goes to place 0 on its next step.
    *   A chain of 16 at place 5, lengthened to 32, goes to place 6.
    *   A two-phrase chain playing its second phrase keeps its row when the first phrase is shortened.
    *   An undo, and an edit to a chain's rows, while playing keep the row.
    *   An `Every` launch during a `:len` on a section with no fixed length lands on the new grid.
    *   No allocation in the callback, with the `forbidden` guard tests in `link.rs`.
2.  **Length edits while playing:** remove `can_change_length` from `:double`, `ge` and `:len`, its status message and comment, the two app tests that check the wait, and the guide's line about it.
    **Tests:** each of `:len`, `ge` and `:double` while playing leaves the next row played as the old row plus 1, or the chain's first row if that's past the new length.

**Done when** `:len`, `ge`, `:double` and undo work while playing without the playhead moving off its row, and the author has used them in a jam.

### M12 design: lanes

Made with the author's agreement on 2026-11-07, after a review. Not built yet. Each FX column can have its own loop, so it drifts against the notes, and a phrase stops repeating exactly at its length.

**Prior art.** In Metropolix, each of 8 mod lanes has its own length, order and clock division, and each value steps or ramps into the next. Its lanes drift against its tracks, which is the point.

| Metropolix | Here |
|---|---|
| A mod lane's value that steps or ramps | An FX column value with a lock, hold or glide mark |
| `RATCH` and `PROB` in a lane | `RET` and `CHA` in an FX column |
| A track's own length | A track's own chain length |

**Lanes.**

*   **The note column's loop is the phrase's loop.** Velocity shares it. Nothing new is stored for it.
*   **An FX column can have its own loop,** a start and a length in the phrase's stored rows, inside them. With no loop of its own, a column follows the phrase's loop as now. Songs saved without these play as before.
*   **Where a lane reads.** A column with its own loop reads row `start + ((place + offset) % length)`. Here `place` is the track's count since its chain started, `running count - started`, before the chain's `%`. So a lane carries on counting across its phrase's repeats and into the next phrase, and drifts until something resets `started`, just as tracks of unequal lengths do. In the next phrase, it reads that phrase's column loop at the same place.
*   **Resets** are whatever sets `started` today: a section bringing in a new chain, a launch, realigning, a section that resets, and playing from a step. So playing from a step matches playing to it.
*   **Live changes.** `offset` is a small number per column, kept by the engine. When a column's loop changes while playing, `swap` moves it so the column stays on its row, as M11 does for tracks. It's reported to the app with `started`.
*   **What runs.** Each step, each FX column runs the row it reads. `RET` and `CHA` in a drifting column are Metropolix's `RATCH` and `PROB` lanes. They act on the step they land on, and do nothing on a step with no note.
*   **Glides** go to the next value in their own column, searching that column's loop and wrapping within it. Their time is that many steps.
*   **Editing commands act on the phrase's rows as now,** and lanes only change what plays. So `ip`, `ic`, paste, rotate, `.` and the shape panel keep working on rows, whichever loop a column plays.
*   **On screen.** Each column dims the rows outside its own loop, and has its own playhead.

**An example.** A phrase loop of 16, with FX 1's loop rows 0 to 2. FX 1 reads rows 0, 1, 2, 0 and so on. The phrase's second pass starts at place 16, and FX 1 reads row `16 % 3 = 1` there. If instead the chain goes on to a phrase of 8 rows with its own FX 1 loop of 3, that phrase starts at place 16, and FX 1 reads its row `16 % 3 = 1`.

**Built** as designed. A lane shows its length in the title, like `FX1:3`, dims the rows outside it, and has its own `>`.

**Making a lane, revised with the author after using it.** At first `gs`, `ge` and `:len` on an FX column made it a lane. That turned out to catch a habit: reaching for `ge` to shorten the phrase while on an FX column quietly gave that column a lane, and it stopped following the notes. So a lane is now asked for by name. `:lane 3` makes the cursor's FX column a lane of 3 steps from the cursor's step, and `:lane` alone puts it back to following the notes. On a column that's already a lane, `gs`, `ge` and `:len` set the lane. On one that isn't, they set the phrase's loop, as on the notes. Sliding the loop moves lanes with it, as a lane is set where the loop is. Doubling the loop leaves a lane's length alone.

**To decide with the author** before issue 1: the keys that set a column's loop, likely `gs`, `ge` and `:len` on an FX column, and how the playheads and dimmed rows look.

**Alternatives considered.**

*   **Resetting each column on every repeat of its phrase:** columns would line up again each pass and never drift. Rejected with the author.
*   **A counter for each column:** it has the same costs as in M11's alternatives.
*   **Separate screens for lanes,** as Metropolix has: lanes kept in the grid keep the grammar, and are seen beside their notes.

**Risks.**

*   **A jam after live edits isn't reproducible.** Rendering or playing from a step works places out again from the formula, not from the offsets live edits left, and may not match what was heard. Rendering what's playing is the way to keep a jam.
*   **Reading the grid.** The playhead per column is tried with the author before the milestone is done.

**Issues.**

1.  **Columns with their own loops:** the loop in the phrase, saved with a serde default, where a lane reads, offsets kept across versions, glides in a lane, and the keys.
    **Tests:**
    *   Golden renders unchanged.
    *   The example above.
    *   A lane's place after playing from a step matches playing to it.
    *   A lane changed while playing keeps its row.
    *   A glide wraps within its lane.
    *   No allocation in the callback.
2.  **Lanes on screen:** dimmed rows and a playhead for each column.
    **Tests:** each column's playhead matches the row the engine plays.

**Done when** the author has used lanes in a jam, and agrees the grid stays readable with them.

### M13 design: pulses

Made with the author's agreement on 2026-11-07, after a review, and revised with the author the same day. Not built yet. A gate column beside the note gives each row a gate type and a length in pulses, so rows take their own time. It's part of the note column, as a note is often just a trigger and the gate is what that trigger does, so it can never have a lane of its own.

**Prior art.** Each Metropolix stage has a `PULSE COUNT` of 1 to 8 clock pulses, and a `GATE TYPE`: `HOLD`, one gate for all its pulses, tied into the next stage; `MULTIPLE`, a gate on each pulse; `SINGLE`, one gate on the first; and `REST`, none. `SKIP` treats a stage as if it isn't there.

**Pulses.**

*   **A pulse is one step,** 240 ticks. From here, a phrase's positions are rows, and steps are only time.
*   **The gate column.** It sits between velocity and the first FX column, and works like an FX column: a cell holds a gate command, and its argument is the row's length in pulses, `0` to `8`. The commands are `SGL`, `MLT`, `HLD` and `RST`, the gate types below, so `HLD3` holds a note for 3 pulses. An empty cell plays as `SGL1`, as rows do now, so a song saved without the column plays as before.
*   **Every gate command's argument is its length.** That's the rule that keeps the column about time, and new gate commands follow it. Anything else a gate type needs, like a slide's time, goes in an FX column.
*   **A cell on a row without a note** still sets the row's length, so `RST4` on an empty row is 4 steps of nothing. Without a note, the gate type does nothing, only the length.
*   **`0` skips the row,** whatever the gate command. It takes no time and fires nothing, FX included, as Metropolix's `SKIP` does. A phrase whose loop sums to 0 pulses is passed over in its chain, and a chain summing to 0 is empty, as a missing chain is now. Looking for the next row stops after one pass, so it can't hang.
*   **Lengths in rows, time in steps.** `:len`, `gs`, `ge` and `:double` count rows. A chain lasts the sum of its rows' pulses, and a section's length, `Launch::Every`, the `:reset` period, render lengths and swing count steps. Swing goes by the step, as now, so it doesn't depend on rows.
*   **Finding the row.** `chain_position` walks the rows of the chain's phrases, adding up their pulses, and the playhead and audition use it too. Running totals worked out in `Published::new`, with a search, are the fallback if the walk's cost in the callback turns out to matter, which issue 2 measures.

**Gate types.**

| `Gate` | Note-ons | The note ends | `RET` and `CHA` |
|---|---|---|---|
| `Single` | On the first pulse | When something ends it, as now | `RET` repeats within the first pulse. `CHA` is rolled once |
| `Multi` | On each pulse, at the same velocity | `KIL`, `OFF` and `DEL` count from each pulse's start | `RET` repeats within each pulse. `CHA` is rolled once, for the row |
| `Hold` | On the first pulse | It's tied into the next row's note, which changes pitch with no new attack, then carries on as that row's note | `RET` repeats within the first pulse. `CHA` is rolled once |
| `Rest` | None | It ends a note that's sounding, as `OFF` would | None |

*   **A `Hold` with no note to tie into** carries on as a `Single`. That covers an empty next row, a `Rest`, an `OFF`, a skipped row, a failed `CHA`, a launch or a jump.
*   **The tie needs each machine to change pitch without restarting.** A sample carries on playing at the new rate, and a synth changes pitch without restarting its envelopes. A machine that can't retriggers.
*   **FX columns.** A column with no lane of its own follows the note rows, and its row's FX fire on the row's first pulse. A lane runs on steps, as in M12, so it drifts against long rows.
*   **Returns and the master** have no notes, so pulses don't apply to them.

**On screen, keys and files.**

*   **The cell** shows as `HLD3`, or `----` when empty, between velocity and FX 1, so the cell itself shows each row's length.
*   **Keys** are an FX cell's: `K` chooses the gate command, `=` and `-` change the pulses, typing `HLD3` enters it, and `x` empties it. It's a column group for selections, paste and the shape panel.
*   **Beat rows.** A row is lit as a beat when it starts on a beat in time. Row numbers still count rows, so `5G` is still the fifth row.
*   **The step text** gains the cell after velocity when a row has one: `C-4 64 HLD3 CUT40 ...`. A row without one is written as before, so saved songs and every step written as text in the tests stay as they are.
*   **Only sound tracks** have a gate column. Returns and the master have no notes.

**Alternatives considered.**

*   **A stage generator writing ordinary rows,** as the Non-Goals once said: rejected with the author, as phasing between lanes and long notes is the point, and that comes from how the grid plays, not from what's written in it.
*   **Pulses as an FX command:** a command in a drifting lane would land on different rows each pass, so rhythm itself would wander.
*   **A pulse digit and gate letter on the note,** as first designed: it gives the vocabulary nowhere to grow. The gate column has room for new gate commands.
*   **`RET`, `CHA`, `DEL`, `KIL` and `OFF` in the gate column:** they stay in FX columns, as drifting ratchet and chance lanes are Metropolix's own, and only work from a column that can drift. The gate column says how long a note's gate is and how it behaves over its pulses. FX columns say what happens during it.
*   **A clock division for each track,** as Metropolix's `DIV`: it stretches a whole track evenly, not each row, and is left for later.

**Risks.**

*   **Reading the grid.** A row of notes and the same row of a lane aren't the same moment once rows have pulses. The gate cell shows each row's length, and the beat rows show time. This is tried with the author before more is built.
*   **The step text changes,** so every test that writes a step as text changes with it.
*   **The tie is new to every machine.** Each one is tested.

**Issues.**

1.  **Pulses and gate types:** the gate cell on a step, the step text, running totals in `Published`, the search, the gate types and the tie.
    **Tests:**
    *   Golden renders unchanged.
    *   A row lasts its pulses, and a chain the sum of its rows'.
    *   Each gate type plays as its table row says.
    *   A `Hold` with nothing to tie into carries on as a `Single`.
    *   A tie on each machine changes pitch with no new attack.
    *   A skipped row takes no time and fires nothing.
    *   A loop of all `0` rows is passed over and doesn't hang.
    *   A lane drifts against long rows, and a default column follows them.
    *   Pulses changed while playing keep the row, as in M11.
    *   No allocation in the callback.
2.  **The gate column on screen and in the grammar:** the cell, its keys, its column group, beat rows by time, and audition.
    **Tests:** entering, changing and emptying a cell, `K` choosing a command, selecting and pasting the column, beat rows with long rows, and auditioning a row with pulses.

**Done when** the author has used pulses in a jam, the grid stays readable to them, and the cost of the search in the callback is measured in a release build.

**Built** in two parts, with the keys as designed. Measured on 2026-11-07 by timing a render in a release build on an M5, 8 tracks of Plaits `VA` with a note on every row, as a share of one core:

| Phrases | No gates | `MLT2` on every row |
|---|---|---|
| 16 rows | 1.73% | 1.71% |
| 4096 rows, the most a phrase stores | 4.45% | 4.45% |

The walk costs nothing to see at ordinary lengths, and about 2.7% of a core across 8 tracks of the longest phrases, as each step adds up every row's pulses. Running totals worked out in `Published::new` would remove that, if long phrases come up in use.

### M14 design: one list of settings

Made with the author's agreement on 2026-11-07, and revised the same day after a review and the author's decisions on it. Not built yet. Every setting that a step can lock or a source can move is registered once, with a long name for finding it and a short label for the grid. Step commands that only set a parameter stop being a list of their own, and each machine, Plaits' voices included, registers its own settings under their real names, which a track remembers for each voice. This carries out the shared parameter list planned in "A shared parameter list" above, which decision 7 held back for the first effect and M9 built for effects only.

**The problem.** The FX column holds two kinds of thing:

*   **Events:** `DEL`, `KIL`, `OFF`, `RET` and `CHA`. They do something to a note, and aren't parameters.
*   **Locks:** `VOL`, `PAN`, `CUT`, `RES`, `SNA`, `SNB`, `HAR`, `TIM`, `MOR` and `ACC`, and the slots' settings. These only set a parameter.

Slot settings already work for any setting: a lock names its target, and shows the setting's name. The named commands are a second, hand-made way of doing the same for a few parameters, and each new one needs an `FxCommand` variant, a name, parsing and a list entry, as `ACC` did (decision 28). The same settings are also listed a third time, in `Target`, for modulation.

Plaits has the same problem in its settings. Its voices share the hardware's three knobs, so a track has one `harmonics`, `timbre` and `morph`, and each voice gives them a meaning. Giving each voice its real ranges and defaults needs special cases today: `harmonics_top` caps 6-op's patch at `5F`, `centred` says which bars draw from the middle, `uses_accent` hides Accent, and 6-op's Bright starts at the shared `00`, which is dark, rather than its own `80`.

**Prior art.**

*   **Elektron:** any parameter on a page can be locked on a trig, with no list of commands.
*   **Ableton and Bitwig:** any parameter can be automated, chosen by its name.
*   **Renoise:** keeps tracker effect commands, and locks a device's parameters by device and parameter number beside them, so it has two systems.
*   **The M8 and Polyend Tracker:** keep three-letter commands for a small screen with no keyboard.
*   **Vital:** registers named controls, which modulation connects to by name, as M9's modulation table does here.

**The registry.** One static table in `vt-model` lists every setting there is. It grows out of `Target`, `TargetInfo` and the effects' `Param` lists, which it replaces, with `VALUE_COMMANDS` in the engine.

| Owner | Its settings |
|---|---|
| The track | Volume, pan, cutoff, resonance, sends, and pitch for modulation only |
| Each machine | Its own, as many as it has, up to `MAX_MACHINE_SETTINGS`, which is 4 |
| Each effect, in each of the 3 slots | Its own, as effects declare them now, by their `Setting` |
| Each LFO | Its depth and phase, for modulation only |

Each entry has:

*   **An id.** In memory it's a `SettingId`, a number indexing the table, so it's small and copied freely, and the table is fixed when the program is built. In files it's a name that never changes: `cut`, `string.decay`, `2.delay.feedback`, `lfo1.depth`. An effect's setting id names the slot, the effect and the setting, so it belongs to that effect.
*   **A long name and a group,** for `K`, `Dest`, the track page and the status line. The status line shows them as group, then name: `Filter: Cutoff`, `String: Decay`, `FX 2 Delay: Feedback`.
*   **A label** of 3 or 4 letters, with the slot's digit before it for an effect's setting, for the grid. Settings without a label, Pitch and the LFOs' for now, can be moved by modulation but not locked from a step.
*   **Its kind,** as today: a level from `00` to a top, which is `FF` or `MAX_LEVEL` for volume, sends and gain; a level centred on `80`, like pan; or a list of choices, which wraps or stops. Kinds that `00` to `FF` doesn't fit are issue #139.
*   **Its default.**

Every setting is a place in fixed arrays the size of the table, so the callback never looks anything up: a value's place is its `SettingId`. A track keeps a value for every setting, and a lock for a setting the track isn't playing, another voice's or an effect that's no longer in its slot, sets a place that nothing reads.

**Events stay apart.** They're a short fixed list, not settings, kept in their own small table. Their labels are checked against settings' labels, so no event and setting share one.

**Labels.**

| Group | Labels |
|---|---|
| Events | `DEL` `KIL` `OFF` `RET` `CHA` |
| The track | `VOL` `PAN` `CUT` `RES` `SNA` `SNB` |
| Distort, here in slot 1 | `1TYPE` `1AMNT` `1MIX` `1GAIN` |
| Delay, here in slot 2 | `2TIME` `2FDBK` `2PING` `2WDTH` `2LCUT` `2HCUT` `2DRIV` `2MIX` `2GAIN` |
| Reverb, here in slot 3 | `3PRE` `3DCAY` `3DAMP` `3LCUT` `3HCUT` `3MIX` `3GAIN` |

*   **No two labels on a track can match.** A track can show at once the events, its own settings, its machine's settings and its slots'. A test checks every machine's labels against the track's and the events', and every effect's against each other. A slot's digit sets its effect's labels apart from the rest.
*   **Distort's Amount is one setting.** On Crush it's shown as Bits, from 2 to 12, as it is now, with the same id and label.
*   **The grid gets narrower.** A command's name takes 9 columns now, for names like `1 Amount`. A label takes 5 and a gap, so a cell reads `CUT   40 .` or `2FDBK 40 .`, which saves 9 columns a track across a step's three FX cells.

**Machines and Plaits' voices.** Each Plaits voice is a machine with its own settings. `Machine` becomes `Sample` or `Plaits(SynthVoice)`, so `K` on Machine lists Sample, then each voice, searched by typing its name, and the Voice row goes. Sample registers no settings. Each voice's code takes the same values as now, given as an array in the order the voice registers them.

| Voice | Settings and labels |
|---|---|
| VA | Detune `DTUN`, Square `SQR`, Saw `SAW` |
| FM | Ratio `RAT`, Index `INDX`, Feedback `FDBK` |
| Fold | Shape `SHPE`, Fold `FOLD`, Slope `SLPE` |
| Waves | Bank `BANK`, Wave `WAVE`, Row `ROW` |
| 6-op | Patch `PAT`, Bright `BRT`, Time `TIME`, Accent `ACC` |
| String, Modal | Material `MATL`, Bright `BRT`, Decay `DCAY`, Accent `ACC` |
| Dust | Spread `SPRD`, Density `DENS`, Filter `FILT` |
| Noise | Type `TYPE`, Clock `CLCK`, Ring `RING` |
| Kick | Punch `PNCH`, Tone `TONE`, Decay `DCAY`, Accent `ACC` |
| Snare | Snappy `SNAP`, Tone `TONE`, Decay `DCAY`, Accent `ACC` |
| Hat | Noise `NOIS`, Tone `TONE`, Decay `DCAY`, Accent `ACC` |

*   **Names.** VA's settings are named for what they move, where Harm, Timbre and Morph name Plaits' knobs. Noise's third is Ring, because Res would match the filter's `RES`.
*   **Kinds and defaults** carry over from today, with the first and third settings centred where `centred()` says, the first and third starting at `80`, the second at `00`, and Accent at `CC`, apart from these: 6-op's Patch is a list of its 96 patches, starting at `20`, `E.PIANO 1`; its Bright starts at `80`, which is the patch as made; and String's and Modal's Material start at `40`, a string or bar in tune.
*   **Patch names** move into `vt-model` as a table generated from Plaits' patch banks, as the sine table was, so the registry can list them. Every value reaches the voice as a fraction from 0 to 1, as now, and 6-op turns its Patch back into a patch's number.
*   **Each voice keeps its own settings.** A track keeps every voice's values, so switching from String to Kick and back finds String as it was. A voice the track hasn't played starts from its defaults. In the file, a track lists the settings it has changed from their defaults, by id, such as `string.decay: 0x90`, and anything not listed is at its default.

**Steps.** An FX cell holds an event, or a lock of a setting by its `SettingId`, with a value and its mark: `.` for a lock, `h` for a hold or `g` for a glide, as M4 defines them.

*   **On screen:** its label and value, as above. A lock that doesn't apply on this track, for another voice or an effect no longer in its slot, keeps its label and value but is drawn faint, so it can't be mistaken for an empty cell and typed over. The status line names it and says why, such as `String: Material, not on this track`. It comes back to life when its voice or effect does, and `x` and `d` clear it as any other cell.
*   **Typing a command:** type a label and press `Enter`, as in `K`, so `cut` or `2fdbk`. Case doesn't matter. If what's typed isn't a label on this track, `Enter` opens `K` with it already typed in, so a name half remembered still gets there. A completer that shows the match as you type can come later, if this is awkward in play.
*   **`K`** lists every setting the track has, grouped as `Dest` is, with `Picker::grouped`, and shows each label beside its long name, so labels are learnt by using it.
*   **In files:** a cell is its id, a colon, the value in hex and the mark: `cut:40`, `string.decay:80g`, `del:03`. The colon is there because an id can end in letters that read as hex. An empty cell is `---`. A song naming an id the program doesn't know doesn't load, and says which.
*   **`Dest`** stores the same ids, so an LFO or envelope aimed at String's Decay on a Kick track does nothing, is drawn faint in `Dest`, and comes back with String.

**Values in the engine.** `TrackState` keeps each value's base, hold, lock and glide by `SettingId`, as it keeps them by `FxCommand` now, in arrays the size of the table. Modulation's offsets are the same size, so `Target::COUNT` becomes the table's length. The voice that's playing gets its machine's values as an array of `MAX_MACHINE_SETTINGS`, in place of `Settings` and its named fields, so each voice's `next` changes its argument but not its sound.

**Risks.**

*   **Songs made before this don't load,** which the author has agreed to. The golden songs are step text in the tests, and are rewritten in the new form.
*   **A lock can go quiet** when its voice or effect changes. It's drawn faint and named in the status line, and returns with its voice or effect.
*   **Labels have to be learnt.** `K` shows each beside its name, and `Enter` on a wrong one opens `K`.
*   **Typing a command takes one more key,** `Enter`. If that's awkward in play, a completer can show the match as it's typed.
*   **Labels could run short** as machines grow. Each track only shows its own machine's, so a new machine can reuse a word another machine uses, and the clash test catches a clash with the track or the events.

**Alternatives considered.**

*   **Keep the named commands** and add one for each new parameter, as `ACC` did. Every new setting needs special cases, and slots already show the general way.
*   **Long names in the grid.** They need no learning, but a command takes 9 columns, so three FX cells with notes don't fit a track in the score view.
*   **Labels of exactly 3 letters,** entered without `Enter` as now. Typing is a key quicker, but 4 letters read better and leave more room for names.
*   **Typing by the start of a long name,** as slots' settings work now after their digit. Without the digit, `fee`, `dec` and `de` match several settings, and there's no point at which a typed name is finished. `K` searches long names instead.
*   **Plaits' three knobs, renamed by each voice.** A lock would carry to the next voice but its meaning would change under it, and ranges and defaults would still need special cases. With a keyboard and screen there's no need to share three physical knobs.
*   **Switching voice loads the new voice's defaults,** keeping nothing. It's simpler, but losing a voice's settings to try another is worse in play.
*   **A slot lock following its place in the effect's list,** as now, so a Delay's Feedback lock becomes a Reverb's Damp when the effect changes. The author found it janky, and it's unlike voices.
*   **Deleting an effect's locks** when the effect changes. Cleaner, but trying another effect would wipe them.

**Left out.**

*   **Locking LFO settings and Pitch from a step,** which issue #139 decides along with their kinds.
*   **Making the track's Drive, Low cut and High cut lockable,** a label each once there's a reason.
*   **A list of settings saved as a preset.**
*   **Settings with the same meaning sharing an id across voices,** so String's Decay lock plays on a Kick, until one phrase on different voices needs it.

**Issues.** Each leaves everything building and every test passing.

1.  **The registry and labels:** the table, `SettingId`, the events' table, every owner's entries with labels, kinds and defaults, machines included, and the clash test. The grid shows labels and gets narrower. Steps still store `FxCommand`, `HAR`, `TIM`, `MOR` and `ACC` show the loaded voice's labels for its settings 1 to 4, and files don't change.
    **Code:** `vt-model`'s new `settings.rs` with `target.rs` and `insert.rs`'s `Param` lists folded in; `views/phrase.rs`'s `COMMAND_WIDTH` and `step_text`.
    **Tests:**
    *   No two labels match on a track: each machine's against the track's and the events', and each effect's against each other.
    *   Every label is 3 or 4 letters after an optional digit, every entry has a long name and a group, and every id is unique.
    *   The grid shows `CUT   40` and `2FDBK 40`, and a command takes 6 columns with its gap.
    *   The existing tests of `Target` and of effects' setting names pass against the table.
2.  **Machines register their settings:** `Machine::Plaits(SynthVoice)`, each voice's settings and defaults, a track keeping every voice's values, the generated patch names, the Sound page's rows from the registry, and a voice's array of values in place of `Settings`. `HAR`, `TIM`, `MOR` and `ACC` set settings 1 to 4 of the loaded voice until issue 3. Tracks' files change.
    **Code:** `song.rs`'s `Track`, `Machine` and `SynthVoice`, which lose `harmonics`, `timbre`, `morph`, `accent`, `harmonics_top`, `centred`, `uses_accent` and `settings`; `action.rs`'s `TrackControl` and the two machine actions, which merge; `track_state.rs`'s `synth`, `LiveValues` and `synth_settings`; `voice.rs` and every voice in `dsp`, which take the array; `six_op.rs`'s `patch_index`; the track page's rows, the Voice and patch pickers, and `views/track.rs`.
    **Tests:**
    *   Every voice renders the same samples as before for the same values, in `tests/synth.rs`.
    *   Switching from String to Kick and back keeps String's values, and a voice not yet played starts from its defaults.
    *   6-op's Patch lists its 96 patches and starts at `E.PIANO 1`, and its Bright starts at `80`.
    *   A track saves and loads its changed settings by id.
    *   No allocation in the callback, with the guard in `rt_guard.rs`.
3.  **Steps and `Dest` name settings:** an FX cell holds an event or a `SettingId`, the file form, typing with `Enter`, `K` grouped, the status line, faint locks that don't apply, and `Dest` storing ids. `FxCommand`'s value commands and `Target` go, and `TrackState` keeps values by `SettingId`.
    **Code:** `step.rs`, `steptext.rs`'s parser, `target.rs`, `lfo.rs` and `envelope.rs`'s destinations, `track_state.rs`'s values, `engine.rs`'s step and track-page paths, `song.rs`'s `next_value`, `shape.rs`, `selection.rs`, `action.rs`'s `snapped`, `picking.rs`, typing in `app.rs`, and `views/status.rs`.
    **Tests:**
    *   Golden renders unchanged, with their songs' steps written in the new form.
    *   A step with each kind of cell saves and loads, and an unknown id fails to load with its name.
    *   `cut` then `Enter` locks Cutoff, `2fdbk` then `Enter` locks slot 2's Delay feedback, and `xyz` then `Enter` opens `K` showing `xyz`.
    *   A String lock on a Kick track, and a slot 2 Delay lock with a Reverb in slot 2, do nothing, are drawn faint, and play again when String or the Delay comes back. The same for an LFO's and an envelope's `Dest`.
    *   No allocation in the callback.

**Issue 3 is built in two parts.** The first, on 2026-11-07, is the model and the engine: an FX cell holds an event or `Set(SettingId)`, songs save `cut:40` and `---`, `Dest` stores ids, `Target` and the named value commands go, and `TrackState` keeps a value for every setting, heard only while its voice or effect is loaded. The second part, the same day, is the interface: a label of up to 5 characters is typed and confirmed with `Enter`, and anything else opens `K` searching for it; `K` is grouped as `Dest` is and opens searching; the status line names a lock by its group and name, saying when the track doesn't have it; and such a lock is drawn faint. The track's own settings keep the page's short names, `Cut`, `Res` and `Vol`, and the Mod page's `Dest` is 13 wide to fit a slot's setting or one that's gone.

**Issue 2 built** on 2026-11-07. The voices keep `Settings`, with its four fields, as their view of the array they're given, so their code and tests are unchanged. A voice with three settings has a last row of only Swing on the Sound page, and output settings stay in the right column.

**Done when** each issue's tests pass, and the author has used locks of track, effect and voice settings in a jam, typing and `K` feel quick to them, and the grid stays readable to them.

### M14b design: kinds of setting

Made with the author's agreement on 2026-11-07, from issue #139. Not built yet. Each setting's kind says what its value means, how fine it goes, and how it's shown and changed. A value is stored as its kind's own number. The grid stays in hex, and the status line shows real units. Continuous settings get a fine part beside their two coarse digits.

**The problem.** Every value is two hex digits, `00` to `FF`, and many settings aren't really that. The code works round it in several places:

| Setting | Its real range | How it's handled now |
|---|---|---|
| LFO depth | −255 to +255 | Not lockable; typed in its own units on the Mod page |
| LFO phase | 0 to 359 degrees | Not lockable; typed in degrees |
| Pitch | Semitones either way | Not lockable; only LFOs and envelopes move it |
| Pan and centred voice settings | Either side of a middle | `80` is the middle |
| Volume, sends, gain | The level scale, `E0` at 0 dB | Capped at `MAX_LEVEL` |
| Patch, Distort's Type, Delay's Time, Ping | A list of choices | A place in the list, which wraps or stops |
| Crush's bits | 2 to 12 | Distort's Amount, turned into whole bits, sharing its place |

There's also too little resolution in places. 256 steps across a filter's range is 47 cents a step, which is too coarse for tuning or for dialling in a resonant filter.

**Prior art.**

*   **tracker-ce:** each parameter has a minimum and maximum in real units, a curve (linear, exponential, steep, power), an `isEnum` flag and a formatter for its units (`Hz`, `st`, `%`, `s`). Values are stored in real units.
*   **VST3:** every parameter is a normalized 0 to 1, with a step count (0 for continuous) and functions to and from real units. Hosts and controllers move a stepped parameter by whole steps.
*   **CLAP:** stores real values, with an `IS_STEPPED` flag. A stepped parameter stored as a normalized value moves when choices are added, which is why CLAP doesn't do that.
*   **MIDI:** a 14-bit control is two CCs, a coarse byte and a fine one, which is coarse and fine here.
*   **The M8:** two hex digits everywhere.
*   **Elektron:** real units on screen, and fine values by pressing and turning.
*   **Renoise:** colours pattern text by what a command means, with volume, panning, pitch, delay and global commands each in their own colour.

**Kinds.** A kind has one of two shapes.

```
Kind::Continuous {
    curve,    // Linear, Exponential or Level: how a value maps onto real units
    middle,   // centred: 8000 is the middle, and each side has its own scale
    wraps,    // goes round past either end, like phase
    top,      // the highest coarse value: FF, or MAX_LEVEL for levels
    units,    // how the status line shows it: Hz, dB, %, st, °, or hex steps
}
Kind::Stepped {
    top,      // the highest value, from 00
    middle,   // centred: 80 is the middle, as for semitones
    wraps,    // goes round past the end
    shown,    // Choices(names), or a number in units such as bits
}
```

*   **A continuous value** is a `u16` from `0000` to `top` followed by `00`, so `FF00` for most settings. The high byte is the coarse value and the low byte is the fine one, so `4080` is half a coarse step above `40`. `FF` means full, and at the top coarse value the fine part is `00`. A coarse-only value reaches its voice or effect as the same fraction as now, since `4000 / FF00` equals `40 / FF`, so songs sound the same.
*   **A stepped value** is its own integer: a list's place from `00`, or a count in its units. It has no fine part. Adding a choice to a list doesn't move any saved value.
*   **Both** are hex in the grid and in files. Their units are only on the status line and the pages.

**Every setting's kind.** Curves carry over from what each setting does now. The kind names the curve so the status line can show units.

| Setting | Kind | Shown as |
|---|---|---|
| Cut | Continuous, exponential | `1.21 kHz` |
| Res, most voice and effect settings | Continuous, linear | `47 %` |
| Pan, centred voice settings | Continuous, linear, middle | `25 % L`, `+12 %` |
| Vol, sends, gains | Continuous, level, top `MAX_LEVEL` | `−6.0 dB` |
| LFO depth | Continuous, linear, middle | `+40` in hex steps of what it moves, as now |
| LFO phase | Continuous, linear, wraps | `45°` |
| Pitch | Continuous, linear, middle, one semitone a coarse step | `+7 st +12 ct` |
| Patch, Distort's Type, Delay's Time, Ping | Stepped, choices | `E.PIANO 1`, `Crush`, `1/8 D` |
| Crush's Bits | Stepped, 2 to 12 | `12 bits` |

*   **Pitch's fine part** is 1/256 of a semitone, about 0.4 cents.

**Distort's modes have their own settings.** A setting's kind never depends on another setting's value. Instead, a mode owns the settings that mean something in it, as a voice does:

| Types | Its own setting | Kind |
|---|---|---|
| Soft, Hard, Fold, Sine | Amount `1AMNT`, the drive | Continuous |
| Crush | Bits `1BITS` | Stepped, 2 to 12 |
| Down | Rate `1RATE`, how long each sample is held | Continuous |

*   **Mix and Gain** are shared by every mode, as they mean the same in each.
*   **Changing Type keeps each mode's value,** as a track keeps every voice's, so going from Crush to Soft and back finds Bits where it was.
*   **A lock for another mode,** such as `1BITS` with Soft in the slot, is grey and does nothing, like a String lock on a Kick track, and plays again when Crush is back.
*   **Type isn't lockable,** like the slot's effect and the track's machine. Choosing a mode chooses a sound, not something to modulate, and a Type lock would make settings appear for one step. `1TYPE` goes, and Type is set on the Sound page.

**In the grid.** A continuous value shows four digits: `CUT   4080 .`. Its coarse digits are bold in `MAUVE`, the colour the track page uses for values. Its fine digits are in `MAUVE` set back 30% toward the FX band. A stepped value and an event show only two digits. An event's value is bold `PEACH`, as it isn't a setting, like Renoise's own colour for timing commands. A lock the track doesn't have now is grey with no colour (`OVERLAY2`). An empty cell is faint (`OVERLAY0`), as a code editor dims whitespace, so what's written stands out. Each slot is 2 columns wider than now.

| Cell | Coarse | Fine | Label and mark |
|---|---|---|---|
| A continuous lock | bold `MAUVE` | `MAUVE`, set back | text, mark dim |
| A stepped lock | bold `MAUVE` | none | text, mark dim |
| An event | bold `PEACH` | none | text |
| A lock the track doesn't have | `OVERLAY2` | `OVERLAY2` | `OVERLAY2` |
| Empty | `OVERLAY0` | none | `OVERLAY0` |

**Editing a value's fine part.**

*   **The cursor** has a field for the fine digits after the coarse ones, so `l` and `h` go command, coarse, fine. `w` and `b` still move between the note part and each slot. Stepped values and events have no fine field.
*   **The coarse digits:** typing, `r`, `s`, `=` `-` `+` and `_` change them and keep the fine part. A new value, typed where there was none, starts with a fine part of `00`, and doesn't copy the one above, as its mark does.
*   **The fine digits** take the same keys. `=` and `-` move the fine part by one and carry into the coarse part past `FF` or below `00`, and `+` and `_` move it by 16. Nothing carries from coarse into fine.
*   **`x` on the fine digits** sets them to `00`. `x` on the coarse digits clears the slot, as now.
*   **At the top,** a value that would go past it, such as `FF` typed over `3F80`, stops at the top, `FF00`.
*   **Stepped values** move one step at a time, and a list's `wraps` says whether they go round. They can't glide, so `g` isn't offered on them.

**The status line** names the setting, then its value in units: `Filter: Cut, 1.21 kHz`, `FX 1 Distort: Bits, 12 bits`, `String: Decay, 62 %, not on this track`.

**The pages.** The Sound and Mod pages show values in their kinds' units. Their typing in units with `Enter` stays as it is, as for LFO phase in degrees. LFO depth and phase are stored in their kinds' form, as every setting is. How a page reaches the fine part is decided in issue 2. `h` and `l` already move between the page's columns, so it needs a different key.

**Modulation, glides and fills.**

*   **LFOs and envelopes** move a continuous value through its curve, so an LFO on Cut sweeps evenly in pitch. Depth stays in coarse steps of what it moves, as now, or in semitones for Pitch, which is the same thing. On a stepped value the result is rounded to a whole step.
*   **Glides** are for continuous values only, and slide through the fine part.
*   **The shape panel and selections:** a ramp between two continuous values fills the fine part too, so it's smooth. On stepped values every fill rounds to whole steps, and Random picks among them.

**In files.** A cell is its id, a colon, the value in hex and its mark: `cut:4080h`. The fine part is left out when it's `00`, so `cut:40h`, which is also how a stepped value is written: `1.distort.type:03`. A track's changed settings save as their stored numbers, such as `string.decay: 0x9000`. A value past its kind's top doesn't load, and says which.

**In the engine.** `ValueLayers`, the steps' values and a track's settings become `u16`. Glides interpolate the `u16`. A continuous value reaches its voice or effect as a fraction of its top, and a stepped value as its integer, so 6-op's `patch_index` goes.

**Locks for LFO settings and Pitch.** Steps can lock LFO depth and phase, and Pitch, once their kinds exist:

*   Their labels are `DEP1`, `PHS1`, `DEP2`, `PHS2` and `PTCH`, so the labels' rule becomes up to 4 characters after an optional slot digit.
*   A lock layers on the value as any lock does. LFO 1 can still modulate LFO 2's depth, so a lock and modulation add up there as they do for any setting.

**Risks.**

*   **Songs made before this don't load,** which the author has agreed to.
*   **The grid gets wider:** 6 more columns across a step's three FX cells. Narrow terminals aren't supported anyway.
*   **The fine part could be missed.** It's always shown, dim, so two cells that sound different never look the same.
*   **Colours can mean too much at once.** Values take `MAUVE` and events `PEACH`, both of which are already used elsewhere, for the track page's values and for out-of-scale notes and jump labels. Further colours, such as one per group as Renoise has, wait until these have been used in a jam.

**Alternatives considered.**

*   **One normalized number for everything,** as VST3 has. Stepped values would move when a list grows, and every display would need rounding back.
*   **Decimal, dB or note names in the cell,** as the parameter-encoding draft proposed. Each needs its own width, typing rules and parser. Hex in the cell with units on the status line gives the same information.
*   **Signed values, `−40` to `+3F`,** for centred kinds. They need a sign key and break two-key typing, so `80` stays the middle.
*   **Fine typed with a `.` after the coarse digits.** Two digits commit and move down, so a `.` couldn't follow without delaying every commit. A field of its own uses the keys that already exist.
*   **Fine only by nudging, and hidden in the grid.** Two cells could read `40` and sound different.
*   **Display modes per column,** minimal, compact and expanded. One display does, and the status line has the full value.
*   **A range up to `FFFF`.** `FF` alone would then be 99.6%, not full.
*   **Typing coarse resets the fine part,** as the draft had. The author prefers keeping it, and `x` on the fine digits clears it.
*   **Micro-timing in each FX cell,** as the draft had. That's issue #146, about moving notes.

**Left out.**

*   **Controllers,** such as 14-bit knobs and endless encoders. The kinds give a controller what it needs: a continuous value takes 14 bits into its `u16`, a stepped one splits a knob's travel into its steps, and an encoder moves one coarse step or one step a detent.
*   **Typing a value in units in the grid,** such as `1.2k`, `375ms` or `-6db`, which could come later as a prompt.
*   **Envelope times as settings,** which aren't lockable. The Mod page shows them in ms as now.
*   **A colour for each group of settings.**
*   **Micro-timing,** issue #146.

**Issues.** Each leaves everything building and every test passing.

1.  **Kinds and 16-bit values:** the two shapes and every setting's kind, `u16` values in steps, tracks and the engine, the file form, the status line's units, and the grid's four digits and colours. The fine part shows and loads but is typed only as `00`.
    **Code:** `settings.rs`'s `Kind` and entries, and whether a mode's setting exists; `insert.rs`'s `Param`s and a value for each Distort mode's setting; `step.rs`'s `FxSlot`; `steptext.rs`'s parser; `song.rs`'s `Track.settings`; `track_state.rs`'s values and how they reach the voices; `six_op.rs`'s `patch_index`; `views/phrase.rs`'s `step_text` and colours; `views/status.rs`; `theme.rs`.
    **Tests:**
    *   Golden renders unchanged, as coarse-only values give the same fractions as before.
    *   `cut:4080h` and `1.distort.type:03` save and load, and `cut:FF80` and a list's place past its top fail to load.
    *   Each kind's units for named values, such as Cut at `FF00`, and Vol at `E000` as `0.0 dB`.
    *   A `1BITS` lock is grey with Soft in the slot and plays with Crush, Bits keeps its value from Crush to Soft and back, and `1type` isn't a label.
    *   The grid's colours for each kind of cell, as the table above has them.
    *   No allocation in the callback.
2.  **Editing the fine part:** the fine field, the coarse and fine editing rules above, carrying, the top, a new value starting at `00`, ramps and glides through the fine part, stepped values moving by steps and not gliding, and a key for the fine part on the pages.
    **Code:** `app.rs`'s `Field` and typing; `grammar.rs`; `selection.rs`, `shape.rs` and `action.rs`'s nudges; `views/phrase.rs`'s field ranges; the track page.
    **Tests:**
    *   `=` on the fine digits of `40FF` gives `4100`, and `=` on the coarse digits of `4012` gives `4112`.
    *   `40` typed over `3F80` gives `4080`, and `FF` typed over it gives `FF00`.
    *   `x` on the fine digits of `4080` gives `4000`, and a new value under `4080` starts at `xx00`.
    *   A ramp from `4000` to `4100` over 4 rows fills `4055` and `40AB` between them, rounded.
    *   On a stepped value, `l` skips to the next slot and `g` isn't offered.
3.  **Locks for LFO settings and Pitch:** their labels, the engine reading their locks, and the Mod page showing their units.
    **Code:** `settings.rs`'s LFO and track entries; `track_state.rs`'s LFO and pitch paths; `lfo.rs`; `views/track.rs`.
    **Tests:**
    *   `DEP1` locks LFO 1's depth for its step, and its value shows as `+40` on the status line.
    *   `PTCH` at `87` plays 7 semitones up, and its fine part bends by 1/256 of a semitone a step.
    *   An LFO 1 aimed at LFO 2's depth adds to a `DEP2` lock.
    *   No allocation in the callback.

**Issue 1 built** on 2026-11-07. Steps' values and the engine's layers are 16 bits, and the engine hears a fine part as the fraction on its `00` to `FF` scale. The track's own settings and a track's machine settings stay 8 bits until issue 2 gives the pages a way to set a fine part, and are read as coarse only. An effect's settings are also 8 bits, so a fine part rounds off when it reaches an effect; voices and the track's filter, pan and levels hear all of it. Typing over a value of the same setting already keeps its fine part, as issue 2 has it. The status line shows units for a setting the track has, and only names one it doesn't.

**Issue 2 is built in two parts.** The first, on 2026-11-07, is the grid: the fine field and its editing rules, fine nudges that carry, `x`, `r`, `s` and typing on the fine digits, ramps and waves from the shape panel filling the fine part, and lists that hold but don't glide. On a row where the cell has no fine part, the cursor can still sit on the fine column, coming from above or below, and typing there says so. The second part is the pages, built the same day. The author chose a fine stop as in the grid: on the Sound page, `l` goes from a value onto its fine digits and then to the next setting, and `=` `-` `+` `_`, `i` and `x` work on whichever part the cursor is on. A track keeps its settings' fine parts in a map by setting, saved as `fine: {"cut": 128}`, rather than widening every field, and the engine adds a fine part to a setting's base. Cut, Res, Pan, the sends and the machine's continuous settings have one. An effect's settings stay whole, as effects still hear only whole values, so the FX page has no fine stop; giving effects fine values is left for when one needs it.

**Issue 3 built** on 2026-11-07. The engine reads an LFO's depth and phase, and the held pitch, from their values as it does every setting's, so locks, holds, glides and the Mod page all reach them the same way, and the Mod page's depth and phase are those values' base. The status line shows their units on the grid and on the Mod page. `K` lists both LFOs' settings after the track's own. Phase goes round when nudged past either end.

**Done when** each issue's tests pass, and the author has dialled in values with the fine part, locked LFOs and Pitch in a jam, and the grid and its colours stay readable to them.

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
47. **`Diode` is a track filter type too, the same diode ladder as `Acid`'s** (Vital's `DiodeFilter`, ported for #171). Its rate-dependent feedback coefficient is passed in rather than stored, so a track's filter channel can hold one. It runs at 2x, as `Ladder` does, and always saturates. Its cutoff is set so its resonance peaks at `Cut`, as `Ladder`'s does, which means with little resonance it falls well under `Cut`, the 303's soft, early slope. Its stages hear each other a sample late, which pulled its resonance up to 365 cents flat at 8 kHz, and its feedback high-pass pulled it sharp near 200 Hz, so the cutoff is corrected for both, and it now rings within 35 cents of `Cut` from 200 Hz to 8 kHz. It goes in at half level, as its last stage clips at 1, and comes out about as loud as it went in at no resonance. `Drive` is its own drive. The curve drawn for it is the four one-poles and the feedback high-pass worked out as an analog filter, which matches what it does to within 1 dB. Vital's other diode style, with a sweepable high-pass before it, is left out. Added on 2026-11-08.
48. **Entering in Insert stays put, and `;` turns on auto-jump, which moves on by the grid** (#192). It used to move down by a step setting, 1 to start with, and the left and right arrows counted their jump from the entry just written so it kept its place in the beat. Now an entry stays where it's written, so another key plays and writes over it, even while the song plays, and the arrows jump by the grid from the cursor, with nothing to work out. Polyend's Tracker (read in tracker-ce's `fPatternEdit.cpp`) has one `Step` value, from 0 to 32 and 1 to start with, which every entry and delete moves by, with its arrows always one row; Renoise's Edit Step is the same. Here there's one number, the grid, which `Option-1` and `Option-2` already set, and auto-jump says whether entering uses it, so the step setting and `:set step` are gone. `;` toggles it, with `Jump` beside `Grid` on the status line while it's on, as `;` toggles audition on the track page; it's kept with the song and, like the scale, isn't undone. To free `;`, the keyboard ends at `l`, as Ableton's does, so `p` and `;` are no longer notes, and the scale's home row is `a` to `l`. `Backspace` clears the cell, and with auto-jump on goes back a grid step first, so it clears the last entry. An advance of one step as a separate case was left out: narrowing the grid does it. Added on 2026-11-08.
49. **Where the terminal reports releases, the track page's keyboard holds a note while its key is down.** A terminal without the kitty keyboard protocol sends a held key as one press and then a stream of repeats, which played the note again on each, so holding a key gave a rapid stutter. With the protocol's event types, a press starts a note on the preview voice with no end, the terminal's repeats of a note key are ignored, and its release, or leaving the keyboard, lets the note go. The flags (every key as an escape code, with its kind and its shifted character, so `>` still reads as `>`) are pushed only while the keyboard is on, and popped when it's left, so everything else reads keys as it always has; modifier keys alone, which the protocol then reports, are dropped. It's checked once at start with crossterm's support query. Without it, each key plays a short note as before: inside tmux 3.8 the query says no, as tmux doesn't pass releases on, and working out a hold from repeat timing was left out, as it would mistake a quick second press of the same key for a hold. The preview voice is still one voice, so a new key takes over from a held one. `probe-keys --kitty` shows what a terminal sends with the same flags. Added on 2026-11-08. Decision 50 replaces the parts about terminals without the protocol.
50. **vimtracker runs only in kitty and Ghostty, with the kitty keyboard protocol on from start to quitting** (M16, #197). The program needs both of kitty's protocols, for keys and for graphics, which means kitty and Ghostty; tmux, ssh and Neovim's terminal aren't supported, and there are no fallbacks for other terminals. At start, before any audio opens, crossterm's support query is asked, and if it says no the program stops with a message naming kitty and Ghostty; `render`, `bench` and `probe-keys` don't need a terminal that has it, and run anywhere. Once the screen opens, the flags decision 49 pushed for the keyboard alone are pushed for the whole program: every key as an escape code, with its kind and its shifted character. They're popped on quitting, and by a panic hook before ratatui's own, though kitty keeps a separate stack for the alternate screen anyway. Every key reaches the app as a press, a repeat or a release, and what a held key does is decided in one place, `App::key_event`, for every mode. The key down is remembered by where it is on the keyboard, so `:` is `;` and `Shift-Tab` is `Tab`, as Shift can be let go before the key. A press of the key that's down is a repeat (#196). A repeat acts as another press, as it did with no protocol, so a held `j` moves on, except where the press started something that lasts while it's held or mustn't happen twice: the transport keys (`Tab`, `Shift-Tab`, `Space c`) act once however long they're held, and a note on the track page's keyboard sounds until its key is let go. That replaces the 120 ms guard that guessed repeats from timing, and the keyboard's short notes for terminals without releases. Shift is folded into the character: where kitty sends the shifted key as an alternate, crossterm uses it, and where it doesn't, as from kitty's remote control, a US layout gives `;` to `:`, `4` to `$` and so on, with letters to capitals, while a character that's already shifted is left alone. `Key::shift` is still only set on arrows, as no other binding needs it. Caps Lock, which the protocol reports as a lock rather than typing capitals, makes letters capitals without Ctrl or Alt, as it does without the protocol. Shift, Caps Lock and the other keys that only change others, which the protocol reports as keys of their own, and media keys are dropped. `probe-keys` always asks for the flags now, says whether the terminal offers them, and shows the key the program reads each event as, so its `--kitty` is gone. Option keys such as `Option-1` still need kitty's `macos_option_as_alt` or Ghostty's `macos-option-as-alt`. Added on 2026-11-08.
51. **In Insert, a held note key writes its note once and holds its preview while it's down** (M16, #198). It works as the track page's keyboard does (decision 49), through the same code: the press writes the note and starts it on the preview voice with no end, the terminal's repeats of that key, and presses of it while it's down (#196), are ignored, and its release, or leaving Insert by any key, lets it go. With auto-jump the cursor moves on once. The note written still gets its length from its gate, not from how long the key was held; setting a length by holding is for live recording. With the scale on, the degree keys are held the same way, and `1` writes its OFF once. Everything else in Insert repeats as a press, as decision 50 has it: hex digits, FX labels, `Backspace`, the arrows, and the octave and velocity keys `z` `x` `c` `v`. A second note key pressed while one is held takes over the one preview voice, and the first key's release doesn't end it. As the release of a key that's down when the window loses focus may never come, the terminal is asked for focus events alongside the keyboard flags, and on losing focus any held note is let go, on the keyboard and in Insert, and the key down is forgotten, so its next press is a press. Nothing happens on gaining focus, and the mode stays as it was. Added on 2026-11-08.
52. **Jump forward is `Ctrl-I`, as in Vim, and the keys the kitty protocol tells apart reach the program as themselves** (M16, #202). Without the protocol `Ctrl-I` arrived as `Tab`, so jump forward was on `Ctrl-N`; with every key sent as an escape code, kitty sends `Ctrl-I` as `i` with Ctrl and `Tab` as `Tab`, so jump forward moves to `Ctrl-I` beside `Ctrl-O`, and `Ctrl-N` is left to the picker's highlight, where it already was. `Tab` stays play and pause: it was chosen as a transport key, not for want of `Ctrl-I`. `Ctrl-J` and `Ctrl-M` arrive apart from `Enter`, and `Ctrl` with every digit, `Ctrl-1` to `Ctrl-0`, as itself, all checked with `probe-keys` in kitty; none of them is bound. The audit found nothing else chosen for want of a key. `Option-1` and `Option-2` for the grid (decision 23) were a stand-in for Ableton's `Cmd-1` and `Cmd-2` rather than for `Ctrl` with a digit, so they stay until Cmd is decided. Cmd, the super key, does reach the program in kitty, as `D-1` in Vim's name for it: `Key` carries it, and the names parse and print it, but nothing is bound to it, so a key pressed with Cmd is dropped rather than read as the key alone, while its release still lets go of a held note. Whether to bind it, as Ableton's `Cmd-1` and `Cmd-2` would want, is the author's choice; kitty's own shortcuts take `Cmd-1` to `Cmd-9` for its windows, and `Cmd-0` for the font size, until `kitty.conf` unmaps them, so a real keyboard's `Cmd-1` only reaches the program then. Shift is still folded into characters and only kept on arrows, as no binding needs `Shift-Enter` or `Shift-Space`, and keeping Shift on `Space` would stop a `Space` typed with Shift a moment early from being the leader; the names now take `S-` only on an arrow, so they can't name a key that never arrives, and modifiers can come in any order. Added on 2026-11-08.
53. **A key held for 300 ms or more is a hold, and a held mute, solo or audition lasts only while it's down** (M16, #200). One constant, `HOLD_TIME`, tells a tap from a hold by how long the key was down, measured from its press to its release. 300 ms was chosen from Polyend's Tracker, whose buttons count as held after 333 ms (`holdTime` in tracker-ce's `Keys.h`); it's a little shorter so a hold for a drop needn't be long. `Space m` and `Space s` switch the mute or solo at once, as before; let go before the hold time, it stays, and held longer, it switches back on release, for a drop while jamming, which is what Elektron's track holds and the M8's momentary mutes are for. The key that counts is the one that finished the binding, `m` or `s`, and its repeats do nothing. Mute and solo are live state, outside undo and written to the song on save (`App::levels`), so while the key is down a save writes the mute or solo as it was before the press, as it may yet go back; if it's let go as a tap, the song is marked changed again so the next save writes it. `Space M` while one is held clears it for good. The mixer's own mute and solo keys are left as they are. `;` in the phrase, chain and song views auditions as before; held, its notes are let go when it's released, through a new `EndAudition` command that releases the voices the audition started, unless playing has begun since. Tapped, a sample plays out as before, but a synth that holds its note until something ends it, such as `VA`, is let go once it has sounded as long as a preview does, 250 ms, or at once if it already has: before, a tapped audition of one droned until the next audition. A row outside the loop, which plays one note on the preview voice, now starts it held; held, it stops on release, and tapped, it lasts at least as long as a preview did, 250 ms (`FinishPreview`). Losing focus ends any of these as if the key was let go then, as it lets go of held notes (decision 51). The sample browser (`Space 5`) doesn't preview samples, so there was nothing to stop on release, and holding a step to see its locks, as Elektron's trig hold does, is left out. A fresh song's tracks are `Sample` with no file, so nothing sounds there, auditions included, until a sample or a synth is chosen; that's not a fault of audition. Added on 2026-11-08.
54. **Held keys play chords, through a bank of eight preview voices, and in Insert every note key still writes** (M16, #199). The engine's one preview voice is now `PREVIEW_VOICES`, 8, made up front in `Previews` (`preview.rs`), each tied to the track and note whose key started it, so `ReleasePreview` and `FinishPreview` name both and let go of that note alone. Eight covers any chord the computer keyboard's rows have room for, two hands' worth; the M8's keyboard is one note at a time, and Ableton's is polyphonic. Measured in a release build on this M5, eight held notes on one track cost from 0.7% of a core (`Fold`) to 3.8% (`FM`), against 0.4% for one, and with none sounding the engine costs what it did, 0.18%: the voices sounding are found once per run, so a run with none asks nothing more of each frame. They still join their track before its filter, slots and level, as the one voice did. A note goes to the voice already holding the same note on that track, which starts it again, or a free one; with all eight busy, the one let go longest ago is taken over, and failing that the oldest, as most synths steal, and a voice taken over fades out over the declick time on a spare ninth place rather than stopping with a click. Taps of one note in Insert don't pile up, as each restarts the same voice. `Acid` plays one note at a time, as a 303 and Open303 do: a key pressed while another is held slides there on the same voice, with its slide time, and letting go of the newest slides back to the newest still down (Open303's note list); a key pressed with none held starts the note again from the last one's filter, as the sequencer's notes do. No other voice has a slide, so every other synth, `6-op` included, plays a voice per key. The app keeps every key holding a note, not just the last, so a press of any of them is a repeat (#196), its release ends only its note, and losing focus or leaving the keyboard or Insert lets go of them all; two keys holding the same note, as an octave apart can, end it only when both are up. In Insert every note key's press writes, as it always has, whether or not another is down, so a fast typist's rolled keys, a second pressed before the first is up, are all written: with auto-jump each moves on, and without it the last key down wins the row, as Polyend's Tracker writes each pad pressed over the step (`setSelectionNote` in tracker-ce's `fPatternEdit.cpp`). Every key held keeps sounding until it's let go, so a chord is still heard, though a row holds one note. Writing only a chord's first key was tried and dropped, as it lost notes in ordinary typing, which is worse than a chord writing over its cell. Repeats, and presses of a key that's down, still write nothing (#196). Polyend's Tracker, with steps of several tracks selected, spreads a chord across them, which could come later; Renoise writes a chord into its note columns, which waits for more than one note per row. Added on 2026-11-08.
55. **`Space r` arms live recording while the song plays, and a note played in Insert goes in where the cursor's track is playing, as long as its key was held** (M16, #201). `Space r` is in the phrase view's Normal keys, free in the binding table and beside `Space c` with the transport keys, and arms only while playing, as Elektron's live record and Polyend's rec run with the sequencer; `REC` follows `playing` on the status line while it's armed. Pausing, which is how playing stops, or leaving the phrase view disarms it, and so does `Space r` again. While it's armed, a note key in Insert on the Note column writes at the playhead rather than the cursor, the cursor stays put, and auto-jump doesn't move it; every other Insert key works as it did. The key sounds through the preview voices while it's held, as in Insert (decisions 51 and 54). **Where:** on the cursor's track, at the row that track is playing in its own chain, found from the engine's running count and the track's `JamState` as its playhead is, so a 4-row phrase playing against 16 rows gets its row 1 at step 9. The engine now reports how far through its step it is (`Telemetry::progress`); the app carries that on by the clock since the report and takes off the output's latency, and a press past halfway goes on the next row, as Polyend's Tracker does (`getDestRecordingStep` in tracker-ce, past 24 of its 48 ticks). A row of several pulses rounds to its own start or the next row's. How far into the step the press came is dropped, as micro-timing (#146) isn't built, and swing isn't counted when rounding. A note that rounds forward isn't written until its row starts, so the sequencer doesn't play it a second time over the preview already sounding. **Length:** a gate's pulses are how long its row lasts, not its note (M13), so writing a hold there would move every row after it and change the loop's length. Elektron's trig length is the model instead: the length goes in an `OFF` on the note's row, which starts the release that many units in, sixths of a step, so it's as fine as a sixth of a step and moves nothing. It goes in an FX column that follows the rows, the one with an `OFF` already or the first empty one; with none free, the note goes in with no length, and the status line says so. Until the key is let go the `OFF` is as long as it can be, and a note still held when its row comes round again ends there, at the track's loop length, and at most `FF`, 42.5 steps. Polyend writes a note off on the row the key is let go on, if that row is empty; that was left out, as it touches another row and is only as fine as a row. **Overdub:** a recorded note replaces its row's note and velocity, makes its gate a `SGL` keeping its pulses, since a `RST` would silence it and a `MLT` or `HLD` isn't what was played, and adds the `OFF`. Other columns and rows are left alone. Velocity is the current `Vel`, as a computer keyboard has none. **Undo:** a pass, from arming to disarming, is one undo step (`History::squash_to`), along with any other edit made during it. Disarming with a key still held writes its length so far. Added on 2026-11-08.
56. **The kitty graphics protocol is required, and the curves have one drawing path: images, with no Braille** (M16, #203). At start, after the keyboard check (decision 50), the program sends the protocol's own query, `a=q` with a PNG of one dot in the format its drawings use, followed by a request for the primary device attributes, and reads the answers from `/dev/tty` in raw mode on a thread of its own, up to the attributes, which every terminal sends, so nothing is left for crossterm to read. An `OK` before the attributes means it starts; anything else, or nothing within 2 seconds, stops it with a message naming kitty and Ghostty, before any audio opens. The query was chosen over trusting `TERM` (`xterm-kitty`, `xterm-ghostty`), as it's what kitty's documentation recommends, answers for the real terminal, and costs one round trip that both terminals answer at once. `probe-keys` reports it beside the keyboard protocol. Gone are the Braille envelopes (`views/braille.rs`, whose dot geometry stays as `views/curve.rs`, which the images are drawn from), `kitty::Support`, the tmux passthrough check and its hint, the escapes that wrapped commands for tmux, and `--no-graphics`. The track page always draws placeholders for the envelopes, the LFO shapes and the filter curve, and leaves their room blank until their images are sent, which in the program is before the first frame. Tests check the placeholders and the commands sent, as ratatui's `TestBackend` can't show images. `just screenshots` puts the real images in: it runs the same `Graphics::sync` the program does into a buffer, takes the PNGs out of the commands, and draws each as an SVG `<image>` over its placeholders, stretched to their cells as the terminal does, so the manual shows what the terminal shows; it was chosen over a stand-in drawing kept for screenshots, which would have shown something the program never draws. Two shots of the track page, Sound and Mod, are added to the manual's introduction. What `kitty.rs` uses was checked against Ghostty's source and issues, as Ghostty isn't installed here; `docs/terminal-notes.md` lists it and what's still to try by hand. Added on 2026-11-08.
57. **A small master scope sits at the top of the main meter's column, on every screen, drawn as a kitty image** (M16, #20). It's 12 characters by 3, after the meter's gap on the left, with a row between it and `Main`, and it takes those 4 rows from the meter's bars, so it shows wherever the meter does and is left out when the column is too short for both (`views::meter::split_scope`). That place was chosen as the M8 keeps its scope at the top right of every screen, beside its meters, and the column already belongs to the main output; the track page and the mixer have no room to spare, and a toggle would hide what's meant to be glanced at. **What it shows:** the last 20 ms of the output, after the limiter, with left and right mixed, at its real level, so full height is `0` and silence is a flat line across the middle. It's one plain line of dots in the dim colour, as the curves on the track page are, with no frame or scale. **Audio side:** `Realtime::process` copies each block, mixed to one channel, into an `rtrb` ring of `SCOPE_QUEUE`, 16,384 samples, a third of a second at 48 kHz, with `write_chunk_uninit` and no allocation; when it's full, what doesn't fit is dropped rather than waited for. The tests in `link.rs` that run under the real-time guard all run with it, and one fills it, checks nothing allocated and that it holds exactly its size, and that a block arrives whole once there's room. **Interface:** `App::tick` reads what's there into `Scope`, which keeps four windows of sound. **Triggering:** each picture starts at the newest rise through zero that leaves a whole window after it, as a hardware scope's trigger does in its auto mode, and the M8's scope, so a steady note stands still; the sound has to go below 5% of its peak first, so a wiggle near zero doesn't count, and with no rise it shows the newest window. **Drawing:** the window is folded to one column of dots for each of the image's 72, each column's lowest and highest sample, joined to the next, and the image is sent only when those columns change, so silence and a steady note send nothing. Measured in a release build on this M5 with a VA note playing, folding and making the PNG takes 52 µs a frame, and the program's CPU went up half a percent of a core; kitty's went from 0.5% to 4.5% at every frame, as each picture is a new image, so the scope is brought up to date every other frame, about 15 a second, which took kitty to between 2.4% and 2.9%. Smaller images, at one pixel a dot, were tried and saved about as much, but the curves use three pixels a dot so kitty's smoothing as it scales them leaves their edges crisp, and the scope matches them; both together took kitty to 1.4%. `just screenshots` draws it, and the lane shot now plays its notes on `VA` so the scope and the meter have something to show. Added on 2026-11-08.
58. **A track playing the Sample machine shows its sample's waveform on its Sound page, beside the filter's curve** (M16, #205). It sits under the output's column, in a box the size of the filter's, 24 characters by 3, on the same rows, so the two boxes line up and the page grows no taller; a track with no file, one whose file didn't load, or one playing a synth shows nothing there. It's drawn at its real level, as the scope is, so full height is `0`, in the dim colour, or the active one with the cursor on `File`, as the filter's curve is picked out on its rows. **Folding:** each of its 144 columns of dots keeps the lowest and highest sample of both channels in its stretch of the sample (`waveform::spans`, using the scope's `fold`), rather than one sample a column, which would alias a fast wave into a slow one that isn't there. **Cost:** the sample doesn't change while it plays, so it's drawn once for each sample and colour, and held by the sample itself rather than its place in the pool, as a reloaded song can give another file the same place; nothing is sent again until the track's sample changes. **Markers:** the Sample machine has no start, end or loop settings yet, as the engine plays the whole file, so none are drawn; `waveform::spans` takes markers as frames into the sample, each a line from top to bottom, for the sample bank and slicing (#189) to use, and folds any stretch of frames it's given, so a slice can be drawn alone. The sample browser has no waveform yet, as it doesn't load or preview its files. `just screenshots` gives the Sound page a kick drum made in the example, as the repository has no sample files, and the manual's introduction shows it. Added on 2026-11-08.
59. **A compressor's slot on the FX page shows its static curve and a gain reduction meter, and a limiter's shows the meter alone** (M16, #205). They follow the slot's two rows of settings, a space after its fourth column, which is as many columns as `Comp` and `Limit` have, so they need no room of their own; at 120 columns they end just before the main meter. **Curve:** input against output, each from -60 dB, as far down as the threshold goes, to 0, worked out by the compressor's own gain computer (`compressor::curve_db`, which is `computed` with the threshold and ratio as the engine reads them), knee and all, before `Auto`, `Mix` and `Gain`. It's 4 characters by 2, which is about square, the smallest that still shows where it bends and how flat it gets, in the dim colour, or the active one with the cursor on the slot, as the other curves are. It follows the threshold and ratio playing, as the filter's curve follows what plays, and only they are compared, so it's sent again only when one of them changes, and an attack or release changes nothing. A bypassed slot fades its text but not the curve, as fading it would mean another image. **Meter:** one character by the slot's two rows, hanging down from the top over 24 dB in sixteenths, drawn as the main meter's `G` bar is, through the same code (`views::meter::hanging_cell`); it's text, not an image, so it moves with every report and costs kitty nothing new. An empty one shows a faint `▔`, so it's seen to be there. **Engine:** `LiveValues` carries `reduction`, each slot's dB turned down, read once a block when the telemetry is sent: the compressor's detector as it stands, and for the limiter, a gain it now keeps from its smoothing, as `-20 log10` of it, so nothing more is worked out each frame; a bypassed slot, or any other effect, reports `0`. A sound track that isn't run, as nothing is going through it, reports `0` too, rather than whatever its compressor last did, which would stay frozen there. `LiveValues` is no longer `Eq`, as it holds `f32`s. A limiter's curve, which is flat at its ceiling, was left out, as it says nothing its `Ceiling` doesn't. `just screenshots` adds the FX page with both, playing, and the manual's introduction shows it. Added on 2026-11-08.
60. **An EQ's slot on the FX page shows its curve, worked out from the bands' own coefficients** (M16, #205). It's 10 characters by the slot's two rows, all the room after its fourth column before the main meter at 120 columns, where decision 59 puts a compressor's curve, from 20 Hz to 20 kHz across, evenly in pitch, as the filter's curve and the bands' frequencies are, and from -18 dB to +18 dB up, a little more than a band's 15 dB, with flat half way. **Maths:** `eq::response_db` sums the three bands' responses from the coefficients `ThreeBands` works out, through the same `bands` function, which now keeps each band's `g` and `k`: Simper's trapezoidal filter is the analog one through the bilinear transform, so at a frequency `f` it's `m0 + (m1 s + m2) / (s^2 + k s + 1)` at `s = j tan(pi f / rate) / g`. A test runs sines through the EQ at seven frequencies on three settings, including both shelves and a narrow and a wide middle, and the worked-out response is within 0.1 dB of what comes out. **Cost:** as with the compressor, it follows what's playing, so an LFO on a band redraws it, and is sent only when one of its settings changes, and once for each of its two colours, dim, and active with the cursor on its slot. A spectrum behind it, which the issue suggested, was left out: it would be a new image at every frame, which is what costs kitty (#214), and would need the slot's sound sent from the audio thread. Smoother meters on the mixer, the issue's last item, were looked at and left: the mixer's meters already fall in eighths of a cell with a peak hold, and drawing them as images would trade text the terminal draws for free for an image every frame. Added on 2026-11-08.
61. **`MS20 LP` and `MS20 HP` are track filter types: the Korg 35 from the MS-20, ported from sst-filters' K35** (#50), which is GPL 3 or later and adapts Odin 2's `Korg35Filter.cpp`, after Pirkle's AN-5 and Zavalishin's book. It's in `dsp/korg35.rs`. **Circuit:** a Sallen-Key loop: a one-pole on the input feeds a sum, a second one-pole takes the sum to the output, and the output, times the feedback `k`, comes back to the sum through a third. The low-pass has low-passes forward and a high-pass back, and keeps its bass at any `Res`; the high-pass is the mirror, falling at 6 dB an octave under its peak, as the MS-20's does. Worked out, they're `1 / (s^2 + (2 - k) s + 1)` and `s (s + 1) / (s^2 + (2 - k) s + 1)`, so `k = 2` self-oscillates right at the cutoff. **Saturation:** at the sum, where the input meets the feedback, as sst's low-pass has it, through the rational `tanh` the ladder uses; the loop is solved as if clean and the sum saturated, as sst does. sst's high-pass saturates after the feedback gain instead, which leaves it clean at low `Res`, so here both saturate at the sum, and `Drive` drives that saturation for both. sst's blend between clean and saturated is left out. **Resonance:** `k` runs straight from 0 to 2.15, the same 7.5% past self-oscillation as Vital's Sallen-Key filter and the ladder's 4.3, so both start to sing by themselves at about `ED`. Vital's resonance boost from drive was left out, as the drawn curve takes no account of drive and would be wrong. **Tuning:** measured by its ringing at full `Res`, it's within 5 cents of `Cut` from 50 Hz to 16 kHz, and its peak sits at `Cut` from `Res E6` up and within 35 cents from `CC`, so it needs no correction; it falls under `Cut` with less, as any two-pole's peak does. Mystran's linearisation was tried in place of saturating after the solve, and with the slope from one sample late it rang 40 cents sharp at 8 kHz, which a second solve fixed, for no gain over sst's way. **Level:** a quiet signal comes through at exactly unity with no resonance, and a 0.7 saw open loses 0.8 dB to the saturation, where `Ladder` loses 0.3. Its self-oscillation peaks at about 0.4 for the low-pass and 0.5 for the high-pass, where `Ladder`'s is 0.13. `Drive` is `Ladder`'s: the input turned up by as much as 8 and the output down by its square root. **Curve:** `korg35::response`, the analog model on the trapezoidal one-poles' warped axis, which matches what comes out to within 0.05 dB. **Cost:** a throwaway release-mode test on an Apple M5, one channel driven, gave 30 ns a sample for `MS20 LP`, 34 for `MS20 HP`, 49 for `Ladder`, 59 for `Diode` and 25 for a driven `LP`. Added on 2026-11-08.
62. **`Reso` is a slot effect: a bank of 24 resonant modes tuned to the track's note, which the track's sound strikes, after Plaits, Rings and Elements** (#50). It's in `dsp/resonator.rs`, ported under MIT with Émilie Gillet's notice kept from Plaits' `physical_modelling/resonator.cc` and `resonator.h`, Rings' `limiter.h` and the pickups of Elements' `resonator.cc` and `part.cc`, and it's like Ableton's Resonators: a drum loop through it plays as a pitch. **Shared:** Plaits' bank, the band-pass modes, their stretch by stiffness, `NthHarmonicCompensation` and the Q falling off with brightness, is one `Modes` type that the `Modal` voice now uses too, rather than a second port; `Modal`'s output is bit for bit what it was, checked in debug and release. **Settings**, in columns of two: `Tune` and `Material`, `Bright` and `Decay`, `Position` and `Width`, then `Mix` and `Gain`. They're named as the `Modal` voice names Plaits' structure, brightness and damping, and `Decay` rings longer as it rises, as Plaits' damping knob does, so a name that says what it does was chosen over "Damping". `Tune` is the pitch shifter's list of whole semitones, ±24, from the track's note and its `Pitch`, as Warps' oscillator follows them (decision 44), so a step or LFO moves it a semitone at a time. `Material` starts at `40`, in tune, where Plaits' stiffness table is flat to `4B`; below it the partials bunch, above they spread, and Plaits' compensation keeps the third partial at three times the note. `Position` is Rings' and Elements' pickup: each mode is heard by a cosine across the modes, its frequency `00` to `FF` taking it from the end to the middle, half a cycle, as past that it comes back the way it went; it starts at `08`, about Plaits' own 0.015, and at `FF` the even partials drop out. `Width` is Elements' stereo trick: a second pickup wanders on a 0.5 Hz triangle from a tenth of the way along to past the middle, Elements' defaults, and what it hears apart from the first is added to one side and taken from the other, up to Elements' widest, 0.7, at `FF`; it starts at `80`, and `00` is mono. The triangle's rate is fixed, as Elements' is, so it's no setting. It starts all wet. **Modes:** 24, Plaits' count, fixed. **Smoothing:** every setting and the note glide over the smoothing time; the modes are tuned every 24 frames, Plaits' block at 48 kHz, but played a frame at a time, so it adds no delay, and each mode's gains move in a straight line across the block, as Elements moves its position, so `Position`, `Width` and the level never step. After resting, it starts at the note and what's set rather than gliding there from where it stopped. **Rate:** Plaits' Q is for 48 kHz, so every mode's Q is scaled by the rate over 48 kHz, which keeps the decay in seconds the same at any rate (the `Modal` voice doesn't do this yet, so it rings shorter at 96 kHz). The bank is tuned from 16 Hz to 12 kHz; its lowest mode is within 3 cents of the note from `C-1` to `C-7` at 44.1 to 192 kHz. **Level:** what goes in is the middle of its two sides at an eighth, as Rings and Elements turn theirs down, and what comes out is turned down by the root of what the modes together would pass: each mode's gain squared, times its frequency, times its Q, but no more Q than rings for 20 ms, a drum's length. So a band-pass that rings for less than a hit is matched as a filter passing noise, and one that rings for longer as a click it gathers up, whose peak doesn't grow with its Q. Then it's set so a drum at `Decay` `80` comes out as loud as it went in: within about 1 dB, by RMS and peak, at middle C, and by RMS within 1.2 dB from `C-2` to `C-6`, its peaks within 3 dB. A longer `Decay` rings on and builds up, by about 5 dB at `C0` and 10 dB at `FF` against noise bursts four a second, and its peaks stay within about 3 dB. **Bounded:** a tone held at its note can ring tens of dB louder, so Rings' limiter follows the loudest of the two sides and their difference, rising quickly and falling over about a second, its constants changed to take as long at any rate, and turns both down to keep that under full scale, with a soft clip after; what comes out never passes full scale, tested with a square at 12 dB over full scale, every extreme and the note jumping between ends every frame. **Memory:** its modes, their two sets of gains and their ramps come to 1448 bytes, so it comes from the effect pool, as Warps' filter bank does, rather than taking each slot from 944 bytes to about 2.4 KB. It stops working once nothing's gone in or come out for 0.1 s. **Cost:** a throwaway release-mode test on an Apple M5 gave about 20 ns a frame at its defaults, 0.1% of a core at 48 kHz and 0.4% at 192 kHz. Left for later: chords, as Rings splits its modes among up to four notes from its chord table; Rings' brake on the highest partials' decay; Elements' bowed modes; and a way to hold the note fixed, since a sample track's notes also change its sample's speed. Added on 2026-11-08.
63. **AIFF and FLAC load wherever a WAV loads** (#226). Ableton's Core Library, which kits are to come from, has AIFF in 1,373 of its 4,680 sample references, and only 51 of its 145 drum racks are all WAV. They're read with `symphonia` 0.6.1 (MPL 2.0, which GPL 3 allows), with only its `aiff`, `flac`, `wav` and `pcm` features, so no other format or codec is built in. **WAV is unchanged:** it's still read by `hound` first, so every WAV that loaded before loads to the same frames and the golden renders don't move; only the decoding moved into its own function, and making the sound stereo and converting its rate is shared. **Which reader:** the extension says, `.aif`, `.aiff` or `.flac` in any case going to `symphonia`, and anything else to `hound` as before, so a file with no extension still loads as a WAV. Ableton names its AIFFs `.aif`. **A WAV `hound` can't read** goes to `symphonia`, and if that fails too, `hound`'s error is the one shown. Eight of Ableton's WAVs need it: they have a 7-byte `inst` chunk before the data, and `hound` 3.5.1 doesn't skip the pad byte after a chunk of odd length. With it, all 3,571 sample files in the Core Library load, 1,351 AIFF and 2,220 WAV, checked by a throwaway scan. **Scaling:** `symphonia` turns 16-bit and 24-bit integers into floats by dividing by 32,768 and 8,388,608, as the WAV reader does, so an AIFF or FLAC loads to exactly the frames of the same sound in a WAV, at its own rate or converted. Checked-in files of 1,200 frames, 16-bit stereo at 48 kHz and 24-bit mono at 44.1 kHz, made from WAVs by `afconvert` and `ffmpeg`, test it. More than two channels is refused, as for a WAV, and a file that won't decode is an error naming it. **The sample browser** lists `.wav`, `.aif`, `.aiff` and `.flac`, from one list of extensions in `vt-model`, and `load_wav` is now `load_sample`. Left for later: other formats such as MP3 or Ogg, which drum kits rarely use. Added on 2026-11-08.
64. **Where kits and samples are found is a setting for the machine, in `settings.ron` beside `audio.ron`** (#226). Kits are to come from the author's Ableton library, which stays where Ableton keeps it, so a folder relative to wherever the program ran no longer works; `samples/kits` from the working folder is gone. **The file** is in the config folder, `$XDG_CONFIG_HOME/vimtracker` or `~/.config/vimtracker`, and holds two lists of folders, `kits` and `samples`, each looked through in order, a folder that isn't there passed over. Paths are kept as written, so `~/` stays readable, and expanded when used. **Defaults:** `~/Music/vimtracker/kits` and `~/Music/vimtracker/samples`, kits and samples in folders of their own under one, as the M8 and the Polyend Tracker keep theirs on the card. **Made on first run:** with no file, the program writes one with the defaults and a note on each line, so there's one to find and edit while there's no page for them; that page is left for the project page (#223). A list left out keeps its default, a name it doesn't know, such as `kit`, is refused rather than ignored, and a file that won't read uses the defaults and says why on the status line at start. It's read once, at start. **Kits:** `:kit 909` opens the first `909.ron` in the kit folders, so an earlier folder overrides a later one, as a search path does; a name found nowhere is reported at the first folder's path. `:kit` alone lists the kits of every folder in order, and with none says which folder to add one to. **Samples:** `Space 5` lists the project's `samples` folder, then the sample folders, then the folder the program ran from, as before. **The author's 909:** `samples/kits/909.ron` in the repo folder is moved to `~/Music/vimtracker/kits` in the next PR, when it's rewritten in the new kit format, rather than the old folder being looked in too. The module is `vt_tui::config`, as `vt_model::settings` is the songs' list of settings. Added on 2026-11-08.
65. **`Comb` is a slot effect: a comb filter tuned to the track's note, after Vital's** (#50). It's in `dsp/comb.rs`, adapted from Vital's `synthesis/filters/comb_filter.cpp` and `comb_filter.h`, with its Catmull-Rom reads from `synthesis/lookups/memory.h`, GPL 3 or later with Matt Tytel's notice kept, and its tuning follows the way Rings' `dsp/string.cc` (MIT) corrects for its damping filter, not its code. **Name:** `Comb`, as the Digitakt II's `COMB` filters and Vital's are called. **Styles:** Vital has six, three feedback styles (`Comb`, `Flange+`, `Flange-`) by two filter styles (`Low High` and `Band Spread`). Here they're a `Mode` of `Comb` or `Flange`, and the sign of `Feedback`: negative is Vital's `Flange-`, reading half a cycle back and turning what it reads over, so the peaks sit on the odd harmonics alone at the same pitch, and it does the same as a `Comb` (Vital's negative comb reads a whole cycle back, which puts its pitch an octave down; here it stays on the note). `Feedback` `80` is none: a `Comb` is then only the sound, and a `Flange` a flanger standing still. Turning the feedback over crosses from one read to the other over its first sixteenth below `80`, and the mode crossfades, so neither clicks. The filter styles are `Tone` and `Spread`: `Tone` is the low-pass in the loop, from the note at `00` to eight octaves up at `FF`, and `Spread` brings the high-pass up under it, from eight octaves under the note at `FF` to the low-pass itself at `00`, Vital's narrowest band; so `Spread` `FF` is Vital's `Low High` at its low-pass end, `Tone` `FF` with `Spread` about `80` its high-pass end, and anything else its `Band Spread`. Vital's blend of the two at a cutoff, a shelf, is left out. The pair is Vital's: one-pole low-pass and high-pass, turned up by `1 + g_high / g_low` so the peak passes at unity, here exact trapezoidal one-poles rather than Vital's approximate coefficient. **Settings**, in columns of two: `Mode` and `Tune`, `Feedback` and `Tone`, `Spread` and `Mix`, then `Gain`. `Tune` is `Reso`'s ±24 semitones from the track's note and `Pitch`. `Feedback` follows Vital's curve, the loop's gain the square root of how far it's set from `80`, up to Vital's 1; it starts at `F0`, about 0.4 s to ring away at middle C, and like any comb's, a lower note rings longer. `Tone` starts at `80`, keeping four octaves of harmonics, and `Spread` at `FF`. It starts all wet. **Tuning:** Vital reads a cycle back, so its filter's delay pulls its pitch off. Here the read is shortened by the band's phase delay, as Rings' string is, but worked out where each harmonic's pole actually is: a little inside the circle, as far as it loses each time round. Its phase there decides the delay and its gain how far inside, so the two are worked out in turn three times; on the circle alone, a loop losing 30% a cycle came out 15 cents off. Each harmonic needs a slightly different delay, so the delay is the average over a spread of harmonics up to the 64th, weighed by the fourth power of how long each rings, which lets the one that rings longest decide: the fundamental under a low-pass, a higher harmonic in a band or over a high-pass. Then the cubic read's own delay at that frequency is taken off, twice over. Measured from what rings after a click, the harmonic that rings longest is within 3 cents of where it should be from `C-1` to `C-7` at 44.1 to 192 kHz, with `Tone` from `00` to `FF`, narrow bands, high-passes and both signs of feedback, except negative feedback darker than `Tone` `30`, which is gone within a few cycles: measured that way it's up to 12 cents off, but fitting its poles offline puts it within 0.1 cent. **Low end:** with feedback positive, nothing at all, which doesn't move, is one of the loop's peaks too, and a first-order high-pass in the loop doesn't stop what's well under the note going round: it rings as a slow wobble about a twentieth of the note, 13 Hz at middle C, as Vital's does. Vital's high-pass four octaves down also moves the harmonics out of tune with each other by about 17 cents. So the high-pass within the loop goes no lower than eight octaves down, about a cent, and what goes into the line is high-passed first by two one-poles, outside the loop where they can't pull the tuning: two octaves under the note as a `Comb`, and four as a `Flange`, whose notches need what comes out of the line to match the sound. What's heard keeps the sound's own bass. **Bounded:** Vital's hard clip on what's fed back, so nothing runs away at full feedback, then Rings' limiter, as `Reso` has, and a soft clip; tested with a square at 12 dB over full scale at every extreme of mode, feedback, `Tone` and `Spread`, and the note jumping between ends every frame. **Level:** what comes out is turned down by the root of the power a hit would make, across eight frequencies, with as many trips round the loop as fit in 20 ms, as `Reso` matches its level. Vital's own turning down of a comb's input as its feedback rises and a flanger's straight sound is kept for their balance. A drum at the default settings comes out within 2 dB of its own level by RMS and peak from `C-2` to `C-6`, either mode. At full `Feedback` it rings on and builds up, by 1–8 dB RMS, and up to 13 dB with `Tone` at `FF` too, its peaks within 5 dB. The level is worked out as if the drum were white noise, so a thin loop, `Spread` near `80` with `Tone` at `FF`, comes out up to 8 dB louder at low notes, where most of a drum is under its high-pass. **Smoothing:** every setting and the note glide over the smoothing time; the loop is worked out every 64 frames and everything it plays with moves in a straight line across them. A test jumping every setting, the mode, the sign of the feedback and the note every 64 frames finds a largest step of 0.0014 against 0.044 unsmoothed. **Memory:** two lines of 1.25 cycles of 16 Hz, the lowest it's tuned to, rounded up to a power of two: 32 KB at 48 kHz and 128 KB at 192 kHz, from the effect pool, as the flanger's are. **Cost:** a throwaway release-mode test on an Apple M5 gave about 13–16 ns a frame with nothing moving and 20 ns with the note gliding or `Tone` sweeping, against 18 ns for `Reso` in the same test: 0.1% of a core at 48 kHz, 0.4% at 192 kHz. Working the loop out takes about 0.5 µs, so it's only done when something's moved by more than can be heard, and only for the read that's heard. Left for later: Vital's shelf between low-pass and high-pass, and keytracking or a fixed note, as for `Reso`. Added on 2026-11-08.
66. **A kit file says where the kit came from and what each sound is, after the format proposed in #226.** A kit has a `name`, a `source` such as "Ableton Core Library" and a `category` such as "Drum Machines", for the kit picker to group by later (#221), and its `sounds`. Each sound has a `label`, the track's name, a `name` for what it is, its `path`, `variants` for other tones of it, `layers` for velocity layers soft to hard, a `choke` group, a `gain` in dB and a `tune` in semitones. **Only `sounds` and each `path` are needed**, so a kit of labels and paths still reads; a kit's name is its file's name if it doesn't say, and a sound's is its file's. The `Space 5` list shows each sound by label and name, where it showed the file name. **Kept, not used yet:** variants and layers wait for the sample bank (#189) and velocity layers, and choke groups, gain and tune for the change after the importer, which sets a track's choke group, level and pitch from its kit sound; nothing a kit says beyond the label and path changes the sound yet. **Ranges:** choke groups are 1 to 16, as Ableton's are, and `None` or left out for none, in RON's own `Some(1)`, as the issue writes it; tune is up to 48 semitones either way, as far as Simpler's transpose goes; gain is any finite number of dB. A field it doesn't know is refused, as `settings.ron` refuses one (decision 64), so a misspelt `chock` isn't quietly lost, and the error names the sound by its label, or its path when it has none. **Writing:** the file's types, `KitFile` and `SoundFile`, also write, leaving out what's empty or zero, for the importer to come. **The author's 909** moved from `samples/kits/909.ron` in the repo folder to `~/Music/vimtracker/kits/909.ron`, the default kit folder, in this format: its paths are now absolute, into `samples/ableton-909` in the repo folder, where the samples stay; its hats share choke group 1; and the other tones in each sound's folder are its variants. Added on 2026-11-08.

67. **`Cmd-h` `Cmd-j` `Cmd-k` `Cmd-l` nudge a value without moving, in place of Option, and the window is titled `vimtracker`** (M16). Option with the movement keys was an awkward reach for the thumb, and Cmd is easy, so `D-l` and `D-h` act as `=` and `-`, and `D-k` and `D-j` as `+` and `_`, wherever the Option ones did: the phrase grid and its selections, the chain and song views, the track page, the mixer and the audio page. The Option versions are gone rather than kept beside them. `App::key_event` still drops a key pressed with Cmd unless it means something in the current mode, after any keys typed before it, so `Cmd-a` in Insert writes nothing and `Cmd-1` is still ignored; a held `Cmd-l` repeats as any other key does, through the same path. kitty takes `cmd+h` (hide), `cmd+k` (clear to the cursor) and `cmd+l` (clear the last command's output) by default, and only `cmd+j` is free, so the program sets the window title with crossterm's `SetTitle`, to `vimtracker` or `vimtracker: <folder name>`, at start and whenever opening, saving as or `:new` changes the project. kitty's `map --when-focus-on title:vimtracker cmd+l` and so on then unmaps those shortcuts for vimtracker's window alone; `docs/terminal-notes.md` has the lines, and the alternative, `map cmd+l no_op`, which frees them in every window. The title isn't put back on quitting, as crossterm has no way to read or restore it; the shell or kitty sets its own again. Ghostty's `keybind = super+l=unbind` frees a key in every window, as it has no condition on the title, and is unchecked. `Option-1`, `Option-2` and `Option-s` are untouched. Added on 2026-11-08.
68. **`vimtracker kits import-ableton [library] [out]` writes a kit for each of Live's drum racks, pointing into Live's library** (#226). **Defaults:** the library is the Core Library of the newest Live in `/Applications`, by version and then Suite, Standard and the rest, so Live 12 Suite here; out is the first kit folder in `settings.ron` (decision 64), under `Ableton`, and each kit goes in a folder named for its rack's, its category: `Ableton/Drum Machines/909 Core Kit.ron`. **Reading:** every `.adg` under `Racks/Drum Racks`, unzipped with `flate2` 1.1.10 and read with `roxmltree` 0.21.2, both MIT or Apache 2.0, small, with no dependencies past `flate2`'s Rust `miniz_oxide`. A pad is a `DrumBranchPreset` of the outermost drum rack, so the 62 racks Live wraps in an instrument rack for their macros read the same; its note is 128 less its `ReceivingNote`, and sounds are in note order, C1 first, as Live's pads go. Its samples come only from its instruments: Simpler's and Sampler's sample parts and Drum Sampler's sample, not the reverb impulses some racks also name. **What it plays:** a pad's samples are sorted by zone, their key range and the chain and sample selectors above them. The zone that covers the note the pad sends, usually C3, at the lowest selectors, is what it plays; the hardest of its velocity layers is the sound's `path`, and when it has more than one, all of them, soft to hard, are its `layers`. Every other zone, another key or selector, gives its hardest layer as a `variant`, as do samples stacked on the main one at the same velocities, which sound together in Live. So the 909's bass drum has its four soft-to-hard layers and three other tunings as variants. **Names:** the pad's name, which Live leaves empty on every pad here, then its first instrument's, such as "Bass Drum", then its sample's file name, as most of the Sampled kits need. **Labels:** guessed from the name's words, then the sample's path's, after the Rytm: `hihat` or `hat` gives `CH`, or `OH` with `open`; then `ride` `RD`, `crash` or `cymbal` `CY`, `cowbell` `CB`, `clap` `CP`, `rim` `RS`, `snare` `SD`, `tom` `LT` `MT` or `HT` by `low`, `mid` or `hi`, and `kick` or `bass drum` `BD`. Hats come first so a hat's `hi` isn't a tom's, and a tom that says no more gets no label. A label goes on the lowest pad that earns it, so a second kick is known by its name alone. **Choke** is the pad's choke group, 1 to 16. **Gain** is the instrument's volume over where Simpler and Drum Sampler start, -12 dB, where 696 of the library's 1,366 Simplers sit, plus the pad's mixer volume in dB, to a tenth; **tune** is its transpose, with its fine tune, and the semitones between the note the pad sends and the sample's root key, to the nearest semitone and within ±48. Chain mixers inside a pad, macros and envelopes aren't read. **Left out:** pads with no sample, synths such as Drift and Meld, and a sample that isn't in the library is reported and its part passed over; a rack with no sample on any pad writes no kit. **Re-running:** each file it writes starts with a comment saying the importer made it. It replaces only files that start so, leaves any other file in its way alone and says so, and removes its own files for racks that have gone, so it can be run again after a Live update and touches nothing else. **The kit picker** now looks in folders inside the kit folders, four deep, as the sample browser does, and lists a kit by its path from its kit folder, such as `Ableton/Drum Machines/909 Core Kit`, which `:kit` takes; the grouping by source and category is #221's. **Run for real** on 2026-11-08 into `~/Music/vimtracker/kits/Ableton`: 138 kits with 2,207 sounds, from 145 racks and their 2,323 pads; 116 pads play a synth, the 7 racks of nothing else make no kit, and every sample named was found. Added on 2026-11-08.
69. **`Vowel` is a slot effect: a voice's five formants, band-passes fixed in pitch, morphing from vowel to vowel, after Plaits' speech synth and Vital's formant filter** (#50). It's in `dsp/vowel.rs`, with the vowel table from Plaits' `speech/naive_speech_synth.cc`, MIT, Émilie Gillet's notice kept; Vital's formant filter (GPL 3) was read for how a bank of band-passes filters a sound rather than a pulse, and none of its code is used. **Name:** `Vowel`, a whole word as the other effects' names are, rather than `Vowl` or `Fmnt`. **Table:** Plaits' five vowels by five registers, each with five formants' pitches and levels, rather than Vital's four vowels on an XY pad with four formants each. It's MIT, has a fifth formant, and its registers give a second axis that means something, so `Vowel` and `Size` are two plain settings that steps lock and LFOs move, rather than an X and a Y whose corners are vowels. Vital's two layouts, `AOIE` and `AIUO`, each leave a vowel out; one line through all five leaves none out. **Settings**, in columns of two: `Vowel` and `Size`, `Res` and `Track`, then `Mix` and `Gain`. `Vowel` has `A` at `00`, `E` at `40`, `I` at `80`, `O` at `C0` and `U` at `FF`, so each sits on a round hex value, and blends each formant's pitch in semitones and its level in a straight line from one to the next, as Plaits does; its value reads as `A`, `A>E` and so on. `Size` is Plaits' register, the smallest voice at `00` and the deepest at `FF`, at the same five places, blended the same way, so the formants lower as the voice grows. The two blend together, as Plaits blends its table in both directions. `Res` is the formants' Q, one for all of them as Plaits has it, from 2 at `00` to 30 at `FF`, evenly in ratio: Plaits' 20 is at about `D9`, and 30 is Vital's most. It starts at `80`, a Q of 7.7. `Track` moves the formants with the track's note and `Pitch` from middle C, none at `00`, the default, as fixed formants are what make a vowel, to a semitone a semitone at `FF`. It starts as an `A` in the middle voice, all wet. **Filters:** the track filter's TPT state-variable filter, its step now shared as `Svf::tick_tuned` so the five can share their tuning between the two sides; each band-pass is scaled by its damping so its centre passes at the table's level, and the five are added, as Plaits and Vital add theirs. **Smoothing:** every setting and the note glide over the smoothing time; the formants are worked out every 32 frames, and their tuning, damping and gains move in a straight line across them, with each one's `1 / (1 + g (g + k))` worked out every frame while they move. A test jumping every setting from end to end and the note every 64 frames finds a largest step of 0.0015 against 0.016 unsmoothed. **Level:** band-passes pass only a share of a sound, so what comes out is turned up by the root of how much of pink noise from 20 Hz to 20 kHz the formants pass: each formant's level squared times a band-pass's share of it at that Q, about `pi / 2Q`. Pink noise comes out within 1.2 dB of its own level at every vowel and voice, and at `Res` `00`, `80` and `FF`. Matching it to pink noise, as music's long-run spectrum roughly is, rather than to the white noise `Reso` and `Comb` match a drum to, means a saw at `80` comes out from 7.7 dB under to 5.9 dB over from `C-2` to `C-5`, louder where a harmonic sits on a formant and quieter where none does, and a drum of white noise 6–12 dB quieter, as most of its power is above the formants. Since the gain depends on the levels and Q alone, and the first formant is always at full level, sweeping `Vowel` doesn't pump. **Bounded:** at `Res` `FF` a tone on a formant comes out about 20 dB up, so Rings' limiter, as `Reso` and `Comb` have, keeps it within full scale, with its soft clip; tested with a square at 12 dB over full scale at every extreme of `Vowel`, `Size`, `Res` and `Track` at 44.1 and 192 kHz, with the note jumping between ends every frame. **Tuning:** each vowel's formants are checked against the table by the frequency response from a click, at every vowel and voice at full `Res`: what comes out is the table's shape, the band-passes as analog ones through the bilinear transform, within 0.02 dB at each formant and between them, and every formant that stands out from the others' skirts peaks within 4 cents of its pitch. **Memory:** its five filters on each side, the formants' ramps and its smoothers come to 368 bytes, kept in every slot as an EQ's bands are, which takes a slot from 944 bytes to 1312. **Cost:** a throwaway release-mode test on an Apple M5 gave about 12 ns a frame standing still and 15 ns with `Vowel` sweeping, against 10 ns for `Comb` and 11 ns for `EQ3` in the same test: under 0.1% of a core at 48 kHz. Left for later: a curve on the FX page, as `EQ3` has; Vital's spread, which pulls the formants towards each other; and a formant shift beyond Plaits' registers. Added on 2026-11-08.
70. **A track has a choke group and a pitch of its own, and a kit sound sets them with its level** (#226). **Choke:** a track's `Choke` is `Off` or a group from 1 to 16, as Ableton's are. When a note starts on a track in a group, from a step, a delayed step or an audition, every other sound track in the group fades what it's sounding over the declick time, as a new note on a track fades its last; so a closed hat cuts an open one, as on Live's drum racks and the Rytm's. The track that plays is never cut by its own group, notes that tie on start nothing so cut nothing, and the track page's keyboard previews don't choke, as they play on voices of their own. Two in a group on the same step leave the later track's. It's read from the published song on the audio thread, with no allocation. **Pitch:** the track's `PTCH` setting, which only modulation and steps moved, now has a base, the track's `Pitch`, a semitone a step either side of `80` with a fine part in cents, as a step's `PTCH` value reads; LFOs, envelopes and steps move it from there. **On the Sound page** they're in the right-hand column, under `Swing`: `Pitch` with a centred bar, and `Choke`, which `=` and `-` step through, `x` turns off, and `i` takes as a number or `off`. **From a kit:** `:kit name all`, and choosing a kit's sound with `Space 5`, set each track's `Level` from the sound's gain, a quarter of a dB a step from unity `E0` up to `F8`, its `Pitch` from its tune, and its `Choke` from its group, with the sample and name, as one step to undo. A sound with none of them sets the level back to `E0`, the pitch to `80` and the choke off, so a track takes the kit's sound as it is, not the last one's settings. Added on 2026-11-08.
71. **`Dirty` is a track filter type: Vital's Dirty filter, a 24 dB low-pass whose resonance loop saturates** (#50), ported from `dirty_filter.cpp`, which is GPL 3 or later, keeping Matt Tytel's notice. It's in `dsp/dirty_filter.rs`, and shares the diode ladder's one-pole, Vital's `tanh` and the complex numbers for its curve, which come from the same source. **Circuit:** two trapezoidal one-poles, then two more into a sum through Vital's `tanh`, which is the output. The sum goes round through a one-pole low-pass, and what that takes out through a second, a band-pass, both with Vital's `quickTanh` on their states, and comes back into the sum, times the resonance `r`. Vital's solve for this sample's feedback is kept as it is, so the loop still hears its states a sample late. The one-poles take the exact trapezoidal coefficient, `g / (1 + g)`, as the MS-20's do, in place of Vital's cheaper warp of it, which runs flat at high cutoffs. **Styles:** only the 24 dB low-pass, as `Dirty`, which keeps the list at nine; Vital's 12 dB style is the same filter without its first two one-poles, and its blend from low-pass through band-pass to high-pass, and its notch styles, are other mixes of the same outputs, so either could become a setting on `Dirty` later at no cost, rather than more types. Vital's 24 dB style feeds its first two one-poles back through states its plain one-poles never write, so that feedback is always zero there; it's left out, with the gain meant to balance it, which only lifted high cutoffs by up to 2.4 dB. **Resonance:** `r` runs from 0.1 to 2.15 on the square root of `Res`, eased off at high cutoffs, as Vital has it; it starts to ring by itself at about 2, which is `Res DB` at low cutoffs, earlier than `Ladder` and the MS-20's `ED`, and later at high cutoffs, where it's eased. **Drive:** as Vital has it, `Drive` turns up only what comes straight into the sum, from 1 to 8 as for the others, divided by `1 + r^2 / 2` so a high resonance isn't swamped, and adds up to 0.05 to `r`. The output is turned down by the square root of the drive, as the others are. As what comes straight in is only a gain on a quiet signal, the extra resonance is the only way drive changes the shape of its response, and unlike #218's choice for the MS-20 it's kept, as it's this filter's character, and the curve takes it: `response_db` now takes the drive, which only `Dirty` reads, and draws the resonance it adds but not the level, as the others leave out drive's level. **Tuning:** under where it rings by itself, the peak is at `Cut`, within 5 cents from `Res D9` and 15 from `CC`, and falls under it with less, as a two-pole's does. Past it, its saturating one-poles slow as it rings louder, so it rang up to 60 cents flat; the loop's coefficient is raised by `0.4 e / (1 - G)^3`, where `e` is how far `r` is past where it starts to ring, worked out from where its poles reach the unit circle, which keeps its ringing within 3 cents of `Cut` from 50 Hz to 16 kHz, with or without drive. Under that it changes nothing, so the curve is untouched where it can be seen. **Level:** a quiet signal with no resonance comes through at unity; Vital's 0.1 of resonance at the bottom and its `tanh`'s slope of 1.004 are taken out. A 0.7 saw, open, loses 1.8 dB to its saturation, where `Ladder` loses 0.3 and `MS20 LP` 0.8, so it's a little crunched even at `Drive 00`. The bass at 100 Hz, with `Cut` at 1 kHz, is 3.1 dB down at `Res 40`, 5.4 at `80` and 6.8 at `C0`, where `Ladder`'s is 6.3, 9.9 and 12.4. Ringing by itself, it peaks at about 0.36, about `MS20 LP`'s 0.37, where `Ladder`'s is 0.13, and the output stays within 0.8. **Curve:** `dirty_filter::response`, worked out exactly from the filter's own updates in the z-plane, with the sample of lag left in, and through the slope of its `tanh`, which matches what comes out to within 0.1 dB. **Aliasing:** a 5 kHz sine driven hard puts its 9th and 7th harmonics, folded to 3 and 13 kHz, at -68 and -71 dB. **Cost:** a throwaway release-mode test on an Apple M5, one channel driven, gave 52 ns a sample for `Dirty`, 49 for `Ladder`, 61 for `Diode`, 31 for `MS20 LP` and 25 for a driven `LP`; its settings are worked out each sample, as drive moves them. Added on 2026-11-08.

72. **`Mode` is one shared filter setting that gives the types their other responses, so the list stays at nine** (#50), from the research comment's small add-ons and decisions 61 and 71. It's `FilterMode` in `song.rs`, `Track.filter_mode`, and the mixes are in `dsp/filter.rs`. **What it means:** `LP`, `HP`, `BP` and `Notch` offer `12 dB` and `24 dB`, as tracker-ce's multimode filter cascades two state-variable filters for its 24 dB low-pass. The second filter has no resonance of its own for a low-pass or high-pass, so the peak stays as high as at 12 dB rather than doubling in dB, and at `Res 00` the pair is a Linkwitz-Riley, 6 dB down at `Cut`; a band-pass or notch has the first's, as their peak and dip don't grow with resonance, so they get narrower and wider. `Ladder` and `Dirty` offer `LP24`, `LP12`, `BP`, `HP12`, `HP24` and `Notch`. The ladder's are Vital's `LadderFilter` weights on what goes into its first stage and each stage's output, such as `{1, -4, 6, -4, 1}` for `HP24` and `{0, 0, -1, 2, -1}` for `BP`, whose sign keeps its peak in step with the others'; `Notch` is Vital's notch-pass-swap at its middle, `{1, -4, 8, -8, 4}`, which is `((1 - L)^2 + L^2)^2`. `Dirty`'s are Vital's 24 dB and 12 dB styles: each of its two two-poles gives low-pass, band-pass or high-pass from its input and one-poles, and its 12 dB modes pass straight through the one before the loop, as Vital's `process12` leaves it out. `BP` is the 24 dB style's, 12 dB an octave either side, rather than offering both. `Diode` offers `Full`, Vital's lowest `2^-9`, as `Acid` and the diode had it, and a low cut 3, 2 and 1 octaves under `Cut`. Vital's second diode style sweeps that high-pass with a knob; four steps were chosen over a continuous setting, so `Mode` stays one list choice, the steps follow `Cut` as Vital's does, and the utility filter's `Low cut` is already a continuous low cut. The MS-20's offer none: their 12 dB and 6 dB responses are the circuit, and `Mode` shows `--`, dimmed, as the track page dims what doesn't apply, with `=` and `K` saying the type has none. **Levels:** a 24 dB band-pass passes a quarter at its peak with no resonance, 12 dB down, so `BP` on the ladder and on `Dirty` is turned up by the low-pass's bass over what it passes at `Cut`, up to 4 and down to 1 as resonance makes the peak the loudest part: it peaks as loud as `LP24` is at its loudest, within 0.6 dB with no resonance and 0.5 at `Res CC`. On the ladder that's `|4 - k| / (1 + k)`; on `Dirty` it's worked out from its response at `Cut` and at DC every 32 frames, and goes on the two-pole before the loop, so the loop saturates as it would for a low-pass. The high-passes pass the top at unity, as the low-passes pass the bass; `Dirty`'s top falls with `Res` as its bass does, as its resonance takes from what comes straight in. `LP12` and `HP12` on the ladder ring twice as loud as `LP24`, as Vital's do. **Tuning:** the loops are unchanged, so every mode of `Ladder` and `Dirty` rings at the same pitch as its low-pass, within 2 Hz from 200 Hz to 4 kHz; the ladder's `Notch` takes out the pitch it rings at, so it hardly self-oscillates, where `Dirty`'s output is its loop and still does. **Changing type:** the track keeps its mode, and a type that doesn't offer it plays its first, so going from `Ladder` at `HP24` through `LP` and back finds `HP24`, as the M14b design has a slot keep each of Distort's modes' settings. **Locks:** `MODE` is a stepped track setting, a place in the list of all twelve, from `00` `12 dB` to `0B` `-1 oct`, so steps can lock it and LFOs and envelopes can move it a whole step at a time. A lock for a mode the type doesn't offer plays its first, by the same rule, and on an MS-20 the lock is grey as a lock the track doesn't have. Unlike the filter type, a mode changes no settings, and it glides, so a lock is safe. **No clicks:** each channel moves its mix toward the mode's with the 10 ms smoother, every sample: the ladder's five weights, `Dirty`'s six, the clean filters' share of the second filter, and the diode's low cut. The second clean filter always runs on the clean types, so 24 dB fades in already settled. Jumping between two modes every 64 frames under a tone gives no bigger step between frames than either mode held, for every pair of neighbours and the first and last, clean and driven, where an unsmoothed mix gave 14 times as much. **Curve:** `response_db` takes the mode: the ladder's is the weighted sum of `(1 + jw)^(4 - i)` over `(1 + jw)^4 + k`; `Dirty`'s mixes its two-poles in the z-plane; the diode's now includes its input high-pass, worked out exactly from its updates; and the clean 24 dB multiplies in the second filter. Every mode of every type matches what comes out within 0.5 dB, the diode's within 1 dB as before. **Saving:** `filter_mode` is saved by name with the track, and `mode:06` in a step. **Unchanged:** each type's first mode is bit-identical to before, measured by summing a second of output on main and here. **Bounds:** at full `Res` with a full-level saw and the cutoff swept eight times a second, every mode stays under 2 for the character filters and 4 for the clean ones, and the aliasing test passes in each mode that passes its tone. **Cost:** a throwaway release-mode test on an Apple M5, one channel: a clean `LP` 5.6 ns a sample against 5.2 on main, for the second filter that always runs, `Ladder` 52 against 51, and `Diode`, `Dirty` and the MS-20's unchanged at 62, 52 and 31–34, in any mode. **Left out:** Braids' "punch" on the SVF, where the low-pass output raises the cutoff and the damping for an 808-style ring that drops in pitch, needs a continuous amount and a cutoff worked out every sample, so it's noted on #50 for later rather than made a mode. Added on 2026-11-08.
73. **Every list is one picker, with categories as a tree one level deep, after snacks.nvim's picker and its explorer** (#221). It's `Picker` in `input/picker.rs`, which holds the keys as well as the state, and `views/picker.rs`; `Picker::grouped`, its groups' column and the old `Focus::Groups` are gone, and machines, filter types and modes, FX commands, `Dest`, a slot's effects and settings, scales, views, tracks, guides, gates, chains, phrases, uses, the shape panel's rows, kits, samples and audio devices all go through it. **Items:** a name, a meaning drawn dim beside it, a category, and optionally a wider choice and a preview, so labels are no longer padded by hand, and the view lines the meanings up. **Categories:** each item names its category, and a category opens and closes, as a folder in snacks' explorer does. The one holding the current value opens with the cursor on that value, which is marked with a dim `•`, and the others start folded, as the explorer follows the current file; with no current value everything starts folded on the first row. A category of one item is listed as the item, with its category dim at the right, so `Dest`'s Machine, which holds only `Pitch`, takes no extra key, as it didn't before. **Keys:** in the list, `j` `k` with counts, `gg`, `G` and `5G`, `C-n` `C-p` and the arrows move; `l` and `Enter` open a category and go into its first item, or choose; `h` closes the category, from inside it, going up to its heading, or on it, as the explorer's `h` does; `/` goes to the search. Any other letter starts a search with itself, so `K` then `pl` still finds `Plaits`, but a name starting with a list key, such as `ladder`, `hld` or `kick`, now needs `/` first, which four tests now do. Digits then `Enter` search for the number and choose the best match, so a chain or phrase is still chosen by number; a count before `Enter` would otherwise mean nothing. In the search every letter types, `C-n` `C-p` and the arrows move, and `Enter` chooses. `Esc` clears a search and goes back to the tree, on the item it found with its category opened; with no search it closes, and a pending count or `g` is let go first, as in Vim. **Opening:** snacks opens most pickers on their input and its explorer on its list, and so does this: the samples, the tracks and the FX commands open on their search, as they're found by name and their tests type straight away, and every other list opens on the list. **Looks:** spartan and one colour: the title is the list's name with `matching/total` dim beside it, categories are dim headings with an arrow, `▸` folded or `▾` open, and their count, items are plain with their meaning dim, aligned whether nested or not, and a search's matched letters are in the picker's own colour, teal, bold. The search line is `> query_`, or a dim `>` with any count in the list. **Layout:** after snacks' `select`, a small centred box sized to the widest item and up to 16 rows; after its `default`, a list with a preview pane beside it, 40 wide, for a list whose items have previews, which only kits have for now. The box stays the same size as it's searched or opened, as it's sized from every item. **The kit picker:** `Space 8`, free beside `Space 5` for samples and `Space 7` for audio, and `:kit`, list the kit folders' kits under the folder each is in, such as `Ableton/Drum Machines` as the importer writes them (decision 68), which is the kit file's `category` without reading every kit to find it, and kits at a folder's top at the top. It opens on the kit in use, which `:kit`, `Enter` and `L` remember. Beside it, the kit under the cursor: its name, how many sounds and its `source`, and each sound's label, name and an 8 by 1 waveform, drawn as decision 58's, folded once for each file and kept for as long as the picker is open, and sent as images only while that kit is shown. `Enter` makes it the kit `Space 5` lists first, as `:kit name`, and `L`, the wider `l`, puts its sounds on tracks as `:kit name all`, as one undo step; `L` is only a key in a list with a wider choice, so `Limit` still types elsewhere, and a hint at the bottom says so. **Samples:** `Space 5` lists files by name under the folder they're in, and the kit's sounds under `Kit name` first, and opens on the track's own sample in its folder. **Left out:** auditioning a kit's sound by moving onto it in the preview, as #221's comment asks, needs the engine to play a file that's on no track, which it can't yet, so the preview has no focus of its own; the sample list has no preview, as loading every file the cursor passes on the edit thread would stall on a long one. Added on 2026-11-08.
74. **`Tab` completes the command line from the parser's own tables, with a list of matches above it, and `Up` and `Down` go back through the commands run, kept in `history.ron`, after Vim's `wildmenu` and history** (#225). **One source:** `input/command.rs` now keeps every command in `COMMANDS`, each with its name, what it does, what it takes and its own parse function, and every `:set` key in `SETTINGS` the same way; `parse` looks the name up there, so a command that isn't in the table can't run, and completion lists exactly the table. The words themselves come from the same places the parser reads: `RENDER_FLAGS`, `KIT_ALL`, the model's `LAUNCH_WORDS`, built from the names `Launch::named` reads, `ROOTS` and `ScaleKind::ALL`. A test completes every command from its first letter and every key from `:set `, and parses every line that any command's or key's completion makes. **What completes:** names, with what each does dim beside it; `:set` keys, with `=` added; their values, `on`, `off`, the scales, lower case with dashes as the guide writes them, and the roots; each command's words, `:launch`, `:track add|del`, `:scale fit`, and `off` for `:reset` and `:tswing`; `:render`'s flags once its path is typed, a flag given not offered again and nothing after `--rows`, which takes a number; kits by name from every kit folder in `settings.ron`, merged, one folder at a time as paths are, or by path when the argument starts with `/`, `~` or `.`, and `all` after a name that is a kit; and paths, one folder at a time with a `/` after a folder, `~` as the home folder, hidden files left out unless a `.` is typed. Paths only list what the command takes: folders for `:e`, `:w` and `:new`, as a project is a folder, folders and sample files for `:sample`, WAVs for `:render`, and kit files for `:kit`. Matching is by prefix, ignoring case, as Vim's file names are on macOS, rather than the picker's fuzzy search, as what completes has to start with what's typed; the match's own case is put in. Nothing adds a space after a name, as in Vim. **Matches:** one match is put in place. Several fill in the part they all share, as Vim's `wildmode=longest:full` does, and list above the command line, through the picker (decision 73) with the first highlighted, or the last for `Shift-Tab`, as Vim's does. `Tab` and `Shift-Tab`, the arrows and `C-n` `C-p` move through it, going round at the ends. Moving puts nothing in the line, unlike Vim's `wildmenu`, so typing narrows the list from what's typed, keeping the highlight, and closes it once nothing matches or a new word starts; `Enter` puts the highlighted match in and closes the list, leaving the line open, so a folder can be gone into with another `Tab`, and a second `Enter` runs it; `Esc` closes the list and keeps what's typed. The list only opens on `Tab`, as Vim's does and the issue asks, rather than as the author's blink.cmp setup shows it while typing. **Looks:** a small box sitting on the status line, just above the command line, with its names under the word they finish, or under the file name in a path, after Vim's popup menu, and a space inside its right edge: the list's name, such as `Commands`, `Settings`, `Paths`, `Kits` or the command or key, bold with the number of matches dim, then the picker's own rows, up to 10 before it scrolls, and no search line, as the command line is its search. **History:** each line run with `Enter` is kept, even one that doesn't parse, so it can be put right with `Up`; `Esc` keeps nothing. `Up` goes back from the newest, and with something typed only through lines starting with it, which is what was typed when `Up` was first pressed, as Vim's does; `Down` comes forward and back to what was typed. Typing or `Backspace` starts a new search from the line as it is. A line run again moves to the end rather than being kept twice, and the last 100 are kept. It's in `input/command_history.rs`, saved as a RON list in `history.ron` in the config folder, beside `settings.ron` and `audio.ron`, for this machine rather than for a song, written each time a line is added; a file that won't read starts an empty history and says why on the status line. Tests keep no file. **Left out:** the command line still has no cursor of its own, so `Left` and `Right` do nothing and completion is always at the end of the line, as it was typed before. Added on 2026-11-08.
