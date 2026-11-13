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
| `D-l` `D-h` `D-k` `D-j` | Cmd and `l` `h` `k` `j`: as `=` `-` `+` `_` |
| The mouse wheel | As `=` `-`, and with Shift as `+` `_`, once it's on |
| `>` `<`, `C-a` `C-x` | Move every value a step, and stay selected |
| `r` | Fill the cursor's column, and stay selected |
| `u` `C-r` | Undo or redo one change, and stay selected |
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

| Row | Does |
|---|---|
| Fill | `Ramp`, `Random`, `Walk`, `1/f`, `Wave`, `List`, `Acid`, `Follow` and more |
| From, To | The ends. `~` swaps them |
| Curve | Bends a ramp. Above 0 it moves early, and below 0 late |
| S | Eases a ramp's ends. Above 0 makes an S, and below 0 the reverse |
| Run | How many rows each value holds for |
| Where | Which rows change, with `Amount` below it for some |

`r` picks a random fill's values anew. `Run` makes stairs: a ramp from `00` to `30` over 8 rows with `Run 2` gives
`00 00 10 10 20 20 30 30`, and a random fill holds each pick for that many rows.
It counts the rows it changes, so on Velocity it's that many notes.

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
the column's range, `10` on Velocity, and at 2 on notes and nudges: `=` and `-`
move it by one, `+` and `_` by `10`, or 4 on a nudge, and `i` types it. With `Wrap`, a walk past
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

`List` goes round a short list of values instead, for accents two values can't
make, such as `70 40 55` over a hat. Its `Values` row takes the place of From and
To: `i`, then up to eight hex values, where two digits make a value and the space
comes by itself, so `704055` types `70 40 55`. It starts as the two ends. `Run`
works as for `Alternate`, so `2` gives `70 70 40 40 55 55`, and `~` turns the
list round.

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

`Euclid` spreads its hits over the rows it could change, as the Euclidean
rhythms do: 3 in 8 is `x..x..x.` and 5 in 8 is `x.xx.xx.`. On Velocity it
counts among the notes however they're spaced, so 5 hits over 16 hats accent 5
of them. `Hits` shows how many rows it spreads them over, as in `5 of 16`.
`Rotate` moves every hit that many of those rows later, going round, so 3 in 8
rotated by 1 is `.x..x..x`.

The ramp's ends land on the first and last rows it changes, and rows keep their
spacing along it. A velocity only ever goes on a note, so for Velocity, Every,
Chance and Euclid choose among the notes. On an FX column, the command is the first one
selected, and a row holding a different command keeps it.

### On the Note column

On the Note column, `S` gives the notes already there new pitches, keeping
their rhythm, velocities and gates, and leaving `OFF` and empty rows alone: keep
this rhythm, give me another line. From and To are notes, typed as `C-3`, `c3`
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

`Snap`, on the Note column only, moves each note to the nearest one in the
scale, the lower of two as near, for a take played without it. It's the only
way notes are forced into the scale: playing never does. A note outside the
scale shows in the panel in the grid's colour for one.

On Nudge, the ends run from `-B` to `+B`, in 24ths of a step, so `Random`
between `-3` and `+3` is a humanise you can see, and a `Ramp` rushes or drags
into a fill. On a gate's length, a `Wave` or `List` of lengths shapes the
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
5 on a nudge. They rework the rows `Where` chooses, as the other transforms do.

| Fill | Row | Does |
|---|---|---|
| Invert bits | Flip | Flips the bits Flip has: `FF` flips them all |
| Mask bits | Keep | Keeps only the bits Keep has |
| Rotate bits | Turn | Turns them round, the highest back as the lowest |

On Velocity, Flip `FF` makes `35` into `4A`, Keep `F0` steps the velocities
in `10`s, and Turn 1 doubles each, the loudest coming round to quiet. Flip
and Keep are typed as two hex digits with `i`, and `=` and `-` move them
by one, `+` and `_` by `10`; Turn goes from 0 to 7. Flipping the same bits
twice gives the values back. A value pushed past the column's highest stops
there, as a nudge's 5 bits go to 31 but a nudge only to `+B`.

On the Note column the number is the note's degree, counted up the scale from
the lowest note while the scale is on, so every note that comes out is in the
scale; with it off, it's semitones. In C major, C-3 is degree 28, `11100`, so
Flip `01` makes it D-3 and Flip `03` F-3. With the scale off, C-3 is `30`, and
Flip `0C` takes it up an octave. Small masks, such as `01` to `07`, keep a line
near where it was.

### On an empty column

On a Note column with no notes in the rows selected, `S` writes notes of its
own, and the panel's title says `Generate`. `Where` gives the rhythm, from
`All rows`, `Every`, `Chance` and `Euclid`, and the fill the pitches: a `Ramp`
from the scale's root an octave up, in the octave notes are typed in, or a
`Random`, `Walk`, `1/f`, `Wave`, `Alternate` or `List`, in the scale's degrees
while it's on.
Each note takes the velocity notes are typed with and the track's gate, and an
`OFF` there stays. `Euclid` with 3 hits over 8 rows is a tresillo to start a
line from.

On an FX column with no command in the rows selected, `S` first lists the
commands, as `K` does, and the one you choose is written on the rows `Where`
picks, every row to begin with, from its lowest value to its highest. `Esc` in
the list leaves the column as it was. A transform has nothing to rework on an
empty column, so it isn't offered. Velocity, Nudge and length still need notes.

### Acid lines

`Acid`, on the Note column only, writes a 303 line: notes, accents and slides
together, as Roland's and Arturia's 303s roll them. It works on any machine,
not only the `Acid`. On an empty column it writes a riff of its own; on notes
already there it keeps their rhythm.

| Row | Does |
|---|---|
| From, To | The riff's notes come from between them, and From is its root |
| Notes | How many notes the riff has, from 1 to 8, its root among them |
| Octave | The chance a note goes an octave up or down |
| Density | On an empty column, the share of rows that play |
| Accent | The chance a note is accented, written as `7F` |
| Slide | The chance a note slides into the next, written as `HLD` |
| Pitches | `Roll` gives new pitches on each `r`, `Keep` keeps them |

The riff's notes come from the scale while it's on, so a set of four in C minor
might be `C-3 D#3 G-3 A#3`, and the root plays more often than the others.
Density counts where each row falls in the bar, counting from the first row
selected: beats play most, then every third row, then the other even rows, and
the rest least, so at `50%` half the rows play, most of them on the beat. A
plain note takes the velocity notes are typed with, and an accent `7F`, as `'`
writes it; a slide is `HLD` on the gate, as `/` writes it.

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

On an FX column with a lane, a live column fills the lane's steps, whichever
rows you selected, and follows the lane when `:lane`, `gs`, `ge` or `]l` move
it; `:lane` alone applies it. Across tracks, each track gets a live column of
its own, which `gl` opens and `Space g a` applies on its own. Doubling a phrase
applies its live columns, and so does `]r` on the rows it rotates. A new live
shape over one on the same column applies the old one first, and starts from
the values it left.

## Values and small edits

| Keys | Does |
|---|---|
| `x` | Clear this field |
| `r` | Replace this field, as in Vim: no move, and back to Normal |
| `s` | Clear this field and start Insert there, as in Vim |
| `=` `-` | Up or down a small step. `C-a` and `C-x` do the same |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `D-l` `D-h` | Cmd and `l` `h`: as `=` `-`, without moving, here and wherever they work |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_`, without moving, here and wherever they work |
| `K` | On an FX column, choose from a list |
| `.` | Repeat the last change |
| `p` | Paste over, from the cursor down. Past the end it stores more |
| `3p` | Paste 3 times, end to end, as one undo |
| `gp` | Paste over, and land just after it, ready for the next |
| `gm` | Merge: empty cells in the copy leave what is there alone |
| `"a` | Use register `a`, before `y`, `d` or `p`. `a` to `z` all work |
| `"0` | The last thing you copied with `y`, which a cut can't replace |
| `u` `C-r` | Undo and redo |

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
| Fingers up or right | As `=`, in the setting's fine step |
| Fingers down or left | As `-`, in the setting's fine step |
| Shift and any way | As `+` `_` |

Up and down and sideways are the same knob, as a trackpad's fingers drift from
one to the other. The value follows the fingers, as a level bar fills, whether
or not macOS scrolls naturally; the audio page's `Direction` row turns it the
other way.

On a value with fine digits a tick is a 16th of `=`'s step, a step of the first
fine digit, as if the cursor were on them; with the cursor on the fine digits
themselves, a tick is their own `=`. On the tempo a tick is a tenth of a BPM,
or a hundredth on its fine part, and on the sample page a 16th of a pixel, as
Cmd-l and Cmd-h nudge a point. A slow or steady turn moves the same for every
tick, so twice the swipe is twice the change; only a fast swipe takes more at a
time, up to one of `=`'s steps, to sweep across. Everywhere else a tick is one
of `=`'s steps: a semitone on a note, the next choice in a list. On a selection
it moves every value, as `=` does, and keeps it selected.

One turn is one undo: `u` takes back everything from the first tick until the
wheel stops for a moment, or a key is pressed. `.` still repeats the last
change made with keys.

`K` on an FX column lists the commands by group, searching as it opens. If the
slot is empty, type the value next. If it has one, it stays.

`r` then `k` changes a note to high C. On other columns it takes a whole value:
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
| `converge` | 1 4 2 3, from both ends in |
| `diverge` | 2 3 1 4, from the middle out |
| `pedal` | 1 2 1 3, 1 4 1 2, the first between the others |
| `shuffle` | each row once a pass, dealt again each pass |
| `random` | any row each step, so rows repeat |

The up-and-down orders and `pedal` take longer than a pass to come round, so
they drift against other tracks as a polymeter does. The walk starts again
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

## Lanes

An FX column can loop on its own, apart from the notes: a lane. It keeps
counting across the phrase's repeats, so a lane of 3 against a loop of 16 lands
somewhere new each pass, as tracks of different lengths do. A column follows
the notes until you give it a lane, so changing the phrase's loop changes it too.

| Keys | Does, on an FX column |
|---|---|
| `:lane 3` | Make it a lane of 3 steps, from the cursor's step |
| `:lane` | Follow the notes again |
| `gs`, `ge`, `:len 3` | On a lane, set its start, end or length |

On a column without a lane, and on the note and velocity columns, `gs`, `ge`
and `:len` set the phrase's loop. On a lane, `gg` and `G` go to its first and
last steps, and `5G` to its fifth. A lane shows its length in the title, like
`FX1:3`, dims the steps outside it, and has its own `>` while playing. Sliding
the loop with `]l` moves lanes with it. Editing works on rows as usual,
whatever loop a column plays. Anything that starts a track's chain over starts
its lanes over too, so playing from a step hears what playing to it would. A
glide in a lane goes to the next value in that lane.

`gg`, `G`, `{`, `}`, `ib`, `ic` and `ip` go by the loop. `5G` means row `04`.
`j` and `k` carry on past its edges. `Space c` on a dim row plays from `00`, and
`;` there plays just that row's note.

