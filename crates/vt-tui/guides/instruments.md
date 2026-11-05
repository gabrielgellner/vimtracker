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

The browser looks 4 folders deep and lists up to 2000 files. Choosing a
sample copies it into the project, so the project keeps working without the
original.

## Using instruments in a phrase

Notes you write use the current instrument, which is the one last shown
here. In the phrase view, `gd` on the Instrument column opens that
instrument, and `C-o` comes back.
