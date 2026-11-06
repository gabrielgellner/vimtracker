# G7 result

G7: on the author's laptop, 16 active tracks run for 10 minutes while editing and redrawing, with a late-block count of zero, no stream errors, and nothing audible.

**Result, 2026-11-06: passed at 16 tracks.** The run wasn't timed, so the 10 minutes isn't confirmed.

| Measure | Result |
|---|---|
| Project | `g7-16`: 16 tracks, each with the ladder filter, drive, utility filter, delay and reverb sends, 2 LFOs and a filter envelope, playing notes on two of every three steps |
| Editing | The author changed the ladder filter and other settings while it played |
| Late blocks | 0 |
| Stream errors | 0 |
| Heard | No dropouts |
| CPU readout | 25-35% of each audio buffer |

**Not known:**

*   The most tracks the laptop sustains. `g7-32` and `g7-64` were made but not run live.
*   Why the live CPU figure is four or five times the 6% the same project takes to render offline (7.6 s for 120 s of audio). Live load should be planned from the readout, not from render times.

This project has no FX inserts or machines, which don't exist yet, so run this again at M9.
