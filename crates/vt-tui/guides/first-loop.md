# Your first loop

From an empty project to a loop you can hear, in about two minutes.

## 1. Load a sound

Type `:sample` for your samples as a tree of folders. `j` and `k` move, and each
sample plays as you reach it; `l` opens a folder, and `/` then `kick` searches the
ones you've opened. Press `Enter` on a WAV, AIFF or FLAC file: it goes in the
project's bank, and the current track plays it.

Not there? Run `:sample /path/to/folder` to browse that folder.

## 2. Write some notes

Press `Space 3` for the phrase view, then `i` for Insert. Press `a` to write a
C. The cursor stays on it, so another key plays and writes over it. Press
`Esc` to leave Insert.

For a kick on every beat, press `Right` after each note, which jumps a beat:

```
i a Right a Right a Right a Esc
```

Or press `;` in Insert for auto-jump, and each note moves on a beat by itself:
`i ; a a a a`. Press `;` again to turn it off.

## 3. Play it

Press `Tab` to play and `Tab` again to pause. `Shift-Tab` plays from the start,
and `Space c` from the cursor. You can keep editing while it plays, and an
edit is heard the next time the loop comes round.

## 4. Keep it

Press `:w` and `Enter` to save. Next, read **Navigation** and **Note entry**.
