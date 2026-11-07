# vimtracker

An experimental music tracker for the terminal, written in Rust and edited with a
keyboard grammar modelled on Neovim. It takes ideas from the Polyend Tracker, the
Dirtywave M8, Elektron sequencers and Jeskola Buzz, but it is a separate program and is
not affiliated with any of them.

It is early, and the design and implementation plan are in
[docs/designs/vimtracker.md](docs/designs/vimtracker.md).

The manual is at <https://gabrielgellner.github.io/vimtracker/>, and its source is in
[docs/manual](docs/manual/src/SUMMARY.md). With [mdBook](https://rust-lang.github.io/mdBook/)
installed, `just docs-serve` opens a local copy in a browser.

How to work on vimtracker, and how releases are made, is in [CONTRIBUTING.md](CONTRIBUTING.md).

## Build and run

```sh
cargo build --release
cargo run --release -p vt-tui --bin vimtracker
```

With [just](https://github.com/casey/just) installed, `just run my-song` does the same for a
project, and `just test` runs the tests.

In kitty, small drawings such as envelopes are images. Inside tmux, this needs `set -g allow-passthrough on` in your config. Other terminals get Braille, as does `--no-graphics`.

Run the tests with `cargo test --workspace`.

## Quick start

Use the release build for playing: the debug build uses far more of the audio deadline.

```sh
vimtracker my-song        # opens my-song, or creates it on the first :w
```

1.  `Space 5` lists WAV files under the project and the folder you ran from, for the current
    track. `:sample path` takes a file, or a folder to browse.
2.  `Space 3` opens the phrase view. `i` starts Insert, and the keyboard plays as in Ableton:
    `a` to `;` are white keys, `w` to `p` black keys, `z` and `x` change octave, `c` and `v` set
    the velocity written, and the arrows move without writing. `Esc` leaves Insert.
3.  `Tab` plays and pauses. `Shift-Tab` plays from the start, and `Space c` from the cursor.
4.  `l` moves to the next field. Type an FX name and two hex digits, such as `CUT40` or `RET14`.
5.  `Space 2` opens the chain view, where `gd` opens a phrase, or adds one on the empty row.
6.  `Space 4` opens the track page: base filter and output values, two envelopes and two LFOs.
7.  `:w` saves. `:render loop.wav` writes the loop, ready to drop into Ableton.

`Space Space` jumps between views. `Space ?` opens the guides, short tutorials on each topic,
and `Space k` lists every key. If you pause after `Space` or another prefix, a popup shows what
can come next.

No samples come with the repo. Put your own WAV files in `samples/`, which git ignores, and run
from the repo root to see them in the browser. A file in `samples/kits/` listing sounds, such as
`909.ron`, is a kit: `:kit 909` puts its sounds on tracks from the first. Tracks can have
two-character names like `BD` and `CH`. See the sounds guide.

Samples stay where they are, and a project refers to them. `:w new-folder` saves a copy with every
sample gathered into it, ready to move or share.

| Command | What it does |
|---|---|
| `:bpm 137.63` | Tempo |
| `:swing 66.7`, `:tswing 60`, `:tswing off` | Project swing, this track's swing, follow the project |
| `:tvol C0` | This track's level, separate from each note's velocity |
| `:name BD`, `:name`, `:chain 2` | Name this track or clear its name, and which chain it plays |
| `:len 32`, `:double` | Phrase length, and repeat the phrase to double it |
| `:track add 4`, `:track del` | Add tracks, or remove the last |
| `:set step=2`, `:set skip=8`, `:seed 7` | How far a note moves, how far the left and right arrows jump, and the random seed for `CHA` |
| `:render path [--rows n] [--tail]` | Render to a WAV file |

`vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` renders
without a terminal or sound device. `vimtracker probe-keys` shows what your terminal sends.

Unsaved work is written to `song.autosave.ron` every minute once the project has a folder,
and is offered back the next time you open it after a crash.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
