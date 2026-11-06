# The track page and LFOs

Press `Space 4` for what a track holds apart from its steps. FX columns belong
to steps, and an LFO keeps moving on its own, like a filter drifting under a
hat pattern.

| Keys | Does |
|---|---|
| `W` `B` | Next or previous track |
| `j` `k` | Next or previous row |
| `h` `l` | Previous or next setting. `0` and `$` go to the ends |
| `+` `-` | Change the value. A count goes further, as in `10+` |
| `i` | Type a value. Hex levels enter themselves, others take `Enter` |
| `K` | On an LFO's Dest, choose from a list |

## Filter and output

`Cut`, `Res`, `Pan`, `Delay` and `Reverb` are where those values start when
playback starts from the top. A step's FX changes them until the next start,
and changing one here is heard straight away. `Level` scales the whole track.
`Swing` overrides the project's. Type `off` to follow it again.

## LFOs

Each track has two. An LFO moves around the current value, so `CUT40` on a
step moves the centre and the LFO carries on around it.

| Setting | Value |
|---|---|
| Dest | `Cut` `Res` `Pan` `Vol` `Dly` `Rev`, or `---` for off |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` |
| Rate | The length of one cycle. `+` is always faster |
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

## When an LFO restarts

Playing from the top restarts every LFO. A `:reset` doesn't, since that is
about patterns. To follow the steps, turn on `Retrig`. While stopped, a track
sits at its held values, so `;` hears those. Pausing freezes the LFO.

`Vol` can only go as high as `FF`. A track at full volume only moves down for
half of each cycle, so hold it lower with `VOL` on a step for an even swing.
