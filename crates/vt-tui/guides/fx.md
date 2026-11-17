# FX commands

Each step has three FX slots, to the right of its gate. A slot is a
command and a hex value. A setting that sweeps, like `CUT`, shows four digits,
`CUT   4080`: the bold pair is the value you type, and the dim pair after it is a
fine part, for dialling in between two values. One on the level scale, like
`VOL`, `LEV`, `SNA` and `SNB`, is two digits, as a quarter of a dB is its step,
and so are a list's value, like `1BITS`, and an event's, like `DEL`. Events are in their own colour,
a lock the track doesn't have is grey, and an empty slot is faint dots, `···   ··`, with no
value.

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
| `LEV` | The fader, the track's `Level`, on the level scale |
| `VOL` | Gain on top of the fader. `E0` is 0 dB, and starts there |
| `PAN` | Pan: `00` is left, `80` centre, `FF` right |
| `CUT` | Filter cutoff, from 20 Hz to 20 kHz |
| `RES` | Filter resonance |
| `MODE` | The filter's mode, by its place: see the Sound page's guide |
| `KTRK` | The filter's `Key track` for the row: `00` off, `01` on |
| `FTYP` | The filter's type, by its place: `00` LP, `01` HP, `04` Ladder, `09` OB-X |
| `FDRV` | The filter's `Drive` |
| `FPCH` | The filter's `Punch`, which the clean types hear |
| `SAFE` | The filter's `Safe`: `00` off, `01` on |
| `LCUT` | The utility high-pass after the filter: `00` off |
| `HCUT` | The utility low-pass after the filter: `FF` off |
| `SNA` | Send to return `A` |
| `SNB` | Send to return `B` |
| `PNCH` | A voice's setting, by its own label, like Punch on `Kick` |
| `ACC` | A voice's Accent, how hard it's struck |
| `SMP` | A sampler's sample, by its number in the bank. `00` is none |
| `SLI` | A sampler's slice. `STA`, `SLEN`, `PLY` and `SREV` too |
| `2FDBK` | A slot's setting, by the slot's digit and its label, below |
| `PTCH` | Pitch: `80` as played, a semitone a step, cents in the fine part |
| `LEN` | Gate, for rows with no length of their own. `80` is half |
| `LDLT` | Gate up or down from a note's own: `80` none, `00` half off |
| `VDLT` | Velocity up or down from a note's own: `80` none, `90` adds `10` |
| `GRV` | The step's groove: `00` the project's, `01` Straight |
| `RATE` | The step's speed: `00` 1/32, `02` 1/16, `04` 1/8, `06` 1/4 |
| `QNT` | Nudges brought to the grid: `00` none, `80` half, `FF` all |
| `JMP` | The chance this step plays any row of the phrase: `00` none, `FF` every |
| `DLP` | Dice loop: `00` off, `01` to `10` steps, `11` 32, `12` 64 |
| `DMU` | Dice mutate: `00` a locked loop, `FF` fresh dice |
| `DJMP` | Dice jump: the chance of another step's dice in the loop |
| `ORD` | The order, places on in the list from the phrase's own: `80` its own |
| `REV` | The walk the other way up, `01`, or as written, `00` |
| `ORDX` | The order's first number: `00` the phrase's own, `01` to `10` |
| `ORDY` | The order's second number: `00` the phrase's own, `01` to `10` |
| `PLEN` | Rows the order walks, up or down from the loop's: `80` is none |
| `PSTA` | The row the order's window starts on, rows on from the first |
| `CDLT` | Every row's chance up or down: `80` none, `40` half, `00` none play |
| `CTGT` | What a row's chance decides: `00` the whole row, `01` each pulse |
| `CUE` | A new window or lane waits for its loop, `01`, or moves at once, `00` |
| `GLD` | How long a tie glides into the next note: `00` off, `80` about 60 ms |
| `OSC` | A synth's `Phase` for the row's note: `00` Free, `01` Reset |
| `VMOD` | The track's `Mode` for the row's note: `00` Mono, `01` Legato |
| `GMOD` | The track's `Glide mode` for the row's note: `00` Ties, `01` Always |
| `1BYP` | Switch slot 1 out, `01`, or back in, `00`. `2BYP` to `4BYP` the others |
| `DEP4` | Mod 4's depth: `80` is none, as on the Mod page. `DEP5` `DEP6` too |
| `DEP2` | An envelope's depth. `DEP1` `DEP3` too, but not the notes' shape |
| `PHS4` | An LFO's phase, or its axis: `00` to `FF` is a turn. `PHS5` `PHS6` too |
| `DFM4` | An LFO's deform, or its chaos: `80` is none. `DFM5` `DFM6` too |
| `POL4` | Mod 4's polarity: `00` one way, `01` both. `POL5` `POL6` too |
| `POL2` | An envelope's polarity. `POL3` too, but not the notes' shape |
| `SHP4` | An LFO's shape: `00` Sine, `01` Tri, `04` Square. `SHP5` `SHP6` too |
| `LMOD4` | An LFO's mode: `00` Free, `01` Trig, `02` Hold. `LMOD5` `LMOD6` too |
| `FADE4` | An LFO's fade: `16` Off, below out, above in. `FADE5` `FADE6` too |
| `SMOD3` | A Steps slot's mode: `00` Trig, `01` Free, `02` Note |
| `LOOP3` | A Steps slot's loop: `00` Once, `01` from 1, `11` Bounce, `12` Shuffle |
| `CRV2` | An envelope's curve: `00` Exp, `01` Lin, `02` Snap. `CRV1` `CRV3` too |
| `HOLD2` | An AHD's hold: `00` none, up to 2 s at `FE`, and the note at `FF` |
| `PEAK2` | A drum envelope's peak, on its times' scale, as a macro turns it |
| `RTCH2` | A drum envelope's strikes: `00` one, `07` eight |
| `ALIM` | The accumulator's `Limit`: `00` Wrap, `02` Bounce, `04` Hold, `05` Random |
| `AADD` | When an `ACU` adds: `00` Row, `01` Pulse, `02` Retrig |
| `ARST` | The accumulator's `Reset`: `00` Chain, `01` Play, `02` 1 loop, `11` 16 |
| `AMOD` | The arpeggio's mode: `00` Up, `01` Down, `04` Random |
| `EATK2` | An envelope's attack: `00` none, `01` 1 ms to `FF` 8 s. `EATK1` `EATK3` too |
| `EDEC2` | An envelope's decay, on the same times. `EDEC1` the notes', `EDEC3` too |
| `ESUS2` | An envelope's sustain: `00` none, `64` all. `ESUS1` `ESUS3` too |
| `EREL2` | An envelope's release, on the same times. `EREL1` `EREL3` too |
| `BODY2` | A drum envelope's body: `00` none, `64` all |
| `CLR2` | A gate's colour, how far it closes the filter: `00` none |
| `RATE4` | An LFO's or Steps' rate, faster up the scale. `RATE5` `RATE6` too |
| `LEN3` | A Steps slot's length: `00` one step, `0F` sixteen |
| `MUT3` | A Steps slot's mutate: `00` none, `64` every value drawn again |
| `SMTH3` | A Steps slot's smooth: `80` steps, above glides, below plucks |
| `JMP3` | A Steps slot's jump: `00` none, `64` every step |
| `SPR3` | A Steps slot's spread: `32` even, `00` the bias, `64` the ends |
| `BIAS3` | A Steps slot's bias: `80` none, below low, above high |
| `MAC1` | Macro 1, and every setting it turns, each along its range |
| `MAC2` | Macro 2. `MAC3` `MAC4` `MAC5` `MAC6` `MAC7` `MAC8` the others |
| `LLEN1` | FX 1's lanes' length, rows up or down: `80` is none. `LLEN2` `LLEN3` too |
| `LRAT1` | FX 1's lanes' speed: `÷` slower, `×` faster. `LRAT2` `LRAT3` too |
| `LSTA1` | FX 1's lanes' first row, rows on from their own. `LSTA2` `LSTA3` too |
| `LJMP1` | FX 1's lanes' chance of any of their rows instead. `LJMP2` `LJMP3` too |

Each voice has its own settings, and the track keeps every voice's values, so
changing voice and back loses nothing. A lock for a voice the track isn't
playing, or an effect no longer in its slot, is drawn faint and does nothing
until its voice or effect is back.

A mod slot's own settings, its times, levels and rates, its lists and its odd
times, end in the slot's number, as `EDEC1` is Mod 1's decay and `SHP4` Mod 4's
shape, and `K` lists each under its slot while the slot's type has it. A lock on
another type, as `SHP4` once Mod 4 is an envelope, is drawn faint and does
nothing. Each is heard from its row to the track's next note, as any lock is, and
one on a note's row is that note's from its start, so `EDEC1` per step is an
Elektron box's amp decay per trig. A swapped wave or mode carries on from where
the slot is: see the Mod page's guide.

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

`ACC`, `SMP`, `SLI`, `STA`, `SLEN`, `PLY` and `SREV` are in **Sounds, samples and kits**.
`LEV`, `VOL`, `SNA` and `SNB` are on the level scale in **Phrases and chains**.
The others start from the values on the track page, `Space 5`, each time playback
starts from the top. An LFO on that page keeps moving a value around wherever
a step has put it. See **The Sound page, output and groups** and **The Mod page, envelopes
and LFOs**.

Each value has a mark beside it, which says how long it lasts:

| Mark | Lasts |
|---|---|
| `󰌾` `.` lock | The default. Until the next note or that command's next value |
| `󰐃` `h` hold | Until playback next starts from the top. A lock returns to this |
| `󰸀` `g` glide | Slides to the next value of that command in the slot |

The grid draws each mark as its glyph, the lock dim and the others in the value's colour.
The letter is how a step's text and the command line write it.

A glide wraps through the phrase's loop to find its value.

Press `~` on a value to cycle its mark, or in Visual to give every value selected
the mark after the first one's, and `.` repeats it. A value typed into an empty slot copies the mark
of the nearest value above it.

A note that a failed `CHA` skips, an `OFF` and a `RET` don't end a lock, as the same note is
still sounding. A note doesn't stop a glide, so a sweep can cross notes, and a glide's last row
uses its own mark, so ending on a lock jumps back at the next note. With no other value to
slide to, a glide stays where it is, as a lock would. An edit on the track page changes what's
underneath, so you hear it straight away unless a lock or glide is on top.

## Fader automation and relative modulation

A phrase can move the mixer two ways, as an Ableton clip's envelopes can.

**Fader automation** sets the faders themselves: `LEV` the track's `Level`, `PAN`
its `Pan`, and `SNA` and `SNB` its sends. While a step drives one, the fader
shows the step's value, on the mixer, on the Sound page's Output device and on the
Roto, whose motor follows it, gliding as a glide does. To keep a value after the
phrase stops, give it `h`: a hold lasts until playback starts from the top again,
through the transport stopping, the chain moving on to a phrase with nothing for
it, and the track being muted, as its steps still run underneath. A lock lasts
until the next note.

**Relative modulation** moves what's heard up and down from the fader, wherever
the fader is: `VOL`, a stage on top of the fader, and a mod slot's `Dest` on `Vol`,
`Pan`, `Send A` or `Send B`. The fader stays where it is, and what's heard is a thin
mark beside it: a line across the mixer's fader, and the mark in the Output
device's bars. A mod slot can't move `LEV`, as `Vol` is where it moves the volume.

**Taking a fader over.** Moving a fader by hand while the steps drive it, with
`=` `-` `+` `_`, the wheel, `i`, `x` or a Roto knob, takes it over: from then that
fader of that track is yours, and the steps' values for it aren't heard. It moves
from where it was heard, so it doesn't jump. The others carry on following their
steps. A taken fader has `󰝁` after its value on the mixer and after its name on the
Sound page, and the status line says `taken over from the steps`. `Space e` gives
the cursor's track's faders back to the steps, which are heard at once where they
have them now, and `Space E` gives back every track's, as Live's Re-Enable
Automation does. A takeover isn't an edit, so `u` doesn't give it back, it isn't
saved, and opening a song gives every fader back. Playing from the top leaves it.

## Things that happen once

These have no mark.

| Command | Does |
|---|---|
| `DEL` | Delay this step by `xx` units |
| `KIL` | Cut the note after `xx` units |
| `OFF` | Start the envelopes' release after `xx` units |
| `RET` | Retrigger every `y` units, `x` fading or swelling each: see below |
| `CHA` | Play the note with a chance of `xx` out of `FF` |
| `CND` | Play the row only when condition `xx` holds: see **Conditions** |
| `ACU` | Move the row's note on each time it plays: see **Accumulators** |
| `ARP` | Go round the note, `x` up and `y` up: see **Arpeggios** |
| `RND` | Vary the note, or the lock to its left, by chance: see **Variations** |

A unit is a sixth of a step, so `DEL03` delays by half a step. The step is the
track's own, so on a track at `1/8` it's half an eighth. They count from where
the note starts, after the groove and the row's nudge, and so does `CHA`, which
goes with the note whichever column it's in.

A `RET` with `y` of `0` doesn't retrigger. Its `x` is its fade or swell: `0`
keeps every repeat at the note's level; `1` to `7` fade, each repeat `0.9` down
to `0.3` times as loud as the one before; and `8` to `F` swell, each `1.1` up to
`1.8` times as loud, never past full. So `x` builds on itself from hit to hit,
an even step in dB. It moves the level of each hit, not its velocity, and an
`Acid` ignores it: its hits all play at the note's own level.
Use `:seed 3A9F12C4` to choose which results `CHA` and `RND` give, or `:seed` alone
for a new one at random, as one undo step. The seed is hex, as the project page
shows it, in either case, so `:seed 42` is `0x42`.

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

### Moving every chance

`CDLT`, `Chance up or down`, moves every row's chance to play at once, as the
Metropolix's Probability and Elektron's track `PROB` do, so it's a density
fader for the whole track. It starts from the track's `Chance`, in the title of
the Sound page's Play device, at `80`, where everything plays as written.

Below `80` it scales every chance down: `40` halves each, so a `CHA 80` plays a
quarter of the time, and `00` plays nothing. A row with no `CHA` counts as one
that always plays and thins too, so a plain hat line loses notes as it turns
down, about as many as the turn says: `40` keeps about half. Above `80` it
closes the rest of the way to always: `C0` brings in about half of what each
`CHA` was missing, and `FF` plays every row, a `CHA 00` too. Rows keep their
order either way, so a ghost note at `CHA 40` still thins before the hat beside
it that always plays. It moves an `RND`'s chance digit the same way, so the
track's variations come more or less often with it.

A row decides with the same roll however `CDLT` is turned, so turning it up
only brings notes in and turning it down only takes them out, and a render
repeats. The rows that have no `CHA` are rolled from the `:seed`, the track and
how far it is into its chain, as `RND` is, so every `CHA` keeps the results it
had.

A row with a condition, `CND`, plays when its condition says, as Elektron's
conditions override `PROB`, though a `CHA` on it still moves. `PRE` and `NEI`
read what a row's `CHA` came to with its chance moved, so a `PRE` row after a
thinned `CHA` row is thinned with it: a flam stays a flam. A row with no `CHA`
that `CDLT` thins isn't a condition, and leaves what `PRE` reads as it was, so
turning the fader never changes which row a `PRE` follows. Like a failed
`CHA`, a thinned row skips only its note, and its locks still run.

`CDLT` on a row sets it for that row: `CDLT 00` drops the row's note whatever
its chance, and `CHA 00 CDLT FF` plays it. It's read before the row's locks
run, as `QNT` is, and a note there ends the last row's lock of it first. A lane
of it is a density pattern of its own, a mod slot or a macro can move it, and
the Roto has it on the Play device's second page.

The track's `Target`, after `Chance` in the title, `CTGT` on a step, is what a
row's chance decides, as the Metropolix's probability targets a stage or a
pulse. On `Row`, as a track starts, it decides the whole row: a `MLT` row's
pulses and a `RET`'s hits play together or not at all. On `Pulse`, each pulse
of a `MLT` row and each hit of a ratchet after the note rolls on its own, with
the row's chance, its `CHA`s moved by `CDLT`, or with no `CHA` what `CDLT`
keeps below `80`. So a `MLT4` row at `CDLT 40` loses some of its four notes
rather than all of them, and a `RET` stutters with gaps. The row's first pulse
decides as on `Row`, and feeds `PRE` as it would; a condition still decides the
whole row. The pulses and hits after it are rolled from the `:seed`, so a
render repeats and every `CHA` keeps the results it had.

`Space l` turns fill on or off, and held, it's on until you let go. The status
line says `FILL` while it's on.

### Skipping steps in a pattern

4ms' Shuffling Clock Multiplier has a `Skip` knob that drops beats from a clock in
a fixed pattern, more of them as it turns, giving more of a pattern than a clock.
`CHA 00`, a chance that never comes up, drops a row's note, and a live column of
them is that knob:

1.  On an FX column with nothing in it, select the rows, `vG`, and `S`. Choose
    `CHA` from the list.
2.  Set `From` and `To` to `00`, `Where` to `Chance`, and its `Amount` to how many
    rows to drop, then `L` to keep it live.
3.  `Space 8`, and `M` on the device's `Amount` puts it on a macro: the track's
    `Skip`, reaching that column in every phrase of the track that has one.

The rows it drops come from the generator's seed, so they're the same each pass,
and turning the amount up only drops more of them, keeping those already dropped.
`r` draws another pattern. `Euclid` in place of `Chance` drops evenly spread rows,
with `Hits` the amount, and `Clusters` drops them in bunches. With `Vary` on `Each pass` it drops
a new pattern every pass instead, which is `CHA` with a die on every row. A `CND`
drops whole passes, where this drops steps within one.

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
| `Limit` | `Wrap`, `Bounce`, `Hold`, `Random`, or a `bi` one for both limits |
| `Adds` | `Row` once a row, `Pulse` each pulse, `Retrig` each hit |
| `Reset` | `Chain`, `Play`, or every 1 to 16 loops of the chain |

The limits are 7 unless changed. With `Moves` at `Track`, every row on the track
moves with the `ACU`s, rows without one too, so `C D E F` plays `D E F G` the
second time. `Wrap` carries on from the note as written, `Bounce` turns back at
the limit and again at the note, and `Hold` stays at the limit. `Random` steps at
random, mostly the `ACU`'s way, by 1 to its amount, from the `:seed`. Each goes
between the note and the limit the `ACU` points to; the `bi` ones go between both
limits, past the note. `Pulse` adds on
every pulse of a `MLT` too, and `Retrig` on every hit of a `RET` as well. `Reset`
says when it goes back to the notes as written: `Chain`, wherever the chain
starts, or `Play`, only when playing starts.

A row that `CHA` or `CND` skips doesn't add, nor does a `RST`, and two `ACU`s on
a row add up. Playing from the top or from a step starts every accumulator from
the notes as written, and so does the chain starting, with `Reset` at `Chain`: a
section that brings it in or resets, a launch, a realign, `:reset` and the
track's own `Reset`. A phrase played twice
in its chain moves on each time.

`Limit`, `Adds` and `Reset` lock from a row, as `ALIM`, `AADD` and `ARST`, and a
macro, a lane or a mod slot can move them, a zone of its range for each choice.
A lock is heard from its row to the track's next note: `ALIM 04` on one row holds
that row's walk at the limit while the others wrap, and `AADD 01` adds on each
pulse of that row's `MLT`. `ARST` is read as its row plays, so a row locked to a
`Reset` other than the track's starts the accumulators again as it plays if they
were counting another way.

The grid shows notes as written. While playing, the row under the playhead shows
the note heard, before the chain's transpose, and the status line says it on the
Note column.

## Arpeggios

`ARP xy` goes round its row's note, the note `x` up and the note `y` up, as
Renoise's `-Axy` and the M8's `ARP` do, so `ARP 47` on a `C-3` plays C, E and G
over and over. With the scale on, `x` and `y` are scale degrees, as the keyboard's
are, so `ARP 24` plays a triad in any scale; with it off they're semitones. A `0`
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

`Mode` locks from a row as `AMOD`, and a macro, a lane or a mod slot can move it,
a zone of its range for each mode. The arpeggio takes its mode as its note
starts, so `AMOD 01` on a row plays that row's arpeggio down and the next note
goes back to the track's own.

It lasts as long as its note: through the row's pulses and any empty rows after,
until the next note, an `OFF`, a `KIL`, a rest or the note's gate closing. A
`HLD` carries it on into the note it ties into, going round from that note where
it was, with that row's own `ARP` if it has one. Any other note starts again with
its own `ARP`, or none. Each pulse of a `MLT` is a new note, so it starts again on
each. It starts where the note does, after the groove, the nudge and any `DEL`, and
an `ACU` or the chain's transpose moves it with the note. One voice plays it, a
note at a time, so it isn't a chord.

## Variations

`RND xy` gives what's to its left on the row a chance of varying each time the
row plays, as the Eloquencer's controlled chance does, so a line wanders a little
around what's written. `x` is the chance, `0` never and `F` always, so `8` is a
little over half; `y` is how far. Type its two digits after choosing it with
`K`; `=` and `-` move `y`, and `+` and `_` move `x`.

What it varies is what's in the column to its left, and in the first FX column,
the note:

| To its left | It varies |
|---|---|
| Nothing, or an event | The note, by 1 to `y` scale degrees |
| A lock, such as `CUT 40` | The lock's value, by up to `y` tens |
| A `RET` | Whether it ratchets at all, and its units |

A note that varies always moves, by 1 to `y` up or down, each as likely, in
semitones with the scale off, and a note outside the scale lands on the next one
in. `CUT 40 RND 83` plays `CUT` from `10` to `70`, never `40`, a little over
half the time. Beside a `RET`, `x` is the chance the row ratchets at all, as the
Eloquencer's ratchet probability, and `y` how far its units may vary when it does. A lock on a list, such as `PLY`
or `SMP`, moves by `y` choices, and one in semitones, such as `PTCH`, by `y`
semitones, so `SMP 04 RND F2` plays samples `02` to `06` but never `04`. For
velocity, put it after a `VDLT 80`, which leaves the velocity as written, so
`VDLT 80 RND 82` moves it up to `20` either way. Everything stays within its
range. A `GRV` or `RATE` is read ahead of its row, so an `RND` beside one does
nothing. Two `RND`s on a note add up.

Each roll is worked out from the `:seed`, the track, how far it is into its
chain and the column, rather than drawn in turn, so `CHA` rolls as it would
without it, a render repeats, and playing from a step hears what playing up to
it would. The chain starting again plays the same variations again, as a
condition's passes start again. Each pulse of a `MLT` is a new note, so it
rolls again on each.

It moves the note from where an `ACU` has it, without adding to the walk, and
an `ARP` goes round the note it moved to. A live generator's notes and values
vary as written ones do. While playing, the row under the playhead shows the
note heard, and the status line says it on the Note column. `Space g A` in
the phrase view writes the notes as last heard into the phrase, or the
selection, and takes their `RND`s out, so the pass you liked stays.

### Looping the dice

A track's dice can go round in a loop, as Marbles' Déjà vu does: with its
`Dice loop` on, in the title of the Sound page's Play device, each step's `CHA`s
and `RND`s, and the rows `CDLT` thins, roll the dice of a place in a loop of that many steps rather than
fresh ones, so a run of chances you like comes round again. It's `Off`, as
every track starts, then 1 to 16 steps, as Marbles' `Length` goes, then 32 and
64 for two and four bars of sixteenths. The places keep to the track's own count
of steps, so a loop of 16 keeps to the bar, and the loop is the track's, kept
across its notes, phrases and chains.

`Dice mutate` is the chance a place rolls afresh as it comes round: `00` locks
the loop, so the same steps play and the same notes vary the same way every
time round, `FF` rolls fresh dice every step, as with no loop, and between them
the loop slowly changes. `Dice jump` is the chance a step takes the dice of any
place of the loop instead of its own, so a locked loop plays the same dice out
of order, as Marbles' knob does past noon. A Steps slot's `Mutate` and `Jump` do
the same for its values, so one macro over both, zoned, is the whole of Déjà vu
for a track's events and values. See **Steps** in **The Mod page**.

The loop fills from the steps that play once playing starts, so playing from
the top again, or from any step, fills it afresh. Everything it rolls comes from
the `:seed`, the track and the step, so a render repeats. `DLP`, `DMU` and
`DJMP` on a row set them for the step that plays it, read before it lands, as
`JMP` is, and a lane, a mod slot or a macro moves them too. With `Dice mutate`
at `FF` an `RND` rolls just as it does with the loop off.

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

`QNT` brings the rows' nudges toward the grid as they play, leaving them written as
they are: `00` plays them as written, `80` halves each and `FF` plays every row on
the grid, so a take recorded loose can be tightened, or loosened again, without
losing what was played. Like `GRV`, it's read before the row lands, so a lock on
a row moves that row's own note, and a lane of it can tighten every other step. A
mod slot's `Dest` lists it too. It starts from the track's `Quantise`, on the Sound
page's Play device, which the Roto has as a knob.

`JMP` is the chance the step plays any row of the phrase instead of the row its
order walks to, from `00`, never, to `FF`, every step. It's read before the row
lands, as `QNT` is: a lock on a row sets the chance for the step that walks to
that row, and the row jumped to then plays with all of its own FX. It starts from
the track's `Jump`, in the Play device's title. See **Order** in **Editing**.

`ORD`, `REV`, `ORDX` and `ORDY` move the walk of the phrase's rows as the Gen
page's `Phrase` device does, read before the row lands, as `JMP` is: a lock on a
row moves the step that walks to that row. `ORD` goes so many places on through
the list of orders from the phrase's own, `80` as written; `REV` turns the walk
the other way up; `ORDX` and `ORDY` are the order's numbers, `00` its own.
`PLEN` and `PSTA` are the window of rows the order walks, read the same way:
`PLEN` rows more or fewer than the loop's, `80` as written, and `PSTA` the row it
starts on. `CUE` `01` has a new window, and a lane's new length, rate or start,
wait for their loop to come round. See **Order** in **Editing**.

## Speed

A track's `Speed`, in its Sound page's Play device, is how long each of its steps is: `1/32`,
`1/16T`, `1/16`, `1/8T`, `1/16D`, `1/8`, `1/4T`, `1/8D`, `1/4`, `1/4D`, `1/2` or
`1/1`, the song's `1/16` unless changed. `D` is dotted, half as long again, as an
LFO's `1/8d` is, so `1/8D` is three sixteenths; `RATE`'s values count through the
list in that order, `05` being `1/8`. A track at `1/8` plays its rows half as fast, and everything on it
follows its step: its pulses, its lanes, `DEL`, `KIL`, `OFF` and `RET`, its
groove's pushes and its notes' `Gate`. A nudge is the same time on every track,
in 24ths of a sixteenth, and goes further on a slower one: see **Note entry**. Sections, `:reset`, launches
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

### Rotating speeds

`Space o` rotates the speeds round the tracks of the cursor's group, as the Rotate
jack of 4ms' Shuffling Clock Multiplier passes its outputs' multiplications along:
each track takes the next one's `Speed`, and the last the first's. On a track in
no group it says so; in Visual, after `W` has reached the tracks you want, it
rotates those, and `Space O` rotates every track that plays sounds.

| Before | `Space o` | Again | Again |
|---|---|---|---|
| `1/16` `1/8` `1/8T` | `1/8` `1/8T` `1/16` | `1/8T` `1/16` `1/8` | `1/16` `1/8` `1/8T` |

So as many presses as tracks bring them back, and a count, `2 Space o`, turns
that many at once. Each press is one undo step, heard from each track's next step,
as a change on the track page is.

While recording, `Space r`, a press writes the rotation into the phrases rather
than the tracks: each track's next row gets a held `RATE` of the speed it takes, in
the FX column holding a `RATE` already, or else the first empty one, so the pass
plays it back where you pressed, from the tracks' own speeds at its start. A
track whose next row has no FX column free is left as it is, and the status line
names it. The `RATE`s go into the pass's one undo step.
