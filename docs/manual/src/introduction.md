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

It runs in [kitty](https://sw.kovidgoyal.net/kitty/) and [Ghostty](https://ghostty.org),
which have the kitty keyboard and graphics protocols, and won't start in other terminals or
inside tmux.

To run it:

```sh
cargo run --release -p vt-tui --bin vimtracker
```

or, with [just](https://github.com/casey/just) installed, `just run my-song` to open or
start a project in the folder `my-song`.

The pictures of the program in this manual are made from the program itself, with
`just screenshots`, so they show what it really draws. That includes the drawings the
terminal is sent as images, like a track's filter curve on its Sound page, and beside
it the waveform of the sample it plays:

![A track's Sound page, with its filter curve and its sample's waveform](images/track-sound.svg)

and its envelopes and LFO shapes on its Mod page:

![A track's Mod page, with its envelopes and LFO shapes](images/track-mod.svg)

and on its FX page, a compressor's curve, with a meter beside it of how far it's turning
down, an EQ's curve, and a limiter's meter:

![A track's FX page, with a compressor's curve and gain reduction, an EQ's curve and a limiter's gain reduction](images/track-fx.svg)

and the sample page, on `Space 0`, with the track's sample as a big waveform, left above right,
its start, end and loop marked, and an overview of the whole sample below with the part in view:

![The sample page, with a drum loop's waveform, its start, end and loop, and the cursor's column](images/sample.svg)
