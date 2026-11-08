# Terminal notes

Results from `vimtracker probe-keys`. Run it with:

```sh
cargo run -p vt-tui --bin vimtracker -- probe-keys
```

## Measured so far

These were sent with `tmux send-keys` to a private tmux server using the author's
`.tmux.conf`, on 2026-11-05, with no keyboard flags pushed.

| Key | Arrives as | Note |
|---|---|---|
| `Ctrl-X`, `Ctrl-O`, `Ctrl-R`, `Ctrl-N`, `Ctrl-P`, `Ctrl-D`, `Ctrl-U`, `Ctrl-E`, `Ctrl-Y` | The same key with `CONTROL` | No replacement needed |
| `Tab`, `Ctrl-I` | `Tab` | The two can't be told apart, so jump forward is on `Tab` |
| `Enter` | `Enter` | |
| `Ctrl-J` | `j` with `CONTROL` | Distinct from `Enter` |
| `Esc` | `Esc` | tmux `escape-time` is 10 ms in the config |

`Ctrl` with a number mostly can't be used. Sent the same way on 2026-11-06, `Ctrl-1` arrives as
`1`, `Ctrl-2` as `Ctrl-Space`, `Ctrl-3` as `Esc`, `Ctrl-8` as `Backspace`, and `Ctrl-9` and
`Ctrl-0` as plain digits. Only `Ctrl-4` to `Ctrl-7` arrive as themselves.

## The kitty keyboard protocol

The track page's keyboard (`;`) asks for it, so a held key holds its note. Inside tmux 3.8 with
the author's `.tmux.conf`, on 2026-11-08, crossterm's support query answered no: tmux answers
the query itself and doesn't report key releases. So inside tmux, the keyboard plays short
notes. `probe-keys --kitty` shows whether a terminal supports it and what it sends.

## Checked on a real keyboard

*   `Option-1`, `Option-2` and `Option-s` work in kitty, on 2026-11-06. That needs
    `macos_option_as_alt`.

## Still to check by hand

`send-keys` writes directly to the pane, so these need a real keyboard, in kitty, with and
without tmux:

*   `Ctrl-A`. It is the tmux prefix and isn't passed through by the config, so it is
    expected not to arrive inside tmux. `+` and `-` are the main keys for this reason.
*   Held keys: the gap before the first repeat, and between repeats. The guard for
    transport keys depends on it.
*   How `Esc` feels in practice.
