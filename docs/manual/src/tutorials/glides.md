# Glides and slides: tracker `3xx` sounds

A tracker's `3xx` is how a line slides: the pitch walks from the note playing to the one on the
row instead of jumping there. vimtracker has no `3xx`, but it has the parts a mono synth has,
and between them they make every slide a tracker makes, and some it can't. This chapter builds
seven small songs, each a few rows on one synth, and says what to listen for in each and which
tracker command it stands in for. A table at the end puts them side by side.

The parts are the gate, `Mode`, `HLD`, `Glide` and `Glide mode`. What each does, and how they fit
together, is [How a note plays](../how-a-track-plays.md#how-a-note-plays-gate-mode-slides-glide-and-retrigger)
in **How a track plays**; this chapter only uses them. Its recipes say what goes in each field,
as the generators chapter's do, not the keys that get there:
[Navigation](../guides/navigation.md#moving-between-things) says how to reach a setting, and
[Selecting and editing](../guides/editing.md#values-and-small-edits) how to change a value.

| Song | Stands in for | The trick |
|---|---|---|
| `glide-speeds` | `3xx` on every row, at three speeds | `Mode` `Legato`, and `GLD` held at three times |
| `slide-into` | `3xx` on one note | `HLD` on the row before, `GLD` on the note's |
| `slide-run` | `3xx` then `300`, `300`, `300` | A `HLD` on each row of a run, and `GLD` chosen by the interval |
| `legato-slides` | A 303's slides | `Legato`, with some rows let go halfway by `LEN` |
| `glide-always` | A synth's portamento, always on | `Glide mode` `Always` |
| `glide-one-row` | `3xx` typed on the note's own row | `GMOD 01`, or `VMOD 01`, on that row |
| `acid-slide` | A TB-303's slide | `Acid`'s own `Slide`, on `HLD` |

## How `3xx` slides, and how a glide does

In ProTracker, FastTracker 2 and OpenMPT, `3xx` on a row with a note doesn't play that note.
The note already sounding carries on, and its pitch moves towards the new one at a speed: `xx`
steps on every tick of the row, in the tracker's own units, a sixteenth of a semitone in
FastTracker 2's linear mode. An effect lasts one row, so the next row needs `300` to carry on at
the same speed, and a row without it stops the slide where it is. The slide is a straight line,
and it stops when it reaches the note.

A glide here is a **time**, as a synth's portamento is: `Glide` on the track, or `GLD` on a row,
is how long the move takes, from 2 ms to 2 s, whatever the interval, so a semitone and an octave
both take 60 ms at `7E`. It's a lag, as a 303's slide is: quick at first and easing in as it
nears the note, halfway in pitch at about a quarter of its time. It's in ms rather than ticks,
so it sounds the same at any tempo.

So a `3xx`'s speed isn't one setting here, but it's one choice per row: give each slide a time
in proportion to its interval, and they all move at one speed. At FastTracker 2's default tempo,
125 BPM and speed 6, a tick is 20 ms, so `310` moves a semitone a tick, 20 ms a semitone. That's
`GLD 7E`, 60 ms, for a minor third, `GLD 91`, 100 ms, for a fourth, and `GLD B1`, 240 ms, for an
octave. `slide-run` does it.

A few `GLD` values to start from:

| `GLD` | Time | | `GLD` | Time |
|---|---|---|---|---|
| `40` | 11 ms | | `91` | 100 ms |
| `60` | 27 ms | | `A0` | 151 ms |
| `6F` | 40 ms | | `B1` | 240 ms |
| `7E` | 60 ms | | `C0` | 360 ms |
| `80` | 63 ms | | `D0` | 557 ms |

On the Play device, `Glide` takes a time in ms after `i`, and shows it so.

## The songs

They're in `docs/manual/src/tutorials/songs`, a folder each, and open as the generators
chapter's do: `just run docs/manual/src/tutorials/songs/slide-into` from a checkout, or `:e` and
the folder. Each is read-only; `:w` and a folder saves a copy to change. `Tab` plays.

Each is one track, `LD`, in C minor at 120 BPM, a step 125 ms long. All but the last are a `VA`
plucked on the Mod page, `Mod 1`'s `Decay` `250 ms` and `Sustain` `40%`, so a struck note is
heard: it climbs back to the top and falls again, where a note slid into carries on falling.
At the full `Sustain` a new track has, the only difference is the dip a struck note takes, two
thirds of a millisecond.

The rows below are the phrase grid's note, velocity, gate and the gate's length, then any FX;
`····` is one pulse of `SGL` and `··` the track's `Gate`, as in **How a track plays**.

## Portamento at three speeds

**Stands in for** `3xx` on every row: each note slides from the last and none is struck.

**How the song does it.** Track 1 is in `Mode` `Legato`, at the full `Gate` a `VA` starts with,
so every note is still held when the next comes and slides into it. One melody of eight notes,
two steps each, plays three times: slowly, `GLD C0`, about 360 ms; at a medium speed, `GLD 80`,
about 63 ms; and fast, `GLD 40`, about 11 ms. Each `GLD` is on the pass's first row with the
hold mark, `h`, so it lasts until the next one rather than ending at the next note, as a lock
does. [`song.ron`](songs/glide-speeds/song.ron)

```text
00 C-3 64     SGL2 ··  GLD C0h       slow
01 G-3 64     SGL2 ··
02 D#3 64     SGL2 ··
…
08 C-3 64     SGL2 ··  GLD 80h       medium
…
10 C-3 64     SGL2 ··  GLD 40h       fast
…
17 G-3 64     SGL2 ··                slides into 00
```

**Listen for** one long note bending through the melody, struck only when playback starts. The
slow pass never quite arrives: 360 ms is longer than a note's 250 ms, so each slide is cut off by
the next and the melody is a blur of swoops, as a small `xx` is. The medium pass lands each note
within about a third of its length, and the fast one is nearly a jump with a smear on its front.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`; on the Play device,
    `Mode` `Legato`.
*   **Phrase:** 24 rows, the melody `C-3 G-3 D#3 F-3 G-3 A#3 C-4 G-3` three times, each note
    with a gate of `SGL2`. `GLD C0` on row `00`, `GLD 80` on `08` and `GLD 40` on `10`, each with
    the hold mark, `h`.

**Try next.** Lower the track's `Gate` on the Play device to `50%`: every note is let go halfway,
the next is struck, and nothing slides, however long `GLD` is. That's the gate deciding.

## Slide into a note without striking it

**Stands in for** `3xx` on one row: the true tracker slide, where the note on the row is never
struck, only reached.

**How the song does it.** Track 1 is a `Mono` `VA` playing a struck bass line. Two notes are slid
into instead: the row before each has `HLD1`, which ties its note into the next row's, and the
row slid into has `GLD`, how long the move takes. [`song.ron`](songs/slide-into/song.ron)

```text
00 C-2 64     ···· ··
01 C-2 64     ···· ··
02 C-3 64     ···· ··
03 C-2 64     HLD1 ··                struck, then tied into 04
04 G-2 64     ···· ··  GLD 90        slides up in about 98 ms
05 C-2 64     ···· ··                struck
…
0A C-3 64     HLD1 ··
0B D#3 64     ···· ··  GLD 60        slides up in about 27 ms
0C C-2 64     ···· ··
```

**Listen for** the two notes with no attack of their own: `G-2` swoops up out of `C-2` and dies
away with it, where every other note pops back up. `D#3`'s slide is quick, a bend more
than a swoop.

`GLD` may go on either row, the one tied from or the one tied into, since a tie is one note.
On the row slid into, it's where `3xx` would be. With no `GLD`, the track's `Glide` times it,
and with that `Off` too, the tie jumps to the note with no attack, which is a slide at `3FF`.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`. `Mode` stays `Mono` and
    `Glide` `Off`.
*   **Phrase:** 16 rows, `C-2 C-2 C-3 C-2 G-2 C-2 A#2 C-2 C-2 C-2 C-3 D#3 C-2 G-2 A#2 G-2`.
    `HLD1` on rows `03` and `0A`, `GLD 90` on `04` and `GLD 60` on `0B`. In Insert, `/` after a
    note writes its `HLD1`, as a 303's slide is written.

## A slide that carries on

**Stands in for** `3xx` followed by `300` on the rows after: one slide carrying on through
several notes, one attack at its start, and a row without it ending the run.

**How the song does it.** A run of six notes, `C-3 D#3 F-3 G-3 C-4 C-3`, two steps each, has
`HLD2` on every row but the last, so each note ties into the next and only the first is struck;
the last, `SGL6`, ends the run as a row without `300` does. It plays twice. The first time, the
track's `Glide` of 60 ms times every move, a step of two semitones and the octave down alike.
The second time, each row's `GLD` is chosen by its interval at 20 ms a semitone, a speed, as
`310` slides. [`song.ron`](songs/slide-run/song.ron)

```text
00 C-3 64     HLD2 ··                struck
01 D#3 64     HLD2 ··                slides, 60 ms
02 F-3 64     HLD2 ··                slides, 60 ms
03 G-3 64     HLD2 ··                slides, 60 ms
04 C-4 64     HLD2 ··                slides, 60 ms
05 C-3 64     SGL6 ··                slides, 60 ms, and ends the run
06 C-3 64     HLD2 ··                struck
07 D#3 64     HLD2 ··  GLD 7E        3 semitones in 60 ms
08 F-3 64     HLD2 ··  GLD 6F        2 in 40 ms
09 G-3 64     HLD2 ··  GLD 6F        2 in 40 ms
0A C-4 64     HLD2 ··  GLD 91        5 in 100 ms
0B C-3 64     SGL6 ··  GLD B1        12 in 240 ms
```

**Listen for** the octave down at the end of each run. The first time it drops as fast as the
small steps climbed, each move the same share of the way 20 ms in, a glide's time. The second
time the small steps land quickly and the octave takes four times as long to fall, sliding as a
tracker does, at one speed.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`; on the Play device,
    `Glide` `60 ms`.
*   **Phrase:** 12 rows, the notes and gates above: `HLD2` on each row but the last of each
    run, `SGL6` on the last. In the second run, `GLD` on each row slid into, as above.

**Try next.** A speed of your own: at 10 ms a semitone, twice as fast, a minor third is 30 ms,
`GLD 65`, and the octave 120 ms, `GLD 98`.

## Slides only where the gate is held: `Legato`

**Stands in for** a 303's slides, and a mono synth's fingered legato: no command at all, the
notes' lengths decide.

**How the song does it.** Track 1 is in `Mode` `Legato`, with `Glide` at `80`, 63 ms. A note
arriving while the last is still held slides into it; a note after the last was let go is
struck. At the full `Gate` every note is held to the next, so `LEN 80` on some rows lets their
note go halfway through its step. Those are the notes before the strikes; the rows left at the
full length slide into the next. [`song.ron`](songs/legato-slides/song.ron)

```text
00 C-2 64     ···· ··  LEN 80        struck, let go halfway
01 C-2 64     ···· ··  LEN 80        struck, let go halfway
02 C-3 64     ···· ··                struck, held
03 D#3 64     ···· ··  LEN 80        slides, let go halfway
04 C-2 64     ···· ··                struck, held
05 G-2 64     ···· ··  LEN 80        slides
06 C-2 64     ···· ··  LEN 80        struck
07 A#2 64     ···· ··                struck, held
08 C-3 64     ···· ··                slides, held
09 G-2 64     ···· ··  LEN 80        slides
…
```

**Listen for** the pairs and threes: a struck note, then one or two that bend out of it without
an attack, then a short note and silence before the next strike. It's the same rule as a 303's,
which slides only where its gate is held into the next step.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`; on the Play device,
    `Mode` `Legato` and `Glide` `80`.
*   **Phrase:** 16 rows, `C-2 C-2 C-3 D#3 C-2 G-2 C-2 A#2 C-3 G-2 C-2 C-2 D#2 F-2 G-2 A#2`,
    with `LEN 80` on rows `00`, `01`, `03`, `05`, `06`, `09`, `0A`, `0C`, `0D` and `0F`.

A row's own length does the same as `LEN`, `SGL1 80` in the gate column, as does the track's
`Gate` lowered with `FF` on the rows to hold: see **How a note plays**.

**Try next.** `Glide` `Off`: the slides now jump to their note, still with no new attack, a
legato trill without a portamento.

## Every note glides, and every note strikes: `Glide mode` `Always`

**Stands in for** a mono synth's portamento switched on: each note glides from the last but is
struck as well. A tracker can't quite do this in one command, since `3xx` doesn't strike; it's
the slide of a synth whose glide is always on, tracker-ce's `Glide` and the Deluge's
portamento.

**How the song does it.** Track 1 is `Mono`, with `Glide mode` at `Always` and `Glide` at `90`,
98 ms. Every note is struck, its envelopes starting again, and its pitch glides from the note
still sounding. After an `OFF` and a rest the sound has died away, so the next note has nothing
to glide from and starts at its own pitch. [`song.ron`](songs/glide-always/song.ron)

```text
00 C-3 64     ···· ··                struck at its pitch
01 G-3 64     ···· ··                struck, gliding up from C-3
02 D#3 64     ···· ··                struck, gliding down from G-3
03 C-4 64     ···· ··
04 G-3 64     ···· ··
05 OFF --     ···· ··
06 --- --     ···· ··
07 C-3 64     ···· ··                struck at its pitch, after silence
08 C-3 64     ···· ··                the same pitch: nothing to glide
09 G-3 64     ···· ··                gliding again
…
```

**Listen for** a pluck on every note, each with a swoop in its front. Against `legato-slides`,
the attacks are all there and only the pitch slides.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`; on the Play device,
    `Glide` `90` and `Glide mode`, shown as `Glides` under `Mode`, `Always`.
*   **Phrase:** 16 rows, `C-3 G-3 D#3 C-4 G-3 OFF --- C-3 C-3 G-3 D#3 A#3 G-3 F-3 D#3 OFF`.

## One glide on one row: `GMOD`

**Stands in for** `3xx` typed on the row of the note it slides to, so that only that note glides.

**How the song does it.** `glide-always`' line, with `Glide mode` back at `Ties`, where struck
notes start at their pitch. Row `03` locks `GMOD 01`, `Always` for its own note, so `C-4` is
struck and glides there from `D#3`, the one swoop in the line. Row `0B` locks `VMOD 01`,
`Legato` for its own note: the note before is still held at the full `Gate`, so the line slides
from `D#3` up to `A#3`, gliding over `Glide`'s time and not struck at all. [`song.ron`](songs/glide-one-row/song.ron)

```text
00 C-3 64     ···· ··
01 G-3 64     ···· ··                struck at its pitch
02 D#3 64     ···· ··                struck at its pitch
03 C-4 64     ···· ··  GMOD 01       struck, gliding up from D#3
04 G-3 64     ···· ··                struck at its pitch
…
0A D#3 64     ···· ··
0B A#3 64     ···· ··  VMOD 01       slides up from D#3, not struck
0C G-3 64     ···· ··
```

**Listen for** the two swoops, one plucked and one not. `VMOD 01` on the note's own row is the
nearest thing to `3xx` there, a slide with no attack and the command where a tracker has it; it
only slides if the note before is still held, so give that one the full length. `GMOD 01` keeps
the attack. Either sits on one row, where a `HLD` sits on the row before.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `VA`; `Mod 1` `Decay` `250 ms`, `Sustain` `40%`; on the Play device,
    `Glide` `90`, and `Glide mode` left at `Ties`.
*   **Phrase:** `glide-always`' 16 rows, with `GMOD 01` on row `03` and `VMOD 01` on row `0B`.

## `Acid`'s own slide

**Stands in for** a TB-303's slide, with nothing to set: the 303's own.

**How the song does it.** `legato-slides`' line written as a 303 would have it, on an `Acid`.
`Acid` starts with its `Gate` at `50%`, so every note is let go halfway, and a slide is a `HLD`
on the row that slides away, as a 303's slide flag is: the gate is held into the next note, the
pitch moves over the voice's own `Slide`, 60 ms at `00`, and neither envelope starts again. An
accent, `7F`, is on each beat. [`song.ron`](songs/acid-slide/song.ron)

```text
00 C-2 7F     ···· ··                accented
01 C-2 64     ···· ··
02 C-3 64     HLD1 ··
03 D#3 64     ···· ··                slides
04 C-2 7F     HLD1 ··
05 G-2 64     ···· ··                slides
…
```

**Listen for** the same shape as `legato-slides`, with the 303's squelch: a slid note keeps the
filter's last sweep instead of starting a new one.

How it differs from the rest:

*   `Slide` lives in the voice, as the 303's slide circuit does, so it's its own lag rather than
    `Glide`'s. The track's `Glide`, when set, times the slides instead.
*   `Glide mode` is dimmed on `Acid` and does nothing: a 303 slides only where it's told to.
*   `Legato` on `Acid` changes nothing until a note is held into the next, since its own `Gate`
    lets every note go: so `HLD` is how it slides, as on a 303.
*   `Slide` goes from 60 ms to the Devil Fish's 360 ms, where `Glide` goes from 2 ms to 2 s.

**Build it.**

*   **Song:** C minor, 120 BPM.
*   **Track 1, `LD`:** `Acid`, `Cutoff` `50`, `Resonance` `C0`, `Env mod` `A0`, `Decay` `58`.
*   **Phrase:** `legato-slides`' 16 notes, `7F` on rows `00`, `04`, `08` and `0C`, and `HLD1` on
    each row that slides away, the ones `legato-slides` holds at the full length: `02`, `04`,
    `07`, `08`, `0B` and `0E`.

## Side by side

| You want | In a tracker or synth | Here |
|---|---|---|
| Slide to a note without striking it | `3xx` on its row | `HLD` on the row before; `GLD` on either for the time. Or `VMOD 01` on its row, the note before held |
| Carry the slide on | `300` on the next rows | `HLD` on each row of the run; a row without it ends the run |
| A slide's speed | `xx`, per tick | `GLD` per row, chosen by the interval: 20 ms a semitone is `310` at 125 BPM |
| Every overlapping note slides | A mono synth's legato | `Mode` `Legato`, notes let go by `LEN` where they should strike |
| A slide on one step | Elektron's slide trig, which slides from the trig before into its own | `HLD` on the row before, or `VMOD 01` on the row; a value's glide mark, `g`, slides an FX value to the next |
| Every note glides, and is struck | A synth's portamento at "Always" | `Glide mode` `Always`, and `Glide` for its time |
| One struck note glides | | `GMOD 01` on its row |
| A 303's slide | The slide flag | `HLD` on an `Acid`, which slides by its own `Slide` |
