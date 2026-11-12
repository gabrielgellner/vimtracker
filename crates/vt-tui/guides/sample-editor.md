# The sample editor

The sample page shows the current track's sample as a big waveform, to hear it,
slice it and set its points. Nothing here writes audio: every edit changes how
the sample plays, and `u` takes it back.

## Opening it

`Space 0` shows the current track's sample, or `g d` on the sample in the Sound
page's Machine title, or on a step's `SLI` to land on that slice. `C-o` goes back to
where you came from. `W` and `B` show the next or previous track's sample.

The start is marked in teal, the end in red, the loop in blue, and each slice's
start in peach, with its number in the ruler above. The rows under the waveform
are the whole sample, its points and `Snap`, the slice under the cursor with its
settings, and the cursor's time, frame and slice.

## Moving and zooming

A sample is like a text: a column of the waveform is a character, a sound is
a word, and a slice is a line. Every motion takes a count.

| Keys | Move to |
|---|---|
| `h` `l` | A pixel earlier or later; faster as it's held |
| `0` `$` | The start or end of the slice |
| `^` `g_` | Its first or last sound, above -60 dBFS |
| `w` `b` | The next or previous sound's start: a hit, as `Sens` finds them |
| `e` | Where the sound decays, 30 dB below its peak, or the next one's |
| `gg` `G` | The start or end of the sample; with a count, a slice |
| `j` `k` | The next or previous slice |
| `}` `{` | The next or previous beat at the song's tempo |
| `]]` `[[` | The next or previous point: a slice, the start, the end, the loop |
| `H` `M` `L` | The left, middle or right of the view |

| Keys | View |
|---|---|
| `zi` `zo` | Zoom in or out by 2 on the cursor, as `Option-1` `Option-2` do |
| `zO` `zv` | The whole sample, or the slice filling the view |
| `C-d` `C-u` | Half a screen later or earlier |
| `C-e` `C-y` | A column later or earlier |
| `zz` `zs` `ze` | The cursor in the middle, at the left or at the right |
| `zc` | The cursor as a thin line, or as its column |

## Sounds

Where each sound starts, its onset, is found once when the sample joins the bank,
and kept with its waveform in the program's cache folder, never beside the file.
`Sens` on the settings row says how readily a rise counts as a sound: at 0 only
the strongest hits do, at 100 nearly every ripple, and 50 to start with. Each
change says how many sounds it finds, and `u` takes it back. `w` and `b` go
from sound to sound, and `e` to where each one has died away, as Vim's go by
word.

## Slices and marks

`:slice 16` cuts the sample into 16 of the same length, `:slice transients`
at each sound's start, as `w` stops, `:slice beat 1/16` each sixteenth, and
`:slice silence` at each sound after silence. `:slice clear` plays it whole
again. `j` and `k` go from slice to slice, `3G` goes to slice
`03`, and `]]` stops at every point on the way. The **Sounds, samples and kits**
guide has the rest of `:slice`.

`m` then a letter names the slice under the cursor, as `:mark a` does, and `'`
then the letter goes to it, as a jump `C-o` comes back from. `]'` and `['` go
to the next or previous named slice. A step can play a named slice as `SLI 'a`.

## Hearing it

| Keys | Plays |
|---|---|
| `;` | The slice under the cursor as the song plays it, or the selection |
| `Space c` | From the cursor to the sample's end |
| `Esc` | Stops it |

Tapped, each plays to its end; held, only while the key is down. A green
playhead follows what's playing. They play through the track, so a muted track,
or another track soloed, plays them silently, and the status line says so.

`i` turns the keys into pads, as on an MPC or tracker-ce's Beat Slice: `a s d f
g h j k` play slices `00` to `07`, and `q w e r t y u i` play `08` to `0F`.
Each moves the cursor to its slice as it plays. `<` and `>` go a page of 16
slices back or on. Every other key works as it does, so `;` still plays.

## Editing points

| Keys | Does |
|---|---|
| `Enter` | Split the slice at the cursor, or at the playhead as it plays |
| `J` | Join the slice with the next |
| `x` | Take the point under the cursor away |
| `=` `-` | Nudge the point under the cursor a pixel later or earlier |
| `+` `_` | Nudge it a column later or earlier |
| `D-l` `D-h` | Cmd and `l` `h`: nudge it a frame later or earlier, finer than `=` `-` |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_` |
| `g s` `g e` | Start or end the sample at the cursor |
| `g l` `g L` | Start or end the loop at the cursor |

To slice a break by ear, play it with `Space c` and tap `Enter` on each hit:
each tap splits where the playhead is. A nudge stops at the points either side.
With `Snap` on, a point placed or nudged goes to the nearest zero crossing, so
it doesn't click; a frame's nudge never snaps.

`>` and `<` turn the slice under the cursor up or down a dB, and `~` reverses
it.

## The settings rows

`K` lists the settings: `Snap` and `Sens`, for the whole sample, and the
slice's `Gain`, `Pan`, `Tune`, `Reverse`, `Fade in` and `Fade out`. Choosing
one puts the focus on it. There `h` and `l` go along the settings, `0` and `$` to their ends, `=`
`-` change it a small step and `+` `_` a large one, Cmd with `l` `h` `k` `j` as
those four, `K` lists its values, and
`x` puts a slice's own back to the sample's. `j` and `k` still go from slice to
slice, so a setting can be set slice after slice. `Esc` goes back to the
waveform.

## Selecting

`v` starts selecting frames from the cursor, and `;` plays them. `o` goes to the
other end, and `v` or `Esc` ends it. While selecting, `is` selects the sound
the cursor is in, from its start to where it decays, and `ip` the slice, as
Vim's `iw` and `ip` select a word and a paragraph; with a count, so many.
`:` while selecting starts the line with `'<,'>`, so `:'<,'>slice transients`
slices only what's selected. `gv` selects it again, and `'<` and `'>`
go to its first and last frame. `''` goes back to where the last jump left.

## Which mode you're in

The badge at the left of the status line names the page's mode, and the line
under it says what the keys do there until something else is said.

| Badge | Is | Leave with |
|---|---|---|
| `SAMPLE` | The waveform: `h` and `l` move the cursor | |
| `PADS` | `i`'s pads: `h` `j` `k` play slices, not move | `Esc` |
| `SETTINGS` | `K`'s focus: `h` `l` go along the settings | `Esc` |
| `VISUAL` | `v`'s selection | `v` or `Esc` |

With the focus on the settings, the cursor's line is set back, as it isn't
what `h` and `l` move.
