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

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
