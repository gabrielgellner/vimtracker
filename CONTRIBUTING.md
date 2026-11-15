# Contributing

## Tools

- [Rust](https://rustup.rs/), the stable toolchain.
- [just](https://github.com/casey/just), which runs the recipes in the `justfile`. Run `just`
  to list them.
- [mdBook](https://rust-lang.github.io/mdBook/), for the manual.
- [cargo-nextest](https://nexte.st/), optional, which runs the tests in about four fifths of the
  time `cargo test` takes: `cargo install cargo-nextest --locked`. `just test` and `just ci` use it when
  it's installed and `cargo test` when it isn't.
- For releases only:
  - [git-cliff](https://git-cliff.org/), which writes the changelog;
  - [cargo-edit](https://github.com/killercup/cargo-edit), which adds `cargo set-version`;
  - [gh](https://cli.github.com/), the GitHub command line.

On Linux, the audio output needs the ALSA headers (`libasound2-dev` on Debian and Ubuntu).

## Development

```sh
just run my-song      # a release build: a debug build uses far more CPU, even when idle
just test             # every test
just screenshots      # remake the manual's screenshots
just symbols-font     # after adding a Nerd Font symbol, cut the manual's font down to them again
just docs-serve       # the manual in a browser, rebuilt as it changes
just live start       # the program in a kitty window of its own, to check a change by hand
```

### Checking the real program

`just live` (`scripts/live.sh`) runs the release build in a minimised kitty window of its own, in a
fresh folder, and drives it through kitty's remote control, so a change can be checked in the real
program from a script or an agent. It needs kitty, as the program does.

```sh
cargo build --release
just live start                 # or: just live start my-song
just live keys enter            # kitty's key names, each pressed and let go
just live text ' 5'             # text as it is: here the leader, then 5, the Sound page
just live screen                # print what's on screen
just live stop
```

`screen` prints text only, so images such as the filter's curve don't show. To see them, start
with `VT_LIVE_SHOW=1`, which opens the window where it can be seen, at the 163 by 42 the program is
laid out for, rather than minimised, and `just live shot out.png` saves a picture of that window
alone, kitty's images and all. Set `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` to folders of the check's
own first, so the real ones are left alone.

A key can't be held from here, so what a held key does is tested in `crates/vt-tui/tests/all/`,
mostly `keys.rs` and `recording.rs`, which can say when a key is let go.

## Pull requests and CI

Every change reaches `main` through a pull request from its own branch. Merge with a merge
commit, not a squash or a rebase. The changelog skips merge commits and lists the branch's own
commits, so their types matter (see below).

Before a pull request merges, `just ci` runs on the author's machine:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo nextest run --workspace --locked    # or cargo test --workspace --locked, without nextest
cargo test --workspace --locked --doc     # nextest doesn't run doctests
```

`--locked` fails if `Cargo.lock` is out of date with the manifests. The `ci` workflow runs the
same checks on GitHub, by hand only.

The tests run with `vt-engine` and `vt-model` optimised, as the `dev` profile in `Cargo.toml`
sets, with debug assertions and overflow checks still on. A few sweeps over every extreme of an
effect run in full only in a release build, and `cargo test --workspace --release` runs them, with
the real-time guard against optimised code and the hashes made in release builds. Run it too
after a change to the engine.

CI runs on Apple silicon macOS. The render tests compare hashes of rendered audio, and the
filter and reverb use library maths that can round differently on other platforms, so the
hashes are only checked on one.

A pull request that makes a choice the issue left open appends it to
[docs/designs/decisions.md](docs/designs/decisions.md) as the next numbered decision. One that
changes the structure, such as a crate, a thread or queue, the song model, what the engine sees,
how pages are built, how images are drawn, an offline path or where a kind of test lives, also
updates the design's [Architecture](docs/designs/vimtracker.md#architecture) section in the same
pull request.

### Tests

`vt-engine`, `vt-model` and `vt-tui` each build their integration tests as one binary,
`tests/all/main.rs`, so they link once rather than once a file (#719). A new file of tests goes in
`tests/all/`, with a `mod` line in its `main.rs`, as a file without one isn't built; a test there
fails on a file missing its line, or left in `tests/`. Helpers that more than one file uses go in
`tests/all/support.rs` for the engine and `tests/all/common/` for the interface, and the DSP unit
tests' signals in `vt-engine`'s `src/dsp/signals.rs`. One file's tests run with
`cargo test -p vt-engine --test all groove::` or `cargo test -p vt-tui --test all grammar::`.

- **The real-time guard.** `vt-engine`'s `src/rt_guard.rs` is the allocator of its own test
  builds. `vt-model`'s `tests/all/main.rs` builds the same file through `#[path]`, as the model
  can't depend on the engine, so `render.rs` there can count what a generator's render allocates.
  It counts only while a test arms it.
- **Goldens.** The engine's `tests/golden/*.hash` and the model's `tests/golden/*.txt` are
  written again with `VT_BLESS=1`, only after a change on purpose. The engine's
  `tests/all/goldens.rs` fails while an effect or a voice has no golden. Beside each hash is what
  the render sounded like: its peak, RMS, DC, where it ends and how bright it is. A changed hash
  fails with each of them before and after and whether it stayed in its band, as a small fix
  keeps it; say in the commit why a sound that left one moved, and the bless's diff shows how.
- **The Roto.** `vt-roto`'s tests are the four files in its `tests/`, against the fake Roto in
  memory. The fake is built only with the crate's `fake` feature, which its own tests and
  `vt-tui`'s turn on as a dev-dependency, so a release build leaves it out.

### Where code goes

- **vt-tui** is in folders by what each module is, as the Architecture's
  [Interface](docs/designs/vimtracker.md#interface) maps them: a page's keys, edits and drawing in
  its folder under `pages/`, what's drawn over a page in `overlays/`, what pages share in
  `services/`. An `impl App` block goes in the folder of the page, overlay or service it's for.
- **vt-engine**'s DSP goes in `src/dsp/` by what it does (`voices/`, `filters/`, `effects/` and
  `dynamics/`), as the [DSP layout](docs/designs/vimtracker.md#dsp-layout) says. A part two of
  them share goes in `dsp/blocks/`. New DSP uses its one-pole, delay lines and Hermite reads,
  coefficients and clips, and `Pcg32` for noise, rather than writing its own. What vt-tui draws of
  the sound is re-exported in `src/pictures.rs`, and what the engine's tests play on their own in
  `src/parts.rs`.
- **Files.** The program writes its files through `vt-model`'s `ron_file.rs`
  (`vt_model::ron_file`), from `vt-tui` too, and reads its songs, presets, kits and tags through
  it, so an error names the file. A new file the program writes uses its `write` or `write_atomic`, which write a temporary file beside it and rename it
  over, so a crash mid-write leaves the file before whole, rather than a bare `fs::write`.

**Visibility.** In `vt-tui` and `vt-engine`, make new code `pub(crate)` unless another crate, a
test, an example or `main.rs` uses it. Both crates' `lib.rs` warn on `unreachable_pub`, which says
when a `pub` can't be reached from outside, and that keeps the dead-code lint working. In
`vt-engine`, `dsp` is `pub(crate)`: reach it from outside only through `pictures` or `parts`.
`vt-model` and `vt-roto` keep their modules `pub`, as the other crates use nearly all of them.

A new effect, voice, generator or mod type has a checklist of its own,
[docs/designs/adding-a-device.md](docs/designs/adding-a-device.md): every place it takes, what the
compiler catches if one is missed, and which pinned names and goldens show the change.

### Dependencies

`Cargo.lock` is committed and keeps builds reproducible, and `just ci` passes `--locked`, so a
manifest changed without its lock fails. `Cargo.toml` gives each dependency as a caret range,
`ratatui = "0.30.2"`, so `cargo update` takes patch and minor releases without editing it. The
exception is a crate whose arithmetic a golden render hears, pinned with `=` so an update can't
move a hash unasked: `realfft`, for `Clouds`' spectral mode, and `wide`, for the Supersaw's and the
4-op's lanes. Reading files, resampling them or drawing doesn't count, as the goldens build their
sounds in memory, so `hound`, `symphonia` and `rubato` take ranges too. A new crate whose output a
golden hears is pinned the same way, with a comment saying which goldens (decision 352). The lock
also holds what those crates bring in, such as `rustfft` under `realfft`, which `cargo update`
moves, so run the tests after one.

## The manual

The manual is in `docs/manual`, and it includes the guides in `crates/vt-tui/guides`. The
`docs` workflow publishes it to <https://gabrielgellner.github.io/vimtracker/> whenever a
change to either reaches `main`.

## Commit style

Commits use [Conventional Commits](https://www.conventionalcommits.org/). The type decides the
heading a commit is listed under in the changelog, and how `just bump` picks the next version:

| Type | Example | Heading | Version |
|---|---|---|---|
| `feat:` | `feat: add the gate column` | Features | minor |
| `fix:` | `fix: keep the cursor on the mixer's rows` | Bug Fixes | patch |
| `perf:` | `perf: draw the filter curve less often` | Performance | patch |
| `refactor:` | `refactor: split the phrase view` | Refactoring | patch |
| `docs:` | `docs: add the lanes chapter` | Documentation | patch |
| `chore:`, `ci:`, `build:`, `test:` | `ci: build on macOS` | Miscellaneous | patch |
| `feat!:`, or `BREAKING CHANGE:` in the body | | | major |

Before version 1.0, a breaking change bumps the minor version instead.

Use `feat:` for anything a user will notice, such as a key that does something new, even when
it feels like fixing a rough edge. Keep `fix:` for something that was broken.

The history before the first release isn't in this form. Those commits are listed under
Changes.

## Releases

Cut a release from an up-to-date `main` with all its pull requests merged.

See what the release would add:

```sh
just changelog-preview
```

Make the release commit and tag:

```sh
just bump 0.1.0    # the first release needs a version
just bump          # after that, git-cliff picks it from the commit types
```

`just bump` sets the workspace version, which every crate inherits; writes `CHANGELOG.md`;
commits it as `chore(release): vX.Y.Z`, which the changelog leaves out; and makes the tag
`vX.Y.Z`. It pushes nothing. Check it with `git log --oneline -5`, and undo it with:

```sh
git tag -d vX.Y.Z
git reset --hard HEAD~1
```

Publish it:

```sh
just release
```

This pushes `main` and the tag, and makes the GitHub release with this version's part of the
changelog as its notes. The tag starts the `release` workflow, which builds `vimtracker` for
macOS on Apple silicon and Linux on x86-64 and arm64, and attaches the archives and a
`SHA256SUMS` file to the release. Watch it with `gh run watch`.

Publishing doesn't change the `vimtracker` on your own machine. `just install` installs the
working tree's.
