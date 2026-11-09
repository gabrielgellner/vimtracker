# FX commands

Each step has FX slots to the right of the Velocity column. A slot is a
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
| `SNA` | Send to return `A` |
| `SNB` | Send to return `B` |
| `PNCH` | A voice's setting, by its own label, like Punch on `Kick` |
| `2FDBK` | A slot's setting, by the slot's digit and its label, below |
| `PTCH` | Pitch: `80` as played, a semitone a step, cents in the fine part |
| `LEN` | Gate, for rows with no length of their own. `80` is half |
| `LDLT` | Gate up or down from a note's own: `80` none, `00` half off |
| `GRV` | The step's groove: `00` the project's, `01` Straight |
| `DEP1` `DEP2` | An LFO's depth: `80` is none, shown as on the Mod page |
| `PHS1` `PHS2` | An LFO's phase: `00` to `FF` is a whole turn, and it goes round |

Each voice has its own settings, and the track keeps every voice's values, so
changing voice and back loses nothing. A lock for a voice the track isn't
playing, or an effect no longer in its slot, is drawn faint and does nothing
until its voice or effect is back.

`LDLT` is for lanes. A note's length is its row's own, in its gate cell, or the
track's `Gate`, and `LDLT` adds to it as the note starts, as an LFO adds to what
a step sets, from `1%` to `100%`. In an FX column with a lane of its own, its
values drift against the notes, so the same row plays short on one pass and long
on the next: an articulation that moves on its own. See **Note entry**.

A slot's settings can be set from a step, the same way as the rest, while the
slot holds an effect. Each is its slot's digit and a short label, like `1AMNT`
or `2FDBK`, and `K` lists them under the effect's group with the names they have
in an LFO's `Dest`, like `1 Amount`. `Bits` takes its place among the depths,
from 2 bits at `00`, so `04` is 6 bits. A step's setting belongs to that effect,
and a mode's to that mode, so a `2FDBK` lock does nothing once slot 2 holds a
Reverb, and a `1BITS` lock waits for `Crush`. A Distort's `Type` isn't set from
a step, as choosing a mode chooses a sound.

`VOL`, `SNA` and `SNB` are on the level scale in **Phrases and chains**. The
others start from the values on the track page, `Space 4`, each time playback
starts from the top. An LFO on that page keeps moving a value around wherever
a step has put it. See **The track page, envelopes and LFOs**.

Each value has a mark beside it, which says how long it lasts:

| Mark | Lasts |
|---|---|
| `.` lock | The default. Until the next note or the next value of that command |
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

A unit is a sixth of a step, so `DEL03` delays by half a step. They count from
where the note starts, after the groove and the row's nudge, and so does `CHA`,
which goes with the note whichever column it's in.

For `RET`, `x` from `0` to `7` gets quieter on each repeat and `8` to `F`
gets louder. Use `:seed 7` to choose which results `CHA` gives.

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

A groove counts the song's steps from its start or the last reset, not each
track's own loop, so tracks of different lengths swing together and stay on the
beat, and a track with an odd length swings different rows each time round.
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
