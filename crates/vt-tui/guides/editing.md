# Selecting and editing

Editing follows Vim's grammar: an operator, then what it covers.

## Operators

| Keys | Does |
|---|---|
| `d` | Clear |
| `y` | Copy |
| `>` `<` | Shift up or down |

Follow one with a motion or a target, or double it for this whole step:

| Keys | Does |
|---|---|
| `dd` | Clear this step |
| `dj` | Clear this column here and on the next step |
| `d4j` | Clear this column here and on the next 4 steps |
| `yy` | Copy this step |
| `dib` | Clear this beat, whole steps |
| `dic` | Clear this column, whole phrase |
| `dip` | Clear this phrase, whole steps |
| `d$` | Clear this step from this column to the end of the row |
| `d0` | Clear this step from the start of the row to this column |

## Selecting

| Keys | Does |
|---|---|
| `v` | Select fields, starting from this one |
| `V` | Select whole steps |
| `W` `B`, `]b` `[b` | In a selection, reach the next or previous track |
| `gv` | Select the last selection again |
| `Esc` | Stop selecting |

Move to grow the selection. The cursor shows which end is moving. Then press
`d`, `x`, `y`, `>`, `<`, `-`, `=`, `_`, `+`, `r`, `S` or `.`.

`r` fills the cursor's column across the selection, as one undo: `vGra` writes
a C on every step to the end of the loop. Notes already there keep their
velocity. A velocity only goes on steps with a
note, an FX value only on steps with a command, and `.` repeats it.

## Across tracks

In `v` or `V`, `W` and `B` reach other tracks, so `VGWWW` covers a whole drum
part. `d`, `y`, `p`, `>`, `<`, `=`, `-`, `+`, `_`, `r` and `~` then act on each of them as one
undo. Rows count from each phrase's own loop start, as on screen. `p` puts each track's part on the track the
same distance along from the cursor.

## Shaping a column

Select part of a Velocity or FX column with `v`, then press `S` for a panel
beside the grid. The column changes as you adjust it, and you hear it if the
loop is playing. `Enter` keeps it as one undo, and `Esc` puts everything back.

| Row | Does |
|---|---|
| Fill | `Ramp` between the ends, `Random`, `Wave` or `Alternate`. `r` picks anew |
| From, To | The ends. `~` swaps them |
| Curve | Bends a ramp. Above 0 it moves early, and below 0 late |
| S | Eases a ramp's ends. Above 0 makes an S, and below 0 the reverse |
| Where | Which rows change, with `Amount` below it for some |

`Wave` swings between the ends, for movement like a pulse across a hat's
velocities, and shows these in place of Curve and S:

| Row | Does |
|---|---|
| Shape | `Sine`, `Tri`, `Saw`, `Ramp` or `Square`, as on an LFO |
| Rate | How long one swing takes, like `1/2`, `1/8d`, `2 bars` or `7st` |
| Phase | Where in the swing the first row selected falls, in degrees |

Each row's value comes from where it falls in time, counting from the first row
selected, so the movement keeps its timing however the notes are spaced. As on an
LFO, `Sine` and `Tri` start half way and head for `To`.

`Alternate` gives the rows it changes one value and then the other, so every
other hit flips however they're spaced. `Run` sets how many rows each value gets
in a row: `1` gives `70 40 70 40` and `2` gives `70 70 40 40`.

The ends start as the first and last values selected, or if those match, the
lowest and highest.

| Where | Rows |
|---|---|
| Notes | Rows with a note |
| Commands | Rows already holding this FX command |
| All rows | Every row |
| No notes | Rows without a note |
| Every | Every so many rows, counting from the first selected |
| Chance | Each row with this percent chance |

The ramp's ends land on the first and last rows it changes, and rows keep their
spacing along it. A velocity only ever goes on a note, so for Velocity, Every
and Chance choose among the notes. On an FX column, the command is the first one
selected, and a row holding a different command keeps it.

The panel uses the track page's keys: `j` `k`, `-` `=` `_` `+`, `i` to type, `K`
to choose from a list and `x` for the default. `Tab` still plays and pauses.
`.` applies the same shape to another selection. A ramp or random fill takes that
selection's own ends, and `Wave` and `Alternate` keep theirs.

## Values and small edits

| Keys | Does |
|---|---|
| `x` | Clear this field |
| `r` | Replace this field, as in Vim: no move, and back to Normal |
| `s` | Clear this field and start Insert there, as in Vim |
| `=` `-` | Up or down a small step. `C-a` and `C-x` do the same |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `M-l` `M-h` | Option and `l` `h`: as `=` `-`, without moving |
| `M-k` `M-j` | Option and `k` `j`: as `+` `_`, without moving |
| `K` | On an FX column, choose from a list |
| `.` | Repeat the last change |
| `p` | Paste over, from the cursor down. Past the end it stores more |
| `3p` | Paste 3 times, end to end, as one undo |
| `gp` | Paste over, and land just after it, ready for the next |
| `gm` | Merge: empty cells in the copy leave what is there alone |
| `"a` | Use register `a`, before `y`, `d` or `p`. `a` to `z` all work |
| `"0` | The last thing you copied with `y`, which a cut can't replace |
| `u` `C-r` | Undo and redo |

A small step is a semitone, or 1 on a value. A large step is an octave, or `10`
in hex, which changes the left digit. Counts work: `3=`. `RET` is different,
since its digits mean separate things: `=` `-` change only the right digit,
the rate, and `+` `_` only the left, the fade, so one never carries into the
other.

`K` on an FX column lists the commands with what each does. If the slot is
empty, type the value next. If it has one, it stays.

`r` then `k` changes a note to high C. On other columns it takes a whole value:
`r40` for a velocity, or `rCUT40` for an FX slot. A
note keeps its velocity, `.` repeats the replace, and `Esc`
cancels it. The cursor and `REPLACE` on the status line turn red while it
waits.

`s` clears the field, and then typing works as in any Insert, moving down
after each entry. On an FX value it keeps the command, so `s60` on `CUT40`
makes `CUT60`.

## The loop

A phrase can store more than it plays, like a clip. The loop is the part
that plays, and everything else stays dim and silent until you move the loop
over it. You can still edit, copy and paste out there.

Rows count from the start of the loop. `00` is always beat 1, and rows above
it are negative, so yellow beats are always `00`, `04`, `08`.

| Keys | Does |
|---|---|
| `]l` `[l` | Slide the loop a step later or earlier |
| `]L` `[L` | Slide it a beat later or earlier. Counts work: `4]L` |
| `gs` | Start the loop on the cursor's step |
| `ge` | End the loop on the cursor's step |
| `]r` `[r` | Rotate the loop's steps later or earlier |
| `:len 32` | Set how many steps the loop plays. Or in hex: `:len 0x20` |
| `:double` or `Space pd` | Repeat the loop so it is twice as long |

Sliding works while playing, and stays in time. `00` stays where it is and
the notes move under it. Sliding past either end adds empty steps.

Rotating wraps whole steps round within the loop, and leaves dim steps
alone. It works while playing, and counts work: `3]r`.

`ge`, `:len` and `:double` work while playing too. Each track stays on the row it was going
to play next, or starts its chain again if that row has gone. The loop is saved with the
phrase, along with what is outside it.

A phrase used in two places shares one loop. Copy it with `gc` to give one
place its own.

## Lanes

An FX column can loop on its own, apart from the notes: a lane. It keeps
counting across the phrase's repeats, so a lane of 3 against a loop of 16 lands
somewhere new each pass, as tracks of different lengths do. A column follows
the notes until you give it a lane, so changing the phrase's loop changes it too.

| Keys | Does, on an FX column |
|---|---|
| `:lane 3` | Make it a lane of 3 steps, from the cursor's step |
| `:lane` | Follow the notes again |
| `gs`, `ge`, `:len 3` | On a lane, set its start, end or length |

On a column without a lane, and on the note and velocity columns, `gs`, `ge`
and `:len` set the phrase's loop. On a lane, `gg` and `G` go to its first and
last steps, and `5G` to its fifth. A lane shows its length in the title, like
`FX1:3`, dims the steps outside it, and has its own `>` while playing. Sliding
the loop with `]l` moves lanes with it. Editing works on rows as usual,
whatever loop a column plays. Anything that starts a track's chain over starts
its lanes over too, so playing from a step hears what playing to it would. A
glide in a lane goes to the next value in that lane.

`gg`, `G`, `{`, `}`, `ib`, `ic` and `ip` go by the loop. `5G` means row `04`.
`j` and `k` carry on past its edges. `Space c` on a dim row plays from `00`, and
`;` there plays just that row's note.

