# Saving and rendering

| Command | Does |
|---|---|
| `:w` | Save |
| `:w path` | Save to a new folder, with a copy of every sample |
| `:e path` | Open a project |
| `:e` | Choose a project to open |
| `:new path` | Start a new project |
| `:q` | Quit, or refuse if there are unsaved changes |
| `:q!` | Quit and discard changes |
| `Space qq` | The same as `:q` |

`Tab` completes a path, one folder at a time, and `Up` brings back an earlier
command. **Navigation** has the rest of the command line.

`:e` on its own lists projects and folders as a tree, starting beside this project,
and shows the one under the cursor: its tempo, its tracks and when it was saved.
`:w` before the project has a folder, and `:new` on its own, list folders: `l` opens
one, and `Enter` chooses it and puts the command on the command line with the folder
and a name filled in, so `Enter` saves it there.

## Names

A new project is given a random name, such as `dusty-breakdown`, and saving it
starts with that name; `:new` starts with a fresh one. A name already in the folder
is never offered: another is rolled. Type over the name, or press `C-r` on the
command line for another, as many times as you like. `C-r` works on any `:w`,
`:new` or `:render` line, keeping the folder, a `.wav` and any options.

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

Hold `;` and the row sounds until you let go. A tap plays a sample out to its
end, and gives a synth that would otherwise hold its note a short blip.

## Rendering to a WAV

`:render loop.wav` writes the loop, ready to drop into Ableton. It uses what
is on screen now, including mutes, and the screen waits until it finishes.
`:render` with no path lists folders, starting with the project's: `Enter` chooses
one, and the command line is filled in with the project's name, which you can change
or roll with `C-r`, and any options you gave. If that file is there already, the name
gets a number, as `dusty-breakdown-2.wav`, so an earlier render isn't written over.

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

`vimtracker bench my-song` plays a saved project as fast as it can, stopped
and then playing, and says how much of a core it took. The CPU figure on the
status line reads higher for the same project: it's the share of each block's
time, and macOS runs a lightly loaded audio thread on a slower core or clock,
then speeds it up as the load rises.

## Autosave

Unsaved work is written to `song.autosave.ron` every minute once the project
has a folder. After a crash you are asked whether to restore it. `:w`, any
quit and answering no all remove it.
