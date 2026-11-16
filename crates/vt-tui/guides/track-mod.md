# The Mod page, envelopes and LFOs

Each track has six mod slots, `Mod 1` to `Mod 6`, and each slot is a type you choose:
an `ADSR`, an `AHD` or a `Drum` envelope, an `LPG`, an `LFO`, `Steps` or `Track`, in any mix. A new track has three ADSRs, then three
LFOs. The slots work out in order, so a slot can move only those after it, and never
round again.

The page is a rack of devices in two columns, as the FX page is a rack of slots:
`Mod 1` to `Mod 3` down the left, `Mod 4` to `Mod 6` down the right, and the
accumulator beside the arpeggio under them. A return, a group and the master have
no notes to move, so their page ends with the slots.

A slot's title says what it is and where it goes: `Mod 4`, its switch, green while
it's in, its type, its mode (an envelope's curve, how notes start an LFO, or what Track follows), its
polarity, and `→` its `Dest`,
so `Mod 4  LFO  Trig  Bi  → Cut` reads as what it's for. `---` there is dim, as it goes
nowhere. Its settings are in two columns of three, and its shape or cycle is in a
box on its right. The device with the cursor has the brighter frame.

| Keys | Does |
|---|---|
| `h` `l` | Along a row or the title, and on into the device beside it |
| `j` `k` | Down and up the column, `k` from a first row to the title |
| `{` `}` | Up or down a device in the column, on the same setting |
| `0` `$` | The first or last setting on the row, in the device |
| `gg` `G` | The top or bottom of the column |
| `K` | On the type, choose another; on the mode, polarity or `Dest`, list them |
| `x` | Put back the slot's first type, or on the polarity, the type's |
| `dd` | A setting to nothing, as on the Sound page; not the type or `Rate` |
| `b` | Switch this slot out, to hear without it, or back in |

**Changing a type.** `K` on the type lists `ADSR`, `AHD`, `Drum`, `LPG`, `LFO`, `Steps` and `Track`, each with what it
does. Choosing one is one undo step, and the slot keeps its `Dest`, its `Depth` and
its switch. Each slot also keeps every type's own settings, so trying an LFO in
`Mod 2` and going back finds its envelope as it was. What it moves glides from the
old type's to the new one's over 5 ms, so it doesn't click. `=` and `-` don't change
a type, as a type changes what the device is: only `K` does, as only `K` chooses an
effect.

`b` works from any row of a slot, as it does in an FX slot. Switched out, it goes
faint, its switch too, and what it moves glides back to where it would be without
it over 5 ms, so it doesn't click. Its settings stay as they are, and it keeps
running underneath, so an LFO comes back in phase and an envelope where the note
has it. It's saved with the song. A step can't switch one, as a step can an FX
slot, but `DEP4 80` on a step takes `Mod 4` to no depth for its note.

**Polarity.** `Uni` moves what a slot reaches one way only, and `Bi` both ways
around it. An envelope is `Uni` and an LFO `Bi` unless you choose: `=` on the
polarity flips it. An LFO at `Uni` goes from nothing to its depth, for a tremolo
that only dips `Vol`, a vibrato that only bends up, or a filter that only opens. An
envelope at `Bi` is centred on the setting: below it at rest and above it at full,
so on `Pitch` a note can start above itself and settle below. A negative `Depth`
still flips either. The picture's dotted line shows it: a baseline for `Uni`, a
centre line for `Bi`. A chosen polarity stays through a type change, and `x` puts
back the type's own. A step locks it with `POL`, as `POL4 00` for one way.

So `}` goes from `Mod 1` to `Mod 2`, and `3}` from `Mod 4` to the arpeggio. Where
the screen has room for fewer than four rows of devices, the page moves by whole
rows of them to keep the cursor's in view.

## Envelopes

An `ADSR` slot starts with each note. It draws its shape in a box on its right.
Time is stretched in the drawing, so a short attack still shows, and a stage's
width depends only on its own time. The times are down the left, and the levels
and `Reset` down the right.

| Setting | Value |
|---|---|
| Curve | In the title: `Exp`, `Lin`, `Snap` or `Analog`, how its stages bend |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Attack, Decay | Milliseconds, up to 8000 |
| Sustain | The percent it holds at while the note lasts |
| Release | Milliseconds, after the note ends |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**The notes' own shape** is the first envelope on `Vol`, whichever slot that is,
so it has no depth and shows `--`. A new track's is `Mod 1`. Make `Mod 1` an LFO
and the next envelope on `Vol` shapes the notes instead, or with none, they have
the plain shape a new track's have.

**A new track's notes** rise over 1 ms, decay over 600 ms to a sustain of 100%,
release over 50 ms, on `Exp`, with `Reset` on, as Live's Meld starts. At full
sustain the decay does nothing. Lower `Sustain` and each note falls to it over the
decay rather than dropping there as its attack ends, which is a click: with
`Decay` at 0 and `Sustain` at 50%, each note jumps to full for a millisecond and
falls by half at once. Shorten the decay for a pluck.

**Switched out**, from the next note, the notes have no shape at all: the sound
passes through at full from its first frame and through its release, and a note
ends as its machine settles. It doesn't hand the job on to a later envelope. That's
for a machine with an envelope of its own, `4-op`, `6-op` or `Acid`, which then
shapes its notes alone and rings out its own release, where the notes' shape would
cut it off with its own. A machine with none, such as `CSaw` or `VA`, starts at
full and stops dead as its gate closes, as an oscillator with no amp would. A new
note, a choke, `KIL` and stopping still fade a sounding note out.

`Mod 2` starts on `Cut` with no depth, so raise its depth to open the filter on
each note. `Mod 3` starts on `---`, with `Mod 2`'s decay, ready to be given
somewhere to go. On `Pitch`, depth is in semitones and in hex, so `+0C` is an
octave. An envelope's depth can be locked on a step too: `DEP2` for `Mod 2`.

**A time of 0** is no time: a decay or release of 0 is a step, and so is any
envelope's, on `Vol` or anywhere else, so a pitch or a filter can snap. What a
note's level does is smoothed on its way to the sound, where it falls, over about
half a millisecond, as a VCA's gain is, so a step down to nothing doesn't click. A
rise isn't: an attack of 0 is as sudden as it's set, but for a note that starts
under the last one's fade, below.

A note lasts until the next note on its track, an `OFF`, or its gate closing,
and the next note doesn't run the last one's release: it takes over the synth
sounding, below, or starts a voice of its own as the last one fades out over
2.7 ms, as a choke, a `KIL` and stopping fade it. An attack shorter than that fade
is a rise over it instead, so the two notes keep their level between them; a
longer attack plays as it's set, under the last note's fade. So a drum is often a
Decay with Sustain at `0%`, and at `Gate` 100% Release only runs on an `OFF`. To
make notes shorter, lower `Gate` in the Play device, give a row a length of its
own, or put `OFF` on a step.

**The curve** is the envelope's mode, after its type in the title, and bends
every stage at once:

| Curve | Attack | Decay and release |
|---|---|---|
| `Exp` | Starts slowly | Fast, then slow. The default |
| `Lin` | Straight | Straight |
| `Snap` | Starts slowly | Twice as fast at first, then a long tail |
| `Analog` | Starts fast | A capacitor's fall |

Halfway through a decay to nothing, `Lin` is 6 dB down, `Exp` 18, `Analog` 30 and
`Snap` 35, so `Snap` gives a drum or a pluck a sharper snap for the same time.
`Analog` is how a hardware envelope charges and discharges a capacitor: halfway
through its attack it's nearly two thirds of the way up, so a long one swells in
rather than fading up from nothing, and its falls are true RC curves. The picture draws the curve it
plays. `=` and `-` on it go through the four, `K` lists them, and `x` puts back
`Exp`. A step can't lock it.

**Without a reset,** a note starts the attack from where the envelope is, at the
point its curve reaches that level, so the attack keeps its shape and takes only
what's left of its time, and the decay comes that much sooner. A fast run of notes
or a legato line keeps its punch rather than crawling up from halfway each time.

**From note to note,** a synth that holds its notes, as `VA` and `CSaw` do, plays
the next note on the voice already sounding, as a mono synth does: its oscillators
run on, the pitch moves to the new note and the envelope starts again. With the
notes' shape's `Reset` on, as a new track has it, the level falls to nothing over
a short fade first, two thirds of a millisecond on a smooth curve, and the attack
starts from there, so each note is struck, with a small gap before it, without a
click, and a long attack opens from nothing on every note. The gap is the fade and
the start of the attack: about 1.3 ms of sound a note on `Exp`, which starts its
attack slowly, and 1 ms on the `Analog` curve, whose attack starts fast, the
tightest. With `Reset`
off, the attack starts at once from where the level is, as an analog envelope
retriggers, so a run of notes flows into one another with no gap, and a long
attack goes on opening across fast notes. At full sustain the level is then
already at the top, so the same note again sounds as one held note, as it does on
an analog mono synth, and only a new pitch or velocity is heard.

`Reset` only ever acts on a note carried on like that. A sample, the struck voices,
`Kick`, `Snare`, `Hat`, `String`, `Modal`, `Bell` and `Drum`, and `Bowed`, `Blown`
and `Fluted`, whose strike starts their string or bore again from still, start each
note afresh, the last one fading under it, and a fresh note always attacks from
nothing, whatever `Reset` says. So does a synth's note after the last one has died
away, though at `Phase` `Free` its oscillators go on from where they stopped. `Acid` carries on as a 303 does, whatever `Reset` says. The track's `Phase`,
on the Sound page's Play device, says whether the oscillators run on through the
change or start again at the bottom of the dip.

All of this is the track's `Mode` at `Mono`, as a track starts. At `Legato`, on the
same device, a note that arrives while the last one's gate is still open isn't
struck at all: it ties into the note sounding as a `HLD` does, so no envelope starts
again and `Reset` has nothing to act on. Only a note after the gate has closed is
struck, by the rules above. A `RET`'s ratchets are always struck.

**A pluck,** `Sustain` at `0%` with a short `Decay`, has died away before the next
note, so each note starts from silence. A 1 ms attack with a short decay on a bright
wave such as a saw is a click by its nature: the wave arrives at nearly full at once.
An `Attack` of 2 or 3 ms, a `Decay` of 40 ms or more, or the `Analog` curve softens it.

## AHD envelopes

An `AHD` slot rises to full over its `Attack`, holds there, then falls to nothing
over its `Decay`, for a drum's punch or a pluck that doesn't care how long the note
is. It has the same curve, `Reset` and `Depth` as an ADSR, and its shape is drawn
the same way, with the hold a stage of its own between two dotted lines. Its times
are down the left, and its depth and reset down the right.

| Setting | Value |
|---|---|
| Curve | In the title: `Exp`, `Lin`, `Snap` or `Analog` |
| Attack, Decay | Milliseconds, up to 8000 |
| Hold | Milliseconds, up to 2000, or `Note` |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**A timed hold** plays through whatever the note's length: it holds for its time,
then decays, even if the note ended first or goes on long after, as the M8's AHD
does. On `Vol`, shaping the notes, it ends each note when its decay ends.

**Holding for the note,** `Note` lasts as long as the note does, then decays from its end, as
the Digitakt's `HOLD NOTE` does. It's below `0 ms`: `-` from `0 ms` goes to it, and
`=` comes back. Typing `note` sets it too, and a number sets a time. A new AHD holds
for 30 ms, with a 1 ms attack and a 200 ms decay.

## Drum envelopes

A `Drum` slot is the M8's drum envelope, for a kick's or a tom's pitch: each note
strikes it at full, it falls steeply to its `Body` over its `Peak`, the click's
sweep, then decays from there to nothing over its `Decay`, the note's drop. It's a
one-shot, whatever the note's length, and on `Vol`, shaping the notes, it ends each
note when its decay ends. Its peak, body and decay are down the left, and its depth
and ratchet down the right. It has no `Reset`, as it always strikes from full.

| Setting | Value |
|---|---|
| Curve | In the title: how its body's decay bends |
| Peak | Milliseconds, the strike's fall from full to the body |
| Body | The percent the peak falls to, where the decay starts |
| Decay | Milliseconds, the body's fall to nothing |
| Depth | How far full moves it, in hex. Negative flips it |
| Ratchet | How many times the peak strikes before the body, 1 to 8 |

**Ratchet** strikes the peak again: with `3`, it falls three times from full, a
`Peak` apart, then the body decays from the third. A low `Body`, a `Peak` of about
10 ms and a long `Decay` make an 808's clap from noise; `2` makes a flam. A new
drum envelope has a 10 ms peak onto a body of 40% and a 300 ms decay, with one
strike: on `Pitch` with a `Depth` of about `+18`, a kick's drop. The peak's fall is
always steep, whatever the curve.

## Low-pass gates

An `LPG` slot is a low-pass gate's vactrol, after Mutable Instruments' Plaits: each
note pings it, it rises over its `Attack`, then falls, fast at first and slower as
it nears silence, over its `Decay`. As it falls it also closes the track's filter,
by its `Colour`, so one slot both fades a note and dulls it as it dies, for plucks,
drums and the bongo sounds a West Coast synth makes. It's a one-shot, and on `Vol`,
shaping the notes, it ends each note when its fall lands. It has no curve, as the
vactrol's fall is the point of it, so its title has no mode: `LPG  Uni  → Vol`.

| Setting | Value |
|---|---|
| Attack | Milliseconds, a straight rise |
| Decay | Milliseconds, the time to fall 60 dB |
| Colour | How far it closes the filter at silence, in hex |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**Colour** is in `Cut`'s own steps: with the track at `Cut 80`, `Colour 40` takes the
filter down to `40` as the note dies, and `00` leaves the filter alone. The filter
follows the vactrol's brightness, which falls much faster than its level, as
Plaits' does, so even a long decay darkens early. It adds to the track's `Cut` and
whatever else moves it, whatever the slot's `Dest` and `Depth`, and works only
while the slot goes somewhere: one on `---` does nothing. A new gate has a 1 ms
attack, a 400 ms decay and a `Colour` of `C0`.

## LFOs

An `LFO` slot moves around the current value, so `CUT40` on a step moves the centre
and the LFO carries on around it. It draws one cycle of its wave in a box on its
right, starting from its `Phase`, upside down for a negative `Depth`, with a dot
riding it where the LFO is now. `Random` and `Smooth` draw only a centre line, as
their values aren't known ahead, and `Lorenz` and `Rössler` draw themselves, below.

| Setting | Value |
|---|---|
| Mode | In the title: how notes start it, below |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` `Smooth` `Lorenz` `Rössler` |
| Rate | The length of one cycle. `=` and `+` are always faster |
| Depth | How far it swings each way, in hex. Negative flips it |
| Phase | Where in the cycle it starts, in degrees |
| Fade | `Off`, or how long it fades in (`+`) or out (`-`) after each note |
| Deform | How its wave bends, in hex from `-FF` to `+FF` |

With `Cut 80` and `Depth +40`, the filter moves between `40` and `C0`. `Saw`
falls and `Ramp` rises. `Random` picks a new value each cycle, and `Smooth` glides
through one after another, never stopping at any, which suits a slow drift on
`Cut`. An LFO's `Shape`, `Rate` and `Depth` are down its left, and its `Phase`,
`Fade` and `Deform` down its right.

**The mode** is the Digitakt's five, in the title:

| Mode | Does |
|---|---|
| `Free` | Runs from the song's position, whatever the notes do |
| `Trig` | Each note starts it again at its `Phase` |
| `Hold` | Runs free, and each note holds where it is until the next |
| `One` | Each note plays one cycle, then it stays where it ended |
| `Half` | The same, for half a cycle |

`Hold` is a sample and hold of the wave. `One` is like an envelope, and `Half`
makes a rise and fall from `Tri`, or a single sweep from `Saw`.

`Random` in `Hold` gives each note a new value, and `One` or `Half` on `Cut` is a
shaped sweep a note long. `Half` draws its half cycle, then a flat line where it
stays, and the dot of a held or ended LFO sits where it holds. Before a track's
first note, `Hold` holds where its wave starts, and `One` and `Half` are already
at their end.

**Fade** fades the LFO in or out over a note division or bars after each note,
from `1/64` to `16 bars`: `+1 bar` brings it in from nothing over a bar, and
`-1 bar` takes it out to nothing. `=` goes from the longest fade out, through
`Off`, to the longest fade in, and `+` and `_` skip the dotted and triplet lengths.
It counts from the track's last note in every mode, so in `Free` only the fade
starts again, not the wave. Before a track's first note, a fade in is done and a
fade out is over. `K` lists them all, and `i` types one, like `+1/4` or `-2 bars`.

**Deform** bends `Sine`, `Tri`, `Saw` and `Ramp` between their peaks, which stay
where they are: raised, the wave bulges up and spends longer high, as a log curve
does; lowered, it sags and spends longer low, as an exp curve. On `Square` it's the
width: `+80` is high for three quarters of the cycle, `-80` for a quarter. The
random shapes have nothing to bend. The picture draws the bent wave. A step locks
it with `DFM`, as `DFM4 C0`, where `80` is none.

**Lorenz and Rössler** are strange attractors: a point that goes round and round
without ever retracing its path, so the LFO moves smoothly but never quite repeats,
a drift for `Cut` or `Pan`, or a wandering tune on `Pitch in scale`. A cycle of its
`Rate` is about one turn: once round a wing of Lorenz's butterfly, which it circles
a few times before it flips to the other, or once round Rössler's loop, a wave that
swells and shrinks. On them `Deform` is what makes them chaotic, Lorenz's ρ and
Rössler's c. At `00` each is the textbook attractor. Lowered, Lorenz lingers a
little longer on each wing, and Rössler calms through loops of four and two turns
to a plain loop at `-FF`. Raised, both get wilder, and Lorenz at the top settles
into a figure of eight.

On an attractor `Phase` is `Axis`, as it has no cycle to start part way through:
it picks which of its three axes you hear, `x` at 0°, `y` at 120° and `z` at 240°,
and blends the two either side between them. `+` and `_` go from one axis to the
next. Lorenz's `x` and `y` swing from side to side as it flips wings, and its `z`
rises and falls once a turn; Rössler's `x` and `y` are its loop, a quarter turn
apart, and its `z` is a spike once a turn. The picture is the attractor seen side
on, the axis you hear up it and the next one round across it, with the dot riding
it where the LFO is.

Each starts from a point the song's seed picks, its own for each track and slot,
so a render repeats. In `Free` it starts at the top of the song; playing from
anywhere else starts it from that point where you play from, rather than where
playing from the top would have taken it. `Trig`, `One` and `Half` start it from
the same point at each note, so each note plays the same chaotic phrase, and `Hold`
holds where it is at each note. A rate moved by a macro or another LFO speeds it up
or slows it down from where it is.

| Type this as a rate | For |
|---|---|
| `1/4` `1/8d` `1/16t` | A note division, dotted or triplet |
| `2 bars` | Whole bars |
| `7st` | A number of steps, to slide against the bar |
| `2.5hz` | Free of the tempo, from `0.01hz` to `100hz` |

A large step is `10` in hex, or 90° of phase. On a rate it skips
dotted and triplet divisions, moves 4 steps, or doubles or halves the hertz.

`i` on the mode, Dest or Shape opens the list too. `K` on Rate lists the note
divisions and bars, and `i` is for anything else, like `7st` or `2.5hz`. `K` on
Fade lists every fade, out, `Off` and in.

**Reaching other slots.** A slot's `Dest` lists the depth, phase and deform of
every slot after it, as `M5 Depth`, `M5 Phase` and `M5 Deform`, so `Mod 4` can bring in `Mod 5`'s depth,
and an envelope in `Mod 2` can bring in an LFO's depth, or a later envelope's, with
each note. It only goes one way, so `Mod 6` can't reach any other.

**Moving the rate.** A later slot's `Dest` lists an LFO's `Rate` too, as `M5 Rate`, and
`M` on it puts it on a macro, so an LFO can speed another up and slow it down, or a
macro can. The LFO speeds up or slows down from where it is in its wave, with no jump,
as Vital's and Surge's do. Up the macro or the modulation is faster, in the rate's own
kind: a note division steps through the divisions and bars `=` goes through, from
`64 bars` to `1/64`; steps go from `4096st` to `1st`, and hertz from `0.01hz` to
`100hz`, both evenly. It counts by the song's ticks, so a synced LFO never drifts
against the bar, however long the song.

An LFO counts where it is from where it was at each change of its rate. Playing from
the top, and each note that starts it, start it again where the song's position puts it
at the rate it has then, so an LFO whose rate never moves is always exactly there,
from anywhere in the song. Once its rate has moved, playing from the middle of the
song puts it close to, but not exactly, where playing up to there would: a render
from the top is exact.

## When an LFO restarts

Playing from the top restarts every LFO. A `:reset` doesn't, since that is
about patterns. To follow the steps, set its mode to `Trig`, `One` or `Half`, or
`Hold` to take one value from it at each note. While stopped, a track sits at its
held values, so `;` hears those. Pausing freezes the LFO.

`Vol`, `Send A` and `Send B` swing in dB, on the level scale in **Phrases and
chains**, so `Depth +40` on `Vol` is 16 dB each way. They stop at `F8`, which
is 6 dB above where `VOL` starts, so a deep swing stays even if you lower `VOL`
on a step.

## Steps

A `Steps` slot plays up to 16 values in turn, a short sequence for any setting it
reaches, as the M8's tables and Bitwig's Steps do. Its values are a row of thin bars
in the box on its right, up from the middle for a value above `00` and down for one
below, with the step playing lit. A new Steps slot has 8 steps of `00` at a `1/16`,
started by each note, and choosing `Steps` for a slot with no depth gives it `+FF`,
so its values are heard as written.

| Setting | Value |
|---|---|
| Mode | In the title: `Trig`, `Free` or `Note`, below |
| Loop | In the title: what it does at its end, below |
| Dest | In the title: what it moves, such as `Pitch`, or `---` |
| Rate | How long each step lasts: `Tick`, `1/64` to `1 bar`, or `Pass` |
| Length | How many steps it plays, from 1 to 16 |
| Depth | How far, in hex: at `+FF` the values are heard as written |
| Start | The step each note starts it at |
| Mutate | The chance each value is drawn again as it comes round, `0%` to `100%` |
| Smooth | How it goes from step to step, in hex from `-FF` to `+FF` |

**The mode**: `Trig` starts it at its `Start` with each note, and waits for the
first; `Free` runs from the song's position, as an LFO in `Free` does; and `Note`
moves on one step with each note, so its `Rate` shows `--`. A negative `Depth`
turns the values over.

**Its values** are in its units at a depth of `+FF`, so `+07` on `Pitch` is seven
semitones up and `+10` on `Cut` is `10` above the track's cutoff. `l` from the grid
goes into the box, on the first step, and the cursor is a `▲` under it:

| Keys | Does |
|---|---|
| `h` `l` | Across the steps, with counts. `h` from the first goes back to the grid |
| `0` `$` | The first or last step |
| `=` `-` | Up or down by one |
| `+` `_` | Up or down an octave, `0C` |
| `i` | Type a value, signed hex such as `-0C` |
| `x` | Back to `00` |

Steps past the `Length` keep their values and aren't drawn, so shortening and
lengthening it finds them again.

**Mutate** is the Turing Machine's knob: each time a step comes round, it's drawn
again with that chance, anywhere within the slot's `Depth`, and keeps what it drew
until it's drawn again. `0%` plays the values as written, `100%` a new one each
time, and between them the loop drifts. It draws from the project's seed, so a
render repeats, and back at `0%` the values are as written again.

**Keeping a drift.** When the loop drifts somewhere you like, `Space g a` on the
slot keeps it: the values it drew are written in as its values, in one undo step,
and the box shows them. Turn `Mutate` down after, and the loop you heard plays as
written, every time round and in a render. Mutate up, listen, `Space g a`, Mutate
down: the Turing Machine's lock. `u` gives back the values as they were. Steps it
hasn't drawn keep their own.

**Smooth** at `00` steps. Up to `+80` each step glides in from the one before over
its first part, all of it at `+80`, and on to `+FF` the glide becomes a curve
through the steps. Down to `-80` each step falls to nothing over its last part, a
pluck a step, and past it the fall is sharper. It follows time, so in `Note` mode
or at `Pass` the steps are as they are.

**The rate.** `Tick` is a sixth of the track's step, as `DEL` counts, so six go by in
each step, for a fast burst. `Pass` moves on once each time round the track's chain,
counted as `CND` counts passes, so `00 10 20 30` on `Cut` opens the filter over four
times round and starts again with the chain. `=` is faster, as an LFO's is.

A later slot's `Dest` lists Steps' `Rate` as `M5 Rate`, and a macro can hold it, as an
LFO's: up is faster, through the divisions from `1 bar` to `1/64`, then `Tick`. Moved,
Steps go on from the step they're on, faster or slower, rather than jumping to
another. `Pass` isn't a speed, so a slot at `Pass` has no `Rate` to move.

**The loop**, after the mode in the title:

| Loop | Does |
|---|---|
| `Once` | Plays to the end and holds the last |
| `from 1` to `from 16` | Plays once, then loops from that step, as the M8's `HOP` |
| `Bounce` | Goes back and forth |
| `Shuffle` | Each step once each time round, in a new order each time |

`from 3` on four steps plays an attack of two, then holds a shape of the last two.
`Shuffle` deals its orders from the project's seed, so a render repeats.

`K` on the mode, the loop or the rate lists them. Some uses:

*   `+0C +07 00` on `Pitch` at `1/32`, `Once`: a flourish on each note.
*   Eight cutoffs in `Note` mode: each note gets the next.
*   `+10 -10` on `Pan` in `Free` at `1/16`: a stepped LFO.

**Start** is the slot's phase, so a step locks it with `PHS`: its value times the
`Length` over 256 picks the step, so `PHS3 80` starts an eight-step run on step 5.
`DEP` locks its depth, as for any slot. A later slot's `Dest` lists it as `M3 Start`.
Steps have no polarity, as their values say which way.

## Track

A `Track` slot follows each note: its pitch, for key tracking, how hard it's played,
or a value rolled for it. What it follows is its mode, in the title, so
`Track  Note  Bi  → Cut` opens the filter for the high notes and closes it for the
low ones. It takes its value as each note starts and holds it to the next, and
before the first note it moves nothing.

| Setting | Value |
|---|---|
| Source | In the title: `Note`, `Velocity` or `Random`, below |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Low | Where its map starts: a note such as `C-1`, or hex |
| High | Where its map ends |
| Depth | How far `High` moves it, in hex. Negative flips it |

**The map** is none of the `Depth` at `Low` and all of it at `High`, a straight
line between and flat either side. Its picture in the box on its right goes across
all of the source's values, with a faint line down at `Low` and at `High`, and a dot
where the last note fell. `Low` above `High` turns it round, so the low notes or the
soft ones move it most. The same in both is a switch: nothing below it, all of it
from there up.

| Source | Follows |
|---|---|
| `Note` | The note as it plays, `c_1` to `G-8` |
| `Velocity` | The velocity as it's heard, `00` to `7F` |
| `Random` | A value rolled at each note, `00` to `FF` |

`Note` is after `ACU`, `ARP` and the chain's transpose, and a new slot maps `C-1`
to `C-5`. `Velocity`, the default, is after `VDLT` and the groove. `Random` rolls
from the project's seed.

Each source keeps its own `Low` and `High`, so trying another and coming back finds
them as they were. `Random`'s rolls are the slot's own, so a render repeats and
`CHA` rolls as it did without it.

**Key tracking.** `Note` at `Bi` is centred half way between `Low` and `High`: with
`C-1` to `C-5`, `C-3` leaves the setting where it is, and at a `Depth` of `+30` each
octave moves `Cut` by `18`, up above `C-3` and down below it. At `Uni`, the polarity's
own, it rises from the setting: velocity from `00` to `7F` on `Vol` or `Cut` adds up
to the `Depth` for the hardest notes.

`=` and `-` move `Low` and `High` by a note or `01`, and `+` and `_` by an octave or
`10`. `i` types a note, such as `C-3`, or hex. `x` puts back the source's own. On
the Roto, `Low`, `High`, `Depth` and `Source` are the first four knobs, with the
polarity on the button under `Depth`, and `Dest` on the next page.

## Pitch in scale

Any slot's `Dest` lists `Pitch` twice: as it is, and `Pitch in scale`, which moves
the note along the song's scale (`:scale`) to the nearest of its notes, the lower of
two as near: `+03` from `C-3` in C major plays `D-3`. So a random LFO, or Steps with
`Mutate`, on pitch play in key; a `Depth` of `+0C` keeps them within an octave. Only
what the slot adds moves: the notes as written never do, and with the scale off
it's plain `Pitch`.

## Writing a slot into the notes

When a Steps or LFO slot on `Pitch` or `Pitch in scale` plays a melody you like,
`Space g A` in the phrase view writes it into the Note column: each note becomes
the one it was moved to the last time it was heard, and the slot is switched out,
so the phrase sounds the same and its notes can be edited. With the scale on,
they land on the scale's notes, as you heard them; off, they move by semitones.
It's one undo step, so `u` brings back the notes and the slot.

It writes the phrase's notes, or only the selected rows' with `V` first. A note
not heard yet stays as written, and the status line says how many. The Note
column has whole notes only, so a move of part of a semitone goes to the nearest,
and a slot that never moved a note half a semitone, such as a gentle vibrato, is
left playing. Envelopes and Track slots on `Pitch` stay too, as they shape each
note rather than choose it. With `Mutate` up, it's the time round just heard that's written, so
play, listen, and press `Space g A` on a pass you like. A `Lorenz` or `Rössler` LFO never plays
the same pass twice, so the same goes for it.

It writes in the phrase's live columns a macro or `Vary` moved too, each applied as it was last
heard and its rows not heard yet as written, so it's the one key for making the phrase what you
just heard.

## The accumulator

The `Accumulator` device, under the envelopes, says how the track's `ACU`s move its
notes from pass to pass: `Up` and `Down`, how far from the note as written, in scale degrees with
the scale on and semitones with it off; `Moves`, `Row` for only the row with the
`ACU` or `Track` for every row; `Limit`, `Wrap`, `Bounce`, `Hold` or `Random`, or one of the `bi` ones between both limits; `Adds`, once a
row, on each `MLT` pulse or on each `RET` hit; and `Reset`, when it goes back to
the notes as written. `=` and `-` move each, `+` and `_` move a limit by 7, `i`
types a limit and `K` lists the others. See **FX commands**.

## The arpeggio

The `Arpeggio` device, beside the accumulator, says how the track's `ARP`s play: `Rate`, how long each note
is, `Tick` or a note division from `1/128` to `1/8`, as a share of the track's step;
`Mode`, `Up` `Down` `Up-down` `Up-down-inc` or `Random`; `Oct`, how many octaves it climbs, 1 to 4;
and `Retrig`, `off` to only change the pitch, sliding on `Acid`, or `on` to start
each note again. `=` and `-` move each, `i` types the octaves and `K` lists the
others. See **FX commands**.
