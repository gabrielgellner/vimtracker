<!-- ANCHOR: before-bars -->
# Selecting and editing

Editing follows Vim's grammar: an operator, then what it covers.

## Operators

| Keys | Does |
|---|---|
| `d` | Clear |
| `y` | Copy |
| `>` `<` | Shift up or down |

Follow one with a motion or a target, or double it for this whole step:

| Keys | Does |
|---|---|
| `dd` | Clear this step |
| `dj` | Clear this column here and on the next step |
| `d4j` | Clear this column here and on the next 4 steps |
| `yy` | Copy this step |
| `>>` `<<` | Shift this step's values up or down a step |
| `dib` | Clear this beat, whole steps |
| `dic` | Clear this column, whole phrase |
| `dip` | Clear this phrase, whole steps |
| `d$` | Clear this step from this column to the end of the row |
| `d0` | Clear this step from the start of the row to this column |

## Selecting

| Keys | Does |
|---|---|
| `v` | Select fields, starting from this one |
| `V` | Select whole steps |
| `W` `B`, `]b` `[b` | In a selection, reach the next or previous track |
| `gv` | Select the last selection again |
| `=` `-` `+` `_` | Move every value a small or large step, and stay selected |
| `Cmd-l` `Cmd-h` | As `=` `-` |
| `Cmd-k` `Cmd-j` | As `+` `_` |
| The mouse wheel | As `=` `-`, further the faster it goes, once it's on |
| `>` `<` | Move every value a step, and stay selected |
| `Ctrl-a` `Ctrl-x` | As `>` `<` |
| `r` | Fill the cursor's column, and stay selected |
| `u` `Ctrl-r` | Undo or redo one change, and stay selected |
| `d` `x` `y` `S` `~` | Clear, copy, shape or mark, and stop selecting |
| `Esc` | Stop selecting |

Move to grow the selection. The cursor shows which end is moving. Then press
`d`, `x`, `y`, `>`, `<`, `-`, `=`, `_`, `+`, `r`, `S` or `.`.

A selection stays until `Esc` while you change its values, so `+ + -` can go in
one after the other as the loop plays, as Vim's popular `vnoremap > >gv` does.
Each press is its own undo, so `u` takes them back one at a time and the
selection stays, and past its own presses `u` takes back what came before. The
cursor stays where it is. The status line keeps the running total for the
selection, in its first column's unit: `+11 semitones` for notes, 24ths for a
nudge, or `+1 large step, -2 steps` for other values. With the scale on, `=` and
`-` count in degrees of it. `.` repeats a press and keeps the selection. `d`,
`x`, `y`, `S` and `~` end the selection, and `gv` brings back the last one, even
after `Esc`.

`r` fills the cursor's column across the selection, as one undo: `vGra` writes
a C on every step to the end of the loop. Notes already there keep their
velocity. A velocity only goes on steps with a
note, an FX value only on steps with a command, and `.` repeats it. The
selection comes back afterwards, so another `r` picks again, and `Esc` at its
prompt comes back to it too. The running total starts again after a fill.

With rows of one column selected with `v`, a note, a velocity, a nudge, a gate's length or an FX
value that's an amount, its values show as bars beside it, as the shape panel shows them (see
**Shaping a column**): each row's value now solid, and what it was when the selection started
behind it, so a run of `+ + -` or a fill with `r` shows how far it has moved things. Above them
are the column's name and range and the lowest, highest and average now, and how many rows
changed. A gate's type, an FX command's name or a selection over several columns has none.
They go when the selection does.

## Across tracks

In `v` or `V`, `W` and `B` reach other tracks, so `VGWWW` covers a whole drum
part. `d`, `y`, `p`, `>`, `<`, `=`, `-`, `+`, `_`, `r` and `~` then act on each of them as one
undo. Rows count from each phrase's own loop start, as on screen. `p` puts each track's part on the track the
same distance along from the cursor. `vjW` then `+ + -` moves both tracks' notes
up 23 semitones, a press at a time, with the selection kept.

## Shaping a column

Select part of a Note, Velocity, Nudge, length or FX column with `v`, then press `S` for a panel
beside the grid. The column changes as you adjust it, and you hear it if the
loop is playing. `Enter` keeps it as one undo, and `Esc` puts everything back.
What's in the column decides what `S` does: on notes or commands already there
it reshapes them, and on a Note or FX column with nothing in the rows selected
it generates, as below. The panel's title says which. A selection reaching
other tracks with `W` shapes the same column on each of them with the same
settings, and each picks its own random values. Each track goes by its own
column, so an empty one generates while the others reshape, and a track with
nothing to shape there, like a velocity with no notes, is left alone.

While the panel is open on the phrase view, the column's values show as bars beside it, one a row,
over the FX lanes and the tracks beside, which can't be edited while you shape. Each row's value
now is solid, and what it was when the panel opened is a fainter bar behind it, so you see what
a fill or a transform moves. Above them, the column's name and its range, `VEL 00 ─ 7F` or an FX
command's own, and a line with the lowest, the highest and the average now, and how many rows
changed: `20─7F avg 4C 6 changed`. Only the rows you shape have a bar, and they scroll with
the grid. On the Note column it's a contour, a mark a row placed by pitch across the notes there,
over faint lines on each of the scale's roots, and on its other notes while there's room. A
column whose values aren't amounts, such as a list setting's lock or `CND`, has no bars. They go
right of the column, and the panel stays top right; where that would cover them, the panel moves
to the top left, and where the right has no room, the bars go left of the column. They go when the
panel closes.
<!-- ANCHOR_END: before-bars -->
<!-- ANCHOR: after-bars -->

| Row | Does |
|---|---|
| Fill | `Ramp`, `Random`, `Walk`, `Wave`, `Acid`, `Grids`, `Ring`, `Tonnetz` and more |
| From, To | The ends. `~` swaps them |
| Curve | Bends a ramp. Above 0 it moves early, and below 0 late |
| S | Eases a ramp's ends. Above 0 makes an S, and below 0 the reverse |
| Run | How many rows each value holds for |
| Where | Which rows change, with `Amount` below it for some |
| Over | What a counted `Where` counts: `All rows` or `Notes` |

`r` picks a random fill's values anew. `Run` makes stairs: a ramp from `00` to `30` over 8 rows with `Run 2` gives
`00 00 10 10 20 20 30 30`, and a random fill holds each pick for that many rows.
It counts the rows it changes, so on Velocity it's that many notes.

The fills with the most settings, `Random`, `Acid`, `Grids`, `Divisions`, `Ring`
and `Tonnetz`, have a section each in **Generators in depth**: how their settings
work together, and recipes to start from. `Space ?` in the panel opens that guide at
the fill's section, and on any other fill at this section; `Esc` comes back to the panel as it was.

`Random` has two more rows, after Mutable Instruments' Marbles, for where its values fall
between the ends:

| Row | Does |
|---|---|
| Spread | How far they stray, from one value to the two ends |
| Bias | Which way they lean, `-100` low to `+100` high |

`Spread` `0%` is one value, low a bell round it, `50%` any value as likely, as it starts, and
high a bowl, until `100%` is the two ends alone. With a narrow Spread, Bias is where the values
gather: `0%` with `+50` writes the value three quarters of the way up on every row. With a wide
one, it's how often they're in the top half, so `100%` with `+40` gives the higher end seven
rows in ten. `=` and `-` move Spread by one and
Bias by five, and `+` and `_` by ten and twenty-five. A macro can turn both while it plays: a
wide Spread for a fill, a narrow one for the verse.

`Walk` wanders: it starts on From, and each row is a small step from the last,
up or down, never past the ends. It's Max's `drunk`, a Brownian line, so a
velocity drifts as a player's does and a melody moves like a hand on the keys.
It shows these in place of Curve and S:

| Row | Does |
|---|---|
| Step | The largest move from one row to the next, either way |
| Edge | At an end, `Reflect` bounces back, and `Wrap` comes round from the other |

A move can be anything from `Step` down to none, so with a Step of `10` a
velocity goes up or down by at most `10` a note. Step starts at an eighth of
the column's range, `10` on Velocity and 2 on a nudge at `1/16`, more on a
slower track, and at 2 on notes: `=` and `-` move it by one, `+` and `_` by
`10`, or 4 on a nudge, and `i` types it. With `Wrap`, a walk past
To comes back in at From's end, which on notes is a leap.

`1/f` is between `Random` and `Walk`: it wanders, but comes back. It sums a few
random values, one new on every row and each of the others half as often as
the one before, so a few rows hang together, a few bars further. It's the
pink noise R. F. Voss and J. Clarke made melodies from in 1978, which their
listeners liked better than random notes, which jump about, or a walk, which
drifts off. `Depth` is how many values it sums, from 2 to 8, 4 to begin with:
more gives longer shapes, rising and falling over more rows. At any depth it
spreads as widely as `Random`, about one row in twelve on an end.

`r` picks a walk's or a `1/f`'s moves anew, `~` swaps the ends, so a walk
starts at the other, and `Run` holds each value for that many rows, as for
`Random`.

`Wave` swings between the ends, for movement like a pulse across a hat's
velocities, and shows these in place of Curve and S:

| Row | Does |
|---|---|
| Shape | `Sine`, `Tri`, `Saw`, `Ramp`, `Square` or `Smooth`, as on an LFO |
| Rate | How long one swing takes, like `1/2`, `1/8d`, `2 bars` or `7st` |
| Phase | Where in the swing the first row selected falls, in degrees |
| Run | How many rows each value holds for, sampled where the run starts |

`Smooth` wanders from one random value to the next, a drift that `r` picks
anew. A Rate of `x3` fits three whole swings into the rows selected, however
many there are, so the selection loops round smoothly: type it with `i`, or
choose `x1` to `x8` with `K`, and `=` `-` add or take one.

Each row's value comes from where it falls in time, counting from the first row
selected, so the movement keeps its timing however the notes are spaced. As on an
LFO, `Sine` and `Tri` start half way and head for `To`.

`Alternate` gives the rows it changes one value and then the other, so every
other hit flips however they're spaced. `Run` sets how many rows each value gets
in a row: `1` gives `70 40 70 40` and `2` gives `70 70 40 40`.

`List` goes round a list of values instead, for accents two values can't
make, such as `70 40 55` over a hat, or a line as long as the phrase, a value a
row. Its `Values` row takes the place of From and To: `i`, then up to 256 hex
values, where two digits make a value and the space comes by itself, so `704055`
types `70 40 55`, starting from the values there. It starts as the two ends. `Run` works as for
`Alternate`, so `2` gives `70 70 40 40 55 55`, and `~` turns the list round. A phrase longer than
the list goes round it again. A list longer than the panel is wide shows its first
values and how many more, as `+248`, and while it's typed the panel shows its end.
`l` goes into the values, a token each (Fields of tokens, below): `=` and `-` turn the one under
the cursor, a note by a degree of the song's scale, or a semitone with it off, and `+` `_` by an
octave, so a line of notes can be walked into a new one without typing it again; any other value
turns by 1, or `10`. `x` takes a value out, `a` types one after it and `p` pastes one, and the
status line says how much room is left, `Value 2: 40, 3 of 256 values`; a 257th is refused,
saying so. A list longer than the field shows the values round the cursor, `…` where it's cut.
The list is kept on the live generator beside its shape, so `x` on `Values`
puts it back to the two ends, and a fill other than `List` keeps none.

The last four fills are transforms: rather than making new values, they rework
the ones already in the rows `Where` chooses, and a row it chooses with no value
in the column is left alone.

| Fill | Does |
|---|---|
| Scale | Maps the values from their own lowest and highest onto From and To |
| Invert | Flips each value within their range, the loudest to the softest |
| Shuffle | Deals the values to the rows in a new order. `r` deals again |
| Reverse | Gives the rows their values in the opposite order |

`Scale` tames a take into `40` to `60`, or widens a hat's accents. Curve and S
bend it as they bend a ramp, and From above To turns it upside down. The others
have no settings but `Where`, and keep the rhythm: only the values move.

The ends start as the first and last values selected, or if those match, the
lowest and highest.

| Where | Rows |
|---|---|
| Notes | Rows with a note |
| Commands | Rows already holding this FX command |
| All rows | Every row |
| No notes | Rows without a note |
| Every | Every so many rows, counting from the first selected |
| Chance | Each row with this percent chance |
| Euclid | `Hits` spread as evenly as they go, with `Rotate` below it |
| Clusters | Runs of two to four rows, about this percent of them |
| Markov | Each row's chance hanging on the rows before it |
| 3 states | Each row a gate, a rest or a tie |
| Complementary | One coin a row for two tracks, so never both |
| Rhythm | A named rhythm's part, each hit by `Child`'s chance |
| Cells | A cellular automaton's cells, by `Rule` from `Start` |
| Beats | About this percent of the rows, the strong beats most of all |

A `Where` is a selection of the rows. `Notes`, `No notes` and `Commands` choose
by what's on a row; every other is a pattern over the rows, whatever is on them.
A row outside the selection is left as it is, and a row inside it is written:
on the Note column a note there gets a new pitch, and a row without one a note
of its own. A row the column can't change is passed over, even inside it: a
velocity, nudge or length without its note, an `OFF`, or an FX slot holding
another command.

`Over` says what a counted pattern counts: `Euclid`, `Every`, `Rhythm`,
Marbles' `Clusters`, `Markov` and `3 states`, and `Cells`. `All rows` counts
every row; `Notes` counts only the rows with a note, a hit or a step for each
note in turn, and on an FX column `Commands` counts the rows with its command.
It starts as `All rows` on the Note and FX columns and as `Notes` on Velocity,
Nudge and length. With 7 notes in 16 rows, `Euclid` 3 over `Notes` is
`x.x.x..` on the notes, the first, third and fifth; over `All rows` it's
`x....x....x.....` on the rows, and on Velocity only a hit that lands on a note
changes one.

`Euclid` spreads its hits as evenly as they go over what it counts, as the
Euclidean rhythms do: 3 in 8 is `x..x..x.` and 5 in 8 is `x.xx.xx.`. On Velocity
it counts the notes however they're spaced, so 5 hits over 16 hats accent 5 of
them. `Hits` shows how many it spreads them over, as in `5 of 16`. `Rotate`
moves every hit that many later, going round, so 3 in 8 rotated by 1 is
`.x..x..x`.

`Clusters`, `Markov`, `3 states` and `Complementary` are the rhythm models of
Mutable Instruments' Marbles, its T section, each with an `Amount`. Like Euclid,
the first three go through the rows `Over` counts in order, so on Velocity they
count the notes; `Complementary` and `Chance` toss a coin for each row by
itself.

- **Clusters** plays in bunches: a run of two to four rows, then a gap. Its
  amount is about how many of the rows play, as `Chance`'s is, so swapping one
  for the other keeps the density and bunches the hits.
- **Markov** is Marbles' Markov model, its amount the module's Bias. Up from
  about a third, it leans toward playing what played eight rows before and away
  from two hits in a row, so at `80%`, where it starts, it plays an eight-row
  loop that changes now and then, and at `100%` it repeats it. At a third it's
  a sparse coin, and below it does the reverse, busier and never the same eight
  rows twice. After more than 24 rests, a row is far likelier to play.
- **3 states** makes each row a gate, a rest or a tie, its amount the module's
  Bias: mostly gates at `0%`, mostly rests at `50%` and mostly ties at `100%`.
  It starts at `25%`, half of the rows resting. A tie holds the value before it,
  and on notes of its own, the note before is held into it with a `HLD` gate,
  no new attack. After a rest, there's nothing to hold, so a tie rests too.
- **Complementary** tosses one coin a row for two tracks. `S` across two tracks
  gives each a side, `First` and `Second`, and the same seed, so every row goes
  to one track or the other, never both, as a hat and a snare trading sixteenths.
  `Amount` is the first track's share, and its rows are the ones `Chance` would
  choose. Across more tracks, each pair has a coin of its own. `Side` shows
  which side a track is, and `K` switches it.

Each of them draws from the seed, so `r` draws again, and `Vary` gives each pass
a rhythm of its own.

`Rhythm` plays a traditional pattern, after Noise Engineering's Zularic
Repetitor. `Rhythm` under it picks the pattern with `K`, each listed with where
it's from and how many rows it goes round in; `Part` which of its three parts
the rows are; and `Child` the chance each of the part's hits plays, from `100%`,
the pattern as written, down to `0%`, none. A child never plays where the
pattern rests, and draws from the seed, so `r` and `Vary` give it other hits.

| Rhythm | Bell | Rows |
|---|---|---|
| Son clave | `x..x..x...x.x...` | 16 |
| Rumba clave | `x..x...x..x.x...` | 16 |
| Bossa nova | `x..x..x...x..x..` | 16 |
| Gahu | `x..x..x...x...x.` | 16 |
| Shiko | `x...x.x...x.x...` | 16 |
| Soukous | `x..x..x...xx....` | 16 |
| Samba | `x.x..x.x.x..x.x.` | 16 |
| Tresillo | `x..x..x.` | 8 |
| Cinquillo | `x.xx.xx.` | 8 |
| Bembé | `x.x.xx.x.x.x` | 12 |
| Motorik | `x.x.x.x.x.x.x.x.` | 16 |

The bells are Godfried Toussaint's transcriptions in his papers on the clave and
bell timelines. The son's `Low drum` is the bass's tumbao, `...x..x....x..x.`, and
its `High drum` the conga's open tones, `......xx......xx`. Motorik is a kit: the
bell is its hats, the low drum its kick, `x.x...x.x.x...x.`, and the high drum its
snare on 2 and 4. For the rest, the low drum is the beat, a row in four, and the
high drum the offbeats between; the bembé's low drum is its four beats and its
high drum the six quarters across them, `x.x.x.x.x.x.`.

A rhythm plays a pulse for each row `Over` counts, going round from the first,
on any column; on Velocity it counts the notes, so it accents the notes in
turn. The bembé's twelve pulses
are a bar's triplet eighths, so on a track at `1/8T` twelve rows are a bar, and
the panel says so on any other. `S` across tracks gives each the next part,
`Bell`, `Low drum`, `High drum` and round again, from the one the panel shows,
each with a seed of its own, so its children are its own; the panel says `Each
track plays the next part`.

**A rhythm of your own.** `K` on `Rhythm` ends with `Own`, a rhythm typed in
`Text`, in TidalCycles' mini-notation, that any fill plays on: an `Acid` on it,
a `Random` velocity, a `Wave` on an FX column. Choosing `Own` starts it from the
named rhythm's part, a token a pulse, and so does editing a named rhythm's
`Text`, which shows on its row: the son clave's bell is
`x . . x . . x . . . x . x . . .`. `i` types the whole text, and `l` goes into
its tokens ([Fields of tokens](#fields-of-tokens)).

| Text | Means |
|---|---|
| `x` `.` | A hit, and a rest; `~` is a rest too |
| `[x x]` | A group: its tokens share its place |
| `x*3` | Three in its place, or in one row a `RET` |
| `x!3` | Three places of `x` |
| `x@3` | One hit three places long |
| `x?` `x?30` | A hit half the time, or 30% of it |
| `<x .>` | One of them each lap, in turn |
| `x(3,8)` `x(3,8,2)` | 3 hits in 8 parts of its place, 2 later |

It goes by steps, not cycles: each top-level token is `Step` rows, from `1 row`
to `16 rows`, so a token more makes the line longer, as a row more does, and the
line goes round to fill the span; a span shorter than the line cuts it. At
`Step 4 rows`, `x . [. x] x(3,8)` is a hit on row 0, one on row 10, and 3 of 8
in the last beat, on rows 12, 13 and 15. Inside a token, a group, `*` and a
Euclid split its rows, each hit on the row nearest it within its place, and two
hits on one row keep the first. So a Euclid is one token: `x(3,8)@8` at `Step 1
row` is the tresillo over eight rows. On the Note column, `x*3` in a single row
is a `RET` in the row's first empty FX slot, three strikes in the row; given rows
enough, as `x*2` at `Step 2 rows`, it's split rows instead.

`<a b>` takes the next choice each lap, counted on from pass to pass, so
`<x . .>` over a span of four laps carries its turn into the next pass; the
phrase holds the first pass and the engine plays the rest. `?` draws from the
seed, its hit and its row, the same each pass until `Vary` draws again. Euclid's
third number moves the hits later, as `Rotate` does, where Tidal's moves them
earlier; `.` is always a rest, never Tidal's feet, so `[ ]` groups. The text is
the generator's, saved with it, and `Step` is a setting a macro can turn.

**A line of rhythm and values.** The fill `Mini` has no `Where`: its `Text`
says where and what, in the same notation as an `Own` rhythm, but each hit is a
value in the column's own units rather than `x`. It starts from what's in the
rows, a token a row, or on an empty column from a line of its own.

| Column | A hit is | Like |
|---|---|---|
| Note | A note, or a degree from `1` | `C-3 [. 3] <5 6>*2 8?` |
| Velocity | Hex | `7F 40 [60 40] 40?` |
| Nudge | A nudge with its sign | `0 +8 0 -4` |
| Length | Hex, or `_` for `HLD` | `40 _ 80 20` |
| FX | Its value, or its name | `30 [60 90] A0 <C0 40>` |

On the Note column a note is `C-3` or `c3`, and a degree counts from `1`, the
root, in `From`'s octave, both in one line. `_` on Length ties the row's note
into the next. On an FX column a value is hex, or its name where `K` names them,
a `CND`'s condition or an `ACU`'s amount. The FX command is the column's, chosen with `K` before `S`, as on any empty FX
column; the line holds only its values. `[ ]`, `< >`, `*`, `!`, `@`, `?` and
`(3,8)` mean what they mean in an `Own` rhythm, a top-level token is `Step` rows,
and the line goes round to fill its span. On the Note column a hit that falls
between rows is nudged off the nearest, as `Divisions` nudges its triplets, so
`[C-3 D-3 E-3]` at `Step 4 rows` is a true triplet, rows 0, 1 and 3, the second
nudged `+8` and the third `-8`; and `C-3*3` in one row is a `RET`. Two hits on a
row keep the first, and the status line says how many were dropped and which
`Step` keeps them all: `2 hits share a row and dropped; Step 3 rows keeps them`.
`<a b>` takes the next choice each lap, on from pass to pass, and `?` draws from
the seed, its hit and its row, the same each pass while `Vary` is `Once`.

A stack, `,`, puts lines over one another, which one track's column can't hold:
on one track it's refused, saying so. Across tracks, `W` selecting them or `S`
across them, each takes the next layer, so `C-2*4, [. G-2]*2` at `Step 4 rows` on
two tracks is a bass on every row and its answer on the offbeats. `K` in the
line lists what can go where the cursor is: the scale's notes round the one
before, or its degrees, a rest, the groups, and after a token the changes it
takes, each with what it does. A line typed again is heard at once; with the
track's `Cue` on, the line playing plays on until its next lap starts, so it's
never heard half the old and half the new. The line is the generator's, saved
with it.
`Beats` is the rhythm `Acid` has always played, for any fill: each row a chance
by where it falls in the bar, counting from the first row selected, the beats
most, then every third row, then the other even rows, and the rest least. Its
`Density` is how many play on average: at `50%`, where it starts, a beat plays
about seven times in ten and an odd row between them about three.

`Cells` is an elementary cellular automaton, one of Stephen Wolfram's 256
rules, as dan derks' *less concepts* plays them: a row of cells, each on or
off, makes its next generation cell by cell from the cell and its two
neighbours, by the rule. `Rule` runs from `0` to `255`: 30 is chaotic, 90
draws Sierpiński's triangle, 110 is complex, 184 moves its cells along like
traffic. On one column each row `Over` counts is a cell, so the rows chosen are the
cells that are on; across tracks each track is a cell and each row a generation, so
the tracks play the automaton's picture down the rows. `Start` is the row it
starts from: `Seed`, cells drawn from the seed, which `r` rolls again; `One`,
one cell in the middle, as Wolfram's pictures start; or `Hits`, the rows with
a note on them, or across tracks the tracks with a note on the first row. The
row is a ring, so the first cell's neighbour is the last. Across tracks, every
track plays one automaton, the row they start from worked out once for all of
them, so a track's `gl` has `Rule` and no `Start`. Kept live, `Vary` steps it
on: see **The Gen page, macros and presets**.

The ramp's ends land on the first and last rows it changes, and rows keep their
spacing along it. A velocity only ever goes on a note, so on Velocity a row
selected without one is passed over. On an FX column, the command is the first
one selected, and a row holding a different command keeps it.

### On the Note column

On the Note column, `S` gives the notes already there new pitches, keeping
their rhythm, velocities and gates, and leaving `OFF` and empty rows alone: keep
this rhythm, give me another line. That's `Where` on `Notes`, as it starts; any
other `Where` writes its own rows, a note of its own on a row without one, and
leaves the notes outside it as they are, so `Euclid` there writes its hits. From and To are notes, typed as `C-3`, `c3`
or `c#3`. With the scale on, every fill moves in its degrees, so it lands in key
as it's made; with it off, in semitones. `=` and `-` move an end a degree, or a
semitone with the scale off, and `+` and `_` an octave.

A `Ramp` walks a run up or down the scale, `Random` picks degrees between the
ends, and `Wave` draws a contour. `Walk`'s Step is in degrees with the scale on,
so a Step of 1 moves by step through the scale and 2 adds thirds; with it off,
in semitones. `=` and `-` move it by one, and `+` and `_` by an octave of them.
`1/f` gives a line that wanders and comes back, more like a tune than
`Random`'s. `List` repeats a motif typed as notes, like
`c3 d#3 g3`, and `Alternate` two notes, both written as they are. `Invert`
turns a melody upside down within its range, in degrees with the scale on;
`Reverse` plays it backwards and `Shuffle` deals the same notes in a new order.

On notes, `Random` has two more rows, for lines that land on the notes that matter:

| Row | Does |
|---|---|
| Steps | How many degrees it plays, the most important first |
| Weights | How often each comes, a hex digit each from the root |

`Steps` starts at all of them, and `Weights` at `F` for each, the most; `0`
is never. `Steps` lets the degrees through in Marbles' order of importance: the root, then the fifth, the
third, the seventh, the sixth, the second and the fourth. So in C major `Steps` 1 plays only
Cs, 2 adds the Gs and 3 the Es, an arpeggio, and the panel names them: `3 of 7: C G E`. In a
minor pentatonic it's the root, the fifth, the minor third, the seventh and the fourth. With
the scale off it goes through the twelve semitones by how consonant each is with the root: C, G,
F, A, E, D#, G#, A#, D, B, C# and F#.

`Weights` is Vermona's meloDICER's row of faders, one for each degree: a hex digit each from the
root, with the note it's for under it, `C  D# F  G  A#` in C minor pentatonic, and with the scale
off a digit for each of the twelve semitones. `l` goes into the digits, on the first, and the
status line names the one under the cursor, `Weight of G: F`. Each change is heard as it's made,
and `Enter` on the panel keeps them all as one undo.

The digits are a field of tokens, each digit a token, and take the keys every text-like setting
is edited with, under Fields of tokens below. On `Weights`:

| Keys | Does |
|---|---|
| `h` `l` | Across the digits. `h` from the first goes back to the rows |
| `^` `$` | The first or last digit |
| `=` `-` | Up or down by one |
| `+` `_` | Up or down by 4 |
| `0` to `F` | Sets the digit, and moves on to the next |
| `r` `s` | Then a digit: sets it, and stays on it |
| `x` | Back to `F` |
| `~` | `0`, the digit rested, and again gives back what it was |
| `Esc` `Enter` | Back to the rows |

`0` types a 0 here, as each hex digit types itself, so `^` goes to the first, and `a` to `f` are
digits too. In C major `F08F` typed from the first keeps the second out and gives the third about
half the root's chances. `i` types them all at once: it starts from the digits there, and the
first digit typed starts them afresh, the degrees after those typed keeping theirs, while
`Backspace` edits them from the end. Spread and Bias still shape the notes let through, so a
narrow Spread stays in the middle of the range, and one value on a degree left out lands on the
nearest one let through, the lower of two as near.

Under the notes, `Rows` marks each row the fill writes `x`, `.` for one it leaves, so in the
digits the rows a digit writes, those whose notes are on its degree, are lit there and in the
Note column as `l` goes along, and while the phrase plays the digit of the note playing is green.
<!-- ANCHOR_END: after-bars -->
<!-- ANCHOR: after-weights -->


`Snap`, on the Note column only, moves each note to the nearest one in the
scale, the lower of two as near, for a take played without it. It's the only
way notes are forced into the scale: playing never does. A note outside the
scale shows in the panel in the grid's colour for one.

On Nudge, the ends run as far as the track's nudges go, `-B` to `+B` at `1/16`
`-2F` to `+2F` at `1/4` and `-7F` to `+7F` at `1/1`, in 24ths of a sixteenth, so `Random` between `-3`
and `+3` is a humanise you can see, and a `Ramp` rushes or drags into a fill. A
live one keeps the range it opened with. On a gate's length, a `Wave` or `List` of lengths shapes the
phrasing; a row with no length of its own, `··`, keeps it unless filled.

The panel uses the track page's keys: `j` `k`, `-` `=` `_` `+` and Cmd with `h` `l` `j` `k` for them, `i` to type, `K`
to choose from a list and `x` for the default. `Tab` still plays and pauses.
`.` applies the same shape to another selection. A ramp, random fill, walk or
`1/f` takes that selection's own ends, `Wave`, `Alternate` and `Scale` keep theirs, `List` keeps
its list, and the other transforms rework that selection's own values.

### Multiplying a rhythm

`Multiply`, on the Note and FX columns, reworks the rhythm there rather than
its values, after Noise Engineering's Numeric Repetitor: one rhythm, a family
of variations from one setting. Sixteen rows are read as a 16-bit number, a
note or the column's command a 1, the first row the lowest bit. The number is
multiplied by `Factor`, from 1 to 16, and kept to 16 bits, so what carries
past the sixteenth row is lost. A longer selection goes sixteen rows at a time.

| Row | Does |
|---|---|
| Factor | What the rhythm is multiplied by. `1` leaves it as it is |
| Masks | `On` thins the rhythm before 2 and 3 multiply it, as the module does |

Four on the floor, `x...x...x...x...`, times 3 is `xx..xx..xx..xx..` and
times 5 `x.x.x.x.x.x.x.x.`. An odd factor always keeps the downbeat, and 2, 4
and 8 move the rhythm 1, 2 or 3 rows later. A note the product keeps stays as
it is, a new one takes the note before it, with its velocity and gate, and one
it drops is cleared; on an FX column the same goes for the command and its
value. An `OFF` stays, and so does another command in the slot. It has no
`Where`, as it reads the rhythm itself. With Masks on, 2 keeps only rows 1–4
and 9–12 before it multiplies, and 3 rows 1, 2 and 13–16. Factor starts at 3:
`=` and `-` move it by one, `+` and `_` by 4, and `i` types it.

### Bits

Three transforms treat each value there as a binary number and work on its
bits, after Xaoc Devices' Leibniz, whose modules invert and mask a number's
bits for odd quantizations and folds. A value has as many bits as its
column's highest needs: 7 on Velocity, 8 on most FX commands and lengths, and
5 on a nudge at `1/16`, as its values there run from 0 to 22. They rework the rows `Where` chooses, as the other transforms do.

| Fill | Row | Does |
|---|---|---|
| Invert bits | Flip | Flips the bits Flip has: `FF` flips them all |
| Mask bits | Keep | Keeps only the bits Keep has |
| Rotate bits | Turn | Turns them round, the highest back as the lowest |

On Velocity, Flip `FF` makes `35` into `4A`, Keep `F0` steps the velocities
in `10`s, and Turn 1 doubles each, the loudest coming round to quiet. Flip
and Keep are typed as two hex digits with `i`, and `=` and `-` move them
by one, `+` and `_` by `10`; Turn goes from 0 to 7. The panel shows Flip and
Keep as their eight bits, the lowest first, `x` for on, with the byte beside
them, `. . . . x x x x   F0`, so a mask reads as what it keeps. `l` goes into
the bits, `=` and `-` switch the one under the cursor, and `1` and `0` set it and
move on; on the row, as for macros and the Roto, they step the byte as a number. Flipping the same bits
twice gives the values back. A value pushed past the column's highest stops
there, as a nudge's 5 bits go to 31 but a nudge only to `+B`.

On the Note column the number is the note's degree, counted up the scale from
the lowest note while the scale is on, so every note that comes out is in the
scale; with it off, it's semitones. In C major, C-3 is degree 28, `11100`, so
Flip `01` makes it D-3 and Flip `03` F-3. With the scale off, C-3 is `30`, and
Flip `0C` takes it up an octave. Small masks, such as `01` to `07`, keep a line
near where it was.

### Drums from Grids' map

`Grids`, on the Note column only, writes a drum part after Mutable
Instruments' Grids, from its map of 25 drum patterns, each with a kick's, a
snare's and a hats' part. `X` and `Y` pick a place on the map, and the four
patterns nearest it are blended. Each step of a part has a level, how much it
belongs to the pattern, and a part's density is how many play: turning it up
adds the next most important hit first, so a kick grows from the downbeat
outwards. A step whose level is over Grids' second threshold is accented, `7F`.

| Row | Does |
|---|---|
| Part | Which part this track plays: `BD`, `SD` or `HH` |
| Note | On an empty column, the note it writes |
| X, Y | The place on the map, `00` to `FF` each way |
| BD fill | The kick's density; SD fill and HH fill the others' |
| Chaos | How far each step's level is raised at random |
| Rows | A row a 16th, `1/16`, or a 32nd, `1/32` |

Select a kick's, a snare's and a hat's tracks with `W`, `vGWW`, and `S`: each
track plays its own part of the same place on the map, so the three are one
beat, and the panel shows all three densities and no Part. A track's part comes
from its sound: an 808 `Kick`, `Snare` or `Hat` voice, or a name such as `BD`,
`SD`, `CP`, `CH` or `OH`, as a kit names its tracks. The others take the parts
left, in order, so three tracks with nothing to go by are the kick, the snare
and the hats.

A hit on a note keeps its pitch, and a new one takes the first note in the
rows, so type one note on each drum track, a low kick and a high hat, before
`S`. On an empty column it writes `Note`. A row that doesn't play is cleared,
and an `OFF` stays. Chaos makes weaker hits come and go: `r` rolls it again, and each bar of a longer
selection rolls its own. The map, X, Y and the densities move by one with `=`
and `-` and by `10` with `+` and `_`, and `i` types them in hex.

Kept live with `L`, it's a live column a track, each with its part, so `gl` on
the snare reopens the snare's alone, with its own density and the Part row; a
change there moves that track and leaves the others. On the Gen page its title
is its part, like `Grids  SD`.

### A chance for each division

`Divisions`, on the Note column only, writes a rhythm after Vermona's
randomRHYTHM: a chance for each note division to strike, the quarters, eighths
and sixteenths and the three triplets. Each strike a division could make in a
bar has a die of its own, and its chance is how high the die must come: at `0%`
the division never strikes, at `100%` it always does, and raising a chance only
adds strikes, as the module's sliders reshape its pattern. The dice are the same
each bar, so the bar repeats; `r` rolls new ones, and `Vary` rolls them again as
it plays, the module's `Realtime`.

| Row | Does |
|---|---|
| Note | On an empty column, the note it writes |
| Channel | `A` or `B`, the module's two, or `A+B`, both |
| Bar | 4/4 or 3/4, in 16ths, or in 32nds for a `1/32` track |
| 1/4 to 1/16T | Each division's chance, `0%` to `100%` |
| B 1/4 to B 1/16T | With `A+B`, channel B's chances |

The divisions share the track. The straight ones fall on the rows, and a
triplet between two rows is the nearer one's, nudged a third of a row early or
late. On a track at `1/32`, with `Bar` in 32nds, every division has rows of its
own: the sixteenths the even rows, the triplets the odd ones, nudged `4`
either way. At `1/16` the eighth and quarter triplets take the odd rows, nudged
`8`, which the sixteenths take too, so a row both strike plays the one on the
row, and the panel says `16ths and triplets need 32nds to fit`. The panel
starts `Bar` in 32nds on a `1/32` track.

The track's steps are its clock, as the module counts the clock it's given. On a
track at another speed the bar is as many of its rows, so at `1/2` the rhythm
plays eight times as slow, and a triplet is nudged a third of the track's row:
`10` at `1/8`, `40` at `1/2`, and `7F` at `1/1`, as far as a nudge goes.

Select three tracks, `vGWW`, and `S`: the first plays channel `A`, the second
`B`, with B's chances, and the third `A+B`, every row either of the others
strikes, as the module's combined output. The panel shows both channels'
chances and no Channel. All three take one seed, so the third is exactly the
other two; kept live, each is a generator of its own, so a change to the first
leaves the third as it was, unless a macro moves both.

A strike on a note keeps its pitch, and a new one takes the first note in the
rows, so type one note on each track first; on an empty column it writes
`Note`. A strike is a note at its nudge, so the generator owns the rows'
nudges; a row it doesn't strike is cleared, and an `OFF` stays. Kept live with
`L`, its title on the Gen page is its channel and bar, like
`Divisions  A  4/4 32nds`. With `A+B`, the twelve chances fill the device, so
its `Note` is set in the panel, with `gl`.

### A noise ring

`Ring`, on the Note column only, writes a rhythm and a melody together from one
shift register, after the Wiard Noise Ring and Malekko's Richter Noise Ring.
The register is clocked once a row: it takes a bit in, and the row plays if
that bit is a 1, at a pitch from the register's last eight bits read as a byte,
from `From` to `To` along the scale's degrees while it's on. So a note and the
notes after it are the same bits moved along, and the melody and the rhythm move
together.

| Row | Does |
|---|---|
| From, To | The lowest byte plays From and the highest To. `~` swaps them |
| Change | The chance a bit coming round is replaced by a new one, `0%` to `100%` |
| Chance | The chance a new bit is a 1: how many rows play |
| Length | How many bits go round, from 1 to 16; the module has 8 |

At `Change` `0%` the loop locks: the same `Length` bits go round, so the rhythm
and its pitches repeat every `Length` rows. Turned up, more of the bits are new
as they come round, until at `100%` every row is drawn afresh. `Chance` is the
density either way, as a new ring starts with bits drawn by it too, and raising
it only adds notes. A `Length` that doesn't go into the rows, such as 5 over 16,
comes in a row earlier each pass.

A row that plays over a note there keeps its velocity and gate and takes the
ring's pitch; a new one takes the first note's, or the velocity notes are typed
with on an empty column. A row that doesn't play is cleared, and an `OFF` stays.
`r` rolls a new ring.

Kept live with `L`, the register runs on from pass to pass, so each time round
the phrase plays where the last pass left it, as the module does, not the bar
written again; there's no `Vary`, as it varies by itself. Each pass is worked out
from the seed and its number, so playing from bar 33 hears what playing to it
would. On the Gen page, `Space g a` keeps the register as the pass just heard
began: that pass then plays first, the ring runs on from there, and `Change`
down to `0%` holds its loop, the Turing Machine's lock.

### Chords on the Tonnetz

`Tonnetz`, on the Note column only, writes a chord progression that walks the
Tonnetz, the lattice of the twelve notes by fifths and thirds, after Ornament &
Crime's Harrington 1200 and Automatonnetz. Each move keeps two notes of a major
or minor triad and moves the third by a semitone or a tone:

| Move | Does | C major to |
|---|---|---|
| `P` | Parallel: the third a semitone | C minor |
| `L` | Leading-tone: major root down a semitone, minor fifth up | E minor |
| `R` | Relative: major fifth up a tone, minor root down | A minor |

Each undoes itself: `L` from E minor is C major again. The first row `Where`
chooses plays the chord on `From`, and each row it chooses after plays the next
chord, one move on, so `Where` is the progression's rhythm: `Every 4` is a chord
a beat, and on a track at `1/2`, `All rows` is a chord each half note.

| Row | Does |
|---|---|
| From | The first chord's root |
| Triad | Whether the first chord is `Major` or `Minor` |
| Voice | What the track writes: `Chord`, `Root`, `Low`, `Mid` or `High` |
| Moves | `List`, as typed; `Random`, each by weight; `Loop`, a loop by weight |
| List | The moves, typed with `i` as letters, like `plrl`, up to 8 |
| P, L, R | Each move's weight, 0 to 9, for `Random` and `Loop` |
| Length | How many moves a `Loop` draws before it goes round, 1 to 16 |
| Scale | `Free`, or `In scale`: only moves that keep to the song's scale |

Each move is drawn over the chord it reaches, `P   L   R   L` over `Cm  G#  Fm  C#` from `C-3`,
and the panel's last line names the first chords as the list goes round, `C Cm G# Fm C# C#m A
F#m`. `l` goes into the moves: `=` and `-` go round `P`, `L` and `R`, so a progression is tried a
move at a time, the status line saying the chords it goes between, `Move 2: L, Cm to G#`, and `K`
lists the three with the chord each would reach from there.

**On the Chord voice.** `Voice` `Chord` writes each row's root, within a
tritone of `From`, and locks the voice's `Chord` to `M` or `m` in the first FX
slot that's free, or the one that already locks it, so one track plays the whole
progression; the row's other FX stay. On the `Chord` voice the panel starts on
`Chord`, and on any other on `Root`, the roots alone, as a bass.

**Across tracks.** `S` on three tracks at once gives each a voice of the triad
in turn: `Low` from the first chord's root, `Mid` from its third and `High` from
its fifth, in close position from `From`. All three walk from one seed, so they
play one progression, and from chord to chord only the track whose note the move
changes moves, by a semitone or a tone: the voice-leading is the Tonnetz's own.
A voice stays within a tritone of where it started: a move that would take it
further takes it an octave the other way, the one leap it makes, so a long walk
keeps each track in its register. A fourth track takes `Low` again. `gl` on one
track shows its own `Voice`.

**In scale.** A move that would bring in a note outside the scale isn't taken: a
`List` or a `Loop` goes on to its next move, and `Random` draws from the others;
with none left, the chord holds. In a major or minor key that leaves `P` out, as
it always brings a note from outside, and `L` and `R` walk round the key's six
major and minor chords. With the scale off it's free either way.

`Random` and `Loop` draw from the seed, so `r` rolls a new walk and they have
`Vary`; a `List` draws nothing.

### On an empty column

On a Note column with no notes in the rows selected, `S` writes notes of its
own, and the panel's title says `Generate`. `Where` gives the rhythm, from
`All rows`, `Every`, `Chance`, `Euclid`, Marbles' four, `Rhythm`, `Cells` and
`Beats`, and the fill the pitches: a `Ramp` from the scale's root an octave up, in the octave
notes are typed in, or a
`Random`, `Walk`, `1/f`, `Wave`, `Alternate` or `List`, in the scale's degrees
while it's on.
Each note takes the velocity notes are typed with and the track's gate, and an
`OFF` there stays. `Euclid` with 3 hits over 8 rows is a tresillo to start a
line from, `3 states` a line with some notes held into the next, and `Rhythm`
a clave or a bell.

On an FX column with no command in the rows selected, `S` first lists the
commands, as `K` does, and the one you choose is written on the rows `Where`
picks, every row to begin with, from its lowest value to its highest. `Esc` in
the list leaves the column as it was. A transform has nothing to rework on an
empty column, so it isn't offered. Velocity, Nudge and length still need notes.

### Acid lines

`Acid`, on the Note column only, writes a 303 line: notes, accents and slides
together, as Roland's and Arturia's 303s roll them. It works on any machine,
not only the `Acid`. It writes on every row its `Where` selects: on an empty
column it starts on `Beats`, a riff of its own with its rhythm weighted toward
the beats; on notes already there it starts on `Notes`, keeping their rhythm,
and another `Where`, such as `Euclid`, writes its own rows, notes of its own
where there were none.

| Row | Does |
|---|---|
| From, To | The riff's notes come from between them, and From is its root |
| Notes | How many notes the riff has, from 1 to 8, its root among them |
| Spread, Bias | Which notes between the ends the riff takes |
| Octave | The chance a note goes an octave up or down |
| Accent | The chance a note is accented, written as `7F` |
| Slide | The chance a note slides into the next, written as `HLD` |
| Pitches | `Roll` gives new pitches on each `r`, `Keep` keeps them |
| Trig loop | On `Beats`, how many rows the rhythm goes round |
| Note loop | How many rows the riff's notes go round |
| Oct loop | How many rows the octaves go round |
| Acc loop | How many rows the accents go round |
| Sld loop | How many rows the slides go round |

The riff's notes come from the scale while it's on, so a set of four in C minor
might be `C-3 D#3 G-3 A#3`, and the root plays more often than the others.
Spread and Bias choose the others, as they lean a `Random`'s values: as they
start, `50%` and `0`, any note between the ends is as likely; a narrow Spread
gathers them round the bias point, so `0%` with four notes from `C-2` to `C-3`,
the scale off, takes `F-2 F#2 G-2` with the root, and a wide one takes them from the ends inward; and Bias
leans them up or down, the root staying wherever From is. With Notes as many as
the ends hold, every note is in the riff, so there's nothing for them to choose.
Its rhythm is its `Where`'s: `Beats`, whose `Density` counts where each row falls
in the bar, counting from the first row selected, so at `50%` half the rows play,
most of them on the beat; `Density` `100%` plays every row. A
plain note takes the velocity notes are typed with, and an accent `7F`, as `'`
writes it; a slide is `HLD` on the gate, as `/` writes it.

**Loops.** Each part of the line can go round a loop of its own, after monome's
Kria: `Off`, as it starts, draws the part afresh on every row, the same each pass,
and `1` to `64` draws it for that many rows and goes round them. A `Trig loop`
of 12, a `Note loop` of 7 and an `Acc loop` of 3 play 12 rows of rhythm, 7
notes and 3 accents, each round and round against the others. Kept live, the
loops run on from pass to pass, so on a phrase of 16 rows the rhythm starts its
second pass four rows into its round, and the loops come round together only
after their least common multiple, 84 rows for those three. The phrase shows the
first pass; the engine plays the rest. `Trig loop` loops `Beats`' draws, so on
any other `Where` there's none, and with Pitches on `Keep` on the notes there, no
`Note loop` either.

`r` rolls a new riff, rhythm and all. With Pitches on `Keep`, `r` rolls only
the accents, slides and octaves, and the notes and rhythm stay: Roland's second
die. On notes already there, `Keep` keeps their own pitches, so a line you wrote
gets new accents and slides and nothing else. Keep it live with `L` to come back
with `gl` and roll again while it loops, until one sticks.

### Following another track

`Follow`, on the Note column only, writes another phrase's notes into the rows
selected, with their velocities, gates, accents and slides: the voices of a
canon. Kept live with `L`, the column mirrors its source, so an edit to the
source rewrites it in the same edit, as one undo with it. Each track still owns
its phrases: the follower keeps its own copy of the notes, which the source
writes again whenever it changes, and plays them at its own track's `Speed`.

| Row | Does |
|---|---|
| Source | What it follows: `T02:05` is track 2's phrase `05`. `K` lists them |
| Transpose | Up or down, in the scale's degrees, or semitones with it off |
| Invert | Turns the notes upside down, about the source's first note |
| About | With Invert on, the note it turns about: `First`, or one chosen |
| Reverse | Plays the source backwards, its last row first |
| Rotate | Comes in that many rows late, going round the source |
| FX | Copies the source's FX columns too, off at first |

FX starts off, as a lock may mean something else on another machine.

`Source` starts on the phrase beside it on the nearest track before, or after
if there's none, or on what that phrase follows, so each voice of a canon starts
on the theme. A source shorter than the rows selected loops to fill them, and a
longer one is cut off. `Enter` writes a one-off copy as plain notes.

It's a live column like any other: `gl` reopens it to change the source or the
moves, `Space g a` freezes it as plain notes, and typing over one of its notes
takes it over by hand, with `u` to make it live again. Its phrase's title names
it with its source, like `󰁨 Note←T02:05`, and the source's title says how many
follow it, like `Followed by 3`. A phrase can follow one that follows another,
but never one that follows it back, directly or through others: those aren't
offered, and `L` refuses one. Deleting the source's track with `:track del`
leaves its followers as plain notes, and says so.

### Mutating, and growing branches

With `Mutation` above `Off`, a `Follow` changes what it copies so many times,
drawn from its seed, as Qu-Bit's Bloom grows its branches. Each change is one of
the kinds switched on, which show once it mutates:

| Row | Does |
|---|---|
| Mutation | How many changes, `Off` up to 32 |
| Repitch | Moves a note a degree or two, in the scale while it's on |
| Shift | Moves every note together, one to four degrees. Off at first |
| Retro | Plays two to four rows backwards |
| Ratchet | Repeats a note in its row, by a `RET` in its first empty FX slot |
| Drop/add | Takes a note away, or adds one on an empty row |

The changes fall in the same places each time, so an edit to the source comes
through everywhere but the rows they change, and `r` draws others. A note it
moves lands in the scale while the scale is on. As it draws from its seed, the
Gen page gives it `Vary`, so a new set of changes plays every pass or every few.

`:bloom` grows branches from the phrase under the cursor, the trunk. Each branch
is a new phrase on the same track, a copy of the trunk whose Note column follows
the branch before it, the first the trunk, with `Mutation` on, FX and all. So
each strays a little further from the trunk, and an edit to the trunk grows
every branch again, in the same undo step. The trunk's row in its chain becomes
the path through them, each row with the trunk's transpose.

| Command | Does |
|---|---|
| `:bloom` | Choose a path from a list, and grow three branches |
| `:bloom 5` | Five branches, with two changes each, played forward |
| `:bloom 4 3 trunk` | Four branches, three changes each, the trunk before each |

| Path | Plays, with three branches |
|---|---|
| `forward` | `T 1 2 3` |
| `trunk` | `T 1 T 2 T 3` |
| `pendulum` | `T 1 2 3 2 1` |
| `ping-pong` | `T 1 2 3 3 2 1 T` |
| `shuffle` | `T`, then each branch once, in an order drawn from the seed |

Up to seven branches, on the lowest free phrase numbers. Each is a live column
like any other: `gl` on one, or the Gen page, changes its Mutation and kinds,
and the branches after it follow; `Space g a` freezes it as plain notes. A macro
mapped to `Mutation` turns every branch at once, as it holds the Note column in
every phrase of its track. Another `:bloom` draws other branches, and `u` takes
the whole bloom back.

### Variations of a line

`Variation`, on the Note column only, writes something like the line already
there: give me another one of these. It learns which row follows which from
the rows selected and nothing else, a Markov chain, and writes a new line of
the same length from what it learned. Each row is learned as its note, in the
scale's degrees while the scale is on, whether it's accented (`70` and up, or
no velocity) and whether it slides (`HLD`), or as a rest or an `OFF`, so the
rhythm comes with the notes. A row it writes copies a note's velocity and gate
from a row of the line that played it there.

| Row | Does |
|---|---|
| Order | How many rows before each row it looks at, from 1 to 3 |
| Temp | The temperature: how far from the likeliest row it strays, `0%` to `100%` |

At Order 1 a row follows any row the line has after the one before it, so the
line comes apart and is put back another way: loose. At Order 3 it follows only
what came after the same three rows: close. A short line, where few runs of rows
come twice, comes back as it was at Order 2 or 3, so those are for longer lines,
two bars or more of a tune that returns to its figures. At `0%` it always writes the likeliest row, at `50%`, where it
starts, each as often as the line has it, and at `100%` any of them alike; it
never writes a row after one that the line never had there. The line goes round
as a loop does, so its last rows lead into its first, and the variation starts
where the loop would.

`r` rolls another. It learns from the first 256 rows selected, and needs four
notes there: with fewer, the panel says so and `Enter` and `L` refuse it. Kept
live with `L`, it learns from the notes under it, never from what it wrote, so
`gl` and another Order or `r` give a new variation of the same line, `Space g a`
keeps the one you like, and on the Gen page `Vary` plays a new one every pass or
every few.

### Keeping it live

`L` in place of `Enter` keeps the shape live: the column stays tied to its
settings, so you can come back and change them while the loop plays. A
Euclidean velocity accent is the one to try: keep it live, then turn its
`Rotate` later.

| Keys | Does |
|---|---|
| `L` | In the panel, keep it live, as one undo |
| `gl` | Open the panel again on the live column here |
| `Space 8`, the Gen page | Every live column of the phrase, as devices |
| `Space g a`, `:gen apply` | Apply it: keep its values as plain ones |

`gl` works from any row of the column, and after the song is saved and opened
again. In the panel it opens, `L` keeps the new settings live, `Enter` applies
it with them, and `Esc` leaves it as it was. A live column's values are drawn
in the shape panel's colour, and the phrase's title names its live columns
after a wand, like `󰁨 Vel,FX1`.

Typing over one of its values takes over by hand: it's applied, its values
stay, and the status line says so; `u` makes it live again. Editing any other
column leaves it live. Where it chooses its rows by the notes, as Velocity,
Nudge and length always do and an FX column does with Where on Notes or No
notes, editing the notes fills it again, so a note added gets a generated
velocity, as one undo with the note. Under the hood it keeps what was in its
rows before it, so turning `Rotate` puts back the values of the rows the hits
leave. Notes generated on an empty column keep live the same way, with their
velocities as its own too: turning `Rotate` moves the notes along, and `gl`
opens it generating again.

**The scale.** A live column of notes follows the song's scale as it is now.
Change the scale, with `:set scale=`, `:set root=` or the project page, and
every live Note column in the song is filled again through the new one at
once, as it plays: the grid shows the new notes, and they're what's saved. It
isn't an undo step, as the scale itself isn't, and `u` never brings back notes
in the old scale. Notes applied, or typed, are plain ones, which a scale change
leaves where they are.

**While it moves.** A `MAC` lock, a glide or an LFO on a macro holding one of
its settings moves it as it plays, and so does `Vary`; the grid still shows
what's written, which is what you edit. The phrase's title marks the column
with `~`, like `󰁨 Vel~`, once what was heard isn't what's written. The row
playing draws what's heard there, as the overview does for every track, and a
lane's live column the row its lane plays. On a cell heard otherwise, the
status line says both, like `C-2 written; D#2 heard on pass 3`. `Space g a`
then applies it as heard: each row as it was last heard, the rows not heard yet
as written, and the status line counts both. `Space g A` does the same for
every moving live column of the phrase, with the pitch the mod slots moved.

On an FX column with a lane, a live column fills the lane's steps, whichever
rows you selected, and follows the lane when `:lane`, `gs`, `ge` or `]l` move
it; `:lane` alone applies it. Across tracks, each track gets a live column of
its own, which `gl` opens and `Space g a` applies on its own. Doubling a phrase
applies its live columns, and so does `]r` on the rows it rotates. A new live
shape over one on the same column applies the old one first, and starts from
the values it left.

## Fields of tokens

A setting whose value is a short text with a grammar of its own, `Weights`, a `List`'s values, a
`Tonnetz`'s moves, a mask's bits and an `Own` rhythm's or a `Mini`'s `Text`, is a field of tokens: `l` goes
into it, the cursor on a
whole token, a digit, a note, a value or a group, and the keys move by the grammar and turn a
token in its own units. A **group** is a bracket and what it holds, `[. 7F]` or `<30 40>`, and a
**rest** is `.`, with `~` read as one too, as Tidal writes it.

| Keys | Does |
|---|---|
| `h` `l` | The token before or after, in and out of groups; out from the first |
| `w` `b` | The next or previous token at the same depth, a group as one |
| `^` `$` | The first or last token |
| `%` | The bracket that matches the one under the cursor |
| `(` `)` | Out to the start or the end of the group the cursor is in |
| `=` `-` | Turns the token by one of its units |
| `+` `_` | Turns it by a large step |
| `x` | Takes the token away; on a fixed length, back to how it starts |
| `~` | Rests the token, and again gives back what it was |
| `r` | Replaces the token: `r`, then the new one, and back on it |
| `s` | Types over the token |
| `i` `a` | Types before or after the token, or on a fixed length types it all |
| `p` | Pastes after the token, or on a fixed length over it |
| `Esc` | Back to the row, putting back the last text that read |
| `Enter` | Back to the row, where the text reads |

A count before a key goes so far, `3l` or `2w`, where digits aren't tokens of the field. `d`, `c`
and `y` take a text object, and `p` pastes what they took:

| Keys | Takes away, types over, or keeps |
|---|---|
| `dit` `cit` `yit` | The token |
| `dat` `cat` `yat` | The token and what changes it after it, `*2` or `?` |
| `di[` `ci[` `yi[` | What the `[ ]` group holds |
| `da[` `ca[` `ya[` | The `[ ]` group, brackets and all |
| `di<` `ci<` `yi<` | What the `< >` group holds |
| `da<` `ca<` `ya<` | The `< >` group |
| `di(` `ci(` `yi(` | A Euclid's numbers |
| `da(` `ca(` `ya(` | A Euclid, with its brackets |

Keys a field doesn't have are the panel's: `j`, `k` and `L` leave the tokens and do what they do
on the rows, while `Tab`, `Space d` and `Space ?` leave the cursor where it is, and `Space d` says
what the token under it does. A field of fixed length, as `Weights` and a mask are, shows every
token and never loses or gains one, and on `Weights` the letters `a` to `f` are digits, so `a`,
`b`, `c` and `d` aren't there. A list has room for 256 values and a `Tonnetz` 8 moves, and the
status line says the room there is. On the Gen page, `l` on a cell in a grammar goes into its
tokens the same way, drawn over the cell with what each is for under it, each change one undo
step; a key the field doesn't have leaves it and does what it does on the page.

**What can go here.** `K` on a token lists what it could be, under it, each with what it means,
opened on the one it is: on a weight the sixteen weights, `0  never` to `F  the most`. `j` and
`k`, the arrows, `Tab` and `Shift-Tab` move through the list, `Enter` puts the one highlighted in
the token's place, and `Esc` closes it. While typing, `K` before anything's typed of a word lists
what can go there, and `Tab` completes the word from the same list, as on the `:` line: the only
match goes in, or what every match starts with and the list of them, which narrows as you type;
`Tab` and `Shift-Tab` go through it and `Enter` puts one in. `Tab` plays again once you're back
among the tokens. What's listed comes from the track: a list's note lists the scale's notes an
octave either way of it, a degree's the scale's degrees named by their notes, an FX value's the
command's values, named where `K` names them on a row, as a condition's are, and a group's
bracket the other kind of group or a rest in its place. FX commands are still chosen with `K` on
the column, never typed: a field on an FX column holds only the command's values.
<!-- ANCHOR_END: after-weights -->
<!-- ANCHOR: after-offer -->

**Text that doesn't read.** Every change reads the text again, and the column plays what it says
at once. Where it doesn't read, the token it stops at is drawn in red with `✗` under it, and the
status line says why, `g isn't a weight, a hex digit from 0 to F`. The column keeps playing the
last text that read, `Enter` is refused, saying why, and `Esc` puts back the last that read.

**The rows a token writes.** Under the field, the rows line says each row of the span as the text
writes it, a mark a row: `x` where a token writes it, `?` where one with a chance after it does,
and `.` where nothing does. The marks are spaced while they fit, and past that a page of them at a
time, the page with the rows lit.

```
Weights F  F  8  F  F
        C  D# F  G  A#
Rows    x x x x x x x x x x x x x x x x
```

In the tokens, the rows the token under the cursor writes are lit as it is, in the rows line and
in the phrase view's column, so moving along the field moves the light along the rows; the grid's
own cursor stays the cursor. While the phrase plays, the token that wrote the row playing is
green, the playhead's colour, and so is that row's mark. A generator whose `Vary` gives a pass a
seed of its own lights the token of the pass playing. While the text doesn't read, the line is of
the last text that did, and says `still the last text that read`.

## Values and small edits

| Keys | Does |
|---|---|
| `x` | Clear this field |
| `r` | Replace this field, as in Vim: no move, and back to Normal |
| `s` | Clear this field and start Insert there, as in Vim |
| `=` `-` | Up or down a small step. `Ctrl-a` and `Ctrl-x` do the same |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `Cmd-l` `Cmd-h` | As `=` `-`, without moving, here and wherever they work |
| `Cmd-k` `Cmd-j` | As `+` `_`, without moving, here and wherever they work |
| `K` | On an FX column, choose from a list |
| `.` | Repeat the last change |
| `p` | Paste over, from the cursor down. Past the end it stores more |
| `3p` | Paste 3 times, end to end, as one undo |
| `gp` | Paste over, and land just after it, ready for the next |
| `gm` | Merge: empty cells in the copy leave what is there alone |
| `"a` | Use register `a`, before `y`, `d` or `p`. `a` to `z` all work |
| `"0` | The last thing you copied with `y`, which a cut can't replace |
| `u` `Ctrl-r` | Undo and redo. An edit that changed nothing isn't one to undo |

A small step is a semitone, or 1 on a value. A large step is an octave, or `10`
in hex, which changes the left digit. Counts work: `3=`. `RET` is different,
since its digits mean separate things: `=` `-` change only the right digit,
the rate, and `+` `_` only the left, the fade, so one never carries into the
other.

## The mouse wheel

With `Wheel` on, on the audio page (`Space 9`, then `L`), the mouse wheel or the
trackpad is a knob on whatever the cursor is on, as Cmd with `h` `j` `k` `l` is from the
keyboard. Where the pointer is doesn't matter.

| Wheel | Does |
|---|---|
| Fingers up or right | As `=`, in the fine step, more the faster |
| Fingers down or left | As `-`, in the fine step, more the faster |
| On a list or a toggle | A choice at a time, stopping at the ends |

Up and down and sideways are the same knob, and work the same, as a trackpad's
fingers drift from one to the other. The value follows the fingers, as a level
bar fills, whether or not macOS scrolls naturally; the audio page's `Direction`
row turns it the other way. The terminal sends about twice as many ticks for a
swipe sideways, as its columns are narrower than its rows, so an up or down tick
counts for two sideways ones, and the same swipe goes as far either way.

Speed sizes each tick, and Shift does nothing. On a value with fine digits a
slow sideways tick is a 16th of `=`'s step, a step of the first fine digit, as
if the cursor were on them, and an up tick two; with the cursor on the fine
digits themselves, a sideways tick is their own `=`. On the tempo a slow
sideways tick is a tenth of a BPM, or a hundredth on its fine part, and on the
sample page a 16th of a pixel, as Cmd-l and Cmd-h nudge a point. A slow or
steady turn moves the same for every tick, so twice the swipe is twice the
change. A faster swipe takes more each tick, rising evenly with its speed up to
`=`'s whole step. Where `=` is already the finest step, a semitone on a note,
a tick either way is one `=`, however fast. On a selection it moves every
value, as `=` does, and keeps it selected.

A list or a toggle goes a choice at a time, and stops at either end where `=`
goes round. Up or right puts a toggle on, and down or left off. The first tick
moves it, and then every third, at most four times a second, so a slow turn
steps through. A flick moves it a choice or two, and the momentum after it,
or its bounce at the end, moves it no further, so a toggle never flips back.
Another flick moves it on. To go far in a long list, `K` lists it. The wheel
leaves the audio page's `Direction` row alone, as turning it would turn the
wheel round.

After a quick swipe, macOS goes on scrolling for about a second once the fingers
lift. A tick the other way in that time is taken as its bounce and let go,
unless three come in a row or the wheel was still for a moment first. On a list
or a toggle that's so after any turn.

One turn is one undo: `u` takes back everything from the first tick until the
wheel stops for a moment, its momentum included, or a key is pressed. `.` still
repeats the last change made with keys.

`K` on an FX column lists the commands by group, searching as it opens. If the
slot is empty, type the value next. If it has one, it stays.

`r` then `j` changes a note to B. On other columns it takes a whole value:
`r40` for a velocity, or `rCUT`, `Enter`, `40` for an FX slot. A
note keeps its velocity, `.` repeats the replace, and `Esc`
cancels it. The cursor and `REPLACE` on the status line turn red while it
waits.

`s` clears the field, and then typing works as in any Insert, moving down
after each entry. On an FX value it keeps the command, so `s60` on `CUT40`
makes `CUT60`.

## The loop

A phrase can store more than it plays, like a clip. The loop is the part
that plays, and everything else stays dim and silent until you move the loop
over it. You can still edit, copy and paste out there.

Rows count from the start of the loop. `00` is always beat 1, and rows above
it are negative, so yellow beats are always `00`, `04`, `08`.

| Keys | Does |
|---|---|
| `]l` `[l` | Slide the loop a step later or earlier |
| `]L` `[L` | Slide it a beat later or earlier. Counts work: `4]L` |
| `gs` | Start the loop on the cursor's step |
| `ge` | End the loop on the cursor's step |
| `]r` `[r` | Rotate the loop's steps later or earlier |
| `:len 32` | Set how many steps the loop plays. Or in hex: `:len 0x20` |
| `:double` or `Space pd` | Repeat the loop so it is twice as long |

Sliding works while playing, and stays in time. `00` stays where it is and
the notes move under it. Sliding past either end adds empty steps.

Rotating wraps whole steps round within the loop, and leaves dim steps
alone. It works while playing, and counts work: `3]r`.

`ge`, `:len` and `:double` work while playing too. Each track stays on the row it was going
to play next, or starts its chain again if that row has gone. The loop is saved with the
phrase, along with what is outside it.

A phrase used in two places shares one loop. Copy it with `gc` to give one
place its own.

## Order

A phrase's rows play top to bottom unless it's given another order, as the
Metropolix's ORDER does. `:order` lists them, opened on the one it plays, and
`:order up-down` sets one by name. The title names an order other than
forward, like `Up-down`, and undo puts it back.

An order is a walk the track keeps taking. A pass of the phrase stays its
length in steps, so the chain and the section keep theirs, and the walk carries
on from one pass to the next.

| Order | Plays, over two passes of a phrase of four |
|---|---|
| `forward` | 1 2 3 4, 1 2 3 4 |
| `reverse` | 4 3 2 1, 4 3 2 1 |
| `up-down` | 1 2 3 4, 3 2 1 2, the end rows once |
| `up-down-inc` | 1 2 3 4, 4 3 2 1, the end rows twice |
| `down-up` | 4 3 2 1, 2 3 4 3, from the last row |
| `down-up-inc` | 4 3 2 1, 1 2 3 4, from the last row |
| `odd-even` | 1 3 2 4, 1 3 2 4 |
| `jump 3` | 1 4 3 2, 1 4 3 2, so many rows on each time |
| `hopscotch` | 1 3 2 4, 3 1 4 2, each row then the one two on |
| `arp 3 1` | 1 2 3 2, 3 4 3 4, runs of 3, each a row on |
| `converge` | 1 4 2 3, from both ends in |
| `diverge` | 2 3 1 4, from the middle out |
| `growing` | 1 1 2 1, 2 3 1 2, runs from the first, growing |
| `shrinking` | 1 2 3 4, 1 2 3 1, runs from the first, shrinking |
| `pedal` | 1 2 1 3, 1 4 1 2, the first between the others |
| `brownian` | a drunken walk from the first row each pass |
| `shuffle` | each row once a pass, dealt again each pass |
| `quasi-random` | any row each step but the one just played |
| `random` | any row each step, so rows repeat |
| `snake spiral` | the rows as a grid, wound through by a path |
| `cartesian 1 3` | the rows as a grid, crossed by two counters |

`:order jump` takes how many rows on, 2 to 16, and alone is `jump 3`. `:order
arp` takes how many rows a run is, 2 to 16, and how far on each starts, 1 to 16,
and alone is `arp 3 1`; the Metropolix's `Arp 4 Shift -2` is `arp 4 1` and its
`Arp 4 Shift -1` is `arp 4 2`. The status line spells out the rows they play.
`brownian` is the Metropolix's drunken walk: half the time a row on, a quarter
the same row and a quarter a row back. It starts from the first row each pass,
dealt from the `:seed` as `shuffle` and `random` are, so a render repeats.

`snake` and `cartesian` read the rows as a grid, as Make Noise's René does:
lines of four rows, a beat of sixteenths each, so 16 rows are 4×4, and lines
of eight above 32 rows, so 64 are 8×8. A last line the rows don't fill is
passed over where they run out.

`:order snake` and a snake's word or number winds a path through every cell:

| Snake | Plays, in a phrase of 16 |
|---|---|
| 1 `zigzag` | along a line, back along the next: 1 2 3 4 8 7 … |
| 2 `columns` | down each column: 1 5 9 13 2 6 10 14 3 … |
| 3 `column-zigzag` | down a column, up the next: 1 5 9 13 14 10 … |
| 4 `diagonal` | along the diagonals: 1 2 5 3 6 9 4 7 … |
| 5 `diagonal-zigzag` | the diagonals, turning: 1 2 5 9 6 3 4 7 … |
| 6 `spiral` | round the edge and in: 1 2 3 4 8 12 16 15 … |
| 7 `spiral-out` | the spiral backwards: 10 11 7 6 5 9 13 … |
| 8 `checker` | a chessboard's black, then white: 1 3 6 8 9 … |

`:order cartesian 1 3` runs two counters: X moves along a line every row,
and Y down the lines every 3 rows, and the row played is the one at X and Y,
Y × the width + X. In a phrase of 16 that's 1 2 3 8 5 6 11 12 9 14 15 16, and
round again. Each rate is 1 to 16 rows. Two more numbers set how far each
counts, 1 to 8, so `:order cartesian 1 3 4 2` keeps X to 4 cells and Y to the
top two lines; without them X counts the grid's width and Y its height, up to
8 lines. Equal rates move both together, on the diagonal; Y's rate at the
width plays the grid forward. `:order cartesian` alone is `1 3`. The title
shows the order as it's typed, `Snake spiral` or `Cartesian 1 3`, and `:order`
lists each snake and Cartesian, with the phrase's own numbers if it has them.

The up-and-down orders, `pedal`, `hopscotch`, the jumps, arps and runs and most
of Cartesian's counters take longer
than a pass to come round, so they drift against other tracks as a polymeter
does. The walk starts again
where the chain does: from the top, a section that brings it in, a launch, or
`:reset`. A row's pulses stay together, so a row whose pulses run past the end
of a pass plays the rest at the start of the next, and a `HLD` ties into
whichever row plays next. A `CND 1:2` counts passes, whatever the order, and
an `ACU` adds each time its row plays. `shuffle` and `random` are dealt from
the project's `:seed` and the pass, so a render repeats, playing from a step
hears what playing to it would, and `CHA` rolls as it did. Lanes keep their
own loops, forward. The playhead shows the row playing, green where something is
written and a darker green where a cell is empty, and `Space c` on a row
plays from that row's step, with the walk wherever it is there. Changing the
order while it plays keeps the row that was next, where the new walk plays it
in the same pass, or stays put if that pass doesn't play it.

**The track's Phrase settings** move the walk live, as the Metropolix's ORDER
and Direction do when they're turned or sent CV. They're the track's, on the
Gen page's `Phrase` device, so they move every phrase it plays, and `:order`
stays each phrase's own:

| Setting | Does |
|---|---|
| `Order` | Places on through the list from the phrase's own: `+0` its own |
| `Reverse` | The walk the other way up: `up-down` as `down-up` |
| `Order X` | The first number: a jump's, an arp's notes, the snake, X's rate |
| `Order Y` | The second number: an arp's step, Y's rate |

From `forward`, `Order` `+1` is `reverse` and `+2` `up-down`, and it stops at
either end of the list. `Order X` and `Order Y` at `Own` keep the phrase's own
numbers. They're read before the row plays, as `JMP` is: an `ORD` on a row moves the step
that walks to that row, a lane of it reaches the steps it's at, and a macro, a
mod slot or an LFO can turn them. The walk is still worked out from the place
in the pass, so a pass keeps its length, the order changes on the next step, and
a render repeats; `shuffle` and `random` are still dealt each pass. While
they move it, the title names the order heard with the phrase's own after it,
`Hopscotch reversed (Jump 3)`, and the playhead follows the row playing.
`Order` and `Reverse` choose the walk and `Jump` then scrambles it.

`Phrase` also has the window of rows the order walks, as Kria's loop and the
Cirklon's last step set one live. `Length` is rows up or down from the loop's,
`+0` as written: at `-4` a loop of 8 walks its first 4, and at `+2` it walks 10,
its first two again after its last. `Start` is the row the window starts on,
rows on from the first, going round the loop. A pass still lasts the loop's
steps, so the chain and the section keep time, and the walk carries on from one
pass to the next, as it does for `up-down`: 6 rows walked in passes of 8 play `1
2 3 4 5 6 1 2`, then `3 4 5 6 1 2 3 4`. `Reverse` turns the window the other way
up. `PLEN` and `PSTA` on a row move the step that walks to it, and the title
names the window, `Forward, 4 rows from 3 (Forward)`.

**Cue**, beside them, makes a new window wait for the next pass, as Kria's
division cueing makes a new division wait for its loop to restart. It's `Off` as
it starts, and a turn is heard on the next step. `On`, the pass takes `Length`
and `Start` at its first step and keeps them to its last, so a macro or an LFO
on them moves the window a pass at a time and the loop's phrasing holds. It
holds a lane the same way, its `Length`, `Rate` and `Start` taken as it comes
round to its first row. The order itself, `Order`, `Reverse`, `Order X` and
`Order Y`, still moves at once, as the Metropolix's does.

**Jump** scrambles an order by degrees: it's the chance each step plays any row
of the window instead of the one the order walks to, and then the walk carries
on where it was. With the window as written that's any row of the loop; a
shorter one keeps the jumps to its rows, and under **Cue** to the window the
pass plays, as Kria's random direction stays within the loop's ends. `0 %` plays the order as it is and `100 %` any row each step,
as `random` does, so turning it up takes any order from as written to scrambled,
as Marbles' Déjà vu knob does past noon. It's a track's setting, `Jump` in the
title of the Sound page's Play device, so it scrambles whatever phrase the track
plays, and its chain stays as it is: a jump never leaves the phrase. The row
jumped to plays whole, its note, velocity, gate and FX, as if the order had
walked there, its locks too, for as long as the row walked to would have
lasted. A row that takes no time is never jumped to. `JMP` on a row sets the
chance for the step that walks to it, a lane of it reaches the steps it's at, a
mod slot or a macro can move it, and the Roto has it on the Play device's
second page. Lanes keep their own loops, and a `HLD` on the row jumped to ties
into the note before it, whichever row that was. Jumps are rolled from the
`:seed` and the pass, so a render repeats, and the playhead shows the row
jumped to.

## Lanes

An FX column can loop on its own, apart from the notes: a lane. It keeps
counting across the phrase's repeats, so a lane of 3 against a loop of 16 lands
somewhere new each pass, as tracks of different lengths do. A column follows
the notes until you give it a lane, so changing the phrase's loop changes it too.

| Keys | Does, on an FX column |
|---|---|
| `:lane 3` | Make it a lane of 3 steps, from the cursor's step |
| `:lane 4/2` | A lane of 4 rows, each lasting 2 of the track's steps |
| `:lane /3` | Each row of its lane lasts 3 steps, the lane kept as it is |
| `:lane` | Follow the notes again |
| `gs`, `ge`, `:len 3` | On a lane, set its start, end or length |

On a column without a lane, and on the note and velocity columns, `gs`, `ge`
and `:len` set the phrase's loop. On a lane, `gg` and `G` go to its first and
last steps, and `5G` to its fifth. A lane shows its length in the title, like
`FX1:3`, and `FX1:4/2` where its rows last more than a step. It dims the steps
outside it, and has its own `>` while playing. Sliding the loop with `]l`
moves lanes with it. Editing works on rows as usual, whatever loop a column
plays. Anything that starts a track's chain over starts its lanes over too, so
playing from a step hears what playing to it would. A glide in a lane goes to
the next value in that lane.

**A slow row.** A lane's row can last from 1 to 16 of the track's steps, as
Kria's divisions do: `:lane 4/2` takes 8 steps to come round. A slow row is
read as a row of several pulses is: its FX run on its first step, so a `RET`
or a `CHA` acts once, and a lock or a hold of it holds through the rest of the
row, a note there or not. Its playhead stays on it for as long, and the phrase
view adds no lines for it.

**Turned from the track.** Each FX column has a `Length` and a `Rate` on the
Gen page's `Lane` device (`Space 8`), which move every phrase's lane on that
column from its own: at `+2` a lane of 4 plays 6 rows and one of 7 plays 9, and
at `÷2` each row lasts twice its own steps, half as fast, and at `×2` half its
own, though never under a step. A lane heard longer than its own
plays its own rows again from the first, so `+2` on `a b c d` plays
`a b c d a b`, and `+4` an exact double; shorter, it plays its first rows. Nothing
is written, so `+0` gives the lane back as it was. While they move it, the title
shows what's heard first and its own after, dim, as `FX1:6/4 (4/2)`, the
playhead sits on the row being read, a copy's own, and rows past a shortened
lane are dim. A turn takes effect at the lane's next row, which counts along
from where it is rather than jumping; playing from a step works its place out
again there. A column without a lane isn't touched. `Start` moves the row each
lane starts on, so many of its own rows on, going round them: at `1`, `a b c d`
plays `b c d a`, and the title adds it, `FX1:4 from 2 (4)`. `Jump` is the
chance each of the lane's rows plays any of the rows it's heard for instead of
the next, as `Jump` is the phrase's: `00` never, `FF` every row. A jump takes one
row's place, for all its steps, and the lane carries on counting from where it
was; it picks from the rows its `Length` and `Start` give, as `Order` walks them,
and the playhead follows it. `Hold` is the chance each of the lane's rows plays the row the lane
played before again instead of its own, so its value holds, as a failed roll does
in Kria: `00` never, `FF` the lane's first row throughout. A hold, too, takes one
row's place for all its steps, and the lane counts on. `Order` walks the lane's
rows in another order from the same list as the chain's `Order`, `Forward` as
written, and `Reverse` turns the walk the other way up: `Up-down` on `a b c d`
plays `a b c d c b a b c d c b`, its round of six drifting across laps of four.
`Jump` and `Hold` act on top of it. A step locks them with `LLEN1`, `LRAT1`,
`LSTA1`, `LJMP1`, `LHLD1`, `LORD1` and `LREV1`, `2` and `3` for the other
columns, and a mod slot, a macro and the Roto turn them too.

`gg`, `G`, `{`, `}`, `ib`, `ic` and `ip` go by the loop. `5G` means row `04`.
`j` and `k` carry on past its edges. `Space c` on a dim row plays from `00`, and
`;` there plays just that row's note.

<!-- ANCHOR_END: after-offer -->
