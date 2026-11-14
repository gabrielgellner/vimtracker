# Generative sequencers, recreated

Some of the best-loved sequencers don't play what you wrote: they play something near it, and
let you steer. This chapter builds twelve of them out of what vimtracker already has, each as a
short song you can open, play and pull apart: a loop of a bar or two per track, synths only, so
there are no samples to find.

| Song | After | The trick |
|---|---|---|
| `turing-machine` | Music Thing's Turing Machine | A Steps slot's `Mutate` on `Pitch in scale`, played from three macros |
| `deja-vu` | Mutable Instruments' Marbles | `Mutate` for new notes, `:order random` to replay old ones |
| `euclid` | Euclidean rhythms | `S`'s `Euclid`, kept live, with `Rotate` |
| `fugue` | Fugue Machine | One theme and three tracks following it live with `Follow`, each at its own `Speed` |
| `kria` | monome's Kria | Triggers, pitches, octaves and lengths on loops of different lengths |
| `metropolix` | Intellijel's Metropolix | Pulses and gates per row, `ACU`, `ARP` and a pendulum order |
| `live-generators` | Elektron's Euclidean mode | Columns kept live with `L`, and what hand edits do to them |
| `acid-lines` | Roland's TB-303 | `S`'s `Acid` fill: notes, accents and slides rolled together |
| `walks` | Max's `drunk` and Voss's 1/f music | `S`'s `Walk` and `1/f` fills, kept live |
| `euclid`, again | Noise Engineering's Numeric Repetitor | `S`'s `Multiply` on the kick's rhythm |
| `walks`, again | Xaoc Devices' Leibniz | `S`'s bit transforms on the tune and the hats |
| `grids` | Mutable Instruments' Grids | `S`'s `Grids` across the kick, snare and hat, one beat from a map |
| `marbles` | Mutable Instruments' Marbles' X, Vermona's meloDICER | `Random`'s `Spread`, `Bias`, `Steps` and `Weights` |
| `variations` | aicd's acid n-grams | `S`'s `Variation`: a Markov chain learned from the line itself |

Each section says what the original does, how the song does it, how to build it yourself from an
empty song, and which knob to turn next. The steps name the keys; **Selecting and editing**,
**The track page** and **FX commands** have the rest.

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

They're the repository's own copies, so save your changes somewhere new with `:w` and a folder,
such as `:w my-turing`, and then `:w` carries on saving there. To hear one without opening it,
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
`Min` and a `Max` of its own (see **Macros** in **The track page**). Three make the slot a panel,
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

Now `Mutate`, `Length` and `Depth` show where their macros put them, with how far each can go
drawn faint behind its bar, and won't turn on their own: `=` on one says to turn its macro
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

**A patch, later.** The Steps slot and the three macros that turn it are a Turing Machine you
could put on any track. Saving them as a device of their own, without the `Acid` under them, is
what `:patch save` will do once patches land
([#619](https://github.com/gabrielgellner/vimtracker/issues/619)); `:patch` would then add it to
another track, its macros and slot in whichever are free.

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
| `V3` | `VA` | `1/4` | `forward` | Inverted, an octave down: `Invert On`, `Transpose -7` |
| `V4` | `Modal` | `1/8T` | `forward` | Backwards, two rows late: `Reverse On`, `Rotate 2` |

With the scale on, `Transpose` counts the scale's degrees, so `+4` is a fifth in key and `-7` an
octave. `V3` plays the theme upside down as a bass line across two bars, while `V2` runs up and
down the answer four times a bar. `V4` goes round three times in two bars, backwards.
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
    name, and `/speed` `Enter`, then `=` for each step slower: twice for `1/8` on `V1`, four
    times for `1/4` on `V3`, once for `1/8T` on `V4`. `V2` stays at `1/16`.
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
Kria track, an `Acid` with a trigger loop of 12 and a pitch loop of 7, under a kick on every beat
and hats in threes. [`song.ron`](songs/kria/song.ron)

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

Track 2 is the same idea: an `Acid`, `:len 12`, `E-2`s on `x..x.xx..x.x`, and a `Steps` slot in
`Free` on `Pitch in scale` with seven values, `00 00 07 00 03 00 0A`. Then a `Kick` on every
beat, and a `Hat` with `:len 3` and three `C-4`s at velocities `60`, `20` and `38`.

The finished song also makes the `VA` a pluck (`Mod 1`'s `Decay` `300 ms` and `Sustain` `50%`),
sets its `Cut` `90` and `Res` `50`, sends it to the delay, sets the `Acid`'s `Cutoff` `48` and
`Resonance` `A0`, a shorter hat, and the levels.

**Try next.**

*   `Length` 7 on the pitch loop, or 4 on the octave loop: the line changes completely.
*   `:len 13` on track 1, a trigger loop that drifts against the bar too.
*   `Rate` `1/8` on the octave loop, so it turns over half as fast, as Kria's clock divisions do.
*   The lane's length: `:len 5` with the cursor on FX 1.

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
    and `=` twice for `1/8`. In the phrase view, `:len 4`, the four notes with `SGL4`, and
    `ARP` on each (`K` `arp` `Enter`, then `24`).
7.  Its `Arpeggio` device, beside the accumulator: `Rate` `1/32`, which on a track at `1/8` is a
    sixteenth, `Mode` `Up-down`, `Oct` `2` and `Retrig` `on`.

Then a `Kick` on every beat, and a `Hat` on the off-beats with a `MLT2` on row `0E`, a ratchet.
The finished song also sets the `Acid`'s `Cutoff` `50`, `Resonance` `C0`, `Env mod` `B0` and
`Decay` `50`, makes the `VA` a short pluck (`Mod 1`'s `Decay` `120 ms` and `Sustain` `20%`, with
`Cut` `88`), sends both to the delay, the `VA` to the reverb too, and sets the levels.

**Try next.**

*   Other orders on track 1: `:order pedal`, `converge`, `odd-even` or `random`.
*   On the `Accumulator` device, `Moves` `Track`: every row climbs together with each `ACU`.
*   `Reset` every 4 loops, so the line climbs for four bars and starts again.
*   Track 2's `ARP` to `13` or `35` for other shapes, `Mode` `Random`, or `Oct` 3.
*   Track 1 at `1/8`, the clock divided, under the same arpeggio.

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

**Try next.**

*   `gl`, `r`, `L` while it plays: a new line every bar, until one sticks.
*   `Pitches` `Keep` in the panel: `r` then rolls only the accents, slides and octaves, Roland's
    second die, and the notes stay.
*   `Density` up for a busier line, `Notes` 2 for a two-note pump, `Octave` up for more jumps.
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

## Generators still to come

These are planned, each its own issue, and this chapter grows a section as each lands:

| Issue | Generator |
|---|---|
| [#586](https://github.com/gabrielgellner/vimtracker/issues/586) | Marbles' rhythm models as `Where` choices |
| [#589](https://github.com/gabrielgellner/vimtracker/issues/589) | Cellular automata rhythms from Wolfram's elementary rules |
| [#590](https://github.com/gabrielgellner/vimtracker/issues/590) | Tonnetz chord movement: P, L and R moves |
| [#591](https://github.com/gabrielgellner/vimtracker/issues/591) | Per-attribute loop lengths after Kria |
| [#592](https://github.com/gabrielgellner/vimtracker/issues/592) | Strange attractors as LFO shapes: Lorenz and Rössler |
