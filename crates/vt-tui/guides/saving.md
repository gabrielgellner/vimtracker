<!-- ANCHOR: top -->
# Saving and rendering

| Command | Does |
|---|---|
| `:w` | Save |
| `:w path` | Save to a new folder, with a copy of every sample |
| `:w!` | Save, even over a read-only song |
| `:set ro`, `:set rw` | Make the song read-only, or not |
| `:e path` | Open a project |
| `:e` | Choose a project to open |
| `:e! path`, `:e!` | The same, discarding changes |
| `:new path` | Start a new project |
| `:new! path`, `:new!` | The same, discarding changes |
| `:q` | Quit, or refuse if there are unsaved changes |
| `:q!` | Quit and discard changes |
| `Space qq` | The same as `:q` |

`Space 9` opens the project page, which does the same from rows; see **The project
page** below. `Tab` completes a path, one folder at a time, and `Up` brings back an earlier
command. **Navigation** has the rest of the command line.

`:e` on its own lists projects and folders as a tree, starting beside this project,
and shows the one under the cursor: its tempo, its tracks and when it was saved.
`:w` before the project has a folder, and `:new` on its own, list folders: `l` opens
one, and `Enter` chooses it and puts the command on the command line with the folder
and a name filled in, so `Enter` saves it there.

`:e` and `:new` refuse while there are unsaved changes, as `:q` does. `:e!` and
`:new!` throw them away, with their autosave, and go ahead; `:e!` on its own opens
the explorer, and the project chosen there opens without asking. `:e!` with this
project's own folder goes back to how it was last saved.

## Names

A new project is given a random name, such as `dusty-breakdown`, which the song
keeps, and the window's title shows. Saving it somewhere new starts with that name,
and the project is then named after the folder it went into, so the two agree unless
you type another. If the folder has that name already, it gets a number, as
`dusty-breakdown-2`. `:new` starts with a fresh random name, and one already in the
folder is never offered. Type over the name, or press `C-r` on the command line for
another, as many times as you like. `C-r` works on any `:w`, `:new` or `:render` line,
keeping the folder, a `.wav` and any options.

Renaming a project on the project page keeps its folder: `:w` still saves there, and
Save as starts with the new name. A project saved before it had a name takes its
folder's.

## Read-only songs

A song can be parked read-only, so it can't be changed by accident, as Vim's `readonly`
keeps a file. The tutorial's songs are, so a stray `:w` can't change the copies in the
repository. `[RO]` on the status line, after the `*` for unsaved changes, and in the
window's title says so.

*   `:w` refuses: `This song is read-only: :w path saves a copy, :w! writes over it`.
*   `:w path` saves a copy in a new folder, which isn't read-only, so you carry on in it
    and `:w` saves there from then on.
*   `:w!` writes over it anyway, on purpose, and it stays read-only.
*   `:set readonly` or `:set ro` makes a song read-only, and `:set readwrite` or
    `:set rw` lets `:w` write over it again; Vim's `:set noro` does too. Either is an
    unsaved change, saved with the song, and isn't undone with `u`. The project page's
    `Read-only` row does the same.
*   With unsaved changes, `:e`, `:new` and `:q` refuse as ever, and offer `:w path` to
    save a copy rather than `:w`.

It's kept in `song.ron` rather than as the file's permission, which git doesn't keep.
Autosave still writes a read-only song's work beside it, and restoring it after a crash
puts it back as unsaved changes, never over the song: `:w path` or `:w!` keeps it.

## The project page

`Space 9` opens the project page, which has two pages, as the track page has four, and
`H` and `L` go between them. It opens on the one you saw last.

*   **Song** shows the project's own settings in one place, each the same value as its
    command changes, with undo where the command has it.
*   **Audio** is this machine's: the device, its rate and buffer, the mouse wheel and
    the Roto-Control. Its title says `Audio · this machine`, as these are kept with the
    machine, in `audio.ron`, `wheel.ron` and `roto.ron` beside `settings.ron`, rather
    than in the song, so a song opened on another machine plays through that one's. See **The audio page** in the meter guide.
<!-- ANCHOR_END: top -->
<!-- ANCHOR: rest -->

The Song page's rows:

| Row | Is | Command |
|---|---|---|
| Name | The project's name | |
| Folder | Where it's saved, and if it has changes | |
| Read-only | Whether `:w` refuses to write over it | `:set ro` |
| BPM | The tempo, and its fine part | `:bpm` |
| Groove | The groove for tracks without their own | `:groove` |
| Launch | When launched chains start | `:launch` |
| Reset | When this section starts tracks again | `:reset` |
| Seed | The seed chance FX play from | `:seed` |
| Scale, Root | The scale for entering notes | `:set scale`, `root` |
| Grid | How far the arrows move in Insert | `:set skip` |
| Auto-jump | Whether entering moves on | `;` in Insert |
| Edit | The groove drawn below, to change | |
| Steps, Humanise | Its length, and its random timing | |
| Push, Velocity | Each step's push and velocity | |
| Audio | The device and rate | `Space 9`, `L` |

`i` on Name edits it, and `C-r` there rolls a new one. `l` on BPM goes onto its fine
part, the digits after the point. `Enter` on Audio opens the Audio page. The limiter
isn't here: it's on the master's Sound page, after its `Level`, and `:set limiter`
switches it too.

It moves as the track page does: `j` `k` go between rows and keep their column,
`gg` `G` to the first or last row, or row n with a count, `h` `l` between settings
and `0` `$` to the first or last. `=` `-` `+` `_` change a value, as do `C-a` `C-x`
and Cmd with `l` `h` `k` `j`, `i` types one, as its command takes it, `K` lists the choices on Groove, Edit, Launch, Scale, Root and Grid,
and switches Auto-jump and Read-only, and `x` puts one back as a new project has it.

At the bottom are Save, Save as, Render, Open and New, which `Enter` does: they are
`:w`, `:w` to a new folder, `:render`, `:e` and `:new`, and open the file explorer
and the name line as those do.

## The project's grooves

On the project page, under the settings, is a groove drawn as its pushes: a tick
above the line for a step that lands late and below it for one that lands early,
as tall as its push, with the push in ticks under it, 240 to a step, and the
step's velocity under that, in percent of each note's own. See **Groove** in the FX guide for what a
groove does.

`Edit` chooses which groove is drawn, the project's until you choose another, and
`K` there lists them. Changing one that comes with vimtracker copies it into the
first of the project's own not yet made, `Custom 1` to `Custom 8`, and the project
plays the copy if it played the one copied, all as one undo step. The project's
own are kept in the song.

`Steps` is how many it has, from 1 to 16, and `Humanise` how far either way each
step may land at random, up to 24 ticks, the same each time it plays from the
project's seed. On `Push`, `=` and `-` move a step a tick, `+` and `_` ten, `i`
types a push such as `-12`, and `x` puts it back on the grid. The first step
stays on the grid: the others are pushed from it, which is what keeps a groove
from changing the tempo. On `Velocity`, `=` and `-` move a percent, `+` and `_`
ten, and `x` puts it back to 100.

## Samples and moving projects

A project refers to its samples where they are, so it is small, but it needs
them to stay put. `:w path` gathers everything into a new folder that can be
moved or shared, copying each of the bank's files into its `samples` folder
once, and you carry on working in that folder.

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
| `--stems` | A file per group and per track in none, in the path as a folder |
| `--split` | Stems of each track, those in groups too |

`:render out --stems` writes `01-BD.wav`, `02.wav` and so on, one for each track
that would be heard, and `G1.wav` for a group, with all its tracks through its
slots, in place of a file for each of them. Each keeps its own delay and reverb,
with the limiter off, so together they add up to the loop, and they all end
together. `--split` writes each track in a group on its own instead, still
through its group's slots.

The file starts with every track at its start, and is as long as the `:reset`
if there is one, or else the longest track. Tracks that don't divide that
length won't join up when the file repeats, so set a reset or use `--rows`.

## From a shell

These run without opening the program, and need no kitty: give them a saved
project's folder.

| Command | Does |
|---|---|
| `vimtracker render my-song out.wav` | Render it, with no sound device |
| `vimtracker bench my-song` | Say how much of a core it takes |
| `vimtracker notes my-song out.toml` | Write a track's notes, as TOML |
| `vimtracker probe spec.toml out.wav` | Play one voice or effect alone |
| `vimtracker probe-keys` | Show the keys the terminal sends |
| `vimtracker kits import-ableton` | Make a kit of each of Live's racks |

`render` takes the same `--rows`, `--tail`, `--stems` and `--split` as `:render`,
and `--seed` for another seed. It stops if a sample is missing.

`bench` plays the project as fast as it can, stopped and then playing, and says
how much of a core it took. The CPU figure on the status line reads higher for
the same project: it's the share of each block's time, and macOS runs a lightly
loaded audio thread on a slower core or clock, then speeds it up as the load
rises; the `󰌪` before the figure says so. `--tracks` also says how long each
track, slot, group, return and the master take, and `--paced` plays each block
when its time comes, as the audio thread does. With `--device` it plays through
the sound device, silently, as the program does, and says how much the system
slowed it; `--buffer 128` asks the device for another buffer than the audio
page's.

`notes` and `probe` are for comparing vimtracker's sound with other instruments
in `tools/compare`: `notes` writes one track's notes once through its chain, with
the tempo and its machine's settings, `--track BD` for a track by name, and `probe`
renders a voice or an effect from a spec file. `probe-keys` prints each key event
as the program receives it, and whether the terminal has the kitty keyboard and
graphics protocols, for when a key doesn't do what it should. **Sounds, samples
and kits** has `kits import-ableton`.

## Autosave

Unsaved work is written to `song.autosave.ron` every minute once the project
has a folder, a read-only one too. After a crash you are asked whether to restore it. `:w`, any
quit, `:e!`, `:new!` and answering no all remove it.
<!-- ANCHOR_END: rest -->
