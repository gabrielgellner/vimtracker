# List the recipes
default:
    @just --list

# Open a project, or start without one: just run my-song
run *args:
    cargo run --release -p vt-tui --bin vimtracker -- {{args}}

# Drive the release build in a kitty window of its own: just live start, keys, text, screen, stop
live *args:
    scripts/live.sh {{args}}

# Make the manual's screenshots of the interface, as SVGs in docs/manual/src/images
screenshots:
    cargo run --release -q -p vt-tui --example screenshots

# Build the manual into docs/manual/book
docs:
    mdbook build docs/manual

# Serve the manual, rebuilding as it changes: just docs-serve, then open the address it prints
docs-serve:
    mdbook serve docs/manual --open

# Run every test
test:
    cargo test --workspace

# Show what the next release would add to the changelog
changelog-preview:
    git-cliff --unreleased

# Write CHANGELOG.md from the whole history
changelog-full:
    git-cliff -o CHANGELOG.md
    @echo "CHANGELOG.md updated"

# Print the version that `just bump` would choose (needs at least one vX.Y.Z tag)
next-version:
    @git-cliff --bumped-version

# For the first release give the version: `just bump 0.1.0`. Otherwise git-cliff picks it from
# the commit types since the last tag; give one yourself when that's wrong. Review with
# `git log --oneline -5`, then run `just release`.
#
# Set the version, write CHANGELOG.md, commit and tag, without pushing: just bump, or just bump 0.2.0
bump version="":
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
        echo "ERROR: the working tree has changes. Commit or stash them first."
        exit 1
    fi

    if [ -n "{{version}}" ]; then
        NEXT="{{version}}"
        [[ "$NEXT" == v* ]] || NEXT="v$NEXT"
    else
        NEXT=$(git-cliff --bumped-version) || {
            echo ""
            echo "ERROR: could not work out the next version (no tags yet?)."
            echo "For the first release run:  just bump 0.1.0"
            exit 1
        }
    fi

    VER="${NEXT#v}"
    echo "Bumping to $NEXT"

    # The crates take their version from the workspace, so this sets all of them, and
    # Cargo.lock with them.
    cargo set-version --workspace "$VER"

    # --tag names the release that the unreleased commits belong to.
    git-cliff --tag "$NEXT" -o CHANGELOG.md

    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "chore(release): $NEXT"
    git tag -a "$NEXT" -m "Release $NEXT"

    echo ""
    echo "Made the commit and tag $NEXT."
    echo "Review with:  git log --oneline -5"
    echo "Then run:     just release"

# Pushing the tag starts the binary builds, which attach to the release. The notes are
# written here because git-cliff's are better than GitHub's generated ones. Whichever of the
# two gets there second edits the release instead of failing.
#
# Push main and the tag, and make the GitHub release with the changelog as its notes
release:
    #!/usr/bin/env bash
    set -euo pipefail

    TAG=$(git describe --tags --exact-match 2>/dev/null) || {
        echo "ERROR: HEAD isn't on a tag. Run 'just bump' first."
        exit 1
    }

    git push origin HEAD
    git push origin "$TAG"

    # This release's section of the changelog, from the first group heading on. The version
    # heading is left out, because the release already has the version as its title.
    NOTES=$(git-cliff --latest --strip all | awk 'f || /^###/ { f = 1; print }')

    gh release create "$TAG" --title "Release $TAG" --notes "$NOTES" 2>/dev/null \
        || gh release edit "$TAG" --title "Release $TAG" --notes "$NOTES"

    echo "Released $TAG."
    echo "The binaries are building: gh run watch"

# Install the working tree's vimtracker into ~/.cargo/bin
install:
    cargo install --path crates/vt-tui --locked
    @echo "Now on PATH: $(vimtracker --version) at $(command -v vimtracker)"
