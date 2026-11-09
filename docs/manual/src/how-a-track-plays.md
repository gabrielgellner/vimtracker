# How a track plays: time, gates and lanes

This chapter explains how a track turns its rows into sound over time. Most of the
time you won't need it, as a track plays its rows in order, one step each, as in any
tracker. It matters once rows have gates of different lengths, FX columns loop on
their own, or a phrase plays its rows in another order.

## What a track plays

```text
Song
 └─ Section, a row of the song view: one chain for each track
     └─ Chain: a list of phrases, played in order, then round again
         └─ Phrase: stored rows, and a loop, the part that plays
```

A phrase stores more rows than it plays. Its **loop** is the window that plays, and
rows outside it are dim and silent.

```text
stored rows:  00 01 02 03 04 05 06 07 08 09 …
loop:                  [03 ............. 08]     only these play
```

## One row has four parts

```text
 Note  Vel     Gate      FX1         FX2         FX3
 C-4   40 -3   HLD3 --   CUT   40 .  ---   00    ---   00
 └─ what sounds          └─ what changes while it plays
              └─ how long it lasts
```

- **Note and velocity** are what sounds. The dim pair after the velocity is the row's
  **nudge**, how far early or late its note plays, in 24ths of a step, and it's blank
  on a row on the grid.
- **Gate** is how long the row lasts, in **pulses**, which are steps, from `0` to `8`,
  and what the note does over that time. An empty cell, `----`, is one pulse of `SGL`,
  as every row was before gates. The dim pair after it is how long the note lasts, and
  `--` leaves that to the track.
- **FX columns** hold commands such as `CUT`, `RET` and `CHA`.

## Time: one count, which everything reads

The engine has one clock, the **step**, a sixteenth note at normal speed. Each track
keeps a **count**: how many steps since its chain started. Everything on the track
works out where it is from that one number. A track's step is a sixteenth unless its
`Speed` makes it longer or shorter: see **Speed: a track's own step** below.

```text
count:   0   1   2   3   4   5   6   7   8 …      one for each step
```

## The note column reads rows by their length

With gates, a row can take several steps, or none:

```text
rows:     00 HLD3      01 SGL1   02 SGL0   03 MLT2
steps:    0   1   2    3         none      4   5
count:    ├─ row 00 ─┤ ├row 01┤            ├row 03┤
```

So count 4 is row 03, and row 02 is skipped. The note column's loop is the phrase's
loop, and when it ends, the chain moves on to its next phrase.

| Gate | Over its pulses |
|---|---|
| `SGL` | One note, on the first |
| `MLT` | A note on each |
| `HLD` | One note, tied into the next row's note with no new attack |
| `RST` | No note, and it ends one that's sounding |

Any gate with `0` pulses skips its row.

How long each note sounds within its row is a share of the note's last pulse: the
row's own length, the dim pair in its gate cell, or the track's `Gate` where it has
none. At `100%`, the default, a note holds until something ends it. Below that, its
gate closes that far through its last pulse and its release starts, as with `OFF`:

```text
Gate 50%:
rows:     00 SGL3                 01 MLT2         02 HLD1 03 SGL1
steps:    0       1       2       3       4       5       6
gate:     ━━━━━━━━━━━━━━━━━━━━────━━━━────━━━━────━━━━━━━━━━━━────
```

A `SGL` holds through its row and closes halfway through its last pulse, each of a
`MLT`'s notes lasts half its pulse, and a `HLD` ties on into the next row's note,
which closes its own gate. `Acid` starts at `50%`, as a 303 does. An LFO on `Gate`,
and `LDLT` on a step, move each note's length up or down from that, a row's own too.

Here's a phrase with gates. Row 00 holds for 3 steps, then ties into row 01. Row 02
plays twice, each note half its pulse long, row 03 is skipped, and row 04 rests for 2
steps. The yellow numbers are
beats, and they fall on the rows that start on a beat, however long the rows before
them are.

![A phrase with a gate column](images/gates.svg)

`K` on the gate column chooses a gate from a list, keeping its pulses:

![The list of gates](images/gate-list.svg)

## A tie glides as long as `Glide` says

A `HLD` ties its note into the next row's, which carries the same note on at a new
pitch with no new attack. The track's `Glide`, on its Sound page, is how long the
pitch takes to get there, in ms. `Off`, the default, jumps, as a tie always has, but
`Acid` slides by its own `Slide`, as a 303 does. Set, it glides on every voice: a
synth by its pitch and a sample by its speed, so a sample's pitch and its speed move
together, as a tape's do.

```text
rows:     00 C-4 HLD1          01 C-5
pitch:    C-4 ─────────────────╱‾‾‾‾‾‾‾‾‾‾‾ C-5
                               ├┤ Glide 60 ms
```

It's a lag, as a 303's slide is: quick at first and slowing as it nears the note,
halfway in pitch at about a quarter of its time, nearly there when its time is up,
whatever the interval. It's in ms rather than a share of the step, so a glide
sounds the same at any tempo and `Speed`, as a slide does on a synth; a glide that
has to land on a row is `PTCH`'s glide mark, which goes by rows. `GLD` on a step
sets it for that tie, on the row tied from or the row tied into, as the tie is
still the same note. An `ARP`'s notes with `Retrig` off are ties too, so they glide
as well. A note that isn't tied never glides.

## An FX column has two modes

### Without a lane: it follows the notes

This is the default. The column is tied to the note column: it reads the same row,
and that row's FX run on the row's first pulse.

```text
count:      0    1    2    3
note row:   00   00   00   01         row 00 is HLD3
FX1 row:    00   00   00   01         its FX run at counts 0 and 3
```

The exception is `MLT`, which plays a note on every pulse. Its row's `DEL`, `KIL`,
`OFF` and `RET` run on every pulse too, though its `CHA` is rolled once, for the row.

Because the column is tied to the notes, changing the phrase's loop changes it too.

### With a lane: it runs on its own

`:lane 3` gives the cursor's FX column a **lane**: a loop of its own, 3 steps long.
It reads by step, whatever the note rows are doing:

```text
FX1's lane is rows 00 to 02

count:      0  1  2  3  4  5  6  7  8  9 …
lane row:   00 01 02 00 01 02 00 01 02 00         count % 3
note row:   00 01 02 03 04 05 06 07 08 09         a 16-row loop
```

Here FX 1 is a lane of 3, with the title showing `FX1:3`. After 21 steps the notes
are on row 04, while the lane, with its own `>`, is on row 02, as `20 % 3 = 2`. The
rows outside its lane are dim in that column only.

![An FX column playing its own lane](images/lane.svg)

On the phrase's second pass, at count 16, the notes are back at row 00, but the lane
is at row `16 % 3 = 1`. That's the **drift**, and it carries on across repeats and
into the next phrase of the chain. Against long rows, a lane keeps moving inside them:

```text
count:      0    1    2    3
note row:   00 ── HLD3 ──   01
lane row:   00   01   02   00         a new value every step, even mid-note
```

`RET` and `CHA` in a lane are ratchet and probability lanes, as on Metropolix: they
act on whatever step they land on, and do nothing on a step with no note. `LDLT` in a
lane is a length lane: it moves each note's length up or down from its own, so the
same row plays short one pass and long the next.

### The two side by side

| | No lane | With a lane |
|---|---|---|
| Its loop | The phrase's loop | Its own start and length |
| The row it reads | The note's row | Its count, wrapped to its lane |
| When its FX run | Each row's first pulse | Every step |
| Long and skipped note rows | Follows them | Ignores them, and drifts |
| Changing the phrase's loop | Changes it too | Leaves it alone |
| Made with | Nothing: it's the default | `:lane 3` |
| Back to following the notes | | `:lane` |

## What starts the count over

The count, and with it every lane, starts over on:

- playing from the top, or from a step;
- a section bringing in a new chain, or a section that resets;
- a launch;
- realigning with `:reset`.

Each of those also lines the track's steps up again, so a track at a slower speed
starts its next step there, even if that cuts the one before short.

Edits while playing don't start it over. Changing `:len`, a gate's pulses, a chain, a
lane's length, a phrase's order, or undoing one of them, leaves the track on the row it
was about to play, and on the pulse within that row, and a lane on its step. If that row has gone,
the track starts its chain again on its next step.

So playing from a step hears exactly what playing up to it would, since both work the
count out in the same way.

## The groove moves steps, not the count

A groove (see **Groove** in the FX guide) moves where each step lands, a little late
or early, but never the count: each step is still the step it was, it only sounds a
few ticks from its place on the grid. The groove counts each track's own steps from
the song's start or its last reset, so every track swings on the same steps whatever
its length, and a track at `1/8` swings its eighths:

```text
steps:     0    1    2    3          Swing 66: every other step 77 ticks late
grid:      |    |    |    |
heard:     |     |   |     |
```

Everything on a step moves with it: each pulse of a `MLT`, a lane's value, `DEL`
counting from where the step lands, and a note's `Gate`, which is a share of its
step as the groove makes it. A step's `GRV` chooses its groove before it lands.

## A nudge moves a row's note, not its lanes

A row's nudge moves its note part off where the groove lands its step, up to `B`
24ths, just under half a step, either way. The order is grid, then groove, then
nudge:

```text
steps:     0    1    2    3          Swing 66, with step 3's row nudged -B
grid:      |    |    |    |
groove:    |     |   |     |
heard:     |     |   |   |            a little early, as -B is more than 77 ticks
```

What moves with it is what belongs to the note: the note with its velocity and
length, so its end moves with its start; the row's own FX columns, the ones without
a lane; `DEL`, `KIL`, `OFF`, `RET` and `CHA` from any column, which count from where
the note now starts; the envelopes; and every pulse of a `MLT`. What stays is what
keeps time on its own: a column with a lane, the LFOs and the groove. So a note
nudged early on a step where a lane changes a value starts with the value before,
and the lane's change comes in on the grid.

A track's notes always play in order. A nudge can't take a row's note past the
next row's on the grid, but a groove's pushes on top of nudges can: then the later
row waits for the earlier one, and is the note left sounding.

## Speed: a track's own step

A track's `Speed` is how long its step is, as a note division: `1/32`, `1/16T`,
`1/16`, `1/8T`, `1/8`, `1/4T` or `1/4`. Everything on the track counts its own
steps, so at `1/8` it moves down its rows half as fast, and its playhead is drawn
where it is:

```text
sixteenths:  0   1   2   3   4   5   6   7   8
1/16 track:  00  01  02  03  04  05  06  07  08
1/8 track:   00      01      02      03      04
1/16T track: 00 01  02 03  04 05  06 …           three to every two sixteenths
```

What's a share of a step is a share of the track's: each pulse of a gate, a lane's
step, `DEL`'s sixths, a nudge's 24ths, a groove's pushes and a note's `Gate`. The
LFOs keep to the song's time.

The song itself counts sixteenths. A section lasts its longest chain at that chain's
speed, so 16 rows at `1/8` take 32 sixteenths, and a triplet chain that doesn't end on
a sixteenth rounds up to the next. Resets, launches, `:reset` and renders count
sixteenths too. Inside a section a triplet track runs on, on its own grid, and lines
up again where its chain next starts.

`RATE` on a step changes the speed for that step and the rest of its note, or from
there on if it's held with `h`. A step at a new speed lasts until the next of that
speed's steps, counted from where the track last lined up, so the track keeps to the
grid of whatever speed it's at:

```text
sixteenths:  0   1   2   3   4   5   6
rows:        00  01  02      03      04       RATE 04h, 1/8, on row 01
                 └ lasts to the next eighth
```

## Conditions count passes

A `CND` plays its row only on some passes, as Elektron's trig conditions do. A pass
is each time the track plays the row since its chain started, so it's the count
divided by the chain's length, times the times the row's phrase is in the chain,
plus the times it came before this place:

```text
chain:       P0 P1                  P0 twice:    P0 P0
count:       0  4  8  12            count:       0  4  8  12
P0's pass:   0  -  1  -             P0's pass:   0  1  2  3
1:2 plays:   x     .                1:2 plays:   x  .  x  .
```

As it's worked out from the count, a pass is what the count is: it follows the
track's own speed, starts over wherever the count does, and playing from a step
hears the pass playing up to it would. A `CND` in a lane counts the lane's loops
instead, its count divided by the lane's length.

A failed condition skips its row's note, events and own locks, while columns with
lanes keep to theirs. A step's `GRV` and `RATE` still time it, as they're chosen
before it lands. `PRE` and `NEI` read what the last condition on this track, or on
the track to the left, decided, as each row lands.

## Accumulators remember what played

An `ACU` moves its row's note on each time the row plays, by scale degrees with the
scale on and semitones with it off. Unlike a condition's pass, which is worked out
from the count, an accumulator is kept as the track plays, as what it adds depends
on what played: a row that `CHA` or `CND` skips, or that rests, adds nothing. So
playing from a step starts it from the notes as written, rather than working out
what playing up to it would have left.

The first time a row plays it's as written, and each time after, its `ACU` adds
before the note sounds. With `Moves` at `Row`, only that row moves; at `Track`,
every row on the track moves by what the track's `ACU`s have added up to:

```text
rows:        C  D  E  F             ACU 01 on the C
Row:         C D E F | D D E F | E D E F
Track:       C D E F | D E F G | E F G A
```

At a limit, `Wrap` carries on from the note as written, `Bounce` turns back and
turns again at the note, and `Hold` stays. With `Reset` at `Chain`, the default, it
starts again wherever the count does; at `Play`, only when playing starts; and
every so many loops of the chain, counted from where it started. It adds once a
row, or with `Adds` set, on each pulse of a `MLT` or each hit of a `RET`.

## An arpeggio lasts its note

An `ARP xy` goes round its row's note, `x` up and `y` up, in scale degrees with the
scale on, at the track's `Rate`, in its `Mode` and over its octaves. Like a lock, it
belongs to the note: it starts where the note does, after the groove, the nudge and
any `DEL`, and lasts as long as the note does, through the row's pulses and the
empty rows after, until the next note, an `OFF`, `KIL` or rest, or the gate closing.
A `MLT`'s pulses are each a new note, so each starts it again; a `HLD` ties it into
the next row's note, where it goes on round from that note:

```text
rows:        C-4 ARP 40 HLD    D-4            Rate 1/32T, three to a step
heard:       C  E  C           F# D  F#
```

Its notes are worked out from the note as it's heard, after an `ACU`, and the
chain's transpose goes on after, so both move it. `Rate` is a share of the track's
step, so a track at `1/8` arpeggiates half as fast. With `Retrig` off, each note
only changes the pitch, as a tie does; on, each starts again, as a `RET`'s does.
`Random` draws from a stream of the track's own, so `CHA` rolls as it would
without it, and a render repeats.

## A phrase's order

A phrase's rows play top to bottom unless `:order` gives it another order, after
the Metropolix's ORDER. A **pass** of the phrase is one time round its order, and
the note column walks it as it walks rows forward, so a row's pulses stay together
and a skipped row takes no time:

```text
rows:        00 SGL1   01 MLT2   02 SGL1
forward:     00  01 01  02                  4 steps
reverse:     02  01 01  00                  4 steps
pendulum:    00  01 01  02  01 01           6 steps: the middle row twice
```

`pendulum`, `ping-pong` and `pedal` play some rows twice, so a pass lasts longer
than the rows do, and the chain, and a section it sets the length of, with it.
Every other order plays each row once a pass, except `random`, which picks a row
each time until the pass has lasted as long as the rows would forward, cutting its
last pick short if it has to, so the length never depends on what it picks.

What plays next is the next row in the order, so a `HLD` ties into it, and a
glide goes to the next value in the order the rows play. A column with a lane
keeps its own forward loop, as it counts steps rather than rows. The groove pushes
each step by its place in the count, not by its row, and a row's nudge goes with
its row.

A condition's pass is a time round the order, still worked out from the count, so
a row that a pendulum plays twice is in the same pass both times, and a `1:2`
plays both or neither. An accumulator adds each time its row plays, so the same
row moves on twice:

```text
rows:        00 -  01 C  02 -  03 -        ACU 01 on row 01's C, pendulum
plays:       00 01 02 03 02 01 | 00 01 02 03 02 01
heard:          C           D  |    E           F
```

`shuffle` deals each pass a new order with every row in it once; `random` picks
any row each time, so rows repeat and some are left out. Both are dealt from the
project's `:seed`, the track and which time round its chain this is, counted from
where the chain started. That's a hash rather than the track's generator, so
`CHA`, a groove's humanise and a `Random` arpeggio roll as they would without it,
a render repeats, and playing from a step hears what playing up to it would. A
chain that starts again deals again from its first time round, as its conditions'
passes start again.

The playhead and the other tracks' columns show the row playing. Playing from a
row starts the phrase where its order first plays that row: a reverse phrase's
top row is its last.

## The keys, by column

| Cursor on | `gs`, `ge`, `:len` | `:lane 3` |
|---|---|---|
| Note, velocity or gate | The phrase's loop | Says it works on an FX column |
| An FX column without a lane | The phrase's loop | Makes it a lane |
| An FX column with a lane | That lane | Makes a new lane of 3, from the cursor's step |

`gg` and `G` follow the same rule: on a lane they go by the lane, and everywhere else by
the phrase's loop.

In short: notes walk rows that each take some steps, FX columns follow those rows unless
you give them a lane, and a lane counts plain steps on a loop of its own, which is where
the drift comes from.
