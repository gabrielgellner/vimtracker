# Terminal notes

vimtracker runs in kitty and Ghostty, with the kitty keyboard protocol on from start to quitting
(decision 50), and draws its curves with the kitty graphics protocol (decision 56). Every key comes
as an escape code, as a press, a repeat or a release, with the shifted character where the terminal
knows it. In any other terminal, or inside tmux, it stops at start and says why.

`vimtracker probe-keys` prints what a terminal sends with the same flags, says whether the terminal
offers the keyboard and graphics protocols, and shows the key the program reads each event as. It
runs in any terminal.
Run it with:

```sh
cargo run -p vt-tui --bin vimtracker -- probe-keys
```

## In kitty

Measured with `probe-keys` in kitty 0.50 on macOS on 2026-11-08, through kitty's remote control
(`scripts/live.sh`):

| Sent | Arrives as | Reads as |
|---|---|---|
| `shift+;`, `shift+a`, `shift+4` and the like | The plain key with `SHIFT`: `;`, `a`, `4` | `:`, `A`, `$` |
| `shift+tab` | `BackTab` with `SHIFT` | `S-Tab` |
| `shift+up` and the other arrows | The arrow with `SHIFT` | `S-Up` |
| `alt+1` | `1` with `ALT` | `M-1` |
| `ctrl+d`, `ctrl+a` | The key with `CONTROL` | `C-d`, `C-a` |
| `ctrl+i`, `ctrl+j`, `ctrl+m` | `i`, `j`, `m` with `CONTROL`, apart from `Tab` and `Enter` | `C-i`, `C-j`, `C-m` |
| `ctrl+1` to `ctrl+0` | The digit with `CONTROL` | `C-1` to `C-0` |
| `ctrl+shift+a`, `ctrl+shift+1` | The plain key with `SHIFT` and `CONTROL` | `C-A`, `C-!` |
| `ctrl+enter`, `ctrl+tab` | The key with `CONTROL` | `C-Enter`, `C-Tab` |
| `super+1`, `super+a` | The key with `SUPER` | `D-1`, `D-a`, dropped |
| `shift+enter`, `shift+space`, `shift+backspace`, `shift+escape` | The key with `SHIFT` | `Enter`, `Space`, `Backspace`, `Esc` |
| `escape`, `enter`, `tab`, `backspace`, `space` | Themselves, with no wait on `Esc` | |

Remote control sends no shifted key, so the program takes Shift on a plain key to mean its shifted
character on a US layout. A real keyboard can send the shifted key as well, which crossterm uses,
taking Shift off. Each key is a `Press` and a `Release`. kitty's names for keys in `send-key` are
the characters themselves for `[`, `]` and `` ` ``, as `shift+]`.

Held on a real keyboard, in a plain kitty window on 2026-11-08, `s` and `j` came as one `Press`,
`Repeat`s every 83 ms after 500 ms, and a `Release`, but `a` as `Press` after `Press` at the same
rhythm, with one `Release` at the end. So a press of the key already down is taken as a repeat: a
real second press always comes after a release.

So every key the protocol frees up reaches the program as itself: `Ctrl-I` apart from `Tab`, which
is why jump forward is now on `Ctrl-I` (decision 52), `Ctrl-J` and `Ctrl-M` apart from `Enter`,
and `Ctrl` with any digit. Shift is kept only on the arrows, and folded into characters or dropped
elsewhere, so `Shift-Enter` is `Enter`.

Cmd is `SUPER`, named `D-` as in Vim. It reaches the program, but nothing is bound to it, so a key
pressed with Cmd is dropped, and `probe-keys` shows it as `D-1, ignored`. Its release still counts,
so letting go of a held note with Cmd down ends the note. Remote control sends a key to the program
past kitty's own shortcuts, and those take `Cmd-1` to `Cmd-9` (its windows), `Cmd-0` (font size),
`Cmd-C`, `Cmd-V`, `Cmd-T`, `Cmd-W` and more by default, so on a real keyboard those only reach
the program once `kitty.conf` unmaps them. The same goes for `Ctrl-Shift` with a letter, kitty's
`kitty_mod`.

`Option-1`, `Option-2` and `Option-s` work in kitty, on 2026-11-06, with `macos_option_as_alt`
set. `scripts/live.sh` sets it.

## Images

At start, after the keyboard check, the program asks the terminal whether it takes an image
(`kitty::supports_graphics`). It sends the graphics protocol's query, `a=q` with id 31 and a PNG of
one dot in the same format as its drawings, then asks for the primary device attributes
(`CSI c`), which every terminal answers. The reply is read from `/dev/tty` in raw mode on a thread
of its own, up to the attributes, so nothing is left over for crossterm to read. An `OK` for id 31
before the attributes is a yes, and anything else, or no answer in 2 seconds, is a no.

What `kitty.rs` uses of the protocol:

| Part | Used as |
|---|---|
| Transmit and place | `a=T,U=1,f=100,i=<id>,c=<columns>,r=<rows>,q=2`, a virtual placement of a PNG |
| PNG | 1 bit deep, with a palette of two colours and the first see-through (`tRNS`) |
| Chunking | Base64 in pieces of at most 4096 bytes, `m=1` on all but the last |
| Unicode placeholders | `U+10EEEE` with row and column diacritics, the image id in the 24-bit foreground colour, no placement id and no third diacritic, as ids fit in 24 bits |
| Delete | `a=d,d=I,i=<id>,q=2`, by id, freeing the data, when an image is replaced and on quitting |
| Query | `a=q,i=31,f=100` at start, as above |

In kitty 0.50 on 2026-11-08, `scripts/live.sh` with the program run under `script` to record what
it wrote showed the query, then the filter curve, envelope and LFO images sent as above on the
track page, and the replaced ones deleted by id.

### Ghostty

Ghostty isn't installed here, so this is from its source and issues, read on 2026-11-08, and not
tried:

*   Unicode placeholders came in Ghostty's PR #2015, in July 2024, before 1.0. Issue #13056 shows
    a `U=1` placement over `U+10EEEE` cells rendering full width outside tmux on 1.3.1. One
    compatibility table (terminfo-dev, Ghostty 1.3.1, 2026-10-07) lists placeholders as not
    rendering, with no detail, which disagrees with that, so it's the first thing to check by hand.
*   PR #15086 (2026-11-05) makes placeholders find only virtual placements, as kitty does. The
    program only makes virtual ones, so it isn't affected either way.
*   `a=q` is parsed and answered with `OK` (`graphics_command.zig`), and deletion by `i` and `I`
    is there. PNGs are decoded by the app (with wuffs); libghostty without a PNG decoder would
    refuse `f=100`, and so the query, which then stops the program rather than drawing nothing.
*   Ghostty doesn't answer the protocol's newer feature queries (issue #14843), which the program
    doesn't use.

## Still to check by hand

*   The same in Ghostty, with `macos-option-as-alt`: shifted keys, held keys, the Option keys,
    `Ctrl-I` apart from `Tab`, `Ctrl` with digits, and which `Cmd` keys Ghostty's own shortcuts take.
*   In Ghostty, that the program starts, so its answer to the image query is `OK`, and that the
    track page's filter curve, envelopes and LFO shapes show, change with their settings and the
    cursor, and leave nothing behind when another view is drawn over them.
*   Whether a real keyboard in kitty sends the shifted key as well, and whether a non-US layout's
    shifted characters read right.
*   A key held while the window loses focus: whether its release still comes, as a held note on the
    track page's keyboard waits for it.

## Before M16

Without the protocol, measured with `tmux send-keys` to a private tmux server using the author's
`.tmux.conf`, on 2026-11-05 and 2026-11-06. These are kept for the record of why keys were
chosen as they were.

| Key | Arrived as | Note |
|---|---|---|
| `Ctrl-X`, `Ctrl-O`, `Ctrl-R`, `Ctrl-N`, `Ctrl-P`, `Ctrl-D`, `Ctrl-U`, `Ctrl-E`, `Ctrl-Y` | The same key with `CONTROL` | |
| `Tab`, `Ctrl-I` | `Tab` | The two couldn't be told apart, so jump forward was on `Ctrl-N` until decision 52 |
| `Enter` | `Enter` | |
| `Ctrl-J` | `j` with `CONTROL` | Distinct from `Enter` |
| `Esc` | `Esc` | tmux `escape-time` is 10 ms in the config |

`Ctrl` with a number mostly couldn't be used: `Ctrl-1` arrived as `1`, `Ctrl-2` as `Ctrl-Space`,
`Ctrl-3` as `Esc`, `Ctrl-8` as `Backspace`, and `Ctrl-9` and `Ctrl-0` as plain digits. Only
`Ctrl-4` to `Ctrl-7` arrived as themselves. Inside tmux 3.8, crossterm's support query for the
protocol says no, as tmux answers it itself and doesn't pass releases on.
