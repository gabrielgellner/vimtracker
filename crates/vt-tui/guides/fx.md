# FX commands

Each step has FX slots to the right of the Velocity column. A slot is a
command and a two-digit hex value.

`K` on a slot lists the commands this track has, grouped as an LFO's `Dest` is:
the filter, the output, the voice, each slot's effect, then the events. It opens
searching, so `send b` or `decay` finds its command, and `Enter` takes it. Type
the value next.

To type one instead, press `i` on the slot, type its label, such as `CUT` or
`2FDBK`, and press `Enter`, then type the value, such as `40`. A label that isn't
on the track opens `K`, searching for what you typed. A slot only takes effect
once its value is typed. The status line names what the cursor is on, such as
`Filter: Cut`.

## Values and their marks

| Command | Sets |
|---|---|
| `VOL` | Track gain from the pattern. `E0` is 0 dB, and starts there |
| `PAN` | Pan: `00` is left, `80` centre, `FF` right |
| `CUT` | Filter cutoff, from 20 Hz to 20 kHz |
| `RES` | Filter resonance |
| `SNA` | Send to return `A` |
| `SNB` | Send to return `B` |
| `PNCH` | A voice's setting, by its own label, like Punch on `Kick` |
| `2FDBK` | A slot's setting, by the slot's digit and its label, below |

Each voice has its own settings, and the track keeps every voice's values, so
changing voice and back loses nothing. A lock for a voice the track isn't
playing, or an effect no longer in its slot, is drawn faint and does nothing
until its voice or effect is back.

A slot's settings can be set from a step, the same way as the rest, while the
slot holds an effect. Each is its slot's digit and a short label, like `1AMNT`
or `2FDBK`, and `K` lists them under the effect's group with the names they have
in an LFO's `Dest`, like `1 Amount`. `Bits` takes whole bits, so `06` is 6 bits,
and `Type` takes its place in the list, from `00`. A step's setting belongs to
that effect, so a `2FDBK` lock does nothing once slot 2 holds a Reverb.

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
