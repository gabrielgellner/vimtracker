# Instruments and samples

Press `Space 5` for the instrument view. An instrument is a sample with a
volume and an envelope. It is created the first time you set something on
it.

| Keys | Does |
|---|---|
| `h` `l` | Previous or next instrument |
| `j` `k` | Next or previous row |
| `p` | Hear this instrument at C-4 |
| `+` `-` | Change the value |
| `i` | On Sample, list samples. On other rows, type a value |

| Row | Value |
|---|---|
| Sample | A WAV file |
| Volume | Two hex digits |
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

## Kits

A kit fills the first instrument slots in one go. It is a file in
`samples/kits`, such as `909.ron`, listing sounds in slot order:

```
(
    sounds: [
        (path: "../drums/kick.wav"),
        (path: "../drums/snare.wav"),
    ],
)
```

Paths are relative to the kit file, or absolute, or start with `~/`.

| Command | Does |
|---|---|
| `:kit` | Choose a kit from `samples/kits` |
| `:kit 909` | Load that kit |
| `:kit path/drums.ron` | Load a kit file anywhere |

A kit replaces the samples in those slots and keeps their other settings,
and `u` undoes the whole load. Slots past the end of the kit stay as they
were.

## Using instruments in a phrase

Notes use the current instrument, shown as `Inst` on the status line. It is
always the last one you entered, however you entered it.

| On the Instrument column | Does |
|---|---|
| `+` `-` | Change it. An empty cell gets `Inst` |
| `i`, hex digits, `Enter` | Type one |
| `gd` | Open it in the instrument view, and `C-o` comes back |
