# The Gen page, macros, and generators in the engine

**Status:** a design study for the author to read and decide on, written on 2026-11-13. No code is
changed. Their answers to §9 will go under it, as **Decided**.

A design study for the author's request of 2026-11-13, in three parts already settled with them:

1. **A Gen page:** a track page of its own, beside Sound, Mod and FX. Each live generator on the
   track gets a device frame, as a mod slot does, with its settings declared once for the page,
   `K`, the Roto and the status line. Turning one rerenders the column, one undo step a touch.
2. **Eight macros a track,** one a Roto knob, after Ableton Live's Rack macros:
   * each maps to any number of settings, each with a min and a max;
   * each is itself a setting, lockable on a step, a destination for modulation, and on the
     Roto's first page;
   * each is named by the author, and saved with track presets (#157).
3. **Generators move into the engine,** in the long term. Each pass of a phrase is rendered again
   from the generator's `Shape`, seed and pass, with its settings as modulation and locks leave
   them, so an LFO or a lock can change notes as they play, as `Mutate`'s draws already do.

A fourth part came from the author during the study: **patches**, the behaviour half of a track
preset, which add a Turing Machine to any track without touching its sound (§5).

**It builds on:**

* the device standard of [`track-pages.md`](track-pages.md) §1;
* the mod slots of [`mod-slots.md`](mod-slots.md) and the Roto's layouts of
  [`roto-control.md`](roto-control.md) §4.3;
* decisions 151 (the rack), 159 to 161 (the Roto), 178 to 182 (live generators), 204 and 269
  (`Mutate` and keeping its draws), 268 (`S` generating), 270 (`Space g A`) and 273 (`Acid`);
* the code: `vt-model`'s `shape.rs`, `shape/acid.rs`, `generator.rs`, `settings.rs` and
  `target.rs`; `vt-engine`'s `track_state.rs` and `engine/steps.rs`; and `vt-tui`'s
  `shaping.rs`, `track_page.rs` and `roto/layouts.rs`;
* the generators tutorial, `docs/manual/src/tutorials/generators.md`, whose songs give the
  examples.

**The recommendation in one paragraph.**

* **The Gen page** is the fourth page, after FX. It shows the generators of **the phrase under the
  cursor**, the one `gl` reopens, and names it in its title.
* **Its first band is the track's eight macros;** under it, one full-width device a live column.
  A device's title is its fill and its `Where`, then its column, as `Acid  Euclid · Note`.
* **A generator's settings are declared once,** in `vt-model`, as the registry declares a
  machine's, and the panel, the page, the Roto and the status line all read that table.
* **Macros are absolute,** as Live's are: a macro at `00` sets each of its settings to its min, at
  `FF` to its max. `M` on any setting maps it; `gm` opens the mapping. A macro is a setting
  called `MAC1` to `MAC8`.
* **Stages:**
  * **(a)** the page, as an editor of what's there;
  * **(b)** macros, which reach every audio-time setting at once and generator settings by hand;
  * **(c)** generators rendered by the engine, a pass at a time and again as their inputs move,
    from code shared with the edit thread and allocation-free, so modulation and locks reach
    them. The phrase keeps showing what's written; the playhead's row and the overview show
    what's heard; and `Space g a` keeps what was heard.
* **Patches** are a preset kind of their own beside #157's sounds: macros and the parts they move,
  never the machine.

---

## 1. Prior art

| | What it is | Where its controls live | Edit time or play time | What we take |
|---|---|---|---|---|
| **Ableton Live's Rack macros** (Reference, "Instrument, Drum and Effect Racks") | 8 macros a rack, up to 16 since Live 11, each mapped to any number of the rack's parameters | the rack's macro panel; Map mode lights every mappable control, a click maps it, and the mapping browser lists each with its Min and Max | play time: a macro is automatable and mappable | the model: one knob, many targets, a range each, min above max allowed; a macro named after its first mapping until renamed; the mapped control locked to its macro |
| **Live's Macro Variations** | snapshots of all of a rack's macro values, recalled by a click | the rack's variations panel | play time | later (§8) |
| **Live 12's MIDI Generators and Transformations** (Rhythm, Seed, Shape, Stacks, Euclidean) | a panel in the clip view that writes notes into the clip | the clip view, beside the notes | edit time: they write notes, and a change writes them again; their controls aren't automated | what our live generators are today, and the reason for stage (c) |
| **Bitwig's Remote Controls** | pages of 8 controls per device, chosen by the user, plus track and project remotes in Bitwig 5 | the device's remote panel and the controller | play time | pages of 8 named controls, the Roto's shape (decision 159 already follows it) |
| **Bitwig's Note FX** (Arpeggiator, Note Echo, Randomize and the rest) | devices that make or change notes as they play | the device chain, before the instrument | play time: every control can be modulated | stage (c)'s goal: generation that modulation reaches |
| **Octatrack scenes and the crossfader** | 16 scenes a part, each locking parameters on any track; the crossfader morphs between scene A and B | the scene buttons and the crossfader | play time | later, a song-wide macro (§9) |
| **Analog Rytm's performance macros** | each of 12 pads moves up to 48 parameters with a depth each | Performance mode, saved in the kit | play time | macros saved with the sound, not the song |
| **Maschine's macros** | 8 macro controls on each sound, group and the master, assigned to any parameter and named | the Macro page, on the hardware's knobs | play time | named macros on the hardware's first page |
| **tracker-ce** (`synth/include/PolySynth/Patch.h`, `docs/custom_synth_sdk.md`) | 6 user macros an instrument, plus a velocity macro; each has modifiers, a parameter and an amount each, added or multiplied by the parameter's own rule; bipolar; named; on CCs from 102; a step FX can lock one | the instrument editor | play time | locks on a macro, and macros saved with the patch. Its macros are relative (an amount added); we take Live's absolute ranges instead (question 3). Its performance mode (`performanceEngine.h`) is per-track FX in slots, not macros. |

**What we take:**

* **From Live,** the macro's model and Map mode's gesture, done as a key on the setting itself.
* **From Live 12, as a warning:** generators that only write notes can't be modulated, which is
  where ours are now.
* **From Bitwig's Note FX,** the goal: a generator is a device that plays.
* **From Bitwig, Maschine and the Roto,** eight named controls on the hardware's first page.
* **From tracker-ce and the Rytm,** macros that a step locks and that a sound saves.

---

## 2. Where things are now

* **A live generator is edit time** (decisions 178 and 180). It's a `Generator` on its phrase,
  holding its column, its span, its `Shape` and the cells from under it. A change to its `Shape`
  writes the column again (`Generator::render`), so the phrase always holds what's heard, and the
  engine plays the phrase as it plays any other.
* **A `Shape`** is plain `Copy` data: `from`, `to`, `curve`, `s`, `kind` (the fill), `wave`,
  `rate`, `phase`, `run`, `values`, `place` (the `Where`, with its amount, hits or rotation),
  `seed`, and `acid`, `Acid`'s own (notes, octave, density, accent, slide, `keep`, `roll`).
* **The panel's rows** are declared in `vt-tui` (`shaping::ShapeRow`), with their labels, keys and
  what each shows, apart from the registry.
* **The registry** (`settings.rs`) declares every setting a step can lock or a slot can move: a
  key songs store, a name, a label for locks (or none, for a setting only modulation reaches), a
  `Kind` and a default. A mod slot has four entries, `depth`, `phase`, `polarity` and `deform`;
  Steps' `Mutate` and `Length` aren't in it.
* **Labels** are three or four capitals, with a slot's digit before an effect's (`1BYP`) and after
  a mod slot's (`DEP4`); a test holds every label to that.
* **The Roto's layouts** (`vt-tui`'s `roto/layouts.rs`, decision 161) are `DeviceLayout`s, pages of
  eight knob slots over eight button slots naming settings by key. The track page's grids are
  declared apart from them, per device, so the screen and the Roto each have their own order;
  both name the same keys.
* **The engine reads a row** from the song it's been given (`Track::step_at`) as it plays, and
  looks ahead at the next for a nudge or a groove (`row_ahead`). It never edits the song.
* **`Mutate`'s draws** (decisions 204 and 269) are the precedent for play-time variation: a
  `Pcg32` stream a slot, from the project's seed, reset when playing starts from the top; what's
  drawn is kept in the slot's run, never in the song; and `Space g a` asks the engine what it
  drew and writes it in as an edit.

---

## 3. The Gen page

### 3.1 Which phrase's generators

Generators live on phrases, and a track plays a chain of them, so the page has to choose.

| | The phrase under the cursor | The phrase playing | Both |
|---|---|---|---|
| What it is | `App::phrase_here`: the cursor's track, section and chain slot, the phrase `Space 3` shows and `gl` reopens | the phrase the track is playing now | the cursor's, with the playing one beside it or a key to follow it |
| Turning a knob | edits what you're looking at, playing or not | edits what you hear | either |
| While a chain plays | stays put | the devices swap under the hand at each phrase, and a turn that spans a change edits two phrases | as either |
| On the Roto | its devices stay put | its devices change every phrase | as either |
| After stage (c) | the page is the phrase's base settings; macros and modulation reach whichever plays | the same, but what you edit keeps moving | as either |

**Recommended: the phrase under the cursor.**

* It's the rule every other edit keeps: what you change is what you're on. Following playback is
  the M8's and the Octatrack's live mode, a view, not where edits go.
* The title names it, `Phrase 00`, and adds `▶ 02`, dim, when the track is playing another, so
  you're never turning one you can't hear without being told.
* What you'd want from "the phrase playing" comes through macros instead: from stage (c) a
  macro reaches the generator on its column in **whichever phrase plays** (§4.1, §7), so a
  performance knob follows the chain while the page stays still.
* A follow mode, the page and the phrase view both going with the playhead, is a separate idea
  for every view and is left for later.

### 3.2 The page

**Its place:** the fourth page, `Sound  Mod  FX  Gen`, so `L` from FX reaches it and nothing you
know moves. A fixed track has no phrases; its Gen page has only the Macros band (macros on a group
or the master are as useful as Maschine's).

**Its rack,** decision 151's:

* **The Macros band** comes first, full width (§4.3).
* **Then a full-width device a live generator,** in the column order of the phrase view: Note,
  Velocity, Nudge, Length, then FX 1 to 3. Two generators on one column at different rows are two
  devices, top row first.
* **Then an `Empty` device,** where `K` lists the columns of the phrase that have no generator and
  makes one live over the phrase's loop (or the column's lane), as `vG`, `S` and `L` would.
* **Past four bands** it scrolls by whole devices, as Decided note 4 of the track pages has it.

**A device's title:**

* **The fill,** then **the `Where`,** both title controls that `K` picks, as a mod slot's type and
  mode are. A `Where` that's its column's default (`Notes` on Velocity, `Commands` on an FX column,
  `All rows` on an empty Note column) is dim.
* **Then `·` and the column, dim,** with its span: `Note 00–0F`, or `FX1 FREQ lane 0C` for an FX
  column's command on a lane. It goes last, as a mod slot's `→ Dest` does, because it's where the
  device acts rather than what it is; and it's not a control, as moving a generator to another
  column is a new generator.
* **So** the author's examples read `Acid  Euclid · Note` and `Ramp  Euclid · Vel`.
* **No switch yet.** A switched-out generator would play what's under it; that's easy in
  stage (c) and awkward before it, so it waits (§8).

**Its grid:** three columns of three, Sound's bar cells (value 5 wide), declared per fill and
`Where` (§3.3). The `Where`'s own settings (`Every`'s step, `Chance`'s percent, Euclid's `Hits` and
`Rotate`) take the last cells, so they stay put as the fill changes.

| Fill | Column 1 | Column 2 | Column 3 |
|---|---|---|---|
| Ramp | From, To, Run | Curve, S | the `Where`'s |
| Random | From, To, Run | | the `Where`'s |
| Wave | From, To, Shape | Rate, Phase, Run | the `Where`'s |
| Alternate, List | From and To, or Values; Run | | the `Where`'s |
| Acid | From, To, Notes | Octave, Accent, Slide | Pitches, then Density or the `Where`'s |
| Scale | From, To, Curve | S | the `Where`'s |
| Invert, Shuffle, Reverse, Snap | | | the `Where`'s |

The seed isn't a cell: `r` rolls it from anywhere in the device, as in the panel.

**Its panel:** size L, 49 by 5, at the right, showing **what the generator writes across its
span**, at three cells a row for 16 rows:

* notes as short lines at their pitch, a mini piano roll;
* velocities, nudges and lengths as bars;
* an FX column as its values' line, as the FX page draws a curve.

The row playing is a placed dot (decision 109's technique, about 60 bytes a move). The picture is
sent again only when the column's values change, so a turn of `Rotate` costs one image.

**The status line** says what the cursor's setting means in the column's units, as the panel's
heading does now: `Hits, 9 of the 16 rows`, `To, C-3`, `Octave, 20%`.

### 3.3 Declared once

A generator's settings get the same machinery as a machine's: a table that says what each is, and
layouts that place them by key.

* **In `vt-model`, `shape/params.rs`:** a `GenParam` a setting, holding the registry's `Entry`
  (key, name, label or none, `Kind`, default) and how to read and write it on a `Shape`. One list
  for every fill and `Where`, with `exists_for(shape)` saying which a shape has, as
  `SettingId::exists_on` says which a track has. New units where a setting needs them: `Note`
  (`C-3`), `Percent` already, `Rows` (`9 of 16`).
* **Its uses:**
  * the shape panel's rows (`ShapeRow`) become a view of it, so the panel and the page can't drift
    apart;
  * the Gen page's grids, `[[Option<&str>; 3]; 3]` a fill, as `Machine::grid()` is;
  * `K` on a list setting (`Fill`, `Where`, `Shape`, `Rate`, `Pitches`) lists its `Kind`'s choices;
  * the status line's text, from `Kind::shown` in the setting's units;
  * the Roto's layout for each fill (§6), a `DeviceLayout` naming the same keys;
  * the macros' targets (§4) and, in stage (c), the engine.
* **Why two layouts and one table:** decision 161 split the Roto's order from the screen's for good
  reasons; what was missing for generators is the table both read. A machine already works this
  way: its entries in `settings.rs`, its grid in `track_page/sound.rs`, its Roto layout in
  `roto/layouts.rs`.

### 3.4 Keys and undo

The track page's keys, unchanged: `h j k l`, `{ }`, `=` and `-` (and `+` and `_` for the larger
step), `i` to type, `x` for the default, `K` on a list, `/` to jump to a setting by name.

* **A turn rerenders the column,** through the same `Reshape` action `gl` and `L` use, and a held
  key or a Roto touch is one undo step (`History::squash_to`, decision 160).
* **`r`** rolls a new seed from anywhere in the device, and with `Acid`'s `Pitches` on `Keep`,
  only its roll, as in the panel.
* **`Space g a`** on a device applies it, leaving plain values, as it does in the phrase view.
* **`dd`** takes the generator away and puts back what was under it; `u` brings it back.
* **`Enter`** opens the shape panel on it, as `gl` does from the phrase, for the settings the page
  leaves out (the transforms' other choices) and for previewing before keeping.
* **`.`** isn't the panel's repeat here; it stays the track page's.

### 3.5 Mockup 1: an acid line, stage (b)

How to read it: as the track pages' mockups, 163 wide with the main meter and two status rows; `›`
is the cursor; braille stands in for kitty images; `░` is a faint band, a mapped setting's range
(§4.3); `●` a placed dot.

* **Macros:** six named and mapped, two empty. The band's last row says where the cursor's macro,
  or the macro the cursor's setting is on, goes.
* **`Acid  Euclid · Note 00–0F`,** a riff of four notes on 9 of 16 rows. The cursor is on
  `Octave`. `Accent`, `Slide`, `Hits` and `Rotate` are on macros, so their bars show their ranges.
  The dot is the row playing.
* **`Wave · FX1 FREQ lane 0C`,** `Acid`'s cutoff swept once round a lane of 12. Its `To` is on
  macro 5 with `Acid`'s own `Cutoff`.
* **`Empty`:** `K` makes another column live.

```
󰙽 Track 01 TB   Sound  Mod  FX  Gen   Phrase 00                                                                              CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Macros ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Hits      9900 ▕█████▍   ▏   Rotate    2000 ▕█▏       ▏   Accent    5000 ▕██▉      ▏   Slide     3800 ▕██       ▏                                 │     Main    
│                                                                                                                                                    │    -9 -8    
│  Cutoff    7400 ▕████▏    ▏   Space     4000 ▕██▎      ▏   Macro 7   0000 ▕         ▏   Macro 8   0000 ▕         ▏                                 │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│  Hits → Note Hits 3–13                                                                                                                             │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Acid  Euclid · Note 00–0F ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  From      C-2  ▕         ▏   To        C-3  ▕█████████▏   Notes     4    ▕████▌    ▏         │                                                 │  │             
│                                                                                               │               ●⠒                            ⠒⠒  │  │             
│  Octave   ›20%  ▕█▊       ▏   Accent    19%  ▕█▊░░░    ▏   Slide     11%  ▕█░░░░    ▏         │      ⠒⠒                            ⠒⠒           │  │             
│                                                                                               │                        ⠒⠒                       │  │ -12         
│  Pitches   Roll               Hits      9    ▕█████▏░  ▏   Rotate    2    ▕█▏░░░░░░░▏         │⠒⠒       ⠒⠒          ⠒⠒       ⠒⠒       ⠒⠒        │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Wave · FX1 FREQ lane 0C ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                               ┌─────────────────────────────────────────────────┐  │     ██ ██   
│  From      30   ▕█▊       ▏   To        A0   ▕█████▋░░ ▏   Shape     Sine                     │    ⣠⠖⠳⠒⡀                   ⣠⠖⠳⠒⡀                │  │     ██ ██   
│                                                                                               │ ⢀⡴⠋⠁   ⠙⠦⣄              ⢀⡴⠋⠁   ⠙⠦⣄              │  │     ██ ██   
│  Rate      x1                 Phase     0°   ▕         ▏   Run       1                        │⠞⠁        ⠈⠓⢤⡀        ⢀⡤⠞⠁        ⠈⠓⢤⡀        ⢀⡤ │  │ -24 ██ ██   
│                                                                                               │             ⠙⠲⣄   ⢀⣠⠞⠁              ⠙⠲⣄   ⢀⣠⠞⠁  │  │     ██ ██   
│                                                                                               │               ⠈⠑⢦⠴⠋                   ⠈⠑⢦⠴⠋     │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Empty ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  K  make a column of this phrase live                                                                                                              │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Octave, 20%  󰐊  126 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                          
The chance each note goes an octave up or down. = and - move it, and the notes follow as it turns                                                                  
```

---

## 4. Macros

### 4.1 The model

```rust
pub const MACROS: usize = 8;

pub struct Macro {
    /// Its name, as typed, or its first mapping's until it's named.
    pub name: Option<String>,
    /// Where it's turned to, `0000` to `FFFF`, coarse and fine as a cutoff is.
    pub value: u16,
    pub maps: Vec<MacroMap>,
}

pub struct MacroMap {
    pub target: MacroTarget,
    /// What the target is at the macro's `0000` and `FFFF`, in its own values. `min` above `max`
    /// turns it the other way.
    pub min: u16,
    pub max: u16,
}

pub enum MacroTarget {
    /// Anything in the registry: the track's, its machine's, an effect's in a slot, a mod slot's,
    /// the sampler's.
    Setting(SettingId),
    /// A setting of the generator on this column, in whichever phrase it's heard: `Note`, `Vel`,
    /// `FX1` and so on, and a `GenParam` key.
    Gen { column: GenColumn, key: &'static str },
}
```

* **On `Track`:** `macros: [Macro; MACROS]`, saved only where it differs from an empty macro, with
  serde defaults, so the song format stays at 9 and older builds read a song as if it had none.
* **Any number of maps a macro,** as Live has. A `Vec` in the song is read, never copied, by the
  engine, so it costs the audio thread nothing to hold. A setting is on at most one macro, as in
  Live: mapping it to another moves it.
* **The registry grows** so a macro can reach a mod slot's own settings: each type's settings
  (Steps' `Rate`, `Length`, `Start`, `Loop`, `Mutate`, `Smooth`; an LFO's `Rate` and `Fade`; an
  envelope's times; Track's `Low` and `High`), and the accumulator's and the arpeggio's, as
  entries **with no label**, so modulation and macros can move them but steps can't, which is the
  registry's rule already. Whether they also appear in `Dest` lists is the same change and costs
  nothing more; recommended yes, so an LFO can move Steps' `Length` as a macro can.
* **Actions:** `SetMacro { track, macro, value }` for a turn, and `MapMacro`, `UnmapMacro`,
  `SetMapping` and `NameMacro`, each one undo step.

### 4.2 A macro is a setting

* **In the registry:** `Owner::Macro`, eight entries, keys `macro1` to `macro8`, labels `MAC1` to
  `MAC8`. The author's `MC1` would break the label rule of three or four capitals before a digit,
  which keeps every label four cells and parseable; `MAC` is free, and `/mac` finds all eight.
* **A lock:** `MAC3 80` on a row turns macro 3 to `80` from that row, as any lock does, and with
  it everything macro 3 reaches.
* **A destination:** a mod slot's `Dest` lists `Macro 1 Change` to `Macro 8`, grouped as `Macros`,
  and its depth is in the macro's `00` to `FF`. So an LFO on a macro sweeps every setting the
  macro holds, each across its own range.
* **On the Roto:** the track's first device is `Macros`, eight knobs named by the macros (§6).
* **What a macro does at play time,** in this order, as the engine works a setting out now:
  1. its own value: the song's, as turned by hand, the Roto or `SetMacro`;
  2. a `MAC` lock on the row, which wins for that row;
  3. modulation onto it, added;
  4. clamped, then mapped onto each target: `min + (max − min) × value`.

### 4.3 Making a mapping: `M`, and the mapping view

**The gesture, on the setting itself,** as Live's Map mode but with no mode to be in:

1. **`M` on any setting** of the track page (Sound, Mod, FX or Gen) opens a picker of the eight
   macros, each with its name and what it holds. A digit chooses at once: `M5`.
2. **`Enter` maps it,** from the setting's bottom to its top to start, as Live does. The first
   mapping of an empty macro turns the macro to where the setting is, so nothing jumps, and names
   the macro after it (`Cutoff`), as Live does. A mapping onto a macro that holds others jumps the
   setting to the macro, and the status line says so.
3. **`M` on a setting already mapped** opens the picker on its macro, where `Enter` on that macro
   unmaps it and on another moves it.

`M` isn't bound on the track page; it's "middle of the view" only in the views that scroll by rows.

**Mockup 2:** `M` on the FX 1 generator's `To`, with the cursor on macro 5.

```
󰙽 Track 01 TB   Sound  Mod  FX  Gen   Phrase 00                                                                              CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Macros ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Hits      9900 ▕█████▍   ▏   Rotate    2000 ▕█▏       ▏   Accent    5000 ▕██▉      ▏   Slide     3800 ▕██       ▏                                 │     Main    
│                                                                                                                                                    │    -9 -8    
│  Cutoff    7400 ▕████▏    ▏   Space     4000 ▕██▎      ▏   Macro 7   0000 ▕         ▏   Macro 8   0000 ▕         ▏                                 │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│  Hits → Note Hits 3–13                                                                                                                             │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Acid  Euclid · Note 00–0F ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  From      C-2  ▕         ▏   To        C-3  ▕████╭─ Map FX1 To to a macro  5/8 ───────────────╮                                                │  │             
│                                                   │ >                                          │              ●⠒                            ⠒⠒  │  │             
│  Octave   ›20%  ▕█▊       ▏   Accent    19%  ▕█▊░░│    1 Hits      Note Hits                   │     ⠒⠒                            ⠒⠒           │  │             
│                                                   │    2 Rotate    Note Rotate                 │                       ⠒⠒                       │  │ -12         
│  Pitches   Roll               Hits      9    ▕████│    3 Accent    Note Accent                 │⠒       ⠒⠒          ⠒⠒       ⠒⠒       ⠒⠒        │  │             
│                                                   │    4 Slide     Note Slide                  │────────────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰───────────────────────────────────────────────────│  › 5 Cutoff    Acid Cutoff                 │───────────────────────────────────────────────────╯     ██ ██   
╭─ Wave · FX1 FREQ lane 0C ─────────────────────────│    6 Space     Send A, Send B              │───────────────────────────────────────────────────╮ -18 ██ ██   
│                                                   │    7 Macro 7   nothing yet                 │────────────────────────────────────────────────┐  │     ██ ██   
│  From      30   ▕█▊       ▏   To        A0   ▕████│    8 Macro 8   nothing yet                 │   ⣠⠖⠳⠒⡀                   ⣠⠖⠳⠒⡀                │  │     ██ ██   
│                                                   ╰────────────────────────────────────────────╯⢀⡴⠋⠁   ⠙⠦⣄              ⢀⡴⠋⠁   ⠙⠦⣄              │  │     ██ ██   
│  Rate      x1                 Phase     0°   ▕         ▏   Run       1                        │⠞⠁        ⠈⠓⢤⡀        ⢀⡤⠞⠁        ⠈⠓⢤⡀        ⢀⡤ │  │ -24 ██ ██   
│                                                                                               │             ⠙⠲⣄   ⢀⣠⠞⠁              ⠙⠲⣄   ⢀⣠⠞⠁  │  │     ██ ██   
│                                                                                               │               ⠈⠑⢦⠴⠋                   ⠈⠑⢦⠴⠋     │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Empty ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  K  make a column of this phrase live                                                                                                              │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  To, A0  󰐊  126 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                               
Enter maps To to the macro chosen, from 00 to FF to start with; a digit chooses at once                                                                            
```

**The mapping view,** after Live's mapping browser: `gm` on a mapped setting opens it on that
setting's mapping, and on a macro in the band, on its first. Each mapping is a row with its
setting, min and max:

* `=` and `-` move a min or max, `+` and `_` by larger steps, and `i` types one, in the setting's
  own values (`30`, `C-2`, `1/16`, `40%`);
* `~` swaps min and max, so the setting goes the other way;
* `dd` unmaps the row, and `u` brings it back;
* `Esc` closes it.

**Mockup 3:** `gm` on macro 5.

```
󰙽 Track 01 TB   Sound  Mod  FX  Gen   Phrase 00                                                                              CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Macros ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Hits      9900 ▕█████▍   ▏   Rotate    2000 ▕█▏       ▏   Accent    5000 ▕██▉      ▏   Slide     3800 ▕██       ▏                                 │     Main    
│                                                                                                                                                    │    -9 -8    
│  Cutoff    7400 ▕████▏    ▏   Space     4000 ▕██▎      ▏   Macro 7   0000 ▕         ▏   Macro 8   0000 ▕         ▏                                 │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│  Hits → Note Hits 3–13                                                                                                                             │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Acid  Euclid · Note 00–0F ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  From      C-2  ▕         ▏   To        C-3  ╭─ Macro 5 Cutoff  2 mappings ─────────────────────────╮                                           │  │             
│                                              │                                                      │         ●⠒                            ⠒⠒  │  │             
│  Octave   ›20%  ▕█▊       ▏   Accent    19%  │  Setting                    Min    Max               │⠒⠒                            ⠒⠒           │  │             
│                                              │                                                      │                  ⠒⠒                       │  │ -12         
│  Pitches   Roll               Hits      9    │ ›Acid Cutoff                30     A0                │   ⠒⠒          ⠒⠒       ⠒⠒       ⠒⠒        │  │             
│                                              │  Gen FX1 FREQ To            60     F0                │───────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰──────────────────────────────────────────────│                                                      │──────────────────────────────────────────────╯     ██ ██   
╭─ Wave · FX1 FREQ lane 0C ────────────────────│  = - move  i types  ~ swaps  dd unmaps               │──────────────────────────────────────────────╮ -18 ██ ██   
│                                              ╰──────────────────────────────────────────────────────╯───────────────────────────────────────────┐  │     ██ ██   
│  From      30   ▕█▊       ▏   To        A0   ▕█████▋░░ ▏   Shape     Sine                     │    ⣠⠖⠳⠒⡀                   ⣠⠖⠳⠒⡀                │  │     ██ ██   
│                                                                                               │ ⢀⡴⠋⠁   ⠙⠦⣄              ⢀⡴⠋⠁   ⠙⠦⣄              │  │     ██ ██   
│  Rate      x1                 Phase     0°   ▕         ▏   Run       1                        │⠞⠁        ⠈⠓⢤⡀        ⢀⡤⠞⠁        ⠈⠓⢤⡀        ⢀⡤ │  │ -24 ██ ██   
│                                                                                               │             ⠙⠲⣄   ⢀⣠⠞⠁              ⠙⠲⣄   ⢀⣠⠞⠁  │  │     ██ ██   
│                                                                                               │               ⠈⠑⢦⠴⠋                   ⠈⠑⢦⠴⠋     │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Empty ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  K  make a column of this phrase live                                                                                                              │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Cutoff, 74  󰐊  126 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                           
At 00 the macro sets Acid Cutoff to 30, at FF to A0; it's at 74, so Cutoff is 63                                                                                   
```

**What a mapped setting looks like** wherever it shows:

* its bar draws the range as a faint band behind the fill, so you see how far the macro can take
  it (`Accent ▕█▊░░░    ▏` above);
* its value is what the macro gives it now;
* the status line says `Accent, 19%: on macro 3 Accent, 0% to 60%`;
* nothing else: no glyph or colour on the cell, by the standard's rule against badges.

**Turning a mapped setting directly** is refused, as Live greys a mapped control out: `=` on
`Accent` says `Accent is on macro 3: turn it there, or gm for its range`. The other choice, the
key turning the macro instead, is question 4.

**Naming:** `:macro Change` on the cursor's macro, in the band or the mapping view, and `:macro`
alone puts its first mapping's name back. Names are up to 12 characters, the Roto's width, ASCII
only as the Roto draws it (decision 161).

### 4.4 What a mapping reaches, stage (b)

Everything in the registry is audio time already, so from stage (b) a macro moves:

* **the track's own settings,** `Cut`, `Res`, `Level`, `Pan`, the sends, `Pitch`, `Gate`, `Speed`
  and the rest;
* **its machine's and its effects'** settings;
* **its mod slots':** depth, phase, polarity and deform today, and with §4.1's entries every
  type's own, so the author's Turing Machine is a Steps slot with `Mutate`, `Length` and `Depth`
  on three macros named `Change`, `Length` and `Range`;
* **the accumulator's and the arpeggio's.**

**How the engine does it:** each block of 32 frames (`LFO_UPDATE_FRAMES`), as modulation is worked
out, each macro's value is worked out as §4.2 has it and written as its targets' held values, the
place a hand's turn sits now (`held_choice` and the `values` a slot keeps for its locks). A step's
lock of the target itself still wins on its row, and modulation of the target still goes on top.
Its smoothing is the target's own. A mod slot's own settings are read through a copy of the slot
with the mapped fields put in, made each block on the stack (a `ModSlot` is `Copy`, about 120
bytes), so `modulate` itself doesn't change.

**It costs** one multiply and add a mapping a block, and nothing when a track has no macros.

### 4.5 Generator settings by hand, stage (b)

Before stage (c) a generator is still edit time, so a macro reaches its settings **by hand**:

* **The column is written from the macro's hand value.** A generator renders from its `Shape`
  with the track's mappings put in, the "heard shape", and the `Shape` saved keeps its own values
  underneath, so unmapping gives them back, as in stage (c).
* **A turn of the macro** by hand or on the Roto rerenders every live column of the track's
  phrases whose generator has the mapped setting, in the same edit, as decision 180's following
  rerenders on a note's change. A macro on a column that has no generator in a phrase does nothing
  there.
* **Its locks and modulation** reach its audio-time targets and not its generator ones until
  stage (c). The mapping view shows a generator row dim with `by hand` after it, and the status
  line says why.

### 4.6 Saved with track presets (#157)

The sound preset of #157 holds the machine, the filter, the mod slots and the effects. It should hold
the macros too, with their names, values and maps, so a track saved as a preset is a whole device
with its knobs: the author's Turing Machine on an `Acid`, with `Change`, `Length` and `Range`.

* **A mapping to a generator** is saved as its column and key. A preset holds no phrases, so it
  keeps the mapping and none of the generator; loaded on a track whose phrases have that column
  live, the macro takes hold of it. That's what keying by column buys.
* **A mapping to a setting the loading track lacks** can't happen for a sound preset, as it brings
  its machine and effects; for a patch it can (§5.3).

---

## 5. Patches: behaviour without the sound

The author's addition: a patch such as the hand-built Turing Machine should be saved and recalled
on its own, apart from a full track preset, so it can go onto a `VA` or an `Acid` track and add
the behaviour while keeping that track's sound.

### 5.1 Prior art

| | What it adds | What it keeps |
|---|---|---|
| **Live's MIDI Effect Rack and Audio Effect Rack presets** | a rack of effects with its macros, dropped before or after the instrument | the instrument and everything else on the track |
| **Bitwig's Note FX presets** and the modulators saved inside a device's preset | note devices, or a device with its modulation | the instrument |
| **Elektron's sounds and kits** | a sound replaces a track's whole sound, and a kit every track's; a sound lock swaps the sound for a step | nothing of the track's sound; there's no behaviour-only preset |
| **tracker-ce** | macros live in the instrument's patch | nothing apart: loading the patch replaces the instrument |

**What we take:** Live's split, a preset for the instrument and another for what's put round it,
in one browser.

### 5.2 What a patch holds

* **The macros:** their names, values, maps and ranges, as they're numbered in the patch.
* **The parts they move, and only those:**
  * **mod slots** a macro maps to, and any mod slot those reach in turn (a slot on another's
    `Depth`), with their types, modes, destinations and every type's settings (so a Steps slot
    keeps its values);
  * **the accumulator and the arpeggio,** if a macro maps to one;
  * **live generators,** by column, with their `Shape`s (and for a lane, its length), if a macro
    maps to one. These come from the phrase under the cursor when it's saved.
* **Never the machine, the filter, the output or the effects.** A map to one of those is kept, as
  a mapping by key to a setting the loading track may or may not have (§5.3).

**Saving:** `:patch save Turing Machine` opens a list of what it would hold, each line switched on
or off with `Space`: the macros with their parts on, and, off, the other mod slots that move
behaviour (on `Pitch`, `Pitch in scale`, `Velocity`, `Gate`) and the phrase's other live columns,
for a part you want to carry without a macro. `Enter` saves.

### 5.3 Recall

`:patch` lists the patches, as `K` lists anything (decision 73), searching by name and tag (#370),
and `Enter` adds the one chosen to the cursor's track, one undo step.

* **Mod slots go into free slots,** a free slot being one going nowhere (`Dest` `---`). They keep
  their order, as earlier slots reach later ones (decision 165), and every reference among them,
  and every map to them, is renumbered. With too few free, `Enter` refuses and says how many it
  needs and how many there are; `!` (`:patch!`) puts them over the last slots instead, the status
  line naming each slot replaced.
* **Macros go into free macros,** a free macro being one with no maps, the same way: in order,
  refused when there are too few, `!` taking the last ones. A macro already mapped is never merged
  into.
* **Generators go onto the phrase under the cursor,** each on its column over the phrase's loop,
  judged as `S` judges a column (decision 268): an empty Note column is generated, notes there
  are reshaped (an `Acid` keeps their rhythm), a column already live is reshaped in place. An FX
  generator goes onto the first FX column that holds its command, else the first empty one, and is
  refused with none. Other phrases are left alone; `:patch` again on another phrase adds the
  generators there, its macros already mapped by column.
* **A map to a setting the track lacks** (an `Acid`'s `Cutoff` on a `VA`) is kept and does
  nothing, drawn dim in the mapping view as `Acid Cutoff ---`, as a mod slot's destination is
  when its effect is gone (`target.rs`'s rule). It takes hold if the track becomes an `Acid`.
* **The sound is never touched.** Applied to a `VA` and then an `Acid`, the Turing Machine plays
  the same melody through two different sounds.

**Mockup 4:** `:patch` on a track with two free macros, the cursor on the Turing Machine, which
needs three.

```
󰙽 Track 01 TB   Sound  Mod  FX  Gen   Phrase 00                                                                              CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Macros ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Hits      9900 ▕█████▍   ▏   Rotate    2000 ▕█▏       ▏   Accent    5000 ▕██▉      ▏   Slide     3800 ▕██       ▏                                 │     Main    
│                                                                                                                                                    │    -9 -8    
│  Cutoff    7400 ▕████▏    ▏   Space     4000 ▕██▎      ▏   Macro 7   0000 ▕         ▏   Macro 8   0000 ▕         ▏                                 │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│  Hits → Note Hits 3–13                                                                                                                             │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Acid  Euclid · Note 00–0F ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  From      C-2  ▕         ▏   To      ╭─ Patches  1/6 ─────────────────────────────────────────────────────╮                                    │  │             
│                                       │ >                                                                  │  ●⠒                            ⠒⠒  │  │             
│  Octave   ›20%  ▕█▊       ▏   Accent  │  › Turing Machine   1 slot, 3 macros          generative pitch     │                       ⠒⠒           │  │             
│                                       │    Deja vu          1 slot, 2 macros          generative pitch     │           ⠒⠒                       │  │ -12         
│  Pitches   Roll               Hits    │    Kria             2 slots, 3 macros         generative pitch     │        ⠒⠒       ⠒⠒       ⠒⠒        │  │             
│                                       │    Euclid           Note generator, 2 macros  generative rhythm    │────────────────────────────────────┘  │     ▅▅ ▃▃   
╰───────────────────────────────────────│    Accents          Vel generator, 2 macros   generative accent    │───────────────────────────────────────╯     ██ ██   
╭─ Wave · FX1 FREQ lane 0C ─────────────│    Acid line        Note generator, 5 macros  generative 303       │───────────────────────────────────────╮ -18 ██ ██   
│                                       ╰────────────────────────────────────────────────────────────────────╯────────────────────────────────────┐  │     ██ ██   
│  From      30   ▕█▊       ▏   To        A0   ▕█████▋░░ ▏   Shape     Sine                     │    ⣠⠖⠳⠒⡀                   ⣠⠖⠳⠒⡀                │  │     ██ ██   
│                                                                                               │ ⢀⡴⠋⠁   ⠙⠦⣄              ⢀⡴⠋⠁   ⠙⠦⣄              │  │     ██ ██   
│  Rate      x1                 Phase     0°   ▕         ▏   Run       1                        │⠞⠁        ⠈⠓⢤⡀        ⢀⡤⠞⠁        ⠈⠓⢤⡀        ⢀⡤ │  │ -24 ██ ██   
│                                                                                               │             ⠙⠲⣄   ⢀⣠⠞⠁              ⠙⠲⣄   ⢀⣠⠞⠁  │  │     ██ ██   
│                                                                                               │               ⠈⠑⢦⠴⠋                   ⠈⠑⢦⠴⠋     │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Empty ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  K  make a column of this phrase live                                                                                                              │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Patch, Turing Machine  󰐊  126 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                
Needs 1 free mod slot and 3 free macros; this track has 2 macros free, so Enter says so and ! puts it over macro 6                                                 
```

### 5.4 Patches and #157's track presets

**Recommended: a patch is a preset kind of its own, in the same browser as #157's sounds and
effects,** and a sound preset holds a patch inside it.

* **Folders:** `patches/` beside `sounds/` and `effects/` in the preset folder.
* **A sound preset is sound plus patch:** loading one replaces the track's sound, mod slots and
  macros, as #157 has it, which is the author's "device". Loading a patch adds behaviour and
  replaces nothing unless `!` says so.
* **`:patch save` on a track** saves only its behaviour; `:preset save` saves both.
* **Tags (#370):** a patch file stores its tags, as presets do from their first version. The kind
  is a built-in tag, `patch`, beside `sound` and `effect`, so `#patch` lists them all. Shipped
  patches carry `generative` and what they act on, `pitch`, `rhythm` or `accent`. The picker's
  row also says what a patch needs (`1 slot, 3 macros`, `Note generator`), which is what decides
  whether it fits.

**The other way,** a patch as part of the sound preset's file rather than a kind of its own,
would leave no way to add a Turing Machine to a track whose sound you like, which is the request.

### 5.5 Shipped patches, from the tutorial's songs

| Patch | From the song | Its parts | Its macros |
|---|---|---|---|
| **Turing Machine** | `turing-machine` | a Steps slot in `Note` mode on `Pitch in scale`, 8 values, `Depth` `0C`, `Mutate` `30%` | `Change` → `Mutate` 0% to 100%; `Length` → `Length` 2 to 16; `Range` → `Depth` `00` to `18` |
| **Deja vu** | `deja-vu` | a Steps slot in `Note` mode on `Pitch in scale`, `Mutate` `12%` | `Deja vu` → `Mutate` 100% to 0%, so up is more memory, as the module's knob; `Spread` → `Depth`. `:order random` is the phrase's, so the patch's description says to set it. |
| **Kria** | `kria` | two Steps slots in `Free`: pitch on `Pitch in scale`, length 5; octave on `Pitch`, length 3 with an `0C` | `Pitch loop` and `Octave loop` → each `Length`; `Octave rate` → the octave slot's `Rate`. The song's `LEN` lane is a phrase's; it comes as an FX generator, a `List` of `LEN` on a lane of 4, if question 10 says patches carry generators. |
| **Euclid** | `euclid` | a Note generator, `Alternate` `C-2` and `C-3` on `Euclid` 7 of 16 | `Hits`, `Rotate` |
| **Accents** | `live-generators` | a Velocity generator, `7F` on `Euclid` 5 | `Hits`, `Rotate` |
| **Acid line** | `acid-lines` | a Note generator, `Acid` from `C-2` to `C-3` | `Density`, `Notes`, `Octave`, `Accent`, `Slide` |

**Not patches:** `fugue` is four tracks' speeds, orders and transposes, more like an Octatrack
part than a patch; `metropolix` is in its rows (gates, `ACU`, `ARP`) and its order, so at most its
arpeggio makes a small `Arp` patch (`Rate`, `Mode`, `Oct`).

---

## 6. The Roto

* **`Macros` is the track's first device,** before the machine: page 1 is the eight macros, knob
  1 to 8 as numbered, named by their macros, cut to 12 ASCII characters. An empty macro is a gap,
  so its knob is unmapped and dark. The colour is modulation's mauve for all eight, as a macro is
  modulation by hand; a macro whose every map is in one family could take that family's colour,
  but one rule is easier to read.
* **A turn** is a `SetMacro`, one undo step a touch (decision 160), and the motors follow a lock or
  modulation as they follow the mixer's steps.
* **Generators** come after the Mod page's devices, one each, laid out by fill from §3.3's table:
  page 1 the fill's settings in their musical order, the `Where`'s on the last knobs (`Hits` and
  `Rotate` as knobs 7 and 8, as an effect's `Mix` and `Gain` are). The fill and the `Where` are
  never on the Roto (the app builds, the Roto plays), so `K` stays the app's. `Pitches` is a
  two-step knob, as a live device has no buttons.
* **`r` on the Roto** would want a button, and live devices have none (decision 161), so rolling
  stays in the app for now.

---

## 7. Generators in the engine

### 7.1 The rule that holds it together

**At rest, what's heard is what's written.** With nothing moving a generator's settings (no lock or
modulation on them or their macro, no new seed each pass), the engine's render of a pass is,
cell for cell, what the phrase holds. So:

* songs render as they do now, and a test can prove it for every fill;
* the phrase view, copying, `Space g a` and every other edit keep working on what's written;
* the engine only needs to render a generator when something reaches it; otherwise it reads the
  phrase as it does today.

That's `Mutate`'s rule too: the song holds the written values, and the engine holds what it drew.

### 7.2 What moves into vt-engine

Nothing moves; the render is shared. (Built in #621 as decision 287 says, which notes where it
differs from this: a cell is 32 bytes, as a `Follow` copying FX carries three slots, and the riff
holds `MAX_ACID_NOTES + 1`.)

* **In `vt-model`:** the render becomes `shape::render_into`, which fills a caller's fixed buffer of
  cells and allocates nothing. `Generator::render` and `shape::written`, on the edit thread, become
  wrappers round it. So the panel, the phrase and the engine run the same code, and can't differ.
* **A cell** is a small `Copy` value of what the generator owns on a row (`GenCell`: a note with
  its velocity and gate for notes of its own or `Acid`, a velocity, a nudge, a gate cell, or an FX
  slot), never a `Step`, whose FX are a `Vec`.
* **What changes in the code to get there:**
  * `targets` and `euclid` build `Vec`s; they become a bitset of the span's rows
    (`MAX_PHRASE_STEPS` bits, 512 bytes) with Bjorklund's algorithm done in place;
  * `Acid`'s riff becomes a `[u8; MAX_ACID_NOTES]`;
  * a ramp's and a wave's values are already worked out a row at a time from the row's place.
* **In `vt-engine`:** a `GenRun` for each live generator of the phrase a track is playing, with:
  * its cell buffer for the span, and the pass and inputs it was rendered for;
  * the generator's place in its phrase and the version it came from.
* **The read path:** where the engine reads a row (`fire_track`, `row_ahead`, `row_nudge`,
  `chosen_ahead`, and the previews), it takes the row's step and lays the run's cell over it for
  the column the generator owns. A small `RowView` of the step and its cells does that in one
  place.

### 7.3 Passes and seeds

* **A pass** is each time the phrase starts on its track, counted from its place in the chain, as
  `chain_pass` is (`place / chain_steps`): the chain's time round and the entry within it. A
  generator on a lane counts its lane's loops instead.
* **Positional, not a stream.** `Mutate`'s draws are a stream from the top, so starting in the
  middle hears something else than playing through. A generator's pass is worked out from where
  the song is, so a render from bar 33 hears pass 8 as playing from the top would.
* **The seed** is the `Shape`'s while nothing asks for more, so every pass at rest is the written
  one. A new setting, `Vary`, would give each pass a seed of its own, `mix(seed, pass)`, the way
  `track_seed` mixes rather than adds (question 9).

### 7.4 What's sampled when

A generator's settings, as heard, are the `Shape`'s with its macros' maps put in, then any
modulation and locks reaching them. When are those read?

| | Once a pass | Each row | Each row, rendered as they change (recommended) |
|---|---|---|---|
| What it means | read at the pass's first row; the whole pass follows | read as each row is read; each row's cell from the settings then | the same as each row, by keeping the last render and doing it again when an input changed |
| A lock on row 8 | heard next pass | heard from row 8 | heard from row 8 |
| An LFO on `Hits` | the pattern changes at the bar line | the pattern moves under the playhead, as a Euclid module's under CV | the same |
| A turn by hand | heard at the next pass | heard at once | heard at once |
| Cost | one render a pass | a render a row | a render a pass, plus one each time an input moves, at most once a row |

**Recommended: each row, as a cache.**

* A lock is a row's, and a turn should be heard now, so the settings are read as each row is read
  (or looked ahead at, for a nudge or a groove).
* A row's cell isn't always its own business: a ramp's values hang on its ends, Euclid's hits on
  how many rows there are, `Acid`'s riff on the notes drawn before. So the run renders the whole
  span and keeps it, and the key it keeps beside it is the inputs it rendered from, all `u8` and
  `u16` after quantising. A row read with the same inputs takes its cell from the buffer; with
  different ones, the span is rendered again first.
* So an LFO slow enough to move `Hits` once a bar costs one render a bar, and one moving it every
  row, one a row.
* **Bar-quantised changes,** Elektron's pattern-change feel, could be a later choice per
  generator; nothing here prevents it.

### 7.5 What the phrase view shows

The phrase holds what's written; the engine plays something else whenever a generator moves. What
should you see?

* **The grid shows what's written.** It's what you edit, copy and save, and the cursor always means
  the cell as written. A hand edit to a cell a generator owns applies it as now (decision 180).
* **The playhead's row shows what's heard.** It's the one row that's about now, and a tracker's
  playhead row has always meant "this is playing". A cell of a moving generator on it draws the
  heard value, in the playhead's colour, as any cell on that row is.
* **The overview shows what's heard,** for every track: it's already read-only and about the whole
  song at a glance.
* **The title says it moves:** a live column whose generator is being moved by something gets a
  `~` after its name, `󰁨 Note~,FX1`, so a written column that doesn't match what you hear is never
  a surprise.
* **The status line on a moving cell** says both: `C-2 written; D#2 heard on pass 3`.
* **A heard view,** the whole last-heard pass drawn in place, read-only, behind a key, is for later
  if the playhead row isn't enough.

### 7.6 Keeping what was heard

As decisions 269 and 270 keep `Mutate`'s draws and the pitch slots moved:

* **The engine reports** each row's heard cells as the row is first heard, for the track watched,
  and only where they differ from what's written, on a queue of its own made ahead
  (`GEN_HEARD_QUEUE`, 256, with what doesn't fit dropped), as `NotePitch` is. Each report carries
  the version, the phrase, the generator's place, the row and the pass.
* **The app keeps** the latest for each row, for the version it came from.
* **`Space g a` on a moving live column** applies it with what was heard: each row as last heard,
  and rows not heard yet as written, the status line counting both. Stopped or at rest, it's
  today's apply.
* **`Space g A`** in the phrase view, today's "write in what the pitch slots moved", writes in what
  the moving generators were heard as too, applying them, so it's the one key for "make the phrase
  what I just heard".
* **Keeping a seed:** with `Vary` (question 9), `Space g a` on the Gen page would rather keep the
  pass's seed as the `Shape`'s and turn `Vary` off, leaving it live: the Turing Machine's lock
  again.

### 7.7 Real-time safety

* **No allocation on the audio thread.** The render fills buffers made ahead. The engine's
  `rt_guard` test counts allocations in a block; each fill gets a test that renders a span under
  it.
* **Where the buffers come from:** each `Published` version is built on the edit thread, as its
  bank and `lead` are now. It gains a `GenScratch` sized for its biggest span, which the engine
  takes as it takes the version, and gives back with it on the queue that already returns old
  versions to be dropped off the audio thread.
* **No editing the song.** What's heard is in the run and the reports; only the app writes.
* **Bounded work.** A render is of one generator's span, at most 4,096 rows, and happens at most
  once a row read. A test times the worst case, a 4,096-row `Acid` rendered every row.
* **Looking ahead:** `Published::lead` is how early a step is worked out, from the earliest nudge
  in the song. A Nudge generator a macro or slot can move counts with the earliest its range
  allows, not what's written.

### 7.8 What it costs

| | Work | Memory | Code |
|---|---|---|---|
| A generator at rest | nothing: no run renders, the phrase is read as now | its run, empty | |
| A moving generator | a render a pass, and one each time its inputs move; a 16-row span is a few microseconds | its cells, 4 to 6 bytes a row (96 bytes for 16 rows), in `GenScratch` | |
| Macros | a multiply-add a mapping a block | the maps, in the song | |
| Reports | one queue entry for each row heard differently | 256 entries made ahead | |
| Undo history | a `Track` grows by its macros: about 50 bytes a mapping and its names | | |
| Building it | | | stage (a) M to L; (b) L; (c) L, mostly `render_into` and the read path |

---

## 8. Plan: proposed issues

Each stage is useful alone.

### Stage (a): the Gen page

| # | Proposed issue | Depends on | Size |
|---|---|---|---|
| 1 | **Generator settings declared:** `shape/params.rs`'s `GenParam` table, `exists_for`, the new units; the shape panel's rows read from it. Nothing visible changes. | none | M, 300–400 |
| 2 | **The Gen page:** `Page::Gen`, the phrase under the cursor with `Phrase nn` and `▶ nn` in the title, a device a live column with its title controls and grid, `=`/`-`/`i`/`K`/`r`, one undo step a touch, `Space g a`, `dd`, `Enter` to the panel, the `Empty` device; guide, manual and a decision. | 1 | L, 600–800 |
| 3 | **The Gen page's panels:** the L preview of each column, the playing dot. | 2 | M, 300–400 |
| 4 | **Generators on the Roto:** a `DeviceLayout` per fill from the table, after the Mod page's devices. | 1, 2 | S–M, 200 |

### Stage (b): macros

| # | Proposed issue | Depends on | Size |
|---|---|---|---|
| 5 | **Macros in the model and the registry:** `Track::macros`, `Owner::Macro` with `MAC1`–`MAC8`, the mod slots', accumulator's and arpeggio's own settings as unlabelled entries, the actions; saved with defaults, no format bump. | none | M, 400–500 |
| 6 | **Macros in the engine:** a macro worked out each block (value, lock, modulation) and written as its targets' held values; mod slots read through a mapped copy; tests that a turn, a `MAC` lock and an LFO on a macro move every target within its range, and nothing allocates. | 5 | M–L, 500 |
| 7 | **Mapping and the band:** the Macros band on the Gen page, `M` and its picker, `gm` and the mapping view, range bands on mapped cells, a mapped setting refusing a turn, `:macro` naming. | 2, 5 | L, 600–800 |
| 8 | **Macros onto generators by hand:** the heard shape, rerendering a track's live columns as a macro turns, the dim `by hand` rows. | 1, 5, 7 | M, 300 |
| 9 | **Macros on the Roto:** the `Macros` device first, named knobs. | 5, 7 | S, 150 |

### Stage (p): patches, after #157

| # | Proposed issue | Depends on | Size |
|---|---|---|---|
| 10 | **#157 holds the macros:** a sound preset saves and loads `Track::macros` with the rest. | #157, 5 | S, folded into #157 |
| 11 | **Patches:** the kind, `:patch save` with its list, `:patch` and `:patch!` with the recall rules, renumbering slots and maps, the `patch` tag. | 10, #370 for tags | L, 600–800 |
| 12 | **Shipped patches** from the tutorial's songs, and a section in the tutorial on each. | 11 | S–M |

### Stage (c): generators in the engine

| # | Proposed issue | Depends on | Size |
|---|---|---|---|
| 13 | **An allocation-free render:** `shape::render_into` and `GenCell`, the bitset Euclid, `Acid`'s fixed riff; the edit thread's render a wrapper; a property test that it writes exactly what `written` did for every fill, column and many seeds; `rt_guard` tests. | 1 | L, 600–800 |
| 14 | **The engine renders moving generators:** `GenRun`, `GenScratch` in `Published`, `RowView` on every read, positional passes, the cache keyed by inputs, `lead` from a Nudge generator's range; tests that a song at rest renders sample for sample as before. | 13 | L, 800–1000 |
| 15 | **Modulation and locks reach generators:** macros' locks and modulation onto generator maps; the heard queue; the playhead row, the overview, the title's `~` and the status line; `Space g a` and `Space g A` keeping what was heard. | 6, 8, 14 | L, 600–800 |
| 16 | **`Vary`,** if question 9 says yes: a new seed each pass, and `Space g a` on the Gen page keeping one. | 14 | S–M, 250 |

**Later, as issues with no milestone:**

* direct `Dest` entries and locks on generator settings, if macros aren't enough (question 6);
* a generator's on/off switch, playing what's under it, once stage (c) makes it cheap;
* Macro Variations, snapshots of the eight macros, recalled by key or on the Roto;
* a song-wide macro and crossfader after the Octatrack's scenes;
* a follow mode for the Gen page and the phrase view;
* bar-quantised generator changes;
* a roll button on the Roto, if a stored setup per fill is worth it.

**Order:** 1 → 2 → 3, 1 → 4; 5 → 6, then 7 → 8 → 9; #157 → 10 → 11 → 12; 1 → 13 → 14 → 15 → 16.
Stage (a) and issues 5 and 6 can run side by side; 13 can start as soon as 1 lands, as it changes
no behaviour.

---

## 9. Open questions for the author

1. **Which phrase does the Gen page show?** Recommended: the phrase under the cursor, named in the
   title with the playing one beside it when they differ; macros reach the playing one from stage
   (c). The alternative is following the playing phrase, which moves devices under the hand.
2. **Where does the Gen page go?** Recommended: fourth, after FX, so nothing moves. It could go
   between Mod and FX, beside the other page about how a track plays rather than how it sounds.
3. **Absolute or relative macros?** Recommended: absolute, as Live's, so a macro's knob shows the
   setting's real value, and min above max turns it the other way. tracker-ce's are relative (an
   amount added to the setting's own), which lets each phrase's generator keep its own `Hits` under
   one macro; with absolute maps, every phrase's generator on that column takes the macro's.
4. **Turning a mapped setting directly:** refused, as Live does (recommended), or the key turns its
   macro instead, moving the macro's other settings with it?
5. **`MAC1` to `MAC8` rather than `MC1` to `MC8`?** Recommended: `MAC`, to keep every label four
   cells and the registry's test as it is.
6. **Do generator settings get locks and destinations of their own, or only through macros?**
   Recommended: only through macros at first. Their own would need about 130 registry entries (7
   columns of up to 19 settings) and labels that name a column (`HITV` for the Velocity
   generator's `Hits`?), which crowds `/` and the `Dest` list; a macro does the same in one more
   step and reads more plainly on a row (`MAC1 80`).
7. **When are a generator's settings read in stage (c)?** Recommended: each row, as a cache that
   renders again only when an input changed, so a lock is heard on its row and a turn at once.
   Once a pass is simpler and quantised to the phrase.
8. **What does the phrase view show while a generator moves?** Recommended: what's written, with
   what's heard on the playhead's row and in the overview, a `~` in the title and both in the
   status line. A read-only "heard" view of the whole pass is for later.
9. **Should a generator be able to roll each pass?** Recommended yes, as `Vary`: `Once` (today),
   `Each pass` or every 2, 4 or 8 passes, each pass's seed from the pass, so a render repeats; it's
   a generator's `Mutate`, and a natural macro (`Change`).
10. **Do patches carry live generators?** Recommended yes, keyed by column and put on the phrase
    under the cursor, as `S` would; without them, the Euclid, Accents and Acid line patches can't
    exist, and Kria loses its length lane.
11. **When a patch doesn't fit:** refuse and say what it needs, with `!` to put it over the last
    slots and macros (recommended), or always ask?
12. **Patches and #157:** a kind of their own in the same browser, with a sound preset holding a
    patch inside it (recommended), or only full track presets?
