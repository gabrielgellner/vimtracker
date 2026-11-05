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
`d`, `x`, `y`, `>`, `<`, `+`, `-` or `.`.

## Values and small edits

| Keys | Does |
|---|---|
| `x` | Clear this field |
| `r` | Replace this field, as in Vim: no move, and back to Normal |
| `s` | Clear this field and start Insert there, as in Vim |
| `+` `-` | Change the value up or down. `C-a` and `C-x` do the same |
| `.` | Repeat the last change |
| `p` | Paste over, from the cursor down |
| `"a` | Use register `a`, before `y`, `d` or `p`. `a` to `z` all work |
| `u` `C-r` | Undo and redo |

`r` then `k` changes a note to high C. On other columns it takes a whole value:
`r40` for a velocity, `rbd` for an instrument, or `rCUT40` for an FX slot. A
note keeps its velocity and instrument, `.` repeats the replace, and `Esc`
cancels it. The cursor and `REPLACE` on the status line turn red while it
waits.

`s` clears the field, and then typing works as in any Insert, moving down
after each entry. On an FX value it keeps the command, so `s60` on `CUT40`
makes `CUT60`.

## Phrase length

| Command | Does |
|---|---|
| `:len 32` | Set the phrase length |
| `:double` or `Space pd` | Repeat the phrase so it is twice as long |
