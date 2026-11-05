# FX commands

Each step has FX slots to the right of the Instrument column. A slot is a
three-letter command and a two-digit hex value.

To write one, go to the slot with `l` in Normal mode, press `i` and type it,
such as `CUT40`. The status line says `FX1 command`, then `FX1 value`. A slot
only takes effect once its value is typed.

## Settings that stay until changed

| Command | Sets |
|---|---|
| `VOL` | Track volume |
| `PAN` | Pan: `00` is left, `80` centre, `FF` right |
| `CUT` | Filter cutoff, from 20 Hz to 20 kHz |
| `RES` | Filter resonance |
| `DLY` | Delay send |
| `REV` | Reverb send |

## Things that happen once

| Command | Does |
|---|---|
| `DEL` | Delay this step by `xx` units |
| `KIL` | Cut the note after `xx` units |
| `OFF` | Start the release after `xx` units |
| `RET` | Retrigger: `xy` repeats every `y` units, and `y` of `0` does nothing |
| `CHA` | Play the note with a chance of `xx` out of `FF` |

A unit is a sixth of a step, so `DEL03` delays by half a step.

For `RET`, `x` from `0` to `7` gets quieter on each repeat and `8` to `F`
gets louder. Use `:seed 7` to choose which results `CHA` gives.

## Swing

| Command | Does |
|---|---|
| `:swing 66.7` | Swing for the project, from 50 to 75 |
| `:tswing 60` | Swing for the cursor's track |
| `:tswing off` | That track follows the project again |
