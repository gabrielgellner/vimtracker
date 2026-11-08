# Sounds, samples and kits

A track makes one sound at a time, from its machine: a sample, or a synth. Everything
else about its sound is on the track page, `Space 4`: see **The track page, envelopes
and LFOs**.

## The synths

`K` on Machine, at the top of the Sound page, chooses `Sample` or one of the synths, by
typing its name. They come from Mutable Instruments' Plaits, and each has its own
settings, named on the page below Machine. A track keeps each synth's settings, so
trying another and coming back finds the first as you left it. One you haven't played on
the track starts from its own defaults.

`VA` is a square and a saw, for bass and leads. Try a note around `C-2`, and bring
`Cut` down.

| Setting | Does |
|---|---|
| `Detune` | Detunes the saw, up to two octaves either way. `80` is in tune |
| `Square` | The square: none at `00`, then a pulse, a square and hard sync |
| `Saw` | The saw: a triangle at `00`, a plain saw at `80`, then a notch |

`VA VCF` is a saw or a square, with a sub an octave down, through a resonant filter, for
acid lines and fat bass. Try a note around `C-2`.

| Setting | Does |
|---|---|
| `Resonance` | Steep below `80`, gentle above, ringing more at either end |
| `Frequency` | The cutoff, from two octaves under the note to far above |
| `Wave` | Saw and sub, saw, square, pulse, then square and sub at `FF` |

`Acid` is our own take on Roland's TB-303, rather than one of Plaits': a saw or a square
through a diode ladder filter, with the 303's accent and slide, for acid lines. It plays one
note at a time, and each note carries on from the last one's filter, as a 303 does. Try
notes around `C-2`, with `Resonance` well up.

- **Accent** comes from velocity. A note at `70` or above is accented: louder, with more
  filter envelope and a shorter decay. Notes are written at `64`, so one step up the
  velocity ladder, to `74`, accents a note, and so does `7F`. Accents close together build
  up, swelling higher each time, the more so with `Resonance` up.
- **Slide** is the `HLD` gate. A note held into the next row's glides to its pitch, with
  neither envelope starting again. `Slide` sets how long it takes.
- A note holds until the next one, or an `OFF`. For the 303's short, bouncing notes, end
  some early with `OFF`.
- From the keyboard, holding a second key slides to its note, and letting go of it slides
  back to the first, rather than playing a chord.

| Setting | Does |
|---|---|
| `Wave` | The saw, or the square a 303 makes from it |
| `Cutoff` | Where the filter sits, from 314 Hz to 2.4 kHz, as on a 303 |
| `Resonance` | How much the filter rings. It thins the bass as it rises |
| `Env mod` | How far each note's envelope sweeps the cutoff |
| `Decay` | How long that sweep takes to fall, from 200 ms to 2 s |
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
west-coast edge. Try a note around `C-2`.

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
away by itself. Try a note around `C-3`.

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

`Kick` is an 808-style bass drum. Its note tunes it, so try `C-2`, or lower.

| Setting | Does |
|---|---|
| `Punch` | The attack's pitch sweep, then its self-modulation and drive |
| `Tone` | How bright it is |
| `Decay` | How long it rings |

`Snare` is an 808-style snare drum. Try `G-3`, or wherever it sits best.

| Setting | Does |
|---|---|
| `Snappy` | The mix of drum and noise: all drum at `00` |
| `Tone` | The drum's overtones, with more of them near the top |
| `Decay` | How long it rings |

`Hat` is an 808-style hi-hat, of six square waves and noise. Try `C-5` for a closed
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

Press `Space 5` to list WAV files in the project's `samples` folder and under
the folder you ran from. Type to filter, and press `Enter` to put one on the
current track.

| Command | Does |
|---|---|
| `:sample path/kick.wav` | Put one file on this track |
| `:sample path/drums` | Browse a folder |

The browser looks 4 folders deep and lists up to 2000 files.

Samples stay where they are, and the project refers to them, so moving a
file breaks the projects that use it. To gather every sample into one
folder, ready to move or share, save a copy with `:w path`.

Press `;` in a phrase to hear the row. Notes you play while editing and
auditions go through this track's filter, levels and sends.

On the Sound page, `Space 4`, `K` on File chooses a sample too, and `x` clears it.

## Names

`:name BD` names the current track, in place of its number. A name is two
characters and starts with a letter. `:name` on its own clears it.

## Kits

A kit fills tracks in one go. It is a file in `samples/kits`, such as `909.ron`:

```
(
    sounds: [
        (label: "BD", path: "../drums/kick.wav"),
        (label: "CH", path: "../drums/hat.wav"),
        (path: "../drums/fx.wav"),
    ],
)
```

A kit is a list of sounds to choose from, so it adds no tracks of its own. Use one
with `:kit 909`, then `Space 5` on a track lists its sounds first. Choosing one puts
it on this track, and its label becomes the track's name. Paths are relative to the
kit file, or absolute, or start with `~/`.

| Command | Does |
|---|---|
| `:kit` | Choose a kit from `samples/kits` |
| `:kit 909` | Choose sounds from that kit |
| `:kit path/drums.ron` | Choose sounds from a kit file anywhere |
| `:kit 909 all` | Put every sound on tracks from the first, in order |

`all` keeps a track's other settings, leaves later tracks alone, and adds tracks
if it has more sounds than you have. If it replaces a different sample, the
status line says which, and `u` undoes the whole load.
