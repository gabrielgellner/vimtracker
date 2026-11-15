# Lanes: a length a macro turns, and a rate of their own

**Status:** built for #709, the last issue of M33, as decisions 346 to 348, on 2026-11-15. The
design was written before it, with the user's answers of that day folded in, as each section
says; **As built**, at the end, says where the build differs and what it settled.

#709 asks for two things:
- an FX column's lane (`:lane 4`) should have a length a macro can turn;
- it should have a clock division of its own, as Kria gives each of its parameters.

The `Kria` patch needs the first. Its length loop comes with a lane of 4, and today no macro
reaches it (decision 318). #591's comment asked for the second.

## 1. What's there

- **The phrase owns its lanes.** `Phrase::lanes` is `[Option<Lane>; FX_SLOTS]`, and
  `Lane { start, length }` is a run of the phrase's stored rows that the FX column loops on by
  itself. `:lane n` sets one from the cursor's row, `:len` on a lane changes its length, and
  `:lane` alone takes it away. Rows of the column outside the lane are kept, dim and silent.
- **The engine finds the row from the track's step count.** A lane reads
  `lane.step_at(count + lane_offsets[slot])`. `count` is the track's steps since its chain
  started, so a lane doesn't start again when the chain's next phrase does. When an edit changes a
  lane, `versions.rs` moves `lane_offsets` so the column keeps the row it was going to read, if
  the new lane has it. The same sum appears in four places in `steps.rs`:
  - `chosen_ahead`;
  - `fire_track`'s FX;
  - a lane's pass for `CND`;
  - `passes`, for the generators.

  `services.rs` repeats it for the playheads from the report's `lane_offsets`.
- **The phrase view shows a lane in storage order, not in time.** Each lane has its own playhead,
  in the gap before its column. The lane's rows are lit, and the rest of the column is dim. The
  title lists each lane's length as `FX1:4`.
- **A generator can fill a lane.** With `Span::Lane`, its rows are the lane's rows, and its passes
  count the lane's loops (decision 288). A patch's generator on a lane carries the lane's length
  (decision 318).
- **A row's FX run on its first pulse.** A row of several pulses (decision 122) runs its FX once,
  as it starts. Only on a `Multi` gate do its timing commands (`DEL`, `KIL`, `OFF`, `RET`, `ACU`,
  `ARP`, `RND`) run again on each pulse. A lock holds because it's a lock, not because it's read
  again.

## 2. Who owns what (the user's answer: relative)

**The phrase keeps the lane's own length and rate. The track gets two settings for each FX column
that move every phrase's lane on that column relative to its own.** Setting them by hand is the
phrase's job, as it is today. Turning them is the track's, where `M`, locks, modulation and the
Roto already work.

Take phrase A with a 4-row lane on FX1 and phrase B with a 7-row one:

| | A hears | B hears |
|---|---|---|
| Nothing holds it | 4 | 7 |
| `FX1 Length` at `+2` | 6 | 9 |
| `FX1 Length` at `-3` | 1 | 4 |

The other answer was an absolute hold, as `MacroTarget::Gen` holds a generator's setting
(decision 294). It would set both lanes to the same number while it held them. It was turned down
because it loses the differences between phrases, and because it has no "as written" point to sit
at. A relative setting is at rest at `+0`, so a centred macro or an LFO round 0 leaves every
phrase as written.

**A phrase with no lane on that column** isn't touched. The settings make no lane where there
isn't one.

### The settings

Both are a new owner, `Owner::Lane(slot)`, one per FX column. They're kept on the track as
`Track::lanes: [LaneMove; FX_SLOTS]` and saved only where they differ from the default, so the
song format doesn't change. As registry settings they get, with nothing new:
- step locks, holds and glides;
- a mod slot's `Dest`;
- macros, `K`, the step text, the Roto;
- patches and presets.

- **`Length`**, key `lane1.length`. It's a centred byte: `80` is `+0`, and one step is one row,
  so `00` is `-128` and `FF` is `+127`. It reads as `+2` and `-3`, drawn centred as `Pitch` is.
  What a lane hears is its own length plus this, kept from 1 to `MAX_PHRASE_STEPS`.
- **`Rate`**, key `lane1.rate`. It's a list centred on `×1`:

  `÷16` … `÷2`, `×1`, `×2` … `×16`

  It multiplies a lane's own division, rounded to the nearest whole step, and kept from 1 to 16.
  (Decision 349 turned it round: `÷2` now doubles the division, slower, and `×2` halves it,
  faster, as the As built note says.) A multiplier rather than a ± because one more step on a
  division of 1 doubles the lane, while on a division of 8 it hardly changes anything (the user's
  answer). As a list, `=` and `-` move one choice and `+` and `_` sixteen, so `+` goes to the end
  (decision 345).

The labels for a step lock are left to the build. Something like `LLN1` and `LRT1` would do. The
label tests check that none clash, and that only the mod slots' end in a digit; an FX slot's begin
with one. The build either makes room in that rule or picks labels that keep it.

## 3. A lane's rate: its own division (the user's answer: solve it now)

- **The unit is track steps:** a lane moves one row every 1 to 16 of the track's steps, as Kria's
  divisions do. It stays on the step grid, where FX are read, and follows the track's `Speed` and
  a `RATE` lock with nothing new. A note division like `Speed`'s was considered. It would put
  lane rows between the steps, where nothing reads FX.
- **The phrase owns it:** `Lane` gains `division`, saved only when it isn't 1. `:lane 4/2` gives a
  lane of 4 rows, each lasting two steps. `:lane /3` changes only the division of the lane under
  the cursor. The title shows `FX1:4/2`, and `FX1:4` at a division of 1, as today.
- **A slow row is read like a row of several pulses.** Its FX run on its first step and not again,
  as a row's do on its first pulse (§1), so a `RET` or a `CHA` acts once. A lock holds through the
  rest of the row because it's a lock. There's no `Multi` exception, as a lane has no gate.
- **The phrase view adds no lines.** Lanes are already shown in storage order, beside the note
  rows they don't line up with. A slow lane's playhead stays on its row for longer. The issue had
  assumed a slow lane's rows would take more lines, as decision 122 lays out a phrase's rows. That
  would need lines in the note grid that belong to the lane, and it's dropped.

## 4. What a stretched lane plays (the user's answer: it copies)

**A lane heard longer than its own length plays its own rows again, from the first.** Nothing is
written, so letting go gives the lane back as it was. Shrinking is the same under any rule: the
first rows play and the rest wait. Take a 4-row lane, rows `a b c d`:

| Heard | Plays |
|---|---|
| 3 (`-1`) | `a b c` |
| 4 (`+0`) | `a b c d` |
| 6 (`+2`) | `a b c d a b` |
| 8 (`+4`) | `a b c d a b c d`, an exact double |

This is the copy you get when you lengthen an Elektron pattern, which copies its pages into the
new ones, or with Ableton's Duplicate Loop, but done as it plays rather than written in. An odd
stretch gives a polymeter variation of the same material rather than new material. The row a
lane reads is
`start + (row mod own length)`, where `row` is the lane's place in the length it's heard at.

**A generator on the lane copies too.** A Euclid 3-of-4 heard at 8 plays 3-of-4 twice. It doesn't
render again as 3-of-8. Nothing renders again, the generator's span stays the lane's own rows,
and one rule covers a hand-written lane and a generated one. A generator's own settings are how
you change what it draws.

**Two other options were turned down:**
- **Playing blanks** only adds silence when you stretch.
- **Revealing the stored rows** past the lane plays the column's dim rows, which may hold
  anything. That's Kria's way, as its 16 steps are always there. With the copy rule, those rows
  stay storage, and `:lane` by hand is still how you take them in.

## 5. Turning it while it plays: it carries on

Today a lane's row comes straight from the track's count, `(count + offset) mod length`. Moving the
length from 4 to 5 at count 37 would jump the lane from row 1 to row 2, so a slow turn would
scramble it rather than stretch it. Decision 301 had the same problem with an LFO's `Rate` and
fixed it with a lap. A lane does the same.

- **While playing, a lane counts along.** Each lane keeps the row it's on and how many steps of
  that row are left. On each of the track's steps it reads its row, and when the row is done it
  moves to the next. Past the length it's heard at, it goes to the first row. A turn takes effect
  at the next row:
  - a shorter length: a lane past the new end finishes its row and goes to the first;
  - a longer one: it carries on into the copies;
  - a new rate: it applies from the next row.
- **On play, seek or reset, it works the place out again**, as decision 301 does:
  `row = (count + offset) / division mod length`, at the length and rate heard there. Playing from
  a step hears what playing from the top would, as long as nothing was turned on the way.
- **A new phrase in the chain** works its lane's place out again from the count, exactly as it
  does today.
- **An edit to a lane** keeps the row it was reading, as `versions.rs` does today, now counted in
  the lane's rows.

**Kria's other way** waits until the lane comes round to its start before a change takes effect.
That's Kria's quantise setting for division changes. It never lands mid-loop, but it makes a fast
LFO stepped and a long lane slow to answer. It's left out. A per-lane switch could add it later.

**Elektron:** its per-track lengths are positional, and `M.LEN` starts every track again from
step 1. I couldn't confirm what its playhead does when you shorten a track mid-play.

**A song nothing holds plays as it did.** At `+0`, `×1` and a division of 1, counting along from
a place worked out from the count gives `(count + offset) mod length` on every step. The golden
renders have to come out unchanged, and a test plays a long chain of lanes both ways.

## 6. The engine

- **`TrackState`** gets a `LaneRun` for each FX column: the row, the steps left in it, and how
  many times the lane has come round, for the passes.
- **Every lane sum goes through one function.** `TrackState::lane_row(slot, count)` gives the row
  a lane reads at a count. That covers `chosen_ahead`'s look ahead, `fire_track`, `CND`'s pass and
  `passes`. Generator passes count the lane's laps, worked out again on a seek as in §5.
- **The held values** come in through `values.rs` like any owner's: what a lock, a macro and
  modulation give, as heard. A lane reads them as each row starts.
- **The report** carries each lane's row as it's read, in place of `lane_offsets`. With a held
  length, the TUI can no longer work the row out from the count. `services::lane_playheads` reads
  the row from the report.
- **Real time:** the run is fixed arrays, so nothing is allocated. Each step adds a few integer
  operations per lane. The allocation tests turn both settings every block.

## 7. Where things are shown and turned (the user's answer: the phrase view and the Gen page)

- **The phrase view** keeps what's the phrase's:
  - `:lane` and `:len`;
  - the title, now `FX1:4/2`, and while something moves it `FX1:6/4 (4/2)`, what's heard first
    and its own after, dim;
  - the lane's lit rows.

  The playhead sits on the stored row being read, so on a copy it's on the row being copied. Rows
  past a shortened length go dim, as the rest of the column is.
- **The Gen page** holds what's the track's. Each FX column with a lane in the phrase under the
  cursor shows a `Lane` device with `Length` and `Rate`, under that column's generator device if
  it has one, as `Loops` sits under `Acid`. `M`, `gd`, the macro mark, `K` and `x` all work on it
  as on any setting. The build checks it fits the page at 163 by 42.

  A column without a lane shows no device, and the settings keep their values. A macro mapped
  there does nothing until a phrase gives that column a lane.
- **The Roto:** the device is one more on the Gen page's list, `Length` and `Rate` its two knobs.
  It follows the rules for the Gen page's devices (decision 161).

## 8. Patches, the tutorial and the manual

- **`Kria` gets its length knob.** The patch gains a `Length loop` macro turning `FX1 Length`
  from `-3` to `+12` (a lane of 1 to 16), which is what #620 lacked. Its lane of 4 is still
  carried, as decision 318 has it. The builder in `tutorial_songs.rs` writes it again with the
  songs.
- **Patches and presets** carry the two settings as they carry any of the track's. A patch saves
  them when a macro maps to them.
- **The guides:**
  - `track-gen.md` gets the `Lane` device;
  - the phrase guide gets `:lane 4/2`, the copy rule and the slow row;
  - the generators tutorial's Kria section gets the `Length loop` knob, in place of "a lane's
    length isn't a setting, so no macro turns it";
  - `generators-page.md` §5 is updated to match.
- **The manual's** screenshots of the Gen page and the phrase view's title are drawn again where
  they show a lane.

## 9. Tests, for the build

- **Engine:**
  - the golden renders are unchanged;
  - a lane at `+0 ×1` reads the same rows as today over 4,096 steps, a chain of phrases and an
    edit;
  - `+2` on a 4-row lane plays `a b c d a b`;
  - turning the length mid-play carries on with no jump;
  - a lane at `/3` runs a `RET` once in three steps and holds a lock through all three;
  - a seek works the place out at the heard length and rate;
  - nothing is allocated while turning.
- **Model:** `:lane 4/2` saves and loads, and a division of 1 and a `+0` aren't saved.
- **TUI:**
  - the Gen page shows `Lane` only for a column with a lane;
  - `M` maps `FX1 Length`, and the mark and `gd` work;
  - the phrase view's title shows what's heard and its own;
  - the playhead sits on the copied row;
  - the `Kria` patch's `Length loop` turns its lane.
- **Screenshots** of the Gen page and the phrase view, where a lane shows.

## 10. Left open, for the build to settle

- The step-lock labels, within the label rules (§2).
- Whether `Length`'s range should be narrower than ±127, if ±64 suits the hex better.
- Kria's wait-for-the-loop way (§5), as a later switch if the immediate way feels wrong on hardware.
- Writing a held length into the phrase, as `Space g a` keeps a generated pass. It's not part of
  #709.

## As built

What the build did where this study left a choice, or where it found the study's way wouldn't
hold:

- **One read a step, carried with it.** §6 had every lane sum call `TrackState::lane_row(slot,
  count)`. A track works its steps out ahead of hearing them, up to 28 with a groove that lands
  steps early, and a nudged row's part lands after the next step is worked out, so a run moved on
  at each call would be read out of order. `Engine::work_out` reads every lane once, through
  `lane_row`, before the step's choices are made, and what it read rides with the step
  (`PendingStep::lanes`, `Worked::lanes`): `fire_track`'s FX, `CND`'s pass, the generators'
  passes and `chosen_ahead` all read that. The run is kept for the step it waits for, so an edit
  can see where it was going.
- **A slow row's locks are put again.** §3 had its FX run on its first step only and a lock hold
  "because it's a lock"; but a lock ends at the next note, so over a note on every step a lane's
  lock at `/4` would have lasted one step of four. Its later steps put a lock or a hold of it
  again, so it holds through the row, while every event and a glide still act once. A `GRV`,
  `RATE` or `QNT` in a slow row is chosen on each of its steps the same way.
- **The labels** are `LLEN1` and `LRAT1`, `2` and `3` for the others, the label rule making room
  for a lane's to end in its column's number, as a mod slot's do.
- **`Length`'s range** is the whole byte, `-128` to `+127`, a row a step: `Pitch`'s scale, so a
  value reads as rows from `80` and none does nothing; ±64 would have left half the byte idle or
  made a step half a row.
- **`Rate` divides the lane's speed, not its division** (decision 349, amending 347, which built
  §2's multiplier): `÷2` is the slower, each row twice its own steps, and `×2` the faster, half
  its own steps, rounded, so `×` does nothing to a division of 1. That's a clock divider's `÷2`,
  Kria's slower higher division, an Elektron track's faster `2×`, and `:lane 4/2`'s slower
  `/2`. Up the list is faster, as the LFO's and Steps' `Rate` are (decision 301). The status line
  says so in words, and that a row a step is as fast as a lane goes.
- **The Gen page's device** is named for its column, `FX1 Lane`, and says how this phrase's lane is
  heard at the end of its title, `· here 6 rows, its own 4 rows`. It turns with the track page's
  keys, not the Gen page's reshaping ones. Making a column live put the cursor a device short of
  the new one since the macros' band came first; it now finds the device.
- **A pause keeps a lane's place,** so playing on carries on from its row; a lane is placed
  again whenever its track's chain starts again, which play, a seek and a reset all do.
- **Drawing without playing** (`App::set_playhead`, the screenshots and tests) places each lane
  from the count at the track's settings, as the engine does from the top.
- **An edit with a lane moved** keeps the stored row it was going to read, and its place in a row
  of several steps; one whose lane is as it was carries on exactly, in a copy too. Its offset for
  later placing is worked out at the length and division heard.
- **A glide in a lane** takes its rows' steps as heard; it still looks for the next value among the
  lane's own rows, not its copies.
- **The Roto** sends `÷` as `/`, as its font has neither.
