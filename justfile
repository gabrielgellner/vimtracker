# List the recipes
default:
    @just --list

# Open a project, or start without one: just run my-song
run *args:
    cargo run --release -p vt-tui --bin vimtracker -- {{args}}

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
