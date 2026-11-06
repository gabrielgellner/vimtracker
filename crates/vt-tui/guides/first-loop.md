# Your first loop

From an empty project to a loop you can hear, in about two minutes.

## 1. Load a sound

Press `Space 5` to list the WAV files nearby, type a few letters to filter, such
as `kick`, and press `Enter`. The sample goes on the current track.

Nothing listed? Run `:sample /path/to/folder` to browse a folder.

## 2. Write some notes

Press `Space 3` for the phrase view, then `i` for Insert. Press `a` to write a
C. The cursor moves down one step each time, so the note you just wrote is
above the cursor. Press `Esc` to leave Insert.

For a kick on every beat, use a bigger step, then press `a` four times:

```
:set step=4
i a a a a Esc
```

Run `:set step=1` afterwards to go back to one step at a time.

## 3. Play it

Press `Tab` to play and `Tab` again to pause. `Shift-Tab` plays from the start,
and `Space c` from the cursor. You can keep editing while it plays, and an
edit is heard the next time the loop comes round.

## 4. Keep it

Press `:w` and `Enter` to save. Next, read **Navigation** and **Note entry**.
