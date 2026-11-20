<!-- ANCHOR: before-zoom -->
# The FX page and its effects

The FX page has 4 slots, which the sound goes through in order after the filters and
before `Level` and `Pan`. An empty slot does nothing and costs nothing.

Each slot is a device: a frame with the slot's number, its switch, its effect and the effect's
type in its title, and for `Beads` its quality and for `Reel` its oversampling too, its settings in three rows of up to four, and, for `Comp`, `Limit` and `EQ3`,
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
| `dd` on a setting | To nothing: lowest, off, middle or first choice |
| `h` `j` `k` `l` | Through the settings, and up to the title |
| `0` `$` | The first or last setting on the row |
| `{` `}` | Previous or next slot, staying on the same setting |
| `dd` on the effect | Take the effect out of this slot |
| `p` | Put it in this slot, swapping with what's there |
| `b` | Bypass this slot, to hear without it, or bring it back |
| `z i` `z o` | Zoom in on this slot, or back out to every slot |

`K` on the effect lists them by kind, roughly in the order a sound goes through them, with
`---` for an empty slot first and loading or saving an effect preset last:

| Group | Effects |
|---|---|
| Dynamics | `Comp`, `Limit` |
| Tone & drive | `EQ3`, `Distort`, `Tape`, `Reel`, `Deck`, `Exciter`, `Colour` |
| Filter | `LPG`, `Vowel` |
| Modulation | `Chorus`, `Phaser`, `Flanger`, `Rotary` |
| Pitch & spectral | `Pitch`, `Freq Shift`, `Warps` |
| Resonators | `Reso`, `Comb` |
| Delay | `Delay`, `Space Echo`, `Stutter` |
| Reverb | `Reverb`, `Plate`, `Hall`, `Spring`, `Space` |
| Granular | `Grains`, `Clouds`, `Beads` |

`=` and `-` on the effect step through them in this order too, from `---` to `Beads` and round
again, as the Roto's knob does.

Each effect places its own settings: what comes out, `Mix` and `Gain`, is in its last column, a
`Comp`'s detector, times and output each have a column, an `EQ3`'s bands are its columns, and a
`Delay`'s repeats, their tone and what comes out are its. A row can have gaps, which `h` and `l`
step over. `j` and `k` keep to the column you were in, landing on the nearest setting where the
row has nothing there, and go on into the next slot's title. The effect is over the first column
and its type over the second: the `Type` of a `Distort`, `Chorus` or `Comp`, and the `Mode` of
an `LPG` or `Comb`, are in the title rather than among the settings. `{` and `}` keep the row
and column, so they come back after an empty slot, and `2j` goes two rows down. `b` and `p` work
from any row of a slot, and `dd` takes the effect out from its name, in the title.

`dd` then `p` is one undo step, and an effect can move to another track's slot too. Modulation and
step commands stay with the slot, so they reach whatever's there now.

A bypassed slot goes faint and crossfades out over 5 ms, so it doesn't click. It keeps running
underneath, so a reverb's tail or a compressor carries on as if it had never gone, and it's
saved with the song. Emptying the slot brings it back in.

A step switches a slot out for its note with `1BYP 01`, for slot 1, and back in with `00`,
as a lock: the next note brings back what `b` set, and held with `h` it stays. While the song
plays, the page fades the slot as it's heard, so it goes faint and comes back with the lock.
`K` lists it with the slot's effect, and LFOs and envelopes can't move it.

## Zooming in on a slot

`z i` zooms the FX page in on the slot the cursor is in, and `z i` again or `z o` goes back to
every slot. The status line says `ZOOM`. Across the top, a strip keeps the whole chain in sight:
a tile for each slot, with its switch, its effect and its type, the first row of its settings by
their short labels, and a `Comp`'s or `Limit`'s gain reduction as a bar. Under it is the slot,
the page's width, with its settings in the same places and bars nearly twice as long.

| Effect | Zoomed in, over its settings |
|---|---|
| `Comp` | Its curve, its gain reduction meter and the last two bars |
| `Limit` | Its gain reduction meter and the last two bars |
| `EQ3` | Its curve, across the whole width |
| `Distort` | Its transfer curve |
| `LPG` | Its curve as far as it's open, and open all the way |
| `Reso`, `Comb` | Its curve at the note playing |
| `Vowel` | Its curve |
| `Delay`, `Space Echo` | Its repeats over time, left up and right down |
| `Reverb`, `Plate` | How its tail dies away |
| `Hall`, `Spring` | How its tail dies away |
| `Space` | How its tail dies away |

A `Comp`'s curve is square and 16 rows tall, and its meter as tall, 0 to 24 dB, with the last two
bars of how far it turned down beside them. A `Distort`'s transfer curve, on `Soft`, `Hard`,
`Fold`, `Sine` and `Crush`, is what each level coming in goes out as, with `Mix` in it.

The filter effects' curves are what each lets through of a quiet sound at each frequency, from
20 Hz to 20 kHz and -36 dB to +24 dB, as the Sound page's filter curve is, with `Mix` in them. A
`Reso`'s and a `Comb`'s peaks sit on the note the track is playing, with `Tune`, and its
harmonics, with a dashed line at that note, so the curve moves as the notes do; its narrowest peaks
are drawn to their tops. An `LPG`'s moves as what it
hears opens and closes it, with a dashed line at its cutoff; the dotted curve under it is where it
goes opened all the way, so what lies between is how far it moves.

A `Delay`'s repeats are lines along the time from the sound, as loud on the left as they reach up
and on the right as they reach down, from -60 dB in the middle to 0 dB at the edges, so a
ping-pong's cross from side to side. The sound itself, with `Mix`, is first, at the left, and
fainter lines mark the beats. Each repeat is as loud as it is where `Low cut` and `High cut` pass
the most. A reverb's tail falls from the top, after its pre-delay, 60 dB in its decay time at its
lows, and a dotted line falls as its highs do, which `Damp` shortens. The box says how long each
takes, and how far apart the lines down it are.

The last two bars are a sweep, as an oscilloscope's: it writes from left to right as the song's
tempo goes, with a gap after the newest, and starts again at the left after two bars, so a beat's
pumping stands still and the sweep shows where it is now. Any other effect, and `Down` and the
pedals, which have no curve, shows its settings alone, so `{` and `}` go through the whole chain
without leaving the zoom. `j` and `k` go on into the next slot's title as before, which zooms in
on that one. The FX page stays zoomed while another page shows.
<!-- ANCHOR_END: before-zoom -->
<!-- ANCHOR: before-clouds -->

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

`Type` locks from a step, as `1TYPE` in slot 1, and a macro, a lane or a mod slot can move
it, a zone of its range for each type. A change while a sound plays crossfades from the old
type to the new over 2.7 ms, as a note cut off fades, so the new type's filters and
oversampling, which start afresh, don't click.

A mod slot can move the ones after it: `Mod 4`'s `Dest` has `M5 Depth`, `M5 Phase`, `M6 Depth` and
`M6 Phase`, and `Mod 2`'s has every later slot's, so an envelope can bring in an LFO's depth with
each note. It only goes one way, so `Mod 6` can't reach any other. See **LFOs** in **The Mod
page, envelopes and LFOs**.

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
| `Decay` | How long its tail takes to fall 60 dB at 1 kHz |
| `Damp` | How quickly its highs fall away as it rings |
| `Low damp` | How quickly its lows fall away as it rings, off at `00` |
| `Low cut`, `High cut` | Shape what goes in |
| `Mod` | How much its tail wobbles; still at `00` |
| `Width` | From `00`, sides together, to `FF`, as wide as it comes |

`Decay` reads in seconds, from a quarter of one at `00` to a minute at `FF`: `60` is 2 s, and
each `20` about doubles it. It's what the middle of the tail does, measured at 1 kHz: the lows
ring nearly twice as long, as a hall's do, and `Damp` shortens the highs and darkens a hit a
little as it goes in, the tail more than its start. A hit is answered at once, as a hall's
first reflections answer it, different on each side and with no separate echoes to flam a
snare, and it fades smoothly into the tail by a fifth of a second. Under a second of `Decay`
that first answer is quieter, so a short room stays short.

`Mod` starts at `20`: the tail wanders in pitch a little, each part its own way, so no note
rings on its own, a little less than a hall plug-in's does. Turned up, the tail turns to a
chorus. At `00` nothing in it moves, and every hit through it comes out the same.
`Low damp` keeps a long tail from going muddy under a kick, where `Low cut` only takes the
lows from what goes in. `Width` leaves the two sides added together
as they were, so it sums to mono the same.

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

`Space Echo` is a Roland RE-201's tape loop: a record head, three playback heads behind it and a
spring reverb, fitted to Arturia's Delay TAPE-201. It starts as TAPE-201's `Default` does: the
first head and the spring, 150 ms behind, fed back a third of the way to running away, with `Mix`
at `80`.
Its `Mode` is in the slot's title, the RE-201's selector, by its number, the heads it plays and
whether the reverb's on:

| Mode | Plays |
|---|---|
| `1: H1`, `2: H2`, `3: H3` | The first, second or third head |
| `4: H2 H3` | The second and third |
| `5: H1 +R`, `6: H2 +R`, `7: H3 +R` | The first, second or third, with the reverb |
| `8: H1 H2 +R` | The first and second, with the reverb |
| `9: H2 H3 +R` | The second and third, with the reverb |
| `10: H1 H3 +R` | The first and third, with the reverb |
| `11: H1 H2 H3 +R` | All three, with the reverb |
| `12: R` | The reverb alone |

The second head is twice as far from the record head as the first, and the third three times, so
a mode of two or three heads plays a rhythm: `4`'s repeats come on two and three, then on four,
five and six as they go round. Each head of two or three is heard at a half or a third, so every
mode is as loud.

| Setting | Does |
|---|---|
| `Rate` | The tape's speed: the first head 1 s behind to 10 ms |
| `Sync` | `Free`, or a note division the first head keeps to |
| `Inertia` | How slowly the motor gets to a new speed |
| `Intensity` | How much goes round again. From about `DC` it runs away |
| `Input` | How hard the tape's driven, ±24 dB, as it comes at `80` |
| `Wow` | How far the tape's speed wavers |
| `Bass`, `Treble` | Lift or cut what comes out, flat at `80` |
| `Reverb` | How much of the spring, in the modes that have it |
| `Echo` | How loud the repeats are |

`Rate` moves the tape, not a delay time, so a change bends what's already on it: slowing the tape
drops the repeats' pitch until the motor gets there, as the machine's does, and `Inertia` makes
that slower and gentler. The motor pulls harder than it brakes, so speeding up bends the pitch up
further and gets there sooner than slowing down bends it down. A step's lock of `Rate` is the classic swoop. `Sync` sets the first
head to a note division at the song's tempo instead, up to two seconds, and a new tempo moves
the tape the same way.

Each repeat goes through the tape once more than the last: darker, as the playback head takes its
highs, a little thinner in the lows, and saturated, as the record head holds what's loud under a
soft ceiling, so loud repeats thicken rather than clip. A faster tape keeps more highs. Turned up
past where the loop gives back what it takes, the repeats grow by themselves until the tape holds
them, a sound of their own that `Rate` then bends. `Wow` wavers as the capstan does, faster as
the tape runs faster, by up to about 21 cents, with a little drift that comes from the track's
seed, so a render is the same each time. `Bass` and `Treble` are on what comes out, the repeats
and the reverb, and not in the loop.

The reverb is the echo's own spring tank, fed from what comes in. A sound drips into it about
50 ms later, its lows first and its highs rising after them, then rings out in a smooth, dark
tail of about four seconds, mostly between 300 Hz and 2 kHz, so it sits under the repeats
rather than over them. It doesn't rattle as `Spring` does: for a full, bright spring reverb on
its own, use `Spring`.

`Mix` keeps the sound at full while it brings the echo up to full at `80`, then takes the sound
down to nothing at `FF`, as TAPE-201's does. So turning it up from `00` to the middle only adds
echo, and its `80` start has the sound and the echo both at full.

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

`Type` locks from a step, as `1TYPE` in slot 1, and a macro, a lane or a mod slot can move
it, a zone of its range for each type. Each type reads its copies from elsewhere in the line,
so a change while a sound plays fades the copies out over 2.7 ms and the new type's in over
2.7 more, the dry sound carrying on under them.

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

`Reel` is a studio reel-to-reel machine, as used for mastering: aligned,
biased right and subtle as it starts. It's built from the physics rather
than by ear: the tape's magnetism follows the head's field round its curve,
so loud sounds thicken, their peaks soften and the mix glues; the playback
head loses its highs to the tape's spacing from it and the depth it's read
from, and the replay EQ makes those up at every speed, so a quiet sound
comes back with its top intact. Loud highs soften at 3.75 ips, as
short wavelengths erase themselves; the heads leave a bump in the lows with
a dip under it. Its ranges are those of a studio machine: for obvious,
broken tape, use `Deck`. Its `Oversampling`, in the title, is how many
times the rate the tape is worked at: `4x` as it starts, or `8x`.

| Setting | Does |
|---|---|
| `Drive` | How hard it hits the tape, -20 dB to +20 dB, 0 dB at `80` |
| `Sat` | The tape's headroom: higher saturates sooner |
| `Bias` | Right at `80`: under is dirtier and brighter, over cleaner and duller |
| `Speed` | 3.75, 7.5, 15 or 30 ips: slower softens loud highs; the bump moves |
| `Gap` | The head's gap, 1 to 5 µm, 3 µm at `80`: narrower is brighter |
| `Space` | How far the tape rides off the head, to 1 µm: highs go fast |
| `Thick` | How deep the coating is read, to 2.4 µm: highs soften gently |
| `Wow` | A slow, smooth drift in pitch, from the reels |
| `Flutter` | A fine, fast wobble, from the capstan and rollers |

A quiet sound comes through at its level at any `Drive` and `Sat`, so
turning them up saturates it sooner rather than turning it up; at 0 dB a
-12 dB sine's distortion is about -54 dB, and each 6 dB of `Drive` is the
same as 6 dB louder. Under-biased, toward `00`, it's less sensitive and
dirtier, and driven loud its harmonics come and go as the level moves. Each
speed has its own top, as an aligned machine's has: at 30 ips the highs are
about 2 dB up. `Gap` turns the top from about 7 dB up at `00` to 2 to 5 dB
down at `FF`, and the level a dB or two the other way. The replay EQ is set
for the `Space` and `Thick` it starts with, so moving them away from there
brightens or darkens the top; at most they take about 10 dB off 16 kHz at
3.75 ips, and less the faster it runs.
The head bump at 15 ips is about 1.3 dB near 62 Hz, with the lows falling
away under 30 Hz; at 7.5 ips it's near 39 Hz and at 30 ips near 80 Hz.
`Flutter` reaches about 10 cents at `FF`, evenly from none at `00`, and
moves with the speed: the light, tremolo-like warble of a worn capstan.
`Wow` is slow: the pitch leans sharp and flat and back, without a pulse.
It goes as far as `Flutter` up to `80`, about 5 cents there, then rises
faster to about 25 cents at `FF`: the top quarter goes past what mastering
tape does, into a seasick effect. The deeper `Wow` is, the later the tape is
read, up to about 30 ms at `FF`, so with `Mix` below `FF` a deep `Wow` also
brings a short delay against the dry sound. At the start there's about a cent,
nearly all flutter. Unlike `Tape` it has no ceiling: a loud sound driven
hard comes out quieter, as tape squashes it.

`8x` sounds the same, with less aliasing: the faint tones that don't
belong to the note, and the grit on bright, loud sounds. It matters most
with `Drive` well up on high leads, hats or a bright mix: on a high saw
lead at +20 dB the aliases are about 35 dB lower, and at 0 dB about
14 dB. It costs nearly twice as much, so `4x` stays the start, for when
CPU counts. A change while a sound plays crossfades in about 10 ms, so it
doesn't click, and `1OVS` sets it from a step in slot 1.

`Deck` is lo-fi tape: a cassette, a VCR's linear audio track, or a worn
cassette, with everything that goes wrong with them. Its `Type`, in the
title, is the format, which sets its highs and lows, its hiss, how fast it
wobbles and how wide its stereo is:

| Type | Its sound |
|---|---|
| `Cassette` | Highs to about 12 kHz, a gentle wow at about 0.6 Hz |
| `VHS` | Softer highs, thin lows, and its stereo narrowed by crosstalk |
| `Worn` | Dull and loose, and it drops out now and then by itself |

| Setting | Does |
|---|---|
| `Drive` | How hard it hits the tape, from -18 dB to +18 dB, 0 dB at `80` |
| `Hiss` | The tape's hiss, from none at `00` to about -40 dB at `FF` |
| `Wow` | A slow, wandering wobble in pitch, never quite the same twice |
| `Flutter` | A fast wobble in pitch, from the capstan |
| `Drops` | How often and how deep the sound dips, dulled, for a moment |
| `Chew` | Now and then a stretch of crinkled tape, bent and dulled |
| `Track` | Noise reduction, mistracking more as it goes up |

`Drive` saturates the highs first, as tape's pre-emphasis does. `Track` at
`00` is no noise reduction at all; just above, it's a Dolby that nearly
tracks, and higher up it reads the tape low, so the highs come back dull and
breathe with the level. The hiss stops a few seconds after the sound does,
so a quiet track falls silent. The wobble, dropouts and chews are drawn from
the track's own dice, so a song plays the same each time.

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

`Colour` is analog saturation, the warmth a console, a tube preamp or a
pedal's diodes give: a circuit's curve that bends what's loud, between two
filters that decide whether the highs or the lows bend first, and a
transformer's lows. Its `Type`, in the title, is the circuit:

| Type | Its curve |
|---|---|
| `Warm` | Smooth and even both ways, as a console's |
| `Tube` | A 12AX7 triode's: soft, lopsided, with a strong second harmonic |
| `Diode` | Two diodes back to back: soft, and it never quite stops rising |
| `Transistor` | Cuts off softly one way and saturates hard the other |

| Setting | Does |
|---|---|
| `Drive` | How hard it hits the curve, from none at `00` to 24 dB at `FF` |
| `Even` | Tips the curve to one side, for a second harmonic that thickens |
| `Hard` | Sharpens the knee toward a hard clip, for brighter odd harmonics |
| `Emph` | Flat at `80`. Above, the highs bend first, as on tape; below, last |
| `Iron` | A transformer's lows: loud bass saturates, lifted a little at 60 Hz |

At `Drive` `00` a quiet sound comes out as it went in, whatever the other
settings: only what's loud is bent, so it starts as gentle colour and turns
into drive as `Drive` goes up, louder as it does, which `Gain` takes back.
`Emph` lifts the highs before the curve and cuts them after by the same,
so it changes what's bent and not the tone of what isn't. `Iron` bends the
lows more the lower and louder they are, so a kick or a bass thickens while
the highs stay clean. `Mix` blends it with the sound going in, and it
starts all wet. It's worked at eight times the rate, so even a hard clip on
a high note stays clean of tones that don't belong.

`Plate` is a second reverb, after Jon Dattorro's plate: brighter and denser
than `Reverb`, and wide from a single sound. It answers at once, its first
echoes blurred into a wash rather than heard one by one or repeating, so a
snare through it doesn't flam or roll. Its first tenth of a second is bright
and falls away quickly into a darker tail, as a plate's does. It starts half
wet and still, and its tail carries on after a note ends. `Pre`, `Decay`, `Damp`, `Low damp`, `High cut`
and `Width` work as `Reverb`'s do, and its tail is as long at every `Size`.

| Setting | Does |
|---|---|
| `Size` | How big the plate is, from half to twice Dattorro's, his at `80` |
| `Mod` | How much its tail shimmers, each line wandering its own way |

`Mod` starts at `00`, where nothing in the plate moves and every hit through
it sounds the same. Turn it up to keep a long tail from ringing metallic. Its
lines wander apart rather than sweeping together, so a held chord shimmers
without the flanging of one slow wobble. A change to `Size` bends the pitch of the tail a little as it glides,
taking about a second end to end.

`Hall` is a big, smooth hall: eight delay lines feeding each other, after
zita-rev1, with early reflections before the tail. It's for strings, pads,
vocals and piano, where `Reverb` is a room and `Plate` is bright. It starts
half wet, its tail about 2 s. `Pre`, `Decay`, `Damp`, `Low cut`, `High cut`
and `Width` work as `Reverb`'s do, and `Size` as `Plate`'s: its tail is as
long at every size.

| Setting | Does |
|---|---|
| `Lows` | How long its lows ring against `Decay`: as long at `80`, up to 4x |
| `Damp` | How long its highs ring: as long as the middle at `00`, a tenth at `FF` |
| `Size` | How big the hall is, from a big room at `00` to twice its own at `FF` |
| `Early` | How loud its first reflections are, off at `00` |
| `Mod` | How much its lines wobble, which keeps the tail from ringing |

`Mod` starts at `40`, a slight wobble, so a snare's tail barely changes from
hit to hit. Higher up the tail goes chorus-like; at `00` nothing moves, and the
hall answers every hit exactly alike.

`Decay` is the middle's tail, from 500 Hz to 2 kHz; `Lows` and `Damp` set the
lows' and highs' against it, so each band falls in its own time.

`Spring` is a spring tank, as a guitar amp or an Octatrack has, after
ChowDSP's model in Surge: a sound goes in, drips and boings down the springs
and comes back round. It suits dub chords, snares and techno stabs. It starts
half wet, and its tail carries on after a note ends. `Damp` works as
`Reverb`'s does; `Decay` goes from a tenth of a second to about four, and a
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
own settings. `Decay` rings on and on towards `FF`, and its tail doesn't
darken as it rings, as the others' do: `Damp` darkens what goes in and what
comes out.

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

`Clouds` is Mutable Instruments' Clouds as the module plays: it records the
sound all the time, with what it feeds back, and plays it back in one of
three modes, or takes it apart in a fourth, then through its diffuser,
filters and reverb. Its `Mode` is in the slot's title, picked with `K`:

* `Granular`, the module's own sound: grains read the recording.
* `Stretch` reads it through two windows, each lined up with the last where
  they match best, so it slows, freezes or shifts the sound without the
  grains' blur.
* `Looping` is a delay, its repeats shifted by `Pitch`. Frozen, it loops what
  it holds.
* `Spectral` takes the sound apart into its spectrum, frame by frame, keeps
  it, bends it and puts it back together, as the module's phase vocoder does.

The first two columns are the module's knobs, the third what its `Blend`
knob also does, each a setting of its own here. In `Granular`:

| Setting | Does |
|---|---|
| `Position` | Where grains read, from just now to the oldest |
| `Size` | How long each grain is, 32 to 512 ms, twice that at 8 bits |
| `Pitch` | How far each grain is shifted, in semitones, two octaves |
| `Density` | How many grains, and how they start |
| `Texture` | The grains' envelope, then the diffuser |
| `Freeze` | On, it stops recording, so grains play what's held |

In `Stretch`, `Position` and `Size` are where its windows read and how long
they are, and `Pitch` shifts it. In `Looping`, `Position` is the delay's
time, gliding there as tape does, and `Pitch` shifts the repeats; frozen,
`Position` is where the loop starts and `Size` how long it is. In both,
`Density` turns up the diffuser, and `Texture` is a low-pass below `80` and
a high-pass above it.

| Setting | Does |
|---|---|
| `Spread` | How far the grains spread across the sides |
| `Feedback` | How much of what comes out is recorded again |
| `Reverb` | How much of Clouds' own reverb comes out |
| `Quality` | Its recording: `Stereo 16`, `Mono 16`, `Stereo 8` or `Mono 8` |
| `Blend` | From `00`, the sound alone, to `FF`, all Clouds |

`Density` makes no grains at `80`. Below it they come at a steady rate, and
above it at random, more and more overlapping towards either end, as the
module's knob does past noon. `Texture` shapes each grain from a square
through a triangle to a bell, and above `C0` turns up the diffuser, which
smears them into a wash.

`Quality` sets how long the recording is, as the module's does: about 1
second in `Stereo 16`, 2 in `Mono 16`, 4 in `Stereo 8` and 7.4 in `Mono 8`.
The 8-bit ones record at half the rate, through mu-law, so they're duller and
grittier. Changing it starts a new recording.

`Mode` and `Quality` lock from a step, as `1MODE` and `1QUAL` in slot 1, and a
macro, a lane or a mod slot can move them, a zone of its range for each. A
change while a sound plays fades what's playing out over 10 ms and the new mode
or recording in over 10 more, so the filters, pitch shifter and diffuser it
starts afresh don't click. Its reverb rings on through it.

Frozen, with `Feedback` up, the feedback fades out and the reverb comes up in
its place, as on the module, for a long wash of what's held. `Blend` is the
module's, and loud sounds soft clip through it as they do on the module. It
starts half way, with a little reverb.

In `Spectral` the knobs take other jobs, as on the module:

| Setting | Does |
|---|---|
| `Position` | Which kept spectrum it writes and plays |
| `Size` | Warps the spectrum, none at `80` |
| `Pitch` | Shifts the spectrum, in semitones |
| `Density` | How often the spectrum is updated |
| `Texture` | Quantises it below `80`, flattens it above |
| `Freeze` | On, it plays what it holds, still turning |

It keeps two spectra in the stereo qualities and six in the mono ones, and
`Position` blends between them: freeze at `00`, let go, freeze again at
`FF`, and `Position` moves from one to the other. `Size` shifts and reverses
the spectrum towards either end. Below `80`, `Density` leaves bins as they
were at random, a partial freeze; above it, it merges each frame with the
last; and towards either end it scatters the phases, a spectral smear.
Below `80`, `Texture` makes the levels coarser and coarser, like a file at
a low bit rate; above it, the strongest bins weaken and the weakest grow,
towards noise.

Each frame is 128 ms, and what comes out is 160 ms late, as on the module;
at 8 bits both are twice as long, with finer bins. It has no diffuser or
filters, `Spread` does nothing, and nothing above 16 kHz comes out, as on
the module, which also makes it 6 dB louder than what goes in. Changing to
or from `Spectral` fades out what's playing first.

`Clouds` and `Grains` are siblings: `Grains` is Beads', Clouds' successor,
with its own envelopes and randomness, and `Pitch` and `Reverb` are Clouds'
own pitch shifter and reverb on their own. Its grains' places come from the
track's seed, so a render is the same each time, as are the random bins and
phases of `Spectral`.
<!-- ANCHOR_END: before-clouds -->
<!-- ANCHOR: rest -->

`Beads` is Mutable Instruments' Beads as the module plays: it records the
sound all the time, with what it feeds back, onto one of four kinds of tape
or memory, and plays it back as grains or as a delay, then through its own
reverb. Its `Mode` and its `Quality` are in the slot's title, picked with
`K`:

* `Granular`, the module's own sound: grains read the recording. These are
  `Grains`' grains, which are Beads'.
* `Delay` is the module with `Size` all the way up: taps read the recording
  at a delay `Density` sets. Frozen, a slice of what it holds loops.

A change of mode takes the module's time, about two seconds: the wet sound
holds a moment, fades out, and fades back in as the other.

| Setting | Does |
|---|---|
| `Time` | Where grains start, from just now to the oldest |
| `Size` | How long each grain is. Below `55` they play backwards |
| `Shape` | The envelope, from square through decay and bell to swell |
| `Pitch` | How far each grain is shifted, in semitones, two octaves |
| `Random` | How far each grain's time, size and shape stray at random |
| `Freeze` | On, it stops recording, so it plays what's held |
| `Density` | How often grains start, or how long the delay is |
| `Clock` | `Free`, by itself, or `Steps`, following the song's steps |
| `Feedback` | How much of what comes out is recorded again |
| `Reverb` | How much of Beads' own reverb comes out |
| `Blend` | From `00`, the sound alone, to `FF`, all Beads |

`Time`, `Size` and `Shape` are the module's knobs, and `Random` is its
attenuverters with nothing patched: each grain strays from them by as much,
and the more they stray, or the more at random grains come, the wider they
spread across the sides. `Size` and `Shape` run as `Grains`' do.

With `Clock` on `Free`, `Density` is the module's: no grains at `80`, evenly
below it and at random above, faster towards either end, up to about 130 a
second. On `Steps`, the song's steps are its clock, as a clock into the
module's `SEED`: below `80` a grain on every step, then every second, third,
fourth and on to every sixteenth, a bar, towards `80`; above it, each step
has a chance of a grain, which grows to every step at `FF`. Grains start on
the step.

In `Delay`, `Density` sets the repeats' time. On `Free` it's the whole
recording at `80`, shorter towards either end, down to a few ms, and above
`80` a second, uneven tap comes in. On `Steps`, it's a share of a step: a
sixteenth, an eighth, a quarter, a half or a whole one below `80`, and
others, a third, a sixth and so on, above. `Time` then multiplies the
repeats' time by 1 to 16, `Shape` puts an envelope on each repeat, none at
`00`, `Pitch` shifts them as a tape echo with rotating heads does, and
`Size` is their tone: a low-pass below `80`, a high-pass above, open at `80`.
Frozen, `Time` picks which slice of what's held loops.

`Quality` is the module's four, each a longer and dirtier recording:

| Quality | Records | Holds |
|---|---|---|
| `Hi-fi` | 16 bits at 48 kHz | 4 s |
| `Cold digital` | 12 bits at 32 kHz | 8 s |
| `Sunny tape` | 12 bits at 24 kHz | 10.7 s |
| `Scorched cassette` | 8-bit mu-law at 24 kHz, with wow and flutter | 16 s |

Each also feeds back its own way, from a clean limit at `Hi-fi` to a tape's
saturation at the others, and the dirtier ones' reverb is darker and rings
longer, as the module's does. `Scorched cassette` codes the dry sound too,
and its wow and flutter move all of it. Changing `Quality` starts a new
recording.

`Mode` and `Quality` lock from a step, as `1MODE` and `1QUAL` in slot 1, and a
macro, a lane or a mod slot can move them, a zone of its range for each. A mode
locked on one row starts the module's crossfade there. A quality changed while a
sound plays dips all of Beads, the sound going through it too, to nothing and
back over about 5 ms, and starts the recording, the reverb and the cassette
empty, so nothing clicks.

`Beads`, `Grains` and `Tape` are related: `Grains` is Beads' granular mode
on its own, with a `Spread` of its own, and `Tape` has Beads' wow and
flutter. Its grains' places, its random intervals and its chances come from
the track's seed, so a render is the same each time. The module's third mode,
a wavetable synth for when nothing's patched in, isn't here.

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
<!-- ANCHOR_END: rest -->
