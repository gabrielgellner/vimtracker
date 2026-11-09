# The track page, envelopes and LFOs

Press `Space 4` for what a track holds apart from its steps, on three pages: Sound
has its machine, filter and output, with a bar beside each value showing where
it sits between the lines at each end of its range, and moving with whatever
plays it, with a dim mark left where it's set, Mod its envelopes and
LFOs, and FX its effects. `H` and `L` go to the previous or next page. FX columns belong
to steps, and an LFO keeps moving on its own, like a filter drifting under a
hat pattern.

| Keys | Does |
|---|---|
| `H` `L` | The Sound page or the Mod page |
| `W` `B`, `]b` `[b` | Next or previous track |
| `j` `k` | Next or previous row, back in the column they left |
| `h` `l` | Previous or next setting. `0` and `$` go to the line's ends |
| `/` | Jump to a setting by its name |
| `=` `-` | Up or down a small step. A count goes further, as in `10=` |
| `+` `_` | Up or down a large step: the same keys with Shift |
| `D-l` `D-h` | Cmd and `l` `h`: as `=` `-`, without moving |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_`, without moving |
| `i` | Type a value. Hex levels enter themselves, others take `Enter` |
| `K` | On a setting with choices, like Machine or Dest, list them |
| `x` | Back to its default: centre pan, 0 dB and so on |
| `;` | `a` `w` `s` `e` `d` `f` play C to F, `<` `>` the octave, until `;` |

A note holds for as long as its key is down, and several keys down play a
chord, up to eight notes; a ninth takes over the oldest. Letting go of one key
ends only its note. `Acid` plays one note at a time, as a 303 does: a key
pressed while another is down slides to its note, and letting go of it slides
back to the one still down.

Cut, Res, Pan, the sends and the machine's settings that sweep have a fine part,
the dim pair of digits after the value, as FX values do in a phrase. `l` stops on
it before the next setting, and there `=` `-` `+` `_`, `i` and `x` work on the
fine part alone, carrying into the value past `FF` or below `00`. The value's own
digits keep it, and at the top there's none. An effect's settings are whole.

## The filter

| Setting | Value |
|---|---|
| Cut, Res | Where `CUT` and `RES` start |
| Drive | How hard it's driven, from `00` for clean |
| Filter | `LP` `HP` `BP` `Notch` `Ladder` `Diode` `MS20 LP` `MS20 HP`. `K` lists them |
| Low cut, High cut | The utility filter: `00` and `FF` are off |

Drive keeps the level about even. `Low cut` and `High cut` are a second, simple
filter after the main one, for thinning out a sound without giving up the main
filter's character. They run from 20 Hz at `00` to 20 kHz at `FF`, at 12 dB with
no resonance. `Low cut 00` and `High cut FF` are off, and off costs nothing.
Unlike `Cut`, steps and modulators don't move them.

`LP` is the clean default. `HP` needs `Cut` lowered to let anything through, as
it starts at `FF`. `Ladder` is fatter and steeper, its bass thins as resonance
rises, and near `FF` it sings by itself while a note lasts. `Diode` is the
303's filter, the one in `Acid`: rubbery and softer at first, as it starts
falling well under `Cut`, with a peak at `Cut` that grows sharp and squelchy
and thins the bass more than `Ladder` does. `MS20 LP` and `MS20 HP` are the
Korg MS-20's filter, at 12 dB and 6 dB. With much `Res` their peak is at
`Cut`. They keep the bass, or for `MS20 HP` the top, however high `Res` goes,
so they get louder and harsher as it rises, and near `FF` they scream by
themselves, much louder than `Ladder`. `Drive` makes them grittier. These four cost more than the
others, which matters only for tracks using them.

A curve in a box under the filter's rows shows what the filter and utility filter
let through, from 20 Hz on the left to 20 kHz on the right. It moves with what's
playing `Cut` and `Res`, including step commands, LFOs and envelopes, but it
doesn't show `Drive`'s saturation.

## Jumping to a setting

Press `/` and type part of a setting's name, like `cu`, and each match on this page
gets a label over its value, and again in its name, just after what you've typed. Press a label to land on
that value, ready for `=` and `-`. `Enter` goes to the nearest match, `Esc` stays where you were, and `Ctrl-O`
comes back after a jump.

A label is never a letter that could carry on your search, so you can keep typing to
narrow it, and a match keeps its label as you do. `/` searches the page you're on,
and `H` and `L` change page.

## Effects

The FX page has 3 slots, which the sound goes through in order after the filters and
before `Level` and `Pan`. An empty slot does nothing and costs nothing.

| Keys | Does |
|---|---|
| `K` or `i` on the effect | Choose what's in the slot. `=` and `-` step through |
| `x` on the effect | Empty the slot |
| `x` on a setting | Back to its default |
| `{` `}` | Previous or next slot, staying on the same setting |
| `dd` | Take the effect out of this slot |
| `p` | Put it in this slot, swapping with what's there |
| `b` | Bypass this slot, to hear without it, or bring it back |

A slot's settings are in columns of two. `j` and `k` go up and down them and on to the
next slot, and on the effect they go straight between slots. `2j` goes to the same place as `}`.
On a slot's bottom line, `0` goes to its first setting there, and `h` from it to the effect.

`dd` then `p` is one undo step, and an effect can move to another track's slot too. Modulation and
step commands stay with the slot, so they reach whatever's there now.

A bypassed slot goes faint and fades out over a moment, so it doesn't click. It keeps running
underneath, so a reverb's tail or a compressor carries on as if it had never gone, and it's
saved with the song. Emptying the slot brings it back in.

`Distort` has a `Type`: `Soft` and `Hard` clip, `Fold` and `Sine` fold the sound back
on itself, `Crush` lowers its bit depth, and `Down` lowers its sample rate. Each mode has its
own setting: `Amount` for how hard the four shaping modes drive, `Bits` on `Crush`, whole
numbers from `12` to `2`, where anything quieter than half a step drops out, and `Rate` on
`Down`, how long each sample is held. A mode keeps its own as you change to another and back.
Few bits gate quiet sounds, so one press up brings them in, and `Mix` blends it with the sound
going in, from `00` dry to `FF` wet. `Gain` turns down what comes out, as `Amount` can make it
much louder, on the level scale,
where `E0` leaves it as it is and every `10` is 4 dB. Every effect has one.

An LFO can move another: `LFO 1`'s `Dest` has `L2 Depth` and `L2 Phase`, and an envelope's has
both LFOs', so `Env 2` can bring in an LFO's depth with each note. It only goes one way, so
`LFO 2` can't reach `LFO 1`. `Rate` can't be moved, as LFOs work out where they are from the
song's position.

`K` on `Dest` shows it by group, opening on what's chosen:

| Keys | Does |
|---|---|
| `j` `k` | Move in the list, or choose a group |
| `h` `l` | Go to the groups or back to the list |
| `/` | Go to the search and back. It reaches every group |
| `Enter` | Choose, or on a group, go into it |

`Dest` and step commands only list what's loaded: a synth's settings under their names on
that voice, on a track playing it. One already chosen stays, reads like `Har ---`, and does
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
| `Time` | A note division, up to `1 bar`. A change glides |
| `Feedback` | How much comes round again. `FF` keeps going |
| `Ping-pong` | `On` goes side to side, and `Off` stays put |
| `Width` | From `00`, sides together, to `FF`, fully apart |
| `Low cut`, `High cut` | Take more from each repeat |
| `Drive` | Saturates louder repeats each time round |

`Chorus` thickens a sound with copies of it that drift in time, after Mutable
Instruments' Rings. It starts half wet, which is the classic sound. Its `Chorus` type
has two slowly drifting copies, and `Ensemble` three on a slow and a fast wobble, as a
string machine has.

| Setting | Does |
|---|---|
| `Type` | `Chorus` or `Ensemble` |
| `Rate` | How fast they move: as Rings has it at `80` |
| `Depth` | How far they move. `00` leaves fixed copies |

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
change it. Leave `Auto` off for ducking, or the bass is louder between kicks.

| Setting | Does |
|---|---|
| `Threshold` | Where it starts turning down, from -60 dB to 0 dB |
| `Ratio` | How much: 1:1 at `00`, 20:1 near the top, a limiter at `FF` |
| `Attack`, `Release` | How fast it turns down and lets go |
| `Source` | `Self`, a track by its number, or return `A` or `B` |
| `Auto` | `On` makes it come out as loud as it went in |
| `Mix` | Below `FF`, the dry sound blends in, for parallel compression |
| `Gain` | Makes up what it took, or trims `Auto`, on the level scale |

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
| `Freeze` | `On` stops recording, so the grains keep playing what's held |
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
| `Source` | `Osc`, a track by its number, or return `A` or `B` |
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
| `Source` | `Self`, a track by its number, or return `A` or `B` |

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
at `C-4` and set `Tune` from steps, as `1TUNE`. `Material` is in tune
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
and to keep a sample loop at its own speed, play it at `C-4` and set `Tune`
from steps. Below `80`, `Feedback` rings on the note's odd harmonics alone,
hollower, like a clarinet, but at the same pitch. At `FF` with `Tone` up
it rings for many seconds. As a `Comb`, `Feedback` `80` is only the sound;
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

Every setting in a slot can be moved: an LFO's or envelope's `Dest` lists them while the
slot holds something, like `1 Amount`, and a step's FX slot can set them too. The bars
follow what's playing. A setting that's whole values, like `Bits`, just steps.

## Output

`Pan`, `Send A` and `Send B` are where those values start when
playback starts from the top. A step's `h` value changes them until the next start,
and changing one here is heard straight away unless a lock or glide is on top. `Level` scales the whole track.
Dimmed `Swing` is the project's, which this track follows. Changing it gives the
track its own, and `x` follows the project again.

## Returns and the master

Every song ends with three fixed tracks, labelled `A`, `B` and `M`. `W`, `B` and the
track list go through them, and `:track add` and `:track del` work before them, so they
always stay last.

`A` and `B` are returns, as in Ableton: what a track's `Send A` and `Send B` send plays
through their slots, and comes out alongside the track. A new song has a `Delay` in
`A`'s first slot and a `Reverb` in `B`'s, each with `Mix` at `FF`, so you only hear
what comes back. You can change them, and with nothing in its slots a return just
passes on what it's sent. `M` is the master: everything goes through it, and then the
limiter.

They make no sound of their own, so they have no machine or filter, and their Sound
page has just `Level` and `Pan`, and the master only `Level`. The rest works as on any
track: 3 slots on the FX page, the Mod page, a strip on the mixer, and phrases. To give
one a phrase, type a chain number in its column on the Song view. A note on one plays
nothing, but it does start the track's envelopes. Steps on a return only reach that
return, so `VOL` on `B`'s phrase rides the reverb.

Soloing a track keeps the returns, so you hear what it sends. Soloing a return plays
only what comes back from it. Muting a return silences what comes back, and the master
can't be muted or soloed.

## Envelopes

Each track has two, which start with each note. The page draws their shapes.
Time is stretched in the drawing, so a short attack still shows, and a stage's
width depends only on its own time.

| Setting | Value |
|---|---|
| Dest | `Vol` `Cut` `Res` `Pan` `SendA` `SendB` `Pitch`, or `---` for off |
| Attack, Decay | Milliseconds, up to 8000 |
| Sustain | The percent it holds at while the note lasts |
| Release | Milliseconds, after the note ends |
| Depth | How far full moves it, in hex. Negative flips it |
| Reset | `on` starts from zero on each note. `off` goes on from there |

`Env 1` starts on `Vol` and shapes each note, so it has no depth. `Env 2` starts
on `Cut` with no depth, so raise its depth to open the filter on each note. On
`Pitch`, depth is in semitones and in hex, so `+0C` is an octave.

A note lasts until the next note on its track or an `OFF`, and the next note
fades the last one quickly rather than running its release. So a drum is often
a Decay with Sustain at `0%`, and Release only runs on an `OFF`. To make notes
shorter, use `OFF` on a step or the `OFF` command.

Decays and releases fall fast and then slow, as an analog envelope does, and an
attack starts slowly.

## LFOs

Each track has two. An LFO moves around the current value, so `CUT40` on a
step moves the centre and the LFO carries on around it.

| Setting | Value |
|---|---|
| Dest | `Cut` `Res` `Pan` `Vol` `SendA` `SendB` `Pitch`, or `---` for off |
| Shape | `Sine` `Tri` `Saw` `Ramp` `Square` `Random` `Smooth` |
| Rate | The length of one cycle. `=` and `+` are always faster |
| Depth | How far it swings each way, in hex. Negative flips it |
| Phase | Where in the cycle it starts, in degrees |
| Retrig | `on` restarts it with each note on this track |

With `Cut 80` and `Depth +40`, the filter moves between `40` and `C0`. `Saw`
falls and `Ramp` rises. `Random` picks a new value each cycle, and `Smooth` eases
from one to the next, which suits a slow drift on `Cut`.

| Type this as a rate | For |
|---|---|
| `1/4` `1/8d` `1/16t` | A note division, dotted or triplet |
| `2 bars` | Whole bars |
| `7st` | A number of steps, to slide against the bar |
| `2.5hz` | Free of the tempo, from `0.01hz` to `100hz` |

A large step is `10` in hex, 5% of swing, or 90° of phase. On a rate it skips
dotted and triplet divisions, moves 4 steps, or doubles or halves the hertz.

`i` on Dest, Shape or Retrig opens the list too. `K` on Rate lists the note
divisions and bars, and `i` is for anything else, like `7st` or `2.5hz`.

## When an LFO restarts

Playing from the top restarts every LFO. A `:reset` doesn't, since that is
about patterns. To follow the steps, turn on `Retrig`. While stopped, a track
sits at its held values, so `;` hears those. Pausing freezes the LFO.

`Vol`, `SendA` and `SendB` swing in dB, on the level scale in **Phrases and
chains**, so `Depth +40` on `Vol` is 16 dB each way. They stop at `F8`, which
is 6 dB above where `VOL` starts, so a deep swing stays even if you lower `VOL`
on a step.

## The mixer

Press `Space 6` for a strip for each track: its name, a meter of what it's playing
with `L` and `R` bars, a fader beside them showing its `Level`, then its `Pan`,
`Send A`, `Send B`, mute and solo. The meters are on the same scale as the main one,
with `0` marked, and show the track after its level and pan. `h` and `l` move between
tracks, `j` and `k` up and down the strip, and values edit as they do on the track page. On mute and solo, `=` switches on,
`-` switches off, and `K` or `i` changes it. `/` jumps to a setting by its name, as
it does on the track page, staying on the cursor's track.
