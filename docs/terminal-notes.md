# Terminal notes

vimtracker runs in kitty and Ghostty, with the kitty keyboard protocol on from start to quitting
(decision 50). Every key comes as an escape code, as a press, a repeat or a release, with the
shifted character where the terminal knows it. In any other terminal, or inside tmux, it stops at
start and says why.

`vimtracker probe-keys` prints what a terminal sends with the same flags, says whether the terminal
offers the protocol, and shows the key the program reads each event as. It runs in any terminal.
Run it with:

```sh
cargo run -p vt-tui --bin vimtracker -- probe-keys
```

## In kitty

Measured on macOS on 2026-11-08, with kitty's remote control (`scripts/live.sh`):

| Sent | Arrives as | Reads as |
|---|---|---|
| `shift+;`, `shift+a`, `shift+4` and the like | The plain key with `SHIFT`: `;`, `a`, `4` | `:`, `A`, `$` |
| `shift+tab` | `BackTab` with `SHIFT` | `S-Tab` |
| `shift+up` and the other arrows | The arrow with `SHIFT` | `S-Up` |
| `alt+1` | `1` with `ALT` | `M-1` |
| `ctrl+d`, `ctrl+a` | The key with `CONTROL` | `C-d`, `C-a` |
| `escape`, `enter`, `tab`, `backspace`, `space` | Themselves, with no wait on `Esc` | |

Remote control sends no shifted key, so the program takes Shift on a plain key to mean its shifted
character on a US layout. A real keyboard can send the shifted key as well, which crossterm uses,
taking Shift off. Each key is a `Press` and a `Release`. kitty's names for keys in `send-key` are
the characters themselves for `[`, `]` and `` ` ``, as `shift+]`.

Held on a real keyboard, in a plain kitty window on 2026-11-08, `s` and `j` came as one `Press`,
`Repeat`s every 83 ms after 500 ms, and a `Release`, but `a` as `Press` after `Press` at the same
rhythm, with one `Release` at the end. So a press of the key already down is taken as a repeat: a
real second press always comes after a release.

`Option-1`, `Option-2` and `Option-s` work in kitty, on 2026-11-06, with `macos_option_as_alt`
set. `scripts/live.sh` sets it.

## Still to check by hand

*   The same in Ghostty, with `macos-option-as-alt`: shifted keys, held keys, and the Option keys.
*   Whether a real keyboard in kitty sends the shifted key as well, and whether a non-US layout's
    shifted characters read right.
*   A key held while the window loses focus: whether its release still comes, as a held note on the
    track page's keyboard waits for it.

## Before M16

Without the protocol, measured with `tmux send-keys` to a private tmux server using the author's
`.tmux.conf`, on 2026-11-05 and 2026-11-06. These are kept for #202, which uses the keys the
protocol frees up.

| Key | Arrived as | Note |
|---|---|---|
| `Ctrl-X`, `Ctrl-O`, `Ctrl-R`, `Ctrl-N`, `Ctrl-P`, `Ctrl-D`, `Ctrl-U`, `Ctrl-E`, `Ctrl-Y` | The same key with `CONTROL` | |
| `Tab`, `Ctrl-I` | `Tab` | The two couldn't be told apart, so jump forward isn't on `Ctrl-I` as in Vim |
| `Enter` | `Enter` | |
| `Ctrl-J` | `j` with `CONTROL` | Distinct from `Enter` |
| `Esc` | `Esc` | tmux `escape-time` is 10 ms in the config |

`Ctrl` with a number mostly couldn't be used: `Ctrl-1` arrived as `1`, `Ctrl-2` as `Ctrl-Space`,
`Ctrl-3` as `Esc`, `Ctrl-8` as `Backspace`, and `Ctrl-9` and `Ctrl-0` as plain digits. Only
`Ctrl-4` to `Ctrl-7` arrived as themselves. Inside tmux 3.8, crossterm's support query for the
protocol says no, as tmux answers it itself and doesn't pass releases on.
