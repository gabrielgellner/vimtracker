# Rich input for generator grammars, and notation from live coding

**Status:** a design study written on 2026-11-20, and decided by the author the same day. Their
answers to §10 are under it, as **Decided**, and the body is brought in line with them: a `List`
as long as a phrase (§4.1), kit lines built (§5.3), and the plan (§9). No code is changed and no
decision numbers are reserved.

The author's request of 2026-11-20, in three parts:

1. *"The whole "live coding" approach that people do (often in javascript frameworks I beleive)
   where they kind of code sequencers/synths with some core primatives in a kind of notebook like
   interactive code interface. Can we survey those platforms and see if anything from that world
   would fit naturally into vimtracker as we are kind of briding a tracker like experience with a
   text editor experience in a groovebox"*
2. *"I am not looking for the "coding" part, rather how my kind of generator/text editor vibe can
   be used by taking some of the notation etc to make interesting generators/machines."*
3. *"have the focus be on how we might make stuff we have or stuff we don't have get a more
   enriched input method, so if we use these generator "grammars" then editing those kinds of
   inputs would have richer input tools so that we could have completion etc, syntax navigation
   etc. Basically like how we started to work on some richer input interactions with the note
   probabilities of some of the random generators."*

So the centre is **how a short string of a generator's is edited**: the fields that are text-like
already, and one editor that gives them all Vim's motions, `=` and `-`, completion, checking, a
preview and the playing token lit. New grammars, Tidal's mini-notation first, come after, as what
that editor makes worth having.

**What stays out.** Live coding, "running code generates the music during playback", is a
non-goal ([`vimtracker.md`](vimtracker.md), Non-Goals), and stays one. Here text is **data**: a
string is a generator's setting, rendered into the grid as any setting is. There's no embedded
language, no REPL, and no user code on the audio thread.

**It builds on:**

* [`generators-page.md`](generators-page.md), the Gen page and generators in the engine, whose
  shape this study follows;
* decisions 178 to 182 (live generators), 204 and 269 (`Mutate` and keeping its draws), 268 (`S`
  generating), 270 (`Space g A`), 273 (`Acid`), 293 (`Vary`), 300 (what was heard), 387
  (`Space d`), 388 (the `Weights` digits) and 425 (each weight a setting);
* the code: `vt-model`'s `param.rs` (`Typing`), `shape.rs` (`Values`, `Where`, `FillKind`),
  `shape/params.rs`, `shape/render.rs` (`render_into`, `GenCell`), `generator.rs`, `condition.rs`,
  `dice.rs`, `phrase.rs`, `lane.rs`, `speed.rs` and `steptext.rs`; `vt-tui`'s `input/typed.rs`,
  `overlays/shape/draw.rs` and `overlays/command/completion.rs`;
* the guides `generators.md`, `track-gen.md` and `editing.md`.

**The recommendation in one paragraph.** Build **one token editor** for every text-like setting,
the `Weights` digits of decision 388 grown up: a field you go into with `l`, whose cursor sits on a
whole token (a note, a value, a move, a group), with `h` `l` `w` `b` `%` to move by the grammar,
`=` and `-` turning the token in its own units (a note by a scale degree, a chance by 10%, a Euclid
count by a hit), `x` and `~` to take one away or rest it, `ci[`-style text objects, `K` listing
what can go where the cursor is, `Tab` completing as the `:` line does, the text checked on every
key with the bad token marked while the last good text keeps playing, the rows each token writes
lit in the grid, and the token playing lit as it plays. Each grammar is declared once in
`vt-model`, as a generator's settings are (§3.3), so the panel, the Gen page, the status line and
`Space d` read one table. **First** move the fields that exist onto it: a `List`'s `Values`, the
Gen page's `Weights` cell, a `Tonnetz`'s `Moves`, a `Bits` mask, a typed rhythm (§4). **Then** add
two grammars from Tidal: an `Own` rhythm, `x..x[xx]x(3,8)`, as a `Where` any fill can take, and
`Mini`, a fill whose string holds both rhythm and values in the column's own units (§5), and then
kit lines, one `Mini` text kept the same across a group's drum tracks (§5.3). All are
**stepwise**: a token is a number of rows, not a fraction of a cycle, since tracks of different
lengths, lanes and `Speed` already give vimtracker its polymeter. FX commands are still chosen with
`K` on the column, and only their values are ever typed.

---

## 1. Prior art

### 1.1 The live-coding platforms

| Platform | Its model | Licence | What to take |
|---|---|---|---|
| **TidalCycles** (`../dsp/tidal`; `tidal-core/src/Sound/Tidal/ParseBP.hs`, `UI.hs`, `Core.hs`) | Patterns are functions of time; a **cycle** is the unit, and a mini-notation string (`"bd*2 [~ sn] hh?"`) divides it. The parser builds a tree (`TPat`): `Seq`, `Stack` (`,`), `Polyrhythm` (`{}` and `<>`), `Fast` and `Slow` (`*` `/`), `Elongate` (`@` `_`), `Repeat` (`!`), `DegradeBy` (`?`), `CycleChoose` (`\|`), `Euclid` (`(3,8,2)`), `EnumFromTo` (`..`), feet (`.`). Every atom keeps its place in the source (`TPat_Atom (Just ((col, line), (col, line)))`), which is what lights it as it plays. Each `?` and `\|` takes a seed of its own from a counter as it's parsed (`newSeed`). Transforms (`rev`, `iter`, `ply`, `chunk`, `every`, `off`, `jux`, `palindrome`, `degradeBy`, `sometimesBy`, `struct`, `mask`, `segment`, `range`) are Haskell functions on patterns. | GPL-3.0 | The grammar and its meanings, adapted with credit (§8): the operators, the per-`?` seed, the source place carried on each atom. Euclid's third number rotates **earlier** (`rotL` in `_euclidOff`), where ours rotates later, so the two read differently. |
| **Strudel** (`../dsp/strudel`) | Tidal in JavaScript in the browser, with a CodeMirror editor. The same mini-notation; `<a b c>` is "one per cycle", and its docs note that this is "more similar to traditional music sequencers". **Stepwise patterning** (`learn/stepwise.mdx`, marked experimental): count steps rather than cycles, `pace(8)` plays any pattern at 8 steps a cycle, `^` marks which level counts as steps. **Active-token highlighting** (`packages/codemirror/highlight.mjs`): each atom's source range is a mark in the editor, and the ones whose events are sounding are lit. The `punchcard` draws the events as a grid. On an error, the last pattern that ran keeps playing (*from memory, not checked in the code*). | AGPL-3.0 | **Ideas only, never code:** steps over cycles; lighting the playing token from source ranges; a grid preview of what a string makes; keeping the last good text playing. |
| **Sonic Pi** | Ruby; `live_loop :name do … end` blocks that loop, re-read when the buffer is run again, and wait at their loop's end; `sync`/`cue` line one loop up with another; `ring`, `spread(3, 8)`, `one_in`, `choose`, `tick`/`look`. (*From memory.*) | MIT (*not checked*) | That a change lands at the **end of its loop**, never mid-loop: our track's `Cue` already does this for `Length` and `Start` (§6.3). Rings and `tick` are what `List` and `Run` do. |
| **Orca** (Hundred Rabbits) | A 2D grid of single-letter operators run each frame: `D` (delay, bangs every n frames), `C` (clock), `R` (random), `X` (write a value to another cell), `*` (bang), `:` (send a MIDI note). Operators bang their neighbours; lower case waits for a bang. (*From memory.*) | MIT (*not checked*) | Very little, as §7 says: what fits is "a cell that triggers another track", and the trig conditions on the track to the left (`CND` `NEI`) are most of it already. |
| **FoxDot** | Python; `p1 >> pluck([0, 2, 4], dur=[1, 1/2])`; `P[...]` patterns with `[]` for subdivision and `()` for stacks; `PDur(3, 8)` for Euclid; degrees by default, in the scale set for the clock. (*From memory.*) | CC BY-SA 4.0 (*not checked*) | Scale degrees as the atom on a melodic line, with the scale held elsewhere: here the song's scale. |
| **Sardine** | Python, with its own pattern language for strings (`"C E G"`, `"[1:5]"` ranges, `!` repeat, `?` chance, `..`); "swimming functions" are coroutines called again each loop. (*From memory.*) | GPL-3.0 (*not checked*) | Ranges such as `1..5` as an atom, which Tidal has too. |
| **Glicol** | Rust and WebAssembly; a graph language, `o: sin 440 >> mul 0.1`, with a `seq` node taking `"60 _60 _ 48"`, `_` a rest. (*From memory.*) | MIT (*not checked*) | Nothing new: its sequence string is a small mini-notation, and its audio graph is our machines' business. |

### 1.2 Richer input for short strings

| | What it does | What to take |
|---|---|---|
| **Decision 388, the `Weights` digits** | Each digit over its note; `h` `l` across; `=` `-` by one, `+` `_` by 4; a digit typed sets and moves on; the status line names it, `Weight of G: F` | the model the editor grows from (§3) |
| **The Mod page's Steps box** | A row of values, `▲` under the cursor's step, `h` `l` `0` `$`, `=` `-`, `i` types one | the same shape for a row of values |
| **The `:` line's `Tab`** (`completion.rs`) | Vim's command-line completion: a list under the line, `Tab` and `Shift-Tab` through it, typing narrows it, the words from the parser's own tables | completion in a field, from the grammar's own tables (§3.5) |
| **Vim's text objects and `Ctrl-a`** | `ci[`, `da<`, `yi(`; `Ctrl-a` and `Ctrl-x` add to the number under the cursor | text objects by the grammar's brackets; `=` and `-` already are this app's `Ctrl-a` |
| **Tree-sitter's text objects** (nvim-treesitter-textobjects) | Motions and objects by the syntax tree, not by characters: next function, inner argument, swap two arguments | moving by token and by group rather than by character (§3.2) |
| **Strudel's highlighting** | Source ranges of the atoms sounding are lit | the playing token (§3.7) |
| **The M8's tables and the Elektron boxes' step entry** | Never a free text field: a cursor on cells, each turned in its own units | why `=` and `-` on a token matter more than typing (§3.4) |

**What we take:** Tidal's grammar, adapted with credit; Strudel's ideas of steps, a grid preview,
a lit token and keeping the last good text, as ideas; Sonic Pi's change at the loop's end; Vim's
text objects and Tree-sitter's moving by syntax; and our own `Weights` digits, Steps box and `:`
line, which already do half of it.

---

## 2. Where things are now

### 2.1 The text-like settings in the code

`param.rs`'s `Typing` says how `i` reads a setting, and `input/typed.rs` holds what's typed. Each
of these is a short string with a grammar of its own, and each is edited one way or another today:

| Setting | Where | Its grammar (`Typing`) | How it's edited now | What it lacks |
|---|---|---|---|---|
| **`List`'s `Values`** | shape panel, Gen page | `HexList`: up to 8 hex values, a space by itself after each two digits (`704055`); on the Note column `Named { list: true }`, notes with spaces (`C-3 D#3 G-3`) | `i` from an empty field, the whole list typed again | no cursor on one value; no `=` on one; no limit shown until it's hit; notes typed by name with the scale nowhere in sight |
| **`Weights`** | shape panel | `Weights`: a hex digit a degree | digit by digit (decision 388) | nothing: it's the model |
| **`Weights`** | Gen page, the fill's cell | the same | `i` from empty, "left as they were" by decision 388 and kept by 425 | what the panel has; the `Weights` device under it has a cell a degree (decision 425), so this cell is only a fast way in |
| **`Tonnetz`'s `Moves`** | shape panel, Gen page | `Moves`: `P`, `L`, `R`, up to 8, spaces as wanted | `i` from empty | the panel's last line names the chords (`C Cm G# Fm …`) but not under their moves; no cursor on one move |
| **A `Bits` mask** | `Invert bits`, `Mask bits`, `Rotate bits` | one byte, `Hex` | `=` and `-` step the byte as a number | the byte **is** eight switches, and stepping it as a number moves them all at once |
| **Named rhythms** | `Where` `Rhythm` | none: `K` picks one of eleven | `K` | the guide prints each as `x..x..x...x.x...`, but you can't edit one or type your own |
| **`Rate`** | a `Wave`, mod slots | `Rate`: `1/4`, `2 bars`, `x3` | `i`, or `=` `-` through its list | little; it's one token |
| **The Steps box** | Mod page | signed hex a step | its own cell editor | typing several at once (`0C 00 -07`) |
| **Macro and track names** | `:macro`, `:name` | free text | typed | nothing needed |
| **The `:` line** | everywhere | each command's words | typed, with `Tab` completion | nothing needed: it's the precedent for completion |
| **Step text** (`steptext.rs`) | on disk and in tests | `C-3 64 HLD3 cut:40 --- ret:14` | not typed by hand | nothing here; a text bridge for copying rows to the clipboard could use it later (§7) |

### 2.2 What a new grammar must fit

* **A `Shape` is plain `Copy` data of a fixed size** (`MAX_VALUES` is 8), copied into each
  generator, the undo history and the engine's renders. A string of any length can't live in it,
  and neither can a `List` as long as a phrase (§4.1).
* **The engine renders a moving generator** a pass at a time and again as its inputs change, from
  `render_into`, allocating nothing (decisions 287 to 289), and **passes are positional**: pass 8
  is worked out from where the song is, not from a stream (decision 293).
* **At rest, what's heard is what's written** (`generators-page.md` §7.1): the phrase holds the
  first pass. `Ring` and `Acid`'s loops already run on from pass to pass without `Vary`, the phrase
  holding the first, which is the precedent for a string whose `<a b c>` comes round more slowly
  than the phrase.
* **A row is a step.** A note between rows is a nudge: `Divisions` already places triplets that
  way, nudged off their rows. Several hits in one row are a ratchet, `RET`, as `Follow`'s
  `Ratchet` mutation writes.
* **Polymeter is already the track's:** each track loops at its own length (`chains.md`, "Tracks
  of different lengths"), a lane loops an FX column at its own length and `Rate` (`lane.rs`), each
  track has its own `Speed` (`speed.rs`), and `Order` walks a phrase's rows in twenty-odd ways.
  Trig conditions (`condition.rs`) count passes, `1:4`, `LST`, `>2`; the dice loop (`dice.rs`)
  repeats a track's chances.
* **A track is one sound and mono.** A note column holds one note a row; chords are tracks.

---

## 3. The token editor

### 3.1 The idea

Decision 388 made one row of the shape panel a place the cursor goes into, where each hex digit is
a thing of its own, drawn over what it means. The token editor makes that general: **any setting
whose value is a string with a grammar** is edited as a row of tokens, and the grammar says what a
token is, what it means, what `=` does to it, what can go after it, and what it writes.

Three states, as 388 has them:

1. **On the row:** the field shows its text; `j` `k` go on to the next setting; `i` types it all
   again, starting from the text there, as 388's `i` does.
2. **In the tokens:** `l` goes in, on the first token. The cursor is a token, lit, and the status
   line names it. The keys of §3.2 to §3.6 work here. `Esc` or `Enter` go back to the row; `j` `k`
   leave and move on, as in 388.
3. **Typing:** `i`, `a`, `c` and `s` type at the cursor, with completion (§3.5), checked as typed
   (§3.6), and `Esc` back to the tokens.

**Undo** is the panel's: every change is heard at once and kept with the rest by `Enter`, or put
back by `Esc`. On the Gen page a change is one undo step, a held key one step, as any turn there is.

### 3.2 Moving by the grammar

| Keys | Does |
|---|---|
| `h` `l` | The previous or next token, into and out of groups, in reading order. `h` from the first goes back to the row |
| `w` `b` | The next or previous token **at the same depth**, a group as one token; the phrase view's `w` `b` go by part the same way |
| `^` `$` | The first or last token |
| `%` | The bracket matching the one under the cursor, as Vim's |
| `(` `)` | Out to the start or the end of the group the cursor is in |
| counts | `3l`, `2w` |

A **token** is what you'd name: a note `D#2`, a value `7F`, a rest `.` or `~`, a move `L`, a
modifier `*2` or `?30`, a Euclid `(3,8)`. A **group** is a bracket and all it holds: `[~ D#2]`,
`<G-2 A#2>`.

### 3.3 Text objects

The operators `d`, `c` and `y`, and `p` to paste, take:

| Object | Is |
|---|---|
| `it` `at` | The token; with its modifiers (`G-2*2?`) for `at` |
| `i[` `a[` | Inside the `[ ]` group, or with its brackets |
| `i<` `a<` | The same for an alternation |
| `i(` `a(` | A Euclid's numbers, or with its brackets |

So `ci[` retypes a beat, `da<` takes an alternation away, `yat` then `p` repeats a note with its
`*2`. `.` repeats the last change, as everywhere. `t` for token, as Vim's `it` is a tag, which no
grammar here has.

### 3.4 Turning a token

`=` and `-` turn the token under the cursor in its own units, and `+` and `_` by the larger step,
as on every other setting:

| Token | `=` `-` | `+` `_` |
|---|---|---|
| a note | a scale degree, a semitone with the scale off | an octave |
| a degree (`3`) | one degree | seven, or the scale's length |
| a hex value | 1 | `10` |
| a weight digit | 1 | 4, as decision 388 |
| a `Tonnetz` move | `P` → `L` → `R` | |
| a bit (§4.4) | on or off | |
| `?` or `?30` | the chance by 10% | by 25% |
| `*2` `!3` `@3` | by 1 | |
| `(3,8)` on the 3 | a hit | |
| `(3,8)` on the 8 | a step | |
| a rest `.` or `~` | nothing; `~` makes it a token again (below) | |

* **`x`** takes the token away, as Vim's; on a field of fixed length (`Weights`, a mask), it puts
  back what it starts as, as 388's.
* **`~`** turns the token into a rest and back. While the field is open it remembers what was
  there, so `~` `~` gives it back. It's the rest's own glyph, and `~` swaps From and To elsewhere,
  so it's always "the other way" here.
* **`r`** replaces one token, as in Vim: `r` then the new one, back to the tokens.
* **The status line** says the token in full, in the column's units and where it lands:
  `D#2, the 3rd of 6: beat 2's second half, row 06`. `Space d` adds what it means
  (decision 387): `?30: plays 7 times in 10`.

This is the most important part. As on the M8 and the Elektron boxes, most edits are turning a
value, not typing one, and a string you can turn token by token is a row of knobs.

### 3.5 What can go here: `K` and `Tab`

* **`K` on a token** lists what the token could be, as `K` lists any setting's choices, with what
  each means. `Enter` replaces the token. On a note: the scale's notes round it, an octave each
  way; on a degree: the scale's degrees named by their notes; on an FX column's value: the
  command's values with their meanings where it has them (a filter type's names, a `CND`'s
  conditions), hex where it hasn't; on a group: `[ ]`, `< >`, a rest.
* **`K` between tokens** (`a` then `K`, or after the last) lists what may follow: a token, a
  group, and the modifiers, each with a line saying what it does, `*2  twice in its place`,
  `?  half the time`, `(3,8)  3 hits spread over 8`. It teaches the grammar without a manual.
* **`Tab` while typing** completes the word, as on the `:` line, from the same lists: `D` gives
  `D-2`, `D#2`, `D-3` in the scale; the list narrows as you type; `Shift-Tab` goes back.
* **The context** comes from the track: its scale and root, its column (Note, Velocity, an FX
  command), for a kit string (§5.3) the tracks of its group by name, and From and To where the
  grammar's degrees count from.

**`K` and FX.** FX are chosen with `K`, never by typing a name: that stays. A string on an FX
column never holds a command's name. The command is the column's, chosen with `K` before the
string is (as `S` on an empty FX column does now, decision 268), and the string holds only its
values, as `i` types a value on a row now. Where a command's values are names (`K` lists them on a
row), `K` on the token lists the same names, and the token shows the name.

### 3.6 Checking as you type

* **Every key** parses the text. A token the grammar can't read is marked: drawn in the error
  colour with `✗` under it, and the status line says why, `[ at 5 has no ]`, or
  `H-2 isn't a note`.
* **The last good text keeps playing,** Strudel's rule. The column is written from the last text
  that read, and the status line says so. `Enter` on text that doesn't read is refused, saying
  why; `Esc` goes back to how it was.
* **Limits show before they're hit:** a field of fixed length (`Weights`, a mask) shows every
  cell; a `List` shows its room in the status line, `3 of 256 values`, and the field refuses the
  257th, saying so (§4.1).

### 3.7 The preview, and the token playing

**The grid is the preview.** As now, the phrase changes as the field does, so the grid shows what
the string writes, and you hear it. Two more things tie the string and the grid together:

* **The rows a token writes are lit** in the phrase view's column and in the Gen page's picture,
  and the rows line under the field (Mockup 1) says them in the string's own terms, `x` for a hit,
  `?` for a chance, `.` for nothing. Moving the cursor along the string moves the light along the
  rows.
* **The token playing is lit** while the phrase plays, in the field and on the Gen page, as Strudel
  lights its atoms. Each render keeps which token wrote each row, a small table beside its cells
  (a byte a row, 4,096 at most); the app knows the row playing, as it draws the playhead, so it
  lights that row's token. For a pass after the first, whose `<a b c>` may be on another choice,
  the app renders that pass's table itself, on the edit thread, from the pass number, as passes
  are positional. Nothing new crosses from the engine.

### 3.8 Declared once

As `shape/params.rs` declares a generator's settings once for every view (`generators-page.md`
§3.3), a grammar is declared once in `vt-model`:

* `Typing` gains `Tokens(GrammarId)`; a `Grammar` says how to **split** the text into tokens with
  their places, how to **read** them into the setting (or why not), what each token **means** for
  the status line and `Space d`, how `=` and `-` **turn** each kind, what `K` and `Tab` **offer**
  at a place from a context, and **which rows** each token writes.
* `vt-tui` has **one widget**, the field of tokens, and one key table for it, in `bindings.tsv`
  as a mode of its own beside the 388 digits, which become a grammar of it.
* A test fails while a grammar's token kind has no meaning, as decision 387's tests do for
  settings.

### Mockup 1: a `Mini` line on the Gen page, the cursor on a token

How to read it: an excerpt of the Gen page, 112 wide; `^^^` under a token is the cursor, drawn as a
lit token on screen; `━━━` is the token playing, drawn in the playhead's colour.

```
╭─ Mini · Note 00–0F ──────────────────────────────────────────────────────────────────────────────────────────╮
│                                                                                                              │
│  Text      C-2 [~ D#2] <G-2 A#2>*2 C-3?                                                                      │
│                   ^^^              ━━━                                                                       │
│  Rows      x . . . . . x . x . x . ? . . .                                                                   │
│                        ^                                                                                     │
│  Step      4 rows ▕██      ▏   Vary      Once                Seed      r rolls                               │
│                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 TRACK  D#2, the 3rd of 6: beat 2's second half, row 06
```

`Step 4 rows` makes each top-level token a beat (§5.2): `C-2` on row 0, `[~ D#2]` splits rows 4
to 7 so `D#2` is on 6, `<G-2 A#2>*2` plays both choices in rows 8 to 11, `C-3?` on row 12 half the
time. `C-3` is playing.

### Mockup 2: `K` between tokens, after `D#2`

```
│  Text      C-2 [~ D#2_] <G-2 A#2>*2 C-3?                                                                     │
│                ╭─ After D#2  1/9 ───────────────────────────────╮                                            │
│  Rows      x . │ >                                              │                                            │
│                │  › a note       C-2 to C-4, in C minor          │                                           │
│  Step      4 ro│    .            a rest                          │                                           │
│                │    [ ]          a group: split its place        │                                           │
│                │    < >          one of them each lap            │                                           │
│                │    *2           twice in its place              │                                           │
│                │    !2           again, in a place of its own    │                                           │
│                │    @2           twice as long                   │                                           │
│                │    ?            half the time; ?30 for 30%      │                                           │
│                │    (3,8)        3 hits spread over 8            │                                           │
│                ╰────────────────────────────────────────────────╯                                            │
 TRACK  Insert after D#2: K lists what can go here, Tab completes a word
```

### Mockup 3: text that doesn't read

```
│  Text      C-2 [~ D#2 <G-2 A#2>*2 C-3?                                                                       │
│                ✗                                                                                             │
│  Rows      x . . . . . x . x . x . ? . . .     still the last good text                                      │
 TRACK  [ at 5 has no ]; the column plays the last text that read. Esc puts it back
```

---

## 4. The settings we have, on the editor

Each moves onto the token editor as a grammar, in this order, smallest first. Nothing about what
they write changes.

### 4.1 A `List`'s `Values`

* Each value a token: hex on Velocity, Nudge, length and FX columns, a note on the Note column.
* `i` starts from the values there (388's rule), not empty.
* `=` `-` turn one value; on notes, a degree of the song's scale, so a list of notes can be walked
  into a new melody without retyping it; `+` `_` an octave.
* `K` on a note lists the scale round it; on an FX value, the command's named values.
* `x` takes a value out, `p` pastes one, `~` (on the row, as now) turns the list round.
* **Up to 256 values** (Decided 3), a value a row of the longest phrase most songs use, rather than
  8. A list longer than the field's width scrolls by token, keeping the cursor in sight, as the
  `:` line scrolls; a list with more values than the rows it reaches plays only its first ones,
  and the status line says how many are heard.

```
  Values    70  40  55
                ^^
 TRACK  40, the 2nd of 3 values, room for 256: = and - turn it, x takes it out
```

#### Where a longer list lives

Eight values fit in a `Shape` (`Values` is `[u8; 8]` and a count, nine bytes). 256 don't: a
`Shape` is `Copy`, and it's copied into each generator, its `held` shape for macros, every undo
step, a patch's generators and each of the engine's `GenRun`s as the shape it last rendered, so
256 bytes more in it would be paid everywhere, by every fill that isn't a `List`. So **the list
moves off the `Shape` onto the `Generator`**, as `Mini`'s text and `Own`'s will be (§5):

* **The model.** `Shape::values` goes. `Generator` gains `list: Box<[u8]>`, empty unless the fill
  is a `List`, at most `MAX_LIST` (256) values, which replaces `MAX_VALUES`. The shape panel,
  which edits a `Shape` before there's a generator, holds the list beside it, and `S` puts both
  on the generator it makes. Anything that copies a generator's shape to make another (`yy` of a
  generator, a patch's generators, `Follow`'s source) copies the list with it. The list isn't a
  macro target, as it isn't now, so `held` needn't carry it.
* **Typing.** `HexList` and `Named { list: true }` read up to `MAX_LIST`, and `i` still types the
  whole list, but at this length the token editor (§3) is how a list is changed.
* **The engine.** `render_into` takes the list as a `&[u8]` beside the shape, borrowed from the
  generator in the song the version carries, so nothing is copied or allocated on the audio
  thread. A `GenRun` decides whether to render again by comparing the shape it last rendered;
  it gains the list's hash, worked out on the edit thread as `GenScratch::for_song` walks the
  generators, so the comparison stays a few bytes. The cells don't change: a list writes one
  value a row as before.
* **The format.** `GeneratorFile` gains `list`, saved as a list of values (as `Values` is now),
  `#[serde(default, skip_serializing_if = "<[u8]>::is_empty")]`, so a song with no `List` saves
  as before and the format number stays 11. The `values` field leaves the saved `Shape`. An old
  song's `List` loads with no values, as no conversion is written (no backwards compatibility);
  the issue says so, and the tutorial's songs that use `List` are saved again.
* **Patches** carry the list with their generators' shapes, under the same rule.
* **Phrases longer than 256 rows** (a phrase stores up to 4,096) go round the list, as a `List`
  goes round now.

### 4.2 `Weights` on the Gen page

The Gen page's single `Weights` cell gets the 388 digits it was left without: `l` into the
digits, each over its note in a second line. The device under it, a cell a degree (decision 425),
stays for the Roto and macros; the cell becomes the fast way to see them all at once and type
`F08F`.

### 4.3 A `Tonnetz`'s `Moves`

Each move a token, and the chord it reaches **under it**, rather than in a line of its own:

```
  Moves     P    L    R    L
            Cm   G#   Fm   C#
                 ^^
 TRACK  L, the 2nd move: C minor to G# major. = turns it to R
```

`=` and `-` go round `P`, `L`, `R`, so a progression is tried a move at a time; `K` lists the
three with the chord each would reach from here.

### 4.4 A `Bits` mask

Shown as its eight bits, `x` for on, so `Mask bits` reads as what it keeps:

```
  Mask      x . x x . . . x      8D
                  ^
 TRACK  Bit 4 on: = turns it off. 8D in hex
```

`=` and `-` switch the bit under the cursor; the byte stays on the row for macros and the Roto,
which still step it as a number.

### 4.5 A rhythm of your own

The named rhythms are already printed in the guide as strings (`x..x..x...x.x...`). `K` on
`Rhythm` gains `Own`, and choosing a named one and then editing it makes it `Own`, starting from
its bell. Its grammar is §5.1's. So the eleven named rhythms become a rhythm grammar's presets,
and the user's own are one edit away.

### 4.6 The Steps box

The box keeps its cells. `i` on it gains the list form, `0C 00 -07 -05`, typed across from the
cursor's step, so a figure goes in at once.

---

## 5. New grammars

Once the editor exists, a grammar costs a `Grammar` and its render, not a new interface. Two are
worth it first; others are listed in §5.5.

### 5.1 `Own` rhythm: a typed `Where`

A rhythm only, so **any fill** plays on it: an `Acid` on a typed rhythm, a `Random` velocity on
it, a `Wave` on an FX column. The tokens are Tidal's, on hits:

| Text | Means |
|---|---|
| `x` `.` | a hit, nothing (as the guide prints rhythms) |
| `[x x]` | split the step |
| `x*3` | three in the step: a `RET` ratchet on the Note column, or split rows if the step has them |
| `x!3` | three steps of `x` |
| `x@3` | one hit three steps long, the next token three steps later |
| `x?` `x?30` | a hit half the time, or 30% of the time, from the generator's seed |
| `<x .>` | a hit one lap, nothing the next |
| `x(3,8)` `x(3,8,2)` | Euclid's 3 over 8 steps, rotated 2 later, as our `Rotate` (Decided 6) |
| `~` | read as `.`, so Tidal's strings paste (Decided 10) |

Example: `x . [. x] x(3,8)` at `Step 4 rows` writes a kick on beat 1, a push on beat 2's last
sixteenth, and 3 of 8 in the rest.

**Its render** is a bitset of the span, as every `Where`'s is (`render.rs`), so it costs what
`Euclid` costs and the engine runs it unchanged.

### 5.2 `Mini`: rhythm and values, in the column's units

A fill with no `Where`, as `Follow` has none: the string says where and what. The atom is the
**column's** value:

| Column | Atom | Example |
|---|---|---|
| Note | a note by name, `C-3` or `c3`, or a scale degree, `1 3 5`, both in one line (Decided 4); `1` is the scale's root, as `Weight 1` is, in From's octave | `1 [. 3] <5 6>*2 8?` |
| Velocity | hex | `7F 40 [60 40] 40?` |
| Nudge | signed hex | `0 +8 0 -4` |
| Length | hex, `_` to tie into the next (`HLD`) | `40 _ 80 20` |
| an FX column | the command's value in hex, or its name where `K` names them; the command chosen with `K` first | `30 [60 90] A0 <C0 40>` on `CUT` |

**Steps, not cycles.** In Tidal a string divides a cycle, so adding a token makes every token
shorter. Here a top-level token is **`Step` rows**, a setting from 1 to 16 (1 a row, 4 a beat), so
adding a token makes the line longer, as typing a row does. The line loops to fill its span; a
span shorter than the line cuts it. Inside a token, `[ ]` and `*` split its rows; a split that
falls between rows is **nudged** off the nearest row, as `Divisions` places its triplets, so
`[x x x]` in a beat is a true triplet. Two hits on one row keep the first, and the status line
says how many were dropped and which `Step` would keep them. This is Strudel's stepwise `pace`
made the rule rather than the exception. There's no cycle mode and no `Step` of `Span` (Decided 5).

**Laps and passes.** `<a b c>` takes the next choice each **lap**, each time the line comes round,
counted from the chain's start, so `<G-2 A#2>` in a four-row line over a 16-row phrase plays G A# G
A# in one pass. A choice whose round outlasts the pass (`<a b c>` over a span of four laps) carries
on into the next pass, which the engine renders, as `Ring` and `Acid`'s loops do; the phrase holds
the first pass, as always. `?` draws from the generator's seed, the token's place and the lap, so
it's the same every pass while `Vary` is `Once` and new each pass with `Vary` on, as `Chance`
already is.

**Mono.** A stack, `,`, can't go on one Note column. On one track it's refused, saying why. Across
tracks (a `W` selection, or `S` across tracks), each layer goes to the next track, as `Tonnetz`
gives each track a voice of the triad and decision 179 gives each track its own generator. So
`C-2*4, [. G-2]*2` on two tracks is a bass and its offbeat answer.

**Its render** is a walk of the token tree, compiled on the edit thread into a fixed array, a few
hundred bytes, kept on the `Generator` beside its `Shape` rather than in it (§2.2). The engine reads
it from the song it's given, as it reads every row, and `render_into` fills the cells from it, the
lap and the seed, allocating nothing. The text is saved; the array is built again on load.

### 5.3 A kit line, across a group's tracks

On a group of drum tracks (a kit puts one sound on each track), a string whose atoms are **the
tracks' names**: `bd*2 [. sn] hh?`. `S` across the group's tracks makes a `Mini` on each one's Note
column, holding the same text: each track plays the rows where its own name is, at the velocity
notes are typed with. An edit on one rewrites the others, as a `Follow`'s source rewrites its
followers. `K` and `Tab` complete the names from the group. It's the one place a stack, `bd, hh*4`,
is natural, as each layer is another track. It's built (Decided 7), as its own issue after `Mini`.

**How the text is kept the same.** Each track's generator holds the text, as any `Mini` does, and
a `kit` mark naming the group, so each generator renders alone and the engine needs nothing new.
The rules:

* **One text.** An edit to the line on any of its tracks, typed or turned, is made to every track
  the line spans in the same undo step, and each track renders its own rows from it. The tracks
  are the group's at the time `S` made the line.
* **A token names a track** by the track's name, matched without case. A name no track of the
  group has is marked as text that doesn't read (§3.6). `:name` on a track in a kit line renames
  its tokens in the text with it, in the same undo step.
* **Leaving the line.** `dd` of the generator on one track, or a track leaving the group, takes
  that track out of the line and leaves the rest; the text keeps the track's tokens, now marked.
* **What each track plays.** A track plays a note on each row where its name is, at the
  generator's note and the velocity notes are typed with; `?`, `*`, `<>` and the rest work as in
  `Mini`, one seed for the line so the tracks agree.
* **On the Gen page** each track's device shows the whole line, with that track's tokens lit.

### 5.4 Edits while playing

Every setting is heard at once now, and the per-row cache (`generators-page.md` §7.4) makes a
token's turn heard from the next row it writes. That's right for `=` and `-`. A retyped line heard
mid-lap can play half of each, so: **changes land at once, as everywhere; with the track's `Cue`
on, they wait for the line's next lap**, as `Cue` makes `Length` and `Start` wait for the pass.
That's Sonic Pi's `live_loop` without a new switch (Decided 8).

### 5.5 Other grammars, and Tidal's transforms

**Tidal's transforms, against what exists.** Most are here already, under other names:

| Tidal / Strudel | Here already | Gap |
|---|---|---|
| `rev` | `Reverse` fill; `Follow`'s `Reverse`; the `Phrase` device's `Reverse`; `Order` `Reverse` | none |
| `rotate`, `(3,8,2)` | `]r` `[r`; Euclid's `Rotate`; `Follow`'s `Rotate` | none |
| `fast` `slow` | `Speed`; a lane's `Rate`; `RATE` | none |
| `every n` | `CND` `1:4`, `LST`; `Vary` every 2, 4, 8 | none |
| `degradeBy`, `?` | `Chance`; `CHA`; `CND` `%`; the dice loop | none |
| `sometimesBy f` | `CND` `%` on a row's locks | applying a **transform** some of the time; not proposed |
| `struct`, `mask` | `Where` is a mask | none |
| `segment` | `Run`; a ramp on an FX column | none |
| `range`, `scale` | From and To; `Scale` fill; the song's scale; `Pitch in scale` | none |
| `arp` | `ARP` and the arpeggio's settings; `Order` `Arp` | none |
| `palindrome` | `Order` `Up-down`; a lane's `Order`; `:bloom`'s `pendulum` | none |
| `off t f` | `Follow` with `Rotate` and `Transpose` on another track | none |
| `linger` | the `Phrase` device's `Length` and `Start` window | none |
| `ply n` | `Follow`'s `Ratchet`; `RET` | `*n` on a token in `Mini` (§5.2) |
| `iter n` | `]r` once | `<…>` in the string covers most; a `Rotate each lap` setting if missed |
| `chunk n f` | none | a quarter of the line changed per lap: not proposed until missed |
| `jux f` | none: it's stereo, a copy in the other ear | out (§7) |
| `euclid` | `Where` `Euclid` | none |

So the transforms don't need adding as operators; the **in-string** ones (`*`, `!`, `@`, `?`,
`<>`, `(k,n)`) are what's new, and they come with the grammar.

**Other grammars the editor makes cheap,** for later, each a question of whether it's missed:

* **Ranges,** `1..5` (Tidal, Sardine), inside `Mini`, expanding to the run of degrees.
* **`|` random choice,** `[C-2|D#2|G-2]`, a token drawn per lap: Tidal's `CycleChoose`.
* **A chord-move line** for `Tonnetz` with groups: `[P L]*2 R`.
* **A lane line:** a string for an FX column's lane, its length the string's.
* **Velocity accents in `Mini`,** `C-2:7F` (Tidal's `:` is a sample index; here it'd be the note's
  velocity), so one string carries both.

---

## 6. What it costs

| | Work | Memory | Code |
|---|---|---|---|
| The editor | a parse on each key, a few microseconds for 64 characters | the text being edited | the widget, the keys, `Grammar`; L |
| A token's rows lit | a byte a row from each render | up to 4,096 bytes a render | S |
| `Own` rhythm | a bitset render, as `Euclid` | the text, and nothing in the `Shape` | M |
| `List` to 256 | as now | 9 bytes less in every `Shape`; up to 256 on a `List`'s `Generator` | M |
| `Mini` | a tree walk a render; at rest, nothing | the text and a fixed array on the `Generator` | L |
| Kit lines | each track renders its own, as a `Mini` | the text on each track's generator | M |
| The song | | new fields, skipped when empty, so no format bump | |

---

## 7. What to leave out, and why

* **An embedded language, a REPL, user code on the audio thread.** The non-goal stands: a string is
  a setting, and the engine runs only our renders.
* **Typing FX names.** The command is chosen with `K` on the column; strings hold values.
* **Chords on one track.** `,` goes across tracks, never into one column.
* **Cycles as the unit of time,** and `fast 1.5`-style fractions of them. A row is a step here, and
  tracks of different lengths, lanes and `Speed` already give polymeter; cycles would be a second
  clock. Steps only, with no `Step` of `Span` (Decided 5).
* **Tidal's `.` feet,** which split a line into groups: here `.` is a rest, as the guides print
  rhythms and trackers show empty rows, and `~` reads as one too (Decided 10). `[ ]` does what feet
  do.
* **`jux`,** a transformed copy in the other ear: a track is one sound, and stereo is the mixer's.
* **`^name` controls** and patterns of patterns as arguments (`"<2 3>"` as `fast`'s argument):
  macros and modulation already reach every setting.
* **Orca as a machine.** Its operators bang cells on a grid each frame, which is a second
  sequencer inside the first. What fits, "a row triggers another track", is close to what
  `CND`'s `NEI` and `PRE` do from the other side; a `TRG` command firing another track's row would
  break "a track plays its own phrase". Neither is built (Decided 9).
* **A step-text bridge** (copying rows to the clipboard as `steptext.rs` lines) is useful, but
  it's about Ableton and other tools, not generators; for its own issue if wanted.
* **Strudel's code,** always (§8).

---

## 8. Licences

* **TidalCycles is GPL-3.0,** as vimtracker is, so its grammar, the meanings of its operators and
  the shape of its parser (`ParseBP.hs`: `resolve_size` for `!` and `@`, a seed a `?` from a
  counter, a source place on each atom) can be adapted, with a comment crediting
  `tidal-core/src/Sound/Tidal/ParseBP.hs` and the TidalCycles authors in the grammar's module, as
  `shape/acid.rs` credits the Endless Acid Banger. The Rust is written fresh; nothing Haskell is
  ported.
* **Strudel is AGPL-3.0:** ideas only. Its docs and highlighting were read for what they do
  (stepwise counting, lighting atoms from their source ranges, a grid preview, the last good
  pattern playing); no code, no text from its docs, no names of its functions beyond the ones that
  are Tidal's.
* **Sonic Pi, Orca, FoxDot, Sardine and Glicol** were not read here; what's said of them is from
  memory and marked so in §1.1, and only ideas are taken.
* **Bjorklund's algorithm** is already in `render.rs`, ours.

---

## 9. Plan: the issues

Filed on 2026-11-20 in the milestone M39: Notation generators, in build order.

| # | Issue | What | Depends on | Size |
|---|---|---|---|---|
| 1 | #1086 | **The token editor:** `Typing::Tokens`, `Grammar` in `vt-model`, the field widget and its keys (§3.2 to §3.4), the error marker and last-good rule, `Space d` meanings; the 388 `Weights` digits moved onto it with no change seen. | none | L, 700–900 |
| 2 | #1087 | **A `List` as long as a phrase:** `Shape::values` moved onto the `Generator` as `list`, up to `MAX_LIST` (256); the panel, `S`, copies and patches carrying it; `render_into` reading it as a slice and the run's key holding its hash; `GeneratorFile`'s `list` skipped when empty, no format bump (§4.1). | none | M, 400–500 |
| 3 | #1088 | **`K` and `Tab` in a field:** the lists at a token and between tokens, from the track's scale, column and group; reusing `completion.rs`'s list. | 1 | M, 400 |
| 4 | #1089 | **The existing fields on it:** `List`'s values with the room in the status line and scrolling, the Gen page's `Weights` cell, `Tonnetz`'s `Moves` with chords under them, a `Bits` mask as bits, the Steps box's list. | 1, 2 | M, 400–500 |
| 5 | #1090 | **Rows lit, token playing lit:** the token table on each render, the rows line, the grid's light, the playhead's token. | 1 | M, 300–400 |
| 6 | #1091 | **`Own` rhythm:** the rhythm grammar, `Where::Own` with its text on the `Generator`, the named rhythms as its presets. | 1, 5 | M, 500 |
| 7 | #1092 | **`Mini`:** the fill, its grammar per column with notes and degrees from 1, `Step`, nudged splits, laps and seeds, `Cue` holding a retyped line to the lap, the stack across tracks; guide and tutorial recipes. | 6 | L, 800–1000 |
| 8 | #1093 | **Kit lines:** one `Mini` text kept the same across a group's tracks, tokens naming the tracks, `S` across the group, edits made to every track in one undo step, `:name` renaming tokens (§5.3). | 7 | M, 400–500 |

**Order:** 1 and 2 side by side → 3, 4, 5 side by side → 6 → 7 → 8. Issues 1 to 5 improve what
exists and are worth having if nothing new is built.

---

## 10. Open questions for the author

1. **The token editor first, for what exists?** Recommended: yes, issues 1 to 4, before any new
   grammar, as they make `List`, `Weights`, `Moves` and masks better on their own.
2. **The keys in a field:** `h` `l` by token into groups, `w` `b` at the same depth, `%` and `(`
   `)` for brackets, `it`/`at` and `i[`/`i<` objects, `~` to rest a token (§3.2 to §3.4)? The
   alternative is `h` `l` by character and `w` `b` by token, closer to Vim's text but further from
   the 388 digits.
3. **`List`'s 8 values:** keep the limit, shown as dim cells, or raise it (16?) now that a list can
   be edited a value at a time? Raising it grows every `Shape`.
4. **Note atoms in `Mini`:** notes by name only (`C-3`), degrees only, or both, with degrees counted
   from From? And degrees from `1` (as `Weight 1` is the root, recommended) or from `0`, as Tidal
   and Strudel count?
5. **Steps or cycles:** stepwise, a top-level token being `Step` rows, recommended; or also a `Step`
   of `Span`, where the string divides its span as Tidal's cycle does?
6. **Euclid's rotation in the strings:** later, as our `Rotate` is (recommended, one meaning in the
   app), or earlier, as Tidal's third number is, so pasted Tidal reads the same?
7. **Kit lines across a group's tracks,** one text kept the same on each track: worth building, or
   is a `Mini` on each track enough?
8. **When a retyped line is heard:** at once, with `Cue` making it wait for the line's lap
   (recommended), or always at the lap?
9. **A `TRG` command, or any Orca-like machine?** Recommended: no; the trig conditions on other
   tracks cover the musical use. Say if a row firing another track is wanted.
10. **`.` as a rest,** rather than Tidal's feet (recommended), and `~` read as a rest too, so Tidal's
    strings paste?
11. **The names:** `Mini` for the fill, `Own` for the typed rhythm, `Step` for rows a token?

**Decided** (the author, 2026-11-20):

1. **Yes, the token editor first,** for the fields that exist: issues 1 to 5 before any new
   grammar.
2. **The keys as recommended:** `h` `l` by token, into and out of groups; `w` `b` at the same
   depth; `%` and `(` `)` for brackets; `it`/`at` and `i[`/`i<` objects; `~` to rest a token.
3. **Raise `List`'s limit to a phrase:** as many values as the longest phrase, up to 256. The list
   moves off the `Copy` `Shape` onto the `Generator`, as `Mini`'s text does (§4.1), with a new
   field skipped when empty and no format bump.
4. **Both** notes by name (`C-3`) and scale degrees in `Mini`, degrees **from 1**, `1` the root, as
   `Weight 1` is.
5. **Steps only:** a top-level token is `Step` rows. No `Span` and no cycle mode.
6. **Euclid's rotation later,** as our `Rotate`, so it has one meaning in the app.
7. **Build kit lines:** one text, kept the same across the tracks of a group it spans, as a part
   of the plan with its own issue after `Mini` (§5.3, issue 8).
8. **A retyped line is heard at once,** and with the track's `Cue` on it waits for the line's lap.
9. **No `TRG`** and no Orca-like machine.
10. **`.` is a rest,** with no Tidal feet, and `~` reads as a rest too.
11. **Keep the names** `Mini`, `Own` and `Step`.
