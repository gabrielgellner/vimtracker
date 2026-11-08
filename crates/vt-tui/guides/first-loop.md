# Your first loop

From an empty project to a loop you can hear, in about two minutes.

## 1. Load a sound

Press `Space 5` to list the WAV files nearby, type a few letters to filter, such
as `kick`, and press `Enter`. The sample goes on the current track.

Nothing listed? Run `:sample /path/to/folder` to browse a folder.

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
