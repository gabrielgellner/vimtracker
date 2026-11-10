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

Cmd is `SUPER`, named `D-` as in Vim. It reaches the program, and only `Cmd-h` `Cmd-j` `Cmd-k`
`Cmd-l` are bound (decision 67). Any other key pressed with Cmd is dropped, and `probe-keys` shows
it as `D-1, ignored`. Its release still counts,
so letting go of a held note with Cmd down ends the note. Remote control sends a key to the program
past kitty's own shortcuts, and those take `Cmd-1` to `Cmd-9` (its windows), `Cmd-0` (font size),
`Cmd-C`, `Cmd-V`, `Cmd-T`, `Cmd-W` and more by default, so on a real keyboard those only reach
the program once `kitty.conf` unmaps them. The same goes for `Ctrl-Shift` with a letter, kitty's
`kitty_mod`.

### Cmd keys

`Cmd-h` `Cmd-j` `Cmd-k` `Cmd-l` change the value under the cursor without moving it. kitty takes
three of them by default: `cmd+h` hides kitty, `cmd+k` clears up to the cursor, and `cmd+l` clears
the last command's output; `cmd+j` is free. The program sets the window title to `vimtracker`, or
`vimtracker: <project>`, so these lines in `kitty.conf` hand the keys to it while its window has
focus, and leave kitty's shortcuts everywhere else:

```
map --when-focus-on title:vimtracker cmd+h
map --when-focus-on title:vimtracker cmd+j
map --when-focus-on title:vimtracker cmd+k
map --when-focus-on title:vimtracker cmd+l
```

A mapping with no action unmaps the shortcut, so the key goes to the program, and
`--when-focus-on` makes that hold only for a window whose title matches (`title:` is a regular
expression, so it matches the project's title too). The alternative is `map cmd+h no_op` and so on,
which passes the key to every program in every window, so kitty's own shortcuts are gone
everywhere.

Ghostty's equivalent is `keybind = super+k=unbind` and so on, from its keybind docs, which pass
the key to the program. Ghostty has no condition on the window title, so that frees the key in
every window. Unchecked: Ghostty isn't installed here, its docs say an unbound key is sent through
"if it is printable", and whether that covers a key with Cmd isn't known.

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
| PNG | 1, 2 or 4 bits deep, as a picture's colours need, with a palette whose first entry is see-through (`tRNS`); the small drawings are 1 bit with one colour. Pixels that fit in one stored zlib block, as every small drawing's do, are stored; bigger ones are compressed inside the PNG |
| Chunking | Base64 in pieces of at most 4096 bytes, `m=1` on all but the last |
| Unicode placeholders | `U+10EEEE` with row and column diacritics, all 297 of them, so an image covers up to 297 characters across and down; the image id in the 24-bit foreground colour, no placement id and no third diacritic, as ids fit in 24 bits |
| Delete | `a=d,d=I,i=<id>,q=2`, by id, freeing the data, when an image is replaced and on quitting |
| Query | `a=q,i=31,f=100` at start, as above |
| Store, to place directly | `a=t,f=100,i=<id>,q=2`: the sample page's cursor as a line, and its ruler's marks, one image for each kind of point, which the program places itself |
| Place directly | `a=p,i=<id>,p=<placement>,X=<pixels>,h=<pixels>,C=1,z=1,q=2` at the cursor, moved there with `CSI row;column H` between a save and a restore of the cursor (`ESC 7`, `ESC 8`). Placing again with the same `p` moves it. The cursor's line is `p=1`, and each of a kind's marks has a `p` of its own |
| Take off | `a=d,d=i,i=<id>,p=<placement>,q=2` for one placement, or without `p` for all of an image's, which keeps the data to place again |

In kitty 0.50 on 2026-11-08, `scripts/live.sh` with the program run under `script` to record what
it wrote showed the query, then the filter curve, envelope and LFO images sent as above on the
track page, and the replaced ones deleted by id.

### Big images

The sample page's waveform is to be 160 characters by 14, which at the author's 15 by 39 pixels a
character (kitty full screen on a MacBook, 163 by 42 characters at 2560×1664) is 2400 by 546
pixels. `cargo run --release -p vt-tui --example big_image` draws one, in four colours, sends it
ten times each way the protocol offers, and times each from starting to write it to the terminal's
`OK`, so the time counts the terminal's decoding too. In kitty 0.50 on macOS, on 2026-11-09, with
the median of ten (other runs agreed within about a millisecond, apart from one on a busy machine
up to 3 ms slower):

| Way | Bytes sent | Encode ms | Round trip ms |
|---|---|---|---|
| PNG, stored, as the small drawings are | 876,556 | 1.7 | 13.2 |
| The stored PNG with `o=z`, zlib level 1 | 47,923 | 1.9 | 12.2 |
| The stored PNG with `o=z`, level 6 | 15,827 | 3.2 | 11.9 |
| PNG compressing its own data, level 1 | 47,346 | 0.9 | 11.3 |
| PNG compressing its own data, level 6, which `Picture::png` makes | 15,518 | 2.2 | 10.8 |
| Raw RGBA with `o=z`, level 1 | 270,400 | 2.8 | 20.8 |
| The stored PNG through shared memory, `t=s` | 64 | 1.6 | 6.2 |
| Raw RGBA through shared memory | 81 | 0.8 | 4.9 |

*   A PNG with `o=z` needs `S=`, the PNG's size before compressing, in kitty, as its docs say;
    without it kitty refused every one. Ghostty's source decompresses first and needs no size.
*   The program compresses inside the PNG instead: the same bytes as `o=z` for a little less time,
    no key that kitty and Ghostty treat differently, and a PNG that any reader, and `just
    screenshots`, takes as it is.
*   Shared memory is the fastest, at half the round trip, but needs `shm_open` and `mmap` through
    `unsafe` code, and an object the terminal never reads stays until the machine restarts. At
    11 ms a send, and only on zoom, scroll or edit, compressing is enough for now.
*   kitty fits an image to the characters its placement covers keeping its aspect ratio, so a big
    image should be drawn at the terminal's own character size in pixels
    (`crossterm::terminal::window_size`) to fill them with no scaling. Remote control's window here
    had characters of 14 by 25.

**Cell backgrounds under an image.** kitty's docs say of a placeholder: "The background color is
interpreted as the background color, visible if the image is transparent." The example draws the
image over cells whose background is a selection, a cursor, and a playhead that moves every frame,
to check by eye. The author checked it full screen on 2026-11-09, in kitty 0.50.0 and Ghostty
1.4.0: the waveform fills its cells past the 28th column, the selection, cursor and playhead show
through it, and the playhead moves smoothly with nothing left behind. A cell background fills whole
cells, so the playhead is a character wide; a finer one is an idea for the sample page (#274).

**A line placed directly** (decision 102). The sample page's cursor can be a 2-pixel line at its
frame rather than its character's background: an image of its own, sent once with `a=t` and placed
with `a=p` at a character, `X=` pixels into it, cropped with `h=` to the rows on screen, above the
text (`z=1`). Moving it is placing it again with the same placement id, about 150 bytes against
about 2.6 KB for redrawing a column of placeholders with a new background. A script sending the
program's own commands with `q=0` to see the answers, on 2026-11-09, in kitty 0.50.0 and Ghostty
1.4.0, found:

*   Both answer `OK` to storing, placing with `X` and `h`, and placing again elsewhere, and placing
    again after `a=d,d=i` took it off.
*   **A screen clear (`CSI 2 J`) frees it.** In both, placing it after a clear answers `ENOENT`,
    whether it was placed or only stored. An image made with `U=1` for placeholders survives the
    same clear. ratatui clears the screen whenever the window changes size, inside its draw, so
    the program places the line after drawing each frame, and sends it again when the window's
    size has changed.
*   Placed directly, it's drawn over whatever is drawn over the page, which a placeholder image
    isn't, so the program takes it off while a list, the help, a guide, the shape panel or the
    key popup is open.

Whether it moves as smoothly as a cell background, and which looks better, is for the author to
see: `zc` switches between them.

**The ruler's marks placed directly** (decision 109). Each point's mark is a small triangle placed
the same way, so its tip is on its line's pixels rather than over the character the line is in.
There's an image for each kind of point, start, end, loop and slice, each placed once for each of
its points in view under a placement id of its own. In kitty 0.50.0 on 2026-11-10, run with
`--dump-commands` while `:slice 16` cut Ableton's `Breakbeat 134 bpm.aif`, kitty took every
placement, one image placed 16 times, and after the window changed size it took the four images
stored again with new ids and placed again. A scroll a column along places each mark in view
again, about 56 bytes a mark: 224 bytes for the 4 in view there at 512 frames a column.

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
*   Read on 2026-11-09: `graphics_unicode.zig` has the same 297 row and column marks as kitty's
    `rowcolumn-diacritics.txt`, in the same order. `graphics_image.zig` decompresses `o=z` before
    decoding a PNG, and reads shared memory where its limits allow it.

## Nerd Font symbols

The program needs the Nerd Font symbols, and uses them for icons and switches (decision 101). kitty
ships them as `Symbols Nerd Font Mono` and Ghostty embeds them, each as a fallback for the
codepoints the font in use doesn't have, so they show with any font and nothing needs setting up.
A font that is itself patched with Nerd Font symbols draws its own copies, which are the same icons.
A `symbol_map` in `kitty.conf` that sends those codepoints to a font without them, or a
`font-codepoint-map` in Ghostty's config that does, shows a box or a blank in their place. Each
glyph is a constant in `glyphs.rs` named after its Nerd Font name and codepoint, such as
`nf-fa-toggle_on`, U+F205.

The manual's screenshots name ordinary monospace fonts first, then `Symbols Nerd Font Mono` and
a few patched Nerd Fonts, which a browser falls back to a character at a time, so the icons show
where any of those is installed and are boxes elsewhere. The explorer's screenshot has them.

### Width

Every glyph is one cell to the terminal and to ratatui, so nothing after it moves. kitty and Ghostty
may draw an icon that's wider than a cell over a space after it, which makes it bigger and leaves
the text where it was. Every glyph outside the explorer has a space or an empty cell after it, so each
one draws the same way wherever it is, apart from the lock in the last FX column in Insert, which
the box's edge follows. kitty's `narrow_symbols` keeps a symbol to one cell.

**The explorer keeps its icons to one cell.** kitty and Ghostty both widen a Nerd Font
symbol over the cell after it when that cell is a plain space (or, in Ghostty, an en space),
so most icons drawn as `icon name` spill into the gap and come out larger than one beside a
letter. Neither widens one before a no-break space, U+00A0, so an icon is followed by
`glyphs::ICON_SPACE` instead, and each stays in its own cell, the same size as the rest.
Checked on 2026-11-09: kitty 0.50's own renderer (`kitty +runpy` with
`kitty.fonts.render.render_string`) put ink in the space after `nf-md-folder`, `nf-md-waveform`,
`nf-md-alert` and `nf-fa-toggle_on`, and none after a no-break space; narrow symbols such as
`nf-md-music_note` aren't widened at all. Ghostty's `constraintWidth` in
`src/renderer/cell.zig` allows two cells only before an empty cell, U+0020 or U+2002. The
grid itself never moves, as the terminal counts each symbol as one cell either way.

### Glyphs for the transport, tracks, LFOs, warnings and work (decision 104)

| Glyph | Name | Codepoint | Where |
|---|---|---|---|
| `󰐊` | `nf-md-play` | U+F040A | Status line, playing, in the active colour |
| `󰏤` | `nf-md-pause` | U+F03E4 | Status line, paused, dim |
| `󰓛` | `nf-md-stop` | U+F04DB | Status line, stopped, dim |
| `󰑊` | `nf-md-record` | U+F044A | Status line, recording, red |
| `󰔟` | `nf-md-timer_sand` | U+F051F | Status line, before `REALIGN` or `LAUNCH` while one waits for the grid |
| `󰀦` | `nf-md-alert` | U+F0026 | Right of the status line, in the warning colour, with the count of dropouts |
| `` to `` | Fira Code's spinner | U+EE06 to U+EE0B | Right of the status line, before `Analysing` or `Loading`, once work has taken 300 ms |
| `󰥛` | `nf-md-sine_wave` | U+F095B | After an LFO's `Sine` |
| `󱑼` | `nf-md-triangle_wave` | U+F147C | After an LFO's `Tri` |
| `󱑺` | `nf-md-sawtooth_wave` | U+F147A | After an LFO's `Ramp`, which rises as the glyph does |
| `󱑻` | `nf-md-square_wave` | U+F147B | After an LFO's `Square` |
| `󱑽` | `nf-md-waveform` | U+F147D | A sampler track's heading, and the machine list |
| `󰙽` | `nf-md-piano` | U+F067D | A synth track's heading, and the machine list |
| `` | `nf-fa-drum` | U+EE32 | A drum track's heading (`Kick`, `Snare`, `Hat`), and the machine list |
| `󱞳` | `nf-md-arrow_u_left_top` | U+F17B3 | A return's heading |
| `󰓃` | `nf-md-speaker` | U+F04C3 | The master's heading |
| `󰝟` | `nf-md-volume_mute` | U+F075F | A muted track's heading and its overview column, and the mixer's `Mute` |
| `󰋋` | `nf-md-headphones` | U+F02CB | A soloed track's, likewise, and the mixer's `Solo` |
| `󰌾` | `nf-md-lock` | U+F033E | A value's `Lock` mark in the phrase grid, in place of `.` |

### Glyphs for the explorer and windows (decisions 101 and 103)

| Glyph | Name | Codepoint | Where |
|---|---|---|---|
|   | `nf-fa-toggle_on`, `nf-fa-toggle_off` | U+F205, U+F204 | The explorer's `H`, `P`, `A` |
| 󰉋 󰝰 | `nf-md-folder`, `nf-md-folder_open` | U+F024B, U+F0770 | Explorer folders |
| 󱍙 | `nf-md-folder_music` | U+F1359 | Explorer projects |
| 󰕰 | `nf-md-view_grid` | U+F0570 | Explorer kits |
| 󱓻 | `nf-md-square_rounded` | U+F14FB | A kit's sound |
| 󰄶 | `nf-md-checkbox_multiple_blank` | U+F0136 | A kit's sound with variants or layers |
| 󱑽 | `nf-md-waveform` | U+F147D | A sample |
| 󰑖 | `nf-md-repeat` | U+F0456 | A loop |
| 󰀦 | `nf-md-alert` | U+F0026 | A file gone or unreadable |
| ╭ ╮ ╰ ╯ | Box drawings light arc | U+256D to U+2570 | Every window's corners |

The rounded corners are box-drawing characters, which both terminals draw themselves at the
cell's exact size, so they join the lines beside them; ratatui's `BorderType::Rounded` draws
them, through `views::window_border`.

### Beat and bar rules, and finer curves (decision 106)

The phrase grid marks where each beat and bar begins with a rule under the row before it, which
is an underline in a colour of its own: SGR 4 with SGR 58 for the colour, which crossterm writes
and kitty and Ghostty both draw. It takes no row and no column. `kitten @ get-text --ansi` on
kitty 0.50 showed `58:2:81:87:109;4` on the rows before a beat and `58:2:115:121:148;4` on the
row before a bar, and the grid's text where it was. The screenshots draw it as a one-pixel line
two pixels under the text.

**Octants and sextants.** Both terminals draw them themselves, as they do box drawing, checked on
2026-11-09 without installing anything:

*   kitty 0.50: `kitty +runpy` calling `fast_data_types.render_box_char` drew U+1FB00 and U+1FB3B
    (sextants) and U+1CD00, U+1CD10 and U+1CDE5 (octants) as the right blocks of a 2×3 and 2×4
    grid, and logged nothing, where it logs `Unknown box drawing character` for a letter.
*   Ghostty 1.4: `ghostty +show-face --config-default-files=false` said each of those, and
    `┃ ┅ ╍ ━ ─`, "is handled by Ghostty's internal sprites".

Nothing uses them. Every curve, the envelopes, the filter, the EQ, the compressor, the scope and
the waveforms, is already a kitty image, the track page's drawn on 6×12 dots a cell and the
sample page's at the screen's own pixels, so an octant's 2×4 would be coarser. The meters' eighth
blocks are finer up and down than an octant's quarters too.

## Still to check by hand

*   The `--when-focus-on` lines above on a real keyboard: that `Cmd-l` adjusts in vimtracker's
    window and still clears in another.
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
