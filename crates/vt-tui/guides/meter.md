# The meter

The column on the right of the screen shows the main output, going into the
limiter. It hides on a very narrow terminal.

```
/\/\/\          the scope
Main            the title
  +3  2         highest recent peak in dB, and the most recently limited
L  R  G         left, right, and how much the limiter is taking off
```

## The scope

The small picture at the top of the column is the main output's waveform,
the last 20 ms of it, with left and right mixed. Its height is full level,
`0` on the meter, so a quiet sound draws small, and silence is a flat line
across the middle.

It starts each picture where the sound last rose through zero, as a
hardware scope does, so a steady note stands still rather than scrolling.
A chord or a noisy sound has more than one such place in each cycle, so it
may jump between them. It draws what comes out, after the limiter, and it
hides on a screen too short for it and the meter both.

## The L and R bars

| Part | Means |
|---|---|
| Solid colour | The average level, close to how loud it sounds |
| Dimmer colour above it | The peak, which jumps with each hit |
| A line above that | The highest recent peak, held for a moment |

The numbers are dBFS: `0` is the loudest the output can carry. Inside the
program a level can go past it without harm, and the scale runs up to `+6`
so you can see by how much. The number on top keeps counting if the bar runs
out of room. The colour goes from green to yellow at `-6`, and to orange at
`0`. Red means over.

## The G bar

G shows the limiter working: how many dB it is taking off right now. It
hangs down from the top, and its full height is `12`. It has its own scale,
so the numbers beside L and R don't apply to it.

The limiter is a safety, not part of the sound. Up to `0` on L and R it does
nothing at all, so what you hear is exactly what went in. Above `0` it clips
hard, and that is meant to sound bad.

So G should stay empty. Any limiting at all turns the number above it red.
If that happens, the fix is the mix: bring tracks down with `:tvol`, or lower
velocities, until the bars stay under `0`.

## Protecting your ears

The limiter stops levels going past full, but full is still very loud, and a
clipped sound can seem louder than a clean one. Your system volume is what
protects your ears, so keep it at a safe level.

## Turning it off

Run `:set limiter=off` for a render to keep levels over `0` in its float WAV,
so another program can turn them down. `:set limiter=on` puts it back. G then
reads `--`. It is saved with the project.

It doesn't change what you hear while playing: the output clips at `0` either
way, so nothing over full reaches your gear.

## The audio page

`Space 7` opens the audio page: which device plays, its rate and its buffer. `j` and
`k` choose a row, `=` and `-` step through its choices, as do Cmd with `l` and `h`, or
`k` and `j`, `K` lists them, and `x` goes back
to the system's own. A change takes effect at once: the transport stops, and the song,
undo, mutes and solos all stay. This machine remembers the choice for next time, in
`~/.config/vimtracker/audio.ron`.

The buffer is how much sound is worked out at a time. A smaller one answers keys sooner,
but leaves less room when the CPU is busy, so it drops out sooner. Lower it until `late`
or `xruns` start to count up, then go back a step. `Latency` is the time from working the
sound out to it leaving the device. On Bluetooth, most of it is the headset's own.

`CPU` at the top right is how much of each block's time the sound took to work out, so
how close it comes to dropping out. A dim `󰌪` before it means the system is running the
audio thread slowly to save power: on an efficiency core or a lower clock, which macOS
chooses for a light load at a large buffer, in Low Power Mode, or because the Mac is
hot. The same work then takes longer, so the figure reads higher than the song is
heavy. On the audio page, the line under the status line says by how much, and about
what the figure would be at full speed:

    󰌪 CPU slowed ×3.6 by power saving, about 8% at full speed

The system speeds the thread up as the work grows, so the leaf is nothing to worry
about while `late` and `xruns` stay at `0`.

Once audio has dropped out, the right of the status line shows a red `󰀦` and how many
times, `late` and `xruns` together, until the device is opened again.

If the device changes or goes away while playing, audio stops and the status line says
so. Choose one on the audio page to play again.
