# Sounds, samples and kits

A track plays one sample. Everything else about its sound is on the track page,
`Space 4`: see **The track page, envelopes and LFOs**.

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

Sounds go on tracks from the first, in order, and a label becomes the track's
name. Paths are relative to the kit file, or absolute, or start with `~/`.

| Command | Does |
|---|---|
| `:kit` | Choose a kit from `samples/kits` |
| `:kit 909` | Load that kit |
| `:kit path/drums.ron` | Load a kit file anywhere |

A kit keeps a track's other settings, leaves later tracks alone, and adds tracks
if it has more sounds than you have. If it replaces a different sample, the
status line says which, and `u` undoes the whole load.
