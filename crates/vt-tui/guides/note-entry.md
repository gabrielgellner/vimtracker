# Note entry

Put the cursor on the Note column, as the status line shows, and press `i`.
The keyboard is now a piano, laid out as in Ableton:

```
 w e   t y u   o p      black keys
a s d f g h j k l ;     white keys, a is C
z x c v                 octave and volume
```

So in Insert, `j` is a note. **Each note plays, is written and moves down**
by the step setting.

| Keys | Does |
|---|---|
| `z` `x` | Octave down or up. The status line shows it as `Oct` |
| `c` `v` | Lower or raise the volume written. The status line shows `Vol` |
| `1` | Note off |
| `Backspace` | Clear this note and move up |
| `Esc` | Back to Normal mode |

Our octave numbers are one above Live's: `a` starts on `C-4`, the same pitch
Live calls C3.

Notes use the current instrument, shown as `Inst` on the status line. Change
it with `:inst 1A`.

## Moving without writing a note

The arrows move and write nothing, and you stay in Insert.

| Keys | Does |
|---|---|
| `Up` `Down` | One step |
| `Left` `Right` | Back or forward by the skip setting, at first 4, so a beat |

Run `:set skip=8` to change how far left and right go. Anything half typed in
another column is dropped when you move.

## Writing every few steps

Set how far each note moves, which suits drums.

| Command | Does |
|---|---|
| `:set step=4` | Each note moves 4 steps, so one on every beat |
| `:set step=2` | Every eighth note |
| `:set step=0` | Stay put, to audition notes on one step |
| `:set step=1` | Back to the start setting |

## Volume while you play

`Vol --` means notes leave the Volume column alone. Press `c` to start at `E0`
and go down in steps of `20`, stopping at `20`. Press `v` to go back up, and
above `E0` it returns to `--`. You hear a note at the instrument's volume as
you type it, and at the written volume when it plays back.

## The other columns

Press `l` in Normal mode to move to another column, then `i`:

| Column | In Insert |
|---|---|
| Volume | Two hex digits, such as `80`, then it moves down |
| Instrument | Hex digits, then `Enter` |
| FX | A name then two hex digits, such as `CUT40`. See **FX commands** |
