# Phrases and chains

A **phrase** is a run of steps. A **chain** is the list of phrases one track
plays in order, each with its own transpose. Tracks can loop at different
lengths.

## The chain view

Press `Space 2`. The row past the end, shown as `--`, adds a phrase.

| Keys | Does |
|---|---|
| `j` `k` | Next or previous row |
| `h` `l` | Phrase or Transpose column |
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
| `Space m` | Mute or unmute this track |
| `Space s` | Solo or unsolo this track |
| `Space M` | Clear all mutes and solos |
