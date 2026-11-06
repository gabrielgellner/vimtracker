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
| `gv` | Select the last selection again |
| `Esc` | Stop selecting |

Move to grow the selection. The cursor shows which end is moving. Then press
`d`, `x`, `y`, `>`, `<`, `-`, `=`, `_`, `+`, `r`, `S` or `.`.

`r` fills the cursor's column across the selection, as one undo: `vGra` writes
a C on every step to the end of the loop. Notes already there keep their
velocity. A velocity only goes on steps with a
note, an FX value only on steps with a command, and `.` repeats it.

## Shaping a column

Select part of a Velocity or FX column with `v`, then press `S` for a panel
beside the grid. The column changes as you adjust it, and you hear it if the
loop is playing. `Enter` keeps it as one undo, and `Esc` puts everything back.

| Row | Does |
|---|---|
| From, To | The ends. `~` swaps them |
| Curve | Bends the line. Above 0 it moves early, and below 0 late |
| S | Eases both ends. Above 0 makes an S, and below 0 the reverse |
| Fill | `Ramp` between them, or `Random`. `r` picks anew |
| Where | Which rows change, with `Amount` below it for some |

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
`.` applies the same shape to another selection, with its own ends.

## Values and small edits

| Keys | Does |
|---|---|
| `x` | Clear this field |
| `r` | Replace this field, as in Vim: no move, and back to Normal |
| `s` | Clear this field and start Insert there, as in Vim |
| `=` `-` | Up or down a small step. `C-a` and `C-x` do the same |
| `+` `_` | Up or down a large step: the same keys with Shift |
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

`ge`, `:len` and `:double` wait until you pause, since a new length moves where playback
lands. The loop is saved with the phrase, along with what is outside it.

A phrase used in two places shares one loop. Copy it with `gc` to give one
place its own.

`gg`, `G`, `{`, `}`, `ib`, `ic` and `ip` go by the loop. `5G` means row `04`.
`j` and `k` carry on past its edges. `Space c` on a dim row plays from `00`, and
`;` there plays just that row's note.

