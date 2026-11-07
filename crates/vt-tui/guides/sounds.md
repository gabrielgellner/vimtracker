# Sounds, samples and kits

A track makes one sound at a time, from its machine: a sample, or a synth. Everything
else about its sound is on the track page, `Space 4`: see **The track page, envelopes
and LFOs**.

## The synths

`K` on Machine, at the top of the Sound page, chooses `Plaits`, and `K` on Voice below
it chooses a synth. They come from Mutable Instruments' Plaits, and each has three
settings, named on the page.

`VA` is a square and a saw, for bass and leads. Try a note around `C-2`, and bring
`Cut` down.

| Setting | Does |
|---|---|
| `Harm` | Detunes the saw, up to two octaves either way. `80` is in tune |
| `Timbre` | The square: none at `00`, then a pulse, a square and hard sync |
| `Morph` | The saw: a triangle at `00`, a plain saw at `80`, then a notch |

`FM` is two sine waves, one bending the other, for bass, bells and keys.

| Setting | Does |
|---|---|
| `Ratio` | The second wave's pitch, in steps that catch: from 1/2 to 8 times |
| `Index` | How hard it bends: a pure sine at `00`, and brighter going up |
| `Feedback` | None at `80`, gritty above, and unstable pitch below |

`Fold` is a wave folded back on itself into harmonics, for bass and leads with a
west-coast edge. Try a note around `C-2`.

| Setting | Does |
|---|---|
| `Shape` | The waveshaper before the fold, plain at `80` |
| `Fold` | How far it's folded: smooth at `00`, and rich going up |
| `Slope` | The wave's tilt, from a triangle at `00` to a ramp |

`Waves` sweeps through a grid of 8 by 8 waves, in four banks: soft and organ-like, then
vocal, then the harsher waves of older digital synths, then all of them shuffled. Below
`80`, `Bank` blends between banks and waves; above it, it steps from one to the next.

| Setting | Does |
|---|---|
| `Bank` | Up through the banks, then down. Above `80`, it steps |
| `Wave` | Which wave along a row of the grid |
| `Row` | Which row of the grid |

`6-op` is six-operator FM playing 96 DX7 patches, from basses and electric pianos to
bells, organs, pads and brass. Each patch has envelopes of its own, which the track's
first envelope shapes again, so give it a long Release to hear a patch's tail. A new
track's `Bright` starts at `00`, which is dark, so set it to `80` to hear a patch as it was
made. A new patch takes over on the next note.

| Setting | Does |
|---|---|
| `Patch` | Which patch, `00` to `5F`, named beside it. `K` lists them by name |
| `Bright` | How hard the modulators bend the sound: as the patch has it at `80` |
| `Time` | Attack and decay: as patched at `80`, faster below, slower above |

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
| `Res` | How much the filter rings at the note |

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

The three are set from steps with `HAR`, `TIM` and `MOR`, in that order on any synth,
and LFOs and envelopes can move them as `Har`, `Tim` and `Mor`. A `VA`, `FM`, `Fold`, `Waves`, `6-op`, `Dust` or `Noise` note holds
until the next note, `OFF` or `KIL`, shaped by the first envelope, so shorten its
Sustain for plucks. The drums, `String` and `Modal` die away by themselves. A track plays one note at a time.

## Accent

`Accent`, below the three settings, is how hard a synth is struck, as Plaits has it. On
`Kick`, `Snare` and `Hat` a harder strike is punchier and louder. On `String` and `Modal`
it's brighter and rings longer, and on `6-op` it's the patch's own response to how hard a
key is played. The other voices ignore it, and it's dimmed on them.

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
