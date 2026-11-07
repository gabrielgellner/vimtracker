# FX commands

Each step has FX slots to the right of the Velocity column. A slot is a
three-letter command and a two-digit hex value.

To write one, go to the slot with `l` in Normal mode, press `i` and type it,
such as `CUT40`. The status line says `FX1 command`, then `FX1 value`. A slot
only takes effect once its value is typed.

## Values and their marks

| Command | Sets |
|---|---|
| `VOL` | Track gain from the pattern. `E0` is 0 dB, and starts there |
| `PAN` | Pan: `00` is left, `80` centre, `FF` right |
| `CUT` | Filter cutoff, from 20 Hz to 20 kHz |
| `RES` | Filter resonance |
| `SNA` | Send to return `A` |
| `SNB` | Send to return `B` |
| `HAR` | A synth's first setting, shown by its name on that voice |
| `TIM` | A synth's second setting |
| `MOR` | A synth's third setting |
| `1 Amount` | A setting in a slot on the FX page, below |

A slot's settings can be set from a step, the same way as the rest. `K` on a command lists them
while the slot holds something, under the names they have in an LFO's `Dest`, like `1 Amount`, or
type the slot's number and the first letters, like `1am`. `Bits` takes whole bits, so `06` is 6 bits,
and `Type` takes its place in the list, from `00`. A step's setting belongs to the slot and its place
in the effect's list, so it reads as whatever's now there, and does nothing in an empty slot.

`VOL`, `SNA` and `SNB` are on the level scale in **Phrases and chains**. The
others start from the values on the track page, `Space 4`, each time playback
starts from the top. An LFO on that page keeps moving a value around wherever
a step has put it. See **The track page, envelopes and LFOs**.

Each value has a mark beside it, which says how long it lasts:

| Mark | Lasts |
|---|---|
| `.` lock | The default. Until the next note or the next value of that command |
| `h` hold | Until playback next starts from the top. A lock returns to this |
| `g` glide | Slides to the next value of that command in the slot |

A glide wraps through the phrase's loop to find its value.

Press `~` on a value to cycle its mark, or in Visual to give every value selected
the mark after the first one's, and `.` repeats it. A value typed into an empty slot copies the mark
of the nearest value above it.

A note that a failed `CHA` skips, an `OFF` and a `RET` don't end a lock, as the same note is
still sounding. A note doesn't stop a glide, so a sweep can cross notes, and a glide's last row
uses its own mark, so ending on a lock jumps back at the next note. With no other value to
slide to, a glide stays where it is, as a lock would. An edit on the track page changes what's
underneath, so you hear it straight away unless a lock or glide is on top.

## Things that happen once

These have no mark.

| Command | Does |
|---|---|
| `DEL` | Delay this step by `xx` units |
| `KIL` | Cut the note after `xx` units |
| `OFF` | Start the envelopes' release after `xx` units |
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

Swing follows the song's grid rather than each track's loop, so a track with
an odd length swings different steps each time round.
