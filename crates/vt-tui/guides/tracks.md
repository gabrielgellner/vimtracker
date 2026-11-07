# The track page, envelopes and LFOs

Press `Space 4` for what a track holds apart from its steps, on three pages: Sound
has its machine, filter and output, with a bar beside each value showing where
it sits between the lines at each end of its range, and moving with whatever
plays it, with a dim mark left where it's set, Mod its envelopes and
LFOs, and FX its effects. `H` and `L` go to the previous or next page. FX columns belong
to steps, and an LFO keeps moving on its own, like a filter drifting under a
hat pattern.

| Keys | Does |
|---|---|
| `H` `L` | The Sound page or the Mod page |
| `W` `B`, `]b` `[b` | Next or previous track |
| `j` `k` | Next or previous row |
| `h` `l` | Previous or next setting. `0` and `$` go to the ends |
| `/` | Jump to a setting by its name |
| `=` `-` | Up or down a small step. A count goes further, as in `10=` |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `i` | Type a value. Hex levels enter themselves, others take `Enter` |
| `K` | On a setting with choices, like Machine or Dest, list them |
| `x` | Back to its default: centre pan, 0 dB and so on |

## The filter

| Setting | Value |
|---|---|
| Cut, Res | Where `CUT` and `RES` start |
| Drive | How hard it's driven, from `00` for clean |
| Filter | `LP` `HP` `BP` `Notch` `Ladder`. `K` lists them |
| Low cut, High cut | The utility filter: `00` and `FF` are off |

Drive keeps the level about even. `Low cut` and `High cut` are a second, simple
filter after the main one, for thinning out a sound without giving up the main
filter's character. They run from 20 Hz at `00` to 20 kHz at `FF`, at 12 dB with
no resonance. `Low cut 00` and `High cut FF` are off, and off costs nothing.
Unlike `Cut`, steps and modulators don't move them.

`LP` is the clean default. `HP` needs `Cut` lowered to let anything through, as
it starts at `FF`. `Ladder` is fatter and steeper, its bass thins as resonance
rises, and near `FF` it sings by itself while a note lasts. It costs more than
the others, which matters only for tracks using it.

In kitty, a curve in a box under the filter's rows shows what the filter and utility filter
let through, from 20 Hz on the left to 20 kHz on the right. It moves with what's
playing `Cut` and `Res`, including step commands, LFOs and envelopes, but it
doesn't show `Drive`'s saturation.

## Jumping to a setting

Press `/` and type part of a setting's name, like `cu`, and each match on this page
gets a label over its value, and again in its name, just after what you've typed. Press a label to land on
that value, ready for `=` and `-`. `Enter` goes to the nearest match, `Esc` stays where you were, and `Ctrl-O`
comes back after a jump.

A label is never a letter that could carry on your search, so you can keep typing to
narrow it, and a match keeps its label as you do. `/` searches the page you're on,
and `H` and `L` change page.

## Effects

The FX page has 3 slots, which the sound goes through in order after the filters and
before `Level` and `Pan`. An empty slot does nothing and costs nothing.

| Keys | Does |
|---|---|
| `K` or `i` on the effect | Choose what's in the slot. `=` and `-` step through |
| `x` on the effect | Empty the slot |
| `x` on a setting | Back to its default |
| `{` `}` | Previous or next slot, staying on the same setting |

A slot's settings are in columns of two. `j` and `k` go up and down them and on to the
next slot, and on the effect they go straight between slots. `2j` goes to the same place as `}`.

`Distort` has a `Type`: `Soft` and `Hard` clip, `Fold` and `Sine` fold the sound back
on itself, `Crush` lowers its bit depth, and `Down` lowers its sample rate. `Amount` is
how much, and on `Crush` it's `Bits`, whole numbers from `12` to `2`, where anything
quieter than half a step drops out. Low settings gate quiet sounds, so one press back
brings them in, and `Mix` blends it with the sound going in, from `00` dry to `FF` wet. `Gain`
turns down what comes out, as `Amount` can make it much louder, on the level scale,
where `E0` leaves it as it is and every `10` is 4 dB. Every effect has one.

Every setting in a slot can be moved: an LFO's or envelope's `Dest` lists them while the
slot holds something, like `1 Amount`, and a step's FX slot can set them too. The bars
follow what's playing. A setting that's whole values, like `Bits`, just steps.

## Output

`Pan`, `Delay` and `Reverb` are where those values start when
playback starts from the top. A step's `h` value changes them until the next start,
and changing one here is heard straight away unless a lock or glide is on top. `Level` scales the whole track.
Dimmed `Swing` is the project's, which this track follows. Changing it gives the
track its own, and `x` follows the project again.

## Envelopes

Each track has two, which start with each note. The page draws their shapes.
Time is stretched in the drawing, so a short attack still shows, and a stage's
width depends only on its own time.

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
attack starts slowly.

## LFOs

Each track has two. An LFO moves around the current value, so `CUT40` on a
step moves the centre and the LFO carries on around it.

| Setting | Value |
|---|---|
| Dest | `Cut` `Res` `Pan` `Vol` `Dly` `Rev` `Pitch`, or `---` for off |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` `Smooth` |
| Rate | The length of one cycle. `=` and `+` are always faster |
| Depth | How far it swings each way, in hex. Negative flips it |
| Phase | Where in the cycle it starts, in degrees |
| Retrig | `on` restarts it with each note on this track |

With `Cut 80` and `Depth +40`, the filter moves between `40` and `C0`. `Saw`
falls and `Ramp` rises. `Random` picks a new value each cycle, and `Smooth` eases
from one to the next, which suits a slow drift on `Cut`.

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

## The mixer

Press `Space 6` for a strip for each track: its name, a meter of what it's playing
with `L` and `R` bars, a fader beside them showing its `Level`, then its `Pan`,
`Delay`, `Reverb`, mute and solo. The meters are on the same scale as the main one,
with `0` marked, and show the track after its level and pan. `h` and `l` move between
tracks, `j` and `k` up and down the strip, and values edit as they do on the track page. On mute and solo, `=` switches on,
`-` switches off, and `K` or `i` changes it. `/` jumps to a setting by its name, as
it does on the track page, staying on the cursor's track.
