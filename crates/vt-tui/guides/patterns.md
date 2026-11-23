# Patterns

A pattern is a line of text that says what plays and where: `C-3 . [. G-2]
<D#3 A#2>`. It's TidalCycles' mini-notation, read by steps rather than cycles,
and it's the same notation everywhere vimtracker takes one: a column entered
with `!`, the fill `Mini`, a rhythm of your own under any fill, and a kit line
across a group's drums. This guide is the notation itself. How each place opens
its line is in the guide for that place.

## Where you type one

| Where | What it writes |
|---|---|
| `!` on a phrase column | The column, from the line, then plain rows |
| `S`, `Fill` `Mini` | The column's values, kept live or written |
| `Rhythm` `Own`, any fill | Where the fill plays: `x` and `.` only |
| Gen, a `Mini`'s `Text` | The live line, as the track plays it |
| `S` or `!` across a group | A kit line: the hits are the tracks' names |

A line is a field of tokens, so the same keys edit it everywhere: `h` and `l`
go token to token, `=` and `-` turn the one under the cursor, `~` rests it, and
`K` lists what can go where the cursor is, each with what it does (**Fields of
tokens**, in the editing guide).

## Hits and rests

Each word of the line is a token, and each top-level token is `Step` rows, from
`1 row` to `16 rows`. At `Step 4 rows`, a token a beat, `C-3 . D-3 .` is `C-3`
on row 00, nothing on row 04, `D-3` on row 08 and nothing on row 12. A token
more makes the line longer, as a row more does, and the line goes round to fill
the rows it covers: four tokens at `Step 4 rows` over 32 rows play twice. A span
shorter than the line cuts it.

| Token | Means |
|---|---|
| `x` | A hit, in a rhythm: where the fill plays |
| `C-3` `3` `40` | A hit that's a value: a note, a degree, hex |
| `.` | A rest. `~` is a rest too |
| `_` | On Length, a tie into the next row's note, `HLD` |

## Changing a token

A mark straight after a token changes it, and most take a number. `K` after a
token lists the marks it takes, each with what it does.

| Token | Means |
|---|---|
| `x*3` | Three hits in its place, up to `*16` |
| `x!3` | Three places of `x`, up to `!64`; `x!` is two |
| `x@3` | One hit three places long, up to `@64` |
| `x?` | A hit half the time |
| `x?30` | A hit 30 times in 100 |
| `x(3,8)` | Euclid: 3 hits spread over 8 parts of its place |
| `x(3,8,2)` | The same, 2 parts later |

`!` and `@` both make a line longer, in different ways: `C-3!4` is four notes,
a token each, and `C-3@4` is one note that takes four tokens' rows. At `Step 4
rows`, `x . [. x] x(3,8)` is a hit on row 0, one on row 10, and 3 of 8 in the
last beat, on rows 12, 13 and 15. A Euclid is one token, so to spread it over rows
give it places: `x(3,8)@8` at `Step 1 row` is the tresillo over eight rows.

`*` on the Note column in a single row is a `RET`, as many strikes in the row,
in its first empty FX slot; given rows enough, as `x*2` at `Step 2 rows`, it's
split over the rows instead.

## Groups and turns

| Token | Means |
|---|---|
| `[a b c]` | A group: its tokens share one place |
| `<a b c>` | One of them each lap, in turn |
| `a, b` | A stack: one line over another |

A group splits its place: at `Step 4 rows`, `[C-3 D-3]` is `C-3` on the beat
and `D-3` two rows later, and `[. x]` is a hit on the offbeat. Groups nest, and
a mark changes a whole group, `[C-3 D-3]*2` or `[x .]?`.

Inside a token's place, its hits land on the row nearest them. On the Note
column a hit that falls between rows is nudged off the nearest instead, so
`[C-3 D-3 E-3]` at `Step 4 rows` is a true triplet: rows 0, 1 and 3, the
second nudged `+8` and the third `-8`. Two hits on one row keep the first, and
the status line says how many were dropped and which `Step` would keep them
all: `2 hits share a row and dropped; Step 3 rows keeps them`.

`[a|b|c]` picks one of its choices at random each time it comes round, as Tidal's
`|` does, so `7F [40|60|20]` on Velocity gives the second beat one of three
velocities. A choice is everything between two `|`s, so `[C-3 C-3|C-3 C-3 C-3]`
is two notes or three, never a mix. It draws from the fill's seed, as a `?` on the
other columns does: the same picks each pass until `Vary` draws again. A `|`
goes only inside `[ ]`, and every choice needs something in it.

`< >` takes the next of its choices each time the line comes round, a lap, and
counts on from pass to pass, so `<C-2 F-2 G-2>` on a line that's one lap a
phrase changes the root each pass, round a three-pass cycle. The phrase holds
the first pass, and the engine plays the rest. Turns nest too: a `< >` inside
a group or inside another `< >` turns when it comes round.

## Values on each column

A hit is a value in the column's own units:

| Column | A hit is | Like |
|---|---|---|
| Note | A note, or a degree from `1` | `C-3 [. 3] <5 6>*2 8?` |
| Velocity | Hex | `7F 40 [60 40] 40?` |
| Nudge | A nudge with its sign | `0 +8 0 -4` |
| Length | Hex, or `_` for `HLD` | `40 _ 80 20` |
| FX | Its value, or its name | `30 [60 90] A0 <C0 40>` |

**Notes and degrees.** A note is `C-3`, `c3` or `C#3`. A degree counts from
`1`, the root, up the song's scale, or by semitones with the scale off, and goes
on past the octave, so in a seven-note scale `8` is the root an octave up, up
to `64`. Notes and degrees mix in one line. A degree's octave is the fill's
`From` in `Mini`, and with `!` it's the octave of the last note typed in Insert.
With `!`, `x` is that last note itself, so `x . x C-3` after typing `E-3` writes
`E-3 . E-3 C-3`, and the line keeps the note by name.

**FX columns.** The command is the column's, chosen with `K` before the line is
typed, as on any empty FX column, so the line holds only its values: hex, or a
name where `K` names them, a `CND`'s condition or an `ACU`'s amount.

## Stacks and layers

`,` stacks lines: `C-2*4, [. G-2]*2` is two lines at once. One track's column
plays one line, so a stack is refused on one track, saying so. Across tracks,
each takes the next layer: on two tracks selected with `W`, or `S` across them,
that line at `Step 4 rows` is a bass on every beat on the first and its answer
on the offbeats on the second.

## Kit lines

Across two or more tracks of one group, on their Note columns, the hits are the
tracks' names, matched without case: `bd . [. bd] . sn . bd ., hh*2 [. hh]`.
Each track holds the whole line and plays where its own name is, at its own note
and the velocity notes are typed with, and rests everywhere else. A stack is
natural here, as each layer is other tracks'. A name in two layers is refused,
as a track plays one, and every track needs a name first, as its number can't be
a token: `:name` them. An edit to the line on any of its tracks is made on all of
them as one change, so a `?` falls the same on every track. How tracks join and
leave a kit line is under **A kit line** in the editing guide.

## Chance

**On the Note column, a chance is the row's own.** `C-3?30` writes the note with
`CHA 4D` on its row, in the row's first free FX slot, rather than deciding there
and then, so the engine rolls it as the row plays, as any `CHA` is. That puts it
in reach of the track's `Chance`, `CDLT`, and its `Thin`: at `Thin Random` the
note plays about 3 times in 10, a new roll each pass, or held by the dice loop;
at `Thin Chance` the row's chance is a threshold, and `CDLT` decides whether it's
in. So `C-3 C-3?30 C-3?70 C-3` at `Step 4 rows` writes four notes, the second at
`CHA 4D` and the third at `CHA B3`, and under `Thin Chance` it's a mask: the 70%
note plays at `80`, the 30% one comes in as `CDLT` rises past `A4`, and the 70%
one goes below `5C` (**Moving every chance** in the FX guide).

It's the same on every Note column: a triplet's hit nudged off its row keeps its
nudge and its `CHA` both, `[C-3 D-3? E-3]` writing `D-3` nudged `+8` at `CHA 80`,
and a kit line's `sn?` is a `CHA` on the snare's row, the one track that plays it.
The one row that can't hold one is a row with all three FX slots taken: its `?`
is drawn as written, as below, and the status line says so, `Row 04 has no FX
slot free for its ?`.

**On the other columns, it's drawn in the pattern,** from the fill's seed, the hit
and its row: on Velocity, Length, Nudge and FX columns there's no note to give a
chance to, so the `?` is the chance its value is written. The same hits play each pass until it draws again:
with `Vary` at `Once` the line keeps its draw, and at `Each pass` it draws anew
every time round. A line written with `Enter` from `!` freezes `< >` and these
draws as their first pass; kept live with `L`, they go on pass to pass.

## Differences from TidalCycles

- **Steps, not cycles.** Tidal fits a whole line into one cycle, so more
  tokens play faster. Here each top-level token is `Step` rows, so more tokens
  make the line longer and the tempo stays put.
- **A dot is a rest.** Tidal uses `.` to mark feet. Here it's a rest, as the
  guides print rhythms, so group with `[ ]`.
- **Euclid's third number moves the hits later**, as `Rotate` does; Tidal's
  moves them earlier.
- **A chance is a percent:** `?30` plays 30 hits in 100, as `Chance` counts.
- **Not here:** `/`, `{ }` and `..` aren't read, and a line has no functions,
  `every` or `fast`: what varies a line from pass to pass is `< >`, `?`, `|`,
  `Vary` and the other generators.

## Recipes

Each is the line, its `Step`, and the column. Type it after `!` on the column,
or in a `Mini`'s `Text`.

**Four on the floor.** Note column of a kick, `Step 4 rows`: `C-3!4`. A kick
on every beat of a bar.

**The son clave.** Note column, `Step 1 row`: `x . . x . . x . . . x . x . . .`.
After `!`, each `x` is the last note typed; as a `Rhythm` `Own`, it's where any
fill plays.

**The tresillo.** Note column, `Step 1 row`: `C-3(3,8)@8`. Three hits spread
over eight rows, on rows 0, 3 and 6.

**A bass in degrees.** Note column, `Step 2 rows`, `From` `C-2`:
`1 . 1 [. 5] 3 . <8 7> .`. Listen for the root, the fifth pushed off the beat,
and on the last beat the octave one bar and the seventh the next.

**A root that moves each pass.** Note column of a 16-row phrase, `Step 16
rows`: `<C-2 F-2 G-2 D#2>`. One long note a pass, a new root each time round.

**A triplet fill.** Note column, `Step 4 rows`: `1 . 3 [5 3 1]`. The last beat
is three notes, the middle two nudged off the rows.

**Velocities from a choice.** Velocity of a hat with a note on every row, `Step 1
row`: `7F [40|60|20] [40|60] [20|40]`. Every beat starts on the accent, and the
rest of it picks among soft velocities, a different mix each time round.

**Accents with a chance.** Velocity of a hat, `Step 4 rows`:
`7F 30 [50 30] 30?40`. An accent on the first beat, a pair of eighths on the
third, and a soft last beat that plays four times in ten.

**A tied line.** Length of a lead with a note on every row, `Step 1 row`:
`40 _ 80 20`. The second row ties into the third, one note held across them.

**A bass and its answer.** Two tracks' Note columns, `W` selecting both, `Step
4 rows`: `C-2*4, [. G-2]*2`.

**A kit groove.** Three drum tracks named `BD`, `SN` and `HH` in group `G1`,
their Note columns selected together, `Step 2 rows`:
`bd . [. bd] . sn . bd ., hh*2 [. hh]`. `sn?` in place of `sn` drops the snare
half the time, the same on every track.
