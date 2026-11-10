# Comparing against reference plugins

One spec renders through both our code, with `vimtracker probe`, and one or more reference
plugins, hosted by Spotify's [pedalboard](https://github.com/spotify/pedalboard), so a test is
written once and rendered on every side (#186, decisions 110, 113 and 125). Only the outputs are
compared: nothing here looks inside a plugin. None of it runs in CI.

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
| `comp-bus-static` | the compressor's `Bus` type at 2, 4 and 10:1 | Bus FORCE | static curve |
| `comp-bus-attack`, `comp-bus-release` | `Bus`'s attacks and set releases | Bus FORCE | attack and release, short and long bursts |
| `comp-bus-auto` | `Bus`'s Auto release | none | release after bursts from 20 ms to 2 s |
| `comp-bus-harmonics` | `Bus` at stepped levels, 100 Hz and 1 kHz | Bus FORCE | THD and the 2nd and 3rd harmonics (`distortion`) |
| `nuance-knob-*` | decision 117's seven `acid-*` knob specs | Acid V and ABL3 | as the `acid-*` specs, with the consensus |
| `nuance-accents` | `AAAA`, `A.A.` and `AA..` twice through after a plain note, at Res 0, 50% and 90% | Acid V and ABL3 | each note's brightness and level over the plain note's, the first accent's and the build-up (`notes`) |
| `nuance-slides` | slides up and down by a semitone, a fifth and an octave | Acid V and ABL3 | the pitch over time, the glide's times, the dip and the brightening (`transition`) |
| `nuance-legato` | two notes 60 ms apart to 20 ms overlapping, and slid | Acid V and ABL3 | the same, for where legato turns to a retrigger |
| `nuance-gate` | one note held from a 128th to a quarter | Acid V and ABL3 | its level as it's held and its release (`gate`) |
| `nuance-extremes` | every knob at its bottom or top, 32 ways, two plain notes and two accents | Acid V and ABL3 | as `nuance-accents` |
| `nuance-chirp` | one plain or accented C-2 at Res 75, 85, 95 and 100%, Env Mod 50 and 100%, Cutoff low and mid, Decay short and long, 64 ways | Acid V and ABL3 | the resonant peak over time: its place, its height over the fundamental and the passband, the skirt an octave over it, and the note's start (`chirp`) |
| `nuance-chirp-riff` | the Blade riff's Acid line from `confusion-pump-panel`, once through | Acid V and ABL3 | the same, note by note along the riff |
| `pedal-distortion-harmonics` | Distort's `Distortion` at `Amount` `40`, `80` and `FF`, stepped levels at 110 Hz and 1 kHz | Dist OPAMP-21, `Modern`, at 25, 50 and 100% `Drive` | THD and the 2nd and 3rd harmonics (`distortion`) |
| `pedal-distortion-response` | `Distortion` given a sweep too quiet to clip, `Tone` open and at its default | Dist OPAMP-21, `Modern` and `Normal` | the EQ around the clip (`response`) |
| `pedal-riff` | `nuance-chirp-riff`'s Blade riff as our Acid plays it, through `Distortion` at three drives | Dist OPAMP-21, the same samples in | the long-term spectrum's shape, level, crest factor and level over time (`spectrum`) |
| `pedal-overdrive` | Distort's `Overdrive` at stepped levels at 110 Hz and 1 kHz, four drives | none, as no reference is a Tube Screamer | THD and the 2nd and 3rd harmonics (`distortion`) |
| `pedal-acid-v` | our Acid's held, open note through `Overdrive` and `Distortion` | Acid V through its own `Overdrive` and `Distorsion` | a held note's harmonics (`harmonics`), against each side's own note with no distortion |

The `filter-taper-*` specs are written by `taper_specs.py`, which sets our Res against Cytomic's
The Drop's: our knob's first 85%, `00` to `D9`, against its 0 to 100%, and the rest against its
100 to 125%, where it self-oscillates. Their `taper` report reads each step's sweep for how far the
peak stands over the passband three octaves away, and whether the filter is ringing by itself near
Cut as the sweep ends, two octaves and more past it.

The Acid's filter is inside the voice, where the probe can't feed it a sweep, so `acid-filter`
reads its shape from a held sawtooth's harmonics on both sides. The track filter's `Diode` type is
the same ladder given a sweep, in `filter-ours`.

## The plugins

Reference plugins are read where they're installed, such as `/Library/Audio/Plug-Ins/VST3`, and
nothing here writes in their folders or their preferences. Plugins write their own files as they
load, which is theirs to do: Arturia's update `/Library/Arturia/Presets/db.db3`, the preset tags
beside it and `/Library/Arturia/<plugin>/tmp/plugin.pref.xml`, and print their logging settings
(`## LogInRelease : 0` and so on); u-he's write a cache.

Acid V accents a note from velocity 100 and ours from `0x70`, 112, so the Acid specs play a plain
note at 99 and an accent at `0x70`, which both sides read the same way. A reference's
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
another spec's case, such as `nuance-chirp-riff`'s riff, written where the input reads it, so
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

Always give pedalboard a host's buffer size: Arturia's plugins put out silence at its default of
8192, so a spec's `buffer_size` is 512 unless it says otherwise.

An Arturia plugin opens with the preset it last had. Acid V opened with "Carbon Based", a sequence
whose sequencer plays its own pattern from a held key, so a spec can start a plugin from a factory
preset, read from `/Library/Arturia/Presets/<plugin>/`, with `preset = "Default"`, and change
settings that aren't parameters with `state`, such as `state = { Sequencer_OnOff = 0 }`
(`arturia.py`). These are the plugin's saved settings, as a host keeps them in a project, changed
by name. Acid V takes them only when given them twice, which `render_reference.py` does.

## A spec

A spec is TOML, which Python 3.11 and later read with `tomllib` and Rust with the `toml` crate.

```toml
rate = 48000        # frames a second, 48000 if not given
duration = 1.0      # seconds; an effect's defaults to its input's length

[[notes]]           # a voice's notes, held for their lengths
time = 0.05         # seconds
length = 0.6        # seconds
note = 36           # MIDI note number; 60 is the program's C-4
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

The 303's slide set, saturation, and voices'
envelopes, each in its own PR (#186).
