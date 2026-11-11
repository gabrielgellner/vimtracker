# The track pages as devices: Sound, Mod and the fixed tracks

**Status:** approved by the author on 2026-11-10 as milestone M29, after M28's FX page
([`fx-page.md`](fx-page.md)). Their answers to §6 are under it, as **Decided**. Built in #380 to
#387, decisions 144, 145, 149 and 151 to 155, and the `track-capacity` branch below merged as decision
138. The study is kept as it was written, so its "now" is the pages before #380; the design's
[Architecture](vimtracker.md#architecture) describes them as built.

A design study for the user's request of 2026-11-10: take the FX page's new standard (option A of
[`fx-page.md`](fx-page.md), milestone M28, #374–#377) to the other track pages, and see what else we might want.
No code is changed.

**What it assumes:**

* **Four FX slots, three LFOs and three envelopes per track.** These come from the `track-capacity`
  branch (commit `27809f4`, "a track has four FX slots, three LFOs and three envelopes"), which was
  read for this study but hasn't merged yet. It only makes the row tables `static` and adds
  `Envelope::unassigned()` for `Env 3`. Nothing in it conflicts with this study.
* **Trim on every track** (#372, decision 136).

**The recommendation in one paragraph.** Every track page becomes a rack of the FX page's devices:

* the same rounded frame, 9 rows tall;
* the same title anatomy: identity, then a toggle if the device can be switched out, then its title
  controls;
* the same three-row settings grid;
* the same right-aligned, faint-boxed graphic panel, drawn smooth at the screen's real pixels.

**The Sound page** is four full-width devices: Machine, Filter, Output and Play. **The Mod page** is
two columns of half-width devices: Env 1–3 beside LFO 1–3, with the Accumulator and Arpeggio
underneath. **Each fills the 36 rows inside the Keys box exactly**, as the FX page does. Groups,
returns and the master use the same Output device, plus a read-only device listing what feeds them.
The mixer and sample page borrow only the title rule and the toggles. The frame, grid, panel and
plot code is built once in M28, written so that FX is only its first user.

---

## 1. The standard

This is the reference every track page follows. It's distilled from the FX study, and new items are
marked **(new)**.

### 1.1 The screen budget at 163×42

| | Normal | Keys box |
|---|---|---|
| View | 150×40 (the meter takes 13 columns, the status line 2 rows) | 148×38 inside the yellow frame |
| Header | row 0: title and page tabs, row 1 blank | rows 1 and 2 |
| Content | **rows 2–37: 36 rows** | **rows 3–38: 36 rows** |
| Full-width device | 150 | 148 |
| Half-width device (new) | 74 + 2 gap + 74 | 73 + 2 + 73 |

* **Four device rows of 9 fill 36 rows**, so every page that keeps to four device rows fits in Keys
  mode with no change.
* **Grids are anchored left and panels right.** The 2 columns that Keys mode takes from the width
  only move the panels, never the settings.
* **Past four device rows** a page scrolls by whole devices, as the FX study's §6 has it. No page
  here needs to.

### 1.2 Device frame and title bar

```
╭─ identity  [toggle]  control  control ─────────── … ──╮
│                                                       │   rows: blank, settings 1, blank,
│  settings 1 …                       ┌panel──────┐     │   settings 2, blank, settings 3, blank
│                                     │  image    │     │
│  settings 2 …                       │  5 rows   │     │
│                                     │           │     │
│  settings 3 …                       └───────────┘     │
│                                                       │
╰───────────────────────────────────────────────────────╯
```

* **Frame:** `widgets::window_border()`, rounded. `SURFACE1` normally, and `OVERLAY2` when the cursor
  is inside the device.
* **The title is three parts, in this order:**
  * **Identity:** dim, and not a control. It says which device this is: the slot digit on FX (`1`),
    the kind on the Sound page (`Machine`, `Filter`, `Output`, `Play`), and the name on the Mod page
    (`Env 2`, `LFO 1`).
  * **Toggle:** only when the device can be switched out. It's `glyphs::SWITCH_ON` U+F205 in `GREEN`
    or `glyphs::SWITCH_OFF` U+F204 in faint. It isn't a cursor stop: `b` flips it from anywhere in
    the device.
  * **Title controls:** what changes the device's character, in the value colour and bold when the
    device has the focus. `K` picks each one. They are the effect and its type on FX, the machine
    and its type on Sound (new), the filter's type and mode (new), and a modulator's destination
    (new). The cursor reaches them with `k` from the first settings row.
* **Every device stays 9 rows,** empty or not, so nothing moves when its contents change.

### 1.3 The settings grid

* **Shape:** up to four columns by three rows, with a blank line between rows. **Each device states
  its own grid** (`grid() -> [[Option<usize>; 3]; 4]`), so meaning decides the layout, never the
  order of the settings. Holes are allowed, and the cursor skips them.
* **Cells:**

| Cell | Layout | Width | Used for |
|---|---|---|---|
| Bar cell | name 10 · value · bar `▕9▏` · gap 3 | 27 when the value is 3 (FX), **29** when it's 5 (Sound: `FF00 `, coarse and dim fine) | anything with a range |
| Word cell | name 10 · word across the value and bar · gap 3 | same as the column | `Straight`, `1/16`, `One-shot` |
| Choice | name 10 · value · name in the bar's place, mauve | same | `00   whole`, a 6-op patch |
| Two-state | name 10 · toggle glyph | same | `Reset`, `Retrig`, `Ping-pong`, `Limiter` (#375's rule) |
| Compact (new) | name 10 · value 8 · gap 3, no bar | 21 | half-width devices, where the panel is the bar |

* **One value width a page.** Every device on a page uses the same value width, so columns line up
  down the page. That's 3 on FX and 5 on Sound, where settings have a fine byte that `l` reaches.
* **Bars** are `ValueBar` with `.playing(live)` and `.centred()`, 9 cells, as now, so modulation shows
  in the bar.
* **Values left at their default are dim** (`at_default`), as now.
* **Bypassed:** everything but the cursor fades (`faded`).

### 1.4 Toggles

* **One glyph pair for every on/off,** the explorer's (decision 101). In a title bar it means
  "this device is in", and in a cell it means "this two-state setting is on".
* **Not used:** `nf-md-power`, a second toggle style, or a coloured word.

### 1.5 Graphic panels

* **Placement:** aligned right with 2 columns of padding, spanning the device's inner rows 0–6. It's
  a faint square box (`image::boxed`, as decision 103 has for boxes that are part of the grid), with
  the image in rows 1–5. A caption, if any, goes in the bottom border (`└GR┘`).
* **Size classes** (5 rows each, in pixels at 15×39):

| Class | Image | Pixels | For |
|---|---|---|---|
| S | 13×5 | 195×195, square | transfer curves: Comp (FX), and later Distort |
| H (new) | 24×5 | 360×195 | half-width devices: envelopes, LFOs |
| M | 33×5 | 495×195 | EQ (FX) |
| L (new) | 49×5 | 735×195 | the Sound page's filter curve and sample thumbnail, beside a two-column grid |
| Text | 2×5, or 49×5 | n/a | GR meter, track meter: text, moving every frame for free |

* **Drawing:** at the terminal's real pixels (`Graphics::cell`), 4-bit palette PNGs with an alpha
  for each palette entry, and an anti-aliased 2-px line from coverage. It's one drawing path
  (`kitty::plot`, from #376) for every curve, replacing `line_png`'s 3-px dots.
* **Inside images:** only faint marks (`OVERLAY0`, at 40–60%), with no text labels. Numbers belong to
  the grid.
  * **Frequency plots** (filter, EQ): dotted verticals at 100 Hz, 1 kHz and 10 kHz, and a 0 dB line.
  * **Transfer curves:** −20 and −40 dB, the 1:1 diagonal, and the threshold.
  * **Time shapes** (envelopes): a baseline, and dotted verticals between the stages.
  * **Cycles** (LFOs): a centre line.
* **Shades:** three images for each panel: dim, active (the device has the focus) and faint
  (bypassed or going nowhere). Each is sent once and cached, so moving the cursor sends nothing.
* **The cost rules:**
  1. **Send only on change.** An image is sent only when what shapes it changes, compared as a key,
     as `SlotDrawing` and `FilterSettings` are now.
  2. **Motion is text or a placement, never a new image.** Meters are text. Dots and lines are
     stored images placed with `X=`/`Y=` (decision 109's technique: about 60 bytes a move).
  3. **Throttle images that follow modulation.** The filter curve under an LFO, or the comp curve
     under a modulated threshold, is redrawn at most every other frame (`SCOPE_FRAMES`, #214).
  4. **Always compress** (decision 93): a mostly empty picture comes to 2–6 KB.
  5. **A panel never repeats what's already on screen (new).** So the master gets no meter beside
     the main meter, and an LFO gets no shape glyph beside its drawn wave.

### 1.6 Focus and dim states

| State | Frame | Identity and title | Contents | Panel |
|---|---|---|---|---|
| Focused (cursor inside) | `OVERLAY2` | controls bold | normal; the cursor is the Rosewater block | active green |
| Not focused | `SURFACE1` | normal | normal | dim |
| Switched off (toggle off, or a lock) | `SURFACE1` | toggle faint | faint (`OVERLAY0`) | faint image |
| Going nowhere (new: a destination of `---` or a depth of `00`) | as focus | destination dim | dim | faint image |
| Empty FX slot | as focus | `Empty` | `K  pick an effect` | none |

**Colour stays semantic:**

* the toggle's green;
* the group colours on `Group` and on track names;
* the sample page's start teal and end red, reused in the sample thumbnail;
* the level colours in meters.

Yellow stays the only coloured frame, on the Keys box.

### 1.7 Badges

* **Modes get a badge on the status line, and nothing else does.** That's `TRACK`, `KEYS`, `PADS`,
  `SETTINGS`, `VISUAL`, and `ZOOM` (#377) later.
* **Devices carry no badges.** A panel can caption its own box (`GR`). The FX chain line's
  `▲ 2 above` (FX study §6) is a count, not a badge.

### 1.8 Not used

* Heavy or double lines.
* Filled title bars.
* Colour for focus.
* Decorative icons. Every glyph is a state or a kind (decision 104).
* Text inside images.
* Shrinking devices to fit more.

---

## 2. The pages

**How to read the mockups.** They use the FX study's conventions:

* Each block is exactly 163 wide, with the 13-column main meter and the two status rows.
* `›` is the cursor.
* Braille stands in for kitty images, which on screen are smooth, anti-aliased lines at real pixels.
* `` (`nf-fa-toggle_on`) and `` (`nf-fa-toggle_off`) are the toggle glyphs. Your browser shows them only with a Nerd Font.
* `●` is a placed dot image.
* Colour, weight and focus can't show in a fixed-width font, so each mockup says which device has
  the focus.

### 2.1 Sound page

**What isn't working now** (`track.rs`, `track_page.rs`, `track-sound.svg`):

* **Two halves with no structure.** The left half is the machine's settings, a blank, then the
  filter. The right half is everything else: Trim, Level, Pan, sends and Group (the output), but
  also Pitch, Groove, Choke, Gate, Speed, Reset and Glide (how notes play). So the right half is a
  13-row list of two unrelated groups, and the machine's settings and the filter are told apart
  only by a blank row.
* **Four row tables for one page** (`SAMPLE_ROWS`, `SYNTH3_ROWS`, `SYNTH_ROWS`, `SYNTH8_ROWS`). They
  exist only to pair the left half's rows with the right half's, so the pairing, not the meaning,
  decides what sits beside what. `Glide` ends up on a row of its own under a 4-setting synth.
* **The curves are small and coarse.** The filter curve is 24×3 cells: 144×36 dots of 3-px blocks,
  432×108 px stretched into 360×117. The sample's waveform is the same size, and shows nothing of
  `Start` or `Length`.
* **No title.** The machine, its sample and the filter's type are ordinary rows, so nothing names a
  part of the page.

**Proposed: four devices, each with its own grid.**

* **Machine.** The title is `Machine`, the machine, and its type: the setting that changes its
  character, as Comp's `Type` does on FX. That's the `Sample` for the sampler, `Wave` for Acid,
  `Patch` for 6-op, and `Chord` for Chord and Chip. Every machine's settings fit the 4×3 grid
  (Acid has 7 once `Wave` is in the title). Every grid is stated per machine
  (`Machine::grid()`), with the default going column by column.
* **Filter.** The title is `Filter`, the type and the mode, both title controls. The grid has
  `Cut`, `Res` and `Drive` (the filter) in one column, and `Low cut` and `High cut` (the utility
  filter) in the next. The panel is the curve, at size L.
* **Output.** `Trim`, `Level` and `Pan` in one column, then `Send A`, `Send B` and `Group`. Reading
  down, that's the signal's order, as decision 136 wants. The panel is the track's own level meter.
* **Play.** How the track plays its notes: `Pitch`, `Glide` and `Gate`, then `Speed`, `Groove` and
  `Reset`, then `Choke` and `Velocity` (decision 166). There's no panel, so its space stays empty, as an FX device with no
  graphic does.

**A four-setting drum synth** (Kick). The cursor is on the machine in the title, so Machine has the
focus. The open filter draws flat along 0 dB, with only the faint gridlines (not shown) around it.

```
 Track 01 Kick   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Machine  ›Kick ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Punch     A000 ▕█████▋   ▏   Accent    8000 ▕████▌    ▏                                                                                           │     Main    
│                                                                                                                                                    │    -9 -8    
│  Tone      3000 ▕█▋       ▏                                                                                                                        │  +6       ▔▔
│                                                                                                                                                    │             
│  Decay     9000 ▕█████    ▏                                                                                                                        │             
│                                                                                                                                                    │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Filter  LP  12 dB ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  Cut       FF00 ▕█████████▏   Low cut   00   ▕         ▏                                      │                                                 │  │             
│                                                                                               │                                                 │  │             
│  Res       0000 ▕         ▏   High cut  FF   ▕█████████▏                                      │⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠑│  │             
│                                                                                               │                                                 │  │ -12         
│  Drive     1800 ▕▊        ▏                                                                   │                                                 │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                               ┌─────────────────────────────────────────────────┐  │     ██ ██   
│  Trim      8000 ▕    │    ▏   Send A    0000 ▕         ▏                                      │                                                 │  │     ██ ██   
│                                                                                               │ L ██████████████████████████████████  ▏         │  │     ██ ██   
│  Level     E000 ▕███████▉ ▏   Send B    1000 ▕█▏       ▏                                      │ R █████████████████████████████████▋  ▏         │  │ -24 ██ ██   
│                                                                                               │         -48     -36     -24     -12       0     │  │     ██ ██   
│  Pan       8000 ▕    │    ▏   Group     G1                                                    │                                                 │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Play ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Pitch     8000 ▕    │    ▏   Speed     1/16               Choke     Off                                                                           │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Glide     Off                Groove    Straight                                                                                                   │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Gate      100%               Reset     Off                                                                                                        │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Machine  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                              
K picks a machine                                                                                                                                                  
```

**The eight-setting synth** (Acid). `Wave` has moved to the title, leaving 7 settings in a 303's
panel order: `Cutoff` and `Resonance`, then `Env mod` and `Decay`, then `Accent` and `Slide`, then
`Drive`. The cursor is on `Cut`, so Filter has the focus and its curve is green. It shows the low
cut's slope, the ladder's resonant peak at 2.6 kHz, and 24 dB an octave above it.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Machine  Acid  Saw ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Cutoff    5C00 ▕███▏     ▏   Env mod   9800 ▕█████▎   ▏   Accent    A000 ▕█████▋   ▏   Drive     3000 ▕█▋       ▏                                 │     Main    
│                                                                                                                                                    │    -9 -8    
│  Resonance B000 ▕██████▏  ▏   Decay     7000 ▕███▉     ▏   Slide     4000 ▕██▎      ▏                                                              │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│                                                                                                                                                    │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Filter  Ladder  LP24 ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  Cut      ›B400 ▕██████▎  ▏   Low cut   28   ▕█▍       ▏                                      │                                ⢀⡠⠤⡀             │  │             
│                                                                                               │                     ⢀⣀⣀⣀⣀⣀⣀⣀⠤⠤⠒⠁  ⠘⢄            │  │             
│  Res       7000 ▕███▉     ▏   High cut  FF   ▕█████████▏                                      │⣀⠤⠤⠤⠒⠒⠒⠒⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁              ⠈⠢⡀          │  │             
│                                                                                               │                                      ⠘⢄         │  │ -12         
│  Drive     2000 ▕█▏       ▏                                                                   │                                        ⠉⠢⣀⣀⣀⣀⣀⣀⣀│  │             
│                                                                                               └─────────────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                               ┌─────────────────────────────────────────────────┐  │     ██ ██   
│  Trim      A000 ▕     │   ▏   Send A    2000 ▕█▏       ▏                                      │                                                 │  │     ██ ██   
│                                                                                               │ L ██████████████████████████████▋  ▏            │  │     ██ ██   
│  Level     D000 ▕███████▎ ▏   Send B    0000 ▕         ▏                                      │ R ██████████████████████████████▋  ▏            │  │ -24 ██ ██   
│                                                                                               │         -48     -36     -24     -12       0     │  │     ██ ██   
│  Pan       8000 ▕    │    ▏   Group     Off                                                   │                                                 │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Play ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Pitch     8000 ▕    │    ▏   Speed     1/16               Choke     Off                                                                           │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Glide     Slide              Groove    Straight                                                                                                   │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Gate      50%                Reset     Off                                                                                                        │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Cut, 2.6 kHz  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                         
                                                                                                                                                                   
```

**The sampler.** The title is `Machine  Sampler  07 hat open`. `K` on the sample picks from the
bank, and `g d` opens the sample page, as on today's `Sample` row. The cursor is on `Start`.

* **The thumbnail** shows the whole slice:
  * where it starts, as a teal line, and where `Length` ends it, as a red line: the sample page's
    colours (decision 102);
  * the sound between them in the text colour, and the sound outside them dim. The braille can't
    show the dimming.
* `Choke 1` and `Group G1` show a kit hat choked by the open hat, in the drums group.

```
󱑽 Track 03 Hats   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Machine  Sampler  07 hat open ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                               ┌─────────────────────────────────────────────────┐  │             
│  Slice     00   whole         Start    ›0800 ▕▎        ▏                                      │⡀⡇⡀                                  ⡇           │  │     Main    
│                                                                                               │⣿⡇⣿⣾⣾⣤⣾⣤⣶⣤⣆⣄⣀⣠⣀⣀⡀ ⡀                  ⡇           │  │    -9 -8    
│  Play      One-shot           Length    C000 ▕██████▊  ▏                                      │⣿⡇⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡷⣿⠶⢾⡷⡷⠶⠶⠶⠶⠶⠶⡇⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶│  │  +6       ▔▔
│                                                                                               │⣿⡇⣿⢿⢿⠛⢿⠛⠿⠛⠏⠋⠉⠙⠉⠉⠁ ⠁                  ⡇           │  │             
│                                                                                               │⠁⡇⠁                                  ⡇           │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Filter  HP  12 dB ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                               ┌─────────────────────────────────────────────────┐  │  -6         
│  Cut       7000 ▕███▉     ▏   Low cut   00   ▕         ▏                                      │                                                 │  │             
│                                                                                               │                    ⢀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀                  │  │             
│  Res       2000 ▕█▏       ▏   High cut  E0   ▕███████▊ ▏                                      │                ⣀⠤⠒⠊⠁          ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠑⠒⠒⠒⠢⠤⠤│  │             
│                                                                                               │           ⢀⡠⠔⠒⠉                                 │  │ -12         
│  Drive     0000 ▕         ▏                                                                   │⣀⣀⣀⣀⣀⣀⣀⡠⠤⠒⠉⠁                                     │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                               ┌─────────────────────────────────────────────────┐  │     ██ ██   
│  Trim      8000 ▕    │    ▏   Send A    0000 ▕         ▏                                      │                                                 │  │     ██ ██   
│                                                                                               │ L ██████████████████████████    ▏               │  │     ██ ██   
│  Level     C000 ▕██████▊  ▏   Send B    0C00 ▕▍        ▏                                      │ R ███████████████████████████▍  ▏               │  │ -24 ██ ██   
│                                                                                               │         -48     -36     -24     -12       0     │  │     ██ ██   
│  Pan       9A00 ▕     │   ▏   Group     G1                                                    │                                                 │  │     ██ ██   
│                                                                                               └─────────────────────────────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Play ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Pitch     8000 ▕    │    ▏   Speed     1/16               Choke     1                                                                             │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Glide     Off                Groove    Straight                                                                                                   │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Gate      100%               Reset     Off                                                                                                        │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Start, 3 %  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                           
                                                                                                                                                                   
```

**Drum and kit tracks don't need a page of their own.**

* **A kit's sounds are sampler tracks.** The kit sets their `Level`, `Pitch` and `Choke` (decision
  70), all of which already have cells.
* **Kick, Snare and Hat are four-setting synths,** shown above.
* **Layered and variant sounds** (decision 99) resolve to one sample a track.

**Where the graphics earn their place:**

* **The filter curve, at L (49×5, 735×195 px).**
  * **Size:** about 3.4 times today's area on screen, and drawn at real pixels with an
    anti-aliased line. It has the faint frequency gridlines and a 0 dB line. The range stays
    −36 to +24 dB.
  * **Motion:** it follows the `Cut` and `Res` that are playing (decision 21). That's one image for
    each change, throttled to every other frame (§1.5). The user has accepted this cost. At 4 bits it's about 72 KB raw and a few KB compressed.
  * **If it's measured too costly** while an LFO moves it, the moving curve drops to 2 bits (four
    alpha levels) before anything else is given up.
* **The sample thumbnail, at L, beside it.**
  * **When it's sent:** once for each sample, its points, `Start` and `Length`, in up to five solid
    colours with no anti-aliasing. It's built as `waveform_spans` folds now.
  * **Why it earns its place:** it answers "what part of this sample is playing" at a glance.
    Today's waveform doesn't.
* **The track meter, in the Output panel** (text, so it moves for free).
  * **What it shows:** L and R as horizontal eighths over −60 to +6 dB, 1.5 dB a cell, in the level
    colours (`theme::level`), with a faint held peak.
  * **Why:** `Trim` is for gain staging (decision 136), and gain staging is done by watching a
    meter. Today that means going to the mixer.
  * **The tap:** it's the same report the mixer's strip reads, which is after Level and Pan. A
    tap before the fader is question 3.
* **Not added: a pan or stereo indicator.** `Pan`'s centred bar already says where the sound sits,
  and a second picture of one number wouldn't earn its room.
* **Later: a scope on a synth's Machine device.** It would show that track's own output, as Ableton's
  synths show their oscillators. It's a moving image, so it costs what the master scope does, and
  it waits until someone misses it.

**Navigation:**

* **Rows.** `j` and `k` go title, settings rows 1–3, then the next device's title, keeping the
  wanted column (FX study §5). `h` and `l` move along a row, through a value's fine part as now,
  and skip holes.
* **Devices.** `{` and `}` go to the same row and column in the previous or next device, with a
  count. So `2}` from the machine goes to Output.
* **Title controls.** `K` on the machine, its type, the filter type or the mode picks one. `g d` on
  the sample works as now.
* **What changes for users:**
  * The output is no longer one `l` away on every row; it's `2}`.
  * The filter's type and mode, and the machine's type, move to their title bars.
  * Glide, Gate and the other timing settings move to Play.

### 2.2 Mod page

**What isn't working now** (`track-mod.svg`):

* **Tables, not devices.** The envelopes and LFOs are rows of words under column headings, like a
  spreadsheet, and nothing groups an LFO's settings except its row.
* **The pictures are cut off from their settings.** The envelope diagrams are a strip at the bottom
  (28×4 cells each, 3-px dots, 504×144 px stretched into 420×156). Three of them take 90 columns,
  and the eye has to match `Env 2`'s row to the second picture.
* **LFOs have no picture,** only a shape glyph (decision 104 replaced the 4-cell images). So `Phase`
  is a number you can't see, and nothing shows where the LFO is in its cycle.
* **No way to switch a modulator out** and hear the track without it, short of setting its depth to
  `00` and remembering the old value.
* **On/off settings are words:** `Reset on`, `Retrig off`.

**Proposed:** a rack of half-width devices, with **envelopes on the left and LFOs on the right**:

* rows 2–28: `[Env 1 | LFO 1]`, `[Env 2 | LFO 2]` and `[Env 3 | LFO 3]`;
* rows 29–37: `[Accumulator | Arpeggio]`.

**Each modulator's device:**

* **Title:** its name, its toggle (question 1), and its destination as the title control. `K` picks
  the destination, as it does on `Dest` now. That's the FX title's "effect and type", because the
  destination is what this modulator *is* for.
* **Grids:**
  * **Envelope:** times down the left (`Attack`, `Decay`, `Release`) and levels down the right
    (`Sustain`, `Depth`, `Reset`).
  * **LFO:** `Shape`, `Rate` and `Depth`, then `Phase` and `Retrig`. That leaves one hole, for the
    `Fade` that #86 will bring.
* **Panels:** size H.

The mockup's states:

* The cursor is on LFO 1's `Rate`, so LFO 1 has the focus. Its wave is green, and the dot is a third
  of the way through its cycle.
* **LFO 2 is switched off,** with a faint toggle, contents and wave.
* **Env 3 is on but goes nowhere** (`---`, from `Envelope::unassigned()`), so it draws dim.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Env 1    Vol ────────────────────────────────────────────────────────╮  ╭─ LFO 1    Cut ────────────────────────────────────────────────────────╮    ⠈⠉       
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │             
│  Attack    2 ms       Sustain   70%        │  ⡦⣀                    │  │  │  Shape     Tri        Phase     0°         │    ⢀⠤⠢⢄                │  │     Main    
│                                            │ ⢠⠃ ⠉⠒⠒⠒⠤⠤⠤⠤⠤⠤⠤⠤⡀       │  │  │                                            │ ⢀⡠⠒⠁   ●⠢⣀             │  │    -9 -8    
│  Decay     300 ms     Depth     --         │ ⢸              ⠸⡀      │  │  │  Rate     ›2 bars     Retrig              │⠔⠁         ⠑⠤⡀         ⡠│  │  +6       ▔▔
│                                            │ ⡎               ⠱⡀     │  │  │                                            │             ⠈⠑⠤⡀   ⡠⠔⠉ │  │             
│  Release   80 ms      Reset               │⠔⠁                ⠈⠒⠒⠤⠤⠤│  │  │  Depth     +30                             │                ⠈⠒⠔⠊    │  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯             
╭─ Env 2    Cut ────────────────────────────────────────────────────────╮  ╭─ LFO 2    Pan ────────────────────────────────────────────────────────╮             
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │  -6         
│  Attack    1 ms       Sustain   0%         │ ⢠⡀                     │  │  │  Shape     Sine       Phase     90°        │⠒⠢⢄⡀                 ⣀⠤⠒│  │             
│                                            │ ⢸⠱⡀                    │  │  │                                            │   ⠈⠢⡀             ⡠⠊   │  │             
│  Decay     180 ms     Depth     +48        │ ⡸ ⠱⡀                   │  │  │  Rate      3/16       Retrig              │     ⠈⢆          ⢀⠜     │  │             
│                                            │ ⡇  ⠈⠢⣀                 │  │  │                                            │       ⠑⢄      ⢀⠔⠁      │  │ -12         
│  Release   50 ms      Reset               │⠜      ⠉⠒⠒⠒⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │  │  Depth     +20                             │         ⠉⠒⠤⠤⠔⠊⠁        │  │             
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Env 3    --- ────────────────────────────────────────────────────────╮  ╭─ LFO 3    L1 Depth ───────────────────────────────────────────────────╮ -18 ██ ██   
│                                            ┌────────────────────────┐  │  │                                            ┌────────────────────────┐  │     ██ ██   
│  Attack    1 ms       Sustain   0%         │ ⢠⡀                     │  │  │  Shape     Ramp       Phase     0°         │                    ⢀⣀⠤⠔│  │     ██ ██   
│                                            │ ⢸⠱⡀                    │  │  │                                            │               ⣀⡠⠤⠒⠊⠁   │  │     ██ ██   
│  Decay     200 ms     Depth     00         │ ⡸ ⠱⡀                   │  │  │  Rate      4 bars     Retrig              │         ⢀⣀⠤⠔⠊⠉         │  │ -24 ██ ██   
│                                            │ ⡇  ⠈⠢⣀                 │  │  │                                            │    ⣀⠤⠔⠒⠉⠁              │  │     ██ ██   
│  Release   50 ms      Reset               │⠜      ⠉⠒⠒⠒⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│  │  │  Depth     +40                             │⠤⠒⠊⠉                    │  │     ██ ██   
│                                            └────────────────────────┘  │  │                                            └────────────────────────┘  │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ Accumulator ──────────────────────────────────────────────────────────╮  ╭─ Arpeggio ─────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Up        7          Limit     Wrap                                   │  │  Rate      Tick          Oct       1                                   │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Down      7          Adds      Row                                    │  │  Mode      Up            Retrig                                       │ -48 ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
│  Moves     Row        Reset     Chain                                  │  │                                                                        │     ██ ██   
│                                                                        │  │                                                                        │     ██ ██   
╰────────────────────────────────────────────────────────────────────────╯  ╰────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Rate, 2 bars  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                         
LFO 1 goes round once every 2 bars                                                                                                                                 
```

**Width check in Keys mode.** A half device is 73 wide there:

* the grid takes columns 3 to 42 (two compact cells);
* the panel's 26-column box starts at 44, which leaves the 2-column gap.

The Arpeggio's `Mode` column is 11 wide, for `Up-down-inc`. It has no panel, so that's fine.

**Where the graphics earn their place:**

* **Envelopes at H (24×5, 360×195 px).**
  * **The curve:** drawn from the engine's Peaks curves, stretched in time as now, so a 1 ms attack
    still shows. Faint dotted verticals separate A, D, S and R, so each grid value has a part of
    the picture to point at.
  * **When it's sent:** only when its own settings change.
  * **Later:** a dot that rides the envelope while a note plays. It needs each envelope's stage and
    level in `LiveValues`.
* **LFOs at H.**
  * **What's drawn:** one cycle, starting at `Phase`, so the phase you set is the phase you see.
    It's drawn at full height, and flipped for a negative depth, because the sign is what you need
    to read. The depth's size is the number beside it.
  * **Why one cycle, not a bar of time:** the image then never changes with `Rate`. The dot's speed
    shows the rate.
  * **Random and Smooth** draw only the centre line, with the dot at the live value. Drawing a
    made-up staircase would say something untrue, which decision 104 avoided for the glyphs.
  * **The phase dot,** while playing: a 5-px dot image, placed with `X=` and `Y=` (a `Y` offset is
    added to `Placement`) at the LFO's position in its cycle and its output. That's about 60 bytes a
    move each frame for the three LFOs, and nothing while stopped.
  * **The data:** it needs `LiveValues.lfos: [(u16 phase, i16 value); LFOS_PER_TRACK]` for the
    cursor's track, a few lines in the engine where the LFO is already worked out. The dot is taken
    off while anything is drawn over the page, as the sample page's lines are.
  * **The shape glyph leaves the grid:** the wave is drawn beside it, so the glyph would say it
    twice (§1.5, rule 5).
* **Accumulator and Arpeggio:** no panel. They act on notes, and their numbers are the whole story.

**Navigation:**

* **Inside a device,** `hjkl` move as on the other pages: `k` from settings row 1 goes to the title,
  that is, the destination.
* **Across devices:**
  * `l` past a device's last column goes into the device on its right, to the same row.
  * `j` past the last row goes to the device below.
  * `{` and `}` go up and down one device in the same column, with a count. So `}` goes from Env 1
    to Env 2, and `3}` from LFO 1 to the Arpeggio.
* **Toggles:** `b` switches the cursor's modulator, if question 1 says yes. A `Reset` or `Retrig`
  cell flips with the keys any two-state setting uses.
* **Fixed tracks:** a return or the master has no Accumulator or Arpeggio, so its Mod page ends at
  row 28.

### 2.3 Groups, returns and the master

**What isn't working now:**

* **A group's page is a list.** `Tracks` runs down the left with its output on the right, which is
  readable but shapeless.
* **A return's or the master's page is 2 or 3 rows** in a corner of an empty screen. It gives no
  sense of what the track is, or what feeds it.

**Proposed:**

* **The same Output device** as a sound track's, with the holes left where a fixed track lacks a
  setting:
  * a group has no `Group`;
  * a return has no sends;
  * the master has no `Pan`.
* **Under it, a read-only device of what feeds the track:**
  * **A group's Tracks:** its tracks by number and name, three to a column, with their mute or solo
    glyph (decision 126), in the group's colour, and `+N more` when they run past 12.
  * **A return's Sends in (new):** every track and group sending to it, with the send's value and
    bar, which today you can only find from the mixer. It's worked out from the song, so it costs
    nothing to keep.
  * The cursor doesn't stop in either device. Jumping to a track from them can come later.
* **The master's Output** gains a `Limiter` two-state cell: the song's limiter switch, which the
  project page has now (question 2). It has no meter panel, because the main meter is ten columns
  away (§1.5, rule 5).

A group, with the cursor on `Level`:

```
󰃸 Group 1   Sound  Mod  FX                                                                                                   CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                               ┌─────────────────────────────────────────────────┐  │             
│  Trim      8000 ▕    │    ▏   Send A    0000 ▕         ▏                                      │                                                 │  │     Main    
│                                                                                               │ L ██████████████████████████████████▋ ▏         │  │    -9 -8    
│  Level    ›D000 ▕███████▎ ▏   Send B    1800 ▕▊        ▏                                      │ R ██████████████████████████████████▍ ▏         │  │  +6       ▔▔
│                                                                                               │         -48     -36     -24     -12       0     │  │             
│  Pan       8000 ▕    │    ▏                                                                   │                                                 │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Tracks ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                                                                                    │  -6         
│  01 Kick                    04 Snare                                                                                                               │             
│                                                                                                                                                    │             
│  03 Hats                    05 Perc 󰝟                                                                                                              │             
│                                                                                                                                                    │ -12         
│                                                                                                                                                    │             
│                                                                                                                                                    │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
                                                                                                                                                       -18 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -24 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -36 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -48 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Level, -3.0 dB  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                       
                                                                                                                                                                   
```

Return B, with its sends coming in:

```
󱞳 Return B   Sound  Mod  FX                                                                                                  CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                               ┌─────────────────────────────────────────────────┐  │             
│  Trim      8000 ▕    │    ▏                                                                   │                                                 │  │     Main    
│                                                                                               │ L ████████████████████████   ▏                  │  │    -9 -8    
│  Level    ›C800 ▕███████  ▏                                                                   │ R ████████████████████████▋  ▏                  │  │  +6       ▔▔
│                                                                                               │         -48     -36     -24     -12       0     │  │             
│  Pan       8000 ▕    │    ▏                                                                   │                                                 │  │             
│                                                                                               └─────────────────────────────────────────────────┘  │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ Sends in ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                                                                                    │  -6         
│  03 Hats   0C00 ▕▍        ▏   G1        1800 ▕▊        ▏                                                                                           │             
│                                                                                                                                                    │             
│  04 Snare  3000 ▕█▋       ▏                                                                                                                        │             
│                                                                                                                                                    │ -12         
│  06 Pad    6000 ▕███▍     ▏                                                                                                                        │             
│                                                                                                                                                    │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
                                                                                                                                                       -18 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -24 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -36 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -48 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Level, -1.5 dB  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                       
                                                                                                                                                                   
```

The master: just the top rows, as nothing else changes.

```
󰓃 Master   Sound  Mod  FX                                                                                                    CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ Output ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Trim      8000 ▕    │    ▏   Limiter                                                                                                             │     Main    
│                                                                                                                                                    │    -9 -8    
│  Level    ›E000 ▕███████▉ ▏                                                                                                                        │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│                                                                                                                                                    │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
                                                                                                                                                                   
                                                                                                                                                        -6         
```

**Navigation:** as on a sound track's Sound page. `{` and `}` have nowhere to go, as there's only one
device with cursor stops.

### 2.4 The mixer: borrow the title rule, nothing more

**The mixer is strips, not devices.** It's dense on purpose: 19 strips at 150 columns. Frames
would cost a strip or more. What it can share is the title-bar language, and the rule costs no
room:

* **Row 1, which is blank now,** puts each section's name on a rule over its strips, in the frames'
  `SURFACE1`. The cursor's section gets `OVERLAY2`.
* **Everything else stays,** including mute and solo as their own glyphs. They're states with
  meaning, not on/off switches (decision 104).

```
Mixer                                                                                                                                                  ⢀⡤⣄   ⢀⡀    
       ─ Tracks ──────────────────────────────────────────────   ─ Groups ──────────────────   ─ Returns ──────────                                    ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
       01 BD  02 Bas…03 HH  04 SD  05 Prc 06 Pad 07 Ld  08 Key   󰃸 G1   󰃸 G2   󰃸 G3   󰃸 G4     󱞳 A    󱞳 B    󰓃 M                                          ⠈⠉       
       G1            G1     G1     G1                            01 03-…                                                                                           
                                                                                                                                                           Main    
```

### 2.5 The sample page: two small touches

* **Toggles:** `Reverse` and `Snap` show the toggle glyphs, by #375's two-state rule.
* **Settings rows:** they keep their one-line form, but take the dim name / plain value / dim
  default styling of the grid.

The waveform is the page, so no frames go round its settings: a 9-row device would take 9 of the
waveform's 28 rows.

---

## 3. Cross-page consistency

| Page | Device | Identity | Toggle | Title controls | Grid | Panel |
|---|---|---|---|---|---|---|
| FX | slot | `1`–`4` | bypass | effect, type | `Insert::grid()` | S curve with GR meter, M EQ, GR meter alone |
| Sound | Machine | `Machine` | none | machine, type (Sample, Wave, Patch, Chord) | `Machine::grid()` | L sample thumbnail (sampler only) |
| Sound | Filter | `Filter` | none | type, mode | Cut/Res/Drive, Low/High cut | L curve |
| Sound | Output | `Output` | none | none | Trim/Level/Pan, sends/Group (master: Limiter) | text track meter (none on the master) |
| Sound | Play | `Play` | none | none | Pitch/Glide/Gate, Speed/Groove/Reset, Choke/Velocity | none |
| Sound (group, return) | Tracks, Sends in | the name | none | none | read-only list | none |
| Mod | Env *n* | `Env n` | on/off (Q1) | destination | times, levels | H shape |
| Mod | LFO *n* | `LFO n` | on/off (Q1) | destination | Shape/Rate/Depth, Phase/Retrig/(Fade) | H cycle with phase dot |
| Mod | Accumulator, Arpeggio | the name | none | none | 2×3, 2×2 | none |

**The same keys everywhere:**

* `hjkl` in the grid, and `k` into the title;
* `{` and `}` between devices;
* `K` picks whatever is under the cursor;
* `b` toggles the device;
* `gg` and `G` go to the first and last rows;
* `/` jump labels on every cell, the title's included.

**Yes, Machine and Filter are devices.** They are what Ableton calls an instrument and an Auto
Filter, and the title bar is where their identity (a machine and its type, a filter type and its
mode) belongs.

**Yes, LFOs and envelopes are devices,** with a title bar holding the destination and, if wanted, a
toggle. The destination reads as the device's purpose, `LFO 1 … Cut`, the way an effect's type
does.

---

## 4. Shared widgets in code

M28 is building most of this for FX. **The ask is to build it in a form the other pages can take
as it is,** so the Sound and Mod PRs add data, not drawing code.

* **`vt-model`:**
  * **`Grid`,** a small type for `[[Option<usize>; 3]; 4]`, with its checks: each setting appears
    exactly once, and the holes. `Insert::grid()` (#374) returns one, and so does
    `Machine::grid()` (new).
  * **`type_param()`** on both, for the title's type.
* **`vt-tui::track_page`:** the row tables (`FX_ROWS`, the four Sound tables, `MOD_ROWS`) become
  **`Page::devices(track) -> Vec<DeviceSpec>`**.
  * **`DeviceSpec`** holds an identity, an optional toggle (`Control` for `b`), the title controls,
    a grid of `Control`s, a panel kind, and a width (full or half).
  * **The cursor** becomes `(device, row, column)`, with row 0 the title. `j`, `k`, `{`, `}`, `0`,
    `$` and the moves across devices are written once over a list of `DeviceSpec`s laid out as a
    rack.
  * **#374 needs this for FX anyway.** Doing it generally costs little more than doing it for slots.
* **`views/widgets/` (new files, all page-agnostic):**
  * **`device.rs`:** `Device`, the frame, the title from its spans, focus and off styles, and the
    split into a grid area and a panel area.
  * **`grid.rs`:** `SettingsGrid`, which draws a `DeviceSpec`'s cells: bar, word, choice,
    two-state and compact cells; the cursor; fine parts; live bars; jump labels; fading. It takes a
    small trait, `CellSource`, for a control's text, bar and live value, so it doesn't reach into
    `App`.
  * **`panel.rs`:** `GraphicPanel`, the faint box at a size class, with image placeholders or text
    lines inside, and an optional caption.
* **`kitty.rs`, splitting off `kitty/plot.rs`:**
  * **Drawing:** `Picture` with an alpha for each palette entry; an anti-aliased polyline at cell
    pixels; the gridline kinds; and always-compressed output.
  * **`PanelImages`:** one cache keyed by `(PanelKey, Shade)`, replacing today's separate `images`,
    `filter`, `waveform` and `slots` fields. `PanelKey` is `Filter`, `Thumb`, `Env(n)`, `Lfo(n)` or
    `Slot(n)`.
  * **`Dots`:** the sample page's direct-placement code (`place_at`, take-off under overlays, store
    again after a clear) made general, with a `Y` offset. The LFO dots use it now, and the comp's
    ball later.
* **`vt-engine`:** `LiveValues` gains `lfos` (phase and value). Later, it gains `envelopes` (stage
  and level).

**How this lines up with #320 and M26:**

* **#321 (closed) put the shared view building blocks in `views/widgets/`.** `device`, `grid` and
  `panel` are the next three, in the same place, with the same rule that a page uses them rather
  than working out its layout again.
* **#320 makes each page a component** (`TrackPage { cursor, … }`, with `key` and `draw` over
  `Ctx`). The device cursor `(device, row, column)` is exactly the state `TrackPage` should own.
  **Order:**
  1. #374–#376 land first, as feature PRs on the track page.
  2. Then #320's **TrackPage** PR (keys and drawing unchanged, as #320 requires) moves the
     now-general device cursor and `Page::devices` off `App`.
  3. Then the Sound and Mod PRs are written against the component.

  That way #320 moves one cursor model, the device one, not today's three tables plus a new one.
  `CellSource` is the seam that keeps the widgets free of `App`, which is what `Ctx` wants.
* **#323 (split `tests/app.rs` by feature):** do its track page part before M29's step 2. Every step
  here adds track-page tests to a file that's already very large.

---

## 5. Plan: proposed issues

### Folded into M28 (scope added to existing issues)

| Into | Adds | Size |
|---|---|---|
| #374 | **General, not FX-only:** the `DeviceSpec` rack, the `(device, row, column)` cursor, and `views/widgets/{device,grid,panel}.rs` with `CellSource`. FX is their first user. Half width is supported but unused. | +100–150 lines |
| #375 | **The two-state rule everywhere at once:** Mod's `Reset`, `Retrig` and the Arpeggio's `Retrig`, and the sample page's `Reverse` and `Snap`, so one rule never has two looks | +40–60 |
| #376 | **One drawing path:** `kitty::plot` replaces `line_png` for every curve, so the filter curve and envelopes go smooth at their current sizes. Also `PanelImages` with three shades. | +120–180 |

### New milestone: M29, "Track pages as devices"

| # | Proposed issue | Depends on | Size |
|---|---|---|---|
| 1 | **TrackPage component.** #320's first PR, moved into this sequence: the cursor, `Page::devices` and track page keys off `App`, with nothing visible changing. The track part of #323 goes first. | #374–#376 | M, about 400 moved |
| 2 | **Sound page devices.** Machine, Filter, Output and Play; `Machine::grid()` and `type_param()`; type, filter type and mode in their titles; navigation; screenshots; decision; manual and guide. | 1 (or #374 alone) | L, 600–800 |
| 3 | **Sound page graphics.** The L filter curve with its gridlines and throttle; the sampler's thumbnail with Start and Length lines; the Output's text meter; and a measure of the moving curve's cost in kitty. | 2, #376 | M, about 350 |
| 4 | **Mod page devices.** The half-width rack, Accumulator and Arpeggio, moves across devices, the destination in the title, and the shape glyph out of the grid. | 1, `track-capacity` | L, 500–700 |
| 5 | **Mod page graphics.** H envelope and LFO panels; `LiveValues.lfos`; `Dots` with `Y=`; the phase dot. | 4, #376 | M, about 400 |
| 6 | **Modulator on/off,** if question 1 says yes. A `bypassed` flag for each LFO and envelope in the model, saved only when set; the engine skipping it; `b`; and the faint state. | 4 | S, 150–250 |
| 7 | **Fixed tracks' Sound pages.** Output with holes; the group's Tracks; the return's Sends in; the master's `Limiter` cell (question 2). | 2 (3 for the meter) | S–M, about 250 |
| 8 | **Mixer section rules** on row 1, with the focused section brighter | none | S, about 60 |

**Later, as issues with no milestone:**

* the envelope's riding dot (`LiveValues.envelopes`);
* a per-track scope on a synth's Machine device;
* #377's zoom for the filter and envelope panels too;
* a filter bypass toggle, if it's missed;
* jumping to a track from the Tracks and Sends in devices.

**Why a new milestone rather than more M28:** M28 is "FX page devices", and its issues are already
written and approved. Folding in the three small scope additions above makes M28 produce the
shared parts once. The page conversions are each as big as #374 and have their own decisions, so
they're a milestone of their own.

**Order:** 1 → 2 → 3, and 1 → 4 → 5 → 6. Step 7 follows 2, and 8 can go any time. Steps 2 and 4 can
run in parallel once 1 has landed, as they touch different `Page::devices` arms. But not at the
same time as each other's graphics PR, because those all edit `kitty.rs`.

---

## 6. Open questions for the user

1. **Should LFOs and envelopes get their own on/off toggle?** It would be a new `bypassed` flag,
   flipped with `b` and shown in the title as an FX slot's is, so you can hear a track without its
   wobble and back without losing the settings. The alternative is no toggle, with a modulator
   that goes nowhere or has `00` depth simply drawn dim.
2. **Should the master's limiter move to the master's Sound page?** It would be a `Limiter` toggle
   in its Output device, after `Level`, where the chain reads Trim → slots → Level → limiter. The
   choices are to show it in both places, the project page and the master, or to move it off the
   project page.
3. **Which level should the Output device's meter show?** Today's report is after `Level` and
   `Pan`, as the mixer shows. For setting `Trim`, a tap straight after `Trim` would be more
   honest, at the cost of a second level report a track. Or is the mixer meter enough, and the
   Output device needs no meter at all?
4. **How should the Mod page grow when M20 lands?** That's Hold (#85), Curve (#46), LPG mode
   (#45), and LFO Fade and one-shot (#86). Two ways to make room:
   * Put each modulator's *mode* in its title, as a type, and put Fade in the LFO's spare cell.
     Hold and Curve would then need a third grid column, which shrinks the panel from H to S.
   * Keep the panels at H and let the Mod page scroll past four device rows.

**Decided** (the author, 2026-11-10):

1. **Yes:** LFOs and envelopes get an on/off toggle in their titles, flipped with `b`, as an FX
   slot's bypass is.
2. **Move it:** the master's limiter goes onto the master's own page, off the project page.
3. **After `Trim`:** the Output device's meter reads the level straight after `Trim`.
4. **Modes in titles, and scrolling:** the Mod page grows by putting each modulator's mode in its
   title, as a type, and scrolls past four device rows rather than shrinking its panels.
