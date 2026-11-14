# Phrases and chains

A **phrase** is a run of steps. A **chain** is the list of phrases one track
plays in order, each with its own transpose. Tracks can loop at different
lengths.

Each track has its own chains and phrases, numbered from `00` to `FF`. Chain
`03` on one track has nothing to do with chain `03` on another. To use a
part on another track, copy its steps with `y` and `p`.

## Tracks of different lengths

Each track loops at its own length, for as long as the song plays. A 3-step
hat against a 4-step kick drifts, and lines up again every 12 steps. Playing
from the top lines everything up.

In the phrase view, the notes beside your track are every track's own phrase,
in order, each named over its column on the row of headings, and each with its
own `>` in a cell of its own. Your track is among them, with its number marked
in green. Row `00` is each track's loop start, and a
column ends where its track ends. A note is dim and an empty row's `---` faint,
so where the other tracks play stands out.

The view is laid out in time. A line is one of your track's steps, so a row
lasting several pulses takes a line for each, the ones after its first dim, and
each column beside it shows the note its track starts in that line's time. A
slower track leaves lines blank, and where a faster one starts more than one
note in a line, the first shows with `⋮` before it. `j`, `k` and counts still
go by rows.

| Keys | Does |
|---|---|
| `z i` | Collapse the view to a line a row, or lay it out in time again |

Collapsed, the columns go by row number, so notes on one row don't always
sound together: follow the `>` marks to see how tracks line up.

To bring them back together on a schedule, set a reset:

| Command | Does |
|---|---|
| `:reset 64` | Send every track to its start every 64 steps |
| `:reset 0x40` | The same, in hex like the row numbers |
| `:reset off` | Let them run, the default |

The status line shows `Reset 64` while one is on. A track longer than the
reset never reaches its end. A single track can have a reset of its own: its
`Reset`, in the Sound page's Play device. You can change it while playing: the beat stays
put, and if the tracks are already past the new reset they go back on the
next step. LFOs keep running through a reset.

## The chain view

Press `Space 2`. The row past the end, shown as `--`, adds a phrase.

| Keys | Does |
|---|---|
| `j` `k` | Next or previous row |
| `gg` `G` | The first row, or the empty one at the end |
| `h` `l` | Phrase or Transpose column. `0` and `$` do the same |
| `gd` | Open this phrase, or add one on the empty row |
| `i` | Type a phrase number in hex or a transpose, then `Enter` |
| `=` `-` | Next or previous phrase, or transpose by a semitone |
| `C-a` `C-x` | The same as `=` `-` |
| `+` `_` | Phrase number by `10` in hex, or transpose by an octave |
| `D-l` `D-h` | Cmd and `l` `h`: as `=` `-`, without moving |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_`, without moving |
| `K` | Choose this row's phrase from a list |
| `x` | Transpose back to 0 |
| `dd` `yy` `p` | Remove a row, copy it, or add the copy below |

Typing a phrase number that doesn't exist creates that phrase. Transpose is
in semitones, up to 48 either way.

## Sharing and copying

A link, `󰌷`, after a phrase's or chain's number means it plays in more than
one place on this track, so an edit changes all of them. A `*` at the end of the
status line means only that there are unsaved changes.

| Keys | Does |
|---|---|
| `gc` | Copy this phrase, and use the copy here |
| `gC` | Copy this chain and all its phrases |
| `grr` | List where this track plays this phrase |
| `:bloom` | Grow branches from this phrase, each a mutation of the one before |

`:bloom` puts the path through the phrase and its new branches in this row's
place, so a chain of one phrase becomes a few bars that wander from it and come
back: see **Selecting and editing** for the paths and what a branch changes.

## Tracks

| Keys | Does |
|---|---|
| `W` `B`, `]b` `[b` | Next or previous track |
| `:chain 2` | Choose which of its chains this track plays |
| `:track add 4` | Add 4 tracks |
| `:track del` | Remove the last track, with its chains and phrases |
| `:tvol C0` | Set this track's level in hex, shown as `TVol` |
| `Space 5` to `Space 8` | The track page: sound, modulation, effects, generators |
| `Space m`, `:mute` | Mute or unmute this track |
| `Space s`, `:solo` | Solo or unsolo this track |
| `Space g m`, `Space g s` | The same, at the next line of the launch grid |
| `Space M` | Clear all mutes and solos |
| `Space e` | Give this track's faders back to its steps |
| `Space E` | The same for every track |

A tap of `m` or `s` switches the mute or solo and leaves it. Hold the key down
for longer than about a third of a second and it switches back when you let go,
for a drop while jamming. A save while the key is down keeps it as it was. A
muted track shows `󰝟` after its name, and a soloed one `󰋋`.

## The level scale

Levels work like Ableton's faders: even steps in dB, with room above 0. Track
level, `Trim`'s steps, the sends, `LEV`, `VOL`, `SNA` and `SNB` all use this scale,
and all are two hex digits, as a quarter of a dB is the scale's step: there's no
fine part, and a glide still sweeps smoothly between two values.

| Value | Level |
|---|---|
| `F8` | +6 dB, the top. Anything higher snaps to it |
| `F0` | +4 dB |
| `E0` | 0 dB, where tracks start |
| `D0` | -4 dB |
| `C0` | -8 dB |
| `80` | -24 dB |
| `40` | -40 dB |
| `00` | Silent |

Every `10` in hex is 4 dB, so `+` is 4 dB louder, and `4=` is 1 dB.

## Velocity and level

Velocity belongs to each note in a phrase. A track's level is separate, so you
can rebalance a track without touching a note. A phrase can still move it, as
an Ableton clip can: `LEV` sets the fader and `VOL` moves the sound up and down
from it; see **Fader automation and relative modulation** in **FX commands**.
The level also turns down what the track sends to the delay and reverb.

## Songs

A song is a list of sections, each choosing one of every track's chains.
Press `Space 1` for the song view, with a row per section. The other views edit
the section the cursor is on, and `Sec 2/4` on the status line says which.

| Keys | Does |
|---|---|
| `j` `k` | Next or previous section |
| `gg` `G` | The first or last section |
| `h` `l` | Previous or next column |
| `0` `$` | The length column, or the last track's |
| `=` `-` `+` `_` | Change the value. `C-a` and `C-x` are `=` `-` |
| `D-l` `D-h` | Cmd and `l` `h`: as `=` `-`, without moving |
| `D-k` `D-j` | Cmd and `k` `j`: as `+` `_`, without moving |
| `i` | Type a length or repeat then `Enter`, or a chain's two hex digits |
| `x` | Length follows the chains, Rpt is 1, Start is new, or no chain |
| `K` | On Start, switch new and reset. On a track, choose its chain |
| `gd` | Open this chain's phrase, or its rows if several, or add a chain |
| `gC` | Copy this chain and its phrases, for this cell alone |
| `a` | Add an empty section after this one |
| `gc` | Copy this section after itself, using the same chains |
| `dd` | Remove this section |
| `Enter` | While playing, play this chain on its track next, or stop it on `--` |
| `C` | While playing, add a section of what's playing, and stay on it |
| `;` | Hear its first row |
| `V` | Select sections, for `:loop`. `V` again or `Esc` ends it |

`z a` folds a group's tracks into its column, `z M` folds every group and `z R`
opens them: see **Folding a group** in **The track page, envelopes and LFOs**.

Sections play in order, and the song goes round to the first at the end. A
section lasts as long as its longest chain unless you set its length, which
cuts its chains short or lets them loop on. An empty section is skipped until
it has a chain or a length, and one with only a length is a rest. A `:reset` belongs to one section.

**Rpt** is how many times a section plays before the song moves on. Its chains keep
going across repeats unless Start is `reset`. A hold or `:loop` keeps it going
after its last repeat.

**Start** says what a section does to the tracks. With `new`, a track given a
different chain starts it from its top, and one given the same chain keeps going,
so a part can run across sections. Give the same chain to two sections of 32 steps
and a 64-step chain plays through once. `reset` sends every track back to its start
each time the section's length comes round, so each repeat is the same. At the end of the song, every chain
starts from its top, and a section looping on itself leaves its chains running.

`Shift-Tab` plays the song from the top. `Space c` in the song view plays from the
section, with each track where the song would have it. In a phrase or chain it
plays from that step of this track's chain, and the other tracks stand as they would
at that step of the section. `:loop` keeps playing the section being edited while you
work on it, and `:loop` again goes back to the song. A `:render` of a song covers
it once through, and `--stems` does too.

To loop several sections, press `V` on the first, move to the last, and `:loop`.
They play in turn, each from its start, and round again, so a verse and a chorus
can alternate for as long as you like. A bar down the left of the song view marks
the sections looping, and the status line says `Sec 2/4 loop 2-3`. `:loop` alone,
on one of them, goes back to the song: the section playing plays to its end, and
the song moves on from there.

While playing a section outside the new loop, the loop's first section starts on
the next line of the launch grid, as a launch does. Until then the bar is dim and
the status line shows `󰔟 LOOP`. So
`V` over the chorus and `:loop` jumps to it on the next bar, and `V` back over the
verse and chorus and `:loop` takes the song back round both.

## Jamming

While the song plays, `Enter` on any chain, on any row, plays it on that track
at the next line of the launch grid. `~` marks a chain waiting to start, `>` one
that's been launched, and the song stays on the section it's on. While one
waits, the status line shows `󰔟 LAUNCH`, as a realign waiting shows `󰔟 REALIGN`. A launch doesn't
change the song, and it ends when the song moves on or plays again. A chain still
waiting when the song moves on starts with the new section.

`:launch` sets the grid for the whole song, counted from the start of the section:

| Command | A launch starts |
|---|---|
| `:launch step` | On the next step |
| `:launch 1/4`, `1/2` | On the next beat, or half bar |
| `:launch 1bar`, `2bar`, `4bar`, `8bar` | On a bar, or so many |
| `:launch 12` | Every so many steps |
| `:launch chain` | When the track's own chain ends |
| `:launch` | Show which it is |

`1bar` is the default. A longer chain is cut at the line, and a shorter one loops
until it. Launching the chain already playing starts it again.

Tracks that have drifted apart can be brought back together on the same grid,
once, as a `:reset` would, without changing what they play:

| Keys | Does, while playing |
|---|---|
| `Space A` | Send every track back to its start at the next line |
| `Space a` | Start this track's chain again at the next line |

Press either again before the line to take it back. While one waits, the status
line shows `󰔟 REALIGN`. With `:launch chain` they wait for the next bar.

Mutes and solos can land on the grid too, for a drop or an entry on the bar:

| Keys | Does, while playing |
|---|---|
| `Space g m` | Mute or unmute this track at the next line |
| `Space g s` | Solo or unsolo this track at the next line |

Arm several, on as many tracks as you like, and they all switch together on the
same line. Press one again before the line to take it back, and `Space m`,
`Space s` or `Space M` switch at once, taking back any waiting. While one waits,
the status line shows `󰔟 MUTE` or `󰔟 SOLO`, and the track shows `󰔟` in place of
its mute or solo, in the phrase view's overview and on the mixer. With
`:launch chain` they wait for the next bar. Stopped, nothing is coming round to
wait for, so they switch at once.

`:label verse bass` names the chain under the cursor, in the song and chain views,
and `:label` alone clears it. Its number stays what rows refer to. The status line
shows the label of the chain under the cursor, and `K` searches labels as well as
numbers. Each chain's label stays with it wherever it's used.

When you like what you hear, `C` adds a section below with what each track is
playing, and carries on in it without a break. It keeps the length, Start and
`:reset` of the section it came from. A stopped track becomes `--`. Keep going to
build up an arrangement a row at a time. Tracks launched at different times can be
out of step, so a row can sound different played from its start.
