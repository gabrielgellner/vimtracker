# Comparing against reference plugins

One spec renders through both our code, with `vimtracker probe`, and a reference plugin, hosted by
Spotify's [pedalboard](https://github.com/spotify/pedalboard), so a test is written once and
rendered twice (#186, decision 110). Only the outputs are compared: nothing here looks inside a
plugin. None of it runs in CI.

## Setting up

With [uv](https://docs.astral.sh/uv/) and a release build of vimtracker:

```sh
cargo build --release
cd tools/compare
uv sync                       # makes .venv with pedalboard 0.9.26, numpy, scipy and mido
uv run run.py specs/acid-cutoff.toml
uv run run.py specs/comp-static.toml --case ratio-4
```

`run.py` renders every case of a spec, or those named with `--case`, into `out/<spec>/` as
`<case>-ours.wav` and `<case>-reference.wav`, and prints both sides' peak, RMS and spectral
centroid. A sweep's input adds the magnitude response in octave bands, and stepped levels add the
static curve. It checks the effect's input it made against the probe's own, which should differ by
0.

Each side can be rendered alone:

```sh
../../target/release/vimtracker probe specs/acid-cutoff.toml ours.wav --case cutoff-80
uv run render_reference.py specs/acid-cutoff.toml reference.wav --case cutoff-80
```

`check_plugin.py <plugin> instrument|effect` is the load check from #186, for trying a new
reference plugin.

## The plugins

Reference plugins are read where they're installed, such as `/Library/Audio/Plug-Ins/VST3`, and
nothing here writes in their folders or their preferences. Plugins write their own files as they
load, which is theirs to do: Arturia's update `/Library/Arturia/Presets/db.db3`, the preset tags
beside it and `/Library/Arturia/<plugin>/tmp/plugin.pref.xml`, and print their logging settings
(`## LogInRelease : 0` and so on); u-he's write a cache.

Always give pedalboard a host's buffer size: Arturia's plugins put out silence at its default of
8192, so a spec's `buffer_size` is 512 unless it says otherwise.

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

[reference]
plugin = "/Library/Audio/Plug-Ins/VST3/Acid V.vst3"
plugin_name = "..."         # optional, for a bundle with several plugins
buffer_size = 512

[reference.parameters]      # by pedalboard's names, in the plugin's own units
cutoff = 0.5

[[cases]]                   # versions of the spec, rendered one at a time
name = "cutoff-00"
ours = { acid.cutoff = 0x00 }
reference = { cutoff = 0.0 }
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

Levels are dB of full scale at the peak, times are in seconds and frequencies in Hz. `spec.py`
makes them by the same formulas as `crates/vt-tui/src/dsp_probe.rs`, in double precision.

Our side plays a note as the keyboard does, held through a track, so the track's voice, filter and
effects are all heard and nothing is sequenced; notes that overlap slide on a voice that slides, as
held keys do. An input goes through the track at full level with no fades. The limiter is off, so
nothing is clipped. A setting the spec has no place for, such as another voice's or one for an
effect not in its slots, is refused, as is a key the program doesn't have, with the keys beside
it.

## Left for later

Plots and an HTML report, knob sweeps and fitting a mapping to them, and the 303's accent and slide
set, each in its own PR (#186).
