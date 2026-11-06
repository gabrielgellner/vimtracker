# Saving and rendering

| Command | Does |
|---|---|
| `:w` | Save |
| `:w path` | Save to a new folder, with a copy of every sample |
| `:e path` | Open a project |
| `:new path` | Start a new project |
| `:q` | Quit, or refuse if there are unsaved changes |
| `:q!` | Quit and discard changes |
| `Space qq` | The same as `:q` |

## Samples and moving projects

A project refers to its samples where they are, so it is small, but it needs
them to stay put. `:w path` gathers everything into a new folder that can be
moved or shared, and you carry on working in that folder.

## Playing

| Keys | Does |
|---|---|
| `Tab` | Play, or pause in place |
| `Shift-Tab` | Play from the start |
| `Space c` | Play from the cursor |
| `;` | Audition: hear this row on every track, while stopped |
| `:bpm 137.63` | Set the tempo |

`;` means the same in the phrase and chain views: in a chain it plays the start
of that row. Press `j` then `;` to step
through a pattern. Each audition cuts the last, and an empty row silences
it. FX on a row don't run, so `DEL`, `CUT` and the like are only heard in
playback.

## Rendering to a WAV

`:render loop.wav` writes the loop, ready to drop into Ableton. It uses what
is on screen now, including mutes, and the screen waits until it finishes.

| Option | Does |
|---|---|
| `--rows 8` | Render 8 rows instead of the whole loop |
| `--tail` | Keep going until delay and reverb die away |
| `--stems` | Write one file per track, with the path as a folder |

`:render out --stems` writes `01-BD.wav`, `02.wav` and so on, one for each track
that would be heard. Each keeps its own delay and reverb, with the limiter off,
so together they add up to the loop, and they all end together.

The file starts with every track at its start, and is as long as the `:reset`
if there is one, or else the longest track. Tracks that don't divide that
length won't join up when the file repeats, so set a reset or use `--rows`.

From a shell, `vimtracker render my-song out.wav` renders a saved project
with no sound device. It stops if a sample is missing.

## Autosave

Unsaved work is written to `song.autosave.ron` every minute once the project
has a folder. After a crash you are asked whether to restore it. `:w`, any
quit and answering no all remove it.
