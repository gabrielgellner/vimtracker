# Phrases and chains

A **phrase** is a run of steps. A **chain** is the list of phrases one track
plays in order, each with its own transpose. Tracks can loop at different
lengths.

## Tracks of different lengths

Each track loops at its own length, for as long as the song plays. A 3-step
hat against a 4-step kick drifts, and lines up again every 12 steps. Playing
from the top lines everything up.

In the phrase view, the notes beside your track are the other tracks' own
phrases, each with its own `>`. Row `00` is each track's loop start, and a
column ends where its track ends. Notes on one row don't always sound
together, so follow the `>` marks to see how tracks line up.

To bring them back together on a schedule, set a reset:

| Command | Does |
|---|---|
| `:reset 64` | Send every track to its start every 64 steps |
| `:reset off` | Let them run, the default |

The status line shows `Reset 64` while one is on. A track longer than the
reset never reaches its end. You can change it while playing: the beat stays
put, and if the tracks are already past the new reset they go back on the
next step.

## The chain view

Press `Space 2`. The row past the end, shown as `--`, adds a phrase.

| Keys | Does |
|---|---|
| `j` `k` | Next or previous row |
| `h` `l` | Phrase or Transpose column. `0` and `$` do the same |
| `gd` | Open this phrase, or add one on the empty row |
| `i` | Type a phrase number in hex or a transpose, then `Enter` |
| `+` `-` | Next or previous phrase, or transpose up or down |
| `dd` `yy` `p` | Remove a row, copy it, or add the copy below |

Typing a phrase number that doesn't exist creates that phrase. Transpose is
in semitones, up to 48 either way.

## Sharing and copying

A `*` means a phrase or chain plays in more than one place, so an edit changes
all of them.

| Keys | Does |
|---|---|
| `gc` | Copy this phrase, and use the copy here |
| `gC` | Copy this chain and all its phrases |
| `grr` | List where this phrase plays, in a phrase or chain view |

## Tracks

| Keys | Does |
|---|---|
| `W` `B` | Next or previous track |
| `:chain 2` | Choose which chain this track plays |
| `:track add 4` | Add 4 tracks |
| `:track del` | Remove the last track |
| `:tvol C0` | Set this track's level in hex, shown as `TVol` |
| `Space m` | Mute or unmute this track |
| `Space s` | Solo or unsolo this track |
| `Space M` | Clear all mutes and solos |

## Velocity and level

Velocity belongs to each note in a phrase. A track's level is separate and
stays out of its patterns, so you can rebalance a track without touching a
note. It also turns down what the track sends to the delay and reverb.
