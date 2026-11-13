# The track page, envelopes and LFOs

Press `Space 4` for what a track holds apart from its steps, on three pages: Sound
has its machine, filter, output and how it plays, with a bar beside each value showing where
it sits between the lines at each end of its range (a centred one, like Pan or Trim, with a faint
`┊` at its middle), and moving with whatever
plays it, with a dim mark left where it's set once it's a cell or more away, Mod its six mod slots,
envelopes and LFOs, its accumulator and arpeggio, and FX its effects. `H` and `L` go to the previous or next page, each with the cursor where you left it, or on its first setting the first time. FX columns belong
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
| `b` | Switch an FX slot or a mod slot out, or back in |
| `;` | `a` `w` `s` `e` `d` `f` play C to F, `<` `>` the octave, until `;` |

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
| Machine | Machine, type | The machine's own |
| Filter | Type, mode | `Cut` `Res` `Drive`; `Punch` `Safe`; `Low cut` `High cut` |
| Output | | `Trim` `Level` `Pan`; `Send A` `Send B` `Group` |
| Play | | `Pitch` `Glide` `Gate`; `Speed` `Groove` `Reset`; |
| | | `Choke` `Velocity` `Quantise` |

A machine's type is the setting that changes its character: the sample the sampler
plays, `Acid`'s `Wave`, `Supersaw`'s `Stack`, 6-op's `Patch`, and the chord of `Chord`
and `Chip`. Other synths have none. `K` lists whatever's under the cursor in a title, as
on a setting, and `g d` on the sample opens the sample page. Each machine places its own
settings: most go down the columns in order, the sampler has `Slice` and `Play` in
one column and `Start` and `Length` in the next, `Acid`'s are in pairs as a
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
| Filter | `LP` `HP` `BP` `Notch` `Ladder` `Diode` `MS20 LP` `MS20 HP` `Dirty` `OB-X` |
| Mode | How the type responds: `--` where it has none |
| Low cut, High cut | The utility filter: `00` and `FF` are off |

Drive keeps the level about even. `Punch` is Mutable Instruments' Braids' trick
for an 808 kick: as the filter's low-pass swings, its cutoff rises with it, so a
loud ring starts higher and falls in pitch to `Cut` as it dies away, and a loud
peak is reined in a little. Strike `LP` or `BP` with something short at a low
`Cut` and high `Res` to hear it: at `FF` a ring at 50 Hz starts at about 200 Hz.
It's added in Hz, so it moves a low cutoff much more than a high one, and a quiet
sound hears the filter as it is, so the curve doesn't show it. Only `LP`, `HP`,
`BP` and `Notch` have it; on the other types it's dim and does nothing, and it
does less with `Drive`, whose clipping holds the ring down. Steps and modulators
don't move it.

`Safe`, beside `Res`, is the switch of that name on Cytomic's The Drop. On, the
whole of `Res`'s turn reaches only as far as the loudest peak that still dies
away, a few steps short of where the type starts to sing by itself, so `FF`
rings on after a strike and fades, and never keeps going alone, at any `Cut`,
`Mode` or `Drive`. Its steps are closer together for it: `FF` is about where
`D7` was, and `80` where `6C` was. The curve shows the peak it leaves. `K`
switches it, `=` puts it on and `-` takes it off. Steps and modulators don't
move it, as with `Drive`, but a `RES` lock or an LFO on `Res` stops short too.
The clean types never ring by themselves, so on `LP`, `HP`, `BP` and `Notch`
it's dim and does nothing.

`Low cut` and `High cut` are a second, simple
filter after the main one, for thinning out a sound without giving up the main
filter's character. They run from 20 Hz at `00` to 20 kHz at `FF`, at 12 dB with
no resonance. `Low cut 00` and `High cut FF` are off, and off costs nothing.
Unlike `Cut`, steps and modulators don't move them.

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

## Effects

The FX page has 4 slots, which the sound goes through in order after the filters and
before `Level` and `Pan`. An empty slot does nothing and costs nothing.

Each slot is a device: a frame with the slot's number, its switch, its effect and the effect's
type in its title, its settings in three rows of up to four, and, for `Comp`, `Limit` and `EQ3`,
a box on the right with its curve or how far it's turning down. The switch is a small pill, its
knob right and green while the slot is in, and left and faint while it's bypassed. A setting
that's off or on, such as a Comp's `Auto`, shows the same switch rather than a word. The device
with the cursor has the brighter frame. An empty slot keeps its frame, says `Empty` and
`K  pick an effect`, so nothing moves when an effect goes in.

| Keys | Does |
|---|---|
| `K` or `i` on the effect | Choose what's in the slot. `=` and `-` step through |
| `K` on the type | Choose the effect's type, such as a Comp's `Glue` |
| `x` on the effect | Empty the slot |
| `x` on a setting | Back to its default |
| `h` `j` `k` `l` | Through the settings, and up to the title |
| `0` `$` | The first or last setting on the row |
| `{` `}` | Previous or next slot, staying on the same setting |
| `dd` | Take the effect out of this slot |
| `p` | Put it in this slot, swapping with what's there |
| `b` | Bypass this slot, to hear without it, or bring it back |

Each effect places its own settings: what comes out, `Mix` and `Gain`, is in its last column, a
`Comp`'s detector, times and output each have a column, an `EQ3`'s bands are its columns, and a
`Delay`'s repeats, their tone and what comes out are its. A row can have gaps, which `h` and `l`
step over. `j` and `k` keep to the column you were in, landing on the nearest setting where the
row has nothing there, and go on into the next slot's title. The effect is over the first column
and its type over the second: the `Type` of a `Distort`, `Chorus` or `Comp`, and the `Mode` of
an `LPG` or `Comb`, are in the title rather than among the settings. `{` and `}` keep the row
and column, so they come back after an empty slot, and `2j` goes two rows down. `b`, `dd` and
`p` work from any row of a slot.

`dd` then `p` is one undo step, and an effect can move to another track's slot too. Modulation and
step commands stay with the slot, so they reach whatever's there now.

A bypassed slot goes faint and crossfades out over 5 ms, so it doesn't click. It keeps running
underneath, so a reverb's tail or a compressor carries on as if it had never gone, and it's
saved with the song. Emptying the slot brings it back in.

A step switches a slot out for its note with `1BYP 01`, for slot 1, and back in with `00`,
as a lock: the next note brings back what `b` set, and held with `h` it stays. While the song
plays, the page fades the slot as it's heard, so it goes faint and comes back with the lock.
`K` lists it with the slot's effect, and LFOs and envelopes can't move it.

`Distort` has a `Type`: `Soft` and `Hard` clip, `Fold` and `Sine` fold the sound back
on itself, `Crush` lowers its bit depth, and `Down` lowers its sample rate. `Overdrive` and
`Distortion` are guitar pedals, as an acid line often goes through: `Overdrive` is a Tube
Screamer's circuit, which clips the mids softly and adds them to the clean sound, so the bass
stays tight, and `Distortion` an op-amp distortion, as a RAT is, with high gain and a hard clip.
Each mode has its own setting: `Amount` for how hard the four shaping modes and the pedals
drive, `Bits` on `Crush`, whole numbers from `12` to `2`, where anything quieter than half a
step drops out, and `Rate` on `Down`, how long each sample is held. A change of `Bits` eases
over a few ms, so an LFO on it sweeps rather than steps. `Mu-law` on `Crush` squeezes the sound
before it's rounded, as an 8-bit sampler did, so quiet sounds keep finer steps and loud ones
grit. `Track` on `Down` holds at a whole number of times the note's pitch, from 256 times at
`00` to once a cycle at `FF`, so what it folds back stays in tune with the note. The pedals have a `Tone`
as well, dark at `00` and bright at `FF`. On `Overdrive`, `80` is the pedal's knob at noon, its
highs falling away under its mids, and `FF` lifts them and `00` cuts them by about 15 dB at the
top. A mode keeps its own as you change to another and
back. Few bits gate quiet sounds, so one press up brings them in, and `Mix` blends it with the
sound going in, from `00` dry to `FF` wet. `Gain` turns down what comes out, as `Amount` can
make it much louder, on the level scale, where `E0` leaves it as it is and every `10` is 4 dB.
Every effect but `Comp` has one; a `Comp` has `Makeup`.

A mod slot can move the ones after it: `Mod 4`'s `Dest` has `M5 Depth`, `M5 Phase`, `M6 Depth` and
`M6 Phase`, and `Mod 2`'s has every later slot's, so an envelope can bring in an LFO's depth with
each note. It only goes one way, so `Mod 6` can't reach any other. See **LFOs**.

`K` on `Dest` lists them in categories, opening inside the one with what's chosen: `l` goes
into a category, `h` closes it, and typing searches them all. See **Lists** in
**Navigation** for every key.

`Dest` and step commands only list what's loaded: a synth's settings under their names on
that voice, on a track playing it. One already chosen stays, reads like `Punch ---`, and does
nothing until it's back.

`Delay` and `Reverb` can go in any slot, starting half wet. Their tails carry on after a
note ends. `Reverb` has these:

| Setting | Does |
|---|---|
| `Pre` | How long before it comes in, up to a quarter second |
| `Decay` | How long it rings. `FF` goes on for a long time |
| `Damp` | How quickly its highs fall away as it rings |
| `Low cut`, `High cut` | Shape what goes in |

`Delay` has these:

| Setting | Does |
|---|---|
| `Time` | A note division, up to `1 bar` |
| `Feedback` | How much comes round again. `FF` keeps going |
| `Ping-pong` | On, it goes side to side; off, it stays put |
| `Width` | From `00`, sides together, to `FF`, fully apart |
| `Low cut`, `High cut` | Take more from each repeat |
| `Drive` | Saturates louder repeats each time round |
| `Retime` | How a new `Time` comes in: `Glide` or `Jump` |
| `Wow` | A slow wobble in the repeats' pitch, as worn tape has |

`Glide` eases to a new time, which bends the repeats' pitch on the way, as a tape delay does.
`Jump` fades from the old time to the new instead, so the repeats keep their pitch: it's for a
time that a step locks, changing the rhythm without a swoop. Its fade lasts half the new time,
from 10 ms to half a second, and a change that comes while it fades waits for it to finish.
`Wow` bends the two sides' pitch opposite ways, by up to about 9 cents, going round once every
two seconds.

`Chorus` thickens a sound with copies of it that drift in time. It starts half wet,
which is the classic sound. Its first two types are after Mutable Instruments' Rings:
`Chorus` has two slowly drifting copies, and `Ensemble` three on a slow and a fast
wobble, as a string machine has. The other three are a Roland Juno's, the short, bright
shimmer of its two chorus buttons: each side a copy a few ms late, swept by a triangle,
the left getting longer as the right gets shorter, so even a sound in the middle comes
out wide.

| Setting | Does |
|---|---|
| `Type` | `Chorus`, `Ensemble`, `Juno I`, `Juno II` or `Juno I+II` |
| `Rate` | How fast they move: as Rings or the Juno has it at `80` |
| `Depth` | How far they move. `00` leaves fixed copies |

`Juno I` sweeps each side from 1.66 to 5.35 ms and back once every two seconds, 0.5 Hz,
and `Juno II` the same a little faster, 0.83 Hz, as a Juno-60 measures. `Juno I+II`, both
buttons down, is a fast, shallow wobble from 3.3 to 3.7 ms at 8 Hz, nearer a vibrato, its
sweep rounded to a sine and both sides moving together, so it doesn't widen the sound as
the other two do. `Depth` at `80` is the
Juno's own sweep, `FF` twice as long, and `Rate` goes two octaves either way. Each copy
loses a little of its top, as the Juno's bucket brigade does, but doesn't dull as it moves.

`EQ3` shapes the tone with three bands: a shelf at the bottom, a bell in the middle and a
shelf at the top. Each band's boost or cut is centred on `80`, where it leaves the sound
as it is, and goes 15 dB either way. It stays smooth as an LFO sweeps it.

| Setting | Does |
|---|---|
| `Low freq`, `Low` | The low shelf: where it sits, and how far it boosts or cuts |
| `Mid freq`, `Mid` | The middle band, the same |
| `Mid Q` | How narrow the middle is: wide at `00`, sharp at `FF` |
| `High freq`, `High` | The high shelf |

`Comp` evens out a track's level, or ducks it under another track. With `Source` on
`Self` it hears its own sound. Set it to another track, like `01`, and it turns this one
down whenever that one plays: put it on a bass, hearing the kick, for the pumping
sidechain sound. It hears the other track after that track's slots and before its level
and mute, so a muted kick track still ducks the bass, and turning the kick down doesn't
change it. Set it to a group, like `G1`, and it hears the whole kit after the group's
slots, even while the group is muted or another track is soloed. Leave `Auto` off for
ducking, or the bass is louder between kicks.

Its `Type` is its character. `Clean` is transparent, with knobs for its ratio and
times. `Console` is an SSL console's bus compressor, fitted to Arturia's Bus FORCE, and
`Glue` is Ableton Live's Glue Compressor, another model of the same SSL, fitted to
Live's own: both have the SSL's switches for their ratio, attack and release, picked
with `K`, and an `Auto` release that lets go of a short peak quickly and a long squeeze
slowly. `Glue`'s knee is softer the lower its ratio, its release names are the SSL's
rather than the times they take, and it settles lower the slower its attack is.

| Setting | Does |
|---|---|
| `Type` | `Clean`, `Console` or `Glue` |
| `Threshold` | Where it starts turning down, from -60 dB to 0 dB |
| `Ratio` | How much: 2, 4 or 10 to 1, on `Clean` 1:1 to a limiter |
| `Attack`, `Release` | How fast it turns down and lets go |
| `Source` | `Self`, a track by its number, `A`, `B`, or `G1` to `G4` |
| `Auto` | Switched on, it comes out as loud as it went in |
| `Mix` | Below `FF`, the dry sound blends in, for parallel compression |
| `Range` | `Glue` only: the most it turns down, up to 70 dB at `FF` |
| `Makeup` | Turns up what it turned down, from 0 dB to +24 dB |
| `Peak Clip` | `Glue` only: on, it soft clips what comes out, last |

`Limit` keeps a sound's peaks under a ceiling. It looks 2 ms ahead, so it's already
turned down when a peak arrives, and nothing gets over, however hard it's pushed. Its
place is the master's first slot, after which the master's own limiter has nothing
left to catch, but it works on any track. Looking ahead means the track it's on comes
out 2 ms late, and nothing makes up for that, so on a single track it can sit a
little behind the others. On the master everything is late together. Both sides are
turned down together, so the stereo picture doesn't move.

| Setting | Does |
|---|---|
| `Input` | Pushes it harder, from 0 dB to +24 dB, for a louder sound |
| `Ceiling` | The most that comes out, from -24 dB to 0 dB. It starts at -0.3 |
| `Release` | How fast it lets go, from 1 ms to a second. It starts at 100 ms |
| `Mix` | Below `FF`, the dry sound blends in, as late as the limited one |
| `Gain` | After the ceiling, on the level scale, so above `E0` it goes over |

`Phaser` and `Flanger` sweep a sound slowly up and down, after Vital's. Both start
half wet, which gives the deepest sweep, and each side sweeps a third of a turn
from the other, so even a sound in the middle comes out wide. Their `Rate` is a
note length, from `1/64` to `16 bars`, so the sweep keeps time with the song.

`Phaser` runs the sound through a chain of all-passes, which make notches that
sweep around a centre frequency. `Feedback` sharpens them into peaks.

| Setting | Does |
|---|---|
| `Rate` | How long one sweep up and down takes. It starts at `2 bars` |
| `Depth` | How far it sweeps either way, up to 4 octaves. It starts at 2 |
| `Centre` | The frequency it sweeps around, from 20 Hz to 20 kHz |
| `Feedback` | How much goes round again, for a sharper, ringing sound |
| `Stages` | `4`, `8` or `12` all-passes: each 4 make two more notches |

`Flanger` mixes in a copy that's a few milliseconds late, which makes a comb of
notches, and sweeps how late it is, for the jet-plane sound.

| Setting | Does |
|---|---|
| `Rate` | How long one sweep up and down takes. It starts at `1 bar` |
| `Depth` | How far the delay sweeps, at `FF` from 0.5 ms to twice `Delay` |
| `Delay` | How late the copy is at the middle, from 0.5 ms to 10 ms |
| `Feedback` | None at `80`. Above, it rings brighter; below, hollower |

`Tape` puts a sound on tape and plays it back, after Airwindows' ToTape, with
Beads' wow and flutter. It warms and thickens what's loud, softens loud highs,
and lifts the lows a little around 69 Hz, while a quiet sound comes through
nearly as it was. It never comes out louder than -0.5 dB, however hard it's
driven, so `Gain` is there to bring it back down. It's a few samples late,
more with wow, so `Mix` below `FF` can blend in a faint chorus.

| Setting | Does |
|---|---|
| `Drive` | How hard it hits the tape, from -18 dB to +18 dB, 0 dB at `80` |
| `Tone` | Flat at `80`. Above, the highs come out brighter; below, darker |
| `Bias` | None at `80`. Above, it softens fast edges; below, it crackles |
| `Bump` | How much the heads lift the lows. It starts half way |
| `Wow` | A slow, drifting wobble in pitch, as a worn transport has |
| `Flutter` | A faster, steady wobble, about twice a second |

`Exciter` adds air, after the Aphex Aural Exciter as ChowDSP models it in
Surge: it takes the highs of the sound, drives them into a soft clip for
new harmonics, and adds those to the sound. A level detector turns what it
makes up as the highs get louder, so loud hats and consonants sparkle while
quiet ones are barely touched. It suits hats, vocals and a whole mix.

| Setting | Does |
|---|---|
| `Drive` | How hard the highs are driven, so how much it makes |
| `Tone` | Where the highs start, from 500 Hz at `00` to 10 kHz at `FF` |
| `Attack` | How fast the detector rises, from 5 ms to 20 ms |
| `Release` | How fast it falls, from 50 ms to 200 ms |

`Mix` is how much of what it makes is added: at `00` the sound is left as
it is, and it starts at `80`. The higher `Tone` is, the less gets through
to be driven, so `Drive` gives it more to make up. Each side has a detector
of its own, so loud highs on one side don't excite the other.

`Plate` is a second reverb, after Jon Dattorro's plate: brighter and denser
than `Reverb`, and wide from a single sound. It starts half wet, and its tail
carries on after a note ends. `Pre`, `Decay`, `Damp` and `High cut` work as
`Reverb`'s do. `FF` rings for about a minute, but always dies away.

| Setting | Does |
|---|---|
| `Size` | How big the plate is, from half to twice Dattorro's, his at `80` |
| `Mod` | How much its tail wobbles, which keeps it from ringing metallic |

A change to `Size` bends the pitch of the tail a little as it glides, taking
about a second end to end.

`Spring` is a spring tank, as a guitar amp or an Octatrack has, after
ChowDSP's model in Surge: a sound goes in, drips and boings down the springs
and comes back round. It suits dub chords, snares and techno stabs. It starts
half wet, and its tail carries on after a note ends. `Decay` and `Damp` work
as `Reverb`'s do; `Decay` goes from a tenth of a second to about four, and a
small spring rings shorter.

| Setting | Does |
|---|---|
| `Size` | How long the springs are: how far apart the drips come round |
| `Spin` | How much each drip is smeared into a chirp: most at `00` |
| `Reflect` | How much comes back from the ends of the springs, which thickens it |
| `Chaos` | How much the springs' length wanders, as a tank that's moving |
| `Knock` | Switched on, the tank is knocked once, as when it's kicked |

A knock rings the springs whether or not anything's playing, so a `1KNCK`
lock on an empty step, for a spring in slot 1, is a hit of its own. It knocks once each
time `Knock` goes on: to knock again, it has to go off first, which the next
note does to a lock. A loud sound driven into it clips softly on the way
round, as a real tank's driver does.

`Rotary` is a rotary speaker, a Leslie, after the one in Surge: a horn
spinning over a crossover at 800 Hz and a drum spinning under it, heard
through two microphones. The horn's pitch wavers as it comes and goes, and
it swells at whichever side it points to; the drum's low middle swells too,
and the deep bass under 200 Hz hardly moves. It's for organs first, and
for guitars, electric pianos and pads. It starts slow, all wet.

| Setting | Does |
|---|---|
| `Fast` | Switched on, the rotors spin fast; off, slow |
| `Inertia` | How long they take to change speed, a Leslie's at `80` |
| `Doppler` | How far the horn's pitch wavers |
| `Tremolo` | How far the loudness swells as the rotors turn |
| `Drive` | How hard its amplifier is driven, from clean at `00` |
| `Width` | How far apart the microphones are heard, from mono at `00` |

`Fast` is for a step to lock, as an organist flicks the switch: a `1FAST 01`
on a step speeds a rotary in slot 1 up, and the next note slows it again,
or a hold mark keeps it fast. The horn is light and gets there in about a
second; the drum is heavy and takes about five, so for a while the two turn
at different speeds, the sound of a Leslie changing over. `Inertia` at
`00` changes at once, and `FF` takes twice as long.

`Space` is a huge, bright wash that rings on and on, after Airwindows'
Galactic: for pads, drones and ambient tails that hang under a whole
section. It starts half wet with its tank at its biggest, near Galactic's
own settings. `Decay` and `Damp` work as `Reverb`'s do, but its tail doesn't
darken as it rings, as theirs do: `Damp` darkens what goes in and what comes
out.

| Setting | Does |
|---|---|
| `Size` | How big its tank is: small and metallic at `00`, biggest at `FF` |
| `Mod` | How fast and far what goes in wobbles in pitch, none at `00` |

With its tank at its biggest, its tail takes about four seconds to fall away
at `00`, eight at `80`, fifteen at `C0` and well over a minute at `FF`,
though it always does. The longer it rings, the less of each new sound goes in, so a long
`Decay` builds slowly as notes pile up. Hold a chord through it with `Mix` at
`FF` on a return for an endless pad.

`Pitch` shifts the sound up or down without changing its length, after the
pitch shifter in Mutable Instruments' Clouds. Two grains read the sound back
faster or slower than it plays, each fading in as the other fades out. It
starts all wet with no shift, so it only delays the sound by half a grain.

| Setting | Does |
|---|---|
| `Pitch` | How far it shifts, in semitones, two octaves either way |
| `Fine` | How far besides, up to 50 cents either way, none at `80` |
| `Size` | How long each grain is, from 10 to 100 ms, about 64 ms to start |
| `Feedback` | How much goes round again, so each trip shifts once more |

Short grains suit drums and fast notes, and long ones suit pads and chords,
which come out smoother. Every change glides. A change to `Size` bends the
pitch a little as it moves, taking about a second end to end. With some
`Fine` and `Mix` near `80`, it thickens the sound like a chorus.

For a shimmer, put `Pitch` at `+12 st` in a slot before `Reverb` or `Plate`,
and turn up `Feedback` to stack the octaves.

`Grains` records the last four seconds of the sound all the time and plays
it back as a cloud of short grains, after the granular mode of Mutable
Instruments' Beads. It starts half wet, with about seven grains a second.

| Setting | Does |
|---|---|
| `Time` | How far back the grains read, from just now to four seconds |
| `Size` | How long each grain is. Below `55` they play backwards |
| `Density` | How often grains start: evenly below `80`, at random above |
| `Spread` | How far the grains spread across the sides |
| `Freeze` | On, it stops recording, so grains play what's held |
| `Random` | How far each grain's time, size and shape stray at random |
| `Pitch` | How far each grain is shifted, in semitones, two octaves |
| `Shape` | The envelope, from square through decay and bell to swell |

`Size` runs as Beads' does: from four seconds backwards at `00`, down to
about 8 ms at `55`, then up to four seconds forwards at `FF`. `Density` is
none at `80`, faster towards either end: evenly up to about 130 a second at
`00`, and at random up to about 65 a second at `FF`. `Shape` is a decay at
`80` and a bell at `AA`, where it starts.

Changes to `Time`, `Size`, `Pitch` and `Shape` reach the next grains, so
moving them never clicks. For a held drone, turn `Freeze` on with a step's
FX slot, then move `Time` to scan through what's held.

`Warps` crosses the sound with a second one, after Mutable Instruments'
Warps. The track's own sound is the carrier. The modulator is another
track, chosen with `Source` as `Comp`'s is, or with `Source` on `Osc`,
Warps' own oscillator, which plays the track's note. It starts as a ring
modulator, with a sine an octave above the note.

| Setting | Does |
|---|---|
| `Algo` | Which algorithm, blending from each one into the next |
| `Timbre` | What that algorithm does with it, as Warps' timbre knob |
| `Source` | `Osc`, a track by its number, `A`, `B`, or `G1` to `G4` |
| `Level` | How hard the modulator is driven, clipping above about `A0` |
| `Pitch` | The oscillator, in semitones from the track's note |
| `Shape` | The oscillator's wave: sine, triangle, saw, square or noise |

`Algo` goes through Warps' algorithms in order, each on a value of its
own: a crossfade at `00`, a wavefolder at `20`, a diode ring modulator at
`40`, a multiplying one at `60`, XOR at `80` and a comparator at `A0`.
Between two, it blends them. Past `A0` it fades into the bare modulator,
at about `B3`, then into a vocoder from `BA`. Further up, the vocoder lets
go more slowly, until at `FF` it holds what it heard. `Timbre` crossfades
from the carrier to the modulator, folds harder, drives the ring
modulators harder, blends XOR in, or picks among the comparator's modes.
In the vocoder, it's none at `80`, and shifts the formants either side.

The modulator track is heard after its slots and before its level and
mute, so mute it to hear only what Warps makes of it. With a silent
modulator, the ring modulators and the vocoder are silent too; the
crossfade, fold, XOR and comparator still shape the carrier on its own.
The diode ring modulator wants loud sounds, as Warps' does. It's mono.

`LPG` is a low-pass gate, after Mutable Instruments' Streams. A follower
listens to the track's own sound, or to another track's with `Source` as
`Comp`'s, and the louder that is, the further it opens. It starts as a
gate on its own sound, for plucked, woody notes.

| Setting | Does |
|---|---|
| `Mode` | `Gate`, `Filter` or `VCA`: what the follower opens |
| `Amount` | How far it opens, from `Base` to all the way |
| `Base` | Where it rests when closed: its cutoff, or its level as `VCA` |
| `Res` | The filter's resonance, in `Gate` and `Filter` |
| `Response` | How slowly it opens and closes |
| `Source` | `Self`, a track by its number, `A`, `B`, or `G1` to `G4` |

`Gate` is Streams' vactrol, which opens the level and the cutoff
together: quickly as the sound comes in, and slowly as it goes, its
cutoff falling first and its level after. `Response` lengthens its decay
up to about four seconds at `80`, then slows its attack as well. `Filter`
moves the cutoff alone, for an envelope filter or auto-wah: set `Base`
where it should rest, and the louder it plays the brighter it gets.
`VCA` moves the level alone. In those two, `Response` lengthens both the
follower's rise and fall. A Streams vactrol can be pinged by a trigger;
a slot has none, so the follower's rise stands in. To gate a pad with a
drum, put `LPG` on the pad with `Source` on the drum's track. With a
silent source, `Gate` and `VCA` stay shut.

`Reso` is a resonator, after Mutable Instruments' Plaits, Rings and
Elements: a bank of 24 modes tuned to the track's note, which the track's
sound strikes, so it rings like a bar, a string or a bell. A drum loop
through it plays as a pitch. It starts in tune with the note, its partials
whole multiples of it, ringing for about a third of a second, all wet.

| Setting | Does |
|---|---|
| `Tune` | Its pitch, in semitones from the track's note |
| `Material` | Partials bunched low, in tune at `40`, spread like a bell high |
| `Bright` | How long the high partials ring against the lowest |
| `Decay` | How long it rings, from a thud at `00` to well over a minute at `FF` |
| `Position` | Where it's struck and heard: its end at `00`, its middle at `FF` |
| `Width` | How far a second, wandering pickup spreads it across the sides |

It follows the track's note and `Pitch`, and glides to a new one over a
moment, so a note change bends the ringing rather than clicking. A sample
plays faster at a higher note, so to keep a loop at its own speed, play it
at `C-3` and set `Tune` from steps, as `1TUNE`. `Material` is in tune
from `40` to `4B`, and `Position` towards `FF` leaves out the even
partials, for a hollow ring. A second pickup wanders slowly along it, as
Elements' does, and `Width` spreads what it hears apart from the first
across the sides; at `00` it's mono.

At `Decay` `80` a drum comes out about as loud as it went in, at most
notes, and a longer `Decay` rings on and builds up. A tone held at its note
can ring very loud, so Rings' limiter holds what comes out under full
scale, turning down at once and letting go over a second or so.

`Comb` is a comb filter tuned to the track's note, after Vital's: the sound
goes into a delay a cycle of the note long, and what comes back out is fed
in again through a filter of its own, so it rings at the note and its
harmonics, like a plucked string or a metal tube. A drum loop through it
plays as a pitch. It starts as a comb in tune with the note, ringing for
under half a second at middle C, all wet.

| Setting | Does |
|---|---|
| `Mode` | `Comb` rings; `Flange` adds the sound, notched between harmonics |
| `Tune` | Its pitch, in semitones from the track's note |
| `Feedback` | How long it rings: none at `80`; odd harmonics alone below |
| `Tone` | How bright it rings: the note alone at `00`, everything at `FF` |
| `Spread` | How far under `Tone` it reaches: a narrow band at `00`, all at `FF` |

Like `Reso`, it follows the track's note and `Pitch`, glides to a new one,
and to keep a sample loop at its own speed, play it at `C-3` and set `Tune`
from steps. Below `80`, `Feedback` rings on the note's odd harmonics alone,
hollower, like a clarinet, but at the same pitch. At `FF` with `Tone` up
it rings for many seconds, and what goes round is rounded off as it nears full
level rather than clipped flat, so a runaway softens instead of squaring. As a `Comb`, `Feedback` `80` is only the sound;
as a `Flange`, it's a flanger standing still, its notches between the
note's harmonics, and feedback sharpens it into a ring.

`Tone` is a low-pass inside the loop: turned down, the high harmonics die
away first, as a string's do. `Spread` brings a high-pass up under it: at
about `80` with `Tone` at `FF` the note's own harmonic goes, for a thin,
metallic ring, and towards `00` only a band of harmonics around `Tone`
rings, for a vowel-like one. However they're set, the harmonics that ring
longest stay on the note.

It sounds unlike `Reso`. `Reso`'s modes ring by themselves once struck,
each fading at its own rate, so a drum becomes a bell or a bar. A comb is
one delay going round, so every repeat is the drum again, a cycle of the
note later and filtered a little more: it buzzes, and the drum's own grain
comes through as the pitch builds.

At most settings a drum comes out about as loud as it went in, and a long
`Feedback` rings on and builds up. Rings' limiter, as `Reso` has, holds
what comes out under full scale.

`Vowel` is a vowel filter, after Plaits' speech synth and Vital's formant
filter: the sound goes through five band-passes side by side, a voice's
formants, fixed in pitch as a vowel's are, so a saw or a drum loop through
it says a vowel. It starts as an `A` in a middle-sized voice, all wet.

| Setting | Does |
|---|---|
| `Vowel` | Which vowel: `A` at `00`, `E` at `40`, `I` at `80`, `O` at `C0`, `U` at `FF` |
| `Size` | How big a voice: a child's at `00`, a deep one at `FF` |
| `Res` | How sharp its formants are, from broad at `00` to ringing at `FF` |
| `Track` | How far its formants follow the note, from not at all at `00` |

Between two places `Vowel` blends from one vowel to the next, so an LFO or
envelope on `1 Vowel` makes it talk as it sweeps through them. A step can
set it as `1VOWL`. `Size` moves the formants together, lower as the voice
grows, as Plaits' five registers do. Their pitches and levels come from
Plaits' table of five vowels in five voices.

Formants stay put whatever note plays, which is what makes it a vowel, so
`Track` starts at `00`. Turned up, they follow the track's note from
middle C: at `FF` an octave up takes them an octave up, as if the voice
were sped up.

`Res` runs from a Q of 2, broad enough to hear the sound through, to 30,
past Plaits' 20, where the formants ring and a saw's harmonics stand out
one at a time as they pass. The level is matched to pink noise at every
setting. A saw comes out louder where a harmonic lands on a formant and
quieter where none does, by up to about 8 dB, and a drum loses what's
above its formants, its hats and the top of its snare, so it comes out
quieter; `Gain` can make that up. A tone held on a formant at full `Res`
can ring loud, so Rings' limiter, as `Reso` has, holds what comes out
under full scale.

`Freq Shift` is a frequency shifter, after Surge's: it moves every
frequency in the sound up or down by the same number of Hz. That's not
what `Pitch` does, which keeps a note's harmonics in step. Shifted by the
same Hz, they stop being whole multiples of the note, so a saw or a voice
turns to a bell or a robot, and a few Hz up or down makes it beat and
swirl. It starts 16 Hz up, all wet.

| Setting | Does |
|---|---|
| `Shift` | How far it moves: 1 kHz down at `00`, none at `80`, 1 kHz up at `FF` |
| `Spread` | How far apart the sides are moved, the left up and the right down |
| `Track` | How far the shift follows the note, from not at all at `00` |
| `Time` | A delay in its loop, from none at `00` to a second at `FF` |
| `Feedback` | How much goes round again, to be shifted again |

`Shift` is on a curve, so the slow shifts have room near the middle: `90`
is about 2 Hz, `A0` 16 Hz, `C0` 128 Hz and `E0` 430 Hz. The status line
reads it in Hz, and its fine digits go between. Shifted down past nothing,
a low frequency folds back up, mirrored, so a big shift down turns the bass
inside out.
`Mix` at `80` puts the shifted sound beside the sound itself, which beats
at the shift: a few Hz is a slow phaser-like swirl, and a few tenths of a
Hz a slow sweep.

`Spread` takes the sides apart by up to 1 kHz, half each way, so a sound in
the middle beats differently in each ear and comes out wide.

`Track` at `00` shifts by the same Hz whatever note plays. At `FF` the
shift follows the track's note from middle C, twice as far an octave up,
so a shift set to the note's own pitch at middle C, about 262 Hz, keeps a
shifted saw in the same inharmonic shape up and down the keyboard.

With `Feedback`, what comes out goes round again to be shifted again, so a
note climbs or falls in a spiral. `Time` spaces the trips round: at `00`
they run together into a ringing smear, and at a few tens of ms each
repeat is heard a step further up or down, the barber-pole effect. Each
time round, what goes round loses a little over 5 kHz and is softly
limited, and what's under 15 Hz is taken out, so a spiral stays bounded and
dies away even at `FF`.

`Stutter` repeats a piece of the sound when a step says, for rolls, glitches
and fills, after Surge's Microgate. It does nothing until `Trig` goes on:
then it takes the next piece of what comes in, `Length` long, lets it play
as it comes, and plays it again `Repeats` times, each a `Length` after the
last, so the repeats fall on the grid. Then the sound comes back where it's
got to. It starts all wet, so the repeats take the place of the sound, with
a sixteenth played three times more: a beat in all.

| Setting | Does |
|---|---|
| `Trig` | Switched on, it takes the next piece and repeats it |
| `Length` | How long the piece is, a note division from `1/64` to `1/4` |
| `Repeats` | How many times it plays it again, 1 to 16, or `Hold` |
| `Decay` | How much quieter each repeat is, from none to 12 dB |

`Trig` is for a step to lock: a `1TRIG 01` on a step, for a stutter in slot
1, repeats what plays from there. Every step that locks it starts it again,
even the very next one, so a lock on each of four steps cuts four pieces,
and a lock beside a `1LEN` or `1RPTS` changes the roll for that step alone.
Switched on by hand, it repeats once each time it goes on.

`Hold` keeps repeating the piece for as long as `Trig` is on: until the next
note ends a lock, or until it's switched off. `Decay` at `80` halves each
repeat, so a roll fades out like an echo; the status line reads it in dB.

The piece starts where the sound crosses zero, and each repeat starts where
what's playing crosses zero the same way, so a splice doesn't click. A
repeat waits at most about 13 ms for that, and never adds up, so the roll
keeps to the grid; a low bass with no crossing in that time is spliced
where it is, with a short fade.

Every setting in a slot can be moved: an LFO's or envelope's `Dest` lists them while the
slot holds something, like `1 Amount`, and a step's FX slot can set them too. The bars
follow what's playing. A setting that's whole values, like `Bits`, just steps.

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
as `PTCH` on a step does for that step. `Choke` puts the track in a group, `1`
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

`Speed` is how long each of the track's steps is, from `1/32` to `1/4` with the
triplets between, so one track can play eighths or triplets against the others'
sixteenths. `=` and `-` move through the list, `K` and `i` choose from it, and
`x` goes back to `1/16`, which is dimmed. Its rows, pulses, groove and timing
commands all follow its step. See **Speed** in the FX guide.

`Glide` is how long a `HLD` tie glides into the next note, from 2 ms to 2 s, on
every voice: a synth by its pitch, a sample by its speed. `Off`, dimmed, jumps, or
on `Acid`, shown as `Slide`, slides by the voice's own `Slide`. `=` and `-` move it
a step, `+` and `_` sixteen, `i` takes a time in ms or `off`, and `x` turns it off.
`GLD` on a step sets it for that tie. See **Note entry**.

`Reset` is how many of the song's steps the track's chain plays before it starts
again from its top, counted from where it last started, so a short part can keep to a
bar while the others drift. `Off`, the default, plays on. `=` and `-` move it a step,
`+` and `_` to the next or previous whole bar, `i` takes a number of steps up to 1024,
bars as `2bar`, or `off`, and `x` turns it off. For a one-off, `Space a` starts the
chain again at the next line of the launch grid: see **Jamming** in **Phrases and
chains**.

## Groups

A group sums tracks into one, as Ableton's group tracks do, so a whole kit can go
through one compressor: `Comp` with its `Type` on `Console` or `Glue` in a group's slot is the glue.
Every song has four, `G1` to `G4`, after the tracks and before the returns.

`:group 1 1-4` puts tracks 01 to 04 in group 1, `:group 2` puts the cursor's track in
group 2, and `:group off` takes it out of any, or `:group off 1-4` a range. Each is one
`u`. The Sound page's `Group`, under `Send B`, does the same for one track: `=` and `-`
go through `G1` to `G4`, `i` takes a number or `off`, and `x` takes it out.

A track in a group plays through it, and the group plays through its own slots,
`Level` and `Pan` to the master. Its sends still go from the track itself, so a snare
can go to the reverb on its own, and the group has `Send A` and `Send B` of its own
too. A group's Sound page has an Output with `Trim`, `Level`, `Pan` and its sends,
where a track's are, and under it Tracks, the tracks in it, in its colour, each with
its own mute or solo. Its `Trim` is at its input, so it sets how hard the sum of its
tracks hits its slots. The rest is as on a return.

Each group has a colour: its icon, its strip on the mixer and the `G1` under each of its
tracks' strips share it, as does a track's `Group` on its Sound page. Its tracks are
named in it wherever tracks are listed: the song view's columns, the phrase view's
columns beside the grid, and the track list. The chain view, the phrase view and the
track page put `G1` after a grouped track's name.

### Folding a group

In the song view and on the mixer, a group can fold its tracks into its own column
or strip, as Ableton folds a group track, to make room. The tracks keep their order;
folding only hides them, and the group's name gets a mark, two arrows pushed together.

| Keys | What they do |
|---|---|
| `z a` | Fold this group, or the one this track is in, or open it |
| `z M` | Fold every group |
| `z R` | Open every group, the empty ones too |

The cursor steps over folded tracks: `h`, `l`, `0` and `$` there, and `W` and `B` in
every view. Folding a group from one of its tracks puts the cursor on the group, as
Vim's cursor stands on a closed fold, and so does putting the cursor's track in a
folded group. Choosing a folded track from the track list (`Space f b`) opens its
group again.

A group with no tracks in it is folded away, so it's left out of the song view and the
mixer, and the cursor goes past it. It comes back as soon as a track joins it, or with
`z R`, faint while it's empty. The phrase view folds the same groups: a folded one has
a column of its own beside the grid, named and empty, in place of its tracks'. Its own
`z a` collapses the timeline instead.

Folds are how the screen shows the song, so they aren't saved with it. They last until
you quit or open another project, which starts with every group open and the empty ones
folded away.

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

## The Mod page

Each track has six mod slots, `Mod 1` to `Mod 6`, and each slot is a type you choose:
an `ADSR`, an `AHD` or a `Drum` envelope, an `LPG`, an `LFO`, `Steps` or `Track`, in any mix. A new track has three ADSRs, then three
LFOs. The slots work out in order, so a slot can move only those after it, and never
round again.

The page is a rack of devices in two columns, as the FX page is a rack of slots:
`Mod 1` to `Mod 3` down the left, `Mod 4` to `Mod 6` down the right, and the
accumulator beside the arpeggio under them. A return, a group and the master have
no notes to move, so their page ends with the slots.

A slot's title says what it is and where it goes: `Mod 4`, its switch, green while
it's in, its type, its mode (an envelope's curve, how notes start an LFO, or what Track follows), its
polarity, and `→` its `Dest`,
so `Mod 4  LFO  Trig  Bi  → Cut` reads as what it's for. `---` there is dim, as it goes
nowhere. Its settings are in two columns of three, and its shape or cycle is in a
box on its right. The device with the cursor has the brighter frame.

| Keys | Does |
|---|---|
| `h` `l` | Along a row or the title, and on into the device beside it |
| `j` `k` | Down and up the column, `k` from a first row to the title |
| `{` `}` | Up or down a device in the column, on the same setting |
| `0` `$` | The first or last setting on the row, in the device |
| `gg` `G` | The top or bottom of the column |
| `K` | On the type, choose another; on the mode, polarity or `Dest`, list them |
| `x` | Put back the slot's first type, or on the polarity, the type's |
| `b` | Switch this slot out, to hear without it, or back in |

**Changing a type.** `K` on the type lists `ADSR`, `AHD`, `Drum`, `LPG`, `LFO`, `Steps` and `Track`, each with what it
does. Choosing one is one undo step, and the slot keeps its `Dest`, its `Depth` and
its switch. Each slot also keeps every type's own settings, so trying an LFO in
`Mod 2` and going back finds its envelope as it was. What it moves glides from the
old type's to the new one's over 5 ms, so it doesn't click. `=` and `-` don't change
a type, as a type changes what the device is: only `K` does, as only `K` chooses an
effect.

`b` works from any row of a slot, as it does in an FX slot. Switched out, it goes
faint, its switch too, and what it moves glides back to where it would be without
it over 5 ms, so it doesn't click. Its settings stay as they are, and it keeps
running underneath, so an LFO comes back in phase and an envelope where the note
has it. It's saved with the song. A step can't switch one, as a step can an FX
slot, but `DEP4 80` on a step takes `Mod 4` to no depth for its note.

**Polarity.** `Uni` moves what a slot reaches one way only, and `Bi` both ways
around it. An envelope is `Uni` and an LFO `Bi` unless you choose: `=` on the
polarity flips it. An LFO at `Uni` goes from nothing to its depth, for a tremolo
that only dips `Vol`, a vibrato that only bends up, or a filter that only opens. An
envelope at `Bi` is centred on the setting: below it at rest and above it at full,
so on `Pitch` a note can start above itself and settle below. A negative `Depth`
still flips either. The picture's dotted line shows it: a baseline for `Uni`, a
centre line for `Bi`. A chosen polarity stays through a type change, and `x` puts
back the type's own. A step locks it with `POL`, as `POL4 00` for one way.

So `}` goes from `Mod 1` to `Mod 2`, and `3}` from `Mod 4` to the arpeggio. Where
the screen has room for fewer than four rows of devices, the page moves by whole
rows of them to keep the cursor's in view.

## Envelopes

An `ADSR` slot starts with each note. It draws its shape in a box on its right.
Time is stretched in the drawing, so a short attack still shows, and a stage's
width depends only on its own time. The times are down the left, and the levels
and `Reset` down the right.

| Setting | Value |
|---|---|
| Curve | In the title: `Exp`, `Lin`, `Snap` or `Analog`, how its stages bend |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Attack, Decay | Milliseconds, up to 8000 |
| Sustain | The percent it holds at while the note lasts |
| Release | Milliseconds, after the note ends |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**The notes' own shape** is the first envelope on `Vol`, whichever slot that is,
so it has no depth and shows `--`. A new track's is `Mod 1`. Switched out, notes
have the plain shape a new track's have, from the next one; it doesn't hand the
job on to a later envelope. Make `Mod 1` an LFO and the next envelope on `Vol`
shapes the notes instead, or none does.

`Mod 2` starts on `Cut` with no depth, so raise its depth to open the filter on
each note. `Mod 3` starts on `---`, with `Mod 2`'s decay, ready to be given
somewhere to go. On `Pitch`, depth is in semitones and in hex, so `+0C` is an
octave. An envelope's depth can be locked on a step too: `DEP2` for `Mod 2`.

A note lasts until the next note on its track, an `OFF`, or its gate closing,
and the next note fades the last one quickly rather than running its release. So
a drum is often a Decay with Sustain at `0%`, and at `Gate` 100% Release only runs
on an `OFF`. To make notes shorter, lower `Gate` in the Play device, give a row a
length of its own, or put `OFF` on a step.

**The curve** is the envelope's mode, after its type in the title, and bends
every stage at once:

| Curve | Attack | Decay and release |
|---|---|---|
| `Exp` | Starts slowly | Fast, then slow. The default |
| `Lin` | Straight | Straight |
| `Snap` | Starts slowly | Twice as fast at first, then a long tail |
| `Analog` | Starts fast | A capacitor's fall |

Halfway through a decay to nothing, `Lin` is 6 dB down, `Exp` 18, `Analog` 30 and
`Snap` 35, so `Snap` gives a drum or a pluck a sharper snap for the same time.
`Analog` is how a hardware envelope charges and discharges a capacitor: halfway
through its attack it's nearly two thirds of the way up, so a long one swells in
rather than fading up from nothing, and its falls are true RC curves. The picture draws the curve it
plays. `=` and `-` on it go through the four, `K` lists them, and `x` puts back
`Exp`. A step can't lock it.

**Without a reset,** a note starts the attack from where the envelope is, at the
point its curve reaches that level, so the attack keeps its shape and takes only
what's left of its time, and the decay comes that much sooner. A fast run of notes
or a legato line keeps its punch rather than crawling up from halfway each time.

## AHD envelopes

An `AHD` slot rises to full over its `Attack`, holds there, then falls to nothing
over its `Decay`, for a drum's punch or a pluck that doesn't care how long the note
is. It has the same curve, `Reset` and `Depth` as an ADSR, and its shape is drawn
the same way, with the hold a stage of its own between two dotted lines. Its times
are down the left, and its depth and reset down the right.

| Setting | Value |
|---|---|
| Curve | In the title: `Exp`, `Lin`, `Snap` or `Analog` |
| Attack, Decay | Milliseconds, up to 8000 |
| Hold | Milliseconds, up to 2000, or `Note` |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**A timed hold** plays through whatever the note's length: it holds for its time,
then decays, even if the note ended first or goes on long after, as the M8's AHD
does. On `Vol`, shaping the notes, it ends each note when its decay ends.

**Holding for the note,** `Note` lasts as long as the note does, then decays from its end, as
the Digitakt's `HOLD NOTE` does. It's below `0 ms`: `-` from `0 ms` goes to it, and
`=` comes back. Typing `note` sets it too, and a number sets a time. A new AHD holds
for 30 ms, with a 1 ms attack and a 200 ms decay.

## Drum envelopes

A `Drum` slot is the M8's drum envelope, for a kick's or a tom's pitch: each note
strikes it at full, it falls steeply to its `Body` over its `Peak`, the click's
sweep, then decays from there to nothing over its `Decay`, the note's drop. It's a
one-shot, whatever the note's length, and on `Vol`, shaping the notes, it ends each
note when its decay ends. Its peak, body and decay are down the left, and its depth
and ratchet down the right. It has no `Reset`, as it always strikes from full.

| Setting | Value |
|---|---|
| Curve | In the title: how its body's decay bends |
| Peak | Milliseconds, the strike's fall from full to the body |
| Body | The percent the peak falls to, where the decay starts |
| Decay | Milliseconds, the body's fall to nothing |
| Depth | How far full moves it, in hex. Negative flips it |
| Ratchet | How many times the peak strikes before the body, 1 to 8 |

**Ratchet** strikes the peak again: with `3`, it falls three times from full, a
`Peak` apart, then the body decays from the third. A low `Body`, a `Peak` of about
10 ms and a long `Decay` make an 808's clap from noise; `2` makes a flam. A new
drum envelope has a 10 ms peak onto a body of 40% and a 300 ms decay, with one
strike: on `Pitch` with a `Depth` of about `+18`, a kick's drop. The peak's fall is
always steep, whatever the curve.

## Low-pass gates

An `LPG` slot is a low-pass gate's vactrol, after Mutable Instruments' Plaits: each
note pings it, it rises over its `Attack`, then falls, fast at first and slower as
it nears silence, over its `Decay`. As it falls it also closes the track's filter,
by its `Colour`, so one slot both fades a note and dulls it as it dies, for plucks,
drums and the bongo sounds a West Coast synth makes. It's a one-shot, and on `Vol`,
shaping the notes, it ends each note when its fall lands. It has no curve, as the
vactrol's fall is the point of it, so its title has no mode: `LPG  Uni  → Vol`.

| Setting | Value |
|---|---|
| Attack | Milliseconds, a straight rise |
| Decay | Milliseconds, the time to fall 60 dB |
| Colour | How far it closes the filter at silence, in hex |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | On, it starts from zero each note; off, it goes on |

**Colour** is in `Cut`'s own steps: with the track at `Cut 80`, `Colour 40` takes the
filter down to `40` as the note dies, and `00` leaves the filter alone. The filter
follows the vactrol's brightness, which falls much faster than its level, as
Plaits' does, so even a long decay darkens early. It adds to the track's `Cut` and
whatever else moves it, whatever the slot's `Dest` and `Depth`, and works only
while the slot goes somewhere: one on `---` does nothing. A new gate has a 1 ms
attack, a 400 ms decay and a `Colour` of `C0`.

## LFOs

An `LFO` slot moves around the current value, so `CUT40` on a step moves the centre
and the LFO carries on around it. It draws one cycle of its wave in a box on its
right, starting from its `Phase`, upside down for a negative `Depth`, with a dot
riding it where the LFO is now. `Random` and `Smooth` draw only a centre line, as
their values aren't known ahead.

| Setting | Value |
|---|---|
| Mode | In the title: how notes start it, below |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` `Smooth` |
| Rate | The length of one cycle. `=` and `+` are always faster |
| Depth | How far it swings each way, in hex. Negative flips it |
| Phase | Where in the cycle it starts, in degrees |
| Fade | `Off`, or how long it fades in (`+`) or out (`-`) after each note |
| Deform | How its wave bends, in hex from `-FF` to `+FF` |

With `Cut 80` and `Depth +40`, the filter moves between `40` and `C0`. `Saw`
falls and `Ramp` rises. `Random` picks a new value each cycle, and `Smooth` glides
through one after another, never stopping at any, which suits a slow drift on
`Cut`. An LFO's `Shape`, `Rate` and `Depth` are down its left, and its `Phase`,
`Fade` and `Deform` down its right.

**The mode** is the Digitakt's five, in the title:

| Mode | Does |
|---|---|
| `Free` | Runs from the song's position, whatever the notes do |
| `Trig` | Each note starts it again at its `Phase` |
| `Hold` | Runs free, and each note holds where it is until the next |
| `One` | Each note plays one cycle, then it stays where it ended |
| `Half` | The same, for half a cycle |

`Hold` is a sample and hold of the wave. `One` is like an envelope, and `Half`
makes a rise and fall from `Tri`, or a single sweep from `Saw`.

`Random` in `Hold` gives each note a new value, and `One` or `Half` on `Cut` is a
shaped sweep a note long. `Half` draws its half cycle, then a flat line where it
stays, and the dot of a held or ended LFO sits where it holds. Before a track's
first note, `Hold` holds where its wave starts, and `One` and `Half` are already
at their end.

**Fade** fades the LFO in or out over a note division or bars after each note,
from `1/64` to `16 bars`: `+1 bar` brings it in from nothing over a bar, and
`-1 bar` takes it out to nothing. `=` goes from the longest fade out, through
`Off`, to the longest fade in, and `+` and `_` skip the dotted and triplet lengths.
It counts from the track's last note in every mode, so in `Free` only the fade
starts again, not the wave. Before a track's first note, a fade in is done and a
fade out is over. `K` lists them all, and `i` types one, like `+1/4` or `-2 bars`.

**Deform** bends `Sine`, `Tri`, `Saw` and `Ramp` between their peaks, which stay
where they are: raised, the wave bulges up and spends longer high, as a log curve
does; lowered, it sags and spends longer low, as an exp curve. On `Square` it's the
width: `+80` is high for three quarters of the cycle, `-80` for a quarter. The
random shapes have nothing to bend. The picture draws the bent wave. A step locks
it with `DFM`, as `DFM4 C0`, where `80` is none.

| Type this as a rate | For |
|---|---|
| `1/4` `1/8d` `1/16t` | A note division, dotted or triplet |
| `2 bars` | Whole bars |
| `7st` | A number of steps, to slide against the bar |
| `2.5hz` | Free of the tempo, from `0.01hz` to `100hz` |

A large step is `10` in hex, or 90° of phase. On a rate it skips
dotted and triplet divisions, moves 4 steps, or doubles or halves the hertz.

`i` on the mode, Dest or Shape opens the list too. `K` on Rate lists the note
divisions and bars, and `i` is for anything else, like `7st` or `2.5hz`. `K` on
Fade lists every fade, out, `Off` and in.

**Reaching other slots.** A slot's `Dest` lists the depth, phase and deform of
every slot after it, as `M5 Depth`, `M5 Phase` and `M5 Deform`, so `Mod 4` can bring in `Mod 5`'s depth,
and an envelope in `Mod 2` can bring in an LFO's depth, or a later envelope's, with
each note. It only goes one way, so `Mod 6` can't reach any other. `Rate` can't be
moved, as LFOs work out where they are from the song's position.

## When an LFO restarts

Playing from the top restarts every LFO. A `:reset` doesn't, since that is
about patterns. To follow the steps, set its mode to `Trig`, `One` or `Half`, or
`Hold` to take one value from it at each note. While stopped, a track sits at its
held values, so `;` hears those. Pausing freezes the LFO.

`Vol`, `Send A` and `Send B` swing in dB, on the level scale in **Phrases and
chains**, so `Depth +40` on `Vol` is 16 dB each way. They stop at `F8`, which
is 6 dB above where `VOL` starts, so a deep swing stays even if you lower `VOL`
on a step.

## Steps

A `Steps` slot plays up to 16 values in turn, a short sequence for any setting it
reaches, as the M8's tables and Bitwig's Steps do. Its values are a row of thin bars
in the box on its right, up from the middle for a value above `00` and down for one
below, with the step playing lit. A new Steps slot has 8 steps of `00` at a `1/16`,
started by each note, and choosing `Steps` for a slot with no depth gives it `+FF`,
so its values are heard as written.

| Setting | Value |
|---|---|
| Mode | In the title: `Trig`, `Free` or `Note`, below |
| Loop | In the title: what it does at its end, below |
| Dest | In the title: what it moves, such as `Pitch`, or `---` |
| Rate | How long each step lasts: `Tick`, `1/64` to `1 bar`, or `Pass` |
| Length | How many steps it plays, from 1 to 16 |
| Depth | How far, in hex: at `+FF` the values are heard as written |
| Start | The step each note starts it at |
| Mutate | The chance each value is drawn again as it comes round, `0%` to `100%` |
| Smooth | How it goes from step to step, in hex from `-FF` to `+FF` |

**The mode**: `Trig` starts it at its `Start` with each note, and waits for the
first; `Free` runs from the song's position, as an LFO in `Free` does; and `Note`
moves on one step with each note, so its `Rate` shows `--`. A negative `Depth`
turns the values over.

**Its values** are in its units at a depth of `+FF`, so `+07` on `Pitch` is seven
semitones up and `+10` on `Cut` is `10` above the track's cutoff. `l` from the grid
goes into the box, on the first step, and the cursor is a `▲` under it:

| Keys | Does |
|---|---|
| `h` `l` | Across the steps, with counts. `h` from the first goes back to the grid |
| `0` `$` | The first or last step |
| `=` `-` | Up or down by one |
| `+` `_` | Up or down an octave, `0C` |
| `i` | Type a value, signed hex such as `-0C` |
| `x` | Back to `00` |

Steps past the `Length` keep their values and aren't drawn, so shortening and
lengthening it finds them again.

**Mutate** is the Turing Machine's knob: each time a step comes round, it's drawn
again with that chance, anywhere within the slot's `Depth`, and keeps what it drew
until it's drawn again. `0%` plays the values as written, `100%` a new one each
time, and between them the loop drifts. It draws from the project's seed, so a
render repeats, and back at `0%` the values are as written again.

**Smooth** at `00` steps. Up to `+80` each step glides in from the one before over
its first part, all of it at `+80`, and on to `+FF` the glide becomes a curve
through the steps. Down to `-80` each step falls to nothing over its last part, a
pluck a step, and past it the fall is sharper. It follows time, so in `Note` mode
or at `Pass` the steps are as they are.

**The rate.** `Tick` is a sixth of the track's step, as `DEL` counts, so six go by in
each step, for a fast burst. `Pass` moves on once each time round the track's chain,
counted as `CND` counts passes, so `00 10 20 30` on `Cut` opens the filter over four
times round and starts again with the chain. `=` is faster, as an LFO's is.

**The loop**, after the mode in the title:

| Loop | Does |
|---|---|
| `Once` | Plays to the end and holds the last |
| `from 1` to `from 16` | Plays once, then loops from that step, as the M8's `HOP` |
| `Bounce` | Goes back and forth |
| `Shuffle` | Each step once each time round, in a new order each time |

`from 3` on four steps plays an attack of two, then holds a shape of the last two.
`Shuffle` deals its orders from the project's seed, so a render repeats.

`K` on the mode, the loop or the rate lists them. Some uses:

*   `+0C +07 00` on `Pitch` at `1/32`, `Once`: a flourish on each note.
*   Eight cutoffs in `Note` mode: each note gets the next.
*   `+10 -10` on `Pan` in `Free` at `1/16`: a stepped LFO.

**Start** is the slot's phase, so a step locks it with `PHS`: its value times the
`Length` over 256 picks the step, so `PHS3 80` starts an eight-step run on step 5.
`DEP` locks its depth, as for any slot. A later slot's `Dest` lists it as `M3 Start`.
Steps have no polarity, as their values say which way.

## Track

A `Track` slot follows each note: its pitch, for key tracking, how hard it's played,
or a value rolled for it. What it follows is its mode, in the title, so
`Track  Note  Bi  → Cut` opens the filter for the high notes and closes it for the
low ones. It takes its value as each note starts and holds it to the next, and
before the first note it moves nothing.

| Setting | Value |
|---|---|
| Source | In the title: `Note`, `Velocity` or `Random`, below |
| Dest | In the title: what it moves, such as `Cut`, or `---` |
| Low | Where its map starts: a note such as `C-1`, or hex |
| High | Where its map ends |
| Depth | How far `High` moves it, in hex. Negative flips it |

**The map** is none of the `Depth` at `Low` and all of it at `High`, a straight
line between and flat either side. Its picture in the box on its right goes across
all of the source's values, with a faint line down at `Low` and at `High`, and a dot
where the last note fell. `Low` above `High` turns it round, so the low notes or the
soft ones move it most. The same in both is a switch: nothing below it, all of it
from there up.

| Source | Follows |
|---|---|
| `Note` | The note as it plays, `c_1` to `G-8` |
| `Velocity` | The velocity as it's heard, `00` to `7F` |
| `Random` | A value rolled at each note, `00` to `FF` |

`Note` is after `ACU`, `ARP` and the chain's transpose, and a new slot maps `C-1`
to `C-5`. `Velocity`, the default, is after `VDLT` and the groove. `Random` rolls
from the project's seed.

Each source keeps its own `Low` and `High`, so trying another and coming back finds
them as they were. `Random`'s rolls are the slot's own, so a render repeats and
`CHA` rolls as it did without it.

**Key tracking.** `Note` at `Bi` is centred half way between `Low` and `High`: with
`C-1` to `C-5`, `C-3` leaves the setting where it is, and at a `Depth` of `+30` each
octave moves `Cut` by `18`, up above `C-3` and down below it. At `Uni`, the polarity's
own, it rises from the setting: velocity from `00` to `7F` on `Vol` or `Cut` adds up
to the `Depth` for the hardest notes.

`=` and `-` move `Low` and `High` by a note or `01`, and `+` and `_` by an octave or
`10`. `i` types a note, such as `C-3`, or hex. `x` puts back the source's own. On
the Roto, `Low`, `High`, `Depth` and `Source` are the first four knobs, with the
polarity on the button under `Depth`, and `Dest` on the next page.

## Pitch in scale

Any slot's `Dest` lists `Pitch` twice: as it is, and `Pitch in scale`, which moves
the note along the song's scale (`:scale`) to the nearest of its notes, the lower of
two as near: `+03` from `C-3` in C major plays `D-3`. So a random LFO, or Steps with
`Mutate`, on pitch play in key; a `Depth` of `+0C` keeps them within an octave. Only
what the slot adds moves: the notes as written never do, and with the scale off
it's plain `Pitch`.

## The accumulator

The `Accumulator` device, under the envelopes, says how the track's `ACU`s move its
notes from pass to pass: `Up` and `Down`, how far from the note as written, in scale degrees with
the scale on and semitones with it off; `Moves`, `Row` for only the row with the
`ACU` or `Track` for every row; `Limit`, `Wrap` `Bounce` or `Hold`; `Adds`, once a
row, on each `MLT` pulse or on each `RET` hit; and `Reset`, when it goes back to
the notes as written. `=` and `-` move each, `+` and `_` move a limit by 7, `i`
types a limit and `K` lists the others. See **FX commands**.

## The arpeggio

The `Arpeggio` device, beside the accumulator, says how the track's `ARP`s play: `Rate`, how long each note
is, `Tick` or a note division from `1/128` to `1/8`, as a share of the track's step;
`Mode`, `Up` `Down` `Up-down` `Up-down-inc` or `Random`; `Oct`, how many octaves it climbs, 1 to 4;
and `Retrig`, `off` to only change the pitch, sliding on `Acid`, or `on` to start
each note again. `=` and `-` move each, `i` types the octaves and `K` lists the
others. See **FX commands**.

## The mixer

Press `Space 6` for a strip for each track: its name, a meter of what it's playing
with `L` and `R` bars, a fader beside them showing its `Level`, then its `Trim`,
`Level`, `Pan`, `Send A`, `Send B`, mute and solo. It opens on `Level`. The meters are on the same scale as the main one,
with `0` marked, and show the track after its level and pan. `h` and `l` move between
tracks, `0` and `$` to the first or last, `j` and `k` up and down the strip, and `gg` and
`G` to its top or bottom. Values edit as they do on the track page, with `=` `-` `+` `_`,
`C-a` `C-x`, or Cmd with `l` `h` `k` `j`, and `x` puts one back to its default. On mute
and solo, `=` switches on, `-` switches off, and `K` or `i` changes it. `/` jumps to a setting by its name, as
it does on the track page, staying on the cursor's track.

While a step's `LEV`, `PAN`, `SNA` or `SNB` drives a fader, the strip shows the
step's value in the playing colour, and the fader beside the meters moves with it.
What `VOL` and modulation make of the level is a thin line across the fader, which
stays where it is. Moving the value by hand takes it over from the steps, marked
`󰝁`, until `Space e` gives the track's faders back, or `Space E` every track's: see
**Fader automation and relative modulation** in **FX commands**.

The tracks come first, then the groups, then `A`, `B` and `M`, with a gap between each
part, and a rule over each part's strips names it, `Tracks`, `Groups` or `Returns`, the
master being under `Returns`. The part with the cursor has the brighter rule. Under a
track's name is the group it's in, and under a group's name the tracks
in it, as `01-04 07`. `z a` folds a group's strips into its own, `z M` folds them all
and `z R` opens them; see **Folding a group**.
