<!-- ANCHOR: before-gen -->
# The Gen page, macros and presets

The fourth page, after FX, shows the live columns of the phrase under the cursor,
the one `gl` reopens, as devices you turn as you would a filter. Its title names
the phrase, `Phrase 00`, and adds `▶ 02`, dim, while the track is playing another.
A fixed track has no phrases, so its page has only the macros. Its first band is the
track's eight macros (see **Macros**), then the live columns, then `Phrase`.

Each live column is a device, under the macros, in the phrase view's order: Note, Velocity, Nudge,
Length, then the FX columns. Its title is its fill and its `Where`, as `K` chooses
them, `Acid`'s `Pitches` after them and a `Follow`'s source in place of the
`Where`, a `Variation`'s and a `Ring`'s fill alone, as each makes its rows with
its notes, and a `Divisions`' channel and bar, as it makes its own rhythm;
then `Vary` on a fill that draws, and `Mutate` after it while it varies, then where it acts, dim: `Ramp  Euclid ·
Note 00–0F`, or `FX1 CUT lane 0C` for a lane. A `Where` that's its column's own is
dim too, and so are an `Over` that's its column's own and a `Vary` of `Once`. Its settings
are the shape panel's, laid out by what they do: the fill's down the first two columns,
and the `Where`'s in the third, so they stay put as the fill changes: a counted one's `Over`
at the top, then `Hits` and `Rotate`, `Rule` and `Start`, or an amount, such as `Beats`'
`Density`; `Complementary`'s amount and `Side`; or a named rhythm's `Rhythm`, `Part` and
`Child`, whose `Over` goes to the fourth. Where `Random`'s `Weights` takes the top of the
third, what was there goes to the fourth. A `Follow`, which has no `Where`, has its
`Mutation` and the kinds it may make in the third column and the fourth, and `Acid`,
whose three are full, its riff's `Spread` and `Bias` in the fourth, as a `Tonnetz` has its
moves' weights, `P`, `L` and `R`. An `Acid`'s loops
(see **Acid lines** in the editing guide) are a device of their own under it, `Loops`:
`Trig loop` and `Note loop` in its first column, `Oct loop`, `Acc loop` and `Sld loop` in
its second, level with `Octave`, `Accent` and `Slide` above, and at the end of its
title, dim, when the loops come round together and when the phrase plays the same pass
again, `in step again every 420 rows, the phrase every 105 passes`. A `Random` on notes has its
weights on a device of their own under it too, `Weights`: a cell for each degree of the column's
scale, in rows of four from the root, each named by its note, `C`, `D#`, and at the end of its
title, dim, how many of them `Steps` lets through, `Steps lets 5 of 5 through`. They're the shape
panel's `Weights` digits, so a turn of a cell is that digit, and each takes a macro, a `MAC` lock
or an LFO on its macro as any setting here does, so a weight can move while it plays. With the
cursor on a weight, the generator's preview marks the rows its degree writes with dotted lines
down the picture, and while the phrase plays, the weight of the note playing is named in green,
the playhead's colour, on the pass the phrase is on. The Roto has
the first eight on one page and the last four on a second. A value with a bar is brief, as `9` for hits,
as its bar shows how many of how many; the status line says it in full, `Hits, 9
of the 16 rows`.

**Phrase.** The device after the live columns and lanes, before `Empty`, holds the track's settings that move
the walk of its phrases' rows (see **Order** in the editing guide). They're the
track's, so they move every phrase it plays, and each phrase keeps its own
`:order`. `Order` is places on through the list of orders from the phrase's own,
`+0` as written, centred as `Pitch` is: from `forward`, `+1` is `reverse` and
`+2` `up-down`, and it stops at either end of the list, as the Metropolix's
order CV does. `Reverse` turns the walk the other way up, so `forward` plays
from the last row and `up-down` becomes `down-up`. `Order X` and `Order Y` are
the order's two numbers, `Own` for the phrase's: a jump's rows, an arp's notes
and step, the snake by its number, and Cartesian's X and Y rates. At the end of
its title, dim, it says the order this phrase plays in: `· here Hopscotch, its
own Jump 3`, or `· here Jump 3, as written`. `=` `-` `+` `_`, `i`, `K`, `x`, `M`
and `gd` work on them as on any of the track's settings, a step's `ORD`, `REV`,
`ORDX` and `ORDY` lock them, and a mod slot or an LFO can go to them. Under them,
`Length` and `Start` are the window of rows the order walks: `Length` rows up or
down from the loop's, `+0` as written, its own rows again past its last, and
`Start` the row it starts on, rows on from the first, going round; the title adds
it, `· here Forward, 4 rows from 3, its own Forward`. A pass keeps the loop's
steps, so the chain keeps time; a step's `PLEN` and `PSTA` lock them. Beside
them, `Cue` is a switch, `Off` as it starts: on, a turn of `Length` or `Start`
waits for the pass to end, and a turn of a lane's `Length`, `Rate` or `Start`
for the lane to come round to its first row, as Kria's division cueing does.
The order's four still move at once. `CUE` on a step locks it. At the foot,
`Chain` and the `Jump` beside it, `Chain jump` in lists, walk the chain's
phrases as the rest walk a phrase's rows: `Chain` is an order from the same
list, `Forward` as written, and `Jump` the chance each of the chain's rows plays any of its phrases instead. They
change at the chain's next row, not mid-phrase, and `CORD` and `CJMP` on a step
lock them (see **Chain order** in **Phrases and chains**). The Roto has the
first seven on knobs 1 to 7, and the chain's two on a second page.

**A lane's Length, Rate, Start, Jump, Hold, Order and Reverse.** Each FX column with a lane in the phrase has a
device of its own, `FX1 Lane`, under that column's generator if it has one, as
`Loops` sits under `Acid`, and where its column comes if not. Its settings are
the track's, not the phrase's: they move every phrase's lane on that column from
its own (see **Lanes** in the editing guide). `Length` is rows up or down, `+0` as
written, centred as `Pitch` is; at `+2` a lane of 4 plays 6 rows, its own again
from the first, and at `-3` 1 row. `Rate` is a list from `÷16` to `×16`, `×1` as
written, slower below it and faster above, as a clock divider's: at `÷2` each of
its rows lasts twice its own steps, and `×2` halves a division of 8 to 4; it's
rounded to a step, from 1 to 16, so a lane of a step a row goes no faster. `Start`
is the row each lane starts on, so many of its own rows on, going round them.
`Jump` is the chance, `00` to `FF`, that each of its rows plays any of the rows
`Length` and `Start` give instead of the next, as the Play device's `Jump` is for
the phrase's window:
the lane counts on from where it was, and a slow row jumps once for all its
steps. `Hold`, under `Length`, is the chance, `00` to `FF`, that each of its rows
plays the row the lane played before again instead of its own, so the value
holds, as a failed roll does in Kria and on the Metropolix: a jumped row kept is
kept as it jumped, and a lane's first row, as it starts or as playing starts
from a step, is always its own. It rolls apart from `Jump`, so turning it leaves
the jumps where they were. After an `SED`, its jumps and holds come out the same
each time that row plays (see **Reseeding** in the FX guide). `Order`, beside
`Hold`, walks the lane's rows in an order from the whole list, as the chain's
`Order` does, `Forward` as written: it walks the rows the lane is heard for, from
its `Start`, and the lap stays the lane's length, so `Up-down` on a lane of 4
plays `1 2 3 4 | 3 2 1 2 | 3 4 3 2`, drifting across its laps, and a dealt order,
`Shuffle`, `Random`, `Quasi-random` or `Brownian`, is dealt again each lap from
the seed, `Brownian` from the lane's first row. `Reverse`, beside it, turns the
walk the other way up, whatever the order, as the `Phrase` device's `Reverse`
does. `Jump` and `Hold` act on top of the order. After an `SED`, a dealt order
walks the same each time that row plays. `Cue` never holds any of them back, as
they're how the loop is walked, not its bounds. At the end of its title, dim, it says how this phrase's lane
is heard against its own: `· here 6 rows, its own 4 rows`. `=` `-` `+` `_`, `i`,
`K` on `Rate`, `x`, `M` and `gd` work on them as on any of the track's settings,
a step's `LLEN1`, `LRAT1`, `LSTA1`, `LJMP1`, `LHLD1`, `LORD1` and `LREV1` lock them, and a mod slot can go to them, full depth moving `Rate`, `Order` and `Reverse` across their lists. A column
without a lane has no device, and the settings keep their values: a macro mapped
there does nothing until a phrase gives that column a lane.

On its right is a picture of what it writes across its span: notes as short
lines at their pitch, from the lowest to the highest there, a small piano roll;
velocities and lengths as bars, and nudges up or down from the middle; and an FX
column as a line through its values. While the phrase plays, a dot rides it on
the row playing, or on a lane, where the lane is reading. It's drawn from the
phrase as it is, so it changes as you turn a setting or edit a row.

What a fill's settings do together, and recipes for the ones with the most, are in
**Generators in depth**.
<!-- ANCHOR_END: before-gen -->
<!-- ANCHOR: before-panel -->

| Keys | Does |
|---|---|
| `=` `-` `+` `_` | Turn it, writing the column again, one undo a press |
| `i` | Type a value: a note, a number, or hex on an FX column |
| `K` | List a setting's choices; switch one that's off or on |
| `x` | Back to where it starts |
| `r` | Roll the generator again, from anywhere in it, as in the panel |
| `Space g a` | Apply it, as plain values; with `Vary` on, keep the pass heard |
| `dd` | To nothing; on `Fill`, take it away, putting back what was under it |
| `Enter` | Open the shape panel on it, in the phrase view, as `gl` does |

**A new variation each pass.** A fill that draws from its seed, `Random`, `Walk`,
`1/f`, `Acid`, `Shuffle`, `Grids` (its `Chaos`), `Divisions` (its dice), `Variation`, a
`Wave` of `Random` or `Smooth`,
a `Follow` that mutates, a `Tonnetz` whose moves are `Random` or a `Loop`, or any fill on
rows chosen by `Chance`, one of Marbles' rhythms or a named `Rhythm`, has `Vary`: `Once`, as it's always been, the
same every time round; `Each pass`, a new roll each time the phrase comes round; or
`Every 2 passes`, 4 or 8, so each variation plays that many times before the next,
the way an Elektron's `1:4` condition keeps to the bars. Each pass's seed comes from
the generator's own and which pass it is, so the first pass plays what's written, a
render plays the same again, and playing from bar 33 hears what playing to it would.
With `Acid`'s `Pitches` on `Keep`, it varies only the accents, slides and octaves,
as `r` would. The phrase shows what's written, the first pass; what plays after it
is the engine's.

**A generation each pass.** Rows chosen by `Cells` have `Vary` on any fill, and
it steps the automaton on rather than rolling a new seed: with `Each pass`, the
first pass plays the row it starts from and each pass after it the next
generation, or across tracks the next rows of generations, carrying on from the
last; `Every 2 passes`, 4 or 8 holds each for that many. The fill's own values
keep their seed. Each pass's generation is worked out from the row it starts
from and which pass it is, so playing from bar 33 hears what playing to it
would, and it starts again from that row after 256 passes on one column, and
after 4096 generations across tracks, 256 passes of a 16-row phrase. `Space g a`
applies it as last heard, as plain values, as a generation isn't a seed to keep.

**Déjà vu.** While it varies, `Mutate`, after `Vary` in the title, is the share of its rows each
new variation draws again, after Marbles' Déjà vu and the Turing Machine's big knob: `100%`, as
it starts, draws every row again, as above; `0%` draws none, so the first pass heard plays on
every pass, locked; between, each new variation draws about that share of the rows again and
keeps the rest as the last pass played them, so the line drifts. Which rows it draws comes from
the generator's seed and which variation it is, so a render plays the same again, and a higher
`Mutate` draws every row a lower one does and more. It works on a `Clusters`, `Markov` or other
rhythm's rows as on the values, so a rhythm can drift a hit at a time. What's kept is each row as
it was heard, so turning `Spread` on a locked line changes only the rows drawn again after.
Playing from anywhere starts from that pass's whole variation, as Steps' `Mutate` does, and
keeps rows from there; an edit while it plays keeps what was heard. A macro, a `MAC` lock and an
LFO on a macro reach it as any setting here, and on the Roto it's beside `Vary`, or on a page
of its own, `Mutate`, where the fill's are full. `Space g a` on a generator that mutates applies it as last
heard, as plain values, as rows kept from earlier passes aren't a seed to keep.

When you hear one you like, `Space g a` keeps it: the pass just heard becomes the
generator's seed and `Vary` goes back to `Once`, so that variation plays every pass
and the phrase holds it, still live to turn, as one undo step. Before it's been
heard varying, it says so. It's the Turing Machine's lock, as `Space g a` on a Steps
slot's `Mutate` is.

A `Ring` has no `Vary`: its register runs on from pass to pass by itself, each pass
where the last left it, worked out from its seed and the pass so playing from bar 33
hears what playing to it would. `Space g a` on one keeps the register as the pass
just heard began, so that pass plays first and the ring runs on from there,
`Change` where it was; turn `Change` to `0%` and that loop holds, every pass. `r`
lets go of it and starts a new ring.

An `Acid` with a loop on runs on from pass to pass the same way, each loop where the
last pass left it, with or without `Vary`. `Space g a` on one keeps the pass just
heard as the one written, and its loops go on from there; `r` starts them from
their first rows again.

**Making a column live.** The last device, `Empty`, has `K` list the columns of
the phrase with nothing live on them. Choosing one makes it live over the phrase's
loop, or its lane, with the ramp `S` starts with, as `vG` `S` `L` would in the
phrase view, and puts the cursor on it. An empty FX column lists the commands
first. Notes generated on an empty column own their velocities, so a live
Velocity column over them applies them, as a hand edit would; their nudges and
lengths are free.

## Macros

Each track has eight macros, after Ableton Live's Rack macros: a macro is one knob that turns
any number of the track's settings at once, each between a `Min` and a `Max` of its own. It's
absolute, so at `00` every setting it holds is at its `Min` and at `FF` at its `Max`, and a `Min`
above its `Max` turns that setting the other way. A mapping can act over part of the turn alone,
its zone, so one macro can be a knob with a different job on each side of its middle. A setting
is on one macro at most.

The `Macros` band, first on the Gen page, has the eight in two rows of four, each with its name,
its value in four digits and a bar. `=` `-` `+` `_` turn one, `i` types it, `x` takes it to `00`,
and the status line says what it holds. A macro is named after the first setting it holds until
you name it.

| Keys | Does |
|---|---|
| `M` | On a setting, list the macros to map it to |
| `M1` to `M8` | Map it to that macro at once |
| `gm` | The mapping of this macro, or of this setting's |
| `gd` | Go to this setting's macro, or this macro's mapping |
| `:macro Name` | Name this macro, or this setting's, up to 12 letters |
| `:macro` | Go back to its first setting's name |

**The macro panel.** `Space t` shows the track's eight macros in a strip at the bottom of any
page, above the status line, so they're at hand while you work elsewhere, and hides it again. The
page gives up the strip's three rows while it shows. It follows the cursor's track as it moves,
and on the project page, which has no track of its own, it shows the track last on. `Space T`
puts the keys in it, showing it if it's hidden, on the macro they were last on:

| Keys | Does |
|---|---|
| `h` `l` | The macro before, or the next |
| `=` `-` `+` `_` | Turn it a step or a large step |
| `i` | Type it, in two hex digits |
| `x` | Take it to `00` |
| `u` `Ctrl-r` | Take the last change back, or do it again |
| `W` `B` | The next or previous track |
| `Tab` | Play, or pause |
| `Esc` | Back to the page, its cursor where it was, the panel showing |
| `Space t` | Hide it, and back to the page |

A turn in the panel is the band's own, one undo step a press, and the wheel turns it too, a turn
of it one step, as on any setting. The status line says what the macro holds, as in the band, and
a macro holding nothing is dim. Whether the panel shows is kept until you quit, and isn't saved
in the song.
<!-- ANCHOR_END: before-panel -->
<!-- ANCHOR: before-mapping -->

**Mapping.** `M` on a setting lists the eight with what each holds; `Enter` or the macro's digit
maps it, from the setting's bottom to its top. The first setting on a macro holding nothing turns
the macro to where the setting is, so nothing jumps. One on a macro holding others jumps to where
that macro puts it, and the status line says so. `M` on a setting a macro holds opens the list on
its macro: choosing that one takes it off, back to its own value, and choosing another moves it,
range and all. Each is one undo step.

**A setting a macro holds** shows where the macro puts it, with the range the macro can take it
over drawn faint behind its bar, `▕██▎░░░░  ▏`, and the status line says which macro and how far,
as `Cut, 1.05 kHz: on macro 1 Bite, 170 Hz to 9.56 kHz`. One with no bar, such as a choice, a Mod
page value or a device's title, has a faint dot after its value instead, `1/16 •`, after the dot
Live puts on a control it automates. Turning it is refused, as Live greys a mapped control out:
turn its macro, or open its range with `gm`. `gd` on it goes to its macro in the Gen page's
`Macros` band, ready to turn, and `Ctrl-o` comes back to it; `gm` there opens its mapping. So does
`gd` on a fader on the mixer a macro holds, and `Ctrl-o` comes back to the mixer.

**The mapping view**, `gm`, lists each setting the macro holds with its `Min`, its `Max`, the
`Curve` and `S` of its response and its `Zone`, with a drawing of the response of the one under
the cursor beside them:

| Keys | Does |
|---|---|
| `j` `k` | The next or previous setting |
| `h` `l` | Across its `Min`, `Max`, `Curve`, `S` and the two ends of its `Zone` |
| `=` `-` `+` `_` | Move an end a step or `10`, a bend `5` or `25` |
| `i` | Type it: an end in two hex digits, a bend `-100` to `100` and `Enter` |
| `x` | Put `Curve` or `S` back to straight, `0`, or `Zone` to `00–FF` |
| `~` | Swap `Min` and `Max`, so the setting goes the other way |
| `dd` | Take the setting off the macro |
| `u` | Take the last change back |
| `Space d` | Say what's under the cursor, or stop |
| `Esc` | Close it |

**Each setting its own way.** A setting's `Min` and `Max` are its own, so one macro can turn one
setting up while it turns another down: map both, then `~` on the one to go the other way. One
macro then opens the cutoff while it takes a mod envelope's depth down, say.

**Each setting its own curve.** `Curve` and `S` bend a mapping's response as the shape panel's
`Curve` and `S` bend a ramp, with the same numbers. `Curve` above `0` moves the setting early,
most of the way toward its `Max` in the macro's first part, and below `0` late. `S` then eases
both ends, above `0` an S, slow at `Min` and `Max` and quick between, and below `0` the reverse.
The ends stay `Min` and `Max` however it's bent, and a setting turned the other way bends the
same way along the turn, so `Curve +60` reaches a lower `Max` early too. Both start at `0`, a
straight line, and are shown dim while they are. The drawing has the macro's turn across, the
setting up, the straight line dotted behind the curve and a faint line where the macro is turned;
the range on a mapped setting's bar is still its `Min` to its `Max`. A sound preset or a patch
keeps each mapping's curve.

**Each setting its own part of the turn.** A mapping's `Zone` is the part of the macro's turn it
acts over, from a start to an end on the macro's `00` to `FF`, the whole turn, `00–FF`, to start
with, shown dim while it is. Its ends move as a range's do, `=` `-` a step and `+` `_` `10`, and
`i` types one in two hex digits. Across the zone the setting goes from its `Min` at the zone's
start to its `Max` at its end, along its `Curve` and `S`; below the zone it stays at its `Min`, and
above it at its `Max`. A zone always starts below its end: to go the other way, swap `Min` and
`Max` with `~`. The drawing has the setting flat outside the zone, and the status line and the line
`Space d` shows say the zone, as `Res 0 % to 63 % over 80–FF`. Moving a zone moves what's heard at
once, as a held setting outside it goes to its end. Locks and modulation of the macro reach each
setting through its zone, and a sound preset or a patch keeps it.

**Recipe: a bipolar knob.** Marbles' Déjà vu is one knob with two jobs: from the left to the
middle it takes the random sequence from new each time to looped, and from the middle to the
right it loops it with jumps. Do the same with a Steps slot and a macro:

1. `M1` on Steps' `Mutate`, then on its `Jump`, in the slot's title.
2. `gm`. On `Mutate`, `~` so it comes down, from its top at `00` to `0`, then `lllll` to
   the zone's end and `i7F`: `Zone 00–7F`.
3. `j` to `Jump`, on its zone's end, `h` to its start and `i80`: `Zone 80–FF`.

Now at `00` `Mutate` is at its top and `Jump` at `0%`; at the middle, `7F` to `80`, both are
`0%`, the loop held; and toward `FF` the loop's steps play out of order, more often as it turns,
every step at `FF`. The shipped patch `Deja vu` is this knob. The same goes for any crossfade
between two things: a dry-to-wet on one side and a drive on the other, two sends, or two
filters.
<!-- ANCHOR_END: before-mapping -->
<!-- ANCHOR: before-presets -->

**What a macro reaches:** the track's own settings, its machine's, its effects', its mod slots'
depths, phases and deforms, and their types' own: an envelope's `Attack`, `Decay`, `Sustain` and
`Release`, a drum's `Body`, a gate's `Colour`, Steps' `Length`, `Mutate`, `Jump`, `Spread`,
`Bias` and `Smooth`, and an LFO's and Steps' `Rate`, so a Turing Machine is a Steps slot with
`Mutate`, `Length` and `Depth` on three macros. It reaches the accumulator's `Up` and `Down` and the arpeggio's `Rate` and
`Octaves` too, but never another macro.

**A live generator's settings,** `M` on one on the Gen page, as `Hits`: the macro holds that
setting of the generator on that column in every phrase of the track, between numbers of its
own, its bounds to start with. Turning the macro writes each of those columns again, one undo
step, and the generator keeps its own setting underneath, so taking it off the macro gives it
back. A `MAC` lock, a glide or an LFO on the macro reaches it too, as it plays: the engine renders
the generator again with the setting where the macro has it, from the row the lock is on, and the
phrase still holds what the macro's turn writes, so with nothing moving the macro, what's heard is
what's written. The phrase view shows what was heard of it, and `Space g a` keeps it. A `Wave` or a
`Rate` is the exception: a turn writes it in, but a lock or an LFO doesn't reach it, which the
mapping view says, dim, with `by hand`. The fill, the `Where` and a `Follow`'s source aren't for
macros, as they're what the generator is.

**Playing a macro.** A step's `MAC1` to `MAC8` locks one for its note, and with it everything it
holds, as `MAC3 80`. A mod slot's `Dest` lists the eight under `Macros`, so an LFO on a macro
sweeps every setting it holds, each across its own range. A step's lock of a setting the macro
holds still wins on its row, and modulation of that setting goes on top.

**Group macros.** A group's track has eight macros too, and they reach the settings of the tracks
in the group as well as the group's own: one knob over a whole kit. `M` on a setting of a track in
a group lists the group's eight after the track's own, as `G1 1 Kit`; `Enter` on one maps it
there. The mapping keeps the track by its place in the group, first, second and so on, so a group's
patch reaches the same places on any group; a track moved within the group or out of it leaves
its place to the track now there.
The group's mapping view (`gm` on the group's macro) names each setting after its track, as
`Snare Cut`. On the track, the setting refuses a turn of its own, the status line says
`on G1's macro 1`, and `gd` goes to that macro on the group's track. A setting both on one of the
track's own macros and on the group's is heard where the group puts it; `M` moves it from one to
the other. Generators' settings are reached too, each track's in its own phrases, so `Hits` on a
kick, a snare and a hat can turn together from one macro.

A mod slot on the group's track sends to its macros as to any, so an LFO on a group macro moves
every setting it holds on every track, and a `MAC` lock on the group's own row reaches them from
that row. A track's macro panel (`Space t`) still shows its own eight; the group's are on the
group's track. Zones work as on any macro, so one group macro can crossfade between two tracks,
one turned down over its left half and the other turned up over its right, as an Octatrack's
crossfader does.

## Presets

A preset saves a track's sound, or one slot's effect, to load on any track of any song, as
Elektron's sounds, the M8's instrument files and Ableton's device presets do. Each is a small
file in the presets folder, `~/Music/vimtracker/presets` unless `settings.ron` says otherwise
(see **Kit and sample folders** in **Sounds, samples and kits**): sounds in `sounds`, effects
in `effects`, and folders below those for sorting, such as `sounds/bass`.

**A sound preset** is the whole device: the machine and its settings, the filter, `Pitch`,
`Glide`, `Gate`, `Velocity` and `Phase`, the four slots' effects, the six mod slots, the accumulator, the
arpeggio, and the eight macros with their names, mappings and ranges. It leaves out the track's
place in the song: its `Level`, `Trim`, `Pan`, sends, group, choke group, groove, `Speed`,
`Reset`, `Quantise`, mute, solo, name, and its phrases and chains. A sampler's sound keeps its
sample, with its slices and how it plays, and its file by its full path, so loading it in another
song puts the same file in that song's bank, or finds it there.

**An effect preset** is one slot's effect and its settings, a Distort with every mode's, and
loads into any slot.

| Command | Does |
|---|---|
| `:preset` | List the presets of what's here |
| `:preset save bass/Bite` | Save it under that name |
| `:preset! save bass/Bite` | Save it, writing over one |
| `:preset load bass/Bite` | Load that one |

What a preset is depends on where the cursor is: on the FX page on a slot, it's that slot's
effect, and anywhere else the cursor's track's sound. `K` on `Machine`, the Sound page's first
setting, and on an effect's name on the FX page, lists `Load a sound preset` or `Load an effect
preset` after the machines or effects, which opens the list on the presets folder, and `Save as a
sound preset` or `Save as an effect preset`, which types `:preset save ` for the name. A name is
letters, digits, spaces and `-` `_` `.` `(` `)`, with `/` before a folder; `Tab` completes one. A
name that's taken is refused, so nothing is written over by mistake.

**Loading** is one undo step. A sound keeps the track's level, pan, sends, name and phrases, and
the slots all start in; an effect replaces only the slot under the cursor. The list shows each
preset with what it is beside it, as `Acid · Distort Delay · 3 macros`, a sound by its machine's
icon, and the folder you last saved in or loaded from at its top.

**Tags** find presets as they find samples (see **Tags** in **Navigation**): every sound preset
is tagged `sound` and by its machine, as `synth`, `analog` and `acid`, and every effect preset
`effect` and by its effect, as `delay`, so `#acid` lists the acid sounds. `T` in the list gives
one tags of its own, such as `bass`, kept in its file so they go where it goes.
<!-- ANCHOR_END: before-presets -->
<!-- ANCHOR: rest -->

## Patches

A patch is what plays a sound without the sound: a track's macros and the parts they move, saved
to add to any other track while it keeps its own machine, filter and effects, as Live's MIDI
Effect Rack presets go in front of whatever instrument is there. A Steps slot with `Change`,
`Length` and `Range` on it is a Turing Machine you can put on an `Acid`, a `VA` or a kick. Patches
are kept in the presets folder's `patches`, beside `sounds` and `effects`. Each is tagged
`patch`, and `T` in the list gives it tags of its own, such as `pitch`.

**Shipped patches** come with vimtracker, made from the generators tutorial's songs, and are listed
under `Factory` after your own, so `:patch` has some from the start. They're built into the
program rather than written into your folder, so they can't be changed, tagged or written over;
a patch of yours with the same name comes first, and `:patch save` always saves one of yours.
Each is tagged `generative` and with what it acts on, so `#generative` lists them and `#rhythm`
finds `Euclid`, `Acid loops` and `Noise Ring`.

| Patch | From the song | Its macros |
|---|---|---|
| `Turing Machine` | `turing-machine` | `Change`, `Length`, `Range` |
| `Deja vu` | `deja-vu` | `Deja vu`, `Spread` |
| `Euclid` | `euclid`'s bass | `Hits`, `Rotate` |
| `Kria` | `kria` | `Pitch loop` to `Chance loop`, all five |
| `Acid loops` | `kria`'s bass | `Trig loop` to `Sld loop`, all five |
| `Accents` | `live-generators`' hats | `Hits`, `Rotate` |
| `Acid line` | `acid-lines` | `Density`, `Notes`, `Octave`, `Accent`, `Slide` |
| `Drunk walk` | `walks`' bass | `Step`, `Hits` |
| `Noise Ring` | `noise-ring`'s bell | `Change`, `Chance`, `Length` |
| `Marbles X` | `marbles`' tune | `Spread`, `Bias`, `Steps` |
| `Variations` | `variations` | `Order`, `Temperature` |

Each is the part of that song's track that plays it, with its macros turned to where the song
has them; the generators tutorial has a section on each. `Deja vu` holds Marbles' whole knob on
one macro: its left half turns `Mutate` down to a locked loop, and its right half turns `Jump`
up, the loop played out of order. `Kria`'s length, ratchet
and probability loops come with their lanes, of 4, 3 and 7 on FX 1 to 3, a row a step, as a
patch carries a lane's length but not its division.

**What's saved** goes by the macros, as they're the patch's knobs: each macro that holds
something, with its name, value, mappings and ranges; every mod slot a macro maps to, the slots
those go to in turn, and any slot going to one of them, as an LFO on a macro or on a slot's
`Depth`; the accumulator and the arpeggio if any of that reaches them; and the live generators of
the phrase under the cursor that a macro holds by its column, with their shapes and, for one on
a lane, the lane's length, and any slot or macro an FX one locks, as `DEP4`, so the lock has its
slot wherever the patch goes. Never the
machine, the filter, the output or the effects: a macro mapped to one of those keeps the mapping
by the setting's name, and on a track without that setting it does nothing. A track whose macros
hold nothing has no patch to save; a sound preset (`:preset save`) keeps the whole track instead.
A group's patch keeps its macros' mappings on its tracks by their places, and reaches the tracks
at those places in the group it's added to; added to a sound track, which has no tracks of its
own, those mappings are left out.
| Command | Does |
|---|---|
| `:patch` | List the patches, to add one to this track |
| `:patch!` | The same, over the last ones if too few are free |
| `:patch save Turing` | Save the track's patch under that name |
| `:patch! save Turing` | Save it, writing over one |
| `:patch load Turing` | Add that one |
| `:patch! load Turing` | Add it, over the last ones if too few are free |

**Adding** one is one undo step. Its mod slots go into the track's free ones, a free slot being
one going nowhere (`Dest` `---`) that nothing reaches, and its macros into free macros, ones
holding nothing, each in order, so a slot still reaches the slots after it. Every mapping and
`Dest` among them is renumbered to where they went: a Turing Machine saved from `Mod 4` and added
to a track whose first two slots shape its note and cutoff goes into `Mod 3`, and `Change` turns
`Mod 3`'s `Mutate`. A macro already holding something is never added to. A setting the patch's
macros hold comes off any of the track's own macros, as a setting is on one macro at most. Its
generators go onto the phrase under the cursor over its loop, as `S` would: an empty Note column
is written, notes there are reshaped, and a column already live takes the new shape. An FX
generator goes on the first FX column live with its command or holding it, else the first empty
one, and one that filled a lane gives an empty column a lane as long from the loop's first row,
as `Kria`'s lane of 4. One that locks a slot's or macro's setting by its number, as `DEP4` or `MAC2`, locks the
slot or macro where it went: a generator saved writing `DEP4` writes `DEP3` on a track where
that slot went into `Mod 3`. Locks already on the track's own rows stay as they are. The status line
says where everything went.

With too few free, nothing is added, and the status line says how many the patch needs and how
many the track has free. `!` puts it over the last ones that aren't free instead, saying which;
anything that reached a slot or macro it replaced lets go of it.

`K` on a mod slot's type, in the title of its device on the Mod page, lists `Add a patch` and
`Save as a patch` after the types, and `K` on a macro of the Gen page's band lists the two alone.
The patches' list shows what each needs beside its name, as `1 slot, 3 macros` or
`Note generator, 2 macros`, your own first and then `Factory`'s. `Tab` after `:patch load`
completes both.
<!-- ANCHOR_END: rest -->
