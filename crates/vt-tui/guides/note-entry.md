# Note entry

Put the cursor on the Note column, as the status line shows, and press `i`.
The grid's frame and the cursor turn green, and the keyboard is now a piano,
laid out as in Ableton:

```
 w e   t y u   o        black keys
a s d f g h j k l   '   white keys, a is C, then accent
z x c v           /     octave and velocity, then slide
```

So in Insert, `j` is a note. **Each note plays and is written, and the cursor
stays on it**, so you can play another in over it, even while the song plays.
`;` turns on auto-jump: then each note moves on by the grid.

Holding a note key writes the note once and keeps it sounding until you let
go, or leave Insert. The note written is still as long as its gate, not as
long as you held it, unless you're recording: see **Recording while it plays**. `1` is written once too, however long it's held. Other
keys, such as `Backspace`, the arrows and the digits in other columns, repeat
while held. If the window loses focus while a note is held, it stops.

Each note key writes as it goes down, even while another is held, so keys
rolled one into the next are all written. Without auto-jump the last one down
wins the row; with it, each moves on. Every key held keeps sounding until you
let go, so a chord is heard, though a row holds one note.

| Keys | Does |
|---|---|
| `z` `x` | Octave down or up. The status line shows it as `Oct` |
| `c` `v` | Lower or raise the velocity written. The status line shows `Vel` |
| `1` | Note off |
| `'` | Accent the note just written, or held, the notes played |
| `/` | Slide from the note just written, or held, the notes played |
| `;` | Auto-jump on or off. The status line shows `Jump` |
| `Backspace` | Clear this note. With auto-jump, back a grid step first |
| `Tab` | Play or pause, and stay in Insert |
| `Shift-Tab` | Play from the start, and stay in Insert |
| `Esc` | Back to Normal mode |

Notes are named as Live names them: `a` starts on `C-3`, middle C, which Live
calls C3. Octaves go from -1 to 8. Below 0 a note is written with a small
letter, `_` for a natural and `+` for a sharp, so it fits three characters:
`c_1` is the C an octave under `C-0`, which Live calls C-1.

Notes play the track's sample: see **Sounds, samples and kits**.

## Accent and slide

As on a 303, `'` and `/` sit past the top of the keyboard, for accent and slide.
They work on every track, though `Acid` is made for them.

- **After a note.** Tap `'` and the note just written is accented; tap it again
  and it isn't. With auto-jump that's the row the cursor just left. If you've
  moved since, with the arrows, it's the cursor's row.
- **With a note.** Hold `'` and every note you play while it's down is written
  accented, a run of them too. Letting go then changes nothing more.
- **Slide** is the same on `/`, and both can be held at once.

An accent is velocity `7F`. Taking one off writes `Vel`, or `64` if `Vel` is
itself an accent, `70` or above. A slide is `HLD` on the row's gate, tying its
note into the next row's, which `Acid` glides to: put it on the note that slides
away, as a 303 does. The gate keeps its pulses and length, and taking the slide
off makes it a `SGL` again. The status line says `Accent on`, `Slide off` and so
on, and `u` takes each back as it does a note.

While recording, hold them as you play: a tap has no row to go on, as a note
goes in only once its row starts.

## Moving without writing a note

The arrows move and write nothing, and you stay in Insert.

| Keys | Does |
|---|---|
| `Up` `Down` | One step |
| `Left` `Right` | Back or on by the grid |
| `Shift-Left` `Shift-Right` | To the previous or next beat on the grid |

A plain arrow keeps a note's place in the beat. For hats on the "and", start
on step 2 and press `a` then `Right` over and over: steps 2, 6, A and E.

The distance is the grid, shown as `Grid` on the status line. As in Ableton,
`Option-1` makes it narrower and `Option-2` wider, halving or doubling it from
`1/16` up to a bar, in Insert and Normal. `:set skip=8` sets it to any number of
steps. Anything half typed in another column is dropped when you move.

## Recording while it plays

While the song plays, `Space r` in Normal mode arms recording, and the status
line shows a red record dot, `󰑊`, beside play's `󰐊`. Then press `i`, and each note you play on the Note
column goes in where the track is playing, not at the cursor, which stays put.
Notes sound as you play them, chords too.

| Keys | Does |
|---|---|
| `Space r` | Arm or disarm recording, while playing in the phrase view |
| `Tab` | Pausing disarms it, and so does leaving the phrase view |

- **Where.** The note goes on the cursor's track, on the row that track is
  playing. Each track loops at its own length, so a 4-row phrase against a
  16-row one gets its own row. A key pressed past halfway through a row goes on
  the next one. How early or late it was goes in the row's nudge, to the nearest
  24th of a step, so it plays back as you played it. A track at another `Speed`
  is recorded on its own rows, as long as its speed makes them.
- **How long.** The note's length goes in its gate cell, and its row keeps its
  pulses, so recording never changes how long a row or the loop is. The length is
  how much of the row's last pulse you held it for: half a step on a one-pulse row
  is `SGL1 80`. Held past the row's end, it's `FF`, which rings on until the next
  note, as a held note does. A tap on a row of several pulses gets `00`, so it
  lasts just into the last. A note still held when its row comes round again ends
  there.
- **What's replaced.** Only the note, its velocity, its nudge and its gate cell, which
  becomes a `SGL` with the pulses it had. Other columns and rows stay. The velocity is `Vel`, and
  auto-jump doesn't move the cursor.
- **Undo.** `u` takes back everything recorded since it was armed, at once.

## Auto-jump

With auto-jump on, each note moves on by the grid by itself, which suits
drums: with the grid at a beat, `a a a a` puts one on every beat. Narrow the
grid to `1/16` with `Option-1` to write every step. It's kept with the song,
and isn't undone.

## Scales

Set a scale and the home row plays it, with `a` on the root, going up through
the scale from `a` to `l`. The row above is silent, so every key is in key. The
grid still shows real note names.

```
 a s d f g h j k l
 1 2 3 4 5 6 7 1 2       scale degrees, in a seven-note scale
```

| Keys | Does |
|---|---|
| `:set scale=dorian` | Choose the kind, and turn it on |
| `:set root=d` | Choose the root, like `F#` or `Bb`, and turn it on |
| `:scale` | Choose the kind from a list |
| `Option-s` | Off and on, in Normal and Insert, keeping both |
| `:set scale=off`, `on` | The same as a command |
| `:scale fit` | Move this phrase's notes to the nearest in the scale |

A kind with two words takes a dash, as in `:set scale=harmonic-minor`. If two notes
are as near, `:scale fit` takes the lower.

While it's on, `=` `-` `>` `<` move a note through the scale, and a count moves that
many notes along it. From a note outside the scale, a step lands on the next one in.
`+` and `_` are still an octave, and `C-a` and `C-x` are still a semitone, for a note
outside the scale on purpose.

A scale is only for entering and moving notes. It changes nothing you've written,
so it can't change how a song sounds, unless you run `:scale fit`. Notes outside it
show in a different colour, and the status line shows `Scale D Dor`. It's saved with
the song, and `u` doesn't undo it.

A chain's `Transpose` plays a phrase higher or lower whatever the scale, so the grid
judges the notes as written.

## Velocity while you play

Every note you write gets the velocity on the status line, as `Vel`, and you
hear it as you type. Velocity runs from `00` to `7F`, the same range as MIDI.
It starts at `64`, which leaves room above it for louder hits. Press `c` and
`v` to go down or up by the top digit: `54`, `44` and so on to `04`, or `74`
then `7F`, which is full. Like the octave, the setting isn't saved with the
project.

`Backspace` clears a note with the velocity it was written with. An empty velocity, `··`, plays at full.

For how loud a whole track is, use its `Level`, on the mixer or the Sound page's
Output, or `:tvol`. See **Phrases and chains**. To move the notes' velocities as
they play, leaving what's written, use `VDLT` on a step, in a lane or from a mod
slot, or the track's `Velocity` on the Sound page. See **FX commands**.

## The other columns

Press `l` in Normal mode to move to another column, then `i`:

| Column | In Insert |
|---|---|
| Velocity | Two hex digits up to `7F`, such as `40`. Auto-jump moves on |
| Nudge | A digit to `B`, after `-` if early, such as `-3`. `0` is on the grid |
| Gate | A gate and its pulses, such as `HLD3`, or a digit alone |
| Length | The gate's length, two hex digits: `80` is half the last pulse |
| FX | A label, `Enter`, then a value, like `CUT` `Enter` `40`. See **FX commands** |

## Gates

The gate column, after velocity, says how long a row lasts and what its note does
over that time. Its argument is always the row's length in pulses, which are
steps, from `0` to `8`. An empty cell, `···· ··`, is one pulse of `SGL`, as every
row used to be. The dim pair after it is how long the note lasts: see **How long a
note lasts**.

| Gate | Its pulses |
|---|---|
| `SGL` | One note, on the first |
| `MLT` | A note on each |
| `HLD` | One note, tied into the next row's with no new attack, by `Glide` |
| `RST` | No note, and it ends one that's sounding |

Any gate with `0` pulses skips the row: it takes no time and plays nothing. A gate on
a row with no note still sets how long the row lasts, so `RST4` there is four steps
of nothing. `K` chooses the gate, `=` and `-` change the pulses, and `x` empties
it, length and all. The row's FX run on its first pulse, though a `MLT` runs its `DEL`, `KIL`,
`OFF` and `RET` on each, and lanes keep running on every step. A beat row is one
that starts on a beat, however long the rows before it.

## Nudging a note off the grid

A row's nudge moves its note a little early or late, in 24ths of a step, up to
`B`, just under half a step, either way, as Elektron's micro-timing does. The
step is the track's own, so on a track at `1/8` a 24th is twice as long. It sits
between the velocity and the gate, drawn dim, and is blank on a row that's on
the grid: `C-3 40 -3 SGL1` plays three 24ths early.

| Keys | Does |
|---|---|
| `=` `-` | Nudge it a 24th later or earlier |
| `+` `_` | A sixth of a step, `DEL`'s unit |
| `i` | `-` for early, then the 24ths as one digit; a digit alone is late |
| `x` | Back on the grid |

The whole row's note part moves: the note, its velocity and length, its own FX
columns, and its envelopes. `DEL`, `KIL`, `OFF` and `RET` count from where the
note now starts, and each pulse of a `MLT` moves together. What stays on the
grid is what keeps time apart from the note: an FX column with a lane of its own,
the LFOs and the groove. A note nudged early on a step where a lane changes a
value starts with the value before, and the lane's change comes in on the grid.

The nudge adds to where the groove lands the step, so a swung step nudged `-2`
plays a little before its swing. A track's notes always play in order: one
nudged early never starts before the row before it, which in a groove that
pushes that row late can mean it waits for it.

It's in the velocity's column group, so a selection of velocities copies, pastes
and clears their nudges too, while `x` on either clears only that one. `=` and
`-` on a selection give a nudge only to rows with a note.

The track's `Quantise`, on the Sound page's Play device, brings every nudge toward
the grid as it plays, from none to all, without changing what's written, so a
recorded take can be tightened live, or loosened again. `QNT` on a step does it for
that row. `d` on a selection of nudges puts them on the grid for good.

## How long a note lasts

A note lasts so much of its row's last pulse before its gate closes, from `1%` to
`100%`. Closing the gate starts the release, as `OFF` does, so the release shapes
the gap before the next note.

A row can say how long in its gate cell, after the gate and its pulses: `SGL1 80`
lasts half its step, and `SGL3 40` two steps and a quarter. It's on the `00` to
`FF` scale, with `FF` all of it, and drawn dim. `l` goes from the gate onto it,
two hex digits in Insert or after `r` set it, `=` and `-` move it by one and `+`
and `_` by 16, and `x` takes it away. It's copied and pasted with the gate. An
empty one, `··`, is the track's `Gate`, in the Sound page's Play device under
`Glide`, and `=` or `-` on it start from there.

| Gate | At `Gate` 50% |
|---|---|
| `SGL` | Held to halfway through its last pulse, so `SGL3` is 2.5 steps |
| `MLT` | Each pulse's note lasts half the pulse, for a staccato run |
| `HLD` | Ignores it, tying into the next row's note, which closes its own |
| `RST` | No note, as before |

At `100%`, every voice's default but `Acid`'s, a note holds until something ends
it. A delayed or nudged note's length counts from where it starts, so a nudge
moves its end with it, and a `RET` repeats only while the gate is open.

What moves `Gate` moves a note's length up or down from where it is, a row's own
too, as an LFO moves any setting around where a step put it, and it stays from
`1%` to `100%`:

- `LEN` on a step sets `Gate` for its row's notes, if the row has no length of
  its own.
- `LDLT` on a step moves the note's length: `80` leaves it, `40` takes a quarter
  of the pulse off, and `C0` adds a quarter.
- An LFO or envelope on `Gate` moves every note's length as it starts, a row's
  own too.

`HLD` ignores all of them. `LDLT` is made for lanes: in an FX column with a lane of
its own, it drifts against the notes, so a row plays short one pass and long the
next. See **FX commands**.
