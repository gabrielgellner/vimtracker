# vimtracker

An experimental music tracker for the terminal, written in Rust and edited with a
keyboard grammar modelled on Neovim. It takes ideas from the Polyend Tracker, the
Dirtywave M8, Elektron sequencers and Jeskola Buzz, but it is a separate program and is
not affiliated with any of them.

It is early, and the design and implementation plan are in
[docs/designs/vimtracker.md](docs/designs/vimtracker.md).

## Build and run

```sh
cargo build --release
cargo run --release -p vt-tui --bin vimtracker
```

Run the tests with `cargo test --workspace`.

## Quick start

Use the release build for playing: the debug build uses far more of the audio deadline.

```sh
vimtracker my-song        # opens my-song, or creates it on the first :w
```

1.  `Space 5` opens the instrument view. `i` on the Sample row lists WAV files under the
    project and the folder you ran from. `:sample path` takes a file, or a folder to browse.
2.  `Space 3` opens the phrase view. `i` starts Insert, and the keyboard plays like a piano,
    `z` to `m` and `q` to `u`, with `[` and `]` changing octave. `Esc` leaves Insert.
3.  `Enter` plays from the cursor and stops. `Space x` pauses and `Space c` returns to the top.
4.  `l` moves to the next field. Type an FX name and two hex digits, such as `CUT40` or `RET14`.
5.  `Space 2` opens the chain view, where `gd` opens a phrase, or adds one on the empty row.
6.  `:w` saves. `:render loop.wav` writes the loop, ready to drop into Ableton.

`Space Space` jumps between views, and `Space ?` lists every key. If you pause after `Space`
or another prefix, a popup shows what can come next.

No samples come with the repo. Put your own WAV files in `samples/`, which git ignores, and run
from the repo root to see them in the browser. Choosing one copies it into the project's own
`samples/` folder, so a saved project keeps working without the original.

| Command | What it does |
|---|---|
| `:bpm 137.63` | Tempo |
| `:swing 66.7`, `:tswing 60`, `:tswing off` | Project swing, this track's swing, follow the project |
| `:inst 1A`, `:chain 2` | Current instrument, and which chain this track plays |
| `:len 32`, `:double` | Phrase length, and repeat the phrase to double it |
| `:track add 4`, `:track del` | Add tracks, or remove the last |
| `:set step=2`, `:seed 7` | How far Insert moves, and the random seed for `CHA` |
| `:render path [--rows n] [--tail]` | Render to a WAV file |

`vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` renders
without a terminal or sound device. `vimtracker probe-keys` shows what your terminal sends.

Unsaved work is written to `song.autosave.ron` every minute once the project has a folder,
and is offered back the next time you open it after a crash.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
