# Mixer automation: `LEV`, faders that show what's heard, and touch override

**Status:** the design for #430, written before it was built, on 2026-11-11. Decisions 174
(automation, touch override and what's shown) and 175 (two digits or four) record it.

The author's request: phrases may automate the mixer, as an Ableton Live Session clip's envelopes
can. Most of it was there already (the issue's audit), so this is the gap and how it fits.

## 1. What's there, and the gap

*   **Marks** (decision 5, the FX guide): every step value has one. A **lock** lasts until the next
    note or the next value of its command; a **hold** lasts until playback next starts from the top,
    and a lock returns to it; a **glide** slides to the next value. Playing from the top puts every
    value back to the track page's.
*   **Absolute automation** of Pan and the sends works: `PAN`, `SNA` and `SNB` take all three marks.
*   **Relative automation** of the volume works: `VOL` is a stage on the level scale, apart from the
    fader and on top of it, as Live's clip modulation of volume is. The mod slots move `Vol`, `Pan`
    and the sends relatively too.
*   **The gap:** the fader itself, the track's `Level`, can't be set from a step.

## 2. `LEV`: the fader from a step

`LEV` is a track setting like `PAN`: key `level`, name `Level`, in `Output`, on the level scale
(`E0` is 0 dB, `F8` the top), with lock, hold and glide. It's where `Level` already was: the
engine's level stage after the slots, which also scales the sends. Its base is the track's `level`,
so a step's `LEV` replaces the fader and `VOL` stays a stage on top of it. Every track has it, the
master too.

**Modulation can't reach it.** A mod slot's `Dest` doesn't list `LEV`, as `VOL` is where modulation
moves the volume, relatively. Two ways to move one thing relatively would only blur which is which.

**"Keep the last value when the phrase stops"** is the hold mark, with no new rule (the author,
2026-11-11). A `LEV h` keeps its value until playback starts from the top again, whatever happens
meanwhile. "Stops" is three things, and each is the same, as each leaves the steps where they were:

*   **The transport stops:** a hold stays, and so does a lock, until a note ends it.
*   **The chain moves on** to a phrase with no automation: a hold stays; a lock ends at the next note,
    as every lock does.
*   **The track is muted:** its steps still run, silently, so its automation goes on underneath and
    is where it would be when it's unmuted.

Only playing from the top puts a fader back to its set value, as for every setting.

## 3. Faders show what's heard

**A fader** is the value's digits and its bar on the Sound page's Output device, the strip's value
and fader on the mixer, and the knob, its value and its motor on the Roto, in MIX mode and on the
Output device in PLUGIN mode. While a step's `LEV`, `PAN`, `SNA` or `SNB` drives it, a fader shows
the step's value, gliding as a glide does, not the track page's. It's what the hand then moves.

**Modulation is a thin mark, and the fader stays where it is.** `VOL` and the mod slots move what's
heard around the fader, so a fader shows them as a mark beside it, not by moving:

*   on the mixer, a one-row line in the fader's column, in the playing colour, at the level heard;
*   on the Sound page, the Output device's bars are filled to the fader, with the thin mark at what's
    heard (the other devices' bars are filled to what's heard, with the mark where it's set, as
    before: they aren't faders);
*   on the Roto, nothing: the knob and its motor are the fader.

**Why a mark, not the fader:** the fader is what a hand moves, and a touch takes it over (§4). If
modulation moved it, a motor would chase an LFO under a finger, the digits would flicker too fast to
read, and a touch couldn't take the modulation over anyway, as it's on top of the fader rather than
the fader itself. Live does the same: clip modulation leaves the fader where it is and shows how far
it moves things with a thin indicator. And the meter beside the fader already shows what's heard.

## 4. Touch override

**A hand takes a fader over.** Moving a fader by hand, by the keys, the wheel, a typed value, `x`
or a Roto knob, while a step's automation drives it, takes that one fader of that one track over from
the steps: from then the steps' values for it are left unheard and the track page's value is heard,
which the hand goes on setting. The other faders, and the other tracks, go on following their steps.
The keys and the wheel move it from where it is heard, not from where it was set, so it doesn't jump.

**"Drives it"** means a step has set it since playback last started from the top. A fader no step has
set is only an edit, as now, and taking over needs no setting up: it's like Live's, where changing an
automated control during playback overrides it.

**Marked:** a taken fader has `󰝁` (`nf-md-gesture_tap`, U+F0741) after its value on the mixer and
after its name on the Sound page, and the status line says so: `Level -6.0 dB, taken over from the
steps: Space e gives it back`. One the steps drive reads `-6.0 dB, from the steps`.

**Back to automation** is `Space e`, for every fader of the cursor's track, and `Space E` for every
track, as Live's Re-Enable Automation button gives back every control at once. `Space` leads the
playing keys, as `Space m` mutes, and the capital is wider, as `Space M` clears every mute and
`Space A` realigns every track (decision 172); `e` is Live's *re-Enable*. Given back, a fader is heard
at once where the steps have it now: a hold's value, a lock still on, or a glide's place, which the
engine kept following underneath.

**What lasts:** a takeover is a state of playing, kept in the engine as a mute is, not in the song: it
isn't an undo step, isn't saved, and isn't undone by undoing the hand's move. Playing from the top
leaves it, as in Live; opening a song gives every fader back.

## 5. The Roto's motors

MIX mode's knobs, and PLUGIN mode's knobs for the Output device, follow the faders: as the steps move
one, its knob's motor moves and its screen shows the value, at most 12 times a second, as the meters
are sent, and only for those that moved. A touched knob is never moved under the hand (§4.4 of
[`roto-control.md`](roto-control.md)), and turning one is a hand moving its fader, so it takes over.

## 6. Relative modulation needs no code

`VOL` and the mod slots' `Vol`, `Pan`, `Send A` and `Send B` are the relative mode, Live's clip
modulation: they move what's heard up and down from the fader, wherever it is, set by hand or by
`LEV`. The guide puts the two side by side: **fader automation** sets the fader (`LEV`, `PAN`,
`SNA`, `SNB`), and **relative modulation** moves around it (`VOL` and the mod slots).

## 7. Two digits or four (decision 175)

The author: "we want things as consistent as possible." A value on a fixed scale with natural steps
is two hex digits; a continuous position is four, its fine pair set back.

*   **Two:** everything on the level scale, a quarter of a dB a step: `Level` and `Trim` as before,
    `Send A` and `Send B`, the `VOL` stage, and the `LEV`, `VOL`, `SNA` and `SNB` locks, and an
    effect's level-scale `Gain` too. And every list, as before.
*   **Four:** continuous positions, `Pan`, `Cut`, `Res`, `PTCH` and the voices' settings, where a step
    between two coarse values can be heard.

The level scale's fine byte is dropped from what's stored, edited and shown: a lock's value is
`E0`, never `E080`, and a track keeps no fine part for its sends. The song format goes up to 10 for
it, with no migration. A glide still sweeps continuously between its two values in the engine.

## 8. What's saved

Only `LEV` locks, in the phrases as every lock is. Nothing about automation being followed or taken
over is saved.
