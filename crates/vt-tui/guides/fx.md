# FX commands

Each step has three FX slots, to the right of its gate. A slot is a
command and a hex value. A setting that sweeps, like `CUT`, shows four digits,
`CUT   4080`: the bold pair is the value you type, and the dim pair after it is a
fine part, for dialling in between two values. A list's value, like `1BITS`,
and an event's, like `DEL`, are only two digits. Events are in their own colour,
a lock the track doesn't have is grey, and empty cells are faint.

`K` on a slot lists the commands this track has, in categories as an LFO's `Dest`
is: the filter, the output, the voice, each slot's effect, then the events. It opens
searching, so `send b` or `decay` finds its command, and `Enter` takes it. `Esc`
goes to the list, to browse the categories. Type the value next.

To type one instead, press `i` on the slot, type its label, such as `CUT` or
`2FDBK`, and press `Enter`, then type the value, such as `40`. A label that isn't
on the track opens `K`, searching for what you typed. A slot only takes effect
once its value is typed. The status line names what the cursor is on and its
value in real units, such as `Filter: Cut, 1.21 kHz`.

The fine digits are a field of their own: `l` goes from the value onto them, and
passes them by on a list or an event, which have none. There `=` and `-` move the
fine part by one, and carry into the value past `FF` or below `00`, `+` and `_`
move it by 16, two digits in Insert or after `r` set it, and `x` sets it back to
`00`. The value's own digits keep the fine part as they change, and stop at the
top, where the fine part is `00`. A new value starts with none. A ramp from the
shape panel fills the fine part too, so it moves evenly even between two values
a step apart. A list's value can't glide, so `~` goes between lock and hold.

## Values and their marks

| Command | Sets |
|---|---|
| `VOL` | Track gain from the pattern. `E0` is 0 dB, and starts there |
| `PAN` | Pan: `00` is left, `80` centre, `FF` right |
| `CUT` | Filter cutoff, from 20 Hz to 20 kHz |
| `RES` | Filter resonance |
| `MODE` | The filter's mode, by its place: see the track page's guide |
| `SNA` | Send to return `A` |
| `SNB` | Send to return `B` |
| `PNCH` | A voice's setting, by its own label, like Punch on `Kick` |
| `ACC` | A voice's Accent, how hard it's struck |
| `SMP` | A sampler's sample, by its number in the bank. `00` is none |
| `SLI` | A sampler's slice. `STA`, `SLEN` and `PLY` too |
| `2FDBK` | A slot's setting, by the slot's digit and its label, below |
| `PTCH` | Pitch: `80` as played, a semitone a step, cents in the fine part |
| `LEN` | Gate, for rows with no length of their own. `80` is half |
| `LDLT` | Gate up or down from a note's own: `80` none, `00` half off |
| `VDLT` | Velocity up or down from a note's own: `80` none, `90` adds `10` |
| `GRV` | The step's groove: `00` the project's, `01` Straight |
| `RATE` | The step's speed: `00` 1/32, `02` 1/16, `04` 1/8, `06` 1/4 |
| `GLD` | How long a tie glides into the next note: `00` off, `80` about 60 ms |
| `1BYP` | Switch slot 1 out, `01`, or back in, `00`. `2BYP` to `4BYP` the others |
| `DEP4` | Mod 4's depth: `80` is none, as on the Mod page. `DEP5` `DEP6` too |
| `DEP2` | An envelope's depth. `DEP1` `DEP3` too, but not the notes' shape |
| `PHS4` | An LFO's phase: `00` to `FF` is a turn, round. `PHS5` `PHS6` too |
| `POL4` | Mod 4's polarity: `00` one way, `01` both. `POL5` `POL6` too |
| `POL2` | An envelope's polarity. `POL3` too, but not the notes' shape |

Each voice has its own settings, and the track keeps every voice's values, so
changing voice and back loses nothing. A lock for a voice the track isn't
playing, or an effect no longer in its slot, is drawn faint and does nothing
until its voice or effect is back.

`LDLT` is for lanes. A note's length is its row's own, in its gate cell, or the
track's `Gate`, and `LDLT` adds to it as the note starts, as an LFO adds to what
a step sets, from `1%` to `100%`. In an FX column with a lane of its own, its
values drift against the notes, so the same row plays short on one pass and long
on the next: an articulation that moves on its own. See **Note entry**.

`VDLT` is its twin for velocity. It adds to each note's own velocity as the note
starts, a step of `VDLT` a step of velocity, kept from `00` to `7F`, so `90` adds
`10`, `70` takes `10` off, and `00` and `FF` play every note silent or full. The
note keeps what's written, and a note that's sounding keeps the velocity it
started with. On a step, `VDLT 90` makes one note louder without rewriting it. In
a lane of its own, a column of `VDLT`s is an accent pattern that drifts against
the notes. Glided, `VDLT 40g` to `VDLT C0` many rows on, it's a crescendo, each
note on the way getting its own share. A mod slot's `Dest` lists it too, for an
LFO's swells. It starts from the track's `Velocity`, on the Sound page's Play
device, which the Roto has as a knob, for dynamics played live.

On `Acid`, velocity only says whether a note is accented, at `70` or above, so a
`VDLT` that takes a note across `70` switches its accent on or off, and does
nothing else.

A slot's settings can be set from a step, the same way as the rest, while the
slot holds an effect. Each is its slot's digit and a short label, like `1AMNT`
or `2FDBK`, and `K` lists them under the effect's group with the names they have
in an LFO's `Dest`, like `1 Amount`. `Bits` takes its place among the depths,
from 2 bits at `00`, so `04` is 6 bits. A step's setting belongs to that effect,
and a mode's to that mode, so a `2FDBK` lock does nothing once slot 2 holds a
Reverb, a `1BITS` lock waits for `Crush`, and a `1TONE` lock for a pedal. What's in
a slot's title, the `Type` of a `Distort`, `Chorus` or `Comp` and the `Mode` of an
`LPG` or `Comb`, isn't set from a step, as choosing it chooses a sound, and nor is a
`Source`.

`ACC`, `SMP`, `SLI`, `STA`, `SLEN` and `PLY` are in **Sounds, samples and kits**.
`VOL`, `SNA` and `SNB` are on the level scale in **Phrases and chains**. The
others start from the values on the track page, `Space 4`, each time playback
starts from the top. An LFO on that page keeps moving a value around wherever
a step has put it. See **The track page, envelopes and LFOs**.

Each value has a mark beside it, which says how long it lasts:

| Mark | Lasts |
|---|---|
| `󰌾` lock | The default. Until the next note or the next value of that command |
| `h` hold | Until playback next starts from the top. A lock returns to this |
| `g` glide | Slides to the next value of that command in the slot |

A glide wraps through the phrase's loop to find its value.

Press `~` on a value to cycle its mark, or in Visual to give every value selected
the mark after the first one's, and `.` repeats it. A value typed into an empty slot copies the mark
of the nearest value above it.

A note that a failed `CHA` skips, an `OFF` and a `RET` don't end a lock, as the same note is
still sounding. A note doesn't stop a glide, so a sweep can cross notes, and a glide's last row
uses its own mark, so ending on a lock jumps back at the next note. With no other value to
slide to, a glide stays where it is, as a lock would. An edit on the track page changes what's
underneath, so you hear it straight away unless a lock or glide is on top.

## Things that happen once

These have no mark.

| Command | Does |
|---|---|
| `DEL` | Delay this step by `xx` units |
| `KIL` | Cut the note after `xx` units |
| `OFF` | Start the envelopes' release after `xx` units |
| `RET` | Retrigger: `xy` repeats every `y` units, and `y` of `0` does nothing |
| `CHA` | Play the note with a chance of `xx` out of `FF` |
| `CND` | Play the row only when condition `xx` holds: see **Conditions** |
| `ACU` | Move the row's note on each time it plays: see **Accumulators** |
| `ARP` | Go round the note, `x` up and `y` up: see **Arpeggios** |

A unit is a sixth of a step, so `DEL03` delays by half a step. The step is the
track's own, so on a track at `1/8` it's half an eighth. They count from where
the note starts, after the groove and the row's nudge, and so does `CHA`, which
goes with the note whichever column it's in.

For `RET`, `x` from `0` to `7` gets quieter on each repeat and `8` to `F`
gets louder, except on an `Acid`, whose hits all play at the note's own level.
Use `:seed 7` to choose which results `CHA` gives.

## Conditions

`CND` plays its row only some of the time, as Elektron's trig conditions do, so a
loop can change from pass to pass without each variation written out. Choose `CND`
with `K` and then its condition from the list that follows; `K` on its value lists
them again, and `=` and `-` step through them.

| Value | Name | Plays the row |
|---|---|---|
| `12` to `88` | `1:2` to `8:8` | On pass `x` of every `y` |
| `01` `02` | `FILL` `!FILL` | While fill is on, or off |
| `03` `04` | `PRE` `!PRE` | When this track's last condition passed, or not |
| `05` `06` | `NEI` `!NEI` | Likewise for the track to the left |
| `07` `08` | `1ST` `!1ST` | The first time round only, or every time after |
| `92` to `9F` | `>2` to `>15` | From pass `n` on, so a part joins and stays |
| `A2` to `AF` | `<2` to `<15` | Only before pass `n`, so a part drops out |
| `B0` to `BF` | `>16` to `>31` | From pass 16 to 31 on |
| `C0` to `CF` | `<16` to `<31` | Only before pass 16 to 31 |

So `12` plays every other pass, and `44` the last of four. `>` and `<` are for
builds: a hat with `95` joins on the fifth pass and stays, and an intro fill with
`A3` plays the first two passes and drops out. `>2` is `!1ST` and `<2` is `1ST`.
Past 15, `B` and `C` carry on where the second digit stops, so `B4` is `>20`.
They count the same passes as the rest, so a build starts again with its chain,
a section's reset, `:reset` and the track's `Reset`, and a section with `Rpt 8`
builds across its repeats unless its Start is `reset`. `PRE` and `NEI` chain
other rows to a built one. In the list, `K`, they're under **From**
and **Until**.

A pass is each time the track plays the row since its chain started, counting its
own steps at its own speed, so a phrase played twice in its chain counts two passes
each time round. Passes start again wherever the chain starts: playing from the
top, a section that brings the chain in or resets, a launch, a realign, `:reset`
and the track's own `Reset`. Playing from a step counts the passes before it, as playing up to it would.

When a condition fails, the row's note, its events and its own locks are all
skipped, as Elektron skips a trig and its locks. A column with a lane of its own
keeps to its loop, and a `CND` in a lane counts the lane's passes. With two
conditions on a row, both must hold. While playing, the row under the playhead is
faint when its `CND` or `CHA` skipped it.

`PRE` and `NEI` read what the last `CND` or `CHA` decided, and aren't counted
themselves, so after a `1:2`, a `PRE` row plays with it and a `!PRE` row plays
instead of it. Nothing has passed when playing starts. `CHA` is still rolled when a
condition fails it, so its results stay the same, and a failed `CHA` skips only the
note, as it always has.

`Space l` turns fill on or off, and held, it's on until you let go. The status
line says `FILL` while it's on.

## Accumulators

`ACU` moves its row's note on each time the row plays, as the Metropolix's
accumulator does, so a line climbs, falls or swings from pass to pass. It moves by
scale degrees with the scale on, `:set scale=on`, and by semitones with it off. A
note outside the scale lands on the next one in. The value is signed: `01` to `1C`
go up 1 to 28, and `FF` to `E4` go down 1 to 28, so `FE` is two down. Choose `ACU`
with `K` and then how far from the list that follows; `K` on its value lists them
again, `=` and `-` go up and down through `00`, and `+` and `_` move by 7.

The first time the row plays it's as written, and each time after, its `ACU`
adds first. With `C D E F` and `ACU 01` on the `C`, it plays:

```text
C D E F | D D E F | E D E F | F D E F
```

The track's accumulator settings are the `Accumulator` device on the Mod page:

| Setting | Does |
|---|---|
| `Up` `Down` | How far from the note as written it goes, 1 to 28 |
| `Moves` | `Row` moves only its own row, `Track` every row |
| `Limit` | `Wrap`, `Bounce` or `Hold` at the limit |
| `Adds` | `Row` once a row, `Pulse` each pulse, `Retrig` each hit |
| `Reset` | `Chain`, `Play`, or every 1 to 16 loops of the chain |

The limits are 7 unless changed. With `Moves` at `Track`, every row on the track
moves with the `ACU`s, rows without one too, so `C D E F` plays `D E F G` the
second time. `Wrap` carries on from the note as written, `Bounce` turns back at
the limit and again at the note, and `Hold` stays at the limit. `Pulse` adds on
every pulse of a `MLT` too, and `Retrig` on every hit of a `RET` as well. `Reset`
says when it goes back to the notes as written: `Chain`, wherever the chain
starts, or `Play`, only when playing starts.

A row that `CHA` or `CND` skips doesn't add, nor does a `RST`, and two `ACU`s on
a row add up. Playing from the top or from a step starts every accumulator from
the notes as written, and so does the chain starting, with `Reset` at `Chain`: a
section that brings it in or resets, a launch, a realign, `:reset` and the
track's own `Reset`. A phrase played twice
in its chain moves on each time.

The grid shows notes as written. While playing, the row under the playhead shows
the note heard, before the chain's transpose, and the status line says it on the
Note column.

## Arpeggios

`ARP xy` goes round its row's note, the note `x` up and the note `y` up, as
Renoise's `-Axy` and the M8's `ARP` do, so `ARP 47` on a `C-3` plays C, E and G
over and over. With the scale on, `x` and `y` are scale degrees, as the home row
is, so `ARP 24` plays a triad in any scale; with it off they're semitones. A `0`
leaves that one out, for a two-note arpeggio: `ARP 70` and `ARP 07` both go
between the note and its fifth. Type its two digits after choosing it with `K`;
`=` and `-` move `y`, and `+` and `_` move `x`.

The track's arpeggio settings are the `Arpeggio` device on the Mod page:

| Setting | Does |
|---|---|
| `Rate` | How long each note is: `Tick`, a sixth of a step, or `1/128` to `1/8` |
| `Mode` | `Up`, `Down`, `Up-down`, `Up-down-inc` or `Random` |
| `Oct` | How many octaves it climbs before it starts again, 1 to 4 |
| `Retrig` | `off` only changes the pitch, `on` starts each note again |

`Tick` is the default, six notes to a step, the tracker feel. A division is as
long against the track's step as on a track at `1/16`, so on a track at `1/8`
each note is twice as long, as everything that's a share of a step is. `Up` plays
the note, `x` then `y`, then the same an octave up for each octave; `Down` plays
that backwards from the top, `Up-down` goes up and back without playing the ends
twice, `Up-down-inc` plays them twice, and `Random` picks any of them each time, the same in every render. With
`Oct` at 2, `ARP 00` goes between the note and its octave. With `Retrig` off each
note only changes the pitch, and on `Acid` it slides; on, each note starts again
with its envelopes, as a `RET` does.

It lasts as long as its note: through the row's pulses and any empty rows after,
until the next note, an `OFF`, a `KIL`, a rest or the note's gate closing. A
`HLD` carries it on into the note it ties into, going round from that note where
it was, with that row's own `ARP` if it has one. Any other note starts again with
its own `ARP`, or none. Each pulse of a `MLT` is a new note, so it starts again on
each. It starts where the note does, after the groove, the nudge and any `DEL`, and
an `ACU` or the chain's transpose moves it with the note. One voice plays it, a
note at a time, so it isn't a chord.

## Groove

A groove is a track's feel: a short pattern of pushes, how late or early each
step lands, with a velocity for each and a little random timing if you like. It
moves steps about the grid and never the grid itself, so the tempo and every
loop's length stay as they are, and the pattern's first step is always on the
grid. Swing is a groove of two steps.

| Command | Does |
|---|---|
| `:groove swing-66` | The project's groove, for tracks without their own |
| `:groove` | Choose the project's groove from a list |
| `:swing 66` | The same as `:groove swing-66`, for 50 to 70 |
| `:tgroove mpc-58` | The cursor's track's own, or `:tgroove` to choose |
| `:tgroove off` | That track follows the project's again |

The grooves that come with vimtracker:

| Groove | Feel |
|---|---|
| Straight | Every step on the grid |
| Swing 52 to Swing 70 | Every other step late, as swing was |
| MPC 54 to MPC 71 | An MPC 60's sixteenth swing, on its own clock |
| Late last | A bar's last step a sixth of a step late |
| Push | Ahead of the beat: the off-beat eighths early |
| Pull | Laid back: the backbeats late |
| Custom 1 to 8 | The project's own, from the project page |

A groove counts each track's own steps from the song's start or the last reset,
not each track's own loop, so tracks of different lengths swing together and stay
on the beat, and a track with an odd length swings different rows each time
round. A track at `1/8` counts its eighths, so it swings its eighths, and its
pushes are shares of its own step, twice as long as a sixteenth's.
Every pulse of a row is a step, so `MLT4` is grooved on each pulse, and lanes
play on the grooved steps too. `DEL` adds to where the groove lands a step. A
note's `Gate` is a share of its last pulse as the groove makes it, so the long
first step of a swung pair holds its note longer.

A row's nudge goes on top: grid, then groove, then nudge. It moves the row's
note and its own FX columns, while a column with a lane of its own stays where
the groove lands the step, so a lane's value on a step comes in on time however
the note there is nudged. See **Note entry**.

`GRV` on a step chooses the groove that step and the rest of its note play with,
worked out before the step lands, so `GRV 01`, Straight, on a fill's rows plays
them straight. Held with `h`, it stays until another `GRV` changes it, and `00`
is the project's. `K` lists them by name.

## Speed

A track's `Speed`, in its Sound page's Play device, is how long each of its steps is: `1/32`,
`1/16T`, `1/16`, `1/8T`, `1/8`, `1/4T` or `1/4`, the song's `1/16` unless
changed. A track at `1/8` plays its rows half as fast, and everything on it
follows its step: its pulses, its lanes, `DEL`, `KIL`, `OFF` and `RET`, its
nudges, its groove's pushes and its notes' `Gate`. Sections, `:reset`, launches
and renders count the song's sixteenths, so a section lasts its longest chain
at that chain's speed: 16 rows at `1/8` are 32 sixteenths.

`RATE` on a step sets the speed of that step and the rest of its note, worked
out before the step plays, as `GRV` is, and held with `h` it stays. A step at a
new speed lasts until the next of that speed's steps, counted from where the
track last lined up, so the track keeps to its new speed's grid: an eighth on
the second sixteenth lasts one sixteenth, and the track then plays on the
eighths. A track lines up again wherever its chain starts: at the song's start,
a reset, a section that gives it a new chain, or a launch. LFOs keep to the
song's time whatever the speed.
