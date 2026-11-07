# Introduction

vimtracker is a music tracker for the terminal, written in Rust and edited with a keyboard
grammar modelled on Neovim. It takes ideas from the Polyend Tracker, the Dirtywave M8,
Elektron sequencers, Metropolix and Jeskola Buzz, but it's a separate program, and isn't
affiliated with any of them.

This manual has three parts:

- **Getting started** takes you from an empty project to a loop you can hear.
- **How it works** explains the ideas the rest is built on.
- **Reference** covers each part of the program in turn.

The reference chapters are the same guides the program shows with `Space ?`, so what you
read here is what you see there.

To run it:

```sh
cargo run --release -p vt-tui --bin vimtracker
```

or, with [just](https://github.com/casey/just) installed, `just run my-song` to open or
start a project in the folder `my-song`.

The pictures of the program in this manual are made from the program itself, with
`just screenshots`, so they show what it really draws. What the terminal draws as
images, like the filter curve, isn't in them.
