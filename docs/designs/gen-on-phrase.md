# Gen on the phrase view, beside the grid

**Status:** decided by the author on 2026-11-20 (below, **Decided**); the plan in §10 is filed in
the milestone. No code is changed here.

The author asked, on 2026-11-20: "maybe Gen should be a page on the Phrase page, as it really is
about that place, so S and the Gen areas are grouped in a more logical place. Keep the Macros on
the track area on its own. Also can we have a way of sliding the Gen page in on the right side so
we can see the tracker columns? (we can hide the main meter etc.) Nice to have; a full page like
the track pages is fine as well, and should be an option regardless."

**Decided already (the author, 2026-11-20):** while the Gen side panel is open, the screen shows
**only the phrase under the cursor's own columns** (Note, Velocity, Nudge, Length and its FX
columns) on the left and the Gen panel on the right. No meter, and none of the other tracks'
columns, the timeline or the row inspector. Every width below is measured on that basis.

**Decided (the author, 2026-11-20), the answers to §9:**

1. **The side form is the default.** The full form is `Ctrl-w o` away, remembered for the session.
2. **`Space 8` opens Gen,** on the phrase view. Macros is reached with `L` from FX, the picker or
   `Space T`.
3. **`Phrase` and the lane devices move with Gen.**
4. **Vim's `Ctrl-w` family:** `h` `l` `w` `o` `v` `c`, with `Esc` from the panel back to the grid.
5. **`S` keeps its own box,** drawn over the panel at its top left, for now.
6. **No setting for the default form** yet; the session remembers the last.
7. **Previews in the side form are the phrase view's own bars,** not the devices' pictures. While
   the keys are in a device, the column it writes shows its values as the bars `S` already draws
   (editing guide, **Shaping a column**): each row's value solid, with what it was when the keys
   entered the device fainter behind it; the column's name, range and `lowest─highest avg n
   changed` line above; and on the Note column the contour over the scale's roots. They're drawn
   over the phrase's other columns, which can't be edited while the keys are in the panel, as `S`
   draws them over the lanes today, and go when the keys go back to the grid. A column whose
   values aren't amounts has none, as now.
8. **Following stops while the keys are in the panel;** the panel's cursor leads and lights its
   device's rows, and following resumes when the keys go back to the grid.

**It builds on:**

* [`generators-page.md`](generators-page.md), which put Gen on the track page (§3) with the macros'
  band first (§4);
* [`notation-generators.md`](notation-generators.md) and decisions 456 to 461, the token fields and
  the lit rows (460);
* #1106, the pop-out token buffer being built now;
* the code: `vt-tui`'s `pages/phrase/draw.rs` (the grid and the overview), `pages/phrase/inspector.rs`,
  `pages/track/draw/generators.rs` (the Gen page), `overlays/shape/draw.rs` (the `S` panel),
  `overlays/macro_panel.rs`, `screen/meter.rs` and `input/bindings.tsv`;
* the guides `track-gen.md`, `navigation.md` and `meter.md`.

**The recommendation in one paragraph.**

* **Gen moves to the phrase view,** whose generators it edits, as a page of its own beside the grid.
  `Space 8` keeps opening it. The track page's fourth page becomes **Macros**, the eight macros and
  what each one holds, with nothing else on it.
* **The side form is the default.** At 163x42 with the phrase's three FX columns, the grid takes 67
  columns and the panel gets the other 95. A device 95 wide has **91 columns of settings, exactly
  what the Gen page has today beside its preview**, so every generator's settings lie where they do
  now. The preview is the one thing dropped, and the real column is beside it instead. Only the
  devices that today run four full columns (`Phrase`, and the lanes) need the narrower cells the
  four-column generators already use.
* **The panel follows the grid's cursor.** On Velocity, the panel shows Velocity's generator, or an
  `Empty` for Velocity if it has none. That is the big win: the grid and its generator stay side by
  side, and moving along a row runs through each column's generator, as the M8 opens the
  instrument of the row you're on.
* **Focus moves with Vim's window keys,** `Ctrl-w l` and `Ctrl-w h`, and `Esc` from the panel goes
  back to the grid, as it does from the macro panel. `Ctrl-w o` makes the panel the whole page (the
  full form) and `Ctrl-w v` puts it back beside the grid.
* **`S` stays** the way to make a generator from a selection, modal and cancellable, drawn over the
  side panel's place rather than over the grid. With the panel open, `gl` just moves the panel to
  that column's device.
* **The token buffer of #1106 opens over the panel, never the grid,** so the rows the cursor's
  token writes (decision 460) light up in the column beside it as you edit the text. Today that
  only happens in the `S` panel, as the Gen page has no grid.

---

## 1. Prior art

| | Where the generator or editor sits | What it shows of the notes meanwhile | What we take |
|---|---|---|---|
| **Ableton Live 12's clip view** (Reference, "Clip View" and "MIDI Transformations and Generators") | the `Transform` and `Generate` tabs sit in the clip view's left panel, beside `Notes` and `Envelopes` | the MIDI note editor stays beside the panel, so a generator's notes appear as you turn it | generators belong to the clip, the phrase, and live beside its notes. The strongest precedent for the author's idea |
| **Renoise's Advanced Edit panel** (Manual, "Advanced Edit") | a panel docked beside the pattern editor, toggled from its header: transpose, interpolate, randomise, humanise on the selection or the column | the pattern stays in view, the selection lit | a side panel that works on the column the cursor is in, toggled, with the grid untouched |
| **Renoise's lower frame** (Track DSPs, automation) | under the pattern editor, a frame for the selected track's devices | the pattern, shorter | Renoise splits by height. At 163x42, with rows we want to keep, width is the room we have |
| **The M8** (Manual, "Screen map") | one screen at a time on a map: Song, Chain, Phrase, Instrument and Table in a row. `Shift-Right` from a phrase opens **the instrument of the row under the cursor**, and from an instrument its table | none: 320x240 shows one screen | the cursor chooses what the next screen shows. The side panel does that without leaving the grid |
| **Elektron's Digitakt and Syntakt** | parameter pages (TRIG, SRC, FLTR, AMP, FX, MOD) on the screen | the 16 trig keys light the steps beside them all the time | the steps stay in front of you while you turn what plays them: the side form, done in hardware |
| **The Polyend Tracker and tracker-ce** (`core/interface/patternEditor/gPatternEdit.cpp`, `showFillPopup`) | the Fill and Randomise popups open over the bottom of the pattern, a row of lists matched to the buttons under the screen (each `800 / 8` wide) | the pattern's rows above the popup | the modal fill on a selection, our `S`. It applies once and closes; it isn't a place you stay |
| **Bitwig's Inspector** | a side panel showing the selected notes' or clip's properties | the editor beside it | a panel whose content follows the selection |
| **Vim's windows** (`:help windows`) | `Ctrl-w v` splits, `Ctrl-w h` and `Ctrl-w l` move, `Ctrl-w o` keeps only this one, `Ctrl-w c` closes | both windows | the keys, which the author knows by hand |

**What we take:** from Live 12, the place, generators as part of the clip view; from Renoise and
Bitwig, a toggled side panel that follows the cursor; from the M8, the rule that the cursor's
column chooses the device; from Elektron, the steps kept in view; from the Polyend and tracker-ce,
`S` stays a modal fill on a selection; and from Vim, the keys.

---

## 2. Where things are now

* **Generators are the phrase's.** `Phrase::generators` (`vt-model/src/phrase.rs`) holds each live
  column's `Generator`: its column (`ShapeColumn`: Note, Velocity, Nudge, Length or an FX slot with
  its command), its span (rows, or the slot's lane) and its `Shape`. A column can have more than
  one, on rows that don't overlap (`add_generator` drops any it overlaps).
* **The Gen page is the track page's fourth** (`Page::Gen`), after Sound, Mod and FX, opened with
  `Space 8` from any view, `H` and `L` between the pages. It shows the phrase under the cursor, the
  one `gl` reopens. Its devices, top to bottom: the track's eight **Macros**, each live column in
  the phrase view's order, an `Acid`'s `Loops` and a `Random`'s `Weights` under theirs, a lane
  device per FX column with a lane, **Phrase** (the track's order settings) and **Empty**.
* **The phrase view has no pages.** It is the grid, the overview of the other tracks' notes to its
  right, the row inspector (`Space i`) right of that while there's room, and the main meter at the
  screen's right edge. `H` `M` `L` are Vim's screen top, middle and bottom there, `zi` turns the
  timeline on and off, and `Tab` plays.
* **The `S` panel** (`overlays/shape`) opens over the top right of the phrase view, 42 wide, on a
  selection: a list of the fill's settings, `Enter` keeps it as plain values, `L` keeps it live,
  `Esc` cancels. While it's open, the grid draws the column's values as bars and lights the rows the
  cursor's token writes (decision 460).
* **The macro panel** (`Space t`, decision 385) is a strip of the eight macros at the bottom of any
  page. `Space T` puts the keys in it and `Esc` gives them back. It's the precedent for a panel that
  takes the keys and gives them back.
* **`Ctrl-w` is free** in every mode (`bindings.tsv`), as are `gL` in the phrase view and `Space G`.

---

## 3. Measured: the room at 163x42

Each number was taken from the real draw code: the screen drawn with ratatui's test backend through
`vt_tui::run::draw_screen` at 163x42 (the `screen_cells` helper), and the Gen page drawn through
`draw_view` at 150 and at 95 wide. It was a throwaway test that wasn't kept.

| | Columns | Rows | From |
|---|---|---|---|
| The screen | 163 | 42 | the author's fullscreen kitty |
| The title line, and the status and message lines | 163 | 1, and 2 | row 0, rows 40 and 41 |
| A page's area | 150 | 39 (rows 1 to 39) | the meter takes the rest |
| The main meter | 13 (cols 150 to 162) | 39 | `meter::WIDTH` |
| **The phrase's own grid, 3 FX columns** | **67 (cols 0 to 66)** | 36 phrase rows and its frame | gutter 9, frame 2, body 57 |
| The overview of the other tracks | 5 a track, from col 69 | | `NEIGHBOUR_WIDTH` |
| The row inspector | 39 to 60 (seen at cols 74 to 133) | 13 | `inspector::NARROWEST`, `WIDEST` |
| The `S` panel | 42 (cols 108 to 149) | from row 2 | `shape/draw.rs` `WIDTH` |
| A Gen page device | 150 | 9 (`DEVICE_HEIGHT`) | 4 devices fit in 39 rows |
| Its settings, beside a preview | 91 (cols 3 to 93) | | the preview's box is 51, at cols 96 to 146 |
| Its settings, with no preview | 146 | | `Phrase`, lanes, `Macros` |
| A Sound cell (name 10, value 5, bar 11, gap 3) | 29 | | `CellWidths::SOUND`, `GEN_CELLS` |
| A four-column generator's cell (bar 5, gap 2) | 22 | | `GEN_FOUR_CELLS` |

**The grid's width.** A row is the note part, 18 columns (`C-3 64 ···· ·· ···`), then 13 for each
FX slot (`COMMAND_WIDTH` 6, the coarse and fine digits 4, the mark 2, a gap), inside a gutter of 9
and the frame's two edges: **28 + 13 × FX columns**. Every step has `FX_SLOTS = 3` slots today, a
constant, not a track setting, so 67 is what the author will see. The other counts are given in case
that ever changes:

| FX columns | Grid | Left for the panel (163 − grid − 1 gap) | Settings (panel − 4) | Sound cells that fit (29n − 3) | Four-column cells (22n − 2) |
|---|---|---|---|---|---|
| 1 | 41 | 121 | 117 | 4 | 4 |
| 2 | 54 | 108 | 104 | 3 | 4 |
| **3 (today)** | **67** | **95** | **91** | **3** | **4** |
| 4 | 80 | 82 | 78 | 2 | 3 |

**What that means for the devices.** The Gen page's generators already lay their settings out in
91 columns, as the preview takes the rest. A 95-wide panel with no preview gives them **the same 91
columns**, so they lie exactly as they do now. That was checked by drawing the Gen page 95 wide:

* **The generators:** unchanged. Three Sound columns end at col 87 (`From`, `Curve`, `Over`/`Hits`/
  `Rotate`), and a four-column generator (a `Follow` that mutates, an `Acid`, an `Own` rhythm's
  `Over`) uses its 22-column cells, 86 of the 91.
* **`Phrase` and the lane devices** run four Sound columns at cols 3, 32, 61 and 90, as they have the
  whole width at 150. At 95 the fourth is cut (`Order Y` shows as `Orde`). They take the 22-column
  cells too, by the rule *a device whose columns don't fit in Sound cells uses the four-column ones*,
  which the generators follow already.
* **`Weights`** (a cell a degree, rows of four) and **`Loops`** (two columns) fit as they are.
* **The macros' band**, 4 × 32, would be cut, but it leaves Gen (§4).
* **Height:** 39 rows hold four devices of 9, with 3 to spare, the same as the Gen page holds once
  the macros' band is gone. The grid beside them shows 36 of the phrase's rows.

So nothing has to re-flow into two columns, and no device has to be paged one at a time. The panel
is the Gen page's devices, minus the previews, 95 wide.

---

## 4. Where Gen and the macros go

### 4.1 Gen: a page of the phrase view

The phrase view gets two pages, **Grid** (as it is) and **Gen**, named in its title as the track
page's are, `Track 01  Grid  Gen  Phrase 00`. Gen keeps everything it has: its devices, the phrase
under the cursor and the `▶ 02` when another one plays, its keys (the track page's: `=` `-` `+` `_`,
`i`, `K`, `x`, `r`, `dd`, `M`, `Space g a`, `Enter`), the Roto's layouts and the status line.

* **`Space 8`** opens Gen, in the form last used (§6). The view picker lists it as `Phrase: Gen`.
* **`Space 3`** opens the Grid, with the side panel still open if it was.
* **`H` and `L` stay Vim's** screen top and bottom in the grid. In the full form, whose keys are the
  track page's, `H` goes back to the Grid, as `H` goes back a page there.

Moving Gen loses nothing on the track page: every device on Gen is about the phrase or the walk of
its rows. `Phrase` and the lane devices hold the track's settings, not the phrase's, but they're
settings of how the phrase's rows are walked, so they're at home beside the grid. That's
question 3.

### 4.2 Macros: the track page's fourth page

The track page's pages become **Sound, Mod, FX, Macros**. The Macros page has room the band never
had:

* the eight macros in two rows of four, as the band has them, at the Sound page's 29-column cells,
  or the band's 32;
* under them, **every mapping of the cursor's macro**: each setting, its min and its max, as `gm`'s
  mapping view lists them, so the page answers "what does Macro 3 move?" without opening anything;
* and the macros' Roto page, unchanged.

`Space t` and `Space T` (the macro strip on any page), `M` and `gm` stay as they are. The page
needs a key: `Space 8` is Gen's, so the Macros page is reached with `L` from FX, the picker or
`Space T`'s strip. Question 2 asks whether `Space 8` should go to Macros instead.

---

## 5. The side form

### 5.1 What's on screen

The grid at cols 0 to 66, a gap at col 67, the panel at cols 68 to 162 (§3). The title line keeps
the phrase view's title on the left, puts the panel's on the right (`Gen  Phrase 00 · on Vel, row
00`) and keeps the CPU figure where it is. The status and message lines are as they are. Hidden
while the panel shows, as decided: the meter, the overview, the row inspector and the timeline.
`Space i` and `zi` are remembered and come back when the panel closes. The macro strip
(`Space t`) and `Space d`'s line take their rows from both, as they do from any page, since the
author asked for them.

The devices' previews aren't drawn in the side form. The column beside them is the preview, at
full height, with the grid's own colours. They come back in the full form.

### 5.2 Mockup 1: the side form, the cursor on Velocity

At 163x42, built from the real renders: the grid as `draw_screen` draws it, the devices as the Gen
page draws them 95 wide, `Phrase` set in the four-column cells. The device the panel follows is
drawn here with a heavy frame. On screen it's the focused device's frame colour, as now. Nerd Font
icons are shown as `T`, `~` and `■`.

```text
 T Track 01  Chain 00  00  ~ Vel                                    Gen  Phrase 00 · on Vel, row 00                                CPU  0%  late 0  xruns 0
        ╭Note ───────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮ ╭─ Ramp  Euclid · Note 00–1F ─────────────────────────────────────────────────────────────────╮
    00  │C-3 00     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
 1  01  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  From      C-3  ▕███▋     ▏   Curve     0    ▕    │    ▏   Over      All rows               │
 2  02  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
 3  03  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  To        C-4  ▕████▋    ▏   S         0    ▕    │    ▏   Hits      3    ▕█▋       ▏       │
 4  04  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
 5  05  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  Run       1    ▕         ▏                                Rotate    0    ▕         ▏       │
 6  06  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
 7  07  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ╰─────────────────────────────────────────────────────────────────────────────────────────────╯
 8  08  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┏━ Ramp  Notes · Vel 00–1F ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
 9  09  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃                                                                                             ┃
10  0A  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃  From      00   ▕         ▏   Curve     0    ▕    │    ▏                                    ┃
11  0B  │F#3 40     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃                                                                                             ┃
12  0C  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃  To        7F   ▕█████████▏   S         0    ▕    │    ▏                                    ┃
13  0D  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃                                                                                             ┃
14  0E  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃  Run       1    ▕         ▏                                                                 ┃
15  0F  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃                                                                                             ┃
16  10  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
17  11  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ╭─ Phrase · here Forward, as written ─────────────────────────────────────────────────────────╮
18  12  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
19  13  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  Order     +0   ▕ │ ▏  Reverse               Order X   Own         Order Y   Own            │
20  14  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
21  15  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  Length    +0   ▕ │ ▏  Start     0           Cue                                            │
22  16  │C-4 7F     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
23  17  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  Chain     Forward     Jump      0000 ▕   ▏                                                 │
24  18  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
25  19  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ╰─────────────────────────────────────────────────────────────────────────────────────────────╯
26  1A  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ╭─ Empty ─────────────────────────────────────────────────────────────────────────────────────╮
27  1B  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
28  1C  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
29  1D  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
30  1E  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │  K  make a column of this phrase live                                                       │
31  1F  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ │                                                                                             │
        ╰─────────────────────────────────────────────────────────╯ │                                                                                             │
                                                                    │                                                                                             │
                                                                    ╰─────────────────────────────────────────────────────────────────────────────────────────────╯



 NORMAL  Velocity  ■  120 BPM  Groove Straight  Oct 3 Vel 64  Grid 1/4  TVol E0 *
Vel is live, rows 00–1F: Ctrl-w l turns it, Space g a applies it
```

### 5.3 The grid's cursor chooses the device

This is the side form's reason to exist. While the keys are in the grid, the panel follows the
cursor's field and row:

| Cursor on | The panel shows |
|---|---|
| Note, Velocity, Nudge, Length | that column's generator whose rows hold the cursor's row, or, if none does, the column's first generator. On a column with none, an `Empty` for that column |
| The gate (`G1`, `M2`) | Length's, as the gate cell's last digits are its length |
| An FX slot's command or value | that slot's generator, then its lane device under it. With neither, its `Empty` |

* **Following isn't moving.** The followed device gets the focused frame, and the panel scrolls by
  whole devices to keep it in view, as the Gen page scrolls to its cursor. The order stays the
  phrase view's, so devices never jump about.
* **`Empty` names the column**: `Empty · Vel`, `K` lists fills to make Velocity live over the whole
  phrase, as the Gen page's `Empty` does, and `S` on a selection still makes one on just those rows.
* **The status line says where it is,** `Vel is live, rows 00–1F`, so `l` along a row reads each
  column's generator out loud.
* **`Space d`** describes the setting under the panel's cursor while the keys are in the panel, and
  the grid's field while they're in the grid.

### 5.4 Keys: between the grid and the panel

| Keys | Does |
|---|---|
| `Space 8` | Open Gen in the form last used, the keys in it, on the device the grid's cursor chooses |
| `gL` | On a live column, open the side panel on its device, the keys in it: the wider `gl` |
| `Ctrl-w l`, `Ctrl-w h` | Keys to the panel, or back to the grid |
| `Ctrl-w w` | Keys to the other one |
| `Esc` | In the panel, keys back to the grid, the panel staying open, as from the macro panel |
| `Ctrl-w o` | The full form: Gen as the whole page (§6) |
| `Ctrl-w v` | The side form: Gen beside the grid |
| `Ctrl-w c` | Close the panel, the grid as it was, meter and overview back |

* **In the panel, the keys are the Gen page's,** unchanged: `j` `k` through the settings and on to
  the next device, `h` `l` along a row, `}` `{` by device, and the rest of its table. `Tab` still
  plays. One keymap, two places.
* **The grid's cursor stays put** while you work in the panel. The panel's device lights its rows in
  the grid (the live column's span, as `live` marks it now), and if they're all off screen the grid
  scrolls to put the first one in the middle, as `zz` would.
* **The Roto** follows the panel's device in either case, as it follows the Gen page's cursor now.
* **Ctrl-w** is free in every mode, and kitty passes it on (its own keys are `Ctrl-Shift`). Vim
  users already use these keys between windows, and the side form is a vertical split.

---

## 6. The full form, and choosing between them

The full form is today's Gen page moved to the phrase view: 150 wide with the meter, the previews
drawn, four devices in view, and the macros' band gone to the track page. It's there for a phrase
with many generators, for the previews, and for anyone who'd rather have it out of the grid's way.

* **The form is remembered for the session,** as the macro panel's showing is, not in the song.
  `Space 8` opens Gen in whichever form was used last, side to begin with.
* **`Ctrl-w o`** from the side form makes it full, and **`Ctrl-w v`** from the full form puts it back
  beside the grid. `Space 3`, or `H`, goes from the full form to the Grid.
* **A setting for the default form** (`Gen opens beside the grid`, on the project page's Audio
  sub-page, with this machine's other settings) is question 6.

### 6.1 Mockup 2: the full form, the cursor on Velocity's `From`

Built from the real 150-wide render with the macros' band taken out. The `▒` boxes are the previews,
which kitty draws as images. The meter is on the right as on every full page.

```text
 T Track 01  Grid  Gen   Phrase 00                                                                                                 CPU  0%  late 0  xruns 0
╭─ Ramp  Euclid · Note 00–1F ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│                                                                                               ╭─────────────────────────────────────────────────╮  │
│  From      C-3  ▕███▋     ▏   Curve     0    ▕    │    ▏   Over      All rows                 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  │
│                                                                                               │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  │     Main
│  To        C-4  ▕████▋    ▏   S         0    ▕    │    ▏   Hits      3    ▕█▋       ▏         │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  │        --  0
│                                                                                               │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  │   6
│  Run       1    ▕         ▏                                Rotate    0    ▕         ▏         │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  │
│                                                                                               ╰─────────────────────────────────────────────────╯  │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
┏━ Ramp  Notes · Vel 00–1F ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓   0
┃                                                                                               ╭─────────────────────────────────────────────────╮  ┃
┃  From      00   ▕         ▏   Curve     0    ▕    │    ▏                                      │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  ┃
┃                                                                                               │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  ┃
┃  To        7F   ▕█████████▏   S         0    ▕    │    ▏                                      │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  ┃  -6
┃                                                                                               │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  ┃
┃  Run       1    ▕         ▏                                                                   │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│  ┃
┃                                                                                               ╰─────────────────────────────────────────────────╯  ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛ -12
╭─ Phrase · here Forward, as written ────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│                                                                                                                                                    │
│  Order     +0   ▕    │    ▏   Reverse                     Order X   Own                Order Y   Own                                              │
│                                                                                                                                                    │ -18
│  Length    +0   ▕    │    ▏   Start     0                  Cue                                                                                    │
│                                                                                                                                                    │
│  Chain     Forward            Jump      0000 ▕         ▏                                                                                           │
│                                                                                                                                                    │ -24
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ Empty ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│  K  make a column of this phrase live                                                                                                              │ -36
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -48

                                                                                                                                                       -60
                                                                                                                                                           L  R  G
 NORMAL  From 00: where the ramp starts  ■  120 BPM  Groove Straight  Oct 3 Vel 64  TVol E0 *

```

---

## 7. `S`, `gl` and the side panel

**`S` stays,** for three reasons. It works on a selection, the rows you chose, while the panel
works on what's already live. It's modal and cancellable: `Esc` leaves the phrase as it was, which
a live device can't. And it works without the side panel, from the grid alone, as now.

What changes with the panel open:

* **`S` draws in the panel's place,** its 42-wide box at the top left of the panel, not over the
  grid, so the grid's columns stay fully in view, its value bars and lit rows included. On `L` (keep live) the new generator's device appears in the panel with the keys in
  it, followed, since it's the cursor's column. On `Enter` (keep as plain values) or `Esc`, the keys
  go back to the grid.
* **`gl` with the panel open** moves the panel to that column's device instead of opening the `S`
  panel, as there's nothing the `S` panel would show that the device doesn't. Without the panel, `gl`
  is as now.
* **Later, perhaps:** the `S` panel drawn as a device, `New · Vel 03–0B`, in the side panel, with
  the same cells as a live one and `Enter keep  L keep live  Esc cancel` in its frame. That makes
  one way of showing a generator's settings instead of two. It's bigger work and isn't needed to
  ship the side form (question 5).

---

## 8. The token buffer (#1106) and lit rows (decision 460)

#1106 replaces editing tokens in a cell with a buffer: on the cell `h` `l` `j` `k` only move,
`Enter` opens a dialog with the whole text, and `i` `a` open it typing at the end. Decision 460 lights
the rows the cursor's token writes in the field's `Rows` line and in the phrase view's column.

* **Today the grid's lit rows only show from the `S` panel,** as the Gen page has no grid
  (`Grid::token_rows` reads `ctx.shape_panel()`). In the side form, the panel's open field is the
  second source: the grid lights the rows of the buffer's token as you move along the text. That's
  the side form's second win. You see `x` on row 03 light row 03 beside it.
* **The dialog opens over the panel,** under its cell, as wide as it needs up to the panel's 91, and
  never over the grid. An `Own` rhythm's text (48 columns of room in the `S` panel) fits on one line.
* **Nothing in #1106 has to change for this.** It needs its dialog placed inside an area it's given
  (the Gen page's, the `S` panel's, or the side panel's), and `token_rows` to read the open buffer
  as well as the `S` panel. That's a small follow-up once both have landed (§10, item 6).

### 8.1 Mockup 3, a sketch: the buffer open on an `Own` rhythm's `Text`

The top of the screen only. The cursor is on the second `x` (the `‾` under it), which writes row 03.
The `▸` in the gap marks the row lit in the grid for this sketch only. On screen the row's cells are
in the cursor's colour, bold, as decision 460 draws them.

```text
 T Track 01  Chain 00  00  ~ Note                                   Gen  Phrase 00 · on Note, row 03                               CPU  0%  late 0  xruns 0
        ╭Note ───────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮ ┏━ Ramp  Own · Note 00–1F ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
    00  │C-3 64     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃                                                                                             ┃
 1  01  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃  From      C-3  ▕███▋     ▏   Curve     0    ▕    │    ▏   Rhythm    Own                    ┃
 2  02  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃ ╭ Text · Own rhythm ────────────────────────────────────────────────────────────────────╮   ┃
 3  03  │C#3 64     ···· ·· ···   ··     ···   ··     ···   ··    │▸┃ │ x . . x . . x . x . . x . . x .                                                       │   ┃
 4  04  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃ │       ‾                                                                               │   ┃
 5  05  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃ │ Rows  x · · x · · x · x · · x · · x · x · · x · · x · x · · x · · x ·                 │   ┃
 6  06  │D-3 64     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃ │ h l w b by token   = - turn it   i a type   Esc Enter close                           │   ┃
 7  07  │--- ··     ···· ·· ···   ··     ···   ··     ···   ··    │ ┃ ╰───────────────────────────────────────────────────────────────────────────────────────╯   ┃
 8  08  │D#3 64     ···· ·· ···   ··     ···   ··     ···   ··    │ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

---

## 9. Open questions for the author

1. **Is the side form the default?** Recommended yes: it's what the author asked for, and at 163x42
   it costs the generators nothing but their previews. The full form stays one key away
   (`Ctrl-w o`), remembered for the session.
2. **Which page does `Space 8` open?** Recommended: Gen, on the phrase view, so the key the author's
   hands know keeps its meaning. The Macros page is reached with `L` from FX, the picker or the
   strip (`Space T`). The other way: `Space 8` for the track page's fourth page, Macros, as
   `Space 5` to `Space 8` are its four pages now, and `gL` or `Space 3` then `Ctrl-w l` for Gen.
3. **Do `Phrase` and the lane devices move with Gen?** Recommended yes. They hold the track's
   settings, but they're about how the phrase's rows are walked, and they're most useful beside the
   grid whose rows they move. The other way: they move to the Macros page, or a page of their own on
   the track page.
4. **Focus keys: Vim's `Ctrl-w` family, or `Space 8` and `Esc` alone?** Recommended `Ctrl-w h` `l`
   `w` `o` `v` `c`, with `Esc` from the panel as well, as the macro panel's. The smaller set is
   `Space 8` to open and focus, `Esc` back to the grid, and `Space 8` from the grid to focus again,
   with `Ctrl-w o` for full the only `Ctrl-w` key.
5. **`S` in the side form: its own box over the panel, or a device in it?** Recommended: its box,
   at the panel's top left, for now. Drawing it as a `New` device is one way of showing a
   generator's settings instead of two, but it's bigger work for the same keys, so it comes later.
6. **A setting for the default form?** Recommended no, not yet: the session remembers the last form,
   and one more setting is worth having only if the author finds themselves pressing `Ctrl-w o`
   every session.
7. **Previews in the side form?** Recommended none, as the column is beside them. If one is missed,
   a one-device zoom could show the cursor's device with its preview 95 wide (91 of settings don't
   fit beside a 51-wide preview, so the settings would page). That's a follow-up only if asked for.
8. **Should following stop while the keys are in the panel?** Recommended yes: in the panel the
   panel's cursor leads and lights its device's rows in the grid. Following the grid's cursor
   resumes as soon as the keys go back.

---

## 10. Plan: proposed issues

Sizes: **S** is a day or less, **M** a few days, **L** a week or more. Each comes with its guide
changes (`track-gen.md` becomes the phrase view's Gen guide, plus `navigation.md` and
`editing.md`), the manual, and a decision.

1. **The track page's Macros page (M).** `Page::Gen` becomes `Page::Macros`: the band, and the
   cursor's macro's mappings under it. The Gen devices are still drawn there until item 2 lands.
2. **Gen as a page of the phrase view, the full form (L).** The devices, their keys (the track
   keymap's Gen rows, moved to a `PhraseGen` mode or shared), `Space 8`, the title's `Grid  Gen`,
   the picker, the Roto's layouts and `gen_page.rs`'s tests moved. Depends on 1.
3. **The side form (L).** The phrase view drawn as the grid alone plus a 95-wide panel, with the
   meter, overview, inspector and timeline hidden. Previews skipped. A device whose columns don't
   fit in Sound cells uses the four-column ones. `Ctrl-w` `h` `l` `w` `o` `v` `c`, `Esc`, the form
   remembered for the session. A 163x42 `screen_cells` test pins the columns: the grid at 0 to 66,
   the panel at 68 to 162. Depends on 2.
4. **The panel follows the grid's cursor (M).** The table in §5.3, `Empty` for the cursor's column,
   the status line, and the panel's device lighting its rows in the grid; while the keys are in a
   device, its column's values as `S`'s bars over the other columns (Decided 7). Depends on 3.
5. **`S` and `gl` with the panel open (S).** `S`'s box in the panel's place, `L` landing in the
   panel, `gl` moving the panel, and `gL`. Depends on 3.
6. **Lit rows from the panel's buffer (S).** `token_rows` reads the open token buffer as well as
   the `S` panel, and #1106's dialog is placed in the area it's given. Depends on #1106 and 3.
7. **Later, if asked for:** the `S` panel as a `New` device (question 5) and a setting for the default
   form (question 6).
