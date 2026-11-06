# The track page, envelopes and LFOs

Press `Space 4` for what a track holds apart from its steps. FX columns belong
to steps, and an LFO keeps moving on its own, like a filter drifting under a
hat pattern.

| Keys | Does |
|---|---|
| `W` `B` | Next or previous track |
| `j` `k` | Next or previous row |
| `h` `l` | Previous or next setting. `0` and `$` go to the ends |
| `=` `-` | Up or down a small step. A count goes further, as in `10=` |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `i` | Type a value. Hex levels enter themselves, others take `Enter` |
| `K` | On Dest, Shape, Rate, Retrig or Reset, choose from a list |
| `x` | Back to its default: centre pan, 0 dB and so on |

## Filter and output

`Cut`, `Res`, `Pan`, `Delay` and `Reverb` are where those values start when
playback starts from the top. A step's FX changes them until the next start,
and changing one here is heard straight away. `Level` scales the whole track.
Dimmed `Swing` is the project's, which this track follows. Changing it gives the
track its own, and `x` follows the project again.

## Envelopes

Each track has two, which start with each note. The page draws their shapes.
Time is stretched in the drawing, so a short attack still shows.

| Setting | Value |
|---|---|
| Dest | `Vol` `Cut` `Res` `Pan` `Dly` `Rev` `Pitch`, or `---` for off |
| Attack, Decay | Milliseconds, up to 8000 |
| Sustain | The percent it holds at while the note lasts |
| Release | Milliseconds, after the note ends |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | `on` starts from zero on each note. `off` goes on from there |

`Env 1` starts on `Vol` and shapes each note, so it has no depth. `Env 2` starts
on `Cut` with no depth, so raise its depth to open the filter on each note. On
`Pitch`, depth is in semitones and in hex, so `+0C` is an octave.

A note lasts until the next note on its track or an `OFF`, and the next note
fades the last one quickly rather than running its release. So a drum is often
a Decay with Sustain at `0%`, and Release only runs on an `OFF`. To make notes
shorter, use `OFF` on a step or the `OFF` command.

Decays and releases fall fast and then slow, as an analog envelope does, and an
attack starts slowly. Tracks with several instruments share these shapes.

## LFOs

Each track has two. An LFO moves around the current value, so `CUT40` on a
step moves the centre and the LFO carries on around it.

| Setting | Value |
|---|---|
| Dest | `Cut` `Res` `Pan` `Vol` `Dly` `Rev` `Pitch`, or `---` for off |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` |
| Rate | The length of one cycle. `=` and `+` are always faster |
| Depth | How far it swings each way, in hex. Negative flips it |
| Phase | Where in the cycle it starts, in degrees |
| Retrig | `on` restarts it with each note on this track |

With `Cut 80` and `Depth +40`, the filter moves between `40` and `C0`. `Saw`
falls and `Ramp` rises. `Random` picks a new value each cycle.

| Type this as a rate | For |
|---|---|
| `1/4` `1/8d` `1/16t` | A note division, dotted or triplet |
| `2 bars` | Whole bars |
| `7st` | A number of steps, to slide against the bar |
| `2.5hz` | Free of the tempo, from `0.01hz` to `100hz` |

A large step is `10` in hex, 5% of swing, or 90° of phase. On a rate it skips
dotted and triplet divisions, moves 4 steps, or doubles or halves the hertz.

`i` on Dest, Shape or Retrig opens the list too. `K` on Rate lists the note
divisions and bars, and `i` is for anything else, like `7st` or `2.5hz`.

## When an LFO restarts

Playing from the top restarts every LFO. A `:reset` doesn't, since that is
about patterns. To follow the steps, turn on `Retrig`. While stopped, a track
sits at its held values, so `;` hears those. Pausing freezes the LFO.

`Vol`, `Dly` and `Rev` swing in dB, on the level scale in **Phrases and
chains**, so `Depth +40` on `Vol` is 16 dB each way. They stop at `F8`, which
is 6 dB above where `VOL` starts, so a deep swing stays even if you lower `VOL`
on a step.
