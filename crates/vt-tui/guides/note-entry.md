# Note entry

Put the cursor on the Note column, as the status line shows, and press `i`.
The keyboard is now a piano, laid out as in Ableton:

```
 w e   t y u   o p      black keys
a s d f g h j k l ;     white keys, a is C
z x c v                 octave and velocity
```

So in Insert, `j` is a note. **Each note plays, is written and moves down**
by the step setting.

| Keys | Does |
|---|---|
| `z` `x` | Octave down or up. The status line shows it as `Oct` |
| `c` `v` | Lower or raise the velocity written. The status line shows `Vel` |
| `1` | Note off |
| `Backspace` | Clear this note and move up |
| `Tab` | Play or pause, and stay in Insert |
| `Shift-Tab` | Play from the start, and stay in Insert |
| `Esc` | Back to Normal mode |

Our octave numbers are one above Live's: `a` starts on `C-4`, the same pitch
Live calls C3.

Notes play the track's sample: see **Sounds, samples and kits**.

## Moving without writing a note

The arrows move and write nothing, and you stay in Insert.

| Keys | Does |
|---|---|
| `Up` `Down` | One step |
| `Left` `Right` | One beat from the note you just wrote |
| `Shift-Left` `Shift-Right` | To the previous or next beat on the grid |

A plain arrow keeps a note's place in the beat. For hats on the "and", start
on step 2 and press `a` then `Right` over and over: steps 2, 6, A and E. If
you haven't just written a note, it counts from the cursor.

The distance is the grid, shown as `Grid` on the status line. As in Ableton,
`Option-1` makes it narrower and `Option-2` wider, halving or doubling it from
`1/16` up to a bar, in Insert and Normal. `:set skip=8` sets it to any number of
steps. Anything half typed in another column is dropped when you move.

## Writing every few steps

Set how far each note moves, which suits drums.

| Command | Does |
|---|---|
| `:set step=4` | Each note moves 4 steps, so one on every beat |
| `:set step=2` | Every eighth note |
| `:set step=0` | Stay put, to audition notes on one step |
| `:set step=1` | Back to the start setting |

## Scales

Set a scale and the home row plays it, with `a` on the root, going up through
the scale from `a` to `;`. The row above is silent, so every key is in key. The
grid still shows real note names.

```
 a s d f g h j k l ;
 1 2 3 4 5 6 7 1 2 3     scale degrees, in a seven-note scale
```

| Keys | Does |
|---|---|
| `:set scale=dorian` | Choose the kind, and turn it on |
| `:set root=d` | Choose the root, like `F#` or `Bb`, and turn it on |
| `:scale` | Choose the kind from a list |
| `Option-s` | Off and on, in Normal and Insert, keeping both |
| `:set scale=off`, `on` | The same as a command |
| `:scale fit` | Move this phrase's notes to the nearest in the scale |

A kind with two words takes a dash, as in `:set scale=harmonic-minor`. If two notes
are as near, `:scale fit` takes the lower.

While it's on, `=` `-` `>` `<` move a note through the scale, and a count moves that
many notes along it. From a note outside the scale, a step lands on the next one in.
`+` and `_` are still an octave, and `C-a` and `C-x` are still a semitone, for a note
outside the scale on purpose.

A scale is only for entering and moving notes. It changes nothing you've written,
so it can't change how a song sounds, unless you run `:scale fit`. Notes outside it
show in a different colour, and the status line shows `Scale D Dor`. It's saved with
the song, and `u` doesn't undo it.

A chain's `Transpose` plays a phrase higher or lower whatever the scale, so the grid
judges the notes as written.

## Velocity while you play

Every note you write gets the velocity on the status line, as `Vel`, and you
hear it as you type. Velocity runs from `00` to `7F`, the same range as MIDI.
It starts at `64`, which leaves room above it for louder hits. Press `c` and
`v` to go down or up by the top digit: `54`, `44` and so on to `04`, or `74`
then `7F`, which is full. Like the octave, the setting isn't saved with the
project.

`Backspace` clears a note with the velocity it was written with. A `--` in the Velocity column plays at full.

For how loud a whole track is, use `:tvol`. See **Phrases and chains**.

## The other columns

Press `l` in Normal mode to move to another column, then `i`:

| Column | In Insert |
|---|---|
| Velocity | Two hex digits up to `7F`, such as `40`, then it moves down |
| Gate | A gate and its pulses, such as `HLD3`, or a digit alone |
| FX | A label, `Enter`, then a value, like `CUT` `Enter` `40`. See **FX commands** |

## Gates

The gate column, after velocity, says how long a row lasts and what its note does
over that time. Its argument is always the row's length in pulses, which are
steps, from `0` to `8`. An empty cell, `----`, is one pulse of `SGL`, as every row
used to be.

| Gate | Its pulses |
|---|---|
| `SGL` | One note, on the first |
| `MLT` | A note on each |
| `HLD` | One note, tied into the next row's note with no new attack |
| `RST` | No note, and it ends one that's sounding |

Any gate with `0` pulses skips the row: it takes no time and plays nothing. A gate on
a row with no note still sets how long the row lasts, so `RST4` there is four steps
of nothing. `K` chooses the gate, `=` and `-` change the pulses, and `x` empties
it. The row's FX run on its first pulse, though a `MLT` runs its `DEL`, `KIL`,
`OFF` and `RET` on each, and lanes keep running on every step. A beat row is one
that starts on a beat, however long the rows before it.
