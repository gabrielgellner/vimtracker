# Adding a device: an effect, a voice, a generator or a mod type

A checklist for each family, written against the code as it is (#784). It lists every place a new
one takes, what the compiler says if you miss one, and which tests show the change. Why the code
is shaped this way is in [device-interface.md](device-interface.md); the
[Architecture](vimtracker.md#architecture) section describes it as built. Paths are under
`crates/`.

**Two rules for every family.**
- **Add it last, and never reorder.** The registry numbers a family's settings in its enum's
  order, and each table's rows follow it. Songs save a device by its serde name and its settings by
  key, so a new one moves later settings' places but loses nothing, as long as no key changes. A
  voice's place in the machine list is its `KINDS` group's, not its place in `ALL`.
- **Every key is for ever:** an effect's or a voice's `key`, each setting's `key`, each
  `GenParam` key and each Roto layout's `kind`. Pick them once.

**The pins.** Each list below ends with what shows the change. These are what they mean:
- **`stable_names`** (`vt-model/tests/all/stable_names.rs`) keeps every name a song stores in
  `vt-model/tests/golden/*.txt`. A new device adds lines. Write them with
  `VT_BLESS=1 cargo test -p vt-model --test all stable_names`, then read the diff: only new lines
  and, in `settings.txt`, later places moving. A key changed or gone is a bug.
- **The Roto's pins** (`vt-tui/tests/all/roto_pins.rs`) hold each layout's `kind`, hash and stored
  version. A new layout is one new row, which the failure prints. Every row already there stays
  as it is.
- **The engine's goldens** (`vt-engine/tests/golden/*.hash`): a new device gets one of its own,
  written with `VT_BLESS=1` (see vt-engine's `tests/all/exciter.rs`). Those already there stay
  unchanged. `tests/all/goldens.rs` renders every effect and voice that no other file pins, and its
  `every_effect_and_every_voice_has_a_golden` fails while one has none.
- **`just screenshots`** gives no diff unless a screenshot lists every device of the family.
- **`cargo test --workspace --release`**, as for any engine change, runs the real-time guard over
  every effect (`link.rs`'s `every_effect_never_allocates_in_the_callback`).

## An effect

1. **Its settings**, in `vt-model/src/insert/<family>.rs`, the file of the effects it's like
   (`colour`, `dynamics`, `filters`, `modulation`, `pitch` or `space`):
   - the struct, `#[serde(default)]`, with `DEFAULT` and `Default`;
   - `effect_settings!`, listing its fields in the order its `*_PARAMS` keeps them;
   - `*_PARAMS`, with `*_GRID` if it places its settings by meaning, or `*_SHOWN` if each type shows
     its own;
   - a new `Units` in `settings/units.rs`, only if none fits.

   `insert/tests.rs` checks the defaults, the order, the ranges and `MAX_PARAMS`.
2. **`insert.rs`**: one line at the end of `effects!`, `Variant(Settings)`. The macro writes the
   `Insert` variant, which is the save format, the `EffectKind` variant, both `ALL`s, `kind`, and
   `read` and `write`'s forwarding to its `effect_settings!`.
3. **`insert/kind.rs`**: its `EFFECTS` row (key, name, meaning, params, shown, grid), at the end
   too, and the import of its `*_PARAMS`. The compiler asks for the row, as `EFFECTS` has one for
   each `EffectKind`; `each_effect_kind_is_its_insert_and_its_row` checks they're in step.
4. **Its DSP**, in `vt-engine/src/dsp/effects/<name>.rs`, or `filters/` or `dynamics/` by what it
   does, with a `pub mod` line in the folder's `.rs`. Implement `SlotEffect`, with the model's
   struct as its `Settings`, and `MONO` and `MIXING` if it isn't `Stereo` and `Wet`. Implement
   `Rung` if the slot plays it through `ring`. Nothing may allocate but `new`.
5. **Where the slot keeps it:**
   - **With a long buffer:** one line of `memories!` in `dsp/effects/memory.rs`, under `rung:`, or
     under `played_by_slot:` with an arm of its own in `slot.rs`'s `effect` if it needs the note or
     another track.
   - **Small enough for every slot:** a `KEPT_IN_SLOT` row in `dsp/effects/slot.rs`, an
     `InsertState` field with its default, and its arms in `take`, `effect`, `without_memory` and
     `kept_as`. The compiler asks only for `without_memory`'s. A missed `effect` arm rings a buffer
     it hasn't, so its wet sound is silent, and its golden render is what shows it.

   `FACTS` won't build if the effect is in neither list or in both, or lacks the `Mix` or `Gain`
   its `MIXING` needs. `slot.rs`'s test `by_hand` matches every effect, so the tests won't build
   without its arm there, the levels and `Mono` the slot should find for it. vt-engine's
   `tests/all/mono.rs` checks a `MONO` other than `Stereo`.
6. **The Roto**, in `vt-tui/src/roto/layouts.rs`: its `DeviceLayout`, kept to the module's rules
   (`Mix` and `Gain` are knobs 7 and 8), and its `BY_EFFECT` row, or one layout for each type of an
   effect whose settings change with its type. The compiler asks for the row.
   vt-tui's `tests/all/roto_layouts.rs` checks each setting is placed once.
7. **Only if it draws**, in `vt-tui/src/`: its picture in `graphics/slots.rs`, its panes on the FX
   page in `pages/track/draw.rs`, and the zoom's in `pages/track/draw/zoom.rs`. These matches have a
   catch-all, so nothing asks.
8. **The guide**: `vt-tui/guides/track-fx.md`, with its name and every setting's. `guides.rs`'s
   `every_effect_its_types_and_its_settings_are_in_the_fx_page_guide` fails until they're there.

**Pins:** `effects.txt` and `inserts.txt` gain a line, and `settings.txt` gains its settings in each
slot. One new Roto pin. A golden render of its own: its settings moved from their defaults in
`goldens.rs`'s `moved`, which renders it as `effect-<key>` and `effect-<key>-moved`, or a file of
its own named in `effect_golden`. The release tests.

## A voice

1. **`vt-model/src/machine.rs`**: the `SynthVoice` variant, its `ALL` entry, its place in one
   `KINDS` group, and its `VOICES` row (key, name, meaning, params, holds, braids, type key, grid,
   role, gate). The compiler asks for the row, and `Machine::ALL` won't build unless each voice is
   in exactly one kind.
2. **`vt-model/src/settings/voices.rs`**: its list of `Entry`s, which its row names, at most
   `MAX_MACHINE_SETTINGS`. `machine.rs`'s tests check that its grid places each setting once.
3. **Its DSP**, in `vt-engine/src/dsp/voices/<name>.rs`, with a `pub mod` line in `dsp/voices.rs`.
   Implement `SynthEngine`: `new` and `next`, and `DRIFTS`, `NOISE`, `trigger`, `release` and the
   rest only where it has them. It reads its settings by place from `Settings`, in its entries'
   order.
4. **`vt-engine/src/voice.rs`**: one `synths!` line, `Voice(Engine)`, and its `use`. The compiler
   asks for it, as `Synth::new` matches every voice.
5. **The Roto**: its layout and `BY_VOICE` row in `roto/layouts.rs`. The compiler asks for the row.
6. **`glyphs::machine`** (`vt-tui/src/widgets/glyphs.rs`), only if it's a drum. The catch-all gives it the
   synth's icon.
7. **The guide**: `vt-tui/guides/sounds.md`, with its name and every setting's.
   `every_machine_and_its_settings_are_in_the_sounds_guide` fails until they're there.

**Pins:** `machines.txt` gains a line, and `settings.txt` its settings, with every place after them
moving. One new Roto pin. The engine's `synth.rs`, `tails.rs` and `glide.rs` play every voice.
`goldens.rs` renders a new one as `voice-<key>` with nothing more to write; one with a character to
keep can have a file of its own instead, as `supersaw.rs` and `four_op.rs` do, named in
`voice_golden`.

## A generator (a fill)

1. **`vt-model/src/shape.rs`**: the `FillKind` variant and `ALL` entry, and its `FILLS` row (name,
   meaning, reworks, on bits, has a `Where`, its grid). Its own settings go in a module of their
   own, `shape/<name>.rs`, as a `Shape` field, `#[serde(default, skip_serializing_if = ...)]`, with
   its default in `impl Default for Shape`. Add it to `Shape::draws` if it draws from the seed. The
   compiler asks for the row and the default.
2. **`vt-model/src/shape/params.rs`**: a `GenParam` for each new setting, its place in `ALL`, which
   is the panel's order, and its `GEN_PARAMS` row, in the enum's order, as `spec` finds a row by the
   variant. Then its cases in `exists_for`, `shown` and `reset`, which the compiler asks for, and in
   `grid`, `typed`, `choices` and `chosen` where it needs them, which it doesn't. A row in the wrong
   place shows as keys moving in `gen_params.txt`.
3. **`vt-model/src/shape/render.rs`**: its arm in the match on each row, or a render of its own
   before it, as `tonnetz::render` is, with an `unreachable!` arm. The compiler asks for the arm.
4. **The engine**: nothing, unless it carries something from one pass to the next, as `Ring` and
   `Acid`'s loops do (`vt-engine/src/gens.rs`). Each `GEN_PARAMS` row's `engine` says whether the
   engine can hold that setting.
5. **The Roto**: its `gen/` layout and `BY_FILL` row in `roto/layouts.rs`. The compiler asks for the
   row. vt-tui's `tests/all/roto_generators.rs` checks each setting is on one knob.
6. **Its hint** under the shape panel, in `vt-tui/src/overlays/shape/draw.rs`. The catch-all gives it
   `~ swap the ends`.
7. **The guides**: `vt-tui/guides/editing.md`, the shape panel's fills, and `track-gen.md`, the Gen
   page.

**Pins:** `fills.txt` and `gen_params.txt` gain lines. vt-model's `tests/render.rs` keeps what
every fill writes on every column in `renders.txt`: write its lines with
`VT_BLESS=1 cargo test -p vt-model --test render`, and every other line stays. One new Roto pin.

## A mod type

1. **`vt-model/src/modulation.rs`**: the `ModKind` variant and `ALL` entry, and its settings as a
   `ModSlot` field, `#[serde(default, skip_serializing_if = "is_default")]`. The compiler asks for
   the field in the slot's constructors, and for its arms in `mode_name` and `envelope`.
2. **`vt-model/src/modulation/fields.rs`**: its `MOD_SPECS` row (name, meaning, whether it's an
   envelope, its mode's label, polarity, title, fields and grid), any `ModField` it has that no
   other type has, and its arms in `bounds`, `field` and `set_field`. The compiler asks for the row
   and the `Mode` arms; the other fields have a catch-all. A setting macros and modulation move
   goes in `MOD_OWN_FIELDS` and the registry's `settings/mods.rs`.
3. **Its running**, in `vt-engine/src/track_state/mods.rs`. The compiler asks for its arms there.
4. **The Roto**: its layout and `BY_MOD` row in `roto/layouts.rs`. The compiler asks for the row.
5. **The Mod page**, in `vt-tui/src/pages/track/`: its fields' arms in `view.rs`'s `mod_view`, its
   lists in `pickers.rs`, and its picture in `pictures.rs`'s `mod_panels`, whose match the compiler
   asks for (`continue` for none).
6. **The guide**: `vt-tui/guides/track-mod.md`.

**Pins:** `mods.txt` gains a line, and `settings.txt` too if it adds to `MOD_OWN_FIELDS`.
vt-model's `tests/all/mod_fields.rs` lists each type's name, meaning and mode label, so it gains
the new type's. One new Roto pin. vt-tui's `tests/all/mod_page.rs` checks each setting is on the
page once and the keys reach it.

## Then

As for any PR: `just ci`, the release tests for an engine change, and a decision in
[decisions.md](decisions.md) for what the issue left open.
