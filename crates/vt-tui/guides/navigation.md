# Navigation

These work in Normal mode. If `j` is writing notes, you are in Insert, so
press `Esc` first. The status line shows your mode, and the column you are
on beside it.

## Moving around a phrase

| Keys | Does |
|---|---|
| `j` `k` | Next or previous step |
| `h` `l` | Previous or next field |
| `w` `b` | Next or previous group of columns |
| `0` `$` | Start or end of the row: the Note column, or the last FX value |
| `}` `{` | Next or previous beat, which is 4 steps |
| `gg` `G` | First or last step |
| `H` `M` `L` | Top, middle or bottom of the screen |
| `C-d` `C-u` | Half a screen down or up |
| `C-e` `C-y` | Scroll the view without moving the cursor |

## Counts

A number before a motion repeats it, as in Vim.

| Keys | Does |
|---|---|
| `4j` | Down 4 steps |
| `2}` | Down 2 beats |
| `8G` | To step 8, counting from the top in decimal |

Step numbers on screen are hex, so step `10` on screen is the 17th step.

## Moving between things

| Keys | Does |
|---|---|
| `W` `B` | Next or previous track |
| `]p` `[p` | Next or previous phrase in the chain |
| `]P` `[P` | Last or first phrase in the chain |
| `Space 2` | Chain view |
| `Space 3` | Phrase view |
| `Space 5` | Instrument view |
| `Space Space` | Pick a view. `Enter` flips back to the last one |
| `C-o` `Tab` | Jump back or forward, like Vim's jump list |

## When you forget

Press `Space` or another prefix and wait: a popup lists what can come next.
`Space k` lists every key, and `Space ?` opens these guides.
