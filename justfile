# List the recipes
default:
    @just --list

# Open a project, or start without one: just run my-song
run *args:
    cargo run --release -p vt-tui --bin vimtracker -- {{args}}

# Run every test
test:
    cargo test --workspace
