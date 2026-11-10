# Comparing against reference plugins

One spec renders through both our code, with `vimtracker probe`, and a reference plugin, hosted by
Spotify's [pedalboard](https://github.com/spotify/pedalboard), so a test is written once and
rendered twice (#186, decisions 110 and 113). Only the outputs are compared: nothing here looks
inside a plugin. None of it runs in CI.

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
`<case>-ours.wav` and `<case>-reference.wav`, and prints both sides' peak, RMS and spectral
centroid. A sweep's input adds the magnitude response in octave bands, and stepped levels add the
static curve. It checks the effect's input it made against the probe's own, which should differ by
0. A spec with no `[reference]` renders ours alone.

`--report page.html` writes one self-contained web page for the spec: what the test is, what each
line is, what differs most, both sides plotted together by matplotlib, and the numbers in tables
(`report.py`). Every spec at once:

```sh
for spec in specs/*.toml; do uv run run.py "$spec" --report "out/$(basename "$spec" .toml).html"; done
```

The pages are about 0.1 to 2 MB each, so they stay in `out/`, which git ignores.

Each side can be rendered alone:

```sh
../../target/release/vimtracker probe specs/acid-cutoff.toml ours.wav --case cutoff-88
uv run render_reference.py specs/acid-cutoff.toml reference.wav --case cutoff-88
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
| `acid-filter` | Acid's diode ladder | Acid V | a held note's harmonics |
| `acid-cutoff` | Acid's Cutoff, 16 steps | Acid V's | centroid and peak against the knob (#295) |
| `comp-static`, `comp-static-force` | the compressor | Comp VCA-65, Bus FORCE | static curve |
| `comp-timing`, `comp-timing-force` | the compressor | Comp VCA-65, Bus FORCE | attack, release, short and long bursts |

The Acid's filter is inside the voice, where the probe can't feed it a sweep, so `acid-filter`
reads its shape from a held sawtooth's harmonics on both sides. The track filter's `Diode` type is
the same ladder given a sweep, in `filter-ours`.

## The plugins

Reference plugins are read where they're installed, such as `/Library/Audio/Plug-Ins/VST3`, and
nothing here writes in their folders or their preferences. Plugins write their own files as they
load, which is theirs to do: Arturia's update `/Library/Arturia/Presets/db.db3`, the preset tags
beside it and `/Library/Arturia/<plugin>/tmp/plugin.pref.xml`, and print their logging settings
(`## LogInRelease : 0` and so on); u-he's write a cache.

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

[report]                    # how report.py lays it out; see its docstring
kind = "knob"
knob = "acid.cutoff"
reference_knob = "cutoff"

[[cases]]                   # versions of the spec, rendered one at a time
name = "cutoff-00"
ours = { acid.cutoff = 0x00 }
reference = { cutoff = 0.0 }
input = { level = -20.0 }   # optional: the input's fields it changes, or another signal
```

A spec has `notes` for a voice or an `input` for an effect, not both. An effect's input is one of:

| `signal` | Fields | What it is |
|---|---|---|
| `file` | `path` | a file, from the spec's folder, at the spec's rate; give it at that rate, as the two sides resample differently |
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

Fitting a mapping to a knob sweep, the 303's accent and slide set, saturation, and voices'
envelopes, each in its own PR (#186).
