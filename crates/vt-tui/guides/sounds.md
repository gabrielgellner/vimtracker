# Sounds, samples and kits

A track makes one sound at a time, from its machine: a sample, or a synth. Everything
else about its sound is on the track page, `Space 4`: see **The track page, envelopes
and LFOs**.

## The synths

`K` on Machine, at the top of the Sound page, chooses `Sampler` or one of the synths, by
typing its name. They come from Mutable Instruments' Plaits, and each has its own
settings, named on the page below Machine. A track keeps each synth's settings, so
trying another and coming back finds the first as you left it. One you haven't played on
the track starts from its own defaults.

`VA` is a square and a saw, for bass and leads. Try a note around `C-1`, and bring
`Cut` down.

| Setting | Does |
|---|---|
| `Detune` | Detunes the saw, up to two octaves either way. `80` is in tune |
| `Square` | The square: none at `00`, then a pulse, a square and hard sync |
| `Saw` | The saw: a triangle at `00`, a plain saw at `80`, then a notch |

`VA VCF` is a saw or a square, with a sub an octave down, through a resonant filter, for
acid lines and fat bass. Try a note around `C-1`.

| Setting | Does |
|---|---|
| `Resonance` | Steep below `80`, gentle above, ringing more at either end |
| `Frequency` | The cutoff, from two octaves under the note to far above |
| `Wave` | Saw and sub, saw, square, pulse, then square and sub at `FF` |

`Acid` is our own take on Roland's TB-303, rather than one of Plaits': a saw or a square
through a diode ladder filter, with the 303's accent and slide, for acid lines. It plays one
note at a time, and each note carries on from the last one's filter, as a 303 does. Try
notes around `C-1`, with `Resonance` well up.

- **Accent** comes from velocity, and it's all velocity does here, as a 303 has no
  velocity. A note at `70` or above is accented: louder, with more filter envelope and a
  shorter decay. Below `70`, every note sounds the same, whether it's at `10` or `6F`, and
  every accent sounds the same from `70` to `7F`. Notes are written at `64`, so one step up
  the velocity ladder, to `74`, accents a note, and so does `7F`. `Accent` sets how much:
  hardly any louder at `00`, about 4.5 dB at noon and about 10 dB at `FF`, brightening
  mostly over its top half. Accents a sixteenth apart build up, swelling higher each time,
  the more so with `Resonance` up, while accents with a rest between them hardly do.
- **Retriggers.** `RET` repeats a note at its own level, plain or accented, as a 303 has
  no other: its fade and swell, the first digit, do nothing here, and every hit of an
  accented note is accented.
- **Slide** is the `HLD` gate, on the row that slides away, as a 303's slide is on the
  step it leaves. A note held into the next row's glides to its pitch, with neither
  envelope starting again. `Slide` sets how long it takes, unless the track's `Glide`
  is on, which then sets it, as on every other voice.
- **Gate.** Its notes are short and bouncy, as a 303's are: its `Gate` starts at `50%`,
  so each note's gate closes halfway through its step and it falls silent about 16 ms
  later. A slide holds its gate on into the note it slides to, and an `SGL2` is a note
  tied over two steps. Turn `Gate` up to `100%`, on the track page, for notes that hold
  until the next, or give one note a length of its own in its gate cell, like `SGL1 FF`.
- From the keyboard, holding a second key slides to its note, and letting go of it slides
  back to the first, rather than playing a chord. These take `Slide`'s time.

| Setting | Does |
|---|---|
| `Wave` | The saw, or the square a 303 makes from it |
| `Cutoff` | Where the filter sits, 170 Hz to 1.45 kHz, moving most up top |
| `Resonance` | How much the filter rings. It thins the bass as it rises |
| `Env mod` | How far each note's envelope sweeps the cutoff |
| `Decay` | How long that sweep takes to fall, from 40 ms to 600 ms |
| `Accent` | How much louder and brighter an accented note is |
| `Slide` | How long a slide takes: 60 ms at `00`, as on a 303, to 360 ms |
| `Drive` | How hard the saw drives the filter, up to 24 dB, for grit |

The track's own filter comes after it, as for any voice. It's best left open, with `Cut`
at `FF`, so the diode ladder does the work.

`FM` is two sine waves, one bending the other, for bass, bells and keys.

| Setting | Does |
|---|---|
| `Ratio` | The second wave's pitch, in steps that catch: from 1/2 to 8 times |
| `Index` | How hard it bends: a pure sine at `00`, and brighter going up |
| `Feedback` | None at `80`, gritty above, and unstable pitch below |

`PD` is phase distortion, as on Casio's CZ synths: a sine whose phase is bent by a second
wave, for bass, keys and resonant sweeps.

| Setting | Does |
|---|---|
| `Ratio` | The bending wave's pitch, catching on ratios. An octave at `80` |
| `Amount` | How far it's bent: a pure sine at `00`, and brighter going up |
| `Skew` | Which way it's bent, for an edge like a saw or a pulse |

`Fold` is a wave folded back on itself into harmonics, for bass and leads with a
west-coast edge. Try a note around `C-1`.

| Setting | Does |
|---|---|
| `Shape` | The waveshaper before the fold, plain at `80` |
| `Fold` | How far it's folded: smooth at `00`, and rich going up |
| `Slope` | The wave's tilt, from a triangle at `00` to a ramp |

`Formant` is two formants made of grains, like a voice's, for vowels and nasal leads.
Move `Formant` slowly for a talking sweep. Below `80`, `Interval` also brings in a bare
pulse.

| Setting | Does |
|---|---|
| `Interval` | The second formant, up to two octaves from the first |
| `Formant` | Where the formants sit, from low and muffled to bright |
| `Width` | How wide and soft the formants are |

`Additive` is 24 harmonics, set in bumps along the spectrum, for organs and glassy, bright
tones.

| Setting | Does |
|---|---|
| `Bumps` | How many bumps: one at `00`, and more going up |
| `Peak` | Which harmonic is loudest, from the fundamental up |
| `Shape` | The bumps' shape, from wide and flat to narrow and peaked |

`Waves` sweeps through a grid of 8 by 8 waves, in four banks: soft and organ-like, then
vocal, then the harsher waves of older digital synths, then all of them shuffled. Below
`80`, `Bank` blends between banks and waves; above it, it steps from one to the next.

| Setting | Does |
|---|---|
| `Bank` | Up through the banks, then down. Above `80`, it steps |
| `Wave` | Which wave along a row of the grid |
| `Row` | Which row of the grid |

`Terrain` traces a circle over a surface and plays the height it passes, for tones that
shift and evolve. The first five surfaces are worked out from formulas, and the last
three are made of the waves `Waves` uses.

| Setting | Does |
|---|---|
| `Terrain` | Which surface, blending from one to the next |
| `Size` | How wide the circle is: small and pure at `00`, rich at `FF` |
| `Offset` | Where the circle sits on the surface |

`6-op` is six-operator FM playing 96 DX7 patches, from basses and electric pianos to
bells, organs, pads and brass. Each patch has envelopes of its own, which the track's
first envelope shapes again, so give it a long Release to hear a patch's tail. It starts
on `E.PIANO 1`, with `Bright` and `Time` at `80`, as each patch was made. A new patch
takes over on the next note.

| Setting | Does |
|---|---|
| `Patch` | Which patch, `00` to `5F`, named beside it. `K` lists them by name |
| `Bright` | How hard the modulators bend the sound: as the patch has it at `80` |
| `Time` | Attack and decay: as patched at `80`, faster below, slower above |

`Chord` plays a chord from one note, on an organ or a string machine below `80` on
`Wave`, and on waves above it. `Chord` is a list: `=` steps through it, and `K` lists
them by name.

| Setting | Does |
|---|---|
| `Chord` | Octave, 5, sus4, m, m7, m9, m11, 6/9, M9, M7, then M at `0A` |
| `Inversion` | Moves the lowest note up an octave at a time. At pitch at `66` |
| `Wave` | Organ and string registrations below `80`, then waves above it |

`Chip` is a chord of square waves, as on old game consoles. It plays the chord's notes
together, as the rows do the arpeggio.

| Setting | Does |
|---|---|
| `Chord` | The same chords as `Chord` |
| `Inversion` | Moves the lowest note up an octave at a time. At pitch at `66` |
| `Shape` | A thin pulse below `80`, a square at `80`, and hard sync above |

`Swarm` is eight saws, each in grains that start and stop, for pads, drones and clouds.
Each note starts a fresh burst of grains.

| Setting | Does |
|---|---|
| `Spread` | How far the saws' pitches spread, up to four octaves |
| `Density` | How often grains start: sparse at `00`, a cloud at `FF` |
| `Overlap` | How long grains are. Near `FF`, held tones that glide |

`String` is a plucked string: a burst of noise ringing in a modelled string, which dies
away by itself. Try a note around `C-2`.

| Setting | Does |
|---|---|
| `Material` | A sitar's buzz at `00`, plain at `40`, more metal going up |
| `Bright` | How bright the pluck is |
| `Decay` | How long it rings. Near `FF` it rings until the next note |

`Modal` is a struck resonator, for bells, bars, plates and woodblocks. It dies away by
itself.

| Setting | Does |
|---|---|
| `Material` | Partials bunched low, in tune at `40`, spread like a bell high |
| `Bright` | How bright the strike is, and how long the high partials last |
| `Decay` | How long it rings |

`Dust` is random clicks through filters tuned around the note, for crackle, rain and
droplets. It sounds until the note ends, shaped by the first envelope.

| Setting | Does |
|---|---|
| `Spread` | How far each click's pitch strays from the note |
| `Density` | How many clicks: a few at `00`, a hiss at `FF` |
| `Filter` | Below `80`, more and more echoing; above, more and more ringing |

`Noise` is noise, held for a clock's cycle, through a resonant filter at the note. It
sounds until the note ends, shaped by the first envelope.

| Setting | Does |
|---|---|
| `Type` | The filter: low-pass at `00`, band-pass at `80`, high-pass at `FF` |
| `Clock` | How fast the noise changes: grainy steps low, a hiss high |
| `Ring` | How much the filter rings at the note |

`Kick` is an 808-style bass drum. Its note tunes it, so try `C-1`, or lower.

| Setting | Does |
|---|---|
| `Punch` | The attack's pitch sweep, then its self-modulation and drive |
| `Tone` | How bright it is |
| `Decay` | How long it rings |

`Snare` is an 808-style snare drum. Try `G-2`, or wherever it sits best.

| Setting | Does |
|---|---|
| `Snappy` | The mix of drum and noise: all drum at `00` |
| `Tone` | The drum's overtones, with more of them near the top |
| `Decay` | How long it rings |

`Hat` is an 808-style hi-hat, of six square waves and noise. Try `C-4` for a closed
hat, and raise `Decay` for an open one.

| Setting | Does |
|---|---|
| `Noise` | How much plain noise replaces the metal |
| `Tone` | How bright it is |
| `Decay` | How long it rings |

The first three are set from steps with `HAR`, `TIM` and `MOR`, in that order on any
synth, and LFOs and envelopes can move them as `Har`, `Tim` and `Mor`. A `VA`, `FM`, `Fold`, `Waves`, `6-op`, `Dust` or `Noise` note holds
until the next note, `OFF` or `KIL`, shaped by the first envelope, so shorten its
Sustain for plucks. The drums, `String` and `Modal` die away by themselves. A track plays one note at a time.

## Accent

`Accent`, the fourth setting of the voices that have it, is how hard a synth is struck,
as Plaits has it. On
`Kick`, `Snare` and `Hat` a harder strike is punchier and louder. On `String` and `Modal`
it's brighter and rings longer, and on `6-op` it's the patch's own response to how hard a
key is played. The other voices don't have it.

It starts at `CC`, Plaits' own strength, and it's apart from velocity: velocity is only
how loud a note is, so a quiet note can still be struck hard. `ACC` sets it from a step,
and LFOs and envelopes can move it as `Accent`. Set it high on the track page and every
note on the track rings out, or lock it on a step for one hard hit.

## Choosing a sample

Press `Space 5`, or type `:sample`, to choose a WAV, AIFF or FLAC file from a tree of
folders: the last few you chose from, the project's folder, your kit folders, your
sample folders, Ableton Live's sample library if it's installed, where you ran the
program from and your home folder. It opens on the track's own sample.
`l` opens a folder, `h` closes it, and `l` or `Enter` on a file puts it in the project's
bank and has the current track play it. **Navigation** has the rest of the keys.

A kit opens like a folder, named by the kit, to list its sounds by label and name,
so `kits`, `Ableton`, `Drum Machines`, `909 Core Kit`, `BD` reaches a kick. Choosing
one is the same as choosing it from the kit in use, below. A sound with variants or
layers has stacked pads for its icon: `Enter` chooses the sound itself, and `l` opens
it to list its own file, its variants and its velocity layers, softest first, any of
which can be chosen in its place. A sound whose file has gone is still listed, with a
warning for its icon, and its preview says why.
`library` is the `Samples` folder of the newest Live in `/Applications`, with its
`Loops`, `One Shots` and `Multisamples`, which the imported kits play from.

Moving onto a sample plays it through the current track, as an Elektron's or the M8's
browser does, and `A` stops that or starts it again. Beside the list is its waveform,
how long it is, its rate and whether it's mono or stereo. A long file is read in the
background, so the list doesn't wait for it, and the last few are kept.

| Command | Does |
|---|---|
| `:sample start 0.25` | Start 0.25 s in, or alone, at the start |
| `:sample end 1.5` | End there, or alone, at the file's end |
| `:sample loop 0.5 1 20ms` | Loop 0.5 s to 1 s, crossfading 20 ms |
| `:sample loop off` | No loop of its own |
| `:sample reverse` | Backwards, or forwards again: or `on`, `off` |
| `:sample fade 10ms 0.2` | Fade in and out over these. `off` for none |
| `:sample gain -3` | Its gain in dB, up to 24 either way |
| `:sample tune 12 -20` | Its tune, in semitones and cents |

Wherever a start or an end cuts into the sound, it fades over a millisecond or so, so
it never clicks. A loop with no crossfade of its own gets one as short. The track's own
settings cut the sample further, in the Sound page's Machine, and a step can lock
any of them:

| Setting | Step | Does |
|---|---|---|
| `Slice` | `SLI` | Which slice plays, once it's sliced: see **Slices** |
| `Start` | `STA` | Where a note starts: `80` is half way in |
| `Length` | `SLEN` | How much plays from there. `FF` is all of it |
| `Play` | `PLY` | `One-shot`, round its `Loop`, `Ping-pong`, or `Thru` |

`Loop` and `Ping-pong` go round the sample's own loop, or round all that plays if it
has none, until the note ends. `Thru` plays once from the slice on to the sample's
end. `Start` and `Length` are shares of the slice, or of the whole, counted
from where the sample starts playing, so on a reversed sample they count back from
its end. An LFO or an envelope can move them, read as each note starts, as an
Elektron's sample start is. A sample's fade out is for a one-shot, as a looping note
never reaches its end.

## Normalising and trimming

Neither writes audio: each changes how the track's sample plays, and `u` takes
it back. Both read the sample's analysis, which is worked out in the background
when a file joins the bank and kept in `~/Library/Caches/vimtracker`
(`$XDG_CACHE_HOME/vimtracker` if that's set), never beside your files and never
in the project, so a project opened again finds it there. While a long file is
being analysed, the right of the status line shows a spinner and `Analysing`.

| Command | Does |
|---|---|
| `:normalize -0.3` | Bring the sample's peak to -0.3 dBFS, as a gain |
| `:normalize` | The same, to 0 dBFS |
| `:normalize off` | Play it at its own level again |
| `:trim` | Start and end it where its sound does, past any silence |

Normalising scales both channels by the same gain, set by the louder, so a
stereo sample keeps its balance. The sample's own gain comes on top. `:trim`
keeps from the first frame above -60 dBFS to the last, setting the start and end
that `:sample start` and `:sample end` set, and leaves its loop where it is. The
track's `Start` and `Length` then count from what it kept.

## Slices

A sample can be cut into slices, numbered from `00` by where they start, as
Vim numbers lines. Each plays from its start to the next one's, and the last
to the sample's end. The track's `Slice`, in the Sound page's Machine, and
`SLI` on a step, chooses which one a note plays, so `SLI 00` to `0F` down a
phrase plays a loop cut in 16 back in order, and a lock plays another slice for
one note. Past the last slice it plays the last, and a sample with no slices
plays whole. An LFO or an envelope can sweep through them, a slice for each
step of depth.

| Command | Does |
|---|---|
| `:slice 16` | Cut it into 16 of the same length |
| `:slice clear` | Play it whole again |
| `:slice gain -3` | This slice's own gain, in dB |
| `:slice pan -25` | Its own pan, from -50 at the left to 50 at the right |
| `:slice tune 7` | Its own tune, in semitones |
| `:slice reverse on` | Play it backwards, or `off` forwards |
| `:slice fade 10ms 0.2` | Its own fades in and out |

Each of a slice's own settings on its own, as `:slice gain`, plays it as the
sample does again; one it doesn't have is the sample's. They change the slice
the sample page's cursor is in, or, elsewhere, the one the track's `Slice` is
on. Slicing again moves each step's `SLI` to the new slice holding the sound it
played, so a pattern keeps its chops, and `u` takes back the slices and the
steps together. `Play`'s `Thru` plays from the slice on past its end to the
sample's, as tracker-ce's Play Thru does.

`K` on `Slice`, or on a step's `SLI`, lists the slices with where each starts,
and `g d` on a step's `SLI` shows that slice on the sample page; `C-o` comes
back.

## Marks

A letter, `a` to `z`, can name a slice, as Vim's marks name lines, and a step
can lock a slice by its letter, `SLI 'a`, rather than its number. The letter
stays with its slice as slices before it come and go and its number changes,
so the step keeps playing the same chop. One letter names one slice, and a
slice can have several. The slice here is the one the sample page's cursor is
in, or elsewhere the one the track's `Slice` is on.

| Command | Does |
|---|---|
| `:mark a` | Name the slice here `a` |
| `:mark a 03` | Name slice `03` `a`, its number in hex as `SLI` shows it |
| `:marks` | List the letters in the order of their slices, to go to one |
| `:delmarks a-c` | Take letters off, as `a b`, `abc` or `a-c` |
| `:delmarks!` | Take every letter off |

Slicing again moves each letter to the new slice holding its slice's start, and
`:slice clear` takes them all off. Splitting a slice leaves its letters on the
part before, and joining two gives the one left the other's letters. Each is
one undo step with the slicing.

`K` on a step's `SLI` lists the letters first, as `'a  03  0.214 s`, to lock by,
and then the slices by number, each with its letters. A step's `SLI 'a` shows
as `'a`, and `=` and `-` move it to the next or previous letter. If no slice is
named `a`, the step keeps the letter and plays the track's own `Slice`, as if
it had no `SLI`; the status line says so. Two hex digits typed over it lock by
number again.

## The sample page

`Space 0` shows the current track's sample as a big waveform, to hear it, slice
it, name its slices and set its points by eye and by ear; `C-o` comes back.
**The sample editor** has its keys.

## Names

`:name BD` names the current track, in place of its number. A name is two
characters and starts with a letter. `:name` on its own clears it.

## Kits

A kit fills tracks in one go. It is a file in a kit folder, such as
`~/Music/vimtracker/kits/909.ron`:

```
(
    name: "909",
    source: "Ableton Core Library",
    category: "Drum Machines",
    sounds: [
        (label: "BD", name: "Kick 909", path: "../drums/kick.wav",
         variants: ["../drums/kick 2.wav"]),
        (label: "CH", name: "Closed Hat", path: "../drums/hat.wav",
         choke: Some(1)),
        (label: "OH", name: "Open Hat", path: "../drums/open.wav",
         choke: Some(1)),
        (path: "../drums/fx.wav", gain: -3.0, tune: 12),
    ],
)
```

A kit is a list of sounds to choose from, so it adds no tracks of its own. Use one
with `:kit 909`, or choose it from `Space 8`, then `Space 5` on a track lists its
sounds first, by label and name, under the kit's name. Choosing one puts it in the bank
and on this track, and its label becomes the track's name.
Paths are relative to the kit file, or absolute, or start with `~/`.

Only `sounds`, and each sound's `path`, are needed. A kit's `name` is its file's
name if it doesn't say, and a sound's is its file's. `source` and `category` say
where it came from and what kind it is. For each sound:

| Field | Is |
|---|---|
| `label` | The track's name, two characters, such as `BD` |
| `variants` | Other tones of the sound |
| `layers` | Velocity layers, soft to hard |
| `choke` | A choke group, `Some(1)` to `Some(16)`, as open and closed hats share |
| `gain` | In dB |
| `tune` | In semitones, up to 48 either way |

A sound joins the bank with its label, its name, its gain and its tune, which
it keeps on whatever track plays it, and its track takes its choke group,
whether `:kit 909 all` puts it there or `Space 5` chooses it. A sound already
in the bank, the same file played the same way, isn't added twice, while one
file tuned two ways, as a kit's toms can be, is two samples. A variant or a
layer can be chosen in its sound's place from `Space 5`, opening the kit's file
there; otherwise they don't yet change what you hear.

| Command | Does |
|---|---|
| `Space 8` or `:kit` | Choose a kit from the kit folders |
| `:kit 909` | Choose sounds from that kit |
| `:kit path/drums.ron` | Choose sounds from a kit file anywhere |
| `:kit 909 all` | Put every sound on tracks from the first, in order |

`all` puts each sound in the bank, keeps a track's other settings, leaves later
tracks alone, and adds tracks if it has more sounds than you have. If it replaces a different sample, the
status line says which, and `u` undoes the whole load.

## Kit and sample folders

Where kits and samples are found is set for this machine in
`~/.config/vimtracker/settings.ron`, made the first time the program runs:

```
(
    kits: ["~/Music/vimtracker/kits"],
    samples: ["~/Music/vimtracker/samples"],
)
```

Each is a list of folders, looked through in order, and a folder that isn't
there is passed over. `:kit 909` opens the first `909.ron` found. Changes are
read the next time the program starts.

`Space 8` opens the kit folders as a tree, with the first one open, and opens on
the kit in use. Beside the list are the sounds of the kit under the cursor, each
with its label, its name and its waveform. `Enter` uses the kit, as `:kit 909`
does, and `L` puts its sounds on tracks, as `:kit 909 all` does. `:kit` takes a
kit's path in its folder, such as `:kit Ableton/Drum Machines/909 Core Kit`.

## Kits from Ableton Live

If you have Ableton Live, run this in a terminal to make a kit of each of its
drum racks:

```
vimtracker kits import-ableton
```

It reads the newest Live in `/Applications` and writes into the first kit
folder, under `Ableton`, a folder for each kind of rack. The kits point at
Live's samples where they are, so nothing is copied. Each pad becomes a sound,
named and labelled from its name and its sample, with its choke group, its
level and tune, and the other tones and velocity layers it has. Pads that play
a synth are left out. Run it again after Live updates: it replaces the kits it
made, and leaves anything else alone. To change one of its kits, copy it out of
the `Ableton` folder first. A library and a folder to write to can be given:

```
vimtracker kits import-ableton "/path/to/Core Library" ~/kits/Live
```
