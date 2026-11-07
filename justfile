# List the recipes
default:
    @just --list

# Open a project, or start without one: just run my-song
run *args:
    cargo run --release -p vt-tui --bin vimtracker -- {{args}}

# Build the manual into docs/manual/book
docs:
    mdbook build docs/manual

# Serve the manual, rebuilding as it changes: just docs-serve, then open the address it prints
docs-serve:
    mdbook serve docs/manual --open

# Run every test
test:
    cargo test --workspace
