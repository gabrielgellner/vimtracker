# Instruments and samples

Press `Space 5` for the instrument view. An instrument is a sample with a
volume and an envelope. It is created the first time you set something on
it.

| Keys | Does |
|---|---|
| `h` `l` | Previous or next instrument |
| `j` `k` | Next or previous row |
| `;` | Audition: hear this instrument at C-4 |
| `=` `-` | Change the value by a small step |
| `+` `_` | Change the value by a large step |
| `x` | Back to its default. On Sample, no sample |
| `i` `K` | On Sample, list samples. On other rows, `i` types a value |

| Row | Value |
|---|---|
| Sample | A WAV file |
| Volume | Two hex digits. `E0` is 0 dB, as in **Phrases and chains** |
| Attack | Milliseconds from 0 to 5000, then `Enter` |
| Release | Milliseconds from 0 to 5000, then `Enter` |

## Finding samples

Pressing `i` on the Sample row lists WAV files in the project's `samples`
folder and under the folder you ran from. Type to filter, and press `Enter`
to choose one.

| Command | Does |
|---|---|
| `:sample path/kick.wav` | Load one file |
| `:sample path/drums` | Browse a folder |

The browser looks 4 folders deep and lists up to 2000 files.

Samples stay where they are, and the project refers to them, so moving a
file breaks the projects that use it. To gather every sample into one
folder, ready to move or share, save a copy with `:w path`.

## Labels

A label is a two-character name for a slot, such as `BD` or `CH`, so you don't
have to remember numbers. A label is the slot: `BD` is slot `BD`, and `CH` has
a slot of its own. So `CH` means the same thing in every kit and song, and a
pattern keeps working when you load another kit with the same labels.

Labels start with a letter, and show in their own colour in the phrase and
the instrument view. A number stays a number, so a `BD` nobody labelled looks
plain.

| Command | Does |
|---|---|
| `:inst ch` | Make that slot current, then `:sample` fills it |
| `:label BD` | Mark slot `BD` as labelled, and make it current |
| `:label off` | Stop treating this numbered slot as labelled |

## Kits

A kit fills slots in one go. It is a file in `samples/kits`, such as `909.ron`:

```
(
    sounds: [
        (label: "BD", path: "../drums/kick.wav"),
        (label: "CH", path: "../drums/hat.wav"),
        (path: "../drums/fx.wav"),
    ],
)
```

A sound with a label goes in that slot, and the others fill upward from `00`.
Paths are relative to the kit file, or absolute, or start with `~/`.

| Command | Does |
|---|---|
| `:kit` | Choose a kit from `samples/kits` |
| `:kit 909` | Load that kit |
| `:kit path/drums.ron` | Load a kit file anywhere |

A kit keeps a slot's other settings, and leaves slots it doesn't name alone.
If it replaces a different sample, the status line says which, and `u` undoes
the whole load.

## Using instruments in a phrase

Notes use the current instrument, shown as `Inst` on the status line. It is
always the last one you entered, however you entered it.

| On the Instrument column | Does |
|---|---|
| `K` | Choose from a list, labels first. Type to filter |
| `i`, then `bd` or `04` | Type a label or number |
| `=` `-` | Change it. An empty cell gets `Inst` |
| `gd` | Open it in the instrument view, and `C-o` comes back |

From anywhere, `Space i K` chooses the current instrument from the same list.
It writes nothing to the step, so the next note you type uses it.
