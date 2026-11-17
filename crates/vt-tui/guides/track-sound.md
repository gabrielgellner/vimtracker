<!-- ANCHOR: before-folded -->
# The Sound page, output and groups

Press `Space 5` to `Space 8` for what a track holds apart from its steps, on four pages: Sound
has its machine, filter, output and how it plays, with a bar beside each value showing where
it sits between the lines at each end of its range (a centred one, like Pan or Trim, with a faint
`┊` at its middle), and moving with whatever
plays it, with a dim mark left where it's set once it's a cell or more away, Mod its six mod slots,
envelopes and LFOs, its accumulator and arpeggio, FX its effects, and Gen the live
columns of the phrase under the cursor, on `Space 5`, `Space 6`, `Space 7` and `Space 8` in that order. `H` and `L` go to the previous or next page, each with the cursor where you left it, or on its first setting the first time. FX columns belong
to steps, and an LFO keeps moving on its own, like a filter drifting under a
hat pattern.

The page's heading, and the phrase view's, starts with what the track is: `󱑽` a
sampler, `󰙽` a synth, `` a drum, `󰃸` a group,
`󱞳` a return and `󰓃` the master. The machine list shows the same beside
each machine.

| Keys | Does |
|---|---|
| `H` `L` | The previous or next page, back where you left it |
| `W` `B`, `]b` `[b` | Next or previous track |
| `]g` `[g` | Next or previous group, folded or open |
| `j` `k` | Next or previous row, back in the column they left |
| `gg` `G` | The top or bottom of this column of devices |
| `{` `}` | Previous or next device, on the same row and column. `2}` goes two |
| `h` `l` | Previous or next setting. `0` and `$` go to the line's ends |
| `/` | Jump to a setting by its name |
| `=` `-` | Up or down a small step. A count goes further, as in `10=` |
| `C-a` `C-x` | The same as `=` `-` |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `D-l` `D-h` | Cmd and `l` `h`: as `=` `-`, without moving |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_`, without moving |
| `i` | Type a value. Hex levels enter themselves, others take `Enter` |
| `K` | On a setting with choices, like Machine or Dest, list them |
| `x` | Back to its default: centre pan, 0 dB and so on |
| `dd` | To nothing: its lowest, off, its centre or its first choice |
| `b` | Switch an FX slot or a mod slot out, or back in |
| `;` | `a` `w` `s` `e` `d` `f` play C to F, `<` `>` the octave, until `;` |

A count before `=` `-` `+` `_` goes that many steps. On a value each is its step, so `10=`
moves a hex value `0A`. On a list, `=` and `-` move one choice and `+` and `_` sixteen, so `3=`
moves three choices: the track's lists, Machine, the filter's Type and Mode, Role, Speed, Groove,
a slot's effect and an effect's lists, a machine's own, such as the 6-op's patches, a mod slot's
and the Gen page's all go so. Machine, Type, Mode, Role, a slot's effect and a mod slot's lists go
round past either end with `=` and `-`; `+` and `_` stop at the first or last, as every key does
on Speed, Groove and a machine's own lists.

`dd` sets a setting to nothing, where `x` puts it back to its default: a level, a time or an
amount, such as a `Sustain`, to its lowest, `00` or 0 ms; a switch off; a setting either side of
a middle, such as `Pan`, `Trim`, a `Depth`, a tune or cents, to its middle, not its end; and a
list, such as the filter's `Type`, to its first choice. It's one undo step, it records like any
other change, and the status line says what it set, as `Sustain to 00`. The machine and a mod
slot's type say what a device is, so `dd` leaves them, and a mod slot's `Rate`, which isn't one
number, too; the status line says so. `d` alone waits for the second `d`, and `Esc` drops it.

A note holds for as long as its key is down, and several keys down play a
chord, up to eight notes; a ninth takes over the oldest. Letting go of one key
ends only its note. `Acid` plays one note at a time, as a 303 does: a key
pressed while another is down slides to its note, and letting go of it slides
back to the one still down.

Cut, Res, Pan and the machine's settings that sweep have a fine part, the dim
pair of digits after the value, as FX values do in a phrase. `l` stops on it
before the next setting, and there `=` `-` `+` `_`, `i` and `x` work on the fine
part alone, carrying into the value past `FF` or below `00`. The value's own
digits keep it, and at the top there's none. What's on the level scale, `Trim`,
`Level` and the sends, is two digits, as a quarter of a dB is its step, and so
are an effect's settings.

## The Sound page

A sound track's Sound page is four devices, one under the other, as the FX page's
slots are: each a frame with its name and what gives it its character in its title,
and its settings in up to four columns of three.

| Device | Title | Settings, column by column |
|---|---|---|
| Machine | Machine, type | The machine's own; a sample's `Tune` `Cents` |
| Filter | Type, mode | `Cut` `Res` `Drive`; `Punch` `Safe` `Key track`; |
| | | `Low cut` `High cut` |
| Output | | `Trim` `Level` `Pan`; `Send A` `Send B` `Group`; `Role` |
| Play | | `Pitch` `Glide` `Gate`; `Speed` `Groove` `Reset`; |
| | | `Choke` `Velocity` `Quantise`; |
| | | a synth's `Phase`, then `Mode` `Glides` |

A machine's type is the setting that changes its character: the sample the sampler
plays, `Acid`'s `Wave`, `Supersaw`'s `Stack`, 6-op's `Patch`, and the chord of `Chord`
and `Chip`. Other synths have none. `K` lists whatever's under the cursor in a title, as
on a setting, and `g d` on the sample opens the sample page, as `g d` on a setting a macro
holds goes to the macro (see **Macros** in **The Gen page, macros and presets**).
Each machine places its own settings: most go down the columns in order, the sampler has `Slice` and `Play` in
one column, `Start` and `Length` in the next, and its sample's own `Tune` and `Cents`
in the third, which tune the sample rather than the track (see **Tuning a sample** in
**Sounds, samples and kits**), `Acid`'s are in pairs as a
TB-303's knobs are, `Cutoff` and `Resonance`, `Env mod` and `Decay`, `Accent` and
`Slide`, then `Drive` and `Drift`, and `Supersaw` has its stack's spread, `Saws`,
`Detune` and `Warp`, down its first column, how it sounds, `Mix`, `Shape` and `Width`,
down its second, and `Phase` and `Drift` in its third.

A machine with more settings than one device holds has more devices after its own.
`4-op`'s type is its `Algorithm`, and its Machine has the settings its operators share;
under it are `Op 1` to `Op 4`, each an operator's pitch and level, then its sound, then
its envelope in pairs. With them the page is taller than the screen, so it goes down by
whole devices to keep the cursor's in view, and `j` and `k` go from one to the next as
from any device.

`j` and `k` go from a device's title down its rows and on into the next device,
keeping the column, and pass the Output's and Play's titles by, as there's nothing
in them. `{` and `}` keep the row and column: from `Cut`, `}` goes to `Trim` and
`2}` to `Pitch`. The device with the cursor has the brighter frame. A sampler's
Machine shows the slice it plays in a box on its right, with a teal line where `Start`
puts a note's start and a red one where `Length` ends it.

The Output's box is a meter of the track right after `Trim`, before the filter, so
`Trim` can be set by eye: `L` and `R` bars from -60 dB to +6 dB, each side's peak,
with a faint mark at the highest it reached lately. The mixer's meters are after the
fader.

A group's, a return's and the master's Sound page is the same Output, with a gap
where it lacks a setting, and under it a device of what feeds it, which the cursor
doesn't stop in:

| Track | Output | Under it |
|---|---|---|
| Group | `Trim` `Level` `Pan`; `Send A` `Send B` | Tracks, those in it |
| Return | `Trim` `Level` `Pan` | Sends in, with each send |
| Master | `Trim` `Level`; `Limiter` | |

Both lists follow the song as it changes, three down each column, and past 12 the
last says how many more there are. A group's and a return's meter reads after `Trim`,
before their slots. The master's Output has no meter, as the main meter beside it
shows what it plays.

## The filter

| Setting | Value |
|---|---|
| Cut, Res | Where `CUT` and `RES` start |
| Drive | How hard it's driven, from `00` for clean |
| Punch | How high a loud ring starts over `Cut`, from `00` for none |
| Safe | On, `Res` stops short of ringing by itself |
| Key track | On, `Cut` follows the note: an octave for an octave |
| Filter | `LP` `HP` `BP` `Notch` `Ladder` `Diode` `MS20 LP` `MS20 HP` `Dirty` `OB-X` |
| Mode | How the type responds: `--` where it has none |
| Low cut, High cut | The utility filter: `00` and `FF` are off |

Drive keeps the level about even. A step can lock it, `FDRV`, and macros and
modulators move it, as they do `Cut`. `Punch` is Mutable Instruments' Braids' trick
for an 808 kick: as the filter's low-pass swings, its cutoff rises with it, so a
loud ring starts higher and falls in pitch to `Cut` as it dies away, and a loud
peak is reined in a little. Strike `LP` or `BP` with something short at a low
`Cut` and high `Res` to hear it: at `FF` a ring at 50 Hz starts at about 200 Hz.
It's added in Hz, so it moves a low cutoff much more than a high one, and a quiet
sound hears the filter as it is, so the curve doesn't show it. Only `LP`, `HP`,
`BP` and `Notch` have it; on the other types it's dim and does nothing, and it
does less with `Drive`, whose clipping holds the ring down. A step can lock it,
`FPCH`, and macros and modulators move it.

`Safe`, beside `Res`, is the switch of that name on Cytomic's The Drop. On, the
whole of `Res`'s turn reaches only as far as the loudest peak that still dies
away, a few steps short of where the type starts to sing by itself, so `FF`
rings on after a strike and fades, and never keeps going alone, at any `Cut`,
`Mode` or `Drive`. Its steps are closer together for it: `FF` is about where
`D7` was, and `80` where `6C` was. The curve shows the peak it leaves. `K`
switches it, `=` puts it on and `-` takes it off, and a step can lock it, `SAFE`
`01` on and `00` off, for its row. A macro can switch it, and modulators don't,
as it's a switch. A `RES` lock or an LFO on `Res` stops short too.
The clean types never ring by themselves, so on `LP`, `HP`, `BP` and `Notch`
it's dim and does nothing.

`Key track`, under `Safe`, moves the cutoff with the note, as most synths' filters
can: an octave up in the note is an octave up in the cutoff, and `C-3` leaves it
where `Cut` is. Without it the cutoff stays put while the notes move, so a
resonant peak a low note sits in is left far above the notes an octave higher,
and a run up the keys gets quieter and duller as it goes. With it each note meets
the filter as `C-3` does, so the same `Cut` and `Res` sound alike all the way up.
It follows the note as it sounds: after the chain's transpose, with `Pitch`, a
glide or `Acid`'s slide as far as it's gone, and what modulation does to `Pitch`,
but not a sample's own tune. A new note's cutoff moves there as a change of
`Cut` does, quickly but without a click, and a glide takes it along. It adds to
`Cut`, `CUT` locks and what modulates `Cut`, and stops at the filter's ends.
`K` switches it, `=` puts it on and `-` takes it off, and a step can lock it,
`KTRK` `01` on and `00` off, for its row. The curve follows it as the notes play.

`Low cut` and `High cut` are a second, simple
filter after the main one, for thinning out a sound without giving up the main
filter's character. They run from 20 Hz at `00` to 20 kHz at `FF`, at 12 dB with
no resonance. `Low cut 00` and `High cut FF` are off, and off costs nothing.
As with `Cut`, steps lock them, `LCUT` and `HCUT`, and macros and modulators
move them, each with a fine part.

`K` on `Filter` lists the types. `LP` is the clean default. `HP` needs `Cut` lowered to let anything through, as
it starts at `FF`. `Ladder` is fatter and steeper, its bass thins as resonance
rises, and near `FF` it sings by itself while a note lasts. `Diode` is the
303's filter, the one in `Acid`: rubbery and softer at first, as it starts
falling well under `Cut`, with a peak at `Cut` that grows sharp and squelchy
and thins the bass more than `Ladder` does. `MS20 LP` and `MS20 HP` are the
Korg MS-20's filter, at 12 dB and 6 dB. With much `Res` their peak is at
`Cut`. They keep the bass, or for `MS20 HP` the top, however high `Res` goes,
so they get louder and harsher as it rises, and near `FF` they scream by
themselves, much louder than `Ladder`. `Drive` makes them grittier. `Dirty` is
Vital's: 24 dB, as `Ladder` is, but its resonance comes round a loop that
saturates. It thins the bass as `Res` rises, though less than `Ladder` does,
and it sings by itself sooner, from about `DB`, at `Cut`. A loud sound comes
out a little crunched even at `Drive 00`. `Drive` grits up what comes straight
in, less so as `Res` rises so the peak isn't swamped, and adds a little
resonance. `OB-X` is the Oberheim OB-X's 12 dB filter, after OB-Xd's: it
sounds as the clean `LP` does when quiet, but its damping grows as it gets
louder, so its resonance holds itself in with no clipper, and loud notes tame
it, the OB's squelch. From about `D9` it sings by itself at `Cut`, in tune, so
it plays as an oscillator. These six cost more than the others, which matters
only for tracks using them.

A step can lock the type, `FTYP`, by its place in the list: `00` `LP`, `01`
`HP`, `02` `BP`, `03` `Notch`, `04` `Ladder`, `05` `Diode`, `06` `MS20 LP`,
`07` `MS20 HP`, `08` `Dirty`, `09` `OB-X`. A new type starts from silence, so
while something's playing through the filter the one it leaves carries on and
fades out under it over about 10 ms, and a lock doesn't click. A macro can choose
the type too; modulators don't, as types are chosen rather than moved through.

`Mode` gives a type its other responses, made from what it already has, and `K`
on it lists the ones the type offers:

| Filter | Modes |
|---|---|
| `LP` `HP` `BP` `Notch` | `12 dB`, or `24 dB` with a second filter after the first |
| `Ladder` | `LP24` `Fat24` `LP18` `LP12` `LP6` `BP24` `BP12` `HP6` `HP12` `HP18` `HP24` |
| | `N24` `N12` `PH18` `HP12+LP6` `HP18+LP6` `N12+LP6` `PH18+LP6` |
| `Dirty` | `LP24` `LP12` `BP24` `HP12` `HP24` `N24` |
| `OB-X` | `LP12` `BP12` `HP12` `N12` |
| `Diode` | `Full`, or a low cut `-3 oct` `-2 oct` `-1 oct` under `Cut` |

At `24 dB` a low-pass or high-pass is twice as steep with the same peak, a
band-pass is narrower and a notch wider. `Ladder` and `Dirty` mix their stages,
so whichever mode they're in, their resonance still sings at `Cut` and they
still saturate. The number is the slope in dB an octave: `BP24` falls at 12 dB
either side and `BP12`, wider, at 6. `N24` is Vital's notch and `N12` a
narrower one. The ladder's extra modes are the Oberheim Xpander's: `PH18` is
its phaser, three stages' shift against the input, which dips at about 1.7
times `Cut` and thins the bass, and the four with `+LP6` add a gentle low-pass
to a high-pass, notch or phaser, so `HP12+LP6` and `HP18+LP6` are lopsided
bands. Each band peaks as loud as `LP24` is at its loudest. `Fat24` is the
ladder's `LP24` with much of the bass its resonance takes given back, as
Surge's compensated ladders do: about 10 dB more near full `Res`, so the peak
rides on a full bass, and runs into its saturation sooner. The ladder's notches
take out the very pitch it sings at, so they hardly sing at all, where
`Dirty`'s still does. `OB-X`'s modes are the low-pass, band-pass and high-pass
of its one filter, and its notch the low-pass and high-pass together, so its
`BP12` and `HP12` sing as its `LP12` does and its `N12` hardly at all. Its
`BP12` peaks at 0 dB with no resonance, and as loud as `LP12` is at `Cut` once
it has some. `Diode` at `Full` has all its
bass, as in `Acid`. The others cut the bass before the ladder, from that many
octaves under `Cut`, moving with it, so `Drive` grits up only what's left. The
MS-20's have no modes, and show `--`.

A track keeps its mode when its type changes, and a type that doesn't offer it
plays its own first, so going to another type and back finds it again. A step
can lock `MODE` to any mode, by its place in this list, which the status line
names: `00` `12 dB`, `01` `24 dB`, `02` `LP24`, `03` `LP12`, `04` `BP24`, `05`
`HP12`, `06` `HP24`, `07` `N24`, `08` `Full`, `09` `-3 oct`, `0A` `-2 oct`,
`0B` `-1 oct`, then the ladder's own, which `OB-X` shares two of: `0C` `LP18`, `0D` `LP6`, `0E` `BP12`, `0F`
`HP6`, `10` `HP18`, `11` `N12`, `12` `PH18`, `13` `HP12+LP6`, `14` `HP18+LP6`,
`15` `N12+LP6`, `16` `PH18+LP6`, `17` `Fat24`. Changing mode glides over about 10 ms, so a lock doesn't click.

A curve in a box at the right of the Filter shows what the filter and utility filter
let through, from 20 Hz on the left to 20 kHz on the right, in the track's
`Mode`, over faint lines at 0 dB, 100 Hz, 1 kHz and 10 kHz, with a dashed line up to
it at `Cut` unless the filter is open. It moves with what's playing `Cut` and `Res`, including step commands,
LFOs and envelopes, but it doesn't show `Drive`'s saturation. For `Dirty` it shows the resonance `Drive`
adds.

## Jumping to a setting

Press `/` and type part of a setting's name, like `cu`, and each match on this page
gets a label over its value, and again in its name, just after what you've typed. Press a label to land on
that value, ready for `=` and `-`. `Enter` goes to the nearest match, `Esc` stays where you were, and `C-o`
comes back after a jump.

A label is never a letter that could carry on your search, so you can keep typing to
narrow it, and a match keeps its label as you do. `/` searches the page you're on,
and `H` and `L` change page.

## Output and Play

`Trim`, at the top, is the gain at the track's input, for gain staging, so `Level`
is left for the mix, as a console's input trim and fader are. It's a quarter of a dB a
step either side of `80`, which is 0 dB: `FF` is +31.75 dB, `01` -31.75 dB, and `00`
is off, which silences the track. It comes straight after the machine, before the
filter, so turning it up drives `Drive` and the slots harder. Steps and modulators
don't move it, and a change glides as `Level`'s does. The status line reads it in dB;
`=` and `-` move it a quarter of a dB, `+` and `_` 4 dB, and `x` goes back to 0 dB.

`Level`, `Pan`, `Send A` and `Send B` are the track's faders, and where `LEV`,
`PAN`, `SNA` and `SNB` start when playback starts from the top. `Level` scales the
whole track. While a step drives one, it shows the step's value, in the playing
colour, and its bar is filled to it, with what `VOL` and modulation make of it a
thin mark. Changing one by hand then takes it over from the steps, marked `󰝁` after
its name, until `Space e` gives it back; see **Fader automation and relative
modulation** in **FX commands**.
Dimmed `Groove` is the project's, which this track follows. Choosing one with `K`,
or moving through them with `=` and `-`, gives the track its own, and `x` follows
the project again.

`Pitch` moves every note on the track, a semitone a step either side of `80`,
as `PTCH` on a step does for that step. On a sampler it's on top of the sample's
own `Tune`, in the Machine: tune the sample there once, and transpose with
`Pitch`. `Choke` puts the track in a group, `1`
to `16`, or `Off`: a note on it cuts off what's sounding on the other tracks in
the group, as a closed hat cuts an open one. A kit sets both, with the level.

`Gate` is how much of its last pulse each note lasts before it's let go, from
`1%` to `100%`, unless its row has a length of its own in its gate cell. `LEN` on
a step sets it for the rows that don't, and an LFO on it moves every note's
length up or down, a row's own too. `=` and `-` move it a
percent, `+` and `_` ten, and `i` takes a percent. Dimmed, it's the machine's
own: `100%`, which holds a note until the next, or `50%` on `Acid`, as a 303's
gate closes halfway through each step. `x` goes back to that. See **Note entry**
for what it does to each gate.

`Velocity` moves every note's velocity up or down from its own as it starts, a
step of velocity a step either side of `80`, which leaves them, and kept from `00`
to `7F`. It's where held `VDLT` starts, so a step's `VDLT` or a lane's replaces it
and an LFO on `VDLT` moves around it, and on the Roto it's the Play device's last
knob, for playing dynamics live. The notes keep what's written. On `Acid` it only
moves notes across the accent at `70`. See **FX commands**.

`Quantise`, under it, brings every row's nudge toward the grid as it plays, from
`00`, `0 %`, which plays them as written, to `FF`, `100 %`, which plays every row
on the grid; `80` halves them. The nudges stay as they're written, so a
recorded take can be tightened live and loosened again. It's where held `QNT`
starts, so a step's `QNT` or a lane's replaces it and an LFO on `QNT` moves around
it, and on the Roto it's the first knob of the Play device's second page. See
**Nudging a note off the grid** in **Note entry**.

`Jump`, in the Play device's title, is the chance each step plays any row of
its phrase instead of the one the phrase's `:order` walks to, from `0 %`, the
order as it is, to `100 %`, any row every step. It's where held `JMP` starts, so
a step's `JMP` or a lane's replaces it and an LFO on `JMP` moves around it, and
on the Roto it's on the Play device's second page. See **Order** in **Editing**.

`Chance`, after `Jump` in the title, moves every row's chance to play up or
down, a density fader for the track, from `00`, nothing plays, through `80`,
everything as written, to `FF`, every row plays, a `CHA 00` too. Below `80` it
scales every chance down, rows with no `CHA` too, so `40` plays about half the
notes; above it, it brings in the rows each `CHA` drops. It's where held
`CDLT` starts, so a step's `CDLT` or a lane's replaces it and an LFO on `CDLT`
moves around it, and on the Roto it's on the Play device's second page after
`Jump`. See **Moving every chance** in **FX commands**.

`Phase`, in a fourth column of a synth's Play device, is whether its oscillators run
on from note to note or start again on each. A synth that holds its notes plays the
next one on the voice already sounding (see **Envelopes** in **The Mod page**). At
`Free`, as a track starts, they run on, as an analog synth's do, so each note starts
wherever they've got to, even after the last note has died away, as a pluck's does
before the next: its voice waits, quiet, and the next note on the same machine takes
it up again where its oscillators stopped. At `Reset` they start again from where a new note starts
them, as Elektron's oscillator reset, Operator's retrigger and Serum's retrig do, so
every note at the same pitch starts the same. That can only happen at silence, so
each note with `Reset` takes the short dip to nothing before its attack, whatever the
notes' shape's own `Reset` says. `Drift` on `VA`, and the `Supersaw`'s `Phase`, still
start each note's oscillators anywhere in their cycles. A step's `OSC` sets it for its
note, and on the Roto it's the second knob of the Play device's second page. A
noise, a struck or blown model and `Acid` have no phase to start again, so it's
dimmed there and does nothing, and a sampler, which starts its sample again on each
note anyway, hasn't one.

`Mode`, under `Phase`, or alone in the sampler's fourth column, is how the track's
notes join. At `Mono`, as a track starts, each note is struck, as described under
**Envelopes** in **The Mod page**: a synth that holds its notes carries its voice on,
with the notes' shape's `Reset` deciding whether it dips first, and anything else
starts under the last note's fade. `Legato` is `Mono` with one rule more: a note
that arrives while the one sounding still has its gate open, on the same synth, or
the same sample and slice, ties into it as a `HLD` does. Its pitch moves, gliding by
`Glide` if that's set, it takes the new velocity, and nothing is struck again: no
envelope or mod slot starts over, and a struck voice rings on at the new pitch. A
note that comes after the gate has closed, by the row's `LEN`, the track's `Gate` or
an `OFF`, is a `Mono` note.

Mind the gate. At the full `Gate`, `100%`, that every machine but `Acid` starts
with, each note is held until the next one comes, so in `Legato` every note ties
into the next: a whole phrase plays as one note sliding from pitch to pitch, with
one attack at its start. To strike notes, let the gate close before the next one:
lower the track's `Gate`, `80` or `50%` for a 303's half step, to strike them all;
give a row a length, such as `SGL1 80`, to strike only the note after it; or put an
`OFF` between two notes. A row left at the full length still ties into the next, so
mixing the two gives slides and strikes in one line, as a 303 does. `Acid`'s own
`Gate` lets each note go, so it plays as in `Mono` until a note is held into the
next, which then slides. A `RET`'s
ratchets always strike, as they're there to be heard, and so do a `MLT`'s later
pulses, and an arpeggio with `Retrig` on. `=` and `-` move through the list, `K`
and `i` choose from it, and `x` goes back to `Mono`, which is dimmed. A step's
`VMOD` sets it for its note, and on the Roto it's on the Play device's second page,
after `Quantise` and a synth's `Phase`. How the gate, `Mode`, `HLD`, `Glide` and
`Reset` fit together, with examples, is **How a note plays** in the manual's
chapter **How a track plays**.

`Speed` is how long each of the track's steps is, from `1/32` to `1/4` with the
triplets between, so one track can play eighths or triplets against the others'
sixteenths. `=` and `-` move through the list, `K` and `i` choose from it, and
`x` goes back to `1/16`, which is dimmed. Its rows, pulses, groove and timing
commands all follow its step. `Space o` rotates the speeds round a group's tracks,
each taking the next one's. See **Speed** in the FX guide.

`Glide` is how long a `HLD` tie glides into the next note, from 2 ms to 2 s, on
every voice: a synth by its pitch, a sample by its speed. `Off`, dimmed, jumps, or
on `Acid`, shown as `Slide`, slides by the voice's own `Slide`. `=` and `-` move it
a step, `+` and `_` sixteen, `i` takes a time in ms or `off`, and `x` turns it off.
`GLD` on a step sets it for that tie. See **Note entry**.

`Glide mode`, `Glides` on the Play device under `Mode`, says which notes `Glide`
slides into. At `Ties`, as a track starts, only a tie glides: a `HLD`, a `Legato`
tie, and an arpeggio's notes with `Retrig` off. At `Always`, every note glides from
the pitch of the note still
sounding, as a tracker's `3xx` and a mono synth's portamento do: a struck note
still strikes, its envelopes starting again as `Mode` and `Reset` say, and only
its pitch slides, over `Glide`'s time. A note after silence, the first note, and
a note on another sample or slice have nothing to glide from, so they start at
their own pitch, and with `Glide` off nothing glides. A `RET`'s ratchets, a `MLT`'s
pulses and an arpeggio with `Retrig` on glide too, as each is a struck note; a
ratchet on the same pitch just carries on a glide still under way. `Acid` slides
by its own rules alone, so it's dimmed there and does nothing. `=` and `-` move
through the list, `K` and `i` choose from it, and `x` goes back to `Ties`, which
is dimmed. A step's `GMOD` sets it for its note, and on the Roto it's on the Play
device's second page, after `Mode`. How it fits with the rest is **How a note
plays** in the manual's chapter **How a track plays**.

`Reset` is how many of the song's steps the track's chain plays before it starts
again from its top, counted from where it last started, so a short part can keep to a
bar while the others drift. `Off`, the default, plays on. `=` and `-` move it a step,
`+` and `_` to the next or previous whole bar, `i` takes a number of steps up to 1024,
bars as `2bar`, or `off`, and `x` turns it off. For a one-off, `Space a` starts the
chain again at the next line of the launch grid: see **Jamming** in **Phrases and
chains**.

## Track colours

What a track is for is a thin rule under its name, in a colour of its own, so drums,
bass and synths stand apart at a glance: in the song view's columns, on the mixer's
strips, in the phrase view's columns and titles, in the track list (`Space f b`) and in
the track page's title. The name keeps its own colour, a group's, so the rule says what
a track is and the name's colour which group it's in.

| Role | Colour | What it's for |
|---|---|---|
| `Drums` | maroon | Drums and percussion |
| `Bass` | yellow | A bass line |
| `Synth` | sky blue | A lead, an arpeggio or a melody |
| `Pad` | lavender | Chords and pads |
| `Texture` | pink | Noise, risers and atmosphere |
| `None` | none | Nothing to say |

`Role`, at the end of the Output, sets it. It starts on `Auto`, which goes by the
machine and shows what that makes it, dimmed: the drum voices are `Drums`, `Acid` is
`Bass`, `Chord` and `Paraphonic` are `Pad`, the noise voices are `Texture` and every
other voice is `Synth`. A sampler could be playing anything, so it has no colour until
you say, but a kit's sounds are `Drums`, whether `:kit` puts them on tracks or `:sample`
chooses one for a track left on `Auto`. `K` lists the
roles in their colours, `=` and `-` go through them, and `x` goes back to `Auto`.

A group takes the colour its tracks all share, so a folded kit still reads as drums;
with tracks for different things, and on the returns and the master, the rule is grey,
as they play nothing of their own.

## Groups

A group sums tracks into one, as Ableton's group tracks do, so a whole kit can go
through one compressor: `Comp` with its `Type` on `Console` or `Glue` in a group's slot is the glue.
Every song has four, `G1` to `G4`, after the tracks and before the returns.

`:group 1 1-4` puts tracks 01 to 04 in group 1, `:group 2` puts the cursor's track in
group 2, and `:group off` takes it out of any, or `:group off 1-4` a range. Each is one
`u`. The Sound page's `Group`, under `Send B`, does the same for one track: `=` and `-`
go through `G1` to `G4`, `i` takes a number or `off`, and `x` takes it out.

`:group name drums` names the cursor's group, or the group the cursor's track is in,
`DRUMS`: up to five letters or digits, shown whole in the song view, on the mixer, in
the phrase view's columns and in the track page's title, `Group 1 DRUMS`. `:group name`
on its own gives it back its letters, `G1`. Each is one `u`. With the cursor on the
group's own track, `:name drums` does the same, while on a sound track `:name` keeps
to two characters.

A track in a group plays through it, and the group plays through its own slots,
`Level` and `Pan` to the master. Its sends still go from the track itself, so a snare
can go to the reverb on its own, and the group has `Send A` and `Send B` of its own
too. A group's Sound page has an Output with `Trim`, `Level`, `Pan` and its sends,
where a track's are, and under it Tracks, the tracks in it, in its colour, each with
its own mute or solo. Its `Trim` is at its input, so it sets how hard the sum of its
tracks hits its slots. The rest is as on a return.

A group's macros can hold settings on its tracks as well as its own, so one knob turns the
cutoff on every drum, or `Hits` on each of their generators: `M` on a track's setting lists
the group's macros after the track's own. The Gen page's guide has the details, under
Group macros.

Each group has a colour: its icon, its strip on the mixer and the `G1` under each of its
tracks' strips share it, as does a track's `Group` on its Sound page. Its tracks are
named in it wherever tracks are listed: the song view's columns, the phrase view's
columns beside the grid, and the track list. The chain view, the phrase view and the
track page put `G1` after a grouped track's name.

### Folding a group

A group can fold its tracks into itself, as Ableton folds a group track, to make room
and to get past a kit in one move. The tracks keep their order; folding only hides
them, and the group's name gets a mark, two arrows pushed together. The keys work in
the song view, the chain and phrase views, on the track pages and on the mixer.

| Keys | What they do |
|---|---|
| `z a` | Fold this group, or the one this track is in, or open it |
| `z M` | Fold every group |
| `z R` | Open every group, the empty ones too |
| `]g` `[g` | Next or previous group, folded or open |

A folded group is one stop for `W` and `B`, and `]b` and `[b`: they land on the group
itself, its Sound page with its tracks and output and its FX page, where its tracks
would be, then go on to the track after them. The phrase view's columns beside the
grid show it there too, a column of its own, named and empty, in place of its
tracks'. In the song view and on the mixer, the folded group keeps its own column or
strip after the tracks, and the cursor goes along them as they're drawn, stepping
over the folded tracks with `h`, `l`, `0` and `$` as well.
<!-- ANCHOR_END: before-folded -->
<!-- ANCHOR: rest -->

Folding a group from one of its tracks puts the cursor on the group, as Vim's cursor
stands on a closed fold; in the song view and on the mixer, so does putting the
cursor's track in a folded group. Folding it from another view leaves the phrase or
page you're on in front until you move off it. Choosing a folded track from the track
list (`Space f b`) opens its group again.

Loading a kit's sounds from `:kit` puts them in a new group, named after the kit and
folded (see **Starting from a kit** in **Sounds, samples and kits**).

A group with no tracks in it is folded away, so it's left out of the song view and the
mixer, and the cursor goes past it. It comes back as soon as a track joins it, or with
`z R`, faint while it's empty.

Folds are saved with the song, so a project opens as you left it. They aren't changes
to undo: `u` never opens or folds a group, but folding marks the song changed, so `:w`
and the autosave keep it.

Muting a group mutes its tracks. Soloing a track in a group keeps the group, so you
hear it through the group's slots, and soloing a group plays all its tracks. A track
muted or soloed by its group shows the glyph in the group's colour, where one of its
own is green. A group can't be in a group.

## Returns and the master

After the groups, every song ends with three more fixed tracks, labelled `A`, `B` and
`M`. `W`, `B` and the track list go through them, and `:track add` and `:track del`
work before the groups, so the fixed tracks always stay last.

`A` and `B` are returns, as in Ableton: what a track's `Send A` and `Send B` send plays
through their slots, and comes out alongside the track. A new song has a `Delay` in
`A`'s first slot and a `Reverb` in `B`'s, each with `Mix` at `FF`, so you only hear
what comes back. You can change them, and with nothing in its slots a return just
passes on what it's sent. `M` is the master: everything goes through it, and then the
limiter.

They make no sound of their own, so they have no machine or filter, and their Sound
page is an Output with just `Trim`, `Level` and `Pan`, and the master's only `Trim`,
`Level` and `Limiter`. `Trim` is at the input, before the slots, so it sets how hard
what's sent, or the whole mix, hits them, and `Level` after them sets what goes on.
Under a return's Output, Sends in lists every track and group sending to it, with its
`Send A` or `Send B` and its bar, which saves finding them strip by strip on the mixer.

The master's `Limiter` is the song's limiter, after its `Level`, so the master reads
`Trim`, its slots, `Level`, then the limiter. It's a toggle: `K` or `i`
switch it, `=` puts it on, `-` takes it off and `x` puts it back on, each one `u`, as
`:set limiter` does. See the meter's guide for what it does. The rest works as on any
track: 4 slots on the FX page, the Mod page, a strip on the mixer, and phrases. To give
one a phrase, type a chain number in its column on the Song view. A note on one plays
nothing, but it does start the track's envelopes. Steps on a return only reach that
return, so `VOL` on `B`'s phrase rides the reverb.

Soloing a track keeps the returns, so you hear what it sends. Soloing a return plays
only what comes back from it. Muting a return silences what comes back, and the master
can't be muted or soloed.

## The mixer

Press `Space 4` for a strip for each track: its name, a meter of what it's playing
with `L` and `R` bars, a fader beside them showing its `Level`, then its `Trim`,
`Level`, `Pan`, `Send A`, `Send B`, mute and solo. It opens on `Level`. The meters are on the same scale as the main one,
with `0` marked, and show the track after its level and pan. `h` and `l` move between
tracks, `0` and `$` to the first or last, `j` and `k` up and down the strip, and `gg` and
`G` to its top or bottom. Values edit as they do on the track page, with `=` `-` `+` `_`,
`C-a` `C-x`, or Cmd with `l` `h` `k` `j`, `x` puts one back to its default and `dd` to
nothing: a fader down, a pan to the middle, mute and solo off. On mute
and solo, `=` switches on, `-` switches off, and `K` or `i` changes it. `/` jumps to a setting by its name, as
it does on the track page, staying on the cursor's track. `gd` on a fader a macro holds goes
to that macro on the Gen page, and `C-o` comes back; with more than one holding it, a list
of them to choose from.

While a step's `LEV`, `PAN`, `SNA` or `SNB` drives a fader, the strip shows the
step's value in the playing colour, and the fader beside the meters moves with it.
What `VOL` and modulation make of the level is a thin line across the fader, which
stays where it is. Moving the value by hand takes it over from the steps, marked
`󰝁`, until `Space e` gives the track's faders back, or `Space E` every track's: see
**Fader automation and relative modulation** in **FX commands**.

A fader a macro holds shows where the macro puts it, as on the track page, and its fader
beside the meters stands there too; see **Macros** in **The Gen page, macros and presets**.

The tracks come first, then the groups, then `A`, `B` and `M`, with a gap between each
part, and a rule over each part's strips names it, `Tracks`, `Groups` or `Returns`, the
master being under `Returns`. The part with the cursor has the brighter rule. Under a
track's name is the group it's in, and under a group's name the tracks
in it, as `01-04 07`. `z a` folds a group's strips into its own, `z M` folds them all
and `z R` opens them; see **Folding a group**.
<!-- ANCHOR_END: rest -->
