
# Generators in depth

The shape panel's fills with the most settings, one by one: what each is for, what
its settings do together, and a few recipes to start from. **Shaping a column** in
the editing guide says how the panel opens and how its rows are set; this guide
says what to set them to. `Space d` in the panel or on Gen says in a line
what the setting under the cursor does, and `Space ?` in the panel opens this guide at the
fill's section; `Esc` goes back to the panel as it was.

The recipes give a value for each field and leave the rest as the panel starts
them. Those on the Note column are in C minor with the scale on. Each ends with
what to listen for. Keep a recipe live with `L` to turn its settings while it
plays, and `r` rolls a fill that draws from its seed again.

## List

`List` goes round the values typed into its `Values`, up to 256, so a line can be
as long as the phrase, a value a row, and a longer phrase goes round it again. On
the Note column they're notes, `c3 d#3 g3`, written as they are, in the scale or
not; elsewhere two hex digits each. `Run` gives each value that many rows.

**A sixteen-step accent line.** On a hat's Velocity over a 16-row loop: `Fill`
`List`, `Values` `7F 30 50 30 70 30 50 40 7F 30 50 30 70 40 60 50`, `Run` `1`.
Listen for an accent on every beat and a lift into the next bar, which two values
in turn can't give.

**A bass line typed as notes.** On an empty Note column over 32 rows: `Fill`
`List`, `Values` `c2 c2 d#2 c2 g2 c2 a#1 c2`, `Run` `4`. Listen for the
eight notes, each on four rows in a row, a beat, then the line again from the first.

## Mini

`Mini` is a line in TidalCycles' mini-notation whose hits are the column's own
values, so its `Text` is its rhythm and its values at once: notes by name or by
degree from `1` on the Note column, hex on Velocity and Length, signed nudges, and
an FX column's values. Each top-level token is `Step` rows. See **Shaping a
column** in the editing guide for the whole notation.

**A bass line in degrees.** On an empty Note column over 16 rows: `Fill` `Mini`,
`Text` `1 . 1 [. 5] 3 . <8 7> .`, `Step` `2 rows`, `From` `C-2`. Listen for the
root, the fifth pushed off the beat, and on the last beat the octave one bar and
the seventh the next.

**A triplet fill.** On the same column, `Text` `1 . 3 [5 3 1]`, `Step` `4 rows`.
The last beat is three notes, the middle two nudged off the rows, a true triplet.

**Accents with a chance.** On a hat's Velocity over 16 rows: `Fill` `Mini`, `Text`
`7F 30 [50 30] 30?40`, `Step` `4 rows`. An accent on the first beat, a pair of
eighths on the third, and a soft last beat that plays four times in ten, the same
each pass while `Vary` is `Once`.

**A tied length.** On a lead's Length, a note on every row: `Text` `40 _ 80 20`,
`Step` `1 row`. The second row ties into the third, `HLD`, one note held across
them, and the line goes round every four rows.

**A bass and its answer.** Two tracks' empty Note columns selected together:
`Text` `C-2*4, [. G-2]*2`, `Step` `4 rows`. The first track plays `C-2` on every
row, the second `G-2` on the offbeats: a layer a track. On tracks in one group,
the same selection makes a kit line instead, as below, and the line takes names.

**A kit line.** Three drum tracks named `BD`, `SN` and `HH`, all with `Group` `G1`,
their empty Note columns over 16 rows selected together: `Fill` `Mini`, `Text`
`bd . [. bd] . sn . bd ., hh*2 [. hh]`, `Step` `2 rows`. One line on all three,
each playing where its name is: the kick on the first and last beats and a
sixteenth into the second, the snare on the third, and the hat, its own layer, two
sixteenths and the last of every beat. Type the line again on any of the three
tracks' Gen and all three change; `sn?` in place of `sn` drops the snare
half the time, the same on every track.

## Random

Random values between two ends, after Mutable Instruments' Marbles: anything
from one value every time, through values gathered round a point, to the two
ends alone. On the Note column it also weighs the notes, after Vermona's
meloDICER, so a line lands on the root and the fifth more than on the rest.

| Row | Does |
|---|---|
| From, To | The range. Which is which doesn't matter |
| Spread | How far the values stray from the bias point |
| Bias | The bias point: where they gather, or which end wins |
| Steps | On notes: how many degrees may play, the most important first |
| Weights | On notes: how often each degree comes, a hex digit each |
| Curve, S | Show `--`: a random fill isn't bent |
| Run | How many of the rows it changes take each value |
| Where | Which rows it changes, and on an empty Note column the rhythm |
| Vary | Once, or a new roll each pass or every few |
| Mutate | While it varies: the share of rows each new roll draws again |

**How a value is chosen.** Each row has a draw of its own, fixed by the seed and
the row's place. Spread and Bias turn that draw into a place in the range:

| Spread | Values |
|---|---|
| `0%` | The bias point, every row |
| Below `50%` | A bell round the bias point, wider as it goes up |
| `50%` | Any value as likely, Bias leaning them to one half |
| Above `50%` | A bowl: more at the ends than in the middle |
| `100%` | The two ends alone |

Bias puts the bias point between the ends: `0` the middle, `-50` a quarter of the
way up, `+100` the top. With a narrow Spread it's where the values gather; with a
wide one, it's how often they're in the top half, so at `100%` with `+40` the high
end comes seven rows in ten.

On the Note column, each note in the range then gets a chance: its share of where
Spread and Bias put the values, times its degree's weight. A note needs both to
come often, a place near the bias point and a heavy weight. `Steps` gives every
degree past it a weight of 0, so in C minor `Steps` 2 plays only C and G, as
`Weights` `F000F00` would, and turning Steps up again gives the others back their
weights. With the scale off, `Weights` has a digit for each of the twelve
semitones. In the panel `Weights` is a field of tokens, a digit over each note:
`l` opens its buffer, `=` and `-` turn the digit under the cursor, `~` rests it at `0`
and gives it back, `K` lists the sixteen weights with how often each comes, for
`Enter` to put one in, and a digit that isn't one is marked `✗` while the line keeps
playing the last weights that read (the editing guide's Fields of tokens).

**Over time.** `Run` holds a value for that many of the rows it changes: `Run 4`
on every row is a new value a beat on a track at `1/16`. `Where` chooses the rows, so `Euclid` or
`Chance` gives the line its rhythm on an empty column. `Vary` `Each pass` draws
every row again each time the phrase comes round, and `Every 4 passes` keeps each
draw for four. `Mutate` below `100%` draws only that share of the rows again each
time, keeping the rest as they were heard, so the line drifts, and `0%` holds the
first one heard, Marbles' Déjà vu. `Space g a` on Gen keeps the one you hear.

**What's going on.** As each row keeps its own draw, turning Spread or Bias moves
the line rather than rolling a new one: the rows keep their order, so the row that
drew highest still writes the highest value. A macro on Spread opens a line out
from one note to the whole range, and back. Kept live, each of `Weights`' digits is
a setting of its own as well, a cell on Gen's `Weights` device, so a macro,
a `MAC` lock or an LFO on a macro moves one degree's weight while the line plays: the
meloDICER's fader for that note, turned for you.

**A fifth that comes and goes.** Note column, live, From `C-2`, To `C-3`, Steps all
7, Weights `F000000`. On Gen, `Weights`' `G` on a macro, `Min` `0`, `Max`
`F`, and an LFO on the macro. Listen for a bass on the two Cs alone that brings in
G as the LFO rises, a third of the time at the top, and back to C alone as it falls.

**A bass near the root.** Note column, From `C-2`, To `C-3`, Spread `30%`, Bias
`-60`, Weights `F4F4F4F`. Listen for a line that stays low: C about a third of the
time, D# nearly as often and D, the next note up, a quarter, and F or higher now
and then. Weights `F0F4F04` leaves out the D: C and D# nearly nine times in ten.

**Root and fifth.** From `C-2`, To `C-3`, Steps `2`. Spread `50%` plays C-2, G-2
and C-3 a third of the time each. Bias `-50` makes it C-2 seven times in ten, G-2
two and C-3 one: a pumping bass that lifts now and then.

**An arpeggio in any order.** From `C-3`, To `C-5`, Steps `3`, Spread `50%`. C, D#
and G in both octaves and the top C, each one time in seven. `Run 2` plays each
note twice in a row.

**A tune round the middle.** From `C-3`, To `C-4`, Spread `20%`. F and G four
times in ten each, the notes either side less. Put Spread on a macro and turn it
up for the chorus: the same rows reach further out.

**Leaps.** From `C-3`, To `C-5`, Spread `85%`. The two Cs about a quarter of the
time each, the notes next to them less, the middle rarely: a line that calls low
and answers high.

**Humanised velocity.** Velocity column, From `40`, To `7F`, Spread `20%`. Eight
notes in ten between `57` and `68`, the odd one further out.

**A filter that moves each bar.** An empty FX column, `CUT`, Where `All rows`, From
`20`, To `C0`, Run `4`, Vary `Each pass`. A new cutoff every four rows, and new
ones each time round.

## Acid

A 303 line: notes, accents and slides rolled together, as Roland's and Arturia's
303s roll them. **Acid lines** in the editing guide lists its settings; here is
how they make a line. It starts with Notes `4`, Octave `20%`, Accent `30%` and
Slide `20%`, and on an empty column Where `Beats` at Density `50%`.

**The riff's notes.** From is the root. The riff is the root and `Notes` less one
others, taken from between From and To, in the scale while it's on; Spread and
Bias choose those as they lean a `Random`, and as they start any note between the
ends is as likely. Each row plays one of the riff at random, the root counted
twice, so with `Notes` 4 the root comes two rows in five and the others one each.
`Notes` 1 is the root alone.

**The rhythm.** It plays on every row its `Where` selects. On an empty column
that's `Beats`: each row has a chance to play by where it falls in the bar,
counting from the first row selected, the beats most, then every third row,
then the other even rows, and the rest least. Its `Density` is how many play on
average: at `50%` a beat plays about seven times in ten and an odd row between
them about three. On notes already there it starts on `Notes`, so the rhythm is
theirs; `Euclid`, `Rhythm` or any other `Where` plays its own rows instead,
notes of its own where there were none, and leaves the notes outside it alone.

**The rest.** Each note may go an octave up or down, as likely either way, by
`Octave`; be accented, `7F`, by `Accent`; and slide into the next, a `HLD` gate,
by `Slide`. These are a second die: with `Pitches` on `Keep`, `r` rolls only
them, and the notes and rhythm stay.

**Loops.** Each part, the rhythm, the notes, the octaves, the accents and the
slides, can go round a loop of its own, from 1 to 64 rows. Loops of different
lengths drift against each other, and the whole comes round again only after
their least common multiple, which Gen's `Loops` title gives.

**One note that talks.** Notes `1`, Octave `40%`, Accent `40%`, Slide `30%`.
The root alone, moved by octaves, accents and slides: listen to the filter and
the accent do the work.

**A riff of three.** From `C-2`, To `C-3`, Notes `3`. The root and two others,
the root half the time. `r` until the two others sound right, then Pitches
`Keep` and `r` for accents and slides that fit them.

**Sparse and heavy.** Where `Beats`, Density `30%`, Accent `50%`, Slide `10%`.
Mostly on the beats, half of them accented.

**Busy and legato.** Where `Beats`, Density `85%`, Slide `35%`. Nearly every
row, a third of them sliding: runs of tied notes that glide.

**A tresillo over your notes.** On notes you wrote, Where `Euclid`, Hits `3`.
The tresillo's rows get the riff, a note of its own where you had none, and
your notes off them stay as they are. Over `Notes` instead, the three hits fall
among your notes.

**A rhythm of your own.** On an empty column, Where `Rhythm`, Rhythm `Own`,
Text `x . [. x] <x [x x]>`, Step `4 rows`. A beat a token: the riff on beat 1,
the third sixteenth of beat 3, and on beat 4 a hit one bar and two the next.

**Your line, new accents.** On notes you wrote, Fill `Acid`, Pitches `Keep`. Your
notes and rhythm stay; `r` deals accents, slides and octaves until they fall
right.

**Drifting loops.** On an empty column, Trig loop `16`, Note loop `7`, Acc loop
`5`, Sld loop `3`, kept live. The rhythm repeats each bar while the notes go
round seven rows and the accents five, so every bar is a little different, and
the whole repeats only after 1680 rows.

## Grids

A drum beat from Mutable Instruments' Grids, its map of 25 drum patterns. Shape a
kick's, a snare's and a hat's tracks at once, `vGWW` and `S`, and each plays its
part of the same place on the map. **Drums from Grids' map** in the editing guide
lists the settings.

**How it chooses.** `X` and `Y` are a place on the map, and the four patterns
round it are blended. Each row then has a level, how much it belongs to the
pattern. A part's fill is how far down the levels it plays: a row plays once its
level is above `FF` less the fill, so turning a fill up adds the next most
important hit and never takes one away. A row whose level is over `C0` is
accented, `7F`.

**Chaos** raises each row's level by a random amount, up to a quarter of Chaos:
at `80`, up to about `20`. Hits just under the fill come and go, and a few strong ones
cross into accents. It's what `r` and `Vary` roll.

The patterns below are what each place plays over a bar of 16 rows with Chaos
`00`: `X` an accent, `x` a plain hit and `.` none.

**The middle of the map.** X `80`, Y `80`, where it starts, each fill `40`. Kick
on 1 and 3, the snare on 2, and on 4 with the sixteenth after it, hats between:

```
BD  X.......X.......
SD  ....X.......XX..
HH  ..X...X...X.....
```

**Four on the floor.** X `80`, Y `00`, BD fill `40`, SD fill `40`:

```
BD  X...X...X...X...
SD  ....X...........
```

BD fill `80` adds a run of plain kicks at the end of the bar, `X...X...X...Xxxx`.

**A backbeat with offbeat hats.** X `00`, Y `80`, each fill `40`:

```
BD  X.....X.........
SD  ....X.......X...
HH  ..X...X...X...X.
```

**Broken.** X `00`, Y `FF`, each fill `40`. A kick on the eighth after 3, and
a snare just before it:

```
BD  X.......X.X.....
SD  ....X..X....X...
HH  ..X...X...X...X.
```

**Busy.** X `FF`, Y `FF`, each fill `80`:

```
BD  X...X..Xx...X...
SD  X...X..X.x..x...
HH  ..X...X..x.x..X.
```

**Ghosts that change.** Any place, HH fill `80`, Chaos `80`, Vary `Each pass`.
The strong hats stay, and the weak ones are new each bar.

## Divisions

A rhythm from a chance for each note division, after Vermona's randomRHYTHM:
quarters, eighths, sixteenths and their triplets. **A chance for each division**
in the editing guide lists the settings and how triplets fall between the rows.

**How a row plays.** Every place a division can strike in the bar has a die of
its own, the same each bar. A row plays if any division landing on it strikes, so
where divisions meet their chances add up. A beat is a quarter, an eighth and a
sixteenth at once: with `1/4` at `75%`, `1/8` at `25%` and `1/16` at `10%`, as
channel A starts, a beat plays about 83 times in 100, an eighth between the beats
about one in three, and the other sixteenths one in ten. The triplets land on
beats too. Raising a chance only adds strikes, as each die stays where it fell.

**A steady kick with pushes.** `1/4` `100%`, `1/16` `15%`, the rest `0%`. Every
beat, and a sixteenth now and then.

**Triplet hats.** On a track at `1/32`, Bar `4/4 32nds`, `1/8T` `70%`, `1/8`
`30%`. Hats on the eighth triplets, nudged off the rows, and a few straight
eighths against them.

**Three against four.** Three tracks at once, `vGWW` and `S`. Channel A: `1/4`
`100%`, the rest `0%`. Channel B: `B 1/4T` `100%`, the rest `0%`. The first
track plays the beats, the second six quarter triplets a bar, and the third,
`A+B`, both.

**Realtime.** Every chance `20%`, Vary `Each pass`. A sparse part that's new each
bar, as the module's `Realtime` rolls it.

## Ring

A noise ring after the Wiard Noise Ring: one shift register gives the rhythm and
the melody together. **A noise ring** in the editing guide lists the settings.
It starts with Change `10%`, Chance `50%` and Length `8`.

**How it plays.** Each row a bit goes into the register. It's the bit that went in
`Length` rows before, unless `Change` replaces it with a new one, a 1 by
`Chance`. A row plays if its bit is a 1, at a pitch from the last eight bits read
as a number: the newest is the smallest part of it and the one seven rows back
the largest.

**What's going on.** So the rhythm comes back as the melody. A row plays in the
top half of From to To if the row seven before it played, and within that half
higher if the row six before played, and so on. More 1s means larger numbers:
a high Chance gives a line that's busy and high, a low one a line that's sparse
and low. From above To turns that round.

**A locked figure.** Change `0%`, Length `8`. The same eight rows of notes and
rests twice a bar. `r` for another.

**A slow drift.** Change `10%`, as it starts, kept live. The figure mostly
repeats, a bit changing now and then. When one sounds right, `Space g a` on
Gen and Change `0%` hold it.

**Against the bar.** Change `0%`, Length `5`, on a 16-row phrase. A figure of
five rows that never lines up with the bar.

**High and busy, low and sparse.** From `C-2`, To `C-4`, Chance `80%`: most rows
play, mostly in the upper octave. Chance `25%`: a few notes, mostly low.

## Tonnetz

Chord progressions that walk the Tonnetz, after Ornament & Crime's Harrington 1200
and Automatonnetz. **Chords on the Tonnetz** in the editing guide lists the moves
and settings. Each move changes one note of the chord by a semitone or a tone, so
any walk voice-leads smoothly; which moves, in what order, decides where it goes.

**Two moves in turn.** A `List` of two moves, from a major chord on C:

| List | Chords | Goes |
|---|---|---|
| `rl` | `C Am F Dm A# Gm D# Cm` | Down by fifths, each with its relative |
| `lr` | `C Em G Bm D F#m A C#m` | Up by fifths, the same way |
| `pl` | `C Cm G# G#m E Em` | Six chords, then round again |
| `pr` | `C Cm D# D#m F# F#m A Am` | Up by minor thirds |

`Where` is the rhythm: `Every`, Amount `4`, over 16 rows plays the first four,
so `rl` is C, Am, F and Dm, a chord a beat.

**Pads in three voices.** The `rl` list on three tracks at once, `vGWW` and `S`.
Each plays one voice, `Low`, `Mid` and `High`, and only the track whose note the
move changes moves, a step at a time.

**A loop in key.** Moves `Loop`, Length `4`, P `0`, L `3`, R `3`, Scale `In
scale`. Four moves drawn once and gone round, among the key's six major and minor
chords. `r` draws another loop.

**A darker drift.** Moves `Random`, P `6`, L `2`, R `2`, Scale `Free`, Vary
`Every 4 passes`. Mostly major and minor on the same root, now and then moving
away.

**The roots alone.** Voice `Root`, on a bass track beside the chords, with the
same From and List: the chords' roots, each within a tritone of From.
