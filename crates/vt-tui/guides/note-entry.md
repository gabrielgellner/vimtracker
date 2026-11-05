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
| `Enter` | Play from the cursor, or stop, and stay in Insert |
| `Esc` | Back to Normal mode |

Our octave numbers are one above Live's: `a` starts on `C-4`, the same pitch
Live calls C3.

Notes use the current instrument, shown as `Inst` on the status line. Change
it with `:inst 1A`, or by label with `:inst bd`.

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

Run `:set skip=8` to move 8 steps instead of a beat, for a kick on every other
beat. Anything half typed in another column is dropped when you move.

## Writing every few steps

Set how far each note moves, which suits drums.

| Command | Does |
|---|---|
| `:set step=4` | Each note moves 4 steps, so one on every beat |
| `:set step=2` | Every eighth note |
| `:set step=0` | Stay put, to audition notes on one step |
| `:set step=1` | Back to the start setting |

## Velocity while you play

`Vel --` means notes leave the Velocity column alone, and an empty column
plays at full. Press `c` to start at `E0` and go down in steps of `20`,
stopping at `20`. Press `v` to go back up, and above `E0` it returns to `--`.
You hear a note at the instrument's volume as you type it, and with its
velocity when it plays back.

For how loud a whole track is, use `:tvol`. See **Phrases and chains**.

## The other columns

Press `l` in Normal mode to move to another column, then `i`:

| Column | In Insert |
|---|---|
| Velocity | Two hex digits, such as `80`, then it moves down |
| Instrument | Two characters, a number or a label, such as `04` or `bd` |
| FX | A name then two hex digits, such as `CUT40`. See **FX commands** |
