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
| `M-l` `M-h` | Option and `l` `h`: as `=` `-`, without moving |
| `M-k` `M-j` | Option and `k` `j`: as `+` `_`, without moving |
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
| `dd` | Take the effect out of this slot |
| `p` | Put it in this slot, swapping with what's there |

A slot's settings are in columns of two. `j` and `k` go up and down them and on to the
next slot, and on the effect they go straight between slots. `2j` goes to the same place as `}`.

`dd` then `p` is one undo step, and an effect can move to another track's slot too. Modulation and
step commands stay with the slot, so they reach whatever's there now.

`Distort` has a `Type`: `Soft` and `Hard` clip, `Fold` and `Sine` fold the sound back
on itself, `Crush` lowers its bit depth, and `Down` lowers its sample rate. `Amount` is
how much, and on `Crush` it's `Bits`, whole numbers from `12` to `2`, where anything
quieter than half a step drops out. Low settings gate quiet sounds, so one press back
brings them in, and `Mix` blends it with the sound going in, from `00` dry to `FF` wet. `Gain`
turns down what comes out, as `Amount` can make it much louder, on the level scale,
where `E0` leaves it as it is and every `10` is 4 dB. Every effect has one.

An LFO can move another: `LFO 1`'s `Dest` has `L2 Depth` and `L2 Phase`, and an envelope's has
both LFOs', so `Env 2` can bring in an LFO's depth with each note. It only goes one way, so
`LFO 2` can't reach `LFO 1`. `Rate` can't be moved, as LFOs work out where they are from the
song's position.

`K` on `Dest` shows it by group, opening on what's chosen:

| Keys | Does |
|---|---|
| `j` `k` | Move in the list, or choose a group |
| `h` `l` | Go to the groups or back to the list |
| `/` | Go to the search and back. It reaches every group |
| `Enter` | Choose, or on a group, go into it |

`Dest` and step commands only list what's loaded: a synth's settings under their names on
that voice, on a track playing it. One already chosen stays, reads like `Har ---`, and does
nothing until it's back.

`Delay` and `Reverb` can go in any slot, starting half wet. Their tails carry on after a
note ends. `Reverb` has these:

| Setting | Does |
|---|---|
| `Pre` | How long before it comes in, up to a quarter second |
| `Decay` | How long it rings. `FF` goes on for a long time |
| `Damp` | How quickly its highs fall away as it rings |
| `Low cut`, `High cut` | Shape what goes in |

`Delay` has these:

| Setting | Does |
|---|---|
| `Time` | A note division, up to `1 bar`. A change glides |
| `Feedback` | How much comes round again. `FF` keeps going |
| `Ping-pong` | `On` goes side to side, and `Off` stays put |
| `Width` | From `00`, sides together, to `FF`, fully apart |
| `Low cut`, `High cut` | Take more from each repeat |
| `Drive` | Saturates louder repeats each time round |

Every setting in a slot can be moved: an LFO's or envelope's `Dest` lists them while the
slot holds something, like `1 Amount`, and a step's FX slot can set them too. The bars
follow what's playing. A setting that's whole values, like `Bits`, just steps.

## Output

`Pan`, `Send A` and `Send B` are where those values start when
playback starts from the top. A step's `h` value changes them until the next start,
and changing one here is heard straight away unless a lock or glide is on top. `Level` scales the whole track.
Dimmed `Swing` is the project's, which this track follows. Changing it gives the
track its own, and `x` follows the project again.

## Returns and the master

Every song ends with three fixed tracks, labelled `A`, `B` and `M`. `W`, `B` and the
track list go through them, and `:track add` and `:track del` work before them, so they
always stay last.

`A` and `B` are returns, as in Ableton: what a track's `Send A` and `Send B` send plays
through their slots, and comes out alongside the track. A new song has a `Delay` in
`A`'s first slot and a `Reverb` in `B`'s, each with `Mix` at `FF`, so you only hear
what comes back. You can change them, and with nothing in its slots a return just
passes on what it's sent. `M` is the master: everything goes through it, and then the
limiter.

They make no sound of their own, so they have no machine or filter, and their Sound
page has just `Level` and `Pan`, and the master only `Level`. The rest works as on any
track: 3 slots on the FX page, the Mod page, a strip on the mixer, and phrases. To give
one a phrase, type a chain number in its column on the Song view. A note on one plays
nothing, but it does start the track's envelopes. Steps on a return only reach that
return, so `VOL` on `B`'s phrase rides the reverb.

Soloing a track keeps the returns, so you hear what it sends. Soloing a return plays
only what comes back from it. Muting a return silences what comes back, and the master
can't be muted or soloed.

## Envelopes

Each track has two, which start with each note. The page draws their shapes.
Time is stretched in the drawing, so a short attack still shows, and a stage's
width depends only on its own time.

| Setting | Value |
|---|---|
| Dest | `Vol` `Cut` `Res` `Pan` `SendA` `SendB` `Pitch`, or `---` for off |
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
| Dest | `Cut` `Res` `Pan` `Vol` `SendA` `SendB` `Pitch`, or `---` for off |
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

`Vol`, `SendA` and `SendB` swing in dB, on the level scale in **Phrases and
chains**, so `Depth +40` on `Vol` is 16 dB each way. They stop at `F8`, which
is 6 dB above where `VOL` starts, so a deep swing stays even if you lower `VOL`
on a step.

## The mixer

Press `Space 6` for a strip for each track: its name, a meter of what it's playing
with `L` and `R` bars, a fader beside them showing its `Level`, then its `Pan`,
`Send A`, `Send B`, mute and solo. The meters are on the same scale as the main one,
with `0` marked, and show the track after its level and pan. `h` and `l` move between
tracks, `j` and `k` up and down the strip, and values edit as they do on the track page. On mute and solo, `=` switches on,
`-` switches off, and `K` or `i` changes it. `/` jumps to a setting by its name, as
it does on the track page, staying on the cursor's track.
