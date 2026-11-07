# Contributing

## Tools

- [Rust](https://rustup.rs/), the stable toolchain.
- [just](https://github.com/casey/just), which runs the recipes in the `justfile`. Run `just`
  to list them.
- [mdBook](https://rust-lang.github.io/mdBook/), for the manual.
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
just docs-serve       # the manual in a browser, rebuilt as it changes
```

## Pull requests and CI

Every change reaches `main` through a pull request from its own branch. Merge with a merge
commit, not a squash or a rebase. The changelog skips merge commits and lists the branch's own
commits, so their types matter (see below).

Every pull request and every push to `main` runs the `ci` workflow, which runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Run the same before pushing. `--locked` fails if `Cargo.lock` is out of date with the
manifests.

CI runs on Apple silicon macOS. The render tests compare hashes of rendered audio, and the
filter and reverb use library maths that can round differently on other platforms, so the
hashes are only checked on one.

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
