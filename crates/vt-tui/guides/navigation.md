# Navigation

These work in Normal mode. If `j` is writing notes, you are in Insert, so
press `Esc` first. The status line shows your mode, and the column you are
on beside it.

## Moving around a phrase

| Keys | Does |
|---|---|
| `j` `k` | Next or previous step |
| `h` `l` | Previous or next field |
| `w` `b` | Next or previous part: the note, then each FX slot |
| `0` `$` | Start or end of the row: the Note column, or the last FX value |
| `}` `{` | Next or previous beat, which is 4 steps |
| `gg` `G` | First or last step of the loop |
| `H` `M` `L` | Top, middle or bottom of the screen |
| `C-d` `C-u` | Half a screen down or up |
| `C-e` `C-y` | Scroll the view without moving the cursor |

## Counts

A number before a motion repeats it, as in Vim.

| Keys | Does |
|---|---|
| `4j` | Down 4 steps |
| `2}` | Down 2 beats |
| `8G` | To the 8th step of the loop, counting in decimal |

Step numbers on screen are hex, so step `10` on screen is the 17th step.

The fainter number left of each step is how far it is from the cursor, as in
Neovim's relative line numbers. It is decimal, so it is the count to type:
if a row shows `10`, then `10j` or `10k` lands on it.

## Moving between things

| Keys | Does |
|---|---|
| `W` `B`, `]b` `[b` | Next or previous track |
| `]p` `[p` | Next or previous phrase in the chain |
| `]P` `[P` | Last or first phrase in the chain |
| `Option-1` `Option-2` | Narrower or wider grid for the arrows in Insert |
| `Option-s` | Scale on or off. See **Note entry** |
| `Space 1` | Song view |
| `Space 2` | Chain view |
| `Space 3` | Phrase view |
| `Space 4` | Track page: filter, output, envelopes and LFOs |
| `Space 5` | Choose this track's sample |
| `Space 6` | Mixer |
| `Space 7` | Audio: the device, its rate and buffer |
| `Space 8` | Choose a kit |
| `Space 9` | Project: tempo, grooves, scale, saving and rendering |
| `Space Space` | Pick a view. `Enter` flips back to the last one |
| `Space fb` | Pick a track by number or name, like a buffer |
| `C-o` `C-i` | Jump back or forward, as in Vim's jump list |

## Lists

`K`, `Space Space`, `Space 5` and the others open the same list, in a box over the page,
with a search line above it. It opens on what's chosen now, marked with a `•`. Long
lists, such as `Dest` and the FX commands, keep their items in categories: dim headings with
a count, which open and close. The one with what's chosen in it starts open, and the
rest closed. The title shows how many items match, out of how many there are.

| Keys | Does |
|---|---|
| `j` `k` | Down or up. A count goes further, as in `3j` |
| `gg` `G` | The first row or the last. `5G` goes to the fifth |
| `C-n` `C-p` | Down or up, in the search too |
| `C-d` `C-u` | Half the list down or up, in the search too |
| `zz` `zt` `zb` | Scroll the cursor's row to the middle, top or bottom |
| `l` `Enter` | Open a category and go into it, or choose |
| `h` | Close the category, from inside it or on it |
| `/` | Search |
| `Esc` | Close, or leave the search for the list |

A long list scrolls before the cursor reaches its edge, keeping three rows beyond it in
view, as Vim's `scrolloff` does, so you can see what's coming. It stays where it is as the
cursor turns round, and `zt` and `zb` keep those three rows too. `zz` and `zt` can scroll
past the end of the list, as in Vim, so the last rows can sit in the middle or at the top.

Any other letter starts a search with itself: on Machine, `K` then `pl` finds `Plaits`.
A name that starts with one of the keys above, such as `ladder` or `zap`, needs `/` first. A
search looks through every category, ranks the best match first, shows each match's
category beside it, and picks out the letters it matched. Digits then `Enter` choose by
number, as for a chain or a phrase. The tracks and the FX commands open on their search
instead, as they're usually found by name. `Esc` goes from the search to the list, keeping
what it found so `j`, `k` and `zz` move through it, as `Esc` leaves Insert mode, and a second
`Esc` closes the list. Typing a letter in the list starts a new search.

## Files

Samples, kits and projects are chosen in the same list as a tree of folders: `Space 5`,
`Space 8`, and `:e`, `:w`, `:new`, `:sample` and `:render` with nothing after them. At
the top are the places it starts from, each with why it's there: `recent` for the last
few you chose from, then the project, your sample or kit folders, `here` for where the
program was run from, and `home`. It opens on the current file, with its folders open.
Only what fits is listed: samples and folders, kits and folders, or projects and
folders, and hidden files are left out. Choosing a sample, a kit opens like a folder to
its sounds.

Each row has an icon that says what it is. They are dim, the one already chosen is
green, and a warning is red.

| Icon | Is |
|---|---|
| 󰉋 󰝰 | A folder, closed or open |
| 󱍙 | A project |
| 󰕰 | A kit, as a grid of pads |
| 󱓻 | One of a kit's sounds, as a pad |
| 󰄶 | A kit's sound with variants or layers, which `l` opens |
| 󱑽 | A sample |
| 󰑖 | A loop: under a `Loops` folder, or with a tempo in its name |
| 󰀦 | A file that has gone, or won't read |

A sample is shown as a loop by its folders and name alone: the nearest folder named
for loops or one-shots decides, as in Ableton's library, and otherwise a tempo such as
`120 bpm` in its name. A file that won't read is found out when the cursor reaches it.

| Keys | Does |
|---|---|
| `l` | Open a folder and go into it, or choose |
| `h` | Close the folder, from inside it or on it |
| `Backspace` | Go up: the folder at the top becomes its parent |
| `H` | Show hidden files, or hide them |
| `P` | Show the preview, or hide it |
| `A` | Stop playing samples as you move, or start |
| `/` | Search what's been opened |

The hint under the list shows `H`, `P` and `A` each with a switch, green while it's on
and faint while it's off.

The numbers down the left count from the cursor, so `12j` can be read off them. A
search matches names exactly, ignoring case, among the folders you've opened, and
shows each match under its folders.

## The command line

`:` opens the command line at the bottom. `Tab` completes what you're typing, as in
Vim: command names, `:set` keys and their values, each command's own words, such as
`:launch 4bar` and `:track add`, kit names, and paths for `:e`, `:w`, `:new`,
`:sample`, `:render` and `:kit`. Paths go one folder at a time, and `~` is your home
folder.

| Keys | Does |
|---|---|
| `Tab` | Complete, or list the matches and fill in what they share |
| `Tab` `Shift-Tab` | Move through the list. Typing narrows it |
| `Enter` | Take the match from the list, or run the line |
| `Esc` | Close the list and keep what's typed, or leave the line |
| `Up` `Down` | Earlier commands, those starting with what is typed |
| `C-r` | A new random name, on a `:w`, `:new` or `:render` line |

The commands you run are kept for next time in `history.ron`, beside `settings.ron`,
the last 100, with one run again moved to the end.

## When you forget

Press `Space` or another prefix and wait: a popup lists what can come next.
`Space k` lists every key, and `Space ?` opens these guides.
