# Comparing against reference plugins

One spec renders through both our code, with `vimtracker probe`, and one or more reference
plugins, hosted by Spotify's [pedalboard](https://github.com/spotify/pedalboard), or Ableton
Live's own devices through a bridge, so a test is written once and rendered on every side (#186,
#364, decisions 110, 113, 125 and 133). Only the outputs are compared: nothing here looks inside a
plugin or a device. None of it runs in CI.

## Setting up

With [uv](https://docs.astral.sh/uv/) and a release build of vimtracker:

```sh
cargo build --release
cd tools/compare
uv sync          # makes .venv with pedalboard 0.9.26, numpy, scipy, mido and matplotlib
uv run run.py specs/filter-ladder.toml --report out/filter-ladder.html
uv run run.py specs/comp-static.toml --case ratio-4
```

`run.py` renders every case of a spec, or those named with `--case`, into `out/<spec>/` as
`<case>-ours.wav` and `<case>-<reference>.wav` for each reference, such as `<case>-acid-v.wav`,
and prints every side's peak, RMS and spectral centroid. A sweep's input adds the magnitude
response in octave bands, and stepped levels add the static curve. It checks the effect's input it
made against the probe's own, which should differ by 0. A spec with no reference renders ours
alone.

`--report page.html` writes one self-contained web page for the spec: what the test is, what each
line is, what differs most, every side plotted together by matplotlib, and the numbers in tables
(`report.py`). Every spec at once:

```sh
for spec in specs/*.toml; do uv run run.py "$spec" --report "out/$(basename "$spec" .toml).html"; done
```

The pages are about 0.1 to 2 MB each, so they stay in `out/`, which git ignores.

`run.py --suite NAME` runs every `specs/NAME-*.toml`, writes each one's report into `out/NAME/`,
and writes an `index.html` beside them linking each page and ranking our gaps from the references'
consensus. The suites in use are `nuance`, below, and `live`, Ableton Live's devices
([below](#ableton-lives-devices)), but any prefix works, such as `filter-refs` or `pedal`.

### The nuance suite

M27's specs, `specs/nuance-*.toml`, render the Acid against two references at once, Arturia's
Acid V and AudioRealism's ABL3 (#347, decision 125). One command runs them all:

```sh
uv run run.py --suite nuance     # about 5 minutes
```

It writes each spec's report into `out/nuance/`, with `index.html` beside them ranking our
biggest gaps from the references' consensus first, the measures the references disagree on, and a
link to each page, and the same as `consensus.json`. The specs are written by `nuance_specs.py`,
which reads decision 117's `acid-*` knob specs for the single knobs, so it's run again only to
change them.

Each side can be rendered alone, a reference by its name, or the first if none is given:

```sh
../../target/release/vimtracker probe specs/acid-cutoff.toml ours.wav --case cutoff-88
uv run render_reference.py specs/acid-cutoff.toml reference.wav --case cutoff-88
uv run render_reference.py specs/nuance-slides.toml abl3.wav --case up-fifth --reference ABL3
```

`probe --response drawn.csv` also writes the track filter's response as the Sound page draws it,
which `run.py` asks for with a sweep, so what's heard can be checked against what's drawn.

`check_plugin.py <plugin> instrument|effect` is the load check from #186, for trying a new
reference plugin.

### A song's line

`nuance-chirp-riff`, and `pedal-riff` and `live-pedal-riff` through it, play the Acid line of a
song of yours rather than one written here, as no 303 pattern's transcription is kept in the
repository (decision 143). The spec is a stub whose `[song]` names a project folder and the track
to read, by `machine` or `track` name. Each run, `song_line.py` has `vimtracker notes` read that
track's notes once through its chain, the song's tempo and the track's knobs and wave, with the
program's own model and without writing to the song, and writes the whole spec, notes and every
side's knobs, into `out/songs/`, where every side then reads it. The track's drive, filter and
effects are left out, so every side plays the voice alone.

The folder is `~/Music/vimtracker/projects/303-patterns/confusion-pump-panel` unless another is
given, by `--song` or by `VT_COMPARE_SONG`, the flag first:

```sh
uv run run.py specs/nuance-chirp-riff.toml --song ~/Music/vimtracker/projects/my-song
VT_COMPARE_SONG=~/Music/vimtracker/projects/my-song uv run run.py --suite nuance
../../target/release/vimtracker notes ~/Music/vimtracker/projects/my-song line.toml --machine acid
```

With no song in the folder, as on another machine, the spec and those whose input it renders are
skipped with a message, and a suite carries on without them. A track whose rows use FX, lanes,
nudges or a groove other than `Straight` is refused with the row named, rather than played
differently from the song. The song is read again on every run, so its numbers move as it's
edited.

## The specs

| Spec | Ours | Against | Report |
|---|---|---|---|
| `filter-clean` | the clean LP, HP, BP and notch | Filter M12 | sweep response at 3 cutoffs and Res 0, 50%, 90% |
| `filter-ladder` | Ladder, LP24 | Filter MINI | the same |
| `filter-ladder-level` | Ladder at Res 90% | Filter MINI | the same at five input levels |
| `filter-ours` | every other type and mode | the Sound page's drawing | the same |
| `filter-ringing` | Ladder at full Res | Filter M12, LP4P | pitch of its self-oscillation, in cents |
| `filter-ringing-ours` | Diode, MS20 LP and HP, Dirty | none | the same |
| `filter-taper-svf` | the clean LP, HP, BP and notch at 12 dB, Res in 33 steps | The Drop's OSR | the peak's height over the passband, or a band's width, its place, the bass and where it starts to ring, at 3 cutoffs (#333) |
| `filter-taper-ladder` | Ladder, LP24, the same | The Drop's PRD, 4 poles | the same |
| `filter-taper-ms20`, `filter-taper-ms20-ms2` | MS20 LP and HP, the same | The Drop's MS1, and MS2 | the same |
| `filter-taper-ours` | Diode and Dirty, the same | the Sound page's drawing | the same |
| `filter-refs-svf` | the clean LP, HP, BP and notch at 12 dB | The Drop's OSR and Filter SEM | sweep response at 3 cutoffs, at Res 0, half way to ringing and 95% of the way, written by `filter_refs.py` |
| `filter-refs-ladder` | Ladder, LP24 | The Drop's PRD and Filter MINI | the same |
| `filter-refs-ms20` | MS20 LP and HP | The Drop's MS1 and Filter MS-20 | the same |
| `acid-filter` | Acid's diode ladder | Acid V | a held note's harmonics |
| `acid-cutoff` | Acid's Cutoff, 16 steps | Acid V's | centroid and peak against the knob (#295) |
| `acid-cutoff-peak` | Acid's Cutoff at Res 90%, 17 steps | Acid V's | where the filter rings, its height and the bass |
| `acid-resonance` | Acid's Resonance at the top Cutoff, 17 steps | Acid V's | the same, so the peak's growth and the bass it thins |
| `acid-envmod` | Acid's Env Mod, 17 steps | Acid V's | centroid and peak against the knob |
| `acid-decay` | Acid's Decay, 17 steps | Acid V's | the time constant of the brightness's fall |
| `acid-accent`, `acid-accent-apart` | Acid's Accent, 17 steps | Acid V's | an accent's brightness and level over a plain note, and how accents in a row, or apart, build |
| `acid-velocity` | Acid at ten velocities, 10 to 127 | Acid V | each note's level, peak and brightness, which only an accent should change (decision 121) |
| `comp-static`, `comp-static-force` | the compressor | Comp VCA-65, Bus FORCE | static curve |
| `comp-timing`, `comp-timing-force` | the compressor | Comp VCA-65, Bus FORCE | attack, release, short and long bursts |
| `comp-bus-static` | the compressor's `Console` type at 2, 4 and 10:1 | Bus FORCE | static curve |
| `comp-bus-attack`, `comp-bus-release` | `Console`'s attacks and set releases | Bus FORCE | attack and release, short and long bursts |
| `comp-bus-auto` | `Console`'s Auto release | none | release after bursts from 20 ms to 2 s |
| `comp-bus-harmonics` | `Console` at stepped levels, 100 Hz and 1 kHz | Bus FORCE | THD and the 2nd and 3rd harmonics (`distortion`) |
| `nuance-knob-*` | decision 117's seven `acid-*` knob specs | Acid V and ABL3 | as the `acid-*` specs, with the consensus |
| `nuance-accents` | `AAAA`, `A.A.` and `AA..` twice through after a plain note, at Res 0, 50% and 90% | Acid V and ABL3 | each note's brightness and level over the plain note's, the first accent's and the build-up (`notes`) |
| `nuance-slides` | slides up and down by a semitone, a fifth and an octave | Acid V and ABL3 | the pitch over time, the glide's times, the dip and the brightening (`transition`) |
| `nuance-legato` | two notes 60 ms apart to 20 ms overlapping, and slid | Acid V and ABL3 | the same, for where legato turns to a retrigger |
| `nuance-gate` | one note held from a 128th to a quarter | Acid V and ABL3 | its level as it's held and its release (`gate`) |
| `nuance-extremes` | every knob at its bottom or top, 32 ways, two plain notes and two accents | Acid V and ABL3 | as `nuance-accents` |
| `nuance-chirp` | one plain or accented C-1 at Res 75, 85, 95 and 100%, Env Mod 50 and 100%, Cutoff low and mid, Decay short and long, 64 ways | Acid V and ABL3 | the resonant peak over time: its place, its height over the fundamental and the passband, the skirt an octave over it, and the note's start (`chirp`) |
| `nuance-chirp-riff` | a song's Acid line, read from the song as it runs, once through ([A song's line](#a-songs-line)) | Acid V and ABL3 | the same, note by note along the line |
| `pedal-distortion-harmonics` | Distort's `Distortion` at `Amount` `40`, `80` and `FF`, stepped levels at 110 Hz and 1 kHz | Dist OPAMP-21, `Modern`, at 25, 50 and 100% `Drive` | THD and the 2nd and 3rd harmonics (`distortion`) |
| `pedal-distortion-response` | `Distortion` given a sweep too quiet to clip, `Tone` open and at its default | Dist OPAMP-21, `Modern` and `Normal` | the EQ around the clip (`response`) |
| `pedal-riff` | `nuance-chirp-riff`'s song line as our Acid plays it, through `Distortion` at three drives | Dist OPAMP-21, the same samples in | the long-term spectrum's shape, level, crest factor and level over time (`spectrum`) |
| `pedal-overdrive` | Distort's `Overdrive` at stepped levels at 110 Hz and 1 kHz, four drives | none, as no reference is a Tube Screamer | THD and the 2nd and 3rd harmonics (`distortion`) |
| `pedal-acid-v` | our Acid's held, open note through `Overdrive` and `Distortion` | Acid V through its own `Overdrive` and `Distorsion` | a held note's harmonics (`harmonics`), against each side's own note with no distortion |
| `live-glue-static` | the compressor's `Glue` type at 2, 4 and 10:1 | Live's Glue Compressor | static curve |
| `live-glue-attack` | `Glue`'s seven attacks, and 10 ms at 2:1 and 10:1 | Glue Compressor | attack, as `comp-bus-attack` |
| `live-glue-release` | `Glue`'s six set releases and its Auto | Glue Compressor | release after short and long bursts |
| `live-glue-range` | `Glue`'s `Range` at 3, 6, 12, 20 and 40 dB, at 10:1 | Glue Compressor | static curve |
| `live-glue-detector` | `Glue` at 4:1 from a 100 Hz tone | Glue Compressor | static curve, unfitted |
| `live-glue-clip` | `Glue`'s `Peak Clip` on and off, a 1 kHz sine stepped from -12 to +12 dBFS, with the output 12 dB up and at half dry, and a 2311 Hz sine for its aliases (#338) | Glue Compressor's `Peak Clip In` | THD and the 2nd and 3rd harmonics (`distortion`), which `Peak Clip` is fitted to |
| `live-pedal-overdrive` | Distort's `Overdrive`, stepped levels at 110 Hz and 1 kHz | Live's Pedal, Overdrive | THD and the 2nd and 3rd harmonics (`distortion`) |
| `live-pedal-distortion` | Distort's `Distortion`, the same | Pedal, Distortion, and Dist OPAMP-21 | the same |
| `live-pedal-riff` | the same song line through `Overdrive` and `Distortion` | Pedal in both modes | the long-term spectrum and dynamics (`spectrum`) |

The `filter-taper-*` specs are written by `taper_specs.py`, which sets our Res against Cytomic's
The Drop's: our knob's first 85%, `00` to `D9`, against its 0 to 100%, and the rest against its
100 to 125%, where it self-oscillates. Their `taper` report reads each step's sweep for how far the
peak stands over the passband three octaves away, and whether the filter is ringing by itself near
Cut as the sweep ends, two octaves and more past it.

### The filter references

`filter_refs.py` checks each of the track filter's circuits against two independent references,
the circuit of The Drop it's paired with and an Arturia filter modelled on the same one, and takes
where they agree as the target, as the nuance suite does for the Acid (decision 137):

```sh
uv run filter_refs.py                    # every circuit, about 2 minutes once the references are kept
uv run filter_refs.py --circuit ms20     # one: svf, obx, ladder or ms20
uv run filter_refs.py --write-specs      # and write specs/filter-refs-*.toml from the same settings
```

| Ours | The Drop | Arturia |
|---|---|---|
| `LP`, `HP`, `BP`, `Notch` at 12 dB | OSR, two poles | Filter SEM |
| `OB-X` in `LP12`, `HP12`, `BP12`, `N12` | OSR, two poles | Filter SEM |
| `Ladder`, `LP24` | PRD, four poles | Filter MINI |
| `MS20 LP`, `MS20 HP` | MS1, two poles | Filter MS-20 |

It maps each side's knobs by measuring: The Drop, Filter SEM and Filter MS-20 have their cutoff in
hertz, and Filter MINI's knob is set where, with no resonance, it passes 12 dB under its passband,
as a four-pole ladder's four equal poles do at its cutoff. Each Res knob is turned in 33 steps at
200 Hz, 988 Hz and 5 kHz, our first 85% against each reference's turn up to where it starts to
ring, which it measures for the Arturia filters and which is The Drop's 100%. Filter SEM never
rings, so its whole turn is set against our 85%. Then, from quiet sweeps, a click and a stepped
sine: the slope's shape at Res 0, the peak's height along the way to ringing and the passband it
takes, the peak's place, width and fall at the same height of peak on every side, its ringing's
pitch and level at six cutoffs, and the harmonic distortion at stepped levels. The page,
`out/filter-refs/index.html`, and `consensus.json` have each measure's consensus, as
`consensus.py` gives it, and plots of the peak and the passband along Res.

Reference renders are kept in `out/filter-refs/cache/`, so a run after a change to our code
compares with the same reference samples, and `--fresh` renders them again; ours are kept by the
binary's time and size. Each Arturia filter starts from its `Default` preset, its state given
twice, with everything that isn't the bare filter switched off, as its spec says. Found by
measuring, on 2026-11-10:

- **Filter MINI**'s `limit_resonance` does the opposite of its name: off, as it loads, the
  resonance stops growing at 0.74 and never rings; on, it rings from 0.75. Its input's coupling
  takes 2.3 dB at 20 Hz, so its response with the knob open is taken out under 1 kHz.
- **Filter SEM** never self-oscillates, at any resonance, as an SEM doesn't. Its input's coupling
  takes 3.2 dB at 20 Hz, taken out the same way. Its mode knob is low-pass at 0, a notch at 0.5,
  half the low-pass and half the high-pass, 6 dB down, and high-pass at 1.
- **Filter MS-20** takes a new setting only at its next render: its cutoff stays where the last
  case left it for a whole render, and a pre-roll doesn't help, so a spec gives it `settle = true`
  and `render_reference.py` plays it a buffer of silence with each case's settings first. Its
  distortion's filters dull the top by 10 dB at 16 kHz even at no drive, so its dry/wet is at 0.
  Its `Clean` mode is linear until it clips at 0 dBFS, and `Dirty` clips everything over
  −12 dBFS. Its high-pass and low-pass are in series, so the one not measured is left open, and
  that open response, which lifts the bass by up to 4.6 dB around 60 Hz, is taken out of every
  other. Its `limit_resonance` stops it ringing, as its name says, so it's off.

The Acid's filter is inside the voice, where the probe can't feed it a sweep, so `acid-filter`
reads its shape from a held sawtooth's harmonics on both sides. The track filter's `Diode` type is
the same ladder given a sweep, in `filter-ours`.

## The plugins

Reference plugins are read where they're installed, such as `/Library/Audio/Plug-Ins/VST3`, and
nothing here writes in their folders or their preferences.

**Buffers of 512.** Arturia's plugins put out silence at pedalboard's default buffer of 8192
frames, so every plugin is played in a host's buffer size: `render_reference.py` and
`check_plugin.py` use 512 frames unless a spec's `buffer_size` says otherwise, and the specs that
`nuance_specs.py` and `filter_refs.py` write give 512 themselves. Keep an Arturia plugin at 512.

Plugins write their own files as they
load, which is theirs to do: Arturia's update `/Library/Arturia/Presets/db.db3`, the preset tags
beside it and `/Library/Arturia/<plugin>/tmp/plugin.pref.xml`, and print their logging settings
(`## LogInRelease : 0` and so on); u-he's write a cache.

Acid V accents a note from velocity 100 and ours from `0x70`, 112, so the Acid specs play a plain
note at 99 and an accent at `0x70`, which both sides read the same way. It slides into a note that
overlaps the last by 1 ms or more, and plays one that only touches it, or follows a gap, afresh
(`take.py probe`, [below](#performance-takes)). A reference's
`accent_from` gives its threshold, so a note's `accent = true` reaches each side at or over its
own, and `accent = false` under all of them. They also turn Acid V's hiss
off, `noise_db = -100`, which would otherwise brighten a quiet note's centroid.

A `knob` report's `measure` says what each step is measured by: `note`, the whole render's centroid
and peak; `filter`, a held note's harmonics read as `harmonics` reads them; `decay`, an exponential
fitted to the note's brightness; or `accents`, each note of a pattern of a plain note and then
accents. The summary gives how far apart the two curves are on average over the knob.

A `notes` report plays a pattern in each case, a plain note first, and reads each note's
brightness and level as `accents` does, against the first note's. A `transition` report plays
two notes in each case and reads the change between them: the pitch over time, by YIN, the time
a held note takes to glide 90% of the way and from 10 to 90%, the level's dip, and how much
brighter the second note starts than the first ended, which a retriggered envelope raises. A
`gate` report holds one note for a different length in each case and reads its level at its end
and its release, 20 and 40 dB down. Their levels are the RMS over one cycle of the note, which a
wave's phase doesn't move.

A `chirp` report follows each note's resonant peak as the filter envelope sweeps it down, the
303's squelch at high Res (#348, #349). Every 2 ms it reads two cycles of the note through a Hann
window, a length that puts each harmonic on its neighbours' zeros, so none leaks into the next,
and takes each harmonic times its number, so a sawtooth's are level and what's left is the
filter's shape (`measure.resonance_track`). The peak is the loudest harmonic above the
fundamental, read between harmonics by a parabola in dB. Its height still wavers by about a dB as
it crosses the harmonics, so each measure is a median over a window: the peak's place at 20, 60
and 150 ms, how far it falls from 20 to 120 ms, its height over the fundamental at those times,
its height over the harmonics an octave and more under it, the level an octave over it, which
says how the filter falls past its resonance, and the note's first 20 ms over the 40 after. The
harmonics are read at each side's own tuning, the layout's `tuning` in cents, as the sweeping
filter's phase moves the pitch a tracker reads: on held notes with nothing sweeping, ours and
Acid V play MIDI 36 at 65.41 Hz and ABL3 at 65.55, 3.8 cents sharp.

A `spectrum` report puts a whole passage through an effect on every side, such as a riff through
a pedal (#361), and reads each side as a whole (`measure.passage`): its long-term spectrum in
third-octave bands, each band's power against the side's own total, so the shape is compared with
the levels taken out; its RMS, peak and crest factor; and its level over 10 ms windows, whose 10th
and 90th percentiles, over the windows within 40 dB of the loudest, are its quiet and loud
moments. Against a reference it gives how far apart the shapes are, as the RMS of their
difference over the bands from 40 Hz to 16 kHz, and the levels over time once their overall
difference is taken out. Its input is usually a `file` that `made_by` renders first: our side of
another spec's case, such as `nuance-chirp-riff`'s song line, written where the input reads it, so
every side is given the same samples and only the effect differs.

The Arturia distortions are started from their `Default` preset as Acid V is. Dist OPAMP-21, after
Tech 21's SansAmp, is compared in its `Modern` mode, which Arturia describes as having no colouring
or EQ, with its post EQ off, so its clip is heard alone, and in its `Normal` mode, the SansAmp's own
voicing, for its EQ. Its output falls back from an offset over about 120 ms once a note stops, as
a coupling capacitor of about 1.3 Hz would.

### The consensus

A report whose kind gives its measurements, `knob`, `notes`, `transition` and `gate`, has a
consensus view (`consensus.py`). For each measure and case: where the references agree within a
tolerance, their mean is the target, and our gap is ours less it; where they disagree, both are
shown and the measure is flagged. Hz and ms are compared by their ratio, in octaves, and the rest
by their difference. The tolerances, two independent emulations of one circuit being only so
close: 1/6 octave (two semitones) for Hz, a quarter of an octave (about 19%) for ms, 1 dB, 0.1
octaves of brightness, a quarter of a semitone, and 25 cents. A gap's size over its tolerance
ranks gaps in different units against each other, on the report and across a suite's index,
where a measure the references agree on in fewer than a quarter of its cases, or only one, comes
after the ranked ones, as what they agree on there is too thin to stand for the measure.
Levels each side sets for itself, such as a knob sweep's peak, are compared against the same
side's at the knob's bottom. With one reference, it's the target alone.

A `velocity` report has no cases: one render plays notes at different velocities, far enough apart
for each to start from rest. Each note's RMS over its first 60 ms and its peak over its first
250 ms are read against the first note's, with its brightness over its first 40 ms.

### ABL3

AudioRealism's ABL3 3.3.5 (`/Library/Audio/Plug-Ins/VST3/ABL3x.vst3`) is the second Acid
reference, set up as a stock 303. What it does, measured on 2026-11-10 from its output alone,
with its manual's word for its settings:

- **Its MIDI mode** is in its state, not a parameter: `MidiMode = 0`, the Pattern mode it loads
  in, plays one of its 128 patterns for each key, transposed, on the host's clock, so a note's
  length and velocity change nothing. `MidiMode = 1` is its Note mode, where MIDI notes play it.
  `SyncMode` changed nothing heard. The state is JUCE's XML, whose outermost element's
  attributes a spec's `state` sets by name (`plugin_state.py`).
- **It wakes on a middle C:** after a reset, in Note mode, it stays silent until it's played MIDI
  60. Every note after one plays, at any pitch; before one, none does. And for the first 90 ms
  after a reset it puts out a burst of its own, about −25 dBFS. So it renders after a second's
  pre-roll that starts with a tenth of a second's middle C, `preroll = 1.0` and `wake_note = 60`,
  cut off before anything's measured; the pre-roll is rounded up to whole buffers so its buffers
  fall where they would without it.
- **Its output is mono,** `channels = 1`; pedalboard refuses two.
- **Tuning:** it plays MIDI 36 at 65.5 Hz and 48 at 131.0, as ours does, so it needs no
  transposing, where Acid V needs `osc_pitch_st = 12`. Its `tuning` stays at 0.
- **Accent:** a note is accented from velocity 101: a sweep from 1 to 127 found 100 and under all
  the same, and 101 and over all accented, about 9 dB louder and 1.5 octaves brighter at full
  Accent. The manual says both "100 or greater" and "over 100"; the output says over. So its
  `accent_from = 101`, and plain notes at 99 and accents at `0x70` mean the same on all three
  sides.
- **Slide:** notes that overlap slide, from 1 ms of overlap, with the gate held: a fifth's glide
  reaches 90% in about 50 ms. A note starting just as the last ends, or after it, is played afresh.
  The spec's `slide = true` holds a note 1 ms into the next, so it slides on all three sides.
- **MIDI timing** is to the sample: a note's onset moved with its time within a buffer.
- **Stock settings:** its hardware mods' trims, `reso_trim`, `gate_trim`, `vcf_trim`,
  `bass_boost`, `treble_boost`, `detune` and `vco_base`, at the 50% they load at, which the manual
  says is matched to a TB-303, and `pregain` at its 0; no `drive`, `distort` or `highpass`;
  `noise_level`, the VCA's noise, and `vco_shift`, the VCO's flutter, at 0, as Acid V's hiss is
  off; and in its state `VCAClicks = 0`, an extra click in the attack the manual says is there to
  help it cut through, which a 303 hasn't (2 dB on a note's first 5 ms), and `OutputClipping = 0`,
  as our limiter is off, though nothing measured reached it.
- It loads in about a tenth of a second and renders the same samples every time.

An Arturia plugin opens with the preset it last had. Acid V opened with "Carbon Based", a sequence
whose sequencer plays its own pattern from a held key, so a spec can start a plugin from a factory
preset, read from `/Library/Arturia/Presets/<plugin>/`, with `preset = "Default"`, and change
settings that aren't parameters with `state`, such as `state = { Sequencer_OnOff = 0 }`
(`arturia.py`). These are the plugin's saved settings, as a host keeps them in a project, changed
by name. Acid V takes them only when given them twice, which `render_reference.py` does. A
reference that takes a new setting only at its next render, as Filter MS-20 does, is given
`settle = true`, and played a buffer of silence with each case's settings before it renders.

## Ableton Live's devices

Live's built-in devices aren't plugins, so pedalboard can't host them. A bridge renders through
them instead (#364): a control surface script running inside Live 12, `live/VimtrackerBridge`,
which takes requests from the harness on `127.0.0.1:47364` and nowhere else, and
`live_bridge.py` on this side. Rendering is real time and needs Live open; each render is cached in
`out/live-cache/`, so a spec runs again without Live, and `run.py --fresh-live` renders anew.

A spec names a Live device by the name Live shows, and sets its parameters by Live's names, with
values as Live shows them. A number can carry its unit or not (`-20` means what Live shows as
`-20.0 dB`), a list's item is named (`"Overdrive"`, or `"A"` for Glue's Auto release), a switch is
`true` or `false`, and `"min"`, `"max"` and `"default"` mean what they say. The bridge loads the
device fresh for every case, so whatever a spec leaves out is where Live loads it.

```toml
[[references]]
name = "Glue"
live = "Glue Compressor"
marker_gap = 1.0            # optional: seconds between the marker and the input
tail = 1.0                  # optional: seconds of silence after the input, to ring out
[references.parameters]
Threshold = "-20 dB"
Ratio = 4
Attack = "10 ms"
Release = "A"
"Dry/Wet" = "100 %"
```

A case changes them under `references` or `reference`, as for a plugin. A Live device takes an
input; Live's instruments aren't bridged.

### How it renders

The bridge keeps two audio tracks in the open set, and makes them if they aren't there:
`vt bridge source`, which plays the test file as an unwarped clip through the device under test,
its only device, and `vt bridge sink`, armed, monitoring off, recording `vt bridge source` at
`Post FX`, after its devices and before its fader and pan. For each case `live_bridge.py`:

1. writes what Live plays into `out/live-cache/`: a quarter second of silence, a marker of one
   sample at -30 dBFS, a second of silence, the input, and a second more of silence;
2. asks the bridge to load the device on the source track, alone and fresh (`Track.insert_device`,
   Live 12.3 and later, or the browser before that), and to set each parameter. A continuous
   parameter is set by searching its range with `str_for_value` for the values that show what's
   asked and taking the middle of them; a list's by its items. What Live then shows is sent back
   and checked against the spec, to the shown value's rounding, so a render where Glue's Threshold
   showed `-19.5 dB` is refused rather than compared;
3. asks it to render: it puts the file in the source track's first clip slot
   (`ClipSlot.create_audio_clip`), turns its warping and looping off, sets launch quantization to
   none, fires the sink's empty slot, which records, and the source's, waits the file's length and
   half a second, stops the transport, and answers with the recorded clip's `file_path` once Live
   has finished writing it;
4. reads the recording, refusing it if it isn't at the spec's rate, finds the marker, the loudest
   sample before the input could arrive, and cuts the recording from the input's start plus that
   lag. The lag is Live's delay from playing to recording and the device's latency, which Live
   reports as `latency_in_samples` and is kept beside the render.

The marker is quiet so a compressor or a pedal passes it as it is, and the second of silence lets
a compressor recover before the input. A device with a linear-phase filter puts its impulse's peak
at its latency, and a minimum-phase one a few samples after its start, so the lag can be a few
samples long for a device that filters; ours isn't moved.

`uv run live_bridge.py null` checks the path: a sweep at -6 dBFS through the probe with no device,
which should come back at the same level, within 0.01 dB, and the same to under -90 dBFS once
lined up. The other commands are `ping` and `params <device>`, which lists a device's parameters,
their ranges and items, for writing a spec.

The specs: `live-glue-static`, `live-glue-attack`, `live-glue-release`, `live-glue-range` and
`live-glue-detector` set our `Glue` against Glue Compressor, which it's modelled on and fitted to;
`Console`, fitted to Bus FORCE, is measured in the `comp-bus-*` specs. `live-pedal-overdrive`,
`live-pedal-distortion` and `live-pedal-riff` set Distort's `Overdrive` and `Distortion` against
Pedal's two modes, its Drive at the same place as our `Amount`, unfitted, with Dist OPAMP-21
beside `Distortion`. `uv run run.py --suite live` runs them all, about 6 minutes of Live's time.

### Setting it up

```sh
just live-bridge-install        # or tools/compare/live/install.sh
just live-bridge-uninstall      # takes it away again
```

The install makes one thing, `~/Music/Ableton/User Library/Remote Scripts/VimtrackerBridge`, a
symbolic link to `tools/compare/live/VimtrackerBridge`, and the `Remote Scripts` folder if there
isn't one; `ABLETON_USER_LIBRARY` names another library. `install.sh --copy` copies the script
there instead, marked as the bridge's, for a Live that won't follow the link, and `install.sh
status` says what's there. Uninstalling removes only the link or the marked copy. Live writes its
compiled `__pycache__` beside the script, which git ignores.

Then restart Live, open a new set, and in Settings, Link, Tempo & MIDI choose `VimtrackerBridge`
as a Control Surface, with no input or output; Live's status bar says `vimtracker bridge listening
on 127.0.0.1:47364`. Set Live's sample rate to the specs' 48000 Hz in Settings, Audio, and its
recordings to 32-bit in Settings, Record, Warp & Launch, so nothing is clipped or rounded. Live's
log, `~/Library/Preferences/Ableton/Live 12.*/Log.txt`, has the bridge's lines.

The tests, which need no Live, drive the bridge with stand-ins for Live's objects:

```sh
uv run python -m unittest discover tests
```

### If Live won't do it this way

The route kept in reserve is Max for Live: a source device playing the file with `sfplay~` before
the device under test and a sink recording with `sfrecord~` after it, on one track, commanded over
`udpreceive`, so the recording is written by Max as 32-bit float and doesn't depend on Live's
session recording. The `.amxd` files would have to be generated from a readable patcher in the
repo, as Max can't be opened here, which is why Live's own clip and recording API came first.

## Performance takes

A knob swept on its own never meets the others, and a riff played with the knobs moving does
(#565, after #348 and #349). So the author plays Acid V in Live, recording the notes and every
knob move into a clip, and `take.py` reads the clip through the bridge, plays the same riff on our
Acid with the same moves, and compares the two over time:

```sh
just take export riff          # the clip called riff: out/takes/riff/take.json
just take render-live riff     # Acid V playing it in Live, silently: acid-v-live.wav
just take probe-live "Acid V"  # how Acid V on that track takes accents and slides, by output
just take compare riff         # our song, both sides rendered, and report.md
```

`just take` builds vimtracker and the song builder, then runs `uv run take.py` here; the first
three need Live open with the bridge, and `compare` doesn't. It writes into `out/takes/riff/`:

- `take.json`: the clip's notes in beats, with their velocities and lengths, so overlaps show;
  every parameter of Acid V as it is; and each envelope the clip has, read every 64th of a beat
  over one pass of the clip, from its start marker to its loop's end;
- `ours/`, a project `vimtracker` opens, built by `crates/vt-tui/examples/take_song.rs` with the
  program's own model from `rows.toml`, which `take_rows.py` lays out: the notes on an `Acid`
  track, nudged off the grid as played, held for their lengths, accents at `7F` and slides as
  `HLD`; and the knobs that moved in FX lanes, one value a step, gliding from each to the next;
- `ours.wav`; `acid-v.wav`, Acid V hosted here playing the take's MIDI with its knobs moved along
  the envelopes a buffer at a time (`take_replay.py`); and `acid-v-live.wav`, Live's own;
- `report.md`, with `overview.png`, `spectrograms.png`, `balance.png` and `harmonics.png`, and
  `measures.json` (`take_report.py`).

The report reads each note on both sides where the take has it: its level over its first 60 ms
and its brightness over its first 40, as the `notes` reports do, its brightness over the whole
note, and its first eight harmonics every 5 ms. It gives ours less Acid V's, with the take's
overall level difference taken out, then the gaps by each knob's thirds of the turn the take
used, by accent and by slide, and the notes we drift most on with the knobs they were played
with. Over the whole take: the spectrograms side by side and their difference, the long-term
balance in third octaves, the levels over time, and how long one side's release rings on after
the other's. It checks accent and slide by output too: how much louder and brighter the accented
notes are on each side, and how far each slid-into note's pitch still has to go 5 to 25 ms in.
With Live's render there, that's the side compared, and the replay is checked against it first,
which says whether the export caught what Live played; without it, the replay is compared.

`just take synthetic NAME` writes a take of its own, four bars at 126 BPM with accents, slides,
notes played a little off the grid and three knobs moving, so everything after `export` runs
without Live: `just take synthetic try && just take compare try`, about 20 seconds.

**Acid V's knobs to ours.** Decision 117 fitted our `Cutoff`, `Resonance`, `Env mod`, `Decay` and
`Accent` to Acid V's so the same share of a knob's turn sounds the same, as `nuance_specs.py` sets
them, so a take's knob at 0.5 is our `80`, with the fine part for what's between. A knob with no
envelope stays where it was on both sides. Acid V sounds an octave under the note Live sends at
its `Osc Pitch` of 0, so ours plays each note 12 semitones down, and `Osc Pitch` moves that.
What ours can't follow is listed at the report's top: a knob that isn't one of the five, its
front panel's `Brightness`, `Timbre`, `Time` and `Movement`, which pedalboard finds independent of
the knobs they sit over, and its distortion, sub oscillator or hiss turned up.

**Accent and slide from Live's MIDI.** `take.py probe` plays Acid V velocities from 1 to 127 a
note at a time, and pairs of notes a fifth apart from 20 ms apart to 30 ms overlapping, and reads
the output: an accent is 1.5 dB louder or a quarter octave brighter than the plain notes, and a
slide is a pitch still 150 cents short of the second note 5 to 25 ms in. On 2026-11-12, Acid V
on its Default preset with Accent full: every velocity from 100 up accented, about 9 dB louder
and 1.3 octaves brighter, and none under, with nothing between, as decision 121 has it; and a
slide from an overlap of 1 ms, while notes that touch, or have any gap, play afresh. A take's
notes are read the same way: an accent from velocity 100, and a slide where a note runs past the
next one's start. `probe-live` plays the same notes in Live, from a clip it makes in the track's
first empty slot and deletes after, and its findings, in `out/takes/acid-v-midi-live.json`, then
take the place of these.

**The bridge's side** (`live/VimtrackerBridge/takes.py`) has three requests: `take` reads a
Session View clip, its notes (`Clip.get_notes_extended`) and each parameter's envelope
(`Clip.automation_envelope(parameter).value_at_time`); `render take` plays it and records the
track after its devices, as `render` records the source; and `probe take` plays notes from a clip
it makes. While either plays, the master's fader is at its bottom, so nothing is heard, the track
is disarmed and Session Record off, so nothing records into the clip, and all three are put back
after; the recording is taken before the master, so it's the same either way. `render take`
first enables automation again, as Live's Re-Enable Automation button does, so a knob touched
since doesn't hold its envelope back. The recording is lined up with the take by its first
note's onset, to about a millisecond.

### Recording a take

1. In Live, at 48 kHz with 32-bit recordings as the bridge needs ([Setting it up](#setting-it-up)),
   put Acid V alone on a MIDI track, with nothing after it.
2. In Acid V, from any preset: turn its **sequencer off**, so a key plays one
   note; its **effects off**, the FX section bypassed and `Distortion` at 0% dry/wet; its `Sub
   Osc` and `Noise` all the way down; `Master Volume` at 0 dB if you like, as the report takes
   the overall level out anyway; and leave `Osc Fine` and `Master Tune` where they load.
3. **Arm the track and switch Automation Arm on**, the button beside Live's transport.
4. **Record into an empty Session View clip slot**, not Arrangement View, whose automation a
   script can't read. Play the riff, unquantized is fine: velocity 100 and up is an accent, and a
   note held on into the next is a slide. Turn these knobs as you play, on Acid V's panel or a
   controller mapped to them, and they're followed:
   - `Cutoff`, `Resonance`, `Envelope Mod`, `Decay` and `Accent`.

   Leave its front panel's `Brightness`, `Timbre`, `Time` and `Movement` alone, as ours has no
   counterpart; any other parameter that moves is recorded and listed as not followed.
5. Stop, and give the clip a name of its own, such as `riff`, used by no other clip in the set.
   The export reads the parameters Live lists for Acid V: if its device view shows none, press
   `Configure` and turn the five knobs once on Acid V's panel, so Live lists them.
6. Then `just take export riff`, `just take render-live riff`, `just take probe-live "<track>"`
   with the track's name, and `just take compare riff`. Renders are silent and take as long as
   the clip, and the probe about 18 seconds.

## A spec

A spec is TOML, which Python 3.11 and later read with `tomllib` and Rust with the `toml` crate.

```toml
rate = 48000        # frames a second, 48000 if not given
duration = 1.0      # seconds; an effect's defaults to its input's length

[[notes]]           # a voice's notes, held for their lengths
time = 0.05         # seconds
length = 0.6        # seconds
note = 36           # MIDI note number; 60 is the program's C-3
velocity = 100      # 1 to 127, 100 if not given
accent = true       # optional: raise the velocity to an accent's, 0x70, or with false keep it under
slide = true        # optional: hold on into the next note, so a voice that slides glides there

[ours]
machine = "acid"    # "sampler", or a voice's key: va, vcf, acid, fm, kick and so on
sample = "kick.wav" # the file a sampler plays, from the spec's folder
effects = ["comp"]  # the track's slots, from the first, by effect key: distort, delay, comp, eq3...

[ours.settings]     # by the keys songs store, with the values the program shows
acid.cutoff = 0x40          # bare dotted keys, or quoted: "acid.cutoff" = 0x40
1.comp.threshold = 0xAA     # a slot's effect: slot, effect, setting
acid.wave = "Saw"           # a choice by its name, or its place from 0
cut = 0xC0                  # the track's own: vol, pan, cut, res, mode, pitch, glide
filter = "Ladder"           # the track's filter type, by the name the Sound page shows

[reference]
plugin = "/Library/Audio/Plug-Ins/VST3/Acid V.vst3"
plugin_name = "..."         # optional, for a bundle with several plugins
buffer_size = 512
preset = "Default"          # optional: an Arturia factory preset to start from
state = { Sequencer_OnOff = 0 }   # optional: saved settings that aren't parameters

[reference.parameters]      # by pedalboard's names, in the plugin's own units
cutoff = 0.5

# Or several references, each with a name, which reports and file names use:
# [[references]]
# name = "ABL3"
# plugin = "/Library/Audio/Plug-Ins/VST3/ABL3x.vst3"
# channels = 1              # how many it puts out, 2 if not given
# accent_from = 101         # the velocity it accents from, so `accent` means the same to it
# preroll = 1.0             # seconds played first and cut, for a plugin with a sound of its own
# wake_note = 60            # a note played at the pre-roll's start, for one that needs waking
# state = { MidiMode = 1 }  # its saved settings, Arturia's archive or JUCE's XML, by name
# [references.parameters]
# cutoff = 0.5

[report]                    # how report.py lays it out; see its docstring
kind = "knob"
knob = "acid.cutoff"
reference_knob = "cutoff"

[[cases]]                   # versions of the spec, rendered one at a time
name = "cutoff-00"
ours = { acid.cutoff = 0x00 }
reference = { cutoff = 0.0 }
input = { level = -20.0 }   # optional: the input's fields it changes, or another signal
# references = { "Acid V" = { cutoff = 0.0 }, ABL3 = { cutoff = 0.0 } }   # each reference's
# notes = [{ time = 0.05, length = 0.25, note = 36, slide = true }, ...]  # its own notes
```

A case's `reference` changes every reference's parameters, and its `references` each one's by
name, after it. A case's `notes` replace the spec's, so a spec can leave out its own when every
case has some.

A spec has `notes` for a voice or an `input` for an effect, not both. An effect's input is one of:

| `signal` | Fields | What it is |
|---|---|---|
| `file` | `path`, and `made_by` if `run.py` makes it | a file, from the spec's folder, at the spec's rate; give it at that rate, as the two sides resample differently. With `made_by = { spec = "...", case = "..." }`, `run.py` first renders our side of that spec's case into it |
| `sine` | `frequency`, `level`, `seconds` | a steady sine |
| `sweep` | `from`, `to`, `level`, `seconds` | Farina's exponential sweep |
| `steps` | `frequency`, `from`, `to`, `step` (1 if not given), `hold` | a sine at levels from `from` to `to` dB, each held `hold` seconds, its phase running on |
| `burst` | `frequency`, `level`, `rest`, `before`, `on`, `after` | a sine at `rest` dB, then `level` for `on` seconds, then `rest` again |
| `silence` | `seconds` | nothing |
| `impulse` | `level`, `seconds` | one sample at `level`, then nothing, for a filter to ring from |

Levels are dB of full scale at the peak, times are in seconds and frequencies in Hz. `spec.py`
makes them by the same formulas as `crates/vt-tui/src/dsp_probe.rs`, in double precision.

Our side plays a note as the keyboard does, held through a track, so the track's voice, filter and
effects are all heard and nothing is sequenced; notes that overlap slide on a voice that slides, as
held keys do. An input goes through the track at full level with no fades. The limiter is off, so
nothing is clipped. A setting the spec has no place for, such as another voice's or one for an
effect not in its slots, is refused, as is a key the program doesn't have, with the keys beside
it.

## Left for later

The 303's slide set and saturation, which #186 listed, are measured now, by `nuance-slides` and
`nuance-legato` and by the `pedal-*` and `live-pedal-*` specs. Still to come: other voices'
envelopes (#186), and a blind listening page for the Acid against its references (#349).
