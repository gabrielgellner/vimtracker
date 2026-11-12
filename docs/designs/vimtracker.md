# vimtracker

*   **Author:** gabrielgellner
*   **Status:** Built and in use, with milestones up to M29 on GitHub. The sections before
    Architecture, the Technical Plan and the milestone designs are the plan as it was written,
    kept for its reasoning.
*   **Date:** 2026-11-05
*   **As built:** [Architecture](#architecture) describes the code as it is now, and is kept current.
*   **Decisions:** numbered, and kept as history in [decisions.md](decisions.md). "Decision 94"
    anywhere here means number 94 there.

--------------------------------------------------------------------------------

## Overview

vimtracker is a music tracker that runs in a terminal, is written in Rust, and is edited
with a keyboard grammar modelled on Neovim. It takes ideas from the Dirtywave M8, Elektron
sequencers, the Polyend Tracker and Jeskola Buzz, but it is a new program. It does not port
their code, copy their appearance or read their files.

The first goal was to find out whether this could become an instrument the author wants to
pick up, and the author's jams answered it (G1, below). It has since grown from an M8 or
Polyend-sized tool into one at about an Octatrack's scale (decision 138): sampler and synth
machines, four effect slots, six mod slots a track (decision 165), groups, returns and a
master, used alongside Ableton Live rather than in place of it. It runs only in kitty and
Ghostty (decisions 50 and 56).

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
    graphics are drawn with text characters. No core feature depends on key release. *Since
    changed by decisions 50 and 56: it runs only in kitty and Ghostty, with the kitty keyboard
    protocol's releases and the kitty graphics protocol's images, and not inside tmux.*
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
    Synth engines and granular effects were non-goals for the first prototype, and are built now,
    under Direction.
*   Becoming a DAW, or an MPC-style workstation. The scale in mind is full songs at about Maschine's size.
*   Image graphics. Text-mode graphics come first. Kitty's image protocol remains possible
    later if text cannot do what is needed. *Since reversed: decision 56 requires the kitty
    graphics protocol, and every curve is an image.*
*   Live-coding, where running code generates the music during playback.
*   Official support for Linux or Windows. The code avoids depending on macOS without a reason,
    but only macOS is tested.

## Open Questions

*   **Key input:** Which keys do the author's terminal and tmux setup deliver? Issue 2 tests this.
    *Answered by M16: the kitty keyboard protocol, in kitty and Ghostty only (decision 50), and
    [terminal-notes.md](../terminal-notes.md).*
*   **Track capacity:** How many active tracks does the author's laptop sustain? Issue 23 measures it.
*   **Sweep smoothing:** Do sweeps sound stepped on a sustained sample? Issue 15 ends with a
    listening test, and the Audio path section states the fallback.
*   **FX persistence:** Mixer and filter values hold until changed, and note events apply once.
    Elektron parameter locks appear to revert after their step. This needs confirming by ear,
    particularly for sparse values.
*   **Braille graphics:** Can text-mode graphics deliver the desired feel, such as an old MPC
    screen or an M8-style scope? A small throwaway demo can answer this before Issue 20.
    *Answered: images replaced Braille (decisions 20 and 56).*

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
    with no change. `.` applies the same shape to another selection, with its own ends. `L` in place of
    `Enter` leaves the sweep live: see CUJ 4.

### CUJ 4: Adjust a live sweep, then take over by hand

*   **Given**: A phrase with a live sweep in its `CUT` column, marked in the grid.
*   **When**: The user moves away and returns, presses `gl` on the column and reshapes it, and later
    edits one of its fields by hand.
*   **Then**: Reshaping rerenders the column while the loop plays. The hand edit applies the sweep,
    leaving plain values, and the status line says so. `u` restores both the old value and the live
    sweep. Editing notes in the same phrase does not apply it. `Space g a` applies it explicitly.

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

## Architecture

This section describes the code as it is now, and is kept current: a pull request that changes the
structure updates it in the same PR, as well as appending its decision to
[the decision log](decisions.md). Each part ends with the decisions behind it, by their numbers
there. The [Technical Plan](#technical-plan) after it is the plan as first written, kept for its
reasoning. Where the two differ, this section is the one that matches the code.

### Summary

The program has one thread that owns and edits the song, and one audio callback that plays it. The
edit thread never changes a version the callback has. Each edit makes a new version that shares
everything unchanged with the old one, and sends it over. Sequencing happens inside the callback,
so screen redraws don't affect timing. Two helper threads read and analyse sample files, so a long
file doesn't hold up the screen, and the Roto-Control's link has a thread of its own (`vt-roto`).

```text
 Keyboard: presses, repeats and releases, through the kitty keyboard protocol
    |
    v
+--------------------------------+  versions (one slot),     +-----------------------------+
| Edit thread (vt-tui)           |  long buffers, commands,  | Audio callback              |
|                                |  one-off files            | (cpal's thread, vt-engine)  |
| App: modes, keys -> Actions    |-------------------------->|                             |
| history of song versions       |                           | Engine: transport,          |
| Published::after: bank entries |  old versions, buffers    |   sequencing, FX            |
|   compiled into Playables      |  and files, freed here    | sound tracks: machine,      |
| saving, autosave, :render      |<--------------------------|   trim, filters, inserts,   |
| ratatui drawing, kitty images  |  telemetry, scope,        |   sends                     |
|                                |  jam changes              | groups -> returns -> master |
|                                |<--------------------------|   -> limit                  |
+--------------------------------+                           +-----------------------------+
      |  ^              |  ^
      v  |              v  |
 analysis thread      loader thread
 (the bank's files)   (the explorer's file)
```

The same engine runs on the calling thread, with no sound device, for `render`, `bench` and `probe`
([Offline paths](#offline-paths)).

### Crates

| Crate | Owns | May depend on |
|---|---|---|
| `vt-model` | The song and everything in it, `Action`s and `edit`, undo history, the settings registry, the bank and `Playable`, the step text format, saving and loading, kits, sample decoding and resampling, and sample analysis | No audio device, terminal or other vimtracker crate: `serde`, `ron`, `hound`, `symphonia`, `rubato`, `thiserror` and `xxhash-rust` |
| `vt-engine` | `Published`, the queues to and from the callback (`link.rs`), transport and sequencing (`engine.rs`), each track's playing state, voices and the sample reader, the preview voices, the DSP (`dsp/`, [below](#dsp-layout)), the limiter, the slowdown check's kernel (`slowdown.rs`), and rendering offline | `vt-model`, with `rtrb` for the queues and `hound` to write WAVs |
| `vt-roto` | Melbourne Instruments' Roto-Control: its SysEx, CC and serial protocols, typed each way (`sysex.rs`, `cc.rs`, `serial.rs`), the handshake (`session.rs`), the link's thread with finding the ports and reconnecting (`link.rs`, `transport.rs`), devices in PLUGIN mode, live or stored, with their buttons' lights, the Roto's track list and what changed sent back (`live.rs`), MIX mode's banks, track focus, lights and meters (`mix.rs`), a device's pages of eight knobs and eight buttons as data (`layout.rs`), vimtracker's own stored setups, written only under its own hashes and only when they change (`store.rs`), a fake Roto in memory (`fake.rs`), and `roto-probe` | No other vimtracker crate: `midir` for MIDI, `serialport` (without `libudev`) for the serial port, `thiserror` and `xxhash-rust` |
| `vt-tui` | The `vimtracker` program and its subcommands, the terminal and the audio device, `App` and its keys, the views, kitty graphics, the helper threads, kits from Ableton Live, the mouse wheel as a knob (`wheel.rs`), the Roto's setting, every kind of device's layout on it (`roto/layouts.rs`), its devices as the Roto shows them, and its knobs' and buttons' moves as edits (`roto.rs`, `roto/devices.rs`, `roto/edits.rs`), the song as MIX mode's strips (`roto/mix.rs`), the faders as the steps automate them and a hand takes them over (`automating.rs`), and `probe` | `vt-model`, `vt-engine` and `vt-roto`, with `ratatui`, `cpal`, `clap` and the rest |

*   **One direction.** `vt-tui` uses `vt-engine`, which uses `vt-model`, and never the other way. The
    model doesn't know how it's played or shown, so its tests need neither, and the engine never
    draws or reads a terminal. `vt-roto` knows nothing of songs: it takes devices as names, feels,
    positions and colours, and pages of eight knobs and buttons naming settings of a type it
    doesn't know. `vt-tui`'s `roto/layouts.rs` declares each kind of device's pages, by its
    settings' keys, `roto/devices.rs` makes the Roto's devices from those layouts and the settings
    registry alone, never the track page's grid, and `roto/edits.rs` turns a knob's position or a
    button's press back into the `Action` a key would give ([`roto-control.md`](roto-control.md)).
*   **What the interface borrows from the engine.** Besides `Published` and the queues, `vt-tui`
    calls the engine's DSP to draw what a sound does: the track filter's response, a compressor's
    static curve and an EQ's response come from `dsp::filter`, `dsp::compressor` and `dsp::eq`, so a
    drawing is the sound's own maths. `dsp.rs` re-exports those at their old paths.
*   **A new setting needs no new interface.** Every setting a step can lock or a modulator can move
    is registered once, in `vt-model`'s `settings.rs`, with the key songs store it by, a name, a
    label for the grid, a kind and a default. The track page, `K`, the grid and the engine all read
    that table.

**Decisions:** 29, 30, 59, 60, 158, 159 and 161, the [M14](#m14-design-one-list-of-settings) and
[M14b](#m14b-design-kinds-of-setting) designs, and the three crates of the
[Detailed Implementation Plan](#detailed-implementation-plan).

### Threads and versions

*   **The edit thread** runs `run::main_loop` once a frame, every `FRAME_MS` (33 ms): `App::tick`
    frees what came back and reads the newest report, autosave runs if it's due, `Graphics::sync`
    sends the images the screen is about to show, ratatui draws, `Graphics::place` places the images
    that are placed directly, and then keys are read until the next frame. Saving, `:render` and
    loading samples at a new rate happen here too.
*   **The audio callback** is `cpal`'s output thread, in `audio.rs`. It calls `Realtime::process` in
    blocks of up to `MAX_BLOCK` frames, into scratch made when the stream starts, clamps the output
    at full level, and counts its load, late blocks and latency in `AudioStats`' atomics for the
    status line. Four times a second of audio, after its block, it times a small fixed kernel
    (`vt-engine`'s `slowdown.rs`) against the kernel's time at full speed, and stores how many
    times slower it ran in `AudioStats::slowdown`; the main loop smooths that in `SlowdownReadout`
    for the leaf before the CPU figure, and asks the system every 2 s whether Low Power Mode is on
    or the Mac is hot (`PowerState`, through `objc2-foundation`), never from the callback. A
    change on the audio page builds a new engine at the new rate and connects it afresh, keeping
    the song and its undo.
*   **The full-speed thread** (`audio::measure_full_speed`) times the kernel for 100 ms as the
    program starts, while the song loads, at the user-interactive QoS that asks for a performance
    core, and keeps the fastest time it has ever seen in `full-speed.txt` in the cache folder, so
    a start in Low Power Mode or on a busy machine still has full speed to go by.
*   **The analysis thread** (`analysing.rs`) works out each of the bank's files in turn, as it joins
    the bank or a project opens: its peaks, its level and where its sound is, read from the cache
    folder by content hash if they're there.
*   **The loader thread** (`loading.rs`) reads the file the explorer's cursor is on, only the newest
    asked for, so its preview and the sound it plays don't hold up the screen.
*   **The Roto's thread** (`vt-roto`'s `link.rs`, named `roto`) watches every second for the
    Roto-Control's MIDI ports, opens them, does the handshake and answers its pings, and turns what
    it sends into events; midir's callback only copies bytes to it. The audio page's Roto setting
    starts it (off unless turned on, kept in `roto.ron`), and `App::tick` takes its events each
    frame and answers them from `vt-roto`'s `PluginView`, with the devices `roto/devices.rs` makes
    of the song, or in MIX mode its `MixView`, with the strips `roto/mix.rs` makes; turning it off
    sends `DAW STARTED` and stops it. Only the vimtracker holding `roto.lock` in the cache folder
    starts it. `vimtracker roto-probe` starts
    one of its own. It never touches the audio thread ([`roto-control.md`](roto-control.md) §7).
*   **The Roto's setups thread** (`roto.rs`, named `roto setups`) opens the Roto's serial port
    each time it connects, checks vimtracker's own stored setups and writes those missing or
    changed (`vt-roto`'s `store.rs`), and closes the port; `App::tick` takes its answer, and the
    devices with buttons are shown stored from then on. Which version of each is on the Roto is
    kept in `roto-setups.ron` in the cache folder.

**How a version reaches the callback** (`link.rs`):

1.  An `Action` goes through `vt_model::action::edit`, the only path that changes a song, which
    returns a new `Arc<Song>`. Chains and phrases are each an `Arc`, so the new version shares
    every one it didn't change. `History` keeps versions for undo, up to `HISTORY_LIMIT`, and
    `App::apply` commits the new one and calls `App::publish`.
2.  `App::publish` builds a `Published` from the song, the sample pool and the last version, which
    compiles the bank ([What the engine sees](#what-the-engine-sees)), and gives it to
    `Handle::publish`.
3.  The queue to the callback holds one version, `PUBLISH_QUEUE`. `rtrb` can't take back a queued
    item, so the newest unsent version waits in a slot on the edit thread, and every edit and every
    frame's `Handle::pump` tries to send it. A newer version replaces the one waiting, which is
    freed there, so the last edit always reaches audio.
4.  At the start of each block, the callback takes a version only if the return queue has room,
    swaps it in with `Engine::swap`, and pushes the one it replaces back. It never drops one. The
    edit thread empties the return queue each frame, and the versions are freed there.
5.  **Long buffers**, for the effects with memory such as delays and reverbs, are made on the edit
    thread for what a version needs (`MemoryCount`) and sent ahead of it. Spare ones are asked back
    with `Command::ReleaseMemory`, and come back on a queue of their own to be freed.
6.  **One-off files**, such as the explorer's file or a slice auditioned on the sample page, go over
    as an `Arc<SampleData>` with an optional `Span`, and come back on the `heard` queue.

**Commands** carry what isn't an edit: transport, previews and held notes, auditions, mute and
solo, fill, realigns, launches, the scale, which track to watch, and a hand taking a fader over
from the steps or giving it back (`TakeOver` and `GiveBack`, decision 174). The engine resolves each
against its own state, so two quick presses can't act on an out-of-date view of it. **Telemetry**
comes back each block: where playback is, each track's place and levels, each track's faders as
they're heard (`Faders`: where `LEV`, `PAN`, `SNA` and `SNB` put them, what `VOL` and modulation make
of them, and which the steps drive and a hand has taken over, from `TrackState::faders`), the watched
track's live values, and up to four sample playheads. The live values carry the watched track's peak right after its trim
(`LiveValues::after_trim`), which `TrackState` keeps as it trims each frame and
`Engine::take_after_trim` takes, for the Output device's meter, and where each mod slot running as
an LFO is in its wave (`LiveValues::mods`, a phase and a value each, which `TrackState::modulate`
keeps as it works the slots out, and none while stopped), for the Mod page's dots. `Handle::latest` merges every
report since the last frame, so a meter doesn't miss a peak. The scope's sound comes through a ring of its own, which drops what doesn't
fit, and jam changes through another.

**Real-time safety.** Nothing in `Realtime::process` allocates, frees, locks or does I/O. Each
track's playing state for all `MAX_TRACKS`, the preview voices and the effect slots are made ahead of
time. In `vt-engine`'s test builds, `rt_guard` is the global allocator and counts every allocation
and free on a thread while it's armed, and the tests in `link.rs` run whole blocks under `forbidden`, swapping
versions, taking long buffers and filling the scope among them. `cargo test --release` runs them
against optimised code too.

**Work skipped.** Playing from the top resets only the track states the song's versions have had a
track in since the last reset, `Engine::used`; the rest keep the empty track's settings that reset
gave them (decision 147). What a block skips as it plays is under
[The engine's block](#the-engines-block).

**Samples.** The pool only grows during a session, and sample IDs stay stable, so a voice keeps
reading its sample until its note ends, and a replaced sample stays in memory until the project
reloads.

**Decisions:** 33, 54, 57, 75, 98, 114, 147, 150, 153 and 158, the
[M10 design](#m10-design-tracks-that-process)'s memory, and the first of the
[Known Gaps From Review](#known-gaps-from-review).

### Song model

```text
Song                               song.ron, refused unless format_version is FORMAT_VERSION
 |- name, millibpm, seed, scale, limiter, launch, auto_jump, skip
 |- groove, grooves[]              the project's groove, and up to 8 of its own
 |- bank                           01..FF -> BankEntry: label, name, edit
 |   '- SampleEdit                 timeline (one whole file for now), points, play
 |       |- Points                 start, end, loop, slices[], next_id, marks{a..z}
 |       '- PlayParams             gain, tune, cents, reverse, fades, normalize, snap_zero
 |- tracks[]                       sound tracks, then the groups G1 to G4, Return A, Return B
 |   |                             and Master
 |   |- kind, name, mute, solo     kind: Sound, Group(n), ReturnA, ReturnB or Master
 |   |- machine                    Sampler, or Plaits(voice)
 |   |- settings{id}, fine{id}     machine settings changed from their defaults
 |   |- filter, filter_mode, cutoff, resonance, drive, low_cut, high_cut
 |   |- inserts[4], bypassed[4]    an effect and its settings in each slot
 |   |- level, trim, pan, send_a, send_b
 |   |- pitch, velocity_delta, choke, gate_length, speed, reset, glide, groove
 |   |- group                      the group a sound track plays through, or none
 |   |- accumulation, arpeggio     how ACU and ARP play on it
 |   |- mods[6]                    ModSlot: kind, destination, depth, switch, polarity, each type's
 |   |                             own settings (adsr, lfo), kept while another type plays
 |   |- chains{id}                 Chain: entries[(phrase, transpose)], label
 |   '- phrases{id}                Phrase: steps[], start, length, lanes[3], order, generators[]
 |       |- Step                   note, velocity, nudge, gate, fx[]
 |       |   '- FxSlot             command, value (coarse and fine), mark
 |       '- Generator              a live shape: column, span (rows or the lane), Shape, and the
 |                                 column's cells from under it, saved as steps
 '- sections[]                     Section: chains[one per track], length, repeat, start, reset
```

*   **Tracks.** Up to `MAX_SOUND_TRACKS` (64) sound tracks, then the fixed tracks every song has:
    `MAX_GROUPS` (4) groups, `G1` to `G4`, the returns `A` and `B`, which start with a delay and a
    reverb in their first slot, and the master `M`. Every kind has steps, inserts and a page, and
    only a sound track has a machine. A sound track's level scales what it sends, and `send_a` and
    `send_b` feed the returns. A sound track whose `group` is set plays into that group, which
    plays the sum through its own slots, level and pan, and sends from there; the other sound
    tracks, the groups and the returns feed the master. A group can't be in a group, so the order
    stays fixed. `ensure_fixed_tracks` puts in any fixed track a song lacks, so raising
    `MAX_GROUPS` keeps saved songs loading. `Song::group_members` lists the tracks a group sums.
    A compressor's, Warps' or a gate's `Source` names a sound track by its number, a return, or a
    group, numbered after the returns so theirs stay as songs saved them; while one hears a group,
    the group's tracks play into it even while it's muted or soloed out, as a muted track a
    compressor hears plays on silently.
*   **Machines and their settings.** A `Machine` is the `Sampler`, which plays the bank entry its
    `Sample` setting or a step's `SMP` chooses, or `Plaits(voice)`, one of the synth voices. The
    track's own controls are fields. A machine's settings are a map by `SettingId`, saved by key,
    holding only what's changed from its default, so switching voice and back finds the first as it
    was. A continuous value has a fine byte as well, in `fine`.
*   **Inserts.** `INSERTS` (4) slots in order after the filters, each an `Insert` holding its
    effect and that effect's settings, and each can be bypassed.
*   **The bank.** Numbered `01` to `FF`, with `00` as no sample, after Elektron's project pool. A
    `BankEntry` is metadata over audio that's never written: its `SampleEdit` names the file by its
    path and content hash, and holds its `Points` and `PlayParams`, in frames at the file's own
    rate. Slices are numbered by where they start, as Vim numbers lines, each with an id that's never
    used again and settings of its own where they differ from the entry's. The letters `a` to `z`
    mark slices by id, so `SLI 'a` follows its slice as others come and go.
*   **Sections, chains and phrases.** A section is a row of the song view, with one chain per track,
    a length, how many times it repeats, what it does when it starts, and a reset. Each track owns
    its chains and phrases, numbered within it and shared only within it. A chain is a list of
    phrases, each with a transpose. A phrase stores more steps than it plays, like a clip: a loop of
    `length` steps from `start`, a lane for any FX column that loops on its own, and the order its
    rows play in.
*   **Steps and FX slots.** A step has a note or `OFF`, a velocity, a nudge in 24ths of a step, a
    gate of pulses and a length, and `FX_SLOTS` (3) FX slots, and is saved as one line of text. A
    slot holds an event (`DEL`, `KIL`, `OFF`, `RET`, `CHA`), `CND`, `ACU`, `ARP`, or a lock of any
    registered setting, `FxCommand::Set`. Its value is a coarse and a fine byte, with a lock, hold or
    glide mark.
*   **Mod slots** (`modulation.rs`, decision 165). Each track has `MOD_SLOTS` (6) slots on its
    Mod page, each a `ModSlot` of a type, `ModKind::Adsr`, `Ahd`, `Drum`, `Lpg` or `Lfo`, with a destination
    setting, a depth, a switch and a polarity (`Uni` or `Bi`, its type's until one is chosen),
    which every type shares, and each type's own settings, which a
    slot keeps while another type plays. The defaults are three ADSRs, then three LFOs. An LFO's
    rate is a note division or bars, a count of steps, or hertz, and its mode `Free` or `Trig`. An
    ADSR is in milliseconds, with a reset and a curve (`Exp`, `Lin` or `Snap`, its mode), and an
    AHD the same with a hold of a time or `Note` in place of a sustain and release, and a drum
    envelope a steep peak from full onto a body, struck one to eight times, then a decay, and a low-pass gate Plaits' vactrol solved in closed
    form, which also closes the track's `Cut` by its `Colour`. The first
    envelope-type slot on `Vol` shapes the notes (`note_shape_slot`): its `ModSlot::envelope`, an
    `envelope::Envelope` of any of these types, is what each voice keeps as its `shape`, and a
    one-shot (an AHD with a timed hold, a drum or a gate) isn't released when its note ends. Slots work out in order, so a slot's destination can be the depth or phase
    of a slot after it (`SettingId::for_mod`), never one before.
*   **Accumulators and arpeggios.** A step's `ACU` and `ARP` say how far, and the track's
    `accumulation` and `arpeggio` say how they move and when they start again.
*   **Grooves and time.** Tempo is in thousandths of a BPM. The project has a groove, a track can have
    its own, and a step's `GRV` can choose one. A groove is up to 16 pushes, each step's velocity and
    a humanise, and swing is a groove of two steps. A track's `speed` is a note division, its
    `reset` restarts its chain every so many of the song's steps, and the song's `launch` says when
    launched chains start.
*   **Saving.** A project is a folder holding `song.ron`. Loading reads `format_version` first and
    refuses any other version, with no migration. What belongs to the machine rather than the song
    is in the config folder, `$XDG_CONFIG_HOME/vimtracker` or `~/.config/vimtracker`: where kits
    and samples are (`settings.ron`, `config.rs`), the audio choice (`audio.ron`), the command
    history (`history.ron`) and the explorer's last folders (`places.ron`). The cache folder,
    `$XDG_CACHE_HOME/vimtracker` or `~/Library/Caches/vimtracker` on macOS, holds the sample
    analyses and `full-speed.txt`. Autosave (`autosave.rs`) writes `song.autosave.ron` beside
    `song.ron` every `AUTOSAVE_SECONDS`, and offers it back after a crash.

**Decisions:** 2, 5, 8, 10, 11, 15, 17, 29, 30, 37, 70, 78 to 82, 84 to 88, 91, 94, 95, 105, 111,
123, 126, 136 and 138;
the [M9](#m9-design-fx-inserts), [M10](#m10-design-tracks-that-process),
[M12](#m12-design-lanes) and [M13](#m13-design-pulses) designs, and [song-flow.md](song-flow.md) for
sections; and the [FX interpreter](#fx-interpreter) for LFOs and envelopes.

### What the engine sees

`Published` is one complete state as the callback plays it: the `Arc<Song>`, the `Arc<SamplePool>`
of decoded audio at the program's rate, and `bank`, 256 places of `Option<Arc<Playable>>`, one for
each bank number. It also carries what's worked out once rather than every block: how far ahead
steps are worked out so a groove or a nudge can play them early (`lead` and `groove_lead`), and
flags that let the engine skip looking for nudges, `GRV`, `RATE` and other speeds in a song that has
none.

A `Playable` is one bank entry ready to play: its `SampleId` and frames, a `Span` for each slice in
`SLI`'s order, or one for the whole entry, the whole entry's span for the sample page, its start
and end, and the slice number each mark's letter names. A `Span` is in the pool's frames at the
program's rate: its start and end, its loop and crossfade, whether it's reversed, its fades, and
its gain, speed and pan. A slice's own settings are already in place of the entry's, and the gain
already takes in what normalising needs, from the file's analysis.

**Why the engine never sees `SampleEdit`.** An entry is edited in its file's own frames and rate,
by path and content hash, with settings a slice may or may not override. Working that out takes
strings, the pool's lookups and the analysis, which the callback can't do. So `Published::after`
compiles each entry into a `Playable` on the edit thread, and keeps the last version's for each
entry whose edit, samples and peak haven't changed, so an edit elsewhere costs nothing. A new field
in `SampleEdit` changes only how a span is worked out, and the engine plays spans as it always has;
a slice's settings cost the voice nothing; and `SLI 'a` is looked up rather than searched for.
`vt-engine` names nothing from `vt_model::bank`.

**Decisions:** 94, 97, 98, 105 and 111.

### The engine's block

`Engine::process` (`engine.rs`) plays a block in runs. Each run goes up to the next thing that
happens at a frame: the song's next step, a track's own step at its `Speed`, a delayed, cut,
release, gate, retrigger or arpeggio event, or the mod slots' update every `LFO_UPDATE_FRAMES`
(32). So a step lands on its frame, and nothing inside a run asks whether one
is due. `Engine::render` then plays the run's frames.

*   **Who plays.** A note or a preview only starts between runs, so a track that's silent as a run
    starts, with no voice, preview or slot still ringing, is skipped for the whole run, and the
    rest are listed once, with where each goes: its group, or the master.
*   **A sound track's frame** is its voice (`voice.rs`: the sampler, reading its span through
    `reader.rs`, or a Plaits voice), the tail of the note before, fading out over
    `DECLICK_FRAMES`, and any preview voice on it (`preview.rs`), through `TrackState::shape`:
    the trim, after which the Output device's meter reads its peak (`after_trim`); the filter and
    its drive; the low and high cuts; the slots in order (`through_slots`); then the level and
    pan, and the sends, which follow the level.
*   **Groups, returns and the master.** A grouped track's frame goes to its group, which plays the
    sum through its own trim, slots, level and pan and sends from there (`process_group`); each
    return plays what's sent to it (`process_return`); and the master plays the sum through its
    trim, slots and level (`process_master`). Last, `mixer::master` turns anything that isn't a
    number into silence and, with the song's `limiter` on, clips at full level; the callback
    clamps what it gives the device at full level either way.
*   **Sidechains.** Each track's frame after its slots, before its level and mute, is kept in
    `Keys` for a compressor, Warps or a gate whose `Source` names it. A track later in the order is
    heard a frame late, far too little to hear.
*   **One side for both** (decision 147). `through_slots` looks once whether a frame's sides are the
    same, and each slot passes that on. An effect says what it does with such a sound
    (`effects::distort::Mono`): `OneSide` works the left side out and gives it to both, keeping the
    right's state in `Sides` to take over the moment they differ; `Linked`, such as a compressor's
    linked detector, passes "the same" on without looking; and `Stereo` works out both and the
    slot looks again.
*   **Smoothing.** Every continuous setting moves through a one-pole `Smoothed`
    (`blocks/smoother.rs`), over 10 ms (`SMOOTHING_MS`) for a change by hand and 1 ms
    (`FAST_SMOOTHING_MS`) while modulation moves it, so nothing clicks.
*   **Mod slots** (`track_state.rs`). `TrackState::modulate` works a track's six slots out in
    order, every `LFO_UPDATE_FRAMES`, through a fixed `slots: [SlotRun; MOD_SLOTS]`: each reads its
    held depth and phase plus what earlier slots added to them, maps its output by its held
    polarity (`Polarity::of`, which leaves its type's own exactly as it was), and adds to its destination, an
    envelope's through the fast smoothers and an LFO's into `offsets`, through the slow ones. A
    slot keeps every type's run going (its envelope's stage, its cycles' start), so a type change
    picks the new type up as though it had always been there, and the jump between what the old
    type last added and what the new one adds glides out over 5 ms (`SlotRun::add`). Nothing in it
    allocates.
*   **Timing** (`timing.rs`). For `bench --tracks`, a `Stopwatch` laps between the parts, the
    sequencing, each track's voice and the rest of its work, each slot, the mixing and the limiter,
    so they add up to the block. Unless asked, a lap is a check of an empty option.

**Decisions:** 36, 123, 136, 142 and 147, and the [M10 design](#m10-design-tracks-that-process) for
the routing.

### DSP layout

`crates/vt-engine/src/dsp/` is grouped by what each part does to the sound (#322, 2026-11-10), one
module per file, each keeping its port's licence notice:

| Folder | Holds | Modules |
|---|---|---|
| `voices/` | What makes a track's sound from its notes, and the parts only voices use | `acid`, `additive`, `chiptune`, `chord`, `chord_bank`, `drums`, `fm`, `grain`, `kick`, `modal`, `noise`, `particle`, `phase_distortion`, `six_op`, `string_model`, `swarm`, `variable_shape`, `virtual_analog`, `virtual_analog_vcf`, `wave_terrain`, `waveshaping`, `waveshaping_tables`, `wavetable`, and `tables/` with Plaits' waves and DX7 patches |
| `filters/` | The track's filter and the character filters it switches between, and the slot effects that are filters | `filter`, `diode_filter`, `dirty_filter`, `korg35`, `comb`, `lpg`, `vowel` |
| `effects/` | The other slot effects, and what plays a slot | `chorus`, `delay`, `distort` (also `InsertState`, which plays whichever effect a slot has, and `Mono`, what each effect says it does with a sound the same on both sides, decision 147), `flanger`, `freq_shifter`, `granular`, `memory` (the pool of long buffers effects take), `pedal` (`Distort`'s guitar pedals, `Overdrive` and `Distortion`), `phaser`, `pitch_shifter`, `plate`, `resonator`, `reverb`, `tape`, `warps`, and `tables/` with Warps' fold |
| `dynamics/` | The mixing tools | `compressor` (the `Comp` slot, an enum of its types, the `Auto` and `Makeup` they share, and `Clean`), `console` (its `Console` type, fitted to Bus FORCE), `glue` (its `Glue` type, fitted to Live's Glue Compressor, and the `Peak Clip` the slot runs after its mix), `limiter`, `eq` |
| `blocks/` | Small parts the others share, which use nothing else in `dsp` | `oversampler`, `sides` (a state for each side, worked out once for both while a sound is the same on both, as every `OneSide` effect keeps its own), `smoother`, `sine_table`, `plaits_parts` |

A new voice, filter, effect or dynamics tool goes in its folder. A part that two folders would
share, such as the antiderivative antialiasing, 4x oversampling and state variable filter of #260,
goes in `blocks/`. A binary table sits in a `tables/` beside the modules that read it, so their
`include_bytes!` paths stay short. `dsp.rs` re-exports the modules used outside `dsp` at their old
paths, such as `dsp::filter` and `dsp::compressor`, so the rest of the engine and `vt-tui` name them
as before.

Where the unclear ones went: `comb`, `lpg` and `vowel` are slot effects, but filters by what they do,
as Vital files its comb and formant filters. `resonator` is the `Reso` slot effect's code; the `Modal`
voice borrows its bank of modes. `memory` names every effect with a long buffer, so it sits beside
them in `effects/` rather than under them in `blocks/`. `grain` (Plaits' formant voice) and `noise`
(its clocked noise) are voices; `granular`, Beads' effect, is an effect. `eq` sits with the
compressor and limiter as a mixing tool, as a channel strip has them. Each of the compressor's types
(#338) has a module of its own beside `compressor`, which matches the slot's type each frame, so
`VCA`, `FET` and `Diode` each add a module and an arm. `compressor` applies the `Auto` and `Makeup`
every type shares after the type's own module, so a new type has both with nothing of its own
(decision 132). `plaits_parts` holds Plaits'
and stmlib's shared filters and helpers, used by voices, `lpg`, `warps` and `resonator`, so it's a
block. Two soft clips, `drums::soft_clip` and `filter::soft_clip`, stay
where they were written and are borrowed across folders; #260 may move shared shapers into `blocks/`.

### Interface

**`App`** (`app.rs`) is the program's state and what each key does. It needs no terminal, so tests
drive it directly. It holds the history, the engine's `Handle`, the last `Published`, the sample
pool, the view and the mode, the cursor, the picker, the sample page and much more: over 120
fields, and 33 files besides `app.rs` add an `impl App` block each, such as `track_page.rs`,
`banking.rs`, `slicing.rs`, `shaping.rs`, `picking.rs`, `recording.rs` (live recording, `Space r`),
`project_page.rs`, `audio_page.rs` and `sample_page.rs`.

**Pages and modes** (`input/mode.rs`). A `View` is a page: the song view (`Space 1`), the chain view
(`Space 2`), the phrase view (`Space 3`), the track page (`Space 4`) with its Sound, Mod and FX pages
on `H` and `L`, the mixer (`Space 6`), the audio page (`Space 7`), the project page (`Space 9`) and the
sample page (`Space 0`). `Space 5` and `Space 8` choose a sample and a kit in the picker. A `Mode` is
Normal, operator-pending, the two Visual modes, Insert, Replace, the command line, the shape panel,
a jump, the keyboard playing, the picker, a guide or help. The chain, track, mixer and song modes
only name Normal's keys on those pages, for the binding table.

**Keys** (`input/`, and `grammar.rs` beside it):

*   **`keys.rs`:** `Key`, made from crossterm's events, and `KEYBOARD_FLAGS`, the kitty keyboard
    protocol's flags, pushed from start to quitting, so every key comes with whether it's a press, a
    repeat or a release. `App::key_event` sends each to `App::key`, `App::key_repeated` or
    `App::key_released`. A press of the key that's already down counts as a repeat, as kitty on macOS
    sends some repeats as presses.
*   **`wheel.rs`** (beside `input/`): the mouse wheel as a knob on the cursor's setting, while its
    setting, `wheel.ron` in the config folder, is on; the main loop asks the terminal for mouse
    reports only then. `App::mouse_event` reads both axes as one knob, flipped to follow the
    fingers under macOS's natural scrolling (`wheel::natural_scrolling`, read at start) and by
    the setting's `reversed`. A gesture drops a tick the other way that comes too soon, as a
    bounce, and keeps a decaying speed that sizes each tick by `wheel::tick_size`, the constants
    all in `wheel::feel`. Ticks are owed, and `App::wheel_frame`, each frame, or any key, makes
    what's owed as one press of `=` `-` `+` `_`, or the sample page's Cmd-l and Cmd-h, with a
    count, through `App::press`, in whatever mode binds them, so every page and a locked
    selection take it as they take the key. Where the setting has a finer step than `=`, the
    cursor is put on its fine part for the press and back after. A gesture's presses are squashed
    into one undo step (`History::squash_to`, as the Roto's touch to let-go), and in a selection
    `Adjustments::join_from` keeps its running total in step (decisions 162 and 163).
*   **`bindings.rs`:** the binding table, one list of mode, key sequence, `Cmd` and description. The
    which-key popup and `Space k` are generated from it. `CMD_ADJUSTS` is the one rule for Cmd
    with `h` `j` `k` `l`: `bindings` gives every mode that binds `=` `-` `+` `_` their Cmd keys
    beside them, unless the mode binds one itself, and `adjusting_key` turns them into those keys
    for the shape panel, which reads its own (decision 148).
*   **`grammar.rs`:** counts, registers, operators, Visual modes and `.`, on top of the table. A
    command ends in an `Action` through `App::apply`, or a `Command` to the engine. A selection
    stays through what moves or fills its values, and `Adjustments` counts those for the status
    line's running total, going back and forward with undo and redo; `SelectionPlace` is a
    selection to come back to after `r` (decision 148).
*   **`picker.rs`:** one picker for every list, with categories as a tree one level deep, which is
    also the file explorer. `picking.rs`, `explorer.rs` and `exploring.rs` open it and act on what it
    chooses.
*   **The command line:** `command.rs` parses a `:` line into a `Line`, `completion.rs` completes it
    with `Tab` from the parser's own tables, and `command_history.rs` keeps the lines run, in
    `history.ron`. `command_line.rs` has the line's keys.

**Drawing** (`views/`). `run::draw_screen` lays out the page, the meter's column on the right with
the scope at its top, and the status line, then draws the current page and whatever is open over
it. Each page is a module with a `draw(frame, area, &App)`. `views/widgets/` holds what pages are
built from, so a new page uses them rather than working out its layout again:

*   `table`: `Columns`, a page's fixed column widths and the room between them, for ratatui
    `Table`s and for `Layout` cells that hold images.
*   `bar`: `ValueBar`, a value's bar. `setting`: a setting's name and value. `meter`: a meter's
    cells.
*   `image`: an image's placeholder rows, and the faint box round the track page's curves.
*   `hint`: a window's hint along its border. `window_border`: the rounded border every window has.
*   `toggle`: the switch glyph pair, `nf-fa-toggle_on` green and `nf-fa-toggle_off` faint, for
    everything that's on or off (decision 140): a device's switch, a two-state setting's value on
    the FX, Mod and sample pages, and a hint's switches.
*   `group`: a group's colour, from `theme::group`, the tracks it sums as runs such as `01-04 07`,
    the `G1` mark a grouped track carries, and whether a track's mute or solo is its own or its
    group's, for the mixer, the song view, the track page and the phrase and chain views.
*   **A rack's devices** (decision 139, [`fx-page.md`](fx-page.md)), page-agnostic, so the Sound
    and Mod pages can take them as they are ([`track-pages.md`](track-pages.md)):
    *   `device`: `Device`, a device's rounded frame 9 rows tall (`DEVICE_HEIGHT`), its title on
        the top border (its identity, dim, its switch's `toggle` or the room for one, and its
        title controls, bold while it has the cursor), its brighter frame while it has the cursor, and the areas of its
        settings and its panel; and `rack_areas`, where each device of a rack goes from its band
        and side, full width or a half with `HALF_GAP` between, going by whole bands to keep the
        cursor's in view where fewer fit.
    *   `grid`: `SettingsGrid`, up to four columns of three rows with holes, at `CellWidths`, read
        through the `CellSource` trait (a setting's name, value and `ValueBar`), so it knows
        nothing of `App`; and `faded`, for a device switched out. `CellWidths::FX` has bars, and
        `CellWidths::COMPACT` none, for a half-width device whose panel shows what its values do.
    *   `panel`: `GraphicPanel`, faint square boxes side by side, with an image's placeholders or a
        meter's cells inside and a caption in the bottom border, such as `GR`.

**The track page** (`track_page.rs` and `track_page/`, decision 145) is the first page that's a
component (#320). `App::track_page` is a `TrackPage`: the page showing (`Page`: Sound, Mod or FX),
the cursor's row and field, the field `j` and `k` aim for, and whether it's on a value's fine
digits. Its methods are given the cursor's track (`App::track_here`) and read no `App`:
`TrackPage::rows`, `place`, `control`, `devices` and `device_cursor` say where the cursor is and on
what, and `TrackPage::key` runs the page's own keys, the moves, `{` and `}` and the pages either
side, returning an `Outcome`: done, refused with a reason for the status line, or passed back.
`App::run_track_page` settles the cursor, hands each key to `TrackPage::key`, and runs what's passed
back itself, the edits, which change the song: `editing.rs` (`=`, `-`, the fine digits and `x`),
`pickers.rs` (`K`) and `typing.rs` (`i`), with `b`, `dd` and `p` in `fx.rs`. `App::track_control`
is the control under the cursor on the track page or the mixer, whose own cursor is in `mixer.rs`.

*   **Each page's settings in its own file:** `sound.rs` has the Sound page's devices on a sound
    track and the fixed tracks' Outputs, `modulation.rs` the Mod page's devices, and
    `fx.rs` the slots; `controls.rs` has `Control`, what each setting is called and holds on a
    track.
*   **Racks** (`device.rs`). `Page::devices` asks each page's file for its `DeviceSpec`s: the FX
    page a slot each (`DeviceSpec::slot`), and the Mod page each mod slot of the left column, then
    the one three after it beside it, `Mod 1`, `Mod 4`, `Mod 2` and so on, each with its type's
    grid and a title of its type, its mode and its destination, then the accumulator and the
    arpeggio (decisions 151 and 165). A `DeviceSpec` is its
    identity, its title controls over the grid's first columns, its grid of `Control`s and its
    width, full or half, built by `DeviceSpec::new` from a model's `Grid` (`vt-model`'s
    `grid.rs`), as `Insert::grid` is. `rack_places` lays a page's devices out in the order they're
    read: a full-width device has a band to itself, and two half-width ones in a row share one,
    left and right. A page with devices is a rack: its cursor rows are the devices' rows one after
    another, each `DEVICE_ROWS` long, with holes `h`, `l`, `0` and `$` step over and `j` and `k`
    land beside, and `TrackPage::device_cursor` reads the cursor as a device, a row (the title is
    0) and a column; a row with nothing in it, such as an empty title, stands for the nearest row
    with something in the same device. `j`, `k`, `{`, `}`, `gg` and `G` keep to the cursor's
    column of devices (`rack_column`), and `h` and `l` go on past a row's ends into the device
    beside it (`device_beside`). A sound track's Sound page is a rack of four (`sound::devices`):
    the Machine, from `Machine::grid` with `Machine::type_param` in its title, and the Filter,
    Output and Play, each a fixed grid of the track's own controls. A group's, a return's and the
    master's is its Output alone, the same grid with holes where it lacks a setting, the master's
    with the song's `Control::Limiter` in Send A's place (decision 155). Every page is a rack, so
    there are no row tables. What feeds a group or a return, its Tracks or its Sends in, is drawn
    under the Output by `views/track/sound.rs` from the song each frame, and isn't a device, as
    the cursor doesn't stop in it.
*   **Drawing** is in `views/track.rs`, with the Sound page's in `views/track/sound.rs` and the Mod
    page's in `views/track/modulation.rs`: a device's frame and title (`widgets::device`), its grid
    read through a `CellSource` (`widgets::grid`: `CellWidths::FX`, `CellWidths::SOUND`, whose
    value has room for its fine digits, and `CellWidths::COMPACT`, with no bar, for the Mod page's
    half-width devices), its panel (`widgets::panel`), and, for the Mod page's two columns,
    `widgets::device::rack_areas`.

Colours are in `theme.rs`, and the Nerd Font glyphs in `glyphs.rs`, each named with its codepoint.
The guides (`guides.rs`) are the Markdown files in `crates/vt-tui/guides/`, built into the program
with `include_str!` and opened with `Space ?`; the manual includes the same files, so the two can't
differ.

**The phrase view's timeline** (`timeline.rs`). The phrase view draws lines, not rows: a row of the
loop takes a line for each of its pulses, a line being one step of the track's `Speed`, unless
`z a` has collapsed it to a line a row (`App::rows_collapsed`). `timeline::lines` lays the rows out,
each line with its row, its pulse and when it starts, and `views/phrase.rs` draws a row's own line,
its dim continuation lines and the lines past the phrase from that, the beat rules from each line's
start, and the other tracks' columns from the ticks each line covers (`timeline::starting_in`).
The cursor, counts, selections and `App::top` stay in rows, so only scrolling counts lines: the
row-to-line helpers there are what `App::follow_cursor`, `H`, `M`, `L`, `Ctrl-d`, `Ctrl-u` and the
view's furthest scroll go by.

**Folding groups** (`folding.rs`). `App::folds` says which groups are folded, `z a`, `zM` and `zR`
setting each, and a group none has set is folded while it's empty. `Folds::hides` is what the song
view, the mixer and the phrase view's overview leave out: a folded group's tracks, and a folded
group with none. Those views draw `Folds::shown` rather than every track, and the moves between
tracks go through `Folds::step`, so they step over what's hidden. After each key,
`App::keep_cursor_shown` puts a song view or mixer cursor left on a hidden track on what
`Folds::stand_in` gives, its group or the next shown. Folds are view state: not in the song, not
saved, and reset when another project opens.

**Where it's going** (#320, the track page's cursor and moves done). Each page becomes a component that owns its state, keys
and drawing, `TrackPage`, `ProjectPage`, `MixerPage`, `SamplePage`, `SongView` and the phrase and
chain views among them, with `fn key(&mut self, key, ctx: &mut Ctx) -> Outcome` and
`fn draw(&self, frame, area, ctx: &Ctx)`. `Ctx` borrows the services every page shares: the song and
history, the engine's handle, the sample pool and analyses, graphics, settings and the status line.
`App` shrinks to those services, the current view, the modes and popups that span pages (the picker,
the command line and help) and the jump list, and the feature files become methods on the page or
service they belong to. It goes a page per PR, the track page first, with keys, drawing and
screenshots unchanged. `TrackPage` has its state and the keys that need only the cursor's track,
given to it rather than borrowed through a `Ctx`, while its edits are still `impl App` blocks;
`SamplePage` is half there too: it keeps its own cursor, zoom, scroll,
selection and settings focus, while its keys are still `impl App` blocks, in `sample_page.rs`, and
in `placing.rs` and `sample_settings.rs` for editing its points and settings.

**Decisions:** 49 to 53, 73 to 75, 100 to 104, 108, 122, 126, 128, 139, 140, 144, 145, 148, 149 and
151 to 155, with #320 and #330.

### Graphics

Pictures are drawn with the kitty graphics protocol, which kitty and Ghostty both have, and which the
program asks the terminal for at start. `kitty.rs` makes the PNGs and the commands, and `Graphics`
keeps track of which images the terminal has. Every command is sent with `q=2`, so the terminal's
answers never arrive as keys. A picture reaches the screen in one of two ways.

*   **Unicode placeholders, for pictures that fill cells.** The filter's curve, a sampler's thumbnail
    on its Sound page, the mod slots' envelopes and cycles, the FX slots' drawings, the scope, the
    picker's previews, and the sample page's waveform and overview are sent with `a=T,U=1` and
    drawn as placeholder characters, whose colour is the image's id and whose marks number its rows
    and columns, up to 297 each way. As they're text, ratatui moves and clears them as it does any
    cell, a window drawn over them hides them, and the cell backgrounds show through see-through
    pixels. That's how the cursor, the selection and
    the playheads show behind a waveform without sending it again. `Graphics::sync` sends what the
    screen is about to show, before ratatui draws, and only when what it shows has changed. A
    picture can have up to 15 colours that each mean something, and a big one is compressed inside
    its PNG.
*   **One drawing path for curves, `kitty/plot.rs`.** The FX page's compressor and EQ curves, the
    Sound page's filter curve and the Mod page's envelopes and LFO cycles are drawn by `kitty::plot` at the
    terminal's own pixels, the characters they cover times `Graphics::cell`, so nothing is
    stretched. Its `Plot` is an anti-aliased 2-pixel line, each pixel covered in eighths by its
    distance from the line, over faint dotted gridlines, as a PNG whose palette has an alpha for
    each entry, always compressed, about 1 to 2 KB. The filter curve is the Sound page's large
    panel, 49 by 5 characters, with frequency gridlines, a 0 dB line and a dashed mark at `Cut`.
    The Mod page's panels are the half size, 24 by 5: an envelope over its baseline and stage
    lines, and one cycle of an LFO from its `Phase`, drawn from the engine's own `lfo::wave`. The
    scope and the pickers' waveforms stay solid dots.
*   **Placed dots.** An LFO's dot is a small image of its own, stored once in each of two shades
    and placed with pixel offsets across and down (`Placement`'s `X=` and `Y=`) where
    `LiveValues::mods` says the LFO is, after the screen is drawn, as the sample page's lines and
    marks are; moving it is placing it again, about 60 bytes. The track page's drawing gives back
    where each cycle is (`CyclesAt`), and a dot is taken off under anything drawn over the page.
*   **Panel images, `kitty/panels.rs`.** `PanelImages` holds the track page's panels' images by
    `(PanelKey, Shade)`: the filter, a sampler's thumbnail, each mod slot, drawn by its type, and each FX slot, each in
    up to three shades, dim, focused and faint (a switched-out slot), with the shape it was drawn
    from. `Graphics::panel` sends one only when its shape is new, and one that changes again within
    a frame of a change waits a frame, so a curve following an LFO is sent at most 15 times a
    second, as the scope is. Moving the cursor or switching a slot out and back sends nothing once
    each shade has been sent.
*   **A sampler's thumbnail, `sample_wave::Thumbnail`.** The Sound page's Machine device shows the
    slice its `Slice` is on, cut as the engine cuts a note: `Start` and `Length` are shares of the
    slice in play order. It's drawn by the sample page's own `Drawing`, at one pixel for each of
    the screen's, with the start's teal line, the end's red one, and the sound outside them dim.
*   **Direct placements with `X=`, for thin lines and markers.** A placeholder can only sit on a
    character, about 15 pixels wide on the author's screen, which is too coarse for the sample
    page's cursor line, its playheads and its ruler's marks. Their images are stored once with
    `a=t`, then placed with `a=p` at a character and `X=` pixels into it, each under a placement id
    of its own, with `C=1` so the terminal's cursor stays put. Placing again under the same id moves
    it, about 60 bytes, and `a=d` takes it off. `Graphics::place` does this after ratatui draws, so
    they go where the page is now, and only for what moved. While anything is drawn over the page,
    they're taken off and drawn as characters instead, as they are in a render.
*   **Storing again after a screen clear.** ratatui clears the screen when the window changes size,
    and in kitty and Ghostty a clear frees an image placed directly, though not one shown through
    placeholders. So `Graphics::use_window` retires the lines' and marks' images when the window's
    size changes, and they're stored again under new ids. The panels' images are retired too, and
    whenever the character's size in pixels changes, so they're drawn again at the new pixels and
    none is left blank. Retired images are deleted with `a=d,d=I` at the start of the next frame.

`just screenshots` puts the pictures in its SVGs as the terminal would be sent them, and draws
each Nerd Font symbol as its outline from `screenshot/symbols.rs`, which `just symbols-font`
writes from the manual's cut-down Symbols Nerd Font Mono, as a picture on a page has no Nerd
Font to use (decision 156).
`docs/terminal-notes.md` has what's been measured and what's left to check by hand.

**Decisions:** 20, 50, 56, 57, 93, 102, 107, 109, 114, 141, 153, 154 and 156.

### Offline paths

These need no terminal and no sound device, so they run anywhere.

*   **`vimtracker render <project> <out.wav>`** (`run::render`) loads a project and its samples at
    `--rate`, 48 kHz unless given, and plays it with `vt_engine::render`: one silent pass fills
    delays and reverbs, then every track goes back to its start for the pass that's kept, so a loop
    joins up in Ableton. A missing sample is an error, rather than a render without it. `--rows`,
    `--seed` and `--tail` change its length, its seed and whether it rings out, and `--stems` writes
    one file for each group and each sound track in none that's heard, with the limiter off so they
    add up to the whole; `--split` writes one for each sound track, those in groups too. Each is a
    render with that track or group soloed. `:render` does the same from the program, on the edit
    thread, for the song as it is.
*   **`vimtracker bench <project>`** (`bench.rs`) plays a project through an `Engine` on this
    thread, in blocks the size a device asks for, stopped and then playing, and prints the share of
    a core it takes and its worst block, as the status line's CPU figure measures it, with how much
    of the worst block its thread was on the CPU, so a block the system put aside reads as that, and
    how long starting to play took. `--paced` plays each block when its time comes rather than flat
    out. `--device` plays through the output device's own callback (`Output::start_measured`), at
    its rate and buffer, sending it only silence, and splits each callback's time into the
    engine's (`Realtime::engine_time`), the queues' and the report's, and the rest, and says how
    slowed the slowdown check found it, stopped and playing; `--buffer` asks the device for
    another buffer than the audio page's. `--tracks` also
    times each track's voices and the rest of its work, each slot with an effect in it, each group,
    return and the master, the limiter, the mixing and the sequencing, with `Engine::time_parts`:
    a `Stopwatch` (`vt-engine`'s `timing.rs`) reads the clock between parts, and each track's
    `SlotTimes` around its slots, both doing nothing until asked, and the clock's own cost is taken
    off.
*   **`vimtracker probe <spec> <out.wav>`** (`dsp_probe.rs`) plays one voice or one effect through a
    track with no song, from a TOML spec, whose cases can each play notes of their own.
    `tools/compare/` renders the same spec through one or more reference plugins hosted by
    Spotify's pedalboard, or Ableton Live's own devices through a control surface script in Live
    (`tools/compare/live/`), compares the outputs, and writes a report page for each spec, with the
    references' consensus where they agree and a flag where they don't; `run.py --suite nuance`
    runs the Acid's nuance suite against Acid V and ABL3 and ranks our gaps, and `filter_refs.py`
    checks each track filter circuit against The Drop's and an Arturia filter's. `--response` also
    writes the track filter's response as the Sound page draws it. The Python side doesn't run in
    CI, while `crates/vt-tui/tests/probe.rs` checks that the specs in `tools/compare/specs/` parse
    and prepare on our side.
*   **`vimtracker notes <project> <out.toml>`** (`track_notes.rs`) reads a project, never writing
    to it, and writes one sound track's notes once through its chain, the first in any section
    that gives it one, as a probe spec's notes, with the song's tempo and the track's machine
    settings. `--track` picks the track by its name and `--machine` by its machine. Rows are timed
    as the sequencer plays them, each pulse a step at the track's speed, with each gate's share
    and a `Hold` sliding into the next row's note; a track with FX, lanes, nudges or a groove
    other than `Straight` is refused rather than read wrongly. `tools/compare/song_line.py` uses
    it for a spec that's a stub naming a song, writing the whole spec from the song each run, so a
    song's line is compared without being copied into the repository.
*   **`probe-keys`** prints the key events the terminal delivers, and **`kits import-ableton`**
    writes a kit for each of Ableton Live's drum racks.

**Decisions:** 13, 68, 110, 113, 123, 125, 133, 142 and 149, with CUJ 7 and #190 for `bench`.

### Testing

About 2,100 tests. CI runs `cargo fmt --check`, `cargo clippy` and `cargo test` on Apple silicon
macOS, the one platform the golden hashes hold on.

| Kind | Where | What it checks |
|---|---|---|
| Model | `crates/vt-model/tests/`, and unit tests in its modules | Actions on songs, undo, saving and loading, kits, decoding the sample files in `tests/samples/`, slices and marks, analysis |
| Engine | `crates/vt-engine/tests/`, a file for each feature, such as `groove.rs`, `returns.rs` and `slices.rs` | What plays when, from songs built in code and played offline |
| Golden renders | `crates/vt-engine/tests/golden/`, through `tests/support/` | An FNV-1a hash of a render's exact samples. `VT_BLESS=1` updates one after a change to the sound on purpose, and `VT_DUMP` writes the samples, to compare two builds |
| DSP | Unit tests beside each module in `dsp/` | Each voice, filter and effect on its own |
| Real-time safety | The tests in `crates/vt-engine/src/link.rs`, under `rt_guard` | That a block allocates and frees nothing |
| Interface | `crates/vt-tui/tests/`: `app.rs`, and files of their own for the track page's settings and keys (`track_page.rs`), its Sound page's rack (`sound_page.rs`), its Mod page's rack (`mod_page.rs`), its FX page (`fx_page.rs`) and the sample page (`sample_page.rs`), with the helpers they share in `common/`; and `grammar.rs` | Keys pressed into an `App` connected to a real engine, whose `Realtime` the test runs. A held key can't be pressed from a test, so held keys go through `App::key_released` and `App::key_repeated` |
| Graphics | Unit tests in `kitty.rs` and `views/` | The commands sent, and the PNGs, pinned by their size and CRC |
| Other paths | `crates/vt-tui/tests/probe.rs`, `notes.rs`, `analysis.rs` and `ableton.rs` | `probe`'s specs, `notes`' timing of a song's track, the analysis thread and its cache, and importing kits from Ableton |
| Comparison harness | `tools/compare/tests/`, with `uv run python -m unittest discover tests` | The Live bridge, against stand-ins for Live's objects. Not in CI, as nothing in `tools/compare` is |
| Screenshots | `just screenshots`, from `crates/vt-tui/examples/screenshots.rs`, into `docs/manual/src/images/` | Not a test: the SVGs are committed, so a change to what's drawn shows in the PR's diff |

`crates/vt-tui/examples/big_image.rs` and `sample_page_cost.rs` measure what sending pictures costs,
in a real terminal. `just live` drives the release build in a kitty window of its own, to check a
change by hand (`CONTRIBUTING.md`).

**Decisions:** 57, 93, 110 and 113, and the [Verification Plan](#verification-plan).

## Technical Plan

The plan as first written, before M4, kept for the reasoning behind transport, time, FX, the audio
path, editing and saving. Much of it still holds. Later decisions and the milestone designs below
change parts of it, and [Architecture](#architecture) describes the code as it is now.

### When a new version arrives

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

**Faders taken over.** A hand moving a fader the steps drive takes it over (decision 174): `App::apply`
sees the edit move a fader that the last report says the steps drive, and sends `TakeOver` before
publishing it. The engine keeps each fader's track page value and the steps' last hold of it apart from
its layers, so a takeover hears the hand's value, and `GiveBack` the steps', whichever of the
command and the version comes first. Like the mutes it's live, not in undo history, and it isn't
saved; opening a song gives every fader back. What the app has asked and the engine hasn't yet
reported is kept in `automating.rs` for half a second, so the mark doesn't flicker.

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
on each pass and stays on the same grid as everything else. Decision 80 makes swing a groove of two
steps, one of a list, with the same delays.

Whole-tick arithmetic holds for straight rows at normal speed. Decision 82 gives each track a speed
of its own, whose steps it counts, with the shares of a step worked out exactly at `1/16`.

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
`Vol`, where it shapes each note in the voice, and replaced the instrument's attack and release. `Env 2` defaults to `Cut` with no depth, and `Env 3`, added later, to no destination. Each can go to any LFO destination or to `Pitch`,
whose depth is in semitones. A second envelope on `Vol` moves the level by its depth on the level scale. The shape of `Env 1`, if it's not on `Vol`, is a plain 1 ms attack and 50 ms release.

*   **Curves:** taken from Mutable Instruments' Peaks rather than Polyend's straight segments, which on level hang on and then drop. An attack follows `t^3.32`, and decay and release
    `(1 - e^-4t) / (1 - e^-4)`, so times are exact and stages land exactly on their levels. Decision 176 makes these one of three, the envelope's mode `Curve`: `Exp` is these, `Lin` is straight, and `Snap` falls with `e^-8t`.
*   **Reset:** on by default, as on Elektron, starting each note's envelope from zero. Off goes on from the current level, as Peaks does without its hard reset. Release starts from the current level either way.
*   **Notes:** a note holds until the next note or an `OFF`. The next note fades the last with the usual short fade rather than running its release, so `Sustain 0%` suits a drum. `OFF` and `KIL` start the release of mod envelopes.
*   **Speed:** position is worked out from frames played, like LFOs. What envelopes add to `Cut`, `Res` and `Pan` passes through a `FAST_SMOOTHING_MS` smoother rather than `SMOOTHING_MS`, and `Pitch` slews per frame, so a fast snap survives.
*   **Times:** milliseconds up to `MAX_ENVELOPE_MS`, 8 seconds, as on Peaks. Not tempo-synced for now, and neither are Elektron's as far as is known.
*   **Diagram:** the track page draws each envelope. A stage's width depends only on its own time, on a log scale so a 1 ms attack shows beside a long release, and changing a stage only moves what comes after it.

**Filter.** A track has one, with `Filter`, `Cut`, `Res` and `Drive` on its page. `LP`, `HP`, `BP` and `Notch` are outputs of one TPT state-variable filter, the same design as Mutable Instruments' `stmlib::Svf`, so they cost about the same. `Ladder` is the
character filter: a saturating four-pole ladder at 2x, from Huovilainen's model, Zavalishin's integrators and Mystran's linearisation, after VCV Fundamental's VCF, which is GPL 3 or later. It self-oscillates, and its cost only falls on tracks using it. `Diode` is Vital's diode ladder, the one in `Acid`, at 2x too (decision 47). `MS20 LP` and `MS20 HP` are the Korg 35, the MS-20's filter, from sst-filters' K35, at 2x too (decision 61). `Dirty` is Vital's Dirty filter, a 24 dB low-pass whose resonance loop saturates, at 2x too (decision 71). `Mode`, under `Filter`, gives each type its other responses from what it already has, rather than more types: `12 dB` or `24 dB` for the clean four, a second state-variable filter after the first; `LP24`, `LP12`, `BP24`, `HP12`, `HP24` and `N24` for `Ladder` and `Dirty`, their stages mixed as Vital mixes them, and eleven more of the Oberheim Xpander's mixes for `Ladder` (decision 184); and where the diode's input high-pass sits. The MS-20's have none. A track keeps its mode across types, a type that doesn't offer it plays its first, steps can lock `MODE`, and a change glides (decision 72). `Res` turns each family on a taper of its own, so each equal turn raises the peak about as much as Cytomic's The Drop's knob does, and the types that self-oscillate start to from about `D9`, 85% of its turn (decision 120).
``Drive` turns up the input, soft-clips the feedback and the output at 2x, and compensates for level. At `00` the state-variable path is untouched and not oversampled. Changing `Filter` starts the new filter from silence. A low-pass with nothing to do is skipped.

**Utility filter.** Each track also has `Low cut` and `High cut` after its main filter, as Elektron's Digitakt II and Digitone II pair a swappable filter with a base-width one. They use the same
state-variable filter at 12 dB and Q 0.707 on the same 20 Hz to 20 kHz scale, with no resonance, modulation or FX for now. `00` and `FF` are off, cost nothing, and leave renders unchanged. The position is fixed after the main filter, so it can tame a driven ladder, rather than movable as on the Digitone II.

**Randomness.** Each track has its own PCG32 generator, seeded from the project seed and the track's index.
Generators restart when playback starts from the top. Because tracks are only added or removed at the end
of the list, this does not change other tracks.

### Audio path

Each track has one voice: one-shot sample playback, a filter, a volume envelope and equal-power pan, with
`80` defined as exact centre. Final gain is step velocity x `VOL` x track level x instrument volume, and `--` in the step
means full. Velocity runs `00` to `7F`, matching MIDI, and is still linear. `Acid` has none, as a 303 has none: its velocity only says whether a note is accented (decision 121). Level values run `00` to `F8` on an even scale in dB, as a fader
does, since a linear one covers only 6 dB in its top half: 4 steps is 1 dB, `E0` is 0 dB and the default, `F8` is +6 dB of headroom, `01` is -55.75 dB
and `00` is silent, so every `10` in hex is 4 dB. Anything entered above `F8` snaps to it, and the engine treats any higher value in a file the same way. Track level, instrument volume, `VOL`, `DLY` and `REV` use it, as do LFOs moving those. The scale follows
Polyend Tracker's gain tables and Ableton Live's fader, which run about -36 to +6 dB and -70 to +6 dB. Track level comes before the sends, so it scales them too, and it belongs on mixer pages
once those exist. A track's trim comes before everything else it does: on a sound track straight after the voice,
before the filter and its drive, and on a group, a return or the master before its slots. It runs a quarter of a dB a
step either side of `80`, to +31.75 dB at `FF` and off at `00`, is set outside patterns and isn't modulated, and
glides as the level does (decision 136). Transposed notes clamp to the note range.

Tracks feed a mixer with shared delay and reverb sends, a track in a group through its group's
trim, slots, level and pan, then a
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
in `meter.rs`, apart from its drawing, so a mixer can have one per track. The scope is built too (decision 57): the
callback copies the master's signal into a preallocated ring that drops frames when full, so it never waits on the UI,
and `scope.rs` folds it for the picture at the top of the meter's column. Track levels go through telemetry.

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

A generator is a pure function of its parameters, its target and a seed. The shape panel is the generator: it combines a
**mask**, `Where`, which chooses the rows to change (as Polyend's Fill tool does), with a **value source**, `Fill`, which
chooses what to write or reworks what's there, and a `Shape` holds every setting of both, the seed included, as plain
data. A generator that can still be changed is that panel kept live (decision 178), so there's one interface for both.

**Shape panel.** Built ahead of live generators, by the author's decision: Visual `S` opens a panel whose result is a
preview with no undo step, `Enter` keeps it as one undo and leaves plain values, and `Esc` restores what was there.
Selecting and pressing `S` again reshapes, from the values now there, and `.` repeats the last `Shape` on another
selection. A selection across tracks shapes each track's column with the same settings and a seed of its own, as one undo
(decision 179).

**Sweep.** A sweep covers one column, Note, Velocity, Nudge, gate length or FX, across at least two rows (decision 171 for the note's columns). A multi-column, gate-pulses or instrument-column
selection is refused with a message. `From` and `To` default to the first and last values selected, or if those match the lowest and
highest, or if those match the column's range, so a fade doesn't need swapping and a selection needn't end on a value. An FX column uses the
first command selected, and rows holding another command keep it. A `Fill` of `Random` picks from the range instead, `Wave` follows an LFO's shape (`Smooth` too), rate and phase by where each row falls in time, or makes a whole number of cycles across the selection, and `Alternate` gives the rows it changes each value in turn, for `Run` rows at a time, as `List` does with up to eight values in place of `From` and `To` (decision 96); `Ramp`, `Random` and `Wave` hold each value for `Run` rows too (decision 170). Four transforms rework the values already in the rows instead: `Scale` maps them from their own lowest and highest onto `From` and `To` along the curve, `Invert` flips them within that range, `Shuffle` deals them out again from the seed, and `Reverse` turns their order round (decision 169). `Where` chooses the
rows, as in Polyend's Fill tool: notes, no notes, those with the command, all, every n, a percent chance, or a Euclidean spread of `Hits` among the rows it can change, turned by `Rotate` (decision 168). A velocity only ever goes on a note.

**Curve.** `f(t)` applies bias then gain, where `t` runs by row position from 0 at the first row filled to 1 at the last, and the
value is `start + (end - start) x f(t)`. `bias(t,a) = t / ((1/a - 2)(1 - t) + 1)`, and gain is Schlick's gain built from it. The panel shows
them as `Curve` and `S`, from -100 to 100 with 0 a straight line, where `a = 0.5 + Curve/200` and `g = 0.5 - S/200`. The distance travelled rounds half up, so
falling mirrors rising exactly, and results clamp to the column's range.

**Live state.** A live generator is the shape panel kept live: a saved `Shape` and its target, the phrase it's saved on,
the track that phrase is on, the column, and the rows or the lane it fills. It also keeps the column's values from under
it, so moving `Where` or `Rotate` puts back what a row had, and a transform reworks the same values each time; changing
its shape never depends on what it last wrote. It **owns** the cells it fills, its column on its rows, and **reads** the
notes there when its rows are chosen by them: always on Velocity, Nudge and a length, whose rows are the notes, and on an
FX column whose `Where` is Notes or No notes.

*   The panel has two ways out: `Enter` leaves plain values, as one-shot shaping does, and `L` keeps it live. `gl` on any
    row of a live column reopens the same panel on its saved `Shape`, from anywhere and after a reload; there `Enter`
    applies it with the new settings, `L` keeps it live, and `Esc` leaves it as it was.
*   Changing parameters rerenders it, changing owned cells only. A rerender never applies it.
*   A hand edit to an owned cell applies it, shows a status message, and leaves plain values. `u` restores both.
*   A hand edit to a column it reads but doesn't own rerenders it, as Elektron's Euclidean mode follows the rhythm, and the
    edit and the rerender are one undo step: a note added under a velocity generator gets a generated velocity. A note
    that comes or goes takes the velocity, nudge or length the edit gave it as what's under it. Editing any other column
    does neither.
*   On an FX column with a lane, a live generator owns the lane's steps, filling the lane as its own loop, and follows
    the lane when it moves or changes length. Taking the lane away applies it.
*   A selection across tracks kept live is one generator per track, each reopened, changed and applied on its own, with
    its own seed.
*   `Space g a` or `:gen apply` applies the live generator under the cursor.
*   Doubling a phrase applies its generators, as the rows after the loop move. Rotating the loop's steps applies those on
    the rows it moves. Changing the loop's length doesn't, as every step is kept. A new live generator overlapping one on
    the same column applies the old one, then starts from the values it left.
*   Undo restores the live state, because it is part of the phrase. Cloning copies it, and each clone renders on its own.
*   Live cells are marked in the grid, and the phrase's heading names its live columns.

**Built** as above (decisions 180 and 181). `generator.rs` holds a `Generator`, kept in the phrase's `generators`; after
every edit, `edit` lets each live generator of each phrase the edit changed follow it, comparing its rows with how it last
left them, so no edit path needs to know about generators. A song saved with them reads in an older build as plain
values, so the format is unchanged.

### Saving and loading

A project is a folder holding `song.ron`, and a `samples/` folder once it has been bundled. Samples are referred to where they are, by a full path, or a relative one if they are inside the project. `:w path` copies
samples to a new folder. Saving writes to a temporary file and renames it, so a process crash cannot leave a partial song,
and a failed save leaves the previous file intact. Loading first reads only `format_version` and compares it, then parses
the full `Song`. A different version is refused with a message naming both versions and suggesting rendering to WAV before
updating. There is no migration code. Steps are saved as one readable string per step.

**Autosave.** Every 60 seconds, if changed, `song.autosave.ron` is written beside `song.ron` on the edit thread. On opening,
if it is newer, the program offers to restore it.

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

*Since built, in M5 to M15 and after: machines, the shared list of settings (M14), returns, groups and a master as tracks, inserts and the three track pages. [Architecture](#architecture) describes them as they are.*

This section is the plan from the author's design review after playing with the prototype, and it was **decided, not yet built** when written. Where it differs from
the rest of this document, this section wins. The rest describes what's built or was planned earlier, and has drifted from the code in places. The author reports the instrument already exceeds their expectations in
jams, and has drifted toward an Elektron feel on purpose, as those are the machines they know best. The aim is to make full songs at
about the scale of a Maschine, with the UI as the deciding concern, and not to replace Ableton.

### Machines replace instruments

A track owns everything about its sound, and steps no longer pick an instrument. Sound locks are rarely wanted when tracks are plentiful, and one sound per track makes stems simple. The signal path through a track becomes:

```text
Machine -> Filter -> Utility filter -> FX inserts -> Level, Pan -> Sends or a bus
           (3 envelopes and 3 LFOs can reach any of these)
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

A track has 3 inserts (*four since decision 138*), each holding one effect chosen with `K`, and their settings have their own page. Inserts are rows on that page, so `dd` and `p` move an effect to change the order, in place of the Heat's Flow page. A fixed number keeps it readable in a terminal, with no patching. A track can hold two of the same effect, though that won't be common: a step's command reaches the first one, and a second is set from its page and through modulation. Core effects, in order: compressor with sidechain from another track, saturation and bitcrush, 3-band EQ, chorus, then character delay and granular. Delay and reverb move to send tracks, and need real settings: decay, damping and pre-delay on a reverb after Mutable's Griesinger design, replacing the fixed Freeverb, and time, feedback, a filter and ping-pong on the delay.

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
| 3 slots | The number of slots becomes a per-track setting, and folding hides what isn't needed. *Not built: a step still has three FX slots (`FX_SLOTS`)* |
| Nothing longer than a phrase | Long moves use a long phrase, or a bus or send track's own steps |

The shape panel is already what makes ramps here. It needs only to learn glide.

**Marks.** Each value in a slot has a one-character mark beside it, the author's idea. Marks are meant to be tried in play, and are stored as one small value per slot so the rules can change cheaply.

```text
    Note V  FX1      FX2
00  C-3 64  CUT40 .  ---00
01  --- --  ---00    ---00
02  C-3 64  CUT80 g  DLY30 h
03  --- --  ---00    ---00
04  C-3 64  CUTC0 .  ---00
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

The author's decisions on this direction, from decision 1, and every one made since, are numbered in
[decisions.md](decisions.md).

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

### One-shot generators with remembered settings, and nothing more

*   **Pros**: Nothing to store or keep linked. This is how the Polyend Fill tool works.
*   **Cons**: Settings are not tied to a part, so an older part cannot be adjusted.
*   **Reason for Rejection**: Not rejected as a start: it's what ships, as the shape panel, with `.` repeating the last
    `Shape` on another selection. It's rejected only as the whole answer, as the author wants a generated part to stay
    adjustable until they take over by hand. Live generators (issue 18) are the upgrade, and they're that same panel
    kept live with its `Shape` saved on the phrase, rather than a second interface (decision 178).

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
5.  **Generators.** The shape panel already has masks and value sources, Euclid among them, and the transforms modelled on Ableton's MIDI tools; live generators keep it live. Still to come: Euclidean with two pulse generators and boolean operators, stage sequences, and chords across tracks with strum. One generator writing to several tracks is one per track, edited on its own (decision 178).
6.  **Smaller items.** Chasing held values on play, a fine-timing nudge command, keyframes in sweeps, track groups, macros, Vim-style marks, `/` search, sequence increments across cursors, Harpoon-style assignable view slots, Elektron-style conditions and fill mode, swing at group level as in Maschine, MIDI, device recovery, a portable project file, and `vimtracker export-json <project> [out.json]`, which an earlier test showed round-trips between RON 0.12.3 and JSON. Comments are not kept.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is split into issues, each intended as one pull request. Every issue leaves the workspace building with `cargo test` passing. The order after M3 is under Milestones after M3, and the rest is listed under After the milestones. The readiness bar applies to M1-M3 only. **M4 onward aren't ready to implement as written:** each gets a short design with its issues and file lists when it starts.

### Fixed Specifications

These applied to every issue of M1 to M3, as first written. Some rows have been kept current, such as the note names and the constants; others describe the first prototype, such as instruments, which machines replaced in M5, and the views on `Space 1` to `Space 6`. Where they differ, [Architecture](#architecture), the binding table (`Space k`) and the manual describe the program as it is.

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
| `FORMAT_VERSION` | 8 | Current `song.ron` format |
| `MAX_SOUND_TRACKS` | 64 | Sound tracks a song can have |
| `MAX_TRACKS` | 71 | Tracks the engine prepares state for: the sound tracks and the 7 fixed ones, `G1` to `G4`, `A`, `B` and `M` |
| `DEFAULT_TRACKS` | 8 | Tracks in a new project |
| `TICKS_PER_STEP` | 240 | Clock ticks per step, at 960 per quarter note |
| `GROOVE_UNIT_TICKS` | 40 | One unit for `DEL`, `KIL`, `OFF` and `RET`, 6 per step |
| `DEFAULT_PHRASE_STEPS` | 16 | Steps in a new phrase |
| `MAX_PHRASE_STEPS` | 4096 | The most steps a phrase keeps, so a big count can't use up memory |
| `BEAT_STEPS` | 4 | Steps per beat, for `ib` and `{ }` |
| `FX_SLOTS` | 3 | FX slots per step |
| `DEFAULT_MILLIBPM` | 120000 | New project tempo, in thousandths of a BPM |
| `HISTORY_LIMIT` | 1000 | Undo versions kept |
| `DEFAULT_RENDER_RATE` | 44100 | Hz, for tests; `vimtracker render` defaults to 48000 |
| `PUBLISH_QUEUE` | 1 | Song versions waiting for the callback |
| `RETURN_QUEUE` | 16 | Versions waiting to be freed |
| `COMMAND_QUEUE` | 64 | Transport, preview and mute messages |
| `TELEMETRY_QUEUE` | 64 | Playhead and load messages; newest dropped when full |

**Tuning values** can change by ear with no document update: `SMOOTHING_MS` (10), `DECLICK_FRAMES` (64), `PREVIEW_MS` (250), `FRAME_MS` (33), and `AUTOSAVE_SECONDS` (60).

**Step text format.** Fields are separated by one space, and an FX slot is a command and value with no space. A hold or glide adds one letter, as in `CUT40h`.

```text
Step:  NOTE VOL INST FX FX FX     C-3 64 01 CUT40 ---00 RET14
```

*   `NOTE`: `C-3` or `C#3`, from `c_1` (MIDI 12) to `G-8` (MIDI 127). `C-3` is MIDI 60, middle C, as Ableton Live names it (decision 146). Octave -1 is written with a small letter and `_` or `+`, as Furnace does, to fit three characters: `c_1`, `c+1`. `---` is empty and `OFF` is a note-off. MIDI 0-11 are not available.
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
| Normal | `;` | Audition: hear this row on every track while not playing, with no FX, changing nothing playback relies on. In the chain view, the start of that row. In the instrument view, that instrument at C-3 |
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
| Normal | `Space g a` | Apply the live generator under the cursor |
| Normal | `Space pd` | Double the current phrase |
| Normal | `Tab` | Play, or pause in place |
| Normal | `Shift-Tab` | Play from the start |
| Normal | `Space c` | Play from the cursor's step |
| Normal | `Space r` | Arm or disarm live recording, in the phrase view while playing (decision 55) |
| Normal | `i`, `v`, `V`, `:` | Insert, field Visual, step Visual, Command |
| Visual | Motions, targets, `d y > < = - + _ .` | Extend, then apply to the selection |
| Visual | `r`, then a value | Fill the cursor's column across the selection, as one undo. `vGra` writes C on each step. Velocity and instrument only go on notes, and FX values only on commands |
| Visual | `S` | Open the shape panel on one Note, Velocity, Nudge, length or FX column, on each track selected |
| Visual | `Esc` | Normal mode |
| Shape | `j k` | The row above or below |
| Shape | `= - + _` | Change the row's value, a small or a large step |
| Shape | `~`, `r` | Swap the ends, pick the random values again |
| Shape | `Enter` | Keep it as plain values, as one undo |
| Shape | `L` | Keep it live, as one undo |
| Shape | `Esc` | Cancel, restoring the version from before Shape mode |
| Insert (note) | `a w s e d f t g y h u j` | C to B, current octave, as in Ableton's computer keyboard |
| Insert (note) | `k o l p ;` | C to E, one octave up |
| Insert (note) | `1` | Note-off |
| Insert (note) | `z x` | Octave down or up. Starts at octave 3, where `a` is `C-3`, middle C, as Live names it. From -1 to 8 |
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

**Status line.** It always shows the mode, groove and BPM. Errors and notices go here. How the program itself is running, meaning CPU load (callback time divided by buffer duration), the late count and xruns, sits at the top right of the view instead, and turns red on a dropout. A dim leaf before it says the system is running the audio thread slowly to save power, and on the audio page the line under the status line says how slowed and about what the load would be at full speed (decision 150).

**Step numbers.** The phrase view shows each step's number in hex and, more faintly to its left, its distance from the cursor in decimal, which is the count to type before `j` or `k`.

**Commands.** `:q`, `:e`, `:new` and `Space qq` refuse with unsaved changes, as in Vim; `:q!`, `:e!` and `:new!` discard them.

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
| `:groove name`, `:tgroove name`, `:tgroove off` | Project groove, track groove, follow the project's again. `:swing n` is the Swing groove at `n`%. See Decision 80 |
| `:tvol xx` | This track's level in hex |
| `:reset n`, `:reset off` | Send every track to its start every `n` steps, or let them run. Off by default |
| `:kit [name or path]` | Make a kit's sounds the ones `Space 5` lists first, or choose a kit. With `all` after it, put them on tracks from the first as one undo step. See Decision 14 |
| `:seed n` | Project random seed |
| `:set skip=n` | The grid: how far `Left`, `Right` and auto-jump move in Insert, shown on the status line |
| `:set scale=kind`, `:set root=note` | A scale's kind and root, either turning it on |
| `:set scale=on`, `off` | Turn the scale on or off |
| `:scale`, `:scale fit` | Choose a kind from a list, or move this phrase's notes into the scale |
| `:set limiter=on\|off` | Whether a render clips at full level or keeps levels above it. The live output always clips at full. It starts on and is saved with the project, and is the master's `Limiter`, in its Output device after `Level` (decision 155) |
| `:len n` | How many steps the loop plays, while stopped. Decimal, or hex after `0x` as in Rust: `:len 0x10`. `:reset` reads numbers the same way. Steps outside it are kept and saved |
| `:double` | Double the loop, while stopped |
| `:gen apply` | Apply the live generator under the cursor |
| `:render path [--rows n] [--tail]` | Render to WAV |

The subcommands are `vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` and `vimtracker probe-keys`. `vimtracker [project]` starts the program. *Since joined by `--stems` and `--split`, `bench`, `probe`, `notes` and `kits import-ableton`: see [Offline paths](#offline-paths).*

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

**Audio.** Voice: linear-interpolated playback, with `C-3` at original pitch, and a 1 ms attack and 50 ms release as instrument defaults. A new note fades the old one over `DECLICK_FRAMES`. Filter: a TPT state-variable low-pass. Delay: stereo at 3/16 note time, with its buffer allocated for 20 BPM, about 2.25 seconds. Reverb: Freeverb-style. Sample import accepts 16-bit, 24-bit and 32-bit float WAV, mono or stereo, and refuses others with a message naming the file; AIFF and FLAC are read too (decision 63). Rate conversion uses `rubato`.

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
*   `src/dsp/`: the voices, filters, effects and dynamics in `voices/`, `filters/`, `effects/` and `dynamics/`, on the shared building blocks in `blocks/`, as "DSP layout" in the Technical Plan gives them.
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
*   **Issue 19: Shape mode.** Add `shape.rs`, Visual `S`, `gl`, and `Space g a`. Each change rerenders and publishes without adding history. `Enter` adds one history entry and `Esc` restores the starting version. `.` applies the last shape to the current selection. **Done when:** CUJ 3 and CUJ 4 work with audio.
*   **Issue 20: Scope and meters.** Add the scope tap to `link.rs` and `views/scope.rs`, with a small master scope and per-track level meters using Braille and block characters. **Tests:** the real-time guard stays at zero with the tap active; a full tap drops frames rather than waiting.
*   **Issue 21: Render command.** Add `vimtracker render` and `:render`. **Tests:** golden hash tests; a render is exactly the loop's length; `--tail` extends it; `VT_BLESS=1` updates hashes. **Done when:** CUJ 7 works.
*   **Issue 22: Autosave.** Add autosave to `project.rs`. **Tests:** only a changed project is written; a newer autosave is offered on opening; audio is unaffected.
*   **Issue 23: G7.** Run G7, and record how many tracks the laptop sustains in `docs/g7-result.md`. G1 is retired.

### Milestones after M3

These milestones build the section Direction: a track is one sound, in order, and G7 runs before M4. Each gets a short design when it starts, and ends when the author has used it in a jam. *Each design below was written before its milestone was built, so "Not built yet" in one means then; M4 to M14b are built, with M15's effects and machines, and later milestones (M16 to M29) are on GitHub, with their designs in the decision log and in [song-flow.md](song-flow.md), [fx-page.md](fx-page.md) and [track-pages.md](track-pages.md).* M5 comes before M8 so songs aren't made in a format about to break, and M9 comes before M10 as send tracks reuse inserts. The Filters milestone on GitHub stays open for #50 alongside these.

*   **M4: Marks.** Lock and hold on the existing value commands, with `~` to change them, then glide. **Tests:** a lock returns to the value beneath it at the next note or value, a hold stays until playing from the top, and a glide lands exactly on its next value.
*   **M5: A track is one sound.** Remove instruments from steps, name tracks, kits as sets of sounds, a `Sound` type with the one-shot sampler as its first machine, a sample browser, the Sound and Mod pages, and `:render --stems`. This stops old songs loading. **Tests:** a track's sound saves and loads, stems sum to the full render without sends, and renders stay repeatable.
*   **M6: Editing across tracks.** Selections and operators that reach across tracks, as drum parts now spread over several.
*   **M7: Mixer.** One page of level, pan, sends, mute and solo across all tracks. **Tests:** an edit here matches the same edit on the track page, and is one undo step.
*   **M8: Songs.** Sections, a song view, section lengths, and each section resetting or carrying track positions. **Tests:** a song plays its sections in order, a render covers a whole song, and stems cover a whole song.
*   **M8b: Jamming.** Launch a chain on one track while playing, and capture what's playing as a new section. **Tests:** a launch waits for the track's chain to end, stopping a track, a launch ending with its section, capturing without a break, and no allocation in the callback.
*   **M8d: Session workflow.** Launch timing on a beat or bar grid, one-phrase chains opening straight to their phrase, chain labels, and cloning a chain with its phrases from the song view. **Tests:** as [its design](song-flow.md) lists.
*   **M8c: Song flow.** One position rule for rows and launches, so a changed chain starts from its top and a part can span rows, and rows that repeat. **Tests:** as [its design](song-flow.md) lists.
*   **M9: FX inserts.** 3 inserts on a track (four since decision 138), the first effect, the FX page, the shared parameter list for effects, and slots naming any parameter on that list. **Tests:** as [its design](#m9-design-fx-inserts) lists.
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

**Step text.** `NOTE VEL FX FX FX`, as in `C-3 64 CUT40 ---00 RET14`. Field and column numbers shift down by one.

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

Made with the author's agreement on 2026-11-06. Not built yet. *Since built, with four slots since decision 138.* It adds 3 insert slots to each track, one effect to put in them, and an FX page, and sets up what later effects reuse.

**Prior art.** The boxes nearest this workflow mostly have a fixed chain per track: the Digitakt II has bit reduction, overdrive and sample rate reduction before its filter, the M8 has `DEGRADE` and a limiter per instrument, and tracker-ce has overdrive and bit depth per instrument. Slots come from Maschine, which has as many as you like on sounds, groups and master, and the Tonverk, which has two. Slots are kept here because order matters once M14 adds a compressor, and a fixed chain would get long.

**Signal path.** `Machine -> Filter -> Utility filter -> Insert 1, 2, 3 -> Level, Pan -> Sends`, as in Direction above. In the engine, this goes in `TrackState::shape`, after `trimmed` and before the gain. `Drive` stays on the filter.

**Data.** `Track` gains `inserts: [Insert; 3]`, with a serde default of all empty, and `FORMAT_VERSION` stays as it is, so existing songs load and sound the same. `Insert` is an enum of `Empty` and `Distort`, whose settings are plain copyable values. Changes go through `Action`, so each is one undo step.

**Engine.** `TrackState` gains a preallocated, copyable state for each slot, as machines are, so the callback doesn't allocate. An empty slot is a match and nothing else, and a track with no inserts renders exactly as before.

**The first effect: `Distort`.** One effect with a `Type`, after #87.

| Setting | Values | Default |
|---|---|---|
| `Type` | `Soft`, `Hard`, `Fold`, `Sine`, `Crush`, `Down`, and the guitar pedals `Overdrive` and `Distortion` (#361, decision 130) | `Soft` |
| `Tone` | The pedals only: `00` dark to `FF` bright | `80` |
| `Amount` | `00` to `FF`, more of what `Type` does. `Crush` has `Bits` here | `40` |
| `Mix` | `00` dry to `FF` wet | `FF` |
| `Gain` | On the level scale, after `Mix` | `E0`, which is 0 dB |

The shapes come from Vital's `distortion.cpp`, which is GPL 3 or later as we are, so its copyright notice goes in `dsp/effects/distort.rs`. With `d` from `Amount`:

*   `Soft`: the overdrive from Plaits' `dsp/fx/overdrive.h`, MIT, which is curved so a little does little, and holds a loud sound within about 3 dB as it's turned up. A quieter sound still comes up, by about 9 dB from half level and 22 dB from a tenth at `FF`, as a clip can't hold back what doesn't reach it. `Amount` covers its top half, as below that it gets quieter.
*   `Hard`: `x * d`, clamped to -1 and 1.
*   `Fold`: a triangle wave of `x * d`, from Vital's linear fold.
*   `Sine`: a sine of `x * d`, from Vital's sine fold.
*   `Crush`: `x` rounded to steps for whole `Bits`, from `12` to `2`, in place of `Amount`, by the author's decision. Bit depth is a set of whole values, so it changes at once, with nothing in between to smooth. Nothing is lost, as a step's lock gives the same instant change, and there's no special rule: an LFO can still be assigned to it, and just steps.
*   `Down`: holds a sample for a time set by `Amount`, in time rather than samples, so it sounds the same at any device rate.

`Soft`, `Hard`, `Fold` and `Sine` run inside the `Oversampler` at 2x, which moves out of `filter.rs` to be shared. `Crush` and `Down` are meant to alias, and don't. `Mix` is a straight blend outside the oversampling. Vital's file doesn't define how it scales drive, or have a mix or level compensation, so these are ours: for the types other than `Soft`, `d` runs from 0 dB at `00` to +30 dB at `FF`, and there's no compensation to begin with. `Amount` and `Mix` are smoothed as other values are, so they can move without clicks.

**Every effect ends with a `Gain`**, by the author's decision after hearing `Distort`, so a change in level can be fixed while the knob is being turned, with no change of page. Matching levels automatically is left out: a level measured over 300 to 500 ms would make a drum loop pump, and a fixed curve depends on how loud the sound going in is.

**The parameter list.** An effect declares its settings in a static list: name, step command, modulation destination, highest value and default. The FX page, stepping, `x`, `K`, typing, step commands and `Dest` all read it, so a new effect needs no new page code. This covers effects only. The Sound and Mod pages' existing controls stay as they are, which goes back on the wording under A shared parameter list above: a file of about 1,000 lines that works isn't rewritten until there's a reason. That leaves two systems side by side for now.

**The FX page.** *Since replaced by a rack of devices, decision 139 and [`fx-page.md`](fx-page.md).* A third page on the track page, with one row per slot:

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

1.  **Inserts and `Distort` in the engine:** the data, the state in `TrackState`, `dsp/effects/distort.rs` and the shared `Oversampler`. **Tests:** golden renders unchanged with empty slots, each type's response, `Mix` at each end, oversampling keeps aliasing down for the types that use it, slots run in order, a saved insert loads, and no allocation under the real-time guard.
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

**Order.** Each update runs envelopes, then LFO 1, then LFO 2, then everything else, and each object reads what's before it. `LFO 2`'s `Dest` list has `LFO 1`'s settings, and not the reverse, so there's no feedback to detect and a project always sounds the same. *Since decision 138 there are three of each, in the same one-way order: `LFO 1` reaches `LFO 2` and `LFO 3`, `LFO 2` reaches `LFO 3`, and every envelope reaches every LFO.*

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

*   **A return or the master is a `Track` with no machine.** It makes no sound of its own, so it has no Machine or filter rows on its Sound page, and notes on its steps do nothing. It has everything else a track has: 4 slots, LFOs, envelopes, phrases, a mixer strip and a meter, with no new code.
*   **Its own steps reach it.** A `1 Decay 40` on `B`'s phrase sequences its reverb, and no other track's steps can, as on the Syntakt, which keeps a phrase self-contained.
*   **Sends.** The `Delay` and `Reverb` settings on a track become `Send A` and `Send B`, and the `DLY` and `REV` commands follow. A return has no sends, so there's no feedback between them.
*   **Solo and mute.** Soloing a track keeps the returns and master sounding, as in Ableton. Soloing a return plays what comes back from it, with what sends to it still playing but not heard. Muting a return silences it, and the master can't be muted or soloed. **Names.** The commands are `SNA` and `SNB`.
*   **Where they show.** In track lists, the song view and the mixer, labelled `A`, `B` and `M`. `W`, `B` and `]b` go through them. They're left out of `:track add` and of how many tracks a kit fills.

**`Delay` and `Reverb` are effects.** They're chosen for a slot like `Distort`, so any slot on any track can hold them. A new song has `Delay` in `A`'s first slot and `Reverb` in `B`'s, each with `Mix` at `FF`. The fixed global delay and reverb go.

| Effect | Settings |
|---|---|
| `Delay` | `Time`, `Feedback`, `Ping-pong`, `Width`, `Low cut`, `High cut`, `Drive`, `Mix`, `Gain` |
| `Reverb` | `Pre`, `Decay`, `Damp`, `Low cut`, `High cut`, `Mix`, `Gain` |

`MAX_PARAMS` grows to 9, and the FX page keeps its columns of two, top then bottom. The places in `Target` and the commands for slots grow with it. *Since: `MAX_PARAMS` is 12 (decision 135), and the FX page is a rack of devices of three rows (decision 139).*

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
*   **The step text** gains the cell after velocity when a row has one: `C-3 64 HLD3 CUT40 ...`. A row without one is written as before, so saved songs and every step written as text in the tests stay as they are.
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
| Each effect, in each of the 3 slots (four since decision 138) | Its own, as effects declare them now, by their `Setting` |
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
