# vimtracker

[![Manual](https://img.shields.io/badge/docs-manual-blue)](https://gabrielgellner.github.io/vimtracker/)

An experimental music tracker for the terminal, written in Rust and edited with a
keyboard grammar modelled on Neovim. It takes ideas from the Polyend Tracker, the
Dirtywave M8, Elektron sequencers and Jeskola Buzz, but it is a separate program and is
not affiliated with any of them.

It is early, and the design and implementation plan are in
[docs/designs/vimtracker.md](docs/designs/vimtracker.md), with its numbered decisions in
[docs/designs/decisions.md](docs/designs/decisions.md).

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

Run the tests with `cargo test --workspace`.

### Terminal

vimtracker runs in [kitty](https://sw.kovidgoyal.net/kitty/) and [Ghostty](https://ghostty.org),
and won't start in other terminals or inside tmux. It needs the kitty keyboard protocol, which
says when a key is held and let go. The Option keys, such as `Option-1`, need kitty's
`macos_option_as_alt yes` or Ghostty's `macos-option-as-alt = true` in your config. `Cmd-h`
`Cmd-j` `Cmd-k` `Cmd-l` nudge a value, and kitty keeps three of them for itself until your
`kitty.conf` frees them; [docs/terminal-notes.md](docs/terminal-notes.md#cmd-keys) has the lines.

It also needs the kitty graphics protocol, as small drawings such as envelopes and the filter's
curve are images. `vimtracker probe-keys` says whether a terminal has both, and `render`, `bench`,
`probe` and `probe-keys` run in any terminal.

## Quick start

Use the release build for playing: the debug build uses far more of the audio deadline.

```sh
vimtracker my-song        # opens my-song, or creates it on the first :w
```

1.  `Space 5` lists WAV, AIFF and FLAC files under the project, your kits and sample folders and
    the folder you ran from, for the current track, with each kit opening like a folder to its
    sounds. `:sample path` takes a file, or a folder to browse.
2.  `Space 3` opens the phrase view. `i` starts Insert, and the keyboard plays as in Ableton:
    `a` to `l` are white keys, `w` to `o` black keys, `z` and `x` change octave, `c` and `v` set
    the velocity written, and the arrows move without writing. A note stays where it's written,
    and `;` turns on auto-jump, which moves on by the grid. `Esc` leaves Insert.
3.  `Tab` plays and pauses. `Shift-Tab` plays from the start, and `Space c` from the cursor.
4.  `l` moves to the next field. `K` lists the FX commands, or type a label, `Enter`, then two hex digits, such as `CUT` `Enter` `40`.
5.  `Space 2` opens the chain view, where `gd` opens a phrase, or adds one on the empty row.
6.  `Space 4` opens the track page: base filter and output values, three envelopes, three LFOs
    and four FX slots.
7.  `:w` saves. `:render loop.wav` writes the loop, ready to drop into Ableton.

`Space Space` jumps between views. `Space ?` opens the guides, short tutorials on each topic,
and `Space k` lists every key. If you pause after `Space` or another prefix, a popup shows what
can come next.

No samples come with the repo. Put your own WAV, AIFF or FLAC files in
`~/Music/vimtracker/samples` to see them in the browser. A file in `~/Music/vimtracker/kits`
listing sounds, such as `909.ron`, is a kit: `:kit 909 all` puts its sounds on tracks from the
first. Both folders are lists you can change in `~/.config/vimtracker/settings.ron`. With
Ableton Live installed, `vimtracker kits import-ableton` makes a kit of each of its drum racks,
pointing at Live's samples where they are. Tracks can have two-character names like `BD` and
`CH`. See the sounds guide.

Samples stay where they are, and a project refers to them. `:w new-folder` saves a copy with every
sample gathered into it, ready to move or share.

| Command | What it does |
|---|---|
| `:bpm 137.63` | Tempo |
| `:groove swing-66`, `:tgroove mpc-58`, `:tgroove off` | Project groove, this track's groove, follow the project |
| `:swing 66` | The project's swing groove at a percent |
| `:tvol C0` | This track's level, separate from each note's velocity |
| `:name BD`, `:name`, `:chain 2` | Name this track or clear its name, and which chain it plays |
| `:len 32`, `:double` | Phrase length, and repeat the phrase to double it |
| `:track add 4`, `:track del` | Add tracks, or remove the last |
| `:set skip=8`, `:seed 7` | The grid, which the left and right arrows and auto-jump move by, and the random seed for `CHA` |
| `:render path [--rows n] [--tail]` | Render to a WAV file |

`vimtracker render <project> <out.wav> [--rate n] [--rows n] [--seed n] [--tail]` renders
without a terminal or sound device. `vimtracker probe-keys` shows what your terminal sends, with
the kitty keyboard protocol on as the program has it, and runs in any terminal.
`vimtracker bench <project> [--rate n] [--block n] [--seconds n]` measures how much of a core a
project takes to play, stopped and playing. `cargo build --profile profiling` builds with symbols
for a profiler. `vimtracker probe <spec.toml> <out.wav> [--case name]` plays one voice or effect
with no song, from a spec that [tools/compare](tools/compare/README.md) also renders through a
reference plugin, to compare the two.

Unsaved work is written to `song.autosave.ron` every minute once the project has a folder,
and is offered back the next time you open it after a crash.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
