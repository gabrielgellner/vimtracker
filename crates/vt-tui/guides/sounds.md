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
and LFOs and envelopes can move them as `Har`, `Tim` and `Mor`. A `VA`, `FM` or `Fold` note holds
until the next note, `OFF` or `KIL`, shaped by the first envelope, so shorten its
Sustain for plucks. The drums die away by themselves. A track plays one note at a time.

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
