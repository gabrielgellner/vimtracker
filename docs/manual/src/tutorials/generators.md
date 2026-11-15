# Generative sequencers, recreated

Some of the best-loved sequencers don't play what you wrote: they play something near it, and
let you steer. This chapter builds twenty-two of them out of what vimtracker already has, each as a
short song you can open, play and pull apart: a loop of a bar or two per track, synths only, so
there are no samples to find.

| Song | After | The trick |
|---|---|---|
| `turing-machine` | Music Thing's Turing Machine | A Steps slot's `Mutate` on `Pitch in scale`, played from three macros |
| `deja-vu` | Mutable Instruments' Marbles | `Mutate` for new notes, `:order random` to replay old ones |
| `euclid` | Euclidean rhythms | `S`'s `Euclid`, kept live, with `Rotate` |
| `fugue` | Fugue Machine | One theme and three tracks following it live with `Follow`, each at its own `Speed` |
| `kria` | monome's Kria | Triggers, pitches, octaves and lengths on loops of different lengths, and an `Acid` fill's own loops |
| `metropolix` | Intellijel's Metropolix | Pulses and gates per row, `ACU`, `ARP` and a pendulum order |
| `eloquencer` | Winter Modular's Eloquencer | `RND`: a chance on each row of a note, a length, a velocity or a ratchet varying |
| `live-generators` | Elektron's Euclidean mode | Columns kept live with `L`, and what hand edits do to them |
| `acid-lines` | Roland's TB-303 | `S`'s `Acid` fill: notes, accents and slides rolled together |
| `walks` | Max's `drunk` and Voss's 1/f music | `S`'s `Walk` and `1/f` fills, kept live |
| `euclid`, again | Noise Engineering's Numeric Repetitor | `S`'s `Multiply` on the kick's rhythm |
| `walks`, again | Xaoc Devices' Leibniz | `S`'s bit transforms on the tune and the hats |
| `grids` | Mutable Instruments' Grids | `S`'s `Grids` across the kick, snare and hat, one beat from a map |
| `random-rhythm` | Vermona's randomRHYTHM | `S`'s `Divisions`: a chance per division, triplets nudged between the rows |
| `noise-ring` | The Wiard Noise Ring, Malekko's Richter Noise Ring | `S`'s `Ring`: rhythm and pitch from one shift register, running on each pass |
| `marbles` | Mutable Instruments' Marbles' X, Vermona's meloDICER | `Random`'s `Spread`, `Bias`, `Steps` and `Weights` |
| `marbles-t` | Mutable Instruments' Marbles' T | `S`'s `Complementary`, `Markov`, `3 states` and `Clusters` rows |
| `world-rhythms` | Noise Engineering's Zularic Repetitor | `S`'s `Rhythm` rows: a clave, a bell and their drums, a part a track, with children |
| `variations` | aicd's acid n-grams | `S`'s `Variation`: a Markov chain learned from the line itself |
| `rene` | Make Noise's René | The rows as a grid: `:order cartesian` on chords, `:order snake` on a bass |
| `bloom` | Qu-Bit's Bloom | `:bloom`: branches grown from a trunk, each a mutation of the one before, on a path |
| `clock-multiplier` | 4ms' Shuffling Clock Multiplier | `Space o` rotating a group's speeds, and a live `CHA 00` column as `Skip` |
| `cells` | dan derks' *less concepts* | `S`'s `Cells`: Wolfram's rules, a row a cell or a track a cell, a generation a pass |
| `tonnetz` | Ornament & Crime's Harrington 1200 and Automatonnetz | `S`'s `Tonnetz`: chords by P, L and R, on the `Chord` voice and a voice a track |
| `turing-machine`, again | Nonlinear Circuits' Sloth, Ornament & Crime's Low-rents | A `Lorenz` LFO on `Pitch in scale`: a tune that drifts and never goes round |

Each section says what the original does, how the song does it, how to build it yourself from an
empty song, and which knob to turn next. The steps name the keys; **Selecting and editing**,
the track pages' chapters and **FX commands** have the rest.

## Opening the songs

The songs are in vimtracker's repository, in `docs/manual/src/tutorials/songs`, a project folder
each. From a checkout:

```sh
just run docs/manual/src/tutorials/songs/turing-machine
```

or `vimtracker docs/manual/src/tutorials/songs/turing-machine` with an installed build, or `:e`
and the folder from inside the program. Each folder is a whole project: its `song.ron` is all
there is. Without a checkout, download the `song.ron` linked in each section into a folder of its
own, and open that folder.

They're the repository's own copies, so each is read-only, `[RO]` on the status line, and `:w`
refuses to write over it. Save your changes somewhere new with `:w` and a folder, such as
`:w my-turing`: the copy isn't read-only, and `:w` carries on saving there. To hear one without opening it,
`vimtracker render docs/manual/src/tutorials/songs/euclid euclid.wav --rows 128` writes eight
bars of it.

Every song starts with the generative track as track 1, and its drums after it. Press `Tab` to
play.

## Building from an empty song

Start the program with no project, `just run`. Its eight tracks each play a sampler, with a
phrase `00` of 16 rows in chain `00`. Three things come up in every song:

*   **A synth voice.** `Space 5` opens the track page on its Sound page. With the cursor on the machine, in the Machine device's title, `K` lists
    the machines: type part of a name, such as `acid`, and `Enter`. A name that starts with a
    key the list moves with, such as `kick` or `hat`, needs `/` first: `K` `/kick` `Enter`.
*   **A name.** `:name BD` names the track, so the overview says what's what.
*   **A setting on the track page.** `/` and part of its name, then `Enter`, jumps to it, so
    `/speed` `Enter` lands on `Speed`, ready for `=` and `-`, or `i` to type a value.

`W` and `B` go to the next and previous track in any view. In the phrase view, `0` puts the
cursor back on the Note column before you type notes.

## Turing Machine

[Music Thing's Turing Machine](https://www.musicthing.co.uk/Turing-Machine/) is a looping random
sequencer. A shift register of up to 16 bits goes round, and its one big knob is the chance that
a bit is flipped as it comes round: at noon every bit is new, turned to the right less and less
changes, and fully right the loop is locked. A switch sets how many steps the loop has, from 2 to
16, and a `Scale` knob how wide its voltage goes. Run that through a quantiser and it writes
melodies that drift, a little at a time, until you hear one you want to keep and lock it.

**How the song does it.** Track 1, an `Acid`, plays a written rhythm, every note a `C-2`. A
`Steps` slot in `Note` mode, which moves on one step with each note, chooses each note's pitch:
its eight values are the register, and its `Dest` is `Pitch in scale`, so every move lands in C
minor. `Mutate` is the big knob: the chance each value is drawn again as it comes round. Three
macros are the module's panel: `Change` turns `Mutate` from `0%`, locked, to `100%`, every step
new, the big knob's right half the other way round; `Length` turns the slot's `Length` from 2
steps to 16, the switch; and `Range` turns its `Depth` from none to two octaves either way, the
`Scale` knob. A kick and hats keep time under it. [`song.ron`](songs/turing-machine/song.ron)

```text
00  C-2 7F          08  C-2 7F
01  ---             09  ---
02  C-2 64          0A  C-2 64
03  C-2 64          0B  C-2 64
04  ---             0C  ---
05  C-2 64          0D  C-2 64
06  ---             0E  C-2 64
07  C-2 64  HLD1    0F  ---
```

The song has a loop already kept, and `Change` at `20%`, so it wanders away from it and back.

**Build it.**

1.  `:set scale=minor` and `:bpm 118`. The root is C unless you choose another.
2.  Track 1's machine: `Space 5`, `K`, `acid`, `Enter`. Then `:name TM`.
3.  The rhythm: `Space 3`, then `Option-1` twice for a grid of a sixteenth. `i` for Insert,
    `;` for auto-jump and `z` for octave 2, then type
    `a Down a a Down a Down a a Down a a Down a a Down` and `Esc`. Each `a` writes a `C-2` and
    moves on; each `Down` leaves a rest.
4.  The register: `Space 6` for the Mod page. Put the cursor on `Mod 4`'s type, in the
    title of the device at the top right, `LFO` (`l` goes along a title and on into the device
    beside it), and press `K`, `steps`, `Enter`.
5.  Still in its title: `l` to its mode, `Trig`, and `K`, `note`, `Enter`. `l` `l` to its
    `Dest`, `---`, and `K`, `scale`, `Enter` for `Pitch in scale`.
6.  Its `Depth`, the third row on its left, `i` `0C` `Enter`, so a drawn value moves the note
    by up to an octave either way.
7.  Its `Mutate`, beside `Length`: `+` three times, for `30%`.
8.  `Tab`. The line starts on C and drifts. When you like where it's got to, press `Space g a`
    on the slot to keep it: what it drew is written in as its values. Then `x` on `Mutate`
    puts it back to `0%`, and the loop you heard plays every time. That's the Turing Machine's
    lock.

Then the drums: on track 2, `/kick` for a `Kick`, and a `C-1` on each beat (with `Option-2`
twice for a grid of a beat, `i`, `z` to octave 1, `a a a a`); on track 3, `/hat` for a `Hat`
with `C-4`s on the off-beats, rows `02`, `06`, `0A` and `0E`. The finished song also accents
rows `00` and `08` (`'` after the note, in Insert), slides from `07` into `08` (`/`), and sets
`Acid`'s `Cutoff` `58`, `Resonance` `B0`, `Env mod` `A0` and `Decay` `60`, `Send A` `A0` for
some delay, a shorter hat (`Decay` `30`), and the tracks' `Level`s.

**Give it knobs.** A macro is one knob that turns any of the track's settings, each between a
`Min` and a `Max` of its own (see **Macros** in **The Gen page, macros and presets**). Three make the slot a panel,
all on track 1's Mod page, `Space 6`:

1.  `/mutate` `Enter` puts the cursor on `Mod 4`'s `Mutate`. `M` lists the track's eight
    macros, and `1` maps `Mutate` to the first, from its bottom to its top: `0%` to `100%`, all
    `Change` needs. The first setting on a macro turns the macro to where the setting is, so
    nothing jumps. `:macro Change` names it.
2.  `h` to `Length`, `M` `2`, and `:macro Length`. `gm` opens its mapping, `Min` `1` and `Max`
    `16`, on `Min`: `=` makes it `2`, as a loop of one step plays one note. The macro stays
    where it was, so the slot now plays 9 steps. `Esc`.
3.  `j` to `Depth`, `M` `3`, and `:macro Range`. `gm`, then `i` `80` for its `Min` and `l` `i`
    `8C` for its `Max`. A depth's ends are typed on its own scale, where `80` is none and `8C`
    is `+18`, two octaves either way, as the status line says. `Esc`.

Now `Mutate`, `Length` and `Depth` show where their macros put them, each with a faint dot after
its value to say a macro holds it, and won't turn on their own: `=` on one says to turn its macro
instead, or `gm` for its range. The song's macros are what these keys make.

**Play it.** `Space 8` for the Gen page. Its first band is the `Macros`, with `Change`,
`Length` and `Range` along the top row, each with its value in four digits and a bar. `h` and
`l` go between them, `=` and `-` turn one a little, `+` and `_` by `10`, about a step of
`Length`, `i` types two hex digits, and `x` takes one to `00`. To put it as the song has it,
`Length` `i` `6D` for 8 steps and `Range` `i` `7F` for `+0C`.

*   `Change` up, `i` `40`, and the line drifts. When it's somewhere you like, `Space 6` and
    `Space g a` on the slot keeps it, then `Space 8` and `x` on `Change` locks it there.
*   `Length` up to 16 steps for a line longer than the bar. A step past the eighth has no value
    of its own yet, so it plays C until `Change` draws one.
*   `Range` to `00` and every note is the written `C-2`, the rhythm alone; turned up, the line
    opens out to two octaves.

On the Roto, a track's first device is `Macros`, so its first three knobs are `Change`,
`Length` and `Range`, named, the module's panel under your hands.

There's no `Rate` knob. In `Note` mode the slot moves on with each note, so the written rhythm is
its clock, as a Turing Machine's clock input is, and its `Rate` shows `--`. In `Free` or `Trig`
it has a `Rate` to map, for a register that steps in time rather than with the notes.

**Try next.**

*   **Let it drift by itself:** a slow LFO on `Change`. On the Mod page, `/mutate` `Enter`, `h`
    `j` to `Depth`, then `}` down to `Mod 5`'s `Depth`, `i` `40` `Enter`. `k` to its `Rate`, `K`
    `8 bars` `Enter`. `k` `k` to its type in the title and `l` `l` `l` to its `Dest`, then `K`
    `change` `Enter`, for `Macro 1 Change`. `Space 8`, and on `Change`, `i` `20`. `Tab`: `Change`
    swings from below `00` to `60` and back every 8 bars, the band showing where it's heard with
    a mark where you set it. At `00` the slot plays its values as written, so for two bars in
    eight the line is the loop you kept; then it wanders off a new way, and comes home again.
*   `Length` 3 or 7 against the ten notes in the bar, and the loop no longer lines up with it,
    so a short register makes a long melody.
*   Another scale: `:scale` and choose one. The register stays; every note moves into the new
    scale.
*   To have the line as notes you can edit, `Space 3` and `Space g A`: each note becomes the
    one it was moved to the last time it was heard, and the slot is switched out, knobs and all.

**Save it as a patch.** The Steps slot and the three macros that turn it are a Turing Machine you
can put on any track, keeping that track's sound (see **Patches** in **The Gen page, macros and presets**).

1.  On track 1, `:patch save Turing Machine`. A patch is the track's macros that hold something
    and what they move, so this one is `Mod 4` and the three macros, `1 slot, 3 macros` as the
    status line says, and not the `Acid`, its filter or the other slots. It's kept in the presets
    folder's `patches`, `~/Music/vimtracker/presets/patches` unless `settings.ron` says otherwise.
2.  Track 2, the kick, and `:patch`. The list shows `Turing Machine` with `1 slot, 3 macros`
    beside it; `Enter` adds it. The kick's first two slots shape its note and its cutoff, so the
    Steps slot goes into `Mod 3`, the first going nowhere, and the macros into the first three
    holding nothing, every mapping moved to match: `Change` turns `Mod 3`'s `Mutate` there. The
    status line says where each part went, and `u` takes it all off at once.
3.  `Tab`. The kick's beats now wander in pitch from the same kind of loop, through the kick's
    own sound, and `Space 8` shows `Change`, `Length` and `Range` on its Gen page, to play as on
    track 1.

A track with too few free slots or macros refuses it, saying how many it needs and how many are
free, and `:patch!` puts it over the last ones instead. `K` on a mod slot's type lists `Add a
patch` and `Save as a patch` after the types.

The song's slot and macros are also the shipped patch `Turing Machine`; add it with `:patch`,
where it's listed under `Factory`, on any track, without saving one of your own first.

## Marbles' Déjà vu

[Mutable Instruments' Marbles](https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/)
makes random gates and random voltages. Its Déjà vu knob decides how much it remembers: fully
left, everything is new; turned up toward noon, it replays what it played, until the output is
a locked loop of so many steps; past noon, it plays that loop's values in a random order. Its
T section makes random gates, each with its own chance.

**How the song does it.** Track 1, a `String`, plays an eight-note loop in D dorian, with
`:order random`, so each step plays any of its rows: the loop's own notes, jumping around
inside it, as Déjà vu does past noon. A `Steps` slot with a little `Mutate` on `Pitch in scale`
brings in new notes now and then, the left side of the knob. Under it, track 2 is a drone with
a `Smooth` LFO wandering on its filter, as Marbles' Y output wanders, and track 3 is a hat on
every step, each with a `CHA` of its own, Marbles' T. [`song.ron`](songs/deja-vu/song.ron)

```text
00  D-3 64        04  E-4 64
01  F-3 50        05  A-3 50
02  A-3 64        06  G-3 64
03  C-4 50        07  D-4 50
```

**Build it.**

1.  `:set scale=dorian`, `:set root=d` and `:bpm 100`.
2.  Track 1: `K`, `string`, `Enter` on its machine, and `:name X1`.
3.  `Space 3`, `:len 8`, and type the eight notes above in Insert.
4.  `:order random`. The phrase's title says `Random`, and the playhead jumps.
5.  `Space 6`, and make `Mod 4` a `Steps` slot as in the Turing Machine: `K` `steps`
    `Enter` on its type, `K` `note` `Enter` on its mode, `Pitch in scale` as its `Dest`,
    `Depth` `0C`, and `Mutate` `12%` (`+` once, `=` twice).
6.  The drone: `W`, a `VA` (`K`, `va`, `Enter`), `:name Y1`. In the phrase view, `:len 2`, a
    `D-1` on row `00` and an `A-1` on row `01`, each with a gate of `SGL8`: in Insert on the gate
    column, type `SGL8`. On its Mod page, `Mod 4` is an LFO already: give it `Dest` `Cut`,
    `Shape` `Smooth`, `Rate` `2 bars` (each with `K`) and `Depth` `30`, and on the Sound page
    bring its filter's `Cut` down to `60` and `Res` up to `40`.
7.  The T section: `W`, a `Hat` (`K` `/hat` `Enter`), `:name T1`, and a `C-4` on every row
    (a grid of a sixteenth, `i`, auto-jump on, so the status line shows `Jump`, then `a`
    sixteen times). Then `l` across to FX 1, `gg`, `vG`
    and `S`: the column is empty, so the panel first lists the commands. Choose `CHA` (`cha`,
    `Enter`), then on `Fill` choose `Random` with `K`, set `From` to `50` and `To` to `F0`
    (`i` and the value), and `Enter`. Each hat now has its own chance.

The finished song also sets the `String`'s `Decay` `B0` and `Bright` `50`, sends it to the delay
and reverb (`Send A` `B0`, `Send B` `B8`), shortens the hat (`Decay` `28`), and sets the levels.

**One knob, two settings.** Marbles' knob has two halves, and here each is a setting of its
own, as they act on different things: `Mutate` draws the Steps slot's values again, the notes
that come in new, and `:order` is the order the phrase plays its rows, the loop's own notes.
Turn the knob from left to right like this:

| Déjà vu | Here |
|---|---|
| Fully left: every note new | `Mutate` `100%`, `:order forward` |
| Toward noon: new notes now and then | `Mutate` low, as `12%` |
| Noon: the loop locked | `Space g a` on the Steps slot to keep what it drew, then `Mutate` `0%` |
| Past noon: the loop out of order | `:order shuffle`, every note once a pass |
| Fully right: any of the loop's notes each step | `:order random` |

`Space g a` comes first at noon, as `Mutate` at `0%` plays the values as written and forgets
what it drew; Marbles locks the loop it drifted to, and `Space g a` is that lock. The song has a
little of both halves at once, `Mutate` `12%` and `:order random`, which Marbles' one knob can't
do: the phrase's notes jump around while the slot brings a new one in now and then.

**As a patch.** Track 1's Steps slot is the shipped patch `Deja vu`; add it with `:patch`. Its
`Deja vu` macro is the knob's left half, turning `Mutate` from all new at `00` to none at `FF`,
and `Spread` turns its `Depth`. The right half is the phrase's `:order`, which a macro can't turn
and a patch doesn't hold, so give the phrase that yourself.

**Try next.**

*   Turn `Mutate` up for more new notes, and to `0%` for a loop that only ever replays itself.
*   `:order shuffle` in place of `random`: every note once a pass, in a new order each pass.
*   `:order forward`: Déjà vu back at noon, the locked loop as written.
*   `r` in the shape panel deals the hats' chances again: `gv` `S` on FX 1, `r`, `Enter`.
*   `:seed 7`, or any number, for another roll of every chance and draw.

## Euclidean rhythms

Spread some hits as evenly as they go over some steps and you get a rhythm much older than any
sequencer: 3 in 8 is the tresillo, 5 in 8 the cinquillo, and Godfried Toussaint's paper
[*The Euclidean Algorithm Generates Traditional Musical
Rhythms*](https://cgm.cs.mcgill.ca/~godfried/publications/banff.pdf) finds dozens more, from West
African bells to Bulgarian folk dances. Euclidean sequencers give each track a length, a number
of hits and a rotation, and stack them.

**How the song does it.** Every part is made by the shape panel's `Euclid` on an empty Note
column, and kept live, so its `Hits` and `Rotate` can be turned later. The kick is 5 in 16
(`x..x..x..x..x...`), the snare 2 in 16 rotated by 4, a backbeat, and the bass 7 in 16 rotated
by 2, going between `C-2` and `C-3`. The hats are 7 in 12, the West African bell pattern, on a
track at `1/8T`: twelve triplet eighths a bar against the others' sixteen sixteenths, a true
polyrhythm. [`song.ron`](songs/euclid/song.ron)

**Build it.**

1.  `:set scale=minor` and `:bpm 124`.
2.  The kick: track 1, `Space 5`, `K` `/kick` `Enter`, `:name BD`. `Space 3`, then `vG` on the
    Note column to select all 16 rows, and `S`. The panel's title says `Generate`, as there are
    no notes to reshape.
3.  In the panel, `j` to `From` and `i` `c1` `Enter`, then `j` to `To` and `i` `c1` `Enter`, so
    every hit is a `C-1`. `j` down to `Where`, `K`, `euclid`, `Enter`. `j` to `Hits`, which
    says `3 of 16`, and `=` twice for 5. The grid follows as you go.
4.  `L` keeps it live. The phrase's title shows `󰁨 Note`, and its notes are drawn in teal.
5.  The snare: `W`, on its Sound page `K` `snare` `Enter`, `:name SD`, and in the phrase view
    the same with `g2` for both ends, `Hits` 2 and `Rotate` (under `Hits`) `=` four times, then
    `L`.
6.  The hats: `W`, on its Sound page `K` `/hat` `Enter`, `:name HH`, and `/speed` `Enter` and
    `=` once for `1/8T`. In the phrase view, `:len 12`, then `vG`, `S`, both ends `c4`,
    `Euclid` with 7 hits, and `L`.
7.  The bass: `W`, on its Sound page `K` `acid` `Enter`, `:name BS`. In the phrase view, `vG`,
    `S`, then on `Fill`, `K`, `alternate`, `Enter`: the ends are now the two notes it goes
    between, `c2` and `c3`. `Euclid` with 7 hits and `Rotate` 2, and `L`.

The finished song also lengthens the kick (`Decay` `90`), shortens the snare (`Decay` `60`) and
the hat (`Decay` `38`), sets the `Acid`'s `Cutoff` `50`, `Resonance` `90` and `Env mod` `90`,
sends the snare to the reverb, and sets the levels.

**As a patch.** The bass is the shipped patch `Euclid`, with its `Hits` and `Rotate` on macros;
add it with `:patch`.

**Try next.** Every part stays live, so `gl` on any of its rows opens its panel again, as it
was:

*   On the bass, `gl`, `j` down to `Rotate`, and `=` while it plays: the riff slides along the
    bar, a new groove each time. `L` keeps it.
*   On the kick, `Hits` 4 is four on the floor, 3 a half-time stomp, 7 a breakbeat.
*   On the hats, `Hits` 5 is the Cuban cinquillo's cousin, 9 a busy shaker.
*   `Space g a` on a part turns it back into plain notes, when you want to edit it by hand.

## Fugue Machine

[Fugue Machine](https://alexandernaut.com/fuguemachine/) is an iPad sequencer that plays one
melody with up to four playheads at once, each at its own speed and direction and transposed on
its own, so a single line becomes a canon: the same tune against itself, faster, slower,
backwards and in another key.

**How the song does it.** The theme, eight notes in A harmonic minor, is on `V1`. The other three
tracks follow it with the shape panel's `Follow`, kept live: each writes the theme's notes into
its own phrase, turned its own way, and plays them at its own `Speed`.

| Track | Voice | Speed | Order | Follows `T01:00` |
|---|---|---|---|---|
| `V1` | `FM` | `1/8` | `forward` | The theme itself |
| `V2` | `String` | `1/16` | `up-down` | A fifth up: `Transpose +4` |
| `V3` | `VA` | `1/2` | `forward` | Inverted, an octave down: `Invert On`, `Transpose -7` |
| `V4` | `Modal` | `1/8T` | `forward` | Backwards, two rows late: `Reverse On`, `Rotate 2` |

With the scale on, `Transpose` counts the scale's degrees, so `+4` is a fifth in key and `-7` an
octave. `V3` is the canon's slow voice, its augmentation: the theme upside down as a bass line
in half notes, a quarter of the theme's speed, across four bars, while `V2` runs up and down the
answer four times a bar. `V4` goes round three times in two bars, backwards.
[`song.ron`](songs/fugue/song.ron)

```text
00  A-3 64        04  B-3 64
01  E-4 50        05  C-4 50
02  D-4 64        06  G#3 64
03  C-4 50        07  B-3 50
```

Edit the theme on `V1` and the other three change with it, in the same undo step, so `u` takes
the edit back from all four. Each track still owns its phrases: a follower keeps its own copy of
the notes, and `Follow` writes it again whenever the theme changes. The followers' titles say
`󰁨 Note←T01:00`, and the theme's says `Followed by 3`.

**Build it.**

1.  `:set scale=harmonic-minor`, `:set root=a` and `:bpm 92`.
2.  Track 1: `K` `fm` `Enter`, `:name V1`. `Space 3`, `:len 8`, and type the theme.
3.  Track 2: `W`, `:len 8`, then `0` `gg` `vG` `S` to shape its empty Note column, and `K`
    `follow` `Enter`. `Source` starts on `T01:00`, the phrase beside it on the track before.
    `j` `j` to `Transpose`, `i` `4` `Enter`, then `L` to keep it live.
4.  Track 3: `W`, `:len 8`, `0` `gg` `vG` `S` `K` `follow` `Enter`. `Source` starts on `T01:00`
    again, as the phrase beside it follows that one. `j` `j` `i` `-7` `Enter` for an octave down,
    and `j` `=` to turn `Invert` on. `L`.
5.  Track 4: the same up to `Enter`, then `j` `j` `j` `j` `=` to turn `Reverse` on, and `j` `=`
    `=` for `Rotate` `2 rows late`. `L`.
6.  The voices and speeds: on each track, `Space 5`, its machine (`string`, `va`, `modal`), its
    name, and `/speed` `Enter`, then `=` for each step slower: three times for `1/8` on `V1`,
    eight times for `1/2` on `V3`, once for `1/8T` on `V4`. `V2` stays at `1/16`.
7.  The order: in the phrase view, `:order up-down` on track 2.

The finished song also shapes the voices: the `FM`'s `Index` `40`; `String` `Decay` `90`; plucky
envelopes on `Mod 1` of the `FM` and the `VA` (a `Decay` of `400 ms` and `900 ms`, `Sustain` `40%`
and `60%`); the `VA`'s `Cut` at `70`; the `Modal`'s `Decay` `90`; `V2` panned left and `V4`
right, some delay and reverb, and the levels.

**Try next.**

*   Change a note of the theme while it plays, and hear all four voices take it.
*   `gl` on `V4`'s notes, then `Rotate` `4` for a later entry, or `Invert` on with `About` on
    another note, so it turns about that one.
*   Another order on `V2`: `:order down-up`, `converge` or `pedal`.
*   `V4` at `1/16T`: still triplets, twice as fast, and the canon tightens.
*   `V3` at `1/1`, a whole note a row, and the slow voice takes eight bars; or at `1/4D`, a
    dotted quarter, three eighths a row, so its notes fall across the beats and meet the bar again
    every three bars.
*   `Space g a` on a voice freezes it: it keeps its notes as plain ones, and stops following.

## Kria

[Kria](https://monome.org/docs/ansible/kria/), for monome's Ansible and Grid, splits a note into
parts, its trigger, its pitch, its octave, its length and more, and gives each part a loop of
its own length. A trigger loop of 16, a note loop of 5 and an octave loop of 3 never line up
the same way twice in a bar, so a few steps make a line that keeps turning over.

**How the song does it.** Track 1, a `VA`, has a trigger loop: a phrase of 16 rows, every note
the scale's root, `E-3`, in E minor pentatonic. The other loops each run on sixteenths, apart
from the notes:

| Loop | Length | Where | Values |
|---|---|---|---|
| Trigger | 16 | The phrase | `x.x.xx.x.xx.x.xx` |
| Pitch | 5 | `Mod 4`, `Steps` in `Free` on `Pitch in scale` | `00 03 07 05 0A` |
| Octave | 3 | `Mod 5`, `Steps` in `Free` on `Pitch` | `00 00 0C` |
| Length | 4 | FX 1, a lane of 4 holding `LEN` | `20 C0 08 E6` |

In `Free` a Steps slot runs from the song's position, a step each sixteenth, as Kria's loops
do, so each note takes whatever pitch and octave loop are on when it lands. Track 2 is a second
Kria line made by one generator: the shape panel's `Acid` fill, whose five parts each loop on a
length of their own, under a kick on every beat and hats in threes.
[`song.ron`](songs/kria/song.ron)

| Loop | Length | What goes round |
|---|---|---|
| `Trig loop` | 12 | Which rows play |
| `Note loop` | 7 | Which of the riff's notes each plays |
| `Oct loop` | 5 | Which go an octave up or down |
| `Acc loop` | 3 | Which are accented |
| `Sld loop` | 4 | Which slide into the next |

Each part is drawn for its first rows, 12 or 7 or 3, and goes round them. The count runs on
from pass to pass, so on a phrase of 16 the trigger loop starts the second pass four rows into
its round. The loops come round together only after 420 rows, their least common multiple,
over 26 bars, and the phrase plays the same pass again only after 105 passes.

**Build it.**

1.  `:set scale=minor-pentatonic`, `:set root=e` and `:bpm 120`.
2.  Track 1: `K` `va` `Enter`, `:name KR`. In the phrase view, with a grid of a sixteenth, type
    the trigger loop, an `E-3` (`a` in octave 3) on each `x`.
3.  The pitch loop: `Space 6`, `Mod 4` a `Steps` slot (`K` `steps` `Enter` on its type).
    `=` once on its mode for `Free`, and `Pitch in scale` as its `Dest`. Its `Depth` stays `+FF`,
    so a value is that many semitones before the scale moves it: `03` is a minor third. `Length`
    `5` (`-` three times). Then `l` from the right-hand column goes into its box, on the first
    step: `l` to the second, `i` `03` `Enter`, and so on for `07`, `05` and `0A`.
4.  The octave loop: `Mod 5`, under it, the same, but with plain `Pitch` as its `Dest` (`K`,
    `pitch`, `Enter`), `Length` `3`, and its third step `0C`, an octave.
5.  The length loop: `Space 3`, `l` across to FX 1 on row `00`, and `K` `len` `Enter` `20`. On
    the next three rows the same with `C0`, `08` and `E6`. Back on row `00`, `:lane 4`: FX 1 now
    loops its first four steps on its own, and the title says `FX1:4`.
6.  `Tab`. The notes are all `E-3` on the grid, but no two bars sound alike.

Track 2, all in one generator:

1.  `K` `acid` `Enter`, `:name BS`. In the phrase view, `vG` on the empty Note column of its 16
    rows, `S`, and on `Fill`, `K` `acid` `Enter`.
2.  `From` `e2`, `To` `e3`, `Octave` `30%`, `Density` `60%`, `Accent` `40%` and `Slide` `30%`.
3.  Under `Pitches`, the loops: `Trig loop` `12`, `Note loop` `7`, `Oct loop` `5`, `Acc loop` `3`
    and `Sld loop` `4` (`i`, the number, `Enter`). `0` is `Off`, the part drawn afresh on every
    row of the span, the same each pass, as `Acid` always was.
4.  `r` until the first bar sounds right, and `L` to keep it live: the loops only run on from
    pass to pass while it's live, as the engine plays each pass.

On the Gen page (`Space 8`) the loops are a device of their own, `Loops`, under the `Acid`'s,
which says how long the whole line takes to come round. Then a `Kick` on every beat, and a
`Hat` with `:len 3` and three `C-4`s at velocities `60`, `20` and `38`.

The finished song also makes the `VA` a pluck (`Mod 1`'s `Decay` `300 ms` and `Sustain` `50%`),
sets its `Cut` `90` and `Res` `50`, sends it to the delay, sets the `Acid`'s `Cutoff` `48` and
`Resonance` `A0`, a shorter hat, and the levels.

**As a patch.** Track 1's pitch, octave and length loops are the shipped patch `Kria`; add it
with `:patch`. `Pitch loop` and `Octave loop` are macros turning the two slots' `Length` from 1
step to 16. The length loop comes as it is, a live `List` of the four `LEN`s on a lane of 4,
and the patch gives the FX column it goes on a lane as long. `Length loop` turns that lane's
length, `FX1 Length` on the Gen page's `Lane`, from 1 row to 16: past 4 the lane plays its four
lengths again from the first, so at 6 it's `20 C0 08 E6 20 C0`, and the line drifts against
itself as Kria's does. Track 2 is the shipped patch `Acid loops`, its five loops on macros, `Trig
loop`, `Note loop`, `Oct loop`, `Acc loop` and `Sld loop`, each from 1 row to 16.

**Try next.**

*   `Length` 7 on the pitch loop, or 4 on the octave loop: the line changes completely.
*   `:len 13` on track 1, a trigger loop that drifts against the bar too.
*   `Rate` `1/8` on the octave loop, so it turns over half as fast, as Kria's clock divisions do.
*   The lane's length: `:len 5` with the cursor on FX 1, or `Length loop` on the patch.
*   `Rate` `÷2` on the Gen page's `FX1 Lane`, so each length lasts two steps, or `:lane /3` on
    FX 1 for three: Kria's clock division on one part.
*   On track 2, `Acc loop` `16` against `Trig loop` `12`: the accents fall somewhere new each
    bar for three bars. Or every loop at `16`, and it's an ordinary 16-row `Acid` line again.
*   `Space g a` on track 2's Gen page keeps the pass just heard as the one written, and the
    loops go on from there.

## Metropolix

[Intellijel's Metropolix](https://intellijel.com/shop/eurorack/metropolix/) is a sequencer of
eight stages, each with a pitch, a number of pulses and a gate type: one note held over all its
pulses, a ratchet of a note on each, a tie into the next, or a rest. On top of that, each stage
can accumulate, moving its pitch on each time round; a track can play its stages in many orders,
pendulum, random and the rest; and an arpeggiator and clock dividers make second lines from the
first.

**How the song does it.** Track 1, an `Acid` in A minor, is eight stages. Its gate column is the
Metropolix's: `SGL` one note over the row's pulses, `MLT` a note on each, `HLD` a slide into the
next, `RST` a rest. Two rows carry an `ACU`, so row `03` climbs a degree each time it plays and
row `07` falls one, bouncing at four degrees, and `:order up-down` plays the stages there and
back. The pulses add up to 16, so a pass is a bar, while the pendulum takes 14 visits to come
round and drifts against it. Track 2, a `VA` at `1/8`, half the clock, holds a chord a half bar
and arpeggiates it with `ARP 24`, a triad in the scale, up and down two octaves.
[`song.ron`](songs/metropolix/song.ron)

```text
Track 1                       Track 2, at 1/8
00  A-1 7F  SGL2              00  A-3 50  SGL4  ARP 24
01  A-1 50  MLT2              01  F-3 50  SGL4  ARP 24
02  C-2 64  HLD2              02  C-4 50  SGL4  ARP 24
03  E-2 64  SGL1  ACU 01      03  G-3 50  SGL4  ARP 24
04  --- --  RST2
05  G-1 64  SGL2
06  A-1 7F  MLT3
07  D-2 64  SGL2  ACU FF
```

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 126`.
2.  Track 1: `K` `acid` `Enter`, `:name MX`. `Space 3`, `:len 8`, and type the notes and
    velocities. On the gate column, `i` and a gate with its pulses, such as `MLT2`, on each row.
    An empty gate is `SGL1`.
3.  The accumulators: on row `03`'s FX 1, `K` `acu` `Enter`, and `Enter` again for `+1`. On row
    `07`'s, the same, then `-` twice on the value for `FF`, one degree down.
4.  On the Mod page, the `Accumulator` device under `Mod 3`: `Up` and `Down` `4` (`i` `4`
    `Enter`), and `Limit` `Bounce` (`K`).
5.  `:order up-down`.
6.  Track 2: `W`, and on its Sound page `K` `va` `Enter`, `:name AR`, and `/speed` `Enter`
    and `=` three times for `1/8`. In the phrase view, `:len 4`, the four notes with `SGL4`, and
    `ARP` on each (`K` `arp` `Enter`, then `24`).
7.  Its `Arpeggio` device, beside the accumulator: `Rate` `1/32`, which on a track at `1/8` is a
    sixteenth, `Mode` `Up-down`, `Oct` `2` and `Retrig` `on`.

Then a `Kick` on every beat, and a `Hat` on the off-beats with a `MLT2` on row `0E`, a ratchet.
The finished song also sets the `Acid`'s `Cutoff` `50`, `Resonance` `C0`, `Env mod` `B0` and
`Decay` `50`, makes the `VA` a short pluck (`Mod 1`'s `Decay` `120 ms` and `Sustain` `20%`, with
`Cut` `88`), sends both to the delay, the `VA` to the reverb too, and sets the levels.

**Try next.**

*   Other orders on track 1: `:order pedal`, `converge`, `odd-even`, `hopscotch`, `growing`,
    `jump 3`, `arp 3 1` or `random`, each as the Metropolix's ORDER plays it.
*   `:order brownian`, the Metropolix's drunken walk: from the first stage each bar, a stage on
    half the time, the same a quarter and back a quarter.
*   On the `Accumulator` device, `Limit` `Bounce bi`: row `03` climbs four degrees, turns, and
    goes on down past its note to four below, and row `07` does the same upside down. `Random`
    walks them drunkenly instead, mostly each `ACU`'s way, and `Random bi` between both limits.
*   On the `Accumulator` device, `Moves` `Track`: every row climbs together with each `ACU`.
*   `Reset` every 4 loops, so the line climbs for four bars and starts again.
*   Track 2's `ARP` to `13` or `35` for other shapes, `Mode` `Random`, or `Oct` 3.
*   Track 1 at `1/8`, the clock divided, under the same arpeggio.

## Eloquencer

Winter Modular's Eloquencer is a sequencer whose steps each carry their own chances, which it
calls controlled chance: how likely the step's pitch is to vary and how far, how likely its gate
is to play, how much its gate's length varies, and how likely it is to ratchet, with a range of
ratchets to choose from. The line stays the one you wrote, and wanders round it by as much as you
let each step. `RND xy` is this on a row: a chance `x` in `F` that what's to its left varies, by
up to `y`.

**How the song does it.** Track 1, a `VA` in D minor, is a line of twelve notes. The roots carry
`RND 31`, so a fifth of the time they move a degree, and the rest wander more: `RND 62` and
`RND A2` move up to two degrees, two times in five and two in three, `RND 83` up to three about
half the time, and the last note, `RND F3`, always moves. The two `F-3`s have none. On the roots and the `G-3`, an `RND` beside a `LEN` varies the length instead:
`LEN 60 RND A4` plays the note anywhere from `20` to `A0` long, two times in three. Track 2's hats
have the other two of the module's chances: an `RND` beside a `VDLT 80` on the off-beats, so
their velocity wanders up to `30` either way, and beside a `RET 02` before each beat, so they
ratchet two times in five, with the ratchet's units anywhere from 1 to 3. Track 3 is a kick on
every beat. [`song.ron`](songs/eloquencer/song.ron)

```text
Track 1                                Track 2
00  D-3 64  RND 31  LEN 60  RND A4     00  --- --
01  --- --                             01  --- --
02  A-3 50  RND 62                     02  C-4 50  VDLT 80  RND F3
03  F-3 50                             03  C-4 40  RET 02   RND 61
04  D-3 64  RND 31  LEN 60  RND A4     ... the same each beat
05  C-4 50  RND A2
06  --- --
07  A-3 50  RND 62
08  D-3 64  RND 31  LEN 60  RND A4
09  --- --
0A  E-3 50  RND 83
0B  F-3 50
0C  G-3 64  RND 62  LEN 40  RND A6
0D  A-3 50  RND A2
0E  --- --
0F  C-4 50  RND F3
```

Each roll is worked out from the `:seed` and where the track is, so the line goes a different
way every bar, the same every time you play it, and playing from a step plays what playing up to
it would.

**Build it.**

1.  `:set scale=minor`, `:set root=d` and `:bpm 116`.
2.  Track 1: `K` `va` `Enter`, `:name EQ`. In the phrase view, type the notes and velocities.
3.  The chances: on row `00`'s FX 1, `K` `rnd` `Enter` and `31`. The status line says what it
    does: `20% of the times it plays, moves the note up to 1 scale degree either way`. Then the
    other rows' `RND`s the same way, each in FX 1, so there's nothing to their left but the note.
4.  The lengths: on row `00`'s FX 2, `K` `len` `Enter` `60`, and on its FX 3 `K` `rnd` `Enter`
    `A4`. Beside the `LEN`, the status line says `67% of the times it plays, moves LEN up to 40
    either way`. The same on rows `04` and `08`, and `LEN 40 RND A6` on row `0C`.
5.  Track 2: `W`, `K` `/hat` `Enter`, `:name HH`, and `C-4 50` on rows `02`, `06`, `0A` and
    `0E`, each with `VDLT 80` (`K` `vdlt` `Enter` `80`), which leaves the velocity as written,
    and `RND F3` beside it. On rows `03`, `07`, `0B` and `0F`, `C-4 40` with `RET 02` and
    `RND 61`.
6.  Track 3: a `Kick` on every beat.

The finished song also makes the `VA` a pluck (`Mod 1`'s `Decay` `260 ms` and `Sustain` `45%`),
sets its `Cut` `90`, sends it to the delay and the reverb, a shorter hat, and the levels.

**Try next.**

*   `+` on an `RND`'s value raises its chance a step and `=` widens its range, so the roots
    wander as much as the rest, or `-` and `_` pin a note back down.
*   `:seed 9`, and every `RND` rolls otherwise, as the module's random source would.
*   `:set scale=off`: the same `RND`s move by semitones, and the line goes chromatic.
*   An `ACU 01` on row `05`: the `C-4` climbs a degree each bar and wanders round where it's got.
*   When a bar goes somewhere you like, `Space g A` on track 1 writes its notes in, as they were
    heard, and takes their `RND`s out, so it stays. The `RND`s beside the `LEN`s stay, so the
    lengths keep varying.

## Live generators

Generators on Elektron's newer boxes, the Cirklon and the Hapax stay tied to their settings:
a Euclidean track keeps its pulses and rotation as knobs, so a pattern can be turned while it
plays rather than written again. In vimtracker that's the shape panel's `L`: the column keeps its
`Shape`, and `gl` opens it again.

**How the song does it.** Three live columns, under a kick and snare written by hand:

| Track | Column | Shape |
|---|---|---|
| `HH` | Velocity | `Euclid` 5 hits over the 16 hats, at `7F`, as accents |
| `BS` | Note | `Random` notes from `F-1` to `F-2` in F minor, on `Euclid` 9 of 16 |
| `BS` | FX 1, a lane of 12 | A `Wave` of `Acid`'s `FREQ` from `30` to `A0`, once round the lane |

The hats are typed quietly, at `34`, and the generator puts the accents on 5 of them. The bass is
written by the panel on an empty Note column. Its filter wave runs on a lane of 12 steps, so it
opens and closes against the 16-step line, landing somewhere new each bar.
[`song.ron`](songs/live-generators/song.ron)

**Build it.**

1.  `:set scale=minor`, `:set root=f` and `:bpm 122`.
2.  Track 1, a `Kick` on every beat. Track 4, a `Snare` (`G-2`) on rows `04` and `0C`.
3.  The hats, on track 2: `K` `/hat` `Enter`, `:name HH`. In Insert, `c` three times for a
    velocity of `34`, then a `C-4` on all 16 rows with auto-jump and a grid of a sixteenth.
4.  `l` to the Velocity column, `gg`, `vG`, `S`. `From` and `To` both `7F` (`i` `7F`), `Where`
    `Euclid`, `Hits` 5, and `L`. Five hats are accents now.
5.  The bass, on track 3: `K` `acid` `Enter`, `:name BS`. On the Note column, `vG`, `S`:
    `Fill` `Random` (`K`), `From` `f1` and `To` `f2` (`i`, the note, `Enter`), `Where` `Euclid`
    with 9 hits, and `L`. `r` in the panel, before `L`, picks another line.
6.  Its filter: `l` across to FX 1, `:lane 12`, `vG`, `S`, and from the commands `cutoff`
    `Enter`, which is `Acid`'s `FREQ`. `Fill` `Wave`, `From` `30`, `To` `A0`, `Rate` `x1` (`K`,
    `x1`, `Enter`), and `L`. It fills the lane, and the title says `FX1:12  󰁨 Note,FX1`.

The finished song also sets the `Acid`'s `Resonance` `B0` and `Env mod` `70`, the hat's `Decay`
`2C`, the delay and reverb sends, and the levels.

**What a hand edit does.** A live column follows the rest of the phrase:

*   Clear a hat with `x` on its note. The accents spread themselves over the 15 left, as
    `Euclid` on Velocity counts among the notes. `u` brings it back, accents and all.
*   Type a velocity over one of the hats yourself, and the column is yours: it's applied, its
    values stay, and the status line says so. `u` makes it live again.
*   Edit any other column, and it stays live.

**As a patch.** The hats' accents are the shipped patch `Accents`, with `Hits` and `Rotate` on
macros; add it with `:patch` to a track with notes to accent.

**Try next.**

*   `gl` on the hats' Velocity, then `Rotate`, a row at a time, while it plays.
*   `gl` on the bass, `r` for a new line, `L` to keep it, as many times as you like.
*   `gl` on FX 1, `Shape` `Saw` or `Smooth`, or `:lane 7` for a shorter drift: the wave fills the
    new lane.
*   `Space g a` on any of them when you're done, to keep the values as plain ones.
*   All three on track 3's bass at once: `Space 8`, the Gen page, where each live
    column of the phrase under the cursor is a device, `Random  Euclid · Note 00–0F` and
    `Wave  All rows · FX1 FREQ lane 0C`. `jj$` on the note device lands on `Hits`, and `=` and `-` turn it
    as the line plays, each press its own undo step.
*   `M1` on `Hits` maps it to macro 1, then on the Mod page an LFO with `Dest` `MAC1`: the hits
    move under the playhead as it sweeps. The title says `󰁨 Note~`, the row playing shows the
    notes heard, and `Space g a` keeps the pass just heard as plain notes.

## Acid lines

The [TB-303](https://www.roland.com/global/products/rc_tb-303/)'s sequencer was so awkward to
program that its lines came out half by accident, and that's the sound: a few notes round a
root, some an octave up, a few accented, a few sliding into the next. Roland's software 303
has a randomiser that rolls a whole line, or keeps the notes and rolls only the rest. David
Whiting's [Endless Acid Banger](https://www.vitling.xyz/toys/acid-banger/) does it for ever, in a
browser.

**How the song does it.** Track 1, an `Acid` in C minor, has a line written by the shape panel's
`Acid` fill on an empty Note column: four notes from `C-2` up to `C-3`, its root most often,
half the rows playing, the beats more than the rest, with accents (`7F`) and slides (`HLD`)
rolled at the same time. It's kept live, so `gl` and `r` roll another while it loops, under a
kick, a snare and hats. [`song.ron`](songs/acid-lines/song.ron)

**Build it.**

1.  `:set scale=minor` and `:bpm 126`.
2.  Track 1: `K` `acid` `Enter`, `:name TB`.
3.  `Space 3`, `vG` on the Note column, and `S`. On `Fill`, `K`, `acid`, `Enter`.
4.  `From` `c2` and `To` `c3` (`i`, the note, `Enter`). `Notes` is how many notes the riff has,
    `Density` how many rows play, `Accent` and `Slide` the chance of each.
5.  `r` until you like it, and `L` to keep it live.

Then a `Kick` on every beat, a `Snare` (`G-2`) on rows `04` and `0C` and a `Hat` on the
off-beats. The finished song also sets the `Acid`'s `Cutoff` `50`, `Resonance` `C0`, `Env mod`
`A0` and `Decay` `58`, sends it to the delay, and sets the levels.

**As a patch.** The line is the shipped patch `Acid line`, its dice on five macros, `Density`,
`Notes`, `Octave`, `Accent` and `Slide`; add it with `:patch`.

**Try next.**

*   `gl`, `r`, `L` while it plays: a new line every bar, until one sticks.
*   `Pitches` `Keep` in the panel: `r` then rolls only the accents, slides and octaves, Roland's
    second die, and the notes stay.
*   `Density` up for a busier line, `Notes` 2 for a two-note pump, `Octave` up for more jumps.
*   `Spread` and `Bias` choose the riff's notes besides its root, as they lean `Random`'s
    values in Marbles' X below: `Spread` `0%` gathers them round the middle of the range,
    `D#2 F-2 G-2`, a tight riff, and `Bias` `+100` with it takes them from the top, `G#2 A#2
    C-3`, leaps up from the root. On the Gen page they're in the fourth column, to turn while
    it plays.
*   Write a line by hand first, then `S` with `Acid` on it: its rhythm stays, and the panel gives
    it new pitches, accents and slides.
*   `Space g a` keeps the one you like as plain notes, to tweak by hand.
*   On the Gen page, `Vary` `Each pass` (`K` on it, after `Pitches` in the title): a new line
    every time round, as Endless Acid Banger plays, and the first pass is the one written. `Every
    4 passes` holds each for four bars. When one sticks, `Space g a` keeps it, the line just heard,
    and `Vary` goes back to `Once`.

## Walks and 1/f

Max's `drunk` object is the classic random
walk: each number a small step from the last, so a line drifts the way a hand does over the
keys. In 1978 R. F. Voss and J. Clarke made melodies from three kinds of noise and played them
to listeners: white, each note random, sounded too random, and brown, a walk, too wandering. They
liked 1/f, or pink, noise best, which wanders but keeps coming back, as many tunes do. Martin
Gardner's column on their paper gave the dice method this fill uses: several dice, each thrown
half as often as the one before, summed.

**How the song does it.** Three live columns in A minor, over a kick:

| Track | Column | Shape |
|---|---|---|
| `LD` | Note | `1/f` from `A-2` to `A-4` on every other row of two bars, `Depth` 4 |
| `BS` | Note | `Walk` from `A-1` to `A-2`, `Step` 1, on `Euclid` 7 of 16 |
| `HH` | Velocity | `Walk` from `30` to `70`, `Step` `10`, over 16 hats |

The tune moves a few degrees at a time and now and then further, and comes back toward the
middle; the bass walks up and down the scale a degree at most; the hats' velocity drifts, as a
drummer's does, rather than jumping about as `Random` would. [`song.ron`](songs/walks/song.ron)

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 96`.
2.  The tune, on track 1: `Space 5`, `K` `string` `Enter`, `:name LD`. `Space 3`, `:len 32`
    for two bars, then `vG` on the Note column and `S`. On `Fill`, `K`, `/1/f`, `Enter`: its name
    starts with a digit, so `/` comes first. `From` `a2` and `To` `a4` (`i`, the note, `Enter`),
    `Where` `Every` with an `Amount` of 2, and `L`.
3.  The bass: `W`, on its Sound page `K` `va` `Enter`, `:name BS`, and in the phrase view `vG`,
    `S`, `Fill` `Walk` (`K` `walk` `Enter`), `From` `a1`, `To` `a2`, `Step` 1, `Where` `Euclid`
    with 7 hits, and `L`.
4.  The hats: `W`, on its Sound page `K` `/hat` `Enter`, `:name HH`, and in the phrase view a
    `C-4` on all 16 rows. `l` to the Velocity column, `gg`, `vG`, `S`, `Fill` `Walk`, `From` `30`
    and `To` `70` (`i`, two hex digits), leave `Step` at the `10` it starts at, and `L`.

Then a `Kick` on every beat. The finished song also sets the string's `Decay` and `Bright`, the
bass's filter, the delay and reverb sends, and the levels.

**As a patch.** The bass is the shipped patch `Drunk walk`, with `Step` from a degree to seven
and `Hits` on macros; add it with `:patch`.

**Try next.**

*   `gl` on the tune, then `r` for another: each is new, but they all wander the same way.
*   `Depth` 2 for a tune nearer `Random`, 8 for long rises and falls.
*   `Fill` `Walk` on the tune, then `1/f` again, to hear brown against pink; `Random` is white.
*   `Edge` `Wrap` on the bass: a walk past `A-2` comes back in at `A-1`.
*   `Step` `20` on the hats for a rougher player, or `Run` 2 to hold each velocity for two.

## Numeric Repetitor

Noise Engineering's Numeric Repetitor reads a 16-step rhythm as a 16-bit number and multiplies
it: one knob turns a single rhythm into a family of related ones, all of them still in step with
it. Doubling moves the rhythm a step later, and an odd number always keeps the downbeat.

**Try it on the `euclid` song.** Open it, and on the kick's phrase (track 1, `Space 3`):

1.  `Space g a` turns the live `Euclid` into plain notes, `x..x..x..x..x...`.
2.  `gg` `vG` on the Note column and `S`. On `Fill`, `K`, `multiply`, `Enter`. `Factor` starts
    at 3: `xx.xx.xx.xx.xx..`, a gallop.
3.  `L` keeps it live, so `gl` opens it again later.

**Try next.** `gl` on the kick and `=` or `-` on `Factor` while it plays:

| Factor | Kick |
|---|---|
| 1 | `x..x..x..x..x...`, as it was |
| 2 | `.x..x..x..x..x..`, a row late |
| 5 | `x.xx.xx.xx.xx.x.` |
| 9 | `x...x..x..x..x.x` |
| 11 | `xx...x..x..x..xx` |

`Masks` `On` thins the rhythm before 2 and 3 multiply it, as the module does. On the bass,
after `Space g a`, whose notes differ, a new note takes the pitch of the one before it, so the
line keeps its shape while the rhythm changes.

## Leibniz

Xaoc Devices' Leibniz subsystem turns a voltage into an 8-bit number and back, and its modules
work on the bits in between: Lipsk inverts any of them, Gera masks them off. Xaoc encourage
misusing them, and on a melody or a velocity they give odd folds and quantizations that no
other knob reaches.

**Try it on the `walks` song.** Open it, and on the tune's phrase (track 1, `Space 3`):

1.  `Space g a` turns the live `1/f` into plain notes.
2.  `gg` `vG` on the Note column and `S`. On `Fill`, `K`, `flipped`, `Enter`, for `Invert
    bits`. `j` to `Flip`, and `i` `01`: each note swaps with its neighbour degree in A minor, a
    tune that moves the same way but never on the same notes.
3.  `i` `03` swaps them in fours, and `04` throws every other group up or down four degrees.
    Every note is still in the scale.
4.  `L` keeps it live, so `gl` opens it again.

**Try next.**

*   On the hats' Velocity, after `Space g a`, `Mask bits` with `Keep` `F0` steps the walk's
    drift into steps of `10`, and `E0` into steps of `20`, a drum machine's few accent levels.
*   `Rotate bits` on the hats turns a soft velocity loud and a loud one soft: `Turn` 1 doubles
    each, and the loudest come round to quiet.

## Grids

[Mutable Instruments' Grids](https://pichenettes.github.io/mutable-instruments-documentation/modules/grids/)
plays a kick, a snare and a hat from a map of 25 drum patterns, made by hand from real beats.
Two knobs, X and Y, move round the map, blending the four patterns nearest, so the beat changes
smoothly from one style to the next. Each step of a pattern has a level, how much it belongs to
it, and each drum has a density knob: turning it up adds the next most important hit first, so a
sparse kick fills in from the downbeat outwards rather than at random. Chaos raises the levels at
random, so weaker hits come and go.

**How the song does it.** Tracks 1 to 3 are a `Kick`, a `Snare` and a `Hat`, each with one live
`Grids` column over two bars, all at the same place on the map, `X` `40` and `Y` `40`. Each plays
its own part of the pattern there: the kick's, the snare's or the hats', as its 808 voice says.
The hats' density is higher, `A0`, and `Chaos` `C0` makes the two bars differ. Track 4 is an
`Acid` bass. [`song.ron`](songs/grids/song.ron) The beat, its accents `X`:

```text
BD  X...x.X.X.x.x.X.  X...x.X.X.x.x.x.
SD  ....X.X...x.X.x.  ..x.X.x...x.X.X.
HH  XxX.X.x.X.X.X.x.  X.X.X.x.X.X.X.x.
```

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 118`.
2.  Tracks 1 to 3: the machines `kick`, `snare` and `hat` (`K` `/kick` `Enter` and so on), named
    `BD`, `SD` and `HH`. On each, `Space 3`, `:len 32`, and one note on row `00` for its pitch:
    `C-1`, `G-2` and `C-4`.
3.  Back on track 1, `gg` `vG` on the Note column, `W` `W` to reach track 3, and `S`. On `Fill`,
    `K` `/grids` `Enter`. The panel shows `Each track plays its own part`, and the three tracks
    take the kick's, the snare's and the hats' parts, as their machines say.
4.  `j` to `X` and `i` `40`, `j` to `Y` and `i` `40`. Then `j` `j` `j` to `HH fill`, the hats'
    density, and `i` `A0`, and `j` to `Chaos` and `i` `C0`. Two hex digits are kept as they're
    typed.
5.  `L` keeps it live, a generator on each track.
6.  Track 4: `K` `acid` `Enter`, `:name BS`, and `A-1`s on `x..x..x...x.x...`.

A note it plays keeps its pitch and a new one takes the first note's, which is why each track
starts with one. The snare doesn't play on row `00`, so its `G-2` is gone from the grid, but the
generator still has it, under its own notes. Tracks with names it can't tell take the parts in
order, so three tracks of samples named `BD`, `SD` and `CH`, or nothing at all, work the same.

**Try next.**

*   `gl` on any drum and `+` or `_` on `X` or `Y`: that drum moves through the map alone. Do it
    on all three for a new beat, or `S` across them again.
*   `Y` `00` with the kick's density at `40`: four on the floor. Up from there, it fills in.
*   `r` on the hats rolls their chaos again; `Chaos` `00` makes both bars the same.
*   On the Gen page, `Space 8`, the drum's device is titled `Grids  BD`, its map, density and
    chaos ready to turn while it plays, and on the Roto its first knobs.

## Vermona's randomRHYTHM

Vermona's randomRHYTHM, a Eurorack module of 2018, makes rhythms from chances. Each of its two
channels has four sliders, the quarters, the eighths, the sixteenths and the triplets, and each
slider is how likely that division is to strike. Every strike a bar
could have has a random value, and a slider is the line it must get over, so at the bottom a
division never plays, at the top it always does, and in between, pushing a slider up only adds
strikes to the ones you have. In `Dice` mode the values are fixed for a bar, so the bar repeats
while the sliders reshape it; in `Realtime` mode they're rolled again all the time. A third
output plays both channels at once. `Divisions` is all of this, with three triplets, and the two
channels and their sum as three tracks.

**How the song does it.** Tracks 1 to 3 are a `Kick`, a woodblock, a `Modal` voice high up, and a
`Hat`, at `Speed` `1/32`, each with one live `Divisions` column over a bar of 32 rows. The kick
plays channel `A`, mostly quarters, `1/4` `90%`, with `1/8` `20%`, `1/16` `10%` and `1/8T`
`10%`. The woodblock plays channel `B`, mostly triplets: `1/4T` `35%`, `1/8T` `40%` and `1/16T`
`10%`, with `1/8` `25%`. The hats play `A+B`, every strike of either. Track 4 is a snare on the
backbeat and track 5 a bass, both written, at `1/16`. [`song.ron`](songs/random-rhythm/song.ron)
The bar, `x` a strike on its row and `<` and `>` a triplet nudged a third of a row early or late:

```text
BD  x.....x.x.......x.......x.......
WB  x..<x......<x>..x.......x..<.>..
HH  x..<x.x.x..<x>..x.......x..<.>..
```

At `1/32` every division has rows of its own: the sixteenths fall on the even rows, and a
triplet between two rows is the nearer one's, nudged `4`, 40 ticks, either way. So the
woodblock's triplets play as triplets against the kick's straight time, in one track.

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 108`.
2.  Tracks 1 to 3: the machines `kick`, `modal` and `hat` (`K` `/kick` `Enter` and so on), named `BD`, `WB` and `HH`. On each,
    `/speed` `Enter` and `-` twice, to `1/32`; then `Space 3`, `:len 32`, and one note on row `00`
    for its pitch: `C-1`, `E-4` and `C-4`.
3.  Back on track 1, `gg` `vG` on the Note column, `W` `W` to reach track 3, and `S`. On `Fill`,
    `K` `/divisions` `Enter`. The panel shows `Each track its own channel: A, B, A+B`, and `Bar`
    `4/4 32nds`, as the tracks are at `1/32`.
4.  Channel A's chances: `j` `j` to `1/4` and `i` `90` `Enter`, `j` and `i` `20` `Enter`, `j`
    and `i` `10` `Enter`, then `j` `j` to `1/8T` and `i` `10` `Enter`. Then B's: `j` `j` `j` to
    `B 1/8` and `i` `25` `Enter`, `j` `j` to `B 1/4T` and `i` `35` `Enter`, `j` and `i` `40`
    `Enter`, `j` and `i` `10` `Enter`.
5.  `r` rolls new dice until you like the bar, and `L` keeps it live, a generator on each track.
6.  Track 4: `K` `/snare` `Enter`, `:name SD`, and `G-2`s on rows `04` and `0C`. Track 5: `K`
    `va` `Enter`, `:name BS`, and a line of `A-1`s round the kick, with a `C-2`, `G-1` and `E-1`.

A strike on a note keeps its pitch, and a new one takes the first note's, which is why each
track starts with one. All three take one seed, as the module's channels come from one module,
so the hats are exactly the kick's and the woodblock's strikes; the woodblock has channel B's
chances as its own.

**Try next.**

*   `gl` on the woodblock and `+` on `1/8T`: more triplets come in, and every one there stays.
    Down to `0%`, its triplets go, and the straight eighths are left.
*   `1/4` `100%` on the kick: four on the floor, whatever the dice. Then `1/16` up, and the
    sixteenths it adds are always the same ones, as the dice are fixed.
*   On the Gen page, `Space 8`, `Vary` `Each pass` on all three tracks, the module's `Realtime`:
    a new bar every time round. Set it the same on all three, so the hats stay the sum of the
    other two. `Every 4 passes` holds each roll for four bars.
*   `Bar` `3/4 32nds` on the woodblock: its dice go round every 24 rows against the kick's 32,
    so its figure drifts across the bar and comes back every three bars.
*   The hats are a generator of their own, so `gl` on the kick changes the kick alone, and the
    hats keep its old chances. `S` across the three again sets them together.

## Noise Ring

Grant Richter's Wiard 1210 Noise Ring, of the Buchla 266's line and in Eurorack as Malekko's
Richter Noise Ring, is an 8-bit shift register going round. Each clock a bit comes out of one
end and goes back in at the other, unless `Change` replaces it with a new bit, which `Chance`
makes a 1 or a 0. Its outputs are the register itself: each bit as a gate, and the byte as a
stepped voltage. Patch a gate to a drum and the voltage to a quantiser, and the rhythm and the
melody come from the same bits, so they move together: a note is the one before it with its
bits moved along. It's the Turing Machine's cousin, but where the Turing Machine's loop only
chooses pitches over a rhythm you play it, the Noise Ring's is both.

`Ring` is this, a row a clock: a row plays if the bit just taken in is a 1, at the pitch of the
register's byte, from `From` to `To` through the scale. `Change` and `Chance` are the module's two
knobs, and `Length` is the Turing Machine's switch, how many bits go round, 8 as the module has.

**How the song does it.** Track 1, a `Modal` bell, has a live `Ring` over 16 rows from `D-3` to
`D-5` in D dorian: `Change` `15%`, so one bit in seven is new as it comes round, `Chance` `55%`, a
little over half of them 1s, `Length` 8. Kept live, the register runs on from pass to pass, so
each bar plays where the last one left it, drifting a bit at a time. Track 2, a `VA` bass, has a
second ring from `D-1` to `D-2`, locked at `Change` `0%`, its `Length` 5: five bits go round the
bar of 16, so the figure comes in a row earlier each bar and is back where it started on the
sixth. A kick and hats keep time. [`song.ron`](songs/noise-ring/song.ron)

```text
    00  01  02  03  04  05  06  07  08  09  0A  0B  0C  0D  0E  0F
NR  G-4 B-3 --- D-4 --- D-5 --- B-4 G-4 B-3 G-4 B-3 --- D-4 --- D-5
BS  D-2 --- --- E-1 G-1 D-2 --- --- E-1 G-1 D-2 --- --- E-1 G-1 D-2
```

That's the first bar, as the phrases hold it. The bell's second half is its first again with one
bit new, and the next bar, `C-5 B-4 G-4 B-3 --- D-4 --- D-5`, twice, is the same bits moved on.
A falling run, as `C-5 B-4 G-4 B-3`, is a run of 1s at the top of the byte moving out of it a row
at a time.

**Build it.**

1.  `:set scale=dorian`, `:set root=d` and `:bpm 112`.
2.  Track 1: `K` `modal` `Enter`, `:name NR`.
3.  `Space 3`, `vG` on the Note column, and `S`. On `Fill`, `K` `/ring` `Enter`. The panel shows
    `From`, `To`, `Change` `10%`, `Chance` `50%` and `Length` `8`.
4.  `From` `d3` and `To` `d5` (`i`, the note, `Enter`), `Change` `i` `15` `Enter` and `Chance`
    `i` `55` `Enter`. `r` rolls a new ring until you like the bar, and `L` keeps it live.
5.  Track 2: `W`, `K` `va` `Enter`, `:name BS`, then in the phrase view `vG`, `S`, `Fill` `Ring`,
    `From` `d1`, `To` `d2`, `Change` `0`, `Chance` `60` and `Length` `5`, and `L`.

Then a `Kick` on every beat and a `Hat` on the off-beats. The finished song also sets the bass's
filter, the bell's delay and reverb sends, and the levels.

**Play it.** `Tab`, and `Space 8` on track 1 for the Gen page: the ring's device has `From` and
`To`, then `Change`, `Chance` and `Length`.

*   `Change` up to `40%` while it plays: the bar changes more each time round, and at `100%`
    every row is new. Down to `0%` it locks on whatever loop it has reached.
*   When a bar comes round that you like, `Space g a` keeps the register as that pass began: it
    plays first from then on, and the ring runs on from there. `Change` to `0%` then holds that
    loop, the Turing Machine's lock. `r` lets go of it for a new ring.
*   `Chance` is the density: down to `25%` and the bell plays a few notes a bar, up to `80%` and
    most rows play, the new 1s joining the ones there.
*   `Length` 16 for a loop as long as the bar, or 3 for a riff of three bits, its byte the same
    three going round, so a short ring is a repeating arpeggio.
*   The bass's `Length` 4, and it lines up with the bar; 7 and it takes seven bars to come back.

**As a patch.** The bell's ring is the shipped patch `Noise Ring`, with `Change`, `Chance` and
`Length` on macros, the module's two knobs and the Turing Machine's switch; add it with
`:patch` on any track and turn them on the Gen page or the Roto.

## Marbles' X

Marbles' X section makes random voltages, and three knobs shape them. Spread runs from one
value, through a bell narrowing round it, to any value as likely at noon, and on to the two ends
alone. Bias leans them low or high. Steps quantizes them to a scale, and turned further lets
fewer of its degrees through, the most important first: the root, then the fifth, then the
third. Vermona's meloDICER has a fader for each semitone, how often it comes. `Random` has all
of these: `Spread`, `Bias`, and on the Note column `Steps` and `Weights`.

**How the song does it.** Four tracks in D dorian, three of them live `Random`s:

| Track | Column | Shape |
|---|---|---|
| `X1` | Note | `D-3` to `D-5` on every other row of two bars, `Spread` `40%`, `Steps` 4 |
| `X2` | Note | `D-1` to `D-2` on `Euclid` 7 of 16, `Spread` `100%`, `Bias` `-40` |
| `HH` | Velocity | `30` to `7F` over 16 hats, `Spread` `25%`, `Bias` `+50` |

`Steps` 4 lets through dorian's root, fifth, third and seventh, D, A, F and C, so the tune is
always on a Dm7, and the bell keeps it near the middle, round `D-4`, with a leap now and then.
The bass has only its two ends, an octave apart, the low one seven notes in ten. The hats'
velocities gather toward the top, loud but never the same twice. Then a kick on every beat.
[`song.ron`](songs/marbles/song.ron)

**Build it.**

1.  `:set scale=dorian`, `:set root=d` and `:bpm 112`.
2.  The tune, on track 1: `Space 5`, `K` `string` `Enter`, `:name X1`. `Space 3`, `:len 32`,
    then `vG` on the Note column and `S`. On `Fill`, `K` `random` `Enter`. `From` `d3` and `To`
    `d5` (`i`, the note, `Enter`), `Spread` `40` and `Steps` 4 (`i` and the number, `Enter`),
    `Where` `Every` with an `Amount` of 2, and `L`. The panel says `4 of 7: D A F C`.
3.  The bass: `W`, on its Sound page `K` `va` `Enter`, `:name X2`, and in the phrase view `vG`,
    `S`, `Fill` `Random`, `From` `d1`, `To` `d2`, `Spread` `100`, `Bias` `-40`, `Where` `Euclid`
    with 7 hits, and `L`.
4.  The hats: `W`, `K` `/hat` `Enter`, `:name HH`, and a `C-4` on all 16 rows. `l` to the
    Velocity column, `gg`, `vG`, `S`, `Fill` `Random`, `From` `30`, `To` `7F`, `Spread` `25`,
    `Bias` `50`, and `L`.

Then a `Kick` on every beat. The finished song also sets the string's `Decay` and `Bright`, the
bass's filter, the delay and reverb sends, and the levels.

**As a patch.** The tune is the shipped patch `Marbles X`, with the module's `Spread`, `Bias`
and `Steps` on macros; add it with `:patch`.

**Try next.**

*   `gl` on the tune and `-` on `Steps`: at 3 it's a D minor arpeggio, at 1 only Ds, an octave
    line. All 7 is dorian again.
*   `Steps` back to all 7, then `Weights`, `i` `F4F4F4F` `Enter`: a hex digit a degree from D,
    so all of dorian plays, but the chord's four notes most of the time, as on the meloDICER.
*   `Spread` `0%` on the tune: one note, `D-4`, the middle of the range. With `Bias` `+50`, the
    `A-4` three quarters of the way up.
*   On the Gen page, `Space 8`, `Spread` and `Bias` are cells to turn while it plays, and with
    `M` a macro: map both to one, and an LFO on it sweeps the tune from a drone to a scatter.
*   `Vary` `Each pass` on the bass, and every pass draws a new pattern of octaves.

## Marbles' T

Marbles' T section is a random clock: its gates fire at random, and its models say how. In
*complementary Bernoulli*, one coin a tick decides which of two outputs fires, so they never fire
together; in *clusters*, hits bunch together, then leave gaps; in *Markov*, each tick's chance
hangs on what fired before, leaning toward what fired eight ticks back; and in *three states*,
each tick is one output, the other, or neither. One knob, Bias, shapes each model. These four are
`Where` choices, each with an `Amount`: `Complementary` across two tracks, `Clusters`, `Markov`,
and `3 states`, where on one track the two outputs are a gate and a tie, a note held into the
next.

**How the song does it.** Five tracks in A minor, each a live generator writing notes of its own
on 16 rows:

| Track | Where | Fill |
|---|---|---|
| `HH` | `Complementary` `65%`, `First` | `C-4` on every row it plays |
| `PC` | `Complementary` `65%`, `Second` | `Random`, `A-3` to `A-4` |
| `BD` | `Markov` `90%` | `C-1` on every row it plays |
| `BS` | `3 states` `80%` | `Random`, `A-1` to `A-2` |
| `ST` | `Clusters` `35%` | `Random`, `A-3` to `A-5` |

The hat and the modal percussion share one coin, so every sixteenth is one's or the other's,
never both, the hat on about two in three. The kick's Markov at `90%` keeps to an eight-row loop,
here the same twice; lower, the second eight rows stray further from the first. The bass's three states at `80%` are mostly rests and ties,
so it holds its notes, `~` below for a note held into the next (its gate `HLD`). The string plays
in bunches of two to four. [`song.ron`](songs/marbles-t/song.ron)

```text
HH  x.xxx..x..xxx.x.
PC  .x...xx.xx...x.x
BD  x..x...xx..x...x
BS  ...xx~x...~~x...
ST  .xxxx..xxx......
```

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 120`.
2.  Tracks 1 to 5: on each Sound page, `Space 5`, the machines `/hat`, `/modal`, `/kick`, `va` and
    `string` (`K`, the name, `Enter`), named `HH`, `PC`, `BD`, `BS` and `ST` with `:name`.
3.  The hat and the percussion: on track 1, `Space 3`, `vG` on the Note column, `W` to reach
    track 2, and `S`. `From` and `To` `c4` (`i`, the note, `Enter`). `j` down to `Where`, `K`
    `/compl` `Enter`: the panel says `Each row goes to one track or the other`. `j` to `Amount`,
    `i` `65` `Enter`, and `L`. Track 1 is the coin's `First` side and track 2 its `Second`.
4.  On track 2, `gl` on one of its notes reopens its own generator. On `Fill`, `K` `random`
    `Enter`, `From` `a3`, `To` `a4`, and `L`. Its rows stay the coin's.
5.  The kick: track 3, `vG`, `S`, both ends `c1`, `Where` `Markov` with an `Amount` of 90, and
    `L`.
6.  The bass: track 4, `vG`, `S`, `Fill` `Random`, `From` `a1`, `To` `a2`, `Where` `3 states`
    with 80, and `L`.
7.  The string: track 5, `vG`, `S`, `Fill` `Random`, `From` `a3`, `To` `a5`, `Where` `Clusters`
    with 35, and `L`.

Each `S` draws a seed of its own, so your rhythms differ from the song's; `r` in the panel draws
again. The finished song also sets the hat's and the kick's `Decay`, the string's, the bass's
filter, the sends and the levels.

**Try next.**

*   `gl` on the hat and `Amount` `30`, then the same on the percussion: it takes most of the
    rows now. Each generator keeps its own amount, so turn both, or map both to one macro.
*   `K` on the percussion's `Side` makes it `First`: it plays the hat's rows, together.
*   On the kick, `Amount` `100%` repeats its eight rows exactly; `33%` is a sparse coin; `0%` is
    busier, and never plays the same eight rows twice.
*   On the bass, `Amount` `0%` is mostly gates, short notes, and `50%` mostly rests.
*   `Vary` `Each pass` on the string, and every pass bunches its notes anew.
*   On the Gen page, `Space 8`, each `Amount` is a cell to turn while it plays, and on the Roto
    knob 7.

## Zularic Repetitor

Noise Engineering's Zularic Repetitor plays rhythms from around the world, an idea it took from
William Sethares' book *Rhythm and Transforms*. You pick a pattern, and its outputs play the
pattern's parts, with *children*: offshoots in which each beat of the pattern plays only by
chance, so a part varies round the pattern without losing it. Here that's a `Where`, `Rhythm`:
`Rhythm` picks the pattern, `Part` which of its three parts a track plays, `Bell`, `Low drum` or
`High drum`, and `Child` the chance each of the part's hits plays, `100%` the pattern as written.

The patterns are vimtracker's own, transcribed from Godfried Toussaint's papers on the clave and
bell timelines and from the sources each names, not taken from the module: the son and rumba
claves, the bossa nova, gahu, shiko, soukous and samba bells, the tresillo and the cinquillo, the
12/8 bembé bell and the motorik beat. A pattern plays a pulse a row, from the selection's first
row. The 16-pulse ones are a bar on a `1/16` track, the tresillo and the cinquillo half of one,
and the bembé's twelve pulses are a bar's triplet eighths, so it wants a track at `1/8T`, where
twelve rows are a bar, its four beats on the song's.

**How the song does it.** Five tracks in A minor at 100 BPM, each a live generator writing notes of
its own. The first three are the son: one `S` across them gave each the next part of `Son clave`,
the clave's bell, the bass's tumbao on the two offbeats of each tresillo, and the conga's open
tones at the end of each half, the conga a child at `80%`. The last two are at `1/8T`, twelve
rows: the bembé's bell, a child at `75%` that `Vary` makes anew each pass, and its four beats on
a kick. [`song.ron`](songs/world-rhythms/song.ron)

| Track | Rhythm | Part | Child | Fill |
|---|---|---|---|---|
| `CL` | `Son clave` | `Bell` | `100%` | `E-5` on every row it plays |
| `BS` | `Son clave` | `Low drum` | `100%` | `Alternate`, `A-1` and `E-2` |
| `CG` | `Son clave` | `High drum` | `80%` | `Alternate`, `E-3` and `A-3` |
| `BL` | `Bembé` | `Bell` | `75%`, `Vary` `Each pass` | `C-4` |
| `BD` | `Bembé` | `Low drum` | `100%` | `C-1` |

```text
CL  x..x..x...x.x...
BS  ...x..x....x..x.
CG  ......xx.......x
BL  ..x.xx...x..        the bell, x.x.xx.x.x.x, a child
BD  x..x..x..x..
```

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 100`.
2.  Tracks 1 to 5: the machines `modal`, `va`, `modal`, `hat` and `kick` (`K`, the name, `Enter`),
    named `CL`, `BS`, `CG`, `BL` and `BD` with `:name`. On tracks 4 and 5, `/speed` `Enter` and
    `+` once, to `1/8T`, then `Space 3` and `:len 12`.
3.  The son: on track 1, `Space 3`, `vG` on the Note column, `W` `W` to reach track 3, and `S`.
    `From` and `To` `e5` (`i`, the note, `Enter`). `j` down to `Where`, `K` `/rhythm` `Enter`: it
    starts on `Son clave`, and the panel says `Each track plays the next part`. `L`.
4.  On track 2, `gl` on one of its notes reopens its own generator: `Fill` `Alternate`, `From`
    `a1`, `To` `e2`, and `L`. On track 3, the same with `e3` and `a3`, then `Child` `80` and `L`.
5.  The bembé: track 4, `vG`, `W`, `S`, `Where` `Rhythm`, then `Rhythm`, `K` `/bemb` `Enter`. On a
    `1/16` track the panel would say `Its 12 rows make a bar on a 1/8T track`. `From` and `To`
    `c4`, `L`.
6.  On track 4, `gl`: `Child` `75` and `Vary` `Each pass`, and `L`. On track 5, `gl`: `From` and
    `To` `c1`, and `L`.

A child draws from the seed, so yours drop other hits than the song's; `r` in the panel draws
again. The finished song also sets the voices' decays, the bass's filter, the sends and the
levels.

**Try next.**

*   `gl` on the clave and `Rhythm` `Rumba clave`: its third stroke moves a row later. The bass and
    conga are generators of their own, so they keep the son's parts; `S` across all three again
    sets them together, `Rumba clave`'s drums being its beat and offbeats.
*   On the conga, `Child` `100%` gives back its four open tones, and `40%` leaves fewer, which
    ones hanging on the seed.
*   `Vary` `Each pass` on the conga too, and it plays a new child each bar round the bembé's.
*   `Rhythm` `Motorik` across the first three tracks makes its hats, kick and snare, Neu!'s beat,
    if the voices are a hat, a kick and a snare.
*   On the Gen page, `Space 8`, `Child` is a cell at the foot of the third column, and on the
    Roto knob 7; a macro or an LFO on it thins a part and fills it in again as it plays.

## Variations of your own phrase

Write a line, then ask for another one like it. A Markov chain does that by
counting: which note came after which, and how often, so it can write a new line
that only ever takes steps the old one took. aicd, a Max for Live device, writes
acid lines this way from n-grams of hundreds of 303 patterns, with a temperature
dial for how adventurous it gets. Here the only pattern it learns from is yours:
the rows you select, their rests, accents and slides with the notes.

**How the song does it.** Track 1, an `Acid` in C minor, has a 16-row line
written by hand, and a live `Variation` over it at `Order` 1 and `Temp`
`50%`, with `Vary` on `Every 4 passes`, under a kick, hats and a snare. What
plays is a variation; the line you wrote is under it, which the generator learns
from every time. [`song.ron`](songs/variations/song.ron) The line, its accents
`7F` and slides `HLD`:

```text
C-2 7F   C-2 50   C-3 HLD  C-2 50   ---      D#2 50   C-2 7F   G-2 HLD
A#1 50   ---      C-2 50   C-3 7F   ---      G-2 HLD  D#2 50   C-2 50
```

**Build it.**

1.  `:set scale=minor` and `:bpm 124`.
2.  Track 1: `K` `acid` `Enter`, `:name TB`, and the line above in the phrase view:
    `'` accents a note and `/` makes it slide.
3.  `gg` `vG` on the Note column and `S`. On `Fill`, `K`, `/variation`, `Enter`.
    The panel's heading counts the notes it wrote, and the line changes.
4.  `r` until you like it, and `L` to keep it live.
5.  `Space 8` for the Gen page, and on the device's title, `K` on `Vary`, `Every
    4 passes`.

Then a `Kick` on every beat, `Hat`s on the off-beats and a `Snare` on rows `04`
and `0C`, as in `acid-lines`.

**As a patch.** The `Variation` is the shipped patch `Variations`, with `Order` and
`Temperature` on macros; add it with `:patch` to a track with a line of at least four notes for
it to learn from.

**Try next.**

*   `gl`, then `Order` 3 and `Temp` 0: the line you wrote comes back, as
    every three rows of it are followed by one row only. In a bar this short
    `Order` 2 is the same, as no two rows of it come twice in a row; `Order` 1
    takes it apart and puts it together another way.
*   `Temp` `100%`: every row that ever followed is as likely, so a slide
    the line has once comes as often as a note it has five times.
*   `dd` on the Gen page takes the generator away and gives the line back, to
    edit; `Space g a` instead keeps the variation as plain notes.
*   On a longer line, two bars or four of a melody that comes back to its
    figures, `Order` 2 gives phrases that sound like it without repeating it.
*   A line with fewer than four notes is too little to learn from, and the panel
    says so.

## René

[Make Noise's René](https://www.makenoisemusic.com/modules/rene) is a sequencer of sixteen
places on a 4×4 grid, each with a pitch. Its X and Y channels walk the grid along "snakes",
paths that wind round it or jump, one place on each clock. Its Cartesian channel is moved across
by the X clock and down by the Y clock, so the pattern is made by the two clocks: on the same
clock it moves on the diagonal, and with Y slower than X it plays a line, steps down, and plays
the next line from wherever X has got to. Lay a chord along each line and X picks the chord's
notes while Y moves on through the chords.

**How the song does it.** Track 1, a plucked `VA` in D minor, is sixteen rows, which
vimtracker reads as René's 4×4: each line of four a chord, Dm7, B♭maj7, Fmaj7 and C7, its
first note accented. `:order cartesian 1 3` moves X along a line every row and Y down the lines
every three, so the line changes a step before X wraps: `1 2 3 8 5 6 11 12 9 14 15 16`, twelve
steps that drift across the bar of sixteen. Track 2, an `Acid` bass, is eight rows, two lines of
four, the second an octave above the first; `:order snake columns` reads them down the columns,
so each note is followed by its octave. Then a kick on the beat and hats off it.
[`song.ron`](songs/rene/song.ron)

```text
Track 1, a grid of 4×4                    Track 2, a grid of 4×2
00 D-3 7F  01 F-3 50  02 A-3 64  03 C-4 50     00 D-1 7F  01 D-1 64  02 A#0 64  03 C-1 64
04 A#2 7F  05 D-3 50  06 F-3 64  07 A-3 50     04 D-2 64  05 F-1 64  06 A#1 7F  07 C-2 64
08 F-3 7F  09 A-3 50  0A C-4 64  0B E-4 50
0C C-3 7F  0D E-3 50  0E G-3 64  0F A#3 50
```

**Build it.**

1.  `:set scale=minor`, `:set root=d` and `:bpm 112`.
2.  Track 1: `K` `va` `Enter`, `:name RN`. In the phrase view, the sixteen notes and
    velocities, a line of four rows a chord as above.
3.  `:order cartesian 1 3`. The phrase's title says `Cartesian 1 3`, and the playhead jumps
    down a line every third step.
4.  Track 2: `W`, `K` `acid` `Enter`, `:name BS`, `:len 8`, and its eight notes.
5.  `:order snake columns`, or `:order snake 2`. The title says `Snake columns`.

Then a `Kick` on every beat and a `Hat` on the off-beats. The finished song also makes the `VA` a
pluck (`Mod 1`'s `Decay` `140 ms` and `Sustain` `20%`, with `Cut` `90`) sent to the delay and
the reverb, and sets the `Acid`'s `Cutoff` `48`, `Resonance` `A0` and `Env mod` `90`.

**Try next.**

*   `:order cartesian 1 1` on track 1: X and Y move together, down the diagonal, the first note
    of the first chord, the second of the next, and so on, a four-note line.
*   `:order cartesian 1 4`: Y moves as X wraps, so the grid plays forward. With `1 5` the line
    changes a step after X wraps, and with `1 2` twice a line.
*   `:order cartesian 2 3 4 2`: X on every other row over the whole line, Y over the first two
    chords only.
*   Other snakes on track 1: `:order snake spiral` plays the first chord, the last notes of the
    others, the last chord backwards, and in to the middle; `checker` plays every other note, then
    the rest.
*   `:order` alone lists every order, each snake and Cartesian with what it plays in a phrase of
    16.

## Bloom

[Qu-Bit's Bloom](https://www.qubitelectronix.com/) calls itself a fractal sequencer.
You write a sequence, the trunk, and its `Branches` knob grows up to seven more, each a
variation built from the one before it, its `Mutation` knob setting how much each changes. So
the first branch is nearly the trunk and the seventh has wandered off, and `Path` chooses the
route through them: in order, back to the trunk between each, or around and back.

**How the song does it.** Track 1, a plucked `VA` in D dorian, has a bar written by hand, the
trunk, phrase `00`. `:bloom 4 2 trunk` grew four branches from it, phrases `01` to `04`, each a
copy of the trunk whose Note column follows the branch before it live, with `Mutation` at two
changes: a note moved a degree or two, a few rows played backwards, a ratchet, a note dropped or
added. Its chain plays the trunk before each branch, `00 01 00 02 00 03 00 04`: eight bars that
stray further each time and come home between. An `Acid` bass, a kick and hats keep it steady.
[`song.ron`](songs/bloom/song.ron)

```text
The trunk, phrase 00                        Branch 4, phrase 04
00 D-3 7F  04 ---     08 G-3 7F  0C ---      00 ---     04 E-3 64  08 G-3 7F  0C ---
01 ---     05 C-4 50  09 ---     0D D-3 50   01 D-3 7F  05 C-4 50  09 ---     0D D-3 50
02 A-3 50  06 A-3 64  0A E-3 50  0E C-3 64   02 F-3 50  06 ---     0A G-3 50  0E C-3 64
03 F-3 64  07 ---     0B F-3 64  0F ---      03 F-3 64  07 ---     0B F-3 64  0F D-3 64
```

By the fourth branch the first note has moved a row late, the `A-3`s have gone, an `E-3` and a
last `D-3` have come in, and the `D-3` on row `0D` is ratcheted with `RET 02`, three hits in
its step, which it took from branch 2.

Each branch is still a live column, so an edit to the trunk grows all four again, in one undo
step: change the trunk's `G-3` on row `08` and every branch plays the new note, except where its
own changes fall. The branches' titles say `󰁨 Note←T01:00`, `󰁨 Note←T01:01` and so on, and the
trunk's `Followed by 1`.

**Build it.**

1.  `:set scale=dorian`, `:set root=d` and `:bpm 116`.
2.  Track 1: `K` `va` `Enter`, `:name BL`, and the trunk above in the phrase view.
3.  `:bloom 4 2 trunk`: four branches, two changes each, the trunk before each. The status line
    names the new phrases, and `Space 2`, the chain view, shows the path. Your branches differ
    from the song's, as each bloom draws its own seed: `u` and `:bloom 4 2 trunk` again for
    others, until you like them.

Then, on tracks 2 to 4, an `Acid` bass on `D-1` (`X..x..x...x..x..`), a `Kick` on every beat
and a `Hat` on the off-beats. The finished song also makes the `VA` a pluck (`Mod 1`'s `Decay`
`220 ms` and `Sustain` `25%`, with `Cut` `88`) sent to the delay and the reverb.

**Try next.**

*   `:bloom` alone, on the trunk's row of a new chain, lists the paths: `pendulum` goes out to the
    last branch and back, and `shuffle` plays the branches in an order of its own.
*   `gl` on a branch's notes, or `Space 8` for the Gen page, then `Mutation` up to `6`: that
    branch strays further, and the ones after it with it. Turn `Shift` on and the branch moves
    up or down the scale as a whole, as Bloom's `Root` would.
*   On the Gen page, `Vary` `Each pass` on branch `04`: it mutates again every time round the
    eight bars, so the last branch is always new.
*   Map a macro to `Mutation` on the Gen page with `M`, and one knob turns every branch at once.
*   `Space g a` on a branch keeps it as plain notes, to edit by hand; it stops following.

## Shuffling Clock Multiplier

[4ms' Shuffling Clock Multiplier](https://4mscompany.com/) takes one clock and gives eight
outputs at one to eight times its rate. `Shuffle` pushes some of their pulses early and `Slip`
pushes them late; `Skip` drops beats in a pattern, so an output plays more of a pattern than a
clock; and a voltage at its `Rotate` jack passes the multiplications round the outputs, so each
output takes the next one's rate and the drums they play trade places.

Each track here has its own clock already, its `Speed`, and a groove does what `Shuffle` and
`Slip` do. Rotate is `Space o`, which passes the speeds round a group's tracks, and Skip is a live
column of `CHA 00`, a chance that never comes up, on the rows a seeded `Chance` picks.

**How the song does it.** Tracks 1 to 3, in group 1, are three clocks in A minor: a `Modal` bell at
`1/16`, a plucked `VA` at `1/8` and a `VA` bass at `1/8T`, eight rows each. A kick keeps the beat,
and a hat plays every sixteenth, its FX column a live `Chance` of `CHA 00` at `35%` that drops
five of them, the same five every bar. The hat's first macro, `Skip`, holds that amount.
[`song.ron`](songs/clock-multiplier/song.ron)

```text
    00  01  02  03  04  05  06  07  08  09  0A  0B  0C  0D  0E  0F
HH  X   x   .   x   X   x   x   .   X   .   .   x   X   x   .   x       . is CHA 00
```

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 118`.
2.  Track 1: `K` `modal` `Enter`, `:name BL`, and eight rows of a tune, such as
    `A-4 C-5 E-5 --- G-4 E-5 D-5 ---`, with `:len 8` on the Note column for a phrase of eight.
3.  Track 2: a `VA`, `:name PL`, eight rows such as `A-3 E-3 C-4 G-3 A-3 D-4 E-3 C-4`, and
    `/speed` `Enter`, then `K` to choose `1/8` from the list.
4.  Track 3: a `VA`, `:name BS`, eight rows of bass on `A-1`, `E-1` and `G-1`, at `1/8T`.
5.  `:group 1 1-3` puts the three in group 1.
6.  A `Kick` on every beat, and on track 5 a `Hat` on every row. In the hat's phrase, `l` to its
    first FX column, `vG` and `S`, and choose `CHA`. Set `From` and `To` to `00`, `Where` to
    `Chance` and its `Amount` to `35%`, then `L`.
7.  `Space 8` on the hat, `M` on the generator's `Amount`, and `:macro Skip` to name it.

**Play it.**

*   `Space o` on any of the first three tracks: the bell takes the eighths, the pluck the
    triplets and the bass the sixteenths. Again, and they move on; a third time, and they're
    back. Each press is heard from each track's next step, and `u` takes one back.
*   `Space r`, then `Space o` on a bar line and again two bars later: the turns go in as held
    `RATE`s on the rows where you pressed, so the loop plays them back each time round.
*   Turn `Skip` up while it plays: the hat drops more steps, keeping the ones already gone, until at
    `100%` it's silent; down to `0%` it's a clock again. `r` on the Gen page draws other steps.
*   `Where` on `Euclid` with `Hits` as the amount spreads the dropped steps evenly instead.
*   Give the group a groove of its own, `Swing 62` on each track's `Groove`, for the SCM's
    `Shuffle`.

## Cellular automata

Stephen Wolfram's elementary cellular automata are a row of cells, each on or off, that makes
its next generation from itself: each cell looks at itself and its two neighbours, and a rule
says, for each of the eight ways those three can be, whether the cell is on next. Write the
eight answers as bits and the rule is a number from 0 to 255. Rule 30 is chaotic, rule 90 draws
Sierpiński's triangle, 110 is complex enough to compute with, and 184 moves its cells along like
cars in traffic. dan derks' *less concepts*, a script for monome's norns, plays them as a
sequencer: a seed row and a rule, and each new generation is the next step's notes.

`Cells` is a `Where`, so it chooses the rows any fill writes on, in two ways. On one column, each
row is a cell, and with `Vary` each pass plays the next generation. Across tracks, each track is
a cell and each row a generation, so the tracks play the automaton's picture down the phrase,
and each pass carries on from the last row. The row is a ring: the first cell's left neighbour
is the last.

**How the song does it.** Track 1 is a plucked `String` whose rows are rule 30's cells, a
`Random` fill of notes between `A-3` and `A-5` on them. It starts from one cell, in the middle
of its sixteen rows, with `Vary` `Each pass`, so the first bar plays one note and each bar after
it the next generation, growing until it's chaos. Tracks 2 to 6, a hat, a rimshot, a woodblock,
a shaker and a low tom, are the five cells of rule 26, from one cell, the woodblock's, each row a
generation, with `Vary` on too. Its picture goes round every twenty generations and each bar
carries on from the last, so the beat comes back to the first bar's every five bars. A written
kick and bass keep time. [`song.ron`](songs/cells/song.ron)

```text
The lead, a pass a line       The five across, a row a line, the first pass
pass 1  ........x.......      00 ..x..    04 ...x.    08 ....x    0C x....
pass 2  .......xxx......      01 .x.x.    05 ..x.x    09 x..x.    0D .x..x
pass 3  ......xx..x.....      02 x...x    06 xx...    0A .xx..    0E ..xx.
pass 4  .....xx.xxxx....      03 .x.xx    07 x.x.x    0B xx.x.    0F .xx.x
```

Read down the five columns of the second picture and each is one track's rhythm: the hat, the
first, plays rows `02`, `06`, `07`, `09`, `0B` and `0C`.

**Build it.**

1.  `:set scale=minor`, `:set root=a` and `:bpm 112`.
2.  The lead, track 1: `Space 5`, `K` `string` `Enter`, `:name LD`. `Space 3`, `vG` on the Note
    column, and `S`. On `Fill`, `K` `random` `Enter`; `From` `i` `a3` `Enter` and `To` `i` `a5`
    `Enter`. On `Where`, `K` `cells` `Enter`: the panel says `Each row a cell, stepped on
    by Vary`. `Rule` `i` `30` `Enter`, `Start` `K` `one` `Enter`, and `Vary` `K`
    `each` `Enter`. `L` keeps it live.
3.  The five: on tracks 2 to 6 the machines `hat`, `snare`, `modal`, `dust` and `modal` (`K`
    `/hat` `Enter` and so on), named `HH`, `RS`, `WB`, `SH` and `LT`.
4.  On track 2, `Space 3`, `vG` on the Note column, `W` four times to reach track 6, and `S`. The
    panel says `Each track a cell, a row a generation`. `From` and `To` `c4`, `Where` `Cells`,
    `Rule` `26`, `Start` `One` and `Vary` `Each pass`, then `L`: a generator on each track, each
    its own cell.
5.  Each its own note: on the rimshot's rows, `gl`, `From` and `To` `c3`, and `L`; the same with
    `e4` for the woodblock, `c5` for the shaker and `a2` for the tom. Its rows stay as they were,
    as the note doesn't change them.
6.  Tracks 7 and 8: a `Kick` on every beat, and a `VA` bass on `A-1`, `E-1`, `C-2` and `G-1`.

All five start from one row worked out for all of them as `S` shapes them, so they play one
automaton, and a track's `gl` has `Rule` but no `Start`. `Seed` would draw that row from the
seed, which `r` rolls again, and `Hits` would take it from which tracks have a note on the first
row.

**Try next.**

*   On the lead, `gl` and `Rule` `110`: it grows to the left only. `90` draws Sierpiński's
    triangle, and on a ring of sixteen dies out after eight generations, so its bars thin to
    nothing; `184` slides its one note a row along each bar.
*   `Start` `Seed` on the lead: it starts from cells drawn from the seed, about half of them on,
    and `r` rolls another start.
*   `Vary` `Every 4 passes` holds each generation for four bars. `Once` stops it: the first pass
    plays every time.
*   On the Gen page, `Space 8`, `Rule` and `Start` are at the foot of the third column, where
    Euclid's `Hits` and `Rotate` go: map a macro to `Rule` with `M`, and an LFO on the macro
    goes through the rules as it plays.
*   `S` across the five again with `Rule` `30`: chaos down the kit. `gl` on one track and a new
    `Rule` takes that track alone to another automaton, as each is a generator of its own.
*   `Space g a` keeps the rows as last heard, as plain notes, to edit by hand.

## The Tonnetz

The Tonnetz, Euler's "tone network", lays the twelve notes out in a lattice: fifths one way,
major thirds another, minor thirds the third, so every triangle is a major or minor triad. Hugo
Riemann's harmonic theory, taken up by David Lewin and Richard Cohn as neo-Riemannian theory,
moves between neighbouring triangles by three moves, each keeping two of a triad's notes and
moving the third by a semitone or a tone:

| Move | C major becomes | What moves |
|---|---|---|
| `P`, parallel | C minor | The third, E to E♭ |
| `L`, leading-tone exchange | E minor | The root, C down to B |
| `R`, relative | A minor | The fifth, G up to A |

Chains of them make the progressions of late Romantic and film music: `P` and `L` in turn go
round six chords and back, `P` and `R` round eight, and `L` and `R` walk the circle of fifths a
third at a time. Ornament & Crime's Harrington 1200 applies them to a triad on triggers, and its
Automatonnetz walks a grid of them as a vector sequencer, after Noise Engineering's Tonnetz
Sequent; Live 12's Stacks writes chord progressions in a scale.

`Tonnetz` is this: from the chord on `From`, each row `Where` chooses plays the next chord, one
move on. The moves are a list you type, such as `P L R L`, going round, or drawn from the seed by
a weight for each, each row afresh or as a loop of so many. `Voice` says what a track writes of
each chord: on the `Chord` voice the root with the chord locked, and across three tracks a voice
of the triad each, so plain mono voices play the progression between them, one note moving at a
time.

**How the song does it.** One walk, `L R L R R L R L` from C major in G major, played three
ways. Track 1, a pad on the `Chord` voice at `1/2`, has a chord each half note: each row is the
chord's root with `CHRD` locked to `M` (`0A`) or `m` (`03`) in its first FX column.
Tracks 2 to 4, plucked `String`s, are the three voices across, each on a Euclidean eight of 64
sixteenths, the middle voice two rows and the top four rows after the low, so each chord rises
as an arpeggio. Track 5, a `VA` bass at `1/2`, plays the roots. A kick and hats keep time.
[`song.ron`](songs/tonnetz/song.ron)

```text
         C    Em   G    Bm   D    Bm   G    Em
CH       C-3  E-3  G-2  B-2  D-3  B-2  G-2  E-3
CHRD     0A   03   0A   03   0A   03   0A   03
HI       G-4  G-4  G-4  F#4  F#4  F#4  G-4  G-4
MD       E-4  E-4  D-4  D-4  D-4  D-4  D-4  E-4
LO       C-4  B-3  B-3  B-3  A-3  B-3  B-3  B-3
BS       C-1  E-1  G-0  B-0  D-1  B-0  G-0  E-1
```

Read the three strings across: from each chord to the next, one of them moves, by a semitone or
a tone, and the other two hold. `L` takes C's root down to B, `R` takes E minor's root down to D,
`L` again takes G's root down to F♯, and the walk climbs to D major and comes back down the same
way, so the last chord leads into the first. The walk keeps to G major by itself: all eight are
the key's chords, IV, vi, I, iii and V.

**Build it.**

1.  `:set scale=major`, `:set root=g` and `:bpm 92`.
2.  The pad, track 1: `Space 5`, `K` `chord` `Enter`, `:name CH`, and on the Play device
    `/speed` `Enter`, `=` up to `1/2`. In the phrase view, `:len 8`, a chord each half note
    for four bars, then `vG` on the Note column and `S`. On `Fill`, `K` `/tonnetz` `Enter`: on the `Chord` voice its `Voice` is
    `Chord`, and the last line names the chords, `C Cm G# Fm C# C#m A F#m` for `P L R L`.
3.  `From` `i` `c3` `Enter`; `List` `i` `lrlrrlrl` `Enter`, and the last line says
    `C Em G Bm D Bm G Em`; `Scale` `i` for `In scale`. `L` keeps it live.
4.  The strings: tracks 2 to 4 `K` `string` `Enter`, named `LO`, `MD` and `HI`, each with a
    phrase of 64 rows, `:len 64` (see **The loop**).
5.  On track 2, `Space 3`, `vG` on the Note column, `W` twice to reach track 4, and `S`. `Fill`
    `Tonnetz`: the last line starts `A voice a track`. `From` `c4`, `List` `lrlrrlrl`, `Scale`
    `In scale`, `Where` `Euclid` with `Hits` `8`, and `L`: each track a generator of its own,
    `Low`, `Mid` and `High`.
6.  The arpeggio: on track 3, `gl`, `Rotate` `2`, `L`; on track 4, `Rotate` `4`.
7.  Track 5, the bass: `K` `va` `Enter`, `:name BS`, `Speed` `1/2` and `:len 8`, then `vG`,
    `S`, `Tonnetz`, `From` `c1`, the same list and `Scale`, and `L`; its `Voice` is `Root`, as
    it isn't a `Chord` voice. Then a `Kick` on every beat and a `Hat` on the off-beats.

The finished song also sets the pad's wave, the sends and the levels.

**Try next.** Each track's generator is its own, so a change on one leaves the others on the
old walk: solo the track to hear it alone, or make the same change on each.

*   On the pad, `gl` and `List` `pl`: the hexatonic cycle, C, C minor, A♭, A♭ minor, E, E
    minor and back. With `Scale` `In scale` the `P`s are left out, so it rocks between C and E
    minor; `Scale` `Free` lets them through.
*   `List` `pr`, free: the octatonic cycle, eight chords a minor third apart in pairs, round to
    C again.
*   `Moves` `Random`: each chord a move drawn by the weights `P`, `L` and `R`, and `r` rolls
    another walk. `Vary` `Each pass` walks anew every time round. `Loop` with `Length` 3 draws
    three moves and goes round them.
*   `Triad` `Minor` starts from C minor, and the walk follows from there.
*   Across the strings, `S` again with `Where` `Every` `4`: a chord each beat, all three struck
    together, and over 64 rows the walk goes four times as far.
*   `Space g a` keeps any of them as plain notes and locks, to edit by hand.

## Strange attractors

Nonlinear Circuits' Sloth is a chaotic circuit so slow that one wander takes minutes, made to
nudge a patch that never quite repeats. Ornament & Crime's Low-rents runs two textbook
attractors, Lorenz's and Rössler's, side by side, with a knob for each one's chaos and a choice
of which axis goes out. A point on one goes round and round and never retraces its path: a
modulation that wanders, but smoothly, and keeps coming back near where it was.

Here they're two LFO shapes, `Lorenz` and `Rössler`. A cycle of `Rate` is about one turn, its
`Deform` is the chaos knob, and its `Phase`, shown as `Axis`, picks `x`, `y` or `z`.

**Try it on the `turing-machine` song.** Its tune comes from a Steps slot in `Mod 4` on `Pitch
in scale`; a `Lorenz` in its place writes a line that drifts without ever going round.

1.  `Space 6` for the Mod page, and `$` `l` to `Mod 4`'s title. `b` switches the Steps slot out.
2.  `}` down to `Mod 5`, an LFO going nowhere, and `l` `l` `l` to its `Dest`, then `K`
    `pitch in` `Enter`, for `Pitch in scale`.
3.  `j` `0` to its `Shape`, `K` `/lorenz` `Enter`. Its picture is the attractor seen side on,
    its two wings an S, and `Axis` reads `x`.
4.  `j` to its `Rate`, `K` `1 bar` `Enter`, and `j` to `Depth`, `i` `0C` `Enter`, for an octave
    either way.
5.  `Tab`. The tune circles a few notes for a bar or two, then flips to another register, as the
    point flips from one wing to the other.

The mode in its title is `Free`, so it runs on through the notes. `K` on the mode and `Hold`
holds where it is at each note instead: one note of the scale each, the rhythm as written.

**Try next.**

*   `+` on `Axis` for `y`, then `z`, and the picture becomes the butterfly: `z` never flips
    wings, it rises and falls once a turn, a line that climbs and drops back.
*   `Rössler` for a line that swells and shrinks round one loop. `Deform` down to `-FF` makes it
    a plain loop, the same few notes every bar; back up through `-80` it doubles to a loop of two
    bars, then four, then chaos.
*   A slower `Rate`, `4 bars`, on the `Acid`'s `Cut` rather than its pitch: a filter that
    breathes and never quite repeats.
*   When a pass is one you want, `Space 3` and `Space g A`: the notes it played are written in
    and the slot is switched out, as with the Steps slot.
