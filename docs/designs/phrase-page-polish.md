# Phrase page: a fit-and-finish study

**Status:** a study for the author to read, written on 2026-11-12. No code is changed. It looks at the
phrase view (`Space 3`, `crates/vt-tui/src/views/phrase.rs`) beside the pages that have had their
polish pass: the track page's Sound, Mod and FX devices, the mixer and the sample page.

**In one line:** the grid's bones are right, and most of what feels unfinished is small: empty cells
that shout as loud as the bar lines, a playhead that paints empty cells green, an overview whose
notes and empties look the same, and a title that runs into itself. Eight small fixes come first;
three bigger ideas follow, for later.

---

## How it was looked at

The release build in a kitty window of its own at 163x42, driven with `scripts/live.sh`, with copies
of two of the author's songs, `sweet-groove` and `daft-punk-03`, in a scratch folder (the originals
weren't opened for writing). A few values were added to the copies to see every kind of cell: `CUT`
locks with each mark, a `KIL` event, a nudge, a gate length and a live velocity ramp. The pictures
were kept out of the repository and deleted; what they showed is described in words.

* **Stopped, `sweet-groove`, track `BD`.** Sixteen rows of `C-3 64` on the beats, with everything else
  `---- --` and `---   00` in faint grey. The grid and the overview take the left 100 columns and the
  top 18 rows; the main meter sits on the right; the rest, about 45 columns by 20 rows, is empty.
* **Track `03`, the acid line, in the timeline.** `MLT2` repeats its note on a continuation line,
  `SGL2` and `SGL3` hold theirs with a `│`, `SLID 1000` with its lock. Rows `0A` to `0F`, outside
  the loop, are faint. Beat rules sit under rows `02`, `05` and `07`, before the yellow `03`, `06`
  and `08` that start beats in time, and a bar's under `09`, the loop's end.
* **Playing.** The playhead row turns every cell green, the empty dashes and the empty `00`s too, so a
  silent row is a bright green stripe. In the overview the playing line reads `>C-3>C-3>F-2>--->---`,
  the markers butting into the notes before them.
* **Cursor on an FX value.** The rosewater cursor covers the coarse digits of `1000`; the status line
  says `Acid: Slide, 79 ms`, which is the page's inspector, and a good one.
* **Visual.** The selection's grey with bold text, over the notes and the overview's column for the
  same track. Clear.
* **Insert.** A green box with square corners appears around the grid, with `Note` and `FX` set into
  its top edge in green. The cursor on the nudge field is a block over two blank cells.
* **Shape panel and a live column.** The panel opens as a rounded window on the overview; kept live,
  the velocities turn teal and the title gains a wand and `Vel` in teal.
* **`daft-punk-03`, with marks.** `CUT 8000` with a dim lock, `CUT 2000 h` and `CUT C000 g` with the
  letters in plain text colour, `KIL 03` in peach, `+4` and `08` dim.
* **The other pages.** Sound, Mod and FX are devices: rounded frames, a dim name in the top edge,
  dim labels, bright values, mauve bars, the focused device's frame brighter. The mixer heads its
  parts with dim rules, `─ Tracks ───` and `─ Returns ──`. The song view is a plain table. The
  sample page is a waveform with a dense settings block under it.

---

## What's good, and should stay

* **The columns are honest.** Fixed widths, hex everywhere, one character a cell, and the cursor is
  exactly the field it's on. Nothing jitters as values change.
* **Colour already means things.** Mauve for a setting's value, its fine digits set back; peach for
  an event; teal for a live column; green for the playhead and Insert; rosewater for the cursor;
  yellow for beat numbers; a group's colour on its tracks. None of it is decoration.
* **Empty is faint, written is bright,** as a code editor dims whitespace, so the shape of a phrase
  reads from across the room.
* **The FX band** (a shade darker under the FX slots) splits the row into its two parts without a
  line.
* **The gutter** says three things without labels: relative counts for `j` and `k`, the row in hex,
  and the beat in yellow. Rows outside the loop go faint.
* **The timeline's continuation lines,** with `│` under a held note and the note again for `MLT`,
  make a row's length visible, which no other tracker does as plainly.
* **The status line as inspector:** `Nudge, 4/24 of a step late`, `Gate length, 3 %`,
  `Acid: Slide, 79 ms`. It answers "what is this cell" without a help screen.

---

## Where it's out of step with the other pages

1. **Frames.** Every other window and device has rounded corners (decision 103). The phrase grid has
   no frame in Normal and a square-cornered green one in Insert.
2. **Headings.** Devices name themselves in a dim label set into their top edge, and the mixer heads
   its parts with dim rules. The grid has two loose dim words, `Note` and `FX`, and the overview's
   track labels sit up on the title row, three rows from the notes they name.
3. **Title.** The track page reads `Track 03 G1  Sound  Mod  FX`, two spaces between its parts. The
   phrase reads `Track 03 G1 Phrase 00 1/1`, single spaces, all bold, so it's one long word. The
   chain view says `Track 01` where the phrase says `Track BD`, and has no machine glyph.
4. **`*` means two things.** `Phrase 00*` (and `Chain 00*`) is a shared phrase; ` *` at the end of
   the status line is unsaved changes.
5. **Mark vocabulary.** A lock is a Nerd Font glyph, dim; hold and glide are the letters `h` and `g` in
   text colour. Elsewhere every switch and state became a glyph (decisions 101 and 104).
6. **Empty cells.** Velocity and length show `--` when empty, an empty FX slot shows `00` as if it had
   a value.
7. **Use of the screen.** The device pages fill the width; the phrase page uses about 100 of its 150 columns
   and, for a 16-step phrase, 18 of 39 rows.

---

## Proposals

Mocks are cell for cell, as `phrase.rs` lays a line out: a 9-cell gutter, 56 cells of steps (note,
velocity, nudge, gate and length in 17, then three FX slots of 13 from cell 17), two cells, then the
overview. Nerd Font glyphs don't render on GitHub, so the mocks use one-cell stand-ins:

| Stand-in | Glyph |
|---|---|
| `S` | the track's machine glyph, as `nf-md-piano` |
| `L` | a lock, `nf-md-lock` U+F033E |
| `P` | hold, proposed: `nf-md-pin` U+F0403 |
| `/` | glide, proposed: `nf-md-slope_uphill` U+F0E00 |
| `@` | a shared phrase, proposed: `nf-md-link` U+F0337 |
| `W` | a live column's wand, `nf-md-auto_fix` U+F0068 |

The proposed codepoints were checked in the Symbols Nerd Font Mono that kitty ships.

### Do now: fit and finish

#### 1. Empty value cells as dots, and no `00` in an empty slot

Before:

```
    00   B-1 64    ---- -- CUT   8000 L ---   00     ---   00       B-1 C-3 --- ---
 1  01   D-4 64    ---- -- ---   00     ---   00     ---   00       D-4 --- --- ---
 2  02   B-2 64    HLD1 -- ---   00     KIL   03     ---   00       B-2 --- --- C-3
 3  03   A-3 64    ---- -- ---   00     ---   00     ---   00       A-3 --- --- ---
 4  04   G-2 7F    HLD1 -- CUT   2000 h ---   00     ---   00       G-2 C-3 C-3 ---
 5  05   E-2 7F +4 ---- -- ---   00     ---   00     ---   00       E-2 --- --- ---
 6  06   --- --    ---- 08 ---   00     ---   00     ---   00       E-1 --- --- C-3
```

After:

```
    00   B-1 64    ···· ·· CUT   8000 L ···   ··     ···   ··       B-1 C-3 --- ---
 1  01   D-4 64    ···· ·· ···   ··     ···   ··     ···   ··       D-4 --- --- ---
 2  02   B-2 64    HLD1 ·· ···   ··     KIL   03     ···   ··       B-2 --- --- C-3
 3  03   A-3 64    ···· ·· ···   ··     ···   ··     ···   ··       A-3 --- --- ---
 4  04   G-2 7F    HLD1 ·· CUT   2000 h ···   ··     ···   ··       G-2 C-3 C-3 ---
 5  05   E-2 7F +4 ···· ·· ···   ··     ···   ··     ···   ··       E-2 --- --- ---
 6  06   --- ··    ···· 08 ···   ··     ···   ··     ···   ··       E-1 --- --- C-3
```

The note keeps `---`, the tracker's sign for "nothing plays here"; velocity, gate, length and the FX
slots, which are values with a default underneath, become faint middle dots, and an empty slot's
value is `··` rather than `00`.

* **Why:** it's #313's first option. A row of dashes is a line at mid-height in the same grey as the
  bar rule, so the eye joins them; dots can't be read as a line, so the rule is the only line left.
  It also stops an empty slot claiming a value of `00`. Prior art: Renoise keeps `---` for an empty
  note and draws empty instrument, volume and panning cells as `..`, and OpenMPT an empty row as
  `... .. .. ...`; the M8 and tracker-ce (the Polyend Tracker) use dashes, and the M8 shows
  `---00` in an empty FX slot, which is where the `00` came from.
* **Cost:** small. `step_text` and the constants `GATE_EMPTY` and `LENGTH_EMPTY`; tests that
  compare grid text change with it.
* **Risk:** low. Dots are lighter than dashes, so the grid gets quieter, which is the aim. The
  cursor on an empty FX value covers `··` rather than `00`, which reads as "type a value".

#### 2. The overview's empty notes faint, its notes dim

Before, every overview cell is `dim` (`OVERLAY2`), notes and `---` alike:

```
 B-1 C-3 --- ---      every cell OVERLAY2
 D-4 --- --- ---
```

After, the grid's own rule, "empty is faint":

```
 B-1 C-3 --- ---      B-1, C-3 in OVERLAY2; each --- in OVERLAY0 (faint)
 D-4 --- --- ---
```

* **Why:** in the screenshots the overview is an even grey block; where the other tracks play is
  hard to see, which is the overview's one job. Elektron's step LEDs are off, dim or lit, never the
  same for an empty step and a trig.
* **Cost:** small, one branch in `column_spans`.
* **Risk:** none found.

#### 3. The overview's playhead gets its own cell

Before, playing (`NEIGHBOUR_WIDTH` 4; the marker shares the gap):

```
 3  03 > A-3 64    ---- -- ---   00     ---   00     ---   00      >A-3>--->--->---
```

After (`NEIGHBOUR_WIDTH` 5: marker, note, gap), with proposal 1's dots:

```
 3  03 > A-3 64    ···· ·· ···   ··     ···   ··     ···   ··      >A-3 >--- >--- >---
```

* **Why:** the marker of one column touches the note of the one before, so the playing line reads as
  one string. A cell of its own keeps each track a column.
* **Cost:** small: the constant, and the labels' padding in `column_label`.
* **Risk:** low. Eight tracks take 40 columns rather than 32, and there are about 80 free at 163 columns,
  so 16 tracks still fit before the overview has to scroll around the cursor, as it already can.

#### 4. The playhead row: green for what's written, dimmed green for what's empty

Before: every cell of the playing row in `active` green, dashes and `00`s included.

```
 3  03 > A-3 64    ---- -- ---   00     ---   00     ---   00      (all green)
```

After: written cells green, empty cells in `theme::dimmed(GREEN)`, the same green pushed toward the
screen, so the row still reads as one lit bar but its shape stays:

```
 3  03 > A-3 64    ···· ·· ···   ··     ···   ··     ···   ··      A-3 64 green; dots dimmed green
```

* **Why:** an empty row playing is today the brightest thing on screen. The device pages light what
  moves (a bar's fill follows what's playing, decision 21) rather than everything near it. The
  marker stays `>` (or a narrow `▸`, U+25B8); a Nerd Font play glyph is drawn two cells wide over
  the space after it (see `ICON_SPACE` in `glyphs.rs`), which would crowd the note.
* **Cost:** small: the empty-cell patch in `draw` applies its look to the playhead row too.
* **Risk:** low. At fast tempos a mostly empty phrase shows less flash; the `>` and the dimmed green
  still mark the line.

#### 5. A title in groups, as the track page's is

Before:

```
S Track 03 G1 Phrase 00* 1/4 FX1:3 W Vel                            BD  CP  03  04  05  06  07  08
```

After (the labels move down, proposal 6):

```
S Track 03 G1  Phrase 00 @  1/4  FX1:3  W Vel
```

`Track 03 G1` and `Phrase 00` stay bold; `1/4` and `FX1:3` go dim, as the device titles' second
words do; a shared phrase gets the link glyph in place of `*`.

* **Why:** two spaces between groups, as `Track 03 G1  Sound  Mod  FX` has, make it scan as parts.
  The `*` is freed to mean only "unsaved", as on the status line.
* **Cost:** small, in `draw`'s heading and the chain view's title (which should also show the
  machine glyph and the track's name, as the phrase's does).
* **Risk:** low. The chain view's `*` changes with it; the guides mention `*` for shared phrases in
  `chains.md`, which would need the glyph instead.

#### 6. One headings row: the FX slots numbered, and the track labels over their columns

Before (rows 1 and 2):

```
S Track 03 G1 Phrase 00* 1/4 FX1:3 W Vel                            BD  CP  03  04  05  06  07  08
         Note              FX
```

After (with proposal 3's five-cell columns):

```
S Track 03 G1  Phrase 00 @  1/4  FX1:3  W Vel
         Note              FX 1         FX 2         FX 3           BD   CP   03   04   05   06   07   08
```

* **Why:** the track labels name columns, so they belong in the headings row, one line above the
  notes, as `Note` is, rather than on the title line with the CPU figure. The title row is then a
  title, as on every other page. Numbering the slots matches the title's `FX1:3` lanes and the FX
  page's numbered devices.
* **Cost:** small. The labels' code moves from `title` to `top_rule`; in Insert they sit to the right
  of the box's top edge.
* **Risk:** low. A muted track's glyph and a folded group's mark travel with the label.

#### 7. The Insert box with rounded corners, its headings spaced like a device's

Before:

```
        ┌Note──────────────FX────────────────────────────────────┐
    00  │B-1 64    ---- -- CUT   8000 L ---   00     ---   00    │  B-1 C-3
        └────────────────────────────────────────────────────────┘
```

After:

```
        ╭Note ──────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮
    00  │B-1 64    ---- -- CUT   8000 L ---   00     ---   00    │  B-1  C-3
        ╰────────────────────────────────────────────────────────╯
```

* **Why:** decision 103, "every window has rounded corners"; and a label set into a frame with a
  space either side is how the devices and the shape panel do it.
* **Cost:** small: the four corner characters and the heading's padding.
* **Risk:** none found. Each label keeps the column it heads.

#### 8. Hold and glide as glyphs, like the lock

Before:

```
    00   B-1 64    ···· ·· CUT   8000 L ···   ··     ···   ··       B-1 C-3 --- ---
 4  04   G-2 7F    HLD1 ·· CUT   2000 h ···   ··     ···   ··       G-2 C-3 C-3 ---
 8  08   G-3 64    HLD1 ·· CUT   C000 g ···   ··     ···   ··       G-3 C-3 --- ---
```

After:

```
    00   B-1 64    ···· ·· CUT   8000 L ···   ··     ···   ··       B-1 C-3 --- ---
 4  04   G-2 7F    HLD1 ·· CUT   2000 P ···   ··     ···   ··       G-2 C-3 C-3 ---
 8  08   G-3 64    HLD1 ·· CUT   C000 / ···   ··     ···   ··       G-3 C-3 --- ---
```

The lock stays dim, as the default; a pin for hold and a rising slope for glide, both in the value's
fine colour, so the two marks that change what a value does are seen beside it and belong to it.

* **Why:** one vocabulary for the three marks, and decision 104's rule of a glyph in place of a
  word, which the lock and the FX page's bypass switch already follow.
* **Cost:** small: `Mark::glyph` stays the letter for text and the command line; the grid swaps the
  glyph in, as it does for the lock. Two constants in `glyphs.rs`, and `scripts/symbols-font.sh`
  for the manual.
* **Risk:** medium. `h` and `g` are what the guide teaches, and `~` cycles them by those names; the
  guide's table would show the glyphs beside the letters. **Alternative:** keep the letters, in the
  value's fine colour, and leave the lock as it is.

### The beat and bar rules (#313)

The rule is an underline under the row before a beat (`SURFACE1`) or a bar (`OVERLAY0`). The author
found it mixed: it sits a little inside the row, and lines up with the dashes of empty cells. The
second follows from the colours: the bar rule is `OVERLAY0`, the same grey as the faint dashes.

Before (rows `02` to `05` of `daft-punk-03`; `__` marks a row whose cells carry the rule):

```
 2  02   B-2 64    HLD1 -- ---   00     KIL   03     ---   00
 3  03   A-3 64    ---- -- ---   00     ---   00     ---   00     __ beat rule, SURFACE1
 4  04   G-2 7F    HLD1 -- CUT   2000 h ---   00     ---   00
 5  05   E-2 7F +4 ---- -- ---   00     ---   00     ---   00
```

The options, from the issue and from looking:

* **A. Change the empty marks** (proposal 1). Dots can't join the rule into a line. Cost small,
  risk low, and it fixes the `00` too.
* **B. A rule for bars only.** Beats keep their yellow numbers and lose the line. Cost tiny. With
  16-step bars a phrase of 16 shows one line at its loop end, so the grid loses most of its rhythm.
* **C. Move the line down, in the terminal.** kitty's `modify_font underline_position` and
  Ghostty's `adjust-underline-position` both move it, by a pixel or two (the sign that moves it
  down is to be tried on the author's screen). The program can't set them;
  `docs/terminal-notes.md` could suggest them. Cost tiny, but it moves every underline in the
  author's terminal, nvim's included.
* **D. A band on the beat's row,** Renoise's way: the row that starts a beat on `SURFACE0`, a shade
  lighter than the screen, in place of a line. No line at all, so nothing to line up. Cost small.
  Risk medium: the FX band, a selection and the cursor are backgrounds too, and four kinds of
  background on one grid is a lot to read.
* **E. An overline on the beat's own row,** which kitty and Ghostty draw (SGR 53) but ratatui has no
  modifier for. Cost medium to high (writing the escape outside ratatui's buffer); and decision 106
  found a mark on the beat's row reads as the yellow number does, rather than as a line between.
* **F. Drop it,** and keep the yellow numbers alone, as the M8 and the Polyend Tracker do.

**Recommendation:** A now, then live with it a week. If the line still reads as inside the row,
add C to `docs/terminal-notes.md` as a suggestion. B and D are the fallbacks, F the last.

### Bigger ideas

#### B1. The grid as a device: a frame always there, dim in Normal, green in Insert

The box's rows are already kept (`ROWS_AROUND_STEPS`), so a frame in Normal moves nothing. It would be
`SURFACE1` with its headings dim, as an unfocused device's is, and turn green in Insert:

```
        ╭Note ──────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮ ╭ Tracks ─────────────────────────────╮
    00  │B-1 64    ···· ·· CUT   8000 L ···   ··     ···   ··    │ │ B-1  C-3  ---  ---                  │
```

* **Why:** the phrase page would read as the same family as Sound, Mod and FX: framed parts with
  their names in the edge. Insert stops being a box that appears and becomes the frame lighting up,
  as a focused device's frame does.
* **Cost:** medium: the frame for both parts, and the overview's width at the frame's edge.
* **Risk:** medium. Two more vertical lines and a frame around the overview make the page heavier;
  spartan may want only the grid framed, or neither.

#### B2. A row inspector in the empty right-hand space

At 163 columns with eight tracks about 40 columns sit empty right of the overview. A device there
could spell out the cursor's row, as the Sound page spells out a voice:

```
╭ Row 04 ─────────────────────────────╮
│                                     │
│  Note      G-2     MIDI 43          │
│  Velocity  7F      ▕█████████▏      │
│  Gate      Hold    1 pulse          │
│  Length    --      the track's 50%  │
│  FX 1      Cutoff  20.00  hold      │
│                                     │
╰─────────────────────────────────────╯
```

* **Why:** the status line describes one field; this describes the row, with the words a value
  means, as the M8's and Elektron's parameter pages do. It would also give the page the same weight
  as the device pages.
* **Cost:** medium: a new view drawn from `Step` and the command registry's names.
* **Risk:** medium. It repeats the status line's work, and with 16 tracks the overview needs the room
  back, so it has to give way. It might better be a toggle than always there.

#### B3. The chain in the title, in place of `1/4`

`1/4` says where in the chain this phrase is, but not what's around it:

```
S Track 03 G1  Chain 02  00 03 03 07  W Vel
                            ^^
```

The chain's phrases in order, this one in the cursor's colour (`^^` here), a shared phrase's number
with the link glyph. `]p` and `[p` then visibly move along it.

* **Why:** the M8 and Elektron's pattern pages keep the position in the song on screen; here it
  would answer "which 03 is this" when a phrase repeats in a chain.
* **Cost:** small to medium.
* **Risk:** low; a long chain is cut at the title's width with `…`.

---

## Order of work, if taken up

1. Proposals 1, 2 and 7 together: empty cells and the box, all in `phrase.rs`, one PR (#313 option A).
2. Proposals 3, 5 and 6: the overview's width, the title and the headings row, one PR.
3. Proposal 4, the playhead row, on its own so it can be judged playing.
4. Proposal 8, after the author picks glyphs or coloured letters.
5. The bigger ideas, each as its own study or issue, if any of them appeal.
